<!-- stream-sync/docs/operations/continuous-output-pipeline-experiment-plan.md -->

# Continuous Output Pipeline Experiment Plan

Last updated: 2026-05-28

## Purpose
- Design the next docs-first candidates after output availability diagnostics
  became runtime-valid.
- Keep the next code slice opt-in, slot0-only, two-real preview loop only, and
  `--enable-continuous-stream-decoder` only.
- Separate continuous output backlog causes before changing defaults:
  - completed correspondence latency
  - stdout/raw BGRA pipe throughput
  - FFmpeg scale path cost
  - reader blocking phases
- Keep Production Readiness as FAIL.

## Latest Evidence
- latest output availability rerun:
  - `S:\stream-sync\manual-logs\two-client-output-availability-rerun-20260527-173716`
- Source side is healthy for this slice:
  - client1 `frames_sent=900`, `effective_output_fps=29.538`
  - client2 `frames_sent=900`, `effective_output_fps=28.694`
  - server `frames_queued=1800`
- Continuous output is behind source cadence:
  - `continuous_decode_input_frame_count=431`
  - `continuous_decode_output_frame_count=316`
  - `continuous_decode_output_throughput_fps=21.269`
  - `continuous_decode_pending_correspondence_count=115`
  - `continuous_decode_pending_correspondence_age_ms_avg=1948.809`
  - `continuous_decode_pending_correspondence_age_ms_max=3939`
  - `continuous_decode_latest_input_to_output_frame_gap=115`
  - `continuous_decode_output_lag_to_selected_frames=99`
  - `continuous_decode_reader_full_frame_elapsed_ms_avg=46.430`
  - `continuous_decode_reader_full_frame_elapsed_ms_max=1125`
  - `continuous_decode_reader_full_frame_slow_count=42`
  - `continuous_decode_output_availability_stale_count=238`
  - `continuous_decode_output_availability_not_ready_count=22`
- Verdict:
  - output availability diagnostics are VALID
  - client / server / feed are PASS
  - not-ready is secondary in this rerun
  - stale output, pending correspondence, reader full-frame latency, and output
    backlog are dominant
  - threshold tuning alone is insufficient

## Safety Constraints
- No stale unrestricted fallback.
- No decoded frame newer than the selected / requested frame.
- Keep same-source guard.
- Keep exact lookup first, bounded lookup second, and one-shot fallback /
  suppression safety paths.
- Do not change default allowed lag.
- Do not default one-shot suppression.
- Do not change feed max count.
- Do not change FFmpeg args, pixel format, or scale path without an explicit
  opt-in experiment flag.
- Do not expand to slot1, 4-client, server/client/protocol, request/response
  persistent decoder, or GPU decode.

## Candidate Priority
1. Completed correspondence latency diagnostics
   - Best first code slice because it is additive diagnostics and does not
     change FFmpeg output shape or read semantics.
   - It complements pending age: pending age shows unfinished backlog, while
     completed latency shows how long successful input-to-output matches took.

2. stdout/raw BGRA pipe throughput opt-in experiment planning
   - Next behavior-adjacent planning target.
   - Current output is `-f rawvideo -pix_fmt bgra pipe:1` with expected frame
     bytes `921600`.
   - Use existing metrics plus any experiment flag summary to decide whether
     full-frame pipe/read/materialization cost is limiting output cadence.

3. FFmpeg scale path split opt-in experiment planning
   - Plan how to separate scale cost from raw pipe/output cost.
   - Current path is `-vf scale=640:360:flags=neighbor` plus BGRA rawvideo.
   - Source-size raw output can multiply bytes/frame and is not a safe first
     default candidate.

4. Reader blocking phase diagnostics
   - Useful if attribution remains unclear after completed correspondence
     latency and pipe/scale planning.
   - Split the reader state without producing partial decoded frames.

## Completed Correspondence Latency Diagnostics
Question:

- For outputs that do complete, how long did metadata wait from correspondence
  queue insertion until the reader emitted the full raw frame?

Proposed summary fields:

- `continuous_decode_completed_correspondence_latency_ms_avg`
- `continuous_decode_completed_correspondence_latency_ms_max`
- `continuous_decode_completed_correspondence_latency_slow_count`
- `continuous_decode_completed_correspondence_latency_slow_threshold_ms`
- `continuous_decode_completed_correspondence_count`

Interpretation:

- If completed latency avg/max is close to pending age, the backlog is likely
  steady output-pipeline pressure.
- If completed latency is low while pending age is high, investigate mismatched
  correspondence, reader blocking edge cases, or stalled outputs.
- If completed latency aligns with reader full-frame elapsed, stdout/FFmpeg
  output cadence is the first suspect.

Implementation boundary if code is selected:

- diagnostics-only
- slot0 / two-real / opt-in continuous only
- no FFmpeg args changes
- no lookup policy changes
- no fallback policy changes

## stdout/raw BGRA Pipe Throughput Experiment Plan
Current baseline:

- FFmpeg output:
  - `-f rawvideo`
  - `-pix_fmt bgra`
  - `pipe:1`
- Scaled frame size:
  - `640x360`
  - expected frame bytes `921600`
- Current evidence:
  - output bytes/sec `19601911.557`
  - stdout read throughput `19849.073` bytes/ms
  - output frame interval avg `43.006ms`
  - reader full-frame avg `46.430ms`

Questions:

- Is the raw BGRA pipe/read/materialization boundary limiting frame delivery?
- Are full-frame reads slow because of pipe cadence, reader scheduling, FFmpeg
  output cadence, or per-frame buffer materialization?
- Does output bytes/sec scale with output fps, or does it plateau below source
  cadence?

Metrics to compare:

- `continuous_decode_output_bytes_per_sec`
- `continuous_decode_stdout_read_throughput_bytes_per_ms`
- `continuous_decode_reader_full_frame_elapsed_ms_avg`
- `continuous_decode_reader_full_frame_elapsed_ms_max`
- `continuous_decode_reader_full_frame_slow_count`
- `continuous_decode_output_frame_interval_ms_avg`
- `continuous_decode_output_frame_interval_ms_max`
- `continuous_decode_pending_correspondence_count`
- `continuous_decode_pending_correspondence_age_ms_avg`
- `continuous_decode_latest_input_to_output_frame_gap`

Opt-in shape if implemented later:

- Add an explicit experiment flag and summary field that names the active
  output-pipeline experiment.
- Preserve full-frame correctness; never emit partial raw frames.
- Keep default raw BGRA output unchanged when the flag is absent.
- Keep same-source and no-future-frame guards unchanged.

## FFmpeg Scale Path Split Experiment Plan
Current baseline:

- `-vf scale=640:360:flags=neighbor`
- `-f rawvideo -pix_fmt bgra pipe:1`
- expected bytes/frame `921600`

Questions:

- Is FFmpeg decode+scale+BGRA conversion the dominant cost?
- Is the pipe/rawvideo output boundary the dominant cost?
- Does removing or changing scale reduce latency enough to justify a later
  renderer-side responsibility discussion?

Candidate comparisons:

1. Baseline scaled BGRA raw output
   - Current default and control case.
   - Must remain the default.

2. Scale-path split diagnostics without display policy changes
   - Prefer an opt-in diagnostic variant that reports the experiment shape and
     keeps render safety unchanged.
   - Use the same summary fields as the pipe experiment for comparison.

3. Source-size raw output
   - High-risk comparison only.
   - A 1280x720 BGRA frame would be `3686400` bytes, about 4x the current
     `640x360` output.
   - Do not adopt as default and do not treat it as an obvious improvement.

4. Alternative scale/output placement
   - Planning-only until total pipeline cost is known.
   - Moving scaling responsibility out of FFmpeg can shift cost into renderer
     copy/resize work and must be measured end to end.

Acceptance for any future scale-path code slice:

- explicit opt-in flag
- summary includes active experiment name, output dimensions, pixel format,
  expected bytes/frame, output fps, reader latency, pending correspondence, and
  render continuous use
- no default behavior change
- no source-size raw output default

## Reader Blocking Phase Diagnostics
Question:

- When `read_exact(expected_len)` is slow, where is the reader spending time?

Possible phases:

- waiting for first byte of a frame
- partial stdout progress below one full frame
- waiting for remaining bytes to complete the full frame
- completed full-frame read and correspondence pop
- output event send to render-side drain

Boundary:

- Do not emit partial frames.
- Do not change blocking semantics in the first diagnostic slice unless a later
  opt-in reader-buffering experiment is explicitly selected.

## Next Recommendation
- First code slice: completed correspondence latency diagnostics.
- Then compare whether stdout/raw BGRA pipe throughput or FFmpeg scale path
  split should be the first opt-in behavior experiment.
- Keep threshold branch HOLD / candidate and one-shot suppression as supporting
  evidence, not the next main default policy.
- Production Readiness remains FAIL.
