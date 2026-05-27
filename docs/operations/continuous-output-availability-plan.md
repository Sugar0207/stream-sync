<!-- stream-sync/docs/operations/continuous-output-availability-plan.md -->

# Continuous Output Availability Plan

Last updated: 2026-05-28

## Purpose
- Move the next continuous-stream decoder investigation from bounded lookup
  threshold tuning back to output availability / throughput.
- Keep threshold lag8 as a held adoption candidate, not a default change.
- Compare the next safe code candidates before changing FFmpeg output shape,
  lookup policy, suppression defaults, feed pressure, slot1, 4-client, or GPU.
- Keep Production Readiness as FAIL.

## Current Verdict
- latest output availability rerun:
  - `S:\stream-sync\manual-logs\two-client-output-availability-rerun-20260527-173716`
  - build and runtime evidence are VALID:
    - switcher binary:
      `C:\streamsync-target\stream-sync-rerun\debug\stream-sync-switcher.exe`
      LastWriteTime `2026/05/27 17:25:51`
    - client FFmpeg preflight/spawn errors are `none`
    - client1/client2 sent `900` frames each at `29.538fps` / `28.694fps`
    - server queued `1800` frames total:
      `player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`
  - continuous feed is PASS for the current slot0 / two-real / opt-in scope:
    - `continuous_feed_frame_received_count=453`
    - `continuous_feed_enqueued_count=423`
    - `continuous_decode_input_frame_count=431`
  - output availability diagnostics are VALID:
    - `continuous_decode_output_frame_count=316`
    - `continuous_decode_output_throughput_fps=21.269`
    - `continuous_decode_pending_correspondence_count=115`
    - `continuous_decode_pending_correspondence_age_ms_avg=1948.809`
    - `continuous_decode_latest_input_to_output_frame_gap=115`
    - `continuous_decode_output_lag_to_selected_frames=99`
    - reader full-frame average `46.430ms`, max `1125ms`, slow count `42`
  - interpretation:
    - not-ready is not the main issue in this rerun:
      `continuous_decode_output_availability_not_ready_count=22`
    - stale/output backlog is dominant:
      `continuous_decode_output_availability_stale_count=238`
    - continuous output throughput is about `21fps` while source is about
      `29fps`
    - pending correspondence count `115` and average age about `1.95s`
      indicate output pipeline backlog
    - threshold tuning alone is insufficient
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
| Pending correspondence pressure diagnostics | Whether writer-accepted inputs are piling up before full stdout frames can be matched to metadata. | Latest availability rerun shows pending correspondence `115`, avg age `1948.809ms`, and latest input-output gap `115`. | Low if diagnostics-only. | Implemented and runtime VALID. |
| Raw BGRA pipe throughput / stdout reader buffering diagnostics | Whether `921600` byte full-frame reads, short reads, allocation, or reader scheduling dominate output latency. | Latest availability rerun shows reader avg `46.430ms`, max `1125ms`, slow count `42`, and output throughput `21.269fps` while source is about `29fps`. | Low for diagnostics; medium for buffering behavior changes. | Next opt-in output pipeline experiment planning candidate. |
| Continuous output queue/cache policy diagnostics | Whether decoded cache bound `30`, dropped stale count, or drain cadence hides usable decoded frames. | Cache drops are visible, but newest decoded output itself is still behind; diagnostics can confirm whether cache policy is a symptom or contributor. | Low if diagnostics-only. | Secondary diagnostics in the same or next slice. |
| FFmpeg continuous output scale path experiment | Separates FFmpeg scale cost from raw pipe / pixel conversion cost. | Current path is `-vf scale=640:360:flags=neighbor -f rawvideo -pix_fmt bgra pipe:1`; scale and BGRA conversion remain plausible contributors after client/server/feed PASS. | Medium: source-size raw BGRA can be much heavier, and moving scale responsibility may require renderer-side conversion/copy work. | Next opt-in planning candidate, not implemented. |
| One-shot competing load | Measures continuous-vs-one-shot process contention. | Suppression ON already made this a strong contributor candidate and improved throughput/render use, but the latest rerun points more directly at output backlog/stale output. | Medium if made default; low as already opt-in. | Supporting evidence; not the next main culprit and not a default policy. |

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

## Implementation Status
- 2026-05-27 first diagnostics-only slice implemented for the slot0 /
  two-real / opt-in continuous path.
- 2026-05-27 output availability rerun
  `S:\stream-sync\manual-logs\two-client-output-availability-rerun-20260527-173716`
  validates the diagnostics. The key shape is output backlog rather than
  transport, client encoding, server queueing, feed intake, or not-ready
  pressure.
- Summary now exposes:
  - `continuous_decode_pending_correspondence_count`
  - `continuous_decode_pending_correspondence_age_ms_max`
  - `continuous_decode_pending_correspondence_age_ms_avg`
  - `continuous_decode_pending_correspondence_oldest_frame_id`
  - `continuous_decode_pending_correspondence_newest_frame_id`
  - `continuous_decode_latest_input_to_output_frame_gap`
  - `continuous_decode_latest_selected_to_output_frame_gap`
  - `continuous_decode_output_availability_not_ready_count`
  - `continuous_decode_output_availability_stale_count`
  - `continuous_decode_output_availability_future_count`
  - existing reader fields:
    `continuous_decode_reader_full_frame_elapsed_ms_avg`,
    `continuous_decode_reader_full_frame_elapsed_ms_max`,
    `continuous_decode_reader_full_frame_slow_count`,
    `continuous_decode_reader_full_frame_slow_threshold_ms`
- The new availability counters are additive diagnostics tied to bounded lookup
  rejection classes. They do not change exact lookup, bounded lookup, one-shot
  fallback, feed pressure, FFmpeg args, pixel format, scale path, slot coverage,
  or defaults.
- Next human rerun should read these fields together with bounded lookup
  reject counts, output throughput, output lag to selected, pending
  correspondence min/max, and queue drop counts.

## Next Code Candidate Direction
- Do not move back to threshold tuning as the next main line. Lag8 stays a
  held candidate, but the latest availability rerun shows stale/output backlog
  dominates over not-ready.
- The next code candidate should be planning for opt-in output pipeline
  experiments, now tracked in
  `docs/operations/continuous-output-pipeline-experiment-plan.md`, still slot0
  / two-real / opt-in continuous only:
  1. stdout/raw BGRA pipe throughput experiment
  2. FFmpeg scale path split experiment
  3. completed correspondence latency diagnostics
  4. reader blocking phase diagnostics
- These are not default behavior changes. They must keep full-frame correctness,
  same-source and no-future-frame guards, one-shot fallback, current feed max
  count, and current default FFmpeg path unless an explicit opt-in flag is used.
- Recommended first code slice from that plan is completed correspondence
  latency diagnostics because it is additive and does not change FFmpeg output
  shape or reader semantics. This slice is now implemented and adds completed
  correspondence count, avg/max/latest latency, slow count/threshold, and
  completed frame-id min/max to the two-real slot0 opt-in continuous summary.

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
