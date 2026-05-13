<!-- stream-sync/docs/operations/distributed-pc-validation.md -->

# Distributed-PC Validation Planning

## Status
- This document is the next-step source of truth after:
  - same-PC `4`-client all-real functional PASS:
    - `manual-logs/four-client-20260513-184503`
  - same-PC OBS `Window Capture` PASS:
    - `manual-logs/obs-capture-20260513-190909`
- The existing checkpoints remain fixed:
  - same-PC `4`-client all-real functional result stays `PASS`
  - OBS capture visibility result stays `PASS`
- The latest OBS run also recorded a same-PC runtime `PARTIAL`:
  - `client2` / `client3` hit `EncodeFailure`
  - client effective output FPS fell into the `16-18fps` band
  - switcher perceived FPS was very low
  - switcher final summary was not collected because the switcher did not exit
    within `360` seconds
- Those same-PC saturation symptoms remain follow-up items, but they are not a
  blocker for planning the distributed-PC phase.
- This step is docs-first only:
  - no code change
  - no distributed-PC actual run yet
  - no protocol or architecture change

## Validation Purpose

The purpose of the distributed-PC phase is narrow:

- move at least part of the client capture/encode load off the streaming PC
- keep `server + switcher + OBS` on the streaming PC
- verify that client auth/send, server receive/queue, named-pipe handoff,
  switcher selection/render, and OBS preview still hold when one or more
  clients send from other PCs
- separate same-PC saturation symptoms from true cross-host network/runtime
  problems

This phase is not for:

- rolling back the same-PC `4`-client PASS
- reclassifying OBS capture PASS as a failure
- starting OBS WebSocket or advanced OBS control
- starting retry/backoff manager work
- starting persistent decoder context work
- starting generic N-view refactor work

## Recommended PC Topology

### Preferred Role Split

Use one dedicated streaming PC and up to four client PCs.

- Streaming PC:
  - `stream-sync-server`
  - `stream-sync-switcher`
  - `OBS`
  - log collection
- Client PC 1:
  - `client1`
- Client PC 2:
  - `client2`
- Client PC 3:
  - `client3`
- Client PC 4:
  - `client4`

Reason:

- this keeps `server + switcher + OBS` on the downstream output host, which
  matches the current MVP direction in `system-design.md`
- it removes capture/encode pressure from the streaming PC as much as possible
- it keeps the named-pipe handoff local to the streaming PC

### Minimum Staged Rollout

Do not jump to `4` remote clients first if the available hardware is limited.
Use this staged rollout:

1. Stage A: `1` remote client
   - streaming PC:
     - `server`
     - `switcher`
     - `OBS`
     - remaining local clients
   - one other PC:
     - one remote client
2. Stage B: `2` or `3` remote clients
   - spread as many clients as available to separate PCs
3. Stage C: `4` remote clients
   - one client per gameplay PC if available

Recommended first actual run:

- Stage A with exactly `1` remote client

Reason:

- it is the smallest topology change from the existing same-PC PASS
- it proves cross-host UDP reachability and auth without requiring all four
  client PCs on the first attempt
- it reduces same-PC saturation enough to start separating network issues from
  local resource pressure

## Network Preconditions

### Server Bind

Current server manual config already uses:

- `bind_host = "0.0.0.0"`
- `bind_port = 5000`

Distributed-PC expectation:

- server keeps listening on UDP `5000` on the streaming PC
- server must remain reachable from other PCs on the same LAN

### Client Server Address

Current local client configs use:

- `server_host = "127.0.0.1"`
- `server_port = 5000`

Distributed-PC expectation:

- every remote client must use:
  - `server_host = <streaming-pc-lan-ip>`
  - `server_port = 5000`
- when any client is remote, prefer all clients in that run to use the same
  `<streaming-pc-lan-ip>` for consistency
- if some local clients stay on `127.0.0.1`, record that explicitly in the
  evidence block

### Firewall

The streaming PC must allow:

- inbound UDP `5000`

Recommended assumption:

- allow the rule on the private/LAN profile only

Remote client PCs do not need an inbound rule for this phase because they are
only sending to the server and receiving normal auth responses on the same UDP
socket they opened.

### Run Identity And Auth Consistency

All active participants in one run must agree on:

- `run_id = streamsync-dev-session`
- unique `client_id` per client:
  - `player1`
  - `player2`
  - `player3`
  - `player4`
- per-client `shared_token` matching the server auth config
- `pipe_name = streamsync-handoff-dev` on the streaming PC

Interpretation:

- `pipe_name` is local only between `server` and `switcher`
- remote clients never touch the named pipe directly

## Preflight Checklist

Before the first distributed-PC actual run:

1. Record the streaming PC LAN IP.
2. Record the exact PC placement for:
   - `server`
   - `switcher`
   - `OBS`
   - `client1`
   - `client2`
   - `client3`
   - `client4`
3. Confirm the streaming PC can keep:
   - `configs/manual/server.two-real-slots.toml`
   - `pipe_name=streamsync-handoff-dev`
4. Prepare per-client config values so each active client uses:
   - correct `client_id`
   - correct `shared_token`
   - the intended `server_host`
   - `server_port=5000`
   - `run_id=streamsync-dev-session`
5. Confirm Windows Firewall allows inbound UDP `5000` on the streaming PC.
6. Confirm OBS is ready to capture:
   - `StreamSync 4-view Output`
7. Build before the run:

```powershell
cargo build -p stream-sync-server -p stream-sync-switcher -p stream-sync-client
```

## Launch Order

Use this order exactly.

1. Create a log directory and record topology metadata.
2. Start the server on the streaming PC.
3. Wait for the server ready line and confirm:
   - `receive_ready=true`
   - `handoff_ready=true`
   - `runtime_mode=concurrent`
   - `validation_ready=n/a`
   - `expected_reassembled_frames_enabled=false`
   - `expected_clients_enabled=false`
   - `expected_per_client_frames_enabled=false`
4. Start the switcher on the streaming PC.
5. Start OBS on the streaming PC.
6. Confirm OBS is pointed at:
   - `StreamSync 4-view Output`
7. Start the clients with minimal delay in this order:
   - `client1`
   - `client2`
   - `client3`
   - `client4`
8. For summary-required runs:
   - wait for the switcher final summary
   - wait for all client final summaries
   - wait for the server stopped summary
9. For OBS-operation runs:
   - capture OBS evidence first
   - still wait for client and server summaries
   - switcher final summary may be missing and must be classified separately

## Command Shape

Use the existing manual runtime commands. This phase changes topology and
config values, not the binary interface.

### Log Directory Naming

Use one of these shapes:

- summary-required run:
  - `manual-logs/distributed-pc-summary-<yyyymmdd-hhmmss>`
- OBS-operation run:
  - `manual-logs/distributed-pc-obs-<yyyymmdd-hhmmss>`

### Server Command

Run on the streaming PC:

```powershell
.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-continuous configs/manual/server.two-real-slots.toml streamsync-handoff-dev 4000 300000 600000 0 0 false 268435456 0 0
```

Expected meaning:

- keep the current functional PASS baseline
- keep `bind_host=0.0.0.0`
- keep UDP port `5000`
- keep the named pipe local on the streaming PC

### Switcher Command

#### Summary-Required Run

Run on the streaming PC:

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 180 preview-latest-decodable
```

Use this when:

- final switcher summary is required
- this run is the main functional judgment run

#### OBS-Operation Run

Run on the streaming PC:

```powershell
.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 900 preview-latest-decodable
```

Use this when:

- the operator needs longer time to manipulate OBS
- the run is for capture evidence, not for strict switcher-summary closeout

### Client Command

Run on each client PC:

```powershell
.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded <client-config-path> 900 16 1 --encoder-runtime persistent --cadence-mode deadline
```

`<client-config-path>` must resolve to a config for that player's:

- `client_id`
- `shared_token`
- `run_id`
- `server_host`
- `server_port`

Distributed-PC config rule:

- remote clients must not use `127.0.0.1`
- they must point at the streaming PC LAN IP

### OBS Capture Check

On the streaming PC:

1. Select or update a normal `Window Capture` source.
2. Confirm the selected title is exactly:
   - `StreamSync 4-view Output`
3. Confirm OBS preview shows the `4`-view output.
4. Capture a screenshot or equivalent pasted-back evidence.

## Success Criterion

Use a two-layer judgment:

- functional PASS gate
- performance interpretation

### Functional PASS Gate

Treat the distributed-PC run as a functional PASS only when all of the
following are true.

#### Client Gate

All active clients show:

- `accepted=true`
- `frames_sent=900`
- `send_failures=0`
- `h264_parameter_sets_cached=true`
- `stop_reason=Some(MaxFramesReached)`

Preferred stronger evidence:

- `frames_encoded=900`
- `keyframes_sent=30`
- `encode_failures=0`

#### Server Gate

The server ready line must show:

- `receive_ready=true`
- `handoff_ready=true`
- `runtime_mode=concurrent`
- `validation_ready=n/a`

The final server stopped summary must show:

- `stop_reason=ReceiveStopped`
- `receive_stop_reason=ReceiveTimedOut`
- `handoff_stop_reason=StopRequested`
- `frames_queued=3600`
- `per_client_queued_frames` includes all four scopes at `900`
- `retained_keyframe_clients=4`
- `frame_read_count > 0`
- `io_error_count=0`

#### Switcher Gate

For the summary-required run, the final switcher summary must show:

- `command_name=--four-view-four-real-handoff-preview-loop`
- `preview_mode=preview-latest-decodable`
- `read_mode=inspect-latest-decodable`
- `scheduler_status=AllSelected`
- `slot_result_kinds=Selected|Selected|Selected|Selected`
- `clean_output_render_result_kind=Rendered`
- `render_failures=0`
- `window_title=StreamSync 4-view Output`
- `frames_rendered > 0`

#### OBS Gate

OBS must show:

- selected window title is `StreamSync 4-view Output`
- preview is visible
- preview is not black
- preview is not transparent
- preview is not the wrong window
- all `4` slots are visible

### Performance Interpretation

Performance is a separate judgment surface from functional PASS.

Treat the run as having acceptable distributed-phase performance evidence when:

- no client stops with `EncodeFailure`
- client effective output FPS does not collapse into the same
  `16-18fps` band seen in the same-PC OBS PARTIAL run
- switcher output is not subjectively reported as extremely low FPS

Interpretation rule:

- if the functional PASS gate succeeds but performance is still degraded,
  classify the run as:
  - `functional PASS with distributed performance follow-up`
- do not collapse that directly into network or OBS failure

## Failure Classification

Classify one primary bucket first.

### 1. Network / Firewall

Use this when:

- remote clients cannot reach the streaming PC
- server ready line exists but remote traffic does not arrive
- auth/send attempts stall around basic connectivity

Typical evidence:

- no or near-zero `packets_received`
- remote client send path runs but server shows no matching traffic
- issue clears only after firewall/address correction

### 2. Auth / Config Mismatch

Use this when:

- `run_id` differs across participants
- wrong `client_id` is used
- wrong `shared_token` is used
- client points at the wrong `server_host` or `server_port`

Typical evidence:

- `accepted=false`
- auth reject reason such as invalid token
- queue participation missing for one client scope only

### 3. Client Capture / Encode

Use this when any client shows:

- `EncodeFailure`
- `frames_sent < 900`
- `send_failures > 0`
- local capture/encode degradation before transport is the main problem

### 4. Server Receive / Queue

Use this when:

- server ready line exists
- clients authenticate and send
- but final queue participation is incomplete or missing

Typical evidence:

- `frames_queued < 3600`
- missing entries in `per_client_queued_frames`
- `retained_keyframe_clients < 4`
- `io_error_count > 0`

### 5. Handoff Transport

Use this when:

- server queue state looks healthy
- but named-pipe handoff or switcher read path fails

Typical evidence:

- `handoff_response_kind=HandoffError`
- `parse_error!=none`
- `io_error!=none`
- switcher cannot read despite healthy queue evidence

### 6. Switcher Selection / Decode / Render

Use this when:

- handoff read exists
- but switcher final state does not reach clean all-real output

Typical evidence:

- `scheduler_status!=AllSelected`
- any final slot is not `Selected`
- `decode_error!=none`
- `clean_output_render_result_kind!=Rendered`
- `render_failures>0`

### 7. OBS Capture

Use this when:

- StreamSync output may exist
- but OBS cannot capture it correctly

Typical evidence:

- wrong selected title
- black preview
- transparent preview
- wrong window
- `4` slots not visible inside OBS preview

### 8. Distributed Clock / Timing / Performance

Use this when:

- network and auth are basically correct
- but cross-host timing/resource behavior is still degraded

Typical evidence:

- visible low-FPS playback
- client FPS collapse without a primary auth or queue failure
- functional PASS gate succeeds, but runtime quality remains poor

## Evidence Shape

Use this pasted-back report shape.

```text
log_dir=manual-logs/distributed-pc-<summary-or-obs>-<timestamp>
topology_stage=StageA|StageB|StageC
streaming_pc=<hostname or label>
streaming_pc_lan_ip=<ip:port>
server_pc=<hostname or label>
switcher_pc=<hostname or label>
obs_pc=<hostname or label>
client1_pc=<hostname or label>
client2_pc=<hostname or label>
client3_pc=<hostname or label>
client4_pc=<hostname or label>
client_server_hosts=player1:<host>|player2:<host>|player3:<host>|player4:<host>
selected_window_title=StreamSync 4-view Output|<other>
obs_preview_result=Visible4View|Black|Transparent|WrongWindow|Partial4View|NotCollected
primary_classification=<one bucket above>
functional_result=PASS|PARTIAL|FAIL
performance_result=PASS|PARTIAL|FAIL
notes=<short human observation>
```

Required attached evidence:

- server ready line
- server stopped summary
- switcher final summary for summary-required runs
- client1..4 final summaries
- OBS manual result
- log directory path
- PC placement memo

## Long OBS Run And Final Summary Handling

The repo must treat OBS capture evidence and switcher final-summary recovery as
separate concerns.

### Rule 1

`OBS capture PASS` does not require switcher final summary recovery from a long
operator-driven run.

### Rule 2

When final switcher summary is required, use the short bounded switcher run:

- `frames=180`

This is the main functional gate.

### Rule 3

When the operator needs longer time for OBS interaction, use the longer
switcher run:

- `frames=900`

This run may end with:

- OBS capture evidence collected
- client summaries collected
- server summary collected
- switcher final summary not collected within the operator wait window

That outcome must be classified as:

- OBS evidence collected successfully
- switcher final-summary recovery missing

not as:

- OBS capture failure by itself

### Rule 4

If both outputs are required, do two runs:

1. short summary-required run
2. longer OBS-operation run

This is preferred over trying to force one long run to satisfy both judgments.

## Expected Next Actual Run Order

1. Execute Stage A with `1` remote client as the first distributed-PC run.
2. Run the short summary-required command set first.
3. If Stage A functionally passes, run the longer OBS-operation capture pass.
4. Only then widen to Stage B or Stage C.

## Not In Scope Yet

- distributed-PC code changes
- retry/backoff manager
- persistent decoder context
- generic N-view refactor
- OBS WebSocket / advanced OBS control
- protocol or architecture changes
- re-opening same-PC `4`-client PASS
- re-opening OBS capture PASS
