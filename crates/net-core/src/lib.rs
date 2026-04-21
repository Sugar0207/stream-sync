use std::{
    io,
    net::{SocketAddr, UdpSocket},
};

use stream_sync_protocol::{
    decode_fixed_header, decode_payload_by_message_type, validate_protocol_version, ClientId,
    DecodeContext, EncodeContext, MessageEncoder, MessageType, ProtocolError, ProtocolMessage,
    RunId,
};

pub const CRATE_NAME: &str = "stream-sync-net-core";
pub const DEFAULT_UDP_PACKET_BUFFER_LEN: usize = 65_507;

/// Source address attached to a received packet.
///
/// This is a boundary value only. Opening sockets and receiving datagrams are
/// still outside this placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PacketSource {
    pub address: SocketAddr,
}

impl From<SocketAddr> for PacketSource {
    fn from(address: SocketAddr) -> Self {
        Self { address }
    }
}

/// Destination address for a packet that should be sent later.
///
/// This is metadata only. The actual UDP socket send is outside this
/// placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PacketDestination {
    pub address: SocketAddr,
}

impl From<SocketAddr> for PacketDestination {
    fn from(address: SocketAddr) -> Self {
        Self { address }
    }
}

impl From<PacketSource> for PacketDestination {
    fn from(source: PacketSource) -> Self {
        Self {
            address: source.address,
        }
    }
}

/// One received UDP datagram plus source metadata.
///
/// The byte slice borrows the caller-owned receive buffer. This keeps the
/// socket layer allocation-free while still letting the server receive loop
/// consume the packet immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpReceivedPacket<'a> {
    pub source: PacketSource,
    pub bytes: &'a [u8],
}

/// Minimal synchronous UDP socket I/O boundary.
///
/// This owns only one-datagram receive and send operations. It does not run an
/// async runtime, retry, fragmentation, encryption, queue processing, or log
/// output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct UdpSocketIoBoundary;

impl UdpSocketIoBoundary {
    pub fn bind(&self, address: SocketAddr) -> io::Result<UdpSocket> {
        UdpSocket::bind(address)
    }

    pub fn receive_one<'a>(
        &self,
        socket: &UdpSocket,
        buffer: &'a mut [u8],
    ) -> io::Result<UdpReceivedPacket<'a>> {
        let (len, source) = socket.recv_from(buffer)?;

        Ok(UdpReceivedPacket {
            source: source.into(),
            bytes: &buffer[..len],
        })
    }

    pub fn send_encoded(
        &self,
        socket: &UdpSocket,
        packet: &EncodedOutboundPacket,
    ) -> io::Result<usize> {
        socket.send_to(&packet.bytes, packet.destination.address)
    }
}

/// Raw packet bytes plus the source metadata collected by the future UDP layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InboundPacket<'a> {
    pub source: PacketSource,
    pub bytes: &'a [u8],
}

/// Result handed from net-core to app/server handlers after protocol decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedInboundPacket {
    pub source: PacketSource,
    pub message: ProtocolMessage,
}

/// Typed outbound message plus destination metadata for the future send layer.
///
/// This is intentionally pre-encode: it carries a `ProtocolMessage`, not wire
/// bytes, and does not imply any queue or socket behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundPacket {
    pub destination: PacketDestination,
    pub message: ProtocolMessage,
}

/// Single outbound queue handoff item.
///
/// The real queue implementation, async runtime, backpressure, encode, and UDP
/// socket send remain outside this placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundQueueItem {
    pub packet: OutboundPacket,
}

/// Queue-side state for a single outbound item.
///
/// This is not a real queue implementation. It only names the lifecycle states
/// that a future queue will use while handing items to the send layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundQueueItemState {
    Queued,
    ReadyForEncode,
    Encoded,
    Sent,
    Dropped,
}

/// One outbound item while it is owned by a future queue.
///
/// Holding a single item in this type documents ownership without implementing
/// buffering, ordering, wakeups, retry, or backpressure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedOutboundItem {
    pub item: OutboundQueueItem,
    pub state: OutboundQueueItemState,
}

/// Handoff from the outbound queue to the net send layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundQueueSendHandoff {
    pub item: OutboundQueueItem,
}

/// State names for one future packet send-loop tick.
///
/// This is not a continuous loop implementation. It only names the checkpoints
/// that connect queue dequeue, encoder handoff, socket send, and send logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundSendLoopTickState {
    ReadyForEncode,
    Encoded,
    SocketSendSucceeded,
    Failed,
}

/// Plan for one send-loop tick after queue storage selects an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundSendLoopTickPlan {
    pub state: OutboundSendLoopTickState,
    pub encode_request: OutboundEncodeRequest,
    pub log_context: OutboundSendLogContext,
}

/// Event produced by observing a send-loop checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundSendLoopEvent {
    pub state: OutboundSendLoopTickState,
    pub log_event: Option<SendLogEvent>,
}

/// Minimal boundary for one packet send-loop tick.
///
/// This boundary does not own queue storage, run a loop, encode bytes, call
/// sockets, retry, requeue, or write logs. It prepares the handoff into the
/// encoder and turns observed encode/socket results into structured log events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OutboundSendLoopTickBoundary {
    encoder: OutboundPacketEncoderBoundary,
}

impl OutboundSendLoopTickBoundary {
    pub fn plan_encode(
        &self,
        context: EncodeContext,
        handoff: OutboundQueueSendHandoff,
    ) -> OutboundSendLoopTickPlan {
        let log_context = OutboundSendLogContext::from_packet(&handoff.item.packet);
        let encode_request = self.encoder.prepare_encode(context, handoff.item);
        OutboundSendLoopTickPlan {
            state: OutboundSendLoopTickState::ReadyForEncode,
            encode_request,
            log_context,
        }
    }

    pub fn observe_encode_success(
        &self,
        plan: &OutboundSendLoopTickPlan,
        encoded: &EncodedOutboundPacket,
    ) -> OutboundSendLoopEvent {
        OutboundSendLoopEvent {
            state: OutboundSendLoopTickState::Encoded,
            log_event: Some(SendLogEvent::encode_succeeded(
                plan.log_context.clone(),
                encoded.bytes.len(),
            )),
        }
    }

    pub fn observe_encode_failure(&self, plan: &OutboundSendLoopTickPlan) -> OutboundSendLoopEvent {
        OutboundSendLoopEvent {
            state: OutboundSendLoopTickState::Failed,
            log_event: Some(SendLogEvent::send_failed(
                plan.log_context.clone(),
                SendLogStage::Encode,
                None,
                SendFailureKind::EncodeFailed,
            )),
        }
    }

    pub fn observe_socket_send_success(&self) -> OutboundSendLoopEvent {
        OutboundSendLoopEvent {
            state: OutboundSendLoopTickState::SocketSendSucceeded,
            log_event: None,
        }
    }

    pub fn observe_socket_send_failure(
        &self,
        plan: &OutboundSendLoopTickPlan,
        encoded_len: usize,
        failure: SendFailureKind,
    ) -> OutboundSendLoopEvent {
        OutboundSendLoopEvent {
            state: OutboundSendLoopTickState::Failed,
            log_event: Some(SendLogEvent::send_failed(
                plan.log_context.clone(),
                SendLogStage::SocketSend,
                Some(encoded_len),
                failure,
            )),
        }
    }
}

/// Lifecycle states for a future continuous outbound send loop.
///
/// This is not a loop implementation. It only defines the control checkpoints
/// around dequeue, one-item processing, retry deferral, and shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundSendLoopLifecycleState {
    Stopped,
    WaitingForQueueItem,
    ProcessingOneItem,
    RetryDeferred,
}

/// Result of asking future queue storage for the next ready item.
///
/// The actual queue collection and dequeue operation remain out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundSendLoopDequeueStatus {
    NoReadyItem,
    ReadyItem,
}

/// Next action a future send loop body may take.
///
/// These are planning values only. They do not block, sleep, spawn work, send
/// sockets, retry, or requeue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundSendLoopLifecycleAction {
    Stop,
    WaitForQueueItem,
    ProcessOneItem,
    DeferRetry,
}

/// Minimal input for planning one continuous send-loop lifecycle step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboundSendLoopLifecycleInput {
    pub stop_requested: bool,
    pub dequeue_status: OutboundSendLoopDequeueStatus,
}

/// Planned lifecycle state/action for the future continuous send loop body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboundSendLoopLifecyclePlan {
    pub state: OutboundSendLoopLifecycleState,
    pub action: OutboundSendLoopLifecycleAction,
}

/// Minimal lifecycle boundary for the future outbound send loop body.
///
/// This boundary decides whether the loop should stop, wait for a queue item,
/// process one selected item through the tick boundary, or defer retry policy.
/// It does not own queue storage, encode, socket send, retry execution, requeue,
/// scheduling, or logging.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OutboundSendLoopLifecycleBoundary;

impl OutboundSendLoopLifecycleBoundary {
    pub fn plan_next(
        &self,
        input: OutboundSendLoopLifecycleInput,
    ) -> OutboundSendLoopLifecyclePlan {
        if input.stop_requested {
            return OutboundSendLoopLifecyclePlan {
                state: OutboundSendLoopLifecycleState::Stopped,
                action: OutboundSendLoopLifecycleAction::Stop,
            };
        }

        match input.dequeue_status {
            OutboundSendLoopDequeueStatus::NoReadyItem => OutboundSendLoopLifecyclePlan {
                state: OutboundSendLoopLifecycleState::WaitingForQueueItem,
                action: OutboundSendLoopLifecycleAction::WaitForQueueItem,
            },
            OutboundSendLoopDequeueStatus::ReadyItem => OutboundSendLoopLifecyclePlan {
                state: OutboundSendLoopLifecycleState::ProcessingOneItem,
                action: OutboundSendLoopLifecycleAction::ProcessOneItem,
            },
        }
    }

    pub fn plan_after_send_failure(
        &self,
        disposition: SendFailureDisposition,
    ) -> OutboundSendLoopLifecyclePlan {
        match disposition {
            SendFailureDisposition::RetryCandidate => OutboundSendLoopLifecyclePlan {
                state: OutboundSendLoopLifecycleState::RetryDeferred,
                action: OutboundSendLoopLifecycleAction::DeferRetry,
            },
            SendFailureDisposition::DropCandidate | SendFailureDisposition::WarningCandidate => {
                OutboundSendLoopLifecyclePlan {
                    state: OutboundSendLoopLifecycleState::WaitingForQueueItem,
                    action: OutboundSendLoopLifecycleAction::WaitForQueueItem,
                }
            }
        }
    }
}

/// Current MVP queue sizing policy.
///
/// This is an admission-policy placeholder, not a real queue. The future queue
/// should stay bounded and return a decision immediately instead of blocking a
/// receive or handler path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboundQueueCapacityPolicy {
    pub max_items: usize,
    pub control_when_full: OutboundQueueBackpressureAction,
    pub video_when_full: OutboundQueueBackpressureAction,
    pub telemetry_when_full: OutboundQueueBackpressureAction,
}

impl Default for OutboundQueueCapacityPolicy {
    fn default() -> Self {
        Self {
            max_items: 64,
            control_when_full: OutboundQueueBackpressureAction::DropIncoming,
            video_when_full: OutboundQueueBackpressureAction::DropOldestThenAccept,
            telemetry_when_full: OutboundQueueBackpressureAction::DropIncoming,
        }
    }
}

/// Queue policy class derived from the outbound message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundQueueItemClass {
    Control,
    TimeSensitiveVideo,
    Telemetry,
}

/// Backpressure action selected when the future bounded queue is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundQueueBackpressureAction {
    Accept,
    DropIncoming,
    DropOldestThenAccept,
}

/// Reason attached to a queue admission rejection or replacement decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundQueueDropReason {
    CapacityReached,
}

/// Result of applying queue admission policy to one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutboundQueueAdmissionDecision {
    Accepted {
        item_class: OutboundQueueItemClass,
    },
    DropIncoming {
        item_class: OutboundQueueItemClass,
        reason: OutboundQueueDropReason,
    },
    DropOldestThenAccept {
        item_class: OutboundQueueItemClass,
        reason: OutboundQueueDropReason,
    },
}

/// Snapshot of future bounded outbound queue storage.
///
/// This is state metadata only. It does not own a collection, allocate storage,
/// preserve ordering, or wake a send loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboundQueueStorageState {
    pub len: usize,
    pub capacity: usize,
}

impl OutboundQueueStorageState {
    pub fn from_policy(policy: OutboundQueueCapacityPolicy, len: usize) -> Self {
        Self {
            len,
            capacity: policy.max_items,
        }
    }

    pub fn has_capacity(self) -> bool {
        self.len < self.capacity
    }

    pub fn is_full(self) -> bool {
        !self.has_capacity()
    }
}

/// Result of evaluating one candidate against future queue storage state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboundQueueStorageDecision {
    pub state_before: OutboundQueueStorageState,
    pub admission: OutboundQueueAdmissionDecision,
}

impl OutboundQueueStorageDecision {
    pub fn accepts_candidate(self) -> bool {
        matches!(
            self.admission,
            OutboundQueueAdmissionDecision::Accepted { .. }
                | OutboundQueueAdmissionDecision::DropOldestThenAccept { .. }
        )
    }

    pub fn planned_len_after(self) -> usize {
        match self.admission {
            OutboundQueueAdmissionDecision::Accepted { .. } => {
                self.state_before.len.saturating_add(1)
            }
            OutboundQueueAdmissionDecision::DropIncoming { .. } => self.state_before.len,
            OutboundQueueAdmissionDecision::DropOldestThenAccept { .. } => {
                self.state_before.capacity
            }
        }
    }
}

/// Minimal admission-policy boundary for the future outbound queue.
///
/// It only decides what should happen for a single candidate item at a given
/// queue length. It does not mutate a collection, choose a concrete item to
/// evict, encode bytes, send sockets, sleep, or retry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OutboundQueueAdmissionPolicyBoundary;

impl OutboundQueueAdmissionPolicyBoundary {
    pub fn evaluate(
        &self,
        policy: OutboundQueueCapacityPolicy,
        current_len: usize,
        item: &OutboundQueueItem,
    ) -> OutboundQueueAdmissionDecision {
        let item_class = outbound_queue_item_class(&item.packet.message);
        if current_len < policy.max_items {
            return OutboundQueueAdmissionDecision::Accepted { item_class };
        }

        let action = match item_class {
            OutboundQueueItemClass::Control => policy.control_when_full,
            OutboundQueueItemClass::TimeSensitiveVideo => policy.video_when_full,
            OutboundQueueItemClass::Telemetry => policy.telemetry_when_full,
        };

        match action {
            OutboundQueueBackpressureAction::Accept => {
                OutboundQueueAdmissionDecision::Accepted { item_class }
            }
            OutboundQueueBackpressureAction::DropIncoming => {
                OutboundQueueAdmissionDecision::DropIncoming {
                    item_class,
                    reason: OutboundQueueDropReason::CapacityReached,
                }
            }
            OutboundQueueBackpressureAction::DropOldestThenAccept => {
                OutboundQueueAdmissionDecision::DropOldestThenAccept {
                    item_class,
                    reason: OutboundQueueDropReason::CapacityReached,
                }
            }
        }
    }
}

/// Minimal queue storage planning boundary.
///
/// This binds bounded storage metadata to admission policy before a future send
/// loop exists. It does not push into a collection, evict items, dequeue,
/// encode, send sockets, or retry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OutboundQueueStorageBoundary {
    admission: OutboundQueueAdmissionPolicyBoundary,
}

impl OutboundQueueStorageBoundary {
    pub fn evaluate_push(
        &self,
        policy: OutboundQueueCapacityPolicy,
        current_len: usize,
        item: &OutboundQueueItem,
    ) -> OutboundQueueStorageDecision {
        let state_before = OutboundQueueStorageState::from_policy(policy, current_len);
        let admission = self.admission.evaluate(policy, current_len, item);
        OutboundQueueStorageDecision {
            state_before,
            admission,
        }
    }
}

/// Minimal boundary between app/server response code and a future send queue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OutboundPacketQueueBoundary;

impl OutboundPacketQueueBoundary {
    pub fn handoff(&self, packet: OutboundPacket) -> OutboundQueueItem {
        OutboundQueueItem { packet }
    }
}

/// Minimal queue lifecycle boundary.
///
/// This boundary models one-item state transitions only. It does not store a
/// collection, schedule work, run async tasks, execute retry, encode packets, or
/// call sockets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OutboundQueueLifecycleBoundary;

impl OutboundQueueLifecycleBoundary {
    pub fn hold_for_send(&self, item: OutboundQueueItem) -> QueuedOutboundItem {
        QueuedOutboundItem {
            item,
            state: OutboundQueueItemState::Queued,
        }
    }

    pub fn handoff_to_send_layer(&self, queued: QueuedOutboundItem) -> OutboundQueueSendHandoff {
        OutboundQueueSendHandoff { item: queued.item }
    }

    pub fn mark_encoded(&self, _packet: &EncodedOutboundPacket) -> OutboundQueueItemState {
        OutboundQueueItemState::Encoded
    }

    pub fn mark_send_completed(&self) -> OutboundQueueItemState {
        OutboundQueueItemState::Sent
    }

    pub fn mark_dropped(&self) -> OutboundQueueItemState {
        OutboundQueueItemState::Dropped
    }
}

/// Request passed from the net send layer into the protocol encoder boundary.
///
/// This keeps destination metadata alongside the typed message while the encoder
/// only receives protocol-specific inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundEncodeRequest {
    pub destination: PacketDestination,
    pub context: EncodeContext,
    pub message: ProtocolMessage,
}

/// Encoded outbound packet ready for a future socket send layer.
///
/// This type is a boundary shape only. No real encoder or UDP send is
/// implemented in net-core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedOutboundPacket {
    pub destination: PacketDestination,
    pub bytes: Vec<u8>,
}

/// Log context that should follow an outbound message through encode and send.
///
/// This is structured for future JSON Lines output. It intentionally carries
/// metadata only and does not perform logging by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundSendLogContext {
    pub destination: PacketDestination,
    pub message_type: MessageType,
    pub run_id: Option<RunId>,
    pub client_id: Option<ClientId>,
}

impl OutboundSendLogContext {
    pub fn from_packet(packet: &OutboundPacket) -> Self {
        let (run_id, client_id) = outbound_message_ids(&packet.message);

        Self {
            destination: packet.destination,
            message_type: packet.message.message_type(),
            run_id,
            client_id,
        }
    }
}

/// Send path stage used by future send log events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SendLogStage {
    Encode,
    BeforeSocketSend,
    SocketSend,
}

/// Minimal outbound send failure categories.
///
/// These categories are policy hints only. They do not execute retry, queue
/// mutation, socket writes, or logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SendFailureKind {
    EncodeFailed,
    DestinationUnavailable,
    PacketTooLarge,
    SocketWouldBlock,
    SocketInterrupted,
    ConnectionRefused,
    NetworkUnreachable,
    PermissionDenied,
    OtherSocketError,
}

/// Initial action hint for send failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SendFailureDisposition {
    RetryCandidate,
    DropCandidate,
    WarningCandidate,
}

impl SendFailureKind {
    pub const fn disposition(self) -> SendFailureDisposition {
        match self {
            Self::SocketWouldBlock | Self::SocketInterrupted => {
                SendFailureDisposition::RetryCandidate
            }
            Self::EncodeFailed | Self::DestinationUnavailable | Self::PacketTooLarge => {
                SendFailureDisposition::DropCandidate
            }
            Self::ConnectionRefused
            | Self::NetworkUnreachable
            | Self::PermissionDenied
            | Self::OtherSocketError => SendFailureDisposition::WarningCandidate,
        }
    }
}

/// Structured send event placeholder for future JSON Lines logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendLogEvent {
    pub context: OutboundSendLogContext,
    pub stage: SendLogStage,
    pub encoded_len: Option<usize>,
    pub failure: Option<SendFailureKind>,
    pub disposition: Option<SendFailureDisposition>,
}

impl SendLogEvent {
    pub fn encode_succeeded(context: OutboundSendLogContext, encoded_len: usize) -> Self {
        Self {
            context,
            stage: SendLogStage::Encode,
            encoded_len: Some(encoded_len),
            failure: None,
            disposition: None,
        }
    }

    pub fn send_failed(
        context: OutboundSendLogContext,
        stage: SendLogStage,
        encoded_len: Option<usize>,
        failure: SendFailureKind,
    ) -> Self {
        Self {
            context,
            stage,
            encoded_len,
            failure: Some(failure),
            disposition: Some(failure.disposition()),
        }
    }
}

/// Error returned while bridging outbound typed messages into encoded packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEncodeError {
    Protocol {
        destination: PacketDestination,
        error: ProtocolError,
    },
}

/// Minimal net send layer to protocol encoder boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OutboundPacketEncoderBoundary;

impl OutboundPacketEncoderBoundary {
    pub fn prepare_encode(
        &self,
        context: EncodeContext,
        item: OutboundQueueItem,
    ) -> OutboundEncodeRequest {
        OutboundEncodeRequest {
            destination: item.packet.destination,
            context,
            message: item.packet.message,
        }
    }

    pub fn encode_with<E: MessageEncoder>(
        &self,
        encoder: &E,
        request: OutboundEncodeRequest,
    ) -> Result<EncodedOutboundPacket, NetEncodeError> {
        let mut bytes = Vec::new();
        encoder
            .encode_message(request.context, &request.message, &mut bytes)
            .map_err(|error| NetEncodeError::Protocol {
                destination: request.destination,
                error,
            })?;

        Ok(EncodedOutboundPacket {
            destination: request.destination,
            bytes,
        })
    }
}

fn outbound_message_ids(message: &ProtocolMessage) -> (Option<RunId>, Option<ClientId>) {
    match message {
        ProtocolMessage::AuthRequest(message) => (
            Some(message.run_id.clone()),
            Some(message.client_id.clone()),
        ),
        ProtocolMessage::AuthResponse(message) => (
            Some(message.run_id.clone()),
            Some(message.client_id.clone()),
        ),
        ProtocolMessage::Heartbeat(message) => (
            Some(message.run_id.clone()),
            Some(message.client_id.clone()),
        ),
        ProtocolMessage::HeartbeatAck(message) => (
            Some(message.run_id.clone()),
            Some(message.client_id.clone()),
        ),
        ProtocolMessage::VideoFrame(message) => (
            Some(message.run_id.clone()),
            Some(message.client_id.clone()),
        ),
        ProtocolMessage::ClientStats(message) => (
            Some(message.run_id.clone()),
            Some(message.client_id.clone()),
        ),
        ProtocolMessage::ServerNotice(message) => (Some(message.run_id.clone()), None),
    }
}

fn outbound_queue_item_class(message: &ProtocolMessage) -> OutboundQueueItemClass {
    match message {
        ProtocolMessage::VideoFrame(_) => OutboundQueueItemClass::TimeSensitiveVideo,
        ProtocolMessage::ClientStats(_) => OutboundQueueItemClass::Telemetry,
        ProtocolMessage::AuthRequest(_)
        | ProtocolMessage::AuthResponse(_)
        | ProtocolMessage::Heartbeat(_)
        | ProtocolMessage::HeartbeatAck(_)
        | ProtocolMessage::ServerNotice(_) => OutboundQueueItemClass::Control,
    }
}

/// Error returned while bridging raw packet bytes into protocol messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetDecodeError {
    Protocol {
        source: PacketSource,
        error: ProtocolError,
    },
}

/// Minimal receive-side decode boundary.
///
/// This type does not receive from UDP sockets and does not call app handlers.
/// It only preserves packet source metadata and calls protocol crate decode
/// entry points in the agreed order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct InboundPacketDecoder;

impl InboundPacketDecoder {
    pub fn decode(
        &self,
        context: DecodeContext,
        packet: InboundPacket<'_>,
    ) -> Result<DecodedInboundPacket, NetDecodeError> {
        let packet_view = decode_fixed_header(packet.bytes)
            .map_err(|error| protocol_error(packet.source, error))?;
        validate_protocol_version(context, packet_view.header)
            .map_err(|error| protocol_error(packet.source, error))?;
        let message =
            decode_payload_by_message_type(context, packet_view.header, packet_view.payload)
                .map_err(|error| protocol_error(packet.source, error))?;

        Ok(DecodedInboundPacket {
            source: packet.source,
            message,
        })
    }
}

fn protocol_error(source: PacketSource, error: ProtocolError) -> NetDecodeError {
    NetDecodeError::Protocol { source, error }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stream_sync_protocol::{
        encode_auth_response_payload, AuthResponse, AuthResponseReasonCode, ClientId, Codec,
        HeartbeatAck, MessageType, ProtocolVersion, RunId, TimestampMicros, FIXED_HEADER_LEN,
        HEADER_FLAGS_OFFSET, HEADER_LENGTH_OFFSET, HEADER_MESSAGE_TYPE_OFFSET,
        HEADER_PAYLOAD_LENGTH_OFFSET, HEADER_PROTOCOL_VERSION_OFFSET, HEADER_RESERVED_OFFSET,
    };

    #[test]
    fn decodes_received_packet_into_protocol_message() {
        let source = packet_source();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let decoder = InboundPacketDecoder;

        let decoded = decoder
            .decode(
                DecodeContext {
                    expected_protocol_version: ProtocolVersion(2),
                },
                InboundPacket {
                    source,
                    bytes: &packet,
                },
            )
            .expect("packet should decode");

        assert_eq!(decoded.source, source);
        let ProtocolMessage::Heartbeat(heartbeat) = decoded.message else {
            panic!("expected heartbeat message");
        };
        assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
        assert_eq!(heartbeat.sent_at, TimestampMicros(1_234_567));
    }

    #[test]
    fn rejects_protocol_version_before_payload_decode() {
        let source = packet_source();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 1, &payload);
        let decoder = InboundPacketDecoder;

        let decoded = decoder.decode(
            DecodeContext {
                expected_protocol_version: ProtocolVersion(2),
            },
            InboundPacket {
                source,
                bytes: &packet,
            },
        );

        assert_eq!(
            decoded,
            Err(NetDecodeError::Protocol {
                source,
                error: ProtocolError::UnsupportedProtocolVersion {
                    expected: ProtocolVersion(2),
                    actual: ProtocolVersion(1)
                }
            })
        );
    }

    #[test]
    fn decodes_auth_response_packet() {
        let source = packet_source();
        let response = AuthResponse {
            message_type: MessageType::AuthResponse,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            accepted: true,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time: Some(TimestampMicros(2_000_000)),
            expected_protocol_version: None,
        };
        let mut payload = Vec::new();
        encode_auth_response_payload(&response, &mut payload)
            .expect("auth response payload should encode");
        let packet = test_packet(
            MessageType::AuthResponse as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoder = InboundPacketDecoder;

        let decoded = decoder.decode(
            DecodeContext {
                expected_protocol_version: ProtocolVersion(2),
            },
            InboundPacket {
                source,
                bytes: &packet,
            },
        );

        let decoded = decoded.expect("auth response should decode");
        assert_eq!(decoded.source, source);
        assert_eq!(decoded.message, ProtocolMessage::AuthResponse(response));
    }

    #[test]
    fn prepares_outbound_packet_for_queue_handoff() {
        let destination: PacketDestination = packet_source().into();
        let message = heartbeat_ack_message();
        let boundary = OutboundPacketQueueBoundary;

        let item = boundary.handoff(OutboundPacket {
            destination,
            message: message.clone(),
        });

        assert_eq!(item.packet.destination, destination);
        assert_eq!(item.packet.message, message);
    }

    #[test]
    fn queue_lifecycle_holds_item_and_hands_off_to_send_layer() {
        let destination: PacketDestination = packet_source().into();
        let message = heartbeat_ack_message();
        let queue_boundary = OutboundPacketQueueBoundary;
        let lifecycle = OutboundQueueLifecycleBoundary;
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: message.clone(),
        });

        let queued = lifecycle.hold_for_send(item);

        assert_eq!(queued.state, OutboundQueueItemState::Queued);
        assert_eq!(queued.item.packet.destination, destination);
        assert_eq!(queued.item.packet.message, message);

        let handoff = lifecycle.handoff_to_send_layer(queued);

        assert_eq!(handoff.item.packet.destination, destination);
    }

    #[test]
    fn queue_lifecycle_marks_encode_and_send_terminal_states() {
        let destination: PacketDestination = packet_source().into();
        let lifecycle = OutboundQueueLifecycleBoundary;
        let encoded = EncodedOutboundPacket {
            destination,
            bytes: vec![0xaa, 0xbb],
        };

        assert_eq!(
            lifecycle.mark_encoded(&encoded),
            OutboundQueueItemState::Encoded
        );
        assert_eq!(
            lifecycle.mark_send_completed(),
            OutboundQueueItemState::Sent
        );
        assert_eq!(lifecycle.mark_dropped(), OutboundQueueItemState::Dropped);
    }

    #[test]
    fn outbound_queue_admission_accepts_when_capacity_remains() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let admission = OutboundQueueAdmissionPolicyBoundary;
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        });

        let decision = admission.evaluate(OutboundQueueCapacityPolicy::default(), 0, &item);

        assert_eq!(
            decision,
            OutboundQueueAdmissionDecision::Accepted {
                item_class: OutboundQueueItemClass::Control
            }
        );
    }

    #[test]
    fn outbound_queue_admission_drops_incoming_control_when_full() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let admission = OutboundQueueAdmissionPolicyBoundary;
        let policy = OutboundQueueCapacityPolicy::default();
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        });

        let decision = admission.evaluate(policy, policy.max_items, &item);

        assert_eq!(
            decision,
            OutboundQueueAdmissionDecision::DropIncoming {
                item_class: OutboundQueueItemClass::Control,
                reason: OutboundQueueDropReason::CapacityReached
            }
        );
    }

    #[test]
    fn outbound_queue_admission_replaces_oldest_video_when_full() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let admission = OutboundQueueAdmissionPolicyBoundary;
        let policy = OutboundQueueCapacityPolicy::default();
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: video_frame_message(),
        });

        let decision = admission.evaluate(policy, policy.max_items, &item);

        assert_eq!(
            decision,
            OutboundQueueAdmissionDecision::DropOldestThenAccept {
                item_class: OutboundQueueItemClass::TimeSensitiveVideo,
                reason: OutboundQueueDropReason::CapacityReached
            }
        );
    }

    #[test]
    fn outbound_queue_storage_state_tracks_capacity_without_collection() {
        let state =
            OutboundQueueStorageState::from_policy(OutboundQueueCapacityPolicy::default(), 7);

        assert_eq!(state.len, 7);
        assert_eq!(state.capacity, 64);
        assert!(state.has_capacity());
        assert!(!state.is_full());
    }

    #[test]
    fn outbound_queue_storage_boundary_plans_push_before_send_loop() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let storage = OutboundQueueStorageBoundary::default();
        let policy = OutboundQueueCapacityPolicy::default();
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        });

        let decision = storage.evaluate_push(policy, 0, &item);

        assert_eq!(
            decision.state_before,
            OutboundQueueStorageState {
                len: 0,
                capacity: policy.max_items,
            }
        );
        assert_eq!(
            decision.admission,
            OutboundQueueAdmissionDecision::Accepted {
                item_class: OutboundQueueItemClass::Control
            }
        );
        assert!(decision.accepts_candidate());
        assert_eq!(decision.planned_len_after(), 1);
    }

    #[test]
    fn outbound_queue_storage_boundary_preserves_full_queue_drop_policy() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let storage = OutboundQueueStorageBoundary::default();
        let policy = OutboundQueueCapacityPolicy::default();
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        });

        let decision = storage.evaluate_push(policy, policy.max_items, &item);

        assert!(decision.state_before.is_full());
        assert_eq!(
            decision.admission,
            OutboundQueueAdmissionDecision::DropIncoming {
                item_class: OutboundQueueItemClass::Control,
                reason: OutboundQueueDropReason::CapacityReached,
            }
        );
        assert!(!decision.accepts_candidate());
        assert_eq!(decision.planned_len_after(), policy.max_items);
    }

    #[test]
    fn send_loop_tick_boundary_plans_encoder_handoff_from_queue_item() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let lifecycle = OutboundQueueLifecycleBoundary;
        let send_loop = OutboundSendLoopTickBoundary::default();
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        });
        let queued = lifecycle.hold_for_send(item);
        let handoff = lifecycle.handoff_to_send_layer(queued);

        let plan = send_loop.plan_encode(
            EncodeContext {
                protocol_version: ProtocolVersion(2),
            },
            handoff,
        );

        assert_eq!(plan.state, OutboundSendLoopTickState::ReadyForEncode);
        assert_eq!(plan.encode_request.destination, destination);
        assert_eq!(
            plan.encode_request.context.protocol_version,
            ProtocolVersion(2)
        );
        assert_eq!(plan.log_context.destination, destination);
        assert_eq!(plan.log_context.message_type, MessageType::HeartbeatAck);
        assert_eq!(plan.log_context.run_id, Some(RunId("run-1".to_string())));
    }

    #[test]
    fn send_loop_tick_boundary_observes_encode_success_as_log_event() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let lifecycle = OutboundQueueLifecycleBoundary;
        let send_loop = OutboundSendLoopTickBoundary::default();
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        });
        let plan = send_loop.plan_encode(
            EncodeContext {
                protocol_version: ProtocolVersion(2),
            },
            lifecycle.handoff_to_send_layer(lifecycle.hold_for_send(item)),
        );
        let encoded = EncodedOutboundPacket {
            destination,
            bytes: vec![0xaa, 0xbb, 0xcc],
        };

        let event = send_loop.observe_encode_success(&plan, &encoded);

        assert_eq!(event.state, OutboundSendLoopTickState::Encoded);
        assert_eq!(
            event.log_event,
            Some(SendLogEvent::encode_succeeded(plan.log_context, 3))
        );
    }

    #[test]
    fn send_loop_tick_boundary_observes_socket_send_failure_as_log_event() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let lifecycle = OutboundQueueLifecycleBoundary;
        let send_loop = OutboundSendLoopTickBoundary::default();
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        });
        let plan = send_loop.plan_encode(
            EncodeContext {
                protocol_version: ProtocolVersion(2),
            },
            lifecycle.handoff_to_send_layer(lifecycle.hold_for_send(item)),
        );

        let event =
            send_loop.observe_socket_send_failure(&plan, 42, SendFailureKind::SocketWouldBlock);

        assert_eq!(event.state, OutboundSendLoopTickState::Failed);
        assert_eq!(
            event.log_event,
            Some(SendLogEvent::send_failed(
                plan.log_context,
                SendLogStage::SocketSend,
                Some(42),
                SendFailureKind::SocketWouldBlock,
            ))
        );
    }

    #[test]
    fn send_loop_lifecycle_waits_when_queue_has_no_ready_item() {
        let lifecycle = OutboundSendLoopLifecycleBoundary;

        let plan = lifecycle.plan_next(OutboundSendLoopLifecycleInput {
            stop_requested: false,
            dequeue_status: OutboundSendLoopDequeueStatus::NoReadyItem,
        });

        assert_eq!(
            plan,
            OutboundSendLoopLifecyclePlan {
                state: OutboundSendLoopLifecycleState::WaitingForQueueItem,
                action: OutboundSendLoopLifecycleAction::WaitForQueueItem,
            }
        );
    }

    #[test]
    fn send_loop_lifecycle_processes_one_ready_item() {
        let lifecycle = OutboundSendLoopLifecycleBoundary;

        let plan = lifecycle.plan_next(OutboundSendLoopLifecycleInput {
            stop_requested: false,
            dequeue_status: OutboundSendLoopDequeueStatus::ReadyItem,
        });

        assert_eq!(
            plan,
            OutboundSendLoopLifecyclePlan {
                state: OutboundSendLoopLifecycleState::ProcessingOneItem,
                action: OutboundSendLoopLifecycleAction::ProcessOneItem,
            }
        );
    }

    #[test]
    fn send_loop_lifecycle_stops_when_requested() {
        let lifecycle = OutboundSendLoopLifecycleBoundary;

        let plan = lifecycle.plan_next(OutboundSendLoopLifecycleInput {
            stop_requested: true,
            dequeue_status: OutboundSendLoopDequeueStatus::ReadyItem,
        });

        assert_eq!(
            plan,
            OutboundSendLoopLifecyclePlan {
                state: OutboundSendLoopLifecycleState::Stopped,
                action: OutboundSendLoopLifecycleAction::Stop,
            }
        );
    }

    #[test]
    fn send_loop_lifecycle_defers_retry_without_requeue() {
        let lifecycle = OutboundSendLoopLifecycleBoundary;

        let plan =
            lifecycle.plan_after_send_failure(SendFailureKind::SocketWouldBlock.disposition());

        assert_eq!(
            plan,
            OutboundSendLoopLifecyclePlan {
                state: OutboundSendLoopLifecycleState::RetryDeferred,
                action: OutboundSendLoopLifecycleAction::DeferRetry,
            }
        );
    }

    #[test]
    fn extracts_outbound_send_log_context_from_typed_message() {
        let destination: PacketDestination = packet_source().into();
        let packet = OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        };

        let context = OutboundSendLogContext::from_packet(&packet);

        assert_eq!(context.destination, destination);
        assert_eq!(context.message_type, MessageType::HeartbeatAck);
        assert_eq!(context.run_id, Some(RunId("run-1".to_string())));
        assert_eq!(context.client_id, Some(ClientId("client-1".to_string())));
    }

    #[test]
    fn classifies_send_failures_for_future_policy() {
        assert_eq!(
            SendFailureKind::SocketWouldBlock.disposition(),
            SendFailureDisposition::RetryCandidate
        );
        assert_eq!(
            SendFailureKind::EncodeFailed.disposition(),
            SendFailureDisposition::DropCandidate
        );
        assert_eq!(
            SendFailureKind::NetworkUnreachable.disposition(),
            SendFailureDisposition::WarningCandidate
        );
    }

    #[test]
    fn builds_send_log_event_with_context_and_disposition() {
        let destination: PacketDestination = packet_source().into();
        let packet = OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        };
        let context = OutboundSendLogContext::from_packet(&packet);

        let event = SendLogEvent::send_failed(
            context.clone(),
            SendLogStage::SocketSend,
            Some(32),
            SendFailureKind::ConnectionRefused,
        );

        assert_eq!(event.context, context);
        assert_eq!(event.stage, SendLogStage::SocketSend);
        assert_eq!(event.encoded_len, Some(32));
        assert_eq!(event.failure, Some(SendFailureKind::ConnectionRefused));
        assert_eq!(
            event.disposition,
            Some(SendFailureDisposition::WarningCandidate)
        );
    }

    #[test]
    fn udp_socket_io_receives_one_packet_with_source() {
        let io = UdpSocketIoBoundary;
        let receiver = io
            .bind("127.0.0.1:0".parse().expect("address should parse"))
            .expect("receiver should bind");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        let receiver_addr = receiver.local_addr().expect("receiver should have address");
        let sender_addr = sender.local_addr().expect("sender should have address");

        sender
            .send_to(&[0xaa, 0xbb, 0xcc], receiver_addr)
            .expect("packet should send");

        let mut buffer = [0_u8; 16];
        let packet = io
            .receive_one(&receiver, &mut buffer)
            .expect("packet should receive");

        assert_eq!(packet.source.address, sender_addr);
        assert_eq!(packet.bytes, &[0xaa, 0xbb, 0xcc]);
    }

    #[test]
    fn udp_socket_io_sends_encoded_packet_to_destination() {
        let io = UdpSocketIoBoundary;
        let sender = io
            .bind("127.0.0.1:0".parse().expect("address should parse"))
            .expect("sender should bind");
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let receiver_addr = receiver.local_addr().expect("receiver should have address");
        let packet = EncodedOutboundPacket {
            destination: PacketDestination {
                address: receiver_addr,
            },
            bytes: vec![0x11, 0x22, 0x33],
        };

        let sent = io
            .send_encoded(&sender, &packet)
            .expect("packet should send");

        let mut buffer = [0_u8; 16];
        let (received_len, source) = receiver
            .recv_from(&mut buffer)
            .expect("packet should receive");
        assert_eq!(sent, packet.bytes.len());
        assert_eq!(
            source,
            sender.local_addr().expect("sender should have address")
        );
        assert_eq!(&buffer[..received_len], packet.bytes.as_slice());
    }

    #[test]
    fn prepares_outbound_encode_request_from_queue_item() {
        let destination: PacketDestination = packet_source().into();
        let message = heartbeat_ack_message();
        let queue_boundary = OutboundPacketQueueBoundary;
        let encoder_boundary = OutboundPacketEncoderBoundary;
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: message.clone(),
        });

        let request = encoder_boundary.prepare_encode(
            EncodeContext {
                protocol_version: ProtocolVersion(2),
            },
            item,
        );

        assert_eq!(request.destination, destination);
        assert_eq!(request.context.protocol_version, ProtocolVersion(2));
        assert_eq!(request.message, message);
    }

    #[test]
    fn maps_protocol_encoder_error_with_destination() {
        let destination: PacketDestination = packet_source().into();
        let queue_boundary = OutboundPacketQueueBoundary;
        let encoder_boundary = OutboundPacketEncoderBoundary;
        let item = queue_boundary.handoff(OutboundPacket {
            destination,
            message: heartbeat_ack_message(),
        });
        let request = encoder_boundary.prepare_encode(
            EncodeContext {
                protocol_version: ProtocolVersion(2),
            },
            item,
        );

        let encoded = encoder_boundary.encode_with(&RejectingEncoder, request);

        assert_eq!(
            encoded,
            Err(NetEncodeError::Protocol {
                destination,
                error: ProtocolError::EncodeNotImplemented(MessageType::HeartbeatAck)
            })
        );
    }

    fn packet_source() -> PacketSource {
        "127.0.0.1:5000"
            .parse::<SocketAddr>()
            .expect("source address should parse")
            .into()
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct RejectingEncoder;

    impl MessageEncoder for RejectingEncoder {
        fn encode_message(
            &self,
            _context: EncodeContext,
            message: &ProtocolMessage,
            _output: &mut Vec<u8>,
        ) -> Result<(), ProtocolError> {
            Err(ProtocolError::EncodeNotImplemented(message.message_type()))
        }
    }

    fn heartbeat_ack_message() -> ProtocolMessage {
        ProtocolMessage::HeartbeatAck(HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000_000),
            server_received_at: TimestampMicros(1_000_100),
            server_sent_at: TimestampMicros(1_000_200),
        })
    }

    fn video_frame_message() -> ProtocolMessage {
        ProtocolMessage::VideoFrame(stream_sync_protocol::VideoFrame {
            message_type: MessageType::VideoFrame,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            frame_id: 1,
            capture_timestamp: TimestampMicros(1_000_000),
            send_timestamp: TimestampMicros(1_000_050),
            is_keyframe: false,
            metadata_reserved: [0; 3],
            width: 1280,
            height: 720,
            fps_nominal: 30,
            codec: Codec::H264,
            payload_size: 3,
            payload: vec![0xaa, 0xbb, 0xcc],
        })
    }

    fn heartbeat_payload() -> Vec<u8> {
        let mut payload = Vec::new();
        push_string(&mut payload, "client-1");
        push_string(&mut payload, "run-1");
        push_u64(&mut payload, 1_234_567);
        payload.push(0);
        payload.push(0);
        payload
    }

    fn push_string(output: &mut Vec<u8>, value: &str) {
        output.extend_from_slice(&(value.len() as u16).to_le_bytes());
        output.extend_from_slice(value.as_bytes());
    }

    fn push_u64(output: &mut Vec<u8>, value: u64) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn test_packet(
        message_type: u16,
        header_length: u16,
        protocol_version: u32,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut packet = vec![0; usize::from(FIXED_HEADER_LEN)];
        packet[HEADER_MESSAGE_TYPE_OFFSET..HEADER_MESSAGE_TYPE_OFFSET + 2]
            .copy_from_slice(&message_type.to_le_bytes());
        packet[HEADER_LENGTH_OFFSET..HEADER_LENGTH_OFFSET + 2]
            .copy_from_slice(&header_length.to_le_bytes());
        packet[HEADER_PROTOCOL_VERSION_OFFSET..HEADER_PROTOCOL_VERSION_OFFSET + 4]
            .copy_from_slice(&protocol_version.to_le_bytes());
        packet[HEADER_PAYLOAD_LENGTH_OFFSET..HEADER_PAYLOAD_LENGTH_OFFSET + 4]
            .copy_from_slice(&(payload.len() as u32).to_le_bytes());
        packet[HEADER_FLAGS_OFFSET..HEADER_FLAGS_OFFSET + 2].copy_from_slice(&0_u16.to_le_bytes());
        packet[HEADER_RESERVED_OFFSET..HEADER_RESERVED_OFFSET + 2]
            .copy_from_slice(&0_u16.to_le_bytes());
        packet.extend_from_slice(payload);
        packet
    }
}
