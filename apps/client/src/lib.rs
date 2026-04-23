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
