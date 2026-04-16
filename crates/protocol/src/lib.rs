pub const CRATE_NAME: &str = "stream-sync-protocol";

/// Client identifier allowed by the server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientId(pub String);

/// Identifier for one application/session run.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunId(pub String);

/// Version of the StreamSync wire protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProtocolVersion(pub u32);

/// Version of an application binary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppVersion(pub String);

/// Timestamp value expressed in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimestampMicros(pub u64);

/// Byte length of the fixed packet header used by the initial wire format.
pub const FIXED_HEADER_LEN: u16 = 16;

/// Byte offset of `message_type` in the fixed packet header.
pub const HEADER_MESSAGE_TYPE_OFFSET: usize = 0;

/// Byte offset of `header_length` in the fixed packet header.
pub const HEADER_LENGTH_OFFSET: usize = 2;

/// Byte offset of `protocol_version` in the fixed packet header.
pub const HEADER_PROTOCOL_VERSION_OFFSET: usize = 4;

/// Byte offset of `payload_length` in the fixed packet header.
pub const HEADER_PAYLOAD_LENGTH_OFFSET: usize = 8;

/// Byte offset of `flags` in the fixed packet header.
pub const HEADER_FLAGS_OFFSET: usize = 12;

/// Byte offset of the reserved field in the fixed packet header.
pub const HEADER_RESERVED_OFFSET: usize = 14;

/// Decoded fixed packet header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedHeader {
    pub message_type: MessageType,
    pub header_length: u16,
    pub protocol_version: ProtocolVersion,
    pub payload_length: u32,
    pub flags: u16,
    pub reserved: u16,
}

/// Borrowed view produced after fixed header decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketView<'a> {
    pub header: FixedHeader,
    pub payload: &'a [u8],
}

/// Minimal fixed header decoder for the initial wire format.
///
/// This decoder parses only the 16 byte fixed packet header and returns a raw
/// payload slice. It does not check protocol compatibility or interpret the
/// payload body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FixedHeaderCodec;

impl FixedHeaderCodec {
    pub fn decode_fixed_header<'a>(
        &self,
        packet: &'a [u8],
    ) -> Result<PacketView<'a>, ProtocolError> {
        decode_fixed_header(packet)
    }
}

/// Decode the 16 byte fixed packet header and split out the raw payload slice.
pub fn decode_fixed_header(packet: &[u8]) -> Result<PacketView<'_>, ProtocolError> {
    let fixed_header_len = usize::from(FIXED_HEADER_LEN);
    if packet.len() < fixed_header_len {
        return Err(ProtocolError::BufferTooShort);
    }

    let message_type = MessageType::try_from(read_u16_le(packet, HEADER_MESSAGE_TYPE_OFFSET))?;
    let header_length = read_u16_le(packet, HEADER_LENGTH_OFFSET);
    if header_length != FIXED_HEADER_LEN {
        return Err(ProtocolError::InvalidHeaderLength {
            actual: header_length,
        });
    }

    let protocol_version = ProtocolVersion(read_u32_le(packet, HEADER_PROTOCOL_VERSION_OFFSET));
    let payload_length = read_u32_le(packet, HEADER_PAYLOAD_LENGTH_OFFSET);
    let flags = read_u16_le(packet, HEADER_FLAGS_OFFSET);
    let reserved = read_u16_le(packet, HEADER_RESERVED_OFFSET);

    let payload_len = payload_length as usize;
    let expected_packet_len =
        fixed_header_len
            .checked_add(payload_len)
            .ok_or(ProtocolError::InvalidPayloadLength {
                expected: payload_length,
                actual: packet.len().saturating_sub(fixed_header_len),
            })?;

    if packet.len() != expected_packet_len {
        return Err(ProtocolError::InvalidPayloadLength {
            expected: payload_length,
            actual: packet.len().saturating_sub(fixed_header_len),
        });
    }

    Ok(PacketView {
        header: FixedHeader {
            message_type,
            header_length,
            protocol_version,
            payload_length,
            flags,
            reserved,
        },
        payload: &packet[fixed_header_len..],
    })
}

fn read_u16_le(packet: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([packet[offset], packet[offset + 1]])
}

fn read_u32_le(packet: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        packet[offset],
        packet[offset + 1],
        packet[offset + 2],
        packet[offset + 3],
    ])
}

/// Context passed to future decode entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecodeContext {
    pub expected_protocol_version: ProtocolVersion,
}

/// Context passed to future encode entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EncodeContext {
    pub protocol_version: ProtocolVersion,
}

/// Decoded protocol message variants.
///
/// This enum defines the message dispatch boundary. Constructing these variants
/// from bytes is intentionally left for a later implementation step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolMessage {
    AuthRequest(AuthRequest),
    AuthResponse(AuthResponse),
    Heartbeat(Heartbeat),
    HeartbeatAck(HeartbeatAck),
    VideoFrame(VideoFrame),
    ClientStats(ClientStats),
    ServerNotice(ServerNotice),
}

/// Errors that future wire encode/decode entry points may return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    BufferTooShort,
    InvalidHeaderLength {
        actual: u16,
    },
    InvalidPayloadLength {
        expected: u32,
        actual: usize,
    },
    UnknownMessageType(u16),
    UnsupportedProtocolVersion {
        expected: ProtocolVersion,
        actual: ProtocolVersion,
    },
    PayloadDecodeNotImplemented(MessageType),
    EncodeNotImplemented(MessageType),
}

/// Future entry point for fixed header decoding.
///
/// Implementors should only parse the fixed packet header and split out the
/// payload slice. They should not perform socket I/O or app-level handling.
pub trait FixedHeaderDecoder {
    fn decode_fixed_header<'a>(&self, packet: &'a [u8]) -> Result<PacketView<'a>, ProtocolError>;
}

impl FixedHeaderDecoder for FixedHeaderCodec {
    fn decode_fixed_header<'a>(&self, packet: &'a [u8]) -> Result<PacketView<'a>, ProtocolError> {
        decode_fixed_header(packet)
    }
}

/// Future entry point for payload decoding after message type dispatch.
pub trait PayloadDecoder {
    fn decode_payload(
        &self,
        context: DecodeContext,
        header: FixedHeader,
        payload: &[u8],
    ) -> Result<ProtocolMessage, ProtocolError>;
}

/// Future entry point for message encoding into one packet buffer.
pub trait MessageEncoder {
    fn encode_message(
        &self,
        context: EncodeContext,
        message: &ProtocolMessage,
        output: &mut Vec<u8>,
    ) -> Result<(), ProtocolError>;
}

/// Message kinds used by the MVP protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum MessageType {
    AuthRequest = 1,
    AuthResponse = 2,
    Heartbeat = 3,
    HeartbeatAck = 4,
    VideoFrame = 5,
    ClientStats = 6,
    ServerNotice = 7,
}

impl TryFrom<u16> for MessageType {
    type Error = ProtocolError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::AuthRequest),
            2 => Ok(Self::AuthResponse),
            3 => Ok(Self::Heartbeat),
            4 => Ok(Self::HeartbeatAck),
            5 => Ok(Self::VideoFrame),
            6 => Ok(Self::ClientStats),
            7 => Ok(Self::ServerNotice),
            value => Err(ProtocolError::UnknownMessageType(value)),
        }
    }
}

/// Initial authentication request sent from a client to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub app_version: AppVersion,
    pub shared_token: String,
    pub display_name: Option<String>,
    pub capabilities: Vec<String>,
    pub requested_video_profile: Option<String>,
}

/// Authentication result returned by the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthResponse {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub accepted: bool,
    pub reason_code: AuthResponseReasonCode,
    pub message: Option<String>,
    pub server_time: Option<TimestampMicros>,
    pub expected_protocol_version: Option<ProtocolVersion>,
}

/// Reason code for an authentication response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthResponseReasonCode {
    Ok,
    InvalidToken,
    UnknownClient,
    ProtocolMismatch,
    AlreadyConnected,
    InternalError,
}

/// Periodic liveness message sent by an authenticated client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heartbeat {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub sent_at: TimestampMicros,
    pub local_time: Option<TimestampMicros>,
    pub short_status: Option<String>,
}

/// Server response to a heartbeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatAck {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub echoed_sent_at: TimestampMicros,
    pub server_received_at: TimestampMicros,
    pub server_sent_at: TimestampMicros,
}

/// Encoded video frame sent from a client to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub frame_id: u64,
    pub capture_timestamp: TimestampMicros,
    pub send_timestamp: TimestampMicros,
    pub is_keyframe: bool,
    pub width: u32,
    pub height: u32,
    pub fps_nominal: u32,
    pub codec: Codec,
    pub payload_size: usize,
    pub payload: Vec<u8>,
}

/// Video codec identifier for encoded frame payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    H264,
}

/// Periodic client-side metrics used for monitoring and troubleshooting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientStats {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub sent_at: TimestampMicros,
    pub capture_fps: u32,
    pub dropped_frames: u64,
    pub bitrate_kbps: u32,
}

/// Server-side notice sent to report warnings, disconnects, or protocol issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNotice {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub run_id: RunId,
    pub notice_type: NoticeType,
    pub message: String,
}

/// Type of server notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoticeType {
    Warning,
    Disconnect,
    ProtocolError,
    AuthExpired,
    ServerShutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_fixed_header_and_payload_slice() {
        let payload = [0xaa, 0xbb, 0xcc];
        let packet = test_packet(
            MessageType::VideoFrame as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );

        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        assert_eq!(decoded.header.message_type, MessageType::VideoFrame);
        assert_eq!(decoded.header.header_length, FIXED_HEADER_LEN);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));
        assert_eq!(decoded.header.payload_length, payload.len() as u32);
        assert_eq!(decoded.header.flags, 0x0034);
        assert_eq!(decoded.header.reserved, 0);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn rejects_short_header() {
        let packet = [0; 15];

        let decoded = decode_fixed_header(&packet);

        assert_eq!(decoded, Err(ProtocolError::BufferTooShort));
    }

    #[test]
    fn rejects_unknown_message_type() {
        let packet = test_packet(999, FIXED_HEADER_LEN, 1, &[]);

        let decoded = decode_fixed_header(&packet);

        assert_eq!(decoded, Err(ProtocolError::UnknownMessageType(999)));
    }

    #[test]
    fn rejects_invalid_header_length() {
        let packet = test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN + 1,
            1,
            &[],
        );

        let decoded = decode_fixed_header(&packet);

        assert_eq!(
            decoded,
            Err(ProtocolError::InvalidHeaderLength {
                actual: FIXED_HEADER_LEN + 1
            })
        );
    }

    #[test]
    fn rejects_invalid_payload_length() {
        let mut packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 1, &[0xaa]);
        packet[HEADER_PAYLOAD_LENGTH_OFFSET..HEADER_PAYLOAD_LENGTH_OFFSET + 4]
            .copy_from_slice(&2_u32.to_le_bytes());

        let decoded = decode_fixed_header(&packet);

        assert_eq!(
            decoded,
            Err(ProtocolError::InvalidPayloadLength {
                expected: 2,
                actual: 1
            })
        );
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
        packet[HEADER_FLAGS_OFFSET..HEADER_FLAGS_OFFSET + 2]
            .copy_from_slice(&0x0034_u16.to_le_bytes());
        packet[HEADER_RESERVED_OFFSET..HEADER_RESERVED_OFFSET + 2]
            .copy_from_slice(&0_u16.to_le_bytes());
        packet.extend_from_slice(payload);
        packet
    }
}
