fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--auth-request-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.example.toml".to_string());
            match stream_sync_client::run_auth_request_poc_once_from_path(&config_path) {
                Ok(outcome) => {
                    println!(
                        "auth request PoC sent {} bytes to {}",
                        outcome.bytes_sent, outcome.destination
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
