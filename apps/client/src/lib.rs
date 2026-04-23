use std::{
    fs, io,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use stream_sync_protocol::{
    decode_fixed_header, decode_payload_by_message_type, validate_protocol_version, AppVersion,
    AuthRequest, AuthResponse, ClientId, ClientStats, DecodeContext, EncodeContext, Heartbeat,
    HeartbeatAck, HeartbeatAckObservation, HeartbeatAckObservationBoundary,
    HeartbeatObservationCarrier, HeartbeatObservationCarrierBoundary, MessageEncoder, MessageType,
    ProtocolError, ProtocolMessage, ProtocolMessageEncoderBoundary, ProtocolVersion, RunId,
    TimestampMicros,
};

const DEFAULT_AUTH_RESPONSE_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_HEARTBEAT_INTERVAL_MS: u32 = 1_000;
const DEFAULT_ONE_TICK_RETRY_ATTEMPTS: u32 = 3;
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

/// One-shot client-side auth + heartbeat PoC.
///
/// This keeps a single UDP socket, sends one `AuthRequest`, waits for one
/// accepted `AuthResponse`, then sends one `Heartbeat` and waits for one
/// `HeartbeatAck`. It does not start a continuous heartbeat loop, retry,
/// fragment, encrypt, send video, or introduce an async runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientAuthHeartbeatPocLauncher {
    encoder: ProtocolMessageEncoderBoundary,
}

impl ClientAuthHeartbeatPocLauncher {
    pub fn load_startup_config_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ClientAuthHeartbeatPocStartupConfig, ClientAuthHeartbeatPocError> {
        let auth_config = ClientAuthRequestPocLauncher::default()
            .load_startup_config_from_path(path)
            .map_err(ClientAuthHeartbeatPocError::AuthPoc)?;
        Ok(ClientAuthHeartbeatPocStartupConfig {
            destination: auth_config.destination,
            response_timeout_ms: auth_config.response_timeout_ms,
            request: auth_config.request,
            heartbeat_short_status: Some("poc-once".to_string()),
        })
    }

    pub fn run_once_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ClientAuthHeartbeatPocOutcome, ClientAuthHeartbeatPocError> {
        let startup_config = self.load_startup_config_from_path(path)?;
        self.run_once(startup_config)
    }

    pub fn run_once(
        &self,
        startup_config: ClientAuthHeartbeatPocStartupConfig,
    ) -> Result<ClientAuthHeartbeatPocOutcome, ClientAuthHeartbeatPocError> {
        let socket = UdpSocket::bind(ephemeral_bind_address(startup_config.destination))
            .map_err(|error| ClientAuthHeartbeatPocError::Bind(error.kind()))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(
                startup_config.response_timeout_ms,
            )))
            .map_err(|error| ClientAuthHeartbeatPocError::SetReadTimeout(error.kind()))?;

        let mut auth_request_bytes = Vec::new();
        let context = EncodeContext {
            protocol_version: startup_config.request.protocol_version,
        };
        self.encoder
            .encode_message(
                context,
                &ProtocolMessage::AuthRequest(startup_config.request.clone()),
                &mut auth_request_bytes,
            )
            .map_err(ClientAuthHeartbeatPocError::Encode)?;
        let auth_request_bytes_sent = socket
            .send_to(&auth_request_bytes, startup_config.destination)
            .map_err(|error| ClientAuthHeartbeatPocError::Send(error.kind()))?;

        let (auth_response_source, auth_response_bytes, auth_response) =
            receive_auth_response(&socket, startup_config.request.protocol_version)
                .map_err(ClientAuthHeartbeatPocError::AuthResponse)?;
        if !auth_response.accepted {
            return Err(ClientAuthHeartbeatPocError::AuthRejected(auth_response));
        }

        let sent_at = current_timestamp_micros();
        let heartbeat = Heartbeat {
            message_type: MessageType::Heartbeat,
            protocol_version: startup_config.request.protocol_version,
            client_id: startup_config.request.client_id.clone(),
            run_id: startup_config.request.run_id.clone(),
            sent_at,
            local_time: Some(sent_at),
            short_status: startup_config.heartbeat_short_status,
        };
        let mut heartbeat_bytes = Vec::new();
        self.encoder
            .encode_message(
                context,
                &ProtocolMessage::Heartbeat(heartbeat.clone()),
                &mut heartbeat_bytes,
            )
            .map_err(ClientAuthHeartbeatPocError::Encode)?;
        let heartbeat_bytes_sent = socket
            .send_to(&heartbeat_bytes, startup_config.destination)
            .map_err(|error| ClientAuthHeartbeatPocError::Send(error.kind()))?;

        let (heartbeat_ack_source, heartbeat_ack_bytes, heartbeat_ack) =
            receive_heartbeat_ack(&socket, heartbeat.protocol_version)
                .map_err(ClientAuthHeartbeatPocError::HeartbeatAck)?;

        Ok(ClientAuthHeartbeatPocOutcome {
            destination: startup_config.destination,
            request: startup_config.request,
            auth_request_bytes,
            auth_request_bytes_sent,
            auth_response_source,
            auth_response_bytes,
            auth_response,
            heartbeat,
            heartbeat_bytes,
            heartbeat_bytes_sent,
            heartbeat_ack_source,
            heartbeat_ack_bytes,
            heartbeat_ack,
        })
    }
}

/// Convenience entry point for the client binary and manual PoC wiring.
pub fn run_auth_heartbeat_poc_once_from_path(
    path: impl AsRef<Path>,
) -> Result<ClientAuthHeartbeatPocOutcome, ClientAuthHeartbeatPocError> {
    ClientAuthHeartbeatPocLauncher::default().run_once_from_path(path)
}

/// One-shot client-side auth + heartbeat + stats observation PoC.
///
/// This keeps a single UDP socket, sends one `AuthRequest`, one `Heartbeat`,
/// observes one `HeartbeatAck`, wraps that observation in the `ClientStats`
/// carrier, and sends one `ClientStats` packet. It does not start continuous
/// heartbeat/stats loops, send video, retry, fragment, encrypt, or introduce
/// an async runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientAuthHeartbeatStatsPocLauncher {
    encoder: ProtocolMessageEncoderBoundary,
}

impl ClientAuthHeartbeatStatsPocLauncher {
    pub fn load_startup_config_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ClientAuthHeartbeatPocStartupConfig, ClientAuthHeartbeatPocError> {
        ClientAuthHeartbeatPocLauncher::default().load_startup_config_from_path(path)
    }

    pub fn run_once_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<ClientAuthHeartbeatStatsPocOutcome, ClientAuthHeartbeatPocError> {
        let startup_config = self.load_startup_config_from_path(path)?;
        self.run_once(startup_config)
    }

    pub fn run_once(
        &self,
        startup_config: ClientAuthHeartbeatPocStartupConfig,
    ) -> Result<ClientAuthHeartbeatStatsPocOutcome, ClientAuthHeartbeatPocError> {
        let socket = UdpSocket::bind(ephemeral_bind_address(startup_config.destination))
            .map_err(|error| ClientAuthHeartbeatPocError::Bind(error.kind()))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(
                startup_config.response_timeout_ms,
            )))
            .map_err(|error| ClientAuthHeartbeatPocError::SetReadTimeout(error.kind()))?;

        let mut auth_request_bytes = Vec::new();
        let context = EncodeContext {
            protocol_version: startup_config.request.protocol_version,
        };
        self.encoder
            .encode_message(
                context,
                &ProtocolMessage::AuthRequest(startup_config.request.clone()),
                &mut auth_request_bytes,
            )
            .map_err(ClientAuthHeartbeatPocError::Encode)?;
        let auth_request_bytes_sent = socket
            .send_to(&auth_request_bytes, startup_config.destination)
            .map_err(|error| ClientAuthHeartbeatPocError::Send(error.kind()))?;

        let (auth_response_source, auth_response_bytes, auth_response) =
            receive_auth_response(&socket, startup_config.request.protocol_version)
                .map_err(ClientAuthHeartbeatPocError::AuthResponse)?;
        if !auth_response.accepted {
            return Err(ClientAuthHeartbeatPocError::AuthRejected(auth_response));
        }

        let sent_at = current_timestamp_micros();
        let heartbeat = Heartbeat {
            message_type: MessageType::Heartbeat,
            protocol_version: startup_config.request.protocol_version,
            client_id: startup_config.request.client_id.clone(),
            run_id: startup_config.request.run_id.clone(),
            sent_at,
            local_time: Some(sent_at),
            short_status: startup_config.heartbeat_short_status,
        };
        let mut heartbeat_bytes = Vec::new();
        self.encoder
            .encode_message(
                context,
                &ProtocolMessage::Heartbeat(heartbeat.clone()),
                &mut heartbeat_bytes,
            )
            .map_err(ClientAuthHeartbeatPocError::Encode)?;
        let heartbeat_bytes_sent = socket
            .send_to(&heartbeat_bytes, startup_config.destination)
            .map_err(|error| ClientAuthHeartbeatPocError::Send(error.kind()))?;

        let (heartbeat_ack_source, heartbeat_ack_bytes, heartbeat_ack) =
            receive_heartbeat_ack(&socket, heartbeat.protocol_version)
                .map_err(ClientAuthHeartbeatPocError::HeartbeatAck)?;
        let heartbeat_ack_client_received_at = current_timestamp_micros();
        let heartbeat_ack_observation = ClientHeartbeatAckObservationBoundary::default()
            .observe_ack(&heartbeat_ack, heartbeat_ack_client_received_at);
        let heartbeat_observation_carrier = ClientHeartbeatObservationCarrierBoundary::default()
            .build_client_stats_carrier(
                heartbeat.protocol_version,
                heartbeat_ack_observation.clone(),
            );
        let client_stats = ClientStats {
            message_type: MessageType::ClientStats,
            protocol_version: heartbeat_observation_carrier.protocol_version,
            client_id: heartbeat_observation_carrier.observation.client_id.clone(),
            run_id: heartbeat_observation_carrier.observation.run_id.clone(),
            sent_at: current_timestamp_micros(),
            capture_fps: 0,
            dropped_frames: 0,
            bitrate_kbps: 0,
            heartbeat_observation: Some(heartbeat_observation_carrier.observation.clone()),
        };
        let mut client_stats_bytes = Vec::new();
        self.encoder
            .encode_message(
                context,
                &ProtocolMessage::ClientStats(client_stats.clone()),
                &mut client_stats_bytes,
            )
            .map_err(ClientAuthHeartbeatPocError::Encode)?;
        let client_stats_bytes_sent = socket
            .send_to(&client_stats_bytes, startup_config.destination)
            .map_err(|error| ClientAuthHeartbeatPocError::Send(error.kind()))?;

        Ok(ClientAuthHeartbeatStatsPocOutcome {
            heartbeat: ClientAuthHeartbeatPocOutcome {
                destination: startup_config.destination,
                request: startup_config.request,
                auth_request_bytes,
                auth_request_bytes_sent,
                auth_response_source,
                auth_response_bytes,
                auth_response,
                heartbeat,
                heartbeat_bytes,
                heartbeat_bytes_sent,
                heartbeat_ack_source,
                heartbeat_ack_bytes,
                heartbeat_ack,
            },
            heartbeat_ack_observation,
            heartbeat_observation_carrier,
            client_stats,
            client_stats_bytes,
            client_stats_bytes_sent,
        })
    }
}

/// Convenience entry point for the client binary and manual PoC wiring.
pub fn run_auth_heartbeat_stats_poc_once_from_path(
    path: impl AsRef<Path>,
) -> Result<ClientAuthHeartbeatStatsPocOutcome, ClientAuthHeartbeatPocError> {
    ClientAuthHeartbeatStatsPocLauncher::default().run_once_from_path(path)
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

/// How a future heartbeat loop should return ack observations to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatAckObservationReturnMode {
    Disabled,
    ClientStatsOncePerAck,
}

/// Cadence inputs for a future continuous client heartbeat loop.
///
/// This is data only. It does not sleep, receive acks, send stats, or start a
/// loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCadenceInput {
    pub heartbeat_interval_micros: u64,
    pub ack_receive_timeout_micros: u64,
    pub ack_observation_return: ClientHeartbeatAckObservationReturnMode,
}

/// Stop policy inputs for a future continuous client heartbeat loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopStopCondition {
    RunUntilStopped,
    MaxHeartbeats { max_sent_heartbeats: u64 },
    MaxMissedAcks { max_missed_acks: u64 },
}

/// Snapshot of loop state supplied by a future loop body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopStateSnapshot {
    pub sent_heartbeats: u64,
    pub received_acks: u64,
    pub missed_acks: u64,
    pub last_heartbeat_sent_at: Option<TimestampMicros>,
    pub stop_requested: bool,
}

/// Input for deciding the next future client heartbeat loop action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopPolicyInput {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub now: TimestampMicros,
    pub cadence: ClientHeartbeatLoopCadenceInput,
    pub stop_condition: ClientHeartbeatLoopStopCondition,
    pub state: ClientHeartbeatLoopStateSnapshot,
}

/// Reason a future client heartbeat loop policy reached its decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopPolicyReason {
    StopRequested,
    MaxHeartbeatsReached,
    MaxMissedAcksReached,
    WaitingForCadence,
    HeartbeatDue,
}

/// Typed log handoff for a future client heartbeat loop decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopLogHandoff {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub observed_at: TimestampMicros,
    pub reason: ClientHeartbeatLoopPolicyReason,
    pub heartbeat_interval_micros: u64,
    pub ack_receive_timeout_micros: u64,
    pub sent_heartbeats: u64,
    pub received_acks: u64,
    pub missed_acks: u64,
}

/// Next action selected for a future client heartbeat loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopPolicyAction {
    Stop {
        reason: ClientHeartbeatLoopPolicyReason,
        log: ClientHeartbeatLoopLogHandoff,
    },
    Wait {
        next_heartbeat_due_at: TimestampMicros,
        log: ClientHeartbeatLoopLogHandoff,
    },
    SendHeartbeat {
        send_at: TimestampMicros,
        ack_deadline_at: TimestampMicros,
        ack_observation_return: ClientHeartbeatAckObservationReturnMode,
        log: ClientHeartbeatLoopLogHandoff,
    },
}

/// Policy boundary for a future continuous client heartbeat loop.
///
/// This boundary decides only whether the next loop body should stop, wait, or
/// send one heartbeat. It does not sleep, send UDP packets, receive acks,
/// create `HeartbeatAckObservation`, send `ClientStats`, or write logs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopPolicyBoundary;

impl ClientHeartbeatLoopPolicyBoundary {
    pub fn evaluate(
        &self,
        input: ClientHeartbeatLoopPolicyInput,
    ) -> ClientHeartbeatLoopPolicyAction {
        if input.state.stop_requested {
            return ClientHeartbeatLoopPolicyAction::Stop {
                reason: ClientHeartbeatLoopPolicyReason::StopRequested,
                log: client_heartbeat_loop_log(
                    &input,
                    ClientHeartbeatLoopPolicyReason::StopRequested,
                ),
            };
        }

        match input.stop_condition {
            ClientHeartbeatLoopStopCondition::RunUntilStopped => {}
            ClientHeartbeatLoopStopCondition::MaxHeartbeats {
                max_sent_heartbeats,
            } if input.state.sent_heartbeats >= max_sent_heartbeats => {
                return ClientHeartbeatLoopPolicyAction::Stop {
                    reason: ClientHeartbeatLoopPolicyReason::MaxHeartbeatsReached,
                    log: client_heartbeat_loop_log(
                        &input,
                        ClientHeartbeatLoopPolicyReason::MaxHeartbeatsReached,
                    ),
                };
            }
            ClientHeartbeatLoopStopCondition::MaxMissedAcks { max_missed_acks }
                if input.state.missed_acks >= max_missed_acks =>
            {
                return ClientHeartbeatLoopPolicyAction::Stop {
                    reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                    log: client_heartbeat_loop_log(
                        &input,
                        ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                    ),
                };
            }
            ClientHeartbeatLoopStopCondition::MaxHeartbeats { .. }
            | ClientHeartbeatLoopStopCondition::MaxMissedAcks { .. } => {}
        }

        let next_heartbeat_due_at = input
            .state
            .last_heartbeat_sent_at
            .map(|timestamp| {
                timestamp_saturating_add(timestamp, input.cadence.heartbeat_interval_micros)
            })
            .unwrap_or(input.now);

        if input.now.0 < next_heartbeat_due_at.0 {
            return ClientHeartbeatLoopPolicyAction::Wait {
                next_heartbeat_due_at,
                log: client_heartbeat_loop_log(
                    &input,
                    ClientHeartbeatLoopPolicyReason::WaitingForCadence,
                ),
            };
        }

        ClientHeartbeatLoopPolicyAction::SendHeartbeat {
            send_at: input.now,
            ack_deadline_at: timestamp_saturating_add(
                input.now,
                input.cadence.ack_receive_timeout_micros,
            ),
            ack_observation_return: input.cadence.ack_observation_return,
            log: client_heartbeat_loop_log(&input, ClientHeartbeatLoopPolicyReason::HeartbeatDue),
        }
    }
}

/// Why a future client heartbeat loop cannot take ownership yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopOwnershipNotReadyReason {
    AuthNotAccepted,
    SocketNotBound,
}

/// Ownership inputs needed before entering a future client heartbeat loop body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopOwnershipInput {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub auth_accepted: bool,
    pub socket_bound: bool,
}

/// State ownership plan for a future client heartbeat loop body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopOwnershipPlan {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub owns_udp_socket: bool,
    pub owns_loop_state: bool,
    pub owns_ack_wait: bool,
    pub owns_stats_return: bool,
}

/// Readiness decision before a future client heartbeat loop owns state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopOwnershipDecision {
    Ready(ClientHeartbeatLoopOwnershipPlan),
    NotReady {
        client_id: ClientId,
        run_id: RunId,
        reason: ClientHeartbeatLoopOwnershipNotReadyReason,
    },
}

/// Boundary that names client-side ownership before entering a future loop.
///
/// This boundary only validates that accepted auth and a bound socket exist,
/// then names the state the future loop body owns. It does not start the loop,
/// move a real `UdpSocket`, send heartbeats, receive acks, or retry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopOwnershipBoundary;

impl ClientHeartbeatLoopOwnershipBoundary {
    pub fn evaluate(
        &self,
        input: ClientHeartbeatLoopOwnershipInput,
    ) -> ClientHeartbeatLoopOwnershipDecision {
        if !input.auth_accepted {
            return ClientHeartbeatLoopOwnershipDecision::NotReady {
                client_id: input.client_id,
                run_id: input.run_id,
                reason: ClientHeartbeatLoopOwnershipNotReadyReason::AuthNotAccepted,
            };
        }
        if !input.socket_bound {
            return ClientHeartbeatLoopOwnershipDecision::NotReady {
                client_id: input.client_id,
                run_id: input.run_id,
                reason: ClientHeartbeatLoopOwnershipNotReadyReason::SocketNotBound,
            };
        }

        ClientHeartbeatLoopOwnershipDecision::Ready(ClientHeartbeatLoopOwnershipPlan {
            client_id: input.client_id,
            run_id: input.run_id,
            protocol_version: input.protocol_version,
            owns_udp_socket: true,
            owns_loop_state: true,
            owns_ack_wait: true,
            owns_stats_return: true,
        })
    }
}

/// Input for deriving one ack receive socket wait timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatAckReceiveTimeoutInput {
    pub now: TimestampMicros,
    pub ack_deadline_at: TimestampMicros,
    pub max_socket_wait_micros: u64,
}

/// Socket wait decision for one future ack receive attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatAckReceiveTimeoutDecision {
    DeadlineElapsed,
    Wait {
        receive_timeout_micros: u64,
        deadline_at: TimestampMicros,
    },
}

/// Boundary that derives the blocking receive timeout for one ack wait.
///
/// This only calculates the timeout value a future socket wait would use. It
/// does not call `set_read_timeout`, receive UDP packets, or classify errors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatAckReceiveTimeoutBoundary;

impl ClientHeartbeatAckReceiveTimeoutBoundary {
    pub fn plan_wait(
        &self,
        input: ClientHeartbeatAckReceiveTimeoutInput,
    ) -> ClientHeartbeatAckReceiveTimeoutDecision {
        if input.now.0 >= input.ack_deadline_at.0 {
            return ClientHeartbeatAckReceiveTimeoutDecision::DeadlineElapsed;
        }

        let remaining_micros = input.ack_deadline_at.0 - input.now.0;
        ClientHeartbeatAckReceiveTimeoutDecision::Wait {
            receive_timeout_micros: remaining_micros.min(input.max_socket_wait_micros),
            deadline_at: input.ack_deadline_at,
        }
    }
}

/// Retry reason placeholder for future client heartbeat loop failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopRetryReason {
    HeartbeatSendFailed,
    AckReceiveTimeout,
    AckDecodeFailed,
    StatsReturnSendFailed,
}

/// Retry policy placeholder for future client heartbeat loop work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopRetryPolicy {
    pub max_attempts: u32,
    pub retry_delay_micros: u64,
}

/// Input for one retry decision in a future client heartbeat loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopRetryInput {
    pub reason: ClientHeartbeatLoopRetryReason,
    pub attempts_used: u32,
    pub policy: ClientHeartbeatLoopRetryPolicy,
    pub now: TimestampMicros,
}

/// Retry decision placeholder for future client heartbeat loop work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopRetryDecision {
    RetryLater {
        reason: ClientHeartbeatLoopRetryReason,
        next_attempt: u32,
        retry_at: TimestampMicros,
    },
    GiveUp {
        reason: ClientHeartbeatLoopRetryReason,
        attempts_used: u32,
    },
}

/// Boundary that classifies retry timing without executing retry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopRetryBoundary;

impl ClientHeartbeatLoopRetryBoundary {
    pub fn decide(&self, input: ClientHeartbeatLoopRetryInput) -> ClientHeartbeatLoopRetryDecision {
        if input.attempts_used >= input.policy.max_attempts {
            return ClientHeartbeatLoopRetryDecision::GiveUp {
                reason: input.reason,
                attempts_used: input.attempts_used,
            };
        }

        ClientHeartbeatLoopRetryDecision::RetryLater {
            reason: input.reason,
            next_attempt: input.attempts_used.saturating_add(1),
            retry_at: timestamp_saturating_add(input.now, input.policy.retry_delay_micros),
        }
    }
}

/// Input for one future client heartbeat loop body iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopBodyInput {
    pub ownership: ClientHeartbeatLoopOwnershipInput,
    pub policy: ClientHeartbeatLoopPolicyInput,
    pub max_ack_socket_wait_micros: u64,
}

/// Handoff telling a future body to send exactly one heartbeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopBodySendHandoff {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub send_at: TimestampMicros,
    pub ack_deadline_at: TimestampMicros,
    pub ack_wait: ClientHeartbeatAckReceiveTimeoutDecision,
    pub ack_observation_return: ClientHeartbeatAckObservationReturnMode,
}

/// Runtime-shaped result for one future client heartbeat loop body iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopBodyResult {
    OwnershipNotReady(ClientHeartbeatLoopOwnershipDecision),
    Stop {
        reason: ClientHeartbeatLoopPolicyReason,
        log: ClientHeartbeatLoopLogHandoff,
    },
    Wait {
        next_heartbeat_due_at: TimestampMicros,
        log: ClientHeartbeatLoopLogHandoff,
    },
    SendHeartbeat {
        handoff: ClientHeartbeatLoopBodySendHandoff,
        log: ClientHeartbeatLoopLogHandoff,
    },
}

/// Boundary for one future client heartbeat loop body iteration.
///
/// This composes auth/socket ownership, cadence policy, and ack wait timeout
/// planning for one iteration. It does not send UDP packets, receive acks,
/// build observations, send stats, sleep, or retry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopBodyBoundary {
    ownership: ClientHeartbeatLoopOwnershipBoundary,
    policy: ClientHeartbeatLoopPolicyBoundary,
    ack_wait: ClientHeartbeatAckReceiveTimeoutBoundary,
}

impl ClientHeartbeatLoopBodyBoundary {
    pub fn run_one(&self, input: ClientHeartbeatLoopBodyInput) -> ClientHeartbeatLoopBodyResult {
        let ownership = self.ownership.evaluate(input.ownership);
        let ClientHeartbeatLoopOwnershipDecision::Ready(ownership_plan) = ownership else {
            return ClientHeartbeatLoopBodyResult::OwnershipNotReady(ownership);
        };

        match self.policy.evaluate(input.policy) {
            ClientHeartbeatLoopPolicyAction::Stop { reason, log } => {
                ClientHeartbeatLoopBodyResult::Stop { reason, log }
            }
            ClientHeartbeatLoopPolicyAction::Wait {
                next_heartbeat_due_at,
                log,
            } => ClientHeartbeatLoopBodyResult::Wait {
                next_heartbeat_due_at,
                log,
            },
            ClientHeartbeatLoopPolicyAction::SendHeartbeat {
                send_at,
                ack_deadline_at,
                ack_observation_return,
                log,
            } => {
                let ack_wait = self
                    .ack_wait
                    .plan_wait(ClientHeartbeatAckReceiveTimeoutInput {
                        now: send_at,
                        ack_deadline_at,
                        max_socket_wait_micros: input.max_ack_socket_wait_micros,
                    });

                ClientHeartbeatLoopBodyResult::SendHeartbeat {
                    handoff: ClientHeartbeatLoopBodySendHandoff {
                        client_id: ownership_plan.client_id,
                        run_id: ownership_plan.run_id,
                        protocol_version: ownership_plan.protocol_version,
                        send_at,
                        ack_deadline_at,
                        ack_wait,
                        ack_observation_return,
                    },
                    log,
                }
            }
        }
    }
}

/// Input for building, encoding, and sending one heartbeat from a loop handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopEncodeSendInput {
    pub destination: SocketAddr,
    pub handoff: ClientHeartbeatLoopBodySendHandoff,
    pub local_time: Option<TimestampMicros>,
    pub short_status: Option<String>,
}

/// Encoded heartbeat handoff produced before the UDP send attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopEncodedSendHandoff {
    pub destination: SocketAddr,
    pub heartbeat: Heartbeat,
    pub encoded_bytes: Vec<u8>,
    pub ack_deadline_at: TimestampMicros,
    pub ack_wait: ClientHeartbeatAckReceiveTimeoutDecision,
    pub ack_observation_return: ClientHeartbeatAckObservationReturnMode,
}

/// Runtime-shaped result after one heartbeat send attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopEncodeSendRuntimeResult {
    pub handoff: ClientHeartbeatLoopEncodedSendHandoff,
    pub bytes_sent: usize,
}

/// Error from the client heartbeat encode/send handoff boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopEncodeSendError {
    Encode(ProtocolError),
    Send(io::ErrorKind),
}

/// Boundary that connects a body send handoff to heartbeat build/encode/send.
///
/// This builds one `Heartbeat`, encodes it through the protocol encoder, and
/// performs one UDP `send_to` using the caller-owned socket. It does not wait
/// for `HeartbeatAck`, create observations, send `ClientStats`, retry, sleep,
/// or run a continuous heartbeat loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopEncodeSendBoundary {
    encoder: ProtocolMessageEncoderBoundary,
}

impl ClientHeartbeatLoopEncodeSendBoundary {
    pub fn encode_handoff(
        &self,
        input: ClientHeartbeatLoopEncodeSendInput,
    ) -> Result<ClientHeartbeatLoopEncodedSendHandoff, ClientHeartbeatLoopEncodeSendError> {
        let heartbeat = Heartbeat {
            message_type: MessageType::Heartbeat,
            protocol_version: input.handoff.protocol_version,
            client_id: input.handoff.client_id,
            run_id: input.handoff.run_id,
            sent_at: input.handoff.send_at,
            local_time: input.local_time,
            short_status: input.short_status,
        };
        let mut encoded_bytes = Vec::new();
        self.encoder
            .encode_message(
                EncodeContext {
                    protocol_version: heartbeat.protocol_version,
                },
                &ProtocolMessage::Heartbeat(heartbeat.clone()),
                &mut encoded_bytes,
            )
            .map_err(ClientHeartbeatLoopEncodeSendError::Encode)?;

        Ok(ClientHeartbeatLoopEncodedSendHandoff {
            destination: input.destination,
            heartbeat,
            encoded_bytes,
            ack_deadline_at: input.handoff.ack_deadline_at,
            ack_wait: input.handoff.ack_wait,
            ack_observation_return: input.handoff.ack_observation_return,
        })
    }

    pub fn send_one(
        &self,
        socket: &UdpSocket,
        input: ClientHeartbeatLoopEncodeSendInput,
    ) -> Result<ClientHeartbeatLoopEncodeSendRuntimeResult, ClientHeartbeatLoopEncodeSendError>
    {
        let handoff = self.encode_handoff(input)?;
        let bytes_sent = socket
            .send_to(&handoff.encoded_bytes, handoff.destination)
            .map_err(|error| ClientHeartbeatLoopEncodeSendError::Send(error.kind()))?;

        Ok(ClientHeartbeatLoopEncodeSendRuntimeResult {
            handoff,
            bytes_sent,
        })
    }
}

/// Input for preparing ack observation return from a decoded HeartbeatAck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopAckObservationReturnInput {
    pub sent: ClientHeartbeatLoopEncodedSendHandoff,
    pub ack_source: SocketAddr,
    pub ack_bytes: Vec<u8>,
    pub ack: HeartbeatAck,
    pub client_received_at: TimestampMicros,
    pub client_stats_sent_at: TimestampMicros,
}

/// Handoff for a future `ClientStats` return datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopClientStatsReturnHandoff {
    pub destination: SocketAddr,
    pub client_stats: ClientStats,
    pub encoded_bytes: Vec<u8>,
}

/// Runtime-shaped result after receiving an ack and preparing optional return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopAckObservationReturnRuntimeResult {
    pub ack_source: SocketAddr,
    pub ack_bytes: Vec<u8>,
    pub ack: HeartbeatAck,
    pub observation: HeartbeatAckObservation,
    pub client_stats_return: Option<ClientHeartbeatLoopClientStatsReturnHandoff>,
}

/// Error from the client ack receive / observation return boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopAckObservationReturnError {
    AckReceive(ClientResponseReceiveError),
    AckCorrelationMismatch {
        expected_client_id: ClientId,
        actual_client_id: ClientId,
        expected_run_id: RunId,
        actual_run_id: RunId,
        expected_echoed_sent_at: TimestampMicros,
        actual_echoed_sent_at: TimestampMicros,
    },
    Encode(ProtocolError),
}

/// Boundary that connects ack receive/decode to observation return handoff.
///
/// This can receive and decode one `HeartbeatAck`, build one
/// `HeartbeatAckObservation`, and optionally encode one `ClientStats` return
/// datagram. It does not send the `ClientStats` datagram, retry, sleep, or run
/// a continuous heartbeat loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopAckObservationReturnBoundary {
    observation: ClientHeartbeatAckObservationBoundary,
    carrier: ClientHeartbeatObservationCarrierBoundary,
    encoder: ProtocolMessageEncoderBoundary,
}

impl ClientHeartbeatLoopAckObservationReturnBoundary {
    pub fn receive_one(
        &self,
        socket: &UdpSocket,
        sent: ClientHeartbeatLoopEncodedSendHandoff,
    ) -> Result<
        ClientHeartbeatLoopAckObservationReturnRuntimeResult,
        ClientHeartbeatLoopAckObservationReturnError,
    > {
        let (ack_source, ack_bytes, ack) =
            receive_heartbeat_ack(socket, sent.heartbeat.protocol_version)
                .map_err(ClientHeartbeatLoopAckObservationReturnError::AckReceive)?;
        let client_received_at = current_timestamp_micros();
        self.prepare_return(ClientHeartbeatLoopAckObservationReturnInput {
            sent,
            ack_source,
            ack_bytes,
            ack,
            client_received_at,
            client_stats_sent_at: current_timestamp_micros(),
        })
    }

    pub fn prepare_return(
        &self,
        input: ClientHeartbeatLoopAckObservationReturnInput,
    ) -> Result<
        ClientHeartbeatLoopAckObservationReturnRuntimeResult,
        ClientHeartbeatLoopAckObservationReturnError,
    > {
        self.validate_ack_correlation(&input.sent.heartbeat, &input.ack)?;

        let observation = self
            .observation
            .observe_ack(&input.ack, input.client_received_at);
        let client_stats_return = match input.sent.ack_observation_return {
            ClientHeartbeatAckObservationReturnMode::Disabled => None,
            ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck => {
                Some(self.build_client_stats_return(
                    input.sent.destination,
                    input.sent.heartbeat.protocol_version,
                    observation.clone(),
                    input.client_stats_sent_at,
                )?)
            }
        };

        Ok(ClientHeartbeatLoopAckObservationReturnRuntimeResult {
            ack_source: input.ack_source,
            ack_bytes: input.ack_bytes,
            ack: input.ack,
            observation,
            client_stats_return,
        })
    }

    fn validate_ack_correlation(
        &self,
        heartbeat: &Heartbeat,
        ack: &HeartbeatAck,
    ) -> Result<(), ClientHeartbeatLoopAckObservationReturnError> {
        if heartbeat.client_id != ack.client_id
            || heartbeat.run_id != ack.run_id
            || heartbeat.sent_at != ack.echoed_sent_at
        {
            return Err(
                ClientHeartbeatLoopAckObservationReturnError::AckCorrelationMismatch {
                    expected_client_id: heartbeat.client_id.clone(),
                    actual_client_id: ack.client_id.clone(),
                    expected_run_id: heartbeat.run_id.clone(),
                    actual_run_id: ack.run_id.clone(),
                    expected_echoed_sent_at: heartbeat.sent_at,
                    actual_echoed_sent_at: ack.echoed_sent_at,
                },
            );
        }

        Ok(())
    }

    fn build_client_stats_return(
        &self,
        destination: SocketAddr,
        protocol_version: ProtocolVersion,
        observation: HeartbeatAckObservation,
        sent_at: TimestampMicros,
    ) -> Result<
        ClientHeartbeatLoopClientStatsReturnHandoff,
        ClientHeartbeatLoopAckObservationReturnError,
    > {
        let carrier = self
            .carrier
            .build_client_stats_carrier(protocol_version, observation.clone());
        let client_stats = ClientStats {
            message_type: MessageType::ClientStats,
            protocol_version: carrier.protocol_version,
            client_id: carrier.observation.client_id.clone(),
            run_id: carrier.observation.run_id.clone(),
            sent_at,
            capture_fps: 0,
            dropped_frames: 0,
            bitrate_kbps: 0,
            heartbeat_observation: Some(carrier.observation),
        };
        let mut encoded_bytes = Vec::new();
        self.encoder
            .encode_message(
                EncodeContext { protocol_version },
                &ProtocolMessage::ClientStats(client_stats.clone()),
                &mut encoded_bytes,
            )
            .map_err(ClientHeartbeatLoopAckObservationReturnError::Encode)?;

        Ok(ClientHeartbeatLoopClientStatsReturnHandoff {
            destination,
            client_stats,
            encoded_bytes,
        })
    }
}

/// Runtime-shaped result after sending one ClientStats return datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopClientStatsReturnSendRuntimeResult {
    pub handoff: ClientHeartbeatLoopClientStatsReturnHandoff,
    pub bytes_sent: usize,
}

/// Error from sending one ClientStats return datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopClientStatsReturnSendError {
    Send(io::ErrorKind),
}

/// Boundary that sends one already-encoded ClientStats return datagram.
///
/// This boundary consumes the handoff prepared by
/// `ClientHeartbeatLoopAckObservationReturnBoundary` and performs one UDP
/// `send_to` using the caller-owned socket. It does not encode `ClientStats`,
/// wait for another ack, retry, sleep, or run a continuous heartbeat loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopClientStatsReturnSendBoundary;

impl ClientHeartbeatLoopClientStatsReturnSendBoundary {
    pub fn send_one(
        &self,
        socket: &UdpSocket,
        handoff: ClientHeartbeatLoopClientStatsReturnHandoff,
    ) -> Result<
        ClientHeartbeatLoopClientStatsReturnSendRuntimeResult,
        ClientHeartbeatLoopClientStatsReturnSendError,
    > {
        let bytes_sent = socket
            .send_to(&handoff.encoded_bytes, handoff.destination)
            .map_err(|error| ClientHeartbeatLoopClientStatsReturnSendError::Send(error.kind()))?;

        Ok(ClientHeartbeatLoopClientStatsReturnSendRuntimeResult {
            handoff,
            bytes_sent,
        })
    }
}

/// Mutable counters owned by a future continuous client heartbeat loop.
///
/// This state is intentionally small and local to the client loop. It records
/// the result of already-executed loop steps; it does not send packets, wait on
/// sockets, retry, sleep, or decide the next loop action.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCountersState {
    pub sent_heartbeats: u64,
    pub received_acks: u64,
    pub missed_acks: u64,
    pub stats_returns_sent: u64,
    pub heartbeat_send_failures: u64,
    pub ack_receive_failures: u64,
    pub stats_return_send_failures: u64,
    pub last_heartbeat_sent_at: Option<TimestampMicros>,
    pub last_ack_received_at: Option<TimestampMicros>,
    pub last_stats_return_sent_at: Option<TimestampMicros>,
}

impl ClientHeartbeatLoopCountersState {
    pub fn as_policy_snapshot(&self, stop_requested: bool) -> ClientHeartbeatLoopStateSnapshot {
        ClientHeartbeatLoopStateSnapshot {
            sent_heartbeats: self.sent_heartbeats,
            received_acks: self.received_acks,
            missed_acks: self.missed_acks,
            last_heartbeat_sent_at: self.last_heartbeat_sent_at,
            stop_requested,
        }
    }
}

/// Failure class used when a future loop body reports a failed iteration step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopIterationFailureKind {
    HeartbeatSend,
    AckReceive,
    ClientStatsReturnSend,
}

/// Runtime-shaped result emitted by one client heartbeat loop iteration step.
///
/// The future continuous loop body will choose which variant to emit after it
/// runs heartbeat send, ack receive, observation return, and optional
/// `ClientStats` send steps. This type does not perform those steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopIterationRuntimeResult {
    Waited {
        next_heartbeat_due_at: TimestampMicros,
    },
    Stopped {
        reason: ClientHeartbeatLoopPolicyReason,
    },
    HeartbeatSent {
        sent_at: TimestampMicros,
    },
    AckReceived {
        client_received_at: TimestampMicros,
        stats_return_prepared: bool,
    },
    AckMissed {
        missed_at: TimestampMicros,
    },
    ClientStatsReturnSent {
        sent_at: TimestampMicros,
    },
    Failed {
        kind: ClientHeartbeatLoopIterationFailureKind,
        failed_at: TimestampMicros,
    },
}

impl ClientHeartbeatLoopIterationRuntimeResult {
    pub fn from_heartbeat_send(result: &ClientHeartbeatLoopEncodeSendRuntimeResult) -> Self {
        Self::HeartbeatSent {
            sent_at: result.handoff.heartbeat.sent_at,
        }
    }

    pub fn from_ack_return(result: &ClientHeartbeatLoopAckObservationReturnRuntimeResult) -> Self {
        Self::AckReceived {
            client_received_at: result.observation.client_received_at,
            stats_return_prepared: result.client_stats_return.is_some(),
        }
    }

    pub fn from_stats_return_send(
        result: &ClientHeartbeatLoopClientStatsReturnSendRuntimeResult,
    ) -> Self {
        Self::ClientStatsReturnSent {
            sent_at: result.handoff.client_stats.sent_at,
        }
    }
}

/// Result of applying one iteration result to client loop counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCountersUpdateOutcome {
    pub previous: ClientHeartbeatLoopCountersState,
    pub current: ClientHeartbeatLoopCountersState,
}

/// Boundary that commits one client loop iteration result into counters.
///
/// It is a pure state-update boundary for future loop orchestration. It does
/// not execute heartbeat send, receive `HeartbeatAck`, build observations,
/// send `ClientStats`, decide retries, write logs, or run a continuous loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCountersBoundary;

impl ClientHeartbeatLoopCountersBoundary {
    pub fn commit_result(
        &self,
        state: &mut ClientHeartbeatLoopCountersState,
        result: ClientHeartbeatLoopIterationRuntimeResult,
    ) -> ClientHeartbeatLoopCountersUpdateOutcome {
        let previous = state.clone();

        match result {
            ClientHeartbeatLoopIterationRuntimeResult::Waited { .. }
            | ClientHeartbeatLoopIterationRuntimeResult::Stopped { .. } => {}
            ClientHeartbeatLoopIterationRuntimeResult::HeartbeatSent { sent_at } => {
                state.sent_heartbeats = state.sent_heartbeats.saturating_add(1);
                state.last_heartbeat_sent_at = Some(sent_at);
            }
            ClientHeartbeatLoopIterationRuntimeResult::AckReceived {
                client_received_at, ..
            } => {
                state.received_acks = state.received_acks.saturating_add(1);
                state.last_ack_received_at = Some(client_received_at);
            }
            ClientHeartbeatLoopIterationRuntimeResult::AckMissed { .. } => {
                state.missed_acks = state.missed_acks.saturating_add(1);
            }
            ClientHeartbeatLoopIterationRuntimeResult::ClientStatsReturnSent { sent_at } => {
                state.stats_returns_sent = state.stats_returns_sent.saturating_add(1);
                state.last_stats_return_sent_at = Some(sent_at);
            }
            ClientHeartbeatLoopIterationRuntimeResult::Failed { kind, .. } => match kind {
                ClientHeartbeatLoopIterationFailureKind::HeartbeatSend => {
                    state.heartbeat_send_failures = state.heartbeat_send_failures.saturating_add(1);
                }
                ClientHeartbeatLoopIterationFailureKind::AckReceive => {
                    state.ack_receive_failures = state.ack_receive_failures.saturating_add(1);
                }
                ClientHeartbeatLoopIterationFailureKind::ClientStatsReturnSend => {
                    state.stats_return_send_failures =
                        state.stats_return_send_failures.saturating_add(1);
                }
            },
        }

        ClientHeartbeatLoopCountersUpdateOutcome {
            previous,
            current: state.clone(),
        }
    }
}

/// Why a future client heartbeat controller would sleep or decline sleeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopSleepReason {
    CadenceWait,
    AckWait,
    RetryBackoff,
    RetryExhausted,
}

/// Input for converting a planned wake timestamp into one bounded sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopSleepInput {
    pub now: TimestampMicros,
    pub wake_at: TimestampMicros,
    pub max_sleep_micros: u64,
    pub reason: ClientHeartbeatLoopSleepReason,
}

/// Sleep decision for one future controller step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopSleepDecision {
    NoSleep {
        reason: ClientHeartbeatLoopSleepReason,
    },
    Sleep {
        reason: ClientHeartbeatLoopSleepReason,
        sleep_micros: u64,
        wake_at: TimestampMicros,
    },
}

/// Boundary that plans one bounded sleep without blocking the current thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopSleepBoundary;

impl ClientHeartbeatLoopSleepBoundary {
    pub fn plan_sleep(
        &self,
        input: ClientHeartbeatLoopSleepInput,
    ) -> ClientHeartbeatLoopSleepDecision {
        if input.now.0 >= input.wake_at.0 || input.max_sleep_micros == 0 {
            return ClientHeartbeatLoopSleepDecision::NoSleep {
                reason: input.reason,
            };
        }

        ClientHeartbeatLoopSleepDecision::Sleep {
            reason: input.reason,
            sleep_micros: (input.wake_at.0 - input.now.0).min(input.max_sleep_micros),
            wake_at: input.wake_at,
        }
    }
}

/// Input for applying one retry decision to failure and sleep handoffs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopRetryApplyInput {
    pub reason: ClientHeartbeatLoopRetryReason,
    pub failure_kind: ClientHeartbeatLoopIterationFailureKind,
    pub attempts_used: u32,
    pub policy: ClientHeartbeatLoopRetryPolicy,
    pub failed_at: TimestampMicros,
    pub max_sleep_micros: u64,
}

/// Result of connecting one failed step to retry and sleep planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopRetryApplyResult {
    pub failure_result: ClientHeartbeatLoopIterationRuntimeResult,
    pub retry_decision: ClientHeartbeatLoopRetryDecision,
    pub sleep: ClientHeartbeatLoopSleepDecision,
}

/// Boundary that connects a classified failure to retry and sleep handoffs.
///
/// This does not re-run the failed operation, block the thread, or mutate
/// counters. The caller may commit `failure_result` through
/// `ClientHeartbeatLoopCountersBoundary`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopRetryApplyBoundary {
    retry: ClientHeartbeatLoopRetryBoundary,
    sleep: ClientHeartbeatLoopSleepBoundary,
}

impl ClientHeartbeatLoopRetryApplyBoundary {
    pub fn apply_failure(
        &self,
        input: ClientHeartbeatLoopRetryApplyInput,
    ) -> ClientHeartbeatLoopRetryApplyResult {
        let failure_result = ClientHeartbeatLoopIterationRuntimeResult::Failed {
            kind: input.failure_kind,
            failed_at: input.failed_at,
        };
        let retry_decision = self.retry.decide(ClientHeartbeatLoopRetryInput {
            reason: input.reason,
            attempts_used: input.attempts_used,
            policy: input.policy,
            now: input.failed_at,
        });
        let sleep = match retry_decision {
            ClientHeartbeatLoopRetryDecision::RetryLater { retry_at, .. } => {
                self.sleep.plan_sleep(ClientHeartbeatLoopSleepInput {
                    now: input.failed_at,
                    wake_at: retry_at,
                    max_sleep_micros: input.max_sleep_micros,
                    reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                })
            }
            ClientHeartbeatLoopRetryDecision::GiveUp { .. } => {
                ClientHeartbeatLoopSleepDecision::NoSleep {
                    reason: ClientHeartbeatLoopSleepReason::RetryExhausted,
                }
            }
        };

        ClientHeartbeatLoopRetryApplyResult {
            failure_result,
            retry_decision,
            sleep,
        }
    }
}

/// Input for a minimal controller step over one body result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopControllerInput {
    pub body_result: ClientHeartbeatLoopBodyResult,
    pub now: TimestampMicros,
    pub max_sleep_micros: u64,
}

/// Controller plan after one policy/body result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopControllerPlan {
    OwnershipNotReady {
        decision: ClientHeartbeatLoopOwnershipDecision,
    },
    Stop {
        reason: ClientHeartbeatLoopPolicyReason,
        log: ClientHeartbeatLoopLogHandoff,
        iteration_result: ClientHeartbeatLoopIterationRuntimeResult,
    },
    Sleep {
        sleep: ClientHeartbeatLoopSleepDecision,
        log: ClientHeartbeatLoopLogHandoff,
        iteration_result: ClientHeartbeatLoopIterationRuntimeResult,
    },
    SendHeartbeat {
        handoff: ClientHeartbeatLoopBodySendHandoff,
        log: ClientHeartbeatLoopLogHandoff,
    },
}

/// Coarse action class selected by the client loop controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopControllerAction {
    OwnershipNotReady,
    Stop,
    Sleep,
    SendHeartbeat,
}

/// Shutdown decision extracted from one controller plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopShutdownDecision {
    Continue,
    Stop {
        reason: ClientHeartbeatLoopPolicyReason,
    },
}

/// Typed log handoff prepared after controller planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopControllerLogHandoff {
    pub action: ClientHeartbeatLoopControllerAction,
    pub policy_log: ClientHeartbeatLoopLogHandoff,
    pub iteration_result: Option<ClientHeartbeatLoopIterationRuntimeResult>,
}

/// Final typed result for one pre-loop controller step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopControllerResult {
    pub action: ClientHeartbeatLoopControllerAction,
    pub plan: ClientHeartbeatLoopControllerPlan,
    pub log: Option<ClientHeartbeatLoopControllerLogHandoff>,
    pub shutdown: ClientHeartbeatLoopShutdownDecision,
    pub iteration_result: Option<ClientHeartbeatLoopIterationRuntimeResult>,
}

/// Input for one minimal client heartbeat loop runtime tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopOneTickRuntimeInput {
    pub destination: SocketAddr,
    pub body: ClientHeartbeatLoopBodyInput,
    pub local_time: Option<TimestampMicros>,
    pub short_status: Option<String>,
    pub controller_now: TimestampMicros,
    pub max_sleep_micros: u64,
    pub retry_policy: ClientHeartbeatLoopRetryPolicy,
    pub retry_attempts_used: u32,
}

/// Failure observed by the one-tick runtime boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopOneTickRuntimeFailure {
    HeartbeatSend(ClientHeartbeatLoopEncodeSendError),
    AckReceive(ClientHeartbeatLoopAckObservationReturnError),
    ClientStatsReturnSend(ClientHeartbeatLoopClientStatsReturnSendError),
}

/// Result of one minimal client heartbeat loop runtime tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopOneTickRuntimeResult {
    pub controller: ClientHeartbeatLoopControllerResult,
    pub heartbeat_send: Option<ClientHeartbeatLoopEncodeSendRuntimeResult>,
    pub ack_return: Option<ClientHeartbeatLoopAckObservationReturnRuntimeResult>,
    pub stats_return_send: Option<ClientHeartbeatLoopClientStatsReturnSendRuntimeResult>,
    pub retry: Option<ClientHeartbeatLoopRetryApplyResult>,
    pub failure: Option<ClientHeartbeatLoopOneTickRuntimeFailure>,
    pub counters_updates: Vec<ClientHeartbeatLoopCountersUpdateOutcome>,
    pub final_counters: ClientHeartbeatLoopCountersState,
}

/// Boundary that prepares controller log handoff without writing logs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopControllerLogHandoffBoundary;

impl ClientHeartbeatLoopControllerLogHandoffBoundary {
    pub fn prepare(
        &self,
        plan: &ClientHeartbeatLoopControllerPlan,
    ) -> Option<ClientHeartbeatLoopControllerLogHandoff> {
        match plan {
            ClientHeartbeatLoopControllerPlan::OwnershipNotReady { .. } => None,
            ClientHeartbeatLoopControllerPlan::Stop {
                log,
                iteration_result,
                ..
            } => Some(ClientHeartbeatLoopControllerLogHandoff {
                action: ClientHeartbeatLoopControllerAction::Stop,
                policy_log: log.clone(),
                iteration_result: Some(iteration_result.clone()),
            }),
            ClientHeartbeatLoopControllerPlan::Sleep {
                log,
                iteration_result,
                ..
            } => Some(ClientHeartbeatLoopControllerLogHandoff {
                action: ClientHeartbeatLoopControllerAction::Sleep,
                policy_log: log.clone(),
                iteration_result: Some(iteration_result.clone()),
            }),
            ClientHeartbeatLoopControllerPlan::SendHeartbeat { log, .. } => {
                Some(ClientHeartbeatLoopControllerLogHandoff {
                    action: ClientHeartbeatLoopControllerAction::SendHeartbeat,
                    policy_log: log.clone(),
                    iteration_result: None,
                })
            }
        }
    }
}

/// Boundary that maps a controller plan to a one-step shutdown decision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopShutdownDecisionBoundary;

impl ClientHeartbeatLoopShutdownDecisionBoundary {
    pub fn decide(
        &self,
        plan: &ClientHeartbeatLoopControllerPlan,
    ) -> ClientHeartbeatLoopShutdownDecision {
        match plan {
            ClientHeartbeatLoopControllerPlan::Stop { reason, .. } => {
                ClientHeartbeatLoopShutdownDecision::Stop { reason: *reason }
            }
            ClientHeartbeatLoopControllerPlan::OwnershipNotReady { .. }
            | ClientHeartbeatLoopControllerPlan::Sleep { .. }
            | ClientHeartbeatLoopControllerPlan::SendHeartbeat { .. } => {
                ClientHeartbeatLoopShutdownDecision::Continue
            }
        }
    }
}

/// Boundary that combines controller plan, log handoff, and shutdown decision.
///
/// This is the last pre-loop connection point. It does not execute shutdown,
/// write JSON Lines, mutate counters, sleep, retry, or call sockets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopControllerResultBoundary {
    log: ClientHeartbeatLoopControllerLogHandoffBoundary,
    shutdown: ClientHeartbeatLoopShutdownDecisionBoundary,
}

impl ClientHeartbeatLoopControllerResultBoundary {
    pub fn finalize(
        &self,
        plan: ClientHeartbeatLoopControllerPlan,
    ) -> ClientHeartbeatLoopControllerResult {
        let action = client_heartbeat_loop_controller_action(&plan);
        let log = self.log.prepare(&plan);
        let shutdown = self.shutdown.decide(&plan);
        let iteration_result = client_heartbeat_loop_controller_iteration_result(&plan);

        ClientHeartbeatLoopControllerResult {
            action,
            plan,
            log,
            shutdown,
            iteration_result,
        }
    }
}

/// Minimal one-tick runtime boundary for the future client heartbeat loop.
///
/// This connects the already-separated body, controller, encode/send, ack
/// receive, optional stats return, counters, retry planning, log handoff, and
/// shutdown decision exactly once. It does not repeat, sleep, open log sinks,
/// execute shutdown cleanup, or introduce an async runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopOneTickRuntimeBoundary {
    body: ClientHeartbeatLoopBodyBoundary,
    controller: ClientHeartbeatLoopControllerBoundary,
    controller_result: ClientHeartbeatLoopControllerResultBoundary,
    encode_send: ClientHeartbeatLoopEncodeSendBoundary,
    ack_return: ClientHeartbeatLoopAckObservationReturnBoundary,
    stats_return_send: ClientHeartbeatLoopClientStatsReturnSendBoundary,
    counters: ClientHeartbeatLoopCountersBoundary,
    retry: ClientHeartbeatLoopRetryApplyBoundary,
}

impl ClientHeartbeatLoopOneTickRuntimeBoundary {
    pub fn run_one(
        &self,
        socket: &UdpSocket,
        counters: &mut ClientHeartbeatLoopCountersState,
        input: ClientHeartbeatLoopOneTickRuntimeInput,
    ) -> ClientHeartbeatLoopOneTickRuntimeResult {
        let body_result = self.body.run_one(input.body.clone());
        let controller_plan = self
            .controller
            .plan_next(ClientHeartbeatLoopControllerInput {
                body_result,
                now: input.controller_now,
                max_sleep_micros: input.max_sleep_micros,
            });
        let controller = self.controller_result.finalize(controller_plan.clone());
        let mut result = ClientHeartbeatLoopOneTickRuntimeResult {
            controller,
            heartbeat_send: None,
            ack_return: None,
            stats_return_send: None,
            retry: None,
            failure: None,
            counters_updates: Vec::new(),
            final_counters: counters.clone(),
        };

        match controller_plan {
            ClientHeartbeatLoopControllerPlan::OwnershipNotReady { .. } => {
                result.final_counters = counters.clone();
                result
            }
            ClientHeartbeatLoopControllerPlan::Stop {
                iteration_result, ..
            }
            | ClientHeartbeatLoopControllerPlan::Sleep {
                iteration_result, ..
            } => {
                self.commit_iteration(counters, &mut result, iteration_result);
                result
            }
            ClientHeartbeatLoopControllerPlan::SendHeartbeat { handoff, .. } => {
                self.run_send_ack_stats(socket, counters, &mut result, handoff, input);
                result
            }
        }
    }

    fn run_send_ack_stats(
        &self,
        socket: &UdpSocket,
        counters: &mut ClientHeartbeatLoopCountersState,
        result: &mut ClientHeartbeatLoopOneTickRuntimeResult,
        handoff: ClientHeartbeatLoopBodySendHandoff,
        input: ClientHeartbeatLoopOneTickRuntimeInput,
    ) {
        let send = match self.encode_send.send_one(
            socket,
            ClientHeartbeatLoopEncodeSendInput {
                destination: input.destination,
                handoff,
                local_time: input.local_time,
                short_status: input.short_status,
            },
        ) {
            Ok(send) => send,
            Err(error) => {
                self.apply_failure(
                    counters,
                    result,
                    ClientHeartbeatLoopOneTickRuntimeFailure::HeartbeatSend(error),
                    ClientHeartbeatLoopRetryReason::HeartbeatSendFailed,
                    ClientHeartbeatLoopIterationFailureKind::HeartbeatSend,
                    input.retry_attempts_used,
                    input.retry_policy,
                    input.controller_now,
                    input.max_sleep_micros,
                );
                return;
            }
        };
        self.commit_iteration(
            counters,
            result,
            ClientHeartbeatLoopIterationRuntimeResult::from_heartbeat_send(&send),
        );
        result.heartbeat_send = Some(send.clone());

        if matches!(
            send.handoff.ack_wait,
            ClientHeartbeatAckReceiveTimeoutDecision::DeadlineElapsed
        ) {
            self.commit_iteration(
                counters,
                result,
                ClientHeartbeatLoopIterationRuntimeResult::AckMissed {
                    missed_at: input.controller_now,
                },
            );
            return;
        }

        let ack = match self.ack_return.receive_one(socket, send.handoff.clone()) {
            Ok(ack) => ack,
            Err(error) => {
                if client_heartbeat_loop_ack_error_is_timeout(&error) {
                    self.commit_iteration(
                        counters,
                        result,
                        ClientHeartbeatLoopIterationRuntimeResult::AckMissed {
                            missed_at: input.controller_now,
                        },
                    );
                }
                self.apply_failure(
                    counters,
                    result,
                    ClientHeartbeatLoopOneTickRuntimeFailure::AckReceive(error.clone()),
                    client_heartbeat_loop_retry_reason_for_ack_error(&error),
                    ClientHeartbeatLoopIterationFailureKind::AckReceive,
                    input.retry_attempts_used,
                    input.retry_policy,
                    input.controller_now,
                    input.max_sleep_micros,
                );
                return;
            }
        };
        self.commit_iteration(
            counters,
            result,
            ClientHeartbeatLoopIterationRuntimeResult::from_ack_return(&ack),
        );
        result.ack_return = Some(ack.clone());

        let Some(stats_return) = ack.client_stats_return else {
            return;
        };
        match self.stats_return_send.send_one(socket, stats_return) {
            Ok(stats_send) => {
                self.commit_iteration(
                    counters,
                    result,
                    ClientHeartbeatLoopIterationRuntimeResult::from_stats_return_send(&stats_send),
                );
                result.stats_return_send = Some(stats_send);
            }
            Err(error) => {
                self.apply_failure(
                    counters,
                    result,
                    ClientHeartbeatLoopOneTickRuntimeFailure::ClientStatsReturnSend(error),
                    ClientHeartbeatLoopRetryReason::StatsReturnSendFailed,
                    ClientHeartbeatLoopIterationFailureKind::ClientStatsReturnSend,
                    input.retry_attempts_used,
                    input.retry_policy,
                    input.controller_now,
                    input.max_sleep_micros,
                );
            }
        }
    }

    fn commit_iteration(
        &self,
        counters: &mut ClientHeartbeatLoopCountersState,
        result: &mut ClientHeartbeatLoopOneTickRuntimeResult,
        iteration_result: ClientHeartbeatLoopIterationRuntimeResult,
    ) {
        let update = self.counters.commit_result(counters, iteration_result);
        result.final_counters = update.current.clone();
        result.counters_updates.push(update);
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_failure(
        &self,
        counters: &mut ClientHeartbeatLoopCountersState,
        result: &mut ClientHeartbeatLoopOneTickRuntimeResult,
        failure: ClientHeartbeatLoopOneTickRuntimeFailure,
        retry_reason: ClientHeartbeatLoopRetryReason,
        failure_kind: ClientHeartbeatLoopIterationFailureKind,
        attempts_used: u32,
        retry_policy: ClientHeartbeatLoopRetryPolicy,
        failed_at: TimestampMicros,
        max_sleep_micros: u64,
    ) {
        let retry = self
            .retry
            .apply_failure(ClientHeartbeatLoopRetryApplyInput {
                reason: retry_reason,
                failure_kind,
                attempts_used,
                policy: retry_policy,
                failed_at,
                max_sleep_micros,
            });
        self.commit_iteration(counters, result, retry.failure_result.clone());
        result.retry = Some(retry);
        result.failure = Some(failure);
    }
}

/// Minimal controller boundary for one pre-loop client heartbeat step.
///
/// It connects the body decision to either a typed send handoff, a bounded
/// sleep plan, or an iteration result that can later be committed to counters.
/// It does not call socket I/O, actually sleep, retry, or run a loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopControllerBoundary {
    sleep: ClientHeartbeatLoopSleepBoundary,
}

impl ClientHeartbeatLoopControllerBoundary {
    pub fn plan_next(
        &self,
        input: ClientHeartbeatLoopControllerInput,
    ) -> ClientHeartbeatLoopControllerPlan {
        match input.body_result {
            ClientHeartbeatLoopBodyResult::OwnershipNotReady(decision) => {
                ClientHeartbeatLoopControllerPlan::OwnershipNotReady { decision }
            }
            ClientHeartbeatLoopBodyResult::Stop { reason, log } => {
                ClientHeartbeatLoopControllerPlan::Stop {
                    reason,
                    log,
                    iteration_result: ClientHeartbeatLoopIterationRuntimeResult::Stopped { reason },
                }
            }
            ClientHeartbeatLoopBodyResult::Wait {
                next_heartbeat_due_at,
                log,
            } => {
                let sleep = self.sleep.plan_sleep(ClientHeartbeatLoopSleepInput {
                    now: input.now,
                    wake_at: next_heartbeat_due_at,
                    max_sleep_micros: input.max_sleep_micros,
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                });
                ClientHeartbeatLoopControllerPlan::Sleep {
                    sleep,
                    log,
                    iteration_result: ClientHeartbeatLoopIterationRuntimeResult::Waited {
                        next_heartbeat_due_at,
                    },
                }
            }
            ClientHeartbeatLoopBodyResult::SendHeartbeat { handoff, log } => {
                ClientHeartbeatLoopControllerPlan::SendHeartbeat { handoff, log }
            }
        }
    }
}

fn client_heartbeat_loop_controller_action(
    plan: &ClientHeartbeatLoopControllerPlan,
) -> ClientHeartbeatLoopControllerAction {
    match plan {
        ClientHeartbeatLoopControllerPlan::OwnershipNotReady { .. } => {
            ClientHeartbeatLoopControllerAction::OwnershipNotReady
        }
        ClientHeartbeatLoopControllerPlan::Stop { .. } => ClientHeartbeatLoopControllerAction::Stop,
        ClientHeartbeatLoopControllerPlan::Sleep { .. } => {
            ClientHeartbeatLoopControllerAction::Sleep
        }
        ClientHeartbeatLoopControllerPlan::SendHeartbeat { .. } => {
            ClientHeartbeatLoopControllerAction::SendHeartbeat
        }
    }
}

fn client_heartbeat_loop_controller_iteration_result(
    plan: &ClientHeartbeatLoopControllerPlan,
) -> Option<ClientHeartbeatLoopIterationRuntimeResult> {
    match plan {
        ClientHeartbeatLoopControllerPlan::Stop {
            iteration_result, ..
        }
        | ClientHeartbeatLoopControllerPlan::Sleep {
            iteration_result, ..
        } => Some(iteration_result.clone()),
        ClientHeartbeatLoopControllerPlan::OwnershipNotReady { .. }
        | ClientHeartbeatLoopControllerPlan::SendHeartbeat { .. } => None,
    }
}

fn client_heartbeat_loop_ack_error_is_timeout(
    error: &ClientHeartbeatLoopAckObservationReturnError,
) -> bool {
    matches!(
        error,
        ClientHeartbeatLoopAckObservationReturnError::AckReceive(
            ClientResponseReceiveError::Receive(
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            )
        )
    )
}

fn client_heartbeat_loop_retry_reason_for_ack_error(
    error: &ClientHeartbeatLoopAckObservationReturnError,
) -> ClientHeartbeatLoopRetryReason {
    if client_heartbeat_loop_ack_error_is_timeout(error) {
        ClientHeartbeatLoopRetryReason::AckReceiveTimeout
    } else {
        ClientHeartbeatLoopRetryReason::AckDecodeFailed
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

/// Startup config for the one-shot auth + heartbeat PoC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAuthHeartbeatPocStartupConfig {
    pub destination: SocketAddr,
    pub response_timeout_ms: u64,
    pub request: AuthRequest,
    pub heartbeat_short_status: Option<String>,
}

/// Result of one auth round trip followed by one heartbeat round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAuthHeartbeatPocOutcome {
    pub destination: SocketAddr,
    pub request: AuthRequest,
    pub auth_request_bytes: Vec<u8>,
    pub auth_request_bytes_sent: usize,
    pub auth_response_source: SocketAddr,
    pub auth_response_bytes: Vec<u8>,
    pub auth_response: AuthResponse,
    pub heartbeat: Heartbeat,
    pub heartbeat_bytes: Vec<u8>,
    pub heartbeat_bytes_sent: usize,
    pub heartbeat_ack_source: SocketAddr,
    pub heartbeat_ack_bytes: Vec<u8>,
    pub heartbeat_ack: HeartbeatAck,
}

/// Result of auth + heartbeat round trip followed by one ClientStats observation send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAuthHeartbeatStatsPocOutcome {
    pub heartbeat: ClientAuthHeartbeatPocOutcome,
    pub heartbeat_ack_observation: HeartbeatAckObservation,
    pub heartbeat_observation_carrier: HeartbeatObservationCarrier,
    pub client_stats: ClientStats,
    pub client_stats_bytes: Vec<u8>,
    pub client_stats_bytes_sent: usize,
}

/// Return mode used by the one-tick heartbeat runtime launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatOneTickRuntimeMode {
    HeartbeatOnly,
    HeartbeatWithStats,
}

impl ClientHeartbeatOneTickRuntimeMode {
    fn ack_observation_return(self) -> ClientHeartbeatAckObservationReturnMode {
        match self {
            Self::HeartbeatOnly => ClientHeartbeatAckObservationReturnMode::Disabled,
            Self::HeartbeatWithStats => {
                ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck
            }
        }
    }

    fn default_short_status(self) -> &'static str {
        match self {
            Self::HeartbeatOnly => "one-tick-runtime",
            Self::HeartbeatWithStats => "one-tick-runtime-stats",
        }
    }
}

/// Startup config for one auth round trip followed by one client loop tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatOneTickRuntimeStartupConfig {
    pub mode: ClientHeartbeatOneTickRuntimeMode,
    pub destination: SocketAddr,
    pub response_timeout_ms: u64,
    pub request: AuthRequest,
    pub heartbeat_interval_micros: u64,
    pub max_ack_socket_wait_micros: u64,
    pub max_sleep_micros: u64,
    pub retry_policy: ClientHeartbeatLoopRetryPolicy,
    pub short_status: Option<String>,
}

/// Outcome of one auth round trip followed by one client heartbeat loop tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatOneTickRuntimeOutcome {
    pub mode: ClientHeartbeatOneTickRuntimeMode,
    pub destination: SocketAddr,
    pub request: AuthRequest,
    pub auth_request_bytes: Vec<u8>,
    pub auth_request_bytes_sent: usize,
    pub auth_response_source: SocketAddr,
    pub auth_response_bytes: Vec<u8>,
    pub auth_response: AuthResponse,
    pub repeated_loop_handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff,
    pub runtime: ClientHeartbeatLoopOneTickRuntimeResult,
}

/// Static handoff produced by the launcher for a future repeated loop owner.
///
/// This names the configuration and accepted-auth identity that a future
/// repeated loop would keep after launcher/bootstrap work is complete. It does
/// not own a real socket, counters state, timers, shutdown execution, or retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopRepeatedRuntimeHandoff {
    pub mode: ClientHeartbeatOneTickRuntimeMode,
    pub destination: SocketAddr,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub protocol_version: ProtocolVersion,
    pub cadence: ClientHeartbeatLoopCadenceInput,
    pub stop_condition: ClientHeartbeatLoopStopCondition,
    pub max_ack_socket_wait_micros: u64,
    pub max_sleep_micros: u64,
    pub retry_policy: ClientHeartbeatLoopRetryPolicy,
    pub local_time_enabled: bool,
    pub short_status: Option<String>,
}

impl ClientHeartbeatLoopRepeatedRuntimeHandoff {
    pub fn build_one_tick_input(
        &self,
        now: TimestampMicros,
        state: ClientHeartbeatLoopStateSnapshot,
        retry_attempts_used: u32,
    ) -> ClientHeartbeatLoopOneTickRuntimeInput {
        ClientHeartbeatLoopOneTickRuntimeInput {
            destination: self.destination,
            body: ClientHeartbeatLoopBodyInput {
                ownership: ClientHeartbeatLoopOwnershipInput {
                    client_id: self.client_id.clone(),
                    run_id: self.run_id.clone(),
                    protocol_version: self.protocol_version,
                    auth_accepted: true,
                    socket_bound: true,
                },
                policy: ClientHeartbeatLoopPolicyInput {
                    client_id: self.client_id.clone(),
                    run_id: self.run_id.clone(),
                    now,
                    cadence: self.cadence,
                    stop_condition: self.stop_condition,
                    state,
                },
                max_ack_socket_wait_micros: self.max_ack_socket_wait_micros,
            },
            local_time: self.local_time_enabled.then_some(now),
            short_status: self.short_status.clone(),
            controller_now: now,
            max_sleep_micros: self.max_sleep_micros,
            retry_policy: self.retry_policy,
            retry_attempts_used,
        }
    }
}

/// Dynamic inputs owned by a future repeated loop body for one iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopRepeatedRuntimeBodyInput {
    pub handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff,
    pub now: TimestampMicros,
    pub stop_requested: bool,
    pub retry_attempts_used: u32,
}

/// Result of one future repeated loop body delegation into the one-tick runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopRepeatedRuntimeBodyResult {
    pub handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff,
    pub one_tick_input: ClientHeartbeatLoopOneTickRuntimeInput,
    pub runtime: ClientHeartbeatLoopOneTickRuntimeResult,
    pub shutdown: ClientHeartbeatLoopShutdownDecision,
}

/// Coarse outer-loop action after one repeated body step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopOuterControllerAction {
    ContinueLoop,
    StopLoop,
}

/// Final outer-controller result after observing one repeated body step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopOuterControllerResult {
    pub action: ClientHeartbeatLoopOuterControllerAction,
    pub shutdown: ClientHeartbeatLoopShutdownDecision,
}

/// Boundary that classifies one repeated body step for a future outer loop.
///
/// This boundary does not run timers, retries, reconnects, or process
/// shutdown. It only maps one repeated body result to a coarse outer-loop
/// action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopOuterControllerBoundary;

impl ClientHeartbeatLoopOuterControllerBoundary {
    pub fn observe(
        &self,
        body: &ClientHeartbeatLoopRepeatedRuntimeBodyResult,
    ) -> ClientHeartbeatLoopOuterControllerResult {
        let action = match body.shutdown {
            ClientHeartbeatLoopShutdownDecision::Continue => {
                ClientHeartbeatLoopOuterControllerAction::ContinueLoop
            }
            ClientHeartbeatLoopShutdownDecision::Stop { .. } => {
                ClientHeartbeatLoopOuterControllerAction::StopLoop
            }
        };

        ClientHeartbeatLoopOuterControllerResult {
            action,
            shutdown: body.shutdown,
        }
    }
}

/// Result of mapping one shutdown decision to future apply work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopShutdownApplyResult {
    ContinueLoop,
    StopLoop {
        reason: ClientHeartbeatLoopPolicyReason,
        cleanup_required: bool,
    },
}

/// Boundary that prepares typed shutdown-apply work without executing it.
///
/// This does not flush logs, close sockets, stop threads, or clean up
/// resources. It only names what a future outer loop/apply layer must do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopShutdownApplyBoundary;

impl ClientHeartbeatLoopShutdownApplyBoundary {
    pub fn apply(
        &self,
        decision: ClientHeartbeatLoopShutdownDecision,
    ) -> ClientHeartbeatLoopShutdownApplyResult {
        match decision {
            ClientHeartbeatLoopShutdownDecision::Continue => {
                ClientHeartbeatLoopShutdownApplyResult::ContinueLoop
            }
            ClientHeartbeatLoopShutdownDecision::Stop { reason } => {
                ClientHeartbeatLoopShutdownApplyResult::StopLoop {
                    reason,
                    cleanup_required: true,
                }
            }
        }
    }
}

/// Full one-step result for future outer repeated-loop orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
    pub body: ClientHeartbeatLoopRepeatedRuntimeBodyResult,
    pub controller: ClientHeartbeatLoopOuterControllerResult,
    pub shutdown_apply: ClientHeartbeatLoopShutdownApplyResult,
}

/// Why a future completed loop lifecycle would stop after one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopLifecycleStopReason {
    CallerRequestedStop,
    PolicyRequestedStop {
        reason: ClientHeartbeatLoopPolicyReason,
    },
}

/// Input for deciding the next state of a future completed loop lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopLifecycleInput {
    pub continue_requested: bool,
    pub step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult,
}

/// Result of one lifecycle decision after an outer repeated-loop step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopLifecycleResult {
    pub step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult,
    pub continue_loop: bool,
    pub stop_reason: Option<ClientHeartbeatLoopLifecycleStopReason>,
    pub cleanup_required: bool,
}

/// Timer wait decision extracted from lifecycle output for a future completed loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopTimerWaitDecision {
    NoWait,
    Wait {
        sleep: ClientHeartbeatLoopSleepDecision,
    },
}

/// Retry execution handoff extracted from lifecycle output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopRetryExecutionResult {
    NoRetryScheduled,
    RetryScheduled {
        retry: ClientHeartbeatLoopRetryApplyResult,
    },
}

/// Cleanup sequencing result after lifecycle stops a future completed loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopCleanupSequencingResult {
    NoCleanup,
    BeginCleanup {
        stop_reason: ClientHeartbeatLoopLifecycleStopReason,
    },
}

/// Sequencing result that connects lifecycle to timer, retry, and cleanup work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopSequencingResult {
    pub lifecycle: ClientHeartbeatLoopLifecycleResult,
    pub timer_wait: ClientHeartbeatLoopTimerWaitDecision,
    pub retry_execution: ClientHeartbeatLoopRetryExecutionResult,
    pub cleanup: ClientHeartbeatLoopCleanupSequencingResult,
}

/// Ordered next step for a future completed loop body after sequencing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopStepOrdering {
    ContinueImmediately,
    WaitThenContinue {
        sleep: ClientHeartbeatLoopSleepDecision,
    },
    RetryThenContinue {
        retry: ClientHeartbeatLoopRetryApplyResult,
    },
}

/// Handoff from sequencing into a future completed loop body step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedBodySequencingHandoff {
    pub sequencing: ClientHeartbeatLoopSequencingResult,
    pub ordering: ClientHeartbeatLoopStepOrdering,
}

/// Stop result returned before a future completed loop body would begin cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCompletedBodyStopResult {
    pub stop_reason: ClientHeartbeatLoopLifecycleStopReason,
    pub cleanup: ClientHeartbeatLoopCleanupSequencingResult,
}

/// Result of mapping sequencing output into future completed-loop body ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopStepOrderingResult {
    Continue {
        handoff: ClientHeartbeatLoopCompletedBodySequencingHandoff,
    },
    Stop {
        result: ClientHeartbeatLoopCompletedBodyStopResult,
    },
}

/// Input for one minimal completed-loop-equivalent client step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedStepRuntimeInput {
    pub continue_requested: bool,
    pub body: ClientHeartbeatLoopRepeatedRuntimeBodyInput,
}

/// Result of connecting one repeated step through lifecycle and ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedStepRuntimeResult {
    pub step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult,
    pub lifecycle: ClientHeartbeatLoopLifecycleResult,
    pub sequencing: ClientHeartbeatLoopSequencingResult,
    pub ordering: ClientHeartbeatLoopStepOrderingResult,
    pub final_counters: ClientHeartbeatLoopCountersState,
}

/// Stop handoff from a one-step completed runtime into future cleanup ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopWhileLoopStopHandoff {
    pub stop: ClientHeartbeatLoopCompletedBodyStopResult,
    pub final_counters: ClientHeartbeatLoopCountersState,
}

/// Caller-facing result after handing one completed step to eventual while-loop ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCallerContractResult {
    Continue {
        ordering: ClientHeartbeatLoopCompletedBodySequencingHandoff,
        final_counters: ClientHeartbeatLoopCountersState,
    },
    Stop {
        handoff: ClientHeartbeatLoopWhileLoopStopHandoff,
    },
}

/// Caller-owned stop flag refresh for one eventual repeated invocation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopStopRefreshInput {
    pub now: TimestampMicros,
    pub stop_requested: bool,
}

/// Carry state that an eventual while-loop would keep for the next step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopIterationCarryState {
    pub ordering: ClientHeartbeatLoopStepOrdering,
    pub final_counters: ClientHeartbeatLoopCountersState,
    pub next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput,
}

/// Result of one repeated-invocation skeleton turn after caller contract refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopSkeletonResult {
    Continue {
        carry: ClientHeartbeatLoopIterationCarryState,
    },
    Stop {
        handoff: ClientHeartbeatLoopWhileLoopStopHandoff,
    },
}

/// Cleanup trigger extracted before any future cleanup work would run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCleanupTrigger {
    pub handoff: ClientHeartbeatLoopWhileLoopStopHandoff,
}

/// Ordered apply result for future timer / retry / cleanup execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopApplyOrderResult {
    ContinueWithoutApply {
        carry: ClientHeartbeatLoopIterationCarryState,
    },
    ApplyTimerThenContinue {
        sleep: ClientHeartbeatLoopSleepDecision,
        carry: ClientHeartbeatLoopIterationCarryState,
    },
    ApplyRetryThenContinue {
        retry: ClientHeartbeatLoopRetryApplyResult,
        carry: ClientHeartbeatLoopIterationCarryState,
    },
    TriggerCleanup {
        trigger: ClientHeartbeatLoopCleanupTrigger,
    },
}

/// Why a future completed continuous loop outer shell would stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopShellStopReason {
    CleanupRequested {
        stop_reason: ClientHeartbeatLoopLifecycleStopReason,
    },
}

/// Caller-facing result of one outer-shell planning step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopShellResult {
    Continue {
        apply_order: ClientHeartbeatLoopApplyOrderResult,
    },
    Stop {
        reason: ClientHeartbeatLoopShellStopReason,
        trigger: ClientHeartbeatLoopCleanupTrigger,
    },
}

/// Why the caller-facing shell runner would stop after one outer-shell step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopShellRunnerStopReason {
    CleanupRequested {
        stop_reason: ClientHeartbeatLoopLifecycleStopReason,
    },
}

/// Caller-facing result of running one completed-loop outer shell turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopShellRunnerResult {
    Continue {
        apply_order: ClientHeartbeatLoopApplyOrderResult,
    },
    Stop {
        reason: ClientHeartbeatLoopShellRunnerStopReason,
        trigger: ClientHeartbeatLoopCleanupTrigger,
    },
}

/// Why eventual repeated invocation would stop after one shell-runner turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopRepeatedInvocationStopReason {
    CleanupRequested {
        stop_reason: ClientHeartbeatLoopLifecycleStopReason,
    },
}

/// Continue-state carry that a future actual while-loop would consume next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopRepeatedInvocationNextStepCarry {
    ContinueImmediately {
        carry: ClientHeartbeatLoopIterationCarryState,
    },
    ApplyTimerThenContinue {
        sleep: ClientHeartbeatLoopSleepDecision,
        carry: ClientHeartbeatLoopIterationCarryState,
    },
    ApplyRetryThenContinue {
        retry: ClientHeartbeatLoopRetryApplyResult,
        carry: ClientHeartbeatLoopIterationCarryState,
    },
}

/// Result of mapping one shell-runner turn into eventual repeated invocation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopRepeatedInvocationResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Stop {
        reason: ClientHeartbeatLoopRepeatedInvocationStopReason,
        trigger: ClientHeartbeatLoopCleanupTrigger,
    },
}

/// Stop handoff returned from a future actual while-loop step into cleanup ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopActualWhileLoopStopHandoff {
    pub reason: ClientHeartbeatLoopRepeatedInvocationStopReason,
    pub trigger: ClientHeartbeatLoopCleanupTrigger,
}

/// Caller-facing result of one future actual while-loop invocation step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopInvocationStepResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Stop {
        handoff: ClientHeartbeatLoopActualWhileLoopStopHandoff,
    },
}

/// Explicit cleanup plan extracted after the while-loop decides to stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCleanupPlan {
    CleanupOnStop {
        trigger: ClientHeartbeatLoopCleanupTrigger,
    },
}

/// Input owned by cleanup responsibility after loop control stops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCleanupResponsibilityInput {
    pub handoff: ClientHeartbeatLoopActualWhileLoopStopHandoff,
    pub plan: ClientHeartbeatLoopCleanupPlan,
}

/// Result of deciding whether loop control continues or cleanup takes ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCleanupResponsibilityResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Cleanup {
        input: ClientHeartbeatLoopCleanupResponsibilityInput,
    },
}

/// Explicit stop-only input handed from cleanup responsibility into cleanup ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCleanupOrderingInput {
    pub handoff: ClientHeartbeatLoopActualWhileLoopStopHandoff,
    pub plan: ClientHeartbeatLoopCleanupPlan,
}

impl ClientHeartbeatLoopCleanupOrderingInput {
    pub fn from_responsibility(
        responsibility: ClientHeartbeatLoopCleanupResponsibilityResult,
    ) -> Result<Self, ClientHeartbeatLoopRepeatedInvocationNextStepCarry> {
        match responsibility {
            ClientHeartbeatLoopCleanupResponsibilityResult::Continue { carry } => Err(carry),
            ClientHeartbeatLoopCleanupResponsibilityResult::Cleanup { input } => Ok(Self {
                handoff: input.handoff,
                plan: input.plan,
            }),
        }
    }
}

/// Ordered cleanup plan produced after stop-only cleanup ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopOrderedCleanupPlan {
    CleanupOnStop {
        trigger: ClientHeartbeatLoopCleanupTrigger,
    },
}

/// Ordered handoff returned before any future cleanup execution runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCleanupOrderingHandoff {
    pub stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason,
    pub ordered_plan: ClientHeartbeatLoopOrderedCleanupPlan,
}

/// Result of mapping cleanup responsibility into cleanup ordering state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCleanupOrderingResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Ordered {
        handoff: ClientHeartbeatLoopCleanupOrderingHandoff,
    },
}

/// Explicit stop-only input handed from cleanup ordering into execution planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCleanupExecutionInput {
    pub handoff: ClientHeartbeatLoopCleanupOrderingHandoff,
}

impl ClientHeartbeatLoopCleanupExecutionInput {
    pub fn from_ordering(
        ordering: ClientHeartbeatLoopCleanupOrderingResult,
    ) -> Result<Self, ClientHeartbeatLoopRepeatedInvocationNextStepCarry> {
        match ordering {
            ClientHeartbeatLoopCleanupOrderingResult::Continue { carry } => Err(carry),
            ClientHeartbeatLoopCleanupOrderingResult::Ordered { handoff } => Ok(Self { handoff }),
        }
    }
}

/// Future stop-path cleanup actions that remain ordered but unexecuted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopFutureCleanupAction {
    FinalFlush,
    LogWriterInvocation,
    ResourceRelease,
}

/// Stop-only cleanup execution plan returned before any real side effects run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCleanupExecutionPlan {
    CleanupOnStop {
        trigger: ClientHeartbeatLoopCleanupTrigger,
        future_actions: [ClientHeartbeatLoopFutureCleanupAction; 3],
    },
}

/// Execution planning handoff returned before future cleanup side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCleanupExecutionPlanningHandoff {
    pub stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason,
    pub execution_plan: ClientHeartbeatLoopCleanupExecutionPlan,
}

/// Typed cleanup execution planning result returned before any real cleanup runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCleanupExecutionResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Planned {
        handoff: ClientHeartbeatLoopCleanupExecutionPlanningHandoff,
    },
}

/// Explicit stop-only input handed from execution planning into side-effect apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCleanupSideEffectInput {
    pub handoff: ClientHeartbeatLoopCleanupExecutionPlanningHandoff,
}

impl ClientHeartbeatLoopCleanupSideEffectInput {
    pub fn from_execution_planning(
        execution: ClientHeartbeatLoopCleanupExecutionResult,
    ) -> Result<Self, ClientHeartbeatLoopRepeatedInvocationNextStepCarry> {
        match execution {
            ClientHeartbeatLoopCleanupExecutionResult::Continue { carry } => Err(carry),
            ClientHeartbeatLoopCleanupExecutionResult::Planned { handoff } => Ok(Self { handoff }),
        }
    }
}

/// Explicit cleanup side effects after stop-only planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopCleanupAppliedAction {
    FinalFlush,
    LogWriterInvocation,
    ResourceRelease,
}

/// Result returned after stop-path cleanup side-effect apply remains explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCleanupSideEffectApplyResult {
    pub stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason,
    pub cleanup_completed: bool,
    pub applied_actions: [ClientHeartbeatLoopCleanupAppliedAction; 3],
}

/// Typed cleanup side-effect result returned from the stop path only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCleanupSideEffectResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Applied {
        result: ClientHeartbeatLoopCleanupSideEffectApplyResult,
    },
}

/// Explicit stop-only input handed into terminal completed-loop stop-path output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedLoopStopPathInput {
    pub result: ClientHeartbeatLoopCleanupSideEffectApplyResult,
}

impl ClientHeartbeatLoopCompletedLoopStopPathInput {
    pub fn from_cleanup_side_effect(
        side_effect: ClientHeartbeatLoopCleanupSideEffectResult,
    ) -> Result<Self, ClientHeartbeatLoopRepeatedInvocationNextStepCarry> {
        match side_effect {
            ClientHeartbeatLoopCleanupSideEffectResult::Continue { carry } => Err(carry),
            ClientHeartbeatLoopCleanupSideEffectResult::Applied { result } => Ok(Self { result }),
        }
    }
}

/// Terminal stop-path output for a future completed continuous heartbeat loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopTerminalStopPathOutput {
    pub stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason,
    pub cleanup_completed: bool,
    pub applied_actions: [ClientHeartbeatLoopCleanupAppliedAction; 3],
}

/// Completed-loop handoff returned only after stop-path cleanup apply finishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedLoopStopPathHandoff {
    pub output: ClientHeartbeatLoopTerminalStopPathOutput,
}

/// Result of connecting cleanup apply state into completed-loop stop-path output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCompletedLoopStopPathResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Stop {
        handoff: ClientHeartbeatLoopCompletedLoopStopPathHandoff,
    },
}

/// Explicit stop-only input handed into actual while-loop termination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopActualWhileLoopTerminationInput {
    pub handoff: ClientHeartbeatLoopCompletedLoopStopPathHandoff,
}

impl ClientHeartbeatLoopActualWhileLoopTerminationInput {
    pub fn from_completed_loop_stop_path(
        stop_path: ClientHeartbeatLoopCompletedLoopStopPathResult,
    ) -> Result<Self, ClientHeartbeatLoopRepeatedInvocationNextStepCarry> {
        match stop_path {
            ClientHeartbeatLoopCompletedLoopStopPathResult::Continue { carry } => Err(carry),
            ClientHeartbeatLoopCompletedLoopStopPathResult::Stop { handoff } => {
                Ok(Self { handoff })
            }
        }
    }
}

/// Terminal output returned after actual while-loop termination becomes explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopActualWhileLoopTerminalOutput {
    pub stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason,
    pub cleanup_completed: bool,
    pub applied_actions: [ClientHeartbeatLoopCleanupAppliedAction; 3],
}

/// Result of mapping completed-loop stop output into actual while-loop termination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopActualWhileLoopTerminationResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Terminated {
        output: ClientHeartbeatLoopActualWhileLoopTerminalOutput,
    },
}

/// Explicit stop-only input handed into completed continuous heartbeat loop body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedBodyInput {
    pub output: ClientHeartbeatLoopActualWhileLoopTerminalOutput,
}

impl ClientHeartbeatLoopCompletedBodyInput {
    pub fn from_actual_while_loop_termination(
        termination: ClientHeartbeatLoopActualWhileLoopTerminationResult,
    ) -> Result<Self, ClientHeartbeatLoopRepeatedInvocationNextStepCarry> {
        match termination {
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Continue { carry } => Err(carry),
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Terminated { output } => {
                Ok(Self { output })
            }
        }
    }
}

/// Terminal stop-path output surfaced by the completed continuous heartbeat loop body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedBodyTerminalOutput {
    pub stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason,
    pub cleanup_completed: bool,
    pub applied_actions: [ClientHeartbeatLoopCleanupAppliedAction; 3],
}

/// Result of integrating actual while-loop termination into completed loop body state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCompletedBodyIntegrationResult {
    Continue {
        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    },
    Stop {
        output: ClientHeartbeatLoopCompletedBodyTerminalOutput,
    },
}

/// Explicit continue-path input handed into future timer / retry / reconnect planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopTimerRetryReconnectIntegrationInput {
    pub carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
}

impl ClientHeartbeatLoopTimerRetryReconnectIntegrationInput {
    pub fn from_completed_body_result(
        completed_body: ClientHeartbeatLoopCompletedBodyIntegrationResult,
    ) -> Result<Self, ClientHeartbeatLoopCompletedBodyTerminalOutput> {
        match completed_body {
            ClientHeartbeatLoopCompletedBodyIntegrationResult::Continue { carry } => {
                Ok(Self { carry })
            }
            ClientHeartbeatLoopCompletedBodyIntegrationResult::Stop { output } => Err(output),
        }
    }
}

/// Continue-path handoff preserved for future timer / retry / reconnect planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff {
    pub carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
}

/// Result of connecting completed loop body output into future timer / retry / reconnect planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopTimerRetryReconnectIntegrationResult {
    ContinuePlanning {
        handoff: ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff,
    },
    Stop {
        output: ClientHeartbeatLoopCompletedBodyTerminalOutput,
    },
}

/// Explicit continue-path input handed into future actual timer / retry / reconnect execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput {
    pub handoff: ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff,
}

impl ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput {
    pub fn from_planning_handoff(
        planning: ClientHeartbeatLoopTimerRetryReconnectIntegrationResult,
    ) -> Result<Self, ClientHeartbeatLoopCompletedBodyTerminalOutput> {
        match planning {
            ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::ContinuePlanning {
                handoff,
            } => Ok(Self { handoff }),
            ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::Stop { output } => Err(output),
        }
    }
}

/// Future actual timer wait execution scope kept explicit before runtime behavior exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopFutureActualTimerWaitAction {
    NoTimerWait,
    TimerWait {
        sleep: ClientHeartbeatLoopSleepDecision,
    },
}

/// Future actual retry execution scope kept explicit before runtime behavior exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopFutureActualRetryExecutionAction {
    NoRetryExecution,
    RetryExecution {
        retry: ClientHeartbeatLoopRetryApplyResult,
    },
}

/// Future actual reconnect execution scope kept explicit before runtime behavior exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopFutureActualReconnectExecutionAction {
    NoReconnectExecution,
}

/// Continue-path handoff returned before actual timer / retry / reconnect execution exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff {
    pub carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    pub timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction,
    pub retry_execution: ClientHeartbeatLoopFutureActualRetryExecutionAction,
    pub reconnect_execution: ClientHeartbeatLoopFutureActualReconnectExecutionAction,
}

/// Result of connecting planning handoff into future actual timer / retry / reconnect execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult {
    ContinueExecution {
        handoff: ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff,
    },
    Stop {
        output: ClientHeartbeatLoopCompletedBodyTerminalOutput,
    },
}

/// Explicit continue-path input handed into completed continuous heartbeat loop body connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedContinuousBodyConnectionInput {
    pub handoff: ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff,
}

impl ClientHeartbeatLoopCompletedContinuousBodyConnectionInput {
    pub fn from_actual_execution_integration(
        execution: ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult,
    ) -> Result<Self, ClientHeartbeatLoopCompletedBodyTerminalOutput> {
        match execution {
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::ContinueExecution {
                handoff,
            } => Ok(Self { handoff }),
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::Stop { output } => {
                Err(output)
            }
        }
    }
}

/// Continue-path output surfaced for future completed continuous heartbeat loop body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput {
    pub carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry,
    pub timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction,
    pub retry_execution: ClientHeartbeatLoopFutureActualRetryExecutionAction,
    pub reconnect_execution: ClientHeartbeatLoopFutureActualReconnectExecutionAction,
}

/// Result of connecting actual execution integration into completed loop body state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCompletedContinuousBodyConnectionResult {
    Continue {
        output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput,
    },
    Stop {
        output: ClientHeartbeatLoopCompletedBodyTerminalOutput,
    },
}

/// Final result surfaced by the minimal completed continuous heartbeat loop body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopCompletedContinuousBodyResult {
    Continue {
        output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput,
    },
    Stop {
        output: ClientHeartbeatLoopCompletedBodyTerminalOutput,
    },
}

/// Explicit continue-path input handed into future heartbeat timeout notice wakeup planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput {
    pub output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput,
}

impl ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput {
    pub fn from_completed_continuous_body(
        body: ClientHeartbeatLoopCompletedContinuousBodyResult,
    ) -> Result<Self, ClientHeartbeatLoopCompletedBodyTerminalOutput> {
        match body {
            ClientHeartbeatLoopCompletedContinuousBodyResult::Continue { output } => {
                Ok(Self { output })
            }
            ClientHeartbeatLoopCompletedContinuousBodyResult::Stop { output } => Err(output),
        }
    }
}

/// Future heartbeat timeout notice wakeup scope kept explicit before runtime behavior exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan {
    WakeupNotNeeded,
    WakeupDuringTimerWait {
        sleep: ClientHeartbeatLoopSleepDecision,
    },
}

/// Continue-path handoff returned when future heartbeat timeout notice wakeup remains explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff {
    pub output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput,
    pub wakeup: ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan,
}

/// Result of determining whether future heartbeat timeout notice wakeup follow-up is needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult {
    ContinueWithoutWakeup {
        output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput,
    },
    ContinueWithWakeup {
        handoff: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff,
    },
    Stop {
        output: ClientHeartbeatLoopCompletedBodyTerminalOutput,
    },
}

/// Explicit continue-path input handed into future heartbeat timeout notice wakeup execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput {
    pub handoff: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff,
}

impl ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput {
    pub fn from_wakeup_planning(
        planning: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult,
    ) -> Result<Self, ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult> {
        match planning {
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithWakeup {
                handoff,
            } => Ok(Self { handoff }),
            passthrough => Err(passthrough),
        }
    }
}

/// Explicit result of applying heartbeat timeout notice wakeup execution shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupApplyResult {
    WakeupApplied {
        wakeup: ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan,
    },
}

/// Continue-path output surfaced after heartbeat timeout notice wakeup execution is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionOutput {
    pub output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput,
    pub wakeup_apply: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupApplyResult,
}

/// Result of connecting wakeup planning into future wakeup execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult {
    ContinueWithoutWakeupExecution {
        output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput,
    },
    ContinueWithWakeupExecutionApplied {
        output: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionOutput,
    },
    Stop {
        output: ClientHeartbeatLoopCompletedBodyTerminalOutput,
    },
}

/// Boundary that connects one step result to future completed-loop lifecycle flow.
///
/// This does not run a while-loop, sleep, reconnect, flush logs, close
/// sockets, or execute cleanup. It only decides whether the next completed-loop
/// lifecycle state would continue or enter stop/cleanup flow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopLifecycleBoundary;

impl ClientHeartbeatLoopLifecycleBoundary {
    pub fn plan_next(
        &self,
        input: ClientHeartbeatLoopLifecycleInput,
    ) -> ClientHeartbeatLoopLifecycleResult {
        let stop_reason = if !input.continue_requested {
            Some(ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop)
        } else {
            match input.step.shutdown_apply {
                ClientHeartbeatLoopShutdownApplyResult::ContinueLoop => None,
                ClientHeartbeatLoopShutdownApplyResult::StopLoop { reason, .. } => {
                    Some(ClientHeartbeatLoopLifecycleStopReason::PolicyRequestedStop { reason })
                }
            }
        };

        ClientHeartbeatLoopLifecycleResult {
            cleanup_required: stop_reason.is_some(),
            continue_loop: stop_reason.is_none(),
            stop_reason,
            step: input.step,
        }
    }
}

/// Boundary that converts lifecycle output into timer, retry, and cleanup handoffs.
///
/// This does not actually sleep, re-run failed work, reconnect sockets, flush
/// logs, or execute cleanup. It only decides which future completed-loop
/// follow-up stage would run next.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopSequencingBoundary;

impl ClientHeartbeatLoopSequencingBoundary {
    pub fn plan_next(
        &self,
        lifecycle: ClientHeartbeatLoopLifecycleResult,
    ) -> ClientHeartbeatLoopSequencingResult {
        if let Some(stop_reason) = lifecycle.stop_reason {
            return ClientHeartbeatLoopSequencingResult {
                lifecycle,
                timer_wait: ClientHeartbeatLoopTimerWaitDecision::NoWait,
                retry_execution: ClientHeartbeatLoopRetryExecutionResult::NoRetryScheduled,
                cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup { stop_reason },
            };
        }

        let retry_execution = lifecycle
            .step
            .body
            .runtime
            .retry
            .clone()
            .map(|retry| ClientHeartbeatLoopRetryExecutionResult::RetryScheduled { retry })
            .unwrap_or(ClientHeartbeatLoopRetryExecutionResult::NoRetryScheduled);

        let timer_wait = match &retry_execution {
            ClientHeartbeatLoopRetryExecutionResult::RetryScheduled { retry } => {
                match retry.sleep {
                    ClientHeartbeatLoopSleepDecision::NoSleep { .. } => {
                        ClientHeartbeatLoopTimerWaitDecision::NoWait
                    }
                    sleep @ ClientHeartbeatLoopSleepDecision::Sleep { .. } => {
                        ClientHeartbeatLoopTimerWaitDecision::Wait { sleep }
                    }
                }
            }
            ClientHeartbeatLoopRetryExecutionResult::NoRetryScheduled => {
                match &lifecycle.step.body.runtime.controller.plan {
                    ClientHeartbeatLoopControllerPlan::Sleep { sleep, .. } => match *sleep {
                        ClientHeartbeatLoopSleepDecision::NoSleep { .. } => {
                            ClientHeartbeatLoopTimerWaitDecision::NoWait
                        }
                        sleep @ ClientHeartbeatLoopSleepDecision::Sleep { .. } => {
                            ClientHeartbeatLoopTimerWaitDecision::Wait { sleep }
                        }
                    },
                    ClientHeartbeatLoopControllerPlan::OwnershipNotReady { .. }
                    | ClientHeartbeatLoopControllerPlan::Stop { .. }
                    | ClientHeartbeatLoopControllerPlan::SendHeartbeat { .. } => {
                        ClientHeartbeatLoopTimerWaitDecision::NoWait
                    }
                }
            }
        };

        ClientHeartbeatLoopSequencingResult {
            lifecycle,
            timer_wait,
            retry_execution,
            cleanup: ClientHeartbeatLoopCleanupSequencingResult::NoCleanup,
        }
    }
}

/// Boundary that fixes the next execution order for a future completed loop body.
///
/// It consumes typed sequencing output and chooses whether the future body
/// would stop for cleanup, run retry work, wait, or continue immediately. It
/// does not block, retry, reconnect, or run a real while-loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopStepOrderingBoundary;

impl ClientHeartbeatLoopStepOrderingBoundary {
    pub fn plan_next(
        &self,
        sequencing: ClientHeartbeatLoopSequencingResult,
    ) -> ClientHeartbeatLoopStepOrderingResult {
        if let ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup { stop_reason } =
            sequencing.cleanup
        {
            return ClientHeartbeatLoopStepOrderingResult::Stop {
                result: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason,
                    cleanup: sequencing.cleanup,
                },
            };
        }

        let ordering = match &sequencing.retry_execution {
            ClientHeartbeatLoopRetryExecutionResult::RetryScheduled { retry } => {
                ClientHeartbeatLoopStepOrdering::RetryThenContinue {
                    retry: retry.clone(),
                }
            }
            ClientHeartbeatLoopRetryExecutionResult::NoRetryScheduled => {
                match sequencing.timer_wait {
                    ClientHeartbeatLoopTimerWaitDecision::Wait { sleep } => {
                        ClientHeartbeatLoopStepOrdering::WaitThenContinue { sleep }
                    }
                    ClientHeartbeatLoopTimerWaitDecision::NoWait => {
                        ClientHeartbeatLoopStepOrdering::ContinueImmediately
                    }
                }
            }
        };

        ClientHeartbeatLoopStepOrderingResult::Continue {
            handoff: ClientHeartbeatLoopCompletedBodySequencingHandoff {
                sequencing,
                ordering,
            },
        }
    }
}

/// Minimal runtime boundary before a full completed continuous heartbeat loop.
///
/// It connects one repeated-loop step to lifecycle, sequencing, and ordering
/// exactly once. It does not repeat, sleep, retry, reconnect, or execute
/// cleanup; it only returns the typed next-step decision for caller-owned loop
/// orchestration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCompletedStepRuntimeBoundary {
    step: ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary,
    lifecycle: ClientHeartbeatLoopLifecycleBoundary,
    sequencing: ClientHeartbeatLoopSequencingBoundary,
    ordering: ClientHeartbeatLoopStepOrderingBoundary,
}

impl ClientHeartbeatLoopCompletedStepRuntimeBoundary {
    pub fn run_one(
        &self,
        socket: &UdpSocket,
        counters: &mut ClientHeartbeatLoopCountersState,
        input: ClientHeartbeatLoopCompletedStepRuntimeInput,
    ) -> ClientHeartbeatLoopCompletedStepRuntimeResult {
        let step = self.step.run_one(socket, counters, input.body);
        let lifecycle = self.lifecycle.plan_next(ClientHeartbeatLoopLifecycleInput {
            continue_requested: input.continue_requested,
            step: step.clone(),
        });
        let sequencing = self.sequencing.plan_next(lifecycle.clone());
        let ordering = self.ordering.plan_next(sequencing.clone());

        ClientHeartbeatLoopCompletedStepRuntimeResult {
            step,
            lifecycle,
            sequencing,
            ordering,
            final_counters: counters.clone(),
        }
    }
}

/// Boundary that maps one completed-step runtime result into caller-owned loop ownership.
///
/// This boundary does not run a while-loop, refresh stop flags, sleep, retry,
/// reconnect, or execute cleanup. It only tells the eventual caller whether it
/// still owns another step or should hand stop state into cleanup flow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopWhileLoopOwnershipBoundary;

impl ClientHeartbeatLoopWhileLoopOwnershipBoundary {
    pub fn handoff(
        &self,
        runtime: ClientHeartbeatLoopCompletedStepRuntimeResult,
    ) -> ClientHeartbeatLoopCallerContractResult {
        match runtime.ordering {
            ClientHeartbeatLoopStepOrderingResult::Continue { handoff: ordering } => {
                ClientHeartbeatLoopCallerContractResult::Continue {
                    ordering,
                    final_counters: runtime.final_counters,
                }
            }
            ClientHeartbeatLoopStepOrderingResult::Stop { result: stop } => {
                ClientHeartbeatLoopCallerContractResult::Stop {
                    handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                        stop,
                        final_counters: runtime.final_counters,
                    },
                }
            }
        }
    }
}

/// Boundary that maps caller contract into one repeated-invocation skeleton turn.
///
/// This boundary does not run an actual while-loop, sleep, retry, reconnect,
/// or cleanup. It only refreshes caller-owned stop input and produces the next
/// carry state that a future repeated invocation loop would use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopSkeletonBoundary;

impl ClientHeartbeatLoopSkeletonBoundary {
    pub fn plan_next(
        &self,
        contract: ClientHeartbeatLoopCallerContractResult,
        refresh: ClientHeartbeatLoopStopRefreshInput,
    ) -> ClientHeartbeatLoopSkeletonResult {
        match contract {
            ClientHeartbeatLoopCallerContractResult::Continue {
                ordering,
                final_counters,
            } => {
                let retry_attempts_used = match &ordering.ordering {
                    ClientHeartbeatLoopStepOrdering::ContinueImmediately
                    | ClientHeartbeatLoopStepOrdering::WaitThenContinue { .. } => 0,
                    ClientHeartbeatLoopStepOrdering::RetryThenContinue { retry } => {
                        match retry.retry_decision {
                            ClientHeartbeatLoopRetryDecision::RetryLater {
                                next_attempt, ..
                            } => next_attempt,
                            ClientHeartbeatLoopRetryDecision::GiveUp { attempts_used, .. } => {
                                attempts_used
                            }
                        }
                    }
                };

                ClientHeartbeatLoopSkeletonResult::Continue {
                    carry: ClientHeartbeatLoopIterationCarryState {
                        ordering: ordering.ordering.clone(),
                        final_counters: final_counters.clone(),
                        next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                            continue_requested: !refresh.stop_requested,
                            body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                                handoff: ordering.sequencing.lifecycle.step.body.handoff.clone(),
                                now: refresh.now,
                                stop_requested: refresh.stop_requested,
                                retry_attempts_used,
                            },
                        },
                    },
                }
            }
            ClientHeartbeatLoopCallerContractResult::Stop { handoff } => {
                ClientHeartbeatLoopSkeletonResult::Stop { handoff }
            }
        }
    }
}

/// Boundary that fixes the future call order for timer / retry / cleanup apply.
///
/// This boundary does not execute timer waits, retry work, reconnects, or
/// cleanup. It only tells a future runtime which apply branch would run next.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopApplyOrderBoundary;

impl ClientHeartbeatLoopApplyOrderBoundary {
    pub fn plan_next(
        &self,
        skeleton: ClientHeartbeatLoopSkeletonResult,
    ) -> ClientHeartbeatLoopApplyOrderResult {
        match skeleton {
            ClientHeartbeatLoopSkeletonResult::Stop { handoff } => {
                ClientHeartbeatLoopApplyOrderResult::TriggerCleanup {
                    trigger: ClientHeartbeatLoopCleanupTrigger { handoff },
                }
            }
            ClientHeartbeatLoopSkeletonResult::Continue { carry } => match &carry.ordering {
                ClientHeartbeatLoopStepOrdering::ContinueImmediately => {
                    ClientHeartbeatLoopApplyOrderResult::ContinueWithoutApply { carry }
                }
                ClientHeartbeatLoopStepOrdering::WaitThenContinue { sleep } => {
                    ClientHeartbeatLoopApplyOrderResult::ApplyTimerThenContinue {
                        sleep: *sleep,
                        carry,
                    }
                }
                ClientHeartbeatLoopStepOrdering::RetryThenContinue { retry } => {
                    ClientHeartbeatLoopApplyOrderResult::ApplyRetryThenContinue {
                        retry: retry.clone(),
                        carry,
                    }
                }
            },
        }
    }
}

/// Boundary that exposes the smallest caller-facing outer shell result.
///
/// This boundary does not repeat a loop, execute timer waits, perform retries,
/// reconnect sockets, flush logs, or run cleanup. It only converts apply-order
/// planning into a final continue-or-stop shell result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopOuterShellBoundary;

impl ClientHeartbeatLoopOuterShellBoundary {
    pub fn plan_next(
        &self,
        apply_order: ClientHeartbeatLoopApplyOrderResult,
    ) -> ClientHeartbeatLoopShellResult {
        match apply_order {
            ClientHeartbeatLoopApplyOrderResult::TriggerCleanup { trigger } => {
                ClientHeartbeatLoopShellResult::Stop {
                    reason: ClientHeartbeatLoopShellStopReason::CleanupRequested {
                        stop_reason: trigger.handoff.stop.stop_reason,
                    },
                    trigger,
                }
            }
            apply_order => ClientHeartbeatLoopShellResult::Continue { apply_order },
        }
    }
}

/// Boundary that exposes one caller-facing shell runner turn above outer shell.
///
/// This boundary does not repeat, block on timers, retry failed work,
/// reconnect sockets, flush logs, or execute cleanup. It only runs the outer
/// shell mapping once and returns the caller-owned continue-or-stop result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopShellRunnerBoundary {
    outer_shell: ClientHeartbeatLoopOuterShellBoundary,
}

impl ClientHeartbeatLoopShellRunnerBoundary {
    pub fn run_one(
        &self,
        apply_order: ClientHeartbeatLoopApplyOrderResult,
    ) -> ClientHeartbeatLoopShellRunnerResult {
        match self.outer_shell.plan_next(apply_order) {
            ClientHeartbeatLoopShellResult::Continue { apply_order } => {
                ClientHeartbeatLoopShellRunnerResult::Continue { apply_order }
            }
            ClientHeartbeatLoopShellResult::Stop { reason, trigger } => {
                let reason = match reason {
                    ClientHeartbeatLoopShellStopReason::CleanupRequested { stop_reason } => {
                        ClientHeartbeatLoopShellRunnerStopReason::CleanupRequested { stop_reason }
                    }
                };

                ClientHeartbeatLoopShellRunnerResult::Stop { reason, trigger }
            }
        }
    }
}

/// Boundary that maps one shell-runner turn into repeated-invocation state.
///
/// This boundary does not run a real while-loop, execute timer waits, perform
/// retries, reconnect sockets, flush logs, or execute cleanup. It only
/// converts caller-facing shell-runner output into continue carry or stop
/// handoff for a future repeated invocation owner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopRepeatedInvocationBoundary;

impl ClientHeartbeatLoopRepeatedInvocationBoundary {
    pub fn plan_next(
        &self,
        runner: ClientHeartbeatLoopShellRunnerResult,
    ) -> ClientHeartbeatLoopRepeatedInvocationResult {
        match runner {
            ClientHeartbeatLoopShellRunnerResult::Continue { apply_order } => {
                let carry = match apply_order {
                    ClientHeartbeatLoopApplyOrderResult::ContinueWithoutApply { carry } => {
                        ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
                            carry,
                        }
                    }
                    ClientHeartbeatLoopApplyOrderResult::ApplyTimerThenContinue {
                        sleep,
                        carry,
                    } => {
                        ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                            sleep,
                            carry,
                        }
                    }
                    ClientHeartbeatLoopApplyOrderResult::ApplyRetryThenContinue {
                        retry,
                        carry,
                    } => {
                        ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyRetryThenContinue {
                            retry,
                            carry,
                        }
                    }
                    ClientHeartbeatLoopApplyOrderResult::TriggerCleanup { trigger } => {
                        return ClientHeartbeatLoopRepeatedInvocationResult::Stop {
                            reason:
                                ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                                    stop_reason: trigger.handoff.stop.stop_reason,
                                },
                            trigger,
                        };
                    }
                };

                ClientHeartbeatLoopRepeatedInvocationResult::Continue { carry }
            }
            ClientHeartbeatLoopShellRunnerResult::Stop { reason, trigger } => {
                let reason = match reason {
                    ClientHeartbeatLoopShellRunnerStopReason::CleanupRequested { stop_reason } => {
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason,
                        }
                    }
                };

                ClientHeartbeatLoopRepeatedInvocationResult::Stop { reason, trigger }
            }
        }
    }
}

/// Boundary that exposes the smallest caller-facing step of a future actual while-loop.
///
/// This boundary does not actually repeat, wait on timers, execute retries,
/// reconnect sockets, flush logs, or run cleanup. It only converts one
/// repeated-invocation result into the next while-loop step result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopActualWhileLoopBoundary;

impl ClientHeartbeatLoopActualWhileLoopBoundary {
    pub fn plan_next(
        &self,
        repeated: ClientHeartbeatLoopRepeatedInvocationResult,
    ) -> ClientHeartbeatLoopInvocationStepResult {
        match repeated {
            ClientHeartbeatLoopRepeatedInvocationResult::Continue { carry } => {
                ClientHeartbeatLoopInvocationStepResult::Continue { carry }
            }
            ClientHeartbeatLoopRepeatedInvocationResult::Stop { reason, trigger } => {
                ClientHeartbeatLoopInvocationStepResult::Stop {
                    handoff: ClientHeartbeatLoopActualWhileLoopStopHandoff { reason, trigger },
                }
            }
        }
    }
}

/// Boundary that explicitly transfers ownership from loop control to cleanup.
///
/// Cleanup is triggered only after the while-loop step has already stopped. It
/// is not entered on retry planning or normal continue iterations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCleanupResponsibilityBoundary;

impl ClientHeartbeatLoopCleanupResponsibilityBoundary {
    pub fn plan_next(
        &self,
        step: ClientHeartbeatLoopInvocationStepResult,
    ) -> ClientHeartbeatLoopCleanupResponsibilityResult {
        match step {
            ClientHeartbeatLoopInvocationStepResult::Continue { carry } => {
                ClientHeartbeatLoopCleanupResponsibilityResult::Continue { carry }
            }
            ClientHeartbeatLoopInvocationStepResult::Stop { handoff } => {
                ClientHeartbeatLoopCleanupResponsibilityResult::Cleanup {
                    input: ClientHeartbeatLoopCleanupResponsibilityInput {
                        plan: ClientHeartbeatLoopCleanupPlan::CleanupOnStop {
                            trigger: handoff.trigger.clone(),
                        },
                        handoff,
                    },
                }
            }
        }
    }
}

/// Boundary that fixes explicit stop-only cleanup ordering before execution.
///
/// This boundary does not execute cleanup, flush logs, close sockets, or run
/// final release logic. It only orders stop-path cleanup into a typed handoff
/// that a later cleanup execution boundary can consume.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCleanupOrderingBoundary;

impl ClientHeartbeatLoopCleanupOrderingBoundary {
    pub fn plan_next(
        &self,
        responsibility: ClientHeartbeatLoopCleanupResponsibilityResult,
    ) -> ClientHeartbeatLoopCleanupOrderingResult {
        match ClientHeartbeatLoopCleanupOrderingInput::from_responsibility(responsibility) {
            Err(carry) => ClientHeartbeatLoopCleanupOrderingResult::Continue { carry },
            Ok(input) => {
                let ordered_plan = match input.plan {
                    ClientHeartbeatLoopCleanupPlan::CleanupOnStop { trigger } => {
                        ClientHeartbeatLoopOrderedCleanupPlan::CleanupOnStop { trigger }
                    }
                };

                ClientHeartbeatLoopCleanupOrderingResult::Ordered {
                    handoff: ClientHeartbeatLoopCleanupOrderingHandoff {
                        stop_reason: input.handoff.reason,
                        ordered_plan,
                    },
                }
            }
        }
    }
}

/// Boundary that exposes explicit cleanup execution planning without side effects.
///
/// This boundary does not flush logs, close sockets, sleep, retry, reconnect,
/// or run final cleanup. It only converts ordered stop-path cleanup into a
/// future execution plan that later side-effect code must consume.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCleanupExecutionBoundary;

impl ClientHeartbeatLoopCleanupExecutionBoundary {
    pub fn plan_next(
        &self,
        ordering: ClientHeartbeatLoopCleanupOrderingResult,
    ) -> ClientHeartbeatLoopCleanupExecutionResult {
        match ClientHeartbeatLoopCleanupExecutionInput::from_ordering(ordering) {
            Err(carry) => ClientHeartbeatLoopCleanupExecutionResult::Continue { carry },
            Ok(input) => {
                let execution_plan = match input.handoff.ordered_plan {
                    ClientHeartbeatLoopOrderedCleanupPlan::CleanupOnStop { trigger } => {
                        ClientHeartbeatLoopCleanupExecutionPlan::CleanupOnStop {
                            trigger,
                            future_actions: [
                                ClientHeartbeatLoopFutureCleanupAction::FinalFlush,
                                ClientHeartbeatLoopFutureCleanupAction::LogWriterInvocation,
                                ClientHeartbeatLoopFutureCleanupAction::ResourceRelease,
                            ],
                        }
                    }
                };

                ClientHeartbeatLoopCleanupExecutionResult::Planned {
                    handoff: ClientHeartbeatLoopCleanupExecutionPlanningHandoff {
                        stop_reason: input.handoff.stop_reason,
                        execution_plan,
                    },
                }
            }
        }
    }
}

/// Boundary that applies only stop-path planned cleanup side effects.
///
/// This boundary consumes cleanup execution planning output only. It does not
/// re-order cleanup actions, trigger cleanup on continue/retry paths, or add
/// complex flush/log/release bodies beyond explicit ordered placeholder apply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCleanupSideEffectBoundary;

impl ClientHeartbeatLoopCleanupSideEffectBoundary {
    pub fn apply(
        &self,
        execution: ClientHeartbeatLoopCleanupExecutionResult,
    ) -> ClientHeartbeatLoopCleanupSideEffectResult {
        match ClientHeartbeatLoopCleanupSideEffectInput::from_execution_planning(execution) {
            Err(carry) => ClientHeartbeatLoopCleanupSideEffectResult::Continue { carry },
            Ok(input) => {
                let applied_actions = match input.handoff.execution_plan {
                    ClientHeartbeatLoopCleanupExecutionPlan::CleanupOnStop {
                        future_actions,
                        ..
                    } => future_actions.map(|action| match action {
                        ClientHeartbeatLoopFutureCleanupAction::FinalFlush => {
                            ClientHeartbeatLoopCleanupAppliedAction::FinalFlush
                        }
                        ClientHeartbeatLoopFutureCleanupAction::LogWriterInvocation => {
                            ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation
                        }
                        ClientHeartbeatLoopFutureCleanupAction::ResourceRelease => {
                            ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease
                        }
                    }),
                };

                ClientHeartbeatLoopCleanupSideEffectResult::Applied {
                    result: ClientHeartbeatLoopCleanupSideEffectApplyResult {
                        stop_reason: input.handoff.stop_reason,
                        cleanup_completed: true,
                        applied_actions,
                    },
                }
            }
        }
    }
}

/// Boundary that turns cleanup apply output into terminal completed-loop stop output.
///
/// This boundary consumes cleanup side-effect output only. It does not re-run
/// cleanup, re-interpret cleanup ordering or execution planning, or change
/// continue-path ownership. It only exposes explicit terminal stop-path output
/// for a future completed continuous heartbeat loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCompletedLoopStopPathBoundary;

impl ClientHeartbeatLoopCompletedLoopStopPathBoundary {
    pub fn plan_next(
        &self,
        side_effect: ClientHeartbeatLoopCleanupSideEffectResult,
    ) -> ClientHeartbeatLoopCompletedLoopStopPathResult {
        match ClientHeartbeatLoopCompletedLoopStopPathInput::from_cleanup_side_effect(side_effect) {
            Err(carry) => ClientHeartbeatLoopCompletedLoopStopPathResult::Continue { carry },
            Ok(input) => ClientHeartbeatLoopCompletedLoopStopPathResult::Stop {
                handoff: ClientHeartbeatLoopCompletedLoopStopPathHandoff {
                    output: ClientHeartbeatLoopTerminalStopPathOutput {
                        stop_reason: input.result.stop_reason,
                        cleanup_completed: input.result.cleanup_completed,
                        applied_actions: input.result.applied_actions,
                    },
                },
            },
        }
    }
}

/// Boundary that exposes explicit actual while-loop termination from stop output.
///
/// This boundary consumes completed-loop stop-path output only. It does not
/// re-run cleanup, re-interpret cleanup ordering/execution planning/side
/// effects, or add business logic into the actual while-loop body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopActualWhileLoopTerminationBoundary;

impl ClientHeartbeatLoopActualWhileLoopTerminationBoundary {
    pub fn plan_next(
        &self,
        stop_path: ClientHeartbeatLoopCompletedLoopStopPathResult,
    ) -> ClientHeartbeatLoopActualWhileLoopTerminationResult {
        match ClientHeartbeatLoopActualWhileLoopTerminationInput::from_completed_loop_stop_path(
            stop_path,
        ) {
            Err(carry) => ClientHeartbeatLoopActualWhileLoopTerminationResult::Continue { carry },
            Ok(input) => ClientHeartbeatLoopActualWhileLoopTerminationResult::Terminated {
                output: ClientHeartbeatLoopActualWhileLoopTerminalOutput {
                    stop_reason: input.handoff.output.stop_reason,
                    cleanup_completed: input.handoff.output.cleanup_completed,
                    applied_actions: input.handoff.output.applied_actions,
                },
            },
        }
    }
}

/// Boundary that integrates actual while-loop termination into completed loop body output.
///
/// This boundary consumes actual while-loop termination only. It does not
/// re-run or reinterpret cleanup logic, and it does not implement future
/// timer wait, retry execution, reconnect, or timeout wakeup behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCompletedBodyIntegrationBoundary;

impl ClientHeartbeatLoopCompletedBodyIntegrationBoundary {
    pub fn plan_next(
        &self,
        termination: ClientHeartbeatLoopActualWhileLoopTerminationResult,
    ) -> ClientHeartbeatLoopCompletedBodyIntegrationResult {
        match ClientHeartbeatLoopCompletedBodyInput::from_actual_while_loop_termination(termination)
        {
            Err(carry) => ClientHeartbeatLoopCompletedBodyIntegrationResult::Continue { carry },
            Ok(input) => ClientHeartbeatLoopCompletedBodyIntegrationResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason: input.output.stop_reason,
                    cleanup_completed: input.output.cleanup_completed,
                    applied_actions: input.output.applied_actions,
                },
            },
        }
    }
}

/// Boundary that exposes future timer / retry / reconnect planning ownership.
///
/// This boundary consumes completed loop body output only. It does not execute
/// timer wait, retry execution, reconnect, or timeout wakeup behavior, and it
/// does not reinterpret stop-path cleanup logic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopTimerRetryReconnectIntegrationBoundary;

impl ClientHeartbeatLoopTimerRetryReconnectIntegrationBoundary {
    pub fn plan_next(
        &self,
        completed_body: ClientHeartbeatLoopCompletedBodyIntegrationResult,
    ) -> ClientHeartbeatLoopTimerRetryReconnectIntegrationResult {
        match ClientHeartbeatLoopTimerRetryReconnectIntegrationInput::from_completed_body_result(
            completed_body,
        ) {
            Ok(input) => {
                ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::ContinuePlanning {
                    handoff: ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff {
                        carry: input.carry,
                    },
                }
            }
            Err(output) => ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::Stop { output },
        }
    }
}

/// Boundary that exposes explicit future actual timer / retry / reconnect execution actions.
///
/// This boundary consumes planning handoff only. It does not execute timer
/// wait, retry work, reconnects, or timeout wakeup behavior, and it keeps stop
/// passthrough explicit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary;

impl ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary {
    pub fn plan_next(
        &self,
        planning: ClientHeartbeatLoopTimerRetryReconnectIntegrationResult,
    ) -> ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult {
        match ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput::from_planning_handoff(
            planning,
        ) {
            Err(output) => {
                ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::Stop { output }
            }
            Ok(input) => {
                let carry = input.handoff.carry;
                let (timer_wait, retry_execution) = match &carry {
                    ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
                        ..
                    } => (
                        ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait,
                        ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
                    ),
                    ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                        sleep,
                        ..
                    } => (
                        ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                            sleep: *sleep,
                        },
                        ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
                    ),
                    ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyRetryThenContinue {
                        retry,
                        ..
                    } => (
                        ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait,
                        ClientHeartbeatLoopFutureActualRetryExecutionAction::RetryExecution {
                            retry: retry.clone(),
                        },
                    ),
                };

                ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::ContinueExecution {
                    handoff: ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff {
                        carry,
                        timer_wait,
                        retry_execution,
                        reconnect_execution:
                            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
                    },
                }
            }
        }
    }
}

/// Boundary that connects explicit future actual execution actions into completed loop body state.
///
/// This boundary consumes actual timer / retry / reconnect execution
/// integration only. It does not execute timer wait, retry execution,
/// reconnect, or timeout wakeup behavior, and it keeps stop passthrough
/// explicit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCompletedContinuousBodyConnectionBoundary;

impl ClientHeartbeatLoopCompletedContinuousBodyConnectionBoundary {
    pub fn plan_next(
        &self,
        execution: ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult,
    ) -> ClientHeartbeatLoopCompletedContinuousBodyConnectionResult {
        match ClientHeartbeatLoopCompletedContinuousBodyConnectionInput::from_actual_execution_integration(
            execution,
        ) {
            Ok(input) => ClientHeartbeatLoopCompletedContinuousBodyConnectionResult::Continue {
                output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput {
                    carry: input.handoff.carry,
                    timer_wait: input.handoff.timer_wait,
                    retry_execution: input.handoff.retry_execution,
                    reconnect_execution: input.handoff.reconnect_execution,
                },
            },
            Err(output) => ClientHeartbeatLoopCompletedContinuousBodyConnectionResult::Stop {
                output,
            },
        }
    }
}

/// Minimal completed continuous heartbeat loop body composition over existing boundaries.
///
/// This boundary wires repeated invocation, stop-path cleanup flow, actual
/// while-loop termination, completed body integration, timer/retry/reconnect
/// integration, actual execution integration, and completed body connection
/// once. It does not execute timer wait, retry work, reconnects, timeout
/// wakeup, or a real while-loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopCompletedContinuousBodyBoundary {
    actual_while_loop: ClientHeartbeatLoopActualWhileLoopBoundary,
    cleanup_responsibility: ClientHeartbeatLoopCleanupResponsibilityBoundary,
    cleanup_ordering: ClientHeartbeatLoopCleanupOrderingBoundary,
    cleanup_execution: ClientHeartbeatLoopCleanupExecutionBoundary,
    cleanup_side_effect: ClientHeartbeatLoopCleanupSideEffectBoundary,
    stop_path: ClientHeartbeatLoopCompletedLoopStopPathBoundary,
    termination: ClientHeartbeatLoopActualWhileLoopTerminationBoundary,
    completed_body_integration: ClientHeartbeatLoopCompletedBodyIntegrationBoundary,
    timer_retry_reconnect_integration: ClientHeartbeatLoopTimerRetryReconnectIntegrationBoundary,
    actual_execution: ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary,
    body_connection: ClientHeartbeatLoopCompletedContinuousBodyConnectionBoundary,
}

impl ClientHeartbeatLoopCompletedContinuousBodyBoundary {
    pub fn plan_next(
        &self,
        repeated: ClientHeartbeatLoopRepeatedInvocationResult,
    ) -> ClientHeartbeatLoopCompletedContinuousBodyResult {
        let step = self.actual_while_loop.plan_next(repeated);
        let responsibility = self.cleanup_responsibility.plan_next(step);
        let ordering = self.cleanup_ordering.plan_next(responsibility);
        let execution = self.cleanup_execution.plan_next(ordering);
        let side_effect = self.cleanup_side_effect.apply(execution);
        let stop_path = self.stop_path.plan_next(side_effect);
        let termination = self.termination.plan_next(stop_path);
        let completed_body = self.completed_body_integration.plan_next(termination);
        let planning = self
            .timer_retry_reconnect_integration
            .plan_next(completed_body);
        let actual_execution = self.actual_execution.plan_next(planning);

        match self.body_connection.plan_next(actual_execution) {
            ClientHeartbeatLoopCompletedContinuousBodyConnectionResult::Continue { output } => {
                ClientHeartbeatLoopCompletedContinuousBodyResult::Continue { output }
            }
            ClientHeartbeatLoopCompletedContinuousBodyConnectionResult::Stop { output } => {
                ClientHeartbeatLoopCompletedContinuousBodyResult::Stop { output }
            }
        }
    }
}

/// Boundary that exposes future heartbeat timeout notice wakeup need explicitly.
///
/// This boundary consumes completed continuous heartbeat loop body result only.
/// It does not execute timeout notice wakeup, timer wait, retry execution,
/// reconnect execution, or reinterpret cleanup logic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary;

impl ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary {
    pub fn plan_next(
        &self,
        body: ClientHeartbeatLoopCompletedContinuousBodyResult,
    ) -> ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult {
        match ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput::from_completed_continuous_body(
            body,
        ) {
            Err(output) => ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::Stop { output },
            Ok(input) => match input.output.timer_wait {
                ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait => {
                    ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithoutWakeup {
                        output: input.output,
                    }
                }
                ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait { sleep } => {
                    ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithWakeup {
                        handoff: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff {
                            output: input.output,
                            wakeup:
                                ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan::WakeupDuringTimerWait {
                                    sleep,
                                },
                        },
                    }
                }
            },
        }
    }
}

/// Boundary that exposes future heartbeat timeout notice wakeup execution explicitly.
///
/// This boundary consumes wakeup planning result only. It does not execute a
/// real wakeup side effect, timer wait, retry execution, reconnect execution,
/// or reinterpret cleanup logic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary;

impl ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary {
    pub fn apply(
        &self,
        planning: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult,
    ) -> ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult {
        match ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput::from_wakeup_planning(
            planning,
        ) {
            Ok(input) => {
                ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult::ContinueWithWakeupExecutionApplied {
                    output: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionOutput {
                        output: input.handoff.output,
                        wakeup_apply:
                            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupApplyResult::WakeupApplied {
                                wakeup: input.handoff.wakeup,
                            },
                    },
                }
            }
            Err(
                ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithoutWakeup {
                    output,
                },
            ) => {
                ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult::ContinueWithoutWakeupExecution {
                    output,
                }
            }
            Err(ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::Stop { output }) => {
                ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult::Stop { output }
            }
            Err(ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithWakeup {
                ..
            }) => unreachable!("continue-with-wakeup should already be converted into execution input"),
        }
    }
}

/// Boundary that shows how a future repeated loop body delegates one step.
///
/// This boundary owns only the one-iteration bridge from caller-owned loop
/// state into `ClientHeartbeatLoopOneTickRuntimeBoundary`. It does not repeat,
/// sleep, reconnect, execute shutdown, or decide process lifetime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopRepeatedRuntimeBodyBoundary {
    runtime: ClientHeartbeatLoopOneTickRuntimeBoundary,
}

impl ClientHeartbeatLoopRepeatedRuntimeBodyBoundary {
    pub fn run_one(
        &self,
        socket: &UdpSocket,
        counters: &mut ClientHeartbeatLoopCountersState,
        input: ClientHeartbeatLoopRepeatedRuntimeBodyInput,
    ) -> ClientHeartbeatLoopRepeatedRuntimeBodyResult {
        let state = counters.as_policy_snapshot(input.stop_requested);
        let one_tick_input =
            input
                .handoff
                .build_one_tick_input(input.now, state, input.retry_attempts_used);
        let runtime = self
            .runtime
            .run_one(socket, counters, one_tick_input.clone());
        let shutdown = runtime.controller.shutdown;

        ClientHeartbeatLoopRepeatedRuntimeBodyResult {
            handoff: input.handoff,
            one_tick_input,
            runtime,
            shutdown,
        }
    }
}

/// Boundary that shows the smallest outer repeated-loop step orchestration.
///
/// It runs one repeated body step, lets the outer controller classify it, and
/// converts the returned shutdown decision into typed apply work. It does not
/// repeat, sleep, reconnect, or execute shutdown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary {
    body: ClientHeartbeatLoopRepeatedRuntimeBodyBoundary,
    controller: ClientHeartbeatLoopOuterControllerBoundary,
    shutdown_apply: ClientHeartbeatLoopShutdownApplyBoundary,
}

impl ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary {
    pub fn run_one(
        &self,
        socket: &UdpSocket,
        counters: &mut ClientHeartbeatLoopCountersState,
        input: ClientHeartbeatLoopRepeatedRuntimeBodyInput,
    ) -> ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
        let body = self.body.run_one(socket, counters, input);
        let controller = self.controller.observe(&body);
        let shutdown_apply = self.shutdown_apply.apply(controller.shutdown);

        ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
            body,
            controller,
            shutdown_apply,
        }
    }
}

/// Inputs needed before the launcher can hand work to a future repeated loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopLauncherOwnershipInput {
    pub mode: ClientHeartbeatOneTickRuntimeMode,
    pub destination: SocketAddr,
    pub request: AuthRequest,
    pub auth_response: AuthResponse,
    pub socket_bound: bool,
    pub cadence: ClientHeartbeatLoopCadenceInput,
    pub stop_condition: ClientHeartbeatLoopStopCondition,
    pub max_ack_socket_wait_micros: u64,
    pub max_sleep_micros: u64,
    pub retry_policy: ClientHeartbeatLoopRetryPolicy,
    pub local_time_enabled: bool,
    pub short_status: Option<String>,
}

/// Result of launcher-side ownership preparation for a future repeated loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHeartbeatLoopLauncherOwnershipResult {
    pub ownership: ClientHeartbeatLoopOwnershipDecision,
    pub repeated_loop_handoff: Option<ClientHeartbeatLoopRepeatedRuntimeHandoff>,
}

/// Boundary that separates launcher/bootstrap ownership from a future repeated loop.
///
/// This checks only that accepted auth and a bound socket exist, then fixes the
/// static handoff a future repeated loop would own. It does not run the loop,
/// move a real socket, mutate counters, sleep, retry, or execute shutdown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatLoopLauncherOwnershipBoundary {
    ownership: ClientHeartbeatLoopOwnershipBoundary,
}

impl ClientHeartbeatLoopLauncherOwnershipBoundary {
    pub fn prepare(
        &self,
        input: ClientHeartbeatLoopLauncherOwnershipInput,
    ) -> ClientHeartbeatLoopLauncherOwnershipResult {
        let ownership = self.ownership.evaluate(ClientHeartbeatLoopOwnershipInput {
            client_id: input.request.client_id.clone(),
            run_id: input.request.run_id.clone(),
            protocol_version: input.request.protocol_version,
            auth_accepted: input.auth_response.accepted,
            socket_bound: input.socket_bound,
        });

        let repeated_loop_handoff = match &ownership {
            ClientHeartbeatLoopOwnershipDecision::Ready(_) => {
                Some(ClientHeartbeatLoopRepeatedRuntimeHandoff {
                    mode: input.mode,
                    destination: input.destination,
                    client_id: input.request.client_id,
                    run_id: input.request.run_id,
                    protocol_version: input.request.protocol_version,
                    cadence: input.cadence,
                    stop_condition: input.stop_condition,
                    max_ack_socket_wait_micros: input.max_ack_socket_wait_micros,
                    max_sleep_micros: input.max_sleep_micros,
                    retry_policy: input.retry_policy,
                    local_time_enabled: input.local_time_enabled,
                    short_status: input.short_status,
                })
            }
            ClientHeartbeatLoopOwnershipDecision::NotReady { .. } => None,
        };

        ClientHeartbeatLoopLauncherOwnershipResult {
            ownership,
            repeated_loop_handoff,
        }
    }
}

/// Launcher for one accepted auth round trip plus one client heartbeat loop tick.
///
/// This entry point is for manual/runtime wiring only. It binds one UDP socket,
/// performs one accepted auth round trip, then delegates exactly one tick to
/// `ClientHeartbeatLoopOneTickRuntimeBoundary`. It does not repeat, sleep, run
/// a completed continuous loop, or introduce an async runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClientHeartbeatOneTickRuntimeLauncher {
    encoder: ProtocolMessageEncoderBoundary,
    ownership: ClientHeartbeatLoopLauncherOwnershipBoundary,
    repeated_body: ClientHeartbeatLoopRepeatedRuntimeBodyBoundary,
}

impl ClientHeartbeatOneTickRuntimeLauncher {
    pub fn load_startup_config_from_path_with_mode(
        &self,
        path: impl AsRef<Path>,
        mode: ClientHeartbeatOneTickRuntimeMode,
    ) -> Result<ClientHeartbeatOneTickRuntimeStartupConfig, ClientHeartbeatOneTickRuntimeError>
    {
        let path = path.as_ref();
        let content =
            fs::read_to_string(path).map_err(|error| ClientHeartbeatOneTickRuntimeError::Io {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        self.load_startup_config_from_str_with_mode(&content, mode)
    }

    pub fn load_startup_config_from_str_with_mode(
        &self,
        input: &str,
        mode: ClientHeartbeatOneTickRuntimeMode,
    ) -> Result<ClientHeartbeatOneTickRuntimeStartupConfig, ClientHeartbeatOneTickRuntimeError>
    {
        let settings = parse_client_poc_settings(input)
            .map_err(ClientHeartbeatOneTickRuntimeError::from_auth_request_error)?;
        let destination = resolve_destination(&settings.server_host, settings.server_port)
            .map_err(ClientHeartbeatOneTickRuntimeError::from_auth_request_error)?;
        let heartbeat_interval_micros =
            u64::from(settings.heartbeat_interval_ms).saturating_mul(1_000);
        let response_timeout_ms = u64::from(settings.connect_timeout_ms);

        Ok(ClientHeartbeatOneTickRuntimeStartupConfig {
            mode,
            destination,
            response_timeout_ms,
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
            heartbeat_interval_micros,
            max_ack_socket_wait_micros: response_timeout_ms.saturating_mul(1_000),
            max_sleep_micros: heartbeat_interval_micros,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: DEFAULT_ONE_TICK_RETRY_ATTEMPTS,
                retry_delay_micros: heartbeat_interval_micros,
            },
            short_status: Some(mode.default_short_status().to_string()),
        })
    }

    pub fn run_once_from_path_with_mode(
        &self,
        path: impl AsRef<Path>,
        mode: ClientHeartbeatOneTickRuntimeMode,
    ) -> Result<ClientHeartbeatOneTickRuntimeOutcome, ClientHeartbeatOneTickRuntimeError> {
        let startup_config = self.load_startup_config_from_path_with_mode(path, mode)?;
        self.run_once(startup_config)
    }

    pub fn run_once(
        &self,
        startup_config: ClientHeartbeatOneTickRuntimeStartupConfig,
    ) -> Result<ClientHeartbeatOneTickRuntimeOutcome, ClientHeartbeatOneTickRuntimeError> {
        let socket = UdpSocket::bind(ephemeral_bind_address(startup_config.destination))
            .map_err(|error| ClientHeartbeatOneTickRuntimeError::Bind(error.kind()))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(
                startup_config.response_timeout_ms,
            )))
            .map_err(|error| ClientHeartbeatOneTickRuntimeError::SetReadTimeout(error.kind()))?;

        let mut auth_request_bytes = Vec::new();
        self.encoder
            .encode_message(
                EncodeContext {
                    protocol_version: startup_config.request.protocol_version,
                },
                &ProtocolMessage::AuthRequest(startup_config.request.clone()),
                &mut auth_request_bytes,
            )
            .map_err(ClientHeartbeatOneTickRuntimeError::Encode)?;
        let auth_request_bytes_sent = socket
            .send_to(&auth_request_bytes, startup_config.destination)
            .map_err(|error| ClientHeartbeatOneTickRuntimeError::Send(error.kind()))?;
        let (auth_response_source, auth_response_bytes, auth_response) =
            receive_auth_response(&socket, startup_config.request.protocol_version)
                .map_err(ClientHeartbeatOneTickRuntimeError::AuthResponse)?;
        if !auth_response.accepted {
            return Err(ClientHeartbeatOneTickRuntimeError::AuthRejected(
                auth_response,
            ));
        }

        let launcher = self
            .ownership
            .prepare(ClientHeartbeatLoopLauncherOwnershipInput {
                mode: startup_config.mode,
                destination: startup_config.destination,
                request: startup_config.request.clone(),
                auth_response: auth_response.clone(),
                socket_bound: true,
                cadence: ClientHeartbeatLoopCadenceInput {
                    heartbeat_interval_micros: startup_config.heartbeat_interval_micros,
                    ack_receive_timeout_micros: startup_config.max_ack_socket_wait_micros,
                    ack_observation_return: startup_config.mode.ack_observation_return(),
                },
                stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                max_ack_socket_wait_micros: startup_config.max_ack_socket_wait_micros,
                max_sleep_micros: startup_config.max_sleep_micros,
                retry_policy: startup_config.retry_policy,
                local_time_enabled: true,
                short_status: startup_config.short_status.clone(),
            });
        let repeated_loop_handoff = launcher
            .repeated_loop_handoff
            .expect("accepted auth and bound socket should produce launcher handoff");
        let mut counters = ClientHeartbeatLoopCountersState::default();
        let now = current_timestamp_micros();
        let policy_snapshot = counters.as_policy_snapshot(false);
        let repeated_body = self.repeated_body.run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                handoff: repeated_loop_handoff.clone(),
                now,
                stop_requested: policy_snapshot.stop_requested,
                retry_attempts_used: 0,
            },
        );
        let runtime = repeated_body.runtime;

        if runtime.failure.is_some() {
            return Err(ClientHeartbeatOneTickRuntimeError::RuntimeFailed(runtime));
        }
        if runtime.controller.action != ClientHeartbeatLoopControllerAction::SendHeartbeat {
            return Err(ClientHeartbeatOneTickRuntimeError::UnexpectedController(
                runtime.controller,
            ));
        }

        Ok(ClientHeartbeatOneTickRuntimeOutcome {
            mode: startup_config.mode,
            destination: startup_config.destination,
            request: startup_config.request,
            auth_request_bytes,
            auth_request_bytes_sent,
            auth_response_source,
            auth_response_bytes,
            auth_response,
            repeated_loop_handoff,
            runtime,
        })
    }
}

/// Convenience entry point for one auth round trip plus one heartbeat runtime tick.
pub fn run_auth_heartbeat_one_tick_runtime_from_path(
    path: impl AsRef<Path>,
) -> Result<ClientHeartbeatOneTickRuntimeOutcome, ClientHeartbeatOneTickRuntimeError> {
    ClientHeartbeatOneTickRuntimeLauncher::default()
        .run_once_from_path_with_mode(path, ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly)
}

/// Convenience entry point for one auth round trip plus one stats-returning heartbeat tick.
pub fn run_auth_heartbeat_stats_one_tick_runtime_from_path(
    path: impl AsRef<Path>,
) -> Result<ClientHeartbeatOneTickRuntimeOutcome, ClientHeartbeatOneTickRuntimeError> {
    ClientHeartbeatOneTickRuntimeLauncher::default()
        .run_once_from_path_with_mode(path, ClientHeartbeatOneTickRuntimeMode::HeartbeatWithStats)
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

/// Error from the one-shot auth + heartbeat PoC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientAuthHeartbeatPocError {
    AuthPoc(ClientAuthRequestPocError),
    Encode(ProtocolError),
    Bind(io::ErrorKind),
    SetReadTimeout(io::ErrorKind),
    Send(io::ErrorKind),
    AuthResponse(ClientResponseReceiveError),
    AuthRejected(AuthResponse),
    HeartbeatAck(ClientResponseReceiveError),
}

/// Error from one auth round trip plus one client heartbeat loop tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHeartbeatOneTickRuntimeError {
    Io { path: PathBuf, message: String },
    Config(ClientAuthRequestPocConfigError),
    Destination(ClientAuthRequestPocConfigError),
    Encode(ProtocolError),
    Bind(io::ErrorKind),
    SetReadTimeout(io::ErrorKind),
    Send(io::ErrorKind),
    AuthResponse(ClientResponseReceiveError),
    AuthRejected(AuthResponse),
    UnexpectedController(ClientHeartbeatLoopControllerResult),
    RuntimeFailed(ClientHeartbeatLoopOneTickRuntimeResult),
}

impl ClientHeartbeatOneTickRuntimeError {
    fn from_auth_request_error(error: ClientAuthRequestPocError) -> Self {
        match error {
            ClientAuthRequestPocError::Io { path, message } => Self::Io { path, message },
            ClientAuthRequestPocError::Config(error) => Self::Config(error),
            ClientAuthRequestPocError::Destination(error) => Self::Destination(error),
            ClientAuthRequestPocError::Encode(error) => Self::Encode(error),
            ClientAuthRequestPocError::Bind(error) => Self::Bind(error),
            ClientAuthRequestPocError::SetReadTimeout(error) => Self::SetReadTimeout(error),
            ClientAuthRequestPocError::Send(error) => Self::Send(error),
            ClientAuthRequestPocError::Receive(error) => {
                Self::AuthResponse(ClientResponseReceiveError::Receive(error))
            }
            ClientAuthRequestPocError::Decode(error) => {
                Self::AuthResponse(ClientResponseReceiveError::Decode(error))
            }
            ClientAuthRequestPocError::UnexpectedResponseMessage { actual } => {
                Self::AuthResponse(ClientResponseReceiveError::UnexpectedResponseMessage { actual })
            }
        }
    }
}

/// Error from receiving and decoding one expected client-side response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientResponseReceiveError {
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
    heartbeat_interval_ms: u32,
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
    heartbeat_interval_ms: Option<u32>,
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
            ("network", "heartbeat_interval_ms") => {
                parsed.heartbeat_interval_ms = Some(parse_poc_u32(value, line_number, key)?);
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
        heartbeat_interval_ms: parsed
            .heartbeat_interval_ms
            .unwrap_or(DEFAULT_HEARTBEAT_INTERVAL_MS),
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

fn receive_auth_response(
    socket: &UdpSocket,
    expected_protocol_version: ProtocolVersion,
) -> Result<(SocketAddr, Vec<u8>, AuthResponse), ClientResponseReceiveError> {
    let (source, bytes, message) = receive_protocol_message(socket, expected_protocol_version)?;
    let ProtocolMessage::AuthResponse(response) = message else {
        return Err(ClientResponseReceiveError::UnexpectedResponseMessage {
            actual: message.message_type(),
        });
    };
    Ok((source, bytes, response))
}

fn receive_heartbeat_ack(
    socket: &UdpSocket,
    expected_protocol_version: ProtocolVersion,
) -> Result<(SocketAddr, Vec<u8>, HeartbeatAck), ClientResponseReceiveError> {
    let (source, bytes, message) = receive_protocol_message(socket, expected_protocol_version)?;
    let ProtocolMessage::HeartbeatAck(ack) = message else {
        return Err(ClientResponseReceiveError::UnexpectedResponseMessage {
            actual: message.message_type(),
        });
    };
    Ok((source, bytes, ack))
}

fn receive_protocol_message(
    socket: &UdpSocket,
    expected_protocol_version: ProtocolVersion,
) -> Result<(SocketAddr, Vec<u8>, ProtocolMessage), ClientResponseReceiveError> {
    let mut response_buffer = vec![0_u8; UDP_PACKET_BUFFER_LEN];
    let (response_len, response_source) = socket
        .recv_from(&mut response_buffer)
        .map_err(|error| ClientResponseReceiveError::Receive(error.kind()))?;
    let response_bytes = response_buffer[..response_len].to_vec();
    let packet_view =
        decode_fixed_header(&response_bytes).map_err(ClientResponseReceiveError::Decode)?;
    let decode_context = DecodeContext {
        expected_protocol_version,
    };
    validate_protocol_version(decode_context, packet_view.header)
        .map_err(ClientResponseReceiveError::Decode)?;
    let decoded_message =
        decode_payload_by_message_type(decode_context, packet_view.header, packet_view.payload)
            .map_err(ClientResponseReceiveError::Decode)?;
    Ok((response_source, response_bytes, decoded_message))
}

fn current_timestamp_micros() -> TimestampMicros {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or(0);
    TimestampMicros(u64::try_from(micros).unwrap_or(u64::MAX))
}

fn timestamp_saturating_add(timestamp: TimestampMicros, micros: u64) -> TimestampMicros {
    TimestampMicros(timestamp.0.saturating_add(micros))
}

fn client_heartbeat_loop_log(
    input: &ClientHeartbeatLoopPolicyInput,
    reason: ClientHeartbeatLoopPolicyReason,
) -> ClientHeartbeatLoopLogHandoff {
    ClientHeartbeatLoopLogHandoff {
        client_id: input.client_id.clone(),
        run_id: input.run_id.clone(),
        observed_at: input.now,
        reason,
        heartbeat_interval_micros: input.cadence.heartbeat_interval_micros,
        ack_receive_timeout_micros: input.cadence.ack_receive_timeout_micros,
        sent_heartbeats: input.state.sent_heartbeats,
        received_acks: input.state.received_acks,
        missed_acks: input.state.missed_acks,
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
        decode_auth_request_payload, decode_client_stats_payload, decode_fixed_header,
        decode_heartbeat_payload, AuthResponseReasonCode,
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
    fn client_heartbeat_one_tick_runtime_launcher_loads_example_config() {
        let launcher = ClientHeartbeatOneTickRuntimeLauncher::default();
        let config = launcher
            .load_startup_config_from_str_with_mode(
                include_str!("../../../configs/examples/client.accepted.example.toml"),
                ClientHeartbeatOneTickRuntimeMode::HeartbeatWithStats,
            )
            .expect("accepted example config should load");

        assert_eq!(
            config.mode,
            ClientHeartbeatOneTickRuntimeMode::HeartbeatWithStats
        );
        assert_eq!(config.destination, "127.0.0.1:5000".parse().unwrap());
        assert_eq!(config.request.message_type, MessageType::AuthRequest);
        assert_eq!(config.request.protocol_version, ProtocolVersion(1));
        assert_eq!(config.request.client_id, ClientId("player1".to_string()));
        assert_eq!(
            config.request.run_id,
            RunId("streamsync-dev-session".to_string())
        );
        assert_eq!(config.response_timeout_ms, 5_000);
        assert_eq!(config.heartbeat_interval_micros, 1_000_000);
        assert_eq!(config.max_ack_socket_wait_micros, 5_000_000);
        assert_eq!(config.max_sleep_micros, 1_000_000);
        assert_eq!(
            config.retry_policy,
            ClientHeartbeatLoopRetryPolicy {
                max_attempts: DEFAULT_ONE_TICK_RETRY_ATTEMPTS,
                retry_delay_micros: 1_000_000,
            }
        );
        assert_eq!(
            config.short_status,
            Some("one-tick-runtime-stats".to_string())
        );
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
    fn client_auth_heartbeat_poc_sends_heartbeat_and_receives_ack_once() {
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
        let config = ClientAuthHeartbeatPocStartupConfig {
            destination,
            response_timeout_ms: 1_000,
            request: request.clone(),
            heartbeat_short_status: Some("test-once".to_string()),
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
            let mut buffer = [0_u8; 1024];
            let (auth_len, source) = receiver
                .recv_from(&mut buffer)
                .expect("receiver should get auth request");
            let decoded_auth =
                decode_fixed_header(&buffer[..auth_len]).expect("auth fixed header should decode");
            let decoded_request =
                decode_auth_request_payload(decoded_auth.header, decoded_auth.payload)
                    .expect("auth request should decode");
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

            let (heartbeat_len, heartbeat_source) = receiver
                .recv_from(&mut buffer)
                .expect("receiver should get heartbeat");
            assert_eq!(heartbeat_source, source);
            let decoded_heartbeat = decode_fixed_header(&buffer[..heartbeat_len])
                .expect("heartbeat fixed header should decode");
            let heartbeat =
                decode_heartbeat_payload(decoded_heartbeat.header, decoded_heartbeat.payload)
                    .expect("heartbeat should decode");
            assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
            assert_eq!(heartbeat.run_id, RunId("run-1".to_string()));
            assert_eq!(heartbeat.short_status, Some("test-once".to_string()));

            let ack = HeartbeatAck {
                message_type: MessageType::HeartbeatAck,
                protocol_version: ProtocolVersion(2),
                client_id: heartbeat.client_id,
                run_id: heartbeat.run_id,
                echoed_sent_at: heartbeat.sent_at,
                server_received_at: TimestampMicros(3_000_100),
                server_sent_at: TimestampMicros(3_000_200),
            };
            let mut ack_bytes = Vec::new();
            ProtocolMessageEncoderBoundary
                .encode_message(
                    EncodeContext {
                        protocol_version: ack.protocol_version,
                    },
                    &ProtocolMessage::HeartbeatAck(ack.clone()),
                    &mut ack_bytes,
                )
                .expect("heartbeat ack should encode");
            receiver
                .send_to(&ack_bytes, source)
                .expect("heartbeat ack should send");
            (auth_len, heartbeat_len, ack)
        });

        let outcome = ClientAuthHeartbeatPocLauncher::default()
            .run_once(config)
            .expect("auth heartbeat should complete");

        let (auth_len, heartbeat_len, ack) = server.join().expect("server thread should finish");
        assert_eq!(outcome.auth_request_bytes_sent, auth_len);
        assert_eq!(outcome.heartbeat_bytes_sent, heartbeat_len);
        assert_eq!(outcome.auth_response, response);
        assert_eq!(outcome.heartbeat_ack, ack);
        assert_eq!(
            outcome.heartbeat_ack.echoed_sent_at,
            outcome.heartbeat.sent_at
        );
    }

    #[test]
    fn client_auth_heartbeat_stats_poc_sends_observation_in_client_stats_once() {
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
        let config = ClientAuthHeartbeatPocStartupConfig {
            destination,
            response_timeout_ms: 1_000,
            request: request.clone(),
            heartbeat_short_status: Some("test-once".to_string()),
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
            let mut buffer = [0_u8; 2048];
            let (auth_len, source) = receiver
                .recv_from(&mut buffer)
                .expect("receiver should get auth request");
            let decoded_auth =
                decode_fixed_header(&buffer[..auth_len]).expect("auth fixed header should decode");
            let decoded_request =
                decode_auth_request_payload(decoded_auth.header, decoded_auth.payload)
                    .expect("auth request should decode");
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

            let (heartbeat_len, heartbeat_source) = receiver
                .recv_from(&mut buffer)
                .expect("receiver should get heartbeat");
            assert_eq!(heartbeat_source, source);
            let decoded_heartbeat = decode_fixed_header(&buffer[..heartbeat_len])
                .expect("heartbeat fixed header should decode");
            let heartbeat =
                decode_heartbeat_payload(decoded_heartbeat.header, decoded_heartbeat.payload)
                    .expect("heartbeat should decode");

            let ack = HeartbeatAck {
                message_type: MessageType::HeartbeatAck,
                protocol_version: ProtocolVersion(2),
                client_id: heartbeat.client_id,
                run_id: heartbeat.run_id,
                echoed_sent_at: heartbeat.sent_at,
                server_received_at: TimestampMicros(3_000_100),
                server_sent_at: TimestampMicros(3_000_200),
            };
            let mut ack_bytes = Vec::new();
            ProtocolMessageEncoderBoundary
                .encode_message(
                    EncodeContext {
                        protocol_version: ack.protocol_version,
                    },
                    &ProtocolMessage::HeartbeatAck(ack.clone()),
                    &mut ack_bytes,
                )
                .expect("heartbeat ack should encode");
            receiver
                .send_to(&ack_bytes, source)
                .expect("heartbeat ack should send");

            let (stats_len, stats_source) = receiver
                .recv_from(&mut buffer)
                .expect("receiver should get client stats");
            assert_eq!(stats_source, source);
            let decoded_stats = decode_fixed_header(&buffer[..stats_len])
                .expect("client stats fixed header should decode");
            let stats = decode_client_stats_payload(decoded_stats.header, decoded_stats.payload)
                .expect("client stats should decode");
            (stats_len, ack, stats)
        });

        let outcome = ClientAuthHeartbeatStatsPocLauncher::default()
            .run_once(config)
            .expect("auth heartbeat stats should complete");

        let (stats_len, ack, stats) = server.join().expect("server thread should finish");
        assert_eq!(outcome.client_stats_bytes_sent, stats_len);
        assert_eq!(outcome.heartbeat.heartbeat_ack, ack);
        assert_eq!(stats.capture_fps, 0);
        assert_eq!(stats.dropped_frames, 0);
        assert_eq!(stats.bitrate_kbps, 0);
        assert_eq!(
            stats.heartbeat_observation,
            Some(outcome.heartbeat_ack_observation.clone())
        );
        assert_eq!(
            outcome.heartbeat_observation_carrier.observation,
            outcome.heartbeat_ack_observation
        );
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

    #[test]
    fn client_heartbeat_loop_policy_sends_when_no_previous_heartbeat_exists() {
        let input = ClientHeartbeatLoopPolicyInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            now: TimestampMicros(10_000),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000_000,
                ack_receive_timeout_micros: 200_000,
                ack_observation_return:
                    ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            state: ClientHeartbeatLoopStateSnapshot {
                sent_heartbeats: 0,
                received_acks: 0,
                missed_acks: 0,
                last_heartbeat_sent_at: None,
                stop_requested: false,
            },
        };

        let action = ClientHeartbeatLoopPolicyBoundary.evaluate(input);

        let ClientHeartbeatLoopPolicyAction::SendHeartbeat {
            send_at,
            ack_deadline_at,
            ack_observation_return,
            log,
        } = action
        else {
            panic!("first loop policy decision should send a heartbeat");
        };
        assert_eq!(send_at, TimestampMicros(10_000));
        assert_eq!(ack_deadline_at, TimestampMicros(210_000));
        assert_eq!(
            ack_observation_return,
            ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck
        );
        assert_eq!(log.reason, ClientHeartbeatLoopPolicyReason::HeartbeatDue);
        assert_eq!(log.client_id, ClientId("client-1".to_string()));
    }

    #[test]
    fn client_heartbeat_loop_policy_waits_until_next_cadence() {
        let input = ClientHeartbeatLoopPolicyInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            now: TimestampMicros(10_500),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 200,
                ack_observation_return:
                    ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            state: ClientHeartbeatLoopStateSnapshot {
                sent_heartbeats: 1,
                received_acks: 1,
                missed_acks: 0,
                last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                stop_requested: false,
            },
        };

        let action = ClientHeartbeatLoopPolicyBoundary.evaluate(input);

        let ClientHeartbeatLoopPolicyAction::Wait {
            next_heartbeat_due_at,
            log,
        } = action
        else {
            panic!("loop policy should wait until heartbeat cadence is due");
        };
        assert_eq!(next_heartbeat_due_at, TimestampMicros(11_000));
        assert_eq!(
            log.reason,
            ClientHeartbeatLoopPolicyReason::WaitingForCadence
        );
    }

    #[test]
    fn client_heartbeat_loop_policy_stops_after_max_heartbeats() {
        let input = ClientHeartbeatLoopPolicyInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            now: TimestampMicros(10_000),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 200,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::MaxHeartbeats {
                max_sent_heartbeats: 3,
            },
            state: ClientHeartbeatLoopStateSnapshot {
                sent_heartbeats: 3,
                received_acks: 3,
                missed_acks: 0,
                last_heartbeat_sent_at: Some(TimestampMicros(9_000)),
                stop_requested: false,
            },
        };

        let action = ClientHeartbeatLoopPolicyBoundary.evaluate(input);

        let ClientHeartbeatLoopPolicyAction::Stop { reason, log } = action else {
            panic!("loop policy should stop at max heartbeat count");
        };
        assert_eq!(
            reason,
            ClientHeartbeatLoopPolicyReason::MaxHeartbeatsReached
        );
        assert_eq!(
            log.reason,
            ClientHeartbeatLoopPolicyReason::MaxHeartbeatsReached
        );
    }

    #[test]
    fn client_heartbeat_loop_ownership_boundary_requires_accepted_auth_and_socket() {
        let input = ClientHeartbeatLoopOwnershipInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            auth_accepted: true,
            socket_bound: true,
        };

        let decision = ClientHeartbeatLoopOwnershipBoundary.evaluate(input);

        let ClientHeartbeatLoopOwnershipDecision::Ready(plan) = decision else {
            panic!("accepted auth and bound socket should be ready");
        };
        assert_eq!(plan.client_id, ClientId("client-1".to_string()));
        assert_eq!(plan.run_id, RunId("run-1".to_string()));
        assert_eq!(plan.protocol_version, ProtocolVersion(2));
        assert!(plan.owns_udp_socket);
        assert!(plan.owns_loop_state);
        assert!(plan.owns_ack_wait);
        assert!(plan.owns_stats_return);
    }

    #[test]
    fn client_heartbeat_loop_launcher_ownership_boundary_prepares_repeated_loop_handoff() {
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
        let auth_response = AuthResponse {
            message_type: MessageType::AuthResponse,
            protocol_version: ProtocolVersion(2),
            client_id: request.client_id.clone(),
            run_id: request.run_id.clone(),
            accepted: true,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time: Some(TimestampMicros(2_000)),
            expected_protocol_version: None,
        };

        let result = ClientHeartbeatLoopLauncherOwnershipBoundary::default().prepare(
            ClientHeartbeatLoopLauncherOwnershipInput {
                mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatWithStats,
                destination: "127.0.0.1:5000".parse().unwrap(),
                request,
                auth_response,
                socket_bound: true,
                cadence: ClientHeartbeatLoopCadenceInput {
                    heartbeat_interval_micros: 1_000_000,
                    ack_receive_timeout_micros: 5_000_000,
                    ack_observation_return:
                        ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck,
                },
                stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                max_ack_socket_wait_micros: 5_000_000,
                max_sleep_micros: 1_000_000,
                retry_policy: ClientHeartbeatLoopRetryPolicy {
                    max_attempts: 3,
                    retry_delay_micros: 1_000_000,
                },
                local_time_enabled: true,
                short_status: Some("one-tick-runtime-stats".to_string()),
            },
        );

        let ClientHeartbeatLoopOwnershipDecision::Ready(plan) = result.ownership else {
            panic!("accepted auth should prepare repeated loop ownership");
        };
        assert!(plan.owns_udp_socket);
        let handoff = result
            .repeated_loop_handoff
            .expect("ready launcher ownership should produce handoff");
        assert_eq!(
            handoff.mode,
            ClientHeartbeatOneTickRuntimeMode::HeartbeatWithStats
        );
        assert_eq!(handoff.destination, "127.0.0.1:5000".parse().unwrap());
        assert_eq!(handoff.client_id, ClientId("client-1".to_string()));
        assert_eq!(handoff.run_id, RunId("run-1".to_string()));
        assert_eq!(handoff.protocol_version, ProtocolVersion(2));
        assert_eq!(
            handoff.cadence.ack_observation_return,
            ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck
        );
        assert!(handoff.local_time_enabled);
    }

    #[test]
    fn client_heartbeat_loop_repeated_runtime_handoff_builds_one_tick_input() {
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000_000,
                ack_receive_timeout_micros: 500_000,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500_000,
            max_sleep_micros: 1_000_000,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };

        let input = handoff.build_one_tick_input(
            TimestampMicros(10_000),
            ClientHeartbeatLoopStateSnapshot {
                sent_heartbeats: 1,
                received_acks: 1,
                missed_acks: 0,
                last_heartbeat_sent_at: Some(TimestampMicros(9_000)),
                stop_requested: false,
            },
            2,
        );

        assert_eq!(input.destination, "127.0.0.1:5000".parse().unwrap());
        assert_eq!(
            input.body.ownership.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(input.body.policy.now, TimestampMicros(10_000));
        assert_eq!(input.body.policy.state.sent_heartbeats, 1);
        assert_eq!(input.local_time, Some(TimestampMicros(10_000)));
        assert_eq!(input.short_status, Some("one-tick-runtime".to_string()));
        assert_eq!(input.retry_attempts_used, 2);
    }

    #[test]
    fn client_heartbeat_loop_repeated_runtime_body_delegates_wait_path() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };

        let result = ClientHeartbeatLoopRepeatedRuntimeBodyBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                handoff,
                now: TimestampMicros(10_500),
                stop_requested: false,
                retry_attempts_used: 1,
            },
        );

        assert_eq!(
            result.one_tick_input.controller_now,
            TimestampMicros(10_500)
        );
        assert_eq!(result.one_tick_input.retry_attempts_used, 1);
        assert_eq!(result.one_tick_input.body.policy.state.sent_heartbeats, 0);
        assert_eq!(
            result.runtime.controller.action,
            ClientHeartbeatLoopControllerAction::Sleep
        );
        assert_eq!(
            result.shutdown,
            ClientHeartbeatLoopShutdownDecision::Continue
        );
    }

    #[test]
    fn client_heartbeat_loop_repeated_runtime_body_returns_stop_without_executing_shutdown() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState::default();
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };

        let result = ClientHeartbeatLoopRepeatedRuntimeBodyBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                handoff,
                now: TimestampMicros(10_500),
                stop_requested: true,
                retry_attempts_used: 0,
            },
        );

        assert_eq!(
            result.runtime.controller.action,
            ClientHeartbeatLoopControllerAction::Stop
        );
        assert_eq!(
            result.shutdown,
            ClientHeartbeatLoopShutdownDecision::Stop {
                reason: ClientHeartbeatLoopPolicyReason::StopRequested
            }
        );
        assert_eq!(result.runtime.heartbeat_send, None);
    }

    #[test]
    fn client_heartbeat_loop_outer_controller_continues_on_non_stop_body_result() {
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let body = ClientHeartbeatLoopRepeatedRuntimeBodyResult {
            handoff,
            one_tick_input: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                destination: "127.0.0.1:5000".parse().unwrap(),
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                cadence: ClientHeartbeatLoopCadenceInput {
                    heartbeat_interval_micros: 1_000,
                    ack_receive_timeout_micros: 500,
                    ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
                },
                stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                max_ack_socket_wait_micros: 500,
                max_sleep_micros: 250,
                retry_policy: ClientHeartbeatLoopRetryPolicy {
                    max_attempts: 3,
                    retry_delay_micros: 1_000,
                },
                local_time_enabled: true,
                short_status: Some("one-tick-runtime".to_string()),
            }
            .build_one_tick_input(
                TimestampMicros(10_500),
                ClientHeartbeatLoopStateSnapshot {
                    sent_heartbeats: 0,
                    received_acks: 0,
                    missed_acks: 0,
                    last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                    stop_requested: false,
                },
                0,
            ),
            runtime: ClientHeartbeatLoopOneTickRuntimeResult {
                controller: ClientHeartbeatLoopControllerResult {
                    action: ClientHeartbeatLoopControllerAction::Sleep,
                    plan: ClientHeartbeatLoopControllerPlan::Sleep {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 250,
                            wake_at: TimestampMicros(11_000),
                        },
                        log: ClientHeartbeatLoopLogHandoff {
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            observed_at: TimestampMicros(10_500),
                            reason: ClientHeartbeatLoopPolicyReason::WaitingForCadence,
                            heartbeat_interval_micros: 1_000,
                            ack_receive_timeout_micros: 500,
                            sent_heartbeats: 0,
                            received_acks: 0,
                            missed_acks: 0,
                        },
                        iteration_result: ClientHeartbeatLoopIterationRuntimeResult::Waited {
                            next_heartbeat_due_at: TimestampMicros(11_000),
                        },
                    },
                    log: None,
                    shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                    iteration_result: Some(ClientHeartbeatLoopIterationRuntimeResult::Waited {
                        next_heartbeat_due_at: TimestampMicros(11_000),
                    }),
                },
                heartbeat_send: None,
                ack_return: None,
                stats_return_send: None,
                retry: None,
                failure: None,
                counters_updates: Vec::new(),
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
            shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
        };

        let result = ClientHeartbeatLoopOuterControllerBoundary.observe(&body);

        assert_eq!(
            result.action,
            ClientHeartbeatLoopOuterControllerAction::ContinueLoop
        );
        assert_eq!(
            result.shutdown,
            ClientHeartbeatLoopShutdownDecision::Continue
        );
    }

    #[test]
    fn client_heartbeat_loop_repeated_runtime_loop_step_marks_shutdown_apply_on_stop() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState::default();
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };

        let result = ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                handoff,
                now: TimestampMicros(10_500),
                stop_requested: true,
                retry_attempts_used: 0,
            },
        );

        assert_eq!(
            result.controller.action,
            ClientHeartbeatLoopOuterControllerAction::StopLoop
        );
        assert_eq!(
            result.shutdown_apply,
            ClientHeartbeatLoopShutdownApplyResult::StopLoop {
                reason: ClientHeartbeatLoopPolicyReason::StopRequested,
                cleanup_required: true
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_lifecycle_continues_when_step_and_caller_allow() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let step = ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                handoff,
                now: TimestampMicros(10_500),
                stop_requested: false,
                retry_attempts_used: 0,
            },
        );

        let result =
            ClientHeartbeatLoopLifecycleBoundary.plan_next(ClientHeartbeatLoopLifecycleInput {
                continue_requested: true,
                step,
            });

        assert!(result.continue_loop);
        assert_eq!(result.stop_reason, None);
        assert!(!result.cleanup_required);
    }

    #[test]
    fn client_heartbeat_loop_lifecycle_stops_when_caller_requests_stop() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let step = ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                handoff,
                now: TimestampMicros(10_500),
                stop_requested: false,
                retry_attempts_used: 0,
            },
        );

        let result =
            ClientHeartbeatLoopLifecycleBoundary.plan_next(ClientHeartbeatLoopLifecycleInput {
                continue_requested: false,
                step,
            });

        assert!(!result.continue_loop);
        assert_eq!(
            result.stop_reason,
            Some(ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop)
        );
        assert!(result.cleanup_required);
    }

    #[test]
    fn client_heartbeat_loop_sequencing_prefers_retry_sleep_when_retry_is_present() {
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let policy_log = ClientHeartbeatLoopLogHandoff {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            observed_at: TimestampMicros(10_500),
            reason: ClientHeartbeatLoopPolicyReason::WaitingForCadence,
            heartbeat_interval_micros: 1_000,
            ack_receive_timeout_micros: 500,
            sent_heartbeats: 1,
            received_acks: 0,
            missed_acks: 0,
        };
        let controller_plan = ClientHeartbeatLoopControllerPlan::Sleep {
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                sleep_micros: 250,
                wake_at: TimestampMicros(11_000),
            },
            log: policy_log.clone(),
            iteration_result: ClientHeartbeatLoopIterationRuntimeResult::Waited {
                next_heartbeat_due_at: TimestampMicros(11_000),
            },
        };
        let retry = ClientHeartbeatLoopRetryApplyResult {
            failure_result: ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                failed_at: TimestampMicros(10_600),
            },
            retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                next_attempt: 2,
                retry_at: TimestampMicros(11_100),
            },
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                sleep_micros: 250,
                wake_at: TimestampMicros(11_100),
            },
        };
        let lifecycle = ClientHeartbeatLoopLifecycleResult {
            step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
                body: ClientHeartbeatLoopRepeatedRuntimeBodyResult {
                    handoff: handoff.clone(),
                    one_tick_input: handoff.build_one_tick_input(
                        TimestampMicros(10_500),
                        ClientHeartbeatLoopStateSnapshot {
                            sent_heartbeats: 1,
                            received_acks: 0,
                            missed_acks: 0,
                            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                            stop_requested: false,
                        },
                        1,
                    ),
                    runtime: ClientHeartbeatLoopOneTickRuntimeResult {
                        controller: ClientHeartbeatLoopControllerResult {
                            action: ClientHeartbeatLoopControllerAction::Sleep,
                            plan: controller_plan.clone(),
                            log: Some(ClientHeartbeatLoopControllerLogHandoff {
                                action: ClientHeartbeatLoopControllerAction::Sleep,
                                policy_log,
                                iteration_result: Some(
                                    ClientHeartbeatLoopIterationRuntimeResult::Waited {
                                        next_heartbeat_due_at: TimestampMicros(11_000),
                                    },
                                ),
                            }),
                            shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                            iteration_result: Some(
                                ClientHeartbeatLoopIterationRuntimeResult::Waited {
                                    next_heartbeat_due_at: TimestampMicros(11_000),
                                },
                            ),
                        },
                        heartbeat_send: None,
                        ack_return: None,
                        stats_return_send: None,
                        retry: Some(retry.clone()),
                        failure: Some(ClientHeartbeatLoopOneTickRuntimeFailure::AckReceive(
                            ClientHeartbeatLoopAckObservationReturnError::AckReceive(
                                ClientResponseReceiveError::Receive(io::ErrorKind::TimedOut),
                            ),
                        )),
                        counters_updates: Vec::new(),
                        final_counters: ClientHeartbeatLoopCountersState::default(),
                    },
                    shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                },
                controller: ClientHeartbeatLoopOuterControllerResult {
                    action: ClientHeartbeatLoopOuterControllerAction::ContinueLoop,
                    shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                },
                shutdown_apply: ClientHeartbeatLoopShutdownApplyResult::ContinueLoop,
            },
            continue_loop: true,
            stop_reason: None,
            cleanup_required: false,
        };

        let result = ClientHeartbeatLoopSequencingBoundary.plan_next(lifecycle);

        assert_eq!(
            result.timer_wait,
            ClientHeartbeatLoopTimerWaitDecision::Wait { sleep: retry.sleep }
        );
        assert_eq!(
            result.retry_execution,
            ClientHeartbeatLoopRetryExecutionResult::RetryScheduled { retry }
        );
        assert_eq!(
            result.cleanup,
            ClientHeartbeatLoopCleanupSequencingResult::NoCleanup
        );
    }

    #[test]
    fn client_heartbeat_loop_sequencing_starts_cleanup_when_lifecycle_stops() {
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let lifecycle = ClientHeartbeatLoopLifecycleResult {
            step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
                body: ClientHeartbeatLoopRepeatedRuntimeBodyResult {
                    handoff: handoff.clone(),
                    one_tick_input: handoff.build_one_tick_input(
                        TimestampMicros(10_500),
                        ClientHeartbeatLoopStateSnapshot {
                            sent_heartbeats: 1,
                            received_acks: 1,
                            missed_acks: 0,
                            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                            stop_requested: false,
                        },
                        0,
                    ),
                    runtime: ClientHeartbeatLoopOneTickRuntimeResult {
                        controller: ClientHeartbeatLoopControllerResult {
                            action: ClientHeartbeatLoopControllerAction::SendHeartbeat,
                            plan: ClientHeartbeatLoopControllerPlan::SendHeartbeat {
                                handoff: ClientHeartbeatLoopBodySendHandoff {
                                    client_id: handoff.client_id.clone(),
                                    run_id: handoff.run_id.clone(),
                                    protocol_version: handoff.protocol_version,
                                    send_at: TimestampMicros(10_500),
                                    ack_deadline_at: TimestampMicros(11_000),
                                    ack_wait: ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                                        receive_timeout_micros: 500,
                                        deadline_at: TimestampMicros(11_000),
                                    },
                                    ack_observation_return:
                                        ClientHeartbeatAckObservationReturnMode::Disabled,
                                },
                                log: ClientHeartbeatLoopLogHandoff {
                                    client_id: handoff.client_id.clone(),
                                    run_id: handoff.run_id.clone(),
                                    observed_at: TimestampMicros(10_500),
                                    reason: ClientHeartbeatLoopPolicyReason::HeartbeatDue,
                                    heartbeat_interval_micros: 1_000,
                                    ack_receive_timeout_micros: 500,
                                    sent_heartbeats: 1,
                                    received_acks: 1,
                                    missed_acks: 0,
                                },
                            },
                            log: None,
                            shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                            iteration_result: None,
                        },
                        heartbeat_send: None,
                        ack_return: None,
                        stats_return_send: None,
                        retry: None,
                        failure: None,
                        counters_updates: Vec::new(),
                        final_counters: ClientHeartbeatLoopCountersState::default(),
                    },
                    shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                },
                controller: ClientHeartbeatLoopOuterControllerResult {
                    action: ClientHeartbeatLoopOuterControllerAction::StopLoop,
                    shutdown: ClientHeartbeatLoopShutdownDecision::Stop {
                        reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                    },
                },
                shutdown_apply: ClientHeartbeatLoopShutdownApplyResult::StopLoop {
                    reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                    cleanup_required: true,
                },
            },
            continue_loop: false,
            stop_reason: Some(
                ClientHeartbeatLoopLifecycleStopReason::PolicyRequestedStop {
                    reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                },
            ),
            cleanup_required: true,
        };

        let result = ClientHeartbeatLoopSequencingBoundary.plan_next(lifecycle);

        assert_eq!(
            result.timer_wait,
            ClientHeartbeatLoopTimerWaitDecision::NoWait
        );
        assert_eq!(
            result.retry_execution,
            ClientHeartbeatLoopRetryExecutionResult::NoRetryScheduled
        );
        assert_eq!(
            result.cleanup,
            ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::PolicyRequestedStop {
                    reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_step_ordering_prefers_retry_over_wait() {
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let retry = ClientHeartbeatLoopRetryApplyResult {
            failure_result: ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                failed_at: TimestampMicros(10_600),
            },
            retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                next_attempt: 2,
                retry_at: TimestampMicros(11_100),
            },
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                sleep_micros: 250,
                wake_at: TimestampMicros(11_100),
            },
        };
        let sequencing = ClientHeartbeatLoopSequencingResult {
            lifecycle: ClientHeartbeatLoopLifecycleResult {
                step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyResult {
                        handoff: handoff.clone(),
                        one_tick_input: handoff.build_one_tick_input(
                            TimestampMicros(10_500),
                            ClientHeartbeatLoopStateSnapshot {
                                sent_heartbeats: 1,
                                received_acks: 0,
                                missed_acks: 0,
                                last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                                stop_requested: false,
                            },
                            1,
                        ),
                        runtime: ClientHeartbeatLoopOneTickRuntimeResult {
                            controller: ClientHeartbeatLoopControllerResult {
                                action: ClientHeartbeatLoopControllerAction::Sleep,
                                plan: ClientHeartbeatLoopControllerPlan::Sleep {
                                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                        sleep_micros: 250,
                                        wake_at: TimestampMicros(11_000),
                                    },
                                    log: ClientHeartbeatLoopLogHandoff {
                                        client_id: handoff.client_id.clone(),
                                        run_id: handoff.run_id.clone(),
                                        observed_at: TimestampMicros(10_500),
                                        reason: ClientHeartbeatLoopPolicyReason::WaitingForCadence,
                                        heartbeat_interval_micros: 1_000,
                                        ack_receive_timeout_micros: 500,
                                        sent_heartbeats: 1,
                                        received_acks: 0,
                                        missed_acks: 0,
                                    },
                                    iteration_result:
                                        ClientHeartbeatLoopIterationRuntimeResult::Waited {
                                            next_heartbeat_due_at: TimestampMicros(11_000),
                                        },
                                },
                                log: None,
                                shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                                iteration_result: None,
                            },
                            heartbeat_send: None,
                            ack_return: None,
                            stats_return_send: None,
                            retry: Some(retry.clone()),
                            failure: None,
                            counters_updates: Vec::new(),
                            final_counters: ClientHeartbeatLoopCountersState::default(),
                        },
                        shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                    },
                    controller: ClientHeartbeatLoopOuterControllerResult {
                        action: ClientHeartbeatLoopOuterControllerAction::ContinueLoop,
                        shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                    },
                    shutdown_apply: ClientHeartbeatLoopShutdownApplyResult::ContinueLoop,
                },
                continue_loop: true,
                stop_reason: None,
                cleanup_required: false,
            },
            timer_wait: ClientHeartbeatLoopTimerWaitDecision::Wait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                    sleep_micros: 250,
                    wake_at: TimestampMicros(11_100),
                },
            },
            retry_execution: ClientHeartbeatLoopRetryExecutionResult::RetryScheduled {
                retry: retry.clone(),
            },
            cleanup: ClientHeartbeatLoopCleanupSequencingResult::NoCleanup,
        };

        let result = ClientHeartbeatLoopStepOrderingBoundary.plan_next(sequencing);

        let ClientHeartbeatLoopStepOrderingResult::Continue { handoff } = result else {
            panic!("ordering should continue when cleanup is not required");
        };
        assert_eq!(
            handoff.ordering,
            ClientHeartbeatLoopStepOrdering::RetryThenContinue { retry }
        );
    }

    #[test]
    fn client_heartbeat_loop_step_ordering_waits_when_no_retry_is_scheduled() {
        let sequencing = ClientHeartbeatLoopSequencingResult {
            lifecycle: ClientHeartbeatLoopLifecycleResult {
                step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyResult {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        one_tick_input: ClientHeartbeatLoopOneTickRuntimeInput {
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            body: ClientHeartbeatLoopBodyInput {
                                ownership: ClientHeartbeatLoopOwnershipInput {
                                    client_id: ClientId("client-1".to_string()),
                                    run_id: RunId("run-1".to_string()),
                                    protocol_version: ProtocolVersion(2),
                                    auth_accepted: true,
                                    socket_bound: true,
                                },
                                policy: ClientHeartbeatLoopPolicyInput {
                                    client_id: ClientId("client-1".to_string()),
                                    run_id: RunId("run-1".to_string()),
                                    now: TimestampMicros(10_500),
                                    cadence: ClientHeartbeatLoopCadenceInput {
                                        heartbeat_interval_micros: 1_000,
                                        ack_receive_timeout_micros: 500,
                                        ack_observation_return:
                                            ClientHeartbeatAckObservationReturnMode::Disabled,
                                    },
                                    stop_condition:
                                        ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                    state: ClientHeartbeatLoopStateSnapshot {
                                        sent_heartbeats: 1,
                                        received_acks: 1,
                                        missed_acks: 0,
                                        last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                                        stop_requested: false,
                                    },
                                },
                                max_ack_socket_wait_micros: 500,
                            },
                            local_time: Some(TimestampMicros(10_500)),
                            short_status: Some("one-tick-runtime".to_string()),
                            controller_now: TimestampMicros(10_500),
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            retry_attempts_used: 0,
                        },
                        runtime: ClientHeartbeatLoopOneTickRuntimeResult {
                            controller: ClientHeartbeatLoopControllerResult {
                                action: ClientHeartbeatLoopControllerAction::Sleep,
                                plan: ClientHeartbeatLoopControllerPlan::Sleep {
                                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                        sleep_micros: 250,
                                        wake_at: TimestampMicros(11_000),
                                    },
                                    log: ClientHeartbeatLoopLogHandoff {
                                        client_id: ClientId("client-1".to_string()),
                                        run_id: RunId("run-1".to_string()),
                                        observed_at: TimestampMicros(10_500),
                                        reason: ClientHeartbeatLoopPolicyReason::WaitingForCadence,
                                        heartbeat_interval_micros: 1_000,
                                        ack_receive_timeout_micros: 500,
                                        sent_heartbeats: 1,
                                        received_acks: 1,
                                        missed_acks: 0,
                                    },
                                    iteration_result:
                                        ClientHeartbeatLoopIterationRuntimeResult::Waited {
                                            next_heartbeat_due_at: TimestampMicros(11_000),
                                        },
                                },
                                log: None,
                                shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                                iteration_result: Some(
                                    ClientHeartbeatLoopIterationRuntimeResult::Waited {
                                        next_heartbeat_due_at: TimestampMicros(11_000),
                                    },
                                ),
                            },
                            heartbeat_send: None,
                            ack_return: None,
                            stats_return_send: None,
                            retry: None,
                            failure: None,
                            counters_updates: Vec::new(),
                            final_counters: ClientHeartbeatLoopCountersState::default(),
                        },
                        shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                    },
                    controller: ClientHeartbeatLoopOuterControllerResult {
                        action: ClientHeartbeatLoopOuterControllerAction::ContinueLoop,
                        shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                    },
                    shutdown_apply: ClientHeartbeatLoopShutdownApplyResult::ContinueLoop,
                },
                continue_loop: true,
                stop_reason: None,
                cleanup_required: false,
            },
            timer_wait: ClientHeartbeatLoopTimerWaitDecision::Wait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 250,
                    wake_at: TimestampMicros(11_000),
                },
            },
            retry_execution: ClientHeartbeatLoopRetryExecutionResult::NoRetryScheduled,
            cleanup: ClientHeartbeatLoopCleanupSequencingResult::NoCleanup,
        };

        let result = ClientHeartbeatLoopStepOrderingBoundary.plan_next(sequencing);

        let ClientHeartbeatLoopStepOrderingResult::Continue { handoff } = result else {
            panic!("ordering should continue for wait path");
        };
        assert_eq!(
            handoff.ordering,
            ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 250,
                    wake_at: TimestampMicros(11_000),
                }
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_step_ordering_stops_for_cleanup() {
        let sequencing = ClientHeartbeatLoopSequencingResult {
            lifecycle: ClientHeartbeatLoopLifecycleResult {
                step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyResult {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        one_tick_input: ClientHeartbeatLoopOneTickRuntimeInput {
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            body: ClientHeartbeatLoopBodyInput {
                                ownership: ClientHeartbeatLoopOwnershipInput {
                                    client_id: ClientId("client-1".to_string()),
                                    run_id: RunId("run-1".to_string()),
                                    protocol_version: ProtocolVersion(2),
                                    auth_accepted: true,
                                    socket_bound: true,
                                },
                                policy: ClientHeartbeatLoopPolicyInput {
                                    client_id: ClientId("client-1".to_string()),
                                    run_id: RunId("run-1".to_string()),
                                    now: TimestampMicros(10_500),
                                    cadence: ClientHeartbeatLoopCadenceInput {
                                        heartbeat_interval_micros: 1_000,
                                        ack_receive_timeout_micros: 500,
                                        ack_observation_return:
                                            ClientHeartbeatAckObservationReturnMode::Disabled,
                                    },
                                    stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                    state: ClientHeartbeatLoopStateSnapshot {
                                        sent_heartbeats: 1,
                                        received_acks: 1,
                                        missed_acks: 3,
                                        last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                                        stop_requested: false,
                                    },
                                },
                                max_ack_socket_wait_micros: 500,
                            },
                            local_time: Some(TimestampMicros(10_500)),
                            short_status: Some("one-tick-runtime".to_string()),
                            controller_now: TimestampMicros(10_500),
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            retry_attempts_used: 0,
                        },
                        runtime: ClientHeartbeatLoopOneTickRuntimeResult {
                            controller: ClientHeartbeatLoopControllerResult {
                                action: ClientHeartbeatLoopControllerAction::Stop,
                                plan: ClientHeartbeatLoopControllerPlan::Stop {
                                    reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                                    log: ClientHeartbeatLoopLogHandoff {
                                        client_id: ClientId("client-1".to_string()),
                                        run_id: RunId("run-1".to_string()),
                                        observed_at: TimestampMicros(10_500),
                                        reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                                        heartbeat_interval_micros: 1_000,
                                        ack_receive_timeout_micros: 500,
                                        sent_heartbeats: 1,
                                        received_acks: 1,
                                        missed_acks: 3,
                                    },
                                    iteration_result: ClientHeartbeatLoopIterationRuntimeResult::Stopped {
                                        reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                                    },
                                },
                                log: None,
                                shutdown: ClientHeartbeatLoopShutdownDecision::Stop {
                                    reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                                },
                                iteration_result: Some(
                                    ClientHeartbeatLoopIterationRuntimeResult::Stopped {
                                        reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                                    },
                                ),
                            },
                            heartbeat_send: None,
                            ack_return: None,
                            stats_return_send: None,
                            retry: None,
                            failure: None,
                            counters_updates: Vec::new(),
                            final_counters: ClientHeartbeatLoopCountersState::default(),
                        },
                        shutdown: ClientHeartbeatLoopShutdownDecision::Stop {
                            reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                        },
                    },
                    controller: ClientHeartbeatLoopOuterControllerResult {
                        action: ClientHeartbeatLoopOuterControllerAction::StopLoop,
                        shutdown: ClientHeartbeatLoopShutdownDecision::Stop {
                            reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                        },
                    },
                    shutdown_apply: ClientHeartbeatLoopShutdownApplyResult::StopLoop {
                        reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                        cleanup_required: true,
                    },
                },
                continue_loop: false,
                stop_reason: Some(ClientHeartbeatLoopLifecycleStopReason::PolicyRequestedStop {
                    reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                }),
                cleanup_required: true,
            },
            timer_wait: ClientHeartbeatLoopTimerWaitDecision::NoWait,
            retry_execution: ClientHeartbeatLoopRetryExecutionResult::NoRetryScheduled,
            cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::PolicyRequestedStop {
                    reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                },
            },
        };

        let result = ClientHeartbeatLoopStepOrderingBoundary.plan_next(sequencing);

        assert_eq!(
            result,
            ClientHeartbeatLoopStepOrderingResult::Stop {
                result: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::PolicyRequestedStop {
                        reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                    },
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::PolicyRequestedStop {
                            reason: ClientHeartbeatLoopPolicyReason::MaxMissedAcksReached,
                        },
                    },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_completed_step_runtime_returns_wait_ordering() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };

        let result = ClientHeartbeatLoopCompletedStepRuntimeBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopCompletedStepRuntimeInput {
                continue_requested: true,
                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                    handoff,
                    now: TimestampMicros(10_500),
                    stop_requested: false,
                    retry_attempts_used: 0,
                },
            },
        );

        assert!(result.lifecycle.continue_loop);
        assert_eq!(
            result.ordering,
            ClientHeartbeatLoopStepOrderingResult::Continue {
                handoff: ClientHeartbeatLoopCompletedBodySequencingHandoff {
                    sequencing: result.sequencing.clone(),
                    ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 250,
                            wake_at: TimestampMicros(11_000),
                        },
                    },
                },
            }
        );
        assert_eq!(result.final_counters, counters);
    }

    #[test]
    fn client_heartbeat_loop_completed_step_runtime_returns_stop_when_caller_stops() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };

        let result = ClientHeartbeatLoopCompletedStepRuntimeBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopCompletedStepRuntimeInput {
                continue_requested: false,
                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                    handoff,
                    now: TimestampMicros(10_500),
                    stop_requested: false,
                    retry_attempts_used: 0,
                },
            },
        );

        assert!(!result.lifecycle.continue_loop);
        assert_eq!(
            result.lifecycle.stop_reason,
            Some(ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop)
        );
        assert_eq!(
            result.ordering,
            ClientHeartbeatLoopStepOrderingResult::Stop {
                result: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
            }
        );
        assert_eq!(result.final_counters, counters);
    }

    #[test]
    fn client_heartbeat_loop_while_loop_ownership_returns_continue_contract() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let runtime = ClientHeartbeatLoopCompletedStepRuntimeBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopCompletedStepRuntimeInput {
                continue_requested: true,
                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                    handoff,
                    now: TimestampMicros(10_500),
                    stop_requested: false,
                    retry_attempts_used: 0,
                },
            },
        );

        let result = ClientHeartbeatLoopWhileLoopOwnershipBoundary.handoff(runtime.clone());

        assert_eq!(
            result,
            ClientHeartbeatLoopCallerContractResult::Continue {
                ordering: ClientHeartbeatLoopCompletedBodySequencingHandoff {
                    sequencing: runtime.sequencing.clone(),
                    ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 250,
                            wake_at: TimestampMicros(11_000),
                        },
                    },
                },
                final_counters: counters,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_while_loop_ownership_returns_stop_handoff() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let runtime = ClientHeartbeatLoopCompletedStepRuntimeBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopCompletedStepRuntimeInput {
                continue_requested: false,
                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                    handoff,
                    now: TimestampMicros(10_500),
                    stop_requested: false,
                    retry_attempts_used: 0,
                },
            },
        );

        let result = ClientHeartbeatLoopWhileLoopOwnershipBoundary.handoff(runtime);

        assert_eq!(
            result,
            ClientHeartbeatLoopCallerContractResult::Stop {
                handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                    stop: ClientHeartbeatLoopCompletedBodyStopResult {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    },
                    final_counters: counters,
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_skeleton_builds_next_iteration_carry_from_wait_contract() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let repeated_handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let runtime = ClientHeartbeatLoopCompletedStepRuntimeBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopCompletedStepRuntimeInput {
                continue_requested: true,
                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                    handoff: repeated_handoff.clone(),
                    now: TimestampMicros(10_500),
                    stop_requested: false,
                    retry_attempts_used: 0,
                },
            },
        );
        let contract = ClientHeartbeatLoopWhileLoopOwnershipBoundary.handoff(runtime);

        let result = ClientHeartbeatLoopSkeletonBoundary.plan_next(
            contract,
            ClientHeartbeatLoopStopRefreshInput {
                now: TimestampMicros(11_000),
                stop_requested: true,
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopSkeletonResult::Continue {
                carry: ClientHeartbeatLoopIterationCarryState {
                    ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 250,
                            wake_at: TimestampMicros(11_000),
                        },
                    },
                    final_counters: counters,
                    next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                        continue_requested: false,
                        body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                            handoff: repeated_handoff,
                            now: TimestampMicros(11_000),
                            stop_requested: true,
                            retry_attempts_used: 0,
                        },
                    },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_skeleton_carries_retry_attempt_forward() {
        let retry = ClientHeartbeatLoopRetryApplyResult {
            failure_result: ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                failed_at: TimestampMicros(10_600),
            },
            retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                next_attempt: 2,
                retry_at: TimestampMicros(11_100),
            },
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                sleep_micros: 250,
                wake_at: TimestampMicros(11_100),
            },
        };
        let repeated_handoff = ClientHeartbeatLoopRepeatedRuntimeHandoff {
            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
            destination: "127.0.0.1:5000".parse().unwrap(),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            cadence: ClientHeartbeatLoopCadenceInput {
                heartbeat_interval_micros: 1_000,
                ack_receive_timeout_micros: 500,
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
            max_ack_socket_wait_micros: 500,
            max_sleep_micros: 250,
            retry_policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 1_000,
            },
            local_time_enabled: true,
            short_status: Some("one-tick-runtime".to_string()),
        };
        let contract = ClientHeartbeatLoopCallerContractResult::Continue {
            ordering: ClientHeartbeatLoopCompletedBodySequencingHandoff {
                sequencing: ClientHeartbeatLoopSequencingResult {
                    lifecycle: ClientHeartbeatLoopLifecycleResult {
                        step: ClientHeartbeatLoopRepeatedRuntimeLoopStepResult {
                            body: ClientHeartbeatLoopRepeatedRuntimeBodyResult {
                                handoff: repeated_handoff.clone(),
                                one_tick_input: repeated_handoff.build_one_tick_input(
                                    TimestampMicros(10_500),
                                    ClientHeartbeatLoopStateSnapshot {
                                        sent_heartbeats: 1,
                                        received_acks: 0,
                                        missed_acks: 0,
                                        last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                                        stop_requested: false,
                                    },
                                    1,
                                ),
                                runtime: ClientHeartbeatLoopOneTickRuntimeResult {
                                    controller: ClientHeartbeatLoopControllerResult {
                                        action: ClientHeartbeatLoopControllerAction::SendHeartbeat,
                                        plan: ClientHeartbeatLoopControllerPlan::SendHeartbeat {
                                            handoff: ClientHeartbeatLoopBodySendHandoff {
                                                client_id: repeated_handoff.client_id.clone(),
                                                run_id: repeated_handoff.run_id.clone(),
                                                protocol_version: repeated_handoff.protocol_version,
                                                send_at: TimestampMicros(10_500),
                                                ack_deadline_at: TimestampMicros(11_000),
                                                ack_wait: ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                                                    receive_timeout_micros: 500,
                                                    deadline_at: TimestampMicros(11_000),
                                                },
                                                ack_observation_return:
                                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                                            },
                                            log: ClientHeartbeatLoopLogHandoff {
                                                client_id: repeated_handoff.client_id.clone(),
                                                run_id: repeated_handoff.run_id.clone(),
                                                observed_at: TimestampMicros(10_500),
                                                reason: ClientHeartbeatLoopPolicyReason::HeartbeatDue,
                                                heartbeat_interval_micros: 1_000,
                                                ack_receive_timeout_micros: 500,
                                                sent_heartbeats: 1,
                                                received_acks: 0,
                                                missed_acks: 0,
                                            },
                                        },
                                        log: None,
                                        shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                                        iteration_result: None,
                                    },
                                    heartbeat_send: None,
                                    ack_return: None,
                                    stats_return_send: None,
                                    retry: Some(retry.clone()),
                                    failure: None,
                                    counters_updates: Vec::new(),
                                    final_counters: ClientHeartbeatLoopCountersState::default(),
                                },
                                shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                            },
                            controller: ClientHeartbeatLoopOuterControllerResult {
                                action: ClientHeartbeatLoopOuterControllerAction::ContinueLoop,
                                shutdown: ClientHeartbeatLoopShutdownDecision::Continue,
                            },
                            shutdown_apply: ClientHeartbeatLoopShutdownApplyResult::ContinueLoop,
                        },
                        continue_loop: true,
                        stop_reason: None,
                        cleanup_required: false,
                    },
                    timer_wait: ClientHeartbeatLoopTimerWaitDecision::Wait {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                            sleep_micros: 250,
                            wake_at: TimestampMicros(11_100),
                        },
                    },
                    retry_execution: ClientHeartbeatLoopRetryExecutionResult::RetryScheduled {
                        retry: retry.clone(),
                    },
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::NoCleanup,
                },
                ordering: ClientHeartbeatLoopStepOrdering::RetryThenContinue {
                    retry: retry.clone(),
                },
            },
            final_counters: ClientHeartbeatLoopCountersState::default(),
        };

        let result = ClientHeartbeatLoopSkeletonBoundary.plan_next(
            contract,
            ClientHeartbeatLoopStopRefreshInput {
                now: TimestampMicros(11_100),
                stop_requested: false,
            },
        );

        let ClientHeartbeatLoopSkeletonResult::Continue { carry } = result else {
            panic!("retry contract should stay in continue state");
        };
        assert_eq!(
            carry.ordering,
            ClientHeartbeatLoopStepOrdering::RetryThenContinue { retry }
        );
        assert_eq!(carry.next_runtime_input.continue_requested, true);
        assert_eq!(carry.next_runtime_input.body.stop_requested, false);
        assert_eq!(carry.next_runtime_input.body.now, TimestampMicros(11_100));
        assert_eq!(carry.next_runtime_input.body.retry_attempts_used, 2);
    }

    #[test]
    fn client_heartbeat_loop_apply_order_prefers_timer_for_wait_carry() {
        let carry = ClientHeartbeatLoopIterationCarryState {
            ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 250,
                    wake_at: TimestampMicros(11_000),
                },
            },
            final_counters: ClientHeartbeatLoopCountersState::default(),
            next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                continue_requested: true,
                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                    handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                        mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                        destination: "127.0.0.1:5000".parse().unwrap(),
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        protocol_version: ProtocolVersion(2),
                        cadence: ClientHeartbeatLoopCadenceInput {
                            heartbeat_interval_micros: 1_000,
                            ack_receive_timeout_micros: 500,
                            ack_observation_return:
                                ClientHeartbeatAckObservationReturnMode::Disabled,
                        },
                        stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                        max_ack_socket_wait_micros: 500,
                        max_sleep_micros: 250,
                        retry_policy: ClientHeartbeatLoopRetryPolicy {
                            max_attempts: 3,
                            retry_delay_micros: 1_000,
                        },
                        local_time_enabled: true,
                        short_status: Some("one-tick-runtime".to_string()),
                    },
                    now: TimestampMicros(11_000),
                    stop_requested: false,
                    retry_attempts_used: 0,
                },
            },
        };

        let result = ClientHeartbeatLoopApplyOrderBoundary.plan_next(
            ClientHeartbeatLoopSkeletonResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopApplyOrderResult::ApplyTimerThenContinue {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 250,
                    wake_at: TimestampMicros(11_000),
                },
                carry,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_apply_order_prefers_retry_for_retry_carry() {
        let retry = ClientHeartbeatLoopRetryApplyResult {
            failure_result: ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                failed_at: TimestampMicros(10_600),
            },
            retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                next_attempt: 2,
                retry_at: TimestampMicros(11_100),
            },
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                sleep_micros: 250,
                wake_at: TimestampMicros(11_100),
            },
        };
        let carry = ClientHeartbeatLoopIterationCarryState {
            ordering: ClientHeartbeatLoopStepOrdering::RetryThenContinue {
                retry: retry.clone(),
            },
            final_counters: ClientHeartbeatLoopCountersState::default(),
            next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                continue_requested: true,
                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                    handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                        mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                        destination: "127.0.0.1:5000".parse().unwrap(),
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        protocol_version: ProtocolVersion(2),
                        cadence: ClientHeartbeatLoopCadenceInput {
                            heartbeat_interval_micros: 1_000,
                            ack_receive_timeout_micros: 500,
                            ack_observation_return:
                                ClientHeartbeatAckObservationReturnMode::Disabled,
                        },
                        stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                        max_ack_socket_wait_micros: 500,
                        max_sleep_micros: 250,
                        retry_policy: ClientHeartbeatLoopRetryPolicy {
                            max_attempts: 3,
                            retry_delay_micros: 1_000,
                        },
                        local_time_enabled: true,
                        short_status: Some("one-tick-runtime".to_string()),
                    },
                    now: TimestampMicros(11_100),
                    stop_requested: false,
                    retry_attempts_used: 2,
                },
            },
        };

        let result = ClientHeartbeatLoopApplyOrderBoundary.plan_next(
            ClientHeartbeatLoopSkeletonResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopApplyOrderResult::ApplyRetryThenContinue { retry, carry }
        );
    }

    #[test]
    fn client_heartbeat_loop_apply_order_triggers_cleanup_from_stop_handoff() {
        let handoff = ClientHeartbeatLoopWhileLoopStopHandoff {
            stop: ClientHeartbeatLoopCompletedBodyStopResult {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                },
            },
            final_counters: ClientHeartbeatLoopCountersState::default(),
        };

        let result = ClientHeartbeatLoopApplyOrderBoundary.plan_next(
            ClientHeartbeatLoopSkeletonResult::Stop {
                handoff: handoff.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopApplyOrderResult::TriggerCleanup {
                trigger: ClientHeartbeatLoopCleanupTrigger { handoff }
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_outer_shell_preserves_continue_apply_order() {
        let apply_order = ClientHeartbeatLoopApplyOrderResult::ApplyTimerThenContinue {
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                sleep_micros: 250,
                wake_at: TimestampMicros(11_000),
            },
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                        sleep_micros: 250,
                        wake_at: TimestampMicros(11_000),
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopOuterShellBoundary.plan_next(apply_order.clone());

        assert_eq!(
            result,
            ClientHeartbeatLoopShellResult::Continue { apply_order }
        );
    }

    #[test]
    fn client_heartbeat_loop_outer_shell_maps_cleanup_trigger_to_stop_reason() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopOuterShellBoundary.plan_next(
            ClientHeartbeatLoopApplyOrderResult::TriggerCleanup {
                trigger: trigger.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopShellResult::Stop {
                reason: ClientHeartbeatLoopShellStopReason::CleanupRequested {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                },
                trigger,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_shell_runner_preserves_continue_apply_order() {
        let apply_order = ClientHeartbeatLoopApplyOrderResult::ApplyRetryThenContinue {
            retry: ClientHeartbeatLoopRetryApplyResult {
                failure_result: ClientHeartbeatLoopIterationRuntimeResult::Failed {
                    kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                    failed_at: TimestampMicros(10_600),
                },
                retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                    reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                    next_attempt: 2,
                    retry_at: TimestampMicros(11_100),
                },
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                    sleep_micros: 250,
                    wake_at: TimestampMicros(11_100),
                },
            },
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::RetryThenContinue {
                    retry: ClientHeartbeatLoopRetryApplyResult {
                        failure_result: ClientHeartbeatLoopIterationRuntimeResult::Failed {
                            kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                            failed_at: TimestampMicros(10_600),
                        },
                        retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                            reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                            next_attempt: 2,
                            retry_at: TimestampMicros(11_100),
                        },
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                            sleep_micros: 250,
                            wake_at: TimestampMicros(11_100),
                        },
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_100),
                        stop_requested: false,
                        retry_attempts_used: 2,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopShellRunnerBoundary::default().run_one(apply_order.clone());

        assert_eq!(
            result,
            ClientHeartbeatLoopShellRunnerResult::Continue { apply_order }
        );
    }

    #[test]
    fn client_heartbeat_loop_shell_runner_maps_cleanup_to_runner_stop_reason() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopShellRunnerBoundary::default().run_one(
            ClientHeartbeatLoopApplyOrderResult::TriggerCleanup {
                trigger: trigger.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopShellRunnerResult::Stop {
                reason: ClientHeartbeatLoopShellRunnerStopReason::CleanupRequested {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                },
                trigger,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_repeated_invocation_preserves_next_step_carry() {
        let carry = ClientHeartbeatLoopIterationCarryState {
            ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 250,
                    wake_at: TimestampMicros(11_000),
                },
            },
            final_counters: ClientHeartbeatLoopCountersState::default(),
            next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                continue_requested: true,
                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                    handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                        mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                        destination: "127.0.0.1:5000".parse().unwrap(),
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        protocol_version: ProtocolVersion(2),
                        cadence: ClientHeartbeatLoopCadenceInput {
                            heartbeat_interval_micros: 1_000,
                            ack_receive_timeout_micros: 500,
                            ack_observation_return:
                                ClientHeartbeatAckObservationReturnMode::Disabled,
                        },
                        stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                        max_ack_socket_wait_micros: 500,
                        max_sleep_micros: 250,
                        retry_policy: ClientHeartbeatLoopRetryPolicy {
                            max_attempts: 3,
                            retry_delay_micros: 1_000,
                        },
                        local_time_enabled: true,
                        short_status: Some("one-tick-runtime".to_string()),
                    },
                    now: TimestampMicros(11_000),
                    stop_requested: false,
                    retry_attempts_used: 0,
                },
            },
        };

        let result = ClientHeartbeatLoopRepeatedInvocationBoundary.plan_next(
            ClientHeartbeatLoopShellRunnerResult::Continue {
                apply_order: ClientHeartbeatLoopApplyOrderResult::ApplyTimerThenContinue {
                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                        sleep_micros: 250,
                        wake_at: TimestampMicros(11_000),
                    },
                    carry: carry.clone(),
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopRepeatedInvocationResult::Continue {
                carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                        sleep_micros: 250,
                        wake_at: TimestampMicros(11_000),
                    },
                    carry,
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_repeated_invocation_maps_stop_to_cleanup_handoff() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopRepeatedInvocationBoundary.plan_next(
            ClientHeartbeatLoopShellRunnerResult::Stop {
                reason: ClientHeartbeatLoopShellRunnerStopReason::CleanupRequested {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                },
                trigger: trigger.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopRepeatedInvocationResult::Stop {
                reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                },
                trigger,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_actual_while_loop_preserves_continue_carry() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopActualWhileLoopBoundary.plan_next(
            ClientHeartbeatLoopRepeatedInvocationResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopInvocationStepResult::Continue { carry }
        );
    }

    #[test]
    fn client_heartbeat_loop_actual_while_loop_maps_stop_to_handoff() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopActualWhileLoopBoundary.plan_next(
            ClientHeartbeatLoopRepeatedInvocationResult::Stop {
                reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                },
                trigger: trigger.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopInvocationStepResult::Stop {
                handoff: ClientHeartbeatLoopActualWhileLoopStopHandoff {
                    reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                    trigger,
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_responsibility_preserves_continue_carry() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCleanupResponsibilityBoundary.plan_next(
            ClientHeartbeatLoopInvocationStepResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCleanupResponsibilityResult::Continue { carry }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_responsibility_builds_explicit_cleanup_input() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };
        let handoff = ClientHeartbeatLoopActualWhileLoopStopHandoff {
            reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            },
            trigger: trigger.clone(),
        };

        let result = ClientHeartbeatLoopCleanupResponsibilityBoundary.plan_next(
            ClientHeartbeatLoopInvocationStepResult::Stop {
                handoff: handoff.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCleanupResponsibilityResult::Cleanup {
                input: ClientHeartbeatLoopCleanupResponsibilityInput {
                    handoff,
                    plan: ClientHeartbeatLoopCleanupPlan::CleanupOnStop { trigger },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_execution_input_converts_stop_path() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };
        let handoff = ClientHeartbeatLoopCleanupOrderingHandoff {
            stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            },
            ordered_plan: ClientHeartbeatLoopOrderedCleanupPlan::CleanupOnStop { trigger },
        };

        let input = ClientHeartbeatLoopCleanupExecutionInput::from_ordering(
            ClientHeartbeatLoopCleanupOrderingResult::Ordered {
                handoff: handoff.clone(),
            },
        )
        .expect("stop path should produce cleanup execution input");

        assert_eq!(input, ClientHeartbeatLoopCleanupExecutionInput { handoff });
    }

    #[test]
    fn client_heartbeat_loop_cleanup_execution_skips_continue_path() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCleanupExecutionInput::from_ordering(
            ClientHeartbeatLoopCleanupOrderingResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(result, Err(carry));
    }

    #[test]
    fn client_heartbeat_loop_cleanup_execution_planning_preserves_stop_only_semantics() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopCleanupExecutionBoundary.plan_next(
            ClientHeartbeatLoopCleanupOrderingResult::Ordered {
                handoff: ClientHeartbeatLoopCleanupOrderingHandoff {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    ordered_plan: ClientHeartbeatLoopOrderedCleanupPlan::CleanupOnStop {
                        trigger: trigger.clone(),
                    },
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCleanupExecutionResult::Planned {
                handoff: ClientHeartbeatLoopCleanupExecutionPlanningHandoff {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    execution_plan: ClientHeartbeatLoopCleanupExecutionPlan::CleanupOnStop {
                        trigger,
                        future_actions: [
                            ClientHeartbeatLoopFutureCleanupAction::FinalFlush,
                            ClientHeartbeatLoopFutureCleanupAction::LogWriterInvocation,
                            ClientHeartbeatLoopFutureCleanupAction::ResourceRelease,
                        ],
                    },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_execution_keeps_future_actions_ordered_only() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopCleanupExecutionBoundary.plan_next(
            ClientHeartbeatLoopCleanupOrderingResult::Ordered {
                handoff: ClientHeartbeatLoopCleanupOrderingHandoff {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    ordered_plan: ClientHeartbeatLoopOrderedCleanupPlan::CleanupOnStop { trigger },
                },
            },
        );

        let ClientHeartbeatLoopCleanupExecutionResult::Planned { handoff } = result else {
            panic!("stop path should produce cleanup execution planning");
        };

        let future_actions = match handoff.execution_plan {
            ClientHeartbeatLoopCleanupExecutionPlan::CleanupOnStop { future_actions, .. } => {
                future_actions
            }
        };

        assert_eq!(
            future_actions,
            [
                ClientHeartbeatLoopFutureCleanupAction::FinalFlush,
                ClientHeartbeatLoopFutureCleanupAction::LogWriterInvocation,
                ClientHeartbeatLoopFutureCleanupAction::ResourceRelease,
            ]
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_side_effect_input_converts_stop_path() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let input = ClientHeartbeatLoopCleanupSideEffectInput::from_execution_planning(
            ClientHeartbeatLoopCleanupExecutionResult::Planned {
                handoff: ClientHeartbeatLoopCleanupExecutionPlanningHandoff {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    execution_plan: ClientHeartbeatLoopCleanupExecutionPlan::CleanupOnStop {
                        trigger: trigger.clone(),
                        future_actions: [
                            ClientHeartbeatLoopFutureCleanupAction::FinalFlush,
                            ClientHeartbeatLoopFutureCleanupAction::LogWriterInvocation,
                            ClientHeartbeatLoopFutureCleanupAction::ResourceRelease,
                        ],
                    },
                },
            },
        )
        .expect("stop path should produce cleanup side-effect input");

        assert_eq!(
            input,
            ClientHeartbeatLoopCleanupSideEffectInput {
                handoff: ClientHeartbeatLoopCleanupExecutionPlanningHandoff {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    execution_plan: ClientHeartbeatLoopCleanupExecutionPlan::CleanupOnStop {
                        trigger,
                        future_actions: [
                            ClientHeartbeatLoopFutureCleanupAction::FinalFlush,
                            ClientHeartbeatLoopFutureCleanupAction::LogWriterInvocation,
                            ClientHeartbeatLoopFutureCleanupAction::ResourceRelease,
                        ],
                    },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_side_effect_input_skips_continue_path() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCleanupSideEffectInput::from_execution_planning(
            ClientHeartbeatLoopCleanupExecutionResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(result, Err(carry));
    }

    #[test]
    fn client_heartbeat_loop_cleanup_side_effect_boundary_preserves_stop_only_semantics() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopCleanupSideEffectBoundary.apply(
            ClientHeartbeatLoopCleanupExecutionResult::Planned {
                handoff: ClientHeartbeatLoopCleanupExecutionPlanningHandoff {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    execution_plan: ClientHeartbeatLoopCleanupExecutionPlan::CleanupOnStop {
                        trigger,
                        future_actions: [
                            ClientHeartbeatLoopFutureCleanupAction::FinalFlush,
                            ClientHeartbeatLoopFutureCleanupAction::LogWriterInvocation,
                            ClientHeartbeatLoopFutureCleanupAction::ResourceRelease,
                        ],
                    },
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCleanupSideEffectResult::Applied {
                result: ClientHeartbeatLoopCleanupSideEffectApplyResult {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_side_effect_apply_keeps_flush_log_release_order_explicit() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopCleanupSideEffectBoundary.apply(
            ClientHeartbeatLoopCleanupExecutionResult::Planned {
                handoff: ClientHeartbeatLoopCleanupExecutionPlanningHandoff {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    execution_plan: ClientHeartbeatLoopCleanupExecutionPlan::CleanupOnStop {
                        trigger,
                        future_actions: [
                            ClientHeartbeatLoopFutureCleanupAction::FinalFlush,
                            ClientHeartbeatLoopFutureCleanupAction::LogWriterInvocation,
                            ClientHeartbeatLoopFutureCleanupAction::ResourceRelease,
                        ],
                    },
                },
            },
        );

        let ClientHeartbeatLoopCleanupSideEffectResult::Applied { result } = result else {
            panic!("stop path should produce cleanup side-effect apply result");
        };

        assert_eq!(
            result.applied_actions,
            [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ]
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_stop_path_output_converts_stop_path() {
        let input = ClientHeartbeatLoopCompletedLoopStopPathInput::from_cleanup_side_effect(
            ClientHeartbeatLoopCleanupSideEffectResult::Applied {
                result: ClientHeartbeatLoopCleanupSideEffectApplyResult {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        )
        .expect("stop path should produce terminal stop-path input");

        assert_eq!(
            input,
            ClientHeartbeatLoopCompletedLoopStopPathInput {
                result: ClientHeartbeatLoopCleanupSideEffectApplyResult {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_stop_path_output_skips_continue_path() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCompletedLoopStopPathInput::from_cleanup_side_effect(
            ClientHeartbeatLoopCleanupSideEffectResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(result, Err(carry));
    }

    #[test]
    fn client_heartbeat_loop_cleanup_stop_path_boundary_keeps_continue_path_non_terminal() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCompletedLoopStopPathBoundary.plan_next(
            ClientHeartbeatLoopCleanupSideEffectResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCompletedLoopStopPathResult::Continue { carry }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_stop_path_boundary_preserves_stop_only_semantics() {
        let result = ClientHeartbeatLoopCompletedLoopStopPathBoundary.plan_next(
            ClientHeartbeatLoopCleanupSideEffectResult::Applied {
                result: ClientHeartbeatLoopCleanupSideEffectApplyResult {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCompletedLoopStopPathResult::Stop {
                handoff: ClientHeartbeatLoopCompletedLoopStopPathHandoff {
                    output: ClientHeartbeatLoopTerminalStopPathOutput {
                        stop_reason:
                            ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                                stop_reason:
                                    ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                            },
                        cleanup_completed: true,
                        applied_actions: [
                            ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                            ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                            ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                        ],
                    },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_stop_path_output_does_not_reinterpret_cleanup_planning() {
        let result = ClientHeartbeatLoopCompletedLoopStopPathBoundary.plan_next(
            ClientHeartbeatLoopCleanupSideEffectResult::Applied {
                result: ClientHeartbeatLoopCleanupSideEffectApplyResult {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        );

        let ClientHeartbeatLoopCompletedLoopStopPathResult::Stop { handoff } = result else {
            panic!("stop path should produce terminal stop-path output");
        };

        assert_eq!(
            handoff.output.applied_actions,
            [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ]
        );
        assert!(handoff.output.cleanup_completed);
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_while_loop_termination_input_converts_stop_path() {
        let input =
            ClientHeartbeatLoopActualWhileLoopTerminationInput::from_completed_loop_stop_path(
                ClientHeartbeatLoopCompletedLoopStopPathResult::Stop {
                    handoff: ClientHeartbeatLoopCompletedLoopStopPathHandoff {
                        output: ClientHeartbeatLoopTerminalStopPathOutput {
                            stop_reason:
                                ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                                    stop_reason:
                                        ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                                },
                            cleanup_completed: true,
                            applied_actions: [
                                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                            ],
                        },
                    },
                },
            )
            .expect("stop path should produce actual while-loop termination input");

        assert_eq!(
            input,
            ClientHeartbeatLoopActualWhileLoopTerminationInput {
                handoff: ClientHeartbeatLoopCompletedLoopStopPathHandoff {
                    output: ClientHeartbeatLoopTerminalStopPathOutput {
                        stop_reason:
                            ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                                stop_reason:
                                    ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                            },
                        cleanup_completed: true,
                        applied_actions: [
                            ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                            ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                            ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                        ],
                    },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_while_loop_termination_input_skips_continue_path() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result =
            ClientHeartbeatLoopActualWhileLoopTerminationInput::from_completed_loop_stop_path(
                ClientHeartbeatLoopCompletedLoopStopPathResult::Continue {
                    carry: carry.clone(),
                },
            );

        assert_eq!(result, Err(carry));
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_while_loop_termination_keeps_continue_path_non_terminal(
    ) {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopActualWhileLoopTerminationBoundary.plan_next(
            ClientHeartbeatLoopCompletedLoopStopPathResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Continue { carry }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_while_loop_termination_preserves_stop_only_semantics() {
        let result = ClientHeartbeatLoopActualWhileLoopTerminationBoundary.plan_next(
            ClientHeartbeatLoopCompletedLoopStopPathResult::Stop {
                handoff: ClientHeartbeatLoopCompletedLoopStopPathHandoff {
                    output: ClientHeartbeatLoopTerminalStopPathOutput {
                        stop_reason:
                            ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                                stop_reason:
                                    ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                            },
                        cleanup_completed: true,
                        applied_actions: [
                            ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                            ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                            ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                        ],
                    },
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Terminated {
                output: ClientHeartbeatLoopActualWhileLoopTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_while_loop_termination_output_preserves_cleanup_state()
    {
        let result = ClientHeartbeatLoopActualWhileLoopTerminationBoundary.plan_next(
            ClientHeartbeatLoopCompletedLoopStopPathResult::Stop {
                handoff: ClientHeartbeatLoopCompletedLoopStopPathHandoff {
                    output: ClientHeartbeatLoopTerminalStopPathOutput {
                        stop_reason:
                            ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                                stop_reason:
                                    ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                            },
                        cleanup_completed: true,
                        applied_actions: [
                            ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                            ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                            ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                        ],
                    },
                },
            },
        );

        let ClientHeartbeatLoopActualWhileLoopTerminationResult::Terminated { output } = result
        else {
            panic!("stop path should produce actual while-loop termination output");
        };

        assert_eq!(
            output.stop_reason,
            ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            }
        );
        assert!(output.cleanup_completed);
        assert_eq!(
            output.applied_actions,
            [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ]
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_input_converts_stop_path() {
        let input = ClientHeartbeatLoopCompletedBodyInput::from_actual_while_loop_termination(
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Terminated {
                output: ClientHeartbeatLoopActualWhileLoopTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        )
        .expect("stop path should produce completed loop body input");

        assert_eq!(
            input,
            ClientHeartbeatLoopCompletedBodyInput {
                output: ClientHeartbeatLoopActualWhileLoopTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_input_skips_continue_path() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCompletedBodyInput::from_actual_while_loop_termination(
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(result, Err(carry));
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_integration_keeps_continue_path_separate() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCompletedBodyIntegrationBoundary.plan_next(
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCompletedBodyIntegrationResult::Continue { carry }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_integration_preserves_stop_only_semantics() {
        let result = ClientHeartbeatLoopCompletedBodyIntegrationBoundary.plan_next(
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Terminated {
                output: ClientHeartbeatLoopActualWhileLoopTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCompletedBodyIntegrationResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_result_preserves_cleanup_state() {
        let result = ClientHeartbeatLoopCompletedBodyIntegrationBoundary.plan_next(
            ClientHeartbeatLoopActualWhileLoopTerminationResult::Terminated {
                output: ClientHeartbeatLoopActualWhileLoopTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        );

        let ClientHeartbeatLoopCompletedBodyIntegrationResult::Stop { output } = result else {
            panic!("stop path should produce completed loop body result");
        };

        assert_eq!(
            output.stop_reason,
            ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            }
        );
        assert!(output.cleanup_completed);
        assert_eq!(
            output.applied_actions,
            [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ]
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timer_retry_reconnect_input_converts_continue_path() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                sleep_micros: 1_000,
                wake_at: TimestampMicros(12_000),
            },
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                        sleep_micros: 1_000,
                        wake_at: TimestampMicros(12_000),
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let input =
            ClientHeartbeatLoopTimerRetryReconnectIntegrationInput::from_completed_body_result(
                ClientHeartbeatLoopCompletedBodyIntegrationResult::Continue {
                    carry: carry.clone(),
                },
            )
            .expect("continue path should produce timer/retry/reconnect planning input");

        assert_eq!(
            input,
            ClientHeartbeatLoopTimerRetryReconnectIntegrationInput { carry }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timer_retry_reconnect_input_skips_stop_path() {
        let output = ClientHeartbeatLoopCompletedBodyTerminalOutput {
            stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            },
            cleanup_completed: true,
            applied_actions: [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ],
        };

        let result =
            ClientHeartbeatLoopTimerRetryReconnectIntegrationInput::from_completed_body_result(
                ClientHeartbeatLoopCompletedBodyIntegrationResult::Stop {
                    output: output.clone(),
                },
            );

        assert_eq!(result, Err(output));
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timer_retry_reconnect_preserves_stop_only_semantics() {
        let result = ClientHeartbeatLoopTimerRetryReconnectIntegrationBoundary.plan_next(
            ClientHeartbeatLoopCompletedBodyIntegrationResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timer_retry_reconnect_keeps_continue_stop_and_planning_distinct(
    ) {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopTimerRetryReconnectIntegrationBoundary.plan_next(
            ClientHeartbeatLoopCompletedBodyIntegrationResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::ContinuePlanning {
                handoff: ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff { carry }
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_execution_input_converts_continue_planning_handoff() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                sleep_micros: 1_000,
                wake_at: TimestampMicros(12_000),
            },
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                        sleep_micros: 1_000,
                        wake_at: TimestampMicros(12_000),
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let input =
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput::from_planning_handoff(
                ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::ContinuePlanning {
                    handoff: ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff {
                        carry: carry.clone(),
                    },
                },
            )
            .expect("continue planning should produce actual execution input");

        assert_eq!(
            input,
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput {
                handoff: ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff { carry }
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_execution_input_skips_stop_path() {
        let output = ClientHeartbeatLoopCompletedBodyTerminalOutput {
            stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            },
            cleanup_completed: true,
            applied_actions: [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ],
        };

        let result =
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput::from_planning_handoff(
                ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::Stop {
                    output: output.clone(),
                },
            );

        assert_eq!(result, Err(output));
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_execution_preserves_continue_stop_separation() {
        let result = ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary.plan_next(
            ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_actual_execution_keeps_timer_retry_reconnect_explicit() {
        let timer_result = ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary.plan_next(
            ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::ContinuePlanning {
                handoff: ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff {
                    carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 1_000,
                            wake_at: TimestampMicros(12_000),
                        },
                        carry: ClientHeartbeatLoopIterationCarryState {
                            ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                    sleep_micros: 1_000,
                                    wake_at: TimestampMicros(12_000),
                                },
                            },
                            final_counters: ClientHeartbeatLoopCountersState::default(),
                            next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                                continue_requested: true,
                                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                                    handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                        mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                        destination: "127.0.0.1:5000".parse().unwrap(),
                                        client_id: ClientId("client-1".to_string()),
                                        run_id: RunId("run-1".to_string()),
                                        protocol_version: ProtocolVersion(2),
                                        cadence: ClientHeartbeatLoopCadenceInput {
                                            heartbeat_interval_micros: 1_000,
                                            ack_receive_timeout_micros: 500,
                                            ack_observation_return:
                                                ClientHeartbeatAckObservationReturnMode::Disabled,
                                        },
                                        stop_condition:
                                            ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                        max_ack_socket_wait_micros: 500,
                                        max_sleep_micros: 250,
                                        retry_policy: ClientHeartbeatLoopRetryPolicy {
                                            max_attempts: 3,
                                            retry_delay_micros: 1_000,
                                        },
                                        local_time_enabled: true,
                                        short_status: Some("one-tick-runtime".to_string()),
                                    },
                                    now: TimestampMicros(11_000),
                                    stop_requested: false,
                                    retry_attempts_used: 0,
                                },
                            },
                        },
                    },
                },
            },
        );

        let retry_result = ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary.plan_next(
            ClientHeartbeatLoopTimerRetryReconnectIntegrationResult::ContinuePlanning {
                handoff: ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff {
                    carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyRetryThenContinue {
                        retry: ClientHeartbeatLoopRetryApplyResult {
                            failure_result: ClientHeartbeatLoopIterationRuntimeResult::Failed {
                                kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                                failed_at: TimestampMicros(11_000),
                            },
                            retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                                next_attempt: 2,
                                retry_at: TimestampMicros(12_000),
                            },
                            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                                sleep_micros: 1_000,
                                wake_at: TimestampMicros(12_000),
                            },
                        },
                        carry: ClientHeartbeatLoopIterationCarryState {
                            ordering: ClientHeartbeatLoopStepOrdering::RetryThenContinue {
                                retry: ClientHeartbeatLoopRetryApplyResult {
                                    failure_result:
                                        ClientHeartbeatLoopIterationRuntimeResult::Failed {
                                            kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                                            failed_at: TimestampMicros(11_000),
                                        },
                                    retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                                        reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                                        next_attempt: 2,
                                        retry_at: TimestampMicros(12_000),
                                    },
                                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                        reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                                        sleep_micros: 1_000,
                                        wake_at: TimestampMicros(12_000),
                                    },
                                },
                            },
                            final_counters: ClientHeartbeatLoopCountersState::default(),
                            next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                                continue_requested: true,
                                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                                    handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                        mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                        destination: "127.0.0.1:5000".parse().unwrap(),
                                        client_id: ClientId("client-1".to_string()),
                                        run_id: RunId("run-1".to_string()),
                                        protocol_version: ProtocolVersion(2),
                                        cadence: ClientHeartbeatLoopCadenceInput {
                                            heartbeat_interval_micros: 1_000,
                                            ack_receive_timeout_micros: 500,
                                            ack_observation_return:
                                                ClientHeartbeatAckObservationReturnMode::Disabled,
                                        },
                                        stop_condition:
                                            ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                        max_ack_socket_wait_micros: 500,
                                        max_sleep_micros: 250,
                                        retry_policy: ClientHeartbeatLoopRetryPolicy {
                                            max_attempts: 3,
                                            retry_delay_micros: 1_000,
                                        },
                                        local_time_enabled: true,
                                        short_status: Some("one-tick-runtime".to_string()),
                                    },
                                    now: TimestampMicros(11_000),
                                    stop_requested: false,
                                    retry_attempts_used: 0,
                                },
                            },
                        },
                    },
                },
            },
        );

        let ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::ContinueExecution {
            handoff: timer_handoff,
        } = timer_result
        else {
            panic!("continue planning should produce explicit timer execution handoff");
        };
        let ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::ContinueExecution {
            handoff: retry_handoff,
        } = retry_result
        else {
            panic!("continue planning should produce explicit retry execution handoff");
        };

        assert_eq!(
            timer_handoff.timer_wait,
            ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
            }
        );
        assert_eq!(
            timer_handoff.retry_execution,
            ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution
        );
        assert_eq!(
            timer_handoff.reconnect_execution,
            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution
        );
        assert_eq!(
            retry_handoff.timer_wait,
            ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait
        );
        match retry_handoff.retry_execution {
            ClientHeartbeatLoopFutureActualRetryExecutionAction::RetryExecution { .. } => {}
            ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution => {
                panic!("retry carry should remain an explicit retry execution action");
            }
        }
        assert_eq!(
            retry_handoff.reconnect_execution,
            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_connection_input_converts_continue_execution() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let input = ClientHeartbeatLoopCompletedContinuousBodyConnectionInput::from_actual_execution_integration(
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::ContinueExecution {
                handoff: ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff {
                    carry: carry.clone(),
                    timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait,
                    retry_execution:
                        ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
                    reconnect_execution:
                        ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
                },
            },
        )
        .expect("continue execution should produce completed loop body connection input");

        assert_eq!(
            input,
            ClientHeartbeatLoopCompletedContinuousBodyConnectionInput {
                handoff: ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff {
                    carry,
                    timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait,
                    retry_execution:
                        ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
                    reconnect_execution:
                        ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_connection_input_skips_stop_path() {
        let output = ClientHeartbeatLoopCompletedBodyTerminalOutput {
            stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            },
            cleanup_completed: true,
            applied_actions: [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ],
        };

        let result = ClientHeartbeatLoopCompletedContinuousBodyConnectionInput::from_actual_execution_integration(
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::Stop {
                output: output.clone(),
            },
        );

        assert_eq!(result, Err(output));
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_connection_preserves_continue_stop_separation()
    {
        let result = ClientHeartbeatLoopCompletedContinuousBodyConnectionBoundary.plan_next(
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCompletedContinuousBodyConnectionResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_body_connection_keeps_execution_actions_explicit() {
        let result = ClientHeartbeatLoopCompletedContinuousBodyConnectionBoundary.plan_next(
            ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult::ContinueExecution {
                handoff: ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff {
                    carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 1_000,
                            wake_at: TimestampMicros(12_000),
                        },
                        carry: ClientHeartbeatLoopIterationCarryState {
                            ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                    sleep_micros: 1_000,
                                    wake_at: TimestampMicros(12_000),
                                },
                            },
                            final_counters: ClientHeartbeatLoopCountersState::default(),
                            next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                                continue_requested: true,
                                body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                                    handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                        mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                        destination: "127.0.0.1:5000".parse().unwrap(),
                                        client_id: ClientId("client-1".to_string()),
                                        run_id: RunId("run-1".to_string()),
                                        protocol_version: ProtocolVersion(2),
                                        cadence: ClientHeartbeatLoopCadenceInput {
                                            heartbeat_interval_micros: 1_000,
                                            ack_receive_timeout_micros: 500,
                                            ack_observation_return:
                                                ClientHeartbeatAckObservationReturnMode::Disabled,
                                        },
                                        stop_condition:
                                            ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                        max_ack_socket_wait_micros: 500,
                                        max_sleep_micros: 250,
                                        retry_policy: ClientHeartbeatLoopRetryPolicy {
                                            max_attempts: 3,
                                            retry_delay_micros: 1_000,
                                        },
                                        local_time_enabled: true,
                                        short_status: Some("one-tick-runtime".to_string()),
                                    },
                                    now: TimestampMicros(11_000),
                                    stop_requested: false,
                                    retry_attempts_used: 0,
                                },
                            },
                        },
                    },
                    timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 1_000,
                            wake_at: TimestampMicros(12_000),
                        },
                    },
                    retry_execution:
                        ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
                    reconnect_execution:
                        ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
                },
            },
        );

        let ClientHeartbeatLoopCompletedContinuousBodyConnectionResult::Continue { output } =
            result
        else {
            panic!("continue execution should remain explicit in completed body connection");
        };

        assert_eq!(
            output.timer_wait,
            ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
            }
        );
        assert_eq!(
            output.retry_execution,
            ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution
        );
        assert_eq!(
            output.reconnect_execution,
            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_continuous_body_preserves_continue_future_execution_actions(
    ) {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                sleep_micros: 1_000,
                wake_at: TimestampMicros(12_000),
            },
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                        sleep_micros: 1_000,
                        wake_at: TimestampMicros(12_000),
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCompletedContinuousBodyBoundary::default().plan_next(
            ClientHeartbeatLoopRepeatedInvocationResult::Continue {
                carry: carry.clone(),
            },
        );

        let ClientHeartbeatLoopCompletedContinuousBodyResult::Continue { output } = result else {
            panic!("continue path should remain explicit in completed continuous body");
        };

        assert_eq!(output.carry, carry);
        assert_eq!(
            output.timer_wait,
            ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
            }
        );
        assert_eq!(
            output.retry_execution,
            ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution
        );
        assert_eq!(
            output.reconnect_execution,
            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_continuous_body_preserves_stop_semantics() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopCompletedContinuousBodyBoundary::default().plan_next(
            ClientHeartbeatLoopRepeatedInvocationResult::Stop {
                reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                },
                trigger,
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCompletedContinuousBodyResult::Stop {
                output: ClientHeartbeatLoopCompletedBodyTerminalOutput {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    cleanup_completed: true,
                    applied_actions: [
                        ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                        ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                        ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
                    ],
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_completed_continuous_body_keeps_continue_and_stop_distinct() {
        let continue_result = ClientHeartbeatLoopCompletedContinuousBodyBoundary::default()
            .plan_next(ClientHeartbeatLoopRepeatedInvocationResult::Continue {
                carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
                    carry: ClientHeartbeatLoopIterationCarryState {
                        ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                        final_counters: ClientHeartbeatLoopCountersState::default(),
                        next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                            continue_requested: true,
                            body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                                handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                    mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                    destination: "127.0.0.1:5000".parse().unwrap(),
                                    client_id: ClientId("client-1".to_string()),
                                    run_id: RunId("run-1".to_string()),
                                    protocol_version: ProtocolVersion(2),
                                    cadence: ClientHeartbeatLoopCadenceInput {
                                        heartbeat_interval_micros: 1_000,
                                        ack_receive_timeout_micros: 500,
                                        ack_observation_return:
                                            ClientHeartbeatAckObservationReturnMode::Disabled,
                                    },
                                    stop_condition:
                                        ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                    max_ack_socket_wait_micros: 500,
                                    max_sleep_micros: 250,
                                    retry_policy: ClientHeartbeatLoopRetryPolicy {
                                        max_attempts: 3,
                                        retry_delay_micros: 1_000,
                                    },
                                    local_time_enabled: true,
                                    short_status: Some("one-tick-runtime".to_string()),
                                },
                                now: TimestampMicros(11_000),
                                stop_requested: false,
                                retry_attempts_used: 0,
                            },
                        },
                    },
                },
            });
        let stop_result = ClientHeartbeatLoopCompletedContinuousBodyBoundary::default().plan_next(
            ClientHeartbeatLoopRepeatedInvocationResult::Stop {
                reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                },
                trigger: ClientHeartbeatLoopCleanupTrigger {
                    handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                        stop: ClientHeartbeatLoopCompletedBodyStopResult {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                            cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                                stop_reason:
                                    ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                            },
                        },
                        final_counters: ClientHeartbeatLoopCountersState::default(),
                    },
                },
            },
        );

        match continue_result {
            ClientHeartbeatLoopCompletedContinuousBodyResult::Continue { .. } => {}
            ClientHeartbeatLoopCompletedContinuousBodyResult::Stop { .. } => {
                panic!("continue path should not collapse into stop output");
            }
        }

        match stop_result {
            ClientHeartbeatLoopCompletedContinuousBodyResult::Stop { .. } => {}
            ClientHeartbeatLoopCompletedContinuousBodyResult::Continue { .. } => {
                panic!("stop path should not collapse into continue output");
            }
        }
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timeout_notice_wakeup_input_converts_continue_with_timer_wait()
    {
        let output = ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput {
            carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
                carry: ClientHeartbeatLoopIterationCarryState {
                    ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 1_000,
                            wake_at: TimestampMicros(12_000),
                        },
                    },
                    final_counters: ClientHeartbeatLoopCountersState::default(),
                    next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                        continue_requested: true,
                        body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                            handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                destination: "127.0.0.1:5000".parse().unwrap(),
                                client_id: ClientId("client-1".to_string()),
                                run_id: RunId("run-1".to_string()),
                                protocol_version: ProtocolVersion(2),
                                cadence: ClientHeartbeatLoopCadenceInput {
                                    heartbeat_interval_micros: 1_000,
                                    ack_receive_timeout_micros: 500,
                                    ack_observation_return:
                                        ClientHeartbeatAckObservationReturnMode::Disabled,
                                },
                                stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                max_ack_socket_wait_micros: 500,
                                max_sleep_micros: 250,
                                retry_policy: ClientHeartbeatLoopRetryPolicy {
                                    max_attempts: 3,
                                    retry_delay_micros: 1_000,
                                },
                                local_time_enabled: true,
                                short_status: Some("one-tick-runtime".to_string()),
                            },
                            now: TimestampMicros(11_000),
                            stop_requested: false,
                            retry_attempts_used: 0,
                        },
                    },
                },
            },
            timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
            },
            retry_execution: ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
            reconnect_execution:
                ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
        };

        let input =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput::from_completed_continuous_body(
                ClientHeartbeatLoopCompletedContinuousBodyResult::Continue {
                    output: output.clone(),
                },
            )
            .expect("continue body output should produce timeout notice wakeup input");

        assert_eq!(
            input,
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput { output }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timeout_notice_wakeup_can_pass_through_without_wakeup() {
        let retry = ClientHeartbeatLoopRetryApplyResult {
            failure_result: ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                failed_at: TimestampMicros(11_000),
            },
            retry_decision: ClientHeartbeatLoopRetryDecision::RetryLater {
                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                next_attempt: 2,
                retry_at: TimestampMicros(12_000),
            },
            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                sleep_micros: 1_000,
                wake_at: TimestampMicros(12_000),
            },
        };

        let result = ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary
            .plan_next(ClientHeartbeatLoopCompletedContinuousBodyResult::Continue {
            output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput {
                carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyRetryThenContinue {
                    retry: retry.clone(),
                    carry: ClientHeartbeatLoopIterationCarryState {
                        ordering: ClientHeartbeatLoopStepOrdering::RetryThenContinue {
                            retry: retry.clone(),
                        },
                        final_counters: ClientHeartbeatLoopCountersState::default(),
                        next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                            continue_requested: true,
                            body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                                handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                    mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                    destination: "127.0.0.1:5000".parse().unwrap(),
                                    client_id: ClientId("client-1".to_string()),
                                    run_id: RunId("run-1".to_string()),
                                    protocol_version: ProtocolVersion(2),
                                    cadence: ClientHeartbeatLoopCadenceInput {
                                        heartbeat_interval_micros: 1_000,
                                        ack_receive_timeout_micros: 500,
                                        ack_observation_return:
                                            ClientHeartbeatAckObservationReturnMode::Disabled,
                                    },
                                    stop_condition:
                                        ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                    max_ack_socket_wait_micros: 500,
                                    max_sleep_micros: 250,
                                    retry_policy: ClientHeartbeatLoopRetryPolicy {
                                        max_attempts: 3,
                                        retry_delay_micros: 1_000,
                                    },
                                    local_time_enabled: true,
                                    short_status: Some("one-tick-runtime".to_string()),
                                },
                                now: TimestampMicros(11_000),
                                stop_requested: false,
                                retry_attempts_used: 1,
                            },
                        },
                    },
                },
                timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait,
                retry_execution:
                    ClientHeartbeatLoopFutureActualRetryExecutionAction::RetryExecution { retry },
                reconnect_execution:
                    ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
            },
        });

        let ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithoutWakeup { output } =
            result
        else {
            panic!("continue path without timer wait should pass through without wakeup");
        };

        assert_eq!(
            output.timer_wait,
            ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait
        );
        match output.retry_execution {
            ClientHeartbeatLoopFutureActualRetryExecutionAction::RetryExecution { .. } => {}
            ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution => {
                panic!("retry execution should remain separate from wakeup planning");
            }
        }
        assert_eq!(
            output.reconnect_execution,
            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timeout_notice_wakeup_preserves_stop_passthrough() {
        let output = ClientHeartbeatLoopCompletedBodyTerminalOutput {
            stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            },
            cleanup_completed: true,
            applied_actions: [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ],
        };

        let input_result =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput::from_completed_continuous_body(
                ClientHeartbeatLoopCompletedContinuousBodyResult::Stop {
                    output: output.clone(),
                },
            );
        let boundary_result = ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary.plan_next(
            ClientHeartbeatLoopCompletedContinuousBodyResult::Stop {
                output: output.clone(),
            },
        );

        assert_eq!(input_result, Err(output.clone()));
        assert_eq!(
            boundary_result,
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::Stop { output }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timeout_notice_wakeup_stays_separate_from_timer_retry_reconnect_execution(
    ) {
        let result = ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary
            .plan_next(ClientHeartbeatLoopCompletedContinuousBodyResult::Continue {
            output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput {
                carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                        sleep_micros: 1_000,
                        wake_at: TimestampMicros(12_000),
                    },
                    carry: ClientHeartbeatLoopIterationCarryState {
                        ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                sleep_micros: 1_000,
                                wake_at: TimestampMicros(12_000),
                            },
                        },
                        final_counters: ClientHeartbeatLoopCountersState::default(),
                        next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                            continue_requested: true,
                            body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                                handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                    mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                    destination: "127.0.0.1:5000".parse().unwrap(),
                                    client_id: ClientId("client-1".to_string()),
                                    run_id: RunId("run-1".to_string()),
                                    protocol_version: ProtocolVersion(2),
                                    cadence: ClientHeartbeatLoopCadenceInput {
                                        heartbeat_interval_micros: 1_000,
                                        ack_receive_timeout_micros: 500,
                                        ack_observation_return:
                                            ClientHeartbeatAckObservationReturnMode::Disabled,
                                    },
                                    stop_condition:
                                        ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                    max_ack_socket_wait_micros: 500,
                                    max_sleep_micros: 250,
                                    retry_policy: ClientHeartbeatLoopRetryPolicy {
                                        max_attempts: 3,
                                        retry_delay_micros: 1_000,
                                    },
                                    local_time_enabled: true,
                                    short_status: Some("one-tick-runtime".to_string()),
                                },
                                now: TimestampMicros(11_000),
                                stop_requested: false,
                                retry_attempts_used: 0,
                            },
                        },
                    },
                },
                timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                        sleep_micros: 1_000,
                        wake_at: TimestampMicros(12_000),
                    },
                },
                retry_execution:
                    ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
                reconnect_execution:
                    ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
            },
        });

        let ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithWakeup { handoff } =
            result
        else {
            panic!("timer wait should produce an explicit wakeup-ready handoff");
        };

        assert_eq!(
            handoff.wakeup,
            ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan::WakeupDuringTimerWait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
            }
        );
        assert_eq!(
            handoff.output.timer_wait,
            ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
            }
        );
        assert_eq!(
            handoff.output.retry_execution,
            ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution
        );
        assert_eq!(
            handoff.output.reconnect_execution,
            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timeout_notice_wakeup_execution_input_and_result_follow_continue_with_wakeup(
    ) {
        let output = ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput {
            carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
                carry: ClientHeartbeatLoopIterationCarryState {
                    ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 1_000,
                            wake_at: TimestampMicros(12_000),
                        },
                    },
                    final_counters: ClientHeartbeatLoopCountersState::default(),
                    next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                        continue_requested: true,
                        body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                            handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                destination: "127.0.0.1:5000".parse().unwrap(),
                                client_id: ClientId("client-1".to_string()),
                                run_id: RunId("run-1".to_string()),
                                protocol_version: ProtocolVersion(2),
                                cadence: ClientHeartbeatLoopCadenceInput {
                                    heartbeat_interval_micros: 1_000,
                                    ack_receive_timeout_micros: 500,
                                    ack_observation_return:
                                        ClientHeartbeatAckObservationReturnMode::Disabled,
                                },
                                stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                max_ack_socket_wait_micros: 500,
                                max_sleep_micros: 250,
                                retry_policy: ClientHeartbeatLoopRetryPolicy {
                                    max_attempts: 3,
                                    retry_delay_micros: 1_000,
                                },
                                local_time_enabled: true,
                                short_status: Some("one-tick-runtime".to_string()),
                            },
                            now: TimestampMicros(11_000),
                            stop_requested: false,
                            retry_attempts_used: 0,
                        },
                    },
                },
            },
            timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
            },
            retry_execution: ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
            reconnect_execution:
                ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
        };
        let planning = ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithWakeup {
            handoff: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff {
                output: output.clone(),
                wakeup:
                    ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan::WakeupDuringTimerWait {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 1_000,
                            wake_at: TimestampMicros(12_000),
                        },
                    },
            },
        };

        let input =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput::from_wakeup_planning(
                planning.clone(),
            )
            .expect("continue-with-wakeup should produce wakeup execution input");
        let result =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary.apply(planning);

        assert_eq!(
            input,
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput {
                handoff: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff {
                    output: output.clone(),
                    wakeup:
                        ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan::WakeupDuringTimerWait {
                            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                sleep_micros: 1_000,
                                wake_at: TimestampMicros(12_000),
                            },
                        },
                },
            }
        );
        assert_eq!(
            result,
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult::ContinueWithWakeupExecutionApplied {
                output: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionOutput {
                    output,
                    wakeup_apply:
                        ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupApplyResult::WakeupApplied {
                            wakeup:
                                ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan::WakeupDuringTimerWait {
                                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                        sleep_micros: 1_000,
                                        wake_at: TimestampMicros(12_000),
                                    },
                                },
                        },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timeout_notice_wakeup_execution_input_skips_continue_without_wakeup(
    ) {
        let output = ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput {
            carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
                carry: ClientHeartbeatLoopIterationCarryState {
                    ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                    final_counters: ClientHeartbeatLoopCountersState::default(),
                    next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                        continue_requested: true,
                        body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                            handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                destination: "127.0.0.1:5000".parse().unwrap(),
                                client_id: ClientId("client-1".to_string()),
                                run_id: RunId("run-1".to_string()),
                                protocol_version: ProtocolVersion(2),
                                cadence: ClientHeartbeatLoopCadenceInput {
                                    heartbeat_interval_micros: 1_000,
                                    ack_receive_timeout_micros: 500,
                                    ack_observation_return:
                                        ClientHeartbeatAckObservationReturnMode::Disabled,
                                },
                                stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                max_ack_socket_wait_micros: 500,
                                max_sleep_micros: 250,
                                retry_policy: ClientHeartbeatLoopRetryPolicy {
                                    max_attempts: 3,
                                    retry_delay_micros: 1_000,
                                },
                                local_time_enabled: true,
                                short_status: Some("one-tick-runtime".to_string()),
                            },
                            now: TimestampMicros(11_000),
                            stop_requested: false,
                            retry_attempts_used: 0,
                        },
                    },
                },
            },
            timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::NoTimerWait,
            retry_execution: ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
            reconnect_execution:
                ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
        };
        let planning =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithoutWakeup {
                output: output.clone(),
            };

        let input_result =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput::from_wakeup_planning(
                planning.clone(),
            );
        let result =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary.apply(planning);

        assert_eq!(
            input_result,
            Err(
                ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithoutWakeup {
                    output: output.clone(),
                }
            )
        );
        assert_eq!(
            result,
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult::ContinueWithoutWakeupExecution {
                output,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timeout_notice_wakeup_execution_preserves_stop_passthrough() {
        let output = ClientHeartbeatLoopCompletedBodyTerminalOutput {
            stop_reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            },
            cleanup_completed: true,
            applied_actions: [
                ClientHeartbeatLoopCleanupAppliedAction::FinalFlush,
                ClientHeartbeatLoopCleanupAppliedAction::LogWriterInvocation,
                ClientHeartbeatLoopCleanupAppliedAction::ResourceRelease,
            ],
        };
        let planning = ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::Stop {
            output: output.clone(),
        };

        let input_result =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput::from_wakeup_planning(
                planning.clone(),
            );
        let result =
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary.apply(planning);

        assert_eq!(
            input_result,
            Err(
                ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::Stop {
                    output: output.clone(),
                }
            )
        );
        assert_eq!(
            result,
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult::Stop { output }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_timeout_notice_wakeup_execution_stays_separate_from_timer_retry_reconnect_concerns(
    ) {
        let result = ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary.apply(
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult::ContinueWithWakeup {
                handoff: ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff {
                    output: ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput {
                        carry: ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ApplyTimerThenContinue {
                            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                sleep_micros: 1_000,
                                wake_at: TimestampMicros(12_000),
                            },
                            carry: ClientHeartbeatLoopIterationCarryState {
                                ordering: ClientHeartbeatLoopStepOrdering::WaitThenContinue {
                                    sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                        reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                        sleep_micros: 1_000,
                                        wake_at: TimestampMicros(12_000),
                                    },
                                },
                                final_counters: ClientHeartbeatLoopCountersState::default(),
                                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                                    continue_requested: true,
                                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                                            destination: "127.0.0.1:5000".parse().unwrap(),
                                            client_id: ClientId("client-1".to_string()),
                                            run_id: RunId("run-1".to_string()),
                                            protocol_version: ProtocolVersion(2),
                                            cadence: ClientHeartbeatLoopCadenceInput {
                                                heartbeat_interval_micros: 1_000,
                                                ack_receive_timeout_micros: 500,
                                                ack_observation_return:
                                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                                            },
                                            stop_condition:
                                                ClientHeartbeatLoopStopCondition::RunUntilStopped,
                                            max_ack_socket_wait_micros: 500,
                                            max_sleep_micros: 250,
                                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                                max_attempts: 3,
                                                retry_delay_micros: 1_000,
                                            },
                                            local_time_enabled: true,
                                            short_status: Some("one-tick-runtime".to_string()),
                                        },
                                        now: TimestampMicros(11_000),
                                        stop_requested: false,
                                        retry_attempts_used: 0,
                                    },
                                },
                            },
                        },
                        timer_wait: ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                sleep_micros: 1_000,
                                wake_at: TimestampMicros(12_000),
                            },
                        },
                        retry_execution:
                            ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution,
                        reconnect_execution:
                            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution,
                    },
                    wakeup:
                        ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan::WakeupDuringTimerWait {
                            sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                                sleep_micros: 1_000,
                                wake_at: TimestampMicros(12_000),
                            },
                        },
                },
            },
        );

        let ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult::ContinueWithWakeupExecutionApplied { output } =
            result
        else {
            panic!("continue-with-wakeup should remain explicit through wakeup execution");
        };

        assert_eq!(
            output.wakeup_apply,
            ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupApplyResult::WakeupApplied {
                wakeup:
                    ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan::WakeupDuringTimerWait {
                        sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                            sleep_micros: 1_000,
                            wake_at: TimestampMicros(12_000),
                        },
                    },
            }
        );
        assert_eq!(
            output.output.timer_wait,
            ClientHeartbeatLoopFutureActualTimerWaitAction::TimerWait {
                sleep: ClientHeartbeatLoopSleepDecision::Sleep {
                    reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                    sleep_micros: 1_000,
                    wake_at: TimestampMicros(12_000),
                },
            }
        );
        assert_eq!(
            output.output.retry_execution,
            ClientHeartbeatLoopFutureActualRetryExecutionAction::NoRetryExecution
        );
        assert_eq!(
            output.output.reconnect_execution,
            ClientHeartbeatLoopFutureActualReconnectExecutionAction::NoReconnectExecution
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_ordering_input_converts_stop_path() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };
        let handoff = ClientHeartbeatLoopActualWhileLoopStopHandoff {
            reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
            },
            trigger: trigger.clone(),
        };

        let input = ClientHeartbeatLoopCleanupOrderingInput::from_responsibility(
            ClientHeartbeatLoopCleanupResponsibilityResult::Cleanup {
                input: ClientHeartbeatLoopCleanupResponsibilityInput {
                    handoff: handoff.clone(),
                    plan: ClientHeartbeatLoopCleanupPlan::CleanupOnStop {
                        trigger: trigger.clone(),
                    },
                },
            },
        )
        .expect("stop path should produce cleanup ordering input");

        assert_eq!(
            input,
            ClientHeartbeatLoopCleanupOrderingInput {
                handoff,
                plan: ClientHeartbeatLoopCleanupPlan::CleanupOnStop { trigger },
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_ordering_skips_continue_path() {
        let carry = ClientHeartbeatLoopRepeatedInvocationNextStepCarry::ContinueImmediately {
            carry: ClientHeartbeatLoopIterationCarryState {
                ordering: ClientHeartbeatLoopStepOrdering::ContinueImmediately,
                final_counters: ClientHeartbeatLoopCountersState::default(),
                next_runtime_input: ClientHeartbeatLoopCompletedStepRuntimeInput {
                    continue_requested: true,
                    body: ClientHeartbeatLoopRepeatedRuntimeBodyInput {
                        handoff: ClientHeartbeatLoopRepeatedRuntimeHandoff {
                            mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                            destination: "127.0.0.1:5000".parse().unwrap(),
                            client_id: ClientId("client-1".to_string()),
                            run_id: RunId("run-1".to_string()),
                            protocol_version: ProtocolVersion(2),
                            cadence: ClientHeartbeatLoopCadenceInput {
                                heartbeat_interval_micros: 1_000,
                                ack_receive_timeout_micros: 500,
                                ack_observation_return:
                                    ClientHeartbeatAckObservationReturnMode::Disabled,
                            },
                            stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                            max_ack_socket_wait_micros: 500,
                            max_sleep_micros: 250,
                            retry_policy: ClientHeartbeatLoopRetryPolicy {
                                max_attempts: 3,
                                retry_delay_micros: 1_000,
                            },
                            local_time_enabled: true,
                            short_status: Some("one-tick-runtime".to_string()),
                        },
                        now: TimestampMicros(11_000),
                        stop_requested: false,
                        retry_attempts_used: 0,
                    },
                },
            },
        };

        let result = ClientHeartbeatLoopCleanupOrderingBoundary.plan_next(
            ClientHeartbeatLoopCleanupResponsibilityResult::Continue {
                carry: carry.clone(),
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCleanupOrderingResult::Continue { carry }
        );
    }

    #[test]
    fn client_heartbeat_loop_cleanup_ordering_preserves_stop_only_semantics() {
        let trigger = ClientHeartbeatLoopCleanupTrigger {
            handoff: ClientHeartbeatLoopWhileLoopStopHandoff {
                stop: ClientHeartbeatLoopCompletedBodyStopResult {
                    stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    cleanup: ClientHeartbeatLoopCleanupSequencingResult::BeginCleanup {
                        stop_reason: ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                    },
                },
                final_counters: ClientHeartbeatLoopCountersState::default(),
            },
        };

        let result = ClientHeartbeatLoopCleanupOrderingBoundary.plan_next(
            ClientHeartbeatLoopCleanupResponsibilityResult::Cleanup {
                input: ClientHeartbeatLoopCleanupResponsibilityInput {
                    handoff: ClientHeartbeatLoopActualWhileLoopStopHandoff {
                        reason: ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                        trigger: trigger.clone(),
                    },
                    plan: ClientHeartbeatLoopCleanupPlan::CleanupOnStop {
                        trigger: trigger.clone(),
                    },
                },
            },
        );

        assert_eq!(
            result,
            ClientHeartbeatLoopCleanupOrderingResult::Ordered {
                handoff: ClientHeartbeatLoopCleanupOrderingHandoff {
                    stop_reason:
                        ClientHeartbeatLoopRepeatedInvocationStopReason::CleanupRequested {
                            stop_reason:
                                ClientHeartbeatLoopLifecycleStopReason::CallerRequestedStop,
                        },
                    ordered_plan: ClientHeartbeatLoopOrderedCleanupPlan::CleanupOnStop { trigger },
                },
            }
        );
    }

    #[test]
    fn client_heartbeat_ack_receive_timeout_clamps_to_max_socket_wait() {
        let decision = ClientHeartbeatAckReceiveTimeoutBoundary.plan_wait(
            ClientHeartbeatAckReceiveTimeoutInput {
                now: TimestampMicros(10_000),
                ack_deadline_at: TimestampMicros(15_000),
                max_socket_wait_micros: 1_000,
            },
        );

        assert_eq!(
            decision,
            ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                receive_timeout_micros: 1_000,
                deadline_at: TimestampMicros(15_000),
            }
        );
    }

    #[test]
    fn client_heartbeat_retry_boundary_gives_up_when_attempt_budget_is_used() {
        let decision = ClientHeartbeatLoopRetryBoundary.decide(ClientHeartbeatLoopRetryInput {
            reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
            attempts_used: 3,
            policy: ClientHeartbeatLoopRetryPolicy {
                max_attempts: 3,
                retry_delay_micros: 500,
            },
            now: TimestampMicros(10_000),
        });

        assert_eq!(
            decision,
            ClientHeartbeatLoopRetryDecision::GiveUp {
                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                attempts_used: 3,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_body_emits_send_handoff_when_heartbeat_is_due() {
        let input = ClientHeartbeatLoopBodyInput {
            ownership: ClientHeartbeatLoopOwnershipInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                auth_accepted: true,
                socket_bound: true,
            },
            policy: ClientHeartbeatLoopPolicyInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                now: TimestampMicros(10_000),
                cadence: ClientHeartbeatLoopCadenceInput {
                    heartbeat_interval_micros: 1_000,
                    ack_receive_timeout_micros: 2_000,
                    ack_observation_return:
                        ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck,
                },
                stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                state: ClientHeartbeatLoopStateSnapshot {
                    sent_heartbeats: 0,
                    received_acks: 0,
                    missed_acks: 0,
                    last_heartbeat_sent_at: None,
                    stop_requested: false,
                },
            },
            max_ack_socket_wait_micros: 500,
        };

        let result = ClientHeartbeatLoopBodyBoundary::default().run_one(input);

        let ClientHeartbeatLoopBodyResult::SendHeartbeat { handoff, log } = result else {
            panic!("heartbeat body should emit send handoff");
        };
        assert_eq!(handoff.client_id, ClientId("client-1".to_string()));
        assert_eq!(handoff.run_id, RunId("run-1".to_string()));
        assert_eq!(handoff.protocol_version, ProtocolVersion(2));
        assert_eq!(handoff.send_at, TimestampMicros(10_000));
        assert_eq!(handoff.ack_deadline_at, TimestampMicros(12_000));
        assert_eq!(
            handoff.ack_wait,
            ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                receive_timeout_micros: 500,
                deadline_at: TimestampMicros(12_000),
            }
        );
        assert_eq!(
            handoff.ack_observation_return,
            ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck
        );
        assert_eq!(log.reason, ClientHeartbeatLoopPolicyReason::HeartbeatDue);
    }

    #[test]
    fn client_heartbeat_loop_body_stops_when_auth_precondition_is_missing() {
        let input = ClientHeartbeatLoopBodyInput {
            ownership: ClientHeartbeatLoopOwnershipInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                auth_accepted: false,
                socket_bound: true,
            },
            policy: ClientHeartbeatLoopPolicyInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                now: TimestampMicros(10_000),
                cadence: ClientHeartbeatLoopCadenceInput {
                    heartbeat_interval_micros: 1_000,
                    ack_receive_timeout_micros: 2_000,
                    ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
                },
                stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                state: ClientHeartbeatLoopStateSnapshot {
                    sent_heartbeats: 0,
                    received_acks: 0,
                    missed_acks: 0,
                    last_heartbeat_sent_at: None,
                    stop_requested: false,
                },
            },
            max_ack_socket_wait_micros: 500,
        };

        let result = ClientHeartbeatLoopBodyBoundary::default().run_one(input);

        let ClientHeartbeatLoopBodyResult::OwnershipNotReady(
            ClientHeartbeatLoopOwnershipDecision::NotReady { reason, .. },
        ) = result
        else {
            panic!("missing accepted auth should stop before body work");
        };
        assert_eq!(
            reason,
            ClientHeartbeatLoopOwnershipNotReadyReason::AuthNotAccepted
        );
    }

    #[test]
    fn client_heartbeat_loop_encode_send_boundary_encodes_body_handoff() {
        let input = ClientHeartbeatLoopEncodeSendInput {
            destination: "127.0.0.1:5000".parse().unwrap(),
            handoff: ClientHeartbeatLoopBodySendHandoff {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                send_at: TimestampMicros(10_000),
                ack_deadline_at: TimestampMicros(12_000),
                ack_wait: ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                    receive_timeout_micros: 500,
                    deadline_at: TimestampMicros(12_000),
                },
                ack_observation_return:
                    ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck,
            },
            local_time: Some(TimestampMicros(10_000)),
            short_status: Some("loop-once".to_string()),
        };

        let encoded = ClientHeartbeatLoopEncodeSendBoundary::default()
            .encode_handoff(input)
            .expect("heartbeat should encode");

        let packet = decode_fixed_header(&encoded.encoded_bytes)
            .expect("encoded heartbeat fixed header should decode");
        let heartbeat = decode_heartbeat_payload(packet.header, packet.payload)
            .expect("encoded heartbeat payload should decode");
        assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
        assert_eq!(heartbeat.run_id, RunId("run-1".to_string()));
        assert_eq!(heartbeat.sent_at, TimestampMicros(10_000));
        assert_eq!(heartbeat.local_time, Some(TimestampMicros(10_000)));
        assert_eq!(heartbeat.short_status, Some("loop-once".to_string()));
        assert_eq!(encoded.ack_deadline_at, TimestampMicros(12_000));
        assert_eq!(
            encoded.ack_observation_return,
            ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck
        );
    }

    #[test]
    fn client_heartbeat_loop_encode_send_boundary_sends_one_udp_datagram() {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("read timeout should be set");
        let destination = receiver.local_addr().expect("local addr should exist");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        let input = ClientHeartbeatLoopEncodeSendInput {
            destination,
            handoff: ClientHeartbeatLoopBodySendHandoff {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(2),
                send_at: TimestampMicros(10_000),
                ack_deadline_at: TimestampMicros(12_000),
                ack_wait: ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                    receive_timeout_micros: 500,
                    deadline_at: TimestampMicros(12_000),
                },
                ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
            },
            local_time: None,
            short_status: None,
        };

        let result = ClientHeartbeatLoopEncodeSendBoundary::default()
            .send_one(&sender, input)
            .expect("heartbeat should send once");

        let mut buffer = [0_u8; 1024];
        let (received_len, _) = receiver
            .recv_from(&mut buffer)
            .expect("receiver should get heartbeat datagram");
        assert_eq!(result.bytes_sent, received_len);
        let packet =
            decode_fixed_header(&buffer[..received_len]).expect("sent heartbeat should decode");
        let heartbeat = decode_heartbeat_payload(packet.header, packet.payload)
            .expect("sent heartbeat payload should decode");
        assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
        assert_eq!(heartbeat.run_id, RunId("run-1".to_string()));
        assert_eq!(heartbeat.sent_at, TimestampMicros(10_000));
    }

    #[test]
    fn client_heartbeat_loop_ack_return_boundary_builds_client_stats_handoff() {
        let destination = "127.0.0.1:5000".parse().unwrap();
        let sent = ClientHeartbeatLoopEncodedSendHandoff {
            destination,
            heartbeat: Heartbeat {
                message_type: MessageType::Heartbeat,
                protocol_version: ProtocolVersion(2),
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                sent_at: TimestampMicros(10_000),
                local_time: Some(TimestampMicros(10_000)),
                short_status: None,
            },
            encoded_bytes: Vec::new(),
            ack_deadline_at: TimestampMicros(12_000),
            ack_wait: ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                receive_timeout_micros: 500,
                deadline_at: TimestampMicros(12_000),
            },
            ack_observation_return: ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck,
        };
        let ack = HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(10_000),
            server_received_at: TimestampMicros(10_500),
            server_sent_at: TimestampMicros(10_600),
        };

        let result = ClientHeartbeatLoopAckObservationReturnBoundary::default()
            .prepare_return(ClientHeartbeatLoopAckObservationReturnInput {
                sent,
                ack_source: destination,
                ack_bytes: vec![1, 2, 3],
                ack,
                client_received_at: TimestampMicros(10_900),
                client_stats_sent_at: TimestampMicros(10_950),
            })
            .expect("ack observation return should be prepared");

        assert_eq!(
            result.observation.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(result.observation.run_id, RunId("run-1".to_string()));
        assert_eq!(result.observation.echoed_sent_at, TimestampMicros(10_000));
        assert_eq!(
            result.observation.client_received_at,
            TimestampMicros(10_900)
        );
        let stats_return = result
            .client_stats_return
            .expect("client stats return should be prepared");
        assert_eq!(stats_return.destination, destination);
        assert_eq!(stats_return.client_stats.sent_at, TimestampMicros(10_950));
        assert_eq!(
            stats_return.client_stats.heartbeat_observation,
            Some(result.observation)
        );
        let packet = decode_fixed_header(&stats_return.encoded_bytes)
            .expect("encoded client stats fixed header should decode");
        let decoded_stats = decode_client_stats_payload(packet.header, packet.payload)
            .expect("encoded client stats should decode");
        assert_eq!(decoded_stats, stats_return.client_stats);
    }

    #[test]
    fn client_heartbeat_loop_ack_return_boundary_receives_ack_once() {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("receiver read timeout should be set");
        let receiver_addr = receiver.local_addr().expect("receiver addr should exist");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        let sender_addr = sender.local_addr().expect("sender addr should exist");
        let ack = HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(10_000),
            server_received_at: TimestampMicros(10_500),
            server_sent_at: TimestampMicros(10_600),
        };
        let mut ack_bytes = Vec::new();
        ProtocolMessageEncoderBoundary
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &ProtocolMessage::HeartbeatAck(ack),
                &mut ack_bytes,
            )
            .expect("ack should encode");
        sender
            .send_to(&ack_bytes, receiver_addr)
            .expect("ack should send to receiver");
        let sent = ClientHeartbeatLoopEncodedSendHandoff {
            destination: sender_addr,
            heartbeat: Heartbeat {
                message_type: MessageType::Heartbeat,
                protocol_version: ProtocolVersion(2),
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                sent_at: TimestampMicros(10_000),
                local_time: None,
                short_status: None,
            },
            encoded_bytes: Vec::new(),
            ack_deadline_at: TimestampMicros(12_000),
            ack_wait: ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                receive_timeout_micros: 500,
                deadline_at: TimestampMicros(12_000),
            },
            ack_observation_return: ClientHeartbeatAckObservationReturnMode::Disabled,
        };

        let result = ClientHeartbeatLoopAckObservationReturnBoundary::default()
            .receive_one(&receiver, sent)
            .expect("ack should be received and observed");

        assert_eq!(result.ack_source, sender_addr);
        assert_eq!(result.ack.echoed_sent_at, TimestampMicros(10_000));
        assert_eq!(
            result.observation.client_id,
            ClientId("client-1".to_string())
        );
        assert!(result.client_stats_return.is_none());
    }

    #[test]
    fn client_heartbeat_loop_client_stats_return_send_boundary_sends_one_datagram() {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("receiver read timeout should be set");
        let destination = receiver.local_addr().expect("receiver addr should exist");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        let observation = HeartbeatAckObservation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(10_000),
            server_received_at: TimestampMicros(10_500),
            server_sent_at: TimestampMicros(10_600),
            client_received_at: TimestampMicros(10_900),
        };
        let client_stats = ClientStats {
            message_type: MessageType::ClientStats,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            sent_at: TimestampMicros(10_950),
            capture_fps: 0,
            dropped_frames: 0,
            bitrate_kbps: 0,
            heartbeat_observation: Some(observation),
        };
        let mut encoded_bytes = Vec::new();
        ProtocolMessageEncoderBoundary
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &ProtocolMessage::ClientStats(client_stats.clone()),
                &mut encoded_bytes,
            )
            .expect("client stats should encode");
        let handoff = ClientHeartbeatLoopClientStatsReturnHandoff {
            destination,
            client_stats: client_stats.clone(),
            encoded_bytes,
        };

        let result = ClientHeartbeatLoopClientStatsReturnSendBoundary
            .send_one(&sender, handoff)
            .expect("client stats return should send once");

        let mut buffer = [0_u8; 2048];
        let (received_len, _) = receiver
            .recv_from(&mut buffer)
            .expect("receiver should get client stats datagram");
        assert_eq!(result.bytes_sent, received_len);
        assert_eq!(result.handoff.client_stats, client_stats);
        let packet =
            decode_fixed_header(&buffer[..received_len]).expect("client stats should decode");
        let decoded_stats = decode_client_stats_payload(packet.header, packet.payload)
            .expect("client stats payload should decode");
        assert_eq!(decoded_stats, client_stats);
    }

    #[test]
    fn client_heartbeat_loop_counters_boundary_updates_send_ack_and_stats_return() {
        let mut state = ClientHeartbeatLoopCountersState::default();
        let boundary = ClientHeartbeatLoopCountersBoundary;

        let send_outcome = boundary.commit_result(
            &mut state,
            ClientHeartbeatLoopIterationRuntimeResult::HeartbeatSent {
                sent_at: TimestampMicros(10_000),
            },
        );
        assert_eq!(
            send_outcome.previous,
            ClientHeartbeatLoopCountersState::default()
        );
        assert_eq!(send_outcome.current.sent_heartbeats, 1);
        assert_eq!(
            send_outcome.current.last_heartbeat_sent_at,
            Some(TimestampMicros(10_000))
        );

        boundary.commit_result(
            &mut state,
            ClientHeartbeatLoopIterationRuntimeResult::AckReceived {
                client_received_at: TimestampMicros(10_900),
                stats_return_prepared: true,
            },
        );
        boundary.commit_result(
            &mut state,
            ClientHeartbeatLoopIterationRuntimeResult::ClientStatsReturnSent {
                sent_at: TimestampMicros(10_950),
            },
        );

        assert_eq!(state.sent_heartbeats, 1);
        assert_eq!(state.received_acks, 1);
        assert_eq!(state.missed_acks, 0);
        assert_eq!(state.stats_returns_sent, 1);
        assert_eq!(state.last_ack_received_at, Some(TimestampMicros(10_900)));
        assert_eq!(
            state.last_stats_return_sent_at,
            Some(TimestampMicros(10_950))
        );
    }

    #[test]
    fn client_heartbeat_loop_counters_boundary_tracks_missed_ack_and_failures() {
        let mut state = ClientHeartbeatLoopCountersState::default();
        let boundary = ClientHeartbeatLoopCountersBoundary;

        boundary.commit_result(
            &mut state,
            ClientHeartbeatLoopIterationRuntimeResult::AckMissed {
                missed_at: TimestampMicros(12_000),
            },
        );
        boundary.commit_result(
            &mut state,
            ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::HeartbeatSend,
                failed_at: TimestampMicros(13_000),
            },
        );
        boundary.commit_result(
            &mut state,
            ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                failed_at: TimestampMicros(14_000),
            },
        );
        boundary.commit_result(
            &mut state,
            ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::ClientStatsReturnSend,
                failed_at: TimestampMicros(15_000),
            },
        );

        assert_eq!(state.missed_acks, 1);
        assert_eq!(state.heartbeat_send_failures, 1);
        assert_eq!(state.ack_receive_failures, 1);
        assert_eq!(state.stats_return_send_failures, 1);
        assert_eq!(state.sent_heartbeats, 0);
        assert_eq!(state.received_acks, 0);
        assert_eq!(state.stats_returns_sent, 0);
    }

    #[test]
    fn client_heartbeat_loop_counters_state_exports_policy_snapshot() {
        let state = ClientHeartbeatLoopCountersState {
            sent_heartbeats: 3,
            received_acks: 2,
            missed_acks: 1,
            stats_returns_sent: 2,
            heartbeat_send_failures: 0,
            ack_receive_failures: 1,
            stats_return_send_failures: 0,
            last_heartbeat_sent_at: Some(TimestampMicros(30_000)),
            last_ack_received_at: Some(TimestampMicros(30_500)),
            last_stats_return_sent_at: Some(TimestampMicros(30_550)),
        };

        assert_eq!(
            state.as_policy_snapshot(true),
            ClientHeartbeatLoopStateSnapshot {
                sent_heartbeats: 3,
                received_acks: 2,
                missed_acks: 1,
                last_heartbeat_sent_at: Some(TimestampMicros(30_000)),
                stop_requested: true,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_sleep_boundary_clamps_sleep_duration() {
        let decision = ClientHeartbeatLoopSleepBoundary.plan_sleep(ClientHeartbeatLoopSleepInput {
            now: TimestampMicros(10_000),
            wake_at: TimestampMicros(15_000),
            max_sleep_micros: 1_000,
            reason: ClientHeartbeatLoopSleepReason::CadenceWait,
        });

        assert_eq!(
            decision,
            ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                sleep_micros: 1_000,
                wake_at: TimestampMicros(15_000),
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_retry_apply_plans_failure_result_and_retry_sleep() {
        let result = ClientHeartbeatLoopRetryApplyBoundary::default().apply_failure(
            ClientHeartbeatLoopRetryApplyInput {
                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                failure_kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                attempts_used: 1,
                policy: ClientHeartbeatLoopRetryPolicy {
                    max_attempts: 3,
                    retry_delay_micros: 5_000,
                },
                failed_at: TimestampMicros(10_000),
                max_sleep_micros: 1_000,
            },
        );

        assert_eq!(
            result.failure_result,
            ClientHeartbeatLoopIterationRuntimeResult::Failed {
                kind: ClientHeartbeatLoopIterationFailureKind::AckReceive,
                failed_at: TimestampMicros(10_000),
            }
        );
        assert_eq!(
            result.retry_decision,
            ClientHeartbeatLoopRetryDecision::RetryLater {
                reason: ClientHeartbeatLoopRetryReason::AckReceiveTimeout,
                next_attempt: 2,
                retry_at: TimestampMicros(15_000),
            }
        );
        assert_eq!(
            result.sleep,
            ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::RetryBackoff,
                sleep_micros: 1_000,
                wake_at: TimestampMicros(15_000),
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_retry_apply_does_not_sleep_when_retry_exhausted() {
        let result = ClientHeartbeatLoopRetryApplyBoundary::default().apply_failure(
            ClientHeartbeatLoopRetryApplyInput {
                reason: ClientHeartbeatLoopRetryReason::StatsReturnSendFailed,
                failure_kind: ClientHeartbeatLoopIterationFailureKind::ClientStatsReturnSend,
                attempts_used: 3,
                policy: ClientHeartbeatLoopRetryPolicy {
                    max_attempts: 3,
                    retry_delay_micros: 5_000,
                },
                failed_at: TimestampMicros(10_000),
                max_sleep_micros: 1_000,
            },
        );

        assert_eq!(
            result.retry_decision,
            ClientHeartbeatLoopRetryDecision::GiveUp {
                reason: ClientHeartbeatLoopRetryReason::StatsReturnSendFailed,
                attempts_used: 3,
            }
        );
        assert_eq!(
            result.sleep,
            ClientHeartbeatLoopSleepDecision::NoSleep {
                reason: ClientHeartbeatLoopSleepReason::RetryExhausted,
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_controller_turns_wait_into_sleep_and_iteration_result() {
        let log = ClientHeartbeatLoopLogHandoff {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            observed_at: TimestampMicros(10_000),
            reason: ClientHeartbeatLoopPolicyReason::WaitingForCadence,
            heartbeat_interval_micros: 1_000,
            ack_receive_timeout_micros: 2_000,
            sent_heartbeats: 1,
            received_acks: 1,
            missed_acks: 0,
        };

        let plan = ClientHeartbeatLoopControllerBoundary::default().plan_next(
            ClientHeartbeatLoopControllerInput {
                body_result: ClientHeartbeatLoopBodyResult::Wait {
                    next_heartbeat_due_at: TimestampMicros(11_000),
                    log,
                },
                now: TimestampMicros(10_000),
                max_sleep_micros: 500,
            },
        );

        let ClientHeartbeatLoopControllerPlan::Sleep {
            sleep,
            iteration_result,
            ..
        } = plan
        else {
            panic!("wait body result should become a controller sleep plan");
        };
        assert_eq!(
            sleep,
            ClientHeartbeatLoopSleepDecision::Sleep {
                reason: ClientHeartbeatLoopSleepReason::CadenceWait,
                sleep_micros: 500,
                wake_at: TimestampMicros(11_000),
            }
        );
        assert_eq!(
            iteration_result,
            ClientHeartbeatLoopIterationRuntimeResult::Waited {
                next_heartbeat_due_at: TimestampMicros(11_000),
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_controller_result_marks_stop_for_shutdown_and_logging() {
        let log = ClientHeartbeatLoopLogHandoff {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            observed_at: TimestampMicros(10_000),
            reason: ClientHeartbeatLoopPolicyReason::StopRequested,
            heartbeat_interval_micros: 1_000,
            ack_receive_timeout_micros: 2_000,
            sent_heartbeats: 2,
            received_acks: 2,
            missed_acks: 0,
        };
        let plan = ClientHeartbeatLoopControllerPlan::Stop {
            reason: ClientHeartbeatLoopPolicyReason::StopRequested,
            log: log.clone(),
            iteration_result: ClientHeartbeatLoopIterationRuntimeResult::Stopped {
                reason: ClientHeartbeatLoopPolicyReason::StopRequested,
            },
        };

        let result = ClientHeartbeatLoopControllerResultBoundary::default().finalize(plan);

        assert_eq!(result.action, ClientHeartbeatLoopControllerAction::Stop);
        assert_eq!(
            result.shutdown,
            ClientHeartbeatLoopShutdownDecision::Stop {
                reason: ClientHeartbeatLoopPolicyReason::StopRequested
            }
        );
        assert_eq!(
            result.iteration_result,
            Some(ClientHeartbeatLoopIterationRuntimeResult::Stopped {
                reason: ClientHeartbeatLoopPolicyReason::StopRequested,
            })
        );
        let log_handoff = result.log.expect("stop should produce log handoff");
        assert_eq!(
            log_handoff.action,
            ClientHeartbeatLoopControllerAction::Stop
        );
        assert_eq!(log_handoff.policy_log, log);
        assert_eq!(
            log_handoff.iteration_result,
            Some(ClientHeartbeatLoopIterationRuntimeResult::Stopped {
                reason: ClientHeartbeatLoopPolicyReason::StopRequested,
            })
        );
    }

    #[test]
    fn client_heartbeat_loop_controller_result_keeps_send_as_continue_handoff() {
        let log = ClientHeartbeatLoopLogHandoff {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            observed_at: TimestampMicros(10_000),
            reason: ClientHeartbeatLoopPolicyReason::HeartbeatDue,
            heartbeat_interval_micros: 1_000,
            ack_receive_timeout_micros: 2_000,
            sent_heartbeats: 1,
            received_acks: 1,
            missed_acks: 0,
        };
        let handoff = ClientHeartbeatLoopBodySendHandoff {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(2),
            send_at: TimestampMicros(10_000),
            ack_deadline_at: TimestampMicros(12_000),
            ack_wait: ClientHeartbeatAckReceiveTimeoutDecision::Wait {
                receive_timeout_micros: 500,
                deadline_at: TimestampMicros(12_000),
            },
            ack_observation_return: ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck,
        };
        let plan = ClientHeartbeatLoopControllerPlan::SendHeartbeat {
            handoff,
            log: log.clone(),
        };

        let result = ClientHeartbeatLoopControllerResultBoundary::default().finalize(plan);

        assert_eq!(
            result.action,
            ClientHeartbeatLoopControllerAction::SendHeartbeat
        );
        assert_eq!(
            result.shutdown,
            ClientHeartbeatLoopShutdownDecision::Continue
        );
        assert_eq!(result.iteration_result, None);
        let log_handoff = result
            .log
            .expect("send heartbeat should produce log handoff");
        assert_eq!(
            log_handoff.action,
            ClientHeartbeatLoopControllerAction::SendHeartbeat
        );
        assert_eq!(log_handoff.policy_log, log);
        assert_eq!(log_handoff.iteration_result, None);
    }

    #[test]
    fn client_heartbeat_loop_controller_result_does_not_log_ownership_not_ready() {
        let plan = ClientHeartbeatLoopControllerPlan::OwnershipNotReady {
            decision: ClientHeartbeatLoopOwnershipDecision::NotReady {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                reason: ClientHeartbeatLoopOwnershipNotReadyReason::AuthNotAccepted,
            },
        };

        let result = ClientHeartbeatLoopControllerResultBoundary::default().finalize(plan);

        assert_eq!(
            result.action,
            ClientHeartbeatLoopControllerAction::OwnershipNotReady
        );
        assert_eq!(
            result.shutdown,
            ClientHeartbeatLoopShutdownDecision::Continue
        );
        assert_eq!(result.log, None);
        assert_eq!(result.iteration_result, None);
    }

    #[test]
    fn client_heartbeat_loop_one_tick_runtime_commits_wait_without_sleeping() {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("socket should bind");
        let destination = "127.0.0.1:5000".parse().unwrap();
        let mut counters = ClientHeartbeatLoopCountersState {
            last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
            ..ClientHeartbeatLoopCountersState::default()
        };
        let policy_snapshot = counters.as_policy_snapshot(false);

        let result = ClientHeartbeatLoopOneTickRuntimeBoundary::default().run_one(
            &socket,
            &mut counters,
            ClientHeartbeatLoopOneTickRuntimeInput {
                destination,
                body: ClientHeartbeatLoopBodyInput {
                    ownership: ClientHeartbeatLoopOwnershipInput {
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        protocol_version: ProtocolVersion(2),
                        auth_accepted: true,
                        socket_bound: true,
                    },
                    policy: ClientHeartbeatLoopPolicyInput {
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        now: TimestampMicros(10_500),
                        cadence: ClientHeartbeatLoopCadenceInput {
                            heartbeat_interval_micros: 1_000,
                            ack_receive_timeout_micros: 2_000,
                            ack_observation_return:
                                ClientHeartbeatAckObservationReturnMode::Disabled,
                        },
                        stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                        state: policy_snapshot,
                    },
                    max_ack_socket_wait_micros: 500,
                },
                local_time: None,
                short_status: None,
                controller_now: TimestampMicros(10_500),
                max_sleep_micros: 250,
                retry_policy: ClientHeartbeatLoopRetryPolicy {
                    max_attempts: 3,
                    retry_delay_micros: 1_000,
                },
                retry_attempts_used: 0,
            },
        );

        assert_eq!(
            result.controller.action,
            ClientHeartbeatLoopControllerAction::Sleep
        );
        assert_eq!(result.heartbeat_send, None);
        assert_eq!(result.ack_return, None);
        assert_eq!(result.stats_return_send, None);
        assert_eq!(result.retry, None);
        assert_eq!(result.counters_updates.len(), 1);
        assert_eq!(
            result.counters_updates[0].current,
            ClientHeartbeatLoopCountersState {
                last_heartbeat_sent_at: Some(TimestampMicros(10_000)),
                ..ClientHeartbeatLoopCountersState::default()
            }
        );
    }

    #[test]
    fn client_heartbeat_loop_one_tick_runtime_sends_ack_returns_stats_and_updates_counters() {
        let client_socket = UdpSocket::bind("127.0.0.1:0").expect("client socket should bind");
        client_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("client read timeout should be set");
        let client_addr = client_socket
            .local_addr()
            .expect("client socket addr should exist");
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        server_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("server read timeout should be set");
        let server_addr = server_socket
            .local_addr()
            .expect("server socket addr should exist");
        let ack_sender = UdpSocket::bind("127.0.0.1:0").expect("ack sender should bind");
        let ack = HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(10_000),
            server_received_at: TimestampMicros(10_100),
            server_sent_at: TimestampMicros(10_200),
        };
        let mut ack_bytes = Vec::new();
        ProtocolMessageEncoderBoundary
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &ProtocolMessage::HeartbeatAck(ack),
                &mut ack_bytes,
            )
            .expect("ack should encode");
        ack_sender
            .send_to(&ack_bytes, client_addr)
            .expect("ack should be queued for client");

        let mut counters = ClientHeartbeatLoopCountersState::default();
        let policy_snapshot = counters.as_policy_snapshot(false);
        let result = ClientHeartbeatLoopOneTickRuntimeBoundary::default().run_one(
            &client_socket,
            &mut counters,
            ClientHeartbeatLoopOneTickRuntimeInput {
                destination: server_addr,
                body: ClientHeartbeatLoopBodyInput {
                    ownership: ClientHeartbeatLoopOwnershipInput {
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        protocol_version: ProtocolVersion(2),
                        auth_accepted: true,
                        socket_bound: true,
                    },
                    policy: ClientHeartbeatLoopPolicyInput {
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        now: TimestampMicros(10_000),
                        cadence: ClientHeartbeatLoopCadenceInput {
                            heartbeat_interval_micros: 1_000,
                            ack_receive_timeout_micros: 2_000,
                            ack_observation_return:
                                ClientHeartbeatAckObservationReturnMode::ClientStatsOncePerAck,
                        },
                        stop_condition: ClientHeartbeatLoopStopCondition::RunUntilStopped,
                        state: policy_snapshot,
                    },
                    max_ack_socket_wait_micros: 500,
                },
                local_time: Some(TimestampMicros(10_000)),
                short_status: Some("one-tick".to_string()),
                controller_now: TimestampMicros(10_000),
                max_sleep_micros: 250,
                retry_policy: ClientHeartbeatLoopRetryPolicy {
                    max_attempts: 3,
                    retry_delay_micros: 1_000,
                },
                retry_attempts_used: 0,
            },
        );

        assert_eq!(
            result.controller.action,
            ClientHeartbeatLoopControllerAction::SendHeartbeat
        );
        assert!(result.heartbeat_send.is_some());
        assert!(result.ack_return.is_some());
        assert!(result.stats_return_send.is_some());
        assert_eq!(result.retry, None);
        assert_eq!(result.failure, None);
        assert_eq!(result.final_counters.sent_heartbeats, 1);
        assert_eq!(result.final_counters.received_acks, 1);
        assert_eq!(result.final_counters.stats_returns_sent, 1);
        assert_eq!(result.final_counters.missed_acks, 0);
        assert_eq!(result.counters_updates.len(), 3);

        let mut saw_heartbeat = false;
        let mut saw_stats = false;
        for _ in 0..2 {
            let mut buffer = [0_u8; 2048];
            let (received_len, _) = server_socket
                .recv_from(&mut buffer)
                .expect("server should receive heartbeat and stats");
            let packet = decode_fixed_header(&buffer[..received_len])
                .expect("received packet should decode fixed header");
            match packet.header.message_type {
                MessageType::Heartbeat => {
                    let heartbeat = decode_heartbeat_payload(packet.header, packet.payload)
                        .expect("heartbeat should decode");
                    assert_eq!(heartbeat.sent_at, TimestampMicros(10_000));
                    assert_eq!(heartbeat.short_status, Some("one-tick".to_string()));
                    saw_heartbeat = true;
                }
                MessageType::ClientStats => {
                    let stats = decode_client_stats_payload(packet.header, packet.payload)
                        .expect("client stats should decode");
                    assert!(stats.heartbeat_observation.is_some());
                    saw_stats = true;
                }
                other => panic!("unexpected packet type received by server: {other:?}"),
            }
        }
        assert!(saw_heartbeat);
        assert!(saw_stats);
    }

    #[test]
    fn client_heartbeat_one_tick_runtime_launcher_runs_auth_and_one_heartbeat_tick() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").expect("server socket should bind");
        server_socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("server read timeout should be set");
        let destination = server_socket
            .local_addr()
            .expect("server local addr should exist");

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
        let request_for_thread = request.clone();
        let server = std::thread::spawn(move || {
            let encoder = ProtocolMessageEncoderBoundary;
            let mut auth_buffer = [0_u8; 1024];
            let (auth_len, auth_source) = server_socket
                .recv_from(&mut auth_buffer)
                .expect("server should receive auth request");
            let auth_packet = decode_fixed_header(&auth_buffer[..auth_len])
                .expect("auth request should decode fixed header");
            let auth_request = decode_auth_request_payload(auth_packet.header, auth_packet.payload)
                .expect("auth request should decode");
            assert_eq!(auth_request.client_id, request_for_thread.client_id);
            assert_eq!(auth_request.run_id, request_for_thread.run_id);

            let auth_response = AuthResponse {
                message_type: MessageType::AuthResponse,
                protocol_version: request_for_thread.protocol_version,
                client_id: request_for_thread.client_id.clone(),
                run_id: request_for_thread.run_id.clone(),
                accepted: true,
                reason_code: AuthResponseReasonCode::Ok,
                message: None,
                server_time: Some(TimestampMicros(20_000)),
                expected_protocol_version: None,
            };
            let mut auth_response_bytes = Vec::new();
            encoder
                .encode_message(
                    EncodeContext {
                        protocol_version: request_for_thread.protocol_version,
                    },
                    &ProtocolMessage::AuthResponse(auth_response),
                    &mut auth_response_bytes,
                )
                .expect("auth response should encode");
            server_socket
                .send_to(&auth_response_bytes, auth_source)
                .expect("server should send auth response");

            let mut heartbeat_buffer = [0_u8; 1024];
            let (heartbeat_len, heartbeat_source) = server_socket
                .recv_from(&mut heartbeat_buffer)
                .expect("server should receive heartbeat");
            let heartbeat_packet = decode_fixed_header(&heartbeat_buffer[..heartbeat_len])
                .expect("heartbeat should decode fixed header");
            let heartbeat =
                decode_heartbeat_payload(heartbeat_packet.header, heartbeat_packet.payload)
                    .expect("heartbeat should decode");
            assert_eq!(heartbeat.client_id, request_for_thread.client_id);
            assert_eq!(heartbeat.run_id, request_for_thread.run_id);
            assert_eq!(heartbeat.short_status, Some("one-tick-runtime".to_string()));

            let heartbeat_ack = HeartbeatAck {
                message_type: MessageType::HeartbeatAck,
                protocol_version: request_for_thread.protocol_version,
                client_id: request_for_thread.client_id.clone(),
                run_id: request_for_thread.run_id.clone(),
                echoed_sent_at: heartbeat.sent_at,
                server_received_at: TimestampMicros(30_000),
                server_sent_at: TimestampMicros(30_500),
            };
            let mut heartbeat_ack_bytes = Vec::new();
            encoder
                .encode_message(
                    EncodeContext {
                        protocol_version: request_for_thread.protocol_version,
                    },
                    &ProtocolMessage::HeartbeatAck(heartbeat_ack),
                    &mut heartbeat_ack_bytes,
                )
                .expect("heartbeat ack should encode");
            server_socket
                .send_to(&heartbeat_ack_bytes, heartbeat_source)
                .expect("server should send heartbeat ack");
        });

        let outcome = ClientHeartbeatOneTickRuntimeLauncher::default()
            .run_once(ClientHeartbeatOneTickRuntimeStartupConfig {
                mode: ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly,
                destination,
                response_timeout_ms: 1_000,
                request,
                heartbeat_interval_micros: 1_000_000,
                max_ack_socket_wait_micros: 1_000_000,
                max_sleep_micros: 1_000_000,
                retry_policy: ClientHeartbeatLoopRetryPolicy {
                    max_attempts: DEFAULT_ONE_TICK_RETRY_ATTEMPTS,
                    retry_delay_micros: 1_000_000,
                },
                short_status: Some("one-tick-runtime".to_string()),
            })
            .expect("launcher should complete one auth and heartbeat tick");
        server.join().expect("server thread should exit cleanly");

        assert_eq!(
            outcome.mode,
            ClientHeartbeatOneTickRuntimeMode::HeartbeatOnly
        );
        assert!(outcome.auth_response.accepted);
        assert_eq!(
            outcome.repeated_loop_handoff.client_id,
            ClientId("client-1".to_string())
        );
        assert_eq!(
            outcome.repeated_loop_handoff.short_status,
            Some("one-tick-runtime".to_string())
        );
        assert!(outcome.repeated_loop_handoff.local_time_enabled);
        assert_eq!(
            outcome.runtime.controller.action,
            ClientHeartbeatLoopControllerAction::SendHeartbeat
        );
        assert!(outcome.runtime.heartbeat_send.is_some());
        assert!(outcome.runtime.ack_return.is_some());
        assert_eq!(outcome.runtime.stats_return_send, None);
        assert_eq!(outcome.runtime.retry, None);
        assert_eq!(outcome.runtime.failure, None);
        assert_eq!(outcome.runtime.final_counters.sent_heartbeats, 1);
        assert_eq!(outcome.runtime.final_counters.received_acks, 1);
        assert_eq!(outcome.runtime.final_counters.stats_returns_sent, 0);
    }
}
