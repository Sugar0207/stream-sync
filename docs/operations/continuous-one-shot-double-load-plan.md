<!-- stream-sync/docs/operations/continuous-one-shot-double-load-plan.md -->

# Continuous One-Shot Double-Load Plan

Last updated: 2026-05-22

## Purpose
- Design the next docs-first opt-in experiment after throughput diagnostics became runtime-valid.
- Isolate whether slot0 continuous output throughput moves toward 28fps-class source cadence when slot0 one-shot fallback load is suppressed while the slot0 continuous runtime is already running.
- Keep the experiment narrower than a fallback policy change: default behavior remains unchanged and Production Readiness remains FAIL.

## Evidence Gate
- latest valid rerun:
  - suppression ON `S:\stream-sync\manual-logs\two-client-render-rerun-20260522-082451`
- prior valid OFF baseline:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260522-075029`
- build / runtime validity:
  - build PASS from `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
  - low-latency args active
  - suppression diagnostics present in summary
- separated result:
  - feed PASS
  - continuous output PASS
  - continuous render consumption PARTIAL PASS
- suppression ON:
  - `continuous_decode_slot0_one_shot_suppression_enabled=true`
  - `continuous_decode_slot0_one_shot_suppressed_count=216`
  - render safety stayed on `decode_deferred_placeholder:216|unknown:0`
- throughput / source cadence:
  - continuous output `22.327fps`
  - client output `22.340fps` / `22.453fps`
- suppression ON double-load signal:
  - `continuous_decode_competing_one_shot_attempt_count=12`
  - `continuous_decode_competing_one_shot_decode_elapsed_ms=1414`

## Experiment Question
- When slot0 continuous runtime is enabled and running, does removing slot0 one-shot fallback work from that same preview loop materially improve:
  - `continuous_decode_output_throughput_fps`
  - reader full-frame latency
  - output lag to selected frames
- This asks whether double-load is a meaningful contributor. It does not claim double-load is the only FPS cause.
- Suppression ON evidence is valid, but the first ON rerun is not a matched causal
  comparison because client output fps fell from the OFF baseline `28fps` class
  to the ON run `22fps` class.

## Flag Shape
First implementation flag:

- `--continuous-decoder-slot0-suppress-one-shot-fallback`

Decision:

- Use the explicit suppression name because the first slice only skips slot0 one-shot fallback after the slot0 continuous runtime is running.
- Keep it an isolation experiment through scope, diagnostics, and docs. The default path does not change.

## Scope
- slot0 only
- two-real preview loop only
- opt-in continuous decoder only
- active only while slot0 continuous runtime is enabled and running
- slot1 stays on current one-shot behavior
- default path stays unchanged

## Behavior Boundary
- Current safe default remains:
  - exact continuous lookup
  - guarded bounded-lag lookup
  - one-shot fallback on miss/reject
- Experiment candidate:
  - when slot0 continuous runtime is enabled/running and the isolation flag is set, do not launch slot0 one-shot fallback for that experiment path
  - do not suppress slot1 one-shot work
  - do not remove fallback globally

First-slice render safety:

- Return the existing H.264 decode-deferred path with `ContinuousOneShotSuppressed`.
- That path becomes the existing decode-deferred placeholder instead of one-shot decoded output.
- Do not hold an unlimited stale continuous decoded frame and do not invent a new production display fallback policy.

Held alternatives:

- previous slot0 frame hold
- no-updated-frame tick behavior

## Diagnostics
Experiment-state fields to add if code proceeds:

- `continuous_decode_slot0_one_shot_suppression_enabled`
- `continuous_decode_slot0_one_shot_suppressed_count`
- `continuous_decode_slot0_one_shot_suppressed_reason_counts`
- `continuous_decode_slot0_one_shot_suppressed_render_safety_counts`
- `continuous_decode_slot0_one_shot_suppressed_continuous_not_ready_count`
- `continuous_decode_slot0_one_shot_suppressed_stale_count`

Existing fields to compare:

- `continuous_decode_output_throughput_fps`
- `continuous_decode_reader_full_frame_elapsed_ms_avg`
- `continuous_decode_output_lag_to_selected_frames`
- `render_used_continuous_decoded_count`
- `render_used_one_shot_fallback_count`
- `effective_render_fps_after_first_render`

Useful supporting readback:

- `continuous_decode_competing_one_shot_attempt_count`
- `continuous_decode_competing_one_shot_decode_elapsed_ms`
- `continuous_decode_output_frame_interval_ms_avg`
- `continuous_decode_output_frame_interval_ms_max`
- `continuous_decode_reader_full_frame_elapsed_ms_max`

## Acceptance For A Future Opt-In Code Slice
- Flag-off behavior remains unchanged.
- Flag-on summary proves slot0 suppression/isolation is active and counted.
- Slot1 one-shot decode remains available.
- Continuous decoder output throughput can be compared against the valid `20260522-075029` baseline.
- Render consumption staying FAIL is allowed evidence; the experiment is for throughput isolation first.
- Production Readiness remains FAIL.

## First Implementation Status
- 2026-05-22 first code slice is implemented for slot0 / two-real / opt-in continuous only.
- Parser wiring accepts the new CLI flag with default `false`.
- Suppression becomes effective only when the flag is on and the slot0 continuous process is running.
- Exact and bounded-lag continuous lookup still run first.
- If lookup still misses or rejects while suppression is active, slot0 returns `ContinuousOneShotSuppressed` into the existing decode-deferred placeholder path instead of launching slot0 one-shot fallback.
- Slot1 one-shot behavior remains unchanged.
- Summary now exposes:
  - `continuous_decode_slot0_one_shot_suppression_enabled`
  - `continuous_decode_slot0_one_shot_suppressed_count`
  - `continuous_decode_slot0_one_shot_suppressed_reason_counts`
  - `continuous_decode_slot0_one_shot_suppressed_render_safety_counts`
  - `continuous_decode_slot0_one_shot_suppressed_continuous_not_ready_count`
  - `continuous_decode_slot0_one_shot_suppressed_stale_count`
- The existing competing one-shot counters stay visible for on/off comparison.
- Next human rerun keeps the base suffix and adds:
  - `--continuous-decoder-slot0-suppress-one-shot-fallback`

## Suppression ON Runtime Result
- latest ON rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260522-082451`
- VALID / PASS:
  - binary path remains `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
  - low-latency args remain active
  - suppression flag is active
  - feed, continuous output, and suppression diagnostics wiring are PASS
- one-shot suppression effect: PARTIAL PASS
  - `continuous_decode_slot0_one_shot_suppressed_count=216`
  - `continuous_decode_slot0_one_shot_suppressed_reason_counts=continuous_not_ready:51|stale:165|future:0|unknown:0`
  - `continuous_decode_slot0_one_shot_suppressed_render_safety_counts=decode_deferred_placeholder:216|unknown:0`
  - competing one-shot fell from OFF baseline `34` attempts / `3515ms`
    to ON `12` attempts / `1414ms`
- continuous render consumption: PARTIAL PASS
  - `render_used_continuous_decoded_count=3`
  - `continuous_decode_bounded_lookup_hit_count=3`
  - `continuous_decode_render_used_bounded_lag_count=3`
- throughput causality: INCONCLUSIVE
  - OFF baseline clients were `28.358fps` / `28.501fps`
  - ON clients were `22.340fps` / `22.453fps`
  - output lag to selected moved from OFF `73` to ON `17`, but the input
    cadence changed enough that this is not yet suppression-only evidence

## Matched A/B Rerun
- Use the same build and same target exe:
  - `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
- OFF suffix:
  - `--disable-persistent-decoder --enable-continuous-stream-decoder --continuous-decoder-low-latency-args`
- ON suffix:
  - `--disable-persistent-decoder --enable-continuous-stream-decoder --continuous-decoder-low-latency-args --continuous-decoder-slot0-suppress-one-shot-fallback`
- Compare:
  - client `effective_output_fps`
  - `continuous_decode_output_throughput_fps`
  - `continuous_decode_output_lag_to_selected_frames`
  - `continuous_decode_latest_input_minus_latest_output_lag`
  - `continuous_decode_competing_one_shot_attempt_count`
  - `continuous_decode_competing_one_shot_decode_elapsed_ms`
  - `render_used_continuous_decoded_count`
  - `continuous_decode_bounded_lookup_hit_count`
  - `effective_render_fps_after_first_render`
- Mark the comparison noisy if client fps differs by more than roughly `2fps`.

## Held
- allowed lag threshold changes
- targetTime-aware decoded queue lookup
- latest decoded fallback
- feed max count changes
- continuous output pixel format changes
- FFmpeg scale-path changes
- low-latency args default changes
- slot1 continuous rollout
- 4-client rollout
- request/response persistent decoder revival
- GPU decode
- one-shot fallback removal
