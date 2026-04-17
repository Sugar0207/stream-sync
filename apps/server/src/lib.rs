use stream_sync_net_core::{
    DecodedInboundPacket, InboundPacket, InboundPacketDecoder, NetDecodeError, OutboundPacket,
    OutboundPacketQueueBoundary, OutboundQueueItem, PacketSource,
};
use stream_sync_protocol::{
    AppVersion, AuthRequest, AuthResponse, AuthResponseReasonCode, ClientId, Heartbeat,
    HeartbeatAck, MessageType, ProtocolError, ProtocolMessage, ProtocolVersion, RunId,
    TimestampMicros, VideoFrame,
};

/// One-packet receive loop boundary for the future UDP server.
///
/// The real UDP socket loop will receive bytes and source metadata, then call
/// this step. This placeholder does not open sockets, block on I/O, spawn async
/// tasks, or run auth/heartbeat/video handlers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveLoopStep {
    decoder: InboundPacketDecoder,
    router: ServerInboundRouter,
}

impl ServerReceiveLoopStep {
    pub fn handle_received_packet(
        &self,
        expected_protocol_version: ProtocolVersion,
        source: PacketSource,
        packet_bytes: &[u8],
    ) -> ServerReceiveLoopOutcome {
        let context = stream_sync_protocol::DecodeContext {
            expected_protocol_version,
        };
        let packet = InboundPacket {
            source,
            bytes: packet_bytes,
        };

        match self.decoder.decode(context, packet) {
            Ok(decoded) => ServerReceiveLoopOutcome::Routed(self.router.route(decoded)),
            Err(NetDecodeError::Protocol { source, error }) => {
                ServerReceiveLoopOutcome::Rejected(ServerRejectedPacket {
                    source,
                    action: classify_protocol_error(&error),
                    error,
                })
            }
        }
    }
}

/// Result of processing one already-received packet through the server boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveLoopOutcome {
    Routed(ServerInboundRoute),
    Rejected(ServerRejectedPacket),
}

/// Decode failure plus the server receive loop policy for that failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRejectedPacket {
    pub source: PacketSource,
    pub action: ServerDecodeErrorAction,
    pub error: ProtocolError,
}

/// Minimal policy for protocol errors observed by the receive loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerDecodeErrorAction {
    DropPacket,
    RejectProtocolVersion,
    UnsupportedInboundMessage,
}

fn classify_protocol_error(error: &ProtocolError) -> ServerDecodeErrorAction {
    match error {
        ProtocolError::UnsupportedProtocolVersion { .. } => {
            ServerDecodeErrorAction::RejectProtocolVersion
        }
        ProtocolError::PayloadDecodeNotImplemented(_) => {
            ServerDecodeErrorAction::UnsupportedInboundMessage
        }
        _ => ServerDecodeErrorAction::DropPacket,
    }
}

/// Routes decoded inbound packets to the server-side responsibility boundary.
///
/// This does not authenticate clients, update heartbeat state, or store video
/// frames. It only classifies decoded messages so future server handlers can
/// own the actual application behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerInboundRouter;

impl ServerInboundRouter {
    pub fn route(&self, packet: DecodedInboundPacket) -> ServerInboundRoute {
        match packet.message {
            ProtocolMessage::AuthRequest(request) => ServerInboundRoute::AuthRequest {
                source: packet.source,
                request,
            },
            ProtocolMessage::Heartbeat(heartbeat) => ServerInboundRoute::Heartbeat {
                source: packet.source,
                heartbeat,
            },
            ProtocolMessage::VideoFrame(frame) => ServerInboundRoute::VideoFrame {
                source: packet.source,
                frame,
            },
            message => ServerInboundRoute::UnsupportedForServer {
                source: packet.source,
                message_type: message_type(&message),
                message,
            },
        }
    }
}

/// Server-side handler boundary after net-core has decoded the packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerInboundRoute {
    AuthRequest {
        source: PacketSource,
        request: AuthRequest,
    },
    Heartbeat {
        source: PacketSource,
        heartbeat: Heartbeat,
    },
    VideoFrame {
        source: PacketSource,
        frame: VideoFrame,
    },
    UnsupportedForServer {
        source: PacketSource,
        message_type: MessageType,
        message: ProtocolMessage,
    },
}

/// Boundary that prepares decoded auth requests for server auth checks.
///
/// This placeholder does not validate tokens, read a whitelist, update server
/// state, or generate `AuthResponse`. It only marks the handoff from routing to
/// the future auth decision logic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthHandlerBoundary;

impl ServerAuthHandlerBoundary {
    pub fn prepare_from_route(
        &self,
        route: ServerInboundRoute,
    ) -> Result<ServerAuthCheck, ServerAuthBoundaryError> {
        match route {
            ServerInboundRoute::AuthRequest { source, request } => {
                Ok(self.prepare_auth_check(source, request))
            }
            ServerInboundRoute::Heartbeat { .. } => Err(ServerAuthBoundaryError::UnexpectedRoute {
                message_type: MessageType::Heartbeat,
            }),
            ServerInboundRoute::VideoFrame { .. } => {
                Err(ServerAuthBoundaryError::UnexpectedRoute {
                    message_type: MessageType::VideoFrame,
                })
            }
            ServerInboundRoute::UnsupportedForServer { message_type, .. } => {
                Err(ServerAuthBoundaryError::UnexpectedRoute { message_type })
            }
        }
    }

    pub fn prepare_auth_check(
        &self,
        source: PacketSource,
        request: AuthRequest,
    ) -> ServerAuthCheck {
        ServerAuthCheck { source, request }
    }
}

/// Input collected for future auth decision logic.
///
/// The auth handler will eventually check `shared_token`, `client_id`,
/// `protocol_version`, and `app_version`. Applying the result to server state
/// and creating outbound responses stays outside this boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthCheck {
    pub source: PacketSource,
    pub request: AuthRequest,
}

impl ServerAuthCheck {
    pub fn client_id(&self) -> &ClientId {
        &self.request.client_id
    }

    pub fn run_id(&self) -> &RunId {
        &self.request.run_id
    }

    pub fn protocol_version(&self) -> ProtocolVersion {
        self.request.protocol_version
    }

    pub fn app_version(&self) -> &AppVersion {
        &self.request.app_version
    }

    pub fn shared_token(&self) -> &str {
        &self.request.shared_token
    }

    pub fn display_name(&self) -> Option<&str> {
        self.request.display_name.as_deref()
    }
}

/// Errors at the server auth handoff boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerAuthBoundaryError {
    UnexpectedRoute { message_type: MessageType },
}

/// Result produced by future server auth decision logic.
///
/// This type carries the decision into the response boundary. It does not
/// perform token validation, whitelist lookup, connection state updates, or
/// socket sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthDecision {
    pub source: PacketSource,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub accepted: bool,
    pub reason_code: AuthResponseReasonCode,
    pub message: Option<String>,
    pub server_time: Option<TimestampMicros>,
    pub expected_protocol_version: Option<ProtocolVersion>,
}

impl ServerAuthDecision {
    pub fn accepted(
        source: PacketSource,
        client_id: ClientId,
        run_id: RunId,
        protocol_version: ProtocolVersion,
        server_time: Option<TimestampMicros>,
    ) -> Self {
        Self {
            source,
            client_id,
            run_id,
            protocol_version,
            accepted: true,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time,
            expected_protocol_version: None,
        }
    }

    pub fn rejected(
        source: PacketSource,
        client_id: ClientId,
        run_id: RunId,
        protocol_version: ProtocolVersion,
        reason_code: AuthResponseReasonCode,
        message: Option<String>,
        expected_protocol_version: Option<ProtocolVersion>,
    ) -> Self {
        Self {
            source,
            client_id,
            run_id,
            protocol_version,
            accepted: false,
            reason_code,
            message,
            server_time: None,
            expected_protocol_version,
        }
    }
}

/// Boundary that converts auth decisions into outbound auth responses.
///
/// This builds the typed `AuthResponse` message and destination handoff only.
/// Wire encoding and UDP socket sending are intentionally left to a later net
/// send layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthResponseBoundary;

impl ServerAuthResponseBoundary {
    pub fn build_for_send(&self, decision: ServerAuthDecision) -> ServerOutboundAuthResponse {
        let response = AuthResponse {
            message_type: MessageType::AuthResponse,
            protocol_version: decision.protocol_version,
            client_id: decision.client_id,
            run_id: decision.run_id,
            accepted: decision.accepted,
            reason_code: decision.reason_code,
            message: decision.message,
            server_time: decision.server_time,
            expected_protocol_version: decision.expected_protocol_version,
        };

        ServerOutboundAuthResponse {
            destination: decision.source,
            message: ProtocolMessage::AuthResponse(response),
        }
    }
}

/// Auth response handoff for a future net send layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOutboundAuthResponse {
    pub destination: PacketSource,
    pub message: ProtocolMessage,
}

impl ServerOutboundAuthResponse {
    pub fn auth_response(&self) -> Option<&AuthResponse> {
        match &self.message {
            ProtocolMessage::AuthResponse(response) => Some(response),
            _ => None,
        }
    }

    pub fn into_outbound_packet(self) -> OutboundPacket {
        OutboundPacket {
            destination: self.destination.into(),
            message: self.message,
        }
    }
}

/// Input for building a typed heartbeat acknowledgement handoff.
///
/// This is not heartbeat management. It only carries the already-decided ack
/// fields into the outbound message boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatAckInput {
    pub destination: PacketSource,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub echoed_sent_at: TimestampMicros,
    pub server_received_at: TimestampMicros,
    pub server_sent_at: TimestampMicros,
}

/// Boundary that converts heartbeat ack fields into a typed outbound message.
///
/// This does not update heartbeat state, calculate RTT, encode bytes, or send
/// through UDP sockets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatAckBoundary;

impl ServerHeartbeatAckBoundary {
    pub fn build_for_send(&self, input: ServerHeartbeatAckInput) -> ServerOutboundHeartbeatAck {
        let ack = HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: input.protocol_version,
            client_id: input.client_id,
            run_id: input.run_id,
            echoed_sent_at: input.echoed_sent_at,
            server_received_at: input.server_received_at,
            server_sent_at: input.server_sent_at,
        };

        ServerOutboundHeartbeatAck {
            destination: input.destination,
            message: ProtocolMessage::HeartbeatAck(ack),
        }
    }
}

/// HeartbeatAck handoff for a future net send layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOutboundHeartbeatAck {
    pub destination: PacketSource,
    pub message: ProtocolMessage,
}

impl ServerOutboundHeartbeatAck {
    pub fn heartbeat_ack(&self) -> Option<&HeartbeatAck> {
        match &self.message {
            ProtocolMessage::HeartbeatAck(ack) => Some(ack),
            _ => None,
        }
    }

    pub fn into_outbound_packet(self) -> OutboundPacket {
        OutboundPacket {
            destination: self.destination.into(),
            message: self.message,
        }
    }
}

/// Server-side outbound handoff boundary for a future net send queue.
///
/// This is a one-item handoff only. It does not implement an in-memory queue,
/// encode bytes, apply retry policy, or send through UDP sockets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerOutboundQueueBoundary {
    queue: OutboundPacketQueueBoundary,
}

impl ServerOutboundQueueBoundary {
    pub fn handoff_auth_response(&self, response: ServerOutboundAuthResponse) -> OutboundQueueItem {
        self.queue.handoff(response.into_outbound_packet())
    }

    pub fn handoff_heartbeat_ack(&self, ack: ServerOutboundHeartbeatAck) -> OutboundQueueItem {
        self.queue.handoff(ack.into_outbound_packet())
    }
}

fn message_type(message: &ProtocolMessage) -> MessageType {
    match message {
        ProtocolMessage::AuthRequest(_) => MessageType::AuthRequest,
        ProtocolMessage::AuthResponse(_) => MessageType::AuthResponse,
        ProtocolMessage::Heartbeat(_) => MessageType::Heartbeat,
        ProtocolMessage::HeartbeatAck(_) => MessageType::HeartbeatAck,
        ProtocolMessage::VideoFrame(_) => MessageType::VideoFrame,
        ProtocolMessage::ClientStats(_) => MessageType::ClientStats,
        ProtocolMessage::ServerNotice(_) => MessageType::ServerNotice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use stream_sync_protocol::{
        AppVersion, ClientId, Codec, ProtocolVersion, RunId, TimestampMicros, FIXED_HEADER_LEN,
        HEADER_FLAGS_OFFSET, HEADER_LENGTH_OFFSET, HEADER_MESSAGE_TYPE_OFFSET,
        HEADER_PAYLOAD_LENGTH_OFFSET, HEADER_PROTOCOL_VERSION_OFFSET, HEADER_RESERVED_OFFSET,
    };

    #[test]
    fn receive_loop_routes_decoded_packet() {
        let source = packet_source();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let receive_loop = ServerReceiveLoopStep::default();

        let outcome =
            receive_loop.handle_received_packet(ProtocolVersion(2), source, packet.as_slice());

        let ServerReceiveLoopOutcome::Routed(ServerInboundRoute::Heartbeat {
            source: routed_source,
            heartbeat,
        }) = outcome
        else {
            panic!("expected routed heartbeat");
        };
        assert_eq!(routed_source, source);
        assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
        assert_eq!(heartbeat.sent_at, TimestampMicros(1_234_567));
    }

    #[test]
    fn receive_loop_classifies_protocol_version_mismatch() {
        let source = packet_source();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 1, &payload);
        let receive_loop = ServerReceiveLoopStep::default();

        let outcome =
            receive_loop.handle_received_packet(ProtocolVersion(2), source, packet.as_slice());

        assert_eq!(
            outcome,
            ServerReceiveLoopOutcome::Rejected(ServerRejectedPacket {
                source,
                action: ServerDecodeErrorAction::RejectProtocolVersion,
                error: ProtocolError::UnsupportedProtocolVersion {
                    expected: ProtocolVersion(2),
                    actual: ProtocolVersion(1)
                }
            })
        );
    }

    #[test]
    fn receive_loop_classifies_malformed_packet_as_drop() {
        let source = packet_source();
        let receive_loop = ServerReceiveLoopStep::default();

        let outcome = receive_loop.handle_received_packet(ProtocolVersion(2), source, &[0; 15]);

        assert_eq!(
            outcome,
            ServerReceiveLoopOutcome::Rejected(ServerRejectedPacket {
                source,
                action: ServerDecodeErrorAction::DropPacket,
                error: ProtocolError::BufferTooShort
            })
        );
    }

    #[test]
    fn routes_auth_request_to_auth_boundary() {
        let source = packet_source();
        let request = AuthRequest {
            message_type: MessageType::AuthRequest,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: AppVersion("0.1.0".to_string()),
            shared_token: "shared-secret".to_string(),
            display_name: None,
            capabilities: Vec::new(),
            requested_video_profile: None,
        };
        let router = ServerInboundRouter;

        let route = router.route(DecodedInboundPacket {
            source,
            message: ProtocolMessage::AuthRequest(request.clone()),
        });

        assert_eq!(route, ServerInboundRoute::AuthRequest { source, request });
    }

    #[test]
    fn auth_handler_prepares_check_from_auth_route() {
        let source = packet_source();
        let request = AuthRequest {
            message_type: MessageType::AuthRequest,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: AppVersion("0.1.0".to_string()),
            shared_token: "shared-secret".to_string(),
            display_name: Some("Alice".to_string()),
            capabilities: Vec::new(),
            requested_video_profile: None,
        };
        let boundary = ServerAuthHandlerBoundary;

        let check = boundary
            .prepare_from_route(ServerInboundRoute::AuthRequest {
                source,
                request: request.clone(),
            })
            .expect("auth request route should prepare auth check");

        assert_eq!(check.source, source);
        assert_eq!(check.client_id(), &request.client_id);
        assert_eq!(check.run_id(), &request.run_id);
        assert_eq!(check.protocol_version(), request.protocol_version);
        assert_eq!(check.app_version(), &request.app_version);
        assert_eq!(check.shared_token(), request.shared_token);
        assert_eq!(check.display_name(), Some("Alice"));
    }

    #[test]
    fn auth_handler_rejects_non_auth_route() {
        let source = packet_source();
        let heartbeat = Heartbeat {
            message_type: MessageType::Heartbeat,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            sent_at: TimestampMicros(1_234_567),
            local_time: None,
            short_status: None,
        };
        let boundary = ServerAuthHandlerBoundary;

        let result =
            boundary.prepare_from_route(ServerInboundRoute::Heartbeat { source, heartbeat });

        assert_eq!(
            result,
            Err(ServerAuthBoundaryError::UnexpectedRoute {
                message_type: MessageType::Heartbeat
            })
        );
    }

    #[test]
    fn auth_response_boundary_builds_accepted_response_for_send() {
        let source = packet_source();
        let boundary = ServerAuthResponseBoundary;
        let decision = ServerAuthDecision::accepted(
            source,
            ClientId("client-1".to_string()),
            RunId("run-1".to_string()),
            ProtocolVersion(2),
            Some(TimestampMicros(2_000_000)),
        );

        let outbound = boundary.build_for_send(decision);

        assert_eq!(outbound.destination, source);
        let response = outbound
            .auth_response()
            .expect("outbound message should be AuthResponse");
        assert_eq!(response.message_type, MessageType::AuthResponse);
        assert_eq!(response.protocol_version, ProtocolVersion(2));
        assert_eq!(response.client_id, ClientId("client-1".to_string()));
        assert_eq!(response.run_id, RunId("run-1".to_string()));
        assert!(response.accepted);
        assert_eq!(response.reason_code, AuthResponseReasonCode::Ok);
        assert_eq!(response.message, None);
        assert_eq!(response.server_time, Some(TimestampMicros(2_000_000)));
        assert_eq!(response.expected_protocol_version, None);
    }

    #[test]
    fn auth_response_boundary_builds_rejected_response_for_send() {
        let source = packet_source();
        let boundary = ServerAuthResponseBoundary;
        let decision = ServerAuthDecision::rejected(
            source,
            ClientId("client-1".to_string()),
            RunId("run-1".to_string()),
            ProtocolVersion(1),
            AuthResponseReasonCode::ProtocolMismatch,
            Some("unsupported protocol_version".to_string()),
            Some(ProtocolVersion(2)),
        );

        let outbound = boundary.build_for_send(decision);

        assert_eq!(outbound.destination, source);
        let response = outbound
            .auth_response()
            .expect("outbound message should be AuthResponse");
        assert_eq!(response.message_type, MessageType::AuthResponse);
        assert_eq!(response.protocol_version, ProtocolVersion(1));
        assert_eq!(response.client_id, ClientId("client-1".to_string()));
        assert_eq!(response.run_id, RunId("run-1".to_string()));
        assert!(!response.accepted);
        assert_eq!(
            response.reason_code,
            AuthResponseReasonCode::ProtocolMismatch
        );
        assert_eq!(
            response.message.as_deref(),
            Some("unsupported protocol_version")
        );
        assert_eq!(response.server_time, None);
        assert_eq!(response.expected_protocol_version, Some(ProtocolVersion(2)));
    }

    #[test]
    fn outbound_queue_boundary_hands_off_auth_response_to_net_send_layer() {
        let source = packet_source();
        let response_boundary = ServerAuthResponseBoundary;
        let queue_boundary = ServerOutboundQueueBoundary::default();
        let decision = ServerAuthDecision::accepted(
            source,
            ClientId("client-1".to_string()),
            RunId("run-1".to_string()),
            ProtocolVersion(2),
            Some(TimestampMicros(2_000_000)),
        );
        let outbound = response_boundary.build_for_send(decision);

        let queue_item = queue_boundary.handoff_auth_response(outbound);

        assert_eq!(queue_item.packet.destination.address, source.address);
        let ProtocolMessage::AuthResponse(response) = queue_item.packet.message else {
            panic!("expected AuthResponse outbound message");
        };
        assert_eq!(response.message_type, MessageType::AuthResponse);
        assert!(response.accepted);
    }

    #[test]
    fn heartbeat_ack_boundary_builds_ack_for_send() {
        let source = packet_source();
        let boundary = ServerHeartbeatAckBoundary;
        let input = ServerHeartbeatAckInput {
            destination: source,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000_000),
            server_received_at: TimestampMicros(1_000_100),
            server_sent_at: TimestampMicros(1_000_200),
        };

        let outbound = boundary.build_for_send(input);

        assert_eq!(outbound.destination, source);
        let ack = outbound
            .heartbeat_ack()
            .expect("outbound message should be HeartbeatAck");
        assert_eq!(ack.message_type, MessageType::HeartbeatAck);
        assert_eq!(ack.protocol_version, ProtocolVersion(2));
        assert_eq!(ack.client_id, ClientId("client-1".to_string()));
        assert_eq!(ack.run_id, RunId("run-1".to_string()));
        assert_eq!(ack.echoed_sent_at, TimestampMicros(1_000_000));
        assert_eq!(ack.server_received_at, TimestampMicros(1_000_100));
        assert_eq!(ack.server_sent_at, TimestampMicros(1_000_200));
    }

    #[test]
    fn outbound_queue_boundary_hands_off_heartbeat_ack_to_net_send_layer() {
        let source = packet_source();
        let ack_boundary = ServerHeartbeatAckBoundary;
        let queue_boundary = ServerOutboundQueueBoundary::default();
        let outbound = ack_boundary.build_for_send(ServerHeartbeatAckInput {
            destination: source,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000_000),
            server_received_at: TimestampMicros(1_000_100),
            server_sent_at: TimestampMicros(1_000_200),
        });

        let queue_item = queue_boundary.handoff_heartbeat_ack(outbound);

        assert_eq!(queue_item.packet.destination.address, source.address);
        let ProtocolMessage::HeartbeatAck(ack) = queue_item.packet.message else {
            panic!("expected HeartbeatAck outbound message");
        };
        assert_eq!(ack.message_type, MessageType::HeartbeatAck);
        assert_eq!(ack.echoed_sent_at, TimestampMicros(1_000_000));
    }

    #[test]
    fn routes_heartbeat_to_heartbeat_boundary() {
        let source = packet_source();
        let heartbeat = Heartbeat {
            message_type: MessageType::Heartbeat,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            sent_at: TimestampMicros(1_234_567),
            local_time: None,
            short_status: None,
        };
        let router = ServerInboundRouter;

        let route = router.route(DecodedInboundPacket {
            source,
            message: ProtocolMessage::Heartbeat(heartbeat.clone()),
        });

        assert_eq!(route, ServerInboundRoute::Heartbeat { source, heartbeat });
    }

    #[test]
    fn routes_video_frame_to_video_boundary() {
        let source = packet_source();
        let frame = VideoFrame {
            message_type: MessageType::VideoFrame,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            frame_id: 42,
            capture_timestamp: TimestampMicros(1_000_000),
            send_timestamp: TimestampMicros(1_000_100),
            is_keyframe: true,
            metadata_reserved: [0; 3],
            width: 1280,
            height: 720,
            fps_nominal: 30,
            codec: Codec::H264,
            payload_size: 3,
            payload: vec![0xaa, 0xbb, 0xcc],
        };
        let router = ServerInboundRouter;

        let route = router.route(DecodedInboundPacket {
            source,
            message: ProtocolMessage::VideoFrame(frame.clone()),
        });

        assert_eq!(route, ServerInboundRoute::VideoFrame { source, frame });
    }

    fn packet_source() -> PacketSource {
        "127.0.0.1:5000"
            .parse::<SocketAddr>()
            .expect("source address should parse")
            .into()
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
