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
- `scaled-bgr24` adoption: HOLD / FAIL.
- Default BGRA remains the safer runtime path.
- BGR24-to-BGRA conversion cost is now a strong bottleneck candidate.
- Threshold tuning alone remains insufficient.
- Production Readiness remains FAIL.

## Candidate Comparison

| Candidate | What It Answers | Expected Value | Risk | Verdict |
| --- | --- | --- | --- | --- |
| BGR24 conversion buffer reuse | Whether allocation / fresh output buffer materialization is a major part of the `26.25ms/frame` conversion cost. | Low-risk first optimization candidate if implemented behind the existing opt-in experiment mode. | Low to medium: must avoid frame lifetime aliasing and preserve decoded-cache ownership. | Best first code candidate after docs review. |
| BGR24 conversion loop optimization | Whether per-pixel conversion mechanics dominate after allocation is reduced. | Could reduce conversion time while keeping renderer-facing BGRA unchanged. | Medium: unsafe writes or SIMD require careful tests and bounds guarantees. | Second candidate after buffer reuse or a focused benchmark. |
| Direct BGR24 render path | Whether avoiding conversion entirely beats optimized conversion. | Removes the `26.25ms/frame` conversion tax if renderer can consume BGR24 safely. | High: current render, compose cache, GDI/OBS-friendly output, and decoded-frame format contracts are BGRA-oriented. | Not first code slice; docs-first impact review only. |
| FFmpeg scale path split | Whether scale/format conversion inside FFmpeg still dominates once pipe bytes and conversion are separated. | Clarifies scale cost vs output bytes vs renderer-side work. | Medium: source-size raw output can multiply bytes/frame and should not become default. | Next opt-in experiment candidate after conversion path review. |
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
1. Next docs/code candidate: BGR24 conversion optimization with buffer reuse or
   safe scalar conversion, opt-in only.
2. Next docs-only comparison: FFmpeg scale path split experiment shape.
3. Later diagnostics: reader blocking phase diagnostics.

Keep default BGRA. Keep `scaled-bgr24` as HOLD / FAIL for adoption. Keep raw
pipe bytes as PARTIAL PASS. Keep Production Readiness FAIL.
