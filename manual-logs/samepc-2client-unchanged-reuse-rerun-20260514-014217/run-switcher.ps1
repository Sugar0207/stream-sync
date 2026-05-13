$RepoRoot = 'S:\stream-sync'
$LogDir = 'S:\stream-sync\manual-logs\samepc-2client-unchanged-reuse-rerun-20260514-014217'
Set-Location $RepoRoot
Write-Host '=== StreamSync switcher ==='
Write-Host "Log: $LogDir\switcher.log"
& .\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 180 preview-latest-decodable 2>&1 |
  Tee-Object -FilePath (Join-Path $LogDir 'switcher.log')
