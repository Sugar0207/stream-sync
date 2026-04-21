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
        _ => {
            println!(
                "stream-sync-server scaffold; use --auth-response-poc-once [config-path] or --receive-send-once [config-path]"
            );
        }
    }
}

fn current_timestamp_micros() -> stream_sync_protocol::TimestampMicros {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or(0);
    stream_sync_protocol::TimestampMicros(u64::try_from(micros).unwrap_or(u64::MAX))
}
