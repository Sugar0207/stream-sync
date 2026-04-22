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
        Some("--auth-heartbeat-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            match stream_sync_client::run_auth_heartbeat_poc_once_from_path(&config_path) {
                Ok(outcome) => {
                    println!(
                        "auth heartbeat PoC sent AuthRequest {} bytes to {} and received AuthResponse {} bytes from {}; accepted={} reason_code={:?}; sent Heartbeat {} bytes and received HeartbeatAck {} bytes from {}; client_id={} run_id={} protocol_version={} heartbeat_sent_at={} echoed_sent_at={} server_received_at={} server_sent_at={}",
                        outcome.auth_request_bytes_sent,
                        outcome.destination,
                        outcome.auth_response_bytes.len(),
                        outcome.auth_response_source,
                        outcome.auth_response.accepted,
                        outcome.auth_response.reason_code,
                        outcome.heartbeat_bytes_sent,
                        outcome.heartbeat_ack_bytes.len(),
                        outcome.heartbeat_ack_source,
                        outcome.request.client_id.0,
                        outcome.request.run_id.0,
                        outcome.request.protocol_version.0,
                        outcome.heartbeat.sent_at.0,
                        outcome.heartbeat_ack.echoed_sent_at.0,
                        outcome.heartbeat_ack.server_received_at.0,
                        outcome.heartbeat_ack.server_sent_at.0
                    );
                }
                Err(error) => {
                    eprintln!("auth heartbeat PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--auth-heartbeat-stats-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            match stream_sync_client::run_auth_heartbeat_stats_poc_once_from_path(&config_path) {
                Ok(outcome) => {
                    let heartbeat = &outcome.heartbeat;
                    println!(
                        "auth heartbeat stats PoC sent AuthRequest {} bytes to {} and received AuthResponse {} bytes from {}; accepted={} reason_code={:?}; sent Heartbeat {} bytes and received HeartbeatAck {} bytes from {}; sent ClientStats {} bytes with HeartbeatAckObservation; client_id={} run_id={} protocol_version={} heartbeat_sent_at={} echoed_sent_at={} server_received_at={} server_sent_at={} client_received_at={}",
                        heartbeat.auth_request_bytes_sent,
                        heartbeat.destination,
                        heartbeat.auth_response_bytes.len(),
                        heartbeat.auth_response_source,
                        heartbeat.auth_response.accepted,
                        heartbeat.auth_response.reason_code,
                        heartbeat.heartbeat_bytes_sent,
                        heartbeat.heartbeat_ack_bytes.len(),
                        heartbeat.heartbeat_ack_source,
                        outcome.client_stats_bytes_sent,
                        heartbeat.request.client_id.0,
                        heartbeat.request.run_id.0,
                        heartbeat.request.protocol_version.0,
                        heartbeat.heartbeat.sent_at.0,
                        outcome.heartbeat_ack_observation.echoed_sent_at.0,
                        outcome.heartbeat_ack_observation.server_received_at.0,
                        outcome.heartbeat_ack_observation.server_sent_at.0,
                        outcome.heartbeat_ack_observation.client_received_at.0
                    );
                }
                Err(error) => {
                    eprintln!("auth heartbeat stats PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!(
                "stream-sync-client scaffold; use --auth-request-poc-once [config-path], --auth-heartbeat-poc-once [config-path], or --auth-heartbeat-stats-poc-once [config-path]"
            );
        }
    }
}
