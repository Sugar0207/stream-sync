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
    SwitcherLiveTwoViewManualRuntimeBoundary, SwitcherLiveTwoViewManualRuntimeResult,
    SwitcherPlaceholderManualVerificationBoundary, SwitcherPlaceholderManualVerificationInput,
    SwitcherPlaceholderManualVerificationResult, SwitcherQueuedFrameHandoffInput,
    SwitcherQueuedFrameHandoffResult, SwitcherSingleClientQueueSourceMode,
    SwitcherTwoViewComposedCanvasRenderBoundary, SwitcherTwoViewComposedCanvasRenderResult,
    SwitcherTwoViewCompositionBoundary, SwitcherTwoViewCompositionInput,
    SwitcherTwoViewCompositionResult, SwitcherTwoViewLayoutPolicy, SwitcherTwoViewLayoutSideInput,
    SwitcherTwoViewManualVerificationBoundary, SwitcherTwoViewManualVerificationInput,
    SwitcherTwoViewManualVerificationResult, SwitcherTwoViewManualVerificationSideSummary,
    SwitcherTwoViewSide, SwitcherTwoViewTargetTimeSelectionPolicy, SwitcherWindowRenderBoundary,
    SwitcherWindowRenderResult,
};
#[cfg(target_os = "windows")]
use stream_sync_switcher::{
    SwitcherNamedPipeQueuedFrameHandoff, SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    SwitcherNamedPipeQueuedFrameHandoffRequestOutput,
    SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
    SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
};

#[cfg(not(target_os = "windows"))]
use stream_sync_switcher::SwitcherUnavailableWindowRenderRuntimeHook;
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
                "stream-sync-switcher scaffold; use --placeholder-fixture-once [client-id], --placeholder-empty-once [client-id], --decode-latest-frame-once [client-id] [output-path], --receive-auth-video-placeholder-bridge-once [config-path] [client-id], --receive-auth-video-decode-latest-once [config-path] [client-id] [output-path], --receive-auth-video-render-decoded-once [config-path] [client-id] [hold-ms], --two-view-sync-fixture-once [left-client-id] [right-client-id] [hold-ms], --render-two-view-composed-fixture-once [hold-ms], --live-two-view-switcher-once [config-path] [left-client-id] [right-client-id], or --read-queued-frame-handoff-once [pipe-name] [client-id] [run-id] [read-mode] [request-id]"
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
    format_named_pipe_handoff_switcher_result_summary(
        &output.summary.pipe_name,
        output.summary.request_id,
        result_client_id(&output.result),
        result_run_id(&output.result),
        handoff_read_mode_from_switcher_mode(output.summary.read_mode),
        format_named_pipe_request_status(output.summary.request_status),
        format_named_pipe_response_status(output.summary.response_status),
        output.summary.timeout_millis,
        output.summary.elapsed_millis,
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
    timeout_millis: u32,
    elapsed_millis: u64,
    result: &SwitcherQueuedFrameHandoffResult,
) -> String {
    match result {
        SwitcherQueuedFrameHandoffResult::FrameRead {
            frame,
            remaining_client_queue_len,
            ..
        } => format!(
            "switcher named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=FrameRead queue_len={} frame_id={} capture_timestamp={} send_timestamp={} queued_at={} width={} height={} fps_nominal={} codec={:?} is_keyframe={} encoded_payload_len={}",
            pipe_name,
            request_id,
            client_id.0,
            run_id.0,
            format_handoff_read_mode(read_mode),
            timeout_millis,
            elapsed_millis,
            request_status,
            response_status,
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
            "switcher named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=NoFrame queue_len={}",
            pipe_name,
            request_id,
            client_id.0,
            run_id.0,
            format_handoff_read_mode(read_mode),
            timeout_millis,
            elapsed_millis,
            request_status,
            response_status,
            client_queue_len
        ),
        SwitcherQueuedFrameHandoffResult::HandoffError { error, .. } => format!(
            "switcher named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=HandoffError queue_len=none handoff_error={:?}",
            pipe_name,
            request_id,
            client_id.0,
            run_id.0,
            format_handoff_read_mode(read_mode),
            timeout_millis,
            elapsed_millis,
            request_status,
            response_status,
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
        SwitcherNamedPipeQueuedFrameHandoffRequestOutput,
        SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
        SwitcherNamedPipeQueuedFrameHandoffRequestSummary,
        SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
        SwitcherNamedPipeQueuedFrameHandoffResultKind, SwitcherSingleViewSelectedEncodedFrame,
    };

    use super::{
        format_handoff_mode, format_handoff_read_mode,
        format_named_pipe_handoff_switcher_result_summary,
        format_named_pipe_handoff_switcher_summary, parse_handoff_mode_or_exit,
        SwitcherQueuedFrameHandoffError, SwitcherQueuedFrameHandoffResult,
        SwitcherSingleClientQueueSourceMode,
    };

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
            5000,
            17,
            &result,
        );

        assert!(summary.contains("request_id=88"));
        assert!(summary.contains("timeout_millis=5000"));
        assert!(summary.contains("elapsed_millis=17"));
        assert!(summary.contains("result_kind=FrameRead"));
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
                timeout_millis: 2500,
                request_status: SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodeFailed,
                response_status: SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None,
                result_kind: SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError,
                elapsed_millis: 4,
            },
            runtime: None,
            result,
        };

        let summary = format_named_pipe_handoff_switcher_summary(&output);

        assert!(summary.contains("request_status=encode_failed"));
        assert!(summary.contains("response_status=none"));
        assert!(summary.contains("result_kind=HandoffError"));
        assert!(summary.contains("handoff_error=MalformedResponse"));
        assert!(summary.contains("timeout_millis=2500"));
        assert!(summary.contains("elapsed_millis=4"));
    }
}
