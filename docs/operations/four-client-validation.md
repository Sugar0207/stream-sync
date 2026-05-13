<!-- stream-sync/docs/operations/four-client-validation.md -->

# 4-client All-Real Validation Preparation

## Status
- This document is the docs-first source of truth for the next phase after the
  2-client same-PC concurrent validation PASS at:
  - `manual-logs/handoff-20260513-134658`
- The 2-client concurrent PASS checkpoint is closed.
- The next validation target is:
  - same-PC first
  - 4 real clients
  - concurrent server runtime
  - final-state-based PASS judgment
- Distributed-PC validation remains a later phase after the same-PC 4-client
  result is recorded.

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
.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 180
```

Chosen shape:

- fixed all-real slot order:
  - slot `0` -> `player1`
  - slot `1` -> `player2`
  - slot `2` -> `player3`
  - slot `3` -> `player4`
- `frames=180`
  - preserves the same preview-window length used by the latest 2-client PASS
- current command does not expose a preview-mode override
  - this phase therefore validates the existing fixed all-real preview path as
    implemented today
  - it does not widen scope into preview-mode redesign

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

## Known Risks

- same-PC `4`-client load is materially heavier than the 2-client PASS:
  - `4` captures
  - `4` encoders
  - `4` auth/send streams
  - `4` real switcher decode/render paths
- the all-real `4`-slot preview command does not expose the
  `preview-latest-decodable` override used in the 2-real concurrent PASS
- startup `NoFrame` traffic scales with `4` real slots, so operator timing and
  request-budget headroom matter more than in the 2-client phase
- switcher persistent decoder context is still out of scope, so a final-state
  failure may still be rooted in one-shot decode limitations rather than
  transport failure
- same-PC success here does not prove distributed-PC behavior

## Not In Scope Yet

- distributed-PC validation
- OBS WebSocket / advanced OBS control
- retry/backoff manager
- switcher persistent decoder context
- generic N-view refactor
- protocol or architecture changes
- re-opening the 2-client concurrent PASS judgment
- implementation changes for this phase before one run is classified

## Expected Next Step After This Preparation

1. Execute exactly this same-PC 4-client all-real concurrent recipe.
2. Record the full client/server/switcher stdout evidence.
3. Classify the result with the buckets above before deciding whether any code
   change is justified.
