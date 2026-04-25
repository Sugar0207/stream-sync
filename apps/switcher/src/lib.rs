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
}
