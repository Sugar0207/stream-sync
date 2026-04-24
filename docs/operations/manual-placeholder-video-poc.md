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
- Switcher can run a fixture-backed manual helper that feeds a caller-owned
  `ServerVideoFrameQueueState` into the same selection / placeholder handoff
  boundary and prints a compact summary.
- Switcher can run an in-process bridge launcher that owns the server
  auth-then-video queue path in the same process, then passes the returned
  caller-owned queue state into the switcher placeholder helper.

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

Use the switcher unit tests to verify latest-frame selection, queue-to-switcher
placeholder helper behavior, and the decode-deferred display handoff:

```powershell
cargo test -p stream-sync-switcher
```

Use the fixture-backed switcher CLI helper to verify the current
queue-to-switcher placeholder handoff without a running server process:

```powershell
cargo run -p stream-sync-switcher -- --placeholder-fixture-once client-1
```

Expected stdout includes:

- `fixture_queue=true`
- `cross_process_queue=false`
- `no_frame=false`
- `selected_client_id=client-1`
- `frame_id=42`
- `payload_len=3`
- `decode_status=DeferredPlaceholder`

Use the empty-queue helper to verify the no-frame summary path:

```powershell
cargo run -p stream-sync-switcher -- --placeholder-empty-once client-1
```

Expected stdout includes:

- `fixture_queue=true`
- `cross_process_queue=false`
- `no_frame=true`
- `selected_client_id=client-1`
- `frame_id=none`
- `payload_len=none`
- `decode_status=none`

The switcher helper consumes a caller-owned / fixture
`ServerVideoFrameQueueState`. It does not read the server manual launcher's
in-memory queue across process boundaries.

## In-Process Bridge Verification Available Now

The switcher-owned in-process bridge can be run with:

```powershell
cargo run -p stream-sync-switcher -- --receive-auth-video-placeholder-bridge-once configs/examples/server.example.toml client-1
```

Then run the same-socket client sender in another terminal:

```powershell
cargo run -p stream-sync-client -- --auth-placeholder-video-frame-poc-once configs/examples/client.accepted.example.toml
```

Expected switcher bridge stdout combines server queue state and switcher
placeholder handoff summary:

- `in_process=true`
- `cross_process_queue=false`
- auth accepted / rejected
- video received / accepted / rejected
- queued / not queued
- queue length / dropped-oldest
- selected client id
- selected frame id
- payload length
- `decode_status=DeferredPlaceholder`
- no-frame state when the queue is empty or no accepted frame was queued

The bridge still does not share queue state with a separate server process. It
calls the server auth-then-video queue launcher in-process and consumes the
returned caller-owned `ServerVideoFrameQueueState`.

The manual client-to-server queue path is now runnable as a two-command PoC.
Decode, rendering, switcher window output, OBS capture, cross-process queue
sharing, and 4-view sync remain outside this manual launcher.
