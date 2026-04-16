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

/// Placeholder for the fixed packet header. Encoding and decoding are not
/// implemented yet; this mirrors the documented byte layout.
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
///
/// This is an API-boundary placeholder. The actual fixed header parser is not
/// implemented yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketView<'a> {
    pub header: FixedHeader,
    pub payload: &'a [u8],
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
