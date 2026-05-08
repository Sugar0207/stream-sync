$RepoPath = "\\desktop-89uvrhh\d\stream-sync"
$RunMinutes = 30
$FrameRate = 30
$ReceiveTimeoutMs = 15000
$HeartbeatTimeoutMicros = 5000000
$ReceiveBufferBytes = 8388608
$FragmentPacingEvery = 16
$FragmentPacingDelayMs = 1

$ErrorActionPreference = "Stop"

function Quote-Pwsh([string]$Value) {
    return "'" + $Value.Replace("'", "''") + "'"
}

function Resolve-NativePath([string]$Path) {
    return (Resolve-Path -LiteralPath $Path).ProviderPath
}

function Assert-NativePath([string]$Label, [string]$Path) {
    if ($Path -like "Microsoft.PowerShell.Core\FileSystem::*") {
        throw "$Label must be a native Windows path: $Path"
    }
}

$RepoPath = Resolve-NativePath $RepoPath
Assert-NativePath "RepoPath" $RepoPath
Set-Location -LiteralPath $RepoPath

$ServerExe = Join-Path $RepoPath "target\debug\stream-sync-server.exe"
$ClientExe = Join-Path $RepoPath "target\debug\stream-sync-client.exe"

$ServerConfig = Resolve-NativePath (Join-Path $RepoPath "configs\manual\server.two-real-slots.toml")
$Client1Config = Resolve-NativePath (Join-Path $RepoPath "configs\manual\client.player1.toml")
$Client2Config = Resolve-NativePath (Join-Path $RepoPath "configs\manual\client.player2.toml")

$ConfigPaths = @($ServerConfig, $Client1Config, $Client2Config)
foreach ($Path in $ConfigPaths) {
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing config: $Path"
    }
    Assert-NativePath "ConfigPath" $Path
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
$LogDir = Resolve-NativePath $LogDir
Assert-NativePath "LogDir" $LogDir

$ServerLog = Join-Path $LogDir "server-continuous.log"
$Client1Log = Join-Path $LogDir "client-player1.log"
$Client2Log = Join-Path $LogDir "client-player2.log"

Assert-NativePath "ServerLog" $ServerLog
Assert-NativePath "Client1Log" $Client1Log
Assert-NativePath "Client2Log" $Client2Log

$ClientFrames = $RunMinutes * 60 * $FrameRate

Write-Host "Resolved repo path: $RepoPath"
Write-Host "Resolved server config path: $ServerConfig"
Write-Host "Resolved client1 config path: $Client1Config"
Write-Host "Resolved client2 config path: $Client2Config"
Write-Host "Resolved log dir: $LogDir"
Write-Host ""

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
Write-Host 'server config path:' $QServerConfig
& $QServerExe --receive-send-runtime-continuous $QServerConfig $ReceiveTimeoutMs 0 $HeartbeatTimeoutMicros $ReceiveBufferBytes 2>&1 |
    Tee-Object -FilePath $QServerLog
"@

$Client1Command = @"
Set-Location -LiteralPath $QRepoPath
`$Host.UI.RawUI.WindowTitle = 'StreamSync Client Player1'
Write-Host 'client1 config path:' $QClient1Config
& $QClientExe --auth-real-encoded-video-frame-poc-bounded $QClient1Config $ClientFrames $FragmentPacingEvery $FragmentPacingDelayMs 2>&1 |
    Tee-Object -FilePath $QClient1Log
"@

$Client2Command = @"
Set-Location -LiteralPath $QRepoPath
`$Host.UI.RawUI.WindowTitle = 'StreamSync Client Player2'
Write-Host 'client2 config path:' $QClient2Config
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
Write-Host "Receive buffer bytes: $ReceiveBufferBytes"
Write-Host ""
Write-Host "Watch these server summary fields:"
Write-Host "  receive_buffer_requested_bytes"
Write-Host "  receive_buffer_effective_bytes"
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
