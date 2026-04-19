use std::{
    collections::BTreeMap,
    env, fs, io,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
};
use stream_sync_config::{
    ConfigLoadError, ServerAuthConfig, ServerAuthConfigBoundary, SharedTokenSecretRef,
};
use stream_sync_net_core::{
    DecodedInboundPacket, EncodedOutboundPacket, InboundPacket, InboundPacketDecoder,
    NetDecodeError, NetEncodeError, OutboundPacket, OutboundPacketEncoderBoundary,
    OutboundPacketQueueBoundary, OutboundQueueItem, PacketSource, UdpSocketIoBoundary,
    DEFAULT_UDP_PACKET_BUFFER_LEN,
};
use stream_sync_protocol::{
    AppVersion, AuthRequest, AuthResponse, AuthResponseReasonCode, ClientId, Heartbeat,
    HeartbeatAck, MessageType, ProtocolError, ProtocolMessage, ProtocolMessageEncoderBoundary,
    ProtocolVersion, RunId, TimestampMicros, VideoFrame,
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
    gate: PacketAcceptanceGateBoundary,
}

impl ServerReceiveLoopStep {
    pub fn handle_received_packet(
        &self,
        expected_protocol_version: ProtocolVersion,
        source: PacketSource,
        packet_bytes: &[u8],
    ) -> ServerReceiveLoopOutcome {
        match self.decode_and_route(expected_protocol_version, source, packet_bytes) {
            Ok(route) => ServerReceiveLoopOutcome::Routed(route),
            Err(rejection) => ServerReceiveLoopOutcome::Rejected(rejection),
        }
    }

    pub fn handle_received_packet_with_gate(
        &self,
        expected_protocol_version: ProtocolVersion,
        registry: &AuthenticatedSenderRegistry,
        source: PacketSource,
        packet_bytes: &[u8],
    ) -> ServerReceiveLoopGateOutcome {
        let route = match self.decode_and_route(expected_protocol_version, source, packet_bytes) {
            Ok(route) => route,
            Err(rejection) => {
                return ServerReceiveLoopGateOutcome::Rejected(
                    ServerReceiveLoopGateRejection::Decode(rejection),
                );
            }
        };

        match self.gate.evaluate_route(registry, &route) {
            PacketAcceptanceDecision::Accepted => ServerReceiveLoopGateOutcome::Accepted(route),
            PacketAcceptanceDecision::Rejected(rejection) => {
                ServerReceiveLoopGateOutcome::Rejected(ServerReceiveLoopGateRejection::Acceptance(
                    rejection,
                ))
            }
        }
    }

    fn decode_and_route(
        &self,
        expected_protocol_version: ProtocolVersion,
        source: PacketSource,
        packet_bytes: &[u8],
    ) -> Result<ServerInboundRoute, ServerRejectedPacket> {
        let context = stream_sync_protocol::DecodeContext {
            expected_protocol_version,
        };
        let packet = InboundPacket {
            source,
            bytes: packet_bytes,
        };

        match self.decoder.decode(context, packet) {
            Ok(decoded) => Ok(self.router.route(decoded)),
            Err(NetDecodeError::Protocol { source, error }) => Err(ServerRejectedPacket {
                source,
                action: classify_protocol_error(&error),
                error,
            }),
        }
    }
}

/// Minimal synchronous UDP socket adapter for the server.
///
/// This is the first concrete socket I/O layer: it receives one datagram from a
/// bound UDP socket and immediately hands it to `ServerReceiveLoopStep`, or
/// sends already-encoded bytes to a destination. It does not run an event loop,
/// spawn async tasks, execute retry, fragment packets, or handle application
/// routes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerUdpSocketIoStep {
    socket_io: UdpSocketIoBoundary,
    receive_loop: ServerReceiveLoopStep,
}

impl ServerUdpSocketIoStep {
    pub fn receive_one_with_gate(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        expected_protocol_version: ProtocolVersion,
        registry: &AuthenticatedSenderRegistry,
    ) -> io::Result<ServerReceiveLoopGateOutcome> {
        let packet = self.socket_io.receive_one(socket, buffer)?;

        Ok(self.receive_loop.handle_received_packet_with_gate(
            expected_protocol_version,
            registry,
            packet.source,
            packet.bytes,
        ))
    }

    pub fn send_encoded(
        &self,
        socket: &UdpSocket,
        packet: &EncodedOutboundPacket,
    ) -> io::Result<usize> {
        self.socket_io.send_encoded(socket, packet)
    }
}

/// One-shot auth response PoC connection from UDP receive to UDP send.
///
/// This composes the existing receive, auth flow, outbound queue handoff,
/// accepted sender registry registration, protocol encode, and socket send
/// boundaries for one packet. It does not run a continuous loop, spawn async
/// tasks, write JSON Lines logs, retry, fragment, encrypt, or handle heartbeat
/// / video frame routes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthResponsePocStep {
    socket_io: ServerUdpSocketIoStep,
    auth_flow: ServerAuthFlowStep,
    sender_registry: AuthenticatedSenderRegistryBoundary,
    outbound_encoder: OutboundPacketEncoderBoundary,
    protocol_encoder: ProtocolMessageEncoderBoundary,
}

impl ServerAuthResponsePocStep {
    pub fn run_one(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        expected_protocol_version: ProtocolVersion,
        config: &ServerAuthConfig,
        registry: &mut AuthenticatedSenderRegistry,
    ) -> Result<ServerAuthResponsePocOutcome, ServerAuthResponsePocError> {
        let receive_outcome = self
            .socket_io
            .receive_one_with_gate(socket, buffer, expected_protocol_version, registry)
            .map_err(|error| ServerAuthResponsePocError::Receive(error.kind()))?;

        let route = match receive_outcome {
            ServerReceiveLoopGateOutcome::Accepted(route) => route,
            ServerReceiveLoopGateOutcome::Rejected(rejection) => {
                return Err(ServerAuthResponsePocError::Rejected(rejection));
            }
        };

        let auth_flow = self
            .auth_flow
            .handle_auth_route(route, config)
            .map_err(ServerAuthResponsePocError::Auth)?;
        let registered_sender = auth_flow
            .registry_registration
            .clone()
            .map(|registration| self.sender_registry.register(registry, registration));

        let encode_request = self.outbound_encoder.prepare_encode(
            stream_sync_protocol::EncodeContext {
                protocol_version: expected_protocol_version,
            },
            auth_flow.queue_item.clone(),
        );
        let encoded_packet = self
            .outbound_encoder
            .encode_with(&self.protocol_encoder, encode_request)
            .map_err(ServerAuthResponsePocError::Encode)?;
        let bytes_sent = self
            .socket_io
            .send_encoded(socket, &encoded_packet)
            .map_err(|error| ServerAuthResponsePocError::Send(error.kind()))?;

        Ok(ServerAuthResponsePocOutcome {
            auth_flow,
            registered_sender,
            encoded_packet,
            bytes_sent,
        })
    }
}

/// Result of one auth response PoC receive/process/send step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthResponsePocOutcome {
    pub auth_flow: ServerAuthFlowOutcome,
    pub registered_sender: Option<AuthenticatedSenderEntry>,
    pub encoded_packet: EncodedOutboundPacket,
    pub bytes_sent: usize,
}

/// Error from one auth response PoC step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerAuthResponsePocError {
    Receive(io::ErrorKind),
    Rejected(ServerReceiveLoopGateRejection),
    Auth(ServerAuthBoundaryError),
    Encode(NetEncodeError),
    Send(io::ErrorKind),
}

/// Launcher for the one-shot auth response PoC.
///
/// This is the minimal startup boundary: it loads server config, resolves the
/// bind address, binds one UDP socket, initializes the in-memory registry, and
/// calls `ServerAuthResponsePocStep` once. It does not run a continuous loop,
/// spawn an async runtime, write logs, retry, fragment, or handle heartbeat /
/// video frame packets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthResponsePocLauncher {
    socket_io: UdpSocketIoBoundary,
    poc_step: ServerAuthResponsePocStep,
}

impl ServerAuthResponsePocLauncher {
    pub fn load_startup_config_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ServerAuthResponsePocStartupConfig, ServerAuthResponsePocStartupError> {
        let path = path.as_ref();
        let content =
            fs::read_to_string(path).map_err(|error| ServerAuthResponsePocStartupError::Io {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        self.load_startup_config_from_str(&content)
    }

    pub fn load_startup_config_from_str(
        &self,
        input: &str,
    ) -> Result<ServerAuthResponsePocStartupConfig, ServerAuthResponsePocStartupError> {
        let server_settings = parse_server_poc_settings(input)?;
        let auth_config = ServerAuthConfigBoundary::load_from_str(input)
            .map_err(ServerAuthResponsePocStartupError::AuthConfig)?;

        Ok(ServerAuthResponsePocStartupConfig {
            bind_address: resolve_bind_address(
                &server_settings.bind_host,
                server_settings.bind_port,
            )?,
            expected_protocol_version: ProtocolVersion(server_settings.protocol_version),
            auth_config,
        })
    }

    pub fn run_once_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ServerAuthResponsePocStartupOutcome, ServerAuthResponsePocStartupError> {
        let startup_config = self.load_startup_config_from_path(path)?;
        self.run_once(startup_config)
    }

    pub fn run_once(
        &self,
        startup_config: ServerAuthResponsePocStartupConfig,
    ) -> Result<ServerAuthResponsePocStartupOutcome, ServerAuthResponsePocStartupError> {
        let socket = self
            .socket_io
            .bind(startup_config.bind_address)
            .map_err(|error| ServerAuthResponsePocStartupError::Bind {
                address: startup_config.bind_address,
                kind: error.kind(),
            })?;
        let mut buffer = vec![0_u8; DEFAULT_UDP_PACKET_BUFFER_LEN];
        let mut registry = AuthenticatedSenderRegistry::default();
        let outcome = self
            .poc_step
            .run_one(
                &socket,
                &mut buffer,
                startup_config.expected_protocol_version,
                &startup_config.auth_config,
                &mut registry,
            )
            .map_err(ServerAuthResponsePocStartupError::Poc)?;

        Ok(ServerAuthResponsePocStartupOutcome {
            bind_address: startup_config.bind_address,
            registry,
            outcome,
        })
    }
}

/// Startup config needed by the one-shot auth response PoC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthResponsePocStartupConfig {
    pub bind_address: SocketAddr,
    pub expected_protocol_version: ProtocolVersion,
    pub auth_config: ServerAuthConfig,
}

/// Result of launching the one-shot auth response PoC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthResponsePocStartupOutcome {
    pub bind_address: SocketAddr,
    pub registry: AuthenticatedSenderRegistry,
    pub outcome: ServerAuthResponsePocOutcome,
}

/// Startup error for the one-shot auth response PoC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerAuthResponsePocStartupError {
    Io {
        path: PathBuf,
        message: String,
    },
    Config(ServerAuthResponsePocConfigError),
    AuthConfig(ConfigLoadError),
    Bind {
        address: SocketAddr,
        kind: io::ErrorKind,
    },
    Poc(ServerAuthResponsePocError),
}

/// Minimal config parse errors for the one-shot auth response PoC launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerAuthResponsePocConfigError {
    InvalidTomlLine {
        line: usize,
        message: String,
    },
    InvalidTomlString {
        line: usize,
        key: String,
    },
    InvalidNumber {
        line: usize,
        key: String,
    },
    MissingField {
        section: &'static str,
        key: &'static str,
    },
    InvalidBindAddress {
        value: String,
        message: String,
    },
}

/// Convenience entry point for the server binary and manual PoC wiring.
pub fn run_auth_response_poc_once_from_path(
    path: impl AsRef<Path>,
) -> Result<ServerAuthResponsePocStartupOutcome, ServerAuthResponsePocStartupError> {
    ServerAuthResponsePocLauncher::default().run_once_from_path(path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerPocSettings {
    bind_host: String,
    bind_port: u16,
    protocol_version: u32,
}

#[derive(Debug, Default)]
struct PartialServerPocSettings {
    bind_host: Option<String>,
    bind_port: Option<u16>,
    protocol_version: Option<u32>,
}

fn parse_server_poc_settings(
    input: &str,
) -> Result<ServerPocSettings, ServerAuthResponsePocStartupError> {
    let mut current_section: Option<&str> = None;
    let mut parsed = PartialServerPocSettings::default();

    for (index, raw_line) in input.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(section_name) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            current_section = match section_name {
                "server" => Some("server"),
                "session" => Some("session"),
                _ => None,
            };
            continue;
        }

        let Some(section) = current_section else {
            continue;
        };
        let Some((key, raw_value)) = line.split_once('=') else {
            return Err(ServerAuthResponsePocStartupError::Config(
                ServerAuthResponsePocConfigError::InvalidTomlLine {
                    line: line_number,
                    message: "expected key = value".to_string(),
                },
            ));
        };
        let key = key.trim();
        let value = raw_value.trim();

        match (section, key) {
            ("server", "bind_host") => {
                parsed.bind_host = Some(parse_poc_toml_string(value, line_number, key)?);
            }
            ("server", "bind_port") => {
                parsed.bind_port = Some(parse_poc_u16(value, line_number, key)?);
            }
            ("session", "protocol_version") => {
                parsed.protocol_version = Some(parse_poc_u32(value, line_number, key)?);
            }
            _ => {}
        }
    }

    Ok(ServerPocSettings {
        bind_host: parsed.bind_host.ok_or_else(|| {
            ServerAuthResponsePocStartupError::Config(
                ServerAuthResponsePocConfigError::MissingField {
                    section: "server",
                    key: "bind_host",
                },
            )
        })?,
        bind_port: parsed.bind_port.ok_or_else(|| {
            ServerAuthResponsePocStartupError::Config(
                ServerAuthResponsePocConfigError::MissingField {
                    section: "server",
                    key: "bind_port",
                },
            )
        })?,
        protocol_version: parsed.protocol_version.ok_or_else(|| {
            ServerAuthResponsePocStartupError::Config(
                ServerAuthResponsePocConfigError::MissingField {
                    section: "session",
                    key: "protocol_version",
                },
            )
        })?,
    })
}

fn resolve_bind_address(
    host: &str,
    port: u16,
) -> Result<SocketAddr, ServerAuthResponsePocStartupError> {
    let value = format!("{host}:{port}");
    (host, port)
        .to_socket_addrs()
        .map_err(|error| {
            ServerAuthResponsePocStartupError::Config(
                ServerAuthResponsePocConfigError::InvalidBindAddress {
                    value: value.clone(),
                    message: error.to_string(),
                },
            )
        })?
        .next()
        .ok_or_else(|| {
            ServerAuthResponsePocStartupError::Config(
                ServerAuthResponsePocConfigError::InvalidBindAddress {
                    value,
                    message: "address resolved to no socket addresses".to_string(),
                },
            )
        })
}

fn parse_poc_toml_string(
    value: &str,
    line: usize,
    key: &str,
) -> Result<String, ServerAuthResponsePocStartupError> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(ToString::to_string)
        .ok_or_else(|| {
            ServerAuthResponsePocStartupError::Config(
                ServerAuthResponsePocConfigError::InvalidTomlString {
                    line,
                    key: key.to_string(),
                },
            )
        })
}

fn parse_poc_u16(
    value: &str,
    line: usize,
    key: &str,
) -> Result<u16, ServerAuthResponsePocStartupError> {
    value.parse::<u16>().map_err(|_| {
        ServerAuthResponsePocStartupError::Config(ServerAuthResponsePocConfigError::InvalidNumber {
            line,
            key: key.to_string(),
        })
    })
}

fn parse_poc_u32(
    value: &str,
    line: usize,
    key: &str,
) -> Result<u32, ServerAuthResponsePocStartupError> {
    value.parse::<u32>().map_err(|_| {
        ServerAuthResponsePocStartupError::Config(ServerAuthResponsePocConfigError::InvalidNumber {
            line,
            key: key.to_string(),
        })
    })
}

fn strip_toml_comment(line: &str) -> &str {
    line.split_once('#')
        .map(|(before_comment, _)| before_comment)
        .unwrap_or(line)
}

/// Result of processing one already-received packet through the server boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveLoopOutcome {
    Routed(ServerInboundRoute),
    Rejected(ServerRejectedPacket),
}

/// Result after decode, route, and packet acceptance gate evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveLoopGateOutcome {
    Accepted(ServerInboundRoute),
    Rejected(ServerReceiveLoopGateRejection),
}

/// Rejection decision passed to future drop / log handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveLoopGateRejection {
    Decode(ServerRejectedPacket),
    Acceptance(PacketAcceptanceRejection),
}

/// Boundary that prepares receive rejections for future drop and log layers.
///
/// This preserves the rejection reason for both layers. It does not execute the
/// packet drop, emit logs, update state, or call UDP sockets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerRejectionDropLogHandoffBoundary;

impl ServerRejectionDropLogHandoffBoundary {
    pub fn handoff(
        &self,
        rejection: ServerReceiveLoopGateRejection,
    ) -> ServerRejectionDropLogInput {
        let (source, reason) = match rejection {
            ServerReceiveLoopGateRejection::Decode(rejected) => (
                rejected.source,
                ServerRejectionHandoffReason::Decode {
                    action: rejected.action,
                    error: rejected.error,
                },
            ),
            ServerReceiveLoopGateRejection::Acceptance(rejected) => (
                rejected.source,
                ServerRejectionHandoffReason::Acceptance {
                    message_type: rejected.message_type,
                    client_id: rejected.client_id,
                    reason: rejected.reason,
                },
            ),
        };

        ServerRejectionDropLogInput {
            drop_input: ServerPacketDropInput {
                source,
                reason: reason.clone(),
            },
            log_input: ServerPacketLogInput { source, reason },
        }
    }
}

/// Typed handoff to future packet drop and receive log layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRejectionDropLogInput {
    pub drop_input: ServerPacketDropInput,
    pub log_input: ServerPacketLogInput,
}

/// Input for a future packet drop layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPacketDropInput {
    pub source: PacketSource,
    pub reason: ServerRejectionHandoffReason,
}

/// Input for a future receive log layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPacketLogInput {
    pub source: PacketSource,
    pub reason: ServerRejectionHandoffReason,
}

pub const SERVER_RECEIVE_REJECTION_JSON_LOG_EVENT_NAME: &str = "server.receive_rejection";

/// Boundary that maps receive rejection log handoff input to JSON Lines event input.
///
/// This prepares typed event fields for a future JSON Lines writer. It does not
/// serialize JSON, write files, emit logs, or execute packet drops.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveRejectionJsonLogEventBoundary;

impl ServerReceiveRejectionJsonLogEventBoundary {
    pub fn build_event(
        &self,
        input: ServerPacketLogInput,
        timestamp: TimestampMicros,
    ) -> ServerReceiveRejectionJsonLogEventInput {
        let ServerPacketLogInput { source, reason } = input;
        let (client_id, message_type, rejection_reason, detail) = match reason {
            ServerRejectionHandoffReason::Decode { action, error } => (
                None,
                None,
                ServerReceiveRejectionReason::DecodeError,
                ServerReceiveRejectionDetail::Decode { action, error },
            ),
            ServerRejectionHandoffReason::Acceptance {
                message_type,
                client_id,
                reason,
            } => {
                let rejection_reason = match &reason {
                    PacketAcceptanceRejectReason::UnauthenticatedSource => {
                        ServerReceiveRejectionReason::UnauthenticatedSource
                    }
                    PacketAcceptanceRejectReason::UnknownClient => {
                        ServerReceiveRejectionReason::UnknownClient
                    }
                    PacketAcceptanceRejectReason::EndpointMismatch => {
                        ServerReceiveRejectionReason::EndpointMismatch
                    }
                };

                (
                    client_id,
                    Some(message_type),
                    rejection_reason,
                    ServerReceiveRejectionDetail::Acceptance { reason },
                )
            }
        };

        ServerReceiveRejectionJsonLogEventInput {
            event_name: SERVER_RECEIVE_REJECTION_JSON_LOG_EVENT_NAME,
            run_id: None,
            client_id,
            source,
            message_type,
            rejection_reason,
            detail,
            timestamp,
        }
    }
}

/// JSON Lines event input for receive rejections.
///
/// `run_id`, `client_id`, and `message_type` stay optional because decode
/// failures and early gate rejections may happen before those fields are known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReceiveRejectionJsonLogEventInput {
    pub event_name: &'static str,
    pub run_id: Option<RunId>,
    pub client_id: Option<ClientId>,
    pub source: PacketSource,
    pub message_type: Option<MessageType>,
    pub rejection_reason: ServerReceiveRejectionReason,
    pub detail: ServerReceiveRejectionDetail,
    pub timestamp: TimestampMicros,
}

/// Minimal receive rejection log output boundary.
///
/// This connects the existing rejection handoff and event schema to an
/// `io::Write` sink. It writes one JSON Lines record and does not own files,
/// rotation, async I/O, buffering policy, or global logging configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveRejectionLogOutputBoundary {
    handoff: ServerRejectionDropLogHandoffBoundary,
    event: ServerReceiveRejectionJsonLogEventBoundary,
    writer: ServerReceiveRejectionJsonLineWriter,
}

impl ServerReceiveRejectionLogOutputBoundary {
    pub fn write_rejection<W: io::Write>(
        &self,
        rejection: ServerReceiveLoopGateRejection,
        timestamp: TimestampMicros,
        writer: W,
    ) -> io::Result<ServerReceiveRejectionJsonLogEventInput> {
        let handoff = self.handoff.handoff(rejection);
        let event = self.event.build_event(handoff.log_input, timestamp);
        self.writer.write_event(&event, writer)?;
        Ok(event)
    }
}

/// Minimal JSON Lines writer for receive rejection events.
///
/// This is intentionally small and schema-specific. A broader JSON Lines writer
/// can replace this boundary later without changing the receive rejection event
/// schema.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveRejectionJsonLineWriter;

impl ServerReceiveRejectionJsonLineWriter {
    pub fn write_event<W: io::Write>(
        &self,
        event: &ServerReceiveRejectionJsonLogEventInput,
        mut writer: W,
    ) -> io::Result<()> {
        write!(writer, "{{")?;
        write_json_field(&mut writer, "event_name", event.event_name)?;
        write!(writer, ",")?;
        write_optional_json_field(
            &mut writer,
            "run_id",
            event.run_id.as_ref().map(|run_id| run_id.0.as_str()),
        )?;
        write!(writer, ",")?;
        write_optional_json_field(
            &mut writer,
            "client_id",
            event
                .client_id
                .as_ref()
                .map(|client_id| client_id.0.as_str()),
        )?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "source", &event.source.address.to_string())?;
        write!(writer, ",")?;
        write_optional_json_field(
            &mut writer,
            "message_type",
            event
                .message_type
                .as_ref()
                .map(|message_type| receive_rejection_message_type_name(*message_type)),
        )?;
        write!(writer, ",")?;
        write_json_field(
            &mut writer,
            "rejection_reason",
            receive_rejection_reason_name(event.rejection_reason),
        )?;
        write!(writer, ",\"detail\":")?;
        write_receive_rejection_detail(&mut writer, &event.detail)?;
        write!(writer, ",\"timestamp\":{}", event.timestamp.0)?;
        writeln!(writer, "}}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerReceiveRejectionReason {
    DecodeError,
    UnauthenticatedSource,
    UnknownClient,
    EndpointMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveRejectionDetail {
    Decode {
        action: ServerDecodeErrorAction,
        error: ProtocolError,
    },
    Acceptance {
        reason: PacketAcceptanceRejectReason,
    },
}

fn write_json_field<W: io::Write>(writer: &mut W, key: &str, value: &str) -> io::Result<()> {
    write!(writer, "\"{key}\":")?;
    write_json_string(writer, value)
}

fn write_optional_json_field<W: io::Write>(
    writer: &mut W,
    key: &str,
    value: Option<&str>,
) -> io::Result<()> {
    write!(writer, "\"{key}\":")?;
    match value {
        Some(value) => write_json_string(writer, value),
        None => write!(writer, "null"),
    }
}

fn write_receive_rejection_detail<W: io::Write>(
    writer: &mut W,
    detail: &ServerReceiveRejectionDetail,
) -> io::Result<()> {
    match detail {
        ServerReceiveRejectionDetail::Decode { action, error } => {
            write!(writer, "{{")?;
            write_json_field(writer, "kind", "Decode")?;
            write!(writer, ",")?;
            write_json_field(writer, "action", &format!("{action:?}"))?;
            write!(writer, ",")?;
            write_json_field(writer, "error", &format!("{error:?}"))?;
            write!(writer, "}}")
        }
        ServerReceiveRejectionDetail::Acceptance { reason } => {
            write!(writer, "{{")?;
            write_json_field(writer, "kind", "Acceptance")?;
            write!(writer, ",")?;
            write_json_field(
                writer,
                "reason",
                packet_acceptance_reject_reason_name(*reason),
            )?;
            write!(writer, "}}")
        }
    }
}

fn write_json_string<W: io::Write>(writer: &mut W, value: &str) -> io::Result<()> {
    write!(writer, "\"")?;
    for character in value.chars() {
        match character {
            '"' => write!(writer, "\\\"")?,
            '\\' => write!(writer, "\\\\")?,
            '\n' => write!(writer, "\\n")?,
            '\r' => write!(writer, "\\r")?,
            '\t' => write!(writer, "\\t")?,
            character if character.is_control() => write!(writer, "\\u{:04x}", character as u32)?,
            character => write!(writer, "{character}")?,
        }
    }
    write!(writer, "\"")
}

fn receive_rejection_message_type_name(message_type: MessageType) -> &'static str {
    match message_type {
        MessageType::AuthRequest => "AuthRequest",
        MessageType::AuthResponse => "AuthResponse",
        MessageType::Heartbeat => "Heartbeat",
        MessageType::HeartbeatAck => "HeartbeatAck",
        MessageType::VideoFrame => "VideoFrame",
        MessageType::ClientStats => "ClientStats",
        MessageType::ServerNotice => "ServerNotice",
    }
}

fn receive_rejection_reason_name(reason: ServerReceiveRejectionReason) -> &'static str {
    match reason {
        ServerReceiveRejectionReason::DecodeError => "DecodeError",
        ServerReceiveRejectionReason::UnauthenticatedSource => "UnauthenticatedSource",
        ServerReceiveRejectionReason::UnknownClient => "UnknownClient",
        ServerReceiveRejectionReason::EndpointMismatch => "EndpointMismatch",
    }
}

fn packet_acceptance_reject_reason_name(reason: PacketAcceptanceRejectReason) -> &'static str {
    match reason {
        PacketAcceptanceRejectReason::UnauthenticatedSource => "UnauthenticatedSource",
        PacketAcceptanceRejectReason::UnknownClient => "UnknownClient",
        PacketAcceptanceRejectReason::EndpointMismatch => "EndpointMismatch",
    }
}

/// Receive rejection reason preserved across drop and log handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerRejectionHandoffReason {
    Decode {
        action: ServerDecodeErrorAction,
        error: ProtocolError,
    },
    Acceptance {
        message_type: MessageType,
        client_id: Option<ClientId>,
        reason: PacketAcceptanceRejectReason,
    },
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

/// Boundary that combines decoded auth input with configured auth policy input.
///
/// This placeholder does not load config, verify tokens, perform whitelist
/// lookup, or return an allow/reject decision. It only prepares the full input
/// shape for future auth decision code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthConfigInputBoundary;

impl ServerAuthConfigInputBoundary {
    pub fn prepare_check_input(
        &self,
        check: ServerAuthCheck,
        config: &ServerAuthConfig,
    ) -> ServerAuthCheckInput {
        ServerAuthCheckInput {
            check,
            allowed_clients: config
                .allowed_clients
                .iter()
                .map(|client| ServerAllowedClientAuthInput {
                    client_id: ClientId(client.client_id.clone()),
                    shared_token_id: client.shared_token_id.clone(),
                })
                .collect(),
            shared_tokens: config
                .shared_tokens
                .iter()
                .map(|token| ServerSharedTokenAuthInput {
                    token_id: token.token_id.clone(),
                    secret_ref: token.secret_ref.clone(),
                })
                .collect(),
        }
    }
}

/// Complete input for future auth decision logic.
///
/// The decoded request and configured whitelist/token references are present in
/// one value, but no auth judgement is made here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthCheckInput {
    pub check: ServerAuthCheck,
    pub allowed_clients: Vec<ServerAllowedClientAuthInput>,
    pub shared_tokens: Vec<ServerSharedTokenAuthInput>,
}

impl ServerAuthCheckInput {
    pub fn presented_shared_token(&self) -> &str {
        self.check.shared_token()
    }

    pub fn requested_client_id(&self) -> &ClientId {
        self.check.client_id()
    }
}

/// Auth decision input after configured token references have been resolved.
///
/// This keeps decoded request context and whitelist entries unchanged, but
/// replaces token references with redacted-debug material prepared by the
/// secret resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerResolvedAuthCheckInput {
    pub check: ServerAuthCheck,
    pub allowed_clients: Vec<ServerAllowedClientAuthInput>,
    pub shared_tokens: Vec<ServerResolvedSharedTokenAuthInput>,
}

impl ServerResolvedAuthCheckInput {
    pub fn presented_shared_token(&self) -> &str {
        self.check.shared_token()
    }

    pub fn requested_client_id(&self) -> &ClientId {
        self.check.client_id()
    }
}

/// Whitelisted client entry prepared for auth checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAllowedClientAuthInput {
    pub client_id: ClientId,
    pub shared_token_id: String,
}

/// Token reference prepared for future verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSharedTokenAuthInput {
    pub token_id: String,
    pub secret_ref: SharedTokenSecretRef,
}

impl ServerSharedTokenAuthInput {
    pub fn secret_resolution_status(&self) -> ServerSharedTokenSecretResolutionStatus {
        match &self.secret_ref {
            SharedTokenSecretRef::InlinePlaceholder(_) => {
                ServerSharedTokenSecretResolutionStatus::InlinePlaceholderAvailable
            }
            SharedTokenSecretRef::EnvironmentVariable(name) => {
                ServerSharedTokenSecretResolutionStatus::EnvironmentVariablePending {
                    name: name.clone(),
                }
            }
        }
    }
}

/// Placeholder status for the future secret resolution boundary.
///
/// This type classifies whether auth decision input already carries PoC inline
/// token material or only a reference that must be resolved before production
/// verification. It does not read environment variables or secret stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerSharedTokenSecretResolutionStatus {
    InlinePlaceholderAvailable,
    EnvironmentVariablePending { name: String },
}

/// Minimal boundary for server secret resolution.
///
/// This resolves PoC inline token material and `shared_token_env` references
/// into redacted-debug token material for auth decision input. It does not
/// connect to secret stores, compare tokens, or log token material.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSecretResolverBoundary;

impl ServerSecretResolverBoundary {
    pub fn resolve_auth_input(
        &self,
        input: ServerAuthCheckInput,
    ) -> Result<ServerResolvedAuthCheckInput, ServerSecretResolutionError> {
        let shared_tokens = input
            .shared_tokens
            .iter()
            .map(|token| self.resolve_token(token))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ServerResolvedAuthCheckInput {
            check: input.check,
            allowed_clients: input.allowed_clients,
            shared_tokens,
        })
    }

    pub fn resolve_token(
        &self,
        token: &ServerSharedTokenAuthInput,
    ) -> Result<ServerResolvedSharedTokenAuthInput, ServerSecretResolutionError> {
        match &token.secret_ref {
            SharedTokenSecretRef::InlinePlaceholder(value) => {
                Ok(ServerResolvedSharedTokenAuthInput {
                    token_id: token.token_id.clone(),
                    material: ServerResolvedSharedTokenMaterial::PoCInline(value.clone()),
                })
            }
            SharedTokenSecretRef::EnvironmentVariable(name) => match env::var(name) {
                Ok(value) if value.trim().is_empty() => {
                    Err(ServerSecretResolutionError::EmptyEnvironmentVariable {
                        token_id: token.token_id.clone(),
                        name: name.clone(),
                    })
                }
                Ok(value) => Ok(ServerResolvedSharedTokenAuthInput {
                    token_id: token.token_id.clone(),
                    material: ServerResolvedSharedTokenMaterial::EnvironmentVariable(value),
                }),
                Err(env::VarError::NotPresent) => {
                    Err(ServerSecretResolutionError::MissingEnvironmentVariable {
                        token_id: token.token_id.clone(),
                        name: name.clone(),
                    })
                }
                Err(env::VarError::NotUnicode(_)) => {
                    Err(ServerSecretResolutionError::InvalidEnvironmentVariable {
                        token_id: token.token_id.clone(),
                        name: name.clone(),
                    })
                }
            },
        }
    }

    pub fn plan_resolution(
        &self,
        token: &ServerSharedTokenAuthInput,
    ) -> ServerSecretResolutionPlan {
        match &token.secret_ref {
            SharedTokenSecretRef::InlinePlaceholder(value) => {
                ServerSecretResolutionPlan::AlreadyResolved(ServerResolvedSharedTokenAuthInput {
                    token_id: token.token_id.clone(),
                    material: ServerResolvedSharedTokenMaterial::PoCInline(value.clone()),
                })
            }
            SharedTokenSecretRef::EnvironmentVariable(name) => {
                ServerSecretResolutionPlan::NeedsEnvironmentVariable {
                    token_id: token.token_id.clone(),
                    name: name.clone(),
                }
            }
        }
    }
}

/// Typed failure from resolving configured shared token references.
///
/// Token values are never carried in these errors. Environment variable names
/// are configuration references, not secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerSecretResolutionError {
    MissingEnvironmentVariable { token_id: String, name: String },
    EmptyEnvironmentVariable { token_id: String, name: String },
    InvalidEnvironmentVariable { token_id: String, name: String },
}

/// Resolution plan for one configured server shared token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerSecretResolutionPlan {
    AlreadyResolved(ServerResolvedSharedTokenAuthInput),
    NeedsEnvironmentVariable { token_id: String, name: String },
}

/// Token material prepared for future auth decision input.
///
/// The value is intentionally debug-redacted because it may contain shared
/// token material after the real resolver is implemented.
#[derive(Clone, PartialEq, Eq)]
pub struct ServerResolvedSharedTokenAuthInput {
    pub token_id: String,
    pub material: ServerResolvedSharedTokenMaterial,
}

impl std::fmt::Debug for ServerResolvedSharedTokenAuthInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServerResolvedSharedTokenAuthInput")
            .field("token_id", &self.token_id)
            .field("material", &self.material)
            .finish()
    }
}

/// Resolved token material. Debug output must never reveal the material value.
#[derive(Clone, PartialEq, Eq)]
pub enum ServerResolvedSharedTokenMaterial {
    PoCInline(String),
    EnvironmentVariable(String),
}

impl ServerResolvedSharedTokenMaterial {
    fn as_str(&self) -> &str {
        match self {
            Self::PoCInline(value) | Self::EnvironmentVariable(value) => value,
        }
    }
}

impl std::fmt::Debug for ServerResolvedSharedTokenMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PoCInline(_) => formatter
                .debug_tuple("PoCInline")
                .field(&"<redacted>")
                .finish(),
            Self::EnvironmentVariable(_) => formatter
                .debug_tuple("EnvironmentVariable")
                .field(&"<redacted>")
                .finish(),
        }
    }
}

/// Minimal server auth decision boundary.
///
/// This checks the prepared client whitelist and token input and returns a
/// `ServerAuthDecision`. It does not read TOML, resolve external secrets,
/// register authenticated sources, or send responses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthDecisionBoundary;

impl ServerAuthDecisionBoundary {
    pub fn decide_resolved(&self, input: ServerResolvedAuthCheckInput) -> ServerAuthDecision {
        let source = input.check.source;
        let client_id = input.check.request.client_id.clone();
        let run_id = input.check.request.run_id.clone();
        let protocol_version = input.check.request.protocol_version;
        let app_version = input.check.request.app_version.clone();

        let Some(allowed_client) = input
            .allowed_clients
            .iter()
            .find(|client| client.client_id == client_id)
        else {
            return ServerAuthDecision::rejected(
                source,
                client_id,
                run_id,
                protocol_version,
                AuthResponseReasonCode::UnknownClient,
                Some("unknown client_id".to_string()),
                None,
            )
            .with_app_version(app_version);
        };

        let Some(shared_token) = input
            .shared_tokens
            .iter()
            .find(|token| token.token_id == allowed_client.shared_token_id)
        else {
            return ServerAuthDecision::rejected(
                source,
                client_id,
                run_id,
                protocol_version,
                AuthResponseReasonCode::InternalError,
                Some("configured token reference was not found".to_string()),
                None,
            )
            .with_app_version(app_version);
        };

        if input.presented_shared_token() == shared_token.material.as_str() {
            ServerAuthDecision::accepted(source, client_id, run_id, protocol_version, None)
                .with_app_version(app_version)
        } else {
            ServerAuthDecision::rejected(
                source,
                client_id,
                run_id,
                protocol_version,
                AuthResponseReasonCode::InvalidToken,
                Some("invalid shared_token".to_string()),
                None,
            )
            .with_app_version(app_version)
        }
    }

    pub fn reject_secret_resolution_error(
        &self,
        check: &ServerAuthCheck,
        error: &ServerSecretResolutionError,
    ) -> ServerAuthDecision {
        let message = match error {
            ServerSecretResolutionError::MissingEnvironmentVariable { .. } => {
                "token secret environment variable is missing"
            }
            ServerSecretResolutionError::EmptyEnvironmentVariable { .. } => {
                "token secret environment variable is empty"
            }
            ServerSecretResolutionError::InvalidEnvironmentVariable { .. } => {
                "token secret environment variable is invalid"
            }
        };

        ServerAuthDecision::rejected(
            check.source,
            check.request.client_id.clone(),
            check.request.run_id.clone(),
            check.request.protocol_version,
            AuthResponseReasonCode::InternalError,
            Some(message.to_string()),
            None,
        )
        .with_app_version(check.request.app_version.clone())
    }

    pub fn decide(&self, input: ServerAuthCheckInput) -> ServerAuthDecision {
        let source = input.check.source;
        let client_id = input.check.request.client_id.clone();
        let run_id = input.check.request.run_id.clone();
        let protocol_version = input.check.request.protocol_version;
        let app_version = input.check.request.app_version.clone();

        let Some(allowed_client) = input
            .allowed_clients
            .iter()
            .find(|client| client.client_id == client_id)
        else {
            return ServerAuthDecision::rejected(
                source,
                client_id,
                run_id,
                protocol_version,
                AuthResponseReasonCode::UnknownClient,
                Some("unknown client_id".to_string()),
                None,
            )
            .with_app_version(app_version);
        };

        let Some(shared_token) = input
            .shared_tokens
            .iter()
            .find(|token| token.token_id == allowed_client.shared_token_id)
        else {
            return ServerAuthDecision::rejected(
                source,
                client_id,
                run_id,
                protocol_version,
                AuthResponseReasonCode::InternalError,
                Some("configured token reference was not found".to_string()),
                None,
            )
            .with_app_version(app_version);
        };

        match &shared_token.secret_ref {
            SharedTokenSecretRef::InlinePlaceholder(expected_token) => {
                if input.presented_shared_token() == expected_token {
                    ServerAuthDecision::accepted(source, client_id, run_id, protocol_version, None)
                        .with_app_version(app_version)
                } else {
                    ServerAuthDecision::rejected(
                        source,
                        client_id,
                        run_id,
                        protocol_version,
                        AuthResponseReasonCode::InvalidToken,
                        Some("invalid shared_token".to_string()),
                        None,
                    )
                    .with_app_version(app_version)
                }
            }
            SharedTokenSecretRef::EnvironmentVariable(_) => ServerAuthDecision::rejected(
                source,
                client_id,
                run_id,
                protocol_version,
                AuthResponseReasonCode::InternalError,
                Some("token secret is not resolved".to_string()),
                None,
            )
            .with_app_version(app_version),
        }
    }
}

/// Minimal server auth flow step from decoded auth route to outbound queue item.
///
/// This connects existing boundaries only. It does not load real config,
/// register authenticated sources, run a queue, encode bytes, or send UDP
/// packets. Secret resolution is limited to PoC inline material and
/// `shared_token_env` environment variable lookup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthFlowStep {
    auth_handler: ServerAuthHandlerBoundary,
    config_input: ServerAuthConfigInputBoundary,
    secret_resolver: ServerSecretResolverBoundary,
    decision: ServerAuthDecisionBoundary,
    auth_log: ServerAuthLogHandoffBoundary,
    sender_registry: AuthenticatedSenderRegistryBoundary,
    response: ServerAuthResponseBoundary,
    queue: ServerOutboundQueueBoundary,
}

impl ServerAuthFlowStep {
    pub fn handle_auth_route(
        &self,
        route: ServerInboundRoute,
        config: &ServerAuthConfig,
    ) -> Result<ServerAuthFlowOutcome, ServerAuthBoundaryError> {
        let check = self.auth_handler.prepare_from_route(route)?;
        Ok(self.handle_auth_check(check, config))
    }

    pub fn handle_auth_check(
        &self,
        check: ServerAuthCheck,
        config: &ServerAuthConfig,
    ) -> ServerAuthFlowOutcome {
        let auth_input = self.config_input.prepare_check_input(check, config);
        let decision = match self.secret_resolver.resolve_auth_input(auth_input.clone()) {
            Ok(resolved_auth_input) => self.decision.decide_resolved(resolved_auth_input),
            Err(error) => self
                .decision
                .reject_secret_resolution_error(&auth_input.check, &error),
        };
        let auth_log_input = self.auth_log.handoff(&decision);
        let registry_registration = self.sender_registry.registration_from_decision(&decision);
        let outbound_response = self.response.build_for_send(decision.clone());
        let queue_item = self.queue.handoff_auth_response(outbound_response.clone());

        ServerAuthFlowOutcome {
            decision,
            auth_log_input,
            registry_registration,
            outbound_response,
            queue_item,
        }
    }
}

/// Result of connecting auth decision to the outbound queue handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthFlowOutcome {
    pub decision: ServerAuthDecision,
    pub auth_log_input: ServerAuthLogInput,
    pub registry_registration: Option<AuthenticatedSenderRegistration>,
    pub outbound_response: ServerOutboundAuthResponse,
    pub queue_item: OutboundQueueItem,
}

/// In-memory authenticated sender registry boundary.
///
/// This maps an accepted `client_id` to the source endpoint observed during
/// auth. It does not persist state, manage timeout, revoke entries, or perform
/// reauthentication policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthenticatedSenderRegistry {
    entries_by_client_id: BTreeMap<String, AuthenticatedSenderEntry>,
}

impl AuthenticatedSenderRegistry {
    pub fn entries(&self) -> impl Iterator<Item = &AuthenticatedSenderEntry> {
        self.entries_by_client_id.values()
    }

    pub fn get(&self, client_id: &ClientId) -> Option<&AuthenticatedSenderEntry> {
        self.entries_by_client_id.get(client_id.0.as_str())
    }

    pub fn contains_source(&self, source: PacketSource) -> bool {
        self.entries_by_client_id
            .values()
            .any(|entry| entry.source == source)
    }
}

/// One accepted client to source-endpoint binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSenderEntry {
    pub client_id: ClientId,
    pub source: PacketSource,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub registered_at: Option<TimestampMicros>,
}

/// Registration input produced from an accepted auth decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSenderRegistration {
    pub client_id: ClientId,
    pub source: PacketSource,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub registered_at: Option<TimestampMicros>,
}

impl AuthenticatedSenderRegistration {
    fn into_entry(self) -> AuthenticatedSenderEntry {
        AuthenticatedSenderEntry {
            client_id: self.client_id,
            source: self.source,
            run_id: self.run_id,
            protocol_version: self.protocol_version,
            registered_at: self.registered_at,
        }
    }
}

/// Result of checking whether a decoded packet belongs to an authenticated
/// sender binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticatedSenderCheck {
    Accepted(AuthenticatedSenderEntry),
    Rejected(AuthenticatedSenderRejectReason),
}

/// Minimal reject reasons for authenticated sender lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthenticatedSenderRejectReason {
    UnknownClient,
    EndpointMismatch,
}

/// Boundary that registers accepted auth decisions and checks later packets.
///
/// This boundary owns only the accepted-client to endpoint mapping shape. It
/// does not run UDP receive loops, persist state, enforce timeout/revocation, or
/// execute reauthentication.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct AuthenticatedSenderRegistryBoundary;

impl AuthenticatedSenderRegistryBoundary {
    pub fn registration_from_decision(
        &self,
        decision: &ServerAuthDecision,
    ) -> Option<AuthenticatedSenderRegistration> {
        decision.accepted.then(|| AuthenticatedSenderRegistration {
            client_id: decision.client_id.clone(),
            source: decision.source,
            run_id: decision.run_id.clone(),
            protocol_version: decision.protocol_version,
            registered_at: decision.server_time,
        })
    }

    pub fn register(
        &self,
        registry: &mut AuthenticatedSenderRegistry,
        registration: AuthenticatedSenderRegistration,
    ) -> AuthenticatedSenderEntry {
        let entry = registration.into_entry();
        registry
            .entries_by_client_id
            .insert(entry.client_id.0.clone(), entry.clone());
        entry
    }

    pub fn check_source(
        &self,
        registry: &AuthenticatedSenderRegistry,
        client_id: &ClientId,
        source: PacketSource,
    ) -> AuthenticatedSenderCheck {
        let Some(entry) = registry.get(client_id) else {
            return AuthenticatedSenderCheck::Rejected(
                AuthenticatedSenderRejectReason::UnknownClient,
            );
        };

        if entry.source != source {
            return AuthenticatedSenderCheck::Rejected(
                AuthenticatedSenderRejectReason::EndpointMismatch,
            );
        }

        AuthenticatedSenderCheck::Accepted(entry.clone())
    }
}

/// Boundary decision for allowing a decoded route to reach its handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketAcceptanceDecision {
    Accepted,
    Rejected(PacketAcceptanceRejection),
}

/// Rejection data produced before handler execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketAcceptanceRejection {
    pub source: PacketSource,
    pub message_type: MessageType,
    pub client_id: Option<ClientId>,
    pub reason: PacketAcceptanceRejectReason,
}

/// Minimal authenticated packet rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketAcceptanceRejectReason {
    UnauthenticatedSource,
    UnknownClient,
    EndpointMismatch,
}

/// Gate between server routing and later packet handlers.
///
/// This consults the authenticated sender registry for client-scoped packets.
/// It does not drop packets, emit logs, update timeout state, or run heartbeat /
/// video frame handlers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PacketAcceptanceGateBoundary {
    registry: AuthenticatedSenderRegistryBoundary,
}

impl PacketAcceptanceGateBoundary {
    pub fn evaluate_route(
        &self,
        registry: &AuthenticatedSenderRegistry,
        route: &ServerInboundRoute,
    ) -> PacketAcceptanceDecision {
        match route {
            ServerInboundRoute::AuthRequest { .. } => PacketAcceptanceDecision::Accepted,
            ServerInboundRoute::Heartbeat { source, heartbeat } => self.evaluate_client_packet(
                registry,
                *source,
                &heartbeat.client_id,
                MessageType::Heartbeat,
            ),
            ServerInboundRoute::VideoFrame { source, frame } => self.evaluate_client_packet(
                registry,
                *source,
                &frame.client_id,
                MessageType::VideoFrame,
            ),
            ServerInboundRoute::UnsupportedForServer { .. } => PacketAcceptanceDecision::Accepted,
        }
    }

    fn evaluate_client_packet(
        &self,
        registry: &AuthenticatedSenderRegistry,
        source: PacketSource,
        client_id: &ClientId,
        message_type: MessageType,
    ) -> PacketAcceptanceDecision {
        match self.registry.check_source(registry, client_id, source) {
            AuthenticatedSenderCheck::Accepted(_) => PacketAcceptanceDecision::Accepted,
            AuthenticatedSenderCheck::Rejected(
                AuthenticatedSenderRejectReason::EndpointMismatch,
            ) => self.reject(
                source,
                message_type,
                Some(client_id.clone()),
                PacketAcceptanceRejectReason::EndpointMismatch,
            ),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient) => {
                let reason = if registry.contains_source(source) {
                    PacketAcceptanceRejectReason::UnknownClient
                } else {
                    PacketAcceptanceRejectReason::UnauthenticatedSource
                };
                self.reject(source, message_type, Some(client_id.clone()), reason)
            }
        }
    }

    fn reject(
        &self,
        source: PacketSource,
        message_type: MessageType,
        client_id: Option<ClientId>,
        reason: PacketAcceptanceRejectReason,
    ) -> PacketAcceptanceDecision {
        PacketAcceptanceDecision::Rejected(PacketAcceptanceRejection {
            source,
            message_type,
            client_id,
            reason,
        })
    }
}

/// Registered client packet ready to be handed to a server handler.
///
/// This is produced only after decode, route classification, packet acceptance,
/// and authenticated sender registry lookup. It does not run heartbeat or video
/// frame business logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerRegisteredClientPacket {
    Heartbeat(ServerRegisteredHeartbeatPacket),
    VideoFrame(ServerRegisteredVideoFramePacket),
}

/// Handler input for an accepted heartbeat packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRegisteredHeartbeatPacket {
    pub source: PacketSource,
    pub authenticated_sender: AuthenticatedSenderEntry,
    pub heartbeat: Heartbeat,
}

/// Handler input for an accepted video frame packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRegisteredVideoFramePacket {
    pub source: PacketSource,
    pub authenticated_sender: AuthenticatedSenderEntry,
    pub frame: VideoFrame,
}

/// Error from converting an accepted route into registered handler input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerRegisteredPacketBoundaryError {
    NotClientScoped { message_type: MessageType },
    NotAccepted(PacketAcceptanceRejection),
}

/// Bridge from accepted receive routes to future heartbeat / video handlers.
///
/// This boundary preserves the authenticated sender binding next to the decoded
/// message so later handlers do not repeat source-authentication policy. It
/// does not update heartbeat state, enqueue video frames, emit logs, drop
/// packets, manage timeout, or perform reauthentication.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerRegisteredPacketBoundary {
    gate: PacketAcceptanceGateBoundary,
    registry: AuthenticatedSenderRegistryBoundary,
}

impl ServerRegisteredPacketBoundary {
    pub fn prepare_for_handler(
        &self,
        registry: &AuthenticatedSenderRegistry,
        route: ServerInboundRoute,
    ) -> Result<ServerRegisteredClientPacket, ServerRegisteredPacketBoundaryError> {
        match self.gate.evaluate_route(registry, &route) {
            PacketAcceptanceDecision::Accepted => self.attach_sender(registry, route),
            PacketAcceptanceDecision::Rejected(rejection) => {
                Err(ServerRegisteredPacketBoundaryError::NotAccepted(rejection))
            }
        }
    }

    fn attach_sender(
        &self,
        registry: &AuthenticatedSenderRegistry,
        route: ServerInboundRoute,
    ) -> Result<ServerRegisteredClientPacket, ServerRegisteredPacketBoundaryError> {
        match route {
            ServerInboundRoute::Heartbeat { source, heartbeat } => {
                let authenticated_sender = self.require_sender(
                    registry,
                    &heartbeat.client_id,
                    source,
                    MessageType::Heartbeat,
                )?;
                Ok(ServerRegisteredClientPacket::Heartbeat(
                    ServerRegisteredHeartbeatPacket {
                        source,
                        authenticated_sender,
                        heartbeat,
                    },
                ))
            }
            ServerInboundRoute::VideoFrame { source, frame } => {
                let authenticated_sender = self.require_sender(
                    registry,
                    &frame.client_id,
                    source,
                    MessageType::VideoFrame,
                )?;
                Ok(ServerRegisteredClientPacket::VideoFrame(
                    ServerRegisteredVideoFramePacket {
                        source,
                        authenticated_sender,
                        frame,
                    },
                ))
            }
            ServerInboundRoute::AuthRequest { .. } => {
                Err(ServerRegisteredPacketBoundaryError::NotClientScoped {
                    message_type: MessageType::AuthRequest,
                })
            }
            ServerInboundRoute::UnsupportedForServer { message_type, .. } => {
                Err(ServerRegisteredPacketBoundaryError::NotClientScoped { message_type })
            }
        }
    }

    fn require_sender(
        &self,
        registry: &AuthenticatedSenderRegistry,
        client_id: &ClientId,
        source: PacketSource,
        message_type: MessageType,
    ) -> Result<AuthenticatedSenderEntry, ServerRegisteredPacketBoundaryError> {
        match self.registry.check_source(registry, client_id, source) {
            AuthenticatedSenderCheck::Accepted(entry) => Ok(entry),
            AuthenticatedSenderCheck::Rejected(
                AuthenticatedSenderRejectReason::EndpointMismatch,
            ) => Err(ServerRegisteredPacketBoundaryError::NotAccepted(
                PacketAcceptanceRejection {
                    source,
                    message_type,
                    client_id: Some(client_id.clone()),
                    reason: PacketAcceptanceRejectReason::EndpointMismatch,
                },
            )),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient) => {
                let reason = if registry.contains_source(source) {
                    PacketAcceptanceRejectReason::UnknownClient
                } else {
                    PacketAcceptanceRejectReason::UnauthenticatedSource
                };
                Err(ServerRegisteredPacketBoundaryError::NotAccepted(
                    PacketAcceptanceRejection {
                        source,
                        message_type,
                        client_id: Some(client_id.clone()),
                        reason,
                    },
                ))
            }
        }
    }
}

/// Errors at the server auth handoff boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerAuthBoundaryError {
    UnexpectedRoute { message_type: MessageType },
}

/// Result produced by server auth decision logic.
///
/// This type carries the decision into the response boundary. It does not
/// perform checks by itself, update connection state, or send sockets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthDecision {
    pub source: PacketSource,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub app_version: Option<AppVersion>,
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
            app_version: None,
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
            app_version: None,
            protocol_version,
            accepted: false,
            reason_code,
            message,
            server_time: None,
            expected_protocol_version,
        }
    }

    pub fn with_app_version(mut self, app_version: AppVersion) -> Self {
        self.app_version = Some(app_version);
        self
    }
}

/// Boundary that prepares auth decisions for a future log layer.
///
/// This keeps success/failure context together and does not emit JSON Lines,
/// update metrics, mutate auth state, or call UDP sockets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthLogHandoffBoundary;

impl ServerAuthLogHandoffBoundary {
    pub fn handoff(&self, decision: &ServerAuthDecision) -> ServerAuthLogInput {
        ServerAuthLogInput {
            source: decision.source,
            client_id: decision.client_id.clone(),
            run_id: decision.run_id.clone(),
            app_version: decision.app_version.clone(),
            protocol_version: decision.protocol_version,
            outcome: if decision.accepted {
                ServerAuthLogOutcome::Success
            } else {
                ServerAuthLogOutcome::Failure
            },
            reason_code: decision.reason_code,
            message: decision.message.clone(),
            server_time: decision.server_time,
            expected_protocol_version: decision.expected_protocol_version,
        }
    }
}

/// Typed input for future auth success / failure logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthLogInput {
    pub source: PacketSource,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub app_version: Option<AppVersion>,
    pub protocol_version: ProtocolVersion,
    pub outcome: ServerAuthLogOutcome,
    pub reason_code: AuthResponseReasonCode,
    pub message: Option<String>,
    pub server_time: Option<TimestampMicros>,
    pub expected_protocol_version: Option<ProtocolVersion>,
}

/// Auth result category for future log layer input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerAuthLogOutcome {
    Success,
    Failure,
}

/// Event name used by the future JSON Lines auth result log.
pub const SERVER_AUTH_JSON_LOG_EVENT_NAME: &str = "server.auth_result";

/// Boundary that maps typed auth log handoff input to the future JSON Lines
/// event schema input.
///
/// This does not serialize JSON, write files, update metrics, or decide auth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthJsonLogEventBoundary;

impl ServerAuthJsonLogEventBoundary {
    pub fn build_event(
        &self,
        input: ServerAuthLogInput,
        timestamp: TimestampMicros,
    ) -> ServerAuthJsonLogEventInput {
        ServerAuthJsonLogEventInput {
            event_name: SERVER_AUTH_JSON_LOG_EVENT_NAME,
            run_id: input.run_id,
            client_id: input.client_id,
            source: input.source,
            accepted: matches!(input.outcome, ServerAuthLogOutcome::Success),
            reason_code: input.reason_code,
            message: input.message,
            app_version: input.app_version,
            protocol_version: input.protocol_version,
            timestamp,
            expected_protocol_version: input.expected_protocol_version,
        }
    }
}

/// Input shape for the future auth result JSON Lines event writer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthJsonLogEventInput {
    pub event_name: &'static str,
    pub run_id: RunId,
    pub client_id: ClientId,
    pub source: PacketSource,
    pub accepted: bool,
    pub reason_code: AuthResponseReasonCode,
    pub message: Option<String>,
    pub app_version: Option<AppVersion>,
    pub protocol_version: ProtocolVersion,
    pub timestamp: TimestampMicros,
    pub expected_protocol_version: Option<ProtocolVersion>,
}

/// Minimal auth result log output boundary.
///
/// This connects the existing auth log handoff and event schema to an
/// `io::Write` sink. It writes one JSON Lines record and does not own files,
/// rotation, async I/O, buffering policy, metrics, or global logging config.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthLogOutputBoundary {
    event: ServerAuthJsonLogEventBoundary,
    writer: ServerAuthJsonLineWriter,
}

impl ServerAuthLogOutputBoundary {
    pub fn write_auth_result<W: io::Write>(
        &self,
        input: ServerAuthLogInput,
        timestamp: TimestampMicros,
        writer: W,
    ) -> io::Result<ServerAuthJsonLogEventInput> {
        let event = self.event.build_event(input, timestamp);
        self.writer.write_event(&event, writer)?;
        Ok(event)
    }
}

/// Minimal JSON Lines writer for auth result events.
///
/// This is schema-specific and intentionally separate from receive rejection
/// output. A future shared writer can replace it while keeping the auth event
/// schema stable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthJsonLineWriter;

impl ServerAuthJsonLineWriter {
    pub fn write_event<W: io::Write>(
        &self,
        event: &ServerAuthJsonLogEventInput,
        mut writer: W,
    ) -> io::Result<()> {
        write!(writer, "{{")?;
        write_json_field(&mut writer, "event_name", event.event_name)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "run_id", &event.run_id.0)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "client_id", &event.client_id.0)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "source", &event.source.address.to_string())?;
        write!(writer, ",\"accepted\":{}", event.accepted)?;
        write!(writer, ",")?;
        write_json_field(
            &mut writer,
            "reason_code",
            auth_response_reason_code_name(event.reason_code),
        )?;
        write!(writer, ",")?;
        write_optional_json_field(&mut writer, "message", event.message.as_deref())?;
        write!(writer, ",")?;
        write_optional_json_field(
            &mut writer,
            "app_version",
            event
                .app_version
                .as_ref()
                .map(|app_version| app_version.0.as_str()),
        )?;
        write!(writer, ",\"protocol_version\":{}", event.protocol_version.0)?;
        write!(writer, ",\"timestamp\":{}", event.timestamp.0)?;
        write!(writer, ",\"expected_protocol_version\":")?;
        match event.expected_protocol_version {
            Some(protocol_version) => write!(writer, "{}", protocol_version.0)?,
            None => write!(writer, "null")?,
        }
        writeln!(writer, "}}")
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

/// Server-side timestamps prepared for the minimal heartbeat ack handoff.
///
/// This is explicit input so the handler boundary does not read clocks or
/// calculate RTT / offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatAckTiming {
    pub server_received_at: TimestampMicros,
    pub server_sent_at: TimestampMicros,
}

/// Result of connecting a registered heartbeat packet to ack queue handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatAckHandoff {
    pub registered_packet: ServerRegisteredHeartbeatPacket,
    pub ack_input: ServerHeartbeatAckInput,
    pub outbound_ack: ServerOutboundHeartbeatAck,
    pub queue_item: OutboundQueueItem,
}

/// Minimal heartbeat handler bridge for producing a HeartbeatAck queue item.
///
/// This consumes an already registered heartbeat packet, echoes `sent_at`, and
/// hands a typed `HeartbeatAck` to the outbound queue boundary. It does not
/// update heartbeat state, calculate RTT / offset, read clocks, manage timeout,
/// encode bytes, or send UDP packets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatHandlerBoundary {
    ack: ServerHeartbeatAckBoundary,
    queue: ServerOutboundQueueBoundary,
}

impl ServerHeartbeatHandlerBoundary {
    pub fn handoff_ack(
        &self,
        packet: ServerRegisteredHeartbeatPacket,
        timing: ServerHeartbeatAckTiming,
    ) -> ServerHeartbeatAckHandoff {
        let ack_input = ServerHeartbeatAckInput {
            destination: packet.source,
            protocol_version: packet.heartbeat.protocol_version,
            client_id: packet.heartbeat.client_id.clone(),
            run_id: packet.heartbeat.run_id.clone(),
            echoed_sent_at: packet.heartbeat.sent_at,
            server_received_at: timing.server_received_at,
            server_sent_at: timing.server_sent_at,
        };
        let outbound_ack = self.ack.build_for_send(ack_input.clone());
        let queue_item = self.queue.handoff_heartbeat_ack(outbound_ack.clone());

        ServerHeartbeatAckHandoff {
            registered_packet: packet,
            ack_input,
            outbound_ack,
            queue_item,
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

fn auth_response_reason_code_name(reason_code: AuthResponseReasonCode) -> &'static str {
    match reason_code {
        AuthResponseReasonCode::Ok => "Ok",
        AuthResponseReasonCode::InvalidToken => "InvalidToken",
        AuthResponseReasonCode::UnknownClient => "UnknownClient",
        AuthResponseReasonCode::ProtocolMismatch => "ProtocolMismatch",
        AuthResponseReasonCode::AlreadyConnected => "AlreadyConnected",
        AuthResponseReasonCode::InternalError => "InternalError",
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
    use std::net::{SocketAddr, UdpSocket};
    use stream_sync_config::{AllowedClientConfig, SharedTokenConfig};
    use stream_sync_protocol::{
        decode_fixed_header, encode_auth_response_payload, AppVersion, ClientId, Codec,
        ProtocolVersion, RunId, TimestampMicros, FIXED_HEADER_LEN, HEADER_FLAGS_OFFSET,
        HEADER_LENGTH_OFFSET, HEADER_MESSAGE_TYPE_OFFSET, HEADER_PAYLOAD_LENGTH_OFFSET,
        HEADER_PROTOCOL_VERSION_OFFSET, HEADER_RESERVED_OFFSET,
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
    fn receive_loop_with_gate_accepts_registered_heartbeat() {
        let source = packet_source();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let registry = registry_with_client("client-1", source);
        let receive_loop = ServerReceiveLoopStep::default();

        let outcome = receive_loop.handle_received_packet_with_gate(
            ProtocolVersion(2),
            &registry,
            source,
            packet.as_slice(),
        );

        let ServerReceiveLoopGateOutcome::Accepted(ServerInboundRoute::Heartbeat {
            source: routed_source,
            heartbeat,
        }) = outcome
        else {
            panic!("expected accepted heartbeat");
        };
        assert_eq!(routed_source, source);
        assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
        assert_eq!(heartbeat.sent_at, TimestampMicros(1_234_567));
    }

    #[test]
    fn receive_loop_with_gate_rejects_unauthenticated_heartbeat() {
        let source = packet_source();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let registry = AuthenticatedSenderRegistry::default();
        let receive_loop = ServerReceiveLoopStep::default();

        let outcome = receive_loop.handle_received_packet_with_gate(
            ProtocolVersion(2),
            &registry,
            source,
            packet.as_slice(),
        );

        assert_eq!(
            outcome,
            ServerReceiveLoopGateOutcome::Rejected(ServerReceiveLoopGateRejection::Acceptance(
                PacketAcceptanceRejection {
                    source,
                    message_type: MessageType::Heartbeat,
                    client_id: Some(ClientId("client-1".to_string())),
                    reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
                }
            ))
        );
    }

    #[test]
    fn receive_loop_with_gate_keeps_decode_rejection_for_drop_or_log_layer() {
        let source = packet_source();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 1, &payload);
        let registry = AuthenticatedSenderRegistry::default();
        let receive_loop = ServerReceiveLoopStep::default();

        let outcome = receive_loop.handle_received_packet_with_gate(
            ProtocolVersion(2),
            &registry,
            source,
            packet.as_slice(),
        );

        assert_eq!(
            outcome,
            ServerReceiveLoopGateOutcome::Rejected(ServerReceiveLoopGateRejection::Decode(
                ServerRejectedPacket {
                    source,
                    action: ServerDecodeErrorAction::RejectProtocolVersion,
                    error: ProtocolError::UnsupportedProtocolVersion {
                        expected: ProtocolVersion(2),
                        actual: ProtocolVersion(1)
                    }
                }
            ))
        );
    }

    #[test]
    fn udp_socket_step_receives_packet_and_calls_receive_loop_gate() {
        let step = ServerUdpSocketIoStep::default();
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        let receiver_addr = receiver.local_addr().expect("receiver should have address");
        let sender_source: PacketSource = sender
            .local_addr()
            .expect("sender should have address")
            .into();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let registry = registry_with_client("client-1", sender_source);

        sender
            .send_to(packet.as_slice(), receiver_addr)
            .expect("packet should send");

        let mut buffer = [0_u8; 128];
        let outcome = step
            .receive_one_with_gate(&receiver, &mut buffer, ProtocolVersion(2), &registry)
            .expect("packet should receive");

        let ServerReceiveLoopGateOutcome::Accepted(ServerInboundRoute::Heartbeat {
            source,
            heartbeat,
        }) = outcome
        else {
            panic!("expected accepted heartbeat");
        };
        assert_eq!(source, sender_source);
        assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
    }

    #[test]
    fn udp_socket_step_sends_encoded_packet() {
        let step = ServerUdpSocketIoStep::default();
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let receiver_addr = receiver.local_addr().expect("receiver should have address");
        let packet = EncodedOutboundPacket {
            destination: receiver_addr.into(),
            bytes: vec![0x44, 0x55, 0x66],
        };

        let sent = step
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
    fn auth_response_poc_receives_auth_request_and_sends_auth_response() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        let auth_packet = auth_request_packet("client-1", "presented-secret");
        client_socket
            .send_to(auth_packet.as_slice(), server_address)
            .expect("auth request should send");
        let mut receive_buffer = vec![0_u8; 1024];
        let mut registry = AuthenticatedSenderRegistry::default();
        let config = auth_config(Some("presented-secret"));
        let step = ServerAuthResponsePocStep::default();

        let outcome = step
            .run_one(
                &server_socket,
                &mut receive_buffer,
                ProtocolVersion(2),
                &config,
                &mut registry,
            )
            .expect("auth response PoC step should complete");

        assert!(outcome.auth_flow.decision.accepted);
        assert_eq!(
            outcome.auth_flow.decision.reason_code,
            AuthResponseReasonCode::Ok
        );
        assert!(outcome.registered_sender.is_some());
        assert!(registry.get(&ClientId("client-1".to_string())).is_some());
        assert_eq!(outcome.bytes_sent, outcome.encoded_packet.bytes.len());

        let mut response_buffer = vec![0_u8; 1024];
        let (response_len, _source) = client_socket
            .recv_from(&mut response_buffer)
            .expect("auth response should receive");
        let decoded = decode_fixed_header(&response_buffer[..response_len])
            .expect("auth response fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::AuthResponse);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));

        let auth_response = outcome
            .auth_flow
            .outbound_response
            .auth_response()
            .expect("outbound response should carry AuthResponse");
        let mut expected_payload = Vec::new();
        encode_auth_response_payload(auth_response, &mut expected_payload)
            .expect("expected auth response payload should encode");
        assert_eq!(decoded.header.payload_length, expected_payload.len() as u32);
        assert_eq!(decoded.payload, expected_payload.as_slice());
    }

    #[test]
    fn auth_response_poc_launcher_loads_startup_config_from_example() {
        let launcher = ServerAuthResponsePocLauncher::default();
        let startup_config = launcher
            .load_startup_config_from_str(include_str!(
                "../../../configs/examples/server.example.toml"
            ))
            .expect("example config should load");

        assert_eq!(
            startup_config.bind_address,
            "0.0.0.0:5000"
                .parse::<SocketAddr>()
                .expect("address should parse")
        );
        assert_eq!(startup_config.expected_protocol_version, ProtocolVersion(1));
        assert_eq!(startup_config.auth_config.allowed_clients.len(), 4);
        assert_eq!(startup_config.auth_config.shared_tokens.len(), 4);
    }

    #[test]
    fn auth_response_poc_launcher_requires_bind_port() {
        let launcher = ServerAuthResponsePocLauncher::default();
        let result = launcher.load_startup_config_from_str(
            r#"
[server]
bind_host = "127.0.0.1"

[session]
protocol_version = 2

[auth]
enabled = true

[auth.clients.client-1]
shared_token = "secret"
"#,
        );

        assert_eq!(
            result,
            Err(ServerAuthResponsePocStartupError::Config(
                ServerAuthResponsePocConfigError::MissingField {
                    section: "server",
                    key: "bind_port",
                }
            ))
        );
    }

    #[test]
    fn rejection_handoff_preserves_decode_error_for_drop_and_log_layers() {
        let source = packet_source();
        let boundary = ServerRejectionDropLogHandoffBoundary;
        let rejection = ServerReceiveLoopGateRejection::Decode(ServerRejectedPacket {
            source,
            action: ServerDecodeErrorAction::RejectProtocolVersion,
            error: ProtocolError::UnsupportedProtocolVersion {
                expected: ProtocolVersion(2),
                actual: ProtocolVersion(1),
            },
        });

        let input = boundary.handoff(rejection);
        let reason = ServerRejectionHandoffReason::Decode {
            action: ServerDecodeErrorAction::RejectProtocolVersion,
            error: ProtocolError::UnsupportedProtocolVersion {
                expected: ProtocolVersion(2),
                actual: ProtocolVersion(1),
            },
        };

        assert_eq!(
            input,
            ServerRejectionDropLogInput {
                drop_input: ServerPacketDropInput {
                    source,
                    reason: reason.clone(),
                },
                log_input: ServerPacketLogInput { source, reason },
            }
        );
    }

    #[test]
    fn rejection_handoff_preserves_unauthenticated_source_for_drop_and_log_layers() {
        let source = packet_source();
        let boundary = ServerRejectionDropLogHandoffBoundary;
        let rejection = ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
            source,
            message_type: MessageType::Heartbeat,
            client_id: Some(ClientId("client-1".to_string())),
            reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
        });

        let input = boundary.handoff(rejection);
        let reason = ServerRejectionHandoffReason::Acceptance {
            message_type: MessageType::Heartbeat,
            client_id: Some(ClientId("client-1".to_string())),
            reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
        };

        assert_eq!(
            input,
            ServerRejectionDropLogInput {
                drop_input: ServerPacketDropInput {
                    source,
                    reason: reason.clone(),
                },
                log_input: ServerPacketLogInput { source, reason },
            }
        );
    }

    #[test]
    fn rejection_handoff_preserves_unknown_client_and_endpoint_mismatch_reasons() {
        let source = packet_source();
        let boundary = ServerRejectionDropLogHandoffBoundary;

        let unknown_client = boundary.handoff(ServerReceiveLoopGateRejection::Acceptance(
            PacketAcceptanceRejection {
                source,
                message_type: MessageType::Heartbeat,
                client_id: Some(ClientId("client-2".to_string())),
                reason: PacketAcceptanceRejectReason::UnknownClient,
            },
        ));
        assert_eq!(
            unknown_client.drop_input.reason,
            ServerRejectionHandoffReason::Acceptance {
                message_type: MessageType::Heartbeat,
                client_id: Some(ClientId("client-2".to_string())),
                reason: PacketAcceptanceRejectReason::UnknownClient,
            }
        );
        assert_eq!(
            unknown_client.log_input.reason,
            unknown_client.drop_input.reason
        );

        let endpoint_mismatch = boundary.handoff(ServerReceiveLoopGateRejection::Acceptance(
            PacketAcceptanceRejection {
                source,
                message_type: MessageType::VideoFrame,
                client_id: Some(ClientId("client-1".to_string())),
                reason: PacketAcceptanceRejectReason::EndpointMismatch,
            },
        ));
        assert_eq!(
            endpoint_mismatch.drop_input.reason,
            ServerRejectionHandoffReason::Acceptance {
                message_type: MessageType::VideoFrame,
                client_id: Some(ClientId("client-1".to_string())),
                reason: PacketAcceptanceRejectReason::EndpointMismatch,
            }
        );
        assert_eq!(
            endpoint_mismatch.log_input.reason,
            endpoint_mismatch.drop_input.reason
        );
    }

    #[test]
    fn receive_rejection_json_log_event_boundary_builds_decode_event_input() {
        let source = packet_source();
        let boundary = ServerReceiveRejectionJsonLogEventBoundary;

        let event = boundary.build_event(
            ServerPacketLogInput {
                source,
                reason: ServerRejectionHandoffReason::Decode {
                    action: ServerDecodeErrorAction::RejectProtocolVersion,
                    error: ProtocolError::UnsupportedProtocolVersion {
                        expected: ProtocolVersion(2),
                        actual: ProtocolVersion(1),
                    },
                },
            },
            TimestampMicros(123_456),
        );

        assert_eq!(
            event,
            ServerReceiveRejectionJsonLogEventInput {
                event_name: SERVER_RECEIVE_REJECTION_JSON_LOG_EVENT_NAME,
                run_id: None,
                client_id: None,
                source,
                message_type: None,
                rejection_reason: ServerReceiveRejectionReason::DecodeError,
                detail: ServerReceiveRejectionDetail::Decode {
                    action: ServerDecodeErrorAction::RejectProtocolVersion,
                    error: ProtocolError::UnsupportedProtocolVersion {
                        expected: ProtocolVersion(2),
                        actual: ProtocolVersion(1),
                    },
                },
                timestamp: TimestampMicros(123_456),
            }
        );
    }

    #[test]
    fn receive_rejection_json_log_event_boundary_preserves_acceptance_reason() {
        let source = packet_source();
        let boundary = ServerReceiveRejectionJsonLogEventBoundary;

        let event = boundary.build_event(
            ServerPacketLogInput {
                source,
                reason: ServerRejectionHandoffReason::Acceptance {
                    message_type: MessageType::VideoFrame,
                    client_id: Some(ClientId("client-1".to_string())),
                    reason: PacketAcceptanceRejectReason::EndpointMismatch,
                },
            },
            TimestampMicros(234_567),
        );

        assert_eq!(
            event,
            ServerReceiveRejectionJsonLogEventInput {
                event_name: SERVER_RECEIVE_REJECTION_JSON_LOG_EVENT_NAME,
                run_id: None,
                client_id: Some(ClientId("client-1".to_string())),
                source,
                message_type: Some(MessageType::VideoFrame),
                rejection_reason: ServerReceiveRejectionReason::EndpointMismatch,
                detail: ServerReceiveRejectionDetail::Acceptance {
                    reason: PacketAcceptanceRejectReason::EndpointMismatch,
                },
                timestamp: TimestampMicros(234_567),
            }
        );
    }

    #[test]
    fn receive_rejection_json_line_writer_outputs_minimal_json_line() {
        let source = packet_source();
        let writer = ServerReceiveRejectionJsonLineWriter;
        let event = ServerReceiveRejectionJsonLogEventInput {
            event_name: SERVER_RECEIVE_REJECTION_JSON_LOG_EVENT_NAME,
            run_id: None,
            client_id: Some(ClientId("client-1".to_string())),
            source,
            message_type: Some(MessageType::Heartbeat),
            rejection_reason: ServerReceiveRejectionReason::UnauthenticatedSource,
            detail: ServerReceiveRejectionDetail::Acceptance {
                reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
            },
            timestamp: TimestampMicros(345_678),
        };
        let mut output = Vec::new();

        writer
            .write_event(&event, &mut output)
            .expect("json line should write");

        assert_eq!(
            String::from_utf8(output).expect("json line should be utf8"),
            r#"{"event_name":"server.receive_rejection","run_id":null,"client_id":"client-1","source":"127.0.0.1:5000","message_type":"Heartbeat","rejection_reason":"UnauthenticatedSource","detail":{"kind":"Acceptance","reason":"UnauthenticatedSource"},"timestamp":345678}"#
                .to_string()
                + "\n"
        );
    }

    #[test]
    fn receive_rejection_log_output_boundary_connects_handoff_event_and_writer() {
        let source = packet_source();
        let boundary = ServerReceiveRejectionLogOutputBoundary::default();
        let rejection = ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
            source,
            message_type: MessageType::VideoFrame,
            client_id: Some(ClientId("client-1".to_string())),
            reason: PacketAcceptanceRejectReason::EndpointMismatch,
        });
        let mut output = Vec::new();

        let event = boundary
            .write_rejection(rejection, TimestampMicros(456_789), &mut output)
            .expect("json line should write");

        assert_eq!(
            event.rejection_reason,
            ServerReceiveRejectionReason::EndpointMismatch
        );
        let output = String::from_utf8(output).expect("json line should be utf8");
        assert!(output.contains(r#""event_name":"server.receive_rejection""#));
        assert!(output.contains(r#""message_type":"VideoFrame""#));
        assert!(output.contains(r#""rejection_reason":"EndpointMismatch""#));
        assert!(output.contains(r#""timestamp":456789"#));
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
    fn auth_config_input_boundary_combines_request_and_config_without_deciding() {
        let source = packet_source();
        let check = ServerAuthCheck {
            source,
            request: AuthRequest {
                message_type: MessageType::AuthRequest,
                protocol_version: ProtocolVersion(2),
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                app_version: AppVersion("0.1.0".to_string()),
                shared_token: "presented-secret".to_string(),
                display_name: None,
                capabilities: Vec::new(),
                requested_video_profile: None,
            },
        };
        let config = ServerAuthConfig {
            allowed_clients: vec![AllowedClientConfig {
                client_id: "client-1".to_string(),
                shared_token_id: "token-main".to_string(),
            }],
            shared_tokens: vec![SharedTokenConfig {
                token_id: "token-main".to_string(),
                secret_ref: SharedTokenSecretRef::EnvironmentVariable(
                    "STREAM_SYNC_TOKEN_MAIN".to_string(),
                ),
            }],
        };
        let boundary = ServerAuthConfigInputBoundary;

        let input = boundary.prepare_check_input(check, &config);

        assert_eq!(
            input.requested_client_id(),
            &ClientId("client-1".to_string())
        );
        assert_eq!(input.presented_shared_token(), "presented-secret");
        assert_eq!(
            input.allowed_clients,
            vec![ServerAllowedClientAuthInput {
                client_id: ClientId("client-1".to_string()),
                shared_token_id: "token-main".to_string(),
            }]
        );
        assert_eq!(
            input.shared_tokens,
            vec![ServerSharedTokenAuthInput {
                token_id: "token-main".to_string(),
                secret_ref: SharedTokenSecretRef::EnvironmentVariable(
                    "STREAM_SYNC_TOKEN_MAIN".to_string(),
                ),
            }]
        );
    }

    #[test]
    fn auth_decision_accepts_known_client_with_matching_inline_token() {
        let input = auth_check_input("client-1", "presented-secret", Some("presented-secret"));
        let boundary = ServerAuthDecisionBoundary;

        let decision = boundary.decide(input);

        assert!(decision.accepted);
        assert_eq!(decision.reason_code, AuthResponseReasonCode::Ok);
        assert_eq!(decision.client_id, ClientId("client-1".to_string()));
        assert_eq!(decision.run_id, RunId("run-1".to_string()));
        assert_eq!(decision.app_version, Some(AppVersion("0.1.0".to_string())));
        assert_eq!(decision.server_time, None);
    }

    #[test]
    fn auth_decision_rejects_unknown_client() {
        let input = auth_check_input("client-2", "presented-secret", Some("presented-secret"));
        let boundary = ServerAuthDecisionBoundary;

        let decision = boundary.decide(input);

        assert!(!decision.accepted);
        assert_eq!(decision.reason_code, AuthResponseReasonCode::UnknownClient);
        assert_eq!(decision.app_version, Some(AppVersion("0.1.0".to_string())));
        assert_eq!(decision.message.as_deref(), Some("unknown client_id"));
    }

    #[test]
    fn auth_decision_rejects_invalid_token() {
        let input = auth_check_input("client-1", "wrong-secret", Some("presented-secret"));
        let boundary = ServerAuthDecisionBoundary;

        let decision = boundary.decide(input);

        assert!(!decision.accepted);
        assert_eq!(decision.reason_code, AuthResponseReasonCode::InvalidToken);
        assert_eq!(decision.app_version, Some(AppVersion("0.1.0".to_string())));
        assert_eq!(decision.message.as_deref(), Some("invalid shared_token"));
    }

    #[test]
    fn auth_log_handoff_preserves_success_context() {
        let decision = ServerAuthDecision::accepted(
            packet_source(),
            ClientId("client-1".to_string()),
            RunId("run-1".to_string()),
            ProtocolVersion(2),
            Some(TimestampMicros(2_000_000)),
        )
        .with_app_version(AppVersion("0.1.0".to_string()));
        let boundary = ServerAuthLogHandoffBoundary;

        let input = boundary.handoff(&decision);

        assert_eq!(
            input,
            ServerAuthLogInput {
                source: packet_source(),
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                app_version: Some(AppVersion("0.1.0".to_string())),
                protocol_version: ProtocolVersion(2),
                outcome: ServerAuthLogOutcome::Success,
                reason_code: AuthResponseReasonCode::Ok,
                message: None,
                server_time: Some(TimestampMicros(2_000_000)),
                expected_protocol_version: None,
            }
        );
    }

    #[test]
    fn auth_log_handoff_preserves_failure_reason_and_context() {
        let decision = ServerAuthDecision::rejected(
            packet_source(),
            ClientId("client-1".to_string()),
            RunId("run-1".to_string()),
            ProtocolVersion(2),
            AuthResponseReasonCode::InvalidToken,
            Some("invalid shared_token".to_string()),
            None,
        )
        .with_app_version(AppVersion("0.1.0".to_string()));
        let boundary = ServerAuthLogHandoffBoundary;

        let input = boundary.handoff(&decision);

        assert_eq!(input.source, packet_source());
        assert_eq!(input.client_id, ClientId("client-1".to_string()));
        assert_eq!(input.run_id, RunId("run-1".to_string()));
        assert_eq!(input.app_version, Some(AppVersion("0.1.0".to_string())));
        assert_eq!(input.protocol_version, ProtocolVersion(2));
        assert_eq!(input.outcome, ServerAuthLogOutcome::Failure);
        assert_eq!(input.reason_code, AuthResponseReasonCode::InvalidToken);
        assert_eq!(input.message.as_deref(), Some("invalid shared_token"));
    }

    #[test]
    fn auth_json_log_event_boundary_builds_success_event_schema_input() {
        let input = ServerAuthLogInput {
            source: packet_source(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: Some(AppVersion("0.1.0".to_string())),
            protocol_version: ProtocolVersion(2),
            outcome: ServerAuthLogOutcome::Success,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time: Some(TimestampMicros(2_000_000)),
            expected_protocol_version: None,
        };
        let boundary = ServerAuthJsonLogEventBoundary;

        let event = boundary.build_event(input, TimestampMicros(2_000_100));

        assert_eq!(event.event_name, SERVER_AUTH_JSON_LOG_EVENT_NAME);
        assert_eq!(event.run_id, RunId("run-1".to_string()));
        assert_eq!(event.client_id, ClientId("client-1".to_string()));
        assert_eq!(event.source, packet_source());
        assert!(event.accepted);
        assert_eq!(event.reason_code, AuthResponseReasonCode::Ok);
        assert_eq!(event.message, None);
        assert_eq!(event.app_version, Some(AppVersion("0.1.0".to_string())));
        assert_eq!(event.protocol_version, ProtocolVersion(2));
        assert_eq!(event.timestamp, TimestampMicros(2_000_100));
        assert_eq!(event.expected_protocol_version, None);
    }

    #[test]
    fn auth_json_log_event_boundary_preserves_failure_fields() {
        let input = ServerAuthLogInput {
            source: packet_source(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: Some(AppVersion("0.1.0".to_string())),
            protocol_version: ProtocolVersion(1),
            outcome: ServerAuthLogOutcome::Failure,
            reason_code: AuthResponseReasonCode::ProtocolMismatch,
            message: Some("unsupported protocol_version".to_string()),
            server_time: None,
            expected_protocol_version: Some(ProtocolVersion(2)),
        };
        let boundary = ServerAuthJsonLogEventBoundary;

        let event = boundary.build_event(input, TimestampMicros(2_000_200));

        assert_eq!(event.event_name, SERVER_AUTH_JSON_LOG_EVENT_NAME);
        assert!(!event.accepted);
        assert_eq!(event.reason_code, AuthResponseReasonCode::ProtocolMismatch);
        assert_eq!(
            event.message.as_deref(),
            Some("unsupported protocol_version")
        );
        assert_eq!(event.protocol_version, ProtocolVersion(1));
        assert_eq!(event.expected_protocol_version, Some(ProtocolVersion(2)));
        assert_eq!(event.timestamp, TimestampMicros(2_000_200));
    }

    #[test]
    fn auth_json_line_writer_outputs_minimal_json_line() {
        let writer = ServerAuthJsonLineWriter;
        let event = ServerAuthJsonLogEventInput {
            event_name: SERVER_AUTH_JSON_LOG_EVENT_NAME,
            run_id: RunId("run-1".to_string()),
            client_id: ClientId("client-1".to_string()),
            source: packet_source(),
            accepted: false,
            reason_code: AuthResponseReasonCode::InvalidToken,
            message: Some("invalid shared_token".to_string()),
            app_version: Some(AppVersion("0.1.0".to_string())),
            protocol_version: ProtocolVersion(1),
            timestamp: TimestampMicros(2_000_300),
            expected_protocol_version: Some(ProtocolVersion(2)),
        };
        let mut output = Vec::new();

        writer
            .write_event(&event, &mut output)
            .expect("json line should write");

        assert_eq!(
            String::from_utf8(output).expect("json line should be utf8"),
            r#"{"event_name":"server.auth_result","run_id":"run-1","client_id":"client-1","source":"127.0.0.1:5000","accepted":false,"reason_code":"InvalidToken","message":"invalid shared_token","app_version":"0.1.0","protocol_version":1,"timestamp":2000300,"expected_protocol_version":2}"#
                .to_string()
                + "\n"
        );
    }

    #[test]
    fn auth_log_output_boundary_connects_event_schema_and_writer() {
        let input = ServerAuthLogInput {
            source: packet_source(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: None,
            protocol_version: ProtocolVersion(1),
            outcome: ServerAuthLogOutcome::Success,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time: None,
            expected_protocol_version: None,
        };
        let boundary = ServerAuthLogOutputBoundary::default();
        let mut output = Vec::new();

        let event = boundary
            .write_auth_result(input, TimestampMicros(2_000_400), &mut output)
            .expect("json line should write");

        assert!(event.accepted);
        let output = String::from_utf8(output).expect("json line should be utf8");
        assert!(output.contains(r#""event_name":"server.auth_result""#));
        assert!(output.contains(r#""accepted":true"#));
        assert!(output.contains(r#""reason_code":"Ok""#));
        assert!(output.contains(r#""timestamp":2000400"#));
    }

    #[test]
    fn auth_decision_rejects_missing_configured_token_as_internal_error() {
        let input = auth_check_input("client-1", "presented-secret", None);
        let boundary = ServerAuthDecisionBoundary;

        let decision = boundary.decide(input);

        assert!(!decision.accepted);
        assert_eq!(decision.reason_code, AuthResponseReasonCode::InternalError);
        assert_eq!(
            decision.message.as_deref(),
            Some("configured token reference was not found")
        );
    }

    #[test]
    fn auth_decision_rejects_unresolved_token_reference_as_internal_error() {
        let mut input = auth_check_input("client-1", "presented-secret", Some("presented-secret"));
        input.shared_tokens[0].secret_ref =
            SharedTokenSecretRef::EnvironmentVariable("STREAM_SYNC_TOKEN_MAIN".to_string());
        let boundary = ServerAuthDecisionBoundary;

        let decision = boundary.decide(input);

        assert!(!decision.accepted);
        assert_eq!(decision.reason_code, AuthResponseReasonCode::InternalError);
        assert_eq!(
            decision.message.as_deref(),
            Some("token secret is not resolved")
        );
    }

    #[test]
    fn secret_resolver_boundary_plans_inline_placeholder_as_resolved() {
        let boundary = ServerSecretResolverBoundary;
        let token = ServerSharedTokenAuthInput {
            token_id: "client-1".to_string(),
            secret_ref: SharedTokenSecretRef::InlinePlaceholder("presented-secret".to_string()),
        };

        let plan = boundary.plan_resolution(&token);

        assert_eq!(
            plan,
            ServerSecretResolutionPlan::AlreadyResolved(ServerResolvedSharedTokenAuthInput {
                token_id: "client-1".to_string(),
                material: ServerResolvedSharedTokenMaterial::PoCInline(
                    "presented-secret".to_string()
                ),
            })
        );
        assert_eq!(
            format!("{plan:?}"),
            "AlreadyResolved(ServerResolvedSharedTokenAuthInput { token_id: \"client-1\", material: PoCInline(\"<redacted>\") })"
        );
    }

    #[test]
    fn secret_resolver_boundary_plans_environment_variable_lookup() {
        let boundary = ServerSecretResolverBoundary;
        let token = ServerSharedTokenAuthInput {
            token_id: "client-1".to_string(),
            secret_ref: SharedTokenSecretRef::EnvironmentVariable(
                "STREAM_SYNC_TOKEN_MAIN".to_string(),
            ),
        };

        let plan = boundary.plan_resolution(&token);

        assert_eq!(
            plan,
            ServerSecretResolutionPlan::NeedsEnvironmentVariable {
                token_id: "client-1".to_string(),
                name: "STREAM_SYNC_TOKEN_MAIN".to_string(),
            }
        );
    }

    #[test]
    fn secret_resolver_boundary_resolves_environment_variable() {
        let variable_name = "STREAM_SYNC_TEST_TOKEN_RESOLVES_ENVIRONMENT_VARIABLE";
        std::env::set_var(variable_name, "presented-secret");
        let boundary = ServerSecretResolverBoundary;
        let token = ServerSharedTokenAuthInput {
            token_id: "client-1".to_string(),
            secret_ref: SharedTokenSecretRef::EnvironmentVariable(variable_name.to_string()),
        };

        let resolved = boundary
            .resolve_token(&token)
            .expect("environment variable token should resolve");

        assert_eq!(
            resolved,
            ServerResolvedSharedTokenAuthInput {
                token_id: "client-1".to_string(),
                material: ServerResolvedSharedTokenMaterial::EnvironmentVariable(
                    "presented-secret".to_string()
                ),
            }
        );
        assert_eq!(
            format!("{resolved:?}"),
            "ServerResolvedSharedTokenAuthInput { token_id: \"client-1\", material: EnvironmentVariable(\"<redacted>\") }"
        );
        std::env::remove_var(variable_name);
    }

    #[test]
    fn secret_resolver_boundary_rejects_missing_environment_variable() {
        let variable_name = "STREAM_SYNC_TEST_TOKEN_MISSING_ENVIRONMENT_VARIABLE";
        std::env::remove_var(variable_name);
        let boundary = ServerSecretResolverBoundary;
        let token = ServerSharedTokenAuthInput {
            token_id: "client-1".to_string(),
            secret_ref: SharedTokenSecretRef::EnvironmentVariable(variable_name.to_string()),
        };

        let result = boundary.resolve_token(&token);

        assert_eq!(
            result,
            Err(ServerSecretResolutionError::MissingEnvironmentVariable {
                token_id: "client-1".to_string(),
                name: variable_name.to_string(),
            })
        );
    }

    #[test]
    fn secret_resolver_boundary_rejects_empty_environment_variable() {
        let variable_name = "STREAM_SYNC_TEST_TOKEN_EMPTY_ENVIRONMENT_VARIABLE";
        std::env::set_var(variable_name, "  ");
        let boundary = ServerSecretResolverBoundary;
        let token = ServerSharedTokenAuthInput {
            token_id: "client-1".to_string(),
            secret_ref: SharedTokenSecretRef::EnvironmentVariable(variable_name.to_string()),
        };

        let result = boundary.resolve_token(&token);

        assert_eq!(
            result,
            Err(ServerSecretResolutionError::EmptyEnvironmentVariable {
                token_id: "client-1".to_string(),
                name: variable_name.to_string(),
            })
        );
        std::env::remove_var(variable_name);
    }

    #[test]
    fn auth_flow_step_accepts_environment_variable_token() {
        let variable_name = "STREAM_SYNC_TEST_TOKEN_AUTH_FLOW_ACCEPTS_ENV";
        std::env::set_var(variable_name, "presented-secret");
        let source = packet_source();
        let route = ServerInboundRoute::AuthRequest {
            source,
            request: auth_request("client-1", "presented-secret"),
        };
        let config = ServerAuthConfig {
            allowed_clients: vec![AllowedClientConfig {
                client_id: "client-1".to_string(),
                shared_token_id: "token-main".to_string(),
            }],
            shared_tokens: vec![SharedTokenConfig {
                token_id: "token-main".to_string(),
                secret_ref: SharedTokenSecretRef::EnvironmentVariable(variable_name.to_string()),
            }],
        };
        let step = ServerAuthFlowStep::default();

        let outcome = step
            .handle_auth_route(route, &config)
            .expect("auth route should be handled");

        assert!(outcome.decision.accepted);
        assert_eq!(outcome.decision.reason_code, AuthResponseReasonCode::Ok);
        std::env::remove_var(variable_name);
    }

    #[test]
    fn auth_flow_step_rejects_missing_environment_variable_token() {
        let variable_name = "STREAM_SYNC_TEST_TOKEN_AUTH_FLOW_MISSING_ENV";
        std::env::remove_var(variable_name);
        let source = packet_source();
        let route = ServerInboundRoute::AuthRequest {
            source,
            request: auth_request("client-1", "presented-secret"),
        };
        let config = ServerAuthConfig {
            allowed_clients: vec![AllowedClientConfig {
                client_id: "client-1".to_string(),
                shared_token_id: "token-main".to_string(),
            }],
            shared_tokens: vec![SharedTokenConfig {
                token_id: "token-main".to_string(),
                secret_ref: SharedTokenSecretRef::EnvironmentVariable(variable_name.to_string()),
            }],
        };
        let step = ServerAuthFlowStep::default();

        let outcome = step
            .handle_auth_route(route, &config)
            .expect("auth route should be handled");

        assert!(!outcome.decision.accepted);
        assert_eq!(
            outcome.decision.reason_code,
            AuthResponseReasonCode::InternalError
        );
        assert_eq!(
            outcome.decision.message.as_deref(),
            Some("token secret environment variable is missing")
        );
    }

    #[test]
    fn auth_flow_step_hands_accepted_decision_to_auth_response_queue() {
        let source = packet_source();
        let route = ServerInboundRoute::AuthRequest {
            source,
            request: auth_request("client-1", "presented-secret"),
        };
        let config = auth_config(Some("presented-secret"));
        let step = ServerAuthFlowStep::default();

        let outcome = step
            .handle_auth_route(route, &config)
            .expect("auth route should be handled");

        assert!(outcome.decision.accepted);
        assert_eq!(outcome.decision.reason_code, AuthResponseReasonCode::Ok);
        assert_eq!(
            outcome.auth_log_input,
            ServerAuthLogInput {
                source,
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                app_version: Some(AppVersion("0.1.0".to_string())),
                protocol_version: ProtocolVersion(2),
                outcome: ServerAuthLogOutcome::Success,
                reason_code: AuthResponseReasonCode::Ok,
                message: None,
                server_time: None,
                expected_protocol_version: None,
            }
        );
        assert_eq!(
            outcome.registry_registration,
            Some(AuthenticatedSenderRegistration {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            })
        );
        let outbound_response = outcome
            .outbound_response
            .auth_response()
            .expect("outbound message should be AuthResponse");
        assert!(outbound_response.accepted);
        assert_eq!(outbound_response.reason_code, AuthResponseReasonCode::Ok);
        let ProtocolMessage::AuthResponse(queued_response) = outcome.queue_item.packet.message
        else {
            panic!("expected queued AuthResponse");
        };
        assert_eq!(
            outcome.queue_item.packet.destination.address,
            source.address
        );
        assert!(queued_response.accepted);
        assert_eq!(queued_response.reason_code, AuthResponseReasonCode::Ok);
    }

    #[test]
    fn auth_flow_step_hands_rejected_decision_to_auth_response_queue() {
        let source = packet_source();
        let route = ServerInboundRoute::AuthRequest {
            source,
            request: auth_request("client-1", "wrong-secret"),
        };
        let config = auth_config(Some("presented-secret"));
        let step = ServerAuthFlowStep::default();

        let outcome = step
            .handle_auth_route(route, &config)
            .expect("auth route should be handled");

        assert!(!outcome.decision.accepted);
        assert_eq!(
            outcome.decision.reason_code,
            AuthResponseReasonCode::InvalidToken
        );
        assert_eq!(
            outcome.auth_log_input,
            ServerAuthLogInput {
                source,
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                app_version: Some(AppVersion("0.1.0".to_string())),
                protocol_version: ProtocolVersion(2),
                outcome: ServerAuthLogOutcome::Failure,
                reason_code: AuthResponseReasonCode::InvalidToken,
                message: Some("invalid shared_token".to_string()),
                server_time: None,
                expected_protocol_version: None,
            }
        );
        assert_eq!(outcome.registry_registration, None);
        let ProtocolMessage::AuthResponse(queued_response) = outcome.queue_item.packet.message
        else {
            panic!("expected queued AuthResponse");
        };
        assert_eq!(
            outcome.queue_item.packet.destination.address,
            source.address
        );
        assert!(!queued_response.accepted);
        assert_eq!(
            queued_response.reason_code,
            AuthResponseReasonCode::InvalidToken
        );
        assert_eq!(
            queued_response.message.as_deref(),
            Some("invalid shared_token")
        );
    }

    #[test]
    fn accepted_auth_flow_registration_updates_registry_for_later_gate() {
        let source = packet_source();
        let route = ServerInboundRoute::AuthRequest {
            source,
            request: auth_request("client-1", "presented-secret"),
        };
        let config = auth_config(Some("presented-secret"));
        let flow_step = ServerAuthFlowStep::default();
        let registry_boundary = AuthenticatedSenderRegistryBoundary;
        let gate = PacketAcceptanceGateBoundary::default();
        let mut registry = AuthenticatedSenderRegistry::default();

        let outcome = flow_step
            .handle_auth_route(route, &config)
            .expect("accepted auth route should be handled");
        let registration = outcome
            .registry_registration
            .expect("accepted auth should produce registry registration");
        let entry = registry_boundary.register(&mut registry, registration);

        assert_eq!(entry.client_id, ClientId("client-1".to_string()));
        assert_eq!(entry.source, source);
        assert_eq!(
            gate.evaluate_route(&registry, &heartbeat_route("client-1", source)),
            PacketAcceptanceDecision::Accepted
        );
    }

    #[test]
    fn auth_flow_step_rejects_non_auth_route_before_decision() {
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
        let step = ServerAuthFlowStep::default();
        let config = auth_config(Some("presented-secret"));

        let result =
            step.handle_auth_route(ServerInboundRoute::Heartbeat { source, heartbeat }, &config);

        assert_eq!(
            result,
            Err(ServerAuthBoundaryError::UnexpectedRoute {
                message_type: MessageType::Heartbeat
            })
        );
    }

    #[test]
    fn authenticated_sender_registry_registers_accepted_decision() {
        let source = packet_source();
        let boundary = AuthenticatedSenderRegistryBoundary;
        let mut registry = AuthenticatedSenderRegistry::default();
        let decision = ServerAuthDecision::accepted(
            source,
            ClientId("client-1".to_string()),
            RunId("run-1".to_string()),
            ProtocolVersion(2),
            Some(TimestampMicros(2_000_000)),
        );
        let registration = boundary
            .registration_from_decision(&decision)
            .expect("accepted decision should produce registration");

        let entry = boundary.register(&mut registry, registration);

        assert_eq!(entry.client_id, ClientId("client-1".to_string()));
        assert_eq!(entry.source, source);
        assert_eq!(entry.run_id, RunId("run-1".to_string()));
        assert_eq!(entry.registered_at, Some(TimestampMicros(2_000_000)));
        assert_eq!(registry.entries().count(), 1);
    }

    #[test]
    fn authenticated_sender_registry_ignores_rejected_decision() {
        let source = packet_source();
        let boundary = AuthenticatedSenderRegistryBoundary;
        let decision = ServerAuthDecision::rejected(
            source,
            ClientId("client-1".to_string()),
            RunId("run-1".to_string()),
            ProtocolVersion(2),
            AuthResponseReasonCode::InvalidToken,
            Some("invalid shared_token".to_string()),
            None,
        );

        let registration = boundary.registration_from_decision(&decision);

        assert_eq!(registration, None);
    }

    #[test]
    fn authenticated_sender_registry_checks_later_packet_source() {
        let source = packet_source();
        let boundary = AuthenticatedSenderRegistryBoundary;
        let mut registry = AuthenticatedSenderRegistry::default();
        let registration = AuthenticatedSenderRegistration {
            client_id: ClientId("client-1".to_string()),
            source,
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            registered_at: None,
        };
        boundary.register(&mut registry, registration.clone());

        let check = boundary.check_source(&registry, &registration.client_id, source);

        assert_eq!(
            check,
            AuthenticatedSenderCheck::Accepted(AuthenticatedSenderEntry {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            })
        );
    }

    #[test]
    fn authenticated_sender_registry_rejects_unknown_or_mismatched_source() {
        let source = packet_source();
        let other_source: PacketSource = "127.0.0.1:5001"
            .parse::<SocketAddr>()
            .expect("source address should parse")
            .into();
        let boundary = AuthenticatedSenderRegistryBoundary;
        let mut registry = AuthenticatedSenderRegistry::default();
        boundary.register(
            &mut registry,
            AuthenticatedSenderRegistration {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            },
        );

        assert_eq!(
            boundary.check_source(&registry, &ClientId("client-2".to_string()), source),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient)
        );
        assert_eq!(
            boundary.check_source(&registry, &ClientId("client-1".to_string()), other_source),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::EndpointMismatch)
        );
    }

    #[test]
    fn packet_acceptance_gate_allows_auth_request_before_registry_lookup() {
        let source = packet_source();
        let gate = PacketAcceptanceGateBoundary::default();
        let registry = AuthenticatedSenderRegistry::default();
        let route = ServerInboundRoute::AuthRequest {
            source,
            request: auth_request("client-1", "presented-secret"),
        };

        let decision = gate.evaluate_route(&registry, &route);

        assert_eq!(decision, PacketAcceptanceDecision::Accepted);
    }

    #[test]
    fn packet_acceptance_gate_accepts_registered_heartbeat() {
        let source = packet_source();
        let gate = PacketAcceptanceGateBoundary::default();
        let registry = registry_with_client("client-1", source);
        let route = heartbeat_route("client-1", source);

        let decision = gate.evaluate_route(&registry, &route);

        assert_eq!(decision, PacketAcceptanceDecision::Accepted);
    }

    #[test]
    fn packet_acceptance_gate_rejects_unauthenticated_source() {
        let source = packet_source();
        let gate = PacketAcceptanceGateBoundary::default();
        let registry = AuthenticatedSenderRegistry::default();
        let route = heartbeat_route("client-1", source);

        let decision = gate.evaluate_route(&registry, &route);

        assert_eq!(
            decision,
            PacketAcceptanceDecision::Rejected(PacketAcceptanceRejection {
                source,
                message_type: MessageType::Heartbeat,
                client_id: Some(ClientId("client-1".to_string())),
                reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
            })
        );
    }

    #[test]
    fn packet_acceptance_gate_rejects_unknown_client_from_authenticated_source() {
        let source = packet_source();
        let gate = PacketAcceptanceGateBoundary::default();
        let registry = registry_with_client("client-1", source);
        let route = heartbeat_route("client-2", source);

        let decision = gate.evaluate_route(&registry, &route);

        assert_eq!(
            decision,
            PacketAcceptanceDecision::Rejected(PacketAcceptanceRejection {
                source,
                message_type: MessageType::Heartbeat,
                client_id: Some(ClientId("client-2".to_string())),
                reason: PacketAcceptanceRejectReason::UnknownClient,
            })
        );
    }

    #[test]
    fn packet_acceptance_gate_rejects_endpoint_mismatch_for_video_frame() {
        let registered_source = packet_source();
        let packet_source: PacketSource = "127.0.0.1:5001"
            .parse::<SocketAddr>()
            .expect("source address should parse")
            .into();
        let gate = PacketAcceptanceGateBoundary::default();
        let registry = registry_with_client("client-1", registered_source);
        let route = video_frame_route("client-1", packet_source);

        let decision = gate.evaluate_route(&registry, &route);

        assert_eq!(
            decision,
            PacketAcceptanceDecision::Rejected(PacketAcceptanceRejection {
                source: packet_source,
                message_type: MessageType::VideoFrame,
                client_id: Some(ClientId("client-1".to_string())),
                reason: PacketAcceptanceRejectReason::EndpointMismatch,
            })
        );
    }

    #[test]
    fn registered_packet_boundary_prepares_heartbeat_handler_input() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let route = heartbeat_route("client-1", source);
        let boundary = ServerRegisteredPacketBoundary::default();

        let packet = boundary
            .prepare_for_handler(&registry, route)
            .expect("registered heartbeat should be prepared for handler");

        let ServerRegisteredClientPacket::Heartbeat(heartbeat) = packet else {
            panic!("expected registered heartbeat packet");
        };
        assert_eq!(heartbeat.source, source);
        assert_eq!(
            heartbeat.authenticated_sender.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(heartbeat.authenticated_sender.source, source);
        assert_eq!(
            heartbeat.heartbeat.client_id,
            ClientId("client-1".to_string())
        );
    }

    #[test]
    fn registered_packet_boundary_prepares_video_frame_handler_input() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let route = video_frame_route("client-1", source);
        let boundary = ServerRegisteredPacketBoundary::default();

        let packet = boundary
            .prepare_for_handler(&registry, route)
            .expect("registered video frame should be prepared for handler");

        let ServerRegisteredClientPacket::VideoFrame(frame) = packet else {
            panic!("expected registered video frame packet");
        };
        assert_eq!(frame.source, source);
        assert_eq!(
            frame.authenticated_sender.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(frame.authenticated_sender.source, source);
        assert_eq!(frame.frame.client_id, ClientId("client-1".to_string()));
    }

    #[test]
    fn registered_packet_boundary_keeps_rejection_for_unregistered_packet() {
        let source = packet_source();
        let registry = AuthenticatedSenderRegistry::default();
        let route = heartbeat_route("client-1", source);
        let boundary = ServerRegisteredPacketBoundary::default();

        let result = boundary.prepare_for_handler(&registry, route);

        assert_eq!(
            result,
            Err(ServerRegisteredPacketBoundaryError::NotAccepted(
                PacketAcceptanceRejection {
                    source,
                    message_type: MessageType::Heartbeat,
                    client_id: Some(ClientId("client-1".to_string())),
                    reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
                }
            ))
        );
    }

    #[test]
    fn registered_packet_boundary_rejects_auth_route_as_not_client_scoped() {
        let source = packet_source();
        let registry = AuthenticatedSenderRegistry::default();
        let route = ServerInboundRoute::AuthRequest {
            source,
            request: auth_request("client-1", "presented-secret"),
        };
        let boundary = ServerRegisteredPacketBoundary::default();

        let result = boundary.prepare_for_handler(&registry, route);

        assert_eq!(
            result,
            Err(ServerRegisteredPacketBoundaryError::NotClientScoped {
                message_type: MessageType::AuthRequest,
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
    fn heartbeat_handler_handoff_builds_ack_queue_item_from_registered_packet() {
        let source = packet_source();
        let registered_packet = ServerRegisteredHeartbeatPacket {
            source,
            authenticated_sender: AuthenticatedSenderEntry {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: Some(TimestampMicros(1_000_000)),
            },
            heartbeat: Heartbeat {
                message_type: MessageType::Heartbeat,
                protocol_version: ProtocolVersion(2),
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                sent_at: TimestampMicros(2_000_000),
                local_time: None,
                short_status: None,
            },
        };
        let timing = ServerHeartbeatAckTiming {
            server_received_at: TimestampMicros(2_000_100),
            server_sent_at: TimestampMicros(2_000_200),
        };
        let handler = ServerHeartbeatHandlerBoundary::default();

        let handoff = handler.handoff_ack(registered_packet.clone(), timing);

        assert_eq!(handoff.registered_packet, registered_packet);
        assert_eq!(handoff.ack_input.destination, source);
        assert_eq!(handoff.ack_input.echoed_sent_at, TimestampMicros(2_000_000));
        assert_eq!(
            handoff.ack_input.server_received_at,
            TimestampMicros(2_000_100)
        );
        assert_eq!(handoff.ack_input.server_sent_at, TimestampMicros(2_000_200));
        let ProtocolMessage::HeartbeatAck(ack) = handoff.queue_item.packet.message else {
            panic!("expected queued HeartbeatAck");
        };
        assert_eq!(
            handoff.queue_item.packet.destination.address,
            source.address
        );
        assert_eq!(ack.client_id, ClientId("client-1".to_string()));
        assert_eq!(ack.run_id, RunId("run-1".to_string()));
        assert_eq!(ack.echoed_sent_at, TimestampMicros(2_000_000));
        assert_eq!(ack.server_received_at, TimestampMicros(2_000_100));
        assert_eq!(ack.server_sent_at, TimestampMicros(2_000_200));
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

    fn auth_check_input(
        request_client_id: &str,
        presented_token: &str,
        configured_token: Option<&str>,
    ) -> ServerAuthCheckInput {
        let check = ServerAuthCheck {
            source: packet_source(),
            request: auth_request(request_client_id, presented_token),
        };

        let config = auth_config(configured_token);

        ServerAuthConfigInputBoundary.prepare_check_input(check, &config)
    }

    fn auth_request(client_id: &str, shared_token: &str) -> AuthRequest {
        AuthRequest {
            message_type: MessageType::AuthRequest,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId(client_id.to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: AppVersion("0.1.0".to_string()),
            shared_token: shared_token.to_string(),
            display_name: None,
            capabilities: Vec::new(),
            requested_video_profile: None,
        }
    }

    fn auth_request_packet(client_id: &str, shared_token: &str) -> Vec<u8> {
        let mut payload = Vec::new();
        push_string(&mut payload, client_id);
        push_string(&mut payload, "run-1");
        push_string(&mut payload, "0.1.0");
        push_string(&mut payload, shared_token);
        payload.push(0);
        test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        )
    }

    fn auth_config(configured_token: Option<&str>) -> ServerAuthConfig {
        ServerAuthConfig {
            allowed_clients: vec![AllowedClientConfig {
                client_id: "client-1".to_string(),
                shared_token_id: "token-main".to_string(),
            }],
            shared_tokens: configured_token
                .map(|token| {
                    vec![SharedTokenConfig {
                        token_id: "token-main".to_string(),
                        secret_ref: SharedTokenSecretRef::InlinePlaceholder(token.to_string()),
                    }]
                })
                .unwrap_or_default(),
        }
    }

    fn registry_with_client(client_id: &str, source: PacketSource) -> AuthenticatedSenderRegistry {
        let mut registry = AuthenticatedSenderRegistry::default();
        AuthenticatedSenderRegistryBoundary.register(
            &mut registry,
            AuthenticatedSenderRegistration {
                client_id: ClientId(client_id.to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            },
        );
        registry
    }

    fn heartbeat_route(client_id: &str, source: PacketSource) -> ServerInboundRoute {
        ServerInboundRoute::Heartbeat {
            source,
            heartbeat: Heartbeat {
                message_type: MessageType::Heartbeat,
                protocol_version: ProtocolVersion(2),
                client_id: ClientId(client_id.to_string()),
                run_id: RunId("run-1".to_string()),
                sent_at: TimestampMicros(1_234_567),
                local_time: None,
                short_status: None,
            },
        }
    }

    fn video_frame_route(client_id: &str, source: PacketSource) -> ServerInboundRoute {
        ServerInboundRoute::VideoFrame {
            source,
            frame: VideoFrame {
                message_type: MessageType::VideoFrame,
                protocol_version: ProtocolVersion(2),
                client_id: ClientId(client_id.to_string()),
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
            },
        }
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
