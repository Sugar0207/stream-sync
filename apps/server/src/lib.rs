use stream_sync_net_core::{
    DecodedInboundPacket, InboundPacket, InboundPacketDecoder, NetDecodeError, PacketSource,
};
use stream_sync_protocol::{
    AuthRequest, Heartbeat, MessageType, ProtocolError, ProtocolMessage, ProtocolVersion,
    VideoFrame,
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
