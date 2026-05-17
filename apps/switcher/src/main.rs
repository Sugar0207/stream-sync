use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(target_os = "windows")]
use std::{
    fs::File,
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
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
    SwitcherDecodedFramePixelFormat, SwitcherDecodedFrameRenderInput,
    SwitcherFfmpegH264DecodeRuntimeHook, SwitcherFourViewCleanOutputWindowBoundary,
    SwitcherFourViewCleanOutputWindowInput, SwitcherFourViewCleanOutputWindowOutput,
    SwitcherFourViewCleanOutputWindowProofBoundary, SwitcherFourViewCleanOutputWindowProofResult,
    SwitcherFourViewCleanOutputWindowRenderResult, SwitcherFourViewComposedCanvasRenderResult,
    SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult,
    SwitcherFourViewComposedFrame, SwitcherFourViewDisplayedSlot,
    SwitcherFourViewHandoffQuadCompositionRenderSlot, SwitcherFourViewHandoffValidationBoundary,
    SwitcherFourViewHandoffValidationInput, SwitcherFourViewHandoffValidationOutput,
    SwitcherFourViewHandoffValidationPreCompositionOutput,
    SwitcherFourViewManualPreviewCompositionInstructionKind,
    SwitcherFourViewManualPreviewDisplaySlotKind, SwitcherFourViewManualPreviewProofBoundary,
    SwitcherFourViewManualPreviewProofFixtureMode, SwitcherFourViewManualPreviewProofInput,
    SwitcherFourViewManualPreviewProofResult, SwitcherFourViewManualPreviewProofSummary,
    SwitcherFourViewManualPreviewSchedulerSlotKind, SwitcherFourViewQuadComposedSlotKind,
    SwitcherFourViewQuadComposedSlotMetadata, SwitcherFourViewQuadComposedSlotRect,
    SwitcherFourViewQuadCompositionInvalidReason, SwitcherFourViewQuadCompositionOutput,
    SwitcherFourViewQuadCompositionResult, SwitcherFourViewQuadLayoutPolicy,
    SwitcherFourViewQuadRenderFacingConnectionBoundary, SwitcherFourViewTargetTimeSourceSlotConfig,
    SwitcherH264AnnexBPayloadInspectionBoundary, SwitcherH264DecodeDeferredReason,
    SwitcherH264DecodeFailure, SwitcherH264DecodeInput, SwitcherH264DecodeResult,
    SwitcherH264DecodeRuntimeDiagnostics, SwitcherH264DecodeRuntimeHook,
    SwitcherH264DecodeSourceIdentity, SwitcherLiveTwoViewManualRuntimeBoundary,
    SwitcherLiveTwoViewManualRuntimeResult, SwitcherPersistentFfmpegH264DecodeRuntimeHook,
    SwitcherPlaceholderManualVerificationBoundary, SwitcherPlaceholderManualVerificationInput,
    SwitcherPlaceholderManualVerificationResult, SwitcherQueuedFrameHandoff,
    SwitcherQueuedFrameHandoffInput, SwitcherQueuedFrameHandoffResult,
    SwitcherSingleClientQueueSourceMode, SwitcherSingleClientTargetTimeHandoffSourceResult,
    SwitcherTwoViewComposedCanvasRenderBoundary, SwitcherTwoViewComposedCanvasRenderResult,
    SwitcherTwoViewCompositionBoundary, SwitcherTwoViewCompositionInput,
    SwitcherTwoViewCompositionResult, SwitcherTwoViewLayoutPolicy, SwitcherTwoViewLayoutSideInput,
    SwitcherTwoViewManualVerificationBoundary, SwitcherTwoViewManualVerificationInput,
    SwitcherTwoViewManualVerificationResult, SwitcherTwoViewManualVerificationSideSummary,
    SwitcherTwoViewSide, SwitcherTwoViewTargetTimeSelectionPolicy,
    SwitcherUnavailableWindowRenderRuntimeHook, SwitcherWindowRenderBoundary,
    SwitcherWindowRenderRequest, SwitcherWindowRenderResult, SwitcherWindowRenderRuntimeHook,
    SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE,
};

#[cfg(target_os = "windows")]
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
        },
        System::Console::{
            GetConsoleMode, GetStdHandle, SetConsoleMode, CONSOLE_MODE, ENABLE_ECHO_INPUT,
            ENABLE_LINE_INPUT, STD_INPUT_HANDLE,
        },
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
            PIPE_WAIT,
        },
    },
};

#[cfg(target_os = "windows")]
use stream_sync_switcher::{
    SwitcherNamedPipeQueuedFrameHandoff, SwitcherNamedPipeQueuedFrameHandoffRequestConfig,
    SwitcherNamedPipeQueuedFrameHandoffRequestOutput,
    SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
    SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
    SwitcherNamedPipeQueuedFrameHandoffRetryClassification,
    SwitcherNamedPipeQueuedFrameHandoffRuntime,
};

#[cfg(target_os = "windows")]
use stream_sync_switcher::SwitcherWindowsGdiWindowRenderRuntimeHook;

thread_local! {
    static REUSABLE_OBS_RENDER_BUFFER: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

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
        Some("--four-view-real-handoff-preview-loop") => {
            let pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing pipe-name");
                std::process::exit(1);
            });
            let real_slot_index = parse_four_view_real_slot_index_or_exit(args.next());
            let client_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client-id");
                std::process::exit(1);
            }));
            let run_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run-id");
                std::process::exit(1);
            }));
            let frames = parse_positive_u32_arg_or_exit(args.next(), "frames");
            match run_four_view_real_handoff_preview_loop(
                &pipe_name,
                real_slot_index,
                client_id,
                run_id,
                frames,
            ) {
                Ok(summary) => println!(
                    "{}",
                    format_four_view_real_handoff_preview_loop_summary(&summary)
                ),
                Err(error) => {
                    eprintln!("switcher four-view real handoff preview loop failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("--four-view-two-real-handoff-preview-loop") => {
            let pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing pipe-name");
                std::process::exit(1);
            });
            let slot0_index = parse_four_view_real_slot_index_or_exit(args.next());
            let client0_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client0-id");
                std::process::exit(1);
            }));
            let run0_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run0-id");
                std::process::exit(1);
            }));
            let slot1_index = parse_four_view_real_slot_index_or_exit(args.next());
            let client1_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client1-id");
                std::process::exit(1);
            }));
            let run1_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run1-id");
                std::process::exit(1);
            }));
            validate_distinct_four_view_real_slot_indices_or_exit(slot0_index, slot1_index);
            let frames = parse_positive_u32_arg_or_exit(args.next(), "frames");
            let read_mode = parse_optional_real_handoff_preview_mode_or_exit(args.next());
            match run_four_view_two_real_handoff_preview_loop(
                &pipe_name,
                slot0_index,
                client0_id,
                run0_id,
                slot1_index,
                client1_id,
                run1_id,
                frames,
                read_mode,
            ) {
                Ok(summary) => println!(
                    "{}",
                    format_four_view_two_real_handoff_preview_loop_summary(&summary)
                ),
                Err(error) => {
                    eprintln!("switcher four-view two-real handoff preview loop failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("--four-view-four-real-handoff-preview-loop") => {
            let pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing pipe-name");
                std::process::exit(1);
            });
            let client0_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client0-id");
                std::process::exit(1);
            }));
            let run0_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run0-id");
                std::process::exit(1);
            }));
            let client1_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client1-id");
                std::process::exit(1);
            }));
            let run1_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run1-id");
                std::process::exit(1);
            }));
            let client2_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client2-id");
                std::process::exit(1);
            }));
            let run2_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run2-id");
                std::process::exit(1);
            }));
            let client3_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client3-id");
                std::process::exit(1);
            }));
            let run3_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run3-id");
                std::process::exit(1);
            }));
            let frames = parse_positive_u32_arg_or_exit(args.next(), "frames");
            let read_mode = parse_optional_real_handoff_preview_mode_or_exit(args.next());
            match run_four_view_four_real_handoff_preview_loop(
                &pipe_name, client0_id, run0_id, client1_id, run1_id, client2_id, run2_id,
                client3_id, run3_id, frames, read_mode,
            ) {
                Ok(summary) => println!(
                    "{}",
                    format_four_view_four_real_handoff_preview_loop_summary(&summary)
                ),
                Err(error) => {
                    eprintln!("switcher four-view four-real handoff preview loop failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("--four-view-focused-handoff-preview-loop") => {
            let pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing pipe-name");
                std::process::exit(1);
            });
            let focused_slot_index = parse_four_view_real_slot_index_or_exit(args.next());
            let client0_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client0-id");
                std::process::exit(1);
            }));
            let run0_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run0-id");
                std::process::exit(1);
            }));
            let client1_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client1-id");
                std::process::exit(1);
            }));
            let run1_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run1-id");
                std::process::exit(1);
            }));
            let client2_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client2-id");
                std::process::exit(1);
            }));
            let run2_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run2-id");
                std::process::exit(1);
            }));
            let client3_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client3-id");
                std::process::exit(1);
            }));
            let run3_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run3-id");
                std::process::exit(1);
            }));
            let frames = parse_positive_u32_arg_or_exit(args.next(), "frames");
            match run_four_view_focused_handoff_preview_loop(
                &pipe_name,
                focused_slot_index,
                client0_id,
                run0_id,
                client1_id,
                run1_id,
                client2_id,
                run2_id,
                client3_id,
                run3_id,
                frames,
            ) {
                Ok(summary) => println!(
                    "{}",
                    format_four_view_focused_handoff_preview_loop_summary(&summary)
                ),
                Err(error) => {
                    eprintln!("switcher four-view focused handoff preview loop failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("--four-view-controlled-handoff-preview-loop") => {
            let pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing pipe-name");
                std::process::exit(1);
            });
            let client0_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client0-id");
                std::process::exit(1);
            }));
            let run0_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run0-id");
                std::process::exit(1);
            }));
            let client1_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client1-id");
                std::process::exit(1);
            }));
            let run1_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run1-id");
                std::process::exit(1);
            }));
            let client2_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client2-id");
                std::process::exit(1);
            }));
            let run2_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run2-id");
                std::process::exit(1);
            }));
            let client3_id = ClientId(args.next().unwrap_or_else(|| {
                eprintln!("missing client3-id");
                std::process::exit(1);
            }));
            let run3_id = RunId(args.next().unwrap_or_else(|| {
                eprintln!("missing run3-id");
                std::process::exit(1);
            }));
            let max_ticks_per_command =
                parse_positive_u32_arg_or_exit(args.next(), "max-ticks-per-command");
            let command_source =
                parse_four_view_control_command_source_or_exit(args.collect::<Vec<_>>());
            match run_four_view_controlled_handoff_preview_loop(
                &pipe_name,
                client0_id,
                run0_id,
                client1_id,
                run1_id,
                client2_id,
                run2_id,
                client3_id,
                run3_id,
                max_ticks_per_command,
                command_source,
            ) {
                Ok(summary) => {
                    for command_summary in &summary.command_summaries {
                        println!(
                            "{}",
                            format_four_view_controlled_handoff_preview_command_summary(
                                command_summary
                            )
                        );
                    }
                    println!(
                        "{}",
                        format_four_view_controlled_handoff_preview_loop_summary(&summary)
                    );
                }
                Err(error) => {
                    eprintln!("switcher four-view controlled handoff preview loop failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("--four-view-operator-wrapper") => {
            let control_pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing control-pipe-name");
                std::process::exit(1);
            });
            let input_source =
                parse_four_view_operator_wrapper_input_source_or_exit(args.collect::<Vec<_>>());
            match run_four_view_operator_wrapper(&control_pipe_name, input_source) {
                Ok(summary) => {
                    for key_summary in &summary.key_summaries {
                        println!(
                            "{}",
                            format_four_view_operator_wrapper_key_summary(key_summary)
                        );
                    }
                    println!(
                        "{}",
                        format_four_view_operator_wrapper_loop_summary(&summary)
                    );
                }
                Err(error) => {
                    eprintln!("switcher four-view operator wrapper failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("--send-control-command") => {
            let control_pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing control-pipe-name");
                std::process::exit(1);
            });
            let command = args.next().unwrap_or_else(|| {
                eprintln!("missing command");
                std::process::exit(1);
            });
            if args.next().is_some() {
                eprintln!("unexpected extra arguments");
                std::process::exit(1);
            }
            match run_send_control_command(&control_pipe_name, &command) {
                Ok(response) => println!("{response}"),
                Err(error) => {
                    eprintln!("switcher send-control-command failed: {error}");
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
                "stream-sync-switcher scaffold; use --placeholder-fixture-once [client-id], --placeholder-empty-once [client-id], --decode-latest-frame-once [client-id] [output-path], --receive-auth-video-placeholder-bridge-once [config-path] [client-id], --receive-auth-video-decode-latest-once [config-path] [client-id] [output-path], --receive-auth-video-render-decoded-once [config-path] [client-id] [hold-ms], --two-view-sync-fixture-once [left-client-id] [right-client-id] [hold-ms], --render-two-view-composed-fixture-once [hold-ms], --live-two-view-switcher-once [config-path] [left-client-id] [right-client-id], --four-view-proof-fixture-once [all-renderable|mixed-placeholder-source-error|placeholder-only], --four-view-proof-window-once [all-renderable], --four-view-clean-output-window-once [all-renderable], --four-view-clean-output-window-loop [all-renderable] [frames], --four-view-real-handoff-preview-loop [pipe-name] [real-slot-index] [client-id] [run-id] [frames], --four-view-two-real-handoff-preview-loop [pipe-name] [slot0-index] [client0-id] [run0-id] [slot1-index] [client1-id] [run1-id] [frames] [preview-oldest|preview-latest|preview-latest-decodable], --four-view-four-real-handoff-preview-loop [pipe-name] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [frames] [preview-oldest|preview-latest|preview-latest-decodable], --four-view-focused-handoff-preview-loop [pipe-name] [focused-slot-index] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [frames], --four-view-controlled-handoff-preview-loop [pipe-name] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [max-ticks-per-command] [--commands \"status;focus 0;all;quit\"|--control-pipe streamsync-control-dev], --four-view-operator-wrapper [control-pipe-name] [--keys \"s;1;2;3;4;0;q;q\"|--raw-keys], --send-control-command [control-pipe-name] [command], or --read-queued-frame-handoff-once [pipe-name] [client-id] [run-id] [read-mode] [request-id]"
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

fn parse_optional_four_view_control_script(args: Vec<String>) -> Result<Option<String>, String> {
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() != 2 || args[0] != "--commands" {
        return Err(
            "unexpected extra arguments: expected optional --commands \"cmd;cmd;quit\"".to_string(),
        );
    }
    Ok(Some(args[1].clone()))
}

fn parse_four_view_control_command_source(
    args: Vec<String>,
) -> Result<FourViewControlCommandSource, String> {
    if !args.iter().any(|arg| arg == "--control-pipe") {
        return parse_optional_four_view_control_script(args).map(|scripted_commands| {
            scripted_commands
                .map(FourViewControlCommandSource::Scripted)
                .unwrap_or(FourViewControlCommandSource::Stdin)
        });
    }

    let mut scripted_commands = None;
    let mut control_pipe_name = None;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--commands" => {
                if scripted_commands.is_some() || control_pipe_name.is_some() {
                    return Err(
                        "use either --commands \"cmd;cmd;quit\" or --control-pipe pipe-name"
                            .to_string(),
                    );
                }
                let Some(script) = args.get(index + 1) else {
                    return Err("missing value for --commands".to_string());
                };
                scripted_commands = Some(script.clone());
                index += 2;
            }
            "--control-pipe" => {
                if scripted_commands.is_some() || control_pipe_name.is_some() {
                    return Err(
                        "use either --commands \"cmd;cmd;quit\" or --control-pipe pipe-name"
                            .to_string(),
                    );
                }
                let Some(pipe_name) = args.get(index + 1) else {
                    return Err("missing value for --control-pipe".to_string());
                };
                if pipe_name.trim().is_empty() {
                    return Err("control pipe name must not be empty".to_string());
                }
                control_pipe_name = Some(pipe_name.clone());
                index += 2;
            }
            _ => {
                return Err(
                    "unexpected extra arguments: expected optional --commands \"cmd;cmd;quit\" or --control-pipe pipe-name"
                        .to_string(),
                )
            }
        }
    }

    Ok(match (scripted_commands, control_pipe_name) {
        (Some(script), None) => FourViewControlCommandSource::Scripted(script),
        (None, Some(pipe_name)) => FourViewControlCommandSource::ControlPipe(pipe_name),
        (None, None) => FourViewControlCommandSource::Stdin,
        (Some(_), Some(_)) => unreachable!("mutually exclusive control sources are guarded"),
    })
}

fn parse_four_view_control_command_source_or_exit(
    args: Vec<String>,
) -> FourViewControlCommandSource {
    parse_four_view_control_command_source(args).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    })
}

fn parse_four_view_operator_wrapper_input_source(
    args: Vec<String>,
) -> Result<FourViewOperatorWrapperInputSource, String> {
    match args.as_slice() {
        [] => Ok(FourViewOperatorWrapperInputSource::Stdin),
        [flag, keys] if flag == "--keys" => Ok(FourViewOperatorWrapperInputSource::ScriptedKeys(
            keys.clone(),
        )),
        [flag] if flag == "--raw-keys" => Ok(FourViewOperatorWrapperInputSource::RawKeys),
        _ => Err(
            "unexpected extra arguments: expected optional --keys \"s;1;2;3;4;0;q;q\" or --raw-keys".to_string(),
        ),
    }
}

fn parse_four_view_operator_wrapper_input_source_or_exit(
    args: Vec<String>,
) -> FourViewOperatorWrapperInputSource {
    parse_four_view_operator_wrapper_input_source(args).unwrap_or_else(|error| {
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
        Some("preview-latest-decodable") => {
            SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable
        }
        Some("consume-oldest") => SwitcherSingleClientQueueSourceMode::ConsumeOldest,
        Some(_) => {
            eprintln!(
                "invalid {name}: expected preview-oldest, preview-latest, preview-latest-decodable, or consume-oldest"
            );
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

fn format_handoff_mode(mode: SwitcherSingleClientQueueSourceMode) -> &'static str {
    match mode {
        SwitcherSingleClientQueueSourceMode::PreviewOldest => "preview-oldest",
        SwitcherSingleClientQueueSourceMode::PreviewLatest => "preview-latest",
        SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable => "preview-latest-decodable",
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
        SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable => {
            stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatestDecodable
        }
        SwitcherSingleClientQueueSourceMode::ConsumeOldest => {
            stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::DequeueOldest
        }
    }
}

fn preview_target_time_mode_from_switcher_mode(
    mode: SwitcherSingleClientQueueSourceMode,
) -> stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode {
    match mode {
        SwitcherSingleClientQueueSourceMode::PreviewOldest => {
            stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode::PreviewOldestIfAtOrBefore
        }
        SwitcherSingleClientQueueSourceMode::PreviewLatest => {
            stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore
        }
        SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable => {
            stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode::PreviewLatestDecodableIfAtOrBefore
        }
        SwitcherSingleClientQueueSourceMode::ConsumeOldest => {
            stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode::ConsumeOldestAtOrBefore
        }
    }
}

fn format_actual_handoff_pipe_path(pipe_name: &str) -> String {
    stream_sync_net_core::normalize_windows_local_named_pipe_path(pipe_name)
        .unwrap_or_else(|_| "invalid".to_string())
}

#[cfg(windows)]
fn format_named_pipe_handoff_switcher_summary(
    output: &SwitcherNamedPipeQueuedFrameHandoffRequestOutput,
) -> String {
    let last_error = format_named_pipe_last_error(output.summary.last_error.as_ref());
    let handoff_response_kind = output
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.response.as_ref())
        .map(format_handoff_response_kind)
        .unwrap_or("none");
    let response_payload_len = output
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.response_payload_len)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    let parse_error = sanitize_summary_value(
        output
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.parse_error.clone())
            .or_else(|| output.summary.local_error.clone())
            .as_deref()
            .unwrap_or("none"),
    );
    let io_error = sanitize_summary_value(
        output
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.io_error.clone())
            .as_deref()
            .unwrap_or("none"),
    );
    format_named_pipe_handoff_switcher_result_summary(
        &output.summary.pipe_name,
        output
            .summary
            .actual_pipe_path
            .as_deref()
            .unwrap_or("invalid"),
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
        handoff_response_kind,
        &response_payload_len,
        &parse_error,
        &io_error,
        &output.result,
    )
}

fn format_named_pipe_handoff_switcher_result_summary(
    pipe_name: &str,
    actual_pipe_path: &str,
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
    handoff_response_kind: &str,
    response_payload_len: &str,
    parse_error: &str,
    io_error: &str,
    result: &SwitcherQueuedFrameHandoffResult,
) -> String {
    match result {
        SwitcherQueuedFrameHandoffResult::FrameRead {
            frame,
            remaining_client_queue_len,
            ..
        } => format!(
            "switcher named-pipe handoff once pipe_name={} actual_pipe_path={} request_id={} client_id={} run_id={} read_mode={} attempt_count={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=FrameRead final_result={} last_error={} retry_classification={} handoff_response_kind={} response_payload_len={} parse_error={} io_error={} queue_len={} frame_id={} capture_timestamp={} send_timestamp={} queued_at={} width={} height={} fps_nominal={} codec={:?} is_keyframe={} encoded_payload_len={}",
            pipe_name,
            actual_pipe_path,
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
            handoff_response_kind,
            response_payload_len,
            parse_error,
            io_error,
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
            "switcher named-pipe handoff once pipe_name={} actual_pipe_path={} request_id={} client_id={} run_id={} read_mode={} attempt_count={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=NoFrame final_result={} last_error={} retry_classification={} handoff_response_kind={} response_payload_len={} parse_error={} io_error={} queue_len={}",
            pipe_name,
            actual_pipe_path,
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
            handoff_response_kind,
            response_payload_len,
            parse_error,
            io_error,
            client_queue_len
        ),
        SwitcherQueuedFrameHandoffResult::HandoffError { error, .. } => format!(
            "switcher named-pipe handoff once pipe_name={} actual_pipe_path={} request_id={} client_id={} run_id={} read_mode={} attempt_count={} timeout_millis={} elapsed_millis={} request_status={} response_status={} result_kind=HandoffError final_result={} last_error={} retry_classification={} handoff_response_kind={} response_payload_len={} parse_error={} io_error={} queue_len=none handoff_error={:?}",
            pipe_name,
            actual_pipe_path,
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
            handoff_response_kind,
            response_payload_len,
            parse_error,
            io_error,
            error
        ),
    }
}

#[cfg(windows)]
fn format_handoff_response_kind(
    response: &stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse,
) -> &'static str {
    match response {
        stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::FrameRead { .. } => {
            "FrameRead"
        }
        stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::NoFrame { .. } => "NoFrame",
        stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::HandoffError { .. } => {
            "HandoffError"
        }
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
        stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatestDecodable => {
            "inspect-latest-decodable"
        }
        stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::DequeueOldest => "dequeue-oldest",
    }
}

fn format_handoff_decodable_source(
    source: stream_sync_net_core::ServerSwitcherQueuedFrameDecodableSource,
) -> &'static str {
    match source {
        stream_sync_net_core::ServerSwitcherQueuedFrameDecodableSource::None => "none",
        stream_sync_net_core::ServerSwitcherQueuedFrameDecodableSource::Queue => "queue",
        stream_sync_net_core::ServerSwitcherQueuedFrameDecodableSource::RetainedKeyframe => {
            "retained_keyframe"
        }
    }
}

fn format_handoff_no_frame_reason(
    reason: stream_sync_net_core::ServerSwitcherQueuedFrameNoFrameReason,
) -> &'static str {
    match reason {
        stream_sync_net_core::ServerSwitcherQueuedFrameNoFrameReason::NoFramesQueuedForClient => {
            "NoFramesQueuedForClient"
        }
        stream_sync_net_core::ServerSwitcherQueuedFrameNoFrameReason::NoFramesQueuedForRequestedRun => {
            "NoFramesQueuedForRequestedRun"
        }
        stream_sync_net_core::ServerSwitcherQueuedFrameNoFrameReason::NoDecodableFrameAvailable => {
            "NoDecodableFrameAvailable"
        }
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

fn parse_four_view_real_slot_index_or_exit(value: Option<String>) -> usize {
    let Some(raw) = value else {
        eprintln!("missing real-slot-index");
        std::process::exit(1);
    };
    let Ok(real_slot_index) = raw.parse::<usize>() else {
        eprintln!("invalid real-slot-index: expected integer 0..3");
        std::process::exit(1);
    };
    if real_slot_index > 3 {
        eprintln!("invalid real-slot-index: expected integer 0..3");
        std::process::exit(1);
    }
    real_slot_index
}

fn validate_distinct_four_view_real_slot_indices(
    slot0_index: usize,
    slot1_index: usize,
) -> Result<(), String> {
    if slot0_index == slot1_index {
        return Err(
            "invalid slot indices: slot0-index and slot1-index must be distinct".to_string(),
        );
    }
    Ok(())
}

fn validate_distinct_four_view_real_slot_indices_or_exit(slot0_index: usize, slot1_index: usize) {
    validate_distinct_four_view_real_slot_indices(slot0_index, slot1_index).unwrap_or_else(
        |error| {
            eprintln!("{error}");
            std::process::exit(1);
        },
    );
}

fn parse_optional_real_handoff_preview_mode_or_exit(
    value: Option<String>,
) -> SwitcherSingleClientQueueSourceMode {
    match value {
        Some(raw) => {
            let mode = parse_handoff_mode_or_exit(Some(raw), "read-mode");
            match mode {
                SwitcherSingleClientQueueSourceMode::PreviewOldest
                | SwitcherSingleClientQueueSourceMode::PreviewLatest
                | SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable => mode,
                SwitcherSingleClientQueueSourceMode::ConsumeOldest => {
                    eprintln!(
                        "invalid read-mode: expected preview-oldest, preview-latest, or preview-latest-decodable"
                    );
                    std::process::exit(1);
                }
            }
        }
        None => SwitcherSingleClientQueueSourceMode::PreviewLatest,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwitcherFourViewRealHandoffPreviewLoopSummary {
    real_slot_index: usize,
    pipe_name: String,
    actual_pipe_path: String,
    client_id: ClientId,
    run_id: RunId,
    frames_attempted: u32,
    frames_rendered: u32,
    render_failures: u32,
    scheduler_status: stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus,
    slot_bindings: [String; 4],
    slot_result_kinds: [String; 4],
    slot_diagnostics: [String; 4],
    clean_output_render_result_kind: &'static str,
    window_title: String,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwitcherFourViewTwoRealHandoffPreviewLoopSummary {
    real_slot0_index: usize,
    real_slot1_index: usize,
    pipe_name: String,
    actual_pipe_path: String,
    preview_mode: &'static str,
    read_mode: &'static str,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    frames_attempted: u32,
    frames_rendered: u32,
    render_failures: u32,
    elapsed_ms: u128,
    target_fps: u32,
    configured_frame_interval_ms: u128,
    effective_attempt_fps: String,
    effective_render_fps: String,
    first_render_attempt_index: Option<u32>,
    first_render_elapsed_ms: Option<u128>,
    rendered_after_first_render: u32,
    effective_render_fps_after_first_render: String,
    no_render_before_first_render: u32,
    selected_count: u32,
    no_frame_count: u32,
    handoff_error_count: u32,
    decode_attempt_count: u32,
    decode_success_count: u32,
    render_success_count: u32,
    render_failure_count: u32,
    unchanged_frame_reuse_count: u32,
    skipped_decode_unchanged_frame_count: u32,
    redecoded_same_frame_count: u32,
    decode_elapsed_ms: u128,
    decode_process_spawn_elapsed_ms: u128,
    decode_input_write_elapsed_ms: u128,
    decode_input_payload_bytes_total: usize,
    decode_output_read_elapsed_ms: u128,
    decode_output_read_exact_elapsed_ms: u128,
    decode_output_vec_resize_elapsed_ms: u128,
    decode_process_wait_elapsed_ms: u128,
    decode_pixel_convert_elapsed_ms: u128,
    decode_buffer_allocation_count: u32,
    decode_output_bytes_total: usize,
    decode_stdout_expected_bytes_total: usize,
    decode_cached_frame_reuse_count: u32,
    decode_cache_miss_count: u32,
    decoded_buffer_clone_count: u32,
    decode_cache_hit_clone_count: u32,
    decode_cache_store_clone_count: u32,
    decoded_buffer_clone_elapsed_ms: u128,
    composed_buffer_clone_count: u32,
    decode_output_buffer_reuse_count: u32,
    persistent_decode_enabled: bool,
    persistent_decode_attempt_count: u32,
    persistent_decode_success_count: u32,
    persistent_decode_failure_count: u32,
    persistent_decode_fallback_count: u32,
    persistent_decode_process_spawn_count: u32,
    persistent_decode_process_restart_count: u32,
    persistent_decode_stdin_write_elapsed_ms: u128,
    persistent_decode_stdout_read_elapsed_ms: u128,
    persistent_decode_stdout_read_exact_elapsed_ms: u128,
    persistent_decode_output_bytes_total: usize,
    persistent_decode_last_error: String,
    one_shot_decode_fallback_count: u32,
    handoff_elapsed_ms: u128,
    render_elapsed_ms: u128,
    avg_decode_elapsed_ms: String,
    avg_decode_input_write_elapsed_ms: String,
    avg_decode_output_read_elapsed_ms: String,
    avg_decode_process_spawn_elapsed_ms: String,
    avg_handoff_elapsed_ms: String,
    avg_render_elapsed_ms: String,
    loop_total_elapsed_ms: u128,
    attempt_body_elapsed_ms: u128,
    loop_sleep_elapsed_ms: u128,
    frame_interval_wait_elapsed_ms: u128,
    event_pump_elapsed_ms: u128,
    window_update_elapsed_ms: u128,
    render_prepare_elapsed_ms: u128,
    render_buffer_cpu_scale_copy_elapsed_ms: u128,
    render_buffer_copy_elapsed_ms: u128,
    render_buffer_materialization_elapsed_ms: u128,
    render_buffer_scale_prepare_elapsed_ms: u128,
    render_buffer_scale_loop_elapsed_ms: u128,
    render_buffer_output_copy_elapsed_ms: u128,
    render_buffer_resize_elapsed_ms: u128,
    render_buffer_clear_elapsed_ms: u128,
    render_buffer_passthrough_count: u32,
    render_buffer_same_size_copy_count: u32,
    render_buffer_half_scale_count: u32,
    render_buffer_generic_scale_count: u32,
    render_buffer_reuse_count: u32,
    render_buffer_allocation_count: u32,
    render_buffer_bytes_copied_total: usize,
    render_backend_wait_elapsed_ms: u128,
    gdi_invalidate_elapsed_ms: u128,
    gdi_paint_wait_elapsed_ms: u128,
    gdi_wm_paint_elapsed_ms: u128,
    gdi_stretchdibits_elapsed_ms: u128,
    texture_upload_elapsed_ms: u128,
    window_present_elapsed_ms: u128,
    vsync_or_present_block_elapsed_ms: u128,
    quad_view_compose_elapsed_ms: u128,
    quad_view_compose_attempt_count: u32,
    quad_view_compose_success_count: u32,
    quad_view_compose_skipped_unchanged_count: u32,
    quad_view_composed_frame_reuse_count: u32,
    quad_view_visual_unchanged_count: u32,
    quad_view_visual_changed_count: u32,
    materialization_reason_first_render_count: u32,
    materialization_reason_visual_changed_count: u32,
    materialization_reason_previous_output_missing_count: u32,
    materialization_reason_profile_or_size_mismatch_count: u32,
    materialization_reason_force_render_count: u32,
    materialization_reason_unknown_count: u32,
    slot0_frame_id_changed_count: u32,
    slot1_frame_id_changed_count: u32,
    slot2_frame_id_changed_count: u32,
    slot3_frame_id_changed_count: u32,
    slot0_selected_source_changed_count: u32,
    slot1_selected_source_changed_count: u32,
    slot2_selected_source_changed_count: u32,
    slot3_selected_source_changed_count: u32,
    placeholder_visual_changed_count: u32,
    quad_view_incremental_update_count: u32,
    quad_view_full_compose_count: u32,
    quad_view_changed_slot_update_count: u32,
    quad_view_reused_slot_count: u32,
    quad_view_allocation_count: u32,
    avg_render_buffer_cpu_scale_copy_elapsed_ms: String,
    avg_render_buffer_materialization_elapsed_ms: String,
    avg_gdi_paint_wait_elapsed_ms: String,
    avg_gdi_wm_paint_elapsed_ms: String,
    avg_gdi_stretchdibits_elapsed_ms: String,
    avg_quad_view_incremental_update_elapsed_ms: String,
    avg_quad_view_compose_elapsed_ms: String,
    render_call_elapsed_ms: u128,
    render_input_unchanged_count: u32,
    render_reuse_frame_count: u32,
    unaccounted_elapsed_ms: u128,
    avg_attempt_elapsed_ms: String,
    max_attempt_elapsed_ms: u128,
    slow_attempt_count: u32,
    slow_attempt_threshold_ms: u128,
    scheduler_status: stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus,
    slot_bindings: [String; 4],
    slot_result_kinds: [String; 4],
    slot_diagnostics: [String; 4],
    clean_output_render_result_kind: &'static str,
    window_title: String,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwitcherFourViewFourRealHandoffPreviewLoopSummary {
    pipe_name: String,
    actual_pipe_path: String,
    preview_mode: &'static str,
    read_mode: &'static str,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    frames_attempted: u32,
    frames_rendered: u32,
    render_failures: u32,
    scheduler_status: stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus,
    slot_bindings: [String; 4],
    slot_result_kinds: [String; 4],
    slot_diagnostics: [String; 4],
    clean_output_render_result_kind: &'static str,
    window_title: String,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwitcherFourViewFocusedHandoffPreviewLoopSummary {
    pipe_name: String,
    actual_pipe_path: String,
    focused_slot_index: usize,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    focused_client_id: ClientId,
    focused_run_id: RunId,
    focused_result_kind: String,
    frames_attempted: u32,
    frames_rendered: u32,
    render_failures: u32,
    scheduler_status: stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus,
    slot_bindings: [String; 4],
    slot_result_kinds: [String; 4],
    slot_diagnostics: [String; 4],
    clean_output_render_result_kind: &'static str,
    window_title: String,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SwitcherFourViewControlledPreviewViewState {
    AllView,
    Focused(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SwitcherFourViewControlledPreviewCommand {
    All,
    Focus(usize),
    Status,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FourViewControlCommandSource {
    Stdin,
    Scripted(String),
    ControlPipe(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FourViewOperatorWrapperInputSource {
    Stdin,
    ScriptedKeys(String),
    RawKeys,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FourViewOperatorWrapperGuardState {
    quit_armed: bool,
    armed_until_millis: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FourViewOperatorWrapperKeySummary {
    key_index: usize,
    wrapper_key: String,
    mapped_command: String,
    guard_state: String,
    send_result: String,
    response_line: String,
    command_parse_error: String,
    wrapper_error: String,
    exit_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FourViewOperatorWrapperLoopSummary {
    control_pipe_name: String,
    input_source: String,
    key_summaries: Vec<FourViewOperatorWrapperKeySummary>,
    keys_processed: u32,
    commands_sent: u32,
    ignored_keys: u32,
    final_guard_state: String,
    raw_console_restore_result: String,
    raw_console_restore_error: String,
    exit_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwitcherFourViewControlledPreviewCommandSummary {
    command_index: usize,
    control_command_name: String,
    current_view_state: SwitcherFourViewControlledPreviewViewState,
    view_render_mode: String,
    output_layout: String,
    requested_transition: String,
    transition_result: String,
    selected_slot_result: String,
    rendered_slot_count: u32,
    focused_slot_index: Option<usize>,
    frames_rendered: u32,
    render_failures: u32,
    scheduler_status: stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus,
    clean_output_render_result_kind: String,
    all_view_render_result_kind: String,
    command_parse_error: String,
    exit_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwitcherFourViewControlledHandoffPreviewLoopSummary {
    pipe_name: String,
    actual_pipe_path: String,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    max_ticks_per_command: u32,
    command_source: String,
    command_summaries: Vec<SwitcherFourViewControlledPreviewCommandSummary>,
    final_view_state: SwitcherFourViewControlledPreviewViewState,
    commands_processed: u32,
    commands_rejected: u32,
    view_render_mode: String,
    output_layout: String,
    rendered_slot_count: u32,
    focused_slot_index: Option<usize>,
    frames_rendered: u32,
    render_failures: u32,
    scheduler_status: stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus,
    slot_bindings: [String; 4],
    slot_result_kinds: [String; 4],
    slot_diagnostics: [String; 4],
    clean_output_render_result_kind: String,
    all_view_render_result_kind: String,
    window_title: String,
    output_width: Option<u32>,
    output_height: Option<u32>,
    exit_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FourViewControlledPreviewRenderOutcome {
    view_render_mode: String,
    output_layout: String,
    selected_slot_result: String,
    rendered_slot_count: u32,
    focused_slot_index: Option<usize>,
    frames_rendered: u32,
    render_failures: u32,
    scheduler_status: stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus,
    slot_result_kinds: [String; 4],
    slot_diagnostics: [String; 4],
    clean_output_render_result_kind: String,
    all_view_render_result_kind: String,
    window_title: String,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FourViewControlledCommandIterationOutcome {
    summary: SwitcherFourViewControlledPreviewCommandSummary,
    render_outcome: Option<FourViewControlledPreviewRenderOutcome>,
}

const DEFAULT_CONTROL_PIPE_CONNECT_TIMEOUT_MILLIS: u32 = 5_000;
const DEFAULT_OPERATOR_WRAPPER_QUIT_GUARD_WINDOW_MILLIS: u64 = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FourViewPreviewSlotDiagnosticSummary {
    slot_index: usize,
    client_id: ClientId,
    run_id: RunId,
    request_id: Option<u64>,
    actual_pipe_path: Option<String>,
    handoff_response_kind: Option<&'static str>,
    parse_error: Option<String>,
    io_error: Option<String>,
    response_payload_len: Option<usize>,
    frame_id: Option<u64>,
    frame_payload_len: Option<usize>,
    frame_is_keyframe: Option<bool>,
    handoff_no_frame_reason: Option<String>,
    decodable_source: Option<String>,
    retained_keyframe_available: Option<bool>,
    retained_keyframe_frame_id: Option<u64>,
    decode_attempted: Option<bool>,
    decode_skipped_reason: Option<String>,
    decode_error: Option<String>,
    decode_input_payload_len: Option<usize>,
    decode_expected_width: Option<u32>,
    decode_expected_height: Option<u32>,
    decode_expected_pixel_format: Option<String>,
    decode_expected_rawvideo_len: Option<usize>,
    decoded_stdout_len: Option<usize>,
    ffmpeg_exit_status: Option<i32>,
    ffmpeg_stderr_summary: Option<String>,
    payload_has_sps: Option<bool>,
    payload_has_pps: Option<bool>,
    payload_has_idr: Option<bool>,
    payload_has_non_idr_vcl: Option<bool>,
    payload_nal_kinds: Option<String>,
    renderable_frame_available: Option<bool>,
    renderable_frame_missing_reason: Option<String>,
    selected_frame_available: Option<bool>,
    selected_frame_id: Option<u64>,
    selected_frame_source: Option<String>,
    target_selection_result: &'static str,
    render_input_kind: &'static str,
    final_slot_result_kind: &'static str,
}

fn unobserved_four_view_preview_slot_diagnostic(
    slot_index: usize,
    slot: &SwitcherFourViewTargetTimeSourceSlotConfig,
) -> FourViewPreviewSlotDiagnosticSummary {
    FourViewPreviewSlotDiagnosticSummary {
        slot_index,
        client_id: slot.client_id.clone(),
        run_id: slot.run_id.clone(),
        request_id: None,
        actual_pipe_path: None,
        handoff_response_kind: None,
        parse_error: None,
        io_error: None,
        response_payload_len: None,
        frame_id: None,
        frame_payload_len: None,
        frame_is_keyframe: None,
        handoff_no_frame_reason: None,
        decodable_source: None,
        retained_keyframe_available: None,
        retained_keyframe_frame_id: None,
        decode_attempted: None,
        decode_skipped_reason: None,
        decode_error: None,
        decode_input_payload_len: None,
        decode_expected_width: None,
        decode_expected_height: None,
        decode_expected_pixel_format: None,
        decode_expected_rawvideo_len: None,
        decoded_stdout_len: None,
        ffmpeg_exit_status: None,
        ffmpeg_stderr_summary: None,
        payload_has_sps: None,
        payload_has_pps: None,
        payload_has_idr: None,
        payload_has_non_idr_vcl: None,
        payload_nal_kinds: None,
        renderable_frame_available: None,
        renderable_frame_missing_reason: None,
        selected_frame_available: None,
        selected_frame_id: None,
        selected_frame_source: None,
        target_selection_result: "Unobserved",
        render_input_kind: "Unobserved",
        final_slot_result_kind: "NoFrameAvailable",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FourViewPreviewSlotHandoffObservation {
    slot_index: usize,
    client_id: ClientId,
    run_id: RunId,
    request_output: Option<SwitcherNamedPipeQueuedFrameHandoffRequestOutput>,
    result: SwitcherQueuedFrameHandoffResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewLoopRealHandoffCall {
    request_output: Option<SwitcherNamedPipeQueuedFrameHandoffRequestOutput>,
    result: SwitcherQueuedFrameHandoffResult,
}

trait SwitcherFrameCadenceSleepHook {
    fn sleep(&self, duration: Duration);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TwoRealPreviewLoopFpsDiagnostics {
    elapsed_ms: u128,
    target_fps: u32,
    configured_frame_interval_ms: u128,
    effective_attempt_fps: String,
    effective_render_fps: String,
    first_render_attempt_index: Option<u32>,
    first_render_elapsed_ms: Option<u128>,
    rendered_after_first_render: u32,
    effective_render_fps_after_first_render: String,
    no_render_before_first_render: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TwoRealPreviewLoopTickDiagnostics {
    selected_count: u32,
    no_frame_count: u32,
    handoff_error_count: u32,
    decode_attempt_count: u32,
    decode_success_count: u32,
    render_success_count: u32,
    render_failure_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TwoRealPreviewLoopRuntimeTiming {
    handoff_elapsed_ms: u128,
    handoff_call_count: u32,
    decode_elapsed_ms: u128,
    decode_attempt_count: u32,
    decode_success_count: u32,
    decode_process_spawn_elapsed_ms: u128,
    decode_input_write_elapsed_ms: u128,
    decode_input_payload_bytes_total: usize,
    decode_output_read_elapsed_ms: u128,
    decode_output_read_exact_elapsed_ms: u128,
    decode_output_vec_resize_elapsed_ms: u128,
    decode_process_wait_elapsed_ms: u128,
    decode_pixel_convert_elapsed_ms: u128,
    decode_buffer_allocation_count: u32,
    decode_output_bytes_total: usize,
    decode_stdout_expected_bytes_total: usize,
    decode_cached_frame_reuse_count: u32,
    decode_cache_miss_count: u32,
    decoded_buffer_clone_count: u32,
    decode_cache_hit_clone_count: u32,
    decode_cache_store_clone_count: u32,
    decoded_buffer_clone_elapsed_ms: u128,
    composed_buffer_clone_count: u32,
    decode_output_buffer_reuse_count: u32,
    persistent_decode_enabled: bool,
    persistent_decode_attempt_count: u32,
    persistent_decode_success_count: u32,
    persistent_decode_failure_count: u32,
    persistent_decode_fallback_count: u32,
    persistent_decode_process_spawn_count: u32,
    persistent_decode_process_restart_count: u32,
    persistent_decode_stdin_write_elapsed_ms: u128,
    persistent_decode_stdout_read_elapsed_ms: u128,
    persistent_decode_stdout_read_exact_elapsed_ms: u128,
    persistent_decode_output_bytes_total: usize,
    persistent_decode_last_error: Option<String>,
    one_shot_decode_fallback_count: u32,
    render_elapsed_ms: u128,
    render_call_count: u32,
    attempt_body_elapsed_ms: u128,
    loop_sleep_elapsed_ms: u128,
    frame_interval_wait_elapsed_ms: u128,
    event_pump_elapsed_ms: u128,
    window_update_elapsed_ms: u128,
    render_prepare_elapsed_ms: u128,
    render_buffer_copy_elapsed_ms: u128,
    render_buffer_scale_prepare_elapsed_ms: u128,
    render_buffer_scale_loop_elapsed_ms: u128,
    render_buffer_output_copy_elapsed_ms: u128,
    render_buffer_resize_elapsed_ms: u128,
    render_buffer_clear_elapsed_ms: u128,
    render_buffer_passthrough_count: u32,
    render_buffer_same_size_copy_count: u32,
    render_buffer_half_scale_count: u32,
    render_buffer_generic_scale_count: u32,
    render_buffer_reuse_count: u32,
    render_buffer_allocation_count: u32,
    render_buffer_bytes_copied_total: usize,
    render_backend_wait_elapsed_ms: u128,
    texture_upload_elapsed_ms: u128,
    window_present_elapsed_ms: u128,
    vsync_or_present_block_elapsed_ms: u128,
    quad_view_compose_elapsed_ms: u128,
    quad_view_incremental_update_elapsed_ms: u128,
    render_call_elapsed_ms: u128,
    max_attempt_elapsed_ms: u128,
    slow_attempt_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TwoRealPreviewLoopDecodedSlotIdentity {
    client_id: ClientId,
    run_id: RunId,
    frame_id: u64,
    decodable_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TwoRealPreviewLoopSlotVisualIdentity {
    SourceFrame {
        client_id: ClientId,
        run_id: RunId,
        frame_id: u64,
        selected_frame_source: Option<String>,
    },
    NoDisplayPlaceholder {
        reason: String,
    },
    SourceErrorPlaceholder {
        reason: String,
    },
    DecodeDeferredPlaceholder {
        client_id: ClientId,
        run_id: RunId,
        frame_id: u64,
        selected_frame_source: Option<String>,
        reason: String,
    },
    DecodeFailedPlaceholder {
        client_id: ClientId,
        run_id: RunId,
        frame_id: u64,
        selected_frame_source: Option<String>,
        failure: String,
    },
    MissingDecodedPixels {
        client_id: ClientId,
        run_id: RunId,
        frame_id: u64,
        selected_frame_source: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TwoRealPreviewLoopVisualChangeDiagnostics {
    frame_id_changed_counts: [u32; 4],
    selected_source_changed_counts: [u32; 4],
    placeholder_visual_changed_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TwoRealPreviewLoopMaterializationReason {
    FirstRender,
    VisualChanged,
    PreviousOutputMissing,
    #[allow(dead_code)]
    ProfileOrSizeMismatch,
    ForceRender,
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TwoRealPreviewLoopQuadCompositionCache {
    width: u32,
    height: u32,
    placeholder_bgra: [u8; 4],
    pixels: Vec<u8>,
    placeholder_row_width: u32,
    placeholder_row_bgra: [u8; 4],
    placeholder_row: Vec<u8>,
    allocation_count: u32,
}

impl TwoRealPreviewLoopQuadCompositionCache {
    fn prepare_canvas(
        &mut self,
        width: u32,
        height: u32,
        len: usize,
        placeholder_bgra: [u8; 4],
    ) -> bool {
        let needs_allocation = self.pixels.len() != len;
        if needs_allocation {
            self.pixels.resize(len, 0);
            self.allocation_count = self.allocation_count.saturating_add(1);
        }
        self.width = width;
        self.height = height;
        self.placeholder_bgra = placeholder_bgra;
        needs_allocation
    }

    fn prepare_placeholder_row(&mut self, width: u32, placeholder_bgra: [u8; 4]) {
        let len = width as usize * 4;
        if self.placeholder_row.len() != len
            || self.placeholder_row_width != width
            || self.placeholder_row_bgra != placeholder_bgra
        {
            self.placeholder_row.resize(len, 0);
            fill_two_real_bgra(&mut self.placeholder_row, placeholder_bgra);
            self.placeholder_row_width = width;
            self.placeholder_row_bgra = placeholder_bgra;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TwoRealPreviewLoopQuadCompositionUpdateDiagnostics {
    full_compose: bool,
    incremental_update: bool,
    changed_slot_count: u32,
    reused_slot_count: u32,
    allocation_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TwoRealPreviewLoopDecodeCacheKey {
    width: u32,
    height: u32,
    source_identity: Option<SwitcherH264DecodeSourceIdentity>,
    encoded_payload: Vec<u8>,
}

struct TimedSwitcherH264DecodeRuntime<'a, Runtime> {
    inner: &'a Runtime,
    timing: Rc<RefCell<TwoRealPreviewLoopRuntimeTiming>>,
    decoded_cache: RefCell<HashMap<TwoRealPreviewLoopDecodeCacheKey, SwitcherDecodedFrame>>,
}

impl<'a, Runtime> TimedSwitcherH264DecodeRuntime<'a, Runtime> {
    fn new(inner: &'a Runtime, timing: Rc<RefCell<TwoRealPreviewLoopRuntimeTiming>>) -> Self {
        Self {
            inner,
            timing,
            decoded_cache: RefCell::new(HashMap::new()),
        }
    }
}

impl<Runtime> SwitcherH264DecodeRuntimeHook for TimedSwitcherH264DecodeRuntime<'_, Runtime>
where
    Runtime: SwitcherH264DecodeRuntimeHook,
{
    fn decode_annex_b_h264(&self, input: SwitcherH264DecodeInput) -> SwitcherH264DecodeResult {
        let source_identity = input.source_identity.clone();
        let key = TwoRealPreviewLoopDecodeCacheKey {
            width: input.width,
            height: input.height,
            encoded_payload: if source_identity.is_some() {
                Vec::new()
            } else {
                input.encoded_payload.clone()
            },
            source_identity,
        };
        if let Some(decoded) = self.decoded_cache.borrow().get(&key) {
            let clone_start = Instant::now();
            let decoded = decoded.clone();
            let mut timing = self.timing.borrow_mut();
            timing.decode_cached_frame_reuse_count += 1;
            timing.decode_cache_hit_clone_count += 1;
            timing.decoded_buffer_clone_count += 1;
            timing.decoded_buffer_clone_elapsed_ms += clone_start.elapsed().as_millis();
            return SwitcherH264DecodeResult::Decoded(decoded);
        }

        self.timing.borrow_mut().decode_cache_miss_count += 1;
        let start = Instant::now();
        let output = self.inner.decode_annex_b_h264_with_diagnostics(input);
        let result = output.result;
        let mut timing = self.timing.borrow_mut();
        timing.decode_elapsed_ms += start.elapsed().as_millis();
        add_decode_runtime_diagnostics(&mut timing, output.diagnostics);
        timing.decode_attempt_count += 1;
        if let SwitcherH264DecodeResult::Decoded(decoded) = &result {
            timing.decode_success_count += 1;
            let clone_start = Instant::now();
            let decoded_clone = decoded.clone();
            timing.decoded_buffer_clone_elapsed_ms += clone_start.elapsed().as_millis();
            timing.decode_cache_store_clone_count += 1;
            timing.decoded_buffer_clone_count += 1;
            self.decoded_cache.borrow_mut().insert(key, decoded_clone);
        }
        result
    }
}

fn add_decode_runtime_diagnostics(
    timing: &mut TwoRealPreviewLoopRuntimeTiming,
    diagnostics: SwitcherH264DecodeRuntimeDiagnostics,
) {
    timing.decode_process_spawn_elapsed_ms += diagnostics.process_spawn_elapsed_ms;
    timing.decode_input_write_elapsed_ms += diagnostics.input_write_elapsed_ms;
    timing.decode_input_payload_bytes_total = timing
        .decode_input_payload_bytes_total
        .saturating_add(diagnostics.input_payload_bytes);
    timing.decode_output_read_elapsed_ms += diagnostics.output_read_elapsed_ms;
    timing.decode_output_read_exact_elapsed_ms += diagnostics.output_read_exact_elapsed_ms;
    timing.decode_output_vec_resize_elapsed_ms += diagnostics.output_vec_resize_elapsed_ms;
    timing.decode_process_wait_elapsed_ms += diagnostics.process_wait_elapsed_ms;
    timing.decode_pixel_convert_elapsed_ms += diagnostics.pixel_convert_elapsed_ms;
    timing.decode_buffer_allocation_count = timing
        .decode_buffer_allocation_count
        .saturating_add(diagnostics.buffer_allocation_count);
    timing.decode_output_bytes_total = timing
        .decode_output_bytes_total
        .saturating_add(diagnostics.output_bytes);
    timing.decode_stdout_expected_bytes_total = timing
        .decode_stdout_expected_bytes_total
        .saturating_add(diagnostics.output_expected_bytes);
    timing.decode_output_buffer_reuse_count = timing
        .decode_output_buffer_reuse_count
        .saturating_add(diagnostics.output_buffer_reuse_count);
    timing.persistent_decode_enabled |= diagnostics.persistent_decode_enabled;
    timing.persistent_decode_attempt_count = timing
        .persistent_decode_attempt_count
        .saturating_add(diagnostics.persistent_decode_attempt_count);
    timing.persistent_decode_success_count = timing
        .persistent_decode_success_count
        .saturating_add(diagnostics.persistent_decode_success_count);
    timing.persistent_decode_failure_count = timing
        .persistent_decode_failure_count
        .saturating_add(diagnostics.persistent_decode_failure_count);
    timing.persistent_decode_fallback_count = timing
        .persistent_decode_fallback_count
        .saturating_add(diagnostics.persistent_decode_fallback_count);
    timing.persistent_decode_process_spawn_count = timing
        .persistent_decode_process_spawn_count
        .saturating_add(diagnostics.persistent_decode_process_spawn_count);
    timing.persistent_decode_process_restart_count = timing
        .persistent_decode_process_restart_count
        .saturating_add(diagnostics.persistent_decode_process_restart_count);
    timing.persistent_decode_stdin_write_elapsed_ms = timing
        .persistent_decode_stdin_write_elapsed_ms
        .saturating_add(diagnostics.persistent_decode_stdin_write_elapsed_ms);
    timing.persistent_decode_stdout_read_elapsed_ms = timing
        .persistent_decode_stdout_read_elapsed_ms
        .saturating_add(diagnostics.persistent_decode_stdout_read_elapsed_ms);
    timing.persistent_decode_stdout_read_exact_elapsed_ms = timing
        .persistent_decode_stdout_read_exact_elapsed_ms
        .saturating_add(diagnostics.persistent_decode_stdout_read_exact_elapsed_ms);
    timing.persistent_decode_output_bytes_total = timing
        .persistent_decode_output_bytes_total
        .saturating_add(diagnostics.persistent_decode_output_bytes);
    if let Some(last_error) = diagnostics.persistent_decode_last_error {
        timing.persistent_decode_last_error = Some(last_error);
    }
    timing.one_shot_decode_fallback_count = timing
        .one_shot_decode_fallback_count
        .saturating_add(diagnostics.one_shot_decode_fallback_count);
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
    event_pump_elapsed_ms: u128,
    window_update_elapsed_ms: u128,
    gdi_invalidate_elapsed_ms: u128,
    gdi_paint_wait_elapsed_ms: u128,
    gdi_wm_paint_elapsed_ms: u128,
    gdi_stretchdibits_elapsed_ms: u128,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct BgraRenderBufferDiagnostics {
    allocation_count: u32,
    reuse_count: u32,
    bytes_copied_total: usize,
    scale_prepare_elapsed_ms: u128,
    scale_loop_elapsed_ms: u128,
    output_copy_elapsed_ms: u128,
    resize_elapsed_ms: u128,
    clear_elapsed_ms: u128,
    passthrough_count: u32,
    same_size_copy_count: u32,
    half_scale_count: u32,
    generic_scale_count: u32,
}

fn take_reusable_obs_render_buffer(expected_len: usize) -> (Vec<u8>, BgraRenderBufferDiagnostics) {
    REUSABLE_OBS_RENDER_BUFFER.with(|buffer_slot| {
        let prepare_start = Instant::now();
        let mut buffer_slot = buffer_slot.borrow_mut();
        let mut diagnostics = BgraRenderBufferDiagnostics::default();
        if let Some(mut pixels) = buffer_slot.take() {
            let reusable_capacity = pixels.capacity() >= expected_len;
            let resize_start = Instant::now();
            pixels.resize(expected_len, 0);
            diagnostics.resize_elapsed_ms = resize_start.elapsed().as_millis();
            if reusable_capacity {
                diagnostics.reuse_count = 1;
            } else {
                diagnostics.allocation_count = 1;
            }
            diagnostics.scale_prepare_elapsed_ms = prepare_start
                .elapsed()
                .as_millis()
                .saturating_sub(diagnostics.resize_elapsed_ms);
            return (pixels, diagnostics);
        }

        let resize_start = Instant::now();
        let pixels = vec![0u8; expected_len];
        let resize_elapsed_ms = resize_start.elapsed().as_millis();
        (
            pixels,
            BgraRenderBufferDiagnostics {
                allocation_count: 1,
                scale_prepare_elapsed_ms: prepare_start
                    .elapsed()
                    .as_millis()
                    .saturating_sub(resize_elapsed_ms),
                resize_elapsed_ms,
                ..BgraRenderBufferDiagnostics::default()
            },
        )
    })
}

fn recycle_obs_render_buffer(pixels: Vec<u8>) {
    if pixels.is_empty() {
        return;
    }

    REUSABLE_OBS_RENDER_BUFFER.with(|buffer_slot| {
        *buffer_slot.borrow_mut() = Some(pixels);
    });
}

struct ObsFriendlyFourViewLoopWindowRenderRuntime<'a, Runtime> {
    inner: &'a Runtime,
    metadata: Mutex<ObsFriendlyFourViewLoopRenderMetadataSnapshot>,
    timing: Option<Rc<RefCell<TwoRealPreviewLoopRuntimeTiming>>>,
}

impl<'a, Runtime> ObsFriendlyFourViewLoopWindowRenderRuntime<'a, Runtime> {
    fn new(inner: &'a Runtime) -> Self {
        Self {
            inner,
            metadata: Mutex::new(ObsFriendlyFourViewLoopRenderMetadataSnapshot::default()),
            timing: None,
        }
    }

    fn with_timing(
        inner: &'a Runtime,
        timing: Rc<RefCell<TwoRealPreviewLoopRuntimeTiming>>,
    ) -> Self {
        Self {
            inner,
            metadata: Mutex::new(ObsFriendlyFourViewLoopRenderMetadataSnapshot::default()),
            timing: Some(timing),
        }
    }

    fn metadata_snapshot(&self) -> ObsFriendlyFourViewLoopRenderMetadataSnapshot {
        *self
            .metadata
            .lock()
            .expect("obs-friendly loop metadata mutex should not be poisoned")
    }

    fn render_bgra_once(
        &self,
        width: u32,
        height: u32,
        pixel_format: SwitcherDecodedFramePixelFormat,
        pixels: &[u8],
        title: String,
        hold_millis: u64,
    ) -> SwitcherWindowRenderResult
    where
        Runtime: SwitcherWindowRenderRuntimeHook,
    {
        let scale_start = Instant::now();
        let (scaled_frame, diagnostics) = scale_four_view_bgra_to_obs_validation_profile_from_slice(
            width,
            height,
            pixel_format,
            pixels,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
        );
        self.record_render_buffer_diagnostics(scale_start.elapsed().as_millis(), diagnostics);
        self.render_scaled_frame(width, height, scaled_frame, title, hold_millis)
    }

    fn record_render_buffer_diagnostics(
        &self,
        render_buffer_copy_elapsed_ms: u128,
        diagnostics: BgraRenderBufferDiagnostics,
    ) {
        if let Some(timing) = &self.timing {
            let mut timing = timing.borrow_mut();
            timing.render_buffer_copy_elapsed_ms += render_buffer_copy_elapsed_ms;
            timing.render_buffer_scale_prepare_elapsed_ms += diagnostics.scale_prepare_elapsed_ms;
            timing.render_buffer_scale_loop_elapsed_ms += diagnostics.scale_loop_elapsed_ms;
            timing.render_buffer_output_copy_elapsed_ms += diagnostics.output_copy_elapsed_ms;
            timing.render_buffer_resize_elapsed_ms += diagnostics.resize_elapsed_ms;
            timing.render_buffer_clear_elapsed_ms += diagnostics.clear_elapsed_ms;
            timing.render_buffer_passthrough_count = timing
                .render_buffer_passthrough_count
                .saturating_add(diagnostics.passthrough_count);
            timing.render_buffer_same_size_copy_count = timing
                .render_buffer_same_size_copy_count
                .saturating_add(diagnostics.same_size_copy_count);
            timing.render_buffer_half_scale_count = timing
                .render_buffer_half_scale_count
                .saturating_add(diagnostics.half_scale_count);
            timing.render_buffer_generic_scale_count = timing
                .render_buffer_generic_scale_count
                .saturating_add(diagnostics.generic_scale_count);
            timing.render_buffer_reuse_count = timing
                .render_buffer_reuse_count
                .saturating_add(diagnostics.reuse_count);
            timing.render_buffer_allocation_count = timing
                .render_buffer_allocation_count
                .saturating_add(diagnostics.allocation_count);
            timing.render_buffer_bytes_copied_total = timing
                .render_buffer_bytes_copied_total
                .saturating_add(diagnostics.bytes_copied_total);
        }
    }

    fn render_scaled_frame(
        &self,
        source_width: u32,
        source_height: u32,
        scaled_frame: stream_sync_switcher::SwitcherDecodedFrameRenderInput,
        title: String,
        hold_millis: u64,
    ) -> SwitcherWindowRenderResult
    where
        Runtime: SwitcherWindowRenderRuntimeHook,
    {
        let output_width = scaled_frame.width;
        let output_height = scaled_frame.height;
        let bgra_payload_len = scaled_frame.pixels.len();
        let window_update_start = Instant::now();
        let result = self
            .inner
            .render_once(stream_sync_switcher::SwitcherWindowRenderRequest {
                frame: scaled_frame,
                title,
                hold_millis,
            });
        if let Some(timing) = &self.timing {
            let window_update_elapsed_ms = window_update_start.elapsed().as_millis();
            let mut timing = timing.borrow_mut();
            timing.window_update_elapsed_ms += window_update_elapsed_ms;
            timing.render_backend_wait_elapsed_ms += window_update_elapsed_ms;
        }

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
        let scale_start = Instant::now();
        let (scaled_frame, diagnostics) = if request.frame.width
            == FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
            && request.frame.height == FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
        {
            (
                request.frame,
                BgraRenderBufferDiagnostics {
                    passthrough_count: 1,
                    reuse_count: 1,
                    ..BgraRenderBufferDiagnostics::default()
                },
            )
        } else {
            scale_four_view_bgra_render_input_to_obs_validation_profile(
                &request.frame,
                FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
                FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
            )
        };
        self.record_render_buffer_diagnostics(scale_start.elapsed().as_millis(), diagnostics);
        self.render_scaled_frame(
            source_width,
            source_height,
            scaled_frame,
            request.title,
            request.hold_millis,
        )
    }
}

trait SwitcherPersistentWindowLoopRuntimeHook: SwitcherWindowRenderRuntimeHook {
    fn close_persistent_window(&self);
    fn lifecycle_snapshot(&self) -> PersistentWindowLifecycleSnapshot;
    fn pump_persistent_window_events(&self) {}
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

struct MultiRealAndPlaceholderQueuedFrameHandoff<RealHandoff> {
    slots: [SwitcherFourViewTargetTimeSourceSlotConfig; 4],
    real_slots: Vec<(ClientId, RunId)>,
    real_handoff: RealHandoff,
    last_frame_slot_observations: [Option<FourViewPreviewSlotHandoffObservation>; 4],
    timing: Option<Rc<RefCell<TwoRealPreviewLoopRuntimeTiming>>>,
}

impl<RealHandoff> MultiRealAndPlaceholderQueuedFrameHandoff<RealHandoff> {
    fn new(
        slots: [SwitcherFourViewTargetTimeSourceSlotConfig; 4],
        real_slots: Vec<(ClientId, RunId)>,
        real_handoff: RealHandoff,
    ) -> Self {
        Self {
            slots,
            real_slots,
            real_handoff,
            last_frame_slot_observations: std::array::from_fn(|_| None),
            timing: None,
        }
    }

    fn with_timing(
        slots: [SwitcherFourViewTargetTimeSourceSlotConfig; 4],
        real_slots: Vec<(ClientId, RunId)>,
        real_handoff: RealHandoff,
        timing: Rc<RefCell<TwoRealPreviewLoopRuntimeTiming>>,
    ) -> Self {
        Self {
            slots,
            real_slots,
            real_handoff,
            last_frame_slot_observations: std::array::from_fn(|_| None),
            timing: Some(timing),
        }
    }

    fn begin_frame(&mut self) {
        self.last_frame_slot_observations = std::array::from_fn(|_| None);
    }

    fn take_last_frame_slot_diagnostics(
        &self,
        validation: &SwitcherFourViewHandoffValidationOutput,
    ) -> [String; 4] {
        self.take_last_frame_slot_diagnostics_with_decode_reuse(validation, [false; 4])
    }

    fn take_last_frame_slot_diagnostics_with_decode_reuse(
        &self,
        validation: &SwitcherFourViewHandoffValidationOutput,
        unchanged_frame_reuse_slots: [bool; 4],
    ) -> [String; 4] {
        self.take_last_frame_slot_diagnostics_from_pre_composition(
            &SwitcherFourViewHandoffValidationPreCompositionOutput {
                scheduler: validation.scheduler.clone(),
                decode_render_adapter: validation.decode_render_adapter.clone(),
                display: validation.display.clone(),
                composition_instruction: validation.composition_instruction.clone(),
                composition_render: validation.composition_render.clone(),
            },
            unchanged_frame_reuse_slots,
        )
    }

    fn take_last_frame_slot_diagnostics_from_pre_composition(
        &self,
        pre_composition: &SwitcherFourViewHandoffValidationPreCompositionOutput,
        unchanged_frame_reuse_slots: [bool; 4],
    ) -> [String; 4] {
        std::array::from_fn(|slot_index| {
            let slot = &self.slots[slot_index];
            let observation = self.last_frame_slot_observations[slot_index].as_ref();
            let mut diagnostic = build_four_view_preview_slot_diagnostic(
                slot_index,
                slot,
                observation,
                &pre_composition.scheduler.slots[slot_index].result,
                &pre_composition.composition_render.composition.slots[slot_index],
            );
            if unchanged_frame_reuse_slots[slot_index] {
                diagnostic.decode_attempted = Some(false);
                diagnostic.decode_skipped_reason = Some("UnchangedFrameReuse".to_string());
            }
            format_four_view_preview_slot_diagnostic(&diagnostic)
        })
    }
}

trait PreviewLoopRealHandoff {
    fn read_handoff_frame_with_observation(
        &mut self,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> PreviewLoopRealHandoffCall;
}

impl<T> PreviewLoopRealHandoff for T
where
    T: SwitcherQueuedFrameHandoff,
{
    fn read_handoff_frame_with_observation(
        &mut self,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> PreviewLoopRealHandoffCall {
        PreviewLoopRealHandoffCall {
            request_output: None,
            result: SwitcherQueuedFrameHandoff::read_handoff_frame(self, input),
        }
    }
}

struct ObservedNamedPipePreviewHandoff<R, C> {
    inner: SwitcherNamedPipeQueuedFrameHandoff<R, C>,
}

impl<R, C> ObservedNamedPipePreviewHandoff<R, C> {
    fn new(inner: SwitcherNamedPipeQueuedFrameHandoff<R, C>) -> Self {
        Self { inner }
    }
}

impl<R, C> PreviewLoopRealHandoff for ObservedNamedPipePreviewHandoff<R, C>
where
    R: SwitcherNamedPipeQueuedFrameHandoffRuntime,
    C: stream_sync_switcher::SwitcherNamedPipeQueuedFrameHandoffClock,
{
    fn read_handoff_frame_with_observation(
        &mut self,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> PreviewLoopRealHandoffCall {
        let output = self.inner.read_handoff_frame_with_config(
            input,
            SwitcherNamedPipeQueuedFrameHandoffRequestConfig::default(),
        );
        PreviewLoopRealHandoffCall {
            request_output: Some(output.clone()),
            result: output.result,
        }
    }
}

impl<RealHandoff> SwitcherQueuedFrameHandoff
    for MultiRealAndPlaceholderQueuedFrameHandoff<RealHandoff>
where
    RealHandoff: PreviewLoopRealHandoff,
{
    fn read_handoff_frame(
        &mut self,
        input: SwitcherQueuedFrameHandoffInput,
    ) -> SwitcherQueuedFrameHandoffResult {
        let slot_index = self
            .slots
            .iter()
            .position(|slot| slot.client_id == input.client_id && slot.run_id == input.run_id)
            .unwrap_or(0);
        if self
            .real_slots
            .iter()
            .any(|(client_id, run_id)| input.client_id == *client_id && input.run_id == *run_id)
        {
            let handoff_start = Instant::now();
            let call = self
                .real_handoff
                .read_handoff_frame_with_observation(input.clone());
            if let Some(timing) = &self.timing {
                let mut timing = timing.borrow_mut();
                timing.handoff_elapsed_ms += handoff_start.elapsed().as_millis();
                timing.handoff_call_count += 1;
            }
            self.last_frame_slot_observations[slot_index] =
                Some(FourViewPreviewSlotHandoffObservation {
                    slot_index,
                    client_id: input.client_id,
                    run_id: input.run_id,
                    request_output: call.request_output,
                    result: call.result.clone(),
                });
            return call.result;
        }

        let client_id = input.client_id.clone();
        let run_id = input.run_id.clone();
        let result = SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
            client_id: input.client_id,
            run_id: input.run_id,
            mode: input.mode,
            client_queue_len: 0,
        };
        self.last_frame_slot_observations[slot_index] =
            Some(FourViewPreviewSlotHandoffObservation {
                slot_index,
                client_id,
                run_id,
                request_output: None,
                result: result.clone(),
            });
        result
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

    fn pump_persistent_window_events(&self) {
        windows_persistent_render_pump_events(&self.state);
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

#[cfg(windows)]
fn run_four_view_real_handoff_preview_loop(
    pipe_name: &str,
    real_slot_index: usize,
    client_id: ClientId,
    run_id: RunId,
    frames: NonZeroU32,
) -> Result<SwitcherFourViewRealHandoffPreviewLoopSummary, String> {
    let handoff = ObservedNamedPipePreviewHandoff::new(SwitcherNamedPipeQueuedFrameHandoff::new(
        pipe_name,
        DEFAULT_ONE_SHOT_REQUEST_ID,
    ));
    let render_runtime = SwitcherWindowsGdiPersistentWindowRenderRuntime::default();
    Ok(
        run_four_view_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            pipe_name,
            real_slot_index,
            client_id,
            run_id,
            frames,
            real_four_view_preview_target_timestamp(),
            handoff,
            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
            &render_runtime,
            &RealSwitcherFrameCadenceSleepHook,
        ),
    )
}

#[cfg(not(windows))]
fn run_four_view_real_handoff_preview_loop(
    _pipe_name: &str,
    _real_slot_index: usize,
    _client_id: ClientId,
    _run_id: RunId,
    _frames: NonZeroU32,
) -> Result<SwitcherFourViewRealHandoffPreviewLoopSummary, String> {
    Err("four-view real handoff preview loop is only available on Windows".to_string())
}

#[cfg(windows)]
fn run_four_view_two_real_handoff_preview_loop(
    pipe_name: &str,
    slot0_index: usize,
    client0_id: ClientId,
    run0_id: RunId,
    slot1_index: usize,
    client1_id: ClientId,
    run1_id: RunId,
    frames: NonZeroU32,
    read_mode: SwitcherSingleClientQueueSourceMode,
) -> Result<SwitcherFourViewTwoRealHandoffPreviewLoopSummary, String> {
    let handoff = ObservedNamedPipePreviewHandoff::new(SwitcherNamedPipeQueuedFrameHandoff::new(
        pipe_name,
        DEFAULT_ONE_SHOT_REQUEST_ID,
    ));
    let render_runtime = SwitcherWindowsGdiPersistentWindowRenderRuntime::default();
    let persistent_decode_runtime = SwitcherPersistentFfmpegH264DecodeRuntimeHook::default();
    Ok(run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep(
        pipe_name,
        slot0_index,
        client0_id,
        run0_id,
        slot1_index,
        client1_id,
        run1_id,
        frames,
        read_mode,
        real_four_view_preview_target_timestamp,
        handoff,
        &persistent_decode_runtime,
        &render_runtime,
        &RealSwitcherFrameCadenceSleepHook,
    ))
}

#[cfg(not(windows))]
fn run_four_view_two_real_handoff_preview_loop(
    _pipe_name: &str,
    _slot0_index: usize,
    _client0_id: ClientId,
    _run0_id: RunId,
    _slot1_index: usize,
    _client1_id: ClientId,
    _run1_id: RunId,
    _frames: NonZeroU32,
    _read_mode: SwitcherSingleClientQueueSourceMode,
) -> Result<SwitcherFourViewTwoRealHandoffPreviewLoopSummary, String> {
    Err("four-view two-real handoff preview loop is only available on Windows".to_string())
}

#[cfg(windows)]
fn run_four_view_four_real_handoff_preview_loop(
    pipe_name: &str,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    frames: NonZeroU32,
    read_mode: SwitcherSingleClientQueueSourceMode,
) -> Result<SwitcherFourViewFourRealHandoffPreviewLoopSummary, String> {
    let handoff = ObservedNamedPipePreviewHandoff::new(SwitcherNamedPipeQueuedFrameHandoff::new(
        pipe_name,
        DEFAULT_ONE_SHOT_REQUEST_ID,
    ));
    let render_runtime = SwitcherWindowsGdiPersistentWindowRenderRuntime::default();
    Ok(run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep(
        pipe_name,
        client0_id,
        run0_id,
        client1_id,
        run1_id,
        client2_id,
        run2_id,
        client3_id,
        run3_id,
        frames,
        read_mode,
        real_four_view_preview_target_timestamp,
        handoff,
        &SwitcherFfmpegH264DecodeRuntimeHook::default(),
        &render_runtime,
        &RealSwitcherFrameCadenceSleepHook,
    ))
}

#[cfg(not(windows))]
fn run_four_view_four_real_handoff_preview_loop(
    _pipe_name: &str,
    _client0_id: ClientId,
    _run0_id: RunId,
    _client1_id: ClientId,
    _run1_id: RunId,
    _client2_id: ClientId,
    _run2_id: RunId,
    _client3_id: ClientId,
    _run3_id: RunId,
    _frames: NonZeroU32,
    _read_mode: SwitcherSingleClientQueueSourceMode,
) -> Result<SwitcherFourViewFourRealHandoffPreviewLoopSummary, String> {
    Err("four-view four-real handoff preview loop is only available on Windows".to_string())
}

#[cfg(windows)]
fn run_four_view_focused_handoff_preview_loop(
    pipe_name: &str,
    focused_slot_index: usize,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    frames: NonZeroU32,
) -> Result<SwitcherFourViewFocusedHandoffPreviewLoopSummary, String> {
    let handoff = ObservedNamedPipePreviewHandoff::new(SwitcherNamedPipeQueuedFrameHandoff::new(
        pipe_name,
        DEFAULT_ONE_SHOT_REQUEST_ID,
    ));
    let render_runtime = SwitcherWindowsGdiPersistentWindowRenderRuntime::default();
    Ok(
        run_four_view_focused_handoff_preview_loop_with_handoff_runtime_and_sleep(
            pipe_name,
            focused_slot_index,
            client0_id,
            run0_id,
            client1_id,
            run1_id,
            client2_id,
            run2_id,
            client3_id,
            run3_id,
            frames,
            real_four_view_preview_target_timestamp(),
            handoff,
            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
            &render_runtime,
            &RealSwitcherFrameCadenceSleepHook,
        ),
    )
}

#[cfg(not(windows))]
fn run_four_view_focused_handoff_preview_loop(
    _pipe_name: &str,
    _focused_slot_index: usize,
    _client0_id: ClientId,
    _run0_id: RunId,
    _client1_id: ClientId,
    _run1_id: RunId,
    _client2_id: ClientId,
    _run2_id: RunId,
    _client3_id: ClientId,
    _run3_id: RunId,
    _frames: NonZeroU32,
) -> Result<SwitcherFourViewFocusedHandoffPreviewLoopSummary, String> {
    Err("four-view focused handoff preview loop is only available on Windows".to_string())
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

fn run_four_view_real_handoff_preview_loop_with_handoff_runtime_and_sleep<
    RealHandoff,
    DecodeRuntime,
    RenderRuntime,
>(
    pipe_name: &str,
    real_slot_index: usize,
    client_id: ClientId,
    run_id: RunId,
    frames: NonZeroU32,
    target_timestamp: TimestampMicros,
    real_handoff: RealHandoff,
    decode_runtime: &DecodeRuntime,
    render_runtime: &RenderRuntime,
    cadence_sleep: &impl SwitcherFrameCadenceSleepHook,
) -> SwitcherFourViewRealHandoffPreviewLoopSummary
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook + SwitcherPersistentWindowLoopRuntimeHook,
{
    let actual_pipe_path = format_actual_handoff_pipe_path(pipe_name);
    let slots = default_four_view_real_handoff_preview_slots(
        real_slot_index,
        client_id.clone(),
        run_id.clone(),
    );
    let slot_bindings = slots
        .clone()
        .map(|slot| format_four_view_slot_binding(&slot));
    let mut handoff = MultiRealAndPlaceholderQueuedFrameHandoff::new(
        slots.clone(),
        vec![(client_id.clone(), run_id.clone())],
        real_handoff,
    );
    let mut frames_attempted = 0u32;
    let mut frames_rendered = 0u32;
    let mut render_failures = 0u32;
    let mut scheduler_status =
        stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus::NoFrames;
    let mut slot_result_kinds = [
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
    ];
    let mut slot_diagnostics = std::array::from_fn(|slot_index| {
        format_four_view_preview_slot_diagnostic(&unobserved_four_view_preview_slot_diagnostic(
            slot_index,
            &slots[slot_index],
        ))
    });
    let mut clean_output_render_result_kind = "NoRenderableQuadView";
    let mut window_title = SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE.to_string();
    let cadence = four_view_clean_output_window_loop_frame_cadence();
    let obs_runtime = ObsFriendlyFourViewLoopWindowRenderRuntime::new(render_runtime);

    for frame_index in 0..frames.get() {
        handoff.begin_frame();
        let validation = run_four_view_real_handoff_preview_validation_with_runtime_and_handoff(
            &mut handoff,
            slots.clone(),
            target_timestamp,
            stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
            decode_runtime,
        );
        let clean_output = SwitcherFourViewCleanOutputWindowBoundary::default()
            .render_with_runtime(
                SwitcherFourViewCleanOutputWindowInput {
                    render_facing_output: validation.render_facing.clone(),
                },
                &obs_runtime,
            );

        frames_attempted += 1;
        scheduler_status = validation.scheduler.status;
        slot_result_kinds = validation.scheduler.slots.clone().map(|slot| {
            format_four_view_real_handoff_scheduler_slot_kind(&slot.result).to_string()
        });
        slot_diagnostics = handoff.take_last_frame_slot_diagnostics(&validation);
        clean_output_render_result_kind =
            format_four_view_clean_output_window_result_kind(&clean_output.output_window);
        window_title = clean_output.window_identity.title.clone();

        if let SwitcherFourViewCleanOutputWindowRenderResult::RenderReady { render, .. } =
            &clean_output.output_window
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
    let render_metadata = obs_runtime.metadata_snapshot();

    SwitcherFourViewRealHandoffPreviewLoopSummary {
        real_slot_index,
        pipe_name: pipe_name.to_string(),
        actual_pipe_path,
        client_id,
        run_id,
        frames_attempted,
        frames_rendered,
        render_failures,
        scheduler_status,
        slot_bindings,
        slot_result_kinds,
        slot_diagnostics,
        clean_output_render_result_kind,
        window_title,
        output_width: render_metadata.output_width,
        output_height: render_metadata.output_height,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep<
    RealHandoff,
    DecodeRuntime,
    RenderRuntime,
>(
    pipe_name: &str,
    slot0_index: usize,
    client0_id: ClientId,
    run0_id: RunId,
    slot1_index: usize,
    client1_id: ClientId,
    run1_id: RunId,
    frames: NonZeroU32,
    read_mode: SwitcherSingleClientQueueSourceMode,
    target_timestamp: TimestampMicros,
    real_handoff: RealHandoff,
    decode_runtime: &DecodeRuntime,
    render_runtime: &RenderRuntime,
    cadence_sleep: &impl SwitcherFrameCadenceSleepHook,
) -> SwitcherFourViewTwoRealHandoffPreviewLoopSummary
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook + SwitcherPersistentWindowLoopRuntimeHook,
{
    run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep(
        pipe_name,
        slot0_index,
        client0_id,
        run0_id,
        slot1_index,
        client1_id,
        run1_id,
        frames,
        read_mode,
        move || target_timestamp,
        real_handoff,
        decode_runtime,
        render_runtime,
        cadence_sleep,
    )
}

fn run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep<
    RealHandoff,
    DecodeRuntime,
    RenderRuntime,
    TargetTimestampHook,
>(
    pipe_name: &str,
    slot0_index: usize,
    client0_id: ClientId,
    run0_id: RunId,
    slot1_index: usize,
    client1_id: ClientId,
    run1_id: RunId,
    frames: NonZeroU32,
    read_mode: SwitcherSingleClientQueueSourceMode,
    mut target_timestamp_hook: TargetTimestampHook,
    real_handoff: RealHandoff,
    decode_runtime: &DecodeRuntime,
    render_runtime: &RenderRuntime,
    cadence_sleep: &impl SwitcherFrameCadenceSleepHook,
) -> SwitcherFourViewTwoRealHandoffPreviewLoopSummary
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook + SwitcherPersistentWindowLoopRuntimeHook,
    TargetTimestampHook: FnMut() -> TimestampMicros,
{
    let actual_pipe_path = format_actual_handoff_pipe_path(pipe_name);
    let slots = default_four_view_two_real_handoff_preview_slots(
        slot0_index,
        client0_id.clone(),
        run0_id.clone(),
        slot1_index,
        client1_id.clone(),
        run1_id.clone(),
    );
    let slot_bindings = slots
        .clone()
        .map(|slot| format_four_view_slot_binding(&slot));
    let timing = Rc::new(RefCell::new(TwoRealPreviewLoopRuntimeTiming::default()));
    let mut handoff = MultiRealAndPlaceholderQueuedFrameHandoff::with_timing(
        slots.clone(),
        vec![
            (client0_id.clone(), run0_id.clone()),
            (client1_id.clone(), run1_id.clone()),
        ],
        real_handoff,
        Rc::clone(&timing),
    );
    let mut frames_attempted = 0u32;
    let mut frames_rendered = 0u32;
    let mut render_failures = 0u32;
    let mut selected_count = 0u32;
    let mut no_frame_count = 0u32;
    let mut handoff_error_count = 0u32;
    let mut render_success_count = 0u32;
    let mut render_failure_count = 0u32;
    let mut first_render_attempt_index = None;
    let mut first_render_elapsed_ms = None;
    let mut unchanged_frame_reuse_count = 0u32;
    let mut skipped_decode_unchanged_frame_count = 0u32;
    let redecoded_same_frame_count = 0u32;
    let mut scheduler_status =
        stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus::NoFrames;
    let mut slot_result_kinds = [
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
    ];
    let mut slot_diagnostics = std::array::from_fn(|slot_index| {
        format_four_view_preview_slot_diagnostic(&unobserved_four_view_preview_slot_diagnostic(
            slot_index,
            &slots[slot_index],
        ))
    });
    let mut clean_output_render_result_kind = "NoRenderableQuadView";
    let mut window_title = SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE.to_string();
    let cadence = four_view_clean_output_window_loop_frame_cadence();
    let slow_attempt_threshold_ms = cadence.as_millis().saturating_mul(2).max(1);
    let obs_runtime =
        ObsFriendlyFourViewLoopWindowRenderRuntime::with_timing(render_runtime, Rc::clone(&timing));
    let timed_decode_runtime =
        TimedSwitcherH264DecodeRuntime::new(decode_runtime, Rc::clone(&timing));
    let mut previous_slots: [Option<SwitcherFourViewDisplayedSlot>; 4] =
        std::array::from_fn(|_| None);
    let mut decoded_slot_identities: [Option<TwoRealPreviewLoopDecodedSlotIdentity>; 4] =
        std::array::from_fn(|_| None);
    let mut previous_visual_identities: Option<[TwoRealPreviewLoopSlotVisualIdentity; 4]> = None;
    let mut previous_bgra_composition: Option<SwitcherFourViewQuadCompositionResult> = None;
    let mut previous_clean_output: Option<SwitcherFourViewCleanOutputWindowRenderResult> = None;
    let mut quad_composition_cache = TwoRealPreviewLoopQuadCompositionCache::default();
    let mut quad_view_compose_attempt_count = 0u32;
    let mut quad_view_compose_success_count = 0u32;
    let mut quad_view_compose_skipped_unchanged_count = 0u32;
    let mut quad_view_composed_frame_reuse_count = 0u32;
    let mut quad_view_visual_unchanged_count = 0u32;
    let mut quad_view_visual_changed_count = 0u32;
    let mut materialization_reason_first_render_count = 0u32;
    let mut materialization_reason_visual_changed_count = 0u32;
    let mut materialization_reason_previous_output_missing_count = 0u32;
    let mut materialization_reason_profile_or_size_mismatch_count = 0u32;
    let mut materialization_reason_force_render_count = 0u32;
    let mut materialization_reason_unknown_count = 0u32;
    let mut slot_frame_id_changed_counts = [0u32; 4];
    let mut slot_selected_source_changed_counts = [0u32; 4];
    let mut placeholder_visual_changed_count = 0u32;
    let mut quad_view_incremental_update_count = 0u32;
    let mut quad_view_full_compose_count = 0u32;
    let mut quad_view_changed_slot_update_count = 0u32;
    let mut quad_view_reused_slot_count = 0u32;
    let mut render_input_unchanged_count = 0u32;
    let mut render_reuse_frame_count = 0u32;
    let loop_start = Instant::now();

    for frame_index in 0..frames.get() {
        let attempt_start = Instant::now();
        handoff.begin_frame();
        let target_timestamp = target_timestamp_hook();
        let previous_slot_identities = decoded_slot_identities.clone();
        let timing_before_validation = timing.borrow().clone();
        let validation_start = Instant::now();
        let validation_input = SwitcherFourViewHandoffValidationInput {
            slots: slots.clone(),
            target_timestamp,
            previous_slots: previous_slots.clone(),
            display_current_time: TimestampMicros(target_timestamp.0.saturating_add(1)),
            layout_policy: SwitcherFourViewQuadLayoutPolicy::default(),
            composed_window_title: "StreamSync 4-view".to_string(),
            composed_render_hold_millis: 0,
        };
        let pre_composition = SwitcherFourViewHandoffValidationBoundary::default()
            .run_from_handoff_to_composition_render_with_runtimes_and_mode(
                &mut handoff,
                &validation_input,
                preview_target_time_mode_from_switcher_mode(read_mode),
                &timed_decode_runtime,
            );
        let validation_elapsed_ms = validation_start.elapsed().as_millis();
        let timing_after_validation = timing.borrow().clone();
        let validation_handoff_elapsed_ms = timing_after_validation
            .handoff_elapsed_ms
            .saturating_sub(timing_before_validation.handoff_elapsed_ms);
        let validation_decode_elapsed_ms = timing_after_validation
            .decode_elapsed_ms
            .saturating_sub(timing_before_validation.decode_elapsed_ms);
        let _pre_composition_elapsed_ms = validation_elapsed_ms
            .saturating_sub(validation_handoff_elapsed_ms)
            .saturating_sub(validation_decode_elapsed_ms);
        let current_slot_identities = update_two_real_decoded_slot_identities(
            &decoded_slot_identities,
            &handoff.last_frame_slot_observations,
            &pre_composition,
        );
        let current_visual_identities =
            two_real_preview_loop_visual_identities(&current_slot_identities, &pre_composition);
        let visual_change_diagnostics = two_real_preview_loop_visual_change_diagnostics(
            previous_visual_identities.as_ref(),
            &current_visual_identities,
        );
        for slot_index in 0..4 {
            slot_frame_id_changed_counts[slot_index] = slot_frame_id_changed_counts[slot_index]
                .saturating_add(visual_change_diagnostics.frame_id_changed_counts[slot_index]);
            slot_selected_source_changed_counts[slot_index] =
                slot_selected_source_changed_counts[slot_index].saturating_add(
                    visual_change_diagnostics.selected_source_changed_counts[slot_index],
                );
        }
        placeholder_visual_changed_count = placeholder_visual_changed_count
            .saturating_add(visual_change_diagnostics.placeholder_visual_changed_count);
        let visual_unchanged = previous_visual_identities
            .as_ref()
            .map(|previous| {
                two_real_preview_loop_visual_identities_render_equal(
                    previous,
                    &current_visual_identities,
                )
            })
            .unwrap_or(false);
        if visual_unchanged {
            quad_view_visual_unchanged_count += 1;
        } else {
            quad_view_visual_changed_count += 1;
        }
        let previous_clean_output_is_rendered = previous_clean_output
            .as_ref()
            .map(clean_output_window_result_was_rendered)
            .unwrap_or(false);
        let can_reuse_rendered_output = visual_unchanged && previous_clean_output_is_rendered;
        if visual_unchanged {
            render_input_unchanged_count += 1;
        }

        let mut render_facing_for_next: Option<
            stream_sync_switcher::SwitcherFourViewQuadRenderFacingConnectionOutput,
        > = None;
        let clean_output = if can_reuse_rendered_output {
            render_reuse_frame_count += 1;
            timing.borrow_mut().render_buffer_reuse_count += 1;
            render_runtime.pump_persistent_window_events();
            previous_clean_output
                .clone()
                .expect("render reuse should have previous clean output")
        } else {
            let bgra_composition = if visual_unchanged {
                if let Some(previous_composition) = previous_bgra_composition.clone() {
                    timing.borrow_mut().composed_buffer_clone_count += 1;
                    quad_view_compose_skipped_unchanged_count += 1;
                    quad_view_reused_slot_count += 4;
                    if matches!(
                        previous_composition,
                        SwitcherFourViewQuadCompositionResult::ComposedFrame { .. }
                    ) {
                        quad_view_composed_frame_reuse_count += 1;
                    }
                    SwitcherFourViewQuadCompositionOutput {
                        scheduler_status: pre_composition.scheduler.status,
                        connection: pre_composition.composition_render.clone(),
                        composition: previous_composition,
                    }
                } else {
                    compose_two_real_preview_quad_view(
                        &pre_composition,
                        validation_input.layout_policy,
                        &timing,
                        &mut quad_composition_cache,
                        previous_visual_identities.as_ref(),
                        &current_visual_identities,
                        &mut quad_view_compose_attempt_count,
                        &mut quad_view_compose_success_count,
                        &mut quad_view_incremental_update_count,
                        &mut quad_view_full_compose_count,
                        &mut quad_view_changed_slot_update_count,
                        &mut quad_view_reused_slot_count,
                    )
                }
            } else {
                compose_two_real_preview_quad_view(
                    &pre_composition,
                    validation_input.layout_policy,
                    &timing,
                    &mut quad_composition_cache,
                    previous_visual_identities.as_ref(),
                    &current_visual_identities,
                    &mut quad_view_compose_attempt_count,
                    &mut quad_view_compose_success_count,
                    &mut quad_view_incremental_update_count,
                    &mut quad_view_full_compose_count,
                    &mut quad_view_changed_slot_update_count,
                    &mut quad_view_reused_slot_count,
                )
            };
            let render_facing = SwitcherFourViewQuadRenderFacingConnectionBoundary::default()
                .connect_composition_output(bgra_composition);
            let timing_before_render = timing.borrow().clone();
            let render_ready_before_clean_output = matches!(
                &render_facing.render,
                stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult::RenderReady { .. }
            );
            let render_start = Instant::now();
            let clean_output_render = render_four_view_clean_output_from_borrowed_render_facing(
                &render_facing,
                &obs_runtime,
            );
            let render_call_elapsed_ms = render_start.elapsed().as_millis();
            {
                let timing_after_render = timing.borrow().clone();
                let render_buffer_copy_delta = timing_after_render
                    .render_buffer_copy_elapsed_ms
                    .saturating_sub(timing_before_render.render_buffer_copy_elapsed_ms);
                let render_backend_wait_delta = timing_after_render
                    .render_backend_wait_elapsed_ms
                    .saturating_sub(timing_before_render.render_backend_wait_elapsed_ms);
                let mut timing = timing.borrow_mut();
                timing.render_elapsed_ms += render_call_elapsed_ms;
                timing.render_call_elapsed_ms += render_call_elapsed_ms;
                timing.render_prepare_elapsed_ms += render_call_elapsed_ms
                    .saturating_sub(render_buffer_copy_delta)
                    .saturating_sub(render_backend_wait_delta);
                timing.render_call_count += 1;
                if render_ready_before_clean_output {
                    match classify_two_real_preview_loop_materialization_reason(
                        frames_attempted,
                        previous_clean_output.as_ref(),
                        visual_unchanged,
                    ) {
                        TwoRealPreviewLoopMaterializationReason::FirstRender => {
                            materialization_reason_first_render_count =
                                materialization_reason_first_render_count.saturating_add(1);
                        }
                        TwoRealPreviewLoopMaterializationReason::VisualChanged => {
                            materialization_reason_visual_changed_count =
                                materialization_reason_visual_changed_count.saturating_add(1);
                        }
                        TwoRealPreviewLoopMaterializationReason::PreviousOutputMissing => {
                            materialization_reason_previous_output_missing_count =
                                materialization_reason_previous_output_missing_count
                                    .saturating_add(1);
                        }
                        TwoRealPreviewLoopMaterializationReason::ProfileOrSizeMismatch => {
                            materialization_reason_profile_or_size_mismatch_count =
                                materialization_reason_profile_or_size_mismatch_count
                                    .saturating_add(1);
                        }
                        TwoRealPreviewLoopMaterializationReason::ForceRender => {
                            materialization_reason_force_render_count =
                                materialization_reason_force_render_count.saturating_add(1);
                        }
                        TwoRealPreviewLoopMaterializationReason::Unknown => {
                            materialization_reason_unknown_count =
                                materialization_reason_unknown_count.saturating_add(1);
                        }
                    }
                }
            }
            render_facing_for_next = Some(render_facing);
            clean_output_render
        };

        if let Some(render_facing) = render_facing_for_next {
            previous_bgra_composition = Some(render_facing.composition.composition);
        }

        frames_attempted += 1;
        scheduler_status = pre_composition.scheduler.status;
        slot_result_kinds = pre_composition.scheduler.slots.clone().map(|slot| {
            format_four_view_real_handoff_scheduler_slot_kind(&slot.result).to_string()
        });
        clean_output_render_result_kind =
            format_four_view_clean_output_window_result_kind(&clean_output);
        window_title = SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE.to_string();
        let unchanged_reuse_slots = two_real_unchanged_frame_reuse_slots(
            &previous_slot_identities,
            &current_slot_identities,
            &pre_composition,
        );
        slot_diagnostics = handoff.take_last_frame_slot_diagnostics_from_pre_composition(
            &pre_composition,
            unchanged_reuse_slots,
        );
        let tick_diagnostics = four_view_two_real_tick_diagnostics_from_composition_render(
            &slot_result_kinds,
            &pre_composition.composition_render,
            &clean_output,
        );
        selected_count += tick_diagnostics.selected_count;
        no_frame_count += tick_diagnostics.no_frame_count;
        handoff_error_count += tick_diagnostics.handoff_error_count;
        render_success_count += tick_diagnostics.render_success_count;
        render_failure_count += tick_diagnostics.render_failure_count;
        let unchanged_reuse = two_real_unchanged_frame_reuse_count(&unchanged_reuse_slots);
        unchanged_frame_reuse_count += unchanged_reuse;
        skipped_decode_unchanged_frame_count += unchanged_reuse;

        if let SwitcherFourViewCleanOutputWindowRenderResult::RenderReady { render, .. } =
            &clean_output
        {
            match render {
                SwitcherFourViewComposedCanvasRenderResult::Rendered { .. } => {
                    frames_rendered += 1;
                    if first_render_attempt_index.is_none() {
                        first_render_attempt_index = Some(frames_attempted);
                        first_render_elapsed_ms = Some(loop_start.elapsed().as_millis());
                    }
                }
                SwitcherFourViewComposedCanvasRenderResult::RenderFailed { .. } => {
                    render_failures += 1;
                }
                _ => {}
            }
        }
        previous_slots = update_four_view_previous_slots_from_composition_render(
            &previous_slots,
            &pre_composition.composition_render,
        );
        decoded_slot_identities = current_slot_identities;
        previous_visual_identities = Some(current_visual_identities);
        previous_clean_output = Some(clean_output.clone());

        let attempt_elapsed_ms = attempt_start.elapsed().as_millis();
        {
            let mut timing = timing.borrow_mut();
            timing.attempt_body_elapsed_ms += attempt_elapsed_ms;
            timing.max_attempt_elapsed_ms = timing.max_attempt_elapsed_ms.max(attempt_elapsed_ms);
            if attempt_elapsed_ms > slow_attempt_threshold_ms {
                timing.slow_attempt_count += 1;
            }
        }

        if frame_index + 1 < frames.get() {
            let sleep_start = Instant::now();
            cadence_sleep.sleep(cadence);
            let sleep_elapsed_ms = sleep_start.elapsed().as_millis();
            let mut timing = timing.borrow_mut();
            timing.loop_sleep_elapsed_ms += sleep_elapsed_ms;
            timing.frame_interval_wait_elapsed_ms += sleep_elapsed_ms;
        }
    }

    render_runtime.close_persistent_window();
    let render_metadata = obs_runtime.metadata_snapshot();
    let lifecycle = render_runtime.lifecycle_snapshot();
    timing.borrow_mut().event_pump_elapsed_ms += lifecycle.event_pump_elapsed_ms;
    let timing = timing.borrow().clone();
    let loop_total_elapsed_ms = loop_start.elapsed().as_millis();
    let fps_diagnostics = build_two_real_preview_loop_fps_diagnostics(
        loop_total_elapsed_ms,
        cadence,
        frames_attempted,
        frames_rendered,
        first_render_attempt_index,
        first_render_elapsed_ms,
    );

    SwitcherFourViewTwoRealHandoffPreviewLoopSummary {
        real_slot0_index: slot0_index,
        real_slot1_index: slot1_index,
        pipe_name: pipe_name.to_string(),
        actual_pipe_path,
        preview_mode: format_handoff_mode(read_mode),
        read_mode: format_handoff_read_mode(handoff_read_mode_from_switcher_mode(read_mode)),
        client0_id,
        run0_id,
        client1_id,
        run1_id,
        frames_attempted,
        frames_rendered,
        render_failures,
        elapsed_ms: fps_diagnostics.elapsed_ms,
        target_fps: fps_diagnostics.target_fps,
        configured_frame_interval_ms: fps_diagnostics.configured_frame_interval_ms,
        effective_attempt_fps: fps_diagnostics.effective_attempt_fps,
        effective_render_fps: fps_diagnostics.effective_render_fps,
        first_render_attempt_index: fps_diagnostics.first_render_attempt_index,
        first_render_elapsed_ms: fps_diagnostics.first_render_elapsed_ms,
        rendered_after_first_render: fps_diagnostics.rendered_after_first_render,
        effective_render_fps_after_first_render: fps_diagnostics
            .effective_render_fps_after_first_render,
        no_render_before_first_render: fps_diagnostics.no_render_before_first_render,
        selected_count,
        no_frame_count,
        handoff_error_count,
        decode_attempt_count: timing.decode_attempt_count,
        decode_success_count: timing.decode_success_count,
        render_success_count,
        render_failure_count,
        unchanged_frame_reuse_count,
        skipped_decode_unchanged_frame_count,
        redecoded_same_frame_count,
        decode_elapsed_ms: timing.decode_elapsed_ms,
        decode_process_spawn_elapsed_ms: timing.decode_process_spawn_elapsed_ms,
        decode_input_write_elapsed_ms: timing.decode_input_write_elapsed_ms,
        decode_input_payload_bytes_total: timing.decode_input_payload_bytes_total,
        decode_output_read_elapsed_ms: timing.decode_output_read_elapsed_ms,
        decode_output_read_exact_elapsed_ms: timing.decode_output_read_exact_elapsed_ms,
        decode_output_vec_resize_elapsed_ms: timing.decode_output_vec_resize_elapsed_ms,
        decode_process_wait_elapsed_ms: timing.decode_process_wait_elapsed_ms,
        decode_pixel_convert_elapsed_ms: timing.decode_pixel_convert_elapsed_ms,
        decode_buffer_allocation_count: timing.decode_buffer_allocation_count,
        decode_output_bytes_total: timing.decode_output_bytes_total,
        decode_stdout_expected_bytes_total: timing.decode_stdout_expected_bytes_total,
        decode_cached_frame_reuse_count: timing.decode_cached_frame_reuse_count,
        decode_cache_miss_count: timing.decode_cache_miss_count,
        decoded_buffer_clone_count: timing.decoded_buffer_clone_count,
        decode_cache_hit_clone_count: timing.decode_cache_hit_clone_count,
        decode_cache_store_clone_count: timing.decode_cache_store_clone_count,
        decoded_buffer_clone_elapsed_ms: timing.decoded_buffer_clone_elapsed_ms,
        composed_buffer_clone_count: timing.composed_buffer_clone_count,
        decode_output_buffer_reuse_count: timing.decode_output_buffer_reuse_count,
        persistent_decode_enabled: timing.persistent_decode_enabled,
        persistent_decode_attempt_count: timing.persistent_decode_attempt_count,
        persistent_decode_success_count: timing.persistent_decode_success_count,
        persistent_decode_failure_count: timing.persistent_decode_failure_count,
        persistent_decode_fallback_count: timing.persistent_decode_fallback_count,
        persistent_decode_process_spawn_count: timing.persistent_decode_process_spawn_count,
        persistent_decode_process_restart_count: timing.persistent_decode_process_restart_count,
        persistent_decode_stdin_write_elapsed_ms: timing.persistent_decode_stdin_write_elapsed_ms,
        persistent_decode_stdout_read_elapsed_ms: timing.persistent_decode_stdout_read_elapsed_ms,
        persistent_decode_stdout_read_exact_elapsed_ms: timing
            .persistent_decode_stdout_read_exact_elapsed_ms,
        persistent_decode_output_bytes_total: timing.persistent_decode_output_bytes_total,
        persistent_decode_last_error: timing
            .persistent_decode_last_error
            .clone()
            .unwrap_or_else(|| "none".to_string()),
        one_shot_decode_fallback_count: timing.one_shot_decode_fallback_count,
        handoff_elapsed_ms: timing.handoff_elapsed_ms,
        render_elapsed_ms: timing.render_elapsed_ms,
        avg_decode_elapsed_ms: format_preview_loop_average_elapsed(
            timing.decode_elapsed_ms,
            timing.decode_attempt_count,
        ),
        avg_decode_input_write_elapsed_ms: format_preview_loop_average_elapsed(
            timing.decode_input_write_elapsed_ms,
            timing.decode_attempt_count,
        ),
        avg_decode_output_read_elapsed_ms: format_preview_loop_average_elapsed(
            timing.decode_output_read_elapsed_ms,
            timing.decode_attempt_count,
        ),
        avg_decode_process_spawn_elapsed_ms: format_preview_loop_average_elapsed(
            timing.decode_process_spawn_elapsed_ms,
            timing.decode_attempt_count,
        ),
        avg_handoff_elapsed_ms: format_preview_loop_average_elapsed(
            timing.handoff_elapsed_ms,
            timing.handoff_call_count,
        ),
        avg_render_elapsed_ms: format_preview_loop_average_elapsed(
            timing.render_elapsed_ms,
            timing.render_call_count,
        ),
        loop_total_elapsed_ms,
        attempt_body_elapsed_ms: timing.attempt_body_elapsed_ms,
        loop_sleep_elapsed_ms: timing.loop_sleep_elapsed_ms,
        frame_interval_wait_elapsed_ms: timing.frame_interval_wait_elapsed_ms,
        event_pump_elapsed_ms: timing.event_pump_elapsed_ms,
        window_update_elapsed_ms: timing.window_update_elapsed_ms,
        render_prepare_elapsed_ms: timing.render_prepare_elapsed_ms,
        render_buffer_cpu_scale_copy_elapsed_ms: timing.render_buffer_copy_elapsed_ms,
        render_buffer_copy_elapsed_ms: timing.render_buffer_copy_elapsed_ms,
        render_buffer_materialization_elapsed_ms: timing.render_buffer_copy_elapsed_ms,
        render_buffer_scale_prepare_elapsed_ms: timing.render_buffer_scale_prepare_elapsed_ms,
        render_buffer_scale_loop_elapsed_ms: timing.render_buffer_scale_loop_elapsed_ms,
        render_buffer_output_copy_elapsed_ms: timing.render_buffer_output_copy_elapsed_ms,
        render_buffer_resize_elapsed_ms: timing.render_buffer_resize_elapsed_ms,
        render_buffer_clear_elapsed_ms: timing.render_buffer_clear_elapsed_ms,
        render_buffer_passthrough_count: timing.render_buffer_passthrough_count,
        render_buffer_same_size_copy_count: timing.render_buffer_same_size_copy_count,
        render_buffer_half_scale_count: timing.render_buffer_half_scale_count,
        render_buffer_generic_scale_count: timing.render_buffer_generic_scale_count,
        render_buffer_reuse_count: timing.render_buffer_reuse_count,
        render_buffer_allocation_count: timing.render_buffer_allocation_count,
        render_buffer_bytes_copied_total: timing.render_buffer_bytes_copied_total,
        render_backend_wait_elapsed_ms: timing.render_backend_wait_elapsed_ms,
        gdi_invalidate_elapsed_ms: lifecycle.gdi_invalidate_elapsed_ms,
        gdi_paint_wait_elapsed_ms: lifecycle.gdi_paint_wait_elapsed_ms,
        gdi_wm_paint_elapsed_ms: lifecycle.gdi_wm_paint_elapsed_ms,
        gdi_stretchdibits_elapsed_ms: lifecycle.gdi_stretchdibits_elapsed_ms,
        texture_upload_elapsed_ms: timing.texture_upload_elapsed_ms,
        window_present_elapsed_ms: timing.window_present_elapsed_ms,
        vsync_or_present_block_elapsed_ms: timing.vsync_or_present_block_elapsed_ms,
        quad_view_compose_elapsed_ms: timing.quad_view_compose_elapsed_ms,
        quad_view_compose_attempt_count,
        quad_view_compose_success_count,
        quad_view_compose_skipped_unchanged_count,
        quad_view_composed_frame_reuse_count,
        quad_view_visual_unchanged_count,
        quad_view_visual_changed_count,
        materialization_reason_first_render_count,
        materialization_reason_visual_changed_count,
        materialization_reason_previous_output_missing_count,
        materialization_reason_profile_or_size_mismatch_count,
        materialization_reason_force_render_count,
        materialization_reason_unknown_count,
        slot0_frame_id_changed_count: slot_frame_id_changed_counts[0],
        slot1_frame_id_changed_count: slot_frame_id_changed_counts[1],
        slot2_frame_id_changed_count: slot_frame_id_changed_counts[2],
        slot3_frame_id_changed_count: slot_frame_id_changed_counts[3],
        slot0_selected_source_changed_count: slot_selected_source_changed_counts[0],
        slot1_selected_source_changed_count: slot_selected_source_changed_counts[1],
        slot2_selected_source_changed_count: slot_selected_source_changed_counts[2],
        slot3_selected_source_changed_count: slot_selected_source_changed_counts[3],
        placeholder_visual_changed_count,
        quad_view_incremental_update_count,
        quad_view_full_compose_count,
        quad_view_changed_slot_update_count,
        quad_view_reused_slot_count,
        quad_view_allocation_count: quad_composition_cache.allocation_count,
        avg_render_buffer_cpu_scale_copy_elapsed_ms: format_preview_loop_average_elapsed(
            timing.render_buffer_copy_elapsed_ms,
            timing.render_call_count,
        ),
        avg_render_buffer_materialization_elapsed_ms: format_preview_loop_average_elapsed(
            timing.render_buffer_copy_elapsed_ms,
            timing.render_call_count,
        ),
        avg_gdi_paint_wait_elapsed_ms: format_preview_loop_average_elapsed(
            lifecycle.gdi_paint_wait_elapsed_ms,
            timing.render_call_count,
        ),
        avg_gdi_wm_paint_elapsed_ms: format_preview_loop_average_elapsed(
            lifecycle.gdi_wm_paint_elapsed_ms,
            timing.render_call_count,
        ),
        avg_gdi_stretchdibits_elapsed_ms: format_preview_loop_average_elapsed(
            lifecycle.gdi_stretchdibits_elapsed_ms,
            timing.render_call_count,
        ),
        avg_quad_view_incremental_update_elapsed_ms: format_preview_loop_average_elapsed(
            timing.quad_view_incremental_update_elapsed_ms,
            quad_view_incremental_update_count,
        ),
        avg_quad_view_compose_elapsed_ms: format_preview_loop_average_elapsed(
            timing.quad_view_compose_elapsed_ms,
            quad_view_compose_attempt_count,
        ),
        render_call_elapsed_ms: timing.render_call_elapsed_ms,
        render_input_unchanged_count,
        render_reuse_frame_count,
        unaccounted_elapsed_ms: loop_total_elapsed_ms
            .saturating_sub(timing.attempt_body_elapsed_ms)
            .saturating_sub(timing.loop_sleep_elapsed_ms),
        avg_attempt_elapsed_ms: format_preview_loop_average_elapsed(
            timing.attempt_body_elapsed_ms,
            frames_attempted,
        ),
        max_attempt_elapsed_ms: timing.max_attempt_elapsed_ms,
        slow_attempt_count: timing.slow_attempt_count,
        slow_attempt_threshold_ms,
        scheduler_status,
        slot_bindings,
        slot_result_kinds,
        slot_diagnostics,
        clean_output_render_result_kind,
        window_title,
        output_width: render_metadata.output_width,
        output_height: render_metadata.output_height,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_and_sleep<
    RealHandoff,
    DecodeRuntime,
    RenderRuntime,
>(
    pipe_name: &str,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    frames: NonZeroU32,
    read_mode: SwitcherSingleClientQueueSourceMode,
    target_timestamp: TimestampMicros,
    real_handoff: RealHandoff,
    decode_runtime: &DecodeRuntime,
    render_runtime: &RenderRuntime,
    cadence_sleep: &impl SwitcherFrameCadenceSleepHook,
) -> SwitcherFourViewFourRealHandoffPreviewLoopSummary
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook + SwitcherPersistentWindowLoopRuntimeHook,
{
    run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep(
        pipe_name,
        client0_id,
        run0_id,
        client1_id,
        run1_id,
        client2_id,
        run2_id,
        client3_id,
        run3_id,
        frames,
        read_mode,
        move || target_timestamp,
        real_handoff,
        decode_runtime,
        render_runtime,
        cadence_sleep,
    )
}

fn run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep<
    RealHandoff,
    DecodeRuntime,
    RenderRuntime,
    TargetTimestampHook,
>(
    pipe_name: &str,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    frames: NonZeroU32,
    read_mode: SwitcherSingleClientQueueSourceMode,
    mut target_timestamp_hook: TargetTimestampHook,
    real_handoff: RealHandoff,
    decode_runtime: &DecodeRuntime,
    render_runtime: &RenderRuntime,
    cadence_sleep: &impl SwitcherFrameCadenceSleepHook,
) -> SwitcherFourViewFourRealHandoffPreviewLoopSummary
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook + SwitcherPersistentWindowLoopRuntimeHook,
    TargetTimestampHook: FnMut() -> TimestampMicros,
{
    let actual_pipe_path = format_actual_handoff_pipe_path(pipe_name);
    let slots = default_four_view_four_real_handoff_preview_slots(
        client0_id.clone(),
        run0_id.clone(),
        client1_id.clone(),
        run1_id.clone(),
        client2_id.clone(),
        run2_id.clone(),
        client3_id.clone(),
        run3_id.clone(),
    );
    let slot_bindings = slots
        .clone()
        .map(|slot| format_four_view_slot_binding(&slot));
    let mut handoff = MultiRealAndPlaceholderQueuedFrameHandoff::new(
        slots.clone(),
        vec![
            (client0_id.clone(), run0_id.clone()),
            (client1_id.clone(), run1_id.clone()),
            (client2_id.clone(), run2_id.clone()),
            (client3_id.clone(), run3_id.clone()),
        ],
        real_handoff,
    );
    let mut frames_attempted = 0u32;
    let mut frames_rendered = 0u32;
    let mut render_failures = 0u32;
    let mut scheduler_status =
        stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus::NoFrames;
    let mut slot_result_kinds = [
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
    ];
    let mut slot_diagnostics = std::array::from_fn(|slot_index| {
        format_four_view_preview_slot_diagnostic(&unobserved_four_view_preview_slot_diagnostic(
            slot_index,
            &slots[slot_index],
        ))
    });
    let mut clean_output_render_result_kind = "NoRenderableQuadView";
    let mut window_title = SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE.to_string();
    let cadence = four_view_clean_output_window_loop_frame_cadence();
    let obs_runtime = ObsFriendlyFourViewLoopWindowRenderRuntime::new(render_runtime);

    for frame_index in 0..frames.get() {
        handoff.begin_frame();
        let target_timestamp = target_timestamp_hook();
        let validation = run_four_view_real_handoff_preview_validation_with_runtime_and_handoff(
            &mut handoff,
            slots.clone(),
            target_timestamp,
            preview_target_time_mode_from_switcher_mode(read_mode),
            decode_runtime,
        );
        let clean_output = SwitcherFourViewCleanOutputWindowBoundary::default()
            .render_with_runtime(
                SwitcherFourViewCleanOutputWindowInput {
                    render_facing_output: validation.render_facing.clone(),
                },
                &obs_runtime,
            );

        frames_attempted += 1;
        scheduler_status = validation.scheduler.status;
        slot_result_kinds = validation.scheduler.slots.clone().map(|slot| {
            format_four_view_real_handoff_scheduler_slot_kind(&slot.result).to_string()
        });
        slot_diagnostics = handoff.take_last_frame_slot_diagnostics(&validation);
        clean_output_render_result_kind =
            format_four_view_clean_output_window_result_kind(&clean_output.output_window);
        window_title = clean_output.window_identity.title.clone();

        if let SwitcherFourViewCleanOutputWindowRenderResult::RenderReady { render, .. } =
            &clean_output.output_window
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
    let render_metadata = obs_runtime.metadata_snapshot();

    SwitcherFourViewFourRealHandoffPreviewLoopSummary {
        pipe_name: pipe_name.to_string(),
        actual_pipe_path,
        preview_mode: format_handoff_mode(read_mode),
        read_mode: format_handoff_read_mode(handoff_read_mode_from_switcher_mode(read_mode)),
        client0_id,
        run0_id,
        client1_id,
        run1_id,
        client2_id,
        run2_id,
        client3_id,
        run3_id,
        frames_attempted,
        frames_rendered,
        render_failures,
        scheduler_status,
        slot_bindings,
        slot_result_kinds,
        slot_diagnostics,
        clean_output_render_result_kind,
        window_title,
        output_width: render_metadata.output_width,
        output_height: render_metadata.output_height,
    }
}

fn run_four_view_focused_handoff_preview_loop_with_handoff_runtime_and_sleep<
    RealHandoff,
    DecodeRuntime,
    RenderRuntime,
>(
    pipe_name: &str,
    focused_slot_index: usize,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    frames: NonZeroU32,
    target_timestamp: TimestampMicros,
    real_handoff: RealHandoff,
    decode_runtime: &DecodeRuntime,
    render_runtime: &RenderRuntime,
    cadence_sleep: &impl SwitcherFrameCadenceSleepHook,
) -> SwitcherFourViewFocusedHandoffPreviewLoopSummary
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook + SwitcherPersistentWindowLoopRuntimeHook,
{
    let actual_pipe_path = format_actual_handoff_pipe_path(pipe_name);
    let slots = default_four_view_four_real_handoff_preview_slots(
        client0_id.clone(),
        run0_id.clone(),
        client1_id.clone(),
        run1_id.clone(),
        client2_id.clone(),
        run2_id.clone(),
        client3_id.clone(),
        run3_id.clone(),
    );
    let slot_bindings = slots
        .clone()
        .map(|slot| format_four_view_slot_binding(&slot));
    let focused_client_id = slots[focused_slot_index].client_id.clone();
    let focused_run_id = slots[focused_slot_index].run_id.clone();
    let mut handoff = MultiRealAndPlaceholderQueuedFrameHandoff::new(
        slots.clone(),
        vec![
            (client0_id.clone(), run0_id.clone()),
            (client1_id.clone(), run1_id.clone()),
            (client2_id.clone(), run2_id.clone()),
            (client3_id.clone(), run3_id.clone()),
        ],
        real_handoff,
    );
    let mut frames_attempted = 0u32;
    let mut frames_rendered = 0u32;
    let mut render_failures = 0u32;
    let mut scheduler_status =
        stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus::NoFrames;
    let mut slot_result_kinds = [
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
    ];
    let mut slot_diagnostics = std::array::from_fn(|slot_index| {
        format_four_view_preview_slot_diagnostic(&unobserved_four_view_preview_slot_diagnostic(
            slot_index,
            &slots[slot_index],
        ))
    });
    let mut focused_result_kind = "NoFrameAvailable".to_string();
    let mut clean_output_render_result_kind = "NoRenderableFocusedView";
    let window_title = SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE.to_string();
    let cadence = four_view_clean_output_window_loop_frame_cadence();
    let obs_runtime = ObsFriendlyFourViewLoopWindowRenderRuntime::new(render_runtime);

    for frame_index in 0..frames.get() {
        handoff.begin_frame();
        let validation = run_four_view_real_handoff_preview_validation_with_runtime_and_handoff(
            &mut handoff,
            slots.clone(),
            target_timestamp,
            stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
            decode_runtime,
        );

        frames_attempted += 1;
        scheduler_status = validation.scheduler.status;
        slot_result_kinds = validation.scheduler.slots.clone().map(|slot| {
            format_four_view_real_handoff_scheduler_slot_kind(&slot.result).to_string()
        });
        slot_diagnostics = handoff.take_last_frame_slot_diagnostics(&validation);
        focused_result_kind = slot_result_kinds[focused_slot_index].clone();
        clean_output_render_result_kind = render_four_view_focused_slot_with_runtime(
            &validation.composition_render.composition.slots[focused_slot_index],
            &obs_runtime,
            &window_title,
        );

        match clean_output_render_result_kind {
            "Rendered" => {
                frames_rendered += 1;
            }
            "RenderFailed" | "InvalidFrame" => {
                render_failures += 1;
            }
            _ => {}
        }

        if frame_index + 1 < frames.get() {
            cadence_sleep.sleep(cadence);
        }
    }

    render_runtime.close_persistent_window();
    let render_metadata = obs_runtime.metadata_snapshot();

    SwitcherFourViewFocusedHandoffPreviewLoopSummary {
        pipe_name: pipe_name.to_string(),
        actual_pipe_path,
        focused_slot_index,
        client0_id,
        run0_id,
        client1_id,
        run1_id,
        client2_id,
        run2_id,
        client3_id,
        run3_id,
        focused_client_id,
        focused_run_id,
        focused_result_kind,
        frames_attempted,
        frames_rendered,
        render_failures,
        scheduler_status,
        slot_bindings,
        slot_result_kinds,
        slot_diagnostics,
        clean_output_render_result_kind,
        window_title,
        output_width: render_metadata.output_width,
        output_height: render_metadata.output_height,
    }
}

#[cfg(windows)]
fn run_four_view_controlled_handoff_preview_loop(
    pipe_name: &str,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    max_ticks_per_command: NonZeroU32,
    command_source: FourViewControlCommandSource,
) -> Result<SwitcherFourViewControlledHandoffPreviewLoopSummary, String> {
    if matches!(
        &command_source,
        FourViewControlCommandSource::ControlPipe(control_pipe_name) if control_pipe_name == pipe_name
    ) {
        return Err("control pipe name must differ from handoff pipe name".to_string());
    }
    let handoff = ObservedNamedPipePreviewHandoff::new(SwitcherNamedPipeQueuedFrameHandoff::new(
        pipe_name,
        DEFAULT_ONE_SHOT_REQUEST_ID,
    ));
    let render_runtime = SwitcherWindowsGdiPersistentWindowRenderRuntime::default();
    Ok(
        run_four_view_controlled_handoff_preview_loop_with_handoff_runtime_and_sleep(
            pipe_name,
            client0_id,
            run0_id,
            client1_id,
            run1_id,
            client2_id,
            run2_id,
            client3_id,
            run3_id,
            max_ticks_per_command,
            command_source,
            real_four_view_preview_target_timestamp(),
            handoff,
            &SwitcherFfmpegH264DecodeRuntimeHook::default(),
            &render_runtime,
            &RealSwitcherFrameCadenceSleepHook,
        ),
    )
}

#[cfg(not(windows))]
fn run_four_view_controlled_handoff_preview_loop(
    _pipe_name: &str,
    _client0_id: ClientId,
    _run0_id: RunId,
    _client1_id: ClientId,
    _run1_id: RunId,
    _client2_id: ClientId,
    _run2_id: RunId,
    _client3_id: ClientId,
    _run3_id: RunId,
    _max_ticks_per_command: NonZeroU32,
    _command_source: FourViewControlCommandSource,
) -> Result<SwitcherFourViewControlledHandoffPreviewLoopSummary, String> {
    Err("four-view controlled handoff preview loop is only available on Windows".to_string())
}

fn run_four_view_controlled_handoff_preview_loop_with_handoff_runtime_and_sleep<
    RealHandoff,
    DecodeRuntime,
    RenderRuntime,
>(
    pipe_name: &str,
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
    max_ticks_per_command: NonZeroU32,
    command_source: FourViewControlCommandSource,
    target_timestamp: TimestampMicros,
    real_handoff: RealHandoff,
    decode_runtime: &DecodeRuntime,
    render_runtime: &RenderRuntime,
    cadence_sleep: &impl SwitcherFrameCadenceSleepHook,
) -> SwitcherFourViewControlledHandoffPreviewLoopSummary
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook + SwitcherPersistentWindowLoopRuntimeHook,
{
    let actual_pipe_path = format_actual_handoff_pipe_path(pipe_name);
    let slots = default_four_view_four_real_handoff_preview_slots(
        client0_id.clone(),
        run0_id.clone(),
        client1_id.clone(),
        run1_id.clone(),
        client2_id.clone(),
        run2_id.clone(),
        client3_id.clone(),
        run3_id.clone(),
    );
    let slot_bindings = slots
        .clone()
        .map(|slot| format_four_view_slot_binding(&slot));
    let mut handoff = MultiRealAndPlaceholderQueuedFrameHandoff::new(
        slots.clone(),
        vec![
            (client0_id.clone(), run0_id.clone()),
            (client1_id.clone(), run1_id.clone()),
            (client2_id.clone(), run2_id.clone()),
            (client3_id.clone(), run3_id.clone()),
        ],
        real_handoff,
    );
    let obs_runtime = ObsFriendlyFourViewLoopWindowRenderRuntime::new(render_runtime);
    let command_source_summary = match &command_source {
        FourViewControlCommandSource::Scripted(script) => {
            format!("scripted:{}", sanitize_summary_value(script))
        }
        FourViewControlCommandSource::ControlPipe(pipe_name) => {
            format!("control-pipe:{}", sanitize_summary_value(pipe_name))
        }
        FourViewControlCommandSource::Stdin => "stdin".to_string(),
    };
    let mut final_view_state = SwitcherFourViewControlledPreviewViewState::AllView;
    let mut commands_processed = 0u32;
    let mut commands_rejected = 0u32;
    let mut frames_rendered_total = 0u32;
    let mut render_failures_total = 0u32;
    let mut scheduler_status =
        stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus::NoFrames;
    let mut slot_result_kinds = [
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
    ];
    let mut slot_diagnostics = std::array::from_fn(|slot_index| {
        format_four_view_preview_slot_diagnostic(&unobserved_four_view_preview_slot_diagnostic(
            slot_index,
            &slots[slot_index],
        ))
    });
    let mut view_render_mode = "AllView".to_string();
    let mut output_layout = "QuadView".to_string();
    let mut rendered_slot_count = 0u32;
    let mut focused_slot_index = None;
    let mut clean_output_render_result_kind = "NoRenderableQuadView".to_string();
    let mut all_view_render_result_kind = "NoRenderableQuadView".to_string();
    let mut window_title = SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE.to_string();
    let mut exit_reason = "CommandSequenceCompleted".to_string();
    let mut command_summaries = Vec::new();

    match command_source {
        FourViewControlCommandSource::Scripted(script) => {
            let commands = split_scripted_control_commands(&script);
            for (command_index, raw_command) in commands.iter().enumerate() {
                let command_outcome = run_four_view_control_command_iteration(
                    FourViewControlledCommandIterationInput {
                        command_index,
                        raw_command,
                        current_view_state: &mut final_view_state,
                        handoff: &mut handoff,
                        slots: slots.clone(),
                        target_timestamp,
                        max_ticks_per_command,
                        decode_runtime,
                        obs_runtime: &obs_runtime,
                        cadence_sleep,
                    },
                );
                let command_summary = &command_outcome.summary;
                commands_processed += 1;
                if command_summary.transition_result == "Rejected" {
                    commands_rejected += 1;
                }
                if command_summary.exit_reason != "none" {
                    exit_reason = command_summary.exit_reason.clone();
                }
                if let Some(render_outcome) = command_outcome.render_outcome {
                    view_render_mode = render_outcome.view_render_mode;
                    output_layout = render_outcome.output_layout;
                    rendered_slot_count = render_outcome.rendered_slot_count;
                    focused_slot_index = render_outcome.focused_slot_index;
                    frames_rendered_total += render_outcome.frames_rendered;
                    render_failures_total += render_outcome.render_failures;
                    scheduler_status = render_outcome.scheduler_status;
                    slot_result_kinds = render_outcome.slot_result_kinds;
                    slot_diagnostics = render_outcome.slot_diagnostics;
                    clean_output_render_result_kind =
                        render_outcome.clean_output_render_result_kind;
                    all_view_render_result_kind = render_outcome.all_view_render_result_kind;
                    window_title = render_outcome.window_title;
                }
                command_summaries.push(command_summary.clone());
                if command_summary.exit_reason == "QuitRequested" {
                    break;
                }
            }
        }
        FourViewControlCommandSource::ControlPipe(control_pipe_name) => {
            for command_index in 0usize.. {
                let round_trip =
                    match run_control_pipe_command_round_trip(&control_pipe_name, |raw_command| {
                        let trimmed = raw_command.trim().to_string();
                        let command_outcome = run_four_view_control_command_iteration(
                            FourViewControlledCommandIterationInput {
                                command_index,
                                raw_command: &trimmed,
                                current_view_state: &mut final_view_state,
                                handoff: &mut handoff,
                                slots: slots.clone(),
                                target_timestamp,
                                max_ticks_per_command,
                                decode_runtime,
                                obs_runtime: &obs_runtime,
                                cadence_sleep,
                            },
                        );
                        let response = format_four_view_control_pipe_command_response(
                            &trimmed,
                            &command_outcome.summary,
                        );
                        (trimmed, command_outcome, response)
                    }) {
                        Ok(round_trip) => round_trip,
                        Err(error) => {
                            exit_reason = format!(
                                "ControlPipeRuntimeError:{}",
                                sanitize_summary_value(&error.to_string())
                            );
                            break;
                        }
                    };
                let (_, command_outcome) = round_trip;
                let command_summary = &command_outcome.summary;
                commands_processed += 1;
                if command_summary.transition_result == "Rejected" {
                    commands_rejected += 1;
                }
                if command_summary.exit_reason != "none" {
                    exit_reason = command_summary.exit_reason.clone();
                }
                if let Some(render_outcome) = command_outcome.render_outcome {
                    view_render_mode = render_outcome.view_render_mode;
                    output_layout = render_outcome.output_layout;
                    rendered_slot_count = render_outcome.rendered_slot_count;
                    focused_slot_index = render_outcome.focused_slot_index;
                    frames_rendered_total += render_outcome.frames_rendered;
                    render_failures_total += render_outcome.render_failures;
                    scheduler_status = render_outcome.scheduler_status;
                    slot_result_kinds = render_outcome.slot_result_kinds;
                    slot_diagnostics = render_outcome.slot_diagnostics;
                    clean_output_render_result_kind =
                        render_outcome.clean_output_render_result_kind;
                    all_view_render_result_kind = render_outcome.all_view_render_result_kind;
                    window_title = render_outcome.window_title;
                }
                command_summaries.push(command_summary.clone());
                if command_summary.exit_reason == "QuitRequested" {
                    break;
                }
            }
        }
        FourViewControlCommandSource::Stdin => {
            let stdin = io::stdin();
            for (command_index, line) in stdin.lock().lines().enumerate() {
                let line = match line {
                    Ok(line) => line,
                    Err(error) => {
                        exit_reason = format!(
                            "StdinReadError:{}",
                            sanitize_summary_value(&error.to_string())
                        );
                        break;
                    }
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let command_outcome = run_four_view_control_command_iteration(
                    FourViewControlledCommandIterationInput {
                        command_index,
                        raw_command: trimmed,
                        current_view_state: &mut final_view_state,
                        handoff: &mut handoff,
                        slots: slots.clone(),
                        target_timestamp,
                        max_ticks_per_command,
                        decode_runtime,
                        obs_runtime: &obs_runtime,
                        cadence_sleep,
                    },
                );
                let command_summary = &command_outcome.summary;
                commands_processed += 1;
                if command_summary.transition_result == "Rejected" {
                    commands_rejected += 1;
                }
                if command_summary.exit_reason != "none" {
                    exit_reason = command_summary.exit_reason.clone();
                }
                if let Some(render_outcome) = command_outcome.render_outcome {
                    view_render_mode = render_outcome.view_render_mode;
                    output_layout = render_outcome.output_layout;
                    rendered_slot_count = render_outcome.rendered_slot_count;
                    focused_slot_index = render_outcome.focused_slot_index;
                    frames_rendered_total += render_outcome.frames_rendered;
                    render_failures_total += render_outcome.render_failures;
                    scheduler_status = render_outcome.scheduler_status;
                    slot_result_kinds = render_outcome.slot_result_kinds;
                    slot_diagnostics = render_outcome.slot_diagnostics;
                    clean_output_render_result_kind =
                        render_outcome.clean_output_render_result_kind;
                    all_view_render_result_kind = render_outcome.all_view_render_result_kind;
                    window_title = render_outcome.window_title;
                }
                command_summaries.push(command_summary.clone());
                if command_summary.exit_reason == "QuitRequested" {
                    break;
                }
            }
            if exit_reason == "CommandSequenceCompleted" {
                exit_reason = "EndOfInput".to_string();
            }
        }
    }

    render_runtime.close_persistent_window();
    let render_metadata = obs_runtime.metadata_snapshot();

    SwitcherFourViewControlledHandoffPreviewLoopSummary {
        pipe_name: pipe_name.to_string(),
        actual_pipe_path,
        client0_id,
        run0_id,
        client1_id,
        run1_id,
        client2_id,
        run2_id,
        client3_id,
        run3_id,
        max_ticks_per_command: max_ticks_per_command.get(),
        command_source: command_source_summary,
        command_summaries,
        final_view_state,
        commands_processed,
        commands_rejected,
        view_render_mode,
        output_layout,
        rendered_slot_count,
        focused_slot_index,
        frames_rendered: frames_rendered_total,
        render_failures: render_failures_total,
        scheduler_status,
        slot_bindings,
        slot_result_kinds,
        slot_diagnostics,
        clean_output_render_result_kind,
        all_view_render_result_kind,
        window_title,
        output_width: render_metadata.output_width,
        output_height: render_metadata.output_height,
        exit_reason,
    }
}

struct FourViewControlledCommandIterationInput<'a, RealHandoff, DecodeRuntime, RenderRuntime> {
    command_index: usize,
    raw_command: &'a str,
    current_view_state: &'a mut SwitcherFourViewControlledPreviewViewState,
    handoff: &'a mut MultiRealAndPlaceholderQueuedFrameHandoff<RealHandoff>,
    slots: [SwitcherFourViewTargetTimeSourceSlotConfig; 4],
    target_timestamp: TimestampMicros,
    max_ticks_per_command: NonZeroU32,
    decode_runtime: &'a DecodeRuntime,
    obs_runtime: &'a ObsFriendlyFourViewLoopWindowRenderRuntime<'a, RenderRuntime>,
    cadence_sleep: &'a dyn SwitcherFrameCadenceSleepHook,
}

fn run_four_view_control_command_iteration<RealHandoff, DecodeRuntime, RenderRuntime>(
    input: FourViewControlledCommandIterationInput<'_, RealHandoff, DecodeRuntime, RenderRuntime>,
) -> FourViewControlledCommandIterationOutcome
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook,
{
    let parsed = parse_four_view_control_command(input.raw_command);
    let command_name: String;
    let mut view_render_mode = "none".to_string();
    let mut output_layout = "none".to_string();
    let requested_transition: String;
    let transition_result: String;
    let mut selected_slot_result = "NotApplicable".to_string();
    let mut rendered_slot_count = 0u32;
    let mut focused_slot_index = None;
    let mut frames_rendered = 0u32;
    let mut render_failures = 0u32;
    let mut scheduler_status =
        stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus::NoFrames;
    let mut clean_output_render_result_kind = "none".to_string();
    let mut all_view_render_result_kind = "not_applicable".to_string();
    let mut command_parse_error = "none".to_string();
    let mut exit_reason = "none".to_string();
    let mut render_outcome = None;

    match parsed {
        Ok(SwitcherFourViewControlledPreviewCommand::All) => {
            command_name = "all".to_string();
            requested_transition = "AllView".to_string();
            transition_result = match &*input.current_view_state {
                SwitcherFourViewControlledPreviewViewState::AllView => "NoChange".to_string(),
                _ => "Transitioned".to_string(),
            };
            *input.current_view_state = SwitcherFourViewControlledPreviewViewState::AllView;
            let rendered = render_four_view_controlled_state_for_ticks(
                input.current_view_state,
                input.handoff,
                input.slots,
                input.target_timestamp,
                input.max_ticks_per_command,
                input.decode_runtime,
                input.obs_runtime,
                input.cadence_sleep,
            );
            view_render_mode = rendered.view_render_mode.clone();
            output_layout = rendered.output_layout.clone();
            selected_slot_result = rendered.selected_slot_result.clone();
            rendered_slot_count = rendered.rendered_slot_count;
            focused_slot_index = rendered.focused_slot_index;
            frames_rendered = rendered.frames_rendered;
            render_failures = rendered.render_failures;
            scheduler_status = rendered.scheduler_status;
            clean_output_render_result_kind = rendered.clean_output_render_result_kind.clone();
            all_view_render_result_kind = rendered.all_view_render_result_kind.clone();
            render_outcome = Some(rendered);
        }
        Ok(SwitcherFourViewControlledPreviewCommand::Focus(slot_index)) => {
            command_name = "focus".to_string();
            requested_transition = format!("Focused({slot_index})");
            transition_result = match &*input.current_view_state {
                SwitcherFourViewControlledPreviewViewState::Focused(current)
                    if *current == slot_index =>
                {
                    "NoChange".to_string()
                }
                _ => "Transitioned".to_string(),
            };
            *input.current_view_state =
                SwitcherFourViewControlledPreviewViewState::Focused(slot_index);
            let rendered = render_four_view_controlled_state_for_ticks(
                input.current_view_state,
                input.handoff,
                input.slots,
                input.target_timestamp,
                input.max_ticks_per_command,
                input.decode_runtime,
                input.obs_runtime,
                input.cadence_sleep,
            );
            view_render_mode = rendered.view_render_mode.clone();
            output_layout = rendered.output_layout.clone();
            selected_slot_result = rendered.selected_slot_result.clone();
            rendered_slot_count = rendered.rendered_slot_count;
            focused_slot_index = rendered.focused_slot_index;
            frames_rendered = rendered.frames_rendered;
            render_failures = rendered.render_failures;
            scheduler_status = rendered.scheduler_status;
            clean_output_render_result_kind = rendered.clean_output_render_result_kind.clone();
            all_view_render_result_kind = rendered.all_view_render_result_kind.clone();
            render_outcome = Some(rendered);
        }
        Ok(SwitcherFourViewControlledPreviewCommand::Status) => {
            command_name = "status".to_string();
            requested_transition = "Status".to_string();
            transition_result = "Observed".to_string();
            let rendered = render_four_view_controlled_state_for_ticks(
                input.current_view_state,
                input.handoff,
                input.slots,
                input.target_timestamp,
                input.max_ticks_per_command,
                input.decode_runtime,
                input.obs_runtime,
                input.cadence_sleep,
            );
            view_render_mode = rendered.view_render_mode.clone();
            output_layout = rendered.output_layout.clone();
            selected_slot_result = rendered.selected_slot_result.clone();
            rendered_slot_count = rendered.rendered_slot_count;
            focused_slot_index = rendered.focused_slot_index;
            frames_rendered = rendered.frames_rendered;
            render_failures = rendered.render_failures;
            scheduler_status = rendered.scheduler_status;
            clean_output_render_result_kind = rendered.clean_output_render_result_kind.clone();
            all_view_render_result_kind = rendered.all_view_render_result_kind.clone();
            render_outcome = Some(rendered);
        }
        Ok(SwitcherFourViewControlledPreviewCommand::Quit) => {
            command_name = "quit".to_string();
            requested_transition = "Quit".to_string();
            transition_result = "ExitRequested".to_string();
            exit_reason = "QuitRequested".to_string();
        }
        Err(error) => {
            command_name = error.command_name;
            requested_transition = error.requested_transition;
            transition_result = "Rejected".to_string();
            command_parse_error = error.message;
        }
    }

    FourViewControlledCommandIterationOutcome {
        summary: SwitcherFourViewControlledPreviewCommandSummary {
            command_index: input.command_index,
            control_command_name: command_name,
            current_view_state: (*input.current_view_state).clone(),
            view_render_mode,
            output_layout,
            requested_transition,
            transition_result,
            selected_slot_result,
            rendered_slot_count,
            focused_slot_index,
            frames_rendered,
            render_failures,
            scheduler_status,
            clean_output_render_result_kind,
            all_view_render_result_kind,
            command_parse_error,
            exit_reason,
        },
        render_outcome,
    }
}

fn render_four_view_controlled_state_for_ticks<RealHandoff, DecodeRuntime, RenderRuntime>(
    current_view_state: &SwitcherFourViewControlledPreviewViewState,
    handoff: &mut MultiRealAndPlaceholderQueuedFrameHandoff<RealHandoff>,
    slots: [SwitcherFourViewTargetTimeSourceSlotConfig; 4],
    target_timestamp: TimestampMicros,
    ticks: NonZeroU32,
    decode_runtime: &DecodeRuntime,
    obs_runtime: &ObsFriendlyFourViewLoopWindowRenderRuntime<'_, RenderRuntime>,
    cadence_sleep: &dyn SwitcherFrameCadenceSleepHook,
) -> FourViewControlledPreviewRenderOutcome
where
    RealHandoff: PreviewLoopRealHandoff,
    DecodeRuntime: SwitcherH264DecodeRuntimeHook,
    RenderRuntime: SwitcherWindowRenderRuntimeHook,
{
    let view_render_mode = match current_view_state {
        SwitcherFourViewControlledPreviewViewState::AllView => "AllView".to_string(),
        SwitcherFourViewControlledPreviewViewState::Focused(_) => "Focused".to_string(),
    };
    let output_layout = match current_view_state {
        SwitcherFourViewControlledPreviewViewState::AllView => "QuadView".to_string(),
        SwitcherFourViewControlledPreviewViewState::Focused(_) => "FocusedFullWindow".to_string(),
    };
    let mut frames_rendered = 0u32;
    let mut render_failures = 0u32;
    let mut scheduler_status =
        stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus::NoFrames;
    let mut slot_result_kinds = [
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
        "NoFrameAvailable".to_string(),
    ];
    let mut slot_diagnostics = std::array::from_fn(|slot_index| {
        format_four_view_preview_slot_diagnostic(&unobserved_four_view_preview_slot_diagnostic(
            slot_index,
            &slots[slot_index],
        ))
    });
    let mut clean_output_render_result_kind = match current_view_state {
        SwitcherFourViewControlledPreviewViewState::AllView => "NoRenderableQuadView".to_string(),
        SwitcherFourViewControlledPreviewViewState::Focused(_) => {
            "NoRenderableFocusedView".to_string()
        }
    };
    let mut all_view_render_result_kind = "not_applicable".to_string();
    let mut window_title = SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE.to_string();
    let mut rendered_slot_count = 0u32;

    for frame_index in 0..ticks.get() {
        handoff.begin_frame();
        let validation = run_four_view_real_handoff_preview_validation_with_runtime_and_handoff(
            handoff,
            slots.clone(),
            target_timestamp,
            stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode::PreviewLatestIfAtOrBefore,
            decode_runtime,
        );

        scheduler_status = validation.scheduler.status;
        slot_result_kinds = validation.scheduler.slots.clone().map(|slot| {
            format_four_view_real_handoff_scheduler_slot_kind(&slot.result).to_string()
        });
        slot_diagnostics = handoff.take_last_frame_slot_diagnostics(&validation);

        match current_view_state {
            SwitcherFourViewControlledPreviewViewState::AllView => {
                let clean_output = SwitcherFourViewCleanOutputWindowBoundary::default()
                    .render_with_runtime(
                        SwitcherFourViewCleanOutputWindowInput {
                            render_facing_output: validation.render_facing.clone(),
                        },
                        obs_runtime,
                    );
                clean_output_render_result_kind =
                    format_four_view_clean_output_window_result_kind(&clean_output.output_window)
                        .to_string();
                all_view_render_result_kind = clean_output_render_result_kind.clone();
                window_title = clean_output.window_identity.title.clone();
                rendered_slot_count = count_renderable_four_view_slots(
                    &validation.composition_render.composition.slots,
                );
                if let SwitcherFourViewCleanOutputWindowRenderResult::RenderReady {
                    render, ..
                } = &clean_output.output_window
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
            }
            SwitcherFourViewControlledPreviewViewState::Focused(focused_slot_index) => {
                clean_output_render_result_kind = render_four_view_focused_slot_with_runtime(
                    &validation.composition_render.composition.slots[*focused_slot_index],
                    obs_runtime,
                    &window_title,
                )
                .to_string();
                rendered_slot_count = focused_four_view_slot_is_renderable(
                    &validation.composition_render.composition.slots[*focused_slot_index],
                ) as u32;
                match clean_output_render_result_kind.as_str() {
                    "Rendered" => frames_rendered += 1,
                    "RenderFailed" | "InvalidFrame" => render_failures += 1,
                    _ => {}
                }
            }
        }

        if frame_index + 1 < ticks.get() {
            cadence_sleep.sleep(four_view_clean_output_window_loop_frame_cadence());
        }
    }

    let selected_slot_result = match current_view_state {
        SwitcherFourViewControlledPreviewViewState::AllView => "NotApplicable".to_string(),
        SwitcherFourViewControlledPreviewViewState::Focused(slot_index) => {
            slot_result_kinds[*slot_index].clone()
        }
    };
    let render_metadata = obs_runtime.metadata_snapshot();

    FourViewControlledPreviewRenderOutcome {
        view_render_mode,
        output_layout,
        selected_slot_result,
        rendered_slot_count,
        focused_slot_index: match current_view_state {
            SwitcherFourViewControlledPreviewViewState::AllView => None,
            SwitcherFourViewControlledPreviewViewState::Focused(slot_index) => Some(*slot_index),
        },
        frames_rendered,
        render_failures,
        scheduler_status,
        slot_result_kinds,
        slot_diagnostics,
        clean_output_render_result_kind,
        all_view_render_result_kind,
        window_title,
        output_width: render_metadata.output_width,
        output_height: render_metadata.output_height,
    }
}

fn focused_four_view_slot_is_renderable(
    slot: &SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> bool {
    match slot {
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame { frame, .. }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            frame, ..
        } => frame.decoded.is_some(),
        _ => false,
    }
}

fn count_renderable_four_view_slots(
    slots: &[SwitcherFourViewHandoffQuadCompositionRenderSlot; 4],
) -> u32 {
    slots
        .iter()
        .filter(|slot| focused_four_view_slot_is_renderable(slot))
        .count() as u32
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FourViewControlCommandParseError {
    command_name: String,
    requested_transition: String,
    message: String,
}

fn parse_four_view_control_command(
    raw: &str,
) -> Result<SwitcherFourViewControlledPreviewCommand, FourViewControlCommandParseError> {
    let trimmed = raw.trim();
    let mut tokens = trimmed.split_whitespace();
    let Some(command_name) = tokens.next() else {
        return Err(FourViewControlCommandParseError {
            command_name: "empty".to_string(),
            requested_transition: "none".to_string(),
            message: "empty command".to_string(),
        });
    };
    match command_name {
        "all" => {
            if tokens.next().is_some() {
                return Err(FourViewControlCommandParseError {
                    command_name: "all".to_string(),
                    requested_transition: "AllView".to_string(),
                    message: "all does not accept arguments".to_string(),
                });
            }
            Ok(SwitcherFourViewControlledPreviewCommand::All)
        }
        "status" => {
            if tokens.next().is_some() {
                return Err(FourViewControlCommandParseError {
                    command_name: "status".to_string(),
                    requested_transition: "Status".to_string(),
                    message: "status does not accept arguments".to_string(),
                });
            }
            Ok(SwitcherFourViewControlledPreviewCommand::Status)
        }
        "quit" => {
            if tokens.next().is_some() {
                return Err(FourViewControlCommandParseError {
                    command_name: "quit".to_string(),
                    requested_transition: "Quit".to_string(),
                    message: "quit does not accept arguments".to_string(),
                });
            }
            Ok(SwitcherFourViewControlledPreviewCommand::Quit)
        }
        "focus" => {
            let Some(slot_index_raw) = tokens.next() else {
                return Err(FourViewControlCommandParseError {
                    command_name: "focus".to_string(),
                    requested_transition: "Focused(?)".to_string(),
                    message: "missing focus index".to_string(),
                });
            };
            if tokens.next().is_some() {
                return Err(FourViewControlCommandParseError {
                    command_name: "focus".to_string(),
                    requested_transition: format!("Focused({slot_index_raw})"),
                    message: "focus accepts exactly one slot index".to_string(),
                });
            }
            let slot_index =
                slot_index_raw
                    .parse::<usize>()
                    .map_err(|_| FourViewControlCommandParseError {
                        command_name: "focus".to_string(),
                        requested_transition: format!("Focused({slot_index_raw})"),
                        message: "invalid focus index: expected integer 0..3".to_string(),
                    })?;
            if slot_index > 3 {
                return Err(FourViewControlCommandParseError {
                    command_name: "focus".to_string(),
                    requested_transition: format!("Focused({slot_index_raw})"),
                    message: "invalid focus index: expected integer 0..3".to_string(),
                });
            }
            Ok(SwitcherFourViewControlledPreviewCommand::Focus(slot_index))
        }
        _ => Err(FourViewControlCommandParseError {
            command_name: sanitize_summary_value(command_name),
            requested_transition: sanitize_summary_value(trimmed),
            message: "unknown command".to_string(),
        }),
    }
}

fn split_scripted_control_commands(script: &str) -> Vec<String> {
    script
        .split(';')
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn split_scripted_operator_wrapper_keys(script: &str) -> Vec<String> {
    script
        .split(';')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(ToString::to_string)
        .collect()
}

trait FourViewOperatorWrapperClock {
    fn now_millis(&self) -> u64;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct SystemFourViewOperatorWrapperClock;

impl FourViewOperatorWrapperClock for SystemFourViewOperatorWrapperClock {
    fn now_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_millis(0))
            .as_millis() as u64
    }
}

trait FourViewOperatorWrapperRawKeyReader {
    fn read_next_key(&mut self) -> Result<Option<String>, String>;
}

trait FourViewOperatorWrapperRawKeyRuntime {
    type Reader: FourViewOperatorWrapperRawKeyReader;

    fn open(
        &mut self,
    ) -> Result<
        (
            Self::Reader,
            FourViewOperatorWrapperRawConsoleRestoreTracker,
        ),
        String,
    >;
}

#[derive(Debug, Clone)]
struct FourViewOperatorWrapperRawConsoleRestoreTracker {
    state: Rc<RefCell<FourViewOperatorWrapperRawConsoleRestoreState>>,
}

impl Default for FourViewOperatorWrapperRawConsoleRestoreTracker {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(
                FourViewOperatorWrapperRawConsoleRestoreState::Pending,
            )),
        }
    }
}

impl FourViewOperatorWrapperRawConsoleRestoreTracker {
    fn mark_restored(&self) {
        *self.state.borrow_mut() = FourViewOperatorWrapperRawConsoleRestoreState::Restored;
    }

    fn mark_failed(&self, error: String) {
        *self.state.borrow_mut() = FourViewOperatorWrapperRawConsoleRestoreState::Failed(error);
    }

    fn summary_fields(&self) -> (String, String) {
        match &*self.state.borrow() {
            FourViewOperatorWrapperRawConsoleRestoreState::Pending => {
                ("pending".to_string(), "none".to_string())
            }
            FourViewOperatorWrapperRawConsoleRestoreState::Restored => {
                ("restored".to_string(), "none".to_string())
            }
            FourViewOperatorWrapperRawConsoleRestoreState::Failed(error) => {
                ("restore_failed".to_string(), sanitize_summary_value(error))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FourViewOperatorWrapperRawConsoleRestoreState {
    Pending,
    Restored,
    Failed(String),
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct WindowsFourViewOperatorWrapperRawKeyRuntime;

#[cfg(target_os = "windows")]
struct WindowsFourViewOperatorWrapperRawKeyReader {
    stdin: io::Stdin,
    _restore_guard: WindowsConsoleModeRestoreGuard,
}

#[cfg(target_os = "windows")]
struct WindowsConsoleModeRestoreGuard {
    handle: HANDLE,
    original_mode: CONSOLE_MODE,
    restore_tracker: FourViewOperatorWrapperRawConsoleRestoreTracker,
    restored: bool,
}

#[cfg(target_os = "windows")]
impl WindowsConsoleModeRestoreGuard {
    fn new(
        handle: HANDLE,
        original_mode: CONSOLE_MODE,
        restore_tracker: FourViewOperatorWrapperRawConsoleRestoreTracker,
    ) -> Self {
        Self {
            handle,
            original_mode,
            restore_tracker,
            restored: false,
        }
    }

    fn restore_now(&mut self) {
        if self.restored {
            return;
        }

        match unsafe { SetConsoleMode(self.handle, self.original_mode) } {
            Ok(()) => self.restore_tracker.mark_restored(),
            Err(_) => self
                .restore_tracker
                .mark_failed(format!("{}", io::Error::last_os_error())),
        }
        self.restored = true;
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsConsoleModeRestoreGuard {
    fn drop(&mut self) {
        self.restore_now();
    }
}

#[cfg(target_os = "windows")]
impl FourViewOperatorWrapperRawKeyRuntime for WindowsFourViewOperatorWrapperRawKeyRuntime {
    type Reader = WindowsFourViewOperatorWrapperRawKeyReader;

    fn open(
        &mut self,
    ) -> Result<
        (
            Self::Reader,
            FourViewOperatorWrapperRawConsoleRestoreTracker,
        ),
        String,
    > {
        let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) }.map_err(|_| {
            format!(
                "console stdin handle unavailable: {}",
                io::Error::last_os_error()
            )
        })?;
        let mut original_mode = CONSOLE_MODE(0);
        unsafe { GetConsoleMode(handle, &mut original_mode) }.map_err(|_| {
            format!(
                "raw key capture requires a Windows console stdin handle: {}",
                io::Error::last_os_error()
            )
        })?;

        // Disable line buffering and echo so one key can be read immediately.
        let raw_mode = CONSOLE_MODE(original_mode.0 & !(ENABLE_LINE_INPUT.0 | ENABLE_ECHO_INPUT.0));
        unsafe { SetConsoleMode(handle, raw_mode) }.map_err(|_| {
            format!(
                "raw key console mode setup failed: {}",
                io::Error::last_os_error()
            )
        })?;

        let restore_tracker = FourViewOperatorWrapperRawConsoleRestoreTracker::default();
        let restore_guard =
            WindowsConsoleModeRestoreGuard::new(handle, original_mode, restore_tracker.clone());

        Ok((
            WindowsFourViewOperatorWrapperRawKeyReader {
                stdin: io::stdin(),
                _restore_guard: restore_guard,
            },
            restore_tracker,
        ))
    }
}

#[cfg(target_os = "windows")]
impl FourViewOperatorWrapperRawKeyReader for WindowsFourViewOperatorWrapperRawKeyReader {
    fn read_next_key(&mut self) -> Result<Option<String>, String> {
        let mut buffer = [0u8; 1];
        self.stdin
            .read_exact(&mut buffer)
            .map_err(|error| format!("raw key read failed: {error}"))?;
        Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
    }
}

#[cfg(windows)]
fn run_four_view_operator_wrapper(
    control_pipe_name: &str,
    input_source: FourViewOperatorWrapperInputSource,
) -> Result<FourViewOperatorWrapperLoopSummary, String> {
    let mut raw_key_runtime = WindowsFourViewOperatorWrapperRawKeyRuntime;
    run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime(
        control_pipe_name,
        input_source,
        &mut SwitcherFourViewControlNamedPipeClientRuntime,
        &SystemFourViewOperatorWrapperClock,
        &mut raw_key_runtime,
    )
}

#[cfg(not(windows))]
fn run_four_view_operator_wrapper(
    _control_pipe_name: &str,
    _input_source: FourViewOperatorWrapperInputSource,
) -> Result<FourViewOperatorWrapperLoopSummary, String> {
    Err("four-view operator wrapper is only available on Windows".to_string())
}

fn run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime<
    TRawKeyRuntime: FourViewOperatorWrapperRawKeyRuntime,
>(
    control_pipe_name: &str,
    input_source: FourViewOperatorWrapperInputSource,
    runtime: &mut impl FourViewControlPipeClientRuntime,
    clock: &impl FourViewOperatorWrapperClock,
    raw_key_runtime: &mut TRawKeyRuntime,
) -> Result<FourViewOperatorWrapperLoopSummary, String> {
    if control_pipe_name.trim().is_empty() {
        return Err("control pipe name must not be empty".to_string());
    }

    let input_source_summary = match &input_source {
        FourViewOperatorWrapperInputSource::Stdin => "stdin".to_string(),
        FourViewOperatorWrapperInputSource::ScriptedKeys(script) => {
            format!("scripted:{}", sanitize_summary_value(script))
        }
        FourViewOperatorWrapperInputSource::RawKeys => "raw_keys".to_string(),
    };

    let mut guard_state = FourViewOperatorWrapperGuardState {
        quit_armed: false,
        armed_until_millis: None,
    };
    let mut key_summaries = Vec::new();
    let mut commands_sent = 0u32;
    let mut ignored_keys = 0u32;
    let mut exit_reason = "InputClosed".to_string();
    let mut raw_console_restore_result = "not_applicable".to_string();
    let mut raw_console_restore_error = "none".to_string();

    match input_source {
        FourViewOperatorWrapperInputSource::Stdin => {
            let stdin = io::stdin();
            for (key_index, line) in stdin.lock().lines().enumerate() {
                let line = line.map_err(|error| format!("stdin read failed: {error}"))?;
                let summary = process_four_view_operator_wrapper_key(
                    key_index,
                    &line,
                    &mut guard_state,
                    control_pipe_name,
                    runtime,
                    clock,
                );
                if summary.send_result == "Sent" {
                    commands_sent += 1;
                }
                if summary.send_result == "Ignored" {
                    ignored_keys += 1;
                }
                let should_exit = summary.exit_reason != "Continue";
                if should_exit {
                    exit_reason = summary.exit_reason.clone();
                }
                key_summaries.push(summary);
                if should_exit {
                    break;
                }
            }
            if key_summaries.is_empty() {
                exit_reason = "InputClosed".to_string();
            } else if exit_reason == "Continue" {
                exit_reason = "InputClosed".to_string();
            }
        }
        FourViewOperatorWrapperInputSource::ScriptedKeys(script) => {
            let keys = split_scripted_operator_wrapper_keys(&script);
            exit_reason = "ScriptedKeysCompleted".to_string();
            for (key_index, key) in keys.iter().enumerate() {
                let summary = process_four_view_operator_wrapper_key(
                    key_index,
                    key,
                    &mut guard_state,
                    control_pipe_name,
                    runtime,
                    clock,
                );
                if summary.send_result == "Sent" {
                    commands_sent += 1;
                }
                if summary.send_result == "Ignored" {
                    ignored_keys += 1;
                }
                let should_exit = summary.exit_reason != "Continue";
                if should_exit {
                    exit_reason = summary.exit_reason.clone();
                }
                key_summaries.push(summary);
                if should_exit {
                    break;
                }
            }
        }
        FourViewOperatorWrapperInputSource::RawKeys => {
            let (mut raw_key_reader, restore_tracker) = raw_key_runtime
                .open()
                .map_err(|error| format!("raw key setup failed: {error}"))?;
            let raw_key_result = (|| -> Result<(), String> {
                loop {
                    let Some(raw_key) = raw_key_reader
                        .read_next_key()
                        .map_err(|error| format!("raw key read failed: {error}"))?
                    else {
                        break;
                    };
                    let key_index = key_summaries.len();
                    let summary = process_four_view_operator_wrapper_key(
                        key_index,
                        &raw_key,
                        &mut guard_state,
                        control_pipe_name,
                        runtime,
                        clock,
                    );
                    if summary.send_result == "Sent" {
                        commands_sent += 1;
                    }
                    if summary.send_result == "Ignored" {
                        ignored_keys += 1;
                    }
                    let should_exit = summary.exit_reason != "Continue";
                    if should_exit {
                        exit_reason = summary.exit_reason.clone();
                    }
                    key_summaries.push(summary);
                    if should_exit {
                        break;
                    }
                }
                Ok(())
            })();
            drop(raw_key_reader);
            (raw_console_restore_result, raw_console_restore_error) =
                restore_tracker.summary_fields();
            if raw_console_restore_result == "restore_failed" {
                return match raw_key_result {
                    Ok(()) => Err(format!(
                        "raw key console mode restore failed: {}",
                        raw_console_restore_error
                    )),
                    Err(error) => Err(format!(
                        "{error}; raw key console mode restore failed: {}",
                        raw_console_restore_error
                    )),
                };
            }
            raw_key_result?;
            if key_summaries.is_empty() {
                exit_reason = "InputClosed".to_string();
            } else if exit_reason == "Continue" {
                exit_reason = "InputClosed".to_string();
            }
        }
    }

    Ok(FourViewOperatorWrapperLoopSummary {
        control_pipe_name: control_pipe_name.to_string(),
        input_source: input_source_summary,
        keys_processed: key_summaries.len() as u32,
        key_summaries,
        commands_sent,
        ignored_keys,
        final_guard_state: format_four_view_operator_wrapper_guard_state(&guard_state),
        raw_console_restore_result,
        raw_console_restore_error,
        exit_reason,
    })
}

fn process_four_view_operator_wrapper_key(
    key_index: usize,
    raw_key: &str,
    guard_state: &mut FourViewOperatorWrapperGuardState,
    control_pipe_name: &str,
    runtime: &mut impl FourViewControlPipeClientRuntime,
    clock: &impl FourViewOperatorWrapperClock,
) -> FourViewOperatorWrapperKeySummary {
    clear_expired_four_view_operator_wrapper_quit_guard(guard_state, clock.now_millis());

    let wrapper_key = sanitize_summary_value(raw_key.trim());
    let key_char = parse_four_view_operator_wrapper_key(raw_key);
    let mut mapped_command = "none".to_string();
    let send_result;
    let mut response_line = "none".to_string();
    let mut command_parse_error = "none".to_string();
    let mut wrapper_error = "none".to_string();
    let mut exit_reason = "Continue".to_string();

    match key_char {
        Some('q') | Some('Q') => {
            mapped_command = "quit".to_string();
            if guard_state.quit_armed {
                guard_state.quit_armed = false;
                guard_state.armed_until_millis = None;
                match run_send_control_command_with_runtime(control_pipe_name, "quit", runtime) {
                    Ok(response) => {
                        send_result = "Sent".to_string();
                        command_parse_error =
                            extract_summary_field(&response, "command_parse_error")
                                .unwrap_or_else(|| "none".to_string());
                        exit_reason = normalize_four_view_operator_wrapper_exit_reason(
                            extract_summary_field(&response, "exit_reason").as_deref(),
                        );
                        response_line = sanitize_summary_value(&response);
                    }
                    Err(error) => {
                        send_result = "SendFailed".to_string();
                        wrapper_error = sanitize_summary_value(&error);
                        exit_reason = "WrapperSendFailed".to_string();
                    }
                }
            } else {
                guard_state.quit_armed = true;
                guard_state.armed_until_millis =
                    Some(clock.now_millis() + DEFAULT_OPERATOR_WRAPPER_QUIT_GUARD_WINDOW_MILLIS);
                send_result = "GuardArmed".to_string();
            }
        }
        Some(mapped @ ('1' | '2' | '3' | '4' | '0' | 'a' | 'A' | 's' | 'S')) => {
            clear_four_view_operator_wrapper_quit_guard(guard_state);
            let command = match mapped {
                '1' => "focus 0",
                '2' => "focus 1",
                '3' => "focus 2",
                '4' => "focus 3",
                '0' | 'a' | 'A' => "all",
                's' | 'S' => "status",
                _ => unreachable!("supported mapped keys are handled explicitly"),
            };
            mapped_command = sanitize_summary_value(command);
            match run_send_control_command_with_runtime(control_pipe_name, command, runtime) {
                Ok(response) => {
                    send_result = "Sent".to_string();
                    command_parse_error = extract_summary_field(&response, "command_parse_error")
                        .unwrap_or_else(|| "none".to_string());
                    exit_reason = normalize_four_view_operator_wrapper_exit_reason(
                        extract_summary_field(&response, "exit_reason").as_deref(),
                    );
                    response_line = sanitize_summary_value(&response);
                }
                Err(error) => {
                    send_result = "SendFailed".to_string();
                    wrapper_error = sanitize_summary_value(&error);
                    exit_reason = "WrapperSendFailed".to_string();
                }
            }
        }
        Some(_) => {
            clear_four_view_operator_wrapper_quit_guard(guard_state);
            send_result = "Ignored".to_string();
            wrapper_error = "unknown_key".to_string();
        }
        None => {
            clear_four_view_operator_wrapper_quit_guard(guard_state);
            send_result = "Ignored".to_string();
            wrapper_error = "unknown_key".to_string();
        }
    }

    FourViewOperatorWrapperKeySummary {
        key_index,
        wrapper_key: if wrapper_key.is_empty() {
            "empty".to_string()
        } else {
            wrapper_key
        },
        mapped_command,
        guard_state: format_four_view_operator_wrapper_guard_state(guard_state),
        send_result,
        response_line,
        command_parse_error: sanitize_summary_value(&command_parse_error),
        wrapper_error,
        exit_reason: sanitize_summary_value(&exit_reason),
    }
}

fn parse_four_view_operator_wrapper_key(raw_key: &str) -> Option<char> {
    let trimmed = raw_key.trim();
    if trimmed.chars().count() != 1 {
        return None;
    }
    trimmed.chars().next()
}

fn clear_four_view_operator_wrapper_quit_guard(
    guard_state: &mut FourViewOperatorWrapperGuardState,
) {
    guard_state.quit_armed = false;
    guard_state.armed_until_millis = None;
}

fn clear_expired_four_view_operator_wrapper_quit_guard(
    guard_state: &mut FourViewOperatorWrapperGuardState,
    now_millis: u64,
) {
    if guard_state.quit_armed
        && guard_state
            .armed_until_millis
            .map(|armed_until| now_millis >= armed_until)
            .unwrap_or(false)
    {
        clear_four_view_operator_wrapper_quit_guard(guard_state);
    }
}

fn format_four_view_operator_wrapper_guard_state(
    guard_state: &FourViewOperatorWrapperGuardState,
) -> String {
    if guard_state.quit_armed {
        "quit_armed=true".to_string()
    } else {
        "quit_armed=false".to_string()
    }
}

fn extract_summary_field(summary: &str, field_name: &str) -> Option<String> {
    let prefix = format!("{field_name}=");
    summary
        .split_whitespace()
        .find_map(|part| part.strip_prefix(&prefix).map(ToString::to_string))
}

fn normalize_four_view_operator_wrapper_exit_reason(value: Option<&str>) -> String {
    match value {
        Some("none") | None => "Continue".to_string(),
        Some(value) => value.to_string(),
    }
}

trait FourViewControlPipeClientRuntime {
    fn send_command(
        &mut self,
        pipe_name: &str,
        command: &str,
        connect_timeout_millis: u32,
    ) -> io::Result<String>;
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct SwitcherFourViewControlNamedPipeClientRuntime;

#[cfg(target_os = "windows")]
impl FourViewControlPipeClientRuntime for SwitcherFourViewControlNamedPipeClientRuntime {
    fn send_command(
        &mut self,
        pipe_name: &str,
        command: &str,
        connect_timeout_millis: u32,
    ) -> io::Result<String> {
        let mut pipe = open_control_named_pipe_client(pipe_name, connect_timeout_millis)?;
        write_length_prefixed_utf8_message(&mut pipe, command)?;
        read_length_prefixed_utf8_message(&mut pipe)
    }
}

fn run_send_control_command_with_runtime(
    pipe_name: &str,
    command: &str,
    runtime: &mut impl FourViewControlPipeClientRuntime,
) -> Result<String, String> {
    if pipe_name.trim().is_empty() {
        return Err("control pipe name must not be empty".to_string());
    }
    runtime
        .send_command(
            pipe_name,
            command,
            DEFAULT_CONTROL_PIPE_CONNECT_TIMEOUT_MILLIS,
        )
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn run_send_control_command(pipe_name: &str, command: &str) -> Result<String, String> {
    run_send_control_command_with_runtime(
        pipe_name,
        command,
        &mut SwitcherFourViewControlNamedPipeClientRuntime,
    )
}

#[cfg(not(target_os = "windows"))]
fn run_send_control_command(_pipe_name: &str, _command: &str) -> Result<String, String> {
    Err("send-control-command is only available on Windows".to_string())
}

#[cfg(target_os = "windows")]
fn run_control_pipe_command_round_trip<T>(
    pipe_name: &str,
    handler: impl FnOnce(String) -> (String, T, String),
) -> io::Result<(String, T)> {
    let mut pipe = create_control_named_pipe_server_connection(pipe_name)?;
    connect_control_named_pipe_server(&pipe)?;
    let command = read_length_prefixed_utf8_message(&mut pipe)?;
    let (command_text, output, response) = handler(command);
    write_length_prefixed_utf8_message(&mut pipe, &response)?;
    Ok((command_text, output))
}

#[cfg(not(target_os = "windows"))]
fn run_control_pipe_command_round_trip<T>(
    _pipe_name: &str,
    _handler: impl FnOnce(String) -> (String, T, String),
) -> io::Result<(String, T)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "control pipe is only available on Windows",
    ))
}

#[cfg(target_os = "windows")]
fn create_control_named_pipe_server_connection(pipe_name: &str) -> io::Result<File> {
    let pipe_path = control_named_pipe_path(pipe_name)?;
    let wide_name: Vec<u16> = pipe_path.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(wide_name.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            16 * 1024,
            16 * 1024,
            0,
            None,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let owned = unsafe { OwnedHandle::from_raw_handle(handle.0 as *mut _) };
    Ok(File::from(owned))
}

#[cfg(target_os = "windows")]
fn connect_control_named_pipe_server(pipe: &File) -> io::Result<()> {
    let handle = HANDLE(pipe.as_raw_handle() as *mut _);
    let connected = unsafe { ConnectNamedPipe(handle, None) };
    if connected.is_ok() {
        return Ok(());
    }

    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_PIPE_CONNECTED.0 as i32) {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(target_os = "windows")]
fn open_control_named_pipe_client(pipe_name: &str, timeout_millis: u32) -> io::Result<File> {
    let pipe_path = control_named_pipe_path(pipe_name)?;
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

#[cfg(target_os = "windows")]
fn write_length_prefixed_utf8_message(writer: &mut impl Write, message: &str) -> io::Result<()> {
    let body = message.as_bytes();
    let body_len = u32::try_from(body.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "control pipe message exceeds u32 length prefix",
        )
    })?;
    writer.write_all(&body_len.to_le_bytes())?;
    writer.write_all(body)?;
    writer.flush()
}

#[cfg(target_os = "windows")]
fn read_length_prefixed_utf8_message(reader: &mut impl Read) -> io::Result<String> {
    let mut prefix = [0u8; 4];
    reader.read_exact(&mut prefix)?;
    let body_len = u32::from_le_bytes(prefix) as usize;
    let mut body = vec![0u8; body_len];
    reader.read_exact(&mut body)?;
    String::from_utf8(body).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(target_os = "windows")]
fn control_named_pipe_path(pipe_name: &str) -> io::Result<String> {
    if pipe_name.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "control pipe name must not be empty",
        ));
    }

    Ok(format!(r"\\.\pipe\{pipe_name}"))
}

fn format_four_view_control_pipe_command_response(
    command: &str,
    summary: &SwitcherFourViewControlledPreviewCommandSummary,
) -> String {
    format!(
        "switcher four-view control response command={} transition_result={} current_view_state={} view_render_mode={} output_layout={} rendered_slot_count={} focused_slot_index={} selected_slot_result={} clean_output_render_result_kind={} all_view_render_result_kind={} command_parse_error={} exit_reason={}",
        sanitize_summary_value(command),
        summary.transition_result,
        format_four_view_controlled_preview_view_state(&summary.current_view_state),
        summary.view_render_mode,
        summary.output_layout,
        summary.rendered_slot_count,
        format_optional_usize(summary.focused_slot_index),
        summary.selected_slot_result,
        summary.clean_output_render_result_kind,
        summary.all_view_render_result_kind,
        sanitize_summary_value(&summary.command_parse_error),
        summary.exit_reason,
    )
}

fn format_four_view_controlled_preview_view_state(
    view_state: &SwitcherFourViewControlledPreviewViewState,
) -> String {
    match view_state {
        SwitcherFourViewControlledPreviewViewState::AllView => "AllView".to_string(),
        SwitcherFourViewControlledPreviewViewState::Focused(slot_index) => {
            format!("Focused({slot_index})")
        }
    }
}

fn four_view_clean_output_window_loop_frame_cadence() -> Duration {
    Duration::from_secs_f64(1.0 / 30.0)
}

fn render_four_view_focused_slot_with_runtime(
    slot: &SwitcherFourViewHandoffQuadCompositionRenderSlot,
    render_runtime: &impl SwitcherWindowRenderRuntimeHook,
    window_title: &str,
) -> &'static str {
    let decoded = match slot {
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame { frame, .. }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            frame, ..
        } => match frame.decoded.as_ref() {
            Some(decoded) => decoded,
            None => return "NoRenderableFocusedView",
        },
        _ => return "NoRenderableFocusedView",
    };

    let render_input = match SwitcherDecodedFrameRenderInput::from_decoded_frame(decoded) {
        Ok(render_input) => render_input,
        Err(_) => return "InvalidFrame",
    };
    let (scaled_input, _scaled_diagnostics) =
        scale_four_view_bgra_render_input_to_obs_validation_profile(
            &render_input,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
        );

    format_focused_window_render_result_kind(&render_runtime.render_once(
        SwitcherWindowRenderRequest {
            frame: scaled_input,
            title: window_title.to_string(),
            hold_millis: 0,
        },
    ))
}

fn run_four_view_real_handoff_preview_validation_with_runtime_and_handoff(
    handoff: &mut impl SwitcherQueuedFrameHandoff,
    slots: [SwitcherFourViewTargetTimeSourceSlotConfig; 4],
    target_timestamp: TimestampMicros,
    read_mode: stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode,
    decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
) -> SwitcherFourViewHandoffValidationOutput {
    SwitcherFourViewHandoffValidationBoundary::default().run_from_handoff_with_runtimes_and_mode(
        handoff,
        SwitcherFourViewHandoffValidationInput {
            slots,
            target_timestamp,
            previous_slots: [None, None, None, None],
            display_current_time: TimestampMicros(target_timestamp.0.saturating_add(1)),
            layout_policy: SwitcherFourViewQuadLayoutPolicy::default(),
            composed_window_title: "StreamSync 4-view".to_string(),
            composed_render_hold_millis: 0,
        },
        read_mode,
        decode_runtime,
        &SwitcherUnavailableWindowRenderRuntimeHook,
    )
}

#[allow(dead_code)]
fn run_four_view_real_handoff_preview_validation_with_runtime_and_handoff_and_previous_slots(
    handoff: &mut impl SwitcherQueuedFrameHandoff,
    slots: [SwitcherFourViewTargetTimeSourceSlotConfig; 4],
    target_timestamp: TimestampMicros,
    previous_slots: [Option<SwitcherFourViewDisplayedSlot>; 4],
    read_mode: stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode,
    decode_runtime: &impl SwitcherH264DecodeRuntimeHook,
) -> SwitcherFourViewHandoffValidationOutput {
    SwitcherFourViewHandoffValidationBoundary::default().run_from_handoff_with_runtimes_and_mode(
        handoff,
        SwitcherFourViewHandoffValidationInput {
            slots,
            target_timestamp,
            previous_slots,
            display_current_time: TimestampMicros(target_timestamp.0.saturating_add(1)),
            layout_policy: SwitcherFourViewQuadLayoutPolicy::default(),
            composed_window_title: "StreamSync 4-view".to_string(),
            composed_render_hold_millis: 0,
        },
        read_mode,
        decode_runtime,
        &SwitcherUnavailableWindowRenderRuntimeHook,
    )
}

fn update_four_view_previous_slots_from_validation(
    current_previous_slots: &[Option<SwitcherFourViewDisplayedSlot>; 4],
    validation: &SwitcherFourViewHandoffValidationOutput,
) -> [Option<SwitcherFourViewDisplayedSlot>; 4] {
    update_four_view_previous_slots_from_composition_render(
        current_previous_slots,
        &validation.composition_render,
    )
}

fn update_four_view_previous_slots_from_composition_render(
    current_previous_slots: &[Option<SwitcherFourViewDisplayedSlot>; 4],
    composition_render: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderConnectionOutput,
) -> [Option<SwitcherFourViewDisplayedSlot>; 4] {
    std::array::from_fn(|index| match &composition_render.composition.slots[index] {
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame { frame, .. }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            frame, ..
        } if frame.decoded.is_some() => Some(frame.clone()),
        _ => current_previous_slots[index].clone(),
    })
}

fn compose_two_real_preview_quad_view(
    pre_composition: &SwitcherFourViewHandoffValidationPreCompositionOutput,
    layout_policy: SwitcherFourViewQuadLayoutPolicy,
    timing: &Rc<RefCell<TwoRealPreviewLoopRuntimeTiming>>,
    cache: &mut TwoRealPreviewLoopQuadCompositionCache,
    previous_visual_identities: Option<&[TwoRealPreviewLoopSlotVisualIdentity; 4]>,
    current_visual_identities: &[TwoRealPreviewLoopSlotVisualIdentity; 4],
    quad_view_compose_attempt_count: &mut u32,
    quad_view_compose_success_count: &mut u32,
    quad_view_incremental_update_count: &mut u32,
    quad_view_full_compose_count: &mut u32,
    quad_view_changed_slot_update_count: &mut u32,
    quad_view_reused_slot_count: &mut u32,
) -> SwitcherFourViewQuadCompositionOutput {
    *quad_view_compose_attempt_count += 1;
    let compose_start = Instant::now();
    let (composition, update_diagnostics) = compose_two_real_preview_quad_view_incremental(
        &pre_composition.composition_render,
        layout_policy,
        cache,
        previous_visual_identities,
        current_visual_identities,
    );
    let compose_elapsed_ms = compose_start.elapsed().as_millis();
    {
        let mut timing = timing.borrow_mut();
        timing.quad_view_compose_elapsed_ms += compose_elapsed_ms;
        if update_diagnostics.incremental_update {
            timing.quad_view_incremental_update_elapsed_ms += compose_elapsed_ms;
        }
    }
    if matches!(
        composition,
        SwitcherFourViewQuadCompositionResult::ComposedFrame { .. }
    ) {
        *quad_view_compose_success_count += 1;
        timing.borrow_mut().composed_buffer_clone_count += 1;
    }
    if update_diagnostics.incremental_update {
        *quad_view_incremental_update_count += 1;
    }
    if update_diagnostics.full_compose {
        *quad_view_full_compose_count += 1;
    }
    *quad_view_changed_slot_update_count += update_diagnostics.changed_slot_count;
    *quad_view_reused_slot_count += update_diagnostics.reused_slot_count;
    let output = SwitcherFourViewQuadCompositionOutput {
        scheduler_status: pre_composition.scheduler.status,
        connection: pre_composition.composition_render.clone(),
        composition,
    };
    output
}

fn compose_two_real_preview_quad_view_incremental(
    connection: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderConnectionOutput,
    policy: SwitcherFourViewQuadLayoutPolicy,
    cache: &mut TwoRealPreviewLoopQuadCompositionCache,
    previous_visual_identities: Option<&[TwoRealPreviewLoopSlotVisualIdentity; 4]>,
    current_visual_identities: &[TwoRealPreviewLoopSlotVisualIdentity; 4],
) -> (
    SwitcherFourViewQuadCompositionResult,
    TwoRealPreviewLoopQuadCompositionUpdateDiagnostics,
) {
    let base_slots = connection
        .composition
        .slots
        .clone()
        .map(two_real_quad_composed_slot_metadata_without_rect);

    if let Some(placement) = connection
        .composition
        .slots
        .iter()
        .find_map(two_real_missing_decoded_slot_placement)
    {
        return (
            SwitcherFourViewQuadCompositionResult::InvalidQuadView {
                reason: SwitcherFourViewQuadCompositionInvalidReason::MissingDecodedPixels {
                    placement,
                },
                slots: base_slots,
            },
            TwoRealPreviewLoopQuadCompositionUpdateDiagnostics::default(),
        );
    }

    let Some((slot_width, slot_height)) = two_real_quad_slot_size(&connection.composition.slots)
    else {
        return (
            SwitcherFourViewQuadCompositionResult::NoRenderableQuadView { slots: base_slots },
            TwoRealPreviewLoopQuadCompositionUpdateDiagnostics::default(),
        );
    };

    for slot in &connection.composition.slots {
        if let Err(reason) = validate_two_real_quad_renderable_slot(slot) {
            return (
                SwitcherFourViewQuadCompositionResult::InvalidQuadView {
                    reason,
                    slots: base_slots,
                },
                TwoRealPreviewLoopQuadCompositionUpdateDiagnostics::default(),
            );
        }
    }

    let Some(virtual_canvas_width) = slot_width.checked_mul(2) else {
        return (
            SwitcherFourViewQuadCompositionResult::InvalidQuadView {
                reason: SwitcherFourViewQuadCompositionInvalidReason::CanvasTooLarge,
                slots: base_slots,
            },
            TwoRealPreviewLoopQuadCompositionUpdateDiagnostics::default(),
        );
    };
    let Some(virtual_canvas_height) = slot_height.checked_mul(2) else {
        return (
            SwitcherFourViewQuadCompositionResult::InvalidQuadView {
                reason: SwitcherFourViewQuadCompositionInvalidReason::CanvasTooLarge,
                slots: base_slots,
            },
            TwoRealPreviewLoopQuadCompositionUpdateDiagnostics::default(),
        );
    };
    let output_width = FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH;
    let output_height = FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT;
    let Some(output_len) = output_width
        .checked_mul(output_height)
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|len| len as usize)
    else {
        return (
            SwitcherFourViewQuadCompositionResult::InvalidQuadView {
                reason: SwitcherFourViewQuadCompositionInvalidReason::CanvasTooLarge,
                slots: base_slots,
            },
            TwoRealPreviewLoopQuadCompositionUpdateDiagnostics::default(),
        );
    };

    let changed_slots = two_real_preview_loop_changed_visual_slots(
        previous_visual_identities,
        current_visual_identities,
    );
    let allocated = cache.prepare_canvas(
        output_width,
        output_height,
        output_len,
        policy.placeholder_bgra,
    );
    cache.prepare_placeholder_row(output_width, policy.placeholder_bgra);
    fill_two_real_bgra_rows(
        &mut cache.pixels,
        output_width,
        0,
        0,
        output_width,
        output_height,
        &cache.placeholder_row,
    );

    let output_slot_width = output_width / 2;
    let output_slot_height = output_height / 2;
    let slots = connection.composition.slots.clone().map(|slot| {
        two_real_quad_composed_slot_metadata_with_rect(slot, output_slot_width, output_slot_height)
    });

    let renderable_frames_by_slot: [Option<&SwitcherDecodedFrame>; 4] = connection
        .composition
        .slots
        .each_ref()
        .map(|slot| two_real_renderable_frame_and_placement(slot).map(|(frame, _)| frame));
    let output_stride = output_width as usize * 4;
    let x_mappings = (0..output_width)
        .map(|x| {
            let source_x = x as u64 * virtual_canvas_width as u64 / output_width as u64;
            (
                (source_x / slot_width as u64) as usize,
                (source_x % slot_width as u64) as u32,
            )
        })
        .collect::<Vec<_>>();
    let y_mappings = (0..output_height)
        .map(|y| {
            let source_y = y as u64 * virtual_canvas_height as u64 / output_height as u64;
            (
                (source_y / slot_height as u64) as usize,
                (source_y % slot_height as u64) as u32,
            )
        })
        .collect::<Vec<_>>();

    for (dst_y, (slot_row, local_y)) in y_mappings.iter().copied().enumerate() {
        let row_start = dst_y * output_stride;
        for (dst_x, (slot_column, local_x)) in x_mappings.iter().copied().enumerate() {
            let slot_index = slot_row * 2 + slot_column;
            let Some(frame) = renderable_frames_by_slot[slot_index] else {
                continue;
            };
            if local_x >= frame.width || local_y >= frame.height {
                continue;
            }

            let source_index = ((local_y as usize * frame.width as usize) + local_x as usize) * 4;
            let destination_index = row_start + dst_x * 4;
            cache.pixels[destination_index..destination_index + 4]
                .copy_from_slice(&frame.pixels[source_index..source_index + 4]);
        }
    }

    let changed_slot_count = changed_slots.iter().filter(|changed| **changed).count() as u32;
    let reused_slot_count = 4u32.saturating_sub(changed_slot_count);
    let diagnostics = TwoRealPreviewLoopQuadCompositionUpdateDiagnostics {
        full_compose: true,
        incremental_update: false,
        changed_slot_count,
        reused_slot_count,
        allocation_count: u32::from(allocated),
    };

    (
        SwitcherFourViewQuadCompositionResult::ComposedFrame {
            frame: SwitcherFourViewComposedFrame {
                width: output_width,
                height: output_height,
                pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                pixels: cache.pixels.clone(),
                slots,
            },
        },
        diagnostics,
    )
}

fn two_real_preview_loop_changed_visual_slots(
    previous_visual_identities: Option<&[TwoRealPreviewLoopSlotVisualIdentity; 4]>,
    current_visual_identities: &[TwoRealPreviewLoopSlotVisualIdentity; 4],
) -> [bool; 4] {
    std::array::from_fn(|index| {
        previous_visual_identities
            .map(|previous| {
                !two_real_preview_loop_slot_visual_identity_render_eq(
                    &previous[index],
                    &current_visual_identities[index],
                )
            })
            .unwrap_or(true)
    })
}

fn two_real_quad_composed_slot_metadata_without_rect(
    slot: SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> SwitcherFourViewQuadComposedSlotMetadata {
    two_real_quad_composed_slot_metadata(slot, None)
}

fn two_real_quad_composed_slot_metadata_with_rect(
    slot: SwitcherFourViewHandoffQuadCompositionRenderSlot,
    slot_width: u32,
    slot_height: u32,
) -> SwitcherFourViewQuadComposedSlotMetadata {
    let placement = two_real_quad_connected_slot_placement(&slot);
    let rect = Some(two_real_quad_slot_rect(placement, slot_width, slot_height));
    two_real_quad_composed_slot_metadata(slot, rect)
}

fn two_real_quad_composed_slot_metadata(
    slot: SwitcherFourViewHandoffQuadCompositionRenderSlot,
    rect: Option<SwitcherFourViewQuadComposedSlotRect>,
) -> SwitcherFourViewQuadComposedSlotMetadata {
    let placement = two_real_quad_connected_slot_placement(&slot);
    let kind = match slot {
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame {
            frame,
            selected,
            consumed,
            ..
        } => SwitcherFourViewQuadComposedSlotKind::Updated {
            frame,
            selected,
            consumed,
        },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            frame,
            skipped,
            hold_duration_micros,
            ..
        } => SwitcherFourViewQuadComposedSlotKind::HeldPrevious {
            frame,
            skipped,
            hold_duration_micros,
        },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrameWithoutDecoded {
            frame,
            skipped,
            hold_duration_micros,
            ..
        } => SwitcherFourViewQuadComposedSlotKind::MissingDecodedPixels {
            frame,
            skipped,
            hold_duration_micros,
        },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseNoDisplayPlaceholder {
            skipped,
            ..
        } => SwitcherFourViewQuadComposedSlotKind::NoDisplayPlaceholder { skipped },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseSourceErrorPlaceholder {
            skipped,
            ..
        } => SwitcherFourViewQuadComposedSlotKind::SourceErrorPlaceholder { skipped },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeDeferredPlaceholder {
            selected,
            consumed,
            reason,
            ..
        } => SwitcherFourViewQuadComposedSlotKind::DecodeDeferredPlaceholder {
            selected,
            consumed,
            reason,
        },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
            selected,
            consumed,
            failure,
            ..
        } => SwitcherFourViewQuadComposedSlotKind::DecodeFailedPlaceholder {
            selected,
            consumed,
            failure,
        },
    };

    SwitcherFourViewQuadComposedSlotMetadata {
        placement,
        rect,
        kind,
    }
}

fn two_real_quad_connected_slot_placement(
    slot: &SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> stream_sync_switcher::SwitcherFourViewQuadSlotPlacement {
    match slot {
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame { placement, .. }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            placement,
            ..
        }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrameWithoutDecoded {
            placement,
            ..
        }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseNoDisplayPlaceholder {
            placement,
            ..
        }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseSourceErrorPlaceholder {
            placement,
            ..
        }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeDeferredPlaceholder {
            placement,
            ..
        }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
            placement,
            ..
        } => *placement,
    }
}

fn two_real_missing_decoded_slot_placement(
    slot: &SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> Option<stream_sync_switcher::SwitcherFourViewQuadSlotPlacement> {
    match slot {
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrameWithoutDecoded {
            placement,
            ..
        } => Some(*placement),
        _ => None,
    }
}

fn two_real_quad_slot_rect(
    placement: stream_sync_switcher::SwitcherFourViewQuadSlotPlacement,
    slot_width: u32,
    slot_height: u32,
) -> SwitcherFourViewQuadComposedSlotRect {
    SwitcherFourViewQuadComposedSlotRect {
        placement,
        x: placement.column as u32 * slot_width,
        y: placement.row as u32 * slot_height,
        width: slot_width,
        height: slot_height,
    }
}

fn two_real_renderable_frame_and_placement(
    slot: &SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> Option<(
    &SwitcherDecodedFrame,
    stream_sync_switcher::SwitcherFourViewQuadSlotPlacement,
)> {
    match slot {
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame {
            placement,
            frame,
            ..
        }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            placement,
            frame,
            ..
        } => frame.decoded.as_ref().map(|decoded| (decoded, *placement)),
        _ => None,
    }
}

fn two_real_quad_slot_size(
    slots: &[SwitcherFourViewHandoffQuadCompositionRenderSlot; 4],
) -> Option<(u32, u32)> {
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;

    for slot in slots {
        let Some((frame, _)) = two_real_renderable_frame_and_placement(slot) else {
            continue;
        };
        width = Some(width.map_or(frame.width, |current| current.max(frame.width)));
        height = Some(height.map_or(frame.height, |current| current.max(frame.height)));
    }

    Some((width?, height?))
}

fn validate_two_real_quad_renderable_slot(
    slot: &SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> Result<(), SwitcherFourViewQuadCompositionInvalidReason> {
    let Some((frame, placement)) = two_real_renderable_frame_and_placement(slot) else {
        return Ok(());
    };

    if frame.pixel_format != SwitcherDecodedFramePixelFormat::Bgra8 {
        return Err(
            SwitcherFourViewQuadCompositionInvalidReason::UnsupportedPixelFormat {
                placement,
                actual: frame.pixel_format,
            },
        );
    }
    if frame.width == 0 || frame.height == 0 {
        return Err(SwitcherFourViewQuadCompositionInvalidReason::InvalidDimensions { placement });
    }
    let Some(expected) = frame
        .width
        .checked_mul(frame.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|len| len as usize)
    else {
        return Err(SwitcherFourViewQuadCompositionInvalidReason::CanvasTooLarge);
    };
    if frame.pixels.len() != expected {
        return Err(
            SwitcherFourViewQuadCompositionInvalidReason::InvalidBufferLength {
                placement,
                expected,
                actual: frame.pixels.len(),
            },
        );
    }

    Ok(())
}

fn fill_two_real_bgra(pixels: &mut [u8], color: [u8; 4]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&color);
    }
}

fn fill_two_real_bgra_rows(
    canvas: &mut [u8],
    canvas_width: u32,
    dst_x: u32,
    dst_y: u32,
    width: u32,
    height: u32,
    placeholder_row: &[u8],
) {
    let canvas_stride = canvas_width as usize * 4;
    let row_len = width as usize * 4;
    let placeholder_row = &placeholder_row[..row_len];
    for row in 0..height as usize {
        let dst_start = (dst_y as usize + row) * canvas_stride + dst_x as usize * 4;
        let dst_end = dst_start + row_len;
        canvas[dst_start..dst_end].copy_from_slice(placeholder_row);
    }
}

fn update_two_real_decoded_slot_identities(
    current_identities: &[Option<TwoRealPreviewLoopDecodedSlotIdentity>; 4],
    observations: &[Option<FourViewPreviewSlotHandoffObservation>; 4],
    pre_composition: &SwitcherFourViewHandoffValidationPreCompositionOutput,
) -> [Option<TwoRealPreviewLoopDecodedSlotIdentity>; 4] {
    std::array::from_fn(|index| {
        match &pre_composition.composition_render.composition.slots[index] {
            SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame { frame, .. }
                if frame.decoded.is_some() =>
            {
                two_real_decoded_slot_identity(index, observations, pre_composition)
                    .or_else(|| current_identities[index].clone())
            }
            SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
                frame,
                ..
            } if frame.decoded.is_some() => current_identities[index].clone(),
            _ => current_identities[index].clone(),
        }
    })
}

fn two_real_decoded_slot_identity(
    slot_index: usize,
    observations: &[Option<FourViewPreviewSlotHandoffObservation>; 4],
    pre_composition: &SwitcherFourViewHandoffValidationPreCompositionOutput,
) -> Option<TwoRealPreviewLoopDecodedSlotIdentity> {
    let selected = match &pre_composition.scheduler.slots[slot_index].result {
        SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(selected) => selected,
        _ => return None,
    };
    Some(TwoRealPreviewLoopDecodedSlotIdentity {
        client_id: selected.frame.client_id.clone(),
        run_id: selected.frame.run_id.clone(),
        frame_id: selected.frame.frame_id,
        decodable_source: observations[slot_index]
            .as_ref()
            .and_then(two_real_observation_decodable_source),
    })
}

fn two_real_observation_decodable_source(
    observation: &FourViewPreviewSlotHandoffObservation,
) -> Option<String> {
    let response = observation
        .request_output
        .as_ref()
        .and_then(|output| output.runtime.as_ref())
        .and_then(|runtime| runtime.response.as_ref());
    match response {
        Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
            decodable_source,
            ..
        })
        | Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
            decodable_source,
            ..
        }) => Some(format_handoff_decodable_source(*decodable_source).to_string()),
        _ => None,
    }
}

fn two_real_preview_loop_visual_identities(
    decoded_identities: &[Option<TwoRealPreviewLoopDecodedSlotIdentity>; 4],
    pre_composition: &SwitcherFourViewHandoffValidationPreCompositionOutput,
) -> [TwoRealPreviewLoopSlotVisualIdentity; 4] {
    std::array::from_fn(|index| {
        two_real_preview_loop_slot_visual_identity(
            decoded_identities[index].as_ref(),
            &pre_composition.composition_render.composition.slots[index],
        )
    })
}

fn two_real_preview_loop_slot_visual_identity(
    decoded_identity: Option<&TwoRealPreviewLoopDecodedSlotIdentity>,
    slot: &SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> TwoRealPreviewLoopSlotVisualIdentity {
    match slot {
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame { frame, .. }
        | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            frame, ..
        } => two_real_preview_loop_source_frame_visual_identity(frame, decoded_identity),
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrameWithoutDecoded {
            frame,
            ..
        } => {
            let (client_id, run_id, frame_id, selected_frame_source) =
                two_real_preview_loop_displayed_frame_identity_parts(frame, decoded_identity);
            TwoRealPreviewLoopSlotVisualIdentity::MissingDecodedPixels {
                client_id,
                run_id,
                frame_id,
                selected_frame_source,
            }
        }
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseNoDisplayPlaceholder {
            skipped,
            ..
        } => TwoRealPreviewLoopSlotVisualIdentity::NoDisplayPlaceholder {
            reason: format_four_view_skipped_instruction_reason(skipped).to_string(),
        },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseSourceErrorPlaceholder {
            skipped,
            ..
        } => TwoRealPreviewLoopSlotVisualIdentity::SourceErrorPlaceholder {
            reason: format_four_view_skipped_instruction_reason(skipped).to_string(),
        },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeDeferredPlaceholder {
            selected,
            reason,
            ..
        } => TwoRealPreviewLoopSlotVisualIdentity::DecodeDeferredPlaceholder {
            client_id: selected.frame.client_id.clone(),
            run_id: selected.frame.run_id.clone(),
            frame_id: selected.frame.frame_id,
            selected_frame_source: decoded_identity
                .and_then(|identity| identity.decodable_source.clone()),
            reason: format_switcher_h264_decode_deferred_reason(*reason).to_string(),
        },
        SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
            selected,
            failure,
            ..
        } => TwoRealPreviewLoopSlotVisualIdentity::DecodeFailedPlaceholder {
            client_id: selected.frame.client_id.clone(),
            run_id: selected.frame.run_id.clone(),
            frame_id: selected.frame.frame_id,
            selected_frame_source: decoded_identity
                .and_then(|identity| identity.decodable_source.clone()),
            failure: failure.message.clone(),
        },
    }
}

fn two_real_preview_loop_source_frame_visual_identity(
    frame: &SwitcherFourViewDisplayedSlot,
    decoded_identity: Option<&TwoRealPreviewLoopDecodedSlotIdentity>,
) -> TwoRealPreviewLoopSlotVisualIdentity {
    let (client_id, run_id, frame_id, selected_frame_source) =
        two_real_preview_loop_displayed_frame_identity_parts(frame, decoded_identity);
    TwoRealPreviewLoopSlotVisualIdentity::SourceFrame {
        client_id,
        run_id,
        frame_id,
        selected_frame_source,
    }
}

fn two_real_preview_loop_displayed_frame_identity_parts(
    frame: &SwitcherFourViewDisplayedSlot,
    decoded_identity: Option<&TwoRealPreviewLoopDecodedSlotIdentity>,
) -> (ClientId, RunId, u64, Option<String>) {
    if let Some(identity) = decoded_identity {
        return (
            identity.client_id.clone(),
            identity.run_id.clone(),
            identity.frame_id,
            identity.decodable_source.clone(),
        );
    }

    if let Some(selected) = &frame.selected {
        return (
            selected.frame.client_id.clone(),
            selected.frame.run_id.clone(),
            selected.frame.frame_id,
            None,
        );
    }

    (
        ClientId("unknown-client".to_string()),
        RunId("unknown-run".to_string()),
        0,
        None,
    )
}

fn two_real_preview_loop_visual_change_diagnostics(
    previous_visual_identities: Option<&[TwoRealPreviewLoopSlotVisualIdentity; 4]>,
    current_visual_identities: &[TwoRealPreviewLoopSlotVisualIdentity; 4],
) -> TwoRealPreviewLoopVisualChangeDiagnostics {
    let Some(previous_visual_identities) = previous_visual_identities else {
        return TwoRealPreviewLoopVisualChangeDiagnostics::default();
    };

    let mut diagnostics = TwoRealPreviewLoopVisualChangeDiagnostics::default();
    for slot_index in 0..4 {
        let previous = &previous_visual_identities[slot_index];
        let current = &current_visual_identities[slot_index];
        if previous == current {
            continue;
        }

        let previous_frame_id = two_real_preview_loop_visual_identity_frame_id(previous);
        let current_frame_id = two_real_preview_loop_visual_identity_frame_id(current);
        if previous_frame_id.is_some()
            && current_frame_id.is_some()
            && previous_frame_id != current_frame_id
        {
            diagnostics.frame_id_changed_counts[slot_index] = 1;
        }

        let previous_selected_source =
            two_real_preview_loop_visual_identity_selected_source(previous);
        let current_selected_source =
            two_real_preview_loop_visual_identity_selected_source(current);
        if previous_selected_source != current_selected_source
            && (previous_selected_source.is_some() || current_selected_source.is_some())
        {
            diagnostics.selected_source_changed_counts[slot_index] = 1;
        }

        if !two_real_preview_loop_slot_visual_identity_render_eq(previous, current)
            && (two_real_preview_loop_visual_identity_is_placeholder(previous)
                || two_real_preview_loop_visual_identity_is_placeholder(current))
        {
            diagnostics.placeholder_visual_changed_count = diagnostics
                .placeholder_visual_changed_count
                .saturating_add(1);
        }
    }

    diagnostics
}

fn two_real_preview_loop_visual_identity_frame_id(
    identity: &TwoRealPreviewLoopSlotVisualIdentity,
) -> Option<u64> {
    match identity {
        TwoRealPreviewLoopSlotVisualIdentity::SourceFrame { frame_id, .. }
        | TwoRealPreviewLoopSlotVisualIdentity::DecodeDeferredPlaceholder { frame_id, .. }
        | TwoRealPreviewLoopSlotVisualIdentity::DecodeFailedPlaceholder { frame_id, .. }
        | TwoRealPreviewLoopSlotVisualIdentity::MissingDecodedPixels { frame_id, .. } => {
            Some(*frame_id)
        }
        TwoRealPreviewLoopSlotVisualIdentity::NoDisplayPlaceholder { .. }
        | TwoRealPreviewLoopSlotVisualIdentity::SourceErrorPlaceholder { .. } => None,
    }
}

fn two_real_preview_loop_visual_identities_render_equal(
    previous: &[TwoRealPreviewLoopSlotVisualIdentity; 4],
    current: &[TwoRealPreviewLoopSlotVisualIdentity; 4],
) -> bool {
    previous
        .iter()
        .zip(current.iter())
        .all(|(previous, current)| {
            two_real_preview_loop_slot_visual_identity_render_eq(previous, current)
        })
}

fn two_real_preview_loop_slot_visual_identity_render_eq(
    previous: &TwoRealPreviewLoopSlotVisualIdentity,
    current: &TwoRealPreviewLoopSlotVisualIdentity,
) -> bool {
    match (previous, current) {
        (
            TwoRealPreviewLoopSlotVisualIdentity::SourceFrame {
                client_id: previous_client_id,
                run_id: previous_run_id,
                frame_id: previous_frame_id,
                ..
            },
            TwoRealPreviewLoopSlotVisualIdentity::SourceFrame {
                client_id: current_client_id,
                run_id: current_run_id,
                frame_id: current_frame_id,
                ..
            },
        ) => {
            previous_client_id == current_client_id
                && previous_run_id == current_run_id
                && previous_frame_id == current_frame_id
        }
        (
            TwoRealPreviewLoopSlotVisualIdentity::NoDisplayPlaceholder {
                reason: previous_reason,
            },
            TwoRealPreviewLoopSlotVisualIdentity::NoDisplayPlaceholder {
                reason: current_reason,
            },
        )
        | (
            TwoRealPreviewLoopSlotVisualIdentity::SourceErrorPlaceholder {
                reason: previous_reason,
            },
            TwoRealPreviewLoopSlotVisualIdentity::SourceErrorPlaceholder {
                reason: current_reason,
            },
        ) => previous_reason == current_reason,
        (
            TwoRealPreviewLoopSlotVisualIdentity::DecodeDeferredPlaceholder {
                client_id: previous_client_id,
                run_id: previous_run_id,
                frame_id: previous_frame_id,
                reason: previous_reason,
                ..
            },
            TwoRealPreviewLoopSlotVisualIdentity::DecodeDeferredPlaceholder {
                client_id: current_client_id,
                run_id: current_run_id,
                frame_id: current_frame_id,
                reason: current_reason,
                ..
            },
        ) => {
            previous_client_id == current_client_id
                && previous_run_id == current_run_id
                && previous_frame_id == current_frame_id
                && previous_reason == current_reason
        }
        (
            TwoRealPreviewLoopSlotVisualIdentity::DecodeFailedPlaceholder {
                client_id: previous_client_id,
                run_id: previous_run_id,
                frame_id: previous_frame_id,
                failure: previous_failure,
                ..
            },
            TwoRealPreviewLoopSlotVisualIdentity::DecodeFailedPlaceholder {
                client_id: current_client_id,
                run_id: current_run_id,
                frame_id: current_frame_id,
                failure: current_failure,
                ..
            },
        ) => {
            previous_client_id == current_client_id
                && previous_run_id == current_run_id
                && previous_frame_id == current_frame_id
                && previous_failure == current_failure
        }
        (
            TwoRealPreviewLoopSlotVisualIdentity::MissingDecodedPixels {
                client_id: previous_client_id,
                run_id: previous_run_id,
                frame_id: previous_frame_id,
                ..
            },
            TwoRealPreviewLoopSlotVisualIdentity::MissingDecodedPixels {
                client_id: current_client_id,
                run_id: current_run_id,
                frame_id: current_frame_id,
                ..
            },
        ) => {
            previous_client_id == current_client_id
                && previous_run_id == current_run_id
                && previous_frame_id == current_frame_id
        }
        _ => false,
    }
}

fn two_real_preview_loop_visual_identity_selected_source(
    identity: &TwoRealPreviewLoopSlotVisualIdentity,
) -> Option<&str> {
    match identity {
        TwoRealPreviewLoopSlotVisualIdentity::SourceFrame {
            selected_frame_source,
            ..
        }
        | TwoRealPreviewLoopSlotVisualIdentity::DecodeDeferredPlaceholder {
            selected_frame_source,
            ..
        }
        | TwoRealPreviewLoopSlotVisualIdentity::DecodeFailedPlaceholder {
            selected_frame_source,
            ..
        }
        | TwoRealPreviewLoopSlotVisualIdentity::MissingDecodedPixels {
            selected_frame_source,
            ..
        } => selected_frame_source.as_deref(),
        TwoRealPreviewLoopSlotVisualIdentity::NoDisplayPlaceholder { .. }
        | TwoRealPreviewLoopSlotVisualIdentity::SourceErrorPlaceholder { .. } => None,
    }
}

fn two_real_preview_loop_visual_identity_is_placeholder(
    identity: &TwoRealPreviewLoopSlotVisualIdentity,
) -> bool {
    !matches!(
        identity,
        TwoRealPreviewLoopSlotVisualIdentity::SourceFrame { .. }
    )
}

fn classify_two_real_preview_loop_materialization_reason(
    frames_attempted_before_increment: u32,
    previous_clean_output: Option<&SwitcherFourViewCleanOutputWindowRenderResult>,
    visual_unchanged: bool,
) -> TwoRealPreviewLoopMaterializationReason {
    if frames_attempted_before_increment == 0 {
        return TwoRealPreviewLoopMaterializationReason::FirstRender;
    }

    let Some(previous_clean_output) = previous_clean_output else {
        return TwoRealPreviewLoopMaterializationReason::PreviousOutputMissing;
    };

    if !visual_unchanged {
        return TwoRealPreviewLoopMaterializationReason::VisualChanged;
    }

    if !clean_output_window_result_was_rendered(previous_clean_output) {
        return TwoRealPreviewLoopMaterializationReason::ForceRender;
    }

    TwoRealPreviewLoopMaterializationReason::Unknown
}

fn two_real_unchanged_frame_reuse_slots(
    previous_identities: &[Option<TwoRealPreviewLoopDecodedSlotIdentity>; 4],
    current_identities: &[Option<TwoRealPreviewLoopDecodedSlotIdentity>; 4],
    pre_composition: &SwitcherFourViewHandoffValidationPreCompositionOutput,
) -> [bool; 4] {
    std::array::from_fn(|index| {
        two_real_preview_loop_decoded_slot_identity_render_eq(
            previous_identities[index].as_ref(),
            current_identities[index].as_ref(),
        ) && matches!(
            &pre_composition.composition_render.composition.slots[index],
            SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame { .. }
        )
    })
}

fn two_real_preview_loop_decoded_slot_identity_render_eq(
    previous: Option<&TwoRealPreviewLoopDecodedSlotIdentity>,
    current: Option<&TwoRealPreviewLoopDecodedSlotIdentity>,
) -> bool {
    matches!(
        (previous, current),
        (Some(previous), Some(current))
            if previous.client_id == current.client_id
                && previous.run_id == current.run_id
                && previous.frame_id == current.frame_id
    )
}

fn two_real_unchanged_frame_reuse_count(unchanged_frame_reuse_slots: &[bool; 4]) -> u32 {
    unchanged_frame_reuse_slots
        .iter()
        .filter(|unchanged| **unchanged)
        .count() as u32
}

fn four_view_two_real_tick_diagnostics(
    slot_result_kinds: &[String; 4],
    validation: &SwitcherFourViewHandoffValidationOutput,
    clean_output: &SwitcherFourViewCleanOutputWindowRenderResult,
) -> TwoRealPreviewLoopTickDiagnostics {
    four_view_two_real_tick_diagnostics_from_composition_render(
        slot_result_kinds,
        &validation.composition_render,
        clean_output,
    )
}

fn four_view_two_real_tick_diagnostics_from_composition_render(
    slot_result_kinds: &[String; 4],
    composition_render: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderConnectionOutput,
    clean_output: &SwitcherFourViewCleanOutputWindowRenderResult,
) -> TwoRealPreviewLoopTickDiagnostics {
    let mut diagnostics = TwoRealPreviewLoopTickDiagnostics::default();

    for kind in slot_result_kinds {
        match kind.as_str() {
            "Selected" => diagnostics.selected_count += 1,
            "NoFrameAvailable" => diagnostics.no_frame_count += 1,
            "HandoffError" => diagnostics.handoff_error_count += 1,
            _ => {}
        }
    }

    for slot in &composition_render.composition.slots {
        match slot {
            SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame { .. } => {
                diagnostics.decode_attempt_count += 1;
                diagnostics.decode_success_count += 1;
            }
            SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeDeferredPlaceholder {
                ..
            }
            | SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
                ..
            } => {
                diagnostics.decode_attempt_count += 1;
            }
            _ => {}
        }
    }

    if let SwitcherFourViewCleanOutputWindowRenderResult::RenderReady { render, .. } = clean_output
    {
        match render {
            SwitcherFourViewComposedCanvasRenderResult::Rendered { .. } => {
                diagnostics.render_success_count += 1;
            }
            SwitcherFourViewComposedCanvasRenderResult::RenderFailed { .. } => {
                diagnostics.render_failure_count += 1;
            }
            _ => {}
        }
    }

    diagnostics
}

fn format_preview_loop_fps(count: u32, elapsed_ms: u128) -> String {
    if elapsed_ms == 0 {
        return "n/a".to_string();
    }
    format!("{:.3}", count as f64 * 1000.0 / elapsed_ms as f64)
}

fn format_preview_loop_average_elapsed(total_elapsed_ms: u128, count: u32) -> String {
    if count == 0 {
        return "n/a".to_string();
    }
    format!("{:.3}", total_elapsed_ms as f64 / count as f64)
}

fn build_two_real_preview_loop_fps_diagnostics(
    elapsed_ms: u128,
    cadence: Duration,
    frames_attempted: u32,
    frames_rendered: u32,
    first_render_attempt_index: Option<u32>,
    first_render_elapsed_ms: Option<u128>,
) -> TwoRealPreviewLoopFpsDiagnostics {
    let target_fps = if cadence.as_nanos() == 0 {
        0
    } else {
        (1_000_000_000u128 / cadence.as_nanos()) as u32
    };
    let configured_frame_interval_ms = cadence.as_millis();
    let no_render_before_first_render = first_render_attempt_index
        .map(|attempt_index| attempt_index.saturating_sub(1))
        .unwrap_or(frames_attempted);
    let rendered_after_first_render = if first_render_attempt_index.is_some() {
        frames_rendered
    } else {
        0
    };
    let elapsed_after_first_render_ms = first_render_elapsed_ms
        .map(|first_elapsed| elapsed_ms.saturating_sub(first_elapsed))
        .unwrap_or(0);

    TwoRealPreviewLoopFpsDiagnostics {
        elapsed_ms,
        target_fps,
        configured_frame_interval_ms,
        effective_attempt_fps: format_preview_loop_fps(frames_attempted, elapsed_ms),
        effective_render_fps: format_preview_loop_fps(frames_rendered, elapsed_ms),
        first_render_attempt_index,
        first_render_elapsed_ms,
        rendered_after_first_render,
        effective_render_fps_after_first_render: format_preview_loop_fps(
            rendered_after_first_render,
            elapsed_after_first_render_ms,
        ),
        no_render_before_first_render,
    }
}

fn default_four_view_real_handoff_preview_slots(
    real_slot_index: usize,
    real_client_id: ClientId,
    real_run_id: RunId,
) -> [SwitcherFourViewTargetTimeSourceSlotConfig; 4] {
    std::array::from_fn(|slot_index| {
        if slot_index == real_slot_index {
            SwitcherFourViewTargetTimeSourceSlotConfig {
                slot_index,
                client_id: real_client_id.clone(),
                run_id: real_run_id.clone(),
            }
        } else {
            SwitcherFourViewTargetTimeSourceSlotConfig {
                slot_index,
                client_id: ClientId(format!("fixture-placeholder-slot-{slot_index}")),
                run_id: RunId(format!("fixture-placeholder-run-{slot_index}")),
            }
        }
    })
}

fn default_four_view_two_real_handoff_preview_slots(
    slot0_index: usize,
    client0_id: ClientId,
    run0_id: RunId,
    slot1_index: usize,
    client1_id: ClientId,
    run1_id: RunId,
) -> [SwitcherFourViewTargetTimeSourceSlotConfig; 4] {
    std::array::from_fn(|slot_index| {
        if slot_index == slot0_index {
            SwitcherFourViewTargetTimeSourceSlotConfig {
                slot_index,
                client_id: client0_id.clone(),
                run_id: run0_id.clone(),
            }
        } else if slot_index == slot1_index {
            SwitcherFourViewTargetTimeSourceSlotConfig {
                slot_index,
                client_id: client1_id.clone(),
                run_id: run1_id.clone(),
            }
        } else {
            SwitcherFourViewTargetTimeSourceSlotConfig {
                slot_index,
                client_id: ClientId(format!("fixture-placeholder-slot-{slot_index}")),
                run_id: RunId(format!("fixture-placeholder-run-{slot_index}")),
            }
        }
    })
}

fn default_four_view_four_real_handoff_preview_slots(
    client0_id: ClientId,
    run0_id: RunId,
    client1_id: ClientId,
    run1_id: RunId,
    client2_id: ClientId,
    run2_id: RunId,
    client3_id: ClientId,
    run3_id: RunId,
) -> [SwitcherFourViewTargetTimeSourceSlotConfig; 4] {
    [
        SwitcherFourViewTargetTimeSourceSlotConfig {
            slot_index: 0,
            client_id: client0_id,
            run_id: run0_id,
        },
        SwitcherFourViewTargetTimeSourceSlotConfig {
            slot_index: 1,
            client_id: client1_id,
            run_id: run1_id,
        },
        SwitcherFourViewTargetTimeSourceSlotConfig {
            slot_index: 2,
            client_id: client2_id,
            run_id: run2_id,
        },
        SwitcherFourViewTargetTimeSourceSlotConfig {
            slot_index: 3,
            client_id: client3_id,
            run_id: run3_id,
        },
    ]
}

fn real_four_view_preview_target_timestamp() -> TimestampMicros {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    TimestampMicros(
        now.as_micros()
            .saturating_add(5_000_000u128)
            .min(u64::MAX as u128) as u64,
    )
}

fn scale_four_view_bgra_render_input_to_obs_validation_profile(
    frame: &stream_sync_switcher::SwitcherDecodedFrameRenderInput,
    output_width: u32,
    output_height: u32,
) -> (
    stream_sync_switcher::SwitcherDecodedFrameRenderInput,
    BgraRenderBufferDiagnostics,
) {
    scale_four_view_bgra_to_obs_validation_profile_from_slice(
        frame.width,
        frame.height,
        frame.pixel_format,
        &frame.pixels,
        output_width,
        output_height,
    )
}

fn scale_four_view_bgra_to_obs_validation_profile_from_slice(
    width: u32,
    height: u32,
    pixel_format: SwitcherDecodedFramePixelFormat,
    pixels_source: &[u8],
    output_width: u32,
    output_height: u32,
) -> (
    stream_sync_switcher::SwitcherDecodedFrameRenderInput,
    BgraRenderBufferDiagnostics,
) {
    let expected_len = output_width as usize * output_height as usize * 4;
    let (mut pixels, mut diagnostics) = take_reusable_obs_render_buffer(expected_len);

    if width == output_width && height == output_height {
        let output_copy_start = Instant::now();
        pixels.copy_from_slice(pixels_source);
        diagnostics.output_copy_elapsed_ms = output_copy_start.elapsed().as_millis();
        diagnostics.bytes_copied_total =
            diagnostics.bytes_copied_total.saturating_add(pixels.len());
        diagnostics.same_size_copy_count = 1;
        return (
            stream_sync_switcher::SwitcherDecodedFrameRenderInput {
                width: output_width,
                height: output_height,
                pixel_format,
                pixels,
            },
            diagnostics,
        );
    }

    if width == output_width.saturating_mul(2) && height == output_height.saturating_mul(2) {
        let scale_loop_start = Instant::now();
        let source_stride = width as usize * 4;
        let destination_stride = output_width as usize * 4;
        for y in 0..output_height as usize {
            let source_row_start = y * 2 * source_stride;
            let destination_row_start = y * destination_stride;
            for x in 0..output_width as usize {
                let source_index = source_row_start + x * 2 * 4;
                let destination_index = destination_row_start + x * 4;
                pixels[destination_index..destination_index + 4]
                    .copy_from_slice(&pixels_source[source_index..source_index + 4]);
            }
        }
        diagnostics.scale_loop_elapsed_ms = scale_loop_start.elapsed().as_millis();
        diagnostics.bytes_copied_total =
            diagnostics.bytes_copied_total.saturating_add(pixels.len());
        diagnostics.half_scale_count = 1;

        return (
            stream_sync_switcher::SwitcherDecodedFrameRenderInput {
                width: output_width,
                height: output_height,
                pixel_format,
                pixels,
            },
            diagnostics,
        );
    }

    let scale_loop_start = Instant::now();
    for y in 0..output_height as usize {
        let source_y = y * height as usize / output_height as usize;
        for x in 0..output_width as usize {
            let source_x = x * width as usize / output_width as usize;
            let source_index = (source_y * width as usize + source_x) * 4;
            let destination_index = (y * output_width as usize + x) * 4;
            pixels[destination_index..destination_index + 4]
                .copy_from_slice(&pixels_source[source_index..source_index + 4]);
        }
    }
    diagnostics.scale_loop_elapsed_ms = scale_loop_start.elapsed().as_millis();
    diagnostics.bytes_copied_total = diagnostics.bytes_copied_total.saturating_add(pixels.len());
    diagnostics.generic_scale_count = 1;

    (
        stream_sync_switcher::SwitcherDecodedFrameRenderInput {
            width: output_width,
            height: output_height,
            pixel_format,
            pixels,
        },
        diagnostics,
    )
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

fn render_four_view_clean_output_from_borrowed_render_facing<RenderRuntime>(
    render_facing: &stream_sync_switcher::SwitcherFourViewQuadRenderFacingConnectionOutput,
    runtime: &ObsFriendlyFourViewLoopWindowRenderRuntime<'_, RenderRuntime>,
) -> stream_sync_switcher::SwitcherFourViewCleanOutputWindowRenderResult
where
    RenderRuntime: SwitcherWindowRenderRuntimeHook,
{
    match &render_facing.render {
        stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult::RenderReady {
            input: render_input,
        } => render_four_view_clean_output_ready_from_borrowed_composition(
            render_input,
            &render_facing.composition.composition,
            runtime,
        ),
        stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult::NoRenderableQuadView {
            slots,
        } => stream_sync_switcher::SwitcherFourViewCleanOutputWindowRenderResult::NoRenderableQuadView {
            slots: slots.clone(),
        },
        stream_sync_switcher::SwitcherFourViewQuadRenderFacingResult::InvalidQuadView {
            reason,
            slots,
        } => stream_sync_switcher::SwitcherFourViewCleanOutputWindowRenderResult::InvalidQuadView {
            reason: stream_sync_switcher::SwitcherFourViewComposedCanvasWindowRenderInvalidReason::Upstream(reason.clone()),
            slots: slots.clone(),
        },
    }
}

fn render_four_view_clean_output_ready_from_borrowed_composition<RenderRuntime>(
    render_input: &stream_sync_switcher::SwitcherFourViewComposedFrameRenderInput,
    composition: &stream_sync_switcher::SwitcherFourViewQuadCompositionResult,
    runtime: &ObsFriendlyFourViewLoopWindowRenderRuntime<'_, RenderRuntime>,
) -> stream_sync_switcher::SwitcherFourViewCleanOutputWindowRenderResult
where
    RenderRuntime: SwitcherWindowRenderRuntimeHook,
{
    let stream_sync_switcher::SwitcherFourViewQuadCompositionResult::ComposedFrame { frame } =
        composition
    else {
        return stream_sync_switcher::SwitcherFourViewCleanOutputWindowRenderResult::InvalidQuadView {
            reason:
                stream_sync_switcher::SwitcherFourViewComposedCanvasWindowRenderInvalidReason::MissingComposedFrameForRenderReady,
            slots: render_input.slots.clone(),
        };
    };

    if frame.width != render_input.width
        || frame.height != render_input.height
        || frame.pixels.len() != render_input.bgra_payload_len
    {
        return stream_sync_switcher::SwitcherFourViewCleanOutputWindowRenderResult::InvalidQuadView {
            reason:
                stream_sync_switcher::SwitcherFourViewComposedCanvasWindowRenderInvalidReason::RenderReadyMetadataMismatch {
                    expected_width: render_input.width,
                    expected_height: render_input.height,
                    expected_bgra_payload_len: render_input.bgra_payload_len,
                    actual_width: frame.width,
                    actual_height: frame.height,
                    actual_bgra_payload_len: frame.pixels.len(),
                },
            slots: render_input.slots.clone(),
        };
    }

    let render = match runtime.render_bgra_once(
        frame.width,
        frame.height,
        frame.pixel_format,
        &frame.pixels,
        SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE.to_string(),
        0,
    ) {
        SwitcherWindowRenderResult::Rendered(render) => {
            SwitcherFourViewComposedCanvasRenderResult::Rendered { render }
        }
        SwitcherWindowRenderResult::RenderDeferred { reason } => {
            SwitcherFourViewComposedCanvasRenderResult::RenderDeferred { reason }
        }
        SwitcherWindowRenderResult::BackendUnavailable { reason, message } => {
            SwitcherFourViewComposedCanvasRenderResult::BackendUnavailable { reason, message }
        }
        SwitcherWindowRenderResult::InvalidFrame { error } => {
            SwitcherFourViewComposedCanvasRenderResult::InvalidComposedFrame { error }
        }
        SwitcherWindowRenderResult::RenderFailed { message } => {
            SwitcherFourViewComposedCanvasRenderResult::RenderFailed { message }
        }
    };

    stream_sync_switcher::SwitcherFourViewCleanOutputWindowRenderResult::RenderReady {
        width: render_input.width,
        height: render_input.height,
        bgra_payload_len: render_input.bgra_payload_len,
        slots: render_input.slots.clone(),
        render,
    }
}

#[cfg(target_os = "windows")]
fn windows_persistent_render_update(
    state: &Mutex<WindowsPersistentWindowState>,
    request: stream_sync_switcher::SwitcherWindowRenderRequest,
) -> SwitcherWindowRenderResult {
    let window_update_start = Instant::now();
    use std::{
        ptr::null_mut,
        sync::atomic::{AtomicU64, Ordering},
    };
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
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
    static PERSISTENT_WM_PAINT_ELAPSED_MS: AtomicU64 = AtomicU64::new(0);
    static PERSISTENT_STRETCH_DIBITS_ELAPSED_MS: AtomicU64 = AtomicU64::new(0);

    #[allow(static_mut_refs)]
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_PAINT => {
                let paint_start = Instant::now();
                let mut paint = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                let mut stretch_elapsed_ms = 0u128;
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
                    let stretch_start = Instant::now();
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
                    stretch_elapsed_ms = stretch_start.elapsed().as_millis();
                }
                let _ = EndPaint(hwnd, &paint);
                PERSISTENT_WM_PAINT_ELAPSED_MS.fetch_add(
                    paint_start.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    Ordering::Relaxed,
                );
                PERSISTENT_STRETCH_DIBITS_ELAPSED_MS.fetch_add(
                    stretch_elapsed_ms.min(u64::MAX as u128) as u64,
                    Ordering::Relaxed,
                );
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    let frame_width = request.frame.width;
    let frame_height = request.frame.height;
    #[allow(static_mut_refs)]
    let recycled_pixels = unsafe {
        PERSISTENT_PAINT_FRAME
            .take()
            .map(|previous_frame| previous_frame.pixels)
    };
    if let Some(pixels) = recycled_pixels {
        recycle_obs_render_buffer(pixels);
    }
    unsafe {
        PERSISTENT_PAINT_FRAME = Some(request.frame);
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
                frame_width as i32,
                frame_height as i32,
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

    let invalidate_start = Instant::now();
    let _ = unsafe { InvalidateRect(Some(hwnd), None, true) };
    let invalidate_elapsed_ms = invalidate_start.elapsed().as_millis();
    let paint_elapsed_before = PERSISTENT_WM_PAINT_ELAPSED_MS.load(Ordering::Relaxed);
    let stretch_elapsed_before = PERSISTENT_STRETCH_DIBITS_ELAPSED_MS.load(Ordering::Relaxed);
    let mut msg = MSG::default();
    let event_pump_start = Instant::now();
    while unsafe { PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    let event_pump_elapsed_ms = event_pump_start.elapsed().as_millis();
    let paint_elapsed_ms = PERSISTENT_WM_PAINT_ELAPSED_MS
        .load(Ordering::Relaxed)
        .saturating_sub(paint_elapsed_before) as u128;
    let stretch_elapsed_ms = PERSISTENT_STRETCH_DIBITS_ELAPSED_MS
        .load(Ordering::Relaxed)
        .saturating_sub(stretch_elapsed_before) as u128;

    state.lifecycle.window_updates += 1;
    state.lifecycle.event_pump_elapsed_ms += event_pump_elapsed_ms;
    state.lifecycle.window_update_elapsed_ms += window_update_start.elapsed().as_millis();
    state.lifecycle.gdi_invalidate_elapsed_ms += invalidate_elapsed_ms;
    state.lifecycle.gdi_paint_wait_elapsed_ms += event_pump_elapsed_ms;
    state.lifecycle.gdi_wm_paint_elapsed_ms += paint_elapsed_ms;
    state.lifecycle.gdi_stretchdibits_elapsed_ms += stretch_elapsed_ms;

    SwitcherWindowRenderResult::Rendered(stream_sync_switcher::SwitcherWindowRenderSuccess {
        width: frame_width,
        height: frame_height,
        title: request.title,
        hold_millis: request.hold_millis,
    })
}

#[cfg(target_os = "windows")]
fn windows_persistent_render_pump_events(state: &Mutex<WindowsPersistentWindowState>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    let mut state = state
        .lock()
        .expect("persistent window state mutex should not be poisoned");
    let Some(hwnd) = state.hwnd else {
        return;
    };

    let event_pump_start = Instant::now();
    let mut msg = MSG::default();
    while unsafe { PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    state.lifecycle.event_pump_elapsed_ms += event_pump_start.elapsed().as_millis();
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

fn format_four_view_real_handoff_preview_loop_summary(
    summary: &SwitcherFourViewRealHandoffPreviewLoopSummary,
) -> String {
    format!(
        "switcher four-view real handoff preview loop command_name=--four-view-real-handoff-preview-loop real_handoff=true real_slot_count=1 real_slot_index={} pipe_name={} actual_pipe_path={} client_id={} run_id={} frames_attempted={} frames_rendered={} render_failures={} scheduler_status={:?} slot_bindings={} slot_result_kinds={} slot_diagnostics={} clean_output_render_result_kind={} window_title={} output_width={} output_height={}",
        summary.real_slot_index,
        summary.pipe_name,
        summary.actual_pipe_path,
        summary.client_id.0,
        summary.run_id.0,
        summary.frames_attempted,
        summary.frames_rendered,
        summary.render_failures,
        summary.scheduler_status,
        summary.slot_bindings.join("|"),
        summary.slot_result_kinds.join("|"),
        summary.slot_diagnostics.join("|"),
        summary.clean_output_render_result_kind,
        summary.window_title,
        format_optional_u32(summary.output_width),
        format_optional_u32(summary.output_height),
    )
}

fn format_four_view_two_real_handoff_preview_loop_summary(
    summary: &SwitcherFourViewTwoRealHandoffPreviewLoopSummary,
) -> String {
    format!(
        "switcher four-view two-real handoff preview loop command_name=--four-view-two-real-handoff-preview-loop real_handoff=true real_slot_count=2 real_slot0_index={} real_slot1_index={} pipe_name={} actual_pipe_path={} preview_mode={} read_mode={} client0_id={} run0_id={} client1_id={} run1_id={} frames_attempted={} frames_rendered={} render_failures={} elapsed_ms={} target_fps={} configured_frame_interval_ms={} effective_attempt_fps={} effective_render_fps={} first_render_attempt_index={} first_render_elapsed_ms={} rendered_after_first_render={} effective_render_fps_after_first_render={} no_render_before_first_render={} selected_count={} no_frame_count={} handoff_error_count={} decode_attempt_count={} decode_success_count={} render_success_count={} render_failure_count={} unchanged_frame_reuse_count={} skipped_decode_unchanged_frame_count={} redecoded_same_frame_count={} decode_elapsed_ms={} decode_process_spawn_elapsed_ms={} decode_input_write_elapsed_ms={} decode_input_payload_bytes_total={} decode_output_read_elapsed_ms={} decode_output_read_exact_elapsed_ms={} decode_output_vec_resize_elapsed_ms={} decode_process_wait_elapsed_ms={} decode_pixel_convert_elapsed_ms={} decode_buffer_allocation_count={} decode_output_bytes_total={} decode_stdout_expected_bytes_total={} decode_cached_frame_reuse_count={} decode_cache_miss_count={} decoded_buffer_clone_count={} decode_cache_hit_clone_count={} decode_cache_store_clone_count={} decoded_buffer_clone_elapsed_ms={} composed_buffer_clone_count={} decode_output_buffer_reuse_count={} persistent_decode_enabled={} persistent_decode_attempt_count={} persistent_decode_success_count={} persistent_decode_failure_count={} persistent_decode_fallback_count={} persistent_decode_process_spawn_count={} persistent_decode_process_restart_count={} persistent_decode_stdin_write_elapsed_ms={} persistent_decode_stdout_read_elapsed_ms={} persistent_decode_stdout_read_exact_elapsed_ms={} persistent_decode_output_bytes_total={} persistent_decode_last_error={} one_shot_decode_fallback_count={} handoff_elapsed_ms={} render_elapsed_ms={} avg_decode_elapsed_ms={} avg_decode_input_write_elapsed_ms={} avg_decode_output_read_elapsed_ms={} avg_decode_process_spawn_elapsed_ms={} avg_handoff_elapsed_ms={} avg_render_elapsed_ms={} loop_total_elapsed_ms={} attempt_body_elapsed_ms={} loop_sleep_elapsed_ms={} frame_interval_wait_elapsed_ms={} event_pump_elapsed_ms={} window_update_elapsed_ms={} render_prepare_elapsed_ms={} render_buffer_cpu_scale_copy_elapsed_ms={} render_buffer_copy_elapsed_ms={} render_buffer_materialization_elapsed_ms={} render_buffer_scale_prepare_elapsed_ms={} render_buffer_scale_loop_elapsed_ms={} render_buffer_output_copy_elapsed_ms={} render_buffer_resize_elapsed_ms={} render_buffer_clear_elapsed_ms={} render_buffer_passthrough_count={} render_buffer_same_size_copy_count={} render_buffer_half_scale_count={} render_buffer_generic_scale_count={} render_buffer_reuse_count={} render_buffer_allocation_count={} render_buffer_bytes_copied_total={} render_backend_wait_elapsed_ms={} gdi_invalidate_elapsed_ms={} gdi_paint_wait_elapsed_ms={} gdi_wm_paint_elapsed_ms={} gdi_stretchdibits_elapsed_ms={} texture_upload_elapsed_ms={} window_present_elapsed_ms={} vsync_or_present_block_elapsed_ms={} quad_view_compose_elapsed_ms={} quad_view_compose_attempt_count={} quad_view_compose_success_count={} quad_view_compose_skipped_unchanged_count={} quad_view_composed_frame_reuse_count={} quad_view_visual_unchanged_count={} quad_view_visual_changed_count={} materialization_reason_first_render_count={} materialization_reason_visual_changed_count={} materialization_reason_previous_output_missing_count={} materialization_reason_profile_or_size_mismatch_count={} materialization_reason_force_render_count={} materialization_reason_unknown_count={} slot0_frame_id_changed_count={} slot1_frame_id_changed_count={} slot2_frame_id_changed_count={} slot3_frame_id_changed_count={} slot0_selected_source_changed_count={} slot1_selected_source_changed_count={} slot2_selected_source_changed_count={} slot3_selected_source_changed_count={} placeholder_visual_changed_count={} quad_view_incremental_update_count={} quad_view_full_compose_count={} quad_view_changed_slot_update_count={} quad_view_reused_slot_count={} quad_view_allocation_count={} avg_render_buffer_cpu_scale_copy_elapsed_ms={} avg_render_buffer_materialization_elapsed_ms={} avg_gdi_paint_wait_elapsed_ms={} avg_gdi_wm_paint_elapsed_ms={} avg_gdi_stretchdibits_elapsed_ms={} avg_quad_view_incremental_update_elapsed_ms={} avg_quad_view_compose_elapsed_ms={} render_call_elapsed_ms={} render_input_unchanged_count={} render_reuse_frame_count={} unaccounted_elapsed_ms={} avg_attempt_elapsed_ms={} max_attempt_elapsed_ms={} slow_attempt_count={} slow_attempt_threshold_ms={} scheduler_status={:?} slot_bindings={} slot_result_kinds={} slot_diagnostics={} clean_output_render_result_kind={} window_title={} output_width={} output_height={}",
        summary.real_slot0_index,
        summary.real_slot1_index,
        summary.pipe_name,
        summary.actual_pipe_path,
        summary.preview_mode,
        summary.read_mode,
        summary.client0_id.0,
        summary.run0_id.0,
        summary.client1_id.0,
        summary.run1_id.0,
        summary.frames_attempted,
        summary.frames_rendered,
        summary.render_failures,
        summary.elapsed_ms,
        summary.target_fps,
        summary.configured_frame_interval_ms,
        summary.effective_attempt_fps,
        summary.effective_render_fps,
        format_optional_u32(summary.first_render_attempt_index),
        format_optional_u128(summary.first_render_elapsed_ms),
        summary.rendered_after_first_render,
        summary.effective_render_fps_after_first_render,
        summary.no_render_before_first_render,
        summary.selected_count,
        summary.no_frame_count,
        summary.handoff_error_count,
        summary.decode_attempt_count,
        summary.decode_success_count,
        summary.render_success_count,
        summary.render_failure_count,
        summary.unchanged_frame_reuse_count,
        summary.skipped_decode_unchanged_frame_count,
        summary.redecoded_same_frame_count,
        summary.decode_elapsed_ms,
        summary.decode_process_spawn_elapsed_ms,
        summary.decode_input_write_elapsed_ms,
        summary.decode_input_payload_bytes_total,
        summary.decode_output_read_elapsed_ms,
        summary.decode_output_read_exact_elapsed_ms,
        summary.decode_output_vec_resize_elapsed_ms,
        summary.decode_process_wait_elapsed_ms,
        summary.decode_pixel_convert_elapsed_ms,
        summary.decode_buffer_allocation_count,
        summary.decode_output_bytes_total,
        summary.decode_stdout_expected_bytes_total,
        summary.decode_cached_frame_reuse_count,
        summary.decode_cache_miss_count,
        summary.decoded_buffer_clone_count,
        summary.decode_cache_hit_clone_count,
        summary.decode_cache_store_clone_count,
        summary.decoded_buffer_clone_elapsed_ms,
        summary.composed_buffer_clone_count,
        summary.decode_output_buffer_reuse_count,
        summary.persistent_decode_enabled,
        summary.persistent_decode_attempt_count,
        summary.persistent_decode_success_count,
        summary.persistent_decode_failure_count,
        summary.persistent_decode_fallback_count,
        summary.persistent_decode_process_spawn_count,
        summary.persistent_decode_process_restart_count,
        summary.persistent_decode_stdin_write_elapsed_ms,
        summary.persistent_decode_stdout_read_elapsed_ms,
        summary.persistent_decode_stdout_read_exact_elapsed_ms,
        summary.persistent_decode_output_bytes_total,
        sanitize_summary_value(&summary.persistent_decode_last_error),
        summary.one_shot_decode_fallback_count,
        summary.handoff_elapsed_ms,
        summary.render_elapsed_ms,
        summary.avg_decode_elapsed_ms,
        summary.avg_decode_input_write_elapsed_ms,
        summary.avg_decode_output_read_elapsed_ms,
        summary.avg_decode_process_spawn_elapsed_ms,
        summary.avg_handoff_elapsed_ms,
        summary.avg_render_elapsed_ms,
        summary.loop_total_elapsed_ms,
        summary.attempt_body_elapsed_ms,
        summary.loop_sleep_elapsed_ms,
        summary.frame_interval_wait_elapsed_ms,
        summary.event_pump_elapsed_ms,
        summary.window_update_elapsed_ms,
        summary.render_prepare_elapsed_ms,
        summary.render_buffer_cpu_scale_copy_elapsed_ms,
        summary.render_buffer_copy_elapsed_ms,
        summary.render_buffer_materialization_elapsed_ms,
        summary.render_buffer_scale_prepare_elapsed_ms,
        summary.render_buffer_scale_loop_elapsed_ms,
        summary.render_buffer_output_copy_elapsed_ms,
        summary.render_buffer_resize_elapsed_ms,
        summary.render_buffer_clear_elapsed_ms,
        summary.render_buffer_passthrough_count,
        summary.render_buffer_same_size_copy_count,
        summary.render_buffer_half_scale_count,
        summary.render_buffer_generic_scale_count,
        summary.render_buffer_reuse_count,
        summary.render_buffer_allocation_count,
        summary.render_buffer_bytes_copied_total,
        summary.render_backend_wait_elapsed_ms,
        summary.gdi_invalidate_elapsed_ms,
        summary.gdi_paint_wait_elapsed_ms,
        summary.gdi_wm_paint_elapsed_ms,
        summary.gdi_stretchdibits_elapsed_ms,
        summary.texture_upload_elapsed_ms,
        summary.window_present_elapsed_ms,
        summary.vsync_or_present_block_elapsed_ms,
        summary.quad_view_compose_elapsed_ms,
        summary.quad_view_compose_attempt_count,
        summary.quad_view_compose_success_count,
        summary.quad_view_compose_skipped_unchanged_count,
        summary.quad_view_composed_frame_reuse_count,
        summary.quad_view_visual_unchanged_count,
        summary.quad_view_visual_changed_count,
        summary.materialization_reason_first_render_count,
        summary.materialization_reason_visual_changed_count,
        summary.materialization_reason_previous_output_missing_count,
        summary.materialization_reason_profile_or_size_mismatch_count,
        summary.materialization_reason_force_render_count,
        summary.materialization_reason_unknown_count,
        summary.slot0_frame_id_changed_count,
        summary.slot1_frame_id_changed_count,
        summary.slot2_frame_id_changed_count,
        summary.slot3_frame_id_changed_count,
        summary.slot0_selected_source_changed_count,
        summary.slot1_selected_source_changed_count,
        summary.slot2_selected_source_changed_count,
        summary.slot3_selected_source_changed_count,
        summary.placeholder_visual_changed_count,
        summary.quad_view_incremental_update_count,
        summary.quad_view_full_compose_count,
        summary.quad_view_changed_slot_update_count,
        summary.quad_view_reused_slot_count,
        summary.quad_view_allocation_count,
        summary.avg_render_buffer_cpu_scale_copy_elapsed_ms,
        summary.avg_render_buffer_materialization_elapsed_ms,
        summary.avg_gdi_paint_wait_elapsed_ms,
        summary.avg_gdi_wm_paint_elapsed_ms,
        summary.avg_gdi_stretchdibits_elapsed_ms,
        summary.avg_quad_view_incremental_update_elapsed_ms,
        summary.avg_quad_view_compose_elapsed_ms,
        summary.render_call_elapsed_ms,
        summary.render_input_unchanged_count,
        summary.render_reuse_frame_count,
        summary.unaccounted_elapsed_ms,
        summary.avg_attempt_elapsed_ms,
        summary.max_attempt_elapsed_ms,
        summary.slow_attempt_count,
        summary.slow_attempt_threshold_ms,
        summary.scheduler_status,
        summary.slot_bindings.join("|"),
        summary.slot_result_kinds.join("|"),
        summary.slot_diagnostics.join("|"),
        summary.clean_output_render_result_kind,
        summary.window_title,
        format_optional_u32(summary.output_width),
        format_optional_u32(summary.output_height),
    )
}

fn format_four_view_four_real_handoff_preview_loop_summary(
    summary: &SwitcherFourViewFourRealHandoffPreviewLoopSummary,
) -> String {
    format!(
        "switcher four-view four-real handoff preview loop command_name=--four-view-four-real-handoff-preview-loop real_handoff=true real_slot_count=4 pipe_name={} actual_pipe_path={} preview_mode={} read_mode={} client0_id={} run0_id={} client1_id={} run1_id={} client2_id={} run2_id={} client3_id={} run3_id={} frames_attempted={} frames_rendered={} render_failures={} scheduler_status={:?} slot_bindings={} slot_result_kinds={} slot_diagnostics={} clean_output_render_result_kind={} window_title={} output_width={} output_height={}",
        summary.pipe_name,
        summary.actual_pipe_path,
        summary.preview_mode,
        summary.read_mode,
        summary.client0_id.0,
        summary.run0_id.0,
        summary.client1_id.0,
        summary.run1_id.0,
        summary.client2_id.0,
        summary.run2_id.0,
        summary.client3_id.0,
        summary.run3_id.0,
        summary.frames_attempted,
        summary.frames_rendered,
        summary.render_failures,
        summary.scheduler_status,
        summary.slot_bindings.join("|"),
        summary.slot_result_kinds.join("|"),
        summary.slot_diagnostics.join("|"),
        summary.clean_output_render_result_kind,
        summary.window_title,
        format_optional_u32(summary.output_width),
        format_optional_u32(summary.output_height),
    )
}

fn format_four_view_focused_handoff_preview_loop_summary(
    summary: &SwitcherFourViewFocusedHandoffPreviewLoopSummary,
) -> String {
    format!(
        "switcher four-view focused handoff preview loop command_name=--four-view-focused-handoff-preview-loop real_handoff=true real_slot_count=4 view_state=Focused focused_slot_index={} pipe_name={} actual_pipe_path={} client0_id={} run0_id={} client1_id={} run1_id={} client2_id={} run2_id={} client3_id={} run3_id={} focused_client_id={} focused_run_id={} focused_result_kind={} frames_attempted={} frames_rendered={} render_failures={} scheduler_status={:?} slot_bindings={} slot_result_kinds={} slot_diagnostics={} clean_output_render_result_kind={} window_title={} output_width={} output_height={}",
        summary.focused_slot_index,
        summary.pipe_name,
        summary.actual_pipe_path,
        summary.client0_id.0,
        summary.run0_id.0,
        summary.client1_id.0,
        summary.run1_id.0,
        summary.client2_id.0,
        summary.run2_id.0,
        summary.client3_id.0,
        summary.run3_id.0,
        summary.focused_client_id.0,
        summary.focused_run_id.0,
        summary.focused_result_kind,
        summary.frames_attempted,
        summary.frames_rendered,
        summary.render_failures,
        summary.scheduler_status,
        summary.slot_bindings.join("|"),
        summary.slot_result_kinds.join("|"),
        summary.slot_diagnostics.join("|"),
        summary.clean_output_render_result_kind,
        summary.window_title,
        format_optional_u32(summary.output_width),
        format_optional_u32(summary.output_height),
    )
}

fn format_four_view_controlled_handoff_preview_command_summary(
    summary: &SwitcherFourViewControlledPreviewCommandSummary,
) -> String {
    format!(
        "switcher four-view controlled handoff preview command command_name=--four-view-controlled-handoff-preview-loop command_index={} control_command_name={} current_view_state={} view_render_mode={} output_layout={} requested_transition={} transition_result={} selected_slot_result={} rendered_slot_count={} focused_slot_index={} frames_rendered={} render_failures={} scheduler_status={:?} clean_output_render_result_kind={} all_view_render_result_kind={} command_parse_error={} exit_reason={}",
        summary.command_index,
        summary.control_command_name,
        format_four_view_controlled_preview_view_state(&summary.current_view_state),
        summary.view_render_mode,
        summary.output_layout,
        summary.requested_transition,
        summary.transition_result,
        summary.selected_slot_result,
        summary.rendered_slot_count,
        format_optional_usize(summary.focused_slot_index),
        summary.frames_rendered,
        summary.render_failures,
        summary.scheduler_status,
        summary.clean_output_render_result_kind,
        summary.all_view_render_result_kind,
        sanitize_summary_value(&summary.command_parse_error),
        summary.exit_reason,
    )
}

fn format_four_view_controlled_handoff_preview_loop_summary(
    summary: &SwitcherFourViewControlledHandoffPreviewLoopSummary,
) -> String {
    format!(
        "switcher four-view controlled handoff preview loop command_name=--four-view-controlled-handoff-preview-loop real_handoff=true real_slot_count=4 pipe_name={} actual_pipe_path={} client0_id={} run0_id={} client1_id={} run1_id={} client2_id={} run2_id={} client3_id={} run3_id={} command_source={} max_ticks_per_command={} commands_processed={} commands_rejected={} current_view_state={} view_render_mode={} output_layout={} rendered_slot_count={} focused_slot_index={} frames_rendered={} render_failures={} scheduler_status={:?} slot_bindings={} slot_result_kinds={} slot_diagnostics={} clean_output_render_result_kind={} all_view_render_result_kind={} window_title={} output_width={} output_height={} exit_reason={}",
        summary.pipe_name,
        summary.actual_pipe_path,
        summary.client0_id.0,
        summary.run0_id.0,
        summary.client1_id.0,
        summary.run1_id.0,
        summary.client2_id.0,
        summary.run2_id.0,
        summary.client3_id.0,
        summary.run3_id.0,
        summary.command_source,
        summary.max_ticks_per_command,
        summary.commands_processed,
        summary.commands_rejected,
        format_four_view_controlled_preview_view_state(&summary.final_view_state),
        summary.view_render_mode,
        summary.output_layout,
        summary.rendered_slot_count,
        format_optional_usize(summary.focused_slot_index),
        summary.frames_rendered,
        summary.render_failures,
        summary.scheduler_status,
        summary.slot_bindings.join("|"),
        summary.slot_result_kinds.join("|"),
        summary.slot_diagnostics.join("|"),
        summary.clean_output_render_result_kind,
        summary.all_view_render_result_kind,
        summary.window_title,
        format_optional_u32(summary.output_width),
        format_optional_u32(summary.output_height),
        summary.exit_reason,
    )
}

fn format_four_view_operator_wrapper_key_summary(
    summary: &FourViewOperatorWrapperKeySummary,
) -> String {
    format!(
        "switcher four-view operator wrapper key command_name=--four-view-operator-wrapper key_index={} wrapper_key={} mapped_command={} guard_state={} send_result={} response_line={} command_parse_error={} wrapper_error={} exit_reason={}",
        summary.key_index,
        summary.wrapper_key,
        summary.mapped_command,
        summary.guard_state,
        summary.send_result,
        summary.response_line,
        summary.command_parse_error,
        summary.wrapper_error,
        summary.exit_reason,
    )
}

fn format_four_view_operator_wrapper_loop_summary(
    summary: &FourViewOperatorWrapperLoopSummary,
) -> String {
    format!(
        "switcher four-view operator wrapper loop command_name=--four-view-operator-wrapper control_pipe_name={} input_source={} keys_processed={} commands_sent={} ignored_keys={} final_guard_state={} raw_console_restore_result={} raw_console_restore_error={} exit_reason={}",
        sanitize_summary_value(&summary.control_pipe_name),
        summary.input_source,
        summary.keys_processed,
        summary.commands_sent,
        summary.ignored_keys,
        summary.final_guard_state,
        summary.raw_console_restore_result,
        summary.raw_console_restore_error,
        summary.exit_reason,
    )
}

fn build_four_view_preview_slot_diagnostic(
    slot_index: usize,
    slot: &SwitcherFourViewTargetTimeSourceSlotConfig,
    observation: Option<&FourViewPreviewSlotHandoffObservation>,
    scheduler_result: &SwitcherSingleClientTargetTimeHandoffSourceResult,
    render_slot: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> FourViewPreviewSlotDiagnosticSummary {
    let request_output = observation.and_then(|value| value.request_output.as_ref());
    let runtime = request_output.and_then(|output| output.runtime.as_ref());
    let response = runtime.and_then(|value| value.response.as_ref());
    let observation_result = observation.map(|value| &value.result);
    let handoff_response_kind = match response {
        Some(response) => Some(format_handoff_response_kind(response)),
        None => match observation_result {
            Some(SwitcherQueuedFrameHandoffResult::FrameRead { .. }) => Some("FrameRead"),
            Some(SwitcherQueuedFrameHandoffResult::NoFrameAvailable { .. }) => Some("NoFrame"),
            Some(SwitcherQueuedFrameHandoffResult::HandoffError { .. }) => Some("HandoffError"),
            None => None,
        },
    };
    let (frame_id, frame_payload_len, frame_is_keyframe) = match response {
        Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
            frame,
            ..
        }) => (
            Some(frame.frame_id),
            Some(frame.encoded_payload_len as usize),
            Some(frame.is_keyframe),
        ),
        _ => match observation_result {
            Some(SwitcherQueuedFrameHandoffResult::FrameRead { frame, .. }) => (
                Some(frame.frame_id),
                Some(frame.encoded_payload_len),
                Some(frame.is_keyframe),
            ),
            _ => (None, None, None),
        },
    };
    let handoff_no_frame_reason = match response {
        Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
            no_frame_reason,
            ..
        }) => Some(format_handoff_no_frame_reason(*no_frame_reason).to_string()),
        _ => None,
    };
    let decodable_source = match response {
        Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
            decodable_source,
            ..
        })
        | Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
            decodable_source,
            ..
        }) => Some(format_handoff_decodable_source(*decodable_source).to_string()),
        _ => None,
    };
    let (retained_keyframe_available, retained_keyframe_frame_id) = match response {
        Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
            retained_keyframe_available,
            retained_keyframe_frame_id,
            ..
        })
        | Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
            retained_keyframe_available,
            retained_keyframe_frame_id,
            ..
        }) => (
            Some(*retained_keyframe_available),
            *retained_keyframe_frame_id,
        ),
        _ => (None, None),
    };
    let frame_payload = match response {
        Some(stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
            frame,
            ..
        }) => Some(frame.encoded_payload.as_slice()),
        _ => match observation_result {
            Some(SwitcherQueuedFrameHandoffResult::FrameRead { frame, .. }) => {
                Some(frame.encoded_payload.as_slice())
            }
            _ => None,
        },
    };
    let payload_summary = frame_payload
        .map(|payload| SwitcherH264AnnexBPayloadInspectionBoundary.inspect_payload(payload));
    let decode_failure = four_view_preview_slot_decode_failure(render_slot);
    let decode_attempted = Some(four_view_preview_slot_decode_attempted(render_slot));
    let target_selection_result =
        format_four_view_real_handoff_scheduler_slot_kind(scheduler_result);
    let (selected_frame_available, selected_frame_id) = match scheduler_result {
        SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(selected) => {
            (Some(true), Some(selected.frame.frame_id))
        }
        _ => (Some(false), None),
    };
    let selected_frame_source = if selected_frame_available == Some(true) {
        decodable_source.clone()
    } else {
        None
    };
    let renderable_frame_available = Some(four_view_preview_slot_renderable_frame_available(
        render_slot,
    ));

    FourViewPreviewSlotDiagnosticSummary {
        slot_index,
        client_id: slot.client_id.clone(),
        run_id: slot.run_id.clone(),
        request_id: request_output.map(|output| output.summary.request_id),
        actual_pipe_path: request_output.and_then(|output| output.summary.actual_pipe_path.clone()),
        handoff_response_kind,
        parse_error: runtime
            .and_then(|value| value.parse_error.clone())
            .or_else(|| request_output.and_then(|output| output.summary.local_error.clone())),
        io_error: runtime.and_then(|value| value.io_error.clone()),
        response_payload_len: runtime.and_then(|value| value.response_payload_len),
        frame_id,
        frame_payload_len,
        frame_is_keyframe,
        handoff_no_frame_reason,
        decodable_source,
        retained_keyframe_available,
        retained_keyframe_frame_id,
        decode_attempted,
        decode_skipped_reason: if decode_attempted == Some(false) {
            four_view_preview_slot_decode_skipped_reason(render_slot)
        } else {
            None
        },
        decode_error: decode_failure
            .as_ref()
            .map(|failure| failure.message.clone()),
        decode_input_payload_len: decode_failure
            .as_ref()
            .map(|failure| failure.decode_input_payload_len),
        decode_expected_width: decode_failure
            .as_ref()
            .map(|failure| failure.decode_expected_width),
        decode_expected_height: decode_failure
            .as_ref()
            .map(|failure| failure.decode_expected_height),
        decode_expected_pixel_format: decode_failure.as_ref().map(|failure| {
            failure
                .decode_expected_pixel_format
                .as_config_str()
                .to_string()
        }),
        decode_expected_rawvideo_len: decode_failure
            .as_ref()
            .map(|failure| failure.decode_expected_rawvideo_len),
        decoded_stdout_len: decode_failure
            .as_ref()
            .map(|failure| failure.decoded_stdout_len),
        ffmpeg_exit_status: decode_failure
            .as_ref()
            .and_then(|failure| failure.ffmpeg_exit_status),
        ffmpeg_stderr_summary: decode_failure
            .as_ref()
            .and_then(|failure| failure.ffmpeg_stderr_summary.clone()),
        payload_has_sps: payload_summary.as_ref().map(|summary| summary.has_sps),
        payload_has_pps: payload_summary.as_ref().map(|summary| summary.has_pps),
        payload_has_idr: payload_summary.as_ref().map(|summary| summary.has_idr),
        payload_has_non_idr_vcl: payload_summary
            .as_ref()
            .map(|summary| summary.has_non_idr_vcl),
        payload_nal_kinds: payload_summary
            .as_ref()
            .map(format_switcher_payload_nal_kinds),
        renderable_frame_available,
        renderable_frame_missing_reason: if renderable_frame_available == Some(false) {
            four_view_preview_slot_renderable_frame_missing_reason(render_slot)
        } else {
            None
        },
        selected_frame_available,
        selected_frame_id,
        selected_frame_source,
        target_selection_result,
        render_input_kind: format_four_view_render_input_kind(render_slot),
        final_slot_result_kind: target_selection_result,
    }
}

fn format_four_view_preview_slot_diagnostic(
    diagnostic: &FourViewPreviewSlotDiagnosticSummary,
) -> String {
    format!(
        "{}:client_id={},run_id={},request_id={},actual_pipe_path={},handoff_response_kind={},parse_error={},io_error={},response_payload_len={},frame_id={},frame_payload_len={},frame_is_keyframe={},handoff_no_frame_reason={},decodable_source={},retained_keyframe_available={},retained_keyframe_frame_id={},selected_frame_available={},selected_frame_id={},selected_frame_source={},target_selection_result={},decode_attempted={},decode_skipped_reason={},decode_error={},decode_input_payload_len={},decode_expected_width={},decode_expected_height={},decode_expected_pixel_format={},decode_expected_rawvideo_len={},decoded_stdout_len={},ffmpeg_exit_status={},ffmpeg_stderr_summary={},payload_has_sps={},payload_has_pps={},payload_has_idr={},payload_has_non_idr_vcl={},payload_nal_kinds={},renderable_frame_available={},renderable_frame_missing_reason={},render_input_kind={},final_slot_result_kind={}",
        diagnostic.slot_index,
        diagnostic.client_id.0,
        diagnostic.run_id.0,
        format_optional_u64(diagnostic.request_id),
        sanitize_summary_value(diagnostic.actual_pipe_path.as_deref().unwrap_or("none")),
        diagnostic.handoff_response_kind.unwrap_or("none"),
        sanitize_summary_value(diagnostic.parse_error.as_deref().unwrap_or("none")),
        sanitize_summary_value(diagnostic.io_error.as_deref().unwrap_or("none")),
        format_optional_usize(diagnostic.response_payload_len),
        format_optional_u64(diagnostic.frame_id),
        format_optional_usize(diagnostic.frame_payload_len),
        format_optional_bool(diagnostic.frame_is_keyframe),
        sanitize_summary_value(
            diagnostic
                .handoff_no_frame_reason
                .as_deref()
                .unwrap_or("none"),
        ),
        sanitize_summary_value(diagnostic.decodable_source.as_deref().unwrap_or("none")),
        format_optional_bool(diagnostic.retained_keyframe_available),
        format_optional_u64(diagnostic.retained_keyframe_frame_id),
        format_optional_bool(diagnostic.selected_frame_available),
        format_optional_u64(diagnostic.selected_frame_id),
        sanitize_summary_value(diagnostic.selected_frame_source.as_deref().unwrap_or("none")),
        diagnostic.target_selection_result,
        format_optional_bool(diagnostic.decode_attempted),
        sanitize_summary_value(diagnostic.decode_skipped_reason.as_deref().unwrap_or("none")),
        sanitize_summary_value(diagnostic.decode_error.as_deref().unwrap_or("none")),
        format_optional_usize(diagnostic.decode_input_payload_len),
        format_optional_u32(diagnostic.decode_expected_width),
        format_optional_u32(diagnostic.decode_expected_height),
        sanitize_summary_value(
            diagnostic
                .decode_expected_pixel_format
                .as_deref()
                .unwrap_or("none"),
        ),
        format_optional_usize(diagnostic.decode_expected_rawvideo_len),
        format_optional_usize(diagnostic.decoded_stdout_len),
        diagnostic
            .ffmpeg_exit_status
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        sanitize_summary_value(diagnostic.ffmpeg_stderr_summary.as_deref().unwrap_or("none")),
        format_optional_bool(diagnostic.payload_has_sps),
        format_optional_bool(diagnostic.payload_has_pps),
        format_optional_bool(diagnostic.payload_has_idr),
        format_optional_bool(diagnostic.payload_has_non_idr_vcl),
        sanitize_summary_value(diagnostic.payload_nal_kinds.as_deref().unwrap_or("none")),
        format_optional_bool(diagnostic.renderable_frame_available),
        sanitize_summary_value(
            diagnostic
                .renderable_frame_missing_reason
                .as_deref()
                .unwrap_or("none"),
        ),
        diagnostic.render_input_kind,
        diagnostic.final_slot_result_kind,
    )
}

fn format_four_view_render_input_kind(
    slot: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> &'static str {
    match slot {
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame {
            ..
        } => "UseUpdatedFrame",
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            ..
        } => "UseHeldPreviousFrame",
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrameWithoutDecoded {
            ..
        } => "UseHeldPreviousFrameWithoutDecoded",
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseNoDisplayPlaceholder {
            ..
        } => "UseNoDisplayPlaceholder",
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseSourceErrorPlaceholder {
            ..
        } => "UseSourceErrorPlaceholder",
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeDeferredPlaceholder {
            ..
        } => "UseDecodeDeferredPlaceholder",
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
            ..
        } => "UseDecodeFailedPlaceholder",
    }
}

fn four_view_preview_slot_decode_failure(
    slot: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> Option<&SwitcherH264DecodeFailure> {
    match slot {
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
            failure,
            ..
        } => Some(failure),
        _ => None,
    }
}

fn four_view_preview_slot_decode_attempted(
    slot: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> bool {
    matches!(
        slot,
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame {
            ..
        }
            | stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeDeferredPlaceholder {
                ..
            }
            | stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
                ..
            }
    )
}

fn four_view_preview_slot_decode_skipped_reason(
    slot: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> Option<String> {
    match slot {
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame {
            ..
        }
        | stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeDeferredPlaceholder {
            ..
        }
        | stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
            ..
        } => None,
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            ..
        } => Some("HoldPreviousFrame".to_string()),
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrameWithoutDecoded {
            ..
        } => Some("HoldPreviousFrameWithoutDecoded".to_string()),
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseNoDisplayPlaceholder {
            skipped,
            ..
        }
        | stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseSourceErrorPlaceholder {
            skipped,
            ..
        } => Some(format_four_view_skipped_instruction_reason(skipped).to_string()),
    }
}

fn four_view_preview_slot_renderable_frame_available(
    slot: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> bool {
    matches!(
        slot,
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame {
            ..
        }
            | stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
                ..
            }
    )
}

fn four_view_preview_slot_renderable_frame_missing_reason(
    slot: &stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot,
) -> Option<String> {
    match slot {
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseUpdatedFrame {
            ..
        }
        | stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrame {
            ..
        } => None,
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseHeldPreviousFrameWithoutDecoded {
            ..
        } => Some("HeldPreviousFrameWithoutDecoded".to_string()),
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseNoDisplayPlaceholder {
            skipped,
            ..
        } => Some(format!(
            "NoDisplayPlaceholder:{}",
            format_four_view_skipped_instruction_reason(skipped)
        )),
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseSourceErrorPlaceholder {
            skipped,
            ..
        } => Some(format!(
            "SourceErrorPlaceholder:{}",
            format_four_view_skipped_instruction_reason(skipped)
        )),
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeDeferredPlaceholder {
            reason,
            ..
        } => Some(format!(
            "DecodeDeferred:{}",
            format_switcher_h264_decode_deferred_reason(*reason)
        )),
        stream_sync_switcher::SwitcherFourViewHandoffQuadCompositionRenderSlot::UseDecodeFailedPlaceholder {
            ..
        } => Some("DecodeFailed".to_string()),
    }
}

fn format_four_view_skipped_instruction_reason(
    skipped: &stream_sync_switcher::SwitcherFourViewHandoffSchedulerDecodeRenderSlotInstruction,
) -> &'static str {
    match skipped {
        stream_sync_switcher::SwitcherFourViewHandoffSchedulerDecodeRenderSlotInstruction::RenderFrame {
            ..
        } => "RenderFrame",
        stream_sync_switcher::SwitcherFourViewHandoffSchedulerDecodeRenderSlotInstruction::SkipNoFrameAvailable {
            ..
        } => "NoFrameAvailable",
        stream_sync_switcher::SwitcherFourViewHandoffSchedulerDecodeRenderSlotInstruction::SkipWaitingForFrameAtOrBeforeTarget {
            ..
        } => "WaitingForFrameAtOrBeforeTarget",
        stream_sync_switcher::SwitcherFourViewHandoffSchedulerDecodeRenderSlotInstruction::SkipHandoffError {
            ..
        } => "HandoffError",
    }
}

fn format_switcher_h264_decode_deferred_reason(
    reason: SwitcherH264DecodeDeferredReason,
) -> &'static str {
    match reason {
        SwitcherH264DecodeDeferredReason::EmptyPayload => "EmptyPayload",
        SwitcherH264DecodeDeferredReason::InvalidDimensions => "InvalidDimensions",
        SwitcherH264DecodeDeferredReason::FfmpegUnavailable => "FfmpegUnavailable",
    }
}

fn format_switcher_payload_nal_kinds(
    summary: &stream_sync_switcher::SwitcherH264AnnexBPayloadSummary,
) -> String {
    if summary.nal_kinds.is_empty() {
        return "none".to_string();
    }
    summary
        .nal_kinds
        .iter()
        .map(|kind| kind.as_summary_str())
        .collect::<Vec<_>>()
        .join("+")
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

fn format_four_view_real_handoff_scheduler_slot_kind(
    result: &SwitcherSingleClientTargetTimeHandoffSourceResult,
) -> &'static str {
    match result {
        SwitcherSingleClientTargetTimeHandoffSourceResult::Selected(_) => "Selected",
        SwitcherSingleClientTargetTimeHandoffSourceResult::NoFrameAvailable { .. } => {
            "NoFrameAvailable"
        }
        SwitcherSingleClientTargetTimeHandoffSourceResult::WaitingForFrameAtOrBeforeTarget {
            ..
        } => "WaitingForFrameAtOrBeforeTarget",
        SwitcherSingleClientTargetTimeHandoffSourceResult::HandoffError { .. } => "HandoffError",
    }
}

fn format_four_view_slot_binding(slot: &SwitcherFourViewTargetTimeSourceSlotConfig) -> String {
    format!("{}:{}/{}", slot.slot_index, slot.client_id.0, slot.run_id.0)
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

fn clean_output_window_was_rendered(output: &SwitcherFourViewCleanOutputWindowOutput) -> bool {
    clean_output_window_result_was_rendered(&output.output_window)
}

fn clean_output_window_result_was_rendered(
    output: &SwitcherFourViewCleanOutputWindowRenderResult,
) -> bool {
    matches!(
        output,
        SwitcherFourViewCleanOutputWindowRenderResult::RenderReady {
            render: SwitcherFourViewComposedCanvasRenderResult::Rendered { .. },
            ..
        }
    )
}

fn format_focused_window_render_result_kind(result: &SwitcherWindowRenderResult) -> &'static str {
    match result {
        SwitcherWindowRenderResult::Rendered(_) => "Rendered",
        SwitcherWindowRenderResult::RenderDeferred { .. } => "RenderDeferred",
        SwitcherWindowRenderResult::BackendUnavailable { .. } => "BackendUnavailable",
        SwitcherWindowRenderResult::InvalidFrame { .. } => "InvalidFrame",
        SwitcherWindowRenderResult::RenderFailed { .. } => "RenderFailed",
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

fn format_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn format_optional_u128(value: Option<u128>) -> String {
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

fn sanitize_summary_value(value: &str) -> String {
    value
        .chars()
        .map(|char| match char {
            ' ' | '\t' | '\r' | '\n' | '|' | ',' => '_',
            _ => char,
        })
        .collect()
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
    use std::collections::VecDeque;
    use std::io;
    use std::num::NonZeroU32;
    use std::rc::Rc;
    use std::time::Duration;

    use stream_sync_protocol::{ClientId, Codec, RunId, TimestampMicros};
    use stream_sync_switcher::{
        SwitcherDecodedFramePixelFormat, SwitcherFourViewCleanOutputWindowRenderResult,
        SwitcherFourViewManualPreviewBgraCompositionKind,
        SwitcherFourViewManualPreviewProofFixtureMode,
        SwitcherFourViewManualPreviewRenderFacingKind,
        SwitcherFourViewManualPreviewWindowRenderKind,
        SwitcherNamedPipeQueuedFrameHandoffRequestOutput,
        SwitcherNamedPipeQueuedFrameHandoffRequestStatus,
        SwitcherNamedPipeQueuedFrameHandoffRequestSummary,
        SwitcherNamedPipeQueuedFrameHandoffResponseStatus,
        SwitcherNamedPipeQueuedFrameHandoffResultKind,
        SwitcherNamedPipeQueuedFrameHandoffRetryClassification, SwitcherQueuedFrameHandoff,
        SwitcherQueuedFrameHandoffError, SwitcherQueuedFrameHandoffInput,
        SwitcherSingleViewSelectedEncodedFrame, SwitcherUnavailableWindowRenderRuntimeHook,
        SwitcherWindowRenderRequest, SwitcherWindowRenderResult, SwitcherWindowRenderRuntimeHook,
        SwitcherWindowRenderSuccess,
    };

    use super::{
        format_four_view_clean_output_window_loop_summary,
        format_four_view_clean_output_window_summary,
        format_four_view_control_pipe_command_response,
        format_four_view_controlled_handoff_preview_command_summary,
        format_four_view_controlled_handoff_preview_loop_summary,
        format_four_view_focused_handoff_preview_loop_summary,
        format_four_view_four_real_handoff_preview_loop_summary,
        format_four_view_manual_preview_proof_summary,
        format_four_view_manual_preview_window_summary,
        format_four_view_real_handoff_preview_loop_summary,
        format_four_view_two_real_handoff_preview_loop_summary, format_handoff_mode,
        format_handoff_read_mode, format_named_pipe_handoff_switcher_result_summary,
        format_named_pipe_handoff_switcher_summary,
        four_view_clean_output_window_loop_frame_cadence, handoff_read_mode_from_switcher_mode,
        parse_four_view_actual_window_fixture_mode_or_exit,
        parse_four_view_all_renderable_fixture_mode, parse_four_view_control_command,
        parse_four_view_control_command_source,
        parse_four_view_manual_preview_fixture_mode_or_exit,
        parse_four_view_operator_wrapper_input_source, parse_four_view_real_slot_index_or_exit,
        parse_handoff_mode_or_exit, parse_optional_four_view_control_script,
        parse_optional_real_handoff_preview_mode_or_exit, parse_positive_u32_arg,
        preview_target_time_mode_from_switcher_mode, process_four_view_operator_wrapper_key,
        read_length_prefixed_utf8_message, recycle_obs_render_buffer,
        run_four_view_clean_output_window_loop_with_runtime_and_sleep,
        run_four_view_clean_output_window_with_runtime,
        run_four_view_controlled_handoff_preview_loop_with_handoff_runtime_and_sleep,
        run_four_view_focused_handoff_preview_loop_with_handoff_runtime_and_sleep,
        run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_and_sleep,
        run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep,
        run_four_view_manual_preview_proof_once, run_four_view_manual_preview_proof_with_runtime,
        run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime,
        run_four_view_real_handoff_preview_loop_with_handoff_runtime_and_sleep,
        run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep,
        run_send_control_command_with_runtime,
        scale_four_view_bgra_to_obs_validation_profile_from_slice,
        split_scripted_operator_wrapper_keys, take_reusable_obs_render_buffer,
        validate_distinct_four_view_real_slot_indices, write_length_prefixed_utf8_message,
        DeterministicFourViewFixtureDecodeRuntime, FourViewControlCommandSource,
        FourViewControlPipeClientRuntime, FourViewOperatorWrapperClock,
        FourViewOperatorWrapperGuardState, FourViewOperatorWrapperInputSource,
        FourViewOperatorWrapperRawConsoleRestoreTracker, FourViewOperatorWrapperRawKeyReader,
        FourViewOperatorWrapperRawKeyRuntime, ObsFriendlyFourViewLoopWindowRenderRuntime,
        PersistentWindowLifecycleSnapshot, PreviewLoopRealHandoff, PreviewLoopRealHandoffCall,
        SwitcherFourViewControlledPreviewCommand, SwitcherFourViewControlledPreviewCommandSummary,
        SwitcherFrameCadenceSleepHook, SwitcherPersistentWindowLoopRuntimeHook,
        SwitcherQueuedFrameHandoffResult, SwitcherSingleClientQueueSourceMode,
        TwoRealPreviewLoopRuntimeTiming, FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
        FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH, FOUR_VIEW_CLEAN_OUTPUT_LOOP_SCALE_MODE,
        REUSABLE_OBS_RENDER_BUFFER,
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
            parse_handoff_mode_or_exit(Some("preview-latest-decodable".to_string()), "mode"),
            SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable
        );
        assert_eq!(
            parse_handoff_mode_or_exit(Some("consume-oldest".to_string()), "mode"),
            SwitcherSingleClientQueueSourceMode::ConsumeOldest
        );
    }

    #[test]
    fn switcher_optional_real_handoff_preview_mode_accepts_preview_latest_decodable() {
        assert_eq!(
            parse_optional_real_handoff_preview_mode_or_exit(Some(
                "preview-latest-decodable".to_string()
            )),
            SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable
        );
    }

    #[test]
    fn switcher_optional_real_handoff_preview_mode_defaults_to_preview_latest() {
        assert_eq!(
            parse_optional_real_handoff_preview_mode_or_exit(None),
            SwitcherSingleClientQueueSourceMode::PreviewLatest
        );
    }

    #[test]
    fn switcher_preview_latest_decodable_maps_to_expected_handoff_and_target_time_modes() {
        assert_eq!(
            handoff_read_mode_from_switcher_mode(
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable
            ),
            stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatestDecodable
        );
        assert_eq!(
            preview_target_time_mode_from_switcher_mode(
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable
            ),
            stream_sync_switcher::SwitcherSingleClientTargetTimeSourceMode::PreviewLatestDecodableIfAtOrBefore
        );
    }

    #[test]
    fn switcher_handoff_formats_mode_names() {
        assert_eq!(
            format_handoff_mode(SwitcherSingleClientQueueSourceMode::PreviewLatest),
            "preview-latest"
        );
        assert_eq!(
            format_handoff_mode(SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable),
            "preview-latest-decodable"
        );
        assert_eq!(
            format_handoff_read_mode(
                stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatest
            ),
            "inspect-latest"
        );
        assert_eq!(
            format_handoff_read_mode(
                stream_sync_net_core::ServerSwitcherQueuedFrameReadMode::InspectLatestDecodable
            ),
            "inspect-latest-decodable"
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
            r"\\.\pipe\pipe-a",
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
            "FrameRead",
            "3",
            "none",
            "none",
            &result,
        );

        assert!(summary.contains("request_id=88"));
        assert!(summary.contains(r"actual_pipe_path=\\.\pipe\pipe-a"));
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
                actual_pipe_path: Some(r"\\.\pipe\pipe-b".to_string()),
                request_id: 9,
                read_mode: SwitcherSingleClientQueueSourceMode::ConsumeOldest,
                attempt_count: 1,
                timeout_millis: 2500,
                request_status: SwitcherNamedPipeQueuedFrameHandoffRequestStatus::EncodeFailed,
                response_status: SwitcherNamedPipeQueuedFrameHandoffResponseStatus::None,
                result_kind: SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError,
                final_result: SwitcherNamedPipeQueuedFrameHandoffResultKind::HandoffError,
                last_error: Some(SwitcherQueuedFrameHandoffError::MalformedResponse),
                local_error: Some("encode_request:BodyTooLong".to_string()),
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
    fn switcher_four_view_real_slot_index_parses_zero_to_three() {
        assert_eq!(
            parse_four_view_real_slot_index_or_exit(Some("0".to_string())),
            0
        );
        assert_eq!(
            parse_four_view_real_slot_index_or_exit(Some("3".to_string())),
            3
        );
    }

    #[test]
    fn switcher_four_view_real_slot_indices_must_be_distinct() {
        validate_distinct_four_view_real_slot_indices(0, 1)
            .expect("distinct real slot indices should be accepted");

        let error = validate_distinct_four_view_real_slot_indices(2, 2)
            .expect_err("duplicate real slot indices should be rejected");
        assert_eq!(
            error,
            "invalid slot indices: slot0-index and slot1-index must be distinct"
        );
    }

    #[test]
    fn switcher_four_view_control_script_parses_optional_commands_flag() {
        assert_eq!(
            parse_optional_four_view_control_script(vec![
                "--commands".to_string(),
                "status;focus 0;quit".to_string()
            ])
            .expect("script flag should parse"),
            Some("status;focus 0;quit".to_string())
        );
        assert_eq!(
            parse_optional_four_view_control_script(Vec::new())
                .expect("missing script should stay optional"),
            None
        );
    }

    #[test]
    fn switcher_four_view_control_command_source_parses_scripted_and_control_pipe_modes() {
        assert_eq!(
            parse_four_view_control_command_source(vec![
                "--commands".to_string(),
                "status;quit".to_string(),
            ])
            .expect("scripted source should parse"),
            FourViewControlCommandSource::Scripted("status;quit".to_string())
        );
        assert_eq!(
            parse_four_view_control_command_source(vec![
                "--control-pipe".to_string(),
                "streamsync-control-dev".to_string(),
            ])
            .expect("control pipe source should parse"),
            FourViewControlCommandSource::ControlPipe("streamsync-control-dev".to_string())
        );
        assert_eq!(
            parse_four_view_control_command_source(Vec::new())
                .expect("empty control args should default to stdin"),
            FourViewControlCommandSource::Stdin
        );
    }

    #[test]
    fn switcher_four_view_control_command_source_rejects_mixed_optional_modes() {
        let error = parse_four_view_control_command_source(vec![
            "--commands".to_string(),
            "status".to_string(),
            "--control-pipe".to_string(),
            "streamsync-control-dev".to_string(),
        ])
        .expect_err("mixed control sources should be rejected");

        assert!(error.contains("use either --commands"));
    }

    #[test]
    fn switcher_four_view_operator_wrapper_input_source_parses_scripted_keys_mode() {
        assert_eq!(
            parse_four_view_operator_wrapper_input_source(vec![
                "--keys".to_string(),
                "s;1;2;q;q".to_string()
            ])
            .expect("scripted keys should parse"),
            FourViewOperatorWrapperInputSource::ScriptedKeys("s;1;2;q;q".to_string())
        );
        assert_eq!(
            parse_four_view_operator_wrapper_input_source(Vec::new())
                .expect("missing keys should default to stdin"),
            FourViewOperatorWrapperInputSource::Stdin
        );
    }

    #[test]
    fn switcher_four_view_operator_wrapper_input_source_parses_raw_keys_mode() {
        assert_eq!(
            parse_four_view_operator_wrapper_input_source(vec!["--raw-keys".to_string()])
                .expect("raw keys should parse"),
            FourViewOperatorWrapperInputSource::RawKeys
        );
    }

    #[test]
    fn switcher_four_view_control_command_parser_accepts_expected_commands() {
        assert_eq!(
            parse_four_view_control_command("all").expect("all should parse"),
            SwitcherFourViewControlledPreviewCommand::All
        );
        assert_eq!(
            parse_four_view_control_command("status").expect("status should parse"),
            SwitcherFourViewControlledPreviewCommand::Status
        );
        assert_eq!(
            parse_four_view_control_command("focus 3").expect("focus 3 should parse"),
            SwitcherFourViewControlledPreviewCommand::Focus(3)
        );
        assert_eq!(
            parse_four_view_control_command("quit").expect("quit should parse"),
            SwitcherFourViewControlledPreviewCommand::Quit
        );
    }

    #[test]
    fn switcher_four_view_control_command_parser_rejects_invalid_focus_index() {
        let error =
            parse_four_view_control_command("focus 7").expect_err("focus 7 should be rejected");

        assert_eq!(error.command_name, "focus");
        assert_eq!(error.requested_transition, "Focused(7)");
        assert_eq!(error.message, "invalid focus index: expected integer 0..3");
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
            let render = SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                width: request.frame.width,
                height: request.frame.height,
                title: request.title.clone(),
                hold_millis: request.hold_millis,
            });
            recycle_obs_render_buffer(request.frame.pixels);
            render
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
        fn render_once(&self, request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            let mut lifecycle = self.lifecycle.borrow_mut();
            if !lifecycle.window_created {
                lifecycle.window_created = true;
                lifecycle.persistent_window = true;
                *self.create_calls.borrow_mut() += 1;
            }
            lifecycle.window_updates += 1;
            let result = SwitcherWindowRenderResult::RenderFailed {
                message: "fixture render failed".to_string(),
            };
            recycle_obs_render_buffer(request.frame.pixels);
            result
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedWindowRenderSnapshot {
        title: String,
        width: u32,
        height: u32,
        top_left: [u8; 4],
        top_right: [u8; 4],
        bottom_left: [u8; 4],
        bottom_right: [u8; 4],
    }

    impl RecordedWindowRenderSnapshot {
        fn unique_corner_count(&self) -> usize {
            let mut corners = vec![
                self.top_left,
                self.top_right,
                self.bottom_left,
                self.bottom_right,
            ];
            corners.sort();
            corners.dedup();
            corners.len()
        }
    }

    #[derive(Debug, Default)]
    struct RecordingPersistentFixtureRenderedWindowRuntime {
        lifecycle: RefCell<PersistentWindowLifecycleSnapshot>,
        requests: RefCell<Vec<RecordedWindowRenderSnapshot>>,
    }

    impl SwitcherWindowRenderRuntimeHook for RecordingPersistentFixtureRenderedWindowRuntime {
        fn render_once(&self, request: SwitcherWindowRenderRequest) -> SwitcherWindowRenderResult {
            let mut lifecycle = self.lifecycle.borrow_mut();
            if !lifecycle.window_created {
                lifecycle.window_created = true;
                lifecycle.persistent_window = true;
            }
            lifecycle.window_updates += 1;
            self.requests
                .borrow_mut()
                .push(record_window_render_snapshot(&request));
            let render = SwitcherWindowRenderResult::Rendered(SwitcherWindowRenderSuccess {
                width: request.frame.width,
                height: request.frame.height,
                title: request.title.clone(),
                hold_millis: request.hold_millis,
            });
            recycle_obs_render_buffer(request.frame.pixels);
            render
        }
    }

    impl SwitcherPersistentWindowLoopRuntimeHook for RecordingPersistentFixtureRenderedWindowRuntime {
        fn close_persistent_window(&self) {
            self.lifecycle.borrow_mut().window_closed = true;
        }

        fn lifecycle_snapshot(&self) -> PersistentWindowLifecycleSnapshot {
            *self.lifecycle.borrow()
        }
    }

    fn record_window_render_snapshot(
        request: &SwitcherWindowRenderRequest,
    ) -> RecordedWindowRenderSnapshot {
        RecordedWindowRenderSnapshot {
            title: request.title.clone(),
            width: request.frame.width,
            height: request.frame.height,
            top_left: sample_bgra_pixel(&request.frame.pixels, request.frame.width, 0, 0),
            top_right: sample_bgra_pixel(
                &request.frame.pixels,
                request.frame.width,
                request.frame.width.saturating_sub(1),
                0,
            ),
            bottom_left: sample_bgra_pixel(
                &request.frame.pixels,
                request.frame.width,
                0,
                request.frame.height.saturating_sub(1),
            ),
            bottom_right: sample_bgra_pixel(
                &request.frame.pixels,
                request.frame.width,
                request.frame.width.saturating_sub(1),
                request.frame.height.saturating_sub(1),
            ),
        }
    }

    fn sample_bgra_pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * width + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    }

    #[derive(Debug, Default)]
    struct PerClientStubRealQueuedFrameHandoff {
        calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for PerClientStubRealQueuedFrameHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            *self.calls.borrow_mut() += 1;
            let seed = match input.client_id.0.as_str() {
                "real-client-0" => 0x10,
                "real-client-1" => 0x40,
                "real-client-2" => 0x70,
                "real-client-3" => 0xa0,
                _ => 0xdd,
            };
            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id: 1,
                    capture_timestamp: TimestampMicros(1_000_001),
                    send_timestamp: TimestampMicros(1_000_101),
                    queued_at: TimestampMicros(2_400_001),
                    is_keyframe: true,
                    width: 2,
                    height: 2,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![seed],
                },
                mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Default)]
    struct ObsProfileSizedPerClientHandoff {
        calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for ObsProfileSizedPerClientHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            *self.calls.borrow_mut() += 1;
            let seed = match input.client_id.0.as_str() {
                "real-client-0" => 0x10,
                "real-client-1" => 0x40,
                "real-client-2" => 0x70,
                "real-client-3" => 0xa0,
                _ => 0xdd,
            };
            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id: 1,
                    capture_timestamp: TimestampMicros(1_000_001),
                    send_timestamp: TimestampMicros(1_000_101),
                    queued_at: TimestampMicros(2_400_001),
                    is_keyframe: true,
                    width: FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
                    height: FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![seed],
                },
                mode: input.mode,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Default)]
    struct IncrementingFramePerClientHandoff {
        client0_calls: RefCell<u32>,
        client1_calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for IncrementingFramePerClientHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            let (calls, seed_base) = match input.client_id.0.as_str() {
                "real-client-0" => (&self.client0_calls, 0x10),
                "real-client-1" => (&self.client1_calls, 0x40),
                _ => (&self.client0_calls, 0xdd),
            };
            let mut call_count = calls.borrow_mut();
            let frame_id = *call_count as u64 + 1;
            *call_count += 1;
            let seed = seed_base + frame_id as u8 - 1;
            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id,
                    capture_timestamp: TimestampMicros(1_000_000 + frame_id),
                    send_timestamp: TimestampMicros(1_000_100 + frame_id),
                    queued_at: TimestampMicros(2_400_000 + frame_id),
                    is_keyframe: true,
                    width: 2,
                    height: 1,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![seed],
                },
                mode: input.mode,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Default)]
    struct SamePayloadIncrementingFramePerClientHandoff {
        client0_calls: RefCell<u32>,
        client1_calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for SamePayloadIncrementingFramePerClientHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            let (calls, seed) = match input.client_id.0.as_str() {
                "real-client-0" => (&self.client0_calls, 0x10),
                "real-client-1" => (&self.client1_calls, 0x40),
                _ => (&self.client0_calls, 0xdd),
            };
            let mut call_count = calls.borrow_mut();
            let frame_id = *call_count as u64 + 1;
            *call_count += 1;
            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id,
                    capture_timestamp: TimestampMicros(1_000_000 + frame_id),
                    send_timestamp: TimestampMicros(1_000_100 + frame_id),
                    queued_at: TimestampMicros(2_400_000 + frame_id),
                    is_keyframe: true,
                    width: 2,
                    height: 1,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![seed],
                },
                mode: input.mode,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Default)]
    struct FirstReadThenHandoffErrorPerClientHandoff {
        client0_calls: RefCell<u32>,
        client1_calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for FirstReadThenHandoffErrorPerClientHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            let (calls, seed) = match input.client_id.0.as_str() {
                "real-client-0" => (&self.client0_calls, 0x10),
                "real-client-1" => (&self.client1_calls, 0x40),
                _ => (&self.client0_calls, 0xdd),
            };
            let mut call_count = calls.borrow_mut();
            let current_call = *call_count;
            *call_count += 1;

            if current_call > 0 {
                return SwitcherQueuedFrameHandoffResult::HandoffError {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    mode: input.mode,
                    error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                };
            }

            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id: 1,
                    capture_timestamp: TimestampMicros(1_000_001),
                    send_timestamp: TimestampMicros(1_000_101),
                    queued_at: TimestampMicros(2_400_001),
                    is_keyframe: true,
                    width: 2,
                    height: 1,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![seed],
                },
                mode: input.mode,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Default)]
    struct FirstReadThenErrorThenSameFramePerClientHandoff {
        client0_calls: RefCell<u32>,
        client1_calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for FirstReadThenErrorThenSameFramePerClientHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            let (calls, seed) = match input.client_id.0.as_str() {
                "real-client-0" => (&self.client0_calls, 0x10),
                "real-client-1" => (&self.client1_calls, 0x40),
                _ => (&self.client0_calls, 0xdd),
            };
            let mut call_count = calls.borrow_mut();
            let current_call = *call_count;
            *call_count += 1;

            if current_call == 1 {
                return SwitcherQueuedFrameHandoffResult::HandoffError {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    mode: input.mode,
                    error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                };
            }

            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id: 1,
                    capture_timestamp: TimestampMicros(1_000_001),
                    send_timestamp: TimestampMicros(1_000_101),
                    queued_at: TimestampMicros(2_400_001),
                    is_keyframe: true,
                    width: 2,
                    height: 1,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![seed],
                },
                mode: input.mode,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Default)]
    struct Slot0SecondReadErrorSlot2StableHandoff {
        client0_calls: RefCell<u32>,
        client1_calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for Slot0SecondReadErrorSlot2StableHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            let (calls, seed) = match input.client_id.0.as_str() {
                "real-client-0" => (&self.client0_calls, 0x10),
                "real-client-1" => (&self.client1_calls, 0x40),
                _ => (&self.client0_calls, 0xdd),
            };
            let mut call_count = calls.borrow_mut();
            let current_call = *call_count;
            *call_count += 1;

            if input.client_id.0 == "real-client-0" && current_call > 0 {
                return SwitcherQueuedFrameHandoffResult::HandoffError {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    mode: input.mode,
                    error: SwitcherQueuedFrameHandoffError::SourceUnavailable,
                };
            }

            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id: 1,
                    capture_timestamp: TimestampMicros(1_000_001),
                    send_timestamp: TimestampMicros(1_000_101),
                    queued_at: TimestampMicros(2_400_001),
                    is_keyframe: true,
                    width: 2,
                    height: 1,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![seed],
                },
                mode: input.mode,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Clone)]
    struct StubRealQueuedFrameHandoff {
        result: SwitcherQueuedFrameHandoffResult,
        calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for StubRealQueuedFrameHandoff {
        fn read_handoff_frame(
            &mut self,
            _input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            *self.calls.borrow_mut() += 1;
            self.result.clone()
        }
    }

    fn observed_frame_read_request_output(
        pipe_name: &str,
        input: &SwitcherQueuedFrameHandoffInput,
        request_id: u64,
        frame: &SwitcherSingleViewSelectedEncodedFrame,
        decodable_source: stream_sync_net_core::ServerSwitcherQueuedFrameDecodableSource,
        retained_keyframe_available: bool,
        retained_keyframe_frame_id: Option<u64>,
    ) -> SwitcherNamedPipeQueuedFrameHandoffRequestOutput {
        let result = SwitcherQueuedFrameHandoffResult::FrameRead {
            frame: frame.clone(),
            mode: input.mode,
            remaining_client_queue_len: 0,
        };
        let response = stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
            request_id,
            remaining_client_queue_len: 0,
            decodable_source,
            retained_keyframe_available,
            retained_keyframe_frame_id,
            frame: stream_sync_net_core::ServerSwitcherQueuedFrameHandoffFrame {
                client_id: frame.client_id.clone(),
                run_id: frame.run_id.clone(),
                frame_id: frame.frame_id,
                capture_timestamp: frame.capture_timestamp,
                send_timestamp: frame.send_timestamp,
                queued_at: frame.queued_at,
                is_keyframe: frame.is_keyframe,
                width: frame.width,
                height: frame.height,
                fps_nominal: frame.fps_nominal,
                codec: frame.codec,
                encoded_payload_len: frame.encoded_payload_len as u32,
                encoded_payload: frame.encoded_payload.clone(),
            },
        };

        SwitcherNamedPipeQueuedFrameHandoffRequestOutput {
            summary: SwitcherNamedPipeQueuedFrameHandoffRequestSummary {
                pipe_name: pipe_name.to_string(),
                actual_pipe_path: Some(format!(r"\\.\pipe\{pipe_name}")),
                request_id,
                read_mode: input.mode,
                attempt_count: 1,
                timeout_millis: 5_000,
                request_status: SwitcherNamedPipeQueuedFrameHandoffRequestStatus::Sent,
                response_status: SwitcherNamedPipeQueuedFrameHandoffResponseStatus::Decoded,
                result_kind: SwitcherNamedPipeQueuedFrameHandoffResultKind::FrameRead,
                final_result: SwitcherNamedPipeQueuedFrameHandoffResultKind::FrameRead,
                last_error: None,
                local_error: None,
                retry_classification: None,
                elapsed_millis: 0,
            },
            runtime: Some(
                stream_sync_switcher::SwitcherNamedPipeQueuedFrameHandoffRuntimeOutput {
                    request: stream_sync_net_core::ServerSwitcherQueuedFrameHandoffRequest {
                        handoff_version: stream_sync_net_core::SERVER_SWITCHER_HANDOFF_VERSION,
                        request_id,
                        client_id: input.client_id.clone(),
                        run_id: input.run_id.clone(),
                        read_mode: handoff_read_mode_from_switcher_mode(input.mode),
                    },
                    response: Some(response),
                    response_payload_len: Some(frame.encoded_payload_len),
                    parse_error: None,
                    io_error: None,
                    result: result.clone(),
                },
            ),
            result,
        }
    }

    #[derive(Debug, Default)]
    struct SourceFlappingObservedPerClientHandoff {
        client0_calls: RefCell<u32>,
        client1_calls: RefCell<u32>,
    }

    impl PreviewLoopRealHandoff for SourceFlappingObservedPerClientHandoff {
        fn read_handoff_frame_with_observation(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> PreviewLoopRealHandoffCall {
            let (calls, seed, request_id_base) = match input.client_id.0.as_str() {
                "real-client-0" => (&self.client0_calls, 0x10, 100u64),
                "real-client-1" => (&self.client1_calls, 0x40, 200u64),
                _ => (&self.client0_calls, 0xdd, 900u64),
            };
            let mut call_count = calls.borrow_mut();
            let current_call = *call_count;
            *call_count += 1;

            let frame = SwitcherSingleViewSelectedEncodedFrame {
                client_id: input.client_id.clone(),
                run_id: input.run_id.clone(),
                frame_id: 1,
                capture_timestamp: TimestampMicros(1_000_001),
                send_timestamp: TimestampMicros(1_000_101),
                queued_at: TimestampMicros(2_400_001),
                is_keyframe: true,
                width: 2,
                height: 2,
                fps_nominal: 30,
                codec: Codec::H264,
                encoded_payload_len: 1,
                encoded_payload: vec![seed],
            };
            let decodable_source = if current_call == 0 {
                stream_sync_net_core::ServerSwitcherQueuedFrameDecodableSource::Queue
            } else {
                stream_sync_net_core::ServerSwitcherQueuedFrameDecodableSource::RetainedKeyframe
            };
            let request_output = observed_frame_read_request_output(
                "fixture-pipe",
                &input,
                request_id_base + current_call as u64,
                &frame,
                decodable_source,
                true,
                Some(frame.frame_id),
            );
            let result = SwitcherQueuedFrameHandoffResult::FrameRead {
                frame,
                mode: input.mode,
                remaining_client_queue_len: 0,
            };

            PreviewLoopRealHandoffCall {
                request_output: Some(request_output),
                result,
            }
        }
    }

    #[derive(Debug, Clone)]
    struct RecordingInputModeQueuedFrameHandoff {
        modes: std::rc::Rc<RefCell<Vec<SwitcherSingleClientQueueSourceMode>>>,
    }

    impl SwitcherQueuedFrameHandoff for RecordingInputModeQueuedFrameHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            self.modes.borrow_mut().push(input.mode);
            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id: 21,
                    capture_timestamp: TimestampMicros(1_000_001),
                    send_timestamp: TimestampMicros(1_000_101),
                    queued_at: TimestampMicros(2_400_001),
                    is_keyframe: true,
                    width: 2,
                    height: 2,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![0x88],
                },
                mode: input.mode,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Default)]
    struct FirstReadThenNoFramePerClientHandoff {
        client0_calls: RefCell<u32>,
        client1_calls: RefCell<u32>,
    }

    impl SwitcherQueuedFrameHandoff for FirstReadThenNoFramePerClientHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            let (calls, seed) = match input.client_id.0.as_str() {
                "real-client-0" => (&self.client0_calls, 0x10),
                "real-client-1" => (&self.client1_calls, 0x40),
                _ => (&self.client0_calls, 0xdd),
            };
            let mut call_count = calls.borrow_mut();
            let current_call = *call_count;
            *call_count += 1;

            if current_call > 0 {
                return SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    mode: input.mode,
                    client_queue_len: 0,
                };
            }

            SwitcherQueuedFrameHandoffResult::FrameRead {
                frame: SwitcherSingleViewSelectedEncodedFrame {
                    client_id: input.client_id,
                    run_id: input.run_id,
                    frame_id: current_call as u64 + 1,
                    capture_timestamp: TimestampMicros(1_000_001),
                    send_timestamp: TimestampMicros(1_000_101),
                    queued_at: TimestampMicros(2_400_001),
                    is_keyframe: true,
                    width: 2,
                    height: 1,
                    fps_nominal: 30,
                    codec: Codec::H264,
                    encoded_payload_len: 1,
                    encoded_payload: vec![seed],
                },
                mode: input.mode,
                remaining_client_queue_len: 0,
            }
        }
    }

    #[derive(Debug, Default)]
    struct AlwaysNoFramePerClientHandoff;

    impl SwitcherQueuedFrameHandoff for AlwaysNoFramePerClientHandoff {
        fn read_handoff_frame(
            &mut self,
            input: SwitcherQueuedFrameHandoffInput,
        ) -> SwitcherQueuedFrameHandoffResult {
            SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                client_id: input.client_id,
                run_id: input.run_id,
                mode: input.mode,
                client_queue_len: 0,
            }
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

    #[test]
    fn obs_render_buffer_reuses_single_retained_buffer() {
        REUSABLE_OBS_RENDER_BUFFER.with(|buffer| *buffer.borrow_mut() = None);

        let expected_len = (FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
            * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
            * 4) as usize;
        let (first, first_diagnostics) = take_reusable_obs_render_buffer(expected_len);
        let (second, second_diagnostics) = take_reusable_obs_render_buffer(expected_len);

        assert_eq!(first_diagnostics.allocation_count, 1);
        assert_eq!(first_diagnostics.reuse_count, 0);
        assert_eq!(second_diagnostics.allocation_count, 1);
        assert_eq!(second_diagnostics.reuse_count, 0);

        recycle_obs_render_buffer(first);
        recycle_obs_render_buffer(second);

        let (_third, third_diagnostics) = take_reusable_obs_render_buffer(expected_len);
        let (_fourth, fourth_diagnostics) = take_reusable_obs_render_buffer(expected_len);

        assert_eq!(third_diagnostics.allocation_count, 0);
        assert_eq!(third_diagnostics.reuse_count, 1);
        assert_eq!(fourth_diagnostics.allocation_count, 1);
        assert_eq!(fourth_diagnostics.reuse_count, 0);

        REUSABLE_OBS_RENDER_BUFFER.with(|buffer| *buffer.borrow_mut() = None);
    }

    #[test]
    fn obs_scale_helper_records_same_size_copy_path() {
        REUSABLE_OBS_RENDER_BUFFER.with(|buffer| *buffer.borrow_mut() = None);

        let expected_len = (FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
            * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
            * 4) as usize;
        let source = vec![0x2a; expected_len];
        let (scaled, diagnostics) = scale_four_view_bgra_to_obs_validation_profile_from_slice(
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
            SwitcherDecodedFramePixelFormat::Bgra8,
            &source,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
        );

        assert_eq!(scaled.width, FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH);
        assert_eq!(scaled.height, FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT);
        assert_eq!(scaled.pixels.len(), expected_len);
        assert_eq!(diagnostics.bytes_copied_total, expected_len);
        assert_eq!(diagnostics.same_size_copy_count, 1);
        assert_eq!(diagnostics.half_scale_count, 0);
        assert_eq!(diagnostics.generic_scale_count, 0);
        assert_eq!(diagnostics.passthrough_count, 0);

        REUSABLE_OBS_RENDER_BUFFER.with(|buffer| *buffer.borrow_mut() = None);
    }

    #[test]
    fn obs_scale_helper_records_half_scale_path() {
        REUSABLE_OBS_RENDER_BUFFER.with(|buffer| *buffer.borrow_mut() = None);

        let source_width = FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH * 2;
        let source_height = FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT * 2;
        let source_len = (source_width * source_height * 4) as usize;
        let source = vec![0x55; source_len];
        let (scaled, diagnostics) = scale_four_view_bgra_to_obs_validation_profile_from_slice(
            source_width,
            source_height,
            SwitcherDecodedFramePixelFormat::Bgra8,
            &source,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
        );

        assert_eq!(scaled.width, FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH);
        assert_eq!(scaled.height, FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT);
        assert_eq!(
            scaled.pixels.len(),
            (FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
                * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
                * 4) as usize
        );
        assert_eq!(diagnostics.same_size_copy_count, 0);
        assert_eq!(diagnostics.half_scale_count, 1);
        assert_eq!(diagnostics.generic_scale_count, 0);
        assert_eq!(diagnostics.passthrough_count, 0);

        REUSABLE_OBS_RENDER_BUFFER.with(|buffer| *buffer.borrow_mut() = None);
    }

    #[test]
    fn obs_scale_helper_half_scale_keeps_top_left_pixel_of_each_2x2_block() {
        REUSABLE_OBS_RENDER_BUFFER.with(|buffer| *buffer.borrow_mut() = None);

        let source = [
            [1, 2, 3, 255],
            [9, 9, 9, 255],
            [4, 5, 6, 255],
            [8, 8, 8, 255],
            [7, 7, 7, 255],
            [7, 7, 7, 255],
            [6, 6, 6, 255],
            [6, 6, 6, 255],
            [10, 11, 12, 255],
            [5, 5, 5, 255],
            [13, 14, 15, 255],
            [4, 4, 4, 255],
            [3, 3, 3, 255],
            [3, 3, 3, 255],
            [2, 2, 2, 255],
            [2, 2, 2, 255],
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        let (scaled, diagnostics) = scale_four_view_bgra_to_obs_validation_profile_from_slice(
            4,
            4,
            SwitcherDecodedFramePixelFormat::Bgra8,
            &source,
            2,
            2,
        );

        assert_eq!(diagnostics.half_scale_count, 1);
        assert_eq!(
            sample_bgra_pixel(&scaled.pixels, scaled.width, 0, 0),
            [1, 2, 3, 255]
        );
        assert_eq!(
            sample_bgra_pixel(&scaled.pixels, scaled.width, 1, 0),
            [4, 5, 6, 255]
        );
        assert_eq!(
            sample_bgra_pixel(&scaled.pixels, scaled.width, 0, 1),
            [10, 11, 12, 255]
        );
        assert_eq!(
            sample_bgra_pixel(&scaled.pixels, scaled.width, 1, 1),
            [13, 14, 15, 255]
        );

        REUSABLE_OBS_RENDER_BUFFER.with(|buffer| *buffer.borrow_mut() = None);
    }

    #[test]
    fn obs_render_runtime_records_passthrough_when_frame_already_matches_obs_profile() {
        #[derive(Debug, Default)]
        struct RecordingRender;

        impl SwitcherWindowRenderRuntimeHook for RecordingRender {
            fn render_once(
                &self,
                request: SwitcherWindowRenderRequest,
            ) -> SwitcherWindowRenderResult {
                SwitcherWindowRenderResult::Rendered(
                    stream_sync_switcher::SwitcherWindowRenderSuccess {
                        width: request.frame.width,
                        height: request.frame.height,
                        title: request.title,
                        hold_millis: request.hold_millis,
                    },
                )
            }
        }

        let timing = Rc::new(RefCell::new(TwoRealPreviewLoopRuntimeTiming::default()));
        let runtime = RecordingRender;
        let obs_runtime =
            ObsFriendlyFourViewLoopWindowRenderRuntime::with_timing(&runtime, Rc::clone(&timing));
        let request = SwitcherWindowRenderRequest {
            frame: stream_sync_switcher::SwitcherDecodedFrameRenderInput {
                width: FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH,
                height: FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT,
                pixel_format: SwitcherDecodedFramePixelFormat::Bgra8,
                pixels: vec![
                    0;
                    (FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
                        * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
                        * 4) as usize
                ],
            },
            title: "StreamSync 4-view Output".to_string(),
            hold_millis: 0,
        };

        let result = obs_runtime.render_once(request);

        assert!(matches!(result, SwitcherWindowRenderResult::Rendered(_)));
        let timing = timing.borrow().clone();
        assert_eq!(timing.render_buffer_passthrough_count, 1);
        assert_eq!(timing.render_buffer_same_size_copy_count, 0);
        assert_eq!(timing.render_buffer_half_scale_count, 0);
        assert_eq!(timing.render_buffer_generic_scale_count, 0);
        assert_eq!(timing.render_buffer_scale_loop_elapsed_ms, 0);
    }

    #[test]
    fn switcher_four_view_real_handoff_preview_loop_uses_one_real_slot_and_three_placeholders() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            1,
            ClientId("real-client".to_string()),
            RunId("real-run".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client".to_string()),
                        run_id: RunId("real-run".to_string()),
                        frame_id: 1,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 1,
                        encoded_payload: vec![0x33],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.real_slot_index, 1);
        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.render_failures, 0);
        assert_eq!(
            summary.slot_bindings,
            [
                "0:fixture-placeholder-slot-0/fixture-placeholder-run-0".to_string(),
                "1:real-client/real-run".to_string(),
                "2:fixture-placeholder-slot-2/fixture-placeholder-run-2".to_string(),
                "3:fixture-placeholder-slot-3/fixture-placeholder-run-3".to_string(),
            ]
        );
        assert_eq!(
            summary.slot_result_kinds,
            [
                "NoFrameAvailable".to_string(),
                "Selected".to_string(),
                "NoFrameAvailable".to_string(),
                "NoFrameAvailable".to_string(),
            ]
        );
        assert_eq!(summary.clean_output_render_result_kind, "Rendered");
        assert_eq!(
            summary.window_title,
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        );
        assert_eq!(
            summary.output_width,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH)
        );
        assert_eq!(
            summary.output_height,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT)
        );
    }

    #[test]
    fn switcher_four_view_real_handoff_preview_summary_formats_expected_fields() {
        let summary = run_four_view_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client".to_string()),
            RunId("real-run".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client".to_string()),
                        run_id: RunId("real-run".to_string()),
                        frame_id: 1,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 1,
                        encoded_payload: vec![0x21],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &PersistentFixtureRenderFailedWindowRuntime::default(),
            &RecordingCadenceSleepHook::default(),
        );
        let formatted = format_four_view_real_handoff_preview_loop_summary(&summary);

        assert!(formatted.contains("command_name=--four-view-real-handoff-preview-loop"));
        assert!(formatted.contains("real_handoff=true"));
        assert!(formatted.contains("real_slot_count=1"));
        assert!(formatted.contains("real_slot_index=0"));
        assert!(formatted.contains("pipe_name=fixture-pipe"));
        assert!(formatted.contains(r"actual_pipe_path=\\.\pipe\fixture-pipe"));
        assert!(formatted.contains("client_id=real-client"));
        assert!(formatted.contains("run_id=real-run"));
        assert!(formatted.contains("scheduler_status=PartialSelected"));
        assert!(formatted.contains(
            "slot_result_kinds=Selected|NoFrameAvailable|NoFrameAvailable|NoFrameAvailable"
        ));
        assert!(formatted.contains("clean_output_render_result_kind=RenderFailed"));
        assert!(formatted.contains("window_title=StreamSync 4-view Output"));
        assert!(formatted.contains(&format!(
            "output_width={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
        )));
        assert!(formatted.contains(&format!(
            "output_height={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
        )));
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_uses_two_real_slots_and_two_placeholders() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            PerClientStubRealQueuedFrameHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.real_slot0_index, 0);
        assert_eq!(summary.real_slot1_index, 2);
        assert_eq!(
            summary.actual_pipe_path,
            r"\\.\pipe\fixture-pipe".to_string()
        );
        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.render_failures, 0);
        assert_eq!(summary.target_fps, 30);
        assert_eq!(summary.configured_frame_interval_ms, 33);
        assert_eq!(summary.first_render_attempt_index, Some(1));
        assert_eq!(summary.rendered_after_first_render, 2);
        assert_eq!(summary.no_render_before_first_render, 0);
        assert_eq!(summary.selected_count, 4);
        assert_eq!(summary.no_frame_count, 4);
        assert_eq!(summary.handoff_error_count, 0);
        assert_eq!(summary.decode_attempt_count, 2);
        assert_eq!(summary.decode_success_count, 2);
        assert_eq!(summary.render_success_count, 2);
        assert_eq!(summary.render_failure_count, 0);
        assert_eq!(summary.render_buffer_reuse_count, 1);
        assert_eq!(summary.render_buffer_allocation_count, 1);
        assert_eq!(
            summary.render_buffer_bytes_copied_total,
            (FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
                * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
                * 4) as usize
        );
        assert_eq!(summary.unchanged_frame_reuse_count, 2);
        assert_eq!(summary.skipped_decode_unchanged_frame_count, 2);
        assert_eq!(summary.redecoded_same_frame_count, 0);
        assert_eq!(summary.quad_view_compose_attempt_count, 1);
        assert_eq!(summary.quad_view_compose_success_count, 1);
        assert_eq!(summary.quad_view_compose_skipped_unchanged_count, 0);
        assert_eq!(summary.quad_view_composed_frame_reuse_count, 0);
        assert_eq!(summary.quad_view_visual_unchanged_count, 1);
        assert_eq!(summary.quad_view_visual_changed_count, 1);
        assert_eq!(summary.materialization_reason_first_render_count, 1);
        assert_eq!(summary.materialization_reason_visual_changed_count, 0);
        assert_eq!(summary.materialization_reason_force_render_count, 0);
        assert_eq!(summary.materialization_reason_unknown_count, 0);
        assert_eq!(summary.slot0_frame_id_changed_count, 0);
        assert_eq!(summary.slot2_frame_id_changed_count, 0);
        assert_eq!(summary.placeholder_visual_changed_count, 0);
        assert_eq!(summary.quad_view_incremental_update_count, 0);
        assert_eq!(summary.quad_view_full_compose_count, 1);
        assert_eq!(summary.quad_view_changed_slot_update_count, 4);
        assert_eq!(summary.quad_view_reused_slot_count, 0);
        assert_eq!(summary.quad_view_allocation_count, 1);
        assert_eq!(
            summary.slot_bindings,
            [
                "0:real-client-0/real-run-0".to_string(),
                "1:fixture-placeholder-slot-1/fixture-placeholder-run-1".to_string(),
                "2:real-client-1/real-run-1".to_string(),
                "3:fixture-placeholder-slot-3/fixture-placeholder-run-3".to_string(),
            ]
        );
        assert_eq!(
            summary.slot_result_kinds,
            [
                "Selected".to_string(),
                "NoFrameAvailable".to_string(),
                "Selected".to_string(),
                "NoFrameAvailable".to_string(),
            ]
        );
        assert_eq!(summary.clean_output_render_result_kind, "Rendered");
        assert_eq!(
            summary.window_title,
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        );
        assert_eq!(
            summary.output_width,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH)
        );
        assert_eq!(
            summary.output_height,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT)
        );
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_direct_compose_hits_same_size_copy_for_obs_profile(
    ) {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            ObsProfileSizedPerClientHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.quad_view_compose_attempt_count, 1);
        assert_eq!(summary.quad_view_compose_success_count, 1);
        assert_eq!(summary.quad_view_incremental_update_count, 0);
        assert_eq!(summary.quad_view_full_compose_count, 1);
        assert_eq!(summary.render_input_unchanged_count, 1);
        assert_eq!(summary.render_reuse_frame_count, 1);
        assert_eq!(summary.render_buffer_passthrough_count, 0);
        assert_eq!(summary.render_buffer_same_size_copy_count, 1);
        assert_eq!(summary.render_buffer_half_scale_count, 0);
        assert_eq!(summary.render_buffer_generic_scale_count, 0);
        assert_eq!(
            summary.render_buffer_bytes_copied_total,
            (FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
                * FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
                * 4) as usize
        );
        assert_eq!(
            summary.output_width,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH)
        );
        assert_eq!(
            summary.output_height,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT)
        );

        let requests = render_runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].width,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
        );
        assert_eq!(
            requests[0].height,
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
        );
        assert_eq!(requests[0].top_left, [0x10, 0x11, 0x12, 255]);
        assert_eq!(requests[0].top_right, [16, 16, 16, 255]);
        assert_eq!(requests[0].bottom_left, [0x40, 0x41, 0x42, 255]);
        assert_eq!(requests[0].bottom_right, [16, 16, 16, 255]);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_reuses_last_renderable_frames() {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            FirstReadThenNoFramePerClientHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.first_render_attempt_index, Some(1));
        assert_eq!(summary.rendered_after_first_render, 2);
        assert_eq!(summary.no_render_before_first_render, 0);
        assert_eq!(summary.selected_count, 2);
        assert_eq!(summary.no_frame_count, 6);
        assert_eq!(summary.decode_attempt_count, 2);
        assert_eq!(summary.decode_success_count, 2);
        assert_eq!(summary.render_success_count, 2);
        assert_eq!(summary.render_failure_count, 0);
        assert_eq!(summary.unchanged_frame_reuse_count, 0);
        assert_eq!(summary.skipped_decode_unchanged_frame_count, 0);
        assert_eq!(summary.quad_view_compose_attempt_count, 1);
        assert_eq!(summary.quad_view_compose_success_count, 1);
        assert_eq!(summary.quad_view_compose_skipped_unchanged_count, 0);
        assert_eq!(summary.quad_view_composed_frame_reuse_count, 0);
        assert_eq!(summary.quad_view_visual_unchanged_count, 1);
        assert_eq!(summary.quad_view_visual_changed_count, 1);
        assert_eq!(summary.materialization_reason_first_render_count, 1);
        assert_eq!(summary.materialization_reason_visual_changed_count, 0);
        assert_eq!(summary.materialization_reason_force_render_count, 0);
        assert_eq!(summary.quad_view_incremental_update_count, 0);
        assert_eq!(summary.quad_view_full_compose_count, 1);
        assert_eq!(summary.quad_view_changed_slot_update_count, 4);
        assert_eq!(summary.quad_view_reused_slot_count, 0);
        assert_eq!(summary.quad_view_allocation_count, 1);
        assert_eq!(summary.render_input_unchanged_count, 1);
        assert_eq!(summary.render_reuse_frame_count, 1);
        assert_eq!(summary.render_buffer_reuse_count, 1);
        assert_eq!(summary.render_buffer_allocation_count, 1);
        assert_eq!(
            summary.slot_result_kinds,
            [
                "NoFrameAvailable".to_string(),
                "NoFrameAvailable".to_string(),
                "NoFrameAvailable".to_string(),
                "NoFrameAvailable".to_string(),
            ]
        );

        let requests = render_runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].top_left, [0x10, 0x11, 0x12, 255]);
        assert_eq!(requests[0].bottom_left, [0x40, 0x41, 0x42, 255]);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_skips_decode_for_unchanged_selected_frames()
    {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            PerClientStubRealQueuedFrameHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.selected_count, 4);
        assert_eq!(summary.decode_attempt_count, 2);
        assert_eq!(summary.decode_success_count, 2);
        assert_eq!(summary.unchanged_frame_reuse_count, 2);
        assert_eq!(summary.skipped_decode_unchanged_frame_count, 2);
        assert_eq!(summary.redecoded_same_frame_count, 0);
        assert_eq!(summary.quad_view_compose_attempt_count, 1);
        assert_eq!(summary.quad_view_compose_success_count, 1);
        assert_eq!(summary.quad_view_compose_skipped_unchanged_count, 0);
        assert_eq!(summary.quad_view_composed_frame_reuse_count, 0);
        assert_eq!(summary.quad_view_visual_unchanged_count, 1);
        assert_eq!(summary.quad_view_visual_changed_count, 1);
        assert_eq!(summary.materialization_reason_first_render_count, 1);
        assert_eq!(summary.materialization_reason_visual_changed_count, 0);
        assert_eq!(summary.materialization_reason_force_render_count, 0);
        assert_eq!(summary.quad_view_incremental_update_count, 0);
        assert_eq!(summary.quad_view_full_compose_count, 1);
        assert_eq!(summary.quad_view_changed_slot_update_count, 4);
        assert_eq!(summary.quad_view_reused_slot_count, 0);
        assert_eq!(summary.quad_view_allocation_count, 1);
        assert_eq!(summary.render_input_unchanged_count, 1);
        assert_eq!(summary.render_reuse_frame_count, 1);
        assert_eq!(summary.render_buffer_reuse_count, 1);
        assert_eq!(summary.render_buffer_allocation_count, 1);
        assert!(summary.slot_diagnostics[0].contains("decode_attempted=false"));
        assert!(summary.slot_diagnostics[0].contains("decode_skipped_reason=UnchangedFrameReuse"));
        assert!(summary.slot_diagnostics[2].contains("decode_attempted=false"));
        assert!(summary.slot_diagnostics[2].contains("decode_skipped_reason=UnchangedFrameReuse"));

        let requests = render_runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].top_left, [0x10, 0x11, 0x12, 255]);
        assert_eq!(requests[0].bottom_left, [0x40, 0x41, 0x42, 255]);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_reuses_render_when_only_selected_source_changes(
    ) {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
            TimestampMicros(1_000_004),
            SourceFlappingObservedPerClientHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.decode_attempt_count, 2);
        assert_eq!(summary.skipped_decode_unchanged_frame_count, 2);
        assert_eq!(summary.quad_view_visual_unchanged_count, 1);
        assert_eq!(summary.quad_view_visual_changed_count, 1);
        assert_eq!(summary.materialization_reason_first_render_count, 1);
        assert_eq!(summary.materialization_reason_visual_changed_count, 0);
        assert_eq!(summary.materialization_reason_force_render_count, 0);
        assert_eq!(summary.slot0_frame_id_changed_count, 0);
        assert_eq!(summary.slot2_frame_id_changed_count, 0);
        assert_eq!(summary.slot0_selected_source_changed_count, 1);
        assert_eq!(summary.slot2_selected_source_changed_count, 1);
        assert_eq!(summary.placeholder_visual_changed_count, 0);
        assert_eq!(summary.render_input_unchanged_count, 1);
        assert_eq!(summary.render_reuse_frame_count, 1);
        assert_eq!(summary.render_buffer_reuse_count, 1);
        assert!(summary.slot_diagnostics[0].contains("selected_frame_source=retained_keyframe"));
        assert!(summary.slot_diagnostics[2].contains("selected_frame_source=retained_keyframe"));

        let requests = render_runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].top_left, [0x10, 0x11, 0x12, 255]);
        assert_eq!(requests[0].bottom_left, [0x40, 0x41, 0x42, 255]);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_redecodes_different_frame_ids() {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            IncrementingFramePerClientHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.selected_count, 4);
        assert_eq!(summary.decode_attempt_count, 4);
        assert_eq!(summary.decode_success_count, 4);
        assert_eq!(summary.unchanged_frame_reuse_count, 0);
        assert_eq!(summary.skipped_decode_unchanged_frame_count, 0);
        assert_eq!(summary.quad_view_compose_attempt_count, 2);
        assert_eq!(summary.quad_view_compose_success_count, 2);
        assert_eq!(summary.quad_view_compose_skipped_unchanged_count, 0);
        assert_eq!(summary.quad_view_composed_frame_reuse_count, 0);
        assert_eq!(summary.quad_view_visual_unchanged_count, 0);
        assert_eq!(summary.quad_view_visual_changed_count, 2);
        assert_eq!(summary.materialization_reason_first_render_count, 1);
        assert_eq!(summary.materialization_reason_visual_changed_count, 1);
        assert_eq!(summary.materialization_reason_force_render_count, 0);
        assert_eq!(summary.slot0_frame_id_changed_count, 1);
        assert_eq!(summary.slot2_frame_id_changed_count, 1);
        assert_eq!(summary.slot0_selected_source_changed_count, 0);
        assert_eq!(summary.slot2_selected_source_changed_count, 0);
        assert_eq!(summary.placeholder_visual_changed_count, 0);
        assert_eq!(summary.quad_view_incremental_update_count, 0);
        assert_eq!(summary.quad_view_full_compose_count, 2);
        assert_eq!(summary.quad_view_changed_slot_update_count, 6);
        assert_eq!(summary.quad_view_reused_slot_count, 2);
        assert_eq!(summary.quad_view_allocation_count, 1);
        assert_eq!(summary.render_buffer_reuse_count, 1);
        assert_eq!(summary.render_buffer_allocation_count, 1);

        let requests = render_runtime.requests.borrow();
        assert_eq!(requests.len(), 2);
        assert_ne!(requests[1].top_left, requests[0].top_left);
        assert_ne!(requests[1].bottom_left, requests[0].bottom_left);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_decodes_different_frame_ids_even_with_same_payload(
    ) {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            SamePayloadIncrementingFramePerClientHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.selected_count, 4);
        assert_eq!(summary.decode_attempt_count, 4);
        assert_eq!(summary.decode_success_count, 4);
        assert_eq!(summary.decode_cache_miss_count, 4);
        assert_eq!(summary.decode_cached_frame_reuse_count, 0);
        assert_eq!(summary.skipped_decode_unchanged_frame_count, 0);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_keeps_unchanged_frame_reuse_after_source_error(
    ) {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(3).expect("3 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            FirstReadThenErrorThenSameFramePerClientHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.selected_count, 4);
        assert_eq!(summary.handoff_error_count, 2);
        assert_eq!(summary.decode_attempt_count, 2);
        assert_eq!(summary.decode_success_count, 2);
        assert_eq!(summary.decode_cache_miss_count, 2);
        assert_eq!(summary.decode_cached_frame_reuse_count, 0);
        assert_eq!(summary.unchanged_frame_reuse_count, 2);
        assert_eq!(summary.skipped_decode_unchanged_frame_count, 2);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_does_not_reuse_source_error_as_content() {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            FirstReadThenHandoffErrorPerClientHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 1);
        assert_eq!(summary.handoff_error_count, 2);
        assert_eq!(summary.decode_attempt_count, 2);
        assert_eq!(summary.unchanged_frame_reuse_count, 0);
        assert_eq!(summary.skipped_decode_unchanged_frame_count, 0);
        assert_eq!(summary.quad_view_compose_attempt_count, 2);
        assert_eq!(summary.quad_view_compose_success_count, 1);
        assert_eq!(summary.quad_view_compose_skipped_unchanged_count, 0);
        assert_eq!(summary.quad_view_composed_frame_reuse_count, 0);
        assert_eq!(summary.quad_view_visual_unchanged_count, 0);
        assert_eq!(summary.quad_view_visual_changed_count, 2);
        assert_eq!(
            summary.slot_result_kinds,
            [
                "HandoffError".to_string(),
                "NoFrameAvailable".to_string(),
                "HandoffError".to_string(),
                "NoFrameAvailable".to_string(),
            ]
        );

        let requests = render_runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_updates_only_source_error_slot_region() {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            Slot0SecondReadErrorSlot2StableHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.handoff_error_count, 1);
        assert_eq!(summary.quad_view_compose_attempt_count, 2);
        assert_eq!(summary.quad_view_compose_success_count, 2);
        assert_eq!(summary.materialization_reason_first_render_count, 1);
        assert_eq!(summary.materialization_reason_visual_changed_count, 1);
        assert_eq!(summary.placeholder_visual_changed_count, 1);
        assert_eq!(summary.quad_view_incremental_update_count, 0);
        assert_eq!(summary.quad_view_full_compose_count, 2);
        assert_eq!(summary.quad_view_changed_slot_update_count, 5);
        assert_eq!(summary.quad_view_reused_slot_count, 3);
        assert_eq!(summary.quad_view_allocation_count, 1);
        assert_eq!(
            summary.slot_result_kinds,
            [
                "HandoffError".to_string(),
                "NoFrameAvailable".to_string(),
                "Selected".to_string(),
                "NoFrameAvailable".to_string(),
            ]
        );

        let requests = render_runtime.requests.borrow();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].top_left, [0x10, 0x11, 0x12, 255]);
        assert_eq!(requests[1].top_left, [16, 16, 16, 255]);
        assert_eq!(requests[1].bottom_left, requests[0].bottom_left);
        assert_eq!(requests[1].top_right, requests[0].top_right);
        assert_eq!(requests[1].bottom_right, requests[0].bottom_right);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_skips_stable_initial_no_frame_composition()
    {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(3).expect("3 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            AlwaysNoFramePerClientHandoff,
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 3);
        assert_eq!(summary.frames_rendered, 0);
        assert_eq!(summary.no_frame_count, 12);
        assert_eq!(summary.decode_attempt_count, 0);
        assert_eq!(summary.quad_view_compose_attempt_count, 1);
        assert_eq!(summary.quad_view_compose_success_count, 0);
        assert_eq!(summary.quad_view_compose_skipped_unchanged_count, 2);
        assert_eq!(summary.quad_view_composed_frame_reuse_count, 0);
        assert_eq!(summary.quad_view_visual_unchanged_count, 2);
        assert_eq!(summary.quad_view_visual_changed_count, 1);
        assert_eq!(summary.quad_view_incremental_update_count, 0);
        assert_eq!(summary.quad_view_full_compose_count, 0);
        assert_eq!(summary.quad_view_changed_slot_update_count, 0);
        assert_eq!(summary.quad_view_reused_slot_count, 8);
        assert_eq!(summary.quad_view_allocation_count, 0);
        assert_eq!(
            summary.clean_output_render_result_kind,
            "NoRenderableQuadView"
        );
        assert!(render_runtime.requests.borrow().is_empty());
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_reports_force_render_materialization_reason(
    ) {
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            PerClientStubRealQueuedFrameHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &PersistentFixtureRenderFailedWindowRuntime::default(),
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 0);
        assert_eq!(summary.render_failures, 2);
        assert_eq!(summary.render_input_unchanged_count, 1);
        assert_eq!(summary.render_reuse_frame_count, 0);
        assert_eq!(summary.materialization_reason_first_render_count, 1);
        assert_eq!(summary.materialization_reason_visual_changed_count, 0);
        assert_eq!(summary.materialization_reason_force_render_count, 1);
        assert_eq!(summary.materialization_reason_unknown_count, 0);
        assert_eq!(summary.slot0_frame_id_changed_count, 0);
        assert_eq!(summary.slot2_frame_id_changed_count, 0);
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_summary_formats_expected_fields() {
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            1,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            3,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            PerClientStubRealQueuedFrameHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &PersistentFixtureRenderFailedWindowRuntime::default(),
            &RecordingCadenceSleepHook::default(),
        );
        let formatted = format_four_view_two_real_handoff_preview_loop_summary(&summary);

        assert!(formatted.contains("command_name=--four-view-two-real-handoff-preview-loop"));
        assert!(formatted.contains("real_handoff=true"));
        assert!(formatted.contains("real_slot_count=2"));
        assert!(formatted.contains("real_slot0_index=1"));
        assert!(formatted.contains("real_slot1_index=3"));
        assert!(formatted.contains("pipe_name=fixture-pipe"));
        assert!(formatted.contains(r"actual_pipe_path=\\.\pipe\fixture-pipe"));
        assert!(formatted.contains("preview_mode=preview-latest"));
        assert!(formatted.contains("read_mode=inspect-latest"));
        assert!(formatted.contains("client0_id=real-client-0"));
        assert!(formatted.contains("run0_id=real-run-0"));
        assert!(formatted.contains("client1_id=real-client-1"));
        assert!(formatted.contains("run1_id=real-run-1"));
        assert!(formatted.contains("elapsed_ms="));
        assert!(formatted.contains("target_fps=30"));
        assert!(formatted.contains("configured_frame_interval_ms=33"));
        assert!(formatted.contains("effective_attempt_fps="));
        assert!(formatted.contains("effective_render_fps="));
        assert!(formatted.contains("first_render_attempt_index=none"));
        assert!(formatted.contains("first_render_elapsed_ms=none"));
        assert!(formatted.contains("rendered_after_first_render=0"));
        assert!(formatted.contains("effective_render_fps_after_first_render=n/a"));
        assert!(formatted.contains("no_render_before_first_render=1"));
        assert!(formatted.contains("selected_count=2"));
        assert!(formatted.contains("no_frame_count=2"));
        assert!(formatted.contains("handoff_error_count=0"));
        assert!(formatted.contains("decode_attempt_count=2"));
        assert!(formatted.contains("decode_success_count=2"));
        assert!(formatted.contains("render_success_count=0"));
        assert!(formatted.contains("render_failure_count=1"));
        assert!(formatted.contains("unchanged_frame_reuse_count=0"));
        assert!(formatted.contains("skipped_decode_unchanged_frame_count=0"));
        assert!(formatted.contains("redecoded_same_frame_count=0"));
        assert!(formatted.contains("decode_elapsed_ms="));
        assert!(formatted.contains("decode_process_spawn_elapsed_ms="));
        assert!(formatted.contains("decode_input_write_elapsed_ms="));
        assert!(formatted.contains("decode_input_payload_bytes_total="));
        assert!(formatted.contains("decode_output_read_elapsed_ms="));
        assert!(formatted.contains("decode_output_read_exact_elapsed_ms="));
        assert!(formatted.contains("decode_output_vec_resize_elapsed_ms="));
        assert!(formatted.contains("decode_process_wait_elapsed_ms="));
        assert!(formatted.contains("decode_pixel_convert_elapsed_ms=0"));
        assert!(formatted.contains("decode_buffer_allocation_count="));
        assert!(formatted.contains("decode_output_bytes_total="));
        assert!(formatted.contains("decode_stdout_expected_bytes_total="));
        assert!(formatted.contains("decode_cached_frame_reuse_count=0"));
        assert!(formatted.contains("decode_cache_miss_count=2"));
        assert!(formatted.contains("decoded_buffer_clone_count="));
        assert!(formatted.contains("decode_cache_hit_clone_count="));
        assert!(formatted.contains("decode_cache_store_clone_count="));
        assert!(formatted.contains("decoded_buffer_clone_elapsed_ms="));
        assert!(formatted.contains("composed_buffer_clone_count="));
        assert!(formatted.contains("decode_output_buffer_reuse_count=0"));
        assert!(formatted.contains("persistent_decode_enabled=false"));
        assert!(formatted.contains("persistent_decode_attempt_count=0"));
        assert!(formatted.contains("persistent_decode_success_count=0"));
        assert!(formatted.contains("persistent_decode_failure_count=0"));
        assert!(formatted.contains("persistent_decode_fallback_count=0"));
        assert!(formatted.contains("persistent_decode_process_spawn_count=0"));
        assert!(formatted.contains("persistent_decode_process_restart_count=0"));
        assert!(formatted.contains("persistent_decode_last_error=none"));
        assert!(formatted.contains("one_shot_decode_fallback_count=0"));
        assert!(formatted.contains("handoff_elapsed_ms="));
        assert!(formatted.contains("render_elapsed_ms="));
        assert!(formatted.contains("avg_decode_elapsed_ms="));
        assert!(formatted.contains("avg_decode_input_write_elapsed_ms="));
        assert!(formatted.contains("avg_decode_output_read_elapsed_ms="));
        assert!(formatted.contains("avg_decode_process_spawn_elapsed_ms="));
        assert!(formatted.contains("avg_handoff_elapsed_ms="));
        assert!(formatted.contains("avg_render_elapsed_ms="));
        assert!(formatted.contains("loop_total_elapsed_ms="));
        assert!(formatted.contains("attempt_body_elapsed_ms="));
        assert!(formatted.contains("loop_sleep_elapsed_ms="));
        assert!(formatted.contains("frame_interval_wait_elapsed_ms="));
        assert!(formatted.contains("event_pump_elapsed_ms="));
        assert!(formatted.contains("window_update_elapsed_ms="));
        assert!(formatted.contains("render_prepare_elapsed_ms="));
        assert!(formatted.contains("render_buffer_cpu_scale_copy_elapsed_ms="));
        assert!(formatted.contains("render_buffer_copy_elapsed_ms="));
        assert!(formatted.contains("render_buffer_materialization_elapsed_ms="));
        assert!(formatted.contains("render_buffer_scale_prepare_elapsed_ms="));
        assert!(formatted.contains("render_buffer_scale_loop_elapsed_ms="));
        assert!(formatted.contains("render_buffer_output_copy_elapsed_ms="));
        assert!(formatted.contains("render_buffer_resize_elapsed_ms="));
        assert!(formatted.contains("render_buffer_clear_elapsed_ms="));
        assert!(formatted.contains("render_buffer_passthrough_count="));
        assert!(formatted.contains("render_buffer_same_size_copy_count="));
        assert!(formatted.contains("render_buffer_half_scale_count="));
        assert!(formatted.contains("render_buffer_generic_scale_count="));
        assert!(formatted.contains("render_buffer_reuse_count=0"));
        assert!(formatted.contains("render_buffer_allocation_count=1"));
        assert!(formatted.contains("render_buffer_bytes_copied_total="));
        assert!(formatted.contains("render_backend_wait_elapsed_ms="));
        assert!(formatted.contains("gdi_invalidate_elapsed_ms="));
        assert!(formatted.contains("gdi_paint_wait_elapsed_ms="));
        assert!(formatted.contains("gdi_wm_paint_elapsed_ms="));
        assert!(formatted.contains("gdi_stretchdibits_elapsed_ms="));
        assert!(formatted.contains("texture_upload_elapsed_ms=0"));
        assert!(formatted.contains("window_present_elapsed_ms=0"));
        assert!(formatted.contains("vsync_or_present_block_elapsed_ms=0"));
        assert!(formatted.contains("quad_view_compose_elapsed_ms="));
        assert!(formatted.contains("quad_view_compose_attempt_count=1"));
        assert!(formatted.contains("quad_view_compose_success_count=1"));
        assert!(formatted.contains("quad_view_compose_skipped_unchanged_count=0"));
        assert!(formatted.contains("quad_view_composed_frame_reuse_count=0"));
        assert!(formatted.contains("quad_view_visual_unchanged_count=0"));
        assert!(formatted.contains("quad_view_visual_changed_count=1"));
        assert!(formatted.contains("materialization_reason_first_render_count=1"));
        assert!(formatted.contains("materialization_reason_visual_changed_count=0"));
        assert!(formatted.contains("materialization_reason_previous_output_missing_count=0"));
        assert!(formatted.contains("materialization_reason_profile_or_size_mismatch_count=0"));
        assert!(formatted.contains("materialization_reason_force_render_count=0"));
        assert!(formatted.contains("materialization_reason_unknown_count=0"));
        assert!(formatted.contains("slot0_frame_id_changed_count=0"));
        assert!(formatted.contains("slot1_frame_id_changed_count=0"));
        assert!(formatted.contains("slot2_frame_id_changed_count=0"));
        assert!(formatted.contains("slot3_frame_id_changed_count=0"));
        assert!(formatted.contains("slot0_selected_source_changed_count=0"));
        assert!(formatted.contains("slot1_selected_source_changed_count=0"));
        assert!(formatted.contains("slot2_selected_source_changed_count=0"));
        assert!(formatted.contains("slot3_selected_source_changed_count=0"));
        assert!(formatted.contains("placeholder_visual_changed_count=0"));
        assert!(formatted.contains("quad_view_incremental_update_count=0"));
        assert!(formatted.contains("quad_view_full_compose_count=1"));
        assert!(formatted.contains("quad_view_changed_slot_update_count=4"));
        assert!(formatted.contains("quad_view_reused_slot_count=0"));
        assert!(formatted.contains("quad_view_allocation_count=1"));
        assert!(formatted.contains("avg_render_buffer_cpu_scale_copy_elapsed_ms="));
        assert!(formatted.contains("avg_render_buffer_materialization_elapsed_ms="));
        assert!(formatted.contains("avg_gdi_paint_wait_elapsed_ms="));
        assert!(formatted.contains("avg_gdi_wm_paint_elapsed_ms="));
        assert!(formatted.contains("avg_gdi_stretchdibits_elapsed_ms="));
        assert!(formatted.contains("avg_quad_view_incremental_update_elapsed_ms=n/a"));
        assert!(formatted.contains("avg_quad_view_compose_elapsed_ms="));
        assert!(formatted.contains("render_call_elapsed_ms="));
        assert!(formatted.contains("render_input_unchanged_count=0"));
        assert!(formatted.contains("render_reuse_frame_count=0"));
        assert!(formatted.contains("unaccounted_elapsed_ms="));
        assert!(formatted.contains("avg_attempt_elapsed_ms="));
        assert!(formatted.contains("max_attempt_elapsed_ms="));
        assert!(formatted.contains("slow_attempt_count="));
        assert!(formatted.contains("slow_attempt_threshold_ms=66"));
        assert_eq!(summary.loop_total_elapsed_ms, summary.elapsed_ms);
        assert!(summary.unaccounted_elapsed_ms <= summary.loop_total_elapsed_ms);
        assert!(summary.render_call_elapsed_ms >= summary.window_update_elapsed_ms);
        assert!(formatted.contains("scheduler_status=PartialSelected"));
        assert!(formatted
            .contains("slot_result_kinds=NoFrameAvailable|Selected|NoFrameAvailable|Selected"));
        assert!(formatted.contains("frame_is_keyframe=true"));
        assert!(formatted.contains("clean_output_render_result_kind=RenderFailed"));
        assert!(formatted.contains("window_title=StreamSync 4-view Output"));
        assert!(formatted.contains(&format!(
            "output_width={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
        )));
        assert!(formatted.contains(&format!(
            "output_height={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
        )));
    }

    #[test]
    fn switcher_four_view_preview_slot_diagnostic_formats_decodable_fields() {
        let formatted = super::format_four_view_preview_slot_diagnostic(
            &super::FourViewPreviewSlotDiagnosticSummary {
                slot_index: 0,
                client_id: ClientId("player1".to_string()),
                run_id: RunId("run-1".to_string()),
                request_id: Some(9),
                actual_pipe_path: Some(r"\\.\pipe\fixture-pipe".to_string()),
                handoff_response_kind: Some("NoFrame"),
                parse_error: None,
                io_error: None,
                response_payload_len: Some(0),
                frame_id: None,
                frame_payload_len: None,
                frame_is_keyframe: None,
                handoff_no_frame_reason: Some("NoDecodableFrameAvailable".to_string()),
                decodable_source: Some("retained_keyframe".to_string()),
                retained_keyframe_available: Some(true),
                retained_keyframe_frame_id: Some(901),
                decode_attempted: Some(false),
                decode_skipped_reason: Some("NoFrameAvailable".to_string()),
                decode_error: None,
                decode_input_payload_len: None,
                decode_expected_width: None,
                decode_expected_height: None,
                decode_expected_pixel_format: None,
                decode_expected_rawvideo_len: None,
                decoded_stdout_len: None,
                ffmpeg_exit_status: None,
                ffmpeg_stderr_summary: None,
                payload_has_sps: None,
                payload_has_pps: None,
                payload_has_idr: None,
                payload_has_non_idr_vcl: None,
                payload_nal_kinds: None,
                renderable_frame_available: Some(false),
                renderable_frame_missing_reason: Some(
                    "NoDisplayPlaceholder:NoFrameAvailable".to_string(),
                ),
                selected_frame_available: Some(false),
                selected_frame_id: None,
                selected_frame_source: None,
                target_selection_result: "NoFrameAvailable",
                render_input_kind: "UseNoDisplayPlaceholder",
                final_slot_result_kind: "NoFrameAvailable",
            },
        );

        assert!(formatted.contains("handoff_no_frame_reason=NoDecodableFrameAvailable"));
        assert!(formatted.contains("decodable_source=retained_keyframe"));
        assert!(formatted.contains("retained_keyframe_available=true"));
        assert!(formatted.contains("retained_keyframe_frame_id=901"));
        assert!(formatted.contains("selected_frame_available=false"));
        assert!(formatted.contains("target_selection_result=NoFrameAvailable"));
        assert!(formatted.contains("decode_attempted=false"));
        assert!(formatted.contains("decode_skipped_reason=NoFrameAvailable"));
        assert!(formatted.contains("renderable_frame_available=false"));
        assert!(formatted
            .contains("renderable_frame_missing_reason=NoDisplayPlaceholder:NoFrameAvailable"));
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_reports_waiting_decode_skip_diagnostics() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            2,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
            TimestampMicros(1_000_000),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client-0".to_string()),
                        run_id: RunId("real-run-0".to_string()),
                        frame_id: 9,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 4,
                        encoded_payload: vec![0, 0, 0, 1],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_rendered, 0);
        assert_eq!(
            summary.slot_result_kinds[0],
            "WaitingForFrameAtOrBeforeTarget".to_string()
        );
        assert!(summary.slot_diagnostics[0].contains("handoff_response_kind=FrameRead"));
        assert!(summary.slot_diagnostics[0].contains("frame_is_keyframe=true"));
        assert!(summary.slot_diagnostics[0].contains("selected_frame_available=false"));
        assert!(summary.slot_diagnostics[0]
            .contains("target_selection_result=WaitingForFrameAtOrBeforeTarget"));
        assert!(summary.slot_diagnostics[0].contains("decode_attempted=false"));
        assert!(summary.slot_diagnostics[0]
            .contains("decode_skipped_reason=WaitingForFrameAtOrBeforeTarget"));
        assert!(summary.slot_diagnostics[0].contains("renderable_frame_available=false"));
        assert!(summary.slot_diagnostics[0].contains(
            "renderable_frame_missing_reason=NoDisplayPlaceholder:WaitingForFrameAtOrBeforeTarget"
        ));
        assert!(summary.slot_diagnostics[0].contains("render_input_kind=UseNoDisplayPlaceholder"));
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_loop_recomputes_target_timestamp_per_frame() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let mut target_timestamp_calls = 0u32;
        let summary =
            super::run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep(
                "fixture-pipe",
                0,
                ClientId("real-client-0".to_string()),
                RunId("real-run-0".to_string()),
                2,
                ClientId("real-client-1".to_string()),
                RunId("real-run-1".to_string()),
                NonZeroU32::new(2).expect("2 should be non-zero"),
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                || {
                    target_timestamp_calls += 1;
                    if target_timestamp_calls == 1 {
                        TimestampMicros(1_000_000)
                    } else {
                        TimestampMicros(1_000_004)
                    }
                },
                StubRealQueuedFrameHandoff {
                    result: SwitcherQueuedFrameHandoffResult::FrameRead {
                        frame: SwitcherSingleViewSelectedEncodedFrame {
                            client_id: ClientId("real-client-0".to_string()),
                            run_id: RunId("real-run-0".to_string()),
                            frame_id: 11,
                            capture_timestamp: TimestampMicros(1_000_001),
                            send_timestamp: TimestampMicros(1_000_101),
                            queued_at: TimestampMicros(2_400_001),
                            is_keyframe: true,
                            width: 2,
                            height: 1,
                            fps_nominal: 30,
                            codec: Codec::H264,
                            encoded_payload_len: 1,
                            encoded_payload: vec![0x44],
                        },
                        mode: SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                        remaining_client_queue_len: 0,
                    },
                    calls: RefCell::new(0),
                },
                &DeterministicFourViewFixtureDecodeRuntime,
                &render_runtime,
                &RecordingCadenceSleepHook::default(),
            );

        assert_eq!(target_timestamp_calls, 2);
        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 1);
        assert_eq!(summary.slot_result_kinds[0], "Selected".to_string());
        assert!(summary.slot_diagnostics[0].contains("selected_frame_available=true"));
        assert!(summary.slot_diagnostics[0].contains("selected_frame_id=11"));
        assert!(summary.slot_diagnostics[0].contains("decode_attempted=true"));
        assert!(summary.slot_diagnostics[0].contains("renderable_frame_available=true"));
        assert!(summary.slot_diagnostics[0].contains("render_input_kind=UseUpdatedFrame"));
    }

    #[test]
    fn switcher_four_view_four_real_handoff_preview_loop_recomputes_target_timestamp_per_frame() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let mut target_timestamp_calls = 0u32;
        let summary =
            run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_target_timestamp_hook_and_sleep(
                "fixture-pipe",
                ClientId("real-client-0".to_string()),
                RunId("real-run-0".to_string()),
                ClientId("real-client-1".to_string()),
                RunId("real-run-1".to_string()),
                ClientId("real-client-2".to_string()),
                RunId("real-run-2".to_string()),
                ClientId("real-client-3".to_string()),
                RunId("real-run-3".to_string()),
                NonZeroU32::new(2).expect("2 should be non-zero"),
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                || {
                    target_timestamp_calls += 1;
                    if target_timestamp_calls == 1 {
                        TimestampMicros(1_000_000)
                    } else {
                        TimestampMicros(1_000_004)
                    }
                },
                StubRealQueuedFrameHandoff {
                    result: SwitcherQueuedFrameHandoffResult::FrameRead {
                        frame: SwitcherSingleViewSelectedEncodedFrame {
                            client_id: ClientId("real-client-0".to_string()),
                            run_id: RunId("real-run-0".to_string()),
                            frame_id: 31,
                            capture_timestamp: TimestampMicros(1_000_001),
                            send_timestamp: TimestampMicros(1_000_101),
                            queued_at: TimestampMicros(2_400_001),
                            is_keyframe: true,
                            width: 2,
                            height: 1,
                            fps_nominal: 30,
                            codec: Codec::H264,
                            encoded_payload_len: 1,
                            encoded_payload: vec![0x99],
                        },
                        mode: SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                        remaining_client_queue_len: 0,
                    },
                    calls: RefCell::new(0),
                },
                &DeterministicFourViewFixtureDecodeRuntime,
                &render_runtime,
                &RecordingCadenceSleepHook::default(),
            );

        assert_eq!(target_timestamp_calls, 2);
        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 1);
        assert_eq!(summary.preview_mode, "preview-latest-decodable");
        assert_eq!(summary.read_mode, "inspect-latest-decodable");
        assert_eq!(summary.slot_result_kinds[0], "Selected".to_string());
        assert!(summary.slot_diagnostics[0].contains("selected_frame_available=true"));
        assert!(summary.slot_diagnostics[0].contains("selected_frame_id=31"));
        assert!(summary.slot_diagnostics[0].contains("decode_attempted=true"));
        assert!(summary.slot_diagnostics[0].contains("renderable_frame_available=true"));
    }

    #[test]
    fn switcher_four_view_two_real_handoff_preview_summary_normalizes_full_pipe_path() {
        let summary = run_four_view_two_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            r"\\.\pipe\fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            1,
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatest,
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                    client_id: ClientId("real-client-0".to_string()),
                    run_id: RunId("real-run-0".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &PersistentFixtureRenderedWindowRuntime::default(),
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.pipe_name, r"\\.\pipe\fixture-pipe".to_string());
        assert_eq!(
            summary.actual_pipe_path,
            r"\\.\pipe\fixture-pipe".to_string()
        );
    }

    #[test]
    fn switcher_four_view_four_real_handoff_preview_loop_uses_all_real_slots() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let modes = std::rc::Rc::new(RefCell::new(Vec::new()));
        let summary = run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
            TimestampMicros(1_000_004),
            RecordingInputModeQueuedFrameHandoff {
                modes: modes.clone(),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.render_failures, 0);
        assert_eq!(summary.preview_mode, "preview-latest-decodable");
        assert_eq!(summary.read_mode, "inspect-latest-decodable");
        assert_eq!(
            modes.borrow().as_slice(),
            &[
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
                SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
            ]
        );
        assert_eq!(
            summary.slot_bindings,
            [
                "0:real-client-0/real-run-0".to_string(),
                "1:real-client-1/real-run-1".to_string(),
                "2:real-client-2/real-run-2".to_string(),
                "3:real-client-3/real-run-3".to_string(),
            ]
        );
        assert_eq!(
            summary.slot_result_kinds,
            [
                "Selected".to_string(),
                "Selected".to_string(),
                "Selected".to_string(),
                "Selected".to_string(),
            ]
        );
        assert_eq!(summary.clean_output_render_result_kind, "Rendered");
        assert_eq!(
            summary.window_title,
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        );
        assert_eq!(
            summary.output_width,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH)
        );
        assert_eq!(
            summary.output_height,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT)
        );
    }

    #[test]
    fn switcher_four_view_four_real_handoff_preview_summary_formats_expected_fields() {
        let summary = run_four_view_four_real_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            SwitcherSingleClientQueueSourceMode::PreviewLatestDecodable,
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client-0".to_string()),
                        run_id: RunId("real-run-0".to_string()),
                        frame_id: 1,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 1,
                        encoded_payload: vec![0x77],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &PersistentFixtureRenderFailedWindowRuntime::default(),
            &RecordingCadenceSleepHook::default(),
        );
        let formatted = format_four_view_four_real_handoff_preview_loop_summary(&summary);

        assert!(formatted.contains("command_name=--four-view-four-real-handoff-preview-loop"));
        assert!(formatted.contains("real_handoff=true"));
        assert!(formatted.contains("real_slot_count=4"));
        assert!(formatted.contains("pipe_name=fixture-pipe"));
        assert!(formatted.contains("preview_mode=preview-latest-decodable"));
        assert!(formatted.contains("read_mode=inspect-latest-decodable"));
        assert!(formatted.contains("client0_id=real-client-0"));
        assert!(formatted.contains("run0_id=real-run-0"));
        assert!(formatted.contains("client1_id=real-client-1"));
        assert!(formatted.contains("run1_id=real-run-1"));
        assert!(formatted.contains("client2_id=real-client-2"));
        assert!(formatted.contains("run2_id=real-run-2"));
        assert!(formatted.contains("client3_id=real-client-3"));
        assert!(formatted.contains("run3_id=real-run-3"));
        assert!(formatted.contains("scheduler_status=AllSelected"));
        assert!(formatted.contains("slot_result_kinds=Selected|Selected|Selected|Selected"));
        assert!(formatted.contains("clean_output_render_result_kind=RenderFailed"));
        assert!(formatted.contains("window_title=StreamSync 4-view Output"));
        assert!(formatted.contains(&format!(
            "output_width={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
        )));
        assert!(formatted.contains(&format!(
            "output_height={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
        )));
    }

    #[test]
    fn switcher_four_view_focused_handoff_preview_loop_renders_focused_slot_full_window() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_focused_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            2,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(2).expect("2 should be non-zero"),
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client-2".to_string()),
                        run_id: RunId("real-run-2".to_string()),
                        frame_id: 1,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 1,
                        encoded_payload: vec![0x88],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.focused_slot_index, 2);
        assert_eq!(
            summary.focused_client_id,
            ClientId("real-client-2".to_string())
        );
        assert_eq!(summary.focused_run_id, RunId("real-run-2".to_string()));
        assert_eq!(summary.focused_result_kind, "Selected");
        assert_eq!(summary.frames_attempted, 2);
        assert_eq!(summary.frames_rendered, 2);
        assert_eq!(summary.render_failures, 0);
        assert_eq!(summary.clean_output_render_result_kind, "Rendered");
        assert_eq!(
            summary.window_title,
            SWITCHER_FOUR_VIEW_CLEAN_OUTPUT_WINDOW_TITLE
        );
        assert_eq!(
            summary.output_width,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH)
        );
        assert_eq!(
            summary.output_height,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT)
        );
    }

    #[test]
    fn switcher_four_view_focused_handoff_preview_loop_reports_no_renderable_focused_view_for_no_frame(
    ) {
        let summary = run_four_view_focused_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            0,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::NoFrameAvailable {
                    client_id: ClientId("real-client-0".to_string()),
                    run_id: RunId("real-run-0".to_string()),
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &PersistentFixtureRenderedWindowRuntime::default(),
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.focused_result_kind, "NoFrameAvailable");
        assert_eq!(summary.frames_rendered, 0);
        assert_eq!(summary.render_failures, 0);
        assert_eq!(
            summary.clean_output_render_result_kind,
            "NoRenderableFocusedView"
        );
        assert_eq!(summary.output_width, None);
        assert_eq!(summary.output_height, None);
    }

    #[test]
    fn switcher_four_view_focused_handoff_preview_summary_formats_expected_fields() {
        let summary = run_four_view_focused_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            3,
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client-3".to_string()),
                        run_id: RunId("real-run-3".to_string()),
                        frame_id: 1,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 1,
                        encoded_payload: vec![0x99],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &PersistentFixtureRenderFailedWindowRuntime::default(),
            &RecordingCadenceSleepHook::default(),
        );
        let formatted = format_four_view_focused_handoff_preview_loop_summary(&summary);

        assert!(formatted.contains("command_name=--four-view-focused-handoff-preview-loop"));
        assert!(formatted.contains("real_handoff=true"));
        assert!(formatted.contains("real_slot_count=4"));
        assert!(formatted.contains("view_state=Focused"));
        assert!(formatted.contains("focused_slot_index=3"));
        assert!(formatted.contains("pipe_name=fixture-pipe"));
        assert!(formatted.contains("focused_client_id=real-client-3"));
        assert!(formatted.contains("focused_run_id=real-run-3"));
        assert!(formatted.contains("focused_result_kind=Selected"));
        assert!(formatted.contains("scheduler_status=AllSelected"));
        assert!(formatted.contains("slot_result_kinds=Selected|Selected|Selected|Selected"));
        assert!(formatted.contains("clean_output_render_result_kind=RenderFailed"));
        assert!(formatted.contains("window_title=StreamSync 4-view Output"));
        assert!(formatted.contains(&format!(
            "output_width={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
        )));
        assert!(formatted.contains(&format!(
            "output_height={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
        )));
    }

    #[test]
    fn switcher_four_view_controlled_handoff_preview_loop_runs_scripted_all_focus_all_quit() {
        let render_runtime = PersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_controlled_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            FourViewControlCommandSource::Scripted("status;focus 1;all;quit".to_string()),
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client-0".to_string()),
                        run_id: RunId("real-run-0".to_string()),
                        frame_id: 1,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 1,
                        encoded_payload: vec![0xaa],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.commands_processed, 4);
        assert_eq!(summary.commands_rejected, 0);
        assert_eq!(summary.frames_rendered, 3);
        assert_eq!(summary.render_failures, 0);
        assert_eq!(summary.exit_reason, "QuitRequested");
        assert_eq!(
            summary.clean_output_render_result_kind,
            "Rendered".to_string()
        );
        assert_eq!(summary.command_summaries.len(), 4);
        assert_eq!(
            summary.command_summaries[0].current_view_state,
            super::SwitcherFourViewControlledPreviewViewState::AllView
        );
        assert_eq!(
            summary.command_summaries[1].current_view_state,
            super::SwitcherFourViewControlledPreviewViewState::Focused(1)
        );
        assert_eq!(summary.command_summaries[1].view_render_mode, "Focused");
        assert_eq!(
            summary.command_summaries[1].output_layout,
            "FocusedFullWindow"
        );
        assert_eq!(summary.command_summaries[1].focused_slot_index, Some(1));
        assert_eq!(
            summary.command_summaries[1].selected_slot_result,
            "Selected".to_string()
        );
        assert_eq!(summary.command_summaries[2].view_render_mode, "AllView");
        assert_eq!(summary.command_summaries[2].output_layout, "QuadView");
        assert_eq!(summary.command_summaries[2].focused_slot_index, None);
        assert_eq!(
            summary.command_summaries[2].all_view_render_result_kind,
            "Rendered"
        );
        assert_eq!(
            summary.final_view_state,
            super::SwitcherFourViewControlledPreviewViewState::AllView
        );
        assert_eq!(
            summary.output_width,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH)
        );
        assert_eq!(
            summary.output_height,
            Some(FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT)
        );
    }

    #[test]
    fn switcher_four_view_controlled_handoff_preview_loop_keeps_invalid_focus_rejected() {
        let summary = run_four_view_controlled_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            FourViewControlCommandSource::Scripted("focus 9;quit".to_string()),
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client-0".to_string()),
                        run_id: RunId("real-run-0".to_string()),
                        frame_id: 1,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 1,
                        encoded_payload: vec![0xbb],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &PersistentFixtureRenderedWindowRuntime::default(),
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.commands_processed, 2);
        assert_eq!(summary.commands_rejected, 1);
        assert_eq!(summary.frames_rendered, 0);
        assert_eq!(
            summary.command_summaries[0].transition_result,
            "Rejected".to_string()
        );
        assert_eq!(
            summary.command_summaries[0].command_parse_error,
            "invalid focus index: expected integer 0..3".to_string()
        );
        assert_eq!(
            summary.command_summaries[1].exit_reason,
            "QuitRequested".to_string()
        );
    }

    #[test]
    fn switcher_four_view_controlled_handoff_preview_summary_formats_expected_fields() {
        let render_runtime = PersistentFixtureRenderFailedWindowRuntime::default();
        let summary = run_four_view_controlled_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            FourViewControlCommandSource::Scripted("status;focus 2;quit".to_string()),
            TimestampMicros(1_000_004),
            StubRealQueuedFrameHandoff {
                result: SwitcherQueuedFrameHandoffResult::FrameRead {
                    frame: SwitcherSingleViewSelectedEncodedFrame {
                        client_id: ClientId("real-client-0".to_string()),
                        run_id: RunId("real-run-0".to_string()),
                        frame_id: 1,
                        capture_timestamp: TimestampMicros(1_000_001),
                        send_timestamp: TimestampMicros(1_000_101),
                        queued_at: TimestampMicros(2_400_001),
                        is_keyframe: true,
                        width: 2,
                        height: 1,
                        fps_nominal: 30,
                        codec: Codec::H264,
                        encoded_payload_len: 1,
                        encoded_payload: vec![0xcc],
                    },
                    mode: SwitcherSingleClientQueueSourceMode::PreviewLatest,
                    remaining_client_queue_len: 0,
                },
                calls: RefCell::new(0),
            },
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );
        let command_formatted = format_four_view_controlled_handoff_preview_command_summary(
            &summary.command_summaries[0],
        );
        let loop_formatted = format_four_view_controlled_handoff_preview_loop_summary(&summary);

        assert!(
            command_formatted.contains("command_name=--four-view-controlled-handoff-preview-loop")
        );
        assert!(command_formatted.contains("control_command_name=status"));
        assert!(command_formatted.contains("current_view_state=AllView"));
        assert!(command_formatted.contains("view_render_mode=AllView"));
        assert!(command_formatted.contains("output_layout=QuadView"));
        assert!(command_formatted.contains("rendered_slot_count=4"));
        assert!(command_formatted.contains("focused_slot_index=none"));
        assert!(command_formatted.contains("requested_transition=Status"));
        assert!(command_formatted.contains("transition_result=Observed"));
        assert!(command_formatted.contains("command_parse_error=none"));
        assert!(loop_formatted.contains("command_name=--four-view-controlled-handoff-preview-loop"));
        assert!(loop_formatted.contains("real_slot_count=4"));
        assert!(loop_formatted.contains("command_source=scripted:status;focus_2;quit"));
        assert!(loop_formatted.contains("max_ticks_per_command=1"));
        assert!(loop_formatted.contains("commands_processed=3"));
        assert!(loop_formatted.contains("commands_rejected=0"));
        assert!(loop_formatted.contains("current_view_state=Focused(2)"));
        assert!(loop_formatted.contains("view_render_mode=Focused"));
        assert!(loop_formatted.contains("output_layout=FocusedFullWindow"));
        assert!(loop_formatted.contains("focused_slot_index=2"));
        assert!(loop_formatted.contains("render_failures=2"));
        assert!(loop_formatted.contains("clean_output_render_result_kind=RenderFailed"));
        assert!(loop_formatted.contains("all_view_render_result_kind=not_applicable"));
        assert!(loop_formatted.contains("window_title=StreamSync 4-view Output"));
        assert!(loop_formatted.contains(&format!(
            "output_width={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_WIDTH
        )));
        assert!(loop_formatted.contains(&format!(
            "output_height={}",
            FOUR_VIEW_CLEAN_OUTPUT_LOOP_OBS_OUTPUT_HEIGHT
        )));
        assert!(loop_formatted.contains("exit_reason=QuitRequested"));
    }

    #[test]
    fn switcher_four_view_controlled_handoff_preview_focus_then_all_uses_quad_view_render_path() {
        let render_runtime = RecordingPersistentFixtureRenderedWindowRuntime::default();
        let summary = run_four_view_controlled_handoff_preview_loop_with_handoff_runtime_and_sleep(
            "fixture-pipe",
            ClientId("real-client-0".to_string()),
            RunId("real-run-0".to_string()),
            ClientId("real-client-1".to_string()),
            RunId("real-run-1".to_string()),
            ClientId("real-client-2".to_string()),
            RunId("real-run-2".to_string()),
            ClientId("real-client-3".to_string()),
            RunId("real-run-3".to_string()),
            NonZeroU32::new(1).expect("1 should be non-zero"),
            FourViewControlCommandSource::Scripted("focus 3;all;quit".to_string()),
            TimestampMicros(1_000_004),
            PerClientStubRealQueuedFrameHandoff::default(),
            &DeterministicFourViewFixtureDecodeRuntime,
            &render_runtime,
            &RecordingCadenceSleepHook::default(),
        );

        assert_eq!(summary.command_summaries.len(), 3);
        assert_eq!(summary.command_summaries[0].view_render_mode, "Focused");
        assert_eq!(
            summary.command_summaries[0].output_layout,
            "FocusedFullWindow"
        );
        assert_eq!(summary.command_summaries[0].focused_slot_index, Some(3));
        assert_eq!(summary.command_summaries[0].rendered_slot_count, 1);
        assert_eq!(summary.command_summaries[1].view_render_mode, "AllView");
        assert_eq!(summary.command_summaries[1].output_layout, "QuadView");
        assert_eq!(summary.command_summaries[1].focused_slot_index, None);
        assert_eq!(summary.command_summaries[1].rendered_slot_count, 4);
        assert_eq!(
            summary.command_summaries[1].all_view_render_result_kind,
            "Rendered"
        );

        let requests = render_runtime.requests.borrow();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].unique_corner_count(), 1);
        assert!(
            requests[1].unique_corner_count() > 1,
            "AllView should render a quad surface instead of reusing the focused full-window frame",
        );
    }

    #[test]
    fn switcher_four_view_control_pipe_response_formats_required_fields() {
        let response = format_four_view_control_pipe_command_response(
            "focus 2",
            &SwitcherFourViewControlledPreviewCommandSummary {
                command_index: 1,
                control_command_name: "focus".to_string(),
                current_view_state: super::SwitcherFourViewControlledPreviewViewState::Focused(2),
                view_render_mode: "Focused".to_string(),
                output_layout: "FocusedFullWindow".to_string(),
                requested_transition: "Focused(2)".to_string(),
                transition_result: "Transitioned".to_string(),
                selected_slot_result: "Selected".to_string(),
                rendered_slot_count: 1,
                focused_slot_index: Some(2),
                frames_rendered: 1,
                render_failures: 0,
                scheduler_status:
                    stream_sync_switcher::SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus::AllSelected,
                clean_output_render_result_kind: "Rendered".to_string(),
                all_view_render_result_kind: "not_applicable".to_string(),
                command_parse_error: "none".to_string(),
                exit_reason: "none".to_string(),
            },
        );

        assert!(response.contains("command=focus_2"));
        assert!(response.contains("transition_result=Transitioned"));
        assert!(response.contains("current_view_state=Focused(2)"));
        assert!(response.contains("view_render_mode=Focused"));
        assert!(response.contains("output_layout=FocusedFullWindow"));
        assert!(response.contains("rendered_slot_count=1"));
        assert!(response.contains("focused_slot_index=2"));
        assert!(response.contains("selected_slot_result=Selected"));
        assert!(response.contains("clean_output_render_result_kind=Rendered"));
        assert!(response.contains("all_view_render_result_kind=not_applicable"));
        assert!(response.contains("command_parse_error=none"));
        assert!(response.contains("exit_reason=none"));
    }

    struct FakeControlPipeClientRuntime {
        last_pipe_name: Option<String>,
        last_command: Option<String>,
        response: String,
    }

    impl FourViewControlPipeClientRuntime for FakeControlPipeClientRuntime {
        fn send_command(
            &mut self,
            pipe_name: &str,
            command: &str,
            _connect_timeout_millis: u32,
        ) -> io::Result<String> {
            self.last_pipe_name = Some(pipe_name.to_string());
            self.last_command = Some(command.to_string());
            Ok(self.response.clone())
        }
    }

    #[test]
    fn switcher_send_control_command_uses_client_runtime_and_returns_response() {
        let mut runtime = FakeControlPipeClientRuntime {
            last_pipe_name: None,
            last_command: None,
            response:
                "switcher four-view control response command=status transition_result=Observed current_view_state=AllView selected_slot_result=Selected clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=none"
                    .to_string(),
        };

        let response =
            run_send_control_command_with_runtime("streamsync-control-dev", "status", &mut runtime)
                .expect("send-control-command should succeed");

        assert_eq!(
            runtime.last_pipe_name.as_deref(),
            Some("streamsync-control-dev")
        );
        assert_eq!(runtime.last_command.as_deref(), Some("status"));
        assert!(response.contains("command=status"));
    }

    #[derive(Default)]
    struct FakeOperatorWrapperClientRuntime {
        commands: Vec<String>,
        responses: Vec<String>,
        send_error: Option<String>,
    }

    impl FakeOperatorWrapperClientRuntime {
        fn failing(message: &str) -> Self {
            Self {
                commands: Vec::new(),
                responses: Vec::new(),
                send_error: Some(message.to_string()),
            }
        }
    }

    impl FourViewControlPipeClientRuntime for FakeOperatorWrapperClientRuntime {
        fn send_command(
            &mut self,
            _pipe_name: &str,
            command: &str,
            _connect_timeout_millis: u32,
        ) -> io::Result<String> {
            self.commands.push(command.to_string());
            if let Some(error) = self.send_error.clone() {
                return Err(io::Error::other(error));
            }
            Ok(self.responses.remove(0))
        }
    }

    struct FakeOperatorWrapperClock {
        now_millis: std::cell::Cell<u64>,
    }

    impl FakeOperatorWrapperClock {
        fn new(now_millis: u64) -> Self {
            Self {
                now_millis: std::cell::Cell::new(now_millis),
            }
        }

        fn set_now_millis(&self, now_millis: u64) {
            self.now_millis.set(now_millis);
        }
    }

    impl FourViewOperatorWrapperClock for FakeOperatorWrapperClock {
        fn now_millis(&self) -> u64 {
            self.now_millis.get()
        }
    }

    struct FakeOperatorWrapperRawKeyReader {
        keys: VecDeque<String>,
        restore_tracker: FourViewOperatorWrapperRawConsoleRestoreTracker,
        restore_error: Option<String>,
    }

    impl FourViewOperatorWrapperRawKeyReader for FakeOperatorWrapperRawKeyReader {
        fn read_next_key(&mut self) -> Result<Option<String>, String> {
            Ok(self.keys.pop_front())
        }
    }

    impl Drop for FakeOperatorWrapperRawKeyReader {
        fn drop(&mut self) {
            if let Some(error) = self.restore_error.clone() {
                self.restore_tracker.mark_failed(error);
            } else {
                self.restore_tracker.mark_restored();
            }
        }
    }

    struct FakeOperatorWrapperRawKeyRuntime {
        keys: VecDeque<String>,
        open_error: Option<String>,
        restore_error: Option<String>,
    }

    impl FakeOperatorWrapperRawKeyRuntime {
        fn from_keys(keys: &[&str]) -> Self {
            Self {
                keys: keys.iter().map(|value| value.to_string()).collect(),
                open_error: None,
                restore_error: None,
            }
        }

        fn failing(message: &str) -> Self {
            Self {
                keys: VecDeque::new(),
                open_error: Some(message.to_string()),
                restore_error: None,
            }
        }

        fn from_keys_with_restore_error(keys: &[&str], restore_error: &str) -> Self {
            Self {
                keys: keys.iter().map(|value| value.to_string()).collect(),
                open_error: None,
                restore_error: Some(restore_error.to_string()),
            }
        }
    }

    impl FourViewOperatorWrapperRawKeyRuntime for FakeOperatorWrapperRawKeyRuntime {
        type Reader = FakeOperatorWrapperRawKeyReader;

        fn open(
            &mut self,
        ) -> Result<
            (
                Self::Reader,
                FourViewOperatorWrapperRawConsoleRestoreTracker,
            ),
            String,
        > {
            if let Some(error) = self.open_error.clone() {
                return Err(error);
            }

            let restore_tracker = FourViewOperatorWrapperRawConsoleRestoreTracker::default();
            Ok((
                FakeOperatorWrapperRawKeyReader {
                    keys: std::mem::take(&mut self.keys),
                    restore_tracker: restore_tracker.clone(),
                    restore_error: self.restore_error.clone(),
                },
                restore_tracker,
            ))
        }
    }

    #[test]
    fn switcher_four_view_operator_wrapper_key_mapping_maps_expected_commands() {
        let clock = FakeOperatorWrapperClock::new(0);
        let mut runtime = FakeOperatorWrapperClientRuntime {
            commands: Vec::new(),
            responses: vec![
                "switcher four-view control response command=focus_0 transition_result=Transitioned current_view_state=Focused(0) selected_slot_result=Selected clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=none".to_string(),
                "switcher four-view control response command=all transition_result=Transitioned current_view_state=AllView selected_slot_result=Selected clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=none".to_string(),
                "switcher four-view control response command=status transition_result=Observed current_view_state=AllView selected_slot_result=Selected clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=none".to_string(),
            ],
            send_error: None,
        };
        let mut guard_state = FourViewOperatorWrapperGuardState {
            quit_armed: false,
            armed_until_millis: None,
        };

        let focus_summary = process_four_view_operator_wrapper_key(
            0,
            "1",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );
        let all_summary = process_four_view_operator_wrapper_key(
            1,
            "a",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );
        let status_summary = process_four_view_operator_wrapper_key(
            2,
            "S",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );

        assert_eq!(
            runtime.commands,
            vec![
                "focus 0".to_string(),
                "all".to_string(),
                "status".to_string()
            ]
        );
        assert_eq!(focus_summary.mapped_command, "focus_0");
        assert_eq!(all_summary.mapped_command, "all");
        assert_eq!(status_summary.mapped_command, "status");
        assert_eq!(status_summary.send_result, "Sent");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_unknown_key_is_ignored_locally() {
        let clock = FakeOperatorWrapperClock::new(0);
        let mut runtime = FakeOperatorWrapperClientRuntime::default();
        let mut guard_state = FourViewOperatorWrapperGuardState {
            quit_armed: false,
            armed_until_millis: None,
        };

        let summary = process_four_view_operator_wrapper_key(
            0,
            "x",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );

        assert!(runtime.commands.is_empty());
        assert_eq!(summary.send_result, "Ignored");
        assert_eq!(summary.wrapper_error, "unknown_key");
        assert_eq!(summary.mapped_command, "none");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_q_once_arms_guard_without_sending_quit() {
        let clock = FakeOperatorWrapperClock::new(100);
        let mut runtime = FakeOperatorWrapperClientRuntime::default();
        let mut guard_state = FourViewOperatorWrapperGuardState {
            quit_armed: false,
            armed_until_millis: None,
        };

        let summary = process_four_view_operator_wrapper_key(
            0,
            "q",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );

        assert!(runtime.commands.is_empty());
        assert_eq!(summary.send_result, "GuardArmed");
        assert_eq!(summary.guard_state, "quit_armed=true");
        assert_eq!(guard_state.armed_until_millis, Some(2_100));
    }

    #[test]
    fn switcher_four_view_operator_wrapper_q_twice_within_guard_sends_quit() {
        let clock = FakeOperatorWrapperClock::new(100);
        let mut runtime = FakeOperatorWrapperClientRuntime {
            commands: Vec::new(),
            responses: vec![
                "switcher four-view control response command=quit transition_result=ExitRequested current_view_state=AllView selected_slot_result=NotApplicable clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=QuitRequested".to_string(),
            ],
            send_error: None,
        };
        let mut guard_state = FourViewOperatorWrapperGuardState {
            quit_armed: false,
            armed_until_millis: None,
        };

        process_four_view_operator_wrapper_key(
            0,
            "q",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );
        clock.set_now_millis(500);
        let summary = process_four_view_operator_wrapper_key(
            1,
            "Q",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );

        assert_eq!(runtime.commands, vec!["quit".to_string()]);
        assert_eq!(summary.send_result, "Sent");
        assert_eq!(summary.exit_reason, "QuitRequested");
        assert_eq!(summary.guard_state, "quit_armed=false");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_non_q_clears_quit_guard() {
        let clock = FakeOperatorWrapperClock::new(100);
        let mut runtime = FakeOperatorWrapperClientRuntime {
            commands: Vec::new(),
            responses: vec![
                "switcher four-view control response command=status transition_result=Observed current_view_state=AllView selected_slot_result=Selected clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=none".to_string(),
            ],
            send_error: None,
        };
        let mut guard_state = FourViewOperatorWrapperGuardState {
            quit_armed: false,
            armed_until_millis: None,
        };

        process_four_view_operator_wrapper_key(
            0,
            "q",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );
        let summary = process_four_view_operator_wrapper_key(
            1,
            "s",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );

        assert_eq!(runtime.commands, vec!["status".to_string()]);
        assert_eq!(summary.guard_state, "quit_armed=false");
        assert!(!guard_state.quit_armed);
    }

    #[test]
    fn switcher_four_view_operator_wrapper_guard_timeout_clears_before_second_q() {
        let clock = FakeOperatorWrapperClock::new(100);
        let mut runtime = FakeOperatorWrapperClientRuntime::default();
        let mut guard_state = FourViewOperatorWrapperGuardState {
            quit_armed: false,
            armed_until_millis: None,
        };

        process_four_view_operator_wrapper_key(
            0,
            "q",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );
        clock.set_now_millis(2_101);
        let summary = process_four_view_operator_wrapper_key(
            1,
            "q",
            &mut guard_state,
            "streamsync-control-dev",
            &mut runtime,
            &clock,
        );

        assert!(runtime.commands.is_empty());
        assert_eq!(summary.send_result, "GuardArmed");
        assert_eq!(summary.guard_state, "quit_armed=true");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_scripted_keys_parser_splits_semicolon_tokens() {
        assert_eq!(
            split_scripted_operator_wrapper_keys("s;1;2;3;4;0;q;q"),
            vec![
                "s".to_string(),
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "0".to_string(),
                "q".to_string(),
                "q".to_string()
            ]
        );
    }

    #[test]
    fn switcher_four_view_operator_wrapper_raw_keys_mode_reuses_existing_mapping() {
        let clock = FakeOperatorWrapperClock::new(0);
        let mut runtime = FakeOperatorWrapperClientRuntime {
            commands: Vec::new(),
            responses: vec![
                "switcher four-view control response command=focus_0 transition_result=Transitioned current_view_state=Focused(0) selected_slot_result=Selected clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=none".to_string(),
                "switcher four-view control response command=all transition_result=Transitioned current_view_state=AllView selected_slot_result=NotApplicable clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=none".to_string(),
                "switcher four-view control response command=status transition_result=Observed current_view_state=AllView selected_slot_result=NotApplicable clean_output_render_result_kind=Rendered command_parse_error=none exit_reason=none".to_string(),
            ],
            send_error: None,
        };
        let mut raw_key_runtime = FakeOperatorWrapperRawKeyRuntime::from_keys(&["1", "A", "s"]);

        let summary = run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime(
            "streamsync-control-dev",
            FourViewOperatorWrapperInputSource::RawKeys,
            &mut runtime,
            &clock,
            &mut raw_key_runtime,
        )
        .expect("raw keys wrapper run should succeed");

        assert_eq!(
            runtime.commands,
            vec![
                "focus 0".to_string(),
                "all".to_string(),
                "status".to_string()
            ]
        );
        assert_eq!(summary.input_source, "raw_keys");
        assert_eq!(summary.commands_sent, 3);
        assert_eq!(summary.key_summaries[0].mapped_command, "focus_0");
        assert_eq!(summary.key_summaries[1].mapped_command, "all");
        assert_eq!(summary.key_summaries[2].mapped_command, "status");
        assert_eq!(summary.raw_console_restore_result, "restored");
        assert_eq!(summary.raw_console_restore_error, "none");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_raw_keys_q_once_and_q_twice_reuse_guard_logic() {
        let clock = FakeOperatorWrapperClock::new(100);
        let mut runtime = FakeOperatorWrapperClientRuntime {
            commands: Vec::new(),
            responses: vec![
                "switcher four-view control response command=quit transition_result=ExitRequested current_view_state=AllView selected_slot_result=NotApplicable clean_output_render_result_kind=none command_parse_error=none exit_reason=QuitRequested".to_string(),
            ],
            send_error: None,
        };
        let mut raw_key_runtime = FakeOperatorWrapperRawKeyRuntime::from_keys(&["q", "q"]);

        let summary = run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime(
            "streamsync-control-dev",
            FourViewOperatorWrapperInputSource::RawKeys,
            &mut runtime,
            &clock,
            &mut raw_key_runtime,
        )
        .expect("raw key guarded quit should succeed");

        assert_eq!(runtime.commands, vec!["quit".to_string()]);
        assert_eq!(summary.key_summaries[0].send_result, "GuardArmed");
        assert_eq!(summary.key_summaries[1].send_result, "Sent");
        assert_eq!(summary.exit_reason, "QuitRequested");
        assert_eq!(summary.raw_console_restore_result, "restored");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_raw_keys_unknown_key_is_ignored() {
        let clock = FakeOperatorWrapperClock::new(0);
        let mut runtime = FakeOperatorWrapperClientRuntime::default();
        let mut raw_key_runtime = FakeOperatorWrapperRawKeyRuntime::from_keys(&["x"]);

        let summary = run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime(
            "streamsync-control-dev",
            FourViewOperatorWrapperInputSource::RawKeys,
            &mut runtime,
            &clock,
            &mut raw_key_runtime,
        )
        .expect("raw key unknown-key run should succeed");

        assert!(runtime.commands.is_empty());
        assert_eq!(summary.ignored_keys, 1);
        assert_eq!(summary.key_summaries[0].send_result, "Ignored");
        assert_eq!(summary.key_summaries[0].wrapper_error, "unknown_key");
        assert_eq!(summary.raw_console_restore_result, "restored");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_raw_keys_setup_failure_is_explicit() {
        let clock = FakeOperatorWrapperClock::new(0);
        let mut runtime = FakeOperatorWrapperClientRuntime::default();
        let mut raw_key_runtime =
            FakeOperatorWrapperRawKeyRuntime::failing("fixture_raw_keys_unavailable");

        let error = run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime(
            "streamsync-control-dev",
            FourViewOperatorWrapperInputSource::RawKeys,
            &mut runtime,
            &clock,
            &mut raw_key_runtime,
        )
        .expect_err("raw key setup failure should be surfaced");

        assert_eq!(error, "raw key setup failed: fixture_raw_keys_unavailable");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_raw_keys_send_failure_still_restores_console_mode() {
        let clock = FakeOperatorWrapperClock::new(0);
        let mut runtime = FakeOperatorWrapperClientRuntime::failing("fixture_send_failed");
        let mut raw_key_runtime = FakeOperatorWrapperRawKeyRuntime::from_keys(&["s"]);

        let summary = run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime(
            "streamsync-control-dev",
            FourViewOperatorWrapperInputSource::RawKeys,
            &mut runtime,
            &clock,
            &mut raw_key_runtime,
        )
        .expect("raw key send failure should still return wrapper summary");

        assert_eq!(summary.exit_reason, "WrapperSendFailed");
        assert_eq!(summary.raw_console_restore_result, "restored");
        assert_eq!(summary.raw_console_restore_error, "none");
    }

    #[test]
    fn switcher_four_view_operator_wrapper_raw_keys_restore_failure_is_explicit() {
        let clock = FakeOperatorWrapperClock::new(0);
        let mut runtime = FakeOperatorWrapperClientRuntime::default();
        let mut raw_key_runtime = FakeOperatorWrapperRawKeyRuntime::from_keys_with_restore_error(
            &["x"],
            "fixture_restore_failed",
        );

        let error = run_four_view_operator_wrapper_with_runtime_and_clock_and_raw_key_runtime(
            "streamsync-control-dev",
            FourViewOperatorWrapperInputSource::RawKeys,
            &mut runtime,
            &clock,
            &mut raw_key_runtime,
        )
        .expect_err("raw key restore failure should be surfaced");

        assert_eq!(
            error,
            "raw key console mode restore failed: fixture_restore_failed"
        );
    }

    #[test]
    fn switcher_control_pipe_length_prefixed_utf8_message_round_trips() {
        let mut buffer = Vec::new();
        write_length_prefixed_utf8_message(&mut buffer, "focus 3").expect("message should encode");
        let mut cursor = std::io::Cursor::new(buffer);
        let decoded =
            read_length_prefixed_utf8_message(&mut cursor).expect("message should decode");

        assert_eq!(decoded, "focus 3".to_string());
    }
}
