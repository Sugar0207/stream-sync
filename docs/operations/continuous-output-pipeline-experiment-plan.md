<!-- stream-sync/docs/operations/continuous-output-pipeline-experiment-plan.md -->

# Continuous Output Pipeline Experiment Plan

Last updated: 2026-06-03

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
- latest Program-first ProgramOutput validation:
  - mode:
    `--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode`
  - classification: ProgramOutput PASS / near-MVP for OBS output
  - OBS captured `StreamSync Program Output` and did not accidentally capture
    `StreamSync 4-view Output`
  - no Preview labels / borders / debug UI mixed into Program
  - black / placeholder: none
  - perceived stutter: small
  - Program output:
    - `program_render_effective_fps=21.795`
    - `effective_program_render_fps=21.795`
    - `continuous_decode_output_throughput_fps=27.102`
    - `program_decode_fps=27.102`
    - `program_render_used_continuous_latest_count=2841`
    - `program_render_used_one_shot_fallback_count=0`
  - one-shot pressure:
    - `one_shot_decode_attempt_count=0`
    - `program_first_suppressed_preview_one_shot_decode_count=2872`
    - `program_first_remaining_one_shot_decode_count=0`
  - Preview in this validation mode:
    - `StreamSync 4-view Output` was not displayed
    - `frames_rendered=0`
    - `clean_output_render_result_kind=NoRenderableQuadView`
    - `output_width=none`
    - `output_height=none`
  - Decision:
    - keep `--program-first-validation-mode` as ProgramOutput validation mode,
      not final operator mode
    - Preview may be absent, stale, or reduced in this mode
    - do not add low-frequency Preview refresh yet; a useful refresh path
      without default non-Program one-shot decode pressure needs a separate
      low-cost Preview / multiview design
    - new diagnostics expose the current decision:
      `program_first_preview_visible`,
      `program_first_preview_refresh_interval`,
      `program_first_preview_refresh_count`, and
      `program_first_preview_suppressed_count`
  - Follow-up operator low-cost Preview restore candidate:
    - `--program-first-preview-refresh-interval <ticks>` is now the narrow
      opt-in validation shape for restoring a human-facing Preview cadence
      without changing default behavior.
    - It keeps `--program-first-validation-mode` Program-first semantics and
      only attempts Preview compose/render every N ticks.
    - Non-refresh ticks continue to skip/reuse Preview composition so Program
      rendering stays prioritized.
    - Non-Program Preview one-shot suppression remains active; refresh ticks
      should compose from already available decoded/cache/continuous data or
      clearly diagnose that no usable Preview frame exists yet.
    - The key guard metric is
      `operator_preview_forced_one_shot_decode_count`, which should remain `0`
      in the intended Program continuous + smooth-latest operator validation
      run.
  - Latest low-cost Preview operator validation with interval `30` made the
    Preview window visible, but monitoring failed:
    `operator_preview_refresh_success_count=4`,
    `operator_preview_refresh_skipped_count=2900`,
    `operator_preview_render_effective_fps=0.031`, and client1 ended black /
    `DecodeDeferred:ContinuousOneShotSuppressed`.
  - New follow-up:
    `--program-first-preview-decode-refresh-interval <ticks>` allows at most
    one non-Program Preview one-shot decode on matching Preview refresh ticks.
    Watch `operator_preview_decode_refresh_attempt_count`,
    `operator_preview_decode_refresh_success_count`,
    `operator_preview_decode_refresh_source_counts`,
    `operator_preview_decode_refresh_budget_exceeded_count`, and
    `operator_preview_non_program_visible_count` while keeping
    `program_render_used_one_shot_fallback_count` near zero.
  - Latest decode-refresh validation with
    `--program-first-preview-refresh-interval 10` and
    `--program-first-preview-decode-refresh-interval 30` improved the
    non-Program Preview slot but exposed a Program-slot reuse gap:
    - Program stayed selected-only and used continuous latest:
      `program_render_effective_fps=19.884`,
      `program_render_used_continuous_latest_count=2846`,
      `program_render_used_one_shot_fallback_count=0`,
      `program_output_black_frame_render_count=0`, and
      `program_output_placeholder_render_count=0`.
    - Preview decode refresh made client1 / slot0 visible:
      `operator_preview_decode_refresh_success_count=96`,
      `operator_preview_decode_refresh_source_counts=slot0:96|slot1:0`,
      and `operator_preview_non_program_visible_count=1`.
    - Program-selected player2 / slot1 still ended black / decode-deferred in
      Preview even though ProgramOutput rendered player2 from continuous
      latest.
    - Current implementation adds an opt-in Preview-only reuse path for the
      explicit Program-selected slot. It reuses `continuous_latest` first and
      `last_valid_program` second, without changing ProgramOutput rendering or
      default Preview behavior.
    - Next validation should keep both refresh knobs at `10` and `30`, and
      watch `operator_preview_reused_program_frame_count`,
      `operator_preview_program_slot_visible_count`,
      `operator_preview_program_slot_reuse_source`,
      `operator_preview_program_slot_black_count`,
      `operator_preview_decode_refresh_source_counts`, and
      `program_render_used_one_shot_fallback_count`.
- latest optimized BGR24 A/B rerun:
  - root:
    `S:\stream-sync\manual-logs\two-client-optimized-bgr24-ab-rerun-20260528-103130`
  - default:
    `S:\stream-sync\manual-logs\two-client-optimized-bgr24-ab-rerun-20260528-103130\default-bgra`
  - optimized:
    `S:\stream-sync\manual-logs\two-client-optimized-bgr24-ab-rerun-20260528-103130\optimized-scaled-bgr24`
- Validity:
  - FFmpeg available before runtime.
  - Build PASS with existing dead-code warnings only.
  - Same `C:\streamsync-target\stream-sync-rerun\debug\*.exe` runtime.
  - Both servers queued `1800` frames:
    `player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`.
  - Client FPS was close enough for useful comparison.
- Default BGRA:
  - mode `default`
  - pixel format `bgra`
  - bytes/frame `921600`
  - output throughput `26.272fps`
  - reader full-frame avg/max/slow `36.604ms` / `1157ms` / `32`
  - stdout read throughput `25177.811` bytes/ms
  - completed count `328`
  - completed latency avg/max/latest `1123.244ms` / `1349ms` / `1066ms`
  - pending count `35`
  - pending age avg/max `733.029ms` / `1361ms`
  - latest input-output gap `35`
  - output lag to selected `33`
  - bounded lookup hits `11`
  - render FPS after first render `15.883`
- Optimized scaled BGR24:
  - mode `scaled-bgr24`
  - pixel format `bgr24`
  - bytes/frame `691200`
  - pipe bytes saved/frame `230400`
  - output throughput `26.092fps`
  - reader full-frame avg/max/slow `31.108ms` / `1139ms` / `24`
  - stdout read throughput `22219.387` bytes/ms
  - completed count `389`
  - completed latency avg/max/latest `1350.666ms` / `1659ms` / `1466ms`
  - pending count `44`
  - pending age avg/max `932.068ms` / `1653ms`
  - latest input-output gap `46`
  - output lag to selected `28`
  - bounded lookup hits `4`
  - render FPS after first render `16.361`
  - pixel conversion total/max/count `2105ms` / `8ms` / `389`
  - pixel conversion avg about `5.41ms/frame`
  - conversion buffer reuse/allocation `389` / `0`
  - bytes written total/per-frame `358502400` / `921600`
  - conversion mode `bgr24-in-place-safe-scalar`
- Verdict:
  - The A/B is VALID-ish / useful evidence.
  - Conversion optimization is PASS.
  - Raw pipe bytes remain PARTIAL PASS: bytes/frame and reader avg improved.
  - Optimized `scaled-bgr24` is PARTIAL PASS but adoption HOLD.
  - Default BGRA remains the safer runtime path because completed latency,
    pending age/count, output throughput, and bounded lookup hits still favor
    default overall.
  - Next candidate should be FFmpeg scale path split opt-in experiment or
    reader/completed latency breakdown diagnostics.

## Current Code Slice
- 2026-05-28 first FFmpeg scale path split slice is implemented as opt-in
  `no-scale-bgra`.
- Scope:
  - slot0 continuous decoder only
  - two-real handoff preview loop only
  - requires `--enable-continuous-stream-decoder`
  - selected with
    `--continuous-decoder-output-pipeline-experiment no-scale-bgra`
- Behavior:
  - removes continuous FFmpeg `-vf scale=640:360:flags=neighbor`
  - keeps `-pix_fmt bgra`
  - reads source-size raw BGRA from stdout
  - leaves default `default` mode as scaled 640x360 BGRA
  - leaves optimized `scaled-bgr24` as scaled 640x360 BGR24 with safe scalar
    in-place BGRA expansion
- Diagnostics now expose:
  - `continuous_decode_output_pipeline_scale_mode`
  - `continuous_decode_output_source_width`
  - `continuous_decode_output_source_height`
  - `continuous_decode_output_scaled_width`
  - `continuous_decode_output_scaled_height`
  - `continuous_decode_output_scale_removed_count`
  - `continuous_decode_output_scale_path_experiment_enabled`
- Expected bytes:
  - default BGRA: `640 * 360 * 4 = 921600`
  - scaled BGR24 pipe: `640 * 360 * 3 = 691200`
  - no-scale BGRA at 1280x720 source: `1280 * 720 * 4 = 3686400`
- Verdict before runtime rerun:
  - implementation wiring is ready for human-side A/B
  - `no-scale-bgra` is diagnostics-only, not an adoption candidate
  - default BGRA remains the safe path
  - optimized `scaled-bgr24` remains adoption HOLD
  - Production Readiness remains FAIL

- latest output pipeline A/B rerun:
  - root:
    `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200`
  - default:
    `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200\default-bgra`
  - scaled:
    `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200\scaled-bgr24`
- Validity:
  - FFmpeg was available before runtime.
  - Build was PASS with existing dead-code warnings only.
  - Both runs used the same build / same
    `C:\streamsync-target\stream-sync-rerun\debug\*.exe`.
  - Default and scaled servers both queued `1800` frames:
    `player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`.
  - Client FPS was close enough for comparison: default around `29fps`, scaled
    around `28.5-29.6fps`.
- Default BGRA:
  - mode `default`
  - pixel format `bgra`
  - bytes/frame `921600`
  - pipe bytes saved/frame `0`
  - output throughput `25.816fps`
  - reader full-frame avg/max/slow `37.968ms` / `1199ms` / `47`
  - stdout read throughput `24273.288` bytes/ms
  - completed correspondence count `402`
  - completed latency avg/max/latest `1309.796ms` / `1827ms` / `1591ms`
  - pending count `44`
  - pending age avg/max `803.227ms` / `1646ms`
  - latest input-output gap `45`
  - output lag to selected `46`
  - bounded lookup hits `6`
  - stale/not-ready `234` / `24`
  - render FPS `14.626`
- Scaled BGR24:
  - mode `scaled-bgr24`
  - pixel format `bgr24`
  - bytes/frame `691200`
  - pipe bytes saved/frame `230400`
  - output throughput `22.150fps`
  - reader full-frame avg/max/slow `17.739ms` / `1145ms` / `24`
  - stdout read throughput `38965.867` bytes/ms
  - completed correspondence count `329`
  - completed latency avg/max/latest `2037.903ms` / `3508ms` / `3508ms`
  - pending count `105`
  - pending age avg/max `1709.438ms` / `3502ms`
  - latest input-output gap `106`
  - output lag to selected `88`
  - bounded lookup hits `3`
  - stale/not-ready `236` / `25`
  - render FPS `15.688`
  - pixel conversion total/max/count `8636ms` / `41ms` / `329`
  - pixel conversion average is about `26.25ms/frame`
- Verdict:
  - The A/B is VALID-ish / useful evidence.
  - `scaled-bgr24` wiring, FFmpeg args, expected bytes, and summary fields are
    PASS.
  - Raw stdout bytes are a real factor: reader avg improved from `37.968ms` to
    `17.739ms`, and stdout throughput improved from `24273.288` to
    `38965.867` bytes/ms.
  - Raw pipe bytes hypothesis is PARTIAL PASS.
  - End-to-end pipeline is worse with `scaled-bgr24`: output throughput,
    completed latency, pending age, output lag, and bounded lookup hits all
    regress.
  - BGR24-to-BGRA conversion cost is the new strong bottleneck candidate.
  - `scaled-bgr24` remains a useful diagnostic but is not an adoption candidate
    yet.
  - Default BGRA remains the safer runtime path.
  - Detailed conversion/direct-render follow-up now lives in
    `docs/operations/continuous-pixel-conversion-plan.md`.

- latest completed correspondence rerun:
  - `S:\stream-sync\manual-logs\two-client-completed-correspondence-rerun-20260528-010504`
- Validity:
  - `ffmpeg -version` succeeded before runtime
  - detected FFmpeg version: `8.1.1-full_build-www.gyan.dev`
  - `stream-sync-switcher` compiled with existing dead-code warnings only
  - switcher binary:
    `C:\streamsync-target\stream-sync-rerun\debug\stream-sync-switcher.exe`
    LastWriteTime `2026/05/28 1:05:18`
- Source / transport / feed are PASS for this slice:
  - client1 `frames_sent=900`, `effective_output_fps=29.443`
  - client2 `frames_sent=900`, `effective_output_fps=29.112`
  - server `frames_queued=1800`
  - server per-client frames:
    `player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`
- Completed correspondence diagnostics are VALID:
  - `continuous_decode_completed_correspondence_count=301`
  - `continuous_decode_completed_correspondence_latency_ms_avg=2624.940`
  - `continuous_decode_completed_correspondence_latency_ms_max=5258`
  - `continuous_decode_completed_correspondence_latency_slow_count=301`
  - `continuous_decode_completed_correspondence_latest_latency_ms=5251`
  - completed frame range `4..373`
- Pending / gap evidence confirms the same backlog shape:
  - `continuous_decode_pending_correspondence_count=137`
  - `continuous_decode_pending_correspondence_age_ms_avg=2540.606`
  - `continuous_decode_pending_correspondence_age_ms_max=5300`
  - pending frame range `371..529`
  - `continuous_decode_latest_input_to_output_frame_gap=156`
  - `continuous_decode_output_lag_to_selected_frames=150`
- Output pipeline evidence:
  - source is about `29fps`
  - continuous output is `17.151fps`
  - `continuous_decode_output_bytes_per_sec=15806358.974`
  - `continuous_decode_output_frame_interval_ms_avg=53.770`
  - reader full-frame avg `57.488ms`, max `1176ms`, slow count `43`
- Verdict:
  - completed latency and pending age are both around `2.5s` or more
  - every completed correspondence exceeded the `66ms` slow threshold
  - not-ready `19` is small compared with stale `228`
  - continuous output pipeline is not keeping up with source cadence
  - threshold tuning alone is insufficient
  - next candidate should move to raw BGRA pipe / stdout throughput and FFmpeg
    scale path split planning

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

Implementation status:

- 2026-05-28 first code slice implemented.
- Summary now exposes:
  - `continuous_decode_completed_correspondence_count`
  - `continuous_decode_completed_correspondence_latency_ms_avg`
  - `continuous_decode_completed_correspondence_latency_ms_max`
  - `continuous_decode_completed_correspondence_latency_slow_count`
  - `continuous_decode_completed_correspondence_latency_slow_threshold_ms`
  - `continuous_decode_completed_correspondence_frame_id_min`
  - `continuous_decode_completed_correspondence_frame_id_max`
  - `continuous_decode_completed_correspondence_latest_latency_ms`
- Pending correspondence age remains the unfinished backlog metric. Completed
  correspondence latency is a separate completed-output metric measured from
  correspondence queue insertion to reader-emitted output.
- Behavior is unchanged: no FFmpeg args, pixel format, scale path, feed max,
  lookup order, threshold default, suppression default, or fallback policy
  change.

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

Implementation status:

- 2026-05-28 first code slice implemented for slot0 / two-real /
  `--enable-continuous-stream-decoder` only.
- New opt-in flag:
  - `--continuous-decoder-output-pipeline-experiment <mode>`
- Implemented modes:
  - `default`
  - `scaled-bgr24`
- Default mode remains unchanged:
  - `-vf scale=640:360:flags=neighbor`
  - `-f rawvideo`
  - `-pix_fmt bgra`
  - expected stdout frame bytes `640 * 360 * 4 = 921600`
- `scaled-bgr24` keeps the same FFmpeg scale path but changes stdout pixel
  format to `bgr24`:
  - expected stdout frame bytes `640 * 360 * 3 = 691200`
  - pipe bytes saved per frame `230400`
  - reader converts BGR24 back to BGRA before inserting into the decoded cache,
    so render remains usable and the downstream renderer contract stays BGRA.
- Summary fields added for the comparison:
  - `continuous_decode_output_pipeline_experiment_mode`
  - `continuous_decode_output_bytes_per_frame`
  - `continuous_decode_output_pipe_bytes_saved_per_frame`
  - `continuous_decode_output_pixel_convert_elapsed_ms`
  - `continuous_decode_output_pixel_convert_elapsed_ms_max`
  - `continuous_decode_output_pixel_convert_count`
- Existing pipe/read fields remain the main comparison surface:
  - `continuous_decode_ffmpeg_output_pixel_format`
  - `continuous_decode_stdout_expected_frame_bytes`
  - `continuous_decode_output_bytes_per_sec`
  - `continuous_decode_stdout_read_throughput_bytes_per_ms`
  - reader full-frame avg/max/slow
  - completed and pending correspondence latency/age
- Not implemented in this slice:
  - source-size raw output
  - `scaled-rgb24`
  - `no-scale-bgra`
  - FFmpeg scale path split
  - reader buffering behavior changes
  - lookup, fallback, threshold, suppression, feed, slot1, or 4-client changes

Runtime verdict:

- Optimized `scaled-bgr24` conversion optimization is PASS, but adoption is
  HOLD for now.
- Keep `default` BGRA as the recommended runtime path.
- Treat `scaled-bgr24` as diagnostic evidence that raw pipe bytes matter, not
  as a throughput fix.
- Next investigation should be:
  1. FFmpeg scale path split opt-in experiment
  2. reader/completed latency breakdown diagnostics
  3. direct BGR24 render path impact review only

Conversion follow-up:

- Source of truth:
  `docs/operations/continuous-pixel-conversion-plan.md`.
- Preferred next code slice, if selected, is not direct BGR24 render. Start with
  opt-in conversion optimization:
  - buffer reuse, or
  - safe scalar conversion with pre-sized output
- Direct BGR24 render remains docs-first only until the BGRA-oriented decoded
  frame, composition, GDI, and OBS-friendly output contracts are reviewed.
- 2026-05-28 first conversion optimization slice is implemented for
  `scaled-bgr24`: BGR24 is expanded to BGRA in-place with a safe reverse scalar
  loop, and summary reports reuse/allocation counts, bytes written, and
  conversion mode. The optimized path reuses the final BGRA frame buffer as the
  conversion target and should not allocate a separate conversion buffer.
  Default BGRA is unchanged.
- 2026-05-28 optimized BGR24 A/B validates this slice:
  - conversion avg improved to about `5.41ms/frame`
  - reuse/allocation counts were `389` / `0`
  - conversion mode was `bgr24-in-place-safe-scalar`
  - adoption remains HOLD because default BGRA still wins the safer end-to-end
    read.

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

## Switcher current output/render path investigation

### Findings
- Decoded frames are currently produced in two families:
  - one-shot / persistent FFmpeg decode hooks in `apps/switcher/src/lib.rs`
    produce `SwitcherDecodedFrame { pixel_format: Bgra8, pixels: Vec<u8> }`
    from Annex B H.264 payloads.
  - the opt-in two-real continuous decoder in `apps/switcher/src/main.rs`
    feeds H.264 access units to a long-running FFmpeg process and reads raw
    frames from stdout into the same renderer-facing BGRA contract.
- The standard one-shot and request/response persistent decode paths ask
  FFmpeg for `-pix_fmt bgra`; scaled output is represented by the requested
  output width/height plus `scaled_output_enabled`.
- The continuous decoder experiment modes are:
  - `default`: FFmpeg scales to slot size and emits BGRA.
  - `scaled-bgr24`: FFmpeg scales to slot size and emits BGR24; the reader
    expands it back to BGRA in-place before the frame reaches render/composition.
  - `no-scale-bgra`: FFmpeg emits source-size BGRA without the scale filter;
    this is diagnostics-only and can multiply stdout bytes substantially.
- The current 4-view / multiview path still creates one CPU-side composed BGRA
  canvas before window rendering:
  - `SwitcherFourViewQuadCompositionBoundary` builds a
    `SwitcherFourViewComposedFrame`.
  - `compose_four_view_quad_canvas` allocates/fills one BGRA canvas and copies
    renderable slot frames into fixed 2x2 rects.
  - the two-real loop can reuse a previous composed frame or perform an
    incremental update, but the output presented to the window remains one
    composed BGRA frame.
- The render abstraction exists, but it is frame-oriented rather than
  slot-renderer-oriented:
  - `SwitcherWindowRenderRuntimeHook` accepts one `SwitcherWindowRenderRequest`
    containing one `SwitcherDecodedFrameRenderInput`.
  - `SwitcherFourViewQuadRenderFacingConnectionOutput` is the clearest
    downstream seam before presentation because it preserves scheduler status,
    slot metadata, and render readiness without owning a new pixel buffer.
  - There is no current renderer backend that draws independent source frames
    into slot rectangles directly.
- OBS-facing output and human-facing preview are currently mixed at the window
  level:
  - the stable clean output window title is `StreamSync 4-view Output`.
  - the OBS-friendly wrapper scales/copies whatever 4-view/focused output is
    produced into the fixed 1280x720 validation profile and presents it through
    the same persistent GDI window.
  - there is no separate Program window/output concept yet.
- Selected-source concepts already exist, but they are preview-state concepts:
  - scheduler results preserve selected frames per slot.
  - controlled preview has `AllView` and `Focused(slot_index)` view state.
  - focused preview renders one selected slot full-window, but this is still
    under the preview/clean-output window family, not a separate ProgramOutput.
- Diagnostics are already rich around the temporary CPU-heavy path:
  - clone/copy/allocation: decoded buffer clone counts, composed buffer clone
    count, render buffer reuse/allocation, bytes copied, scale/copy elapsed
    fields.
  - decode: FFmpeg spawn/write/read/wait, stdout first-byte/full-frame,
    persistent fallback/timeout, continuous input/output/correspondence/backlog.
  - render: render call elapsed, GDI invalidate/paint/StretchDIBits, window
    lifecycle/update counters.

### Architecture implication
- Treat the current CPU-side composed BGRA path as the validated compatibility
  implementation for Preview, not as the long-term rendering architecture.
- Do not rename the current clean output path to Program. It currently carries
  both human preview and OBS capture duties, and renaming would merge concepts
  before Program semantics exist.
- The safest future seam for `ProgramOutput` is downstream of targetTime /
  handoff scheduling and decode, but before the 4-view BGRA composition:
  - reuse source selection, decoded-frame production, and per-slot/source
    metadata.
  - add a separate Program render/output owner that renders only the active
    source full-screen.
  - keep Preview responsible for 4-view/operator layout until a slot renderer
    replaces the composed BGRA canvas.
- A later Preview slot renderer can most likely attach near the current
  `SwitcherFourViewQuadRenderFacingConnectionOutput` / composition-render
  connection, because that area already carries fixed slot placement,
  renderability, selected/no-frame/waiting/source-error detail, and scheduler
  status.

### Recommended next step
- Add a docs-first `ProgramOutput` boundary plan before any implementation:
  - define `PreviewOutput` vs `ProgramOutput` responsibilities.
  - identify the active-source state owner.
  - decide whether Program initially reuses decoded BGRA frames and the existing
    `SwitcherWindowRenderRuntimeHook`, or gets a new typed output boundary.
  - keep the first implementation, if selected later, as a separate Program
    full-window output path without changing existing 4-view Preview behavior.
- Keep the current four-view/two-real preview loops unchanged until that plan
  is explicit.

## ProgramOutput boundary plan

This plan records the target boundary only. It does not rename the current
4-view output to Program and does not require renderer changes in this step.

### Responsibility split

- `PreviewOutput` is the human-facing multiview / monitoring output.
  - It may show 4 slots, a selected border, labels, status overlays,
    diagnostics, and other operator-facing UI.
  - The current implementation may continue to use one CPU-side composed BGRA
    canvas temporarily.
  - The long-term target is slot layout rendering: fixed slot rectangles,
    per-source frame rendering into each slot, then selected border / labels /
    status overlays on top.
- `ProgramOutput` is the OBS-facing selected-only output.
  - It renders only the currently selected client/source full-screen.
  - It should not include Preview labels, debug UI, slot borders, or the
    multiview layout by default.
  - Once implemented, it should be the only OBS Window Capture target.

### Boundary seam

The safest future seam for `ProgramOutput` is after handoff / targetTime /
decode selection and before the current 4-view BGRA composition path.

At that point the switcher has already selected or produced decoded BGRA frame
data for sources, but it has not yet committed those frames to the Preview-only
quad canvas. Splitting there lets `PreviewOutput` keep its multiview
composition/overlay responsibilities while `ProgramOutput` consumes the selected
source directly for full-window rendering. It also avoids treating Preview focus
or layout rendering as the final broadcast output contract.

### Selected source state ownership

- `selected_client_id` / selected source identity is the safest Program-facing
  owner because it follows the broadcast source instead of the Preview layout.
  If frame identity needs to be disambiguated, carry the matching run/session
  identity with it.
- Selected slot index is useful operator input and provenance for the current
  4-fixed-slot UI, but it is layout-coupled. It should map to a source rather
  than become the Program state by itself.
- `Focused(slot_index)` already behaves like Preview display state. Reusing it
  silently as Program state would mix monitoring focus with broadcast selection.
  If a focus action should also switch Program later, that mapping should be
  explicit in code and docs.

Recommended initial owner/name: introduce a Program-specific selection boundary
such as `ProgramSelection` / `active_program_source` with
`selected_client_id`, optional run/session identity, and optional
`selected_slot_index` provenance. Do not make `Focused(slot_index)` the Program
state name.

### First implementation choice

- Option A: reuse existing decoded BGRA frames and
  `SwitcherWindowRenderRuntimeHook` as a temporary full-window selected-frame
  renderer. This is small, but it is only safe after the Program boundary is
  named separately.
- Option B: introduce a minimal `PreviewOutput` / `ProgramOutput` boundary first
  without behavior change, then add the selected-only render path behind that
  boundary.
- Option C: wait for a new renderer abstraction before Program output. This is
  not required for the first Program slice and would delay the OBS separation.

Recommended path: B first, then A. Create the output boundary and naming before
adding behavior, then initially render Program by reusing the existing BGRA /
window-render machinery where safe. Slot layout / GPU renderer work should stay
as a later Preview optimization, not a blocker for Program separation.

### Staged implementation plan

1. Docs boundary plan only.
2. Introduce minimal `PreviewOutput` / `ProgramOutput` naming or types without
   behavior change.
3. Add selected-only Program render path using existing BGRA/window render where
   safe.
4. Make OBS capture the Program window only.
5. Later investigate slot layout rendering / GPU renderer for Preview.

### Boundary type slice status

- 2026-05-29 code boundary slice added marker/naming types only:
  - `SwitcherPreviewOutputBoundary`
  - `SwitcherProgramOutputBoundary`
  - `SwitcherProgramSelection`
- `SwitcherProgramSelection` keeps `selected_client_id` as the primary source
  identity and treats `selected_slot_index` as optional Preview-layout
  provenance.
- The current controlled-preview `Focused(slot_index)` state is documented as
  Preview display state. It is not Program state unless a later slice explicitly
  maps it to `SwitcherProgramSelection`.
- No Program render path, second window, OBS capture change, renderer rewrite,
  or 4-view Preview behavior change was added in this slice.

### Internal Program render path status

- 2026-05-29 first selected-only Program render path is implemented as an
  internal boundary only:
  - `SwitcherProgramOutputBoundary::render_selected_decoded_frame_with_runtime`
  - `SwitcherProgramOutputRenderInput`
  - `SwitcherProgramOutputRenderResult`
  - `SWITCHER_PROGRAM_OUTPUT_WINDOW_TITLE`
- The boundary requires caller-supplied `SwitcherProgramSelection` and a selected
  decoded BGRA frame. It does not derive Program selection from
  `Focused(slot_index)`.
- The selected Program frame is passed directly to the existing
  `SwitcherWindowRenderBoundary`, so there is no Preview label, multiview
  layout, selected border, debug overlay, or 4-view BGRA composition in this
  internal path.
- This is not wired into the live Preview loops yet. No Program window is
  created by default, no CLI flag activates it yet, and OBS capture remains
  unchanged.

### Opt-in live Program window status

- 2026-05-29 the internal selected-only Program render boundary is wired into
  `--four-view-two-real-handoff-preview-loop` behind the opt-in flag
  `--enable-program-output-window`.
- Default behavior remains Preview-only. Without the flag, the current
  `StreamSync 4-view Output` path and OBS capture expectations are unchanged.
- With the flag, the two-real loop creates a separate persistent render runtime
  and renders the selected Program frame to `StreamSync Program Output`.
- Program selection is not derived from `Focused(slot_index)`.
  `--program-selected-client-id <client_id>` now requests the Program source by
  primary `selected_client_id`. If the requested client maps to a known real
  slot, `selected_slot_index` is filled only as provenance.
- If no explicit Program selection is provided, the loop preserves the
  previous fallback: first renderable decoded real slot in slot-index order. If
  no decoded Program frame is available, it falls back to the first configured
  real slot identity and reports `MissingSelectedSource`.
- If explicit Program selection is provided but the requested client is missing
  or not currently renderable, ProgramOutput reports `MissingSelectedSource`
  plus `program_output_missing_selected_source_reason`.
- Summary diagnostics include `program_output_selection_mode`,
  `program_output_requested_client_id`, `program_output_selected_client_id`,
  `program_output_selected_slot_index`, and
  `program_output_missing_selected_source_reason`.
- 2026-06-01 Program stability follow-up adds the following summary fields:
  - `program_output_missing_before_first_render_count`
  - `program_output_missing_after_first_render_count`
  - `program_output_reused_previous_frame_count`
  - `program_output_placeholder_render_count`
  - `program_output_black_frame_render_count`
  - `program_output_first_render_attempt_index`
  - `program_output_first_render_elapsed_ms`
- For explicit Program selection, if the requested source has no newly decoded
  frame on a tick after a valid Program frame has already rendered, ProgramOutput
  now reuses the last valid Program frame for that same selected source. Missing
  selected-source reporting is preserved. Preview placeholders are not reused.
- The first-renderable fallback path remains the compatibility behavior only
  when no explicit Program selection is supplied.
- 2026-06-01 selected Program decode follow-up adds
  `--enable-program-continuous-decode` as an opt-in path for the same
  two-real loop. When Program output is enabled and
  `--program-selected-client-id <client_id>` maps to one of the known real
  slots, the existing single-source continuous decoder is configured for that
  selected Program source instead of the previous hard-coded first real source.
- `--enable-program-continuous-decode` is intentionally inert without
  `--enable-program-output-window` and an explicit Program client selection
  that maps to a real slot. It does not change the no-flag default, and it does
  not change the first-renderable Program fallback when no explicit selection is
  supplied.
- The selected Program continuous path keeps `selected_client_id` as the
  primary Program identity. `selected_slot_index` remains provenance only. It
  does not derive Program state from `Focused(slot_index)`.
- 2026-06-01 smooth Program playout follow-up adds
  `--program-continuous-decode-mode <mode>`:
  - `target-frame` is the default and preserves the existing exact/bounded
    target-frame lookup behavior.
  - `smooth-latest` is opt-in and Program-only. It accepts the latest available
    continuous decoded frame for the explicit Program source, even when that
    frame is older than the current target frame.
  - In `smooth-latest`, ProgramOutput prioritizes smoothness over low latency.
    Delayed Program video is acceptable for MVP if it is smooth.
  - The previous exact/bounded path remains available for future low-latency
    work.
- 2026-06-01 Program-first validation follow-up adds
  `--program-first-validation-mode`:
  - the flag is opt-in only and does not change default Preview / Program
    behavior.
  - intended validation command shape keeps Program selected-only with
    `--enable-program-output-window`,
    `--program-selected-client-id <client_id>`,
    `--enable-program-continuous-decode`, and
    `--program-continuous-decode-mode smooth-latest`.
  - after the first Preview render, Preview may reuse its previous composed
    output instead of recomposing/rendering the 4-view canvas on every tick.
  - the mode also enables Program-only decode pressure reduction after a
    previous Preview output exists. The Program continuous source keeps the
    existing continuous one-shot suppression behavior while that continuous
    process is running, and non-Program Preview sources are prevented from
    falling through to Preview-side one-shot decode.
  - Program still renders the latest available continuous decoded frame; Preview
    freshness is intentionally reduced for this validation.
  - new summary diagnostics:
    `program_first_validation_enabled`,
    `preview_compose_skipped_for_program_count`,
    `preview_compose_reused_for_program_count`,
    `program_render_loop_attempt_count`,
    `program_window_render_success_count`,
    `program_window_render_failure_count`,
    `program_render_effective_fps`, and
    `effective_program_render_fps`.
  - additional decode-suppression diagnostics:
    `program_first_suppressed_preview_one_shot_decode_count`,
    `program_first_suppressed_preview_one_shot_decode_slot_counts`,
    `program_first_program_only_decode_path_enabled`, and
    `program_first_remaining_one_shot_decode_count`.
- Manual validation confirmed that Preview and Program windows appear
  separately, Program shows one video only, Preview 4-view layout / selected
  border / labels / debug UI are not mixed into ProgramOutput, and OBS Window
  Capture lists `StreamSync Program Output`.
- OBS capture is not changed yet. The Program window is only made available as
  a future capture target.

### Latest Program OBS stability evidence

- Long OBS validation captured `StreamSync Program Output` with explicit
  `--program-selected-client-id player2`; OBS did not capture
  `StreamSync 4-view Output`.
- Program selection identity looked correct:
  - `program_output_selection_mode=explicit`
  - requested / selected client: `player2`
  - selected slot provenance: `1`
  - last result: `Rendered`
- The stability result is still FAIL / follow-up required:
  - frequent black/placeholder was observed
  - large perceived stutter was observed
  - `frames_attempted=3000`
  - `frames_rendered=2908`
  - `effective_render_fps=15.327`
  - `program_output_render_count=2777`
  - `program_output_missing_selected_source_count=223`
  - `one_shot_decode_elapsed_ms=37600`
  - `quad_view_compose_elapsed_ms=10844`
  - `render_buffer_cpu_scale_copy_elapsed_ms=6332`
  - continuous decoder was disabled
- A later Program reuse rerun still shows selected Program throughput as the
  next bottleneck rather than OBS target selection:
  - `program_output_missing_before_first_render_count=130`
  - `program_output_missing_after_first_render_count=154`
  - `program_output_reused_previous_frame_count=154`
  - `program_output_placeholder_render_count=0`
  - `program_output_black_frame_render_count=0`
  - `program_output_last_result_kind=Rendered`
  - `effective_render_fps=14.202`
  - `effective_render_fps_after_first_render=14.526`
  - `decode_attempt_count=383`
  - `decode_success_count=383`
  - `avg_decode_elapsed_ms=116.131`
  - `one_shot_decode_elapsed_ms=44474`
  - `continuous_decode_config_enabled=false`
  - `continuous_decode_runtime_enabled=false`
- The latest `smooth-latest` Program continuous validation changed the evidence:
  - Program continuous output is now structurally used:
    `program_decode_mode=continuous`,
    `program_render_used_continuous_decoded_count=2887`,
    `program_render_used_continuous_latest_count=2887`,
    `program_render_used_continuous_stale_but_accepted_count=2585`,
    `program_render_used_one_shot_fallback_count=1`.
  - Program black/placeholder counters are no longer the main issue:
    `program_output_placeholder_render_count=0`,
    `program_output_black_frame_render_count=0`,
    `program_output_missing_after_first_render_count=0`.
  - Perceived stutter remains large:
    `program_decode_fps=19.724`,
    `continuous_decode_output_throughput_fps=19.724`,
    `effective_render_fps=12.021`,
    `effective_render_fps_after_first_render=12.180`.
  - Current leading blocker candidate is shared Preview work in the same loop:
    `quad_view_compose_elapsed_ms=15432`,
    `render_buffer_cpu_scale_copy_elapsed_ms=8798`,
    `one_shot_decode_elapsed_ms=48683`.
  - The server stopped at `MaxHandoffRequestsReached` with
    `max_handoff_requests=16000`, so the next 3000-attempt validation should use
    a much larger bounded handoff budget such as `64000` or the current
    no-budget / very-large-budget validation setting.
- Interpretation: `smooth-latest` fixed the previous continuous-frame use
  problem. ProgramOutput now needs a Program-first validation path that reduces
  Preview 4-view composition / one-shot decode pressure before hotkey or
  control-pipe switching work. This is still not a renderer or OBS automation
  slice.
- New selected Program continuous decode diagnostics include:
  - `program_decode_mode`
  - `program_continuous_decode_enabled`
  - `program_continuous_decode_mode`
  - `program_continuous_decode_output_frame_count`
  - `program_continuous_decode_lookup_hit_count`
  - `program_continuous_decode_lookup_miss_count`
  - `program_render_used_continuous_decoded_count`
  - `program_render_used_continuous_latest_count`
  - `program_render_used_continuous_exact_count`
  - `program_render_used_continuous_stale_but_accepted_count`
  - `program_render_used_one_shot_fallback_count`
  - `program_continuous_latest_frame_id`
  - `program_continuous_selected_frame_lag`
  - `program_continuous_latest_output_age_ms`
  - `program_decode_fps`
  - `program_selected_source_frame_lag`
  - `program_first_validation_enabled`
  - `preview_compose_skipped_for_program_count`
  - `preview_compose_reused_for_program_count`
  - `program_render_loop_attempt_count`
  - `program_window_render_success_count`
  - `program_window_render_failure_count`
  - `program_render_effective_fps`
  - `effective_program_render_fps`

### OBS Capture Operation

For the separated PreviewOutput / ProgramOutput flow, OBS capture should be
operated as follows:

- OBS Window Capture target: `StreamSync Program Output`
- Do not capture `StreamSync 4-view Output` for production Program output
- `StreamSync 4-view Output` remains human-facing Preview / monitoring output
- Program output is enabled with `--enable-program-output-window`
- Explicit Program source selection uses `--program-selected-client-id <client_id>`
- Selected Program continuous decode is opt-in via
  `--enable-program-continuous-decode`
- Smooth delayed Program playout is opt-in via
  `--program-continuous-decode-mode smooth-latest`
- Program-first validation is opt-in via `--program-first-validation-mode`
- Low-frequency operator Preview decode allowance is opt-in via
  `--program-first-preview-decode-refresh-interval <ticks>`
- Program-selected low-cost Preview slot reuse is opt-in under Program-first
  low-cost Preview mode and explicit Program selection; it reuses the Program
  continuous/latest frame for Preview only

Validated command examples:

```text
--enable-program-output-window
--enable-program-output-window --program-selected-client-id player1
--enable-program-output-window --program-selected-client-id player2
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-first-preview-refresh-interval 10 --program-first-preview-decode-refresh-interval 30
```

Current limitations:

- Program selection is CLI/static for now
- no hotkey or control-pipe switching yet
- no `--program-selected-run-id` yet
- Preview still uses the CPU-side composed BGRA path
- selected Program continuous decode reuses the existing single-source
  continuous decoder; it is not a GPU renderer, not slot-layout rendering, and
  not a multi-source continuous decoder
- `smooth-latest` is Program-only and does not change Preview's target-frame
  exact/bounded behavior
- `--program-first-validation-mode` intentionally reduces Preview freshness /
  quality during validation; it is not a production Preview behavior change
- `--program-first-preview-decode-refresh-interval` only allows non-Program
  Preview one-shot decode on matching low-cost Preview refresh ticks and uses a
  one-source-per-tick budget; it is not a return to every-tick Preview decode
- Program-selected Preview slot reuse does not render 4-view as Program and
  does not mix Preview labels / borders / debug UI into ProgramOutput
- OBS setup remains manual and is not changed by code

### Latest Operator Low-cost Preview Result

- The latest operator low-cost Preview validation used
  `--program-first-preview-refresh-interval 10` and
  `--program-first-preview-decode-refresh-interval 30`.
- ProgramOutput remained structurally good:
  `program_render_effective_fps=19.884`,
  `program_render_used_continuous_latest_count=2846`,
  `program_render_used_one_shot_fallback_count=0`,
  `program_output_black_frame_render_count=0`, and
  `program_output_placeholder_render_count=0`.
- Preview was visible and the non-Program slot improved:
  `operator_preview_decode_refresh_success_count=96`,
  `operator_preview_decode_refresh_source_counts=slot0:96|slot1:0`, and
  `operator_preview_non_program_visible_count=1`.
- Preview was still not useful enough for monitoring because the explicit
  Program source player2 / slot1 stayed black / decode-deferred even though
  ProgramOutput rendered player2 from continuous latest.
- Current code now adds opt-in Program-selected Preview slot reuse. Next
  validation should keep the same `10` / `30` refresh knobs and compare
  `operator_preview_reused_program_frame_count`,
  `operator_preview_program_slot_visible_count`,
  `operator_preview_program_slot_reuse_source`,
  `operator_preview_program_slot_black_count`,
  `operator_preview_decode_refresh_source_counts`, and
  `program_render_used_one_shot_fallback_count`.

### Non-goals for the first Program slice

- Do not make Program output the default OBS target yet.
- Do not remove or change the current 4-view Preview behavior.
- Do not convert Preview to GPU or slot rendering yet.
- Do not call the current 4-view output Program.
- Do not include Preview labels, diagnostics, or slot borders in Program by
  default.

## Next Recommendation
- First raw pipe / stdout throughput code slice is implemented as opt-in
  `scaled-bgr24`, and the first A/B rerun is now reflected.
- Latest completed correspondence rerun validates that both pending backlog and
  completed outputs are delayed by seconds, so the next main line remains
  output pipeline evidence rather than threshold tuning.
- Latest optimized BGR24 A/B shows conversion optimization worked, but
  `scaled-bgr24` still does not clearly beat default BGRA end to end.
- Next candidate order:
  1. human-side Program-first low-cost Preview rerun with
     `--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-first-preview-refresh-interval 10 --program-first-preview-decode-refresh-interval 30`
     to validate Program-selected Preview slot reuse.
  2. human-side `no-scale-bgra` A/B rerun for the scale path split slice
  3. reader/completed latency breakdown diagnostics if no-scale evidence is
     ambiguous
  4. direct BGR24 render path docs-first impact review only
- The `no-scale-bgra` code slice is already implemented; do not broaden it
  before runtime evidence.
- Keep one-shot suppression as strong contributor evidence, but not the current
  main bottleneck.
- Keep threshold branch HOLD / candidate and one-shot suppression as supporting
  evidence, not the next main default policy.
- Production Readiness remains FAIL.
