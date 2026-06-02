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
- `StreamSync 4-view Output` remains human-facing Preview / monitoring only
- do not use `StreamSync 4-view Output` for production Program capture
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
- New fields to watch on the next Program OBS rerun:
  - `program_first_validation_enabled`
  - `preview_compose_skipped_for_program_count`
  - `preview_compose_reused_for_program_count`
  - `program_render_loop_attempt_count`
  - `program_window_render_success_count`
  - `program_window_render_failure_count`
  - `program_render_effective_fps`
  - `effective_program_render_fps`
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
