use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use stream_sync_protocol::{ClientId, TimestampMicros};
use stream_sync_server::{
    ServerQueuedVideoFrame, ServerReceiveAuthVideoQueueOnceStartupOutcome,
    ServerReceiveAuthVideoQueueOnceVideoOutcome, ServerVideoFrameQueueRuntimeResult,
    ServerVideoFrameQueueState, ServerVideoFrameQueueStorageResult,
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
    pub frame_id: u64,
    pub capture_timestamp: TimestampMicros,
    pub send_timestamp: TimestampMicros,
    pub queued_at: TimestampMicros,
    pub is_keyframe: bool,
    pub width: u32,
    pub height: u32,
    pub fps_nominal: u32,
    pub encoded_payload_len: usize,
    pub encoded_payload: Vec<u8>,
}

impl From<&ServerQueuedVideoFrame> for SwitcherSingleViewSelectedEncodedFrame {
    fn from(queued: &ServerQueuedVideoFrame) -> Self {
        Self {
            client_id: queued.frame.client_id.clone(),
            frame_id: queued.frame.frame_id,
            capture_timestamp: queued.frame.capture_timestamp,
            send_timestamp: queued.frame.send_timestamp,
            queued_at: queued.queued_at,
            is_keyframe: queued.frame.is_keyframe,
            width: queued.frame.width,
            height: queued.frame.height,
            fps_nominal: queued.frame.fps_nominal,
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
    use stream_sync_net_core::PacketSource;
    use stream_sync_protocol::{Codec, MessageType, ProtocolVersion, RunId, VideoFrame};
    use stream_sync_server::{
        AuthenticatedSenderEntry, ServerDispatchRuntimeSideEffectApplyResult,
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
            frame_id: 3,
            capture_timestamp: TimestampMicros(1_000_003),
            send_timestamp: TimestampMicros(1_000_103),
            queued_at: TimestampMicros(2_300_000),
            is_keyframe: true,
            width: 1280,
            height: 720,
            fps_nominal: 30,
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

    fn store_frame_with_payload(
        state: &mut ServerVideoFrameQueueState,
        client_id: &str,
        frame_id: u64,
        queued_at: TimestampMicros,
        width: u32,
        height: u32,
        payload: Vec<u8>,
    ) -> ServerVideoFrameQueueStorageResult {
        let source = PacketSource {
            address: "127.0.0.1:5001".parse().unwrap(),
        };
        let payload_size = payload.len();
        let packet = ServerRegisteredVideoFramePacket {
            source,
            authenticated_sender: AuthenticatedSenderEntry {
                client_id: ClientId(client_id.to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(1),
                registered_at: None,
            },
            frame: VideoFrame {
                message_type: MessageType::VideoFrame,
                protocol_version: ProtocolVersion(1),
                client_id: ClientId(client_id.to_string()),
                run_id: RunId("run-1".to_string()),
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
        };
        let input = ServerVideoFrameHandlerBoundary.prepare_input(packet);
        ServerVideoFrameQueueStorageBoundary.store_frame(
            state,
            input,
            queued_at,
            ServerVideoFrameQueuePolicy::default(),
        )
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
                frame_id,
                capture_timestamp: TimestampMicros(1_000_000 + frame_id),
                send_timestamp: TimestampMicros(1_000_100 + frame_id),
                queued_at: TimestampMicros(2_000_000 + frame_id),
                is_keyframe: true,
                width: 2,
                height: 1,
                fps_nominal: 30,
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
