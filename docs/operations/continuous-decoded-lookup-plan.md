<!-- stream-sync/docs/operations/continuous-decoded-lookup-plan.md -->

# Continuous Decoded Lookup Plan

最終更新: 2026-05-27

## 目的
- bounded feed helper PASS 後も render consumption が `0` のままなので、slot0 continuous decoded queue の参照方針を docs-first で整理する
- exact selected-frame lookup を最優先に残しつつ、decoded output が requested frame から bounded lag 内にある場合だけ continuous decoded frame を render に使う候補を設計する
- unbounded latest decoded fallback は stale frame 表示リスクがあるため採用しない
- one-shot fallback は安全弁として維持する

## latest evidence
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-lag-reverse-ab-rerun-20260527-164258`
- validity:
  - both runs used `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
  - client FPS stayed within 2fps, so the comparison is VALID
- lag8:
  - `continuous_decode_bounded_lookup_allowed_lag_frames=8`
  - `continuous_decode_bounded_lookup_hit_count=2`
  - `continuous_decode_bounded_lookup_lag_frames=7`
  - `continuous_decode_bounded_lookup_rejected_stale_count=221`
  - `continuous_decode_bounded_lookup_rejected_not_ready_count=25`
  - `render_used_continuous_decoded_count=2`
  - `effective_render_fps_after_first_render=12.159`
  - `placeholder_visual_changed_count=80`
  - `continuous_decode_output_lag_to_selected_frames=89`
  - `continuous_decode_output_throughput_fps=19.635`
  - `continuous_decode_reader_full_frame_elapsed_ms_avg=50.000`
- lag5:
  - `continuous_decode_bounded_lookup_allowed_lag_frames=5`
  - `continuous_decode_bounded_lookup_hit_count=1`
  - `continuous_decode_bounded_lookup_lag_frames=5`
  - `continuous_decode_bounded_lookup_rejected_stale_count=238`
  - `continuous_decode_bounded_lookup_rejected_not_ready_count=22`
  - `render_used_continuous_decoded_count=1`
  - `effective_render_fps_after_first_render=12.342`
  - `placeholder_visual_changed_count=81`
  - `continuous_decode_output_lag_to_selected_frames=120`
  - `continuous_decode_output_throughput_fps=17.189`
  - `continuous_decode_reader_full_frame_elapsed_ms_avg=57.473`
- interpretation:
  - lag8 is a small PARTIAL PASS and a stronger adoption candidate than lag5
  - lag8 improves bounded lookup hit rate, stale reject count, output lag, throughput, and reader average latency
  - lag5 keeps a slightly higher render FPS and slightly fewer not-ready rejects
  - default `8` promotion is HOLD
  - default `5` remains the current guard

## current exact lookup problem
- current render consumption requires exact selected-frame cache key match:
  - same output profile
  - same `client_id`
  - same `run_id`
  - same `frame_id`
- bounded feed helper now feeds many access units before render-demand fallback, but decoded output still trails requested frame ids
- when render requests `frame_id=459` and decoded queue newest is `426`, exact lookup cannot hit even though decoded frames exist
- exact lookup protects sync correctness, but by itself it is too strict while decoded queue has bounded lag
- latest decoded fallback without guard would display old frames and can undermine the sync-first goal

## bounded-lag decoded queue lookup goal
- Use continuous decoded output only when it is close enough to the requested selected frame
- Keep display-side targetTime and stale policy in switcher; decoder runtime does not decide targetTime
- Prefer correctness over FPS:
  - exact frame hit is best
  - bounded-lag hit is acceptable only under explicit guards
  - otherwise one-shot fallback remains safer
- Do not make the first lookup policy a general latest-frame fallback

## targetTime-aware vs frame_id-nearest lookup
targetTime-aware lookup:

- Selects decoded candidates by decoded frame capture timestamp relative to the current targetTime
- Must not choose a decoded frame whose capture timestamp is after targetTime
- Better aligned with the sync model because display eligibility is time-based
- Requires decoded queue entries to retain capture timestamp and source identity

frame_id-nearest lookup:

- Selects the nearest decoded frame id at or before the requested selected frame id
- Simpler and fits current diagnostics such as requested/latest frame id lag
- Assumes frame_id ordering follows capture/display order for the source
- Can be used as a conservative first implementation if it also enforces targetTime no-future guard when timestamps are available

First design preference:

- exact `frame_id` match first
- then targetTime-aware candidate search when capture timestamp is available
- use frame_id-nearest as the tie-breaker / fallback ranking among candidates that are not after targetTime
- reject candidates that exceed allowed lag even if they are the latest decoded frame

## allowed lag threshold candidates
- Implemented first slice:
  - `allowed_lag_frames=5`
  - keep this as the current default guard until a future policy review explicitly promotes a wider threshold
  - it already produced ON bounded-lookup hits at lag `5`
  - it keeps the current display guard close to sync-first behavior while stale
    rejects remain high
- Benefits of staying at default `5`:
  - limits visibly old slot0 continuous frames while frame-id lag still varies
  - keeps the first bounded-lookup evidence distinct from suppression evidence
  - makes stale reject count a useful signal instead of silently widening display
    eligibility
- Costs of staying at default `5`:
  - suppression ON still rejected stale candidates `228` times in the matched A/B
  - output lag to selected improved to `8`, so some near-threshold candidates may
    remain unusable even when output throughput improves
  - render may keep using the suppression safety placeholder when a bounded but
    slightly older decoded frame could be available
- Fixed opt-in experiment candidates:
  - `allowed_lag_frames=8`
  - first candidate because it is only slightly above the matched ON output lag
    signal and remains below one third of a second at 30fps
  - `allowed_lag_frames=10`
  - second candidate if `8` remains too narrow; risk is higher stale display
    tolerance and comparison noise from letting more old frames through
- latest reverse-order threshold A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-lag-reverse-ab-rerun-20260527-164258`
  - comparison is VALID
  - lag `8` is a small PARTIAL PASS and a held adoption candidate
  - lag `5` keeps a tiny render-FPS edge and slightly fewer not-ready rejects
  - default `8` promotion is HOLD; keep default `5` unchanged for now
- First threshold experiment code slice:
  - `--continuous-decoder-bounded-lookup-allowed-lag-frames <N>` is now wired
    into the two-real continuous slot0 bounded lookup threshold
  - omitted flag keeps the default `5`
  - CLI values above the decoded queue bound are rejected
  - first human comparison value is explicit `8`; `10` stays a later candidate
    only if `8` is still narrow after same-build evidence
  - summary keeps reporting the effective
    `continuous_decode_bounded_lookup_allowed_lag_frames` and accepted
    `continuous_decode_bounded_lookup_lag_frames`
- Dynamic threshold candidate:
  - docs-only hold for now
  - it would need an explicit bound, a clear input signal, and summary visibility
    for the chosen threshold on every run
  - do not derive it silently from queue length, last hit lag, or render FPS
- Queue-bound ceiling:
  - never allow more than the decoded queue bound, currently `30`
  - `30` should be a debug ceiling, not the first display default
- Runtime evidence from latest rerun:
  - current lag around `40` should be rejected as stale by the first policy
  - this is important because accepting lag `40` would risk visibly old video
- Latest bounded-lookup rerun update:
  - current lag reached `88` and max lag reached `163`
  - threshold expansion is held because the risk is stale frame display, not just a narrow guard value

## stale frame guard
- Reject decoded candidates when:
  - source identity does not match requested `client_id + run_id`
  - candidate capture timestamp is after targetTime
  - candidate frame id is after requested frame id
  - requested frame id minus candidate frame id exceeds `allowed_lag_frames`
  - candidate is older than a future display-policy max staleness window
  - output/correspondence state suggests frame_id mismatch risk
- Rejected stale candidates should not prevent one-shot fallback
- Rejection should increment explicit diagnostics rather than being hidden as a generic miss
- Threshold experiments must keep these guards:
  - never use a decoded frame after the requested frame id
  - never turn lookup miss into unbounded latest decoded fallback
  - keep future-frame rejection explicit
  - keep the selected source identity check
  - keep placeholder / previous display safety decisions separate from the lookup
    threshold itself
- If the experiment threshold is wider than `5`, summary output must make the
  actual allowed lag and accepted hit lag obvious before comparing render FPS.

## lookup priority
1. Exact selected-frame lookup:
   - if exact decoded cache key exists, use it
   - increment existing exact hit diagnostics
2. Bounded-lag decoded queue lookup:
   - same source only
   - candidate at or before targetTime
   - candidate at or before requested frame id
   - lag within threshold
   - choose nearest candidate to targetTime / requested frame
3. One-shot fallback:
   - unchanged safety path
   - used when exact and bounded-lag lookup both miss or are rejected

## startup / output_pending handling
- Before any decoded output exists:
  - do not use bounded lookup
  - classify as not-ready
  - keep one-shot fallback
- While `continuous_decode_output_pending_correspondence_count > 0`:
  - bounded lookup may use already decoded queued frames only if they pass staleness guards
  - do not block waiting for pending output
- If decoded queue is empty:
  - classify as not-ready
  - keep one-shot fallback
- If decoded queue has frames but all are too old:
  - classify as rejected stale
  - keep one-shot fallback

## scheduler_status=HandoffError relation
- `scheduler_status=HandoffError` is a handoff/source aggregate status, not a decoded lookup result
- Bounded lookup must not hide source errors:
  - if the selected source result is `HandoffError`, preserve the existing source-error path
  - do not show a stale decoded frame just because a handoff read failed
- A bounded decoded candidate may only be considered when the render path has a selected encoded frame identity to compare against
- If the current tick has no selected frame because of source error, the first implementation should leave existing display policy / one-shot behavior unchanged

## diagnostics
Add first-slice summary fields:

- `continuous_decode_bounded_lookup_enabled`
- `continuous_decode_bounded_lookup_allowed_lag_frames`
- `continuous_decode_bounded_lookup_hit_count`
- `continuous_decode_bounded_lookup_used_frame_id`
- `continuous_decode_bounded_lookup_requested_frame_id`
- `continuous_decode_bounded_lookup_lag_frames`
- `continuous_decode_bounded_lookup_rejected_stale_count`
- `continuous_decode_bounded_lookup_rejected_future_count`
- `continuous_decode_bounded_lookup_rejected_not_ready_count`
- `continuous_decode_bounded_lookup_fallback_to_one_shot_count`
- `continuous_decode_render_used_exact_count`
- `continuous_decode_render_used_bounded_lag_count`

Optional later diagnostics:

- `continuous_decode_bounded_lookup_candidate_count`
- `continuous_decode_bounded_lookup_candidate_oldest_frame_id`
- `continuous_decode_bounded_lookup_candidate_newest_frame_id`
- `continuous_decode_bounded_lookup_rejected_source_mismatch_count`

## first implementation slice
- slot0 only
- two-real preview loop only
- opt-in continuous only
- exact lookup first
- bounded-lag lookup second
- one-shot fallback third
- no slot1 rollout
- no 4-client rollout
- no server / client / protocol change
- no feed max count change
- no request/response persistent decoder revival
- no GPU decode
- no Production Readiness PASS

## acceptance for first code slice
- summary proves bounded lookup was enabled
- exact hit count remains separately visible
- bounded lookup hit count is visible even if it remains `0` in first rerun
- stale rejection and not-ready rejection are visible
- one-shot fallback remains visible and functional
- no stale frame is accepted when lag exceeds the first threshold
- Production Readiness remains FAIL until real render consumption and sync safety are proven

## first implementation status
2026-05-20 first code slice implemented:

- slot0 only
- two-real preview loop only
- opt-in continuous only
- exact selected-frame lookup remains first
- bounded-lag frame_id-nearest lookup runs only after exact lookup misses
- one-shot fallback remains third
- allowed lag is a fixed safety-first `5` frames
- requested frame_id より未来の decoded frame は使わない
- lag が `5` frames を超える decoded frame は stale として拒否する
- startup / queue empty / no usable decoded frame は not-ready として one-shot fallback に進む

Added diagnostics:

- `continuous_decode_bounded_lookup_enabled`
- `continuous_decode_bounded_lookup_allowed_lag_frames`
- `continuous_decode_bounded_lookup_hit_count`
- `continuous_decode_bounded_lookup_used_frame_id`
- `continuous_decode_bounded_lookup_requested_frame_id`
- `continuous_decode_bounded_lookup_lag_frames`
- `continuous_decode_bounded_lookup_rejected_stale_count`
- `continuous_decode_bounded_lookup_rejected_future_count`
- `continuous_decode_bounded_lookup_rejected_not_ready_count`
- `continuous_decode_bounded_lookup_fallback_to_one_shot_count`
- `continuous_decode_render_used_exact_count`
- `continuous_decode_render_used_bounded_lag_count`

Still not implemented:

- targetTime-aware decoded queue lookup 本格実装
- CLI-configurable lag threshold
- slot1 continuous
- 4-client continuous
- server / client / protocol changes
- unbounded latest decoded fallback
- one-shot fallback removal

Runtime guidance:

- Codex did not run a manual rerun
- next human rerun should be from `S:\stream-sync`
- keep:
  - `--disable-persistent-decoder --enable-continuous-stream-decoder --continuous-decoder-low-latency-args`
- first read:
  - `continuous_decode_bounded_lookup_hit_count`
  - `continuous_decode_bounded_lookup_lag_frames`
  - `continuous_decode_bounded_lookup_rejected_stale_count`
  - `continuous_decode_bounded_lookup_rejected_future_count`
  - `continuous_decode_bounded_lookup_rejected_not_ready_count`
  - `continuous_decode_render_used_bounded_lag_count`
  - `render_used_continuous_decoded_count`

## first bounded-lag runtime evidence
latest rerun:

- `S:\stream-sync\manual-logs\two-client-render-rerun-20260520-005310`

Wiring PASS:

- `continuous_decode_config_enabled=true`
- `continuous_decode_runtime_enabled=true`
- `continuous_decode_slot0_enabled=true`
- `continuous_decode_ffmpeg_low_latency_args_enabled=true`
- `continuous_decode_bounded_lookup_enabled=true`
- `continuous_decode_bounded_lookup_allowed_lag_frames=5`

Feed helper PASS:

- `continuous_feed_enabled=true`
- `continuous_feed_attempt_count=300`
- `continuous_feed_frame_received_count=369`
- `continuous_feed_enqueued_count=361`
- `continuous_feed_skipped_count=9`
- `continuous_feed_skip_reason_counts=duplicate:8|future_frame:0|runtime_disabled:0|startup_not_ready:0|input_queue_full:0|source_mismatch:0|consume_mismatch:1|unknown:0`
- `continuous_decode_input_from_feeder_count=361`
- `continuous_decode_input_from_render_demand_count=17`
- `continuous_decode_feeder_lag_to_selected=0`

Bounded-lag render consumption FAIL:

- `continuous_decode_bounded_lookup_hit_count=0`
- `continuous_decode_bounded_lookup_rejected_stale_count=17`
- `continuous_decode_bounded_lookup_rejected_not_ready_count=2`
- `continuous_decode_bounded_lookup_fallback_to_one_shot_count=19`
- `continuous_decode_render_used_exact_count=0`
- `continuous_decode_render_used_bounded_lag_count=0`
- `render_used_continuous_decoded_count=0`
- `render_used_one_shot_fallback_count=19`

Output / lag evidence:

- `continuous_decode_input_frame_count=378`
- `continuous_decode_output_frame_count=297`
- `continuous_decode_queue_len=30`
- `continuous_decode_dropped_stale_count=267`
- `continuous_decode_requested_frame_id=627`
- `continuous_decode_latest_decoded_frame_id=551`
- `continuous_decode_requested_minus_latest_lag=88`
- `continuous_decode_frame_id_lag=163`
- `continuous_decode_output_pending_correspondence_count=79`
- `continuous_decode_stdout_read_elapsed_ms=20840`
- `continuous_decode_stdout_reader_blocked_count=17`

Interpretation:

- The bounded-lag diagnostics appear in the summary, so the first implementation wiring is valid.
- The lookup policy is behaving safely: it rejects stale candidates instead of displaying frames far behind the requested selection.
- The `5` frame threshold is not the only problem in this rerun. The latest decoded frame is `88` frames behind the requested frame, and max lag reached `163`; accepting that by widening the threshold would create stale frame display risk.
- Next work should not be threshold tuning. The next docs-first design target is continuous decoder output lag, output pending correspondence backlog, stdout read latency / throughput, and decoded queue/drop policy.

Next diagnostics candidates:

- `continuous_decode_output_latency_frames_avg`
- `continuous_decode_output_latency_frames_max`
- `continuous_decode_input_to_output_lag_frames_avg`
- `continuous_decode_input_to_output_lag_frames_max`
- `continuous_decode_correspondence_pending_age_ms`
- `continuous_decode_queue_drop_reason_counts`
- `continuous_decode_output_lag_to_selected_frames`
- `continuous_decode_reader_full_frame_elapsed_ms_max`
- `continuous_decode_output_throughput_fps`

Follow-up:

- Detailed output lag / pending correspondence analysis now lives in `docs/operations/continuous-output-lag-plan.md`.
- Detailed output throughput / stdout full-frame read / raw BGRA path / one-shot double-load analysis now lives in `docs/operations/continuous-output-throughput-plan.md`.
- Do not widen `allowed_lag_frames=5` until the output lag plan has runtime diagnostics for pending correspondence frame_id range, latest input/output lag, and output throughput.
- Bounded lookup remains a guarded render-consumption policy; it is not expected to fix decoder output lag by itself.

## output lag diagnostics rerun update
latest rerun:

- `S:\stream-sync\manual-logs\two-client-render-rerun-20260520-014041`

Wiring / output PASS:

- `continuous_decode_config_enabled=true`
- `continuous_decode_runtime_enabled=true`
- `continuous_decode_slot0_enabled=true`
- `continuous_decode_ffmpeg_low_latency_args_enabled=true`
- `continuous_feed_enabled=true`
- `continuous_decode_bounded_lookup_allowed_lag_frames=5`
- `continuous_decode_input_from_feeder_count=412`
- `continuous_decode_input_from_render_demand_count=5`
- `continuous_decode_output_frame_count=367`

Lookup adoption / render consumption FAIL:

- `continuous_decode_bounded_lookup_hit_count=0`
- `continuous_decode_bounded_lookup_rejected_stale_count=13`
- `continuous_decode_bounded_lookup_rejected_not_ready_count=2`
- `continuous_decode_bounded_lookup_fallback_to_one_shot_count=15`
- `render_used_continuous_decoded_count=0`
- `render_used_one_shot_fallback_count=15`

Output lag evidence:

- `continuous_decode_requested_frame_id=446`
- `continuous_decode_latest_decoded_frame_id=401`
- `continuous_decode_requested_minus_latest_lag=64`
- `continuous_decode_frame_id_lag=64`
- `continuous_decode_latest_input_minus_latest_output_lag=78`
- `continuous_decode_output_lag_to_selected_frames=64`
- `continuous_decode_output_pending_correspondence_count=48`
- `continuous_decode_output_throughput_fps=23.309`
- `continuous_decode_reader_full_frame_elapsed_ms_max=1305`

Interpretation:

- Bounded lookup remains safety-correct: it rejects stale/not-ready frames instead of displaying continuous output that is far behind requested selection.
- `allowed_lag_frames=5` should not be widened as the next step. The observed lag is much larger than a safe sync-first guard.
- TargetTime-aware decoded queue lookup and latest decoded fallback stay held until continuous output is close enough to selected/source cadence.
- Next work should be docs-first analysis of continuous decoder output throughput, stdout full-frame read latency, raw BGRA output path cost, and one-shot fallback double-load.
- That docs-first analysis is now split to `docs/operations/continuous-output-throughput-plan.md`; the lookup plan remains a policy guard, not the first root-cause fix for `23.309fps` output throughput.

## throughput diagnostics validity update
latest rerun:

- matched suppression OFF/ON evidence:
  - `S:\stream-sync\manual-logs\two-client-ab-rerun-20260522-103943`

Interpretation:

- Same-build OFF/ON comparison is VALID寄り and source fps mismatch is not noisy enough to reject the read.
- OFF no suppression had throughput `20.129fps`, competing one-shot `37` attempts / `5401ms`, render continuous use `0`, and bounded lookup hits `0`.
- ON slot0 suppression had throughput `26.814fps`, competing one-shot `13` attempts / `942ms`, render continuous use `11`, and bounded lookup hits `11`.
- ON suppression reasons still show stale `228` and continuous-not-ready `27`.
- One-shot double-load is now a strong contributor candidate, but suppression stays opt-in evidence rather than the default policy.
- The bounded lookup allowed-lag threshold / policy review has now produced a
  held lag8 candidate, not a default promotion. The next docs-first gate moves
  to output availability / throughput; targetTime-aware lookup and latest
  decoded fallback remain held.

## matched A/B threshold policy review
Evidence premise:

- matched suppression OFF/ON rerun:
  - `S:\stream-sync\manual-logs\two-client-ab-rerun-20260522-103943`
- OFF:
  - `continuous_decode_output_lag_to_selected_frames=17`
  - `continuous_decode_latest_input_minus_latest_output_lag=20`
  - `continuous_decode_bounded_lookup_hit_count=0`
- ON:
  - `continuous_decode_output_lag_to_selected_frames=8`
  - `continuous_decode_latest_input_minus_latest_output_lag=33`
  - `continuous_decode_bounded_lookup_hit_count=11`
  - `continuous_decode_slot0_one_shot_suppressed_reason_counts=continuous_not_ready:27|stale:228|future:0|unknown:0`

Policy reading:

- The current fixed `5` guard is not useless: ON evidence has bounded hits at the
  current limit and keeps future/unbounded fallback out of render use.
- The same ON evidence leaves many stale and not-ready rejects. Threshold review
  is justified, but stale count alone does not prove that every rejected frame is
  safe to display.
- `not_ready` is not solved by widening lag. It can mean output/cache availability
  is still missing at lookup time.
- A wider threshold can trade fewer placeholders for more visible lag. That is an
  experiment question, not a default policy change.

Opt-in experiment shape:

- implemented first flag:
  - `--continuous-decoder-bounded-lookup-allowed-lag-frames <N>`
- first values to compare:
  - default-equivalent `5`
  - fixed `8`
  - fixed `10` only if `8` remains informative but too narrow
- first scope:
  - slot0 only
  - two-real preview loop only
  - opt-in continuous only
  - same-build rerun from `S:\stream-sync`
- recommended isolation boundary:
  - run the threshold comparison with
    `--continuous-decoder-slot0-suppress-one-shot-fallback` ON so the matched
    A/B double-load result is not reopened while threshold policy is measured
  - keep the threshold flag explicit even if it is accepted only under that
    suppression experiment scope in the first code slice

Diagnostics to compare:

- `continuous_decode_bounded_lookup_allowed_lag_frames`
- `continuous_decode_bounded_lookup_hit_count`
- `continuous_decode_bounded_lookup_lag_frames`
- `continuous_decode_bounded_lookup_rejected_stale_count`
- `continuous_decode_bounded_lookup_rejected_not_ready_count`
- `render_used_continuous_decoded_count`
- `effective_render_fps_after_first_render`
- `placeholder_visual_changed_count`
- `continuous_decode_slot0_one_shot_suppressed_count`

Comparison rules:

- compare threshold `5` vs `8` or `10` on the same build
- keep suppression ON for the first threshold comparison
- treat large client FPS differences as noisy before attributing threshold effects
- read accepted bounded-hit lag together with stale reject count; a hit increase is
  not a success if it hides an unsafe stale-frame display policy

Reverse-order threshold verdict:

- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-lag-reverse-ab-rerun-20260527-164258`
- lag8 is a small PARTIAL PASS and remains an adoption candidate.
- Default `8` promotion is HOLD; default `5` remains the current guard.
- The branch should be kept as candidate evidence, not expanded into dynamic
  thresholding, targetTime-aware lookup, or latest-decoded fallback yet.
- The next main line moves to output availability / throughput because allowed
  lag alone does not solve not-ready rejects, pending correspondence, stdout
  reader latency, or continuous output throughput.
- Candidate comparison for that next line now lives in
  `docs/operations/continuous-output-availability-plan.md`.

Held:

- default `5` threshold change
- dynamic allowed-lag implementation
- targetTime-aware decoded queue lookup
- unbounded latest decoded fallback
- suppression defaulting
- slot1 / 4-client widening
- Production Readiness PASS
