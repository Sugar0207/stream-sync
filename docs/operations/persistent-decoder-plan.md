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

## out of scope
- persistent decoder の full architecture rewrite
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
