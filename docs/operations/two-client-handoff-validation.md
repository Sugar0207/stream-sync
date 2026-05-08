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
```

What this switcher command owns:

- named-pipe handoff reads for the two real slots
- switcher-side targetTime selection
- `Selected` / `NoFrameAvailable` / `WaitingForFrameAtOrBeforeTarget` /
  `HandoffError` preservation
- decode / render into the existing clean output window path

Important current CLI note:

- use plain pipe name such as `streamsync-handoff-dev`
- do not pass full `\\.\pipe\...` path to current CLI args

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
- `rejected_packets`
- `incomplete_reassembly_frames`
- `manual_expected_reassembled_clients`
- `manual_expected_reassembled_frames_per_client`
- `observed_reassembled_clients`
- `per_client_reassembled_frames`
- `stop_reason`

### Server Bounded Handoff Request Lines

Primary handoff-side fields:

- `request_id`
- `result_kind`
- `selected_client_id`
- `selected_run_id`
- `frame_id`
- `frame_payload_len`
- `handoff_error`

For a good 2-client handoff run, both `player1` and `player2` should appear in
successful `FrameRead` request lines at least once.

### Switcher Raw One-Shot Read

Primary fields from `--read-queued-frame-handoff-once`:

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
- bounded handoff request budget matches the planned preview loop

### Window 2: Client 1

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 900 4 2
```

### Window 3: Client 2

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 900 4 2
```

### Window 4: Switcher Main Handoff Preview

Start this after both clients have been started and a few seconds of receive
time have passed.

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 180
```

Expected render surface:

- `window_title=StreamSync 4-view Output`
- `output_width=1280`
- `output_height=720`

## Optional Transport Isolation Rerun

Use this only if the main preview loop reports an unexpected real-slot
`HandoffError` or persistent `NoFrameAvailable`.

### Window 4a: Raw One-Shot Read For Player 1

```powershell
.\target\debug\stream-sync-switcher.exe --read-queued-frame-handoff-once streamsync-handoff-dev player1 streamsync-dev-session preview-latest 1
```

### Window 4b: Raw One-Shot Read For Player 2

```powershell
.\target\debug\stream-sync-switcher.exe --read-queued-frame-handoff-once streamsync-handoff-dev player2 streamsync-dev-session preview-latest 2
```

Expected success shape for the raw rerun:

- `final_result=FrameRead`
- `handoff_response_kind=FrameRead`
- `parse_error=none`
- `io_error=none`
- `encoded_payload_len > 0`

## Success Conditions

### Server Receive Side

- `registered_clients=2`
- `frames_reassembled=1800`
- `frames_queued=1800`
- `rejected_packets=0`
- `incomplete_reassembly_frames=0`
- `manual_expected_reassembled_clients=2`
- `manual_expected_reassembled_frames_per_client=900`
- `observed_reassembled_clients=2`
- `per_client_reassembled_frames` shows both:
  - `player1/streamsync-dev-session:900`
  - `player2/streamsync-dev-session:900`

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
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`
  - `final_slot_result_kind=Selected`
- slot `1` / `player2`
  - `handoff_response_kind=FrameRead`
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`
  - `final_slot_result_kind=Selected`

### Not A Failure By Itself

- `NoFrameAvailable` on slots `2` and `3`
- `scheduler_status=PartialSelected` in this exact 2-real + 2-placeholder path
- `stop_reason=ReceiveTimedOut` after expected counts were already reached

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
rejected_packets=
incomplete_reassembly_frames=
manual_expected_reassembled_clients=
manual_expected_reassembled_frames_per_client=
observed_reassembled_clients=
per_client_reassembled_frames=
stop_reason=

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

1. server -> switcher handoff follow-up only if a narrow observability gap
   still blocks interpretation
2. 4-client all-real validation preparation
