fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--auth-request-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.example.toml".to_string());
            match stream_sync_client::run_auth_request_poc_once_from_path(&config_path) {
                Ok(outcome) => {
                    let response_message = outcome.response.message.as_deref().unwrap_or("null");
                    let expected_protocol_version = outcome
                        .response
                        .expected_protocol_version
                        .map(|version| version.0.to_string())
                        .unwrap_or_else(|| "null".to_string());
                    println!(
                        "auth request PoC sent {} bytes to {} and received AuthResponse {} bytes from {}; client_id={} run_id={} protocol_version={} accepted={} reason_code={:?} message={} expected_protocol_version={}",
                        outcome.bytes_sent,
                        outcome.destination,
                        outcome.response_bytes.len(),
                        outcome.response_source,
                        outcome.request.client_id.0,
                        outcome.request.run_id.0,
                        outcome.request.protocol_version.0,
                        outcome.response.accepted,
                        outcome.response.reason_code,
                        response_message,
                        expected_protocol_version
                    );
                }
                Err(error) => {
                    eprintln!("auth request PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!(
                "stream-sync-client scaffold; use --auth-request-poc-once [config-path] for one-shot auth request PoC"
            );
        }
    }
}
