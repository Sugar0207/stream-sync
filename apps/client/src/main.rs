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
        Some("--placeholder-video-frame-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            match stream_sync_client::run_placeholder_video_frame_poc_once_from_path(&config_path) {
                Ok(outcome) => {
                    println!(
                        "placeholder video frame PoC sent {} bytes to {}; client_id={} run_id={} protocol_version={} frame_id={} capture_timestamp={} send_timestamp={} width={} height={} fps_nominal={} payload_len={} placeholder_payload=true",
                        outcome.bytes_sent,
                        outcome.destination,
                        outcome.frame.client_id.0,
                        outcome.frame.run_id.0,
                        outcome.frame.protocol_version.0,
                        outcome.frame.frame_id,
                        outcome.frame.capture_timestamp.0,
                        outcome.frame.send_timestamp.0,
                        outcome.frame.width,
                        outcome.frame.height,
                        outcome.frame.fps_nominal,
                        outcome.frame.payload_size
                    );
                }
                Err(error) => {
                    eprintln!("placeholder video frame PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--auth-placeholder-video-frame-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            match stream_sync_client::run_auth_placeholder_video_frame_poc_once_from_path(
                &config_path,
            ) {
                Ok(outcome) => {
                    println!(
                        "auth placeholder video frame PoC sent AuthRequest {} bytes from {} to {} and received AuthResponse {} bytes from {}; accepted={} reason_code={:?}; sent VideoFrame {} bytes from same_source=true; client_id={} run_id={} protocol_version={} frame_id={} capture_timestamp={} send_timestamp={} width={} height={} fps_nominal={} payload_len={} placeholder_payload=true",
                        outcome.auth_request_bytes_sent,
                        outcome.local_source,
                        outcome.destination,
                        outcome.auth_response_bytes.len(),
                        outcome.auth_response_source,
                        outcome.auth_response.accepted,
                        outcome.auth_response.reason_code,
                        outcome.video_frame_bytes_sent,
                        outcome.frame.client_id.0,
                        outcome.frame.run_id.0,
                        outcome.frame.protocol_version.0,
                        outcome.frame.frame_id,
                        outcome.frame.capture_timestamp.0,
                        outcome.frame.send_timestamp.0,
                        outcome.frame.width,
                        outcome.frame.height,
                        outcome.frame.fps_nominal,
                        outcome.frame.payload_size
                    );
                }
                Err(error) => {
                    eprintln!("auth placeholder video frame PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--real-encoded-video-frame-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            match stream_sync_client::run_real_encoded_video_frame_poc_once_from_path(&config_path)
            {
                Ok(stream_sync_client::ClientRealEncodedVideoFramePocOutcome::Sent(sent)) => {
                    println!(
                        "real encoded video frame PoC sent {} bytes to {}; frame_id={} capture_timestamp={} width={} height={} fps_nominal={} payload_len={} source_kind={:?}",
                        sent.bytes_sent,
                        sent.destination,
                        sent.frame.frame_id,
                        sent.frame.capture_timestamp.0,
                        sent.frame.width,
                        sent.frame.height,
                        sent.frame.fps_nominal,
                        sent.frame.payload_size,
                        sent.source_kind
                    );
                }
                Ok(
                    stream_sync_client::ClientRealEncodedVideoFramePocOutcome::SessionConfigNotPrepared {
                        destination,
                        backend,
                        reason,
                    },
                ) => {
                    eprintln!(
                        "real encoded video frame PoC did not send to {destination}: capture session config not prepared backend={backend:?} reason={reason:?}"
                    );
                    std::process::exit(1);
                }
                Ok(
                    stream_sync_client::ClientRealEncodedVideoFramePocOutcome::SessionNotCreated {
                        destination,
                        backend,
                        reason,
                        message,
                    },
                ) => {
                    eprintln!(
                        "real encoded video frame PoC did not send to {destination}: capture session not created backend={backend:?} reason={reason:?} message={}",
                        message.as_deref().unwrap_or("none")
                    );
                    std::process::exit(1);
                }
                Ok(stream_sync_client::ClientRealEncodedVideoFramePocOutcome::NotSent {
                    destination,
                    result,
                }) => {
                    eprintln!(
                        "real encoded video frame PoC did not send to {destination}: result={result:?}"
                    );
                    std::process::exit(1);
                }
                Err(error) => {
                    eprintln!("real encoded video frame PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--auth-real-encoded-video-frame-poc-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            match stream_sync_client::run_auth_real_encoded_video_frame_poc_once_from_path(
                &config_path,
            ) {
                Ok(outcome) => match outcome.video {
                    stream_sync_client::ClientRealEncodedVideoFramePocOutcome::Sent(sent) => {
                        println!(
                            "auth real encoded video frame PoC sent AuthRequest {} bytes from {} to {} and received AuthResponse {} bytes from {}; accepted={} reason_code={:?}; sent VideoFrame {} bytes from same_source=true; frame_id={} capture_timestamp={} width={} height={} fps_nominal={} payload_len={} source_kind={:?}",
                            outcome.auth_request_bytes_sent,
                            outcome.local_source,
                            outcome.destination,
                            outcome.auth_response_bytes.len(),
                            outcome.auth_response_source,
                            outcome.auth_response.accepted,
                            outcome.auth_response.reason_code,
                            sent.bytes_sent,
                            sent.frame.frame_id,
                            sent.frame.capture_timestamp.0,
                            sent.frame.width,
                            sent.frame.height,
                            sent.frame.fps_nominal,
                            sent.frame.payload_size,
                            sent.source_kind
                        );
                    }
                    stream_sync_client::ClientRealEncodedVideoFramePocOutcome::SessionConfigNotPrepared {
                        destination,
                        backend,
                        reason,
                    } => {
                        eprintln!(
                            "auth real encoded video frame PoC did not send to {destination}: capture session config not prepared backend={backend:?} reason={reason:?}"
                        );
                        std::process::exit(1);
                    }
                    stream_sync_client::ClientRealEncodedVideoFramePocOutcome::SessionNotCreated {
                        destination,
                        backend,
                        reason,
                        message,
                    } => {
                        eprintln!(
                            "auth real encoded video frame PoC did not send to {destination}: capture session not created backend={backend:?} reason={reason:?} message={}",
                            message.as_deref().unwrap_or("none")
                        );
                        std::process::exit(1);
                    }
                    stream_sync_client::ClientRealEncodedVideoFramePocOutcome::NotSent {
                        destination,
                        result,
                    } => {
                        eprintln!(
                            "auth real encoded video frame PoC did not send to {destination}: result={result:?}"
                        );
                        std::process::exit(1);
                    }
                },
                Err(error) => {
                    eprintln!("auth real encoded video frame PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--auth-real-encoded-video-frame-poc-bounded") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            let max_frames = args
                .next()
                .map(|value| value.parse::<u64>())
                .transpose()
                .unwrap_or_else(|error| {
                    eprintln!(
                        "invalid max-frames for bounded auth real encoded video PoC: {error}"
                    );
                    std::process::exit(1);
                })
                .unwrap_or(5);
            match stream_sync_client::run_auth_real_encoded_video_frame_poc_bounded_from_path(
                &config_path,
                max_frames,
            ) {
                Ok(outcome) => match outcome.video {
                    stream_sync_client::ClientContinuousRealEncodedVideoFramePocOutcome::Completed(runtime) => {
                        let summary = runtime.summary;
                        let last_send_failure = summary.last_send_failure.as_ref();
                        let last_send_destination = last_send_failure
                            .map(|failure| failure.destination.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        let last_send_local_source = last_send_failure
                            .and_then(|failure| failure.local_source)
                            .map(|source| source.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        let last_send_frame_id = last_send_failure
                            .map(|failure| failure.frame_id.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        let last_send_payload_len = last_send_failure
                            .map(|failure| failure.payload_len.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        let last_send_packet_len = last_send_failure
                            .and_then(|failure| failure.encoded_packet_len)
                            .map(|len| len.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        let last_send_error = last_send_failure
                            .map(|failure| format!("{:?}", failure.error))
                            .unwrap_or_else(|| "none".to_string());
                        println!(
                            "auth real encoded video frame bounded PoC sent AuthRequest {} bytes from {} to {} and received AuthResponse {} bytes from {}; accepted={} reason_code={:?}; bounded_manual_runtime=true; frames_attempted={} frames_captured={} frames_encoded={} frames_sent={} no_frame_count={} capture_failures={} encode_failures={} frame_build_failures={} send_failures={} stop_reason={:?} last_send_destination={} last_send_local_source={} last_send_frame_id={} last_send_payload_len={} last_send_packet_len={} last_send_error={}",
                            outcome.auth_request_bytes_sent,
                            outcome.local_source,
                            outcome.destination,
                            outcome.auth_response_bytes.len(),
                            outcome.auth_response_source,
                            outcome.auth_response.accepted,
                            outcome.auth_response.reason_code,
                            summary.frames_attempted,
                            summary.frames_captured,
                            summary.frames_encoded,
                            summary.frames_sent,
                            summary.no_frame_count,
                            summary.capture_failures,
                            summary.encode_failures,
                            summary.frame_build_failures,
                            summary.send_failures,
                            summary.stop_reason,
                            last_send_destination,
                            last_send_local_source,
                            last_send_frame_id,
                            last_send_payload_len,
                            last_send_packet_len,
                            last_send_error
                        );
                    }
                    stream_sync_client::ClientContinuousRealEncodedVideoFramePocOutcome::SessionConfigNotPrepared {
                        destination,
                        backend,
                        reason,
                    } => {
                        eprintln!(
                            "auth real encoded video frame bounded PoC did not send to {destination}: capture session config not prepared backend={backend:?} reason={reason:?}"
                        );
                        std::process::exit(1);
                    }
                    stream_sync_client::ClientContinuousRealEncodedVideoFramePocOutcome::SessionNotCreated {
                        destination,
                        backend,
                        reason,
                        message,
                    } => {
                        eprintln!(
                            "auth real encoded video frame bounded PoC did not send to {destination}: capture session not created backend={backend:?} reason={reason:?} message={}",
                            message.as_deref().unwrap_or("none")
                        );
                        std::process::exit(1);
                    }
                },
                Err(error) => {
                    eprintln!("auth real encoded video frame bounded PoC failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--auth-heartbeat-one-tick-runtime") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            match stream_sync_client::run_auth_heartbeat_one_tick_runtime_from_path(&config_path) {
                Ok(outcome) => {
                    let heartbeat_send = outcome
                        .runtime
                        .heartbeat_send
                        .as_ref()
                        .expect("one-tick runtime should send one heartbeat");
                    let ack_return = outcome
                        .runtime
                        .ack_return
                        .as_ref()
                        .expect("one-tick runtime should receive one ack");
                    println!(
                        "auth heartbeat one-tick runtime sent AuthRequest {} bytes to {} and received AuthResponse {} bytes from {}; accepted={} reason_code={:?}; controller_action={:?} shutdown={:?}; sent Heartbeat {} bytes and received HeartbeatAck {} bytes from {}; client_id={} run_id={} protocol_version={} heartbeat_sent_at={} echoed_sent_at={} server_received_at={} server_sent_at={} sent_heartbeats={} received_acks={} missed_acks={} stats_returns_sent={}",
                        outcome.auth_request_bytes_sent,
                        outcome.destination,
                        outcome.auth_response_bytes.len(),
                        outcome.auth_response_source,
                        outcome.auth_response.accepted,
                        outcome.auth_response.reason_code,
                        outcome.runtime.controller.action,
                        outcome.runtime.controller.shutdown,
                        heartbeat_send.bytes_sent,
                        ack_return.ack_bytes.len(),
                        ack_return.ack_source,
                        outcome.request.client_id.0,
                        outcome.request.run_id.0,
                        outcome.request.protocol_version.0,
                        heartbeat_send.handoff.heartbeat.sent_at.0,
                        ack_return.ack.echoed_sent_at.0,
                        ack_return.ack.server_received_at.0,
                        ack_return.ack.server_sent_at.0,
                        outcome.runtime.final_counters.sent_heartbeats,
                        outcome.runtime.final_counters.received_acks,
                        outcome.runtime.final_counters.missed_acks,
                        outcome.runtime.final_counters.stats_returns_sent
                    );
                }
                Err(error) => {
                    eprintln!("auth heartbeat one-tick runtime failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        Some("--auth-heartbeat-stats-one-tick-runtime") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/client.accepted.example.toml".to_string());
            match stream_sync_client::run_auth_heartbeat_stats_one_tick_runtime_from_path(
                &config_path,
            ) {
                Ok(outcome) => {
                    let heartbeat_send = outcome
                        .runtime
                        .heartbeat_send
                        .as_ref()
                        .expect("one-tick runtime should send one heartbeat");
                    let ack_return = outcome
                        .runtime
                        .ack_return
                        .as_ref()
                        .expect("one-tick runtime should receive one ack");
                    let stats_return = outcome
                        .runtime
                        .stats_return_send
                        .as_ref()
                        .expect("stats mode should send one client stats payload");
                    let observation = &ack_return.observation;
                    println!(
                        "auth heartbeat stats one-tick runtime sent AuthRequest {} bytes to {} and received AuthResponse {} bytes from {}; accepted={} reason_code={:?}; controller_action={:?} shutdown={:?}; sent Heartbeat {} bytes and received HeartbeatAck {} bytes from {}; sent ClientStats {} bytes with HeartbeatAckObservation; client_id={} run_id={} protocol_version={} heartbeat_sent_at={} echoed_sent_at={} server_received_at={} server_sent_at={} client_received_at={} sent_heartbeats={} received_acks={} missed_acks={} stats_returns_sent={}",
                        outcome.auth_request_bytes_sent,
                        outcome.destination,
                        outcome.auth_response_bytes.len(),
                        outcome.auth_response_source,
                        outcome.auth_response.accepted,
                        outcome.auth_response.reason_code,
                        outcome.runtime.controller.action,
                        outcome.runtime.controller.shutdown,
                        heartbeat_send.bytes_sent,
                        ack_return.ack_bytes.len(),
                        ack_return.ack_source,
                        stats_return.bytes_sent,
                        outcome.request.client_id.0,
                        outcome.request.run_id.0,
                        outcome.request.protocol_version.0,
                        heartbeat_send.handoff.heartbeat.sent_at.0,
                        observation.echoed_sent_at.0,
                        observation.server_received_at.0,
                        observation.server_sent_at.0,
                        observation.client_received_at.0,
                        outcome.runtime.final_counters.sent_heartbeats,
                        outcome.runtime.final_counters.received_acks,
                        outcome.runtime.final_counters.missed_acks,
                        outcome.runtime.final_counters.stats_returns_sent
                    );
                }
                Err(error) => {
                    eprintln!("auth heartbeat stats one-tick runtime failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!(
                "stream-sync-client scaffold; use --auth-request-poc-once [config-path], --auth-heartbeat-poc-once [config-path], --auth-heartbeat-stats-poc-once [config-path], --placeholder-video-frame-poc-once [config-path], --auth-placeholder-video-frame-poc-once [config-path], --real-encoded-video-frame-poc-once [config-path], --auth-real-encoded-video-frame-poc-once [config-path], --auth-real-encoded-video-frame-poc-bounded [config-path] [max-frames], --auth-heartbeat-one-tick-runtime [config-path], or --auth-heartbeat-stats-one-tick-runtime [config-path]"
            );
        }
    }
}
