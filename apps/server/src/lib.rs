use socket2::SockRef;
use std::{
    collections::{BTreeMap, VecDeque},
    env, fs, io,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    time::Duration,
};
use stream_sync_config::{
    ConfigLoadError, SecretStoreSecretRef, ServerAuthConfig, ServerAuthConfigBoundary,
    SharedTokenRotationConfig, SharedTokenRotationMode, SharedTokenSecretRef,
};
use stream_sync_logging::{JsonLinesSinkConfig, JsonLinesSinkPlan, JsonLinesSinkPlanBoundary};
use stream_sync_net_core::{
    DecodedInboundPacket, EncodedOutboundPacket, InboundPacket, InboundPacketDecoder,
    NetDecodeError, NetEncodeError, OutboundPacket, OutboundPacketEncoderBoundary,
    OutboundPacketQueueBoundary, OutboundQueueAdmissionDecision,
    OutboundQueueAdmissionPolicyBoundary, OutboundQueueCapacityPolicy, OutboundQueueItem,
    OutboundQueueItemState, OutboundQueueLifecycleBoundary, OutboundQueueStorageBoundary,
    OutboundQueueStorageDecision, OutboundSendLogContext, OutboundSendLoopEvent,
    OutboundSendLoopTickBoundary, OutboundSendLoopTickPlan, PacketDestination, PacketSource,
    QueuedOutboundItem, SendFailureDisposition, SendFailureKind, SendLogEvent, SendLogStage,
    ServerSwitcherQueuedFrameHandoffErrorCode, ServerSwitcherQueuedFrameHandoffFrame,
    ServerSwitcherQueuedFrameHandoffRequest, ServerSwitcherQueuedFrameHandoffResponse,
    ServerSwitcherQueuedFrameReadMode, UdpSocketIoBoundary, DEFAULT_UDP_PACKET_BUFFER_LEN,
};
use stream_sync_protocol::{
    AppVersion, AuthRequest, AuthResponse, AuthResponseReasonCode, ClientId, ClientStats, Codec,
    EncodeContext, Heartbeat, HeartbeatAck, HeartbeatAckObservation, HeartbeatObservationCarrier,
    MessageType, NoticeType, ProtocolError, ProtocolMessage, ProtocolMessageEncoderBoundary,
    ProtocolVersion, RunId, ServerNotice, TimestampMicros, VideoFrame, VideoFrameFragment,
};
use stream_sync_timebase::{
    HeartbeatExchangeObservation, HeartbeatRttOffsetCalculationError, HeartbeatRttOffsetCalculator,
    HeartbeatRttOffsetEstimate, HeartbeatTimebaseEstimatePlan, HeartbeatTimebasePlanBoundary,
    HeartbeatTimebaseSample,
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

/// Server-side JSON Lines sink config for auth and receive rejection events.
///
/// This is a config/planning boundary only. It does not open files, write
/// records, rotate logs, or install a global logger.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerAuthReceiveJsonLinesSinkConfig {
    pub auth_result: JsonLinesSinkConfig,
    pub receive_rejection: JsonLinesSinkConfig,
}

impl ServerAuthReceiveJsonLinesSinkConfig {
    pub fn stderr_default() -> Self {
        Self {
            auth_result: JsonLinesSinkConfig::stderr(),
            receive_rejection: JsonLinesSinkConfig::stderr(),
        }
    }

    pub fn file_sinks(
        auth_result_path: impl Into<PathBuf>,
        receive_rejection_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            auth_result: JsonLinesSinkConfig::file(auth_result_path),
            receive_rejection: JsonLinesSinkConfig::file(receive_rejection_path),
        }
    }
}

/// Normalized server JSON Lines sink plan for the current auth/receive writers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerAuthReceiveJsonLinesSinkPlan {
    pub auth_result: JsonLinesSinkPlan,
    pub receive_rejection: JsonLinesSinkPlan,
}

/// Boundary that maps server logging config to auth/receive sink plans.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthReceiveJsonLinesSinkBoundary {
    sink_plan: JsonLinesSinkPlanBoundary,
}

impl ServerAuthReceiveJsonLinesSinkBoundary {
    pub fn plan(
        &self,
        config: ServerAuthReceiveJsonLinesSinkConfig,
    ) -> ServerAuthReceiveJsonLinesSinkPlan {
        ServerAuthReceiveJsonLinesSinkPlan {
            auth_result: self.sink_plan.plan(config.auth_result),
            receive_rejection: self.sink_plan.plan(config.receive_rejection),
        }
    }
}

/// Server-side JSON Lines sink config for receive loop operational events.
///
/// This plans where future continuous receive-loop observations should go. It
/// does not open files, write records, rotate logs, or install a global logger.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerReceiveLoopJsonLinesSinkConfig {
    pub receive_loop: JsonLinesSinkConfig,
}

impl ServerReceiveLoopJsonLinesSinkConfig {
    pub fn stderr_default() -> Self {
        Self {
            receive_loop: JsonLinesSinkConfig::stderr(),
        }
    }

    pub fn file_sink(receive_loop_path: impl Into<PathBuf>) -> Self {
        Self {
            receive_loop: JsonLinesSinkConfig::file(receive_loop_path),
        }
    }
}

/// Normalized server JSON Lines sink plan for receive loop operational events.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerReceiveLoopJsonLinesSinkPlan {
    pub receive_loop: JsonLinesSinkPlan,
}

/// Boundary that maps receive loop logging config to a sink plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveLoopJsonLinesSinkBoundary {
    sink_plan: JsonLinesSinkPlanBoundary,
}

impl ServerReceiveLoopJsonLinesSinkBoundary {
    pub fn plan(
        &self,
        config: ServerReceiveLoopJsonLinesSinkConfig,
    ) -> ServerReceiveLoopJsonLinesSinkPlan {
        ServerReceiveLoopJsonLinesSinkPlan {
            receive_loop: self.sink_plan.plan(config.receive_loop),
        }
    }
}

/// Server-side JSON Lines sink config for send error events.
///
/// This is a config/planning boundary only. It does not open files, write
/// records, rotate logs, or install a global logger.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerSendErrorJsonLinesSinkConfig {
    pub send_error: JsonLinesSinkConfig,
}

impl ServerSendErrorJsonLinesSinkConfig {
    pub fn stderr_default() -> Self {
        Self {
            send_error: JsonLinesSinkConfig::stderr(),
        }
    }

    pub fn file_sink(send_error_path: impl Into<PathBuf>) -> Self {
        Self {
            send_error: JsonLinesSinkConfig::file(send_error_path),
        }
    }
}

/// Normalized server JSON Lines sink plan for future send error output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerSendErrorJsonLinesSinkPlan {
    pub send_error: JsonLinesSinkPlan,
}

/// Boundary that maps server send error logging config to a sink plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSendErrorJsonLinesSinkBoundary {
    sink_plan: JsonLinesSinkPlanBoundary,
}

impl ServerSendErrorJsonLinesSinkBoundary {
    pub fn plan(
        &self,
        config: ServerSendErrorJsonLinesSinkConfig,
    ) -> ServerSendErrorJsonLinesSinkPlan {
        ServerSendErrorJsonLinesSinkPlan {
            send_error: self.sink_plan.plan(config.send_error),
        }
    }
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

/// Result of one UDP receive plus decode / gate evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerUdpSocketGateReceiveOutcome {
    pub packet_len: usize,
    pub outcome: ServerReceiveLoopGateOutcome,
}

impl ServerUdpSocketIoStep {
    pub fn receive_one_with_gate(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        expected_protocol_version: ProtocolVersion,
        registry: &AuthenticatedSenderRegistry,
    ) -> io::Result<ServerReceiveLoopGateOutcome> {
        self.receive_one_with_gate_details(socket, buffer, expected_protocol_version, registry)
            .map(|details| details.outcome)
    }

    pub fn receive_one_with_gate_details(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        expected_protocol_version: ProtocolVersion,
        registry: &AuthenticatedSenderRegistry,
    ) -> io::Result<ServerUdpSocketGateReceiveOutcome> {
        let packet = self.socket_io.receive_one(socket, buffer)?;
        let packet_len = packet.bytes.len();
        let outcome = self.receive_loop.handle_received_packet_with_gate(
            expected_protocol_version,
            registry,
            packet.source,
            packet.bytes,
        );

        Ok(ServerUdpSocketGateReceiveOutcome {
            packet_len,
            outcome,
        })
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

/// Launcher for one controller receive/send iteration from server config.
///
/// This is a manual check entry point. It binds one UDP socket, initializes
/// caller-owned in-memory state, and calls the controller receive/send runtime
/// once. It does not loop, retry, requeue, open file sinks, or install global
/// logging.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerReceiveSendOneIterationLauncher {
    socket_io: UdpSocketIoBoundary,
    runtime: ServerControllerReceiveSendRuntimeBoundary,
}

impl ServerReceiveSendOneIterationLauncher {
    pub fn load_startup_config_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ServerAuthResponsePocStartupConfig, ServerReceiveSendOneIterationStartupError> {
        ServerAuthResponsePocLauncher::default()
            .load_startup_config_from_path(path)
            .map_err(ServerReceiveSendOneIterationStartupError::StartupConfig)
    }

    pub fn load_startup_config_from_str(
        &self,
        input: &str,
    ) -> Result<ServerAuthResponsePocStartupConfig, ServerReceiveSendOneIterationStartupError> {
        ServerAuthResponsePocLauncher::default()
            .load_startup_config_from_str(input)
            .map_err(ServerReceiveSendOneIterationStartupError::StartupConfig)
    }

    pub fn run_once_from_path_with_writers<
        OW: io::Write,
        RW: io::Write,
        AW: io::Write,
        SW: io::Write,
    >(
        &self,
        path: impl AsRef<Path>,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<
        ServerReceiveSendOneIterationStartupOutcome,
        ServerReceiveSendOneIterationStartupError,
    > {
        let startup_config = self.load_startup_config_from_path(path)?;
        self.run_once_with_writers(
            startup_config,
            operational_writer,
            rejection_writer,
            auth_log_writer,
            send_log_writer,
        )
    }

    pub fn run_once_with_writers<OW: io::Write, RW: io::Write, AW: io::Write, SW: io::Write>(
        &self,
        startup_config: ServerAuthResponsePocStartupConfig,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<
        ServerReceiveSendOneIterationStartupOutcome,
        ServerReceiveSendOneIterationStartupError,
    > {
        let socket = self
            .socket_io
            .bind(startup_config.bind_address)
            .map_err(|error| ServerReceiveSendOneIterationStartupError::Bind {
                address: startup_config.bind_address,
                kind: error.kind(),
            })?;
        let mut buffer = vec![0_u8; DEFAULT_UDP_PACKET_BUFFER_LEN];
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let timestamp = current_system_timestamp_micros();
        let outcome = self
            .runtime
            .run_once(
                &socket,
                &mut buffer,
                &mut registry,
                &mut queue_collection,
                &startup_config.auth_config,
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: startup_config.expected_protocol_version,
                        timestamp,
                        continue_requested: true,
                    },
                    heartbeat_timing: ServerHeartbeatAckTiming {
                        server_received_at: timestamp,
                        server_sent_at: timestamp,
                    },
                    encode_context: EncodeContext {
                        protocol_version: startup_config.expected_protocol_version,
                    },
                    auth_log_timestamp: timestamp,
                    send_log_timestamp: timestamp,
                },
                operational_writer,
                rejection_writer,
                auth_log_writer,
                send_log_writer,
            )
            .map_err(ServerReceiveSendOneIterationStartupError::Runtime)?;

        Ok(ServerReceiveSendOneIterationStartupOutcome {
            bind_address: startup_config.bind_address,
            registry,
            queue_collection,
            outcome,
        })
    }
}

/// Result of launching one controller receive/send iteration from config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReceiveSendOneIterationStartupOutcome {
    pub bind_address: SocketAddr,
    pub registry: AuthenticatedSenderRegistry,
    pub queue_collection: ServerOutboundQueueCollection,
    pub outcome: ServerControllerReceiveSendRuntimeResult,
}

/// Startup error for one controller receive/send iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveSendOneIterationStartupError {
    StartupConfig(ServerAuthResponsePocStartupError),
    Bind {
        address: SocketAddr,
        kind: io::ErrorKind,
    },
    Runtime(ServerControllerReceiveSendRuntimeError),
}

/// Launcher for exactly two controller receive/send iterations from server config.
///
/// This is a manual auth-then-heartbeat check entry point. It keeps one UDP
/// socket, registry, and queue collection across two calls to the existing
/// one-iteration runtime. It does not implement a continuous loop, retry,
/// requeue, file sinks, or process-wide logging.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerReceiveSendTwoIterationLauncher {
    socket_io: UdpSocketIoBoundary,
    runtime: ServerControllerReceiveSendRuntimeBoundary,
    liveness_commit: ServerHeartbeatLivenessCommitBoundary,
}

impl ServerReceiveSendTwoIterationLauncher {
    pub fn load_startup_config_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ServerAuthResponsePocStartupConfig, ServerReceiveSendOneIterationStartupError> {
        ServerReceiveSendOneIterationLauncher::default().load_startup_config_from_path(path)
    }

    pub fn run_two_from_path_with_writers<
        OW: io::Write,
        RW: io::Write,
        AW: io::Write,
        SW: io::Write,
    >(
        &self,
        path: impl AsRef<Path>,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<
        ServerReceiveSendTwoIterationStartupOutcome,
        ServerReceiveSendOneIterationStartupError,
    > {
        let startup_config = self.load_startup_config_from_path(path)?;
        self.run_two_with_writers(
            startup_config,
            operational_writer,
            rejection_writer,
            auth_log_writer,
            send_log_writer,
        )
    }

    pub fn run_two_with_writers<OW: io::Write, RW: io::Write, AW: io::Write, SW: io::Write>(
        &self,
        startup_config: ServerAuthResponsePocStartupConfig,
        mut operational_writer: OW,
        mut rejection_writer: RW,
        mut auth_log_writer: AW,
        mut send_log_writer: SW,
    ) -> Result<
        ServerReceiveSendTwoIterationStartupOutcome,
        ServerReceiveSendOneIterationStartupError,
    > {
        let socket = self
            .socket_io
            .bind(startup_config.bind_address)
            .map_err(|error| ServerReceiveSendOneIterationStartupError::Bind {
                address: startup_config.bind_address,
                kind: error.kind(),
            })?;
        let mut buffer = vec![0_u8; DEFAULT_UDP_PACKET_BUFFER_LEN];
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let mut heartbeat_liveness_state = ServerHeartbeatLivenessState::default();
        let first_timestamp = current_system_timestamp_micros();
        let first = self
            .runtime
            .run_once(
                &socket,
                &mut buffer,
                &mut registry,
                &mut queue_collection,
                &startup_config.auth_config,
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: startup_config.expected_protocol_version,
                        timestamp: first_timestamp,
                        continue_requested: true,
                    },
                    heartbeat_timing: ServerHeartbeatAckTiming {
                        server_received_at: first_timestamp,
                        server_sent_at: first_timestamp,
                    },
                    encode_context: EncodeContext {
                        protocol_version: startup_config.expected_protocol_version,
                    },
                    auth_log_timestamp: first_timestamp,
                    send_log_timestamp: first_timestamp,
                },
                &mut operational_writer,
                &mut rejection_writer,
                &mut auth_log_writer,
                &mut send_log_writer,
            )
            .map_err(ServerReceiveSendOneIterationStartupError::Runtime)?;
        let second_timestamp = current_system_timestamp_micros();
        let second = self
            .runtime
            .run_once(
                &socket,
                &mut buffer,
                &mut registry,
                &mut queue_collection,
                &startup_config.auth_config,
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: startup_config.expected_protocol_version,
                        timestamp: second_timestamp,
                        continue_requested: true,
                    },
                    heartbeat_timing: ServerHeartbeatAckTiming {
                        server_received_at: second_timestamp,
                        server_sent_at: second_timestamp,
                    },
                    encode_context: EncodeContext {
                        protocol_version: startup_config.expected_protocol_version,
                    },
                    auth_log_timestamp: second_timestamp,
                    send_log_timestamp: second_timestamp,
                },
                &mut operational_writer,
                &mut rejection_writer,
                &mut auth_log_writer,
                &mut send_log_writer,
            )
            .map_err(ServerReceiveSendOneIterationStartupError::Runtime)?;
        let heartbeat_liveness_commit =
            heartbeat_handoff_from_controller_result(&second).map(|handoff| {
                self.liveness_commit.commit(
                    &mut heartbeat_liveness_state,
                    handoff.processing_inputs.state.clone(),
                )
            });

        Ok(ServerReceiveSendTwoIterationStartupOutcome {
            bind_address: startup_config.bind_address,
            registry,
            queue_collection,
            first,
            second,
            heartbeat_liveness_state,
            heartbeat_liveness_commit,
        })
    }
}

/// Result of launching exactly two controller receive/send iterations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReceiveSendTwoIterationStartupOutcome {
    pub bind_address: SocketAddr,
    pub registry: AuthenticatedSenderRegistry,
    pub queue_collection: ServerOutboundQueueCollection,
    pub first: ServerControllerReceiveSendRuntimeResult,
    pub second: ServerControllerReceiveSendRuntimeResult,
    pub heartbeat_liveness_state: ServerHeartbeatLivenessState,
    pub heartbeat_liveness_commit: Option<ServerHeartbeatLivenessCommitOutcome>,
}

/// Launcher for manual auth-then-video receive and queue verification.
///
/// This owns the authenticated sender registry and video frame queue state for
/// one accepted/rejected auth packet followed by at most one video packet. Auth
/// response sending uses the existing auth response PoC step. The second packet
/// uses the existing receive/send controller runtime and packet acceptance gate,
/// then stores only accepted `VideoFrame` side effects into caller-owned queue
/// state. It does not decode H.264, render, sync 4 views, retry, or loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerReceiveAuthVideoQueueOnceLauncher {
    socket_io: UdpSocketIoBoundary,
    auth: ServerAuthResponsePocStep,
    receive_loop: ServerReceiveLoopStep,
    registered: ServerRegisteredPacketBoundary,
    video_handler: ServerVideoFrameHandlerBoundary,
    video_queue_storage: ServerVideoFrameQueueStorageBoundary,
    reassembly: ServerVideoFrameFragmentReassemblyBoundary,
}

const SERVER_AUTH_VIDEO_QUEUE_DEFAULT_MAX_VIDEO_PACKETS: usize = 4_096;
const SERVER_AUTH_VIDEO_QUEUE_DEFAULT_FRAGMENT_IDLE_TIMEOUT: Duration = Duration::from_secs(15);
const SERVER_AUTH_VIDEO_QUEUE_DEFAULT_RECEIVE_BUFFER_BYTES: usize = 8_388_608;

/// Manual receive policy for auth-then-video queue verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerReceiveAuthVideoQueueOnceManualPolicy {
    pub max_video_packets: usize,
    pub receive_timeout: Duration,
    pub expected_reassembled_frames: u64,
    pub stop_after_expected_reassembled_frames: bool,
    pub receive_buffer_bytes: usize,
}

impl Default for ServerReceiveAuthVideoQueueOnceManualPolicy {
    fn default() -> Self {
        Self {
            max_video_packets: SERVER_AUTH_VIDEO_QUEUE_DEFAULT_MAX_VIDEO_PACKETS,
            receive_timeout: SERVER_AUTH_VIDEO_QUEUE_DEFAULT_FRAGMENT_IDLE_TIMEOUT,
            expected_reassembled_frames: 1,
            stop_after_expected_reassembled_frames: true,
            receive_buffer_bytes: SERVER_AUTH_VIDEO_QUEUE_DEFAULT_RECEIVE_BUFFER_BYTES,
        }
    }
}

impl ServerReceiveAuthVideoQueueOnceLauncher {
    pub fn load_startup_config_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ServerAuthResponsePocStartupConfig, ServerReceiveSendOneIterationStartupError> {
        ServerReceiveSendOneIterationLauncher::default().load_startup_config_from_path(path)
    }

    pub fn run_once_from_path_with_writers<
        OW: io::Write,
        RW: io::Write,
        AW: io::Write,
        SW: io::Write,
    >(
        &self,
        path: impl AsRef<Path>,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<
        ServerReceiveAuthVideoQueueOnceStartupOutcome,
        ServerReceiveAuthVideoQueueOnceStartupError,
    > {
        let startup_config = self
            .load_startup_config_from_path(path)
            .map_err(ServerReceiveAuthVideoQueueOnceStartupError::OneIterationStartup)?;
        self.run_once_with_writers_and_policy(
            startup_config,
            operational_writer,
            rejection_writer,
            auth_log_writer,
            send_log_writer,
            ServerReceiveAuthVideoQueueOnceManualPolicy::default(),
        )
    }

    pub fn run_once_with_writers<OW: io::Write, RW: io::Write, AW: io::Write, SW: io::Write>(
        &self,
        startup_config: ServerAuthResponsePocStartupConfig,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<
        ServerReceiveAuthVideoQueueOnceStartupOutcome,
        ServerReceiveAuthVideoQueueOnceStartupError,
    > {
        self.run_once_with_writers_and_policy(
            startup_config,
            operational_writer,
            rejection_writer,
            auth_log_writer,
            send_log_writer,
            ServerReceiveAuthVideoQueueOnceManualPolicy::default(),
        )
    }

    pub fn run_once_from_path_with_writers_and_policy<
        OW: io::Write,
        RW: io::Write,
        AW: io::Write,
        SW: io::Write,
    >(
        &self,
        path: impl AsRef<Path>,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
        policy: ServerReceiveAuthVideoQueueOnceManualPolicy,
    ) -> Result<
        ServerReceiveAuthVideoQueueOnceStartupOutcome,
        ServerReceiveAuthVideoQueueOnceStartupError,
    > {
        let startup_config = self
            .load_startup_config_from_path(path)
            .map_err(ServerReceiveAuthVideoQueueOnceStartupError::OneIterationStartup)?;
        self.run_once_with_writers_and_policy(
            startup_config,
            operational_writer,
            rejection_writer,
            auth_log_writer,
            send_log_writer,
            policy,
        )
    }

    pub fn run_once_with_writers_and_policy<
        OW: io::Write,
        RW: io::Write,
        AW: io::Write,
        SW: io::Write,
    >(
        &self,
        startup_config: ServerAuthResponsePocStartupConfig,
        _operational_writer: OW,
        _rejection_writer: RW,
        _auth_log_writer: AW,
        _send_log_writer: SW,
        policy: ServerReceiveAuthVideoQueueOnceManualPolicy,
    ) -> Result<
        ServerReceiveAuthVideoQueueOnceStartupOutcome,
        ServerReceiveAuthVideoQueueOnceStartupError,
    > {
        let socket = self
            .socket_io
            .bind(startup_config.bind_address)
            .map_err(|error| ServerReceiveAuthVideoQueueOnceStartupError::Bind {
                address: startup_config.bind_address,
                kind: error.kind(),
            })?;
        let mut buffer = vec![0_u8; DEFAULT_UDP_PACKET_BUFFER_LEN];
        let mut registry = AuthenticatedSenderRegistry::default();
        let queue_collection = ServerOutboundQueueCollection::default();
        let mut video_queue_state = ServerVideoFrameQueueState::default();
        let mut reassembly_state = ServerVideoFrameReassemblyState::default();
        let receive_buffer = apply_manual_receive_buffer_policy(&socket, policy);

        let first_auth = self
            .auth
            .run_one(
                &socket,
                &mut buffer,
                startup_config.expected_protocol_version,
                &startup_config.auth_config,
                &mut registry,
            )
            .map_err(ServerReceiveAuthVideoQueueOnceStartupError::Auth)?;

        let video = if first_auth.auth_flow.decision.accepted {
            socket
                .set_read_timeout(Some(policy.receive_timeout))
                .map_err(
                    |error| ServerReceiveAuthVideoQueueOnceStartupError::VideoReceive {
                        kind: error.kind(),
                    },
                )?;
            self.receive_video_packets_until_queued(
                &socket,
                &mut buffer,
                &registry,
                &mut video_queue_state,
                &mut reassembly_state,
                startup_config.expected_protocol_version,
                policy,
            )?
        } else {
            ServerReceiveAuthVideoQueueOnceVideoOutcome::NotReceivedAuthRejected
        };

        Ok(ServerReceiveAuthVideoQueueOnceStartupOutcome {
            bind_address: startup_config.bind_address,
            registry,
            queue_collection,
            video_queue_state,
            reassembly_state,
            receive_buffer,
            first_auth,
            video,
        })
    }

    fn receive_video_packets_until_queued(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        registry: &AuthenticatedSenderRegistry,
        video_queue_state: &mut ServerVideoFrameQueueState,
        reassembly_state: &mut ServerVideoFrameReassemblyState,
        expected_protocol_version: ProtocolVersion,
        policy: ServerReceiveAuthVideoQueueOnceManualPolicy,
    ) -> Result<
        ServerReceiveAuthVideoQueueOnceVideoOutcome,
        ServerReceiveAuthVideoQueueOnceStartupError,
    > {
        let mut summary = ServerReceiveAuthVideoQueueOnceVideoSummary::default();
        let mut last_queue = None;

        for _ in 0..policy.max_video_packets {
            let received = match self.socket_io.receive_one(socket, buffer) {
                Ok(received) => received,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    summary.receive_timed_out = true;
                    finalize_auth_video_queue_summary(
                        &mut summary,
                        reassembly_state,
                        video_queue_state,
                    );
                    return Ok(ServerReceiveAuthVideoQueueOnceVideoOutcome::Received {
                        summary,
                        queue: last_queue,
                    });
                }
                Err(error) => {
                    return Err(ServerReceiveAuthVideoQueueOnceStartupError::VideoReceive {
                        kind: error.kind(),
                    });
                }
            };
            summary.packets_received = summary.packets_received.saturating_add(1);
            let queued_at = current_system_timestamp_micros();

            match self.receive_loop.handle_received_packet_with_gate(
                expected_protocol_version,
                registry,
                received.source,
                received.bytes,
            ) {
                ServerReceiveLoopGateOutcome::Accepted(route) => match route {
                    ServerInboundRoute::VideoFrame { .. } => {
                        let registered = self
                            .registered
                            .prepare_for_handler(registry, route)
                            .map_err(ServerReceiveAuthVideoQueueOnceStartupError::Registered)?;
                        let ServerRegisteredClientPacket::VideoFrame(packet) = registered else {
                            summary.non_video_packets = summary.non_video_packets.saturating_add(1);
                            continue;
                        };
                        let input = self.video_handler.prepare_input(packet);
                        let queue = self.video_queue_storage.store_frame(
                            video_queue_state,
                            input,
                            queued_at,
                            ServerVideoFrameQueuePolicy::default(),
                        );
                        if matches!(queue, ServerVideoFrameQueueStorageResult::Stored { .. }) {
                            summary.direct_frames_queued =
                                summary.direct_frames_queued.saturating_add(1);
                            summary.frames_queued = summary.frames_queued.saturating_add(1);
                        }
                        summary.queue_len = video_queue_state.total_len();
                        last_queue = Some(ServerVideoFrameQueueRuntimeResult::Queued(queue));
                        return Ok(ServerReceiveAuthVideoQueueOnceVideoOutcome::Received {
                            summary,
                            queue: last_queue,
                        });
                    }
                    ServerInboundRoute::VideoFrameFragment { .. } => {
                        let registered = self
                            .registered
                            .prepare_for_handler(registry, route)
                            .map_err(ServerReceiveAuthVideoQueueOnceStartupError::Registered)?;
                        let ServerRegisteredClientPacket::VideoFrameFragment(packet) = registered
                        else {
                            summary.non_video_packets = summary.non_video_packets.saturating_add(1);
                            continue;
                        };
                        summary.fragments_received = summary.fragments_received.saturating_add(1);
                        match self.reassembly.apply_fragment_and_queue_if_complete(
                            reassembly_state,
                            video_queue_state,
                            packet,
                            queued_at,
                            ServerVideoFrameQueuePolicy::default(),
                        ) {
                            ServerVideoFrameReassemblyApplyResult::FragmentStored { .. } => {}
                            ServerVideoFrameReassemblyApplyResult::DuplicateFragmentIgnored {
                                ..
                            } => {
                                summary.duplicate_fragments =
                                    summary.duplicate_fragments.saturating_add(1);
                            }
                            ServerVideoFrameReassemblyApplyResult::RejectedFragment { .. } => {
                                summary.rejected_fragments =
                                    summary.rejected_fragments.saturating_add(1);
                            }
                            ServerVideoFrameReassemblyApplyResult::FrameComplete {
                                queue_result,
                                ..
                            } => {
                                summary.frames_reassembled =
                                    summary.frames_reassembled.saturating_add(1);
                                if matches!(
                                    queue_result,
                                    ServerVideoFrameQueueStorageResult::Stored { .. }
                                ) {
                                    summary.frames_queued = summary.frames_queued.saturating_add(1);
                                }
                                last_queue =
                                    Some(ServerVideoFrameQueueRuntimeResult::Queued(queue_result));
                                if policy.stop_after_expected_reassembled_frames
                                    && summary.frames_reassembled
                                        >= policy.expected_reassembled_frames.max(1)
                                {
                                    finalize_auth_video_queue_summary(
                                        &mut summary,
                                        reassembly_state,
                                        video_queue_state,
                                    );
                                    return Ok(
                                        ServerReceiveAuthVideoQueueOnceVideoOutcome::Received {
                                            summary,
                                            queue: last_queue,
                                        },
                                    );
                                }
                            }
                        }
                    }
                    _ => {
                        summary.non_video_packets = summary.non_video_packets.saturating_add(1);
                    }
                },
                ServerReceiveLoopGateOutcome::Rejected(rejection) => {
                    summary.rejected_packets = summary.rejected_packets.saturating_add(1);
                    if matches!(
                        rejection,
                        ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
                            message_type: MessageType::VideoFrameFragment,
                            ..
                        })
                    ) {
                        summary.rejected_fragments = summary.rejected_fragments.saturating_add(1);
                    }
                }
            }
        }

        summary.max_packets_reached = true;
        finalize_auth_video_queue_summary(&mut summary, reassembly_state, video_queue_state);
        Ok(ServerReceiveAuthVideoQueueOnceVideoOutcome::Received {
            summary,
            queue: last_queue,
        })
    }
}

/// Result of launching one manual auth-then-video queue verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReceiveAuthVideoQueueOnceStartupOutcome {
    pub bind_address: SocketAddr,
    pub registry: AuthenticatedSenderRegistry,
    pub queue_collection: ServerOutboundQueueCollection,
    pub video_queue_state: ServerVideoFrameQueueState,
    pub reassembly_state: ServerVideoFrameReassemblyState,
    pub receive_buffer: ServerUdpReceiveBufferTuningResult,
    pub first_auth: ServerAuthResponsePocOutcome,
    pub video: ServerReceiveAuthVideoQueueOnceVideoOutcome,
}

/// Second-packet result for the manual auth-then-video queue launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveAuthVideoQueueOnceVideoOutcome {
    Received {
        summary: ServerReceiveAuthVideoQueueOnceVideoSummary,
        queue: Option<ServerVideoFrameQueueRuntimeResult>,
    },
    NotReceivedAuthRejected,
}

/// Manual auth/video queue diagnostics for non-fragmented and fragmented frames.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerReceiveAuthVideoQueueOnceVideoSummary {
    pub packets_received: u64,
    pub fragments_received: u64,
    pub frames_reassembled: u64,
    pub frames_queued: u64,
    pub direct_frames_queued: u64,
    pub rejected_packets: u64,
    pub rejected_fragments: u64,
    pub duplicate_fragments: u64,
    pub non_video_packets: u64,
    pub incomplete_reassembly_frames: usize,
    pub queue_len: usize,
    pub incomplete_frame_progress: Vec<ServerVideoFrameReassemblyFrameProgress>,
    pub receive_timed_out: bool,
    pub max_packets_reached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVideoFrameReassemblyFrameProgress {
    pub key: ServerVideoFrameReassemblyKey,
    pub fragments_received: usize,
    pub fragments_expected: usize,
    pub fragments_missing: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerUdpReceiveBufferTuningResult {
    pub requested_bytes: usize,
    pub effective_bytes: Option<usize>,
    pub set_error: Option<String>,
    pub read_error: Option<String>,
}

fn apply_manual_receive_buffer_policy(
    socket: &UdpSocket,
    policy: ServerReceiveAuthVideoQueueOnceManualPolicy,
) -> ServerUdpReceiveBufferTuningResult {
    let sock_ref = SockRef::from(socket);
    let set_error = sock_ref
        .set_recv_buffer_size(policy.receive_buffer_bytes)
        .err()
        .map(|error| format!("{:?}: {}", error.kind(), error));
    let (effective_bytes, read_error) = match sock_ref.recv_buffer_size() {
        Ok(size) => (Some(size), None),
        Err(error) => (None, Some(format!("{:?}: {}", error.kind(), error))),
    };

    ServerUdpReceiveBufferTuningResult {
        requested_bytes: policy.receive_buffer_bytes,
        effective_bytes,
        set_error,
        read_error,
    }
}

fn finalize_auth_video_queue_summary(
    summary: &mut ServerReceiveAuthVideoQueueOnceVideoSummary,
    reassembly_state: &ServerVideoFrameReassemblyState,
    video_queue_state: &ServerVideoFrameQueueState,
) {
    summary.incomplete_reassembly_frames = reassembly_state.tracked_frame_count();
    summary.incomplete_frame_progress = reassembly_state.frame_progress();
    summary.queue_len = video_queue_state.total_len();
}

/// Startup error for the manual auth-then-video queue launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveAuthVideoQueueOnceStartupError {
    OneIterationStartup(ServerReceiveSendOneIterationStartupError),
    Bind {
        address: SocketAddr,
        kind: io::ErrorKind,
    },
    Auth(ServerAuthResponsePocError),
    Runtime(ServerControllerReceiveSendRuntimeError),
    Registered(ServerRegisteredPacketBoundaryError),
    VideoReceive {
        kind: io::ErrorKind,
    },
}

/// Launcher for exactly three controller receive/send iterations from server config.
///
/// This is a manual auth -> heartbeat -> stats-observation check entry point.
/// It keeps one UDP socket, registry, and queue collection across three calls
/// to the existing one-iteration runtime. It does not implement a continuous
/// loop, retry, requeue, file sinks, process-wide logging, or timebase state
/// commit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerReceiveSendThreeIterationLauncher {
    socket_io: UdpSocketIoBoundary,
    runtime: ServerControllerReceiveSendRuntimeBoundary,
    observation_return: ServerHeartbeatObservationReturnBoundary,
    liveness_commit: ServerHeartbeatLivenessCommitBoundary,
    rtt_offset_policy_commit: ServerHeartbeatRttOffsetPolicyCommitBoundary,
}

impl ServerReceiveSendThreeIterationLauncher {
    pub fn load_startup_config_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ServerAuthResponsePocStartupConfig, ServerReceiveSendThreeIterationStartupError>
    {
        ServerReceiveSendOneIterationLauncher::default()
            .load_startup_config_from_path(path)
            .map_err(ServerReceiveSendThreeIterationStartupError::OneIterationStartup)
    }

    pub fn run_three_from_path_with_writers<
        OW: io::Write,
        RW: io::Write,
        AW: io::Write,
        SW: io::Write,
    >(
        &self,
        path: impl AsRef<Path>,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<
        ServerReceiveSendThreeIterationStartupOutcome,
        ServerReceiveSendThreeIterationStartupError,
    > {
        let startup_config = self.load_startup_config_from_path(path)?;
        self.run_three_with_writers(
            startup_config,
            operational_writer,
            rejection_writer,
            auth_log_writer,
            send_log_writer,
        )
    }

    pub fn run_three_with_writers<OW: io::Write, RW: io::Write, AW: io::Write, SW: io::Write>(
        &self,
        startup_config: ServerAuthResponsePocStartupConfig,
        mut operational_writer: OW,
        mut rejection_writer: RW,
        mut auth_log_writer: AW,
        mut send_log_writer: SW,
    ) -> Result<
        ServerReceiveSendThreeIterationStartupOutcome,
        ServerReceiveSendThreeIterationStartupError,
    > {
        let socket = self
            .socket_io
            .bind(startup_config.bind_address)
            .map_err(|error| ServerReceiveSendThreeIterationStartupError::Bind {
                address: startup_config.bind_address,
                kind: error.kind(),
            })?;
        let mut buffer = vec![0_u8; DEFAULT_UDP_PACKET_BUFFER_LEN];
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let mut heartbeat_liveness_state = ServerHeartbeatLivenessState::default();
        let mut heartbeat_rtt_offset_state = ServerHeartbeatRttOffsetState::default();

        let first = self.run_controller_once(
            &socket,
            &mut buffer,
            &mut registry,
            &mut queue_collection,
            &startup_config,
            current_system_timestamp_micros(),
            &mut operational_writer,
            &mut rejection_writer,
            &mut auth_log_writer,
            &mut send_log_writer,
        )?;
        let second = self.run_controller_once(
            &socket,
            &mut buffer,
            &mut registry,
            &mut queue_collection,
            &startup_config,
            current_system_timestamp_micros(),
            &mut operational_writer,
            &mut rejection_writer,
            &mut auth_log_writer,
            &mut send_log_writer,
        )?;
        let third = self.run_controller_once(
            &socket,
            &mut buffer,
            &mut registry,
            &mut queue_collection,
            &startup_config,
            current_system_timestamp_micros(),
            &mut operational_writer,
            &mut rejection_writer,
            &mut auth_log_writer,
            &mut send_log_writer,
        )?;

        let heartbeat_handoff = heartbeat_handoff_from_controller_result(&second)
            .ok_or(ServerReceiveSendThreeIterationStartupError::MissingHeartbeatHandoff)?;
        let heartbeat_liveness_commit = self.liveness_commit.commit(
            &mut heartbeat_liveness_state,
            heartbeat_handoff.processing_inputs.state.clone(),
        );
        let client_stats = client_stats_input_from_controller_result(&third)
            .ok_or(ServerReceiveSendThreeIterationStartupError::MissingClientStats)?;
        let heartbeat_calculation = self
            .observation_return
            .calculate_from_client_stats(heartbeat_handoff, client_stats)
            .map_err(ServerReceiveSendThreeIterationStartupError::ObservationReturn)?;
        let heartbeat_rtt_offset_policy_commit = self.rtt_offset_policy_commit.evaluate_and_commit(
            &mut heartbeat_rtt_offset_state,
            heartbeat_calculation.clone(),
            ServerHeartbeatRttOffsetCandidatePolicy::default(),
            Some(current_system_timestamp_micros()),
        );

        Ok(ServerReceiveSendThreeIterationStartupOutcome {
            bind_address: startup_config.bind_address,
            registry,
            queue_collection,
            first,
            second,
            third,
            heartbeat_calculation,
            heartbeat_liveness_state,
            heartbeat_liveness_commit,
            heartbeat_rtt_offset_state,
            heartbeat_rtt_offset_policy_commit,
        })
    }

    fn run_controller_once<OW: io::Write, RW: io::Write, AW: io::Write, SW: io::Write>(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        registry: &mut AuthenticatedSenderRegistry,
        queue_collection: &mut ServerOutboundQueueCollection,
        startup_config: &ServerAuthResponsePocStartupConfig,
        timestamp: TimestampMicros,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<ServerControllerReceiveSendRuntimeResult, ServerReceiveSendThreeIterationStartupError>
    {
        self.runtime
            .run_once(
                socket,
                buffer,
                registry,
                queue_collection,
                &startup_config.auth_config,
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: startup_config.expected_protocol_version,
                        timestamp,
                        continue_requested: true,
                    },
                    heartbeat_timing: ServerHeartbeatAckTiming {
                        server_received_at: timestamp,
                        server_sent_at: timestamp,
                    },
                    encode_context: EncodeContext {
                        protocol_version: startup_config.expected_protocol_version,
                    },
                    auth_log_timestamp: timestamp,
                    send_log_timestamp: timestamp,
                },
                operational_writer,
                rejection_writer,
                auth_log_writer,
                send_log_writer,
            )
            .map_err(ServerReceiveSendThreeIterationStartupError::Runtime)
    }
}

/// Result of launching exactly three controller receive/send iterations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReceiveSendThreeIterationStartupOutcome {
    pub bind_address: SocketAddr,
    pub registry: AuthenticatedSenderRegistry,
    pub queue_collection: ServerOutboundQueueCollection,
    pub first: ServerControllerReceiveSendRuntimeResult,
    pub second: ServerControllerReceiveSendRuntimeResult,
    pub third: ServerControllerReceiveSendRuntimeResult,
    pub heartbeat_calculation: ServerHeartbeatRttOffsetCalculation,
    pub heartbeat_liveness_state: ServerHeartbeatLivenessState,
    pub heartbeat_liveness_commit: ServerHeartbeatLivenessCommitOutcome,
    pub heartbeat_rtt_offset_state: ServerHeartbeatRttOffsetState,
    pub heartbeat_rtt_offset_policy_commit: ServerHeartbeatRttOffsetPolicyCommitOutcome,
}

/// Startup error for exactly three controller receive/send iterations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveSendThreeIterationStartupError {
    OneIterationStartup(ServerReceiveSendOneIterationStartupError),
    Bind {
        address: SocketAddr,
        kind: io::ErrorKind,
    },
    Runtime(ServerControllerReceiveSendRuntimeError),
    MissingHeartbeatHandoff,
    MissingClientStats,
    ObservationReturn(ServerHeartbeatObservationReturnError),
}

fn heartbeat_handoff_from_controller_result(
    result: &ServerControllerReceiveSendRuntimeResult,
) -> Option<&ServerHeartbeatAckHandoff> {
    let ServerControllerReceiveSendRuntimeResult::Iteration { iteration, .. } = result else {
        return None;
    };
    let ServerDispatchRuntimeSideEffectApplyResult::HeartbeatAck(handoff) =
        &iteration.side_effect.result
    else {
        return None;
    };
    Some(handoff)
}

fn client_stats_input_from_controller_result(
    result: &ServerControllerReceiveSendRuntimeResult,
) -> Option<&ServerClientStatsHandlerInput> {
    let ServerControllerReceiveSendRuntimeResult::Iteration { iteration, .. } = result else {
        return None;
    };
    let ServerDispatchRuntimeSideEffectApplyResult::ClientStats(input) =
        &iteration.side_effect.result
    else {
        return None;
    };
    Some(input)
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

fn current_system_timestamp_micros() -> TimestampMicros {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or(0);
    TimestampMicros(u64::try_from(micros).unwrap_or(u64::MAX))
}

fn server_timestamp_saturating_add(timestamp: TimestampMicros, micros: u64) -> TimestampMicros {
    TimestampMicros(timestamp.0.saturating_add(micros))
}

fn server_min_timestamp(
    first: TimestampMicros,
    second: Option<TimestampMicros>,
) -> TimestampMicros {
    match second {
        Some(second) if second.0 < first.0 => second,
        _ => first,
    }
}

fn server_heartbeat_continuous_loop_log(
    input: ServerHeartbeatContinuousLoopPolicyInput,
    reason: ServerHeartbeatContinuousLoopPolicyReason,
) -> ServerHeartbeatContinuousLoopLogHandoff {
    ServerHeartbeatContinuousLoopLogHandoff {
        observed_at: input.now,
        reason,
        timeout_tick_interval_micros: input.cadence.timeout_tick_interval_micros,
        metrics_snapshot_interval_micros: input.cadence.metrics_snapshot_interval_micros,
        completed_timeout_ticks: input.state.completed_timeout_ticks,
        exported_metrics_snapshots: input.state.exported_metrics_snapshots,
    }
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

/// Lifecycle states for a future continuous receive loop body.
///
/// This does not implement a loop. It only names the checkpoints around socket
/// receive, one-packet processing, accepted dispatch, rejection logging, and
/// shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerContinuousReceiveLoopLifecycleState {
    Stopped,
    AwaitingSocketReceive,
    ProcessingReceivedPacket,
    DispatchingAcceptedRoute,
    PreparingRejectionLogs,
    SocketReceiveFailed,
}

/// Next action a future continuous receive loop body may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerContinuousReceiveLoopAction {
    Stop,
    ReceiveOneDatagram,
    DecodeAndGateOnePacket,
    DispatchAcceptedRoute,
    PrepareRejectionLogs,
    ObserveSocketReceiveError,
}

/// Minimal input for planning the next continuous receive loop step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopLifecycleInput {
    pub stop_requested: bool,
}

/// Planned lifecycle state/action for a future continuous receive loop body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopLifecyclePlan {
    pub state: ServerContinuousReceiveLoopLifecycleState,
    pub action: ServerContinuousReceiveLoopAction,
    pub operational_log_required: bool,
    pub rejection_log_required: bool,
    pub handler_handoff_required: bool,
}

/// Minimal lifecycle boundary for the future continuous receive loop body.
///
/// This boundary does not call sockets, decode packets, run handlers, drop
/// packets, write logs, block, sleep, or spawn async tasks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopLifecycleBoundary;

impl ServerContinuousReceiveLoopLifecycleBoundary {
    pub fn plan_next(
        &self,
        input: ServerContinuousReceiveLoopLifecycleInput,
    ) -> ServerContinuousReceiveLoopLifecyclePlan {
        if input.stop_requested {
            return ServerContinuousReceiveLoopLifecyclePlan {
                state: ServerContinuousReceiveLoopLifecycleState::Stopped,
                action: ServerContinuousReceiveLoopAction::Stop,
                operational_log_required: false,
                rejection_log_required: false,
                handler_handoff_required: false,
            };
        }

        ServerContinuousReceiveLoopLifecyclePlan {
            state: ServerContinuousReceiveLoopLifecycleState::AwaitingSocketReceive,
            action: ServerContinuousReceiveLoopAction::ReceiveOneDatagram,
            operational_log_required: false,
            rejection_log_required: false,
            handler_handoff_required: false,
        }
    }

    pub fn plan_received_packet(&self) -> ServerContinuousReceiveLoopLifecyclePlan {
        ServerContinuousReceiveLoopLifecyclePlan {
            state: ServerContinuousReceiveLoopLifecycleState::ProcessingReceivedPacket,
            action: ServerContinuousReceiveLoopAction::DecodeAndGateOnePacket,
            operational_log_required: false,
            rejection_log_required: false,
            handler_handoff_required: false,
        }
    }

    pub fn plan_after_gate_outcome(
        &self,
        outcome: &ServerReceiveLoopGateOutcome,
    ) -> ServerContinuousReceiveLoopLifecyclePlan {
        match outcome {
            ServerReceiveLoopGateOutcome::Accepted(_) => ServerContinuousReceiveLoopLifecyclePlan {
                state: ServerContinuousReceiveLoopLifecycleState::DispatchingAcceptedRoute,
                action: ServerContinuousReceiveLoopAction::DispatchAcceptedRoute,
                operational_log_required: true,
                rejection_log_required: false,
                handler_handoff_required: true,
            },
            ServerReceiveLoopGateOutcome::Rejected(_) => ServerContinuousReceiveLoopLifecyclePlan {
                state: ServerContinuousReceiveLoopLifecycleState::PreparingRejectionLogs,
                action: ServerContinuousReceiveLoopAction::PrepareRejectionLogs,
                operational_log_required: true,
                rejection_log_required: true,
                handler_handoff_required: false,
            },
        }
    }

    pub fn plan_socket_receive_error(&self) -> ServerContinuousReceiveLoopLifecyclePlan {
        ServerContinuousReceiveLoopLifecyclePlan {
            state: ServerContinuousReceiveLoopLifecycleState::SocketReceiveFailed,
            action: ServerContinuousReceiveLoopAction::ObserveSocketReceiveError,
            operational_log_required: false,
            rejection_log_required: false,
            handler_handoff_required: false,
        }
    }
}

/// One-tick checkpoints for connecting a future receive loop body.
///
/// This is a planning state, not a runtime loop state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerContinuousReceiveLoopTickState {
    Stopped,
    AwaitingSocketReceive,
    ReceivedPacketReadyForDecode,
    AcceptedRouteReadyForHandoff,
    RejectionReadyForLogs,
    SocketReceiveFailed,
}

/// Planned work for one future continuous receive loop tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopTickPlan {
    pub state: ServerContinuousReceiveLoopTickState,
    pub lifecycle: ServerContinuousReceiveLoopLifecyclePlan,
    pub packet_len: Option<usize>,
}

/// Minimal boundary for connecting one future continuous receive loop tick.
///
/// This boundary observes planned checkpoints only. It does not call sockets,
/// decode packets, invoke handlers, drop packets, write logs, block, sleep, or
/// spawn async tasks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopTickBoundary {
    lifecycle: ServerContinuousReceiveLoopLifecycleBoundary,
}

impl ServerContinuousReceiveLoopTickBoundary {
    pub fn plan_next(
        &self,
        input: ServerContinuousReceiveLoopLifecycleInput,
    ) -> ServerContinuousReceiveLoopTickPlan {
        let lifecycle = self.lifecycle.plan_next(input);
        let state = match lifecycle.state {
            ServerContinuousReceiveLoopLifecycleState::Stopped => {
                ServerContinuousReceiveLoopTickState::Stopped
            }
            ServerContinuousReceiveLoopLifecycleState::AwaitingSocketReceive => {
                ServerContinuousReceiveLoopTickState::AwaitingSocketReceive
            }
            ServerContinuousReceiveLoopLifecycleState::ProcessingReceivedPacket => {
                ServerContinuousReceiveLoopTickState::ReceivedPacketReadyForDecode
            }
            ServerContinuousReceiveLoopLifecycleState::DispatchingAcceptedRoute => {
                ServerContinuousReceiveLoopTickState::AcceptedRouteReadyForHandoff
            }
            ServerContinuousReceiveLoopLifecycleState::PreparingRejectionLogs => {
                ServerContinuousReceiveLoopTickState::RejectionReadyForLogs
            }
            ServerContinuousReceiveLoopLifecycleState::SocketReceiveFailed => {
                ServerContinuousReceiveLoopTickState::SocketReceiveFailed
            }
        };

        ServerContinuousReceiveLoopTickPlan {
            state,
            lifecycle,
            packet_len: None,
        }
    }

    pub fn observe_received_packet(
        &self,
        packet_len: usize,
    ) -> ServerContinuousReceiveLoopTickPlan {
        ServerContinuousReceiveLoopTickPlan {
            state: ServerContinuousReceiveLoopTickState::ReceivedPacketReadyForDecode,
            lifecycle: self.lifecycle.plan_received_packet(),
            packet_len: Some(packet_len),
        }
    }

    pub fn observe_gate_outcome(
        &self,
        packet_len: usize,
        outcome: &ServerReceiveLoopGateOutcome,
    ) -> ServerContinuousReceiveLoopTickPlan {
        let lifecycle = self.lifecycle.plan_after_gate_outcome(outcome);
        let state = match outcome {
            ServerReceiveLoopGateOutcome::Accepted(_) => {
                ServerContinuousReceiveLoopTickState::AcceptedRouteReadyForHandoff
            }
            ServerReceiveLoopGateOutcome::Rejected(_) => {
                ServerContinuousReceiveLoopTickState::RejectionReadyForLogs
            }
        };

        ServerContinuousReceiveLoopTickPlan {
            state,
            lifecycle,
            packet_len: Some(packet_len),
        }
    }

    pub fn observe_socket_receive_error(&self) -> ServerContinuousReceiveLoopTickPlan {
        ServerContinuousReceiveLoopTickPlan {
            state: ServerContinuousReceiveLoopTickState::SocketReceiveFailed,
            lifecycle: self.lifecycle.plan_socket_receive_error(),
            packet_len: None,
        }
    }
}

/// Planned writer / handler handoff after one receive-loop tick outcome.
///
/// This is the connection shape between a future receive tick and existing
/// operational / rejection writer boundaries. It does not write logs or run
/// handlers by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopWriterHandoffPlan {
    pub tick: ServerContinuousReceiveLoopTickPlan,
    pub operational_log: Option<ServerReceiveLoopLogInput>,
    pub rejection_log: Option<ServerReceiveLoopGateRejection>,
    pub handler_handoff_required: bool,
}

/// Boundary that plans log-writer handoff after one receive-loop tick outcome.
///
/// This boundary does not call JSON Lines writers, open sinks, dispatch
/// handlers, drop packets, retry, or run the continuous loop. It only prepares
/// typed inputs for the existing operational and rejection logging boundaries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopWriterHandoffBoundary {
    tick: ServerContinuousReceiveLoopTickBoundary,
    operational_log: ServerReceiveLoopLogHandoffBoundary,
}

impl ServerContinuousReceiveLoopWriterHandoffBoundary {
    pub fn plan_after_gate_outcome(
        &self,
        packet_len: usize,
        outcome: &ServerReceiveLoopGateOutcome,
    ) -> ServerContinuousReceiveLoopWriterHandoffPlan {
        let tick = self.tick.observe_gate_outcome(packet_len, outcome);
        let operational_log = tick
            .lifecycle
            .operational_log_required
            .then(|| self.operational_log.handoff(outcome, packet_len));
        let rejection_log = match outcome {
            ServerReceiveLoopGateOutcome::Rejected(rejection)
                if tick.lifecycle.rejection_log_required =>
            {
                Some(rejection.clone())
            }
            _ => None,
        };

        ServerContinuousReceiveLoopWriterHandoffPlan {
            handler_handoff_required: tick.lifecycle.handler_handoff_required,
            tick,
            operational_log,
            rejection_log,
        }
    }
}

/// Result of calling caller-owned writers for one receive-loop gate outcome.
///
/// This keeps the handoff plan next to the written event inputs so a future
/// loop can inspect what was emitted without depending on a process-wide logger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopWriterRuntimeResult {
    pub handoff: ServerContinuousReceiveLoopWriterHandoffPlan,
    pub operational_event: Option<ServerReceiveLoopJsonLogEventInput>,
    pub rejection_event: Option<ServerReceiveRejectionJsonLogEventInput>,
}

/// Runtime boundary that connects one receive-loop outcome to caller-owned
/// operational / rejection JSON Lines writers.
///
/// This is still not a continuous loop and does not own sink selection, file
/// opening, process-wide logging, handler dispatch, packet drop, or async I/O.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopWriterRuntimeBoundary {
    handoff: ServerContinuousReceiveLoopWriterHandoffBoundary,
    operational_output: ServerReceiveLoopLogOutputBoundary,
    rejection_output: ServerReceiveRejectionLogOutputBoundary,
}

impl ServerContinuousReceiveLoopWriterRuntimeBoundary {
    pub fn write_after_gate_outcome<OW: io::Write, RW: io::Write>(
        &self,
        packet_len: usize,
        outcome: &ServerReceiveLoopGateOutcome,
        timestamp: TimestampMicros,
        mut operational_writer: OW,
        mut rejection_writer: RW,
    ) -> io::Result<ServerContinuousReceiveLoopWriterRuntimeResult> {
        let handoff = self.handoff.plan_after_gate_outcome(packet_len, outcome);

        let operational_event = if handoff.operational_log.is_some() {
            Some(self.operational_output.write_receive_loop_event(
                outcome,
                packet_len,
                timestamp,
                &mut operational_writer,
            )?)
        } else {
            None
        };

        let rejection_event = match handoff.rejection_log.clone() {
            Some(rejection) => Some(self.rejection_output.write_rejection(
                rejection,
                timestamp,
                &mut rejection_writer,
            )?),
            None => None,
        };

        Ok(ServerContinuousReceiveLoopWriterRuntimeResult {
            handoff,
            operational_event,
            rejection_event,
        })
    }
}

/// Reason no handler handoff is produced after a receive-loop tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerContinuousReceiveLoopHandlerHandoffSkipReason {
    RejectedOutcome,
    HandlerHandoffNotRequired,
}

/// Planned handler handoff after writer runtime processing.
///
/// This names the exact runtime bridge before the future continuous loop calls
/// real handlers. It does not execute auth decisions, update heartbeat/video
/// state, drop packets, enqueue outbound responses, or own log sinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerContinuousReceiveLoopHandlerHandoffRuntimePlan {
    Auth(ServerAuthCheck),
    RegisteredClient(ServerRegisteredClientPacket),
    Unsupported {
        source: PacketSource,
        message_type: MessageType,
    },
    AuthError(ServerAuthBoundaryError),
    RegisteredPacketError(ServerRegisteredPacketBoundaryError),
    NotRequired(ServerContinuousReceiveLoopHandlerHandoffSkipReason),
}

/// Result of connecting one receive-loop outcome to writers and handler input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopHandlerHandoffRuntimeResult {
    pub writer: ServerContinuousReceiveLoopWriterRuntimeResult,
    pub handler: ServerContinuousReceiveLoopHandlerHandoffRuntimePlan,
}

/// Runtime boundary that connects writer output to the next handler input.
///
/// The caller still owns socket receive, loop lifecycle, writer sink selection,
/// handler execution, packet drop, and retry decisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary {
    writer: ServerContinuousReceiveLoopWriterRuntimeBoundary,
    auth_handler: ServerAuthHandlerBoundary,
    registered_handler: ServerRegisteredPacketBoundary,
}

impl ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary {
    pub fn handoff_after_gate_outcome<OW: io::Write, RW: io::Write>(
        &self,
        registry: &AuthenticatedSenderRegistry,
        packet_len: usize,
        outcome: ServerReceiveLoopGateOutcome,
        timestamp: TimestampMicros,
        operational_writer: OW,
        rejection_writer: RW,
    ) -> io::Result<ServerContinuousReceiveLoopHandlerHandoffRuntimeResult> {
        let writer = self.writer.write_after_gate_outcome(
            packet_len,
            &outcome,
            timestamp,
            operational_writer,
            rejection_writer,
        )?;
        let handler = match outcome {
            ServerReceiveLoopGateOutcome::Accepted(route)
                if writer.handoff.handler_handoff_required =>
            {
                self.prepare_handler_handoff(registry, route)
            }
            ServerReceiveLoopGateOutcome::Accepted(_) => {
                ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::NotRequired(
                    ServerContinuousReceiveLoopHandlerHandoffSkipReason::HandlerHandoffNotRequired,
                )
            }
            ServerReceiveLoopGateOutcome::Rejected(_) => {
                ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::NotRequired(
                    ServerContinuousReceiveLoopHandlerHandoffSkipReason::RejectedOutcome,
                )
            }
        };

        Ok(ServerContinuousReceiveLoopHandlerHandoffRuntimeResult { writer, handler })
    }

    fn prepare_handler_handoff(
        &self,
        registry: &AuthenticatedSenderRegistry,
        route: ServerInboundRoute,
    ) -> ServerContinuousReceiveLoopHandlerHandoffRuntimePlan {
        match route {
            ServerInboundRoute::AuthRequest { .. } => self
                .auth_handler
                .prepare_from_route(route)
                .map(ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Auth)
                .unwrap_or_else(ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::AuthError),
            ServerInboundRoute::Heartbeat { .. }
            | ServerInboundRoute::VideoFrame { .. }
            | ServerInboundRoute::VideoFrameFragment { .. }
            | ServerInboundRoute::ClientStats { .. } => self
                .registered_handler
                .prepare_for_handler(registry, route)
                .map(ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient)
                .unwrap_or_else(
                    ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredPacketError,
                ),
            ServerInboundRoute::UnsupportedForServer {
                source,
                message_type,
                ..
            } => ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Unsupported {
                source,
                message_type,
            },
        }
    }
}

/// Runtime input for executing a single continuous receive-loop tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopOneTickRuntimeInput {
    pub expected_protocol_version: ProtocolVersion,
    pub timestamp: TimestampMicros,
    pub stop_requested: bool,
}

/// Outcome of the minimal one-tick receive-loop runtime connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerContinuousReceiveLoopOneTickRuntimeOutcome {
    Stopped,
    SocketReceiveFailed {
        socket_error_tick: ServerContinuousReceiveLoopTickPlan,
        error_kind: io::ErrorKind,
    },
    Completed {
        packet_len: usize,
        handler: ServerContinuousReceiveLoopHandlerHandoffRuntimeResult,
    },
}

/// Result of one receive-loop runtime tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopOneTickRuntimeResult {
    pub start_tick: ServerContinuousReceiveLoopTickPlan,
    pub outcome: ServerContinuousReceiveLoopOneTickRuntimeOutcome,
}

/// Minimal runtime boundary for executing one synchronous receive-loop tick.
///
/// This composes stop planning, one socket receive, decode / gate, writer
/// runtime, and handler handoff runtime. It does not run a loop, dispatch
/// handlers, drop packets, open file sinks, install logging, or spawn async
/// work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopOneTickRuntimeBoundary {
    tick: ServerContinuousReceiveLoopTickBoundary,
    socket_io: ServerUdpSocketIoStep,
    handler_handoff: ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary,
}

impl ServerContinuousReceiveLoopOneTickRuntimeBoundary {
    pub fn execute_one_tick<OW: io::Write, RW: io::Write>(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        registry: &AuthenticatedSenderRegistry,
        input: ServerContinuousReceiveLoopOneTickRuntimeInput,
        operational_writer: OW,
        rejection_writer: RW,
    ) -> io::Result<ServerContinuousReceiveLoopOneTickRuntimeResult> {
        let start_tick = self
            .tick
            .plan_next(ServerContinuousReceiveLoopLifecycleInput {
                stop_requested: input.stop_requested,
            });

        if input.stop_requested {
            return Ok(ServerContinuousReceiveLoopOneTickRuntimeResult {
                start_tick,
                outcome: ServerContinuousReceiveLoopOneTickRuntimeOutcome::Stopped,
            });
        }

        let received = match self.socket_io.receive_one_with_gate_details(
            socket,
            buffer,
            input.expected_protocol_version,
            registry,
        ) {
            Ok(received) => received,
            Err(error) => {
                return Ok(ServerContinuousReceiveLoopOneTickRuntimeResult {
                    start_tick,
                    outcome:
                        ServerContinuousReceiveLoopOneTickRuntimeOutcome::SocketReceiveFailed {
                            socket_error_tick: self.tick.observe_socket_receive_error(),
                            error_kind: error.kind(),
                        },
                });
            }
        };

        let handler = self.handler_handoff.handoff_after_gate_outcome(
            registry,
            received.packet_len,
            received.outcome,
            input.timestamp,
            operational_writer,
            rejection_writer,
        )?;

        Ok(ServerContinuousReceiveLoopOneTickRuntimeResult {
            start_tick,
            outcome: ServerContinuousReceiveLoopOneTickRuntimeOutcome::Completed {
                packet_len: received.packet_len,
                handler,
            },
        })
    }
}

/// Input for one minimal continuous receive-loop body iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopBodyInput {
    pub expected_protocol_version: ProtocolVersion,
    pub timestamp: TimestampMicros,
    pub stop_requested: bool,
}

/// Action selected by the minimal loop body before delegating to one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerContinuousReceiveLoopBodyAction {
    Stop,
    ExecuteOneTick,
}

/// Result of one minimal loop body iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopBodyResult {
    pub action: ServerContinuousReceiveLoopBodyAction,
    pub tick: ServerContinuousReceiveLoopOneTickRuntimeResult,
}

/// Minimal continuous receive-loop body boundary.
///
/// This is one loop body iteration only: it evaluates the stop flag and
/// delegates to the one-tick runtime. It does not repeat, dispatch handlers,
/// drop packets, open log files, install process-wide logging, or own retry /
/// backoff policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopBodyBoundary {
    one_tick: ServerContinuousReceiveLoopOneTickRuntimeBoundary,
}

impl ServerContinuousReceiveLoopBodyBoundary {
    pub fn run_once<OW: io::Write, RW: io::Write>(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        registry: &AuthenticatedSenderRegistry,
        input: ServerContinuousReceiveLoopBodyInput,
        operational_writer: OW,
        rejection_writer: RW,
    ) -> io::Result<ServerContinuousReceiveLoopBodyResult> {
        let action = if input.stop_requested {
            ServerContinuousReceiveLoopBodyAction::Stop
        } else {
            ServerContinuousReceiveLoopBodyAction::ExecuteOneTick
        };
        let tick = self.one_tick.execute_one_tick(
            socket,
            buffer,
            registry,
            ServerContinuousReceiveLoopOneTickRuntimeInput {
                expected_protocol_version: input.expected_protocol_version,
                timestamp: input.timestamp,
                stop_requested: input.stop_requested,
            },
            operational_writer,
            rejection_writer,
        )?;

        Ok(ServerContinuousReceiveLoopBodyResult { action, tick })
    }
}

/// Lifecycle state for the future outer continuous receive-loop controller.
///
/// The controller boundary names only the outer orchestration checkpoints
/// around repeated body iterations. It is deliberately separate from the
/// one-iteration body and does not implement a `while` loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerContinuousReceiveLoopControllerState {
    Stopped,
    ReadyToRunBodyOnce,
    BodyIterationCompleted,
    BodyIterationFailed,
}

/// Action selected by the future outer continuous receive-loop controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerContinuousReceiveLoopControllerAction {
    Stop,
    RunBodyOnce,
    YieldToCaller,
    DeferErrorPolicy,
}

/// Input for planning the next outer controller iteration.
///
/// `continue_requested` is intentionally caller-owned. A future shutdown
/// policy may compute it from signals, errors, or operator state, but this
/// placeholder only consumes the already-decided boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopControllerInput {
    pub expected_protocol_version: ProtocolVersion,
    pub timestamp: TimestampMicros,
    pub continue_requested: bool,
}

/// Plan for the next receive-loop body iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopControllerPlan {
    pub state: ServerContinuousReceiveLoopControllerState,
    pub action: ServerContinuousReceiveLoopControllerAction,
    pub body_input: Option<ServerContinuousReceiveLoopBodyInput>,
}

/// Observation after one body iteration returns to the controller caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerContinuousReceiveLoopControllerObservation {
    pub state: ServerContinuousReceiveLoopControllerState,
    pub action: ServerContinuousReceiveLoopControllerAction,
}

/// Minimal boundary for the future outer continuous receive-loop controller.
///
/// This boundary does not run a continuous loop. It only decides whether the
/// caller should execute one body iteration and classifies the returned body
/// result. The caller still owns repeated invocation, shutdown policy, handler
/// dispatch, packet drop side effects, file sink lifecycle, process-wide
/// logging, retry / backoff, and timestamp generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopControllerBoundary;

impl ServerContinuousReceiveLoopControllerBoundary {
    pub fn plan_next_iteration(
        &self,
        input: ServerContinuousReceiveLoopControllerInput,
    ) -> ServerContinuousReceiveLoopControllerPlan {
        if !input.continue_requested {
            return ServerContinuousReceiveLoopControllerPlan {
                state: ServerContinuousReceiveLoopControllerState::Stopped,
                action: ServerContinuousReceiveLoopControllerAction::Stop,
                body_input: None,
            };
        }

        ServerContinuousReceiveLoopControllerPlan {
            state: ServerContinuousReceiveLoopControllerState::ReadyToRunBodyOnce,
            action: ServerContinuousReceiveLoopControllerAction::RunBodyOnce,
            body_input: Some(ServerContinuousReceiveLoopBodyInput {
                expected_protocol_version: input.expected_protocol_version,
                timestamp: input.timestamp,
                stop_requested: false,
            }),
        }
    }

    pub fn observe_body_result(
        &self,
        result: &ServerContinuousReceiveLoopBodyResult,
    ) -> ServerContinuousReceiveLoopControllerObservation {
        match result.tick.outcome {
            ServerContinuousReceiveLoopOneTickRuntimeOutcome::Stopped => {
                ServerContinuousReceiveLoopControllerObservation {
                    state: ServerContinuousReceiveLoopControllerState::Stopped,
                    action: ServerContinuousReceiveLoopControllerAction::Stop,
                }
            }
            ServerContinuousReceiveLoopOneTickRuntimeOutcome::SocketReceiveFailed { .. } => {
                ServerContinuousReceiveLoopControllerObservation {
                    state: ServerContinuousReceiveLoopControllerState::BodyIterationFailed,
                    action: ServerContinuousReceiveLoopControllerAction::DeferErrorPolicy,
                }
            }
            ServerContinuousReceiveLoopOneTickRuntimeOutcome::Completed { .. } => {
                ServerContinuousReceiveLoopControllerObservation {
                    state: ServerContinuousReceiveLoopControllerState::BodyIterationCompleted,
                    action: ServerContinuousReceiveLoopControllerAction::YieldToCaller,
                }
            }
        }
    }
}

/// Reason the receive-loop dispatch bridge does not produce handler work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerContinuousReceiveLoopHandlerDispatchSkipReason {
    LoopStopped,
    SocketReceiveFailed,
    RejectedOutcome,
    HandlerHandoffNotRequired,
}

/// Error carried to a future handler dispatch layer.
///
/// The dispatch bridge preserves preparation errors but does not execute error
/// policy, write logs, or drop packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerContinuousReceiveLoopHandlerDispatchError {
    Auth(ServerAuthBoundaryError),
    RegisteredPacket(ServerRegisteredPacketBoundaryError),
}

/// Minimal handoff plan from the continuous receive loop to future handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerContinuousReceiveLoopHandlerDispatchPlan {
    Auth(ServerAuthCheck),
    RegisteredClient(ServerRegisteredClientPacket),
    Unsupported {
        source: PacketSource,
        message_type: MessageType,
    },
    NotRequired(ServerContinuousReceiveLoopHandlerDispatchSkipReason),
    HandoffError(ServerContinuousReceiveLoopHandlerDispatchError),
}

/// Handoff from one receive-loop body result to the future dispatch layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopHandlerDispatchHandoff {
    pub packet_len: Option<usize>,
    pub plan: ServerContinuousReceiveLoopHandlerDispatchPlan,
}

/// Boundary between the continuous receive loop and future handler dispatch.
///
/// This consumes the handler handoff result produced by the one-tick runtime
/// and shapes it for a future dispatch body. It does not run auth decisions,
/// heartbeat / video / stats handlers, outbound enqueue, packet drop, sink
/// selection, retry, shutdown policy, or async work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopHandlerDispatchBoundary;

impl ServerContinuousReceiveLoopHandlerDispatchBoundary {
    pub fn plan_from_body_result(
        &self,
        result: &ServerContinuousReceiveLoopBodyResult,
    ) -> ServerContinuousReceiveLoopHandlerDispatchHandoff {
        match &result.tick.outcome {
            ServerContinuousReceiveLoopOneTickRuntimeOutcome::Stopped => {
                ServerContinuousReceiveLoopHandlerDispatchHandoff {
                    packet_len: None,
                    plan: ServerContinuousReceiveLoopHandlerDispatchPlan::NotRequired(
                        ServerContinuousReceiveLoopHandlerDispatchSkipReason::LoopStopped,
                    ),
                }
            }
            ServerContinuousReceiveLoopOneTickRuntimeOutcome::SocketReceiveFailed { .. } => {
                ServerContinuousReceiveLoopHandlerDispatchHandoff {
                    packet_len: None,
                    plan: ServerContinuousReceiveLoopHandlerDispatchPlan::NotRequired(
                        ServerContinuousReceiveLoopHandlerDispatchSkipReason::SocketReceiveFailed,
                    ),
                }
            }
            ServerContinuousReceiveLoopOneTickRuntimeOutcome::Completed {
                packet_len,
                handler,
            } => ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: Some(*packet_len),
                plan: self.plan_from_handler_handoff(&handler.handler),
            },
        }
    }

    pub fn plan_from_handler_handoff(
        &self,
        handler: &ServerContinuousReceiveLoopHandlerHandoffRuntimePlan,
    ) -> ServerContinuousReceiveLoopHandlerDispatchPlan {
        match handler {
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Auth(auth) => {
                ServerContinuousReceiveLoopHandlerDispatchPlan::Auth(auth.clone())
            }
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(packet) => {
                ServerContinuousReceiveLoopHandlerDispatchPlan::RegisteredClient(packet.clone())
            }
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Unsupported {
                source,
                message_type,
            } => ServerContinuousReceiveLoopHandlerDispatchPlan::Unsupported {
                source: *source,
                message_type: *message_type,
            },
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::AuthError(error) => {
                ServerContinuousReceiveLoopHandlerDispatchPlan::HandoffError(
                    ServerContinuousReceiveLoopHandlerDispatchError::Auth(error.clone()),
                )
            }
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredPacketError(error) => {
                ServerContinuousReceiveLoopHandlerDispatchPlan::HandoffError(
                    ServerContinuousReceiveLoopHandlerDispatchError::RegisteredPacket(
                        error.clone(),
                    ),
                )
            }
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::NotRequired(reason) => {
                ServerContinuousReceiveLoopHandlerDispatchPlan::NotRequired(match reason {
                    ServerContinuousReceiveLoopHandlerHandoffSkipReason::RejectedOutcome => {
                        ServerContinuousReceiveLoopHandlerDispatchSkipReason::RejectedOutcome
                    }
                    ServerContinuousReceiveLoopHandlerHandoffSkipReason::HandlerHandoffNotRequired => {
                        ServerContinuousReceiveLoopHandlerDispatchSkipReason::HandlerHandoffNotRequired
                    }
                })
            }
        }
    }
}

/// Result of the minimal server handler dispatch body.
///
/// This is a typed classification result only. It preserves the work that a
/// future concrete handler will own without executing auth decisions,
/// heartbeat / video / stats handling, outbound enqueue, or packet drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHandlerDispatchResult {
    Auth(ServerAuthCheck),
    RegisteredHeartbeat(ServerRegisteredHeartbeatPacket),
    RegisteredVideoFrame(ServerRegisteredVideoFramePacket),
    RegisteredVideoFrameFragment(ServerRegisteredVideoFrameFragmentPacket),
    RegisteredClientStats(ServerRegisteredClientStatsPacket),
    Unsupported {
        source: PacketSource,
        message_type: MessageType,
    },
    NotRequired(ServerContinuousReceiveLoopHandlerDispatchSkipReason),
    HandoffError(ServerContinuousReceiveLoopHandlerDispatchError),
}

/// Outcome of dispatching one receive-loop handoff into a handler lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHandlerDispatchOutcome {
    pub packet_len: Option<usize>,
    pub result: ServerHandlerDispatchResult,
}

/// Minimal handler dispatch body boundary.
///
/// The current implementation only separates handler lanes after the
/// continuous receive loop bridge. Concrete auth flow execution, registered
/// packet handlers, outbound enqueue, and stats state commits remain future
/// responsibilities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerHandlerDispatchBoundary;

impl ServerHandlerDispatchBoundary {
    pub fn dispatch_handoff(
        &self,
        handoff: ServerContinuousReceiveLoopHandlerDispatchHandoff,
    ) -> ServerHandlerDispatchOutcome {
        ServerHandlerDispatchOutcome {
            packet_len: handoff.packet_len,
            result: self.dispatch_plan(handoff.plan),
        }
    }

    pub fn dispatch_plan(
        &self,
        plan: ServerContinuousReceiveLoopHandlerDispatchPlan,
    ) -> ServerHandlerDispatchResult {
        match plan {
            ServerContinuousReceiveLoopHandlerDispatchPlan::Auth(auth) => {
                ServerHandlerDispatchResult::Auth(auth)
            }
            ServerContinuousReceiveLoopHandlerDispatchPlan::RegisteredClient(packet) => {
                match packet {
                    ServerRegisteredClientPacket::Heartbeat(packet) => {
                        ServerHandlerDispatchResult::RegisteredHeartbeat(packet)
                    }
                    ServerRegisteredClientPacket::VideoFrame(packet) => {
                        ServerHandlerDispatchResult::RegisteredVideoFrame(packet)
                    }
                    ServerRegisteredClientPacket::VideoFrameFragment(packet) => {
                        ServerHandlerDispatchResult::RegisteredVideoFrameFragment(packet)
                    }
                    ServerRegisteredClientPacket::ClientStats(packet) => {
                        ServerHandlerDispatchResult::RegisteredClientStats(packet)
                    }
                }
            }
            ServerContinuousReceiveLoopHandlerDispatchPlan::Unsupported {
                source,
                message_type,
            } => ServerHandlerDispatchResult::Unsupported {
                source,
                message_type,
            },
            ServerContinuousReceiveLoopHandlerDispatchPlan::NotRequired(reason) => {
                ServerHandlerDispatchResult::NotRequired(reason)
            }
            ServerContinuousReceiveLoopHandlerDispatchPlan::HandoffError(error) => {
                ServerHandlerDispatchResult::HandoffError(error)
            }
        }
    }
}

/// Result of running the minimal auth dispatch runtime.
///
/// `Dispatched` means the existing auth flow step produced decision, log
/// handoff, registry registration handoff, and outbound queue handoff. Non-auth
/// handler lanes are preserved for a different dispatch runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerAuthDispatchRuntimeResult {
    Dispatched(ServerAuthFlowOutcome),
    NotAuth(ServerHandlerDispatchResult),
}

/// Outcome of attempting auth dispatch for one handler dispatch result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthDispatchRuntimeOutcome {
    pub packet_len: Option<usize>,
    pub result: ServerAuthDispatchRuntimeResult,
}

/// Minimal runtime connection from handler dispatch to the auth flow step.
///
/// This boundary calls auth flow only for `ServerHandlerDispatchResult::Auth`.
/// It does not register authenticated sources, write logs, persist queue items,
/// encode bytes, send UDP, run packet drop policy, or own the future loop body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthDispatchRuntimeBoundary {
    auth_flow: ServerAuthFlowStep,
}

impl ServerAuthDispatchRuntimeBoundary {
    pub fn dispatch_outcome(
        &self,
        outcome: ServerHandlerDispatchOutcome,
        config: &ServerAuthConfig,
    ) -> ServerAuthDispatchRuntimeOutcome {
        let result = match outcome.result {
            ServerHandlerDispatchResult::Auth(check) => {
                ServerAuthDispatchRuntimeResult::Dispatched(
                    self.auth_flow.handle_auth_check(check, config),
                )
            }
            other => ServerAuthDispatchRuntimeResult::NotAuth(other),
        };

        ServerAuthDispatchRuntimeOutcome {
            packet_len: outcome.packet_len,
            result,
        }
    }
}

/// Result of running the minimal registered packet dispatch runtime.
///
/// Heartbeat is connected to the existing ack handoff boundary. Video frame
/// and client stats lanes are preserved for future concrete handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerRegisteredPacketDispatchRuntimeResult {
    HeartbeatAck(ServerHeartbeatAckHandoff),
    FutureVideoFrame(ServerRegisteredVideoFramePacket),
    FutureVideoFrameFragment(ServerRegisteredVideoFrameFragmentPacket),
    FutureClientStats(ServerRegisteredClientStatsPacket),
    NotRegistered(ServerHandlerDispatchResult),
}

/// Outcome of attempting registered packet dispatch for one handler result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRegisteredPacketDispatchRuntimeOutcome {
    pub packet_len: Option<usize>,
    pub result: ServerRegisteredPacketDispatchRuntimeResult,
}

/// Minimal runtime connection from handler dispatch to registered handlers.
///
/// This boundary connects heartbeat to the existing ack handoff only. It does
/// not update heartbeat state, buffer video frames, commit stats, persist queue
/// items, encode bytes, send UDP, run packet drop policy, or own the future
/// loop body. Heartbeat timing is caller-owned to avoid clock/runtime policy in
/// this dispatch layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerRegisteredPacketDispatchRuntimeBoundary {
    heartbeat: ServerHeartbeatHandlerBoundary,
}

impl ServerRegisteredPacketDispatchRuntimeBoundary {
    pub fn dispatch_outcome(
        &self,
        outcome: ServerHandlerDispatchOutcome,
        heartbeat_timing: ServerHeartbeatAckTiming,
    ) -> ServerRegisteredPacketDispatchRuntimeOutcome {
        let result = match outcome.result {
            ServerHandlerDispatchResult::RegisteredHeartbeat(packet) => {
                ServerRegisteredPacketDispatchRuntimeResult::HeartbeatAck(
                    self.heartbeat.handoff_ack(packet, heartbeat_timing),
                )
            }
            ServerHandlerDispatchResult::RegisteredVideoFrame(packet) => {
                ServerRegisteredPacketDispatchRuntimeResult::FutureVideoFrame(packet)
            }
            ServerHandlerDispatchResult::RegisteredVideoFrameFragment(packet) => {
                ServerRegisteredPacketDispatchRuntimeResult::FutureVideoFrameFragment(packet)
            }
            ServerHandlerDispatchResult::RegisteredClientStats(packet) => {
                ServerRegisteredPacketDispatchRuntimeResult::FutureClientStats(packet)
            }
            other => ServerRegisteredPacketDispatchRuntimeResult::NotRegistered(other),
        };

        ServerRegisteredPacketDispatchRuntimeOutcome {
            packet_len: outcome.packet_len,
            result,
        }
    }
}

/// Minimal video frame handler input.
///
/// This preserves the authenticated registered packet and records the payload
/// byte length for later buffering policy. It does not decode H.264, enqueue a
/// frame, run sync scheduling, or apply video drop policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVideoFrameHandlerInput {
    pub registered_packet: ServerRegisteredVideoFramePacket,
    pub payload_len: usize,
}

/// Minimal video frame handler boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerVideoFrameHandlerBoundary;

impl ServerVideoFrameHandlerBoundary {
    pub fn prepare_input(
        &self,
        packet: ServerRegisteredVideoFramePacket,
    ) -> ServerVideoFrameHandlerInput {
        let payload_len = packet.frame.payload.len();
        ServerVideoFrameHandlerInput {
            registered_packet: packet,
            payload_len,
        }
    }
}

/// Per-client storage policy for the first single-view video PoC queue.
///
/// The default keeps a tiny live-video queue. When a client's queue is full,
/// the oldest frame is dropped before storing the newest one because stale
/// frames are less useful for a live single-view PoC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerVideoFrameQueuePolicy {
    pub max_frames_per_client: usize,
}

impl Default for ServerVideoFrameQueuePolicy {
    fn default() -> Self {
        Self {
            max_frames_per_client: 8,
        }
    }
}

/// One accepted video frame stored for later sync/display handoff.
///
/// This is still encoded frame data. The queue does not decode H.264, select a
/// target time, or render anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerQueuedVideoFrame {
    pub source: PacketSource,
    pub authenticated_sender: AuthenticatedSenderEntry,
    pub frame: VideoFrame,
    pub payload_len: usize,
    pub queued_at: TimestampMicros,
}

/// Caller-owned per-client frame queue state for the first video PoC slice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerVideoFrameQueueState {
    frames_by_client_id: BTreeMap<String, VecDeque<ServerQueuedVideoFrame>>,
}

impl ServerVideoFrameQueueState {
    pub fn client_queue_len(&self, client_id: &ClientId) -> usize {
        self.frames_by_client_id
            .get(&client_id.0)
            .map(VecDeque::len)
            .unwrap_or(0)
    }

    pub fn total_len(&self) -> usize {
        self.frames_by_client_id.values().map(VecDeque::len).sum()
    }

    pub fn frames_for_client(
        &self,
        client_id: &ClientId,
    ) -> impl Iterator<Item = &ServerQueuedVideoFrame> {
        self.frames_by_client_id
            .get(&client_id.0)
            .into_iter()
            .flat_map(|frames| frames.iter())
    }

    pub fn frames_for_client_run<'a>(
        &'a self,
        client_id: &'a ClientId,
        run_id: &'a RunId,
    ) -> impl Iterator<Item = &'a ServerQueuedVideoFrame> + 'a {
        self.frames_for_client(client_id)
            .filter(move |queued| queued.frame.run_id == *run_id)
    }

    pub fn pop_front(&mut self, client_id: &ClientId) -> Option<ServerQueuedVideoFrame> {
        let queue = self.frames_by_client_id.get_mut(&client_id.0)?;
        let frame = queue.pop_front();
        if queue.is_empty() {
            self.frames_by_client_id.remove(&client_id.0);
        }
        frame
    }

    pub fn pop_front_for_client_run(
        &mut self,
        client_id: &ClientId,
        run_id: &RunId,
    ) -> Option<ServerQueuedVideoFrame> {
        let queue = self.frames_by_client_id.get_mut(&client_id.0)?;
        let frame_index = queue
            .iter()
            .position(|queued| queued.frame.run_id == *run_id)?;
        let frame = queue.remove(frame_index);
        if queue.is_empty() {
            self.frames_by_client_id.remove(&client_id.0);
        }
        frame
    }
}

/// Read mode for the first server-side queue consumption boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerVideoFrameQueueReadMode {
    InspectOldest,
    InspectLatest,
    DequeueOldest,
}

/// Input for inspecting or dequeuing queued encoded video frames.
///
/// This boundary is intentionally keyed by client and run so future sync /
/// switcher callers do not accidentally consume frames from a previous manual
/// run that used the same client id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVideoFrameQueueReadInput {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub mode: ServerVideoFrameQueueReadMode,
}

/// Result of reading a queued encoded video frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerVideoFrameQueueReadResult {
    FrameAvailable {
        frame: ServerQueuedVideoFrame,
        mode: ServerVideoFrameQueueReadMode,
        remaining_client_queue_len: usize,
    },
    NoFrameAvailable {
        client_id: ClientId,
        run_id: RunId,
        client_queue_len: usize,
    },
}

/// Minimal server queue read/dequeue boundary for sync/switcher handoff.
///
/// This is in-process and diagnostic/manual for now. It reads encoded
/// `VideoFrame`s already accepted into caller-owned queue state. It does not
/// receive packets, reassemble fragments, decode H.264, choose targetTime,
/// mutate late frames, orchestrate four views, render UI, or touch OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerVideoFrameQueueReadBoundary;

impl ServerVideoFrameQueueReadBoundary {
    pub fn read(
        &self,
        state: &mut ServerVideoFrameQueueState,
        input: ServerVideoFrameQueueReadInput,
    ) -> ServerVideoFrameQueueReadResult {
        let frame = match input.mode {
            ServerVideoFrameQueueReadMode::InspectOldest => state
                .frames_for_client_run(&input.client_id, &input.run_id)
                .next()
                .cloned(),
            ServerVideoFrameQueueReadMode::InspectLatest => state
                .frames_for_client_run(&input.client_id, &input.run_id)
                .last()
                .cloned(),
            ServerVideoFrameQueueReadMode::DequeueOldest => {
                state.pop_front_for_client_run(&input.client_id, &input.run_id)
            }
        };

        match frame {
            Some(frame) => ServerVideoFrameQueueReadResult::FrameAvailable {
                frame,
                mode: input.mode,
                remaining_client_queue_len: state.client_queue_len(&input.client_id),
            },
            None => ServerVideoFrameQueueReadResult::NoFrameAvailable {
                client_queue_len: state.client_queue_len(&input.client_id),
                client_id: input.client_id,
                run_id: input.run_id,
            },
        }
    }
}

/// Single-request server-side handler for the first real server->switcher handoff.
///
/// This boundary stays transport-neutral. It consumes one decoded handoff
/// request, delegates the queue read to `ServerVideoFrameQueueReadBoundary`,
/// and returns one transport-neutral response DTO. It does not own named-pipe
/// runtime/service lifecycle, socket I/O, decode/render behavior, OBS output,
/// 4-view orchestration, or fragment reassembly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSwitcherQueuedFrameHandoffHandlerBoundary {
    queue_reader: ServerVideoFrameQueueReadBoundary,
}

impl ServerSwitcherQueuedFrameHandoffHandlerBoundary {
    pub fn handle_request(
        &self,
        queue_state: &mut ServerVideoFrameQueueState,
        request: ServerSwitcherQueuedFrameHandoffRequest,
    ) -> ServerSwitcherQueuedFrameHandoffResponse {
        if request.client_id.0.trim().is_empty() || request.run_id.0.trim().is_empty() {
            return ServerSwitcherQueuedFrameHandoffResponse::HandoffError {
                request_id: request.request_id,
                error: ServerSwitcherQueuedFrameHandoffErrorCode::InvalidScope,
            };
        }

        let result = self.queue_reader.read(
            queue_state,
            ServerVideoFrameQueueReadInput {
                client_id: request.client_id.clone(),
                run_id: request.run_id.clone(),
                mode: queue_read_mode_from_handoff(request.read_mode),
            },
        );

        match result {
            ServerVideoFrameQueueReadResult::FrameAvailable {
                frame,
                remaining_client_queue_len,
                ..
            } => ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
                request_id: request.request_id,
                remaining_client_queue_len: queue_len_to_u32(remaining_client_queue_len),
                frame: ServerSwitcherQueuedFrameHandoffFrame {
                    client_id: frame.frame.client_id,
                    run_id: frame.frame.run_id,
                    frame_id: frame.frame.frame_id,
                    capture_timestamp: frame.frame.capture_timestamp,
                    send_timestamp: frame.frame.send_timestamp,
                    queued_at: frame.queued_at,
                    width: frame.frame.width,
                    height: frame.frame.height,
                    fps_nominal: frame.frame.fps_nominal,
                    is_keyframe: frame.frame.is_keyframe,
                    codec: frame.frame.codec,
                    encoded_payload_len: payload_len_to_u32(frame.payload_len),
                    encoded_payload: frame.frame.payload,
                },
            },
            ServerVideoFrameQueueReadResult::NoFrameAvailable {
                client_id,
                run_id,
                client_queue_len,
            } => ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
                request_id: request.request_id,
                client_id,
                run_id,
                read_mode: request.read_mode,
                client_queue_len: queue_len_to_u32(client_queue_len),
            },
        }
    }
}

fn queue_read_mode_from_handoff(
    mode: ServerSwitcherQueuedFrameReadMode,
) -> ServerVideoFrameQueueReadMode {
    match mode {
        ServerSwitcherQueuedFrameReadMode::InspectOldest => {
            ServerVideoFrameQueueReadMode::InspectOldest
        }
        ServerSwitcherQueuedFrameReadMode::InspectLatest => {
            ServerVideoFrameQueueReadMode::InspectLatest
        }
        ServerSwitcherQueuedFrameReadMode::DequeueOldest => {
            ServerVideoFrameQueueReadMode::DequeueOldest
        }
    }
}

fn queue_len_to_u32(len: usize) -> u32 {
    u32::try_from(len).expect("server->switcher queue length must fit into u32")
}

fn payload_len_to_u32(len: usize) -> u32 {
    u32::try_from(len).expect("server->switcher payload length must fit into u32")
}

/// Reason a video frame was not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerVideoFrameQueueDropReason {
    CapacityZero,
}

/// Result of storing one accepted video frame into caller-owned queue state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerVideoFrameQueueStorageResult {
    Stored {
        queued: ServerQueuedVideoFrame,
        previous_client_queue_len: usize,
        current_client_queue_len: usize,
        dropped_oldest: Option<ServerQueuedVideoFrame>,
    },
    Dropped {
        input: ServerVideoFrameHandlerInput,
        reason: ServerVideoFrameQueueDropReason,
    },
}

/// Boundary that stores accepted video frames for the first single-view PoC.
///
/// This boundary consumes the existing authenticated video handler input and
/// mutates caller-owned queue state. It does not authenticate packets, decode
/// H.264, run sync scheduling, choose display frames, notify switcher, render
/// UI, send UDP, or touch OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerVideoFrameQueueStorageBoundary;

impl ServerVideoFrameQueueStorageBoundary {
    pub fn store_frame(
        &self,
        state: &mut ServerVideoFrameQueueState,
        input: ServerVideoFrameHandlerInput,
        queued_at: TimestampMicros,
        policy: ServerVideoFrameQueuePolicy,
    ) -> ServerVideoFrameQueueStorageResult {
        if policy.max_frames_per_client == 0 {
            return ServerVideoFrameQueueStorageResult::Dropped {
                input,
                reason: ServerVideoFrameQueueDropReason::CapacityZero,
            };
        }

        let client_id = input.registered_packet.frame.client_id.0.clone();
        let queue = state.frames_by_client_id.entry(client_id).or_default();
        let previous_client_queue_len = queue.len();
        let dropped_oldest = if queue.len() >= policy.max_frames_per_client {
            queue.pop_front()
        } else {
            None
        };
        let queued = ServerQueuedVideoFrame {
            source: input.registered_packet.source,
            authenticated_sender: input.registered_packet.authenticated_sender,
            frame: input.registered_packet.frame,
            payload_len: input.payload_len,
            queued_at,
        };
        queue.push_back(queued.clone());

        ServerVideoFrameQueueStorageResult::Stored {
            queued,
            previous_client_queue_len,
            current_client_queue_len: queue.len(),
            dropped_oldest,
        }
    }
}

/// Key for caller-owned server-side fragment reassembly state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServerVideoFrameReassemblyKey {
    pub client_id: String,
    pub run_id: String,
    pub frame_id: u64,
}

impl ServerVideoFrameReassemblyKey {
    fn from_fragment(fragment: &VideoFrameFragment) -> Self {
        Self {
            client_id: fragment.client_id.0.clone(),
            run_id: fragment.run_id.0.clone(),
            frame_id: fragment.frame_id,
        }
    }
}

/// Caller-owned state for incomplete server-side `VideoFrameFragment` frames.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerVideoFrameReassemblyState {
    frames: BTreeMap<ServerVideoFrameReassemblyKey, ServerVideoFrameReassemblyFrameState>,
}

impl ServerVideoFrameReassemblyState {
    pub fn tracked_frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn contains_frame(&self, key: &ServerVideoFrameReassemblyKey) -> bool {
        self.frames.contains_key(key)
    }

    pub fn frame_progress(&self) -> Vec<ServerVideoFrameReassemblyFrameProgress> {
        self.frames
            .iter()
            .map(|(key, state)| ServerVideoFrameReassemblyFrameProgress {
                key: key.clone(),
                fragments_received: state.fragments_received(),
                fragments_expected: state.chunk_count as usize,
                fragments_missing: state.missing_chunks().len(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerVideoFrameReassemblyFrameState {
    protocol_version: ProtocolVersion,
    capture_timestamp: TimestampMicros,
    width: u32,
    height: u32,
    fps_nominal: u32,
    total_payload_len: u32,
    chunk_count: u32,
    chunks: Vec<Option<Vec<u8>>>,
    duplicate_count: usize,
}

impl ServerVideoFrameReassemblyFrameState {
    fn new(fragment: &VideoFrameFragment) -> Self {
        Self {
            protocol_version: fragment.protocol_version,
            capture_timestamp: fragment.capture_timestamp,
            width: fragment.width,
            height: fragment.height,
            fps_nominal: fragment.fps_nominal,
            total_payload_len: fragment.total_payload_len,
            chunk_count: fragment.chunk_count,
            chunks: vec![None; fragment.chunk_count as usize],
            duplicate_count: 0,
        }
    }

    fn fragments_received(&self) -> usize {
        self.chunks.iter().filter(|chunk| chunk.is_some()).count()
    }

    fn missing_chunks(&self) -> Vec<u32> {
        self.chunks
            .iter()
            .enumerate()
            .filter_map(|(index, chunk)| chunk.is_none().then_some(index as u32))
            .collect()
    }

    fn is_complete(&self) -> bool {
        self.fragments_received() == self.chunk_count as usize
    }

    fn reassemble_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(self.total_payload_len as usize);
        for chunk in &self.chunks {
            if let Some(chunk) = chunk {
                payload.extend_from_slice(chunk);
            }
        }
        payload
    }
}

/// Reassembly result summary intended for logs / CLI diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVideoFrameReassemblySummary {
    pub key: ServerVideoFrameReassemblyKey,
    pub fragments_received: usize,
    pub fragments_missing: Vec<u32>,
    pub completed_frame_queued: bool,
    pub rejected_fragment_reason: Option<ServerVideoFrameFragmentRejectReason>,
    pub duplicate_count: usize,
}

/// Reason an accepted/authenticated fragment was rejected by reassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerVideoFrameFragmentRejectReason {
    ChunkCountZero,
    ChunkIndexOutOfRange,
    ChunkPayloadLenMismatch,
    MetadataMismatch,
    ReassembledPayloadLenMismatch,
}

/// Result of applying one authenticated fragment to caller-owned reassembly state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerVideoFrameReassemblyApplyResult {
    FragmentStored {
        summary: ServerVideoFrameReassemblySummary,
    },
    DuplicateFragmentIgnored {
        summary: ServerVideoFrameReassemblySummary,
    },
    RejectedFragment {
        summary: ServerVideoFrameReassemblySummary,
    },
    FrameComplete {
        summary: ServerVideoFrameReassemblySummary,
        reassembled_frame: VideoFrame,
        queue_result: ServerVideoFrameQueueStorageResult,
    },
}

/// Boundary that reassembles authenticated `VideoFrameFragment` packets.
///
/// The caller owns the reassembly state and queue state. This boundary does
/// not authenticate packets, decode H.264, retry fragments, expire frames,
/// mutate late-frame queues, run 4-view sync, notify switcher, or touch OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerVideoFrameFragmentReassemblyBoundary {
    storage: ServerVideoFrameQueueStorageBoundary,
}

impl ServerVideoFrameFragmentReassemblyBoundary {
    pub fn apply_fragment_and_queue_if_complete(
        &self,
        reassembly_state: &mut ServerVideoFrameReassemblyState,
        queue_state: &mut ServerVideoFrameQueueState,
        packet: ServerRegisteredVideoFrameFragmentPacket,
        queued_at: TimestampMicros,
        policy: ServerVideoFrameQueuePolicy,
    ) -> ServerVideoFrameReassemblyApplyResult {
        let key = ServerVideoFrameReassemblyKey::from_fragment(&packet.fragment);

        if let Some(reason) = validate_fragment_shape(&packet.fragment) {
            return ServerVideoFrameReassemblyApplyResult::RejectedFragment {
                summary: rejected_fragment_summary(key, reason),
            };
        }

        let frame_state = reassembly_state
            .frames
            .entry(key.clone())
            .or_insert_with(|| ServerVideoFrameReassemblyFrameState::new(&packet.fragment));

        if !metadata_matches(frame_state, &packet.fragment) {
            return ServerVideoFrameReassemblyApplyResult::RejectedFragment {
                summary: summary_for_state(
                    key,
                    frame_state,
                    false,
                    Some(ServerVideoFrameFragmentRejectReason::MetadataMismatch),
                ),
            };
        }

        let chunk_index = packet.fragment.chunk_index as usize;
        if frame_state.chunks[chunk_index].is_some() {
            frame_state.duplicate_count += 1;
            return ServerVideoFrameReassemblyApplyResult::DuplicateFragmentIgnored {
                summary: summary_for_state(key, frame_state, false, None),
            };
        }

        frame_state.chunks[chunk_index] = Some(packet.fragment.chunk_payload.clone());

        if !frame_state.is_complete() {
            return ServerVideoFrameReassemblyApplyResult::FragmentStored {
                summary: summary_for_state(key, frame_state, false, None),
            };
        }

        let payload = frame_state.reassemble_payload();
        if payload.len() != frame_state.total_payload_len as usize {
            let summary = summary_for_state(
                key,
                frame_state,
                false,
                Some(ServerVideoFrameFragmentRejectReason::ReassembledPayloadLenMismatch),
            );
            return ServerVideoFrameReassemblyApplyResult::RejectedFragment { summary };
        }

        let frame = VideoFrame {
            message_type: MessageType::VideoFrame,
            protocol_version: frame_state.protocol_version,
            client_id: packet.fragment.client_id.clone(),
            run_id: packet.fragment.run_id.clone(),
            frame_id: packet.fragment.frame_id,
            capture_timestamp: frame_state.capture_timestamp,
            send_timestamp: frame_state.capture_timestamp,
            is_keyframe: false,
            metadata_reserved: [0; 3],
            width: frame_state.width,
            height: frame_state.height,
            fps_nominal: frame_state.fps_nominal,
            codec: Codec::H264,
            payload_size: payload.len(),
            payload,
        };
        let payload_len = frame.payload.len();
        let handler_input = ServerVideoFrameHandlerInput {
            registered_packet: ServerRegisteredVideoFramePacket {
                source: packet.source,
                authenticated_sender: packet.authenticated_sender,
                frame: frame.clone(),
            },
            payload_len,
        };
        let queue_result = self
            .storage
            .store_frame(queue_state, handler_input, queued_at, policy);
        let removed = reassembly_state
            .frames
            .remove(&key)
            .expect("complete frame state should still be present");
        let summary = summary_for_state(key, &removed, true, None);

        ServerVideoFrameReassemblyApplyResult::FrameComplete {
            summary,
            reassembled_frame: frame,
            queue_result,
        }
    }
}

fn validate_fragment_shape(
    fragment: &VideoFrameFragment,
) -> Option<ServerVideoFrameFragmentRejectReason> {
    if fragment.chunk_count == 0 {
        return Some(ServerVideoFrameFragmentRejectReason::ChunkCountZero);
    }
    if fragment.chunk_index >= fragment.chunk_count {
        return Some(ServerVideoFrameFragmentRejectReason::ChunkIndexOutOfRange);
    }
    if fragment.chunk_payload_len != fragment.chunk_payload.len() {
        return Some(ServerVideoFrameFragmentRejectReason::ChunkPayloadLenMismatch);
    }
    None
}

fn metadata_matches(
    state: &ServerVideoFrameReassemblyFrameState,
    fragment: &VideoFrameFragment,
) -> bool {
    state.protocol_version == fragment.protocol_version
        && state.capture_timestamp == fragment.capture_timestamp
        && state.width == fragment.width
        && state.height == fragment.height
        && state.fps_nominal == fragment.fps_nominal
        && state.total_payload_len == fragment.total_payload_len
        && state.chunk_count == fragment.chunk_count
}

fn rejected_fragment_summary(
    key: ServerVideoFrameReassemblyKey,
    reason: ServerVideoFrameFragmentRejectReason,
) -> ServerVideoFrameReassemblySummary {
    ServerVideoFrameReassemblySummary {
        key,
        fragments_received: 0,
        fragments_missing: Vec::new(),
        completed_frame_queued: false,
        rejected_fragment_reason: Some(reason),
        duplicate_count: 0,
    }
}

fn summary_for_state(
    key: ServerVideoFrameReassemblyKey,
    state: &ServerVideoFrameReassemblyFrameState,
    completed_frame_queued: bool,
    rejected_fragment_reason: Option<ServerVideoFrameFragmentRejectReason>,
) -> ServerVideoFrameReassemblySummary {
    ServerVideoFrameReassemblySummary {
        key,
        fragments_received: state.fragments_received(),
        fragments_missing: state.missing_chunks(),
        completed_frame_queued,
        rejected_fragment_reason,
        duplicate_count: state.duplicate_count,
    }
}

/// Reason the video queue runtime did not store a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerVideoFrameQueueRuntimeSkipReason {
    RejectedVideoFrame(ServerReceiveLoopGateRejection),
    NoAcceptedVideoFrame,
}

/// Result of applying video queue storage after dispatch side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerVideoFrameQueueRuntimeResult {
    Queued(ServerVideoFrameQueueStorageResult),
    NotQueued {
        reason: ServerVideoFrameQueueRuntimeSkipReason,
        side_effect: ServerDispatchRuntimeSideEffectApplyResult,
    },
}

/// Runtime boundary from accepted video side effect to caller-owned queue state.
///
/// This is the narrow receive-loop wiring for the first single-view PoC. It
/// stores only authenticated `VideoFrame` handler inputs into the existing
/// encoded-frame queue. It does not decode H.264, schedule sync, display
/// frames, notify a switcher, write logs, send UDP, or touch OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerVideoFrameQueueRuntimeBoundary {
    storage: ServerVideoFrameQueueStorageBoundary,
}

impl ServerVideoFrameQueueRuntimeBoundary {
    pub fn store_from_receive_side_effect(
        &self,
        state: &mut ServerVideoFrameQueueState,
        body: &ServerContinuousReceiveLoopBodyResult,
        side_effect: ServerDispatchRuntimeSideEffectApplyOutcome,
        queued_at: TimestampMicros,
        policy: ServerVideoFrameQueuePolicy,
    ) -> ServerVideoFrameQueueRuntimeResult {
        match side_effect.result {
            ServerDispatchRuntimeSideEffectApplyResult::VideoFrame(input) => {
                ServerVideoFrameQueueRuntimeResult::Queued(
                    self.storage.store_frame(state, input, queued_at, policy),
                )
            }
            other => {
                let reason = rejected_video_frame_from_body(body)
                    .map(ServerVideoFrameQueueRuntimeSkipReason::RejectedVideoFrame)
                    .unwrap_or(ServerVideoFrameQueueRuntimeSkipReason::NoAcceptedVideoFrame);
                ServerVideoFrameQueueRuntimeResult::NotQueued {
                    reason,
                    side_effect: other,
                }
            }
        }
    }
}

fn rejected_video_frame_from_body(
    body: &ServerContinuousReceiveLoopBodyResult,
) -> Option<ServerReceiveLoopGateRejection> {
    let ServerContinuousReceiveLoopOneTickRuntimeOutcome::Completed { handler, .. } =
        &body.tick.outcome
    else {
        return None;
    };
    let rejection = handler.writer.handoff.rejection_log.as_ref()?;
    match rejection {
        ServerReceiveLoopGateRejection::Acceptance(packet)
            if packet.message_type == MessageType::VideoFrame =>
        {
            Some(rejection.clone())
        }
        _ => None,
    }
}

/// Result of running the minimal video / stats handler runtime.
///
/// This connects future video and stats lanes to typed handler inputs only.
/// Heartbeat ack results and unrelated lanes are preserved for their owners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerVideoStatsHandlerRuntimeResult {
    VideoFrame(ServerVideoFrameHandlerInput),
    ClientStats(ServerClientStatsHandlerInput),
    NotVideoOrStats(ServerRegisteredPacketDispatchRuntimeResult),
}

/// Outcome of attempting video / stats handler input preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVideoStatsHandlerRuntimeOutcome {
    pub packet_len: Option<usize>,
    pub result: ServerVideoStatsHandlerRuntimeResult,
}

/// Minimal runtime connection from registered dispatch to video / stats inputs.
///
/// This boundary does not commit heartbeat state, enqueue outbound messages,
/// buffer video frames, update metrics, calculate RTT / offset state, write
/// logs, or own the future loop body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerVideoStatsHandlerRuntimeBoundary {
    video: ServerVideoFrameHandlerBoundary,
    stats: ServerClientStatsHandlerBoundary,
}

impl ServerVideoStatsHandlerRuntimeBoundary {
    pub fn dispatch_outcome(
        &self,
        outcome: ServerRegisteredPacketDispatchRuntimeOutcome,
    ) -> ServerVideoStatsHandlerRuntimeOutcome {
        let result = match outcome.result {
            ServerRegisteredPacketDispatchRuntimeResult::FutureVideoFrame(packet) => {
                ServerVideoStatsHandlerRuntimeResult::VideoFrame(self.video.prepare_input(packet))
            }
            ServerRegisteredPacketDispatchRuntimeResult::FutureClientStats(packet) => {
                ServerVideoStatsHandlerRuntimeResult::ClientStats(self.stats.prepare_input(packet))
            }
            other => ServerVideoStatsHandlerRuntimeResult::NotVideoOrStats(other),
        };

        ServerVideoStatsHandlerRuntimeOutcome {
            packet_len: outcome.packet_len,
            result,
        }
    }
}

/// Result of dispatching one receive-loop body result through handler runtimes.
///
/// This is one body-result orchestration only. It does not repeat the loop,
/// apply registry registrations, store queue items, write auth logs, encode
/// outbound packets, or send UDP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerContinuousReceiveLoopBodyDispatchRuntimeResult {
    Auth(ServerAuthDispatchRuntimeOutcome),
    Registered(ServerRegisteredPacketDispatchRuntimeOutcome),
    VideoStats(ServerVideoStatsHandlerRuntimeOutcome),
    NoDispatch(ServerHandlerDispatchOutcome),
}

/// Outcome of the minimal body-result to handler-runtime connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopBodyDispatchRuntimeOutcome {
    pub result: ServerContinuousReceiveLoopBodyDispatchRuntimeResult,
}

/// Minimal runtime connection from one body result to handler runtimes.
///
/// The body still owns only one receive-loop iteration. This boundary consumes
/// that result and calls the appropriate lane runtime once. Future loop code
/// remains responsible for when to repeat, when to apply side effects, and how
/// to handle shutdown / retry policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary {
    bridge: ServerContinuousReceiveLoopHandlerDispatchBoundary,
    handler: ServerHandlerDispatchBoundary,
    auth: ServerAuthDispatchRuntimeBoundary,
    registered: ServerRegisteredPacketDispatchRuntimeBoundary,
    video_stats: ServerVideoStatsHandlerRuntimeBoundary,
}

impl ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary {
    pub fn dispatch_body_result(
        &self,
        result: &ServerContinuousReceiveLoopBodyResult,
        auth_config: &ServerAuthConfig,
        heartbeat_timing: ServerHeartbeatAckTiming,
    ) -> ServerContinuousReceiveLoopBodyDispatchRuntimeOutcome {
        let handoff = self.bridge.plan_from_body_result(result);
        let handler = self.handler.dispatch_handoff(handoff);
        let result = match handler.result {
            ServerHandlerDispatchResult::Auth(_) => {
                ServerContinuousReceiveLoopBodyDispatchRuntimeResult::Auth(
                    self.auth.dispatch_outcome(handler, auth_config),
                )
            }
            ServerHandlerDispatchResult::RegisteredHeartbeat(_)
            | ServerHandlerDispatchResult::RegisteredVideoFrame(_)
            | ServerHandlerDispatchResult::RegisteredClientStats(_) => {
                let registered = self.registered.dispatch_outcome(handler, heartbeat_timing);
                match &registered.result {
                    ServerRegisteredPacketDispatchRuntimeResult::FutureVideoFrame(_)
                    | ServerRegisteredPacketDispatchRuntimeResult::FutureClientStats(_) => {
                        ServerContinuousReceiveLoopBodyDispatchRuntimeResult::VideoStats(
                            self.video_stats.dispatch_outcome(registered),
                        )
                    }
                    _ => {
                        ServerContinuousReceiveLoopBodyDispatchRuntimeResult::Registered(registered)
                    }
                }
            }
            _ => ServerContinuousReceiveLoopBodyDispatchRuntimeResult::NoDispatch(handler),
        };

        ServerContinuousReceiveLoopBodyDispatchRuntimeOutcome { result }
    }
}

/// Result of applying the minimal side effects from dispatch runtime output.
///
/// Only auth registry registration is applied here. Outbound queue items,
/// auth log input, heartbeat ack handoff, video input, and stats input stay as
/// typed handoffs for future loop-owned side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerDispatchRuntimeSideEffectApplyResult {
    Auth {
        flow: ServerAuthFlowOutcome,
        registered_sender: Option<AuthenticatedSenderEntry>,
    },
    HeartbeatAck(ServerHeartbeatAckHandoff),
    VideoFrame(ServerVideoFrameHandlerInput),
    ClientStats(ServerClientStatsHandlerInput),
    NoDispatch(ServerHandlerDispatchOutcome),
    UnappliedRegistered(ServerRegisteredPacketDispatchRuntimeOutcome),
    UnappliedVideoStats(ServerVideoStatsHandlerRuntimeOutcome),
}

/// Outcome of applying minimal dispatch runtime side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerDispatchRuntimeSideEffectApplyOutcome {
    pub result: ServerDispatchRuntimeSideEffectApplyResult,
}

/// Minimal side-effect application boundary for dispatch runtime output.
///
/// This boundary applies only accepted auth registry registration. It does not
/// write auth logs, store outbound queue items, commit heartbeat/video/stats
/// state, encode packets, send UDP, open file sinks, or run packet drop policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerDispatchRuntimeSideEffectApplyBoundary {
    registry: AuthenticatedSenderRegistryBoundary,
}

impl ServerDispatchRuntimeSideEffectApplyBoundary {
    pub fn apply_body_dispatch_outcome(
        &self,
        registry: &mut AuthenticatedSenderRegistry,
        outcome: ServerContinuousReceiveLoopBodyDispatchRuntimeOutcome,
    ) -> ServerDispatchRuntimeSideEffectApplyOutcome {
        let result = match outcome.result {
            ServerContinuousReceiveLoopBodyDispatchRuntimeResult::Auth(auth) => {
                self.apply_auth(registry, auth)
            }
            ServerContinuousReceiveLoopBodyDispatchRuntimeResult::Registered(registered) => {
                self.apply_registered(registered)
            }
            ServerContinuousReceiveLoopBodyDispatchRuntimeResult::VideoStats(video_stats) => {
                self.apply_video_stats(video_stats)
            }
            ServerContinuousReceiveLoopBodyDispatchRuntimeResult::NoDispatch(handler) => {
                ServerDispatchRuntimeSideEffectApplyResult::NoDispatch(handler)
            }
        };

        ServerDispatchRuntimeSideEffectApplyOutcome { result }
    }

    fn apply_auth(
        &self,
        registry: &mut AuthenticatedSenderRegistry,
        outcome: ServerAuthDispatchRuntimeOutcome,
    ) -> ServerDispatchRuntimeSideEffectApplyResult {
        match outcome.result {
            ServerAuthDispatchRuntimeResult::Dispatched(flow) => {
                let registered_sender = flow
                    .registry_registration
                    .clone()
                    .map(|registration| self.registry.register(registry, registration));
                ServerDispatchRuntimeSideEffectApplyResult::Auth {
                    flow,
                    registered_sender,
                }
            }
            ServerAuthDispatchRuntimeResult::NotAuth(result) => {
                ServerDispatchRuntimeSideEffectApplyResult::NoDispatch(
                    ServerHandlerDispatchOutcome {
                        packet_len: outcome.packet_len,
                        result,
                    },
                )
            }
        }
    }

    fn apply_registered(
        &self,
        outcome: ServerRegisteredPacketDispatchRuntimeOutcome,
    ) -> ServerDispatchRuntimeSideEffectApplyResult {
        match outcome.result {
            ServerRegisteredPacketDispatchRuntimeResult::HeartbeatAck(handoff) => {
                ServerDispatchRuntimeSideEffectApplyResult::HeartbeatAck(handoff)
            }
            ServerRegisteredPacketDispatchRuntimeResult::NotRegistered(result) => {
                ServerDispatchRuntimeSideEffectApplyResult::NoDispatch(
                    ServerHandlerDispatchOutcome {
                        packet_len: outcome.packet_len,
                        result,
                    },
                )
            }
            result => ServerDispatchRuntimeSideEffectApplyResult::UnappliedRegistered(
                ServerRegisteredPacketDispatchRuntimeOutcome {
                    packet_len: outcome.packet_len,
                    result,
                },
            ),
        }
    }

    fn apply_video_stats(
        &self,
        outcome: ServerVideoStatsHandlerRuntimeOutcome,
    ) -> ServerDispatchRuntimeSideEffectApplyResult {
        match outcome.result {
            ServerVideoStatsHandlerRuntimeResult::VideoFrame(input) => {
                ServerDispatchRuntimeSideEffectApplyResult::VideoFrame(input)
            }
            ServerVideoStatsHandlerRuntimeResult::ClientStats(input) => {
                ServerDispatchRuntimeSideEffectApplyResult::ClientStats(input)
            }
            result => ServerDispatchRuntimeSideEffectApplyResult::UnappliedVideoStats(
                ServerVideoStatsHandlerRuntimeOutcome {
                    packet_len: outcome.packet_len,
                    result,
                },
            ),
        }
    }
}

/// Result of passing one outbound item to the minimal queue storage side.
///
/// This records the storage decision and a one-item queued placeholder when the
/// candidate is accepted. It does not mutate a queue collection or wake a send
/// loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOutboundQueueStorageApplyResult {
    pub decision: OutboundQueueStorageDecision,
    pub queued_item: Option<QueuedOutboundItem>,
}

/// Result of applying output side effects after dispatch side-effect apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerDispatchRuntimeOutputApplyResult {
    Auth {
        flow: ServerAuthFlowOutcome,
        registered_sender: Option<AuthenticatedSenderEntry>,
        auth_log_event: ServerAuthJsonLogEventInput,
        auth_response_storage: Option<ServerOutboundQueueStorageApplyResult>,
    },
    Preserved(ServerDispatchRuntimeSideEffectApplyResult),
}

/// Outcome of connecting dispatch side-effect output to queue/log boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerDispatchRuntimeOutputApplyOutcome {
    pub result: ServerDispatchRuntimeOutputApplyResult,
}

/// Minimal output application after dispatch side-effect apply.
///
/// This writes auth log input to a caller-owned writer and passes accepted auth
/// response queue items to queue storage planning. Heartbeat ack handoffs are
/// preserved for the queue collection bridge. It does not open files, own a
/// process-wide logger, store a real queue collection, encode packets, send
/// UDP, retry, or process video/stats handoffs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerDispatchRuntimeOutputApplyBoundary {
    auth_log: ServerAuthLogOutputBoundary,
    queue: ServerOutboundQueueBoundary,
    queue_lifecycle: OutboundQueueLifecycleBoundary,
}

impl ServerDispatchRuntimeOutputApplyBoundary {
    pub fn apply_outputs<W: io::Write>(
        &self,
        outcome: ServerDispatchRuntimeSideEffectApplyOutcome,
        current_queue_len: usize,
        log_timestamp: TimestampMicros,
        mut auth_log_writer: W,
    ) -> io::Result<ServerDispatchRuntimeOutputApplyOutcome> {
        let result = match outcome.result {
            ServerDispatchRuntimeSideEffectApplyResult::Auth {
                flow,
                registered_sender,
            } => {
                let auth_log_event = self.auth_log.write_auth_result(
                    flow.auth_log_input.clone(),
                    log_timestamp,
                    &mut auth_log_writer,
                )?;
                let auth_response_storage = if flow.decision.accepted {
                    Some(self.plan_auth_response_storage(current_queue_len, &flow.queue_item))
                } else {
                    None
                };

                ServerDispatchRuntimeOutputApplyResult::Auth {
                    flow,
                    registered_sender,
                    auth_log_event,
                    auth_response_storage,
                }
            }
            other => ServerDispatchRuntimeOutputApplyResult::Preserved(other),
        };

        Ok(ServerDispatchRuntimeOutputApplyOutcome { result })
    }

    fn plan_auth_response_storage(
        &self,
        current_queue_len: usize,
        item: &OutboundQueueItem,
    ) -> ServerOutboundQueueStorageApplyResult {
        let decision = self.queue.evaluate_storage_push(current_queue_len, item);
        let queued_item = decision
            .accepts_candidate()
            .then(|| self.queue_lifecycle.hold_for_send(item.clone()));

        ServerOutboundQueueStorageApplyResult {
            decision,
            queued_item,
        }
    }
}

/// Minimal outbound queue collection for one synchronous send handoff path.
///
/// This stores typed queued items in FIFO order only. It does not run a
/// background loop, wake tasks, retry, requeue, persist, or apply drop policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerOutboundQueueCollection {
    items: VecDeque<QueuedOutboundItem>,
}

impl ServerOutboundQueueCollection {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Result of pushing output-apply queue handoff into the collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOutboundQueueCollectionPushOutcome {
    pub queued: bool,
    pub queue_len: usize,
}

/// Result of dequeueing one item for the send side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerOutboundQueueDequeueRuntimeResult {
    Ready(QueuedOutboundItem),
    Empty,
}

/// Minimal queue collection boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerOutboundQueueCollectionBoundary;

impl ServerOutboundQueueCollectionBoundary {
    pub fn push_from_output_apply(
        &self,
        collection: &mut ServerOutboundQueueCollection,
        outcome: &ServerDispatchRuntimeOutputApplyOutcome,
    ) -> ServerOutboundQueueCollectionPushOutcome {
        let queued_item = match &outcome.result {
            ServerDispatchRuntimeOutputApplyResult::Auth {
                auth_response_storage: Some(storage),
                ..
            } => storage.queued_item.clone(),
            ServerDispatchRuntimeOutputApplyResult::Preserved(
                ServerDispatchRuntimeSideEffectApplyResult::HeartbeatAck(handoff),
            ) => {
                let queue = ServerOutboundQueueBoundary::default();
                let decision = queue.evaluate_storage_push(collection.len(), &handoff.queue_item);
                decision.accepts_candidate().then(|| {
                    OutboundQueueLifecycleBoundary.hold_for_send(handoff.queue_item.clone())
                })
            }
            _ => None,
        };

        let queued = if let Some(item) = queued_item {
            collection.items.push_back(item);
            true
        } else {
            false
        };

        ServerOutboundQueueCollectionPushOutcome {
            queued,
            queue_len: collection.len(),
        }
    }

    pub fn dequeue_one(
        &self,
        collection: &mut ServerOutboundQueueCollection,
    ) -> ServerOutboundQueueDequeueRuntimeResult {
        collection
            .items
            .pop_front()
            .map(ServerOutboundQueueDequeueRuntimeResult::Ready)
            .unwrap_or(ServerOutboundQueueDequeueRuntimeResult::Empty)
    }
}

/// Outcome of sending one queued outbound item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOutboundSendOneRuntimeOutcome {
    pub plan: OutboundSendLoopTickPlan,
    pub encoded_packet: EncodedOutboundPacket,
    pub encode_event: OutboundSendLoopEvent,
    pub bytes_sent: usize,
    pub send_event: OutboundSendLoopEvent,
    pub final_state: OutboundQueueItemState,
}

/// Error from the minimal one-item send runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerOutboundSendOneRuntimeError {
    Encode {
        error: NetEncodeError,
        event: OutboundSendLoopEvent,
    },
    SocketSend {
        error_kind: io::ErrorKind,
        event: OutboundSendLoopEvent,
    },
}

/// Minimal runtime from one queued item to encode and socket send.
///
/// This sends one already queued item through the protocol encoder and UDP
/// adapter. It does not loop, retry, requeue, open logs, or own queue storage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerOutboundSendOneRuntimeBoundary {
    queue_lifecycle: OutboundQueueLifecycleBoundary,
    send_tick: OutboundSendLoopTickBoundary,
    encoder: OutboundPacketEncoderBoundary,
    protocol_encoder: ProtocolMessageEncoderBoundary,
    socket_io: ServerUdpSocketIoStep,
}

impl ServerOutboundSendOneRuntimeBoundary {
    pub fn send_queued(
        &self,
        socket: &UdpSocket,
        queued: QueuedOutboundItem,
        context: EncodeContext,
    ) -> Result<ServerOutboundSendOneRuntimeOutcome, ServerOutboundSendOneRuntimeError> {
        let handoff = self.queue_lifecycle.handoff_to_send_layer(queued);
        let plan = self.send_tick.plan_encode(context, handoff);
        let encoded_packet = self
            .encoder
            .encode_with(&self.protocol_encoder, plan.encode_request.clone())
            .map_err(|error| ServerOutboundSendOneRuntimeError::Encode {
                error,
                event: self.send_tick.observe_encode_failure(&plan),
            })?;
        let encode_event = self
            .send_tick
            .observe_encode_success(&plan, &encoded_packet);

        let bytes_sent = self
            .socket_io
            .send_encoded(socket, &encoded_packet)
            .map_err(|error| {
                let error_kind = error.kind();
                ServerOutboundSendOneRuntimeError::SocketSend {
                    error_kind,
                    event: self.send_tick.observe_socket_send_failure(
                        &plan,
                        encoded_packet.bytes.len(),
                        send_failure_kind_from_io_error_kind(error_kind),
                    ),
                }
            })?;
        let send_event = self.send_tick.observe_socket_send_success();

        Ok(ServerOutboundSendOneRuntimeOutcome {
            plan,
            encoded_packet,
            encode_event,
            bytes_sent,
            send_event,
            final_state: self.queue_lifecycle.mark_send_completed(),
        })
    }
}

/// Input for one receive-then-send integration step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerReceiveSendOneIterationRuntimeInput {
    pub body: ServerContinuousReceiveLoopBodyInput,
    pub heartbeat_timing: ServerHeartbeatAckTiming,
    pub encode_context: EncodeContext,
    pub auth_log_timestamp: TimestampMicros,
    pub send_log_timestamp: TimestampMicros,
}

/// Outcome of connecting one receive iteration to one optional send step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReceiveSendOneIterationRuntimeOutcome {
    pub body: ServerContinuousReceiveLoopBodyResult,
    pub dispatch: ServerContinuousReceiveLoopBodyDispatchRuntimeOutcome,
    pub side_effect: ServerDispatchRuntimeSideEffectApplyOutcome,
    pub output: ServerDispatchRuntimeOutputApplyOutcome,
    pub queue_push: ServerOutboundQueueCollectionPushOutcome,
    pub dequeue: ServerOutboundQueueDequeueRuntimeResult,
    pub send: Option<ServerOutboundSendOneRuntimeOutcome>,
    pub send_log: Option<ServerSendJsonLogEventInput>,
}

/// Error from the one-iteration receive/send integration runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReceiveSendOneIterationRuntimeError {
    ReceiveBody(io::ErrorKind),
    OutputApply(io::ErrorKind),
    SendLog(io::ErrorKind),
    Send(ServerOutboundSendOneRuntimeError),
}

/// Minimal integration boundary from one receive body iteration to one send.
///
/// This boundary composes existing one-step runtimes only. It does not repeat
/// receive or send loops, retry, requeue, open files, install process-wide
/// logging, or process heartbeat/video/stat side effects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerReceiveSendOneIterationRuntimeBoundary {
    body: ServerContinuousReceiveLoopBodyBoundary,
    dispatch: ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary,
    side_effect: ServerDispatchRuntimeSideEffectApplyBoundary,
    output: ServerDispatchRuntimeOutputApplyBoundary,
    queue: ServerOutboundQueueCollectionBoundary,
    send: ServerOutboundSendOneRuntimeBoundary,
    send_log: ServerSendLogOutputBoundary,
}

impl ServerReceiveSendOneIterationRuntimeBoundary {
    pub fn run_once<OW: io::Write, RW: io::Write, AW: io::Write, SW: io::Write>(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        registry: &mut AuthenticatedSenderRegistry,
        queue_collection: &mut ServerOutboundQueueCollection,
        auth_config: &ServerAuthConfig,
        input: ServerReceiveSendOneIterationRuntimeInput,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<
        ServerReceiveSendOneIterationRuntimeOutcome,
        ServerReceiveSendOneIterationRuntimeError,
    > {
        let body = self
            .body
            .run_once(
                socket,
                buffer,
                registry,
                input.body,
                operational_writer,
                rejection_writer,
            )
            .map_err(|error| {
                ServerReceiveSendOneIterationRuntimeError::ReceiveBody(error.kind())
            })?;
        let dispatch =
            self.dispatch
                .dispatch_body_result(&body, auth_config, input.heartbeat_timing);
        let side_effect = self
            .side_effect
            .apply_body_dispatch_outcome(registry, dispatch.clone());
        let output = self
            .output
            .apply_outputs(
                side_effect.clone(),
                queue_collection.len(),
                input.auth_log_timestamp,
                auth_log_writer,
            )
            .map_err(|error| {
                ServerReceiveSendOneIterationRuntimeError::OutputApply(error.kind())
            })?;
        let queue_push = self.queue.push_from_output_apply(queue_collection, &output);
        let dequeue = self.queue.dequeue_one(queue_collection);
        let (send, send_log) = match dequeue.clone() {
            ServerOutboundQueueDequeueRuntimeResult::Ready(queued) => {
                match self.send.send_queued(socket, queued, input.encode_context) {
                    Ok(send) => {
                        let send_log = self
                            .send_log
                            .write_send_success(&send, input.send_log_timestamp, send_log_writer)
                            .map_err(|error| {
                                ServerReceiveSendOneIterationRuntimeError::SendLog(error.kind())
                            })?;
                        (Some(send), Some(send_log))
                    }
                    Err(error) => {
                        self.send_log
                            .write_send_failure(&error, input.send_log_timestamp, send_log_writer)
                            .map_err(|log_error| {
                                ServerReceiveSendOneIterationRuntimeError::SendLog(log_error.kind())
                            })?;
                        return Err(ServerReceiveSendOneIterationRuntimeError::Send(error));
                    }
                }
            }
            ServerOutboundQueueDequeueRuntimeResult::Empty => (None, None),
        };

        Ok(ServerReceiveSendOneIterationRuntimeOutcome {
            body,
            dispatch,
            side_effect,
            output,
            queue_push,
            dequeue,
            send,
            send_log,
        })
    }
}

/// Input for the minimal controller-to-receive/send runtime connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerControllerReceiveSendRuntimeInput {
    pub controller: ServerContinuousReceiveLoopControllerInput,
    pub heartbeat_timing: ServerHeartbeatAckTiming,
    pub encode_context: EncodeContext,
    pub auth_log_timestamp: TimestampMicros,
    pub send_log_timestamp: TimestampMicros,
}

/// Result of one controller step that may run one receive/send iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerControllerReceiveSendRuntimeResult {
    Stopped {
        plan: ServerContinuousReceiveLoopControllerPlan,
    },
    Iteration {
        plan: ServerContinuousReceiveLoopControllerPlan,
        iteration: ServerReceiveSendOneIterationRuntimeOutcome,
        observation: ServerContinuousReceiveLoopControllerObservation,
    },
}

/// Error from the minimal controller receive/send runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerControllerReceiveSendRuntimeError {
    MissingBodyInput(ServerContinuousReceiveLoopControllerPlan),
    Iteration(ServerReceiveSendOneIterationRuntimeError),
}

/// Minimal controller-side handoff to one receive/send iteration.
///
/// This boundary runs the controller stop check and, if requested, calls one
/// receive/send integration step. It does not loop, sleep, retry, requeue,
/// open files, install process-wide logging, or own shutdown policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerControllerReceiveSendRuntimeBoundary {
    controller: ServerContinuousReceiveLoopControllerBoundary,
    iteration: ServerReceiveSendOneIterationRuntimeBoundary,
}

impl ServerControllerReceiveSendRuntimeBoundary {
    pub fn run_once<OW: io::Write, RW: io::Write, AW: io::Write, SW: io::Write>(
        &self,
        socket: &UdpSocket,
        buffer: &mut [u8],
        registry: &mut AuthenticatedSenderRegistry,
        queue_collection: &mut ServerOutboundQueueCollection,
        auth_config: &ServerAuthConfig,
        input: ServerControllerReceiveSendRuntimeInput,
        operational_writer: OW,
        rejection_writer: RW,
        auth_log_writer: AW,
        send_log_writer: SW,
    ) -> Result<ServerControllerReceiveSendRuntimeResult, ServerControllerReceiveSendRuntimeError>
    {
        let plan = self.controller.plan_next_iteration(input.controller);
        if matches!(
            plan.action,
            ServerContinuousReceiveLoopControllerAction::Stop
        ) {
            return Ok(ServerControllerReceiveSendRuntimeResult::Stopped { plan });
        }

        let Some(body_input) = plan.body_input else {
            return Err(ServerControllerReceiveSendRuntimeError::MissingBodyInput(
                plan,
            ));
        };

        let iteration = self
            .iteration
            .run_once(
                socket,
                buffer,
                registry,
                queue_collection,
                auth_config,
                ServerReceiveSendOneIterationRuntimeInput {
                    body: body_input,
                    heartbeat_timing: input.heartbeat_timing,
                    encode_context: input.encode_context,
                    auth_log_timestamp: input.auth_log_timestamp,
                    send_log_timestamp: input.send_log_timestamp,
                },
                operational_writer,
                rejection_writer,
                auth_log_writer,
                send_log_writer,
            )
            .map_err(ServerControllerReceiveSendRuntimeError::Iteration)?;
        let observation = self.controller.observe_body_result(&iteration.body);

        Ok(ServerControllerReceiveSendRuntimeResult::Iteration {
            plan,
            iteration,
            observation,
        })
    }
}

fn send_failure_kind_from_io_error_kind(error_kind: io::ErrorKind) -> SendFailureKind {
    match error_kind {
        io::ErrorKind::WouldBlock => SendFailureKind::SocketWouldBlock,
        io::ErrorKind::Interrupted => SendFailureKind::SocketInterrupted,
        io::ErrorKind::ConnectionRefused => SendFailureKind::ConnectionRefused,
        io::ErrorKind::NetworkUnreachable => SendFailureKind::NetworkUnreachable,
        io::ErrorKind::PermissionDenied => SendFailureKind::PermissionDenied,
        _ => SendFailureKind::OtherSocketError,
    }
}

pub const SERVER_RECEIVE_LOOP_JSON_LOG_EVENT_NAME: &str = "server.receive_loop";

/// Operational receive loop outcome used by future continuous-loop logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerReceiveLoopLogOutcome {
    Accepted,
    DecodeRejected,
    AcceptanceRejected,
}

/// Lightweight receive loop observation for future operational JSON Lines logs.
///
/// This records the one-packet loop outcome only. Detailed rejection diagnostics
/// stay in `server.receive_rejection`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReceiveLoopLogInput {
    pub source: PacketSource,
    pub outcome: ServerReceiveLoopLogOutcome,
    pub packet_len: usize,
    pub message_type: Option<MessageType>,
    pub client_id: Option<ClientId>,
    pub rejection_reason: Option<ServerReceiveRejectionReason>,
}

/// Boundary that converts one receive loop outcome into operational log input.
///
/// It does not write JSON Lines, drop packets, call handlers, update metrics,
/// or run the continuous receive loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveLoopLogHandoffBoundary;

impl ServerReceiveLoopLogHandoffBoundary {
    pub fn handoff(
        &self,
        outcome: &ServerReceiveLoopGateOutcome,
        packet_len: usize,
    ) -> ServerReceiveLoopLogInput {
        match outcome {
            ServerReceiveLoopGateOutcome::Accepted(route) => {
                let metadata = inbound_route_log_metadata(route);
                ServerReceiveLoopLogInput {
                    source: metadata.source,
                    outcome: ServerReceiveLoopLogOutcome::Accepted,
                    packet_len,
                    message_type: Some(metadata.message_type),
                    client_id: metadata.client_id,
                    rejection_reason: None,
                }
            }
            ServerReceiveLoopGateOutcome::Rejected(ServerReceiveLoopGateRejection::Decode(
                rejected,
            )) => ServerReceiveLoopLogInput {
                source: rejected.source,
                outcome: ServerReceiveLoopLogOutcome::DecodeRejected,
                packet_len,
                message_type: None,
                client_id: None,
                rejection_reason: Some(ServerReceiveRejectionReason::DecodeError),
            },
            ServerReceiveLoopGateOutcome::Rejected(ServerReceiveLoopGateRejection::Acceptance(
                rejected,
            )) => ServerReceiveLoopLogInput {
                source: rejected.source,
                outcome: ServerReceiveLoopLogOutcome::AcceptanceRejected,
                packet_len,
                message_type: Some(rejected.message_type),
                client_id: rejected.client_id.clone(),
                rejection_reason: Some(match rejected.reason {
                    PacketAcceptanceRejectReason::UnauthenticatedSource => {
                        ServerReceiveRejectionReason::UnauthenticatedSource
                    }
                    PacketAcceptanceRejectReason::UnknownClient => {
                        ServerReceiveRejectionReason::UnknownClient
                    }
                    PacketAcceptanceRejectReason::EndpointMismatch => {
                        ServerReceiveRejectionReason::EndpointMismatch
                    }
                }),
            },
        }
    }
}

/// Boundary that maps receive loop operational input to JSON Lines event fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveLoopJsonLogEventBoundary;

impl ServerReceiveLoopJsonLogEventBoundary {
    pub fn build_event(
        &self,
        input: ServerReceiveLoopLogInput,
        timestamp: TimestampMicros,
    ) -> ServerReceiveLoopJsonLogEventInput {
        ServerReceiveLoopJsonLogEventInput {
            event_name: SERVER_RECEIVE_LOOP_JSON_LOG_EVENT_NAME,
            source: input.source,
            outcome: input.outcome,
            packet_len: input.packet_len,
            message_type: input.message_type,
            client_id: input.client_id,
            rejection_reason: input.rejection_reason,
            timestamp,
        }
    }
}

/// JSON Lines event input for receive loop operational observations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReceiveLoopJsonLogEventInput {
    pub event_name: &'static str,
    pub source: PacketSource,
    pub outcome: ServerReceiveLoopLogOutcome,
    pub packet_len: usize,
    pub message_type: Option<MessageType>,
    pub client_id: Option<ClientId>,
    pub rejection_reason: Option<ServerReceiveRejectionReason>,
    pub timestamp: TimestampMicros,
}

/// Minimal receive loop operational log output boundary.
///
/// This writes one lightweight JSON Lines observation to a caller-owned writer.
/// It does not own files, rotation, async I/O, buffering policy, packet drop,
/// handler execution, metrics aggregation, or global logging configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveLoopLogOutputBoundary {
    handoff: ServerReceiveLoopLogHandoffBoundary,
    event: ServerReceiveLoopJsonLogEventBoundary,
    writer: ServerReceiveLoopJsonLineWriter,
}

impl ServerReceiveLoopLogOutputBoundary {
    pub fn write_receive_loop_event<W: io::Write>(
        &self,
        outcome: &ServerReceiveLoopGateOutcome,
        packet_len: usize,
        timestamp: TimestampMicros,
        writer: W,
    ) -> io::Result<ServerReceiveLoopJsonLogEventInput> {
        let input = self.handoff.handoff(outcome, packet_len);
        let event = self.event.build_event(input, timestamp);
        self.writer.write_event(&event, writer)?;
        Ok(event)
    }
}

/// Minimal JSON Lines writer for receive loop operational events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerReceiveLoopJsonLineWriter;

impl ServerReceiveLoopJsonLineWriter {
    pub fn write_event<W: io::Write>(
        &self,
        event: &ServerReceiveLoopJsonLogEventInput,
        mut writer: W,
    ) -> io::Result<()> {
        write!(writer, "{{")?;
        write_json_field(&mut writer, "event_name", event.event_name)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "source", &event.source.address.to_string())?;
        write!(writer, ",")?;
        write_json_field(
            &mut writer,
            "outcome",
            receive_loop_log_outcome_name(event.outcome),
        )?;
        write!(writer, ",\"packet_len\":{}", event.packet_len)?;
        write!(writer, ",")?;
        write_optional_json_field(
            &mut writer,
            "message_type",
            event.message_type.map(receive_rejection_message_type_name),
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
        write_optional_json_field(
            &mut writer,
            "rejection_reason",
            event.rejection_reason.map(receive_rejection_reason_name),
        )?;
        write!(writer, ",\"timestamp\":{}", event.timestamp.0)?;
        writeln!(writer, "}}")
    }
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

pub const SERVER_SEND_ERROR_EVENT_NAME: &str = "server.send_error";
pub const SERVER_SEND_EVENT_NAME: &str = "server.send";

/// Minimal send log outcome for one-item send observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerSendLogOutcome {
    Success,
    Failure,
}

/// JSON Lines event input for send success/failure observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSendJsonLogEventInput {
    pub event_name: &'static str,
    pub outcome: ServerSendLogOutcome,
    pub run_id: Option<RunId>,
    pub client_id: Option<ClientId>,
    pub destination: PacketDestination,
    pub message_type: MessageType,
    pub stage: SendLogStage,
    pub encoded_len: Option<usize>,
    pub bytes_sent: Option<usize>,
    pub failure: Option<SendFailureKind>,
    pub disposition: Option<SendFailureDisposition>,
    pub timestamp: TimestampMicros,
}

/// Boundary that maps one-item send success/failure into send JSON Lines input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSendJsonLogEventBoundary;

impl ServerSendJsonLogEventBoundary {
    pub fn build_success_event(
        &self,
        outcome: &ServerOutboundSendOneRuntimeOutcome,
        timestamp: TimestampMicros,
    ) -> ServerSendJsonLogEventInput {
        ServerSendJsonLogEventInput {
            event_name: SERVER_SEND_EVENT_NAME,
            outcome: ServerSendLogOutcome::Success,
            run_id: outcome.plan.log_context.run_id.clone(),
            client_id: outcome.plan.log_context.client_id.clone(),
            destination: outcome.plan.log_context.destination,
            message_type: outcome.plan.log_context.message_type,
            stage: SendLogStage::SocketSend,
            encoded_len: Some(outcome.encoded_packet.bytes.len()),
            bytes_sent: Some(outcome.bytes_sent),
            failure: None,
            disposition: None,
            timestamp,
        }
    }

    pub fn build_failure_event(
        &self,
        error: &ServerOutboundSendOneRuntimeError,
        timestamp: TimestampMicros,
    ) -> Option<ServerSendJsonLogEventInput> {
        let send_event = match error {
            ServerOutboundSendOneRuntimeError::Encode { event, .. }
            | ServerOutboundSendOneRuntimeError::SocketSend { event, .. } => {
                event.log_event.as_ref()?
            }
        };
        let failure = send_event.failure?;
        Some(ServerSendJsonLogEventInput {
            event_name: SERVER_SEND_EVENT_NAME,
            outcome: ServerSendLogOutcome::Failure,
            run_id: send_event.context.run_id.clone(),
            client_id: send_event.context.client_id.clone(),
            destination: send_event.context.destination,
            message_type: send_event.context.message_type,
            stage: send_event.stage,
            encoded_len: send_event.encoded_len,
            bytes_sent: None,
            failure: Some(failure),
            disposition: Some(
                send_event
                    .disposition
                    .unwrap_or_else(|| failure.disposition()),
            ),
            timestamp,
        })
    }
}

/// Minimal JSON Lines writer for one-item send success/failure events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSendJsonLineWriter;

impl ServerSendJsonLineWriter {
    pub fn write_event<W: io::Write>(
        &self,
        event: &ServerSendJsonLogEventInput,
        mut writer: W,
    ) -> io::Result<()> {
        write!(writer, "{{")?;
        write_json_field(&mut writer, "event_name", event.event_name)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "outcome", send_log_outcome_name(event.outcome))?;
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
        write_json_field(
            &mut writer,
            "destination",
            &event.destination.address.to_string(),
        )?;
        write!(writer, ",")?;
        write_json_field(
            &mut writer,
            "message_type",
            receive_rejection_message_type_name(event.message_type),
        )?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "stage", send_log_stage_name(event.stage))?;
        write!(writer, ",\"encoded_len\":")?;
        match event.encoded_len {
            Some(encoded_len) => write!(writer, "{encoded_len}")?,
            None => write!(writer, "null")?,
        }
        write!(writer, ",\"bytes_sent\":")?;
        match event.bytes_sent {
            Some(bytes_sent) => write!(writer, "{bytes_sent}")?,
            None => write!(writer, "null")?,
        }
        write!(writer, ",")?;
        write_optional_json_field(
            &mut writer,
            "failure",
            event.failure.map(send_failure_kind_name),
        )?;
        write!(writer, ",")?;
        write_optional_json_field(
            &mut writer,
            "disposition",
            event.disposition.map(send_failure_disposition_name),
        )?;
        write!(writer, ",\"timestamp\":{}", event.timestamp.0)?;
        writeln!(writer, "}}")
    }
}

/// Minimal send log output boundary.
///
/// This writes one success/failure observation for one-item send runtime to a
/// caller-owned writer. It does not own files, rotate logs, buffer globally,
/// retry, requeue, or install a process-wide logger.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSendLogOutputBoundary {
    event: ServerSendJsonLogEventBoundary,
    writer: ServerSendJsonLineWriter,
}

impl ServerSendLogOutputBoundary {
    pub fn write_send_success<W: io::Write>(
        &self,
        outcome: &ServerOutboundSendOneRuntimeOutcome,
        timestamp: TimestampMicros,
        writer: W,
    ) -> io::Result<ServerSendJsonLogEventInput> {
        let event = self.event.build_success_event(outcome, timestamp);
        self.writer.write_event(&event, writer)?;
        Ok(event)
    }

    pub fn write_send_failure<W: io::Write>(
        &self,
        error: &ServerOutboundSendOneRuntimeError,
        timestamp: TimestampMicros,
        writer: W,
    ) -> io::Result<Option<ServerSendJsonLogEventInput>> {
        let Some(event) = self.event.build_failure_event(error, timestamp) else {
            return Ok(None);
        };
        self.writer.write_event(&event, writer)?;
        Ok(Some(event))
    }
}

/// Send error input handed from net-core send events into server logging.
///
/// This is failure-only. Encode/socket success events may be observed by the
/// send loop, but they are not part of the initial send error JSON Lines scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSendErrorLogInput {
    pub context: OutboundSendLogContext,
    pub stage: SendLogStage,
    pub encoded_len: Option<usize>,
    pub failure: SendFailureKind,
    pub disposition: SendFailureDisposition,
}

/// Boundary that filters net-core send events into send error log handoff input.
///
/// It does not write JSON Lines, choose sinks, mutate queues, execute retry, or
/// call sockets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSendErrorLogHandoffBoundary;

impl ServerSendErrorLogHandoffBoundary {
    pub fn handoff(&self, event: SendLogEvent) -> Option<ServerSendErrorLogInput> {
        let failure = event.failure?;
        Some(ServerSendErrorLogInput {
            context: event.context,
            stage: event.stage,
            encoded_len: event.encoded_len,
            failure,
            disposition: event.disposition.unwrap_or_else(|| failure.disposition()),
        })
    }
}

/// Boundary that maps send error log handoff input to JSON Lines event fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSendErrorJsonLogEventBoundary;

impl ServerSendErrorJsonLogEventBoundary {
    pub fn build_event(
        &self,
        input: ServerSendErrorLogInput,
        timestamp: TimestampMicros,
    ) -> ServerSendErrorJsonLogEventInput {
        ServerSendErrorJsonLogEventInput {
            event_name: SERVER_SEND_ERROR_EVENT_NAME,
            run_id: input.context.run_id,
            client_id: input.context.client_id,
            destination: input.context.destination,
            message_type: input.context.message_type,
            stage: input.stage,
            encoded_len: input.encoded_len,
            failure: input.failure,
            disposition: input.disposition,
            timestamp,
        }
    }
}

/// JSON Lines event input for send errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSendErrorJsonLogEventInput {
    pub event_name: &'static str,
    pub run_id: Option<RunId>,
    pub client_id: Option<ClientId>,
    pub destination: PacketDestination,
    pub message_type: MessageType,
    pub stage: SendLogStage,
    pub encoded_len: Option<usize>,
    pub failure: SendFailureKind,
    pub disposition: SendFailureDisposition,
    pub timestamp: TimestampMicros,
}

/// Minimal send error log output boundary.
///
/// This connects net-core send log events, a server JSON Lines schema, and an
/// `io::Write` sink. It writes one JSON Lines record and does not own files,
/// rotation, async I/O, buffering policy, retry, queue mutation, or global
/// logging configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSendErrorLogOutputBoundary {
    handoff: ServerSendErrorLogHandoffBoundary,
    event: ServerSendErrorJsonLogEventBoundary,
    writer: ServerSendErrorJsonLineWriter,
}

impl ServerSendErrorLogOutputBoundary {
    pub fn write_send_error<W: io::Write>(
        &self,
        send_event: SendLogEvent,
        timestamp: TimestampMicros,
        writer: W,
    ) -> io::Result<Option<ServerSendErrorJsonLogEventInput>> {
        let Some(input) = self.handoff.handoff(send_event) else {
            return Ok(None);
        };
        let event = self.event.build_event(input, timestamp);
        self.writer.write_event(&event, writer)?;
        Ok(Some(event))
    }
}

/// Minimal JSON Lines writer for send error events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSendErrorJsonLineWriter;

impl ServerSendErrorJsonLineWriter {
    pub fn write_event<W: io::Write>(
        &self,
        event: &ServerSendErrorJsonLogEventInput,
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
        write_json_field(
            &mut writer,
            "destination",
            &event.destination.address.to_string(),
        )?;
        write!(writer, ",")?;
        write_json_field(
            &mut writer,
            "message_type",
            receive_rejection_message_type_name(event.message_type),
        )?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "stage", send_log_stage_name(event.stage))?;
        write!(writer, ",\"encoded_len\":")?;
        match event.encoded_len {
            Some(encoded_len) => write!(writer, "{encoded_len}")?,
            None => write!(writer, "null")?,
        }
        write!(writer, ",")?;
        write_json_field(
            &mut writer,
            "failure",
            send_failure_kind_name(event.failure),
        )?;
        write!(writer, ",")?;
        write_json_field(
            &mut writer,
            "disposition",
            send_failure_disposition_name(event.disposition),
        )?;
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
        MessageType::VideoFrameFragment => "VideoFrameFragment",
    }
}

fn receive_loop_log_outcome_name(outcome: ServerReceiveLoopLogOutcome) -> &'static str {
    match outcome {
        ServerReceiveLoopLogOutcome::Accepted => "Accepted",
        ServerReceiveLoopLogOutcome::DecodeRejected => "DecodeRejected",
        ServerReceiveLoopLogOutcome::AcceptanceRejected => "AcceptanceRejected",
    }
}

fn send_log_stage_name(stage: SendLogStage) -> &'static str {
    match stage {
        SendLogStage::Encode => "Encode",
        SendLogStage::BeforeSocketSend => "BeforeSocketSend",
        SendLogStage::SocketSend => "SocketSend",
    }
}

fn send_log_outcome_name(outcome: ServerSendLogOutcome) -> &'static str {
    match outcome {
        ServerSendLogOutcome::Success => "Success",
        ServerSendLogOutcome::Failure => "Failure",
    }
}

fn send_failure_kind_name(failure: SendFailureKind) -> &'static str {
    match failure {
        SendFailureKind::EncodeFailed => "EncodeFailed",
        SendFailureKind::DestinationUnavailable => "DestinationUnavailable",
        SendFailureKind::PacketTooLarge => "PacketTooLarge",
        SendFailureKind::SocketWouldBlock => "SocketWouldBlock",
        SendFailureKind::SocketInterrupted => "SocketInterrupted",
        SendFailureKind::ConnectionRefused => "ConnectionRefused",
        SendFailureKind::NetworkUnreachable => "NetworkUnreachable",
        SendFailureKind::PermissionDenied => "PermissionDenied",
        SendFailureKind::OtherSocketError => "OtherSocketError",
    }
}

fn send_failure_disposition_name(disposition: SendFailureDisposition) -> &'static str {
    match disposition {
        SendFailureDisposition::RetryCandidate => "RetryCandidate",
        SendFailureDisposition::DropCandidate => "DropCandidate",
        SendFailureDisposition::WarningCandidate => "WarningCandidate",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InboundRouteLogMetadata {
    source: PacketSource,
    message_type: MessageType,
    client_id: Option<ClientId>,
}

fn inbound_route_log_metadata(route: &ServerInboundRoute) -> InboundRouteLogMetadata {
    match route {
        ServerInboundRoute::AuthRequest { source, request } => InboundRouteLogMetadata {
            source: *source,
            message_type: MessageType::AuthRequest,
            client_id: Some(request.client_id.clone()),
        },
        ServerInboundRoute::Heartbeat { source, heartbeat } => InboundRouteLogMetadata {
            source: *source,
            message_type: MessageType::Heartbeat,
            client_id: Some(heartbeat.client_id.clone()),
        },
        ServerInboundRoute::VideoFrame { source, frame } => InboundRouteLogMetadata {
            source: *source,
            message_type: MessageType::VideoFrame,
            client_id: Some(frame.client_id.clone()),
        },
        ServerInboundRoute::VideoFrameFragment { source, fragment } => InboundRouteLogMetadata {
            source: *source,
            message_type: MessageType::VideoFrameFragment,
            client_id: Some(fragment.client_id.clone()),
        },
        ServerInboundRoute::ClientStats { source, stats } => InboundRouteLogMetadata {
            source: *source,
            message_type: MessageType::ClientStats,
            client_id: Some(stats.client_id.clone()),
        },
        ServerInboundRoute::UnsupportedForServer {
            source,
            message_type,
            ..
        } => InboundRouteLogMetadata {
            source: *source,
            message_type: *message_type,
            client_id: None,
        },
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
            ProtocolMessage::VideoFrameFragment(fragment) => {
                ServerInboundRoute::VideoFrameFragment {
                    source: packet.source,
                    fragment,
                }
            }
            ProtocolMessage::ClientStats(stats) => ServerInboundRoute::ClientStats {
                source: packet.source,
                stats,
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
    VideoFrameFragment {
        source: PacketSource,
        fragment: VideoFrameFragment,
    },
    ClientStats {
        source: PacketSource,
        stats: ClientStats,
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
            ServerInboundRoute::VideoFrameFragment { .. } => {
                Err(ServerAuthBoundaryError::UnexpectedRoute {
                    message_type: MessageType::VideoFrameFragment,
                })
            }
            ServerInboundRoute::ClientStats { .. } => {
                Err(ServerAuthBoundaryError::UnexpectedRoute {
                    message_type: MessageType::ClientStats,
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
            SharedTokenSecretRef::SecretStore(reference) => {
                ServerSharedTokenSecretResolutionStatus::SecretStorePending {
                    reference: reference.clone(),
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
    SecretStorePending { reference: SecretStoreSecretRef },
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
            SharedTokenSecretRef::SecretStore(reference) => {
                Err(ServerSecretResolutionError::UnsupportedSecretStore {
                    token_id: token.token_id.clone(),
                    reference: reference.clone(),
                })
            }
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
            SharedTokenSecretRef::SecretStore(reference) => {
                ServerSecretResolutionPlan::NeedsSecretStore {
                    token_id: token.token_id.clone(),
                    reference: reference.clone(),
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
    MissingEnvironmentVariable {
        token_id: String,
        name: String,
    },
    EmptyEnvironmentVariable {
        token_id: String,
        name: String,
    },
    InvalidEnvironmentVariable {
        token_id: String,
        name: String,
    },
    UnsupportedSecretStore {
        token_id: String,
        reference: SecretStoreSecretRef,
    },
}

/// Resolution plan for one configured server shared token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerSecretResolutionPlan {
    AlreadyResolved(ServerResolvedSharedTokenAuthInput),
    NeedsEnvironmentVariable {
        token_id: String,
        name: String,
    },
    NeedsSecretStore {
        token_id: String,
        reference: SecretStoreSecretRef,
    },
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
            ServerSecretResolutionError::UnsupportedSecretStore { .. } => {
                "token secret store reference is not supported"
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
            SharedTokenSecretRef::SecretStore(_) => ServerAuthDecision::rejected(
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

/// Server-side token rotation plan.
///
/// MVP auth accepts one resolved token per client. Future manual overlap may
/// accept previous and current token material during a bounded operator-driven
/// window, but no multi-token comparison is implemented here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerSharedTokenRotationPlan {
    DisabledForMvp,
    ManualOverlapPlaceholder { overlap_window_seconds: u64 },
}

/// Boundary that normalizes config-level token rotation policy for server auth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerSharedTokenRotationBoundary;

impl ServerSharedTokenRotationBoundary {
    pub fn plan(&self, config: SharedTokenRotationConfig) -> ServerSharedTokenRotationPlan {
        match config.mode {
            SharedTokenRotationMode::DisabledForMvp => {
                ServerSharedTokenRotationPlan::DisabledForMvp
            }
            SharedTokenRotationMode::ManualOverlapPlaceholder {
                overlap_window_seconds,
            } => ServerSharedTokenRotationPlan::ManualOverlapPlaceholder {
                overlap_window_seconds,
            },
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
/// This boundary owns only the accepted-client to endpoint mapping shape and
/// can apply explicit invalidation commands from a higher policy layer. It does
/// not run UDP receive loops, persist state, decide timeout policy, or execute
/// reauthentication.
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

    pub fn invalidate(
        &self,
        registry: &mut AuthenticatedSenderRegistry,
        invalidation: AuthenticatedSenderInvalidation,
    ) -> AuthenticatedSenderInvalidationOutcome {
        let removed_entry = registry
            .entries_by_client_id
            .remove(invalidation.client_id.0.as_str());

        AuthenticatedSenderInvalidationOutcome {
            invalidation,
            removed_entry,
        }
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
            ServerInboundRoute::VideoFrameFragment { source, fragment } => self
                .evaluate_client_packet(
                    registry,
                    *source,
                    &fragment.client_id,
                    MessageType::VideoFrameFragment,
                ),
            ServerInboundRoute::ClientStats { source, stats } => self.evaluate_client_packet(
                registry,
                *source,
                &stats.client_id,
                MessageType::ClientStats,
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
    VideoFrameFragment(ServerRegisteredVideoFrameFragmentPacket),
    ClientStats(ServerRegisteredClientStatsPacket),
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

/// Handler input for an accepted video frame fragment packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRegisteredVideoFrameFragmentPacket {
    pub source: PacketSource,
    pub authenticated_sender: AuthenticatedSenderEntry,
    pub fragment: VideoFrameFragment,
}

/// Handler input for an accepted client stats packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRegisteredClientStatsPacket {
    pub source: PacketSource,
    pub authenticated_sender: AuthenticatedSenderEntry,
    pub stats: ClientStats,
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
            ServerInboundRoute::VideoFrameFragment { source, fragment } => {
                let authenticated_sender = self.require_sender(
                    registry,
                    &fragment.client_id,
                    source,
                    MessageType::VideoFrameFragment,
                )?;
                Ok(ServerRegisteredClientPacket::VideoFrameFragment(
                    ServerRegisteredVideoFrameFragmentPacket {
                        source,
                        authenticated_sender,
                        fragment,
                    },
                ))
            }
            ServerInboundRoute::ClientStats { source, stats } => {
                let authenticated_sender = self.require_sender(
                    registry,
                    &stats.client_id,
                    source,
                    MessageType::ClientStats,
                )?;
                Ok(ServerRegisteredClientPacket::ClientStats(
                    ServerRegisteredClientStatsPacket {
                        source,
                        authenticated_sender,
                        stats,
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

/// Input for building a typed server notice handoff.
///
/// This is not notice policy. It only carries already-decided notice fields into
/// the outbound message boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNoticeInput {
    pub destination: PacketSource,
    pub protocol_version: ProtocolVersion,
    pub run_id: RunId,
    pub notice_type: NoticeType,
    pub message: String,
}

/// Explicit server event that may become a `ServerNotice`.
///
/// This is not state transition detection. Future handlers decide that one of
/// these trigger sources happened, then pass it to the notice trigger boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerNoticeTriggerSource {
    Warning,
    Disconnect,
    ProtocolError,
    AuthExpired,
    ServerShutdown,
}

impl ServerNoticeTriggerSource {
    pub fn notice_type(self) -> NoticeType {
        match self {
            Self::Warning => NoticeType::Warning,
            Self::Disconnect => NoticeType::Disconnect,
            Self::ProtocolError => NoticeType::ProtocolError,
            Self::AuthExpired => NoticeType::AuthExpired,
            Self::ServerShutdown => NoticeType::ServerShutdown,
        }
    }
}

/// Input for planning a `ServerNotice` from an already-decided trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNoticeTriggerInput {
    pub destination: PacketSource,
    pub protocol_version: ProtocolVersion,
    pub run_id: RunId,
    pub source: ServerNoticeTriggerSource,
    pub message: String,
}

/// Planned notice output from a trigger policy boundary.
///
/// This remains a typed plan. It does not enqueue, encode, send, log, or mutate
/// server state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNoticeTriggerPlan {
    pub source: ServerNoticeTriggerSource,
    pub notice: ServerNoticeInput,
}

impl ServerNoticeTriggerPlan {
    pub fn into_notice_input(self) -> ServerNoticeInput {
        self.notice
    }
}

/// Minimal trigger policy boundary for `ServerNotice`.
///
/// The boundary only maps an explicit trigger source to the corresponding
/// notice type and preserves destination/run context. It does not decide that a
/// trigger happened and does not suppress, rate-limit, enqueue, or send notices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerNoticeTriggerPolicyBoundary;

impl ServerNoticeTriggerPolicyBoundary {
    pub fn plan_notice(&self, input: ServerNoticeTriggerInput) -> ServerNoticeTriggerPlan {
        let notice_type = input.source.notice_type();
        ServerNoticeTriggerPlan {
            source: input.source,
            notice: ServerNoticeInput {
                destination: input.destination,
                protocol_version: input.protocol_version,
                run_id: input.run_id,
                notice_type,
                message: input.message,
            },
        }
    }
}

/// Boundary that converts server notice fields into a typed outbound message.
///
/// This does not decide when to notify, encode bytes, enqueue real items, write
/// logs, or send through UDP sockets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerNoticeBoundary;

impl ServerNoticeBoundary {
    pub fn build_for_send(&self, input: ServerNoticeInput) -> ServerOutboundNotice {
        let notice = ServerNotice {
            message_type: MessageType::ServerNotice,
            protocol_version: input.protocol_version,
            run_id: input.run_id,
            notice_type: input.notice_type,
            message: input.message,
        };

        ServerOutboundNotice {
            destination: input.destination,
            message: ProtocolMessage::ServerNotice(notice),
        }
    }
}

/// ServerNotice handoff for a future net send layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOutboundNotice {
    pub destination: PacketSource,
    pub message: ProtocolMessage,
}

impl ServerOutboundNotice {
    pub fn server_notice(&self) -> Option<&ServerNotice> {
        match &self.message {
            ProtocolMessage::ServerNotice(notice) => Some(notice),
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

/// Input for future heartbeat liveness state updates.
///
/// This is a data handoff only. It does not update state or evaluate timeout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatStateInput {
    pub source: PacketSource,
    pub authenticated_sender: AuthenticatedSenderEntry,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub heartbeat_sent_at: TimestampMicros,
    pub server_received_at: TimestampMicros,
    pub short_status: Option<String>,
}

/// Current server-side liveness status for one authenticated heartbeat sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatLivenessStatus {
    Alive,
    TimedOut,
}

/// In-memory heartbeat liveness state for one client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatLivenessEntry {
    pub source: PacketSource,
    pub authenticated_sender: AuthenticatedSenderEntry,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub last_heartbeat_sent_at: TimestampMicros,
    pub last_server_received_at: TimestampMicros,
    pub last_short_status: Option<String>,
    pub received_heartbeats: u64,
    pub status: ServerHeartbeatLivenessStatus,
}

/// In-memory heartbeat liveness state keyed by `client_id`.
///
/// This is not a durable store and does not revoke auth registry entries. It is
/// a small commit target for registered heartbeat observations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerHeartbeatLivenessState {
    entries_by_client_id: BTreeMap<String, ServerHeartbeatLivenessEntry>,
}

impl ServerHeartbeatLivenessState {
    pub fn entries(&self) -> impl Iterator<Item = &ServerHeartbeatLivenessEntry> {
        self.entries_by_client_id.values()
    }

    pub fn get(&self, client_id: &ClientId) -> Option<&ServerHeartbeatLivenessEntry> {
        self.entries_by_client_id.get(client_id.0.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries_by_client_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries_by_client_id.is_empty()
    }
}

/// Result of committing one registered heartbeat into liveness state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatLivenessCommitOutcome {
    pub previous: Option<ServerHeartbeatLivenessEntry>,
    pub committed: ServerHeartbeatLivenessEntry,
}

/// Explicit timeout policy for future liveness evaluation.
///
/// The current one-shot runtimes can evaluate this policy, but they do not run
/// a continuous timeout scanner or remove authenticated sender entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutPolicy {
    pub timeout_after_micros: u64,
}

impl ServerHeartbeatTimeoutPolicy {
    pub fn new(timeout_after_micros: u64) -> Self {
        Self {
            timeout_after_micros,
        }
    }
}

/// Timeout evaluation for one client at a caller-supplied server timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatTimeoutEvaluation {
    NoHeartbeat {
        client_id: ClientId,
    },
    Alive {
        client_id: ClientId,
        last_server_received_at: TimestampMicros,
        elapsed_micros: u64,
        timeout_after_micros: u64,
    },
    TimedOut {
        client_id: ClientId,
        last_server_received_at: TimestampMicros,
        elapsed_micros: u64,
        timeout_after_micros: u64,
    },
}

/// Reason for removing an authenticated sender entry through an explicit policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthenticatedSenderInvalidationReason {
    HeartbeatTimeout,
}

/// Explicit invalidation command for the authenticated sender registry.
///
/// This is produced by a higher policy boundary. The registry boundary applies
/// it, but does not decide when timeout or reauthentication should happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSenderInvalidation {
    pub client_id: ClientId,
    pub source: PacketSource,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub reason: AuthenticatedSenderInvalidationReason,
    pub invalidated_at: TimestampMicros,
}

/// Result of applying one authenticated sender invalidation command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSenderInvalidationOutcome {
    pub invalidation: AuthenticatedSenderInvalidation,
    pub removed_entry: Option<AuthenticatedSenderEntry>,
}

/// Timeout log handoff produced from a timed-out heartbeat evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutLogInput {
    pub source: PacketSource,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub last_server_received_at: TimestampMicros,
    pub evaluated_at: TimestampMicros,
    pub elapsed_micros: u64,
    pub timeout_after_micros: u64,
    pub registry_invalidation_planned: bool,
    pub notice_planned: bool,
}

/// Event name for future heartbeat timeout JSON Lines records.
pub const SERVER_HEARTBEAT_TIMEOUT_JSON_LOG_EVENT_NAME: &str = "server.heartbeat_timeout";

/// JSON Lines event input for heartbeat timeout observations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutJsonLogEventInput {
    pub event_name: &'static str,
    pub source: PacketSource,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub last_server_received_at: TimestampMicros,
    pub evaluated_at: TimestampMicros,
    pub elapsed_micros: u64,
    pub timeout_after_micros: u64,
    pub registry_invalidation_planned: bool,
    pub notice_planned: bool,
}

/// Boundary that maps timeout log handoff input to a JSON Lines event shape.
///
/// This does not write files, update metrics, apply invalidation, enqueue
/// notices, or run a timeout scanner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutJsonLogEventBoundary;

impl ServerHeartbeatTimeoutJsonLogEventBoundary {
    pub fn build_event(
        &self,
        input: ServerHeartbeatTimeoutLogInput,
    ) -> ServerHeartbeatTimeoutJsonLogEventInput {
        ServerHeartbeatTimeoutJsonLogEventInput {
            event_name: SERVER_HEARTBEAT_TIMEOUT_JSON_LOG_EVENT_NAME,
            source: input.source,
            client_id: input.client_id,
            run_id: input.run_id,
            protocol_version: input.protocol_version,
            last_server_received_at: input.last_server_received_at,
            evaluated_at: input.evaluated_at,
            elapsed_micros: input.elapsed_micros,
            timeout_after_micros: input.timeout_after_micros,
            registry_invalidation_planned: input.registry_invalidation_planned,
            notice_planned: input.notice_planned,
        }
    }
}

/// Minimal heartbeat timeout log output boundary.
///
/// This writes one timeout JSON Lines record to a caller-owned writer. It does
/// not open files, rotate logs, install a process-wide logger, apply registry
/// invalidation, enqueue notices, or run a timeout scanner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutLogOutputBoundary {
    event: ServerHeartbeatTimeoutJsonLogEventBoundary,
    writer: ServerHeartbeatTimeoutJsonLineWriter,
}

impl ServerHeartbeatTimeoutLogOutputBoundary {
    pub fn write_timeout<W: io::Write>(
        &self,
        input: ServerHeartbeatTimeoutLogInput,
        writer: W,
    ) -> io::Result<ServerHeartbeatTimeoutJsonLogEventInput> {
        let event = self.event.build_event(input);
        self.writer.write_event(&event, writer)?;
        Ok(event)
    }
}

/// Minimal JSON Lines writer for heartbeat timeout events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutJsonLineWriter;

impl ServerHeartbeatTimeoutJsonLineWriter {
    pub fn write_event<W: io::Write>(
        &self,
        event: &ServerHeartbeatTimeoutJsonLogEventInput,
        mut writer: W,
    ) -> io::Result<()> {
        write!(writer, "{{")?;
        write_json_field(&mut writer, "event_name", event.event_name)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "source", &event.source.address.to_string())?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "client_id", &event.client_id.0)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "run_id", &event.run_id.0)?;
        write!(writer, ",\"protocol_version\":{}", event.protocol_version.0)?;
        write!(
            writer,
            ",\"last_server_received_at\":{}",
            event.last_server_received_at.0
        )?;
        write!(writer, ",\"evaluated_at\":{}", event.evaluated_at.0)?;
        write!(writer, ",\"elapsed_micros\":{}", event.elapsed_micros)?;
        write!(
            writer,
            ",\"timeout_after_micros\":{}",
            event.timeout_after_micros
        )?;
        write!(
            writer,
            ",\"registry_invalidation_planned\":{}",
            event.registry_invalidation_planned
        )?;
        write!(writer, ",\"notice_planned\":{}", event.notice_planned)?;
        writeln!(writer, "}}")
    }
}

/// Planned effects for one heartbeat timeout evaluation.
///
/// The plan is a typed handoff only. Applying registry invalidation, writing
/// logs, enqueueing notices, and sending UDP packets are separate steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutActionPlan {
    pub evaluation: ServerHeartbeatTimeoutEvaluation,
    pub registry_invalidation: Option<AuthenticatedSenderInvalidation>,
    pub timeout_log: Option<ServerHeartbeatTimeoutLogInput>,
    pub notice: Option<ServerNoticeTriggerPlan>,
}

/// Boundary that connects timeout evaluation to later invalidation/log/notice
/// handoffs.
///
/// It plans effects only for `TimedOut` entries that still exist in liveness
/// state. It does not mutate state, remove registry entries, write logs,
/// enqueue notices, send UDP packets, or run a continuous loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutActionBoundary {
    notice: ServerNoticeTriggerPolicyBoundary,
}

impl ServerHeartbeatTimeoutActionBoundary {
    pub fn plan_actions(
        &self,
        state: &ServerHeartbeatLivenessState,
        evaluation: ServerHeartbeatTimeoutEvaluation,
        evaluated_at: TimestampMicros,
    ) -> ServerHeartbeatTimeoutActionPlan {
        let (registry_invalidation, timeout_log, notice) = match &evaluation {
            ServerHeartbeatTimeoutEvaluation::TimedOut {
                client_id,
                last_server_received_at,
                elapsed_micros,
                timeout_after_micros,
            } => {
                let Some(entry) = state.get(client_id) else {
                    return ServerHeartbeatTimeoutActionPlan {
                        evaluation,
                        registry_invalidation: None,
                        timeout_log: None,
                        notice: None,
                    };
                };
                let invalidation = AuthenticatedSenderInvalidation {
                    client_id: entry.client_id.clone(),
                    source: entry.source,
                    run_id: entry.run_id.clone(),
                    protocol_version: entry.protocol_version,
                    reason: AuthenticatedSenderInvalidationReason::HeartbeatTimeout,
                    invalidated_at: evaluated_at,
                };
                let log = ServerHeartbeatTimeoutLogInput {
                    source: entry.source,
                    client_id: entry.client_id.clone(),
                    run_id: entry.run_id.clone(),
                    protocol_version: entry.protocol_version,
                    last_server_received_at: *last_server_received_at,
                    evaluated_at,
                    elapsed_micros: *elapsed_micros,
                    timeout_after_micros: *timeout_after_micros,
                    registry_invalidation_planned: true,
                    notice_planned: true,
                };
                let notice = self.notice.plan_notice(ServerNoticeTriggerInput {
                    destination: entry.source,
                    protocol_version: entry.protocol_version,
                    run_id: entry.run_id.clone(),
                    source: ServerNoticeTriggerSource::AuthExpired,
                    message: format!(
                        "heartbeat timeout: elapsed_micros={} timeout_after_micros={}",
                        elapsed_micros, timeout_after_micros
                    ),
                });

                (Some(invalidation), Some(log), Some(notice))
            }
            ServerHeartbeatTimeoutEvaluation::Alive { .. }
            | ServerHeartbeatTimeoutEvaluation::NoHeartbeat { .. } => (None, None, None),
        };

        ServerHeartbeatTimeoutActionPlan {
            evaluation,
            registry_invalidation,
            timeout_log,
            notice,
        }
    }
}

/// Notice handoff produced while applying one heartbeat timeout action plan.
///
/// This reaches the typed outbound queue item boundary only. Queue storage,
/// encoding, UDP send, retry, and duplicate suppression remain separate work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutNoticeHandoff {
    pub trigger_plan: ServerNoticeTriggerPlan,
    pub outbound_notice: ServerOutboundNotice,
    pub queue_item: OutboundQueueItem,
}

/// Result of applying one timeout action plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutApplyResult {
    pub evaluation: ServerHeartbeatTimeoutEvaluation,
    pub registry_invalidation: Option<AuthenticatedSenderInvalidationOutcome>,
    pub timeout_log_event: Option<ServerHeartbeatTimeoutJsonLogEventInput>,
    pub notice_handoff: Option<ServerHeartbeatTimeoutNoticeHandoff>,
}

/// Boundary that applies planned heartbeat timeout effects for a future loop.
///
/// This is the smallest apply point a continuous heartbeat loop can call after
/// evaluation and action planning. It may remove one registry entry, write one
/// timeout record to a caller-owned writer, and create one typed notice queue
/// item. It does not scan clients, open files, store notice queue items, encode
/// packets, send UDP, retry, rate-limit notices, or request reauthentication.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutApplyBoundary {
    registry: AuthenticatedSenderRegistryBoundary,
    timeout_log: ServerHeartbeatTimeoutLogOutputBoundary,
    notice: ServerNoticeBoundary,
    queue: ServerOutboundQueueBoundary,
}

impl ServerHeartbeatTimeoutApplyBoundary {
    pub fn apply_plan<W: io::Write>(
        &self,
        registry: &mut AuthenticatedSenderRegistry,
        plan: ServerHeartbeatTimeoutActionPlan,
        mut timeout_log_writer: W,
    ) -> io::Result<ServerHeartbeatTimeoutApplyResult> {
        let evaluation = plan.evaluation;
        let registry_invalidation = plan
            .registry_invalidation
            .map(|invalidation| self.registry.invalidate(registry, invalidation));
        let timeout_log_event = match plan.timeout_log {
            Some(input) => Some(
                self.timeout_log
                    .write_timeout(input, &mut timeout_log_writer)?,
            ),
            None => None,
        };
        let notice_handoff = plan.notice.map(|trigger_plan| {
            let outbound_notice = self
                .notice
                .build_for_send(trigger_plan.clone().into_notice_input());
            let queue_item = self.queue.handoff_notice(outbound_notice.clone());
            ServerHeartbeatTimeoutNoticeHandoff {
                trigger_plan,
                outbound_notice,
                queue_item,
            }
        });

        Ok(ServerHeartbeatTimeoutApplyResult {
            evaluation,
            registry_invalidation,
            timeout_log_event,
            notice_handoff,
        })
    }
}

/// Reason a future send loop should be woken after timeout notice storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatTimeoutNoticeSendWakeupReason {
    TimeoutNoticeQueued,
}

/// Placeholder plan for waking a future send loop.
///
/// This does not signal a condvar, spawn a task, send a datagram, or run a send
/// loop. It only records whether the caller should wake whatever send-loop
/// mechanism later owns the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatTimeoutNoticeSendWakeupPlan {
    NotRequested,
    RequestSendLoopWakeup {
        reason: ServerHeartbeatTimeoutNoticeSendWakeupReason,
    },
}

/// Stored timeout notice queue item plus the future wakeup request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutNoticeQueueStored {
    pub decision: OutboundQueueStorageDecision,
    pub queued_item: QueuedOutboundItem,
    pub queue_len: usize,
    pub wakeup: ServerHeartbeatTimeoutNoticeSendWakeupPlan,
}

/// Dropped timeout notice queue item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutNoticeQueueDropped {
    pub decision: OutboundQueueStorageDecision,
    pub queue_len: usize,
    pub wakeup: ServerHeartbeatTimeoutNoticeSendWakeupPlan,
}

/// Result of applying timeout notice handoff to caller-owned queue storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatTimeoutNoticeQueueStorageResult {
    NoNotice {
        wakeup: ServerHeartbeatTimeoutNoticeSendWakeupPlan,
    },
    Stored(ServerHeartbeatTimeoutNoticeQueueStored),
    Dropped(ServerHeartbeatTimeoutNoticeQueueDropped),
}

impl ServerHeartbeatTimeoutNoticeQueueStorageResult {
    pub fn wakeup(&self) -> ServerHeartbeatTimeoutNoticeSendWakeupPlan {
        match self {
            ServerHeartbeatTimeoutNoticeQueueStorageResult::NoNotice { wakeup } => *wakeup,
            ServerHeartbeatTimeoutNoticeQueueStorageResult::Stored(stored) => stored.wakeup,
            ServerHeartbeatTimeoutNoticeQueueStorageResult::Dropped(dropped) => dropped.wakeup,
        }
    }
}

/// Boundary that stores timeout notice queue items and plans future send wakeup.
///
/// This is the narrow bridge after `ServerHeartbeatTimeoutApplyBoundary`: it
/// moves an optional notice handoff into caller-owned in-memory queue storage
/// and returns a wakeup placeholder only when an item is actually queued. It
/// does not scan clients, run continuously, wake a thread, encode, send UDP,
/// retry, or open log sinks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutNoticeQueueStorageBoundary {
    queue: ServerOutboundQueueBoundary,
    lifecycle: OutboundQueueLifecycleBoundary,
}

impl ServerHeartbeatTimeoutNoticeQueueStorageBoundary {
    pub fn store_notice(
        &self,
        collection: &mut ServerOutboundQueueCollection,
        apply: &ServerHeartbeatTimeoutApplyResult,
    ) -> ServerHeartbeatTimeoutNoticeQueueStorageResult {
        let Some(notice) = &apply.notice_handoff else {
            return ServerHeartbeatTimeoutNoticeQueueStorageResult::NoNotice {
                wakeup: ServerHeartbeatTimeoutNoticeSendWakeupPlan::NotRequested,
            };
        };

        let decision = self
            .queue
            .evaluate_storage_push(collection.len(), &notice.queue_item);
        if !decision.accepts_candidate() {
            return ServerHeartbeatTimeoutNoticeQueueStorageResult::Dropped(
                ServerHeartbeatTimeoutNoticeQueueDropped {
                    decision,
                    queue_len: collection.len(),
                    wakeup: ServerHeartbeatTimeoutNoticeSendWakeupPlan::NotRequested,
                },
            );
        }

        let queued_item = self.lifecycle.hold_for_send(notice.queue_item.clone());
        collection.items.push_back(queued_item.clone());

        ServerHeartbeatTimeoutNoticeQueueStorageResult::Stored(
            ServerHeartbeatTimeoutNoticeQueueStored {
                decision,
                queued_item,
                queue_len: collection.len(),
                wakeup: ServerHeartbeatTimeoutNoticeSendWakeupPlan::RequestSendLoopWakeup {
                    reason: ServerHeartbeatTimeoutNoticeSendWakeupReason::TimeoutNoticeQueued,
                },
            },
        )
    }
}

/// Input for one future heartbeat timeout loop tick for a single client.
///
/// The caller chooses which client to evaluate and supplies the current server
/// timestamp. This input does not imply scanning, sleeping, or a completed
/// continuous heartbeat loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutLoopTickInput {
    pub client_id: ClientId,
    pub evaluated_at: TimestampMicros,
    pub policy: ServerHeartbeatTimeoutPolicy,
}

/// Result of one future heartbeat timeout loop tick for a single client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutLoopTickResult {
    pub input: ServerHeartbeatTimeoutLoopTickInput,
    pub action_plan: ServerHeartbeatTimeoutActionPlan,
    pub apply: ServerHeartbeatTimeoutApplyResult,
}

/// Boundary a future continuous heartbeat loop can call for one client.
///
/// This composes timeout evaluation, action planning, and apply for exactly one
/// caller-selected client. It does not scan all clients, loop, sleep, open file
/// sinks, store notice queue items, encode, send UDP packets, retry, or manage
/// reauthentication.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutLoopTickBoundary {
    liveness: ServerHeartbeatLivenessCommitBoundary,
    action: ServerHeartbeatTimeoutActionBoundary,
    apply: ServerHeartbeatTimeoutApplyBoundary,
}

impl ServerHeartbeatTimeoutLoopTickBoundary {
    pub fn run_one_client<W: io::Write>(
        &self,
        liveness_state: &ServerHeartbeatLivenessState,
        registry: &mut AuthenticatedSenderRegistry,
        input: ServerHeartbeatTimeoutLoopTickInput,
        timeout_log_writer: W,
    ) -> io::Result<ServerHeartbeatTimeoutLoopTickResult> {
        let evaluation = self.liveness.evaluate_timeout(
            liveness_state,
            &input.client_id,
            input.evaluated_at,
            input.policy,
        );
        let action_plan = self
            .action
            .plan_actions(liveness_state, evaluation, input.evaluated_at);
        let apply = self
            .apply
            .apply_plan(registry, action_plan.clone(), timeout_log_writer)?;

        Ok(ServerHeartbeatTimeoutLoopTickResult {
            input,
            action_plan,
            apply,
        })
    }
}

/// Input for one multi-client heartbeat timeout loop pass.
///
/// The caller owns the clock sample and policy. This input does not imply
/// sleeping, receiving packets, sending notices, or owning registry/state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutMultiClientLoopInput {
    pub evaluated_at: TimestampMicros,
    pub policy: ServerHeartbeatTimeoutPolicy,
}

/// Result for one registered client processed by the multi-client timeout loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimeoutMultiClientLoopClientResult {
    pub client_id: ClientId,
    pub tick: ServerHeartbeatTimeoutLoopTickResult,
    pub notice_queue_storage: ServerHeartbeatTimeoutNoticeQueueStorageResult,
}

/// Result of one multi-client heartbeat timeout loop pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatTimeoutMultiClientLoopResult {
    NoClientsAvailable {
        input: ServerHeartbeatTimeoutMultiClientLoopInput,
    },
    AllClientsProcessed {
        input: ServerHeartbeatTimeoutMultiClientLoopInput,
        processed: Vec<ServerHeartbeatTimeoutMultiClientLoopClientResult>,
        timeout_actions_applied: usize,
    },
}

/// Thin multi-client heartbeat timeout loop over the existing one-client tick.
///
/// This boundary snapshots authenticated client ids from the caller-owned
/// registry, invokes `ServerHeartbeatTimeoutLoopTickBoundary` once per client,
/// and stores timeout notice handoffs into caller-owned queue storage. It does
/// not reinterpret one-client tick semantics, execute send wakeups, send UDP,
/// receive packets, sleep, or own registry/liveness/queue/writer state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimeoutMultiClientLoopBoundary {
    one_client: ServerHeartbeatTimeoutLoopTickBoundary,
    notice_queue: ServerHeartbeatTimeoutNoticeQueueStorageBoundary,
}

impl ServerHeartbeatTimeoutMultiClientLoopBoundary {
    pub fn run_all_registered<W: io::Write>(
        &self,
        liveness_state: &ServerHeartbeatLivenessState,
        registry: &mut AuthenticatedSenderRegistry,
        notice_queue: &mut ServerOutboundQueueCollection,
        input: ServerHeartbeatTimeoutMultiClientLoopInput,
        mut timeout_log_writer: W,
    ) -> io::Result<ServerHeartbeatTimeoutMultiClientLoopResult> {
        let client_ids: Vec<ClientId> = registry
            .entries()
            .map(|entry| entry.client_id.clone())
            .collect();

        if client_ids.is_empty() {
            return Ok(ServerHeartbeatTimeoutMultiClientLoopResult::NoClientsAvailable { input });
        }

        let mut processed = Vec::with_capacity(client_ids.len());
        let mut timeout_actions_applied = 0usize;

        for client_id in client_ids {
            let tick = self.one_client.run_one_client(
                liveness_state,
                registry,
                ServerHeartbeatTimeoutLoopTickInput {
                    client_id: client_id.clone(),
                    evaluated_at: input.evaluated_at,
                    policy: input.policy,
                },
                &mut timeout_log_writer,
            )?;
            if tick.apply.registry_invalidation.is_some()
                || tick.apply.timeout_log_event.is_some()
                || tick.apply.notice_handoff.is_some()
            {
                timeout_actions_applied = timeout_actions_applied.saturating_add(1);
            }
            let notice_queue_storage = self.notice_queue.store_notice(notice_queue, &tick.apply);
            processed.push(ServerHeartbeatTimeoutMultiClientLoopClientResult {
                client_id,
                tick,
                notice_queue_storage,
            });
        }

        Ok(
            ServerHeartbeatTimeoutMultiClientLoopResult::AllClientsProcessed {
                input,
                processed,
                timeout_actions_applied,
            },
        )
    }
}

/// Boundary that commits heartbeat liveness state and evaluates timeout policy.
///
/// This boundary consumes `ServerHeartbeatStateInput` produced by the heartbeat
/// handler. It does not read clocks, send notices, revoke authentication,
/// remove registry entries, or run a continuous heartbeat loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatLivenessCommitBoundary;

impl ServerHeartbeatLivenessCommitBoundary {
    pub fn commit(
        &self,
        state: &mut ServerHeartbeatLivenessState,
        input: ServerHeartbeatStateInput,
    ) -> ServerHeartbeatLivenessCommitOutcome {
        let key = input.client_id.0.clone();
        let previous = state.entries_by_client_id.get(&key).cloned();
        let received_heartbeats = previous
            .as_ref()
            .map(|entry| entry.received_heartbeats.saturating_add(1))
            .unwrap_or(1);
        let committed = ServerHeartbeatLivenessEntry {
            source: input.source,
            authenticated_sender: input.authenticated_sender,
            client_id: input.client_id,
            run_id: input.run_id,
            protocol_version: input.protocol_version,
            last_heartbeat_sent_at: input.heartbeat_sent_at,
            last_server_received_at: input.server_received_at,
            last_short_status: input.short_status,
            received_heartbeats,
            status: ServerHeartbeatLivenessStatus::Alive,
        };

        state.entries_by_client_id.insert(key, committed.clone());

        ServerHeartbeatLivenessCommitOutcome {
            previous,
            committed,
        }
    }

    pub fn evaluate_timeout(
        &self,
        state: &ServerHeartbeatLivenessState,
        client_id: &ClientId,
        now: TimestampMicros,
        policy: ServerHeartbeatTimeoutPolicy,
    ) -> ServerHeartbeatTimeoutEvaluation {
        let Some(entry) = state.get(client_id) else {
            return ServerHeartbeatTimeoutEvaluation::NoHeartbeat {
                client_id: client_id.clone(),
            };
        };
        let elapsed_micros = now.0.saturating_sub(entry.last_server_received_at.0);

        if elapsed_micros >= policy.timeout_after_micros {
            ServerHeartbeatTimeoutEvaluation::TimedOut {
                client_id: client_id.clone(),
                last_server_received_at: entry.last_server_received_at,
                elapsed_micros,
                timeout_after_micros: policy.timeout_after_micros,
            }
        } else {
            ServerHeartbeatTimeoutEvaluation::Alive {
                client_id: client_id.clone(),
                last_server_received_at: entry.last_server_received_at,
                elapsed_micros,
                timeout_after_micros: policy.timeout_after_micros,
            }
        }
    }
}

/// Input sample for future RTT / offset estimation.
///
/// The timestamp values preserve their original clock domains. This type does
/// not calculate RTT, offset, or smoothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimebaseInput {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub client_sent_at: TimestampMicros,
    pub client_local_time: Option<TimestampMicros>,
    pub server_received_at: TimestampMicros,
    pub server_sent_at: TimestampMicros,
}

/// Server bridge from heartbeat timebase input to the timebase crate plan.
///
/// This preserves app-level ids beside the estimator plan. It does not compute
/// RTT, offset, or smoothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatTimebasePlan {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub estimate: HeartbeatTimebaseEstimatePlan,
}

/// Boundary that converts server heartbeat timebase input into estimator plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatTimebasePlanBoundary {
    timebase: HeartbeatTimebasePlanBoundary,
}

impl ServerHeartbeatTimebasePlanBoundary {
    pub fn build_plan(&self, input: &ServerHeartbeatTimebaseInput) -> ServerHeartbeatTimebasePlan {
        let sample = HeartbeatTimebaseSample {
            client_sent_at_micros: input.client_sent_at.0,
            client_local_time_micros: input.client_local_time.map(|timestamp| timestamp.0),
            server_received_at_micros: input.server_received_at.0,
            server_sent_at_micros: input.server_sent_at.0,
        };

        ServerHeartbeatTimebasePlan {
            client_id: input.client_id.clone(),
            run_id: input.run_id.clone(),
            estimate: self.timebase.build_plan(sample),
        }
    }
}

/// Future client-side observation needed to complete the small RTT / offset unit.
///
/// This is not produced by the current heartbeat handler. A future client
/// report can carry this observation back to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatClientAckObservation {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub echoed_client_sent_at: TimestampMicros,
    pub server_received_at: TimestampMicros,
    pub server_sent_at: TimestampMicros,
    pub client_received_at: TimestampMicros,
}

/// Boundary that accepts a protocol-level HeartbeatAck observation for server use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatClientAckObservationBoundary;

impl ServerHeartbeatClientAckObservationBoundary {
    pub fn prepare(
        &self,
        observation: HeartbeatAckObservation,
    ) -> ServerHeartbeatClientAckObservation {
        ServerHeartbeatClientAckObservation {
            client_id: observation.client_id,
            run_id: observation.run_id,
            echoed_client_sent_at: observation.echoed_sent_at,
            server_received_at: observation.server_received_at,
            server_sent_at: observation.server_sent_at,
            client_received_at: observation.client_received_at,
        }
    }
}

/// Server boundary for accepting the future heartbeat observation carrier.
///
/// This unwraps the typed carrier only. Receive routing, packet acceptance,
/// and `ClientStats` handler wiring remain future work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatObservationCarrierBoundary {
    observation: ServerHeartbeatClientAckObservationBoundary,
}

impl ServerHeartbeatObservationCarrierBoundary {
    pub fn prepare(
        &self,
        carrier: HeartbeatObservationCarrier,
    ) -> ServerHeartbeatClientAckObservation {
        self.observation.prepare(carrier.observation)
    }
}

/// Stats fields prepared for a future metrics/state update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerClientStatsStateInput {
    pub source: PacketSource,
    pub authenticated_sender: AuthenticatedSenderEntry,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub sent_at: TimestampMicros,
    pub capture_fps: u32,
    pub dropped_frames: u64,
    pub bitrate_kbps: u32,
}

/// Handler input extracted from a registered ClientStats packet.
///
/// The heartbeat observation is already converted to the server timebase input
/// shape when present, but no RTT/offset calculation or state commit happens
/// here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerClientStatsHandlerInput {
    pub state: ServerClientStatsStateInput,
    pub heartbeat_observation: Option<ServerHeartbeatClientAckObservation>,
}

/// Minimal ClientStats handler bridge.
///
/// This boundary separates decoded/gated ClientStats handling from protocol
/// decode and receive loop work. It does not update metrics, commit timebase
/// estimates, emit logs, send responses, or run a continuous stats loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerClientStatsHandlerBoundary {
    observation: ServerHeartbeatClientAckObservationBoundary,
}

impl ServerClientStatsHandlerBoundary {
    pub fn prepare_input(
        &self,
        packet: ServerRegisteredClientStatsPacket,
    ) -> ServerClientStatsHandlerInput {
        let heartbeat_observation = packet
            .stats
            .heartbeat_observation
            .clone()
            .map(|observation| self.observation.prepare(observation));

        ServerClientStatsHandlerInput {
            state: ServerClientStatsStateInput {
                source: packet.source,
                authenticated_sender: packet.authenticated_sender,
                client_id: packet.stats.client_id,
                run_id: packet.stats.run_id,
                protocol_version: packet.stats.protocol_version,
                sent_at: packet.stats.sent_at,
                capture_fps: packet.stats.capture_fps,
                dropped_frames: packet.stats.dropped_frames,
                bitrate_kbps: packet.stats.bitrate_kbps,
            },
            heartbeat_observation,
        }
    }
}

/// One stateless RTT / offset calculation result with server correlation ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetCalculation {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub estimate: HeartbeatRttOffsetEstimate,
}

/// Server-side latest committed RTT / offset estimate for one client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetStateEntry {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub latest_estimate: HeartbeatRttOffsetEstimate,
    pub committed_samples: u64,
    pub last_committed_at: Option<TimestampMicros>,
}

/// In-memory server-side RTT / offset state keyed by `client_id`.
///
/// This stores only the latest accepted estimate and sample count. It does not
/// smooth offset, reject outliers, expose corrected timestamps, or drive
/// timeout policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetState {
    entries_by_client_id: BTreeMap<String, ServerHeartbeatRttOffsetStateEntry>,
}

impl ServerHeartbeatRttOffsetState {
    pub fn entries(&self) -> impl Iterator<Item = &ServerHeartbeatRttOffsetStateEntry> {
        self.entries_by_client_id.values()
    }

    pub fn get(&self, client_id: &ClientId) -> Option<&ServerHeartbeatRttOffsetStateEntry> {
        self.entries_by_client_id.get(client_id.0.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries_by_client_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries_by_client_id.is_empty()
    }
}

/// Input for committing one calculated RTT / offset estimate to server state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetCommitInput {
    pub calculation: ServerHeartbeatRttOffsetCalculation,
    pub committed_at: Option<TimestampMicros>,
}

/// Result of committing one RTT / offset estimate to server state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetCommitOutcome {
    pub previous: Option<ServerHeartbeatRttOffsetStateEntry>,
    pub committed: ServerHeartbeatRttOffsetStateEntry,
    pub replaced_previous_run: bool,
}

/// Smoothing mode for server-side RTT / offset candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatRttOffsetSmoothingMode {
    Deferred,
}

impl Default for ServerHeartbeatRttOffsetSmoothingMode {
    fn default() -> Self {
        Self::Deferred
    }
}

/// Minimal outlier policy for candidate evaluation.
///
/// `None` disables each threshold. This is a guardrail only; it is not a
/// complete outlier model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetOutlierPolicy {
    pub max_rtt_delta_micros: Option<u64>,
    pub max_clock_offset_delta_micros: Option<u64>,
}

/// Minimal candidate policy for future estimator work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetCandidatePolicy {
    pub smoothing: ServerHeartbeatRttOffsetSmoothingMode,
    pub outlier: ServerHeartbeatRttOffsetOutlierPolicy,
}

/// Reason a candidate was classified as an outlier by the minimal policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatRttOffsetOutlierReason {
    RttDeltaExceeded {
        previous_micros: u64,
        candidate_micros: u64,
        delta_micros: u64,
        max_delta_micros: u64,
    },
    ClockOffsetDeltaExceeded {
        previous_micros: i64,
        candidate_micros: i64,
        delta_micros: u64,
        max_delta_micros: u64,
    },
}

/// Candidate policy decision before committing latest RTT / offset state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatRttOffsetCandidatePolicyDecision {
    Accept {
        smoothing: ServerHeartbeatRttOffsetSmoothingMode,
    },
    RejectOutlier {
        reason: ServerHeartbeatRttOffsetOutlierReason,
    },
}

/// Result of evaluating one RTT / offset candidate against policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetCandidatePolicyResult {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub previous: Option<ServerHeartbeatRttOffsetStateEntry>,
    pub candidate: HeartbeatRttOffsetEstimate,
    pub decision: ServerHeartbeatRttOffsetCandidatePolicyDecision,
}

/// Boundary that evaluates an RTT / offset candidate before state commit.
///
/// This boundary only compares a candidate with the latest same-run estimate
/// when optional thresholds are configured. It does not calculate RTT / offset,
/// commit state, smooth values, keep history, publish corrected timestamps, or
/// interact with timeout state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetCandidatePolicyBoundary;

impl ServerHeartbeatRttOffsetCandidatePolicyBoundary {
    pub fn evaluate(
        &self,
        state: &ServerHeartbeatRttOffsetState,
        calculation: &ServerHeartbeatRttOffsetCalculation,
        policy: ServerHeartbeatRttOffsetCandidatePolicy,
    ) -> ServerHeartbeatRttOffsetCandidatePolicyResult {
        let previous = state.get(&calculation.client_id).cloned();
        let decision = previous
            .as_ref()
            .filter(|entry| entry.run_id == calculation.run_id)
            .and_then(|entry| {
                self.evaluate_outlier(entry.latest_estimate, calculation.estimate, policy.outlier)
            })
            .map(|reason| ServerHeartbeatRttOffsetCandidatePolicyDecision::RejectOutlier { reason })
            .unwrap_or(ServerHeartbeatRttOffsetCandidatePolicyDecision::Accept {
                smoothing: policy.smoothing,
            });

        ServerHeartbeatRttOffsetCandidatePolicyResult {
            client_id: calculation.client_id.clone(),
            run_id: calculation.run_id.clone(),
            previous,
            candidate: calculation.estimate,
            decision,
        }
    }

    fn evaluate_outlier(
        &self,
        previous: HeartbeatRttOffsetEstimate,
        candidate: HeartbeatRttOffsetEstimate,
        policy: ServerHeartbeatRttOffsetOutlierPolicy,
    ) -> Option<ServerHeartbeatRttOffsetOutlierReason> {
        if let Some(max_delta_micros) = policy.max_rtt_delta_micros {
            let delta_micros = previous.rtt_micros.abs_diff(candidate.rtt_micros);
            if delta_micros > max_delta_micros {
                return Some(ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded {
                    previous_micros: previous.rtt_micros,
                    candidate_micros: candidate.rtt_micros,
                    delta_micros,
                    max_delta_micros,
                });
            }
        }

        if let Some(max_delta_micros) = policy.max_clock_offset_delta_micros {
            let delta_micros = previous
                .clock_offset_micros
                .abs_diff(candidate.clock_offset_micros);
            if delta_micros > max_delta_micros {
                return Some(
                    ServerHeartbeatRttOffsetOutlierReason::ClockOffsetDeltaExceeded {
                        previous_micros: previous.clock_offset_micros,
                        candidate_micros: candidate.clock_offset_micros,
                        delta_micros,
                        max_delta_micros,
                    },
                );
            }
        }

        None
    }
}

/// Reason for skipping latest estimate commit after candidate policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatRttOffsetCommitSkipReason {
    RejectedOutlier(ServerHeartbeatRttOffsetOutlierReason),
}

/// Final result after candidate policy decides whether commit may proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatRttOffsetPolicyCommitResult {
    Committed(ServerHeartbeatRttOffsetCommitOutcome),
    Skipped(ServerHeartbeatRttOffsetCommitSkipReason),
}

/// Combined policy-and-commit outcome for one RTT / offset candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetPolicyCommitOutcome {
    pub policy: ServerHeartbeatRttOffsetCandidatePolicyResult,
    pub result: ServerHeartbeatRttOffsetPolicyCommitResult,
}

impl ServerHeartbeatRttOffsetPolicyCommitOutcome {
    pub fn commit_outcome(&self) -> Option<&ServerHeartbeatRttOffsetCommitOutcome> {
        match &self.result {
            ServerHeartbeatRttOffsetPolicyCommitResult::Committed(outcome) => Some(outcome),
            ServerHeartbeatRttOffsetPolicyCommitResult::Skipped(_) => None,
        }
    }

    pub fn committed_samples(&self) -> Option<u64> {
        self.commit_outcome()
            .map(|outcome| outcome.committed.committed_samples)
    }
}

/// Boundary that commits stateless RTT / offset estimates into server state.
///
/// This boundary overwrites the per-client latest estimate and increments a
/// same-run sample count. It does not calculate estimates, smooth offset,
/// reject outliers, mutate liveness state, emit logs, or run timeout policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetCommitBoundary;

impl ServerHeartbeatRttOffsetCommitBoundary {
    pub fn commit(
        &self,
        state: &mut ServerHeartbeatRttOffsetState,
        input: ServerHeartbeatRttOffsetCommitInput,
    ) -> ServerHeartbeatRttOffsetCommitOutcome {
        let key = input.calculation.client_id.0.clone();
        let previous = state.entries_by_client_id.get(&key).cloned();
        let replaced_previous_run = previous
            .as_ref()
            .map(|entry| entry.run_id != input.calculation.run_id)
            .unwrap_or(false);
        let committed_samples = previous
            .as_ref()
            .filter(|entry| entry.run_id == input.calculation.run_id)
            .map(|entry| entry.committed_samples.saturating_add(1))
            .unwrap_or(1);
        let committed = ServerHeartbeatRttOffsetStateEntry {
            client_id: input.calculation.client_id,
            run_id: input.calculation.run_id,
            latest_estimate: input.calculation.estimate,
            committed_samples,
            last_committed_at: input.committed_at,
        };

        state.entries_by_client_id.insert(key, committed.clone());

        ServerHeartbeatRttOffsetCommitOutcome {
            previous,
            committed,
            replaced_previous_run,
        }
    }
}

/// Boundary that evaluates candidate policy before committing RTT / offset state.
///
/// Rejected candidates are not committed. This boundary still does not smooth
/// values, keep history, publish corrected timestamps, emit logs, or mutate
/// timeout state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetPolicyCommitBoundary {
    policy: ServerHeartbeatRttOffsetCandidatePolicyBoundary,
    commit: ServerHeartbeatRttOffsetCommitBoundary,
}

impl ServerHeartbeatRttOffsetPolicyCommitBoundary {
    pub fn evaluate_and_commit(
        &self,
        state: &mut ServerHeartbeatRttOffsetState,
        calculation: ServerHeartbeatRttOffsetCalculation,
        candidate_policy: ServerHeartbeatRttOffsetCandidatePolicy,
        committed_at: Option<TimestampMicros>,
    ) -> ServerHeartbeatRttOffsetPolicyCommitOutcome {
        let policy = self.policy.evaluate(state, &calculation, candidate_policy);
        let result = match policy.decision {
            ServerHeartbeatRttOffsetCandidatePolicyDecision::Accept { .. } => {
                ServerHeartbeatRttOffsetPolicyCommitResult::Committed(self.commit.commit(
                    state,
                    ServerHeartbeatRttOffsetCommitInput {
                        calculation,
                        committed_at,
                    },
                ))
            }
            ServerHeartbeatRttOffsetCandidatePolicyDecision::RejectOutlier { reason } => {
                ServerHeartbeatRttOffsetPolicyCommitResult::Skipped(
                    ServerHeartbeatRttOffsetCommitSkipReason::RejectedOutlier(reason),
                )
            }
        };

        ServerHeartbeatRttOffsetPolicyCommitOutcome { policy, result }
    }
}

/// Typed log handoff for a rejected RTT / offset candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateLogInput {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub candidate: HeartbeatRttOffsetEstimate,
    pub reason: ServerHeartbeatRttOffsetOutlierReason,
    pub state_commit_skipped: bool,
    pub observed_at: TimestampMicros,
}

/// Typed metrics handoff for future rejected RTT / offset counters.
///
/// This only names the counter deltas. It does not own metrics storage,
/// aggregation, export, or alerting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub reason: ServerHeartbeatRttOffsetOutlierReason,
    pub rejected_candidates_delta: u64,
    pub skipped_commits_delta: u64,
}

/// Aggregated rejected-candidate metrics for one client run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsStateEntry {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub total_rejected_candidates: u64,
    pub total_skipped_commits: u64,
    pub rtt_delta_rejections: u64,
    pub clock_offset_delta_rejections: u64,
    pub last_updated_at: Option<TimestampMicros>,
}

/// In-memory rejected-candidate metrics state keyed by `(client_id, run_id)`.
///
/// This aggregates only counters needed by the current RTT / offset policy
/// boundary. It does not expose dashboards, write logs, export over a socket,
/// persist metrics, or own loop cadence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsState {
    entries_by_client_run:
        BTreeMap<(String, String), ServerHeartbeatRttOffsetRejectedCandidateMetricsStateEntry>,
}

impl ServerHeartbeatRttOffsetRejectedCandidateMetricsState {
    pub fn entries(
        &self,
    ) -> impl Iterator<Item = &ServerHeartbeatRttOffsetRejectedCandidateMetricsStateEntry> {
        self.entries_by_client_run.values()
    }

    pub fn get(
        &self,
        client_id: &ClientId,
        run_id: &RunId,
    ) -> Option<&ServerHeartbeatRttOffsetRejectedCandidateMetricsStateEntry> {
        self.entries_by_client_run
            .get(&(client_id.0.clone(), run_id.0.clone()))
    }

    pub fn len(&self) -> usize {
        self.entries_by_client_run.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries_by_client_run.is_empty()
    }
}

/// Input for committing one rejected-candidate metrics handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitInput {
    pub handoff: ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff,
    pub updated_at: Option<TimestampMicros>,
}

/// Result of committing one rejected-candidate metrics handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitOutcome {
    pub previous: Option<ServerHeartbeatRttOffsetRejectedCandidateMetricsStateEntry>,
    pub committed: ServerHeartbeatRttOffsetRejectedCandidateMetricsStateEntry,
}

/// Boundary that aggregates rejected RTT / offset candidate metrics.
///
/// This boundary consumes the counter delta prepared by the rejected-candidate
/// handoff. It does not evaluate policy, mutate RTT / offset estimate state,
/// write logs, export metrics, or run a continuous loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitBoundary;

impl ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitBoundary {
    pub fn commit(
        &self,
        state: &mut ServerHeartbeatRttOffsetRejectedCandidateMetricsState,
        input: ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitInput,
    ) -> ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitOutcome {
        let key = (
            input.handoff.client_id.0.clone(),
            input.handoff.run_id.0.clone(),
        );
        let previous = state.entries_by_client_run.get(&key).cloned();
        let mut committed = previous.clone().unwrap_or(
            ServerHeartbeatRttOffsetRejectedCandidateMetricsStateEntry {
                client_id: input.handoff.client_id,
                run_id: input.handoff.run_id,
                total_rejected_candidates: 0,
                total_skipped_commits: 0,
                rtt_delta_rejections: 0,
                clock_offset_delta_rejections: 0,
                last_updated_at: None,
            },
        );

        committed.total_rejected_candidates = committed
            .total_rejected_candidates
            .saturating_add(input.handoff.rejected_candidates_delta);
        committed.total_skipped_commits = committed
            .total_skipped_commits
            .saturating_add(input.handoff.skipped_commits_delta);
        match input.handoff.reason {
            ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded { .. } => {
                committed.rtt_delta_rejections = committed
                    .rtt_delta_rejections
                    .saturating_add(input.handoff.rejected_candidates_delta);
            }
            ServerHeartbeatRttOffsetOutlierReason::ClockOffsetDeltaExceeded { .. } => {
                committed.clock_offset_delta_rejections = committed
                    .clock_offset_delta_rejections
                    .saturating_add(input.handoff.rejected_candidates_delta);
            }
        }
        committed.last_updated_at = input.updated_at;

        state.entries_by_client_run.insert(key, committed.clone());

        ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitOutcome {
            previous,
            committed,
        }
    }
}

/// Exportable rejected-candidate metrics record.
///
/// This is a typed snapshot shape only. A future exporter can map it to JSON,
/// a UI model, or another metrics backend without exposing the internal map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsExportRecord {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub total_rejected_candidates: u64,
    pub total_skipped_commits: u64,
    pub rtt_delta_rejections: u64,
    pub clock_offset_delta_rejections: u64,
    pub last_updated_at: Option<TimestampMicros>,
}

/// Snapshot produced for future metrics exporters or dashboards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsSnapshot {
    pub records: Vec<ServerHeartbeatRttOffsetRejectedCandidateMetricsExportRecord>,
}

/// Boundary that snapshots rejected-candidate metrics for future export.
///
/// This does not serialize, write files, send network traffic, retain history,
/// or render a dashboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateMetricsExportBoundary;

impl ServerHeartbeatRttOffsetRejectedCandidateMetricsExportBoundary {
    pub fn snapshot(
        &self,
        state: &ServerHeartbeatRttOffsetRejectedCandidateMetricsState,
    ) -> ServerHeartbeatRttOffsetRejectedCandidateMetricsSnapshot {
        let records = state
            .entries()
            .map(
                |entry| ServerHeartbeatRttOffsetRejectedCandidateMetricsExportRecord {
                    client_id: entry.client_id.clone(),
                    run_id: entry.run_id.clone(),
                    total_rejected_candidates: entry.total_rejected_candidates,
                    total_skipped_commits: entry.total_skipped_commits,
                    rtt_delta_rejections: entry.rtt_delta_rejections,
                    clock_offset_delta_rejections: entry.clock_offset_delta_rejections,
                    last_updated_at: entry.last_updated_at,
                },
            )
            .collect();

        ServerHeartbeatRttOffsetRejectedCandidateMetricsSnapshot { records }
    }
}

/// Consumer target for rejected-candidate metrics snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatRttOffsetMetricsSnapshotConsumer {
    FutureLoop,
    FutureDashboard,
}

/// Typed handoff from metrics state snapshot export to a future consumer.
///
/// This does not serialize the snapshot, send it over the network, render a UI,
/// or schedule a periodic export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetMetricsSnapshotExportHandoff {
    pub consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer,
    pub exported_at: Option<TimestampMicros>,
    pub snapshot: ServerHeartbeatRttOffsetRejectedCandidateMetricsSnapshot,
}

/// Runtime-shaped result for a single metrics snapshot export attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatRttOffsetMetricsSnapshotExportRuntimeResult {
    NoRecords {
        consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer,
        exported_at: Option<TimestampMicros>,
    },
    Handoff(ServerHeartbeatRttOffsetMetricsSnapshotExportHandoff),
}

/// Boundary that creates a metrics snapshot handoff for a future consumer.
///
/// A future loop may call this at a chosen cadence. This boundary only creates
/// one typed handoff from current in-memory state; it does not own cadence,
/// retention, dashboard state, JSON output, file sinks, or network export.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary {
    export: ServerHeartbeatRttOffsetRejectedCandidateMetricsExportBoundary,
}

impl ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary {
    pub fn export_for_consumer(
        &self,
        state: &ServerHeartbeatRttOffsetRejectedCandidateMetricsState,
        consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer,
        exported_at: Option<TimestampMicros>,
    ) -> ServerHeartbeatRttOffsetMetricsSnapshotExportRuntimeResult {
        let snapshot = self.export.snapshot(state);
        if snapshot.records.is_empty() {
            return ServerHeartbeatRttOffsetMetricsSnapshotExportRuntimeResult::NoRecords {
                consumer,
                exported_at,
            };
        }

        ServerHeartbeatRttOffsetMetricsSnapshotExportRuntimeResult::Handoff(
            ServerHeartbeatRttOffsetMetricsSnapshotExportHandoff {
                consumer,
                exported_at,
                snapshot,
            },
        )
    }
}

/// Placeholder input shape for a future dashboard consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetMetricsDashboardSnapshotInput {
    pub exported_at: Option<TimestampMicros>,
    pub records: Vec<ServerHeartbeatRttOffsetRejectedCandidateMetricsExportRecord>,
}

/// Placeholder output after routing a snapshot handoff to its selected consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatRttOffsetMetricsSnapshotConsumerResult {
    FutureLoop(ServerHeartbeatRttOffsetMetricsSnapshotExportHandoff),
    FutureDashboard(ServerHeartbeatRttOffsetMetricsDashboardSnapshotInput),
}

/// Boundary that names how a future consumer receives a metrics snapshot.
///
/// This only adapts the typed snapshot handoff to a future loop or dashboard
/// placeholder. It does not store dashboard state, render UI, notify another
/// thread, or export metrics externally.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetMetricsSnapshotConsumerBoundary;

impl ServerHeartbeatRttOffsetMetricsSnapshotConsumerBoundary {
    pub fn consume(
        &self,
        handoff: ServerHeartbeatRttOffsetMetricsSnapshotExportHandoff,
    ) -> ServerHeartbeatRttOffsetMetricsSnapshotConsumerResult {
        match handoff.consumer {
            ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureLoop => {
                ServerHeartbeatRttOffsetMetricsSnapshotConsumerResult::FutureLoop(handoff)
            }
            ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureDashboard => {
                ServerHeartbeatRttOffsetMetricsSnapshotConsumerResult::FutureDashboard(
                    ServerHeartbeatRttOffsetMetricsDashboardSnapshotInput {
                        exported_at: handoff.exported_at,
                        records: handoff.snapshot.records,
                    },
                )
            }
        }
    }
}

/// Cadence inputs for a future server-side continuous heartbeat loop.
///
/// This controls only when the future loop body should consider calling the
/// already-separated timeout tick and metrics snapshot handoff boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopCadenceInput {
    pub timeout_tick_interval_micros: u64,
    pub metrics_snapshot_interval_micros: Option<u64>,
}

/// Stop policy inputs for a future server-side continuous heartbeat loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatContinuousLoopStopCondition {
    RunUntilStopped,
    MaxTimeoutTicks { max_timeout_ticks: u64 },
}

/// Snapshot of server heartbeat loop state supplied by a future loop body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopStateSnapshot {
    pub completed_timeout_ticks: u64,
    pub exported_metrics_snapshots: u64,
    pub last_timeout_tick_at: Option<TimestampMicros>,
    pub last_metrics_snapshot_at: Option<TimestampMicros>,
    pub stop_requested: bool,
}

/// Input for deciding the next future server heartbeat loop action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopPolicyInput {
    pub now: TimestampMicros,
    pub cadence: ServerHeartbeatContinuousLoopCadenceInput,
    pub stop_condition: ServerHeartbeatContinuousLoopStopCondition,
    pub state: ServerHeartbeatContinuousLoopStateSnapshot,
}

/// Reason a future server heartbeat loop policy reached its decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatContinuousLoopPolicyReason {
    StopRequested,
    MaxTimeoutTicksReached,
    WaitingForCadence,
    TimeoutTickDue,
    MetricsSnapshotDue,
    TimeoutTickAndMetricsSnapshotDue,
}

/// Typed log handoff for a future server heartbeat loop decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopLogHandoff {
    pub observed_at: TimestampMicros,
    pub reason: ServerHeartbeatContinuousLoopPolicyReason,
    pub timeout_tick_interval_micros: u64,
    pub metrics_snapshot_interval_micros: Option<u64>,
    pub completed_timeout_ticks: u64,
    pub exported_metrics_snapshots: u64,
}

/// Next action selected for a future server heartbeat loop body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatContinuousLoopPolicyAction {
    Stop {
        reason: ServerHeartbeatContinuousLoopPolicyReason,
        log: ServerHeartbeatContinuousLoopLogHandoff,
    },
    Wait {
        next_wakeup_at: TimestampMicros,
        log: ServerHeartbeatContinuousLoopLogHandoff,
    },
    Run {
        run_timeout_tick: bool,
        export_metrics_snapshot: bool,
        log: ServerHeartbeatContinuousLoopLogHandoff,
    },
}

/// Policy boundary for a future server-side continuous heartbeat loop.
///
/// This boundary decides only whether the next server loop body should stop,
/// wait, run timeout tick work, and/or export a metrics snapshot. It does not
/// iterate clients, evaluate timeouts, store notices, wake send loops, write
/// logs, export metrics, sleep, or start a completed loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopPolicyBoundary;

impl ServerHeartbeatContinuousLoopPolicyBoundary {
    pub fn evaluate(
        &self,
        input: ServerHeartbeatContinuousLoopPolicyInput,
    ) -> ServerHeartbeatContinuousLoopPolicyAction {
        if input.state.stop_requested {
            return ServerHeartbeatContinuousLoopPolicyAction::Stop {
                reason: ServerHeartbeatContinuousLoopPolicyReason::StopRequested,
                log: server_heartbeat_continuous_loop_log(
                    input,
                    ServerHeartbeatContinuousLoopPolicyReason::StopRequested,
                ),
            };
        }

        match input.stop_condition {
            ServerHeartbeatContinuousLoopStopCondition::RunUntilStopped => {}
            ServerHeartbeatContinuousLoopStopCondition::MaxTimeoutTicks { max_timeout_ticks }
                if input.state.completed_timeout_ticks >= max_timeout_ticks =>
            {
                return ServerHeartbeatContinuousLoopPolicyAction::Stop {
                    reason: ServerHeartbeatContinuousLoopPolicyReason::MaxTimeoutTicksReached,
                    log: server_heartbeat_continuous_loop_log(
                        input,
                        ServerHeartbeatContinuousLoopPolicyReason::MaxTimeoutTicksReached,
                    ),
                };
            }
            ServerHeartbeatContinuousLoopStopCondition::MaxTimeoutTicks { .. } => {}
        }

        let next_timeout_tick_at = input
            .state
            .last_timeout_tick_at
            .map(|timestamp| {
                server_timestamp_saturating_add(
                    timestamp,
                    input.cadence.timeout_tick_interval_micros,
                )
            })
            .unwrap_or(input.now);
        let timeout_tick_due = input.now.0 >= next_timeout_tick_at.0;

        let next_metrics_snapshot_at =
            input
                .cadence
                .metrics_snapshot_interval_micros
                .map(|interval| {
                    input
                        .state
                        .last_metrics_snapshot_at
                        .map(|timestamp| server_timestamp_saturating_add(timestamp, interval))
                        .unwrap_or(input.now)
                });
        let metrics_snapshot_due = next_metrics_snapshot_at
            .map(|timestamp| input.now.0 >= timestamp.0)
            .unwrap_or(false);

        if !timeout_tick_due && !metrics_snapshot_due {
            let next_wakeup_at =
                server_min_timestamp(next_timeout_tick_at, next_metrics_snapshot_at);
            return ServerHeartbeatContinuousLoopPolicyAction::Wait {
                next_wakeup_at,
                log: server_heartbeat_continuous_loop_log(
                    input,
                    ServerHeartbeatContinuousLoopPolicyReason::WaitingForCadence,
                ),
            };
        }

        let reason = match (timeout_tick_due, metrics_snapshot_due) {
            (true, true) => {
                ServerHeartbeatContinuousLoopPolicyReason::TimeoutTickAndMetricsSnapshotDue
            }
            (true, false) => ServerHeartbeatContinuousLoopPolicyReason::TimeoutTickDue,
            (false, true) => ServerHeartbeatContinuousLoopPolicyReason::MetricsSnapshotDue,
            (false, false) => unreachable!("handled by wait branch"),
        };

        ServerHeartbeatContinuousLoopPolicyAction::Run {
            run_timeout_tick: timeout_tick_due,
            export_metrics_snapshot: metrics_snapshot_due,
            log: server_heartbeat_continuous_loop_log(input, reason),
        }
    }
}

/// Missing ownership prerequisite for a future server heartbeat loop body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatContinuousLoopOwnershipMissing {
    AuthenticatedSenderRegistry,
    LivenessState,
    OutboundQueue,
    TimeoutLogWriter,
    RejectedCandidateMetricsState,
}

/// Ownership inputs needed before entering a future server heartbeat loop body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopOwnershipInput {
    pub registry_available: bool,
    pub liveness_state_available: bool,
    pub outbound_queue_available: bool,
    pub timeout_log_writer_available: bool,
    pub rejected_candidate_metrics_state_available: bool,
}

/// State ownership plan for a future server heartbeat loop body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopOwnershipPlan {
    pub owns_registry: bool,
    pub owns_liveness_state: bool,
    pub owns_outbound_queue: bool,
    pub owns_timeout_log_writer: bool,
    pub owns_rejected_candidate_metrics_state: bool,
}

/// Readiness decision before a future server heartbeat loop owns state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatContinuousLoopOwnershipDecision {
    Ready(ServerHeartbeatContinuousLoopOwnershipPlan),
    NotReady {
        missing: Vec<ServerHeartbeatContinuousLoopOwnershipMissing>,
    },
}

/// Boundary that names server-side state ownership before entering a future loop.
///
/// This boundary only checks that the caller has prepared the state holders the
/// future loop body will own. It does not scan clients, evaluate timeouts,
/// mutate registry state, store notices, write logs, export metrics, or start
/// a loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopOwnershipBoundary;

impl ServerHeartbeatContinuousLoopOwnershipBoundary {
    pub fn evaluate(
        &self,
        input: ServerHeartbeatContinuousLoopOwnershipInput,
    ) -> ServerHeartbeatContinuousLoopOwnershipDecision {
        let mut missing = Vec::new();
        if !input.registry_available {
            missing
                .push(ServerHeartbeatContinuousLoopOwnershipMissing::AuthenticatedSenderRegistry);
        }
        if !input.liveness_state_available {
            missing.push(ServerHeartbeatContinuousLoopOwnershipMissing::LivenessState);
        }
        if !input.outbound_queue_available {
            missing.push(ServerHeartbeatContinuousLoopOwnershipMissing::OutboundQueue);
        }
        if !input.timeout_log_writer_available {
            missing.push(ServerHeartbeatContinuousLoopOwnershipMissing::TimeoutLogWriter);
        }
        if !input.rejected_candidate_metrics_state_available {
            missing
                .push(ServerHeartbeatContinuousLoopOwnershipMissing::RejectedCandidateMetricsState);
        }

        if !missing.is_empty() {
            return ServerHeartbeatContinuousLoopOwnershipDecision::NotReady { missing };
        }

        ServerHeartbeatContinuousLoopOwnershipDecision::Ready(
            ServerHeartbeatContinuousLoopOwnershipPlan {
                owns_registry: true,
                owns_liveness_state: true,
                owns_outbound_queue: true,
                owns_timeout_log_writer: true,
                owns_rejected_candidate_metrics_state: true,
            },
        )
    }
}

/// Input for deriving the socket receive timeout before heartbeat loop work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopSocketReceiveTimeoutInput {
    pub now: TimestampMicros,
    pub next_heartbeat_work_due_at: TimestampMicros,
    pub max_socket_receive_timeout_micros: u64,
}

/// Socket wait decision before a future server receive attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatContinuousLoopSocketReceiveTimeoutDecision {
    HeartbeatWorkDueNow,
    Wait {
        receive_timeout_micros: u64,
        heartbeat_work_due_at: TimestampMicros,
    },
}

/// Boundary that derives how long a server socket receive may block before
/// heartbeat work is due.
///
/// This does not call `set_read_timeout`, receive packets, run timeout ticks,
/// export metrics, or sleep.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopSocketReceiveTimeoutBoundary;

impl ServerHeartbeatContinuousLoopSocketReceiveTimeoutBoundary {
    pub fn plan_wait(
        &self,
        input: ServerHeartbeatContinuousLoopSocketReceiveTimeoutInput,
    ) -> ServerHeartbeatContinuousLoopSocketReceiveTimeoutDecision {
        if input.now.0 >= input.next_heartbeat_work_due_at.0 {
            return ServerHeartbeatContinuousLoopSocketReceiveTimeoutDecision::HeartbeatWorkDueNow;
        }

        let remaining_micros = input.next_heartbeat_work_due_at.0 - input.now.0;
        ServerHeartbeatContinuousLoopSocketReceiveTimeoutDecision::Wait {
            receive_timeout_micros: remaining_micros.min(input.max_socket_receive_timeout_micros),
            heartbeat_work_due_at: input.next_heartbeat_work_due_at,
        }
    }
}

/// Retry reason placeholder for future server heartbeat loop work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatContinuousLoopRetryReason {
    SocketReceiveInterrupted,
    TimeoutTickApplyFailed,
    NoticeQueueStorageFailed,
    MetricsSnapshotHandoffFailed,
}

/// Retry policy placeholder for future server heartbeat loop work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopRetryPolicy {
    pub max_attempts: u32,
    pub retry_delay_micros: u64,
}

/// Input for one retry decision in a future server heartbeat loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopRetryInput {
    pub reason: ServerHeartbeatContinuousLoopRetryReason,
    pub attempts_used: u32,
    pub policy: ServerHeartbeatContinuousLoopRetryPolicy,
    pub now: TimestampMicros,
}

/// Retry decision placeholder for future server heartbeat loop work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerHeartbeatContinuousLoopRetryDecision {
    RetryLater {
        reason: ServerHeartbeatContinuousLoopRetryReason,
        next_attempt: u32,
        retry_at: TimestampMicros,
    },
    GiveUp {
        reason: ServerHeartbeatContinuousLoopRetryReason,
        attempts_used: u32,
    },
}

/// Boundary that classifies retry timing without executing retry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopRetryBoundary;

impl ServerHeartbeatContinuousLoopRetryBoundary {
    pub fn decide(
        &self,
        input: ServerHeartbeatContinuousLoopRetryInput,
    ) -> ServerHeartbeatContinuousLoopRetryDecision {
        if input.attempts_used >= input.policy.max_attempts {
            return ServerHeartbeatContinuousLoopRetryDecision::GiveUp {
                reason: input.reason,
                attempts_used: input.attempts_used,
            };
        }

        ServerHeartbeatContinuousLoopRetryDecision::RetryLater {
            reason: input.reason,
            next_attempt: input.attempts_used.saturating_add(1),
            retry_at: server_timestamp_saturating_add(input.now, input.policy.retry_delay_micros),
        }
    }
}

/// Input for one future server heartbeat loop body iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopBodyInput {
    pub ownership: ServerHeartbeatContinuousLoopOwnershipInput,
    pub policy: ServerHeartbeatContinuousLoopPolicyInput,
    pub max_socket_receive_timeout_micros: u64,
    pub metrics_snapshot_consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer,
}

/// Handoff telling a future body to run timeout tick work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopTimeoutTickHandoff {
    pub evaluated_at: TimestampMicros,
}

/// Handoff telling a future body to export rejected-candidate metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopMetricsSnapshotHandoff {
    pub exported_at: TimestampMicros,
    pub consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer,
}

/// Work handoffs emitted for one future server heartbeat loop body iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopBodyHandoff {
    pub timeout_tick: Option<ServerHeartbeatContinuousLoopTimeoutTickHandoff>,
    pub metrics_snapshot: Option<ServerHeartbeatContinuousLoopMetricsSnapshotHandoff>,
}

/// Runtime-shaped result for one future server heartbeat loop body iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatContinuousLoopBodyResult {
    OwnershipNotReady(ServerHeartbeatContinuousLoopOwnershipDecision),
    Stop {
        reason: ServerHeartbeatContinuousLoopPolicyReason,
        log: ServerHeartbeatContinuousLoopLogHandoff,
    },
    Wait {
        socket_wait: ServerHeartbeatContinuousLoopSocketReceiveTimeoutDecision,
        log: ServerHeartbeatContinuousLoopLogHandoff,
    },
    Run {
        handoff: ServerHeartbeatContinuousLoopBodyHandoff,
        log: ServerHeartbeatContinuousLoopLogHandoff,
    },
}

/// Boundary for one future server heartbeat loop body iteration.
///
/// This composes state ownership, cadence policy, and socket wait planning for
/// one iteration. It only emits handoffs for timeout tick and metrics snapshot
/// work; it does not scan clients, mutate state, write logs, receive packets,
/// store notices, export metrics, sleep, or retry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatContinuousLoopBodyBoundary {
    ownership: ServerHeartbeatContinuousLoopOwnershipBoundary,
    policy: ServerHeartbeatContinuousLoopPolicyBoundary,
    socket_wait: ServerHeartbeatContinuousLoopSocketReceiveTimeoutBoundary,
}

impl ServerHeartbeatContinuousLoopBodyBoundary {
    pub fn run_one(
        &self,
        input: ServerHeartbeatContinuousLoopBodyInput,
    ) -> ServerHeartbeatContinuousLoopBodyResult {
        let ownership = self.ownership.evaluate(input.ownership);
        if !matches!(
            ownership,
            ServerHeartbeatContinuousLoopOwnershipDecision::Ready(_)
        ) {
            return ServerHeartbeatContinuousLoopBodyResult::OwnershipNotReady(ownership);
        }

        match self.policy.evaluate(input.policy) {
            ServerHeartbeatContinuousLoopPolicyAction::Stop { reason, log } => {
                ServerHeartbeatContinuousLoopBodyResult::Stop { reason, log }
            }
            ServerHeartbeatContinuousLoopPolicyAction::Wait {
                next_wakeup_at,
                log,
            } => {
                let socket_wait = self.socket_wait.plan_wait(
                    ServerHeartbeatContinuousLoopSocketReceiveTimeoutInput {
                        now: input.policy.now,
                        next_heartbeat_work_due_at: next_wakeup_at,
                        max_socket_receive_timeout_micros: input.max_socket_receive_timeout_micros,
                    },
                );

                ServerHeartbeatContinuousLoopBodyResult::Wait { socket_wait, log }
            }
            ServerHeartbeatContinuousLoopPolicyAction::Run {
                run_timeout_tick,
                export_metrics_snapshot,
                log,
            } => ServerHeartbeatContinuousLoopBodyResult::Run {
                handoff: ServerHeartbeatContinuousLoopBodyHandoff {
                    timeout_tick: run_timeout_tick.then_some(
                        ServerHeartbeatContinuousLoopTimeoutTickHandoff {
                            evaluated_at: input.policy.now,
                        },
                    ),
                    metrics_snapshot: export_metrics_snapshot.then_some(
                        ServerHeartbeatContinuousLoopMetricsSnapshotHandoff {
                            exported_at: input.policy.now,
                            consumer: input.metrics_snapshot_consumer,
                        },
                    ),
                },
                log,
            },
        }
    }
}

/// Combined handoff emitted after policy commit skips a rejected candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateHandoff {
    pub log: ServerHeartbeatRttOffsetRejectedCandidateLogInput,
    pub metrics: ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff,
}

/// Result of preparing rejected-candidate side-effect handoffs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatRttOffsetRejectedCandidateHandoffResult {
    Prepared(ServerHeartbeatRttOffsetRejectedCandidateHandoff),
    NotRejected,
}

/// Boundary that prepares log / metrics handoffs for rejected candidates.
///
/// This boundary is intended to be called after
/// `ServerHeartbeatRttOffsetPolicyCommitBoundary`. It does not evaluate
/// policy, commit state, write logs, update metrics, smooth values, or run a
/// continuous heartbeat loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateHandoffBoundary;

impl ServerHeartbeatRttOffsetRejectedCandidateHandoffBoundary {
    pub fn prepare(
        &self,
        outcome: &ServerHeartbeatRttOffsetPolicyCommitOutcome,
        observed_at: TimestampMicros,
    ) -> ServerHeartbeatRttOffsetRejectedCandidateHandoffResult {
        let reason = match &outcome.result {
            ServerHeartbeatRttOffsetPolicyCommitResult::Skipped(
                ServerHeartbeatRttOffsetCommitSkipReason::RejectedOutlier(reason),
            ) => *reason,
            ServerHeartbeatRttOffsetPolicyCommitResult::Committed(_) => {
                return ServerHeartbeatRttOffsetRejectedCandidateHandoffResult::NotRejected;
            }
        };

        let log = ServerHeartbeatRttOffsetRejectedCandidateLogInput {
            client_id: outcome.policy.client_id.clone(),
            run_id: outcome.policy.run_id.clone(),
            candidate: outcome.policy.candidate,
            reason,
            state_commit_skipped: true,
            observed_at,
        };
        let metrics = ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff {
            client_id: outcome.policy.client_id.clone(),
            run_id: outcome.policy.run_id.clone(),
            reason,
            rejected_candidates_delta: 1,
            skipped_commits_delta: 1,
        };

        ServerHeartbeatRttOffsetRejectedCandidateHandoffResult::Prepared(
            ServerHeartbeatRttOffsetRejectedCandidateHandoff { log, metrics },
        )
    }
}

pub const SERVER_HEARTBEAT_RTT_OFFSET_REJECTED_CANDIDATE_JSON_LOG_EVENT_NAME: &str =
    "server.heartbeat_rtt_offset_rejected_candidate";

/// JSON Lines event input for rejected RTT / offset candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventInput {
    pub event_name: &'static str,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub candidate: HeartbeatRttOffsetEstimate,
    pub reason: ServerHeartbeatRttOffsetOutlierReason,
    pub state_commit_skipped: bool,
    pub observed_at: TimestampMicros,
}

/// Boundary that maps rejected-candidate log handoff input to event fields.
///
/// This does not write JSON Lines, update metrics, or mutate RTT / offset
/// state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventBoundary;

impl ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventBoundary {
    pub fn build_event(
        &self,
        input: ServerHeartbeatRttOffsetRejectedCandidateLogInput,
    ) -> ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventInput {
        ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventInput {
            event_name: SERVER_HEARTBEAT_RTT_OFFSET_REJECTED_CANDIDATE_JSON_LOG_EVENT_NAME,
            client_id: input.client_id,
            run_id: input.run_id,
            candidate: input.candidate,
            reason: input.reason,
            state_commit_skipped: input.state_commit_skipped,
            observed_at: input.observed_at,
        }
    }
}

/// Minimal rejected-candidate log output boundary.
///
/// This writes one JSON Lines record to a caller-owned writer. It does not open
/// files, install a process-wide logger, update metrics, or run a continuous
/// heartbeat loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateLogOutputBoundary {
    event: ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventBoundary,
    writer: ServerHeartbeatRttOffsetRejectedCandidateJsonLineWriter,
}

impl ServerHeartbeatRttOffsetRejectedCandidateLogOutputBoundary {
    pub fn write_rejected_candidate<W: io::Write>(
        &self,
        input: ServerHeartbeatRttOffsetRejectedCandidateLogInput,
        writer: W,
    ) -> io::Result<ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventInput> {
        let event = self.event.build_event(input);
        self.writer.write_event(&event, writer)?;
        Ok(event)
    }
}

/// Minimal JSON Lines writer for rejected RTT / offset candidate events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetRejectedCandidateJsonLineWriter;

impl ServerHeartbeatRttOffsetRejectedCandidateJsonLineWriter {
    pub fn write_event<W: io::Write>(
        &self,
        event: &ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventInput,
        mut writer: W,
    ) -> io::Result<()> {
        let reason = rejected_candidate_reason_fields(event.reason);

        write!(writer, "{{")?;
        write_json_field(&mut writer, "event_name", event.event_name)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "client_id", &event.client_id.0)?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "run_id", &event.run_id.0)?;
        write!(
            writer,
            ",\"candidate_rtt_micros\":{}",
            event.candidate.rtt_micros
        )?;
        write!(
            writer,
            ",\"candidate_server_processing_micros\":{}",
            event.candidate.server_processing_micros
        )?;
        write!(
            writer,
            ",\"candidate_clock_offset_micros\":{}",
            event.candidate.clock_offset_micros
        )?;
        write!(writer, ",")?;
        write_json_field(&mut writer, "reject_reason", reason.name)?;
        write!(
            writer,
            ",\"reason_previous_micros\":{}",
            reason.previous_micros
        )?;
        write!(
            writer,
            ",\"reason_candidate_micros\":{}",
            reason.candidate_micros
        )?;
        write!(writer, ",\"reason_delta_micros\":{}", reason.delta_micros)?;
        write!(
            writer,
            ",\"reason_max_delta_micros\":{}",
            reason.max_delta_micros
        )?;
        write!(
            writer,
            ",\"state_commit_skipped\":{}",
            event.state_commit_skipped
        )?;
        write!(writer, ",\"observed_at\":{}", event.observed_at.0)?;
        writeln!(writer, "}}")
    }
}

/// Server-side validation errors before or during one timebase calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatRttOffsetCalculationError {
    ClientIdMismatch,
    RunIdMismatch,
    EchoedSentAtMismatch {
        expected: TimestampMicros,
        actual: TimestampMicros,
    },
    ServerReceivedAtMismatch {
        expected: TimestampMicros,
        actual: TimestampMicros,
    },
    ServerSentAtMismatch {
        expected: TimestampMicros,
        actual: TimestampMicros,
    },
    Calculation(HeartbeatRttOffsetCalculationError),
}

impl From<HeartbeatRttOffsetCalculationError> for ServerHeartbeatRttOffsetCalculationError {
    fn from(error: HeartbeatRttOffsetCalculationError) -> Self {
        Self::Calculation(error)
    }
}

/// Boundary for the smallest stateless RTT / offset calculation unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatRttOffsetCalculationBoundary {
    calculator: HeartbeatRttOffsetCalculator,
}

impl ServerHeartbeatRttOffsetCalculationBoundary {
    pub fn calculate(
        &self,
        plan: &ServerHeartbeatTimebasePlan,
        observation: &ServerHeartbeatClientAckObservation,
    ) -> Result<ServerHeartbeatRttOffsetCalculation, ServerHeartbeatRttOffsetCalculationError> {
        if plan.client_id != observation.client_id {
            return Err(ServerHeartbeatRttOffsetCalculationError::ClientIdMismatch);
        }
        if plan.run_id != observation.run_id {
            return Err(ServerHeartbeatRttOffsetCalculationError::RunIdMismatch);
        }

        let sample = plan.estimate.sample;
        let expected = TimestampMicros(sample.client_sent_at_micros);
        if expected != observation.echoed_client_sent_at {
            return Err(
                ServerHeartbeatRttOffsetCalculationError::EchoedSentAtMismatch {
                    expected,
                    actual: observation.echoed_client_sent_at,
                },
            );
        }
        let expected_server_received_at = TimestampMicros(sample.server_received_at_micros);
        if expected_server_received_at != observation.server_received_at {
            return Err(
                ServerHeartbeatRttOffsetCalculationError::ServerReceivedAtMismatch {
                    expected: expected_server_received_at,
                    actual: observation.server_received_at,
                },
            );
        }
        let expected_server_sent_at = TimestampMicros(sample.server_sent_at_micros);
        if expected_server_sent_at != observation.server_sent_at {
            return Err(
                ServerHeartbeatRttOffsetCalculationError::ServerSentAtMismatch {
                    expected: expected_server_sent_at,
                    actual: observation.server_sent_at,
                },
            );
        }

        let estimate = self.calculator.calculate(HeartbeatExchangeObservation {
            client_sent_at_micros: sample.client_sent_at_micros,
            server_received_at_micros: sample.server_received_at_micros,
            server_sent_at_micros: sample.server_sent_at_micros,
            client_received_at_micros: observation.client_received_at.0,
        })?;

        Ok(ServerHeartbeatRttOffsetCalculation {
            client_id: plan.client_id.clone(),
            run_id: plan.run_id.clone(),
            estimate,
        })
    }
}

/// Error while connecting a returned heartbeat observation to the stored plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHeartbeatObservationReturnError {
    MissingHeartbeatObservation,
    Calculation(ServerHeartbeatRttOffsetCalculationError),
}

/// Minimal bridge from a heartbeat ack handoff and returned ClientStats observation.
///
/// This uses the timebase plan created when the server handled the heartbeat,
/// and the observation extracted from the later ClientStats packet. It does not
/// store the estimate, smooth offset, update liveness state, or emit metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatObservationReturnBoundary {
    calculator: ServerHeartbeatRttOffsetCalculationBoundary,
}

impl ServerHeartbeatObservationReturnBoundary {
    pub fn calculate_from_client_stats(
        &self,
        heartbeat_handoff: &ServerHeartbeatAckHandoff,
        client_stats: &ServerClientStatsHandlerInput,
    ) -> Result<ServerHeartbeatRttOffsetCalculation, ServerHeartbeatObservationReturnError> {
        let observation = client_stats
            .heartbeat_observation
            .as_ref()
            .ok_or(ServerHeartbeatObservationReturnError::MissingHeartbeatObservation)?;

        self.calculator
            .calculate(
                &heartbeat_handoff.processing_inputs.timebase_plan,
                observation,
            )
            .map_err(ServerHeartbeatObservationReturnError::Calculation)
    }
}

/// Combined heartbeat inputs for state and timebase layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatProcessingInputs {
    pub state: ServerHeartbeatStateInput,
    pub timebase: ServerHeartbeatTimebaseInput,
    pub timebase_plan: ServerHeartbeatTimebasePlan,
    pub ack_timing: ServerHeartbeatAckTiming,
}

/// Boundary that prepares heartbeat state/timebase inputs.
///
/// This consumes a registered heartbeat packet and explicit server timing. It
/// does not mutate heartbeat state, calculate RTT / offset, or build ack
/// messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerHeartbeatInputBoundary;

impl ServerHeartbeatInputBoundary {
    pub fn prepare_inputs(
        &self,
        packet: &ServerRegisteredHeartbeatPacket,
        timing: ServerHeartbeatAckTiming,
    ) -> ServerHeartbeatProcessingInputs {
        let timebase = ServerHeartbeatTimebaseInput {
            client_id: packet.heartbeat.client_id.clone(),
            run_id: packet.heartbeat.run_id.clone(),
            client_sent_at: packet.heartbeat.sent_at,
            client_local_time: packet.heartbeat.local_time,
            server_received_at: timing.server_received_at,
            server_sent_at: timing.server_sent_at,
        };
        let timebase_plan = ServerHeartbeatTimebasePlanBoundary::default().build_plan(&timebase);

        ServerHeartbeatProcessingInputs {
            state: ServerHeartbeatStateInput {
                source: packet.source,
                authenticated_sender: packet.authenticated_sender.clone(),
                client_id: packet.heartbeat.client_id.clone(),
                run_id: packet.heartbeat.run_id.clone(),
                protocol_version: packet.heartbeat.protocol_version,
                heartbeat_sent_at: packet.heartbeat.sent_at,
                server_received_at: timing.server_received_at,
                short_status: packet.heartbeat.short_status.clone(),
            },
            timebase,
            timebase_plan,
            ack_timing: timing,
        }
    }
}

/// Result of connecting a registered heartbeat packet to ack queue handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHeartbeatAckHandoff {
    pub registered_packet: ServerRegisteredHeartbeatPacket,
    pub processing_inputs: ServerHeartbeatProcessingInputs,
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
    input: ServerHeartbeatInputBoundary,
    ack: ServerHeartbeatAckBoundary,
    queue: ServerOutboundQueueBoundary,
}

impl ServerHeartbeatHandlerBoundary {
    pub fn handoff_ack(
        &self,
        packet: ServerRegisteredHeartbeatPacket,
        timing: ServerHeartbeatAckTiming,
    ) -> ServerHeartbeatAckHandoff {
        let processing_inputs = self.input.prepare_inputs(&packet, timing);
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
            processing_inputs,
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
    admission: OutboundQueueAdmissionPolicyBoundary,
    storage: OutboundQueueStorageBoundary,
    capacity_policy: OutboundQueueCapacityPolicy,
}

impl ServerOutboundQueueBoundary {
    pub fn handoff_auth_response(&self, response: ServerOutboundAuthResponse) -> OutboundQueueItem {
        self.queue.handoff(response.into_outbound_packet())
    }

    pub fn handoff_heartbeat_ack(&self, ack: ServerOutboundHeartbeatAck) -> OutboundQueueItem {
        self.queue.handoff(ack.into_outbound_packet())
    }

    pub fn handoff_notice(&self, notice: ServerOutboundNotice) -> OutboundQueueItem {
        self.queue.handoff(notice.into_outbound_packet())
    }

    pub fn evaluate_admission(
        &self,
        current_len: usize,
        item: &OutboundQueueItem,
    ) -> OutboundQueueAdmissionDecision {
        self.admission
            .evaluate(self.capacity_policy, current_len, item)
    }

    pub fn evaluate_storage_push(
        &self,
        current_len: usize,
        item: &OutboundQueueItem,
    ) -> OutboundQueueStorageDecision {
        self.storage
            .evaluate_push(self.capacity_policy, current_len, item)
    }

    pub const fn capacity_policy(&self) -> OutboundQueueCapacityPolicy {
        self.capacity_policy
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

struct RejectedCandidateReasonJsonFields {
    name: &'static str,
    previous_micros: i128,
    candidate_micros: i128,
    delta_micros: u64,
    max_delta_micros: u64,
}

fn rejected_candidate_reason_fields(
    reason: ServerHeartbeatRttOffsetOutlierReason,
) -> RejectedCandidateReasonJsonFields {
    match reason {
        ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded {
            previous_micros,
            candidate_micros,
            delta_micros,
            max_delta_micros,
        } => RejectedCandidateReasonJsonFields {
            name: "RttDeltaExceeded",
            previous_micros: i128::from(previous_micros),
            candidate_micros: i128::from(candidate_micros),
            delta_micros,
            max_delta_micros,
        },
        ServerHeartbeatRttOffsetOutlierReason::ClockOffsetDeltaExceeded {
            previous_micros,
            candidate_micros,
            delta_micros,
            max_delta_micros,
        } => RejectedCandidateReasonJsonFields {
            name: "ClockOffsetDeltaExceeded",
            previous_micros: i128::from(previous_micros),
            candidate_micros: i128::from(candidate_micros),
            delta_micros,
            max_delta_micros,
        },
    }
}

fn message_type(message: &ProtocolMessage) -> MessageType {
    match message {
        ProtocolMessage::AuthRequest(_) => MessageType::AuthRequest,
        ProtocolMessage::AuthResponse(_) => MessageType::AuthResponse,
        ProtocolMessage::Heartbeat(_) => MessageType::Heartbeat,
        ProtocolMessage::HeartbeatAck(_) => MessageType::HeartbeatAck,
        ProtocolMessage::VideoFrame(_) => MessageType::VideoFrame,
        ProtocolMessage::VideoFrameFragment(_) => MessageType::VideoFrameFragment,
        ProtocolMessage::ClientStats(_) => MessageType::ClientStats,
        ProtocolMessage::ServerNotice(_) => MessageType::ServerNotice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{SocketAddr, UdpSocket};
    use stream_sync_config::{AllowedClientConfig, SharedTokenConfig};
    use stream_sync_logging::JsonLinesSinkDestination;
    use stream_sync_net_core::{
        OutboundQueueDropReason, OutboundQueueItemClass, OutboundQueueStorageState,
        OutboundSendLoopTickState,
    };
    use stream_sync_protocol::{
        decode_fixed_header, decode_heartbeat_ack_payload, encode_auth_response_payload,
        AppVersion, ClientId, Codec, MessageEncoder, ProtocolVersion, RunId, TimestampMicros,
        FIXED_HEADER_LEN, HEADER_FLAGS_OFFSET, HEADER_LENGTH_OFFSET, HEADER_MESSAGE_TYPE_OFFSET,
        HEADER_PAYLOAD_LENGTH_OFFSET, HEADER_PROTOCOL_VERSION_OFFSET, HEADER_RESERVED_OFFSET,
    };

    #[test]
    fn server_auth_receive_json_lines_sink_defaults_to_stderr() {
        let boundary = ServerAuthReceiveJsonLinesSinkBoundary::default();

        let plan = boundary.plan(ServerAuthReceiveJsonLinesSinkConfig::stderr_default());

        assert_eq!(
            plan.auth_result.destination,
            JsonLinesSinkDestination::Stderr
        );
        assert_eq!(
            plan.receive_rejection.destination,
            JsonLinesSinkDestination::Stderr
        );
        assert!(!plan.auth_result.is_file_sink());
        assert!(!plan.receive_rejection.is_file_sink());
    }

    #[test]
    fn server_auth_receive_json_lines_sink_accepts_separate_file_paths() {
        let boundary = ServerAuthReceiveJsonLinesSinkBoundary::default();

        let plan = boundary.plan(ServerAuthReceiveJsonLinesSinkConfig::file_sinks(
            "logs/auth.jsonl",
            "logs/receive-rejection.jsonl",
        ));

        let JsonLinesSinkDestination::File(auth_file) = plan.auth_result.destination else {
            panic!("expected auth file sink");
        };
        let JsonLinesSinkDestination::File(receive_file) = plan.receive_rejection.destination
        else {
            panic!("expected receive rejection file sink");
        };
        assert_eq!(auth_file.path, PathBuf::from("logs/auth.jsonl"));
        assert_eq!(
            receive_file.path,
            PathBuf::from("logs/receive-rejection.jsonl")
        );
    }

    #[test]
    fn server_send_error_json_lines_sink_defaults_to_stderr() {
        let boundary = ServerSendErrorJsonLinesSinkBoundary::default();

        let plan = boundary.plan(ServerSendErrorJsonLinesSinkConfig::stderr_default());

        assert_eq!(
            plan.send_error.destination,
            JsonLinesSinkDestination::Stderr
        );
        assert!(!plan.send_error.is_file_sink());
    }

    #[test]
    fn server_send_error_json_lines_sink_accepts_file_path_without_opening_it() {
        let boundary = ServerSendErrorJsonLinesSinkBoundary::default();

        let plan = boundary.plan(ServerSendErrorJsonLinesSinkConfig::file_sink(
            "logs/send-error.jsonl",
        ));

        let JsonLinesSinkDestination::File(send_error_file) = plan.send_error.destination else {
            panic!("expected send error file sink");
        };
        assert_eq!(send_error_file.path, PathBuf::from("logs/send-error.jsonl"));
    }

    #[test]
    fn server_receive_loop_json_lines_sink_defaults_to_stderr() {
        let boundary = ServerReceiveLoopJsonLinesSinkBoundary::default();

        let plan = boundary.plan(ServerReceiveLoopJsonLinesSinkConfig::stderr_default());

        assert_eq!(
            plan.receive_loop.destination,
            JsonLinesSinkDestination::Stderr
        );
        assert!(!plan.receive_loop.is_file_sink());
    }

    #[test]
    fn server_receive_loop_json_lines_sink_accepts_file_path_without_opening_it() {
        let boundary = ServerReceiveLoopJsonLinesSinkBoundary::default();

        let plan = boundary.plan(ServerReceiveLoopJsonLinesSinkConfig::file_sink(
            "logs/receive-loop.jsonl",
        ));

        let JsonLinesSinkDestination::File(receive_loop_file) = plan.receive_loop.destination
        else {
            panic!("expected receive loop file sink");
        };
        assert_eq!(
            receive_loop_file.path,
            PathBuf::from("logs/receive-loop.jsonl")
        );
    }

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
    fn receive_send_one_iteration_launcher_loads_startup_config_from_example() {
        let launcher = ServerReceiveSendOneIterationLauncher::default();
        let startup_config = launcher
            .load_startup_config_from_str(include_str!(
                "../../../configs/examples/server.example.toml"
            ))
            .expect("example config should load for receive/send one iteration");

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
    fn continuous_receive_loop_lifecycle_stops_when_requested() {
        let lifecycle = ServerContinuousReceiveLoopLifecycleBoundary;

        let plan = lifecycle.plan_next(ServerContinuousReceiveLoopLifecycleInput {
            stop_requested: true,
        });

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopLifecyclePlan {
                state: ServerContinuousReceiveLoopLifecycleState::Stopped,
                action: ServerContinuousReceiveLoopAction::Stop,
                operational_log_required: false,
                rejection_log_required: false,
                handler_handoff_required: false,
            }
        );
    }

    #[test]
    fn continuous_receive_loop_lifecycle_waits_for_one_datagram() {
        let lifecycle = ServerContinuousReceiveLoopLifecycleBoundary;

        let plan = lifecycle.plan_next(ServerContinuousReceiveLoopLifecycleInput {
            stop_requested: false,
        });

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopLifecyclePlan {
                state: ServerContinuousReceiveLoopLifecycleState::AwaitingSocketReceive,
                action: ServerContinuousReceiveLoopAction::ReceiveOneDatagram,
                operational_log_required: false,
                rejection_log_required: false,
                handler_handoff_required: false,
            }
        );
    }

    #[test]
    fn continuous_receive_loop_lifecycle_dispatches_accepted_route() {
        let source = packet_source();
        let lifecycle = ServerContinuousReceiveLoopLifecycleBoundary;
        let outcome = ServerReceiveLoopGateOutcome::Accepted(heartbeat_route("client-1", source));

        let plan = lifecycle.plan_after_gate_outcome(&outcome);

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopLifecyclePlan {
                state: ServerContinuousReceiveLoopLifecycleState::DispatchingAcceptedRoute,
                action: ServerContinuousReceiveLoopAction::DispatchAcceptedRoute,
                operational_log_required: true,
                rejection_log_required: false,
                handler_handoff_required: true,
            }
        );
    }

    #[test]
    fn continuous_receive_loop_lifecycle_prepares_rejection_logs() {
        let source = packet_source();
        let lifecycle = ServerContinuousReceiveLoopLifecycleBoundary;
        let outcome = ServerReceiveLoopGateOutcome::Rejected(
            ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
                source,
                message_type: MessageType::Heartbeat,
                client_id: Some(ClientId("client-2".to_string())),
                reason: PacketAcceptanceRejectReason::UnknownClient,
            }),
        );

        let plan = lifecycle.plan_after_gate_outcome(&outcome);

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopLifecyclePlan {
                state: ServerContinuousReceiveLoopLifecycleState::PreparingRejectionLogs,
                action: ServerContinuousReceiveLoopAction::PrepareRejectionLogs,
                operational_log_required: true,
                rejection_log_required: true,
                handler_handoff_required: false,
            }
        );
    }

    #[test]
    fn continuous_receive_loop_tick_plans_socket_receive() {
        let tick = ServerContinuousReceiveLoopTickBoundary::default();

        let plan = tick.plan_next(ServerContinuousReceiveLoopLifecycleInput {
            stop_requested: false,
        });

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopTickPlan {
                state: ServerContinuousReceiveLoopTickState::AwaitingSocketReceive,
                lifecycle: ServerContinuousReceiveLoopLifecyclePlan {
                    state: ServerContinuousReceiveLoopLifecycleState::AwaitingSocketReceive,
                    action: ServerContinuousReceiveLoopAction::ReceiveOneDatagram,
                    operational_log_required: false,
                    rejection_log_required: false,
                    handler_handoff_required: false,
                },
                packet_len: None,
            }
        );
    }

    #[test]
    fn continuous_receive_loop_tick_observes_received_packet_for_decode() {
        let tick = ServerContinuousReceiveLoopTickBoundary::default();

        let plan = tick.observe_received_packet(128);

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopTickPlan {
                state: ServerContinuousReceiveLoopTickState::ReceivedPacketReadyForDecode,
                lifecycle: ServerContinuousReceiveLoopLifecyclePlan {
                    state: ServerContinuousReceiveLoopLifecycleState::ProcessingReceivedPacket,
                    action: ServerContinuousReceiveLoopAction::DecodeAndGateOnePacket,
                    operational_log_required: false,
                    rejection_log_required: false,
                    handler_handoff_required: false,
                },
                packet_len: Some(128),
            }
        );
    }

    #[test]
    fn continuous_receive_loop_tick_connects_accepted_outcome_to_handler_handoff() {
        let source = packet_source();
        let tick = ServerContinuousReceiveLoopTickBoundary::default();
        let outcome = ServerReceiveLoopGateOutcome::Accepted(heartbeat_route("client-1", source));

        let plan = tick.observe_gate_outcome(144, &outcome);

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopTickPlan {
                state: ServerContinuousReceiveLoopTickState::AcceptedRouteReadyForHandoff,
                lifecycle: ServerContinuousReceiveLoopLifecyclePlan {
                    state: ServerContinuousReceiveLoopLifecycleState::DispatchingAcceptedRoute,
                    action: ServerContinuousReceiveLoopAction::DispatchAcceptedRoute,
                    operational_log_required: true,
                    rejection_log_required: false,
                    handler_handoff_required: true,
                },
                packet_len: Some(144),
            }
        );
    }

    #[test]
    fn continuous_receive_loop_tick_connects_rejection_outcome_to_logs() {
        let source = packet_source();
        let tick = ServerContinuousReceiveLoopTickBoundary::default();
        let outcome = ServerReceiveLoopGateOutcome::Rejected(
            ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
                source,
                message_type: MessageType::VideoFrame,
                client_id: Some(ClientId("client-1".to_string())),
                reason: PacketAcceptanceRejectReason::EndpointMismatch,
            }),
        );

        let plan = tick.observe_gate_outcome(256, &outcome);

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopTickPlan {
                state: ServerContinuousReceiveLoopTickState::RejectionReadyForLogs,
                lifecycle: ServerContinuousReceiveLoopLifecyclePlan {
                    state: ServerContinuousReceiveLoopLifecycleState::PreparingRejectionLogs,
                    action: ServerContinuousReceiveLoopAction::PrepareRejectionLogs,
                    operational_log_required: true,
                    rejection_log_required: true,
                    handler_handoff_required: false,
                },
                packet_len: Some(256),
            }
        );
    }

    #[test]
    fn continuous_receive_loop_writer_handoff_plans_operational_log_for_accepted_outcome() {
        let source = packet_source();
        let boundary = ServerContinuousReceiveLoopWriterHandoffBoundary::default();
        let outcome = ServerReceiveLoopGateOutcome::Accepted(heartbeat_route("client-1", source));

        let plan = boundary.plan_after_gate_outcome(160, &outcome);

        assert_eq!(plan.tick.packet_len, Some(160));
        assert_eq!(
            plan.tick.state,
            ServerContinuousReceiveLoopTickState::AcceptedRouteReadyForHandoff
        );
        assert!(plan.handler_handoff_required);
        assert_eq!(plan.rejection_log, None);
        assert_eq!(
            plan.operational_log,
            Some(ServerReceiveLoopLogInput {
                source,
                outcome: ServerReceiveLoopLogOutcome::Accepted,
                packet_len: 160,
                message_type: Some(MessageType::Heartbeat),
                client_id: Some(ClientId("client-1".to_string())),
                rejection_reason: None,
            })
        );
    }

    #[test]
    fn continuous_receive_loop_writer_handoff_plans_operational_and_rejection_logs() {
        let source = packet_source();
        let boundary = ServerContinuousReceiveLoopWriterHandoffBoundary::default();
        let rejection = ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
            source,
            message_type: MessageType::VideoFrame,
            client_id: Some(ClientId("client-1".to_string())),
            reason: PacketAcceptanceRejectReason::EndpointMismatch,
        });
        let outcome = ServerReceiveLoopGateOutcome::Rejected(rejection.clone());

        let plan = boundary.plan_after_gate_outcome(256, &outcome);

        assert_eq!(plan.tick.packet_len, Some(256));
        assert_eq!(
            plan.tick.state,
            ServerContinuousReceiveLoopTickState::RejectionReadyForLogs
        );
        assert!(!plan.handler_handoff_required);
        assert_eq!(plan.rejection_log, Some(rejection));
        assert_eq!(
            plan.operational_log,
            Some(ServerReceiveLoopLogInput {
                source,
                outcome: ServerReceiveLoopLogOutcome::AcceptanceRejected,
                packet_len: 256,
                message_type: Some(MessageType::VideoFrame),
                client_id: Some(ClientId("client-1".to_string())),
                rejection_reason: Some(ServerReceiveRejectionReason::EndpointMismatch),
            })
        );
    }

    #[test]
    fn continuous_receive_loop_writer_runtime_writes_operational_log_for_accepted_outcome() {
        let source = packet_source();
        let boundary = ServerContinuousReceiveLoopWriterRuntimeBoundary::default();
        let outcome = ServerReceiveLoopGateOutcome::Accepted(heartbeat_route("client-1", source));
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        let result = boundary
            .write_after_gate_outcome(
                160,
                &outcome,
                TimestampMicros(901_234),
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("writer runtime should succeed");

        assert!(result.operational_event.is_some());
        assert_eq!(result.rejection_event, None);
        assert!(result.handoff.handler_handoff_required);
        let operational_output =
            String::from_utf8(operational_output).expect("json line should be utf8");
        assert!(operational_output.contains(r#""event_name":"server.receive_loop""#));
        assert!(operational_output.contains(r#""outcome":"Accepted""#));
        assert!(operational_output.contains(r#""timestamp":901234"#));
        assert!(rejection_output.is_empty());
    }

    #[test]
    fn continuous_receive_loop_writer_runtime_writes_operational_and_rejection_logs() {
        let source = packet_source();
        let boundary = ServerContinuousReceiveLoopWriterRuntimeBoundary::default();
        let outcome = ServerReceiveLoopGateOutcome::Rejected(
            ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
                source,
                message_type: MessageType::VideoFrame,
                client_id: Some(ClientId("client-1".to_string())),
                reason: PacketAcceptanceRejectReason::EndpointMismatch,
            }),
        );
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        let result = boundary
            .write_after_gate_outcome(
                256,
                &outcome,
                TimestampMicros(912_345),
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("writer runtime should succeed");

        assert!(result.operational_event.is_some());
        assert!(result.rejection_event.is_some());
        assert!(!result.handoff.handler_handoff_required);
        let operational_output =
            String::from_utf8(operational_output).expect("json line should be utf8");
        let rejection_output =
            String::from_utf8(rejection_output).expect("json line should be utf8");
        assert!(operational_output.contains(r#""event_name":"server.receive_loop""#));
        assert!(operational_output.contains(r#""outcome":"AcceptanceRejected""#));
        assert!(rejection_output.contains(r#""event_name":"server.receive_rejection""#));
        assert!(rejection_output.contains(r#""rejection_reason":"EndpointMismatch""#));
        assert!(rejection_output.contains(r#""timestamp":912345"#));
    }

    #[test]
    fn continuous_receive_loop_handler_handoff_runtime_prepares_auth_input() {
        let source = packet_source();
        let boundary = ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary::default();
        let registry = AuthenticatedSenderRegistry::default();
        let outcome = ServerReceiveLoopGateOutcome::Accepted(ServerInboundRoute::AuthRequest {
            source,
            request: auth_request("client-1", "presented-secret"),
        });
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        let result = boundary
            .handoff_after_gate_outcome(
                &registry,
                192,
                outcome,
                TimestampMicros(923_456),
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("handler handoff runtime should succeed");

        assert!(matches!(
            result.handler,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Auth(ServerAuthCheck {
                request: AuthRequest { ref client_id, .. },
                ..
            }) if client_id == &ClientId("client-1".to_string())
        ));
        let dispatch = ServerContinuousReceiveLoopHandlerDispatchBoundary
            .plan_from_handler_handoff(&result.handler);
        assert!(matches!(
            dispatch,
            ServerContinuousReceiveLoopHandlerDispatchPlan::Auth(ServerAuthCheck {
                request: AuthRequest { ref client_id, .. },
                ..
            }) if client_id == &ClientId("client-1".to_string())
        ));
        assert!(result.writer.operational_event.is_some());
        assert_eq!(result.writer.rejection_event, None);
        assert!(String::from_utf8(operational_output)
            .expect("json line should be utf8")
            .contains(r#""outcome":"Accepted""#));
        assert!(rejection_output.is_empty());
    }

    #[test]
    fn continuous_receive_loop_handler_handoff_runtime_prepares_registered_input() {
        let source = packet_source();
        let boundary = ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary::default();
        let registry = registry_with_client("client-1", source);
        let outcome = ServerReceiveLoopGateOutcome::Accepted(heartbeat_route("client-1", source));
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        let result = boundary
            .handoff_after_gate_outcome(
                &registry,
                160,
                outcome,
                TimestampMicros(934_567),
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("handler handoff runtime should succeed");

        assert!(matches!(
            result.handler,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(
                ServerRegisteredClientPacket::Heartbeat(ServerRegisteredHeartbeatPacket {
                    heartbeat: Heartbeat { ref client_id, .. },
                    ..
                })
            ) if client_id == &ClientId("client-1".to_string())
        ));
        assert!(result.writer.operational_event.is_some());
        assert_eq!(result.writer.rejection_event, None);
        assert!(!operational_output.is_empty());
        assert!(rejection_output.is_empty());
    }

    #[test]
    fn continuous_receive_loop_handler_handoff_runtime_skips_rejected_outcome() {
        let source = packet_source();
        let boundary = ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary::default();
        let registry = AuthenticatedSenderRegistry::default();
        let outcome = ServerReceiveLoopGateOutcome::Rejected(
            ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
                source,
                message_type: MessageType::VideoFrame,
                client_id: Some(ClientId("client-1".to_string())),
                reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
            }),
        );
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        let result = boundary
            .handoff_after_gate_outcome(
                &registry,
                256,
                outcome,
                TimestampMicros(945_678),
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("handler handoff runtime should succeed");

        assert_eq!(
            result.handler,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::NotRequired(
                ServerContinuousReceiveLoopHandlerHandoffSkipReason::RejectedOutcome
            )
        );
        let dispatch = ServerContinuousReceiveLoopHandlerDispatchBoundary
            .plan_from_handler_handoff(&result.handler);
        assert_eq!(
            dispatch,
            ServerContinuousReceiveLoopHandlerDispatchPlan::NotRequired(
                ServerContinuousReceiveLoopHandlerDispatchSkipReason::RejectedOutcome
            )
        );
        assert!(result.writer.operational_event.is_some());
        assert!(result.writer.rejection_event.is_some());
        assert!(!operational_output.is_empty());
        assert!(!rejection_output.is_empty());
    }

    #[test]
    fn continuous_receive_loop_one_tick_runtime_stops_before_socket_receive() {
        let boundary = ServerContinuousReceiveLoopOneTickRuntimeBoundary::default();
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let registry = AuthenticatedSenderRegistry::default();
        let mut buffer = [0_u8; 128];
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        let result = boundary
            .execute_one_tick(
                &receiver,
                &mut buffer,
                &registry,
                ServerContinuousReceiveLoopOneTickRuntimeInput {
                    expected_protocol_version: ProtocolVersion(2),
                    timestamp: TimestampMicros(956_789),
                    stop_requested: true,
                },
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("one tick runtime should stop cleanly");

        assert_eq!(
            result.start_tick,
            ServerContinuousReceiveLoopTickPlan {
                state: ServerContinuousReceiveLoopTickState::Stopped,
                lifecycle: ServerContinuousReceiveLoopLifecyclePlan {
                    state: ServerContinuousReceiveLoopLifecycleState::Stopped,
                    action: ServerContinuousReceiveLoopAction::Stop,
                    operational_log_required: false,
                    rejection_log_required: false,
                    handler_handoff_required: false,
                },
                packet_len: None,
            }
        );
        assert_eq!(
            result.outcome,
            ServerContinuousReceiveLoopOneTickRuntimeOutcome::Stopped
        );
        assert!(operational_output.is_empty());
        assert!(rejection_output.is_empty());
    }

    #[test]
    fn continuous_receive_loop_one_tick_runtime_receives_writes_and_prepares_handler() {
        let boundary = ServerContinuousReceiveLoopOneTickRuntimeBoundary::default();
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("read timeout should be set");
        let receiver_addr = receiver.local_addr().expect("receiver should have address");
        let sender_source: PacketSource = sender
            .local_addr()
            .expect("sender should have address")
            .into();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let registry = registry_with_client("client-1", sender_source);
        let mut buffer = [0_u8; 256];
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        sender
            .send_to(packet.as_slice(), receiver_addr)
            .expect("packet should send");

        let result = boundary
            .execute_one_tick(
                &receiver,
                &mut buffer,
                &registry,
                ServerContinuousReceiveLoopOneTickRuntimeInput {
                    expected_protocol_version: ProtocolVersion(2),
                    timestamp: TimestampMicros(967_890),
                    stop_requested: false,
                },
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("one tick runtime should succeed");

        assert_eq!(
            result.start_tick.state,
            ServerContinuousReceiveLoopTickState::AwaitingSocketReceive
        );
        let ServerContinuousReceiveLoopOneTickRuntimeOutcome::Completed {
            packet_len,
            handler,
        } = result.outcome
        else {
            panic!("expected completed one tick outcome");
        };
        assert_eq!(packet_len, packet.len());
        assert!(matches!(
            handler.handler,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(
                ServerRegisteredClientPacket::Heartbeat(ServerRegisteredHeartbeatPacket {
                    heartbeat: Heartbeat { ref client_id, .. },
                    ..
                })
            ) if client_id == &ClientId("client-1".to_string())
        ));
        assert!(handler.writer.operational_event.is_some());
        assert_eq!(handler.writer.rejection_event, None);
        assert!(String::from_utf8(operational_output)
            .expect("json line should be utf8")
            .contains(r#""event_name":"server.receive_loop""#));
        assert!(rejection_output.is_empty());
    }

    #[test]
    fn continuous_receive_loop_body_stops_without_receiving() {
        let boundary = ServerContinuousReceiveLoopBodyBoundary::default();
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let registry = AuthenticatedSenderRegistry::default();
        let mut buffer = [0_u8; 128];
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        let result = boundary
            .run_once(
                &receiver,
                &mut buffer,
                &registry,
                ServerContinuousReceiveLoopBodyInput {
                    expected_protocol_version: ProtocolVersion(2),
                    timestamp: TimestampMicros(978_901),
                    stop_requested: true,
                },
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("loop body should stop cleanly");

        assert_eq!(result.action, ServerContinuousReceiveLoopBodyAction::Stop);
        assert_eq!(
            result.tick.outcome,
            ServerContinuousReceiveLoopOneTickRuntimeOutcome::Stopped
        );
        let dispatch =
            ServerContinuousReceiveLoopHandlerDispatchBoundary.plan_from_body_result(&result);
        assert_eq!(
            dispatch,
            ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: None,
                plan: ServerContinuousReceiveLoopHandlerDispatchPlan::NotRequired(
                    ServerContinuousReceiveLoopHandlerDispatchSkipReason::LoopStopped
                ),
            }
        );
        assert!(operational_output.is_empty());
        assert!(rejection_output.is_empty());
    }

    #[test]
    fn continuous_receive_loop_controller_stops_when_continue_not_requested() {
        let controller = ServerContinuousReceiveLoopControllerBoundary;

        let plan = controller.plan_next_iteration(ServerContinuousReceiveLoopControllerInput {
            expected_protocol_version: ProtocolVersion(2),
            timestamp: TimestampMicros(990_001),
            continue_requested: false,
        });

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopControllerPlan {
                state: ServerContinuousReceiveLoopControllerState::Stopped,
                action: ServerContinuousReceiveLoopControllerAction::Stop,
                body_input: None,
            }
        );
    }

    #[test]
    fn continuous_receive_loop_controller_plans_one_body_iteration() {
        let controller = ServerContinuousReceiveLoopControllerBoundary;

        let plan = controller.plan_next_iteration(ServerContinuousReceiveLoopControllerInput {
            expected_protocol_version: ProtocolVersion(2),
            timestamp: TimestampMicros(990_002),
            continue_requested: true,
        });

        assert_eq!(
            plan,
            ServerContinuousReceiveLoopControllerPlan {
                state: ServerContinuousReceiveLoopControllerState::ReadyToRunBodyOnce,
                action: ServerContinuousReceiveLoopControllerAction::RunBodyOnce,
                body_input: Some(ServerContinuousReceiveLoopBodyInput {
                    expected_protocol_version: ProtocolVersion(2),
                    timestamp: TimestampMicros(990_002),
                    stop_requested: false,
                }),
            }
        );
    }

    #[test]
    fn continuous_receive_loop_handler_dispatch_plans_auth_input() {
        let source = packet_source();
        let auth = ServerAuthCheck {
            source,
            request: auth_request("client-1", "presented-secret"),
        };

        let dispatch = ServerContinuousReceiveLoopHandlerDispatchBoundary
            .plan_from_handler_handoff(
                &ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Auth(auth),
            );

        assert!(matches!(
            dispatch,
            ServerContinuousReceiveLoopHandlerDispatchPlan::Auth(ServerAuthCheck {
                request: AuthRequest { ref client_id, .. },
                ..
            }) if client_id == &ClientId("client-1".to_string())
        ));
    }

    #[test]
    fn handler_dispatch_body_routes_auth_without_deciding() {
        let source = packet_source();
        let auth = ServerAuthCheck {
            source,
            request: auth_request("client-1", "presented-secret"),
        };

        let outcome = ServerHandlerDispatchBoundary.dispatch_handoff(
            ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: Some(88),
                plan: ServerContinuousReceiveLoopHandlerDispatchPlan::Auth(auth),
            },
        );

        assert_eq!(outcome.packet_len, Some(88));
        assert!(matches!(
            outcome.result,
            ServerHandlerDispatchResult::Auth(ServerAuthCheck {
                request: AuthRequest { ref client_id, .. },
                ..
            }) if client_id == &ClientId("client-1".to_string())
        ));
    }

    #[test]
    fn handler_dispatch_body_routes_client_stats_without_handling() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let route = client_stats_route("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, route)
            .expect("client stats should be accepted");

        let outcome = ServerHandlerDispatchBoundary.dispatch_handoff(
            ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: Some(96),
                plan: ServerContinuousReceiveLoopHandlerDispatchPlan::RegisteredClient(registered),
            },
        );

        assert_eq!(outcome.packet_len, Some(96));
        assert!(matches!(
            outcome.result,
            ServerHandlerDispatchResult::RegisteredClientStats(ServerRegisteredClientStatsPacket {
                stats: ClientStats {
                    capture_fps: 30,
                    ..
                },
                ..
            })
        ));
    }

    #[test]
    fn auth_dispatch_runtime_connects_auth_to_flow_step() {
        let source = packet_source();
        let auth = ServerAuthCheck {
            source,
            request: auth_request("client-1", "presented-secret"),
        };
        let handler_outcome = ServerHandlerDispatchBoundary.dispatch_handoff(
            ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: Some(88),
                plan: ServerContinuousReceiveLoopHandlerDispatchPlan::Auth(auth),
            },
        );
        let runtime = ServerAuthDispatchRuntimeBoundary::default();

        let outcome =
            runtime.dispatch_outcome(handler_outcome, &auth_config(Some("presented-secret")));

        assert_eq!(outcome.packet_len, Some(88));
        let ServerAuthDispatchRuntimeResult::Dispatched(flow) = outcome.result else {
            panic!("expected auth flow dispatch");
        };
        assert!(flow.decision.accepted);
        assert_eq!(flow.decision.reason_code, AuthResponseReasonCode::Ok);
        assert_eq!(
            flow.registry_registration,
            Some(AuthenticatedSenderRegistration {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            })
        );
        let ProtocolMessage::AuthResponse(response) = flow.queue_item.packet.message else {
            panic!("expected queued AuthResponse");
        };
        assert!(response.accepted);
        assert_eq!(response.reason_code, AuthResponseReasonCode::Ok);
    }

    #[test]
    fn auth_dispatch_runtime_skips_non_auth_results() {
        let runtime = ServerAuthDispatchRuntimeBoundary::default();
        let handler_outcome = ServerHandlerDispatchOutcome {
            packet_len: None,
            result: ServerHandlerDispatchResult::NotRequired(
                ServerContinuousReceiveLoopHandlerDispatchSkipReason::LoopStopped,
            ),
        };

        let outcome =
            runtime.dispatch_outcome(handler_outcome, &auth_config(Some("presented-secret")));

        assert_eq!(outcome.packet_len, None);
        assert_eq!(
            outcome.result,
            ServerAuthDispatchRuntimeResult::NotAuth(ServerHandlerDispatchResult::NotRequired(
                ServerContinuousReceiveLoopHandlerDispatchSkipReason::LoopStopped
            ))
        );
    }

    #[test]
    fn registered_packet_dispatch_runtime_connects_heartbeat_to_ack_handoff() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let route = heartbeat_route("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, route)
            .expect("heartbeat should be accepted");
        let handler_outcome = ServerHandlerDispatchBoundary.dispatch_handoff(
            ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: Some(72),
                plan: ServerContinuousReceiveLoopHandlerDispatchPlan::RegisteredClient(registered),
            },
        );
        let timing = ServerHeartbeatAckTiming {
            server_received_at: TimestampMicros(2_000_100),
            server_sent_at: TimestampMicros(2_000_200),
        };
        let runtime = ServerRegisteredPacketDispatchRuntimeBoundary::default();

        let outcome = runtime.dispatch_outcome(handler_outcome, timing);

        assert_eq!(outcome.packet_len, Some(72));
        let ServerRegisteredPacketDispatchRuntimeResult::HeartbeatAck(handoff) = outcome.result
        else {
            panic!("expected heartbeat ack handoff");
        };
        assert_eq!(handoff.ack_input.destination, source);
        assert_eq!(handoff.ack_input.echoed_sent_at, TimestampMicros(1_234_567));
        let ProtocolMessage::HeartbeatAck(ack) = handoff.queue_item.packet.message else {
            panic!("expected queued HeartbeatAck");
        };
        assert_eq!(ack.client_id, ClientId("client-1".to_string()));
        assert_eq!(ack.server_received_at, TimestampMicros(2_000_100));
        assert_eq!(ack.server_sent_at, TimestampMicros(2_000_200));
    }

    #[test]
    fn registered_packet_dispatch_runtime_preserves_future_client_stats() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let route = client_stats_route("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, route)
            .expect("client stats should be accepted");
        let handler_outcome = ServerHandlerDispatchBoundary.dispatch_handoff(
            ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: Some(96),
                plan: ServerContinuousReceiveLoopHandlerDispatchPlan::RegisteredClient(registered),
            },
        );
        let timing = ServerHeartbeatAckTiming {
            server_received_at: TimestampMicros(2_000_100),
            server_sent_at: TimestampMicros(2_000_200),
        };
        let runtime = ServerRegisteredPacketDispatchRuntimeBoundary::default();

        let outcome = runtime.dispatch_outcome(handler_outcome, timing);

        assert_eq!(outcome.packet_len, Some(96));
        assert!(matches!(
            outcome.result,
            ServerRegisteredPacketDispatchRuntimeResult::FutureClientStats(
                ServerRegisteredClientStatsPacket {
                    stats: ClientStats {
                        capture_fps: 30,
                        ..
                    },
                    ..
                }
            )
        ));
    }

    #[test]
    fn video_stats_handler_runtime_prepares_video_input_without_buffering() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let route = video_frame_route("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, route)
            .expect("video frame should be accepted");
        let handler_outcome = ServerHandlerDispatchBoundary.dispatch_handoff(
            ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: Some(128),
                plan: ServerContinuousReceiveLoopHandlerDispatchPlan::RegisteredClient(registered),
            },
        );
        let timing = ServerHeartbeatAckTiming {
            server_received_at: TimestampMicros(2_000_100),
            server_sent_at: TimestampMicros(2_000_200),
        };
        let registered_outcome = ServerRegisteredPacketDispatchRuntimeBoundary::default()
            .dispatch_outcome(handler_outcome, timing);
        let runtime = ServerVideoStatsHandlerRuntimeBoundary::default();

        let outcome = runtime.dispatch_outcome(registered_outcome);

        assert_eq!(outcome.packet_len, Some(128));
        let ServerVideoStatsHandlerRuntimeResult::VideoFrame(input) = outcome.result else {
            panic!("expected video handler input");
        };
        assert_eq!(input.payload_len, 3);
        assert_eq!(input.registered_packet.source, source);
        assert_eq!(
            input.registered_packet.authenticated_sender.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(input.registered_packet.frame.frame_id, 42);
        assert!(input.registered_packet.frame.is_keyframe);
    }

    #[test]
    fn video_frame_queue_storage_stores_accepted_frame_per_client() {
        let source = packet_source();
        let input = video_handler_input("client-1", source);
        let mut state = ServerVideoFrameQueueState::default();

        let result = ServerVideoFrameQueueStorageBoundary.store_frame(
            &mut state,
            input,
            TimestampMicros(2_500_000),
            ServerVideoFrameQueuePolicy::default(),
        );

        let ServerVideoFrameQueueStorageResult::Stored {
            queued,
            previous_client_queue_len,
            current_client_queue_len,
            dropped_oldest,
        } = result
        else {
            panic!("accepted video frame should be stored");
        };
        assert_eq!(previous_client_queue_len, 0);
        assert_eq!(current_client_queue_len, 1);
        assert_eq!(dropped_oldest, None);
        assert_eq!(queued.frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(queued.frame.frame_id, 42);
        assert_eq!(queued.payload_len, 3);
        assert_eq!(queued.queued_at, TimestampMicros(2_500_000));
        assert_eq!(state.total_len(), 1);
        assert_eq!(state.client_queue_len(&ClientId("client-1".to_string())), 1);
    }

    #[test]
    fn video_frame_queue_storage_keeps_state_caller_owned() {
        let source = packet_source();
        let input = video_handler_input("client-1", source);
        let mut state = ServerVideoFrameQueueState::default();

        let _result = ServerVideoFrameQueueStorageBoundary.store_frame(
            &mut state,
            input,
            TimestampMicros(2_600_000),
            ServerVideoFrameQueuePolicy::default(),
        );

        let queued = state
            .pop_front(&ClientId("client-1".to_string()))
            .expect("caller-owned queue should hold the frame");
        assert_eq!(queued.frame.frame_id, 42);
        assert_eq!(state.total_len(), 0);
    }

    #[test]
    fn video_frame_queue_storage_drops_oldest_when_client_queue_is_full() {
        let source = packet_source();
        let mut first = video_handler_input("client-1", source);
        first.registered_packet.frame.frame_id = 1;
        let mut second = video_handler_input("client-1", source);
        second.registered_packet.frame.frame_id = 2;
        let mut state = ServerVideoFrameQueueState::default();
        let policy = ServerVideoFrameQueuePolicy {
            max_frames_per_client: 1,
        };

        let first_result = ServerVideoFrameQueueStorageBoundary.store_frame(
            &mut state,
            first,
            TimestampMicros(2_700_000),
            policy,
        );
        assert!(matches!(
            first_result,
            ServerVideoFrameQueueStorageResult::Stored { .. }
        ));
        let second_result = ServerVideoFrameQueueStorageBoundary.store_frame(
            &mut state,
            second,
            TimestampMicros(2_700_100),
            policy,
        );

        let ServerVideoFrameQueueStorageResult::Stored {
            current_client_queue_len,
            dropped_oldest,
            ..
        } = second_result
        else {
            panic!("newest frame should be stored after dropping oldest");
        };
        assert_eq!(current_client_queue_len, 1);
        assert_eq!(
            dropped_oldest
                .expect("oldest frame should be dropped")
                .frame
                .frame_id,
            1
        );
        let remaining: Vec<u64> = state
            .frames_for_client(&ClientId("client-1".to_string()))
            .map(|frame| frame.frame.frame_id)
            .collect();
        assert_eq!(remaining, vec![2]);
    }

    #[test]
    fn video_frame_queue_storage_keeps_display_execution_deferred() {
        let source = packet_source();
        let input = video_handler_input("client-1", source);
        let mut state = ServerVideoFrameQueueState::default();

        let result = ServerVideoFrameQueueStorageBoundary.store_frame(
            &mut state,
            input,
            TimestampMicros(2_800_000),
            ServerVideoFrameQueuePolicy {
                max_frames_per_client: 0,
            },
        );

        assert!(matches!(
            result,
            ServerVideoFrameQueueStorageResult::Dropped {
                reason: ServerVideoFrameQueueDropReason::CapacityZero,
                ..
            }
        ));
        assert_eq!(state.total_len(), 0);
    }

    #[test]
    fn video_frame_queue_runtime_stores_authenticated_frame_in_client_queue() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, video_frame_route("client-1", source))
            .expect("video frame should be accepted");
        let body = body_result_with_handler_handoff(
            128,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(registered),
        );
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(&body, &auth_config(None), body_dispatch_timing());
        let side_effect = ServerDispatchRuntimeSideEffectApplyBoundary::default()
            .apply_body_dispatch_outcome(&mut AuthenticatedSenderRegistry::default(), dispatch);
        let mut state = ServerVideoFrameQueueState::default();

        let result = ServerVideoFrameQueueRuntimeBoundary::default()
            .store_from_receive_side_effect(
                &mut state,
                &body,
                side_effect,
                TimestampMicros(2_900_000),
                ServerVideoFrameQueuePolicy::default(),
            );

        let ServerVideoFrameQueueRuntimeResult::Queued(
            ServerVideoFrameQueueStorageResult::Stored {
                queued,
                previous_client_queue_len,
                current_client_queue_len,
                dropped_oldest,
            },
        ) = result
        else {
            panic!("authenticated video frame should be queued");
        };
        assert_eq!(queued.frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(queued.frame.frame_id, 42);
        assert_eq!(queued.payload_len, 3);
        assert_eq!(previous_client_queue_len, 0);
        assert_eq!(current_client_queue_len, 1);
        assert_eq!(dropped_oldest, None);
        assert_eq!(state.client_queue_len(&ClientId("client-1".to_string())), 1);
    }

    #[test]
    fn video_frame_queue_runtime_does_not_store_rejected_video_frame() {
        let source = packet_source();
        let rejection = ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
            source,
            message_type: MessageType::VideoFrame,
            client_id: Some(ClientId("client-1".to_string())),
            reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
        });
        let body = body_result_with_gate_rejection(128, rejection.clone());
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(&body, &auth_config(None), body_dispatch_timing());
        let side_effect = ServerDispatchRuntimeSideEffectApplyBoundary::default()
            .apply_body_dispatch_outcome(&mut AuthenticatedSenderRegistry::default(), dispatch);
        let mut state = ServerVideoFrameQueueState::default();

        let result = ServerVideoFrameQueueRuntimeBoundary::default()
            .store_from_receive_side_effect(
                &mut state,
                &body,
                side_effect,
                TimestampMicros(2_900_100),
                ServerVideoFrameQueuePolicy::default(),
            );

        assert_eq!(state.total_len(), 0);
        let ServerVideoFrameQueueRuntimeResult::NotQueued {
            reason: ServerVideoFrameQueueRuntimeSkipReason::RejectedVideoFrame(actual_rejection),
            side_effect,
        } = result
        else {
            panic!("rejected video frame should be reported as not queued");
        };
        assert_eq!(actual_rejection, rejection);
        assert!(matches!(
            side_effect,
            ServerDispatchRuntimeSideEffectApplyResult::NoDispatch(ServerHandlerDispatchOutcome {
                result: ServerHandlerDispatchResult::NotRequired(
                    ServerContinuousReceiveLoopHandlerDispatchSkipReason::RejectedOutcome
                ),
                ..
            })
        ));
    }

    #[test]
    fn video_frame_queue_runtime_surfaces_drop_oldest_storage_result() {
        let source = packet_source();
        let mut state = ServerVideoFrameQueueState::default();
        let policy = ServerVideoFrameQueuePolicy {
            max_frames_per_client: 1,
        };
        let first = video_frame_queue_runtime_body_for_frame("client-1", source, 1);
        let first_side_effect = video_frame_side_effect_from_body(&first);
        let first_result = ServerVideoFrameQueueRuntimeBoundary::default()
            .store_from_receive_side_effect(
                &mut state,
                &first,
                first_side_effect,
                TimestampMicros(2_900_200),
                policy,
            );
        assert!(matches!(
            first_result,
            ServerVideoFrameQueueRuntimeResult::Queued(
                ServerVideoFrameQueueStorageResult::Stored { .. }
            )
        ));

        let second = video_frame_queue_runtime_body_for_frame("client-1", source, 2);
        let second_side_effect = video_frame_side_effect_from_body(&second);
        let second_result = ServerVideoFrameQueueRuntimeBoundary::default()
            .store_from_receive_side_effect(
                &mut state,
                &second,
                second_side_effect,
                TimestampMicros(2_900_300),
                policy,
            );

        let ServerVideoFrameQueueRuntimeResult::Queued(
            ServerVideoFrameQueueStorageResult::Stored {
                current_client_queue_len,
                dropped_oldest,
                queued,
                ..
            },
        ) = second_result
        else {
            panic!("second frame should be queued after dropping oldest");
        };
        assert_eq!(current_client_queue_len, 1);
        assert_eq!(queued.frame.frame_id, 2);
        assert_eq!(
            dropped_oldest
                .expect("oldest should be surfaced")
                .frame
                .frame_id,
            1
        );
        let remaining: Vec<u64> = state
            .frames_for_client(&ClientId("client-1".to_string()))
            .map(|frame| frame.frame.frame_id)
            .collect();
        assert_eq!(remaining, vec![2]);
    }

    #[test]
    fn video_frame_queue_runtime_keeps_decode_and_display_deferred() {
        let source = packet_source();
        let body = video_frame_queue_runtime_body_for_frame("client-1", source, 42);
        let side_effect = video_frame_side_effect_from_body(&body);
        let mut state = ServerVideoFrameQueueState::default();

        let result = ServerVideoFrameQueueRuntimeBoundary::default()
            .store_from_receive_side_effect(
                &mut state,
                &body,
                side_effect,
                TimestampMicros(2_900_400),
                ServerVideoFrameQueuePolicy::default(),
            );

        let ServerVideoFrameQueueRuntimeResult::Queued(
            ServerVideoFrameQueueStorageResult::Stored { queued, .. },
        ) = result
        else {
            panic!("video frame should be queued");
        };
        assert_eq!(queued.frame.payload, vec![0xaa, 0xbb, 0xcc]);
        assert_eq!(queued.payload_len, queued.frame.payload.len());
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn video_frame_queue_read_boundary_inspects_oldest_for_client_run_without_mutation() {
        let source = packet_source();
        let mut state = ServerVideoFrameQueueState::default();
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 1);
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 2);
        store_video_frame_for_read_test(&mut state, "client-1", "run-2", 3);

        let result = ServerVideoFrameQueueReadBoundary.read(
            &mut state,
            ServerVideoFrameQueueReadInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: ServerVideoFrameQueueReadMode::InspectOldest,
            },
        );

        let ServerVideoFrameQueueReadResult::FrameAvailable {
            frame,
            mode,
            remaining_client_queue_len,
        } = result
        else {
            panic!("oldest run frame should be readable");
        };
        assert_eq!(frame.source, source);
        assert_eq!(frame.frame.frame_id, 1);
        assert_eq!(mode, ServerVideoFrameQueueReadMode::InspectOldest);
        assert_eq!(remaining_client_queue_len, 3);
        assert_eq!(state.client_queue_len(&ClientId("client-1".to_string())), 3);
    }

    #[test]
    fn video_frame_queue_read_boundary_inspects_latest_for_client_run() {
        let mut state = ServerVideoFrameQueueState::default();
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 1);
        store_video_frame_for_read_test(&mut state, "client-1", "run-2", 2);
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 3);

        let result = ServerVideoFrameQueueReadBoundary.read(
            &mut state,
            ServerVideoFrameQueueReadInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: ServerVideoFrameQueueReadMode::InspectLatest,
            },
        );

        let ServerVideoFrameQueueReadResult::FrameAvailable { frame, .. } = result else {
            panic!("latest run frame should be readable");
        };
        assert_eq!(frame.frame.frame_id, 3);
        assert_eq!(state.total_len(), 3);
    }

    #[test]
    fn video_frame_queue_read_boundary_dequeues_oldest_for_client_run_only() {
        let mut state = ServerVideoFrameQueueState::default();
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 1);
        store_video_frame_for_read_test(&mut state, "client-1", "run-2", 2);
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 3);
        store_video_frame_for_read_test(&mut state, "client-2", "run-1", 4);

        let result = ServerVideoFrameQueueReadBoundary.read(
            &mut state,
            ServerVideoFrameQueueReadInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: ServerVideoFrameQueueReadMode::DequeueOldest,
            },
        );

        let ServerVideoFrameQueueReadResult::FrameAvailable {
            frame,
            mode,
            remaining_client_queue_len,
        } = result
        else {
            panic!("oldest matching run frame should be dequeued");
        };
        assert_eq!(frame.frame.frame_id, 1);
        assert_eq!(mode, ServerVideoFrameQueueReadMode::DequeueOldest);
        assert_eq!(remaining_client_queue_len, 2);
        let client_1_remaining: Vec<(String, u64)> = state
            .frames_for_client(&ClientId("client-1".to_string()))
            .map(|queued| (queued.frame.run_id.0.clone(), queued.frame.frame_id))
            .collect();
        assert_eq!(
            client_1_remaining,
            vec![("run-2".to_string(), 2), ("run-1".to_string(), 3)]
        );
        assert_eq!(state.client_queue_len(&ClientId("client-2".to_string())), 1);
    }

    #[test]
    fn video_frame_queue_read_boundary_reports_no_frame_for_missing_run() {
        let mut state = ServerVideoFrameQueueState::default();
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 1);

        let result = ServerVideoFrameQueueReadBoundary.read(
            &mut state,
            ServerVideoFrameQueueReadInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-missing".to_string()),
                mode: ServerVideoFrameQueueReadMode::DequeueOldest,
            },
        );

        assert_eq!(
            result,
            ServerVideoFrameQueueReadResult::NoFrameAvailable {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-missing".to_string()),
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn server_switcher_handoff_handler_returns_frame_read_for_eligible_queued_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_video_frame_for_handoff_test(
            &mut state,
            "client-1",
            "run-meta",
            7,
            TimestampMicros(9_000_007),
            854,
            480,
            vec![0xaa, 0xbb, 0xcc, 0xdd],
        );

        let response = ServerSwitcherQueuedFrameHandoffHandlerBoundary::default().handle_request(
            &mut state,
            ServerSwitcherQueuedFrameHandoffRequest {
                handoff_version: 1,
                request_id: 44,
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-meta".to_string()),
                read_mode: ServerSwitcherQueuedFrameReadMode::InspectLatest,
            },
        );

        let ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
            request_id,
            remaining_client_queue_len,
            frame,
        } = response
        else {
            panic!("eligible queued frame should produce FrameRead");
        };
        assert_eq!(request_id, 44);
        assert_eq!(remaining_client_queue_len, 1);
        assert_eq!(frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(frame.run_id, RunId("run-meta".to_string()));
        assert_eq!(frame.frame_id, 7);
        assert_eq!(frame.capture_timestamp, TimestampMicros(1_000_007));
        assert_eq!(frame.send_timestamp, TimestampMicros(1_000_107));
        assert_eq!(frame.queued_at, TimestampMicros(9_000_007));
        assert_eq!(frame.width, 854);
        assert_eq!(frame.height, 480);
        assert_eq!(frame.fps_nominal, 30);
        assert!(frame.is_keyframe);
        assert_eq!(frame.codec, Codec::H264);
        assert_eq!(frame.encoded_payload_len, 4);
        assert_eq!(frame.encoded_payload, vec![0xaa, 0xbb, 0xcc, 0xdd]);
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn server_switcher_handoff_handler_returns_no_frame_for_missing_client_run_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 1);

        let response = ServerSwitcherQueuedFrameHandoffHandlerBoundary::default().handle_request(
            &mut state,
            ServerSwitcherQueuedFrameHandoffRequest {
                handoff_version: 1,
                request_id: 45,
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-missing".to_string()),
                read_mode: ServerSwitcherQueuedFrameReadMode::DequeueOldest,
            },
        );

        assert_eq!(
            response,
            ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
                request_id: 45,
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-missing".to_string()),
                read_mode: ServerSwitcherQueuedFrameReadMode::DequeueOldest,
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn server_switcher_handoff_handler_preserves_request_id_in_invalid_scope_error() {
        let mut state = ServerVideoFrameQueueState::default();
        store_video_frame_for_read_test(&mut state, "client-1", "run-1", 1);

        let response = ServerSwitcherQueuedFrameHandoffHandlerBoundary::default().handle_request(
            &mut state,
            ServerSwitcherQueuedFrameHandoffRequest {
                handoff_version: 1,
                request_id: 99,
                client_id: ClientId("".to_string()),
                run_id: RunId("run-1".to_string()),
                read_mode: ServerSwitcherQueuedFrameReadMode::InspectOldest,
            },
        );

        assert_eq!(
            response,
            ServerSwitcherQueuedFrameHandoffResponse::HandoffError {
                request_id: 99,
                error: ServerSwitcherQueuedFrameHandoffErrorCode::InvalidScope,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn video_frame_fragment_reassembly_completes_in_order() {
        let source = packet_source();
        let mut reassembly_state = ServerVideoFrameReassemblyState::default();
        let mut queue_state = ServerVideoFrameQueueState::default();
        let boundary = ServerVideoFrameFragmentReassemblyBoundary::default();

        let first = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 0, 3, vec![0xaa, 0xbb]),
            TimestampMicros(3_100_000),
            ServerVideoFrameQueuePolicy::default(),
        );
        assert!(matches!(
            first,
            ServerVideoFrameReassemblyApplyResult::FragmentStored { .. }
        ));
        let second = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 1, 3, vec![0xcc, 0xdd]),
            TimestampMicros(3_100_100),
            ServerVideoFrameQueuePolicy::default(),
        );
        assert!(matches!(
            second,
            ServerVideoFrameReassemblyApplyResult::FragmentStored { .. }
        ));
        let complete = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 2, 3, vec![0xee]),
            TimestampMicros(3_100_200),
            ServerVideoFrameQueuePolicy::default(),
        );

        let ServerVideoFrameReassemblyApplyResult::FrameComplete {
            summary,
            reassembled_frame,
            queue_result,
        } = complete
        else {
            panic!("third fragment should complete the frame");
        };
        assert_eq!(
            reassembled_frame.payload,
            vec![0xaa, 0xbb, 0xcc, 0xdd, 0xee]
        );
        assert_eq!(reassembled_frame.payload_size, 5);
        assert_eq!(
            reassembled_frame.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(reassembled_frame.run_id, RunId("run-1".to_string()));
        assert_eq!(reassembled_frame.frame_id, 42);
        assert_eq!(
            reassembled_frame.capture_timestamp,
            TimestampMicros(1_000_000)
        );
        assert_eq!(reassembled_frame.width, 1280);
        assert_eq!(reassembled_frame.height, 720);
        assert_eq!(reassembled_frame.fps_nominal, 30);
        assert_eq!(summary.fragments_received, 3);
        assert_eq!(summary.fragments_missing, Vec::<u32>::new());
        assert!(summary.completed_frame_queued);
        assert_eq!(summary.rejected_fragment_reason, None);
        assert!(matches!(
            queue_result,
            ServerVideoFrameQueueStorageResult::Stored { .. }
        ));
        assert_eq!(queue_state.total_len(), 1);
        assert_eq!(reassembly_state.tracked_frame_count(), 0);
    }

    #[test]
    fn video_frame_fragment_reassembly_completes_out_of_order() {
        let source = packet_source();
        let mut reassembly_state = ServerVideoFrameReassemblyState::default();
        let mut queue_state = ServerVideoFrameQueueState::default();
        let boundary = ServerVideoFrameFragmentReassemblyBoundary::default();

        for (index, payload) in [(2, vec![0xee]), (0, vec![0xaa, 0xbb])] {
            let result = boundary.apply_fragment_and_queue_if_complete(
                &mut reassembly_state,
                &mut queue_state,
                registered_video_frame_fragment(source, index, 3, payload),
                TimestampMicros(3_101_000 + u64::from(index)),
                ServerVideoFrameQueuePolicy::default(),
            );
            assert!(matches!(
                result,
                ServerVideoFrameReassemblyApplyResult::FragmentStored { .. }
            ));
        }

        let complete = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 1, 3, vec![0xcc, 0xdd]),
            TimestampMicros(3_101_100),
            ServerVideoFrameQueuePolicy::default(),
        );

        let ServerVideoFrameReassemblyApplyResult::FrameComplete {
            reassembled_frame, ..
        } = complete
        else {
            panic!("missing middle fragment should complete the frame");
        };
        assert_eq!(
            reassembled_frame.payload,
            vec![0xaa, 0xbb, 0xcc, 0xdd, 0xee]
        );
        assert_eq!(queue_state.total_len(), 1);
    }

    #[test]
    fn video_frame_fragment_reassembly_ignores_duplicate_without_corrupting_payload() {
        let source = packet_source();
        let mut reassembly_state = ServerVideoFrameReassemblyState::default();
        let mut queue_state = ServerVideoFrameQueueState::default();
        let boundary = ServerVideoFrameFragmentReassemblyBoundary::default();

        let _first = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 0, 2, vec![0xaa, 0xbb]),
            TimestampMicros(3_102_000),
            ServerVideoFrameQueuePolicy::default(),
        );
        let duplicate = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 0, 2, vec![0xaa, 0xbb]),
            TimestampMicros(3_102_010),
            ServerVideoFrameQueuePolicy::default(),
        );

        let ServerVideoFrameReassemblyApplyResult::DuplicateFragmentIgnored { summary } = duplicate
        else {
            panic!("duplicate fragment should be explicit");
        };
        assert_eq!(summary.fragments_received, 1);
        assert_eq!(summary.fragments_missing, vec![1]);
        assert_eq!(summary.duplicate_count, 1);

        let complete = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 1, 2, vec![0xcc, 0xdd, 0xee]),
            TimestampMicros(3_102_100),
            ServerVideoFrameQueuePolicy::default(),
        );
        let ServerVideoFrameReassemblyApplyResult::FrameComplete {
            reassembled_frame, ..
        } = complete
        else {
            panic!("second unique fragment should complete");
        };
        assert_eq!(
            reassembled_frame.payload,
            vec![0xaa, 0xbb, 0xcc, 0xdd, 0xee]
        );
    }

    #[test]
    fn video_frame_fragment_reassembly_rejects_inconsistent_metadata() {
        let source = packet_source();
        let mut reassembly_state = ServerVideoFrameReassemblyState::default();
        let mut queue_state = ServerVideoFrameQueueState::default();
        let boundary = ServerVideoFrameFragmentReassemblyBoundary::default();

        let _stored = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 0, 2, vec![0xaa, 0xbb]),
            TimestampMicros(3_103_000),
            ServerVideoFrameQueuePolicy::default(),
        );
        let mut inconsistent =
            registered_video_frame_fragment(source, 1, 2, vec![0xcc, 0xdd, 0xee]);
        inconsistent.fragment.width = 1920;

        let rejected = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            inconsistent,
            TimestampMicros(3_103_100),
            ServerVideoFrameQueuePolicy::default(),
        );

        let ServerVideoFrameReassemblyApplyResult::RejectedFragment { summary } = rejected else {
            panic!("inconsistent metadata should be rejected");
        };
        assert_eq!(
            summary.rejected_fragment_reason,
            Some(ServerVideoFrameFragmentRejectReason::MetadataMismatch)
        );
        assert_eq!(summary.fragments_received, 1);
        assert_eq!(queue_state.total_len(), 0);
        assert_eq!(reassembly_state.tracked_frame_count(), 1);
    }

    #[test]
    fn video_frame_fragment_reassembly_keeps_missing_fragment_incomplete() {
        let source = packet_source();
        let mut reassembly_state = ServerVideoFrameReassemblyState::default();
        let mut queue_state = ServerVideoFrameQueueState::default();

        let result = ServerVideoFrameFragmentReassemblyBoundary::default()
            .apply_fragment_and_queue_if_complete(
                &mut reassembly_state,
                &mut queue_state,
                registered_video_frame_fragment(source, 0, 2, vec![0xaa, 0xbb]),
                TimestampMicros(3_104_000),
                ServerVideoFrameQueuePolicy::default(),
            );

        let ServerVideoFrameReassemblyApplyResult::FragmentStored { summary } = result else {
            panic!("one of two fragments should remain incomplete");
        };
        assert_eq!(summary.fragments_received, 1);
        assert_eq!(summary.fragments_missing, vec![1]);
        assert!(!summary.completed_frame_queued);
        assert_eq!(queue_state.total_len(), 0);
        assert_eq!(reassembly_state.tracked_frame_count(), 1);
    }

    #[test]
    fn video_frame_fragment_reassembly_passes_completed_frame_to_queue() {
        let source = packet_source();
        let mut reassembly_state = ServerVideoFrameReassemblyState::default();
        let mut queue_state = ServerVideoFrameQueueState::default();
        let boundary = ServerVideoFrameFragmentReassemblyBoundary::default();

        let _first = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 0, 2, vec![0xaa, 0xbb]),
            TimestampMicros(3_105_000),
            ServerVideoFrameQueuePolicy::default(),
        );
        let complete = boundary.apply_fragment_and_queue_if_complete(
            &mut reassembly_state,
            &mut queue_state,
            registered_video_frame_fragment(source, 1, 2, vec![0xcc, 0xdd, 0xee]),
            TimestampMicros(3_105_100),
            ServerVideoFrameQueuePolicy::default(),
        );

        assert!(matches!(
            complete,
            ServerVideoFrameReassemblyApplyResult::FrameComplete {
                queue_result: ServerVideoFrameQueueStorageResult::Stored { .. },
                ..
            }
        ));
        let queued: Vec<Vec<u8>> = queue_state
            .frames_for_client(&ClientId("client-1".to_string()))
            .map(|queued| queued.frame.payload.clone())
            .collect();
        assert_eq!(queued, vec![vec![0xaa, 0xbb, 0xcc, 0xdd, 0xee]]);
    }

    #[test]
    fn receive_auth_video_queue_once_accepts_auth_then_queues_video_frame() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        client_socket
            .send_to(
                auth_request_packet("client-1", "presented-secret").as_slice(),
                server_address,
            )
            .expect("auth request should send");
        let mut receive_buffer = vec![0_u8; DEFAULT_UDP_PACKET_BUFFER_LEN];
        let mut registry = AuthenticatedSenderRegistry::default();
        let auth = ServerAuthResponsePocStep::default()
            .run_one(
                &server_socket,
                &mut receive_buffer,
                ProtocolVersion(2),
                &auth_config(Some("presented-secret")),
                &mut registry,
            )
            .expect("auth step should complete");
        assert!(auth.auth_flow.decision.accepted);
        let mut response_buffer = vec![0_u8; 1024];
        let _response = client_socket
            .recv_from(&mut response_buffer)
            .expect("auth response should receive");

        client_socket
            .send_to(
                video_frame_packet("client-1", 42).as_slice(),
                server_address,
            )
            .expect("video frame should send");
        let second = run_controller_once_for_test(
            &server_socket,
            &mut receive_buffer,
            &mut registry,
            &auth_config(Some("presented-secret")),
        );
        let mut queue_state = ServerVideoFrameQueueState::default();
        let queue = queue_from_controller_result_for_test(&second, &mut queue_state);

        let ServerVideoFrameQueueRuntimeResult::Queued(
            ServerVideoFrameQueueStorageResult::Stored {
                queued,
                current_client_queue_len,
                dropped_oldest,
                ..
            },
        ) = queue
        else {
            panic!("accepted auth then video should queue a frame");
        };
        assert_eq!(queued.frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(queued.frame.frame_id, 42);
        assert_eq!(current_client_queue_len, 1);
        assert_eq!(dropped_oldest, None);
        assert_eq!(queue_state.total_len(), 1);
    }

    #[test]
    fn receive_auth_video_queue_once_rejected_auth_does_not_queue_later_video() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        client_socket
            .send_to(
                auth_request_packet("client-1", "wrong-secret").as_slice(),
                server_address,
            )
            .expect("auth request should send");
        let mut receive_buffer = vec![0_u8; DEFAULT_UDP_PACKET_BUFFER_LEN];
        let mut registry = AuthenticatedSenderRegistry::default();
        let auth = ServerAuthResponsePocStep::default()
            .run_one(
                &server_socket,
                &mut receive_buffer,
                ProtocolVersion(2),
                &auth_config(Some("presented-secret")),
                &mut registry,
            )
            .expect("auth step should complete");
        assert!(!auth.auth_flow.decision.accepted);
        assert_eq!(registry.entries().count(), 0);

        client_socket
            .send_to(
                video_frame_packet("client-1", 42).as_slice(),
                server_address,
            )
            .expect("video frame should send");
        let second = run_controller_once_for_test(
            &server_socket,
            &mut receive_buffer,
            &mut registry,
            &auth_config(Some("presented-secret")),
        );
        let mut queue_state = ServerVideoFrameQueueState::default();
        let queue = queue_from_controller_result_for_test(&second, &mut queue_state);

        assert_eq!(queue_state.total_len(), 0);
        let ServerVideoFrameQueueRuntimeResult::NotQueued {
            reason:
                ServerVideoFrameQueueRuntimeSkipReason::RejectedVideoFrame(
                    ServerReceiveLoopGateRejection::Acceptance(rejection),
                ),
            ..
        } = queue
        else {
            panic!("video from rejected auth source should stay out of the queue");
        };
        assert_eq!(
            rejection.reason,
            PacketAcceptanceRejectReason::UnauthenticatedSource
        );
    }

    #[test]
    fn receive_auth_video_queue_once_unexpected_second_packet_is_not_queued() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, heartbeat_route("client-1", source))
            .expect("heartbeat should be accepted");
        let body = body_result_with_handler_handoff(
            72,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(registered),
        );
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(
                &body,
                &auth_config(Some("presented-secret")),
                body_dispatch_timing(),
            );
        let side_effect = ServerDispatchRuntimeSideEffectApplyBoundary::default()
            .apply_body_dispatch_outcome(&mut AuthenticatedSenderRegistry::default(), dispatch);
        let mut queue_state = ServerVideoFrameQueueState::default();

        let queue = ServerVideoFrameQueueRuntimeBoundary::default().store_from_receive_side_effect(
            &mut queue_state,
            &body,
            side_effect,
            TimestampMicros(3_000_100),
            ServerVideoFrameQueuePolicy::default(),
        );

        assert_eq!(queue_state.total_len(), 0);
        assert!(matches!(
            queue,
            ServerVideoFrameQueueRuntimeResult::NotQueued {
                reason: ServerVideoFrameQueueRuntimeSkipReason::NoAcceptedVideoFrame,
                ..
            }
        ));
    }

    #[test]
    fn video_stats_handler_runtime_prepares_stats_input_without_commit() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let route = client_stats_route("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, route)
            .expect("client stats should be accepted");
        let handler_outcome = ServerHandlerDispatchBoundary.dispatch_handoff(
            ServerContinuousReceiveLoopHandlerDispatchHandoff {
                packet_len: Some(96),
                plan: ServerContinuousReceiveLoopHandlerDispatchPlan::RegisteredClient(registered),
            },
        );
        let timing = ServerHeartbeatAckTiming {
            server_received_at: TimestampMicros(2_000_100),
            server_sent_at: TimestampMicros(2_000_200),
        };
        let registered_outcome = ServerRegisteredPacketDispatchRuntimeBoundary::default()
            .dispatch_outcome(handler_outcome, timing);
        let runtime = ServerVideoStatsHandlerRuntimeBoundary::default();

        let outcome = runtime.dispatch_outcome(registered_outcome);

        assert_eq!(outcome.packet_len, Some(96));
        let ServerVideoStatsHandlerRuntimeResult::ClientStats(input) = outcome.result else {
            panic!("expected client stats handler input");
        };
        assert_eq!(input.state.source, source);
        assert_eq!(input.state.client_id, ClientId("client-1".to_string()));
        assert_eq!(input.state.capture_fps, 30);
        assert_eq!(input.state.dropped_frames, 2);
        assert_eq!(input.state.bitrate_kbps, 4500);
        assert_eq!(input.heartbeat_observation, None);
    }

    #[test]
    fn body_dispatch_runtime_connects_body_result_to_auth_dispatch() {
        let source = packet_source();
        let body_result = body_result_with_handler_handoff(
            88,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Auth(ServerAuthCheck {
                source,
                request: auth_request("client-1", "presented-secret"),
            }),
        );
        let runtime = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default();

        let outcome = runtime.dispatch_body_result(
            &body_result,
            &auth_config(Some("presented-secret")),
            body_dispatch_timing(),
        );

        let ServerContinuousReceiveLoopBodyDispatchRuntimeResult::Auth(auth) = outcome.result
        else {
            panic!("expected auth dispatch result");
        };
        assert_eq!(auth.packet_len, Some(88));
        let ServerAuthDispatchRuntimeResult::Dispatched(flow) = auth.result else {
            panic!("expected auth flow dispatch");
        };
        assert!(flow.decision.accepted);
        assert_eq!(flow.decision.reason_code, AuthResponseReasonCode::Ok);
    }

    #[test]
    fn body_dispatch_runtime_connects_body_result_to_registered_heartbeat_dispatch() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, heartbeat_route("client-1", source))
            .expect("heartbeat should be accepted");
        let body_result = body_result_with_handler_handoff(
            72,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(registered),
        );
        let runtime = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default();

        let outcome = runtime.dispatch_body_result(
            &body_result,
            &auth_config(Some("presented-secret")),
            body_dispatch_timing(),
        );

        let ServerContinuousReceiveLoopBodyDispatchRuntimeResult::Registered(registered) =
            outcome.result
        else {
            panic!("expected registered dispatch result");
        };
        assert_eq!(registered.packet_len, Some(72));
        let ServerRegisteredPacketDispatchRuntimeResult::HeartbeatAck(handoff) = registered.result
        else {
            panic!("expected heartbeat ack handoff");
        };
        assert_eq!(handoff.ack_input.destination, source);
        assert_eq!(handoff.ack_input.echoed_sent_at, TimestampMicros(1_234_567));
    }

    #[test]
    fn body_dispatch_runtime_connects_body_result_to_video_stats_runtime() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, video_frame_route("client-1", source))
            .expect("video frame should be accepted");
        let body_result = body_result_with_handler_handoff(
            128,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(registered),
        );
        let runtime = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default();

        let outcome = runtime.dispatch_body_result(
            &body_result,
            &auth_config(Some("presented-secret")),
            body_dispatch_timing(),
        );

        let ServerContinuousReceiveLoopBodyDispatchRuntimeResult::VideoStats(video_stats) =
            outcome.result
        else {
            panic!("expected video stats dispatch result");
        };
        assert_eq!(video_stats.packet_len, Some(128));
        let ServerVideoStatsHandlerRuntimeResult::VideoFrame(input) = video_stats.result else {
            panic!("expected video input");
        };
        assert_eq!(input.payload_len, 3);
        assert_eq!(input.registered_packet.frame.frame_id, 42);
    }

    #[test]
    fn dispatch_side_effect_apply_registers_auth_sender_only() {
        let source = packet_source();
        let body_result = body_result_with_handler_handoff(
            88,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Auth(ServerAuthCheck {
                source,
                request: auth_request("client-1", "presented-secret"),
            }),
        );
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(
                &body_result,
                &auth_config(Some("presented-secret")),
                body_dispatch_timing(),
            );
        let mut registry = AuthenticatedSenderRegistry::default();
        let apply = ServerDispatchRuntimeSideEffectApplyBoundary::default();

        let outcome = apply.apply_body_dispatch_outcome(&mut registry, dispatch);

        let ServerDispatchRuntimeSideEffectApplyResult::Auth {
            flow,
            registered_sender,
        } = outcome.result
        else {
            panic!("expected auth side effect result");
        };
        assert!(flow.decision.accepted);
        let ProtocolMessage::AuthResponse(response) = flow.queue_item.packet.message else {
            panic!("expected queued AuthResponse");
        };
        assert!(response.accepted);
        let registered_sender = registered_sender.expect("sender should register");
        assert_eq!(
            registered_sender.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(registry.entries().count(), 1);
        assert_eq!(
            registry
                .get(&ClientId("client-1".to_string()))
                .expect("registry should contain client")
                .source,
            source
        );
    }

    #[test]
    fn dispatch_side_effect_apply_preserves_heartbeat_outbound_handoff() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, heartbeat_route("client-1", source))
            .expect("heartbeat should be accepted");
        let body_result = body_result_with_handler_handoff(
            72,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(registered),
        );
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(
                &body_result,
                &auth_config(Some("presented-secret")),
                body_dispatch_timing(),
            );
        let apply = ServerDispatchRuntimeSideEffectApplyBoundary::default();

        let outcome = apply.apply_body_dispatch_outcome(&mut registry, dispatch);

        let ServerDispatchRuntimeSideEffectApplyResult::HeartbeatAck(handoff) = outcome.result
        else {
            panic!("expected heartbeat ack side effect handoff");
        };
        assert_eq!(registry.entries().count(), 1);
        let ProtocolMessage::HeartbeatAck(ack) = handoff.queue_item.packet.message else {
            panic!("expected queued HeartbeatAck");
        };
        assert_eq!(ack.client_id, ClientId("client-1".to_string()));
        assert_eq!(ack.server_sent_at, TimestampMicros(2_000_200));
    }

    #[test]
    fn dispatch_side_effect_apply_preserves_stats_prepare_result_without_commit() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, client_stats_route("client-1", source))
            .expect("client stats should be accepted");
        let body_result = body_result_with_handler_handoff(
            96,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(registered),
        );
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(
                &body_result,
                &auth_config(Some("presented-secret")),
                body_dispatch_timing(),
            );
        let apply = ServerDispatchRuntimeSideEffectApplyBoundary::default();

        let outcome = apply.apply_body_dispatch_outcome(&mut registry, dispatch);

        let ServerDispatchRuntimeSideEffectApplyResult::ClientStats(input) = outcome.result else {
            panic!("expected stats prepare side effect handoff");
        };
        assert_eq!(registry.entries().count(), 1);
        assert_eq!(input.state.client_id, ClientId("client-1".to_string()));
        assert_eq!(input.state.capture_fps, 30);
        assert_eq!(input.state.dropped_frames, 2);
        assert_eq!(input.heartbeat_observation, None);
    }

    #[test]
    fn dispatch_output_apply_writes_auth_log_and_holds_accepted_auth_response() {
        let source = packet_source();
        let body_result = body_result_with_handler_handoff(
            88,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Auth(ServerAuthCheck {
                source,
                request: auth_request("client-1", "presented-secret"),
            }),
        );
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(
                &body_result,
                &auth_config(Some("presented-secret")),
                body_dispatch_timing(),
            );
        let mut registry = AuthenticatedSenderRegistry::default();
        let side_effect = ServerDispatchRuntimeSideEffectApplyBoundary::default()
            .apply_body_dispatch_outcome(&mut registry, dispatch);
        let output = ServerDispatchRuntimeOutputApplyBoundary::default();
        let mut auth_log = Vec::new();

        let outcome = output
            .apply_outputs(side_effect, 0, TimestampMicros(9_000_000), &mut auth_log)
            .expect("auth output apply should write log");

        let ServerDispatchRuntimeOutputApplyResult::Auth {
            flow,
            registered_sender,
            auth_log_event,
            auth_response_storage,
        } = outcome.result
        else {
            panic!("expected auth output result");
        };
        assert!(flow.decision.accepted);
        assert!(registered_sender.is_some());
        assert!(auth_log_event.accepted);
        assert_eq!(auth_log_event.reason_code, AuthResponseReasonCode::Ok);
        let storage = auth_response_storage.expect("accepted auth response should reach storage");
        assert!(storage.decision.accepts_candidate());
        let queued = storage.queued_item.expect("accepted item should be held");
        let ProtocolMessage::AuthResponse(response) = queued.item.packet.message else {
            panic!("expected queued AuthResponse");
        };
        assert!(response.accepted);
        let auth_log = String::from_utf8(auth_log).expect("auth log should be utf8");
        assert!(auth_log.contains(r#""event_name":"server.auth_result""#));
        assert!(auth_log.contains(r#""accepted":true"#));
    }

    #[test]
    fn dispatch_output_apply_writes_rejected_auth_log_without_queue_storage() {
        let source = packet_source();
        let body_result = body_result_with_handler_handoff(
            88,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::Auth(ServerAuthCheck {
                source,
                request: auth_request("client-1", "wrong-secret"),
            }),
        );
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(
                &body_result,
                &auth_config(Some("presented-secret")),
                body_dispatch_timing(),
            );
        let mut registry = AuthenticatedSenderRegistry::default();
        let side_effect = ServerDispatchRuntimeSideEffectApplyBoundary::default()
            .apply_body_dispatch_outcome(&mut registry, dispatch);
        let output = ServerDispatchRuntimeOutputApplyBoundary::default();
        let mut auth_log = Vec::new();

        let outcome = output
            .apply_outputs(side_effect, 0, TimestampMicros(9_000_000), &mut auth_log)
            .expect("auth output apply should write log");

        let ServerDispatchRuntimeOutputApplyResult::Auth {
            flow,
            registered_sender,
            auth_log_event,
            auth_response_storage,
        } = outcome.result
        else {
            panic!("expected auth output result");
        };
        assert!(!flow.decision.accepted);
        assert_eq!(registered_sender, None);
        assert_eq!(registry.entries().count(), 0);
        assert!(!auth_log_event.accepted);
        assert_eq!(
            auth_log_event.reason_code,
            AuthResponseReasonCode::InvalidToken
        );
        assert_eq!(auth_response_storage, None);
        let auth_log = String::from_utf8(auth_log).expect("auth log should be utf8");
        assert!(auth_log.contains(r#""accepted":false"#));
        assert!(auth_log.contains(r#""reason_code":"InvalidToken""#));
    }

    #[test]
    fn queue_collection_dequeues_accepted_auth_response_for_send_runtime() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        client_socket
            .send_to(
                auth_request_packet("client-1", "presented-secret").as_slice(),
                server_address,
            )
            .expect("auth request should send");
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut receive_buffer = vec![0_u8; 1024];
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();
        let body = ServerContinuousReceiveLoopBodyBoundary::default();

        let body_result = body
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &registry,
                ServerContinuousReceiveLoopBodyInput {
                    expected_protocol_version: ProtocolVersion(2),
                    timestamp: TimestampMicros(9_000_000),
                    stop_requested: false,
                },
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("body run once should receive auth request");
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(
                &body_result,
                &auth_config(Some("presented-secret")),
                body_dispatch_timing(),
            );
        let side_effect = ServerDispatchRuntimeSideEffectApplyBoundary::default()
            .apply_body_dispatch_outcome(&mut registry, dispatch);
        let mut auth_log = Vec::new();
        let output = ServerDispatchRuntimeOutputApplyBoundary::default()
            .apply_outputs(side_effect, 0, TimestampMicros(9_000_010), &mut auth_log)
            .expect("output apply should write auth log");
        let queue = ServerOutboundQueueCollectionBoundary;
        let mut collection = ServerOutboundQueueCollection::default();

        let push = queue.push_from_output_apply(&mut collection, &output);
        let dequeued = queue.dequeue_one(&mut collection);

        assert!(push.queued);
        assert_eq!(push.queue_len, 1);
        assert!(collection.is_empty());
        let ServerOutboundQueueDequeueRuntimeResult::Ready(queued) = dequeued else {
            panic!("expected queued auth response");
        };

        let send = ServerOutboundSendOneRuntimeBoundary::default()
            .send_queued(
                &server_socket,
                queued,
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
            )
            .expect("queued auth response should encode and send");

        assert_eq!(send.bytes_sent, send.encoded_packet.bytes.len());
        assert_eq!(send.final_state, OutboundQueueItemState::Sent);
        assert_eq!(send.encode_event.state, OutboundSendLoopTickState::Encoded);
        assert_eq!(
            send.send_event.state,
            OutboundSendLoopTickState::SocketSendSucceeded
        );

        let mut response_buffer = vec![0_u8; 1024];
        let (response_len, _source) = client_socket
            .recv_from(&mut response_buffer)
            .expect("auth response should receive from send runtime");
        let decoded = decode_fixed_header(&response_buffer[..response_len])
            .expect("auth response fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::AuthResponse);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));
        assert!(registry.get(&ClientId("client-1".to_string())).is_some());
        let auth_log = String::from_utf8(auth_log).expect("auth log should be utf8");
        assert!(auth_log.contains(r#""accepted":true"#));
    }

    #[test]
    fn receive_send_one_iteration_runtime_sends_accepted_auth_response() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        client_socket
            .send_to(
                auth_request_packet("client-1", "presented-secret").as_slice(),
                server_address,
            )
            .expect("auth request should send");
        let mut receive_buffer = vec![0_u8; 1024];
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();
        let mut auth_log = Vec::new();
        let mut send_log = Vec::new();
        let runtime = ServerReceiveSendOneIterationRuntimeBoundary::default();

        let outcome = runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerReceiveSendOneIterationRuntimeInput {
                    body: ServerContinuousReceiveLoopBodyInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_000),
                        stop_requested: false,
                    },
                    heartbeat_timing: body_dispatch_timing(),
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_010),
                    send_log_timestamp: TimestampMicros(9_000_020),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("one iteration receive/send runtime should complete");

        assert!(outcome.queue_push.queued);
        assert!(matches!(
            outcome.dequeue,
            ServerOutboundQueueDequeueRuntimeResult::Ready(_)
        ));
        let send = outcome
            .send
            .expect("accepted auth should send one response");
        let send_log_event = outcome
            .send_log
            .expect("accepted auth should write send log");
        assert_eq!(send_log_event.outcome, ServerSendLogOutcome::Success);
        assert_eq!(send_log_event.bytes_sent, Some(send.bytes_sent));
        assert_eq!(send.bytes_sent, send.encoded_packet.bytes.len());
        assert!(queue_collection.is_empty());
        assert!(registry.get(&ClientId("client-1".to_string())).is_some());

        let mut response_buffer = vec![0_u8; 1024];
        let (response_len, _source) = client_socket
            .recv_from(&mut response_buffer)
            .expect("auth response should receive from integrated runtime");
        let decoded = decode_fixed_header(&response_buffer[..response_len])
            .expect("auth response fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::AuthResponse);
        let auth_log = String::from_utf8(auth_log).expect("auth log should be utf8");
        assert!(auth_log.contains(r#""event_name":"server.auth_result""#));
        assert!(auth_log.contains(r#""accepted":true"#));
        let send_log = String::from_utf8(send_log).expect("send log should be utf8");
        assert!(send_log.contains(r#""event_name":"server.send""#));
        assert!(send_log.contains(r#""outcome":"Success""#));
        assert!(send_log.contains(r#""message_type":"AuthResponse""#));
        assert!(send_log.contains(r#""bytes_sent":"#));
    }

    #[test]
    fn receive_send_one_iteration_runtime_sends_registered_heartbeat_ack() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        let client_source: PacketSource = client_socket
            .local_addr()
            .expect("client socket should have address")
            .into();
        let packet = test_packet(
            MessageType::Heartbeat as u16,
            FIXED_HEADER_LEN,
            2,
            &heartbeat_payload(),
        );
        client_socket
            .send_to(packet.as_slice(), server_address)
            .expect("heartbeat should send");
        let mut receive_buffer = vec![0_u8; 1024];
        let mut registry = registry_with_client("client-1", client_source);
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();
        let mut auth_log = Vec::new();
        let mut send_log = Vec::new();
        let runtime = ServerReceiveSendOneIterationRuntimeBoundary::default();

        let outcome = runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerReceiveSendOneIterationRuntimeInput {
                    body: ServerContinuousReceiveLoopBodyInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_000),
                        stop_requested: false,
                    },
                    heartbeat_timing: ServerHeartbeatAckTiming {
                        server_received_at: TimestampMicros(9_000_100),
                        server_sent_at: TimestampMicros(9_000_200),
                    },
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_010),
                    send_log_timestamp: TimestampMicros(9_000_020),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("one iteration receive/send runtime should complete");

        assert!(outcome.queue_push.queued);
        let send = outcome
            .send
            .expect("registered heartbeat should send one ack");
        assert_eq!(send.bytes_sent, send.encoded_packet.bytes.len());
        assert!(queue_collection.is_empty());

        let mut response_buffer = vec![0_u8; 1024];
        let (response_len, _source) = client_socket
            .recv_from(&mut response_buffer)
            .expect("heartbeat ack should receive from integrated runtime");
        let decoded = decode_fixed_header(&response_buffer[..response_len])
            .expect("heartbeat ack fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::HeartbeatAck);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));
        let send_log = String::from_utf8(send_log).expect("send log should be utf8");
        assert!(send_log.contains(r#""message_type":"HeartbeatAck""#));
        assert!(auth_log.is_empty());
    }

    #[test]
    fn controller_receive_send_runtime_stops_without_iteration() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let mut receive_buffer = vec![0_u8; 1024];
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();
        let mut auth_log = Vec::new();
        let mut send_log = Vec::new();
        let runtime = ServerControllerReceiveSendRuntimeBoundary::default();

        let outcome = runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_000),
                        continue_requested: false,
                    },
                    heartbeat_timing: body_dispatch_timing(),
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_010),
                    send_log_timestamp: TimestampMicros(9_000_020),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("stopped controller should not run iteration");

        assert_eq!(
            outcome,
            ServerControllerReceiveSendRuntimeResult::Stopped {
                plan: ServerContinuousReceiveLoopControllerPlan {
                    state: ServerContinuousReceiveLoopControllerState::Stopped,
                    action: ServerContinuousReceiveLoopControllerAction::Stop,
                    body_input: None,
                },
            }
        );
        assert_eq!(registry.entries().count(), 0);
        assert!(queue_collection.is_empty());
        assert!(operational_output.is_empty());
        assert!(rejection_output.is_empty());
        assert!(auth_log.is_empty());
        assert!(send_log.is_empty());
    }

    #[test]
    fn controller_receive_send_runtime_runs_one_auth_iteration() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        client_socket
            .send_to(
                auth_request_packet("client-1", "presented-secret").as_slice(),
                server_address,
            )
            .expect("auth request should send");
        let mut receive_buffer = vec![0_u8; 1024];
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();
        let mut auth_log = Vec::new();
        let mut send_log = Vec::new();
        let runtime = ServerControllerReceiveSendRuntimeBoundary::default();

        let outcome = runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_000),
                        continue_requested: true,
                    },
                    heartbeat_timing: body_dispatch_timing(),
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_010),
                    send_log_timestamp: TimestampMicros(9_000_020),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("controller receive/send runtime should complete");

        let ServerControllerReceiveSendRuntimeResult::Iteration {
            plan,
            iteration,
            observation,
        } = outcome
        else {
            panic!("expected one controller iteration");
        };
        assert_eq!(
            plan.action,
            ServerContinuousReceiveLoopControllerAction::RunBodyOnce
        );
        assert_eq!(
            observation,
            ServerContinuousReceiveLoopControllerObservation {
                state: ServerContinuousReceiveLoopControllerState::BodyIterationCompleted,
                action: ServerContinuousReceiveLoopControllerAction::YieldToCaller,
            }
        );
        assert!(iteration.send.is_some());
        assert!(registry.get(&ClientId("client-1".to_string())).is_some());
        assert!(queue_collection.is_empty());

        let mut response_buffer = vec![0_u8; 1024];
        let (response_len, _source) = client_socket
            .recv_from(&mut response_buffer)
            .expect("auth response should receive from controller runtime");
        let decoded = decode_fixed_header(&response_buffer[..response_len])
            .expect("auth response fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::AuthResponse);
        let auth_log = String::from_utf8(auth_log).expect("auth log should be utf8");
        assert!(auth_log.contains(r#""accepted":true"#));
        let send_log = String::from_utf8(send_log).expect("send log should be utf8");
        assert!(send_log.contains(r#""event_name":"server.send""#));
        assert!(send_log.contains(r#""outcome":"Success""#));
    }

    #[test]
    fn controller_receive_send_runtime_can_run_auth_then_heartbeat_iterations() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        client_socket
            .send_to(
                auth_request_packet("client-1", "presented-secret").as_slice(),
                server_address,
            )
            .expect("auth request should send");
        let mut receive_buffer = vec![0_u8; 1024];
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();
        let mut auth_log = Vec::new();
        let mut send_log = Vec::new();
        let runtime = ServerControllerReceiveSendRuntimeBoundary::default();

        let first = runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_000),
                        continue_requested: true,
                    },
                    heartbeat_timing: body_dispatch_timing(),
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_010),
                    send_log_timestamp: TimestampMicros(9_000_020),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("auth iteration should complete");

        assert!(matches!(
            first,
            ServerControllerReceiveSendRuntimeResult::Iteration { .. }
        ));
        assert!(registry.get(&ClientId("client-1".to_string())).is_some());
        let mut response_buffer = vec![0_u8; 1024];
        let (auth_response_len, _source) = client_socket
            .recv_from(&mut response_buffer)
            .expect("auth response should receive");
        let decoded_auth_response = decode_fixed_header(&response_buffer[..auth_response_len])
            .expect("auth response fixed header should decode");
        assert_eq!(
            decoded_auth_response.header.message_type,
            MessageType::AuthResponse
        );

        let heartbeat = test_packet(
            MessageType::Heartbeat as u16,
            FIXED_HEADER_LEN,
            2,
            &heartbeat_payload(),
        );
        client_socket
            .send_to(heartbeat.as_slice(), server_address)
            .expect("heartbeat should send");

        let second = runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_100),
                        continue_requested: true,
                    },
                    heartbeat_timing: ServerHeartbeatAckTiming {
                        server_received_at: TimestampMicros(9_000_110),
                        server_sent_at: TimestampMicros(9_000_120),
                    },
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_130),
                    send_log_timestamp: TimestampMicros(9_000_140),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("heartbeat iteration should complete");

        assert!(matches!(
            second,
            ServerControllerReceiveSendRuntimeResult::Iteration { .. }
        ));
        let (heartbeat_ack_len, _source) = client_socket
            .recv_from(&mut response_buffer)
            .expect("heartbeat ack should receive");
        let decoded_heartbeat_ack = decode_fixed_header(&response_buffer[..heartbeat_ack_len])
            .expect("heartbeat ack fixed header should decode");
        assert_eq!(
            decoded_heartbeat_ack.header.message_type,
            MessageType::HeartbeatAck
        );
        let send_log = String::from_utf8(send_log).expect("send log should be utf8");
        assert!(send_log.contains(r#""message_type":"AuthResponse""#));
        assert!(send_log.contains(r#""message_type":"HeartbeatAck""#));
    }

    #[test]
    fn controller_receive_send_runtime_can_return_heartbeat_observation_via_client_stats() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let server_address = server_socket
            .local_addr()
            .expect("server socket should have address");
        let mut receive_buffer = vec![0_u8; 2048];
        let mut response_buffer = vec![0_u8; 2048];
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut queue_collection = ServerOutboundQueueCollection::default();
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();
        let mut auth_log = Vec::new();
        let mut send_log = Vec::new();
        let runtime = ServerControllerReceiveSendRuntimeBoundary::default();

        client_socket
            .send_to(
                auth_request_packet("client-1", "presented-secret").as_slice(),
                server_address,
            )
            .expect("auth request should send");
        runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_000),
                        continue_requested: true,
                    },
                    heartbeat_timing: body_dispatch_timing(),
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_010),
                    send_log_timestamp: TimestampMicros(9_000_020),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("auth iteration should complete");
        client_socket
            .recv_from(&mut response_buffer)
            .expect("auth response should receive");

        let heartbeat = test_packet(
            MessageType::Heartbeat as u16,
            FIXED_HEADER_LEN,
            2,
            &heartbeat_payload(),
        );
        client_socket
            .send_to(heartbeat.as_slice(), server_address)
            .expect("heartbeat should send");
        let second = runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_100),
                        continue_requested: true,
                    },
                    heartbeat_timing: ServerHeartbeatAckTiming {
                        server_received_at: TimestampMicros(2_000_100),
                        server_sent_at: TimestampMicros(2_000_120),
                    },
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_130),
                    send_log_timestamp: TimestampMicros(9_000_140),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("heartbeat iteration should complete");
        let (ack_len, _source) = client_socket
            .recv_from(&mut response_buffer)
            .expect("heartbeat ack should receive");
        let decoded_ack = decode_fixed_header(&response_buffer[..ack_len])
            .expect("heartbeat ack fixed header should decode");
        let ack = decode_heartbeat_ack_payload(decoded_ack.header, decoded_ack.payload)
            .expect("heartbeat ack should decode");

        let observation = HeartbeatAckObservation {
            client_id: ack.client_id.clone(),
            run_id: ack.run_id.clone(),
            echoed_sent_at: ack.echoed_sent_at,
            server_received_at: ack.server_received_at,
            server_sent_at: ack.server_sent_at,
            client_received_at: TimestampMicros(1_234_667),
        };
        let stats = ClientStats {
            message_type: MessageType::ClientStats,
            protocol_version: ProtocolVersion(2),
            client_id: ack.client_id.clone(),
            run_id: ack.run_id.clone(),
            sent_at: TimestampMicros(1_234_668),
            capture_fps: 0,
            dropped_frames: 0,
            bitrate_kbps: 0,
            heartbeat_observation: Some(observation),
        };
        let mut stats_bytes = Vec::new();
        ProtocolMessageEncoderBoundary
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &ProtocolMessage::ClientStats(stats),
                &mut stats_bytes,
            )
            .expect("client stats should encode");
        client_socket
            .send_to(stats_bytes.as_slice(), server_address)
            .expect("client stats should send");

        let third = runtime
            .run_once(
                &server_socket,
                &mut receive_buffer,
                &mut registry,
                &mut queue_collection,
                &auth_config(Some("presented-secret")),
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(9_000_200),
                        continue_requested: true,
                    },
                    heartbeat_timing: body_dispatch_timing(),
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(9_000_210),
                    send_log_timestamp: TimestampMicros(9_000_220),
                },
                &mut operational_output,
                &mut rejection_output,
                &mut auth_log,
                &mut send_log,
            )
            .expect("client stats iteration should complete");

        let heartbeat_handoff = heartbeat_handoff_from_controller_result(&second)
            .expect("second iteration should preserve heartbeat handoff");
        let client_stats = client_stats_input_from_controller_result(&third)
            .expect("third iteration should preserve client stats input");
        let calculation = ServerHeartbeatObservationReturnBoundary::default()
            .calculate_from_client_stats(heartbeat_handoff, client_stats)
            .expect("observation return should calculate");

        assert_eq!(calculation.client_id, ClientId("client-1".to_string()));
        assert_eq!(calculation.estimate.rtt_micros, 80);
        assert_eq!(calculation.estimate.server_processing_micros, 20);
    }

    #[test]
    fn continuous_receive_loop_body_executes_one_tick_runtime() {
        let boundary = ServerContinuousReceiveLoopBodyBoundary::default();
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("read timeout should be set");
        let receiver_addr = receiver.local_addr().expect("receiver should have address");
        let sender_source: PacketSource = sender
            .local_addr()
            .expect("sender should have address")
            .into();
        let payload = heartbeat_payload();
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let registry = registry_with_client("client-1", sender_source);
        let mut buffer = [0_u8; 256];
        let mut operational_output = Vec::new();
        let mut rejection_output = Vec::new();

        sender
            .send_to(packet.as_slice(), receiver_addr)
            .expect("packet should send");

        let result = boundary
            .run_once(
                &receiver,
                &mut buffer,
                &registry,
                ServerContinuousReceiveLoopBodyInput {
                    expected_protocol_version: ProtocolVersion(2),
                    timestamp: TimestampMicros(989_012),
                    stop_requested: false,
                },
                &mut operational_output,
                &mut rejection_output,
            )
            .expect("loop body should execute one tick");

        assert_eq!(
            result.action,
            ServerContinuousReceiveLoopBodyAction::ExecuteOneTick
        );
        let controller = ServerContinuousReceiveLoopControllerBoundary;
        assert_eq!(
            controller.observe_body_result(&result),
            ServerContinuousReceiveLoopControllerObservation {
                state: ServerContinuousReceiveLoopControllerState::BodyIterationCompleted,
                action: ServerContinuousReceiveLoopControllerAction::YieldToCaller,
            }
        );
        let dispatch =
            ServerContinuousReceiveLoopHandlerDispatchBoundary.plan_from_body_result(&result);
        assert_eq!(dispatch.packet_len, Some(packet.len()));
        assert!(matches!(
            dispatch.plan,
            ServerContinuousReceiveLoopHandlerDispatchPlan::RegisteredClient(
                ServerRegisteredClientPacket::Heartbeat(ServerRegisteredHeartbeatPacket {
                    heartbeat: Heartbeat { ref client_id, .. },
                    ..
                })
            ) if client_id == &ClientId("client-1".to_string())
        ));
        let ServerContinuousReceiveLoopOneTickRuntimeOutcome::Completed {
            packet_len,
            handler,
        } = result.tick.outcome
        else {
            panic!("expected completed one tick outcome");
        };
        assert_eq!(packet_len, packet.len());
        assert!(matches!(
            handler.handler,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(
                ServerRegisteredClientPacket::Heartbeat(ServerRegisteredHeartbeatPacket {
                    heartbeat: Heartbeat { ref client_id, .. },
                    ..
                })
            ) if client_id == &ClientId("client-1".to_string())
        ));
        assert!(String::from_utf8(operational_output)
            .expect("json line should be utf8")
            .contains(r#""event_name":"server.receive_loop""#));
        assert!(rejection_output.is_empty());
    }

    #[test]
    fn receive_loop_log_handoff_records_accepted_packet() {
        let source = packet_source();
        let boundary = ServerReceiveLoopLogHandoffBoundary;
        let outcome = ServerReceiveLoopGateOutcome::Accepted(heartbeat_route("client-1", source));

        let input = boundary.handoff(&outcome, 128);

        assert_eq!(
            input,
            ServerReceiveLoopLogInput {
                source,
                outcome: ServerReceiveLoopLogOutcome::Accepted,
                packet_len: 128,
                message_type: Some(MessageType::Heartbeat),
                client_id: Some(ClientId("client-1".to_string())),
                rejection_reason: None,
            }
        );
    }

    #[test]
    fn receive_loop_log_handoff_records_decode_rejection() {
        let source = packet_source();
        let boundary = ServerReceiveLoopLogHandoffBoundary;
        let outcome = ServerReceiveLoopGateOutcome::Rejected(
            ServerReceiveLoopGateRejection::Decode(ServerRejectedPacket {
                source,
                action: ServerDecodeErrorAction::DropPacket,
                error: ProtocolError::BufferTooShort,
            }),
        );

        let input = boundary.handoff(&outcome, 4);

        assert_eq!(
            input,
            ServerReceiveLoopLogInput {
                source,
                outcome: ServerReceiveLoopLogOutcome::DecodeRejected,
                packet_len: 4,
                message_type: None,
                client_id: None,
                rejection_reason: Some(ServerReceiveRejectionReason::DecodeError),
            }
        );
    }

    #[test]
    fn receive_loop_json_line_writer_outputs_minimal_json_line() {
        let source = packet_source();
        let writer = ServerReceiveLoopJsonLineWriter;
        let event = ServerReceiveLoopJsonLogEventInput {
            event_name: SERVER_RECEIVE_LOOP_JSON_LOG_EVENT_NAME,
            source,
            outcome: ServerReceiveLoopLogOutcome::AcceptanceRejected,
            packet_len: 96,
            message_type: Some(MessageType::VideoFrame),
            client_id: Some(ClientId("client-1".to_string())),
            rejection_reason: Some(ServerReceiveRejectionReason::EndpointMismatch),
            timestamp: TimestampMicros(567_890),
        };
        let mut output = Vec::new();

        writer
            .write_event(&event, &mut output)
            .expect("json line should write");

        assert_eq!(
            String::from_utf8(output).expect("json line should be utf8"),
            r#"{"event_name":"server.receive_loop","source":"127.0.0.1:5000","outcome":"AcceptanceRejected","packet_len":96,"message_type":"VideoFrame","client_id":"client-1","rejection_reason":"EndpointMismatch","timestamp":567890}"#
                .to_string()
                + "\n"
        );
    }

    #[test]
    fn receive_loop_log_output_boundary_connects_handoff_event_and_writer() {
        let source = packet_source();
        let boundary = ServerReceiveLoopLogOutputBoundary::default();
        let outcome = ServerReceiveLoopGateOutcome::Rejected(
            ServerReceiveLoopGateRejection::Acceptance(PacketAcceptanceRejection {
                source,
                message_type: MessageType::Heartbeat,
                client_id: Some(ClientId("client-2".to_string())),
                reason: PacketAcceptanceRejectReason::UnknownClient,
            }),
        );
        let mut output = Vec::new();

        let event = boundary
            .write_receive_loop_event(&outcome, 72, TimestampMicros(678_901), &mut output)
            .expect("json line should write");

        assert_eq!(
            event.outcome,
            ServerReceiveLoopLogOutcome::AcceptanceRejected
        );
        assert_eq!(
            event.rejection_reason,
            Some(ServerReceiveRejectionReason::UnknownClient)
        );
        let output = String::from_utf8(output).expect("json line should be utf8");
        assert!(output.contains(r#""event_name":"server.receive_loop""#));
        assert!(output.contains(r#""outcome":"AcceptanceRejected""#));
        assert!(output.contains(r#""message_type":"Heartbeat""#));
        assert!(output.contains(r#""rejection_reason":"UnknownClient""#));
        assert!(output.contains(r#""timestamp":678901"#));
    }

    #[test]
    fn send_json_line_writer_outputs_success_observation() {
        let event = ServerSendJsonLogEventInput {
            event_name: SERVER_SEND_EVENT_NAME,
            outcome: ServerSendLogOutcome::Success,
            run_id: Some(RunId("run-1".to_string())),
            client_id: Some(ClientId("client-1".to_string())),
            destination: packet_source().into(),
            message_type: MessageType::AuthResponse,
            stage: SendLogStage::SocketSend,
            encoded_len: Some(55),
            bytes_sent: Some(55),
            failure: None,
            disposition: None,
            timestamp: TimestampMicros(789_012),
        };
        let writer = ServerSendJsonLineWriter;
        let mut output = Vec::new();

        writer
            .write_event(&event, &mut output)
            .expect("send json line should write");

        assert_eq!(
            String::from_utf8(output).expect("json line should be utf8"),
            r#"{"event_name":"server.send","outcome":"Success","run_id":"run-1","client_id":"client-1","destination":"127.0.0.1:5000","message_type":"AuthResponse","stage":"SocketSend","encoded_len":55,"bytes_sent":55,"failure":null,"disposition":null,"timestamp":789012}"#
                .to_string()
                + "\n"
        );
    }

    #[test]
    fn send_log_output_boundary_writes_failure_observation() {
        let boundary = ServerSendLogOutputBoundary::default();
        let error = ServerOutboundSendOneRuntimeError::SocketSend {
            error_kind: io::ErrorKind::NetworkUnreachable,
            event: OutboundSendLoopEvent {
                state: OutboundSendLoopTickState::Failed,
                log_event: Some(SendLogEvent::send_failed(
                    send_log_context(),
                    SendLogStage::SocketSend,
                    Some(55),
                    SendFailureKind::NetworkUnreachable,
                )),
            },
        };
        let mut output = Vec::new();

        let event = boundary
            .write_send_failure(&error, TimestampMicros(890_123), &mut output)
            .expect("send failure log should write")
            .expect("failure event should be produced");

        assert_eq!(event.outcome, ServerSendLogOutcome::Failure);
        assert_eq!(event.failure, Some(SendFailureKind::NetworkUnreachable));
        let output = String::from_utf8(output).expect("json line should be utf8");
        assert!(output.contains(r#""event_name":"server.send""#));
        assert!(output.contains(r#""outcome":"Failure""#));
        assert!(output.contains(r#""failure":"NetworkUnreachable""#));
        assert!(output.contains(r#""disposition":"WarningCandidate""#));
        assert!(output.contains(r#""timestamp":890123"#));
    }

    #[test]
    fn send_error_log_handoff_ignores_success_events() {
        let boundary = ServerSendErrorLogHandoffBoundary;
        let event = SendLogEvent::encode_succeeded(send_log_context(), 64);

        let input = boundary.handoff(event);

        assert_eq!(input, None);
    }

    #[test]
    fn send_error_json_log_event_boundary_preserves_failure_context() {
        let destination: PacketDestination = packet_source().into();
        let boundary = ServerSendErrorJsonLogEventBoundary;
        let input = ServerSendErrorLogInput {
            context: OutboundSendLogContext {
                destination,
                message_type: MessageType::HeartbeatAck,
                run_id: Some(RunId("run-1".to_string())),
                client_id: Some(ClientId("client-1".to_string())),
            },
            stage: SendLogStage::SocketSend,
            encoded_len: Some(48),
            failure: SendFailureKind::SocketWouldBlock,
            disposition: SendFailureDisposition::RetryCandidate,
        };

        let event = boundary.build_event(input, TimestampMicros(567_890));

        assert_eq!(
            event,
            ServerSendErrorJsonLogEventInput {
                event_name: SERVER_SEND_ERROR_EVENT_NAME,
                run_id: Some(RunId("run-1".to_string())),
                client_id: Some(ClientId("client-1".to_string())),
                destination,
                message_type: MessageType::HeartbeatAck,
                stage: SendLogStage::SocketSend,
                encoded_len: Some(48),
                failure: SendFailureKind::SocketWouldBlock,
                disposition: SendFailureDisposition::RetryCandidate,
                timestamp: TimestampMicros(567_890),
            }
        );
    }

    #[test]
    fn send_error_json_line_writer_outputs_minimal_json_line() {
        let destination: PacketDestination = packet_source().into();
        let writer = ServerSendErrorJsonLineWriter;
        let event = ServerSendErrorJsonLogEventInput {
            event_name: SERVER_SEND_ERROR_EVENT_NAME,
            run_id: Some(RunId("run-1".to_string())),
            client_id: Some(ClientId("client-1".to_string())),
            destination,
            message_type: MessageType::HeartbeatAck,
            stage: SendLogStage::SocketSend,
            encoded_len: Some(48),
            failure: SendFailureKind::SocketWouldBlock,
            disposition: SendFailureDisposition::RetryCandidate,
            timestamp: TimestampMicros(678_901),
        };
        let mut output = Vec::new();

        writer
            .write_event(&event, &mut output)
            .expect("json line should write");

        assert_eq!(
            String::from_utf8(output).expect("json line should be utf8"),
            r#"{"event_name":"server.send_error","run_id":"run-1","client_id":"client-1","destination":"127.0.0.1:5000","message_type":"HeartbeatAck","stage":"SocketSend","encoded_len":48,"failure":"SocketWouldBlock","disposition":"RetryCandidate","timestamp":678901}"#
                .to_string()
                + "\n"
        );
    }

    #[test]
    fn send_error_log_output_boundary_connects_handoff_event_and_writer() {
        let boundary = ServerSendErrorLogOutputBoundary::default();
        let send_event = SendLogEvent::send_failed(
            send_log_context(),
            SendLogStage::SocketSend,
            Some(52),
            SendFailureKind::NetworkUnreachable,
        );
        let mut output = Vec::new();

        let event = boundary
            .write_send_error(send_event, TimestampMicros(789_012), &mut output)
            .expect("json line should write")
            .expect("failure event should produce json input");

        assert_eq!(event.failure, SendFailureKind::NetworkUnreachable);
        assert_eq!(event.disposition, SendFailureDisposition::WarningCandidate);
        let output = String::from_utf8(output).expect("json line should be utf8");
        assert!(output.contains(r#""event_name":"server.send_error""#));
        assert!(output.contains(r#""stage":"SocketSend""#));
        assert!(output.contains(r#""failure":"NetworkUnreachable""#));
        assert!(output.contains(r#""disposition":"WarningCandidate""#));
        assert!(output.contains(r#""timestamp":789012"#));
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
    fn auth_decision_rejects_secret_store_reference_until_resolved() {
        let mut input = auth_check_input("client-1", "presented-secret", Some("presented-secret"));
        input.shared_tokens[0].secret_ref =
            SharedTokenSecretRef::SecretStore(test_secret_store_reference("stream-sync/player1"));
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
    fn secret_resolver_boundary_plans_secret_store_as_future_lookup() {
        let boundary = ServerSecretResolverBoundary;
        let reference = test_secret_store_reference("stream-sync/player1");
        let token = ServerSharedTokenAuthInput {
            token_id: "client-1".to_string(),
            secret_ref: SharedTokenSecretRef::SecretStore(reference.clone()),
        };

        let plan = boundary.plan_resolution(&token);

        assert_eq!(
            plan,
            ServerSecretResolutionPlan::NeedsSecretStore {
                token_id: "client-1".to_string(),
                reference,
            }
        );
    }

    #[test]
    fn secret_resolver_boundary_rejects_secret_store_until_implemented() {
        let boundary = ServerSecretResolverBoundary;
        let reference = test_secret_store_reference("stream-sync/player1");
        let token = ServerSharedTokenAuthInput {
            token_id: "client-1".to_string(),
            secret_ref: SharedTokenSecretRef::SecretStore(reference.clone()),
        };

        let resolved = boundary.resolve_token(&token);

        assert_eq!(
            resolved,
            Err(ServerSecretResolutionError::UnsupportedSecretStore {
                token_id: "client-1".to_string(),
                reference,
            })
        );
    }

    #[test]
    fn shared_token_rotation_boundary_keeps_mvp_disabled() {
        let boundary = ServerSharedTokenRotationBoundary;

        let plan = boundary.plan(SharedTokenRotationConfig::disabled_for_mvp());

        assert_eq!(plan, ServerSharedTokenRotationPlan::DisabledForMvp);
    }

    #[test]
    fn shared_token_rotation_boundary_records_future_overlap_window() {
        let boundary = ServerSharedTokenRotationBoundary;

        let plan = boundary.plan(SharedTokenRotationConfig::manual_overlap_placeholder(900));

        assert_eq!(
            plan,
            ServerSharedTokenRotationPlan::ManualOverlapPlaceholder {
                overlap_window_seconds: 900
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
    fn server_notice_boundary_builds_notice_for_send() {
        let source = packet_source();
        let boundary = ServerNoticeBoundary;
        let input = ServerNoticeInput {
            destination: source,
            protocol_version: ProtocolVersion(2),
            run_id: RunId("run-1".to_string()),
            notice_type: NoticeType::ProtocolError,
            message: "unsupported protocol_version".to_string(),
        };

        let outbound = boundary.build_for_send(input);

        assert_eq!(outbound.destination, source);
        let notice = outbound
            .server_notice()
            .expect("outbound message should be ServerNotice");
        assert_eq!(notice.message_type, MessageType::ServerNotice);
        assert_eq!(notice.protocol_version, ProtocolVersion(2));
        assert_eq!(notice.run_id, RunId("run-1".to_string()));
        assert_eq!(notice.notice_type, NoticeType::ProtocolError);
        assert_eq!(notice.message, "unsupported protocol_version");
    }

    #[test]
    fn server_notice_trigger_policy_maps_protocol_error_to_notice_plan() {
        let source = packet_source();
        let boundary = ServerNoticeTriggerPolicyBoundary;

        let plan = boundary.plan_notice(ServerNoticeTriggerInput {
            destination: source,
            protocol_version: ProtocolVersion(2),
            run_id: RunId("run-1".to_string()),
            source: ServerNoticeTriggerSource::ProtocolError,
            message: "unsupported protocol_version".to_string(),
        });

        assert_eq!(plan.source, ServerNoticeTriggerSource::ProtocolError);
        assert_eq!(plan.notice.destination, source);
        assert_eq!(plan.notice.protocol_version, ProtocolVersion(2));
        assert_eq!(plan.notice.run_id, RunId("run-1".to_string()));
        assert_eq!(plan.notice.notice_type, NoticeType::ProtocolError);
        assert_eq!(plan.notice.message, "unsupported protocol_version");
    }

    #[test]
    fn server_notice_trigger_plan_can_feed_notice_boundary_without_sending() {
        let source = packet_source();
        let trigger_boundary = ServerNoticeTriggerPolicyBoundary;
        let notice_boundary = ServerNoticeBoundary;
        let plan = trigger_boundary.plan_notice(ServerNoticeTriggerInput {
            destination: source,
            protocol_version: ProtocolVersion(2),
            run_id: RunId("run-1".to_string()),
            source: ServerNoticeTriggerSource::ServerShutdown,
            message: "server is shutting down".to_string(),
        });

        let outbound = notice_boundary.build_for_send(plan.into_notice_input());

        assert_eq!(outbound.destination, source);
        let notice = outbound
            .server_notice()
            .expect("outbound message should be ServerNotice");
        assert_eq!(notice.notice_type, NoticeType::ServerShutdown);
        assert_eq!(notice.message, "server is shutting down");
    }

    #[test]
    fn outbound_queue_boundary_hands_off_server_notice_to_net_send_layer() {
        let source = packet_source();
        let notice_boundary = ServerNoticeBoundary;
        let queue_boundary = ServerOutboundQueueBoundary::default();
        let outbound = notice_boundary.build_for_send(ServerNoticeInput {
            destination: source,
            protocol_version: ProtocolVersion(2),
            run_id: RunId("run-1".to_string()),
            notice_type: NoticeType::Warning,
            message: "server warning".to_string(),
        });

        let queue_item = queue_boundary.handoff_notice(outbound);

        assert_eq!(queue_item.packet.destination.address, source.address);
        let ProtocolMessage::ServerNotice(notice) = queue_item.packet.message else {
            panic!("expected ServerNotice outbound message");
        };
        assert_eq!(notice.message_type, MessageType::ServerNotice);
        assert_eq!(notice.notice_type, NoticeType::Warning);
        assert_eq!(notice.message, "server warning");
    }

    #[test]
    fn outbound_queue_boundary_exposes_capacity_policy_for_handoff_items() {
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
        let policy = queue_boundary.capacity_policy();

        let decision = queue_boundary.evaluate_admission(policy.max_items, &queue_item);

        assert_eq!(
            decision,
            OutboundQueueAdmissionDecision::DropIncoming {
                item_class: OutboundQueueItemClass::Control,
                reason: OutboundQueueDropReason::CapacityReached
            }
        );
    }

    #[test]
    fn outbound_queue_boundary_exposes_storage_push_plan_before_send_loop() {
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

        let decision = queue_boundary.evaluate_storage_push(0, &queue_item);

        assert_eq!(
            decision.state_before,
            OutboundQueueStorageState {
                len: 0,
                capacity: queue_boundary.capacity_policy().max_items,
            }
        );
        assert!(decision.accepts_candidate());
        assert_eq!(decision.planned_len_after(), 1);
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
                local_time: Some(TimestampMicros(1_999_900)),
                short_status: Some("ready".to_string()),
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
        assert_eq!(
            handoff.processing_inputs.state.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(
            handoff.processing_inputs.state.heartbeat_sent_at,
            TimestampMicros(2_000_000)
        );
        assert_eq!(
            handoff.processing_inputs.state.short_status.as_deref(),
            Some("ready")
        );
        assert_eq!(
            handoff.processing_inputs.timebase.client_sent_at,
            TimestampMicros(2_000_000)
        );
        assert_eq!(
            handoff.processing_inputs.timebase.client_local_time,
            Some(TimestampMicros(1_999_900))
        );
        assert_eq!(
            handoff.processing_inputs.timebase.server_received_at,
            TimestampMicros(2_000_100)
        );
        assert_eq!(
            handoff
                .processing_inputs
                .timebase_plan
                .estimate
                .sample
                .client_sent_at_micros,
            2_000_000
        );
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
    fn heartbeat_input_boundary_prepares_state_and_timebase_inputs() {
        let source = packet_source();
        let packet = ServerRegisteredHeartbeatPacket {
            source,
            authenticated_sender: AuthenticatedSenderEntry {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            },
            heartbeat: Heartbeat {
                message_type: MessageType::Heartbeat,
                protocol_version: ProtocolVersion(2),
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                sent_at: TimestampMicros(3_000_000),
                local_time: Some(TimestampMicros(2_999_900)),
                short_status: Some("ok".to_string()),
            },
        };
        let timing = ServerHeartbeatAckTiming {
            server_received_at: TimestampMicros(3_000_100),
            server_sent_at: TimestampMicros(3_000_200),
        };
        let boundary = ServerHeartbeatInputBoundary;

        let inputs = boundary.prepare_inputs(&packet, timing);

        assert_eq!(inputs.ack_timing, timing);
        assert_eq!(inputs.state.source, source);
        assert_eq!(
            inputs.state.authenticated_sender,
            packet.authenticated_sender
        );
        assert_eq!(inputs.state.heartbeat_sent_at, TimestampMicros(3_000_000));
        assert_eq!(inputs.state.server_received_at, TimestampMicros(3_000_100));
        assert_eq!(inputs.state.short_status.as_deref(), Some("ok"));
        assert_eq!(inputs.timebase.client_sent_at, TimestampMicros(3_000_000));
        assert_eq!(
            inputs.timebase.client_local_time,
            Some(TimestampMicros(2_999_900))
        );
        assert_eq!(
            inputs.timebase.server_received_at,
            TimestampMicros(3_000_100)
        );
        assert_eq!(inputs.timebase.server_sent_at, TimestampMicros(3_000_200));
        assert_eq!(
            inputs.timebase_plan.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(inputs.timebase_plan.run_id, RunId("run-1".to_string()));
        assert_eq!(
            inputs
                .timebase_plan
                .estimate
                .sample
                .client_local_time_micros,
            Some(2_999_900)
        );
    }

    #[test]
    fn heartbeat_liveness_commit_boundary_commits_first_heartbeat() {
        let source = packet_source();
        let input = ServerHeartbeatStateInput {
            source,
            authenticated_sender: AuthenticatedSenderEntry {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: Some(TimestampMicros(900_000)),
            },
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            heartbeat_sent_at: TimestampMicros(1_000_000),
            server_received_at: TimestampMicros(1_000_100),
            short_status: Some("ready".to_string()),
        };
        let boundary = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();

        let outcome = boundary.commit(&mut state, input);

        assert_eq!(state.len(), 1);
        assert_eq!(outcome.previous, None);
        assert_eq!(outcome.committed.source, source);
        assert_eq!(
            outcome.committed.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(
            outcome.committed.last_heartbeat_sent_at,
            TimestampMicros(1_000_000)
        );
        assert_eq!(
            outcome.committed.last_server_received_at,
            TimestampMicros(1_000_100)
        );
        assert_eq!(
            outcome.committed.last_short_status.as_deref(),
            Some("ready")
        );
        assert_eq!(outcome.committed.received_heartbeats, 1);
        assert_eq!(
            outcome.committed.status,
            ServerHeartbeatLivenessStatus::Alive
        );
        assert_eq!(
            state
                .get(&ClientId("client-1".to_string()))
                .expect("liveness entry should be committed"),
            &outcome.committed
        );
    }

    #[test]
    fn heartbeat_liveness_commit_boundary_updates_existing_heartbeat() {
        let source = packet_source();
        let boundary = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();
        let first = ServerHeartbeatStateInput {
            source,
            authenticated_sender: AuthenticatedSenderEntry {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            },
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            heartbeat_sent_at: TimestampMicros(1_000_000),
            server_received_at: TimestampMicros(1_000_100),
            short_status: Some("first".to_string()),
        };
        let mut second = first.clone();
        second.heartbeat_sent_at = TimestampMicros(1_500_000);
        second.server_received_at = TimestampMicros(1_500_100);
        second.short_status = Some("second".to_string());

        boundary.commit(&mut state, first);
        let outcome = boundary.commit(&mut state, second);

        assert!(outcome.previous.is_some());
        assert_eq!(outcome.committed.received_heartbeats, 2);
        assert_eq!(
            outcome.committed.last_heartbeat_sent_at,
            TimestampMicros(1_500_000)
        );
        assert_eq!(
            outcome.committed.last_server_received_at,
            TimestampMicros(1_500_100)
        );
        assert_eq!(
            outcome.committed.last_short_status.as_deref(),
            Some("second")
        );
        assert_eq!(
            state
                .get(&ClientId("client-1".to_string()))
                .expect("liveness entry should be updated")
                .received_heartbeats,
            2
        );
    }

    #[test]
    fn heartbeat_liveness_commit_boundary_evaluates_timeout_without_revoking_registry() {
        let source = packet_source();
        let boundary = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();
        boundary.commit(
            &mut state,
            ServerHeartbeatStateInput {
                source,
                authenticated_sender: AuthenticatedSenderEntry {
                    client_id: ClientId("client-1".to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(2),
                    registered_at: None,
                },
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                heartbeat_sent_at: TimestampMicros(2_000_000),
                server_received_at: TimestampMicros(2_000_100),
                short_status: None,
            },
        );
        let policy = ServerHeartbeatTimeoutPolicy::new(500);

        let alive = boundary.evaluate_timeout(
            &state,
            &ClientId("client-1".to_string()),
            TimestampMicros(2_000_599),
            policy,
        );
        let timed_out = boundary.evaluate_timeout(
            &state,
            &ClientId("client-1".to_string()),
            TimestampMicros(2_000_600),
            policy,
        );
        let missing = boundary.evaluate_timeout(
            &state,
            &ClientId("client-2".to_string()),
            TimestampMicros(2_000_600),
            policy,
        );

        assert_eq!(
            alive,
            ServerHeartbeatTimeoutEvaluation::Alive {
                client_id: ClientId("client-1".to_string()),
                last_server_received_at: TimestampMicros(2_000_100),
                elapsed_micros: 499,
                timeout_after_micros: 500,
            }
        );
        assert_eq!(
            timed_out,
            ServerHeartbeatTimeoutEvaluation::TimedOut {
                client_id: ClientId("client-1".to_string()),
                last_server_received_at: TimestampMicros(2_000_100),
                elapsed_micros: 500,
                timeout_after_micros: 500,
            }
        );
        assert_eq!(
            missing,
            ServerHeartbeatTimeoutEvaluation::NoHeartbeat {
                client_id: ClientId("client-2".to_string())
            }
        );
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn heartbeat_timeout_action_boundary_plans_invalidation_log_and_notice() {
        let source = packet_source();
        let liveness = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();
        liveness.commit(
            &mut state,
            ServerHeartbeatStateInput {
                source,
                authenticated_sender: AuthenticatedSenderEntry {
                    client_id: ClientId("client-1".to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(2),
                    registered_at: None,
                },
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                heartbeat_sent_at: TimestampMicros(2_000_000),
                server_received_at: TimestampMicros(2_000_100),
                short_status: None,
            },
        );
        let evaluation = liveness.evaluate_timeout(
            &state,
            &ClientId("client-1".to_string()),
            TimestampMicros(2_000_600),
            ServerHeartbeatTimeoutPolicy::new(500),
        );
        let boundary = ServerHeartbeatTimeoutActionBoundary::default();

        let plan = boundary.plan_actions(&state, evaluation, TimestampMicros(2_000_600));

        assert!(matches!(
            plan.evaluation,
            ServerHeartbeatTimeoutEvaluation::TimedOut { .. }
        ));
        let invalidation = plan
            .registry_invalidation
            .expect("timeout should plan registry invalidation");
        assert_eq!(invalidation.client_id, ClientId("client-1".to_string()));
        assert_eq!(invalidation.source, source);
        assert_eq!(invalidation.run_id, RunId("run-1".to_string()));
        assert_eq!(
            invalidation.reason,
            AuthenticatedSenderInvalidationReason::HeartbeatTimeout
        );
        assert_eq!(invalidation.invalidated_at, TimestampMicros(2_000_600));

        let log = plan.timeout_log.expect("timeout should plan log handoff");
        assert_eq!(log.source, source);
        assert_eq!(log.client_id, ClientId("client-1".to_string()));
        assert_eq!(log.last_server_received_at, TimestampMicros(2_000_100));
        assert_eq!(log.evaluated_at, TimestampMicros(2_000_600));
        assert_eq!(log.elapsed_micros, 500);
        assert_eq!(log.timeout_after_micros, 500);
        assert!(log.registry_invalidation_planned);
        assert!(log.notice_planned);

        let notice = plan.notice.expect("timeout should plan notice");
        assert_eq!(notice.source, ServerNoticeTriggerSource::AuthExpired);
        assert_eq!(notice.notice.destination, source);
        assert_eq!(notice.notice.notice_type, NoticeType::AuthExpired);
        assert_eq!(notice.notice.run_id, RunId("run-1".to_string()));
        assert!(notice.notice.message.contains("heartbeat timeout"));
    }

    #[test]
    fn heartbeat_timeout_action_boundary_ignores_alive_or_missing_evaluations() {
        let source = packet_source();
        let liveness = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();
        liveness.commit(
            &mut state,
            ServerHeartbeatStateInput {
                source,
                authenticated_sender: AuthenticatedSenderEntry {
                    client_id: ClientId("client-1".to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(2),
                    registered_at: None,
                },
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                heartbeat_sent_at: TimestampMicros(3_000_000),
                server_received_at: TimestampMicros(3_000_100),
                short_status: None,
            },
        );
        let alive = liveness.evaluate_timeout(
            &state,
            &ClientId("client-1".to_string()),
            TimestampMicros(3_000_199),
            ServerHeartbeatTimeoutPolicy::new(500),
        );
        let missing = liveness.evaluate_timeout(
            &state,
            &ClientId("client-2".to_string()),
            TimestampMicros(3_000_700),
            ServerHeartbeatTimeoutPolicy::new(500),
        );
        let boundary = ServerHeartbeatTimeoutActionBoundary::default();

        let alive_plan = boundary.plan_actions(&state, alive, TimestampMicros(3_000_199));
        let missing_plan = boundary.plan_actions(&state, missing, TimestampMicros(3_000_700));

        assert!(alive_plan.registry_invalidation.is_none());
        assert!(alive_plan.timeout_log.is_none());
        assert!(alive_plan.notice.is_none());
        assert!(missing_plan.registry_invalidation.is_none());
        assert!(missing_plan.timeout_log.is_none());
        assert!(missing_plan.notice.is_none());
    }

    #[test]
    fn heartbeat_timeout_log_event_boundary_preserves_timeout_fields() {
        let source = packet_source();
        let input = ServerHeartbeatTimeoutLogInput {
            source,
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            last_server_received_at: TimestampMicros(4_000_100),
            evaluated_at: TimestampMicros(4_000_700),
            elapsed_micros: 600,
            timeout_after_micros: 500,
            registry_invalidation_planned: true,
            notice_planned: true,
        };
        let boundary = ServerHeartbeatTimeoutJsonLogEventBoundary;

        let event = boundary.build_event(input);

        assert_eq!(
            event.event_name,
            SERVER_HEARTBEAT_TIMEOUT_JSON_LOG_EVENT_NAME
        );
        assert_eq!(event.source, source);
        assert_eq!(event.client_id, ClientId("client-1".to_string()));
        assert_eq!(event.run_id, RunId("run-1".to_string()));
        assert_eq!(event.last_server_received_at, TimestampMicros(4_000_100));
        assert_eq!(event.evaluated_at, TimestampMicros(4_000_700));
        assert_eq!(event.elapsed_micros, 600);
        assert_eq!(event.timeout_after_micros, 500);
        assert!(event.registry_invalidation_planned);
        assert!(event.notice_planned);
    }

    #[test]
    fn authenticated_sender_registry_boundary_applies_explicit_timeout_invalidation() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let boundary = AuthenticatedSenderRegistryBoundary;
        let invalidation = AuthenticatedSenderInvalidation {
            client_id: ClientId("client-1".to_string()),
            source,
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            reason: AuthenticatedSenderInvalidationReason::HeartbeatTimeout,
            invalidated_at: TimestampMicros(5_000_000),
        };

        let outcome = boundary.invalidate(&mut registry, invalidation);

        assert!(outcome.removed_entry.is_some());
        assert_eq!(
            boundary.check_source(&registry, &ClientId("client-1".to_string()), source),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient)
        );
    }

    #[test]
    fn heartbeat_timeout_apply_boundary_applies_invalidation_log_and_notice_handoff() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let liveness = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();
        liveness.commit(
            &mut state,
            ServerHeartbeatStateInput {
                source,
                authenticated_sender: AuthenticatedSenderEntry {
                    client_id: ClientId("client-1".to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(2),
                    registered_at: None,
                },
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                heartbeat_sent_at: TimestampMicros(6_000_000),
                server_received_at: TimestampMicros(6_000_100),
                short_status: None,
            },
        );
        let evaluation = liveness.evaluate_timeout(
            &state,
            &ClientId("client-1".to_string()),
            TimestampMicros(6_000_700),
            ServerHeartbeatTimeoutPolicy::new(500),
        );
        let plan = ServerHeartbeatTimeoutActionBoundary::default().plan_actions(
            &state,
            evaluation,
            TimestampMicros(6_000_700),
        );
        let boundary = ServerHeartbeatTimeoutApplyBoundary::default();
        let mut output = Vec::new();

        let result = boundary
            .apply_plan(&mut registry, plan, &mut output)
            .expect("timeout apply should write log");

        assert!(matches!(
            result.evaluation,
            ServerHeartbeatTimeoutEvaluation::TimedOut { .. }
        ));
        assert!(result
            .registry_invalidation
            .as_ref()
            .and_then(|outcome| outcome.removed_entry.as_ref())
            .is_some());
        assert_eq!(
            AuthenticatedSenderRegistryBoundary.check_source(
                &registry,
                &ClientId("client-1".to_string()),
                source,
            ),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient)
        );
        let event = result
            .timeout_log_event
            .expect("timeout apply should return log event");
        assert_eq!(
            event.event_name,
            SERVER_HEARTBEAT_TIMEOUT_JSON_LOG_EVENT_NAME
        );
        assert_eq!(event.elapsed_micros, 600);
        let output = String::from_utf8(output).expect("timeout log should be utf-8");
        assert!(output.contains("\"event_name\":\"server.heartbeat_timeout\""));
        assert!(output.contains("\"client_id\":\"client-1\""));

        let notice_handoff = result
            .notice_handoff
            .expect("timeout apply should hand off notice");
        assert_eq!(
            notice_handoff.trigger_plan.source,
            ServerNoticeTriggerSource::AuthExpired
        );
        let notice = notice_handoff
            .outbound_notice
            .server_notice()
            .expect("outbound timeout notice should be ServerNotice");
        assert_eq!(notice.notice_type, NoticeType::AuthExpired);
        let ProtocolMessage::ServerNotice(queued_notice) = notice_handoff.queue_item.packet.message
        else {
            panic!("expected queued ServerNotice");
        };
        assert_eq!(queued_notice.notice_type, NoticeType::AuthExpired);
    }

    #[test]
    fn heartbeat_timeout_apply_boundary_preserves_alive_plan_without_side_effects() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let liveness = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();
        liveness.commit(
            &mut state,
            ServerHeartbeatStateInput {
                source,
                authenticated_sender: AuthenticatedSenderEntry {
                    client_id: ClientId("client-1".to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(2),
                    registered_at: None,
                },
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                heartbeat_sent_at: TimestampMicros(7_000_000),
                server_received_at: TimestampMicros(7_000_100),
                short_status: None,
            },
        );
        let evaluation = liveness.evaluate_timeout(
            &state,
            &ClientId("client-1".to_string()),
            TimestampMicros(7_000_200),
            ServerHeartbeatTimeoutPolicy::new(500),
        );
        let plan = ServerHeartbeatTimeoutActionBoundary::default().plan_actions(
            &state,
            evaluation,
            TimestampMicros(7_000_200),
        );
        let boundary = ServerHeartbeatTimeoutApplyBoundary::default();
        let mut output = Vec::new();

        let result = boundary
            .apply_plan(&mut registry, plan, &mut output)
            .expect("alive apply should not write log");

        assert!(matches!(
            result.evaluation,
            ServerHeartbeatTimeoutEvaluation::Alive { .. }
        ));
        assert!(result.registry_invalidation.is_none());
        assert!(result.timeout_log_event.is_none());
        assert!(result.notice_handoff.is_none());
        assert!(output.is_empty());
        assert!(matches!(
            AuthenticatedSenderRegistryBoundary.check_source(
                &registry,
                &ClientId("client-1".to_string()),
                source,
            ),
            AuthenticatedSenderCheck::Accepted(_)
        ));
    }

    #[test]
    fn heartbeat_timeout_notice_queue_storage_stores_notice_and_requests_wakeup() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let liveness = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();
        liveness.commit(
            &mut state,
            ServerHeartbeatStateInput {
                source,
                authenticated_sender: AuthenticatedSenderEntry {
                    client_id: ClientId("client-1".to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(2),
                    registered_at: None,
                },
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                heartbeat_sent_at: TimestampMicros(22_000_000),
                server_received_at: TimestampMicros(22_000_100),
                short_status: None,
            },
        );
        let evaluation = liveness.evaluate_timeout(
            &state,
            &ClientId("client-1".to_string()),
            TimestampMicros(22_000_700),
            ServerHeartbeatTimeoutPolicy::new(500),
        );
        let plan = ServerHeartbeatTimeoutActionBoundary::default().plan_actions(
            &state,
            evaluation,
            TimestampMicros(22_000_700),
        );
        let mut timeout_log = Vec::new();
        let apply = ServerHeartbeatTimeoutApplyBoundary::default()
            .apply_plan(&mut registry, plan, &mut timeout_log)
            .expect("timeout apply should create notice handoff");
        let boundary = ServerHeartbeatTimeoutNoticeQueueStorageBoundary::default();
        let mut collection = ServerOutboundQueueCollection::default();

        let result = boundary.store_notice(&mut collection, &apply);

        let ServerHeartbeatTimeoutNoticeQueueStorageResult::Stored(stored) = result else {
            panic!("timeout notice should be stored");
        };
        assert_eq!(stored.queue_len, 1);
        assert_eq!(
            stored.wakeup,
            ServerHeartbeatTimeoutNoticeSendWakeupPlan::RequestSendLoopWakeup {
                reason: ServerHeartbeatTimeoutNoticeSendWakeupReason::TimeoutNoticeQueued
            }
        );
        assert_eq!(stored.queued_item.state, OutboundQueueItemState::Queued);
        assert_eq!(collection.len(), 1);
        let dequeued = ServerOutboundQueueCollectionBoundary.dequeue_one(&mut collection);
        let ServerOutboundQueueDequeueRuntimeResult::Ready(queued) = dequeued else {
            panic!("queued timeout notice should be ready for send");
        };
        let ProtocolMessage::ServerNotice(notice) = queued.item.packet.message else {
            panic!("expected queued ServerNotice");
        };
        assert_eq!(notice.notice_type, NoticeType::AuthExpired);
    }

    #[test]
    fn heartbeat_timeout_notice_queue_storage_ignores_missing_notice() {
        let boundary = ServerHeartbeatTimeoutNoticeQueueStorageBoundary::default();
        let mut collection = ServerOutboundQueueCollection::default();
        let apply = ServerHeartbeatTimeoutApplyResult {
            evaluation: ServerHeartbeatTimeoutEvaluation::Alive {
                client_id: ClientId("client-1".to_string()),
                last_server_received_at: TimestampMicros(23_000_000),
                elapsed_micros: 100,
                timeout_after_micros: 500,
            },
            registry_invalidation: None,
            timeout_log_event: None,
            notice_handoff: None,
        };

        let result = boundary.store_notice(&mut collection, &apply);

        assert_eq!(
            result,
            ServerHeartbeatTimeoutNoticeQueueStorageResult::NoNotice {
                wakeup: ServerHeartbeatTimeoutNoticeSendWakeupPlan::NotRequested
            }
        );
        assert!(collection.is_empty());
        assert_eq!(
            result.wakeup(),
            ServerHeartbeatTimeoutNoticeSendWakeupPlan::NotRequested
        );
    }

    #[test]
    fn heartbeat_timeout_loop_tick_boundary_runs_one_client_timeout_path() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let liveness = ServerHeartbeatLivenessCommitBoundary;
        let mut state = ServerHeartbeatLivenessState::default();
        liveness.commit(
            &mut state,
            ServerHeartbeatStateInput {
                source,
                authenticated_sender: AuthenticatedSenderEntry {
                    client_id: ClientId("client-1".to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(2),
                    registered_at: None,
                },
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                heartbeat_sent_at: TimestampMicros(8_000_000),
                server_received_at: TimestampMicros(8_000_100),
                short_status: None,
            },
        );
        let boundary = ServerHeartbeatTimeoutLoopTickBoundary::default();
        let mut output = Vec::new();

        let result = boundary
            .run_one_client(
                &state,
                &mut registry,
                ServerHeartbeatTimeoutLoopTickInput {
                    client_id: ClientId("client-1".to_string()),
                    evaluated_at: TimestampMicros(8_000_700),
                    policy: ServerHeartbeatTimeoutPolicy::new(500),
                },
                &mut output,
            )
            .expect("one timeout loop tick should write timeout log");

        assert_eq!(result.input.client_id, ClientId("client-1".to_string()));
        assert!(matches!(
            result.action_plan.evaluation,
            ServerHeartbeatTimeoutEvaluation::TimedOut { .. }
        ));
        assert!(result
            .apply
            .registry_invalidation
            .as_ref()
            .and_then(|outcome| outcome.removed_entry.as_ref())
            .is_some());
        assert!(result.apply.timeout_log_event.is_some());
        assert!(result.apply.notice_handoff.is_some());
        assert_eq!(
            AuthenticatedSenderRegistryBoundary.check_source(
                &registry,
                &ClientId("client-1".to_string()),
                source,
            ),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient)
        );
        let output = String::from_utf8(output).expect("timeout log should be utf-8");
        assert!(output.contains("\"event_name\":\"server.heartbeat_timeout\""));
    }

    #[test]
    fn heartbeat_timeout_loop_tick_boundary_preserves_missing_client_without_side_effects() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let state = ServerHeartbeatLivenessState::default();
        let boundary = ServerHeartbeatTimeoutLoopTickBoundary::default();
        let mut output = Vec::new();

        let result = boundary
            .run_one_client(
                &state,
                &mut registry,
                ServerHeartbeatTimeoutLoopTickInput {
                    client_id: ClientId("client-2".to_string()),
                    evaluated_at: TimestampMicros(9_000_000),
                    policy: ServerHeartbeatTimeoutPolicy::new(500),
                },
                &mut output,
            )
            .expect("missing heartbeat loop tick should not write log");

        assert!(matches!(
            result.action_plan.evaluation,
            ServerHeartbeatTimeoutEvaluation::NoHeartbeat { .. }
        ));
        assert!(result.apply.registry_invalidation.is_none());
        assert!(result.apply.timeout_log_event.is_none());
        assert!(result.apply.notice_handoff.is_none());
        assert!(output.is_empty());
        assert!(matches!(
            AuthenticatedSenderRegistryBoundary.check_source(
                &registry,
                &ClientId("client-1".to_string()),
                source,
            ),
            AuthenticatedSenderCheck::Accepted(_)
        ));
    }

    #[test]
    fn heartbeat_timeout_multi_client_loop_processes_multiple_registered_clients() {
        let source_1 = packet_source();
        let source_2 = packet_source_at(5001);
        let mut registry = AuthenticatedSenderRegistry::default();
        register_test_client(&mut registry, "client-1", source_1);
        register_test_client(&mut registry, "client-2", source_2);
        let mut state = ServerHeartbeatLivenessState::default();
        commit_liveness_for_test(
            &mut state,
            "client-1",
            source_1,
            TimestampMicros(10_000_100),
        );
        commit_liveness_for_test(
            &mut state,
            "client-2",
            source_2,
            TimestampMicros(10_000_550),
        );
        let mut notices = ServerOutboundQueueCollection::default();
        let mut output = Vec::new();

        let result = ServerHeartbeatTimeoutMultiClientLoopBoundary::default()
            .run_all_registered(
                &state,
                &mut registry,
                &mut notices,
                ServerHeartbeatTimeoutMultiClientLoopInput {
                    evaluated_at: TimestampMicros(10_000_700),
                    policy: ServerHeartbeatTimeoutPolicy::new(500),
                },
                &mut output,
            )
            .expect("multi-client timeout loop should run");

        let ServerHeartbeatTimeoutMultiClientLoopResult::AllClientsProcessed {
            processed,
            timeout_actions_applied,
            ..
        } = result
        else {
            panic!("registered clients should be processed");
        };
        assert_eq!(processed.len(), 2);
        assert_eq!(timeout_actions_applied, 1);
        assert!(matches!(
            processed[0].tick.apply.evaluation,
            ServerHeartbeatTimeoutEvaluation::TimedOut { .. }
        ));
        assert!(matches!(
            processed[1].tick.apply.evaluation,
            ServerHeartbeatTimeoutEvaluation::Alive { .. }
        ));
        assert_eq!(notices.len(), 1);
        assert!(String::from_utf8(output)
            .expect("timeout log should be utf-8")
            .contains("\"client_id\":\"client-1\""));
    }

    #[test]
    fn heartbeat_timeout_multi_client_loop_no_client_path_remains_explicit() {
        let state = ServerHeartbeatLivenessState::default();
        let mut registry = AuthenticatedSenderRegistry::default();
        let mut notices = ServerOutboundQueueCollection::default();
        let input = ServerHeartbeatTimeoutMultiClientLoopInput {
            evaluated_at: TimestampMicros(11_000_000),
            policy: ServerHeartbeatTimeoutPolicy::new(500),
        };
        let mut output = Vec::new();

        let result = ServerHeartbeatTimeoutMultiClientLoopBoundary::default()
            .run_all_registered(
                &state,
                &mut registry,
                &mut notices,
                input.clone(),
                &mut output,
            )
            .expect("empty registry should not fail");

        assert_eq!(
            result,
            ServerHeartbeatTimeoutMultiClientLoopResult::NoClientsAvailable { input }
        );
        assert!(notices.is_empty());
        assert!(output.is_empty());
    }

    #[test]
    fn heartbeat_timeout_multi_client_loop_preserves_timeout_action_application_per_client() {
        let source_1 = packet_source();
        let source_2 = packet_source_at(5002);
        let mut registry = AuthenticatedSenderRegistry::default();
        register_test_client(&mut registry, "client-1", source_1);
        register_test_client(&mut registry, "client-2", source_2);
        let mut state = ServerHeartbeatLivenessState::default();
        commit_liveness_for_test(
            &mut state,
            "client-1",
            source_1,
            TimestampMicros(12_000_100),
        );
        commit_liveness_for_test(
            &mut state,
            "client-2",
            source_2,
            TimestampMicros(12_000_100),
        );
        let mut notices = ServerOutboundQueueCollection::default();
        let mut output = Vec::new();

        let result = ServerHeartbeatTimeoutMultiClientLoopBoundary::default()
            .run_all_registered(
                &state,
                &mut registry,
                &mut notices,
                ServerHeartbeatTimeoutMultiClientLoopInput {
                    evaluated_at: TimestampMicros(12_000_700),
                    policy: ServerHeartbeatTimeoutPolicy::new(500),
                },
                &mut output,
            )
            .expect("multi-client timeout loop should apply timeout actions");

        let ServerHeartbeatTimeoutMultiClientLoopResult::AllClientsProcessed {
            processed,
            timeout_actions_applied,
            ..
        } = result
        else {
            panic!("registered clients should be processed");
        };
        assert_eq!(processed.len(), 2);
        assert_eq!(timeout_actions_applied, 2);
        assert!(processed
            .iter()
            .all(|client| client.tick.apply.registry_invalidation.is_some()));
        assert_eq!(notices.len(), 2);
        assert_eq!(
            AuthenticatedSenderRegistryBoundary.check_source(
                &registry,
                &ClientId("client-1".to_string()),
                source_1,
            ),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient)
        );
        assert_eq!(
            AuthenticatedSenderRegistryBoundary.check_source(
                &registry,
                &ClientId("client-2".to_string()),
                source_2,
            ),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient)
        );
    }

    #[test]
    fn heartbeat_timeout_multi_client_loop_keeps_notice_queue_storage_separate_from_wakeup_execution(
    ) {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let mut state = ServerHeartbeatLivenessState::default();
        commit_liveness_for_test(&mut state, "client-1", source, TimestampMicros(13_000_100));
        let mut notices = ServerOutboundQueueCollection::default();
        let mut output = Vec::new();

        let result = ServerHeartbeatTimeoutMultiClientLoopBoundary::default()
            .run_all_registered(
                &state,
                &mut registry,
                &mut notices,
                ServerHeartbeatTimeoutMultiClientLoopInput {
                    evaluated_at: TimestampMicros(13_000_700),
                    policy: ServerHeartbeatTimeoutPolicy::new(500),
                },
                &mut output,
            )
            .expect("timeout loop should store notice");

        let ServerHeartbeatTimeoutMultiClientLoopResult::AllClientsProcessed { processed, .. } =
            result
        else {
            panic!("registered client should be processed");
        };
        assert_eq!(notices.len(), 1);
        assert_eq!(
            processed[0].notice_queue_storage.wakeup(),
            ServerHeartbeatTimeoutNoticeSendWakeupPlan::RequestSendLoopWakeup {
                reason: ServerHeartbeatTimeoutNoticeSendWakeupReason::TimeoutNoticeQueued
            }
        );
        let queued = ServerOutboundQueueCollectionBoundary.dequeue_one(&mut notices);
        assert!(matches!(
            queued,
            ServerOutboundQueueDequeueRuntimeResult::Ready(_)
        ));
    }

    #[test]
    fn heartbeat_timeout_multi_client_loop_does_not_reinterpret_one_client_tick_semantics() {
        let source = packet_source();
        let mut multi_registry = registry_with_client("client-1", source);
        let mut single_registry = registry_with_client("client-1", source);
        let mut state = ServerHeartbeatLivenessState::default();
        commit_liveness_for_test(&mut state, "client-1", source, TimestampMicros(14_000_100));
        let input = ServerHeartbeatTimeoutLoopTickInput {
            client_id: ClientId("client-1".to_string()),
            evaluated_at: TimestampMicros(14_000_700),
            policy: ServerHeartbeatTimeoutPolicy::new(500),
        };
        let mut single_output = Vec::new();
        let single = ServerHeartbeatTimeoutLoopTickBoundary::default()
            .run_one_client(
                &state,
                &mut single_registry,
                input.clone(),
                &mut single_output,
            )
            .expect("single-client tick should run");
        let mut notices = ServerOutboundQueueCollection::default();
        let mut multi_output = Vec::new();

        let multi = ServerHeartbeatTimeoutMultiClientLoopBoundary::default()
            .run_all_registered(
                &state,
                &mut multi_registry,
                &mut notices,
                ServerHeartbeatTimeoutMultiClientLoopInput {
                    evaluated_at: input.evaluated_at,
                    policy: input.policy,
                },
                &mut multi_output,
            )
            .expect("multi-client loop should run");

        let ServerHeartbeatTimeoutMultiClientLoopResult::AllClientsProcessed { processed, .. } =
            multi
        else {
            panic!("registered client should be processed");
        };
        assert_eq!(processed.len(), 1);
        assert_eq!(processed[0].tick.action_plan, single.action_plan);
        assert_eq!(processed[0].tick.apply.evaluation, single.apply.evaluation);
        assert_eq!(
            processed[0].tick.apply.registry_invalidation,
            single.apply.registry_invalidation
        );
    }

    #[test]
    fn heartbeat_timeout_multi_client_loop_keeps_caller_owned_state_outside_loop_body() {
        let source = packet_source();
        let mut registry = registry_with_client("client-1", source);
        let mut state = ServerHeartbeatLivenessState::default();
        commit_liveness_for_test(&mut state, "client-1", source, TimestampMicros(15_000_100));
        let mut notices = ServerOutboundQueueCollection::default();
        let mut output = Vec::new();

        let _result = ServerHeartbeatTimeoutMultiClientLoopBoundary::default()
            .run_all_registered(
                &state,
                &mut registry,
                &mut notices,
                ServerHeartbeatTimeoutMultiClientLoopInput {
                    evaluated_at: TimestampMicros(15_000_700),
                    policy: ServerHeartbeatTimeoutPolicy::new(500),
                },
                &mut output,
            )
            .expect("multi-client loop should run against caller-owned state");

        assert_eq!(state.len(), 1, "liveness state remains caller-owned");
        assert_eq!(notices.len(), 1, "notice queue remains caller-owned");
        assert_eq!(
            AuthenticatedSenderRegistryBoundary.check_source(
                &registry,
                &ClientId("client-1".to_string()),
                source,
            ),
            AuthenticatedSenderCheck::Rejected(AuthenticatedSenderRejectReason::UnknownClient),
            "caller-owned registry receives explicit timeout invalidation"
        );
    }

    #[test]
    fn heartbeat_timebase_plan_boundary_preserves_ids_and_timestamp_sample() {
        let input = ServerHeartbeatTimebaseInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            client_sent_at: TimestampMicros(4_000_000),
            client_local_time: None,
            server_received_at: TimestampMicros(4_000_100),
            server_sent_at: TimestampMicros(4_000_200),
        };
        let boundary = ServerHeartbeatTimebasePlanBoundary::default();

        let plan = boundary.build_plan(&input);

        assert_eq!(plan.client_id, ClientId("client-1".to_string()));
        assert_eq!(plan.run_id, RunId("run-1".to_string()));
        assert_eq!(plan.estimate.sample.client_sent_at_micros, 4_000_000);
        assert_eq!(plan.estimate.sample.client_local_time_micros, None);
        assert_eq!(plan.estimate.sample.server_received_at_micros, 4_000_100);
        assert_eq!(plan.estimate.sample.server_sent_at_micros, 4_000_200);
    }

    #[test]
    fn heartbeat_rtt_offset_calculation_boundary_calculates_single_exchange() {
        let timebase_input = ServerHeartbeatTimebaseInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            client_sent_at: TimestampMicros(1_000),
            client_local_time: Some(TimestampMicros(1_000)),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
        };
        let plan = ServerHeartbeatTimebasePlanBoundary::default().build_plan(&timebase_input);
        let observation = ServerHeartbeatClientAckObservation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_client_sent_at: TimestampMicros(1_000),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
            client_received_at: TimestampMicros(1_150),
        };
        let boundary = ServerHeartbeatRttOffsetCalculationBoundary::default();

        let calculation = boundary.calculate(&plan, &observation).unwrap();

        assert_eq!(calculation.client_id, ClientId("client-1".to_string()));
        assert_eq!(calculation.run_id, RunId("run-1".to_string()));
        assert_eq!(calculation.estimate.rtt_micros, 100);
        assert_eq!(calculation.estimate.clock_offset_micros, 1_050);
    }

    #[test]
    fn heartbeat_rtt_offset_calculation_boundary_rejects_mismatched_echo() {
        let timebase_input = ServerHeartbeatTimebaseInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            client_sent_at: TimestampMicros(1_000),
            client_local_time: None,
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
        };
        let plan = ServerHeartbeatTimebasePlanBoundary::default().build_plan(&timebase_input);
        let observation = ServerHeartbeatClientAckObservation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_client_sent_at: TimestampMicros(999),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
            client_received_at: TimestampMicros(1_150),
        };
        let boundary = ServerHeartbeatRttOffsetCalculationBoundary::default();

        let error = boundary.calculate(&plan, &observation).unwrap_err();

        assert_eq!(
            error,
            ServerHeartbeatRttOffsetCalculationError::EchoedSentAtMismatch {
                expected: TimestampMicros(1_000),
                actual: TimestampMicros(999),
            }
        );
    }

    #[test]
    fn heartbeat_rtt_offset_commit_boundary_commits_first_estimate() {
        let boundary = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        let calculation = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 100,
                server_processing_micros: 20,
                clock_offset_micros: 1_000,
            },
        };

        let outcome = boundary.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation,
                committed_at: Some(TimestampMicros(10_000)),
            },
        );

        assert_eq!(state.len(), 1);
        assert_eq!(outcome.previous, None);
        assert!(!outcome.replaced_previous_run);
        assert_eq!(
            outcome.committed.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(outcome.committed.run_id, RunId("run-1".to_string()));
        assert_eq!(outcome.committed.latest_estimate.rtt_micros, 100);
        assert_eq!(outcome.committed.latest_estimate.clock_offset_micros, 1_000);
        assert_eq!(outcome.committed.committed_samples, 1);
        assert_eq!(
            outcome.committed.last_committed_at,
            Some(TimestampMicros(10_000))
        );
        assert_eq!(
            state
                .get(&ClientId("client-1".to_string()))
                .expect("estimate should be committed"),
            &outcome.committed
        );
    }

    #[test]
    fn heartbeat_rtt_offset_commit_boundary_increments_same_run_samples() {
        let boundary = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        let first = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 100,
                server_processing_micros: 20,
                clock_offset_micros: 1_000,
            },
        };
        let second = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 120,
                server_processing_micros: 30,
                clock_offset_micros: 1_010,
            },
        };

        boundary.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: first,
                committed_at: None,
            },
        );
        let outcome = boundary.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: second,
                committed_at: Some(TimestampMicros(11_000)),
            },
        );

        assert!(outcome.previous.is_some());
        assert!(!outcome.replaced_previous_run);
        assert_eq!(outcome.committed.committed_samples, 2);
        assert_eq!(outcome.committed.latest_estimate.rtt_micros, 120);
        assert_eq!(outcome.committed.latest_estimate.clock_offset_micros, 1_010);
    }

    #[test]
    fn heartbeat_rtt_offset_commit_boundary_resets_sample_count_on_new_run() {
        let boundary = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        boundary.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: ServerHeartbeatRttOffsetCalculation {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    estimate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 100,
                        server_processing_micros: 20,
                        clock_offset_micros: 1_000,
                    },
                },
                committed_at: None,
            },
        );

        let outcome = boundary.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: ServerHeartbeatRttOffsetCalculation {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-2".to_string()),
                    estimate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 90,
                        server_processing_micros: 10,
                        clock_offset_micros: 900,
                    },
                },
                committed_at: Some(TimestampMicros(12_000)),
            },
        );

        assert!(outcome.previous.is_some());
        assert!(outcome.replaced_previous_run);
        assert_eq!(outcome.committed.run_id, RunId("run-2".to_string()));
        assert_eq!(outcome.committed.committed_samples, 1);
        assert_eq!(outcome.committed.latest_estimate.clock_offset_micros, 900);
    }

    #[test]
    fn heartbeat_rtt_offset_candidate_policy_accepts_without_thresholds() {
        let commit = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        commit.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: ServerHeartbeatRttOffsetCalculation {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    estimate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 100,
                        server_processing_micros: 20,
                        clock_offset_micros: 1_000,
                    },
                },
                committed_at: None,
            },
        );
        let candidate = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 1_000_000,
                server_processing_micros: 20,
                clock_offset_micros: -1_000_000,
            },
        };
        let boundary = ServerHeartbeatRttOffsetCandidatePolicyBoundary;

        let result = boundary.evaluate(
            &state,
            &candidate,
            ServerHeartbeatRttOffsetCandidatePolicy::default(),
        );

        assert!(result.previous.is_some());
        assert_eq!(
            result.decision,
            ServerHeartbeatRttOffsetCandidatePolicyDecision::Accept {
                smoothing: ServerHeartbeatRttOffsetSmoothingMode::Deferred
            }
        );
    }

    #[test]
    fn heartbeat_rtt_offset_candidate_policy_rejects_rtt_delta_outlier() {
        let commit = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        commit.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: ServerHeartbeatRttOffsetCalculation {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    estimate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 100,
                        server_processing_micros: 20,
                        clock_offset_micros: 1_000,
                    },
                },
                committed_at: None,
            },
        );
        let candidate = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 180,
                server_processing_micros: 20,
                clock_offset_micros: 1_010,
            },
        };
        let boundary = ServerHeartbeatRttOffsetCandidatePolicyBoundary;

        let result = boundary.evaluate(
            &state,
            &candidate,
            ServerHeartbeatRttOffsetCandidatePolicy {
                smoothing: ServerHeartbeatRttOffsetSmoothingMode::Deferred,
                outlier: ServerHeartbeatRttOffsetOutlierPolicy {
                    max_rtt_delta_micros: Some(50),
                    max_clock_offset_delta_micros: Some(100),
                },
            },
        );

        assert_eq!(
            result.decision,
            ServerHeartbeatRttOffsetCandidatePolicyDecision::RejectOutlier {
                reason: ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded {
                    previous_micros: 100,
                    candidate_micros: 180,
                    delta_micros: 80,
                    max_delta_micros: 50,
                }
            }
        );
    }

    #[test]
    fn heartbeat_rtt_offset_candidate_policy_rejects_offset_delta_outlier() {
        let commit = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        commit.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: ServerHeartbeatRttOffsetCalculation {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    estimate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 100,
                        server_processing_micros: 20,
                        clock_offset_micros: 1_000,
                    },
                },
                committed_at: None,
            },
        );
        let candidate = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 110,
                server_processing_micros: 20,
                clock_offset_micros: 1_250,
            },
        };
        let boundary = ServerHeartbeatRttOffsetCandidatePolicyBoundary;

        let result = boundary.evaluate(
            &state,
            &candidate,
            ServerHeartbeatRttOffsetCandidatePolicy {
                smoothing: ServerHeartbeatRttOffsetSmoothingMode::Deferred,
                outlier: ServerHeartbeatRttOffsetOutlierPolicy {
                    max_rtt_delta_micros: Some(50),
                    max_clock_offset_delta_micros: Some(100),
                },
            },
        );

        assert_eq!(
            result.decision,
            ServerHeartbeatRttOffsetCandidatePolicyDecision::RejectOutlier {
                reason: ServerHeartbeatRttOffsetOutlierReason::ClockOffsetDeltaExceeded {
                    previous_micros: 1_000,
                    candidate_micros: 1_250,
                    delta_micros: 250,
                    max_delta_micros: 100,
                }
            }
        );
    }

    #[test]
    fn heartbeat_rtt_offset_candidate_policy_accepts_new_run_without_cross_run_outlier_check() {
        let commit = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        commit.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: ServerHeartbeatRttOffsetCalculation {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    estimate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 100,
                        server_processing_micros: 20,
                        clock_offset_micros: 1_000,
                    },
                },
                committed_at: None,
            },
        );
        let candidate = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-2".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 10_000,
                server_processing_micros: 20,
                clock_offset_micros: -10_000,
            },
        };
        let boundary = ServerHeartbeatRttOffsetCandidatePolicyBoundary;

        let result = boundary.evaluate(
            &state,
            &candidate,
            ServerHeartbeatRttOffsetCandidatePolicy {
                smoothing: ServerHeartbeatRttOffsetSmoothingMode::Deferred,
                outlier: ServerHeartbeatRttOffsetOutlierPolicy {
                    max_rtt_delta_micros: Some(50),
                    max_clock_offset_delta_micros: Some(100),
                },
            },
        );

        assert!(result.previous.is_some());
        assert_eq!(
            result.decision,
            ServerHeartbeatRttOffsetCandidatePolicyDecision::Accept {
                smoothing: ServerHeartbeatRttOffsetSmoothingMode::Deferred
            }
        );
    }

    #[test]
    fn heartbeat_rtt_offset_policy_commit_boundary_commits_accepted_candidate() {
        let boundary = ServerHeartbeatRttOffsetPolicyCommitBoundary::default();
        let mut state = ServerHeartbeatRttOffsetState::default();
        let calculation = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 100,
                server_processing_micros: 20,
                clock_offset_micros: 1_000,
            },
        };

        let outcome = boundary.evaluate_and_commit(
            &mut state,
            calculation,
            ServerHeartbeatRttOffsetCandidatePolicy::default(),
            Some(TimestampMicros(13_000)),
        );

        assert!(matches!(
            outcome.policy.decision,
            ServerHeartbeatRttOffsetCandidatePolicyDecision::Accept { .. }
        ));
        let commit = outcome
            .commit_outcome()
            .expect("accepted candidate should be committed");
        assert_eq!(commit.committed.committed_samples, 1);
        assert_eq!(outcome.committed_samples(), Some(1));
        assert_eq!(state.len(), 1);
        assert_eq!(
            state
                .get(&ClientId("client-1".to_string()))
                .expect("accepted candidate should be stored")
                .latest_estimate
                .clock_offset_micros,
            1_000
        );
    }

    #[test]
    fn heartbeat_rtt_offset_policy_commit_boundary_skips_rejected_candidate_without_state_change() {
        let commit = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        commit.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: ServerHeartbeatRttOffsetCalculation {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    estimate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 100,
                        server_processing_micros: 20,
                        clock_offset_micros: 1_000,
                    },
                },
                committed_at: Some(TimestampMicros(14_000)),
            },
        );
        let boundary = ServerHeartbeatRttOffsetPolicyCommitBoundary::default();
        let rejected = ServerHeartbeatRttOffsetCalculation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            estimate: HeartbeatRttOffsetEstimate {
                rtt_micros: 250,
                server_processing_micros: 20,
                clock_offset_micros: 1_500,
            },
        };

        let outcome = boundary.evaluate_and_commit(
            &mut state,
            rejected,
            ServerHeartbeatRttOffsetCandidatePolicy {
                smoothing: ServerHeartbeatRttOffsetSmoothingMode::Deferred,
                outlier: ServerHeartbeatRttOffsetOutlierPolicy {
                    max_rtt_delta_micros: Some(50),
                    max_clock_offset_delta_micros: Some(100),
                },
            },
            Some(TimestampMicros(15_000)),
        );

        assert_eq!(
            outcome.result,
            ServerHeartbeatRttOffsetPolicyCommitResult::Skipped(
                ServerHeartbeatRttOffsetCommitSkipReason::RejectedOutlier(
                    ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded {
                        previous_micros: 100,
                        candidate_micros: 250,
                        delta_micros: 150,
                        max_delta_micros: 50,
                    }
                )
            )
        );
        assert_eq!(outcome.committed_samples(), None);
        let entry = state
            .get(&ClientId("client-1".to_string()))
            .expect("previous estimate should remain");
        assert_eq!(entry.latest_estimate.rtt_micros, 100);
        assert_eq!(entry.latest_estimate.clock_offset_micros, 1_000);
        assert_eq!(entry.committed_samples, 1);
        assert_eq!(entry.last_committed_at, Some(TimestampMicros(14_000)));
    }

    #[test]
    fn heartbeat_rtt_offset_rejected_candidate_handoff_prepares_log_and_metrics() {
        let commit = ServerHeartbeatRttOffsetCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetState::default();
        commit.commit(
            &mut state,
            ServerHeartbeatRttOffsetCommitInput {
                calculation: ServerHeartbeatRttOffsetCalculation {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    estimate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 100,
                        server_processing_micros: 20,
                        clock_offset_micros: 1_000,
                    },
                },
                committed_at: None,
            },
        );
        let policy_commit = ServerHeartbeatRttOffsetPolicyCommitBoundary::default();
        let outcome = policy_commit.evaluate_and_commit(
            &mut state,
            ServerHeartbeatRttOffsetCalculation {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                estimate: HeartbeatRttOffsetEstimate {
                    rtt_micros: 100,
                    server_processing_micros: 20,
                    clock_offset_micros: 1_250,
                },
            },
            ServerHeartbeatRttOffsetCandidatePolicy {
                smoothing: ServerHeartbeatRttOffsetSmoothingMode::Deferred,
                outlier: ServerHeartbeatRttOffsetOutlierPolicy {
                    max_rtt_delta_micros: Some(500),
                    max_clock_offset_delta_micros: Some(100),
                },
            },
            Some(TimestampMicros(16_000)),
        );
        let boundary = ServerHeartbeatRttOffsetRejectedCandidateHandoffBoundary;

        let result = boundary.prepare(&outcome, TimestampMicros(16_100));

        let ServerHeartbeatRttOffsetRejectedCandidateHandoffResult::Prepared(handoff) = result
        else {
            panic!("rejected candidate should prepare log and metrics handoff");
        };
        assert_eq!(handoff.log.client_id, ClientId("client-1".to_string()));
        assert_eq!(handoff.log.run_id, RunId("run-1".to_string()));
        assert_eq!(handoff.log.candidate.clock_offset_micros, 1_250);
        assert!(handoff.log.state_commit_skipped);
        assert_eq!(handoff.log.observed_at, TimestampMicros(16_100));
        assert_eq!(handoff.metrics.rejected_candidates_delta, 1);
        assert_eq!(handoff.metrics.skipped_commits_delta, 1);
        assert_eq!(
            handoff.metrics.reason,
            ServerHeartbeatRttOffsetOutlierReason::ClockOffsetDeltaExceeded {
                previous_micros: 1_000,
                candidate_micros: 1_250,
                delta_micros: 250,
                max_delta_micros: 100,
            }
        );
    }

    #[test]
    fn heartbeat_rtt_offset_rejected_candidate_handoff_ignores_committed_candidate() {
        let policy_commit = ServerHeartbeatRttOffsetPolicyCommitBoundary::default();
        let mut state = ServerHeartbeatRttOffsetState::default();
        let outcome = policy_commit.evaluate_and_commit(
            &mut state,
            ServerHeartbeatRttOffsetCalculation {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                estimate: HeartbeatRttOffsetEstimate {
                    rtt_micros: 100,
                    server_processing_micros: 20,
                    clock_offset_micros: 1_000,
                },
            },
            ServerHeartbeatRttOffsetCandidatePolicy::default(),
            Some(TimestampMicros(17_000)),
        );
        let boundary = ServerHeartbeatRttOffsetRejectedCandidateHandoffBoundary;

        let result = boundary.prepare(&outcome, TimestampMicros(17_100));

        assert_eq!(
            result,
            ServerHeartbeatRttOffsetRejectedCandidateHandoffResult::NotRejected
        );
    }

    #[test]
    fn heartbeat_rtt_offset_rejected_candidate_log_writer_writes_json_line() {
        let boundary = ServerHeartbeatRttOffsetRejectedCandidateLogOutputBoundary::default();
        let mut output = Vec::new();

        let event = boundary
            .write_rejected_candidate(
                ServerHeartbeatRttOffsetRejectedCandidateLogInput {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    candidate: HeartbeatRttOffsetEstimate {
                        rtt_micros: 250,
                        server_processing_micros: 20,
                        clock_offset_micros: 1_500,
                    },
                    reason: ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded {
                        previous_micros: 100,
                        candidate_micros: 250,
                        delta_micros: 150,
                        max_delta_micros: 50,
                    },
                    state_commit_skipped: true,
                    observed_at: TimestampMicros(18_000),
                },
                &mut output,
            )
            .expect("rejected candidate log should be written");

        assert_eq!(
            event.event_name,
            SERVER_HEARTBEAT_RTT_OFFSET_REJECTED_CANDIDATE_JSON_LOG_EVENT_NAME
        );
        let output =
            String::from_utf8(output).expect("rejected candidate log should be valid utf-8");
        assert!(
            output.contains("\"event_name\":\"server.heartbeat_rtt_offset_rejected_candidate\"")
        );
        assert!(output.contains("\"client_id\":\"client-1\""));
        assert!(output.contains("\"reject_reason\":\"RttDeltaExceeded\""));
        assert!(output.contains("\"state_commit_skipped\":true"));
        assert!(output.contains("\"observed_at\":18000"));
    }

    #[test]
    fn heartbeat_rtt_offset_rejected_candidate_metrics_commit_creates_entry() {
        let boundary = ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetRejectedCandidateMetricsState::default();

        let outcome = boundary.commit(
            &mut state,
            ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitInput {
                handoff: ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    reason: ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded {
                        previous_micros: 100,
                        candidate_micros: 250,
                        delta_micros: 150,
                        max_delta_micros: 50,
                    },
                    rejected_candidates_delta: 1,
                    skipped_commits_delta: 1,
                },
                updated_at: Some(TimestampMicros(19_000)),
            },
        );

        assert!(outcome.previous.is_none());
        assert_eq!(
            outcome.committed.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(outcome.committed.run_id, RunId("run-1".to_string()));
        assert_eq!(outcome.committed.total_rejected_candidates, 1);
        assert_eq!(outcome.committed.total_skipped_commits, 1);
        assert_eq!(outcome.committed.rtt_delta_rejections, 1);
        assert_eq!(outcome.committed.clock_offset_delta_rejections, 0);
        assert_eq!(
            outcome.committed.last_updated_at,
            Some(TimestampMicros(19_000))
        );
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn heartbeat_rtt_offset_rejected_candidate_metrics_commit_aggregates_by_reason() {
        let boundary = ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetRejectedCandidateMetricsState::default();

        boundary.commit(
            &mut state,
            ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitInput {
                handoff: ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    reason: ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded {
                        previous_micros: 100,
                        candidate_micros: 180,
                        delta_micros: 80,
                        max_delta_micros: 50,
                    },
                    rejected_candidates_delta: 1,
                    skipped_commits_delta: 1,
                },
                updated_at: Some(TimestampMicros(20_000)),
            },
        );
        let outcome = boundary.commit(
            &mut state,
            ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitInput {
                handoff: ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    reason: ServerHeartbeatRttOffsetOutlierReason::ClockOffsetDeltaExceeded {
                        previous_micros: 1_000,
                        candidate_micros: 1_250,
                        delta_micros: 250,
                        max_delta_micros: 100,
                    },
                    rejected_candidates_delta: 1,
                    skipped_commits_delta: 1,
                },
                updated_at: Some(TimestampMicros(20_100)),
            },
        );

        assert!(outcome.previous.is_some());
        assert_eq!(outcome.committed.total_rejected_candidates, 2);
        assert_eq!(outcome.committed.total_skipped_commits, 2);
        assert_eq!(outcome.committed.rtt_delta_rejections, 1);
        assert_eq!(outcome.committed.clock_offset_delta_rejections, 1);
        assert_eq!(
            outcome.committed.last_updated_at,
            Some(TimestampMicros(20_100))
        );
        let entry = state
            .get(
                &ClientId("client-1".to_string()),
                &RunId("run-1".to_string()),
            )
            .expect("metrics entry should be stored");
        assert_eq!(entry.total_rejected_candidates, 2);
    }

    #[test]
    fn heartbeat_rtt_offset_rejected_candidate_metrics_export_snapshots_entries() {
        let commit = ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetRejectedCandidateMetricsState::default();
        commit.commit(
            &mut state,
            ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitInput {
                handoff: ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff {
                    client_id: ClientId("client-2".to_string()),
                    run_id: RunId("run-2".to_string()),
                    reason: ServerHeartbeatRttOffsetOutlierReason::RttDeltaExceeded {
                        previous_micros: 100,
                        candidate_micros: 200,
                        delta_micros: 100,
                        max_delta_micros: 50,
                    },
                    rejected_candidates_delta: 1,
                    skipped_commits_delta: 1,
                },
                updated_at: Some(TimestampMicros(21_000)),
            },
        );
        let export = ServerHeartbeatRttOffsetRejectedCandidateMetricsExportBoundary;

        let snapshot = export.snapshot(&state);

        assert_eq!(snapshot.records.len(), 1);
        assert_eq!(
            snapshot.records[0].client_id,
            ClientId("client-2".to_string())
        );
        assert_eq!(snapshot.records[0].run_id, RunId("run-2".to_string()));
        assert_eq!(snapshot.records[0].total_rejected_candidates, 1);
        assert_eq!(snapshot.records[0].total_skipped_commits, 1);
        assert_eq!(snapshot.records[0].rtt_delta_rejections, 1);
        assert_eq!(snapshot.records[0].clock_offset_delta_rejections, 0);
        assert_eq!(
            snapshot.records[0].last_updated_at,
            Some(TimestampMicros(21_000))
        );
    }

    #[test]
    fn heartbeat_rtt_offset_metrics_snapshot_export_handoff_returns_no_records_for_empty_state() {
        let state = ServerHeartbeatRttOffsetRejectedCandidateMetricsState::default();
        let boundary = ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary::default();

        let result = boundary.export_for_consumer(
            &state,
            ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureDashboard,
            Some(TimestampMicros(22_000)),
        );

        assert_eq!(
            result,
            ServerHeartbeatRttOffsetMetricsSnapshotExportRuntimeResult::NoRecords {
                consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureDashboard,
                exported_at: Some(TimestampMicros(22_000)),
            }
        );
    }

    #[test]
    fn heartbeat_rtt_offset_metrics_snapshot_export_handoff_targets_future_dashboard() {
        let commit = ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitBoundary;
        let mut state = ServerHeartbeatRttOffsetRejectedCandidateMetricsState::default();
        commit.commit(
            &mut state,
            ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitInput {
                handoff: ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff {
                    client_id: ClientId("client-3".to_string()),
                    run_id: RunId("run-3".to_string()),
                    reason: ServerHeartbeatRttOffsetOutlierReason::ClockOffsetDeltaExceeded {
                        previous_micros: 1_000,
                        candidate_micros: 1_300,
                        delta_micros: 300,
                        max_delta_micros: 100,
                    },
                    rejected_candidates_delta: 1,
                    skipped_commits_delta: 1,
                },
                updated_at: Some(TimestampMicros(23_000)),
            },
        );
        let export = ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary::default();
        let consumer = ServerHeartbeatRttOffsetMetricsSnapshotConsumerBoundary;

        let result = export.export_for_consumer(
            &state,
            ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureDashboard,
            Some(TimestampMicros(23_100)),
        );
        let ServerHeartbeatRttOffsetMetricsSnapshotExportRuntimeResult::Handoff(handoff) = result
        else {
            panic!("non-empty metrics state should export handoff");
        };
        let consumed = consumer.consume(handoff);

        let ServerHeartbeatRttOffsetMetricsSnapshotConsumerResult::FutureDashboard(input) =
            consumed
        else {
            panic!("expected dashboard consumer input");
        };
        assert_eq!(input.exported_at, Some(TimestampMicros(23_100)));
        assert_eq!(input.records.len(), 1);
        assert_eq!(input.records[0].client_id, ClientId("client-3".to_string()));
        assert_eq!(input.records[0].clock_offset_delta_rejections, 1);
    }

    #[test]
    fn heartbeat_rtt_offset_metrics_snapshot_consumer_preserves_future_loop_handoff() {
        let snapshot = ServerHeartbeatRttOffsetRejectedCandidateMetricsSnapshot {
            records: vec![
                ServerHeartbeatRttOffsetRejectedCandidateMetricsExportRecord {
                    client_id: ClientId("client-4".to_string()),
                    run_id: RunId("run-4".to_string()),
                    total_rejected_candidates: 2,
                    total_skipped_commits: 2,
                    rtt_delta_rejections: 2,
                    clock_offset_delta_rejections: 0,
                    last_updated_at: Some(TimestampMicros(24_000)),
                },
            ],
        };
        let handoff = ServerHeartbeatRttOffsetMetricsSnapshotExportHandoff {
            consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureLoop,
            exported_at: Some(TimestampMicros(24_100)),
            snapshot,
        };
        let boundary = ServerHeartbeatRttOffsetMetricsSnapshotConsumerBoundary;

        let result = boundary.consume(handoff);

        let ServerHeartbeatRttOffsetMetricsSnapshotConsumerResult::FutureLoop(loop_handoff) =
            result
        else {
            panic!("expected future loop handoff");
        };
        assert_eq!(loop_handoff.exported_at, Some(TimestampMicros(24_100)));
        assert_eq!(loop_handoff.snapshot.records.len(), 1);
        assert_eq!(
            loop_handoff.snapshot.records[0].run_id,
            RunId("run-4".to_string())
        );
    }

    #[test]
    fn heartbeat_continuous_loop_policy_runs_timeout_and_metrics_when_both_due() {
        let input = ServerHeartbeatContinuousLoopPolicyInput {
            now: TimestampMicros(10_000),
            cadence: ServerHeartbeatContinuousLoopCadenceInput {
                timeout_tick_interval_micros: 1_000,
                metrics_snapshot_interval_micros: Some(5_000),
            },
            stop_condition: ServerHeartbeatContinuousLoopStopCondition::RunUntilStopped,
            state: ServerHeartbeatContinuousLoopStateSnapshot {
                completed_timeout_ticks: 1,
                exported_metrics_snapshots: 1,
                last_timeout_tick_at: Some(TimestampMicros(9_000)),
                last_metrics_snapshot_at: Some(TimestampMicros(5_000)),
                stop_requested: false,
            },
        };

        let action = ServerHeartbeatContinuousLoopPolicyBoundary.evaluate(input);

        let ServerHeartbeatContinuousLoopPolicyAction::Run {
            run_timeout_tick,
            export_metrics_snapshot,
            log,
        } = action
        else {
            panic!("server heartbeat loop policy should run due work");
        };
        assert!(run_timeout_tick);
        assert!(export_metrics_snapshot);
        assert_eq!(
            log.reason,
            ServerHeartbeatContinuousLoopPolicyReason::TimeoutTickAndMetricsSnapshotDue
        );
    }

    #[test]
    fn heartbeat_continuous_loop_policy_waits_for_earliest_due_work() {
        let input = ServerHeartbeatContinuousLoopPolicyInput {
            now: TimestampMicros(10_000),
            cadence: ServerHeartbeatContinuousLoopCadenceInput {
                timeout_tick_interval_micros: 3_000,
                metrics_snapshot_interval_micros: Some(5_000),
            },
            stop_condition: ServerHeartbeatContinuousLoopStopCondition::RunUntilStopped,
            state: ServerHeartbeatContinuousLoopStateSnapshot {
                completed_timeout_ticks: 1,
                exported_metrics_snapshots: 1,
                last_timeout_tick_at: Some(TimestampMicros(8_000)),
                last_metrics_snapshot_at: Some(TimestampMicros(6_000)),
                stop_requested: false,
            },
        };

        let action = ServerHeartbeatContinuousLoopPolicyBoundary.evaluate(input);

        let ServerHeartbeatContinuousLoopPolicyAction::Wait {
            next_wakeup_at,
            log,
        } = action
        else {
            panic!("server heartbeat loop policy should wait before cadence is due");
        };
        assert_eq!(next_wakeup_at, TimestampMicros(11_000));
        assert_eq!(
            log.reason,
            ServerHeartbeatContinuousLoopPolicyReason::WaitingForCadence
        );
    }

    #[test]
    fn heartbeat_continuous_loop_policy_stops_after_max_timeout_ticks() {
        let input = ServerHeartbeatContinuousLoopPolicyInput {
            now: TimestampMicros(10_000),
            cadence: ServerHeartbeatContinuousLoopCadenceInput {
                timeout_tick_interval_micros: 1_000,
                metrics_snapshot_interval_micros: None,
            },
            stop_condition: ServerHeartbeatContinuousLoopStopCondition::MaxTimeoutTicks {
                max_timeout_ticks: 2,
            },
            state: ServerHeartbeatContinuousLoopStateSnapshot {
                completed_timeout_ticks: 2,
                exported_metrics_snapshots: 0,
                last_timeout_tick_at: Some(TimestampMicros(9_000)),
                last_metrics_snapshot_at: None,
                stop_requested: false,
            },
        };

        let action = ServerHeartbeatContinuousLoopPolicyBoundary.evaluate(input);

        let ServerHeartbeatContinuousLoopPolicyAction::Stop { reason, log } = action else {
            panic!("server heartbeat loop policy should stop at max timeout ticks");
        };
        assert_eq!(
            reason,
            ServerHeartbeatContinuousLoopPolicyReason::MaxTimeoutTicksReached
        );
        assert_eq!(
            log.reason,
            ServerHeartbeatContinuousLoopPolicyReason::MaxTimeoutTicksReached
        );
    }

    #[test]
    fn heartbeat_continuous_loop_ownership_boundary_reports_missing_state() {
        let input = ServerHeartbeatContinuousLoopOwnershipInput {
            registry_available: true,
            liveness_state_available: false,
            outbound_queue_available: true,
            timeout_log_writer_available: true,
            rejected_candidate_metrics_state_available: false,
        };

        let decision = ServerHeartbeatContinuousLoopOwnershipBoundary.evaluate(input);

        let ServerHeartbeatContinuousLoopOwnershipDecision::NotReady { missing } = decision else {
            panic!("missing state holders should not be ready");
        };
        assert_eq!(
            missing,
            vec![
                ServerHeartbeatContinuousLoopOwnershipMissing::LivenessState,
                ServerHeartbeatContinuousLoopOwnershipMissing::RejectedCandidateMetricsState,
            ]
        );
    }

    #[test]
    fn heartbeat_continuous_loop_socket_receive_timeout_clamps_to_next_work() {
        let decision = ServerHeartbeatContinuousLoopSocketReceiveTimeoutBoundary.plan_wait(
            ServerHeartbeatContinuousLoopSocketReceiveTimeoutInput {
                now: TimestampMicros(10_000),
                next_heartbeat_work_due_at: TimestampMicros(12_500),
                max_socket_receive_timeout_micros: 5_000,
            },
        );

        assert_eq!(
            decision,
            ServerHeartbeatContinuousLoopSocketReceiveTimeoutDecision::Wait {
                receive_timeout_micros: 2_500,
                heartbeat_work_due_at: TimestampMicros(12_500),
            }
        );
    }

    #[test]
    fn heartbeat_continuous_loop_retry_boundary_schedules_next_attempt() {
        let decision = ServerHeartbeatContinuousLoopRetryBoundary.decide(
            ServerHeartbeatContinuousLoopRetryInput {
                reason: ServerHeartbeatContinuousLoopRetryReason::SocketReceiveInterrupted,
                attempts_used: 1,
                policy: ServerHeartbeatContinuousLoopRetryPolicy {
                    max_attempts: 3,
                    retry_delay_micros: 750,
                },
                now: TimestampMicros(10_000),
            },
        );

        assert_eq!(
            decision,
            ServerHeartbeatContinuousLoopRetryDecision::RetryLater {
                reason: ServerHeartbeatContinuousLoopRetryReason::SocketReceiveInterrupted,
                next_attempt: 2,
                retry_at: TimestampMicros(10_750),
            }
        );
    }

    #[test]
    fn heartbeat_continuous_loop_body_emits_timeout_and_metrics_handoffs() {
        let input = ServerHeartbeatContinuousLoopBodyInput {
            ownership: ServerHeartbeatContinuousLoopOwnershipInput {
                registry_available: true,
                liveness_state_available: true,
                outbound_queue_available: true,
                timeout_log_writer_available: true,
                rejected_candidate_metrics_state_available: true,
            },
            policy: ServerHeartbeatContinuousLoopPolicyInput {
                now: TimestampMicros(10_000),
                cadence: ServerHeartbeatContinuousLoopCadenceInput {
                    timeout_tick_interval_micros: 1_000,
                    metrics_snapshot_interval_micros: Some(5_000),
                },
                stop_condition: ServerHeartbeatContinuousLoopStopCondition::RunUntilStopped,
                state: ServerHeartbeatContinuousLoopStateSnapshot {
                    completed_timeout_ticks: 1,
                    exported_metrics_snapshots: 1,
                    last_timeout_tick_at: Some(TimestampMicros(9_000)),
                    last_metrics_snapshot_at: Some(TimestampMicros(5_000)),
                    stop_requested: false,
                },
            },
            max_socket_receive_timeout_micros: 2_000,
            metrics_snapshot_consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureLoop,
        };

        let result = ServerHeartbeatContinuousLoopBodyBoundary::default().run_one(input);

        let ServerHeartbeatContinuousLoopBodyResult::Run { handoff, log } = result else {
            panic!("server heartbeat body should emit due handoffs");
        };
        assert_eq!(
            handoff.timeout_tick,
            Some(ServerHeartbeatContinuousLoopTimeoutTickHandoff {
                evaluated_at: TimestampMicros(10_000),
            })
        );
        assert_eq!(
            handoff.metrics_snapshot,
            Some(ServerHeartbeatContinuousLoopMetricsSnapshotHandoff {
                exported_at: TimestampMicros(10_000),
                consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureLoop,
            })
        );
        assert_eq!(
            log.reason,
            ServerHeartbeatContinuousLoopPolicyReason::TimeoutTickAndMetricsSnapshotDue
        );
    }

    #[test]
    fn heartbeat_continuous_loop_body_waits_with_socket_timeout() {
        let input = ServerHeartbeatContinuousLoopBodyInput {
            ownership: ServerHeartbeatContinuousLoopOwnershipInput {
                registry_available: true,
                liveness_state_available: true,
                outbound_queue_available: true,
                timeout_log_writer_available: true,
                rejected_candidate_metrics_state_available: true,
            },
            policy: ServerHeartbeatContinuousLoopPolicyInput {
                now: TimestampMicros(10_000),
                cadence: ServerHeartbeatContinuousLoopCadenceInput {
                    timeout_tick_interval_micros: 3_000,
                    metrics_snapshot_interval_micros: None,
                },
                stop_condition: ServerHeartbeatContinuousLoopStopCondition::RunUntilStopped,
                state: ServerHeartbeatContinuousLoopStateSnapshot {
                    completed_timeout_ticks: 1,
                    exported_metrics_snapshots: 0,
                    last_timeout_tick_at: Some(TimestampMicros(8_000)),
                    last_metrics_snapshot_at: None,
                    stop_requested: false,
                },
            },
            max_socket_receive_timeout_micros: 500,
            metrics_snapshot_consumer: ServerHeartbeatRttOffsetMetricsSnapshotConsumer::FutureLoop,
        };

        let result = ServerHeartbeatContinuousLoopBodyBoundary::default().run_one(input);

        let ServerHeartbeatContinuousLoopBodyResult::Wait { socket_wait, log } = result else {
            panic!("server heartbeat body should wait before cadence is due");
        };
        assert_eq!(
            socket_wait,
            ServerHeartbeatContinuousLoopSocketReceiveTimeoutDecision::Wait {
                receive_timeout_micros: 500,
                heartbeat_work_due_at: TimestampMicros(11_000),
            }
        );
        assert_eq!(
            log.reason,
            ServerHeartbeatContinuousLoopPolicyReason::WaitingForCadence
        );
    }

    #[test]
    fn heartbeat_client_ack_observation_boundary_maps_protocol_observation() {
        let observation = HeartbeatAckObservation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
            client_received_at: TimestampMicros(1_150),
        };
        let boundary = ServerHeartbeatClientAckObservationBoundary;

        let server_observation = boundary.prepare(observation);

        assert_eq!(
            server_observation.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(server_observation.run_id, RunId("run-1".to_string()));
        assert_eq!(
            server_observation.echoed_client_sent_at,
            TimestampMicros(1_000)
        );
        assert_eq!(
            server_observation.server_received_at,
            TimestampMicros(2_100)
        );
        assert_eq!(server_observation.server_sent_at, TimestampMicros(2_150));
        assert_eq!(
            server_observation.client_received_at,
            TimestampMicros(1_150)
        );
    }

    #[test]
    fn heartbeat_observation_carrier_boundary_unwraps_client_stats_carrier() {
        let observation = HeartbeatAckObservation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
            client_received_at: TimestampMicros(1_150),
        };
        let carrier = HeartbeatObservationCarrier {
            message_type: MessageType::ClientStats,
            protocol_version: ProtocolVersion(2),
            observation,
        };
        let boundary = ServerHeartbeatObservationCarrierBoundary::default();

        let server_observation = boundary.prepare(carrier);

        assert_eq!(
            server_observation.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(server_observation.run_id, RunId("run-1".to_string()));
        assert_eq!(
            server_observation.echoed_client_sent_at,
            TimestampMicros(1_000)
        );
        assert_eq!(
            server_observation.server_received_at,
            TimestampMicros(2_100)
        );
        assert_eq!(server_observation.server_sent_at, TimestampMicros(2_150));
        assert_eq!(
            server_observation.client_received_at,
            TimestampMicros(1_150)
        );
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

    #[test]
    fn routes_video_frame_fragment_to_fragment_boundary() {
        let source = packet_source();
        let fragment = video_frame_fragment("client-1", 0, 1, vec![0xaa]);
        let router = ServerInboundRouter;

        let route = router.route(DecodedInboundPacket {
            source,
            message: ProtocolMessage::VideoFrameFragment(fragment.clone()),
        });

        assert_eq!(
            route,
            ServerInboundRoute::VideoFrameFragment { source, fragment }
        );
    }

    #[test]
    fn routes_client_stats_to_stats_boundary() {
        let source = packet_source();
        let stats = client_stats("client-1");
        let router = ServerInboundRouter;

        let route = router.route(DecodedInboundPacket {
            source,
            message: ProtocolMessage::ClientStats(stats.clone()),
        });

        assert_eq!(route, ServerInboundRoute::ClientStats { source, stats });
    }

    #[test]
    fn packet_acceptance_gate_checks_client_stats_sender() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let gate = PacketAcceptanceGateBoundary::default();
        let route = client_stats_route("client-1", source);

        let decision = gate.evaluate_route(&registry, &route);

        assert_eq!(decision, PacketAcceptanceDecision::Accepted);
    }

    #[test]
    fn packet_acceptance_gate_checks_video_frame_fragment_sender() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let gate = PacketAcceptanceGateBoundary::default();
        let route = ServerInboundRoute::VideoFrameFragment {
            source,
            fragment: video_frame_fragment("client-1", 0, 1, vec![0xaa]),
        };

        let decision = gate.evaluate_route(&registry, &route);

        assert_eq!(decision, PacketAcceptanceDecision::Accepted);
    }

    #[test]
    fn registered_packet_boundary_prepares_client_stats_for_handler() {
        let source = packet_source();
        let registry = registry_with_client("client-1", source);
        let boundary = ServerRegisteredPacketBoundary::default();
        let route = client_stats_route("client-1", source);

        let registered = boundary
            .prepare_for_handler(&registry, route)
            .expect("client stats should be accepted");

        let ServerRegisteredClientPacket::ClientStats(packet) = registered else {
            panic!("expected registered ClientStats packet");
        };
        assert_eq!(packet.source, source);
        assert_eq!(
            packet.authenticated_sender.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(packet.stats.message_type, MessageType::ClientStats);
        assert_eq!(packet.stats.capture_fps, 30);
    }

    #[test]
    fn client_stats_handler_boundary_extracts_metrics_and_observation() {
        let source = packet_source();
        let authenticated_sender = AuthenticatedSenderEntry {
            client_id: ClientId("client-1".to_string()),
            source,
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            registered_at: Some(TimestampMicros(1_000)),
        };
        let packet = ServerRegisteredClientStatsPacket {
            source,
            authenticated_sender: authenticated_sender.clone(),
            stats: client_stats_with_observation("client-1"),
        };
        let boundary = ServerClientStatsHandlerBoundary::default();

        let input = boundary.prepare_input(packet);

        assert_eq!(input.state.source, source);
        assert_eq!(input.state.authenticated_sender, authenticated_sender);
        assert_eq!(input.state.capture_fps, 30);
        assert_eq!(input.state.dropped_frames, 2);
        assert_eq!(input.state.bitrate_kbps, 4500);
        let observation = input
            .heartbeat_observation
            .expect("heartbeat observation should be preserved");
        assert_eq!(observation.client_id, ClientId("client-1".to_string()));
        assert_eq!(
            observation.echoed_client_sent_at,
            TimestampMicros(2_000_000)
        );
        assert_eq!(observation.client_received_at, TimestampMicros(2_000_250));
    }

    fn packet_source() -> PacketSource {
        "127.0.0.1:5000"
            .parse::<SocketAddr>()
            .expect("source address should parse")
            .into()
    }

    fn packet_source_at(port: u16) -> PacketSource {
        SocketAddr::from(([127, 0, 0, 1], port)).into()
    }

    fn send_log_context() -> OutboundSendLogContext {
        OutboundSendLogContext {
            destination: packet_source().into(),
            message_type: MessageType::HeartbeatAck,
            run_id: Some(RunId("run-1".to_string())),
            client_id: Some(ClientId("client-1".to_string())),
        }
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
        register_test_client(&mut registry, client_id, source);
        registry
    }

    fn register_test_client(
        registry: &mut AuthenticatedSenderRegistry,
        client_id: &str,
        source: PacketSource,
    ) {
        AuthenticatedSenderRegistryBoundary.register(
            registry,
            AuthenticatedSenderRegistration {
                client_id: ClientId(client_id.to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            },
        );
    }

    fn commit_liveness_for_test(
        state: &mut ServerHeartbeatLivenessState,
        client_id: &str,
        source: PacketSource,
        server_received_at: TimestampMicros,
    ) {
        ServerHeartbeatLivenessCommitBoundary.commit(
            state,
            ServerHeartbeatStateInput {
                source,
                authenticated_sender: AuthenticatedSenderEntry {
                    client_id: ClientId(client_id.to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(2),
                    registered_at: None,
                },
                client_id: ClientId(client_id.to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                heartbeat_sent_at: TimestampMicros(server_received_at.0.saturating_sub(100)),
                server_received_at,
                short_status: None,
            },
        );
    }

    fn body_result_with_handler_handoff(
        packet_len: usize,
        handler: ServerContinuousReceiveLoopHandlerHandoffRuntimePlan,
    ) -> ServerContinuousReceiveLoopBodyResult {
        let lifecycle = ServerContinuousReceiveLoopLifecyclePlan {
            state: ServerContinuousReceiveLoopLifecycleState::DispatchingAcceptedRoute,
            action: ServerContinuousReceiveLoopAction::DispatchAcceptedRoute,
            operational_log_required: true,
            rejection_log_required: false,
            handler_handoff_required: true,
        };
        let tick = ServerContinuousReceiveLoopTickPlan {
            state: ServerContinuousReceiveLoopTickState::AcceptedRouteReadyForHandoff,
            lifecycle,
            packet_len: Some(packet_len),
        };
        let writer = ServerContinuousReceiveLoopWriterRuntimeResult {
            handoff: ServerContinuousReceiveLoopWriterHandoffPlan {
                handler_handoff_required: true,
                tick,
                operational_log: None,
                rejection_log: None,
            },
            operational_event: None,
            rejection_event: None,
        };

        ServerContinuousReceiveLoopBodyResult {
            action: ServerContinuousReceiveLoopBodyAction::ExecuteOneTick,
            tick: ServerContinuousReceiveLoopOneTickRuntimeResult {
                start_tick: tick,
                outcome: ServerContinuousReceiveLoopOneTickRuntimeOutcome::Completed {
                    packet_len,
                    handler: ServerContinuousReceiveLoopHandlerHandoffRuntimeResult {
                        writer,
                        handler,
                    },
                },
            },
        }
    }

    fn body_result_with_gate_rejection(
        packet_len: usize,
        rejection: ServerReceiveLoopGateRejection,
    ) -> ServerContinuousReceiveLoopBodyResult {
        let lifecycle = ServerContinuousReceiveLoopLifecyclePlan {
            state: ServerContinuousReceiveLoopLifecycleState::PreparingRejectionLogs,
            action: ServerContinuousReceiveLoopAction::PrepareRejectionLogs,
            operational_log_required: false,
            rejection_log_required: true,
            handler_handoff_required: false,
        };
        let tick = ServerContinuousReceiveLoopTickPlan {
            state: ServerContinuousReceiveLoopTickState::RejectionReadyForLogs,
            lifecycle,
            packet_len: Some(packet_len),
        };
        let writer = ServerContinuousReceiveLoopWriterRuntimeResult {
            handoff: ServerContinuousReceiveLoopWriterHandoffPlan {
                handler_handoff_required: false,
                tick,
                operational_log: None,
                rejection_log: Some(rejection),
            },
            operational_event: None,
            rejection_event: None,
        };

        ServerContinuousReceiveLoopBodyResult {
            action: ServerContinuousReceiveLoopBodyAction::ExecuteOneTick,
            tick: ServerContinuousReceiveLoopOneTickRuntimeResult {
                start_tick: tick,
                outcome: ServerContinuousReceiveLoopOneTickRuntimeOutcome::Completed {
                    packet_len,
                    handler: ServerContinuousReceiveLoopHandlerHandoffRuntimeResult {
                        writer,
                        handler: ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::NotRequired(
                            ServerContinuousReceiveLoopHandlerHandoffSkipReason::RejectedOutcome,
                        ),
                    },
                },
            },
        }
    }

    fn video_frame_queue_runtime_body_for_frame(
        client_id: &str,
        source: PacketSource,
        frame_id: u64,
    ) -> ServerContinuousReceiveLoopBodyResult {
        let registry = registry_with_client(client_id, source);
        let mut route = video_frame_route(client_id, source);
        if let ServerInboundRoute::VideoFrame { frame, .. } = &mut route {
            frame.frame_id = frame_id;
        }
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, route)
            .expect("video frame should be accepted");
        body_result_with_handler_handoff(
            128,
            ServerContinuousReceiveLoopHandlerHandoffRuntimePlan::RegisteredClient(registered),
        )
    }

    fn video_frame_side_effect_from_body(
        body: &ServerContinuousReceiveLoopBodyResult,
    ) -> ServerDispatchRuntimeSideEffectApplyOutcome {
        let dispatch = ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary::default()
            .dispatch_body_result(body, &auth_config(None), body_dispatch_timing());
        ServerDispatchRuntimeSideEffectApplyBoundary::default()
            .apply_body_dispatch_outcome(&mut AuthenticatedSenderRegistry::default(), dispatch)
    }

    fn body_dispatch_timing() -> ServerHeartbeatAckTiming {
        ServerHeartbeatAckTiming {
            server_received_at: TimestampMicros(2_000_100),
            server_sent_at: TimestampMicros(2_000_200),
        }
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

    fn video_frame_fragment(
        client_id: &str,
        chunk_index: u32,
        chunk_count: u32,
        chunk_payload: Vec<u8>,
    ) -> VideoFrameFragment {
        VideoFrameFragment {
            message_type: MessageType::VideoFrameFragment,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId(client_id.to_string()),
            run_id: RunId("run-1".to_string()),
            frame_id: 42,
            capture_timestamp: TimestampMicros(1_000_000),
            width: 1280,
            height: 720,
            fps_nominal: 30,
            total_payload_len: 5,
            chunk_index,
            chunk_count,
            chunk_payload_len: chunk_payload.len(),
            chunk_payload,
        }
    }

    fn registered_video_frame_fragment(
        source: PacketSource,
        chunk_index: u32,
        chunk_count: u32,
        chunk_payload: Vec<u8>,
    ) -> ServerRegisteredVideoFrameFragmentPacket {
        ServerRegisteredVideoFrameFragmentPacket {
            source,
            authenticated_sender: AuthenticatedSenderEntry {
                client_id: ClientId("client-1".to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                registered_at: None,
            },
            fragment: video_frame_fragment("client-1", chunk_index, chunk_count, chunk_payload),
        }
    }

    fn video_frame_packet(client_id: &str, frame_id: u64) -> Vec<u8> {
        let ServerInboundRoute::VideoFrame { mut frame, .. } =
            video_frame_route(client_id, packet_source())
        else {
            panic!("expected video frame route");
        };
        frame.frame_id = frame_id;
        let mut output = Vec::new();
        ProtocolMessageEncoderBoundary
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &ProtocolMessage::VideoFrame(frame),
                &mut output,
            )
            .expect("video frame should encode");
        output
    }

    fn video_handler_input(client_id: &str, source: PacketSource) -> ServerVideoFrameHandlerInput {
        let registry = registry_with_client(client_id, source);
        let registered = ServerRegisteredPacketBoundary::default()
            .prepare_for_handler(&registry, video_frame_route(client_id, source))
            .expect("video frame should be accepted");
        let ServerRegisteredClientPacket::VideoFrame(packet) = registered else {
            panic!("expected registered video frame packet");
        };
        ServerVideoFrameHandlerBoundary.prepare_input(packet)
    }

    fn store_video_frame_for_read_test(
        queue_state: &mut ServerVideoFrameQueueState,
        client_id: &str,
        run_id: &str,
        frame_id: u64,
    ) {
        let source = packet_source();
        let mut input = video_handler_input(client_id, source);
        input.registered_packet.frame.run_id = RunId(run_id.to_string());
        input.registered_packet.frame.frame_id = frame_id;
        input.registered_packet.authenticated_sender.run_id = RunId(run_id.to_string());

        let result = ServerVideoFrameQueueStorageBoundary.store_frame(
            queue_state,
            input,
            TimestampMicros(3_000_000 + frame_id),
            ServerVideoFrameQueuePolicy::default(),
        );
        assert!(matches!(
            result,
            ServerVideoFrameQueueStorageResult::Stored { .. }
        ));
    }

    fn store_video_frame_for_handoff_test(
        queue_state: &mut ServerVideoFrameQueueState,
        client_id: &str,
        run_id: &str,
        frame_id: u64,
        queued_at: TimestampMicros,
        width: u32,
        height: u32,
        payload: Vec<u8>,
    ) {
        let source = packet_source();
        let mut input = video_handler_input(client_id, source);
        input.registered_packet.frame.run_id = RunId(run_id.to_string());
        input.registered_packet.frame.frame_id = frame_id;
        input.registered_packet.frame.capture_timestamp = TimestampMicros(1_000_000 + frame_id);
        input.registered_packet.frame.send_timestamp = TimestampMicros(1_000_100 + frame_id);
        input.registered_packet.frame.width = width;
        input.registered_packet.frame.height = height;
        input.registered_packet.frame.payload = payload;
        input.registered_packet.frame.codec = Codec::H264;
        input.registered_packet.authenticated_sender.run_id = RunId(run_id.to_string());
        input.payload_len = input.registered_packet.frame.payload.len();

        let result = ServerVideoFrameQueueStorageBoundary.store_frame(
            queue_state,
            input,
            queued_at,
            ServerVideoFrameQueuePolicy::default(),
        );
        assert!(matches!(
            result,
            ServerVideoFrameQueueStorageResult::Stored { .. }
        ));
    }

    fn run_controller_once_for_test(
        socket: &UdpSocket,
        receive_buffer: &mut [u8],
        registry: &mut AuthenticatedSenderRegistry,
        auth_config: &ServerAuthConfig,
    ) -> ServerControllerReceiveSendRuntimeResult {
        let mut queue_collection = ServerOutboundQueueCollection::default();
        ServerControllerReceiveSendRuntimeBoundary::default()
            .run_once(
                socket,
                receive_buffer,
                registry,
                &mut queue_collection,
                auth_config,
                ServerControllerReceiveSendRuntimeInput {
                    controller: ServerContinuousReceiveLoopControllerInput {
                        expected_protocol_version: ProtocolVersion(2),
                        timestamp: TimestampMicros(3_000_000),
                        continue_requested: true,
                    },
                    heartbeat_timing: body_dispatch_timing(),
                    encode_context: EncodeContext {
                        protocol_version: ProtocolVersion(2),
                    },
                    auth_log_timestamp: TimestampMicros(3_000_000),
                    send_log_timestamp: TimestampMicros(3_000_000),
                },
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
            .expect("controller runtime should complete")
    }

    fn queue_from_controller_result_for_test(
        result: &ServerControllerReceiveSendRuntimeResult,
        queue_state: &mut ServerVideoFrameQueueState,
    ) -> ServerVideoFrameQueueRuntimeResult {
        let ServerControllerReceiveSendRuntimeResult::Iteration { iteration, .. } = result else {
            panic!("controller should run one iteration");
        };
        ServerVideoFrameQueueRuntimeBoundary::default().store_from_receive_side_effect(
            queue_state,
            &iteration.body,
            iteration.side_effect.clone(),
            TimestampMicros(3_000_100),
            ServerVideoFrameQueuePolicy::default(),
        )
    }

    fn client_stats_route(client_id: &str, source: PacketSource) -> ServerInboundRoute {
        ServerInboundRoute::ClientStats {
            source,
            stats: client_stats(client_id),
        }
    }

    fn client_stats(client_id: &str) -> ClientStats {
        ClientStats {
            message_type: MessageType::ClientStats,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId(client_id.to_string()),
            run_id: RunId("run-1".to_string()),
            sent_at: TimestampMicros(3_000_000),
            capture_fps: 30,
            dropped_frames: 2,
            bitrate_kbps: 4500,
            heartbeat_observation: None,
        }
    }

    fn client_stats_with_observation(client_id: &str) -> ClientStats {
        let mut stats = client_stats(client_id);
        stats.heartbeat_observation = Some(HeartbeatAckObservation {
            client_id: ClientId(client_id.to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(2_000_000),
            server_received_at: TimestampMicros(2_000_100),
            server_sent_at: TimestampMicros(2_000_150),
            client_received_at: TimestampMicros(2_000_250),
        });
        stats
    }

    fn test_secret_store_reference(secret_id: &str) -> SecretStoreSecretRef {
        SecretStoreSecretRef {
            store_id: "local-dev-store".to_string(),
            secret_id: secret_id.to_string(),
            version: Some("current".to_string()),
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
