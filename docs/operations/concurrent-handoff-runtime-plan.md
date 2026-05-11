<!-- stream-sync/docs/operations/concurrent-handoff-runtime-plan.md -->

# Concurrent Handoff Runtime Plan

## Status
- 2-client same-PC staged handoff preview checkpoint is PASS.
- The first concurrent runtime slice is now implemented.
- 2026-05-12 automated validation sweep is PASS:
  - `cargo fmt`
  - `cargo fmt --check`
  - `cargo check --workspace`
  - focused concurrent server tests
  - focused staged handoff regression tests
  - `cargo test --workspace`
  - `git diff --check`
- Existing staged command remains valid:
  - `--receive-auth-video-queue-and-serve-handoff-many`
- New concurrent command is available:
  - `--receive-auth-video-queue-and-serve-handoff-continuous`
- Next gate remains the same-PC human rerun for the concurrent command.

## Goal
Move from the staged lifecycle:

```text
receive/auth/reassembly/queue
-> expected frames reached
-> handoff pipe ready
-> switcher read
```

to a minimal concurrent lifecycle:

```text
receive/auth/reassembly/queue update
|| handoff pipe serve
```

The first goal is narrow:

- same-PC only
- 2 real clients only
- `preview-latest-decodable` only
- retained-keyframe fallback allowed
- no reconnect
- no daemon/service polish
- no 4-client
- no OBS-specific work
- no switcher persistent decoder context

## Implemented Command

```text
stream-sync-server --receive-auth-video-queue-and-serve-handoff-continuous
  [config-path]
  [pipe-name]
  [max-handoff-requests-or-0-for-unbounded]
  [receive-timeout-ms]
  [max-runtime-duration-ms-or-0-for-unbounded]
  [max-video-packets-or-0-for-unbounded]
  [expected-reassembled-frames]
  [stop-after-expected-reassembled-frames]
  [receive-buffer-bytes]
  [expected-reassembled-clients]
  [expected-reassembled-frames-per-client]
```

Current recommended first validation shape:

- `pipe-name=streamsync-handoff-dev`
- `max-handoff-requests=2000`
- `receive-timeout-ms=30000`
- `max-runtime-duration-ms=180000`
- `max-video-packets=0`
- `expected-reassembled-frames=0`
- `stop-after-expected-reassembled-frames=false`
- `receive-buffer-bytes=268435456`
- `expected-reassembled-clients=0`
- `expected-reassembled-frames-per-client=0`

## Responsibility Split

### Receive side
- UDP bind / receive loop
- auth decision and registry updates
- fragment reassembly
- bounded queue insertion
- retained keyframe update
- receive-side counters

### Handoff side
- named-pipe accept / request decode / response encode
- queue read by `client_id + run_id + read_mode`
- handoff-side counters

### Shared state
- authenticated sender registry
- `ServerVideoFrameQueueState`
- retained keyframe state
- `ServerVideoFrameReassemblyState`
- receive summary counters
- handoff summary counters

### Runtime coordination
- early ready line
- stop request propagation
- aggregate stopped summary

## Shared State Policy
- First slice uses one coarse lock around queue state, retained keyframe state, reassembly state, registry, and closely related counters.
- Handoff reads lock only around queue access and counter updates.
- Pipe read / write stays outside the queue lock.
- Summary output uses counters and metadata snapshots, not payload duplication.

## Readiness Semantics

Current concurrent ready line exposes:

- `receive_ready=true`
- `handoff_ready=true`
- `runtime_mode=concurrent`
- `validation_ready=n/a`
- `expected_reassembled_frames_enabled=true|false`
- `expected_clients_enabled=true|false`
- `expected_per_client_frames_enabled=true|false`
- `pipe_name=...`
- `actual_pipe_path=...`

Meaning:

- `receive_ready=true`
  - UDP socket bind and receive loop thread startup completed
- `handoff_ready=true`
  - server is entering the named-pipe accept loop and switcher may connect
- `validation_ready=n/a`
  - concurrent mode is not using the staged bounded validation gate by default
- `expected_*_enabled`
  - `0` means disabled for the corresponding expected threshold
  - current same-PC continuous validation should therefore show:
    - `expected_reassembled_frames_enabled=false`
    - `expected_clients_enabled=false`
    - `expected_per_client_frames_enabled=false`

## Summary Fields

### Receive side
- `packets_received`
- `frames_queued`
- `per_client_queued_frames`
- `keyframes_queued`
- `retained_keyframe_clients`
- `per_client_retained_keyframe_frame_id`

### Handoff side
- `handoff_requests`
- `frame_read_count`
- `no_frame_count`
- `decodable_source_counts`
- `io_error_count`

### Runtime side
- `stop_reason`
- `receive_stop_reason`
- `handoff_stop_reason`
- `runtime_duration_ms`
- `expected_reassembled_frames_enabled`
- `expected_clients_enabled`
- `expected_per_client_frames_enabled`

## Stop Conditions

First-slice concurrent runtime currently supports bounded shutdown by:

- receive timeout
- max runtime duration
- max handoff requests
- max received video packets
- optional expected reassembled frame thresholds

Expected-threshold semantics for the concurrent path:

- `expected_reassembled_frames=0`
  - disabled
  - must not produce `receive_stop_reason=ReassembledFramesThresholdReached`
- `expected_reassembled_clients=0`
  - disabled
  - must not be treated as immediately satisfied
- `expected_reassembled_frames_per_client=0`
  - disabled
  - must not be treated as immediately satisfied
- with the current validation command
  - `validation_ready` stays `n/a`
  - receive closeout should primarily come from:
    - `receive_timeout`
    - `max_runtime_duration`
    - `max_video_packets`
    - explicit stop / handoff shutdown coordination

Known current caveat:

- if receive side stops first, overall process still depends on the handoff loop
  reaching its own stop point
- this is acceptable for the first same-PC manual slice because the switcher
  loop is already bounded by frame count / request budget
- `max_handoff_requests` is a total request safety limit, not the primary human
  validation closeout mechanism
- current switcher preview can legitimately issue many early `NoFrame` requests
  before clients start sending
- for human validation, `max_runtime_duration_ms` should be treated as the main
  closeout bound and `max_handoff_requests` should be set high enough that
  startup `NoFrame` traffic does not end the server early

## Human Validation Order

1. Start the concurrent server runtime.
2. Confirm stdout includes:
   - `receive_ready=true`
   - `handoff_ready=true`
   - `runtime_mode=concurrent`
   - `expected_reassembled_frames_enabled=false`
   - `expected_clients_enabled=false`
   - `expected_per_client_frames_enabled=false`
3. Start the switcher preview loop with `preview-latest-decodable`.
4. Start client1.
5. Start client2.
6. Confirm the switcher reaches:
   - `FrameRead` on the two real slots, or
   - `frames_rendered > 0`
7. Confirm the final server stopped summary includes:
   - `handoff_requests > 0`
   - `packets_received > 1`
   - `frame_read_count > 0`
   - `retained_keyframe_clients >= 1`
   - `expected_reassembled_frames_enabled=false`
   - `expected_clients_enabled=false`
   - `expected_per_client_frames_enabled=false`
   - `receive_stop_reason` is not `ReassembledFramesThresholdReached`
8. Treat this as a known early-failure shape rather than a transport failure:
   - `stop_reason=MaxHandoffRequestsReached`
   - `packets_received=0`
   - `frame_read_count=0`
   - `no_frame_count=max_handoff_requests`

## First Validation Commands

Server:

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-continuous configs/manual/server.two-real-slots.toml streamsync-handoff-dev 2000 30000 180000 0 0 false 268435456 0 0
```

Switcher:

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 180 preview-latest-decodable
```

Client1:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

Client2:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

## Success Gate
- server ready line is printed before clients start sending
- switcher can connect after server start, before client traffic finishes
- final server summary shows `frame_read_count > 0`
- real slots reach `FrameRead`
- `frames_rendered > 0`
- `render_failures=0`
- concurrent server stopped summary shows handoff traffic was actually served
- staged command regressions remain green

## Known Limits
- current preview path is still retained-keyframe-friendly, not a full latest
  non-IDR continuous decode path
- switcher persistent decoder context is still out of scope
- same-PC client FPS variance remains a known issue
- early startup `NoFrame` traffic can exhaust a too-small `max_handoff_requests`
  budget before client frames arrive
- current human validation should therefore prefer a larger request safety limit
  such as `2000` and use `max_runtime_duration_ms` as the main closeout bound
- reconnect / daemon lifecycle polish remains deferred
- 4-client and OBS validation remain later phases
