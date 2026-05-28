<!-- stream-sync/docs/operations/continuous-output-lag-plan.md -->

# Continuous Output Lag Plan

Last updated: 2026-05-28

## Purpose
- Analyze why slot0 continuous decoded output still trails the requested render frame after bounded feed helper and bounded-lag lookup wiring both reached runtime evidence.
- Keep the next implementation slice diagnostics-first and opt-in.
- Do not change allowed lag threshold, feed max count, lookup policy, FFmpeg defaults, server/client/protocol, slot1, or 4-client rollout.
- Define and track the smallest diagnostics slice for continuous output lag / pending correspondence / stdout read latency / decoded queue-drop policy.
- After the reverse-order threshold A/B, treat lag8 as HOLD / candidate and
  move the next main line to output availability / throughput.

## Latest Evidence
- latest optimized BGR24 A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-optimized-bgr24-ab-rerun-20260528-103130`
  - default BGRA:
    - output throughput `26.272fps`
    - completed latency avg/max/latest `1123.244ms` / `1349ms` / `1066ms`
    - pending count `35`
    - pending age avg/max `733.029ms` / `1361ms`
    - latest input-output gap `35`
    - output lag to selected `33`
    - bounded lookup hits `11`
  - optimized scaled BGR24:
    - output throughput `26.092fps`
    - completed latency avg/max/latest `1350.666ms` / `1659ms` / `1466ms`
    - pending count `44`
    - pending age avg/max `932.068ms` / `1653ms`
    - latest input-output gap `46`
    - output lag to selected `28`
    - bounded lookup hits `4`
    - conversion total/max/count `2105ms` / `8ms` / `389`
    - conversion average about `5.41ms/frame`
    - reuse/allocation `389` / `0`
  - lag verdict:
    - BGR24 conversion optimization is PASS
    - output lag to selected improved in optimized `scaled-bgr24`, but
      completed latency, pending age/count, output throughput, and bounded
      lookup hits still favor default BGRA
    - optimized `scaled-bgr24` adoption is HOLD
    - keep default BGRA and move the next candidate to FFmpeg scale path split
      or reader/completed latency breakdown diagnostics

- 2026-05-28 code status:
  - FFmpeg scale path split first slice is implemented as opt-in
    `no-scale-bgra`.
  - It is limited to slot0 / two-real / opt-in continuous mode.
  - The continuous FFmpeg scale filter is removed only for this mode; BGRA
    output remains source-size.
  - At 1280x720 source, stdout expected bytes/frame become `3686400`, so the
    mode is diagnostics-only and not a default/adoption candidate.
  - Summary exposes scale mode, source/scaled dimensions, scale removed count,
    and scale path experiment enabled flag for lag A/B interpretation.
  - No Codex runtime rerun was performed.

- latest output pipeline A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200`
  - default BGRA:
    - output throughput `25.816fps`
    - completed latency avg/max/latest `1309.796ms` / `1827ms` / `1591ms`
    - pending age avg/max `803.227ms` / `1646ms`
    - latest input-output gap `45`
    - output lag to selected `46`
    - bounded lookup hits `6`
  - scaled BGR24:
    - output throughput `22.150fps`
    - completed latency avg/max/latest `2037.903ms` / `3508ms` / `3508ms`
    - pending age avg/max `1709.438ms` / `3502ms`
    - latest input-output gap `106`
    - output lag to selected `88`
    - bounded lookup hits `3`
    - pixel conversion total/max/count `8636ms` / `41ms` / `329`
  - lag verdict:
    - `scaled-bgr24` reduces pipe bytes and reader time, but output lag and
      correspondence delay get worse after conversion cost
    - raw pipe bytes hypothesis is PARTIAL PASS
    - BGR24-to-BGRA conversion is a new strong bottleneck candidate
    - keep default BGRA; hold / fail `scaled-bgr24` adoption
    - detailed conversion/direct-render review now lives in
      `docs/operations/continuous-pixel-conversion-plan.md`
- latest completed correspondence rerun:
  - `S:\stream-sync\manual-logs\two-client-completed-correspondence-rerun-20260528-010504`
  - validity is PASS:
    - FFmpeg version check succeeded before runtime
    - switcher binary:
      `C:\streamsync-target\stream-sync-rerun\debug\stream-sync-switcher.exe`
      LastWriteTime `2026/05/28 1:05:18`
    - client/server/feed are PASS for this slice
  - output lag evidence:
    - input `438`
    - output `301`
    - throughput `17.151fps`
    - completed correspondence latency avg `2624.940ms`
    - completed correspondence latency max `5258ms`
    - completed correspondence latest latency `5251ms`
    - pending correspondence `137`
    - pending avg `2540.606ms`
    - pending max `5300ms`
    - latest input-output gap `156`
    - output lag to selected `150`
  - lookup/render evidence:
    - allowed lag `8`
    - bounded lookup hits `0`
    - render used continuous decoded `0`
    - stale `228`, not-ready `19`, future `0`
  - interpretation:
    - completed output and pending backlog are both seconds late
    - not-ready is secondary
    - stale/output backlog dominates
    - threshold tuning alone is insufficient
    - next work should move to raw BGRA stdout throughput and FFmpeg scale path
      split experiments
- 2026-05-28 first raw pipe / stdout throughput code slice:
  - implemented opt-in `--continuous-decoder-output-pipeline-experiment
    scaled-bgr24`
  - default remains scaled BGRA with `921600` stdout bytes/frame
  - experiment keeps the same scale path and uses BGR24 `691200`
    stdout bytes/frame, then converts back to BGRA before render
  - summary adds experiment mode, bytes/frame, pipe bytes saved/frame, and
    pixel conversion timing/count
  - no threshold, lookup, fallback, feed, slot1, 4-client, FFmpeg default, or
    Production Readiness change
- latest output availability rerun:
  - `S:\stream-sync\manual-logs\two-client-output-availability-rerun-20260527-173716`
  - validity is PASS:
    - build PASS
    - switcher binary:
      `C:\streamsync-target\stream-sync-rerun\debug\stream-sync-switcher.exe`
      LastWriteTime `2026/05/27 17:25:51`
    - client FFmpeg preflight/spawn errors are `none`
  - client / server / feed are PASS:
    - client1/client2 sent `900` frames each at `29.538fps` / `28.694fps`
    - server queued `1800` frames total
    - continuous feed received `453` frames and enqueued `423`
  - output backlog evidence:
    - `continuous_decode_input_frame_count=431`
    - `continuous_decode_output_frame_count=316`
    - `continuous_decode_pending_correspondence_count=115`
    - `continuous_decode_pending_correspondence_age_ms_max=3939`
    - `continuous_decode_pending_correspondence_age_ms_avg=1948.809`
    - `continuous_decode_latest_input_to_output_frame_gap=115`
    - `continuous_decode_output_lag_to_selected_frames=99`
    - `continuous_decode_input_to_output_lag_frames_max=118`
  - reader evidence:
    - `continuous_decode_reader_full_frame_elapsed_ms_avg=46.430`
    - `continuous_decode_reader_full_frame_elapsed_ms_max=1125`
    - `continuous_decode_reader_full_frame_slow_count=42`
    - `continuous_decode_stdout_read_waiting_for_full_frame=true`
  - lookup / availability:
    - `continuous_decode_bounded_lookup_allowed_lag_frames=8`
    - bounded hit count `3`, render continuous use `3`
    - stale availability count `238`
    - not-ready availability count `22`
    - future availability count `0`
  - interpretation:
    - not-ready is secondary in this rerun
    - stale/output backlog is dominant
    - threshold tuning alone is insufficient because newest output remains far
      behind selected/source cadence
- latest reverse-order lag threshold A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-lag-reverse-ab-rerun-20260527-164258`
  - comparison is VALID
  - lag8 improves output lag, throughput, stale rejection, and reader average latency versus lag5
  - lag5 keeps a slight render FPS edge and slightly fewer not-ready rejects
  - default `8` promotion is HOLD; keep default `5` unchanged
- latest matched rerun:
  - `S:\stream-sync\manual-logs\two-client-ab-rerun-20260522-103943`
- PASS:
  - `continuous_decode_config_enabled=true`
  - `continuous_decode_runtime_enabled=true`
  - `continuous_decode_slot0_enabled=true`
  - `continuous_decode_ffmpeg_low_latency_args_enabled=true`
  - `continuous_decode_ffmpeg_probe_args_enabled=true`
  - `continuous_decode_ffmpeg_loglevel=warning`
  - `continuous_feed_enabled=true`
  - matched suppression OFF/ON comparison VALID寄り
  - `continuous_decode_slot0_one_shot_suppression_enabled=true`
  - `continuous_decode_bounded_lookup_enabled=true`
  - `continuous_decode_bounded_lookup_allowed_lag_frames=5`
- OFF no suppression:
  - client fps `27.806` / `27.167`
  - output throughput `20.129fps`
  - output lag to selected `17`
  - latest input minus latest output lag `20`
  - competing one-shot `37` attempts / `5401ms`
  - continuous render use and bounded lookup hit both `0`
  - render FPS `11.594`
- ON slot0 suppression:
  - pasted client evidence includes `28.134fps`
  - output throughput `26.814fps`
  - output lag to selected `8`
  - latest input minus latest output lag `33`
  - competing one-shot `13` attempts / `942ms`
  - continuous render use and bounded lookup hit both `11`
  - render FPS `17.401`
  - suppression reasons `continuous_not_ready:27|stale:228|future:0|unknown:0`

## Code Path Summary
Current continuous runtime has three relevant queues/counters:

1. `input_tx` / `writer_input_queue_len`
   - `enqueue()` increments `writer_input_queue_len`, then `try_send`s `TwoRealContinuousDecodeInput` to a bounded sync channel.
   - The writer thread decrements `writer_input_queue_len` when it receives the input.
   - If `try_send` is full, `continuous_decode_dropped_stale_count` increments and the input is not sent to FFmpeg.

2. `correspondence`
   - The writer thread pushes `TwoRealContinuousDecodeMetadata` into `correspondence` before `stdin.write_all(&encoded_payload)`.
   - The reader thread pops one metadata item only after it has read one full raw BGRA output frame from stdout.
   - Therefore `continuous_decode_output_pending_correspondence_count` is the count of inputs that have been handed to the writer path and are still waiting for matching full stdout frames.

3. `continuous_key_order` / decoded cache
   - `drain_outputs()` receives decoded events from the reader thread and inserts them into the decoded cache.
   - `continuous_key_order` is bounded by `TWO_REAL_CONTINUOUS_DECODE_QUEUE_BOUND` (`30`).
   - When the decoded cache exceeds the bound, oldest decoded keys are removed and `continuous_decode_dropped_stale_count` increments.

## Why Pending Correspondence Can Grow
- Writer can accept input faster than reader receives full BGRA frames.
- Every writer-accepted input pushes metadata to `correspondence`.
- Reader cannot pop metadata until `expected_len` bytes for one full frame have been read from stdout.
- In the latest rerun, input was `417` and output was `367`, so the coarse input-output gap was `50`; pending correspondence was `48`, which is consistent with a writer-to-reader/output backlog.
- `continuous_decode_latest_input_minus_latest_output_lag=78` and `continuous_decode_output_lag_to_selected_frames=64` show that the newest continuous output itself is still behind selected/source cadence.
- This does not prove FFmpeg is broken. It can mean decoder throughput is below input/feed rate, stdout full-frame reads are slow, FFmpeg internal buffering is delaying output, or a correspondence/output mismatch is accumulating.

## Stdout Read Metrics
- `continuous_decode_stdout_read_elapsed_ms` is accumulated in the reader thread for successful full-frame reads.
- It is not direct render-loop blocking time.
- A high value with nonzero output means reader thread spent substantial wall time waiting for full raw frames.
- `continuous_decode_stdout_reader_blocked_count` increments on lookup misses when:
  - pending correspondence is nonzero
  - writer input queue is empty
  - stdout reader is in a full-frame read
- This pattern means accepted input has moved past the writer queue, but render lookup observes the reader still waiting for output. It points after writer queue intake and before decoded cache availability.

## Input / Output Count Gap
- Latest output availability rerun:
  - input `431`
  - output `316`
  - coarse gap `115`
  - pending correspondence `115`
  - pending correspondence average age `1948.809ms`
  - latest input-output gap `115`
  - output lag to selected `99`
  - output throughput `21.269fps`
  - client output fps `29.538` / `28.694`
- This makes the backlog shape clearer than earlier runs: accepted continuous
  input is moving past feed/writer intake, but full stdout frames arrive too
  slowly to keep newest decoded output near the selected frame.
- Latest run:
  - input `417`
  - output `367`
  - coarse gap `50`
  - pending correspondence `48`
  - latest input minus latest output lag `78`
  - output lag to selected `64`
  - output throughput `23.309fps`
  - client output fps `28.561` / `28.721`
- The gap should be treated as a throughput / latency signal, not a single-cause failure.
- Continuous output throughput is below the observed client/source fps. That makes output throughput / stdout read latency / raw BGRA output cost a safer next investigation target than lookup-threshold tuning.
- Possible contributors:
  - feed rate exceeds FFmpeg decode + scale + rawvideo output throughput
  - stdout reader waits for full `921600` byte BGRA frames
  - `scale=640:360:flags=neighbor` plus raw BGRA conversion/output is still too expensive for the current continuous path
  - reader buffering or full-frame read boundaries add burst latency
  - FFmpeg parser/decoder buffering even with low-latency args
  - output event drain cadence from reader thread to render loop is not fast enough
  - one-shot fallback load competes for CPU/process/pipe resources while continuous decode is also active

## Decoded Queue And Drop Policy
- `queue_len=30` means the decoded cache is at the configured bound.
- `dropped_stale_count=337` is now split by reason as `input_queue_full:0|decoded_cache_bound:337|unknown:0`.
- Once decoded frames are older than the last 30 cached continuous outputs, they are removed from `continuous_key_order` and `decoded_cache`.
- This protects memory, but it also means a delayed render lookup cannot recover older decoded frames.
- In the latest rerun, latest decoded was `401` while requested was `446`; output lag to selected was `64`, and latest input minus latest output lag was `78`.
- Decoded cache bound drops are visible, but the newest decoded frame itself is still stale. Queue/drop policy is therefore not the first root cause by itself; the bigger issue is that continuous output throughput is not keeping up.

## One-Shot Fallback Double Load
- Bounded lookup failure led to `render_used_one_shot_fallback_count=15`.
- The same run had `one_shot_decode_attempt_count=30` and `one_shot_decode_elapsed_ms=3659`.
- Because continuous decoding stays active while one-shot fallback runs, the process may be doing both:
  - continuous FFmpeg stdin/stdout decode work
  - one-shot FFmpeg decode attempts for render safety
- This is correct for safety, but it can hide or worsen throughput problems. The next diagnostics should make double-load visible before removing fallback or changing behavior.

## Latest Diagnostics Interpretation
- Output availability diagnostics are now runtime VALID on
  `S:\stream-sync\manual-logs\two-client-output-availability-rerun-20260527-173716`.
- Completed correspondence diagnostics are runtime VALID on
  `S:\stream-sync\manual-logs\two-client-completed-correspondence-rerun-20260528-010504`.
- Completed latency avg/max/latest `2624.940ms` / `5258ms` / `5251ms`
  confirms that even successful outputs are seconds behind.
- Client FFmpeg, server queueing, and continuous feed are PASS in that rerun.
- The main lag shape is pending correspondence / stdout-reader full-frame
  latency / continuous output backlog / stale output.
- Not-ready remains visible, but `22` not-ready versus `238` stale availability
  rejects means not-ready is not the main issue in this rerun.
- Threshold tuning alone is insufficient: even with allowed lag `8`, latest
  input-to-output gap `115` and selected-to-output gap `99` remain far outside
  a safe sync-first display guard.
- Matched suppression OFF/ON comparison is VALID寄り on the same build and source fps mismatch is not noisy enough to reject the A/B read.
- Suppression ON strongly reduced competing one-shot work and improved output throughput, continuous render consumption, bounded lookup adoption, and render FPS.
- One-shot double-load is a strong contributor candidate, but suppression remains opt-in isolation evidence rather than a default policy change.
- ON evidence still suppresses stale `228` and continuous-not-ready `27` cases.
- The bounded lookup allowed-lag threshold / stale-guard review is now recorded in `docs/operations/continuous-decoded-lookup-plan.md`; lag8 is a held adoption candidate, and default `8` promotion remains HOLD.
- Feed max count should remain unchanged for now. Feeding faster while output throughput is already below source cadence may increase correspondence backlog instead of improving render consumption.
- One-shot fallback remains the safe default path. Any suppression must stay slot0/two-real/opt-in and preserve default behavior.

## Next Design Candidates
- Next evidence candidate should move from the optimized `scaled-bgr24` A/B
  result to:
  - human-side `no-scale-bgra` A/B rerun for the implemented FFmpeg scale path
    split slice
  - reader/completed latency breakdown diagnostics if no-scale evidence is
    ambiguous
  - direct BGR24 render path docs-first impact review only
  - keep it slot0 / two-real / opt-in continuous only
  - keep sync-first stale-frame safety explicit
- 2026-05-28 first BGR24 conversion optimization slice is implemented for
  `scaled-bgr24` only. It uses safe in-place reverse scalar expansion and adds
  conversion reuse/allocation/bytes/mode summary fields.
- 2026-05-28 optimized BGR24 A/B evidence now shows conversion optimization
  PASS, but default BGRA remains the safe path and optimized `scaled-bgr24`
  adoption is HOLD.
- 2026-05-28 first FFmpeg scale path split code slice is implemented as
  `no-scale-bgra`; runtime lag evidence is pending.
- Candidate comparison now lives in
  `docs/operations/continuous-output-availability-plan.md`.
- Detailed output pipeline experiment design now lives in
  `docs/operations/continuous-output-pipeline-experiment-plan.md`.
- Held or later throughput experiments:
  - additional FFmpeg scale-path comparison beyond `no-scale-bgra`
  - raw BGRA pipe / stdout reader buffering behavior change
  - continuous output queue/cache policy changes
- Held as risky default behavior:
  - default threshold widening
  - targetTime-aware decoded queue lookup implementation
  - unbounded latest decoded fallback
  - feed max count increase

## Throughput Analysis Split
- Detailed continuous output throughput analysis now lives in `docs/operations/continuous-output-throughput-plan.md`.
- The opt-in double-load isolation design now lives in `docs/operations/continuous-one-shot-double-load-plan.md`.
- This lag plan remains the source for frame-id lag, pending correspondence, decoded cache/drop, and render-consumption interpretation.
- The throughput plan is the source for the matched A/B read that suppression ON reduces double-load and improves continuous output throughput without proving a single global FPS cause.
- Current code-path candidates are:
  - FFmpeg decode + `scale=640:360:flags=neighbor` + BGRA conversion/output
  - stdout full-frame read latency for `921600` byte frames
  - reader buffering / per-frame allocation and materialization
  - continuous decoder and one-shot fallback double-load
- The reverse-order threshold A/B keeps lag8 as a held candidate and moves the
  next docs review to output availability / throughput. Do not turn that review
  into an unguarded stale-frame path or a default suppression change.

## Minimal Next Diagnostics
First priority:

- `continuous_decode_pending_correspondence_frame_id_min`
- `continuous_decode_pending_correspondence_frame_id_max`
- `continuous_decode_latest_input_minus_latest_output_lag`
- `continuous_decode_input_to_output_lag_frames_max`
- `continuous_decode_output_lag_to_selected_frames`
- `continuous_decode_output_throughput_fps`
- `continuous_decode_reader_full_frame_elapsed_ms_max`
- `continuous_decode_queue_drop_reason_counts`

Second priority:

- `continuous_decode_input_to_output_lag_frames_avg`
- `continuous_decode_correspondence_pending_age_ms`

Hold for later:

- `continuous_decode_output_latency_frames_avg`
- `continuous_decode_output_latency_frames_max`

Reason:

- The first priority fields answer whether the backlog is mostly writer-to-reader correspondence lag, latest input/output frame-id lag, or output throughput.
- The second priority fields split read latency and drop reasons after the first backlog shape is visible.
- Average/max output latency requires timestamps or enqueue-time tracking per metadata entry; useful, but more invasive than frame-id min/max and throughput counters.

## First Diagnostics Implementation Slice Candidate
- slot0 only
- two-real preview loop only
- opt-in continuous only
- summary diagnostics only
- no behavior change
- no allowed lag threshold change
- no targetTime-aware lookup implementation
- no latest decoded fallback
- no feed max count change
- no one-shot fallback removal
- no slot1 / 4-client rollout

Implementation shape:

- Extend `TwoRealContinuousDecodeMetadata` or adjacent runtime state only as needed to observe enqueue/write/read age.
- Expose pending correspondence frame-id min/max by peeking the `correspondence` queue.
- Track latest continuous input frame id and latest continuous output frame id, then derive `latest_input_minus_latest_output_lag`.
- Track max full-frame stdout read elapsed from successful reader outputs.
- Derive output throughput from output count over the runtime first-input-to-now elapsed window.
- Split `continuous_decode_dropped_stale_count` into reason counts before changing queue/drop behavior.

## Diagnostics Implementation Status
- 2026-05-20 implementation slice is complete for slot0 / two-real / opt-in continuous summary diagnostics.
- Added summary fields:
  - `continuous_decode_latest_input_minus_latest_output_lag`
  - `continuous_decode_pending_correspondence_frame_id_min`
  - `continuous_decode_pending_correspondence_frame_id_max`
  - `continuous_decode_input_to_output_lag_frames_max`
  - `continuous_decode_output_lag_to_selected_frames`
  - `continuous_decode_output_throughput_fps`
  - `continuous_decode_reader_full_frame_elapsed_ms_max`
  - `continuous_decode_queue_drop_reason_counts`
- `continuous_decode_dropped_stale_count` remains unchanged as the shared historical counter; `continuous_decode_queue_drop_reason_counts` is additive and currently splits:
  - `input_queue_full`
  - `decoded_cache_bound`
  - `unknown`
- 2026-05-27 availability diagnostics slice adds:
  - `continuous_decode_pending_correspondence_count`
  - `continuous_decode_pending_correspondence_age_ms_max`
  - `continuous_decode_pending_correspondence_age_ms_avg`
  - `continuous_decode_pending_correspondence_oldest_frame_id`
  - `continuous_decode_pending_correspondence_newest_frame_id`
  - `continuous_decode_latest_input_to_output_frame_gap`
  - `continuous_decode_latest_selected_to_output_frame_gap`
  - `continuous_decode_output_availability_not_ready_count`
  - `continuous_decode_output_availability_stale_count`
  - `continuous_decode_output_availability_future_count`
- 2026-05-28 completed correspondence latency diagnostics slice adds:
  - `continuous_decode_completed_correspondence_count`
  - `continuous_decode_completed_correspondence_latency_ms_avg`
  - `continuous_decode_completed_correspondence_latency_ms_max`
  - `continuous_decode_completed_correspondence_latency_slow_count`
  - `continuous_decode_completed_correspondence_latency_slow_threshold_ms`
  - `continuous_decode_completed_correspondence_frame_id_min`
  - `continuous_decode_completed_correspondence_frame_id_max`
  - `continuous_decode_completed_correspondence_latest_latency_ms`
- latest completed correspondence rerun validates these fields and shows the
  same backlog shape in both completed and pending correspondence:
  - completed avg/max/latest `2624.940ms` / `5258ms` / `5251ms`
  - pending avg/max `2540.606ms` / `5300ms`
- Held fields:
  - `continuous_decode_input_to_output_lag_frames_avg`
  - `continuous_decode_output_latency_frames_avg`
  - `continuous_decode_output_latency_frames_max`
- Behavior intentionally unchanged:
  - exact lookup first
  - bounded-lag lookup second
  - one-shot fallback third
  - allowed lag threshold `5` frames
  - feed max count
  - low-latency args default
  - no latest decoded fallback
  - no targetTime-aware lookup implementation
  - no slot1 / 4-client rollout

## Out Of Scope
- Changing `continuous_decode_bounded_lookup_allowed_lag_frames`
- targetTime-aware decoded queue lookup implementation
- unbounded latest decoded fallback
- feed max count changes
- slot1 continuous
- 4-client continuous
- server / client / protocol changes
- request/response persistent decoder revival
- GPU decode
- one-shot fallback removal
- Production Readiness PASS

## Readiness
- Bounded feed helper: PASS for current slot0/two-real scope
- Bounded lookup wiring: PASS
- Output lag diagnostics wiring: PASS
- Continuous output: PASS
- Continuous render consumption: PARTIAL PASS on suppression ON evidence
- Production Readiness: FAIL
