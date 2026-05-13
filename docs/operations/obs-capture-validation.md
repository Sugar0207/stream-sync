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

## Not In Scope Yet

- OBS WebSocket / advanced OBS control
- distributed-PC validation
- persistent decoder context work
- retry/backoff manager work
- generic N-view refactor
- protocol or architecture changes
