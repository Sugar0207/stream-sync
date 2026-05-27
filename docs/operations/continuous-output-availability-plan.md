<!-- stream-sync/docs/operations/continuous-output-availability-plan.md -->

# Continuous Output Availability Plan

Last updated: 2026-05-27

## Purpose
- Move the next continuous-stream decoder investigation from bounded lookup
  threshold tuning back to output availability / throughput.
- Keep threshold lag8 as a held adoption candidate, not a default change.
- Compare the next safe code candidates before changing FFmpeg output shape,
  lookup policy, suppression defaults, feed pressure, slot1, 4-client, or GPU.
- Keep Production Readiness as FAIL.

## Current Verdict
- latest reverse-order lag threshold A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-lag-reverse-ab-rerun-20260527-164258`
  - lag8 is a small PARTIAL PASS versus lag5
  - lag8 improved bounded lookup hits, stale rejects, output lag, throughput,
    and reader average latency
  - lag5 kept a tiny render-FPS edge and slightly fewer not-ready rejects
  - default `8` promotion is HOLD; default `5` remains the guard
- This means the threshold branch remains useful evidence, but it did not
  eliminate:
  - large continuous output lag
  - pending correspondence
  - stdout reader full-frame latency
  - output throughput below source cadence in many runs
  - not-ready rejects
  - low render continuous use

## Safety Constraints
- No stale unrestricted fallback.
- No decoded frame newer than the selected / target frame.
- Keep same-source guard.
- Keep one-shot fallback.
- Keep default allowed lag, suppression default, feed max count, FFmpeg defaults,
  pixel format, and scale path unchanged until opt-in evidence exists.
- First code slice, if selected, must stay slot0 / two-real /
  `--enable-continuous-stream-decoder` only.

## Candidate Comparison

| Candidate | What It Answers | Why It Helps Now | Risk | Verdict |
| --- | --- | --- | --- | --- |
| Pending correspondence pressure diagnostics | Whether writer-accepted inputs are piling up before full stdout frames can be matched to metadata. | Not-ready rejects and pending correspondence remain even when lag is widened; this directly measures output availability pressure. | Low if diagnostics-only. | First safe code-slice candidate. |
| Raw BGRA pipe throughput / stdout reader buffering diagnostics | Whether `921600` byte full-frame reads, short reads, allocation, or reader scheduling dominate output latency. | lag8 still has reader avg `50.000ms`, throughput `19.635fps`, and selected-output lag `89`; reader availability is still slower than 30fps cadence. | Low for diagnostics; medium for buffering behavior changes. | Pair with pending-correspondence diagnostics first; defer behavior changes. |
| Continuous output queue/cache policy diagnostics | Whether decoded cache bound `30`, dropped stale count, or drain cadence hides usable decoded frames. | Cache drops are visible, but newest decoded output itself is still behind; diagnostics can confirm whether cache policy is a symptom or contributor. | Low if diagnostics-only. | Secondary diagnostics in the same or next slice. |
| FFmpeg continuous output scale path experiment | Separates FFmpeg scale cost from raw pipe / pixel conversion cost. | Current path is `-vf scale=640:360:flags=neighbor -f rawvideo -pix_fmt bgra pipe:1`; scale and BGRA conversion remain plausible contributors. | Medium: source-size raw BGRA can be much heavier, and moving scale responsibility may require renderer-side conversion/copy work. | Later opt-in experiment after availability diagnostics. |
| One-shot competing load | Measures continuous-vs-one-shot process contention. | Suppression ON already made this a strong contributor candidate and improved throughput/render use. | Medium if made default; low as already opt-in. | Demoted from next main culprit; keep as supporting evidence, not default policy. |

## Recommended Next Code Slice
- Name it as an output availability diagnostics slice, not a policy change.
- Scope:
  - slot0 only
  - two-real preview loop only
  - opt-in continuous enabled only
  - summary diagnostics only
  - no default behavior change
- Add or refine summary fields around:
  - output frame age relative to selected frame
  - decoded frame age / newest decoded age
  - selected-vs-decoded sequence distance
  - pending correspondence frame-id min/max and age min/max
  - reader full-frame phase counts: waiting for first byte, waiting for full
    frame, full-frame success, short/partial progress
  - stdout bytes/sec over the same interval as output fps
  - decoded cache bound drops vs reader output count vs render drain count
- Do not implement:
  - targetTime-aware lookup
  - latest decoded fallback
  - unbounded stale fallback
  - scale or pixel-format changes
  - feed max count changes
  - slot1 continuous
  - 4-client continuous

## Later Opt-In Experiments
1. Raw BGRA pipe / stdout reader buffering experiment
   - Only after diagnostics show reader-bound behavior.
   - Preserve full-frame correctness and do not emit partial frames.
   - Summary must expose the experiment flag and byte/read timing deltas.

2. FFmpeg scale path experiment
   - Keep source-size raw output as an explicit risk, not the default candidate.
   - Compare current scaled BGRA output against a narrow opt-in variant that
     separates scale cost from pipe/output cost.
   - Do not move scaling responsibility permanently without total pipeline cost
     evidence.

3. Queue/cache policy experiment
   - Only after diagnostics prove decoded cache/drop policy is hiding otherwise
     safe frames.
   - Any display decision still needs at-or-before selected/target frame and
     same-source checks.

## Readiness
- Threshold branch: HOLD / candidate.
- Output availability / throughput investigation: next main line.
- Production Readiness: FAIL.
