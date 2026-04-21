use std::{
    fs, io,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    time::Duration,
};

use stream_sync_protocol::{
    decode_fixed_header, decode_payload_by_message_type, validate_protocol_version, AppVersion,
    AuthRequest, AuthResponse, ClientId, DecodeContext, EncodeContext, HeartbeatAck,
    HeartbeatAckObservation, HeartbeatAckObservationBoundary, HeartbeatObservationCarrier,
    HeartbeatObservationCarrierBoundary, MessageEncoder, MessageType, ProtocolError,
    ProtocolMessage, ProtocolMessageEncoderBoundary, ProtocolVersion, RunId, TimestampMicros,
};

const DEFAULT_AUTH_RESPONSE_TIMEOUT_MS: u64 = 5_000;
const UDP_PACKET_BUFFER_LEN: usize = 65_507;

/// One-shot client-side AuthRequest send PoC.
///
/// This boundary loads the minimal client config, builds one `AuthRequest`,
/// encodes it through the protocol encoder, sends one UDP datagram, and waits
/// for one `AuthResponse` datagram on the same socket. It does not run a
/// continuous loop, send heartbeat/video frames, retry, fragment, encrypt, or
/// introduce an async runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientAuthRequestPocLauncher {
    encoder: ProtocolMessageEncoderBoundary,
}

impl ClientAuthRequestPocLauncher {
    pub fn load_startup_config_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ClientAuthRequestPocStartupConfig, ClientAuthRequestPocError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|error| ClientAuthRequestPocError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        self.load_startup_config_from_str(&content)
    }

    pub fn load_startup_config_from_str(
        &self,
        input: &str,
    ) -> Result<ClientAuthRequestPocStartupConfig, ClientAuthRequestPocError> {
        let settings = parse_client_poc_settings(input)?;
        let destination = resolve_destination(&settings.server_host, settings.server_port)?;

        Ok(ClientAuthRequestPocStartupConfig {
            destination,
            response_timeout_ms: u64::from(settings.connect_timeout_ms),
            request: AuthRequest {
                message_type: MessageType::AuthRequest,
                protocol_version: ProtocolVersion(settings.protocol_version),
                client_id: ClientId(settings.client_id),
                run_id: RunId(settings.run_id),
                app_version: AppVersion(settings.app_version),
                shared_token: settings.shared_token,
                display_name: settings.display_name,
                capabilities: Vec::new(),
                requested_video_profile: None,
            },
        })
    }

    pub fn run_once_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ClientAuthRequestPocOutcome, ClientAuthRequestPocError> {
        let startup_config = self.load_startup_config_from_path(path)?;
        self.run_once(startup_config)
    }

    pub fn run_once(
        &self,
        startup_config: ClientAuthRequestPocStartupConfig,
    ) -> Result<ClientAuthRequestPocOutcome, ClientAuthRequestPocError> {
        let mut bytes = Vec::new();
        let context = EncodeContext {
            protocol_version: startup_config.request.protocol_version,
        };
        let message = ProtocolMessage::AuthRequest(startup_config.request.clone());
        self.encoder
            .encode_message(context, &message, &mut bytes)
            .map_err(ClientAuthRequestPocError::Encode)?;

        let socket = UdpSocket::bind(ephemeral_bind_address(startup_config.destination))
            .map_err(|error| ClientAuthRequestPocError::Bind(error.kind()))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(
                startup_config.response_timeout_ms,
            )))
            .map_err(|error| ClientAuthRequestPocError::SetReadTimeout(error.kind()))?;
        let bytes_sent = socket
            .send_to(&bytes, startup_config.destination)
            .map_err(|error| ClientAuthRequestPocError::Send(error.kind()))?;

        let mut response_buffer = vec![0_u8; UDP_PACKET_BUFFER_LEN];
        let (response_len, response_source) = socket
            .recv_from(&mut response_buffer)
            .map_err(|error| ClientAuthRequestPocError::Receive(error.kind()))?;
        let response_bytes = response_buffer[..response_len].to_vec();
        let packet_view =
            decode_fixed_header(&response_bytes).map_err(ClientAuthRequestPocError::Decode)?;
        let decode_context = DecodeContext {
            expected_protocol_version: startup_config.request.protocol_version,
        };
        validate_protocol_version(decode_context, packet_view.header)
            .map_err(ClientAuthRequestPocError::Decode)?;
        let decoded_message =
            decode_payload_by_message_type(decode_context, packet_view.header, packet_view.payload)
                .map_err(ClientAuthRequestPocError::Decode)?;
        let ProtocolMessage::AuthResponse(response) = decoded_message else {
            return Err(ClientAuthRequestPocError::UnexpectedResponseMessage {
                actual: decoded_message.message_type(),
            });
        };

        Ok(ClientAuthRequestPocOutcome {
            destination: startup_config.destination,
            request: startup_config.request,
            encoded_bytes: bytes,
            bytes_sent,
            response_source,
            response_bytes,
            response,
        })
    }
}

/// Convenience entry point for the client binary and manual PoC wiring.
pub fn run_auth_request_poc_once_from_path(
    path: impl AsRef<Path>,
) -> Result<ClientAuthRequestPocOutcome, ClientAuthRequestPocError> {
    ClientAuthRequestPocLauncher::default().run_once_from_path(path)
}

/// Client boundary for observing one received HeartbeatAck.
///
/// This captures the client-side receive timestamp and creates a typed
/// observation for a future client-to-server stats/report path. It does not
/// encode or send that report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatAckObservationBoundary {
    protocol: HeartbeatAckObservationBoundary,
}

impl ClientHeartbeatAckObservationBoundary {
    pub fn observe_ack(
        &self,
        ack: &HeartbeatAck,
        client_received_at: TimestampMicros,
    ) -> HeartbeatAckObservation {
        self.protocol.observe(ack, client_received_at)
    }
}

/// Client boundary for wrapping a heartbeat ack observation in its future carrier.
///
/// This does not send a packet; it only fixes the typed handoff into the
/// `ClientStats` carrier used by the protocol payload encoder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatObservationCarrierBoundary {
    protocol: HeartbeatObservationCarrierBoundary,
}

impl ClientHeartbeatObservationCarrierBoundary {
    pub fn build_client_stats_carrier(
        &self,
        protocol_version: ProtocolVersion,
        observation: HeartbeatAckObservation,
    ) -> HeartbeatObservationCarrier {
        self.protocol
            .build_client_stats_carrier(protocol_version, observation)
    }
}

/// Startup config needed by the one-shot client AuthRequest PoC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAuthRequestPocStartupConfig {
    pub destination: SocketAddr,
    pub response_timeout_ms: u64,
    pub request: AuthRequest,
}

/// Result of one AuthRequest encode/send and AuthResponse receive/decode step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAuthRequestPocOutcome {
    pub destination: SocketAddr,
    pub request: AuthRequest,
    pub encoded_bytes: Vec<u8>,
    pub bytes_sent: usize,
    pub response_source: SocketAddr,
    pub response_bytes: Vec<u8>,
    pub response: AuthResponse,
}

/// Error from the one-shot client AuthRequest PoC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientAuthRequestPocError {
    Io { path: PathBuf, message: String },
    Config(ClientAuthRequestPocConfigError),
    Destination(ClientAuthRequestPocConfigError),
    Encode(ProtocolError),
    Bind(io::ErrorKind),
    SetReadTimeout(io::ErrorKind),
    Send(io::ErrorKind),
    Receive(io::ErrorKind),
    Decode(ProtocolError),
    UnexpectedResponseMessage { actual: MessageType },
}

/// Minimal config parse errors for the one-shot client AuthRequest PoC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientAuthRequestPocConfigError {
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
    InvalidDestination {
        value: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClientPocSettings {
    client_id: String,
    display_name: Option<String>,
    server_host: String,
    server_port: u16,
    shared_token: String,
    run_id: String,
    app_version: String,
    protocol_version: u32,
    connect_timeout_ms: u32,
}

#[derive(Debug, Default)]
struct PartialClientPocSettings {
    client_id: Option<String>,
    display_name: Option<String>,
    server_host: Option<String>,
    server_port: Option<u16>,
    shared_token: Option<String>,
    run_id: Option<String>,
    app_version: Option<String>,
    protocol_version: Option<u32>,
    connect_timeout_ms: Option<u32>,
}

fn parse_client_poc_settings(input: &str) -> Result<ClientPocSettings, ClientAuthRequestPocError> {
    let mut current_section: Option<&str> = None;
    let mut parsed = PartialClientPocSettings::default();

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
                "client" => Some("client"),
                "session" => Some("session"),
                "network" => Some("network"),
                _ => None,
            };
            continue;
        }

        let Some(section) = current_section else {
            continue;
        };
        let Some((key, raw_value)) = line.split_once('=') else {
            return Err(ClientAuthRequestPocError::Config(
                ClientAuthRequestPocConfigError::InvalidTomlLine {
                    line: line_number,
                    message: "expected key = value".to_string(),
                },
            ));
        };
        let key = key.trim();
        let value = raw_value.trim();

        match (section, key) {
            ("client", "client_id") => {
                parsed.client_id = Some(parse_poc_toml_string(value, line_number, key)?);
            }
            ("client", "display_name") => {
                parsed.display_name = Some(parse_poc_toml_string(value, line_number, key)?);
            }
            ("client", "server_host") => {
                parsed.server_host = Some(parse_poc_toml_string(value, line_number, key)?);
            }
            ("client", "server_port") => {
                parsed.server_port = Some(parse_poc_u16(value, line_number, key)?);
            }
            ("client", "shared_token") => {
                parsed.shared_token = Some(parse_poc_toml_string(value, line_number, key)?);
            }
            ("session", "run_id") => {
                parsed.run_id = Some(parse_poc_toml_string(value, line_number, key)?);
            }
            ("session", "app_version") => {
                parsed.app_version = Some(parse_poc_toml_string(value, line_number, key)?);
            }
            ("session", "protocol_version") => {
                parsed.protocol_version = Some(parse_poc_u32(value, line_number, key)?);
            }
            ("network", "connect_timeout_ms") => {
                parsed.connect_timeout_ms = Some(parse_poc_u32(value, line_number, key)?);
            }
            _ => {}
        }
    }

    Ok(ClientPocSettings {
        client_id: require_string(parsed.client_id, "client", "client_id")?,
        display_name: parsed.display_name,
        server_host: require_string(parsed.server_host, "client", "server_host")?,
        server_port: require_u16(parsed.server_port, "client", "server_port")?,
        shared_token: require_string(parsed.shared_token, "client", "shared_token")?,
        run_id: require_string(parsed.run_id, "session", "run_id")?,
        app_version: require_string(parsed.app_version, "session", "app_version")?,
        protocol_version: require_u32(parsed.protocol_version, "session", "protocol_version")?,
        connect_timeout_ms: parsed
            .connect_timeout_ms
            .unwrap_or(DEFAULT_AUTH_RESPONSE_TIMEOUT_MS as u32),
    })
}

fn require_string(
    value: Option<String>,
    section: &'static str,
    key: &'static str,
) -> Result<String, ClientAuthRequestPocError> {
    value.ok_or(ClientAuthRequestPocError::Config(
        ClientAuthRequestPocConfigError::MissingField { section, key },
    ))
}

fn require_u16(
    value: Option<u16>,
    section: &'static str,
    key: &'static str,
) -> Result<u16, ClientAuthRequestPocError> {
    value.ok_or(ClientAuthRequestPocError::Config(
        ClientAuthRequestPocConfigError::MissingField { section, key },
    ))
}

fn require_u32(
    value: Option<u32>,
    section: &'static str,
    key: &'static str,
) -> Result<u32, ClientAuthRequestPocError> {
    value.ok_or(ClientAuthRequestPocError::Config(
        ClientAuthRequestPocConfigError::MissingField { section, key },
    ))
}

fn resolve_destination(host: &str, port: u16) -> Result<SocketAddr, ClientAuthRequestPocError> {
    let value = format!("{host}:{port}");
    (host, port)
        .to_socket_addrs()
        .map_err(|error| {
            ClientAuthRequestPocError::Destination(
                ClientAuthRequestPocConfigError::InvalidDestination {
                    value: value.clone(),
                    message: error.to_string(),
                },
            )
        })?
        .next()
        .ok_or_else(|| {
            ClientAuthRequestPocError::Destination(
                ClientAuthRequestPocConfigError::InvalidDestination {
                    value,
                    message: "address resolved to no socket addresses".to_string(),
                },
            )
        })
}

fn ephemeral_bind_address(destination: SocketAddr) -> SocketAddr {
    if destination.is_ipv6() {
        "[::]:0".parse().expect("valid IPv6 ephemeral bind address")
    } else {
        "0.0.0.0:0"
            .parse()
            .expect("valid IPv4 ephemeral bind address")
    }
}

fn parse_poc_toml_string(
    value: &str,
    line: usize,
    key: &str,
) -> Result<String, ClientAuthRequestPocError> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(ToString::to_string)
        .ok_or_else(|| {
            ClientAuthRequestPocError::Config(ClientAuthRequestPocConfigError::InvalidTomlString {
                line,
                key: key.to_string(),
            })
        })
}

fn parse_poc_u16(value: &str, line: usize, key: &str) -> Result<u16, ClientAuthRequestPocError> {
    value.parse::<u16>().map_err(|_| {
        ClientAuthRequestPocError::Config(ClientAuthRequestPocConfigError::InvalidNumber {
            line,
            key: key.to_string(),
        })
    })
}

fn parse_poc_u32(value: &str, line: usize, key: &str) -> Result<u32, ClientAuthRequestPocError> {
    value.parse::<u32>().map_err(|_| {
        ClientAuthRequestPocError::Config(ClientAuthRequestPocConfigError::InvalidNumber {
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

#[cfg(test)]
mod tests {
    use super::*;
    use stream_sync_protocol::{
        decode_auth_request_payload, decode_fixed_header, AuthResponseReasonCode,
    };

    #[test]
    fn client_auth_request_poc_launcher_loads_example_config() {
        let launcher = ClientAuthRequestPocLauncher::default();
        let config = launcher
            .load_startup_config_from_str(include_str!(
                "../../../configs/examples/client.example.toml"
            ))
            .expect("example config should load");

        assert_eq!(config.destination, "127.0.0.1:5000".parse().unwrap());
        assert_eq!(config.request.message_type, MessageType::AuthRequest);
        assert_eq!(config.request.protocol_version, ProtocolVersion(1));
        assert_eq!(config.request.client_id, ClientId("player1".to_string()));
        assert_eq!(
            config.request.run_id,
            RunId("streamsync-dev-session".to_string())
        );
        assert_eq!(config.request.app_version, AppVersion("0.1.0".to_string()));
        assert_eq!(config.request.shared_token, "replace-with-shared-token");
        assert_eq!(config.request.display_name, Some("Player 1".to_string()));
        assert_eq!(config.response_timeout_ms, 5_000);
    }

    #[test]
    fn client_auth_request_poc_sends_request_and_receives_auth_response_once() {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("read timeout should be set");
        let destination = receiver.local_addr().expect("local addr should exist");
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
        let config = ClientAuthRequestPocStartupConfig {
            destination,
            response_timeout_ms: 1_000,
            request: request.clone(),
        };
        let response = AuthResponse {
            message_type: MessageType::AuthResponse,
            protocol_version: ProtocolVersion(2),
            client_id: request.client_id.clone(),
            run_id: request.run_id.clone(),
            accepted: true,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time: Some(TimestampMicros(2_000_000)),
            expected_protocol_version: None,
        };
        let response_for_thread = response.clone();
        let server = std::thread::spawn(move || {
            let mut buffer = [0_u8; 512];
            let (len, source) = receiver
                .recv_from(&mut buffer)
                .expect("receiver should get one packet");
            let decoded =
                decode_fixed_header(&buffer[..len]).expect("encoded fixed header should decode");
            let decoded_request = decode_auth_request_payload(decoded.header, decoded.payload)
                .expect("encoded auth request should decode");
            assert_eq!(decoded_request, request);

            let mut response_bytes = Vec::new();
            ProtocolMessageEncoderBoundary
                .encode_message(
                    EncodeContext {
                        protocol_version: response_for_thread.protocol_version,
                    },
                    &ProtocolMessage::AuthResponse(response_for_thread),
                    &mut response_bytes,
                )
                .expect("auth response should encode");
            receiver
                .send_to(&response_bytes, source)
                .expect("auth response should send");
            len
        });

        let outcome = ClientAuthRequestPocLauncher::default()
            .run_once(config)
            .expect("auth request should send and receive response");

        let received_request_len = server.join().expect("server thread should finish");
        assert_eq!(outcome.bytes_sent, received_request_len);
        assert_eq!(outcome.response_source, destination);
        assert_eq!(outcome.response, response);

        let decoded = decode_fixed_header(&outcome.encoded_bytes)
            .expect("encoded fixed header should decode");
        let decoded_request = decode_auth_request_payload(decoded.header, decoded.payload)
            .expect("encoded auth request should decode");
        assert_eq!(decoded_request.client_id, ClientId("client-1".to_string()));
    }

    #[test]
    fn client_heartbeat_ack_observation_boundary_captures_receive_time() {
        let ack = HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
        };
        let boundary = ClientHeartbeatAckObservationBoundary::default();

        let observation = boundary.observe_ack(&ack, TimestampMicros(1_150));

        assert_eq!(observation.client_id, ClientId("client-1".to_string()));
        assert_eq!(observation.run_id, RunId("run-1".to_string()));
        assert_eq!(observation.echoed_sent_at, TimestampMicros(1_000));
        assert_eq!(observation.server_received_at, TimestampMicros(2_100));
        assert_eq!(observation.server_sent_at, TimestampMicros(2_150));
        assert_eq!(observation.client_received_at, TimestampMicros(1_150));
    }

    #[test]
    fn client_heartbeat_observation_carrier_boundary_wraps_client_stats_carrier() {
        let observation = HeartbeatAckObservation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
            client_received_at: TimestampMicros(1_150),
        };
        let boundary = ClientHeartbeatObservationCarrierBoundary::default();

        let carrier = boundary.build_client_stats_carrier(ProtocolVersion(2), observation.clone());

        assert_eq!(carrier.message_type, MessageType::ClientStats);
        assert_eq!(carrier.protocol_version, ProtocolVersion(2));
        assert_eq!(carrier.observation, observation);
    }
}
