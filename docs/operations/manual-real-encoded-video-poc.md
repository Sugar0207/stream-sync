# Manual Real Encoded VideoFrame PoC

This note records the current manual verification shape for the one-client real
encoded `VideoFrame` path:

```text
ready capture session -> one BGRA frame -> FFmpeg H.264 encode -> RealCaptureH264 VideoFrame -> UDP send
```

This is a one-shot sender only. It is not continuous streaming, switcher
decode/rendering, OBS integration, or 4-view sync.

## Implemented Boundary

Client can attempt one real encoded `VideoFrame` send with:

```powershell
cargo run -p stream-sync-client -- --real-encoded-video-frame-poc-once configs/examples/client.accepted.example.toml
```

The launcher:

- reads the existing client example config for destination, protocol version,
  client id, and run id
- uses Windows Graphics Capture primary display as the initial manual target
- creates one capture session runtime when Windows Graphics Capture is
  available
- starts capture if needed, attempts one frame acquisition, and does not wait
  for a future frame event
- encodes one BGRA frame with `ClientFfmpegSoftwareH264EncoderRuntimeHook`
- sends one existing protocol `VideoFrame` with `source_kind=RealCaptureH264`

## Requirements

- Windows with Windows Graphics Capture support
- permission for screen capture if the OS requires it
- `ffmpeg` on `PATH`
- FFmpeg build with `libx264`
- a server or UDP receiver listening at the configured destination

On non-Windows, the command is expected to fail explicitly with
`BackendUnsupported`.

## Expected Success Output

Successful client stdout includes:

- sent byte count
- destination
- frame id
- capture timestamp
- width / height / nominal FPS
- encoded payload length
- `source_kind=RealCaptureH264`

Example shape:

```text
real encoded video frame PoC sent <bytes> bytes to 127.0.0.1:5000; frame_id=1 capture_timestamp=<micros> width=<w> height=<h> fps_nominal=30 payload_len=<h264-bytes> source_kind=RealCaptureH264
```

## Explicit Failure Output

The CLI exits non-zero and prints an explicit reason when it cannot send:

- capture session config not prepared
- capture session not created
- capture unavailable
- no frame available
- encode unavailable / failed
- frame build failed
- UDP send failed

Common expected early failures:

- `BackendUnsupported`: running on non-Windows
- `RuntimeUnavailable`: Windows Graphics Capture runtime unavailable
- `PermissionUnavailable`: capture permission unavailable
- `EncoderUnavailable`: `ffmpeg` is not on `PATH`
- `EncodeFailed`: FFmpeg/libx264 failed or produced empty output

## Manual Receiver Options

For low-level client send verification, a UDP receiver or the existing server
manual receiver can observe the packet.

The authenticated server queue path requires an accepted `AuthRequest` from the
same UDP source before it accepts `VideoFrame` packets. The video-only launcher
proves client-side capture/encode/metadata/send behavior, but does not by itself
prove server queue insertion.

Use the authenticated same-source real encoded launcher when the current goal is
to verify the server auth gate and real encoded queue insertion together:

Terminal 1:

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-once configs/examples/server.example.toml
```

Terminal 2:

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-once configs/examples/client.accepted.example.toml
```

The authenticated launcher:

- binds one UDP socket
- sends `AuthRequest`
- receives `AuthResponse`
- requires `accepted=true`
- creates the capture session after auth succeeds
- acquires one BGRA frame
- encodes it with FFmpeg H.264
- sends one `RealCaptureH264` `VideoFrame` from the same UDP source

Successful client stdout includes:

- auth request byte count
- local source address
- destination
- auth response byte count and source
- `same_source=true`
- frame id
- capture timestamp
- width / height / nominal FPS
- encoded payload length
- `source_kind=RealCaptureH264`

Example shape:

```text
auth real encoded video frame PoC sent AuthRequest <bytes> bytes from 127.0.0.1:<port> to 127.0.0.1:5000 and received AuthResponse <bytes> bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; sent VideoFrame <bytes> bytes from same_source=true; frame_id=1 capture_timestamp=<micros> width=<w> height=<h> fps_nominal=30 payload_len=<h264-bytes> source_kind=RealCaptureH264
```

The command exits non-zero and prints an explicit reason for auth rejection,
auth timeout/receive failure, capture/session failure, no frame available,
encode unavailable/failed, or UDP send failure.

## Live Two-View Switcher Manual Runtime

The switcher can now own the bounded auth registry setup and then run the
existing live two-view scheduling path:

```text
client AuthRequest -> switcher/server auth response step -> registry
client VideoFrame -> UDP-backed source -> scheduler -> targetTime selection -> decode -> composition -> window render
```

Terminal 1:

```powershell
cargo run -p stream-sync-switcher -- --live-two-view-switcher-once configs/examples/server.example.toml player1 player2
```

Terminal 2:

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-once configs/examples/client.accepted.example.toml
```

Terminal 3 should use a second client config with `client_id = "player2"` and
the matching `shared_token = "replace-with-shared-token-2"`, then run:

```powershell
cargo run -p stream-sync-client -- --auth-real-encoded-video-frame-poc-once <player2-client-config.toml>
```

Expected switcher stdout includes:

- `bounded_manual_runtime=true`
- bind address and left/right client ids
- auth packets processed / accepted / rejected / registered client counts
- packets processed, accepted frames, rejected frames, decode failures, and timeouts
- ticks processed
- rendered-both / rendered-partial / no-frame / decode-failed / render-not-completed counts
- stop reason

This proves the switcher process can create the auth registry, keep packet
acceptance active, feed accepted `VideoFrame` packets through the UDP-backed
source adapter, and run the existing two-view scheduler. It is still bounded:
the default manual runtime expects two auth setup packets, then consumes a
small bounded number of video/source packets and scheduler ticks.

## Current Limitations

- primary display only
- no continuous acquisition or frame-arrived wait
- no packet fragmentation; large H.264 payloads may fail UDP send
- live two-view switcher runtime is bounded/manual, not a production loop
- no OBS integration
- no 4-view sync
