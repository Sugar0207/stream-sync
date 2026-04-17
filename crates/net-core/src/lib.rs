use std::net::SocketAddr;

use stream_sync_protocol::{
    decode_fixed_header, decode_payload_by_message_type, validate_protocol_version, DecodeContext,
    ProtocolError, ProtocolMessage,
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

/// Minimal boundary between app/server response code and a future send queue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OutboundPacketQueueBoundary;

impl OutboundPacketQueueBoundary {
    pub fn handoff(&self, packet: OutboundPacket) -> OutboundQueueItem {
        OutboundQueueItem { packet }
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
        ClientId, MessageType, ProtocolVersion, TimestampMicros, FIXED_HEADER_LEN,
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
        let message = ProtocolMessage::HeartbeatAck(stream_sync_protocol::HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: stream_sync_protocol::RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000_000),
            server_received_at: TimestampMicros(1_000_100),
            server_sent_at: TimestampMicros(1_000_200),
        });
        let boundary = OutboundPacketQueueBoundary;

        let item = boundary.handoff(OutboundPacket {
            destination,
            message: message.clone(),
        });

        assert_eq!(item.packet.destination, destination);
        assert_eq!(item.packet.message, message);
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
