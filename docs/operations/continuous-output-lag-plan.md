<!-- stream-sync/docs/operations/continuous-output-lag-plan.md -->

# Continuous Output Lag Plan

Last updated: 2026-05-20

## Purpose
- Analyze why slot0 continuous decoded output still trails the requested render frame after bounded feed helper and bounded-lag lookup wiring both reached runtime evidence.
- Keep the implementation slice diagnostics-only.
- Do not change allowed lag threshold, feed max count, lookup policy, FFmpeg defaults, server/client/protocol, slot1, or 4-client rollout.
- Define and track the smallest diagnostics slice for continuous output lag / pending correspondence / stdout read latency / decoded queue-drop policy.

## Latest Evidence
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260520-014041`
- PASS:
  - `continuous_decode_config_enabled=true`
  - `continuous_decode_runtime_enabled=true`
  - `continuous_decode_slot0_enabled=true`
  - `continuous_decode_ffmpeg_low_latency_args_enabled=true`
  - `continuous_feed_enabled=true`
  - `continuous_feed_attempt_count=300`
  - `continuous_feed_handoff_request_count=930`
  - `continuous_feed_frame_received_count=418`
  - `continuous_feed_enqueued_count=412`
  - `continuous_feed_skipped_count=6`
  - `continuous_decode_input_from_feeder_count=412`
  - `continuous_decode_input_from_render_demand_count=5`
  - `continuous_decode_feeder_lag_to_selected=7`
  - `continuous_decode_bounded_lookup_enabled=true`
  - `continuous_decode_bounded_lookup_allowed_lag_frames=5`
- continuous output PASS:
  - `continuous_decode_input_frame_count=417`
  - `continuous_decode_output_frame_count=367`
  - `continuous_decode_queue_len=30`
- FAIL:
  - `continuous_decode_bounded_lookup_hit_count=0`
  - `continuous_decode_bounded_lookup_rejected_stale_count=13`
  - `continuous_decode_bounded_lookup_rejected_not_ready_count=2`
  - `continuous_decode_bounded_lookup_fallback_to_one_shot_count=15`
  - `render_used_continuous_decoded_count=0`
  - `render_used_one_shot_fallback_count=15`
- Lag / backlog:
  - `continuous_decode_requested_frame_id=446`
  - `continuous_decode_latest_decoded_frame_id=401`
  - `continuous_decode_requested_minus_latest_lag=64`
  - `continuous_decode_frame_id_lag=64`
  - `continuous_decode_output_pending_correspondence_count=48`
  - `continuous_decode_latest_input_minus_latest_output_lag=78`
  - `continuous_decode_pending_correspondence_frame_id_min=404`
  - `continuous_decode_pending_correspondence_frame_id_max=479`
  - `continuous_decode_input_to_output_lag_frames_max=78`
  - `continuous_decode_output_lag_to_selected_frames=64`
  - `continuous_decode_output_throughput_fps=23.309`
  - `continuous_decode_reader_full_frame_elapsed_ms_max=1305`
  - `continuous_decode_stdout_read_elapsed_ms=15498`
  - `continuous_decode_stdout_reader_blocked_count=13`
  - `continuous_decode_dropped_stale_count=337`
  - `continuous_decode_queue_drop_reason_counts=input_queue_full:0|decoded_cache_bound:337|unknown:0`
  - `one_shot_decode_attempt_count=30`
  - `one_shot_decode_elapsed_ms=3659`
  - `effective_render_fps_after_first_render=14.198`
- Source/client context:
  - `client1 effective_output_fps=28.561`
  - `client2 effective_output_fps=28.721`
  - server `frames_queued=1800`

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
- Continuous opt-in, low-latency args, bounded feed helper, output-lag diagnostics wiring, and continuous output are PASS for slot0/two-real/opt-in scope.
- Continuous render consumption and bounded lookup adoption remain FAIL because no continuous decoded frame was accepted for render.
- The latest blocker is not a too-small `5` frame bounded-lag threshold. A threshold wide enough to accept lag `64` or `78` would risk stale video and contradict the sync-first goal.
- Output throughput is below the client/source fps range (`23.309fps` vs `28fps` class source output), so the next safest step is docs-first analysis of:
  - continuous decoder output throughput
  - stdout full-frame read latency
  - raw BGRA output volume and scale path cost
  - continuous decoder + one-shot fallback double-load
- Feed max count should remain unchanged for now. Feeding faster while output throughput is already below source cadence may increase correspondence backlog instead of improving render consumption.
- One-shot fallback remains a safety path. Suppressing it before continuous output is usable could reduce visible output safety even if it reduces load.

## Next Design Candidates
- Diagnostics-only candidate:
  - add timing around stdout reader buffering / per-frame read phases if current full-frame max is insufficient
  - expose output reader delivery cadence versus render-loop drain cadence
  - expose raw BGRA read/copy/materialization costs separately from FFmpeg decode/scale when possible
- Small opt-in experiment candidate:
  - compare continuous decoder output pixel format / scale path without changing default behavior
  - keep the experiment two-real / slot0 / opt-in only
  - preserve one-shot fallback and all current stale-frame guards
- Held as risky-first:
  - widening `continuous_decode_bounded_lookup_allowed_lag_frames`
  - targetTime-aware decoded queue lookup implementation
  - unbounded latest decoded fallback
  - one-shot fallback suppression/removal
  - feed max count increase

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
- Held fields:
  - `continuous_decode_input_to_output_lag_frames_avg`
  - `continuous_decode_output_latency_frames_avg`
  - `continuous_decode_output_latency_frames_max`
  - `continuous_decode_correspondence_pending_age_ms`
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
- Continuous render consumption: FAIL
- Production Readiness: FAIL
