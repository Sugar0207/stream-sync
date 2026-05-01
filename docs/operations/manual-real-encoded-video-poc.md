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
request_status=decoded
response_status=written
result_kind=FrameRead|NoFrame|HandoffError
queue_len=<n|none>
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

Expected switcher stdout fields:

```text
pipe_name=streamsync-handoff
request_id=1
client_id=player1
run_id=streamsync-dev-session
read_mode=inspect-latest
request_status=sent
response_status=decoded
result_kind=FrameRead|NoFrame|HandoffError
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

2026-04-30 review status: inconclusive.

The submitted stdout blocks for the switcher, client 1, and client 2 contained
only `...`, so there were no counters or status lines available to verify.

Current answers:

- both clients auth successfully: not proven
- both clients send real encoded frames: not proven
- switcher receive / reassembly / queue for both clients: not proven
- shared targetTime selection from both clients: not proven
- H.264 decode for both selected frames: not proven
- 2-view composition produced a composed frame: not proven
- composed canvas render succeeded: not proven
- waiting / no-frame / stale-like cases: not observable from the submitted text
- next action: rerun the same manual validation and record the real stdout
  counters before adding a new display-policy-chain diagnostic command

No code fix is indicated by this review because no concrete failure output was
provided.

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
