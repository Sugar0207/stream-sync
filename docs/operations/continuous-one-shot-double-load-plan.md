<!-- stream-sync/docs/operations/continuous-one-shot-double-load-plan.md -->

# Continuous One-Shot Double-Load Plan

Last updated: 2026-05-22

## Purpose
- Design the next docs-first opt-in experiment after throughput diagnostics became runtime-valid.
- Isolate whether slot0 continuous output throughput moves toward 28fps-class source cadence when slot0 one-shot fallback load is suppressed while the slot0 continuous runtime is already running.
- Keep the experiment narrower than a fallback policy change: default behavior remains unchanged and Production Readiness remains FAIL.

## Evidence Gate
- latest valid rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260522-075029`
- build / runtime validity:
  - build PASS from `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
  - low-latency args active
  - throughput diagnostics present in summary
- separated result:
  - feed PASS
  - continuous output PASS
  - continuous render consumption FAIL
- throughput gap:
  - continuous output `21.773fps`
  - client output `28.358fps` / `28.501fps`
- double-load signal:
  - `continuous_decode_competing_one_shot_attempt_count=34`
  - `continuous_decode_competing_one_shot_decode_elapsed_ms=3515`
  - one-shot totals match at `34` attempts / `3515ms`

## Experiment Question
- When slot0 continuous runtime is enabled and running, does removing slot0 one-shot fallback work from that same preview loop materially improve:
  - `continuous_decode_output_throughput_fps`
  - reader full-frame latency
  - output lag to selected frames
- This asks whether double-load is a meaningful contributor. It does not claim double-load is the only FPS cause.

## Flag Shape
Preferred flag:

- `--continuous-decoder-slot0-output-throughput-isolation`

Alternate explicit flag:

- `--continuous-decoder-slot0-suppress-one-shot-fallback`

Design preference:

- Prefer the isolation name if the implementation is intentionally diagnostic and may choose a render-safety response richer than a bare fallback suppression toggle.
- Prefer the suppression name only if the behavior is exactly scoped to skipping slot0 one-shot fallback and the summary makes that experiment state unmistakable.

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

Render-safety options to compare before implementation:

1. Hold previous slot0 frame
   - Closest to preserving visible continuity when a prior frame exists.
   - Can make staleness visible for longer, so summary must keep miss/reject reasons.
2. Placeholder
   - Makes missing continuous output obvious.
   - Strong diagnostic isolation, but visually harsher.
3. No updated slot0 frame for the tick
   - Avoids fabricating a replacement decode.
   - Must preserve existing composition/render invariants before use.

The docs-first recommendation is to select the smallest option already compatible with the two-real preview loop display policy. Do not invent a production fallback policy inside this experiment.

## Diagnostics
Experiment-state fields to add if code proceeds:

- `continuous_decode_slot0_one_shot_suppressed_count`
- `continuous_decode_slot0_one_shot_suppressed_reason_counts`

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
