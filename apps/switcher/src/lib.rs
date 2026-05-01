use std::{
    io::{self, Write},
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use stream_sync_net_core::{
    PacketSource, ServerSwitcherQueuedFrameHandoffErrorCode, ServerSwitcherQueuedFrameHandoffFrame,
    ServerSwitcherQueuedFrameHandoffRequest, ServerSwitcherQueuedFrameHandoffResponse,
    ServerSwitcherQueuedFrameReadMode, DEFAULT_UDP_PACKET_BUFFER_LEN,
    SERVER_SWITCHER_HANDOFF_VERSION,
};
use stream_sync_protocol::{ClientId, Codec, MessageType, ProtocolVersion, RunId, TimestampMicros};
use stream_sync_server::{
    AuthenticatedSenderRegistry, PacketAcceptanceRejectReason, ServerAuthResponsePocError,
    ServerAuthResponsePocLauncher, ServerAuthResponsePocOutcome,
    ServerAuthResponsePocStartupConfig, ServerAuthResponsePocStartupError,
    ServerAuthResponsePocStep, ServerInboundRoute, ServerQueuedVideoFrame,
    ServerReceiveAuthVideoQueueOnceStartupOutcome, ServerReceiveAuthVideoQueueOnceVideoOutcome,
    ServerReceiveLoopGateOutcome, ServerReceiveLoopGateRejection, ServerReceiveLoopStep,
    ServerRegisteredClientPacket, ServerRegisteredPacketBoundary, ServerRegisteredVideoFramePacket,
    ServerVideoFrameHandlerBoundary, ServerVideoFrameQueuePolicy,
    ServerVideoFrameQueueReadBoundary, ServerVideoFrameQueueReadInput,
    ServerVideoFrameQueueReadMode, ServerVideoFrameQueueReadResult,
    ServerVideoFrameQueueRuntimeResult, ServerVideoFrameQueueState,
    ServerVideoFrameQueueStorageBoundary, ServerVideoFrameQueueStorageResult,
};

#[cfg(windows)]
use std::{
    fs::File,
    io::Read,
    os::windows::io::{FromRawHandle, OwnedHandle},
};
#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::Pipes::WaitNamedPipeW,
    },
};

pub const CRATE_NAME: &str = "stream-sync-switcher";

/// Input for selecting one client's latest encoded frame for single-view PoC.
///
/// The queue state is borrowed from the caller and is not mutated by this
/// selection boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherSingleViewFrameSelectionInput<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub client_id: &'a ClientId,
}

/// Encoded frame selected for a future single-view display path.
///
/// This remains encoded H.264 payload plus metadata. It is not decoded pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSingleViewSelectedEncodedFrame {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub frame_id: u64,
    pub capture_timestamp: TimestampMicros,
    pub send_timestamp: TimestampMicros,
    pub queued_at: TimestampMicros,
    pub is_keyframe: bool,
    pub width: u32,
    pub height: u32,
    pub fps_nominal: u32,
    pub codec: Codec,
    pub encoded_payload_len: usize,
    pub encoded_payload: Vec<u8>,
}

impl From<&ServerQueuedVideoFrame> for SwitcherSingleViewSelectedEncodedFrame {
    fn from(queued: &ServerQueuedVideoFrame) -> Self {
        Self {
            client_id: queued.frame.client_id.clone(),
            run_id: queued.frame.run_id.clone(),
            frame_id: queued.frame.frame_id,
            capture_timestamp: queued.frame.capture_timestamp,
            send_timestamp: queued.frame.send_timestamp,
            queued_at: queued.queued_at,
            is_keyframe: queued.frame.is_keyframe,
            width: queued.frame.width,
            height: queued.frame.height,
            fps_nominal: queued.frame.fps_nominal,
            codec: queued.frame.codec,
            encoded_payload_len: queued.payload_len,
            encoded_payload: queued.frame.payload.clone(),
        }
    }
}

/// Result of reading the queue for one single-view client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherSingleViewFrameSelectionResult {
    FrameAvailable(SwitcherSingleViewSelectedEncodedFrame),
    NoFrameAvailable { client_id: ClientId },
}

/// Read-only latest-frame selector for the single-view PoC.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleViewLatestFrameSelectionBoundary;

impl SwitcherSingleViewLatestFrameSelectionBoundary {
    pub fn select_latest(
        &self,
        input: SwitcherSingleViewFrameSelectionInput<'_>,
    ) -> SwitcherSingleViewFrameSelectionResult {
        input
            .queue_state
            .frames_for_client(input.client_id)
            .last()
            .map(SwitcherSingleViewSelectedEncodedFrame::from)
            .map(SwitcherSingleViewFrameSelectionResult::FrameAvailable)
            .unwrap_or_else(
                || SwitcherSingleViewFrameSelectionResult::NoFrameAvailable {
                    client_id: input.client_id.clone(),
                },
            )
    }
}

/// Single-client source mode for the first queue-read-backed switcher/sync path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherSingleClientQueueSourceMode {
    PreviewOldest,
    PreviewLatest,
    ConsumeOldest,
}

impl SwitcherSingleClientQueueSourceMode {
    fn queue_read_mode(self) -> ServerVideoFrameQueueReadMode {
        match self {
            Self::PreviewOldest => ServerVideoFrameQueueReadMode::InspectOldest,
            Self::PreviewLatest => ServerVideoFrameQueueReadMode::InspectLatest,
            Self::ConsumeOldest => ServerVideoFrameQueueReadMode::DequeueOldest,
        }
    }

    fn handoff_read_mode(self) -> ServerSwitcherQueuedFrameReadMode {
        match self {
            Self::PreviewOldest => ServerSwitcherQueuedFrameReadMode::InspectOldest,
            Self::PreviewLatest => ServerSwitcherQueuedFrameReadMode::InspectLatest,
            Self::ConsumeOldest => ServerSwitcherQueuedFrameReadMode::DequeueOldest,
        }
    }

    fn from_handoff_read_mode(mode: ServerSwitcherQueuedFrameReadMode) -> Self {
        match mode {
            ServerSwitcherQueuedFrameReadMode::InspectOldest => Self::PreviewOldest,
            ServerSwitcherQueuedFrameReadMode::InspectLatest => Self::PreviewLatest,
            ServerSwitcherQueuedFrameReadMode::DequeueOldest => Self::ConsumeOldest,
        }
    }
}

/// Input for reading one selected client/run from the server queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSingleClientQueueSourceInput {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub mode: SwitcherSingleClientQueueSourceMode,
}

/// Result of the first switcher/sync-facing queue source boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherSingleClientQueueSourceResult {
    FrameAvailable {
        frame: SwitcherSingleViewSelectedEncodedFrame,
        mode: SwitcherSingleClientQueueSourceMode,
        remaining_client_queue_len: usize,
    },
    NoFrameAvailable {
        client_id: ClientId,
        run_id: RunId,
        mode: SwitcherSingleClientQueueSourceMode,
        client_queue_len: usize,
    },
}

/// Minimal in-process source from the server encoded-frame queue to switcher/sync.
///
/// This boundary delegates queue access to `ServerVideoFrameQueueReadBoundary`
/// and only maps the result into the existing switcher encoded-frame handoff
/// shape. It is single-client and manual/diagnostic for now. It does not
/// perform targetTime selection, late-drop mutation, decode, render, 4-view
/// orchestration, socket I/O, or OBS output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleClientQueueSourceBoundary {
    queue_reader: ServerVideoFrameQueueReadBoundary,
}

impl SwitcherSingleClientQueueSourceBoundary {
    pub fn read(
        &self,
        queue_state: &mut ServerVideoFrameQueueState,
        input: SwitcherSingleClientQueueSourceInput,
    ) -> SwitcherSingleClientQueueSourceResult {
        let mode = input.mode;
        let result = self.queue_reader.read(
            queue_state,
            ServerVideoFrameQueueReadInput {
                client_id: input.client_id,
                run_id: input.run_id,
                mode: mode.queue_read_mode(),
            },
        );

        match result {
            ServerVideoFrameQueueReadResult::FrameAvailable {
                frame,
                remaining_client_queue_len,
                ..
            } => SwitcherSingleClientQueueSourceResult::FrameAvailable {
                frame: SwitcherSingleViewSelectedEncodedFrame::from(&frame),
                mode,
                remaining_client_queue_len,
            },
            ServerVideoFrameQueueReadResult::NoFrameAvailable {
                client_id,
                run_id,
                client_queue_len,
            } => SwitcherSingleClientQueueSourceResult::NoFrameAvailable {
                client_id,
                run_id,
                mode,
                client_queue_len,
            },
        }
    }
}

/// Switcher-facing queued encoded-frame source interface.
///
/// This is the first production-oriented server->switcher handoff shape. It
/// keeps `client_id`, `run_id`, and queue read mode in the input, preserves
/// explicit no-frame results, and intentionally says nothing about transport.
pub trait SwitcherQueuedFrameSource {
    fn read_queued_frame(
        &mut self,
        input: SwitcherSingleClientQueueSourceInput,
    ) -> SwitcherSingleClientQueueSourceResult;
}

/// In-process adapter from caller-owned server queues to the switcher source.
///
/// The adapter wraps `SwitcherSingleClientQueueSourceBoundary`, which in turn
/// wraps `ServerVideoFrameQueueReadBoundary`. It does not add IPC, socket I/O,
/// decode/render behavior, OBS output, 4-view orchestration, or fragment
/// reassembly.
pub struct SwitcherInProcessServerQueueFrameSource<'a> {
    queue_state: &'a mut ServerVideoFrameQueueState,
    boundary: SwitcherSingleClientQueueSourceBoundary,
}

impl<'a> SwitcherInProcessServerQueueFrameSource<'a> {
    pub fn new(queue_state: &'a mut ServerVideoFrameQueueState) -> Self {
        Self {
            queue_state,
            boundary: SwitcherSingleClientQueueSourceBoundary::default(),
        }
    }
}

impl SwitcherQueuedFrameSource for SwitcherInProcessServerQueueFrameSource<'_> {
    fn read_queued_frame(
        &mut self,
        input: SwitcherSingleClientQueueSourceInput,
    ) -> SwitcherSingleClientQueueSourceResult {
        self.boundary.read(self.queue_state, input)
    }
}

/// Transport-neutral queued-frame handoff input.
///
/// This is intentionally the same read shape the switcher already uses:
/// one client/run and one explicit queue read mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherQueuedFrameHandoffInput {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub mode: SwitcherSingleClientQueueSourceMode,
}

/// Source/handoff failures are distinct from a normal empty queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherQueuedFrameHandoffError {
    SourceUnavailable,
    Timeout,
    InvalidScope {
        client_id: ClientId,
        run_id: RunId,
    },
    UnsupportedMode {
        mode: SwitcherSingleClientQueueSourceMode,
    },
    MalformedResponse,
    SourceShutdown,
}

/// Fallible handoff result for future server->switcher sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherQueuedFrameHandoffResult {
    FrameRead {
        frame: SwitcherSingleViewSelectedEncodedFrame,
        mode: SwitcherSingleClientQueueSourceMode,
        remaining_client_queue_len: usize,
    },
    NoFrameAvailable {
        client_id: ClientId,
        run_id: RunId,
        mode: SwitcherSingleClientQueueSourceMode,
        client_queue_len: usize,
    },
    HandoffError {
        client_id: ClientId,
        run_id: RunId,
        mode: SwitcherSingleClientQueueSourceMode,
        error: SwitcherQueuedFrameHandoffError,
    },
}

/// Transport-neutral, fallible server->switcher queued-frame handoff.
///
/// Implementations must keep targetTime selection out of the handoff. The
/// handoff only reads queued encoded frames, reports explicit no-frame, or
/// reports source/handoff failure.
pub trait SwitcherQueuedFrameHandoff {
    fn read_handoff_frame(
        &mut self,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> SwitcherQueuedFrameHandoffResult;
}

impl From<ServerSwitcherQueuedFrameHandoffFrame> for SwitcherSingleViewSelectedEncodedFrame {
    fn from(frame: ServerSwitcherQueuedFrameHandoffFrame) -> Self {
        Self {
            client_id: frame.client_id,
            run_id: frame.run_id,
            frame_id: frame.frame_id,
            capture_timestamp: frame.capture_timestamp,
            send_timestamp: frame.send_timestamp,
            queued_at: frame.queued_at,
            is_keyframe: frame.is_keyframe,
            width: frame.width,
            height: frame.height,
            fps_nominal: frame.fps_nominal,
            codec: frame.codec,
            encoded_payload_len: frame.encoded_payload_len as usize,
            encoded_payload: frame.encoded_payload,
        }
    }
}

/// Transport-neutral request/response adapter for the future switcher client runtime.
///
/// This boundary builds a DTO request from the existing switcher handoff input
/// and maps a DTO response back into the existing switcher handoff result. It
/// does not open pipes/sockets, block on I/O, own request-id generation, or
/// implement named-pipe/TCP/UDP/shared-memory transport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherServerQueuedFrameHandoffClientAdapterBoundary;

impl SwitcherServerQueuedFrameHandoffClientAdapterBoundary {
    pub fn build_request(
        &self,
        request_id: u64,
        input: &SwitcherQueuedFrameHandoffInput,
    ) -> ServerSwitcherQueuedFrameHandoffRequest {
        ServerSwitcherQueuedFrameHandoffRequest {
            handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
            request_id,
            client_id: input.client_id.clone(),
            run_id: input.run_id.clone(),
            read_mode: input.mode.handoff_read_mode(),
        }
    }

    pub fn map_response(
        &self,
        input: &SwitcherQueuedFrameHandoffInput,
        response: ServerSwitcherQueuedFrameHandoffResponse,
    ) -> SwitcherQueuedFrameHandoffResult {
        match response {
            ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
                frame,
                remaining_client_queue_len,
                ..
            } => SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: frame.into(),
                mode: input.mode,
                remaining_client_queue_len: remaining_client_queue_len as usize,
            },
            ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
                client_id,
                run_id,
                read_mode,
                client_queue_len,
                ..
            } => SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                client_id,
                run_id,
                mode: SwitcherSingleClientQueueSourceMode::from_handoff_read_mode(read_mode),
                client_queue_len: client_queue_len as usize,
            },
            ServerSwitcherQueuedFrameHandoffResponse::HandoffError { error, .. } => {
                SwitcherQueuedFrameHandoffResult::HandoffError {
                    client_id: input.client_id.clone(),
                    run_id: input.run_id.clone(),
                    mode: input.mode,
                    error: map_handoff_error_code_to_switcher(input, error),
                }
            }
        }
    }
}

fn map_handoff_error_code_to_switcher(
    input: &SwitcherQueuedFrameHandoffInput,
    error: ServerSwitcherQueuedFrameHandoffErrorCode,
) -> SwitcherQueuedFrameHandoffError {
    match error {
        ServerSwitcherQueuedFrameHandoffErrorCode::SourceUnavailable => {
            SwitcherQueuedFrameHandoffError::SourceUnavailable
        }
        ServerSwitcherQueuedFrameHandoffErrorCode::RequestTimeout => {
            SwitcherQueuedFrameHandoffError::Timeout
        }
        ServerSwitcherQueuedFrameHandoffErrorCode::InvalidScope => {
            SwitcherQueuedFrameHandoffError::InvalidScope {
                client_id: input.client_id.clone(),
                run_id: input.run_id.clone(),
            }
        }
        ServerSwitcherQueuedFrameHandoffErrorCode::UnsupportedReadMode => {
            SwitcherQueuedFrameHandoffError::UnsupportedMode { mode: input.mode }
        }
        ServerSwitcherQueuedFrameHandoffErrorCode::MalformedResponse => {
            SwitcherQueuedFrameHandoffError::MalformedResponse
        }
        ServerSwitcherQueuedFrameHandoffErrorCode::SourceShutdown => {
            SwitcherQueuedFrameHandoffError::SourceShutdown
        }
    }
}

/// Output of one named-pipe client request/response handoff on the switcher side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
    pub request: ServerSwitcherQueuedFrameHandoffRequest,
    pub response: Option<ServerSwitcherQueuedFrameHandoffResponse>,
    pub result: SwitcherQueuedFrameHandoffResult,
}

/// Per-request runtime config for one switcher named-pipe handoff.
///
/// This is intentionally small for the first lifecycle slice: only one
/// connect/wait timeout is configurable per request. It does not add retries,
/// reconnect/backoff, or a longer-lived service policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitcherNamedPipeQueuedFrameHandoffRequestConfig {
    pub connect_timeout_millis: u32,
}

impl Default for SwitcherNamedPipeQueuedFrameHandoffRequestConfig {
    fn default() -> Self {
        Self {
            connect_timeout_millis: 5_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherNamedPipeQueuedFrameHandoffRequestStatus {
    Sent,
    EncodedOnly,
    EncodeFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherNamedPipeQueuedFrameHandoffResponseStatus {
    Decoded,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherNamedPipeQueuedFrameHandoffResultKind {
    FrameRead,
    NoFrameAvailable,
    HandoffError,
}

/// Summary for one switcher-side named-pipe handoff request lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherNamedPipeQueuedFrameHandoffRequestSummary {
    pub pipe_name: String,
    pub request_id: u64,
    pub read_mode: SwitcherSingleClientQueueSourceMode,
    pub timeout_millis: u32,
    pub request_status: SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
    pub response_status: SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
    pub result_kind: SwitcherNamedPipeQueuedFrameHandoffResultKind,
    pub elapsed_millis: u64,
}

/// Output for one switcher-side named-pipe handoff request with lifecycle
/// summary kept visible alongside the mapped result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherNamedPipeQueuedFrameHandoffRequestOutput {
    pub summary: SwitcherNamedPipeQueuedFrameHandoffRequestSummary,
    pub runtime: Option<SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput>,
    pub result: SwitcherQueuedFrameHandoffResult,
}

/// Fatal local failure while preparing one named-pipe handoff request.
#[derive(Debug)]
pub enum SwitcherNamedPipeQueuedFrameHandoffRuntimeError {
    EncodeRequest(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffCodecError),
}

/// Runtime abstraction used by the named-pipe-backed switcher handoff wrapper.
///
/// The default Windows implementation performs one real named-pipe round trip.
/// Tests can substitute a fake runtime to verify request-id policy and result
/// propagation without relying on local pipe I/O.
pub trait SwitcherNamedPipeQueuedFrameHandoffRuntime {
    fn run_once_with_config(
        &mut self,
        pipe_name: &str,
        request_id: u64,
        input: SwitcherQueuedFrameHandoffInput,
        config: SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    ) -> Result<
        SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput,
        SwitcherNamedPipeQueuedFrameHandoffRuntimeError,
    >;

    fn run_once(
        &mut self,
        pipe_name: &str,
        request_id: u64,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> Result<
        SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput,
        SwitcherNamedPipeQueuedFrameHandoffRuntimeError,
    > {
        self.run_once_with_config(
            pipe_name,
            request_id,
            input,
            SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
        )
    }
}

/// Clock hook for deterministic lifecycle timing tests.
pub trait SwitcherNamedPipeQueuedFrameHandoffClock {
    fn now_millis(&mut self) -> u64;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherNamedPipeQueuedFrameHandoffSystemClock;

impl SwitcherNamedPipeQueuedFrameHandoffClock for SwitcherNamedPipeQueuedFrameHandoffSystemClock {
    fn now_millis(&mut self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Thin switcher-side handoff wrapper over the one-request / one-response
/// named-pipe runtime.
///
/// The wrapper preserves the existing `SwitcherQueuedFrameHandoff`
/// abstraction. Callers may either provide an explicit request id per call or
/// use the wrapper-owned monotonic request-id counter.
pub struct SwitcherNamedPipeQueuedFrameHandoff<
    R,
    C = SwitcherNamedPipeQueuedFrameHandoffSystemClock,
> {
    pipe_name: String,
    next_request_id: u64,
    runtime: R,
    clock: C,
}

impl<R> SwitcherNamedPipeQueuedFrameHandoff<R, SwitcherNamedPipeQueuedFrameHandoffSystemClock> {
    pub fn from_runtime(pipe_name: impl Into<String>, initial_request_id: u64, runtime: R) -> Self {
        Self::from_runtime_with_clock(
            pipe_name,
            initial_request_id,
            runtime,
            SwitcherNamedPipeQueuedFrameHandoffSystemClock,
        )
    }
}

impl<R, C> SwitcherNamedPipeQueuedFrameHandoff<R, C> {
    pub fn from_runtime_with_clock(
        pipe_name: impl Into<String>,
        initial_request_id: u64,
        runtime: R,
        clock: C,
    ) -> Self {
        Self {
            pipe_name: pipe_name.into(),
            next_request_id: initial_request_id,
            runtime,
            clock,
        }
    }

    pub fn next_request_id(&self) -> u64 {
        self.next_request_id
    }
}

impl<R, C> SwitcherNamedPipeQueuedFrameHandoff<R, C>
where
    R: SwitcherNamedPipeQueuedFrameHandoffRuntime,
    C: SwitcherNamedPipeQueuedFrameHandoffClock,
{
    pub fn read_handoff_frame_with_request_id(
        &mut self,
        request_id: u64,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> SwitcherQueuedFrameHandoffResult {
        self.read_handoff_frame_with_request_id_and_config(
            request_id,
            input,
            SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
        )
        .result
    }

    pub fn read_handoff_frame_with_config(
        &mut self,
        input: SwitcherQueuedFrameHandoffInput,
        config: SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    ) -> SwitcherNamedPipeQueuedFrameHandoffRequestOutput {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.read_handoff_frame_with_request_id_and_config(request_id, input, config)
    }

    pub fn read_handoff_frame_with_request_id_and_config(
        &mut self,
        request_id: u64,
        input: SwitcherQueuedFrameHandoffInput,
        config: SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    ) -> SwitcherNamedPipeQueuedFrameHandoffRequestOutput {
        let start_millis = self.clock.now_millis();
        let runtime_result =
            self.runtime
                .run_once_with_config(&self.pipe_name, request_id, input.clone(), config);
        let elapsed_millis = self.clock.now_millis().saturating_sub(start_millis);

        match runtime_result {
            Ok(output) => {
                let summary = SwitcherNamedPipeQueuedFrameHandoffRequestSummary {
                    pipe_name: self.pipe_name.clone(),
                    request_id,
                    read_mode: input.mode,
                    timeout_millis: config.connect_timeout_millis,
                    request_status: if output.response.is_some() {
                        SwitcherNamedPipeQueuedFrameHandoffRequestStatus::Sent
                    } else {
                        SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodedOnly
                    },
                    response_status: if output.response.is_some() {
                        SwitcherNamedPipeQueuedFrameHandoffResponseStatus::Decoded
                    } else {
                        SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None
                    },
                    result_kind: named_pipe_handoff_result_kind(&output.result),
                    elapsed_millis,
                };
                let result = output.result.clone();
                SwitcherNamedPipeQueuedFrameHandoffRequestOutput {
                    summary,
                    runtime: Some(output),
                    result,
                }
            }
            Err(SwitcherNamedPipeQueuedFrameHandoffRuntimeError::EncodeRequest(_)) => {
                let result = SwitcherQueuedFrameHandoffResult::HandoffError {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    mode: input.mode,
                    error: SwitcherQueuedFrameHandoffError::MalformedResponse,
                };
                let summary = SwitcherNamedPipeQueuedFrameHandoffRequestSummary {
                    pipe_name: self.pipe_name.clone(),
                    request_id,
                    read_mode: input.mode,
                    timeout_millis: config.connect_timeout_millis,
                    request_status: SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodeFailed,
                    response_status: SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None,
                    result_kind: SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError,
                    elapsed_millis,
                };
                SwitcherNamedPipeQueuedFrameHandoffRequestOutput {
                    summary,
                    runtime: None,
                    result,
                }
            }
        }
    }
}

impl<R, C> SwitcherQueuedFrameHandoff for SwitcherNamedPipeQueuedFrameHandoff<R, C>
where
    R: SwitcherNamedPipeQueuedFrameHandoffRuntime,
    C: SwitcherNamedPipeQueuedFrameHandoffClock,
{
    fn read_handoff_frame(
        &mut self,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> SwitcherQueuedFrameHandoffResult {
        self.read_handoff_frame_with_config(
            input,
            SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
        )
        .result
    }
}

#[cfg(windows)]
impl
    SwitcherNamedPipeQueuedFrameHandoff<
        SwitcherNamedPipeQueuedFrameHandoffRuntimeBoundary,
        SwitcherNamedPipeQueuedFrameHandoffSystemClock,
    >
{
    pub fn new(pipe_name: impl Into<String>, initial_request_id: u64) -> Self {
        Self::from_runtime(
            pipe_name,
            initial_request_id,
            SwitcherNamedPipeQueuedFrameHandoffRuntimeBoundary::default(),
        )
    }
}

/// Windows-only one-request / one-response named-pipe client runtime for the
/// first real server->switcher handoff transport slice.
///
/// This runtime builds one DTO request, connects to one named pipe, writes one
/// framed request, reads one framed response, maps it through the existing
/// switcher DTO adapter, and returns. It does not implement retries,
/// reconnects, lifecycle orchestration, service loops, OBS output, or 4-view
/// orchestration.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherNamedPipeQueuedFrameHandoffRuntimeBoundary {
    codec: stream_sync_net_core::ServerSwitcherQueuedFrameHandoffCodecBoundary,
    adapter: SwitcherServerQueuedFrameHandoffClientAdapterBoundary,
}

#[cfg(windows)]
impl SwitcherNamedPipeQueuedFrameHandoffRuntimeBoundary {
    pub fn run_once(
        &self,
        pipe_name: &str,
        request_id: u64,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> Result<
        SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput,
        SwitcherNamedPipeQueuedFrameHandoffRuntimeError,
    > {
        self.run_once_with_config(
            pipe_name,
            request_id,
            input,
            SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
        )
    }

    pub fn run_once_with_config(
        &self,
        pipe_name: &str,
        request_id: u64,
        input: SwitcherQueuedFrameHandoffInput,
        config: SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    ) -> Result<
        SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput,
        SwitcherNamedPipeQueuedFrameHandoffRuntimeError,
    > {
        let request = self.adapter.build_request(request_id, &input);
        let request_frame = self
            .codec
            .encode_request_frame(&request)
            .map_err(SwitcherNamedPipeQueuedFrameHandoffRuntimeError::EncodeRequest)?;

        let mut pipe = match open_named_pipe_client(pipe_name, config.connect_timeout_millis) {
            Ok(pipe) => pipe,
            Err(error) => {
                return Ok(SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request,
                    response: None,
                    result: handoff_error_result_from_io(&input, error),
                });
            }
        };

        if let Err(error) = pipe.write_all(&request_frame) {
            return Ok(SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                request,
                response: None,
                result: handoff_error_result_from_io(&input, error),
            });
        }
        if let Err(error) = pipe.flush() {
            return Ok(SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                request,
                response: None,
                result: handoff_error_result_from_io(&input, error),
            });
        }

        let response_frame = match read_length_prefixed_frame_from_pipe(&mut pipe) {
            Ok(frame) => frame,
            Err(error) => {
                return Ok(SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request,
                    response: None,
                    result: handoff_error_result_from_io(&input, error),
                });
            }
        };
        let response = match self.codec.decode_response_frame(&response_frame) {
            Ok(response) => response,
            Err(_) => {
                return Ok(SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request,
                    response: None,
                    result: SwitcherQueuedFrameHandoffResult::HandoffError {
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        mode: input.mode,
                        error: SwitcherQueuedFrameHandoffError::MalformedResponse,
                    },
                });
            }
        };
        let result = self.adapter.map_response(&input, response.clone());

        Ok(SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
            request,
            response: Some(response),
            result,
        })
    }
}

#[cfg(windows)]
impl SwitcherNamedPipeQueuedFrameHandoffRuntime
    for SwitcherNamedPipeQueuedFrameHandoffRuntimeBoundary
{
    fn run_once_with_config(
        &mut self,
        pipe_name: &str,
        request_id: u64,
        input: SwitcherQueuedFrameHandoffInput,
        config: SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    ) -> Result<
        SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput,
        SwitcherNamedPipeQueuedFrameHandoffRuntimeError,
    > {
        SwitcherNamedPipeQueuedFrameHandoffRuntimeBoundary::run_once_with_config(
            self, pipe_name, request_id, input, config,
        )
    }
}

fn named_pipe_handoff_result_kind(
    result: &SwitcherQueuedFrameHandoffResult,
) -> SwitcherNamedPipeQueuedFrameHandoffResultKind {
    match result {
        SwitcherQueuedFrameHandoffResult::FrameRead { .. } => {
            SwitcherNamedPipeQueuedFrameHandoffResultKind::FrameRead
        }
        SwitcherQueuedFrameHandoffResult::NoFrameAvailable { .. } => {
            SwitcherNamedPipeQueuedFrameHandoffResultKind::NoFrameAvailable
        }
        SwitcherQueuedFrameHandoffResult::HandoffError { .. } => {
            SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError
        }
    }
}

#[cfg(windows)]
fn handoff_error_result_from_io(
    input: &SwitcherQueuedFrameHandoffInput,
    error: io::Error,
) -> SwitcherQueuedFrameHandoffResult {
    let handoff_error = match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => {
            SwitcherQueuedFrameHandoffError::Timeout
        }
        io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::UnexpectedEof => SwitcherQueuedFrameHandoffError::SourceShutdown,
        _ => SwitcherQueuedFrameHandoffError::SourceUnavailable,
    };

    SwitcherQueuedFrameHandoffResult::HandoffError {
        client_id: input.client_id.clone(),
        run_id: input.run_id.clone(),
        mode: input.mode,
        error: handoff_error,
    }
}

#[cfg(windows)]
fn open_named_pipe_client(pipe_name: &str, timeout_millis: u32) -> io::Result<File> {
    let pipe_path = named_pipe_path(pipe_name)?;
    let wide_name: Vec<u16> = pipe_path.encode_utf16().chain(Some(0)).collect();

    let waited = unsafe { WaitNamedPipeW(PCWSTR(wide_name.as_ptr()), timeout_millis) };
    if !waited.as_bool() {
        return Err(io::Error::last_os_error());
    }

    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_name.as_ptr()),
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|_| io::Error::last_os_error())?;
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let owned = unsafe { OwnedHandle::from_raw_handle(handle.0 as *mut _) };
    Ok(File::from(owned))
}

#[cfg(windows)]
fn read_length_prefixed_frame_from_pipe(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut prefix = [0u8; 4];
    reader.read_exact(&mut prefix)?;
    let body_len = u32::from_le_bytes(prefix) as usize;
    let mut frame = Vec::with_capacity(4 + body_len);
    frame.extend_from_slice(&prefix);
    let mut body = vec![0u8; body_len];
    reader.read_exact(&mut body)?;
    frame.extend_from_slice(&body);
    Ok(frame)
}

#[cfg(windows)]
fn named_pipe_path(pipe_name: &str) -> io::Result<String> {
    if pipe_name.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "named pipe name must not be empty",
        ));
    }

    Ok(format!(r"\\.\pipe\{pipe_name}"))
}

/// In-process fallible handoff backed by the current server queue source.
///
/// This adapter adds handoff-level error shape without adding transport. It
/// delegates successful reads/no-frame to `SwitcherInProcessServerQueueFrameSource`.
pub struct SwitcherInProcessQueuedFrameHandoff<'a> {
    source: SwitcherInProcessServerQueueFrameSource<'a>,
}

impl<'a> SwitcherInProcessQueuedFrameHandoff<'a> {
    pub fn new(queue_state: &'a mut ServerVideoFrameQueueState) -> Self {
        Self {
            source: SwitcherInProcessServerQueueFrameSource::new(queue_state),
        }
    }
}

impl SwitcherQueuedFrameHandoff for SwitcherInProcessQueuedFrameHandoff<'_> {
    fn read_handoff_frame(
        &mut self,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> SwitcherQueuedFrameHandoffResult {
        if input.client_id.0.trim().is_empty() || input.run_id.0.trim().is_empty() {
            return SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: input.client_id.clone(),
                run_id: input.run_id.clone(),
                mode: input.mode,
                error: SwitcherQueuedFrameHandoffError::InvalidScope {
                    client_id: input.client_id,
                    run_id: input.run_id,
                },
            };
        }

        match self
            .source
            .read_queued_frame(SwitcherSingleClientQueueSourceInput {
                client_id: input.client_id,
                run_id: input.run_id,
                mode: input.mode,
            }) {
            SwitcherSingleClientQueueSourceResult::FrameAvailable {
                frame,
                mode,
                remaining_client_queue_len,
            } => SwitcherQueuedFrameHandoffResult::FrameRead {
                frame,
                mode,
                remaining_client_queue_len,
            },
            SwitcherSingleClientQueueSourceResult::NoFrameAvailable {
                client_id,
                run_id,
                mode,
                client_queue_len,
            } => SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                client_id,
                run_id,
                mode,
                client_queue_len,
            },
        }
    }
}

/// Result of consuming fallible handoff output at the switcher boundary.
///
/// Frame/no-frame outcomes are adapted back into the existing queue-source
/// result shape. Handoff errors remain separate and are never collapsed into
/// no-frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherQueuedFrameHandoffConsumerResult {
    FrameAvailable {
        source_result: SwitcherSingleClientQueueSourceResult,
    },
    NoFrameAvailable {
        source_result: SwitcherSingleClientQueueSourceResult,
    },
    HandoffError {
        client_id: ClientId,
        run_id: RunId,
        mode: SwitcherSingleClientQueueSourceMode,
        error: SwitcherQueuedFrameHandoffError,
    },
}

/// Smallest switcher-side consumer of fallible queued-frame handoff reads.
///
/// This boundary performs no targetTime selection, decode, rendering, 4-view
/// orchestration, transport I/O, or fragment reassembly. It only converts
/// handoff frame/no-frame results into the existing queue-source shape and
/// preserves handoff failures explicitly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherQueuedFrameHandoffConsumerBoundary;

impl SwitcherQueuedFrameHandoffConsumerBoundary {
    pub fn read_source_result(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> SwitcherQueuedFrameHandoffConsumerResult {
        match handoff.read_handoff_frame(input) {
            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame,
                mode,
                remaining_client_queue_len,
            } => SwitcherQueuedFrameHandoffConsumerResult::FrameAvailable {
                source_result: SwitcherSingleClientQueueSourceResult::FrameAvailable {
                    frame,
                    mode,
                    remaining_client_queue_len,
                },
            },
            SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                client_id,
                run_id,
                mode,
                client_queue_len,
            } => SwitcherQueuedFrameHandoffConsumerResult::NoFrameAvailable {
                source_result: SwitcherSingleClientQueueSourceResult::NoFrameAvailable {
                    client_id,
                    run_id,
                    mode,
                    client_queue_len,
                },
            },
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id,
                run_id,
                mode,
                error,
            } => SwitcherQueuedFrameHandoffConsumerResult::HandoffError {
                client_id,
                run_id,
                mode,
                error,
            },
        }
    }
}

/// Selection behavior for the first single-client targetTime queue source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherSingleClientTargetTimeSourceMode {
    PreviewOldestIfAtOrBefore,
    PreviewLatestIfAtOrBefore,
    ConsumeOldestAtOrBefore,
}

/// Input for selecting one client/run frame against a target timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSingleClientTargetTimeSourceInput {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub target_timestamp: TimestampMicros,
    pub mode: SwitcherSingleClientTargetTimeSourceMode,
}

/// Selected encoded frame plus its timing relationship to targetTime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSingleClientTargetTimeSelectedFrame {
    pub frame: SwitcherSingleViewSelectedEncodedFrame,
    pub target_timestamp: TimestampMicros,
    pub delta_from_target_micros: i64,
    pub consumed: bool,
}

/// Result of one single-client targetTime-aware source read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherSingleClientTargetTimeSourceResult {
    Selected(SwitcherSingleClientTargetTimeSelectedFrame),
    NoFrameAvailable {
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        mode: SwitcherSingleClientTargetTimeSourceMode,
        client_queue_len: usize,
    },
    WaitingForFrameAtOrBeforeTarget {
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        mode: SwitcherSingleClientTargetTimeSourceMode,
        candidate_frame_id: u64,
        candidate_capture_timestamp: TimestampMicros,
        client_queue_len: usize,
    },
}

/// Fallible targetTime-aware result over the queued-frame handoff consumer.
///
/// Handoff errors are source/transport failures and stay distinct from normal
/// no-frame or waiting targetTime states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherSingleClientTargetTimeHandoffSourceResult {
    Selected(SwitcherSingleClientTargetTimeSelectedFrame),
    NoFrameAvailable {
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        mode: SwitcherSingleClientTargetTimeSourceMode,
        client_queue_len: usize,
    },
    WaitingForFrameAtOrBeforeTarget {
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        mode: SwitcherSingleClientTargetTimeSourceMode,
        candidate_frame_id: u64,
        candidate_capture_timestamp: TimestampMicros,
        client_queue_len: usize,
    },
    HandoffError {
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        mode: SwitcherSingleClientTargetTimeSourceMode,
        handoff_mode: SwitcherSingleClientQueueSourceMode,
        error: SwitcherQueuedFrameHandoffError,
    },
}

/// TargetTime-aware source over fallible queued-frame handoff.
///
/// This boundary keeps targetTime selection in the switcher while allowing
/// source/handoff failures to be surfaced separately from no-frame/waiting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleClientTargetTimeHandoffSourceBoundary {
    consumer: SwitcherQueuedFrameHandoffConsumerBoundary,
}

impl SwitcherSingleClientTargetTimeHandoffSourceBoundary {
    pub fn select_from_handoff(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeHandoffSourceResult {
        match input.mode {
            SwitcherSingleClientTargetTimeSourceMode::PreviewOldestIfAtOrBefore => {
                self.preview_oldest_at_or_before(handoff, input)
            }
            SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore => {
                self.preview_latest_at_or_before(handoff, input)
            }
            SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore => {
                self.consume_oldest_at_or_before(handoff, input)
            }
        }
    }

    fn preview_latest_at_or_before(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeHandoffSourceResult {
        let mode = input.mode;
        let target_timestamp = input.target_timestamp;
        let consumer_result = self.consumer.read_source_result(
            handoff,
            SwitcherQueuedFrameHandoffInput {
                client_id: input.client_id,
                run_id: input.run_id,
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            },
        );
        handoff_consumer_result_for_candidate(consumer_result, target_timestamp, mode, false)
    }

    fn preview_oldest_at_or_before(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeHandoffSourceResult {
        let mode = input.mode;
        let target_timestamp = input.target_timestamp;
        let consumer_result = self.consumer.read_source_result(
            handoff,
            SwitcherQueuedFrameHandoffInput {
                client_id: input.client_id,
                run_id: input.run_id,
                mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
            },
        );
        handoff_consumer_result_for_candidate(consumer_result, target_timestamp, mode, false)
    }

    fn consume_oldest_at_or_before(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeHandoffSourceResult {
        let client_id = input.client_id;
        let run_id = input.run_id;
        let mode = input.mode;
        let target_timestamp = input.target_timestamp;
        let preview = self.consumer.read_source_result(
            handoff,
            SwitcherQueuedFrameHandoffInput {
                client_id: client_id.clone(),
                run_id: run_id.clone(),
                mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
            },
        );
        let preview_result =
            handoff_consumer_result_for_candidate(preview, target_timestamp, mode, false);

        if !matches!(
            preview_result,
            SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(_)
        ) {
            return preview_result;
        }

        let consumed = self.consumer.read_source_result(
            handoff,
            SwitcherQueuedFrameHandoffInput {
                client_id,
                run_id,
                mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
            },
        );
        handoff_consumer_result_for_candidate(consumed, target_timestamp, mode, true)
    }
}

/// Minimal targetTime-aware source over the single-client queue source.
///
/// This boundary is single-client and source-driven. It reads encoded frames
/// through `SwitcherQueuedFrameSource`, compares capture timestamp to an
/// explicit target timestamp, and returns a selected/no-frame/waiting result.
/// It does not decode, render, mutate late frames, orchestrate multiple views,
/// or touch OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleClientTargetTimeSourceBoundary;

impl SwitcherSingleClientTargetTimeSourceBoundary {
    pub fn select(
        &self,
        queue_state: &mut ServerVideoFrameQueueState,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeSourceResult {
        let mut source = SwitcherInProcessServerQueueFrameSource::new(queue_state);
        self.select_from_source(&mut source, input)
    }

    pub fn select_from_source(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeSourceResult {
        match input.mode {
            SwitcherSingleClientTargetTimeSourceMode::PreviewOldestIfAtOrBefore => {
                self.preview_oldest_at_or_before(source, input)
            }
            SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore => {
                self.preview_latest_at_or_before(source, input)
            }
            SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore => {
                self.consume_oldest_at_or_before(source, input)
            }
        }
    }

    fn preview_latest_at_or_before(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeSourceResult {
        let client_id = input.client_id;
        let run_id = input.run_id;
        let mode = input.mode;
        let target_timestamp = input.target_timestamp;
        let source_result = source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
            client_id,
            run_id,
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
        });
        result_for_candidate(source_result, target_timestamp, mode, false)
    }

    fn preview_oldest_at_or_before(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeSourceResult {
        let client_id = input.client_id;
        let run_id = input.run_id;
        let mode = input.mode;
        let target_timestamp = input.target_timestamp;
        let source_result = source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
            client_id,
            run_id,
            mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
        });
        result_for_candidate(source_result, target_timestamp, mode, false)
    }

    fn consume_oldest_at_or_before(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherSingleClientTargetTimeSourceInput,
    ) -> SwitcherSingleClientTargetTimeSourceResult {
        let client_id = input.client_id;
        let run_id = input.run_id;
        let mode = input.mode;
        let target_timestamp = input.target_timestamp;
        let preview = source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
            client_id: client_id.clone(),
            run_id: run_id.clone(),
            mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
        });

        let SwitcherSingleClientQueueSourceResult::FrameAvailable {
            frame,
            remaining_client_queue_len,
            ..
        } = preview
        else {
            return result_for_candidate(preview, target_timestamp, mode, false);
        };

        if frame.capture_timestamp.0 > target_timestamp.0 {
            return SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
                client_id,
                run_id,
                target_timestamp,
                mode,
                candidate_frame_id: frame.frame_id,
                candidate_capture_timestamp: frame.capture_timestamp,
                client_queue_len: remaining_client_queue_len,
            };
        }

        let consumed = source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
            client_id,
            run_id,
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        });
        result_for_candidate(consumed, target_timestamp, mode, true)
    }
}

fn result_for_candidate(
    source_result: SwitcherSingleClientQueueSourceResult,
    target_timestamp: TimestampMicros,
    mode: SwitcherSingleClientTargetTimeSourceMode,
    consumed: bool,
) -> SwitcherSingleClientTargetTimeSourceResult {
    match source_result {
        SwitcherSingleClientQueueSourceResult::FrameAvailable {
            frame,
            remaining_client_queue_len,
            ..
        } => {
            if frame.capture_timestamp.0 <= target_timestamp.0 {
                SwitcherSingleClientTargetTimeSourceResult::Selected(
                    SwitcherSingleClientTargetTimeSelectedFrame {
                        delta_from_target_micros: frame.capture_timestamp.0 as i64
                            - target_timestamp.0 as i64,
                        frame,
                        target_timestamp,
                        consumed,
                    },
                )
            } else {
                SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
                    client_id: frame.client_id.clone(),
                    run_id: frame.run_id.clone(),
                    target_timestamp,
                    mode,
                    candidate_frame_id: frame.frame_id,
                    candidate_capture_timestamp: frame.capture_timestamp,
                    client_queue_len: remaining_client_queue_len,
                }
            }
        }
        SwitcherSingleClientQueueSourceResult::NoFrameAvailable {
            client_id,
            run_id,
            client_queue_len,
            ..
        } => SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable {
            client_id,
            run_id,
            target_timestamp,
            mode,
            client_queue_len,
        },
    }
}

fn handoff_consumer_result_for_candidate(
    consumer_result: SwitcherQueuedFrameHandoffConsumerResult,
    target_timestamp: TimestampMicros,
    mode: SwitcherSingleClientTargetTimeSourceMode,
    consumed: bool,
) -> SwitcherSingleClientTargetTimeHandoffSourceResult {
    match consumer_result {
        SwitcherQueuedFrameHandoffConsumerResult::FrameAvailable { source_result }
        | SwitcherQueuedFrameHandoffConsumerResult::NoFrameAvailable { source_result } => {
            target_time_handoff_result_from_source_result(result_for_candidate(
                source_result,
                target_timestamp,
                mode,
                consumed,
            ))
        }
        SwitcherQueuedFrameHandoffConsumerResult::HandoffError {
            client_id,
            run_id,
            mode: handoff_mode,
            error,
        } => SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError {
            client_id,
            run_id,
            target_timestamp,
            mode,
            handoff_mode,
            error,
        },
    }
}

fn target_time_handoff_result_from_source_result(
    result: SwitcherSingleClientTargetTimeSourceResult,
) -> SwitcherSingleClientTargetTimeHandoffSourceResult {
    match result {
        SwitcherSingleClientTargetTimeSourceResult::Selected(selected) => {
            SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(selected)
        }
        SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable {
            client_id,
            run_id,
            target_timestamp,
            mode,
            client_queue_len,
        } => SwitcherSingleClientTargetTimeHandoffSourceResult::NoFrameAvailable {
            client_id,
            run_id,
            target_timestamp,
            mode,
            client_queue_len,
        },
        SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
            client_id,
            run_id,
            target_timestamp,
            mode,
            candidate_frame_id,
            candidate_capture_timestamp,
            client_queue_len,
        } => SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget {
            client_id,
            run_id,
            target_timestamp,
            mode,
            candidate_frame_id,
            candidate_capture_timestamp,
            client_queue_len,
        },
    }
}

/// One configured view for the first queue-backed 2-view targetTime scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewTargetTimeSourceViewConfig {
    pub client_id: ClientId,
    pub run_id: RunId,
}

/// Scheduler-level behavior for two-view targetTime selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherTwoViewTargetTimeSourceSchedulerMode {
    PreviewLatestIfAtOrBefore,
    ConsumeOldestAtOrBeforeAllSelected,
}

/// Input for selecting two client/run views against one shared target timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewTargetTimeSourceSchedulerInput {
    pub left: SwitcherTwoViewTargetTimeSourceViewConfig,
    pub right: SwitcherTwoViewTargetTimeSourceViewConfig,
    pub target_timestamp: TimestampMicros,
    pub mode: SwitcherTwoViewTargetTimeSourceSchedulerMode,
}

/// Aggregate scheduler status for the two per-view source results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherTwoViewTargetTimeSourceSchedulerStatus {
    AllSelected,
    PartialSelected,
    Waiting,
    NoFrames,
}

/// Result of the minimal 2-view queue-backed targetTime scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewTargetTimeSourceSchedulerResult {
    pub target_timestamp: TimestampMicros,
    pub mode: SwitcherTwoViewTargetTimeSourceSchedulerMode,
    pub left: SwitcherSingleClientTargetTimeSourceResult,
    pub right: SwitcherSingleClientTargetTimeSourceResult,
    pub status: SwitcherTwoViewTargetTimeSourceSchedulerStatus,
}

/// Aggregate scheduler status for fallible handoff-backed 2-view selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus {
    AllSelected,
    PartialSelected,
    Waiting,
    NoFrames,
    HandoffError,
}

/// Result of the fallible 2-view targetTime scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult {
    pub target_timestamp: TimestampMicros,
    pub mode: SwitcherTwoViewTargetTimeSourceSchedulerMode,
    pub left: SwitcherSingleClientTargetTimeHandoffSourceResult,
    pub right: SwitcherSingleClientTargetTimeHandoffSourceResult,
    pub status: SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus,
}

/// Minimal 2-view scheduler over the fallible single-client handoff source.
///
/// This boundary keeps targetTime selection in switcher and preserves handoff
/// failures as scheduler-level `HandoffError` instead of treating them as
/// no-frame, waiting, or partial selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary {
    single_client: SwitcherSingleClientTargetTimeHandoffSourceBoundary,
}

impl SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary {
    pub fn select_pair_from_handoff(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
    ) -> SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult {
        match input.mode {
            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore => {
                self.preview_latest_pair(handoff, input)
            }
            SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected => {
                self.consume_oldest_pair_all_selected(handoff, input)
            }
        }
    }

    fn preview_latest_pair(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
    ) -> SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult {
        self.select_pair_with_single_client_mode(
            handoff,
            input,
            SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
        )
    }

    fn consume_oldest_pair_all_selected(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
    ) -> SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult {
        let preview = self.select_pair_with_single_client_mode(
            handoff,
            input.clone(),
            SwitcherSingleClientTargetTimeSourceMode::PreviewOldestIfAtOrBefore,
        );

        if preview.status != SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected {
            return preview;
        }

        self.select_pair_with_single_client_mode(
            handoff,
            input,
            SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
        )
    }

    fn select_pair_with_single_client_mode(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
        single_client_mode: SwitcherSingleClientTargetTimeSourceMode,
    ) -> SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult {
        let target_timestamp = input.target_timestamp;
        let mode = input.mode;
        let left = self.single_client.select_from_handoff(
            handoff,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: input.left.client_id,
                run_id: input.left.run_id,
                target_timestamp,
                mode: single_client_mode,
            },
        );
        let right = self.single_client.select_from_handoff(
            handoff,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: input.right.client_id,
                run_id: input.right.run_id,
                target_timestamp,
                mode: single_client_mode,
            },
        );
        let status = two_view_target_time_handoff_source_scheduler_status(&left, &right);

        SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult {
            target_timestamp,
            mode,
            left,
            right,
            status,
        }
    }
}

/// Per-side instruction produced by adapting queue-backed scheduler output for
/// the existing decode/render input path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewSchedulerDecodeRenderSideInstruction {
    RenderFrame {
        side: SwitcherTwoViewSide,
        selected: SwitcherJitterBufferSelectedFrame,
        consumed: bool,
    },
    SkipNoFrameAvailable {
        side: SwitcherTwoViewSide,
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        client_queue_len: usize,
    },
    SkipWaitingForFrameAtOrBeforeTarget {
        side: SwitcherTwoViewSide,
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        candidate_frame_id: u64,
        candidate_capture_timestamp: TimestampMicros,
        client_queue_len: usize,
    },
}

/// Input for translating scheduler output into the existing decode/render path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewSchedulerDecodeRenderAdapterInput {
    pub scheduler_result: SwitcherTwoViewTargetTimeSourceSchedulerResult,
    pub left_window_title: String,
    pub right_window_title: String,
    pub render_hold_millis: u64,
}

/// Adapter output keeps explicit scheduler-side skip reasons while also
/// carrying the already-supported decode/render input shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewSchedulerDecodeRenderAdapterOutput {
    pub scheduler_status: SwitcherTwoViewTargetTimeSourceSchedulerStatus,
    pub left: SwitcherTwoViewSchedulerDecodeRenderSideInstruction,
    pub right: SwitcherTwoViewSchedulerDecodeRenderSideInstruction,
    pub decode_render_input: SwitcherTwoViewDecodeRenderInput,
}

/// Per-side instruction produced by adapting fallible handoff-backed scheduler
/// output for decode/render-facing code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction {
    RenderFrame {
        side: SwitcherTwoViewSide,
        selected: SwitcherJitterBufferSelectedFrame,
        consumed: bool,
    },
    SkipNoFrameAvailable {
        side: SwitcherTwoViewSide,
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        client_queue_len: usize,
    },
    SkipWaitingForFrameAtOrBeforeTarget {
        side: SwitcherTwoViewSide,
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        candidate_frame_id: u64,
        candidate_capture_timestamp: TimestampMicros,
        client_queue_len: usize,
    },
    SkipHandoffError {
        side: SwitcherTwoViewSide,
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        mode: SwitcherSingleClientTargetTimeSourceMode,
        handoff_mode: SwitcherSingleClientQueueSourceMode,
        error: SwitcherQueuedFrameHandoffError,
    },
}

/// Input for translating fallible handoff-backed scheduler output into
/// decode/render-facing instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
    pub scheduler_result: SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult,
    pub left_window_title: String,
    pub right_window_title: String,
    pub render_hold_millis: u64,
}

/// Adapter output keeps fallible scheduler outcomes explicit. The existing
/// decode/render input is present only when every skipped side can be
/// represented without hiding a handoff/source error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput {
    pub target_timestamp: TimestampMicros,
    pub scheduler_status: SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus,
    pub left: SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction,
    pub right: SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction,
    pub decode_render_input: Option<SwitcherTwoViewDecodeRenderInput>,
}

/// Skipped state produced by the fallible handoff scheduler decode/render
/// connection. This keeps source errors separate from ordinary no-frame and
/// waiting skips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewHandoffDecodeRenderSkippedSide {
    NoFrameAvailable {
        side: SwitcherTwoViewSide,
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        client_queue_len: usize,
    },
    WaitingForFrameAtOrBeforeTarget {
        side: SwitcherTwoViewSide,
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        candidate_frame_id: u64,
        candidate_capture_timestamp: TimestampMicros,
        client_queue_len: usize,
    },
    HandoffError {
        side: SwitcherTwoViewSide,
        client_id: ClientId,
        run_id: RunId,
        target_timestamp: TimestampMicros,
        mode: SwitcherSingleClientTargetTimeSourceMode,
        handoff_mode: SwitcherSingleClientQueueSourceMode,
        error: SwitcherQueuedFrameHandoffError,
    },
    DecodeRenderSkipped {
        side: SwitcherTwoViewSide,
        skipped: SwitcherTwoViewSkippedSide,
    },
}

/// Display-policy-facing result from fallible scheduler decode/render
/// connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewHandoffDecodeRenderConnectionResult {
    BothRendered {
        shared_target_time: TimestampMicros,
        left: SwitcherTwoViewRenderedSide,
        right: SwitcherTwoViewRenderedSide,
    },
    LeftRenderedRightSkipped {
        shared_target_time: TimestampMicros,
        left: SwitcherTwoViewRenderedSide,
        right: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
    },
    RightRenderedLeftSkipped {
        shared_target_time: TimestampMicros,
        left: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
        right: SwitcherTwoViewRenderedSide,
    },
    BothSkipped {
        shared_target_time: TimestampMicros,
        left: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
        right: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
    },
}

/// Input for running fallible adapter output through per-side decode/render
/// without hiding source-error skips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionInput {
    pub adapter_output: SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput,
    pub left_window_title: String,
    pub right_window_title: String,
    pub render_hold_millis: u64,
}

/// Output from fallible adapter output to display-policy-facing decode/render
/// result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput {
    pub scheduler_status: SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus,
    pub adapter: SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput,
    pub render: SwitcherTwoViewHandoffDecodeRenderConnectionResult,
}

/// Input for the smallest scheduler-result -> adapter -> decode/render
/// connection slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewSchedulerDecodeRenderConnectionInput {
    pub scheduler_result: SwitcherTwoViewTargetTimeSourceSchedulerResult,
    pub left_window_title: String,
    pub right_window_title: String,
    pub render_hold_millis: u64,
}

/// Output from running a scheduler result through the adapter and existing
/// decode/render boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewSchedulerDecodeRenderConnectionOutput {
    pub adapter: SwitcherTwoViewSchedulerDecodeRenderAdapterOutput,
    pub render: SwitcherTwoViewDecodeRenderResult,
}

/// Minimal adapter from queue-backed scheduler results to the existing
/// 2-view decode/render boundary input.
///
/// It performs no scheduling, queue mutation, decoding, rendering, display
/// policy, late-drop mutation, 4-view orchestration, or OBS work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary;

impl SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary {
    pub fn adapt(
        &self,
        input: SwitcherTwoViewSchedulerDecodeRenderAdapterInput,
    ) -> SwitcherTwoViewSchedulerDecodeRenderAdapterOutput {
        let scheduler_result = input.scheduler_result;
        let left = scheduler_decode_render_instruction_for_side(
            SwitcherTwoViewSide::Left,
            &scheduler_result.left,
        );
        let right = scheduler_decode_render_instruction_for_side(
            SwitcherTwoViewSide::Right,
            &scheduler_result.right,
        );
        let left_selection = scheduler_decode_render_instruction_to_selection(&left);
        let right_selection = scheduler_decode_render_instruction_to_selection(&right);
        let selection = scheduler_decode_render_selection_from_sides(
            scheduler_result.target_timestamp,
            left_selection,
            right_selection,
        );

        SwitcherTwoViewSchedulerDecodeRenderAdapterOutput {
            scheduler_status: scheduler_result.status,
            left,
            right,
            decode_render_input: SwitcherTwoViewDecodeRenderInput {
                selection,
                left_window_title: input.left_window_title,
                right_window_title: input.right_window_title,
                render_hold_millis: input.render_hold_millis,
            },
        }
    }
}

/// Minimal adapter from fallible handoff-backed scheduler results to
/// decode/render-facing instructions.
///
/// Handoff/source errors remain explicit `SkipHandoffError` instructions. When
/// either side has such an error, this adapter does not synthesize the existing
/// decode/render input because that shape cannot represent source errors
/// without collapsing them into no-frame or waiting states.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary;

impl SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary {
    pub fn adapt(
        &self,
        input: SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput,
    ) -> SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput {
        let scheduler_result = input.scheduler_result;
        let left = handoff_scheduler_decode_render_instruction_for_side(
            SwitcherTwoViewSide::Left,
            &scheduler_result.left,
        );
        let right = handoff_scheduler_decode_render_instruction_for_side(
            SwitcherTwoViewSide::Right,
            &scheduler_result.right,
        );
        let selection = handoff_scheduler_decode_render_selection_from_sides(
            scheduler_result.target_timestamp,
            handoff_scheduler_decode_render_instruction_to_selection(&left),
            handoff_scheduler_decode_render_instruction_to_selection(&right),
        );
        let decode_render_input = selection.map(|selection| SwitcherTwoViewDecodeRenderInput {
            selection,
            left_window_title: input.left_window_title,
            right_window_title: input.right_window_title,
            render_hold_millis: input.render_hold_millis,
        });

        SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput {
            target_timestamp: scheduler_result.target_timestamp,
            scheduler_status: scheduler_result.status,
            left,
            right,
            decode_render_input,
        }
    }
}

/// Minimal fallible connection from handoff scheduler adapter output to
/// per-side decode/render.
///
/// Only `RenderFrame` instructions reach decode/render hooks. No-frame,
/// waiting, and handoff/source-error instructions become explicit skipped
/// display-policy-facing results without creating fake frame inputs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionBoundary {
    decode_render: SwitcherTwoViewDecodeRenderBoundary,
}

impl SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionBoundary {
    pub fn render_adapter_output_with_runtimes(
        &self,
        input: SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionInput,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput {
        let shared_target_time = input.adapter_output.target_timestamp;
        let scheduler_status = input.adapter_output.scheduler_status;
        let left = self.render_instruction_with_runtimes(
            input.adapter_output.left.clone(),
            input.left_window_title,
            input.render_hold_millis,
            decode_runtime,
            render_runtime,
        );
        let right = self.render_instruction_with_runtimes(
            input.adapter_output.right.clone(),
            input.right_window_title,
            input.render_hold_millis,
            decode_runtime,
            render_runtime,
        );
        let render =
            handoff_decode_render_connection_result_from_sides(shared_target_time, left, right);

        SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput {
            scheduler_status,
            adapter: input.adapter_output,
            render,
        }
    }

    fn render_instruction_with_runtimes(
        &self,
        instruction: SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction,
        title: String,
        hold_millis: u64,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewHandoffSideDecodeRenderOutcome {
        match instruction {
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame {
                side,
                selected,
                ..
            } => {
                match self.decode_render.render_side_with_runtimes(
                    side,
                    SwitcherJitterBufferSelectionResult::Selected(selected),
                    title,
                    hold_millis,
                    decode_runtime,
                    render_runtime,
                ) {
                    SwitcherTwoViewSideDecodeRenderOutcome::Rendered(rendered) => {
                        SwitcherTwoViewHandoffSideDecodeRenderOutcome::Rendered(rendered)
                    }
                    SwitcherTwoViewSideDecodeRenderOutcome::Skipped(skipped) => {
                        SwitcherTwoViewHandoffSideDecodeRenderOutcome::Skipped(
                            SwitcherTwoViewHandoffDecodeRenderSkippedSide::DecodeRenderSkipped {
                                side,
                                skipped,
                            },
                        )
                    }
                }
            }
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable {
                side,
                client_id,
                run_id,
                target_timestamp,
                client_queue_len,
            } => SwitcherTwoViewHandoffSideDecodeRenderOutcome::Skipped(
                SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable {
                    side,
                    client_id,
                    run_id,
                    target_timestamp,
                    client_queue_len,
                },
            ),
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget {
                side,
                client_id,
                run_id,
                target_timestamp,
                candidate_frame_id,
                candidate_capture_timestamp,
                client_queue_len,
            } => SwitcherTwoViewHandoffSideDecodeRenderOutcome::Skipped(
                SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget {
                    side,
                    client_id,
                    run_id,
                    target_timestamp,
                    candidate_frame_id,
                    candidate_capture_timestamp,
                    client_queue_len,
                },
            ),
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipHandoffError {
                side,
                client_id,
                run_id,
                target_timestamp,
                mode,
                handoff_mode,
                error,
            } => SwitcherTwoViewHandoffSideDecodeRenderOutcome::Skipped(
                SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                    side,
                    client_id,
                    run_id,
                    target_timestamp,
                    mode,
                    handoff_mode,
                    error,
                },
            ),
        }
    }
}

/// Minimal in-process connection from scheduler output to the existing
/// decode/render boundary via the scheduler adapter.
///
/// This boundary validates wiring only. It does not own queue reads, alter
/// scheduler policy, invent fallback frames, or decide final display policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary {
    adapter: SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary,
    decode_render: SwitcherTwoViewDecodeRenderBoundary,
}

impl SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary {
    pub fn render_scheduler_result_with_runtimes(
        &self,
        input: SwitcherTwoViewSchedulerDecodeRenderConnectionInput,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewSchedulerDecodeRenderConnectionOutput {
        let adapter = self
            .adapter
            .adapt(SwitcherTwoViewSchedulerDecodeRenderAdapterInput {
                scheduler_result: input.scheduler_result,
                left_window_title: input.left_window_title,
                right_window_title: input.right_window_title,
                render_hold_millis: input.render_hold_millis,
            });
        let render = self.decode_render.render_selected_pair_with_runtimes(
            adapter.decode_render_input.clone(),
            decode_runtime,
            render_runtime,
        );

        SwitcherTwoViewSchedulerDecodeRenderConnectionOutput { adapter, render }
    }
}

fn scheduler_decode_render_instruction_for_side(
    side: SwitcherTwoViewSide,
    result: &SwitcherSingleClientTargetTimeSourceResult,
) -> SwitcherTwoViewSchedulerDecodeRenderSideInstruction {
    match result {
        SwitcherSingleClientTargetTimeSourceResult::Selected(selected) => {
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame {
                side,
                selected: SwitcherJitterBufferSelectedFrame {
                    frame: selected.frame.clone(),
                    target_time: selected.target_timestamp,
                    adjusted_capture_timestamp: selected.frame.capture_timestamp,
                    delta_from_target_micros: selected.delta_from_target_micros,
                },
                consumed: selected.consumed,
            }
        }
        SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable {
            client_id,
            run_id,
            target_timestamp,
            client_queue_len,
            ..
        } => SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable {
            side,
            client_id: client_id.clone(),
            run_id: run_id.clone(),
            target_timestamp: *target_timestamp,
            client_queue_len: *client_queue_len,
        },
        SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
            client_id,
            run_id,
            target_timestamp,
            candidate_frame_id,
            candidate_capture_timestamp,
            client_queue_len,
            ..
        } => {
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget {
                side,
                client_id: client_id.clone(),
                run_id: run_id.clone(),
                target_timestamp: *target_timestamp,
                candidate_frame_id: *candidate_frame_id,
                candidate_capture_timestamp: *candidate_capture_timestamp,
                client_queue_len: *client_queue_len,
            }
        }
    }
}

fn handoff_scheduler_decode_render_instruction_for_side(
    side: SwitcherTwoViewSide,
    result: &SwitcherSingleClientTargetTimeHandoffSourceResult,
) -> SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction {
    match result {
        SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(selected) => {
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame {
                side,
                selected: SwitcherJitterBufferSelectedFrame {
                    frame: selected.frame.clone(),
                    target_time: selected.target_timestamp,
                    adjusted_capture_timestamp: selected.frame.capture_timestamp,
                    delta_from_target_micros: selected.delta_from_target_micros,
                },
                consumed: selected.consumed,
            }
        }
        SwitcherSingleClientTargetTimeHandoffSourceResult::NoFrameAvailable {
            client_id,
            run_id,
            target_timestamp,
            client_queue_len,
            ..
        } => SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable {
            side,
            client_id: client_id.clone(),
            run_id: run_id.clone(),
            target_timestamp: *target_timestamp,
            client_queue_len: *client_queue_len,
        },
        SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget {
            client_id,
            run_id,
            target_timestamp,
            candidate_frame_id,
            candidate_capture_timestamp,
            client_queue_len,
            ..
        } => SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget {
            side,
            client_id: client_id.clone(),
            run_id: run_id.clone(),
            target_timestamp: *target_timestamp,
            candidate_frame_id: *candidate_frame_id,
            candidate_capture_timestamp: *candidate_capture_timestamp,
            client_queue_len: *client_queue_len,
        },
        SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError {
            client_id,
            run_id,
            target_timestamp,
            mode,
            handoff_mode,
            error,
        } => SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipHandoffError {
            side,
            client_id: client_id.clone(),
            run_id: run_id.clone(),
            target_timestamp: *target_timestamp,
            mode: *mode,
            handoff_mode: *handoff_mode,
            error: error.clone(),
        },
    }
}

fn scheduler_decode_render_instruction_to_selection(
    instruction: &SwitcherTwoViewSchedulerDecodeRenderSideInstruction,
) -> SwitcherJitterBufferSelectionResult {
    match instruction {
        SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { selected, .. } => {
            SwitcherJitterBufferSelectionResult::Selected(selected.clone())
        }
        SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable {
            client_id,
            target_timestamp,
            ..
        } => SwitcherJitterBufferSelectionResult::NoFrame {
            client_id: client_id.clone(),
            target_time: *target_timestamp,
        },
        SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget {
            client_id,
            target_timestamp,
            candidate_capture_timestamp,
            client_queue_len,
            ..
        } => SwitcherJitterBufferSelectionResult::FrameTooEarly {
            client_id: client_id.clone(),
            target_time: *target_timestamp,
            earliest_frame_time: *candidate_capture_timestamp,
            frames_available: *client_queue_len,
        },
    }
}

fn handoff_scheduler_decode_render_instruction_to_selection(
    instruction: &SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction,
) -> Option<SwitcherJitterBufferSelectionResult> {
    match instruction {
        SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame {
            selected,
            ..
        } => Some(SwitcherJitterBufferSelectionResult::Selected(
            selected.clone(),
        )),
        SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable {
            client_id,
            target_timestamp,
            ..
        } => Some(SwitcherJitterBufferSelectionResult::NoFrame {
            client_id: client_id.clone(),
            target_time: *target_timestamp,
        }),
        SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget {
            client_id,
            target_timestamp,
            candidate_capture_timestamp,
            client_queue_len,
            ..
        } => Some(SwitcherJitterBufferSelectionResult::FrameTooEarly {
            client_id: client_id.clone(),
            target_time: *target_timestamp,
            earliest_frame_time: *candidate_capture_timestamp,
            frames_available: *client_queue_len,
        }),
        SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipHandoffError { .. } => None,
    }
}

fn scheduler_decode_render_selection_from_sides(
    shared_target_time: TimestampMicros,
    left: SwitcherJitterBufferSelectionResult,
    right: SwitcherJitterBufferSelectionResult,
) -> SwitcherTwoViewTargetTimeSelectionResult {
    match (&left, &right) {
        (
            SwitcherJitterBufferSelectionResult::Selected(left_selected),
            SwitcherJitterBufferSelectionResult::Selected(right_selected),
        ) => SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
            shared_target_time,
            left: left_selected.clone(),
            right: right_selected.clone(),
        },
        (SwitcherJitterBufferSelectionResult::Selected(_), _)
        | (_, SwitcherJitterBufferSelectionResult::Selected(_)) => {
            SwitcherTwoViewTargetTimeSelectionResult::Partial {
                shared_target_time,
                left,
                right,
            }
        }
        _ => SwitcherTwoViewTargetTimeSelectionResult::BothUnavailable {
            shared_target_time,
            left,
            right,
        },
    }
}

fn handoff_scheduler_decode_render_selection_from_sides(
    shared_target_time: TimestampMicros,
    left: Option<SwitcherJitterBufferSelectionResult>,
    right: Option<SwitcherJitterBufferSelectionResult>,
) -> Option<SwitcherTwoViewTargetTimeSelectionResult> {
    let left = left?;
    let right = right?;
    Some(scheduler_decode_render_selection_from_sides(
        shared_target_time,
        left,
        right,
    ))
}

enum SwitcherTwoViewHandoffSideDecodeRenderOutcome {
    Rendered(SwitcherTwoViewRenderedSide),
    Skipped(SwitcherTwoViewHandoffDecodeRenderSkippedSide),
}

fn handoff_decode_render_connection_result_from_sides(
    shared_target_time: TimestampMicros,
    left: SwitcherTwoViewHandoffSideDecodeRenderOutcome,
    right: SwitcherTwoViewHandoffSideDecodeRenderOutcome,
) -> SwitcherTwoViewHandoffDecodeRenderConnectionResult {
    match (left, right) {
        (
            SwitcherTwoViewHandoffSideDecodeRenderOutcome::Rendered(left),
            SwitcherTwoViewHandoffSideDecodeRenderOutcome::Rendered(right),
        ) => SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothRendered {
            shared_target_time,
            left,
            right,
        },
        (
            SwitcherTwoViewHandoffSideDecodeRenderOutcome::Rendered(left),
            SwitcherTwoViewHandoffSideDecodeRenderOutcome::Skipped(right),
        ) => SwitcherTwoViewHandoffDecodeRenderConnectionResult::LeftRenderedRightSkipped {
            shared_target_time,
            left,
            right,
        },
        (
            SwitcherTwoViewHandoffSideDecodeRenderOutcome::Skipped(left),
            SwitcherTwoViewHandoffSideDecodeRenderOutcome::Rendered(right),
        ) => SwitcherTwoViewHandoffDecodeRenderConnectionResult::RightRenderedLeftSkipped {
            shared_target_time,
            left,
            right,
        },
        (
            SwitcherTwoViewHandoffSideDecodeRenderOutcome::Skipped(left),
            SwitcherTwoViewHandoffSideDecodeRenderOutcome::Skipped(right),
        ) => SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothSkipped {
            shared_target_time,
            left,
            right,
        },
    }
}

/// Minimal diagnostic 2-view scheduler over the single-client targetTime source.
///
/// This boundary calls `SwitcherSingleClientTargetTimeSourceBoundary` once for
/// each configured view with the same target timestamp and explicit source mode.
/// Preview mode remains non-mutating. Consume mode is scheduler-level
/// all-or-nothing: both oldest candidates are previewed first, and no queue is
/// mutated unless both views have eligible frames. It does not decode, render,
/// perform 4-view orchestration, mutate late frames, or integrate OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewTargetTimeSourceSchedulerBoundary {
    single_client: SwitcherSingleClientTargetTimeSourceBoundary,
}

impl SwitcherTwoViewTargetTimeSourceSchedulerBoundary {
    pub fn select_pair(
        &self,
        queue_state: &mut ServerVideoFrameQueueState,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
    ) -> SwitcherTwoViewTargetTimeSourceSchedulerResult {
        let mut source = SwitcherInProcessServerQueueFrameSource::new(queue_state);
        self.select_pair_from_source(&mut source, input)
    }

    pub fn select_pair_from_source(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
    ) -> SwitcherTwoViewTargetTimeSourceSchedulerResult {
        match input.mode {
            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore => {
                self.preview_latest_pair(source, input)
            }
            SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected => {
                self.consume_oldest_pair_all_selected(source, input)
            }
        }
    }

    fn preview_latest_pair(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
    ) -> SwitcherTwoViewTargetTimeSourceSchedulerResult {
        self.select_pair_with_single_client_mode(
            source,
            input,
            SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
        )
    }

    fn consume_oldest_pair_all_selected(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
    ) -> SwitcherTwoViewTargetTimeSourceSchedulerResult {
        let preview = self.select_pair_with_single_client_mode(
            source,
            input.clone(),
            SwitcherSingleClientTargetTimeSourceMode::PreviewOldestIfAtOrBefore,
        );

        if preview.status != SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected {
            return preview;
        }

        self.select_pair_with_single_client_mode(
            source,
            input,
            SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
        )
    }

    fn select_pair_with_single_client_mode(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherTwoViewTargetTimeSourceSchedulerInput,
        single_client_mode: SwitcherSingleClientTargetTimeSourceMode,
    ) -> SwitcherTwoViewTargetTimeSourceSchedulerResult {
        let target_timestamp = input.target_timestamp;
        let mode = input.mode;
        let left = self.single_client.select_from_source(
            source,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: input.left.client_id,
                run_id: input.left.run_id,
                target_timestamp,
                mode: single_client_mode,
            },
        );
        let right = self.single_client.select_from_source(
            source,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: input.right.client_id,
                run_id: input.right.run_id,
                target_timestamp,
                mode: single_client_mode,
            },
        );
        let status = two_view_target_time_source_scheduler_status(&left, &right);

        SwitcherTwoViewTargetTimeSourceSchedulerResult {
            target_timestamp,
            mode,
            left,
            right,
            status,
        }
    }
}

fn two_view_target_time_source_scheduler_status(
    left: &SwitcherSingleClientTargetTimeSourceResult,
    right: &SwitcherSingleClientTargetTimeSourceResult,
) -> SwitcherTwoViewTargetTimeSourceSchedulerStatus {
    let left_selected = matches!(
        left,
        SwitcherSingleClientTargetTimeSourceResult::Selected(_)
    );
    let right_selected = matches!(
        right,
        SwitcherSingleClientTargetTimeSourceResult::Selected(_)
    );

    match (left_selected, right_selected) {
        (true, true) => SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected,
        (true, false) | (false, true) => {
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::PartialSelected
        }
        (false, false)
            if matches!(
                left,
                SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
            ) || matches!(
                right,
                SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
            ) =>
        {
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::Waiting
        }
        (false, false) => SwitcherTwoViewTargetTimeSourceSchedulerStatus::NoFrames,
    }
}

fn two_view_target_time_handoff_source_scheduler_status(
    left: &SwitcherSingleClientTargetTimeHandoffSourceResult,
    right: &SwitcherSingleClientTargetTimeHandoffSourceResult,
) -> SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus {
    if matches!(
        left,
        SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError { .. }
    ) || matches!(
        right,
        SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError { .. }
    ) {
        return SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError;
    }

    let left_selected = matches!(
        left,
        SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(_)
    );
    let right_selected = matches!(
        right,
        SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(_)
    );

    match (left_selected, right_selected) {
        (true, true) => SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected,
        (true, false) | (false, true) => {
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::PartialSelected
        }
        (false, false)
            if matches!(
                left,
                SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget {
                    ..
                }
            ) || matches!(
                right,
                SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget {
                    ..
                }
            ) =>
        {
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::Waiting
        }
        (false, false) => SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::NoFrames,
    }
}

/// Deterministic targetTime calculation input for one switcher selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitcherTargetTimeInput {
    pub current_switcher_time: TimestampMicros,
    pub playout_delay_micros: u64,
    pub clock_offset_micros: Option<i64>,
}

/// Calculated targetTime in the switcher/server time domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitcherTargetTime {
    pub value: TimestampMicros,
}

/// Minimal targetTime policy.
///
/// The selector looks for frames whose adjusted capture timestamp is inside:
/// `targetTime - max_late_micros ..= targetTime + max_early_micros`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitcherJitterBufferSelectionPolicy {
    pub playout_delay_micros: u64,
    pub clock_offset_micros: Option<i64>,
    pub max_late_micros: u64,
    pub max_early_micros: u64,
    pub min_buffer_frames: usize,
}

impl Default for SwitcherJitterBufferSelectionPolicy {
    fn default() -> Self {
        Self {
            playout_delay_micros: 500_000,
            clock_offset_micros: None,
            max_late_micros: 250_000,
            max_early_micros: 33_334,
            min_buffer_frames: 1,
        }
    }
}

/// Input for one targetTime / jitter-buffer selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherJitterBufferSelectionInput<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub client_id: &'a ClientId,
    pub current_switcher_time: TimestampMicros,
    pub policy: SwitcherJitterBufferSelectionPolicy,
}

/// Shared 2-view targetTime selection policy.
///
/// The shared targetTime is calculated once from switcher time and playout
/// delay. Per-client offsets are applied only when each client's capture
/// timestamps are compared with that shared target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewTargetTimeSelectionPolicy {
    pub playout_delay_micros: u64,
    pub left_clock_offset_micros: Option<i64>,
    pub right_clock_offset_micros: Option<i64>,
    pub max_late_micros: u64,
    pub max_early_micros: u64,
    pub min_buffer_frames: usize,
}

impl Default for SwitcherTwoViewTargetTimeSelectionPolicy {
    fn default() -> Self {
        let single = SwitcherJitterBufferSelectionPolicy::default();
        Self {
            playout_delay_micros: single.playout_delay_micros,
            left_clock_offset_micros: None,
            right_clock_offset_micros: None,
            max_late_micros: single.max_late_micros,
            max_early_micros: single.max_early_micros,
            min_buffer_frames: single.min_buffer_frames,
        }
    }
}

impl SwitcherTwoViewTargetTimeSelectionPolicy {
    fn per_client_policy(
        &self,
        clock_offset_micros: Option<i64>,
    ) -> SwitcherJitterBufferSelectionPolicy {
        SwitcherJitterBufferSelectionPolicy {
            playout_delay_micros: self.playout_delay_micros,
            clock_offset_micros,
            max_late_micros: self.max_late_micros,
            max_early_micros: self.max_early_micros,
            min_buffer_frames: self.min_buffer_frames,
        }
    }
}

/// Input for pure 2-view targetTime selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherTwoViewTargetTimeSelectionInput<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub left_client_id: &'a ClientId,
    pub right_client_id: &'a ClientId,
    pub current_switcher_time: TimestampMicros,
    pub policy: SwitcherTwoViewTargetTimeSelectionPolicy,
}

/// Details for a selected frame's timing relationship to targetTime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherJitterBufferSelectedFrame {
    pub frame: SwitcherSingleViewSelectedEncodedFrame,
    pub target_time: TimestampMicros,
    pub adjusted_capture_timestamp: TimestampMicros,
    pub delta_from_target_micros: i64,
}

/// Result of one targetTime / jitter-buffer frame selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherJitterBufferSelectionResult {
    Selected(SwitcherJitterBufferSelectedFrame),
    NoFrame {
        client_id: ClientId,
        target_time: TimestampMicros,
    },
    WaitingForBuffer {
        client_id: ClientId,
        target_time: TimestampMicros,
        available_frames: usize,
        min_buffer_frames: usize,
    },
    FrameTooEarly {
        client_id: ClientId,
        target_time: TimestampMicros,
        earliest_frame_time: TimestampMicros,
        frames_available: usize,
    },
    FrameTooLateDropped {
        client_id: ClientId,
        target_time: TimestampMicros,
        latest_frame_time: TimestampMicros,
        late_frames: Vec<SwitcherSingleViewSelectedEncodedFrame>,
    },
}

/// Result of selecting two clients against one shared targetTime.
///
/// This result remains encoded-frame selection only. It does not decode,
/// render, mutate queues, or perform multi-view composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewTargetTimeSelectionResult {
    BothSelected {
        shared_target_time: TimestampMicros,
        left: SwitcherJitterBufferSelectedFrame,
        right: SwitcherJitterBufferSelectedFrame,
    },
    Partial {
        shared_target_time: TimestampMicros,
        left: SwitcherJitterBufferSelectionResult,
        right: SwitcherJitterBufferSelectionResult,
    },
    BothUnavailable {
        shared_target_time: TimestampMicros,
        left: SwitcherJitterBufferSelectionResult,
        right: SwitcherJitterBufferSelectionResult,
    },
}

/// Pure targetTime calculator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTargetTimeBoundary;

impl SwitcherTargetTimeBoundary {
    pub fn calculate(&self, input: SwitcherTargetTimeInput) -> SwitcherTargetTime {
        let base = input
            .current_switcher_time
            .0
            .saturating_sub(input.playout_delay_micros);
        SwitcherTargetTime {
            value: TimestampMicros(apply_offset_micros(base, input.clock_offset_micros)),
        }
    }
}

/// Read-only targetTime / jitter-buffer selector for one client.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherJitterBufferSelectionBoundary {
    target_time: SwitcherTargetTimeBoundary,
}

impl SwitcherJitterBufferSelectionBoundary {
    pub fn select_frame(
        &self,
        input: SwitcherJitterBufferSelectionInput<'_>,
    ) -> SwitcherJitterBufferSelectionResult {
        let target_time = self
            .target_time
            .calculate(SwitcherTargetTimeInput {
                current_switcher_time: input.current_switcher_time,
                playout_delay_micros: input.policy.playout_delay_micros,
                clock_offset_micros: input.policy.clock_offset_micros,
            })
            .value;
        self.select_frame_at_target_time(input, target_time)
    }

    pub fn select_frame_at_target_time(
        &self,
        input: SwitcherJitterBufferSelectionInput<'_>,
        target_time: TimestampMicros,
    ) -> SwitcherJitterBufferSelectionResult {
        let frames: Vec<SwitcherSingleViewSelectedEncodedFrame> = input
            .queue_state
            .frames_for_client(input.client_id)
            .map(SwitcherSingleViewSelectedEncodedFrame::from)
            .collect();

        if frames.is_empty() {
            return SwitcherJitterBufferSelectionResult::NoFrame {
                client_id: input.client_id.clone(),
                target_time,
            };
        }
        if frames.len() < input.policy.min_buffer_frames {
            return SwitcherJitterBufferSelectionResult::WaitingForBuffer {
                client_id: input.client_id.clone(),
                target_time,
                available_frames: frames.len(),
                min_buffer_frames: input.policy.min_buffer_frames,
            };
        }

        let lower = target_time.0.saturating_sub(input.policy.max_late_micros);
        let upper = target_time.0.saturating_add(input.policy.max_early_micros);
        let mut timed_frames: Vec<(SwitcherSingleViewSelectedEncodedFrame, TimestampMicros)> =
            frames
                .into_iter()
                .map(|frame| {
                    let adjusted = TimestampMicros(apply_offset_micros(
                        frame.capture_timestamp.0,
                        input.policy.clock_offset_micros,
                    ));
                    (frame, adjusted)
                })
                .collect();
        timed_frames.sort_by_key(|(_, adjusted)| adjusted.0);

        let selected = timed_frames
            .iter()
            .filter(|(_, adjusted)| adjusted.0 >= lower && adjusted.0 <= upper)
            .min_by_key(|(_, adjusted)| adjusted.0.abs_diff(target_time.0));

        if let Some((frame, adjusted)) = selected {
            return SwitcherJitterBufferSelectionResult::Selected(
                SwitcherJitterBufferSelectedFrame {
                    frame: frame.clone(),
                    target_time,
                    adjusted_capture_timestamp: *adjusted,
                    delta_from_target_micros: adjusted.0 as i64 - target_time.0 as i64,
                },
            );
        }

        let earliest = timed_frames
            .first()
            .expect("non-empty timed frames should have first");
        let latest = timed_frames
            .last()
            .expect("non-empty timed frames should have last");
        if earliest.1 .0 > upper {
            return SwitcherJitterBufferSelectionResult::FrameTooEarly {
                client_id: input.client_id.clone(),
                target_time,
                earliest_frame_time: earliest.1,
                frames_available: timed_frames.len(),
            };
        }

        let late_frames = timed_frames
            .iter()
            .filter(|(_, adjusted)| adjusted.0 < lower)
            .map(|(frame, _)| frame.clone())
            .collect();
        SwitcherJitterBufferSelectionResult::FrameTooLateDropped {
            client_id: input.client_id.clone(),
            target_time,
            latest_frame_time: latest.1,
            late_frames,
        }
    }
}

/// Pure 2-view targetTime selector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewTargetTimeSelectionBoundary {
    target_time: SwitcherTargetTimeBoundary,
    per_client_selector: SwitcherJitterBufferSelectionBoundary,
}

impl SwitcherTwoViewTargetTimeSelectionBoundary {
    pub fn select_pair(
        &self,
        input: SwitcherTwoViewTargetTimeSelectionInput<'_>,
    ) -> SwitcherTwoViewTargetTimeSelectionResult {
        let shared_target_time = self
            .target_time
            .calculate(SwitcherTargetTimeInput {
                current_switcher_time: input.current_switcher_time,
                playout_delay_micros: input.policy.playout_delay_micros,
                clock_offset_micros: None,
            })
            .value;
        let left = self.per_client_selector.select_frame_at_target_time(
            SwitcherJitterBufferSelectionInput {
                queue_state: input.queue_state,
                client_id: input.left_client_id,
                current_switcher_time: input.current_switcher_time,
                policy: input
                    .policy
                    .per_client_policy(input.policy.left_clock_offset_micros),
            },
            shared_target_time,
        );
        let right = self.per_client_selector.select_frame_at_target_time(
            SwitcherJitterBufferSelectionInput {
                queue_state: input.queue_state,
                client_id: input.right_client_id,
                current_switcher_time: input.current_switcher_time,
                policy: input
                    .policy
                    .per_client_policy(input.policy.right_clock_offset_micros),
            },
            shared_target_time,
        );

        match (&left, &right) {
            (
                SwitcherJitterBufferSelectionResult::Selected(left_selected),
                SwitcherJitterBufferSelectionResult::Selected(right_selected),
            ) => SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
                shared_target_time,
                left: left_selected.clone(),
                right: right_selected.clone(),
            },
            (SwitcherJitterBufferSelectionResult::Selected(_), _)
            | (_, SwitcherJitterBufferSelectionResult::Selected(_)) => {
                SwitcherTwoViewTargetTimeSelectionResult::Partial {
                    shared_target_time,
                    left,
                    right,
                }
            }
            _ => SwitcherTwoViewTargetTimeSelectionResult::BothUnavailable {
                shared_target_time,
                left,
                right,
            },
        }
    }
}

fn apply_offset_micros(value: u64, offset: Option<i64>) -> u64 {
    match offset {
        Some(offset) if offset < 0 => value.saturating_sub(offset.unsigned_abs()),
        Some(offset) => value.saturating_add(offset as u64),
        None => value,
    }
}

/// Explicit placeholder status for the future H.264 decode step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherSingleViewDecodeStatus {
    DeferredPlaceholder,
    Decoded,
    DecodeDeferred,
    DecodeFailed,
}

/// Placeholder display handoff for a selected single-view frame.
///
/// This is display-ready only in the sense that a future display owner can see
/// which encoded frame would be shown. It does not contain decoded pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSingleViewDisplayPlaceholderHandoff {
    pub selected: SwitcherSingleViewSelectedEncodedFrame,
    pub decode_status: SwitcherSingleViewDecodeStatus,
}

/// Pixel format produced by the first switcher decode PoC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherDecodedFramePixelFormat {
    Bgra8,
}

/// One decoded video frame ready for a future real display path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherDecodedFrame {
    pub width: u32,
    pub height: u32,
    pub pixel_format: SwitcherDecodedFramePixelFormat,
    pub pixels: Vec<u8>,
}

/// Input for decoding one Annex B H.264 encoded frame payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherH264DecodeInput {
    pub encoded_payload: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Deferred reason for switcher-side H.264 decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherH264DecodeDeferredReason {
    EmptyPayload,
    InvalidDimensions,
    FfmpegUnavailable,
}

/// Failure details for switcher-side H.264 decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherH264DecodeFailure {
    pub message: String,
}

/// Result of attempting one H.264 decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherH264DecodeResult {
    Decoded(SwitcherDecodedFrame),
    Deferred {
        reason: SwitcherH264DecodeDeferredReason,
    },
    Failed(SwitcherH264DecodeFailure),
}

/// Runtime hook for H.264 decode.
///
/// This keeps the boundary testable and leaves future library/hardware decode
/// integration caller-owned.
pub trait SwitcherH264DecodeRuntimeHook {
    fn decode_annex_b_h264(&self, input: SwitcherH264DecodeInput) -> SwitcherH264DecodeResult;
}

/// Placeholder-safe default decode runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherDeferredH264DecodeRuntimeHook;

impl SwitcherH264DecodeRuntimeHook for SwitcherDeferredH264DecodeRuntimeHook {
    fn decode_annex_b_h264(&self, _input: SwitcherH264DecodeInput) -> SwitcherH264DecodeResult {
        SwitcherH264DecodeResult::Deferred {
            reason: SwitcherH264DecodeDeferredReason::FfmpegUnavailable,
        }
    }
}

/// Minimal FFmpeg CLI H.264 decoder runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherFfmpegH264DecodeRuntimeHook {
    pub ffmpeg_path: PathBuf,
}

impl Default for SwitcherFfmpegH264DecodeRuntimeHook {
    fn default() -> Self {
        Self {
            ffmpeg_path: PathBuf::from("ffmpeg"),
        }
    }
}

impl SwitcherH264DecodeRuntimeHook for SwitcherFfmpegH264DecodeRuntimeHook {
    fn decode_annex_b_h264(&self, input: SwitcherH264DecodeInput) -> SwitcherH264DecodeResult {
        if input.encoded_payload.is_empty() {
            return SwitcherH264DecodeResult::Deferred {
                reason: SwitcherH264DecodeDeferredReason::EmptyPayload,
            };
        }
        if input.width == 0 || input.height == 0 {
            return SwitcherH264DecodeResult::Deferred {
                reason: SwitcherH264DecodeDeferredReason::InvalidDimensions,
            };
        }

        let mut child = match Command::new(&self.ffmpeg_path)
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-f")
            .arg("h264")
            .arg("-i")
            .arg("pipe:0")
            .arg("-frames:v")
            .arg("1")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("bgra")
            .arg("pipe:1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return SwitcherH264DecodeResult::Deferred {
                    reason: SwitcherH264DecodeDeferredReason::FfmpegUnavailable,
                };
            }
            Err(error) => {
                return SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                    message: error.to_string(),
                });
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = stdin.write_all(&input.encoded_payload) {
                return SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                    message: error.to_string(),
                });
            }
        }

        let output = match child.wait_with_output() {
            Ok(output) => output,
            Err(error) => {
                return SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                    message: error.to_string(),
                });
            }
        };

        if !output.status.success() {
            return SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let expected_len = input.width as usize * input.height as usize * 4;
        if output.stdout.len() != expected_len {
            return SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                message: format!(
                    "decoded rawvideo length mismatch expected={} actual={}",
                    expected_len,
                    output.stdout.len()
                ),
            });
        }

        SwitcherH264DecodeResult::Decoded(SwitcherDecodedFrame {
            width: input.width,
            height: input.height,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: output.stdout,
        })
    }
}

/// Boundary for decoding one selected H.264 frame payload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherH264DecodeBoundary;

impl SwitcherH264DecodeBoundary {
    pub fn decode_with_runtime(
        &self,
        input: SwitcherH264DecodeInput,
        runtime: &impl SwitcherH264DecodeRuntimeHook,
    ) -> SwitcherH264DecodeResult {
        runtime.decode_annex_b_h264(input)
    }
}

/// Real decoded display handoff for a selected single-view frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSingleViewDisplayRealFrameHandoff {
    pub selected: SwitcherSingleViewSelectedEncodedFrame,
    pub decoded: SwitcherDecodedFrame,
    pub decode_status: SwitcherSingleViewDecodeStatus,
}

/// Result of preparing a single-view display handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherSingleViewDisplayHandoffResult {
    DisplayReadyRealFrame(SwitcherSingleViewDisplayRealFrameHandoff),
    DisplayReadyPlaceholder(SwitcherSingleViewDisplayPlaceholderHandoff),
    NoFrameAvailable { client_id: ClientId },
}

/// Placeholder decode/display boundary for the single-view PoC.
///
/// This boundary preserves the selected encoded frame and marks decode as
/// deferred. It does not call FFmpeg, allocate pixel buffers, render UI, sync
/// frames, or integrate with OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleViewPlaceholderDisplayBoundary;

impl SwitcherSingleViewPlaceholderDisplayBoundary {
    pub fn prepare_handoff(
        &self,
        selection: SwitcherSingleViewFrameSelectionResult,
    ) -> SwitcherSingleViewDisplayHandoffResult {
        match selection {
            SwitcherSingleViewFrameSelectionResult::FrameAvailable(selected) => {
                SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(
                    SwitcherSingleViewDisplayPlaceholderHandoff {
                        selected,
                        decode_status: SwitcherSingleViewDecodeStatus::DeferredPlaceholder,
                    },
                )
            }
            SwitcherSingleViewFrameSelectionResult::NoFrameAvailable { client_id } => {
                SwitcherSingleViewDisplayHandoffResult::NoFrameAvailable { client_id }
            }
        }
    }

    pub fn prepare_handoff_with_decode(
        &self,
        selection: SwitcherSingleViewFrameSelectionResult,
        decoder: &SwitcherH264DecodeBoundary,
        runtime: &impl SwitcherH264DecodeRuntimeHook,
    ) -> SwitcherSingleViewDisplayHandoffResult {
        match selection {
            SwitcherSingleViewFrameSelectionResult::FrameAvailable(selected) => {
                let decode = decoder.decode_with_runtime(
                    SwitcherH264DecodeInput {
                        encoded_payload: selected.encoded_payload.clone(),
                        width: selected.width,
                        height: selected.height,
                    },
                    runtime,
                );
                match decode {
                    SwitcherH264DecodeResult::Decoded(decoded) => {
                        SwitcherSingleViewDisplayHandoffResult::DisplayReadyRealFrame(
                            SwitcherSingleViewDisplayRealFrameHandoff {
                                selected,
                                decoded,
                                decode_status: SwitcherSingleViewDecodeStatus::Decoded,
                            },
                        )
                    }
                    SwitcherH264DecodeResult::Deferred { .. } => {
                        SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(
                            SwitcherSingleViewDisplayPlaceholderHandoff {
                                selected,
                                decode_status: SwitcherSingleViewDecodeStatus::DecodeDeferred,
                            },
                        )
                    }
                    SwitcherH264DecodeResult::Failed(_) => {
                        SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(
                            SwitcherSingleViewDisplayPlaceholderHandoff {
                                selected,
                                decode_status: SwitcherSingleViewDecodeStatus::DecodeFailed,
                            },
                        )
                    }
                }
            }
            SwitcherSingleViewFrameSelectionResult::NoFrameAvailable { client_id } => {
                SwitcherSingleViewDisplayHandoffResult::NoFrameAvailable { client_id }
            }
        }
    }
}

/// Thin composition for the current single-view placeholder path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleViewPlaceholderPathBoundary {
    selection: SwitcherSingleViewLatestFrameSelectionBoundary,
    display: SwitcherSingleViewPlaceholderDisplayBoundary,
}

impl SwitcherSingleViewPlaceholderPathBoundary {
    pub fn prepare_latest_display_handoff(
        &self,
        input: SwitcherSingleViewFrameSelectionInput<'_>,
    ) -> SwitcherSingleViewDisplayHandoffResult {
        let selected = self.selection.select_latest(input);
        self.display.prepare_handoff(selected)
    }

    pub fn prepare_latest_display_handoff_with_decode(
        &self,
        input: SwitcherSingleViewFrameSelectionInput<'_>,
        decoder: &SwitcherH264DecodeBoundary,
        runtime: &impl SwitcherH264DecodeRuntimeHook,
    ) -> SwitcherSingleViewDisplayHandoffResult {
        let selected = self.selection.select_latest(input);
        self.display
            .prepare_handoff_with_decode(selected, decoder, runtime)
    }
}

/// Result of dumping one decoded frame to a simple file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherDecodedFrameDump {
    pub path: PathBuf,
    pub bytes_written: usize,
}

/// Error from decoded frame dump output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherDecodedFrameDumpError {
    InvalidDimensions,
    InvalidBufferLength { expected: usize, actual: usize },
    Io { path: PathBuf, kind: io::ErrorKind },
}

/// Minimal frame dump writer for the first real display PoC.
///
/// It writes a 32-bit BMP from BGRA pixels. This is intentionally a file-output
/// display substitute and does not open a window or integrate with OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherDecodedFrameDumpBoundary;

impl SwitcherDecodedFrameDumpBoundary {
    pub fn write_bmp(
        &self,
        frame: &SwitcherDecodedFrame,
        path: impl AsRef<Path>,
    ) -> Result<SwitcherDecodedFrameDump, SwitcherDecodedFrameDumpError> {
        let path = path.as_ref();
        if frame.width == 0 || frame.height == 0 {
            return Err(SwitcherDecodedFrameDumpError::InvalidDimensions);
        }
        let expected = frame.width as usize * frame.height as usize * 4;
        if frame.pixels.len() != expected {
            return Err(SwitcherDecodedFrameDumpError::InvalidBufferLength {
                expected,
                actual: frame.pixels.len(),
            });
        }

        let width = frame.width as usize;
        let height = frame.height as usize;
        let pixel_bytes_len = expected;
        let file_size = 14 + 40 + pixel_bytes_len;
        let mut output = Vec::with_capacity(file_size);
        output.extend_from_slice(b"BM");
        output.extend_from_slice(&(file_size as u32).to_le_bytes());
        output.extend_from_slice(&[0, 0, 0, 0]);
        output.extend_from_slice(&(54_u32).to_le_bytes());
        output.extend_from_slice(&(40_u32).to_le_bytes());
        output.extend_from_slice(&(frame.width as i32).to_le_bytes());
        output.extend_from_slice(&(frame.height as i32).to_le_bytes());
        output.extend_from_slice(&(1_u16).to_le_bytes());
        output.extend_from_slice(&(32_u16).to_le_bytes());
        output.extend_from_slice(&(0_u32).to_le_bytes());
        output.extend_from_slice(&(pixel_bytes_len as u32).to_le_bytes());
        output.extend_from_slice(&(2_835_i32).to_le_bytes());
        output.extend_from_slice(&(2_835_i32).to_le_bytes());
        output.extend_from_slice(&(0_u32).to_le_bytes());
        output.extend_from_slice(&(0_u32).to_le_bytes());

        for row in (0..height).rev() {
            let start = row * width * 4;
            let end = start + width * 4;
            output.extend_from_slice(&frame.pixels[start..end]);
        }

        std::fs::write(path, &output).map_err(|error| SwitcherDecodedFrameDumpError::Io {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;

        Ok(SwitcherDecodedFrameDump {
            path: path.to_path_buf(),
            bytes_written: output.len(),
        })
    }
}

/// Validated render input derived from one decoded BGRA frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherDecodedFrameRenderInput {
    pub width: u32,
    pub height: u32,
    pub pixel_format: SwitcherDecodedFramePixelFormat,
    pub pixels: Vec<u8>,
}

/// Invalid decoded frame reason for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherDecodedFrameRenderInputError {
    UnsupportedPixelFormat {
        actual: SwitcherDecodedFramePixelFormat,
    },
    InvalidDimensions,
    InvalidBufferLength {
        expected: usize,
        actual: usize,
    },
}

impl SwitcherDecodedFrameRenderInput {
    pub fn from_decoded_frame(
        frame: &SwitcherDecodedFrame,
    ) -> Result<Self, SwitcherDecodedFrameRenderInputError> {
        if frame.pixel_format != SwitcherDecodedFramePixelFormat::Bgra8 {
            return Err(
                SwitcherDecodedFrameRenderInputError::UnsupportedPixelFormat {
                    actual: frame.pixel_format,
                },
            );
        }
        if frame.width == 0 || frame.height == 0 {
            return Err(SwitcherDecodedFrameRenderInputError::InvalidDimensions);
        }
        let expected = frame.width as usize * frame.height as usize * 4;
        if frame.pixels.len() != expected {
            return Err(SwitcherDecodedFrameRenderInputError::InvalidBufferLength {
                expected,
                actual: frame.pixels.len(),
            });
        }

        Ok(Self {
            width: frame.width,
            height: frame.height,
            pixel_format: frame.pixel_format,
            pixels: frame.pixels.clone(),
        })
    }
}

/// Request for rendering one decoded frame to a switcher window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherWindowRenderRequest {
    pub frame: SwitcherDecodedFrameRenderInput,
    pub title: String,
    pub hold_millis: u64,
}

/// Successful one-shot window render summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherWindowRenderSuccess {
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub hold_millis: u64,
}

/// Explicit window-render deferred reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherWindowRenderDeferredReason {
    NotImplemented,
}

/// Explicit window-render unavailable reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherWindowBackendUnavailableReason {
    UnsupportedPlatform,
}

/// Result of rendering one decoded frame to a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherWindowRenderResult {
    Rendered(SwitcherWindowRenderSuccess),
    RenderDeferred {
        reason: SwitcherWindowRenderDeferredReason,
    },
    BackendUnavailable {
        reason: SwitcherWindowBackendUnavailableReason,
        message: Option<String>,
    },
    InvalidFrame {
        error: SwitcherDecodedFrameRenderInputError,
    },
    RenderFailed {
        message: String,
    },
}

/// Caller-owned runtime hook for one-shot window rendering.
pub trait SwitcherWindowRenderRuntimeHook {
    fn render_once(&self, request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult;
}

/// Placeholder-safe renderer used when no platform window backend is supplied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherUnavailableWindowRenderRuntimeHook;

impl SwitcherWindowRenderRuntimeHook for SwitcherUnavailableWindowRenderRuntimeHook {
    fn render_once(&self, _request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
        SwitcherWindowRenderResult::BackendUnavailable {
            reason: SwitcherWindowBackendUnavailableReason::UnsupportedPlatform,
            message: Some("switcher window rendering backend is unavailable".to_string()),
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherWindowsGdiWindowRenderRuntimeHook;

#[cfg(target_os = "windows")]
impl SwitcherWindowRenderRuntimeHook for SwitcherWindowsGdiWindowRenderRuntimeHook {
    fn render_once(&self, request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
        windows_render_once(request)
    }
}

/// Boundary for rendering one decoded frame to a switcher window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherWindowRenderBoundary;

impl SwitcherWindowRenderBoundary {
    pub fn render_decoded_frame_with_runtime(
        &self,
        frame: &SwitcherDecodedFrame,
        title: impl Into<String>,
        hold_millis: u64,
        runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherWindowRenderResult {
        let render_input = match SwitcherDecodedFrameRenderInput::from_decoded_frame(frame) {
            Ok(input) => input,
            Err(error) => return SwitcherWindowRenderResult::InvalidFrame { error },
        };
        runtime.render_once(SwitcherWindowRenderRequest {
            frame: render_input,
            title: title.into(),
            hold_millis,
        })
    }
}

/// Side identifier for the first 2-view switcher composition boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherTwoViewSide {
    Left,
    Right,
}

/// Input for connecting 2-view targetTime selection to decode/render.
///
/// This owns a completed selection result. It does not read or mutate queues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewDecodeRenderInput {
    pub selection: SwitcherTwoViewTargetTimeSelectionResult,
    pub left_window_title: String,
    pub right_window_title: String,
    pub render_hold_millis: u64,
}

/// Successful render for one side of the 2-view connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewRenderedSide {
    pub side: SwitcherTwoViewSide,
    pub selected: SwitcherJitterBufferSelectedFrame,
    pub decoded: SwitcherDecodedFrame,
    pub render: SwitcherWindowRenderSuccess,
}

/// Explicit skipped state for one side of the 2-view connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewSkippedSide {
    SelectionUnavailable {
        side: SwitcherTwoViewSide,
        selection: SwitcherJitterBufferSelectionResult,
    },
    DecodeDeferred {
        side: SwitcherTwoViewSide,
        selected: SwitcherJitterBufferSelectedFrame,
        reason: SwitcherH264DecodeDeferredReason,
    },
    DecodeFailed {
        side: SwitcherTwoViewSide,
        selected: SwitcherJitterBufferSelectedFrame,
        failure: SwitcherH264DecodeFailure,
    },
    RenderDeferred {
        side: SwitcherTwoViewSide,
        selected: SwitcherJitterBufferSelectedFrame,
        reason: SwitcherWindowRenderDeferredReason,
    },
    WindowBackendUnavailable {
        side: SwitcherTwoViewSide,
        selected: SwitcherJitterBufferSelectedFrame,
        reason: SwitcherWindowBackendUnavailableReason,
        message: Option<String>,
    },
    InvalidFrame {
        side: SwitcherTwoViewSide,
        selected: SwitcherJitterBufferSelectedFrame,
        error: SwitcherDecodedFrameRenderInputError,
    },
    RenderFailed {
        side: SwitcherTwoViewSide,
        selected: SwitcherJitterBufferSelectedFrame,
        message: String,
    },
}

/// Result of connecting selected 2-view encoded frames to decode/render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewDecodeRenderResult {
    BothRendered {
        shared_target_time: TimestampMicros,
        left: SwitcherTwoViewRenderedSide,
        right: SwitcherTwoViewRenderedSide,
    },
    LeftRenderedRightSkipped {
        shared_target_time: TimestampMicros,
        left: SwitcherTwoViewRenderedSide,
        right: SwitcherTwoViewSkippedSide,
    },
    RightRenderedLeftSkipped {
        shared_target_time: TimestampMicros,
        left: SwitcherTwoViewSkippedSide,
        right: SwitcherTwoViewRenderedSide,
    },
    BothSkipped {
        shared_target_time: TimestampMicros,
        left: SwitcherTwoViewSkippedSide,
        right: SwitcherTwoViewSkippedSide,
    },
}

/// Previously displayed frame state owned by the future display policy caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewDisplayedFrame {
    pub side: SwitcherTwoViewSide,
    pub selected: Option<SwitcherJitterBufferSelectedFrame>,
    pub decoded: SwitcherDecodedFrame,
    pub displayed_at: TimestampMicros,
}

/// Input for deciding what each 2-view display slot should show after
/// decode/render has either rendered or skipped each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewDisplayPolicyInput {
    pub connection: SwitcherTwoViewSchedulerDecodeRenderConnectionOutput,
    pub previous_left: Option<SwitcherTwoViewDisplayedFrame>,
    pub previous_right: Option<SwitcherTwoViewDisplayedFrame>,
    pub current_time: TimestampMicros,
    pub max_hold_duration_micros: Option<u64>,
}

/// Explicit per-side display decision after decode/render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewDisplayDecision {
    Update {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        rendered: SwitcherTwoViewRenderedSide,
    },
    HoldPrevious {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        skipped: SwitcherTwoViewSkippedSide,
        hold_duration_micros: u64,
    },
    PreviousFrameStale {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        skipped: SwitcherTwoViewSkippedSide,
        hold_duration_micros: u64,
        max_hold_duration_micros: u64,
    },
    NoDisplayPlaceholder {
        side: SwitcherTwoViewSide,
        skipped: SwitcherTwoViewSkippedSide,
    },
}

/// Output of applying display policy to both 2-view slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewDisplayPolicyOutput {
    pub shared_target_time: TimestampMicros,
    pub left: SwitcherTwoViewDisplayDecision,
    pub right: SwitcherTwoViewDisplayDecision,
}

/// Input for deciding display behavior from fallible handoff-backed
/// decode/render output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffDisplayPolicyInput {
    pub connection: SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput,
    pub previous_left: Option<SwitcherTwoViewDisplayedFrame>,
    pub previous_right: Option<SwitcherTwoViewDisplayedFrame>,
    pub current_time: TimestampMicros,
    pub max_hold_duration_micros: Option<u64>,
}

/// Display decision for the fallible handoff-backed decode/render path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewHandoffDisplayDecision {
    Update {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        rendered: SwitcherTwoViewRenderedSide,
    },
    HoldPrevious {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
        hold_duration_micros: u64,
    },
    PreviousFrameStale {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
        hold_duration_micros: u64,
        max_hold_duration_micros: u64,
    },
    NoDisplayPlaceholder {
        side: SwitcherTwoViewSide,
        skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
    },
}

/// Output of applying fallible display policy to both 2-view slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffDisplayPolicyOutput {
    pub scheduler_status: SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus,
    pub shared_target_time: TimestampMicros,
    pub left: SwitcherTwoViewHandoffDisplayDecision,
    pub right: SwitcherTwoViewHandoffDisplayDecision,
}

/// Input for adapting fallible display policy decisions into 2-view
/// composition input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffDisplayCompositionAdapterInput {
    pub display: SwitcherTwoViewHandoffDisplayPolicyOutput,
    pub layout_policy: SwitcherTwoViewLayoutPolicy,
}

/// Input for adapting display policy decisions into 2-view composition input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewDisplayCompositionAdapterInput {
    pub display: SwitcherTwoViewDisplayPolicyOutput,
    pub layout_policy: SwitcherTwoViewLayoutPolicy,
}

/// Explicit per-side composition instruction after display policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewDisplayCompositionSideInstruction {
    UseUpdatedFrame {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        rendered: SwitcherTwoViewRenderedSide,
    },
    UseHeldPreviousFrame {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        skipped: SwitcherTwoViewSkippedSide,
        hold_duration_micros: u64,
    },
    UseStalePlaceholder {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        skipped: SwitcherTwoViewSkippedSide,
        hold_duration_micros: u64,
        max_hold_duration_micros: u64,
    },
    UseNoDisplayPlaceholder {
        side: SwitcherTwoViewSide,
        skipped: SwitcherTwoViewSkippedSide,
    },
}

/// Explicit per-side composition instruction after fallible display policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewHandoffDisplayCompositionSideInstruction {
    UseUpdatedFrame {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        rendered: SwitcherTwoViewRenderedSide,
    },
    UseHeldPreviousFrame {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
        hold_duration_micros: u64,
    },
    UseStalePlaceholder {
        side: SwitcherTwoViewSide,
        frame: SwitcherTwoViewDisplayedFrame,
        skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
        hold_duration_micros: u64,
        max_hold_duration_micros: u64,
    },
    UseNoDisplayPlaceholder {
        side: SwitcherTwoViewSide,
        skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
    },
    UseSourceErrorPlaceholder {
        side: SwitcherTwoViewSide,
        skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide,
    },
}

/// Output from adapting display decisions for the existing composition path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewDisplayCompositionAdapterOutput {
    pub shared_target_time: TimestampMicros,
    pub left: SwitcherTwoViewDisplayCompositionSideInstruction,
    pub right: SwitcherTwoViewDisplayCompositionSideInstruction,
    pub composition_input: SwitcherTwoViewCompositionInput,
}

/// Output from adapting fallible display decisions for the existing
/// composition path while preserving source-error detail in adapter
/// instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffDisplayCompositionAdapterOutput {
    pub scheduler_status: SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus,
    pub shared_target_time: TimestampMicros,
    pub left: SwitcherTwoViewHandoffDisplayCompositionSideInstruction,
    pub right: SwitcherTwoViewHandoffDisplayCompositionSideInstruction,
    pub composition_input: SwitcherTwoViewCompositionInput,
}

/// Input for connecting display-composition adapter output to the existing
/// composed-canvas render path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewDisplayCompositionRenderConnectionInput {
    pub adapter_output: SwitcherTwoViewDisplayCompositionAdapterOutput,
    pub window_title: String,
    pub render_hold_millis: u64,
}

/// Input for connecting fallible display-composition adapter output to the
/// existing composed-canvas render path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput {
    pub adapter_output: SwitcherTwoViewHandoffDisplayCompositionAdapterOutput,
    pub window_title: String,
    pub render_hold_millis: u64,
}

/// Result of attempting to render after display-composition adaptation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult {
    RenderedCanvas {
        render: SwitcherTwoViewComposedCanvasRenderResult,
    },
    NoRenderableCanvas {
        left_reason: SwitcherTwoViewManualDecodeRenderStatus,
        right_reason: SwitcherTwoViewManualDecodeRenderStatus,
    },
    CompositionInvalid {
        reason: SwitcherTwoViewCompositionInvalidReason,
    },
}

/// Result of attempting to render after fallible display-composition
/// adaptation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult {
    RenderedCanvas {
        render: SwitcherTwoViewComposedCanvasRenderResult,
    },
    NoRenderableCanvas {
        left_reason: SwitcherTwoViewManualDecodeRenderStatus,
        right_reason: SwitcherTwoViewManualDecodeRenderStatus,
    },
    CompositionInvalid {
        reason: SwitcherTwoViewCompositionInvalidReason,
    },
}

/// Output from the display-composition adapter -> composition -> render
/// connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewDisplayCompositionRenderConnectionOutput {
    pub adapter: SwitcherTwoViewDisplayCompositionAdapterOutput,
    pub composition: SwitcherTwoViewCompositionResult,
    pub render: SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult,
}

/// Output from the fallible display-composition adapter -> composition ->
/// render connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewHandoffDisplayCompositionRenderConnectionOutput {
    pub scheduler_status: SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus,
    pub adapter: SwitcherTwoViewHandoffDisplayCompositionAdapterOutput,
    pub composition: SwitcherTwoViewCompositionResult,
    pub render: SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult,
}

/// Input for the smallest server-mediated 2-view switcher validation.
///
/// The caller supplies server-owned queue state that already contains direct
/// `VideoFrame` packets or server-reassembled `VideoFrameFragment` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherServerMediatedTwoViewValidationInput {
    pub left: SwitcherTwoViewTargetTimeSourceViewConfig,
    pub right: SwitcherTwoViewTargetTimeSourceViewConfig,
    pub target_timestamp: TimestampMicros,
    pub scheduler_mode: SwitcherTwoViewTargetTimeSourceSchedulerMode,
    pub left_window_title: String,
    pub right_window_title: String,
    pub decode_render_hold_millis: u64,
    pub previous_left: Option<SwitcherTwoViewDisplayedFrame>,
    pub previous_right: Option<SwitcherTwoViewDisplayedFrame>,
    pub display_current_time: TimestampMicros,
    pub max_hold_duration_micros: Option<u64>,
    pub layout_policy: SwitcherTwoViewLayoutPolicy,
    pub composed_window_title: String,
    pub composed_render_hold_millis: u64,
}

/// Output from the server-mediated diagnostic path.
///
/// Each stage is kept visible so selected / waiting / no-frame / stale /
/// placeholder decisions can be inspected without collapsing them into a final
/// render-only status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherServerMediatedTwoViewValidationOutput {
    pub scheduler: SwitcherTwoViewTargetTimeSourceSchedulerResult,
    pub decode_render: SwitcherTwoViewSchedulerDecodeRenderConnectionOutput,
    pub display: SwitcherTwoViewDisplayPolicyOutput,
    pub adapter: SwitcherTwoViewDisplayCompositionAdapterOutput,
    pub render: SwitcherTwoViewDisplayCompositionRenderConnectionOutput,
}

/// Output from the fallible server-mediated diagnostic path.
///
/// Each fallible stage is kept visible so selected, waiting, no-frame,
/// handoff/source-error, stale, no-display placeholder, and source-error
/// placeholder states remain inspectable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherServerMediatedTwoViewHandoffValidationOutput {
    pub scheduler: SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult,
    pub decode_render_adapter: SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput,
    pub decode_render: SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput,
    pub display: SwitcherTwoViewHandoffDisplayPolicyOutput,
    pub adapter: SwitcherTwoViewHandoffDisplayCompositionAdapterOutput,
    pub render: SwitcherTwoViewHandoffDisplayCompositionRenderConnectionOutput,
}

/// Minimal in-process connection from server-owned reassembled queue state into
/// the current 2-view switcher display pipeline.
///
/// This boundary does not receive UDP, authenticate clients, reassemble
/// fragments, define cross-process server->switcher transport, alter H.264
/// decode/render behavior, mutate late frames, implement 4-view orchestration,
/// or output to OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherServerMediatedTwoViewValidationBoundary {
    scheduler: SwitcherTwoViewTargetTimeSourceSchedulerBoundary,
    decode_render: SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary,
    display_policy: SwitcherTwoViewDisplayPolicyBoundary,
    adapter: SwitcherTwoViewDisplayCompositionAdapterBoundary,
    render: SwitcherTwoViewDisplayCompositionRenderConnectionBoundary,
    handoff_scheduler: SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary,
    handoff_decode_render_adapter: SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary,
    handoff_decode_render: SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionBoundary,
    handoff_display_policy: SwitcherTwoViewHandoffDisplayPolicyBoundary,
    handoff_adapter: SwitcherTwoViewHandoffDisplayCompositionAdapterBoundary,
    handoff_render: SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary,
}

impl SwitcherServerMediatedTwoViewValidationBoundary {
    pub fn run_with_runtimes(
        &self,
        queue_state: &mut ServerVideoFrameQueueState,
        input: SwitcherServerMediatedTwoViewValidationInput,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherServerMediatedTwoViewValidationOutput {
        let mut source = SwitcherInProcessServerQueueFrameSource::new(queue_state);
        self.run_from_source_with_runtimes(&mut source, input, decode_runtime, render_runtime)
    }

    pub fn run_from_source_with_runtimes(
        &self,
        source: &mut impl SwitcherQueuedFrameSource,
        input: SwitcherServerMediatedTwoViewValidationInput,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherServerMediatedTwoViewValidationOutput {
        let scheduler = self.scheduler.select_pair_from_source(
            source,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: input.left,
                right: input.right,
                target_timestamp: input.target_timestamp,
                mode: input.scheduler_mode,
            },
        );
        let decode_render = self.decode_render.render_scheduler_result_with_runtimes(
            SwitcherTwoViewSchedulerDecodeRenderConnectionInput {
                scheduler_result: scheduler.clone(),
                left_window_title: input.left_window_title,
                right_window_title: input.right_window_title,
                render_hold_millis: input.decode_render_hold_millis,
            },
            decode_runtime,
            render_runtime,
        );
        let display = self
            .display_policy
            .decide(SwitcherTwoViewDisplayPolicyInput {
                connection: decode_render.clone(),
                previous_left: input.previous_left,
                previous_right: input.previous_right,
                current_time: input.display_current_time,
                max_hold_duration_micros: input.max_hold_duration_micros,
            });
        let adapter = self
            .adapter
            .adapt(SwitcherTwoViewDisplayCompositionAdapterInput {
                display: display.clone(),
                layout_policy: input.layout_policy,
            });
        let render = self.render.render_adapter_output_with_runtime(
            SwitcherTwoViewDisplayCompositionRenderConnectionInput {
                adapter_output: adapter.clone(),
                window_title: input.composed_window_title,
                render_hold_millis: input.composed_render_hold_millis,
            },
            render_runtime,
        );

        SwitcherServerMediatedTwoViewValidationOutput {
            scheduler,
            decode_render,
            display,
            adapter,
            render,
        }
    }

    pub fn run_fallible_with_runtimes(
        &self,
        queue_state: &mut ServerVideoFrameQueueState,
        input: SwitcherServerMediatedTwoViewValidationInput,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherServerMediatedTwoViewHandoffValidationOutput {
        let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(queue_state);
        self.run_fallible_from_handoff_with_runtimes(
            &mut handoff,
            input,
            decode_runtime,
            render_runtime,
        )
    }

    pub fn run_fallible_from_handoff_with_runtimes(
        &self,
        handoff: &mut impl SwitcherQueuedFrameHandoff,
        input: SwitcherServerMediatedTwoViewValidationInput,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherServerMediatedTwoViewHandoffValidationOutput {
        let scheduler = self.handoff_scheduler.select_pair_from_handoff(
            handoff,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: input.left,
                right: input.right,
                target_timestamp: input.target_timestamp,
                mode: input.scheduler_mode,
            },
        );
        let decode_render_adapter = self.handoff_decode_render_adapter.adapt(
            SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
                scheduler_result: scheduler.clone(),
                left_window_title: input.left_window_title.clone(),
                right_window_title: input.right_window_title.clone(),
                render_hold_millis: input.decode_render_hold_millis,
            },
        );
        let decode_render = self
            .handoff_decode_render
            .render_adapter_output_with_runtimes(
                SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionInput {
                    adapter_output: decode_render_adapter.clone(),
                    left_window_title: input.left_window_title,
                    right_window_title: input.right_window_title,
                    render_hold_millis: input.decode_render_hold_millis,
                },
                decode_runtime,
                render_runtime,
            );
        let display =
            self.handoff_display_policy
                .decide(SwitcherTwoViewHandoffDisplayPolicyInput {
                    connection: decode_render.clone(),
                    previous_left: input.previous_left,
                    previous_right: input.previous_right,
                    current_time: input.display_current_time,
                    max_hold_duration_micros: input.max_hold_duration_micros,
                });
        let adapter =
            self.handoff_adapter
                .adapt(SwitcherTwoViewHandoffDisplayCompositionAdapterInput {
                    display: display.clone(),
                    layout_policy: input.layout_policy,
                });
        let render = self.handoff_render.render_adapter_output_with_runtime(
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput {
                adapter_output: adapter.clone(),
                window_title: input.composed_window_title,
                render_hold_millis: input.composed_render_hold_millis,
            },
            render_runtime,
        );

        SwitcherServerMediatedTwoViewHandoffValidationOutput {
            scheduler,
            decode_render_adapter,
            decode_render,
            display,
            adapter,
            render,
        }
    }
}

enum SwitcherTwoViewDisplayPolicySideInput {
    Rendered(SwitcherTwoViewRenderedSide),
    Skipped(SwitcherTwoViewSkippedSide),
}

/// Minimal display policy boundary for 2-view decode/render results.
///
/// This boundary decides update / hold previous / stale / placeholder only. It
/// does not render, compose, output to OBS, mutate queues, or create fallback
/// frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewDisplayPolicyBoundary;

impl SwitcherTwoViewDisplayPolicyBoundary {
    pub fn decide(
        &self,
        input: SwitcherTwoViewDisplayPolicyInput,
    ) -> SwitcherTwoViewDisplayPolicyOutput {
        let (shared_target_time, left, right) =
            split_decode_render_result_for_display_policy(input.connection.render);
        SwitcherTwoViewDisplayPolicyOutput {
            shared_target_time,
            left: display_policy_decision_for_side(
                SwitcherTwoViewSide::Left,
                left,
                input.previous_left,
                input.current_time,
                input.max_hold_duration_micros,
            ),
            right: display_policy_decision_for_side(
                SwitcherTwoViewSide::Right,
                right,
                input.previous_right,
                input.current_time,
                input.max_hold_duration_micros,
            ),
        }
    }
}

/// Minimal display policy for fallible handoff-backed decode/render output.
///
/// It mirrors the existing update / hold / stale / no-display behavior while
/// preserving source-error skipped sides as source-error detail.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewHandoffDisplayPolicyBoundary;

impl SwitcherTwoViewHandoffDisplayPolicyBoundary {
    pub fn decide(
        &self,
        input: SwitcherTwoViewHandoffDisplayPolicyInput,
    ) -> SwitcherTwoViewHandoffDisplayPolicyOutput {
        let (shared_target_time, left, right) =
            split_handoff_decode_render_result_for_display_policy(input.connection.render);
        SwitcherTwoViewHandoffDisplayPolicyOutput {
            scheduler_status: input.connection.scheduler_status,
            shared_target_time,
            left: handoff_display_policy_decision_for_side(
                SwitcherTwoViewSide::Left,
                left,
                input.previous_left,
                input.current_time,
                input.max_hold_duration_micros,
            ),
            right: handoff_display_policy_decision_for_side(
                SwitcherTwoViewSide::Right,
                right,
                input.previous_right,
                input.current_time,
                input.max_hold_duration_micros,
            ),
        }
    }
}

fn split_decode_render_result_for_display_policy(
    result: SwitcherTwoViewDecodeRenderResult,
) -> (
    TimestampMicros,
    SwitcherTwoViewDisplayPolicySideInput,
    SwitcherTwoViewDisplayPolicySideInput,
) {
    match result {
        SwitcherTwoViewDecodeRenderResult::BothRendered {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherTwoViewDisplayPolicySideInput::Rendered(left),
            SwitcherTwoViewDisplayPolicySideInput::Rendered(right),
        ),
        SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherTwoViewDisplayPolicySideInput::Rendered(left),
            SwitcherTwoViewDisplayPolicySideInput::Skipped(right),
        ),
        SwitcherTwoViewDecodeRenderResult::RightRenderedLeftSkipped {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherTwoViewDisplayPolicySideInput::Skipped(left),
            SwitcherTwoViewDisplayPolicySideInput::Rendered(right),
        ),
        SwitcherTwoViewDecodeRenderResult::BothSkipped {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherTwoViewDisplayPolicySideInput::Skipped(left),
            SwitcherTwoViewDisplayPolicySideInput::Skipped(right),
        ),
    }
}

enum SwitcherTwoViewHandoffDisplayPolicySideInput {
    Rendered(SwitcherTwoViewRenderedSide),
    Skipped(SwitcherTwoViewHandoffDecodeRenderSkippedSide),
}

fn split_handoff_decode_render_result_for_display_policy(
    result: SwitcherTwoViewHandoffDecodeRenderConnectionResult,
) -> (
    TimestampMicros,
    SwitcherTwoViewHandoffDisplayPolicySideInput,
    SwitcherTwoViewHandoffDisplayPolicySideInput,
) {
    match result {
        SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothRendered {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherTwoViewHandoffDisplayPolicySideInput::Rendered(left),
            SwitcherTwoViewHandoffDisplayPolicySideInput::Rendered(right),
        ),
        SwitcherTwoViewHandoffDecodeRenderConnectionResult::LeftRenderedRightSkipped {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherTwoViewHandoffDisplayPolicySideInput::Rendered(left),
            SwitcherTwoViewHandoffDisplayPolicySideInput::Skipped(right),
        ),
        SwitcherTwoViewHandoffDecodeRenderConnectionResult::RightRenderedLeftSkipped {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherTwoViewHandoffDisplayPolicySideInput::Skipped(left),
            SwitcherTwoViewHandoffDisplayPolicySideInput::Rendered(right),
        ),
        SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothSkipped {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherTwoViewHandoffDisplayPolicySideInput::Skipped(left),
            SwitcherTwoViewHandoffDisplayPolicySideInput::Skipped(right),
        ),
    }
}

fn display_policy_decision_for_side(
    side: SwitcherTwoViewSide,
    input: SwitcherTwoViewDisplayPolicySideInput,
    previous: Option<SwitcherTwoViewDisplayedFrame>,
    current_time: TimestampMicros,
    max_hold_duration_micros: Option<u64>,
) -> SwitcherTwoViewDisplayDecision {
    match input {
        SwitcherTwoViewDisplayPolicySideInput::Rendered(rendered) => {
            let frame = SwitcherTwoViewDisplayedFrame {
                side,
                selected: Some(rendered.selected.clone()),
                decoded: rendered.decoded.clone(),
                displayed_at: current_time,
            };
            SwitcherTwoViewDisplayDecision::Update {
                side,
                frame,
                rendered,
            }
        }
        SwitcherTwoViewDisplayPolicySideInput::Skipped(skipped) => {
            let Some(frame) = previous else {
                return SwitcherTwoViewDisplayDecision::NoDisplayPlaceholder { side, skipped };
            };
            let hold_duration_micros = current_time.0.saturating_sub(frame.displayed_at.0);
            if let Some(max_hold_duration_micros) = max_hold_duration_micros {
                if hold_duration_micros > max_hold_duration_micros {
                    return SwitcherTwoViewDisplayDecision::PreviousFrameStale {
                        side,
                        frame,
                        skipped,
                        hold_duration_micros,
                        max_hold_duration_micros,
                    };
                }
            }
            SwitcherTwoViewDisplayDecision::HoldPrevious {
                side,
                frame,
                skipped,
                hold_duration_micros,
            }
        }
    }
}

fn handoff_display_policy_decision_for_side(
    side: SwitcherTwoViewSide,
    input: SwitcherTwoViewHandoffDisplayPolicySideInput,
    previous: Option<SwitcherTwoViewDisplayedFrame>,
    current_time: TimestampMicros,
    max_hold_duration_micros: Option<u64>,
) -> SwitcherTwoViewHandoffDisplayDecision {
    match input {
        SwitcherTwoViewHandoffDisplayPolicySideInput::Rendered(rendered) => {
            let frame = SwitcherTwoViewDisplayedFrame {
                side,
                selected: Some(rendered.selected.clone()),
                decoded: rendered.decoded.clone(),
                displayed_at: current_time,
            };
            SwitcherTwoViewHandoffDisplayDecision::Update {
                side,
                frame,
                rendered,
            }
        }
        SwitcherTwoViewHandoffDisplayPolicySideInput::Skipped(skipped) => {
            let Some(frame) = previous else {
                return SwitcherTwoViewHandoffDisplayDecision::NoDisplayPlaceholder {
                    side,
                    skipped,
                };
            };
            let hold_duration_micros = current_time.0.saturating_sub(frame.displayed_at.0);
            if let Some(max_hold_duration_micros) = max_hold_duration_micros {
                if hold_duration_micros > max_hold_duration_micros {
                    return SwitcherTwoViewHandoffDisplayDecision::PreviousFrameStale {
                        side,
                        frame,
                        skipped,
                        hold_duration_micros,
                        max_hold_duration_micros,
                    };
                }
            }
            SwitcherTwoViewHandoffDisplayDecision::HoldPrevious {
                side,
                frame,
                skipped,
                hold_duration_micros,
            }
        }
    }
}

/// Minimal adapter from display policy decisions to the existing 2-view
/// composition input.
///
/// Update and hold decisions carry real decoded frames. Stale and no-display
/// decisions stay explicit and enter composition as skipped sides, so this
/// boundary does not create fallback frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewDisplayCompositionAdapterBoundary;

impl SwitcherTwoViewDisplayCompositionAdapterBoundary {
    pub fn adapt(
        &self,
        input: SwitcherTwoViewDisplayCompositionAdapterInput,
    ) -> SwitcherTwoViewDisplayCompositionAdapterOutput {
        let shared_target_time = input.display.shared_target_time;
        let left = display_composition_instruction_for_decision(input.display.left);
        let right = display_composition_instruction_for_decision(input.display.right);
        let composition_input = SwitcherTwoViewCompositionInput {
            left: display_composition_side_input(&left),
            right: display_composition_side_input(&right),
            policy: input.layout_policy,
        };

        SwitcherTwoViewDisplayCompositionAdapterOutput {
            shared_target_time,
            left,
            right,
            composition_input,
        }
    }
}

/// Minimal adapter from fallible display policy decisions to the existing
/// 2-view composition input.
///
/// Update and hold decisions carry real decoded frames. Stale, no-display, and
/// source-error placeholders stay explicit in the adapter output and enter
/// composition as skipped sides, so this boundary does not create fallback
/// frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewHandoffDisplayCompositionAdapterBoundary;

impl SwitcherTwoViewHandoffDisplayCompositionAdapterBoundary {
    pub fn adapt(
        &self,
        input: SwitcherTwoViewHandoffDisplayCompositionAdapterInput,
    ) -> SwitcherTwoViewHandoffDisplayCompositionAdapterOutput {
        let scheduler_status = input.display.scheduler_status;
        let shared_target_time = input.display.shared_target_time;
        let left = handoff_display_composition_instruction_for_decision(input.display.left);
        let right = handoff_display_composition_instruction_for_decision(input.display.right);
        let composition_input = SwitcherTwoViewCompositionInput {
            left: handoff_display_composition_side_input(&left),
            right: handoff_display_composition_side_input(&right),
            policy: input.layout_policy,
        };

        SwitcherTwoViewHandoffDisplayCompositionAdapterOutput {
            scheduler_status,
            shared_target_time,
            left,
            right,
            composition_input,
        }
    }
}

fn display_composition_instruction_for_decision(
    decision: SwitcherTwoViewDisplayDecision,
) -> SwitcherTwoViewDisplayCompositionSideInstruction {
    match decision {
        SwitcherTwoViewDisplayDecision::Update {
            side,
            frame,
            rendered,
        } => SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame {
            side,
            frame,
            rendered,
        },
        SwitcherTwoViewDisplayDecision::HoldPrevious {
            side,
            frame,
            skipped,
            hold_duration_micros,
        } => SwitcherTwoViewDisplayCompositionSideInstruction::UseHeldPreviousFrame {
            side,
            frame,
            skipped,
            hold_duration_micros,
        },
        SwitcherTwoViewDisplayDecision::PreviousFrameStale {
            side,
            frame,
            skipped,
            hold_duration_micros,
            max_hold_duration_micros,
        } => SwitcherTwoViewDisplayCompositionSideInstruction::UseStalePlaceholder {
            side,
            frame,
            skipped,
            hold_duration_micros,
            max_hold_duration_micros,
        },
        SwitcherTwoViewDisplayDecision::NoDisplayPlaceholder { side, skipped } => {
            SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
                side,
                skipped,
            }
        }
    }
}

fn handoff_display_composition_instruction_for_decision(
    decision: SwitcherTwoViewHandoffDisplayDecision,
) -> SwitcherTwoViewHandoffDisplayCompositionSideInstruction {
    match decision {
        SwitcherTwoViewHandoffDisplayDecision::Update {
            side,
            frame,
            rendered,
        } => SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame {
            side,
            frame,
            rendered,
        },
        SwitcherTwoViewHandoffDisplayDecision::HoldPrevious {
            side,
            frame,
            skipped,
            hold_duration_micros,
        } => SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseHeldPreviousFrame {
            side,
            frame,
            skipped,
            hold_duration_micros,
        },
        SwitcherTwoViewHandoffDisplayDecision::PreviousFrameStale {
            side,
            frame,
            skipped,
            hold_duration_micros,
            max_hold_duration_micros,
        } => SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseStalePlaceholder {
            side,
            frame,
            skipped,
            hold_duration_micros,
            max_hold_duration_micros,
        },
        SwitcherTwoViewHandoffDisplayDecision::NoDisplayPlaceholder { side, skipped } => {
            if matches!(
                skipped,
                SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError { .. }
            ) {
                SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder {
                    side,
                    skipped,
                }
            } else {
                SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
                    side,
                    skipped,
                }
            }
        }
    }
}

fn display_composition_side_input(
    instruction: &SwitcherTwoViewDisplayCompositionSideInstruction,
) -> SwitcherTwoViewLayoutSideInput {
    match instruction {
        SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame {
            side, frame, ..
        }
        | SwitcherTwoViewDisplayCompositionSideInstruction::UseHeldPreviousFrame {
            side,
            frame,
            ..
        } => SwitcherTwoViewLayoutSideInput::Decoded {
            side: *side,
            selected: frame.selected.clone(),
            frame: frame.decoded.clone(),
        },
        SwitcherTwoViewDisplayCompositionSideInstruction::UseStalePlaceholder {
            side,
            skipped,
            ..
        }
        | SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            side,
            skipped,
        } => SwitcherTwoViewLayoutSideInput::Skipped {
            side: *side,
            reason: two_view_skipped_status(skipped),
        },
    }
}

fn handoff_display_composition_side_input(
    instruction: &SwitcherTwoViewHandoffDisplayCompositionSideInstruction,
) -> SwitcherTwoViewLayoutSideInput {
    match instruction {
        SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame {
            side,
            frame,
            ..
        }
        | SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseHeldPreviousFrame {
            side,
            frame,
            ..
        } => SwitcherTwoViewLayoutSideInput::Decoded {
            side: *side,
            selected: frame.selected.clone(),
            frame: frame.decoded.clone(),
        },
        SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseStalePlaceholder {
            side,
            skipped,
            ..
        }
        | SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            side,
            skipped,
        }
        | SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder {
            side,
            skipped,
        } => SwitcherTwoViewLayoutSideInput::Skipped {
            side: *side,
            reason: handoff_display_composition_skipped_status(skipped),
        },
    }
}

/// Minimal in-process connection from display-composition adapter output to the
/// existing 2-view composition and composed-canvas render boundaries.
///
/// It renders only composed frames produced from real decoded update/held
/// inputs. Stale and no-display placeholder instructions remain skipped sides
/// in the composition result and are not converted into fake decoded frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewDisplayCompositionRenderConnectionBoundary {
    composer: SwitcherTwoViewCompositionBoundary,
    renderer: SwitcherTwoViewComposedCanvasRenderBoundary,
}

impl SwitcherTwoViewDisplayCompositionRenderConnectionBoundary {
    pub fn render_adapter_output_with_runtime(
        &self,
        input: SwitcherTwoViewDisplayCompositionRenderConnectionInput,
        runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewDisplayCompositionRenderConnectionOutput {
        let composition = self
            .composer
            .compose_side_by_side(input.adapter_output.composition_input.clone());
        let render = match &composition {
            SwitcherTwoViewCompositionResult::BothComposed { frame }
            | SwitcherTwoViewCompositionResult::LeftOnly { frame, .. }
            | SwitcherTwoViewCompositionResult::RightOnly { frame, .. } => {
                SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                    render: self.renderer.render_composed_frame_with_runtime(
                        frame,
                        input.window_title,
                        input.render_hold_millis,
                        runtime,
                    ),
                }
            }
            SwitcherTwoViewCompositionResult::EmptyPlaceholder {
                left_reason,
                right_reason,
            } => {
                SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::NoRenderableCanvas {
                    left_reason: left_reason.clone(),
                    right_reason: right_reason.clone(),
                }
            }
            SwitcherTwoViewCompositionResult::InvalidDimensions { reason } => {
                SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::CompositionInvalid {
                    reason: reason.clone(),
                }
            }
        };

        SwitcherTwoViewDisplayCompositionRenderConnectionOutput {
            adapter: input.adapter_output,
            composition,
            render,
        }
    }
}

/// Minimal in-process connection from fallible display-composition adapter
/// output to the existing 2-view composition and composed-canvas render
/// boundaries.
///
/// The adapter output remains the place where source-error placeholder detail
/// is visible. This connection renders only when at least one side carries a
/// real decoded update/held frame, and it never creates decoded frames for
/// stale, no-display, or source-error placeholder sides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary {
    composer: SwitcherTwoViewCompositionBoundary,
    renderer: SwitcherTwoViewComposedCanvasRenderBoundary,
}

impl SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary {
    pub fn render_adapter_output_with_runtime(
        &self,
        input: SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput,
        runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewHandoffDisplayCompositionRenderConnectionOutput {
        let scheduler_status = input.adapter_output.scheduler_status;
        let composition =
            match handoff_display_composition_placeholder_reasons(&input.adapter_output) {
                Some((left_reason, right_reason)) => {
                    SwitcherTwoViewCompositionResult::EmptyPlaceholder {
                        left_reason,
                        right_reason,
                    }
                }
                None => self
                    .composer
                    .compose_side_by_side(input.adapter_output.composition_input.clone()),
            };
        let render = match &composition {
            SwitcherTwoViewCompositionResult::BothComposed { frame }
            | SwitcherTwoViewCompositionResult::LeftOnly { frame, .. }
            | SwitcherTwoViewCompositionResult::RightOnly { frame, .. } => {
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                    render: self.renderer.render_composed_frame_with_runtime(
                        frame,
                        input.window_title,
                        input.render_hold_millis,
                        runtime,
                    ),
                }
            }
            SwitcherTwoViewCompositionResult::EmptyPlaceholder {
                left_reason,
                right_reason,
            } => {
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::NoRenderableCanvas {
                    left_reason: left_reason.clone(),
                    right_reason: right_reason.clone(),
                }
            }
            SwitcherTwoViewCompositionResult::InvalidDimensions { reason } => {
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::CompositionInvalid {
                    reason: reason.clone(),
                }
            }
        };

        SwitcherTwoViewHandoffDisplayCompositionRenderConnectionOutput {
            scheduler_status,
            adapter: input.adapter_output,
            composition,
            render,
        }
    }
}

fn handoff_display_composition_placeholder_reasons(
    output: &SwitcherTwoViewHandoffDisplayCompositionAdapterOutput,
) -> Option<(
    SwitcherTwoViewManualDecodeRenderStatus,
    SwitcherTwoViewManualDecodeRenderStatus,
)> {
    let left = handoff_placeholder_reason_for_instruction(&output.left)?;
    let right = handoff_placeholder_reason_for_instruction(&output.right)?;
    Some((left, right))
}

fn handoff_placeholder_reason_for_instruction(
    instruction: &SwitcherTwoViewHandoffDisplayCompositionSideInstruction,
) -> Option<SwitcherTwoViewManualDecodeRenderStatus> {
    match instruction {
        SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        | SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseHeldPreviousFrame {
            ..
        } => None,
        SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseStalePlaceholder {
            skipped,
            ..
        }
        | SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            skipped,
            ..
        }
        | SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder {
            skipped,
            ..
        } => Some(handoff_display_composition_skipped_status(skipped)),
    }
}

enum SwitcherTwoViewSideDecodeRenderOutcome {
    Rendered(SwitcherTwoViewRenderedSide),
    Skipped(SwitcherTwoViewSkippedSide),
}

/// Thin 2-view composition from targetTime selection to decode/render.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewDecodeRenderBoundary {
    decoder: SwitcherH264DecodeBoundary,
    renderer: SwitcherWindowRenderBoundary,
}

impl SwitcherTwoViewDecodeRenderBoundary {
    pub fn render_selected_pair_with_runtimes(
        &self,
        input: SwitcherTwoViewDecodeRenderInput,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewDecodeRenderResult {
        let (shared_target_time, left_selection, right_selection) =
            split_two_view_selection(input.selection);
        let left = self.render_side_with_runtimes(
            SwitcherTwoViewSide::Left,
            left_selection,
            input.left_window_title,
            input.render_hold_millis,
            decode_runtime,
            render_runtime,
        );
        let right = self.render_side_with_runtimes(
            SwitcherTwoViewSide::Right,
            right_selection,
            input.right_window_title,
            input.render_hold_millis,
            decode_runtime,
            render_runtime,
        );

        match (left, right) {
            (
                SwitcherTwoViewSideDecodeRenderOutcome::Rendered(left),
                SwitcherTwoViewSideDecodeRenderOutcome::Rendered(right),
            ) => SwitcherTwoViewDecodeRenderResult::BothRendered {
                shared_target_time,
                left,
                right,
            },
            (
                SwitcherTwoViewSideDecodeRenderOutcome::Rendered(left),
                SwitcherTwoViewSideDecodeRenderOutcome::Skipped(right),
            ) => SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped {
                shared_target_time,
                left,
                right,
            },
            (
                SwitcherTwoViewSideDecodeRenderOutcome::Skipped(left),
                SwitcherTwoViewSideDecodeRenderOutcome::Rendered(right),
            ) => SwitcherTwoViewDecodeRenderResult::RightRenderedLeftSkipped {
                shared_target_time,
                left,
                right,
            },
            (
                SwitcherTwoViewSideDecodeRenderOutcome::Skipped(left),
                SwitcherTwoViewSideDecodeRenderOutcome::Skipped(right),
            ) => SwitcherTwoViewDecodeRenderResult::BothSkipped {
                shared_target_time,
                left,
                right,
            },
        }
    }

    fn render_side_with_runtimes(
        &self,
        side: SwitcherTwoViewSide,
        selection: SwitcherJitterBufferSelectionResult,
        title: String,
        hold_millis: u64,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewSideDecodeRenderOutcome {
        let selected = match selection {
            SwitcherJitterBufferSelectionResult::Selected(selected) => selected,
            selection => {
                return SwitcherTwoViewSideDecodeRenderOutcome::Skipped(
                    SwitcherTwoViewSkippedSide::SelectionUnavailable { side, selection },
                );
            }
        };

        let decoded = match self.decoder.decode_with_runtime(
            SwitcherH264DecodeInput {
                encoded_payload: selected.frame.encoded_payload.clone(),
                width: selected.frame.width,
                height: selected.frame.height,
            },
            decode_runtime,
        ) {
            SwitcherH264DecodeResult::Decoded(decoded) => decoded,
            SwitcherH264DecodeResult::Deferred { reason } => {
                return SwitcherTwoViewSideDecodeRenderOutcome::Skipped(
                    SwitcherTwoViewSkippedSide::DecodeDeferred {
                        side,
                        selected,
                        reason,
                    },
                );
            }
            SwitcherH264DecodeResult::Failed(failure) => {
                return SwitcherTwoViewSideDecodeRenderOutcome::Skipped(
                    SwitcherTwoViewSkippedSide::DecodeFailed {
                        side,
                        selected,
                        failure,
                    },
                );
            }
        };

        match self.renderer.render_decoded_frame_with_runtime(
            &decoded,
            title,
            hold_millis,
            render_runtime,
        ) {
            SwitcherWindowRenderResult::Rendered(render) => {
                SwitcherTwoViewSideDecodeRenderOutcome::Rendered(SwitcherTwoViewRenderedSide {
                    side,
                    selected,
                    decoded,
                    render,
                })
            }
            SwitcherWindowRenderResult::RenderDeferred { reason } => {
                SwitcherTwoViewSideDecodeRenderOutcome::Skipped(
                    SwitcherTwoViewSkippedSide::RenderDeferred {
                        side,
                        selected,
                        reason,
                    },
                )
            }
            SwitcherWindowRenderResult::BackendUnavailable { reason, message } => {
                SwitcherTwoViewSideDecodeRenderOutcome::Skipped(
                    SwitcherTwoViewSkippedSide::WindowBackendUnavailable {
                        side,
                        selected,
                        reason,
                        message,
                    },
                )
            }
            SwitcherWindowRenderResult::InvalidFrame { error } => {
                SwitcherTwoViewSideDecodeRenderOutcome::Skipped(
                    SwitcherTwoViewSkippedSide::InvalidFrame {
                        side,
                        selected,
                        error,
                    },
                )
            }
            SwitcherWindowRenderResult::RenderFailed { message } => {
                SwitcherTwoViewSideDecodeRenderOutcome::Skipped(
                    SwitcherTwoViewSkippedSide::RenderFailed {
                        side,
                        selected,
                        message,
                    },
                )
            }
        }
    }
}

fn split_two_view_selection(
    selection: SwitcherTwoViewTargetTimeSelectionResult,
) -> (
    TimestampMicros,
    SwitcherJitterBufferSelectionResult,
    SwitcherJitterBufferSelectionResult,
) {
    match selection {
        SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
            shared_target_time,
            left,
            right,
        } => (
            shared_target_time,
            SwitcherJitterBufferSelectionResult::Selected(left),
            SwitcherJitterBufferSelectionResult::Selected(right),
        ),
        SwitcherTwoViewTargetTimeSelectionResult::Partial {
            shared_target_time,
            left,
            right,
        }
        | SwitcherTwoViewTargetTimeSelectionResult::BothUnavailable {
            shared_target_time,
            left,
            right,
        } => (shared_target_time, left, right),
    }
}

/// Side state consumed by the first 2-view layout/composition boundary.
///
/// This accepts decoded frames only. Selection, H.264 decode, and window
/// rendering remain upstream responsibilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewLayoutSideInput {
    Decoded {
        side: SwitcherTwoViewSide,
        selected: Option<SwitcherJitterBufferSelectedFrame>,
        frame: SwitcherDecodedFrame,
    },
    Skipped {
        side: SwitcherTwoViewSide,
        reason: SwitcherTwoViewManualDecodeRenderStatus,
    },
}

impl SwitcherTwoViewLayoutSideInput {
    pub fn from_rendered_side(rendered: SwitcherTwoViewRenderedSide) -> Self {
        Self::Decoded {
            side: rendered.side,
            selected: Some(rendered.selected),
            frame: rendered.decoded,
        }
    }

    pub fn skipped(
        side: SwitcherTwoViewSide,
        reason: SwitcherTwoViewManualDecodeRenderStatus,
    ) -> Self {
        Self::Skipped { side, reason }
    }
}

/// MVP 2-view layout policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewLayoutPolicy {
    pub placeholder_bgra: [u8; 4],
}

impl Default for SwitcherTwoViewLayoutPolicy {
    fn default() -> Self {
        Self {
            placeholder_bgra: [16, 16, 16, 255],
        }
    }
}

/// Input for composing two decoded/renderable sides into one BGRA canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewCompositionInput {
    pub left: SwitcherTwoViewLayoutSideInput,
    pub right: SwitcherTwoViewLayoutSideInput,
    pub policy: SwitcherTwoViewLayoutPolicy,
}

impl SwitcherTwoViewCompositionInput {
    pub fn from_decode_render_result(
        result: SwitcherTwoViewDecodeRenderResult,
        policy: SwitcherTwoViewLayoutPolicy,
    ) -> Self {
        match result {
            SwitcherTwoViewDecodeRenderResult::BothRendered { left, right, .. } => Self {
                left: SwitcherTwoViewLayoutSideInput::from_rendered_side(left),
                right: SwitcherTwoViewLayoutSideInput::from_rendered_side(right),
                policy,
            },
            SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { left, right, .. } => {
                Self {
                    left: SwitcherTwoViewLayoutSideInput::from_rendered_side(left),
                    right: SwitcherTwoViewLayoutSideInput::skipped(
                        SwitcherTwoViewSide::Right,
                        two_view_skipped_status(&right),
                    ),
                    policy,
                }
            }
            SwitcherTwoViewDecodeRenderResult::RightRenderedLeftSkipped { left, right, .. } => {
                Self {
                    left: SwitcherTwoViewLayoutSideInput::skipped(
                        SwitcherTwoViewSide::Left,
                        two_view_skipped_status(&left),
                    ),
                    right: SwitcherTwoViewLayoutSideInput::from_rendered_side(right),
                    policy,
                }
            }
            SwitcherTwoViewDecodeRenderResult::BothSkipped { left, right, .. } => Self {
                left: SwitcherTwoViewLayoutSideInput::skipped(
                    SwitcherTwoViewSide::Left,
                    two_view_skipped_status(&left),
                ),
                right: SwitcherTwoViewLayoutSideInput::skipped(
                    SwitcherTwoViewSide::Right,
                    two_view_skipped_status(&right),
                ),
                policy,
            },
        }
    }
}

/// Metadata preserved for a composed side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewComposedSideMetadata {
    pub side: SwitcherTwoViewSide,
    pub selected: Option<SwitcherJitterBufferSelectedFrame>,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// One composed 2-view BGRA canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewComposedFrame {
    pub width: u32,
    pub height: u32,
    pub pixel_format: SwitcherDecodedFramePixelFormat,
    pub pixels: Vec<u8>,
    pub left: Option<SwitcherTwoViewComposedSideMetadata>,
    pub right: Option<SwitcherTwoViewComposedSideMetadata>,
}

/// Invalid input details for 2-view composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewCompositionInvalidReason {
    WrongSide {
        expected: SwitcherTwoViewSide,
        actual: SwitcherTwoViewSide,
    },
    UnsupportedPixelFormat {
        side: SwitcherTwoViewSide,
        actual: SwitcherDecodedFramePixelFormat,
    },
    InvalidDimensions {
        side: SwitcherTwoViewSide,
    },
    InvalidBufferLength {
        side: SwitcherTwoViewSide,
        expected: usize,
        actual: usize,
    },
    CanvasTooLarge,
}

/// Result of composing two decoded sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewCompositionResult {
    BothComposed {
        frame: SwitcherTwoViewComposedFrame,
    },
    LeftOnly {
        frame: SwitcherTwoViewComposedFrame,
        right_placeholder_reason: SwitcherTwoViewManualDecodeRenderStatus,
    },
    RightOnly {
        frame: SwitcherTwoViewComposedFrame,
        left_placeholder_reason: SwitcherTwoViewManualDecodeRenderStatus,
    },
    EmptyPlaceholder {
        left_reason: SwitcherTwoViewManualDecodeRenderStatus,
        right_reason: SwitcherTwoViewManualDecodeRenderStatus,
    },
    InvalidDimensions {
        reason: SwitcherTwoViewCompositionInvalidReason,
    },
}

/// Validated render input derived from one composed 2-view BGRA canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewComposedFrameRenderInput {
    pub frame: SwitcherDecodedFrameRenderInput,
    pub left: Option<SwitcherTwoViewComposedSideMetadata>,
    pub right: Option<SwitcherTwoViewComposedSideMetadata>,
}

/// Invalid composed-frame reason for window rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewComposedFrameRenderInputError {
    InvalidFrame(SwitcherDecodedFrameRenderInputError),
    MissingComposedSideMetadata,
}

impl SwitcherTwoViewComposedFrameRenderInput {
    pub fn from_composed_frame(
        frame: &SwitcherTwoViewComposedFrame,
    ) -> Result<Self, SwitcherTwoViewComposedFrameRenderInputError> {
        if frame.left.is_none() && frame.right.is_none() {
            return Err(SwitcherTwoViewComposedFrameRenderInputError::MissingComposedSideMetadata);
        }
        let decoded = SwitcherDecodedFrame {
            width: frame.width,
            height: frame.height,
            pixel_format: frame.pixel_format,
            pixels: frame.pixels.clone(),
        };
        let render_input = SwitcherDecodedFrameRenderInput::from_decoded_frame(&decoded)
            .map_err(SwitcherTwoViewComposedFrameRenderInputError::InvalidFrame)?;

        Ok(Self {
            frame: render_input,
            left: frame.left.clone(),
            right: frame.right.clone(),
        })
    }
}

/// Result of rendering one composed 2-view canvas to a switcher window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherTwoViewComposedCanvasRenderResult {
    Rendered {
        render: SwitcherWindowRenderSuccess,
    },
    RenderDeferred {
        reason: SwitcherWindowRenderDeferredReason,
    },
    BackendUnavailable {
        reason: SwitcherWindowBackendUnavailableReason,
        message: Option<String>,
    },
    InvalidComposedFrame {
        error: SwitcherTwoViewComposedFrameRenderInputError,
    },
    RenderFailed {
        message: String,
    },
}

/// Thin render boundary for a composed 2-view canvas.
///
/// Composition stays upstream; this boundary only validates the composed canvas
/// and hands it to the existing one-frame window render runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewComposedCanvasRenderBoundary;

impl SwitcherTwoViewComposedCanvasRenderBoundary {
    pub fn render_composed_frame_with_runtime(
        &self,
        frame: &SwitcherTwoViewComposedFrame,
        title: impl Into<String>,
        hold_millis: u64,
        runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewComposedCanvasRenderResult {
        let input = match SwitcherTwoViewComposedFrameRenderInput::from_composed_frame(frame) {
            Ok(input) => input,
            Err(error) => {
                return SwitcherTwoViewComposedCanvasRenderResult::InvalidComposedFrame { error };
            }
        };

        match runtime.render_once(SwitcherWindowRenderRequest {
            frame: input.frame,
            title: title.into(),
            hold_millis,
        }) {
            SwitcherWindowRenderResult::Rendered(render) => {
                SwitcherTwoViewComposedCanvasRenderResult::Rendered { render }
            }
            SwitcherWindowRenderResult::RenderDeferred { reason } => {
                SwitcherTwoViewComposedCanvasRenderResult::RenderDeferred { reason }
            }
            SwitcherWindowRenderResult::BackendUnavailable { reason, message } => {
                SwitcherTwoViewComposedCanvasRenderResult::BackendUnavailable { reason, message }
            }
            SwitcherWindowRenderResult::InvalidFrame { error } => {
                SwitcherTwoViewComposedCanvasRenderResult::InvalidComposedFrame {
                    error: SwitcherTwoViewComposedFrameRenderInputError::InvalidFrame(error),
                }
            }
            SwitcherWindowRenderResult::RenderFailed { message } => {
                SwitcherTwoViewComposedCanvasRenderResult::RenderFailed { message }
            }
        }
    }
}

/// Pure 2-view side-by-side BGRA composition boundary.
///
/// This does not select, decode, render to a window, mutate queues, schedule a
/// loop, compose 4-view, or integrate OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewCompositionBoundary;

impl SwitcherTwoViewCompositionBoundary {
    pub fn compose_side_by_side(
        &self,
        input: SwitcherTwoViewCompositionInput,
    ) -> SwitcherTwoViewCompositionResult {
        let left_side = match validate_two_view_layout_side(SwitcherTwoViewSide::Left, input.left) {
            Ok(side) => side,
            Err(reason) => return SwitcherTwoViewCompositionResult::InvalidDimensions { reason },
        };
        let right_side =
            match validate_two_view_layout_side(SwitcherTwoViewSide::Right, input.right) {
                Ok(side) => side,
                Err(reason) => {
                    return SwitcherTwoViewCompositionResult::InvalidDimensions { reason };
                }
            };

        match (left_side, right_side) {
            (
                ValidatedTwoViewLayoutSide::Decoded(left),
                ValidatedTwoViewLayoutSide::Decoded(right),
            ) => match compose_two_view_canvas(Some(left), Some(right), input.policy) {
                Ok(frame) => SwitcherTwoViewCompositionResult::BothComposed { frame },
                Err(reason) => SwitcherTwoViewCompositionResult::InvalidDimensions { reason },
            },
            (
                ValidatedTwoViewLayoutSide::Decoded(left),
                ValidatedTwoViewLayoutSide::Skipped(right_reason),
            ) => match compose_two_view_canvas(Some(left), None, input.policy) {
                Ok(frame) => SwitcherTwoViewCompositionResult::LeftOnly {
                    frame,
                    right_placeholder_reason: right_reason,
                },
                Err(reason) => SwitcherTwoViewCompositionResult::InvalidDimensions { reason },
            },
            (
                ValidatedTwoViewLayoutSide::Skipped(left_reason),
                ValidatedTwoViewLayoutSide::Decoded(right),
            ) => match compose_two_view_canvas(None, Some(right), input.policy) {
                Ok(frame) => SwitcherTwoViewCompositionResult::RightOnly {
                    frame,
                    left_placeholder_reason: left_reason,
                },
                Err(reason) => SwitcherTwoViewCompositionResult::InvalidDimensions { reason },
            },
            (
                ValidatedTwoViewLayoutSide::Skipped(left_reason),
                ValidatedTwoViewLayoutSide::Skipped(right_reason),
            ) => SwitcherTwoViewCompositionResult::EmptyPlaceholder {
                left_reason,
                right_reason,
            },
        }
    }
}

struct DecodedTwoViewLayoutSide {
    selected: Option<SwitcherJitterBufferSelectedFrame>,
    frame: SwitcherDecodedFrame,
}

enum ValidatedTwoViewLayoutSide {
    Decoded(DecodedTwoViewLayoutSide),
    Skipped(SwitcherTwoViewManualDecodeRenderStatus),
}

fn validate_two_view_layout_side(
    expected_side: SwitcherTwoViewSide,
    input: SwitcherTwoViewLayoutSideInput,
) -> Result<ValidatedTwoViewLayoutSide, SwitcherTwoViewCompositionInvalidReason> {
    match input {
        SwitcherTwoViewLayoutSideInput::Decoded {
            side,
            selected,
            frame,
        } => {
            if side != expected_side {
                return Err(SwitcherTwoViewCompositionInvalidReason::WrongSide {
                    expected: expected_side,
                    actual: side,
                });
            }
            validate_two_view_decoded_frame(side, &frame)?;
            Ok(ValidatedTwoViewLayoutSide::Decoded(
                DecodedTwoViewLayoutSide { selected, frame },
            ))
        }
        SwitcherTwoViewLayoutSideInput::Skipped { side, reason } => {
            if side != expected_side {
                return Err(SwitcherTwoViewCompositionInvalidReason::WrongSide {
                    expected: expected_side,
                    actual: side,
                });
            }
            Ok(ValidatedTwoViewLayoutSide::Skipped(reason))
        }
    }
}

fn validate_two_view_decoded_frame(
    side: SwitcherTwoViewSide,
    frame: &SwitcherDecodedFrame,
) -> Result<(), SwitcherTwoViewCompositionInvalidReason> {
    if frame.pixel_format != SwitcherDecodedFramePixelFormat::Bgra8 {
        return Err(
            SwitcherTwoViewCompositionInvalidReason::UnsupportedPixelFormat {
                side,
                actual: frame.pixel_format,
            },
        );
    }
    if frame.width == 0 || frame.height == 0 {
        return Err(SwitcherTwoViewCompositionInvalidReason::InvalidDimensions { side });
    }
    let Some(expected) = frame
        .width
        .checked_mul(frame.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|len| len as usize)
    else {
        return Err(SwitcherTwoViewCompositionInvalidReason::CanvasTooLarge);
    };
    if frame.pixels.len() != expected {
        return Err(
            SwitcherTwoViewCompositionInvalidReason::InvalidBufferLength {
                side,
                expected,
                actual: frame.pixels.len(),
            },
        );
    }
    Ok(())
}

fn compose_two_view_canvas(
    left: Option<DecodedTwoViewLayoutSide>,
    right: Option<DecodedTwoViewLayoutSide>,
    policy: SwitcherTwoViewLayoutPolicy,
) -> Result<SwitcherTwoViewComposedFrame, SwitcherTwoViewCompositionInvalidReason> {
    let Some((slot_width, slot_height)) = two_view_slot_size(left.as_ref(), right.as_ref()) else {
        return Ok(SwitcherTwoViewComposedFrame {
            width: 0,
            height: 0,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: Vec::new(),
            left: None,
            right: None,
        });
    };
    let canvas_width = slot_width
        .checked_mul(2)
        .ok_or(SwitcherTwoViewCompositionInvalidReason::CanvasTooLarge)?;
    let canvas_len = canvas_width
        .checked_mul(slot_height)
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|len| len as usize)
        .ok_or(SwitcherTwoViewCompositionInvalidReason::CanvasTooLarge)?;
    let mut pixels = vec![0; canvas_len];
    fill_bgra(&mut pixels, policy.placeholder_bgra);

    let left_metadata = match left {
        Some(side) => {
            copy_bgra_frame_into_canvas(&mut pixels, canvas_width, &side.frame, 0, 0);
            Some(SwitcherTwoViewComposedSideMetadata {
                side: SwitcherTwoViewSide::Left,
                selected: side.selected,
                x: 0,
                y: 0,
                width: side.frame.width,
                height: side.frame.height,
            })
        }
        None => None,
    };
    let right_metadata = match right {
        Some(side) => {
            copy_bgra_frame_into_canvas(&mut pixels, canvas_width, &side.frame, slot_width, 0);
            Some(SwitcherTwoViewComposedSideMetadata {
                side: SwitcherTwoViewSide::Right,
                selected: side.selected,
                x: slot_width,
                y: 0,
                width: side.frame.width,
                height: side.frame.height,
            })
        }
        None => None,
    };

    Ok(SwitcherTwoViewComposedFrame {
        width: canvas_width,
        height: slot_height,
        pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
        pixels,
        left: left_metadata,
        right: right_metadata,
    })
}

fn two_view_slot_size(
    left: Option<&DecodedTwoViewLayoutSide>,
    right: Option<&DecodedTwoViewLayoutSide>,
) -> Option<(u32, u32)> {
    let width = left
        .map(|side| side.frame.width)
        .into_iter()
        .chain(right.map(|side| side.frame.width))
        .max()?;
    let height = left
        .map(|side| side.frame.height)
        .into_iter()
        .chain(right.map(|side| side.frame.height))
        .max()?;
    Some((width, height))
}

fn fill_bgra(pixels: &mut [u8], color: [u8; 4]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&color);
    }
}

fn copy_bgra_frame_into_canvas(
    canvas: &mut [u8],
    canvas_width: u32,
    frame: &SwitcherDecodedFrame,
    dst_x: u32,
    dst_y: u32,
) {
    let src_stride = frame.width as usize * 4;
    let canvas_stride = canvas_width as usize * 4;
    for row in 0..frame.height as usize {
        let src_start = row * src_stride;
        let dst_start = (dst_y as usize + row) * canvas_stride + dst_x as usize * 4;
        let dst_end = dst_start + src_stride;
        canvas[dst_start..dst_end]
            .copy_from_slice(&frame.pixels[src_start..src_start + src_stride]);
    }
}

/// Bounded policy for one live-like 2-client queue/runtime integration run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitcherLiveTwoViewRuntimePolicy {
    pub max_packets: usize,
    pub current_switcher_time: TimestampMicros,
    pub selection: SwitcherTwoViewTargetTimeSelectionPolicy,
    pub queue: ServerVideoFrameQueuePolicy,
    pub render_hold_millis: u64,
    pub composition: SwitcherTwoViewLayoutPolicy,
}

impl Default for SwitcherLiveTwoViewRuntimePolicy {
    fn default() -> Self {
        Self {
            max_packets: 2,
            current_switcher_time: TimestampMicros(0),
            selection: SwitcherTwoViewTargetTimeSelectionPolicy::default(),
            queue: ServerVideoFrameQueuePolicy::default(),
            render_hold_millis: 16,
            composition: SwitcherTwoViewLayoutPolicy::default(),
        }
    }
}

/// Input for one bounded live-like 2-client switcher runtime run.
pub struct SwitcherLiveTwoViewRuntimeInput<'a, S> {
    pub source: &'a mut S,
    pub left_client_id: &'a ClientId,
    pub right_client_id: &'a ClientId,
    pub policy: SwitcherLiveTwoViewRuntimePolicy,
}

/// One item emitted by a live queue owner or test source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherLiveTwoViewQueueSourceItem {
    AcceptedVideoFrame {
        packet: ServerRegisteredVideoFramePacket,
        queued_at: TimestampMicros,
    },
    RejectedVideoFrame {
        client_id: ClientId,
        reason: PacketAcceptanceRejectReason,
    },
    ProtocolDecodeFailed {
        source: Option<PacketSource>,
        message: String,
    },
    ReceiveFailed {
        message: String,
    },
    NonVideoPacket {
        message_type: MessageType,
    },
    Timeout,
    EndOfInput,
}

/// Caller-owned source for bounded live-like 2-client queue ingestion.
pub trait SwitcherLiveTwoViewQueueSource {
    fn receive_next(&mut self) -> SwitcherLiveTwoViewQueueSourceItem;
}

/// UDP socket-backed source configuration for the two-view live queue adapter.
///
/// The authenticated sender registry is still caller-owned. This adapter only
/// receives, decodes, applies the existing server acceptance gate, and maps the
/// result into `SwitcherLiveTwoViewQueueSourceItem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherUdpLiveTwoViewSourceConfig {
    pub bind_address: SocketAddr,
    pub expected_protocol_version: ProtocolVersion,
    pub left_client_id: ClientId,
    pub right_client_id: ClientId,
    pub max_packets: usize,
    pub receive_timeout: Option<Duration>,
    pub queued_at_base: TimestampMicros,
    pub buffer_len: usize,
}

impl SwitcherUdpLiveTwoViewSourceConfig {
    pub fn for_clients(
        bind_address: SocketAddr,
        expected_protocol_version: ProtocolVersion,
        left_client_id: ClientId,
        right_client_id: ClientId,
    ) -> Self {
        Self {
            bind_address,
            expected_protocol_version,
            left_client_id,
            right_client_id,
            max_packets: 1,
            receive_timeout: Some(Duration::from_millis(1)),
            queued_at_base: TimestampMicros(0),
            buffer_len: DEFAULT_UDP_PACKET_BUFFER_LEN,
        }
    }
}

/// Error from constructing the UDP-backed source adapter.
#[derive(Debug)]
pub enum SwitcherUdpLiveTwoViewSourceBindError {
    BindFailed(io::Error),
    ConfigureTimeoutFailed(io::Error),
}

/// Minimal real UDP socket-backed source for live two-view scheduling.
///
/// This adapter does not authenticate by itself and does not create registry
/// entries. Callers must pass a registry that was populated by the existing
/// server/auth path.
pub struct SwitcherUdpLiveTwoViewQueueSource {
    socket: UdpSocket,
    config: SwitcherUdpLiveTwoViewSourceConfig,
    registry: AuthenticatedSenderRegistry,
    receive_loop: ServerReceiveLoopStep,
    registered_packet: ServerRegisteredPacketBoundary,
    buffer: Vec<u8>,
    packets_read: usize,
}

impl SwitcherUdpLiveTwoViewQueueSource {
    pub fn bind(
        config: SwitcherUdpLiveTwoViewSourceConfig,
        registry: AuthenticatedSenderRegistry,
    ) -> Result<Self, SwitcherUdpLiveTwoViewSourceBindError> {
        let socket = UdpSocket::bind(config.bind_address)
            .map_err(SwitcherUdpLiveTwoViewSourceBindError::BindFailed)?;
        Self::from_socket(socket, config, registry)
    }

    pub fn from_socket(
        socket: UdpSocket,
        config: SwitcherUdpLiveTwoViewSourceConfig,
        registry: AuthenticatedSenderRegistry,
    ) -> Result<Self, SwitcherUdpLiveTwoViewSourceBindError> {
        socket
            .set_read_timeout(config.receive_timeout)
            .map_err(SwitcherUdpLiveTwoViewSourceBindError::ConfigureTimeoutFailed)?;
        Ok(Self {
            socket,
            buffer: vec![0; config.buffer_len],
            config,
            registry,
            receive_loop: ServerReceiveLoopStep::default(),
            registered_packet: ServerRegisteredPacketBoundary::default(),
            packets_read: 0,
        })
    }

    fn map_gate_outcome(
        &self,
        outcome: ServerReceiveLoopGateOutcome,
        queued_at: TimestampMicros,
    ) -> SwitcherLiveTwoViewQueueSourceItem {
        match outcome {
            ServerReceiveLoopGateOutcome::Accepted(route) => {
                self.map_accepted_route(route, queued_at)
            }
            ServerReceiveLoopGateOutcome::Rejected(ServerReceiveLoopGateRejection::Acceptance(
                rejection,
            )) => SwitcherLiveTwoViewQueueSourceItem::RejectedVideoFrame {
                client_id: rejection
                    .client_id
                    .unwrap_or_else(|| ClientId("unknown".to_string())),
                reason: rejection.reason,
            },
            ServerReceiveLoopGateOutcome::Rejected(ServerReceiveLoopGateRejection::Decode(
                rejection,
            )) => SwitcherLiveTwoViewQueueSourceItem::ProtocolDecodeFailed {
                source: Some(rejection.source),
                message: format!("{:?}", rejection.error),
            },
        }
    }

    fn map_accepted_route(
        &self,
        route: ServerInboundRoute,
        queued_at: TimestampMicros,
    ) -> SwitcherLiveTwoViewQueueSourceItem {
        let message_type = server_route_message_type(&route);
        let client_id = match &route {
            ServerInboundRoute::VideoFrame { frame, .. } => frame.client_id.clone(),
            _ => {
                return SwitcherLiveTwoViewQueueSourceItem::NonVideoPacket { message_type };
            }
        };

        if client_id != self.config.left_client_id && client_id != self.config.right_client_id {
            return SwitcherLiveTwoViewQueueSourceItem::RejectedVideoFrame {
                client_id,
                reason: PacketAcceptanceRejectReason::UnknownClient,
            };
        }

        match self
            .registered_packet
            .prepare_for_handler(&self.registry, route)
        {
            Ok(ServerRegisteredClientPacket::VideoFrame(packet)) => {
                SwitcherLiveTwoViewQueueSourceItem::AcceptedVideoFrame { packet, queued_at }
            }
            Ok(ServerRegisteredClientPacket::Heartbeat(_))
            | Ok(ServerRegisteredClientPacket::VideoFrameFragment(_))
            | Ok(ServerRegisteredClientPacket::ClientStats(_)) => {
                SwitcherLiveTwoViewQueueSourceItem::NonVideoPacket { message_type }
            }
            Err(_) => SwitcherLiveTwoViewQueueSourceItem::RejectedVideoFrame {
                client_id,
                reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
            },
        }
    }

    fn queued_at_for_next_packet(&self) -> TimestampMicros {
        TimestampMicros(self.config.queued_at_base.0 + self.packets_read as u64)
    }
}

impl SwitcherLiveTwoViewQueueSource for SwitcherUdpLiveTwoViewQueueSource {
    fn receive_next(&mut self) -> SwitcherLiveTwoViewQueueSourceItem {
        if self.packets_read >= self.config.max_packets {
            return SwitcherLiveTwoViewQueueSourceItem::EndOfInput;
        }

        match self.socket.recv_from(self.buffer.as_mut_slice()) {
            Ok((len, source)) => {
                let queued_at = self.queued_at_for_next_packet();
                self.packets_read += 1;
                let packet_source = PacketSource { address: source };
                let outcome = self.receive_loop.handle_received_packet_with_gate(
                    self.config.expected_protocol_version,
                    &self.registry,
                    packet_source,
                    &self.buffer[..len],
                );
                self.map_gate_outcome(outcome, queued_at)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                self.packets_read += 1;
                SwitcherLiveTwoViewQueueSourceItem::Timeout
            }
            Err(error) => SwitcherLiveTwoViewQueueSourceItem::ReceiveFailed {
                message: error.to_string(),
            },
        }
    }
}

/// Config for the bounded live two-view switcher manual runtime.
///
/// This owns only startup/manual policy. Auth remains handled by the existing
/// server auth response step, and video packets still flow through the UDP
/// source adapter and scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherLiveTwoViewManualRuntimeConfig {
    pub server_startup: ServerAuthResponsePocStartupConfig,
    pub left_client_id: ClientId,
    pub right_client_id: ClientId,
    pub auth_setup_packets: usize,
    pub receive_timeout: Option<Duration>,
    pub udp_source_max_packets: usize,
    pub source_buffer_len: usize,
    pub scheduling: SwitcherContinuousTwoViewSchedulingPolicy,
}

impl SwitcherLiveTwoViewManualRuntimeConfig {
    pub fn from_server_startup(
        server_startup: ServerAuthResponsePocStartupConfig,
        left_client_id: ClientId,
        right_client_id: ClientId,
    ) -> Self {
        let mut scheduling = SwitcherContinuousTwoViewSchedulingPolicy::default();
        scheduling.max_ticks = 4;
        scheduling.max_rendered_frames = 4;
        scheduling.tick_interval_micros = 33_333;
        scheduling.live_runtime.max_packets = 8;
        scheduling.live_runtime.current_switcher_time = TimestampMicros(1_600_011);
        scheduling.live_runtime.selection = SwitcherTwoViewTargetTimeSelectionPolicy {
            playout_delay_micros: 600_000,
            max_late_micros: 50_000,
            max_early_micros: 50_000,
            ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
        };
        scheduling.live_runtime.render_hold_millis = 16;

        Self {
            server_startup,
            left_client_id,
            right_client_id,
            auth_setup_packets: 2,
            receive_timeout: Some(Duration::from_millis(500)),
            udp_source_max_packets: 8,
            source_buffer_len: DEFAULT_UDP_PACKET_BUFFER_LEN,
            scheduling,
        }
    }

    pub fn udp_source_config(&self) -> SwitcherUdpLiveTwoViewSourceConfig {
        let mut config = SwitcherUdpLiveTwoViewSourceConfig::for_clients(
            self.server_startup.bind_address,
            self.server_startup.expected_protocol_version,
            self.left_client_id.clone(),
            self.right_client_id.clone(),
        );
        config.max_packets = self.udp_source_max_packets;
        config.receive_timeout = self.receive_timeout;
        config.queued_at_base = current_system_timestamp_micros_for_switcher();
        config.buffer_len = self.source_buffer_len;
        config
    }
}

/// Auth setup summary for the manual live two-view runtime.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwitcherLiveTwoViewManualAuthSummary {
    pub packets_expected: usize,
    pub packets_processed: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub registered_clients: usize,
    pub receive_failures: usize,
    pub auth_errors: usize,
    pub encode_failures: usize,
    pub send_failures: usize,
    pub rejected_by_gate: usize,
}

impl SwitcherLiveTwoViewManualAuthSummary {
    fn observe_outcome(&mut self, outcome: &ServerAuthResponsePocOutcome) {
        self.packets_processed += 1;
        if outcome.auth_flow.decision.accepted {
            self.accepted += 1;
        } else {
            self.rejected += 1;
        }
        if outcome.registered_sender.is_some() {
            self.registered_clients += 1;
        }
    }

    fn observe_error(&mut self, error: &ServerAuthResponsePocError) {
        self.packets_processed += 1;
        match error {
            ServerAuthResponsePocError::Receive(_) => self.receive_failures += 1,
            ServerAuthResponsePocError::Rejected(_) => self.rejected_by_gate += 1,
            ServerAuthResponsePocError::Auth(_) => self.auth_errors += 1,
            ServerAuthResponsePocError::Encode(_) => self.encode_failures += 1,
            ServerAuthResponsePocError::Send(_) => self.send_failures += 1,
        }
    }
}

/// Result from the bounded live two-view switcher manual runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherLiveTwoViewManualRuntimeResult {
    pub bind_address: SocketAddr,
    pub left_client_id: ClientId,
    pub right_client_id: ClientId,
    pub bounded_manual_runtime: bool,
    pub auth: SwitcherLiveTwoViewManualAuthSummary,
    pub scheduler: SwitcherContinuousTwoViewSchedulingResult,
}

/// Errors from launching the live two-view manual runtime.
#[derive(Debug)]
pub enum SwitcherLiveTwoViewManualRuntimeError {
    Startup(ServerAuthResponsePocStartupError),
    Bind {
        address: SocketAddr,
        error: io::Error,
    },
    ConfigureTimeout(io::Error),
    UdpSource(SwitcherUdpLiveTwoViewSourceBindError),
}

/// Bounded manual launcher/runtime for live two-view switcher verification.
///
/// It performs only three steps: receive bounded auth packets through the
/// existing server auth response path, hand the resulting caller-owned registry
/// to `SwitcherUdpLiveTwoViewQueueSource`, then run the existing continuous
/// two-view scheduler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherLiveTwoViewManualRuntimeBoundary {
    auth_step: ServerAuthResponsePocStep,
    scheduler: SwitcherContinuousTwoViewSchedulingBoundary,
}

impl SwitcherLiveTwoViewManualRuntimeBoundary {
    pub fn load_config_from_path(
        &self,
        path: impl AsRef<Path>,
        left_client_id: ClientId,
        right_client_id: ClientId,
    ) -> Result<SwitcherLiveTwoViewManualRuntimeConfig, SwitcherLiveTwoViewManualRuntimeError> {
        let server_startup = ServerAuthResponsePocLauncher::default()
            .load_startup_config_from_path(path)
            .map_err(SwitcherLiveTwoViewManualRuntimeError::Startup)?;
        Ok(SwitcherLiveTwoViewManualRuntimeConfig::from_server_startup(
            server_startup,
            left_client_id,
            right_client_id,
        ))
    }

    pub fn run_from_path_with_runtimes(
        &self,
        path: impl AsRef<Path>,
        left_client_id: ClientId,
        right_client_id: ClientId,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> Result<SwitcherLiveTwoViewManualRuntimeResult, SwitcherLiveTwoViewManualRuntimeError> {
        let config = self.load_config_from_path(path, left_client_id, right_client_id)?;
        self.run_with_runtimes(config, decode_runtime, render_runtime)
    }

    pub fn run_with_runtimes(
        &self,
        config: SwitcherLiveTwoViewManualRuntimeConfig,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> Result<SwitcherLiveTwoViewManualRuntimeResult, SwitcherLiveTwoViewManualRuntimeError> {
        let socket = UdpSocket::bind(config.server_startup.bind_address).map_err(|error| {
            SwitcherLiveTwoViewManualRuntimeError::Bind {
                address: config.server_startup.bind_address,
                error,
            }
        })?;
        self.run_from_socket_with_runtimes(socket, config, decode_runtime, render_runtime)
    }

    pub fn run_from_socket_with_runtimes(
        &self,
        socket: UdpSocket,
        config: SwitcherLiveTwoViewManualRuntimeConfig,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> Result<SwitcherLiveTwoViewManualRuntimeResult, SwitcherLiveTwoViewManualRuntimeError> {
        socket
            .set_read_timeout(config.receive_timeout)
            .map_err(SwitcherLiveTwoViewManualRuntimeError::ConfigureTimeout)?;

        let mut registry = AuthenticatedSenderRegistry::default();
        let mut buffer = vec![0_u8; config.source_buffer_len];
        let mut auth = SwitcherLiveTwoViewManualAuthSummary {
            packets_expected: config.auth_setup_packets,
            ..SwitcherLiveTwoViewManualAuthSummary::default()
        };

        for _ in 0..config.auth_setup_packets {
            match self.auth_step.run_one(
                &socket,
                &mut buffer,
                config.server_startup.expected_protocol_version,
                &config.server_startup.auth_config,
                &mut registry,
            ) {
                Ok(outcome) => auth.observe_outcome(&outcome),
                Err(error @ ServerAuthResponsePocError::Receive(_)) => {
                    auth.observe_error(&error);
                    break;
                }
                Err(error) => auth.observe_error(&error),
            }
        }

        let source_config = config.udp_source_config();
        let mut source =
            SwitcherUdpLiveTwoViewQueueSource::from_socket(socket, source_config, registry)
                .map_err(SwitcherLiveTwoViewManualRuntimeError::UdpSource)?;
        let scheduler = self.scheduler.run_with_runtimes(
            SwitcherContinuousTwoViewSchedulingInput {
                source: &mut source,
                left_client_id: &config.left_client_id,
                right_client_id: &config.right_client_id,
                policy: config.scheduling,
            },
            decode_runtime,
            render_runtime,
        );

        Ok(SwitcherLiveTwoViewManualRuntimeResult {
            bind_address: config.server_startup.bind_address,
            left_client_id: config.left_client_id,
            right_client_id: config.right_client_id,
            bounded_manual_runtime: true,
            auth,
            scheduler,
        })
    }
}

fn server_route_message_type(route: &ServerInboundRoute) -> MessageType {
    match route {
        ServerInboundRoute::AuthRequest { .. } => MessageType::AuthRequest,
        ServerInboundRoute::Heartbeat { .. } => MessageType::Heartbeat,
        ServerInboundRoute::VideoFrame { .. } => MessageType::VideoFrame,
        ServerInboundRoute::VideoFrameFragment { .. } => MessageType::VideoFrameFragment,
        ServerInboundRoute::ClientStats { .. } => MessageType::ClientStats,
        ServerInboundRoute::UnsupportedForServer { message_type, .. } => *message_type,
    }
}

fn current_system_timestamp_micros_for_switcher() -> TimestampMicros {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or(0);
    TimestampMicros(u64::try_from(micros).unwrap_or(u64::MAX))
}

/// Summary of the queue-owner phase before selection/decode/composition/render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherLiveTwoViewQueueSummary {
    pub packets_observed: usize,
    pub accepted_frames: usize,
    pub rejected_frames: usize,
    pub protocol_decode_failures: usize,
    pub receive_failures: usize,
    pub non_video_packets: usize,
    pub timeouts: usize,
    pub ended: bool,
    pub stopped_by_guard: bool,
    pub queued_left: usize,
    pub queued_right: usize,
    pub total_queued: usize,
}

/// Per-side live pipeline status after targetTime selection and decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherLiveTwoViewSidePipelineStatus {
    Decoded {
        selected: SwitcherJitterBufferSelectedFrame,
        frame: SwitcherDecodedFrame,
    },
    SelectionUnavailable {
        selection: SwitcherJitterBufferSelectionResult,
    },
    DecodeDeferred {
        selected: SwitcherJitterBufferSelectedFrame,
        reason: SwitcherH264DecodeDeferredReason,
    },
    DecodeFailed {
        selected: SwitcherJitterBufferSelectedFrame,
        failure: SwitcherH264DecodeFailure,
    },
}

/// Compact composition shape for a live 2-view pipeline attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherLiveTwoViewCompositionKind {
    Both,
    LeftOnly,
    RightOnly,
    Empty,
}

/// Rendered scope for one live 2-view pipeline attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherLiveTwoViewRenderedKind {
    Both,
    LeftOnly,
    RightOnly,
}

/// Summary of the selection/decode/composition phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherLiveTwoViewPipelineSummary {
    pub shared_target_time: TimestampMicros,
    pub left: SwitcherLiveTwoViewSidePipelineStatus,
    pub right: SwitcherLiveTwoViewSidePipelineStatus,
    pub composition_kind: SwitcherLiveTwoViewCompositionKind,
}

/// Pipeline result after live queue ingestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherLiveTwoViewPipelineResult {
    Rendered {
        kind: SwitcherLiveTwoViewRenderedKind,
        summary: SwitcherLiveTwoViewPipelineSummary,
        render: SwitcherWindowRenderSuccess,
    },
    NoFrames {
        summary: SwitcherLiveTwoViewPipelineSummary,
    },
    CompositionInvalid {
        summary: SwitcherLiveTwoViewPipelineSummary,
        reason: SwitcherTwoViewCompositionInvalidReason,
    },
    RenderDeferred {
        summary: SwitcherLiveTwoViewPipelineSummary,
        reason: SwitcherWindowRenderDeferredReason,
    },
    BackendUnavailable {
        summary: SwitcherLiveTwoViewPipelineSummary,
        reason: SwitcherWindowBackendUnavailableReason,
        message: Option<String>,
    },
    InvalidComposedFrame {
        summary: SwitcherLiveTwoViewPipelineSummary,
        error: SwitcherTwoViewComposedFrameRenderInputError,
    },
    RenderFailed {
        summary: SwitcherLiveTwoViewPipelineSummary,
        message: String,
    },
}

/// Full result of one bounded live-like 2-client switcher runtime run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherLiveTwoViewRuntimeResult {
    pub queue_state: ServerVideoFrameQueueState,
    pub queue: SwitcherLiveTwoViewQueueSummary,
    pub pipeline: SwitcherLiveTwoViewPipelineResult,
}

/// Bounded live-like 2-client queue owner -> 2-view pipeline integration.
///
/// The source owns packet reception. This boundary only stores accepted frames
/// into caller-owned queue state and then composes existing switcher boundaries
/// once. It does not mutate jitter-buffer queues for late drops, schedule a
/// continuous loop, integrate OBS, or implement 4-view orchestration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherLiveTwoViewRuntimeBoundary {
    video_input: ServerVideoFrameHandlerBoundary,
    queue_storage: ServerVideoFrameQueueStorageBoundary,
    selector: SwitcherTwoViewTargetTimeSelectionBoundary,
    decoder: SwitcherH264DecodeBoundary,
    composer: SwitcherTwoViewCompositionBoundary,
    renderer: SwitcherTwoViewComposedCanvasRenderBoundary,
}

impl SwitcherLiveTwoViewRuntimeBoundary {
    pub fn run_once<S: SwitcherLiveTwoViewQueueSource>(
        &self,
        input: SwitcherLiveTwoViewRuntimeInput<'_, S>,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherLiveTwoViewRuntimeResult {
        let mut queue_state = ServerVideoFrameQueueState::default();
        let mut packets_observed = 0;
        let mut accepted_frames = 0;
        let mut rejected_frames = 0;
        let mut protocol_decode_failures = 0;
        let mut receive_failures = 0;
        let mut non_video_packets = 0;
        let mut timeouts = 0;
        let mut ended = false;

        for _ in 0..input.policy.max_packets {
            match input.source.receive_next() {
                SwitcherLiveTwoViewQueueSourceItem::AcceptedVideoFrame { packet, queued_at } => {
                    packets_observed += 1;
                    let handler_input = self.video_input.prepare_input(packet);
                    if matches!(
                        self.queue_storage.store_frame(
                            &mut queue_state,
                            handler_input,
                            queued_at,
                            input.policy.queue,
                        ),
                        ServerVideoFrameQueueStorageResult::Stored { .. }
                    ) {
                        accepted_frames += 1;
                    }
                }
                SwitcherLiveTwoViewQueueSourceItem::RejectedVideoFrame { .. } => {
                    packets_observed += 1;
                    rejected_frames += 1;
                }
                SwitcherLiveTwoViewQueueSourceItem::ProtocolDecodeFailed { .. } => {
                    packets_observed += 1;
                    protocol_decode_failures += 1;
                }
                SwitcherLiveTwoViewQueueSourceItem::ReceiveFailed { .. } => {
                    receive_failures += 1;
                }
                SwitcherLiveTwoViewQueueSourceItem::NonVideoPacket { .. } => {
                    packets_observed += 1;
                    non_video_packets += 1;
                }
                SwitcherLiveTwoViewQueueSourceItem::Timeout => {
                    packets_observed += 1;
                    timeouts += 1;
                }
                SwitcherLiveTwoViewQueueSourceItem::EndOfInput => {
                    ended = true;
                    break;
                }
            }
        }

        let queue = SwitcherLiveTwoViewQueueSummary {
            packets_observed,
            accepted_frames,
            rejected_frames,
            protocol_decode_failures,
            receive_failures,
            non_video_packets,
            timeouts,
            ended,
            stopped_by_guard: !ended && packets_observed >= input.policy.max_packets,
            queued_left: queue_state.client_queue_len(input.left_client_id),
            queued_right: queue_state.client_queue_len(input.right_client_id),
            total_queued: queue_state.total_len(),
        };
        let pipeline = self.run_pipeline_once(
            &queue_state,
            input.left_client_id,
            input.right_client_id,
            input.policy,
            decode_runtime,
            render_runtime,
        );

        SwitcherLiveTwoViewRuntimeResult {
            queue_state,
            queue,
            pipeline,
        }
    }

    fn run_pipeline_once(
        &self,
        queue_state: &ServerVideoFrameQueueState,
        left_client_id: &ClientId,
        right_client_id: &ClientId,
        policy: SwitcherLiveTwoViewRuntimePolicy,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherLiveTwoViewPipelineResult {
        let selection = self
            .selector
            .select_pair(SwitcherTwoViewTargetTimeSelectionInput {
                queue_state,
                left_client_id,
                right_client_id,
                current_switcher_time: policy.current_switcher_time,
                policy: policy.selection,
            });
        let (shared_target_time, left_selection, right_selection) =
            clone_two_view_selection_sides(&selection);
        let left = self.decode_live_side(SwitcherTwoViewSide::Left, left_selection, decode_runtime);
        let right =
            self.decode_live_side(SwitcherTwoViewSide::Right, right_selection, decode_runtime);
        let composition_input = SwitcherTwoViewCompositionInput {
            left: live_side_to_layout_input(SwitcherTwoViewSide::Left, &left),
            right: live_side_to_layout_input(SwitcherTwoViewSide::Right, &right),
            policy: policy.composition,
        };
        let composition = self.composer.compose_side_by_side(composition_input);
        let composition_kind = live_composition_kind(&composition);
        let summary = SwitcherLiveTwoViewPipelineSummary {
            shared_target_time,
            left,
            right,
            composition_kind,
        };

        match composition {
            SwitcherTwoViewCompositionResult::BothComposed { frame } => self
                .render_live_composition(
                    frame,
                    SwitcherLiveTwoViewRenderedKind::Both,
                    summary,
                    policy.render_hold_millis,
                    render_runtime,
                ),
            SwitcherTwoViewCompositionResult::LeftOnly { frame, .. } => self
                .render_live_composition(
                    frame,
                    SwitcherLiveTwoViewRenderedKind::LeftOnly,
                    summary,
                    policy.render_hold_millis,
                    render_runtime,
                ),
            SwitcherTwoViewCompositionResult::RightOnly { frame, .. } => self
                .render_live_composition(
                    frame,
                    SwitcherLiveTwoViewRenderedKind::RightOnly,
                    summary,
                    policy.render_hold_millis,
                    render_runtime,
                ),
            SwitcherTwoViewCompositionResult::EmptyPlaceholder { .. } => {
                SwitcherLiveTwoViewPipelineResult::NoFrames { summary }
            }
            SwitcherTwoViewCompositionResult::InvalidDimensions { reason } => {
                SwitcherLiveTwoViewPipelineResult::CompositionInvalid { summary, reason }
            }
        }
    }

    fn decode_live_side(
        &self,
        _side: SwitcherTwoViewSide,
        selection: SwitcherJitterBufferSelectionResult,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
    ) -> SwitcherLiveTwoViewSidePipelineStatus {
        let selected = match selection {
            SwitcherJitterBufferSelectionResult::Selected(selected) => selected,
            selection => {
                return SwitcherLiveTwoViewSidePipelineStatus::SelectionUnavailable { selection };
            }
        };
        match self.decoder.decode_with_runtime(
            SwitcherH264DecodeInput {
                encoded_payload: selected.frame.encoded_payload.clone(),
                width: selected.frame.width,
                height: selected.frame.height,
            },
            decode_runtime,
        ) {
            SwitcherH264DecodeResult::Decoded(frame) => {
                SwitcherLiveTwoViewSidePipelineStatus::Decoded { selected, frame }
            }
            SwitcherH264DecodeResult::Deferred { reason } => {
                SwitcherLiveTwoViewSidePipelineStatus::DecodeDeferred { selected, reason }
            }
            SwitcherH264DecodeResult::Failed(failure) => {
                SwitcherLiveTwoViewSidePipelineStatus::DecodeFailed { selected, failure }
            }
        }
    }

    fn render_live_composition(
        &self,
        frame: SwitcherTwoViewComposedFrame,
        kind: SwitcherLiveTwoViewRenderedKind,
        summary: SwitcherLiveTwoViewPipelineSummary,
        hold_millis: u64,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherLiveTwoViewPipelineResult {
        match self.renderer.render_composed_frame_with_runtime(
            &frame,
            "StreamSync Switcher 2-view",
            hold_millis,
            render_runtime,
        ) {
            SwitcherTwoViewComposedCanvasRenderResult::Rendered { render } => {
                SwitcherLiveTwoViewPipelineResult::Rendered {
                    kind,
                    summary,
                    render,
                }
            }
            SwitcherTwoViewComposedCanvasRenderResult::RenderDeferred { reason } => {
                SwitcherLiveTwoViewPipelineResult::RenderDeferred { summary, reason }
            }
            SwitcherTwoViewComposedCanvasRenderResult::BackendUnavailable { reason, message } => {
                SwitcherLiveTwoViewPipelineResult::BackendUnavailable {
                    summary,
                    reason,
                    message,
                }
            }
            SwitcherTwoViewComposedCanvasRenderResult::InvalidComposedFrame { error } => {
                SwitcherLiveTwoViewPipelineResult::InvalidComposedFrame { summary, error }
            }
            SwitcherTwoViewComposedCanvasRenderResult::RenderFailed { message } => {
                SwitcherLiveTwoViewPipelineResult::RenderFailed { summary, message }
            }
        }
    }
}

fn live_side_to_layout_input(
    side: SwitcherTwoViewSide,
    status: &SwitcherLiveTwoViewSidePipelineStatus,
) -> SwitcherTwoViewLayoutSideInput {
    match status {
        SwitcherLiveTwoViewSidePipelineStatus::Decoded { selected, frame } => {
            SwitcherTwoViewLayoutSideInput::Decoded {
                side,
                selected: Some(selected.clone()),
                frame: frame.clone(),
            }
        }
        SwitcherLiveTwoViewSidePipelineStatus::SelectionUnavailable { .. } => {
            SwitcherTwoViewLayoutSideInput::skipped(
                side,
                SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
            )
        }
        SwitcherLiveTwoViewSidePipelineStatus::DecodeDeferred { .. } => {
            SwitcherTwoViewLayoutSideInput::skipped(
                side,
                SwitcherTwoViewManualDecodeRenderStatus::DecodeDeferred,
            )
        }
        SwitcherLiveTwoViewSidePipelineStatus::DecodeFailed { .. } => {
            SwitcherTwoViewLayoutSideInput::skipped(
                side,
                SwitcherTwoViewManualDecodeRenderStatus::DecodeFailed,
            )
        }
    }
}

fn live_composition_kind(
    result: &SwitcherTwoViewCompositionResult,
) -> SwitcherLiveTwoViewCompositionKind {
    match result {
        SwitcherTwoViewCompositionResult::BothComposed { .. } => {
            SwitcherLiveTwoViewCompositionKind::Both
        }
        SwitcherTwoViewCompositionResult::LeftOnly { .. } => {
            SwitcherLiveTwoViewCompositionKind::LeftOnly
        }
        SwitcherTwoViewCompositionResult::RightOnly { .. } => {
            SwitcherLiveTwoViewCompositionKind::RightOnly
        }
        SwitcherTwoViewCompositionResult::EmptyPlaceholder { .. }
        | SwitcherTwoViewCompositionResult::InvalidDimensions { .. } => {
            SwitcherLiveTwoViewCompositionKind::Empty
        }
    }
}

/// Caller-owned policy for a bounded continuous 2-view scheduling run.
///
/// The scheduler owns only logical cadence and guard policy. Each tick still
/// delegates queue ingestion, targetTime selection, decode, composition, and
/// render to `SwitcherLiveTwoViewRuntimeBoundary`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherContinuousTwoViewSchedulingPolicy {
    pub max_ticks: usize,
    pub max_rendered_frames: usize,
    pub tick_interval_micros: u64,
    pub live_runtime: SwitcherLiveTwoViewRuntimePolicy,
}

impl Default for SwitcherContinuousTwoViewSchedulingPolicy {
    fn default() -> Self {
        Self {
            max_ticks: 1,
            max_rendered_frames: 1,
            tick_interval_micros: 33_333,
            live_runtime: SwitcherLiveTwoViewRuntimePolicy::default(),
        }
    }
}

/// Input for the first bounded continuous 2-view scheduler.
pub struct SwitcherContinuousTwoViewSchedulingInput<'a, S> {
    pub source: &'a mut S,
    pub left_client_id: &'a ClientId,
    pub right_client_id: &'a ClientId,
    pub policy: SwitcherContinuousTwoViewSchedulingPolicy,
}

/// Compact per-tick outcome for scheduler-level accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherContinuousTwoViewTickOutcome {
    RenderedBoth,
    RenderedPartial,
    NoFrames,
    DecodeFailed,
    RenderNotCompleted,
}

/// One scheduler tick result. The live runtime result is preserved verbatim so
/// callers can inspect queue and per-side pipeline details without the
/// scheduler reinterpreting them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherContinuousTwoViewSchedulingTick {
    pub tick: usize,
    pub current_switcher_time: TimestampMicros,
    pub outcome: SwitcherContinuousTwoViewTickOutcome,
    pub source_ended: bool,
    pub runtime: SwitcherLiveTwoViewRuntimeResult,
}

/// Summary counters for one bounded continuous 2-view scheduling run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwitcherContinuousTwoViewSchedulingSummary {
    pub ticks_processed: usize,
    pub rendered_frames: usize,
    pub rendered_both: usize,
    pub rendered_partial: usize,
    pub no_frame_ticks: usize,
    pub decode_failed_ticks: usize,
    pub render_not_completed_ticks: usize,
    pub source_end_tick: Option<usize>,
    pub live_guard_stop_ticks: usize,
}

/// Reason the bounded continuous 2-view scheduler stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherContinuousTwoViewSchedulingStopReason {
    MaxTicksReached,
    MaxRenderedFramesReached,
    SourceEnded,
}

/// Result of one bounded continuous 2-view scheduling run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherContinuousTwoViewSchedulingResult {
    pub ticks: Vec<SwitcherContinuousTwoViewSchedulingTick>,
    pub summary: SwitcherContinuousTwoViewSchedulingSummary,
    pub stop_reason: SwitcherContinuousTwoViewSchedulingStopReason,
}

/// Minimal continuous 2-view scheduler over the live-like one-pass runtime.
///
/// This boundary does not own sockets, queue storage semantics, targetTime
/// selection, decode, composition, render, late-frame mutation, 4-view layout,
/// or OBS integration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherContinuousTwoViewSchedulingBoundary {
    live_runtime: SwitcherLiveTwoViewRuntimeBoundary,
}

impl SwitcherContinuousTwoViewSchedulingBoundary {
    pub fn run_with_runtimes<S: SwitcherLiveTwoViewQueueSource>(
        &self,
        input: SwitcherContinuousTwoViewSchedulingInput<'_, S>,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherContinuousTwoViewSchedulingResult {
        let mut ticks = Vec::new();
        let mut summary = SwitcherContinuousTwoViewSchedulingSummary::default();

        for tick in 0..input.policy.max_ticks {
            if summary.rendered_frames >= input.policy.max_rendered_frames {
                return SwitcherContinuousTwoViewSchedulingResult {
                    ticks,
                    summary,
                    stop_reason:
                        SwitcherContinuousTwoViewSchedulingStopReason::MaxRenderedFramesReached,
                };
            }

            let current_switcher_time = TimestampMicros(
                input.policy.live_runtime.current_switcher_time.0
                    + input.policy.tick_interval_micros * tick as u64,
            );
            let mut live_policy = input.policy.live_runtime;
            live_policy.current_switcher_time = current_switcher_time;
            let runtime = self.live_runtime.run_once(
                SwitcherLiveTwoViewRuntimeInput {
                    source: input.source,
                    left_client_id: input.left_client_id,
                    right_client_id: input.right_client_id,
                    policy: live_policy,
                },
                decode_runtime,
                render_runtime,
            );
            let outcome = continuous_two_view_tick_outcome(&runtime.pipeline);

            summary.ticks_processed += 1;
            if runtime.queue.stopped_by_guard {
                summary.live_guard_stop_ticks += 1;
            }
            match outcome {
                SwitcherContinuousTwoViewTickOutcome::RenderedBoth => {
                    summary.rendered_frames += 1;
                    summary.rendered_both += 1;
                }
                SwitcherContinuousTwoViewTickOutcome::RenderedPartial => {
                    summary.rendered_frames += 1;
                    summary.rendered_partial += 1;
                }
                SwitcherContinuousTwoViewTickOutcome::NoFrames => {
                    summary.no_frame_ticks += 1;
                }
                SwitcherContinuousTwoViewTickOutcome::DecodeFailed => {
                    summary.decode_failed_ticks += 1;
                }
                SwitcherContinuousTwoViewTickOutcome::RenderNotCompleted => {
                    summary.render_not_completed_ticks += 1;
                }
            }
            let source_ended = runtime.queue.ended;
            if source_ended && summary.source_end_tick.is_none() {
                summary.source_end_tick = Some(tick);
            }
            ticks.push(SwitcherContinuousTwoViewSchedulingTick {
                tick,
                current_switcher_time,
                outcome,
                source_ended,
                runtime,
            });

            if source_ended {
                return SwitcherContinuousTwoViewSchedulingResult {
                    ticks,
                    summary,
                    stop_reason: SwitcherContinuousTwoViewSchedulingStopReason::SourceEnded,
                };
            }
        }

        let stop_reason = if summary.rendered_frames >= input.policy.max_rendered_frames {
            SwitcherContinuousTwoViewSchedulingStopReason::MaxRenderedFramesReached
        } else {
            SwitcherContinuousTwoViewSchedulingStopReason::MaxTicksReached
        };

        SwitcherContinuousTwoViewSchedulingResult {
            ticks,
            summary,
            stop_reason,
        }
    }
}

fn continuous_two_view_tick_outcome(
    pipeline: &SwitcherLiveTwoViewPipelineResult,
) -> SwitcherContinuousTwoViewTickOutcome {
    match pipeline {
        SwitcherLiveTwoViewPipelineResult::Rendered { kind, summary, .. } => {
            if live_summary_has_decode_failure(summary) {
                return SwitcherContinuousTwoViewTickOutcome::DecodeFailed;
            }
            match kind {
                SwitcherLiveTwoViewRenderedKind::Both => {
                    SwitcherContinuousTwoViewTickOutcome::RenderedBoth
                }
                SwitcherLiveTwoViewRenderedKind::LeftOnly
                | SwitcherLiveTwoViewRenderedKind::RightOnly => {
                    SwitcherContinuousTwoViewTickOutcome::RenderedPartial
                }
            }
        }
        SwitcherLiveTwoViewPipelineResult::NoFrames { summary }
        | SwitcherLiveTwoViewPipelineResult::CompositionInvalid { summary, .. } => {
            if live_summary_has_decode_failure(summary) {
                SwitcherContinuousTwoViewTickOutcome::DecodeFailed
            } else {
                SwitcherContinuousTwoViewTickOutcome::NoFrames
            }
        }
        SwitcherLiveTwoViewPipelineResult::RenderDeferred { summary, .. }
        | SwitcherLiveTwoViewPipelineResult::BackendUnavailable { summary, .. }
        | SwitcherLiveTwoViewPipelineResult::InvalidComposedFrame { summary, .. }
        | SwitcherLiveTwoViewPipelineResult::RenderFailed { summary, .. } => {
            if live_summary_has_decode_failure(summary) {
                SwitcherContinuousTwoViewTickOutcome::DecodeFailed
            } else {
                SwitcherContinuousTwoViewTickOutcome::RenderNotCompleted
            }
        }
    }
}

fn live_summary_has_decode_failure(summary: &SwitcherLiveTwoViewPipelineSummary) -> bool {
    matches!(
        summary.left,
        SwitcherLiveTwoViewSidePipelineStatus::DecodeFailed { .. }
    ) || matches!(
        summary.right,
        SwitcherLiveTwoViewSidePipelineStatus::DecodeFailed { .. }
    )
}

/// Manual/runtime input for one 2-view targetTime sync verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherTwoViewManualVerificationInput<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub left_client_id: &'a ClientId,
    pub right_client_id: &'a ClientId,
    pub current_switcher_time: TimestampMicros,
    pub policy: SwitcherTwoViewTargetTimeSelectionPolicy,
    pub render_hold_millis: u64,
}

/// Compact per-side selection status for manual/runtime output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherTwoViewManualSelectionStatus {
    Selected,
    NoFrame,
    WaitingForBuffer,
    FrameTooEarly,
    FrameTooLateDropped,
}

/// Compact per-side decode/render status for manual/runtime output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherTwoViewManualDecodeRenderStatus {
    Rendered,
    SkippedSelectionUnavailable,
    DecodeDeferred,
    DecodeFailed,
    RenderDeferred,
    WindowBackendUnavailable,
    InvalidFrame,
    RenderFailed,
}

/// Compact per-side summary for manual/runtime output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewManualVerificationSideSummary {
    pub side: SwitcherTwoViewSide,
    pub client_id: ClientId,
    pub selection_status: SwitcherTwoViewManualSelectionStatus,
    pub decode_render_status: SwitcherTwoViewManualDecodeRenderStatus,
    pub frame_id: Option<u64>,
    pub encoded_payload_len: Option<usize>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub adjusted_capture_timestamp: Option<TimestampMicros>,
}

/// Compact summary for one 2-view sync manual/runtime verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewManualVerificationSummary {
    pub shared_target_time: TimestampMicros,
    pub left: SwitcherTwoViewManualVerificationSideSummary,
    pub right: SwitcherTwoViewManualVerificationSideSummary,
}

/// Result of one 2-view sync manual/runtime verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherTwoViewManualVerificationResult {
    pub selection: SwitcherTwoViewTargetTimeSelectionResult,
    pub render: SwitcherTwoViewDecodeRenderResult,
    pub summary: SwitcherTwoViewManualVerificationSummary,
}

/// Runtime/manual verification for 2-view targetTime selection -> decode/render.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherTwoViewManualVerificationBoundary {
    selection: SwitcherTwoViewTargetTimeSelectionBoundary,
    decode_render: SwitcherTwoViewDecodeRenderBoundary,
}

impl SwitcherTwoViewManualVerificationBoundary {
    pub fn verify_with_runtimes(
        &self,
        input: SwitcherTwoViewManualVerificationInput<'_>,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewManualVerificationResult {
        let selection = self
            .selection
            .select_pair(SwitcherTwoViewTargetTimeSelectionInput {
                queue_state: input.queue_state,
                left_client_id: input.left_client_id,
                right_client_id: input.right_client_id,
                current_switcher_time: input.current_switcher_time,
                policy: input.policy,
            });
        let render = self.decode_render.render_selected_pair_with_runtimes(
            SwitcherTwoViewDecodeRenderInput {
                selection: selection.clone(),
                left_window_title: format!("StreamSync {}", input.left_client_id.0),
                right_window_title: format!("StreamSync {}", input.right_client_id.0),
                render_hold_millis: input.render_hold_millis,
            },
            decode_runtime,
            render_runtime,
        );
        let summary = two_view_manual_summary(
            &selection,
            &render,
            input.left_client_id,
            input.right_client_id,
        );
        SwitcherTwoViewManualVerificationResult {
            selection,
            render,
            summary,
        }
    }
}

fn two_view_manual_summary(
    selection: &SwitcherTwoViewTargetTimeSelectionResult,
    render: &SwitcherTwoViewDecodeRenderResult,
    left_client_id: &ClientId,
    right_client_id: &ClientId,
) -> SwitcherTwoViewManualVerificationSummary {
    let (shared_target_time, left_selection, right_selection) =
        clone_two_view_selection_sides(selection);
    SwitcherTwoViewManualVerificationSummary {
        shared_target_time,
        left: two_view_manual_side_summary(
            SwitcherTwoViewSide::Left,
            left_client_id,
            left_selection,
            two_view_decode_render_status_for_side(render, SwitcherTwoViewSide::Left),
        ),
        right: two_view_manual_side_summary(
            SwitcherTwoViewSide::Right,
            right_client_id,
            right_selection,
            two_view_decode_render_status_for_side(render, SwitcherTwoViewSide::Right),
        ),
    }
}

fn clone_two_view_selection_sides(
    selection: &SwitcherTwoViewTargetTimeSelectionResult,
) -> (
    TimestampMicros,
    SwitcherJitterBufferSelectionResult,
    SwitcherJitterBufferSelectionResult,
) {
    match selection {
        SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
            shared_target_time,
            left,
            right,
        } => (
            *shared_target_time,
            SwitcherJitterBufferSelectionResult::Selected(left.clone()),
            SwitcherJitterBufferSelectionResult::Selected(right.clone()),
        ),
        SwitcherTwoViewTargetTimeSelectionResult::Partial {
            shared_target_time,
            left,
            right,
        }
        | SwitcherTwoViewTargetTimeSelectionResult::BothUnavailable {
            shared_target_time,
            left,
            right,
        } => (*shared_target_time, left.clone(), right.clone()),
    }
}

fn two_view_manual_side_summary(
    side: SwitcherTwoViewSide,
    client_id: &ClientId,
    selection: SwitcherJitterBufferSelectionResult,
    decode_render_status: SwitcherTwoViewManualDecodeRenderStatus,
) -> SwitcherTwoViewManualVerificationSideSummary {
    let selection_status = two_view_manual_selection_status(&selection);
    let selected = match &selection {
        SwitcherJitterBufferSelectionResult::Selected(selected) => Some(selected),
        _ => None,
    };
    SwitcherTwoViewManualVerificationSideSummary {
        side,
        client_id: client_id.clone(),
        selection_status,
        decode_render_status,
        frame_id: selected.map(|selected| selected.frame.frame_id),
        encoded_payload_len: selected.map(|selected| selected.frame.encoded_payload_len),
        width: selected.map(|selected| selected.frame.width),
        height: selected.map(|selected| selected.frame.height),
        adjusted_capture_timestamp: selected.map(|selected| selected.adjusted_capture_timestamp),
    }
}

fn two_view_manual_selection_status(
    selection: &SwitcherJitterBufferSelectionResult,
) -> SwitcherTwoViewManualSelectionStatus {
    match selection {
        SwitcherJitterBufferSelectionResult::Selected(_) => {
            SwitcherTwoViewManualSelectionStatus::Selected
        }
        SwitcherJitterBufferSelectionResult::NoFrame { .. } => {
            SwitcherTwoViewManualSelectionStatus::NoFrame
        }
        SwitcherJitterBufferSelectionResult::WaitingForBuffer { .. } => {
            SwitcherTwoViewManualSelectionStatus::WaitingForBuffer
        }
        SwitcherJitterBufferSelectionResult::FrameTooEarly { .. } => {
            SwitcherTwoViewManualSelectionStatus::FrameTooEarly
        }
        SwitcherJitterBufferSelectionResult::FrameTooLateDropped { .. } => {
            SwitcherTwoViewManualSelectionStatus::FrameTooLateDropped
        }
    }
}

fn two_view_decode_render_status_for_side(
    render: &SwitcherTwoViewDecodeRenderResult,
    side: SwitcherTwoViewSide,
) -> SwitcherTwoViewManualDecodeRenderStatus {
    match render {
        SwitcherTwoViewDecodeRenderResult::BothRendered { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        }
        SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { right, .. }
            if side == SwitcherTwoViewSide::Left =>
        {
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        }
        SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { right, .. } => {
            two_view_skipped_status(right)
        }
        SwitcherTwoViewDecodeRenderResult::RightRenderedLeftSkipped { left, .. }
            if side == SwitcherTwoViewSide::Right =>
        {
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        }
        SwitcherTwoViewDecodeRenderResult::RightRenderedLeftSkipped { left, .. } => {
            two_view_skipped_status(left)
        }
        SwitcherTwoViewDecodeRenderResult::BothSkipped { left, right, .. } => match side {
            SwitcherTwoViewSide::Left => two_view_skipped_status(left),
            SwitcherTwoViewSide::Right => two_view_skipped_status(right),
        },
    }
}

fn two_view_skipped_status(
    skipped: &SwitcherTwoViewSkippedSide,
) -> SwitcherTwoViewManualDecodeRenderStatus {
    match skipped {
        SwitcherTwoViewSkippedSide::SelectionUnavailable { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        }
        SwitcherTwoViewSkippedSide::DecodeDeferred { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::DecodeDeferred
        }
        SwitcherTwoViewSkippedSide::DecodeFailed { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::DecodeFailed
        }
        SwitcherTwoViewSkippedSide::RenderDeferred { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::RenderDeferred
        }
        SwitcherTwoViewSkippedSide::WindowBackendUnavailable { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::WindowBackendUnavailable
        }
        SwitcherTwoViewSkippedSide::InvalidFrame { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::InvalidFrame
        }
        SwitcherTwoViewSkippedSide::RenderFailed { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::RenderFailed
        }
    }
}

fn handoff_display_composition_skipped_status(
    skipped: &SwitcherTwoViewHandoffDecodeRenderSkippedSide,
) -> SwitcherTwoViewManualDecodeRenderStatus {
    match skipped {
        SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
        | SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget {
            ..
        }
        | SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError { .. } => {
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        }
        SwitcherTwoViewHandoffDecodeRenderSkippedSide::DecodeRenderSkipped { skipped, .. } => {
            two_view_skipped_status(skipped)
        }
    }
}

/// Stop policy for the first continuous single-client render loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitcherContinuousRenderLoopPolicy {
    pub max_iterations: usize,
    pub max_rendered_frames: usize,
    pub render_hold_millis: u64,
}

impl Default for SwitcherContinuousRenderLoopPolicy {
    fn default() -> Self {
        Self {
            max_iterations: 1,
            max_rendered_frames: 1,
            render_hold_millis: 16,
        }
    }
}

/// Input for one continuous single-client decode/render loop run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherContinuousRenderLoopInput {
    pub client_id: ClientId,
    pub window_title: String,
    pub policy: SwitcherContinuousRenderLoopPolicy,
}

/// Caller-owned provider of latest encoded frames for the continuous loop.
///
/// A later runtime can pull from a live server/switcher queue. Tests can supply
/// deterministic scripted sources without creating windows.
pub trait SwitcherContinuousFrameSource {
    fn select_latest(&mut self, client_id: &ClientId) -> SwitcherSingleViewFrameSelectionResult;
}

/// Read-only queue-backed source for the continuous render loop.
pub struct SwitcherQueueLatestFrameSource<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    selection: SwitcherSingleViewLatestFrameSelectionBoundary,
}

impl<'a> SwitcherQueueLatestFrameSource<'a> {
    pub fn new(queue_state: &'a ServerVideoFrameQueueState) -> Self {
        Self {
            queue_state,
            selection: SwitcherSingleViewLatestFrameSelectionBoundary,
        }
    }
}

impl SwitcherContinuousFrameSource for SwitcherQueueLatestFrameSource<'_> {
    fn select_latest(&mut self, client_id: &ClientId) -> SwitcherSingleViewFrameSelectionResult {
        self.selection
            .select_latest(SwitcherSingleViewFrameSelectionInput {
                queue_state: self.queue_state,
                client_id,
            })
    }
}

/// Reason the continuous render loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherContinuousRenderLoopStopReason {
    MaxIterationsReached,
    MaxRenderedFramesReached,
}

/// One continuous render loop event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherContinuousRenderLoopEvent {
    Rendered {
        iteration: usize,
        frame_id: u64,
        render: SwitcherWindowRenderSuccess,
    },
    NoFrame {
        iteration: usize,
        client_id: ClientId,
    },
    DecodeDeferred {
        iteration: usize,
        frame_id: u64,
        reason: SwitcherH264DecodeDeferredReason,
    },
    DecodeFailed {
        iteration: usize,
        frame_id: u64,
        failure: SwitcherH264DecodeFailure,
    },
    RenderNotCompleted {
        iteration: usize,
        frame_id: u64,
        result: SwitcherWindowRenderResult,
    },
}

/// Summary counters for one continuous render loop run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwitcherContinuousRenderLoopSummary {
    pub iterations: usize,
    pub rendered_frames: usize,
    pub no_frame_count: usize,
    pub decode_deferred_count: usize,
    pub decode_failed_count: usize,
    pub render_not_completed_count: usize,
}

/// Result of one bounded continuous render loop run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherContinuousRenderLoopResult {
    pub events: Vec<SwitcherContinuousRenderLoopEvent>,
    pub summary: SwitcherContinuousRenderLoopSummary,
    pub stop_reason: SwitcherContinuousRenderLoopStopReason,
}

/// Minimal continuous single-client decode/render loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherContinuousRenderLoopBoundary {
    decoder: SwitcherH264DecodeBoundary,
    renderer: SwitcherWindowRenderBoundary,
}

impl SwitcherContinuousRenderLoopBoundary {
    pub fn run_with_runtimes(
        &self,
        input: SwitcherContinuousRenderLoopInput,
        source: &mut impl SwitcherContinuousFrameSource,
        decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
        render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherContinuousRenderLoopResult {
        let mut events = Vec::new();
        let mut summary = SwitcherContinuousRenderLoopSummary::default();

        for iteration in 0..input.policy.max_iterations {
            if summary.rendered_frames >= input.policy.max_rendered_frames {
                return SwitcherContinuousRenderLoopResult {
                    events,
                    summary,
                    stop_reason: SwitcherContinuousRenderLoopStopReason::MaxRenderedFramesReached,
                };
            }

            summary.iterations += 1;
            let selected = match source.select_latest(&input.client_id) {
                SwitcherSingleViewFrameSelectionResult::FrameAvailable(selected) => selected,
                SwitcherSingleViewFrameSelectionResult::NoFrameAvailable { client_id } => {
                    summary.no_frame_count += 1;
                    events.push(SwitcherContinuousRenderLoopEvent::NoFrame {
                        iteration,
                        client_id,
                    });
                    continue;
                }
            };

            let decode = self.decoder.decode_with_runtime(
                SwitcherH264DecodeInput {
                    encoded_payload: selected.encoded_payload.clone(),
                    width: selected.width,
                    height: selected.height,
                },
                decode_runtime,
            );
            let decoded = match decode {
                SwitcherH264DecodeResult::Decoded(decoded) => decoded,
                SwitcherH264DecodeResult::Deferred { reason } => {
                    summary.decode_deferred_count += 1;
                    events.push(SwitcherContinuousRenderLoopEvent::DecodeDeferred {
                        iteration,
                        frame_id: selected.frame_id,
                        reason,
                    });
                    continue;
                }
                SwitcherH264DecodeResult::Failed(failure) => {
                    summary.decode_failed_count += 1;
                    events.push(SwitcherContinuousRenderLoopEvent::DecodeFailed {
                        iteration,
                        frame_id: selected.frame_id,
                        failure,
                    });
                    continue;
                }
            };

            match self.renderer.render_decoded_frame_with_runtime(
                &decoded,
                input.window_title.clone(),
                input.policy.render_hold_millis,
                render_runtime,
            ) {
                SwitcherWindowRenderResult::Rendered(render) => {
                    summary.rendered_frames += 1;
                    events.push(SwitcherContinuousRenderLoopEvent::Rendered {
                        iteration,
                        frame_id: selected.frame_id,
                        render,
                    });
                }
                result => {
                    summary.render_not_completed_count += 1;
                    events.push(SwitcherContinuousRenderLoopEvent::RenderNotCompleted {
                        iteration,
                        frame_id: selected.frame_id,
                        result,
                    });
                }
            }
        }

        let stop_reason = if summary.rendered_frames >= input.policy.max_rendered_frames {
            SwitcherContinuousRenderLoopStopReason::MaxRenderedFramesReached
        } else {
            SwitcherContinuousRenderLoopStopReason::MaxIterationsReached
        };

        SwitcherContinuousRenderLoopResult {
            events,
            summary,
            stop_reason,
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_render_once(request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
    use std::{ptr::null_mut, thread, time::Duration};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, EndPaint, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        PAINTSTRUCT, SRCCOPY,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
        RegisterClassW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, MSG,
        PM_REMOVE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY, WM_PAINT, WNDCLASSW,
        WS_OVERLAPPEDWINDOW,
    };

    static mut PAINT_FRAME: Option<SwitcherDecodedFrameRenderInput> = None;

    #[allow(static_mut_refs)]
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                if let Some(frame) = PAINT_FRAME.as_ref() {
                    let mut info = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: frame.width as i32,
                            biHeight: -(frame.height as i32),
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB.0,
                            biSizeImage: frame.pixels.len() as u32,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let _ = StretchDIBits(
                        hdc,
                        0,
                        0,
                        frame.width as i32,
                        frame.height as i32,
                        0,
                        0,
                        frame.width as i32,
                        frame.height as i32,
                        Some(frame.pixels.as_ptr().cast()),
                        &mut info,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                }
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe {
        PAINT_FRAME = Some(request.frame.clone());
    }

    let instance = match unsafe { GetModuleHandleW(None) } {
        Ok(instance) => instance,
        Err(error) => {
            return SwitcherWindowRenderResult::RenderFailed {
                message: format!("GetModuleHandleW failed: {error:?}"),
            };
        }
    };
    let class_name = w!("StreamSyncSwitcherOneShotWindow");
    let wnd_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        ..Default::default()
    };
    let _ = unsafe { RegisterClassW(&wnd_class) };
    let title: Vec<u16> = request.title.encode_utf16().chain(Some(0)).collect();
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            request.frame.width as i32,
            request.frame.height as i32,
            None,
            None,
            Some(instance.into()),
            Some(null_mut()),
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) => {
            return SwitcherWindowRenderResult::RenderFailed {
                message: format!("CreateWindowExW failed: {error:?}"),
            };
        }
    };

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    let started = std::time::Instant::now();
    let hold = Duration::from_millis(request.hold_millis);
    while started.elapsed() < hold {
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        thread::sleep(Duration::from_millis(16));
    }
    unsafe {
        let _ = DestroyWindow(hwnd);
        PAINT_FRAME = None;
    }

    SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
        width: request.frame.width,
        height: request.frame.height,
        title: request.title,
        hold_millis: request.hold_millis,
    })
}

/// Input for manual queue-to-switcher placeholder verification.
///
/// The queue state is caller-owned and borrowed read-only. This is intentionally
/// not a cross-process bridge to a running server queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherPlaceholderManualVerificationInput<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub client_id: &'a ClientId,
}

/// Compact summary for manual placeholder verification output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherPlaceholderManualVerificationSummary {
    pub selected_client_id: ClientId,
    pub frame_id: Option<u64>,
    pub encoded_payload_len: Option<usize>,
    pub decode_status: Option<SwitcherSingleViewDecodeStatus>,
    pub no_frame: bool,
}

/// Result of the manual queue-to-switcher placeholder helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherPlaceholderManualVerificationResult {
    PlaceholderReady {
        summary: SwitcherPlaceholderManualVerificationSummary,
        handoff: SwitcherSingleViewDisplayPlaceholderHandoff,
    },
    NoFrame {
        summary: SwitcherPlaceholderManualVerificationSummary,
    },
}

/// Input for manual decode-and-dump verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherDecodeLatestFrameOnceInput<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub client_id: &'a ClientId,
    pub output_path: PathBuf,
}

/// Compact summary for manual decode-and-dump verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherDecodeLatestFrameOnceSummary {
    pub selected_client_id: ClientId,
    pub frame_id: Option<u64>,
    pub encoded_payload_len: Option<usize>,
    pub decode_status: Option<SwitcherSingleViewDecodeStatus>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub output_path: Option<PathBuf>,
    pub output_bytes: Option<usize>,
    pub no_frame: bool,
}

/// Result of one latest-frame decode and file dump attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherDecodeLatestFrameOnceResult {
    Decoded {
        summary: SwitcherDecodeLatestFrameOnceSummary,
        handoff: SwitcherSingleViewDisplayRealFrameHandoff,
        dump: SwitcherDecodedFrameDump,
    },
    PlaceholderFallback {
        summary: SwitcherDecodeLatestFrameOnceSummary,
        handoff: SwitcherSingleViewDisplayPlaceholderHandoff,
    },
    NoFrame {
        summary: SwitcherDecodeLatestFrameOnceSummary,
    },
    DumpFailed {
        summary: SwitcherDecodeLatestFrameOnceSummary,
        handoff: SwitcherSingleViewDisplayRealFrameHandoff,
        error: SwitcherDecodedFrameDumpError,
    },
}

/// Manual single-frame decode/display substitute.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherDecodeLatestFrameOnceBoundary {
    path: SwitcherSingleViewPlaceholderPathBoundary,
    decoder: SwitcherH264DecodeBoundary,
    dump: SwitcherDecodedFrameDumpBoundary,
}

impl SwitcherDecodeLatestFrameOnceBoundary {
    pub fn decode_latest_with_runtime(
        &self,
        input: SwitcherDecodeLatestFrameOnceInput<'_>,
        runtime: &impl SwitcherH264DecodeRuntimeHook,
    ) -> SwitcherDecodeLatestFrameOnceResult {
        match self.path.prepare_latest_display_handoff_with_decode(
            SwitcherSingleViewFrameSelectionInput {
                queue_state: input.queue_state,
                client_id: input.client_id,
            },
            &self.decoder,
            runtime,
        ) {
            SwitcherSingleViewDisplayHandoffResult::DisplayReadyRealFrame(handoff) => {
                let summary = SwitcherDecodeLatestFrameOnceSummary {
                    selected_client_id: handoff.selected.client_id.clone(),
                    frame_id: Some(handoff.selected.frame_id),
                    encoded_payload_len: Some(handoff.selected.encoded_payload_len),
                    decode_status: Some(handoff.decode_status),
                    width: Some(handoff.decoded.width),
                    height: Some(handoff.decoded.height),
                    output_path: Some(input.output_path.clone()),
                    output_bytes: None,
                    no_frame: false,
                };
                match self.dump.write_bmp(&handoff.decoded, &input.output_path) {
                    Ok(dump) => SwitcherDecodeLatestFrameOnceResult::Decoded {
                        summary: SwitcherDecodeLatestFrameOnceSummary {
                            output_bytes: Some(dump.bytes_written),
                            ..summary
                        },
                        handoff,
                        dump,
                    },
                    Err(error) => SwitcherDecodeLatestFrameOnceResult::DumpFailed {
                        summary,
                        handoff,
                        error,
                    },
                }
            }
            SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(handoff) => {
                SwitcherDecodeLatestFrameOnceResult::PlaceholderFallback {
                    summary: SwitcherDecodeLatestFrameOnceSummary {
                        selected_client_id: handoff.selected.client_id.clone(),
                        frame_id: Some(handoff.selected.frame_id),
                        encoded_payload_len: Some(handoff.selected.encoded_payload_len),
                        decode_status: Some(handoff.decode_status),
                        width: None,
                        height: None,
                        output_path: None,
                        output_bytes: None,
                        no_frame: false,
                    },
                    handoff,
                }
            }
            SwitcherSingleViewDisplayHandoffResult::NoFrameAvailable { client_id } => {
                SwitcherDecodeLatestFrameOnceResult::NoFrame {
                    summary: SwitcherDecodeLatestFrameOnceSummary {
                        selected_client_id: client_id,
                        frame_id: None,
                        encoded_payload_len: None,
                        decode_status: None,
                        width: None,
                        height: None,
                        output_path: None,
                        output_bytes: None,
                        no_frame: true,
                    },
                }
            }
        }
    }
}

/// Runtime helper for the manual placeholder PoC.
///
/// This composes the existing latest-frame selection and placeholder display
/// handoff boundaries, then surfaces a CLI/test-friendly summary. It does not
/// mutate queue state, decode H.264, render a window, share state with a server
/// process, run sync scheduling, or touch OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherPlaceholderManualVerificationBoundary {
    path: SwitcherSingleViewPlaceholderPathBoundary,
}

impl SwitcherPlaceholderManualVerificationBoundary {
    pub fn verify_latest_placeholder(
        &self,
        input: SwitcherPlaceholderManualVerificationInput<'_>,
    ) -> SwitcherPlaceholderManualVerificationResult {
        match self
            .path
            .prepare_latest_display_handoff(SwitcherSingleViewFrameSelectionInput {
                queue_state: input.queue_state,
                client_id: input.client_id,
            }) {
            SwitcherSingleViewDisplayHandoffResult::DisplayReadyRealFrame(handoff) => {
                let summary = SwitcherPlaceholderManualVerificationSummary {
                    selected_client_id: handoff.selected.client_id.clone(),
                    frame_id: Some(handoff.selected.frame_id),
                    encoded_payload_len: Some(handoff.selected.encoded_payload_len),
                    decode_status: Some(handoff.decode_status),
                    no_frame: false,
                };
                SwitcherPlaceholderManualVerificationResult::PlaceholderReady {
                    summary,
                    handoff: SwitcherSingleViewDisplayPlaceholderHandoff {
                        selected: handoff.selected,
                        decode_status: SwitcherSingleViewDecodeStatus::DeferredPlaceholder,
                    },
                }
            }
            SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(handoff) => {
                let summary = SwitcherPlaceholderManualVerificationSummary {
                    selected_client_id: handoff.selected.client_id.clone(),
                    frame_id: Some(handoff.selected.frame_id),
                    encoded_payload_len: Some(handoff.selected.encoded_payload_len),
                    decode_status: Some(handoff.decode_status),
                    no_frame: false,
                };
                SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff }
            }
            SwitcherSingleViewDisplayHandoffResult::NoFrameAvailable { client_id } => {
                SwitcherPlaceholderManualVerificationResult::NoFrame {
                    summary: SwitcherPlaceholderManualVerificationSummary {
                        selected_client_id: client_id,
                        frame_id: None,
                        encoded_payload_len: None,
                        decode_status: None,
                        no_frame: true,
                    },
                }
            }
        }
    }
}

/// Minimal server-to-switcher bridge video observation.
///
/// This is a compact view of the server manual receive path. It does not share
/// queue state across processes or reinterpret packet acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherAuthVideoPlaceholderBridgeVideoStatus {
    NotReceivedAuthRejected,
    Received,
    NotReceivedControllerStopped,
}

/// Input for the in-process auth/video queue to switcher placeholder bridge.
///
/// The queue state remains caller-owned and borrowed read-only. The optional
/// queue result is the server queue runtime result for the packet that produced
/// the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherAuthVideoPlaceholderBridgeInput<'a> {
    pub auth_accepted: bool,
    pub video_status: SwitcherAuthVideoPlaceholderBridgeVideoStatus,
    pub queue_result: Option<&'a ServerVideoFrameQueueRuntimeResult>,
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub client_id: &'a ClientId,
}

/// Compact stdout/test summary for the in-process bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherAuthVideoPlaceholderBridgeSummary {
    pub auth_accepted: bool,
    pub video_received: bool,
    pub video_accepted: bool,
    pub video_rejected: bool,
    pub queued: bool,
    pub dropped_oldest: bool,
    pub queue_len: usize,
    pub selected_client_id: ClientId,
    pub selected_frame_id: Option<u64>,
    pub payload_len: Option<usize>,
    pub decode_status: Option<SwitcherSingleViewDecodeStatus>,
    pub no_frame: bool,
}

/// Result of the switcher-owned in-process bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherAuthVideoPlaceholderBridgeResult {
    PlaceholderReady {
        summary: SwitcherAuthVideoPlaceholderBridgeSummary,
        handoff: SwitcherSingleViewDisplayPlaceholderHandoff,
    },
    NoFrame {
        summary: SwitcherAuthVideoPlaceholderBridgeSummary,
    },
}

/// Switcher-owned in-process bridge for manual placeholder PoC verification.
///
/// This composes an already-run server auth/video queue outcome with the
/// existing switcher placeholder helper. It does not run a cross-process queue
/// bridge, decode H.264, render UI, sync views, or touch OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherAuthVideoPlaceholderBridgeBoundary {
    verification: SwitcherPlaceholderManualVerificationBoundary,
}

impl SwitcherAuthVideoPlaceholderBridgeBoundary {
    pub fn verify(
        &self,
        input: SwitcherAuthVideoPlaceholderBridgeInput<'_>,
    ) -> SwitcherAuthVideoPlaceholderBridgeResult {
        let placeholder = self.verification.verify_latest_placeholder(
            SwitcherPlaceholderManualVerificationInput {
                queue_state: input.queue_state,
                client_id: input.client_id,
            },
        );

        match placeholder {
            SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } => {
                SwitcherAuthVideoPlaceholderBridgeResult::PlaceholderReady {
                    summary: self.summary_from(input, summary),
                    handoff,
                }
            }
            SwitcherPlaceholderManualVerificationResult::NoFrame { summary } => {
                SwitcherAuthVideoPlaceholderBridgeResult::NoFrame {
                    summary: self.summary_from(input, summary),
                }
            }
        }
    }

    pub fn verify_server_outcome(
        &self,
        outcome: &ServerReceiveAuthVideoQueueOnceStartupOutcome,
        client_id: &ClientId,
    ) -> SwitcherAuthVideoPlaceholderBridgeResult {
        let (video_status, queue_result) = match &outcome.video {
            ServerReceiveAuthVideoQueueOnceVideoOutcome::NotReceivedAuthRejected => (
                SwitcherAuthVideoPlaceholderBridgeVideoStatus::NotReceivedAuthRejected,
                None,
            ),
            ServerReceiveAuthVideoQueueOnceVideoOutcome::Received { queue, .. } => (
                if queue.is_some() {
                    SwitcherAuthVideoPlaceholderBridgeVideoStatus::Received
                } else {
                    SwitcherAuthVideoPlaceholderBridgeVideoStatus::NotReceivedControllerStopped
                },
                queue.as_ref(),
            ),
        };

        self.verify(SwitcherAuthVideoPlaceholderBridgeInput {
            auth_accepted: outcome.first_auth.auth_flow.decision.accepted,
            video_status,
            queue_result,
            queue_state: &outcome.video_queue_state,
            client_id,
        })
    }

    fn summary_from(
        &self,
        input: SwitcherAuthVideoPlaceholderBridgeInput<'_>,
        placeholder: SwitcherPlaceholderManualVerificationSummary,
    ) -> SwitcherAuthVideoPlaceholderBridgeSummary {
        let video_received =
            input.video_status == SwitcherAuthVideoPlaceholderBridgeVideoStatus::Received;
        let (video_accepted, video_rejected, queued, dropped_oldest) =
            queue_result_summary(input.queue_result);

        SwitcherAuthVideoPlaceholderBridgeSummary {
            auth_accepted: input.auth_accepted,
            video_received,
            video_accepted,
            video_rejected,
            queued,
            dropped_oldest,
            queue_len: input.queue_state.total_len(),
            selected_client_id: placeholder.selected_client_id,
            selected_frame_id: placeholder.frame_id,
            payload_len: placeholder.encoded_payload_len,
            decode_status: placeholder.decode_status,
            no_frame: placeholder.no_frame,
        }
    }
}

fn queue_result_summary(
    queue_result: Option<&ServerVideoFrameQueueRuntimeResult>,
) -> (bool, bool, bool, bool) {
    match queue_result {
        Some(ServerVideoFrameQueueRuntimeResult::Queued(
            ServerVideoFrameQueueStorageResult::Stored { dropped_oldest, .. },
        )) => (true, false, true, dropped_oldest.is_some()),
        Some(ServerVideoFrameQueueRuntimeResult::Queued(
            ServerVideoFrameQueueStorageResult::Dropped { .. },
        )) => (true, false, false, false),
        Some(ServerVideoFrameQueueRuntimeResult::NotQueued { .. }) => (false, true, false, false),
        None => (false, false, false, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stream_sync_protocol::{
        AppVersion, AuthRequest, Codec, EncodeContext, MessageEncoder, MessageType,
        ProtocolMessage, ProtocolMessageEncoderBoundary, ProtocolVersion, RunId, VideoFrame,
    };
    use stream_sync_server::{
        AuthenticatedSenderEntry, AuthenticatedSenderRegistration,
        AuthenticatedSenderRegistryBoundary, ServerDispatchRuntimeSideEffectApplyResult,
        ServerHandlerDispatchOutcome, ServerHandlerDispatchResult,
        ServerRegisteredVideoFramePacket, ServerVideoFrameHandlerBoundary,
        ServerVideoFrameQueuePolicy, ServerVideoFrameQueueRuntimeSkipReason,
        ServerVideoFrameQueueStorageBoundary,
    };

    #[test]
    fn single_view_latest_selection_returns_newest_frame_for_client() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 1, TimestampMicros(2_000_000));
        store_frame(&mut state, "client-1", 2, TimestampMicros(2_000_100));
        store_frame(&mut state, "client-2", 9, TimestampMicros(2_000_200));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleViewLatestFrameSelectionBoundary.select_latest(
            SwitcherSingleViewFrameSelectionInput {
                queue_state: &state,
                client_id: &client_id,
            },
        );

        let SwitcherSingleViewFrameSelectionResult::FrameAvailable(selected) = result else {
            panic!("latest frame should be available");
        };
        assert_eq!(selected.client_id, client_id);
        assert_eq!(selected.frame_id, 2);
        assert_eq!(selected.queued_at, TimestampMicros(2_000_100));
        assert_eq!(selected.encoded_payload_len, 3);
        assert_eq!(selected.encoded_payload, vec![0x02, 0xbb, 0xcc]);
    }

    #[test]
    fn single_view_latest_selection_no_frame_path_is_explicit() {
        let state = ServerVideoFrameQueueState::default();
        let client_id = ClientId("missing-client".to_string());

        let result = SwitcherSingleViewLatestFrameSelectionBoundary.select_latest(
            SwitcherSingleViewFrameSelectionInput {
                queue_state: &state,
                client_id: &client_id,
            },
        );

        assert_eq!(
            result,
            SwitcherSingleViewFrameSelectionResult::NoFrameAvailable { client_id }
        );
    }

    #[test]
    fn single_client_queue_source_preview_latest_uses_run_scope_without_mutation() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_010_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-2",
            2,
            TimestampMicros(2_010_100),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_010_200),
        );
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let result = SwitcherSingleClientQueueSourceBoundary::default().read(
            &mut state,
            SwitcherSingleClientQueueSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            },
        );

        let SwitcherSingleClientQueueSourceResult::FrameAvailable {
            frame,
            mode,
            remaining_client_queue_len,
        } = result
        else {
            panic!("latest frame for run should be available");
        };
        assert_eq!(frame.client_id, client_id);
        assert_eq!(frame.frame_id, 3);
        assert_eq!(mode, SwitcherSingleClientQueueSourceMode::PreviewLatest);
        assert_eq!(remaining_client_queue_len, before_len);
        assert_eq!(state.client_queue_len(&client_id), before_len);
    }

    #[test]
    fn single_client_queue_source_consume_oldest_dequeues_one_run_scoped_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_020_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-2",
            2,
            TimestampMicros(2_020_100),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_020_200),
        );
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleClientQueueSourceBoundary::default().read(
            &mut state,
            SwitcherSingleClientQueueSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
            },
        );

        let SwitcherSingleClientQueueSourceResult::FrameAvailable {
            frame,
            mode,
            remaining_client_queue_len,
        } = result
        else {
            panic!("oldest run frame should be consumed");
        };
        assert_eq!(frame.frame_id, 1);
        assert_eq!(mode, SwitcherSingleClientQueueSourceMode::ConsumeOldest);
        assert_eq!(remaining_client_queue_len, 2);
        let remaining: Vec<(String, u64)> = state
            .frames_for_client(&client_id)
            .map(|queued| (queued.frame.run_id.0.clone(), queued.frame.frame_id))
            .collect();
        assert_eq!(
            remaining,
            vec![("run-2".to_string(), 2), ("run-1".to_string(), 3)]
        );
    }

    #[test]
    fn single_client_queue_source_missing_run_reports_no_frame_without_mutation() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_030_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleClientQueueSourceBoundary::default().read(
            &mut state,
            SwitcherSingleClientQueueSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-missing".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
            },
        );

        assert_eq!(
            result,
            SwitcherSingleClientQueueSourceResult::NoFrameAvailable {
                client_id,
                run_id: RunId("run-missing".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn single_client_queue_source_trait_adapter_returns_selected_queued_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_031_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-2",
            2,
            TimestampMicros(2_031_100),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut source = SwitcherInProcessServerQueueFrameSource::new(&mut state);
            source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
            })
        };

        let SwitcherSingleClientQueueSourceResult::FrameAvailable { frame, mode, .. } = result
        else {
            panic!("queued frame should be available through trait adapter");
        };
        assert_eq!(frame.client_id, client_id);
        assert_eq!(frame.run_id, RunId("run-1".to_string()));
        assert_eq!(frame.frame_id, 1);
        assert_eq!(mode, SwitcherSingleClientQueueSourceMode::PreviewOldest);
    }

    #[test]
    fn single_client_queue_source_trait_adapter_returns_no_frame_for_missing_run() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_032_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut source = SwitcherInProcessServerQueueFrameSource::new(&mut state);
            source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-missing".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            })
        };

        assert_eq!(
            result,
            SwitcherSingleClientQueueSourceResult::NoFrameAvailable {
                client_id,
                run_id: RunId("run-missing".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn single_client_queue_source_trait_adapter_preview_does_not_mutate_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_033_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            2,
            TimestampMicros(2_033_100),
        );
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let result = {
            let mut source = SwitcherInProcessServerQueueFrameSource::new(&mut state);
            source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            })
        };

        let SwitcherSingleClientQueueSourceResult::FrameAvailable { frame, .. } = result else {
            panic!("latest frame should be inspectable through trait adapter");
        };
        assert_eq!(frame.frame_id, 2);
        assert_eq!(state.client_queue_len(&client_id), before_len);
        let remaining: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(remaining, vec![1, 2]);
    }

    #[test]
    fn single_client_queue_source_trait_adapter_consume_mutates_only_requested_run() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_034_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-2",
            2,
            TimestampMicros(2_034_100),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_034_200),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut source = SwitcherInProcessServerQueueFrameSource::new(&mut state);
            source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
            })
        };

        let SwitcherSingleClientQueueSourceResult::FrameAvailable {
            frame,
            remaining_client_queue_len,
            ..
        } = result
        else {
            panic!("oldest requested run frame should be dequeued through trait adapter");
        };
        assert_eq!(frame.frame_id, 1);
        assert_eq!(remaining_client_queue_len, 2);
        let remaining: Vec<(String, u64)> = state
            .frames_for_client(&client_id)
            .map(|queued| (queued.frame.run_id.0.clone(), queued.frame.frame_id))
            .collect();
        assert_eq!(
            remaining,
            vec![("run-2".to_string(), 2), ("run-1".to_string(), 3)]
        );
    }

    #[test]
    fn single_client_queue_source_trait_adapter_preserves_frame_metadata() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-1",
            "run-meta",
            1,
            TimestampMicros(2_035_000),
            640,
            360,
            vec![0x01, 0x02, 0x03, 0x04],
        );

        let result = {
            let mut source = SwitcherInProcessServerQueueFrameSource::new(&mut state);
            source.read_queued_frame(SwitcherSingleClientQueueSourceInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-meta".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
            })
        };

        let SwitcherSingleClientQueueSourceResult::FrameAvailable { frame, .. } = result else {
            panic!("metadata frame should be available through trait adapter");
        };
        assert_eq!(frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(frame.run_id, RunId("run-meta".to_string()));
        assert_eq!(frame.frame_id, 1);
        assert_eq!(frame.capture_timestamp, TimestampMicros(1_000_001));
        assert_eq!(frame.send_timestamp, TimestampMicros(1_000_101));
        assert_eq!(frame.queued_at, TimestampMicros(2_035_000));
        assert!(frame.is_keyframe);
        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 360);
        assert_eq!(frame.fps_nominal, 30);
        assert_eq!(frame.codec, Codec::H264);
        assert_eq!(frame.encoded_payload_len, 4);
        assert_eq!(frame.encoded_payload, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn single_client_queue_source_handoff_returns_selected_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_036_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            handoff.read_handoff_frame(SwitcherQueuedFrameHandoffInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
            })
        };

        let SwitcherQueuedFrameHandoffResult::FrameRead { frame, mode, .. } = result else {
            panic!("handoff should read selected queued frame");
        };
        assert_eq!(frame.client_id, client_id);
        assert_eq!(frame.run_id, RunId("run-1".to_string()));
        assert_eq!(frame.frame_id, 1);
        assert_eq!(mode, SwitcherSingleClientQueueSourceMode::PreviewOldest);
    }

    #[test]
    fn single_client_queue_source_handoff_returns_no_frame_result() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_037_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            handoff.read_handoff_frame(SwitcherQueuedFrameHandoffInput {
                client_id: client_id.clone(),
                run_id: RunId("run-missing".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            })
        };

        assert_eq!(
            result,
            SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                client_id,
                run_id: RunId("run-missing".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn single_client_queue_source_handoff_invalid_scope_reports_error_without_mutation() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_038_000),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            handoff.read_handoff_frame(SwitcherQueuedFrameHandoffInput {
                client_id: ClientId("".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            })
        };

        assert_eq!(
            result,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: ClientId("".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                error: SwitcherQueuedFrameHandoffError::InvalidScope {
                    client_id: ClientId("".to_string()),
                    run_id: RunId("run-1".to_string()),
                },
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn server_switcher_handoff_client_adapter_builds_request_from_handoff_input() {
        let adapter = SwitcherServerQueuedFrameHandoffClientAdapterBoundary;
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        };

        let request = adapter.build_request(123, &input);

        assert_eq!(request.handoff_version, SERVER_SWITCHER_HANDOFF_VERSION);
        assert_eq!(request.request_id, 123);
        assert_eq!(request.client_id, ClientId("client-1".to_string()));
        assert_eq!(request.run_id, RunId("run-1".to_string()));
        assert_eq!(
            request.read_mode,
            ServerSwitcherQueuedFrameReadMode::DequeueOldest
        );
    }

    #[test]
    fn server_switcher_handoff_client_adapter_maps_frame_read_response_to_handoff_result() {
        let adapter = SwitcherServerQueuedFrameHandoffClientAdapterBoundary;
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-request".to_string()),
            run_id: RunId("run-request".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
        };

        let result = adapter.map_response(
            &input,
            ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
                request_id: 700,
                remaining_client_queue_len: 2,
                frame: ServerSwitcherQueuedFrameHandoffFrame {
                    client_id: ClientId("client-actual".to_string()),
                    run_id: RunId("run-actual".to_string()),
                    frame_id: 41,
                    capture_timestamp: TimestampMicros(5_000_041),
                    send_timestamp: TimestampMicros(5_000_141),
                    queued_at: TimestampMicros(6_000_041),
                    width: 1280,
                    height: 720,
                    fps_nominal: 30,
                    is_keyframe: true,
                    codec: Codec::H264,
                    encoded_payload_len: 4,
                    encoded_payload: vec![0xaa, 0xbb, 0xcc, 0xdd],
                },
            },
        );

        let SwitcherQueuedFrameHandoffResult::FrameRead {
            frame,
            mode,
            remaining_client_queue_len,
        } = result
        else {
            panic!("FrameRead response should map to handoff frame result");
        };
        assert_eq!(mode, SwitcherSingleClientQueueSourceMode::PreviewLatest);
        assert_eq!(remaining_client_queue_len, 2);
        assert_eq!(frame.client_id, ClientId("client-actual".to_string()));
        assert_eq!(frame.run_id, RunId("run-actual".to_string()));
        assert_eq!(frame.frame_id, 41);
        assert_eq!(frame.capture_timestamp, TimestampMicros(5_000_041));
        assert_eq!(frame.send_timestamp, TimestampMicros(5_000_141));
        assert_eq!(frame.queued_at, TimestampMicros(6_000_041));
        assert_eq!(frame.width, 1280);
        assert_eq!(frame.height, 720);
        assert_eq!(frame.fps_nominal, 30);
        assert_eq!(frame.codec, Codec::H264);
        assert_eq!(frame.encoded_payload_len, 4);
        assert_eq!(frame.encoded_payload, vec![0xaa, 0xbb, 0xcc, 0xdd]);
    }

    #[test]
    fn server_switcher_handoff_client_adapter_maps_no_frame_to_no_frame_available() {
        let adapter = SwitcherServerQueuedFrameHandoffClientAdapterBoundary;
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
        };

        let result = adapter.map_response(
            &input,
            ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
                request_id: 701,
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                read_mode: ServerSwitcherQueuedFrameReadMode::InspectOldest,
                client_queue_len: 0,
            },
        );

        assert_eq!(
            result,
            SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
                client_queue_len: 0,
            }
        );
    }

    #[test]
    fn server_switcher_handoff_client_adapter_maps_all_handoff_error_codes() {
        let adapter = SwitcherServerQueuedFrameHandoffClientAdapterBoundary;
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        };
        let cases = vec![
            (
                ServerSwitcherQueuedFrameHandoffErrorCode::SourceUnavailable,
                SwitcherQueuedFrameHandoffError::SourceUnavailable,
            ),
            (
                ServerSwitcherQueuedFrameHandoffErrorCode::RequestTimeout,
                SwitcherQueuedFrameHandoffError::Timeout,
            ),
            (
                ServerSwitcherQueuedFrameHandoffErrorCode::InvalidScope,
                SwitcherQueuedFrameHandoffError::InvalidScope {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                },
            ),
            (
                ServerSwitcherQueuedFrameHandoffErrorCode::UnsupportedReadMode,
                SwitcherQueuedFrameHandoffError::UnsupportedMode {
                    mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                },
            ),
            (
                ServerSwitcherQueuedFrameHandoffErrorCode::MalformedResponse,
                SwitcherQueuedFrameHandoffError::MalformedResponse,
            ),
            (
                ServerSwitcherQueuedFrameHandoffErrorCode::SourceShutdown,
                SwitcherQueuedFrameHandoffError::SourceShutdown,
            ),
        ];

        for (code, expected_error) in cases {
            let result = adapter.map_response(
                &input,
                ServerSwitcherQueuedFrameHandoffResponse::HandoffError {
                    request_id: 702,
                    error: code,
                },
            );

            assert_eq!(
                result,
                SwitcherQueuedFrameHandoffResult::HandoffError {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                    error: expected_error,
                }
            );
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ScriptedNamedPipeRuntimeCall {
        pipe_name: String,
        request_id: u64,
        input: SwitcherQueuedFrameHandoffInput,
        config: SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    }

    struct ScriptedNamedPipeRuntime {
        calls: Vec<ScriptedNamedPipeRuntimeCall>,
        results: Vec<
            Result<
                SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput,
                SwitcherNamedPipeQueuedFrameHandoffRuntimeError,
            >,
        >,
    }

    impl ScriptedNamedPipeRuntime {
        fn new(
            results: Vec<
                Result<
                    SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput,
                    SwitcherNamedPipeQueuedFrameHandoffRuntimeError,
                >,
            >,
        ) -> Self {
            Self {
                calls: Vec::new(),
                results,
            }
        }
    }

    struct ScriptedNamedPipeClock {
        now_millis_values: Vec<u64>,
    }

    impl ScriptedNamedPipeClock {
        fn new(now_millis_values: Vec<u64>) -> Self {
            Self { now_millis_values }
        }
    }

    impl SwitcherNamedPipeQueuedFrameHandoffClock for ScriptedNamedPipeClock {
        fn now_millis(&mut self) -> u64 {
            self.now_millis_values.remove(0)
        }
    }

    impl SwitcherNamedPipeQueuedFrameHandoffRuntime for ScriptedNamedPipeRuntime {
        fn run_once_with_config(
            &mut self,
            pipe_name: &str,
            request_id: u64,
            input: SwitcherQueuedFrameHandoffInput,
            config: SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
        ) -> Result<
            SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput,
            SwitcherNamedPipeQueuedFrameHandoffRuntimeError,
        > {
            self.calls.push(ScriptedNamedPipeRuntimeCall {
                pipe_name: pipe_name.to_string(),
                request_id,
                input,
                config,
            });
            self.results.remove(0)
        }
    }

    #[test]
    fn named_pipe_handoff_wrapper_sends_request_through_runtime_and_preserves_supplied_request_id()
    {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
        };
        let scripted_result = SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            client_queue_len: 0,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime(
            "test-pipe",
            50,
            ScriptedNamedPipeRuntime::new(vec![Ok(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 999,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::InspectLatest,
                    },
                    response: None,
                    result: scripted_result.clone(),
                },
            )]),
        );

        let result = handoff.read_handoff_frame_with_request_id(999, input.clone());

        assert_eq!(result, scripted_result);
        assert_eq!(handoff.next_request_id(), 50);
        assert_eq!(
            handoff.runtime.calls,
            vec![ScriptedNamedPipeRuntimeCall {
                pipe_name: "test-pipe".to_string(),
                request_id: 999,
                input,
                config: SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
            }]
        );
    }

    #[test]
    fn named_pipe_handoff_wrapper_generates_monotonic_request_ids() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime(
            "generated-pipe",
            10,
            ScriptedNamedPipeRuntime::new(vec![
                Ok(SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 10,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::DequeueOldest,
                    },
                    response: None,
                    result: SwitcherQueuedFrameHandoffResult::HandoffError {
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        mode: input.mode,
                        error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                    },
                }),
                Ok(SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 11,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::DequeueOldest,
                    },
                    response: None,
                    result: SwitcherQueuedFrameHandoffResult::HandoffError {
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        mode: input.mode,
                        error: SwitcherQueuedFrameHandoffError::Timeout,
                    },
                }),
            ]),
        );

        let first = handoff.read_handoff_frame(input.clone());
        let second = handoff.read_handoff_frame(input.clone());

        assert!(matches!(
            first,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                ..
            }
        ));
        assert!(matches!(
            second,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                error: SwitcherQueuedFrameHandoffError::Timeout,
                ..
            }
        ));
        assert_eq!(handoff.next_request_id(), 12);
        assert_eq!(handoff.runtime.calls[0].request_id, 10);
        assert_eq!(handoff.runtime.calls[1].request_id, 11);
    }

    #[test]
    fn named_pipe_handoff_wrapper_preserves_frame_read_result() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
        };
        let expected = SwitcherQueuedFrameHandoffResult::FrameRead {
            frame: SwitcherSingleViewSelectedEncodedFrame {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                frame_id: 55,
                capture_timestamp: TimestampMicros(1_000_055),
                send_timestamp: TimestampMicros(1_000_155),
                queued_at: TimestampMicros(2_000_055),
                is_keyframe: true,
                width: 1280,
                height: 720,
                fps_nominal: 30,
                codec: Codec::H264,
                encoded_payload_len: 4,
                encoded_payload: vec![0xaa, 0xbb, 0xcc, 0xdd],
            },
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            remaining_client_queue_len: 3,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime(
            "frame-pipe",
            1,
            ScriptedNamedPipeRuntime::new(vec![Ok(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 1,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::InspectLatest,
                    },
                    response: None,
                    result: expected.clone(),
                },
            )]),
        );

        let result = handoff.read_handoff_frame(input);

        assert_eq!(result, expected);
    }

    #[test]
    fn named_pipe_handoff_wrapper_preserves_no_frame_result() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-missing".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
        };
        let expected = SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-missing".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
            client_queue_len: 4,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime(
            "no-frame-pipe",
            8,
            ScriptedNamedPipeRuntime::new(vec![Ok(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 8,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::InspectOldest,
                    },
                    response: None,
                    result: expected.clone(),
                },
            )]),
        );

        let result = handoff.read_handoff_frame(input);

        assert_eq!(result, expected);
    }

    #[test]
    fn named_pipe_handoff_wrapper_preserves_explicit_handoff_error_result() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        };
        let expected = SwitcherQueuedFrameHandoffResult::HandoffError {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
            error: SwitcherQueuedFrameHandoffError::SourceShutdown,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime(
            "error-pipe",
            20,
            ScriptedNamedPipeRuntime::new(vec![Ok(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 20,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::DequeueOldest,
                    },
                    response: None,
                    result: expected.clone(),
                },
            )]),
        );

        let result = handoff.read_handoff_frame(input);

        assert_eq!(result, expected);
        assert!(!matches!(
            result,
            SwitcherQueuedFrameHandoffResult::NoFrameAvailable { .. }
        ));
    }

    #[test]
    fn named_pipe_handoff_wrapper_maps_runtime_encode_error_to_explicit_handoff_error() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime(
            "encode-error-pipe",
            100,
            ScriptedNamedPipeRuntime::new(vec![Err(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeError::EncodeRequest(
                    stream_sync_net_core::ServerSwitcherQueuedFrameHandoffCodecError::BodyTooLong {
                        len: usize::MAX,
                    },
                ),
            )]),
        );

        let result = handoff.read_handoff_frame(input);

        assert_eq!(
            result,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                error: SwitcherQueuedFrameHandoffError::MalformedResponse,
            }
        );
    }

    #[test]
    fn named_pipe_handoff_wrapper_request_output_preserves_frame_read_summary_fields() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-frame".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
        };
        let config = SwitcherNamedPipeQueuedFrameHandoffRequestConfig {
            connect_timeout_millis: 321,
        };
        let frame_result = SwitcherQueuedFrameHandoffResult::FrameRead {
            frame: SwitcherSingleViewSelectedEncodedFrame {
                client_id: input.client_id.clone(),
                run_id: input.run_id.clone(),
                frame_id: 44,
                capture_timestamp: TimestampMicros(10),
                send_timestamp: TimestampMicros(11),
                queued_at: TimestampMicros(12),
                is_keyframe: false,
                width: 640,
                height: 360,
                fps_nominal: 30,
                codec: Codec::H264,
                encoded_payload_len: 3,
                encoded_payload: vec![1, 2, 3],
            },
            mode: input.mode,
            remaining_client_queue_len: 2,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime_with_clock(
            "summary-pipe",
            7,
            ScriptedNamedPipeRuntime::new(vec![Ok(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 55,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::InspectLatest,
                    },
                    response: Some(ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
                        request_id: 55,
                        frame: ServerSwitcherQueuedFrameHandoffFrame {
                            client_id: input.client_id.clone(),
                            run_id: input.run_id.clone(),
                            frame_id: 44,
                            capture_timestamp: TimestampMicros(10),
                            send_timestamp: TimestampMicros(11),
                            queued_at: TimestampMicros(12),
                            is_keyframe: false,
                            width: 640,
                            height: 360,
                            fps_nominal: 30,
                            codec: Codec::H264,
                            encoded_payload_len: 3,
                            encoded_payload: vec![1, 2, 3],
                        },
                        remaining_client_queue_len: 2,
                    }),
                    result: frame_result.clone(),
                },
            )]),
            ScriptedNamedPipeClock::new(vec![100, 108]),
        );

        let output =
            handoff.read_handoff_frame_with_request_id_and_config(55, input.clone(), config);

        assert_eq!(output.result, frame_result);
        assert_eq!(output.summary.pipe_name, "summary-pipe");
        assert_eq!(output.summary.request_id, 55);
        assert_eq!(output.summary.read_mode, input.mode);
        assert_eq!(output.summary.timeout_millis, 321);
        assert_eq!(
            output.summary.request_status,
            SwitcherNamedPipeQueuedFrameHandoffRequestStatus::Sent
        );
        assert_eq!(
            output.summary.response_status,
            SwitcherNamedPipeQueuedFrameHandoffResponseStatus::Decoded
        );
        assert_eq!(
            output.summary.result_kind,
            SwitcherNamedPipeQueuedFrameHandoffResultKind::FrameRead
        );
        assert_eq!(output.summary.elapsed_millis, 8);
        assert!(output.runtime.is_some());
        assert_eq!(handoff.runtime.calls[0].config, config);
    }

    #[test]
    fn named_pipe_handoff_wrapper_request_output_preserves_no_frame_summary_fields() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-empty".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
        };
        let expected = SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
            client_id: input.client_id.clone(),
            run_id: input.run_id.clone(),
            mode: input.mode,
            client_queue_len: 0,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime_with_clock(
            "no-frame-summary-pipe",
            3,
            ScriptedNamedPipeRuntime::new(vec![Ok(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 3,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::InspectOldest,
                    },
                    response: Some(ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
                        request_id: 3,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::InspectOldest,
                        client_queue_len: 0,
                    }),
                    result: expected.clone(),
                },
            )]),
            ScriptedNamedPipeClock::new(vec![25, 25]),
        );

        let output = handoff.read_handoff_frame_with_config(
            input,
            SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
        );

        assert_eq!(output.result, expected);
        assert_eq!(output.summary.request_id, 3);
        assert_eq!(
            output.summary.result_kind,
            SwitcherNamedPipeQueuedFrameHandoffResultKind::NoFrameAvailable
        );
        assert_eq!(output.summary.elapsed_millis, 0);
        assert!(!matches!(
            output.result,
            SwitcherQueuedFrameHandoffResult::HandoffError { .. }
        ));
    }

    #[test]
    fn named_pipe_handoff_wrapper_request_output_preserves_timeout_as_explicit_error() {
        assert_named_pipe_request_output_error(
            SwitcherQueuedFrameHandoffError::Timeout,
            SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodedOnly,
            SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None,
        );
    }

    #[test]
    fn named_pipe_handoff_wrapper_request_output_preserves_source_unavailable_as_explicit_error() {
        assert_named_pipe_request_output_error(
            SwitcherQueuedFrameHandoffError::SourceUnavailable,
            SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodedOnly,
            SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None,
        );
    }

    #[test]
    fn named_pipe_handoff_wrapper_request_output_preserves_source_shutdown_as_explicit_error() {
        assert_named_pipe_request_output_error(
            SwitcherQueuedFrameHandoffError::SourceShutdown,
            SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodedOnly,
            SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None,
        );
    }

    #[test]
    fn named_pipe_handoff_wrapper_request_output_preserves_malformed_response_as_explicit_error() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-bad".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime_with_clock(
            "malformed-runtime-pipe",
            77,
            ScriptedNamedPipeRuntime::new(vec![Err(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeError::EncodeRequest(
                    stream_sync_net_core::ServerSwitcherQueuedFrameHandoffCodecError::BodyTooLong {
                        len: usize::MAX,
                    },
                ),
            )]),
            ScriptedNamedPipeClock::new(vec![400, 403]),
        );

        let output = handoff.read_handoff_frame_with_config(
            input.clone(),
            SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
        );

        assert_eq!(output.summary.request_id, 77);
        assert_eq!(
            output.summary.request_status,
            SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodeFailed
        );
        assert_eq!(
            output.summary.response_status,
            SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None
        );
        assert_eq!(
            output.summary.result_kind,
            SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError
        );
        assert_eq!(output.summary.elapsed_millis, 3);
        assert!(output.runtime.is_none());
        assert_eq!(
            output.result,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: input.client_id,
                run_id: input.run_id,
                mode: input.mode,
                error: SwitcherQueuedFrameHandoffError::MalformedResponse,
            }
        );
        assert!(!matches!(
            output.result,
            SwitcherQueuedFrameHandoffResult::NoFrameAvailable { .. }
        ));
    }

    fn assert_named_pipe_request_output_error(
        error: SwitcherQueuedFrameHandoffError,
        request_status: SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
        response_status: SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
    ) {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
        };
        let expected = SwitcherQueuedFrameHandoffResult::HandoffError {
            client_id: input.client_id.clone(),
            run_id: input.run_id.clone(),
            mode: input.mode,
            error: error.clone(),
        };
        let mut handoff = SwitcherNamedPipeQueuedFrameHandoff::from_runtime_with_clock(
            "error-summary-pipe",
            41,
            ScriptedNamedPipeRuntime::new(vec![Ok(
                SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id: 41,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: ServerSwitcherQueuedFrameReadMode::InspectLatest,
                    },
                    response: None,
                    result: expected.clone(),
                },
            )]),
            ScriptedNamedPipeClock::new(vec![200, 206]),
        );

        let output = handoff.read_handoff_frame_with_config(
            input,
            SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
        );

        assert_eq!(output.summary.request_id, 41);
        assert_eq!(output.summary.request_status, request_status);
        assert_eq!(output.summary.response_status, response_status);
        assert_eq!(
            output.summary.result_kind,
            SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError
        );
        assert_eq!(output.summary.elapsed_millis, 6);
        assert_eq!(output.result, expected);
        assert!(!matches!(
            output.result,
            SwitcherQueuedFrameHandoffResult::NoFrameAvailable { .. }
        ));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "local Windows named-pipe smoke test"]
    fn named_pipe_handoff_runtime_round_trips_one_request_and_one_response() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-1",
            "run-1",
            21,
            TimestampMicros(2_500_021),
            960,
            540,
            vec![0xaa, 0xbb, 0xcc, 0xdd],
        );
        let pipe_name = format!("stream-sync-switcher-handoff-{}", current_test_suffix());
        let pipe_name_for_server = pipe_name.clone();
        let server = std::thread::spawn(move || {
            let mut queue_state = state;
            stream_sync_server::ServerSwitcherNamedPipeOneRequestRuntimeBoundary::default()
                .serve_once(&mut queue_state, &pipe_name_for_server)
                .expect("server runtime should serve one request")
        });

        let output = SwitcherNamedPipeQueuedFrameHandoffRuntimeBoundary::default()
            .run_once(
                &pipe_name,
                800,
                SwitcherQueuedFrameHandoffInput {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                },
            )
            .expect("client request should encode");

        let server_output = server.join().expect("server thread should join");
        assert_eq!(output.request.request_id, 800);
        assert_eq!(server_output.request.request_id, 800);
        let response = output
            .response
            .as_ref()
            .expect("response should be available after round trip");
        let ServerSwitcherQueuedFrameHandoffResponse::FrameRead { request_id, .. } = response
        else {
            panic!("named-pipe round trip should return FrameRead");
        };
        assert_eq!(*request_id, 800);
        let SwitcherQueuedFrameHandoffResult::FrameRead {
            frame,
            mode,
            remaining_client_queue_len,
        } = output.result
        else {
            panic!("switcher runtime should map FrameRead");
        };
        assert_eq!(mode, SwitcherSingleClientQueueSourceMode::PreviewLatest);
        assert_eq!(remaining_client_queue_len, 1);
        assert_eq!(frame.frame_id, 21);
        assert_eq!(frame.codec, Codec::H264);
        assert_eq!(frame.encoded_payload, vec![0xaa, 0xbb, 0xcc, 0xdd]);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "local Windows named-pipe smoke test"]
    fn named_pipe_handoff_runtime_maps_missing_pipe_to_source_unavailable() {
        let pipe_name = format!("stream-sync-switcher-missing-{}", current_test_suffix());

        let output = SwitcherNamedPipeQueuedFrameHandoffRuntimeBoundary::default()
            .run_once(
                &pipe_name,
                801,
                SwitcherQueuedFrameHandoffInput {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                },
            )
            .expect("missing pipe should map to handoff error output");

        assert_eq!(output.request.request_id, 801);
        assert!(output.response.is_none());
        assert_eq!(
            output.result,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
            }
        );
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_handoff_runtime_maps_not_found_io_to_source_unavailable() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
        };

        let result = handoff_error_result_from_io(
            &input,
            io::Error::new(io::ErrorKind::NotFound, "pipe missing"),
        );

        assert_eq!(
            result,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
            }
        );
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_handoff_runtime_maps_unexpected_eof_io_to_source_shutdown() {
        let input = SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        };

        let result = handoff_error_result_from_io(
            &input,
            io::Error::new(io::ErrorKind::UnexpectedEof, "server closed"),
        );

        assert_eq!(
            result,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                error: SwitcherQueuedFrameHandoffError::SourceShutdown,
            }
        );
    }

    #[derive(Debug, Clone)]
    struct FailingQueuedFrameHandoff {
        error: SwitcherQueuedFrameHandoffError,
    }

    impl SwitcherQueuedFrameHandoff for FailingQueuedFrameHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: input.client_id,
                run_id: input.run_id,
                mode: input.mode,
                error: self.error.clone(),
            }
        }
    }

    #[test]
    fn single_client_queue_source_handoff_propagates_source_error_from_fake() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::Timeout,
        };

        let result = handoff.read_handoff_frame(SwitcherQueuedFrameHandoffInput {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        });

        assert_eq!(
            result,
            SwitcherQueuedFrameHandoffResult::HandoffError {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                error: SwitcherQueuedFrameHandoffError::Timeout,
            }
        );
    }

    #[test]
    fn single_client_queue_source_handoff_preserves_frame_metadata() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-1",
            "run-meta",
            1,
            TimestampMicros(2_039_000),
            800,
            450,
            vec![0x10, 0x20, 0x30, 0x40, 0x50],
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            handoff.read_handoff_frame(SwitcherQueuedFrameHandoffInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-meta".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
            })
        };

        let SwitcherQueuedFrameHandoffResult::FrameRead { frame, .. } = result else {
            panic!("handoff should preserve metadata frame");
        };
        assert_eq!(frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(frame.run_id, RunId("run-meta".to_string()));
        assert_eq!(frame.frame_id, 1);
        assert_eq!(frame.capture_timestamp, TimestampMicros(1_000_001));
        assert_eq!(frame.send_timestamp, TimestampMicros(1_000_101));
        assert_eq!(frame.queued_at, TimestampMicros(2_039_000));
        assert!(frame.is_keyframe);
        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 450);
        assert_eq!(frame.fps_nominal, 30);
        assert_eq!(frame.codec, Codec::H264);
        assert_eq!(frame.encoded_payload_len, 5);
        assert_eq!(frame.encoded_payload, vec![0x10, 0x20, 0x30, 0x40, 0x50]);
    }

    #[test]
    fn single_client_queue_source_handoff_preview_does_not_mutate_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_040_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            2,
            TimestampMicros(2_040_100),
        );
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            handoff.read_handoff_frame(SwitcherQueuedFrameHandoffInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            })
        };

        let SwitcherQueuedFrameHandoffResult::FrameRead { frame, .. } = result else {
            panic!("handoff should inspect latest frame");
        };
        assert_eq!(frame.frame_id, 2);
        assert_eq!(state.client_queue_len(&client_id), before_len);
    }

    #[test]
    fn single_client_queue_source_handoff_consume_mutates_only_requested_run() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_041_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-2",
            2,
            TimestampMicros(2_041_100),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_041_200),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            handoff.read_handoff_frame(SwitcherQueuedFrameHandoffInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
            })
        };

        let SwitcherQueuedFrameHandoffResult::FrameRead {
            frame,
            remaining_client_queue_len,
            ..
        } = result
        else {
            panic!("handoff should dequeue oldest requested run frame");
        };
        assert_eq!(frame.frame_id, 1);
        assert_eq!(remaining_client_queue_len, 2);
        let remaining: Vec<(String, u64)> = state
            .frames_for_client(&client_id)
            .map(|queued| (queued.frame.run_id.0.clone(), queued.frame.frame_id))
            .collect();
        assert_eq!(
            remaining,
            vec![("run-2".to_string(), 2), ("run-1".to_string(), 3)]
        );
    }

    #[test]
    fn single_client_queue_source_handoff_consumer_converts_frame_to_source_result() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_042_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherQueuedFrameHandoffConsumerBoundary.read_source_result(
                &mut handoff,
                SwitcherQueuedFrameHandoffInput {
                    client_id: client_id.clone(),
                    run_id: RunId("run-1".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
                },
            )
        };

        let SwitcherQueuedFrameHandoffConsumerResult::FrameAvailable { source_result } = result
        else {
            panic!("consumer should convert handoff frame into source frame result");
        };
        let SwitcherSingleClientQueueSourceResult::FrameAvailable { frame, mode, .. } =
            source_result
        else {
            panic!("consumer frame result should be usable as queue-source result");
        };
        assert_eq!(frame.client_id, client_id);
        assert_eq!(frame.frame_id, 1);
        assert_eq!(mode, SwitcherSingleClientQueueSourceMode::PreviewOldest);
    }

    #[test]
    fn single_client_queue_source_handoff_consumer_keeps_no_frame_as_source_result() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_043_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherQueuedFrameHandoffConsumerBoundary.read_source_result(
                &mut handoff,
                SwitcherQueuedFrameHandoffInput {
                    client_id: client_id.clone(),
                    run_id: RunId("run-missing".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                },
            )
        };

        assert_eq!(
            result,
            SwitcherQueuedFrameHandoffConsumerResult::NoFrameAvailable {
                source_result: SwitcherSingleClientQueueSourceResult::NoFrameAvailable {
                    client_id,
                    run_id: RunId("run-missing".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    client_queue_len: 1,
                },
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn single_client_queue_source_handoff_consumer_keeps_each_error_distinct_from_no_frame() {
        let errors = vec![
            SwitcherQueuedFrameHandoffError::SourceUnavailable,
            SwitcherQueuedFrameHandoffError::Timeout,
            SwitcherQueuedFrameHandoffError::InvalidScope {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
            },
            SwitcherQueuedFrameHandoffError::UnsupportedMode {
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            },
            SwitcherQueuedFrameHandoffError::MalformedResponse,
            SwitcherQueuedFrameHandoffError::SourceShutdown,
        ];

        for error in errors {
            let mut handoff = FailingQueuedFrameHandoff {
                error: error.clone(),
            };
            let result = SwitcherQueuedFrameHandoffConsumerBoundary.read_source_result(
                &mut handoff,
                SwitcherQueuedFrameHandoffInput {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                },
            );

            assert_eq!(
                result,
                SwitcherQueuedFrameHandoffConsumerResult::HandoffError {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                    error,
                }
            );
        }
    }

    #[test]
    fn single_client_queue_source_handoff_consumer_preserves_metadata() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-1",
            "run-meta",
            1,
            TimestampMicros(2_044_000),
            1024,
            576,
            vec![0xaa, 0xbb, 0xcc, 0xdd],
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherQueuedFrameHandoffConsumerBoundary.read_source_result(
                &mut handoff,
                SwitcherQueuedFrameHandoffInput {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-meta".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewOldest,
                },
            )
        };

        let SwitcherQueuedFrameHandoffConsumerResult::FrameAvailable { source_result } = result
        else {
            panic!("consumer should expose metadata frame as source result");
        };
        let SwitcherSingleClientQueueSourceResult::FrameAvailable { frame, .. } = source_result
        else {
            panic!("metadata should be available as queue-source frame");
        };
        assert_eq!(frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(frame.run_id, RunId("run-meta".to_string()));
        assert_eq!(frame.frame_id, 1);
        assert_eq!(frame.capture_timestamp, TimestampMicros(1_000_001));
        assert_eq!(frame.send_timestamp, TimestampMicros(1_000_101));
        assert_eq!(frame.queued_at, TimestampMicros(2_044_000));
        assert!(frame.is_keyframe);
        assert_eq!(frame.width, 1024);
        assert_eq!(frame.height, 576);
        assert_eq!(frame.fps_nominal, 30);
        assert_eq!(frame.encoded_payload_len, 4);
        assert_eq!(frame.encoded_payload, vec![0xaa, 0xbb, 0xcc, 0xdd]);
    }

    #[test]
    fn single_client_queue_source_handoff_consumer_preview_does_not_mutate_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_045_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            2,
            TimestampMicros(2_045_100),
        );
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherQueuedFrameHandoffConsumerBoundary.read_source_result(
                &mut handoff,
                SwitcherQueuedFrameHandoffInput {
                    client_id: client_id.clone(),
                    run_id: RunId("run-1".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                },
            )
        };

        assert!(matches!(
            result,
            SwitcherQueuedFrameHandoffConsumerResult::FrameAvailable { .. }
        ));
        assert_eq!(state.client_queue_len(&client_id), before_len);
        let remaining: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(remaining, vec![1, 2]);
    }

    #[test]
    fn single_client_queue_source_handoff_consumer_consume_mutates_only_requested_run() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_046_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-2",
            2,
            TimestampMicros(2_046_100),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_046_200),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherQueuedFrameHandoffConsumerBoundary.read_source_result(
                &mut handoff,
                SwitcherQueuedFrameHandoffInput {
                    client_id: client_id.clone(),
                    run_id: RunId("run-1".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                },
            )
        };

        let SwitcherQueuedFrameHandoffConsumerResult::FrameAvailable { source_result } = result
        else {
            panic!("consumer should expose consumed frame as source result");
        };
        let SwitcherSingleClientQueueSourceResult::FrameAvailable {
            frame,
            remaining_client_queue_len,
            ..
        } = source_result
        else {
            panic!("consume should return a source frame result");
        };
        assert_eq!(frame.frame_id, 1);
        assert_eq!(remaining_client_queue_len, 2);
        let remaining: Vec<(String, u64)> = state
            .frames_for_client(&client_id)
            .map(|queued| (queued.frame.run_id.0.clone(), queued.frame.frame_id))
            .collect();
        assert_eq!(
            remaining,
            vec![("run-2".to_string(), 2), ("run-1".to_string(), 3)]
        );
    }

    #[test]
    fn target_time_single_client_preview_latest_selects_frame_at_or_before_target_without_mutation()
    {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_040_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_040_100),
        );
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let result = SwitcherSingleClientTargetTimeSourceBoundary::default().select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_003),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
            },
        );

        let SwitcherSingleClientTargetTimeSourceResult::Selected(selected) = result else {
            panic!("latest frame should be selected at target");
        };
        assert_eq!(selected.frame.client_id, client_id);
        assert_eq!(selected.frame.run_id, RunId("run-1".to_string()));
        assert_eq!(selected.frame.frame_id, 3);
        assert_eq!(selected.target_timestamp, TimestampMicros(1_000_003));
        assert_eq!(selected.delta_from_target_micros, 0);
        assert!(!selected.consumed);
        assert_eq!(state.client_queue_len(&client_id), before_len);
    }

    #[test]
    fn target_time_single_client_preview_latest_waits_without_mutation_when_candidate_is_after_target(
    ) {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_050_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleClientTargetTimeSourceBoundary::default().select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
            },
        );

        assert_eq!(
            result,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
                client_id,
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                candidate_frame_id: 3,
                candidate_capture_timestamp: TimestampMicros(1_000_003),
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn target_time_single_client_consume_oldest_dequeues_only_when_at_or_before_target() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_060_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_060_100),
        );
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleClientTargetTimeSourceBoundary::default().select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
            },
        );

        let SwitcherSingleClientTargetTimeSourceResult::Selected(selected) = result else {
            panic!("oldest frame should be consumed at target");
        };
        assert_eq!(selected.frame.frame_id, 1);
        assert_eq!(selected.delta_from_target_micros, 0);
        assert!(selected.consumed);
        let remaining: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(remaining, vec![3]);
    }

    #[test]
    fn target_time_single_client_consume_oldest_waits_without_dequeue_when_oldest_is_after_target()
    {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_070_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleClientTargetTimeSourceBoundary::default().select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
            },
        );

        assert_eq!(
            result,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
                client_id,
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
                candidate_frame_id: 3,
                candidate_capture_timestamp: TimestampMicros(1_000_003),
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn target_time_single_client_missing_run_reports_no_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_080_000),
        );

        let result = SwitcherSingleClientTargetTimeSourceBoundary::default().select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-missing".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
            },
        );

        assert_eq!(
            result,
            SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-missing".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
                client_queue_len: 1,
            }
        );
    }

    #[test]
    fn target_time_single_client_empty_queue_reports_no_frame() {
        let mut state = ServerVideoFrameQueueState::default();

        let result = SwitcherSingleClientTargetTimeSourceBoundary::default().select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: ClientId("client-empty".to_string()),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
            },
        );

        assert_eq!(
            result,
            SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable {
                client_id: ClientId("client-empty".to_string()),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                client_queue_len: 0,
            }
        );
        assert_eq!(state.total_len(), 0);
    }

    #[test]
    fn target_time_single_client_live_like_queue_progression_is_explicit() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_090_000),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            2,
            TimestampMicros(2_090_100),
        );
        let client_id = ClientId("client-1".to_string());
        let boundary = SwitcherSingleClientTargetTimeSourceBoundary::default();

        let preview = boundary.select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
            },
        );
        let SwitcherSingleClientTargetTimeSourceResult::Selected(previewed) = preview else {
            panic!("latest preview should select frame 2");
        };
        assert_eq!(previewed.frame.frame_id, 2);
        assert!(!previewed.consumed);
        assert_eq!(state.client_queue_len(&client_id), 2);

        let first_consume = boundary.select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
            },
        );
        let SwitcherSingleClientTargetTimeSourceResult::Selected(consumed) = first_consume else {
            panic!("oldest consume should select frame 1");
        };
        assert_eq!(consumed.frame.frame_id, 1);
        assert!(consumed.consumed);
        let remaining: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(remaining, vec![2]);

        let waiting = boundary.select(
            &mut state,
            SwitcherSingleClientTargetTimeSourceInput {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
            },
        );
        assert_eq!(
            waiting,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
                client_id,
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
                candidate_frame_id: 2,
                candidate_capture_timestamp: TimestampMicros(1_000_002),
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn target_time_handoff_source_selects_frame_at_or_before_target() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_090_200),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherSingleClientTargetTimeHandoffSourceBoundary::default().select_from_handoff(
                &mut handoff,
                SwitcherSingleClientTargetTimeSourceInput {
                    client_id: client_id.clone(),
                    run_id: RunId("run-1".to_string()),
                    target_timestamp: TimestampMicros(1_000_001),
                    mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                },
            )
        };

        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(selected) = result else {
            panic!("handoff targetTime source should select eligible frame");
        };
        assert_eq!(selected.frame.client_id, client_id);
        assert_eq!(selected.frame.frame_id, 1);
        assert_eq!(selected.delta_from_target_micros, 0);
        assert!(!selected.consumed);
    }

    #[test]
    fn target_time_handoff_source_waits_when_frame_is_after_target() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_090_300),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherSingleClientTargetTimeHandoffSourceBoundary::default().select_from_handoff(
                &mut handoff,
                SwitcherSingleClientTargetTimeSourceInput {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    target_timestamp: TimestampMicros(1_000_002),
                    mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                },
            )
        };

        assert_eq!(
            result,
            SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                candidate_frame_id: 3,
                candidate_capture_timestamp: TimestampMicros(1_000_003),
                client_queue_len: 1,
            }
        );
        assert_eq!(state.total_len(), 1);
    }

    #[test]
    fn target_time_handoff_source_preserves_no_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_090_400),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherSingleClientTargetTimeHandoffSourceBoundary::default().select_from_handoff(
                &mut handoff,
                SwitcherSingleClientTargetTimeSourceInput {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-missing".to_string()),
                    target_timestamp: TimestampMicros(1_000_001),
                    mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                },
            )
        };

        assert_eq!(
            result,
            SwitcherSingleClientTargetTimeHandoffSourceResult::NoFrameAvailable {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-missing".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                client_queue_len: 1,
            }
        );
    }

    #[test]
    fn target_time_handoff_source_keeps_each_error_explicit() {
        let errors = vec![
            SwitcherQueuedFrameHandoffError::SourceUnavailable,
            SwitcherQueuedFrameHandoffError::Timeout,
            SwitcherQueuedFrameHandoffError::InvalidScope {
                client_id: ClientId("client-1".to_string()),
                run_id: RunId("run-1".to_string()),
            },
            SwitcherQueuedFrameHandoffError::UnsupportedMode {
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            },
            SwitcherQueuedFrameHandoffError::MalformedResponse,
            SwitcherQueuedFrameHandoffError::SourceShutdown,
        ];

        for error in errors {
            let mut handoff = FailingQueuedFrameHandoff {
                error: error.clone(),
            };
            let result = SwitcherSingleClientTargetTimeHandoffSourceBoundary::default()
                .select_from_handoff(
                    &mut handoff,
                    SwitcherSingleClientTargetTimeSourceInput {
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        target_timestamp: TimestampMicros(1_000_001),
                        mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                    },
                );

            assert_eq!(
                result,
                SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-1".to_string()),
                    target_timestamp: TimestampMicros(1_000_001),
                    mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                    handoff_mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    error,
                }
            );
            assert!(!matches!(
                &result,
                SwitcherSingleClientTargetTimeHandoffSourceResult::NoFrameAvailable { .. }
            ));
            assert!(!matches!(
                &result,
                SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget {
                    ..
                }
            ));
        }
    }

    #[test]
    fn target_time_handoff_source_preserves_selected_metadata() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-1",
            "run-meta",
            1,
            TimestampMicros(2_090_500),
            960,
            540,
            vec![0x01, 0x23, 0x45, 0x67],
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherSingleClientTargetTimeHandoffSourceBoundary::default().select_from_handoff(
                &mut handoff,
                SwitcherSingleClientTargetTimeSourceInput {
                    client_id: ClientId("client-1".to_string()),
                    run_id: RunId("run-meta".to_string()),
                    target_timestamp: TimestampMicros(1_000_001),
                    mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                },
            )
        };

        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(selected) = result else {
            panic!("metadata frame should be selected");
        };
        assert_eq!(selected.frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(selected.frame.run_id, RunId("run-meta".to_string()));
        assert_eq!(selected.frame.frame_id, 1);
        assert_eq!(selected.frame.capture_timestamp, TimestampMicros(1_000_001));
        assert_eq!(selected.frame.send_timestamp, TimestampMicros(1_000_101));
        assert_eq!(selected.frame.queued_at, TimestampMicros(2_090_500));
        assert!(selected.frame.is_keyframe);
        assert_eq!(selected.frame.width, 960);
        assert_eq!(selected.frame.height, 540);
        assert_eq!(selected.frame.fps_nominal, 30);
        assert_eq!(selected.frame.encoded_payload_len, 4);
        assert_eq!(selected.frame.encoded_payload, vec![0x01, 0x23, 0x45, 0x67]);
    }

    #[test]
    fn target_time_handoff_source_preview_does_not_mutate() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_090_600),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            2,
            TimestampMicros(2_090_700),
        );
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherSingleClientTargetTimeHandoffSourceBoundary::default().select_from_handoff(
                &mut handoff,
                SwitcherSingleClientTargetTimeSourceInput {
                    client_id: client_id.clone(),
                    run_id: RunId("run-1".to_string()),
                    target_timestamp: TimestampMicros(1_000_002),
                    mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                },
            )
        };

        assert!(matches!(
            result,
            SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(_)
        ));
        assert_eq!(state.client_queue_len(&client_id), before_len);
    }

    #[test]
    fn target_time_handoff_source_consume_mutates_only_when_selected() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            1,
            TimestampMicros(2_090_800),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-2",
            2,
            TimestampMicros(2_090_900),
        );
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_091_000),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherSingleClientTargetTimeHandoffSourceBoundary::default().select_from_handoff(
                &mut handoff,
                SwitcherSingleClientTargetTimeSourceInput {
                    client_id: client_id.clone(),
                    run_id: RunId("run-1".to_string()),
                    target_timestamp: TimestampMicros(1_000_001),
                    mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
                },
            )
        };

        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(selected) = result else {
            panic!("eligible oldest frame should be consumed");
        };
        assert_eq!(selected.frame.frame_id, 1);
        assert!(selected.consumed);
        let remaining: Vec<(String, u64)> = state
            .frames_for_client(&client_id)
            .map(|queued| (queued.frame.run_id.0.clone(), queued.frame.frame_id))
            .collect();
        assert_eq!(
            remaining,
            vec![("run-2".to_string(), 2), ("run-1".to_string(), 3)]
        );
    }

    #[test]
    fn target_time_handoff_source_consume_waits_without_mutation() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-1",
            "run-1",
            3,
            TimestampMicros(2_091_100),
        );
        let client_id = ClientId("client-1".to_string());

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherSingleClientTargetTimeHandoffSourceBoundary::default().select_from_handoff(
                &mut handoff,
                SwitcherSingleClientTargetTimeSourceInput {
                    client_id: client_id.clone(),
                    run_id: RunId("run-1".to_string()),
                    target_timestamp: TimestampMicros(1_000_002),
                    mode: SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore,
                },
            )
        };

        assert!(matches!(
            result,
            SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert_eq!(state.client_queue_len(&client_id), 1);
    }

    fn fallible_two_view_scheduler_input(
        target_timestamp: TimestampMicros,
        mode: SwitcherTwoViewTargetTimeSourceSchedulerMode,
    ) -> SwitcherTwoViewTargetTimeSourceSchedulerInput {
        SwitcherTwoViewTargetTimeSourceSchedulerInput {
            left: SwitcherTwoViewTargetTimeSourceViewConfig {
                client_id: ClientId("client-left".to_string()),
                run_id: RunId("run-left".to_string()),
            },
            right: SwitcherTwoViewTargetTimeSourceViewConfig {
                client_id: ClientId("client-right".to_string()),
                run_id: RunId("run-right".to_string()),
            },
            target_timestamp,
            mode,
        }
    }

    fn fallible_two_view_scheduler_result(
        state: &mut ServerVideoFrameQueueState,
        target_timestamp: TimestampMicros,
    ) -> SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult {
        let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(state);
        SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default().select_pair_from_handoff(
            &mut handoff,
            fallible_two_view_scheduler_input(
                target_timestamp,
                SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            ),
        )
    }

    fn fallible_two_view_adapter_output(
        scheduler_result: SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult,
    ) -> SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput {
        SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        )
    }

    fn render_fallible_two_view_adapter_output(
        adapter_output: SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput,
        decode: &impl SwitcherH264DecodeRuntimeHook,
        render: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput {
        SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionBoundary::default()
            .render_adapter_output_with_runtimes(
                SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionInput {
                    adapter_output,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 5,
                },
                decode,
                render,
            )
    }

    fn fallible_display_policy_output_for_test(
        connection: SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput,
        previous_left: Option<SwitcherTwoViewDisplayedFrame>,
        previous_right: Option<SwitcherTwoViewDisplayedFrame>,
        current_time: TimestampMicros,
        max_hold_duration_micros: Option<u64>,
    ) -> SwitcherTwoViewHandoffDisplayPolicyOutput {
        SwitcherTwoViewHandoffDisplayPolicyBoundary.decide(
            SwitcherTwoViewHandoffDisplayPolicyInput {
                connection,
                previous_left,
                previous_right,
                current_time,
                max_hold_duration_micros,
            },
        )
    }

    #[test]
    fn two_view_target_time_handoff_source_scheduler_selects_both_views() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_091_200),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_091_300),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    fallible_two_view_scheduler_input(
                        TimestampMicros(1_000_002),
                        SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    ),
                )
        };

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(left) = result.left else {
            panic!("left view should be selected");
        };
        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(right) = result.right
        else {
            panic!("right view should be selected");
        };
        assert_eq!(left.frame.frame_id, 1);
        assert_eq!(right.frame.frame_id, 2);
    }

    #[test]
    fn two_view_target_time_handoff_source_scheduler_reports_selected_and_waiting_without_mutation()
    {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_091_400),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_091_500),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    fallible_two_view_scheduler_input(
                        TimestampMicros(1_000_001),
                        SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    ),
                )
        };

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::PartialSelected
        );
        assert!(matches!(
            result.left,
            SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(_)
        ));
        assert!(matches!(
            result.right,
            SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert_eq!(state.client_queue_len(&left_client_id), 1);
        assert_eq!(state.client_queue_len(&right_client_id), 1);
    }

    #[test]
    fn two_view_target_time_handoff_source_scheduler_reports_selected_and_no_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_091_600),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    fallible_two_view_scheduler_input(
                        TimestampMicros(1_000_001),
                        SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    ),
                )
        };

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::PartialSelected
        );
        assert!(matches!(
            result.left,
            SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(_)
        ));
        assert!(matches!(
            result.right,
            SwitcherSingleClientTargetTimeHandoffSourceResult::NoFrameAvailable { .. }
        ));
    }

    #[test]
    fn two_view_target_time_handoff_source_scheduler_reports_selected_and_handoff_error() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_091_700),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    SwitcherTwoViewTargetTimeSourceSchedulerInput {
                        left: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("client-left".to_string()),
                            run_id: RunId("run-left".to_string()),
                        },
                        right: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("".to_string()),
                            run_id: RunId("run-right".to_string()),
                        },
                        target_timestamp: TimestampMicros(1_000_001),
                        mode:
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    },
                )
        };

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            result.left,
            SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(_)
        ));
        assert!(matches!(
            result.right,
            SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError { .. }
        ));
        assert_ne!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::NoFrames
        );
        assert_ne!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::Waiting
        );
    }

    #[test]
    fn two_view_target_time_handoff_source_scheduler_reports_both_handoff_errors() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
        };

        let result = SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
            .select_pair_from_handoff(
                &mut handoff,
                fallible_two_view_scheduler_input(
                    TimestampMicros(1_000_001),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
            );

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            result.left,
            SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError { .. }
        ));
        assert!(matches!(
            result.right,
            SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError { .. }
        ));
        assert_ne!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::NoFrames
        );
        assert_ne!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::Waiting
        );
    }

    #[test]
    fn two_view_target_time_handoff_source_scheduler_consume_is_all_or_nothing_when_selected() {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_091_800),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_091_900),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    fallible_two_view_scheduler_input(
                        TimestampMicros(1_000_002),
                        SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
                    ),
                )
        };

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(left) = result.left else {
            panic!("left frame should be consumed");
        };
        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(right) = result.right
        else {
            panic!("right frame should be consumed");
        };
        assert!(left.consumed);
        assert!(right.consumed);
        assert_eq!(state.client_queue_len(&left_client_id), 0);
        assert_eq!(state.client_queue_len(&right_client_id), 0);
    }

    #[test]
    fn two_view_target_time_handoff_source_scheduler_consume_does_not_mutate_on_handoff_error() {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_092_000),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    SwitcherTwoViewTargetTimeSourceSchedulerInput {
                        left: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: left_client_id.clone(),
                            run_id: RunId("run-left".to_string()),
                        },
                        right: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("".to_string()),
                            run_id: RunId("run-right".to_string()),
                        },
                        target_timestamp: TimestampMicros(1_000_001),
                        mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
                    },
                )
        };

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(left) = result.left else {
            panic!("left eligible frame should only be previewed");
        };
        assert!(!left.consumed);
        assert_eq!(state.client_queue_len(&left_client_id), 1);
    }

    #[test]
    fn two_view_target_time_handoff_source_scheduler_preserves_selected_metadata() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_092_100),
            640,
            360,
            vec![0x01, 0x02, 0x03],
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_092_200),
        );

        let result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    fallible_two_view_scheduler_input(
                        TimestampMicros(1_000_002),
                        SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    ),
                )
        };

        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(left) = result.left else {
            panic!("left metadata frame should be selected");
        };
        assert_eq!(left.frame.client_id, ClientId("client-left".to_string()));
        assert_eq!(left.frame.run_id, RunId("run-left".to_string()));
        assert_eq!(left.frame.width, 640);
        assert_eq!(left.frame.height, 360);
        assert_eq!(left.frame.encoded_payload_len, 3);
        assert_eq!(left.frame.encoded_payload, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn two_view_target_time_source_scheduler_selects_both_views_with_shared_target() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_091_000),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_091_100),
        );

        let result = SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default().select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-left".to_string()),
                    run_id: RunId("run-left".to_string()),
                },
                right: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-right".to_string()),
                    run_id: RunId("run-right".to_string()),
                },
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            },
        );

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        assert_eq!(result.target_timestamp, TimestampMicros(1_000_002));
        let SwitcherSingleClientTargetTimeSourceResult::Selected(left) = result.left else {
            panic!("left view should be selected");
        };
        let SwitcherSingleClientTargetTimeSourceResult::Selected(right) = result.right else {
            panic!("right view should be selected");
        };
        assert_eq!(left.frame.frame_id, 1);
        assert_eq!(right.frame.frame_id, 2);
        assert_eq!(left.target_timestamp, TimestampMicros(1_000_002));
        assert_eq!(right.target_timestamp, TimestampMicros(1_000_002));
        assert!(!left.consumed);
        assert!(!right.consumed);
    }

    #[test]
    fn two_view_target_time_source_scheduler_reports_partial_when_one_view_waits() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_092_000),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_092_100),
        );

        let result = SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default().select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-left".to_string()),
                    run_id: RunId("run-left".to_string()),
                },
                right: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-right".to_string()),
                    run_id: RunId("run-right".to_string()),
                },
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            },
        );

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::PartialSelected
        );
        assert!(matches!(
            result.left,
            SwitcherSingleClientTargetTimeSourceResult::Selected(_)
        ));
        assert_eq!(
            result.right,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
                client_id: ClientId("client-right".to_string()),
                run_id: RunId("run-right".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                candidate_frame_id: 3,
                candidate_capture_timestamp: TimestampMicros(1_000_003),
                client_queue_len: 1,
            }
        );
    }

    #[test]
    fn two_view_target_time_source_scheduler_reports_partial_when_one_view_has_no_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_093_000),
        );

        let result = SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default().select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-left".to_string()),
                    run_id: RunId("run-left".to_string()),
                },
                right: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-right".to_string()),
                    run_id: RunId("run-right".to_string()),
                },
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            },
        );

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::PartialSelected
        );
        assert!(matches!(
            result.left,
            SwitcherSingleClientTargetTimeSourceResult::Selected(_)
        ));
        assert_eq!(
            result.right,
            SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable {
                client_id: ClientId("client-right".to_string()),
                run_id: RunId("run-right".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
                client_queue_len: 0,
            }
        );
    }

    #[test]
    fn two_view_target_time_source_scheduler_preview_does_not_mutate_queues() {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_094_000),
        );
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            2,
            TimestampMicros(2_094_100),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_094_200),
        );
        let before_left_len = state.client_queue_len(&left_client_id);
        let before_right_len = state.client_queue_len(&right_client_id);

        let result = SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default().select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: left_client_id.clone(),
                    run_id: RunId("run-left".to_string()),
                },
                right: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: right_client_id.clone(),
                    run_id: RunId("run-right".to_string()),
                },
                target_timestamp: TimestampMicros(1_000_003),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            },
        );

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        assert_eq!(state.client_queue_len(&left_client_id), before_left_len);
        assert_eq!(state.client_queue_len(&right_client_id), before_right_len);
        let left_frame_ids: Vec<u64> = state
            .frames_for_client(&left_client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        let right_frame_ids: Vec<u64> = state
            .frames_for_client(&right_client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(left_frame_ids, vec![1, 2]);
        assert_eq!(right_frame_ids, vec![3]);
    }

    #[test]
    fn two_view_target_time_source_scheduler_consume_waits_without_partial_mutation() {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_095_000),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_095_100),
        );

        let result = SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default().select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: left_client_id.clone(),
                    run_id: RunId("run-left".to_string()),
                },
                right: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: right_client_id.clone(),
                    run_id: RunId("run-right".to_string()),
                },
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
            },
        );

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::PartialSelected
        );
        let SwitcherSingleClientTargetTimeSourceResult::Selected(left) = result.left else {
            panic!("left eligible frame should be previewed");
        };
        assert_eq!(left.frame.frame_id, 1);
        assert!(!left.consumed);
        assert_eq!(
            result.right,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget {
                client_id: right_client_id.clone(),
                run_id: RunId("run-right".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherSingleClientTargetTimeSourceMode::PreviewOldestIfAtOrBefore,
                candidate_frame_id: 3,
                candidate_capture_timestamp: TimestampMicros(1_000_003),
                client_queue_len: 1,
            }
        );
        assert_eq!(state.client_queue_len(&left_client_id), 1);
        assert_eq!(state.client_queue_len(&right_client_id), 1);
    }

    #[test]
    fn two_view_target_time_source_scheduler_reports_no_frames_when_both_views_empty() {
        let mut state = ServerVideoFrameQueueState::default();

        let result = SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default().select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-left".to_string()),
                    run_id: RunId("run-left".to_string()),
                },
                right: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-right".to_string()),
                    run_id: RunId("run-right".to_string()),
                },
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            },
        );

        assert_eq!(
            result.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::NoFrames
        );
        assert!(matches!(
            result.left,
            SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable { .. }
        ));
        assert!(matches!(
            result.right,
            SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable { .. }
        ));
    }

    #[test]
    fn two_view_target_time_source_scheduler_live_like_preview_progression_does_not_mutate() {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_096_000),
        );
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            2,
            TimestampMicros(2_096_100),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            1,
            TimestampMicros(2_096_200),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_096_300),
        );
        let boundary = SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default();
        let left = SwitcherTwoViewTargetTimeSourceViewConfig {
            client_id: left_client_id.clone(),
            run_id: RunId("run-left".to_string()),
        };
        let right = SwitcherTwoViewTargetTimeSourceViewConfig {
            client_id: right_client_id.clone(),
            run_id: RunId("run-right".to_string()),
        };

        let early = boundary.select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: left.clone(),
                right: right.clone(),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            },
        );
        assert_eq!(
            early.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::Waiting
        );

        let middle = boundary.select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: left.clone(),
                right: right.clone(),
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            },
        );
        assert_eq!(
            middle.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::PartialSelected
        );
        assert!(matches!(
            middle.left,
            SwitcherSingleClientTargetTimeSourceResult::Selected(_)
        ));
        assert!(matches!(
            middle.right,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
        ));

        let ready = boundary.select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left,
                right,
                target_timestamp: TimestampMicros(1_000_003),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            },
        );
        assert_eq!(
            ready.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        let SwitcherSingleClientTargetTimeSourceResult::Selected(left_ready) = ready.left else {
            panic!("left latest frame should be selected at the final target");
        };
        let SwitcherSingleClientTargetTimeSourceResult::Selected(right_ready) = ready.right else {
            panic!("right latest frame should be selected at the final target");
        };
        assert_eq!(left_ready.frame.frame_id, 2);
        assert_eq!(right_ready.frame.frame_id, 3);
        assert!(!left_ready.consumed);
        assert!(!right_ready.consumed);
        assert_eq!(state.client_queue_len(&left_client_id), 2);
        assert_eq!(state.client_queue_len(&right_client_id), 2);
    }

    #[test]
    fn two_view_target_time_source_scheduler_live_like_consume_is_all_or_nothing() {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_097_000),
        );
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            2,
            TimestampMicros(2_097_100),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            1,
            TimestampMicros(2_097_200),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_097_300),
        );
        let boundary = SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default();
        let left = SwitcherTwoViewTargetTimeSourceViewConfig {
            client_id: left_client_id.clone(),
            run_id: RunId("run-left".to_string()),
        };
        let right = SwitcherTwoViewTargetTimeSourceViewConfig {
            client_id: right_client_id.clone(),
            run_id: RunId("run-right".to_string()),
        };

        let first = boundary.select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: left.clone(),
                right: right.clone(),
                target_timestamp: TimestampMicros(1_000_001),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
            },
        );
        assert_eq!(
            first.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        let SwitcherSingleClientTargetTimeSourceResult::Selected(first_left) = first.left else {
            panic!("left first frame should be consumed");
        };
        let SwitcherSingleClientTargetTimeSourceResult::Selected(first_right) = first.right else {
            panic!("right first frame should be consumed");
        };
        assert_eq!(first_left.frame.frame_id, 1);
        assert_eq!(first_right.frame.frame_id, 1);
        assert!(first_left.consumed);
        assert!(first_right.consumed);
        assert_eq!(state.client_queue_len(&left_client_id), 1);
        assert_eq!(state.client_queue_len(&right_client_id), 1);

        let waiting = boundary.select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: left.clone(),
                right: right.clone(),
                target_timestamp: TimestampMicros(1_000_002),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
            },
        );
        assert_eq!(
            waiting.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::PartialSelected
        );
        let SwitcherSingleClientTargetTimeSourceResult::Selected(waiting_left) = waiting.left
        else {
            panic!("left second frame should be preview-selected while right waits");
        };
        assert_eq!(waiting_left.frame.frame_id, 2);
        assert!(!waiting_left.consumed);
        assert!(matches!(
            waiting.right,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert_eq!(state.client_queue_len(&left_client_id), 1);
        assert_eq!(state.client_queue_len(&right_client_id), 1);

        let final_ready = boundary.select_pair(
            &mut state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left,
                right,
                target_timestamp: TimestampMicros(1_000_003),
                mode: SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
            },
        );
        assert_eq!(
            final_ready.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        let SwitcherSingleClientTargetTimeSourceResult::Selected(final_left) = final_ready.left
        else {
            panic!("left second frame should be consumed once both views are ready");
        };
        let SwitcherSingleClientTargetTimeSourceResult::Selected(final_right) = final_ready.right
        else {
            panic!("right future frame should be consumed once the target reaches it");
        };
        assert_eq!(final_left.frame.frame_id, 2);
        assert_eq!(final_right.frame.frame_id, 3);
        assert!(final_left.consumed);
        assert!(final_right.consumed);
        assert_eq!(state.client_queue_len(&left_client_id), 0);
        assert_eq!(state.client_queue_len(&right_client_id), 0);
    }

    #[test]
    fn two_view_scheduler_decode_render_adapter_maps_both_selected_to_renderable_inputs() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_098_000),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_098_100),
        );
        let scheduler_result = two_view_scheduler_result(&mut state, TimestampMicros(1_000_002));

        let output = SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        let SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame {
            selected: left, ..
        } = output.left
        else {
            panic!("left side should be renderable");
        };
        let SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame {
            selected: right,
            ..
        } = output.right
        else {
            panic!("right side should be renderable");
        };
        assert_eq!(left.frame.frame_id, 1);
        assert_eq!(right.frame.frame_id, 2);
        let SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
            shared_target_time,
            left,
            right,
        } = output.decode_render_input.selection
        else {
            panic!("decode/render input should receive both selected frames");
        };
        assert_eq!(shared_target_time, TimestampMicros(1_000_002));
        assert_eq!(left.frame.frame_id, 1);
        assert_eq!(right.frame.frame_id, 2);
    }

    #[test]
    fn two_view_scheduler_decode_render_adapter_maps_selected_and_waiting() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_099_000),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_099_100),
        );
        let scheduler_result = two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));

        let output = SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert_eq!(
            output.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget {
                side: SwitcherTwoViewSide::Right,
                client_id: ClientId("client-right".to_string()),
                run_id: RunId("run-right".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                candidate_frame_id: 3,
                candidate_capture_timestamp: TimestampMicros(1_000_003),
                client_queue_len: 1,
            }
        );
        let SwitcherTwoViewTargetTimeSelectionResult::Partial { left, right, .. } =
            output.decode_render_input.selection
        else {
            panic!("decode/render input should receive partial selection");
        };
        assert!(matches!(
            left,
            SwitcherJitterBufferSelectionResult::Selected(_)
        ));
        assert_eq!(
            right,
            SwitcherJitterBufferSelectionResult::FrameTooEarly {
                client_id: ClientId("client-right".to_string()),
                target_time: TimestampMicros(1_000_001),
                earliest_frame_time: TimestampMicros(1_000_003),
                frames_available: 1,
            }
        );
    }

    #[test]
    fn two_view_scheduler_decode_render_adapter_maps_selected_and_no_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_100_000),
        );
        let scheduler_result = two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));

        let output = SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert_eq!(
            output.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable {
                side: SwitcherTwoViewSide::Right,
                client_id: ClientId("client-right".to_string()),
                run_id: RunId("run-right".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                client_queue_len: 0,
            }
        );
        let SwitcherTwoViewTargetTimeSelectionResult::Partial { left, right, .. } =
            output.decode_render_input.selection
        else {
            panic!("decode/render input should receive partial selection");
        };
        assert!(matches!(
            left,
            SwitcherJitterBufferSelectionResult::Selected(_)
        ));
        assert_eq!(
            right,
            SwitcherJitterBufferSelectionResult::NoFrame {
                client_id: ClientId("client-right".to_string()),
                target_time: TimestampMicros(1_000_001),
            }
        );
    }

    #[test]
    fn two_view_scheduler_decode_render_adapter_does_not_create_fake_frames() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            3,
            TimestampMicros(2_101_000),
        );
        let scheduler_result = two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));

        let output = SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(matches!(
            output.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable { .. }
        ));
        let SwitcherTwoViewTargetTimeSelectionResult::BothUnavailable { left, right, .. } =
            output.decode_render_input.selection
        else {
            panic!("waiting/no-frame should not become a partial render selection");
        };
        assert!(matches!(
            left,
            SwitcherJitterBufferSelectionResult::FrameTooEarly { .. }
        ));
        assert!(matches!(
            right,
            SwitcherJitterBufferSelectionResult::NoFrame { .. }
        ));
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_adapter_maps_both_selected_to_renderable_inputs() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_101_100),
            640,
            360,
            vec![0x01, 0x02, 0x03],
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_101_200),
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_002));

        let output = SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        let SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame {
            selected: left,
            ..
        } = output.left
        else {
            panic!("left side should be renderable");
        };
        let SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame {
            selected: right,
            ..
        } = output.right
        else {
            panic!("right side should be renderable");
        };
        assert_eq!(left.frame.frame_id, 1);
        assert_eq!(left.frame.width, 640);
        assert_eq!(left.frame.height, 360);
        assert_eq!(left.frame.encoded_payload, vec![0x01, 0x02, 0x03]);
        assert_eq!(right.frame.frame_id, 2);
        let input = output
            .decode_render_input
            .expect("both selected should be representable for decode/render");
        let SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
            shared_target_time,
            left,
            right,
        } = input.selection
        else {
            panic!("decode/render input should receive both selected frames");
        };
        assert_eq!(shared_target_time, TimestampMicros(1_000_002));
        assert_eq!(left.frame.frame_id, 1);
        assert_eq!(right.frame.frame_id, 2);
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_adapter_maps_selected_and_waiting() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_101_300),
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_101_400),
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));

        let output = SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert_eq!(
            output.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget {
                side: SwitcherTwoViewSide::Right,
                client_id: ClientId("client-right".to_string()),
                run_id: RunId("run-right".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                candidate_frame_id: 3,
                candidate_capture_timestamp: TimestampMicros(1_000_003),
                client_queue_len: 1,
            }
        );
        let input = output
            .decode_render_input
            .expect("waiting is representable as a decode/render skip");
        let SwitcherTwoViewTargetTimeSelectionResult::Partial { left, right, .. } = input.selection
        else {
            panic!("decode/render input should receive partial selection");
        };
        assert!(matches!(
            left,
            SwitcherJitterBufferSelectionResult::Selected(_)
        ));
        assert!(matches!(
            right,
            SwitcherJitterBufferSelectionResult::FrameTooEarly { .. }
        ));
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_adapter_maps_selected_and_no_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_101_500),
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));

        let output = SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert_eq!(
            output.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable {
                side: SwitcherTwoViewSide::Right,
                client_id: ClientId("client-right".to_string()),
                run_id: RunId("run-right".to_string()),
                target_timestamp: TimestampMicros(1_000_001),
                client_queue_len: 0,
            }
        );
        let input = output
            .decode_render_input
            .expect("no-frame is representable as a decode/render skip");
        let SwitcherTwoViewTargetTimeSelectionResult::Partial { left, right, .. } = input.selection
        else {
            panic!("decode/render input should receive partial selection");
        };
        assert!(matches!(
            left,
            SwitcherJitterBufferSelectionResult::Selected(_)
        ));
        assert!(matches!(
            right,
            SwitcherJitterBufferSelectionResult::NoFrame { .. }
        ));
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_adapter_preserves_selected_and_handoff_error() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_101_600),
        );
        let scheduler_result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    SwitcherTwoViewTargetTimeSourceSchedulerInput {
                        left: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("client-left".to_string()),
                            run_id: RunId("run-left".to_string()),
                        },
                        right: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("".to_string()),
                            run_id: RunId("run-right".to_string()),
                        },
                        target_timestamp: TimestampMicros(1_000_001),
                        mode:
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    },
                )
        };

        let output = SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert!(matches!(
            output.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipHandoffError {
                error: SwitcherQueuedFrameHandoffError::InvalidScope { .. },
                ..
            }
        ));
        assert!(!matches!(
            output.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable { .. }
        ));
        assert!(!matches!(
            output.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(
            output.decode_render_input.is_none(),
            "source errors must not be hidden inside the existing decode/render input"
        );
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_adapter_preserves_both_handoff_errors() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
        };
        let scheduler_result = SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
            .select_pair_from_handoff(
                &mut handoff,
                fallible_two_view_scheduler_input(
                    TimestampMicros(1_000_001),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
            );

        let output = SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipHandoffError {
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                ..
            }
        ));
        assert!(matches!(
            output.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipHandoffError {
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                ..
            }
        ));
        assert!(!matches!(
            output.left,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable { .. }
                | SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(!matches!(
            output.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable { .. }
                | SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(output.decode_render_input.is_none());
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_adapter_creates_no_fake_frames_for_errors() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_for_run(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_101_700),
        );
        let scheduler_result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    SwitcherTwoViewTargetTimeSourceSchedulerInput {
                        left: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("client-left".to_string()),
                            run_id: RunId("run-left".to_string()),
                        },
                        right: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("".to_string()),
                            run_id: RunId("run-right".to_string()),
                        },
                        target_timestamp: TimestampMicros(1_000_001),
                        mode:
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    },
                )
        };

        let output = SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary.adapt(
            SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput {
                scheduler_result,
                left_window_title: "left".to_string(),
                right_window_title: "right".to_string(),
                render_hold_millis: 5,
            },
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert!(matches!(
            output.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipHandoffError { .. }
        ));
        assert!(
            output.decode_render_input.is_none(),
            "no fallback decode/render selection should be created for source errors"
        );
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_connection_renders_both_frames() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_101_800),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_101_900),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_002));
        let adapter_output = fallible_two_view_adapter_output(scheduler_result);
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output =
            render_fallible_two_view_adapter_output(adapter_output, &decode, &render_runtime);

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        let SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothRendered {
            shared_target_time,
            left,
            right,
        } = output.render
        else {
            panic!("both render frame instructions should decode/render");
        };
        assert_eq!(shared_target_time, TimestampMicros(1_000_002));
        assert_eq!(left.selected.frame.frame_id, 1);
        assert_eq!(right.selected.frame.frame_id, 2);
        let decode_inputs = decode.inputs.borrow();
        assert_eq!(decode_inputs.len(), 2);
        assert_eq!(decode_inputs[0].encoded_payload, vec![0, 0, 1, 0x65, 0x11]);
        assert_eq!(decode_inputs[1].encoded_payload, vec![0, 0, 1, 0x65, 0x12]);
        assert_eq!(render_runtime.requests.borrow().len(), 2);
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_connection_preserves_render_and_no_frame_skip() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_102_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));
        let adapter_output = fallible_two_view_adapter_output(scheduler_result);
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output =
            render_fallible_two_view_adapter_output(adapter_output, &decode, &render_runtime);

        let SwitcherTwoViewHandoffDecodeRenderConnectionResult::LeftRenderedRightSkipped {
            left,
            right,
            ..
        } = output.render
        else {
            panic!("left render and right no-frame skip should stay explicit");
        };
        assert_eq!(left.selected.frame.frame_id, 1);
        assert!(matches!(
            right,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable {
                side: SwitcherTwoViewSide::Right,
                client_id,
                run_id,
                target_timestamp: TimestampMicros(1_000_001),
                client_queue_len: 0,
            } if client_id == ClientId("client-right".to_string())
                && run_id == RunId("run-right".to_string())
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render_runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_connection_preserves_render_and_waiting_skip() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_102_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_102_200),
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));
        let adapter_output = fallible_two_view_adapter_output(scheduler_result);
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output =
            render_fallible_two_view_adapter_output(adapter_output, &decode, &render_runtime);

        let SwitcherTwoViewHandoffDecodeRenderConnectionResult::LeftRenderedRightSkipped {
            left,
            right,
            ..
        } = output.render
        else {
            panic!("left render and right waiting skip should stay explicit");
        };
        assert_eq!(left.selected.frame.frame_id, 1);
        assert!(matches!(
            right,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget {
                side: SwitcherTwoViewSide::Right,
                client_id,
                run_id,
                target_timestamp: TimestampMicros(1_000_001),
                candidate_frame_id: 3,
                candidate_capture_timestamp: TimestampMicros(1_000_003),
                client_queue_len: 1,
            } if client_id == ClientId("client-right".to_string())
                && run_id == RunId("run-right".to_string())
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render_runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_connection_preserves_render_and_source_error_skip()
    {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_102_300),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let scheduler_result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    SwitcherTwoViewTargetTimeSourceSchedulerInput {
                        left: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("client-left".to_string()),
                            run_id: RunId("run-left".to_string()),
                        },
                        right: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("".to_string()),
                            run_id: RunId("run-right".to_string()),
                        },
                        target_timestamp: TimestampMicros(1_000_001),
                        mode:
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    },
                )
        };
        let adapter_output = fallible_two_view_adapter_output(scheduler_result);
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output =
            render_fallible_two_view_adapter_output(adapter_output, &decode, &render_runtime);

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        let SwitcherTwoViewHandoffDecodeRenderConnectionResult::LeftRenderedRightSkipped {
            right,
            ..
        } = output.render
        else {
            panic!("left render and right source error skip should stay explicit");
        };
        assert!(matches!(
            right,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                side: SwitcherTwoViewSide::Right,
                error: SwitcherQueuedFrameHandoffError::InvalidScope { .. },
                ..
            }
        ));
        assert!(!matches!(
            right,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
                | SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render_runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_connection_preserves_both_source_errors() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
        };
        let scheduler_result = SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
            .select_pair_from_handoff(
                &mut handoff,
                fallible_two_view_scheduler_input(
                    TimestampMicros(1_000_001),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
            );
        let adapter_output = fallible_two_view_adapter_output(scheduler_result);
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output =
            render_fallible_two_view_adapter_output(adapter_output, &decode, &render_runtime);

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        let SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothSkipped { left, right, .. } =
            output.render
        else {
            panic!("both source errors should produce two source-error skips");
        };
        assert!(matches!(
            left,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                side: SwitcherTwoViewSide::Left,
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                ..
            }
        ));
        assert!(matches!(
            right,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                side: SwitcherTwoViewSide::Right,
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                ..
            }
        ));
        assert!(!matches!(
            left,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
                | SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(!matches!(
            right,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
                | SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert_eq!(decode.inputs.borrow().len(), 0);
        assert_eq!(render_runtime.requests.borrow().len(), 0);
    }

    #[test]
    fn two_view_handoff_scheduler_decode_render_connection_creates_no_fake_frames_for_skips() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::Timeout,
        };
        let scheduler_result = SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
            .select_pair_from_handoff(
                &mut handoff,
                fallible_two_view_scheduler_input(
                    TimestampMicros(1_000_001),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
            );
        let adapter_output = fallible_two_view_adapter_output(scheduler_result);
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output =
            render_fallible_two_view_adapter_output(adapter_output, &decode, &render_runtime);

        assert!(matches!(
            output.render,
            SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothSkipped { .. }
        ));
        assert_eq!(
            decode.inputs.borrow().len(),
            0,
            "source-error skips must not create decode input"
        );
        assert_eq!(
            render_runtime.requests.borrow().len(),
            0,
            "source-error skips must not create render input"
        );
    }

    #[test]
    fn two_view_handoff_display_policy_updates_both_newly_rendered_frames() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_102_400),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_102_500),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_002));
        let connection = render_fallible_two_view_adapter_output(
            fallible_two_view_adapter_output(scheduler_result),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output = fallible_display_policy_output_for_test(
            connection,
            None,
            None,
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        let SwitcherTwoViewHandoffDisplayDecision::Update {
            frame: left_frame,
            rendered: left_rendered,
            ..
        } = output.left
        else {
            panic!("left should update from newly rendered frame");
        };
        let SwitcherTwoViewHandoffDisplayDecision::Update {
            frame: right_frame,
            rendered: right_rendered,
            ..
        } = output.right
        else {
            panic!("right should update from newly rendered frame");
        };
        assert_eq!(left_frame.displayed_at, TimestampMicros(2_000_000));
        assert_eq!(right_frame.displayed_at, TimestampMicros(2_000_000));
        assert_eq!(left_rendered.selected.frame.frame_id, 1);
        assert_eq!(right_rendered.selected.frame.frame_id, 2);
    }

    #[test]
    fn two_view_handoff_display_policy_holds_previous_for_no_frame_skip() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_102_600),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));
        let connection = render_fallible_two_view_adapter_output(
            fallible_two_view_adapter_output(scheduler_result),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output = fallible_display_policy_output_for_test(
            connection,
            None,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_950_000),
            )),
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffDisplayDecision::Update { .. }
        ));
        let SwitcherTwoViewHandoffDisplayDecision::HoldPrevious {
            skipped,
            hold_duration_micros,
            ..
        } = output.right
        else {
            panic!("right no-frame skip should hold previous frame");
        };
        assert_eq!(hold_duration_micros, 50_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable {
                side: SwitcherTwoViewSide::Right,
                ..
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_policy_holds_previous_for_waiting_skip() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_102_700),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_for_run(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_102_800),
        );
        let scheduler_result =
            fallible_two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));
        let connection = render_fallible_two_view_adapter_output(
            fallible_two_view_adapter_output(scheduler_result),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output = fallible_display_policy_output_for_test(
            connection,
            None,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_900_000),
            )),
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffDisplayDecision::Update { .. }
        ));
        let SwitcherTwoViewHandoffDisplayDecision::HoldPrevious {
            skipped,
            hold_duration_micros,
            ..
        } = output.right
        else {
            panic!("right waiting skip should hold previous frame");
        };
        assert_eq!(hold_duration_micros, 100_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget {
                side: SwitcherTwoViewSide::Right,
                candidate_frame_id: 3,
                ..
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_policy_holds_previous_for_source_error_skip() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_102_900),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let scheduler_result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    SwitcherTwoViewTargetTimeSourceSchedulerInput {
                        left: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("client-left".to_string()),
                            run_id: RunId("run-left".to_string()),
                        },
                        right: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("".to_string()),
                            run_id: RunId("run-right".to_string()),
                        },
                        target_timestamp: TimestampMicros(1_000_001),
                        mode:
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    },
                )
        };
        let connection = render_fallible_two_view_adapter_output(
            fallible_two_view_adapter_output(scheduler_result),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output = fallible_display_policy_output_for_test(
            connection,
            None,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_975_000),
            )),
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffDisplayDecision::Update { .. }
        ));
        let SwitcherTwoViewHandoffDisplayDecision::HoldPrevious {
            skipped,
            hold_duration_micros,
            ..
        } = output.right
        else {
            panic!("right source-error skip should hold previous frame");
        };
        assert_eq!(hold_duration_micros, 25_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                side: SwitcherTwoViewSide::Right,
                error: SwitcherQueuedFrameHandoffError::InvalidScope { .. },
                ..
            }
        ));
        assert!(!matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
                | SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget { .. }
        ));
    }

    #[test]
    fn two_view_handoff_display_policy_uses_source_error_placeholder_without_previous() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::Timeout,
        };
        let scheduler_result = SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
            .select_pair_from_handoff(
                &mut handoff,
                fallible_two_view_scheduler_input(
                    TimestampMicros(1_000_001),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
            );
        let connection = render_fallible_two_view_adapter_output(
            fallible_two_view_adapter_output(scheduler_result),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output = fallible_display_policy_output_for_test(
            connection,
            None,
            None,
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let SwitcherTwoViewHandoffDisplayDecision::NoDisplayPlaceholder {
            skipped: left_skipped,
            ..
        } = output.left
        else {
            panic!("left source error without previous frame should be a placeholder");
        };
        let SwitcherTwoViewHandoffDisplayDecision::NoDisplayPlaceholder {
            skipped: right_skipped,
            ..
        } = output.right
        else {
            panic!("right source error without previous frame should be a placeholder");
        };
        assert!(matches!(
            left_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::Timeout,
                ..
            }
        ));
        assert!(matches!(
            right_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::Timeout,
                ..
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_policy_keeps_both_source_errors_explicit() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
        };
        let scheduler_result = SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
            .select_pair_from_handoff(
                &mut handoff,
                fallible_two_view_scheduler_input(
                    TimestampMicros(1_000_001),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
            );
        let connection = render_fallible_two_view_adapter_output(
            fallible_two_view_adapter_output(scheduler_result),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output = fallible_display_policy_output_for_test(
            connection,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Left,
                TimestampMicros(1_990_000),
            )),
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_990_000),
            )),
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let SwitcherTwoViewHandoffDisplayDecision::HoldPrevious {
            skipped: left_skipped,
            ..
        } = output.left
        else {
            panic!("left source error should hold previous with source-error detail");
        };
        let SwitcherTwoViewHandoffDisplayDecision::HoldPrevious {
            skipped: right_skipped,
            ..
        } = output.right
        else {
            panic!("right source error should hold previous with source-error detail");
        };
        assert!(matches!(
            left_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                ..
            }
        ));
        assert!(matches!(
            right_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                ..
            }
        ));
        assert!(!matches!(
            left_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
                | SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(!matches!(
            right_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
                | SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget { .. }
        ));
    }

    #[test]
    fn two_view_handoff_display_policy_marks_stale_previous_source_error_past_max_hold() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::SourceShutdown,
        };
        let scheduler_result = SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
            .select_pair_from_handoff(
                &mut handoff,
                fallible_two_view_scheduler_input(
                    TimestampMicros(1_000_001),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
            );
        let connection = render_fallible_two_view_adapter_output(
            fallible_two_view_adapter_output(scheduler_result),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output = fallible_display_policy_output_for_test(
            connection,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Left,
                TimestampMicros(1_800_000),
            )),
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_800_000),
            )),
            TimestampMicros(2_000_000),
            Some(100_000),
        );

        let SwitcherTwoViewHandoffDisplayDecision::PreviousFrameStale {
            skipped,
            hold_duration_micros,
            max_hold_duration_micros,
            ..
        } = output.left
        else {
            panic!("left source error should become stale when max hold is exceeded");
        };
        assert_eq!(hold_duration_micros, 200_000);
        assert_eq!(max_hold_duration_micros, 100_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::SourceShutdown,
                ..
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_policy_creates_no_fake_frames_for_source_errors() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::MalformedResponse,
        };
        let scheduler_result = SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
            .select_pair_from_handoff(
                &mut handoff,
                fallible_two_view_scheduler_input(
                    TimestampMicros(1_000_001),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
            );
        let connection = render_fallible_two_view_adapter_output(
            fallible_two_view_adapter_output(scheduler_result),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output = fallible_display_policy_output_for_test(
            connection,
            None,
            None,
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffDisplayDecision::NoDisplayPlaceholder {
                skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError { .. },
                ..
            }
        ));
        assert!(matches!(
            output.right,
            SwitcherTwoViewHandoffDisplayDecision::NoDisplayPlaceholder {
                skipped: SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError { .. },
                ..
            }
        ));
        assert!(!matches!(
            output.left,
            SwitcherTwoViewHandoffDisplayDecision::Update { .. }
        ));
        assert!(!matches!(
            output.right,
            SwitcherTwoViewHandoffDisplayDecision::Update { .. }
        ));
    }

    #[test]
    fn two_view_scheduler_decode_render_connection_renders_both_selected() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_102_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_102_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let scheduler_result = two_view_scheduler_result(&mut state, TimestampMicros(1_000_002));
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary::default()
            .render_scheduler_result_with_runtimes(
                SwitcherTwoViewSchedulerDecodeRenderConnectionInput {
                    scheduler_result,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 5,
                },
                &decode,
                &render_runtime,
            );

        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        let SwitcherTwoViewDecodeRenderResult::BothRendered { left, right, .. } = output.render
        else {
            panic!("both selected scheduler frames should reach decode/render");
        };
        assert_eq!(left.selected.frame.frame_id, 1);
        assert_eq!(right.selected.frame.frame_id, 2);
        let decode_inputs = decode.inputs.borrow();
        assert_eq!(decode_inputs.len(), 2);
        assert_eq!(decode_inputs[0].encoded_payload, vec![0, 0, 1, 0x65, 0x11]);
        assert_eq!(decode_inputs[1].encoded_payload, vec![0, 0, 1, 0x65, 0x12]);
        assert_eq!(render_runtime.requests.borrow().len(), 2);
    }

    #[test]
    fn two_view_scheduler_decode_render_connection_preserves_selected_and_waiting() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_103_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_103_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let scheduler_result = two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary::default()
            .render_scheduler_result_with_runtimes(
                SwitcherTwoViewSchedulerDecodeRenderConnectionInput {
                    scheduler_result,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 5,
                },
                &decode,
                &render_runtime,
            );

        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget { .. }
        ));
        let SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { left, right, .. } =
            output.render
        else {
            panic!("selected + waiting should render only selected side");
        };
        assert_eq!(left.selected.frame.frame_id, 1);
        assert_eq!(
            right,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                side: SwitcherTwoViewSide::Right,
                selection: SwitcherJitterBufferSelectionResult::FrameTooEarly {
                    client_id: ClientId("client-right".to_string()),
                    target_time: TimestampMicros(1_000_001),
                    earliest_frame_time: TimestampMicros(1_000_003),
                    frames_available: 1,
                }
            }
        );
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render_runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_scheduler_decode_render_connection_preserves_selected_and_no_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_104_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let scheduler_result = two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary::default()
            .render_scheduler_result_with_runtimes(
                SwitcherTwoViewSchedulerDecodeRenderConnectionInput {
                    scheduler_result,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 5,
                },
                &decode,
                &render_runtime,
            );

        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable { .. }
        ));
        let SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { left, right, .. } =
            output.render
        else {
            panic!("selected + no-frame should render only selected side");
        };
        assert_eq!(left.selected.frame.frame_id, 1);
        assert_eq!(
            right,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                side: SwitcherTwoViewSide::Right,
                selection: SwitcherJitterBufferSelectionResult::NoFrame {
                    client_id: ClientId("client-right".to_string()),
                    target_time: TimestampMicros(1_000_001),
                }
            }
        );
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render_runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_scheduler_decode_render_connection_waiting_no_frame_does_not_fake_render_input() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            3,
            TimestampMicros(2_105_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let scheduler_result = two_view_scheduler_result(&mut state, TimestampMicros(1_000_001));

        let output = SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary::default()
            .render_scheduler_result_with_runtimes(
                SwitcherTwoViewSchedulerDecodeRenderConnectionInput {
                    scheduler_result,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 5,
                },
                &PanicDecode,
                &PanicRender,
            );

        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable { .. }
        ));
        let SwitcherTwoViewDecodeRenderResult::BothSkipped { left, right, .. } = output.render
        else {
            panic!("waiting/no-frame should skip both sides");
        };
        assert!(matches!(
            left,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::FrameTooEarly { .. },
                ..
            }
        ));
        assert!(matches!(
            right,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
    }

    #[test]
    fn two_view_scheduler_decode_render_connection_live_like_preview_keeps_queues() {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_106_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            2,
            TimestampMicros(2_106_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            1,
            TimestampMicros(2_106_200),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x21],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_106_300),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x23],
        );
        let before_left_len = state.client_queue_len(&left_client_id);
        let before_right_len = state.client_queue_len(&right_client_id);

        let scheduler_result = two_view_scheduler_result_with_mode(
            &mut state,
            TimestampMicros(1_000_003),
            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
        );
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();
        let output = render_scheduler_result_for_test(scheduler_result, &decode, &render_runtime);

        let SwitcherTwoViewDecodeRenderResult::BothRendered { left, right, .. } = output.render
        else {
            panic!("both latest preview-selected views should render");
        };
        assert_eq!(left.selected.frame.frame_id, 2);
        assert_eq!(right.selected.frame.frame_id, 3);
        assert_eq!(decode.inputs.borrow().len(), 2);
        assert_eq!(render_runtime.requests.borrow().len(), 2);
        assert_eq!(state.client_queue_len(&left_client_id), before_left_len);
        assert_eq!(state.client_queue_len(&right_client_id), before_right_len);
    }

    #[test]
    fn two_view_scheduler_decode_render_connection_live_like_waiting_skip_is_explicit() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_107_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            2,
            TimestampMicros(2_107_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            1,
            TimestampMicros(2_107_200),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x21],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_107_300),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x23],
        );

        let scheduler_result = two_view_scheduler_result_with_mode(
            &mut state,
            TimestampMicros(1_000_002),
            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
        );
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();
        let output = render_scheduler_result_for_test(scheduler_result, &decode, &render_runtime);

        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget {
                candidate_frame_id: 3,
                ..
            }
        ));
        let SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { right, .. } =
            output.render
        else {
            panic!("waiting right side should be skipped without a fake frame");
        };
        assert!(matches!(
            right,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::FrameTooEarly {
                    earliest_frame_time: TimestampMicros(1_000_003),
                    ..
                },
                ..
            }
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render_runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_scheduler_decode_render_connection_live_like_no_frame_skip_is_explicit() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_108_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            2,
            TimestampMicros(2_108_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );

        let scheduler_result = two_view_scheduler_result_with_mode(
            &mut state,
            TimestampMicros(1_000_002),
            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
        );
        let decode = RecordingTwoViewDecode::default();
        let render_runtime = RecordingTwoViewRender::default();
        let output = render_scheduler_result_for_test(scheduler_result, &decode, &render_runtime);

        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable { .. }
        ));
        let SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { right, .. } =
            output.render
        else {
            panic!("no-frame right side should be skipped without a fake frame");
        };
        assert!(matches!(
            right,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render_runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_scheduler_decode_render_connection_live_like_consume_stays_all_or_nothing() {
        let mut state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_109_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            2,
            TimestampMicros(2_109_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            1,
            TimestampMicros(2_109_200),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x21],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_109_300),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x23],
        );

        let first = two_view_scheduler_result_with_mode(
            &mut state,
            TimestampMicros(1_000_001),
            SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
        );
        let first_decode = RecordingTwoViewDecode::default();
        let first_render = RecordingTwoViewRender::default();
        let first_output = render_scheduler_result_for_test(first, &first_decode, &first_render);
        assert!(matches!(
            first_output.render,
            SwitcherTwoViewDecodeRenderResult::BothRendered { .. }
        ));
        assert_eq!(state.client_queue_len(&left_client_id), 1);
        assert_eq!(state.client_queue_len(&right_client_id), 1);

        let waiting = two_view_scheduler_result_with_mode(
            &mut state,
            TimestampMicros(1_000_002),
            SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
        );
        let waiting_decode = RecordingTwoViewDecode::default();
        let waiting_render = RecordingTwoViewRender::default();
        let waiting_output =
            render_scheduler_result_for_test(waiting, &waiting_decode, &waiting_render);
        assert!(matches!(
            waiting_output.adapter.left,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame {
                consumed: false,
                ..
            }
        ));
        assert!(matches!(
            waiting_output.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert_eq!(waiting_decode.inputs.borrow().len(), 1);
        assert_eq!(waiting_render.requests.borrow().len(), 1);
        assert_eq!(state.client_queue_len(&left_client_id), 1);
        assert_eq!(state.client_queue_len(&right_client_id), 1);

        let final_ready = two_view_scheduler_result_with_mode(
            &mut state,
            TimestampMicros(1_000_003),
            SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
        );
        let final_decode = RecordingTwoViewDecode::default();
        let final_render = RecordingTwoViewRender::default();
        let final_output =
            render_scheduler_result_for_test(final_ready, &final_decode, &final_render);
        assert!(matches!(
            final_output.adapter.left,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { consumed: true, .. }
        ));
        assert!(matches!(
            final_output.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { consumed: true, .. }
        ));
        assert!(matches!(
            final_output.render,
            SwitcherTwoViewDecodeRenderResult::BothRendered { .. }
        ));
        assert_eq!(state.client_queue_len(&left_client_id), 0);
        assert_eq!(state.client_queue_len(&right_client_id), 0);
    }

    #[test]
    fn two_view_display_policy_updates_both_newly_rendered_frames() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_110_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_110_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let connection = render_scheduler_result_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_002)),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output =
            SwitcherTwoViewDisplayPolicyBoundary.decide(SwitcherTwoViewDisplayPolicyInput {
                connection,
                previous_left: None,
                previous_right: None,
                current_time: TimestampMicros(2_000_000),
                max_hold_duration_micros: Some(500_000),
            });

        let SwitcherTwoViewDisplayDecision::Update {
            side: left_side,
            frame: left_frame,
            rendered: left_rendered,
        } = output.left
        else {
            panic!("left should update from newly rendered frame");
        };
        let SwitcherTwoViewDisplayDecision::Update {
            side: right_side,
            frame: right_frame,
            rendered: right_rendered,
        } = output.right
        else {
            panic!("right should update from newly rendered frame");
        };
        assert_eq!(left_side, SwitcherTwoViewSide::Left);
        assert_eq!(right_side, SwitcherTwoViewSide::Right);
        assert_eq!(left_frame.displayed_at, TimestampMicros(2_000_000));
        assert_eq!(right_frame.displayed_at, TimestampMicros(2_000_000));
        assert_eq!(left_rendered.selected.frame.frame_id, 1);
        assert_eq!(right_rendered.selected.frame.frame_id, 2);
    }

    #[test]
    fn two_view_display_policy_holds_previous_for_waiting_view() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_111_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_111_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let connection = render_scheduler_result_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output =
            SwitcherTwoViewDisplayPolicyBoundary.decide(SwitcherTwoViewDisplayPolicyInput {
                connection,
                previous_left: None,
                previous_right: Some(previous_displayed_frame(
                    SwitcherTwoViewSide::Right,
                    TimestampMicros(1_900_000),
                )),
                current_time: TimestampMicros(2_000_000),
                max_hold_duration_micros: Some(500_000),
            });

        assert!(matches!(
            output.left,
            SwitcherTwoViewDisplayDecision::Update { .. }
        ));
        let SwitcherTwoViewDisplayDecision::HoldPrevious {
            side,
            frame,
            skipped,
            hold_duration_micros,
        } = output.right
        else {
            panic!("waiting right side should hold previous frame");
        };
        assert_eq!(side, SwitcherTwoViewSide::Right);
        assert_eq!(frame.displayed_at, TimestampMicros(1_900_000));
        assert_eq!(hold_duration_micros, 100_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::FrameTooEarly { .. },
                ..
            }
        ));
    }

    #[test]
    fn two_view_display_policy_holds_previous_for_no_frame_view() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_112_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let connection = render_scheduler_result_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output =
            SwitcherTwoViewDisplayPolicyBoundary.decide(SwitcherTwoViewDisplayPolicyInput {
                connection,
                previous_left: None,
                previous_right: Some(previous_displayed_frame(
                    SwitcherTwoViewSide::Right,
                    TimestampMicros(1_950_000),
                )),
                current_time: TimestampMicros(2_000_000),
                max_hold_duration_micros: Some(500_000),
            });

        assert!(matches!(
            output.left,
            SwitcherTwoViewDisplayDecision::Update { .. }
        ));
        let SwitcherTwoViewDisplayDecision::HoldPrevious {
            side,
            skipped,
            hold_duration_micros,
            ..
        } = output.right
        else {
            panic!("no-frame right side should hold previous frame");
        };
        assert_eq!(side, SwitcherTwoViewSide::Right);
        assert_eq!(hold_duration_micros, 50_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
    }

    #[test]
    fn two_view_display_policy_uses_placeholder_without_previous_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            3,
            TimestampMicros(2_113_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let connection = render_scheduler_result_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
            &PanicDecode,
            &PanicRender,
        );

        let output =
            SwitcherTwoViewDisplayPolicyBoundary.decide(SwitcherTwoViewDisplayPolicyInput {
                connection,
                previous_left: None,
                previous_right: None,
                current_time: TimestampMicros(2_000_000),
                max_hold_duration_micros: Some(500_000),
            });

        let SwitcherTwoViewDisplayDecision::NoDisplayPlaceholder {
            side: left_side,
            skipped: left_skipped,
        } = output.left
        else {
            panic!("waiting left side without previous frame should be placeholder");
        };
        let SwitcherTwoViewDisplayDecision::NoDisplayPlaceholder {
            side: right_side,
            skipped: right_skipped,
        } = output.right
        else {
            panic!("no-frame right side without previous frame should be placeholder");
        };
        assert_eq!(left_side, SwitcherTwoViewSide::Left);
        assert_eq!(right_side, SwitcherTwoViewSide::Right);
        assert!(matches!(
            left_skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::FrameTooEarly { .. },
                ..
            }
        ));
        assert!(matches!(
            right_skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
    }

    #[test]
    fn two_view_display_policy_marks_stale_previous_frame_past_max_hold() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_114_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let connection = render_scheduler_result_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let output =
            SwitcherTwoViewDisplayPolicyBoundary.decide(SwitcherTwoViewDisplayPolicyInput {
                connection,
                previous_left: None,
                previous_right: Some(previous_displayed_frame(
                    SwitcherTwoViewSide::Right,
                    TimestampMicros(1_800_000),
                )),
                current_time: TimestampMicros(2_000_000),
                max_hold_duration_micros: Some(100_000),
            });

        let SwitcherTwoViewDisplayDecision::PreviousFrameStale {
            side,
            skipped,
            hold_duration_micros,
            max_hold_duration_micros,
            ..
        } = output.right
        else {
            panic!("previous right frame should be stale past max hold duration");
        };
        assert_eq!(side, SwitcherTwoViewSide::Right);
        assert_eq!(hold_duration_micros, 200_000);
        assert_eq!(max_hold_duration_micros, 100_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
    }

    #[test]
    fn two_view_display_composition_adapter_maps_both_updates_to_decoded_inputs() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_115_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_115_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let display = display_policy_output_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_002)),
            None,
            None,
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let output = SwitcherTwoViewDisplayCompositionAdapterBoundary.adapt(
            SwitcherTwoViewDisplayCompositionAdapterInput {
                display,
                layout_policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.right,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        let SwitcherTwoViewLayoutSideInput::Decoded {
            side: left_side,
            selected: Some(left_selected),
            ..
        } = output.composition_input.left
        else {
            panic!("left update should become decoded composition input");
        };
        let SwitcherTwoViewLayoutSideInput::Decoded {
            side: right_side,
            selected: Some(right_selected),
            ..
        } = output.composition_input.right
        else {
            panic!("right update should become decoded composition input");
        };
        assert_eq!(left_side, SwitcherTwoViewSide::Left);
        assert_eq!(right_side, SwitcherTwoViewSide::Right);
        assert_eq!(left_selected.frame.frame_id, 1);
        assert_eq!(right_selected.frame.frame_id, 2);
    }

    #[test]
    fn two_view_display_composition_adapter_maps_update_and_hold_previous() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_116_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_116_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let display = display_policy_output_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
            None,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_900_000),
            )),
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let output = SwitcherTwoViewDisplayCompositionAdapterBoundary.adapt(
            SwitcherTwoViewDisplayCompositionAdapterInput {
                display,
                layout_policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );

        assert!(matches!(
            output.left,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        let SwitcherTwoViewDisplayCompositionSideInstruction::UseHeldPreviousFrame {
            skipped,
            hold_duration_micros,
            ..
        } = &output.right
        else {
            panic!("right waiting side should use held previous frame");
        };
        assert_eq!(*hold_duration_micros, 100_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::FrameTooEarly { .. },
                ..
            }
        ));
        assert!(matches!(
            output.composition_input.left,
            SwitcherTwoViewLayoutSideInput::Decoded { .. }
        ));
        let SwitcherTwoViewLayoutSideInput::Decoded {
            side,
            selected,
            frame,
        } = output.composition_input.right
        else {
            panic!("held previous frame should become decoded composition input");
        };
        assert_eq!(side, SwitcherTwoViewSide::Right);
        assert!(selected.is_none());
        assert_eq!(frame.pixels.len(), 8);
    }

    #[test]
    fn two_view_display_composition_adapter_maps_stale_previous_to_skipped_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_117_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let display = display_policy_output_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
            None,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_800_000),
            )),
            TimestampMicros(2_000_000),
            Some(100_000),
        );

        let output = SwitcherTwoViewDisplayCompositionAdapterBoundary.adapt(
            SwitcherTwoViewDisplayCompositionAdapterInput {
                display,
                layout_policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );

        let SwitcherTwoViewDisplayCompositionSideInstruction::UseStalePlaceholder {
            skipped,
            hold_duration_micros,
            max_hold_duration_micros,
            ..
        } = &output.right
        else {
            panic!("stale previous frame should stay explicit");
        };
        assert_eq!(*hold_duration_micros, 200_000);
        assert_eq!(*max_hold_duration_micros, 100_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
        assert_eq!(
            output.composition_input.right,
            SwitcherTwoViewLayoutSideInput::Skipped {
                side: SwitcherTwoViewSide::Right,
                reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
            }
        );
    }

    #[test]
    fn two_view_display_composition_adapter_maps_no_display_placeholder_to_skipped() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            3,
            TimestampMicros(2_118_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let display = display_policy_output_for_test(
            two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
            None,
            None,
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let output = SwitcherTwoViewDisplayCompositionAdapterBoundary.adapt(
            SwitcherTwoViewDisplayCompositionAdapterInput {
                display,
                layout_policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );

        let SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            skipped: left_skipped,
            ..
        } = &output.left
        else {
            panic!("waiting left without previous should stay placeholder");
        };
        let SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            skipped: right_skipped,
            ..
        } = &output.right
        else {
            panic!("no-frame right without previous should stay placeholder");
        };
        assert!(matches!(
            left_skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::FrameTooEarly { .. },
                ..
            }
        ));
        assert!(matches!(
            right_skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
        assert!(matches!(
            output.composition_input.left,
            SwitcherTwoViewLayoutSideInput::Skipped { .. }
        ));
        assert!(matches!(
            output.composition_input.right,
            SwitcherTwoViewLayoutSideInput::Skipped { .. }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_adapter_maps_both_updates_to_decoded_inputs() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_118_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_118_200),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let display = fallible_display_policy_output_for_test(
            render_fallible_two_view_adapter_output(
                fallible_two_view_adapter_output(fallible_two_view_scheduler_result(
                    &mut state,
                    TimestampMicros(1_000_002),
                )),
                &RecordingTwoViewDecode::default(),
                &RecordingTwoViewRender::default(),
            ),
            None,
            None,
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let output = handoff_display_composition_adapter_output_for_test(display);

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.composition_input.left,
            SwitcherTwoViewLayoutSideInput::Decoded { .. }
        ));
        assert!(matches!(
            output.composition_input.right,
            SwitcherTwoViewLayoutSideInput::Decoded { .. }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_adapter_maps_update_and_held_previous() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_118_300),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let display = fallible_display_policy_output_for_test(
            render_fallible_two_view_adapter_output(
                fallible_two_view_adapter_output(fallible_two_view_scheduler_result(
                    &mut state,
                    TimestampMicros(1_000_001),
                )),
                &RecordingTwoViewDecode::default(),
                &RecordingTwoViewRender::default(),
            ),
            None,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_950_000),
            )),
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let output = handoff_display_composition_adapter_output_for_test(display);

        assert!(matches!(
            output.left,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseHeldPreviousFrame {
            skipped,
            hold_duration_micros,
            ..
        } = &output.right
        else {
            panic!("right no-frame side should use held previous frame");
        };
        assert_eq!(*hold_duration_micros, 50_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
        ));
        let SwitcherTwoViewLayoutSideInput::Decoded {
            side,
            selected,
            frame,
        } = output.composition_input.right
        else {
            panic!("held previous frame should become decoded composition input");
        };
        assert_eq!(side, SwitcherTwoViewSide::Right);
        assert!(selected.is_none());
        assert_eq!(frame.pixels.len(), 8);
    }

    #[test]
    fn two_view_handoff_display_composition_adapter_preserves_source_error_detail_with_previous() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_118_400),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let scheduler_result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    SwitcherTwoViewTargetTimeSourceSchedulerInput {
                        left: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("client-left".to_string()),
                            run_id: RunId("run-left".to_string()),
                        },
                        right: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("".to_string()),
                            run_id: RunId("run-right".to_string()),
                        },
                        target_timestamp: TimestampMicros(1_000_001),
                        mode:
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    },
                )
        };
        let display = fallible_display_policy_output_for_test(
            render_fallible_two_view_adapter_output(
                fallible_two_view_adapter_output(scheduler_result),
                &RecordingTwoViewDecode::default(),
                &RecordingTwoViewRender::default(),
            ),
            None,
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_975_000),
            )),
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let output = handoff_display_composition_adapter_output_for_test(display);

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseHeldPreviousFrame {
            skipped,
            ..
        } = &output.right
        else {
            panic!("source-error hold should stay held previous");
        };
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::InvalidScope { .. },
                ..
            }
        ));
        assert!(matches!(
            output.composition_input.right,
            SwitcherTwoViewLayoutSideInput::Decoded { .. }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_adapter_maps_source_error_without_previous_to_explicit_placeholder(
    ) {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::Timeout,
        };
        let display = fallible_display_policy_output_for_test(
            render_fallible_two_view_adapter_output(
                fallible_two_view_adapter_output(
                    SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                        .select_pair_from_handoff(
                        &mut handoff,
                        fallible_two_view_scheduler_input(
                            TimestampMicros(1_000_001),
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                        ),
                    ),
                ),
                &RecordingTwoViewDecode::default(),
                &RecordingTwoViewRender::default(),
            ),
            None,
            None,
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let output = handoff_display_composition_adapter_output_for_test(display);

        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder {
            skipped: left_skipped,
            ..
        } = &output.left
        else {
            panic!("left source error without previous should stay explicit");
        };
        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder {
            skipped: right_skipped,
            ..
        } = &output.right
        else {
            panic!("right source error without previous should stay explicit");
        };
        assert!(matches!(
            left_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::Timeout,
                ..
            }
        ));
        assert!(matches!(
            right_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::Timeout,
                ..
            }
        ));
        assert!(matches!(
            output.composition_input.left,
            SwitcherTwoViewLayoutSideInput::Skipped { .. }
        ));
        assert!(matches!(
            output.composition_input.right,
            SwitcherTwoViewLayoutSideInput::Skipped { .. }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_adapter_maps_stale_previous_to_skipped_placeholder() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::SourceShutdown,
        };
        let display = fallible_display_policy_output_for_test(
            render_fallible_two_view_adapter_output(
                fallible_two_view_adapter_output(
                    SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                        .select_pair_from_handoff(
                        &mut handoff,
                        fallible_two_view_scheduler_input(
                            TimestampMicros(1_000_001),
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                        ),
                    ),
                ),
                &RecordingTwoViewDecode::default(),
                &RecordingTwoViewRender::default(),
            ),
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Left,
                TimestampMicros(1_800_000),
            )),
            Some(previous_displayed_frame(
                SwitcherTwoViewSide::Right,
                TimestampMicros(1_800_000),
            )),
            TimestampMicros(2_000_000),
            Some(100_000),
        );

        let output = handoff_display_composition_adapter_output_for_test(display);

        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseStalePlaceholder {
            skipped,
            hold_duration_micros,
            max_hold_duration_micros,
            ..
        } = &output.left
        else {
            panic!("stale previous frame should stay explicit");
        };
        assert_eq!(*hold_duration_micros, 200_000);
        assert_eq!(*max_hold_duration_micros, 100_000);
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::SourceShutdown,
                ..
            }
        ));
        assert!(matches!(
            output.composition_input.left,
            SwitcherTwoViewLayoutSideInput::Skipped { .. }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_adapter_maps_no_display_placeholder_to_skipped() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            3,
            TimestampMicros(2_118_500),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let display = fallible_display_policy_output_for_test(
            render_fallible_two_view_adapter_output(
                fallible_two_view_adapter_output(fallible_two_view_scheduler_result(
                    &mut state,
                    TimestampMicros(1_000_001),
                )),
                &PanicDecode,
                &PanicRender,
            ),
            None,
            None,
            TimestampMicros(2_000_000),
            Some(500_000),
        );

        let output = handoff_display_composition_adapter_output_for_test(display);

        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            skipped: left_skipped,
            ..
        } = &output.left
        else {
            panic!("waiting left without previous should stay placeholder");
        };
        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            skipped: right_skipped,
            ..
        } = &output.right
        else {
            panic!("no-frame right without previous should stay placeholder");
        };
        assert!(matches!(
            left_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(matches!(
            right_skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::NoFrameAvailable { .. }
        ));
        assert!(matches!(
            output.composition_input.left,
            SwitcherTwoViewLayoutSideInput::Skipped { .. }
        ));
        assert!(matches!(
            output.composition_input.right,
            SwitcherTwoViewLayoutSideInput::Skipped { .. }
        ));
    }

    #[test]
    fn two_view_display_composition_render_connection_renders_both_updated_sides() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_119_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_119_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let adapter_output =
            display_composition_adapter_output_for_test(display_policy_output_for_test(
                two_view_scheduler_result(&mut state, TimestampMicros(1_000_002)),
                None,
                None,
                TimestampMicros(2_000_000),
                Some(500_000),
            ));
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        let SwitcherTwoViewCompositionResult::BothComposed { frame } = &output.composition else {
            panic!("both updated sides should compose");
        };
        assert!(frame.left.is_some());
        assert!(frame.right.is_some());
        assert!(matches!(
            output.render,
            SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
        assert_eq!(runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_display_composition_render_connection_renders_update_and_held_previous() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_120_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            3,
            TimestampMicros(2_120_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let adapter_output =
            display_composition_adapter_output_for_test(display_policy_output_for_test(
                two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
                None,
                Some(previous_displayed_frame(
                    SwitcherTwoViewSide::Right,
                    TimestampMicros(1_900_000),
                )),
                TimestampMicros(2_000_000),
                Some(500_000),
            ));
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        let SwitcherTwoViewDisplayCompositionSideInstruction::UseHeldPreviousFrame { .. } =
            output.adapter.right
        else {
            panic!("right side should use held previous frame");
        };
        let SwitcherTwoViewCompositionResult::BothComposed { frame } = &output.composition else {
            panic!("updated + held previous should both compose");
        };
        assert_eq!(
            frame
                .left
                .as_ref()
                .and_then(|metadata| metadata.selected.as_ref())
                .map(|selected| selected.frame.frame_id),
            Some(1)
        );
        assert!(frame
            .right
            .as_ref()
            .and_then(|metadata| metadata.selected.as_ref())
            .is_none());
        assert!(matches!(
            output.render,
            SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
    }

    #[test]
    fn two_view_display_composition_render_connection_keeps_stale_placeholder_explicit() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_121_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let adapter_output =
            display_composition_adapter_output_for_test(display_policy_output_for_test(
                two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
                None,
                Some(previous_displayed_frame(
                    SwitcherTwoViewSide::Right,
                    TimestampMicros(1_800_000),
                )),
                TimestampMicros(2_000_000),
                Some(100_000),
            ));
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseStalePlaceholder { .. }
        ));
        assert_eq!(
            output.adapter.composition_input.right,
            SwitcherTwoViewLayoutSideInput::Skipped {
                side: SwitcherTwoViewSide::Right,
                reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
            }
        );
        let SwitcherTwoViewCompositionResult::LeftOnly {
            right_placeholder_reason,
            frame,
        } = &output.composition
        else {
            panic!("left renderable + right stale placeholder should compose left only");
        };
        assert_eq!(
            *right_placeholder_reason,
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        );
        assert!(frame.right.is_none());
        assert!(matches!(
            output.render,
            SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
        assert_eq!(runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_display_composition_render_connection_keeps_no_display_placeholder_explicit() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            3,
            TimestampMicros(2_122_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x13],
        );
        let adapter_output =
            display_composition_adapter_output_for_test(display_policy_output_for_test(
                two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
                None,
                None,
                TimestampMicros(2_000_000),
                Some(500_000),
            ));
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder { .. }
        ));
        assert!(matches!(
            output.composition,
            SwitcherTwoViewCompositionResult::EmptyPlaceholder { .. }
        ));
        assert_eq!(
            output.render,
            SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::NoRenderableCanvas {
                left_reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
                right_reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
            }
        );
        assert!(runtime.requests.borrow().is_empty());
    }

    #[test]
    fn two_view_display_composition_render_connection_preserves_mixed_render_and_placeholder_detail(
    ) {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_123_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let adapter_output =
            display_composition_adapter_output_for_test(display_policy_output_for_test(
                two_view_scheduler_result(&mut state, TimestampMicros(1_000_001)),
                None,
                None,
                TimestampMicros(2_000_000),
                Some(500_000),
            ));
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        let SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            skipped,
            ..
        } = &output.adapter.right
        else {
            panic!("right side should keep no-display placeholder detail");
        };
        assert!(matches!(
            skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
        let SwitcherTwoViewCompositionResult::LeftOnly {
            frame,
            right_placeholder_reason,
        } = &output.composition
        else {
            panic!("left renderable + right placeholder should compose left only");
        };
        assert!(frame.left.is_some());
        assert!(frame.right.is_none());
        assert_eq!(
            *right_placeholder_reason,
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        );
        assert!(matches!(
            output.render,
            SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_render_connection_renders_both_updated_sides() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_125_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_125_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let adapter_output = handoff_display_composition_adapter_output_for_test(
            fallible_display_policy_output_for_test(
                render_fallible_two_view_adapter_output(
                    fallible_two_view_adapter_output(fallible_two_view_scheduler_result(
                        &mut state,
                        TimestampMicros(1_000_002),
                    )),
                    &RecordingTwoViewDecode::default(),
                    &RecordingTwoViewRender::default(),
                ),
                None,
                None,
                TimestampMicros(2_000_000),
                Some(500_000),
            ),
        );
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "fallible composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.composition,
            SwitcherTwoViewCompositionResult::BothComposed { .. }
        ));
        assert!(matches!(
            output.render,
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
        assert_eq!(runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_handoff_display_composition_render_connection_renders_update_and_held_previous() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_126_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let adapter_output = handoff_display_composition_adapter_output_for_test(
            fallible_display_policy_output_for_test(
                render_fallible_two_view_adapter_output(
                    fallible_two_view_adapter_output(fallible_two_view_scheduler_result(
                        &mut state,
                        TimestampMicros(1_000_001),
                    )),
                    &RecordingTwoViewDecode::default(),
                    &RecordingTwoViewRender::default(),
                ),
                None,
                Some(previous_displayed_frame(
                    SwitcherTwoViewSide::Right,
                    TimestampMicros(1_950_000),
                )),
                TimestampMicros(2_000_000),
                Some(500_000),
            ),
        );
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "fallible composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseHeldPreviousFrame { .. }
        ));
        let SwitcherTwoViewCompositionResult::BothComposed { frame } = &output.composition else {
            panic!("updated + held previous should compose both sides");
        };
        assert!(frame.left.is_some());
        assert!(frame.right.is_some());
        assert!(matches!(
            output.render,
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_render_connection_preserves_stale_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_127_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let adapter_output = handoff_display_composition_adapter_output_for_test(
            fallible_display_policy_output_for_test(
                render_fallible_two_view_adapter_output(
                    fallible_two_view_adapter_output(fallible_two_view_scheduler_result(
                        &mut state,
                        TimestampMicros(1_000_001),
                    )),
                    &RecordingTwoViewDecode::default(),
                    &RecordingTwoViewRender::default(),
                ),
                None,
                Some(previous_displayed_frame(
                    SwitcherTwoViewSide::Right,
                    TimestampMicros(1_800_000),
                )),
                TimestampMicros(2_000_000),
                Some(100_000),
            ),
        );
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "fallible composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseStalePlaceholder { .. }
        ));
        let SwitcherTwoViewCompositionResult::LeftOnly {
            frame,
            right_placeholder_reason,
        } = &output.composition
        else {
            panic!("left update + stale right should compose left only");
        };
        assert!(frame.right.is_none());
        assert_eq!(
            *right_placeholder_reason,
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        );
        assert!(matches!(
            output.render,
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_render_connection_preserves_no_display_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_128_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let adapter_output = handoff_display_composition_adapter_output_for_test(
            fallible_display_policy_output_for_test(
                render_fallible_two_view_adapter_output(
                    fallible_two_view_adapter_output(fallible_two_view_scheduler_result(
                        &mut state,
                        TimestampMicros(1_000_001),
                    )),
                    &RecordingTwoViewDecode::default(),
                    &RecordingTwoViewRender::default(),
                ),
                None,
                None,
                TimestampMicros(2_000_000),
                Some(500_000),
            ),
        );
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "fallible composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseNoDisplayPlaceholder { .. }
        ));
        let SwitcherTwoViewCompositionResult::LeftOnly {
            frame,
            right_placeholder_reason,
        } = &output.composition
        else {
            panic!("left update + no-display right should compose left only");
        };
        assert!(frame.right.is_none());
        assert_eq!(
            *right_placeholder_reason,
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        );
        assert!(matches!(
            output.render,
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_render_connection_preserves_source_error_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_129_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x11],
        );
        let scheduler_result = {
            let mut handoff = SwitcherInProcessQueuedFrameHandoff::new(&mut state);
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                .select_pair_from_handoff(
                    &mut handoff,
                    SwitcherTwoViewTargetTimeSourceSchedulerInput {
                        left: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("client-left".to_string()),
                            run_id: RunId("run-left".to_string()),
                        },
                        right: SwitcherTwoViewTargetTimeSourceViewConfig {
                            client_id: ClientId("".to_string()),
                            run_id: RunId("run-right".to_string()),
                        },
                        target_timestamp: TimestampMicros(1_000_001),
                        mode:
                            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    },
                )
        };
        let adapter_output = handoff_display_composition_adapter_output_for_test(
            fallible_display_policy_output_for_test(
                render_fallible_two_view_adapter_output(
                    fallible_two_view_adapter_output(scheduler_result),
                    &RecordingTwoViewDecode::default(),
                    &RecordingTwoViewRender::default(),
                ),
                None,
                None,
                TimestampMicros(2_000_000),
                Some(500_000),
            ),
        );
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "fallible composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder {
            skipped,
            ..
        } = &output.adapter.right
        else {
            panic!("right source error should remain a source-error placeholder");
        };
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError {
                error: SwitcherQueuedFrameHandoffError::InvalidScope { .. },
                ..
            }
        ));
        let SwitcherTwoViewCompositionResult::LeftOnly {
            frame,
            right_placeholder_reason,
        } = &output.composition
        else {
            panic!("left update + source-error right should compose left only");
        };
        assert!(frame.right.is_none());
        assert_eq!(
            *right_placeholder_reason,
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        );
        assert!(matches!(
            output.render,
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
    }

    #[test]
    fn two_view_handoff_display_composition_render_connection_does_not_render_both_source_errors() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
        };
        let adapter_output =
            handoff_display_composition_adapter_output_for_test(fallible_display_policy_output_for_test(
                render_fallible_two_view_adapter_output(
                    fallible_two_view_adapter_output(
                        SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary::default()
                            .select_pair_from_handoff(
                                &mut handoff,
                                fallible_two_view_scheduler_input(
                                    TimestampMicros(1_000_001),
                                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                                ),
                            ),
                    ),
                    &RecordingTwoViewDecode::default(),
                    &RecordingTwoViewRender::default(),
                ),
                None,
                None,
                TimestampMicros(2_000_000),
                Some(500_000),
            ));
        let runtime = RecordingTwoViewRender::default();

        let output = SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary::default()
            .render_adapter_output_with_runtime(
                SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput {
                    adapter_output,
                    window_title: "fallible composed".to_string(),
                    render_hold_millis: 25,
                },
                &runtime,
            );

        assert_eq!(
            output.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder { .. }
        ));
        assert!(matches!(
            output.composition,
            SwitcherTwoViewCompositionResult::EmptyPlaceholder { .. }
        ));
        assert_eq!(
            output.render,
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::NoRenderableCanvas {
                left_reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
                right_reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
            }
        );
        assert!(runtime.requests.borrow().is_empty());
    }

    fn server_mediated_validation_input(
        target_timestamp: TimestampMicros,
        scheduler_mode: SwitcherTwoViewTargetTimeSourceSchedulerMode,
    ) -> SwitcherServerMediatedTwoViewValidationInput {
        SwitcherServerMediatedTwoViewValidationInput {
            left: SwitcherTwoViewTargetTimeSourceViewConfig {
                client_id: ClientId("client-left".to_string()),
                run_id: RunId("run-left".to_string()),
            },
            right: SwitcherTwoViewTargetTimeSourceViewConfig {
                client_id: ClientId("client-right".to_string()),
                run_id: RunId("run-right".to_string()),
            },
            target_timestamp,
            scheduler_mode,
            left_window_title: "left decoded".to_string(),
            right_window_title: "right decoded".to_string(),
            decode_render_hold_millis: 5,
            previous_left: None,
            previous_right: None,
            display_current_time: TimestampMicros(2_000_000),
            max_hold_duration_micros: Some(500_000),
            layout_policy: SwitcherTwoViewLayoutPolicy::default(),
            composed_window_title: "composed server mediated".to_string(),
            composed_render_hold_millis: 25,
        }
    }

    #[test]
    fn server_mediated_two_view_validation_renders_two_eligible_server_queue_frames() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_124_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x21],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_124_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x22],
        );
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default().run_with_runtimes(
            &mut state,
            server_mediated_validation_input(
                TimestampMicros(1_000_002),
                SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            ),
            &decode,
            &render,
        );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        assert!(matches!(
            output.display.left,
            SwitcherTwoViewDisplayDecision::Update { .. }
        ));
        assert!(matches!(
            output.display.right,
            SwitcherTwoViewDisplayDecision::Update { .. }
        ));
        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.render.composition,
            SwitcherTwoViewCompositionResult::BothComposed { .. }
        ));
        assert!(matches!(
            output.render.render,
            SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
        assert_eq!(decode.inputs.borrow().len(), 2);
        assert_eq!(render.requests.borrow().len(), 3);
    }

    #[test]
    fn server_mediated_two_view_validation_runs_over_queued_frame_source_abstraction() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_124_200),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x31],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_124_300),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x32],
        );
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();
        let output = {
            let mut source = SwitcherInProcessServerQueueFrameSource::new(&mut state);
            SwitcherServerMediatedTwoViewValidationBoundary::default()
                .run_from_source_with_runtimes(
                    &mut source,
                    server_mediated_validation_input(
                        TimestampMicros(1_000_002),
                        SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                    ),
                    &decode,
                    &render,
                )
        };

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        assert!(matches!(
            output.decode_render.adapter.left,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert!(matches!(
            output.decode_render.adapter.right,
            SwitcherTwoViewSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.render.render,
            SwitcherTwoViewDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
        assert_eq!(state.total_len(), 2);
    }

    #[test]
    fn server_mediated_two_view_validation_preserves_waiting_without_fake_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_125_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x21],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            10,
            TimestampMicros(2_125_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x2a],
        );
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default().run_with_runtimes(
            &mut state,
            server_mediated_validation_input(
                TimestampMicros(1_000_005),
                SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            ),
            &decode,
            &render,
        );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::PartialSelected
        );
        assert!(matches!(
            output.scheduler.right,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        let SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            skipped,
            ..
        } = &output.adapter.right
        else {
            panic!("waiting side should remain an explicit no-display placeholder");
        };
        assert!(matches!(
            skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::FrameTooEarly { .. },
                ..
            }
        ));
        assert!(matches!(
            output.render.composition,
            SwitcherTwoViewCompositionResult::LeftOnly { .. }
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render.requests.borrow().len(), 2);
    }

    #[test]
    fn server_mediated_two_view_validation_preserves_no_frame_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_126_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x21],
        );
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default().run_with_runtimes(
            &mut state,
            server_mediated_validation_input(
                TimestampMicros(1_000_005),
                SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            ),
            &decode,
            &render,
        );

        assert!(matches!(
            output.scheduler.right,
            SwitcherSingleClientTargetTimeSourceResult::NoFrameAvailable { .. }
        ));
        let SwitcherTwoViewDisplayCompositionSideInstruction::UseNoDisplayPlaceholder {
            skipped,
            ..
        } = &output.adapter.right
        else {
            panic!("empty queue side should remain a no-display placeholder");
        };
        assert!(matches!(
            skipped,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                selection: SwitcherJitterBufferSelectionResult::NoFrame { .. },
                ..
            }
        ));
        assert!(matches!(
            output.render.composition,
            SwitcherTwoViewCompositionResult::LeftOnly { .. }
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render.requests.borrow().len(), 2);
    }

    #[test]
    fn server_mediated_two_view_validation_consume_mode_is_all_or_nothing() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_127_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x21],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            10,
            TimestampMicros(2_127_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x2a],
        );
        let left_client = ClientId("client-left".to_string());
        let right_client = ClientId("client-right".to_string());
        let before_left = state.client_queue_len(&left_client);
        let before_right = state.client_queue_len(&right_client);
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default().run_with_runtimes(
            &mut state,
            server_mediated_validation_input(
                TimestampMicros(1_000_005),
                SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
            ),
            &decode,
            &render,
        );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::PartialSelected
        );
        let SwitcherSingleClientTargetTimeSourceResult::Selected(left) = &output.scheduler.left
        else {
            panic!("left should be selected from preview");
        };
        assert!(!left.consumed);
        assert!(matches!(
            output.scheduler.right,
            SwitcherSingleClientTargetTimeSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert_eq!(state.client_queue_len(&left_client), before_left);
        assert_eq!(state.client_queue_len(&right_client), before_right);
    }

    #[test]
    fn server_mediated_two_view_validation_preview_mode_does_not_mutate_server_queues() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_128_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x21],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_128_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x22],
        );
        let left_client = ClientId("client-left".to_string());
        let right_client = ClientId("client-right".to_string());
        let before_left = state.client_queue_len(&left_client);
        let before_right = state.client_queue_len(&right_client);
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default().run_with_runtimes(
            &mut state,
            server_mediated_validation_input(
                TimestampMicros(1_000_005),
                SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
            ),
            &decode,
            &render,
        );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeSourceSchedulerStatus::AllSelected
        );
        assert_eq!(state.client_queue_len(&left_client), before_left);
        assert_eq!(state.client_queue_len(&right_client), before_right);
    }

    #[test]
    fn server_mediated_two_view_handoff_validation_renders_two_eligible_server_queue_frames() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_129_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x41],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_129_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x42],
        );
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default()
            .run_fallible_with_runtimes(
                &mut state,
                server_mediated_validation_input(
                    TimestampMicros(1_000_002),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
                &decode,
                &render,
            );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        assert!(matches!(
            output.decode_render_adapter.left,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::RenderFrame { .. }
        ));
        assert!(matches!(
            output.decode_render.render,
            SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothRendered { .. }
        ));
        assert!(matches!(
            output.display.left,
            SwitcherTwoViewHandoffDisplayDecision::Update { .. }
        ));
        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseUpdatedFrame { .. }
        ));
        assert!(matches!(
            output.render.composition,
            SwitcherTwoViewCompositionResult::BothComposed { .. }
        ));
        assert!(matches!(
            output.render.render,
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::RenderedCanvas {
                render: SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
            }
        ));
        assert_eq!(decode.inputs.borrow().len(), 2);
        assert_eq!(render.requests.borrow().len(), 3);
    }

    #[test]
    fn server_mediated_two_view_handoff_validation_preserves_waiting_without_fake_frame() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_130_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x41],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            10,
            TimestampMicros(2_130_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x4a],
        );
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default()
            .run_fallible_with_runtimes(
                &mut state,
                server_mediated_validation_input(
                    TimestampMicros(1_000_005),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
                &decode,
                &render,
            );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::PartialSelected
        );
        assert!(matches!(
            output.scheduler.right,
            SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(matches!(
            output.decode_render_adapter.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipWaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert!(matches!(
            output.display.right,
            SwitcherTwoViewHandoffDisplayDecision::NoDisplayPlaceholder { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseNoDisplayPlaceholder { .. }
        ));
        assert!(matches!(
            output.render.composition,
            SwitcherTwoViewCompositionResult::LeftOnly { .. }
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render.requests.borrow().len(), 2);
    }

    #[test]
    fn server_mediated_two_view_handoff_validation_preserves_no_frame_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_131_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x41],
        );
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default()
            .run_fallible_with_runtimes(
                &mut state,
                server_mediated_validation_input(
                    TimestampMicros(1_000_005),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
                &decode,
                &render,
            );

        assert!(matches!(
            output.scheduler.right,
            SwitcherSingleClientTargetTimeHandoffSourceResult::NoFrameAvailable { .. }
        ));
        assert!(matches!(
            output.decode_render_adapter.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipNoFrameAvailable { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseNoDisplayPlaceholder { .. }
        ));
        assert!(matches!(
            output.render.composition,
            SwitcherTwoViewCompositionResult::LeftOnly { .. }
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render.requests.borrow().len(), 2);
    }

    #[test]
    fn server_mediated_two_view_handoff_validation_preserves_source_error_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_132_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x41],
        );
        let mut input = server_mediated_validation_input(
            TimestampMicros(1_000_005),
            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
        );
        input.right.client_id = ClientId("".to_string());
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default()
            .run_fallible_with_runtimes(&mut state, input, &decode, &render);

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            output.scheduler.right,
            SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError { .. }
        ));
        assert!(matches!(
            output.decode_render_adapter.right,
            SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction::SkipHandoffError {
                error: SwitcherQueuedFrameHandoffError::InvalidScope { .. },
                ..
            }
        ));
        assert!(matches!(
            output.display.right,
            SwitcherTwoViewHandoffDisplayDecision::NoDisplayPlaceholder { .. }
        ));
        let SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder {
            skipped,
            ..
        } = &output.adapter.right
        else {
            panic!("handoff error should remain source-error placeholder");
        };
        assert!(matches!(
            skipped,
            SwitcherTwoViewHandoffDecodeRenderSkippedSide::HandoffError { .. }
        ));
        assert_eq!(
            output.render.scheduler_status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            output.render.composition,
            SwitcherTwoViewCompositionResult::LeftOnly { .. }
        ));
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render.requests.borrow().len(), 2);
    }

    #[test]
    fn server_mediated_two_view_handoff_validation_does_not_render_both_handoff_errors() {
        let mut handoff = FailingQueuedFrameHandoff {
            error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
        };
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default()
            .run_fallible_from_handoff_with_runtimes(
                &mut handoff,
                server_mediated_validation_input(
                    TimestampMicros(1_000_005),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
                &decode,
                &render,
            );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::HandoffError
        );
        assert!(matches!(
            output.decode_render.render,
            SwitcherTwoViewHandoffDecodeRenderConnectionResult::BothSkipped { .. }
        ));
        assert!(matches!(
            output.adapter.left,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder { .. }
        ));
        assert!(matches!(
            output.adapter.right,
            SwitcherTwoViewHandoffDisplayCompositionSideInstruction::UseSourceErrorPlaceholder { .. }
        ));
        assert!(matches!(
            output.render.composition,
            SwitcherTwoViewCompositionResult::EmptyPlaceholder { .. }
        ));
        assert_eq!(
            output.render.render,
            SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult::NoRenderableCanvas {
                left_reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
                right_reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
            }
        );
        assert!(decode.inputs.borrow().is_empty());
        assert!(render.requests.borrow().is_empty());
    }

    #[test]
    fn server_mediated_two_view_handoff_validation_consume_mode_is_all_or_nothing() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_133_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x41],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            10,
            TimestampMicros(2_133_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x4a],
        );
        let left_client = ClientId("client-left".to_string());
        let right_client = ClientId("client-right".to_string());
        let before_left = state.client_queue_len(&left_client);
        let before_right = state.client_queue_len(&right_client);
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default()
            .run_fallible_with_runtimes(
            &mut state,
            server_mediated_validation_input(
                TimestampMicros(1_000_005),
                SwitcherTwoViewTargetTimeSourceSchedulerMode::ConsumeOldestAtOrBeforeAllSelected,
            ),
            &decode,
            &render,
        );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::PartialSelected
        );
        let SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(left) =
            &output.scheduler.left
        else {
            panic!("left should be selected from preview");
        };
        assert!(!left.consumed);
        assert!(matches!(
            output.scheduler.right,
            SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget { .. }
        ));
        assert_eq!(state.client_queue_len(&left_client), before_left);
        assert_eq!(state.client_queue_len(&right_client), before_right);
    }

    #[test]
    fn server_mediated_two_view_handoff_validation_preview_mode_does_not_mutate_server_queues() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_run_payload(
            &mut state,
            "client-left",
            "run-left",
            1,
            TimestampMicros(2_134_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x41],
        );
        store_frame_with_run_payload(
            &mut state,
            "client-right",
            "run-right",
            2,
            TimestampMicros(2_134_100),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x42],
        );
        let left_client = ClientId("client-left".to_string());
        let right_client = ClientId("client-right".to_string());
        let before_left = state.client_queue_len(&left_client);
        let before_right = state.client_queue_len(&right_client);
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let output = SwitcherServerMediatedTwoViewValidationBoundary::default()
            .run_fallible_with_runtimes(
                &mut state,
                server_mediated_validation_input(
                    TimestampMicros(1_000_005),
                    SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
                ),
                &decode,
                &render,
            );

        assert_eq!(
            output.scheduler.status,
            SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus::AllSelected
        );
        assert_eq!(state.client_queue_len(&left_client), before_left);
        assert_eq!(state.client_queue_len(&right_client), before_right);
    }

    #[test]
    fn placeholder_display_handoff_preserves_metadata_and_payload_length() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 7, TimestampMicros(2_100_000));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleViewPlaceholderPathBoundary::default()
            .prepare_latest_display_handoff(SwitcherSingleViewFrameSelectionInput {
                queue_state: &state,
                client_id: &client_id,
            });

        let SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(handoff) = result
        else {
            panic!("placeholder display handoff should be available");
        };
        assert_eq!(
            handoff.decode_status,
            SwitcherSingleViewDecodeStatus::DeferredPlaceholder
        );
        assert_eq!(handoff.selected.client_id, client_id);
        assert_eq!(handoff.selected.frame_id, 7);
        assert_eq!(
            handoff.selected.capture_timestamp,
            TimestampMicros(1_000_007)
        );
        assert_eq!(handoff.selected.send_timestamp, TimestampMicros(1_000_107));
        assert_eq!(handoff.selected.width, 1280);
        assert_eq!(handoff.selected.height, 720);
        assert_eq!(handoff.selected.fps_nominal, 30);
        assert_eq!(handoff.selected.encoded_payload_len, 3);
        assert_eq!(handoff.selected.encoded_payload, vec![0x07, 0xbb, 0xcc]);
    }

    #[test]
    fn single_view_queue_read_does_not_mutate_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 1, TimestampMicros(2_200_000));
        store_frame(&mut state, "client-1", 2, TimestampMicros(2_200_100));
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let _result = SwitcherSingleViewPlaceholderPathBoundary::default()
            .prepare_latest_display_handoff(SwitcherSingleViewFrameSelectionInput {
                queue_state: &state,
                client_id: &client_id,
            });

        assert_eq!(state.client_queue_len(&client_id), before_len);
        let frame_ids: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(frame_ids, vec![1, 2]);
    }

    #[test]
    fn jitter_buffer_selects_frame_closest_to_target_time_within_policy() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 1, TimestampMicros(2_210_000));
        store_frame(&mut state, "client-1", 10, TimestampMicros(2_210_010));
        store_frame(&mut state, "client-1", 30, TimestampMicros(2_210_030));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherJitterBufferSelectionBoundary::default().select_frame(
            SwitcherJitterBufferSelectionInput {
                queue_state: &state,
                client_id: &client_id,
                current_switcher_time: TimestampMicros(1_600_012),
                policy: SwitcherJitterBufferSelectionPolicy {
                    playout_delay_micros: 600_000,
                    clock_offset_micros: None,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    min_buffer_frames: 1,
                },
            },
        );

        let SwitcherJitterBufferSelectionResult::Selected(selected) = result else {
            panic!("frame near targetTime should be selected");
        };
        assert_eq!(selected.target_time, TimestampMicros(1_000_012));
        assert_eq!(selected.frame.frame_id, 10);
        assert_eq!(
            selected.adjusted_capture_timestamp,
            TimestampMicros(1_000_010)
        );
        assert_eq!(selected.delta_from_target_micros, -2);
    }

    #[test]
    fn jitter_buffer_waits_when_buffer_is_insufficient() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 1, TimestampMicros(2_220_000));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherJitterBufferSelectionBoundary::default().select_frame(
            SwitcherJitterBufferSelectionInput {
                queue_state: &state,
                client_id: &client_id,
                current_switcher_time: TimestampMicros(1_600_000),
                policy: SwitcherJitterBufferSelectionPolicy {
                    min_buffer_frames: 2,
                    ..SwitcherJitterBufferSelectionPolicy::default()
                },
            },
        );

        assert_eq!(
            result,
            SwitcherJitterBufferSelectionResult::WaitingForBuffer {
                client_id,
                target_time: TimestampMicros(1_100_000),
                available_frames: 1,
                min_buffer_frames: 2
            }
        );
    }

    #[test]
    fn jitter_buffer_reports_no_frame_explicitly() {
        let state = ServerVideoFrameQueueState::default();
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherJitterBufferSelectionBoundary::default().select_frame(
            SwitcherJitterBufferSelectionInput {
                queue_state: &state,
                client_id: &client_id,
                current_switcher_time: TimestampMicros(1_600_000),
                policy: SwitcherJitterBufferSelectionPolicy::default(),
            },
        );

        assert_eq!(
            result,
            SwitcherJitterBufferSelectionResult::NoFrame {
                client_id,
                target_time: TimestampMicros(1_100_000)
            }
        );
    }

    #[test]
    fn jitter_buffer_reports_too_early_frame_explicitly() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 20, TimestampMicros(2_230_000));
        store_frame(&mut state, "client-1", 21, TimestampMicros(2_230_010));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherJitterBufferSelectionBoundary::default().select_frame(
            SwitcherJitterBufferSelectionInput {
                queue_state: &state,
                client_id: &client_id,
                current_switcher_time: TimestampMicros(1_500_000),
                policy: SwitcherJitterBufferSelectionPolicy {
                    playout_delay_micros: 600_000,
                    clock_offset_micros: None,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    min_buffer_frames: 1,
                },
            },
        );

        assert_eq!(
            result,
            SwitcherJitterBufferSelectionResult::FrameTooEarly {
                client_id,
                target_time: TimestampMicros(900_000),
                earliest_frame_time: TimestampMicros(1_000_020),
                frames_available: 2
            }
        );
    }

    #[test]
    fn jitter_buffer_reports_late_frames_to_drop_explicitly() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 1, TimestampMicros(2_240_000));
        store_frame(&mut state, "client-1", 2, TimestampMicros(2_240_010));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherJitterBufferSelectionBoundary::default().select_frame(
            SwitcherJitterBufferSelectionInput {
                queue_state: &state,
                client_id: &client_id,
                current_switcher_time: TimestampMicros(2_000_000),
                policy: SwitcherJitterBufferSelectionPolicy {
                    playout_delay_micros: 500_000,
                    clock_offset_micros: None,
                    max_late_micros: 100,
                    max_early_micros: 0,
                    min_buffer_frames: 1,
                },
            },
        );

        let SwitcherJitterBufferSelectionResult::FrameTooLateDropped {
            target_time,
            latest_frame_time,
            late_frames,
            ..
        } = result
        else {
            panic!("old frames should be reported late");
        };
        assert_eq!(target_time, TimestampMicros(1_500_000));
        assert_eq!(latest_frame_time, TimestampMicros(1_000_002));
        assert_eq!(
            late_frames
                .iter()
                .map(|frame| frame.frame_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn jitter_buffer_preserves_encoded_payload_and_metadata_without_decode_or_render() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_payload(
            &mut state,
            "client-1",
            77,
            TimestampMicros(2_250_000),
            640,
            360,
            vec![0, 0, 1, 0x65, 0xaa],
        );
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherJitterBufferSelectionBoundary::default().select_frame(
            SwitcherJitterBufferSelectionInput {
                queue_state: &state,
                client_id: &client_id,
                current_switcher_time: TimestampMicros(1_600_077),
                policy: SwitcherJitterBufferSelectionPolicy {
                    playout_delay_micros: 600_000,
                    clock_offset_micros: None,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    min_buffer_frames: 1,
                },
            },
        );

        let SwitcherJitterBufferSelectionResult::Selected(selected) = result else {
            panic!("fixture frame should be selected");
        };
        assert_eq!(selected.frame.client_id, client_id);
        assert_eq!(selected.frame.frame_id, 77);
        assert_eq!(selected.frame.width, 640);
        assert_eq!(selected.frame.height, 360);
        assert_eq!(selected.frame.encoded_payload_len, 5);
        assert_eq!(selected.frame.encoded_payload, vec![0, 0, 1, 0x65, 0xaa]);
    }

    #[test]
    fn two_view_target_time_selects_both_clients_against_shared_target() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_payload(
            &mut state,
            "client-left",
            10,
            TimestampMicros(2_260_000),
            640,
            360,
            vec![0, 0, 1, 0x65, 0x10],
        );
        store_frame_with_payload(
            &mut state,
            "client-right",
            12,
            TimestampMicros(2_260_010),
            640,
            360,
            vec![0, 0, 1, 0x65, 0x12],
        );
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewTargetTimeSelectionBoundary::default().select_pair(
            SwitcherTwoViewTargetTimeSelectionInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_011),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
            },
        );

        let SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
            shared_target_time,
            left,
            right,
        } = result
        else {
            panic!("both clients should select frames against the shared targetTime");
        };
        assert_eq!(shared_target_time, TimestampMicros(1_000_011));
        assert_eq!(left.target_time, shared_target_time);
        assert_eq!(right.target_time, shared_target_time);
        assert_eq!(left.frame.frame_id, 10);
        assert_eq!(right.frame.frame_id, 12);
        assert_eq!(left.frame.encoded_payload, vec![0, 0, 1, 0x65, 0x10]);
        assert_eq!(right.frame.encoded_payload, vec![0, 0, 1, 0x65, 0x12]);
    }

    #[test]
    fn two_view_target_time_reports_partial_when_one_client_is_waiting() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-left", 20, TimestampMicros(2_270_000));
        store_frame(&mut state, "client-left", 21, TimestampMicros(2_270_010));
        store_frame(&mut state, "client-right", 20, TimestampMicros(2_270_020));
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewTargetTimeSelectionBoundary::default().select_pair(
            SwitcherTwoViewTargetTimeSelectionInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_020),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    min_buffer_frames: 2,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
            },
        );

        let SwitcherTwoViewTargetTimeSelectionResult::Partial {
            shared_target_time,
            left,
            right,
        } = result
        else {
            panic!("one selected side and one waiting side should be partial");
        };
        assert_eq!(shared_target_time, TimestampMicros(1_000_020));
        assert!(matches!(
            left,
            SwitcherJitterBufferSelectionResult::Selected(_)
        ));
        assert_eq!(
            right,
            SwitcherJitterBufferSelectionResult::WaitingForBuffer {
                client_id: right_client_id,
                target_time: shared_target_time,
                available_frames: 1,
                min_buffer_frames: 2
            }
        );
    }

    #[test]
    fn two_view_target_time_reports_partial_when_one_client_is_too_early() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-left", 20, TimestampMicros(2_280_000));
        store_frame(&mut state, "client-right", 220, TimestampMicros(2_280_010));
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewTargetTimeSelectionBoundary::default().select_pair(
            SwitcherTwoViewTargetTimeSelectionInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_020),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
            },
        );

        let SwitcherTwoViewTargetTimeSelectionResult::Partial {
            shared_target_time,
            left,
            right,
        } = result
        else {
            panic!("one selected side and one too-early side should be partial");
        };
        assert_eq!(shared_target_time, TimestampMicros(1_000_020));
        assert!(matches!(
            left,
            SwitcherJitterBufferSelectionResult::Selected(_)
        ));
        assert_eq!(
            right,
            SwitcherJitterBufferSelectionResult::FrameTooEarly {
                client_id: right_client_id,
                target_time: shared_target_time,
                earliest_frame_time: TimestampMicros(1_000_220),
                frames_available: 1
            }
        );
    }

    #[test]
    fn two_view_target_time_reports_partial_when_one_client_is_too_late() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(
            &mut state,
            "client-left",
            100_020,
            TimestampMicros(2_290_000),
        );
        store_frame(&mut state, "client-right", 1, TimestampMicros(2_290_010));
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewTargetTimeSelectionBoundary::default().select_pair(
            SwitcherTwoViewTargetTimeSelectionInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_700_020),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
            },
        );

        let SwitcherTwoViewTargetTimeSelectionResult::Partial { right, .. } = result else {
            panic!("one selected side and one too-late side should be partial");
        };
        let SwitcherJitterBufferSelectionResult::FrameTooLateDropped {
            client_id,
            target_time,
            latest_frame_time,
            late_frames,
        } = right
        else {
            panic!("right side should be too late");
        };
        assert_eq!(client_id, right_client_id);
        assert_eq!(target_time, TimestampMicros(1_100_020));
        assert_eq!(latest_frame_time, TimestampMicros(1_000_001));
        assert_eq!(late_frames.len(), 1);
        assert_eq!(late_frames[0].frame_id, 1);
    }

    #[test]
    fn two_view_target_time_applies_per_client_offsets_independently() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-left", 100, TimestampMicros(2_300_000));
        store_frame(&mut state, "client-right", 0, TimestampMicros(2_300_010));
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewTargetTimeSelectionBoundary::default().select_pair(
            SwitcherTwoViewTargetTimeSelectionInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_100),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    right_clock_offset_micros: Some(100),
                    max_late_micros: 10,
                    max_early_micros: 10,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
            },
        );

        let SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
            shared_target_time,
            left,
            right,
        } = result
        else {
            panic!("right client offset should align both frames to targetTime");
        };
        assert_eq!(shared_target_time, TimestampMicros(1_000_100));
        assert_eq!(left.adjusted_capture_timestamp, TimestampMicros(1_000_100));
        assert_eq!(right.adjusted_capture_timestamp, TimestampMicros(1_000_100));
        assert_eq!(left.frame.frame_id, 100);
        assert_eq!(right.frame.frame_id, 0);
    }

    #[test]
    fn two_view_target_time_reports_both_unavailable_explicitly() {
        let state = ServerVideoFrameQueueState::default();
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewTargetTimeSelectionBoundary::default().select_pair(
            SwitcherTwoViewTargetTimeSelectionInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_000),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
            },
        );

        assert_eq!(
            result,
            SwitcherTwoViewTargetTimeSelectionResult::BothUnavailable {
                shared_target_time: TimestampMicros(1_000_000),
                left: SwitcherJitterBufferSelectionResult::NoFrame {
                    client_id: left_client_id,
                    target_time: TimestampMicros(1_000_000)
                },
                right: SwitcherJitterBufferSelectionResult::NoFrame {
                    client_id: right_client_id,
                    target_time: TimestampMicros(1_000_000)
                }
            }
        );
    }

    #[test]
    fn two_view_decode_render_renders_both_selected_frames() {
        let (state, selection) =
            selected_two_view_fixture(vec![0, 0, 1, 0x65, 0x10], vec![0, 0, 1, 0x65, 0x12]);
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let before_left_len = state.client_queue_len(&left_client_id);
        let before_right_len = state.client_queue_len(&right_client_id);
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let result = SwitcherTwoViewDecodeRenderBoundary::default()
            .render_selected_pair_with_runtimes(
                SwitcherTwoViewDecodeRenderInput {
                    selection,
                    left_window_title: "StreamSync Left".to_string(),
                    right_window_title: "StreamSync Right".to_string(),
                    render_hold_millis: 7,
                },
                &decode,
                &render,
            );

        let SwitcherTwoViewDecodeRenderResult::BothRendered {
            shared_target_time,
            left,
            right,
        } = result
        else {
            panic!("both selected sides should render");
        };
        assert_eq!(shared_target_time, TimestampMicros(1_000_011));
        assert_eq!(left.side, SwitcherTwoViewSide::Left);
        assert_eq!(right.side, SwitcherTwoViewSide::Right);
        assert_eq!(left.selected.frame.frame_id, 10);
        assert_eq!(right.selected.frame.frame_id, 12);
        assert_eq!(left.render.title, "StreamSync Left");
        assert_eq!(right.render.title, "StreamSync Right");
        assert_eq!(left.render.hold_millis, 7);
        assert_eq!(right.render.hold_millis, 7);

        let inputs = decode.inputs.borrow();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].encoded_payload, vec![0, 0, 1, 0x65, 0x10]);
        assert_eq!(inputs[0].width, 2);
        assert_eq!(inputs[0].height, 1);
        assert_eq!(inputs[1].encoded_payload, vec![0, 0, 1, 0x65, 0x12]);
        assert_eq!(state.client_queue_len(&left_client_id), before_left_len);
        assert_eq!(state.client_queue_len(&right_client_id), before_right_len);
    }

    #[test]
    fn two_view_decode_render_renders_selected_side_and_skips_partial_side() {
        let (state, selection) = selected_two_view_fixture(vec![0, 0, 1, 0x65, 0x10], vec![]);
        let right_client_id = ClientId("client-right".to_string());
        let selection = match selection {
            SwitcherTwoViewTargetTimeSelectionResult::BothSelected {
                shared_target_time,
                left,
                ..
            } => SwitcherTwoViewTargetTimeSelectionResult::Partial {
                shared_target_time,
                left: SwitcherJitterBufferSelectionResult::Selected(left),
                right: SwitcherJitterBufferSelectionResult::NoFrame {
                    client_id: right_client_id.clone(),
                    target_time: shared_target_time,
                },
            },
            other => panic!("fixture should create both-selected selection: {other:?}"),
        };
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::default();

        let result = SwitcherTwoViewDecodeRenderBoundary::default()
            .render_selected_pair_with_runtimes(
                SwitcherTwoViewDecodeRenderInput {
                    selection,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 1,
                },
                &decode,
                &render,
            );

        let SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { left, right, .. } =
            result
        else {
            panic!("partial selection should render only selected side");
        };
        assert_eq!(left.selected.frame.frame_id, 10);
        assert_eq!(
            right,
            SwitcherTwoViewSkippedSide::SelectionUnavailable {
                side: SwitcherTwoViewSide::Right,
                selection: SwitcherJitterBufferSelectionResult::NoFrame {
                    client_id: right_client_id,
                    target_time: TimestampMicros(1_000_011)
                }
            }
        );
        assert_eq!(decode.inputs.borrow().len(), 1);
        assert_eq!(render.requests.borrow().len(), 1);
        assert_eq!(
            state.client_queue_len(&ClientId("client-left".to_string())),
            1
        );
    }

    #[test]
    fn two_view_decode_render_reports_decode_failure_per_side() {
        let (_state, selection) =
            selected_two_view_fixture(vec![0, 0, 1, 0x65, 0x10], vec![0, 0, 1, 0x65, 0x12]);
        let decode = RecordingTwoViewDecode::failing_on_last_byte(0x12);
        let render = RecordingTwoViewRender::default();

        let result = SwitcherTwoViewDecodeRenderBoundary::default()
            .render_selected_pair_with_runtimes(
                SwitcherTwoViewDecodeRenderInput {
                    selection,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 1,
                },
                &decode,
                &render,
            );

        let SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { left, right, .. } =
            result
        else {
            panic!("right decode failure should skip right only");
        };
        assert_eq!(left.selected.frame.frame_id, 10);
        let SwitcherTwoViewSkippedSide::DecodeFailed {
            side,
            selected,
            failure,
        } = right
        else {
            panic!("right side should carry decode failure");
        };
        assert_eq!(side, SwitcherTwoViewSide::Right);
        assert_eq!(selected.frame.frame_id, 12);
        assert_eq!(failure.message, "fixture decode failed for 0x12");
        assert_eq!(decode.inputs.borrow().len(), 2);
        assert_eq!(render.requests.borrow().len(), 1);
    }

    #[test]
    fn two_view_decode_render_reports_render_failure_per_side() {
        let (_state, selection) =
            selected_two_view_fixture(vec![0, 0, 1, 0x65, 0x10], vec![0, 0, 1, 0x65, 0x12]);
        let decode = RecordingTwoViewDecode::default();
        let render = RecordingTwoViewRender::failing_when_title_contains("right");

        let result = SwitcherTwoViewDecodeRenderBoundary::default()
            .render_selected_pair_with_runtimes(
                SwitcherTwoViewDecodeRenderInput {
                    selection,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 1,
                },
                &decode,
                &render,
            );

        let SwitcherTwoViewDecodeRenderResult::LeftRenderedRightSkipped { left, right, .. } =
            result
        else {
            panic!("right render failure should skip right only");
        };
        assert_eq!(left.selected.frame.frame_id, 10);
        let SwitcherTwoViewSkippedSide::RenderFailed {
            side,
            selected,
            message,
        } = right
        else {
            panic!("right side should carry render failure");
        };
        assert_eq!(side, SwitcherTwoViewSide::Right);
        assert_eq!(selected.frame.frame_id, 12);
        assert_eq!(message, "fixture render failed for right");
        assert_eq!(decode.inputs.borrow().len(), 2);
        assert_eq!(render.requests.borrow().len(), 2);
    }

    #[test]
    fn two_view_decode_render_reports_both_unavailable_without_decode_or_render() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let selection = SwitcherTwoViewTargetTimeSelectionResult::BothUnavailable {
            shared_target_time: TimestampMicros(1_000_000),
            left: SwitcherJitterBufferSelectionResult::NoFrame {
                client_id: left_client_id.clone(),
                target_time: TimestampMicros(1_000_000),
            },
            right: SwitcherJitterBufferSelectionResult::NoFrame {
                client_id: right_client_id.clone(),
                target_time: TimestampMicros(1_000_000),
            },
        };

        let result = SwitcherTwoViewDecodeRenderBoundary::default()
            .render_selected_pair_with_runtimes(
                SwitcherTwoViewDecodeRenderInput {
                    selection,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 1,
                },
                &PanicDecode,
                &PanicRender,
            );

        assert_eq!(
            result,
            SwitcherTwoViewDecodeRenderResult::BothSkipped {
                shared_target_time: TimestampMicros(1_000_000),
                left: SwitcherTwoViewSkippedSide::SelectionUnavailable {
                    side: SwitcherTwoViewSide::Left,
                    selection: SwitcherJitterBufferSelectionResult::NoFrame {
                        client_id: left_client_id,
                        target_time: TimestampMicros(1_000_000)
                    }
                },
                right: SwitcherTwoViewSkippedSide::SelectionUnavailable {
                    side: SwitcherTwoViewSide::Right,
                    selection: SwitcherJitterBufferSelectionResult::NoFrame {
                        client_id: right_client_id,
                        target_time: TimestampMicros(1_000_000)
                    }
                }
            }
        );
    }

    #[test]
    fn two_view_manual_verification_fixture_selects_and_renders_both_sides() {
        let (state, _selection) =
            selected_two_view_fixture(vec![0, 0, 1, 0x65, 0x10], vec![0, 0, 1, 0x65, 0x12]);
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewManualVerificationBoundary::default().verify_with_runtimes(
            SwitcherTwoViewManualVerificationInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_011),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
                render_hold_millis: 5,
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(
            result.summary.shared_target_time,
            TimestampMicros(1_000_011)
        );
        assert_eq!(
            result.summary.left.selection_status,
            SwitcherTwoViewManualSelectionStatus::Selected
        );
        assert_eq!(
            result.summary.right.selection_status,
            SwitcherTwoViewManualSelectionStatus::Selected
        );
        assert_eq!(
            result.summary.left.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        );
        assert_eq!(
            result.summary.right.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        );
        assert_eq!(result.summary.left.frame_id, Some(10));
        assert_eq!(result.summary.right.frame_id, Some(12));
        assert_eq!(result.summary.left.encoded_payload_len, Some(5));
        assert_eq!(result.summary.right.encoded_payload_len, Some(5));
    }

    #[test]
    fn two_view_manual_verification_keeps_one_missing_side_explicit() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_payload(
            &mut state,
            "client-left",
            10,
            TimestampMicros(2_320_000),
            2,
            1,
            vec![0, 0, 1, 0x65, 0x10],
        );
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewManualVerificationBoundary::default().verify_with_runtimes(
            SwitcherTwoViewManualVerificationInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_010),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
                render_hold_millis: 5,
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(
            result.summary.left.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        );
        assert_eq!(
            result.summary.right.selection_status,
            SwitcherTwoViewManualSelectionStatus::NoFrame
        );
        assert_eq!(
            result.summary.right.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        );
        assert_eq!(result.summary.right.frame_id, None);
    }

    #[test]
    fn two_view_manual_verification_surfaces_decode_failure_per_side() {
        let (state, _selection) =
            selected_two_view_fixture(vec![0, 0, 1, 0x65, 0x10], vec![0, 0, 1, 0x65, 0x12]);
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewManualVerificationBoundary::default().verify_with_runtimes(
            SwitcherTwoViewManualVerificationInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_011),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
                render_hold_millis: 5,
            },
            &RecordingTwoViewDecode::failing_on_last_byte(0x12),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(
            result.summary.left.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        );
        assert_eq!(
            result.summary.right.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::DecodeFailed
        );
        assert_eq!(result.summary.right.frame_id, Some(12));
    }

    #[test]
    fn two_view_manual_verification_surfaces_render_failure_per_side() {
        let (state, _selection) =
            selected_two_view_fixture(vec![0, 0, 1, 0x65, 0x10], vec![0, 0, 1, 0x65, 0x12]);
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewManualVerificationBoundary::default().verify_with_runtimes(
            SwitcherTwoViewManualVerificationInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_011),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
                render_hold_millis: 5,
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::failing_when_title_contains("client-right"),
        );

        assert_eq!(
            result.summary.left.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        );
        assert_eq!(
            result.summary.right.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::RenderFailed
        );
        assert_eq!(result.summary.right.frame_id, Some(12));
    }

    #[test]
    fn two_view_manual_verification_offset_affects_one_side_selection() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-left", 100, TimestampMicros(2_330_000));
        store_frame(&mut state, "client-right", 0, TimestampMicros(2_330_010));
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());

        let result = SwitcherTwoViewManualVerificationBoundary::default().verify_with_runtimes(
            SwitcherTwoViewManualVerificationInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_100),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    right_clock_offset_micros: Some(100),
                    max_late_micros: 10,
                    max_early_micros: 10,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
                render_hold_millis: 5,
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(
            result.summary.left.adjusted_capture_timestamp,
            Some(TimestampMicros(1_000_100))
        );
        assert_eq!(
            result.summary.right.adjusted_capture_timestamp,
            Some(TimestampMicros(1_000_100))
        );
        assert_eq!(
            result.summary.right.selection_status,
            SwitcherTwoViewManualSelectionStatus::Selected
        );
        assert_eq!(
            result.summary.right.decode_render_status,
            SwitcherTwoViewManualDecodeRenderStatus::Rendered
        );
    }

    #[test]
    fn placeholder_display_boundary_does_not_perform_real_decode_or_display() {
        let selected = SwitcherSingleViewSelectedEncodedFrame {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            frame_id: 3,
            capture_timestamp: TimestampMicros(1_000_003),
            send_timestamp: TimestampMicros(1_000_103),
            queued_at: TimestampMicros(2_300_000),
            is_keyframe: true,
            width: 1280,
            height: 720,
            fps_nominal: 30,
            codec: Codec::H264,
            encoded_payload_len: 3,
            encoded_payload: vec![0x03, 0xbb, 0xcc],
        };

        let result = SwitcherSingleViewPlaceholderDisplayBoundary.prepare_handoff(
            SwitcherSingleViewFrameSelectionResult::FrameAvailable(selected.clone()),
        );

        assert_eq!(
            result,
            SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(
                SwitcherSingleViewDisplayPlaceholderHandoff {
                    selected,
                    decode_status: SwitcherSingleViewDecodeStatus::DeferredPlaceholder,
                }
            )
        );
    }

    #[test]
    fn h264_decode_boundary_success_with_runtime_hook_returns_decoded_bgra() {
        struct SuccessfulDecode;
        impl SwitcherH264DecodeRuntimeHook for SuccessfulDecode {
            fn decode_annex_b_h264(
                &self,
                input: SwitcherH264DecodeInput,
            ) -> SwitcherH264DecodeResult {
                assert_eq!(input.encoded_payload, vec![0x00, 0x00, 0x01, 0x65]);
                SwitcherH264DecodeResult::Decoded(SwitcherDecodedFrame {
                    width: input.width,
                    height: input.height,
                    pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                    pixels: vec![0, 1, 2, 255, 3, 4, 5, 255],
                })
            }
        }

        let result = SwitcherH264DecodeBoundary.decode_with_runtime(
            SwitcherH264DecodeInput {
                encoded_payload: vec![0x00, 0x00, 0x01, 0x65],
                width: 2,
                height: 1,
            },
            &SuccessfulDecode,
        );

        assert_eq!(
            result,
            SwitcherH264DecodeResult::Decoded(SwitcherDecodedFrame {
                width: 2,
                height: 1,
                pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                pixels: vec![0, 1, 2, 255, 3, 4, 5, 255],
            })
        );
    }

    #[test]
    fn h264_decode_boundary_empty_payload_is_deferred() {
        let result = SwitcherH264DecodeBoundary.decode_with_runtime(
            SwitcherH264DecodeInput {
                encoded_payload: Vec::new(),
                width: 2,
                height: 1,
            },
            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
        );

        assert_eq!(
            result,
            SwitcherH264DecodeResult::Deferred {
                reason: SwitcherH264DecodeDeferredReason::EmptyPayload
            }
        );
    }

    #[test]
    fn h264_decode_boundary_failure_is_explicit() {
        struct FailingDecode;
        impl SwitcherH264DecodeRuntimeHook for FailingDecode {
            fn decode_annex_b_h264(
                &self,
                _input: SwitcherH264DecodeInput,
            ) -> SwitcherH264DecodeResult {
                SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                    message: "fixture decode failed".to_string(),
                })
            }
        }

        let result = SwitcherH264DecodeBoundary.decode_with_runtime(
            SwitcherH264DecodeInput {
                encoded_payload: vec![0x01],
                width: 2,
                height: 1,
            },
            &FailingDecode,
        );

        assert_eq!(
            result,
            SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                message: "fixture decode failed".to_string()
            })
        );
    }

    #[test]
    fn real_decode_display_path_writes_bmp_for_decoded_frame() {
        struct SuccessfulDecode;
        impl SwitcherH264DecodeRuntimeHook for SuccessfulDecode {
            fn decode_annex_b_h264(
                &self,
                input: SwitcherH264DecodeInput,
            ) -> SwitcherH264DecodeResult {
                SwitcherH264DecodeResult::Decoded(SwitcherDecodedFrame {
                    width: input.width,
                    height: input.height,
                    pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                    pixels: vec![0, 0, 255, 255, 0, 255, 0, 255],
                })
            }
        }

        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_payload(
            &mut state,
            "client-1",
            20,
            TimestampMicros(3_100_000),
            2,
            1,
            vec![0x00, 0x00, 0x01, 0x65],
        );
        let client_id = ClientId("client-1".to_string());
        let output_path = std::env::temp_dir().join(format!(
            "stream-sync-switcher-decode-{}.bmp",
            current_test_suffix()
        ));

        let result = SwitcherDecodeLatestFrameOnceBoundary::default().decode_latest_with_runtime(
            SwitcherDecodeLatestFrameOnceInput {
                queue_state: &state,
                client_id: &client_id,
                output_path: output_path.clone(),
            },
            &SuccessfulDecode,
        );

        let SwitcherDecodeLatestFrameOnceResult::Decoded {
            summary,
            handoff,
            dump,
        } = result
        else {
            panic!("successful decode should dump one BMP");
        };
        assert_eq!(summary.selected_client_id, client_id);
        assert_eq!(summary.frame_id, Some(20));
        assert_eq!(
            summary.decode_status,
            Some(SwitcherSingleViewDecodeStatus::Decoded)
        );
        assert_eq!(handoff.decoded.width, 2);
        assert_eq!(handoff.decoded.height, 1);
        assert_eq!(dump.path, output_path);
        assert!(dump.bytes_written > 54);
        let bytes = std::fs::read(&dump.path).expect("bmp should be readable");
        assert_eq!(&bytes[0..2], b"BM");
        let _ = std::fs::remove_file(dump.path);
    }

    #[test]
    fn real_decode_display_path_falls_back_to_placeholder_on_decode_failure() {
        struct FailingDecode;
        impl SwitcherH264DecodeRuntimeHook for FailingDecode {
            fn decode_annex_b_h264(
                &self,
                _input: SwitcherH264DecodeInput,
            ) -> SwitcherH264DecodeResult {
                SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                    message: "fixture decode failed".to_string(),
                })
            }
        }

        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 21, TimestampMicros(3_200_000));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherDecodeLatestFrameOnceBoundary::default().decode_latest_with_runtime(
            SwitcherDecodeLatestFrameOnceInput {
                queue_state: &state,
                client_id: &client_id,
                output_path: PathBuf::from("should-not-write.bmp"),
            },
            &FailingDecode,
        );

        let SwitcherDecodeLatestFrameOnceResult::PlaceholderFallback { summary, handoff } = result
        else {
            panic!("decode failure should fall back to placeholder");
        };
        assert_eq!(summary.selected_client_id, client_id);
        assert_eq!(summary.frame_id, Some(21));
        assert_eq!(
            summary.decode_status,
            Some(SwitcherSingleViewDecodeStatus::DecodeFailed)
        );
        assert_eq!(
            handoff.decode_status,
            SwitcherSingleViewDecodeStatus::DecodeFailed
        );
    }

    #[test]
    fn decoded_frame_render_input_validates_dimensions_and_buffer_length() {
        let frame = SwitcherDecodedFrame {
            width: 2,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: vec![0; 8],
        };

        let input = SwitcherDecodedFrameRenderInput::from_decoded_frame(&frame)
            .expect("valid BGRA frame should produce render input");

        assert_eq!(input.width, 2);
        assert_eq!(input.height, 1);
        assert_eq!(input.pixel_format, SwitcherDecodedFramePixelFormat::Bgra8);
        assert_eq!(input.pixels.len(), 8);

        let invalid_dimensions = SwitcherDecodedFrame {
            width: 0,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: Vec::new(),
        };
        assert_eq!(
            SwitcherDecodedFrameRenderInput::from_decoded_frame(&invalid_dimensions),
            Err(SwitcherDecodedFrameRenderInputError::InvalidDimensions)
        );

        let invalid_len = SwitcherDecodedFrame {
            width: 2,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: vec![0; 7],
        };
        assert_eq!(
            SwitcherDecodedFrameRenderInput::from_decoded_frame(&invalid_len),
            Err(SwitcherDecodedFrameRenderInputError::InvalidBufferLength {
                expected: 8,
                actual: 7
            })
        );
    }

    #[test]
    fn window_render_boundary_invalid_frame_is_explicit() {
        let frame = SwitcherDecodedFrame {
            width: 2,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: vec![0; 7],
        };

        let result = SwitcherWindowRenderBoundary.render_decoded_frame_with_runtime(
            &frame,
            "StreamSync Test",
            1,
            &SwitcherUnavailableWindowRenderRuntimeHook,
        );

        assert_eq!(
            result,
            SwitcherWindowRenderResult::InvalidFrame {
                error: SwitcherDecodedFrameRenderInputError::InvalidBufferLength {
                    expected: 8,
                    actual: 7
                }
            }
        );
    }

    #[test]
    fn window_render_boundary_unavailable_backend_is_explicit() {
        let frame = SwitcherDecodedFrame {
            width: 2,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: vec![0; 8],
        };

        let result = SwitcherWindowRenderBoundary.render_decoded_frame_with_runtime(
            &frame,
            "StreamSync Test",
            1,
            &SwitcherUnavailableWindowRenderRuntimeHook,
        );

        assert_eq!(
            result,
            SwitcherWindowRenderResult::BackendUnavailable {
                reason: SwitcherWindowBackendUnavailableReason::UnsupportedPlatform,
                message: Some("switcher window rendering backend is unavailable".to_string())
            }
        );
    }

    #[test]
    fn window_render_boundary_can_render_with_caller_owned_runtime() {
        struct FixtureRender;
        impl SwitcherWindowRenderRuntimeHook for FixtureRender {
            fn render_once(
                &self,
                request: SwitcherWindowRenderRequest,
            ) -> SwitcherWindowRenderResult {
                assert_eq!(request.frame.width, 2);
                assert_eq!(request.frame.height, 1);
                assert_eq!(request.frame.pixels.len(), 8);
                SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                    width: request.frame.width,
                    height: request.frame.height,
                    title: request.title,
                    hold_millis: request.hold_millis,
                })
            }
        }
        let frame = SwitcherDecodedFrame {
            width: 2,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: vec![0; 8],
        };

        let result = SwitcherWindowRenderBoundary.render_decoded_frame_with_runtime(
            &frame,
            "StreamSync Test",
            123,
            &FixtureRender,
        );

        assert_eq!(
            result,
            SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                width: 2,
                height: 1,
                title: "StreamSync Test".to_string(),
                hold_millis: 123
            })
        );
    }

    #[test]
    fn window_render_boundary_stays_separate_from_bmp_dump() {
        struct FixtureRender;
        impl SwitcherWindowRenderRuntimeHook for FixtureRender {
            fn render_once(
                &self,
                request: SwitcherWindowRenderRequest,
            ) -> SwitcherWindowRenderResult {
                SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                    width: request.frame.width,
                    height: request.frame.height,
                    title: request.title,
                    hold_millis: request.hold_millis,
                })
            }
        }
        let frame = SwitcherDecodedFrame {
            width: 1,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: vec![0, 0, 255, 255],
        };
        let output_path = std::env::temp_dir().join(format!(
            "stream-sync-switcher-render-separate-{}.bmp",
            current_test_suffix()
        ));

        let render = SwitcherWindowRenderBoundary.render_decoded_frame_with_runtime(
            &frame,
            "StreamSync Test",
            1,
            &FixtureRender,
        );
        let dump = SwitcherDecodedFrameDumpBoundary
            .write_bmp(&frame, &output_path)
            .expect("bmp dump should remain independently available");

        assert!(matches!(render, SwitcherWindowRenderResult::Rendered(_)));
        assert_eq!(dump.path, output_path);
        assert!(dump.bytes_written > 54);
        let _ = std::fs::remove_file(dump.path);
    }

    #[test]
    fn continuous_render_loop_renders_available_decoded_frames() {
        let client_id = ClientId("client-1".to_string());
        let mut source = ScriptedFrameSource::new(vec![scripted_selected_frame(&client_id, 1)]);

        let result = SwitcherContinuousRenderLoopBoundary::default().run_with_runtimes(
            SwitcherContinuousRenderLoopInput {
                client_id: client_id.clone(),
                window_title: "StreamSync Test".to_string(),
                policy: SwitcherContinuousRenderLoopPolicy {
                    max_iterations: 3,
                    max_rendered_frames: 1,
                    render_hold_millis: 5,
                },
            },
            &mut source,
            &SuccessfulLoopDecode,
            &SuccessfulLoopRender,
        );

        assert_eq!(result.summary.iterations, 1);
        assert_eq!(result.summary.rendered_frames, 1);
        assert_eq!(
            result.stop_reason,
            SwitcherContinuousRenderLoopStopReason::MaxRenderedFramesReached
        );
        assert!(matches!(
            result.events.as_slice(),
            [SwitcherContinuousRenderLoopEvent::Rendered {
                iteration: 0,
                frame_id: 1,
                ..
            }]
        ));
    }

    #[test]
    fn continuous_render_loop_handles_no_frame_explicitly() {
        let client_id = ClientId("client-1".to_string());
        let mut source = ScriptedFrameSource::new(vec![
            SwitcherSingleViewFrameSelectionResult::NoFrameAvailable {
                client_id: client_id.clone(),
            },
            scripted_selected_frame(&client_id, 2),
        ]);

        let result = SwitcherContinuousRenderLoopBoundary::default().run_with_runtimes(
            SwitcherContinuousRenderLoopInput {
                client_id: client_id.clone(),
                window_title: "StreamSync Test".to_string(),
                policy: SwitcherContinuousRenderLoopPolicy {
                    max_iterations: 2,
                    max_rendered_frames: 1,
                    render_hold_millis: 5,
                },
            },
            &mut source,
            &SuccessfulLoopDecode,
            &SuccessfulLoopRender,
        );

        assert_eq!(result.summary.iterations, 2);
        assert_eq!(result.summary.no_frame_count, 1);
        assert_eq!(result.summary.rendered_frames, 1);
        assert!(matches!(
            result.events.as_slice(),
            [
                SwitcherContinuousRenderLoopEvent::NoFrame { iteration: 0, .. },
                SwitcherContinuousRenderLoopEvent::Rendered {
                    iteration: 1,
                    frame_id: 2,
                    ..
                }
            ]
        ));
    }

    #[test]
    fn continuous_render_loop_handles_decode_failure_explicitly() {
        let client_id = ClientId("client-1".to_string());
        let mut source = ScriptedFrameSource::new(vec![scripted_selected_frame(&client_id, 3)]);

        let result = SwitcherContinuousRenderLoopBoundary::default().run_with_runtimes(
            SwitcherContinuousRenderLoopInput {
                client_id,
                window_title: "StreamSync Test".to_string(),
                policy: SwitcherContinuousRenderLoopPolicy {
                    max_iterations: 1,
                    max_rendered_frames: 1,
                    render_hold_millis: 5,
                },
            },
            &mut source,
            &FailingLoopDecode,
            &SuccessfulLoopRender,
        );

        assert_eq!(result.summary.decode_failed_count, 1);
        assert_eq!(result.summary.rendered_frames, 0);
        assert!(matches!(
            result.events.as_slice(),
            [SwitcherContinuousRenderLoopEvent::DecodeFailed {
                iteration: 0,
                frame_id: 3,
                ..
            }]
        ));
    }

    #[test]
    fn continuous_render_loop_handles_render_failure_explicitly() {
        let client_id = ClientId("client-1".to_string());
        let mut source = ScriptedFrameSource::new(vec![scripted_selected_frame(&client_id, 4)]);

        let result = SwitcherContinuousRenderLoopBoundary::default().run_with_runtimes(
            SwitcherContinuousRenderLoopInput {
                client_id,
                window_title: "StreamSync Test".to_string(),
                policy: SwitcherContinuousRenderLoopPolicy {
                    max_iterations: 1,
                    max_rendered_frames: 1,
                    render_hold_millis: 5,
                },
            },
            &mut source,
            &SuccessfulLoopDecode,
            &FailingLoopRender,
        );

        assert_eq!(result.summary.render_not_completed_count, 1);
        assert_eq!(result.summary.rendered_frames, 0);
        assert!(matches!(
            result.events.as_slice(),
            [SwitcherContinuousRenderLoopEvent::RenderNotCompleted {
                iteration: 0,
                frame_id: 4,
                result: SwitcherWindowRenderResult::RenderFailed { .. }
            }]
        ));
    }

    #[test]
    fn continuous_render_loop_max_iteration_guard_stops_deterministically() {
        let client_id = ClientId("client-1".to_string());
        let mut source = ScriptedFrameSource::new(vec![
            SwitcherSingleViewFrameSelectionResult::NoFrameAvailable {
                client_id: client_id.clone(),
            },
            SwitcherSingleViewFrameSelectionResult::NoFrameAvailable {
                client_id: client_id.clone(),
            },
        ]);

        let result = SwitcherContinuousRenderLoopBoundary::default().run_with_runtimes(
            SwitcherContinuousRenderLoopInput {
                client_id,
                window_title: "StreamSync Test".to_string(),
                policy: SwitcherContinuousRenderLoopPolicy {
                    max_iterations: 2,
                    max_rendered_frames: 1,
                    render_hold_millis: 5,
                },
            },
            &mut source,
            &SuccessfulLoopDecode,
            &SuccessfulLoopRender,
        );

        assert_eq!(result.summary.iterations, 2);
        assert_eq!(result.summary.no_frame_count, 2);
        assert_eq!(
            result.stop_reason,
            SwitcherContinuousRenderLoopStopReason::MaxIterationsReached
        );
    }

    #[test]
    fn continuous_render_loop_does_not_break_one_shot_render_boundary() {
        let frame = SwitcherDecodedFrame {
            width: 2,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: vec![0; 8],
        };

        let result = SwitcherWindowRenderBoundary.render_decoded_frame_with_runtime(
            &frame,
            "StreamSync One Shot",
            10,
            &SuccessfulLoopRender,
        );

        assert!(matches!(result, SwitcherWindowRenderResult::Rendered(_)));
    }

    #[test]
    fn manual_verification_helper_selects_latest_frame_from_caller_owned_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 10, TimestampMicros(2_400_000));
        store_frame(&mut state, "client-1", 11, TimestampMicros(2_400_100));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        let SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } =
            result
        else {
            panic!("fixture queue should produce a placeholder handoff");
        };
        assert_eq!(summary.selected_client_id, client_id);
        assert_eq!(summary.frame_id, Some(11));
        assert_eq!(handoff.selected.frame_id, 11);
    }

    #[test]
    fn manual_verification_helper_reports_no_frame_for_empty_queue() {
        let state = ServerVideoFrameQueueState::default();
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        assert_eq!(
            result,
            SwitcherPlaceholderManualVerificationResult::NoFrame {
                summary: SwitcherPlaceholderManualVerificationSummary {
                    selected_client_id: client_id,
                    frame_id: None,
                    encoded_payload_len: None,
                    decode_status: None,
                    no_frame: true,
                }
            }
        );
    }

    #[test]
    fn manual_verification_helper_preserves_metadata_and_payload_length() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 12, TimestampMicros(2_500_000));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        let SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } =
            result
        else {
            panic!("fixture queue should produce a placeholder handoff");
        };
        assert_eq!(summary.frame_id, Some(12));
        assert_eq!(summary.encoded_payload_len, Some(3));
        assert_eq!(
            handoff.selected.capture_timestamp,
            TimestampMicros(1_000_012)
        );
        assert_eq!(handoff.selected.send_timestamp, TimestampMicros(1_000_112));
        assert_eq!(handoff.selected.encoded_payload_len, 3);
        assert_eq!(handoff.selected.encoded_payload, vec![0x0c, 0xbb, 0xcc]);
    }

    #[test]
    fn manual_verification_helper_reports_decode_deferred_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 13, TimestampMicros(2_600_000));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        let SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } =
            result
        else {
            panic!("fixture queue should produce a placeholder handoff");
        };
        assert_eq!(
            summary.decode_status,
            Some(SwitcherSingleViewDecodeStatus::DeferredPlaceholder)
        );
        assert_eq!(
            handoff.decode_status,
            SwitcherSingleViewDecodeStatus::DeferredPlaceholder
        );
    }

    #[test]
    fn manual_verification_helper_does_not_mutate_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 14, TimestampMicros(2_700_000));
        store_frame(&mut state, "client-1", 15, TimestampMicros(2_700_100));
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let _result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        assert_eq!(state.client_queue_len(&client_id), before_len);
        let frame_ids: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(frame_ids, vec![14, 15]);
    }

    #[test]
    fn bridge_composes_server_queue_result_and_switcher_placeholder_handoff() {
        let mut state = ServerVideoFrameQueueState::default();
        let storage = store_frame(&mut state, "client-1", 16, TimestampMicros(2_800_000));
        let queue_result = ServerVideoFrameQueueRuntimeResult::Queued(storage);
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherAuthVideoPlaceholderBridgeBoundary::default().verify(
            SwitcherAuthVideoPlaceholderBridgeInput {
                auth_accepted: true,
                video_status: SwitcherAuthVideoPlaceholderBridgeVideoStatus::Received,
                queue_result: Some(&queue_result),
                queue_state: &state,
                client_id: &client_id,
            },
        );

        let SwitcherAuthVideoPlaceholderBridgeResult::PlaceholderReady { summary, handoff } =
            result
        else {
            panic!("queued frame should produce placeholder handoff");
        };
        assert!(summary.auth_accepted);
        assert!(summary.video_received);
        assert!(summary.video_accepted);
        assert!(!summary.video_rejected);
        assert!(summary.queued);
        assert_eq!(summary.queue_len, 1);
        assert_eq!(summary.selected_client_id, client_id);
        assert_eq!(summary.selected_frame_id, Some(16));
        assert_eq!(summary.payload_len, Some(3));
        assert_eq!(
            summary.decode_status,
            Some(SwitcherSingleViewDecodeStatus::DeferredPlaceholder)
        );
        assert_eq!(handoff.selected.frame_id, 16);
    }

    #[test]
    fn bridge_selects_queued_frame_by_client_id() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 17, TimestampMicros(2_900_000));
        let storage = store_frame(&mut state, "client-2", 21, TimestampMicros(2_900_100));
        let queue_result = ServerVideoFrameQueueRuntimeResult::Queued(storage);
        let client_id = ClientId("client-2".to_string());

        let result = SwitcherAuthVideoPlaceholderBridgeBoundary::default().verify(
            SwitcherAuthVideoPlaceholderBridgeInput {
                auth_accepted: true,
                video_status: SwitcherAuthVideoPlaceholderBridgeVideoStatus::Received,
                queue_result: Some(&queue_result),
                queue_state: &state,
                client_id: &client_id,
            },
        );

        let SwitcherAuthVideoPlaceholderBridgeResult::PlaceholderReady { summary, handoff } =
            result
        else {
            panic!("client-2 queued frame should be selected");
        };
        assert_eq!(summary.selected_client_id, client_id);
        assert_eq!(summary.selected_frame_id, Some(21));
        assert_eq!(handoff.selected.frame_id, 21);
    }

    #[test]
    fn bridge_reports_no_frame_when_queue_has_no_selected_client_frame() {
        let state = ServerVideoFrameQueueState::default();
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherAuthVideoPlaceholderBridgeBoundary::default().verify(
            SwitcherAuthVideoPlaceholderBridgeInput {
                auth_accepted: true,
                video_status: SwitcherAuthVideoPlaceholderBridgeVideoStatus::Received,
                queue_result: None,
                queue_state: &state,
                client_id: &client_id,
            },
        );

        assert_eq!(
            result,
            SwitcherAuthVideoPlaceholderBridgeResult::NoFrame {
                summary: SwitcherAuthVideoPlaceholderBridgeSummary {
                    auth_accepted: true,
                    video_received: true,
                    video_accepted: false,
                    video_rejected: false,
                    queued: false,
                    dropped_oldest: false,
                    queue_len: 0,
                    selected_client_id: client_id,
                    selected_frame_id: None,
                    payload_len: None,
                    decode_status: None,
                    no_frame: true,
                }
            }
        );
    }

    #[test]
    fn bridge_rejected_video_does_not_produce_fake_selected_frame() {
        let state = ServerVideoFrameQueueState::default();
        let client_id = ClientId("client-1".to_string());
        let queue_result = ServerVideoFrameQueueRuntimeResult::NotQueued {
            reason: ServerVideoFrameQueueRuntimeSkipReason::NoAcceptedVideoFrame,
            side_effect: ServerDispatchRuntimeSideEffectApplyResult::NoDispatch(
                ServerHandlerDispatchOutcome {
                    packet_len: None,
                    result: ServerHandlerDispatchResult::Unsupported {
                        source: PacketSource {
                            address: "127.0.0.1:5001".parse().unwrap(),
                        },
                        message_type: MessageType::VideoFrame,
                    },
                },
            ),
        };

        let result = SwitcherAuthVideoPlaceholderBridgeBoundary::default().verify(
            SwitcherAuthVideoPlaceholderBridgeInput {
                auth_accepted: true,
                video_status: SwitcherAuthVideoPlaceholderBridgeVideoStatus::Received,
                queue_result: Some(&queue_result),
                queue_state: &state,
                client_id: &client_id,
            },
        );

        let SwitcherAuthVideoPlaceholderBridgeResult::NoFrame { summary } = result else {
            panic!("rejected video should not produce a placeholder frame");
        };
        assert!(summary.video_received);
        assert!(!summary.video_accepted);
        assert!(summary.video_rejected);
        assert!(!summary.queued);
        assert_eq!(summary.selected_frame_id, None);
        assert!(summary.no_frame);
    }

    #[test]
    fn bridge_does_not_mutate_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 18, TimestampMicros(3_000_000));
        let storage = store_frame(&mut state, "client-1", 19, TimestampMicros(3_000_100));
        let queue_result = ServerVideoFrameQueueRuntimeResult::Queued(storage);
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let _result = SwitcherAuthVideoPlaceholderBridgeBoundary::default().verify(
            SwitcherAuthVideoPlaceholderBridgeInput {
                auth_accepted: true,
                video_status: SwitcherAuthVideoPlaceholderBridgeVideoStatus::Received,
                queue_result: Some(&queue_result),
                queue_state: &state,
                client_id: &client_id,
            },
        );

        assert_eq!(state.client_queue_len(&client_id), before_len);
        let frame_ids: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(frame_ids, vec![18, 19]);
    }

    fn store_frame(
        state: &mut ServerVideoFrameQueueState,
        client_id: &str,
        frame_id: u64,
        queued_at: TimestampMicros,
    ) -> ServerVideoFrameQueueStorageResult {
        store_frame_with_payload(
            state,
            client_id,
            frame_id,
            queued_at,
            1280,
            720,
            vec![frame_id as u8, 0xbb, 0xcc],
        )
    }

    fn store_frame_for_run(
        state: &mut ServerVideoFrameQueueState,
        client_id: &str,
        run_id: &str,
        frame_id: u64,
        queued_at: TimestampMicros,
    ) -> ServerVideoFrameQueueStorageResult {
        store_frame_with_run_payload(
            state,
            client_id,
            run_id,
            frame_id,
            queued_at,
            1280,
            720,
            vec![frame_id as u8, 0xbb, 0xcc],
        )
    }

    fn store_frame_with_payload(
        state: &mut ServerVideoFrameQueueState,
        client_id: &str,
        frame_id: u64,
        queued_at: TimestampMicros,
        width: u32,
        height: u32,
        payload: Vec<u8>,
    ) -> ServerVideoFrameQueueStorageResult {
        store_frame_with_run_payload(
            state, client_id, "run-1", frame_id, queued_at, width, height, payload,
        )
    }

    fn store_frame_with_run_payload(
        state: &mut ServerVideoFrameQueueState,
        client_id: &str,
        run_id: &str,
        frame_id: u64,
        queued_at: TimestampMicros,
        width: u32,
        height: u32,
        payload: Vec<u8>,
    ) -> ServerVideoFrameQueueStorageResult {
        let packet =
            registered_video_packet_for_run(client_id, run_id, frame_id, width, height, payload);
        let input = ServerVideoFrameHandlerBoundary.prepare_input(packet);
        ServerVideoFrameQueueStorageBoundary.store_frame(
            state,
            input,
            queued_at,
            ServerVideoFrameQueuePolicy::default(),
        )
    }

    fn registered_video_packet(
        client_id: &str,
        frame_id: u64,
        width: u32,
        height: u32,
        payload: Vec<u8>,
    ) -> ServerRegisteredVideoFramePacket {
        registered_video_packet_for_run(client_id, "run-1", frame_id, width, height, payload)
    }

    fn registered_video_packet_for_run(
        client_id: &str,
        run_id: &str,
        frame_id: u64,
        width: u32,
        height: u32,
        payload: Vec<u8>,
    ) -> ServerRegisteredVideoFramePacket {
        let source = PacketSource {
            address: "127.0.0.1:5001".parse().unwrap(),
        };
        let payload_size = payload.len();
        ServerRegisteredVideoFramePacket {
            source,
            authenticated_sender: AuthenticatedSenderEntry {
                client_id: ClientId(client_id.to_string()),
                source,
                run_id: RunId(run_id.to_string()),
                protocol_version: ProtocolVersion(1),
                registered_at: None,
            },
            frame: VideoFrame {
                message_type: MessageType::VideoFrame,
                protocol_version: ProtocolVersion(1),
                client_id: ClientId(client_id.to_string()),
                run_id: RunId(run_id.to_string()),
                frame_id,
                capture_timestamp: TimestampMicros(1_000_000 + frame_id),
                send_timestamp: TimestampMicros(1_000_100 + frame_id),
                is_keyframe: frame_id == 1,
                metadata_reserved: [0; 3],
                width,
                height,
                fps_nominal: 30,
                codec: Codec::H264,
                payload_size,
                payload,
            },
        }
    }

    fn current_test_suffix() -> String {
        format!("{}-{:?}", std::process::id(), std::thread::current().id())
    }

    fn selected_two_view_fixture(
        left_payload: Vec<u8>,
        right_payload: Vec<u8>,
    ) -> (
        ServerVideoFrameQueueState,
        SwitcherTwoViewTargetTimeSelectionResult,
    ) {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame_with_payload(
            &mut state,
            "client-left",
            10,
            TimestampMicros(2_310_000),
            2,
            1,
            left_payload,
        );
        store_frame_with_payload(
            &mut state,
            "client-right",
            12,
            TimestampMicros(2_310_010),
            2,
            1,
            right_payload,
        );
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let selection = SwitcherTwoViewTargetTimeSelectionBoundary::default().select_pair(
            SwitcherTwoViewTargetTimeSelectionInput {
                queue_state: &state,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                current_switcher_time: TimestampMicros(1_600_011),
                policy: SwitcherTwoViewTargetTimeSelectionPolicy {
                    playout_delay_micros: 600_000,
                    max_late_micros: 50,
                    max_early_micros: 50,
                    ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
                },
            },
        );
        (state, selection)
    }

    fn two_view_scheduler_result(
        state: &mut ServerVideoFrameQueueState,
        target_timestamp: TimestampMicros,
    ) -> SwitcherTwoViewTargetTimeSourceSchedulerResult {
        two_view_scheduler_result_with_mode(
            state,
            target_timestamp,
            SwitcherTwoViewTargetTimeSourceSchedulerMode::PreviewLatestIfAtOrBefore,
        )
    }

    fn two_view_scheduler_result_with_mode(
        state: &mut ServerVideoFrameQueueState,
        target_timestamp: TimestampMicros,
        mode: SwitcherTwoViewTargetTimeSourceSchedulerMode,
    ) -> SwitcherTwoViewTargetTimeSourceSchedulerResult {
        SwitcherTwoViewTargetTimeSourceSchedulerBoundary::default().select_pair(
            state,
            SwitcherTwoViewTargetTimeSourceSchedulerInput {
                left: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-left".to_string()),
                    run_id: RunId("run-left".to_string()),
                },
                right: SwitcherTwoViewTargetTimeSourceViewConfig {
                    client_id: ClientId("client-right".to_string()),
                    run_id: RunId("run-right".to_string()),
                },
                target_timestamp,
                mode,
            },
        )
    }

    fn render_scheduler_result_for_test(
        scheduler_result: SwitcherTwoViewTargetTimeSourceSchedulerResult,
        decode: &impl SwitcherH264DecodeRuntimeHook,
        render: &impl SwitcherWindowRenderRuntimeHook,
    ) -> SwitcherTwoViewSchedulerDecodeRenderConnectionOutput {
        SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary::default()
            .render_scheduler_result_with_runtimes(
                SwitcherTwoViewSchedulerDecodeRenderConnectionInput {
                    scheduler_result,
                    left_window_title: "left".to_string(),
                    right_window_title: "right".to_string(),
                    render_hold_millis: 5,
                },
                decode,
                render,
            )
    }

    fn display_policy_output_for_test(
        scheduler_result: SwitcherTwoViewTargetTimeSourceSchedulerResult,
        previous_left: Option<SwitcherTwoViewDisplayedFrame>,
        previous_right: Option<SwitcherTwoViewDisplayedFrame>,
        current_time: TimestampMicros,
        max_hold_duration_micros: Option<u64>,
    ) -> SwitcherTwoViewDisplayPolicyOutput {
        let connection = render_scheduler_result_for_test(
            scheduler_result,
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        SwitcherTwoViewDisplayPolicyBoundary.decide(SwitcherTwoViewDisplayPolicyInput {
            connection,
            previous_left,
            previous_right,
            current_time,
            max_hold_duration_micros,
        })
    }

    fn display_composition_adapter_output_for_test(
        display: SwitcherTwoViewDisplayPolicyOutput,
    ) -> SwitcherTwoViewDisplayCompositionAdapterOutput {
        SwitcherTwoViewDisplayCompositionAdapterBoundary.adapt(
            SwitcherTwoViewDisplayCompositionAdapterInput {
                display,
                layout_policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        )
    }

    fn handoff_display_composition_adapter_output_for_test(
        display: SwitcherTwoViewHandoffDisplayPolicyOutput,
    ) -> SwitcherTwoViewHandoffDisplayCompositionAdapterOutput {
        SwitcherTwoViewHandoffDisplayCompositionAdapterBoundary.adapt(
            SwitcherTwoViewHandoffDisplayCompositionAdapterInput {
                display,
                layout_policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        )
    }

    fn previous_displayed_frame(
        side: SwitcherTwoViewSide,
        displayed_at: TimestampMicros,
    ) -> SwitcherTwoViewDisplayedFrame {
        SwitcherTwoViewDisplayedFrame {
            side,
            selected: None,
            decoded: SwitcherDecodedFrame {
                width: 2,
                height: 1,
                pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                pixels: vec![0; 8],
            },
            displayed_at,
        }
    }

    #[derive(Default)]
    struct RecordingTwoViewDecode {
        inputs: std::cell::RefCell<Vec<SwitcherH264DecodeInput>>,
        fail_on_last_byte: Option<u8>,
    }

    impl RecordingTwoViewDecode {
        fn failing_on_last_byte(value: u8) -> Self {
            Self {
                inputs: std::cell::RefCell::new(Vec::new()),
                fail_on_last_byte: Some(value),
            }
        }
    }

    impl SwitcherH264DecodeRuntimeHook for RecordingTwoViewDecode {
        fn decode_annex_b_h264(&self, input: SwitcherH264DecodeInput) -> SwitcherH264DecodeResult {
            self.inputs.borrow_mut().push(input.clone());
            if self.fail_on_last_byte == input.encoded_payload.last().copied() {
                return SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                    message: format!(
                        "fixture decode failed for 0x{:02x}",
                        input.encoded_payload.last().copied().unwrap_or_default()
                    ),
                });
            }
            SwitcherH264DecodeResult::Decoded(SwitcherDecodedFrame {
                width: input.width,
                height: input.height,
                pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                pixels: vec![0; input.width as usize * input.height as usize * 4],
            })
        }
    }

    #[derive(Default)]
    struct RecordingTwoViewRender {
        requests: std::cell::RefCell<Vec<SwitcherWindowRenderRequest>>,
        fail_title_contains: Option<String>,
    }

    impl RecordingTwoViewRender {
        fn failing_when_title_contains(value: &str) -> Self {
            Self {
                requests: std::cell::RefCell::new(Vec::new()),
                fail_title_contains: Some(value.to_string()),
            }
        }
    }

    impl SwitcherWindowRenderRuntimeHook for RecordingTwoViewRender {
        fn render_once(&self, request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            self.requests.borrow_mut().push(request.clone());
            if let Some(value) = &self.fail_title_contains {
                if request.title.contains(value) {
                    return SwitcherWindowRenderResult::RenderFailed {
                        message: format!("fixture render failed for {value}"),
                    };
                }
            }
            SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                width: request.frame.width,
                height: request.frame.height,
                title: request.title,
                hold_millis: request.hold_millis,
            })
        }
    }

    struct PanicDecode;

    impl SwitcherH264DecodeRuntimeHook for PanicDecode {
        fn decode_annex_b_h264(&self, _input: SwitcherH264DecodeInput) -> SwitcherH264DecodeResult {
            panic!("decode should not be called for unavailable 2-view selection");
        }
    }

    struct PanicRender;

    impl SwitcherWindowRenderRuntimeHook for PanicRender {
        fn render_once(&self, _request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            panic!("render should not be called for unavailable 2-view selection");
        }
    }

    #[test]
    fn two_view_composition_composes_both_sides_side_by_side() {
        let result = SwitcherTwoViewCompositionBoundary.compose_side_by_side(
            SwitcherTwoViewCompositionInput {
                left: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Left,
                    selected: None,
                    frame: decoded_bgra_frame(2, 1, [1, 2, 3, 255]),
                },
                right: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Right,
                    selected: None,
                    frame: decoded_bgra_frame(2, 1, [4, 5, 6, 255]),
                },
                policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );

        let SwitcherTwoViewCompositionResult::BothComposed { frame } = result else {
            panic!("expected both-composed result");
        };
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.left.as_ref().map(|left| left.x), Some(0));
        assert_eq!(frame.right.as_ref().map(|right| right.x), Some(2));
        assert_eq!(&frame.pixels[0..8], &[1, 2, 3, 255, 1, 2, 3, 255]);
        assert_eq!(&frame.pixels[8..16], &[4, 5, 6, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn two_view_composition_left_only_keeps_right_placeholder_explicit() {
        let result = SwitcherTwoViewCompositionBoundary.compose_side_by_side(
            SwitcherTwoViewCompositionInput {
                left: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Left,
                    selected: None,
                    frame: decoded_bgra_frame(1, 1, [8, 9, 10, 255]),
                },
                right: SwitcherTwoViewLayoutSideInput::skipped(
                    SwitcherTwoViewSide::Right,
                    SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
                ),
                policy: SwitcherTwoViewLayoutPolicy {
                    placeholder_bgra: [20, 21, 22, 255],
                },
            },
        );

        let SwitcherTwoViewCompositionResult::LeftOnly {
            frame,
            right_placeholder_reason,
        } = result
        else {
            panic!("expected left-only result");
        };
        assert_eq!(
            right_placeholder_reason,
            SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable
        );
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 1);
        assert_eq!(&frame.pixels[0..4], &[8, 9, 10, 255]);
        assert_eq!(&frame.pixels[4..8], &[20, 21, 22, 255]);
        assert!(frame.right.is_none());
    }

    #[test]
    fn two_view_composition_right_only_keeps_left_placeholder_explicit() {
        let result = SwitcherTwoViewCompositionBoundary.compose_side_by_side(
            SwitcherTwoViewCompositionInput {
                left: SwitcherTwoViewLayoutSideInput::skipped(
                    SwitcherTwoViewSide::Left,
                    SwitcherTwoViewManualDecodeRenderStatus::DecodeFailed,
                ),
                right: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Right,
                    selected: None,
                    frame: decoded_bgra_frame(1, 1, [30, 31, 32, 255]),
                },
                policy: SwitcherTwoViewLayoutPolicy {
                    placeholder_bgra: [40, 41, 42, 255],
                },
            },
        );

        let SwitcherTwoViewCompositionResult::RightOnly {
            frame,
            left_placeholder_reason,
        } = result
        else {
            panic!("expected right-only result");
        };
        assert_eq!(
            left_placeholder_reason,
            SwitcherTwoViewManualDecodeRenderStatus::DecodeFailed
        );
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 1);
        assert_eq!(&frame.pixels[0..4], &[40, 41, 42, 255]);
        assert_eq!(&frame.pixels[4..8], &[30, 31, 32, 255]);
        assert!(frame.left.is_none());
    }

    #[test]
    fn two_view_composition_both_missing_remains_empty_placeholder() {
        let result = SwitcherTwoViewCompositionBoundary.compose_side_by_side(
            SwitcherTwoViewCompositionInput {
                left: SwitcherTwoViewLayoutSideInput::skipped(
                    SwitcherTwoViewSide::Left,
                    SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
                ),
                right: SwitcherTwoViewLayoutSideInput::skipped(
                    SwitcherTwoViewSide::Right,
                    SwitcherTwoViewManualDecodeRenderStatus::RenderDeferred,
                ),
                policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );

        assert_eq!(
            result,
            SwitcherTwoViewCompositionResult::EmptyPlaceholder {
                left_reason: SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
                right_reason: SwitcherTwoViewManualDecodeRenderStatus::RenderDeferred,
            }
        );
    }

    #[test]
    fn two_view_composition_rejects_invalid_dimensions() {
        let result = SwitcherTwoViewCompositionBoundary.compose_side_by_side(
            SwitcherTwoViewCompositionInput {
                left: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Left,
                    selected: None,
                    frame: SwitcherDecodedFrame {
                        width: 0,
                        height: 1,
                        pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                        pixels: Vec::new(),
                    },
                },
                right: SwitcherTwoViewLayoutSideInput::skipped(
                    SwitcherTwoViewSide::Right,
                    SwitcherTwoViewManualDecodeRenderStatus::SkippedSelectionUnavailable,
                ),
                policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );

        assert_eq!(
            result,
            SwitcherTwoViewCompositionResult::InvalidDimensions {
                reason: SwitcherTwoViewCompositionInvalidReason::InvalidDimensions {
                    side: SwitcherTwoViewSide::Left,
                }
            }
        );
    }

    #[test]
    fn two_view_composition_preserves_selected_metadata() {
        let (_state, selection) = selected_two_view_fixture(vec![1], vec![2]);
        let SwitcherTwoViewTargetTimeSelectionResult::BothSelected { left, right, .. } = selection
        else {
            panic!("expected both-selected fixture");
        };
        let result = SwitcherTwoViewCompositionBoundary.compose_side_by_side(
            SwitcherTwoViewCompositionInput {
                left: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Left,
                    selected: Some(left.clone()),
                    frame: decoded_bgra_frame(left.frame.width, left.frame.height, [1, 1, 1, 255]),
                },
                right: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Right,
                    selected: Some(right.clone()),
                    frame: decoded_bgra_frame(
                        right.frame.width,
                        right.frame.height,
                        [2, 2, 2, 255],
                    ),
                },
                policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );

        let SwitcherTwoViewCompositionResult::BothComposed { frame } = result else {
            panic!("expected both-composed result");
        };
        assert_eq!(
            frame
                .left
                .as_ref()
                .and_then(|metadata| metadata.selected.as_ref())
                .map(|selected| selected.frame.frame_id),
            Some(left.frame.frame_id)
        );
        assert_eq!(
            frame
                .right
                .as_ref()
                .and_then(|metadata| metadata.selected.as_ref())
                .map(|selected| selected.frame.frame_id),
            Some(right.frame.frame_id)
        );
    }

    #[test]
    fn two_view_composed_frame_converts_into_render_input() {
        let frame = composed_two_view_fixture_frame();
        let input = SwitcherTwoViewComposedFrameRenderInput::from_composed_frame(&frame)
            .expect("composed frame should become render input");

        assert_eq!(input.frame.width, frame.width);
        assert_eq!(input.frame.height, frame.height);
        assert_eq!(
            input.frame.pixel_format,
            SwitcherDecodedFramePixelFormat::Bgra8
        );
        assert_eq!(input.frame.pixels, frame.pixels);
        assert_eq!(input.left.as_ref().map(|metadata| metadata.x), Some(0));
        assert_eq!(input.right.as_ref().map(|metadata| metadata.x), Some(2));
    }

    #[test]
    fn two_view_composed_frame_render_rejects_invalid_dimensions() {
        let mut frame = composed_two_view_fixture_frame();
        frame.width = 0;
        frame.pixels.clear();

        let result = SwitcherTwoViewComposedCanvasRenderBoundary
            .render_composed_frame_with_runtime(
                &frame,
                "StreamSync 2-view",
                16,
                &SwitcherUnavailableWindowRenderRuntimeHook,
            );

        assert_eq!(
            result,
            SwitcherTwoViewComposedCanvasRenderResult::InvalidComposedFrame {
                error: SwitcherTwoViewComposedFrameRenderInputError::InvalidFrame(
                    SwitcherDecodedFrameRenderInputError::InvalidDimensions
                ),
            }
        );
    }

    #[test]
    fn two_view_composed_canvas_render_backend_unavailable_is_explicit() {
        let frame = composed_two_view_fixture_frame();
        let result = SwitcherTwoViewComposedCanvasRenderBoundary
            .render_composed_frame_with_runtime(
                &frame,
                "StreamSync 2-view",
                16,
                &SwitcherUnavailableWindowRenderRuntimeHook,
            );

        assert!(matches!(
            result,
            SwitcherTwoViewComposedCanvasRenderResult::BackendUnavailable {
                reason: SwitcherWindowBackendUnavailableReason::UnsupportedPlatform,
                ..
            }
        ));
    }

    #[test]
    fn two_view_composed_canvas_render_hook_receives_dimensions_and_pixels() {
        #[derive(Default)]
        struct RecordingComposedRender {
            requests: std::cell::RefCell<Vec<SwitcherWindowRenderRequest>>,
        }

        impl SwitcherWindowRenderRuntimeHook for RecordingComposedRender {
            fn render_once(
                &self,
                request: SwitcherWindowRenderRequest,
            ) -> SwitcherWindowRenderResult {
                self.requests.borrow_mut().push(request.clone());
                SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                    width: request.frame.width,
                    height: request.frame.height,
                    title: request.title,
                    hold_millis: request.hold_millis,
                })
            }
        }

        let frame = composed_two_view_fixture_frame();
        let runtime = RecordingComposedRender::default();
        let result = SwitcherTwoViewComposedCanvasRenderBoundary
            .render_composed_frame_with_runtime(&frame, "StreamSync 2-view", 25, &runtime);

        let SwitcherTwoViewComposedCanvasRenderResult::Rendered { render } = result else {
            panic!("expected rendered composed canvas");
        };
        assert_eq!(render.width, frame.width);
        assert_eq!(render.height, frame.height);
        let requests = runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].frame.width, frame.width);
        assert_eq!(requests[0].frame.height, frame.height);
        assert_eq!(requests[0].frame.pixels, frame.pixels);
    }

    #[test]
    fn two_view_composed_canvas_render_stays_separate_from_composition() {
        let frame = SwitcherTwoViewComposedFrame {
            width: 1,
            height: 1,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels: vec![7, 8, 9, 255],
            left: Some(SwitcherTwoViewComposedSideMetadata {
                side: SwitcherTwoViewSide::Left,
                selected: None,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            right: None,
        };
        let runtime = RecordingTwoViewRender::default();
        let result = SwitcherTwoViewComposedCanvasRenderBoundary
            .render_composed_frame_with_runtime(&frame, "precomposed", 10, &runtime);

        assert!(matches!(
            result,
            SwitcherTwoViewComposedCanvasRenderResult::Rendered { .. }
        ));
        assert_eq!(runtime.requests.borrow().len(), 1);
    }

    #[test]
    fn live_two_view_runtime_queues_two_accepted_frames_and_renders_both() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            live_accepted_item("client-left", 10, vec![0, 0, 1, 0x65, 0x10]),
            live_accepted_item("client-right", 12, vec![0, 0, 1, 0x65, 0x12]),
            SwitcherLiveTwoViewQueueSourceItem::EndOfInput,
        ]);

        let result = SwitcherLiveTwoViewRuntimeBoundary::default().run_once(
            SwitcherLiveTwoViewRuntimeInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: live_two_view_test_policy(3),
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(result.queue.accepted_frames, 2);
        assert_eq!(result.queue.rejected_frames, 0);
        assert_eq!(result.queue.queued_left, 1);
        assert_eq!(result.queue.queued_right, 1);
        assert_eq!(result.queue_state.client_queue_len(&left_client_id), 1);
        assert_eq!(result.queue_state.client_queue_len(&right_client_id), 1);
        let SwitcherLiveTwoViewPipelineResult::Rendered { kind, summary, .. } = result.pipeline
        else {
            panic!("both live frames should render");
        };
        assert_eq!(kind, SwitcherLiveTwoViewRenderedKind::Both);
        assert_eq!(
            summary.composition_kind,
            SwitcherLiveTwoViewCompositionKind::Both
        );
    }

    #[test]
    fn live_two_view_runtime_keeps_missing_client_partial_explicit() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            live_accepted_item("client-left", 10, vec![0, 0, 1, 0x65, 0x10]),
            SwitcherLiveTwoViewQueueSourceItem::EndOfInput,
        ]);

        let result = SwitcherLiveTwoViewRuntimeBoundary::default().run_once(
            SwitcherLiveTwoViewRuntimeInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: live_two_view_test_policy(2),
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        let SwitcherLiveTwoViewPipelineResult::Rendered { kind, summary, .. } = result.pipeline
        else {
            panic!("one live frame should render as partial");
        };
        assert_eq!(kind, SwitcherLiveTwoViewRenderedKind::LeftOnly);
        assert!(matches!(
            summary.right,
            SwitcherLiveTwoViewSidePipelineStatus::SelectionUnavailable { .. }
        ));
    }

    #[test]
    fn live_two_view_runtime_rejected_frame_is_not_queued() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            SwitcherLiveTwoViewQueueSourceItem::RejectedVideoFrame {
                client_id: left_client_id.clone(),
                reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
            },
            live_accepted_item("client-right", 12, vec![0, 0, 1, 0x65, 0x12]),
            SwitcherLiveTwoViewQueueSourceItem::EndOfInput,
        ]);

        let result = SwitcherLiveTwoViewRuntimeBoundary::default().run_once(
            SwitcherLiveTwoViewRuntimeInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: live_two_view_test_policy(3),
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(result.queue.rejected_frames, 1);
        assert_eq!(result.queue.queued_left, 0);
        assert_eq!(result.queue.queued_right, 1);
        let SwitcherLiveTwoViewPipelineResult::Rendered { kind, summary, .. } = result.pipeline
        else {
            panic!("right live frame should still render as partial");
        };
        assert_eq!(kind, SwitcherLiveTwoViewRenderedKind::RightOnly);
        assert!(matches!(
            summary.left,
            SwitcherLiveTwoViewSidePipelineStatus::SelectionUnavailable { .. }
        ));
    }

    #[test]
    fn live_two_view_runtime_guard_stops_deterministically() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            live_accepted_item("client-left", 10, vec![0, 0, 1, 0x65, 0x10]),
            live_accepted_item("client-right", 12, vec![0, 0, 1, 0x65, 0x12]),
        ]);

        let result = SwitcherLiveTwoViewRuntimeBoundary::default().run_once(
            SwitcherLiveTwoViewRuntimeInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: live_two_view_test_policy(1),
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(result.queue.packets_observed, 1);
        assert!(result.queue.stopped_by_guard);
        assert_eq!(result.queue.queued_left, 1);
        assert_eq!(result.queue.queued_right, 0);
    }

    #[test]
    fn live_two_view_runtime_decode_failure_stays_per_side_and_partial() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            live_accepted_item("client-left", 10, vec![0, 0, 1, 0x65, 0x10]),
            live_accepted_item("client-right", 12, vec![0, 0, 1, 0x65, 0x12]),
            SwitcherLiveTwoViewQueueSourceItem::EndOfInput,
        ]);

        let result = SwitcherLiveTwoViewRuntimeBoundary::default().run_once(
            SwitcherLiveTwoViewRuntimeInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: live_two_view_test_policy(3),
            },
            &RecordingTwoViewDecode::failing_on_last_byte(0x12),
            &RecordingTwoViewRender::default(),
        );

        let SwitcherLiveTwoViewPipelineResult::Rendered { kind, summary, .. } = result.pipeline
        else {
            panic!("left side should render when right decode fails");
        };
        assert_eq!(kind, SwitcherLiveTwoViewRenderedKind::LeftOnly);
        assert!(matches!(
            summary.right,
            SwitcherLiveTwoViewSidePipelineStatus::DecodeFailed { .. }
        ));
    }

    #[test]
    fn continuous_two_view_scheduler_runs_multiple_ticks_over_live_source() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            live_accepted_item("client-left", 10, vec![0, 0, 1, 0x65, 0x10]),
            live_accepted_item("client-right", 12, vec![0, 0, 1, 0x65, 0x12]),
            live_accepted_item("client-left", 33_344, vec![0, 0, 1, 0x65, 0x20]),
            live_accepted_item("client-right", 33_346, vec![0, 0, 1, 0x65, 0x22]),
        ]);

        let result = SwitcherContinuousTwoViewSchedulingBoundary::default().run_with_runtimes(
            SwitcherContinuousTwoViewSchedulingInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: continuous_two_view_test_policy(2, 10),
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(result.summary.ticks_processed, 2);
        assert_eq!(result.summary.rendered_both, 2);
        assert_eq!(result.summary.rendered_frames, 2);
        assert_eq!(
            result.stop_reason,
            SwitcherContinuousTwoViewSchedulingStopReason::MaxTicksReached
        );
        assert_eq!(
            result.ticks[0].outcome,
            SwitcherContinuousTwoViewTickOutcome::RenderedBoth
        );
        assert_eq!(
            result.ticks[1].current_switcher_time,
            TimestampMicros(1_633_344)
        );
    }

    #[test]
    fn continuous_two_view_scheduler_stops_at_max_rendered_frames() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            live_accepted_item("client-left", 10, vec![0, 0, 1, 0x65, 0x10]),
            live_accepted_item("client-right", 12, vec![0, 0, 1, 0x65, 0x12]),
            live_accepted_item("client-left", 33_344, vec![0, 0, 1, 0x65, 0x20]),
            live_accepted_item("client-right", 33_346, vec![0, 0, 1, 0x65, 0x22]),
        ]);

        let result = SwitcherContinuousTwoViewSchedulingBoundary::default().run_with_runtimes(
            SwitcherContinuousTwoViewSchedulingInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: continuous_two_view_test_policy(4, 1),
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(result.summary.ticks_processed, 1);
        assert_eq!(result.summary.rendered_frames, 1);
        assert_eq!(
            result.stop_reason,
            SwitcherContinuousTwoViewSchedulingStopReason::MaxRenderedFramesReached
        );
    }

    #[test]
    fn continuous_two_view_scheduler_records_partial_and_no_frame_ticks() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            SwitcherLiveTwoViewQueueSourceItem::Timeout,
            live_accepted_item("client-left", 33_344, vec![0, 0, 1, 0x65, 0x10]),
        ]);

        let mut policy = continuous_two_view_test_policy(2, 10);
        policy.live_runtime.max_packets = 1;
        let result = SwitcherContinuousTwoViewSchedulingBoundary::default().run_with_runtimes(
            SwitcherContinuousTwoViewSchedulingInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy,
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(result.summary.no_frame_ticks, 1);
        assert_eq!(result.summary.rendered_partial, 1);
        assert_eq!(
            result.ticks[0].outcome,
            SwitcherContinuousTwoViewTickOutcome::NoFrames
        );
        assert_eq!(
            result.ticks[1].outcome,
            SwitcherContinuousTwoViewTickOutcome::RenderedPartial
        );
    }

    #[test]
    fn continuous_two_view_scheduler_handles_source_end_explicitly() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source =
            ScriptedLiveTwoViewSource::new(vec![SwitcherLiveTwoViewQueueSourceItem::EndOfInput]);

        let result = SwitcherContinuousTwoViewSchedulingBoundary::default().run_with_runtimes(
            SwitcherContinuousTwoViewSchedulingInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: continuous_two_view_test_policy(4, 10),
            },
            &PanicDecode,
            &PanicRender,
        );

        assert_eq!(result.summary.ticks_processed, 1);
        assert_eq!(result.summary.source_end_tick, Some(0));
        assert_eq!(
            result.stop_reason,
            SwitcherContinuousTwoViewSchedulingStopReason::SourceEnded
        );
        assert!(result.ticks[0].source_ended);
        assert_eq!(
            result.ticks[0].outcome,
            SwitcherContinuousTwoViewTickOutcome::NoFrames
        );
    }

    #[test]
    fn continuous_two_view_scheduler_records_decode_failure_without_reinterpreting_runtime() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let mut source = ScriptedLiveTwoViewSource::new(vec![
            live_accepted_item("client-left", 10, vec![0, 0, 1, 0x65, 0x10]),
            live_accepted_item("client-right", 12, vec![0, 0, 1, 0x65, 0x12]),
        ]);

        let result = SwitcherContinuousTwoViewSchedulingBoundary::default().run_with_runtimes(
            SwitcherContinuousTwoViewSchedulingInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: continuous_two_view_test_policy(1, 10),
            },
            &RecordingTwoViewDecode::failing_on_last_byte(0x12),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(result.summary.decode_failed_ticks, 1);
        assert_eq!(
            result.ticks[0].outcome,
            SwitcherContinuousTwoViewTickOutcome::DecodeFailed
        );
        let SwitcherLiveTwoViewPipelineResult::Rendered { kind, summary, .. } =
            &result.ticks[0].runtime.pipeline
        else {
            panic!("one-pass runtime should still preserve partial render detail");
        };
        assert_eq!(*kind, SwitcherLiveTwoViewRenderedKind::LeftOnly);
        assert!(matches!(
            summary.right,
            SwitcherLiveTwoViewSidePipelineStatus::DecodeFailed { .. }
        ));
    }

    #[test]
    fn udp_live_two_view_source_maps_accepted_video_frame() {
        let (receiver, sender) = udp_source_socket_pair();
        let registry = registry_with_clients(
            &["client-left"],
            PacketSource {
                address: sender.local_addr().expect("sender should have address"),
            },
        );
        let mut config = udp_source_test_config(
            receiver.local_addr().expect("receiver should have address"),
            1,
        );
        config.queued_at_base = TimestampMicros(9_000);
        let mut source = SwitcherUdpLiveTwoViewQueueSource::from_socket(receiver, config, registry)
            .expect("source should configure");

        sender
            .send_to(
                &encoded_video_packet("client-left", 10, vec![0, 0, 1, 0x65]),
                source.config.bind_address,
            )
            .expect("video packet should send");

        let item = source.receive_next();

        let SwitcherLiveTwoViewQueueSourceItem::AcceptedVideoFrame { packet, queued_at } = item
        else {
            panic!("accepted video should become source item");
        };
        assert_eq!(packet.frame.client_id, ClientId("client-left".to_string()));
        assert_eq!(packet.frame.frame_id, 10);
        assert_eq!(queued_at, TimestampMicros(9_000));
    }

    #[test]
    fn udp_live_two_view_source_keeps_unauthenticated_video_explicit() {
        let (receiver, sender) = udp_source_socket_pair();
        let config = udp_source_test_config(
            receiver.local_addr().expect("receiver should have address"),
            1,
        );
        let mut source = SwitcherUdpLiveTwoViewQueueSource::from_socket(
            receiver,
            config,
            AuthenticatedSenderRegistry::default(),
        )
        .expect("source should configure");

        sender
            .send_to(
                &encoded_video_packet("client-left", 10, vec![0, 0, 1, 0x65]),
                source.config.bind_address,
            )
            .expect("video packet should send");

        let item = source.receive_next();

        assert_eq!(
            item,
            SwitcherLiveTwoViewQueueSourceItem::RejectedVideoFrame {
                client_id: ClientId("client-left".to_string()),
                reason: PacketAcceptanceRejectReason::UnauthenticatedSource,
            }
        );
    }

    #[test]
    fn udp_live_two_view_source_keeps_protocol_decode_failure_explicit() {
        let (receiver, sender) = udp_source_socket_pair();
        let config = udp_source_test_config(
            receiver.local_addr().expect("receiver should have address"),
            1,
        );
        let mut source = SwitcherUdpLiveTwoViewQueueSource::from_socket(
            receiver,
            config,
            AuthenticatedSenderRegistry::default(),
        )
        .expect("source should configure");

        sender
            .send_to(&[0xaa, 0xbb], source.config.bind_address)
            .expect("malformed packet should send");

        let item = source.receive_next();

        let SwitcherLiveTwoViewQueueSourceItem::ProtocolDecodeFailed {
            source: Some(packet_source),
            message,
        } = item
        else {
            panic!("malformed packet should stay explicit");
        };
        assert_eq!(
            packet_source.address,
            sender.local_addr().expect("sender should have address")
        );
        assert!(message.contains("BufferTooShort"));
    }

    #[test]
    fn udp_live_two_view_source_timeout_is_explicit() {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let config = udp_source_test_config(
            receiver.local_addr().expect("receiver should have address"),
            1,
        );
        let mut source = SwitcherUdpLiveTwoViewQueueSource::from_socket(
            receiver,
            config,
            AuthenticatedSenderRegistry::default(),
        )
        .expect("source should configure");

        assert_eq!(
            source.receive_next(),
            SwitcherLiveTwoViewQueueSourceItem::Timeout
        );
    }

    #[test]
    fn continuous_two_view_scheduler_can_consume_udp_live_source() {
        let left_client_id = ClientId("client-left".to_string());
        let right_client_id = ClientId("client-right".to_string());
        let (receiver, sender) = udp_source_socket_pair();
        let sender_source = PacketSource {
            address: sender.local_addr().expect("sender should have address"),
        };
        let registry = registry_with_clients(&["client-left", "client-right"], sender_source);
        let mut config = udp_source_test_config(
            receiver.local_addr().expect("receiver should have address"),
            2,
        );
        config.queued_at_base = TimestampMicros(10_000);
        let mut source = SwitcherUdpLiveTwoViewQueueSource::from_socket(receiver, config, registry)
            .expect("source should configure");

        sender
            .send_to(
                &encoded_video_packet("client-left", 10, vec![0, 0, 1, 0x65, 0x10]),
                source.config.bind_address,
            )
            .expect("left video should send");
        sender
            .send_to(
                &encoded_video_packet("client-right", 12, vec![0, 0, 1, 0x65, 0x12]),
                source.config.bind_address,
            )
            .expect("right video should send");

        let result = SwitcherContinuousTwoViewSchedulingBoundary::default().run_with_runtimes(
            SwitcherContinuousTwoViewSchedulingInput {
                source: &mut source,
                left_client_id: &left_client_id,
                right_client_id: &right_client_id,
                policy: continuous_two_view_test_policy(1, 10),
            },
            &RecordingTwoViewDecode::default(),
            &RecordingTwoViewRender::default(),
        );

        assert_eq!(result.summary.rendered_both, 1);
        let SwitcherLiveTwoViewPipelineResult::Rendered { kind, .. } =
            &result.ticks[0].runtime.pipeline
        else {
            panic!("udp source should feed existing two-view runtime");
        };
        assert_eq!(*kind, SwitcherLiveTwoViewRenderedKind::Both);
        assert_eq!(result.ticks[0].runtime.queue.accepted_frames, 2);
    }

    #[test]
    fn live_two_view_manual_runtime_initializes_registry_and_preserves_scheduler_summary() {
        let left_client_id = ClientId("player1".to_string());
        let right_client_id = ClientId("player2".to_string());
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let bind_address = receiver.local_addr().expect("receiver should have address");
        let left_sender = UdpSocket::bind("127.0.0.1:0").expect("left sender should bind");
        let right_sender = UdpSocket::bind("127.0.0.1:0").expect("right sender should bind");

        left_sender
            .send_to(
                &encoded_auth_packet("player1", "replace-with-shared-token-1"),
                bind_address,
            )
            .expect("left auth should send");
        right_sender
            .send_to(
                &encoded_auth_packet("player2", "replace-with-shared-token-2"),
                bind_address,
            )
            .expect("right auth should send");
        left_sender
            .send_to(
                &encoded_video_packet("player1", 10, vec![0, 0, 1, 0x65, 0x10]),
                bind_address,
            )
            .expect("left video should send");
        right_sender
            .send_to(
                &encoded_video_packet("player2", 12, vec![0, 0, 1, 0x65, 0x12]),
                bind_address,
            )
            .expect("right video should send");

        let result = SwitcherLiveTwoViewManualRuntimeBoundary::default()
            .run_from_socket_with_runtimes(
                receiver,
                manual_runtime_test_config(bind_address, &left_client_id, &right_client_id),
                &RecordingTwoViewDecode::default(),
                &RecordingTwoViewRender::default(),
            )
            .expect("manual runtime should run");

        assert!(result.bounded_manual_runtime);
        assert_eq!(result.auth.packets_expected, 2);
        assert_eq!(result.auth.packets_processed, 2);
        assert_eq!(result.auth.accepted, 2);
        assert_eq!(result.auth.registered_clients, 2);
        assert_eq!(result.scheduler.summary.ticks_processed, 1);
        assert_eq!(result.scheduler.summary.rendered_both, 1);
        assert_eq!(
            result.scheduler.stop_reason,
            SwitcherContinuousTwoViewSchedulingStopReason::MaxTicksReached
        );
        assert_eq!(result.scheduler.ticks[0].runtime.queue.accepted_frames, 2);
    }

    #[test]
    fn live_two_view_manual_runtime_keeps_rejected_auth_and_unauthenticated_video_explicit() {
        let left_client_id = ClientId("player1".to_string());
        let right_client_id = ClientId("player2".to_string());
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let bind_address = receiver.local_addr().expect("receiver should have address");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");

        sender
            .send_to(&encoded_auth_packet("player1", "wrong-token"), bind_address)
            .expect("bad auth should send");
        sender
            .send_to(
                &encoded_video_packet("player1", 10, vec![0, 0, 1, 0x65, 0x10]),
                bind_address,
            )
            .expect("video should send");

        let mut config =
            manual_runtime_test_config(bind_address, &left_client_id, &right_client_id);
        config.auth_setup_packets = 1;
        config.udp_source_max_packets = 1;
        config.scheduling.max_ticks = 1;
        config.scheduling.live_runtime.max_packets = 1;
        let result = SwitcherLiveTwoViewManualRuntimeBoundary::default()
            .run_from_socket_with_runtimes(
                receiver,
                config,
                &RecordingTwoViewDecode::default(),
                &RecordingTwoViewRender::default(),
            )
            .expect("manual runtime should run with rejected auth");

        assert_eq!(result.auth.rejected, 1);
        assert_eq!(result.auth.registered_clients, 0);
        assert_eq!(result.scheduler.ticks[0].runtime.queue.rejected_frames, 1);
        assert_eq!(result.scheduler.ticks[0].runtime.queue.accepted_frames, 0);
        assert_eq!(result.scheduler.summary.no_frame_ticks, 1);
    }

    #[test]
    fn live_two_view_manual_runtime_surfaces_source_end_stop_reason() {
        let left_client_id = ClientId("player1".to_string());
        let right_client_id = ClientId("player2".to_string());
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let bind_address = receiver.local_addr().expect("receiver should have address");
        let mut config =
            manual_runtime_test_config(bind_address, &left_client_id, &right_client_id);
        config.auth_setup_packets = 0;
        config.udp_source_max_packets = 0;
        config.scheduling.max_ticks = 2;
        config.scheduling.live_runtime.max_packets = 1;

        let result = SwitcherLiveTwoViewManualRuntimeBoundary::default()
            .run_from_socket_with_runtimes(
                receiver,
                config,
                &RecordingTwoViewDecode::default(),
                &RecordingTwoViewRender::default(),
            )
            .expect("manual runtime should run");

        assert_eq!(
            result.scheduler.stop_reason,
            SwitcherContinuousTwoViewSchedulingStopReason::SourceEnded
        );
        assert!(result.scheduler.ticks[0].runtime.queue.ended);
    }

    fn decoded_bgra_frame(width: u32, height: u32, pixel: [u8; 4]) -> SwitcherDecodedFrame {
        let mut pixels = Vec::new();
        for _ in 0..width as usize * height as usize {
            pixels.extend_from_slice(&pixel);
        }
        SwitcherDecodedFrame {
            width,
            height,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels,
        }
    }

    fn composed_two_view_fixture_frame() -> SwitcherTwoViewComposedFrame {
        let result = SwitcherTwoViewCompositionBoundary.compose_side_by_side(
            SwitcherTwoViewCompositionInput {
                left: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Left,
                    selected: None,
                    frame: decoded_bgra_frame(2, 1, [1, 2, 3, 255]),
                },
                right: SwitcherTwoViewLayoutSideInput::Decoded {
                    side: SwitcherTwoViewSide::Right,
                    selected: None,
                    frame: decoded_bgra_frame(2, 1, [4, 5, 6, 255]),
                },
                policy: SwitcherTwoViewLayoutPolicy::default(),
            },
        );
        let SwitcherTwoViewCompositionResult::BothComposed { frame } = result else {
            panic!("expected both-composed fixture");
        };
        frame
    }

    fn live_two_view_test_policy(max_packets: usize) -> SwitcherLiveTwoViewRuntimePolicy {
        SwitcherLiveTwoViewRuntimePolicy {
            max_packets,
            current_switcher_time: TimestampMicros(1_600_011),
            selection: SwitcherTwoViewTargetTimeSelectionPolicy {
                playout_delay_micros: 600_000,
                max_late_micros: 50,
                max_early_micros: 50,
                ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
            },
            queue: ServerVideoFrameQueuePolicy::default(),
            render_hold_millis: 5,
            composition: SwitcherTwoViewLayoutPolicy::default(),
        }
    }

    fn continuous_two_view_test_policy(
        max_ticks: usize,
        max_rendered_frames: usize,
    ) -> SwitcherContinuousTwoViewSchedulingPolicy {
        SwitcherContinuousTwoViewSchedulingPolicy {
            max_ticks,
            max_rendered_frames,
            tick_interval_micros: 33_333,
            live_runtime: live_two_view_test_policy(2),
        }
    }

    fn udp_source_socket_pair() -> (UdpSocket, UdpSocket) {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver should bind");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender should bind");
        (receiver, sender)
    }

    fn udp_source_test_config(
        bind_address: SocketAddr,
        max_packets: usize,
    ) -> SwitcherUdpLiveTwoViewSourceConfig {
        let mut config = SwitcherUdpLiveTwoViewSourceConfig::for_clients(
            bind_address,
            ProtocolVersion(1),
            ClientId("client-left".to_string()),
            ClientId("client-right".to_string()),
        );
        config.max_packets = max_packets;
        config.receive_timeout = Some(Duration::from_millis(20));
        config.buffer_len = 2048;
        config
    }

    fn manual_runtime_test_config(
        bind_address: SocketAddr,
        left_client_id: &ClientId,
        right_client_id: &ClientId,
    ) -> SwitcherLiveTwoViewManualRuntimeConfig {
        let server_config = ServerAuthResponsePocLauncher::default()
            .load_startup_config_from_str(
                r#"
[server]
bind_host = "127.0.0.1"
bind_port = 0

[session]
protocol_version = 1

[auth]
enabled = true
require_known_clients = true

[auth.clients.player1]
shared_token = "replace-with-shared-token-1"

[auth.clients.player2]
shared_token = "replace-with-shared-token-2"
"#,
            )
            .expect("server config should load");
        let mut config = SwitcherLiveTwoViewManualRuntimeConfig::from_server_startup(
            ServerAuthResponsePocStartupConfig {
                bind_address,
                ..server_config
            },
            left_client_id.clone(),
            right_client_id.clone(),
        );
        config.auth_setup_packets = 2;
        config.receive_timeout = Some(Duration::from_millis(20));
        config.udp_source_max_packets = 2;
        config.source_buffer_len = 2048;
        config.scheduling = continuous_two_view_test_policy(1, 10);
        config
    }

    fn registry_with_clients(
        client_ids: &[&str],
        source: PacketSource,
    ) -> AuthenticatedSenderRegistry {
        let mut registry = AuthenticatedSenderRegistry::default();
        for client_id in client_ids {
            AuthenticatedSenderRegistryBoundary.register(
                &mut registry,
                AuthenticatedSenderRegistration {
                    client_id: ClientId((*client_id).to_string()),
                    source,
                    run_id: RunId("run-1".to_string()),
                    protocol_version: ProtocolVersion(1),
                    registered_at: Some(TimestampMicros(1_000)),
                },
            );
        }
        registry
    }

    fn encoded_video_packet(client_id: &str, frame_id: u64, payload: Vec<u8>) -> Vec<u8> {
        let frame = VideoFrame {
            message_type: MessageType::VideoFrame,
            protocol_version: ProtocolVersion(1),
            client_id: ClientId(client_id.to_string()),
            run_id: RunId("run-1".to_string()),
            frame_id,
            capture_timestamp: TimestampMicros(1_000_000 + frame_id),
            send_timestamp: TimestampMicros(1_000_100 + frame_id),
            is_keyframe: true,
            metadata_reserved: [0; 3],
            width: 2,
            height: 1,
            fps_nominal: 30,
            codec: Codec::H264,
            payload_size: payload.len(),
            payload,
        };
        let mut output = Vec::new();
        ProtocolMessageEncoderBoundary
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(1),
                },
                &ProtocolMessage::VideoFrame(frame),
                &mut output,
            )
            .expect("video frame should encode");
        output
    }

    fn encoded_auth_packet(client_id: &str, shared_token: &str) -> Vec<u8> {
        let request = AuthRequest {
            message_type: MessageType::AuthRequest,
            protocol_version: ProtocolVersion(1),
            client_id: ClientId(client_id.to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: AppVersion("0.1.0".to_string()),
            shared_token: shared_token.to_string(),
            display_name: None,
            capabilities: Vec::new(),
            requested_video_profile: None,
        };
        let mut output = Vec::new();
        ProtocolMessageEncoderBoundary
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(1),
                },
                &ProtocolMessage::AuthRequest(request),
                &mut output,
            )
            .expect("auth request should encode");
        output
    }

    fn live_accepted_item(
        client_id: &str,
        frame_id: u64,
        payload: Vec<u8>,
    ) -> SwitcherLiveTwoViewQueueSourceItem {
        SwitcherLiveTwoViewQueueSourceItem::AcceptedVideoFrame {
            packet: registered_video_packet(client_id, frame_id, 2, 1, payload),
            queued_at: TimestampMicros(2_400_000 + frame_id),
        }
    }

    struct ScriptedLiveTwoViewSource {
        items: Vec<SwitcherLiveTwoViewQueueSourceItem>,
    }

    impl ScriptedLiveTwoViewSource {
        fn new(items: Vec<SwitcherLiveTwoViewQueueSourceItem>) -> Self {
            Self { items }
        }
    }

    impl SwitcherLiveTwoViewQueueSource for ScriptedLiveTwoViewSource {
        fn receive_next(&mut self) -> SwitcherLiveTwoViewQueueSourceItem {
            if self.items.is_empty() {
                return SwitcherLiveTwoViewQueueSourceItem::EndOfInput;
            }
            self.items.remove(0)
        }
    }

    struct ScriptedFrameSource {
        results: Vec<SwitcherSingleViewFrameSelectionResult>,
    }

    impl ScriptedFrameSource {
        fn new(results: Vec<SwitcherSingleViewFrameSelectionResult>) -> Self {
            Self { results }
        }
    }

    impl SwitcherContinuousFrameSource for ScriptedFrameSource {
        fn select_latest(
            &mut self,
            client_id: &ClientId,
        ) -> SwitcherSingleViewFrameSelectionResult {
            if self.results.is_empty() {
                return SwitcherSingleViewFrameSelectionResult::NoFrameAvailable {
                    client_id: client_id.clone(),
                };
            }
            self.results.remove(0)
        }
    }

    fn scripted_selected_frame(
        client_id: &ClientId,
        frame_id: u64,
    ) -> SwitcherSingleViewFrameSelectionResult {
        SwitcherSingleViewFrameSelectionResult::FrameAvailable(
            SwitcherSingleViewSelectedEncodedFrame {
                client_id: client_id.clone(),
                run_id: RunId("run-1".to_string()),
                frame_id,
                capture_timestamp: TimestampMicros(1_000_000 + frame_id),
                send_timestamp: TimestampMicros(1_000_100 + frame_id),
                queued_at: TimestampMicros(2_000_000 + frame_id),
                is_keyframe: true,
                width: 2,
                height: 1,
                fps_nominal: 30,
                codec: Codec::H264,
                encoded_payload_len: 4,
                encoded_payload: vec![0, 0, 1, frame_id as u8],
            },
        )
    }

    struct SuccessfulLoopDecode;

    impl SwitcherH264DecodeRuntimeHook for SuccessfulLoopDecode {
        fn decode_annex_b_h264(&self, input: SwitcherH264DecodeInput) -> SwitcherH264DecodeResult {
            SwitcherH264DecodeResult::Decoded(SwitcherDecodedFrame {
                width: input.width,
                height: input.height,
                pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                pixels: vec![0; input.width as usize * input.height as usize * 4],
            })
        }
    }

    struct FailingLoopDecode;

    impl SwitcherH264DecodeRuntimeHook for FailingLoopDecode {
        fn decode_annex_b_h264(&self, _input: SwitcherH264DecodeInput) -> SwitcherH264DecodeResult {
            SwitcherH264DecodeResult::Failed(SwitcherH264DecodeFailure {
                message: "fixture decode failed".to_string(),
            })
        }
    }

    struct SuccessfulLoopRender;

    impl SwitcherWindowRenderRuntimeHook for SuccessfulLoopRender {
        fn render_once(&self, request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                width: request.frame.width,
                height: request.frame.height,
                title: request.title,
                hold_millis: request.hold_millis,
            })
        }
    }

    struct FailingLoopRender;

    impl SwitcherWindowRenderRuntimeHook for FailingLoopRender {
        fn render_once(&self, _request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            SwitcherWindowRenderResult::RenderFailed {
                message: "fixture render failed".to_string(),
            }
        }
    }
}
