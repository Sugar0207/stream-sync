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
- The environment for this phase has moved back to:
  - `S:\stream-sync`
- This step is docs-first only:
  - no code change
  - no distributed-PC actual run yet
  - no protocol or architecture change
  - no `cargo build` / `cargo check` / `cargo test` in this slice
- Build/test execution is intentionally handled in a later dedicated
  build-validation step. This doc assumes the required binaries already exist
  under `S:\stream-sync\target\debug\`.

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

### Validation Order For The Next Slice

Do not jump directly to the distributed `4`-client run. Use this order:

1. distributed-PC `2`-client smoke
   - exactly `1` remote client is required
   - `2` active clients total are recommended for the first runtime gate
2. distributed-PC `2`-client OBS visible
   - keep the same `2` active client scope
   - add manual OBS preview evidence
3. distributed-PC `4`-client summary-required run
4. long OBS run
5. failure-class-specific fixes

Interpretation:

- the next runtime slice is still distributed Stage A
- for this repo, Stage A should first be proven with `2` active clients, not
  with all `4`
- widen to `4` active clients only after the distributed `2`-client runtime
  gate and distributed `2`-client OBS visibility gate are both recorded

## Network Preconditions

### Server Bind

Current server manual config already uses:

- `bind_host = "0.0.0.0"`
- `bind_port = 5000`

Distributed-PC expectation:

- server keeps listening on UDP `5000` on the streaming PC
- server must remain reachable from other PCs on the same LAN or Tailscale
  network

### Client Server Address

Current local client configs use:

- `server_host = "127.0.0.1"`
- `server_port = 5000`

Distributed-PC expectation:

- every active distributed client must use:
  - `server_host = <SERVER_HOST>`
  - `server_port = 5000`
- `<SERVER_HOST>` may be:
  - the streaming PC LAN IP
  - the streaming PC Tailscale IP
- when any client is remote, prefer all active clients in that run to use the
  same `<SERVER_HOST>` for consistency, even if one client is started on the
  streaming PC itself

### Firewall

The streaming PC must allow:

- inbound UDP `5000`

Recommended assumption:

- allow the rule on the private/LAN profile only when using a LAN IP
- if using Tailscale, confirm the chosen network path still reaches UDP `5000`

Remote client PCs do not need a separate inbound rule for this phase because
they only send to the server and receive the auth response on the same UDP
socket they opened.

### Run Identity And Auth Consistency

All active participants in one run must agree on:

- `run_id = streamsync-dev-session`
- unique `client_id` per active client:
  - `player1`
  - `player2`
  - `player3`
  - `player4`
- per-client `shared_token` matching the server auth config
- `pipe_name = streamsync-handoff-dev` on the streaming PC

Interpretation:

- `pipe_name` is local only between `server` and `switcher`
- remote clients never touch the named pipe directly

## Required Replacements

Replace these values before running the command pack:

- `<RUN_STAMP>`
  - one shared timestamp such as `20260513-233000`
  - reuse the exact same value on every participating PC for the same run
- `<SERVER_HOST>`
  - the streaming PC LAN IP or Tailscale IP
- `<RUN_LABEL>`
  - one of:
    - `distributed-pc-2client-smoke`
    - `distributed-pc-2client-obs`
    - `distributed-pc-4client-summary`
    - `distributed-pc-4client-obs`
- active client config files
  - update the referenced TOML files so `server_host = "<SERVER_HOST>"`
  - keep `server_port = 5000`
  - keep `run_id = "streamsync-dev-session"`
  - keep the correct `client_id`
  - keep the correct `shared_token`

## Log Directory Convention

Use the same run directory name on every participating PC under:

- `S:\stream-sync\manual-logs\`

Recommended names:

- distributed `2`-client smoke:
  - `manual-logs/distributed-pc-2client-smoke-<RUN_STAMP>`
- distributed `2`-client OBS visible:
  - `manual-logs/distributed-pc-2client-obs-<RUN_STAMP>`
- distributed `4`-client summary-required:
  - `manual-logs/distributed-pc-4client-summary-<RUN_STAMP>`
- distributed `4`-client long OBS:
  - `manual-logs/distributed-pc-4client-obs-<RUN_STAMP>`

Recommended file names:

- streaming PC:
  - `server.log`
  - `switcher.log`
  - `obs-notes.txt`
  - optional `obs-preview.png`
- each active client PC:
  - `client1.log`
  - `client2.log`
  - `client3.log`
  - `client4.log`

## Preflight Checklist

Before the first distributed-PC actual run:

1. Confirm the repo root on every participating PC is:
   - `S:\stream-sync`
2. Record the streaming PC address to use as:
   - `<SERVER_HOST>`
3. Record the exact PC placement for:
   - `server`
   - `switcher`
   - `OBS`
   - `client1`
   - `client2`
   - `client3`
   - `client4`
4. Confirm the streaming PC can keep:
   - `configs/manual/server.two-real-slots.toml`
   - `pipe_name=streamsync-handoff-dev`
5. Update the active client config files so each active client uses:
   - correct `client_id`
   - correct `shared_token`
   - `server_host=<SERVER_HOST>`
   - `server_port=5000`
   - `run_id=streamsync-dev-session`
6. Confirm Windows Firewall allows inbound UDP `5000` on the streaming PC.
7. Confirm the required binaries already exist from the separate build step:
   - `S:\stream-sync\target\debug\stream-sync-server.exe`
   - `S:\stream-sync\target\debug\stream-sync-switcher.exe`
   - `S:\stream-sync\target\debug\stream-sync-client.exe`
8. Confirm OBS is ready to capture:
   - `StreamSync 4-view Output`

## Copy-Paste Command Pack

These commands are written for `S:\stream-sync`.

### Server PC Command

Run on the streaming PC in its own PowerShell window.

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = '<RUN_LABEL>'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-continuous configs/manual/server.two-real-slots.toml streamsync-handoff-dev 4000 300000 600000 0 0 false 268435456 0 0 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'server.log')
```

Expected meaning:

- keep the current functional PASS baseline
- keep `bind_host=0.0.0.0`
- keep UDP port `5000`
- keep the named pipe local on the streaming PC

### Switcher / OBS PC Command

Run on the streaming PC in a second PowerShell window.

#### Distributed `2`-client Smoke

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = 'distributed-pc-2client-smoke'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 180 preview-latest-decodable 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'switcher.log')
```

Use this when:

- the target is the first distributed runtime gate
- switcher final summary is required

#### Distributed `2`-client OBS Visible

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = 'distributed-pc-2client-obs'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 900 preview-latest-decodable 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'switcher.log')
```

Use this when:

- the target is OBS visibility on the distributed `2`-client scope
- the operator needs more time to manipulate OBS

#### Distributed `4`-client Summary-Required

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = 'distributed-pc-4client-summary'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 180 preview-latest-decodable 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'switcher.log')
```

#### Distributed `4`-client Long OBS Run

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = 'distributed-pc-4client-obs'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 900 preview-latest-decodable 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'switcher.log')
```

OBS manual steps on the same streaming PC:

1. Open or keep OBS ready.
2. Select or update a normal `Window Capture` source.
3. Confirm the selected title is exactly:
   - `StreamSync 4-view Output`
4. Confirm OBS preview shows the intended StreamSync output.
5. Save a screenshot and a short note into the same run directory.

### Client PC 1 Command

Run on the PC assigned to `player1`.

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = '<RUN_LABEL>'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'client1.log')
```

### Client PC 2 Command

Run on the PC assigned to `player2`.

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = '<RUN_LABEL>'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'client2.log')
```

### Optional Placeholder: Client PC 3

Use this only after widening to the distributed `4`-client run.

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = '<RUN_LABEL>'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'client3.log')
```

### Optional Placeholder: Client PC 4

Use this only after widening to the distributed `4`-client run.

```powershell
$RepoRoot = 'S:\stream-sync'
$RunStamp = '<RUN_STAMP>'
$RunLabel = '<RUN_LABEL>'
$LogDir = Join-Path $RepoRoot "manual-logs\$RunLabel-$RunStamp"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
Set-Location $RepoRoot
& .\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline 2>&1 | Tee-Object -FilePath (Join-Path $LogDir 'client4.log')
```

## Startup Order

Use this order exactly.

1. Pick one `<RUN_STAMP>` and one `<RUN_LABEL>` for the run.
2. Update active client config files so they point to `<SERVER_HOST>`.
3. Create the same-named run directory under `S:\stream-sync\manual-logs\` on
   every participating PC.
4. Start the server on the streaming PC.
5. Wait for the server ready line and confirm:
   - `receive_ready=true`
   - `handoff_ready=true`
   - `runtime_mode=concurrent`
   - `validation_ready=n/a`
   - `expected_reassembled_frames_enabled=false`
   - `expected_clients_enabled=false`
   - `expected_per_client_frames_enabled=false`
6. Start the switcher on the streaming PC.
7. For OBS-visible or long OBS runs:
   - start OBS or bring it forward on the streaming PC
8. Start the clients with minimal delay in this order:
   - `client1`
   - `client2`
   - optional `client3`
   - optional `client4`
9. For summary-required runs:
   - wait for the switcher final summary
   - wait for all active client final summaries
   - wait for the server stopped summary
10. For OBS-visible or long OBS runs:
   - capture OBS evidence first
   - still wait for active client and server summaries
   - if the switcher final summary is not collected within the operator wait
     window, classify that separately from OBS visibility

## Expected Summary Lines

### Server Ready Line

Expect all of:

- `receive_ready=true`
- `handoff_ready=true`
- `runtime_mode=concurrent`
- `validation_ready=n/a`
- `expected_reassembled_frames_enabled=false`
- `expected_clients_enabled=false`
- `expected_per_client_frames_enabled=false`

### Client Final Summaries

Every active client should show:

- `accepted=true`
- `frames_sent=900`
- `send_failures=0`
- `h264_parameter_sets_cached=true`
- `stop_reason=Some(MaxFramesReached)`

Preferred stronger evidence:

- `frames_encoded=900`
- `keyframes_sent=30`
- `encode_failures=0`

### Distributed `2`-client Summary-Required Switcher

Expect:

- `command_name=--four-view-two-real-handoff-preview-loop`
- `preview_mode=preview-latest-decodable`
- `scheduler_status=PartialSelected`
- `slot_result_kinds=Selected|Selected|NoFrameAvailable|NoFrameAvailable`
- `clean_output_render_result_kind=Rendered`
- `render_failures=0`
- `window_title=StreamSync 4-view Output`
- `output_width=1280`
- `output_height=720`
- `frames_rendered > 0`

### Distributed `4`-client Summary-Required Switcher

Expect:

- `command_name=--four-view-four-real-handoff-preview-loop`
- `preview_mode=preview-latest-decodable`
- `scheduler_status=AllSelected`
- `slot_result_kinds=Selected|Selected|Selected|Selected`
- `clean_output_render_result_kind=Rendered`
- `render_failures=0`
- `window_title=StreamSync 4-view Output`
- `output_width=1280`
- `output_height=720`
- `frames_rendered > 0`

### Server Stopped Summary

Distributed `2`-client expectation:

- `stop_reason=ReceiveStopped`
- `receive_stop_reason=ReceiveTimedOut`
- `handoff_stop_reason=StopRequested`
- `frames_queued=1800`
- `per_client_queued_frames` includes:
  - `player1/streamsync-dev-session:900`
  - `player2/streamsync-dev-session:900`
- `retained_keyframe_clients=2`
- `frame_read_count > 0`
- `io_error_count=0`

Distributed `4`-client expectation:

- `stop_reason=ReceiveStopped`
- `receive_stop_reason=ReceiveTimedOut`
- `handoff_stop_reason=StopRequested`
- `frames_queued=3600`
- `per_client_queued_frames` includes all four scopes at `900`
- `retained_keyframe_clients=4`
- `frame_read_count > 0`
- `io_error_count=0`

## PASS / PARTIAL / FAIL Criteria

Use three separate judgment surfaces:

- runtime evidence
- OBS visible evidence
- long OBS visual stability evidence

Important separation rule:

- switcher final summary is runtime evidence
- long OBS run is visual stability evidence
- do not merge them into one PASS condition

### PASS

Treat the phase as `PASS` only when all required gates for the target run have
passed.

Distributed `2`-client smoke `PASS`:

- both active clients pass their final summary gate
- server ready line passes
- server stopped summary matches the distributed `2`-client expected counts
- the short `2`-client switcher summary matches the runtime gate

Distributed `2`-client OBS visible `PASS`:

- the earlier distributed `2`-client smoke runtime gate already passed
- OBS selects `StreamSync 4-view Output`
- OBS preview is visible
- OBS preview is not black
- OBS preview is not transparent
- OBS preview is not the wrong window

Distributed `4`-client summary-required `PASS`:

- all four active clients pass their final summary gate
- server ready line passes
- server stopped summary matches the distributed `4`-client expected counts
- the short `4`-client switcher summary matches the runtime gate

### PARTIAL PASS

Treat the run as `PARTIAL` when one evidence surface passes but another remains
follow-up rather than a hard failure.

Typical `PARTIAL` cases:

- functional/runtime gate passes, but performance is still degraded
- OBS visible evidence passes, but the long OBS run shows visible stutter or
  low-FPS behavior
- a long OBS run collects valid visual evidence, active client summaries, and
  server summary, but the switcher final summary is not collected within the
  operator wait window

Interpretation:

- in that long-run case, classify it as:
  - visual evidence collected successfully
  - switcher final-summary recovery missing
- do not classify that shape as an OBS capture failure by itself

### FAIL

Treat the run as `FAIL` when the target gate itself fails.

Examples:

- an active client fails auth/send
- server ready line never reaches the expected concurrent state
- server stopped summary misses required queue participation
- short summary-required switcher gate fails for the target stage
- OBS cannot capture the intended window correctly during the OBS-visible run

## Failure Classification

Classify one primary bucket first.

### 1. Network / Firewall

Use this when:

- remote clients cannot reach the streaming PC
- server ready line exists but remote traffic does not arrive
- auth/send attempts stall around basic connectivity

### 2. Auth / Config Mismatch

Use this when:

- `run_id` differs across participants
- wrong `client_id` is used
- wrong `shared_token` is used
- a client points at the wrong `server_host` or `server_port`

### 3. Client Capture / Encode

Use this when any active client shows:

- `EncodeFailure`
- `frames_sent < 900`
- `send_failures > 0`

### 4. Server Receive / Queue

Use this when:

- clients authenticate and send
- but final queue participation is incomplete or missing

### 5. Handoff Transport

Use this when:

- server queue state looks healthy
- but named-pipe handoff or switcher read path fails

### 6. Switcher Selection / Decode / Render

Use this when:

- handoff read exists
- but switcher final state does not reach the expected selected/rendered state

### 7. OBS Capture

Use this when:

- StreamSync output may exist
- but OBS cannot capture it correctly

### 8. Distributed Clock / Timing / Performance

Use this when:

- network and auth are basically correct
- but cross-host timing/resource behavior is still degraded

## Evidence Paste-Back Template

Use this pasted-back report shape.

```text
repo_root=S:\stream-sync
run_label=distributed-pc-2client-smoke|distributed-pc-2client-obs|distributed-pc-4client-summary|distributed-pc-4client-obs
run_stamp=<RUN_STAMP>
server_host=<SERVER_HOST>
streaming_pc=<hostname or label>
server_pc=<hostname or label>
switcher_pc=<hostname or label>
obs_pc=<hostname or label>
client1_pc=<hostname or label>
client2_pc=<hostname or label>
client3_pc=<hostname or label>|inactive
client4_pc=<hostname or label>|inactive
active_clients=player1,player2|player1,player2,player3,player4
client_server_hosts=player1:<SERVER_HOST>|player2:<SERVER_HOST>|player3:<SERVER_HOST or inactive>|player4:<SERVER_HOST or inactive>
log_dir_streaming_pc=S:\stream-sync\manual-logs\<run_label>-<RUN_STAMP>
log_dir_client1_pc=S:\stream-sync\manual-logs\<run_label>-<RUN_STAMP>
log_dir_client2_pc=S:\stream-sync\manual-logs\<run_label>-<RUN_STAMP>
log_dir_client3_pc=S:\stream-sync\manual-logs\<run_label>-<RUN_STAMP>|inactive
log_dir_client4_pc=S:\stream-sync\manual-logs\<run_label>-<RUN_STAMP>|inactive
selected_window_title=StreamSync 4-view Output|<other>|NotCollected
obs_preview_result=Visible4View|Black|Transparent|WrongWindow|Partial4View|NotCollected
server_ready_result=PASS|FAIL
server_summary_result=PASS|PARTIAL|FAIL
switcher_runtime_summary_result=PASS|PARTIAL|FAIL|NotCollected
long_obs_visual_result=PASS|PARTIAL|FAIL|NotRun
primary_classification=<one bucket above>
overall_result=PASS|PARTIAL|FAIL
notes=<short human observation>
```

Required attached evidence:

- server ready line
- server stopped summary
- switcher final summary for summary-required runs
- active client final summaries
- OBS manual result
- log directory paths
- PC placement memo

## Long OBS Run And Final Summary Handling

The repo must treat runtime evidence and long-run visual evidence as separate
concerns.

### Rule 1

`switcher final summary` is runtime evidence.

### Rule 2

`long OBS run` is visual stability evidence.

### Rule 3

Use the short bounded switcher run when runtime evidence is the main gate:

- distributed `2`-client smoke:
  - `frames=180`
- distributed `4`-client summary-required:
  - `frames=180`

### Rule 4

Use the longer switcher run when the operator needs more time for OBS
interaction:

- distributed `2`-client OBS visible:
  - `frames=900`
- distributed `4`-client long OBS:
  - `frames=900`

### Rule 5

If both runtime evidence and longer OBS evidence are required, do two runs:

1. short summary-required run
2. longer OBS-visible or long OBS run

This is preferred over trying to force one long run to satisfy both judgments.

## Expected Next Actual Run Order

1. Prepare the distributed-PC `2`-client smoke run on `S:\stream-sync`.
2. Execute the short distributed `2`-client smoke command set first.
3. If that runtime gate passes, execute the distributed `2`-client OBS visible
   run.
4. If both `2`-client gates pass, widen to the distributed `4`-client
   summary-required run.
5. After the distributed `4`-client runtime gate, run the long OBS visual
   stability pass.
6. Only then prioritize failure-class-specific fixes.

## Not In Scope Yet

- distributed-PC code changes
- retry/backoff manager
- persistent decoder context
- generic N-view refactor
- OBS WebSocket / advanced OBS control
- protocol or architecture changes
- re-opening same-PC `4`-client PASS
- re-opening same-PC OBS capture PASS
