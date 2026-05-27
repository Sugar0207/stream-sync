<!-- stream-sync/docs/operations/continuous-output-throughput-plan.md -->

# Continuous Output Throughput Plan

Last updated: 2026-05-28

## Purpose
- Analyze why slot0 continuous decoder output throughput stays below source cadence after feed helper, continuous output, and throughput diagnostics all reached runtime evidence.
- Keep this throughput plan docs-first and opt-in: it does not change defaults,
  lookup threshold/policy, or feed max count. Threshold review now lives in the
  decoded lookup plan.
- Use the validated throughput diagnostics to separate stdout full-frame read latency, raw BGRA output volume, FFmpeg scale/output path cost, reader buffering, and one-shot fallback double-load before choosing the next code slice.
- After the reverse-order threshold A/B, move the next main line to output
  availability / throughput. The central candidate comparison now lives in
  `docs/operations/continuous-output-availability-plan.md`.

## Latest Evidence
- latest output pipeline A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200`
  - evidence is VALID-ish / useful on the same
    `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
  - both default and scaled server runs queued `1800` frames
  - `scaled-bgr24` wiring is PASS:
    - `continuous_decode_output_pipeline_experiment_mode=scaled-bgr24`
    - `continuous_decode_ffmpeg_output_pixel_format=bgr24`
    - `continuous_decode_output_bytes_per_frame=691200`
    - `continuous_decode_output_pipe_bytes_saved_per_frame=230400`
  - raw pipe/read improved:
    - reader avg `37.968ms -> 17.739ms`
    - reader slow count `47 -> 24`
    - stdout throughput `24273.288 -> 38965.867` bytes/ms
  - end-to-end throughput regressed:
    - output throughput `25.816fps -> 22.150fps`
    - completed latency avg `1309.796ms -> 2037.903ms`
    - pending age avg `803.227ms -> 1709.438ms`
    - output lag to selected `46 -> 88`
    - bounded lookup hits `6 -> 3`
  - BGR24-to-BGRA conversion cost was large:
    - total `8636ms`
    - count `329`
    - average about `26.25ms/frame`
  - interpretation:
    - raw pipe bytes hypothesis is PARTIAL PASS
    - `scaled-bgr24` adoption is HOLD / FAIL
    - default BGRA remains the safer runtime path
    - next throughput candidate should examine conversion/direct-render path
      and FFmpeg scale path split before any default change
    - detailed conversion/direct-render planning now lives in
      `docs/operations/continuous-pixel-conversion-plan.md`

- latest completed correspondence rerun:
  - `S:\stream-sync\manual-logs\two-client-completed-correspondence-rerun-20260528-010504`
  - evidence is VALID on
    `C:\streamsync-target\stream-sync-rerun\debug\stream-sync-switcher.exe`
    LastWriteTime `2026/05/28 1:05:18`
  - source side remains about `29fps`:
    - client1 `29.443fps`
    - client2 `29.112fps`
    - server `frames_queued=1800`
  - continuous output throughput dropped to `17.151fps`
  - raw output rate was `15806358.974` bytes/sec
  - output interval avg/max was `53.770ms` / `603ms`
  - stdout read throughput was `16031.068` bytes/ms
  - reader full-frame avg/max/slow was `57.488ms` / `1176ms` / `43`
  - completed correspondence latency avg/max/latest was
    `2624.940ms` / `5258ms` / `5251ms`
  - pending correspondence avg/max was `2540.606ms` / `5300ms`
  - interpretation:
    - completed and pending correspondence both show seconds of pipeline delay
    - output throughput is far below source cadence
    - next candidate should be raw BGRA pipe / stdout throughput and FFmpeg
      scale path split, not threshold tuning

- latest output availability rerun:
  - `S:\stream-sync\manual-logs\two-client-output-availability-rerun-20260527-173716`
  - evidence is VALID on
    `C:\streamsync-target\stream-sync-rerun\debug\stream-sync-switcher.exe`
    LastWriteTime `2026/05/27 17:25:51`
  - client FFmpeg is recovered:
    - client1 `frames_sent=900`, `effective_output_fps=29.538`
    - client2 `frames_sent=900`, `effective_output_fps=28.694`
  - server transport / queue is PASS:
    - `packets_received=35935`
    - `frames_queued=1800`
    - `per_client_queued_frames=player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`
  - continuous feed is PASS for current scope:
    - `continuous_feed_frame_received_count=453`
    - `continuous_feed_enqueued_count=423`
    - `continuous_decode_input_frame_count=431`
  - continuous output is below source cadence:
    - `continuous_decode_output_frame_count=316`
    - `continuous_decode_output_throughput_fps=21.269`
    - `continuous_decode_output_bytes_per_sec=19601911.557`
    - `continuous_decode_output_frame_interval_ms_avg=43.006`
    - `continuous_decode_output_frame_interval_ms_max=447`
  - reader full-frame latency aligns with the throughput gap:
    - `continuous_decode_reader_full_frame_elapsed_ms_avg=46.430`
    - `continuous_decode_reader_full_frame_elapsed_ms_max=1125`
    - `continuous_decode_reader_full_frame_slow_count=42`
    - `continuous_decode_stdout_read_waiting_for_full_frame=true`
  - conclusion: client/server/feed are not the main bottleneck; continuous
    output pipeline / stdout reader / FFmpeg scale-output path are the next
    planning target
- latest reverse-order lag threshold A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-lag-reverse-ab-rerun-20260527-164258`
  - comparison is VALID
  - lag8 improved output throughput, output lag, and reader average latency versus lag5
  - render FPS stayed near-tied, so default `8` promotion is HOLD
- latest matched rerun:
  - `S:\stream-sync\manual-logs\two-client-ab-rerun-20260522-103943`
- validity:
  - OFF and ON used the same `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
  - source fps mismatch is not noisy enough to reject the comparison
  - OFF client fps was `27.806` / `27.167`
  - ON pasted client evidence includes `28.134`
- OFF no suppression:
  - `continuous_decode_slot0_one_shot_suppression_enabled=false`
  - `continuous_decode_output_throughput_fps=20.129`
  - `continuous_decode_output_lag_to_selected_frames=17`
  - `continuous_decode_latest_input_minus_latest_output_lag=20`
  - competing one-shot `37` attempts / `5401ms`
  - continuous render use `0`
  - bounded lookup hit `0`
  - render FPS `11.594`
- ON slot0 suppression:
  - `continuous_decode_slot0_one_shot_suppression_enabled=true`
  - `continuous_decode_slot0_one_shot_suppressed_count=255`
  - suppression reasons `continuous_not_ready:27|stale:228|future:0|unknown:0`
  - render safety `decode_deferred_placeholder:255|unknown:0`
  - `continuous_decode_output_throughput_fps=26.814`
  - `continuous_decode_output_lag_to_selected_frames=8`
  - `continuous_decode_latest_input_minus_latest_output_lag=33`
  - competing one-shot `13` attempts / `942ms`
  - continuous render use `11`
  - bounded lookup hit `11`
  - render FPS `17.401`

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
   - 2026-05-28 adds an opt-in comparison mode only:
     `--continuous-decoder-output-pipeline-experiment scaled-bgr24`.
     Default remains raw BGRA. In `scaled-bgr24`, FFmpeg keeps the same scale
     filter and emits `bgr24` rawvideo, reducing pipe bytes/frame from
     `921600` to `691200`; the switcher reader converts BGR24 back to BGRA
     before render.

4. Continuous stdout reader
   - The reader allocates one `expected_len` buffer for each output frame read attempt.
   - For `640x360` BGRA, `expected_len = 640 * 360 * 4 = 921600` bytes/frame.
   - The reader loops until one full raw frame is read from stdout.
   - Only after a full raw frame is available can it pop the correspondence metadata and emit a decoded output event.
   - In the opt-in `scaled-bgr24` experiment, the full-frame read boundary is
     `691200` bytes. Pixel conversion timing is reported separately so reader
     full-frame latency remains the stdout read metric, not the conversion
     metric.

5. Render-side drain and lookup
   - Render drains decoded events into the bounded decoded cache/key order.
   - Exact lookup runs first, bounded-lag lookup runs second, and one-shot fallback remains the safety path.

## Throughput Shape
- Latest output availability rerun reinforces the same shape:
  - source cadence is about `29fps`
  - continuous output is `21.269fps`
  - pending correspondence is `115`
  - reader average full-frame read time is `46.430ms`
  - output frame interval average is `43.006ms`
  - output bytes/sec is about `19.6 MB/s`
- This is consistent with an output pipeline backlog. It is not explained by
  client FFmpeg, server queueing, or feed intake in this rerun.
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
The matched A/B evidence for one-shot double-load isolation now lives in
`docs/operations/continuous-one-shot-double-load-plan.md`. The bounded lookup
threshold branch now lives in `docs/operations/continuous-decoded-lookup-plan.md`
and is HOLD / candidate after the reverse-order A/B. The next docs-first
candidate has moved to output availability / throughput. The output pipeline
experiment design now lives in
`docs/operations/continuous-output-pipeline-experiment-plan.md`.

1. Pending correspondence / output availability diagnostics
   - First safe code-slice candidate; implemented as a diagnostics-only summary
     slice for slot0 / two-real / opt-in continuous.
   - Keep it diagnostics-only, slot0 / two-real / opt-in continuous only.
   - Summary now measures pending correspondence age/range, latest
     input/selected to output frame gaps, and output availability not-ready /
     stale / future counts before changing policy.
   - Latest output availability rerun validates this slice and points to stale /
     output backlog as dominant over not-ready.

2. Completed correspondence latency diagnostics
   - Recommended first code slice in
     `docs/operations/continuous-output-pipeline-experiment-plan.md`.
   - Keep it diagnostics-only and use it to compare pending backlog age against
     successful input-to-output latency before changing output behavior.
   - Implemented on 2026-05-28 with completed count, avg/max/latest latency,
     slow count/threshold, and completed frame-id min/max summary fields.
   - Runtime VALID on
     `S:\stream-sync\manual-logs\two-client-completed-correspondence-rerun-20260528-010504`.
   - Result: completed output latency and pending backlog age are both seconds
     late, so this diagnostics slice points downstream to output pipeline
     throughput.

3. Raw BGRA pipe throughput / stdout reader buffering experiment
   - Next opt-in output pipeline candidate.
   - Use latest availability evidence to separate pipe/read/copy cadence from
     FFmpeg decode/scale/output cadence.
   - Preserve full-frame correctness; do not emit partial raw frames.
   - Any reader buffering behavior change remains opt-in and summarized.

4. FFmpeg scale path comparison
   - Next opt-in planning candidate after or alongside stdout/raw BGRA pipe
     throughput.
   - Current path stays `scale=640:360:flags=neighbor` plus raw BGRA by
     default.
   - After the `scaled-bgr24` A/B, do not combine scale split with direct
     render or unsafe conversion in the first follow-up slice.

5. BGR24 conversion optimization / direct render review
   - New first docs-first candidate after the output pipeline A/B.
   - Start with buffer reuse or safe scalar conversion if code is selected.
   - Direct BGR24 render path is wider because renderer-facing buffers,
     composition, GDI, and OBS-friendly output are BGRA-oriented.
   - Source of truth:
     `docs/operations/continuous-pixel-conversion-plan.md`.
   - Source-size raw output may be heavier than the current `921600`
     bytes/frame path, so do not adopt it without total pipeline evidence.

5. Reader blocking phase diagnostics
   - Add only if output pipeline attribution remains unclear.
   - Split waiting-for-first-byte, partial stdout progress, full-frame wait, and
     completed frame phases without changing read semantics.

6. Queue/cache policy diagnostics
   - Keep decoded cache bound `30` unchanged.
   - Diagnose whether cache bound drops are a symptom of output lag or are
     hiding otherwise safe decoded frames.

7. One-shot fallback double-load isolation
   - Already has strong opt-in evidence from suppression ON.
   - Demote from next main culprit because latest availability evidence points
     to output backlog/stale output as the central shape.
   - Keep production default unchanged.

## Design Decision
- Matched A/B suppression comparison is VALID寄り on `20260522-103943`.
- Suppression ON is useful opt-in isolation evidence: competing one-shot load fell
  from `37` attempts / `5401ms` to `13` / `942ms`, while throughput, continuous
  render use, bounded lookup hit, and render FPS all improved.
- One-shot double-load is now a strong contributor candidate for this slice, but
  this still does not justify a single-cause FPS conclusion or default suppression.
- The reverse-order lag threshold A/B rerun stays consistent with that reading:
  lag8 is a small PARTIAL PASS, but the default `8` change remains HOLD because
  render FPS is near-tied and not-ready rejects still remain.
- The threshold branch stays as a held adoption candidate, not the next default
  move.
- The latest output availability rerun strengthens this: allowed lag tuning
  alone cannot make a `21fps` continuous output pipeline match a `29fps` source
  or clear `115` pending correspondences.
- Stale/not-ready suppression reasons and reverse-order not-ready rejects remain
  visible, so the next code candidate moves to output availability diagnostics:
  pending correspondence pressure, stdout reader full-frame latency, raw BGRA
  pipe throughput, and queue/cache policy diagnostics.
- Pixel-format, scale-path, reader-buffering, and additional FFmpeg args remain
  opt-in experiment candidates after diagnostics identify a likely bottleneck.
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
- Matched suppression A/B evidence is now runtime-valid:
  - OFF throughput `20.129fps`, competing one-shot `37` attempts / `5401ms`,
    bounded lookup/render use `0`
  - ON throughput `26.814fps`, competing one-shot `13` attempts / `942ms`,
    bounded lookup/render use `11`
  - ON suppression count `255`
- Keep suppression as an opt-in isolation path. The threshold branch remains a
  held candidate in the lookup plan; the next main line follows the output
  availability plan rather than changing defaults.

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
