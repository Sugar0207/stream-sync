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
}
