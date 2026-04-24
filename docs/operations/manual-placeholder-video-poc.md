# Manual Placeholder VideoFrame PoC

This note records the current manual verification shape for the one-client
placeholder `VideoFrame` path:

```text
client placeholder VideoFrame -> server receive/auth gate -> server video queue -> switcher placeholder selection
```

The current implementation has the library boundaries for each step, but the
full manual end-to-end command sequence is not complete yet.

## Implemented Boundaries

- Client can construct one placeholder `VideoFrame` and send it with:

  ```powershell
  cargo run -p stream-sync-client -- --placeholder-video-frame-poc-once configs/examples/client.accepted.example.toml
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
- Full manual client-to-server queue verification still needs a same-socket
  auth-then-placeholder-video launcher or an equivalent scripted harness.

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

Run the client one-shot launcher:

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

1. A client one-shot launcher that keeps one UDP socket open, sends
   `AuthRequest`, waits for accepted `AuthResponse`, then sends one placeholder
   `VideoFrame` from the same source.
2. A server one-shot receiver/queue launcher that owns the authenticated
   registry and `ServerVideoFrameQueueState` for an auth packet followed by one
   video packet, then prints whether the frame was queued.
3. An optional switcher-side helper or test fixture that reads that in-process
   queue state and runs the placeholder selection boundary.

Until those pieces exist, the manual PoC is documented as three verified
library/CLI slices rather than one complete end-to-end command sequence.
