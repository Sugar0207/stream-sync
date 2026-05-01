fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--auth-response-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            match stream_sync_server::run_auth_response_poc_once_from_path(&config_path) {
                Ok(outcome) => {
                    let log_boundary = stream_sync_server::ServerAuthLogOutputBoundary::default();
                    if let Err(log_error) = log_boundary.write_auth_result(
                        outcome.outcome.auth_flow.auth_log_input.clone(),
                        current_timestamp_micros(),
                        std::io::stderr().lock(),
                    ) {
                        eprintln!("auth result log output failed: {log_error:?}");
                    }

                    let decision = &outcome.outcome.auth_flow.decision;
                    println!(
                        "auth response PoC handled one packet on {} and sent {} bytes; client_id={} run_id={} accepted={} reason_code={:?}",
                        outcome.bind_address,
                        outcome.outcome.bytes_sent,
                        decision.client_id.0,
                        decision.run_id.0,
                        decision.accepted,
                        decision.reason_code
                    );
                }
                Err(error) => {
                    if let stream_sync_server::ServerAuthResponsePocStartupError::Poc(
                        stream_sync_server::ServerAuthResponsePocError::Rejected(rejection),
                    ) = &error
                    {
                        let log_boundary =
                            stream_sync_server::ServerReceiveRejectionLogOutputBoundary::default();
                        if let Err(log_error) = log_boundary.write_rejection(
                            rejection.clone(),
                            current_timestamp_micros(),
                            std::io::stderr().lock(),
                        ) {
                            eprintln!("receive rejection log output failed: {log_error:?}");
                        }
                    }
                    eprintln!("auth response PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--receive-send-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let launcher = stream_sync_server::ServerReceiveSendOneIterationLauncher::default();
            match launcher.run_once_from_path_with_writers(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
            ) {
                Ok(outcome) => match &outcome.outcome {
                    stream_sync_server::ServerControllerReceiveSendRuntimeResult::Stopped {
                        plan,
                    } => {
                        println!(
                            "receive/send one-iteration runtime stopped on {}; state={:?} action={:?}",
                            outcome.bind_address, plan.state, plan.action
                        );
                    }
                    stream_sync_server::ServerControllerReceiveSendRuntimeResult::Iteration {
                        observation,
                        iteration,
                        ..
                    } => {
                        let sent_bytes = iteration
                            .send
                            .as_ref()
                            .map(|send| send.bytes_sent)
                            .unwrap_or(0);
                        println!(
                            "receive/send one-iteration runtime handled one packet on {}; sent_bytes={} observation_state={:?} observation_action={:?}",
                            outcome.bind_address,
                            sent_bytes,
                            observation.state,
                            observation.action
                        );
                    }
                },
                Err(error) => {
                    eprintln!("receive/send one-iteration runtime failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--receive-send-twice") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let launcher = stream_sync_server::ServerReceiveSendTwoIterationLauncher::default();
            match launcher.run_two_from_path_with_writers(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
            ) {
                Ok(outcome) => {
                    let first_sent = sent_bytes(&outcome.first);
                    let second_sent = sent_bytes(&outcome.second);
                    println!(
                        "receive/send two-iteration runtime handled two packets on {}; first_sent_bytes={} second_sent_bytes={} registered_clients={} heartbeat_liveness_entries={}",
                        outcome.bind_address,
                        first_sent,
                        second_sent,
                        outcome.registry.entries().count(),
                        outcome.heartbeat_liveness_state.len()
                    );
                }
                Err(error) => {
                    eprintln!("receive/send two-iteration runtime failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--receive-send-three") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let launcher = stream_sync_server::ServerReceiveSendThreeIterationLauncher::default();
            match launcher.run_three_from_path_with_writers(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
            ) {
                Ok(outcome) => {
                    let first_sent = sent_bytes(&outcome.first);
                    let second_sent = sent_bytes(&outcome.second);
                    let third_sent = sent_bytes(&outcome.third);
                    println!(
                        "receive/send three-iteration runtime handled three packets on {}; first_sent_bytes={} second_sent_bytes={} third_sent_bytes={} registered_clients={} heartbeat_liveness_entries={} heartbeat_received_count={} heartbeat_rtt_offset_entries={} heartbeat_rtt_offset_samples={} heartbeat_rtt_micros={} heartbeat_server_processing_micros={} heartbeat_clock_offset_micros={}",
                        outcome.bind_address,
                        first_sent,
                        second_sent,
                        third_sent,
                        outcome.registry.entries().count(),
                        outcome.heartbeat_liveness_state.len(),
                        outcome
                            .heartbeat_liveness_commit
                            .committed
                            .received_heartbeats,
                        outcome.heartbeat_rtt_offset_state.len(),
                        outcome
                            .heartbeat_rtt_offset_policy_commit
                            .committed_samples()
                            .unwrap_or(0),
                        outcome.heartbeat_calculation.estimate.rtt_micros,
                        outcome.heartbeat_calculation.estimate.server_processing_micros,
                        outcome.heartbeat_calculation.estimate.clock_offset_micros
                    );
                }
                Err(error) => {
                    eprintln!("receive/send three-iteration runtime failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--receive-auth-video-queue-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let max_video_packets =
                parse_optional_arg_or_exit::<usize>(args.next(), "max-video-packets")
                    .unwrap_or(4_096);
            let receive_timeout_ms =
                parse_optional_arg_or_exit::<u64>(args.next(), "receive-timeout-ms")
                    .unwrap_or(15_000);
            let expected_frames =
                parse_optional_arg_or_exit::<u64>(args.next(), "expected-reassembled-frames")
                    .unwrap_or(1);
            let stop_after_expected =
                parse_optional_bool_or_exit(args.next(), "stop-after-expected-reassembled-frames")
                    .unwrap_or(true);
            let receive_buffer_bytes =
                parse_optional_arg_or_exit::<usize>(args.next(), "receive-buffer-bytes")
                    .unwrap_or(8_388_608);
            let policy = stream_sync_server::ServerReceiveAuthVideoQueueOnceManualPolicy {
                max_video_packets,
                receive_timeout: std::time::Duration::from_millis(receive_timeout_ms),
                expected_reassembled_frames: expected_frames,
                stop_after_expected_reassembled_frames: stop_after_expected,
                receive_buffer_bytes,
            };
            let launcher = stream_sync_server::ServerReceiveAuthVideoQueueOnceLauncher::default();
            match launcher.run_once_from_path_with_writers_and_policy(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                policy,
            ) {
                Ok(outcome) => {
                    let decision = &outcome.first_auth.auth_flow.decision;
                    let (video_status, queued_status, queue_len, dropped_oldest, video_summary) =
                        auth_video_queue_summary(&outcome);
                    let incomplete_progress = video_summary
                        .map(format_incomplete_frame_progress)
                        .unwrap_or_else(|| "none".to_string());
                    let effective_receive_buffer = outcome
                        .receive_buffer
                        .effective_bytes
                        .map(|bytes| bytes.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    let receive_buffer_set_error = outcome
                        .receive_buffer
                        .set_error
                        .as_deref()
                        .unwrap_or("none");
                    let receive_buffer_read_error = outcome
                        .receive_buffer
                        .read_error
                        .as_deref()
                        .unwrap_or("none");
                    println!(
                        "receive auth/video queue runtime handled auth on {}; auth_accepted={} auth_reason={:?} client_id={} run_id={} video={} queued={} queue_len={} dropped_oldest={} registered_clients={} manual_max_video_packets={} manual_receive_timeout_ms={} manual_expected_reassembled_frames={} manual_stop_after_expected_reassembled_frames={} manual_receive_buffer_requested_bytes={} manual_receive_buffer_effective_bytes={} manual_receive_buffer_set_error={} manual_receive_buffer_read_error={} packets_received={} fragments_received={} frames_reassembled={} frames_queued={} direct_frames_queued={} rejected_packets={} rejected_fragments={} duplicate_fragments={} non_video_packets={} incomplete_reassembly_frames={} incomplete_frame_progress={} receive_timed_out={} max_packets_reached={}",
                        outcome.bind_address,
                        decision.accepted,
                        decision.reason_code,
                        decision.client_id.0,
                        decision.run_id.0,
                        video_status,
                        queued_status,
                        queue_len,
                        dropped_oldest,
                        outcome.registry.entries().count(),
                        max_video_packets,
                        receive_timeout_ms,
                        expected_frames,
                        stop_after_expected,
                        outcome.receive_buffer.requested_bytes,
                        effective_receive_buffer,
                        receive_buffer_set_error,
                        receive_buffer_read_error,
                        video_summary
                            .map(|summary| summary.packets_received.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.fragments_received.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.frames_reassembled.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.frames_queued.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.direct_frames_queued.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.rejected_packets.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.rejected_fragments.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.duplicate_fragments.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.non_video_packets.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.incomplete_reassembly_frames.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        incomplete_progress,
                        video_summary
                            .map(|summary| summary.receive_timed_out.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        video_summary
                            .map(|summary| summary.max_packets_reached.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    );
                }
                Err(error) => {
                    eprintln!("receive auth/video queue runtime failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--receive-auth-video-queue-and-serve-handoff-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing pipe-name");
                std::process::exit(1);
            });
            let max_video_packets =
                parse_optional_arg_or_exit::<usize>(args.next(), "max-video-packets")
                    .unwrap_or(4_096);
            let receive_timeout_ms =
                parse_optional_arg_or_exit::<u64>(args.next(), "receive-timeout-ms")
                    .unwrap_or(15_000);
            let expected_frames =
                parse_optional_arg_or_exit::<u64>(args.next(), "expected-reassembled-frames")
                    .unwrap_or(1);
            let stop_after_expected =
                parse_optional_bool_or_exit(args.next(), "stop-after-expected-reassembled-frames")
                    .unwrap_or(true);
            let receive_buffer_bytes =
                parse_optional_arg_or_exit::<usize>(args.next(), "receive-buffer-bytes")
                    .unwrap_or(8_388_608);
            let policy = stream_sync_server::ServerReceiveAuthVideoQueueOnceManualPolicy {
                max_video_packets,
                receive_timeout: std::time::Duration::from_millis(receive_timeout_ms),
                expected_reassembled_frames: expected_frames,
                stop_after_expected_reassembled_frames: stop_after_expected,
                receive_buffer_bytes,
            };
            let launcher = stream_sync_server::ServerReceiveAuthVideoQueueOnceLauncher::default();
            match launcher.run_once_from_path_with_writers_and_policy(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                policy,
            ) {
                Ok(mut outcome) => {
                    match serve_named_pipe_handoff_once(&mut outcome.video_queue_state, &pipe_name)
                    {
                        Ok(summary) => println!("{summary}"),
                        Err(error) => {
                            eprintln!(
                                "receive auth/video queue and serve handoff once failed: {error}"
                            );
                            std::process::exit(1);
                        }
                    }
                }
                Err(error) => {
                    eprintln!("receive auth/video queue and serve handoff once failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!(
                "stream-sync-server scaffold; use --auth-response-poc-once [config-path], --receive-send-once [config-path], --receive-send-twice [config-path], --receive-send-three [config-path], --receive-auth-video-queue-once [config-path] [max-video-packets] [receive-timeout-ms] [expected-reassembled-frames] [stop-after-expected-reassembled-frames] [receive-buffer-bytes], or --receive-auth-video-queue-and-serve-handoff-once [config-path] [pipe-name] [max-video-packets] [receive-timeout-ms] [expected-reassembled-frames] [stop-after-expected-reassembled-frames] [receive-buffer-bytes]"
            );
        }
    }
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

fn parse_optional_bool_or_exit(value: Option<String>, name: &str) -> Option<bool> {
    value.map(|value| match value.as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" => false,
        _ => {
            eprintln!("invalid {name}: expected true/false");
            std::process::exit(1);
        }
    })
}

fn format_incomplete_frame_progress(
    summary: &stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoSummary,
) -> String {
    if summary.incomplete_frame_progress.is_empty() {
        return "none".to_string();
    }

    summary
        .incomplete_frame_progress
        .iter()
        .map(|progress| {
            format!(
                "{}/{}/{}:{}/{}:missing={}",
                progress.key.client_id,
                progress.key.run_id,
                progress.key.frame_id,
                progress.fragments_received,
                progress.fragments_expected,
                progress.fragments_missing
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn sent_bytes(result: &stream_sync_server::ServerControllerReceiveSendRuntimeResult) -> usize {
    match result {
        stream_sync_server::ServerControllerReceiveSendRuntimeResult::Iteration {
            iteration,
            ..
        } => iteration
            .send
            .as_ref()
            .map(|send| send.bytes_sent)
            .unwrap_or(0),
        stream_sync_server::ServerControllerReceiveSendRuntimeResult::Stopped { .. } => 0,
    }
}

fn current_timestamp_micros() -> stream_sync_protocol::TimestampMicros {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or(0);
    stream_sync_protocol::TimestampMicros(u64::try_from(micros).unwrap_or(u64::MAX))
}

fn auth_video_queue_summary(
    outcome: &stream_sync_server::ServerReceiveAuthVideoQueueOnceStartupOutcome,
) -> (
    &'static str,
    &'static str,
    usize,
    bool,
    Option<&stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoSummary>,
) {
    match &outcome.video {
        stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoOutcome::NotReceivedAuthRejected => {
            (
                "not_received_auth_rejected",
                "not_queued",
                outcome.video_queue_state.total_len(),
                false,
                None,
            )
        }
        stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoOutcome::Received {
            summary,
            queue,
            ..
        } => match queue {
            Some(stream_sync_server::ServerVideoFrameQueueRuntimeResult::Queued(
                stream_sync_server::ServerVideoFrameQueueStorageResult::Stored {
                    dropped_oldest,
                    ..
                },
            )) => (
                "received",
                "queued",
                outcome.video_queue_state.total_len(),
                dropped_oldest.is_some(),
                Some(summary),
            ),
            Some(stream_sync_server::ServerVideoFrameQueueRuntimeResult::Queued(
                stream_sync_server::ServerVideoFrameQueueStorageResult::Dropped { .. },
            )) => (
                "received",
                "not_queued_storage_dropped",
                outcome.video_queue_state.total_len(),
                false,
                Some(summary),
            ),
            Some(stream_sync_server::ServerVideoFrameQueueRuntimeResult::NotQueued { .. }) => (
                "received",
                "not_queued_rejected_or_unexpected",
                outcome.video_queue_state.total_len(),
                false,
                Some(summary),
            ),
            None => (
                "received_no_completed_frame",
                "not_queued",
                outcome.video_queue_state.total_len(),
                false,
                Some(summary),
            ),
        },
    }
}

#[cfg(windows)]
fn serve_named_pipe_handoff_once(
    queue_state: &mut stream_sync_server::ServerVideoFrameQueueState,
    pipe_name: &str,
) -> Result<String, String> {
    stream_sync_server::ServerSwitcherNamedPipeOneRequestRuntimeBoundary::default()
        .serve_once(queue_state, pipe_name)
        .map(|output| format_named_pipe_handoff_server_summary(pipe_name, &output))
        .map_err(|error| format!("{error:?}"))
}

#[cfg(not(windows))]
fn serve_named_pipe_handoff_once(
    _queue_state: &mut stream_sync_server::ServerVideoFrameQueueState,
    _pipe_name: &str,
) -> Result<String, String> {
    Err("named-pipe handoff command is only available on Windows".to_string())
}

#[cfg(windows)]
fn format_named_pipe_handoff_server_summary(
    pipe_name: &str,
    output: &stream_sync_server::ServerSwitcherNamedPipeOneRequestRuntimeOutput,
) -> String {
    let request = &output.request;
    match &output.response {
        stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
            remaining_client_queue_len,
            frame,
            ..
        } => format!(
            "server named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} request_status=decoded response_status=written result_kind=FrameRead queue_len={} frame_id={} capture_timestamp={} send_timestamp={} queued_at={} width={} height={} fps_nominal={} codec={:?} is_keyframe={} encoded_payload_len={}",
            pipe_name,
            request.request_id,
            request.client_id.0,
            request.run_id.0,
            format_handoff_read_mode(request.read_mode),
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
        stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::NoFrame {
            client_queue_len, ..
        } => format!(
            "server named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} request_status=decoded response_status=written result_kind=NoFrame queue_len={}",
            pipe_name,
            request.request_id,
            request.client_id.0,
            request.run_id.0,
            format_handoff_read_mode(request.read_mode),
            client_queue_len
        ),
        stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::HandoffError {
            error,
            ..
        } => format!(
            "server named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} request_status=decoded response_status=written result_kind=HandoffError queue_len=none handoff_error={:?}",
            pipe_name,
            request.request_id,
            request.client_id.0,
            request.run_id.0,
            format_handoff_read_mode(request.read_mode),
            error
        ),
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

#[cfg(test)]
mod tests {
    use stream_sync_net_core::{
        ServerSwitcherQueuedFrameHandoffFrame, ServerSwitcherQueuedFrameHandoffRequest,
        ServerSwitcherQueuedFrameHandoffResponse, ServerSwitcherQueuedFrameReadMode,
        SERVER_SWITCHER_HANDOFF_VERSION,
    };
    use stream_sync_protocol::{ClientId, Codec, RunId, TimestampMicros};

    use super::format_handoff_read_mode;

    #[test]
    fn server_handoff_formats_read_mode_names() {
        assert_eq!(
            format_handoff_read_mode(ServerSwitcherQueuedFrameReadMode::InspectOldest),
            "inspect-oldest"
        );
        assert_eq!(
            format_handoff_read_mode(ServerSwitcherQueuedFrameReadMode::InspectLatest),
            "inspect-latest"
        );
        assert_eq!(
            format_handoff_read_mode(ServerSwitcherQueuedFrameReadMode::DequeueOldest),
            "dequeue-oldest"
        );
    }

    #[cfg(windows)]
    #[test]
    fn server_handoff_summary_includes_frame_read_fields() {
        let output = stream_sync_server::ServerSwitcherNamedPipeOneRequestRuntimeOutput {
            request: ServerSwitcherQueuedFrameHandoffRequest {
                handoff_version: SERVER_SWITCHER_HANDOFF_VERSION,
                request_id: 44,
                client_id: ClientId("player1".to_string()),
                run_id: RunId("run-a".to_string()),
                read_mode: ServerSwitcherQueuedFrameReadMode::InspectLatest,
            },
            response: ServerSwitcherQueuedFrameHandoffResponse::FrameRead {
                request_id: 44,
                remaining_client_queue_len: 2,
                frame: ServerSwitcherQueuedFrameHandoffFrame {
                    client_id: ClientId("player1".to_string()),
                    run_id: RunId("run-a".to_string()),
                    frame_id: 77,
                    capture_timestamp: TimestampMicros(10),
                    send_timestamp: TimestampMicros(11),
                    queued_at: TimestampMicros(12),
                    width: 1280,
                    height: 720,
                    fps_nominal: 30,
                    is_keyframe: true,
                    codec: Codec::H264,
                    encoded_payload_len: 3,
                    encoded_payload: vec![1, 2, 3],
                },
            },
        };

        let summary = super::format_named_pipe_handoff_server_summary("pipe-a", &output);

        assert!(summary.contains("pipe_name=pipe-a"));
        assert!(summary.contains("request_id=44"));
        assert!(summary.contains("result_kind=FrameRead"));
        assert!(summary.contains("queue_len=2"));
        assert!(summary.contains("frame_id=77"));
        assert!(summary.contains("codec=H264"));
        assert!(summary.contains("encoded_payload_len=3"));
    }
}
