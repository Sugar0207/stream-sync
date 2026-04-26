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
    SwitcherDecodeLatestFrameOnceBoundary, SwitcherDecodeLatestFrameOnceInput,
    SwitcherDecodeLatestFrameOnceResult, SwitcherDecodedFrame, SwitcherDecodedFramePixelFormat,
    SwitcherFfmpegH264DecodeRuntimeHook, SwitcherPlaceholderManualVerificationBoundary,
    SwitcherPlaceholderManualVerificationInput, SwitcherPlaceholderManualVerificationResult,
    SwitcherTwoViewComposedCanvasRenderBoundary, SwitcherTwoViewComposedCanvasRenderResult,
    SwitcherTwoViewCompositionBoundary, SwitcherTwoViewCompositionInput,
    SwitcherTwoViewCompositionResult, SwitcherTwoViewLayoutPolicy, SwitcherTwoViewLayoutSideInput,
    SwitcherTwoViewManualVerificationBoundary, SwitcherTwoViewManualVerificationInput,
    SwitcherTwoViewManualVerificationResult, SwitcherTwoViewManualVerificationSideSummary,
    SwitcherTwoViewSide, SwitcherTwoViewTargetTimeSelectionPolicy, SwitcherWindowRenderBoundary,
    SwitcherWindowRenderResult,
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
        _ => {
            println!(
                "stream-sync-switcher scaffold; use --placeholder-fixture-once [client-id], --placeholder-empty-once [client-id], --decode-latest-frame-once [client-id] [output-path], --receive-auth-video-placeholder-bridge-once [config-path] [client-id], --receive-auth-video-decode-latest-once [config-path] [client-id] [output-path], --receive-auth-video-render-decoded-once [config-path] [client-id] [hold-ms], --two-view-sync-fixture-once [left-client-id] [right-client-id] [hold-ms], or --render-two-view-composed-fixture-once [hold-ms]"
            );
        }
    }
}

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
