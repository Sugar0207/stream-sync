# Manual Real Encoded VideoFrame E2E Checklist

This checklist verifies the current real encoded video path:

```text
Windows Graphics Capture -> BGRA frame -> FFmpeg H.264 -> RealCaptureH264 VideoFrame -> direct or fragmented UDP -> server auth gate -> reassembly -> queue/source -> switcher decode/render
```

The one-shot named-pipe handoff commands now exist as bounded manual
diagnostics. They are still not a continuous service loop, but they can be
used for one request / one response validation after the server has queued at
least one frame. The current manual checklist now uses:

- `stream-sync-server --receive-auth-video-queue-and-serve-handoff-once ...`
  for queue-owning server receive plus one named-pipe handoff
- `stream-sync-server --receive-auth-video-queue-and-serve-handoff-many ...`
  for the bounded server-owned service session: queue-owning server receive
  plus bounded `max_requests` named-pipe handoff serving in the same process
  lifetime
- `stream-sync-switcher --read-queued-frame-handoff-once ...` for one
  switcher-side named-pipe read
- `stream-sync-server --receive-auth-video-queue-once ...` for the queue-owning
  server path when queue-only verification is enough
- `stream-sync-switcher --live-two-view-switcher-once ...` only as the
  direct-receive diagnostic/legacy path

The preferred sender is now the bounded authenticated sender:

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-bounded configs/examples/client.accepted.example.toml 5
```

The one-shot real encoded commands remain available for low-level checks, but
bounded mode is preferred when the previous one-shot result was:

```text
NoFrameAvailable { message: "Windows Graphics Capture frame pool had no queued frame" }
```

Supplemental deterministic 4-view proof utility:

```powershell
cargo run -p stream-sync-switcher -- --four-view-proof-fixture-once <all-renderable|mixed-placeholder-source-error|placeholder-only>
```

Use this only as a bounded manual proof utility for the internal 4-view
switcher chain. It is deterministic, uses in-process fixtures plus fake or
backend-unavailable window-render behavior, and does not prove actual OS-window
render, OBS output, or real server->switcher handoff.

The next planned manual proof for 4-view actual OS-window rendering should stay
separate from this command. Keep `--four-view-proof-fixture-once` as the
backend-free deterministic proof utility, and add a later isolated actual-window
proof command that starts with the `all-renderable` fixture only.

That isolated actual-window proof command now exists:

```powershell
cargo run -p stream-sync-switcher -- --four-view-proof-window-once all-renderable
```

Use it only for the first isolated 4-view OS-window proof. It reuses the
deterministic all-renderable fixture and the existing composed BGRA window path.
It does not prove OBS output or real server->switcher handoff/manual preview.
The current one-shot proof is expected to close immediately after the render
attempt. In the latest recorded manual pass, a window appeared and then closed
immediately, which is acceptable for this slice. A future `--hold-ms` option
may be useful for visual confirmation, but it is not implemented yet.

The dedicated clean output window command also now exists:

```powershell
cargo run -p stream-sync-switcher -- --four-view-clean-output-window-once all-renderable
```

Use this command when you want the first thin manual/runtime path for the
dedicated OBS-facing output window identity rather than the proof window path.
Its stable window title is:

```text
StreamSync 4-view Output
```

This command remains deterministic and `real_handoff=false`. It is still not
OBS output itself, does not prove real server->switcher handoff/manual preview,
and currently keeps hold duration at `0`, so a future `--hold-ms` remains
optional polish only. In the latest recorded manual pass, a window appeared and
then closed immediately. Because the command is still one-shot, the title could
not be visually confirmed before close; treat the stdout
`window_title=StreamSync 4-view Output` as the current identity proof for this
slice.

For the first OBS validation path, use this dedicated clean output identity as
the intended Window Capture target and keep the proof window path out of OBS.
The current one-shot close is acceptable for proof logging, but it is a
practical limitation for manual OBS selection/preview. Plan manual OBS guidance
first, then add a longer-lived dedicated clean output runtime path before
attempting stable OBS operator validation. A future `--hold-ms` can remain
optional polish rather than the primary OBS-facing plan.

The bounded clean output loop command for that stable manual OBS path now
exists:

```text
stream-sync-switcher --four-view-clean-output-window-loop [all-renderable] [frames]
```

Current behavior for that command:

- keep the stable clean output title `StreamSync 4-view Output`
- repeatedly render the deterministic `all-renderable` fixture first
- stay bounded by frame count rather than becoming an indefinite daemon
- use a fixed 30 fps cadence so the visible lifetime is roughly
  `frames / 30`
- keep one persistent window identity for the whole bounded loop
- update that same window per frame
- close the window once after the bounded loop completes
- apply a fixed OBS validation output profile:
  - `output_width=1280`
  - `output_height=720`
  - `scale_mode=nearest-neighbor`
- preserve the deterministic source frame dimensions separately from the
  scaled output surface
- print:
  - `command_name`
  - `fixture_mode`
  - `clean_output_window=true`
  - `actual_window_render=true`
  - `real_handoff=false`
  - `window_title`
  - `frames_attempted`
  - `frames_rendered`
  - `render_failures`
  - `window_created`
  - `persistent_window`
  - `window_updates`
  - `window_closed`
  - `source_width`
  - `source_height`
  - `output_width`
  - `output_height`
  - `scale_mode`
  - `window_visible`
  - `window_capture_candidate`
  - `bgra_payload_len`

Use this command, not the one-shot clean output command, when manual OBS Window
Capture validation needs a longer-lived stable capture target. The one-shot
command remains useful as a thinner identity proof path.

Recorded limitation from the first non-persistent loop attempt:

- stdout reported:
  - `frames_attempted=300`
  - `frames_rendered=300`
  - `render_failures=0`
- but OBS Window Capture could not select the window and preview stayed empty
- observed behavior was repeated brief window appearance/disappearance during
  the loop, which strongly suggested one window was being created and closed per
  frame

That limitation is the reason the current loop implementation now requires one
persistent window/session across the bounded run. This is still not recorded as
a successful OBS validation yet; rerun manual OBS validation against the
persistent-lifecycle implementation before treating this slice as complete.

Recorded limitation from the persistent-lifecycle rerun:

- stdout reported:
  - `frames_attempted=300`
  - `frames_rendered=300`
  - `render_failures=0`
  - `window_created=true`
  - `persistent_window=true`
  - `window_updates=300`
  - `window_closed=true`
  - `width=4`
  - `height=2`
- the previous flicker/per-frame recreate behavior appeared fixed
- but OBS Window Capture still could not select the window and preview stayed
  empty
- observed behavior was:
  - one persistent window remained alive for the bounded run
  - the visible window surface stayed black

Treat this as an OBS-friendly output profile limitation, not as a completed OBS
validation. The next preferred slice is:

- keep the same persistent window lifecycle
- keep the stable title `StreamSync 4-view Output`
- keep the deterministic `all-renderable` fixture first
- add a fixed OBS validation profile with output size `1280x720`
- scale the current deterministic fixture/composed frame into that larger
  output surface
- extend stdout with:
  - `source_width`
  - `source_height`
  - `output_width`
  - `output_height`
  - `scale_mode`
  - `window_visible`
  - `window_capture_candidate`

Do not widen this next slice into OBS WebSocket, real server->switcher
handoff/manual preview, Focused view, hotkey UI, or generic N-view work.

That fixed `1280x720` profile is now implemented on the existing bounded loop
command. The next manual check is not another planning pass; rerun:

```powershell
cargo run -p stream-sync-switcher -- --four-view-clean-output-window-loop all-renderable 300
```

For the next rerun, confirm the stdout now reports at least:

- `source_width=4`
- `source_height=2`
- `output_width=1280`
- `output_height=720`
- `scale_mode=nearest-neighbor`
- `bgra_payload_len=3686400`

Then retry OBS Window Capture selection and preview against the same stable
title `StreamSync 4-view Output`. If OBS still cannot capture the window after
this larger profile, treat render-surface/window-style investigation as the
next narrower slice.

Recorded successful OBS Window Capture validation:

```powershell
stream-sync-switcher --four-view-clean-output-window-loop all-renderable 900
```

Observed success conditions:

- OBS was already open before running the command
- OBS could select `StreamSync 4-view Output`
- OBS preview showed the clean output window content
- the visible output was the deterministic 4-view QuadView clean output
- the window remained the dedicated clean output path, not the proof window
- `real_handoff=false` remained true for this validation path

What this success proves:

- the dedicated clean output window is now a usable downstream OBS Window
  Capture target
- the fixed `1280x720` output profile solved the earlier too-small/black-surface
  validation problem for the deterministic proof path
- the current clean output semantics remain the fixed QuadView layout:
  - `slot 0`: top-left
  - `slot 1`: top-right
  - `slot 2`: bottom-left
  - `slot 3`: bottom-right

What this success does not prove:

- real server->switcher handoff video
- real 4-view preview driven by queued encoded frames
- OBS API / WebSocket / advanced OBS control

After this success, the next step is not more OBS capture troubleshooting. The
next step is planning how the real server->switcher handoff path should feed
the existing 4-view preview/output family while preserving the dedicated clean
output window path.

Planned smallest real handoff-driven 4-view preview path:

- keep the existing server bounded handoff service session:

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-and-serve-handoff-many <config-path> <pipe-name> <max-requests> <max-video-packets> <receive-timeout-ms> <expected-reassembled-frames> <stop-after-expected-reassembled-frames> <receive-buffer-bytes>
```

- add one bounded switcher-side real preview loop above the existing named-pipe
  handoff wrapper and 4-view validation/output family
- first mixed preview should use:
  - `1` real handoff slot
  - `3` deterministic non-real slots
- preferred first switcher command shape:
- that first switcher command now exists:

```powershell
cargo run -p stream-sync-switcher -- --four-view-real-handoff-preview-loop [pipe-name] [real-slot-index] [client-id] [run-id] [frames]
```

Planned semantics for that first mixed preview:

- `real_handoff=true`
- real handoff enters through the existing named-pipe-backed
  `SwitcherQueuedFrameHandoff`
- the existing `SwitcherFourViewHandoffValidationBoundary` continues to own
  scheduler / display / composition / clean-output rendering
- intentionally non-real slots stay deterministic fixture-backed
- configured real slot with no eligible queued frame:
  - `NoFrameAvailable`
- configured real slot with frame newer than target timestamp:
  - `WaitingForFrameAtOrBeforeTarget`
- named-pipe/runtime failure:
  - `HandoffError`

Current first-slice implementation details:

- validates `real-slot-index` as `0..3`
- validates `frames` as a positive bounded integer
- uses the configured real slot only for named-pipe handoff
- routes the other three slots to deterministic `NoFrameAvailable`
  placeholder content
- reuses `SwitcherFourViewHandoffValidationBoundary`
- reuses the dedicated clean output family:
  - stable title `StreamSync 4-view Output`
  - persistent output-loop semantics
  - fixed `1280x720` OBS-friendly output profile
- suppresses the proof/debug window path for this command

Recommended first manual shape for this slice:

### Terminal 1: Server bounded handoff session

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-and-serve-handoff-many <config-path> <pipe-name> <max-requests> <max-video-packets> <receive-timeout-ms> <expected-reassembled-frames> <stop-after-expected-reassembled-frames> <receive-buffer-bytes>
```

### Terminal 2: Switcher mixed real 4-view preview loop

```powershell
cargo run -p stream-sync-switcher -- --four-view-real-handoff-preview-loop <pipe-name> <real-slot-index> <client-id> <run-id> <frames>
```

The expected stdout summary for the switcher command should include at least:

- `real_handoff=true`
- `real_slot_count=1`
- `real_slot_index`
- `pipe_name`
- `client_id`
- `run_id`
- `frames_attempted`
- `frames_rendered`
- `render_failures`
- `scheduler_status`
- per-slot binding / result-kind information
- `clean_output_render_result_kind`
- `window_title=StreamSync 4-view Output`
- `output_width`
- `output_height`

Recorded successful 1-real-slot manual command sequence:

### Terminal 1: Server

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-and-serve-handoff-many configs/examples/server.example.toml streamsync-handoff-dev 5 4096 15000 1 true 8388608
```

Observed server result:

- auth accepted for `client_id=player1`
- one real frame was received, reassembled, and queued
- bounded named-pipe service served `5/5` requests successfully
- all `5` responses were `FrameRead`

### Terminal 2: Client

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-bounded configs/examples/client.accepted.example.toml 5 16 1
```

Observed client result:

- auth succeeded
- `frames_captured=5`
- `frames_encoded=5`
- `frames_sent=5`
- `fragmented_sends=5`
- `fragments_sent=1815`
- `send_failures=0`

### Terminal 3: Switcher

```powershell
cargo run -p stream-sync-switcher -- --four-view-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 5
```

Observed switcher result:

- `real_handoff=true`
- `real_slot_count=1`
- `real_slot_index=0`
- `frames_attempted=5`
- `frames_rendered=5`
- `render_failures=0`
- `scheduler_status=PartialSelected`
- `slot_result_kinds=Selected|NoFrameAvailable|NoFrameAvailable|NoFrameAvailable`
- `clean_output_render_result_kind=Rendered`
- `window_title=StreamSync 4-view Output`
- `output_width=1280`
- `output_height=720`

### OBS Window Capture

Observed result:

- OBS could display `StreamSync 4-view Output`
- preview showed output
- a real-slot-like image was visible

What this successful manual pass proves:

- client auth succeeded
- client captured / encoded / sent real encoded frames
- server received / reassembled / queued a real frame
- server served repeated named-pipe `FrameRead` responses successfully
- switcher consumed the named-pipe handoff successfully
- configured real slot `0` selected real handoff frames
- the remaining `3` slots stayed deterministic placeholder / no-frame slots
- the existing clean output family rendered the mixed preview successfully
- OBS Window Capture displayed the downstream clean output window

What remains out of scope after this success:

- `2` real slots
- `4` real slots
- `Focused(slot_index)`
- full hotkey UI
- generic N-view refactor
- protocol wire-format / H.264 behavior changes
- switcher-side fragment reassembly
- OBS WebSocket / advanced OBS control

After this success, the next docs/planning step is no longer first real-slot
validation. The next step is planning the smallest `2` real slots preview
slice while keeping the current `1` real slot validated path intact.

Planned next `2`-real-slot preview direction:

- keep one existing bounded server queue/handoff service session
- keep one named pipe
- use two distinct real `client_id + run_id` scopes in that one shared
  queue/service lifetime
- keep the remaining `2` slots deterministic placeholder / no-frame slots
- keep OBS downstream of `StreamSync 4-view Output`
- keep the existing named-pipe handoff and 4-view validation/output family
- do not widen the validated `1`-real-slot command with optional extra
  positional arguments first
- prefer a dedicated next command shape:
  - that dedicated command now exists:

```text
stream-sync-switcher --four-view-two-real-handoff-preview-loop [pipe-name] [slot0-index] [client0-id] [run0-id] [slot1-index] [client1-id] [run1-id] [frames]
```

Current first-slice implementation details for that command:

- validates both slot indices in `0..3`
- validates the two slot indices are distinct
- validates `frames` as a positive bounded integer
- uses the existing named-pipe handoff wrapper/client for the two real slots
- keeps the remaining `2` slots deterministic placeholder / no-frame slots
- reuses `SwitcherFourViewHandoffValidationBoundary`
- reuses the dedicated clean output family:
  - `window_title=StreamSync 4-view Output`
  - persistent output-loop semantics
  - fixed `1280x720` OBS-friendly output profile

Planned semantics for missing one real client in that next slice:

- `Selected + NoFrameAvailable` when one real slot has an eligible frame and
  the other does not
- `Selected + WaitingForFrameAtOrBeforeTarget` when one real slot is newer than
  target timestamp
- `HandoffError` only for named-pipe/runtime/transport failure

Planned stdout summary additions for that next slice:

- `real_handoff=true`
- `real_slot_count=2`
- explicit per-real-slot index / `client_id` / `run_id`
- `slot_bindings`
- `slot_result_kinds`
- `scheduler_status`
- `frames_attempted`
- `frames_rendered`
- `render_failures`
- `clean_output_render_result_kind`
- `window_title=StreamSync 4-view Output`
- `output_width`
- `output_height`

The `2`-real-slot command is now manually validated with two distinct real
`client_id + run_id` scopes in one bounded server handoff session. Keep the
`1`-real-slot path as an isolation baseline, but the `2`-real-slot path now
has two known-good manual recipes recorded below.

Dedicated manual test configs now exist for that upcoming validation:

- `configs/manual/server.two-real-slots.toml`
- `configs/manual/client.player1.toml`
- `configs/manual/client.player2.toml`
- `configs/manual/client.player3.toml`
- `configs/manual/client.player4.toml`

These are based on the existing example configs rather than replacing them.
The current switcher commands still do not read a switcher config file for this
path, so no dedicated `configs/manual/switcher.two-real-slots.toml` was added
in this slice.

Observed successful minimal `2`-real-slot command sequence using the new
configs (`1` frame each):

### Server

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 10 4096 5000 2 true 8388608
```

### Client 1

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 1 16 1
```

### Client 2

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 1 16 1
```

### Switcher

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 5
```

Observed successful preferred `2`-real-slot command sequence (`2` frames each):

### Server

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 10 4096 5000 4 true 8388608
```

### Client 1

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1
```

### Client 2

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1
```

### Switcher

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 5
```

Recommended guarded server recipe for repeatable reruns and future `4`-real-slot
work:

- Keep the existing `expected_reassembled_frames` threshold.
- Add the optional client-aware thresholds at the end of the server command:
  - `expected_reassembled_clients`
  - `expected_reassembled_frames_per_client`
- All enabled stop conditions must be satisfied before the receive phase ends.

Recommended guarded `2`-real-slot proof (`2` frames each):

### Server

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 10 4096 5000 4 true 8388608 2 2
```

### Client 1

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1
```

### Client 2

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1
```

### Switcher

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 5
```

Recommended isolation command when player2 is missing or the 2-real-slot path
reports unexpected `HandoffError`:

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 5
```

Receive-phase sequencing requirement:

- start the server first
- start player1 and player2 while the server is still in receive/auth phase
- start the switcher only after both clients have completed auth/send
- the current server runtime now accepts additional auth requests during the
  receive phase, so the second client no longer depends on winning the very
  first auth slot

Expected diagnostic interpretation for the successful `2`-real-slot rerun:

- server bounded handoff request lines should be read primarily through:
  - `queue_len_before_read`
  - `queue_len_after_read`
  - `selected_client_id`
  - `selected_run_id`
  - `frame_id`
  - `frame_payload_len`
  - `no_frame_reason`
- switcher preview loop lines should be read primarily through:
  - `slot_result_kinds`
  - `slot_diagnostics`
- `slot_diagnostics` now carries per-slot:
  - `request_id`
  - `handoff_response_kind`
  - `parse_error`
  - `io_error`
  - `decode_error`
  - `response_payload_len`
  - `frame_id`
  - `frame_payload_len`
  - `render_input_kind`
  - `final_slot_result_kind`

Expected successful `2`-real-slot results:

- server receive summary:
  - `registered_clients=2`
  - minimal recipe: `frames_reassembled=2` / `frames_queued=2`
  - preferred recipe: `frames_reassembled=4` / `frames_queued=4`
  - guarded preferred recipe:
    - `manual_expected_reassembled_clients=2`
    - `manual_expected_reassembled_frames_per_client=2`
    - `observed_reassembled_clients=2`
    - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2`
    - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
- server bounded handoff request lines:
  - `selected_client_id=player1` -> `result_kind=FrameRead`
  - `selected_client_id=player2` -> `result_kind=FrameRead`
  - `frame_payload_len > 0` for both clients
- switcher summary:
  - `slot_result_kinds=Selected|Selected|NoFrameAvailable|NoFrameAvailable`
  - `scheduler_status=PartialSelected`
  - `clean_output_render_result_kind=Rendered`
- switcher slot diagnostics:
  - slot0/player1 -> `handoff_response_kind=FrameRead`
  - slot1/player2 -> `handoff_response_kind=FrameRead`
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`
  - `final_slot_result_kind=Selected`

Important note for `FrameRead / NoFrame` alternation:

- the old server `queue_len` field did not say whether it was `before` or
  `after` the queue read
- the new logs separate `queue_len_before_read` from `queue_len_after_read`
- in a `2`-real-slot preview run, alternating `FrameRead` and `NoFrame` can be
  normal if the requests are alternating between:
  - slot0 `player1`
  - slot1 `player2`
- check `selected_client_id` / `selected_run_id` before assuming the queue was
  reset or recreated between requests
- the earlier `FrameRead / NoFrame` rerun was request-order-driven because only
  player1 had authenticated and queued before handoff serving began
- after the receive-phase auth fix and updated manual sequencing, both client
  scopes can be present in the same bounded receive/handoff lifetime
- before the optional client-aware guard existed, operator sequencing errors
  could still allow one client to satisfy the total-frame threshold alone
- the guarded recipe above prevents that by requiring both:
  - total reassembled frames
  - client-aware thresholds

Recommended next guarded shape for future `4`-real-slot work:

- minimal proof:
  - total frames `4`
  - `expected_reassembled_clients=4`
  - `expected_reassembled_frames_per_client=1`
- preferred proof:
  - total frames `8`
  - `expected_reassembled_clients=4`
  - `expected_reassembled_frames_per_client=2`
- expected receive summary:
  - `registered_clients=4`
  - `observed_reassembled_clients=4`
  - `per_client_reassembled_frames=` for player1..player4
- expected switcher summary after an all-real 4-slot command exists:
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`

Observed successful guarded `4`-real-slot command sequence (`2` frames each):

### Server

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 20 4096 5000 8 true 8388608 4 2
```

### Client 1

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1
```

### Client 2

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1
```

### Client 3

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 2 16 1
```

### Client 4

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 2 16 1
```

### Switcher

Fixed-order all-real command:

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5
```

This command keeps the current 4-view scope fixed and does not introduce a
generic N-view surface:

- slot0 is always `client0/run0`
- slot1 is always `client1/run1`
- slot2 is always `client2/run2`
- slot3 is always `client3/run3`

Observed successful `4`-real-slot results:

- server receive summary:
  - `registered_clients=4`
  - `manual_expected_reassembled_frames=8`
  - `manual_expected_reassembled_clients=4`
  - `manual_expected_reassembled_frames_per_client=2`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
- server bounded handoff request lines:
  - player1 scope -> `FrameRead`
  - player2 scope -> `FrameRead`
  - player3 scope -> `FrameRead`
  - player4 scope -> `FrameRead`
  - `queue_len_before_read=2`
  - `queue_len_after_read=2`
  - `frame_payload_len > 0` for all four scopes
- switcher summary:
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `frames_rendered=5`
  - `render_failures=0`
- switcher slot diagnostics:
  - all four slots:
    - `handoff_response_kind=FrameRead`
    - `parse_error=none`
    - `io_error=none`
    - `decode_error=none`
    - `final_slot_result_kind=Selected`

Repeated stability observation for the guarded `4`-real-slot recipe:

- Run `1`:
  - server:
    - `registered_clients=4`
    - `frames_reassembled=8`
    - `frames_queued=8`
    - `observed_reassembled_clients=4`
    - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
    - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
    - `receive_timed_out=false`
    - `max_packets_reached=false`
    - `handoff_errors=0`
  - switcher:
    - `frames_attempted=5`
    - `frames_rendered=5`
    - `render_failures=0`
    - `scheduler_status=AllSelected`
    - `slot_result_kinds=Selected|Selected|Selected|Selected`
    - `clean_output_render_result_kind=Rendered`
    - all slot diagnostics kept `parse_error=none`, `io_error=none`,
      `decode_error=none`
- Run `2`:
  - server:
    - `registered_clients=4`
    - `frames_reassembled=8`
    - `frames_queued=8`
    - `observed_reassembled_clients=4`
    - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
    - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
    - `receive_timed_out=false`
    - `max_packets_reached=false`
    - `handoff_errors=0`
  - switcher:
    - `frames_attempted=5`
    - `frames_rendered=5`
    - `render_failures=0`
    - `scheduler_status=AllSelected`
    - `slot_result_kinds=Selected|Selected|Selected|Selected`
    - `clean_output_render_result_kind=Rendered`
    - all slot diagnostics kept `parse_error=none`, `io_error=none`,
      `decode_error=none`
- Run `3`:
  - server:
    - `registered_clients=4`
    - `frames_reassembled=8`
    - `frames_queued=8`
    - `observed_reassembled_clients=4`
    - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
    - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
    - `receive_timed_out=false`
    - `max_packets_reached=false`
    - `handoff_errors=0`
  - switcher:
    - `frames_attempted=5`
    - `frames_rendered=5`
    - `render_failures=0`
    - `scheduler_status=AllSelected`
    - `slot_result_kinds=Selected|Selected|Selected|Selected`
    - `clean_output_render_result_kind=Rendered`
    - all slot diagnostics kept `parse_error=none`, `io_error=none`,
      `decode_error=none`

Observed variance across the three successful runs:

- `packets_received`, `fragments_received`, and `frame_payload_len` varied per
  run, which is expected for real capture/encode traffic
- no instability was observed in:
  - auth acceptance
  - client send success
  - server receive completion
  - reassembly completeness
  - named-pipe handoff
  - switcher parse/io/decode/render

Current conclusion after three consecutive guarded passes:

- the guarded `4`-real-slot recipe is now repeatable enough to treat as the
  operator baseline
- the next design step can move from basic feasibility to operator-facing
  control-surface planning without revisiting transport or stop-condition
  basics first

### Focused View Minimal Command

The first focused-view manual command is now implemented as:

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev [focused-slot-index] player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5
```

Implemented behavior for this first focused slice:

- `focused-slot-index` is validated as `0..3`
- the command reuses the existing guarded `4`-real-slot handoff path and keeps
  per-slot diagnostics visible
- stdout includes:
  - `view_state=Focused`
  - `focused_slot_index`
  - `focused_client_id`
  - `focused_run_id`
  - `focused_result_kind`
  - `scheduler_status`
  - `slot_result_kinds`
  - `slot_diagnostics`
  - `clean_output_render_result_kind`
  - `window_title=StreamSync 4-view Output`
  - `output_width=1280`
  - `output_height=720`

Chosen minimal no-frame behavior:

- if the focused slot has a renderable decoded frame, expect:
  - `focused_result_kind=Selected`
  - `clean_output_render_result_kind=Rendered`
- if the focused slot does not have a renderable decoded frame, this slice
  does not yet synthesize a dedicated full-window placeholder
- instead expect:
  - `clean_output_render_result_kind=NoRenderableFocusedView`
- parse / io / decode failures remain visible via `slot_diagnostics`

Next manual validation targets:

- `Focused(0)`
- `Focused(1)`
- `Focused(2)`
- `Focused(3)`
- confirm `frames_rendered > 0` and `render_failures=0` for renderable focused
  slots
- confirm `AllView -> Focused(slot_index) -> AllView` operator flow without
  changing the guarded server/client startup recipe

### Recorded Focused Actual Validation

Guarded startup recipe used for focused actual validation:

- server:
  `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 20 4096 5000 8 true 8388608 4 2`
- clients:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 2 16 1`

Recorded successful focused switcher runs:

- `Focused(0)`:
  `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  - `view_state=Focused`
  - `focused_slot_index=0`
  - `focused_client_id=player1`
  - `focused_result_kind=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
- `Focused(1)`:
  `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 1 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  - `view_state=Focused`
  - `focused_slot_index=1`
  - `focused_client_id=player2`
  - `focused_result_kind=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
- `Focused(2)` successful rerun:
  `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 2 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  - `view_state=Focused`
  - `focused_slot_index=2`
  - `focused_client_id=player3`
  - `focused_result_kind=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
- `Focused(3)` successful rerun:
  `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 3 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  - `view_state=Focused`
  - `focused_slot_index=3`
  - `focused_client_id=player4`
  - `focused_result_kind=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`

Across the successful focused sessions:

- server summary stayed at:
  - `registered_clients=4`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `receive_timed_out=false`
  - `max_packets_reached=false`
  - `handoff_errors=0`
- clients continued to show:
  - `frames_captured=2`
  - `frames_encoded=2`
  - `frames_sent=2`
  - `capture_failures=0`
  - `encode_failures=0`
  - `send_failures=0`
- slot diagnostics stayed at:
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`

Recorded transient issues during focused validation:

- first `Focused(2)` attempt ended with:
  - `focused_result_kind=Selected`
  - `clean_output_render_result_kind=Rendered`
  - but `frames_rendered=4` instead of `5`
  - rerun succeeded at `5/5`
- first `Focused(3)` attempt failed before a valid handoff session formed:
  - server stderr: `Handoff(CreatePipe(os_error_231))`
  - switcher stdout ended with:
    - `focused_result_kind=HandoffError`
    - `scheduler_status=HandoffError`
    - `clean_output_render_result_kind=NoRenderableFocusedView`
  - rerun after a short release delay succeeded

Current conclusion after focused actual validation:

- `Focused(0..3)` can be validated on the guarded `4`-client all-real session
  using the dedicated focused command
- successful focused sessions preserve:
  - `view_state=Focused`
  - slot-aligned `focused_client_id`
  - `focused_result_kind=Selected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
- observed failures were transient runtime/lifecycle issues, not protocol,
  decode, or scheduler regressions

### Recorded Operator-Flow Validation

The first operator-flow validation was recorded as nearby separate sessions,
not as one long-running in-process state machine. The validated flow shape was:

- `AllView -> Focused(0) -> AllView`
- `AllView -> Focused(1) -> AllView`
- `AllView -> Focused(2) -> AllView`
- `AllView -> Focused(3) -> AllView`

For this validation pass, a short release delay was kept between nearby
sessions before starting the next guarded server/client/switcher run.

Observed result:

- all `12` sessions completed successfully
- all `AllView` sessions reported:
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `frames_rendered=5`
  - `render_failures=0`
- all focused sessions reported:
  - `view_state=Focused`
  - slot-aligned `focused_slot_index`
  - slot-aligned `focused_client_id`
  - `focused_result_kind=Selected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
  - `frames_rendered=5`
  - `render_failures=0`
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`

Per-flow focused targets confirmed:

- `Focused(0)` -> `focused_client_id=player1`
- `Focused(1)` -> `focused_client_id=player2`
- `Focused(2)` -> `focused_client_id=player3`
- `Focused(3)` -> `focused_client_id=player4`

Transient wobble classification for this operator-flow validation:

- `frames_rendered < 5`: not observed
- server `CreatePipe(os_error_231)`: not observed
- switcher `HandoffError`: not observed
- named-pipe release delay: the nearby-session run stayed stable while keeping
  a short release delay between sessions, so this remains a practical
  mitigation note
- client capture / encode / send failure: not observed
- server receive timeout / incomplete reassembly: not observed
- switcher parse / io / decode / render error: not observed

Practical note for repeated nearby-session validation:

- when running many guarded sessions back-to-back, keep a short release delay
  between sessions before starting the next server-owned handoff session
- this validation used that approach and avoided the earlier transient
  `CreatePipe(os_error_231)` wobble

Current conclusion after operator-flow validation:

- nearby-session `AllView -> Focused(slot_index) -> AllView` operator flow is
  now validated
- keep this nearby-session command flow as the validated fallback/manual proof
  path
- keep the short release delay memo when many nearby sessions are run
  back-to-back
- do not treat this flow as the intended live operator surface:
  - it recreates session/window lifecycle per transition
  - it keeps command orchestration on the operator
  - it is sufficient for PoC/manual validation, but not a good live-switching
    shape
- the recommended next step is now a same-session long-running control loop
  with a minimal text command parser, before any hotkey/UI wrapper

Recommended next same-session control shape:

- the first same-session command now exists as:

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-controlled-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5 --commands "status;focus 0;status;focus 1;all;quit"
```

- control commands:
  - `all`
  - `focus 0`
  - `focus 1`
  - `focus 2`
  - `focus 3`
  - `status`
  - `quit`
- `max-ticks-per-command` is the bounded manual/scripted render count per
  accepted command
- `--commands "..."` is optional:
  - when present, commands are read from the semicolon-delimited script
  - when absent, commands are read from stdin line-by-line
- keep the underlying handoff/render path unchanged:
  - guarded `4`-client all-real recipe
  - named-pipe handoff
  - `StreamSync 4-view Output`
  - fixed `1280x720`
- expected stdout additions:
  - `current_view_state`
  - `requested_transition`
  - `transition_result`
  - `selected_slot_result`
  - `frames_rendered`
  - `render_failures`
  - `scheduler_status`
  - `clean_output_render_result_kind`
  - `command_index`
  - `command_parse_error`
  - `exit_reason`
- hotkey/UI wrapper remains later:
  - a thin wrapper over separate commands would still pay session/window churn
  - a thin wrapper over a same-session loop is the preferred later shape

Current implementation note:

- the same-session control loop is implemented
- actual guarded manual pass for this command is not yet recorded
- next manual target is one persistent session that proves:
  - `status -> focus 0 -> all -> quit`
  - `status -> focus 1 -> all -> quit`
  - `status -> focus 2 -> all -> quit`
  - `status -> focus 3 -> all -> quit`

---

## 1. Prerequisite Checks

Run these from the repository root before starting the manual E2E run.

### 1.1 FFmpeg

```powershell
ffmpeg -version
```

Pass:

- command exists
- output includes an FFmpeg version
- the build supports H.264 encoding with `libx264`

Fail diagnosis:

- `ffmpeg` not found: install FFmpeg or add it to `PATH`
- `libx264` missing: use an FFmpeg build with `libx264`

Optional encoder list check:

```powershell
ffmpeg -hide_banner -encoders
```

Look for `libx264`.

### 1.2 Workspace Builds

```powershell
cargo check --workspace
```

Pass:

- command exits successfully

### 1.3 Config Files

Confirm these files exist:

```powershell
Test-Path configs/examples/server.example.toml
Test-Path configs/examples/client.accepted.example.toml
Test-Path configs/manual/server.two-real-slots.toml
Test-Path configs/manual/client.player1.toml
Test-Path configs/manual/client.player2.toml
Test-Path configs/manual/client.player3.toml
Test-Path configs/manual/client.player4.toml
```

Pass:

- both commands print `True`

For two-client switcher verification, prepare a second client config with:

- `client_id = "player2"`
- matching `shared_token = "replace-with-shared-token-2"` for the server config
- same server host/port as the switcher/server runtime

### 1.4 UDP / Firewall

The manual runtimes use UDP. Before testing across machines:

- allow the configured UDP port through Windows Firewall
- confirm all clients target the same server/switcher address and port
- avoid another process already bound to the configured port

For same-machine testing, `127.0.0.1:5000` is the expected default shape.

---

## 2. One-Client Server Queue E2E

Use this first when validating capture/encode/auth/send/reassembly without the
switcher render path. This is the primary check for the previously observed
large frame case:

```text
last_send_payload_len=493150
last_send_packet_len=493245
PacketTooLarge
```

With sender fragmentation and server reassembly, a large encoded frame should
now be sent as `VideoFrameFragment` packets, reassembled by the server, and
queued as one `VideoFrame`.

### Terminal 1: Server Queue Launcher

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-once configs/examples/server.example.toml 4096 15000 1 true 8388608
```

For queue receive plus one named-pipe handoff in the same process, use:

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-and-serve-handoff-once configs/examples/server.example.toml streamsync-handoff 4096 15000 1 true 8388608
```

For queue receive plus bounded named-pipe handoff serving in the same process,
use:

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-and-serve-handoff-many configs/examples/server.example.toml streamsync-handoff 2 4096 15000 1 true 8388608
```

For these CLI commands, use a plain pipe name such as
`streamsync-handoff-dev`. In the latest localhost manual run, the plain name
succeeded, while a full Windows pipe path such as
`\\.\pipe\streamsync-handoff-dev` produced `SourceUnavailable` on the switcher
side.

Arguments after the config path are manual receive policy values:

- `4096`: max post-auth video packets to receive
- `15000`: idle receive timeout in milliseconds
- `1`: expected reassembled frame count
- `true`: stop after the expected reassembled frame count is reached
- `8388608`: requested UDP socket receive buffer size in bytes

If these arguments are omitted, the launcher uses the same defaults. For the
fragmented real encoded PoC, use `max_frames=1` or `2` on the client first so
the server can finish one frame before later frames add more incomplete state.
The OS may clamp the effective receive buffer; compare the requested and
effective stdout fields.

Expected server stdout shape:

- received one `AuthRequest`
- sent accepted `AuthResponse`
- accepted either one authenticated `VideoFrame` or multiple authenticated
  `VideoFrameFragment` packets
- if fragmented, reassembled one frame
- queued one frame for `client_id=player1`

Expected server stdout fields:

```text
auth_accepted=true
auth_reason=Ok
video=received
queued=queued
queue_len=1
registered_clients=1
manual_max_video_packets=4096
manual_receive_timeout_ms=15000
manual_expected_reassembled_frames=1
manual_stop_after_expected_reassembled_frames=true
manual_receive_buffer_requested_bytes=8388608
manual_receive_buffer_effective_bytes=<bytes|unknown>
manual_receive_buffer_set_error=none
manual_receive_buffer_read_error=none
packets_received=<n>
fragments_received=<n>
frames_reassembled=<n>
frames_queued=1
direct_frames_queued=<n>
rejected_packets=0
rejected_fragments=0
duplicate_fragments=0
incomplete_reassembly_frames=0
incomplete_frame_progress=none
receive_timed_out=false
max_packets_reached=false
```

When using `--receive-auth-video-queue-and-serve-handoff-once`, the same
process later prints one additional named-pipe handoff summary line with:

```text
pipe_name=<pipe>
request_id=<id>
client_id=<client>
run_id=<run>
read_mode=<inspect-oldest|inspect-latest|dequeue-oldest>
timeout_millis=<ms>
elapsed_millis=<ms>
request_status=decoded
response_status=written
result_kind=FrameRead|NoFrame|HandoffError
queue_len_before_read=<n>
queue_len_after_read=<n|none>
selected_client_id=<client>
selected_run_id=<run>
frame_id=<id|none>
frame_payload_len=<n|none>
no_frame_reason=<reason|none>
```

When using `--receive-auth-video-queue-and-serve-handoff-many`, the same
process first prints the normal receive/auth/video queue summary line, then
prints one aggregate bounded-loop summary line plus one per-request summary
line for each served request:

```text
server named-pipe handoff bounded pipe_name=<pipe> max_requests=<n> requests_served=<n> successful_responses=<n> handoff_errors=<n>
server named-pipe handoff bounded request pipe_name=<pipe> request_index=<n> request_id=<id> queue_len_before_read=<n> queue_len_after_read=<n|none> result_kind=FrameRead|NoFrame|HandoffError selected_client_id=<client> selected_run_id=<run> frame_id=<id|none> frame_payload_len=<n|none> no_frame_reason=<reason|none> handoff_error=<code|none>
```

Fragmented pass proof:

- `fragments_received > 1`
- `frames_reassembled >= 1`
- `frames_queued >= 1`
- `queue_len >= 1`
- `rejected_fragments = 0`
- `incomplete_reassembly_frames = 0`

Non-fragmented pass proof:

- `direct_frames_queued >= 1`
- `frames_queued >= 1`
- `queue_len >= 1`

The server launcher is bounded for manual verification. It is not a production
receive loop, does not retransmit, and does not implement fragment expiration.

### Terminal 3: One-Shot Named-Pipe Handoff Read

After the server has finished queueing and entered its one-shot handoff wait,
run one switcher-side pull/read over named pipe:

```powershell
cargo run -p stream-sync-switcher -- --read-queued-frame-handoff-once streamsync-handoff player1 streamsync-dev-session preview-latest 1
```

Use the same plain pipe name on both commands. Do not pass the full
`\\.\pipe\...` path to these CLI arguments in the current slice.

Expected switcher stdout fields:

```text
pipe_name=streamsync-handoff
request_id=1
client_id=player1
run_id=streamsync-dev-session
read_mode=inspect-latest
attempt_count=1
timeout_millis=<ms>
elapsed_millis=<ms>
request_status=sent
response_status=decoded
result_kind=FrameRead|NoFrame|HandoffError
final_result=FrameRead|NoFrame|HandoffError
last_error=<error|none>
retry_classification=RetryableLaterSchedulerTick|NonRetryable|none
handoff_response_kind=FrameRead|NoFrame|HandoffError|none
response_payload_len=<n|none>
parse_error=<detail|none>
io_error=<detail|none>
queue_len=<n|none>
```

If `FrameRead` is returned, stdout should also include:

- `frame_id`
- `capture_timestamp`
- `send_timestamp`
- `queued_at`
- `width`
- `height`
- `fps_nominal`
- `codec`
- `is_keyframe`
- `encoded_payload_len`

The command also accepts an omitted `request_id`; in that case the current
one-shot CLI uses the wrapper's initial monotonic value and consumes one id for
the process, which is effectively `1` for a fresh invocation.

### Terminal 2: Bounded Authenticated Client Sender

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-bounded configs/examples/client.accepted.example.toml 1 16 1
```

Arguments after the config path are:

- `1`: max frames to send
- `16`: fragment pacing interval; sleep after every 16 fragments
- `1`: fragment pacing delay in milliseconds

The bounded manual sender defaults to this conservative pacing. Use
`fragment-pacing-every=0` or `fragment-pacing-delay-ms=0` only when testing the
unpaced burst behavior.

Expected client stdout includes:

```text
accepted=true
reason_code=Ok
bounded_manual_runtime=true
fragment_pacing_every=16
fragment_pacing_delay_ms=1
frames_attempted=<n>
frames_captured=<n>
frames_encoded=<n>
frames_sent=<n>
direct_sends=<n>
fragmented_sends=<n>
fragments_attempted=<n>
fragments_sent=<n>
no_frame_count=<n>
capture_failures=0
encode_failures=0
send_failures=0
stop_reason=<reason>
```

One-client send pass:

- `accepted=true`
- `frames_sent >= 1`
- `frames_captured >= 1`
- `frames_encoded >= 1`
- server queued at least one frame

Fragmented send proof:

- `fragmented_sends >= 1`
- `fragments_attempted > 1`
- `fragments_sent = fragments_attempted`
- `send_failures = 0`
- `last_send_error=none`

If the encoded frame is small enough for one safe datagram, `direct_sends >= 1`
and `fragmented_sends = 0` is still a valid non-fragmented queue check.

Bounded sender can still pass with `no_frame_count > 0` as long as at least one
frame is captured/encoded/sent before the bounded runtime stops.

### Observed Successful Fragmented Queue Runs

The recommended server receive-buffer command remains:

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-once configs/examples/server.example.toml 4096 15000 1 true 8388608
```

Observed successful manual results on localhost:

- Fragmented real encoded queue PoC succeeded for both `max_frames=1` and
  `max_frames=2` after adding server UDP receive buffer tuning.
- The effective server receive buffer for the successful runs was
  `manual_receive_buffer_effective_bytes=8388608`.
- The latest recorded successful `max_frames=2` run used client fragment pacing
  `16 1` and produced the following observed summaries.

Server:

```text
receive auth/video queue runtime handled auth on 0.0.0.0:5000; auth_accepted=true auth_reason=Ok client_id=player1 run_id=streamsync-dev-session video=received queued=queued queue_len=2 dropped_oldest=false registered_clients=1 manual_max_video_packets=4096 manual_receive_timeout_ms=15000 manual_expected_reassembled_frames=2 manual_stop_after_expected_reassembled_frames=true manual_receive_buffer_requested_bytes=8388608 manual_receive_buffer_effective_bytes=8388608 manual_receive_buffer_set_error=none manual_receive_buffer_read_error=none packets_received=854 fragments_received=854 frames_reassembled=2 frames_queued=2 direct_frames_queued=0 rejected_packets=0 rejected_fragments=0 duplicate_fragments=0 non_video_packets=0 incomplete_reassembly_frames=0 incomplete_frame_progress=none receive_timed_out=false max_packets_reached=false
```

Client:

```text
auth real encoded video frame bounded PoC sent AuthRequest 96 bytes from 0.0.0.0:50542 to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; bounded_manual_runtime=true; fragment_pacing_every=16 fragment_pacing_delay_ms=1 frames_attempted=18 frames_captured=2 frames_encoded=2 frames_sent=2 direct_sends=0 fragmented_sends=2 fragments_attempted=854 fragments_sent=854 no_frame_count=16 capture_failures=0 encode_failures=0 frame_build_failures=0 send_failures=0 stop_reason=Some(MaxFramesReached) last_send_destination=none last_send_local_source=none last_send_frame_id=none last_send_payload_len=none last_send_packet_len=none last_send_error=none
```

Recorded conclusion from the successful `max_frames=2` run:

- auth succeeded
- client sent `854/854` fragments
- server received `854/854` fragments
- server reassembled `2` frames
- server queued `2` frames
- no incomplete reassembly remained
- receive timeout did not occur
- `8388608` effective UDP receive buffer was sufficient for the current
  localhost manual 1-frame and 2-frame fragmented real encoded queue PoC
- `frames_attempted=18` and `no_frame_count=16` remain capture-cadence
  diagnostics, not blockers for this PoC

For future reruns, treat the command/result pair above as the current known-good
fragmented queue baseline before moving on to switcher/sync-side queue
consumption.

### Observed Successful One-Shot Named-Pipe Handoff Run

Observed successful localhost results for the one-shot named-pipe handoff CLI:

- plain pipe name `streamsync-handoff-dev` succeeded
- full pipe path `\\.\pipe\streamsync-handoff-dev` produced
  `SourceUnavailable` in a separate manual attempt
- `request_id` matched between switcher request and server response
- `FrameRead` was returned without `NoFrame` or `HandoffError`
- frame metadata survived the server->switcher handoff unchanged
- `encoded_payload_len=383887` was non-zero and realistic for a real H.264
  frame
- `queue_len=1` matched on both sides

Switcher:

```text
switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest request_status=sent response_status=decoded result_kind=FrameRead queue_len=1 frame_id=2 capture_timestamp=1777641579940665 send_timestamp=1777641579940665 queued_at=1777641580062096 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=383887
```

Server:

```text
server named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest request_status=decoded response_status=written result_kind=FrameRead queue_len=1 frame_id=2 capture_timestamp=1777641579940665 send_timestamp=1777641579940665 queued_at=1777641580062096 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=383887
```

Recorded conclusion from this successful localhost run:

- named-pipe one-shot handoff succeeded
- switcher request was sent
- server decoded the request
- server returned `FrameRead`
- switcher decoded the response
- `request_id` matched
- `client_id` / `run_id` / `read_mode` matched
- `frame_id` matched
- `capture_timestamp` / `send_timestamp` / `queued_at` matched
- `width` / `height` / `fps_nominal` / `codec` / `is_keyframe` matched
- `encoded_payload_len` matched
- `queue_len` matched
- metadata survived server->switcher handoff
- no `NoFrame` or `HandoffError` occurred in the successful run

### Observed Successful Bounded Named-Pipe Handoff Loop Run

Observed successful localhost results for the bounded named-pipe handoff CLI:

- bounded server handoff loop served `max_requests=2`
- `requests_served=2`
- `successful_responses=2`
- `handoff_errors=0`
- both switcher reads returned `FrameRead`
- `request_id=1` and `request_id=2` were preserved end to end
- `inspect-latest` returned the same queued frame twice, which is expected for
  preview/read-only mode and confirms no queue mutation on repeated reads
- metadata survived the server->switcher handoff unchanged
- `encoded_payload_len=251482` was preserved and non-zero
- `elapsed_millis=1` remained visible on both switcher reads

Server:

```text
server named-pipe handoff bounded pipe_name=streamsync-handoff-dev max_requests=2 requests_served=2 successful_responses=2 handoff_errors=0
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=0 request_id=1 result_kind=FrameRead queue_len=1 handoff_error=none
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=1 request_id=2 result_kind=FrameRead queue_len=1 handoff_error=none
```

Client:

```text
auth real encoded video frame bounded PoC sent AuthRequest 96 bytes from 0.0.0.0:63648 to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; bounded_manual_runtime=true; fragment_pacing_every=16 fragment_pacing_delay_ms=1 frames_attempted=2 frames_captured=1 frames_encoded=1 frames_sent=1 direct_sends=0 fragmented_sends=1 fragments_attempted=246 fragments_sent=246 no_frame_count=1 capture_failures=0 encode_failures=0 frame_build_failures=0 send_failures=0 stop_reason=Some(MaxFramesReached) last_send_destination=none last_send_local_source=none last_send_frame_id=none last_send_payload_len=none last_send_packet_len=none last_send_error=none
```

Switcher read 1:

```text
switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead queue_len=1 frame_id=2 capture_timestamp=1777650284107351 send_timestamp=1777650284107351 queued_at=1777650284378630 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=251482
```

Switcher read 2:

```text
switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=2 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead queue_len=1 frame_id=2 capture_timestamp=1777650284107351 send_timestamp=1777650284107351 queued_at=1777650284378630 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=251482
```

Recorded conclusion from this successful bounded localhost run:

- client auth succeeded
- fragmented real encoded send succeeded
- server receive / reassembly / queue succeeded
- bounded named-pipe loop served two requests and respected `max_requests=2`
- both switcher reads decoded responses successfully
- no handoff errors occurred
- no error was collapsed into `NoFrame`
- repeated `inspect-latest` preserved preview semantics and returned the same
  frame twice without queue mutation

### Observed Successful Lifecycle-Summary Bounded Named-Pipe Rerun

Observed successful localhost results for the lifecycle-summary bounded rerun:

- bounded server handoff loop again served `max_requests=2`
- `requests_served=2`
- `successful_responses=2`
- `handoff_errors=0`
- both switcher reads returned `FrameRead`
- `attempt_count=1` was present on both switcher reads
- `final_result=FrameRead` was present on both switcher reads
- `last_error=none` was present on both switcher reads
- `retry_classification=none` was present on both switcher reads
- `request_id=1` and `request_id=2` were preserved end to end
- `inspect-latest` again returned the same queued frame twice, which is
  expected for preview/read-only mode and confirms no queue mutation on
  repeated reads
- metadata survived the server->switcher handoff unchanged
- `encoded_payload_len=246286` was preserved and non-zero
- classification-only lifecycle reporting appears sufficient for the successful
  path

Server:

```text
server named-pipe handoff bounded pipe_name=streamsync-handoff-dev max_requests=2 requests_served=2 successful_responses=2 handoff_errors=0
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=0 request_id=1 result_kind=FrameRead queue_len=1 handoff_error=none
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=1 request_id=2 result_kind=FrameRead queue_len=1 handoff_error=none
```

Client:

```text
auth real encoded video frame bounded PoC sent AuthRequest 96 bytes from 0.0.0.0:54387 to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; bounded_manual_runtime=true; fragment_pacing_every=16 fragment_pacing_delay_ms=1 frames_attempted=2 frames_captured=1 frames_encoded=1 frames_sent=1 direct_sends=0 fragmented_sends=1 fragments_attempted=241 fragments_sent=241 no_frame_count=1 capture_failures=0 encode_failures=0 frame_build_failures=0 send_failures=0 stop_reason=Some(MaxFramesReached) last_send_destination=none last_send_local_source=none last_send_frame_id=none last_send_payload_len=none last_send_packet_len=none last_send_error=none
```

Switcher read 1:

```text
switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest attempt_count=1 timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead final_result=FrameRead last_error=none retry_classification=none queue_len=1 frame_id=2 capture_timestamp=1777652932152093 send_timestamp=1777652932152093 queued_at=1777652932344643 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=246286
```

Switcher read 2:

```text
switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=2 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest attempt_count=1 timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead final_result=FrameRead last_error=none retry_classification=none queue_len=1 frame_id=2 capture_timestamp=1777652932152093 send_timestamp=1777652932152093 queued_at=1777652932344643 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=246286
```

Recorded conclusion from this successful lifecycle-summary bounded localhost
rerun:

- client auth succeeded
- fragmented real encoded send succeeded
- server receive / reassembly / queue succeeded
- bounded named-pipe loop served two requests and respected `max_requests=2`
- both switcher reads decoded responses successfully
- both reads returned `FrameRead`
- `attempt_count=1` was visible
- `final_result=FrameRead` was visible
- `last_error=none` was visible
- `retry_classification=none` was visible
- `request_id` 1 and 2 were preserved
- metadata survived the server->switcher handoff
- `encoded_payload_len=246286` was preserved and non-zero
- repeated `inspect-latest` preserved preview semantics and returned the same
  frame twice without queue mutation
- no error collapsed into `NoFrame`
- classification-only lifecycle reporting is sufficient for the current
  successful path

### Observed Successful Bounded Service-Session Localhost Run

Observed successful localhost results for the bounded service-session CLI:

- client auth succeeded
- fragmented real encoded send succeeded
- server receive/reassembly/queue succeeded
- bounded service session served `max_requests=2`
- `requests_served=2`
- `successful_responses=2`
- `handoff_errors=0`
- both switcher reads returned `FrameRead`
- `attempt_count=1` was visible
- `final_result=FrameRead` was visible
- `last_error=none` was visible
- `retry_classification=none` was visible
- `request_id=1` and `request_id=2` were preserved end to end
- metadata survived the server->switcher handoff unchanged
- `encoded_payload_len=263025` was preserved and non-zero
- repeated `inspect-latest` returned the same queued frame twice, which again
  confirms preview/read-only semantics and no queue mutation
- bounded service-session MVP appears complete enough to close the current
  transport/lifecycle phase

Server:

```text
server named-pipe handoff bounded pipe_name=streamsync-handoff-dev max_requests=2 requests_served=2 successful_responses=2 handoff_errors=0
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=0 request_id=1 result_kind=FrameRead queue_len=1 handoff_error=none
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=1 request_id=2 result_kind=FrameRead queue_len=1 handoff_error=none
```

Client:

```text
auth real encoded video frame bounded PoC sent AuthRequest 96 bytes from 0.0.0.0:61364 to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; bounded_manual_runtime=true; fragment_pacing_every=16 fragment_pacing_delay_ms=1 frames_attempted=2 frames_captured=1 frames_encoded=1 frames_sent=1 direct_sends=0 fragmented_sends=1 fragments_attempted=257 fragments_sent=257 no_frame_count=1 capture_failures=0 encode_failures=0 frame_build_failures=0 send_failures=0 stop_reason=Some(MaxFramesReached) last_send_destination=none last_send_local_source=none last_send_frame_id=none last_send_payload_len=none last_send_packet_len=none last_send_error=none
```

Switcher read 1:

```text
switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest attempt_count=1 timeout_millis=5000 elapsed_millis=2 request_status=sent response_status=decoded result_kind=FrameRead final_result=FrameRead last_error=none retry_classification=none queue_len=1 frame_id=2 capture_timestamp=1777670084106822 send_timestamp=1777670084106822 queued_at=1777670084284955 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=263025
```

Switcher read 2:

```text
switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=2 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest attempt_count=1 timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead final_result=FrameRead last_error=none retry_classification=none queue_len=1 frame_id=2 capture_timestamp=1777670084106822 send_timestamp=1777670084106822 queued_at=1777670084284955 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=263025
```

Recorded conclusion from this successful bounded service-session localhost run:

- client auth succeeded
- client fragmented real encoded send succeeded
- server receive/reassembly/queue succeeded
- bounded service session served 2 named-pipe requests
- `max_requests=2`
- `requests_served=2`
- `successful_responses=2`
- `handoff_errors=0`
- both switcher reads returned `FrameRead`
- `attempt_count=1`
- `final_result=FrameRead`
- `last_error=none`
- `retry_classification=none`
- `request_id` 1 and 2 were preserved
- metadata survived server->switcher handoff
- `encoded_payload_len=263025` was preserved and non-zero
- repeated `inspect-latest` returned the same frame without queue mutation
- no error collapsed into `NoFrame`
- bounded service-session MVP appears complete enough to close the current
  transport/lifecycle phase

---

## 3. Two-Client Live Switcher E2E

Use this after one-client server queue E2E passes.

The real encoded path should be validated through the server-mediated topology:

```text
client 1 bounded real capture/encode/send
client 2 bounded real capture/encode/send
  -> server auth / UDP receive / receive-buffer tuning
  -> server VideoFrameFragment reassembly
  -> server queue storage
  -> switcher queue read / targetTime scheduler
  -> H.264 decode
  -> display policy
  -> 2-view composition
  -> composed canvas render
```

This is the main path because real encoded H.264 frames normally require
fragmentation, and fragment reassembly is a server responsibility.

The older direct switcher receive command remains useful as a diagnostic path
for complete `VideoFrame` packets only:

```text
client 1 bounded real capture/encode/send
client 2 bounded real capture/encode/send
  -> live two-view switcher in-process server auth setup
  -> UDP source adapter
  -> server-style accepted frame queue storage
  -> shared targetTime selection
  -> H.264 decode
  -> 2-view composition
  -> composed canvas render
```

Current scope:

- proves two clients can authenticate against the switcher-owned manual runtime
- proves accepted client frames can enter switcher-owned caller-local queues
- proves the live two-view scheduler can select against one shared target time
- proves at least partial or full composed-canvas rendering can happen from
  queued real encoded frames

Current limitation:

- `--live-two-view-switcher-once` is diagnostic / legacy for direct client
  receive. It is not suitable for fragmented real encoded validation because it
  does not reassemble `VideoFrameFragment` packets.
- this command does not use `configs/examples/switcher.example.toml`.
- this command loads a server-style config such as
  `configs/examples/server.example.toml`, binds its `[server] bind_host` /
  `bind_port`, and uses its `[auth.clients.*] shared_token` values for
  AuthRequest validation.
- do not start `stream-sync-server` for this command. The switcher owns the UDP
  socket for this manual run; a separate server on the same address will
  conflict.
- `configs/examples/switcher.example.toml` contains switcher UI/server routing
  settings and no auth token material, so it is not valid input for
  `--live-two-view-switcher-once`.
- the current switcher UDP source accepts already complete authenticated
  `VideoFrame` packets. `VideoFrameFragment` packets are currently classified
  as non-video packets in this direct switcher path; fragment reassembly is
  available in the server queue PoC path, not in this manual live switcher
  source.
- do not add switcher-side fragment reassembly just for this command. The next
  minimal implementation slice should connect the server queue/reassembly path
  to the switcher targetTime/display/composition/render path.
- the existing `--live-two-view-switcher-once` runtime still uses the older
  live path: selection -> decode -> composition -> composed-canvas render.
- it does not yet route live manual traffic through the newer queue-backed
  scheduler decode/render adapter -> display policy -> display-composition
  adapter -> display-composition render connection chain.
- stale / held-previous display behavior remains covered by focused in-process
  tests, not by this manual two-client command.

Decision for this planning slice:

- Do not add a dedicated manual/runtime command for
  `SwitcherServerMediatedTwoViewValidationBoundary::run_fallible_*` before
  production server->switcher transport planning.
- The current focused tests already cover fallible eligible render, waiting,
  no-frame, handoff/source error, preview no-mutation, and consume
  all-or-nothing behavior.
- Keep `--live-two-view-switcher-once` as a direct receive diagnostic/legacy
  path only. Do not revive it as the main server-mediated validation path.
- The next step is transport planning around the existing fallible handoff
  contract, not a new manual command.

If a debug-only command is needed later, it should reuse the same auth and UDP
source setup but replace only the per-tick render pipeline after queue
storage:

```text
queue state
  -> SwitcherTwoViewTargetTimeSourceSchedulerBoundary
  -> SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary
  -> SwitcherTwoViewDisplayPolicyBoundary
  -> SwitcherTwoViewDisplayCompositionAdapterBoundary
  -> SwitcherTwoViewDisplayCompositionRenderConnectionBoundary
```

The smallest later command shape would be:

```text
--receive-auth-video-fallible-two-view-once [config-path] [left-client-id] [right-client-id]
```

That future diagnostic command should use the existing in-process
`ServerVideoFrameQueueState` produced by the server manual queue runtime for
happy-path validation. Synthetic/failing handoff sources should remain
test-only through injected `SwitcherQueuedFrameHandoff`.

That future diagnostic command should print, per side:

- scheduler status: selected / waiting / no-frame
- display decision: update / hold previous / stale placeholder / no-display
- composition instruction: updated / held previous / stale placeholder /
  no-display placeholder
- composition result: both / left-only / right-only / empty / invalid
- render result: rendered / deferred / backend unavailable / failed

Do not expand this manual path to 4-view or OBS until the 2-client path above
has a recorded pass.

Smallest next server-mediated validation slice:

```text
server queue output
  -> ServerVideoFrameQueueReadBoundary
  -> SwitcherSingleClientTargetTimeSourceBoundary
  -> SwitcherTwoViewTargetTimeSourceSchedulerBoundary
  -> SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary
  -> SwitcherTwoViewDisplayPolicyBoundary
  -> SwitcherTwoViewDisplayCompositionAdapterBoundary
  -> SwitcherTwoViewDisplayCompositionRenderConnectionBoundary
```

Use a pull/read handoff from server queue state for the next in-process
validation. Do not decide a production push transport yet.

Implemented diagnostic validation:

- `SwitcherServerMediatedTwoViewValidationBoundary` now performs this
  in-process wiring over caller-owned `ServerVideoFrameQueueState`.
- The boundary is covered by focused tests for both-selected render, waiting
  placeholder, no-frame placeholder, all-or-nothing consume, and preview
  no-mutation behavior.
- The fallible path is likewise covered by focused tests for eligible render,
  waiting, no-frame, source-error placeholder, both handoff errors,
  all-or-nothing consume, and preview no-mutation behavior.
- No manual command was added in this slice because it would duplicate focused
  in-process coverage without proving real server->switcher transport.
- Production server->switcher transport is still undecided.

### Terminal 1: Live Two-View Switcher Runtime

Start the switcher first. It binds `0.0.0.0:5000` when using
`configs/examples/server.example.toml`, receives client AuthRequest packets
directly, sends AuthResponse packets directly, then receives video packets from
the same authenticated client UDP sources. No separate server process is used.

```powershell
cargo run -p stream-sync-switcher -- --live-two-view-switcher-once configs/examples/server.example.toml player1 player2
```

Expected switcher stdout includes:

```text
bounded_manual_runtime=true
bind_address=0.0.0.0:5000
left_client_id=player1
right_client_id=player2
auth_packets_processed=<n>
auth_accepted=<n>
auth_rejected=0
auth_registered_clients=<n>
packets_processed=<n>
accepted_frames=<n>
rejected_frames=<n>
ticks_processed=<n>
rendered_both=<n>
rendered_partial=<n>
no_frame=<n>
decode_failed=<n>
render_not_completed=<n>
stop_reason=<reason>
```

The exact field names may vary slightly with the CLI summary, but these are the
counts to inspect.

### Terminal 2: Player 1 Client

The client config must target the switcher-owned manual socket. With the
default server example that means `server_host = "127.0.0.1"` and
`server_port = 5000`. The token must match
`[auth.clients.player1].shared_token` in `configs/examples/server.example.toml`.

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-bounded configs/examples/client.accepted.example.toml 5
```

Expected client proof:

- `accepted=true`
- `frames_attempted >= 1`
- `frames_captured >= 1`
- `frames_encoded >= 1`
- `frames_sent >= 1`

### Terminal 3: Player 2 Client

Use a second client config with `client_id = "player2"`,
`server_host = "127.0.0.1"`, `server_port = 5000`, and
`shared_token = "replace-with-shared-token-2"` to match
`[auth.clients.player2]` in `configs/examples/server.example.toml`.

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-bounded <player2-client-config.toml> 5
```

Expected client proof:

- `accepted=true`
- `frames_attempted >= 1`
- `frames_captured >= 1`
- `frames_encoded >= 1`
- `frames_sent >= 1`

Two-client live switcher pass:

- switcher reports two accepted auth registrations
- switcher reports `accepted_frames >= 2` across both clients
- switcher reports queued frames for both configured client ids
- switcher reports `ticks_processed >= 1`
- switcher reports at least one of:
  - `rendered_both >= 1`
  - `rendered_partial >= 1` with accepted/queued frames present
- decode/render failures are `0` for the strict pass case

Partial pass:

- both clients authenticate and send frames
- switcher queues frames
- switcher reaches `rendered_partial >= 1`
- one side may be missing because of timing/no-frame behavior
- this is enough to prove the queue/source/decode/composition/render path, but
  not enough to claim tight two-client sync

Fail:

- auth is rejected
- no frames are sent by either client
- switcher receives no accepted frames
- all accepted frames fail decode/render

### Latest Manual Result Review

2026-05-01 review status: successful one-shot named-pipe localhost handoff.

Current answers:

- client auth succeeded earlier in the same manual flow: consistent with the
  server queue-owning path, though the handoff stdout itself proves only the
  queued-frame read
- server served one named-pipe handoff request: proven
- switcher connected and sent one handoff request: proven
- `request_id` matched between request and response: proven
- switcher received `FrameRead`: proven
- frame metadata survived the server->switcher handoff: proven
- encoded payload length was non-zero and realistic: proven
- remaining queue length matched: proven
- no `NoFrame` or `HandoffError` occurred in the successful run
- operational guidance update: use a plain pipe name such as
  `streamsync-handoff-dev`; avoid passing the full `\\.\pipe\...` path in the
  current CLI slice

---

## 4. Expected Stdout Reading Guide

### Client

Auth accepted:

```text
accepted=true reason_code=Ok
```

Bounded runtime ran:

```text
bounded_manual_runtime=true
```

Useful counters:

```text
fragment_pacing_every=<n>
fragment_pacing_delay_ms=<n>
frames_attempted=<n>
frames_captured=<n>
frames_encoded=<n>
frames_sent=<n>
direct_sends=<n>
fragmented_sends=<n>
fragments_attempted=<n>
fragments_sent=<n>
no_frame_count=<n>
capture_failures=<n>
encode_failures=<n>
frame_build_failures=<n>
send_failures=<n>
stop_reason=<reason>
last_send_destination=<addr|none>
last_send_local_source=<addr|none>
last_send_frame_id=<id|none>
last_send_payload_len=<bytes|none>
last_send_packet_len=<bytes|none>
last_send_error=<error|none>
```

Interpretation:

- `frames_attempted > frames_captured` usually means no-frame polling happened.
- `no_frame_count > 0` is acceptable if `frames_sent >= 1`.
- `frames_captured > frames_encoded` points to encoder failure.
- `frames_encoded > frames_sent` points to frame build or UDP send failure.
- `fragmented_sends > 0` proves the sender used `VideoFrameFragment` packets.
- `fragments_sent = fragments_attempted` proves all planned fragments were sent by the client.
- `last_send_error=PacketTooLarge { ... }` after fragmentation support usually means a fragment packet still exceeded the conservative safe datagram limit, which should be treated as a bug or policy/config issue.
- `last_send_error=Send { kind: ..., message: ... }` preserves the OS `send_to` error kind and message.

### Server Queue / Reassembly

Fragment receive/reassembly proof:

```text
packets_received=<n>
fragments_received=<n>
frames_reassembled=<n>
frames_queued=<n>
rejected_packets=<n>
rejected_fragments=<n>
duplicate_fragments=<n>
incomplete_reassembly_frames=<n>
incomplete_frame_progress=<none|client/run/frame:received/expected:missing=n;...>
receive_timed_out=<bool>
max_packets_reached=<bool>
```

Interpretation:

- `fragments_received > 1` means the server received fragmented UDP packets.
- `frames_reassembled >= 1` means the server reconstructed one original encoded payload.
- `frames_queued >= 1` and `queue_len >= 1` mean the reassembled frame reached existing queue storage.
- `incomplete_frame_progress` lists caller-owned incomplete frames with
  received / expected / missing fragment counts.
- `receive_timed_out=true` with `incomplete_reassembly_frames > 0` means at least one fragment was missing and the manual launcher stopped waiting.
- `max_packets_reached=true` means the manual bounded receiver hit its packet guard before completion.

### Switcher

Auth/source proof:

```text
auth_accepted=<n>
registered_clients=<n>
accepted_frames=<n>
queued_frames=<n>
```

Scheduler/render proof:

```text
ticks_processed=<n>
rendered_both=<n>
rendered_partial=<n>
no_frame=<n>
decode_failed=<n>
render_failed=<n>
stop_reason=<reason>
```

Interpretation:

- `accepted_frames > 0` and `queued_frames > 0` prove UDP/auth/source/queue.
- `rendered_partial > 0` proves at least one side made it through decode/render.
- `rendered_both > 0` proves both configured clients reached the composed render path in the same scheduler run.

---

## 5. Failure Diagnosis

### Config Not Found

Symptoms:

- CLI exits before binding socket
- error includes an IO/path message

Checks:

```powershell
Test-Path <config-path>
```

Fix:

- run from repo root
- use an existing path such as `configs/examples/client.accepted.example.toml`

### FFmpeg Not Found

Symptoms:

- client reports `EncoderUnavailable`
- no encoded/sent frames

Checks:

```powershell
ffmpeg -version
```

Fix:

- install FFmpeg
- add FFmpeg directory to `PATH`
- restart the terminal

### Auth Rejected

Symptoms:

- client exits with `AuthRejected`
- stdout/stderr shows `accepted=false`
- switcher/server reports rejected auth

Checks:

- client `client_id` exists in server whitelist
- client `shared_token` matches server config or resolved secret
- `protocol_version` matches
- client sends to the same bind address/port used by server/switcher

Fix:

- use `configs/examples/client.accepted.example.toml` for `player1`
- create a matching `player2` config when testing two clients

### NoFrameAvailable

Symptoms:

```text
NoFrameAvailable { message: "Windows Graphics Capture frame pool had no queued frame" }
```

or bounded summary:

```text
no_frame_count>0
frames_captured=0
frames_sent=0
```

Meaning:

- Windows Graphics Capture session started, but the frame pool had no queued frame before the bounded runtime stopped.

Fix / retry:

- use bounded command, not one-shot command
- increase `max-frames` for the manual run
- make sure the captured display is active and changing
- retry after focusing/unminimizing the target display/window
- if still persistent, the next implementation target is OS event-driven frame-arrived wait

### Encode Failed

Symptoms:

- `encode_failures > 0`
- `frames_captured > 0`
- `frames_sent = 0`
- error includes `EncodeFailed`

Checks:

```powershell
ffmpeg -hide_banner -encoders
```

Fix:

- use FFmpeg with `libx264`
- verify capture dimensions are valid
- inspect future encoder stderr logging once production logging is implemented

### UDP / Firewall Issue

Symptoms:

- client auth receive timeout
- server/switcher receives no packets
- switcher reports no accepted frames even though client says it sent

Checks:

- server/switcher command is started before client
- client config destination matches server/switcher bind address and port
- no other process owns the port
- Windows Firewall allows UDP for the process/port
- same-machine test uses `127.0.0.1`

### Packet Too Large

Symptoms:

- client reports `send_failures > 0`
- `last_send_error` contains `PacketTooLarge`
- `last_send_payload_len` / `last_send_packet_len` are large

Meaning:

- capture and encode succeeded, but a packet still exceeded the current safe UDP
  datagram limit.
- Before fragmentation support, this happened for one full `VideoFrame`
  datagram. Now the expected large-frame path is fragmented; persistent
  `PacketTooLarge` means fragment sizing or future encoder output policy needs
  attention.

Fix:

- lower capture/encoder output size once production encoder config exists
- inspect `fragments_attempted`, `fragments_sent`, and `last_send_packet_len`
- treat persistent fragment-level `PacketTooLarge` as an implementation/config issue

### Fragmented Packets Not Received

Symptoms:

- client reports `fragmented_sends >= 1` and `fragments_sent > 1`
- server reports `packets_received=0` or `fragments_received=0`
- server may eventually show `receive_timed_out=true`

Checks:

- server queue launcher was started before the client
- client config destination matches server bind address/port
- same-source auth succeeded before video fragments were sent
- Windows Firewall allows UDP on the configured port

### Incomplete Reassembly

Symptoms:

- server reports `fragments_received > 0`
- server reports `frames_reassembled=0`
- server reports `incomplete_reassembly_frames > 0`
- `receive_timed_out=true` or `max_packets_reached=true`

Meaning:

- at least one fragment for a frame was not received by the server.
- the current slice has no retransmit/retry and no fragment expiration policy.

Fix / retry:

- rerun the manual check on localhost first
- keep the client bounded to `max_frames=1` or `2` while proving reassembly
- use the server manual policy defaults or raise them explicitly, for example
  `--receive-auth-video-queue-once configs/examples/server.example.toml 8192 30000 1 true 8388608`
- compare `manual_receive_buffer_requested_bytes` and
  `manual_receive_buffer_effective_bytes`; if `set_error` or `read_error` is
  not `none`, the server continued without confirmed socket buffer tuning
- use client fragment pacing, for example
  `--auth-real-encoded-video-frame-poc-bounded configs/examples/client.accepted.example.toml 1 16 1`
- inspect `incomplete_frame_progress`; values like
  `player1/streamsync-dev-session/1:180/289:missing=109` indicate the nearest
  tracked frame's received / expected fragment count and missing count
- reduce network loss/firewall interference
- increase sender stability before testing across machines
- keep retransmit/retry as a future task, not a manual workaround

### Queue Not Updated

Symptoms:

- server reports `frames_reassembled >= 1`
- server reports `frames_queued=0` or `queue_len=0`

Checks:

- `queued=queued` should be present for pass
- if `queued=not_queued_storage_dropped`, inspect queue capacity policy
- if `rejected_fragments > 0`, inspect auth/source mismatch or metadata rejection

### Decode / Render Failed

Symptoms:

- switcher reports accepted/queued frames
- `decode_failed > 0` or render failure count > 0
- no `rendered_both` / no `rendered_partial`

Meaning:

- UDP/auth/source/queue path worked, but H.264 decode or window render failed.

Checks:

- FFmpeg exists for switcher decode path
- server queue path reports `frames_queued >= 1`
- run on Windows for window render path
- verify fragmented path completed with `incomplete_reassembly_frames=0`

---

## 6. Clear Pass / Fail Criteria

### One-Client Real Encoded Send Pass

Pass when all are true:

- client auth accepted
- client `frames_captured >= 1`
- client `frames_encoded >= 1`
- client `frames_sent >= 1`
- for fragmented large frames, client `fragmented_sends >= 1`
- for fragmented large frames, server `fragments_received > 1`
- server queue launcher reports `frames_reassembled >= 1` or `direct_frames_queued >= 1`
- server queue launcher reports `frames_queued >= 1`
- server queue launcher reports `queue_len >= 1`

Fail when any are true:

- config cannot load
- auth rejected
- `frames_sent = 0`
- client reports `fragmented_sends >= 1` but server `fragments_received=0`
- server `incomplete_reassembly_frames > 0`
- server receives packets but `frames_queued=0`

### Two-Client Live Switcher Pass

Pass when all are true:

- switcher accepts/registers both `player1` and `player2`
- both clients report `frames_sent >= 1`
- switcher reports accepted/queued frames for both clients
- switcher reports `rendered_both >= 1`
- decode/render failure counts are zero

Acceptable manual partial pass:

- both clients authenticate and send frames
- switcher accepts/queues frames
- switcher reports `rendered_partial >= 1`
- no persistent auth or UDP issue is present

Fail when any are true:

- either client auth is rejected
- either client sends zero frames after bounded retries
- switcher accepts zero frames
- switcher queues frames but all decode/render attempts fail

---

## 7. Known Limitations

- Primary display only.
- Frame-arrived wait is bounded and polling-style; no OS event-driven continuous acquisition loop yet.
- Sender-side packet fragmentation and server-side reassembly exist for manual verification.
- No fragment retransmit/retry yet.
- No fragment expiration policy yet; incomplete reassembly remains caller-owned state during the manual run.
- Live two-view switcher runtime is bounded/manual, not a production loop.
- No late-frame queue mutation/drop policy yet.
- No production H.264 encoder configuration or structured encoder stderr logging yet.
- No OBS integration.
- No 4-view sync.
