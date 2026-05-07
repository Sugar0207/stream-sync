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
            let expected_reassembled_clients =
                parse_optional_arg_or_exit::<u64>(args.next(), "expected-reassembled-clients")
                    .unwrap_or(0);
            let expected_reassembled_frames_per_client = parse_optional_arg_or_exit::<u64>(
                args.next(),
                "expected-reassembled-frames-per-client",
            )
            .unwrap_or(0);
            validate_client_aware_stop_policy_or_exit(
                expected_reassembled_clients,
                expected_reassembled_frames_per_client,
            );
            let policy = stream_sync_server::ServerReceiveAuthVideoQueueOnceManualPolicy {
                max_video_packets,
                receive_timeout: std::time::Duration::from_millis(receive_timeout_ms),
                expected_reassembled_frames: expected_frames,
                stop_after_expected_reassembled_frames: stop_after_expected,
                receive_buffer_bytes,
                expected_reassembled_clients,
                expected_reassembled_frames_per_client,
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
                    println!(
                        "{}",
                        format_receive_auth_video_queue_runtime_summary(&outcome, policy)
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
            let expected_reassembled_clients =
                parse_optional_arg_or_exit::<u64>(args.next(), "expected-reassembled-clients")
                    .unwrap_or(0);
            let expected_reassembled_frames_per_client = parse_optional_arg_or_exit::<u64>(
                args.next(),
                "expected-reassembled-frames-per-client",
            )
            .unwrap_or(0);
            validate_client_aware_stop_policy_or_exit(
                expected_reassembled_clients,
                expected_reassembled_frames_per_client,
            );
            let policy = stream_sync_server::ServerReceiveAuthVideoQueueOnceManualPolicy {
                max_video_packets,
                receive_timeout: std::time::Duration::from_millis(receive_timeout_ms),
                expected_reassembled_frames: expected_frames,
                stop_after_expected_reassembled_frames: stop_after_expected,
                receive_buffer_bytes,
                expected_reassembled_clients,
                expected_reassembled_frames_per_client,
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
        Some("--receive-auth-video-queue-and-serve-handoff-many") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let pipe_name = args.next().unwrap_or_else(|| {
                eprintln!("missing pipe-name");
                std::process::exit(1);
            });
            let max_requests =
                parse_optional_arg_or_exit::<usize>(args.next(), "max-requests").unwrap_or(2);
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
            let expected_reassembled_clients =
                parse_optional_arg_or_exit::<u64>(args.next(), "expected-reassembled-clients")
                    .unwrap_or(0);
            let expected_reassembled_frames_per_client = parse_optional_arg_or_exit::<u64>(
                args.next(),
                "expected-reassembled-frames-per-client",
            )
            .unwrap_or(0);
            validate_client_aware_stop_policy_or_exit(
                expected_reassembled_clients,
                expected_reassembled_frames_per_client,
            );
            let policy = stream_sync_server::ServerReceiveAuthVideoQueueOnceManualPolicy {
                max_video_packets,
                receive_timeout: std::time::Duration::from_millis(receive_timeout_ms),
                expected_reassembled_frames: expected_frames,
                stop_after_expected_reassembled_frames: stop_after_expected,
                receive_buffer_bytes,
                expected_reassembled_clients,
                expected_reassembled_frames_per_client,
            };
            #[cfg(windows)]
            {
                let launcher =
                    stream_sync_server::ServerReceiveAuthVideoQueueHandoffServiceSessionLauncher::default();
                match launcher.run_once_from_path_with_writers_and_policy(
                    &config_path,
                    &pipe_name,
                    max_requests,
                    std::io::stderr(),
                    std::io::stderr(),
                    std::io::stderr(),
                    std::io::stderr(),
                    policy,
                ) {
                    Ok(outcome) => println!(
                        "{}",
                        format_named_pipe_handoff_service_session_summary(&outcome, policy)
                    ),
                    Err(error) => {
                        eprintln!(
                            "receive auth/video queue and serve handoff many failed: {error:?}"
                        );
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (&config_path, &pipe_name, max_requests, policy);
                eprintln!("receive auth/video queue and serve handoff many failed: named-pipe handoff command is only available on Windows");
                std::process::exit(1);
            }
        }
        _ => {
            println!(
                "stream-sync-server scaffold; use --auth-response-poc-once [config-path], --receive-send-once [config-path], --receive-send-twice [config-path], --receive-send-three [config-path], --receive-auth-video-queue-once [config-path] [max-video-packets] [receive-timeout-ms] [expected-reassembled-frames] [stop-after-expected-reassembled-frames] [receive-buffer-bytes] [expected-reassembled-clients] [expected-reassembled-frames-per-client], --receive-auth-video-queue-and-serve-handoff-once [config-path] [pipe-name] [max-video-packets] [receive-timeout-ms] [expected-reassembled-frames] [stop-after-expected-reassembled-frames] [receive-buffer-bytes] [expected-reassembled-clients] [expected-reassembled-frames-per-client], or --receive-auth-video-queue-and-serve-handoff-many [config-path] [pipe-name] [max-requests] [max-video-packets] [receive-timeout-ms] [expected-reassembled-frames] [stop-after-expected-reassembled-frames] [receive-buffer-bytes] [expected-reassembled-clients] [expected-reassembled-frames-per-client]"
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

fn validate_client_aware_stop_policy_or_exit(
    expected_reassembled_clients: u64,
    expected_reassembled_frames_per_client: u64,
) {
    if expected_reassembled_frames_per_client > 0 && expected_reassembled_clients == 0 {
        eprintln!(
            "invalid expected-reassembled-frames-per-client: expected-reassembled-clients must be > 0 when per-client threshold is enabled"
        );
        std::process::exit(1);
    }
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

fn format_receive_auth_video_queue_runtime_summary(
    outcome: &stream_sync_server::ServerReceiveAuthVideoQueueOnceStartupOutcome,
    policy: stream_sync_server::ServerReceiveAuthVideoQueueOnceManualPolicy,
) -> String {
    let decision = &outcome.first_auth.auth_flow.decision;
    let (video_status, queued_status, queue_len, dropped_oldest, video_summary) =
        auth_video_queue_summary(outcome);
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
    let per_client_reassembled_frames = video_summary
        .map(format_per_client_reassembled_frames)
        .unwrap_or_else(|| "none".to_string());
    let stop_reason = match (&outcome.video, video_summary) {
        (
            stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoOutcome::NotReceivedAuthRejected,
            _,
        ) => "AuthRejected".to_string(),
        (
            stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoOutcome::Received {
                summary,
                ..
            },
            _,
        ) => summary
            .stop_reason
            .map(|reason| format!("{reason:?}"))
            .unwrap_or_else(|| "none".to_string()),
    };

    format!(
        "receive auth/video queue runtime handled auth on {}; auth_accepted={} auth_reason={:?} client_id={} run_id={} video={} queued={} queue_len={} dropped_oldest={} registered_clients={} manual_max_video_packets={} manual_receive_timeout_ms={} manual_expected_reassembled_frames={} manual_stop_after_expected_reassembled_frames={} manual_expected_reassembled_clients={} manual_expected_reassembled_frames_per_client={} manual_receive_buffer_requested_bytes={} manual_receive_buffer_effective_bytes={} manual_receive_buffer_set_error={} manual_receive_buffer_read_error={} packets_received={} fragments_received={} frames_reassembled={} frames_queued={} direct_frames_queued={} rejected_packets={} rejected_fragments={} duplicate_fragments={} non_video_packets={} incomplete_reassembly_frames={} incomplete_frame_progress={} observed_reassembled_clients={} per_client_reassembled_frames={} stop_reason={} receive_timed_out={} max_packets_reached={}",
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
        policy.max_video_packets,
        policy.receive_timeout.as_millis(),
        policy.expected_reassembled_frames,
        policy.stop_after_expected_reassembled_frames,
        policy.expected_reassembled_clients,
        policy.expected_reassembled_frames_per_client,
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
            .map(|summary| summary.observed_reassembled_clients.to_string())
            .unwrap_or_else(|| "none".to_string()),
        per_client_reassembled_frames,
        stop_reason,
        video_summary
            .map(|summary| summary.receive_timed_out.to_string())
            .unwrap_or_else(|| "none".to_string()),
        video_summary
            .map(|summary| summary.max_packets_reached.to_string())
            .unwrap_or_else(|| "none".to_string())
    )
}

fn format_per_client_reassembled_frames(
    summary: &stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoSummary,
) -> String {
    if summary.per_client_reassembled_frames.is_empty() {
        return "none".to_string();
    }

    summary
        .per_client_reassembled_frames
        .iter()
        .map(|(scope, frames)| format!("{scope}:{frames}"))
        .collect::<Vec<_>>()
        .join("|")
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
            "server named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} request_status=decoded response_status=written result_kind=FrameRead queue_len_before_read={} queue_len_after_read={} selected_client_id={} selected_run_id={} frame_id={} capture_timestamp={} send_timestamp={} queued_at={} width={} height={} fps_nominal={} codec={:?} is_keyframe={} frame_payload_len={}",
            pipe_name,
            request.request_id,
            request.client_id.0,
            request.run_id.0,
            format_handoff_read_mode(request.read_mode),
            output.queue_len_before_read,
            remaining_client_queue_len,
            frame.client_id.0,
            frame.run_id.0,
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
            client_id,
            run_id,
            client_queue_len,
            ..
        } => format!(
            "server named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} request_status=decoded response_status=written result_kind=NoFrame queue_len_before_read={} queue_len_after_read={} selected_client_id={} selected_run_id={} frame_id=none frame_payload_len=none no_frame_reason={}",
            pipe_name,
            request.request_id,
            request.client_id.0,
            request.run_id.0,
            format_handoff_read_mode(request.read_mode),
            output.queue_len_before_read,
            client_queue_len,
            client_id.0,
            run_id.0,
            if output.queue_len_before_read == 0 {
                "NoFramesQueuedForClient"
            } else {
                "NoFramesQueuedForRequestedRun"
            }
        ),
        stream_sync_net_core::ServerSwitcherQueuedFrameHandoffResponse::HandoffError {
            error,
            ..
        } => format!(
            "server named-pipe handoff once pipe_name={} request_id={} client_id={} run_id={} read_mode={} request_status=decoded response_status=written result_kind=HandoffError queue_len_before_read={} queue_len_after_read=none selected_client_id={} selected_run_id={} frame_id=none frame_payload_len=none no_frame_reason=none handoff_error={:?}",
            pipe_name,
            request.request_id,
            request.client_id.0,
            request.run_id.0,
            format_handoff_read_mode(request.read_mode),
            output.queue_len_before_read,
            request.client_id.0,
            request.run_id.0,
            error
        ),
    }
}

#[cfg(windows)]
fn format_named_pipe_handoff_server_many_summary(
    pipe_name: &str,
    output: &stream_sync_server::ServerSwitcherNamedPipeManyRequestRuntimeOutput,
) -> String {
    let mut lines = vec![format!(
        "server named-pipe handoff bounded pipe_name={} max_requests={} requests_served={} successful_responses={} handoff_errors={}",
        pipe_name,
        output.max_requests,
        output.requests_served,
        output.successful_responses,
        output.handoff_errors
    )];

    lines.extend(
        output
            .requests
            .iter()
            .enumerate()
            .map(|(index, request)| {
                format!(
                    "server named-pipe handoff bounded request pipe_name={} request_index={} request_id={} queue_len_before_read={} queue_len_after_read={} result_kind={} selected_client_id={} selected_run_id={} frame_id={} frame_payload_len={} no_frame_reason={} handoff_error={}",
                    pipe_name,
                    index,
                    request.request_id,
                    request.queue_len_before_read,
                    request
                        .queue_len_after_read
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    format_server_handoff_result_kind(request.result_kind),
                    request.selected_client_id.0,
                    request.selected_run_id.0,
                    request
                        .frame_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    request
                        .frame_payload_len
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    request
                        .no_frame_reason
                        .clone()
                        .unwrap_or_else(|| "none".to_string()),
                    request
                        .handoff_error
                        .map(|value| format!("{value:?}"))
                        .unwrap_or_else(|| "none".to_string())
                )
            }),
    );

    lines.join("\n")
}

#[cfg(windows)]
fn format_named_pipe_handoff_service_session_summary(
    output: &stream_sync_server::ServerReceiveAuthVideoQueueHandoffServiceSessionOutput,
    policy: stream_sync_server::ServerReceiveAuthVideoQueueOnceManualPolicy,
) -> String {
    let receive_summary = format_receive_auth_video_queue_runtime_summary(&output.receive, policy);
    let handoff_summary =
        format_named_pipe_handoff_server_many_summary(&output.pipe_name, &output.handoff);

    format!("{receive_summary}\n{handoff_summary}")
}

#[cfg(windows)]
fn format_server_handoff_result_kind(
    kind: stream_sync_server::ServerSwitcherNamedPipeRequestResultKind,
) -> &'static str {
    match kind {
        stream_sync_server::ServerSwitcherNamedPipeRequestResultKind::FrameRead => "FrameRead",
        stream_sync_server::ServerSwitcherNamedPipeRequestResultKind::NoFrame => "NoFrame",
        stream_sync_server::ServerSwitcherNamedPipeRequestResultKind::HandoffError => {
            "HandoffError"
        }
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
        EncodedOutboundPacket, OutboundQueueItem, ServerSwitcherQueuedFrameHandoffFrame,
        ServerSwitcherQueuedFrameHandoffRequest, ServerSwitcherQueuedFrameHandoffResponse,
        ServerSwitcherQueuedFrameReadMode, SERVER_SWITCHER_HANDOFF_VERSION,
    };
    use stream_sync_protocol::{
        AuthResponse, AuthResponseReasonCode, ClientId, Codec, MessageType, ProtocolMessage,
        ProtocolVersion, RunId, TimestampMicros,
    };

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
            queue_len_before_read: 3,
        };

        let summary = super::format_named_pipe_handoff_server_summary("pipe-a", &output);

        assert!(summary.contains("pipe_name=pipe-a"));
        assert!(summary.contains("request_id=44"));
        assert!(summary.contains("result_kind=FrameRead"));
        assert!(summary.contains("queue_len_before_read=3"));
        assert!(summary.contains("queue_len_after_read=2"));
        assert!(summary.contains("frame_id=77"));
        assert!(summary.contains("codec=H264"));
        assert!(summary.contains("frame_payload_len=3"));
    }

    #[cfg(windows)]
    #[test]
    fn server_handoff_bounded_summary_includes_aggregate_and_request_fields() {
        let output = stream_sync_server::ServerSwitcherNamedPipeManyRequestRuntimeOutput {
            max_requests: 3,
            requests_served: 2,
            successful_responses: 2,
            handoff_errors: 1,
            requests: vec![
                stream_sync_server::ServerSwitcherNamedPipeRequestServeSummary {
                    request_id: 44,
                    queue_len_before_read: 3,
                    queue_len_after_read: Some(2),
                    result_kind: stream_sync_server::ServerSwitcherNamedPipeRequestResultKind::FrameRead,
                    selected_client_id: ClientId("player1".to_string()),
                    selected_run_id: RunId("run-a".to_string()),
                    frame_id: Some(77),
                    frame_payload_len: Some(3),
                    no_frame_reason: None,
                    handoff_error: None,
                },
                stream_sync_server::ServerSwitcherNamedPipeRequestServeSummary {
                    request_id: 45,
                    queue_len_before_read: 0,
                    queue_len_after_read: None,
                    result_kind: stream_sync_server::ServerSwitcherNamedPipeRequestResultKind::HandoffError,
                    selected_client_id: ClientId("player1".to_string()),
                    selected_run_id: RunId("run-a".to_string()),
                    frame_id: None,
                    frame_payload_len: None,
                    no_frame_reason: None,
                    handoff_error: Some(
                        stream_sync_net_core::ServerSwitcherQueuedFrameHandoffErrorCode::SourceShutdown,
                    ),
                },
            ],
        };

        let summary = super::format_named_pipe_handoff_server_many_summary("pipe-b", &output);

        assert!(summary.contains("pipe_name=pipe-b"));
        assert!(summary.contains("max_requests=3"));
        assert!(summary.contains("requests_served=2"));
        assert!(summary.contains("successful_responses=2"));
        assert!(summary.contains("handoff_errors=1"));
        assert!(summary.contains("request_index=0"));
        assert!(summary.contains("request_id=44"));
        assert!(summary.contains("result_kind=FrameRead"));
        assert!(summary.contains("queue_len_before_read=3"));
        assert!(summary.contains("queue_len_after_read=2"));
        assert!(summary.contains("request_index=1"));
        assert!(summary.contains("request_id=45"));
        assert!(summary.contains("result_kind=HandoffError"));
        assert!(summary.contains("queue_len_before_read=0"));
        assert!(summary.contains("queue_len_after_read=none"));
        assert!(summary.contains("handoff_error=SourceShutdown"));
    }

    #[cfg(windows)]
    #[test]
    fn server_handoff_service_session_summary_includes_receive_and_bounded_lines() {
        let summary = super::format_named_pipe_handoff_service_session_summary(
            &test_service_session_output(),
            stream_sync_server::ServerReceiveAuthVideoQueueOnceManualPolicy {
                max_video_packets: 4096,
                receive_timeout: std::time::Duration::from_millis(15_000),
                expected_reassembled_frames: 1,
                stop_after_expected_reassembled_frames: true,
                receive_buffer_bytes: 8_388_608,
                expected_reassembled_clients: 0,
                expected_reassembled_frames_per_client: 0,
            },
        );

        assert!(summary.contains("receive auth/video queue runtime handled auth on"));
        assert!(summary.contains("manual_max_video_packets=4096"));
        assert!(summary.contains("manual_expected_reassembled_clients=0"));
        assert!(summary.contains("manual_expected_reassembled_frames_per_client=0"));
        assert!(summary.contains("pipe_name=pipe-session"));
        assert!(summary.contains("max_requests=2"));
        assert!(summary.contains("requests_served=2"));
        assert!(summary.contains("request_index=0"));
        assert!(summary.contains("request_id=1"));
        assert!(summary.contains("result_kind=FrameRead"));
    }

    #[cfg(windows)]
    fn test_service_session_output(
    ) -> stream_sync_server::ServerReceiveAuthVideoQueueHandoffServiceSessionOutput {
        let source: stream_sync_net_core::PacketSource = "127.0.0.1:5000"
            .parse::<std::net::SocketAddr>()
            .expect("source should parse")
            .into();
        let client_id = ClientId("player1".to_string());
        let run_id = RunId("run-1".to_string());
        let protocol_version = ProtocolVersion(2);
        let auth_response = AuthResponse {
            message_type: MessageType::AuthResponse,
            protocol_version,
            client_id: client_id.clone(),
            run_id: run_id.clone(),
            accepted: true,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time: Some(TimestampMicros(10)),
            expected_protocol_version: None,
        };
        let outbound_response = stream_sync_server::ServerOutboundAuthResponse {
            destination: source,
            message: ProtocolMessage::AuthResponse(auth_response),
        };
        let queue_item = OutboundQueueItem {
            packet: outbound_response.clone().into_outbound_packet(),
        };

        stream_sync_server::ServerReceiveAuthVideoQueueHandoffServiceSessionOutput {
            pipe_name: "pipe-session".to_string(),
            receive: stream_sync_server::ServerReceiveAuthVideoQueueOnceStartupOutcome {
                bind_address: std::net::SocketAddr::from(([127, 0, 0, 1], 5000)),
                registry: stream_sync_server::AuthenticatedSenderRegistry::default(),
                queue_collection: stream_sync_server::ServerOutboundQueueCollection::default(),
                video_queue_state: stream_sync_server::ServerVideoFrameQueueState::default(),
                reassembly_state: stream_sync_server::ServerVideoFrameReassemblyState::default(),
                receive_buffer: stream_sync_server::ServerUdpReceiveBufferTuningResult {
                    requested_bytes: 8_388_608,
                    effective_bytes: Some(8_388_608),
                    set_error: None,
                    read_error: None,
                },
                first_auth: stream_sync_server::ServerAuthResponsePocOutcome {
                    auth_flow: stream_sync_server::ServerAuthFlowOutcome {
                        decision: stream_sync_server::ServerAuthDecision::accepted(
                            source,
                            client_id.clone(),
                            run_id.clone(),
                            protocol_version,
                            Some(TimestampMicros(10)),
                        ),
                        auth_log_input: stream_sync_server::ServerAuthLogInput {
                            source,
                            client_id: client_id.clone(),
                            run_id: run_id.clone(),
                            app_version: None,
                            protocol_version,
                            outcome: stream_sync_server::ServerAuthLogOutcome::Success,
                            reason_code: AuthResponseReasonCode::Ok,
                            message: None,
                            server_time: Some(TimestampMicros(10)),
                            expected_protocol_version: None,
                        },
                        registry_registration: None,
                        outbound_response,
                        queue_item,
                    },
                    registered_sender: None,
                    encoded_packet: EncodedOutboundPacket {
                        destination: source.into(),
                        bytes: vec![1, 2, 3],
                    },
                    bytes_sent: 3,
                },
                video: stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoOutcome::Received {
                    summary: stream_sync_server::ServerReceiveAuthVideoQueueOnceVideoSummary {
                        packets_received: 1,
                        fragments_received: 0,
                        frames_reassembled: 0,
                        frames_queued: 1,
                        direct_frames_queued: 1,
                        rejected_packets: 0,
                        rejected_fragments: 0,
                        duplicate_fragments: 0,
                        non_video_packets: 0,
                        incomplete_reassembly_frames: 0,
                        queue_len: 1,
                        incomplete_frame_progress: Vec::new(),
                        observed_reassembled_clients: 0,
                        per_client_reassembled_frames: std::collections::BTreeMap::new(),
                        receive_timed_out: false,
                        max_packets_reached: false,
                        stop_reason: Some(
                            stream_sync_server::ServerReceiveAuthVideoQueueStopReason::DirectFrameQueued,
                        ),
                    },
                    queue: None,
                },
            },
            handoff: stream_sync_server::ServerSwitcherNamedPipeManyRequestRuntimeOutput {
                max_requests: 2,
                requests_served: 2,
                successful_responses: 2,
                handoff_errors: 0,
                requests: vec![
                    stream_sync_server::ServerSwitcherNamedPipeRequestServeSummary {
                        request_id: 1,
                        queue_len_before_read: 1,
                        queue_len_after_read: Some(1),
                        result_kind:
                            stream_sync_server::ServerSwitcherNamedPipeRequestResultKind::FrameRead,
                        selected_client_id: ClientId("player1".to_string()),
                        selected_run_id: RunId("run-1".to_string()),
                        frame_id: Some(2),
                        frame_payload_len: Some(3),
                        no_frame_reason: None,
                        handoff_error: None,
                    },
                    stream_sync_server::ServerSwitcherNamedPipeRequestServeSummary {
                        request_id: 2,
                        queue_len_before_read: 1,
                        queue_len_after_read: Some(1),
                        result_kind:
                            stream_sync_server::ServerSwitcherNamedPipeRequestResultKind::FrameRead,
                        selected_client_id: ClientId("player1".to_string()),
                        selected_run_id: RunId("run-1".to_string()),
                        frame_id: Some(2),
                        frame_payload_len: Some(3),
                        no_frame_reason: None,
                        handoff_error: None,
                    },
                ],
            },
        }
    }
}
