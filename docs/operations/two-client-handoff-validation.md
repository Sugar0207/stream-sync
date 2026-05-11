# 2-client Same-PC Server to Switcher Handoff Validation

This document is the current source of truth for the next MVP step after the
same-PC 2-client ingest / reassembly longer-run PASS.

Scope for this step:

- same-PC only
- 2 real clients only
- server queue / named-pipe handoff / switcher selection path only
- human-run validation only

Out of scope for this step:

- 4-client all-real validation
- full OBS operator validation
- daemon lifecycle
- reconnect policy
- adaptive jitter buffer expansion
- dashboard / exporter work

Known limitation for this staged validation:

- current `--receive-auth-video-queue-and-serve-handoff-many` is not a
  realtime concurrent runtime
- the command finishes the bounded receive/auth phase first
- only after that does it open the named-pipe handoff service
- this is acceptable for bounded same-PC handoff validation
- realtime preview / production still needs a future concurrent
  receive-and-serve runtime

## Positioning

Current validated baseline before this step:

- same-PC 2-client ingest / reassembly longer-run PASS
- standard receive profile:
  - `receive_buffer_bytes=268435456`
  - `max_packets_per_drain_cycle=1024`
  - summary-only default
  - `receive_timeout_ms=30000`
  - `max_frames=900 per client`
  - `fragment_pacing_every=4`
  - `fragment_pacing_delay_ms=2`

This handoff step checks the next boundary:

```text
client -> server receive/auth/reassembly/queue
  -> server named-pipe handoff
  -> switcher queue read / targetTime selection / decode / render
```

## Current CLI And Boundary

Current server-side handoff owner:

```text
stream-sync-server --receive-auth-video-queue-and-serve-handoff-many
  [config-path]
  [pipe-name]
  [max-requests]
  [max-video-packets]
  [receive-timeout-ms]
  [expected-reassembled-frames]
  [stop-after-expected-reassembled-frames]
  [receive-buffer-bytes]
  [expected-reassembled-clients]
  [expected-reassembled-frames-per-client]
```

What this server command owns:

- auth
- UDP receive
- fragment reassembly
- accepted frame queue insertion
- bounded named-pipe handoff serving

Current switcher-side raw handoff read:

```text
stream-sync-switcher --read-queued-frame-handoff-once
  [pipe-name]
  [client-id]
  [run-id]
  [read-mode]
  [request-id]
```

What this switcher command owns:

- one named-pipe request
- one queued-frame read result
- no targetTime scheduling
- no 4-view render loop

Current switcher-side preview/read path for this step:

```text
stream-sync-switcher --four-view-two-real-handoff-preview-loop
  [pipe-name]
  [slot0-index]
  [client0-id]
  [run0-id]
  [slot1-index]
  [client1-id]
  [run1-id]
  [frames]
  [preview-oldest|preview-latest|preview-latest-decodable]
```

What this switcher command owns:

- named-pipe handoff reads for the two real slots
- switcher-side targetTime selection
- `Selected` / `NoFrameAvailable` / `WaitingForFrameAtOrBeforeTarget` /
  `HandoffError` preservation
- decode / render into the existing clean output window path
- optional IDR-preferring preview mode for one-shot decode:
  - `preview-latest-decodable`

Important current CLI note:

- current handoff CLI now normalizes both:
  - `streamsync-handoff-dev`
  - `\\.\pipe\streamsync-handoff-dev`
- both forms resolve to the same:
  - `actual_pipe_path=\\.\pipe\streamsync-handoff-dev`
- plain short name is still the recommended operator-facing input because it is
  easier to read and compare in PowerShell

## Current Read Path

The current real handoff path is:

```text
ServerVideoFrameQueueState
  -> ServerVideoFrameQueueReadBoundary
  -> server named-pipe handoff request/response
  -> SwitcherQueuedFrameHandoff
  -> switcher targetTime scheduler
  -> switcher decode/render
  -> StreamSync 4-view Output
```

Important ownership split:

- server stops at queued encoded frame handoff
- switcher owns targetTime selection after handoff
- switcher owns `WaitingForFrameAtOrBeforeTarget`
- switcher owns render-facing state

## Same-PC Preconditions

1. Use the current standard same-PC client profile:
   - `max_frames=900 per client`
   - `fragment_pacing_every=4`
   - `fragment_pacing_delay_ms=2`
2. Keep the current server receive profile:
   - `receive_buffer_bytes=268435456`
   - `receive_timeout_ms=30000`
3. Use these configs:
   - `configs/manual/server.two-real-slots.toml`
   - `configs/manual/client.player1.toml`
   - `configs/manual/client.player2.toml`
4. Confirm the same `run_id` is used on both clients:
   - current manual configs use `streamsync-dev-session`
5. Build once before the run:

```powershell
cargo build -p stream-sync-server -p stream-sync-switcher -p stream-sync-client
```

## What To Observe

### Server Receive Summary

Primary receive-side fields:

- `registered_clients`
- `frames_reassembled`
- `frames_queued`
- `direct_frames_queued`
- `rejected_packets`
- `incomplete_reassembly_frames`
- `manual_expected_reassembled_clients`
- `manual_expected_reassembled_frames_per_client`
- `observed_queued_clients`
- `observed_reassembled_clients`
- `per_client_queued_frames`
- `per_client_direct_frames`
- `per_client_reassembled_frames`
- `retained_keyframe_clients`
- `per_client_retained_keyframe_frame_id`
- `validation_ready`
- `ready_reason`
- `receive_stop_reason`
- `stop_reason`

### Client Persistent Encoder Summary

When running the current recommended client command:

```text
--auth-real-encoded-video-frame-poc-bounded ... --encoder-runtime persistent --cadence-mode deadline
```

also inspect:

- `h264_parameter_sets_cached`
- `h264_sps_count`
- `h264_pps_count`
- `h264_parameter_sets_prepended_count`
- `last_payload_had_parameter_sets`
- `h264_parameter_sets_missing_count`
- `last_payload_has_sps`
- `last_payload_has_pps`
- `last_payload_has_idr`
- `last_payload_has_non_idr_vcl`
- `encoder_width`
- `encoder_height`

Interpretation:

- `h264_parameter_sets_cached=true` means the client has cached both SPS and
  PPS from the persistent Annex B stream
- `h264_parameter_sets_prepended_count` shows how many sent access-unit
  payloads needed cached SPS/PPS prepended for switcher one-shot decode
- `h264_parameter_sets_missing_count` shows how many VCL access units were
  deferred because no complete SPS/PPS cache existed yet
- `last_encode_error=MissingH264ParameterSets` means the client chose typed
  encode-deferred behavior instead of sending a likely undecodable payload
- `last_payload_has_idr=false` with `last_payload_has_non_idr_vcl=true` is a
  useful clue if switcher one-shot decode still fails after SPS/PPS prepend
- `encoder_width` / `encoder_height` are now the encoder output dimensions that
  should survive through handoff and reach the switcher decode boundary

### Server Bounded Handoff Request Lines

Primary handoff-side fields:

- `handoff_ready`
- `validation_ready`
- `ready_reason`
- `receive_stop_reason`
- `actual_pipe_path`
- `queued_frames`
- `registered_clients`
- `expected_reassembled_frames`
- `expected_clients`
- `expected_per_client_frames`
- `observed_queued_clients`
- `observed_reassembled_clients`
- `per_client_queued_frames`
- `per_client_direct_frames`
- `per_client_reassembled_frames`
- `retained_keyframe_clients`
- `per_client_retained_keyframe_frame_id`
- `request_id`
- `result_kind`
- `selected_client_id`
- `selected_run_id`
- `frame_id`
- `frame_payload_len`
- `decodable_source`
- `retained_keyframe_available`
- `retained_keyframe_frame_id`
- `handoff_error`

For a good 2-client handoff run, both `player1` and `player2` should appear in
successful `FrameRead` request lines at least once.

### Expected Count Basis

Current validation thresholding for this command is queue-based:

- `frames_queued` is the total threshold counter
- `direct_frames_queued` is the direct-send subset
- `frames_reassembled` is the fragmented/reassembled subset
- per-client thresholding uses `per_client_queued_frames`

Interpretation:

- direct `VideoFrame` packets count toward the same validation-ready threshold
  as fragmented/reassembled frames
- `per_client_reassembled_frames` remains useful observability, but
  `validation_ready` is based on queued frames so mixed direct/fragmented runs
  can still satisfy the expected count

### Switcher Raw One-Shot Read

Primary fields from `--read-queued-frame-handoff-once`:

- `actual_pipe_path`
- `final_result=FrameRead|NoFrame|HandoffError`
- `handoff_response_kind`
- `parse_error`
- `io_error`
- `queue_len`
- `frame_id`
- `encoded_payload_len`

Use this command only as a transport/queue isolation rerun when the preview
loop result is unclear.

### Switcher Preview Loop

Primary fields from `--four-view-two-real-handoff-preview-loop`:

- `actual_pipe_path`
- `frames_attempted`
- `frames_rendered`
- `render_failures`
- `scheduler_status`
- `slot_bindings`
- `slot_result_kinds`
- `slot_diagnostics`
- `clean_output_render_result_kind`
- `window_title`
- `output_width`
- `output_height`

`slot_diagnostics` is the main per-slot drill-down surface. It already carries:

- `request_id`
- `actual_pipe_path`
- `handoff_response_kind`
- `parse_error`
- `io_error`
- `response_payload_len`
- `frame_id`
- `frame_payload_len`
- `decode_error`
- `render_input_kind`
- `final_slot_result_kind`

## How To Read Switcher State

### Selected Frame

Current visible signs:

- `slot_result_kinds` includes `Selected` for the real slot
- matching `slot_diagnostics` entry shows:
  - `handoff_response_kind=FrameRead`
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`
  - `final_slot_result_kind=Selected`

### NoFrame

Current visible signs:

- raw one-shot read:
  - `final_result=NoFrame`
- preview loop:
  - `slot_result_kinds` includes `NoFrameAvailable`

Important interpretation for this 2-real preview command:

- slots `2` and `3` are deterministic placeholder / no-frame slots by design
- `NoFrameAvailable` on those two non-real slots is expected
- real-slot `NoFrameAvailable` is a useful signal, but it is not the same as a
  transport failure

### Waiting

Current visible sign:

- preview loop `slot_result_kinds` includes
  `WaitingForFrameAtOrBeforeTarget`

Interpretation:

- handoff itself succeeded
- the switcher-side targetTime gate decided the candidate frame was newer than
  the current target
- this is not the same as `NoFrame`
- this is not the same as `HandoffError`

### HandoffError

Current visible signs:

- raw one-shot read:
  - `final_result=HandoffError`
- preview loop:
  - `scheduler_status=HandoffError`
  - or real-slot `slot_diagnostics` shows:
    - `handoff_response_kind=HandoffError`
    - `parse_error!=none`
    - or `io_error!=none`

Interpretation:

- named-pipe/runtime/transport failure
- not a normal empty-queue result
- not a targetTime waiting result

### Late-Drop Summary

Current state:

- the real handoff manual CLI does not currently print a dedicated late-drop
  aggregate in the final preview summary
- late-drop mutation exists in switcher-side code paths, but it is not the
  current primary observable for this same-PC handoff manual pass

Current decision:

- do not gate this 2-client handoff validation on explicit late-drop counters
- if a run looks suspicious, use:
  - server receive summary
  - server bounded handoff request lines
  - switcher `slot_result_kinds`
  - switcher `slot_diagnostics`

### Adjusted Timestamp And TargetTime Selection

Current state:

- the preview loop does exercise switcher-side targetTime selection
- but the current real handoff preview summary does not print:
  - `target_timestamp`
  - adjusted per-slot capture timestamp
  - per-slot clock offset used for comparison

Current decision:

- use `Selected` vs `WaitingForFrameAtOrBeforeTarget` as the current visible
  proxy for targetTime gating
- treat missing explicit adjusted-timestamp visibility as a narrow future
  observability gap, not as a blocker for this handoff-prep step

## Recommended Human Validation Recipe

Recommended preview recipe:

- real slots:
  - slot `0` -> `player1`
  - slot `1` -> `player2`
- placeholder slots:
  - slot `2`
  - slot `3`
- switcher preview frames:
  - `180`
- bounded handoff request budget:
  - `360`
  - current rule: `preview_frames * 2 real slots`

### Window 1: Server

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 360 500000 30000 1800 true 268435456 2 900
```

What this should guarantee:

- receive stays on the current same-PC standard profile
- receive phase waits for both clients:
  - `expected_reassembled_clients=2`
  - `expected_reassembled_frames_per_client=900`
- validation-ready frame counting uses queued frames:
  - `expected_reassembled_frames=1800`
  - counted against `frames_queued`
  - `per_client_queued_frames`
  - direct + reassembled frames both contribute
- bounded handoff request budget matches the planned preview loop
- after the receive phase finishes and the named-pipe server is actually ready,
  server stdout now emits a readiness line such as:
  - `handoff_ready=true`
  - `validation_ready=true`
  - `ready_reason=expected_clients_reached`
  - `receive_stop_reason=expected_clients_reached`
  - `pipe_name=streamsync-handoff-dev`
  - `actual_pipe_path=\\.\pipe\streamsync-handoff-dev`
  - `queued_frames=1800`
  - `registered_clients=2`
  - `expected_reassembled_frames=1800`
  - `expected_clients=2`
  - `expected_per_client_frames=900`
  - `observed_queued_clients=2`
  - `observed_reassembled_clients=2`
  - `per_client_queued_frames=player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`
  - `per_client_direct_frames=...`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`

Human start-order rule:

- do not start Window 4, Window 4a, or Window 4b before server stdout prints
  both:
  - `handoff_ready=true`
  - `validation_ready=true`
- if server instead prints:
  - `handoff_ready=true`
  - `validation_ready=false`
  - `ready_reason=receive_timeout`
  - or `ready_reason=max_packets_reached`
  stop the run and treat it as a failed validation-ready gate instead of
  starting switcher
- once the valid readiness line appears, use the printed `actual_pipe_path` as the source of
  truth and start switcher/raw one-shot reads immediately

### Window 2: Client 1

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

### Window 3: Client 2

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

### Window 4: Switcher Main Handoff Preview

Start this only after:

- both clients have been started
- server stdout has printed:
  - `handoff_ready=true`
  - `validation_ready=true`

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 180 preview-latest-decodable
```

Current recommendation:

- use `preview-latest-decodable` for same-PC handoff validation while the
  switcher decode path is still one-shot
- this mode keeps the existing preview behavior opt-in and asks the server
  handoff path for the latest queued frame marked decodable/keyframe-visible
- current persistent client metadata uses `VideoFrame.is_keyframe=true` when
  the encoded access unit contains an IDR NAL
- if the current retained queue window is shorter than one GOP, this mode now
  falls back to the latest retained keyframe for the same `client_id + run_id`
  scope

Expected render surface:

- `window_title=StreamSync 4-view Output`
- `output_width=1280`
- `output_height=720`

## Optional Transport Isolation Rerun

Use this only if the main preview loop reports an unexpected real-slot
`HandoffError` or persistent `NoFrameAvailable`.

### Window 4a: Raw One-Shot Read For Player 1

```powershell
.\target\debug\stream-sync-switcher.exe --read-queued-frame-handoff-once streamsync-handoff-dev player1 streamsync-dev-session preview-latest-decodable 1
```

### Window 4b: Raw One-Shot Read For Player 2

```powershell
.\target\debug\stream-sync-switcher.exe --read-queued-frame-handoff-once streamsync-handoff-dev player2 streamsync-dev-session preview-latest-decodable 2
```

Expected success shape for the raw rerun:

- `final_result=FrameRead`
- `handoff_response_kind=FrameRead`
- `parse_error=none`
- `io_error=none`
- `encoded_payload_len > 0`

## Pipe Troubleshooting

If PowerShell shows a `streamsync-*` pipe in `\\.\pipe\` but switcher still
reports `connect:(os_error_2)`, read the summaries in this order:

1. server bounded handoff line
   - confirm:
     - `handoff_ready=true`
     - `validation_ready=true`
     - `pipe_name=streamsync-handoff-dev`
     - `actual_pipe_path=\\.\pipe\streamsync-handoff-dev`
     - `ready_reason=expected_clients_reached`
     - `receive_stop_reason=expected_clients_reached`
2. if server already printed `handoff_stopped=true`
   - treat the bounded session as finished and rerun Window 1 before launching
     switcher
3. switcher preview summary
   - confirm:
     - `actual_pipe_path=\\.\pipe\streamsync-handoff-dev`
4. switcher `slot_diagnostics`
   - confirm real slots show the same `actual_pipe_path`
   - inspect:
     - `io_error`
     - `handoff_response_kind`
5. optional raw one-shot reads
   - compare `actual_pipe_path` again on:
     - `player1`
     - `player2`

Expected normalization rule:

- input `streamsync-handoff-dev`
  - `actual_pipe_path=\\.\pipe\streamsync-handoff-dev`
- input `\\.\pipe\streamsync-handoff-dev`
  - `actual_pipe_path=\\.\pipe\streamsync-handoff-dev`

If requested `pipe_name` differs but `actual_pipe_path` matches, the issue is
not short-name vs full-path normalization anymore. In that case, treat the next
suspects as:

- bounded server session already exited
- wrong handoff pipe name between windows
- a different process owns the displayed pipe name
- stale binary / stale command line

## Success Conditions

### Server Receive Side

- `registered_clients=2`
- `frames_queued=1800`
- `frames_reassembled` may be less than `1800`
- `direct_frames_queued` may be non-zero
- `rejected_packets=0`
- `incomplete_reassembly_frames=0`
- `manual_expected_reassembled_clients=2`
- `manual_expected_reassembled_frames_per_client=900`
- `observed_queued_clients=2`
- `observed_reassembled_clients` is visible but not the validation-ready gate when
  direct frames are present
- `per_client_queued_frames` shows both:
  - `player1/streamsync-dev-session:900`
  - `player2/streamsync-dev-session:900`
- `per_client_direct_frames` may show a non-zero subset
- `per_client_reassembled_frames` shows both:
  - or the remaining fragmented/reassembled subset for each client

Preferred receive stop result:

- `stop_reason=ReassembledFramesAndClientAwareThresholdReached`

Also acceptable:

- `stop_reason=ReceiveTimedOut`
- only if the expected reassembled/queued counts above were already reached

### Server Handoff Side

- bounded request lines show at least one successful `FrameRead` for:
  - `selected_client_id=player1`
  - `selected_client_id=player2`
- `handoff_error=none`
- readiness line shows:
  - `handoff_ready=true`
  - `validation_ready=true`
  - `ready_reason=expected_clients_reached`
  - `receive_stop_reason=expected_clients_reached`
- `handoff_ready=true` with `validation_ready=true` only means the staged
  receive phase completed and the bounded handoff service is ready
- it does not mean concurrent realtime preview behavior has been validated yet

### Switcher Side

- `frames_attempted=180`
- `frames_rendered=180`
- `render_failures=0`
- `scheduler_status=PartialSelected`
- `slot_result_kinds=Selected|Selected|NoFrameAvailable|NoFrameAvailable`
- `clean_output_render_result_kind=Rendered`
- `window_title=StreamSync 4-view Output`

Real-slot diagnostics should show:

- slot `0` / `player1`
  - `handoff_response_kind=FrameRead`
  - `frame_is_keyframe=true`
  - `decodable_source=queue` or `decodable_source=retained_keyframe`
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`
  - `final_slot_result_kind=Selected`
- slot `1` / `player2`
  - `handoff_response_kind=FrameRead`
  - `frame_is_keyframe=true`
  - `decodable_source=queue` or `decodable_source=retained_keyframe`
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`
  - `final_slot_result_kind=Selected`

If decode still fails, inspect these slot-diagnostic fields before assuming a
transport issue:

- `decode_input_payload_len`
- `decode_expected_width`
- `decode_expected_height`
- `decode_expected_pixel_format`
- `decode_expected_rawvideo_len`
- `decoded_stdout_len`
- `ffmpeg_exit_status`
- `ffmpeg_stderr_summary`
- `payload_has_sps`
- `payload_has_pps`
- `payload_has_idr`
- `payload_has_non_idr_vcl`
- `payload_nal_kinds`
- `handoff_no_frame_reason`
- `decodable_source`
- `retained_keyframe_available`
- `retained_keyframe_frame_id`

Current interpretation for this slice:

- if `payload_has_sps=true` and `payload_has_pps=true` but `decoded_stdout_len=0`,
  the remaining suspects are width/height mismatch in the decode expectation or
  one-shot decode on a non-IDR payload
- if `decode_expected_width` / `decode_expected_height` do not match the client
  encoder config, treat metadata mismatch as the next narrow follow-up
- if SPS/PPS are present but `payload_has_idr=false`, record that as evidence
  for a future keyframe/IDR handling slice
- current code now fixes the persistent-path metadata source so
  `decode_expected_width=1280`, `decode_expected_height=720`, and
  `decode_expected_rawvideo_len=3686400` are the expected same-PC validation
  values when the manual client configs use `1280x720`
- if `preview-latest-decodable` returns `NoFrame` with
  `handoff_no_frame_reason=NoDecodableFrameAvailable`, treat that as "queue
  has frames but neither a queued keyframe nor a retained keyframe was
  available"
- if `preview-latest-decodable` returns `FrameRead` with
  `decodable_source=retained_keyframe`, that is the expected fallback when the
  retained queue window is shorter than one GOP
- if `preview-latest-decodable` still returns `frame_is_keyframe=false` or
  `payload_has_idr=false`, treat that as evidence that the queue does not yet
  contain an IDR-bearing latest decodable payload for that client scope

### Not A Failure By Itself

- `NoFrameAvailable` on slots `2` and `3`
- `scheduler_status=PartialSelected` in this exact 2-real + 2-placeholder path
- `stop_reason=ReceiveTimedOut` after expected counts were already reached
- `handoff_ready=true` appearing only after both clients finish their bounded
  send run, because this command is intentionally staged today

## Latest Narrow Follow-Up

Current latest interpretation after the most recent human rerun:

- server receive / queue / `validation_ready` is already passing
- named-pipe handoff and `FrameRead` are already passing for both clients
- client-side SPS/PPS cache + prepend is already effective because the earlier
  `non-existing PPS 0 referenced` failure disappeared
- one real root cause was metadata width/height mismatch:
  - the persistent client path had been stamping raw capture dimensions into
    `VideoFrame`
  - the current code now stamps encoder output dimensions into `VideoFrame`
- another real blocker was queue retention vs GOP length:
  - the retained queue window was only `16` frames in the latest rerun
  - client GOP was `30`
  - latest-decodable could therefore find no queued IDR even though the run had
    produced IDRs earlier
- the current code now retains the latest keyframe per `client_id + run_id`
  separately from the bounded queue cap
- the next rerun after that still showed `retained_keyframe_clients=0`, which
  narrowed the problem further:
  - fragmented traffic dominated the run
  - client encoder GOP / SPS/PPS behavior still looked healthy
  - the missing piece was keyframe metadata propagation, not keyframe cadence
- the current code now carries `is_keyframe` through
  `VideoFrameFragment` wire encode/decode and server reassembly, and adds
  matching observability:
  - client summary:
    `h264_idr_count`, `h264_non_idr_vcl_count`, `keyframes_encoded`,
    `keyframes_sent`, `first_keyframe_frame_id`, `last_keyframe_frame_id`
  - server receive summary:
    `keyframes_received`, `keyframes_queued`,
    `per_client_keyframes_queued`, `first_keyframe_frame_id`,
    `last_keyframe_frame_id`
- use `preview-latest-decodable` for the next rerun so the preview loop prefers
  the latest queued keyframe first and falls back to the retained keyframe when
  needed

## Latest PASS Checkpoint

Latest same-PC 2-client human rerun is now a PASS for the current staged
handoff-preview scope.

PASS evidence:

- server:
  - `handoff_ready=true`
  - `validation_ready=true`
  - `ready_reason=expected_clients_reached`
  - `receive_stop_reason=expected_clients_reached`
  - `registered_clients=2`
  - `observed_queued_clients=2`
  - `observed_reassembled_clients=2`
  - `per_client_queued_frames=player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`
  - `per_client_direct_frames=player1/...:9|player2/...:9`
  - `per_client_reassembled_frames=player1/...:891|player2/...:891`
  - `retained_keyframe_clients=2`
  - `per_client_retained_keyframe_frame_id=player1/...:968|player2/...:975`
- client1:
  - `frames_sent=900`
  - `h264_idr_count=30`
  - `h264_non_idr_vcl_count=870`
  - `keyframes_encoded=30`
  - `keyframes_sent=30`
  - `first_keyframe_frame_id=4`
  - `last_keyframe_frame_id=968`
  - `h264_parameter_sets_cached=true`
  - `h264_sps_count=1`
  - `h264_pps_count=1`
  - `h264_parameter_sets_prepended_count=870`
  - `encode_failures=0`
  - `send_failures=0`
  - `effective_output_fps=26.385`
- client2:
  - `frames_sent=900`
  - `h264_idr_count=30`
  - `h264_non_idr_vcl_count=870`
  - `keyframes_encoded=30`
  - `keyframes_sent=30`
  - `first_keyframe_frame_id=4`
  - `last_keyframe_frame_id=975`
  - `h264_parameter_sets_cached=true`
  - `h264_sps_count=1`
  - `h264_pps_count=1`
  - `h264_parameter_sets_prepended_count=870`
  - `encode_failures=0`
  - `send_failures=0`
  - `effective_output_fps=26.192`
- switcher:
  - `--four-view-two-real-handoff-preview-loop ... preview-latest-decodable`
  - `frames_attempted=180`
  - `frames_rendered=180`
  - `render_failures=0`
  - `scheduler_status=PartialSelected`
  - `slot_result_kinds=Selected|Selected|NoFrameAvailable|NoFrameAvailable`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
  - slot0/player1:
    - `handoff_response_kind=FrameRead`
    - `frame_id=968`
    - `frame_is_keyframe=true`
    - `decodable_source=retained_keyframe`
    - `retained_keyframe_available=true`
    - `retained_keyframe_frame_id=968`
    - `decode_error=none`
    - `payload_has_sps=true`
    - `payload_has_pps=true`
    - `payload_has_idr=true`
    - `payload_has_non_idr_vcl=false`
    - `render_input_kind=UseUpdatedFrame`
  - slot1/player2:
    - `handoff_response_kind=FrameRead`
    - `frame_id=975`
    - `frame_is_keyframe=true`
    - `decodable_source=retained_keyframe`
    - `retained_keyframe_available=true`
    - `retained_keyframe_frame_id=975`
    - `decode_error=none`
    - `payload_has_sps=true`
    - `payload_has_pps=true`
    - `payload_has_idr=true`
    - `payload_has_non_idr_vcl=false`
    - `render_input_kind=UseUpdatedFrame`

Interpretation of this PASS:

- current 2-client real handoff preview validation is PASS for:
  - server receive / queue / validation-ready
  - client persistent + deadline send
  - SPS/PPS prepend
  - keyframe metadata propagation
  - retained-keyframe fallback
  - switcher handoff `FrameRead`
  - decode
  - 2-real-slot preview render
- `slot2` / `slot3` remain deterministic placeholder / no-frame slots in this
  exact command shape, so `NoFrameAvailable` there is expected and not a
  failure

Known limits that remain after this PASS:

- current `preview-latest-decodable` is still a staged keyframe-preview path:
  - the selected frames in this PASS came from
    `decodable_source=retained_keyframe`
  - this does not yet prove continuous latest non-IDR decode behavior
- receive and handoff serve are still staged rather than concurrent
- switcher still has no persistent decoder context for real-time latest-frame
  decode progression
- same-PC 2-client bounded runs can still drop to effective send rates around
  `26fps`, so capture/cadence load variance remains a known issue rather than a
  blocker for this checkpoint

## Failure Paste-Back Template

```text
[2-client handoff validation]
repo_path=
run_datetime=
pipe_name=streamsync-handoff-dev
receive_buffer_bytes=268435456
receive_timeout_ms=30000
expected_reassembled_frames=1800
expected_reassembled_clients=2
expected_reassembled_frames_per_client=900
preview_frames=180
max_requests=360

[what happened]
pass_or_fail=
same_pc_cpu_note=
operator_note=

[server receive summary]
registered_clients=
frames_reassembled=
frames_queued=
direct_frames_queued=
keyframes_received=
keyframes_queued=
rejected_packets=
incomplete_reassembly_frames=
manual_expected_reassembled_clients=
manual_expected_reassembled_frames_per_client=
observed_queued_clients=
observed_reassembled_clients=
per_client_queued_frames=
per_client_keyframes_queued=
per_client_direct_frames=
per_client_reassembled_frames=
retained_keyframe_clients=
per_client_retained_keyframe_frame_id=
first_keyframe_frame_id=
last_keyframe_frame_id=
validation_ready=
ready_reason=
receive_stop_reason=
stop_reason=

[client summary]
encoder_gop_frames=
frames_sent=
h264_idr_count=
h264_non_idr_vcl_count=
keyframes_encoded=
keyframes_sent=
first_keyframe_frame_id=
last_keyframe_frame_id=
h264_parameter_sets_cached=
h264_sps_count=
h264_pps_count=
h264_parameter_sets_prepended_count=
h264_parameter_sets_missing_count=
last_payload_has_sps=
last_payload_has_pps=
last_payload_has_idr=
last_payload_has_non_idr_vcl=

[server handoff request lines]
request_1=
request_2=
request_3=
request_4=

[switcher preview summary]
frames_attempted=
frames_rendered=
render_failures=
scheduler_status=
slot_bindings=
slot_result_kinds=
slot_diagnostics=
clean_output_render_result_kind=
window_title=
output_width=
output_height=

[optional raw handoff read player1]
final_result=
handoff_response_kind=
parse_error=
io_error=
queue_len=
frame_id=
encoded_payload_len=

[optional raw handoff read player2]
final_result=
handoff_response_kind=
parse_error=
io_error=
queue_len=
frame_id=
encoded_payload_len=
```

## Next Step After A PASS

After this 2-client same-PC handoff pass, move to one of:

1. 2-client PASS checkpoint closeout:
   record the PASS, keep staged-preview limits explicit, and preserve the known
   fps variance note
2. concurrent receive + handoff serve design:
   replace the staged post-receive handoff service with a real concurrent
   runtime plan before calling the path production-like
3. OBS capture validation follow-up:
   re-confirm downstream output-window capture expectations after the preview
   path is treated as a stable checkpoint
4. 4-client all-real validation preparation:
   only after the 2-client PASS checkpoint and known limits are documented
