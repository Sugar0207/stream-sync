<!-- stream-sync/docs/operations/persistent-decoder-plan.md -->

# Persistent Decoder Minimal Plan

最終更新: 2026-05-17

## 目的
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260517-121040` で、current one-shot FFmpeg decode path の dominant cost が `decode_output_read_elapsed_ms=3430` / `decode_output_read_exact_elapsed_ms=2771` に寄っていることを受け、persistent decoder を next design candidate として最小導入設計に整理する
- direct compose と zero-fill removal は維持したまま、decoder-side の next slice を docs-first で固定する
- 今回は実装しない。persistent decoder の full architecture rewrite ではなく、current `apps/switcher` decode path に差し込める最小 slice だけを定義する

## 現在の整理
- current path は one-shot FFmpeg decode で、decode ごとに `Command::spawn()`、stdin write、stdout read、wait、stderr join を繰り返す
- latest rerun では `decode_process_spawn_elapsed_ms=308`、`decode_input_write_elapsed_ms=427`、`decoded_buffer_clone_elapsed_ms=27` より、`decode_output_read_exact_elapsed_ms=2771` / `decode_output_read_elapsed_ms=3430` が支配的だった
- `decode_output_vec_resize_elapsed_ms=0` なので、zero-fill は current blocker ではない
- `decode_output_buffer_reuse_count=0` と decode cache ownership の問題は残るが、latest evidence では first-order bottleneck ではない

## Runtime Failure Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260517-125737` では、first persistent decoder slice は runtime FAIL だった
- current request/response assumption では `persistent_decode_attempt_count=20` に対して `persistent_decode_success_count=0`、`persistent_decode_failure_count=20`、`persistent_decode_fallback_count=20`、`persistent_decode_process_restart_count=20`、`persistent_decode_last_error=persistent_decode_stdout_read_timeout` だった
- `persistent_decode_stdout_read_elapsed_ms=0`、`persistent_decode_stdout_read_exact_elapsed_ms=0`、`persistent_decode_output_bytes_total=0` なので、persistent stdout 側は expected raw BGRA frame を 1 回も返せていない
- one-shot fallback 自体は成功しているが、timeout 待ちのせいで `effective_render_fps_after_first_render=4.411`、`decode_elapsed_ms=42170` まで悪化したため、current request/response path を成功 optimization としては扱わない
- current code では regression stop を優先し、`persistent_decode_stdout_read_timeout` を 1 回観測した時点で runtime-disabled にして以後は即 one-shot fallback へ流す fail-fast / circuit breaker を追加した
- したがって current persistent slice は「request/response path の runtime viability を probing する guarded experiment」であり、continuous-stream decoder rewrite は別 step の候補として明示的に保留する

## Circuit Breaker Rerun Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260517-170552` では、fail-fast / circuit breaker 自体は PASS した
- `persistent_decode_attempt_count=1`、`persistent_decode_process_spawn_count=1`、`persistent_decode_process_restart_count=0`、`persistent_decode_runtime_disabled=true`、`persistent_decode_runtime_disabled_reason=persistent_decode_stdout_read_timeout`、`persistent_decode_skipped_after_disabled_count=50`、`persistent_decode_timeout_count=1` で、timeout 1 回後に request/response persistent path は実質停止し、restart storm は再発しなかった
- `effective_render_fps_after_first_render=4.411 -> 8.107` まで回復したため、regression stop としては有効だった
- ただし `persistent_decode_success_count=0`、`persistent_decode_failure_count=1`、`persistent_decode_fallback_count=51`、`one_shot_decode_fallback_count=51` のままで、persistent decoder 自体は runtime FAIL 継続だった
- この rerun により、current request/response persistent decoder は「guarded fallback protection は有効だが decode optimization としては未成立」と整理する
- したがって current request/response path は再挑戦 priority を下げ、凍結候補として扱う

## One-Shot-Only Baseline Toggle
- current two-real preview loop には `--disable-persistent-decoder` を追加し、persistent decoder を config-disabled のまま起動できる
- config-disabled run では persistent timeout attempt を 1 回も行わず、最初から one-shot decode path に流す
- summary では以下を分けて読む
  - `persistent_decode_config_enabled=false`
  - `persistent_decode_runtime_disabled=false`
  - `persistent_decode_skipped_by_config_count`
  - `one_shot_decode_attempt_count`
  - `one_shot_decode_elapsed_ms`
  - `one_shot_decode_input_write_elapsed_ms`
  - `one_shot_decode_output_read_elapsed_ms`
  - `one_shot_decode_output_read_exact_elapsed_ms`
- one-shot-only baseline rerun command shapeは `S:\stream-sync` を repo root にして、existing `--four-view-two-real-handoff-preview-loop` command の末尾に `--disable-persistent-decoder` を付ける
- この toggle は request/response persistent decoder の再挑戦ではなく、pure one-shot baseline を取り直して decoder / compose / GDI variance を切り分けるための最小比較手段とする

## One-Shot-Only Baseline Rerun Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260517-174753` で、one-shot-only baseline は想定通り成立した
- persistent decoder config-disabled は正常動作し、`persistent_decode_config_enabled=false`、`persistent_decode_enabled=false`、`persistent_decode_attempt_count=0`、`persistent_decode_timeout_count=0`、`persistent_decode_process_spawn_count=0`、`persistent_decode_process_restart_count=0`、`persistent_decode_skipped_by_config_count=60` を確認した
- server / client / transport は成立しており、persistent decoder を完全に避けても switcher FPS は `effective_render_fps_after_first_render=7.760`、`effective_render_fps=7.114` に留まった
- pure one-shot baseline の decoder-side evidence は以下だった
  - `decode_elapsed_ms=8010`
  - `avg_decode_elapsed_ms=133.500`
  - `decode_process_spawn_elapsed_ms=635`
  - `decode_input_write_elapsed_ms=3300`
  - `decode_output_read_elapsed_ms=3671`
  - `decode_output_read_exact_elapsed_ms=2765`
  - `one_shot_decode_attempt_count=60`
  - `one_shot_decode_elapsed_ms=8010`
  - `one_shot_decode_input_write_elapsed_ms=3300`
  - `one_shot_decode_output_read_elapsed_ms=3671`
  - `one_shot_decode_output_read_exact_elapsed_ms=2765`
- same run では `quad_view_compose_elapsed_ms=3899`、`quad_view_compose_success_count=57`、`quad_view_full_compose_count=57`、`quad_view_incremental_update_count=0`、`avg_quad_view_compose_elapsed_ms=62.887` も重く、persistent decoder 以外の dominant cost が戻ってきている
- `gdi_paint_wait_elapsed_ms=51` は今回 run では主犯と見なさない
- したがって persistent decoder を単独原因と断定せず、next dominant candidates は one-shot decode I/O と quad_view_compose full compose cost に絞る

## 何を置き換えるか
- 置き換え対象は `apps/switcher/src/lib.rs` の current one-shot FFmpeg decode runtime のうち、decode ごとに作っている FFmpeg process lifecycle
- 具体的には以下を persistent 化候補とする
  - `Command::spawn()`
  - per-decode `stdin` / `stdout` pipe open-close
  - decode ごとの FFmpeg process warmup
- 置き換えないもの
  - switcher preview loop 自体
  - current decode cache key / render path
  - protocol / server / client path
  - direct compose path

## 最小アーキテクチャ案
- switcher process 内に `PersistentFfmpegDecoder` 相当の caller-owned state を持つ
- state は 1 decoder process と継続利用する `stdin` / `stdout` / `stderr` handles を保持する
- input は current one-shot path と同じ `SwitcherH264DecodeInput` を受ける
- output は current runtime hook と同じ `SwitcherH264DecodeRuntimeOutput` へ揃える
- current one-shot runtime は fallback として残し、persistent runtime failure 時の narrow escape hatch にする

## stdin / stdout 継続利用
- `stdin`
  - current H.264 Annex B payload を access unit 単位で順番に書く
  - decode request ごとに process を閉じず、同じ pipe に継続投入する
- `stdout`
  - expected raw BGRA size `width * height * 4` を current diagnostics と同じ計算で決める
  - decode request ごとに、その expected bytes 分だけを継続 pipe から読み取る
  - current `decode_output_read_exact_elapsed_ms` 相当は persistent mode でも維持し、one-shot と比較可能にする
- `stderr`
  - background reader か non-blocking draining で継続監視する
  - process failure / decode desync / restart 理由の observability を残す

## H.264 access unit 境界
- 最小 slice では、current queue/handoff path が渡している 1 payload = 1 decode request の境界をそのまま access unit 境界として扱う
- persistent decoder 側で複数 frame の batching はしない
- request/response の同期を壊さないため、`stdin` write 1回に対して `stdout` expected bytes 1回を対応づける
- access unit boundary が曖昧な payload を直す作業は今回の設計外とし、current client/server metadata 前提を維持する

## SPS / PPS / keyframe handling
- current path ですでに Annex B payload を decode しており、client 側の SPS/PPS prepend 振る舞いも存在するため、persistent decoder でも first slice は current payload contract を変えない
- non-keyframe 単体で decode 不可な payload は persistent 化しても解決しない可能性があるため、first slice では以下を前提にする
  - payload に必要な SPS/PPS が載るか、client prepend path で補われる
  - decode request 単位の `is_keyframe` / `payload_has_idr` semantics は current path を維持する
- 将来 retry を入れる場合でも、この step では keyframe wait buffer や decoder-side reordering までは入れない

## decode cache / render reuse との関係
- persistent decoder は first-order goal を `spawn` と `stdout read` の amortization に置く
- current decode cache ownership は first slice では変えない
- `decoded_buffer_clone_count` / `decode_cache_store_clone_count` / `decode_output_buffer_reuse_count` は current semantics を維持したまま比較する
- つまり first slice の成功条件は clone elimination ではなく、one-shot decode process lifecycle cost の低下とする

## failure 時 fallback / restart
- persistent process が以下のどれかになったら current request を fail-fast し、process restart を試みる
  - stdin write error
  - stdout read short/oversized mismatch
  - process exit
  - stderr から fatal decode session corruption が疑われるケース
- restart 失敗時は current one-shot decoder に fallback できる形を残す
- fallback は常設でも、debug-only flag でもよいが、first slice では narrow revert path を残すことを優先する
- `persistent_decode_stdout_read_timeout` は current evidence では高リスク failure として扱い、毎 decode で restart を繰り返さず runtime-disabled に倒す
- timeout disable 後は summary diagnostics から disabled reason / skipped count が分かる状態を維持し、同 run 内では即 one-shot fallback に流す

## diagnostics 候補
- 維持したい既存 field
  - `decode_process_spawn_elapsed_ms`
  - `decode_input_write_elapsed_ms`
  - `decode_output_read_elapsed_ms`
  - `decode_output_read_exact_elapsed_ms`
  - `decode_process_wait_elapsed_ms`
  - `decode_output_buffer_reuse_count`
- persistent slice で最小追加候補
  - `decode_persistent_session_spawn_count`
  - `decode_persistent_session_restart_count`
  - `decode_persistent_session_reuse_count`
  - `decode_fallback_to_oneshot_count`
  - `decode_stderr_fatal_count`
- 追加しすぎず、one-shot と比較できる field を優先する

## 最小 implementation slice
1. `apps/switcher/src/lib.rs` に caller-owned persistent FFmpeg decoder state を追加する
2. current runtime hook と同じ input/output を持つ persistent runtime hook を additive に追加する
3. first target は two-real preview loop のみとし、global default runtime 置換はしない
4. process reuse / restart / one-shot fallback の最小 diagnostics を追加する
5. same-PC `2`-client rerun で `decode_output_read_exact_elapsed_ms` / `decode_output_read_elapsed_ms` / `decode_process_spawn_elapsed_ms` を比較する

## Next Candidate Comparison
- current request/response persistent decoder を current path として凍結する
- continuous-stream decoder rewrite は別設計候補として保留し、別 step で必要性を再判断する
- one-shot-only baseline rerun `manual-logs/two-client-render-rerun-20260517-174753` を current baseline とし、decoder I/O と compose cost を next comparison axis にする
- one-shot fallback path の `decode_input_write_elapsed_ms` / `decode_output_read_elapsed_ms` / `decode_output_read_exact_elapsed_ms` / `decode_process_spawn_elapsed_ms` を再度 narrow に調査する
- `quad_view_compose_elapsed_ms` / `quad_view_full_compose_count` / `quad_view_incremental_update_count=0` を見直し、full compose cost を下げられる narrow candidate があるか確認する
- decode cache ownership / `decode_output_buffer_reuse_count=0` / clone-store follow-up は decoder I/O / compose の次比較候補として残す

## out of scope
- persistent decoder の full architecture rewrite
- continuous-stream decoder rewrite
- unsafe 前提の raw buffer tricks
- GPU decode / shared-memory backend
- render config 化
- `4`-client all-real widening
- distributed-PC actual run
- protocol / server / client changes
- Production Readiness PASS 判定

## 実装前の確認ポイント
- one-shot fallback を消さずに残せるか
- persistent process の request/response boundary を current preview loop tick と 1:1 に保てるか
- restart 時に current decode cache / selected frame state を壊さないか
- diagnostics summary を one-shot baseline と比較可能な形で維持できるか
