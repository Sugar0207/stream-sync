$RepoRoot = 'S:\stream-sync'
$LogDir = 'S:\stream-sync\manual-logs\samepc-2client-quad-reuse-rerun-20260514-110207'
Set-Location $RepoRoot
Write-Host '=== StreamSync server ==='
Write-Host "Log: $LogDir\server.log"
& .\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-continuous configs/manual/server.two-real-slots.toml streamsync-handoff-dev 4000 300000 600000 0 0 false 268435456 0 0 2>&1 |
  Tee-Object -FilePath (Join-Path $LogDir 'server.log')
