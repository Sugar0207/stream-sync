use std::collections::BTreeMap;
use stream_sync_config::{ServerAuthConfig, SharedTokenSecretRef};
use stream_sync_net_core::{
    DecodedInboundPacket, InboundPacket, InboundPacketDecoder, NetDecodeError, OutboundPacket,
    OutboundPacketQueueBoundary, OutboundQueueItem, PacketSource,
};
use stream_sync_protocol::{
    AppVersion, AuthRequest, AuthResponse, AuthResponseReasonCode, ClientId, Heartbeat,
    HeartbeatAck, MessageType, ProtocolError, ProtocolMessage, ProtocolVersion, RunId,
    TimestampMicros, VideoFrame,
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

/// Minimal server auth decision boundary.
///
/// This checks the prepared client whitelist and token input and returns a
/// `ServerAuthDecision`. It does not read TOML, resolve external secrets,
/// register authenticated sources, or send responses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthDecisionBoundary;

impl ServerAuthDecisionBoundary {
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
/// resolve secrets, register authenticated sources, run a queue, encode bytes,
/// or send UDP packets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthFlowStep {
    auth_handler: ServerAuthHandlerBoundary,
    config_input: ServerAuthConfigInputBoundary,
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
        let decision = self.decision.decide(auth_input);
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
    use std::net::SocketAddr;
    use stream_sync_config::{AllowedClientConfig, SharedTokenConfig};
    use stream_sync_protocol::{
        AppVersion, ClientId, Codec, ProtocolVersion, RunId, TimestampMicros, FIXED_HEADER_LEN,
        HEADER_FLAGS_OFFSET, HEADER_LENGTH_OFFSET, HEADER_MESSAGE_TYPE_OFFSET,
        HEADER_PAYLOAD_LENGTH_OFFSET, HEADER_PROTOCOL_VERSION_OFFSET, HEADER_RESERVED_OFFSET,
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
