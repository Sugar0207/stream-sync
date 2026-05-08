# 2-client Same-PC Validation

このドキュメントは、現行 step 6 の human validation を
`same-PC smoke / stress profile` として固定するための運用メモです。

当面の 2-client validation は、同一 Windows PC 上で次を同時に動かします。

- `stream-sync-server --receive-send-runtime-continuous`
- client1 capture / FFmpeg encode / UDP send
- client2 capture / FFmpeg encode / UDP send

つまりこれは distributed-PC validation ではありません。
CPU / capture / encode / server receive drain が同じ PC で競合する前提で読みます。

主目的は、same-PC 2-client stress 下での server receive drain throughput と
fragment reassembly 改善確認です。

main path:

```text
same Windows PC:
  client(player1) -> server continuous runtime <- client(player2)
```

## Server Output Policy

- `--receive-send-runtime-continuous` の通常出力は summary 1 行を読む運用とする
- packet / drain cycle / reassembly の詳細ログは default では見ない
- 詳細ログが必要な rerun だけ `--verbose` を明示して使う
- same-PC validation では server window が流れ続けることよりも、final summary の比較を優先する

## Current Baseline

直近の human validation baseline は以下です。

- `player1` / `player2`
  - `300` frames をそれぞれ capture / encode / send
  - `send_failures=0`
- server
  - `receive_buffer_bytes=8388608`
  - `max_packets_per_drain_cycle=64`
  - `max_packets_drained_in_cycle=64`
  - `receive_would_block_count=2`
  - `packets_received=10804`
  - `frames_reassembled=44`
  - `incomplete_reassembly_frames=542`
- clients total
  - `fragments_sent=94797`

現時点の判断:

- `cap=64` では server receive drain が same-PC stress に追いついていない可能性が高い
- current blocker は distributed setup ではなく、same-PC 2-client stress での
  receive drain throughput と incomplete reassembly accumulation

## Current PASS Profile

最新の same-PC 2-client smoke PASS は以下です。

- server
  - `receive_buffer_requested_bytes=268435456`
  - `receive_buffer_effective_bytes=268435456`
  - `receive_buffer_set_error=none`
  - `receive_buffer_read_error=none`
  - `max_packets_per_drain_cycle=1024`
  - `packets_received=116418`
  - `accepted_packets=116418`
  - `rejected_packets=0`
  - `auth_requests_received=2`
  - `auth_responses_sent=2`
  - `frames_reassembled=600`
  - `frames_queued=600`
  - `direct_frames_queued=0`
  - `video_queue_len=16`
  - `incomplete_reassembly_frames=0`
  - `drain_cycles=25378`
  - `max_packets_drained_in_cycle=248`
  - `receive_would_block_count=25377`
  - `registered_clients=2`
  - `stop_reason=ReceiveTimedOut`
- interpretation
  - same-PC 2-client smoke is `PASS`
  - `cap1024` is sufficient for this profile
  - summary-only default solved the validation-readability problem

## Current Longer-run PASS Profile

最新の same-PC 2-client longer-run PASS は以下です。

- settings
  - `receive_buffer_bytes=268435456`
  - `max_packets_per_drain_cycle=1024`
  - summary-only default
  - `receive_timeout_ms=30000`
  - `max_frames=900 per client`
  - `fragment_pacing_every=4`
  - `fragment_pacing_delay_ms=2`
- server
  - `receive_buffer_requested_bytes=268435456`
  - `receive_buffer_effective_bytes=268435456`
  - `receive_buffer_set_error=none`
  - `receive_buffer_read_error=none`
  - `max_packets_per_drain_cycle=1024`
  - `packets_received=356060`
  - `accepted_packets=356060`
  - `rejected_packets=0`
  - `decode_errors=0`
  - `auth_requests_received=2`
  - `auth_responses_sent=2`
  - `frames_reassembled=1800`
  - `frames_queued=1800`
  - `direct_frames_queued=0`
  - `video_queue_len=16`
  - `incomplete_reassembly_frames=0`
  - `drain_cycles=76733`
  - `max_packets_drained_in_cycle=578`
  - `receive_would_block_count=76732`
  - `registered_clients=2`
  - `stop_reason=ReceiveTimedOut`
- interpretation
  - same-PC 2-client longer-run is `PASS`
  - 2-client ingest / reassembly path is passed for MVP smoke / longer-run scope
  - `cap1024` remains sufficient for the current longer-run profile

## Validation Positioning

- これは same-PC smoke / stress profile である
- distributed-PC validation 用の server IP / firewall 手順は主目的にしない
- distributed-PC の話は後続比較用の補足に留める
- switcher / OBS / 4-client validation にはまだ進まない
- adaptive jitter buffer / daemon lifecycle / reconnect policy にも進まない

## Prerequisites

1. Windows PowerShell を使う
2. repo を native Windows path で開く
3. `ffmpeg` が `PATH` にある
4. `configs/manual/server.two-real-slots.toml`
5. `configs/manual/client.player1.toml`
6. `configs/manual/client.player2.toml`

最低限そろえること:

- `shared_token`
- `run_id`
- server / client 両方で `run_id` 一致

## Fixed Recipe

1. 同一 PC 上で `cargo build -p stream-sync-server -p stream-sync-client`
2. server continuous runtime を 1 window で起動
3. `2-3` 秒待つ
4. client1 を別 window で起動
5. `2` 秒待つ
6. client2 を別 window で起動
7. same-PC load を観測しながら run
8. client windows を止める
9. server window を止める
10. summary と log tail を貼り返す

## Runtime Settings

推奨 standard profile:

- `RunMinutes=10`
- `FrameRate=30`
- `ClientFrames=900 per client`
- `ReceiveTimeoutMs=30000`
- `HeartbeatTimeoutMicros=5000000`
- `ReceiveBufferBytes=268435456`
- `FragmentPacingEvery=4`
- `FragmentPacingDelayMs=2`
- `MaxPacketsPerDrainCycle=1024`

same-PC smoke / longer-run の標準設定は上記を採用します。

## PowerShell Script

Source of truth:

- use [two-client-long-run-validation.ps1](/\\desktop-89uvrhh\d\stream-sync\docs\operations\two-client-long-run-validation.ps1)
- script は native path を解決する
- script は resolved config path を表示する
- script は `receive-buffer-bytes` と `max-packets-per-drain-cycle` を
  `--receive-send-runtime-continuous` に渡す

実行時は script 上の `$MaxPacketsPerDrainCycle` を `256` / `512` / `1024` に変えて比較します。
通常運用では script に `--verbose` は付けません。

## Success Conditions

same-PC validation では、CPU 負荷競合がある前提で baseline 比較を行います。

### Client side

- `player1` / `player2` とも auth accepted
- `frames_captured > 0`
- `frames_encoded > 0`
- `frames_sent > 0`
- `send_failures = 0`
- `last_encode_error = none`
- `last_ffmpeg_error = none`

### Server side

- `packets_received > 0`
- `accepted_packets > 0`
- `frames_reassembled > 0`
- `frames_queued > 0` または `direct_frames_queued > 0`
- standard longer-run profile では client 合計 `1800` frames に対して
  `frames_reassembled=1800` / `frames_queued=1800` を目標にする
- `rejected_packets = 0`
- `decode_errors = 0`
- `incomplete_reassembly_frames = 0`
- `max_packets_per_drain_cycle` が summary に出ている
- `max_packets_drained_in_cycle` が cap に張り付くかを見る
- `packets_received` が baseline `10804` を上回るかを見る
- `frames_reassembled` が baseline `44` を上回るかを見る
- `incomplete_reassembly_frames` が相対的に改善するかを見る

### Reading Rule

- `max_packets_drained_in_cycle == max_packets_per_drain_cycle`
  - drain cap 到達。server receive drain がまだ頭打ちの可能性が高い
- `max_packets_drained_in_cycle < max_packets_per_drain_cycle`
  - 少なくともその run では drain cap 固着が外れた可能性がある
- `packets_received` / `frames_reassembled` 改善
  - same-PC stress で receive throughput 改善の有力 signal
- `incomplete_reassembly_frames` 悪化
  - receive drain がまだ不足している可能性が高い
- `stop_reason=ReceiveTimedOut`
  - client 完了後に server が idle closeout した可能性が高い
  - `frames_reassembled` / `frames_queued` が期待値に達している場合は failure ではない

## Failure Candidates

- auth rejected
- `send_failures > 0`
- `packets_received = 0`
- `frames_reassembled = 0`
- `frames_queued = 0` かつ `direct_frames_queued = 0`
- `max_packets_drained_in_cycle` が cap に張り付き続け、baseline より
  `packets_received` / `frames_reassembled` が伸びない
- `incomplete_reassembly_frames` が baseline より明確に悪化する
- active run 中に unexpected runtime rejection が出る

## Recommended Comparison Order

現在は `256MiB receive buffer + cap1024 + summary-only + receive_timeout_ms=30000 + max_frames=900 per client + fragment_pacing_every=4 + fragment_pacing_delay_ms=2` を same-PC smoke / longer-run の標準設定とする。
次の step は cap 探索ではなく、server -> switcher handoff validation 準備または 4-client all-real validation 準備である。

## What To Paste Back

1. 実行条件
2. server summary 1 行
3. client1 / client2 tail

### Paste-back Template

```text
[2-client same-pc validation]
repo_path=
run_datetime=
run_minutes=
client_frames=
receive_timeout_ms=
heartbeat_timeout_micros=
receive_buffer_bytes=
max_packets_per_drain_cycle=
fragment_pacing_every=
fragment_pacing_delay_ms=

[result]
pass_or_fail=
what_happened=
same_pc_cpu_note=

[server summary tail]
command_name=
max_packets_per_drain_cycle=
drain_cycles=
last_packets_drained_in_cycle=
max_packets_drained_in_cycle=
receive_would_block_count=
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
configured_max_frames=
configured_max_ticks=
configured_frame_interval_ms=
runtime_ticks=
capture_attempts=
frames_captured=
frames_encoded=
frames_sent=
no_frame_count=
frames_remaining_to_max=
elapsed_ms=
capture_elapsed_ms=
encode_elapsed_ms=
avg_capture_elapsed_ms=
avg_encode_elapsed_ms=
capture_wait_or_no_frame_elapsed_ms=
effective_output_fps=
effective_fresh_capture_fps=
effective_send_fps=
loop_interval_sleep_ms=
total_fragment_pacing_sleep_ms=
send_elapsed_ms=
encode_failures=
send_failures=
fragments_sent=
last_encode_error=
last_ffmpeg_error=
last_send_error=
stop_reason=

[client2 tail]
client_id=player2
configured_max_frames=
configured_max_ticks=
configured_frame_interval_ms=
runtime_ticks=
capture_attempts=
frames_captured=
frames_encoded=
frames_sent=
no_frame_count=
frames_remaining_to_max=
elapsed_ms=
capture_elapsed_ms=
encode_elapsed_ms=
avg_capture_elapsed_ms=
avg_encode_elapsed_ms=
capture_wait_or_no_frame_elapsed_ms=
effective_output_fps=
effective_fresh_capture_fps=
effective_send_fps=
loop_interval_sleep_ms=
total_fragment_pacing_sleep_ms=
send_elapsed_ms=
encode_failures=
send_failures=
fragments_sent=
last_encode_error=
last_ffmpeg_error=
last_send_error=
stop_reason=
```

## Supplement

## Next Step Pointer

Use this doc as the same-PC ingest / reassembly baseline only.

For the next MVP step, move to:

- [two-client-handoff-validation.md](/\\desktop-89uvrhh\d\stream-sync\docs\operations\two-client-handoff-validation.md)

That handoff doc is now the source of truth for:

- server bounded handoff session
- switcher preview loop
- optional raw named-pipe isolation rerun
- current handoff success conditions
- paste-back template for the human-run handoff validation

distributed-PC validation は今の主目的ではありません。
server IP 固定や firewall 手順が必要になったら、その時点で別ドキュメントか補足として扱います。
