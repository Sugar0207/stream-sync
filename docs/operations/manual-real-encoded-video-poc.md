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

The authenticated server queue path still requires an accepted `AuthRequest`
from the same UDP source before it accepts `VideoFrame` packets. This real
encoded standalone launcher currently sends only the video packet, so it proves
client-side capture/encode/metadata/send behavior but does not by itself prove
server queue insertion.

Use the placeholder same-socket auth + video path when the current goal is to
verify server auth gate and queue insertion.

## Current Limitations

- primary display only
- no auth + real video same-socket launcher yet
- no continuous acquisition or frame-arrived wait
- no packet fragmentation; large H.264 payloads may fail UDP send
- no real decode/rendering in switcher
- no OBS integration
- no 4-view sync
