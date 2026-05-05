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
                "stream-sync-switcher scaffold; use --placeholder-fixture-once [client-id], --placeholder-empty-once [client-id], --decode-latest-frame-once [client-id] [output-path], --receive-auth-video-placeholder-bridge-once [config-path] [client-id], --receive-auth-video-decode-latest-once [config-path] [client-id] [output-path], --receive-auth-video-render-decoded-once [config-path] [client-id] [hold-ms], --two-view-sync-fixture-once [left-client-id] [right-client-id] [hold-ms], --render-two-view-composed-fixture-once [hold-ms], --live-two-view-switcher-once [config-path] [left-client-id] [right-client-id], --four-view-proof-fixture-once [all-renderable|mixed-placeholder-source-error|placeholder-only], --four-view-proof-window-once [all-renderable], --four-view-clean-output-window-once [all-renderable], or --read-queued-frame-handoff-once [pipe-name] [client-id] [run-id] [read-mode] [request-id]"
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
    match value.as_deref() {
        Some("all-renderable") | None => {
            SwitcherFourViewManualPreviewProofFixtureMode::AllRenderable
        }
        Some(_) => {
            eprintln!("invalid fixture-mode: expected all-renderable");
            std::process::exit(1);
        }
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
        format_four_view_clean_output_window_summary,
        format_four_view_manual_preview_proof_summary,
        format_four_view_manual_preview_window_summary, format_handoff_mode,
        format_handoff_read_mode, format_named_pipe_handoff_switcher_result_summary,
        format_named_pipe_handoff_switcher_summary,
        parse_four_view_actual_window_fixture_mode_or_exit,
        parse_four_view_manual_preview_fixture_mode_or_exit, parse_handoff_mode_or_exit,
        run_four_view_clean_output_window_with_runtime, run_four_view_manual_preview_proof_once,
        run_four_view_manual_preview_proof_with_runtime, SwitcherQueuedFrameHandoffResult,
        SwitcherSingleClientQueueSourceMode,
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
}
