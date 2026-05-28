<!-- stream-sync/docs/operations/continuous-pixel-conversion-plan.md -->

# Continuous Pixel Conversion Plan

Last updated: 2026-05-28

## Purpose
- Plan the next docs-first candidate after the output pipeline A/B rerun.
- Keep `scaled-bgr24` as HOLD / FAIL for adoption while preserving it as useful
  diagnostic evidence.
- Compare BGR24 conversion optimization, direct BGR24 render path, FFmpeg scale
  path split, and reader blocking diagnostics before any new code slice.
- Keep the next code slice opt-in, slot0-only, two-real preview loop only, and
  `--enable-continuous-stream-decoder` only.
- Keep Production Readiness as FAIL.

## Latest Evidence
- latest optimized BGR24 A/B rerun:
  - root:
    `S:\stream-sync\manual-logs\two-client-optimized-bgr24-ab-rerun-20260528-103130`
  - default:
    `S:\stream-sync\manual-logs\two-client-optimized-bgr24-ab-rerun-20260528-103130\default-bgra`
  - optimized:
    `S:\stream-sync\manual-logs\two-client-optimized-bgr24-ab-rerun-20260528-103130\optimized-scaled-bgr24`
- Validity:
  - FFmpeg was available before runtime.
  - Build was PASS with existing dead-code warnings only.
  - Both runs used the same `C:\streamsync-target\stream-sync-rerun\debug\*.exe`.
  - Both servers queued `1800` frames with
    `player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`.
  - Client FPS was close enough for useful comparison.
- Default BGRA:
  - output throughput `26.272fps`
  - reader full-frame avg/max/slow `36.604ms` / `1157ms` / `32`
  - completed correspondence latency avg/max/latest
    `1123.244ms` / `1349ms` / `1066ms`
  - pending correspondence count `35`
  - pending age avg/max `733.029ms` / `1361ms`
  - output lag to selected `33`
  - bounded lookup hits `11`
  - render FPS after first render `15.883`
- Optimized scaled BGR24:
  - output throughput `26.092fps`
  - reader full-frame avg/max/slow `31.108ms` / `1139ms` / `24`
  - completed correspondence latency avg/max/latest
    `1350.666ms` / `1659ms` / `1466ms`
  - pending correspondence count `44`
  - pending age avg/max `932.068ms` / `1653ms`
  - output lag to selected `28`
  - bounded lookup hits `4`
  - render FPS after first render `16.361`
  - pixel conversion total/max/count `2105ms` / `8ms` / `389`
  - conversion average about `5.41ms/frame`
  - reuse/allocation `389` / `0`
  - bytes written total/per-frame `358502400` / `921600`
  - conversion mode `bgr24-in-place-safe-scalar`
- Interpretation:
  - BGR24 conversion optimization is PASS.
  - Optimized `scaled-bgr24` is PARTIAL PASS but adoption HOLD.
  - Default BGRA remains the safer runtime path because completed latency,
    pending age/count, output throughput, and bounded lookup hits still favor
    default overall.
  - Next candidate should move to FFmpeg scale path split or reader/completed
    latency breakdown diagnostics, not `scaled-bgr24` default promotion.

## Scale Path Split Code Slice
- 2026-05-28 first FFmpeg scale path split slice is implemented as opt-in
  `no-scale-bgra`.
- Scope remains narrow:
  - slot0 only
  - two-real preview loop only
  - requires opt-in continuous decoder
  - selected explicitly with
    `--continuous-decoder-output-pipeline-experiment no-scale-bgra`
- The slice does not implement direct BGR24 render, unsafe/SIMD conversion, or
  reader blocking phase diagnostics.
- The slice keeps:
  - default BGRA behavior unchanged
  - optimized `scaled-bgr24` behavior unchanged
  - default path as `scale=640:360:flags=neighbor` + `pix_fmt bgra`
  - `scaled-bgr24` as scaled 640x360 BGR24 + safe scalar in-place BGRA
    expansion
- `no-scale-bgra` removes the continuous FFmpeg scale filter and keeps
  `pix_fmt bgra`, so stdout emits source-size BGRA.
- Risk:
  - source-size 1280x720 BGRA is `3686400` bytes/frame, 4x the 640x360 BGRA
    default.
  - Treat this as diagnostics-only until runtime evidence proves otherwise.
- New summary diagnostics:
  - `continuous_decode_output_pipeline_scale_mode`
  - `continuous_decode_output_source_width`
  - `continuous_decode_output_source_height`
  - `continuous_decode_output_scaled_width`
  - `continuous_decode_output_scaled_height`
  - `continuous_decode_output_scale_removed_count`
  - `continuous_decode_output_scale_path_experiment_enabled`
- Runtime rerun:
  - not performed by Codex
  - next rerun should be human-side from `S:\stream-sync` using the valid
    `C:\streamsync-target\stream-sync-rerun\debug\*.exe` runtime

- pre-optimization output pipeline A/B rerun:
  - `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200`
- pre-optimization output pipeline A/B details:
  - root:
    `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200`
  - default:
    `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200\default-bgra`
  - scaled:
    `S:\stream-sync\manual-logs\two-client-output-pipeline-ab-rerun-20260528-014200\scaled-bgr24`
- Validity:
  - FFmpeg was available before runtime.
  - Build was PASS with existing dead-code warnings only.
  - Both runs used the same build and
    `C:\streamsync-target\stream-sync-rerun\debug\*.exe`.
  - Both servers queued `1800` frames.
  - Client FPS was close enough for useful comparison.
- Default BGRA:
  - output throughput `25.816fps`
  - reader full-frame avg `37.968ms`
  - stdout read throughput `24273.288` bytes/ms
  - completed correspondence latency avg `1309.796ms`
  - pending correspondence age avg `803.227ms`
  - output lag to selected `46`
  - bounded lookup hits `6`
- Scaled BGR24:
  - output throughput `22.150fps`
  - reader full-frame avg `17.739ms`
  - stdout read throughput `38965.867` bytes/ms
  - completed correspondence latency avg `2037.903ms`
  - pending correspondence age avg `1709.438ms`
  - output lag to selected `88`
  - bounded lookup hits `3`
  - pixel conversion total/count `8636ms` / `329`
  - conversion average about `26.25ms/frame`

## Current Verdict
- `scaled-bgr24` wiring / args / expected bytes / reader improvement: PASS.
- Raw pipe bytes hypothesis: PARTIAL PASS.
- BGR24 conversion optimization: PASS.
- Optimized `scaled-bgr24` adoption: HOLD.
- Default BGRA remains the safer runtime path.
- BGR24-to-BGRA conversion cost was reduced from about `26.25ms/frame` to about
  `5.41ms/frame`, but end-to-end latency/backlog still does not clearly beat
  default BGRA.
- FFmpeg scale path split first slice is implemented as opt-in
  `no-scale-bgra`, but runtime evidence is still pending.
- Threshold tuning alone remains insufficient.
- Production Readiness remains FAIL.

## Candidate Comparison

| Candidate | What It Answers | Expected Value | Risk | Verdict |
| --- | --- | --- | --- | --- |
| BGR24 conversion buffer reuse | Whether allocation / fresh output buffer materialization is a major part of the `26.25ms/frame` conversion cost. | Low-risk first optimization candidate if implemented behind the existing opt-in experiment mode. | Low to medium: must avoid frame lifetime aliasing and preserve decoded-cache ownership. | Best first code candidate after docs review. |
| BGR24 conversion loop optimization | Whether per-pixel conversion mechanics dominate after allocation is reduced. | Could reduce conversion time while keeping renderer-facing BGRA unchanged. | Medium: unsafe writes or SIMD require careful tests and bounds guarantees. | Second candidate after buffer reuse or a focused benchmark. |
| Direct BGR24 render path | Whether avoiding conversion entirely beats optimized conversion. | Removes the `26.25ms/frame` conversion tax if renderer can consume BGR24 safely. | High: current render, compose cache, GDI/OBS-friendly output, and decoded-frame format contracts are BGRA-oriented. | Not first code slice; docs-first impact review only. |
| FFmpeg scale path split | Whether scale/format conversion inside FFmpeg still dominates once pipe bytes and conversion are separated. | Clarifies scale cost vs output bytes vs renderer-side work. | Medium: source-size raw output can multiply bytes/frame and should not become default. | First opt-in `no-scale-bgra` code slice implemented; runtime evidence pending. |
| Reader blocking phase diagnostics | Whether remaining reader stalls are first-byte, partial-read, or full-frame completion waits. | Useful attribution after conversion/scale candidates are scoped. | Low if diagnostics-only. | Lower priority than conversion/scale for the next slice because reader avg improved in `scaled-bgr24`. |

## BGR24 Conversion Optimization
Question:

- Can `scaled-bgr24` keep the reader / stdout gains without paying about
  `26.25ms/frame` to rebuild BGRA?

Docs-first implementation candidates:

1. Buffer reuse for the conversion output.
   - Reuse a pre-sized BGRA conversion buffer in the reader or runtime.
   - Preserve decoded-cache ownership by cloning only when ownership requires it,
     or by moving a completed owned buffer after conversion.
   - Summary should keep:
     - `continuous_decode_output_pixel_convert_elapsed_ms`
     - `continuous_decode_output_pixel_convert_elapsed_ms_max`
     - `continuous_decode_output_pixel_convert_count`

2. Safer optimized scalar conversion.
   - Pre-size output and write by index rather than repeated small appends.
   - Keep alpha fixed at `255`.
   - Add focused unit tests for byte order and exact output length.

3. Unsafe / SIMD conversion only after a safe baseline.
   - Candidate only if scalar optimized conversion remains too slow.
   - Must stay behind tests and opt-in experiment mode.
   - Do not make it the first implementation slice.

First safe code slice if selected:

- slot0 / two-real / opt-in continuous only.
- Keep default mode BGRA.
- Keep `scaled-bgr24` opt-in.
- Add no new fallback behavior.
- Add or keep summary fields for conversion avg/max/count and pipe bytes saved.
- Success criterion is not production readiness; it is whether conversion avg
  drops enough that end-to-end output throughput / completed latency no longer
  regresses versus default BGRA.

Implementation status:

- 2026-05-28 first conversion optimization code slice implemented for
  `scaled-bgr24` only.
- Default BGRA behavior is unchanged.
- The reader now allocates the final BGRA-sized output buffer for
  `scaled-bgr24`, reads only the BGR24 pipe payload into the front of that
  buffer, then expands BGR24 to BGRA in-place with a safe reverse scalar loop.
- This avoids the previous extra conversion output `Vec` and repeated small
  append path while preserving the renderer-facing BGRA frame contract.
- Diagnostic meaning:
  - reuse count increments when the final BGRA frame buffer is reused as the
    conversion target.
  - allocation count tracks separate conversion-buffer allocation and should
    remain `0` for the optimized `scaled-bgr24` path.
- New summary fields:
  - `continuous_decode_output_pixel_convert_buffer_reuse_count`
  - `continuous_decode_output_pixel_convert_buffer_allocation_count`
  - `continuous_decode_output_pixel_convert_bytes_written_total`
  - `continuous_decode_output_pixel_convert_bytes_written_per_frame`
  - `continuous_decode_output_pixel_convert_mode`
- Expected optimized mode value:
  - `bgr24-in-place-safe-scalar`
- Latest optimized rerun result:
  - conversion optimization PASS
  - `scaled-bgr24` adoption HOLD
  - default BGRA remains the safe path

## Direct BGR24 Render Path
Question:

- Can render consume BGR24 directly so the continuous reader does not convert
  back to BGRA?

Impact boundaries to review before implementation:

- decoded frame pixel format contract
- quad composition / incremental composition
- OBS output profile and expected BGRA buffers
- GDI paint / Windows bitmap assumptions
- placeholder and mixed-source composition behavior
- test fixtures that assume `Bgra8`

Verdict:

- Direct BGR24 render path is potentially valuable, but it is too wide for the
  next code slice unless a narrow internal render boundary already supports
  multi-format input.
- Do not implement direct render path before a focused impact review.
- Do not let direct render path change default output mode or renderer contract
  globally.

## FFmpeg Scale Path Split
Question:

- After proving pipe bytes matter and conversion is expensive, how much of the
  remaining output lag is FFmpeg scale / pixel-format work?

Candidate experiment shapes:

1. Control: default scaled BGRA.
   - Current safe runtime path.
   - `640x360`, BGRA, `921600` bytes/frame.

2. Current diagnostic: scaled BGR24.
   - Keeps scale path fixed.
   - Reduces pipe bytes to `691200`.
   - HOLD / FAIL for adoption due to conversion cost.

3. Scale split with renderer-size output preserved.
   - Prefer an opt-in variant that keeps output dimensions at `640x360` while
     moving only one responsibility at a time.
   - Do not combine no-scale, pixel-format, and render-contract changes in one
     slice.

4. Source-size raw output.
   - High-risk diagnostic only.
   - A `1280x720` BGRA frame is `3686400` bytes, about 4x the current
     `640x360` BGRA output.
   - Do not make source-size raw output a default or first implementation.

Success fields for any scale split rerun:

- output mode / experiment mode
- output dimensions
- FFmpeg output pixel format
- expected bytes/frame
- pipe bytes saved/frame
- reader full-frame avg/max/slow
- stdout throughput
- pixel conversion avg/max/count, if conversion exists
- completed correspondence latency avg/max/latest
- pending correspondence age avg/max
- output lag to selected
- bounded lookup hits / render continuous use

## Reader Blocking Phase Diagnostics
- Keep as a later diagnostics-only candidate.
- `scaled-bgr24` already showed reader avg can improve when bytes/frame is
  reduced, so conversion/scale are higher priority now.
- If output backlog remains after conversion/scale work, split reader wait into:
  - waiting for first byte
  - partial bytes below full frame
  - waiting for remaining full-frame bytes
  - correspondence pop / event send

## Recommendation
1. Next evidence candidate: human-side `no-scale-bgra` A/B rerun for the
   implemented FFmpeg scale path split slice.
2. Next code candidate after that: reader/completed latency breakdown
   diagnostics if no-scale evidence is ambiguous.
3. Keep direct BGR24 render path docs-first only.
4. Keep unsafe / SIMD conversion as a later candidate only if safe scalar
   conversion becomes the proven remaining bottleneck.

Keep default BGRA. Treat optimized `scaled-bgr24` conversion as PASS but
adoption as HOLD. Keep raw pipe bytes as PARTIAL PASS. Keep Production
Readiness FAIL.
