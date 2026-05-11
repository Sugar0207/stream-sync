<!-- stream-sync/docs/operations/concurrent-handoff-runtime-plan.md -->

# Concurrent Handoff Runtime Plan

## 目的

2-client real handoff preview validation は staged checkpoint として PASS
したが、current server runtime は以下の順序で動く。

1. UDP receive / auth / reassembly / queue
2. expected frames 到達
3. handoff pipe ready
4. switcher read

これは bounded validation としては十分だが、realtime preview /
production-like operation としては不十分である。

この doc は、次段の concurrent receive + handoff serve runtime を
docs-only で整理するための source of truth とする。

## 前提

- existing staged command
  `--receive-auth-video-queue-and-serve-handoff-many` は残す
- first concurrent slice は 2-client same-PC のみを対象にする
- first concurrent slice では `preview-latest-decodable` と retained
  keyframe fallback を前提にする
- no reconnect
- no daemon/service polish
- no 4-client
- no OBS-specific work
- no switcher persistent decoder context

## 目指す runtime shape

起動直後に server が以下を並行に持つ。

1. UDP receive/auth/reassembly/queue update runtime
2. named-pipe handoff serve runtime
3. shared queue/retained state
4. shared runtime summary/counters

Expected operator flow:

1. server start
2. `receive_ready=true`
3. `handoff_ready=true`
4. switcher start
5. client1/client2 start
6. client sending 中に switcher が `latest` / `latest-decodable` frame を読む

## Command Candidate

Recommended primary candidate:

- `--receive-auth-video-queue-and-serve-handoff-continuous`

Alternative shorter candidate:

- `--receive-send-handoff-runtime-continuous`

Naming judgment:

- keep the staged command name family visible
- prefer the longer explicit name for the first implementation because it makes
  auth/video-queue/handoff ownership obvious
- keep `continuous` explicit so it is not confused with staged / bounded
  receive-then-serve commands

Recommended initial CLI family:

- staged:
  - `--receive-auth-video-queue-and-serve-handoff-many`
- concurrent:
  - `--receive-auth-video-queue-and-serve-handoff-continuous`

## Responsibility Split

### Receive side

Owns:

- UDP bind / receive loop
- auth decision and registry updates
- `VideoFrameFragment` reassembly
- queue insertion
- retained keyframe update
- receive-side counters

Does not own:

- switcher targetTime scheduling
- decode progression
- display policy

### Handoff side

Owns:

- named-pipe listener lifecycle
- request decode / response encode
- queue read by `client_id + run_id + read_mode`
- handoff-side counters

Does not own:

- frame mutation beyond current read semantics
- switcher retry manager
- decode/display state

### Shared queue state

Owns:

- per `client_id + run_id` queued encoded frames
- retained latest keyframe per `client_id + run_id`
- per-client receive counters needed by both receive/handoff summaries

### Runtime coordination

Owns:

- startup readiness
- stop request propagation
- aggregate summary output
- shutdown ordering

## Shared State Plan

### State owned by the concurrent runtime

- authenticated sender registry
- `ServerVideoFrameQueueState`
- retained keyframe state
- receive-side aggregate counters
- handoff-side aggregate counters
- runtime lifecycle state

### Minimum shared data to expose

- queue per `client_id + run_id`
- retained keyframe per `client_id + run_id`
- per-client queued frame counts
- per-client keyframe counts
- receive totals
- handoff request totals

### Lock granularity

Recommended first slice:

- one runtime-owned shared state object
- one coarse lock around queue + retained-keyframe + related counters

Reason:

- first slice is same-PC, 2-client, design target is correctness and
  observability rather than peak throughput
- queue read and queue write both already operate on one logical queue state
- finer-grained locks add complexity before real contention evidence exists

Deferred lock refinement candidates:

- split queue/retained-frame state from pure counters
- per-client queue partitions
- read-mostly snapshots for summary output

### Snapshot / clone policy

- do not clone encoded payloads for periodic summary output
- summary output should use counters and lightweight metadata snapshots only
- handoff `FrameRead` may still clone payload bytes into the response as today
- queue diagnostics should prefer frame ids / counts / timestamps over payload
  duplication

### Read path policy

- handoff request takes lock
- perform one queue read decision
- clone only the selected frame payload needed for that response
- release lock before blocking pipe write when possible in the implementation

### Write path policy

- receive loop takes lock
- apply one accepted frame insertion
- update retained keyframe if `is_keyframe=true`
- update receive counters
- release lock immediately after state mutation

## Readiness Semantics

Recommended explicit readiness flags:

- `receive_ready=true|false`
- `handoff_ready=true|false`
- `validation_ready=true|false|n/a`

Meaning:

- `receive_ready=true`
  - UDP socket bind and receive loop startup completed
- `handoff_ready=true`
  - named-pipe listener is ready to accept switcher requests
- `validation_ready=true`
  - bounded expected validation condition reached

Continuous runtime note:

- `validation_ready` is optional in continuous mode
- first continuous implementation can expose it as:
  - `validation_ready=n/a` when no expected frame/client thresholds are
    configured
  - `validation_ready=true|false` only when optional bounded thresholds are
    supplied for manual same-PC validation

Recommended first-slice behavior:

- always emit `receive_ready`
- always emit `handoff_ready`
- allow `validation_ready` to be absent or `n/a` by default

## Summary Field Plan

### Receive side

- `packets_received`
- `frames_queued`
- `per_client_queued_frames`
- `keyframes_queued`
- `retained_keyframe_clients`
- `per_client_retained_keyframe_frame_id`
- `observed_queued_clients`
- `observed_reassembled_clients`

### Handoff side

- `handoff_requests`
- `frame_read_count`
- `no_frame_count`
- `decodable_source_queue_count`
- `decodable_source_retained_keyframe_count`
- `decodable_source_none_count`
- `io_error_count`
- `parse_error_count`

### Runtime side

- `stop_reason`
- `receive_stop_reason`
- `handoff_stop_reason`
- `runtime_duration_ms`
- `receive_ready`
- `handoff_ready`
- `validation_ready`

## Stop / Shutdown Coordination

Recommended first-slice stop model:

- one runtime-owned stop flag
- receive loop and handoff serve loop both observe it
- any fatal startup error fails the whole runtime
- normal shutdown is explicit and small:
  - manual stop request
  - configured max runtime / max requests if provided
  - fatal receive error
  - fatal handoff listener error

Deferred:

- reconnect/backoff
- daemon manager
- Ctrl+C polish
- automatic restart
- multi-process supervisor integration

## MVP Slice

Smallest useful concurrent slice:

1. one new concurrent server command
2. same-PC only
3. 2-client target
4. `preview-latest-decodable` only
5. retained-keyframe fallback allowed and expected
6. no reconnect
7. no 4-client
8. no OBS work
9. no persistent decoder context
10. summary-first observability

What this MVP should prove:

- switcher can connect before clients finish sending
- switcher can read during active receive
- shared queue/retained state stays coherent
- `FrameRead` / `NoFrame` / `HandoffError` stay explicit

What this MVP should not try to prove:

- full realtime latest non-IDR decode progression
- production lifecycle polish
- multi-client load beyond current 2-client same-PC checkpoint

## Human Validation Procedure

Recommended first manual procedure:

1. start server concurrent runtime
2. confirm `receive_ready=true`
3. confirm `handoff_ready=true`
4. start switcher preview loop
5. start client1
6. start client2
7. confirm switcher can report `FrameRead` while clients are still sending
8. confirm `frames_rendered > 0`
9. confirm `render_failures=0`
10. confirm real slots can return `decodable_source=queue` or
    `decodable_source=retained_keyframe`

Suggested same-PC evidence to capture:

- server startup readiness line
- one mid-run server runtime summary snapshot if available
- switcher summary while clients are still active
- final server runtime summary
- final client summaries

## Known Issues And Deferred Items

Keep these explicit in the plan:

- switcher persistent decoder context is still unimplemented
- retained-keyframe preview is not the same as continuous latest non-IDR decode
- same-PC client fps variance remains a known issue
- concurrent receive + handoff serve is design-only in this step
- staged command remains necessary as a stable validation baseline

## Next Implementation Order

Recommended order after this design step:

1. add concurrent runtime boundary and command without replacing staged command
2. emit readiness lines early
3. share queue/retained state with coarse locking
4. add runtime aggregate summary counters
5. run 2-client same-PC human validation
6. only then consider 4-client preparation or lifecycle polish
