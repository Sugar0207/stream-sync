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
- latest optimized BGR24 A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-optimized-bgr24-ab-rerun-20260528-103130`
  - evidence is VALID-ish / useful:
    - FFmpeg available before runtime
    - build PASS with existing dead-code warnings only
    - same `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
    - both default and optimized servers queued `1800` frames
  - optimized `scaled-bgr24` conversion optimization PASS:
    - conversion average improved from about `26.25ms/frame` to about
      `5.41ms/frame`
    - reuse count `389` equals conversion count
    - allocation count `0`
    - bytes written per frame `921600`
    - mode `bgr24-in-place-safe-scalar`
  - optimized `scaled-bgr24` availability/read improvements:
    - stdout bytes/frame reduced `921600 -> 691200`
    - reader avg improved `36.604ms -> 31.108ms`
    - reader slow count improved `32 -> 24`
    - output lag to selected improved `33 -> 28`
    - render FPS after first render improved `15.883 -> 16.361`
  - optimized `scaled-bgr24` adoption HOLD:
    - output throughput slightly worsened `26.272fps -> 26.092fps`
    - completed latency avg worsened `1123.244ms -> 1350.666ms`
    - pending age avg worsened `733.029ms -> 932.068ms`
    - pending count worsened `35 -> 44`
    - bounded lookup hits fell `11 -> 4`
  - current availability verdict:
    - conversion optimization is PASS
    - raw pipe bytes hypothesis is PARTIAL PASS
    - default BGRA remains the safe path
    - optimized `scaled-bgr24` adoption is HOLD
    - next candidate is FFmpeg scale path split or reader/completed latency
      breakdown diagnostics
    - Production Readiness remains FAIL
- latest output pipeline A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200`
  - evidence is VALID-ish / useful:
    - FFmpeg available before runtime
    - build PASS with existing dead-code warnings only
    - same build / same `C:\streamsync-target\stream-sync-rerun\debug\*.exe`
    - both default and scaled servers queued `1800` frames
  - `scaled-bgr24` PASS:
    - flag / args / summary wiring correct
    - stdout bytes/frame reduced `921600 -> 691200`
    - reader avg improved `37.968ms -> 17.739ms`
    - stdout throughput improved `24273.288 -> 38965.867` bytes/ms
  - `scaled-bgr24` FAIL for adoption:
    - output throughput worsened `25.816fps -> 22.150fps`
    - completed latency avg worsened `1309.796ms -> 2037.903ms`
    - pending age avg worsened `803.227ms -> 1709.438ms`
    - output lag to selected worsened `46 -> 88`
    - bounded lookup hits fell `6 -> 3`
    - pixel conversion cost was `8636ms / 329 ~= 26.25ms/frame`
  - current availability verdict:
    - raw pipe bytes hypothesis is PARTIAL PASS
    - BGR24-to-BGRA conversion cost is a new strong bottleneck candidate
    - keep default BGRA
    - hold / fail `scaled-bgr24` adoption
    - Production Readiness remains FAIL
    - detailed conversion/direct-render review now lives in
      `docs/operations/continuous-pixel-conversion-plan.md`
- latest completed correspondence rerun:
  - `S:\stream-sync\manual-logs\two-client-completed-correspondence-rerun-20260528-010504`
  - rerun is VALID:
    - FFmpeg preflight passed with `8.1.1-full_build-www.gyan.dev`
    - switcher binary:
      `C:\streamsync-target\stream-sync-rerun\debug\stream-sync-switcher.exe`
      LastWriteTime `2026/05/28 1:05:18`
    - client1/client2 sent `900` frames at `29.443fps` / `29.112fps`
    - server queued `1800` frames total
  - completed correspondence diagnostics are VALID:
    - count `301`
    - latency avg `2624.940ms`
    - max `5258ms`
    - latest `5251ms`
    - slow count `301` at threshold `66ms`
  - pending age is also large:
    - pending count `137`
    - avg `2540.606ms`
    - max `5300ms`
  - output availability verdict:
    - continuous output `17.151fps` versus source about `29fps`
    - stale rejects `228` dominate not-ready `19`
    - completed outputs and unfinished backlog are both seconds late
    - threshold tuning alone is insufficient
    - next candidate moves to raw BGRA pipe / stdout throughput and FFmpeg
      scale path split opt-in experiments
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
| Raw BGRA pipe throughput / stdout reader buffering diagnostics | Whether `921600` byte full-frame reads, short reads, allocation, or reader scheduling dominate output latency. | Optimized A/B shows `scaled-bgr24` improves reader avg after bytes/frame falls to `691200`, but end-to-end still favors default BGRA. | Low for diagnostics; medium for buffering behavior changes. | PARTIAL PASS as a hypothesis; optimized `scaled-bgr24` adoption HOLD. |
| Continuous output queue/cache policy diagnostics | Whether decoded cache bound `30`, dropped stale count, or drain cadence hides usable decoded frames. | Cache drops are visible, but newest decoded output itself is still behind; diagnostics can confirm whether cache policy is a symptom or contributor. | Low if diagnostics-only. | Secondary diagnostics in the same or next slice. |
| FFmpeg continuous output scale path experiment | Separates FFmpeg scale cost from raw pipe / pixel conversion cost. | Optimized BGR24 reduced conversion cost, so scale/output responsibility is now the cleaner next split after client/server/feed PASS. | Medium: source-size raw BGRA can be much heavier, and moving scale responsibility may require renderer-side conversion/copy work. | Next opt-in planning candidate, not implemented. |
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
- The first opt-in output pipeline experiment slice has now been rerun, still
  slot0 / two-real / opt-in continuous only:
  - `--continuous-decoder-output-pipeline-experiment scaled-bgr24`
  - default remains scaled BGRA `921600` bytes/frame
  - experiment mode keeps scaling but emits BGR24 `691200` bytes/frame and
    converts back to BGRA before render
  - summary reports experiment mode, pixel format, bytes/frame, pipe bytes
    saved/frame, pixel conversion time, stdout throughput, reader latency, and
    correspondence backlog
  - verdict: wiring and reader improvement PASS, raw pipe bytes PARTIAL PASS,
    but adoption HOLD / FAIL due to BGR24-to-BGRA conversion cost
- 2026-05-28 first conversion optimization slice is implemented for
  `scaled-bgr24` only. It keeps the renderer-facing BGRA contract but expands
  BGR24 to BGRA in-place with a safe reverse scalar loop and adds conversion
  reuse/allocation/bytes/mode summary fields.
- 2026-05-28 optimized BGR24 A/B rerun validates conversion optimization as
  PASS, but keeps `scaled-bgr24` adoption HOLD. The next code candidate moves
  to FFmpeg scale path split opt-in experiment or reader/completed latency
  breakdown diagnostics rather than another conversion/default-promotion step.
- Next candidate order:
  1. FFmpeg scale path split opt-in experiment docs-first review
  2. reader/completed latency breakdown diagnostics
  3. direct BGR24 render path impact review only
- These are not default behavior changes. They must keep full-frame correctness,
  same-source and no-future-frame guards, one-shot fallback, current feed max
  count, and current default FFmpeg path unless an explicit opt-in flag is used.
- Recommended first code slice from that plan is completed correspondence
  latency diagnostics because it is additive and does not change FFmpeg output
  shape or reader semantics. This slice is now implemented and adds completed
  correspondence count, avg/max/latest latency, slow count/threshold, and
  completed frame-id min/max to the two-real slot0 opt-in continuous summary.
- The latest completed correspondence rerun validates that slice and shifts the
  next candidate to raw BGRA pipe / stdout throughput and FFmpeg scale path
  split opt-in experiments.

## Later Opt-In Experiments
1. FFmpeg scale path experiment
   - Next preferred opt-in experiment family after optimized BGR24 conversion.
   - Keep source-size raw output as an explicit risk, not the default candidate.
   - Compare current scaled BGRA output against a narrow opt-in variant that
     separates scale cost from pipe/output cost.

2. Raw BGRA pipe / stdout reader buffering experiment
   - Only after diagnostics show reader-bound behavior.
   - Preserve full-frame correctness and do not emit partial frames.
   - Summary must expose the experiment flag and byte/read timing deltas.

3. Queue/cache policy experiment
   - Only after diagnostics prove decoded cache/drop policy is hiding otherwise
     safe frames.
   - Any display decision still needs at-or-before selected/target frame and
     same-source checks.

## Readiness
- Threshold branch: HOLD / candidate.
- Output availability / throughput investigation: next main line.
- Production Readiness: FAIL.
