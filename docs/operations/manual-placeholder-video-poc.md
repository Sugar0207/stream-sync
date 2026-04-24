# Manual Placeholder VideoFrame PoC

This note records the current manual verification shape for the one-client
placeholder `VideoFrame` path:

```text
client placeholder VideoFrame -> server receive/auth gate -> server video queue -> switcher placeholder selection
```

The current implementation has the library boundaries for each step, plus a
client launcher that authenticates and sends the placeholder frame from the
same UDP source, and a server launcher that owns the authenticated registry and
video queue for one auth packet followed by one video packet.

## Implemented Boundaries

- Client can construct one placeholder `VideoFrame` and send it with:

  ```powershell
  cargo run -p stream-sync-client -- --placeholder-video-frame-poc-once configs/examples/client.accepted.example.toml
  ```

- Client can authenticate and then send one placeholder `VideoFrame` from the
  same UDP socket/source with:

  ```powershell
  cargo run -p stream-sync-client -- --auth-placeholder-video-frame-poc-once configs/examples/client.accepted.example.toml
  ```

- Server can authenticate one client, keep the accepted sender registry alive,
  receive one following packet through the normal acceptance gate, and queue an
  accepted `VideoFrame` with:

  ```powershell
  cargo run -p stream-sync-server -- --receive-auth-video-queue-once configs/examples/server.example.toml
  ```

- Server can store an accepted authenticated `VideoFrame` side effect into
  caller-owned `ServerVideoFrameQueueState` through
  `ServerVideoFrameQueueRuntimeBoundary::store_from_receive_side_effect`.
- Switcher can borrow `ServerVideoFrameQueueState`, select one client's latest
  queued encoded frame, and create a decode-deferred placeholder display
  handoff.

## Manual Command Sequence

Terminal 1:

```powershell
cargo run -p stream-sync-server -- --receive-auth-video-queue-once configs/examples/server.example.toml
```

Terminal 2:

```powershell
cargo run -p stream-sync-client -- --auth-placeholder-video-frame-poc-once configs/examples/client.accepted.example.toml
```

Expected server stdout includes:

- `auth_accepted=true`
- `video=received`
- `queued=queued`
- `queue_len=1`
- `dropped_oldest=false`
- `registered_clients=1`

Expected client stdout includes the `AuthRequest` byte count, accepted
`AuthResponse`, `VideoFrame` byte count, `same_source=true`, and
`placeholder_payload=true`.

## Remaining Manual Limitation

`--placeholder-video-frame-poc-once` sends a `VideoFrame` only. It does not
authenticate first and it does not reuse a socket from an earlier auth command.

The server acceptance gate intentionally requires `VideoFrame` packets to come
from a source address already registered by an accepted `AuthRequest`.
Running the existing client auth CLI and then running
`--placeholder-video-frame-poc-once` is not enough, because those commands bind
separate UDP sockets and normally use different source ports.

Result:

- A standalone placeholder `VideoFrame` send verifies client-side
  metadata/payload/encode/send behavior.
- It should be treated as rejected/not queued by the authenticated server path
  unless the same UDP source has already been registered.
- Full manual client-to-server queue verification now has the required client
  sender shape and server queue-owning receiver shape.
- There is still no switcher CLI or shared runtime state bridge that reads the
  server-owned in-process queue after this manual launcher exits.

## Server-Side Verification Available Now

Use the server unit tests to verify the receive side-effect to queue boundary:

```powershell
cargo test -p stream-sync-server video_frame_queue
cargo test -p stream-sync-server receive_auth_video_queue_once
```

This covers:

- accepted authenticated `VideoFrame` side effects entering the correct
  per-client queue
- rejected / unauthenticated `VideoFrame` side effects staying out of the queue
- drop-oldest behavior when the per-client queue is full
- the manual auth-then-video receiver preserving the auth registry and packet
  acceptance gate before queue insertion

## Client Send Verification Available Now

Run the video-only client one-shot launcher:

```powershell
cargo run -p stream-sync-client -- --placeholder-video-frame-poc-once configs/examples/client.accepted.example.toml
```

Expected stdout includes:

- destination
- `client_id`
- `run_id`
- `frame_id`
- capture/send timestamps
- 1280x720 / 30 fps placeholder metadata
- payload length
- `placeholder_payload=true`

This does not prove server queue insertion by itself.

Run the same-socket auth + placeholder video launcher with the server
`--receive-auth-video-queue-once` path:

```powershell
cargo run -p stream-sync-client -- --auth-placeholder-video-frame-poc-once configs/examples/client.accepted.example.toml
```

Expected stdout includes the `AuthRequest` byte count, local UDP source,
`AuthResponse` source, accepted reason, `VideoFrame` byte count, frame metadata,
payload length, `same_source=true`, and `placeholder_payload=true`.

## Switcher Placeholder Verification Available Now

Use the switcher unit tests to verify latest-frame selection and the
decode-deferred display handoff:

```powershell
cargo test -p stream-sync-switcher
```

There is no switcher CLI or shared runtime state bridge yet. The switcher path
is currently a callable library boundary over an in-process
`ServerVideoFrameQueueState`.

## Minimal Missing Manual Wiring

The smallest useful next wiring after this step is:

1. An optional switcher-side helper or test fixture that reads that in-process
   queue state and runs the placeholder selection boundary.
2. Later replacement of the explicit placeholder payload source with real
   capture / H.264 encode.

The manual client-to-server queue path is now runnable as a two-command PoC.
Decode, rendering, switcher window output, OBS capture, and 4-view sync remain
outside this manual launcher.
