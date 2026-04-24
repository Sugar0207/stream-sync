# Manual Placeholder VideoFrame PoC

This note records the current manual verification shape for the one-client
placeholder `VideoFrame` path:

```text
client placeholder VideoFrame -> server receive/auth gate -> server video queue -> switcher placeholder selection
```

The current implementation has the library boundaries for each step, plus a
client launcher that authenticates and sends the placeholder frame from the
same UDP source. A queue-owning server manual launcher is still needed before
the full end-to-end command sequence can be run with only project CLIs.

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

- Server can store an accepted authenticated `VideoFrame` side effect into
  caller-owned `ServerVideoFrameQueueState` through
  `ServerVideoFrameQueueRuntimeBoundary::store_from_receive_side_effect`.
- Switcher can borrow `ServerVideoFrameQueueState`, select one client's latest
  queued encoded frame, and create a decode-deferred placeholder display
  handoff.

## Current Manual Limitation

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
  sender shape, but it still needs a server launcher that keeps one
  authenticated registry and one `ServerVideoFrameQueueState` alive while it
  receives `AuthRequest` followed by `VideoFrame`.

## Server-Side Verification Available Now

Use the server unit tests to verify the receive side-effect to queue boundary:

```powershell
cargo test -p stream-sync-server video_frame_queue
```

This covers:

- accepted authenticated `VideoFrame` side effects entering the correct
  per-client queue
- rejected / unauthenticated `VideoFrame` side effects staying out of the queue
- drop-oldest behavior when the per-client queue is full

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

Run the same-socket auth + placeholder video launcher when paired with a server
path that expects `AuthRequest` followed by `VideoFrame`:

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

The smallest useful next wiring for full manual verification is:

1. A server one-shot receiver/queue launcher that owns the authenticated
   registry and `ServerVideoFrameQueueState` for an auth packet followed by one
   video packet, then prints whether the frame was queued.
2. An optional switcher-side helper or test fixture that reads that in-process
   queue state and runs the placeholder selection boundary.

Until those pieces exist, the manual PoC is documented as verified
library/CLI slices rather than one complete end-to-end command sequence.
