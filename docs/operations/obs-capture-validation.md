<!-- stream-sync/docs/operations/obs-capture-validation.md -->

# OBS Capture Validation Follow-up

## Status
- This document is the immediate next-step source of truth after the latest
  same-PC `4`-client all-real concurrent PASS from:
  - `manual-logs/four-client-20260513-184503`
- The runtime/render-side PASS checkpoint is already closed:
  - `preview_mode=preview-latest-decodable`
  - `read_mode=inspect-latest-decodable`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `window_title=StreamSync 4-view Output`
- This slice is docs-first and manual-validation-only:
  - no code change
  - no protocol/architecture change
  - no distributed-PC expansion
- OBS WebSocket / advanced OBS control remain out of scope.
- Latest pasted-back OBS capture validation is recorded separately below:
  - OBS capture itself: PASS
  - StreamSync runtime: PARTIAL
- The next-phase source of truth after this result is:
  - `docs/operations/distributed-pc-validation.md`

## Latest Pasted-Back Result

- log dir:
  - `manual-logs/obs-capture-20260513-190909`
- OBS:
  - `StreamSync 4-view Output` was selectable in OBS
  - OBS preview displayed the `4`-client `4`-view output
  - OBS-side issue: `none`
  - StreamSync-side issue observed in the same run: switcher FPS was very low
- Server:
  - ready line emitted
  - stopped summary emitted
  - `max_handoff_requests=20000`
  - `receive_timeout_ms=300000`
  - `max_runtime_duration_ms=600000`
  - `stop_reason=ReceiveStopped`
  - `receive_stop_reason=ReceiveTimedOut`
  - `handoff_stop_reason=StopRequested`
  - `runtime_duration_ms=356528`
  - `packets_received=79733`
  - `frames_queued=3305`
  - `per_client_queued_frames=player1/streamsync-dev-session:900|player2/streamsync-dev-session:782|player3/streamsync-dev-session:723|player4/streamsync-dev-session:900`
  - `keyframes_queued=112`
  - `retained_keyframe_clients=4`
  - `frame_read_count=1637`
  - `no_frame_count=319`
  - `decodable_source_counts=queue:793|retained_keyframe:844|none:319`
  - `io_error_count=0`
- Switcher:
  - pasted-back summary lines block was empty
  - warning observed: `[WARN] switcher did not exit within 360 seconds.`
  - OBS preview nevertheless showed the `4`-client `4`-view output
  - final switcher summary was not collected because the switcher did not exit within the wait window
- Clients:
  - `client1`: `accepted=true`, `frames_encoded=900`, `frames_sent=900`, `send_failures=0`, `encode_failures=0`, `keyframes_sent=30`, `h264_parameter_sets_cached=true`, `stop_reason=Some(MaxFramesReached)`, `effective_output_fps=18.279`
  - `client2`: `accepted=true`, `frames_encoded=782`, `frames_sent=782`, `send_failures=0`, `encode_failures=1`, `keyframes_sent=27`, `h264_parameter_sets_cached=true`, `stop_reason=Some(EncodeFailure)`, `effective_output_fps=17.050`
  - `client3`: `accepted=true`, `frames_encoded=723`, `frames_sent=723`, `send_failures=0`, `encode_failures=1`, `keyframes_sent=25`, `h264_parameter_sets_cached=true`, `stop_reason=Some(EncodeFailure)`, `effective_output_fps=16.240`
  - `client4`: `accepted=true`, `frames_encoded=900`, `frames_sent=900`, `send_failures=0`, `encode_failures=0`, `keyframes_sent=30`, `h264_parameter_sets_cached=true`, `stop_reason=Some(MaxFramesReached)`, `effective_output_fps=18.400`

## Current Program Capture Rule

The validation evidence above is the historical `StreamSync 4-view Output`
OBS capture PASS. After Preview / Program separation, the production OBS rule
is now:

- OBS Window Capture target: `StreamSync Program Output`
- `StreamSync 4-view Output` remains the human-facing `4`-view Preview and is
  still required for production operator monitoring
- do not use `StreamSync 4-view Output` for production Program capture
- `StreamSync 4-view Output` may stay visible to the operator, but it must not
  be active in the OBS Program scene
- enable Program output with `--enable-program-output-window`
- select an explicit Program source with `--program-selected-client-id <client_id>`

Validated command examples for the Program path:

```text
--enable-program-output-window
--enable-program-output-window --program-selected-client-id player1
--enable-program-output-window --program-selected-client-id player2
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode
--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-first-preview-refresh-interval 10 --program-first-preview-decode-refresh-interval 30
```

## Latest Program Capture Stability Result

- Manual long-run OBS validation used the separated Program target:
  - OBS captured `StreamSync Program Output`
  - OBS did not capture `StreamSync 4-view Output`
  - ProgramOutput was enabled with
    `--enable-program-output-window --program-selected-client-id player2`
- Target selection / window separation result:
  - `program_output_enabled=true`
  - `program_output_selection_mode=explicit`
  - `program_output_requested_client_id=player2`
  - `program_output_selected_client_id=player2`
  - `program_output_selected_slot_index=1`
  - `program_output_last_result_kind=Rendered`
  - `program_output_window_title=StreamSync_Program_Output`
- Stability result:
  - OBS target selection: PASS
  - Program visual stability: FAIL / follow-up required
  - observed issue: frequent black/placeholder appearance and large perceived
    stutter during the long capture
- Relevant metrics from the pasted-back run:
  - `frames_attempted=3000`
  - `frames_rendered=2908`
  - `effective_render_fps=15.327`
  - `program_output_render_count=2777`
  - `program_output_missing_selected_source_count=223`
  - `program_output_missing_selected_source_reason=none`
  - `one_shot_decode_elapsed_ms=37600`
  - `quad_view_compose_elapsed_ms=10844`
  - `render_buffer_cpu_scale_copy_elapsed_ms=6332`
  - continuous decoder was disabled
- Follow-up code slice:
  - adds non-spammy ProgramOutput missing/reuse/first-render counters
  - makes explicit Program selection prefer the last valid Program frame when
    the selected source has no newly decoded frame for a tick
  - keeps missing-source reporting; the cached frame is only a visual
    continuity fallback
  - does not reuse Preview placeholders
- Latest reuse rerun interpretation:
  - Program target selection and explicit `player2` identity remained correct
  - internal Program placeholder / black counters were `0` because the last
    valid Program frame was reused after first render
  - visual stability was still not sufficient: missing before first render
    remained `130`, missing after first render / reused previous frame were
    `154`, render FPS was about `14.202`, and one-shot decode took `44474ms`
  - selected Program one-shot decode is too slow for stable Program output
    under this long run
- New opt-in follow-up:
  - `--enable-program-continuous-decode` maps explicit
    `--program-selected-client-id` to the known real slot source and reuses the
    existing single-source continuous decoder for that selected Program source
  - `--program-continuous-decode-mode target-frame` is the default and keeps the
    exact/bounded target-frame lookup behavior
  - `--program-continuous-decode-mode smooth-latest` is Program-only and accepts
    the latest available continuous decoded frame, even when delayed
  - no default behavior changes
  - no OBS setup changes
  - no hotkey / control-pipe switching yet
  - no GPU renderer or Preview slot layout rendering
- Latest Program continuous decode validation:
  - `smooth-latest` succeeded structurally:
    `program_decode_mode=continuous`,
    `program_render_used_continuous_decoded_count=2887`,
    `program_render_used_continuous_latest_count=2887`,
    `program_render_used_continuous_stale_but_accepted_count=2585`,
    `program_render_used_one_shot_fallback_count=1`
  - Program placeholder / black output was no longer the main issue:
    `program_output_placeholder_render_count=0`,
    `program_output_black_frame_render_count=0`,
    `program_output_missing_after_first_render_count=0`
  - remaining smoothness is still FAIL / follow-up required:
    `program_decode_fps=19.724`,
    `continuous_decode_output_throughput_fps=19.724`,
    `effective_render_fps=12.021`,
    `effective_render_fps_after_first_render=12.180`
  - Preview/shared-loop cost is now the leading blocker candidate:
    `quad_view_compose_elapsed_ms=15432`,
    `render_buffer_cpu_scale_copy_elapsed_ms=8798`,
    `one_shot_decode_elapsed_ms=48683`
  - the server stopped at `MaxHandoffRequestsReached` with
    `max_handoff_requests=16000`, so longer 3000-attempt validation should use
    a larger handoff budget
- Current architectural decision:
  - for OBS Program output, smoothness has priority over low latency when
    `smooth-latest` is enabled
  - delayed Program video is acceptable for the MVP if it is smooth
  - exact/bounded `target-frame` mode remains available for future low-latency
    work
- New opt-in Program-first validation mode:
  - `--program-first-validation-mode` keeps default behavior unchanged unless
    explicitly supplied
  - it is intended for ProgramOutput validation with
    `--enable-program-output-window`,
    `--program-selected-client-id <client_id>`,
    `--enable-program-continuous-decode`, and
    `--program-continuous-decode-mode smooth-latest`
  - it keeps Program selected-only and lets Preview reuse its previous composed
    output after the first Preview render, reducing Preview 4-view
    composition/render materialization pressure during Program validation
  - it also enables Program-only decode pressure reduction after the first
    Preview output exists:
    - the Program continuous source keeps the existing continuous one-shot
      suppression behavior while the continuous process is running
    - non-Program Preview sources are prevented from falling through to
      one-shot decode and instead become decode-deferred placeholders for
      Preview
  - Preview is intentionally lower quality / lower freshness in this validation
    mode; Program smoothness is the metric under test
- Latest Program-first + one-shot suppression validation result:
  - classification: strong ProgramOutput structural evidence, not closeout-ready
  - OBS captured `StreamSync Program Output`
  - OBS did not accidentally capture `StreamSync 4-view Output`
  - selected Program identity was not visually distinguishable in this manual
    run because the sources looked similar
  - 4-view / border / debug UI / Preview labels did not mix into Program
  - black screen / placeholder: none
  - perceived stutter: small
  - Program metrics:
    - `program_render_effective_fps=21.795`
    - `effective_program_render_fps=21.795`
    - `continuous_decode_output_throughput_fps=27.102`
    - `program_decode_fps=27.102`
    - `program_render_used_continuous_latest_count=2841`
    - `program_render_used_one_shot_fallback_count=0`
    - `one_shot_decode_attempt_count=0`
    - `program_output_placeholder_render_count=0`
    - `program_output_black_frame_render_count=0`
    - `program_first_suppressed_preview_one_shot_decode_count=2872`
    - `program_first_remaining_one_shot_decode_count=0`
  - Preview result in this validation mode:
    - `StreamSync 4-view Output` was not displayed
    - `frames_rendered=0`
    - `clean_output_render_result_kind=NoRenderableQuadView`
    - `output_width=none`
    - `output_height=none`
  - Interpretation:
    - `--program-first-validation-mode` is a ProgramOutput validation mode, not
      the final operator mode.
    - In this mode, Preview may be absent, stale, or reduced quality.
    - Practical operation still needs a usable `4`-view Preview / multiview
      surface for operator monitoring, but that Preview must stay outside the
      OBS Program scene while Program remains prioritized.
    - Low-frequency Preview refresh is deferred until it can be added without
      reintroducing default non-Program one-shot decode pressure.
- New opt-in operator low-cost Preview restore candidate:
  - `--program-first-preview-refresh-interval <ticks>` is only an operator
    validation aid for Program-first runs; defaults remain unchanged when it is
    absent.
  - The intended command shape keeps ProgramOutput selected-only and
    prioritized:
    `--enable-program-output-window --program-selected-client-id <client_id> --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-first-preview-refresh-interval <ticks>`.
  - On refresh ticks the switcher attempts Preview compose/render again.
  - On non-refresh ticks Program-first still skips/reuses Preview composition.
  - Non-Program Preview one-shot suppression remains active, including the
    first tick in this low-cost mode; Preview must use already available
    decoded/cache/continuous data or render a reduced/placeholder result.
  - If no usable Preview frame exists yet, this mode may still report no
    visible Preview; the diagnostics below are the source of truth.
  - This does not render 4-view as Program and does not change OBS capture
    target selection.
- Latest operator low-cost Preview validation with interval `30`:
  - OBS captured `StreamSync Program Output`
  - OBS did not capture `StreamSync 4-view Output`
  - Program stayed selected-only; 4-view / borders / debug UI / Preview labels
    were not mixed into Program
  - Program black / placeholder: none
  - Program perceived stutter: large in this run
  - `StreamSync 4-view Output` was displayed, but client1 was black and Preview
    was not useful for monitoring
  - refresh was too sparse:
    `operator_preview_refresh_attempt_count=100`,
    `operator_preview_refresh_success_count=4`,
    `operator_preview_refresh_skipped_count=2900`,
    `operator_preview_render_effective_fps=0.031`
  - non-Program Preview one-shot decode was still fully suppressed:
    `one_shot_decode_attempt_count=0`,
    `program_first_remaining_one_shot_decode_count=0`,
    `operator_preview_forced_one_shot_decode_count=0`, and slot0 / client1
    ended as `DecodeDeferred:ContinuousOneShotSuppressed`
  - Program continuous path remained structurally good:
    `program_render_effective_fps=21.886`,
    `program_render_used_continuous_latest_count=2819`,
    `program_render_used_one_shot_fallback_count=0`
- New opt-in low-frequency Preview decode allowance:
  - `--program-first-preview-decode-refresh-interval <ticks>` is disabled by
    default.
  - It only applies when operator low-cost Preview refresh is active.
  - It only permits non-Program Preview one-shot decode on Preview refresh
    ticks that also match the decode refresh interval.
  - The current budget is at most one non-Program Preview source per matching
    refresh tick; additional non-Program decode requests stay suppressed and
    increment the budget-exceeded diagnostic.
  - ProgramOutput remains selected-only and prioritized; Program continuous
    `smooth-latest` selection is unchanged.
- Latest low-cost Preview decode refresh + Program-slot reuse validation with
  `--program-first-preview-refresh-interval 10` and
  `--program-first-preview-decode-refresh-interval 30`:
  - OBS captured `StreamSync Program Output`
  - OBS did not capture `StreamSync 4-view Output`
  - Program stayed selected-only; 4-view / borders / debug UI / Preview labels
    were not mixed into Program
  - Program black / placeholder: sometimes in manual perception, but summary
    counters stayed `0`
  - Program perceived stutter: large
  - Program metrics:
    - `program_render_effective_fps=15.581`
    - `effective_program_render_fps=15.581`
    - `program_render_used_continuous_latest_count=2799`
    - `program_render_used_one_shot_fallback_count=0`
    - `program_output_black_frame_render_count=0`
    - `program_output_placeholder_render_count=0`
    - `continuous_decode_output_throughput_fps=23.976`
    - `one_shot_decode_attempt_count=94`
    - `one_shot_decode_elapsed_ms=16349`
    - `avg_decode_elapsed_ms=174.096`
  - Preview metrics:
    - `operator_preview_decode_refresh_success_count=94`
    - `one_shot_decode_attempt_slot_counts=slot0:94|slot1:0`
    - `operator_preview_reused_program_frame_count=157`
    - `operator_preview_program_slot_visible_count=1`
    - `operator_preview_program_slot_black_count=201`
    - `operator_preview_render_effective_fps=1.431`
    - `operator_preview_non_program_visible_count=1`
  - Interpretation:
    - Program-selected Preview slot reuse exists and can make the Program slot
      visible
    - low-frequency one-shot decode made client1 / slot0 visible sometimes
    - both client1 and client2 still flickered, so Preview was not useful for
      monitoring
    - the one-shot decode cost hurt Program smoothness
    - low-FPS video Preview is now treated as too expensive / unstable for the
      next operator candidate
- New opt-in Program frame reuse for low-cost Preview:
  - applies only under Program-first low-cost Preview mode with explicit
    Program selection
  - the Program-selected Preview slot may reuse the Program continuous latest
    decoded frame, or the last valid Program frame as fallback, without
    requiring one-shot decode for that Program source Preview slot
  - non-Program Preview behavior is unchanged: non-Program sources still need
    `--program-first-preview-decode-refresh-interval <ticks>` for low-frequency
    one-shot decode, and the per-tick budget remains in force
  - ProgramOutput rendering stays separate; the 4-view Preview is not rendered
    as Program, and Preview labels / borders / debug UI are not mixed into the
    Program window
  - the `10` / `30` validation showed this is not enough by itself because the
    slots still flickered
- New opt-in snapshot-style low-cost Preview candidate:
  - flag: `--operator-preview-snapshot-retention`
  - active only with Program-first low-cost Preview flags
  - once a Preview slot has a renderable image, the loop retains it as a
    last-visible snapshot
  - if a later tick would show `DecodeDeferred`, `NoDisplayPlaceholder`,
    source-error / decode-failed placeholder, or a decoded-less held frame, the
    Preview slot reuses the retained snapshot instead
  - the snapshot path does not trigger extra one-shot decode
  - Program source freshness still prefers Program continuous/latest or
    last-valid Program frame before falling back to retained snapshot
  - non-Program source freshness comes from very infrequent one-shot snapshot
    updates
  - next validation should start with:
    `--program-first-preview-refresh-interval 30`,
    `--program-first-preview-decode-refresh-interval 90`, and
    `--operator-preview-snapshot-retention`
- Latest `5` / `90` snapshot-style low-cost Preview validation:
  - command shape:
    `--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-first-preview-refresh-interval 5 --program-first-preview-decode-refresh-interval 90 --operator-preview-snapshot-retention`
  - OBS setup result:
    - OBS captured `StreamSync Program Output`
    - OBS did not capture `StreamSync 4-view Output`
    - Program did not mix 4-view / borders / debug UI / Preview labels
  - ProgramOutput result:
    - classification: structurally promising / partial PASS, not closeout-ready
    - black / placeholder: none
    - perceived stutter: small
    - `program_render_effective_fps=16.201`
    - `effective_program_render_fps=16.201`
    - `program_render_used_continuous_latest_count=2736`
    - `program_render_used_one_shot_fallback_count=0`
    - `program_output_black_frame_render_count=0`
    - `program_output_placeholder_render_count=0`
  - Preview result:
    - `StreamSync 4-view Output` was displayed
    - client1 was not black
    - client2 was not black
    - client1 / client2 flicker: none
    - Preview was still not useful for monitoring
    - Preview update frequency was too low
    - `operator_preview_render_effective_fps=3.233`
    - `operator_preview_refresh_interval_ticks=5`
    - `operator_preview_decode_refresh_interval_ticks=90`
    - `operator_preview_decode_refresh_success_count=31`
    - `operator_preview_snapshot_retention_enabled=true`
    - `operator_preview_snapshot_reuse_count=2743`
    - `operator_preview_placeholder_avoided_by_snapshot_count=2743`
    - `operator_preview_slot_black_after_snapshot_count=0`
    - `one_shot_decode_attempt_count=31`
  - Comparison:
    - `10` / `90` gave better Program FPS around `18.437`, but Preview was
      still too slow
    - `5` / `90` improved Preview repaint to `3.233fps`, but Preview was still
      too slow and Program FPS dropped to `16.201`
  - Decision:
    - snapshot retention works for stable visibility
    - same-loop Preview refresh tuning is limited / paused
    - do not continue lowering Preview refresh interval in this path
    - current Preview is stable snapshot-only, not the final operator
      monitoring Preview
    - do not claim ProgramOutput near-MVP closeout yet
    - recommended next path is a ProgramOutput non-FPS blocker audit, then
      first-render / source-selection / lag criteria work, before any
      closeout claim
- ProgramOutput non-FPS blocker audit:
  - current status: structurally promising, but not closeout-ready
  - FPS remains a blocker, but not the only blocker
  - startup / first render:
    - `program_output_first_render_elapsed_ms=16045`
    - first render delay is too long to close out without an explicit
      acceptance threshold
  - missing selected source before first render:
    - `program_output_missing_selected_source_count=264`
    - `program_output_missing_before_first_render_count=264`
    - `program_output_missing_after_first_render_count=0`
    - `program_output_missing_selected_source_reason=NoDecodedFrameForSelection`
    - this is structurally improved after first render, but startup missing
      selection is still an operational blocker
  - selected source identity / verification:
    - current Program source is CLI-fixed by `--program-selected-client-id`
    - runtime switching is not implemented
    - visual differentiation between player1/player2 is weak, so selected
      source verification cannot rely on manual visual impression alone
    - closeout needs explicit selected client/run/slot identity evidence and
      stronger visual distinguishability in the validation setup
    - current manual client configs use the same capture title for player1 and
      player2:
      `window_title = "Minecraft"`, so two locally captured clients can be
      visually indistinguishable unless the source content itself is prepared
      differently
    - selected-source visual verification marker:
      - Implemented as validation-only client/source-side marker on the real
        encoded bounded PoC:
        `--validation-source-marker <label>`.
      - The marker is produced after capture and before encode / StreamSync
        transport. It changes the source content for validation only; it does
        not add any ProgramOutput overlay.
      - ProgramOutput remains selected-only and clean; it does not draw labels,
        borders, debug UI, or watermarks in normal Program output.
      - The marker is explicit opt-in and disabled by default.
      - Current implementation surface is CLI-only. A future config block can
        be considered if repeated validation needs less command-line state.
      - Client summary diagnostics include:
        `validation_source_marker_enabled`,
        `validation_source_marker_label`, and
        `validation_source_marker_render_count`.
    - approach comparison:
      - A. Validation-only client visual marker / test pattern:
        best fit. It makes player1/player2 visually distinguishable while
        preserving ProgramOutput as a clean selected-source renderer. It proves
        selected Program output by changing the selected source content, not by
        adding Program overlay.
      - B. Validation-only ProgramOutput watermark or corner marker:
        useful for debugging but not preferred. It risks normalizing Program
        overlays and can obscure whether ProgramOutput is clean.
      - C. OBS/manual setup guidance using distinct source windows:
        safe immediate fallback. It requires no code, but relies on operator
        discipline and can drift between reruns.
      - D. Config-level per-client visual marker:
        acceptable as the implementation surface if clearly under validation
        config and default-off. It is less ad hoc than CLI-only, but touches
        config schema.
      - E. Diagnostics-only source identity validation:
        necessary but insufficient. Existing summaries show selected
        client/slot, but manual OBS closeout still needs a visual proof that
        the captured Program image is the requested source.
    - latest selected-source visual verification result: PASS
      - validation setup:
        player1 `--validation-source-marker P1`,
        player2 `--validation-source-marker P2`,
        Program selected source `--program-selected-client-id player2`
      - client summaries showed:
        `validation_source_marker_enabled=true`,
        `validation_source_marker_label=P1|P2`, and
        `validation_source_marker_render_count=9004`
      - switcher summary showed:
        `program_output_requested_client_id=player2`,
        `program_output_selected_client_id=player2`,
        `program_output_selected_slot_index=1`,
        `program_output_black_frame_render_count=0`,
        `program_output_placeholder_render_count=0`,
        `program_render_effective_fps=22.285`, and
        `program_selected_source_frame_lag=5`
      - OBS captured only `StreamSync Program Output`
      - OBS did not capture / display `StreamSync 4-view Output`
      - Program did not mix 4-view layout, borders, debug UI, or Preview labels
      - Program black / placeholder were not observed
      - perceived stutter was small
      - visible marker in ProgramOutput matched selected player2 source (`P2`)
      - interpretation:
        validation-only source-side marker works, selected source identity is
        visually verifiable, and ProgramOutput remains clean / selected-only
  - smooth-latest latency / lag semantics:
    - current smooth-latest behavior still needs explicit acceptance criteria
      separate from render FPS
    - visual source verification is mandatory for `Good` / `Acceptable`.
      If marker visibility or selected-source identity is not verified, the run
      cannot be classified above `Warning`.
    - treat the lag metrics as separate lenses, not one merged number:
      - `program_selected_source_frame_lag`:
        top-level Program output lag relative to the selected source
      - `program_continuous_selected_frame_lag`:
        continuous decoder side lag relative to the selected source
      - `continuous_decode_latest_selected_to_output_frame_gap`:
        gap between the latest selected continuous decoded frame and the frame
        actually rendered to ProgramOutput
    - distinguish startup bootstrap one-shot from steady-state one-shot
      fallback:
      - startup-only bootstrap usage is allowed for `Good` / `Acceptable` when
        it is confined to first render evidence such as
        `program_startup_bootstrap_used_for_first_render=true`
      - repeated one-shot use after steady state is a downgrade signal and is
        counted against the category
    - latest reference values from the selected-source PASS run:
      `program_selected_source_frame_lag=5`,
      `program_continuous_selected_frame_lag=0`,
      `continuous_decode_latest_selected_to_output_frame_gap=5`,
      `program_render_effective_fps=22.285`,
      black / placeholder `0`
    - draft lag acceptance categories:
      - Good:
        source identity visually verified,
        `program_selected_source_frame_lag<=5`,
        `program_continuous_selected_frame_lag<=1`,
        `continuous_decode_latest_selected_to_output_frame_gap<=5`,
        `program_render_effective_fps>=22`,
        black / placeholder `0`,
        steady-state one-shot fallback count `0`,
        startup bootstrap one-shot is allowed if it is first-render-only,
        perceived smoothness is smooth or only tiny stutter
      - Acceptable:
        source identity visually verified,
        `program_selected_source_frame_lag<=8`,
        `program_continuous_selected_frame_lag<=2`,
        `continuous_decode_latest_selected_to_output_frame_gap<=8`,
        `program_render_effective_fps>=20`,
        black / placeholder `0`,
        one-shot fallback limited to startup-only or isolated evidence,
        perceived stutter remains small / watchable
      - Warning:
        source identity is visually verified with degraded metrics, or marker
        readability is incomplete while logs / selected-source diagnostics do
        not contradict the expected source, and any of the following is true:
        `program_selected_source_frame_lag` in `9..12`,
        `program_continuous_selected_frame_lag` in `3..4`,
        `continuous_decode_latest_selected_to_output_frame_gap` in `9..12`,
        `program_render_effective_fps` in `18..19.999`,
        isolated black / placeholder appears,
        or one-shot fallback is needed repeatedly beyond startup
      - Fail:
        source identity appears wrong,
        selected-source diagnostics contradict the expected source,
        black / placeholder recurs,
        one-shot fallback is required as a steady-state crutch,
        perceived smoothness is clearly poor,
        `program_selected_source_frame_lag>12`,
        `program_continuous_selected_frame_lag>4`,
        `continuous_decode_latest_selected_to_output_frame_gap>12`,
        or `program_render_effective_fps<18`
    - reference interpretation:
      the selected-source PASS reference run satisfies draft `Good` because:
      visual verification passed with visible `P2`, lag/gap were `5 / 0 / 5`,
      Program FPS was `22.285`, black / placeholder were `0`, perceived
      stutter was small, and the observed one-shot fallback was startup-only
      bootstrap (`program_render_used_one_shot_fallback_count=1`,
      `program_startup_bootstrap_used_for_first_render=true`) rather than
      steady-state fallback
    - latest smooth-latest lag diagnostics rerun:
      - log dir:
        `S:\stream-sync\manual-logs\program-output-smooth-latest-lag-rerun-20260607-002942`
      - overall ProgramOutput criteria-based validation:
        `WARNING`, not `PASS` and not `FAIL` after marker ambiguity correction
      - OBS safety classification:
        `PASS`
      - Program cleanliness:
        `PASS`
      - selected-source visual verification:
        `PASS`
      - lag criteria classification:
        `Warning`
      - validation setup:
        selected Program source was `player2`, client markers were `P1` / `P2`,
        ProgramOutput was enabled, smooth-latest was enabled, Program-first
        validation mode was enabled, and startup bootstrap one-shot was enabled
      - manual OBS result:
        OBS captured only `StreamSync Program Output`; OBS did not capture
        `StreamSync 4-view Output`; no Preview / multiview source was in the
        Program scene; ProgramOutput title was verified; no wrong-window
        suspicion; no 4-view layout; no black / placeholder; `P2` marker was
        visible in Program; `P1` marker was not visible in Program; perceived
        stutter was small
      - marker ambiguity correction:
        the visible `P2` marker is a source-side validation marker. It is not a
        Program overlay, debug UI, Preview label, or 4-view UI. Therefore the
        manual field that said Program had border/debug UI/Preview label mixed
        in must not be treated as a Program cleanliness failure.
      - main lag metrics:
        `program_selected_source_frame_lag=16`,
        `program_continuous_selected_frame_lag=16`,
        `continuous_decode_latest_selected_to_output_frame_gap=16`,
        `program_render_effective_fps=23.779`
      - smooth-latest diagnostics:
        selected frame `3089`, rendered frame `3073`, latest continuous frame
        `3073`, selected-minus-rendered lag `16`,
        selected-minus-latest-continuous lag `16`,
        rendered-minus-latest-continuous gap `0`, source mismatch count `0`,
        stale reuse count `41`, cache age `1ms`, frame age `1ms`
      - cause classification:
        Program render selection issue is `unlikely`, source mismatch is
        `unlikely`, stale / last-valid reuse is `not primary` but
        `program_smooth_latest_stale_reuse_count=41` remains worth watching,
        and continuous decoder / feed backlog is `likely`
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
        `continuous_decode_output_frame_interval_ms_avg=46.466`, and
        `continuous_decode_output_frame_interval_ms_max=719`
      - result handling:
        ProgramOutput remains clean / selected-only and overall `WARNING`.
        Do not close ProgramOutput near-MVP on this result. Continue
        continuous decoder / feed backlog investigation. `StreamSync 4-view
        Output` remains required for operator monitoring, but must not be
        active in the OBS Program scene.
    - previous completed-template criteria-based validation run:
      - log dir:
        `D:\stream-sync\manual-logs\program-output-criteria-validation-20260606-001029`
      - overall ProgramOutput criteria-based validation:
        `FAIL`, not `PASS`
      - OBS safety classification:
        `PASS`
      - Program cleanliness:
        `PASS`
      - lag criteria classification:
        `Fail`
      - selected-source visual verification:
        `WARNING`
      - retained good facts:
        `program_output_requested_client_id=player2`,
        `program_output_selected_client_id=player2`,
        `program_output_selected_slot_index=1`,
        `program_output_black_frame_render_count=0`,
        `program_output_placeholder_render_count=0`,
        `program_output_missing_after_first_render_count=0`,
        `program_startup_bootstrap_success_count=1`,
        `program_startup_bootstrap_actual_decode_invoked_count=1`,
        `program_startup_bootstrap_used_for_first_render=true`,
        `program_render_used_one_shot_fallback_count=1`,
        client marker diagnostics still showed `P1` / `P2`
      - fail facts:
        `program_selected_source_frame_lag=56`,
        `program_continuous_selected_frame_lag=56`,
        `continuous_decode_latest_selected_to_output_frame_gap=56`,
        `program_render_effective_fps=21.362`
      - selected-source limitation:
        repo-backed manual visual evidence still does not explicitly record that
        `P2` was human-visible in Program and `P1` was absent from Program, so
        selected-source visual verification is not promoted to `PASS`
      - interpretation:
        the improved-marker rerun is now recorded and ProgramOutput remained
        clean/stable with black / placeholder still `0`, but lag worsened far
        beyond the previous `12 / 12 / 12` warning-level run and crossed the
        draft `Fail` threshold. This rerun therefore fails on lag even though
        OBS safety stays `PASS`.
      - result handling:
        keep ProgramOutput closeout blocked, make smooth-latest lag
        investigation the next task, and preserve the requirement that the
        marker remains source-side and opt-in with no ProgramOutput overlay,
        watermark, Preview label, or 4-view-as-Program fallback.
      - follow-up code investigation:
        current code tracing shows smooth-latest ProgramOutput prefers the
        latest matching continuous decoded frame for the selected Program
        source. The equal `56 / 56 / 56` values therefore most likely mean the
        latest matching continuous decoded frame and the Program-rendered frame
        were both 56 frames behind the selected source frame, not that Program
        chose 4-view Preview or a placeholder/cache path. This remains a
        blocker and still needs runtime confirmation with the new diagnostics
        because the manual log directory is not repo-local in this session.
      - added diagnostics for the next rerun:
        `program_smooth_latest_selected_frame_id`,
        `program_smooth_latest_rendered_frame_id`,
        `program_smooth_latest_latest_continuous_frame_id`,
        `program_smooth_latest_selected_minus_rendered_lag`,
        `program_smooth_latest_selected_minus_latest_continuous_lag`,
        `program_smooth_latest_rendered_minus_latest_continuous_gap`,
        `program_smooth_latest_source_mismatch_count`,
        `program_smooth_latest_stale_reuse_count`,
        `program_smooth_latest_cache_age_ms`, and
        `program_smooth_latest_frame_age_ms`.
      - marker improvement status:
        `large-corner-band-v2` is now recorded in the next rerun phase, but a
        repo-backed human-visible `P2` / not-`P1` confirmation is still
        pending
    - lag-focused validation checklist for any future rerun:
      - improved marker is visible and matches the selected source identity
      - client summaries include `validation_source_marker_style` and
        `validation_source_marker_size`
      - OBS captures only `StreamSync Program Output`
      - `StreamSync 4-view Output` is not captured / displayed
      - Program contains no 4-view layout, borders, debug UI, or Preview labels
      - `program_output_black_frame_render_count`
      - `program_output_placeholder_render_count`
      - perceived stutter classification
      - `program_selected_source_frame_lag`
      - `program_continuous_selected_frame_lag`
      - `continuous_decode_latest_selected_to_output_frame_gap`
      - `program_render_effective_fps`
      - `program_render_used_one_shot_fallback_count`
      - whether one-shot fallback was startup-only or steady-state
      - `program_output_missing_after_first_render_count`
      - reusable manual OBS safety template is fully filled in
    - status:
      ProgramOutput closeout stays blocked until the blocker audit is updated;
      current blockers still include lag `Fail` and non-`PASS` selected-source
      visual verification.
  - OBS capture safety:
    - latest OBS target separation was correct
    - OBS remains manual and can still be pointed at the wrong window
    - closeout needs a checklist for exact `StreamSync Program Output` capture,
      wrong-window prevention, and pasted-back evidence
    - OBS ProgramOutput capture safety checklist:
      - check the OBS scene/source list before validation begins
      - the Program scene must capture only `StreamSync Program Output`
      - `StreamSync 4-view Output` must be hidden, removed, or not present in
        the Program scene
      - `StreamSync 4-view Output` may remain open and visible for operator
        monitoring outside the Program scene
      - the Program scene must not contain Preview / multiview capture sources
      - verify the actual window title is `StreamSync Program Output`
      - when validation markers are enabled, the selected-source marker must be
        visible in ProgramOutput
      - wrong-window capture is automatic `FAIL`
    - PASS / WARNING / FAIL:
      - PASS:
        only `StreamSync Program Output` is captured, the correct selected
        marker is visible, no 4-view / debug / Preview UI appears, and black /
        placeholder counts are `0`
      - WARNING:
        the OBS source list still contains an old or disabled 4-view source but
        it is hidden / inactive in the Program scene, or the marker is hard to
        read but source identity is still confirmed by logs plus selected-source
        evidence, or the manual OBS safety template is incomplete while the run
        otherwise looks structurally correct
      - FAIL:
        OBS captures `StreamSync 4-view Output`, the Program scene includes a
        4-view / multiview source, the wrong marker or wrong source appears,
        debug UI / labels / borders appear in Program, or black / placeholder
        recurs
    - operator preflight checklist:
      - launch server
      - launch switcher with ProgramOutput enabled
      - launch clients, adding validation markers such as `P1` / `P2` when the
        run is a source-identity validation
      - verify the OBS scene/source target before capture
      - verify the ProgramOutput window title is `StreamSync Program Output`
      - verify the expected selected marker / source
      - verify no Preview / multiview UI appears in Program
    - reusable manual validation template:
      - validation purpose:
        OBS ProgramOutput capture safety
      - Program selected source:
        `player2` or current validation target
      - validation markers:
        `client1=P1`, `client2=P2`, or `disabled`
      - OBS Program scene source list checked:
        `yes/no`
      - OBS capture target:
        `StreamSync Program Output` / `other`
      - `StreamSync 4-view Output` present in Program scene:
        `no/hidden/disabled/yes`
      - Preview / multiview source present in Program scene:
        `no/yes`
      - ProgramOutput window title verified:
        `yes/no`
      - selected marker visible and correct:
        `yes/no`
      - wrong-window suspicion:
        `no/yes`
      - Program includes 4-view / border / debug UI / Preview labels:
        `no/yes`
      - `program_output_black_frame_render_count`
      - `program_output_placeholder_render_count`
      - `program_selected_source_frame_lag`
      - `program_continuous_selected_frame_lag`
      - `continuous_decode_latest_selected_to_output_frame_gap`
      - `program_render_effective_fps`
      - `program_render_used_one_shot_fallback_count`
      - startup-only bootstrap or steady-state fallback:
        `startup-only/steady-state/none`
      - final safety classification:
        `PASS/WARNING/FAIL`
      - notes:
        free-text operator notes
  - Program-first validation mode:
    - `--program-first-validation-mode` is still a validation mode, not final
      operator mode
    - current 4-view Preview is stable snapshot-only and not final operator
      monitoring Preview
  - runtime control:
    - hotkey/control pipe is not implemented
    - current source identity is static / CLI-fixed
- ProgramOutput first-render diagnostic slice:
  - implemented diagnostics-only startup fields for the next rerun; default
    behavior is unchanged
  - new elapsed fields:
    - `program_selection_resolved_elapsed_ms`
    - `program_continuous_source_resolved_elapsed_ms`
    - `program_first_source_frame_seen_elapsed_ms`
    - `program_first_continuous_input_elapsed_ms`
    - `program_first_continuous_output_elapsed_ms`
    - `program_first_renderable_decoded_frame_elapsed_ms`
  - new startup classification fields:
    - `program_first_render_waiting_for_decode_count`
    - `program_first_render_missing_reason_counts`
    - `program_startup_one_shot_fallback_allowed`
    - `program_startup_one_shot_fallback_attempt_count`
    - `program_startup_one_shot_fallback_suppressed_count`
    - `program_startup_continuous_pending_count`
    - `program_startup_no_selected_source_count`
    - `program_startup_no_decoded_frame_count`
    - `program_startup_latest_continuous_available_count`
    - `program_startup_latest_continuous_rejected_count`
    - `program_startup_source_identity_mismatch_count`
  - next rerun should compare source frame seen vs continuous input timing,
    continuous input vs continuous output timing, first renderable decoded frame
    vs first Program render timing, startup missing reason counts, and one-shot
    fallback allowed / attempted / suppressed counters
  - latest diagnostic rerun interpretation:
    - `program_selection_resolved_elapsed_ms=0`
    - `program_continuous_source_resolved_elapsed_ms=0`
    - `program_first_source_frame_seen_elapsed_ms=4702`
    - `program_first_continuous_input_elapsed_ms=4702`
    - `program_first_continuous_output_elapsed_ms=6826`
    - `program_first_renderable_decoded_frame_elapsed_ms=6826`
    - `program_output_first_render_elapsed_ms=6826`
    - `program_first_render_missing_reason_counts=NoDecodedFrameForSelection:170|RequestedClientNotInRealSlots:0|unknown:0`
    - `program_startup_one_shot_fallback_allowed=true`
    - `program_startup_one_shot_fallback_attempt_count=0`
    - `program_startup_one_shot_fallback_suppressed_count=34`
    - source identity and selected-source resolution are not the cause in this
      run
    - missing selected source happens before first render only
    - about 2.5s of the elapsed first-render time may be validation process
      start order, because the script starts switcher first, then client1, then
      selected client2
    - after the selected source frame first appears, continuous first output
      and first Program render line up at 6826ms; the remaining observed gap is
      primarily continuous decode startup/output readiness
  - `program_startup_one_shot_fallback_allowed=true` means smooth-latest
    ProgramOutput is allowed to consume an already decoded selected frame as a
    startup fallback. It does not mean ProgramOutput starts a one-shot decode
    itself. If the validation/pre-composition path has not produced a selected
    decoded frame, the fallback attempt counter remains 0.
  - added candidate diagnostics for the next rerun:
    - `program_startup_one_shot_fallback_blocked_reason_counts`
    - `program_startup_selected_frame_keyframe_available_count`
    - `program_startup_selected_frame_source_counts`
    - `program_startup_retained_keyframe_available_count`
    - `program_startup_one_shot_candidate_count`
    - `program_startup_one_shot_candidate_rejected_count`
    - `program_startup_one_shot_candidate_rejected_reason_counts`
  - startup validation should now be run in two start-order shapes:
    - current switcher-first shape:
      start switcher, wait 2s, start client1, wait 0.5s, start selected
      client2
    - clients-before-switcher shape:
      start server and both clients first, wait until live/retained frames are
      available for the selected client, then start switcher ProgramOutput
    - compare:
      `program_first_source_frame_seen_elapsed_ms`,
      `program_first_continuous_input_elapsed_ms`,
      `program_first_continuous_output_elapsed_ms`,
      `program_first_renderable_decoded_frame_elapsed_ms`, and
      `program_output_first_render_elapsed_ms`
    - purpose: separate validation process start order delay from actual
      ProgramOutput decode/render startup delay
  - clients-before-switcher rerun result:
    - `program_first_source_frame_seen_elapsed_ms=246`
    - `program_first_continuous_input_elapsed_ms=246`
    - `program_first_continuous_output_elapsed_ms=1964`
    - `program_first_renderable_decoded_frame_elapsed_ms=1964`
    - `program_output_first_render_elapsed_ms=1964`
    - `program_output_missing_before_first_render_count=29`
    - `program_output_missing_after_first_render_count=0`
    - `program_output_black_frame_render_count=0`
    - `program_output_placeholder_render_count=0`
    - `program_render_effective_fps=19.589`
    - interpretation: process start order delay is separated, but continuous
      first output still takes about 1.6s after first selected source input
  - startup-only one-shot bootstrap is now implemented as opt-in
    `--program-startup-bootstrap-one-shot`:
    - default behavior remains unchanged
    - ProgramOutput only, no Preview one-shot fallback revival
    - only before first Program render
    - requires explicit `--program-selected-client-id`
    - does not override continuous latest, last-valid Program frame, or an
      already decoded selected frame
    - only candidates selected keyframe / retained-keyframe startup source
    - diagnostics:
      `program_startup_bootstrap_enabled`,
      `program_startup_bootstrap_attempt_count`,
      `program_startup_bootstrap_success_count`,
      `program_startup_bootstrap_elapsed_ms`,
      `program_startup_bootstrap_source_counts`,
      `program_startup_bootstrap_rejected_reason_counts`,
      `program_startup_bootstrap_used_for_first_render`
  - clients-before-switcher bootstrap A/B result:
    - baseline without bootstrap:
      `program_output_first_render_elapsed_ms=1964`,
      `program_output_missing_before_first_render_count=29`
    - bootstrap enabled:
      `program_startup_bootstrap_enabled=true`,
      `program_startup_bootstrap_attempt_count=27`,
      `program_startup_bootstrap_success_count=0`,
      `program_startup_bootstrap_elapsed_ms=0`,
      `program_startup_bootstrap_source_counts=queue:0|retained_keyframe:27|none:0|unknown:0`,
      `program_startup_bootstrap_rejected_reason_counts=disabled:0|not_explicit_selection:0|after_first_render:0|no_selected_frame:7|no_keyframe_candidate:0|continuous_latest_preferred:1|last_valid_preferred:0|selected_decoded_preferred:0|decode_failed:27|unknown:0`,
      `program_startup_bootstrap_used_for_first_render=false`,
      `program_output_first_render_elapsed_ms=2666`,
      `program_output_missing_before_first_render_count=34`
    - steady state remained okay:
      `program_output_black_frame_render_count=0`,
      `program_output_placeholder_render_count=0`,
      `program_render_effective_fps=20.984`
    - interpretation: bootstrap attempted retained-keyframe-classified
      candidates but decoded none, was not used for first render, and worsened
      startup timing in this run. Do not claim bootstrap success.
  - follow-up bootstrap diagnostics:
    - `program_startup_bootstrap_attempt_count=24`
    - `program_startup_bootstrap_success_count=0`
    - `program_startup_bootstrap_decode_attempt_elapsed_ms=41`
    - `program_startup_bootstrap_actual_decode_invoked_count=0`
    - `program_startup_bootstrap_decode_skipped_before_invoke_count=24`
    - `program_startup_bootstrap_decode_error_counts=failed:0|deferred_empty_payload:0|deferred_invalid_dimensions:0|deferred_ffmpeg_unavailable:0|deferred_continuous_one_shot_suppressed:24|unknown:0`
    - payload diagnostics showed retained candidates with SPS/PPS/IDR present,
      so the observed failure was not missing parameter sets or empty payload.
    - interpretation: bootstrap decode was routed through the continuous slot0
      one-shot suppression gate and skipped before actual FFmpeg one-shot
      invocation.
  - code follow-up:
    - Program startup bootstrap decode now uses a separate decode purpose from
      normal Preview fallback decode.
    - Preview fallback still uses the existing continuous / Program-first
      one-shot suppression behavior.
    - bootstrap remains opt-in, startup-only, explicit-selection-only, and
      ProgramOutput-only. This is not a bootstrap success claim; it requires a
      new A/B rerun.
  - bootstrap decode failure investigation now adds diagnostics for:
    `program_startup_bootstrap_decode_attempt_elapsed_ms`,
    `program_startup_bootstrap_decode_error_counts`,
    `program_startup_bootstrap_ffmpeg_exit_status`,
    `program_startup_bootstrap_ffmpeg_stderr_summary`,
    `program_startup_bootstrap_payload_bytes_min/max/avg`,
    `program_startup_bootstrap_payload_nal_kinds`,
    `program_startup_bootstrap_payload_has_sps_count`,
    `program_startup_bootstrap_payload_has_pps_count`,
    `program_startup_bootstrap_payload_has_idr_count`,
    `program_startup_bootstrap_frame_id_min/max`,
    `program_startup_bootstrap_slot_counts`,
    `program_startup_bootstrap_client_counts`,
    `program_startup_bootstrap_actual_decode_invoked_count`,
    `program_startup_bootstrap_decode_skipped_before_invoke_count`.
  - latest clients-before-switcher bootstrap bypass rerun: PASS
    - command:
      `--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-startup-bootstrap-one-shot`
    - `program_startup_bootstrap_enabled=true`
    - `program_startup_bootstrap_attempt_count=1`
    - `program_startup_bootstrap_success_count=1`
    - `program_startup_bootstrap_actual_decode_invoked_count=1`
    - `program_startup_bootstrap_decode_skipped_before_invoke_count=0`
    - `program_startup_bootstrap_decode_error_counts=failed:0|deferred_empty_payload:0|deferred_invalid_dimensions:0|deferred_ffmpeg_unavailable:0|deferred_continuous_one_shot_suppressed:0|unknown:0`
    - `program_startup_bootstrap_used_for_first_render=true`
    - `program_output_first_render_elapsed_ms=354`
    - `program_output_missing_selected_source_count=0`
    - `program_output_missing_before_first_render_count=0`
    - `program_output_missing_after_first_render_count=0`
    - `program_output_black_frame_render_count=0`
    - `program_output_placeholder_render_count=0`
    - `program_window_render_success_count=3000`
    - `program_window_render_failure_count=0`
    - `program_render_effective_fps=23.279`
    - `program_render_used_one_shot_fallback_count=1`
    - `program_first_continuous_output_elapsed_ms=1928`
    - `continuous_decode_first_input_to_first_output_elapsed_ms=1688`
    - interpretation:
      the previous `ContinuousOneShotSuppressed` bootstrap wiring bug is fixed,
      bootstrap now reaches actual decode, the first Program render uses the
      bootstrap frame, and the earlier startup missing-selected-source problem
      is eliminated in the clients-before-switcher shape.
    - scope note:
      this is still not ProgramOutput closeout. Continuous first output remains
      about `1.6-1.9s`, but bootstrap hides that wait for the first Program
      render in this start order.
  - latest switcher-first cold-start bootstrap rerun: PASS / limitation
    clarified
    - command shape:
      `--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-startup-bootstrap-one-shot`
    - `program_output_first_render_elapsed_ms=3803`
    - `program_output_missing_selected_source_count=102`
    - `program_output_missing_before_first_render_count=102`
    - `program_output_missing_after_first_render_count=0`
    - `program_output_first_render_attempt_index=103`
    - `program_first_source_frame_seen_elapsed_ms=3590`
    - `program_first_continuous_input_elapsed_ms=3803`
    - `program_first_renderable_decoded_frame_elapsed_ms=3803`
    - `program_first_continuous_output_elapsed_ms=5330`
    - `program_startup_bootstrap_attempt_count=1`
    - `program_startup_bootstrap_success_count=1`
    - `program_startup_bootstrap_actual_decode_invoked_count=1`
    - `program_startup_bootstrap_decode_skipped_before_invoke_count=0`
    - `program_startup_bootstrap_used_for_first_render=true`
    - `program_startup_bootstrap_decode_error_counts=failed:0|deferred_empty_payload:0|deferred_invalid_dimensions:0|deferred_ffmpeg_unavailable:0|deferred_continuous_one_shot_suppressed:0|unknown:0`
    - `program_startup_one_shot_fallback_blocked_reason_counts=no_selected_frame:102|no_selected_decoded_frame:0|continuous_latest_preferred:0|last_valid_preferred:0|unknown:0`
    - `program_startup_bootstrap_rejected_reason_counts=no_selected_frame:102`
    - `program_output_black_frame_render_count=0`
    - `program_output_placeholder_render_count=0`
    - `program_window_render_success_count=2898`
    - `program_window_render_failure_count=102`
    - `program_render_effective_fps=22.381`
    - `program_selected_source_frame_lag=49`
    - interpretation:
      bootstrap reaches actual one-shot decode in both validated start orders
      and can be used for the first Program render once a selected source frame
      exists. In switcher-first cold start, the remaining startup delay is
      primarily waiting for the selected `player2` source frame to arrive.
      Bootstrap cannot render selected-only ProgramOutput before that selected
      frame exists; it only reduces decode / continuous-startup latency after
      source arrival. After first render, missing selected source, black, and
      placeholder counters stayed at `0`.
  - ProgramOutput startup readiness semantics:
    - `program_selection_configured`:
      ProgramOutput is enabled and an explicit Program client selection has
      been resolved.
    - `program_selected_source_waiting`:
      selected-only Program is configured, but no selected source frame is
      available yet. Rendering is not expected in this state.
    - `program_selected_source_seen`:
      the selected client/run has produced at least one selected source frame.
    - `program_first_frame_bootstrapping`:
      first Program render has not happened yet and startup bootstrap is
      attempting to convert the selected source frame into a renderable decoded
      frame.
    - `program_first_frame_rendered`:
      first selected Program frame has rendered, whether from bootstrap,
      continuous latest, selected decoded, or last-valid path.
    - `program_steady_state`:
      after first Program render, normal selected-source Program rendering is
      running; missing selected source / black / placeholder should remain `0`
      in current validated shapes.
  - ProgramOutput startup readiness diagnostics are now implemented as narrow
    summary-only fields:
    - `program_startup_readiness_state`
    - `program_selected_source_wait_elapsed_ms`
    - `program_startup_waiting_for_selected_source_count`
    - `program_startup_bootstrap_after_source_seen_elapsed_ms`
    - `program_startup_selected_source_seen_count`
    These fields do not change bootstrap, smooth-latest, Preview, OBS, or
    Program rendering behavior.
  - Summary value note:
    - `disabled` is used only when ProgramOutput itself is disabled.
    - ProgramOutput-enabled startup states remain the readiness states above.
  - Future readiness diagnostics should not be broadened yet:
    - `program_startup_readiness_state`
    - `program_selected_source_wait_elapsed_ms`
    - `program_startup_waiting_for_selected_source_count`
    - `program_startup_bootstrap_after_source_seen_elapsed_ms`
    - `program_startup_selected_source_seen_count`
    Keep these diagnostics-only unless later validation proves a runtime
    behavior change is needed.
  - other candidate fixes remain deferred:
    - startup continuous decode prewarm
    - startup blocking wait for first continuous frame
    - first-frame special policy for ProgramOutput
    - source identity / run_id mismatch fix if proven
- ProgramOutput closeout criteria to define before closeout:
  - Output correctness:
    - Program window renders only the selected Program source
    - no 4-view layout, Preview labels, slot borders, or debug UI are mixed in
    - black / placeholder counters remain `0` after first valid render
  - Startup behavior:
    - maximum accepted `program_output_first_render_elapsed_ms`
    - maximum accepted missing-selected-source count before first render
    - explicit handling for `NoDecodedFrameForSelection`
  - Source selection correctness:
    - selected client/run/slot identity is visible in summary diagnostics
    - validation sources are visually distinguishable enough for manual check
    - current static CLI selection limitations are documented
  - OBS capture safety:
    - OBS captures exactly `StreamSync Program Output`
    - OBS does not capture `StreamSync 4-view Output`
    - wrong/stale window capture is checked and recorded
    - the Program scene/source list is checked before validation
    - Preview / multiview capture sources are absent from the Program scene
    - the checklist result is recorded as `PASS`, `WARNING`, or `FAIL`
  - Latency / lag acceptance:
    - accepted smooth-latest lag bounds are defined
    - `program_selected_source_frame_lag`,
      `program_continuous_selected_frame_lag`, and
      `continuous_decode_latest_selected_to_output_frame_gap` are included in
      the gate
    - perceived smoothness, black / placeholder count, one-shot fallback count,
      Program FPS, and visual source verification are included together rather
      than treating lag as a single-number gate
  - Stability over long run:
    - no black / placeholder regression
    - no after-first-render selected-source missing regression
    - render FPS and perceived stutter remain within an agreed range
  - Operator visibility / Preview dependency:
    - ProgramOutput closeout does not require finishing same-loop final Preview
      tuning first
    - production operation still requires a usable `4`-view Preview surface for
      operator monitoring and source choice
    - `StreamSync 4-view Output` must stay out of the OBS Program scene even
      when it remains visible to the operator
    - current same-loop Preview is accepted only as stable snapshot-only
  - Diagnostics completeness:
    - first render, missing-source reason, selected identity, lag, black /
      placeholder, and OBS target evidence are all available in pasted-back
      validation
- New fields to watch on the next Program OBS rerun:
  - `program_first_validation_enabled`
  - `program_first_preview_visible`
  - `program_first_preview_refresh_interval`
  - `program_first_preview_refresh_count`
  - `program_first_preview_suppressed_count`
  - `operator_low_cost_preview_enabled`
  - `operator_preview_refresh_interval_ticks`
  - `operator_preview_refresh_attempt_count`
  - `operator_preview_refresh_success_count`
  - `operator_preview_refresh_skipped_count`
  - `operator_preview_used_stale_frame_count`
  - `operator_preview_forced_one_shot_decode_count`
  - `operator_preview_render_effective_fps`
  - `operator_preview_decode_refresh_enabled`
  - `operator_preview_decode_refresh_interval_ticks`
  - `operator_preview_decode_refresh_attempt_count`
  - `operator_preview_decode_refresh_success_count`
  - `operator_preview_decode_refresh_skipped_count`
  - `operator_preview_decode_refresh_source_counts`
  - `operator_preview_decode_refresh_elapsed_ms`
  - `operator_preview_decode_refresh_budget_exceeded_count`
  - `operator_preview_non_program_visible_count`
  - `operator_preview_reused_program_frame_count`
  - `operator_preview_program_slot_visible_count`
  - `operator_preview_program_slot_reuse_source`
  - `operator_preview_program_slot_black_count`
  - `operator_preview_snapshot_retention_enabled`
  - `operator_preview_snapshot_reuse_count`
  - `operator_preview_snapshot_reuse_slot_counts`
  - `operator_preview_snapshot_created_count`
  - `operator_preview_snapshot_created_slot_counts`
  - `operator_preview_placeholder_avoided_by_snapshot_count`
  - `operator_preview_slot_black_after_snapshot_count`
  - `operator_preview_program_fps_impact_estimate`
  - `preview_compose_skipped_for_program_count`
  - `preview_compose_reused_for_program_count`
  - `program_render_loop_attempt_count`
  - `program_window_render_success_count`
  - `program_window_render_failure_count`
  - `program_render_effective_fps`
  - `effective_program_render_fps`
  - `program_selection_resolved_elapsed_ms`
  - `program_continuous_source_resolved_elapsed_ms`
  - `program_first_source_frame_seen_elapsed_ms`
  - `program_first_continuous_input_elapsed_ms`
  - `program_first_continuous_output_elapsed_ms`
  - `program_first_renderable_decoded_frame_elapsed_ms`
  - `program_first_render_waiting_for_decode_count`
  - `program_first_render_missing_reason_counts`
  - `program_startup_one_shot_fallback_allowed`
  - `program_startup_one_shot_fallback_attempt_count`
  - `program_startup_one_shot_fallback_suppressed_count`
  - `program_startup_one_shot_fallback_blocked_reason_counts`
  - `program_startup_selected_frame_keyframe_available_count`
  - `program_startup_selected_frame_source_counts`
  - `program_startup_retained_keyframe_available_count`
  - `program_startup_one_shot_candidate_count`
  - `program_startup_one_shot_candidate_rejected_count`
  - `program_startup_one_shot_candidate_rejected_reason_counts`
  - `program_startup_continuous_pending_count`
  - `program_startup_no_selected_source_count`
  - `program_startup_no_decoded_frame_count`
  - `program_startup_latest_continuous_available_count`
  - `program_startup_latest_continuous_rejected_count`
  - `program_startup_source_identity_mismatch_count`
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
  - `program_first_suppressed_preview_one_shot_decode_count`
  - `program_first_suppressed_preview_one_shot_decode_slot_counts`
  - `program_first_program_only_decode_path_enabled`
  - `program_first_remaining_one_shot_decode_count`
- Long validation handoff budget guidance:
  - avoid `max_handoff_requests=16000` for 3000-attempt Program-first reruns
  - prefer a large bounded value such as `64000`, or the current equivalent
    no-budget / very-large-budget local validation setting if one is used in
    the script, so server shutdown does not end the run early
- OBS capture target remains manual and unchanged by code.

Classification for this run:

- OBS capture validation: PASS
- StreamSync runtime under same-PC OBS load: PARTIAL
- partial reason:
  - same-PC saturation observed
  - `client2` stopped with `EncodeFailure`
  - `client3` stopped with `EncodeFailure`
  - client effective output fps fell into the `16-18fps` range
  - switcher final summary was not collected because the switcher did not exit within `360` seconds
- this does not roll back the earlier same-PC `4`-client all-real PASS from
  `manual-logs/four-client-20260513-184503`

## Position After The Latest Result

- Keep the OBS capture side as `PASS`.
- Keep the same-PC runtime side as `PARTIAL`.
- Do not treat missing switcher final summary from this long run as an OBS
  capture failure by itself.
- Treat `client2` / `client3` `EncodeFailure`, `16-18fps`, and very low
  switcher FPS as same-PC saturation follow-up evidence.
- Move the next docs-first step to:
  - `docs/operations/distributed-pc-validation.md`

## Validation Purpose

The purpose of this follow-up is narrow:

- confirm the current real `4`-client PASS-shaped StreamSync output window can
  be selected from normal OBS `Window Capture`
- confirm OBS preview can display the downstream clean output window
  `StreamSync 4-view Output`
- confirm the visible OBS result is the expected `4`-view output, not a black
  surface, transparent surface, placeholder-only surface, or wrong window

This is not an automation slice. StreamSync still owns render/output behavior,
and OBS remains a manual downstream capture target.

## Reused PASS Baseline

Reuse the latest PASS evidence as the runtime baseline:

- log dir:
  - `manual-logs/four-client-20260513-184503`
- server:
  - `receive_ready=true`
  - `handoff_ready=true`
  - `runtime_mode=concurrent`
  - `frames_queued=3600`
  - `retained_keyframe_clients=4`
  - `frame_read_count=526`
  - `io_error_count=0`
- switcher:
  - `preview_mode=preview-latest-decodable`
  - `read_mode=inspect-latest-decodable`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `window_title=StreamSync 4-view Output`
  - `output_width=1280`
  - `output_height=720`
- clients:
  - `player1..player4` all had `frames_sent=900`
  - `send_failures=0`
  - `h264_parameter_sets_cached=true`
- same-PC saturation remains a known follow-up:
  - client `effective_output_fps` landed around `19-20fps`
  - this does not by itself invalidate OBS capture PASS if capture visibility
    succeeds

## OBS-Side Check Items

OBS must confirm the following:

1. `StreamSync 4-view Output` is visible as a capturable window.
2. A normal OBS `Window Capture` source can target that exact window title.
3. OBS preview shows the StreamSync `4`-view output surface.
4. The preview is not black and not transparent.
5. The preview is not a wrong or stale window.
6. All `4` slots are visible in the preview rather than placeholder-only or
   partially missing output.

## StreamSync Launch Conditions

Use the same runtime shape as the latest PASS and keep collecting stdout/stderr
evidence for `server`, `switcher`, and `client1..4`.

### Evidence Directory

Create a new directory before the run:

- `manual-logs/obs-capture-<yyyymmdd-hhmmss>`

Keep the same file pattern as the latest PASS:

- `server.log`
- `server.err.log`
- `switcher.log`
- `switcher.err.log`
- `client1.log` .. `client4.log`
- `client1.err.log` .. `client4.err.log`

Add manual OBS evidence beside those logs when available:

- `obs-preview.png`
- optional `obs-window-capture-properties.png`
- a short pasted-back judgment note with the primary failure bucket

### Server

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-continuous configs/manual/server.two-real-slots.toml streamsync-handoff-dev 4000 300000 600000 0 0 false 268435456 0 0
```

Expected ready/stopped evidence:

- `receive_ready=true`
- `handoff_ready=true`
- `runtime_mode=concurrent`
- `expected_reassembled_frames_enabled=false`
- `expected_clients_enabled=false`
- `expected_per_client_frames_enabled=false`
- final `frames_queued=3600`
- final `retained_keyframe_clients=4`

### Switcher

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 180 preview-latest-decodable
```

Expected switcher evidence:

- `real_handoff=true`
- `real_slot_count=4`
- `preview_mode=preview-latest-decodable`
- `read_mode=inspect-latest-decodable`
- final `scheduler_status=AllSelected`
- final `slot_result_kinds=Selected|Selected|Selected|Selected`
- final `clean_output_render_result_kind=Rendered`
- final `window_title=StreamSync 4-view Output`
- final `output_width=1280`
- final `output_height=720`

### Clients

Client 1:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

Client 2:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

Client 3:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

Client 4:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

Expected client evidence for all `4` clients:

- `accepted=true`
- `frames_sent=900`
- `send_failures=0`
- `keyframes_sent=30`
- `h264_parameter_sets_cached=true`
- `stop_reason=Some(MaxFramesReached)`

## Manual Validation Recipe

### Preflight

1. Create `manual-logs/obs-capture-<timestamp>` and prepare to keep
   `server` / `switcher` / `client1..4` stdout evidence.
2. Confirm `OBS` is ready to add or edit a normal `Window Capture` source.
3. Confirm the StreamSync target title for this run is exactly:
   - `StreamSync 4-view Output`
4. Close or ignore stale sources/windows that could cause wrong-window
   selection by title.

### Runtime Order

1. Start OBS first.
2. Start the server command and wait for the ready line.
3. Start the switcher command.
4. Start `client1`, `client2`, `client3`, `client4` with minimal manual delay.
5. While the switcher output window is alive, create or edit an OBS
   `Window Capture` source.
6. Select window title:
   - `StreamSync 4-view Output`
7. Confirm OBS preview shows the StreamSync output instead of black,
   transparent, or a wrong window.
8. Confirm the preview shows the `4`-view layout rather than placeholder-only
   content.
9. Save or paste back:
   - OBS preview screenshot
   - the exact selected window title
   - PASS/FAIL judgment
   - primary failure classification when FAIL
10. Wait for final `client1..4` summaries and final server stopped summary.

### Manual OBS Checklist

- `OBS` started before or during the StreamSync runtime
- `Window Capture` source created or updated manually
- selected window title is exactly `StreamSync 4-view Output`
- OBS preview shows the downstream StreamSync output window
- `4` visible slots are present in preview
- pasted-back evidence includes:
  - preview screenshot
  - visual judgment
  - log dir path

## Success Criterion

Treat the OBS capture follow-up as PASS only when all of the following are
true.

### StreamSync Runtime Gate

- latest run still satisfies the existing `4`-client PASS gate:
  - server ready/stopped summaries are present
  - all `4` clients still send `900` frames with `send_failures=0`
  - switcher final state still reaches:
    - `scheduler_status=AllSelected`
    - `slot_result_kinds=Selected|Selected|Selected|Selected`
    - `clean_output_render_result_kind=Rendered`
    - `window_title=StreamSync 4-view Output`

### OBS Capture Gate

- OBS can find `StreamSync 4-view Output` in normal `Window Capture`
- OBS is pointed at that exact window title
- OBS preview shows the StreamSync output window
- OBS preview is not black
- OBS preview is not transparent
- OBS preview is not a wrong or stale window
- all `4` slots are visible in preview

### Evidence Gate

- `server` / `switcher` / `client1..4` stdout evidence is preserved
- OBS preview screenshot or equivalent pasted-back visual evidence is preserved
- the final report includes PASS/FAIL plus a primary failure bucket

Interpretation:

- same-PC saturation may still be present in the client FPS metrics
- if the capture path is visually correct and the runtime gate still passes,
  classify that as PASS with a separate performance follow-up, not as an OBS
  capture failure
- if OBS capture is visually correct but the same-PC runtime is partial, record
  it as `OBS capture PASS / StreamSync runtime PARTIAL` and keep the earlier
  `4`-client PASS checkpoint intact

## Failure Classification

Classify one primary bucket before proposing any code or runtime change.

### 1. StreamSync Window Not Found

Use this when:

- OBS `Window Capture` cannot find `StreamSync 4-view Output`
- the title list does not expose the expected window while the switcher runtime
  should still be active

Typical evidence:

- switcher stdout still names `window_title=StreamSync 4-view Output`
- OBS window list does not show that title

### 2. OBS Captures Black Screen Or Transparent Surface

Use this when:

- OBS is pointed at `StreamSync 4-view Output`
- the preview is black or transparent
- switcher stdout still indicates rendered output

Typical evidence:

- source properties show the correct title
- OBS preview is black/transparent
- switcher final `clean_output_render_result_kind=Rendered`

### 3. OBS Captures Wrong Window / Window Title Mismatch

Use this when:

- OBS source is attached to a different title than
  `StreamSync 4-view Output`
- the preview shows another app, stale window, or unintended StreamSync window

Typical evidence:

- source properties do not show the exact target title
- visible preview content does not match the intended `4`-view output

### 4. StreamSync Window Exists But No Rendered Content

Use this when:

- the StreamSync output window exists, but the window itself has no useful
  rendered content before OBS is even considered
- visible output is blank or placeholder-only because the runtime gate did not
  actually reach the expected render state

Typical evidence:

- `clean_output_render_result_kind!=Rendered`, or
- final switcher state is not the expected all-selected result

### 5. 4 Slots Not Visible

Use this when:

- OBS is capturing the intended window
- a visible output exists
- but the preview does not show all `4` intended slots

Typical evidence:

- one or more quadrants are missing
- one or more quadrants remain placeholder-only
- the preview is cropped or otherwise not showing the intended `4`-way layout

### 6. Same-PC Performance Saturation

Use this when:

- the capture path basically works
- OBS can find and display the correct window
- but same-PC load still causes visible stutter or low effective FPS

Typical evidence:

- client `effective_output_fps` lands around the known `19-20fps` band
- runtime gate still passes
- OBS preview is valid but motion is degraded

Response rule:

- keep this as a performance follow-up
- do not treat it as a failed OBS capture unless visibility/selection also
  fails

## Pasted-Back Report Template

Use this minimum operator report shape:

```text
log_dir=manual-logs/obs-capture-<timestamp>
selected_window_title=StreamSync 4-view Output|<other>
obs_preview_result=Visible4View|Black|Transparent|WrongWindow|Partial4View
primary_classification=<one bucket above>
obs_preview_screenshot=<path or pasted image>
notes=<short human observation>
```

## Long Run Summary Handling

This OBS doc keeps two judgments separate:

- OBS capture result
- switcher final-summary recovery

Rule:

- a long OBS-operation run may still be `OBS capture PASS` even if the
  switcher final summary is not recovered within the local wait window

Preferred handling:

1. use a short bounded run when switcher final summary is the main gate
2. use a longer run when the operator needs more OBS interaction time
3. if both are needed, do two runs instead of forcing one long run to serve
   both purposes

The distributed-PC phase follows the same separation rule in:

- `docs/operations/distributed-pc-validation.md`

## Not In Scope Yet

- OBS WebSocket / advanced OBS control
- distributed-PC actual run before the planning doc is applied
- persistent decoder context work
- retry/backoff manager work
- generic N-view refactor
- protocol or architecture changes
