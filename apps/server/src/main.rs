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
                    let auth_summary = &outcome.outcome.auth_flow.operational_summary;
                    let registration_summary = outcome.outcome.registration_summary.as_ref();
                    println!(
                        "auth response PoC handled one packet on {} and sent {} bytes; client_id={} run_id={} accepted={} reason_code={:?} auth_status={} auth_reason={} registration_status={} registration_reason={}",
                        outcome.bind_address,
                        outcome.outcome.bytes_sent,
                        decision.client_id.0,
                        decision.run_id.0,
                        decision.accepted,
                        decision.reason_code,
                        operational_status_name(auth_summary.status),
                        auth_operational_reason_name(auth_summary.reason),
                        format_optional_operational_status(
                            registration_summary.map(|value| value.status)
                        ),
                        format_optional_registration_reason(
                            registration_summary.map(|value| value.reason)
                        )
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
        Some("--receive-send-runtime-bounded") => {
            let command = parse_receive_send_runtime_bounded_command_args(args.collect::<Vec<_>>());
            let launcher = stream_sync_server::ServerReceiveSendRuntimeBoundedLauncher::default();
            let policy = stream_sync_server::ServerReceiveSendRuntimeBoundedPolicy {
                max_iterations: command.max_iterations,
                receive_timeout: std::time::Duration::from_millis(command.receive_timeout_ms),
            };
            let startup_config = match launcher.load_startup_config_from_path(&command.config_path)
            {
                Ok(startup_config) => startup_config,
                Err(error) => {
                    eprintln!(
                        "{}",
                        format_receive_send_runtime_bounded_failure_summary(
                            "--receive-send-runtime-bounded",
                            &command,
                            &error,
                        )
                    );
                    std::process::exit(1);
                }
            };
            let sink_config = match launcher
                .load_receive_send_iteration_json_lines_sink_config_from_path(&command.config_path)
            {
                Ok(config) => config,
                Err(error) => {
                    let error = stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkConfig(error);
                    eprintln!(
                        "{}",
                        format_receive_send_runtime_bounded_failure_summary(
                            "--receive-send-runtime-bounded",
                            &command,
                            &error,
                        )
                    );
                    std::process::exit(1);
                }
            };
            let sink_plan =
                stream_sync_server::ServerReceiveSendIterationJsonLinesSinkBoundary::default()
                    .plan(sink_config.config, sink_config.disabled);
            let sink_selection = match stream_sync_server::ServerReceiveSendIterationJsonLinesSinkSelectionBoundary::default()
                .select(&sink_plan)
            {
                Ok(selection) => selection,
                Err(error) => {
                    let error = stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkSelection(error);
                    eprintln!(
                        "{}",
                        format_receive_send_runtime_bounded_failure_summary(
                            "--receive-send-runtime-bounded",
                            &command,
                            &error,
                        )
                    );
                    std::process::exit(1);
                }
            };
            let result = match sink_selection {
                stream_sync_server::ServerReceiveSendIterationJsonLinesSinkSelection::Stderr => {
                    launcher.run_bounded_with_all_writers_and_policy(
                        startup_config,
                        std::io::stderr(),
                        std::io::stderr(),
                        std::io::stderr(),
                        std::io::stderr(),
                        std::io::stderr(),
                        policy,
                    )
                }
                stream_sync_server::ServerReceiveSendIterationJsonLinesSinkSelection::Disabled => {
                    launcher.run_bounded_with_all_writers_and_policy(
                        startup_config,
                        std::io::stderr(),
                        std::io::stderr(),
                        std::io::stderr(),
                        std::io::stderr(),
                        std::io::sink(),
                        policy,
                    )
                }
            };
            match result {
                Ok(outcome) => println!(
                    "{}",
                    format_receive_send_runtime_bounded_summary(
                        "--receive-send-runtime-bounded",
                        &command.config_path,
                        &outcome,
                    )
                ),
                Err(error) => {
                    eprintln!(
                        "{}",
                        format_receive_send_runtime_bounded_failure_summary(
                            "--receive-send-runtime-bounded",
                            &command,
                            &error,
                        )
                    );
                    std::process::exit(1);
                }
            }
        }
        Some("--receive-send-runtime-continuous") => {
            let command =
                parse_receive_send_runtime_continuous_command_args(args.collect::<Vec<_>>());
            let launcher =
                stream_sync_server::ServerReceiveSendContinuousRuntimeLauncher::default();
            let policy = stream_sync_server::ServerReceiveSendContinuousRuntimePolicy {
                receive_timeout: std::time::Duration::from_millis(command.receive_timeout_ms),
                max_iterations: command.max_iterations,
                heartbeat_timeout: Some(stream_sync_server::ServerHeartbeatTimeoutPolicy::new(
                    command.heartbeat_timeout_micros,
                )),
                receive_buffer_bytes: command.receive_buffer_bytes,
                max_packets_per_drain_cycle: command.max_packets_per_drain_cycle,
                ..stream_sync_server::ServerReceiveSendContinuousRuntimePolicy::default()
            };
            match launcher.run_from_path_with_writers_and_policy(
                &command.config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                policy,
            ) {
                Ok(outcome) => println!(
                    "{}",
                    format_receive_send_runtime_continuous_summary(
                        "--receive-send-runtime-continuous",
                        &command.config_path,
                        &outcome,
                    )
                ),
                Err(error) => {
                    eprintln!(
                        "{}",
                        format_receive_send_runtime_continuous_failure_summary(
                            "--receive-send-runtime-continuous",
                            &command,
                            &error,
                        )
                    );
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
                "stream-sync-server scaffold; use --auth-response-poc-once [config-path], --receive-send-once [config-path], --receive-send-twice [config-path], --receive-send-three [config-path], --receive-send-runtime-bounded [config-path] [max-iterations] [receive-timeout-ms], --receive-send-runtime-continuous [config-path] [receive-timeout-ms] [max-iterations-or-0-for-unbounded] [heartbeat-timeout-micros] [receive-buffer-bytes] [max-packets-per-drain-cycle], --receive-auth-video-queue-once [config-path] [max-video-packets] [receive-timeout-ms] [expected-reassembled-frames] [stop-after-expected-reassembled-frames] [receive-buffer-bytes] [expected-reassembled-clients] [expected-reassembled-frames-per-client], --receive-auth-video-queue-and-serve-handoff-once [config-path] [pipe-name] [max-video-packets] [receive-timeout-ms] [expected-reassembled-frames] [stop-after-expected-reassembled-frames] [receive-buffer-bytes] [expected-reassembled-clients] [expected-reassembled-frames-per-client], or --receive-auth-video-queue-and-serve-handoff-many [config-path] [pipe-name] [max-requests] [max-video-packets] [receive-timeout-ms] [expected-reassembled-frames] [stop-after-expected-reassembled-frames] [receive-buffer-bytes] [expected-reassembled-clients] [expected-reassembled-frames-per-client]"
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReceiveSendRuntimeBoundedCommandArgs {
    config_path: String,
    max_iterations: usize,
    receive_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReceiveSendRuntimeContinuousCommandArgs {
    config_path: String,
    receive_timeout_ms: u64,
    max_iterations: Option<usize>,
    heartbeat_timeout_micros: u64,
    receive_buffer_bytes: usize,
    max_packets_per_drain_cycle: usize,
}

fn parse_receive_send_runtime_bounded_command_args(
    args: Vec<String>,
) -> ReceiveSendRuntimeBoundedCommandArgs {
    let mut args = args.into_iter();
    ReceiveSendRuntimeBoundedCommandArgs {
        config_path: args
            .next()
            .unwrap_or_else(|| "configs/examples/server.example.toml".to_string()),
        max_iterations: parse_optional_arg_or_exit::<usize>(args.next(), "max-iterations")
            .unwrap_or(16),
        receive_timeout_ms: parse_optional_arg_or_exit::<u64>(args.next(), "receive-timeout-ms")
            .unwrap_or(1_000),
    }
}

fn parse_receive_send_runtime_continuous_command_args(
    args: Vec<String>,
) -> ReceiveSendRuntimeContinuousCommandArgs {
    let mut args = args.into_iter();
    ReceiveSendRuntimeContinuousCommandArgs {
        config_path: args
            .next()
            .unwrap_or_else(|| "configs/examples/server.example.toml".to_string()),
        receive_timeout_ms: parse_optional_arg_or_exit::<u64>(args.next(), "receive-timeout-ms")
            .unwrap_or(1_000),
        max_iterations: parse_optional_arg_or_exit::<usize>(
            args.next(),
            "max-iterations-or-0-for-unbounded",
        )
        .and_then(|value| (value > 0).then_some(value)),
        heartbeat_timeout_micros: parse_optional_arg_or_exit::<u64>(
            args.next(),
            "heartbeat-timeout-micros",
        )
        .unwrap_or(5_000_000),
        receive_buffer_bytes: parse_optional_arg_or_exit::<usize>(
            args.next(),
            "receive-buffer-bytes",
        )
        .unwrap_or(stream_sync_server::SERVER_DEFAULT_RECEIVE_BUFFER_BYTES),
        max_packets_per_drain_cycle: parse_optional_arg_or_exit::<usize>(
            args.next(),
            "max-packets-per-drain-cycle",
        )
        .unwrap_or(
            stream_sync_server::ServerReceiveSendContinuousRuntimePolicy::default()
                .max_packets_per_drain_cycle,
        ),
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

fn format_receive_send_runtime_bounded_summary(
    command_name: &str,
    config_path: &str,
    outcome: &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupOutcome,
) -> String {
    let summary = &outcome.summary;
    format!(
        "command_name={} config_path={} max_iterations={} receive_timeout_ms={} iterations_attempted={} iterations_completed={} auth_requests_received={} auth_responses_sent={} heartbeats_received={} heartbeat_acks_sent={} client_stats_received={} client_stats_returns_sent={} accepted_packets={} rejected_packets={} decode_errors={} send_failures={} timeout_iterations={} timeout_only_run={} outbound_queue_len={} registered_clients={} last_receive_error={} last_send_error={} last_rejected_reason={} last_auth_status={} last_auth_reason={} last_registration_status={} last_registration_reason={} last_runtime_rejection_status={} last_runtime_rejection_reason={} stop_reason={}",
        command_name,
        config_path,
        summary.max_iterations,
        summary.receive_timeout.as_millis(),
        summary.iterations_attempted,
        summary.iterations_completed,
        summary.auth_requests_received,
        summary.auth_responses_sent,
        summary.heartbeats_received,
        summary.heartbeat_acks_sent,
        summary.client_stats_received,
        summary.client_stats_returns_sent,
        summary.accepted_packets,
        summary.rejected_packets,
        summary.decode_errors,
        summary.send_failures,
        summary.timeout_iterations,
        summary.timeout_only_run,
        summary.outbound_queue_len,
        summary.registered_clients,
        format_optional_error_kind(summary.last_receive_error),
        format_optional_string(summary.last_send_error.as_deref()),
        format_optional_string(summary.last_rejected_reason.as_deref()),
        format_optional_operational_status(
            summary.last_auth_summary.as_ref().map(|value| value.status)
        ),
        format_optional_auth_reason(summary.last_auth_summary.as_ref().map(|value| value.reason)),
        format_optional_operational_status(
            summary
                .last_registration_summary
                .as_ref()
                .map(|value| value.status)
        ),
        format_optional_registration_reason(
            summary
                .last_registration_summary
                .as_ref()
                .map(|value| value.reason)
        ),
        format_optional_operational_status(
            summary
                .last_runtime_rejection_summary
                .as_ref()
                .map(|value| value.status)
        ),
        format_optional_packet_reject_reason(
            summary
                .last_runtime_rejection_summary
                .as_ref()
                .map(|value| value.reason)
        ),
        receive_send_runtime_bounded_stop_reason_name(summary.stop_reason),
    )
}

fn format_receive_send_runtime_continuous_summary(
    command_name: &str,
    config_path: &str,
    outcome: &stream_sync_server::ServerReceiveSendContinuousRuntimeOutcome,
) -> String {
    let summary = &outcome.summary;
    let last_heartbeat_timeout = summary.last_heartbeat_timeout_summary.as_ref();
    let effective_receive_buffer = summary
        .receive_buffer
        .effective_bytes
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    let receive_buffer_set_error = summary
        .receive_buffer
        .set_error
        .as_deref()
        .unwrap_or("none");
    let receive_buffer_read_error = summary
        .receive_buffer
        .read_error
        .as_deref()
        .unwrap_or("none");
    format!(
        "command_name={} config_path={} receive_timeout_ms={} max_iterations={} heartbeat_timeout_micros={} receive_buffer_requested_bytes={} receive_buffer_effective_bytes={} receive_buffer_set_error={} receive_buffer_read_error={} max_packets_per_drain_cycle={} iterations_attempted={} iterations_completed={} packets_received={} accepted_packets={} rejected_packets={} decode_errors={} auth_requests_received={} auth_responses_sent={} heartbeats_received={} heartbeat_acks_sent={} client_stats_received={} heartbeat_observations_committed={} frames_reassembled={} frames_queued={} direct_frames_queued={} video_queue_len={} incomplete_reassembly_frames={} drain_cycles={} last_packets_drained_in_cycle={} max_packets_drained_in_cycle={} receive_would_block_count={} outbound_queue_len={} registered_clients={} heartbeat_liveness_clients={} heartbeat_rtt_offset_clients={} last_receive_error={} last_send_error={} last_rejected_reason={} last_auth_status={} last_auth_reason={} last_registration_status={} last_registration_reason={} last_runtime_rejection_status={} last_runtime_rejection_reason={} last_heartbeat_timeout_status={} last_heartbeat_timeout_clients={} last_heartbeat_timeout_timed_out={} last_heartbeat_timeout_client={} last_heartbeat_timeout_reason={} stop_reason={}",
        command_name,
        config_path,
        summary.receive_timeout.as_millis(),
        format_optional_usize(summary.max_iterations),
        last_heartbeat_timeout
            .and_then(|value| {
                value.most_severe_client_summary
                    .as_ref()
                    .and_then(|client| client.timeout_after_micros)
            })
            .unwrap_or(0),
        summary.receive_buffer.requested_bytes,
        effective_receive_buffer,
        receive_buffer_set_error,
        receive_buffer_read_error,
        summary.max_packets_per_drain_cycle,
        summary.iterations_attempted,
        summary.iterations_completed,
        summary.packets_received,
        summary.accepted_packets,
        summary.rejected_packets,
        summary.decode_errors,
        summary.auth_requests_received,
        summary.auth_responses_sent,
        summary.heartbeats_received,
        summary.heartbeat_acks_sent,
        summary.client_stats_received,
        summary.heartbeat_observations_committed,
        summary.frames_reassembled,
        summary.frames_queued,
        summary.direct_frames_queued,
        summary.video_queue_len,
        summary.incomplete_reassembly_frames,
        summary.drain_cycles,
        summary.last_packets_drained_in_cycle,
        summary.max_packets_drained_in_cycle,
        summary.receive_would_block_count,
        summary.outbound_queue_len,
        summary.registered_clients,
        summary.heartbeat_liveness_clients,
        summary.heartbeat_rtt_offset_clients,
        format_optional_error_kind(summary.last_receive_error),
        format_optional_string(summary.last_send_error.as_deref()),
        format_optional_string(summary.last_rejected_reason.as_deref()),
        format_optional_operational_status(
            summary.last_auth_summary.as_ref().map(|value| value.status)
        ),
        format_optional_auth_reason(summary.last_auth_summary.as_ref().map(|value| value.reason)),
        format_optional_operational_status(
            summary
                .last_registration_summary
                .as_ref()
                .map(|value| value.status)
        ),
        format_optional_registration_reason(
            summary
                .last_registration_summary
                .as_ref()
                .map(|value| value.reason)
        ),
        format_optional_operational_status(
            summary
                .last_runtime_rejection_summary
                .as_ref()
                .map(|value| value.status)
        ),
        format_optional_packet_reject_reason(
            summary
                .last_runtime_rejection_summary
                .as_ref()
                .map(|value| value.reason)
        ),
        format_optional_operational_status(last_heartbeat_timeout.map(|value| value.status)),
        last_heartbeat_timeout
            .map(|value| value.clients_evaluated)
            .unwrap_or(0),
        last_heartbeat_timeout
            .map(|value| value.timed_out_clients)
            .unwrap_or(0),
        format_optional_string(
            last_heartbeat_timeout
                .and_then(|value| value.most_severe_client_summary.as_ref())
                .map(|value| value.client_id.0.as_str())
        ),
        format_optional_heartbeat_reason(
            last_heartbeat_timeout
                .and_then(|value| value.most_severe_client_summary.as_ref())
                .map(|value| value.reason)
        ),
        receive_send_runtime_continuous_stop_reason_name(summary.stop_reason),
    )
}

fn format_receive_send_runtime_bounded_failure_summary(
    command_name: &str,
    command: &ReceiveSendRuntimeBoundedCommandArgs,
    error: &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError,
) -> String {
    let partial_summary = bounded_runtime_error_partial_summary(error);
    let iterations_attempted = partial_summary
        .as_ref()
        .map(|summary| summary.iterations_attempted)
        .unwrap_or(0);
    let iterations_completed = partial_summary
        .as_ref()
        .map(|summary| summary.iterations_completed)
        .unwrap_or(0);
    let rejected_packets = partial_summary
        .as_ref()
        .map(|summary| summary.rejected_packets)
        .unwrap_or(0);
    let decode_errors = partial_summary
        .as_ref()
        .map(|summary| summary.decode_errors)
        .unwrap_or(0);
    let send_failures = partial_summary
        .as_ref()
        .map(|summary| summary.send_failures)
        .unwrap_or(0);
    let timeout_iterations = partial_summary
        .as_ref()
        .map(|summary| summary.timeout_iterations)
        .unwrap_or(0);
    let timeout_only_run = partial_summary
        .as_ref()
        .map(|summary| summary.timeout_only_run)
        .unwrap_or(false);
    let last_receive_error = partial_summary
        .as_ref()
        .and_then(|summary| summary.last_receive_error);
    let last_send_error = partial_summary
        .as_ref()
        .and_then(|summary| summary.last_send_error.as_deref());
    let last_rejected_reason = partial_summary
        .as_ref()
        .and_then(|summary| summary.last_rejected_reason.as_deref());
    let last_auth_status = partial_summary
        .as_ref()
        .and_then(|summary| summary.last_auth_summary.as_ref().map(|value| value.status));
    let last_auth_reason = partial_summary
        .as_ref()
        .and_then(|summary| summary.last_auth_summary.as_ref().map(|value| value.reason));
    let last_registration_status = partial_summary.as_ref().and_then(|summary| {
        summary
            .last_registration_summary
            .as_ref()
            .map(|value| value.status)
    });
    let last_registration_reason = partial_summary.as_ref().and_then(|summary| {
        summary
            .last_registration_summary
            .as_ref()
            .map(|value| value.reason)
    });
    let last_runtime_rejection_status = partial_summary.as_ref().and_then(|summary| {
        summary
            .last_runtime_rejection_summary
            .as_ref()
            .map(|value| value.status)
    });
    let last_runtime_rejection_reason = partial_summary.as_ref().and_then(|summary| {
        summary
            .last_runtime_rejection_summary
            .as_ref()
            .map(|value| value.reason)
    });
    format!(
        "command_name={} config_path={} max_iterations={} receive_timeout_ms={} iterations_attempted={} iterations_completed={} rejected_packets={} decode_errors={} send_failures={} timeout_iterations={} timeout_only_run={} last_receive_error={} last_send_error={} last_rejected_reason={} last_auth_status={} last_auth_reason={} last_registration_status={} last_registration_reason={} last_runtime_rejection_status={} last_runtime_rejection_reason={} stop_reason={} fatal_error_kind={} fatal_error_detail={}",
        command_name,
        command.config_path,
        command.max_iterations,
        command.receive_timeout_ms,
        iterations_attempted,
        iterations_completed,
        rejected_packets,
        decode_errors,
        send_failures,
        timeout_iterations,
        timeout_only_run,
        format_optional_error_kind(last_receive_error),
        format_optional_string(last_send_error),
        format_optional_string(last_rejected_reason),
        format_optional_operational_status(last_auth_status),
        format_optional_auth_reason(last_auth_reason),
        format_optional_operational_status(last_registration_status),
        format_optional_registration_reason(last_registration_reason),
        format_optional_operational_status(last_runtime_rejection_status),
        format_optional_packet_reject_reason(last_runtime_rejection_reason),
        bounded_runtime_error_stop_reason(error),
        bounded_runtime_error_kind(error),
        bounded_runtime_error_detail(error),
    )
}

fn format_receive_send_runtime_continuous_failure_summary(
    command_name: &str,
    command: &ReceiveSendRuntimeContinuousCommandArgs,
    error: &stream_sync_server::ServerReceiveSendContinuousRuntimeError,
) -> String {
    let partial_summary = continuous_runtime_error_partial_summary(error);
    let receive_buffer_requested = partial_summary
        .as_ref()
        .map(|summary| summary.receive_buffer.requested_bytes)
        .unwrap_or(command.receive_buffer_bytes);
    let receive_buffer_effective = partial_summary
        .as_ref()
        .and_then(|summary| summary.receive_buffer.effective_bytes)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    let receive_buffer_set_error = partial_summary
        .as_ref()
        .and_then(|summary| summary.receive_buffer.set_error.as_deref())
        .unwrap_or("none");
    let receive_buffer_read_error = partial_summary
        .as_ref()
        .and_then(|summary| summary.receive_buffer.read_error.as_deref())
        .unwrap_or("none");
    format!(
        "command_name={} config_path={} receive_timeout_ms={} max_iterations={} heartbeat_timeout_micros={} receive_buffer_requested_bytes={} receive_buffer_effective_bytes={} receive_buffer_set_error={} receive_buffer_read_error={} max_packets_per_drain_cycle={} iterations_attempted={} iterations_completed={} packets_received={} accepted_packets={} rejected_packets={} frames_reassembled={} frames_queued={} video_queue_len={} last_receive_error={} last_send_error={} last_rejected_reason={} last_auth_status={} last_auth_reason={} last_registration_status={} last_registration_reason={} last_runtime_rejection_status={} last_runtime_rejection_reason={} last_heartbeat_timeout_status={} stop_reason={} fatal_error_kind={} fatal_error_detail={}",
        command_name,
        command.config_path,
        command.receive_timeout_ms,
        format_optional_usize(command.max_iterations),
        command.heartbeat_timeout_micros,
        receive_buffer_requested,
        receive_buffer_effective,
        receive_buffer_set_error,
        receive_buffer_read_error,
        partial_summary
            .as_ref()
            .map(|summary| summary.max_packets_per_drain_cycle)
            .unwrap_or(command.max_packets_per_drain_cycle),
        partial_summary
            .as_ref()
            .map(|summary| summary.iterations_attempted)
            .unwrap_or(0),
        partial_summary
            .as_ref()
            .map(|summary| summary.iterations_completed)
            .unwrap_or(0),
        partial_summary
            .as_ref()
            .map(|summary| summary.packets_received)
            .unwrap_or(0),
        partial_summary
            .as_ref()
            .map(|summary| summary.accepted_packets)
            .unwrap_or(0),
        partial_summary
            .as_ref()
            .map(|summary| summary.rejected_packets)
            .unwrap_or(0),
        partial_summary
            .as_ref()
            .map(|summary| summary.frames_reassembled)
            .unwrap_or(0),
        partial_summary
            .as_ref()
            .map(|summary| summary.frames_queued)
            .unwrap_or(0),
        partial_summary
            .as_ref()
            .map(|summary| summary.video_queue_len)
            .unwrap_or(0),
        format_optional_error_kind(
            partial_summary
                .as_ref()
                .and_then(|summary| summary.last_receive_error)
        ),
        format_optional_string(
            partial_summary
                .as_ref()
                .and_then(|summary| summary.last_send_error.as_deref())
        ),
        format_optional_string(
            partial_summary
                .as_ref()
                .and_then(|summary| summary.last_rejected_reason.as_deref())
        ),
        format_optional_operational_status(partial_summary.as_ref().and_then(|summary| {
            summary.last_auth_summary.as_ref().map(|value| value.status)
        })),
        format_optional_auth_reason(partial_summary.as_ref().and_then(|summary| {
            summary.last_auth_summary.as_ref().map(|value| value.reason)
        })),
        format_optional_operational_status(partial_summary.as_ref().and_then(|summary| {
            summary
                .last_registration_summary
                .as_ref()
                .map(|value| value.status)
        })),
        format_optional_registration_reason(partial_summary.as_ref().and_then(|summary| {
            summary
                .last_registration_summary
                .as_ref()
                .map(|value| value.reason)
        })),
        format_optional_operational_status(partial_summary.as_ref().and_then(|summary| {
            summary
                .last_runtime_rejection_summary
                .as_ref()
                .map(|value| value.status)
        })),
        format_optional_packet_reject_reason(partial_summary.as_ref().and_then(|summary| {
            summary
                .last_runtime_rejection_summary
                .as_ref()
                .map(|value| value.reason)
        })),
        format_optional_operational_status(partial_summary.as_ref().and_then(|summary| {
            summary
                .last_heartbeat_timeout_summary
                .as_ref()
                .map(|value| value.status)
        })),
        continuous_runtime_error_stop_reason(error),
        continuous_runtime_error_kind(error),
        continuous_runtime_error_detail(error),
    )
}

fn receive_send_runtime_bounded_stop_reason_name(
    reason: stream_sync_server::ServerReceiveSendRuntimeBoundedStopReason,
) -> String {
    match reason {
        stream_sync_server::ServerReceiveSendRuntimeBoundedStopReason::MaxIterationsReached => {
            "MaxIterationsReached".to_string()
        }
        stream_sync_server::ServerReceiveSendRuntimeBoundedStopReason::ReceiveTimedOut => {
            "ReceiveTimedOut".to_string()
        }
        stream_sync_server::ServerReceiveSendRuntimeBoundedStopReason::ControllerStopped => {
            "ControllerStopped".to_string()
        }
        stream_sync_server::ServerReceiveSendRuntimeBoundedStopReason::SocketReceiveFailed(
            error_kind,
        ) => format!("SocketReceiveFailed({error_kind:?})"),
    }
}

fn receive_send_runtime_continuous_stop_reason_name(
    reason: stream_sync_server::ServerReceiveSendContinuousRuntimeStopReason,
) -> String {
    match reason {
        stream_sync_server::ServerReceiveSendContinuousRuntimeStopReason::MaxIterationsReached => {
            "MaxIterationsReached".to_string()
        }
        stream_sync_server::ServerReceiveSendContinuousRuntimeStopReason::ReceiveTimedOut => {
            "ReceiveTimedOut".to_string()
        }
        stream_sync_server::ServerReceiveSendContinuousRuntimeStopReason::ControllerStopped => {
            "ControllerStopped".to_string()
        }
        stream_sync_server::ServerReceiveSendContinuousRuntimeStopReason::SocketReceiveFailed(
            error_kind,
        ) => format!("SocketReceiveFailed({error_kind:?})"),
    }
}

fn bounded_runtime_error_stop_reason(
    error: &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError,
) -> &'static str {
    match error {
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::Runtime { .. } => {
            "RuntimeFatalError"
        }
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkConfig(
            _,
        )
        | stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkSelection(
            _,
        )
        | stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::OneIterationStartup(_)
        | stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::Bind { .. }
        | stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::SetReceiveTimeout {
            ..
        } => "StartupFailure",
    }
}

fn bounded_runtime_error_kind(
    error: &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError,
) -> &'static str {
    match error {
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkConfig(
            stream_sync_server::ServerAuthResponsePocStartupError::Config(_),
        ) => "ConfigLoadFailure",
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkConfig(
            _,
        ) => "StartupFailure",
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkSelection(
            _,
        ) => "IterationEventSinkDeferred",
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::OneIterationStartup(_) => {
            "ConfigLoadFailure"
        }
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::Bind { .. } => {
            "SocketBindFailure"
        }
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::SetReceiveTimeout {
            ..
        } => "SocketReceiveSetupFailure",
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::Runtime {
            error, ..
        } => match error {
            stream_sync_server::ServerControllerReceiveSendRuntimeError::Iteration(
                stream_sync_server::ServerReceiveSendOneIterationRuntimeError::Send(_),
            ) => "SendFailure",
            stream_sync_server::ServerControllerReceiveSendRuntimeError::Iteration(
                stream_sync_server::ServerReceiveSendOneIterationRuntimeError::ReceiveBody(_),
            ) => "SocketReceiveFailure",
            _ => "RuntimeFatalError",
        },
    }
}

fn bounded_runtime_error_detail(
    error: &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError,
) -> String {
    match error {
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkConfig(
            startup_error,
        ) => format!("{startup_error:?}"),
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkSelection(
            stream_sync_server::ServerReceiveSendIterationJsonLinesSinkSelectionError::FileDestinationDeferred {
                path,
            },
        ) => format!("file_destination_deferred path={}", path.display()),
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::Runtime {
            iteration_index,
            error,
            ..
        } => format!("iteration_index={} error={error:?}", iteration_index),
        other => format!("{other:?}"),
    }
}

fn bounded_runtime_error_partial_summary(
    error: &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError,
) -> Option<&stream_sync_server::ServerReceiveSendRuntimeBoundedSummary> {
    match error {
        stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::Runtime {
            summary,
            ..
        } => Some(summary),
        _ => None,
    }
}

fn continuous_runtime_error_stop_reason(
    error: &stream_sync_server::ServerReceiveSendContinuousRuntimeError,
) -> &'static str {
    match error {
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::Runtime { .. } => {
            "RuntimeFatalError"
        }
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::OneIterationStartup(_)
        | stream_sync_server::ServerReceiveSendContinuousRuntimeError::Bind { .. }
        | stream_sync_server::ServerReceiveSendContinuousRuntimeError::SetReceiveTimeout {
            ..
        }
        | stream_sync_server::ServerReceiveSendContinuousRuntimeError::SetNonblocking { .. } => {
            "StartupFailure"
        }
    }
}

fn continuous_runtime_error_kind(
    error: &stream_sync_server::ServerReceiveSendContinuousRuntimeError,
) -> &'static str {
    match error {
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::OneIterationStartup(_) => {
            "ConfigLoadFailure"
        }
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::Bind { .. } => {
            "SocketBindFailure"
        }
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::SetReceiveTimeout {
            ..
        } => "SocketReceiveSetupFailure",
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::SetNonblocking { .. } => {
            "SocketReceiveSetupFailure"
        }
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::Runtime { error, .. } => {
            match error {
                stream_sync_server::ServerControllerReceiveSendRuntimeError::Iteration(
                    stream_sync_server::ServerReceiveSendOneIterationRuntimeError::Send(_),
                ) => "SendFailure",
                stream_sync_server::ServerControllerReceiveSendRuntimeError::Iteration(
                    stream_sync_server::ServerReceiveSendOneIterationRuntimeError::ReceiveBody(_),
                ) => "SocketReceiveFailure",
                _ => "RuntimeFatalError",
            }
        }
    }
}

fn continuous_runtime_error_detail(
    error: &stream_sync_server::ServerReceiveSendContinuousRuntimeError,
) -> String {
    match error {
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::Runtime {
            iteration_index,
            error,
            ..
        } => format!("iteration_index={} error={error:?}", iteration_index),
        other => format!("{other:?}"),
    }
}

fn continuous_runtime_error_partial_summary(
    error: &stream_sync_server::ServerReceiveSendContinuousRuntimeError,
) -> Option<&stream_sync_server::ServerReceiveSendContinuousRuntimeSummary> {
    match error {
        stream_sync_server::ServerReceiveSendContinuousRuntimeError::Runtime {
            summary, ..
        } => Some(summary),
        _ => None,
    }
}

fn format_optional_error_kind(error_kind: Option<std::io::ErrorKind>) -> String {
    error_kind
        .map(|kind| format!("{kind:?}"))
        .unwrap_or_else(|| "none".to_string())
}

fn format_optional_string(value: Option<&str>) -> String {
    value.unwrap_or("none").to_string()
}

fn format_optional_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn operational_status_name(
    status: stream_sync_server::ServerOperationalConditionStatus,
) -> &'static str {
    match status {
        stream_sync_server::ServerOperationalConditionStatus::Continue => "Continue",
        stream_sync_server::ServerOperationalConditionStatus::Reject => "Reject",
        stream_sync_server::ServerOperationalConditionStatus::ReconnectRequired => {
            "ReconnectRequired"
        }
        stream_sync_server::ServerOperationalConditionStatus::InvestigationRequired => {
            "InvestigationRequired"
        }
    }
}

fn auth_operational_reason_name(
    reason: stream_sync_server::ServerAuthOperationalReason,
) -> &'static str {
    match reason {
        stream_sync_server::ServerAuthOperationalReason::Accepted => "Accepted",
        stream_sync_server::ServerAuthOperationalReason::InvalidToken => "InvalidToken",
        stream_sync_server::ServerAuthOperationalReason::UnknownClient => "UnknownClient",
        stream_sync_server::ServerAuthOperationalReason::ProtocolMismatch => "ProtocolMismatch",
        stream_sync_server::ServerAuthOperationalReason::AlreadyConnected => "AlreadyConnected",
        stream_sync_server::ServerAuthOperationalReason::InternalError => "InternalError",
    }
}

fn registration_operational_reason_name(
    reason: stream_sync_server::AuthenticatedSenderRegistrationReason,
) -> &'static str {
    match reason {
        stream_sync_server::AuthenticatedSenderRegistrationReason::FreshRegistration => {
            "FreshRegistration"
        }
        stream_sync_server::AuthenticatedSenderRegistrationReason::IdempotentReregistration => {
            "IdempotentReregistration"
        }
        stream_sync_server::AuthenticatedSenderRegistrationReason::RunReplaced => "RunReplaced",
        stream_sync_server::AuthenticatedSenderRegistrationReason::SourceReplaced => {
            "SourceReplaced"
        }
        stream_sync_server::AuthenticatedSenderRegistrationReason::SourceAndRunReplaced => {
            "SourceAndRunReplaced"
        }
    }
}

fn packet_reject_reason_name(
    reason: stream_sync_server::PacketAcceptanceRejectReason,
) -> &'static str {
    match reason {
        stream_sync_server::PacketAcceptanceRejectReason::UnauthenticatedSource => {
            "UnauthenticatedSource"
        }
        stream_sync_server::PacketAcceptanceRejectReason::UnknownClient => "UnknownClient",
        stream_sync_server::PacketAcceptanceRejectReason::EndpointMismatch => "EndpointMismatch",
        stream_sync_server::PacketAcceptanceRejectReason::RunIdMismatch => "RunIdMismatch",
    }
}

fn heartbeat_operational_reason_name(
    reason: stream_sync_server::ServerHeartbeatOperationalReason,
) -> &'static str {
    match reason {
        stream_sync_server::ServerHeartbeatOperationalReason::NoHeartbeatYet => "NoHeartbeatYet",
        stream_sync_server::ServerHeartbeatOperationalReason::Alive => "Alive",
        stream_sync_server::ServerHeartbeatOperationalReason::TimedOut => "TimedOut",
    }
}

fn format_optional_operational_status(
    status: Option<stream_sync_server::ServerOperationalConditionStatus>,
) -> String {
    status
        .map(operational_status_name)
        .unwrap_or("none")
        .to_string()
}

fn format_optional_auth_reason(
    reason: Option<stream_sync_server::ServerAuthOperationalReason>,
) -> String {
    reason
        .map(auth_operational_reason_name)
        .unwrap_or("none")
        .to_string()
}

fn format_optional_registration_reason(
    reason: Option<stream_sync_server::AuthenticatedSenderRegistrationReason>,
) -> String {
    reason
        .map(registration_operational_reason_name)
        .unwrap_or("none")
        .to_string()
}

fn format_optional_packet_reject_reason(
    reason: Option<stream_sync_server::PacketAcceptanceRejectReason>,
) -> String {
    reason
        .map(packet_reject_reason_name)
        .unwrap_or("none")
        .to_string()
}

fn format_optional_heartbeat_reason(
    reason: Option<stream_sync_server::ServerHeartbeatOperationalReason>,
) -> String {
    reason
        .map(heartbeat_operational_reason_name)
        .unwrap_or("none")
        .to_string()
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
    let auth_summary = &outcome.first_auth.auth_flow.operational_summary;
    let registration_summary = outcome.first_auth.registration_summary.as_ref();
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
        "receive auth/video queue runtime handled auth on {}; auth_accepted={} auth_reason={:?} auth_status={} auth_operational_reason={} registration_status={} registration_reason={} client_id={} run_id={} video={} queued={} queue_len={} dropped_oldest={} registered_clients={} manual_max_video_packets={} manual_receive_timeout_ms={} manual_expected_reassembled_frames={} manual_stop_after_expected_reassembled_frames={} manual_expected_reassembled_clients={} manual_expected_reassembled_frames_per_client={} manual_receive_buffer_requested_bytes={} manual_receive_buffer_effective_bytes={} manual_receive_buffer_set_error={} manual_receive_buffer_read_error={} packets_received={} fragments_received={} frames_reassembled={} frames_queued={} direct_frames_queued={} rejected_packets={} rejected_fragments={} duplicate_fragments={} non_video_packets={} incomplete_reassembly_frames={} incomplete_frame_progress={} observed_reassembled_clients={} per_client_reassembled_frames={} stop_reason={} receive_timed_out={} max_packets_reached={}",
        outcome.bind_address,
        decision.accepted,
        decision.reason_code,
        operational_status_name(auth_summary.status),
        auth_operational_reason_name(auth_summary.reason),
        format_optional_operational_status(registration_summary.map(|value| value.status)),
        format_optional_registration_reason(registration_summary.map(|value| value.reason)),
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

    use super::{
        format_handoff_read_mode, format_receive_send_runtime_bounded_failure_summary,
        format_receive_send_runtime_bounded_summary,
        format_receive_send_runtime_continuous_failure_summary,
        format_receive_send_runtime_continuous_summary,
        parse_receive_send_runtime_bounded_command_args,
        parse_receive_send_runtime_continuous_command_args,
    };

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

    #[test]
    fn parse_receive_send_runtime_bounded_command_uses_defaults() {
        let args = parse_receive_send_runtime_bounded_command_args(Vec::new());

        assert_eq!(args.config_path, "configs/examples/server.example.toml");
        assert_eq!(args.max_iterations, 16);
        assert_eq!(args.receive_timeout_ms, 1_000);
    }

    #[test]
    fn parse_receive_send_runtime_bounded_command_parses_custom_values() {
        let args = parse_receive_send_runtime_bounded_command_args(vec![
            "configs/custom/server.toml".to_string(),
            "24".to_string(),
            "250".to_string(),
        ]);

        assert_eq!(args.config_path, "configs/custom/server.toml");
        assert_eq!(args.max_iterations, 24);
        assert_eq!(args.receive_timeout_ms, 250);
    }

    #[test]
    fn parse_receive_send_runtime_continuous_command_uses_defaults() {
        let args = parse_receive_send_runtime_continuous_command_args(Vec::new());

        assert_eq!(args.config_path, "configs/examples/server.example.toml");
        assert_eq!(args.receive_timeout_ms, 1_000);
        assert_eq!(args.max_iterations, None);
        assert_eq!(args.heartbeat_timeout_micros, 5_000_000);
        assert_eq!(
            args.receive_buffer_bytes,
            stream_sync_server::SERVER_DEFAULT_RECEIVE_BUFFER_BYTES
        );
        assert_eq!(args.max_packets_per_drain_cycle, 64);
    }

    #[test]
    fn parse_receive_send_runtime_continuous_command_parses_custom_values() {
        let args = parse_receive_send_runtime_continuous_command_args(vec![
            "configs/custom/server.toml".to_string(),
            "250".to_string(),
            "32".to_string(),
            "9000000".to_string(),
            "4194304".to_string(),
            "256".to_string(),
        ]);

        assert_eq!(args.config_path, "configs/custom/server.toml");
        assert_eq!(args.receive_timeout_ms, 250);
        assert_eq!(args.max_iterations, Some(32));
        assert_eq!(args.heartbeat_timeout_micros, 9_000_000);
        assert_eq!(args.receive_buffer_bytes, 4_194_304);
        assert_eq!(args.max_packets_per_drain_cycle, 256);
    }

    #[test]
    fn parse_receive_send_runtime_continuous_command_treats_zero_max_iterations_as_unbounded() {
        let args = parse_receive_send_runtime_continuous_command_args(vec![
            "configs/custom/server.toml".to_string(),
            "250".to_string(),
            "0".to_string(),
            "9000000".to_string(),
            "2097152".to_string(),
            "512".to_string(),
        ]);

        assert_eq!(args.max_iterations, None);
        assert_eq!(args.receive_buffer_bytes, 2_097_152);
        assert_eq!(args.max_packets_per_drain_cycle, 512);
    }

    #[test]
    fn receive_send_runtime_bounded_summary_includes_required_fields() {
        let summary = format_receive_send_runtime_bounded_summary(
            "--receive-send-runtime-bounded",
            "configs/examples/server.example.toml",
            &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupOutcome {
                bind_address: "127.0.0.1:5000"
                    .parse()
                    .expect("bind address should parse"),
                registry: stream_sync_server::AuthenticatedSenderRegistry::default(),
                queue_collection: stream_sync_server::ServerOutboundQueueCollection::default(),
                iterations: Vec::new(),
                iteration_events: Vec::new(),
                iteration_event_log_summary:
                    stream_sync_server::ServerReceiveSendRuntimeBoundedIterationEventLogSummary {
                        lines_written: 0,
                        write_failures: 0,
                        last_writer_error: None,
                    },
                summary: stream_sync_server::ServerReceiveSendRuntimeBoundedSummary {
                    max_iterations: 16,
                    receive_timeout: std::time::Duration::from_millis(1_000),
                    iterations_attempted: 4,
                    iterations_completed: 4,
                    auth_requests_received: 1,
                    auth_responses_sent: 1,
                    heartbeats_received: 1,
                    heartbeat_acks_sent: 1,
                    client_stats_received: 1,
                    client_stats_returns_sent: 1,
                    accepted_packets: 3,
                    rejected_packets: 0,
                    decode_errors: 0,
                    send_failures: 0,
                    timeout_iterations: 1,
                    timeout_only_run: false,
                    outbound_queue_len: 0,
                    registered_clients: 1,
                    last_receive_error: Some(std::io::ErrorKind::TimedOut),
                    last_send_error: None,
                    last_rejected_reason: Some("Auth:InvalidToken".to_string()),
                    last_auth_summary: Some(stream_sync_server::ServerAuthOperationalSummary {
                        status: stream_sync_server::ServerOperationalConditionStatus::Reject,
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        reason: stream_sync_server::ServerAuthOperationalReason::InvalidToken,
                    }),
                    last_registration_summary: Some(
                        stream_sync_server::AuthenticatedSenderRegistrationSummary {
                            status: stream_sync_server::ServerOperationalConditionStatus::Continue,
                            client_id: ClientId("client-1".to_string()),
                            previous_source: None,
                            current_source: std::net::SocketAddr::from(([127, 0, 0, 1], 5000))
                                .into(),
                            previous_run_id: None,
                            current_run_id: RunId("run-1".to_string()),
                            reason: stream_sync_server::AuthenticatedSenderRegistrationReason::FreshRegistration,
                            entry: stream_sync_server::AuthenticatedSenderEntry {
                                client_id: ClientId("client-1".to_string()),
                                source: std::net::SocketAddr::from(([127, 0, 0, 1], 5000))
                                    .into(),
                                run_id: RunId("run-1".to_string()),
                                protocol_version: ProtocolVersion(2),
                                registered_at: Some(TimestampMicros(10)),
                            },
                        }
                    ),
                    last_runtime_rejection_summary: Some(
                        stream_sync_server::ServerRuntimePacketOperationalSummary {
                            status: stream_sync_server::ServerOperationalConditionStatus::Reject,
                            message_type: MessageType::Heartbeat,
                            client_id: Some(ClientId("client-1".to_string())),
                            run_id: Some(RunId("run-1".to_string())),
                            reason: stream_sync_server::PacketAcceptanceRejectReason::RunIdMismatch,
                        }
                    ),
                    stop_reason:
                        stream_sync_server::ServerReceiveSendRuntimeBoundedStopReason::ReceiveTimedOut,
                },
            },
        );

        assert!(summary.contains("command_name=--receive-send-runtime-bounded"));
        assert!(summary.contains("config_path=configs/examples/server.example.toml"));
        assert!(summary.contains("max_iterations=16"));
        assert!(summary.contains("receive_timeout_ms=1000"));
        assert!(summary.contains("iterations_attempted=4"));
        assert!(summary.contains("iterations_completed=4"));
        assert!(summary.contains("auth_requests_received=1"));
        assert!(summary.contains("auth_responses_sent=1"));
        assert!(summary.contains("heartbeats_received=1"));
        assert!(summary.contains("heartbeat_acks_sent=1"));
        assert!(summary.contains("client_stats_received=1"));
        assert!(summary.contains("client_stats_returns_sent=1"));
        assert!(summary.contains("accepted_packets=3"));
        assert!(summary.contains("rejected_packets=0"));
        assert!(summary.contains("decode_errors=0"));
        assert!(summary.contains("send_failures=0"));
        assert!(summary.contains("timeout_iterations=1"));
        assert!(summary.contains("timeout_only_run=false"));
        assert!(summary.contains("outbound_queue_len=0"));
        assert!(summary.contains("registered_clients=1"));
        assert!(summary.contains("last_receive_error=TimedOut"));
        assert!(summary.contains("last_send_error=none"));
        assert!(summary.contains("last_rejected_reason=Auth:InvalidToken"));
        assert!(summary.contains("last_auth_status=Reject"));
        assert!(summary.contains("last_auth_reason=InvalidToken"));
        assert!(summary.contains("last_registration_status=Continue"));
        assert!(summary.contains("last_registration_reason=FreshRegistration"));
        assert!(summary.contains("last_runtime_rejection_status=Reject"));
        assert!(summary.contains("last_runtime_rejection_reason=RunIdMismatch"));
        assert!(summary.contains("stop_reason=ReceiveTimedOut"));
    }

    #[test]
    fn receive_send_runtime_continuous_summary_includes_required_fields() {
        let summary = format_receive_send_runtime_continuous_summary(
            "--receive-send-runtime-continuous",
            "configs/examples/server.example.toml",
            &stream_sync_server::ServerReceiveSendContinuousRuntimeOutcome {
                bind_address: "127.0.0.1:5000"
                    .parse()
                    .expect("bind address should parse"),
                receive_buffer: stream_sync_server::ServerUdpReceiveBufferTuningResult {
                    requested_bytes: 8_388_608,
                    effective_bytes: Some(8_388_608),
                    set_error: None,
                    read_error: None,
                },
                registry: stream_sync_server::AuthenticatedSenderRegistry::default(),
                queue_collection: stream_sync_server::ServerOutboundQueueCollection::default(),
                video_queue_state: stream_sync_server::ServerVideoFrameQueueState::default(),
                reassembly_state: stream_sync_server::ServerVideoFrameReassemblyState::default(),
                heartbeat_liveness_state: stream_sync_server::ServerHeartbeatLivenessState::default(),
                heartbeat_rtt_offset_state:
                    stream_sync_server::ServerHeartbeatRttOffsetState::default(),
                iterations: Vec::new(),
                iteration_summaries: Vec::new(),
                summary: stream_sync_server::ServerReceiveSendContinuousRuntimeSummary {
                    receive_timeout: std::time::Duration::from_millis(1_000),
                    max_iterations: Some(16),
                    max_packets_per_drain_cycle: 512,
                    receive_buffer: stream_sync_server::ServerUdpReceiveBufferTuningResult {
                        requested_bytes: 8_388_608,
                        effective_bytes: Some(8_388_608),
                        set_error: None,
                        read_error: None,
                    },
                    drain_cycles: 2,
                    last_packets_drained_in_cycle: 32,
                    max_packets_drained_in_cycle: 64,
                    receive_would_block_count: 1,
                    iterations_attempted: 4,
                    iterations_completed: 4,
                    packets_received: 3,
                    accepted_packets: 2,
                    rejected_packets: 1,
                    decode_errors: 0,
                    auth_requests_received: 1,
                    auth_responses_sent: 1,
                    heartbeats_received: 1,
                    heartbeat_acks_sent: 1,
                    client_stats_received: 1,
                    heartbeat_observations_committed: 1,
                    frames_reassembled: 2,
                    frames_queued: 3,
                    direct_frames_queued: 1,
                    outbound_queue_len: 0,
                    video_queue_len: 3,
                    incomplete_reassembly_frames: 0,
                    registered_clients: 2,
                    heartbeat_liveness_clients: 1,
                    heartbeat_rtt_offset_clients: 1,
                    last_receive_error: Some(std::io::ErrorKind::TimedOut),
                    last_send_error: None,
                    last_rejected_reason: Some("UnauthenticatedSource".to_string()),
                    last_auth_summary: Some(stream_sync_server::ServerAuthOperationalSummary {
                        status: stream_sync_server::ServerOperationalConditionStatus::Continue,
                        client_id: ClientId("client-1".to_string()),
                        run_id: RunId("run-1".to_string()),
                        reason: stream_sync_server::ServerAuthOperationalReason::Accepted,
                    }),
                    last_registration_summary: Some(
                        stream_sync_server::AuthenticatedSenderRegistrationSummary {
                            status: stream_sync_server::ServerOperationalConditionStatus::Continue,
                            client_id: ClientId("client-1".to_string()),
                            previous_source: None,
                            current_source: std::net::SocketAddr::from(([127, 0, 0, 1], 5000))
                                .into(),
                            previous_run_id: None,
                            current_run_id: RunId("run-1".to_string()),
                            reason: stream_sync_server::AuthenticatedSenderRegistrationReason::FreshRegistration,
                            entry: stream_sync_server::AuthenticatedSenderEntry {
                                client_id: ClientId("client-1".to_string()),
                                source: std::net::SocketAddr::from(([127, 0, 0, 1], 5000))
                                    .into(),
                                run_id: RunId("run-1".to_string()),
                                protocol_version: ProtocolVersion(2),
                                registered_at: Some(TimestampMicros(10)),
                            },
                        }
                    ),
                    last_runtime_rejection_summary: Some(
                        stream_sync_server::ServerRuntimePacketOperationalSummary {
                            status: stream_sync_server::ServerOperationalConditionStatus::Reject,
                            message_type: MessageType::Heartbeat,
                            client_id: Some(ClientId("client-1".to_string())),
                            run_id: Some(RunId("run-1".to_string())),
                            reason: stream_sync_server::PacketAcceptanceRejectReason::UnauthenticatedSource,
                        }
                    ),
                    last_heartbeat_timeout_summary: Some(
                        stream_sync_server::ServerHeartbeatTimeoutSweepSummary {
                            status: stream_sync_server::ServerOperationalConditionStatus::ReconnectRequired,
                            clients_evaluated: 2,
                            timed_out_clients: 1,
                            most_severe_client_summary: Some(
                                stream_sync_server::ServerHeartbeatOperationalSummary {
                                    status: stream_sync_server::ServerOperationalConditionStatus::ReconnectRequired,
                                    client_id: ClientId("client-2".to_string()),
                                    reason: stream_sync_server::ServerHeartbeatOperationalReason::TimedOut,
                                    last_server_received_at: Some(TimestampMicros(30)),
                                    elapsed_micros: Some(5_000_000),
                                    timeout_after_micros: Some(5_000_000),
                                }
                            ),
                        }
                    ),
                    stop_reason:
                        stream_sync_server::ServerReceiveSendContinuousRuntimeStopReason::ReceiveTimedOut,
                },
            },
        );

        assert!(summary.contains("command_name=--receive-send-runtime-continuous"));
        assert!(summary.contains("receive_timeout_ms=1000"));
        assert!(summary.contains("max_iterations=16"));
        assert!(summary.contains("receive_buffer_requested_bytes=8388608"));
        assert!(summary.contains("receive_buffer_effective_bytes=8388608"));
        assert!(summary.contains("max_packets_per_drain_cycle=512"));
        assert!(summary.contains("packets_received=3"));
        assert!(summary.contains("frames_reassembled=2"));
        assert!(summary.contains("frames_queued=3"));
        assert!(summary.contains("drain_cycles=2"));
        assert!(summary.contains("last_packets_drained_in_cycle=32"));
        assert!(summary.contains("max_packets_drained_in_cycle=64"));
        assert!(summary.contains("receive_would_block_count=1"));
        assert!(summary.contains("last_auth_status=Continue"));
        assert!(summary.contains("last_registration_reason=FreshRegistration"));
        assert!(summary.contains("last_runtime_rejection_reason=UnauthenticatedSource"));
        assert!(summary.contains("last_heartbeat_timeout_status=ReconnectRequired"));
        assert!(summary.contains("last_heartbeat_timeout_client=client-2"));
        assert!(summary.contains("last_heartbeat_timeout_reason=TimedOut"));
        assert!(summary.contains("stop_reason=ReceiveTimedOut"));
    }

    #[test]
    fn receive_send_runtime_continuous_failure_summary_includes_runtime_visibility() {
        let summary = format_receive_send_runtime_continuous_failure_summary(
            "--receive-send-runtime-continuous",
            &super::ReceiveSendRuntimeContinuousCommandArgs {
                config_path: "configs/examples/server.example.toml".to_string(),
                receive_timeout_ms: 500,
                max_iterations: Some(4),
                heartbeat_timeout_micros: 5_000_000,
                receive_buffer_bytes: 4_194_304,
                max_packets_per_drain_cycle: 1_024,
            },
            &stream_sync_server::ServerReceiveSendContinuousRuntimeError::Runtime {
                iteration_index: 2,
                error: stream_sync_server::ServerControllerReceiveSendRuntimeError::Iteration(
                    stream_sync_server::ServerReceiveSendOneIterationRuntimeError::Send(
                        stream_sync_server::ServerOutboundSendOneRuntimeError::SocketSend {
                            error_kind: std::io::ErrorKind::ConnectionRefused,
                            event: stream_sync_net_core::OutboundSendLoopEvent {
                                state: stream_sync_net_core::OutboundSendLoopTickState::Failed,
                                log_event: None,
                            },
                        },
                    ),
                ),
                summary: stream_sync_server::ServerReceiveSendContinuousRuntimeSummary {
                    receive_timeout: std::time::Duration::from_millis(500),
                    max_iterations: Some(4),
                    max_packets_per_drain_cycle: 1_024,
                    receive_buffer: stream_sync_server::ServerUdpReceiveBufferTuningResult {
                        requested_bytes: 4_194_304,
                        effective_bytes: Some(4_194_304),
                        set_error: None,
                        read_error: None,
                    },
                    drain_cycles: 1,
                    last_packets_drained_in_cycle: 2,
                    max_packets_drained_in_cycle: 2,
                    receive_would_block_count: 0,
                    iterations_attempted: 3,
                    iterations_completed: 2,
                    packets_received: 2,
                    accepted_packets: 2,
                    rejected_packets: 0,
                    decode_errors: 0,
                    auth_requests_received: 1,
                    auth_responses_sent: 1,
                    heartbeats_received: 1,
                    heartbeat_acks_sent: 1,
                    client_stats_received: 0,
                    heartbeat_observations_committed: 0,
                    frames_reassembled: 0,
                    frames_queued: 0,
                    direct_frames_queued: 0,
                    outbound_queue_len: 0,
                    video_queue_len: 0,
                    incomplete_reassembly_frames: 0,
                    registered_clients: 1,
                    heartbeat_liveness_clients: 1,
                    heartbeat_rtt_offset_clients: 0,
                    last_receive_error: None,
                    last_send_error: Some("SocketSend(ConnectionRefused)".to_string()),
                    last_rejected_reason: None,
                    last_auth_summary: None,
                    last_registration_summary: None,
                    last_runtime_rejection_summary: None,
                    last_heartbeat_timeout_summary: None,
                    stop_reason:
                        stream_sync_server::ServerReceiveSendContinuousRuntimeStopReason::MaxIterationsReached,
                },
            },
        );

        assert!(summary.contains("command_name=--receive-send-runtime-continuous"));
        assert!(summary.contains("receive_timeout_ms=500"));
        assert!(summary.contains("max_iterations=4"));
        assert!(summary.contains("receive_buffer_requested_bytes=4194304"));
        assert!(summary.contains("receive_buffer_effective_bytes=4194304"));
        assert!(summary.contains("max_packets_per_drain_cycle=1024"));
        assert!(summary.contains("frames_reassembled=0"));
        assert!(summary.contains("last_send_error=SocketSend(ConnectionRefused)"));
        assert!(summary.contains("stop_reason=RuntimeFatalError"));
        assert!(summary.contains("fatal_error_kind=SendFailure"));
        assert!(summary.contains("fatal_error_detail=iteration_index=2"));
    }

    #[test]
    fn receive_send_runtime_bounded_failure_summary_includes_startup_failure_fields() {
        let summary = format_receive_send_runtime_bounded_failure_summary(
            "--receive-send-runtime-bounded",
            &super::ReceiveSendRuntimeBoundedCommandArgs {
                config_path: "configs/examples/server.example.toml".to_string(),
                max_iterations: 8,
                receive_timeout_ms: 500,
            },
            &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::OneIterationStartup(
                stream_sync_server::ServerReceiveSendOneIterationStartupError::StartupConfig(
                    stream_sync_server::ServerAuthResponsePocStartupError::Config(
                        stream_sync_server::ServerAuthResponsePocConfigError::MissingField {
                            section: "server",
                            key: "bind_port",
                        },
                    ),
                ),
            ),
        );

        assert!(summary.contains("command_name=--receive-send-runtime-bounded"));
        assert!(summary.contains("config_path=configs/examples/server.example.toml"));
        assert!(summary.contains("max_iterations=8"));
        assert!(summary.contains("receive_timeout_ms=500"));
        assert!(summary.contains("stop_reason=StartupFailure"));
        assert!(summary.contains("fatal_error_kind=ConfigLoadFailure"));
        assert!(summary.contains("fatal_error_detail="));
    }

    #[test]
    fn receive_send_runtime_bounded_failure_summary_includes_send_failure_visibility() {
        let summary = format_receive_send_runtime_bounded_failure_summary(
            "--receive-send-runtime-bounded",
            &super::ReceiveSendRuntimeBoundedCommandArgs {
                config_path: "configs/examples/server.example.toml".to_string(),
                max_iterations: 6,
                receive_timeout_ms: 1000,
            },
            &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::Runtime {
                iteration_index: 2,
                error: stream_sync_server::ServerControllerReceiveSendRuntimeError::Iteration(
                        stream_sync_server::ServerReceiveSendOneIterationRuntimeError::Send(
                            stream_sync_server::ServerOutboundSendOneRuntimeError::SocketSend {
                                error_kind: std::io::ErrorKind::PermissionDenied,
                                event: stream_sync_net_core::OutboundSendLoopEvent {
                                    state: stream_sync_net_core::OutboundSendLoopTickState::Failed,
                                    log_event: None,
                                },
                            },
                        ),
                ),
                summary: stream_sync_server::ServerReceiveSendRuntimeBoundedSummary {
                    max_iterations: 6,
                    receive_timeout: std::time::Duration::from_millis(1_000),
                    iterations_attempted: 3,
                    iterations_completed: 2,
                    auth_requests_received: 1,
                    auth_responses_sent: 1,
                    heartbeats_received: 1,
                    heartbeat_acks_sent: 0,
                    client_stats_received: 0,
                    client_stats_returns_sent: 0,
                    accepted_packets: 2,
                    rejected_packets: 0,
                    decode_errors: 0,
                    send_failures: 1,
                    timeout_iterations: 0,
                    timeout_only_run: false,
                    outbound_queue_len: 0,
                    registered_clients: 1,
                    last_receive_error: None,
                    last_send_error: Some("SocketSend(PermissionDenied)".to_string()),
                    last_rejected_reason: None,
                    last_auth_summary: None,
                    last_registration_summary: None,
                    last_runtime_rejection_summary: None,
                    stop_reason:
                        stream_sync_server::ServerReceiveSendRuntimeBoundedStopReason::MaxIterationsReached,
                },
            },
        );

        assert!(summary.contains("iterations_attempted=3"));
        assert!(summary.contains("iterations_completed=2"));
        assert!(summary.contains("send_failures=1"));
        assert!(summary.contains("last_send_error=SocketSend(PermissionDenied)"));
        assert!(summary.contains("stop_reason=RuntimeFatalError"));
        assert!(summary.contains("fatal_error_kind=SendFailure"));
        assert!(summary.contains("fatal_error_detail=iteration_index=2"));
    }

    #[test]
    fn receive_send_runtime_bounded_failure_summary_includes_deferred_iteration_sink_visibility() {
        let summary = format_receive_send_runtime_bounded_failure_summary(
            "--receive-send-runtime-bounded",
            &super::ReceiveSendRuntimeBoundedCommandArgs {
                config_path: "configs/examples/server.example.toml".to_string(),
                max_iterations: 6,
                receive_timeout_ms: 1000,
            },
            &stream_sync_server::ServerReceiveSendRuntimeBoundedStartupError::IterationEventSinkSelection(
                stream_sync_server::ServerReceiveSendIterationJsonLinesSinkSelectionError::FileDestinationDeferred {
                    path: "logs/receive-send-iteration.jsonl".into(),
                },
            ),
        );

        assert!(summary.contains("stop_reason=StartupFailure"));
        assert!(summary.contains("fatal_error_kind=IterationEventSinkDeferred"));
        assert!(summary.contains(
            "fatal_error_detail=file_destination_deferred path=logs/receive-send-iteration.jsonl"
        ));
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
        assert!(summary.contains("auth_status=Continue"));
        assert!(summary.contains("auth_operational_reason=Accepted"));
        assert!(summary.contains("registration_status=none"));
        assert!(summary.contains("registration_reason=none"));
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
                        operational_summary: stream_sync_server::ServerAuthOperationalSummary {
                            status: stream_sync_server::ServerOperationalConditionStatus::Continue,
                            client_id: client_id.clone(),
                            run_id: run_id.clone(),
                            reason: stream_sync_server::ServerAuthOperationalReason::Accepted,
                        },
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
                    registration_summary: None,
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
