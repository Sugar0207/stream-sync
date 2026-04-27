# Manual Real Encoded VideoFrame E2E Checklist

This checklist verifies the current real encoded video path:

```text
Windows Graphics Capture -> BGRA frame -> FFmpeg H.264 -> RealCaptureH264 VideoFrame -> direct or fragmented UDP -> server auth gate -> reassembly -> queue/source -> switcher decode/render
```

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
cargo run -p stream-sync-server -- --receive-auth-video-queue-once configs/examples/server.example.toml
```

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
packets_received=<n>
fragments_received=<n>
frames_reassembled=<n>
frames_queued=1
direct_frames_queued=<n>
rejected_packets=0
rejected_fragments=0
duplicate_fragments=0
incomplete_reassembly_frames=0
receive_timed_out=false
max_packets_reached=false
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

### Terminal 2: Bounded Authenticated Client Sender

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-bounded configs/examples/client.accepted.example.toml 5
```

Expected client stdout includes:

```text
accepted=true
reason_code=Ok
bounded_manual_runtime=true
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

---

## 3. Two-Client Live Switcher E2E

Use this after one-client server queue E2E passes.

### Terminal 1: Live Two-View Switcher Runtime

```powershell
cargo run -p stream-sync-switcher -- --live-two-view-switcher-once configs/examples/server.example.toml player1 player2
```

Expected switcher stdout includes:

```text
bounded_manual_runtime=true
left_client_id=player1
right_client_id=player2
auth_processed=<n>
auth_accepted=<n>
auth_rejected=0
registered_clients=<n>
packets_processed=<n>
accepted_frames=<n>
rejected_frames=<n>
queued_frames=<n>
ticks_processed=<n>
rendered_both=<n>
rendered_partial=<n>
no_frame=<n>
decode_failed=<n>
render_failed=<n>
stop_reason=<reason>
```

The exact field names may vary slightly with the CLI summary, but these are the
counts to inspect.

### Terminal 2: Player 1 Client

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

Use a second client config with `client_id = "player2"` and matching token.

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
- switcher reports at least one of:
  - `rendered_both >= 1`
  - `rendered_partial >= 1` with accepted/queued frames present
- decode/render failures are `0` for the strict pass case

Partial pass:

- both clients authenticate and send frames
- switcher queues frames
- switcher reaches `rendered_partial >= 1`
- one side may be missing because of timing/no-frame behavior

Fail:

- auth is rejected
- no frames are sent by either client
- switcher receives no accepted frames
- all accepted frames fail decode/render

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
receive_timed_out=<bool>
max_packets_reached=<bool>
```

Interpretation:

- `fragments_received > 1` means the server received fragmented UDP packets.
- `frames_reassembled >= 1` means the server reconstructed one original encoded payload.
- `frames_queued >= 1` and `queue_len >= 1` mean the reassembled frame reached existing queue storage.
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
