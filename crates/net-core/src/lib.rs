use std::net::SocketAddr;

use stream_sync_protocol::{
    decode_fixed_header, decode_payload_by_message_type, validate_protocol_version, ClientId,
    DecodeContext, EncodeContext, MessageEncoder, MessageType, ProtocolError, ProtocolMessage,
    RunId,
};

pub const CRATE_NAME: &str = "stream-sync-net-core";

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
        ClientId, HeartbeatAck, MessageType, ProtocolVersion, RunId, TimestampMicros,
        FIXED_HEADER_LEN, HEADER_FLAGS_OFFSET, HEADER_LENGTH_OFFSET, HEADER_MESSAGE_TYPE_OFFSET,
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
    fn returns_not_implemented_for_undecoded_payload() {
        let source = packet_source();
        let packet = test_packet(MessageType::AuthResponse as u16, FIXED_HEADER_LEN, 2, &[]);
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
                error: ProtocolError::PayloadDecodeNotImplemented(MessageType::AuthResponse)
            })
        );
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
