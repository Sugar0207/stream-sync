<!-- stream-sync/docs/operations/continuous-output-throughput-plan.md -->

# Continuous Output Throughput Plan

Last updated: 2026-05-22

## Purpose
- Analyze why slot0 continuous decoder output throughput stays below source cadence after feed helper, continuous output, and throughput diagnostics all reached runtime evidence.
- Keep the next candidate docs-first and opt-in: no default behavior changes, no threshold tuning, no lookup policy changes, no feed max count changes.
- Use the validated throughput diagnostics to separate stdout full-frame read latency, raw BGRA output volume, FFmpeg scale/output path cost, reader buffering, and one-shot fallback double-load before choosing the next code slice.

## Latest Evidence
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260522-075029`
- validity:
  - build PASS with `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
  - low-latency args PASS:
    - `continuous_decode_ffmpeg_low_latency_args_enabled=true`
    - `continuous_decode_ffmpeg_probe_args_enabled=true`
    - `continuous_decode_ffmpeg_loglevel=warning`
  - throughput diagnostics runtime evaluation: VALID
- Feed PASS:
  - `continuous_feed_frame_received_count=458`
  - `continuous_feed_enqueued_count=449`
  - `continuous_decode_input_from_feeder_count=449`
  - `continuous_decode_input_from_render_demand_count=5`
  - `continuous_decode_feeder_lag_to_selected=2`
- Continuous output PASS:
  - `continuous_decode_input_frame_count=454`
  - `continuous_decode_output_frame_count=396`
  - `continuous_decode_output_throughput_fps=21.773`
  - `continuous_decode_output_bytes_total=364953600`
  - `continuous_decode_output_bytes_per_sec=20065625.687`
- Continuous render consumption FAIL:
  - `render_used_continuous_decoded_count=0`
  - `continuous_decode_render_exact_hit_count=0`
  - `continuous_decode_bounded_lookup_hit_count=0`
- Output/read diagnostics:
  - `continuous_decode_stdout_expected_frame_bytes=921600`
  - `continuous_decode_reader_full_frame_elapsed_ms_avg=45.192`
  - `continuous_decode_reader_full_frame_elapsed_ms_max=1217`
  - `continuous_decode_reader_full_frame_slow_count=43`
  - `continuous_decode_output_frame_interval_ms_avg=42.228`
  - `continuous_decode_output_frame_interval_ms_max=382`
  - `continuous_decode_stdout_read_throughput_bytes_per_ms=20393.026`
  - `continuous_decode_ffmpeg_scale_enabled=true`
  - `continuous_decode_ffmpeg_output_pixel_format=bgra`
- Output lag:
  - `continuous_decode_requested_frame_id=526`
  - `continuous_decode_latest_decoded_frame_id=458`
  - `continuous_decode_requested_minus_latest_lag=73`
  - `continuous_decode_latest_input_minus_latest_output_lag=74`
  - `continuous_decode_output_lag_to_selected_frames=73`
  - `continuous_decode_pending_correspondence_frame_id_min=464`
  - `continuous_decode_pending_correspondence_frame_id_max=532`
- Source / render safety context:
  - client1 `effective_output_fps=28.358`
  - client2 `effective_output_fps=28.501`
  - `effective_render_fps_after_first_render=13.737`
  - `continuous_decode_competing_one_shot_attempt_count=34`
  - `continuous_decode_competing_one_shot_decode_elapsed_ms=3515`
  - `one_shot_decode_attempt_count=34`
  - `one_shot_decode_elapsed_ms=3515`

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
- Continuous output throughput was `21.773fps`.
- Client output fps was `28.358` / `28.501`, so continuous output was roughly `6.6fps` below observed source cadence.
- The raw stdout volume implied by current output format is:
  - measured current run: `continuous_decode_output_bytes_per_sec=20065625.687`
  - `921600 bytes/frame * 21.773fps = about 20.1 MB/s`
  - `921600 bytes/frame * 28.4fps = about 26.2 MB/s`
  - `921600 bytes/frame * 30fps = about 27.6 MB/s`
- This byte rate is not huge for memory bandwidth, but it is still a per-frame pipe/read/copy boundary attached to FFmpeg decode, scale, pixel conversion, and render-side drain.
- The new diagnostics are runtime-valid:
  - reader full-frame average was `45.192ms`
  - reader full-frame max was `1217ms`
  - `43` full-frame reads crossed the fixed `66ms` slow threshold
  - reader-delivered output frame interval average was `42.228ms`
  - frame interval max was `382ms`
- The reader average and output interval average are already slower than 28fps-class cadence, while the max full-frame stall proves at least one interval waited more than one second for a full raw frame.
- Input/output gap and pending correspondence agree with a throughput backlog:
  - input `454`
  - output `396`
  - coarse gap `58`
  - latest input-output lag `74`
  - output lag to selected `73`

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
- The same run had `continuous_decode_competing_one_shot_attempt_count=34` and `continuous_decode_competing_one_shot_decode_elapsed_ms=3515`; the one-shot totals were the same `34` attempts / `3515ms`.
- Continuous FFmpeg and one-shot FFmpeg can therefore compete for CPU, process scheduling, pipe I/O, and memory bandwidth.
- This makes double-load a strong next isolation candidate, but it is not proof of the sole cause.

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
- Human rerun `S:\stream-sync\manual-logs\two-client-render-rerun-20260522-075029`
  now validates the diagnostics at runtime. Reader avg/slow, raw output
  bytes/sec, output frame interval, stdout throughput, scale/pixel-format, and
  competing one-shot fields all appeared in the summary.

## Opt-In Experiment Candidates
The next docs-first candidate is the one-shot double-load isolation plan in
`docs/operations/continuous-one-shot-double-load-plan.md`.

1. One-shot fallback double-load isolation
   - Suppress slot0 one-shot fallback only behind a two-real / opt-in continuous
     experiment while the slot0 continuous runtime is running.
   - Keep slot1 one-shot behavior unchanged.
   - Compare previous-frame hold, placeholder, and no-updated-frame render safety
     before choosing the experiment behavior.
   - Keep production default unchanged.

2. Continuous output pixel format comparison
   - Hold after the double-load isolation design.
   - Current render/composition path expects BGRA-like decoded frames, so non-BGRA output would need conversion somewhere else.
   - Default must not change until compatibility and total pipeline cost are measured.

3. Scale path comparison
   - Hold after double-load isolation design.
   - Raw 720p BGRA would be much larger than `921600` bytes/frame, so moving scale work is not automatically cheaper.

4. Output reader buffering experiment
   - Compare current full-frame read loop with a small buffering/read helper change only if diagnostics show read-bound behavior.
   - Preserve full-frame correctness; do not emit partial raw frames.

5. Additional FFmpeg args
   - Keep defaults unchanged.
   - Any new FFmpeg args should be opt-in and reported in summary diagnostics.

## Design Decision
- The throughput diagnostics runtime evaluation is VALID on
  `20260522-075029`.
- Feed remains PASS, continuous output remains PASS, and render consumption
  remains FAIL; those outcomes stay separate.
- The next docs-first design target is a slot0 one-shot fallback double-load
  isolation experiment, not threshold tuning.
- Pixel-format, scale-path, reader-buffering, and additional FFmpeg args remain
  later opt-in experiment candidates.
- Threshold tuning is held because output lag is far outside the `5` frame guard.
- TargetTime-aware lookup and latest decoded fallback are held because accepting `73` to `74` frames of lag would violate sync-first behavior.
- Feed max count remains unchanged because output throughput is already below source cadence.
- Production Readiness remains FAIL.

## One-Shot Isolation Implementation Update
- 2026-05-22 first isolation code slice now adds
  `--continuous-decoder-slot0-suppress-one-shot-fallback`.
- Scope stays slot0 / two-real / opt-in continuous only; default behavior and
  slot1 one-shot fallback remain unchanged.
- The safe first-slice render result is the existing decode-deferred placeholder
  path with `ContinuousOneShotSuppressed`, not unbounded stale decoded output.
- Summary now adds suppression-enabled, suppression-count, suppression-reason,
  render-safety, continuous-not-ready, and stale counters while keeping the
  competing one-shot counters from the throughput diagnostics slice.
- Next evidence gate is a human `S:\stream-sync` rerun that compares the base
  low-latency suffix with and without the new suppression flag.

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
