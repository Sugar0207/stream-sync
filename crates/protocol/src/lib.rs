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

/// Message kinds used by the MVP protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    AuthRequest,
    AuthResponse,
    Heartbeat,
    HeartbeatAck,
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
    pub server_time: Option<u64>,
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
    pub sent_at: u64,
    pub local_time: Option<u64>,
    pub short_status: Option<String>,
}

/// Server response to a heartbeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatAck {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub echoed_sent_at: u64,
    pub server_received_at: u64,
    pub server_sent_at: u64,
}
