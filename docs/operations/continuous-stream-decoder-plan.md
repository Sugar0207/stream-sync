<!-- stream-sync/docs/operations/continuous-stream-decoder-plan.md -->

# Continuous Stream Decoder Plan

最終更新: 2026-05-19

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

## current implementation status
- 2026-05-18 first implementation slice として、two-real preview loop 専用の opt-in `--enable-continuous-stream-decoder` を追加した
- scope は first configured real source、つまり command 引数上の `client0_id/run0_id` に限定する。slot1 / second real source は one-shot decode のまま残す
- runtime は request/response persistent decoder helper を復活させず、continuous 専用に input writer thread、stdout raw BGRA reader thread、frame_id correspondence queue、decoded cache/key order を分けて持つ
- startup は selected access unit が `has_sps && has_pps && has_idr` を満たすまで continuous process を開始せず、one-shot fallback に倒す
- output queue / cache は first slice では selected `frame_id` の exact lookup と latest decoded cache reuse に留め、targetTime 厳密 selection は future slice に残す
- memory upper bound は decoded cache key order `30` frames とし、bound 超過時は stale decoded frame を discard する
- runtime rerun は未実施。build validation 通過後、人間側が `S:\stream-sync` で opt-in rerun して summary diagnostics を比較する
- latest human rerun `S:\stream-sync\manual-logs\two-client-render-rerun-20260518-141625` は `frames_attempted=300` / `render_failures=0` まで到達しており crash ではなく bounded loop natural exit 寄り。ただし `continuous_decode_config_enabled=false` だったため continuous-stream decoder opt-in run ではない
- opt-in rerun では `--four-view-two-real-handoff-preview-loop ... [frames] [preview-latest-decodable] --disable-persistent-decoder --enable-continuous-stream-decoder` のように、required args と optional read-mode の後ろへ flags を付ける形を推奨する
- parser 上は optional flags は `[frames]` の後ろであれば read-mode の前後どちらにも置ける。ただし docs / copy-paste command では末尾に flags を並べ、summary で `continuous_decode_config_enabled=true` を first gate として確認する

## First Opt-In Runtime Result
- latest continuous opt-in rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260518-235217`
- PASS / PARTIAL PASS:
  - continuous opt-in flag propagation: PASS
    - `continuous_decode_config_enabled=true`
    - `continuous_decode_runtime_enabled=true`
    - `continuous_decode_slot0_enabled=true`
  - continuous runtime created: PASS
    - `continuous_decode_runtime_disabled=false`
    - `continuous_decode_runtime_disabled_reason=none`
    - `continuous_decode_restart_count=0`
  - continuous input feeding: PASS
    - `continuous_decode_input_frame_count=22`
  - continuous stdout output: PARTIAL PASS
    - `continuous_decode_output_frame_count=10`
    - `continuous_decode_queue_len=10`
    - `continuous_decode_stdout_read_elapsed_ms=20306`
    - `continuous_decode_stall_count=1`
    - `continuous_decode_dropped_stale_count=0`
  - one-shot fallback safety: PASS
    - `continuous_decode_fallback_to_one_shot_count=22`
    - `render_used_one_shot_fallback_count=22`
    - `one_shot_decode_attempt_count=44`
- FAIL:
  - continuous render consumption: FAIL
    - `render_used_continuous_decoded_count=0`
  - FPS improvement: FAIL
    - `effective_render_fps_after_first_render=10.887`
  - Production Readiness: FAIL
- Additional runtime context:
  - `continuous_decode_frame_id_lag=362`
  - `one_shot_decode_elapsed_ms=6354`
  - `one_shot_decode_first_byte_slow_count=8`
  - `one_shot_decode_output_read_slow_count=16`
  - `one_shot_decode_input_write_outlier_count=14`
  - `one_shot_decode_input_payload_bytes_avg=256049.864`
  - `quad_view_compose_elapsed_ms=1432`
  - `gdi_paint_wait_elapsed_ms=0`

## Render-Consumption Code-Path Findings
- current slot0 continuous path is exact-key only:
  - `TimedSwitcherH264DecodeRuntime::decode_annex_b_h264` builds a decode cache key from output width / height plus `source_identity(client_id, run_id, frame_id)`
  - continuous decoded frames are inserted into the same `decoded_cache` under that exact key
  - render increments `render_used_continuous_decoded_count` only when the requested key is already in cache and `continuous_decoded_keys` contains that exact key
- miss behavior:
  - on cache miss for the configured slot0 source, the runtime enqueues the currently selected access unit into the continuous input queue
  - it drains any already available output immediately after enqueue
  - if the same exact key is still absent, it increments `continuous_decode_fallback_to_one_shot_count` and `render_used_one_shot_fallback_count`, then executes the normal one-shot decode path
- There is no current latest-decoded fallback:
  - the first slice does not search for "latest decoded frame <= targetTime"
  - it does not use "latest decoded frame for the same source" when the exact requested `frame_id` is missing
  - therefore decoded queue/cache can contain frames while current render still falls back if selected `frame_id` has moved ahead
- `continuous_decode_frame_id_lag` meaning:
  - updated from the maximum observed `selected_frame_id.saturating_sub(latest_decoded_frame_id)` for the configured source
  - `362` means the latest continuous decoded output observed by the runtime was up to 362 frame ids behind the selected frame requested by render
  - it is a coarse lag signal, not a proof of a single root cause
- Likely contributing structures, still not single-cause:
  - input feeding is render-demand-driven: a frame is enqueued only after render asks to decode that selected frame
  - selected frame exact match is required for render consumption
  - output produced later may correspond to a frame that is no longer selected by the next render tick
  - current summary does not expose requested frame id, latest decoded frame id, decoded queue oldest/newest frame id, or correspondence backlog
- `continuous_decode_output_frame_count=10` vs `input_frame_count=22` can mean decoder lag, stdout reader blocking, FFmpeg buffering/delay, or input/output correspondence backlog. Current diagnostics cannot separate those yet.
- `continuous_decode_stdout_read_elapsed_ms=20306` is accumulated reader-thread `read_exact(expected_len)` time for output frames. With output count `10`, it indicates the reader spent substantial wall time waiting for raw BGRA frames, but because it is accumulated inside the reader thread it should not be read as direct render-loop blocking time.
- `continuous_decode_stall_count=1` is currently incremented when `selected_frame_id - latest_decoded_frame_id` exceeds the queue bound (`30`). It is therefore a lag threshold observation, not a process restart or runtime disable event.

## Lookup Diagnostics Implementation
- 2026-05-19 diagnostics-only slice で、slot0 continuous lookup と frame_id correspondence の summary fields を追加した
- added fields:
  - `continuous_decode_lookup_hit_count`
  - `continuous_decode_lookup_miss_count`
  - `continuous_decode_lookup_miss_reason_counts`
    - `exact_key_missing`
    - `queue_empty`
    - `runtime_disabled`
    - `output_pending`
    - `frame_id_lagging`
    - `unknown`
  - `continuous_decode_requested_frame_id`
  - `continuous_decode_latest_decoded_frame_id`
  - `continuous_decode_requested_minus_latest_lag`
  - `continuous_decode_queue_oldest_frame_id`
  - `continuous_decode_queue_newest_frame_id`
  - `continuous_decode_input_frame_id_min`
  - `continuous_decode_input_frame_id_max`
  - `continuous_decode_output_frame_id_min`
  - `continuous_decode_output_frame_id_max`
  - `continuous_decode_output_pending_correspondence_count`
  - `continuous_decode_writer_input_queue_len`
  - `continuous_decode_exact_match_required_count`
  - `continuous_decode_stale_frame_available_count`
- meaning:
  - `continuous_decode_frame_id_lag` remains max observed lag for the run
  - `continuous_decode_requested_minus_latest_lag` is the latest/current requested-minus-latest lag when latest decoded frame id is known
  - `continuous_decode_lookup_hit_count` counts exact continuous decoded cache hits
  - `continuous_decode_lookup_miss_count` counts slot0 continuous exact lookup misses without changing fallback behavior
  - `continuous_decode_stale_frame_available_count` increments when decoded queue has a newest frame older than the requested frame id at miss time
  - `continuous_decode_output_pending_correspondence_count` reads the correspondence queue length held between writer and reader
  - `continuous_decode_writer_input_queue_len` is an atomic approximation of inputs accepted by the runtime but not yet received by the writer thread
- this slice intentionally does not change:
  - exact-match lookup behavior
  - one-shot fallback
  - decoded cache / queue selection policy
  - FFmpeg args
  - pixel format
  - targetTime-aware lookup
  - latest decoded fallback
  - slot1 / 4-client continuous rollout

## Lookup Diagnostics Runtime Result
- latest continuous lookup diagnostics rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-111942`
- PASS:
  - continuous opt-in flag propagation:
    - `continuous_decode_config_enabled=true`
    - `continuous_decode_runtime_enabled=true`
    - `continuous_decode_slot0_enabled=true`
  - continuous runtime created:
    - `continuous_decode_runtime_disabled=false`
  - slot0 input feeding:
    - `continuous_decode_input_frame_count=9`
  - continuous lookup diagnostics:
    - `continuous_decode_lookup_hit_count=0`
    - `continuous_decode_lookup_miss_count=9`
    - `continuous_decode_exact_match_required_count=9`
- FAIL:
  - continuous stdout output:
    - `continuous_decode_output_frame_count=0`
    - `continuous_decode_queue_len=0`
    - `continuous_decode_latest_decoded_frame_id=none`
    - `continuous_decode_output_frame_id_min=none`
    - `continuous_decode_output_frame_id_max=none`
  - continuous render consumption:
    - `render_used_continuous_decoded_count=0`
    - `render_used_one_shot_fallback_count=9`
    - `continuous_decode_fallback_to_one_shot_count=9`
  - FPS improvement attributable to continuous path:
    - `effective_render_fps_after_first_render=18.810`
    - continuous output / render consumption が 0 なので、この FPS は continuous path 成果として扱わない
  - Production Readiness: FAIL
- Key diagnostic result:
  - `continuous_decode_lookup_miss_reason_counts=exact_key_missing:0|queue_empty:0|runtime_disabled:0|output_pending:9|frame_id_lagging:0|unknown:0`
  - `continuous_decode_output_pending_correspondence_count=9`
  - `continuous_decode_writer_input_queue_len=0`
  - writer input queue は空で、correspondence queue に 9 件残っている。writer thread は accepted input を受け取り FFmpeg stdin write まで進めた可能性が高く、pending は writer より後段、つまり FFmpeg decode / stdout raw frame output / reader delivery の間にある
- Frame range:
  - `continuous_decode_requested_frame_id=307`
  - `continuous_decode_input_frame_id_min=4`
  - `continuous_decode_input_frame_id_max=307`
  - `continuous_decode_output_frame_id_min=none`
  - `continuous_decode_output_frame_id_max=none`
  - input 9 件で frame_id が `4 -> 307` まで飛んでおり、continuous decoder に連続 stream ではなく render-demand selected access unit を sparse に渡している疑いが強い
- Lag semantics for this run:
  - `continuous_decode_frame_id_lag` は latest decoded frame id がある時に `requested_frame_id - latest_decoded_frame_id` の max として更新される
  - 今回は `continuous_decode_latest_decoded_frame_id=none` / `continuous_decode_requested_minus_latest_lag=none` なので、lag を計算できる decoded output が 1 件も無い
  - したがって今回の主 evidence は frame_id lag ではなく `output_pending:9` と output count `0`

## Output-Pending Code-Path Findings
- current feed point is render-demand:
  - `TimedSwitcherH264DecodeRuntime::decode_annex_b_h264` は selected frame の cache key を作る
  - configured slot0 source なら既存 continuous output を drain し、requested frame_id を観測する
  - exact cache hit が無ければ、その tick で要求された selected access unit だけを continuous input queue へ enqueue する
  - enqueue 直後に再度 drain して exact key を探し、それでも無ければ one-shot fallback へ進む
- current continuous input is not a background client stream feed:
  - handoff / targetTime selection が選んだ selected frame だけが decode hook に到達する
  - cache hit しない selected frame だけが continuous runtime に渡る
  - selected frame が `4 -> 307` のように飛ぶと、FFmpeg は参照 frame の連続性を失った non-IDR P-frame を受け取る可能性がある
- startup keyframe gate:
  - runtime startup は first input が `has_sps && has_pps && has_idr` を満たす場合だけ session を作る
  - 今回 `input_frame_count=9` なので startup gate は少なくとも一度通過しているが、その後の input が SPS/PPS/IDR を含むか、non-IDR VCL だけかは current summary では見えない
- output pending classification:
  - lookup miss reason `output_pending` は `pending_correspondence_count > 0 || writer_input_queue_len > 0` の時に付く
  - 今回は `writer_input_queue_len=0` かつ `pending_correspondence_count=9` のため、accepted input は writer thread が受け取り correspondence queue に積んだ後、reader が raw BGRA frame を pop できていない
- stdout / stderr visibility gap:
  - stdout reader は `read_exact(expected_len)` 成功後だけ decoded event と `stdout_read_elapsed_ms` を出す
  - 今回 `continuous_decode_stdout_read_elapsed_ms=0` は successful read が無かったという意味で、reader が待っていない証明ではない
  - stderr thread は現在 `read_to_end` して bytes を捨てており、FFmpeg の reference error / decode error summary は summary に出ない
- Current likely issue:
  - continuous input is sparse / render-demand driven
  - H.264 stream continuity is not preserved
  - FFmpeg may be waiting for enough valid stream / reference frames
  - stdout reader has no decoded raw frame to return
  - exact lookup strictness is not the current first problem for this run

## Output-Pending Diagnostics Implementation
- 2026-05-19 diagnostics-only slice で、slot0 continuous output pending の切り分け fields を追加した
- added fields:
  - `continuous_decode_input_frame_id_gap_max`
  - `continuous_decode_input_frame_id_gap_total`
  - `continuous_decode_input_non_consecutive_count`
  - `continuous_decode_input_keyframe_count`
  - `continuous_decode_input_non_keyframe_count`
  - `continuous_decode_input_has_sps_count`
  - `continuous_decode_input_has_pps_count`
  - `continuous_decode_input_has_idr_count`
  - `continuous_decode_input_has_non_idr_vcl_count`
  - `continuous_decode_last_input_payload_nal_kinds`
  - `continuous_decode_ffmpeg_stderr_summary`
  - `continuous_decode_stdout_reader_blocked_count`
  - `continuous_decode_no_output_after_input_count`
  - `continuous_decode_no_output_after_keyframe_count`
  - `continuous_decode_bootstrap_input_count`
  - `continuous_decode_bootstrap_output_count`
  - `continuous_decode_last_input_frame_id`
  - `continuous_decode_last_output_frame_id`
- meaning:
  - `continuous_decode_input_frame_id_gap_max` / `gap_total` are computed from accepted continuous input order, so they expose sparse render-demand feed
  - `continuous_decode_input_non_consecutive_count` increments when consecutive accepted input frame ids are not exactly `+1`
  - input keyframe count uses `has_idr` from the existing Annex B payload inspection helper
  - SPS/PPS/IDR/non-IDR VCL counts use the same existing payload inspection helper and do not change payload handling
  - `continuous_decode_last_input_payload_nal_kinds` keeps only the latest accepted input's compact NAL kinds
  - `continuous_decode_ffmpeg_stderr_summary` keeps a bounded stderr tail (`512` bytes) and sanitizes at summary formatting
  - `continuous_decode_stdout_reader_blocked_count` increments when lookup misses observe pending correspondence, no writer queue backlog, and the reader thread is inside stdout `read_exact`
  - `continuous_decode_no_output_after_input_count` increments on output-pending lookup miss with correspondence backlog
  - `continuous_decode_no_output_after_keyframe_count` increments when output remains pending after at least one IDR input has been accepted
  - bootstrap input/output counts expose whether output appears before or after the first decoded frame
- this slice intentionally does not change:
  - continuous input feed policy
  - exact-match lookup behavior
  - one-shot fallback
  - decoded cache / queue selection policy
  - FFmpeg args
  - pixel format
  - latest decoded fallback
  - targetTime-aware lookup
  - slot0 per-client feed/drain implementation
  - slot1 / 4-client continuous rollout

## All-Keyframe Output-Pending Rerun
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-122239`
- PASS:
  - opt-in propagation: `continuous_decode_config_enabled=true`
  - runtime created: `continuous_decode_runtime_enabled=true`
  - slot0 input feeding: `continuous_decode_slot0_enabled=true` / `continuous_decode_input_frame_count=18`
  - lookup/output-pending diagnostics: `continuous_decode_lookup_miss_count=18` / `continuous_decode_lookup_miss_reason_counts=exact_key_missing:0|queue_empty:0|runtime_disabled:0|output_pending:18|frame_id_lagging:0|unknown:0`
- FAIL:
  - continuous stdout output: `continuous_decode_output_frame_count=0`
  - continuous render consumption: `render_used_continuous_decoded_count=0` / `render_used_one_shot_fallback_count=18`
  - continuous path FPS improvement: `effective_render_fps_after_first_render=12.601` is not attributable to continuous decoded frames
- key observations:
  - writer queue is not the visible backlog: `continuous_decode_writer_input_queue_len=0`
  - correspondence remains pending after input is consumed: `continuous_decode_output_pending_correspondence_count=18`
  - sparse render-demand feed is confirmed: `continuous_decode_input_frame_id_min=4` / `continuous_decode_input_frame_id_max=525` / `continuous_decode_input_frame_id_gap_max=37` / `continuous_decode_input_frame_id_gap_total=521` / `continuous_decode_input_non_consecutive_count=17`
  - all accepted continuous inputs include keyframe/parameter-set evidence: `continuous_decode_input_keyframe_count=18` / `continuous_decode_input_non_keyframe_count=0` / `continuous_decode_input_has_sps_count=18` / `continuous_decode_input_has_pps_count=18` / `continuous_decode_input_has_idr_count=18` / `continuous_decode_input_has_non_idr_vcl_count=0`
  - latest input NAL summary was `continuous_decode_last_input_payload_nal_kinds=sps+pps+idr+idr+idr+idr+idr+idr+idr+idr`
  - stderr did not explain the missing output: `continuous_decode_ffmpeg_stderr_summary=none`
  - reader/pending counters point at stdout output not appearing: `continuous_decode_stdout_reader_blocked_count=16` / `continuous_decode_no_output_after_input_count=17` / `continuous_decode_no_output_after_keyframe_count=17` / `continuous_decode_bootstrap_input_count=18` / `continuous_decode_bootstrap_output_count=0`
- interpretation:
  - `output_pending:18` with writer queue `0` means current first problem is after input has left the writer queue: FFmpeg stdin handling, decode/probing/buffering, rawvideo stdout emission, stdout read, process state, or stderr visibility
  - sparse render-demand feed remains a structural concern, but all inputs carrying SPS/PPS/IDR makes missing non-IDR reference frames insufficient as the only explanation for output `0`
  - next evidence should come from FFmpeg-runtime diagnostics before changing lookup, fallback, or feed architecture

## FFmpeg Runtime Diagnostics Implementation
- 2026-05-19 diagnostics-only slice added runtime boundary fields:
  - `continuous_decode_ffmpeg_args_summary`
  - `continuous_decode_stdin_write_count`
  - `continuous_decode_stdin_write_bytes_total`
  - `continuous_decode_stdin_write_error_count`
  - `continuous_decode_last_stdin_write_error`
  - `continuous_decode_process_running`
  - `continuous_decode_process_exit_status`
  - `continuous_decode_stdout_read_attempt_count`
  - `continuous_decode_stdout_read_in_progress`
  - `continuous_decode_stderr_reader_alive`
  - `continuous_decode_stderr_bytes_total`
  - `continuous_decode_last_stderr_at_ms`
  - `continuous_decode_last_input_write_elapsed_ms`
  - `continuous_decode_last_input_payload_bytes`
  - `continuous_decode_first_input_to_first_output_elapsed_ms`
  - `continuous_decode_first_input_to_now_elapsed_ms`
- meaning:
  - `continuous_decode_ffmpeg_args_summary` records the continuous process command shape without changing args
  - stdin write count/bytes/error/last-error distinguish writer success from broken pipe or pipe backpressure symptoms
  - process running/exit status checks the child process through the live session and preserves a status when observed
  - stdout read attempt/in-progress separates "no successful read" from "reader currently blocked in read_exact"
  - stderr reader alive/bytes/last time separates true empty stderr from a dead/unobserved stderr reader
  - first-input timers show whether FFmpeg is waiting/buffering after accepted input, and whether the first output ever appeared
- this slice intentionally does not change:
  - continuous feed policy
  - FFmpeg args / pixel format
  - exact-match lookup
  - one-shot fallback
  - latest decoded fallback
  - targetTime-aware lookup
  - slot0 per-client feed/drain architecture
  - slot1 / 4-client rollout

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
first implementation で summary に追加済み:

- `continuous_decode_config_enabled`
- `continuous_decode_runtime_enabled`
- `continuous_decode_slot0_enabled`
- `continuous_decode_input_frame_count`
- `continuous_decode_output_frame_count`
- `continuous_decode_queue_len`
- `continuous_decode_dropped_stale_count`
- `continuous_decode_frame_id_lag`
- `continuous_decode_stdout_read_elapsed_ms`
- `continuous_decode_stall_count`
- `continuous_decode_restart_count`
- `continuous_decode_runtime_disabled`
- `continuous_decode_runtime_disabled_reason`
- `continuous_decode_fallback_to_one_shot_count`
- `render_used_continuous_decoded_count`
- `render_used_one_shot_fallback_count`

追加済み:

- `continuous_decode_lookup_miss_count`
- `continuous_decode_lookup_hit_count`
- `continuous_decode_lookup_miss_reason_counts`
- `continuous_decode_latest_decoded_frame_id`
- `continuous_decode_requested_frame_id`
- `continuous_decode_requested_minus_latest_lag`
- `continuous_decode_exact_match_required_count`
- `continuous_decode_stale_frame_available_count`
- `continuous_decode_queue_oldest_frame_id`
- `continuous_decode_queue_newest_frame_id`
- `continuous_decode_input_frame_id_min`
- `continuous_decode_input_frame_id_max`
- `continuous_decode_output_frame_id_min`
- `continuous_decode_output_frame_id_max`
- `continuous_decode_output_pending_correspondence_count`
- `continuous_decode_writer_input_queue_len`
- `continuous_decode_input_frame_id_gap_max`
- `continuous_decode_input_frame_id_gap_total`
- `continuous_decode_input_non_consecutive_count`
- `continuous_decode_input_keyframe_count`
- `continuous_decode_input_non_keyframe_count`
- `continuous_decode_input_has_sps_count`
- `continuous_decode_input_has_pps_count`
- `continuous_decode_input_has_idr_count`
- `continuous_decode_input_has_non_idr_vcl_count`
- `continuous_decode_last_input_payload_nal_kinds`
- `continuous_decode_ffmpeg_stderr_summary`
- `continuous_decode_stdout_reader_blocked_count`
- `continuous_decode_no_output_after_input_count`
- `continuous_decode_no_output_after_keyframe_count`
- `continuous_decode_bootstrap_input_count`
- `continuous_decode_bootstrap_output_count`
- `continuous_decode_last_input_frame_id`
- `continuous_decode_last_output_frame_id`
- `continuous_decode_ffmpeg_args_summary`
- `continuous_decode_stdin_write_count`
- `continuous_decode_stdin_write_bytes_total`
- `continuous_decode_stdin_write_error_count`
- `continuous_decode_last_stdin_write_error`
- `continuous_decode_process_running`
- `continuous_decode_process_exit_status`
- `continuous_decode_stdout_read_attempt_count`
- `continuous_decode_stdout_read_in_progress`
- `continuous_decode_stderr_reader_alive`
- `continuous_decode_stderr_bytes_total`
- `continuous_decode_last_stderr_at_ms`
- `continuous_decode_last_input_write_elapsed_ms`
- `continuous_decode_last_input_payload_bytes`
- `continuous_decode_first_input_to_first_output_elapsed_ms`
- `continuous_decode_first_input_to_now_elapsed_ms`

追加候補:

- `continuous_decode_input_queue_drop_count`
- `continuous_decode_output_queue_drop_count`
- `continuous_decode_keyframe_wait_count`
- `continuous_decode_frame_id_mismatch_suspicion_count`
- `continuous_decode_stdout_reader_error_count`

読み方:
- `continuous_decode_input_frame_count` と `output_frame_count` の差は decoder lag の coarse signal
- `continuous_decode_frame_id_lag` は selected frame_id と latest decoded frame_id の差を読む
- `render_used_continuous_decoded_count` は render loop が one-shot wait を避けられた回数
- `render_used_one_shot_fallback_count` が高い場合、continuous path はまだ hot path になっていない

次に優先する rerun evidence:

1. FFmpeg runtime / pipe status:
   - args summary
   - stdin write count / bytes / error
   - process running / exit status
   - stdout read attempt / in-progress
   - stderr reader alive / bytes / last time
   - first-input-to-output / first-input-to-now elapsed
2. output `0` の切り分け:
   - stdin write が成功しているのに stdout read が in-progress のままか
   - process が生存しているのに stderr bytes が `0` のままか
   - first input から十分な時間が経っても first output が `none` のままか
3. feed policy decision support:
   - selected-frame-only render-demand feed が sparse である evidence は保持
   - all-keyframe input でも output pending が続くかを FFmpeg runtime diagnostics と合わせて読む

この slice は observability-only とし、slot1 continuous 化、4-client 化、FFmpeg args / pixel format変更、one-shot fallback 削除、latest decoded fallback、targetTime-aware lookup 本実装には進めない。

## 最小 implementation slice
2026-05-18 first slice で実装済み:

1. two-real preview loop 専用の opt-in flag を追加する
   - `--enable-continuous-stream-decoder`
   - existing `--disable-persistent-decoder` とは別物として扱う
2. real slot 1 つだけ continuous decoder runtime を持つ
   - command 上の first real source `client0_id/run0_id` から開始
   - second real source は current one-shot path のまま
   - summary で continuous path 使用回数と one-shot fallback 使用回数を比較する
3. input writer / correspondence queue / decoded output event / decoded cache を continuous runtime state として実装する
4. startup は keyframe wait にし、non-keyframe だけでは decoder を開始しない
5. render loop は selected frame_id の decoded cache lookup を試し、無ければ one-shot fallback に落とす
6. one-shot path、current request/response persistent path、scaled decode output path は削除しない
7. diagnostics は上記 minimum set に限定し、4-client / distributed-PC / GPU decode / protocol変更には進まない

future implementation slice:

1. next rerun で FFmpeg runtime が stdin write 後に alive/blocking/no-stderr/no-output なのか、write error / process exit なのかを確認する
2. stdout stall / input write failure / process exit / frame_id mismatch suspicion の runtime disable reason を actual run evidence に合わせて増やす
3. output `0` が feed policy 由来と確認できた場合のみ、slot0 per-client continuous feed/drain policy を別 step で設計する
4. restart policy は first slice の counter visibility から始め、restart storm を避ける threshold を runtime evidence 後に決める

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
- two-real preview loop 限定の opt-in path がある
- first slice は first real source のみ continuous 対象で、second real source は one-shot のまま
- frame_id correspondence / queue / fallback / diagnostics / implementation slice が整理されている
- one-shot fallback が残っている
- Production Readiness は FAIL 継続
