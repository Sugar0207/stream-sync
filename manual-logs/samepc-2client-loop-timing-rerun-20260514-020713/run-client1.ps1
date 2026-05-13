$RepoRoot = 'S:\stream-sync'
$LogDir = 'S:\stream-sync\manual-logs\samepc-2client-loop-timing-rerun-20260514-020713'
Set-Location $RepoRoot
Write-Host '=== StreamSync client1 ==='
Write-Host "Log: $LogDir\client1.log"
& .\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline 2>&1 |
  Tee-Object -FilePath (Join-Path $LogDir 'client1.log')
