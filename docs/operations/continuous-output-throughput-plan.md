<!-- stream-sync/docs/operations/continuous-output-throughput-plan.md -->

# Continuous Output Throughput Plan

Last updated: 2026-05-22

## Purpose
- Analyze why slot0 continuous decoder output throughput stayed around `23fps` after feed helper and continuous output both reached runtime PASS.
- Keep the next code pass diagnostics-only: no throughput behavior changes, no threshold tuning, no lookup policy changes, no feed max count changes.
- Separate stdout full-frame read latency, raw BGRA output volume, FFmpeg scale/output path cost, reader buffering, and one-shot fallback double-load before choosing the next code slice.

## Latest Evidence
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260520-014041`
- Feed PASS:
  - `continuous_feed_enqueued_count=412`
  - `continuous_decode_input_from_feeder_count=412`
  - `continuous_decode_input_from_render_demand_count=5`
- Continuous output PASS:
  - `continuous_decode_input_frame_count=417`
  - `continuous_decode_output_frame_count=367`
  - `continuous_decode_output_throughput_fps=23.309`
- Continuous render consumption FAIL:
  - `render_used_continuous_decoded_count=0`
  - `continuous_decode_bounded_lookup_hit_count=0`
- Output lag:
  - `continuous_decode_latest_input_minus_latest_output_lag=78`
  - `continuous_decode_output_lag_to_selected_frames=64`
  - `continuous_decode_output_pending_correspondence_count=48`
  - `continuous_decode_reader_full_frame_elapsed_ms_max=1305`
  - `continuous_decode_stdout_read_elapsed_ms=15498`
  - `continuous_decode_stdout_reader_blocked_count=13`
- Source / render safety context:
  - client1 `effective_output_fps=28.561`
  - client2 `effective_output_fps=28.721`
  - `render_used_one_shot_fallback_count=15`
  - `one_shot_decode_attempt_count=30`
  - `one_shot_decode_elapsed_ms=3659`

## Code Path Summary
Continuous slot0 output path:

1. Feed helper / render-demand enqueue
   - The bounded feed helper is now the main input source.
   - Latest evidence shows `412` feeder inputs and only `5` render-demand inputs.

2. Continuous writer
   - Runtime input is accepted into the continuous input channel.
   - The writer thread pushes frame metadata into the correspondence queue before writing encoded H.264 bytes to FFmpeg stdin.

3. FFmpeg continuous process
   - Current output profile is scaled raw video:
     - `scale=640:360:flags=neighbor`
     - `-f rawvideo`
     - `-pix_fmt bgra`
     - `pipe:1`
   - Low-latency args may be enabled, but the output format remains raw BGRA.

4. Continuous stdout reader
   - The reader allocates one `expected_len` buffer for each output frame read attempt.
   - For `640x360` BGRA, `expected_len = 640 * 360 * 4 = 921600` bytes/frame.
   - The reader loops until one full raw frame is read from stdout.
   - Only after a full raw frame is available can it pop the correspondence metadata and emit a decoded output event.

5. Render-side drain and lookup
   - Render drains decoded events into the bounded decoded cache/key order.
   - Exact lookup runs first, bounded-lag lookup runs second, and one-shot fallback remains the safety path.

## Throughput Shape
- Continuous output throughput was `23.309fps`.
- Client output fps was `28.561` / `28.721`, so continuous output was roughly `5.3fps` below observed source cadence.
- The raw stdout volume implied by current output format is:
  - `921600 bytes/frame * 23.309fps = about 21.5 MB/s`
  - `921600 bytes/frame * 28.6fps = about 26.4 MB/s`
  - `921600 bytes/frame * 30fps = about 27.6 MB/s`
- This byte rate is not huge for memory bandwidth, but it is still a per-frame pipe/read/copy boundary attached to FFmpeg decode, scale, pixel conversion, and render-side drain.
- `continuous_decode_reader_full_frame_elapsed_ms_max=1305` is an outlier/max signal. It does not prove the average read is slow, but it proves the current reader can spend more than one second waiting for a full raw frame in at least one observed interval.
- Input/output gap and pending correspondence agree with a throughput backlog:
  - input `417`
  - output `367`
  - coarse gap `50`
  - pending correspondence `48`
  - latest input-output lag `78`

## Candidate Causes
### FFmpeg Decode / Scale / BGRA Conversion
- FFmpeg must decode H.264, scale to `640x360`, convert to BGRA, and write raw frames to stdout.
- `scale=640:360:flags=neighbor` is cheap compared with high-quality scaling, but it is still inside the synchronous FFmpeg output path.
- BGRA conversion is convenient for the current renderer, but it expands every output frame to `921600` bytes.
- This is a plausible first-class contributor, but not proven as the only cause.

### Stdout Full-Frame Boundary
- The reader cannot emit a decoded frame until a complete `921600` byte frame has been read.
- Partial stdout progress is not render-usable; it keeps correspondence pending.
- FFmpeg internal buffering, OS pipe behavior, short reads, and reader scheduling can all show up as full-frame read latency.
- Current diagnostics expose max full-frame elapsed and total stdout read elapsed, but not average, slow-frame count, per-frame interval, or byte throughput.

### Reader Allocation / Copy Cost
- The current reader materializes a full BGRA frame into a fresh buffer per output event.
- At `23fps`, that is about `21.5 MB/s` of raw output materialization; at source cadence it would be about `26-28 MB/s`.
- The volume is still modest, so this should be measured before optimizing. It may be secondary compared with decode/scale/output latency.

### Render Drain Cadence
- Render drains decoded events into a bounded cache.
- If drain cadence falls behind, decoded cache bound drops can grow.
- However, output throughput is measured from reader outputs, so render drain alone does not explain FFmpeg/reader producing only `23.309fps` unless channel backpressure or scheduling is also involved.

### Decoded Cache Bound Drops
- Latest evidence has `continuous_decode_queue_drop_reason_counts=input_queue_full:0|decoded_cache_bound:337|unknown:0`.
- The cache bound is doing its memory-control job, but newest decoded output is still stale.
- This makes decoded cache bound drops more likely a result of output being behind and the cache staying full, not the first root cause.

### One-Shot Fallback Double Load
- One-shot fallback is still required for safety because continuous render consumption is `0`.
- The same run had `one_shot_decode_attempt_count=30` and `one_shot_decode_elapsed_ms=3659`.
- Continuous FFmpeg and one-shot FFmpeg can therefore compete for CPU, process scheduling, pipe I/O, and memory bandwidth.
- This could worsen continuous throughput, but suppressing fallback first is risky because it would remove the current visible safety path.

## Diagnostics-Only Next Slice
The 2026-05-22 code slice is implemented as diagnostics-only and remains slot0 / two-real / opt-in:

- `continuous_decode_reader_full_frame_elapsed_ms_avg`
- `continuous_decode_reader_full_frame_slow_count`
- `continuous_decode_reader_full_frame_slow_threshold_ms`
- `continuous_decode_output_bytes_total`
- `continuous_decode_output_bytes_per_sec`
- `continuous_decode_output_frame_interval_ms_avg`
- `continuous_decode_output_frame_interval_ms_max`
- `continuous_decode_stdout_read_throughput_bytes_per_ms`
- `continuous_decode_ffmpeg_scale_enabled`
- `continuous_decode_ffmpeg_output_pixel_format`
- `continuous_decode_competing_one_shot_decode_elapsed_ms`
- `continuous_decode_competing_one_shot_attempt_count`

Diagnostics interpretation goal:

- If full-frame read avg/slow counts are high, focus on stdout reader / FFmpeg output cadence.
- If output bytes/sec is stable but below source cadence, focus on decode/scale/pixel-format throughput.
- If output frame interval max aligns with one-shot elapsed bursts, double-load becomes a stronger candidate.
- If avg read is healthy but render consumption remains `0`, revisit lookup only after output lag shrinks into a safe guard range.

## Diagnostics Implementation Status
- Summary now exposes the planned throughput fields:
  - reader full-frame average, slow count, and fixed `66ms` slow threshold
  - output raw bytes total and bytes/sec
  - reader-delivered output frame interval average/max
  - stdout full-frame read throughput bytes/ms
  - FFmpeg continuous scale-enabled and output pixel-format diagnostics
  - one-shot attempt/elapsed totals observed while the continuous FFmpeg process is running
- Existing raw BGRA premise remains visible through
  `continuous_decode_stdout_expected_frame_bytes`; current slot size output is
  still `640x360` BGRA and therefore `921600` bytes/frame.
- The slice only aggregates existing slot0 continuous reader/output timing and
  one-shot diagnostics. It does not change continuous decode behavior, lookup
  policy, allowed lag threshold, feed max count, FFmpeg defaults, output pixel
  format, scale path, or one-shot fallback policy.
- Runtime rerun remains human-side from `S:\stream-sync`; Codex did not run the
  two-client preview loop for this implementation.

## Opt-In Experiment Candidates
Only after diagnostics show where the time is going:

1. Continuous output pixel format comparison
   - Compare BGRA against an alternate raw format only behind an opt-in flag.
   - Current render/composition path expects BGRA-like decoded frames, so non-BGRA output would need conversion somewhere else.
   - Default must not change until compatibility and total pipeline cost are measured.

2. Scale path comparison
   - Compare FFmpeg scale versus moving scale/conversion work outside the continuous FFmpeg output path.
   - This is not automatically cheaper: raw 720p BGRA would be much larger than `921600` bytes/frame.
   - Treat it as a measurement experiment, not an architectural decision.

3. Output reader buffering experiment
   - Compare current full-frame read loop with a small buffering/read helper change only if diagnostics show read-bound behavior.
   - Preserve full-frame correctness; do not emit partial raw frames.

4. One-shot fallback suppression
   - Hold as risky-first.
   - It may reduce double-load, but it can also remove render safety while continuous output is still stale and unused.

5. Additional FFmpeg args
   - Keep defaults unchanged.
   - Any new FFmpeg args should be opt-in and reported in summary diagnostics.

## Design Decision
- The diagnostics-only code change is now in place; the next evidence gate is
  the human rerun that reads these summary fields.
- A small opt-in experiment is second choice, after diagnostics identify whether the bottleneck is FFmpeg output, stdout read, raw BGRA volume, reader buffering, or one-shot competition.
- Threshold tuning is held because output lag is far outside the `5` frame guard.
- TargetTime-aware lookup and latest decoded fallback are held because accepting `64` to `78` frames of lag would violate sync-first behavior.
- Feed max count remains unchanged because output throughput is already below source cadence.
- Production Readiness remains FAIL.

## Out Of Scope
- `continuous_decode_bounded_lookup_allowed_lag_frames` changes
- TargetTime-aware decoded queue lookup implementation
- Latest decoded fallback
- Feed max count changes
- Slot1 continuous
- 4-client continuous
- Server / client / protocol changes
- Request/response persistent decoder revival
- GPU decode
- One-shot fallback removal
- Low-latency args default changes
- Single-cause FPS conclusion
- Production Readiness PASS
