<!-- stream-sync/docs/operations/continuous-stream-decoder-plan.md -->

# Continuous Stream Decoder Plan

最終更新: 2026-05-20

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

## FFmpeg Runtime Diagnostics Rerun
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-133514`
- PASS:
  - opt-in propagation:
    - `continuous_decode_config_enabled=true`
    - `continuous_decode_runtime_enabled=true`
    - `continuous_decode_slot0_enabled=true`
  - continuous runtime exists and stays alive:
    - `continuous_decode_process_running=true`
    - `continuous_decode_process_exit_status=none`
  - stdin write:
    - `continuous_decode_stdin_write_count=10`
    - `continuous_decode_stdin_write_bytes_total=659384`
    - `continuous_decode_stdin_write_error_count=0`
    - `continuous_decode_last_stdin_write_error=none`
  - stderr reader / stderr:
    - `continuous_decode_stderr_reader_alive=true`
    - `continuous_decode_stderr_bytes_total=0`
    - `continuous_decode_ffmpeg_stderr_summary=none`
  - stdout reader state:
    - `continuous_decode_stdout_read_attempt_count=1`
    - `continuous_decode_stdout_read_in_progress=true`
- FAIL:
  - continuous stdout output:
    - `continuous_decode_output_frame_count=0`
    - `continuous_decode_lookup_miss_reason_counts=exact_key_missing:0|queue_empty:0|runtime_disabled:0|output_pending:10|frame_id_lagging:0|unknown:0`
    - `continuous_decode_first_input_to_first_output_elapsed_ms=none`
    - `continuous_decode_first_input_to_now_elapsed_ms=11453`
  - continuous render consumption:
    - `render_used_continuous_decoded_count=0`
    - `render_used_one_shot_fallback_count=10`
  - FPS improvement attributable to continuous path:
    - `effective_render_fps_after_first_render=20.451` is not attributable to continuous decoded frames because output/render consumption stayed `0`
  - Production Readiness: FAIL
- input shape:
  - sparse feed remains visible:
    - `continuous_decode_input_frame_id_min=4`
    - `continuous_decode_input_frame_id_max=342`
    - `continuous_decode_input_frame_id_gap_max=38`
    - `continuous_decode_input_frame_id_gap_total=338`
    - `continuous_decode_input_non_consecutive_count=9`
  - all accepted continuous input still carries keyframe/parameter-set evidence:
    - `continuous_decode_input_keyframe_count=10`
    - `continuous_decode_input_non_keyframe_count=0`
    - `continuous_decode_input_has_sps_count=10`
    - `continuous_decode_input_has_pps_count=10`
    - `continuous_decode_input_has_idr_count=10`
    - `continuous_decode_input_has_non_idr_vcl_count=0`
    - `continuous_decode_last_input_payload_nal_kinds=sps+pps+idr+idr+idr+idr+idr+idr+idr+idr`
- one-shot comparison:
  - one-shot fallback remains the safety path and succeeds on the same scaled output shape:
    - `one_shot_decode_attempt_count=19`
    - `one_shot_decode_elapsed_ms=1175`
    - `one_shot_decode_output_width=640`
    - `one_shot_decode_output_height=360`
    - `one_shot_decode_scaled_output_enabled=true`
  - raw BGRA frame boundary remains `640 * 360 * 4 = 921600` bytes
- interpretation:
  - main evidence is `stdin write success / process alive / stderr none / stdout read in progress / output 0`
  - because every accepted continuous input includes SPS/PPS/IDR and no non-IDR VCL was observed, P-frame reference missing alone is not enough to explain output `0`
  - current suspicion shifts from payload validity toward continuous FFmpeg runtime semantics: stdin kept open, parser/probing/buffering, lack of EOF/flush, lack of low-latency/probe args, stdout reader waiting for a full raw frame, or stderr being too quiet under `-loglevel error`

## FFmpeg Args / Read Boundary Findings
- exact one-shot args in the scaled two-real path:
  - `ffmpeg -hide_banner -loglevel error -f h264 -i pipe:0 -frames:v 1 -vf scale=640:360:flags=neighbor -f rawvideo -pix_fmt bgra pipe:1`
- exact continuous args in the latest rerun:
  - `ffmpeg -hide_banner -loglevel error -f h264 -i pipe:0 -vf scale=640:360:flags=neighbor -f rawvideo -pix_fmt bgra pipe:1`
- exact diff:
  - one-shot has `-frames:v 1`; continuous does not
  - one-shot writes one access unit, drops stdin to send EOF, reads one raw frame, then waits for process exit
  - continuous writes access units to the same stdin handle and keeps it open; stdout reader loops with `read_exact(expected_len)` and only reports after a full `921600` byte raw BGRA frame is available
  - both use the same scale filter, output pixel format, rawvideo muxer, and `-loglevel error`
- expectation:
  - an open-stdin continuous H.264 Annex B stream to rawvideo stdout is a reasonable FFmpeg process shape in principle, but current evidence shows this exact args/lifetime shape does not emit a raw frame in the observed run
  - `-frames:v 1` plus stdin EOF may be helping one-shot flush parser/decoder output; continuous lacks that EOF boundary by design
  - current reader has no partial-byte counter, so `stdout_read_in_progress=true` only proves it is waiting for a full frame, not whether FFmpeg emitted zero bytes or a partial raw frame
  - `-loglevel error` may hide useful warnings about probing, buffering, parser delay, or decode-drop behavior
- next minimal experiment design:
  - keep default continuous args unchanged
  - add an opt-in experimental args toggle only if code changes proceed, e.g. `--continuous-decoder-low-latency-args`
  - candidate pre-input args:
    - `-fflags nobuffer`
    - `-flags low_delay`
    - `-analyzeduration 0`
    - `-probesize 32`
  - candidate output/logging args:
    - `-flush_packets 1`
    - temporary bounded `-loglevel warning` or `-loglevel info`
  - pair the toggle with diagnostics:
    - `continuous_decode_ffmpeg_loglevel`
    - `continuous_decode_ffmpeg_low_latency_args_enabled`
    - `continuous_decode_ffmpeg_probe_args_enabled`
    - `continuous_decode_stdout_partial_bytes_read`
    - `continuous_decode_stdout_first_byte_seen`
    - `continuous_decode_stdout_first_byte_elapsed_ms`
  - do not use this experiment to implement latest decoded fallback, targetTime-aware lookup, slot0 per-client feed/drain, slot1 rollout, or 4-client rollout

## Experimental Low-Latency Args Toggle Implementation
- 2026-05-19 implementation slice added a two-real preview loop only opt-in flag:
  - `--continuous-decoder-low-latency-args`
  - intended usage with the existing continuous opt-in:
    - `--disable-persistent-decoder --enable-continuous-stream-decoder --continuous-decoder-low-latency-args`
- default behavior:
  - default continuous FFmpeg args remain unchanged:
    - `ffmpeg -hide_banner -loglevel error -f h264 -i pipe:0 -vf scale=640:360:flags=neighbor -f rawvideo -pix_fmt bgra pipe:1`
  - continuous feeding, exact lookup, one-shot fallback, pixel format, and decoded queue/cache behavior are unchanged
- opt-in behavior:
  - `-loglevel warning`
  - pre-input:
    - `-fflags nobuffer`
    - `-flags low_delay`
    - `-analyzeduration 0`
    - `-probesize 32`
  - output-side:
    - `-flush_packets 1`
  - `continuous_decode_ffmpeg_args_summary` includes the selected args so the human rerun can verify the variant actually ran
- added summary diagnostics:
  - `continuous_decode_ffmpeg_low_latency_args_enabled`
  - `continuous_decode_ffmpeg_probe_args_enabled`
  - `continuous_decode_ffmpeg_loglevel`
  - `continuous_decode_stdout_first_byte_seen`
  - `continuous_decode_stdout_first_byte_elapsed_ms`
  - `continuous_decode_stdout_partial_bytes_read`
  - `continuous_decode_stdout_partial_read_count`
  - `continuous_decode_stdout_expected_frame_bytes`
  - `continuous_decode_stdout_read_waiting_for_full_frame`
- stdout reader boundary:
  - the reader still waits for a full raw BGRA frame before producing a decoded output event
  - for the current scaled path, expected frame bytes are `640 * 360 * 4 = 921600`
  - partial-byte diagnostics expose whether FFmpeg has emitted no stdout bytes or has emitted less than one full frame while the reader remains blocked
- this slice intentionally does not change:
  - slot1 continuous decode
  - 4-client rollout
  - server / client / protocol code
  - request/response persistent decoder
  - GPU decode
  - one-shot fallback
  - pixel format
  - latest decoded fallback
  - targetTime-aware decoded queue lookup
  - slot0 per-client feed/drain policy

## Low-Latency / Probe Args Runtime Result
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-171331`
- PASS:
  - continuous opt-in propagation:
    - `continuous_decode_config_enabled=true`
    - `continuous_decode_runtime_enabled=true`
    - `continuous_decode_slot0_enabled=true`
  - low-latency / probe args variant enabled:
    - `continuous_decode_ffmpeg_low_latency_args_enabled=true`
    - `continuous_decode_ffmpeg_probe_args_enabled=true`
    - `continuous_decode_ffmpeg_loglevel=warning`
  - one-shot fallback safety:
    - `render_used_one_shot_fallback_count=15`
    - `continuous_decode_fallback_to_one_shot_count=15`
- PARTIAL PASS:
  - continuous stdout output:
    - previous output-pending runs observed `continuous_decode_output_frame_count=0`
    - this rerun produced `continuous_decode_output_frame_count=11`
    - `continuous_decode_queue_len=11`
    - `continuous_decode_stdout_first_byte_seen=true`
    - `continuous_decode_stdout_first_byte_elapsed_ms=4126`
    - `continuous_decode_first_input_to_first_output_elapsed_ms=5322`
    - `continuous_decode_stdout_partial_bytes_read=0`
    - `continuous_decode_stdout_expected_frame_bytes=921600`
  - interpretation:
    - low-latency / probe args changed the runtime boundary enough for FFmpeg stdout to deliver full raw BGRA frames
    - this is not a continuous render-path success because decoded frames were not consumed by render
- FAIL:
  - continuous render consumption:
    - `render_used_continuous_decoded_count=0`
    - `continuous_decode_lookup_hit_count=0`
    - `continuous_decode_lookup_miss_count=15`
    - `continuous_decode_lookup_miss_reason_counts=exact_key_missing:0|queue_empty:0|runtime_disabled:0|output_pending:15|frame_id_lagging:0|unknown:0`
    - `render_used_one_shot_fallback_count=15`
  - FPS improvement attributable to continuous path:
    - `effective_render_fps_after_first_render=13.627`
    - because `render_used_continuous_decoded_count=0`, this FPS is not attributed to continuous decoded-frame consumption
  - Production Readiness: FAIL
- stale decoded frame evidence:
  - requested selected frame is far ahead of latest continuous decoded output:
    - `continuous_decode_requested_frame_id=535`
    - `continuous_decode_latest_decoded_frame_id=386`
    - `continuous_decode_requested_minus_latest_lag=149`
    - `continuous_decode_frame_id_lag=173`
  - decoded queue contains only older frames relative to the requested frame:
    - `continuous_decode_queue_oldest_frame_id=4`
    - `continuous_decode_queue_newest_frame_id=386`
    - `continuous_decode_stale_frame_available_count=11`
    - `continuous_decode_output_frame_id_min=4`
    - `continuous_decode_output_frame_id_max=386`
  - continuous input remains sparse / render-demand driven:
    - `continuous_decode_input_frame_count=15`
    - `continuous_decode_input_frame_id_min=4`
    - `continuous_decode_input_frame_id_max=535`
    - `continuous_decode_input_frame_id_gap_max=66`
    - `continuous_decode_input_frame_id_gap_total=531`
    - `continuous_decode_input_non_consecutive_count=14`
    - `continuous_decode_input_keyframe_count=15`
    - `continuous_decode_input_non_keyframe_count=0`
  - output correspondence lag remains visible:
    - `continuous_decode_output_pending_correspondence_count=3`
- stderr note:
  - `continuous_decode_ffmpeg_stderr_summary=[in#0/h264_...] Stream #0: not enough frames to estimate rate; consider increasing probesize`
  - current evidence should be read as args-variant output progress plus stale decode lag, not as full FFmpeg runtime success
- current interpretation:
  - low-latency / probe args are useful enough to keep as an opt-in diagnostic/runtime variant
  - output can now be produced, but the current render-demand sparse feed cannot keep latest decoded output close enough to the selected frame
  - exact `frame_id` lookup cannot hit because requested frame `535` is far ahead of latest decoded frame `386`
  - decoded queue has stale frames available, but silently displaying latest decoded frame would risk showing old video out of sync
  - latest decoded fallback is therefore held out of the next candidate until a targetTime/staleness policy proves it is safe
  - next work should move docs-first to slot0 per-client continuous feed/drain policy, not to latest decoded fallback

## Slot0 Per-Client Continuous Feed / Drain Policy Draft
- source of truth:
  - detailed first implementation boundary is split into `docs/operations/continuous-feed-drain-plan.md`
  - this section remains the short rationale inside the broader continuous-stream decoder plan
- goal:
  - stop treating continuous decoder input as a render-demand side effect of exact selected-frame cache miss
  - feed slot0 from a per-client stream source continuously enough that decoded output can stay near targetTime / selected frame instead of lagging by hundreds of frame ids
  - keep this as a docs-first design until the minimum policy is clear
- current render-demand feed:
  - scheduler selects one encoded frame for the current render tick
  - decode hook checks exact selected `client_id + run_id + frame_id` cache key
  - on miss, it enqueues only that selected access unit into the continuous runtime
  - the same tick falls back to one-shot if exact output is not already available
  - result in latest rerun: sparse input `4 -> 535`, output only through `386`, exact lookup `0`, one-shot fallback `15`
- proposed per-client feed:
  - slot0 owns a continuous feed cursor for its configured `client_id + run_id`
  - each tick, before render decode lookup, the feed boundary reads a bounded batch of consecutive or nearest-available queued encoded frames from the handoff/source side
  - the feed boundary prefers monotonic `frame_id` progression and avoids enqueueing duplicate frame ids already accepted by the continuous runtime
  - targetTime selection still decides what render wants to display; the feed boundary only keeps the decoder warm and near the source stream
- handoff source access:
  - first design should stay switcher-side and reuse existing handoff/source abstractions where possible
  - do not move targetTime selection to server
  - do not change client/server UDP protocol
  - if current named-pipe handoff exposes only one read per request, the first design may need a small switcher-side feed helper that issues bounded repeated reads for slot0 only
  - a hot-path queue snapshot or server push stream remains out of scope unless repeated one-frame reads prove insufficient
- queue / backpressure / drop policy:
  - input queue is per slot and bounded by frame count
  - first policy target: keep roughly one second or less of 30fps input, e.g. `30` accepted access units, unless runtime evidence suggests a smaller bound
  - when full, drop oldest frames that are already older than the feed cursor / render target window
  - prefer keeping the newest decodable keyframe plus recent frames over preserving every stale access unit
  - never block render loop waiting for feed queue space
  - expose drop counts before using drops as a tuning signal
- decoder input feed cadence:
  - feed at most a bounded batch per render tick, rather than unbounded catch-up
  - initial docs candidate: enqueue up to `N` new slot0 access units per tick, where `N` starts small and is measured against pipe/write pressure
  - feed should happen before exact cache lookup drain so freshly decoded output from previous ticks is visible
  - writer thread remains the owner of FFmpeg stdin writes; render thread only enqueues or skips
- targetTime / latest decoded / exact frame_id handling:
  - exact selected `frame_id` cache hit remains the only safe continuous render consumption in the current code path
  - latest decoded fallback is not the next candidate because latest decoded frame may be stale by `149` frame ids or more
  - targetTime-aware decoded queue lookup remains a future policy, but should require explicit maximum staleness and no-future-frame guards before it can render old decoded frames
  - decoder runtime should not decide sync; it should provide decoded frames plus metadata and diagnostics
- one-shot fallback:
  - keep one-shot fallback as the safety path for exact miss, runtime disabled, feed stall, or suspicious lag
  - first feed/drain slice should reduce fallback count only if exact decoded frames become available naturally
  - do not delete one-shot fallback while continuous output is stale or unconsumed
- diagnostics needed for first feed/drain slice:
  - feed attempts / accepted / skipped duplicate / dropped stale counts
  - feed batch size and per-tick feed elapsed
  - source read count and source no-frame/waiting/error counts for feed
  - input queue high-water mark and drop reason counts
  - latest fed frame id, latest requested frame id, latest decoded frame id, requested-minus-latest lag
  - exact hit count, stale decoded available count, and fallback-to-one-shot count remain mandatory
- first implementation slice boundary, if this design is accepted later:
  - slot0 only
  - two-real preview loop only
  - opt-in continuous decoder only
  - docs/tests/diagnostics first, no slot1 continuous and no 4-client widening
  - preserve exact lookup and one-shot fallback
  - no latest decoded fallback and no targetTime-aware render consumption in the first feed/drain implementation

## Slot0 Bounded Feed Helper First Slice
2026-05-19 implementation status:

- implemented for two-real preview loop only
- implemented for slot0 configured `client0_id + run0_id` only
- runs only when the opt-in continuous stream decoder is enabled
- runs before validation/decode/render
- reads `PreviewOldest` and attempts at most `2` access units per preview-loop tick
- enqueues accepted access units into the existing slot0 continuous runtime input path
- advances the source with guarded `ConsumeOldest` only after enqueue success
- skips targetTime-future oldest frames
- keeps exact selected-frame lookup as the only continuous render consumption path
- keeps render-demand enqueue as fallback on exact miss
- keeps one-shot fallback

Added diagnostics:

- `continuous_feed_enabled`
- `continuous_feed_attempt_count`
- `continuous_feed_handoff_request_count`
- `continuous_feed_frame_received_count`
- `continuous_feed_no_frame_count`
- `continuous_feed_handoff_error_count`
- `continuous_feed_enqueued_count`
- `continuous_feed_skipped_count`
- `continuous_feed_skip_reason_counts`
- `continuous_feed_dropped_stale_input_count`
- `continuous_feed_latest_received_frame_id`
- `continuous_feed_latest_enqueued_frame_id`
- `continuous_decode_input_from_feeder_count`
- `continuous_decode_input_from_render_demand_count`
- `continuous_decode_feeder_lag_to_selected`
- `continuous_decode_render_exact_hit_count`
- `continuous_decode_render_miss_stale_count`
- `continuous_decode_render_miss_not_ready_count`

This slice intentionally still does not implement latest decoded fallback, targetTime-aware decoded render consumption, slot1 continuous decode, 4-client continuous decode, server/client/protocol changes, request/response persistent decoder revival, GPU decode, or one-shot fallback removal. Production Readiness remains FAIL.

## Bounded Feed Helper Runtime Result
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-202043`
- PASS:
  - continuous opt-in: `continuous_decode_config_enabled=true`
  - continuous runtime: `continuous_decode_runtime_enabled=true`
  - slot0 enabled: `continuous_decode_slot0_enabled=true`
  - low-latency args: `continuous_decode_ffmpeg_low_latency_args_enabled=true`
  - bounded feed helper: `continuous_feed_enabled=true`
  - feeder as main input: `continuous_decode_input_from_feeder_count=368`
  - render-demand sparse feed reduced: `continuous_decode_input_from_render_demand_count=4`
  - feeder lag to selected: `continuous_decode_feeder_lag_to_selected=0`
- PASS / PARTIAL PASS:
  - continuous input: `continuous_decode_input_frame_count=372`
  - continuous output: `continuous_decode_output_frame_count=340`
  - decoded queue length: `continuous_decode_queue_len=30`
  - stale decoded drops: `continuous_decode_dropped_stale_count=310`
- FAIL:
  - continuous render consumption: `render_used_continuous_decoded_count=0`
  - exact render hit: `continuous_decode_render_exact_hit_count=0`
  - exact miss stale: `continuous_decode_render_miss_stale_count=12`
  - exact miss not-ready: `continuous_decode_render_miss_not_ready_count=2`
  - one-shot fallback use: `continuous_decode_fallback_to_one_shot_count=14` / `render_used_one_shot_fallback_count=14`
  - Production Readiness: FAIL

Key interpretation:

- The bounded feed helper did its job: slot0 continuous input is no longer mainly fed by render-demand exact misses.
- Continuous decoder output is available in volume.
- Render still uses `0` continuous decoded frames because render consumption is exact selected-frame lookup only.
- Requested frame and latest decoded frame still have lag:
  - `continuous_decode_requested_frame_id=459`
  - `continuous_decode_latest_decoded_frame_id=426`
  - `continuous_decode_requested_minus_latest_lag=40`
  - `continuous_decode_frame_id_lag=42`
  - decoded queue range `390..426`
- This moves the next design question to targetTime-aware / bounded-lag decoded queue lookup.
- Unbounded latest decoded fallback remains unsafe because it can show stale frames and break the sync-first goal.
- One-shot fallback remains mandatory while bounded lookup policy is designed and tested.

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
- render loop の lookup 方針は current first slice では selected encoded frame の `client_id + run_id + frame_id` に一致する decoded frame を preferred / effectively only safe consumption path とする
- same source の latest decoded frame fallback は、latest rerun で `requested_frame_id=535` に対して `latest_decoded_frame_id=386` まで stale になり得ることが分かったため、現時点では次候補から外す
- future targetTime-aware decoded queue lookup を検討する場合は、targetTime より未来に進まないことに加え、最大 staleness / frame_id lag guard を持つまでは render に使わない
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
- `continuous_decode_ffmpeg_loglevel`
- `continuous_decode_ffmpeg_low_latency_args_enabled`
- `continuous_decode_ffmpeg_probe_args_enabled`
- `continuous_decode_stdout_partial_bytes_read`
- `continuous_decode_stdout_first_byte_seen`
- `continuous_decode_stdout_first_byte_elapsed_ms`
- `continuous_decode_stdout_partial_read_count`
- `continuous_decode_stdout_expected_frame_bytes`
- `continuous_decode_stdout_read_waiting_for_full_frame`

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

次に優先する docs/design evidence:

1. targetTime-aware / bounded-lag decoded queue lookup:
   - exact selected-frame lookup が miss した後、同じ source の decoded queue から targetTime 以前かつ bounded lag 内の候補を探す
   - unbounded latest decoded fallback は stale frame 表示リスクがあるため採用しない
   - `docs/operations/continuous-decoded-lookup-plan.md` を source of truth とする
2. stale decoded frame safety:
   - `requested_frame_id=459` / `latest_decoded_frame_id=426` / lag `40` の evidence では、guard なし fallback は unsafe
   - exact frame_id lookup は最優先で維持し、bounded-lag lookup は second choice に限定する
3. diagnostics for bounded lookup:
   - bounded lookup hit / rejected stale / rejected not-ready / fallback-to-one-shot を first implementation candidate に含める

この次 slice も docs-first とし、slot1 continuous 化、4-client 化、default FFmpeg args / pixel format変更、one-shot fallback 削除、無制限 latest decoded fallback には進めない。

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

1. `docs/operations/continuous-decoded-lookup-plan.md` に沿って bounded-lag decoded queue lookup の first implementation slice を切る
2. first implementation scope を slot0 / two-real preview loop / opt-in continuous decoder / diagnostics-first に限定する
3. lookup order は exact selected-frame lookup first、bounded-lag decoded lookup second、one-shot fallback third とする
4. targetTime より未来の decoded frame は表示しない
5. allowed lag threshold と stale guard は conservative default から始め、runtime evidence 後に調整する
6. one-shot fallback は必ず残す

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
