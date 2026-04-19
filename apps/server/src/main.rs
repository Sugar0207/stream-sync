fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--auth-response-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            match stream_sync_server::run_auth_response_poc_once_from_path(&config_path) {
                Ok(outcome) => {
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
        _ => {
            println!(
                "stream-sync-server scaffold; use --auth-response-poc-once [config-path] for one-shot auth response PoC"
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
