use std::num::NonZeroU32;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use stream_sync_net_core::PacketSource;
use stream_sync_protocol::{
    ClientId, Codec, MessageType, ProtocolVersion, RunId, TimestampMicros, VideoFrame,
};
use stream_sync_server::{
    AuthenticatedSenderEntry, ServerReceiveAuthVideoQueueOnceLauncher,
    ServerRegisteredVideoFramePacket, ServerVideoFrameHandlerBoundary, ServerVideoFrameQueuePolicy,
    ServerVideoFrameQueueState, ServerVideoFrameQueueStorageBoundary,
};
use stream_sync_switcher::{
    SwitcherAuthVideoPlaceholderBridgeBoundary, SwitcherAuthVideoPlaceholderBridgeResult,
    SwitcherContinuousTwoViewSchedulingStopReason, SwitcherDecodeLatestFrameOnceBoundary,
    SwitcherDecodeLatestFrameOnceInput, SwitcherDecodeLatestFrameOnceResult, SwitcherDecodedFrame,
    SwitcherDecodedFramePixelFormat, SwitcherFfmpegH264DecodeRuntimeHook,
    SwitcherFourViewCleanOutputWindowProofBoundary, SwitcherFourViewCleanOutputWindowProofResult,
    SwitcherFourViewCleanOutputWindowRenderResult, SwitcherFourViewComposedCanvasRenderResult,
    SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult,
    SwitcherFourViewManualPreviewCompositionInstructionKind,
    SwitcherFourViewManualPreviewDisplaySlotKind, SwitcherFourViewManualPreviewProofBoundary,
    SwitcherFourViewManualPreviewProofFixtureMode, SwitcherFourViewManualPreviewProofInput,
    SwitcherFourViewManualPreviewProofResult, SwitcherFourViewManualPreviewProofSummary,
    SwitcherFourViewManualPreviewSchedulerSlotKind, SwitcherFourViewQuadLayoutPolicy,
    SwitcherFourViewTargetTimeSourceSlotConfig, SwitcherH264DecodeDeferredReason,
    SwitcherH264DecodeInput, SwitcherH264DecodeResult, SwitcherH264DecodeRuntimeHook,
    SwitcherLiveTwoViewManualRuntimeBoundary, SwitcherLiveTwoViewManualRuntimeResult,
    SwitcherPlaceholderManualVerificationBoundary, SwitcherPlaceholderManualVerificationInput,
    SwitcherPlaceholderManualVerificationResult, SwitcherQueuedFrameHandoffInput,
    SwitcherQueuedFrameHandoffResult, SwitcherSingleClientQueueSourceMode,
    SwitcherTwoViewComposedCanvasRenderBoundary, SwitcherTwoViewComposedCanvasRenderResult,
    SwitcherTwoViewCompositionBoundary, SwitcherTwoViewCompositionInput,
    SwitcherTwoViewCompositionResult, SwitcherTwoViewLayoutPolicy, SwitcherTwoViewLayoutSideInput,
    SwitcherTwoViewManualVerificationBoundary, SwitcherTwoViewManualVerificationInput,
    SwitcherTwoViewManualVerificationResult, SwitcherTwoViewManualVerificationSideSummary,
    SwitcherTwoViewSide, SwitcherTwoViewTargetTimeSelectionPolicy,
    SwitcherUnavailableWindowRenderRuntimeHook, SwitcherWindowRenderBoundary,
    SwitcherWindowRenderResult, SwitcherWindowRenderRuntimeHook,
};
#[cfg(target_os = "windows")]
use stream_sync_switcher::{
    SwitcherNamedPipeQueuedFrameHandoff, SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    SwitcherNamedPipeQueuedFrameHandoffRequestOutput,
    SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
    SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
    SwitcherNamedPipeQueuedFrameHandoffRetryClassification,
};

#[cfg(target_os = "windows")]
use stream_sync_switcher::SwitcherWindowsGdiWindowRenderRuntimeHook;

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--placeholder-fixture-once") => {
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let queue_state = fixture_queue_state(&client_id);
            let result = SwitcherPlaceholderManualVerificationBoundary::default()
                .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                    queue_state: &queue_state,
                    client_id: &client_id,
                });
            print_summary(result, true);
        }
        Some("--placeholder-empty-once") => {
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let queue_state = ServerVideoFrameQueueState::default();
            let result = SwitcherPlaceholderManualVerificationBoundary::default()
                .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                    queue_state: &queue_state,
                    client_id: &client_id,
                });
            print_summary(result, true);
        }
        Some("--decode-latest-frame-once") => {
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let output_path = args.next().unwrap_or_else(|| "frame_dump.bmp".to_string());
            let queue_state = fixture_queue_state(&client_id);
            let result = SwitcherDecodeLatestFrameOnceBoundary::default()
                .decode_latest_with_runtime(
                    SwitcherDecodeLatestFrameOnceInput {
                        queue_state: &queue_state,
                        client_id: &client_id,
                        output_path: output_path.into(),
                    },
                    &SwitcherFfmpegH264DecodeRuntimeHook::default(),
                );
            print_decode_summary(result, true);
        }
        Some("--receive-auth-video-placeholder-bridge-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let launcher = ServerReceiveAuthVideoQueueOnceLauncher::default();
            match launcher.run_once_from_path_with_writers(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
            ) {
                Ok(server_outcome) => {
                    let result = SwitcherAuthVideoPlaceholderBridgeBoundary::default()
                        .verify_server_outcome(&server_outcome, &client_id);
                    print_bridge_summary(result);
                }
                Err(error) => {
                    eprintln!("switcher auth/video placeholder bridge failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--receive-auth-video-decode-latest-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let output_path = args.next().unwrap_or_else(|| "frame_dump.bmp".to_string());
            let launcher = ServerReceiveAuthVideoQueueOnceLauncher::default();
            match launcher.run_once_from_path_with_writers(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
            ) {
                Ok(server_outcome) => {
                    let result = SwitcherDecodeLatestFrameOnceBoundary::default()
                        .decode_latest_with_runtime(
                            SwitcherDecodeLatestFrameOnceInput {
                                queue_state: &server_outcome.video_queue_state,
                                client_id: &client_id,
                                output_path: output_path.into(),
                            },
                            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
                        );
                    print_decode_summary(result, false);
                }
                Err(error) => {
                    eprintln!("switcher auth/video decode latest failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--receive-auth-video-render-decoded-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let hold_millis = args
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2_000);
            let launcher = ServerReceiveAuthVideoQueueOnceLauncher::default();
            match launcher.run_once_from_path_with_writers(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
            ) {
                Ok(server_outcome) => {
                    let decode = SwitcherDecodeLatestFrameOnceBoundary::default()
                        .decode_latest_with_runtime(
                            SwitcherDecodeLatestFrameOnceInput {
                                queue_state: &server_outcome.video_queue_state,
                                client_id: &client_id,
                                output_path: "frame_dump.bmp".into(),
                            },
                            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
                        );
                    match decode {
                        SwitcherDecodeLatestFrameOnceResult::Decoded { handoff, .. } => {
                            let render = render_decoded_frame_once(
                                &handoff.decoded,
                                "StreamSync Switcher",
                                hold_millis,
                            );
                            print_render_summary(
                                render,
                                &client_id,
                                Some(handoff.selected.frame_id),
                            );
                        }
                        other => {
                            print_decode_summary(other, false);
                        }
                    }
                }
                Err(error) => {
                    eprintln!("switcher auth/video render decoded failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--two-view-sync-fixture-once") => {
            let left_client_id = ClientId(args.next().unwrap_or_else(|| "client-left".to_string()));
            let right_client_id =
                ClientId(args.next().unwrap_or_else(|| "client-right".to_string()));
            let hold_millis = args
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2_000);
            let queue_state = fixture_two_view_queue_state(&left_client_id, &right_client_id);
            let result = verify_two_view_fixture_once(
                &queue_state,
                &left_client_id,
                &right_client_id,
                hold_millis,
            );
            print_two_view_sync_summary(result);
        }
        Some("--render-two-view-composed-fixture-once") => {
            let hold_millis = args
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2_000);
            match fixture_two_view_composed_frame() {
                SwitcherTwoViewCompositionResult::BothComposed { frame } => {
                    let result = render_two_view_composed_frame_once(
                        &frame,
                        "StreamSync Switcher 2-view",
                        hold_millis,
                    );
                    print_two_view_composed_render_summary(result);
                }
                other => {
                    eprintln!("switcher render two-view composed fixture compose_failed={other:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--live-two-view-switcher-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let left_client_id = ClientId(args.next().unwrap_or_else(|| "player1".to_string()));
            let right_client_id = ClientId(args.next().unwrap_or_else(|| "player2".to_string()));
            match run_live_two_view_switcher_once(&config_path, left_client_id, right_client_id) {
                Ok(result) => print_live_two_view_switcher_summary(result),
                Err(error) => {
                    eprintln!("switcher live two-view manual runtime failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--four-view-proof-fixture-once") => {
            let fixture_mode = parse_four_view_manual_preview_fixture_mode_or_exit(args.next());
            let result = run_four_view_manual_preview_proof_once(fixture_mode);
            println!(
                "{}",
                format_four_view_manual_preview_proof_summary(&result.summary)
            );
        }
        Some("--four-view-proof-window-once") => {
            let fixture_mode = parse_four_view_actual_window_fixture_mode_or_exit(args.next());
            let result = run_four_view_manual_preview_window_once(fixture_mode);
            println!(
                "{}",
                format_four_view_manual_preview_window_summary(fixture_mode, &result)
            );
        }
        Some("--four-view-clean-output-window-once") => {
            let fixture_mode = parse_four_view_actual_window_fixture_mode_or_exit(args.next());
            let result = run_four_view_clean_output_window_once(fixture_mode);
            println!(
                "{}",
                format_four_view_clean_output_window_summary(fixture_mode, &result)
            );
        }
        Some("--four-view-clean-output-window-loop") => {
            let fixture_mode = parse_four_view_actual_window_fixture_mode_or_exit(args.next());
            let frames = parse_positive_u32_arg_or_exit(args.next(), "frames");
            let summary = run_four_view_clean_output_window_loop(fixture_mode, frames);
            println!(
                "{}",
                format_four_view_clean_output_window_loop_summary(&summary)
            );
        }
        Some("--read-queued-frame-handoff-once") => {
            let pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing pipe-name");
                std::process::exit(1);
            });
            let client_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client-id");
                std::process::exit(1);
            }));
            let run_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run-id");
                std::process::exit(1);
            }));
            let mode = parse_handoff_mode_or_exit(args.next(), "read-mode");
            let request_id = parse_optional_arg_or_exit::<u64>(args.next(), "request-id");
            let input = SwitcherQueuedFrameHandoffInput {
                client_id,
                run_id,
                mode,
            };
            match run_named_pipe_handoff_once(&pipe_name, input, request_id) {
                Ok(summary) => println!("{summary}"),
                Err(error) => {
                    eprintln!("switcher queued-frame handoff once failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!(
                "stream-sync-switcher scaffold; use --placeholder-fixture-once [client-id], --placeholder-empty-once [client-id], --decode-latest-frame-once [client-id] [output-path], --receive-auth-video-placeholder-bridge-once [config-path] [client-id], --receive-auth-video-decode-latest-once [config-path] [client-id] [output-path], --receive-auth-video-render-decoded-once [config-path] [client-id] [hold-ms], --two-view-sync-fixture-once [left-client-id] [right-client-id] [hold-ms], --render-two-view-composed-fixture-once [hold-ms], --live-two-view-switcher-once [config-path] [left-client-id] [right-client-id], --four-view-proof-fixture-once [all-renderable|mixed-placeholder-source-error|placeholder-only], --four-view-proof-window-once [all-renderable], --four-view-clean-output-window-once [all-renderable], --four-view-clean-output-window-loop [all-renderable] [frames], or --read-queued-frame-handoff-once [pipe-name] [client-id] [run-id] [read-mode] [request-id]"
            );
        }
    }
}

const DEFAULT_ONE_SHOT_REQUEST_ID: u64 = 1;

fn print_summary(result: SwitcherPlaceholderManualVerificationResult, fixture_queue: bool) {
    match result {
        SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } => {
            println!(
                "switcher placeholder helper fixture_queue={} cross_process_queue=false no_frame={} selected_client_id={} frame_id={} payload_len={} decode_status={:?}",
                fixture_queue,
                summary.no_frame,
                summary.selected_client_id.0,
                summary.frame_id.unwrap_or(0),
                summary.encoded_payload_len.unwrap_or(0),
                handoff.decode_status
            );
        }
        SwitcherPlaceholderManualVerificationResult::NoFrame { summary } => {
            println!(
                "switcher placeholder helper fixture_queue={} cross_process_queue=false no_frame={} selected_client_id={} frame_id=none payload_len=none decode_status=none",
                fixture_queue, summary.no_frame, summary.selected_client_id.0
            );
        }
    }
}

fn print_bridge_summary(result: SwitcherAuthVideoPlaceholderBridgeResult) {
    match result {
        SwitcherAuthVideoPlaceholderBridgeResult::PlaceholderReady { summary, .. } => {
            println!(
                "switcher auth/video placeholder bridge in_process=true cross_process_queue=false auth_accepted={} video_received={} video_accepted={} video_rejected={} queued={} queue_len={} dropped_oldest={} no_frame={} selected_client_id={} frame_id={} payload_len={} decode_status={:?}",
                summary.auth_accepted,
                summary.video_received,
                summary.video_accepted,
                summary.video_rejected,
                summary.queued,
                summary.queue_len,
                summary.dropped_oldest,
                summary.no_frame,
                summary.selected_client_id.0,
                summary.selected_frame_id.unwrap_or(0),
                summary.payload_len.unwrap_or(0),
                summary.decode_status
            );
        }
        SwitcherAuthVideoPlaceholderBridgeResult::NoFrame { summary } => {
            println!(
                "switcher auth/video placeholder bridge in_process=true cross_process_queue=false auth_accepted={} video_received={} video_accepted={} video_rejected={} queued={} queue_len={} dropped_oldest={} no_frame={} selected_client_id={} frame_id=none payload_len=none decode_status=none",
                summary.auth_accepted,
                summary.video_received,
                summary.video_accepted,
                summary.video_rejected,
                summary.queued,
                summary.queue_len,
                summary.dropped_oldest,
                summary.no_frame,
                summary.selected_client_id.0
            );
        }
    }
}

fn print_decode_summary(result: SwitcherDecodeLatestFrameOnceResult, fixture_queue: bool) {
    match result {
        SwitcherDecodeLatestFrameOnceResult::Decoded { summary, dump, .. } => {
            println!(
                "switcher decode latest frame fixture_queue={} cross_process_queue=false decoded=true no_frame={} selected_client_id={} frame_id={} payload_len={} width={} height={} output_path={} output_bytes={} decode_status={:?}",
                fixture_queue,
                summary.no_frame,
                summary.selected_client_id.0,
                summary.frame_id.unwrap_or(0),
                summary.encoded_payload_len.unwrap_or(0),
                summary.width.unwrap_or(0),
                summary.height.unwrap_or(0),
                dump.path.display(),
                dump.bytes_written,
                summary.decode_status
            );
        }
        SwitcherDecodeLatestFrameOnceResult::PlaceholderFallback { summary, .. } => {
            println!(
                "switcher decode latest frame fixture_queue={} cross_process_queue=false decoded=false fallback=placeholder no_frame={} selected_client_id={} frame_id={} payload_len={} decode_status={:?}",
                fixture_queue,
                summary.no_frame,
                summary.selected_client_id.0,
                summary.frame_id.unwrap_or(0),
                summary.encoded_payload_len.unwrap_or(0),
                summary.decode_status
            );
            std::process::exit(1);
        }
        SwitcherDecodeLatestFrameOnceResult::NoFrame { summary } => {
            println!(
                "switcher decode latest frame fixture_queue={} cross_process_queue=false decoded=false no_frame={} selected_client_id={} frame_id=none payload_len=none decode_status=none",
                fixture_queue, summary.no_frame, summary.selected_client_id.0
            );
            std::process::exit(1);
        }
        SwitcherDecodeLatestFrameOnceResult::DumpFailed { summary, error, .. } => {
            eprintln!(
                "switcher decode latest frame dump failed selected_client_id={} frame_id={} output_path={:?} error={:?}",
                summary.selected_client_id.0,
                summary.frame_id.unwrap_or(0),
                summary.output_path,
                error
            );
            std::process::exit(1);
        }
    }
}

fn render_decoded_frame_once(
    frame: &stream_sync_switcher::SwitcherDecodedFrame,
    title: &str,
    hold_millis: u64,
) -> SwitcherWindowRenderResult {
    #[cfg(target_os = "windows")]
    {
        SwitcherWindowRenderBoundary.render_decoded_frame_with_runtime(
            frame,
            title,
            hold_millis,
            &SwitcherWindowsGdiWindowRenderRuntimeHook,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        SwitcherWindowRenderBoundary.render_decoded_frame_with_runtime(
            frame,
            title,
            hold_millis,
            &SwitcherUnavailableWindowRenderRuntimeHook,
        )
    }
}

fn print_render_summary(
    result: SwitcherWindowRenderResult,
    client_id: &ClientId,
    frame_id: Option<u64>,
) {
    match result {
        SwitcherWindowRenderResult::Rendered(rendered) => {
            println!(
                "switcher render decoded frame rendered=true selected_client_id={} frame_id={} width={} height={} hold_millis={} title={}",
                client_id.0,
                frame_id.unwrap_or(0),
                rendered.width,
                rendered.height,
                rendered.hold_millis,
                rendered.title
            );
        }
        SwitcherWindowRenderResult::RenderDeferred { reason } => {
            eprintln!(
                "switcher render decoded frame deferred selected_client_id={} frame_id={} reason={reason:?}",
                client_id.0,
                frame_id.unwrap_or(0)
            );
            std::process::exit(1);
        }
        SwitcherWindowRenderResult::BackendUnavailable { reason, message } => {
            eprintln!(
                "switcher render decoded frame backend unavailable selected_client_id={} frame_id={} reason={reason:?} message={}",
                client_id.0,
                frame_id.unwrap_or(0),
                message.as_deref().unwrap_or("none")
            );
            std::process::exit(1);
        }
        SwitcherWindowRenderResult::InvalidFrame { error } => {
            eprintln!(
                "switcher render decoded frame invalid selected_client_id={} frame_id={} error={error:?}",
                client_id.0,
                frame_id.unwrap_or(0)
            );
            std::process::exit(1);
        }
        SwitcherWindowRenderResult::RenderFailed { message } => {
            eprintln!(
                "switcher render decoded frame failed selected_client_id={} frame_id={} message={message}",
                client_id.0,
                frame_id.unwrap_or(0)
            );
            std::process::exit(1);
        }
    }
}

fn render_two_view_composed_frame_once(
    frame: &stream_sync_switcher::SwitcherTwoViewComposedFrame,
    title: &str,
    hold_millis: u64,
) -> SwitcherTwoViewComposedCanvasRenderResult {
    #[cfg(target_os = "windows")]
    {
        SwitcherTwoViewComposedCanvasRenderBoundary.render_composed_frame_with_runtime(
            frame,
            title,
            hold_millis,
            &SwitcherWindowsGdiWindowRenderRuntimeHook,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        SwitcherTwoViewComposedCanvasRenderBoundary.render_composed_frame_with_runtime(
            frame,
            title,
            hold_millis,
            &SwitcherUnavailableWindowRenderRuntimeHook,
        )
    }
}

fn print_two_view_composed_render_summary(result: SwitcherTwoViewComposedCanvasRenderResult) {
    match result {
        SwitcherTwoViewComposedCanvasRenderResult::Rendered { render } => {
            println!(
                "switcher render two-view composed fixture rendered=true width={} height={} hold_millis={} title={} source=fixture",
                render.width, render.height, render.hold_millis, render.title
            );
        }
        SwitcherTwoViewComposedCanvasRenderResult::RenderDeferred { reason } => {
            eprintln!("switcher render two-view composed fixture deferred reason={reason:?}");
            std::process::exit(1);
        }
        SwitcherTwoViewComposedCanvasRenderResult::BackendUnavailable { reason, message } => {
            eprintln!(
                "switcher render two-view composed fixture backend_unavailable reason={reason:?} message={}",
                message.as_deref().unwrap_or("none")
            );
            std::process::exit(1);
        }
        SwitcherTwoViewComposedCanvasRenderResult::InvalidComposedFrame { error } => {
            eprintln!("switcher render two-view composed fixture invalid_frame error={error:?}");
            std::process::exit(1);
        }
        SwitcherTwoViewComposedCanvasRenderResult::RenderFailed { message } => {
            eprintln!("switcher render two-view composed fixture failed message={message}");
            std::process::exit(1);
        }
    }
}

fn run_live_two_view_switcher_once(
    config_path: &str,
    left_client_id: ClientId,
    right_client_id: ClientId,
) -> Result<
    SwitcherLiveTwoViewManualRuntimeResult,
    stream_sync_switcher::SwitcherLiveTwoViewManualRuntimeError,
> {
    #[cfg(target_os = "windows")]
    {
        SwitcherLiveTwoViewManualRuntimeBoundary::default().run_from_path_with_runtimes(
            config_path,
            left_client_id,
            right_client_id,
            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
            &SwitcherWindowsGdiWindowRenderRuntimeHook,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        SwitcherLiveTwoViewManualRuntimeBoundary::default().run_from_path_with_runtimes(
            config_path,
            left_client_id,
            right_client_id,
            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
            &SwitcherUnavailableWindowRenderRuntimeHook,
        )
    }
}

fn print_live_two_view_switcher_summary(result: SwitcherLiveTwoViewManualRuntimeResult) {
    let queue_totals = result.scheduler.ticks.iter().fold(
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize),
        |acc, tick| {
            (
                acc.0 + tick.runtime.queue.packets_observed,
                acc.1 + tick.runtime.queue.accepted_frames,
                acc.2 + tick.runtime.queue.rejected_frames,
                acc.3 + tick.runtime.queue.protocol_decode_failures,
                acc.4 + tick.runtime.queue.receive_failures,
                acc.5 + tick.runtime.queue.non_video_packets,
                acc.6 + tick.runtime.queue.timeouts,
            )
        },
    );
    println!(
        "switcher live two-view manual runtime bounded_manual_runtime={} bind_address={} left_client_id={} right_client_id={} auth_packets_processed={} auth_accepted={} auth_rejected={} auth_registered_clients={} auth_receive_failures={} auth_gate_rejections={} packets_processed={} accepted_frames={} rejected_frames={} protocol_decode_failures={} receive_failures={} non_video_packets={} timeouts={} ticks_processed={} rendered_both={} rendered_partial={} no_frame={} decode_failed={} render_not_completed={} stop_reason={} source_ended={}",
        result.bounded_manual_runtime,
        result.bind_address,
        result.left_client_id.0,
        result.right_client_id.0,
        result.auth.packets_processed,
        result.auth.accepted,
        result.auth.rejected,
        result.auth.registered_clients,
        result.auth.receive_failures,
        result.auth.rejected_by_gate,
        queue_totals.0,
        queue_totals.1,
        queue_totals.2,
        queue_totals.3,
        queue_totals.4,
        queue_totals.5,
        queue_totals.6,
        result.scheduler.summary.ticks_processed,
        result.scheduler.summary.rendered_both,
        result.scheduler.summary.rendered_partial,
        result.scheduler.summary.no_frame_ticks,
        result.scheduler.summary.decode_failed_ticks,
        result.scheduler.summary.render_not_completed_ticks,
        format_live_two_view_stop_reason(result.scheduler.stop_reason),
        matches!(
            result.scheduler.stop_reason,
            SwitcherContinuousTwoViewSchedulingStopReason::SourceEnded
        )
    );
}

fn parse_optional_arg_or_exit<T: std::str::FromStr>(value: Option<String>, name: &str) -> Option<T>
where
    T::Err: std::fmt::Display,
{
    value.map(|value| {
        value.parse::<T>().unwrap_or_else(|error| {
            eprintln!("invalid {name}: {error}");
            std::process::exit(1);
        })
    })
}

fn parse_positive_u32_arg(value: Option<String>, name: &str) -> Result<NonZeroU32, String> {
    let value = value.ok_or_else(|| format!("missing {name}"))?;
    let parsed = value
        .parse::<u32>()
        .map_err(|error| format!("invalid {name}: {error}"))?;
    NonZeroU32::new(parsed).ok_or_else(|| format!("invalid {name}: expected positive integer"))
}

fn parse_positive_u32_arg_or_exit(value: Option<String>, name: &str) -> NonZeroU32 {
    parse_positive_u32_arg(value, name).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    })
}

fn parse_handoff_mode_or_exit(
    value: Option<String>,
    name: &str,
) -> SwitcherSingleClientQueueSourceMode {
    match value.as_deref() {
        Some("preview-oldest") => SwitcherSingleClientQueueSourceMode::PreviewOldest,
        Some("preview-latest") => SwitcherSingleClientQueueSourceMode::PreviewLatest,
        Some("consume-oldest") => SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        Some(_) => {
            eprintln!("invalid {name}: expected preview-oldest, preview-latest, or consume-oldest");
            std::process::exit(1);
        }
        None => {
            eprintln!("missing {name}");
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn run_named_pipe_handoff_once(
    pipe_name: &str,
    input: SwitcherQueuedFrameHandoffInput,
    request_id: Option<u64>,
) -> Result<String, String> {
    let mut handoff =
        SwitcherNamedPipeQueuedFrameHandoff::new(pipe_name, DEFAULT_ONE_SHOT_REQUEST_ID);
    let config = SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default();
    let output = match request_id {
        Some(request_id) => {
            handoff.read_handoff_frame_with_request_id_and_config(request_id, input, config)
        }
        None => handoff.read_handoff_frame_with_config(input, config),
    };

    Ok(format_named_pipe_handoff_switcher_summary(&output))
}

#[cfg(not(windows))]
fn run_named_pipe_handoff_once(
    _pipe_name: &str,
    _input: SwitcherQueuedFrameHandoffInput,
    _request_id: Option<u64>,
) -> Result<String, String> {
    Err("named-pipe handoff command is only available on Windows".to_string())
}

#[cfg(test)]
fn format_handoff_mode(mode: SwitcherSingleClientQueueSourceMode) -> &'static str {
    match mode {
        SwitcherSingleClientQueueSourceMode::PreviewOldest => "preview-oldest",
        SwitcherSingleClientQueueSourceMode::PreviewLatest => "preview-latest",
        SwitcherSingleClientQueueSourceMode::ConsumeOldest => "consume-oldest",
    }
}

fn handoff_read_mode_from_switcher_mode(
    mode: SwitcherSingleClientQueueSourceMode,
) -> stream_sync_net_core::ServerSwitcherQueuedFrameReadMode {
    match mode {
        SwitcherSingleClientQueueSourceMode::PreviewOldest => {
            stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectOldest
        }
        SwitcherSingleClientQueueSourceMode::PreviewLatest => {
            stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatest
        }
        SwitcherSingleClientQueueSourceMode::ConsumeOldest => {
            stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::DequeueOldest
        }
    }
}

#[cfg(windows)]
fn format_named_pipe_handoff_switcher_summary(
    output: &SwitcherNamedPipeQueuedFrameHandoffRequestOutput,
) -> String {
    let last_error = format_named_pipe_last_error(output.summary.last_error.as_ref());
    format_named_pipe_handoff_switcher_result_summary(
        &output.summary.pipe_name,
        output.summary.request_id,
        result_client_id(&output.result),
        result_run_id(&output.result),
        handoff_read_mode_from_switcher_mode(output.summary.read_mode),
        format_named_pipe_request_status(output.summary.request_status),
        format_named_pipe_response_status(output.summary.response_status),
        output.summary.attempt_count,
        output.summary.timeout_millis,
        output.summary.elapsed_millis,
        format_named_pipe_final_result(output.summary.final_result),
        &last_error,
        format_named_pipe_retry_classification(output.summary.retry_classification),
        &output.result,
    )
}

fn format_named_pipe_handoff_switcher_result_summary(
    pipe_name: &str,
    request_id: u64,
    client_id: &ClientId,
    run_id: &RunId,
    read_mode: stream_sync_net_core::ServerSwitcherQueuedFrameReadMode,
    request_status: &str,
    response_status: &str,
    attempt_count: u32,
    timeout_millis: u32,
    elapsed_millis: u64,
    final_result: &str,
    last_error: &str,
    retry_classification: &str,
    result: &SwitcherQueuedFrameHandoffResult,
) -> String {
    match result {
        SwitcherQueuedFrameHandoffResult::FrameRead {
            frame,
            remaining_client_queue_len,
            ..
        } => format!(
            "switcher named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} attempt_count={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=FrameRead final_result={} last_error={} retry_classification={} queue_len={} frame_id={} capture_timestamp={} send_timestamp={} queued_at={} width={} height={} fps_nominal={} codec={:?} is_keyframe={} encoded_payload_len={}",
            pipe_name,
            request_id,
            client_id.0,
            run_id.0,
            format_handoff_read_mode(read_mode),
            attempt_count,
            timeout_millis,
            elapsed_millis,
            request_status,
            response_status,
            final_result,
            last_error,
            retry_classification,
            remaining_client_queue_len,
            frame.frame_id,
            frame.capture_timestamp.0,
            frame.send_timestamp.0,
            frame.queued_at.0,
            frame.width,
            frame.height,
            frame.fps_nominal,
            frame.codec,
            frame.is_keyframe,
            frame.encoded_payload_len
        ),
        SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
            client_queue_len, ..
        } => format!(
            "switcher named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} attempt_count={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=NoFrame final_result={} last_error={} retry_classification={} queue_len={}",
            pipe_name,
            request_id,
            client_id.0,
            run_id.0,
            format_handoff_read_mode(read_mode),
            attempt_count,
            timeout_millis,
            elapsed_millis,
            request_status,
            response_status,
            final_result,
            last_error,
            retry_classification,
            client_queue_len
        ),
        SwitcherQueuedFrameHandoffResult::HandoffError { error, .. } => format!(
            "switcher named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} attempt_count={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=HandoffError final_result={} last_error={} retry_classification={} queue_len=none handoff_error={:?}",
            pipe_name,
            request_id,
            client_id.0,
            run_id.0,
            format_handoff_read_mode(read_mode),
            attempt_count,
            timeout_millis,
            elapsed_millis,
            request_status,
            response_status,
            final_result,
            last_error,
            retry_classification,
            error
        ),
    }
}

#[cfg(windows)]
fn result_client_id(result: &SwitcherQueuedFrameHandoffResult) -> &ClientId {
    match result {
        SwitcherQueuedFrameHandoffResult::FrameRead { frame, .. } => &frame.client_id,
        SwitcherQueuedFrameHandoffResult::NoFrameAvailable { client_id, .. } => client_id,
        SwitcherQueuedFrameHandoffResult::HandoffError { client_id, .. } => client_id,
    }
}

#[cfg(windows)]
fn result_run_id(result: &SwitcherQueuedFrameHandoffResult) -> &RunId {
    match result {
        SwitcherQueuedFrameHandoffResult::FrameRead { frame, .. } => &frame.run_id,
        SwitcherQueuedFrameHandoffResult::NoFrameAvailable { run_id, .. } => run_id,
        SwitcherQueuedFrameHandoffResult::HandoffError { run_id, .. } => run_id,
    }
}

#[cfg(windows)]
fn format_named_pipe_request_status(
    status: SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
) -> &'static str {
    match status {
        SwitcherNamedPipeQueuedFrameHandoffRequestStatus::Sent => "sent",
        SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodedOnly => "encoded",
        SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodeFailed => "encode_failed",
    }
}

#[cfg(windows)]
fn format_named_pipe_response_status(
    status: SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
) -> &'static str {
    match status {
        SwitcherNamedPipeQueuedFrameHandoffResponseStatus::Decoded => "decoded",
        SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None => "none",
    }
}

#[cfg(windows)]
fn format_named_pipe_final_result(
    result: stream_sync_switcher::SwitcherNamedPipeQueuedFrameHandoffResultKind,
) -> &'static str {
    match result {
        stream_sync_switcher::SwitcherNamedPipeQueuedFrameHandoffResultKind::FrameRead => {
            "FrameRead"
        }
        stream_sync_switcher::SwitcherNamedPipeQueuedFrameHandoffResultKind::NoFrameAvailable => {
            "NoFrame"
        }
        stream_sync_switcher::SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError => {
            "HandoffError"
        }
    }
}

#[cfg(windows)]
fn format_named_pipe_last_error(
    error: Option<&stream_sync_switcher::SwitcherQueuedFrameHandoffError>,
) -> String {
    error
        .map(|error| format!("{error:?}"))
        .unwrap_or_else(|| "none".to_string())
}

#[cfg(windows)]
fn format_named_pipe_retry_classification(
    classification: Option<SwitcherNamedPipeQueuedFrameHandoffRetryClassification>,
) -> &'static str {
    match classification {
        Some(
            SwitcherNamedPipeQueuedFrameHandoffRetryClassification::RetryableLaterSchedulerTick,
        ) => "RetryableLaterSchedulerTick",
        Some(SwitcherNamedPipeQueuedFrameHandoffRetryClassification::NonRetryable) => {
            "NonRetryable"
        }
        None => "none",
    }
}

fn format_handoff_read_mode(
    mode: stream_sync_net_core::ServerSwitcherQueuedFrameReadMode,
) -> &'static str {
    match mode {
        stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectOldest => "inspect-oldest",
        stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatest => "inspect-latest",
        stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::DequeueOldest => "dequeue-oldest",
    }
}

fn format_live_two_view_stop_reason(
    reason: SwitcherContinuousTwoViewSchedulingStopReason,
) -> &'static str {
    match reason {
        SwitcherContinuousTwoViewSchedulingStopReason::MaxTicksReached => "MaxTicksReached",
        SwitcherContinuousTwoViewSchedulingStopReason::MaxRenderedFramesReached => {
            "MaxRenderedFramesReached"
        }
        SwitcherContinuousTwoViewSchedulingStopReason::SourceEnded => "SourceEnded",
    }
}

fn parse_four_view_manual_preview_fixture_mode_or_exit(
    value: Option<String>,
) -> SwitcherFourViewManualPreviewProofFixtureMode {
    match value.as_deref() {
        Some("all-renderable") | None => {
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable
        }
        Some("mixed-placeholder-source-error") => {
            SwitcherFourViewManualPreviewProofFixtureMode::MixedPlaceholderAndSourceError
        }
        Some("placeholder-only") => SwitcherFourViewManualPreviewProofFixtureMode::PlaceholderOnly,
        Some(_) => {
            eprintln!(
                "invalid fixture-mode: expected all-renderable, mixed-placeholder-source-error, or placeholder-only"
            );
            std::process::exit(1);
        }
    }
}

fn parse_four_view_actual_window_fixture_mode_or_exit(
    value: Option<String>,
) -> SwitcherFourViewManualPreviewProofFixtureMode {
    parse_four_view_all_renderable_fixture_mode(value).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    })
}

fn parse_four_view_all_renderable_fixture_mode(
    value: Option<String>,
) -> Result<SwitcherFourViewManualPreviewProofFixtureMode, String> {
    match value.as_deref() {
        Some("all-renderable") | None => {
            Ok(SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable)
        }
        Some(_) => Err("invalid fixture-mode: expected all-renderable".to_string()),
    }
}

fn run_four_view_manual_preview_proof_once(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
) -> SwitcherFourViewManualPreviewProofResult {
    run_four_view_manual_preview_proof_with_runtime(
        fixture_mode,
        &SwitcherUnavailableWindowRenderRuntimeHook,
    )
}

fn run_four_view_manual_preview_proof_with_runtime(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
    render_runtime: &impl SwitcherWindowRenderRuntimeHook,
) -> SwitcherFourViewManualPreviewProofResult {
    SwitcherFourViewManualPreviewProofBoundary::default().prove_fixture_with_runtimes(
        default_four_view_manual_preview_proof_input(fixture_mode),
        &DeterministicFourViewFixtureDecodeRuntime,
        render_runtime,
    )
}

fn run_four_view_manual_preview_window_once(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
) -> SwitcherFourViewManualPreviewProofResult {
    #[cfg(target_os = "windows")]
    {
        run_four_view_manual_preview_proof_with_runtime(
            fixture_mode,
            &SwitcherWindowsGdiWindowRenderRuntimeHook,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        run_four_view_manual_preview_proof_with_runtime(
            fixture_mode,
            &SwitcherUnavailableWindowRenderRuntimeHook,
        )
    }
}

fn run_four_view_clean_output_window_once(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
) -> SwitcherFourViewCleanOutputWindowProofResult {
    #[cfg(target_os = "windows")]
    {
        run_four_view_clean_output_window_with_runtime(
            fixture_mode,
            &SwitcherWindowsGdiWindowRenderRuntimeHook,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        run_four_view_clean_output_window_with_runtime(
            fixture_mode,
            &SwitcherUnavailableWindowRenderRuntimeHook,
        )
    }
}

fn run_four_view_clean_output_window_with_runtime(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
    render_runtime: &impl SwitcherWindowRenderRuntimeHook,
) -> SwitcherFourViewCleanOutputWindowProofResult {
    SwitcherFourViewCleanOutputWindowProofBoundary::default().prove_fixture_with_runtimes(
        default_four_view_manual_preview_proof_input(fixture_mode),
        &DeterministicFourViewFixtureDecodeRuntime,
        render_runtime,
    )
}

const FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH: u32 = 1280;
const FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT: u32 = 720;
const FOUR_VIEW_CLEAN_OUTPUT_LOOP_SCALE_MODE: &str = "nearest-neighbor";

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwitcherFourViewCleanOutputWindowLoopSummary {
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
    window_title: String,
    frames_attempted: u32,
    frames_rendered: u32,
    render_failures: u32,
    window_created: bool,
    persistent_window: bool,
    window_updates: u32,
    window_closed: bool,
    source_width: Option<u32>,
    source_height: Option<u32>,
    output_width: Option<u32>,
    output_height: Option<u32>,
    scale_mode: &'static str,
    window_visible: Option<bool>,
    window_capture_candidate: Option<bool>,
    bgra_payload_len: Option<usize>,
}

trait SwitcherFrameCadenceSleepHook {
    fn sleep(&self, duration: Duration);
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct RealSwitcherFrameCadenceSleepHook;

impl SwitcherFrameCadenceSleepHook for RealSwitcherFrameCadenceSleepHook {
    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct PersistentWindowLifecycleSnapshot {
    window_created: bool,
    persistent_window: bool,
    window_updates: u32,
    window_closed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct ObsFriendlyFourViewLoopRenderMetadataSnapshot {
    source_width: Option<u32>,
    source_height: Option<u32>,
    output_width: Option<u32>,
    output_height: Option<u32>,
    bgra_payload_len: Option<usize>,
    window_visible: Option<bool>,
    window_capture_candidate: Option<bool>,
}

struct ObsFriendlyFourViewLoopWindowRenderRuntime<'a, Runtime> {
    inner: &'a Runtime,
    metadata: Mutex<ObsFriendlyFourViewLoopRenderMetadataSnapshot>,
}

impl<'a, Runtime> ObsFriendlyFourViewLoopWindowRenderRuntime<'a, Runtime> {
    fn new(inner: &'a Runtime) -> Self {
        Self {
            inner,
            metadata: Mutex::new(ObsFriendlyFourViewLoopRenderMetadataSnapshot::default()),
        }
    }

    fn metadata_snapshot(&self) -> ObsFriendlyFourViewLoopRenderMetadataSnapshot {
        *self
            .metadata
            .lock()
            .expect("obs-friendly loop metadata mutex should not be poisoned")
    }
}

impl<Runtime> SwitcherWindowRenderRuntimeHook
    for ObsFriendlyFourViewLoopWindowRenderRuntime<'_, Runtime>
where
    Runtime: SwitcherWindowRenderRuntimeHook,
{
    fn render_once(
        &self,
        request: stream_sync_switcher::SwitcherWindowRenderRequest,
    ) -> SwitcherWindowRenderResult {
        let source_width = request.frame.width;
        let source_height = request.frame.height;
        let scaled_frame = scale_four_view_bgra_render_input_to_obs_validation_profile(
            &request.frame,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
        );
        let output_width = scaled_frame.width;
        let output_height = scaled_frame.height;
        let bgra_payload_len = scaled_frame.pixels.len();

        let result = self
            .inner
            .render_once(stream_sync_switcher::SwitcherWindowRenderRequest {
                frame: scaled_frame,
                title: request.title,
                hold_millis: request.hold_millis,
            });

        let (window_visible, window_capture_candidate) =
            four_view_clean_output_window_runtime_visibility_flags(&result);
        *self
            .metadata
            .lock()
            .expect("obs-friendly loop metadata mutex should not be poisoned") =
            ObsFriendlyFourViewLoopRenderMetadataSnapshot {
                source_width: Some(source_width),
                source_height: Some(source_height),
                output_width: Some(output_width),
                output_height: Some(output_height),
                bgra_payload_len: Some(bgra_payload_len),
                window_visible,
                window_capture_candidate,
            };

        result
    }
}

trait SwitcherPersistentWindowLoopRuntimeHook: SwitcherWindowRenderRuntimeHook {
    fn close_persistent_window(&self);
    fn lifecycle_snapshot(&self) -> PersistentWindowLifecycleSnapshot;
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug, Default)]
struct UnavailablePersistentWindowRenderRuntime {
    lifecycle: Mutex<PersistentWindowLifecycleSnapshot>,
}

#[cfg(not(target_os = "windows"))]
impl SwitcherWindowRenderRuntimeHook for UnavailablePersistentWindowRenderRuntime {
    fn render_once(
        &self,
        _request: stream_sync_switcher::SwitcherWindowRenderRequest,
    ) -> SwitcherWindowRenderResult {
        SwitcherWindowRenderResult::BackendUnavailable {
            reason:
                stream_sync_switcher::SwitcherWindowBackendUnavailableReason::UnsupportedPlatform,
            message: Some("switcher window rendering backend is unavailable".to_string()),
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl SwitcherPersistentWindowLoopRuntimeHook for UnavailablePersistentWindowRenderRuntime {
    fn close_persistent_window(&self) {}

    fn lifecycle_snapshot(&self) -> PersistentWindowLifecycleSnapshot {
        *self
            .lifecycle
            .lock()
            .expect("lifecycle mutex should not be poisoned")
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Default)]
struct SwitcherWindowsGdiPersistentWindowRenderRuntime {
    state: Mutex<WindowsPersistentWindowState>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Default)]
struct WindowsPersistentWindowState {
    hwnd: Option<windows::Win32::Foundation::HWND>,
    lifecycle: PersistentWindowLifecycleSnapshot,
}

#[cfg(target_os = "windows")]
impl SwitcherWindowRenderRuntimeHook for SwitcherWindowsGdiPersistentWindowRenderRuntime {
    fn render_once(
        &self,
        request: stream_sync_switcher::SwitcherWindowRenderRequest,
    ) -> SwitcherWindowRenderResult {
        windows_persistent_render_update(&self.state, request)
    }
}

#[cfg(target_os = "windows")]
impl SwitcherPersistentWindowLoopRuntimeHook for SwitcherWindowsGdiPersistentWindowRenderRuntime {
    fn close_persistent_window(&self) {
        windows_persistent_render_close(&self.state);
    }

    fn lifecycle_snapshot(&self) -> PersistentWindowLifecycleSnapshot {
        self.state
            .lock()
            .expect("persistent window state mutex should not be poisoned")
            .lifecycle
    }
}

fn run_four_view_clean_output_window_loop(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
    frames: NonZeroU32,
) -> SwitcherFourViewCleanOutputWindowLoopSummary {
    #[cfg(target_os = "windows")]
    {
        let render_runtime = SwitcherWindowsGdiPersistentWindowRenderRuntime::default();
        run_four_view_clean_output_window_loop_with_runtime_and_sleep(
            fixture_mode,
            frames,
            &render_runtime,
            &RealSwitcherFrameCadenceSleepHook,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        let render_runtime = UnavailablePersistentWindowRenderRuntime::default();
        run_four_view_clean_output_window_loop_with_runtime_and_sleep(
            fixture_mode,
            frames,
            &render_runtime,
            &RealSwitcherFrameCadenceSleepHook,
        )
    }
}

fn run_four_view_clean_output_window_loop_with_runtime_and_sleep(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
    frames: NonZeroU32,
    render_runtime: &(impl SwitcherWindowRenderRuntimeHook + SwitcherPersistentWindowLoopRuntimeHook),
    cadence_sleep: &impl SwitcherFrameCadenceSleepHook,
) -> SwitcherFourViewCleanOutputWindowLoopSummary {
    let mut frames_attempted = 0u32;
    let mut frames_rendered = 0u32;
    let mut render_failures = 0u32;
    let mut window_title = String::new();
    let cadence = four_view_clean_output_window_loop_frame_cadence();
    let obs_runtime = ObsFriendlyFourViewLoopWindowRenderRuntime::new(render_runtime);

    for frame_index in 0..frames.get() {
        let result = run_four_view_clean_output_window_with_runtime(fixture_mode, &obs_runtime);
        frames_attempted += 1;
        window_title = result.clean_output.window_identity.title.clone();

        if let SwitcherFourViewCleanOutputWindowRenderResult::RenderReady { render, .. } =
            &result.clean_output.output_window
        {
            match render {
                SwitcherFourViewComposedCanvasRenderResult::Rendered { .. } => {
                    frames_rendered += 1;
                }
                SwitcherFourViewComposedCanvasRenderResult::RenderFailed { .. } => {
                    render_failures += 1;
                }
                _ => {}
            }
        }

        if frame_index + 1 < frames.get() {
            cadence_sleep.sleep(cadence);
        }
    }

    render_runtime.close_persistent_window();
    let lifecycle = render_runtime.lifecycle_snapshot();
    let render_metadata = obs_runtime.metadata_snapshot();

    SwitcherFourViewCleanOutputWindowLoopSummary {
        fixture_mode,
        window_title,
        frames_attempted,
        frames_rendered,
        render_failures,
        window_created: lifecycle.window_created,
        persistent_window: lifecycle.persistent_window,
        window_updates: lifecycle.window_updates,
        window_closed: lifecycle.window_closed,
        source_width: render_metadata.source_width,
        source_height: render_metadata.source_height,
        output_width: render_metadata.output_width,
        output_height: render_metadata.output_height,
        scale_mode: FOUR_VIEW_CLEAN_OUTPUT_LOOP_SCALE_MODE,
        window_visible: render_metadata.window_visible,
        window_capture_candidate: render_metadata.window_capture_candidate,
        bgra_payload_len: render_metadata.bgra_payload_len,
    }
}

fn four_view_clean_output_window_loop_frame_cadence() -> Duration {
    Duration::from_secs_f64(1.0 / 30.0)
}

fn scale_four_view_bgra_render_input_to_obs_validation_profile(
    frame: &stream_sync_switcher::SwitcherDecodedFrameRenderInput,
    output_width: u32,
    output_height: u32,
) -> stream_sync_switcher::SwitcherDecodedFrameRenderInput {
    let expected_len = output_width as usize * output_height as usize * 4;
    let mut pixels = vec![0u8; expected_len];

    for y in 0..output_height as usize {
        let source_y = y * frame.height as usize / output_height as usize;
        for x in 0..output_width as usize {
            let source_x = x * frame.width as usize / output_width as usize;
            let source_index = (source_y * frame.width as usize + source_x) * 4;
            let destination_index = (y * output_width as usize + x) * 4;
            pixels[destination_index..destination_index + 4]
                .copy_from_slice(&frame.pixels[source_index..source_index + 4]);
        }
    }

    stream_sync_switcher::SwitcherDecodedFrameRenderInput {
        width: output_width,
        height: output_height,
        pixel_format: frame.pixel_format,
        pixels,
    }
}

fn four_view_clean_output_window_runtime_visibility_flags(
    result: &SwitcherWindowRenderResult,
) -> (Option<bool>, Option<bool>) {
    match result {
        SwitcherWindowRenderResult::Rendered(render) => (
            Some(true),
            Some(!render.title.is_empty() && render.width > 0 && render.height > 0),
        ),
        SwitcherWindowRenderResult::RenderFailed { .. }
        | SwitcherWindowRenderResult::InvalidFrame { .. } => (Some(false), Some(false)),
        SwitcherWindowRenderResult::RenderDeferred { .. }
        | SwitcherWindowRenderResult::BackendUnavailable { .. } => (None, None),
    }
}

#[cfg(target_os = "windows")]
fn windows_persistent_render_update(
    state: &Mutex<WindowsPersistentWindowState>,
    request: stream_sync_switcher::SwitcherWindowRenderRequest,
) -> SwitcherWindowRenderResult {
    use std::ptr::null_mut;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, EndPaint, InvalidateRect, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, PAINTSTRUCT, SRCCOPY,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW, RegisterClassW,
        ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, MSG, PM_REMOVE,
        SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY, WM_PAINT, WNDCLASSW,
        WS_OVERLAPPEDWINDOW,
    };

    static mut PERSISTENT_PAINT_FRAME: Option<
        stream_sync_switcher::SwitcherDecodedFrameRenderInput,
    > = None;

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
                if let Some(frame) = PERSISTENT_PAINT_FRAME.as_ref() {
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
        PERSISTENT_PAINT_FRAME = Some(request.frame.clone());
    }

    let mut state = state
        .lock()
        .expect("persistent window state mutex should not be poisoned");
    let hwnd = if let Some(hwnd) = state.hwnd {
        hwnd
    } else {
        let instance = match unsafe { GetModuleHandleW(None) } {
            Ok(instance) => instance,
            Err(error) => {
                return SwitcherWindowRenderResult::RenderFailed {
                    message: format!("GetModuleHandleW failed: {error:?}"),
                };
            }
        };
        let class_name = w!("StreamSyncSwitcherPersistentWindow");
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
        state.hwnd = Some(hwnd);
        state.lifecycle.window_created = true;
        state.lifecycle.persistent_window = true;
        hwnd
    };

    let _ = unsafe { InvalidateRect(Some(hwnd), Some(&RECT::default()), true) };
    let mut msg = MSG::default();
    while unsafe { PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    state.lifecycle.window_updates += 1;

    SwitcherWindowRenderResult::Rendered(stream_sync_switcher::SwitcherWindowRenderSuccess {
        width: request.frame.width,
        height: request.frame.height,
        title: request.title,
        hold_millis: request.hold_millis,
    })
}

#[cfg(target_os = "windows")]
fn windows_persistent_render_close(state: &Mutex<WindowsPersistentWindowState>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    let mut state = state
        .lock()
        .expect("persistent window state mutex should not be poisoned");
    if let Some(hwnd) = state.hwnd.take() {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        state.lifecycle.window_closed = true;
    }
}

fn default_four_view_manual_preview_proof_input(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
) -> SwitcherFourViewManualPreviewProofInput {
    let slots = match fixture_mode {
        SwitcherFourViewManualPreviewProofFixtureMode::MixedPlaceholderAndSourceError => [
            four_view_manual_preview_slot(0, "client-0", "run-0"),
            four_view_manual_preview_slot(1, "client-1", "run-1"),
            four_view_manual_preview_slot(2, "client-2", "run-missing"),
            four_view_manual_preview_slot(3, "", "run-3"),
        ],
        _ => [
            four_view_manual_preview_slot(0, "client-0", "run-0"),
            four_view_manual_preview_slot(1, "client-1", "run-1"),
            four_view_manual_preview_slot(2, "client-2", "run-2"),
            four_view_manual_preview_slot(3, "client-3", "run-3"),
        ],
    };

    SwitcherFourViewManualPreviewProofInput {
        slots,
        target_timestamp: TimestampMicros(1_000_004),
        fixture_mode,
        previous_slots: [None, None, None, None],
        display_current_time: TimestampMicros(5_000_000),
        layout_policy: SwitcherFourViewQuadLayoutPolicy::default(),
        composed_window_title: "StreamSync 4-view".to_string(),
        composed_render_hold_millis: 25,
    }
}

fn four_view_manual_preview_slot(
    slot_index: usize,
    client_id: &str,
    run_id: &str,
) -> SwitcherFourViewTargetTimeSourceSlotConfig {
    SwitcherFourViewTargetTimeSourceSlotConfig {
        slot_index,
        client_id: ClientId(client_id.to_string()),
        run_id: RunId(run_id.to_string()),
    }
}

fn format_four_view_manual_preview_proof_summary(
    summary: &SwitcherFourViewManualPreviewProofSummary,
) -> String {
    format!(
        "switcher four-view proof fixture deterministic=true real_handoff=false actual_window_render=false target_timestamp={} scheduler_status={:?} bgra_composition_result_kind={:?} render_facing_result_kind={:?} window_render_result_kind={:?} placeholder_count={} source_error_count={} scheduler_slot_kinds={} display_slot_kinds={} composition_instruction_kinds={}",
        summary.target_timestamp.0,
        summary.scheduler_status,
        summary.bgra_composition_kind,
        summary.render_facing_kind,
        summary.window_render_kind,
        summary.placeholder_count,
        summary.source_error_count,
        format_four_view_scheduler_slot_kinds(&summary.scheduler_slot_kinds),
        format_four_view_display_slot_kinds(&summary.display_slot_kinds),
        format_four_view_composition_instruction_kinds(&summary.composition_instruction_kinds),
    )
}

fn format_four_view_manual_preview_window_summary(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
    result: &SwitcherFourViewManualPreviewProofResult,
) -> String {
    let summary = &result.summary;
    let (width, height, bgra_payload_len) =
        four_view_window_render_ready_dimensions(&result.validation.window_render.window_render);
    format!(
        "switcher four-view proof window command_name=--four-view-proof-window-once fixture_mode={} deterministic_fixture=true real_handoff=false actual_window_render=true target_timestamp={} scheduler_status={:?} bgra_composition_result_kind={:?} render_facing_result_kind={:?} window_render_result_kind={} width={} height={} bgra_payload_len={} placeholder_count={} source_error_count={}",
        format_four_view_manual_preview_fixture_mode_name(fixture_mode),
        summary.target_timestamp.0,
        summary.scheduler_status,
        summary.bgra_composition_kind,
        summary.render_facing_kind,
        format_four_view_actual_window_render_result_kind(
            &result.validation.window_render.window_render
        ),
        format_optional_u32(width),
        format_optional_u32(height),
        format_optional_usize(bgra_payload_len),
        summary.placeholder_count,
        summary.source_error_count,
    )
}

fn format_four_view_clean_output_window_summary(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
    result: &SwitcherFourViewCleanOutputWindowProofResult,
) -> String {
    let render_facing_result_kind =
        format_four_view_render_facing_result_kind(&result.clean_output.render_facing.render);
    let output_window_result_kind =
        format_four_view_clean_output_window_result_kind(&result.clean_output.output_window);
    let (width, height, bgra_payload_len) =
        four_view_clean_output_window_ready_dimensions(&result.clean_output.output_window);
    let (placeholder_count, source_error_count) =
        four_view_clean_output_placeholder_and_source_error_counts(&result.clean_output);
    format!(
        "switcher four-view clean output window command_name=--four-view-clean-output-window-once fixture_mode={} clean_output_window=true actual_window_render=true real_handoff=false window_title={} scheduler_status={:?} render_facing_result_kind={} output_window_result_kind={} width={} height={} bgra_payload_len={} placeholder_count={} source_error_count={}",
        format_four_view_manual_preview_fixture_mode_name(fixture_mode),
        result.clean_output.window_identity.title,
        result.clean_output.scheduler_status,
        render_facing_result_kind,
        output_window_result_kind,
        format_optional_u32(width),
        format_optional_u32(height),
        format_optional_usize(bgra_payload_len),
        placeholder_count,
        source_error_count,
    )
}

fn format_four_view_clean_output_window_loop_summary(
    summary: &SwitcherFourViewCleanOutputWindowLoopSummary,
) -> String {
    format!(
        "switcher four-view clean output window loop command_name=--four-view-clean-output-window-loop fixture_mode={} clean_output_window=true actual_window_render=true real_handoff=false window_title={} frames_attempted={} frames_rendered={} render_failures={} window_created={} persistent_window={} window_updates={} window_closed={} source_width={} source_height={} output_width={} output_height={} scale_mode={} window_visible={} window_capture_candidate={} bgra_payload_len={}",
        format_four_view_manual_preview_fixture_mode_name(summary.fixture_mode),
        summary.window_title,
        summary.frames_attempted,
        summary.frames_rendered,
        summary.render_failures,
        summary.window_created,
        summary.persistent_window,
        summary.window_updates,
        summary.window_closed,
        format_optional_u32(summary.source_width),
        format_optional_u32(summary.source_height),
        format_optional_u32(summary.output_width),
        format_optional_u32(summary.output_height),
        summary.scale_mode,
        format_optional_bool(summary.window_visible),
        format_optional_bool(summary.window_capture_candidate),
        format_optional_usize(summary.bgra_payload_len),
    )
}

fn format_four_view_manual_preview_fixture_mode_name(
    fixture_mode: SwitcherFourViewManualPreviewProofFixtureMode,
) -> &'static str {
    match fixture_mode {
        SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable => "all-renderable",
        SwitcherFourViewManualPreviewProofFixtureMode::MixedPlaceholderAndSourceError => {
            "mixed-placeholder-source-error"
        }
        SwitcherFourViewManualPreviewProofFixtureMode::PlaceholderOnly => "placeholder-only",
    }
}

fn four_view_window_render_ready_dimensions(
    result: &SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult,
) -> (Option<u32>, Option<u32>, Option<usize>) {
    match result {
        SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult::RenderReady {
            width,
            height,
            bgra_payload_len,
            ..
        } => (Some(*width), Some(*height), Some(*bgra_payload_len)),
        _ => (None, None, None),
    }
}

fn format_four_view_actual_window_render_result_kind(
    result: &SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult,
) -> &'static str {
    match result {
        SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult::RenderReady {
            render,
            ..
        } => match render {
            SwitcherFourViewComposedCanvasRenderResult::Rendered { .. } => "Rendered",
            SwitcherFourViewComposedCanvasRenderResult::RenderDeferred { .. } => "RenderDeferred",
            SwitcherFourViewComposedCanvasRenderResult::BackendUnavailable { .. } => {
                "BackendUnavailable"
            }
            SwitcherFourViewComposedCanvasRenderResult::InvalidComposedFrame { .. } => {
                "InvalidFrame"
            }
            SwitcherFourViewComposedCanvasRenderResult::RenderFailed { .. } => "RenderFailed",
        },
        SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult::NoRenderableQuadView {
            ..
        } => "NoRenderableQuadView",
        SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult::InvalidQuadView {
            ..
        } => "InvalidQuadView",
    }
}

fn format_four_view_render_facing_result_kind(
    result: &stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult,
) -> &'static str {
    match result {
        stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult::RenderReady { .. } => {
            "RenderReady"
        }
        stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult::NoRenderableQuadView {
            ..
        } => "NoRenderableQuadView",
        stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult::InvalidQuadView {
            ..
        } => "InvalidQuadView",
    }
}

fn four_view_clean_output_window_ready_dimensions(
    result: &SwitcherFourViewCleanOutputWindowRenderResult,
) -> (Option<u32>, Option<u32>, Option<usize>) {
    match result {
        SwitcherFourViewCleanOutputWindowRenderResult::RenderReady {
            width,
            height,
            bgra_payload_len,
            ..
        } => (Some(*width), Some(*height), Some(*bgra_payload_len)),
        _ => (None, None, None),
    }
}

fn format_four_view_clean_output_window_result_kind(
    result: &SwitcherFourViewCleanOutputWindowRenderResult,
) -> &'static str {
    match result {
        SwitcherFourViewCleanOutputWindowRenderResult::RenderReady { render, .. } => match render {
            SwitcherFourViewComposedCanvasRenderResult::Rendered { .. } => "Rendered",
            SwitcherFourViewComposedCanvasRenderResult::RenderDeferred { .. } => "RenderDeferred",
            SwitcherFourViewComposedCanvasRenderResult::BackendUnavailable { .. } => {
                "BackendUnavailable"
            }
            SwitcherFourViewComposedCanvasRenderResult::InvalidComposedFrame { .. } => {
                "InvalidFrame"
            }
            SwitcherFourViewComposedCanvasRenderResult::RenderFailed { .. } => "RenderFailed",
        },
        SwitcherFourViewCleanOutputWindowRenderResult::NoRenderableQuadView { .. } => {
            "NoRenderableQuadView"
        }
        SwitcherFourViewCleanOutputWindowRenderResult::InvalidQuadView { .. } => "InvalidQuadView",
    }
}

fn four_view_clean_output_placeholder_and_source_error_counts(
    result: &stream_sync_switcher::SwitcherFourViewCleanOutputWindowOutput,
) -> (usize, usize) {
    let slots = match &result.output_window {
        SwitcherFourViewCleanOutputWindowRenderResult::RenderReady { slots, .. }
        | SwitcherFourViewCleanOutputWindowRenderResult::NoRenderableQuadView { slots }
        | SwitcherFourViewCleanOutputWindowRenderResult::InvalidQuadView { slots, .. } => slots,
    };
    let placeholder_count =
        slots
            .iter()
            .filter(|slot| {
                matches!(
                slot.kind,
                stream_sync_switcher::SwitcherFourViewQuadComposedSlotKind::NoDisplayPlaceholder {
                    ..
                }
            )
            })
            .count();
    let source_error_count = slots
        .iter()
        .filter(|slot| {
            matches!(
                slot.kind,
                stream_sync_switcher::SwitcherFourViewQuadComposedSlotKind::SourceErrorPlaceholder {
                    ..
                }
            )
        })
        .count();
    (placeholder_count, source_error_count)
}

fn format_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn format_optional_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn format_optional_bool(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn format_four_view_scheduler_slot_kinds(
    kinds: &[SwitcherFourViewManualPreviewSchedulerSlotKind; 4],
) -> String {
    [
        format!("{:?}", kinds[0]),
        format!("{:?}", kinds[1]),
        format!("{:?}", kinds[2]),
        format!("{:?}", kinds[3]),
    ]
    .join("|")
}

fn format_four_view_display_slot_kinds(
    kinds: &[SwitcherFourViewManualPreviewDisplaySlotKind; 4],
) -> String {
    [
        format!("{:?}", kinds[0]),
        format!("{:?}", kinds[1]),
        format!("{:?}", kinds[2]),
        format!("{:?}", kinds[3]),
    ]
    .join("|")
}

fn format_four_view_composition_instruction_kinds(
    kinds: &[SwitcherFourViewManualPreviewCompositionInstructionKind; 4],
) -> String {
    [
        format!("{:?}", kinds[0]),
        format!("{:?}", kinds[1]),
        format!("{:?}", kinds[2]),
        format!("{:?}", kinds[3]),
    ]
    .join("|")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct DeterministicFourViewFixtureDecodeRuntime;

impl SwitcherH264DecodeRuntimeHook for DeterministicFourViewFixtureDecodeRuntime {
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

        let seed = input.encoded_payload[0];
        let mut pixels = Vec::with_capacity(input.width as usize * input.height as usize * 4);
        for _ in 0..input.width as usize * input.height as usize {
            pixels.extend_from_slice(&[seed, seed.wrapping_add(1), seed.wrapping_add(2), 255]);
        }

        SwitcherH264DecodeResult::Decoded(SwitcherDecodedFrame {
            width: input.width,
            height: input.height,
            pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
            pixels,
        })
    }
}

fn verify_two_view_fixture_once(
    queue_state: &ServerVideoFrameQueueState,
    left_client_id: &ClientId,
    right_client_id: &ClientId,
    hold_millis: u64,
) -> SwitcherTwoViewManualVerificationResult {
    let input = SwitcherTwoViewManualVerificationInput {
        queue_state,
        left_client_id,
        right_client_id,
        current_switcher_time: TimestampMicros(1_600_011),
        policy: SwitcherTwoViewTargetTimeSelectionPolicy {
            playout_delay_micros: 600_000,
            max_late_micros: 50,
            max_early_micros: 50,
            ..SwitcherTwoViewTargetTimeSelectionPolicy::default()
        },
        render_hold_millis: hold_millis,
    };

    #[cfg(target_os = "windows")]
    {
        SwitcherTwoViewManualVerificationBoundary::default().verify_with_runtimes(
            input,
            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
            &SwitcherWindowsGdiWindowRenderRuntimeHook,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        SwitcherTwoViewManualVerificationBoundary::default().verify_with_runtimes(
            input,
            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
            &SwitcherUnavailableWindowRenderRuntimeHook,
        )
    }
}

fn print_two_view_sync_summary(result: SwitcherTwoViewManualVerificationResult) {
    println!(
        "switcher two-view sync fixture cross_process_queue=false continuous=false target_time={} left={} right={}",
        result.summary.shared_target_time.0,
        format_two_view_side_summary(&result.summary.left),
        format_two_view_side_summary(&result.summary.right)
    );
}

fn format_two_view_side_summary(summary: &SwitcherTwoViewManualVerificationSideSummary) -> String {
    format!(
        "side={:?},client_id={},selection={:?},decode_render={:?},frame_id={},payload_len={},width={},height={},adjusted_capture_timestamp={}",
        summary.side,
        summary.client_id.0,
        summary.selection_status,
        summary.decode_render_status,
        summary
            .frame_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        summary
            .encoded_payload_len
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        summary
            .width
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        summary
            .height
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        summary
            .adjusted_capture_timestamp
            .map(|value| value.0.to_string())
            .unwrap_or_else(|| "none".to_string())
    )
}

fn fixture_queue_state(client_id: &ClientId) -> ServerVideoFrameQueueState {
    let mut state = ServerVideoFrameQueueState::default();
    store_fixture_frame(
        &mut state,
        client_id,
        42,
        1280,
        720,
        vec![0x42, 0xbb, 0xcc],
        TimestampMicros(2_000_042),
    );
    state
}

fn fixture_two_view_queue_state(
    left_client_id: &ClientId,
    right_client_id: &ClientId,
) -> ServerVideoFrameQueueState {
    let mut state = ServerVideoFrameQueueState::default();
    store_fixture_frame(
        &mut state,
        left_client_id,
        10,
        2,
        1,
        vec![0, 0, 1, 0x65, 0x10],
        TimestampMicros(2_000_010),
    );
    store_fixture_frame(
        &mut state,
        right_client_id,
        12,
        2,
        1,
        vec![0, 0, 1, 0x65, 0x12],
        TimestampMicros(2_000_012),
    );
    state
}

fn fixture_two_view_composed_frame() -> SwitcherTwoViewCompositionResult {
    SwitcherTwoViewCompositionBoundary.compose_side_by_side(SwitcherTwoViewCompositionInput {
        left: SwitcherTwoViewLayoutSideInput::Decoded {
            side: SwitcherTwoViewSide::Left,
            selected: None,
            frame: fixture_decoded_frame(2, 2, [32, 96, 180, 255]),
        },
        right: SwitcherTwoViewLayoutSideInput::Decoded {
            side: SwitcherTwoViewSide::Right,
            selected: None,
            frame: fixture_decoded_frame(2, 2, [180, 96, 32, 255]),
        },
        policy: SwitcherTwoViewLayoutPolicy::default(),
    })
}

fn fixture_decoded_frame(width: u32, height: u32, pixel: [u8; 4]) -> SwitcherDecodedFrame {
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

fn store_fixture_frame(
    state: &mut ServerVideoFrameQueueState,
    client_id: &ClientId,
    frame_id: u64,
    width: u32,
    height: u32,
    payload: Vec<u8>,
    queued_at: TimestampMicros,
) {
    let source = PacketSource {
        address: "127.0.0.1:5001"
            .parse()
            .expect("fixture source should parse"),
    };
    let packet = ServerRegisteredVideoFramePacket {
        source,
        authenticated_sender: AuthenticatedSenderEntry {
            client_id: client_id.clone(),
            source,
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(1),
            registered_at: None,
        },
        frame: VideoFrame {
            message_type: MessageType::VideoFrame,
            protocol_version: ProtocolVersion(1),
            client_id: client_id.clone(),
            run_id: RunId("run-1".to_string()),
            frame_id,
            capture_timestamp: TimestampMicros(1_000_000 + frame_id),
            send_timestamp: TimestampMicros(1_000_100 + frame_id),
            is_keyframe: true,
            metadata_reserved: [0; 3],
            width,
            height,
            fps_nominal: 30,
            codec: Codec::H264,
            payload_size: payload.len(),
            payload,
        },
    };
    let input = ServerVideoFrameHandlerBoundary.prepare_input(packet);
    let _result = ServerVideoFrameQueueStorageBoundary.store_frame(
        state,
        input,
        queued_at,
        ServerVideoFrameQueuePolicy::default(),
    );
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::num::NonZeroU32;
    use std::time::Duration;

    use stream_sync_protocol::{ClientId, Codec, RunId, TimestampMicros};
    use stream_sync_switcher::{
        SwitcherFourViewCleanOutputWindowRenderResult,
        SwitcherFourViewManualPreviewBgraCompositionKind,
        SwitcherFourViewManualPreviewProofFixtureMode,
        SwitcherFourViewManualPreviewRenderFacingKind,
        SwitcherFourViewManualPreviewWindowRenderKind,
        SwitcherNamedPipeQueuedFrameHandoffRequestOutput,
        SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
        SwitcherNamedPipeQueuedFrameHandoffRequestSummary,
        SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
        SwitcherNamedPipeQueuedFrameHandoffResultKind,
        SwitcherNamedPipeQueuedFrameHandoffRetryClassification, SwitcherQueuedFrameHandoffError,
        SwitcherSingleViewSelectedEncodedFrame, SwitcherUnavailableWindowRenderRuntimeHook,
        SwitcherWindowRenderRequest, SwitcherWindowRenderResult, SwitcherWindowRenderRuntimeHook,
        SwitcherWindowRenderSuccess,
    };

    use super::{
        format_four_view_clean_output_window_loop_summary,
        format_four_view_clean_output_window_summary,
        format_four_view_manual_preview_proof_summary,
        format_four_view_manual_preview_window_summary, format_handoff_mode,
        format_handoff_read_mode, format_named_pipe_handoff_switcher_result_summary,
        format_named_pipe_handoff_switcher_summary,
        four_view_clean_output_window_loop_frame_cadence,
        parse_four_view_actual_window_fixture_mode_or_exit,
        parse_four_view_all_renderable_fixture_mode,
        parse_four_view_manual_preview_fixture_mode_or_exit, parse_handoff_mode_or_exit,
        parse_positive_u32_arg, run_four_view_clean_output_window_loop_with_runtime_and_sleep,
        run_four_view_clean_output_window_with_runtime, run_four_view_manual_preview_proof_once,
        run_four_view_manual_preview_proof_with_runtime, PersistentWindowLifecycleSnapshot,
        SwitcherFrameCadenceSleepHook, SwitcherPersistentWindowLoopRuntimeHook,
        SwitcherQueuedFrameHandoffResult, SwitcherSingleClientQueueSourceMode,
        FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
        FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH, FOUR_VIEW_CLEAN_OUTPUT_LOOP_SCALE_MODE,
    };
    use stream_sync_switcher::SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE;

    #[test]
    fn switcher_handoff_parses_mode_names() {
        assert_eq!(
            parse_handoff_mode_or_exit(Some("preview-oldest".to_string()), "mode"),
            SwitcherSingleClientQueueSourceMode::PreviewOldest
        );
        assert_eq!(
            parse_handoff_mode_or_exit(Some("preview-latest".to_string()), "mode"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest
        );
        assert_eq!(
            parse_handoff_mode_or_exit(Some("consume-oldest".to_string()), "mode"),
            SwitcherSingleClientQueueSourceMode::ConsumeOldest
        );
    }

    #[test]
    fn switcher_handoff_formats_mode_names() {
        assert_eq!(
            format_handoff_mode(SwitcherSingleClientQueueSourceMode::PreviewLatest),
            "preview-latest"
        );
        assert_eq!(
            format_handoff_read_mode(
                stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatest
            ),
            "inspect-latest"
        );
    }

    #[test]
    fn switcher_handoff_summary_keeps_frame_read_visible() {
        let result = SwitcherQueuedFrameHandoffResult::FrameRead {
            frame: SwitcherSingleViewSelectedEncodedFrame {
                client_id: ClientId("player1".to_string()),
                run_id: RunId("run-a".to_string()),
                frame_id: 7,
                capture_timestamp: TimestampMicros(10),
                send_timestamp: TimestampMicros(11),
                queued_at: TimestampMicros(12),
                is_keyframe: true,
                width: 1280,
                height: 720,
                fps_nominal: 30,
                codec: Codec::H264,
                encoded_payload_len: 3,
                encoded_payload: vec![1, 2, 3],
            },
            mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
            remaining_client_queue_len: 4,
        };

        let summary = format_named_pipe_handoff_switcher_result_summary(
            "pipe-a",
            88,
            &ClientId("player1".to_string()),
            &RunId("run-a".to_string()),
            stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatest,
            "sent",
            "decoded",
            1,
            5000,
            17,
            "FrameRead",
            "none",
            "none",
            &result,
        );

        assert!(summary.contains("request_id=88"));
        assert!(summary.contains("attempt_count=1"));
        assert!(summary.contains("timeout_millis=5000"));
        assert!(summary.contains("elapsed_millis=17"));
        assert!(summary.contains("result_kind=FrameRead"));
        assert!(summary.contains("final_result=FrameRead"));
        assert!(summary.contains("last_error=none"));
        assert!(summary.contains("retry_classification=none"));
        assert!(summary.contains("queue_len=4"));
        assert!(summary.contains("frame_id=7"));
        assert!(summary.contains("codec=H264"));
    }

    #[test]
    fn switcher_handoff_encode_failure_stays_explicit() {
        let result = SwitcherQueuedFrameHandoffResult::HandoffError {
            client_id: ClientId("player1".to_string()),
            run_id: RunId("run-a".to_string()),
            mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
            error: SwitcherQueuedFrameHandoffError::MalformedResponse,
        };
        let output = SwitcherNamedPipeQueuedFrameHandoffRequestOutput {
            summary: SwitcherNamedPipeQueuedFrameHandoffRequestSummary {
                pipe_name: "pipe-b".to_string(),
                request_id: 9,
                read_mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                attempt_count: 1,
                timeout_millis: 2500,
                request_status: SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodeFailed,
                response_status: SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None,
                result_kind: SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError,
                final_result: SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError,
                last_error: Some(SwitcherQueuedFrameHandoffError::MalformedResponse),
                retry_classification: Some(
                    SwitcherNamedPipeQueuedFrameHandoffRetryClassification::NonRetryable,
                ),
                elapsed_millis: 4,
            },
            runtime: None,
            result,
        };

        let summary = format_named_pipe_handoff_switcher_summary(&output);

        assert!(summary.contains("request_status=encode_failed"));
        assert!(summary.contains("response_status=none"));
        assert!(summary.contains("result_kind=HandoffError"));
        assert!(summary.contains("attempt_count=1"));
        assert!(summary.contains("final_result=HandoffError"));
        assert!(summary.contains("last_error=MalformedResponse"));
        assert!(summary.contains("retry_classification=NonRetryable"));
        assert!(summary.contains("handoff_error=MalformedResponse"));
        assert!(summary.contains("timeout_millis=2500"));
        assert!(summary.contains("elapsed_millis=4"));
    }

    #[test]
    fn switcher_four_view_manual_proof_fixture_mode_parses_names() {
        assert_eq!(
            parse_four_view_manual_preview_fixture_mode_or_exit(Some("all-renderable".to_string())),
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable
        );
        assert_eq!(
            parse_four_view_manual_preview_fixture_mode_or_exit(Some(
                "mixed-placeholder-source-error".to_string()
            )),
            SwitcherFourViewManualPreviewProofFixtureMode::MixedPlaceholderAndSourceError
        );
        assert_eq!(
            parse_four_view_manual_preview_fixture_mode_or_exit(Some(
                "placeholder-only".to_string()
            )),
            SwitcherFourViewManualPreviewProofFixtureMode::PlaceholderOnly
        );
        assert_eq!(
            parse_four_view_manual_preview_fixture_mode_or_exit(None),
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable
        );
    }

    #[test]
    fn switcher_four_view_actual_window_fixture_mode_parses_only_all_renderable() {
        assert_eq!(
            parse_four_view_actual_window_fixture_mode_or_exit(Some("all-renderable".to_string())),
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable
        );
        assert_eq!(
            parse_four_view_actual_window_fixture_mode_or_exit(None),
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable
        );
    }

    #[test]
    fn switcher_four_view_all_renderable_fixture_mode_rejects_unsupported_modes() {
        let error = parse_four_view_all_renderable_fixture_mode(Some(
            "mixed-placeholder-source-error".to_string(),
        ))
        .expect_err("unsupported fixture mode should be rejected");

        assert_eq!(error, "invalid fixture-mode: expected all-renderable");
    }

    #[test]
    fn switcher_four_view_clean_output_loop_frames_parse_positive_integer() {
        assert_eq!(
            parse_positive_u32_arg(Some("3".to_string()), "frames")
                .expect("positive frames should parse"),
            NonZeroU32::new(3).expect("3 should be non-zero")
        );

        let zero_error =
            parse_positive_u32_arg(Some("0".to_string()), "frames").expect_err("zero is invalid");
        assert_eq!(zero_error, "invalid frames: expected positive integer");
    }

    #[test]
    fn switcher_four_view_manual_proof_helper_uses_deterministic_backend_free_runtime() {
        let result = run_four_view_manual_preview_proof_once(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
        );

        assert_eq!(result.summary.target_timestamp.0, 1_000_004);
        assert_eq!(
            result.summary.bgra_composition_kind,
            SwitcherFourViewManualPreviewBgraCompositionKind::ComposedFrame
        );
        assert_eq!(
            result.summary.render_facing_kind,
            SwitcherFourViewManualPreviewRenderFacingKind::RenderReady
        );
        assert_eq!(
            result.summary.window_render_kind,
            SwitcherFourViewManualPreviewWindowRenderKind::BackendUnavailable
        );
        assert_eq!(result.summary.placeholder_count, 0);
        assert_eq!(result.summary.source_error_count, 0);
    }

    #[test]
    fn switcher_four_view_manual_proof_summary_formats_expected_fields() {
        let result = run_four_view_manual_preview_proof_once(
            SwitcherFourViewManualPreviewProofFixtureMode::MixedPlaceholderAndSourceError,
        );
        let summary = format_four_view_manual_preview_proof_summary(&result.summary);

        assert!(summary.contains("target_timestamp=1000004"));
        assert!(summary.contains("scheduler_status=HandoffError"));
        assert!(summary.contains("bgra_composition_result_kind=ComposedFrame"));
        assert!(summary.contains("render_facing_result_kind=RenderReady"));
        assert!(summary.contains("window_render_result_kind=BackendUnavailable"));
        assert!(summary.contains("placeholder_count=2"));
        assert!(summary.contains("source_error_count=1"));
        assert!(summary.contains("scheduler_slot_kinds=Selected|WaitingForFrameAtOrBeforeTarget|NoFrameAvailable|HandoffError"));
        assert!(summary.contains("display_slot_kinds=Update|NoDisplayPlaceholder|NoDisplayPlaceholder|SourceErrorPlaceholder"));
        assert!(summary.contains("composition_instruction_kinds=UpdatedFrame|NoDisplayPlaceholder|NoDisplayPlaceholder|SourceErrorPlaceholder"));
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    struct FixtureRenderedWindowRuntime;

    impl SwitcherWindowRenderRuntimeHook for FixtureRenderedWindowRuntime {
        fn render_once(&self, request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                width: request.frame.width,
                height: request.frame.height,
                title: request.title,
                hold_millis: request.hold_millis,
            })
        }
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    struct FixtureRenderFailedWindowRuntime;

    impl SwitcherWindowRenderRuntimeHook for FixtureRenderFailedWindowRuntime {
        fn render_once(&self, _request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            SwitcherWindowRenderResult::RenderFailed {
                message: "fixture render failed".to_string(),
            }
        }
    }

    #[derive(Debug, Default)]
    struct PersistentFixtureRenderedWindowRuntime {
        lifecycle: RefCell<PersistentWindowLifecycleSnapshot>,
        create_calls: RefCell<u32>,
        close_calls: RefCell<u32>,
    }

    impl SwitcherWindowRenderRuntimeHook for PersistentFixtureRenderedWindowRuntime {
        fn render_once(&self, request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            let mut lifecycle = self.lifecycle.borrow_mut();
            if !lifecycle.window_created {
                lifecycle.window_created = true;
                lifecycle.persistent_window = true;
                *self.create_calls.borrow_mut() += 1;
            }
            lifecycle.window_updates += 1;
            SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                width: request.frame.width,
                height: request.frame.height,
                title: request.title,
                hold_millis: request.hold_millis,
            })
        }
    }

    impl SwitcherPersistentWindowLoopRuntimeHook for PersistentFixtureRenderedWindowRuntime {
        fn close_persistent_window(&self) {
            self.lifecycle.borrow_mut().window_closed = true;
            *self.close_calls.borrow_mut() += 1;
        }

        fn lifecycle_snapshot(&self) -> PersistentWindowLifecycleSnapshot {
            *self.lifecycle.borrow()
        }
    }

    #[derive(Debug, Default)]
    struct PersistentFixtureRenderFailedWindowRuntime {
        lifecycle: RefCell<PersistentWindowLifecycleSnapshot>,
        create_calls: RefCell<u32>,
        close_calls: RefCell<u32>,
    }

    impl SwitcherWindowRenderRuntimeHook for PersistentFixtureRenderFailedWindowRuntime {
        fn render_once(&self, _request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            let mut lifecycle = self.lifecycle.borrow_mut();
            if !lifecycle.window_created {
                lifecycle.window_created = true;
                lifecycle.persistent_window = true;
                *self.create_calls.borrow_mut() += 1;
            }
            lifecycle.window_updates += 1;
            SwitcherWindowRenderResult::RenderFailed {
                message: "fixture render failed".to_string(),
            }
        }
    }

    impl SwitcherPersistentWindowLoopRuntimeHook for PersistentFixtureRenderFailedWindowRuntime {
        fn close_persistent_window(&self) {
            self.lifecycle.borrow_mut().window_closed = true;
            *self.close_calls.borrow_mut() += 1;
        }

        fn lifecycle_snapshot(&self) -> PersistentWindowLifecycleSnapshot {
            *self.lifecycle.borrow()
        }
    }

    #[derive(Debug, Default)]
    struct RecordingCadenceSleepHook {
        durations: RefCell<Vec<Duration>>,
    }

    impl SwitcherFrameCadenceSleepHook for RecordingCadenceSleepHook {
        fn sleep(&self, duration: Duration) {
            self.durations.borrow_mut().push(duration);
        }
    }

    #[test]
    fn switcher_four_view_actual_window_summary_formats_rendered_metadata() {
        let result = run_four_view_manual_preview_proof_with_runtime(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &FixtureRenderedWindowRuntime,
        );
        let summary = format_four_view_manual_preview_window_summary(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &result,
        );

        assert!(summary.contains("command_name=--four-view-proof-window-once"));
        assert!(summary.contains("fixture_mode=all-renderable"));
        assert!(summary.contains("actual_window_render=true"));
        assert!(summary.contains("real_handoff=false"));
        assert!(summary.contains("scheduler_status=AllSelected"));
        assert!(summary.contains("bgra_composition_result_kind=ComposedFrame"));
        assert!(summary.contains("render_facing_result_kind=RenderReady"));
        assert!(summary.contains("window_render_result_kind=Rendered"));
        assert!(summary.contains("width="));
        assert!(!summary.contains("width=none"));
        assert!(summary.contains("height="));
        assert!(!summary.contains("height=none"));
        assert!(summary.contains("bgra_payload_len="));
        assert!(!summary.contains("bgra_payload_len=none"));
        assert!(summary.contains("placeholder_count=0"));
        assert!(summary.contains("source_error_count=0"));
    }

    #[test]
    fn switcher_four_view_actual_window_summary_keeps_render_failed_explicit() {
        let result = run_four_view_manual_preview_proof_with_runtime(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &FixtureRenderFailedWindowRuntime,
        );
        let summary = format_four_view_manual_preview_window_summary(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &result,
        );

        assert!(summary.contains("window_render_result_kind=RenderFailed"));
        assert!(summary.contains("render_facing_result_kind=RenderReady"));
        assert!(summary.contains("bgra_composition_result_kind=ComposedFrame"));
    }

    #[test]
    fn switcher_four_view_clean_output_helper_uses_stable_title_and_backend_free_default() {
        let result = run_four_view_clean_output_window_with_runtime(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &SwitcherUnavailableWindowRenderRuntimeHook,
        );

        assert_eq!(
            result.clean_output.window_identity.title,
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        );
        assert!(matches!(
            result.clean_output.output_window,
            SwitcherFourViewCleanOutputWindowRenderResult::RenderReady { .. }
        ));
        assert!(matches!(
            result.validation.render_facing.render,
            stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult::RenderReady { .. }
        ));
    }

    #[test]
    fn switcher_four_view_clean_output_summary_formats_rendered_metadata() {
        let result = run_four_view_clean_output_window_with_runtime(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &FixtureRenderedWindowRuntime,
        );
        let summary = format_four_view_clean_output_window_summary(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &result,
        );

        assert!(summary.contains("command_name=--four-view-clean-output-window-once"));
        assert!(summary.contains("fixture_mode=all-renderable"));
        assert!(summary.contains("clean_output_window=true"));
        assert!(summary.contains("actual_window_render=true"));
        assert!(summary.contains("real_handoff=false"));
        assert!(summary.contains(&format!(
            "window_title={}",
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        )));
        assert!(summary.contains("scheduler_status=AllSelected"));
        assert!(summary.contains("render_facing_result_kind=RenderReady"));
        assert!(summary.contains("output_window_result_kind=Rendered"));
        assert!(summary.contains("width="));
        assert!(!summary.contains("width=none"));
        assert!(summary.contains("height="));
        assert!(!summary.contains("height=none"));
        assert!(summary.contains("bgra_payload_len="));
        assert!(!summary.contains("bgra_payload_len=none"));
        assert!(summary.contains("placeholder_count=0"));
        assert!(summary.contains("source_error_count=0"));
    }

    #[test]
    fn switcher_four_view_clean_output_summary_keeps_render_failed_explicit() {
        let result = run_four_view_clean_output_window_with_runtime(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &FixtureRenderFailedWindowRuntime,
        );
        let summary = format_four_view_clean_output_window_summary(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            &result,
        );

        assert!(summary.contains("render_facing_result_kind=RenderReady"));
        assert!(summary.contains("output_window_result_kind=RenderFailed"));
        assert!(summary.contains(&format!(
            "window_title={}",
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        )));
    }

    #[test]
    fn switcher_four_view_clean_output_loop_counts_rendered_frames_and_sleeps_between_frames() {
        let sleep_hook = RecordingCadenceSleepHook::default();
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_clean_output_window_loop_with_runtime_and_sleep(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            NonZeroU32::new(3).expect("3 should be non-zero"),
            &render_runtime,
            &sleep_hook,
        );

        assert_eq!(summary.frames_attempted, 3);
        assert_eq!(summary.frames_rendered, 3);
        assert_eq!(summary.render_failures, 0);
        assert!(summary.window_created);
        assert!(summary.persistent_window);
        assert_eq!(summary.window_updates, 3);
        assert!(summary.window_closed);
        assert_eq!(
            summary.window_title,
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        );
        assert_eq!(summary.source_width, Some(4));
        assert_eq!(summary.source_height, Some(2));
        assert_eq!(
            summary.output_width,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH)
        );
        assert_eq!(
            summary.output_height,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT)
        );
        assert_eq!(summary.scale_mode, FOUR_VIEW_CLEAN_OUTPUT_LOOP_SCALE_MODE);
        assert_eq!(summary.window_visible, Some(true));
        assert_eq!(summary.window_capture_candidate, Some(true));
        assert_eq!(
            summary.bgra_payload_len,
            Some(
                FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH as usize
                    * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT as usize
                    * 4
            )
        );
        assert_eq!(*render_runtime.create_calls.borrow(), 1);
        assert_eq!(*render_runtime.close_calls.borrow(), 1);

        let durations = sleep_hook.durations.borrow();
        assert_eq!(durations.len(), 2);
        assert!(durations
            .iter()
            .all(|duration| *duration == four_view_clean_output_window_loop_frame_cadence()));
    }

    #[test]
    fn switcher_four_view_clean_output_loop_counts_render_failures_explicitly() {
        let render_runtime = PersistentFixtureRenderFailedWindowRuntime::default();
        let summary = run_four_view_clean_output_window_loop_with_runtime_and_sleep(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            NonZeroU32::new(2).expect("2 should be non-zero"),
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 0);
        assert_eq!(summary.render_failures, 2);
        assert!(summary.window_created);
        assert!(summary.persistent_window);
        assert_eq!(summary.window_updates, 2);
        assert!(summary.window_closed);
        assert_eq!(summary.source_width, Some(4));
        assert_eq!(summary.source_height, Some(2));
        assert_eq!(
            summary.output_width,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH)
        );
        assert_eq!(
            summary.output_height,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT)
        );
        assert_eq!(summary.scale_mode, FOUR_VIEW_CLEAN_OUTPUT_LOOP_SCALE_MODE);
        assert_eq!(summary.window_visible, Some(false));
        assert_eq!(summary.window_capture_candidate, Some(false));
        assert_eq!(
            summary.bgra_payload_len,
            Some(
                FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH as usize
                    * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT as usize
                    * 4
            )
        );
        assert_eq!(*render_runtime.create_calls.borrow(), 1);
        assert_eq!(*render_runtime.close_calls.borrow(), 1);
    }

    #[test]
    fn switcher_four_view_clean_output_loop_summary_formats_expected_fields() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_clean_output_window_loop_with_runtime_and_sleep(
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable,
            NonZeroU32::new(2).expect("2 should be non-zero"),
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );
        let formatted = format_four_view_clean_output_window_loop_summary(&summary);

        assert!(formatted.contains("command_name=--four-view-clean-output-window-loop"));
        assert!(formatted.contains("fixture_mode=all-renderable"));
        assert!(formatted.contains("clean_output_window=true"));
        assert!(formatted.contains("actual_window_render=true"));
        assert!(formatted.contains("real_handoff=false"));
        assert!(formatted.contains(&format!(
            "window_title={}",
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        )));
        assert!(formatted.contains("frames_attempted=2"));
        assert!(formatted.contains("frames_rendered=2"));
        assert!(formatted.contains("render_failures=0"));
        assert!(formatted.contains("window_created=true"));
        assert!(formatted.contains("persistent_window=true"));
        assert!(formatted.contains("window_updates=2"));
        assert!(formatted.contains("window_closed=true"));
        assert!(formatted.contains("source_width=4"));
        assert!(formatted.contains("source_height=2"));
        assert!(formatted.contains(&format!(
            "output_width={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
        )));
        assert!(formatted.contains(&format!(
            "output_height={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
        )));
        assert!(formatted.contains(&format!(
            "scale_mode={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_SCALE_MODE
        )));
        assert!(formatted.contains("window_visible=true"));
        assert!(formatted.contains("window_capture_candidate=true"));
        assert!(formatted.contains(&format!(
            "bgra_payload_len={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH as usize
                * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT as usize
                * 4
        )));
    }
}
