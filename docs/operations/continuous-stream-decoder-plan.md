<!-- stream-sync/docs/operations/continuous-stream-decoder-plan.md -->

# Continuous Stream Decoder Plan

最終更新: 2026-05-18

## 目的
- two-real preview loop の 30fps 目標に向けて、render loop から one-shot FFmpeg wait を外すための next design candidate を整理する
- current request/response persistent decoder の runtime FAIL 実装を復活させるのではなく、別設計の continuous-stream decoder として扱う
- decoded frame queue / cache を decoder runtime 側で育て、render loop は targetTime に合う decoded frame を参照するだけに近づける
- first implementation candidate は two-real preview loop 限定に留め、server / client / protocol / 4-client / GPU decode には広げない

## 背景
- scaled one-shot decode output は PASS 継続で、two-real slot size は `640x360`、raw BGRA expected bytes は `921600` に縮んでいる
- incremental quad compose も PASS 継続で、latest good-ish rerun では render / GDI は first-order culprit ではなかった
- latest good-ish rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260518-124418`
  - `effective_render_fps_after_first_render=17.247`
  - `decode_attempt_count=26`
  - `one_shot_decode_elapsed_ms=1893`
  - `one_shot_decode_first_byte_slow_count=0`
  - `one_shot_decode_output_read_slow_count=0`
  - `one_shot_decode_input_write_outlier_count=0`
  - `quad_view_compose_elapsed_ms=636`
  - `gdi_paint_wait_elapsed_ms=12`
- latest evidence では decode attempt frequency、slow first-byte、slow output-read、input-write outlier のどれか 1 つを主犯とは断定しない
- 残課題は one-shot FFmpeg の per-attempt variance と、decode miss 時に render loop が FFmpeg I/O を同期的に待つ構造にある

## request/response persistent decoder との違い
- request/response persistent decoder:
  - render loop から `1 request -> stdin write -> stdout expected bytes read -> response wait` を同期的に待つ
  - stdout read timeout が出ると、その tick の render loop も待たされる
  - current runtime evidence では `persistent_decode_stdout_read_timeout` により runtime FAIL
  - scaled-output path では現状 one-shot に直行し、request/response path は凍結候補
- continuous-stream decoder:
  - `1 request -> 1 response` を render loop が待たない
  - H.264 access unit を slot ごとの input queue に入れ、decoder writer thread が FFmpeg stdin へ連続投入する
  - FFmpeg stdout raw BGRA frame は reader thread が読み続け、decoded output queue/cache に積む
  - render loop は decoded queue/cache から targetTime に近い frame を lookup し、未到着なら previous / placeholder / one-shot fallback policy に従う
  - failure / stall / restart は decoder runtime 側で管理し、render loop blocking を避ける

## two-real preview loop 限定の最小構成
- scope:
  - `--four-view-two-real-handoff-preview-loop` の real slot `2` つだけ
  - player1 / player2 相当の configured real slot ごとに decoder runtime を 1 つ持つ案を first candidate にする
  - 残り 2 placeholder slot、4-client path、focused path、controlled path は対象外
- per-real-slot runtime:
  - `ContinuousH264DecoderRuntime` 相当の caller-owned state
  - access unit input queue
  - frame-id correspondence queue
  - decoded frame output queue/cache
  - stdout reader thread
  - stdin writer thread
  - stderr drain / process monitor
  - restart / disabled / fallback state
- render loop との接続:
  - handoff / targetTime selection は current two-real preview loop のまま維持する
  - selected encoded frame が見つかったら、render loop は decoder input queue への enqueue を試みる
  - render loop は同 tick で decode 完了を待たず、decoded cache から selected frame_id または targetTime に合う latest decoded frame を参照する
  - decoded frame がまだ無ければ current display policy の hold previous / no-display placeholder / source-error placeholder を使う
- first導入の安全策:
  - one client / one slot だけ opt-in できる形を first slice にする余地を残す
  - one-shot path は必ず残す
  - runtime flag / CLI toggle で continuous path を明示 opt-in にする

## targetTime selection との関係
- server / handoff / targetTime source は引き続き encoded frame を選ぶ
- continuous decoder は targetTime を決めない
- decoder runtime は「投入された access unit を順序通り decode し、decoded frame を frame_id / timestamp metadata と一緒に保持する」だけに寄せる
- render loop の lookup 方針は first slice では以下の二段にする
  - preferred: selected encoded frame の `client_id + run_id + frame_id` に一致する decoded frame
  - fallback candidate: same source の latest decoded frame で、targetTime より未来に進みすぎないもの
- targetTime より古すぎる decoded frame を使うかどうかは display policy / stale policy 側で扱い、decoder runtime が勝手に同期判断をしない

## frame_id 対応
- FFmpeg stdout rawvideo には `frame_id` が含まれない
- first candidate は input order と output order の対応で紐づける
  - input queue に `QueuedAccessUnit { client_id, run_id, frame_id, capture_timestamp, is_keyframe, payload }` を積む
  - writer thread が FFmpeg stdin へ書いた順に `frame_id correspondence queue` へ metadata を push する
  - reader thread が raw BGRA 1 frame 分を読むたびに correspondence queue の head を pop し、その metadata を decoded frame に付ける
- 注意点:
  - decoder が frame を drop / skip した場合、input order と output order がずれる
  - B-frame reorder がある場合、output order が input order と一致しない
  - decoder delay がある場合、initial output が数 access unit 遅れる可能性がある
- first slice 前提:
  - client encode は low-latency / zerolatency / no B-frame 前提として扱う
  - libx264 `zerolatency` 相当の no B-frame assumption を docs / diagnostics で明示確認する
  - B-frame が混ざる可能性がある run では continuous-stream frame_id correspondence は unsafe とし、one-shot fallback を使う
- mismatch suspicion:
  - output count が input count を超えた
  - correspondence queue が空なのに stdout frame が来た
  - long lag が一定以上続く
  - keyframe後も expected frame_id の decoded output が長時間来ない
  - これらは `frame_id mismatch suspicion` として runtime disable / restart / fallback の候補にする

## SPS / PPS / keyframe handling
- current queue/handoff path は retained keyframe と parameter sets prepend 済み payload の文脈を持つ
- continuous decoder startup では、最初の投入は decodable な keyframe access unit を要求する
  - `has_sps && has_pps && has_idr`、または retained SPS/PPS prepend 済み IDR payload
  - startup 前の non-keyframe は enqueue せず、one-shot fallback または wait 扱いにする
- source recovered 後:
  - handoff error / no-frame / waiting から復帰した場合、decoder state がまだ健全なら通常 enqueue を続ける
  - decoder restart 後は keyframe wait state に戻す
  - retained keyframe が利用できる場合は、restart bootstrap 用に retained keyframe を優先投入する候補を残す
- retained keyframe:
  - decoder runtime 自体が retained keyframe の source of truth にはならない
  - server / handoff 側の retained keyframe visibility を使い、bootstrap/restart 時に caller が投入する
  - first slice では retained keyframe injection を自動化しすぎず、decodable selected frame が来るまで wait して one-shot fallback を残す

## queue / drop policy
- access unit input queue:
  - per slot bounded queue
  - bound は frame count で持つ。first candidate は small bound, e.g. `30` frames 程度
  - full の場合は stale non-keyframe を drop し、latest keyframe / latest selected candidate を優先する
- decoded frame output queue/cache:
  - per slot bounded queue
  - first candidate は `latest decoded by frame_id` + small time-ordered queue
  - memory upper bound は `640 * 360 * 4 * queued_frames * real_slots` を基準に明示する
  - 例: `921600 bytes * 30 frames * 2 slots = 約55MB` に metadata overhead が加わる
- stale discard:
  - targetTime より十分古く、display policy が使わない decoded frame は discard
  - frame_id が render loop で今後参照されないと判断できるものも discard
  - discard は decoder runtime 側の memory pressure policy とし、同期判断は switcher display policy に残す
- latest-only vs target-time queue:
  - latest-only は実装が簡単だが、targetTime selection と相性が悪く、未来 frame を出す危険がある
  - first design は target-time queue を優先し、minimum lookup は `by frame_id` と `latest <= targetTime` にする
  - first implementation が重い場合、one slot の `by frame_id cache + latest decoded` から始める

## fallback policy
- startup failure:
  - FFmpeg spawn failure / pipe creation failure / unsupported args は continuous runtime disabled
  - render loop は one-shot path または current placeholder policyへ戻る
- stdout stall:
  - reader thread が一定時間 raw frame を読めない、かつ input queue / correspondence queue に backlog がある場合 stall
  - first occurrence は decoder restart 候補
  - repeated stall は runtime disabled
- input write failure:
  - writer thread の stdin write error は process failure扱い
  - process stop -> keyframe wait -> restart
  - restart 失敗時は runtime disabled
- frame_id mismatch suspicion:
  - correspondence queue underflow / output ahead / persistent lag / impossible output count は high-risk
  - first slice では immediate runtime disabled + one-shot fallback を優先し、silent wrong-frame display を避ける
- one-shot fallback:
  - first implementationでは必ず残す
  - fallbackを使った render は `render_used_one_shot_fallback_count` で可視化する
  - fallback は continuous runtime disable 時の escape hatch とし、continuous と one-shot の同時 decode storm を避ける
- restart policy:
  - per slot runtime restart count を持つ
  - restart 後は keyframe wait state
  - restart storm 防止のため、同 run 内の disable threshold を持つ
- runtime disable:
  - CLI flag / config flag で continuous-stream decoder を完全 disable できる
  - runtime failureでも disabled state に落とせる
  - disabled state では current one-shot path と同等に戻す

## diagnostics
first implementationで summary に追加したい候補:

- `continuous_decode_enabled`
- `continuous_decode_input_frame_count`
- `continuous_decode_output_frame_count`
- `continuous_decode_queue_len`
- `continuous_decode_dropped_stale_count`
- `continuous_decode_frame_id_lag`
- `continuous_decode_stdout_read_elapsed_ms`
- `continuous_decode_stall_count`
- `continuous_decode_restart_count`
- `continuous_decode_fallback_to_one_shot_count`
- `render_used_continuous_decoded_count`
- `render_used_one_shot_fallback_count`

追加候補:

- `continuous_decode_runtime_disabled`
- `continuous_decode_runtime_disabled_reason`
- `continuous_decode_input_queue_drop_count`
- `continuous_decode_output_queue_drop_count`
- `continuous_decode_keyframe_wait_count`
- `continuous_decode_bootstrap_keyframe_count`
- `continuous_decode_frame_id_mismatch_suspicion_count`
- `continuous_decode_stdout_reader_error_count`
- `continuous_decode_stdin_write_error_count`
- `continuous_decode_per_slot_input_counts`
- `continuous_decode_per_slot_output_counts`
- `continuous_decode_per_slot_queue_lens`

読み方:
- `continuous_decode_input_frame_count` と `output_frame_count` の差は decoder lag の coarse signal
- `continuous_decode_frame_id_lag` は selected frame_id と latest decoded frame_id の差を読む
- `render_used_continuous_decoded_count` は render loop が one-shot wait を避けられた回数
- `render_used_one_shot_fallback_count` が高い場合、continuous path はまだ hot path になっていない

## 最小 implementation slice
docs-only後に実装する場合の最小候補:

1. two-real preview loop 専用の opt-in flag を追加する
   - 例: `--enable-continuous-stream-decoder`
   - existing `--disable-persistent-decoder` とは別物として扱う
2. real slot 1 つだけ continuous decoder runtime を持つ
   - player1 / slot0 から開始
   - slot1 は current one-shot path のまま
   - summary で continuous slot と one-shot slot を比較する
3. per-slot input queue / correspondence queue / decoded output queue を caller-owned state として実装する
4. startup は keyframe wait にし、non-keyframe だけでは decoder を開始しない
5. render loop は selected frame_id の decoded cache lookup を試し、無ければ one-shot fallback または hold/placeholder に落とす
6. one-shot path、current request/response persistent path、scaled decode output path は削除しない
7. diagnostics は上記 minimum set に限定し、4-client / distributed-PC / GPU decode / protocol変更には進まない

## out of scope
- request/response persistent decoder の復活
- server / client / protocol code の変更
- GPU decode
- 4-client continuous decode
- distributed-PC actual run
- OBS advanced control
- Production Readiness PASS 判定
- FPS原因の単一断定
- H.264 wire format変更
- audio integration

## 完了条件
- continuous-stream decoder は request/response persistent decoder と別設計として扱う
- two-real preview loop 限定の最小導入設計がある
- frame_id correspondence / queue / fallback / diagnostics / implementation slice が整理されている
- code変更は次 step 以降に分離する
- Production Readiness は FAIL 継続
