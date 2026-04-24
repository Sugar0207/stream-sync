use stream_sync_net_core::PacketSource;
use stream_sync_protocol::{
    ClientId, Codec, MessageType, ProtocolVersion, RunId, TimestampMicros, VideoFrame,
};
use stream_sync_server::{
    AuthenticatedSenderEntry, ServerReceiveAuthVideoQueueOnceLauncher,
    ServerRegisteredVideoFramePacket, ServerVideoFrameHandlerBoundary, ServerVideoFrameQueuePolicy,
    ServerVideoFrameQueueState, ServerVideoFrameQueueStorageBoundary,
};
use stream_sync_switcher::{
    SwitcherAuthVideoPlaceholderBridgeBoundary, SwitcherAuthVideoPlaceholderBridgeResult,
    SwitcherPlaceholderManualVerificationBoundary, SwitcherPlaceholderManualVerificationInput,
    SwitcherPlaceholderManualVerificationResult,
};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--placeholder-fixture-once") => {
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let queue_state = fixture_queue_state(&client_id);
            let result = SwitcherPlaceholderManualVerificationBoundary::default()
                .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                    queue_state: &queue_state,
                    client_id: &client_id,
                });
            print_summary(result, true);
        }
        Some("--placeholder-empty-once") => {
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let queue_state = ServerVideoFrameQueueState::default();
            let result = SwitcherPlaceholderManualVerificationBoundary::default()
                .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                    queue_state: &queue_state,
                    client_id: &client_id,
                });
            print_summary(result, true);
        }
        Some("--receive-auth-video-placeholder-bridge-once") => {
            let config_path = args
                .next()
                .unwrap_or_else(|| "configs/examples/server.example.toml".to_string());
            let client_id = ClientId(args.next().unwrap_or_else(|| "client-1".to_string()));
            let launcher = ServerReceiveAuthVideoQueueOnceLauncher::default();
            match launcher.run_once_from_path_with_writers(
                &config_path,
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
                std::io::stderr(),
            ) {
                Ok(server_outcome) => {
                    let result = SwitcherAuthVideoPlaceholderBridgeBoundary::default()
                        .verify_server_outcome(&server_outcome, &client_id);
                    print_bridge_summary(result);
                }
                Err(error) => {
                    eprintln!("switcher auth/video placeholder bridge failed: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!(
                "stream-sync-switcher scaffold; use --placeholder-fixture-once [client-id], --placeholder-empty-once [client-id], or --receive-auth-video-placeholder-bridge-once [config-path] [client-id]"
            );
        }
    }
}

fn print_summary(result: SwitcherPlaceholderManualVerificationResult, fixture_queue: bool) {
    match result {
        SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } => {
            println!(
                "switcher placeholder helper fixture_queue={} cross_process_queue=false no_frame={} selected_client_id={} frame_id={} payload_len={} decode_status={:?}",
                fixture_queue,
                summary.no_frame,
                summary.selected_client_id.0,
                summary.frame_id.unwrap_or(0),
                summary.encoded_payload_len.unwrap_or(0),
                handoff.decode_status
            );
        }
        SwitcherPlaceholderManualVerificationResult::NoFrame { summary } => {
            println!(
                "switcher placeholder helper fixture_queue={} cross_process_queue=false no_frame={} selected_client_id={} frame_id=none payload_len=none decode_status=none",
                fixture_queue, summary.no_frame, summary.selected_client_id.0
            );
        }
    }
}

fn print_bridge_summary(result: SwitcherAuthVideoPlaceholderBridgeResult) {
    match result {
        SwitcherAuthVideoPlaceholderBridgeResult::PlaceholderReady { summary, .. } => {
            println!(
                "switcher auth/video placeholder bridge in_process=true cross_process_queue=false auth_accepted={} video_received={} video_accepted={} video_rejected={} queued={} queue_len={} dropped_oldest={} no_frame={} selected_client_id={} frame_id={} payload_len={} decode_status={:?}",
                summary.auth_accepted,
                summary.video_received,
                summary.video_accepted,
                summary.video_rejected,
                summary.queued,
                summary.queue_len,
                summary.dropped_oldest,
                summary.no_frame,
                summary.selected_client_id.0,
                summary.selected_frame_id.unwrap_or(0),
                summary.payload_len.unwrap_or(0),
                summary.decode_status
            );
        }
        SwitcherAuthVideoPlaceholderBridgeResult::NoFrame { summary } => {
            println!(
                "switcher auth/video placeholder bridge in_process=true cross_process_queue=false auth_accepted={} video_received={} video_accepted={} video_rejected={} queued={} queue_len={} dropped_oldest={} no_frame={} selected_client_id={} frame_id=none payload_len=none decode_status=none",
                summary.auth_accepted,
                summary.video_received,
                summary.video_accepted,
                summary.video_rejected,
                summary.queued,
                summary.queue_len,
                summary.dropped_oldest,
                summary.no_frame,
                summary.selected_client_id.0
            );
        }
    }
}

fn fixture_queue_state(client_id: &ClientId) -> ServerVideoFrameQueueState {
    let mut state = ServerVideoFrameQueueState::default();
    let source = PacketSource {
        address: "127.0.0.1:5001"
            .parse()
            .expect("fixture source should parse"),
    };
    let packet = ServerRegisteredVideoFramePacket {
        source,
        authenticated_sender: AuthenticatedSenderEntry {
            client_id: client_id.clone(),
            source,
            run_id: RunId("run-1".to_string()),
            protocol_version: ProtocolVersion(1),
            registered_at: None,
        },
        frame: VideoFrame {
            message_type: MessageType::VideoFrame,
            protocol_version: ProtocolVersion(1),
            client_id: client_id.clone(),
            run_id: RunId("run-1".to_string()),
            frame_id: 42,
            capture_timestamp: TimestampMicros(1_000_042),
            send_timestamp: TimestampMicros(1_000_142),
            is_keyframe: true,
            metadata_reserved: [0; 3],
            width: 1280,
            height: 720,
            fps_nominal: 30,
            codec: Codec::H264,
            payload_size: 3,
            payload: vec![0x42, 0xbb, 0xcc],
        },
    };
    let input = ServerVideoFrameHandlerBoundary.prepare_input(packet);
    let _result = ServerVideoFrameQueueStorageBoundary.store_frame(
        &mut state,
        input,
        TimestampMicros(2_000_042),
        ServerVideoFrameQueuePolicy::default(),
    );
    state
}
