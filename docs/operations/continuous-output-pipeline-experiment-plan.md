<!-- stream-sync/docs/operations/continuous-output-pipeline-experiment-plan.md -->

# Continuous Output Pipeline Experiment Plan

Last updated: 2026-06-10

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
  - classification: strong ProgramOutput structural evidence, not closeout-ready
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
    - this validation-mode behavior does not remove the production requirement
      for a usable operator Preview surface outside the OBS Program scene
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
  - Latest decode-refresh + Program-slot reuse validation with
    `--program-first-preview-refresh-interval 10` and
    `--program-first-preview-decode-refresh-interval 30` made both Preview
    slots visible sometimes, but it failed as an operator direction:
    - Program stayed selected-only and used continuous latest, but smoothness
      worsened: `program_render_effective_fps=15.581`,
      `program_render_used_continuous_latest_count=2799`, and
      `program_render_used_one_shot_fallback_count=0`.
    - Preview one-shot cost was high:
      `one_shot_decode_attempt_count=94`,
      `one_shot_decode_elapsed_ms=16349`, and
      `avg_decode_elapsed_ms=174.096`.
    - Program-selected reuse was active:
      `operator_preview_reused_program_frame_count=157`,
      `operator_preview_program_slot_visible_count=1`, and
      `operator_preview_program_slot_black_count=201`.
    - Non-Program Preview was also visible sometimes:
      `operator_preview_non_program_visible_count=1`.
    - Manual result: client1 and client2 both flickered, and 4-view Preview was
      not useful for monitoring.
    - Decision: do not keep pursuing low-FPS video Preview as the next
      operator candidate. Use stable snapshot-style Preview instead.
    - Current implementation adds opt-in
      `--operator-preview-snapshot-retention`, which retains last-visible
      per-slot snapshots and reuses them when the next tick would otherwise
      show placeholder/deferred output. It does not trigger extra one-shot
      decode and keeps ProgramOutput rendering separate.
    - Latest `5` / `90` snapshot validation used
      `--program-first-preview-refresh-interval 5`,
      `--program-first-preview-decode-refresh-interval 90`, and
      `--operator-preview-snapshot-retention`.
    - Snapshot retention worked for stable visibility:
      client1/client2 were not black, flicker was gone,
      `operator_preview_snapshot_retention_enabled=true`,
      `operator_preview_snapshot_reuse_count=2743`,
      `operator_preview_placeholder_avoided_by_snapshot_count=2743`, and
      `operator_preview_slot_black_after_snapshot_count=0`.
    - ProgramOutput remained structurally promising / partial PASS, not closeout-ready:
      OBS target separation was correct, Program did not mix Preview UI,
      black/placeholder counters were `0`, perceived stutter was small,
      `program_render_used_continuous_latest_count=2736`, and
      `program_render_used_one_shot_fallback_count=0`.
    - Same-loop Preview tuning is now limited:
      `operator_preview_render_effective_fps=3.233` was still too slow for
      operator monitoring, and Program FPS dropped to
      `program_render_effective_fps=16.201`.
    - Comparison: `10` / `90` gave better Program FPS around `18.437` but
      Preview was still too slow; `5` / `90` improved repaint to `3.233fps`
      but remained too slow and cost Program FPS.
    - Decision: pause same-loop low-cost Preview refresh tuning. Do not keep
      lowering Preview refresh interval in this path. Do not close
      ProgramOutput near-MVP yet; first audit non-FPS blockers and define
      closeout criteria. Treat current Preview as stable snapshot-only, and
      move future operator Preview work to a separate cadence/runtime or
      lighter renderer design.
    - ProgramOutput non-FPS blockers from latest validation:
      first render was delayed (`program_output_first_render_elapsed_ms=16045`),
      selected source was missing before first render
      (`program_output_missing_selected_source_count=264`,
      `program_output_missing_before_first_render_count=264`,
      `program_output_missing_after_first_render_count=0`,
      `program_output_missing_selected_source_reason=NoDecodedFrameForSelection`),
      smooth-latest lag is not acceptance-defined
      (`program_selected_source_frame_lag=299`,
      `program_continuous_selected_frame_lag=285`,
      `continuous_decode_latest_selected_to_output_frame_gap=299`),
      current source identity is CLI-fixed, runtime switching is missing,
      Preview is not final monitoring, OBS can still be manually
      misconfigured, and player1/player2 visual differentiation is weak.
    - ProgramOutput startup diagnostic slice now adds stdout summary fields for
      selection/source/input/output/renderable timing and startup
      classification:
      `program_selection_resolved_elapsed_ms`,
      `program_continuous_source_resolved_elapsed_ms`,
      `program_first_source_frame_seen_elapsed_ms`,
      `program_first_continuous_input_elapsed_ms`,
      `program_first_continuous_output_elapsed_ms`,
      `program_first_renderable_decoded_frame_elapsed_ms`,
      `program_first_render_waiting_for_decode_count`,
      `program_first_render_missing_reason_counts`,
      `program_startup_one_shot_fallback_allowed`,
      `program_startup_one_shot_fallback_attempt_count`,
      `program_startup_one_shot_fallback_suppressed_count`,
      `program_startup_continuous_pending_count`,
      `program_startup_no_selected_source_count`,
      `program_startup_no_decoded_frame_count`,
      `program_startup_latest_continuous_available_count`,
      `program_startup_latest_continuous_rejected_count`, and
      `program_startup_source_identity_mismatch_count`.
    - Next evidence gate: rerun the same Program-first smooth-latest validation
      shape with these fields, then decide whether the smallest fix should be
      startup-only one-shot fallback, continuous prewarm, blocking wait for
      first continuous frame, retained-keyframe bootstrap, first-frame Program
      policy, or a source identity/run_id fix. Do not implement those fixes
      before the rerun evidence.
    - Latest ProgramOutput startup rerun narrows the first-render delay:
      selection/source identity resolve at `0ms`, selected source frame and
      continuous input first appear at `4702ms`, continuous output /
      renderable frame / first Program render all happen at `6826ms`, and
      missing source is only before first render. Because the validation
      script starts switcher first, then waits before starting client1/client2,
      about 2.5s of the elapsed first render may be process start order rather
      than runtime decode/render delay.
    - Startup one-shot fallback interpretation:
      `program_startup_one_shot_fallback_allowed=true` means smooth-latest
      ProgramOutput may use an already decoded selected frame if one exists;
      ProgramOutput does not currently launch one-shot decode itself. If the
      validation/pre-composition path has not produced a selected decoded
      frame, `program_startup_one_shot_fallback_attempt_count` remains `0`.
    - Additional candidate diagnostics now separate no-candidate from
      rejected-candidate cases:
      `program_startup_one_shot_fallback_blocked_reason_counts`,
      `program_startup_selected_frame_keyframe_available_count`,
      `program_startup_selected_frame_source_counts`,
      `program_startup_retained_keyframe_available_count`,
      `program_startup_one_shot_candidate_count`,
      `program_startup_one_shot_candidate_rejected_count`, and
      `program_startup_one_shot_candidate_rejected_reason_counts`.
    - Clients-before-switcher bootstrap bypass validation is now PASS:
      `program_startup_bootstrap_attempt_count=1`,
      `program_startup_bootstrap_success_count=1`,
      `program_startup_bootstrap_actual_decode_invoked_count=1`,
      `program_startup_bootstrap_decode_skipped_before_invoke_count=0`,
      `program_startup_bootstrap_used_for_first_render=true`,
      `program_output_first_render_elapsed_ms=354`, and
      `program_output_missing_before_first_render_count=0`.
    - Switcher-first cold-start bootstrap validation is also PASS once the
      selected source frame exists:
      `program_output_first_render_elapsed_ms=3803`,
      `program_output_missing_before_first_render_count=102`,
      `program_output_missing_after_first_render_count=0`,
      `program_first_source_frame_seen_elapsed_ms=3590`,
      `program_first_continuous_input_elapsed_ms=3803`,
      `program_first_renderable_decoded_frame_elapsed_ms=3803`,
      `program_first_continuous_output_elapsed_ms=5330`,
      `program_startup_bootstrap_attempt_count=1`,
      `program_startup_bootstrap_success_count=1`,
      `program_startup_bootstrap_actual_decode_invoked_count=1`, and
      `program_startup_bootstrap_used_for_first_render=true`.
    - Startup limitation is now explicit: selected-only ProgramOutput cannot
      render before the selected client frame exists. Bootstrap reduces
      decode / continuous startup latency after selected source arrival, but it
      does not remove the switcher-first wait for the selected source.
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
  - `program_startup_bootstrap_enabled`
  - `program_startup_bootstrap_attempt_count`
  - `program_startup_bootstrap_success_count`
  - `program_startup_bootstrap_elapsed_ms`
  - `program_startup_bootstrap_decode_attempt_elapsed_ms`
  - `program_startup_bootstrap_decode_error_counts`
  - `program_startup_bootstrap_ffmpeg_exit_status`
  - `program_startup_bootstrap_ffmpeg_stderr_summary`
  - `program_startup_bootstrap_payload_bytes_min/max/avg`
  - `program_startup_bootstrap_payload_nal_kinds`
  - `program_startup_bootstrap_payload_has_sps_count`
  - `program_startup_bootstrap_payload_has_pps_count`
  - `program_startup_bootstrap_payload_has_idr_count`
  - `program_startup_bootstrap_frame_id_min/max`
  - `program_startup_bootstrap_slot_counts`
  - `program_startup_bootstrap_client_counts`
  - `program_startup_bootstrap_actual_decode_invoked_count`
  - `program_startup_bootstrap_decode_skipped_before_invoke_count`
  - `program_startup_bootstrap_source_counts`
  - `program_startup_bootstrap_rejected_reason_counts`
  - `program_startup_bootstrap_used_for_first_render`

### OBS Capture Operation

For the separated PreviewOutput / ProgramOutput flow, OBS capture should be
operated as follows:

- OBS Window Capture target: `StreamSync Program Output`
- Do not capture `StreamSync 4-view Output` for production Program output
- `StreamSync 4-view Output` remains the human-facing `4`-view Preview and is
  still required for operator monitoring
- `StreamSync 4-view Output` may remain visible to the operator, but it must
  not be active in the OBS Program scene
- Program output is enabled with `--enable-program-output-window`
- Explicit Program source selection uses `--program-selected-client-id <client_id>`
- Selected Program continuous decode is opt-in via
  `--enable-program-continuous-decode`
- Smooth delayed Program playout is opt-in via
  `--program-continuous-decode-mode smooth-latest`
- Program-first validation is opt-in via `--program-first-validation-mode`
- Program startup one-shot bootstrap is opt-in via
  `--program-startup-bootstrap-one-shot`; it is ProgramOutput-only, startup-only,
  and keeps default behavior unchanged
- Low-frequency operator Preview decode allowance is opt-in via
  `--program-first-preview-decode-refresh-interval <ticks>`
- Program-selected low-cost Preview slot reuse is opt-in under Program-first
  low-cost Preview mode and explicit Program selection; it reuses the Program
  continuous/latest frame for Preview only
- Snapshot-style low-cost Preview retention is opt-in via
  `--operator-preview-snapshot-retention`
- Same-loop low-cost Preview refresh tuning is paused after the `5` / `90`
  validation. Do not keep lowering the refresh interval in this path.
- ProgramOutput is structurally promising but not closeout-ready until
  non-FPS operational blockers and closeout criteria are resolved.
- Latest clients-before-switcher startup baseline reduced first selected
  source/input visibility to `246ms` and first Program render to `1964ms`, with
  after-first missing / black / placeholder all `0`. The first bootstrap A/B
  did not improve startup and exposed `decode_failed:27`, but follow-up
  diagnostics showed `program_startup_bootstrap_actual_decode_invoked_count=0`,
  `program_startup_bootstrap_decode_skipped_before_invoke_count=24`, and
  `deferred_continuous_one_shot_suppressed:24`, so that result was a
  pre-invoke `ContinuousOneShotSuppressed` routing bug rather than proven
  FFmpeg decode failure.
- The bootstrap decode purpose / suppression bypass fix is now validated in the
  clients-before-switcher shape:
  `program_startup_bootstrap_attempt_count=1`,
  `program_startup_bootstrap_success_count=1`,
  `program_startup_bootstrap_actual_decode_invoked_count=1`,
  `program_startup_bootstrap_decode_skipped_before_invoke_count=0`,
  `program_startup_bootstrap_decode_error_counts=failed:0|deferred_empty_payload:0|deferred_invalid_dimensions:0|deferred_ffmpeg_unavailable:0|deferred_continuous_one_shot_suppressed:0|unknown:0`,
  `program_startup_bootstrap_used_for_first_render=true`,
  `program_output_first_render_elapsed_ms=354`, and
  `program_output_missing_before_first_render_count=0`.
- The switcher-first cold-start validation is now also recorded:
  `program_output_first_render_elapsed_ms=3803`,
  `program_output_missing_before_first_render_count=102`,
  `program_output_missing_after_first_render_count=0`,
  `program_first_source_frame_seen_elapsed_ms=3590`,
  `program_first_continuous_input_elapsed_ms=3803`,
  `program_first_renderable_decoded_frame_elapsed_ms=3803`,
  `program_first_continuous_output_elapsed_ms=5330`,
  `program_startup_bootstrap_attempt_count=1`,
  `program_startup_bootstrap_success_count=1`,
  `program_startup_bootstrap_actual_decode_invoked_count=1`,
  `program_startup_bootstrap_used_for_first_render=true`,
  `program_output_black_frame_render_count=0`, and
  `program_output_placeholder_render_count=0`.
- Bootstrap bypass is fixed and validated in both start-order shapes, but it is
  still not a full startup-latency fix. ProgramOutput is selected-only and
  cannot render before the selected client frame exists. In switcher-first cold
  start, bootstrap only helps after selected source arrival.
- ProgramOutput startup readiness semantics are now:
  `program_selection_configured`,
  `program_selected_source_waiting`,
  `program_selected_source_seen`,
  `program_first_frame_bootstrapping`,
  `program_first_frame_rendered`, and
  `program_steady_state`.
- Narrow readiness diagnostics are now implemented:
  `program_startup_readiness_state`,
  `program_selected_source_wait_elapsed_ms`,
  `program_startup_waiting_for_selected_source_count`,
  `program_startup_bootstrap_after_source_seen_elapsed_ms`, and
  `program_startup_selected_source_seen_count`. These are summary-only and do
  not change bootstrap, smooth-latest, Program/Preview separation, or OBS
  setup.
- ProgramOutput closeout remains blocked and same-loop Preview tuning remains
  paused.

Validated command examples:

```text
--enable-program-output-window
--enable-program-output-window --program-selected-client-id player1
--enable-program-output-window --program-selected-client-id player2
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-first-preview-refresh-interval 10 --program-first-preview-decode-refresh-interval 30
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-first-preview-refresh-interval 30 --program-first-preview-decode-refresh-interval 90 --operator-preview-snapshot-retention
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-first-preview-refresh-interval 5 --program-first-preview-decode-refresh-interval 90 --operator-preview-snapshot-retention
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-startup-bootstrap-one-shot
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
- `--program-startup-bootstrap-one-shot` does not make 4-view Preview a Program
  source, does not change OBS setup, and does not add hotkey/control-pipe
  switching. It only lets ProgramOutput attempt a selected-source one-shot
  bootstrap before first Program render when continuous latest / last-valid /
  selected decoded frames are not yet available.
- Program-selected Preview slot reuse does not render 4-view as Program and
  does not mix Preview labels / borders / debug UI into ProgramOutput
- Snapshot retention keeps last-visible Preview slot images when the next tick
  would otherwise show placeholder/deferred output; it does not add extra
  decode pressure
- OBS setup remains manual and is not changed by code

### Latest Operator Low-cost Preview Result

- The latest operator low-cost Preview validation used
  `--program-first-preview-refresh-interval 5`,
  `--program-first-preview-decode-refresh-interval 90`, and
  `--operator-preview-snapshot-retention`.
- ProgramOutput remains structurally promising / partial PASS, not closeout-ready:
  `program_render_effective_fps=16.201`,
  `program_render_used_continuous_latest_count=2736`,
  `program_render_used_one_shot_fallback_count=0`,
  `program_output_black_frame_render_count=0`, and
  `program_output_placeholder_render_count=0`.
- OBS target separation was correct and Program did not mix Preview UI.
- Snapshot retention fixed black/flicker:
  client1/client2 were not black, flicker was gone,
  `operator_preview_snapshot_reuse_count=2743`,
  `operator_preview_placeholder_avoided_by_snapshot_count=2743`, and
  `operator_preview_slot_black_after_snapshot_count=0`.
- Preview remained too slow for operator monitoring:
  `operator_preview_render_effective_fps=3.233`,
  `operator_preview_decode_refresh_success_count=31`, and
  `one_shot_decode_attempt_count=31`.
- Same-loop low-cost Preview refresh tuning is now paused. ProgramOutput is not
  closeout-ready yet because first-render delay, startup missing selected
  source, smooth-latest lag acceptance, static CLI source identity, missing
  runtime switching, manual OBS safety, weak visual source differentiation, and
  final Preview monitoring remain unresolved.

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
- Selected-source visual verification has an earlier recorded PASS reference
  using the validation-only client/source-side marker:
  `--validation-source-marker <label>` on the client real encoded bounded PoC.
  In the latest manual validation, player1 used `P1`, player2 used `P2`,
  Program selected `player2`, OBS captured only `StreamSync Program Output`,
  Program stayed clean/selected-only, and the visible Program marker matched
  `P2`.
- Latest selected-source PASS reference metrics:
  - `program_selected_source_frame_lag=5`
  - `program_continuous_selected_frame_lag=0`
  - `continuous_decode_latest_selected_to_output_frame_gap=5`
  - `program_render_effective_fps=22.285`
  - black / placeholder `0`
- Applying the current draft lag criteria to that result:
  - classification: `Good`
  - rationale:
    visual source verification passed with `P2`, OBS captured only
    `StreamSync Program Output`, lag/gap were `5 / 0 / 5`, Program FPS was
    `22.285`, black / placeholder were `0`, perceived stutter was small, and
    the single one-shot fallback was startup bootstrap only
    (`program_render_used_one_shot_fallback_count=1`,
    `program_startup_bootstrap_used_for_first_render=true`) rather than
    steady-state fallback
- Latest smooth-latest lag diagnostics rerun:
  - log dir:
    `S:\stream-sync\manual-logs\program-output-smooth-latest-lag-rerun-20260607-002942`
  - overall ProgramOutput criteria-based validation:
    `WARNING`, not `PASS` and not `FAIL` after marker ambiguity correction
  - corrected classifications:
    OBS safety `PASS`, Program cleanliness `PASS`, selected-source visual
    verification `PASS`, lag criteria `Warning`
  - marker ambiguity correction:
    the visible `P2` marker was the source-side validation marker. It must not
    be treated as Program overlay, debug UI, Preview label, or 4-view UI.
  - lag metrics:
    `program_selected_source_frame_lag=16`,
    `program_continuous_selected_frame_lag=16`,
    `continuous_decode_latest_selected_to_output_frame_gap=16`,
    `program_render_effective_fps=23.779`
  - smooth-latest frame relation:
    selected frame `3089`, rendered frame `3073`, latest continuous frame
    `3073`, selected-minus-rendered lag `16`,
    selected-minus-latest-continuous lag `16`,
    rendered-minus-latest-continuous gap `0`, source mismatch count `0`,
    stale reuse count `41`, cache age / frame age `1ms`
  - interpretation:
    Program render selection is unlikely to be the main issue because rendered
    frame equals latest available continuous frame. Source mismatch is unlikely
    because mismatch count is `0`. Stale / last-valid reuse is not primary, but
    `program_smooth_latest_stale_reuse_count=41` remains a watch item. The
    latest available continuous frame is still 16 frames behind the selected
    source, so continuous decoder / feed backlog is the likely cause.
  - backlog evidence:
    `continuous_decode_output_throughput_fps=20.906`,
    `continuous_decode_latest_input_minus_latest_output_lag=41`,
    `continuous_decode_latest_input_to_output_frame_gap=41`,
    `continuous_decode_output_lag_to_selected_frames=16`,
    `continuous_decode_pending_correspondence_count=41`,
    `continuous_decode_pending_correspondence_age_ms_avg=1004.488`,
    `continuous_decode_completed_correspondence_latency_ms_avg=1486.485`,
    `continuous_decode_completed_correspondence_latency_ms_max=2228`,
    `continuous_decode_reader_full_frame_elapsed_ms_avg=47.295`,
    `continuous_decode_reader_full_frame_slow_count=422`,
    `continuous_decode_stdout_reader_blocked_count=2194`,
    `continuous_decode_no_output_after_input_count=2213`,
    `continuous_decode_output_frame_interval_ms_avg=46.466`,
    `continuous_decode_output_frame_interval_ms_max=719`
  - next backlog diagnostics:
    `continuous_decode_input_throughput_fps`,
    `continuous_decode_output_to_input_fps_ratio`,
    `continuous_decode_backlog_frame_gap`,
    `continuous_decode_backlog_age_ms`, and
    `continuous_decode_backlog_classification` are now added as summary-only
    fields for the next rerun.
  - result handling:
    the previous `56 / 56 / 56` `FAIL` is superseded by this latest `16 / 16 /
    16` `WARNING`. ProgramOutput remains clean / selected-only, but closeout is
    still blocked by lag/backlog and operator Preview requirements.
- Latest unbounded handoff backlog rerun:
  - log dir:
    `S:\stream-sync\manual-logs\program-output-backlog-rerun-unbounded-handoff-20260608-014106`
  - overall ProgramOutput criteria-based validation:
    `FAIL`
  - classifications:
    OBS safety `PASS`, Program cleanliness / availability `PASS`,
    selected-source visual verification `WARNING`, lag criteria `Fail`
  - ProgramOutput stayed clean:
    black / placeholder / after-first missing were all `0`
  - lag metrics:
    `program_render_effective_fps=10.865`,
    `program_selected_source_frame_lag=37`,
    `program_continuous_selected_frame_lag=20`,
    `continuous_decode_latest_input_to_output_frame_gap=37`,
    `continuous_decode_backlog_classification=pending_correspondence_backlog`
  - smooth-latest frame relation:
    selected frame `876`, rendered frame `856`, latest continuous frame `856`,
    selected-minus-rendered `20`, selected-minus-latest-continuous `20`,
    rendered-minus-latest-continuous `0`, cache age `1ms`
  - interpretation:
    Program selection is probably reading latest continuous output correctly.
    The `37` lag is likely using the continuous requested/input frame basis,
    while the actual smooth-latest selected/rendered relation is `20`. The
    pending correspondence range `857..893` with latest output `856` still
    points to real continuous decode output backlog.
  - next basis diagnostics:
    `program_selected_source_frame_lag_basis`,
    `program_selected_source_frame_lag_basis_frame_id`, and
    `program_selected_source_frame_lag_matches_smooth_latest` are added for the
    next rerun.
- Latest ProgramOutput lag basis rerun:
  - log dir:
    `S:\stream-sync\manual-logs\program-output-lag-basis-rerun-20260610-133454`
  - rerun validity:
    `valid`; server/client/switcher stderr were empty
  - ProgramOutput stayed clean and available after first render:
    black / placeholder / missing-after-first-render were all `0`
  - basis diagnostics:
    `program_selected_source_frame_lag=27`,
    `program_selected_source_frame_lag_basis=continuous_decode_requested_minus_latest_decoded`,
    `program_selected_source_frame_lag_basis_frame_id=844`,
    `program_selected_source_frame_lag_matches_smooth_latest=false`
  - smooth-latest frame relation:
    selected frame `844`, rendered frame `843`, latest continuous frame `843`,
    selected-minus-rendered `1`, selected-minus-latest-continuous `1`,
    rendered-minus-latest-continuous `0`, cache age `0ms`, continuous latest
    output age `0ms`
  - continuous decode backlog:
    `continuous_decode_backlog_classification=pending_correspondence_backlog`,
    `continuous_decode_backlog_frame_gap=27`,
    `continuous_decode_backlog_age_ms=1348`,
    `continuous_decode_pending_correspondence_count=27`,
    pending frame id range `844..870`, input/output fps `18.668 / 18.067`,
    output/input ratio `0.968`
  - interpretation:
    the large `program_selected_source_frame_lag` is a requested/input versus
    latest decoded basis metric, not the actual smooth-latest Program render
    lag. The primary smooth-latest render lag is `1 + 0`, so selection
    correctness and render-source choice are `PASS`. Continuous decode backlog
    remains a separate pipeline health warning.
  - classification:
    Program cleanliness `PASS`, Program availability after first render
    `PASS`, smooth-latest selection correctness `PASS`, smooth-latest render
    lag `PASS`, continuous decode backlog `WARNING`, Program render FPS `FAIL`
    because `program_render_effective_fps=12.253`; overall closeout remains
    blocked until render FPS improves and selected-source visual verification
    is human-confirmed for this rerun.
- Latest Program render FPS basis investigation:
  - `program_render_effective_fps` is total-run Program success FPS:
    `program_window_render_success_count / loop_total_elapsed_ms`.
  - The latest `241` Program window render failures match
    `program_output_missing_before_first_render_count=241`; after-first-render
    missing stayed `0`, so this is startup wait, not steady-state Program
    output unavailability.
  - Program after-first-render success is count-level `PASS`: `659` successes
    after the startup missing period.
  - Total attempt cadence is already low: `900 / 53781ms`, about `16.734fps`.
  - Shared-loop timing explains the cadence drop better than Program-only
    render cost:
    `attempt_body_elapsed_ms=21842` plus fixed cadence sleep
    `loop_sleep_elapsed_ms=29835`; the loop sleeps after body work rather than
    subtracting body time from the frame interval.
  - Preview/clean-output timings are not Program-only timings:
    `quad_view_compose_elapsed_ms=3045`,
    `render_call_elapsed_ms=3218`,
    `render_buffer_cpu_scale_copy_elapsed_ms=1824`, and
    `gdi_paint_wait_elapsed_ms=1070` are collected through the Preview
    `ObsFriendlyFourViewLoopWindowRenderRuntime`.
  - One-shot decode is the largest clearly measured shared-loop body cost:
    `one_shot_decode_elapsed_ms=6235`,
    `one_shot_decode_output_read_elapsed_ms=3131`, and
    `continuous_decode_competing_one_shot_decode_elapsed_ms=6131`.
  - Result:
    treat total-run Program FPS as overall loop health `FAIL`, but do not read
    it as Program smooth-latest render lag or Program cleanliness failure.
    Closeout should split startup, after-first-render Program success cadence,
    and shared-loop workload.
  - Implemented summary-only FPS split diagnostics:
    `program_rendered_after_first_render`,
    `program_render_effective_fps_after_first_render`,
    `program_window_render_failure_before_first_render`,
    `program_window_render_failure_after_first_render`,
    `program_window_render_elapsed_ms`,
    `program_window_render_elapsed_ms_avg`, and
    `program_window_render_elapsed_ms_max`.
  - These diagnostics do not change ProgramOutput rendering behavior. They only
    make the next rerun able to separate startup waiting, after-first-render
    Program cadence, and Program tick/render elapsed from total-loop health.
- Latest after-first-render FPS rerun:
  `S:\stream-sync\manual-logs\program-output-after-first-render-fps-rerun-20260611-002339`.
  - Program cleanliness / availability / smooth-latest render lag remain
    `PASS`: black / placeholder / after-first missing are `0 / 0 / 0`,
    after-first Program window failures are `0`, and smooth-latest
    selected-rendered / rendered-latest gaps are `0 / 0`.
  - Program window render itself is cheap:
    `program_window_render_elapsed_ms=303`,
    `program_window_render_elapsed_ms_avg=0.337`,
    `program_window_render_elapsed_ms_max=16`.
  - FPS is still below target because the shared loop is below target:
    total-run Program FPS `13.207`, after-first-render Program FPS `15.799`,
    loop attempt FPS `17.558`.
  - The main reduction candidate is one-shot decode competing with continuous
    smooth-latest decode:
    `one_shot_decode_elapsed_ms=5599`,
    `continuous_decode_competing_one_shot_decode_elapsed_ms=5528`,
    and `one_shot_decode_attempt_count=60`.
  - Preview compose/materialization is secondary:
    `quad_view_compose_elapsed_ms=2487`,
    `render_buffer_materialization_elapsed_ms=1362`.
- Minimal code slice for the next validation:
  extend existing opt-in `--program-first-validation-mode` behavior only when
  ProgramOutput is enabled, continuous decode is smooth-latest, and continuous
  latest is already available for the selected Program source. In that case,
  suppress Program-source Preview one-shot decode and rely on the existing
  operator Preview Program-frame reuse path. Default behavior and ProgramOutput
  rendering stay unchanged.
- New diagnostics for that slice:
  `program_first_suppressed_program_preview_one_shot_decode_count`,
  `program_first_suppressed_program_preview_one_shot_decode_slot_counts`, and
  `program_first_suppressed_program_preview_one_shot_decode_reason_counts`
  (`continuous_latest_available`).
- Latest Program-first suppression rerun:
  `S:\stream-sync\manual-logs\program-output-program-first-suppression-rerun-20260611-005543`.
  - The switcher included `--program-first-validation-mode`; classify the run
    as ProgramOutput validation/performance mode.
  - Program cleanliness, after-first availability, and smooth-latest render lag
    are `PASS`.
  - Aggregate one-shot suppression is `PASS`:
    `one_shot_decode_attempt_count=0`,
    `one_shot_decode_elapsed_ms=0`,
    `continuous_decode_competing_one_shot_decode_elapsed_ms=0`, and
    `continuous_decode_competing_one_shot_attempt_count=0`.
  - After-first Program FPS improved from `15.799` to `21.848`. Loop workload
    improved from `effective_attempt_fps=17.558` to `23.432`,
    `attempt_body_elapsed_ms=19244` to `6872`, and `slow_attempt_count=65` to
    `4`.
  - Continuous decode remains around `20fps`, with output throughput `20.249`
    and output/input ratio `0.980`; client effective output fps is around
    `21fps`.
  - Operator 4-view Preview was intentionally not usable in this mode:
    `frames_rendered=0`,
    `clean_output_render_result_kind=NoRenderableQuadView`,
    `program_first_preview_visible=false`,
    `program_first_preview_suppressed_count=899`,
    `preview_compose_skipped_for_program_count=899`, and
    `quad_view_compose_elapsed_ms=0`.
  - Startup regressed and remains `WARNING`:
    `program_output_first_render_elapsed_ms=11038`,
    `program_output_missing_before_first_render_count=301`,
    `program_startup_one_shot_fallback_attempt_count=0`, and
    `program_startup_one_shot_fallback_suppressed_count=54`.
  - `program_first_suppressed_program_preview_one_shot_decode_count=0` did not
    fire for player2 / slot1, but aggregate one-shot suppression succeeded via
    the existing Program-first Preview suppression path. Treat this as a
    diagnostic attribution caveat, not as a workload failure.
- Previous completed-template criteria-based ProgramOutput validation rerun:
  - log dir:
    `D:\stream-sync\manual-logs\program-output-criteria-validation-20260606-001029`
  - overall ProgramOutput criteria-based validation:
    `FAIL`, not `PASS`
  - OBS safety:
    `PASS`
  - Program cleanliness:
    `PASS`
  - lag criteria:
    `Fail`
  - selected-source visual verification:
    `WARNING`
  - retained good facts:
    selected source diagnostics still resolved to `player2`, client marker
    diagnostics still showed `P1` / `P2`, Program black / placeholder /
    after-first missing stayed `0`, and startup bootstrap remained
    startup-only
  - fail reasons:
    lag/gap worsened again from the earlier `12 / 12 / 12` warning run to
    `56 / 56 / 56`, while Program FPS only recovered to `21.362`, which is
    still insufficient to offset the lag failure
  - selected-source limitation:
    repo-backed manual visual evidence still does not explicitly record that
    `P2` was human-visible in Program and `P1` was absent from Program, so the
    rerun does not upgrade selected-source visual verification to `PASS`
  - result handling:
    OBS safety and Program cleanliness remain `PASS`, but the rerun is a
    ProgramOutput `FAIL` because lag crossed the draft failure threshold and
    selected-source visual verification is still not `PASS`
  - marker implementation status:
    the improved validation-only source marker phase is now represented by a
    recorded rerun; marker behavior remains opt-in and source-side, and
    ProgramOutput still gets no overlay, watermark, Preview label, or 4-view
    Program fallback.
- Next candidate order:
  1. Split closeout scope explicitly: ProgramOutput-only validation/performance
     mode is close to `PASS` pending selected-source visual confirmation, while
     normal Program + operator 4-view Preview coexistence remains open.
  2. Confirm selected-source visual identity for the latest Program-first
     suppression rerun with human evidence.
  3. Design or validate a normal operator monitoring path: separate Preview
     cadence/runtime, a lower-cost Preview refresh strategy, or a dedicated
     monitoring mode that does not reintroduce one-shot contention.
  4. Investigate continuous decoder / feed backlog. Start with throughput below
     input, FFmpeg scale path, stdout read cadence, output interval, pending
     correspondence age, completed latency, reader blocked count, no-output
     counts, decoded cache dropping, input feed vs output throughput, and
     whether selected-source feed priority is needed.
  5. Keep possible fixes narrow if evidence points clearly: no-scale or
     lower-cost FFmpeg path, low-latency / probe args, aggressive pending decode
     input dropping, decode only latest selected Program source, or workload
     reduction. Do not change OBS setup or add Program overlays.
  6. Continue ProgramOutput non-FPS blocker audit and closeout criteria
     definition: OBS capture safety run evidence, Program-first validation vs
     final operator mode, diagnostics completeness, and long-run stability.
  7. Keep the production operator Preview requirement active while same-loop
     Preview cadence/runtime tuning stays paused; current Preview is stable
     snapshot-only and future work may move to a separate cadence/runtime or a
     lighter renderer.
  8. Program source switching over hotkey/control pipe later, after
     ProgramOutput criteria are defined.
  9. human-side `no-scale-bgra` A/B rerun for the scale path split slice
  10. reader/completed latency breakdown diagnostics if no-scale evidence is
     ambiguous
  11. direct BGR24 render path docs-first impact review only
- The `no-scale-bgra` code slice is already implemented; do not broaden it
  before runtime evidence.
- Keep one-shot suppression as strong contributor evidence, but not the current
  main bottleneck.
- Keep threshold branch HOLD / candidate and one-shot suppression as supporting
  evidence, not the next main default policy.
- Production Readiness remains FAIL.
