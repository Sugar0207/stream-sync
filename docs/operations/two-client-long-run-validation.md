<!-- stream-sync/docs/operations/two-client-long-run-validation.md -->

# 2-client Long-run Validation

このドキュメントは、MVP クリティカルパス step 6 の最小固定手順です。

目的は、`stream-sync-server --receive-send-runtime-continuous` と 2 台の real encoded client sender を使って、人間が Windows 実環境で長時間 validation を実行し、成功条件と失敗時のログ回収項目を同じ形で共有できるようにすることです。

今回の main path は次です。

```text
client(player1) -> server continuous runtime <- client(player2)
```

この step では switcher / OBS は必須にしません。理由は次です。

- current `--live-two-view-switcher-once` は direct receive diagnostic / legacy path であり、fragmented real encoded main path ではない
- current long-run validation で先に固定したいのは、auth / heartbeat / fragment reassembly / frame queue / runtime rejection / timeout summary の観測である
- 4-client all-real + OBS Window Capture は次の step に分離する

## Prerequisites

1. Windows PowerShell を使う
2. repo を local path または UNC path で開ける
3. `ffmpeg` が `PATH` にある
4. `configs/manual/server.two-real-slots.toml`
5. `configs/manual/client.player1.toml`
6. `configs/manual/client.player2.toml`

事前に必ず確認すること:

- `server.two-real-slots.toml` の `shared_token`
- `client.player1.toml` の `shared_token`
- `client.player2.toml` の `shared_token`
- `run_id`

`player1` / `player2` の token は server 側 whitelist と一致させること。`run_id` も 3 ファイルで一致させること。

## Fixed Recipe

1. 事前 build を current PowerShell で 1 回だけ実行する
2. server continuous runtime を別 window で起動する
3. `2-3` 秒待つ
4. client 1 を別 window で起動する
5. `2` 秒待つ
6. client 2 を別 window で起動する
7. 30 分以上の run を観測する
8. 先に client window を止める
9. 最後に server window を止める
10. 各 log を回収して成功条件と照合する

## Recommended Runtime Settings

- `RunMinutes=30`
- `FrameRate=30`
- `ClientFrames=54000`
- `ReceiveTimeoutMs=15000`
- `HeartbeatTimeoutMicros=5000000`
- `FragmentPacingEvery=16`
- `FragmentPacingDelayMs=1`

`ClientFrames` は `RunMinutes * 60 * FrameRate` で決める。30 分より長く見たい場合は `RunMinutes` を増やす。

## PowerShell Script

次の script は PowerShell にそのまま貼れる完成形です。repo path を変数化し、server / client1 / client2 を別 window で起動し、各 window の stdout/stderr を log に保存します。

同じ内容は repo 内の [two-client-long-run-validation.ps1](/\\desktop-89uvrhh\d\stream-sync\docs\operations\two-client-long-run-validation.ps1) にも保存してあります。

```powershell
$RepoPath = "\\desktop-89uvrhh\d\stream-sync"
$RunMinutes = 30
$FrameRate = 30
$ReceiveTimeoutMs = 15000
$HeartbeatTimeoutMicros = 5000000
$FragmentPacingEvery = 16
$FragmentPacingDelayMs = 1

$ErrorActionPreference = "Stop"

function Quote-Pwsh([string]$Value) {
    return "'" + $Value.Replace("'", "''") + "'"
}

$RepoPath = (Resolve-Path -LiteralPath $RepoPath).Path
Set-Location -LiteralPath $RepoPath

$ServerExe = Join-Path $RepoPath "target\debug\stream-sync-server.exe"
$ClientExe = Join-Path $RepoPath "target\debug\stream-sync-client.exe"

$ServerConfig = Join-Path $RepoPath "configs\manual\server.two-real-slots.toml"
$Client1Config = Join-Path $RepoPath "configs\manual\client.player1.toml"
$Client2Config = Join-Path $RepoPath "configs\manual\client.player2.toml"

$ConfigPaths = @($ServerConfig, $Client1Config, $Client2Config)
foreach ($Path in $ConfigPaths) {
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing config: $Path"
    }
}

$PlaceholderTokens = Select-String -Path $ConfigPaths -Pattern "replace-with-shared-token" -SimpleMatch
if ($PlaceholderTokens) {
    throw "Replace shared_token placeholders in configs/manual before running the validation."
}

Get-Command ffmpeg -ErrorAction Stop | Out-Null

cargo build -p stream-sync-server -p stream-sync-client

$Timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$LogDir = Join-Path $RepoPath ("artifacts\manual-validation\two-client-long-run\" + $Timestamp)
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null

$ServerLog = Join-Path $LogDir "server-continuous.log"
$Client1Log = Join-Path $LogDir "client-player1.log"
$Client2Log = Join-Path $LogDir "client-player2.log"

$ClientFrames = $RunMinutes * 60 * $FrameRate

$QRepoPath = Quote-Pwsh $RepoPath
$QServerExe = Quote-Pwsh $ServerExe
$QClientExe = Quote-Pwsh $ClientExe
$QServerConfig = Quote-Pwsh $ServerConfig
$QClient1Config = Quote-Pwsh $Client1Config
$QClient2Config = Quote-Pwsh $Client2Config
$QServerLog = Quote-Pwsh $ServerLog
$QClient1Log = Quote-Pwsh $Client1Log
$QClient2Log = Quote-Pwsh $Client2Log

$ServerCommand = @"
Set-Location -LiteralPath $QRepoPath
`$Host.UI.RawUI.WindowTitle = 'StreamSync Server Continuous'
& $QServerExe --receive-send-runtime-continuous $QServerConfig $ReceiveTimeoutMs 0 $HeartbeatTimeoutMicros 2>&1 |
    Tee-Object -FilePath $QServerLog
"@

$Client1Command = @"
Set-Location -LiteralPath $QRepoPath
`$Host.UI.RawUI.WindowTitle = 'StreamSync Client Player1'
& $QClientExe --auth-real-encoded-video-frame-poc-bounded $QClient1Config $ClientFrames $FragmentPacingEvery $FragmentPacingDelayMs 2>&1 |
    Tee-Object -FilePath $QClient1Log
"@

$Client2Command = @"
Set-Location -LiteralPath $QRepoPath
`$Host.UI.RawUI.WindowTitle = 'StreamSync Client Player2'
& $QClientExe --auth-real-encoded-video-frame-poc-bounded $QClient2Config $ClientFrames $FragmentPacingEvery $FragmentPacingDelayMs 2>&1 |
    Tee-Object -FilePath $QClient2Log
"@

Start-Process powershell.exe -ArgumentList @("-NoExit", "-Command", $ServerCommand)
Start-Sleep -Seconds 3

Start-Process powershell.exe -ArgumentList @("-NoExit", "-Command", $Client1Command)
Start-Sleep -Seconds 2

Start-Process powershell.exe -ArgumentList @("-NoExit", "-Command", $Client2Command)
Start-Sleep -Seconds 2

Write-Host ""
Write-Host "2-client long-run validation windows started."
Write-Host "Log directory: $LogDir"
Write-Host "Client frames per sender: $ClientFrames"
Write-Host ""
Write-Host "Watch these server summary fields:"
Write-Host "  packets_received"
Write-Host "  accepted_packets"
Write-Host "  rejected_packets"
Write-Host "  frames_reassembled"
Write-Host "  frames_queued"
Write-Host "  direct_frames_queued"
Write-Host "  video_queue_len"
Write-Host "  incomplete_reassembly_frames"
Write-Host "  last_runtime_rejection_status"
Write-Host "  last_runtime_rejection_reason"
Write-Host "  last_heartbeat_timeout_status"
Write-Host "  stop_reason"
Write-Host ""
Write-Host "After the run, collect tails with:"
Write-Host "  Get-Content -LiteralPath '$ServerLog' -Tail 80"
Write-Host "  Get-Content -LiteralPath '$Client1Log' -Tail 40"
Write-Host "  Get-Content -LiteralPath '$Client2Log' -Tail 40"
```

## Success Conditions

以下を満たせば、step 6 の最小 2-client validation pass とする。

### Client side

- `player1` / `player2` ともに auth accepted される
- `frames_captured > 0`
- `frames_encoded > 0`
- `frames_sent > 0`
- `send_failures = 0` が望ましい
- `last_encode_error = none`
- `last_ffmpeg_error = none`

### Server side

- `packets_received > 0`
- `accepted_packets > 0`
- `frames_reassembled > 0` または `direct_frames_queued > 0`
- `frames_queued > 0` または `direct_frames_queued > 0`
- `video_queue_len` が単調に破綻しない
  - current step では exact steady-state upper bound は固定しない
  - ただし run 中に急増し続ける場合は failure candidate とする
- `incomplete_reassembly_frames` が run closeout 時に不自然に増え続けていない
- `last_runtime_rejection_status` が `Reject` になっても想定外 reason で増えない
  - `UnauthenticatedSource`
  - `UnknownClient`
  - `EndpointMismatch`
  - `RunIdMismatch`
  が継続的に出る場合は failure
- `last_heartbeat_timeout_status`
  - active run 中は `Continue` が期待値
  - closeout 直後に sender 停止後 timeout で `ReconnectRequired` へ移ること自体は即 failure にしない

### Failure candidates

- auth rejected
- `packets_received = 0`
- `frames_reassembled = 0` かつ `direct_frames_queued = 0`
- `frames_queued = 0` かつ `direct_frames_queued = 0`
- `video_queue_len` が run 中ずっと増え続ける
- `last_runtime_rejection_reason` が同じ unexpected reason で繰り返し出る
- active run 中に heartbeat timeout が継続発生する

## What To Paste Back

失敗時は、次の 3 つをまとめて貼り返す。

1. 実行条件
2. server log tail
3. client1 / client2 log tail

### Paste-back Template

```text
[2-client long-run validation]
repo_path=
run_datetime=
run_minutes=
client_frames=
receive_timeout_ms=
heartbeat_timeout_micros=
fragment_pacing_every=
fragment_pacing_delay_ms=

[result]
pass_or_fail=
what_happened=

[server summary tail]
command_name=
iterations_attempted=
iterations_completed=
packets_received=
accepted_packets=
rejected_packets=
frames_reassembled=
frames_queued=
direct_frames_queued=
video_queue_len=
incomplete_reassembly_frames=
heartbeat_observations_committed=
last_receive_error=
last_send_error=
last_rejected_reason=
last_auth_status=
last_auth_reason=
last_registration_status=
last_registration_reason=
last_runtime_rejection_status=
last_runtime_rejection_reason=
last_heartbeat_timeout_status=
last_heartbeat_timeout_clients=
last_heartbeat_timeout_timed_out=
last_heartbeat_timeout_client=
last_heartbeat_timeout_reason=
stop_reason=

[client1 tail]
client_id=player1
frames_attempted=
frames_captured=
frames_encoded=
frames_sent=
no_frame_count=
encode_failures=
send_failures=
fragments_sent=
last_encode_error=
last_ffmpeg_error=
last_send_error=
stop_reason=

[client2 tail]
client_id=player2
frames_attempted=
frames_captured=
frames_encoded=
frames_sent=
no_frame_count=
encode_failures=
send_failures=
fragments_sent=
last_encode_error=
last_ffmpeg_error=
last_send_error=
stop_reason=

[raw log tail server]
<paste last 80 lines>

[raw log tail client1]
<paste last 40 lines>

[raw log tail client2]
<paste last 40 lines>
```

## Scope Notes

- これは human-run step 6 recipe 固定であり、Codex が長時間 validation を実行した記録ではない
- 4-client all-real validation は次 step
- OBS Window Capture 本格 validation は次 step
- retry / requeue / daemon lifecycle / reconnect policy は今回含めない
