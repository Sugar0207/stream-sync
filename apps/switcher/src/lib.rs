use stream_sync_protocol::{ClientId, TimestampMicros};
use stream_sync_server::{ServerQueuedVideoFrame, ServerVideoFrameQueueState};

pub const CRATE_NAME: &str = "stream-sync-switcher";

/// Input for selecting one client's latest encoded frame for single-view PoC.
///
/// The queue state is borrowed from the caller and is not mutated by this
/// selection boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherSingleViewFrameSelectionInput<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub client_id: &'a ClientId,
}

/// Encoded frame selected for a future single-view display path.
///
/// This remains encoded H.264 payload plus metadata. It is not decoded pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSingleViewSelectedEncodedFrame {
    pub client_id: ClientId,
    pub frame_id: u64,
    pub capture_timestamp: TimestampMicros,
    pub send_timestamp: TimestampMicros,
    pub queued_at: TimestampMicros,
    pub is_keyframe: bool,
    pub width: u32,
    pub height: u32,
    pub fps_nominal: u32,
    pub encoded_payload_len: usize,
    pub encoded_payload: Vec<u8>,
}

impl From<&ServerQueuedVideoFrame> for SwitcherSingleViewSelectedEncodedFrame {
    fn from(queued: &ServerQueuedVideoFrame) -> Self {
        Self {
            client_id: queued.frame.client_id.clone(),
            frame_id: queued.frame.frame_id,
            capture_timestamp: queued.frame.capture_timestamp,
            send_timestamp: queued.frame.send_timestamp,
            queued_at: queued.queued_at,
            is_keyframe: queued.frame.is_keyframe,
            width: queued.frame.width,
            height: queued.frame.height,
            fps_nominal: queued.frame.fps_nominal,
            encoded_payload_len: queued.payload_len,
            encoded_payload: queued.frame.payload.clone(),
        }
    }
}

/// Result of reading the queue for one single-view client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherSingleViewFrameSelectionResult {
    FrameAvailable(SwitcherSingleViewSelectedEncodedFrame),
    NoFrameAvailable { client_id: ClientId },
}

/// Read-only latest-frame selector for the single-view PoC.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleViewLatestFrameSelectionBoundary;

impl SwitcherSingleViewLatestFrameSelectionBoundary {
    pub fn select_latest(
        &self,
        input: SwitcherSingleViewFrameSelectionInput<'_>,
    ) -> SwitcherSingleViewFrameSelectionResult {
        input
            .queue_state
            .frames_for_client(input.client_id)
            .last()
            .map(SwitcherSingleViewSelectedEncodedFrame::from)
            .map(SwitcherSingleViewFrameSelectionResult::FrameAvailable)
            .unwrap_or_else(
                || SwitcherSingleViewFrameSelectionResult::NoFrameAvailable {
                    client_id: input.client_id.clone(),
                },
            )
    }
}

/// Explicit placeholder status for the future H.264 decode step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitcherSingleViewDecodeStatus {
    DeferredPlaceholder,
}

/// Placeholder display handoff for a selected single-view frame.
///
/// This is display-ready only in the sense that a future display owner can see
/// which encoded frame would be shown. It does not contain decoded pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherSingleViewDisplayPlaceholderHandoff {
    pub selected: SwitcherSingleViewSelectedEncodedFrame,
    pub decode_status: SwitcherSingleViewDecodeStatus,
}

/// Result of preparing a single-view display handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherSingleViewDisplayHandoffResult {
    DisplayReadyPlaceholder(SwitcherSingleViewDisplayPlaceholderHandoff),
    NoFrameAvailable { client_id: ClientId },
}

/// Placeholder decode/display boundary for the single-view PoC.
///
/// This boundary preserves the selected encoded frame and marks decode as
/// deferred. It does not call FFmpeg, allocate pixel buffers, render UI, sync
/// frames, or integrate with OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleViewPlaceholderDisplayBoundary;

impl SwitcherSingleViewPlaceholderDisplayBoundary {
    pub fn prepare_handoff(
        &self,
        selection: SwitcherSingleViewFrameSelectionResult,
    ) -> SwitcherSingleViewDisplayHandoffResult {
        match selection {
            SwitcherSingleViewFrameSelectionResult::FrameAvailable(selected) => {
                SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(
                    SwitcherSingleViewDisplayPlaceholderHandoff {
                        selected,
                        decode_status: SwitcherSingleViewDecodeStatus::DeferredPlaceholder,
                    },
                )
            }
            SwitcherSingleViewFrameSelectionResult::NoFrameAvailable { client_id } => {
                SwitcherSingleViewDisplayHandoffResult::NoFrameAvailable { client_id }
            }
        }
    }
}

/// Thin composition for the current single-view placeholder path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherSingleViewPlaceholderPathBoundary {
    selection: SwitcherSingleViewLatestFrameSelectionBoundary,
    display: SwitcherSingleViewPlaceholderDisplayBoundary,
}

impl SwitcherSingleViewPlaceholderPathBoundary {
    pub fn prepare_latest_display_handoff(
        &self,
        input: SwitcherSingleViewFrameSelectionInput<'_>,
    ) -> SwitcherSingleViewDisplayHandoffResult {
        let selected = self.selection.select_latest(input);
        self.display.prepare_handoff(selected)
    }
}

/// Input for manual queue-to-switcher placeholder verification.
///
/// The queue state is caller-owned and borrowed read-only. This is intentionally
/// not a cross-process bridge to a running server queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherPlaceholderManualVerificationInput<'a> {
    pub queue_state: &'a ServerVideoFrameQueueState,
    pub client_id: &'a ClientId,
}

/// Compact summary for manual placeholder verification output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherPlaceholderManualVerificationSummary {
    pub selected_client_id: ClientId,
    pub frame_id: Option<u64>,
    pub encoded_payload_len: Option<usize>,
    pub decode_status: Option<SwitcherSingleViewDecodeStatus>,
    pub no_frame: bool,
}

/// Result of the manual queue-to-switcher placeholder helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitcherPlaceholderManualVerificationResult {
    PlaceholderReady {
        summary: SwitcherPlaceholderManualVerificationSummary,
        handoff: SwitcherSingleViewDisplayPlaceholderHandoff,
    },
    NoFrame {
        summary: SwitcherPlaceholderManualVerificationSummary,
    },
}

/// Runtime helper for the manual placeholder PoC.
///
/// This composes the existing latest-frame selection and placeholder display
/// handoff boundaries, then surfaces a CLI/test-friendly summary. It does not
/// mutate queue state, decode H.264, render a window, share state with a server
/// process, run sync scheduling, or touch OBS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SwitcherPlaceholderManualVerificationBoundary {
    path: SwitcherSingleViewPlaceholderPathBoundary,
}

impl SwitcherPlaceholderManualVerificationBoundary {
    pub fn verify_latest_placeholder(
        &self,
        input: SwitcherPlaceholderManualVerificationInput<'_>,
    ) -> SwitcherPlaceholderManualVerificationResult {
        match self
            .path
            .prepare_latest_display_handoff(SwitcherSingleViewFrameSelectionInput {
                queue_state: input.queue_state,
                client_id: input.client_id,
            }) {
            SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(handoff) => {
                let summary = SwitcherPlaceholderManualVerificationSummary {
                    selected_client_id: handoff.selected.client_id.clone(),
                    frame_id: Some(handoff.selected.frame_id),
                    encoded_payload_len: Some(handoff.selected.encoded_payload_len),
                    decode_status: Some(handoff.decode_status),
                    no_frame: false,
                };
                SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff }
            }
            SwitcherSingleViewDisplayHandoffResult::NoFrameAvailable { client_id } => {
                SwitcherPlaceholderManualVerificationResult::NoFrame {
                    summary: SwitcherPlaceholderManualVerificationSummary {
                        selected_client_id: client_id,
                        frame_id: None,
                        encoded_payload_len: None,
                        decode_status: None,
                        no_frame: true,
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stream_sync_net_core::PacketSource;
    use stream_sync_protocol::{Codec, MessageType, ProtocolVersion, RunId, VideoFrame};
    use stream_sync_server::{
        AuthenticatedSenderEntry, ServerRegisteredVideoFramePacket,
        ServerVideoFrameHandlerBoundary, ServerVideoFrameQueuePolicy,
        ServerVideoFrameQueueStorageBoundary,
    };

    #[test]
    fn single_view_latest_selection_returns_newest_frame_for_client() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 1, TimestampMicros(2_000_000));
        store_frame(&mut state, "client-1", 2, TimestampMicros(2_000_100));
        store_frame(&mut state, "client-2", 9, TimestampMicros(2_000_200));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleViewLatestFrameSelectionBoundary.select_latest(
            SwitcherSingleViewFrameSelectionInput {
                queue_state: &state,
                client_id: &client_id,
            },
        );

        let SwitcherSingleViewFrameSelectionResult::FrameAvailable(selected) = result else {
            panic!("latest frame should be available");
        };
        assert_eq!(selected.client_id, client_id);
        assert_eq!(selected.frame_id, 2);
        assert_eq!(selected.queued_at, TimestampMicros(2_000_100));
        assert_eq!(selected.encoded_payload_len, 3);
        assert_eq!(selected.encoded_payload, vec![0x02, 0xbb, 0xcc]);
    }

    #[test]
    fn single_view_latest_selection_no_frame_path_is_explicit() {
        let state = ServerVideoFrameQueueState::default();
        let client_id = ClientId("missing-client".to_string());

        let result = SwitcherSingleViewLatestFrameSelectionBoundary.select_latest(
            SwitcherSingleViewFrameSelectionInput {
                queue_state: &state,
                client_id: &client_id,
            },
        );

        assert_eq!(
            result,
            SwitcherSingleViewFrameSelectionResult::NoFrameAvailable { client_id }
        );
    }

    #[test]
    fn placeholder_display_handoff_preserves_metadata_and_payload_length() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 7, TimestampMicros(2_100_000));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherSingleViewPlaceholderPathBoundary::default()
            .prepare_latest_display_handoff(SwitcherSingleViewFrameSelectionInput {
                queue_state: &state,
                client_id: &client_id,
            });

        let SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(handoff) = result
        else {
            panic!("placeholder display handoff should be available");
        };
        assert_eq!(
            handoff.decode_status,
            SwitcherSingleViewDecodeStatus::DeferredPlaceholder
        );
        assert_eq!(handoff.selected.client_id, client_id);
        assert_eq!(handoff.selected.frame_id, 7);
        assert_eq!(
            handoff.selected.capture_timestamp,
            TimestampMicros(1_000_007)
        );
        assert_eq!(handoff.selected.send_timestamp, TimestampMicros(1_000_107));
        assert_eq!(handoff.selected.width, 1280);
        assert_eq!(handoff.selected.height, 720);
        assert_eq!(handoff.selected.fps_nominal, 30);
        assert_eq!(handoff.selected.encoded_payload_len, 3);
        assert_eq!(handoff.selected.encoded_payload, vec![0x07, 0xbb, 0xcc]);
    }

    #[test]
    fn single_view_queue_read_does_not_mutate_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 1, TimestampMicros(2_200_000));
        store_frame(&mut state, "client-1", 2, TimestampMicros(2_200_100));
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let _result = SwitcherSingleViewPlaceholderPathBoundary::default()
            .prepare_latest_display_handoff(SwitcherSingleViewFrameSelectionInput {
                queue_state: &state,
                client_id: &client_id,
            });

        assert_eq!(state.client_queue_len(&client_id), before_len);
        let frame_ids: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(frame_ids, vec![1, 2]);
    }

    #[test]
    fn placeholder_display_boundary_does_not_perform_real_decode_or_display() {
        let selected = SwitcherSingleViewSelectedEncodedFrame {
            client_id: ClientId("client-1".to_string()),
            frame_id: 3,
            capture_timestamp: TimestampMicros(1_000_003),
            send_timestamp: TimestampMicros(1_000_103),
            queued_at: TimestampMicros(2_300_000),
            is_keyframe: true,
            width: 1280,
            height: 720,
            fps_nominal: 30,
            encoded_payload_len: 3,
            encoded_payload: vec![0x03, 0xbb, 0xcc],
        };

        let result = SwitcherSingleViewPlaceholderDisplayBoundary.prepare_handoff(
            SwitcherSingleViewFrameSelectionResult::FrameAvailable(selected.clone()),
        );

        assert_eq!(
            result,
            SwitcherSingleViewDisplayHandoffResult::DisplayReadyPlaceholder(
                SwitcherSingleViewDisplayPlaceholderHandoff {
                    selected,
                    decode_status: SwitcherSingleViewDecodeStatus::DeferredPlaceholder,
                }
            )
        );
    }

    #[test]
    fn manual_verification_helper_selects_latest_frame_from_caller_owned_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 10, TimestampMicros(2_400_000));
        store_frame(&mut state, "client-1", 11, TimestampMicros(2_400_100));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        let SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } =
            result
        else {
            panic!("fixture queue should produce a placeholder handoff");
        };
        assert_eq!(summary.selected_client_id, client_id);
        assert_eq!(summary.frame_id, Some(11));
        assert_eq!(handoff.selected.frame_id, 11);
    }

    #[test]
    fn manual_verification_helper_reports_no_frame_for_empty_queue() {
        let state = ServerVideoFrameQueueState::default();
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        assert_eq!(
            result,
            SwitcherPlaceholderManualVerificationResult::NoFrame {
                summary: SwitcherPlaceholderManualVerificationSummary {
                    selected_client_id: client_id,
                    frame_id: None,
                    encoded_payload_len: None,
                    decode_status: None,
                    no_frame: true,
                }
            }
        );
    }

    #[test]
    fn manual_verification_helper_preserves_metadata_and_payload_length() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 12, TimestampMicros(2_500_000));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        let SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } =
            result
        else {
            panic!("fixture queue should produce a placeholder handoff");
        };
        assert_eq!(summary.frame_id, Some(12));
        assert_eq!(summary.encoded_payload_len, Some(3));
        assert_eq!(
            handoff.selected.capture_timestamp,
            TimestampMicros(1_000_012)
        );
        assert_eq!(handoff.selected.send_timestamp, TimestampMicros(1_000_112));
        assert_eq!(handoff.selected.encoded_payload_len, 3);
        assert_eq!(handoff.selected.encoded_payload, vec![0x0c, 0xbb, 0xcc]);
    }

    #[test]
    fn manual_verification_helper_reports_decode_deferred_placeholder() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 13, TimestampMicros(2_600_000));
        let client_id = ClientId("client-1".to_string());

        let result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        let SwitcherPlaceholderManualVerificationResult::PlaceholderReady { summary, handoff } =
            result
        else {
            panic!("fixture queue should produce a placeholder handoff");
        };
        assert_eq!(
            summary.decode_status,
            Some(SwitcherSingleViewDecodeStatus::DeferredPlaceholder)
        );
        assert_eq!(
            handoff.decode_status,
            SwitcherSingleViewDecodeStatus::DeferredPlaceholder
        );
    }

    #[test]
    fn manual_verification_helper_does_not_mutate_queue() {
        let mut state = ServerVideoFrameQueueState::default();
        store_frame(&mut state, "client-1", 14, TimestampMicros(2_700_000));
        store_frame(&mut state, "client-1", 15, TimestampMicros(2_700_100));
        let client_id = ClientId("client-1".to_string());
        let before_len = state.client_queue_len(&client_id);

        let _result = SwitcherPlaceholderManualVerificationBoundary::default()
            .verify_latest_placeholder(SwitcherPlaceholderManualVerificationInput {
                queue_state: &state,
                client_id: &client_id,
            });

        assert_eq!(state.client_queue_len(&client_id), before_len);
        let frame_ids: Vec<u64> = state
            .frames_for_client(&client_id)
            .map(|queued| queued.frame.frame_id)
            .collect();
        assert_eq!(frame_ids, vec![14, 15]);
    }

    fn store_frame(
        state: &mut ServerVideoFrameQueueState,
        client_id: &str,
        frame_id: u64,
        queued_at: TimestampMicros,
    ) {
        let source = PacketSource {
            address: "127.0.0.1:5001".parse().unwrap(),
        };
        let packet = ServerRegisteredVideoFramePacket {
            source,
            authenticated_sender: AuthenticatedSenderEntry {
                client_id: ClientId(client_id.to_string()),
                source,
                run_id: RunId("run-1".to_string()),
                protocol_version: ProtocolVersion(1),
                registered_at: None,
            },
            frame: VideoFrame {
                message_type: MessageType::VideoFrame,
                protocol_version: ProtocolVersion(1),
                client_id: ClientId(client_id.to_string()),
                run_id: RunId("run-1".to_string()),
                frame_id,
                capture_timestamp: TimestampMicros(1_000_000 + frame_id),
                send_timestamp: TimestampMicros(1_000_100 + frame_id),
                is_keyframe: frame_id == 1,
                metadata_reserved: [0; 3],
                width: 1280,
                height: 720,
                fps_nominal: 30,
                codec: Codec::H264,
                payload_size: 3,
                payload: vec![frame_id as u8, 0xbb, 0xcc],
            },
        };
        let input = ServerVideoFrameHandlerBoundary.prepare_input(packet);
        let _result = ServerVideoFrameQueueStorageBoundary.store_frame(
            state,
            input,
            queued_at,
            ServerVideoFrameQueuePolicy::default(),
        );
    }
}
