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
                        sent.send.bytes_sent,
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
                            sent.send.bytes_sent,
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
            let fragment_pacing_every =
                parse_optional_arg_or_exit::<u32>(args.next(), "fragment-pacing-every")
                    .unwrap_or(16);
            let fragment_pacing_delay_ms =
                parse_optional_arg_or_exit::<u64>(args.next(), "fragment-pacing-delay-ms")
                    .unwrap_or(1);
            let mut encoder_runtime =
                stream_sync_client::ClientRealEncodedVideoFrameEncoderRuntime::PerFrame;
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--encoder-runtime" => {
                        let Some(value) = args.next() else {
                            eprintln!(
                                "missing encoder runtime value for bounded auth real encoded video PoC"
                            );
                            std::process::exit(1);
                        };
                        encoder_runtime = stream_sync_client::ClientRealEncodedVideoFrameEncoderRuntime::parse_config_str(
                            &value,
                        )
                        .unwrap_or_else(|| {
                            eprintln!(
                                "invalid encoder runtime for bounded auth real encoded video PoC: {value}"
                            );
                            std::process::exit(1);
                        });
                    }
                    other => {
                        eprintln!(
                            "unexpected argument for bounded auth real encoded video PoC: {other}"
                        );
                        std::process::exit(1);
                    }
                }
            }
            let fragment_pacing = stream_sync_client::ClientVideoFrameFragmentPacingPolicy {
                delay_every_fragments: fragment_pacing_every,
                delay_micros: fragment_pacing_delay_ms.saturating_mul(1_000),
            };
            let launcher =
                stream_sync_client::ClientAuthRealEncodedVideoFrameBoundedPocLauncher::default();
            let mut startup_config = launcher
                .load_startup_config_from_path(&config_path, max_frames)
                .unwrap_or_else(|error| {
                    eprintln!("auth real encoded video frame bounded PoC failed: {error:?}");
                    std::process::exit(1);
                });
            startup_config.policy.fragment_pacing = fragment_pacing;
            startup_config.encoder_runtime = encoder_runtime;
            let encoder_config = startup_config.video.encoder_config.clone();
            let ffmpeg_preflight =
                stream_sync_client::probe_client_ffmpeg_preflight(&encoder_config);
            let encoder_runtime =
                stream_sync_client::ClientObservedFfmpegSoftwareH264EncoderRuntimeHook::from_video_encoder_config(
                    encoder_config.clone(),
                );
            let outcome = {
                #[cfg(target_os = "windows")]
                {
                    launcher.run_once_with_runtime_selection(
                        startup_config,
                        &stream_sync_client::ClientWindowsGraphicsCaptureSessionRuntimeHook,
                        &stream_sync_client::ClientWindowsGraphicsCaptureFrameAcquisitionRuntimeHook,
                        &encoder_runtime,
                    )
                }

                #[cfg(not(target_os = "windows"))]
                {
                    launcher.run_once_with_runtime_selection(
                        startup_config,
                        &stream_sync_client::ClientUnavailableCaptureSessionRuntimeHook,
                        &stream_sync_client::ClientUnavailableCaptureFrameAcquisitionRuntimeHook,
                        &encoder_runtime,
                    )
                }
            };
            let ffmpeg_visibility = encoder_runtime.snapshot();
            match outcome {
                Ok(outcome) => match outcome.video {
                    stream_sync_client::ClientContinuousRealEncodedVideoFramePocOutcome::Completed(runtime) => {
                        let summary = runtime.summary;
                        let last_send_failure = summary.last_send_failure.as_ref();
                        let last_encode_error =
                            last_encode_error_from_results(&runtime.results);
                        let last_payload_len = last_payload_len_from_results(&runtime.results);
                        let oversized_payload_count =
                            oversized_payload_count_from_results(&runtime.results);
                        let fragmentation_pressure_count =
                            fragmentation_pressure_count_from_results(&runtime.results);
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
                        let encoder_width = encoder_config
                            .width
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "input".to_string());
                        let encoder_height = encoder_config
                            .height
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "input".to_string());
                        let encoder_bitrate_kbps = encoder_config
                            .bitrate_kbps
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        let encoder_gop_frames = encoder_config
                            .gop_frames
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        let encoder_profile = encoder_config
                            .profile
                            .clone()
                            .unwrap_or_else(|| "none".to_string());
                        let encoder_level = encoder_config
                            .level
                            .clone()
                            .unwrap_or_else(|| "none".to_string());
                        let elapsed_ms = format_duration_ms(summary.elapsed_micros);
                        let capture_elapsed_ms =
                            format_duration_ms(summary.capture_elapsed_micros);
                        let encode_elapsed_ms =
                            format_duration_ms(summary.encode_elapsed_micros);
                        let avg_capture_elapsed_ms = format_average_duration_ms(
                            summary.capture_elapsed_micros,
                            summary.frames_captured + summary.capture_failures,
                        );
                        let avg_encode_elapsed_ms = format_average_duration_ms(
                            summary.encode_elapsed_micros,
                            summary.frames_encoded + summary.encode_failures,
                        );
                        let capture_wait_or_no_frame_elapsed_ms =
                            format_duration_ms(summary.capture_wait_or_no_frame_elapsed_micros);
                        let send_elapsed_ms = format_duration_ms(summary.send_elapsed_micros);
                        let configured_frame_interval_ms =
                            format_duration_ms(summary.configured_frame_interval_micros);
                        let loop_interval_sleep_ms =
                            format_duration_ms(summary.loop_interval_sleep_micros);
                        let total_fragment_pacing_sleep_ms =
                            format_duration_ms(summary.total_fragment_pacing_sleep_micros);
                        let effective_output_fps =
                            format_fps(summary.frames_sent, summary.elapsed_micros);
                        let effective_fresh_capture_fps =
                            format_fps(summary.frames_captured, summary.elapsed_micros);
                        let effective_send_fps =
                            format_fps(summary.frames_sent, summary.send_elapsed_micros);
                        let last_encoder_exit_status = summary
                            .last_encoder_exit_status
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string());
                        println!(
                            "auth real encoded video frame bounded PoC sent AuthRequest {} bytes from {} to {} and received AuthResponse {} bytes from {}; accepted={} reason_code={:?}; bounded_manual_runtime=true; fragment_pacing_every={} fragment_pacing_delay_ms={} encoder_backend={} encoder_width={} encoder_height={} encoder_fps={} encoder_bitrate_kbps={} encoder_gop_frames={} encoder_preset={} encoder_tune={} encoder_pixel_format={} encoder_profile={} encoder_level={} ffmpeg_path={} ffmpeg_version_detected={} ffmpeg_preflight_error={} ffmpeg_spawn_error={} configured_max_frames={} configured_max_ticks={} configured_frame_interval_ms={} encoder_runtime={} encoder_process_start_count={} runtime_ticks={} capture_attempts={} frames_captured={} frames_encoded={} frames_sent={} direct_sends={} fragmented_sends={} fragments_attempted={} fragments_sent={} no_frame_count={} capture_failures={} encode_failures={} frame_build_failures={} send_failures={} persistent_access_units_emitted={} persistent_no_complete_access_unit_count={} persistent_stdout_closed_count={} persistent_malformed_stream_count={} last_encoder_exit_status={} frames_remaining_to_max={} elapsed_ms={} capture_elapsed_ms={} encode_elapsed_ms={} avg_capture_elapsed_ms={} avg_encode_elapsed_ms={} capture_wait_or_no_frame_elapsed_ms={} effective_output_fps={} effective_fresh_capture_fps={} effective_send_fps={} loop_interval_sleep_ms={} total_fragment_pacing_sleep_ms={} send_elapsed_ms={} ticks_elapsed_while_sending={} last_encode_error={} last_ffmpeg_error={} last_payload_len={} oversized_payload_count={} fragmentation_pressure_count={} stop_reason={:?} last_send_destination={} last_send_local_source={} last_send_frame_id={} last_send_payload_len={} last_send_packet_len={} last_send_error={}",
                            outcome.auth_request_bytes_sent,
                            outcome.local_source,
                            outcome.destination,
                            outcome.auth_response_bytes.len(),
                            outcome.auth_response_source,
                            outcome.auth_response.accepted,
                            outcome.auth_response.reason_code,
                            fragment_pacing_every,
                            fragment_pacing_delay_ms,
                            encoder_config.backend.as_config_str(),
                            encoder_width,
                            encoder_height,
                            encoder_config.fps,
                            encoder_bitrate_kbps,
                            encoder_gop_frames,
                            encoder_config.preset,
                            encoder_config.tune,
                            encoder_config.pixel_format,
                            encoder_profile,
                            encoder_level,
                            ffmpeg_preflight.ffmpeg_path.display(),
                            ffmpeg_preflight.version_detected.unwrap_or_else(|| "none".to_string()),
                            ffmpeg_preflight.error.unwrap_or_else(|| "none".to_string()),
                            ffmpeg_visibility
                                .ffmpeg_spawn_error
                                .unwrap_or_else(|| "none".to_string()),
                            summary.configured_max_frames,
                            summary.configured_max_ticks,
                            configured_frame_interval_ms,
                            summary.encoder_runtime.as_config_str(),
                            summary.encoder_process_start_count,
                            summary.runtime_ticks,
                            summary.capture_attempts,
                            summary.frames_captured,
                            summary.frames_encoded,
                            summary.frames_sent,
                            summary.direct_sends,
                            summary.fragmented_sends,
                            summary.fragments_attempted,
                            summary.fragments_sent,
                            summary.no_frame_count,
                            summary.capture_failures,
                            summary.encode_failures,
                            summary.frame_build_failures,
                            summary.send_failures,
                            summary.persistent_access_units_emitted,
                            summary.persistent_no_complete_access_unit_count,
                            summary.persistent_stdout_closed_count,
                            summary.persistent_malformed_stream_count,
                            last_encoder_exit_status,
                            summary.frames_remaining_to_max,
                            elapsed_ms,
                            capture_elapsed_ms,
                            encode_elapsed_ms,
                            avg_capture_elapsed_ms,
                            avg_encode_elapsed_ms,
                            capture_wait_or_no_frame_elapsed_ms,
                            effective_output_fps,
                            effective_fresh_capture_fps,
                            effective_send_fps,
                            loop_interval_sleep_ms,
                            total_fragment_pacing_sleep_ms,
                            send_elapsed_ms,
                            summary.ticks_elapsed_while_sending,
                            last_encode_error,
                            ffmpeg_visibility
                                .last_ffmpeg_error
                                .unwrap_or_else(|| "none".to_string()),
                            last_payload_len,
                            oversized_payload_count,
                            fragmentation_pressure_count,
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
                "stream-sync-client scaffold; use --auth-request-poc-once [config-path], --auth-heartbeat-poc-once [config-path], --auth-heartbeat-stats-poc-once [config-path], --placeholder-video-frame-poc-once [config-path], --auth-placeholder-video-frame-poc-once [config-path], --real-encoded-video-frame-poc-once [config-path], --auth-real-encoded-video-frame-poc-once [config-path], --auth-real-encoded-video-frame-poc-bounded [config-path] [max-frames] [fragment-pacing-every] [fragment-pacing-delay-ms] [--encoder-runtime per_frame|persistent] (internal bounded guard: configured_max_ticks = max(max_frames, max_frames * 10)), --auth-heartbeat-one-tick-runtime [config-path], or --auth-heartbeat-stats-one-tick-runtime [config-path]"
            );
        }
    }
}

fn format_duration_ms(duration_micros: u64) -> String {
    format!("{:.3}", duration_micros as f64 / 1_000.0)
}

fn format_fps(count: u64, duration_micros: u64) -> String {
    if count == 0 || duration_micros == 0 {
        return "0.000".to_string();
    }
    format!(
        "{:.3}",
        count as f64 / (duration_micros as f64 / 1_000_000.0)
    )
}

fn format_average_duration_ms(total_duration_micros: u64, count: u64) -> String {
    if count == 0 {
        return "0.000".to_string();
    }
    format!(
        "{:.3}",
        total_duration_micros as f64 / count as f64 / 1_000.0
    )
}

fn last_encode_error_from_results(
    results: &[stream_sync_client::ClientRealEncodedVideoFrameOneShotResult],
) -> String {
    results
        .iter()
        .rev()
        .find_map(|result| match result {
            stream_sync_client::ClientRealEncodedVideoFrameOneShotResult::EncodeUnavailable {
                reason,
            } => Some(format!("{reason:?}")),
            _ => None,
        })
        .unwrap_or_else(|| "none".to_string())
}

fn last_payload_len_from_results(
    results: &[stream_sync_client::ClientRealEncodedVideoFrameOneShotResult],
) -> String {
    results
        .iter()
        .rev()
        .find_map(|result| match result {
            stream_sync_client::ClientRealEncodedVideoFrameOneShotResult::Sent(sent) => {
                Some(sent.frame.payload.len().to_string())
            }
            stream_sync_client::ClientRealEncodedVideoFrameOneShotResult::SendFailed {
                failure,
            } => Some(failure.payload_len.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "none".to_string())
}

fn oversized_payload_count_from_results(
    results: &[stream_sync_client::ClientRealEncodedVideoFrameOneShotResult],
) -> usize {
    results
        .iter()
        .filter(|result| {
            matches!(
                result,
                stream_sync_client::ClientRealEncodedVideoFrameOneShotResult::SendFailed {
                    failure: stream_sync_client::ClientVideoFrameEncodeSendFailure {
                        error: stream_sync_client::ClientVideoFrameEncodeSendError::PacketTooLarge {
                            ..
                        },
                        ..
                    }
                }
            )
        })
        .count()
}

fn fragmentation_pressure_count_from_results(
    results: &[stream_sync_client::ClientRealEncodedVideoFrameOneShotResult],
) -> usize {
    results
        .iter()
        .filter(|result| match result {
            stream_sync_client::ClientRealEncodedVideoFrameOneShotResult::Sent(sent) => {
                matches!(
                    sent.send.summary,
                    stream_sync_client::ClientVideoFrameSendSummary::FragmentedSent { .. }
                )
            }
            stream_sync_client::ClientRealEncodedVideoFrameOneShotResult::SendFailed {
                failure,
            } => failure.fragments_attempted > 0,
            _ => false,
        })
        .count()
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
