<!-- stream-sync/docs/operations/four-client-validation.md -->

# 4-client All-Real Validation Preparation

## Status
- This document is the docs-first source of truth for the next phase after the
  2-client same-PC concurrent validation PASS at:
  - `manual-logs/handoff-20260513-134658`
- The 2-client concurrent PASS checkpoint is closed.
- Latest same-PC 4-client all-real human run has been recorded from:
  - `manual-logs/four-client-20260513-184503`
- Latest 4-client result is PASS under the final-state-based criterion:
  - server ready / stopped summary passed
  - client auth / send passed
  - server receive / queue participation passed
  - named-pipe handoff transport passed
  - switcher final state reached `AllSelected`
  - `preview_mode=preview-latest-decodable`
  - `read_mode=inspect-latest-decodable`
  - `clean_output_render_result_kind=Rendered`
- The latest result proves the server/client/handoff transport path:
  - server ready line emitted
  - server stopped summary emitted
  - all 4 clients authenticated and sent `900` frames
  - server queued `3600` frames, `900` per client
  - server retained keyframes for all 4 client/run scopes
  - switcher final real slots reached `Selected`
  - switcher final diagnostics had `parse_error=none` and `io_error=none`
  - retained-keyframe fallback was used in the final real-slot path
- Same-PC saturation is still a known follow-up:
  - client effective output FPS landed in the `19-20fps` band
- The remaining follow-up target is:
  - distributed-PC validation
  - OBS capture follow-up
  - same-PC performance follow-up

## Goal

Validate that the current concurrent runtime can sustain the full MVP fixed
4-view shape on one Windows PC:

```text
server receive/auth/reassembly/queue update
|| named-pipe handoff serve
-> switcher 4-slot all-real read/select/decode/render
```

This phase is not a generic scale-out exercise. The goal is narrow:

- prove all `4` real slots can participate in one concurrent runtime
- keep the target fixed to the current MVP `4`-view shape
- keep the PASS gate focused on final all-real handoff/render state
- treat same-PC `4`-client execution as a stress validation because capture,
  encode, receive, decode, and render all share one host

## Reused From The 2-client PASS Checkpoint

The following conditions carry forward without reopening the 2-client PASS:

- same server config:
  - `configs/manual/server.two-real-slots.toml`
  - despite the filename, this config already authorizes
    `player1..player4` and keeps `max_clients=4`
- same run scope:
  - `run_id=streamsync-dev-session`
  - `pipe_name=streamsync-handoff-dev`
- same client runtime profile:
  - `max_frames=900`
  - `fragment_pacing_every=4`
  - `fragment_pacing_delay_ms=2`
  - `--encoder-runtime persistent`
  - `--cadence-mode deadline`
- same server receive profile:
  - `receive_buffer_bytes=268435456`
  - disabled expected thresholds:
    - `expected_reassembled_frames=0`
    - `expected_reassembled_clients=0`
    - `expected_reassembled_frames_per_client=0`
- same concurrent ready-line expectations:
  - `receive_ready=true`
  - `handoff_ready=true`
  - `runtime_mode=concurrent`
  - `validation_ready=n/a`
  - `expected_reassembled_frames_enabled=false`
  - `expected_clients_enabled=false`
  - `expected_per_client_frames_enabled=false`
- same final-state-based interpretation:
  - `frames_rendered` is observability, not an equality gate against
    `frames_attempted`
  - final handoff/read/render state is the primary PASS surface
- same downstream render/output surface:
  - `window_title=StreamSync 4-view Output`
  - `output_width=1280`
  - `output_height=720`

## What Is New In The 4-client Phase

Compared with the 2-client PASS, this phase must newly confirm:

- all `4` authenticated client scopes participate in one concurrent run
- server final summary shows queue participation for `player1..player4`
- switcher final summary reaches all-real `AllSelected`, not 2-real
  `PartialSelected`
- no slot is allowed to finish as placeholder-only state:
  - `NoFrameAvailable`
  - `WaitingForFrameAtOrBeforeTarget`
  - `HandoffError`
- same-PC load does not hide a per-client starvation problem
- startup timing and request budget remain sufficient when one preview tick now
  touches `4` real slots instead of `2`

## Command Shape

### Server

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-continuous configs/manual/server.two-real-slots.toml streamsync-handoff-dev 4000 120000 360000 0 0 false 268435456 0 0
```

Chosen shape:

- `max_handoff_requests=4000`
  - gives headroom above the nominal `180 frames * 4 real slots = 720`
    request count
  - avoids repeating the earlier startup `NoFrame` request-budget failure shape
- `receive_timeout_ms=120000`
  - matches the latest 2-client PASS checkpoint
- `max_runtime_duration_ms=360000`
  - intentionally longer than the 2-client checkpoint because same-PC
    `4`-client execution is expected to be heavier
- all expected-threshold arguments remain disabled in this phase
  - the pass/fail gate comes from final state, not from staged bounded receive

### Switcher

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 180 preview-latest-decodable
```

Chosen shape:

- fixed all-real slot order:
  - slot `0` -> `player1`
  - slot `1` -> `player2`
  - slot `2` -> `player3`
  - slot `3` -> `player4`
- `frames=180`
  - preserves the same preview-window length used by the latest 2-client PASS
- current command now accepts an optional preview-mode override:
  - `preview-oldest`
  - `preview-latest`
  - `preview-latest-decodable`
- recommended rerun mode is:
  - `preview-latest-decodable`
  - this maps to switcher targetTime mode
    `PreviewLatestDecodableIfAtOrBefore`
  - this maps to named-pipe handoff read mode `InspectLatestDecodable`
  - that allows the same retained-keyframe fallback used by the 2-client PASS
- omitted preview mode remains backward-compatible:
  - default is still `preview-latest`
- current command now recomputes `real_four_view_preview_target_timestamp()`
  on each preview tick through the same target timestamp hook shape used by the
  2-real preview loop

### Clients

Client 1:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

Client 2:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

Client 3:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

Client 4:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

## Same-PC First Rule

This phase is explicitly same-PC first.

Interpretation:

- same-PC `4`-client execution is the higher-priority next check because it is
  the narrowest path from the current 2-client PASS checkpoint
- treat the run as stress, not as the distributed production topology proof
- do not mix in distributed-PC variables yet:
  - server IP changes
  - firewall triage
  - cross-host capture latency
  - cross-host clock/environment drift

Distributed-PC validation can be considered only after the same-PC 4-client
result is recorded with this document's classification.

## Manual Validation Order

### Preflight

1. Confirm these files exist:
   - `configs/manual/server.two-real-slots.toml`
   - `configs/manual/client.player1.toml`
   - `configs/manual/client.player2.toml`
   - `configs/manual/client.player3.toml`
   - `configs/manual/client.player4.toml`
2. Confirm all client configs still use:
   - `run_id=streamsync-dev-session`
   - `server_host=127.0.0.1`
   - `server_port=5000`
3. Confirm server config still authorizes:
   - `player1`
   - `player2`
   - `player3`
   - `player4`
4. Build before the run:

```powershell
cargo build -p stream-sync-server -p stream-sync-switcher -p stream-sync-client
```

### Startup Order

1. Start the concurrent server command.
2. Wait for the ready line and confirm:
   - `receive_ready=true`
   - `handoff_ready=true`
   - `runtime_mode=concurrent`
   - `validation_ready=n/a`
   - `expected_reassembled_frames_enabled=false`
   - `expected_clients_enabled=false`
   - `expected_per_client_frames_enabled=false`
3. Start the switcher all-real preview loop.
4. Immediately start clients in this fixed order with minimal manual delay:
   - client1
   - client2
   - client3
   - client4
5. Let the switcher exit on its own bounded frame count.
6. Wait for the final client summaries.
7. Wait for the final server stopped summary.

Operator rule:

- do not insert long pauses between switcher start and client starts
- if a rerun is needed, first adjust operator timing or runtime bounds before
  proposing retry/backoff or decoder-context work

## Success Criterion

Treat the run as PASS only when all of the following are true.

### Client Gate

All `4` clients must show:

- `accepted=true`
- `frames_encoded=900`
- `frames_sent=900`
- `send_failures=0`
- `keyframes_sent=30`
- `h264_parameter_sets_cached=true`
- `stop_reason=Some(MaxFramesReached)`

### Server Ready Gate

The server ready line must show:

- `receive_ready=true`
- `handoff_ready=true`
- `runtime_mode=concurrent`
- `validation_ready=n/a`
- `expected_reassembled_frames_enabled=false`
- `expected_clients_enabled=false`
- `expected_per_client_frames_enabled=false`

### Server Final Gate

The final concurrent stopped summary must be emitted and must show:

- `stop_reason=ReceiveStopped`
- `receive_stop_reason=ReceiveTimedOut`
- `handoff_stop_reason=StopRequested`
- `packets_received > 4`
- `frames_queued=3600`
- `per_client_queued_frames` includes:
  - `player1/streamsync-dev-session:900`
  - `player2/streamsync-dev-session:900`
  - `player3/streamsync-dev-session:900`
  - `player4/streamsync-dev-session:900`
- `retained_keyframe_clients=4`
- `frame_read_count > 0`
- `io_error_count=0`

### Switcher Final Gate

The final switcher summary must show:

- `command_name=--four-view-four-real-handoff-preview-loop`
- `real_slot_count=4`
- `scheduler_status=AllSelected`
- `slot_result_kinds=Selected|Selected|Selected|Selected`
- `render_failures=0`
- `clean_output_render_result_kind=Rendered`
- `window_title=StreamSync 4-view Output`
- `output_width=1280`
- `output_height=720`
- `frames_rendered > 0`

Final slot diagnostics for all `4` real slots must show:

- `handoff_response_kind=FrameRead`
- `parse_error=none`
- `io_error=none`
- `decode_error=none`
- `final_slot_result_kind=Selected`

Interpretation:

- `frames_rendered=frames_attempted` is stronger evidence, not a required gate
- unlike the 2-real command, this all-real path has no placeholder slots
  inside the intended MVP view, so final `NoFrameAvailable` / `Waiting` on any
  real slot is a failure

## Failure Classification

Classify a failed or partial run into one primary bucket before proposing any
code change.

### 1. Preflight / Config Failure

Use this when:

- a required config file is missing
- `run_id` does not match across clients/switcher/server
- wrong pipe name is used
- wrong binary/command shape is launched

### 2. Startup / Ready Failure

Use this when:

- server never emits the concurrent ready line
- `runtime_mode!=concurrent`
- any `expected_*_enabled` field is unexpectedly `true`
- switcher starts against a server that is not yet handoff-ready

### 3. Client Auth / Send Failure

Use this when any client shows:

- `accepted=false`
- `frames_sent < 900`
- `send_failures > 0`
- `h264_parameter_sets_cached=false`

### 4. Server Receive / Queue Participation Failure

Use this when:

- final server summary is missing
- `frames_queued < 3600`
- `per_client_queued_frames` is missing any of the `4` client scopes
- `retained_keyframe_clients < 4`
- `io_error_count > 0`

### 5. Handoff Transport / Runtime Failure

Use this when any real slot or server summary shows:

- `handoff_response_kind=HandoffError`
- `parse_error!=none`
- `io_error!=none`
- final switcher state indicates transport/runtime failure instead of empty or
  late-frame behavior

### 6. Switcher Selection / Decode / Render Failure

Use this when:

- `scheduler_status!=AllSelected`
- any real slot ends as:
  - `NoFrameAvailable`
  - `WaitingForFrameAtOrBeforeTarget`
  - `HandoffError`
- `decode_error!=none`
- `render_failures>0`
- `clean_output_render_result_kind!=Rendered`

### 7. Same-PC Saturation Failure

Use this when:

- all commands are correct but same-PC load prevents a clean final state
- symptoms show timing/resource pressure rather than config mismatch, such as:
  - severe client FPS collapse
  - long startup gaps causing request-budget waste
  - `receive_stop_reason=MaxRuntimeDurationReached`
  - partial queue participation under otherwise correct auth

Response rule:

- treat this as a runtime/stress classification first
- do not jump directly to retry/backoff, persistent decoder context, or
  distributed-PC work

## Latest Same-PC 4-client Result

Latest human run:

- `manual-logs/four-client-20260513-184503`

### Judgment

Result:

- PASS

Reason:

- client auth/send gate passed for `player1..player4`
- server ready gate passed
- server receive/queue participation gate passed
- handoff transport/runtime gate passed
- switcher final all-real state reached `AllSelected`
- `preview-latest-decodable` allowed retained-keyframe fallback to keep all 4
  real slots selected
- same-PC saturation was observed, but it did not block the final-state PASS

### Evidence

Server:

- `receive_ready=true`
- `handoff_ready=true`
- `runtime_mode=concurrent`
- `receive_timeout_ms=300000`
- `max_runtime_duration_ms=600000`
- `stop_reason=ReceiveStopped`
- `receive_stop_reason=ReceiveTimedOut`
- `handoff_stop_reason=StopRequested`
- `runtime_duration_ms=354363`
- `packets_received=73657`
- `frames_queued=3600`
- `per_client_queued_frames`:
  - `player1/streamsync-dev-session:900`
  - `player2/streamsync-dev-session:900`
  - `player3/streamsync-dev-session:900`
  - `player4/streamsync-dev-session:900`
- `keyframes_queued=120`
- `retained_keyframe_clients=4`
- `frame_read_count=526`
- `no_frame_count=178`
- `decodable_source_counts=queue:11|retained_keyframe:515|none:178`
- `io_error_count=0`

Clients:

- `player1..player4` all show:
  - `accepted=true`
  - `frames_encoded=900`
  - `frames_sent=900`
  - `send_failures=0`
  - `keyframes_sent=30`
  - `h264_parameter_sets_cached=true`
  - `stop_reason=Some(MaxFramesReached)`
- same-PC saturation is visible:
  - `effective_output_fps=19.732|20.201|20.299|20.040`

Switcher:

- command:
  - `--four-view-four-real-handoff-preview-loop`
- `preview_mode=preview-latest-decodable`
- `read_mode=inspect-latest-decodable`
- `frames_attempted=180`
- `frames_rendered=137`
- `render_failures=0`
- `scheduler_status=AllSelected`
- `slot_result_kinds=Selected|Selected|Selected|Selected`
- final real-slot diagnostics for all 4 slots show:
  - `handoff_response_kind=FrameRead`
  - `parse_error=none`
  - `io_error=none`
  - `decodable_source=retained_keyframe`
  - `target_selection_result=Selected`
  - `decode_error=none`
  - `renderable_frame_available=true`
  - `final_slot_result_kind=Selected`
- `clean_output_render_result_kind=Rendered`
- `output_width=1280`
- `output_height=720`

### Code Review

Current 2-real preview path:

- CLI:
  - `--four-view-two-real-handoff-preview-loop ... [preview-oldest|preview-latest|preview-latest-decodable]`
- latest PASS used:
  - `preview-latest-decodable`
- `preview-latest-decodable` maps to:
  - switcher targetTime mode:
    `PreviewLatestDecodableIfAtOrBefore`
  - handoff queue read mode:
    `InspectLatestDecodable`
- server `InspectLatestDecodable` behavior:
  - select the latest queued keyframe for the client/run if available
  - otherwise fall back to the retained keyframe for that client/run
  - otherwise return no decodable frame
- the 2-real loop recomputes targetTime on each preview tick.

4-real preview path at the time of the latest human run on 2026-05-13:

- CLI:
  - `--four-view-four-real-handoff-preview-loop`
- no preview-mode argument is parsed
- loop calls the 4-view validation with:
  - `PreviewLatestIfAtOrBefore`
- `PreviewLatestIfAtOrBefore` maps to:
  - handoff queue read mode:
    `InspectLatest`
- `InspectLatest` returns the latest queued frame regardless of keyframe /
  one-shot decodability
- `InspectLatest` reports `decodable_source=none`
- `InspectLatest` can still report `retained_keyframe_available=true`, but it
  does not use the retained keyframe as the selected response frame
- the 4-real loop computes `real_four_view_preview_target_timestamp()` once
  before the loop and reuses it for all `180` ticks.

Current 4-real preview path after the 2026-05-13 parity implementation:

- CLI:
  - `--four-view-four-real-handoff-preview-loop ... [preview-oldest|preview-latest|preview-latest-decodable]`
- omitted preview mode keeps:
  - `preview-latest`
  - `PreviewLatestIfAtOrBefore`
  - `InspectLatest`
- `preview-latest-decodable` now maps to:
  - `PreviewLatestDecodableIfAtOrBefore`
  - `InspectLatestDecodable`
- the 4-real loop now recomputes targetTime per preview tick.

`WaitingForFrameAtOrBeforeTarget` condition:

- switcher first receives a frame from handoff
- switcher applies targetTime selection
- if the adjusted candidate capture timestamp is newer than targetTime, the
  slot result becomes `WaitingForFrameAtOrBeforeTarget`
- decode is intentionally skipped for that slot
- render input becomes a no-display placeholder
- if every real slot is waiting, scheduler status becomes `Waiting`
- if no slot is renderable, clean output becomes `NoRenderableQuadView`

This exactly matches the latest final diagnostics:

- all 4 handoff responses were `FrameRead`
- no parse or IO error occurred
- all 4 selected-frame results were unavailable because targetTime rejected the
  candidates as too new
- all 4 decodes were skipped before H.264 decode
- the clean output had no renderable quad view

### Interpretation

The strongest current interpretation is:

- primary gate is now PASS, not transport failure
- `preview-latest-decodable` exercised the retained-keyframe fallback path
- final-state success is governed by the selected real slots and clean output,
  not by `frames_rendered == frames_attempted`
- `frames_rendered=137/180` is completion-count observability, not a blocker
- same-PC saturation remains a follow-up because the client FPS landed in the
  `19-20fps` band

Therefore the current next step is follow-up work on distributed-PC
validation, OBS capture behavior, and same-PC performance, not another 4-client
rerun.

## Implemented Parity Slice

Completed on 2026-05-13:

- optional preview-mode argument added to
  `--four-view-four-real-handoff-preview-loop`:
  - `preview-oldest`
  - `preview-latest`
  - `preview-latest-decodable`
- backward compatibility kept:
  - omitted argument still means `preview-latest`
- selected mode is now wired through the 4-real loop in the same shape as the
  2-real loop:
  - targetTime mode via `preview_target_time_mode_from_switcher_mode`
  - handoff read mode via the existing
    `SwitcherSingleClientQueueSourceMode` mapping
- the 4-real loop now recomputes targetTime per tick, matching the 2-real
  `target_timestamp_hook` behavior
- final switcher summary now exposes:
  - `preview_mode`
  - `read_mode`

Still explicitly out of scope for this slice:

- retry/backoff manager
- persistent decoder context
- OBS WebSocket / advanced OBS control
- generic N-view refactor
- protocol wire-format changes

## Known Risks

- same-PC `4`-client load is materially heavier than the 2-client PASS:
  - `4` captures
  - `4` encoders
  - `4` auth/send streams
  - `4` real switcher decode/render paths
- startup `NoFrame` traffic scales with `4` real slots, so operator timing and
  request-budget headroom matter more than in the 2-client phase
- switcher persistent decoder context is still out of scope, so a future
  regression may still be rooted in one-shot decode limitations rather than
  transport failure
- same-PC success here does not prove distributed-PC behavior
- same-PC saturation is now a known follow-up because the pass run still landed
  in the `19-20fps` band on all four clients

## Not In Scope Yet

- distributed-PC validation
- OBS WebSocket / advanced OBS control
- retry/backoff manager
- switcher persistent decoder context
- generic N-view refactor
- protocol or architecture changes
- re-opening the 2-client concurrent PASS judgment
- further implementation changes before the latest PASS result is recorded in
  the repo docs

## Expected Next Step After This Preparation

1. Treat the latest same-PC 4-client all-real run as PASS and use it as the
   current validation checkpoint.
2. Move the next follow-up to distributed-PC validation, OBS capture
   verification, and same-PC performance tuning in that order.
