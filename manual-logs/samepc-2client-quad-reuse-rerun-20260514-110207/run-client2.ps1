$RepoRoot = 'S:\stream-sync'
$LogDir = 'S:\stream-sync\manual-logs\samepc-2client-quad-reuse-rerun-20260514-110207'
Set-Location $RepoRoot
Write-Host '=== StreamSync client2 ==='
Write-Host "Log: $LogDir\client2.log"
& .\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 900 16 1 --encoder-runtime persistent --cadence-mode deadline 2>&1 |
  Tee-Object -FilePath (Join-Path $LogDir 'client2.log')
