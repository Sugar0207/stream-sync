<!-- stream-sync/docs/operations/todo.md -->

# StreamSync TODO

最終更新: 2026-05-28

このファイルは「現在どこまで終わっていて、次に何をやるか」を確認するための TODO です。  
時系列の作業履歴、判断理由、各回の作業メモは `docs/operations/session-log.md` を正とします。

## 運用ルール
- このファイルを StreamSync の最新版 TODO として扱う
- このファイルには現在位置とタスク一覧を書く
- このファイルには時系列の作業履歴を書かない
- 時系列の作業履歴は `docs/operations/session-log.md` を正とする
- 同じ意味のタスクを複数箇所に重複して書かない
- 完了タスクは `[x]` のまま残してよい
- 未完了タスクは `[ ]` として管理する
- 項目の状態が変わったら必ず更新する
- 大きな仕様変更があれば関連する `docs/requirements` や `docs/architecture` も更新する
- Codex 作業後は、この TODO と `docs/operations/session-log.md` を更新する

---

## 現在位置
- latest reverse-order lag threshold A/B rerun is `manual-logs/two-client-lag-reverse-ab-rerun-20260527-164258` as the current threshold evidence. lag8 vs lag5 is VALID, lag8 is a small PARTIAL PASS and held adoption candidate, and default `8` promotion is HOLD while default `5` remains the current guard
- latest output availability rerun is `manual-logs/two-client-output-availability-rerun-20260527-173716` and is VALID. Client FFmpeg recovered, client1/client2 sent `900` frames each at about `29.538fps` / `28.694fps`, server queued `1800` frames, and continuous feed received/enqueued `453` / `423` frames, so client / server / feed are PASS for this slice
- next continuous-stream decoder main line is output availability / throughput rather than another default threshold move. The diagnostics slice is runtime VALID and points to stale/output backlog rather than not-ready: continuous output is `316` frames at `21.269fps`, pending correspondence is `115` with avg age `1948.809ms`, latest input-output gap is `115`, selected-output gap is `99`, reader full-frame avg is `46.430ms`, and stale availability rejects `238` exceed not-ready `22`
- latest completed correspondence rerun is `manual-logs/two-client-completed-correspondence-rerun-20260528-010504` and is VALID. Client/server/feed remain PASS: client1/client2 sent `900` frames at `29.443fps` / `29.112fps`, server queued `1800` frames, and FFmpeg preflight succeeded. Completed correspondence diagnostics are VALID and show completed outputs are also seconds late: completed latency avg `2624.940ms`, max `5258ms`, latest `5251ms`, slow `301/301`
- output backlog is now the dominant continuous line: continuous output is `301` frames at `17.151fps` while source is about `29fps`; pending correspondence is `137` with avg age `2540.606ms` and max `5300ms`; latest input-output gap is `156`; output lag to selected is `150`; stale rejects `228` exceed not-ready `19`. Threshold tuning alone is insufficient, and Production Readiness remains FAIL
- latest output pipeline A/B rerun is `manual-logs/two-client-output-pipeline-ab-rerun-20260528-014200` and is VALID-ish / useful evidence. `scaled-bgr24` wiring is PASS: mode / FFmpeg pix_fmt / expected bytes changed correctly, stdout bytes/frame dropped from `921600` to `691200`, reader avg improved from `37.968ms` to `17.739ms`, and stdout throughput improved from `24273.288` to `38965.867` bytes/ms. Raw pipe bytes hypothesis is PARTIAL PASS
- end-to-end result favors default BGRA. `scaled-bgr24` output throughput fell `25.816fps -> 22.150fps`, completed latency avg worsened `1309.796ms -> 2037.903ms`, pending age avg worsened `803.227ms -> 1709.438ms`, output lag to selected worsened `46 -> 88`, and bounded lookup hits fell `6 -> 3`. BGR24-to-BGRA conversion cost is the new strong bottleneck candidate: `8636ms / 329 frames ~= 26.25ms/frame`. Keep default-bgra, hold / fail scaled-bgr24 adoption, and keep Production Readiness FAIL
- BGR24 conversion / direct render / scale split docs-first review now lives in `docs/operations/continuous-pixel-conversion-plan.md`. Next code candidate, if selected, should be a narrow opt-in `scaled-bgr24` conversion optimization slice first, preferably buffer reuse or safe scalar conversion. Direct BGR24 render path is wider because current render / compose / GDI / OBS-friendly output contracts are BGRA-oriented, so it stays docs-first only
- 2026-05-28 BGR24 conversion optimization first slice is implemented for `scaled-bgr24` only. The reader now expands BGR24 to BGRA in-place with a safe reverse scalar loop, avoiding the previous extra conversion Vec / append path. Summary adds conversion buffer reuse/allocation counts, bytes-written total/per-frame, and conversion mode; the optimized path should report reuse for the final BGRA frame buffer and no separate conversion buffer allocation. Default BGRA output path remains unchanged and `scaled-bgr24` is still opt-in / adoption HOLD until human rerun evidence exists
- latest good-ish same-PC `2`-client rerun は `manual-logs/two-client-render-rerun-20260518-124418` として扱う。two-real preview loop 限定 scaled one-shot decode output は runtime PASS 継続で、`one_shot_decode_output_width=640`、`one_shot_decode_output_height=360`、`one_shot_decode_expected_output_bytes_per_frame=921600` を維持した
- persistent decoder config-disabled toggle も PASS 継続だった。request/response persistent decoder は過去に `persistent_decode_stdout_read_timeout` で runtime FAIL しているため、引き続き凍結候補として扱い、continuous-stream decoder とは別物として整理する
- latest good-ish rerun の switcher は `effective_render_fps_after_first_render=17.247` で、30fps には未達だった。`decode_attempt_count=26`、`one_shot_decode_elapsed_ms=1893`、`one_shot_decode_first_byte_slow_count=0`、`one_shot_decode_output_read_slow_count=0`、`one_shot_decode_input_write_outlier_count=0` なので、decode attempt frequency / slow first-byte / slow output-read / input-write outlier のいずれか 1 つを主犯とは断定しない
- latest good-ish rerun では `quad_view_compose_elapsed_ms=636`、`gdi_paint_wait_elapsed_ms=12` で、incremental compose と render/GDI は regression guard として残すが、current evidence の first-order culprit とは置かない
- 30fps に向けた next design candidate は one-shot FFmpeg の per-attempt variance と render-loop blocking を外す continuous-stream decoder とする。2026-05-18 first implementation slice では two-real preview loop 専用 opt-in `--enable-continuous-stream-decoder` を追加し、first configured real source だけ continuous path 対象にした
- continuous-stream decoder の source of truth は `docs/operations/continuous-stream-decoder-plan.md`。per-real-slot access unit input queue、stdout reader thread、decoded frame queue/cache、frame_id correspondence queue、fallback/restart/diagnostics、two-real preview loop 限定の最小 implementation slice を整理し、first slice の実装状況も反映済み
- continuous-stream decoder は request/response persistent decoder の復活ではない。`1 request -> 1 response` を待たず、H.264 access unit を stream として投入し、reader thread が raw BGRA stdout を読み続け、render loop は decoded queue/cache を参照する別設計として扱う
- continuous-stream decoder first slice は slot1 / second real source、4-client、server / client / protocol、GPU decode、targetTime 厳密 decoded queue selection には広げていない。one-shot fallback は維持し、Production Readiness は FAIL 継続
- latest human rerun `S:\stream-sync\manual-logs\two-client-render-rerun-20260518-141625` は `frames_attempted=300` / `render_failures=0` まで到達しており crash ではなく bounded loop natural exit 寄り。ただし `continuous_decode_config_enabled=false` / `continuous_decode_runtime_enabled=false` / `continuous_decode_slot0_enabled=false` だったため、continuous-stream decoder opt-in rerun ではない
- latest continuous opt-in rerun は `S:\stream-sync\manual-logs\two-client-render-rerun-20260518-235217` として扱う。`continuous_decode_config_enabled=true` / `continuous_decode_runtime_enabled=true` / `continuous_decode_slot0_enabled=true` / `continuous_decode_input_frame_count=22` / `continuous_decode_output_frame_count=10` で、flag propagation、runtime 作成、slot0 input feeding、stdout output は PASS / PARTIAL PASS として記録する
- 同 rerun では `render_used_continuous_decoded_count=0`、`continuous_decode_fallback_to_one_shot_count=22`、`render_used_one_shot_fallback_count=22`、`continuous_decode_frame_id_lag=362`、`effective_render_fps_after_first_render=10.887` だった。continuous render consumption と FPS improvement は FAIL で、one-shot fallback safety は PASS、Production Readiness は FAIL 継続とする
- current code path では slot0 continuous decoded frame は selected frame の decode cache key exact match でだけ render に使われる。miss 時は selected frame を continuous input queue へ enqueue した直後に同 tick で one-shot fallback へ進み、latest decoded frame fallback / targetTime-aware decoded queue lookup はまだ無い。decoded queue に frame があっても selected `frame_id` が先へ進むと render は continuous frame を使えない
- `continuous_decode_frame_id_lag` は requested selected `frame_id - latest_decoded_frame_id` の最大値として更新される。`362` は latest decoded output が current selected frame より大きく遅れていた coarse signal で、input feeding が render-demand-driven であること、reader output が 10/22 に留まること、exact lookup だけでは追いつけないことの複合候補として扱い、原因を 1 つに断定しない
- 2026-05-19 diagnostics-only slice で slot0 continuous lookup observability を追加した。summary には `continuous_decode_lookup_hit_count` / `continuous_decode_lookup_miss_count` / `continuous_decode_lookup_miss_reason_counts`、requested/latest frame_id、requested-minus-latest current lag、decoded queue oldest/newest frame_id、input/output frame_id min/max、pending correspondence count、writer input queue len、exact-match-required count、stale-frame-available count が出る
- この diagnostics slice は挙動変更なし。slot0 exact match lookup、one-shot fallback、decoded cache / queue selection policy は維持し、latest decoded fallback / targetTime-aware decoded queue lookup / slot1 continuous 化 / 4-client 化には進めていない。Production Readiness は FAIL 継続
- latest continuous lookup diagnostics rerun は `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-111942` として扱う。`continuous_decode_config_enabled=true` / `continuous_decode_runtime_enabled=true` / `continuous_decode_slot0_enabled=true` / `continuous_decode_input_frame_count=9` により opt-in propagation、runtime 作成、slot0 input feeding、lookup diagnostics 自体は PASS とする
- 同 rerun では `continuous_decode_output_frame_count=0` / `continuous_decode_queue_len=0` / `continuous_decode_lookup_hit_count=0` / `continuous_decode_lookup_miss_count=9` / `continuous_decode_lookup_miss_reason_counts=exact_key_missing:0|queue_empty:0|runtime_disabled:0|output_pending:9|frame_id_lagging:0|unknown:0` / `continuous_decode_output_pending_correspondence_count=9` / `continuous_decode_writer_input_queue_len=0` だった。render consumption FAIL は exact key mismatch や stale decoded frame 以前に、writer が input を消費して correspondence に積んだ後、stdout decoded output が 1 frame も返っていない `output_pending` 問題として扱う
- latest rerun の continuous input frame range は `continuous_decode_input_frame_id_min=4` / `continuous_decode_input_frame_id_max=307` で、input `9` 件に対して frame_id が大きく飛んでいる。current code path は render の selected frame cache miss 後にその selected access unit だけを enqueue する render-demand feed なので、continuous-stream decoder に連続した client stream を渡せていない可能性を高く見る。ただし gap max / NAL kind / FFmpeg stderr はまだ summary に無いため断定しない
- `continuous_decode_stdout_read_elapsed_ms=0` / `continuous_decode_stall_count=0` / `continuous_decode_latest_decoded_frame_id=none` は、reader が decoded raw frame を 1 件も送れていない状態と読む。stdout reader が `read_exact` で block しているのか、FFmpeg が stderr に reference / decode error を出しているのかは current diagnostics では分からない
- current next design direction は latest decoded fallback ではなく、slot0 per-client continuous feed/drain policy の検討に寄せる。continuous-stream decoder として成立させるには selected frame だけでなく、client stream の連続 access unit を feed する必要がある可能性が高い。大きな feed architecture rewrite は次 step 以降に回す
- 2026-05-19 output-pending diagnostics-only slice で、slot0 continuous input gap / NAL / stderr / stdout-pending observability を追加した。summary には `continuous_decode_input_frame_id_gap_max` / `continuous_decode_input_frame_id_gap_total` / `continuous_decode_input_non_consecutive_count`、input keyframe/non-keyframe count、SPS/PPS/IDR/non-IDR VCL count、last input NAL kinds、FFmpeg stderr summary、stdout reader blocked count、no-output-after-input/keyframe count、bootstrap input/output count、last input/output frame_id が出る
- この output-pending diagnostics slice も挙動変更なし。continuous feeding は render-demand selected frame enqueue のまま、exact match lookup と one-shot fallback も維持し、latest decoded fallback / targetTime-aware lookup / slot0 per-client feed-drain 本実装 / slot1 continuous 化 / 4-client 化には進めていない。Production Readiness は FAIL 継続
- latest output-pending diagnostics rerun は `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-122239` として扱う。`continuous_decode_config_enabled=true` / `continuous_decode_runtime_enabled=true` / `continuous_decode_slot0_enabled=true` / `continuous_decode_input_frame_count=18` で opt-in propagation、runtime 作成、slot0 input feeding、lookup/output-pending diagnostics 自体は PASS とする
- 同 rerun では `continuous_decode_output_frame_count=0` / `continuous_decode_lookup_miss_reason_counts=exact_key_missing:0|queue_empty:0|runtime_disabled:0|output_pending:18|frame_id_lagging:0|unknown:0` / `continuous_decode_output_pending_correspondence_count=18` / `continuous_decode_writer_input_queue_len=0` / `render_used_continuous_decoded_count=0` / `render_used_one_shot_fallback_count=18` だった。writer queue は空なので、主因は input queue backlog ではなく FFmpeg stdin write 後の decode/stdout rawvideo output 側に寄せて扱う
- sparse feed は確認済み。`continuous_decode_input_frame_id_min=4` / `continuous_decode_input_frame_id_max=525` / `continuous_decode_input_frame_id_gap_max=37` / `continuous_decode_input_frame_id_gap_total=521` / `continuous_decode_input_non_consecutive_count=17` で、render-demand selected frame access unit だけを飛び飛びに continuous runtime へ投入している。ただし `continuous_decode_input_keyframe_count=18` / `continuous_decode_input_non_keyframe_count=0` / `continuous_decode_input_has_sps_count=18` / `continuous_decode_input_has_pps_count=18` / `continuous_decode_input_has_idr_count=18` / `continuous_decode_input_has_non_idr_vcl_count=0` でも output は 0 なので、missing non-IDR reference だけでは説明しきれない
- `continuous_decode_ffmpeg_stderr_summary=none` / `continuous_decode_stdout_reader_blocked_count=16` / `continuous_decode_no_output_after_input_count=17` / `continuous_decode_no_output_after_keyframe_count=17` / `continuous_decode_bootstrap_input_count=18` / `continuous_decode_bootstrap_output_count=0` は、FFmpeg process が stderr error を出さないまま stdout raw BGRA frame を返していない、または current stderr capture/process status diagnostics では見えていない状態として扱う。continuous stdout output、render consumption、continuous path による FPS improvement は FAIL、one-shot fallback safety は PASS、Production Readiness は FAIL 継続
- 2026-05-19 FFmpeg-runtime diagnostics-only slice で、continuous runtime の args / stdin write / stdout read attempt / process running-exit status / stderr reader liveness / stderr bytes / first-input timing を summary に追加した。FFmpeg args、feeding policy、lookup policy、fallback policy は変えていない
- latest continuous FFmpeg runtime diagnostics rerun は `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-133514` として扱う。`continuous_decode_config_enabled=true` / `continuous_decode_runtime_enabled=true` / `continuous_decode_slot0_enabled=true` / `continuous_decode_input_frame_count=10` で opt-in propagation、runtime 作成、slot0 input feeding は PASS とする
- 同 rerun の主判断は `stdin write success / process alive / stderr none / stdout read in progress / output 0` とする。`continuous_decode_stdin_write_count=10` / `continuous_decode_stdin_write_bytes_total=659384` / `continuous_decode_stdin_write_error_count=0` / `continuous_decode_process_running=true` / `continuous_decode_process_exit_status=none` / `continuous_decode_stdout_read_attempt_count=1` / `continuous_decode_stdout_read_in_progress=true` / `continuous_decode_stderr_reader_alive=true` / `continuous_decode_stderr_bytes_total=0` / `continuous_decode_ffmpeg_stderr_summary=none` / `continuous_decode_output_frame_count=0` だった
- latest rerun でも all-keyframe input は確認済み。`continuous_decode_input_keyframe_count=10` / `continuous_decode_input_non_keyframe_count=0` / `continuous_decode_input_has_sps_count=10` / `continuous_decode_input_has_pps_count=10` / `continuous_decode_input_has_idr_count=10` / `continuous_decode_input_has_non_idr_vcl_count=0` / `continuous_decode_last_input_payload_nal_kinds=sps+pps+idr+idr+idr+idr+idr+idr+idr+idr` なので、P-frame reference missing だけでは output `0` を説明しきれない
- latest rerun の continuous args は `ffmpeg -hide_banner -loglevel error -f h264 -i pipe:0 -vf scale=640:360:flags=neighbor -f rawvideo -pix_fmt bgra pipe:1` 相当で、同じ scaled `640x360` / raw BGRA / expected bytes `921600` の one-shot fallback は成功している。one-shot 側との差分は、one-shot が `-frames:v 1` を付け、payload write 後に stdin を close/EOF し、process を終了させるのに対し、continuous は `-frames:v 1` なしで stdin を開いたまま process lifetime を維持し、stdout reader が 1 raw frame 分の `921600` bytes を `read_exact` で待つ点にある
- current suspicion は payload 自体ではなく、continuous FFmpeg runtime の stdin open / H.264 demuxer-parser buffering / EOF or flush wait / missing low-latency probe args / stdout full-frame read boundary / quiet `-loglevel error` に寄せる。次は default args を変えず、opt-in experimental args toggle または diagnostics-only で `-fflags nobuffer` / `-flags low_delay` / `-analyzeduration 0` / `-probesize 32` / `-flush_packets 1` と temporary `-loglevel warning|info`、`continuous_decode_stdout_partial_bytes_read` / first-byte diagnostics を検討する
- 2026-05-19 experimental continuous FFmpeg args / stdout boundary diagnostics slice で、two-real preview loop 限定の opt-in `--continuous-decoder-low-latency-args` を追加した。default continuous args は変更せず、flag 有効時だけ `-loglevel warning`、`-fflags nobuffer`、`-flags low_delay`、`-analyzeduration 0`、`-probesize 32`、`-flush_packets 1` を continuous FFmpeg args に追加する
- 同 slice で summary に `continuous_decode_ffmpeg_low_latency_args_enabled` / `continuous_decode_ffmpeg_probe_args_enabled` / `continuous_decode_ffmpeg_loglevel`、`continuous_decode_stdout_first_byte_seen` / `continuous_decode_stdout_first_byte_elapsed_ms`、`continuous_decode_stdout_partial_bytes_read` / `continuous_decode_stdout_partial_read_count` / `continuous_decode_stdout_expected_frame_bytes` / `continuous_decode_stdout_read_waiting_for_full_frame` を追加した。reader thread は full raw frame 待ちの意味を維持しつつ、full frame 未満の stdout 進捗を観測する
- この experimental slice も挙動変更は opt-in args variant と diagnostics に限定する。continuous feeding / exact lookup / one-shot fallback / pixel format / latest decoded fallback / targetTime-aware lookup / slot0 per-client feed-drain / slot1 continuous / 4-client continuous は変えていない。Production Readiness は FAIL 継続
- latest low-latency/probe args rerun は `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-171331` として扱う。`continuous_decode_config_enabled=true` / `continuous_decode_runtime_enabled=true` / `continuous_decode_slot0_enabled=true` / `continuous_decode_ffmpeg_low_latency_args_enabled=true` / `continuous_decode_ffmpeg_probe_args_enabled=true` / `continuous_decode_ffmpeg_loglevel=warning` で opt-in propagation と args variant enablement は PASS とする
- 同 rerun では `continuous_decode_input_frame_count=15` / `continuous_decode_output_frame_count=11` / `continuous_decode_queue_len=11` / `continuous_decode_stdout_first_byte_seen=true` / `continuous_decode_stdout_first_byte_elapsed_ms=4126` / `continuous_decode_first_input_to_first_output_elapsed_ms=5322` / `continuous_decode_stdout_expected_frame_bytes=921600` だった。continuous stdout output は `0 -> 11` へ進んだため PARTIAL PASS とする
- ただし render consumption は FAIL 継続。`render_used_continuous_decoded_count=0` / `continuous_decode_lookup_hit_count=0` / `continuous_decode_lookup_miss_count=15` / `continuous_decode_lookup_miss_reason_counts=exact_key_missing:0|queue_empty:0|runtime_disabled:0|output_pending:15|frame_id_lagging:0|unknown:0` / `continuous_decode_fallback_to_one_shot_count=15` / `render_used_one_shot_fallback_count=15` で、render は continuous decoded frame を 1 回も使っていない
- latest rerun の主問題は stale decoded frame / lag として扱う。`continuous_decode_requested_frame_id=535` に対して `continuous_decode_latest_decoded_frame_id=386`、`continuous_decode_requested_minus_latest_lag=149`、`continuous_decode_frame_id_lag=173`、`continuous_decode_stale_frame_available_count=11`、`continuous_decode_queue_oldest_frame_id=4`、`continuous_decode_queue_newest_frame_id=386` で、decoded queue に frame はあるが requested selected frame から大きく遅れている
- sparse render-demand feed も継続している。`continuous_decode_input_frame_id_min=4` / `continuous_decode_input_frame_id_max=535` / `continuous_decode_input_frame_id_gap_max=66` / `continuous_decode_input_frame_id_gap_total=531` / `continuous_decode_input_non_consecutive_count=14` で、selected access unit だけを飛び飛びに feed する current path では continuous decoder が render requested frame に追いつけていない
- latest decoded fallback は古い frame を表示する危険があるため、現時点の next candidate から外す。次候補は latest decoded fallback / targetTime-aware lookup 実装ではなく、slot0 per-client continuous feed/drain policy の docs-first 最小設計に寄せる。one-shot fallback は維持し、FPS改善を continuous path 起因とは断定せず、Production Readiness は FAIL 継続とする
- slot0 per-client continuous feed/drain policy は `docs/operations/continuous-feed-drain-plan.md` に切り出した。first candidate は slot0 / two-real preview loop / opt-in continuous decoder 限定で、oldest-driven bounded feed を第一候補にし、render selection は display authority のまま維持する。exact selected frame lookup と one-shot fallback は残し、latest decoded fallback と targetTime-aware decoded render consumption は stale guard 設計まで保留する
- 2026-05-19 first implementation slice で、two-real preview loop 限定の slot0 bounded continuous feed helper を additive に追加した。helper は opt-in continuous decoder 有効時だけ validation/decode/render 前に `PreviewOldest` から最大 `2` frame を試し、enqueue 成功時だけ guarded `ConsumeOldest` で進める。slot1 / 4-client / server-client-protocol / latest decoded fallback / targetTime-aware lookup は未変更で、one-shot fallback と exact selected frame lookup は維持する
- 同 implementation slice で summary に continuous feed diagnostics を追加した。`continuous_feed_*`、`continuous_decode_input_from_feeder_count` / `continuous_decode_input_from_render_demand_count`、`continuous_decode_feeder_lag_to_selected`、`continuous_decode_render_exact_hit_count` / `continuous_decode_render_miss_stale_count` / `continuous_decode_render_miss_not_ready_count` を次回 rerun の確認対象にする。Production Readiness は FAIL 継続
- latest bounded-feed rerun は `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-202043` として扱う。`continuous_decode_config_enabled=true` / `continuous_decode_runtime_enabled=true` / `continuous_decode_slot0_enabled=true` / `continuous_decode_ffmpeg_low_latency_args_enabled=true` / `continuous_feed_enabled=true` で continuous opt-in、low-latency args、bounded feed helper は runtime PASS とする
- 同 rerun では `continuous_feed_attempt_count=300` / `continuous_feed_handoff_request_count=910` / `continuous_feed_frame_received_count=368` / `continuous_feed_enqueued_count=368` / `continuous_feed_skipped_count=0` / `continuous_feed_latest_received_frame_id=467` / `continuous_feed_latest_enqueued_frame_id=467` だった。`continuous_decode_input_from_feeder_count=368` / `continuous_decode_input_from_render_demand_count=4` / `continuous_decode_feeder_lag_to_selected=0` により、slot0 continuous input は render-demand sparse feed から feeder 主入力へかなり脱出できたと扱う
- continuous decoder output は `continuous_decode_input_frame_count=372` / `continuous_decode_output_frame_count=340` / `continuous_decode_queue_len=30` / `continuous_decode_dropped_stale_count=310` で PASS / PARTIAL PASS とする。一方で render consumption は `render_used_continuous_decoded_count=0` / `continuous_decode_render_exact_hit_count=0` / `continuous_decode_render_miss_stale_count=12` / `continuous_decode_render_miss_not_ready_count=2` / `continuous_decode_fallback_to_one_shot_count=14` / `render_used_one_shot_fallback_count=14` なので FAIL 継続とする
- latest rerun の lookup 問題は exact selected-frame lookup strictness と decoded lag として扱う。`continuous_decode_requested_frame_id=459` に対して `continuous_decode_latest_decoded_frame_id=426`、`continuous_decode_requested_minus_latest_lag=40`、`continuous_decode_frame_id_lag=42`、queue range は `390..426` だった。feeder は selected side に追いついているが、decoded output が requested frame から約 `40` frame 遅れており、exact hit だけでは render hot path に乗らない
- next candidate は `docs/operations/continuous-decoded-lookup-plan.md` の targetTime-aware / bounded-lag decoded queue lookup docs-first 設計へ移す。latest decoded fallback は無制限に使うと stale frame 表示リスクがあるため、bounded-lag guard と no-future-frame guard を前提にする。Production Readiness は FAIL 継続
- 2026-05-20 first implementation slice で、slot0 / two-real preview loop / opt-in continuous 限定の bounded-lag decoded queue lookup を追加した。lookup order は exact selected-frame lookup first、bounded-lag frame_id-nearest lookup second、one-shot fallback third とする。first threshold は safety-first の固定 `5` frames で、CLI flag 化や targetTime-aware 本格 lookup には進めていない
- bounded-lag lookup は requested `frame_id` 以下の decoded frame だけを候補にし、requested より未来の decoded frame は拒否する。lag が `5` frames を超える候補は stale として拒否し、one-shot fallback へ進む。summary には `continuous_decode_bounded_lookup_*` と `continuous_decode_render_used_exact_count` / `continuous_decode_render_used_bounded_lag_count` を追加した
- code path 上、`first_render_attempt_index` / `first_render_elapsed_ms` は two-real preview loop の clean output が `RenderReady` かつ inner render が `Rendered` になった最初の tick でだけ確定する。`no_render_before_first_render` は別 counter ではなく、その first render attempt index から導出されるため、startup で renderable slot が 1 つも成立しない tick が続くとそのまま増える
- `no_frame_count=864`、`handoff_error_count=22`、server `no_frame_count=264`、`placeholder_visual_changed_count=46`、`scheduler_status=PartialSelected` は、after-first FPS 改善後も availability / startup 側に未解決が残ることを示している。Production Readiness は FAIL 継続
- startup/no-frame availability の current code path は `scheduler -> decode/render adapter -> display policy -> quad composition -> render-facing -> clean output render` で、`SkipNoFrameAvailable` / `SkipWaitingForFrameAtOrBeforeTarget` は previous frame が無い間 `NoDisplayPlaceholder` に落ちる。さらに renderable slot count が `0` の tick は `NoRenderableQuadView` のままなので、server aggregate `frames_queued=1800` が成立していても switcher startup が直ちに first render へ進むとは限らない
- switcher `no_frame_count` と server `no_frame_count` は同じ意味ではない。switcher 側は per-tick/per-slot 集計で、server 側は handoff/queue 読み出し側の no-frame 観測なので、absolute value を 1:1 比較せず availability trend として読む
- `source_recovered` は previous slot diagnostic の `selected_frame_available != true` から current tick で `selected_frame_available == true` へ戻ったときに付く。previous unavailable は `NoFrameAvailable` だけでなく waiting や `HandoffError` でも起こり得るため、latest rerun で見えた `source_recovered` slow bias は post-gap / post-error recovery decode の偏り候補として扱うが、まだ single-cause 断定はしない
- 2026-05-15 の narrow compile-fix slice では、same-PC `2`-client switcher hot path optimization 後に壊れていた `apps/switcher/src/main.rs` の型不一致を最小修正で解消した。`render_four_view_focused_slot_with_runtime` は OBS validation profile 変換 helper の返り値を `(SwitcherDecodedFrameRenderInput, BgraRenderBufferDiagnostics)` として受け、`frame:` には `SwitcherDecodedFrameRenderInput` だけを渡すように戻した。focused preview helper 自体は summary timing を持たないため diagnostics は `_scaled_diagnostics` として明示受けに留め、unused import warning も削除した。`cargo fmt`、`cargo fmt --check`、`cargo check -p stream-sync-switcher`、focused switcher tests、`cargo check --workspace`、`git diff --check`、`cargo build -p stream-sync-server -p stream-sync-switcher -p stream-sync-client` は PASS し、`target\debug\stream-sync-switcher.exe` の timestamp は `2026-05-15 22:59:39` に更新された。この compile-fix が latest render smoke rerun の前提になった
- latest same-PC `4`-client all-real concurrent validation は `manual-logs/four-client-20260513-184503` を latest evidence として PASS 判定にした。server ready / stopped summary、client1..4 auth/send、server queue participation、named-pipe handoff transport は PASS しており、final switcher state は `AllSelected` / `Selected|Selected|Selected|Selected`、`clean_output_render_result_kind=Rendered`、`preview_mode=preview-latest-decodable`、`read_mode=inspect-latest-decodable` だった。same-PC saturation は残っており、client effective output fps は `19.732|20.201|20.299|20.040` まで落ちた
- latest OBS capture validation は `manual-logs/obs-capture-20260513-190909` で追加され、OBS 側は `StreamSync 4-view Output` の選択と preview 表示が PASS した。一方で StreamSync runtime は same-PC saturation により PARTIAL で、client2 / client3 は `EncodeFailure`、client effective output fps は `16-18fps` 台、switcher final summary は `360` 秒以内に終了しなかったため未回収だった。これは既存の same-PC `4`-client all-real PASS を巻き戻すものではない
- distributed-PC validation planning の source of truth を `docs/operations/distributed-pc-validation.md` に切り出した。same-PC `4`-client all-real functional PASS と OBS capture PASS は維持したまま、next phase を `server/switcher/OBS on streaming PC + one or more remote clients` の実行計画として固定し、PC配置、ネットワーク前提、起動順、command shape、success criterion、failure classification、evidence shape、long OBS run と switcher final summary の分離方針を明文化した
- distributed-PC command-pack preparation も `docs/operations/distributed-pc-validation.md` に反映済みになった。`S:\stream-sync` を repo root とする copy-pasteable command pack、`manual-logs` 配下の run label 規約、`<SERVER_HOST>` / `<RUN_STAMP>` の置換項目、distributed `2`-client smoke -> distributed `2`-client OBS visible -> distributed `4`-client -> long OBS run の validation order、runtime evidence と long OBS visual stability evidence の分離、evidence paste-back template をこの docs-first slice の current source of truth にした。build/check/test は意図的に別の build-validation step に分離する
- 2026-05-13 の narrow switcher parity slice は 184503 の PASS で実地確認まで完了した。`--four-view-four-real-handoff-preview-loop` は optional preview mode `[preview-oldest|preview-latest|preview-latest-decodable]` を受け付け、`preview-latest-decodable` で `PreviewLatestDecodableIfAtOrBefore` / `InspectLatestDecodable` を使える。retained-keyframe fallback は 2-client PASS path と同じ read-mode mapping に揃い、4-real targetTime も tick ごとに再計算される。summary には `preview_mode` / `read_mode` も出る。`frames_rendered=137/180` は completion-count observability であり、OBS capture PASS までを閉じたうえで、次の docs-first follow-up は distributed-PC validation planning と same-PC performance tuning に置く
- latest same-PC `4`-client PASS 後の downstream `Window Capture` follow-up source of truth も追加した。`docs/operations/obs-capture-validation.md` は `manual-logs/four-client-20260513-184503` を runtime baseline にして、OBS 側の目的、manual checklist、success criterion、failure classification、pasted-back evidence shape を整理する。OBS WebSocket / advanced OBS control はこの step でも引き続き out of scope にする
- same-PC 2-client concurrent validation は `manual-logs/handoff-20260513-134658` を latest PASS evidence として closed にした。次 phase は rerun ではなく docs-first の `4`-client all-real validation preparation で、source of truth は `docs/operations/four-client-validation.md` とする
- `4`-client validation の初期方針も固定した。distributed-PC より same-PC first を優先し、server + switcher + client1..4 を同一 Windows PC 上で動かす stress validation として扱う。main path は concurrent server `--receive-auth-video-queue-and-serve-handoff-continuous` + switcher `--four-view-four-real-handoff-preview-loop` + client1..4 bounded persistent/deadline send で、PASS criterion は final all-real slot state / clean output renderability / per-client queue participation を主 gate にする
- latest concurrent human validation では `receive_ready=true` / `handoff_ready=true` / `runtime_mode=concurrent` は PASS したが、receive side が expected-threshold `0` を disabled ではなく stop判定に混ぜていた。実際の失敗 shape は `stop_reason=MaxHandoffRequestsReached` / `receive_stop_reason=ReassembledFramesThresholdReached` / `packets_received=1` / `frames_queued=0` / `frame_read_count=0` で、client 側は `frames_sent=900` まで進んでいたため、auth failure でも encoder failure でもなく concurrent receive closeout semantics の問題として扱う
- current fix では concurrent runtime の `expected_reassembled_frames=0` / `expected_clients=0` / `expected_per_client_frames=0` を disabled として扱うようにした。receive stop判定は enabled threshold のみを見るようにし、ready/stopped summary には `expected_reassembled_frames_enabled` / `expected_clients_enabled` / `expected_per_client_frames_enabled` を追加した。same-PC continuous validation では `validation_ready=n/a` のまま、主な receive closeout は `receive_timeout` / `max_runtime_duration_ms` / `max_video_packets` / explicit stop に寄せる
- 2026-05-12 の requested automated validation sweep は PASS した。`cargo fmt` / `cargo fmt --check` / `cargo check --workspace` / focused concurrent server tests / focused staged handoff regression tests / `cargo test --workspace` / `git diff --check` を通し、disabled-threshold semantics と staged regression に新たな自動 test failure は出ていない。その後の same-PC human rerun でも server closeout gate は PASS し、`expected_*_enabled=false` と `receive_stop_reason != ReassembledFramesThresholdReached` の実機確認に加えて stopped summary も回収できた
- latest same-PC concurrent human rerun は `manual-logs/handoff-20260513-134658` を latest evidence として扱う。ready-line disabled-threshold semantics は PASS し、client1/client2 はそれぞれ `accepted=true` / `frames_sent=900` / `send_failures=0` / `keyframes_sent=30` / `h264_parameter_sets_cached=true` / `stop_reason=Some(MaxFramesReached)` / `effective_output_fps=29.690|29.507` を満たした
- first concurrent receive + handoff serve runtime slice 自体は維持している。new command `--receive-auth-video-queue-and-serve-handoff-continuous` は staged command `--receive-auth-video-queue-and-serve-handoff-many` を残したまま、coarse-lock shared state 上で UDP receive/auth/reassembly/queue update と named-pipe handoff serve を同時に持てる
- 2026-05-13 の latest same-PC concurrent human rerun では server closeout gate と queue/handoff read gate も PASS した。server stopped summary は `runtime_mode=concurrent` / `stop_reason=ReceiveStopped` / `receive_stop_reason=ReceiveTimedOut` / `handoff_stop_reason=StopRequested` / `runtime_duration_ms=154708` / `packets_received=36122` / `frames_queued=1800` / `per_client_queued_frames=player1/streamsync-dev-session:900|player2/streamsync-dev-session:900` / `keyframes_queued=60` / `retained_keyframe_clients=2` / `frame_read_count=231` / `no_frame_count=126` / `decodable_source_counts=queue:20|retained_keyframe:211|none:126` / `io_error_count=0` を満たした
- previous switcher final `HandoffError` / `os_error_2` follow-up は解消済みになった。latest switcher final summary は `frames_attempted=180` / `frames_rendered=117` / `render_failures=0` / `scheduler_status=PartialSelected` / `slot_result_kinds=Selected|Selected|NoFrameAvailable|NoFrameAvailable` / final real-slot `handoff_response_kind=FrameRead` / `io_error=none` / `decodable_source=retained_keyframe` / `decode_error=none` / `clean_output_render_result_kind=Rendered` を満たし、final real-slot handoff selection と renderability 自体は確認できている
- switcher summary semantics review も完了した。`apps/switcher/src/main.rs` の current loop では `frames_attempted` は preview-loop tick ごとに 1 増え、`frames_rendered` は clean output window result が `Rendered` のときだけ増える。`NoRenderableQuadView` や他の non-render tick は `frames_rendered` に含まれず、固定 placeholder slot `2/3` 自体は 2-real + 2-placeholder render を妨げない。したがって latest `frames_rendered=117/180` は hidden failure ではなく completion-count observability として扱い、concurrent validation の success criterion は `no final HandoffError` / final real slots `Selected` / final `handoff_response_kind=FrameRead` / final `io_error=none` / `render_failures=0` / `clean_output_render_result_kind=Rendered` を主 gate にする
- current concurrent validation is now PASS and closed. The next phase should move to later-phase planning rather than re-running the 2-client concurrent gate.
- closeout fix 後の automated validation は PASS。`cargo fmt`、`cargo test -p stream-sync-server concurrent -- --nocapture`、focused test `concurrent_runtime_max_duration_closeout_returns_summary_without_client_requests`、`cargo test --workspace`、`git diff --check` を通した。current gate is closed; next work should be later-phase planning, not another 2-client concurrent rerun
- 2-client human validation 方針は same-PC smoke / stress profile に固定した。今後の 2-client validation は server + client1 + client2 + capture + FFmpeg encode を同一 Windows PC 上で動かす前提とし、distributed-PC validation 用の server IP / firewall 手順は主目的にしない
- same-PC 2-client longer-run validation は PASS 済み。標準設定は `receive_buffer_bytes=268435456` + `max_packets_per_drain_cycle=1024` + summary-only + `receive_timeout_ms=30000` + `max_frames=900 per client` + `fragment_pacing_every=4` + `fragment_pacing_delay_ms=2` とし、client 合計 `1800` frames に対して server は `frames_reassembled=1800` / `frames_queued=1800` / `rejected_packets=0` / `decode_errors=0` / `incomplete_reassembly_frames=0` を確認した。`max_packets_drained_in_cycle=578` のため cap `1024` は現状十分である
- 2-client validation の human-run recipe を same-PC 前提に更新した。`docs/operations/two-client-long-run-validation.md` と `docs/operations/two-client-long-run-validation.ps1` は same-PC smoke / stress profile、baseline 比較、`256` / `512` / `1024` の drain cap 比較、貼り返し template を source of truth とする
- same-PC 2-client handoff validation preparation も docs に切り出した。main path は `stream-sync-server --receive-auth-video-queue-and-serve-handoff-many` + `stream-sync-switcher --four-view-two-real-handoff-preview-loop` とし、raw named-pipe isolation は `--read-queued-frame-handoff-once` を使う。`docs/operations/two-client-handoff-validation.md` を次の human-run source of truth とする
- handoff named-pipe troubleshooting の narrow follow-up も進めた。server / switcher ともに short pipe name と full local pipe path を同じ actual Windows pipe path に normalize し、summary には requested `pipe_name` と normalized `actual_pipe_path` の両方を出す。server 側は summary-only 運用でも receive 完了直後に `handoff_ready=true` readiness line を stdout に出し、さらに `validation_ready` / `ready_reason` / `receive_stop_reason` / `expected_clients` / `expected_per_client_frames` / `observed_queued_clients` / `observed_reassembled_clients` / `per_client_queued_frames` / `per_client_direct_frames` / `per_client_reassembled_frames` も出す。human validation では `handoff_ready=true` だけでなく `validation_ready=true` を gate にし、`ready_reason=receive_timeout|max_packets_reached` は premature fallback として扱う。validation-ready の expected count は queued frame 基準で、direct frame と fragmented/reassembled frame を両方含む。serve 終了時には `handoff_stopped=true` / `stop_reason` / `handoff_requests_completed` / `frame_read_count` / `no_frame_count` / `parse_error_count` / `io_error_count` を含む stopped summary を出す
- latest same-PC 2-client handoff validation では transport / queue / `FrameRead` までは PASS 済みになった。server ready は `handoff_ready=true` + `validation_ready=true`、switcher real slots も `handoff_response_kind=FrameRead` を返せている。一方で current switcher one-shot decode は persistent encoder payload を単体 decode できず、`SwitcherH264DecodeFailure` with `non-existing PPS 0 referenced` が next blocker である。current client persistent path には SPS/PPS cache + prepend の experimental behavior と summary visibility (`h264_parameter_sets_cached` / `h264_sps_count` / `h264_pps_count` / `h264_parameter_sets_prepended_count` / `last_payload_had_parameter_sets` / `h264_parameter_sets_missing_count`) を追加したため、次の human rerun は decode-error clearance の確認が main gate になる
- latest rerun では `non-existing PPS 0 referenced` は解消し、client summary でも `h264_parameter_sets_cached=true` / `h264_parameter_sets_missing_count=0` / `h264_parameter_sets_prepended_count=870` を確認できた。switcher decode observability の追加後、`decoded_rawvideo_length_mismatch_expected=8294400_actual=0` は `decode_expected_width=1920` / `decode_expected_height=1080` と `payload_has_idr=false` の組み合わせまで切り分けられた。current fix では client persistent path が raw capture size ではなく encoder output size `1280x720` を `VideoFrame` metadata に載せ、`VideoFrame.is_keyframe` は access unit の IDR 有無で立てる。next human rerun では switcher 側 expected decode size が `1280x720` / `3686400` へ揃うことと、`preview-latest-decodable` で IDR-bearing payload を選べるかを main gate にする
- server handoff / switcher preview には `InspectLatestDecodable` / `preview-latest-decodable` の opt-in mode も追加済み。current one-shot decode validation では `preview-latest` ではなく `preview-latest-decodable` を推奨し、`frame_is_keyframe` / `payload_has_idr` / `decode_expected_width` / `decode_expected_height` / `decode_expected_rawvideo_len` を同時に読む。server concurrent receive + handoff serve や switcher persistent decoder context はこの slice では触らない
- latest human rerun では `preview-latest-decodable` が `NoFrame` になり、原因は「IDR が存在しない」よりも「queue cap 16 が GOP 30 より短く、retained queue に keyframe が残っていない」可能性が高いと判明した。current fix では server queue と別に latest keyframe を `client_id + run_id` ごとに retained し、`InspectLatestDecodable` / `preview-latest-decodable` は queued keyframe が無いとき retained keyframe へ fallback する。summary / slot diagnostics には `decodable_source` / `retained_keyframe_available` / `retained_keyframe_frame_id` / `handoff_no_frame_reason` を追加した
- latest human rerun では `retained_keyframe_clients=0` / `decodable_source=none` / `handoff_no_frame_reason=NoDecodableFrameAvailable` まで narrowed できた。root cause は retained-keyframe fallback 不足そのものではなく、fragment-dominant path で `VideoFrameFragment -> server reassembly -> queued VideoFrame` の間に `is_keyframe` metadata が落ちていたことだった。current fix では fragment wire format と reassembly state に `is_keyframe` を通し、client persistent summary に `h264_idr_count` / `h264_non_idr_vcl_count` / `keyframes_encoded` / `keyframes_sent` / `first_keyframe_frame_id` / `last_keyframe_frame_id`、server receive summary に `keyframes_received` / `keyframes_queued` / `per_client_keyframes_queued` / `first_keyframe_frame_id` / `last_keyframe_frame_id` を追加した
- latest same-PC 2-client handoff preview human rerun は PASS 済み。server は `handoff_ready=true` / `validation_ready=true` / `ready_reason=expected_clients_reached` / `registered_clients=2` / `observed_queued_clients=2` / `observed_reassembled_clients=2` / `retained_keyframe_clients=2` を満たし、client は `frames_sent=900` / `h264_idr_count=30` / `keyframes_sent=30` / `encode_failures=0` / `send_failures=0`、switcher は `frames_rendered=180` / `render_failures=0` / `clean_output_render_result_kind=Rendered` / real slots `handoff_response_kind=FrameRead` / `decode_error=none` まで通った。current checkpoint では server receive / queue、client persistent + deadline send、SPS/PPS prepend、keyframe metadata propagation、retained-keyframe fallback、2 real slot preview render を PASS 扱いにしてよい
- ただし current `preview-latest-decodable` は `decodable_source=retained_keyframe` を返した staged keyframe-preview path であり、continuous latest non-IDR decode や production-like realtime preview はまだ未達である。current bounded same-PC 2-client run では `effective_output_fps` が `26fps` 前後まで落ちるケースもあるため、capture/cadence負荷は known issue として残す
- 次の implementation phase は concurrent receive + handoff serve runtime の design-first slice とする。new source of truth は `docs/operations/concurrent-handoff-runtime-plan.md` とし、existing staged command `--receive-auth-video-queue-and-serve-handoff-many` は残したまま、新しい concurrent command 候補 `--receive-auth-video-queue-and-serve-handoff-continuous` を first implementation target とする。first slice は 2-client same-PC、retained-keyframe based `preview-latest-decodable`、no reconnect、no daemon polish、no OBS、no 4-client で切る
- concurrent runtime で最初に固めるべき点は、`receive_ready=true` / `handoff_ready=true` の早期 readiness、queue + retained keyframe + per-client counters の coarse-lock shared state、handoff request counters と decodable-source counts を含む runtime summary、manual same-PC 手順で「client送信中に switcher が FrameRead できること」の確認である
- current handoff-many command は引き続き staged validation 用であり、receive/auth phase 完了後に handoff pipe を開く。bounded same-PC validation では許容するが、realtime preview / production では receive と handoff serve を並行に持つ runtime が別 slice で必要である
- client bounded real encoded sender の human-validation summary も整理した。`frames_attempted` は loop tick と capture/send attempt を混同しやすかったため human-facing 出力から外し、`configured_max_frames` / `configured_max_ticks` / `configured_frame_interval_ms` / `runtime_ticks` / `capture_attempts` / `frames_remaining_to_max` / `elapsed_ms` / `capture_elapsed_ms` / `encode_elapsed_ms` / `avg_capture_elapsed_ms` / `avg_encode_elapsed_ms` / `capture_wait_or_no_frame_elapsed_ms` / `effective_output_fps` / `effective_fresh_capture_fps` / `effective_send_fps` / `loop_interval_sleep_ms` / `total_fragment_pacing_sleep_ms` / `send_elapsed_ms` / `ticks_elapsed_while_sending` を追加した。current loop は synchronous なので `ticks_elapsed_while_sending=0` が期待値であり、`MaxTicksReached` 時は `frames_remaining_to_max > 0` で max-frames 未達が即読できる。per-frame baseline では `frames_sent=100` / `elapsed_ms=12977.898` / `avg_capture_elapsed_ms=5.042` / `avg_encode_elapsed_ms=72.569` / `effective_output_fps=7.705` / `loop_interval_sleep_ms=3366.633` / `send_elapsed_ms=1706.978` を観測した
- client 側には experimental persistent FFmpeg encoder boundary の最小 slice も追加した。`ClientPersistentFfmpegH264EncoderBoundary` は 1 process spawn、BGRA raw frame bytes の `stdin` write、raw H.264 Annex B stream bytes の `stdout` read、typed shutdown / stdout close / non-zero exit を client-only library boundary として持つ。既存 bounded PoC と既存 per-frame encoder path はまだ切り替えておらず、access-unit/frame boundary recovery も未実装のため、現段階では observability と lifecycle 固定のための experimental runtime として扱う
- client 側には experimental Annex B access-unit reader の最小 slice も追加した。`ClientAnnexBAccessUnitReaderBoundary` は persistent FFmpeg `stdout` byte stream を incremental に蓄積し、`0x000001` / `0x00000001` start code 付き NAL unit を切り出しながら conservative な sendable access unit を typed result で返せる。partial buffer / EOF incomplete / malformed stream は分離して扱い、SPS/PPS/SEI は current frame に VCL が見えた後は次 frame 側へ寄せる保守的境界として扱う。既存 bounded PoC への統合と real timing re-measure は次の slice
- client 側には persistent runtime stdout read と Annex B reader を結線する experimental session boundary も追加した。`ClientPersistentFfmpegH264AccessUnitBoundary` / session は BGRA frame を persistent encoder `stdin` に書き、`stdout` の raw Annex B bytes を reader に feed し、`AccessUnit` / `NoCompleteAccessUnitYet` / malformed / stdout close / EOF / non-zero exit を typed のまま扱える
- `--auth-real-encoded-video-frame-poc-bounded` には experimental persistent encoder runtime path も opt-in で統合済みになった。CLI では `--encoder-runtime persistent` を指定すると bounded run 全体で encoder process を 1 回だけ起動し、captured BGRA frame を persistent encoder `stdin` に投入しながら `stdout` Annex B bytes を access-unit reader に流し、complete access unit を既存 `VideoFrame` payload として送る。summary には `encoder_runtime` / `encoder_process_start_count` / `persistent_access_units_emitted` / `persistent_no_complete_access_unit_count` / `persistent_stdout_closed_count` / `persistent_malformed_stream_count` / `last_encoder_exit_status` を追加した。default は従来どおり `per_frame` のまま維持する
- persistent bounded path の first human validation では `encoder_runtime=persistent` / `encoder_process_start_count=1` / `frames_sent=100` / `persistent_access_units_emitted=100` / `persistent_no_complete_access_unit_count=6` / `last_encoder_exit_status=0` / `elapsed_ms=4775.901` / `avg_encode_elapsed_ms=3.821` / `effective_output_fps=20.938` / `loop_interval_sleep_ms=3566.631` / `send_elapsed_ms=186.360` を確認した。主因だった per-frame FFmpeg spawn cost は解消できており、次の bottleneck は fixed cadence sleep である
- bounded client loop には `--cadence-mode deadline` の opt-in path も追加した。deadline mode は `run_start + output_frame_index * frame_interval` を基準にし、deadline より早い場合だけ sleep、遅れている場合は sleep せず `deadline_overrun_ms` と `late_tick_count` に集計する。summary には `cadence_mode` / `deadline_sleep_ms` / `deadline_overrun_ms` / `late_tick_count` / `max_deadline_overrun_ms` を追加した。default は従来どおり `fixed` のまま維持する
- first deadline-mode human validation は bug を再現した。`--encoder-runtime persistent --cadence-mode deadline` で `elapsed_ms=4.416` の間に `runtime_ticks=1000` / `capture_attempts=1000` / `frames_captured=0` / `frames_sent=0` / `encoder_process_start_count=0` / `stop_reason=Some(MaxTicksReached)` となり、body を評価する前に max-ticks まで空回りした。root cause は persistent path で `NoFrameAvailable` / `CaptureUnavailable` の summary 更新が抜けていたことと、deadline 進行基準を `frames_sent` に置いたため first send 前に deadline が前進せず busy-spin したことだった。current fix では summary 更新を persistent path 全 result に戻し、deadline cadence の進行基準を actual `capture_attempts` に変更して no-frame 時でも cadence が前進するようにした
- fixed persistent path の `100`-frame validation では `effective_output_fps=20.938` だったが、persistent + deadline rerun では `encoder_process_start_count=1` / `frames_sent=100` / `persistent_access_units_emitted=100` / `avg_encode_elapsed_ms=3.245` / `effective_output_fps=28.600` / `elapsed_ms=3496.500` / `deadline_sleep_ms=2353.324` / `deadline_overrun_ms=50.367` / `late_tick_count=4` / `max_deadline_overrun_ms=35.345` を確認した。理想 `100 / 30 = 3.333s` に対して差分は約 `163ms` まで縮み、client 30fps bounded PoC は MVP human validation 上は PASS 寄りとして扱える
- persistent + deadline の `900`-frame longer-run human validation も PASS 済み。`frames_sent=900` / `persistent_access_units_emitted=900` / `encode_failures=0` / `send_failures=0` / `persistent_malformed_stream_count=0` / `avg_encode_elapsed_ms=5.517` / `effective_output_fps=29.162` / `elapsed_ms=30861.902` / `deadline_overrun_ms=2813.034` / `late_tick_count=82` / `max_deadline_overrun_ms=183.092` を確認した。client 30fps bounded PoC は MVP human validation 上 PASS として扱い、次の major slice は server -> switcher handoff validation 再実行、または 4-client に向けた準備へ進むこと
- current handoff observability では `Selected` / `NoFrameAvailable` / `WaitingForFrameAtOrBeforeTarget` / `HandoffError` は `slot_result_kinds` と `slot_diagnostics` で読める。一方で explicit late-drop aggregate と adjusted timestamp / target_timestamp は current real handoff preview summary に直接は出ないため、今回の handoff prep では non-blocking gap として整理し、pass/fail gate には使わない
- continuous receive / send runtime の最小 sliceを拡張し、`stream-sync-server --receive-send-runtime-continuous [config-path] [receive-timeout-ms] [max-iterations-or-0-for-unbounded] [heartbeat-timeout-micros] [receive-buffer-bytes] [max-packets-per-drain-cycle]` で drain cap を CLI 指定できるようにした。summary には `max_packets_per_drain_cycle` / `drain_cycles` / `last_packets_drained_in_cycle` / `max_packets_drained_in_cycle` / `receive_would_block_count` を出し、same-PC rerun で cap 張り付き有無を比較できる
- server continuous runtime の default 出力は summary-only に固定した。same-PC validation では packet / drain cycle / reassembly の大量ログを通常モードで流さず、final summary 1 行だけを比較する。詳細ログが必要な場合だけ `--verbose` を付ける
- `stop_reason=ReceiveTimedOut` は same-PC smoke / longer-run では client 完了後の idle closeout として読む。期待値の `frames_reassembled` / `frames_queued` に達している場合は failure ではない
- 認証 / runtime hardening の最小 slice を実装した。auth decision、same-client registration、client-scoped gate rejection、heartbeat timeout を雑な文字列に寄せず typed status/reason で読めるようにし、`Reject` と `ReconnectRequired` と `InvestigationRequired` を `Continue` から分離した。manual auth PoC、`--receive-auth-video-queue-once`、`--receive-send-runtime-bounded` summary には typed auth / registration / runtime rejection visibility を追加した
- `NoFrame` / `Waiting` / `HandoffError` の長時間 run 向け最小 status 整理を実装した。source-backed 2-view fallible validation には typed operational summary を追加し、per-side result kind を `Selected` / `NoFrame` / `Waiting` / `HandoffError` のまま保持しつつ、run-state を `Continue` / `RetryLater` / `ReconnectRequired` / `InvestigationRequired` で読めるようにした。late-drop summary あり path では post-mutation の `NoFrame` / `Waiting` 判断を summary へ接続できる
- late frame queue mutation / jitter buffer / drop policy の最小 slice を source-backed path で実装した。`SwitcherSingleClientLateFrameQueueMutationBoundary` が oldest head を targetTime 基準で評価し、補正後 timestamp が `targetTime - max_late_micros` より古い frame だけを conservative に drop する。drop summary は testable に返し、source-backed 2-view validation では opt-in で接続できる
- `RTT / offset` 平滑化と補正後 timestamp の targetTime selection 接続は最小 slice を完了した。server 側は latest raw estimate と smoothed estimate を分離保持し、switcher 側は optional な per-client clock offset を targetTime-aware source / scheduler / validation boundary へ薄く配線できる
- 仕様固定、Cargo workspace 初期化、`apps/*` / `crates/*` の scaffold は完了している
- `crates/protocol` / `crates/config` / `crates/net-core` の最小実装は揃っており、主要 message 型、timestamp 型、fixed header decode / encode、server auth 設定読み込み、`shared_token_env` 解決、UDP 1 datagram receive / send adapter までは完了している
- server 側は auth one-shot、accepted auth registry 登録、heartbeat ack / liveness / timeout action plan / timeout apply / notice queue storage、RTT / offset state commit と metrics snapshot handoff までの最小境界が揃っている。current hardening slice では auth reject、same-client re-registration、`run_id` mismatch、unregistered/unknown client、stale heartbeat timeout の扱いを typed summary として切り出した
- client 側は auth one-shot、heartbeat one-shot、`HeartbeatAckObservation` 付き `ClientStats` one-shot、one-tick runtime、accepted path 手動確認まで完了している
- client continuous heartbeat loop は thin composition の completed body まで実装済みで、heartbeat timeout notice wakeup planning 境界、wakeup execution 境界、wakeup actual side-effect 境界、outer while-loop connection 境界、outer while-loop one-turn execution body 境界、actual timer wait / retry execution / reconnect 実行境界、outer while-loop 反復実行本体、reconnect policy 境界、caller-owned hook 付き actual socket 再確立境界、real UDP socket 差し替え hook、repeated body からの hook 注入経路まで完了している
- 4-view operator MVP closeout は完了し、final regression も通過、push も完了している。bounded real encoded video / raw-key operator wrapper / `AllView` / `Focused(0..3)` / `AllView` return / raw console restore / `[video.encoder]` profile wiring / production H.264 stdout visibility / short OBS Window Capture validation までは current completed scope とする
- 未完了の中心は server -> switcher handoff validation、4-client all-real validation 準備、4-view sync orchestration の長時間運用 polish、実 outbound queue flush / `ServerNotice` 実送信 / lifecycle の後続 slice、dashboard UI rendering である。2-client ingest / reassembly の same-PC smoke / longer-run は通過扱いとし、next major phase は handoff validation 準備へ移る
- `stream-sync-server --receive-send-runtime-bounded [config-path] [max-iterations] [receive-timeout-ms]` は追加済みで、1 process lifetime で 1 bound UDP socket / 1 `AuthenticatedSenderRegistry` / 1 `ServerOutboundQueueCollection` / caller-owned writers を維持しながら existing `ServerControllerReceiveSendRuntimeBoundary` を outer loop から繰り返し呼べる
- bounded repeated runtime summary には `command_name` / `config_path` / `max_iterations` / `receive_timeout_ms` / `iterations_attempted` / `iterations_completed` / `auth_requests_received` / `auth_responses_sent` / `heartbeats_received` / `heartbeat_acks_sent` / `client_stats_received` / `client_stats_returns_sent` / `accepted_packets` / `rejected_packets` / `decode_errors` / `send_failures` / `outbound_queue_len` / `registered_clients` / `stop_reason` を出す。current hardening slice では追加で `last_auth_status` / `last_auth_reason` / `last_registration_status` / `last_registration_reason` / `last_runtime_rejection_status` / `last_runtime_rejection_reason` を読める
- fatal/stop visibility の narrow slice も追加済みで、success summary には `timeout_iterations` / `timeout_only_run` / `last_receive_error` / `last_send_error` / `last_rejected_reason` を追加した。fatal/startup failure 時は same command args を含む one-line failure summary を stderr に出し、`stop_reason` / `fatal_error_kind` / `fatal_error_detail` で silent failure を避ける。`last_rejected_reason` は backward-compatible string のまま残しつつ、typed summary を並置する
- receive/send continuous logging ownership の docs 設計も固定済みで、stdout/stderr summary は bounded run closeout 専用、structured operational logs は per-iteration/per-packet event 専用として責務を分離した。caller-owned writers は維持し、file sink open / rotation / process-wide logger / dashboard/exporter transport は future boundary に残す
- per-iteration receive/send event handoff の narrow implementation も追加済みで、`ServerReceiveSendRuntimeBoundedStartupOutcome` は `iteration_events` を持つ。event fields は `command_name` / `iteration_index` / `receive_outcome_kind` / `accepted_packet_kind` / `auth_outcome_kind` / `rejection_kind` / `send_outcome_kind` / `sent_message_kind` / `receive_error` / `send_error` とし、outer loop の typed observation surface に限定する。hardening の詳細は aggregate summary に寄せ、selection/queue/I/O と混ぜない
- iteration event の JSONL writer ownership 最小接続も追加済みで、typed `iteration_events` は compact JSONL (`event_type=receive_send_iteration`) として caller-owned writer に書ける。writer failure は runtime stop に直結させず、`iteration_event_log_summary.lines_written` / `write_failures` / `last_writer_error` で outcome 側に可視化する
- iteration-event JSONL sink plan / optional config wiring の docs 設計も固定済みで、その first implementation として launcher/config layer に `[logging.receive_send_iteration]` parse と `stderr` / `disabled` selection を追加済み。section absent は current CLI default と同じ stderr、`enabled=false` と `destination="disabled"` は discard sink、`destination="file"` は parse までは通すが current slice では explicit deferred startup error に留める。runtime は引き続き file path を知らない
- current rejected-auth note: `auth_responses_sent` は accepted auth response send count として扱っており、rejected auth は current one-item send pathでは送信 count に入らない。その代わり `last_rejected_reason=Auth:...` と typed `last_auth_status/last_auth_reason` で visibility を持たせている
- code-level validation では command parser、summary formatter、`max_iterations` stop、`ReceiveTimedOut` stop、timeout-only run summary、repeated auth registry persistence、repeated heartbeat existing-registry reuse、`ClientStats` observation path count、auth rejection visibility、gate rejection visibility、same-client re-registration summary、`run_id` mismatch rejection、heartbeat timeout status、startup failure summary formatting、send failure summary formatting、existing one-iteration runtime non-regression を追加済み
- lightweight smoke validation も完了している。CLI shape は client 側 `--auth-request-poc-once`、`--auth-heartbeat-poc-once`、`--auth-heartbeat-stats-poc-once` を確認済みで、direct `ClientStats`-only sender CLI は未追加だが `--auth-heartbeat-stats-poc-once` で `ClientStats` observation path を刺激できる
- bounded smoke rerun では rebuilt binary を使って `stream-sync-server --receive-send-runtime-bounded configs/examples/server.example.toml 6 5000` と `stream-sync-client --auth-heartbeat-stats-poc-once configs/examples/client.accepted.example.toml` を組み合わせ、`iterations_attempted=4` / `iterations_completed=4` / `auth_requests_received=1` / `auth_responses_sent=1` / `heartbeats_received=1` / `heartbeat_acks_sent=1` / `client_stats_received=1` / `client_stats_returns_sent=1` / `accepted_packets=3` / `send_failures=0` / `registered_clients=1` / `stop_reason=ReceiveTimedOut` を確認済み
- continuous runtime summary には `packets_received` / `accepted_packets` / `rejected_packets` / `frames_reassembled` / `frames_queued` / `direct_frames_queued` / `video_queue_len` / `incomplete_reassembly_frames` / `heartbeat_observations_committed` / `heartbeat_liveness_clients` / `heartbeat_rtt_offset_clients` を追加し、iteration summary には typed continuation reason と queue length を追加した。heartbeat timeout sweep は summary-only で接続し、`NoHeartbeatYet` / `Alive` / `TimedOut` を continuous path からも読める
- continuous runtime focused test では typed continuation / stop reason、auth summary、runtime rejection summary、fragment reassembly + queue counters、heartbeat timeout summary を固定済みで、CLI 側は command parser / success summary / failure summary を固定した
- 実 outbound queue flush、`ServerNotice` 実送信、timeout apply に基づく reconnect policy、file sink open、process-wide logger は未実装
- named-pipe handoff の manual localhost validation は、plain pipe name `streamsync-handoff-dev` を使った one-shot pass と bounded `max_requests=2` pass の両方が成功記録済みで、bounded pass では `inspect-latest` が同じ frame を 2 回返して queue mutation しない preview semantics を確認済み
- switcher 側 reconnect/lifecycle の次 slice は retry 実行より先に no-auto-retry / classification-first を固定し、1 scheduler read = 1 logical request = 1 transport attempt のまま explicit `HandoffError` を保持する方針で進める
- switcher 側 one-request handoff は lifecycle classifier と summary extension まで完了し、`attempt_count=1`、`final_result`、`last_error`、`retry_classification` を持ちながら retry は実行しない
- lifecycle-summary 付き bounded localhost rerun も成功記録済みで、`FrameRead` 成功時に `attempt_count=1`、`final_result=FrameRead`、`last_error=none`、`retry_classification=none` が見え、classification-only で成功系を十分に説明できることを確認済み
- 現時点の MVP では classification-only で十分と判断し、bounded retry wrapper は concrete な transient failure evidence が出るまで保留とする。次タスクは retry 実装ではなく service lifecycle planning に戻す
- service lifecycle の最小次段は full daemon ではなく、server が UDP receive/reassembly/queue と named-pipe handoff serving を同じ process lifetime で持つ bounded service session とする。停止条件はまず `max_requests` を優先し、Ctrl+C / idle-timeout / reconnect manager は後回しにする
- bounded service session は既存 `--receive-auth-video-queue-and-serve-handoff-many` の実装として追加済みで、receive/auth/video queue summary と bounded handoff aggregate/per-request summary を同じ process lifetime で返して自然終了する
- bounded service session の localhost manual pass も成功記録済みで、`FrameRead`、`attempt_count=1`、`final_result=FrameRead`、`last_error=none`、`retry_classification=none` を保ったまま 2 request を処理し、transport/lifecycle phase を閉じられる状態まで確認できた
- 次の major phase は OBS/output boundary より先に 4-view orchestration planning とする。最初の 4-view slice は preview/read-only、shared targetTime、per-view fallible outcome preservation を優先し、generic N-view 化や hotkey/UI は後回しにする
- dedicated `SwitcherFourViewTargetTimeHandoffSourceSchedulerBoundary` による最小 4-view preview/read-only scheduler は実装済みで、4 explicit slots、shared targetTime、preview-only、per-slot selected/no-frame/waiting/handoff-error preservation、aggregate `AllSelected` / `PartialSelected` / `Waiting` / `NoFrames` / `HandoffError`、slot order preservation、preview non-mutation まで focused test で固定した
- dedicated `SwitcherFourViewHandoffSchedulerDecodeRenderAdapterBoundary` による最小 4-view render-facing adapter も実装済みで、scheduler result から 4 explicit slot instructions へ `RenderFrame` / `SkipNoFrameAvailable` / `SkipWaitingForFrameAtOrBeforeTarget` / `SkipHandoffError` を preserving map し、aggregate scheduler status と slot order を保持しながら fake frame を作らないことを focused test で固定した
- dedicated `SwitcherFourViewHandoffDisplayPolicyBoundary` と `SwitcherFourViewHandoffQuadCompositionAdapterBoundary` による最小 4-view display/composition instruction path も実装済みで、slot ごとの update / hold previous / no-display placeholder / source-error placeholder と fixed `QuadView` 2x2 placement を explicit に保持し、placeholder-only slots を drop しないことを focused test で固定した
- dedicated `SwitcherFourViewHandoffQuadCompositionRenderConnectionBoundary` による最小 4-view composition/render-facing connection も実装済みで、既存 `QuadView` composition adapter output から updated slot だけを decode して composition-ready decoded slot へ変換し、held previous / no-display placeholder / source-error placeholder / placeholder-only no-render を explicit に保持しながら fake decoded frame を作らないことを focused test で固定した
- dedicated `SwitcherFourViewQuadCompositionBoundary` による最小 fixed `QuadView` BGRA composition も実装済みで、composition-ready decoded slot 結果から fixed 2x2 の in-memory BGRA canvas を作りつつ、updated / held previous / no-display placeholder / source-error placeholder / decode deferred / decode failed の slot metadata を保持し、placeholder-only no-render と missing decoded pixels invalid を explicit result として focused test で固定した
- `SwitcherFourViewComposedFrame` の次段 planning も更新済みで、次 slice は OBS や即 window render ではなく dedicated 4-view render-facing adapter/connection を先に追加し、`NoRenderableQuadView` / `InvalidQuadView` を explicit no-render state のまま downstream へ渡す方針を architecture/todo に反映済み
- dedicated `SwitcherFourViewQuadRenderFacingConnectionBoundary` による最小 4-view render-facing adapter/connection も実装済みで、`SwitcherFourViewQuadCompositionOutput` から composed BGRA frame を pixel clone せず validation/metadata shaping だけ行い、`RenderReady` / `NoRenderableQuadView` / `InvalidQuadView` と width / height / BGRA payload length / four-slot metadata / aggregate scheduler status / placeholder・source-error 情報を explicit に保持することを focused test で固定した
- 4-view isolated OS window render の planning も更新済みで、次 slice は actual OS proof や OBS 直結ではなく、既存 `SwitcherWindowRenderRuntimeHook` を再利用する dedicated composed-canvas window render boundary を先に追加し、`RenderReady` / `NoRenderableQuadView` / `InvalidQuadView` を collapse せず metadata-visible のまま扱う方針を architecture/todo に反映済み
- dedicated `SwitcherFourViewComposedCanvasWindowRenderBoundary` による最小 4-view composed-canvas window render も実装済みで、`SwitcherFourViewQuadRenderFacingConnectionOutput` から `RenderReady` のみ existing `SwitcherWindowRenderRequest` と injected runtime hook へ接続し、`NoRenderableQuadView` / `InvalidQuadView` は runtime を呼ばず explicit に保持しつつ width / height / BGRA payload length / four-slot metadata / aggregate scheduler status / placeholder・source-error 情報を visible に保つことを focused test で固定した
- 4-view thin orchestration/manual preview planning も更新済みで、次 slice は direct manual CLI や actual OS proof ではなく、2-view validation path と同様に stage output を全部 visible に保つ dedicated 4-view orchestration/validation boundary を先に追加し、その上に bounded one-shot manual preview を later step とする方針を architecture/todo に反映済み
- dedicated `SwitcherFourViewHandoffValidationBoundary` による最小 4-view orchestration/validation boundary も実装済みで、handoff source / decode runtime / window render hook を caller-owned input に取りながら scheduler / decode-render adapter / display policy / QuadView composition instruction / composition render connection / fixed BGRA composition / render-facing / window render の全 stage output を visible に保ち、slot order / aggregate scheduler status / placeholder・source-error metadata を full chain で preserving することを focused test で固定した
- 4-view bounded one-shot manual preview/proof の planning も更新済みで、次 slice は real server->switcher handoff や actual OS proof ではなく、`SwitcherFourViewHandoffValidationBoundary` を thin に包む deterministic な manual wrapper を先に追加し、first proof path は in-process handoff/queue fixture + fake decode/window-render runtime とする方針を architecture/todo に反映済み
- bounded deterministic `SwitcherFourViewManualPreviewProofBoundary` も実装済みで、in-process fixture queue から `SwitcherFourViewHandoffValidationBoundary` を薄く呼び、full 8-stage output を visible に保ったまま target timestamp / scheduler status / per-slot kind / BGRA composition kind / render-facing kind / window render kind / placeholder・source-error count を compact summary として返すことを focused test で固定した
- thin manual CLI/entry point `--four-view-proof-fixture-once` も実装済みで、deterministic fixture mode を選んで `SwitcherFourViewManualPreviewProofBoundary` を呼び、real named-pipe handoff や actual OS window render に依存せず compact proof summary を stdout に出せることを formatter/helper test で固定した
- deterministic 4-view proof fixture CLI validation も完了し、`all-renderable` / `mixed-placeholder-source-error` / `placeholder-only` の 3 mode で expected summary fields、placeholder/source-error preservation、`NoRenderableQuadView` propagation、deterministic behavior、`real_handoff=false`、`actual_window_render=false` を手動 stdout で確認済み
- 次判断は actual OS window proof を OBS/output boundary planning より先に進めることで確定し、最初の actual proof は isolated な別 command で existing `SwitcherWindowRenderRuntimeHook` / composed-canvas window render path を再利用しつつ deterministic `all-renderable` fixture を使う方針まで docs 反映済み
- isolated actual OS window proof command `--four-view-proof-window-once [all-renderable]` も実装済みで、deterministic `all-renderable` fixture を既存 `SwitcherFourViewManualPreviewProofBoundary` に通しつつ actual window render runtime hook を使う separate command として追加した。existing backend-free `--four-view-proof-fixture-once` の挙動は維持し、formatter/helper tests では fake runtime のみを使って default test を real OS window 非依存のまま保っている
- `--four-view-proof-window-once all-renderable` の manual actual OS window proof も成功記録済みで、`scheduler_status=AllSelected`、`bgra_composition_result_kind=ComposedFrame`、`render_facing_result_kind=RenderReady`、`window_render_result_kind=Rendered`、`width=4`、`height=2`、`bgra_payload_len=32`、`placeholder_count=0`、`source_error_count=0` を確認済み。window が即閉じる one-shot 動作は想定どおりで、将来の visual confirmation 用 `--hold-ms` は optional polish としてのみ保留する
- OBS/output boundary planning も更新済みで、OBS の最初の取り込み対象は current proof window ではなく render-facing family の downstream に置く dedicated clean output window とする方針を固定した。OBS は composition internals や handoff transport には直接触れず、最初の implementation slice は OBS API 追加ではなく dedicated clean output window boundary とその metadata/logging preservation にとどめる。`--hold-ms` は proof/preview polish のまま保留し、OBS の前提にはしない
- dedicated `SwitcherFourViewCleanOutputWindowBoundary` による最小 4-view clean output window boundary も実装済みで、`SwitcherFourViewQuadRenderFacingConnectionOutput` を入力に stable `StreamSync 4-view Output` title と hold `0` の dedicated output window request へ接続し、`RenderReady` / `NoRenderableQuadView` / `InvalidQuadView` を collapse せず、width / height / `bgra_payload_len` / aggregate scheduler status / four-slot metadata / placeholder・source-error count / stable window identity を preserve したまま fake runtime test で固定した。existing proof window path は `StreamSync 4-view` title のまま分離維持している
- thin manual/runtime entry point `--four-view-clean-output-window-once [all-renderable]` も実装済みで、deterministic `all-renderable` fixture を dedicated clean output window boundary に通し、stable `StreamSync 4-view Output` title / `clean_output_window=true` / explicit output-window result kind / width / height / `bgra_payload_len` / placeholder・source-error count を compact stdout に出せることを formatter/helper test で固定した。existing proof fixture command と proof window command は unchanged のまま維持している
- `--four-view-clean-output-window-once all-renderable` の manual actual clean output window proof も成功記録済みで、`clean_output_window=true`、`actual_window_render=true`、`real_handoff=false`、`window_title=StreamSync 4-view Output`、`scheduler_status=AllSelected`、`render_facing_result_kind=RenderReady`、`output_window_result_kind=Rendered`、`width=4`、`height=2`、`bgra_payload_len=32`、`placeholder_count=0`、`source_error_count=0` を確認済み。window title は即閉じのため目視確認できなかったが、stdout identity は正しい。proof window path は分離維持しており、将来の visual confirmation 用 `--hold-ms` は optional polish のまま保留する
- OBS Window Capture guidance / validation planning も更新済みで、最初の OBS validation は dedicated clean output window `StreamSync 4-view Output` を手動の Window Capture で選ぶ docs/manual path に固定した。OBS は clean output window の downstream に留め、proof window や composition internals を直接使わない。one-shot immediate close は planning 上の blocker ではないが manual OBS validation には practical limitation なので、次の最小実装 slice は `--hold-ms` より先に dedicated clean output continuous/runtime path を追加して stable capture target を用意する方針にした。OBS WebSocket / advanced OBS control は引き続き out of scope とする
- dedicated clean output continuous/runtime path の planning も更新済みで、最小の次 command は deterministic `all-renderable` fixture を dedicated clean output window `StreamSync 4-view Output` へ bounded frame loop で繰り返し描画する thin runtime とする。最初の control surface は bounded duration ではなく bounded `frames` と fixed 30fps cadence を優先し、stdout summary は少なくとも `frames_attempted` / `frames_rendered` / `render_failures` / `window_title` / `width` / `height` / `bgra_payload_len` を含める。想定 command shape は `--four-view-clean-output-window-loop [all-renderable] [frames]` で、proof window `StreamSync 4-view`・real server->switcher handoff/manual preview・OBS API/WebSocket・`Focused(slot_index)`・full hotkey UI・generic N-view refactor・protocol/H.264 変更・switcher-side fragment reassembly は引き続き out of scope とする
- bounded clean output loop command `--four-view-clean-output-window-loop [all-renderable] [frames]` も実装済みで、deterministic `all-renderable` fixture だけを dedicated clean output window `StreamSync 4-view Output` に対して bounded frame / fixed 30fps cadence で繰り返し描画できる。unsupported fixture mode は explicit に reject し、`frames` は positive bounded integer として validate する。stdout summary には `command_name` / `fixture_mode` / `clean_output_window=true` / `actual_window_render=true` / `real_handoff=false` / `window_title` / `frames_attempted` / `frames_rendered` / `render_failures` / `width` / `height` / `bgra_payload_len` を含め、default tests は fake render runtime と fake cadence hook で real OS window 非依存のまま保っている
- ただし最初の manual loop pass は OBS validation としては失敗記録になっている。stdout では `frames_attempted=300` / `frames_rendered=300` / `render_failures=0` だった一方、OBS Window Capture では window を選択できず preview も出なかった。観測された挙動は「短時間出てすぐ消える window が loop 中に繰り返し現れる」で、1 frame ごとに window を作り直して閉じていたことを示唆した
- そのため persistent clean output window loop が次 slice となり、同じ `--four-view-clean-output-window-loop` command shape のまま one persistent window identity を loop 全体で維持し、frame ごとに update して loop 完了時に一度だけ close する実装へ更新済み。lifecycle summary と focused fake-runtime tests では `window_created` / `persistent_window=true` / `window_updates` / `window_closed` を可視化して、1 window per frame ではなく 1 persistent window session を使うことを固定した
- persistent lifecycle 版 loop の rerun でも OBS Window Capture validation はまだ成功していない。stdout では `frames_attempted=300` / `frames_rendered=300` / `render_failures=0` / `window_created=true` / `persistent_window=true` / `window_updates=300` / `window_closed=true` を確認でき、window recreate 問題は解消した一方で、OBS では window を選択できず preview も出ず、visible surface も黒のままだった
- ここまでで clean output loop の window lifecycle 修正は完了扱いとし、次の主因は lifecycle ではなく OBS-friendly output surface/profile 側にある前提で進める。現状の `width=4` / `height=2` は OBS Window Capture validation target として小さすぎるため、次 slice は persistent lifecycle を維持したまま deterministic `all-renderable` fixture を OBS-friendly size へ拡大する固定 validation profile を優先する
- fixed `1280x720` OBS validation profile も loop command に実装済みで、`--four-view-clean-output-window-loop [all-renderable] [frames]` は persistent lifecycle を保ったまま deterministic source frame を `1280x720` output surface へ nearest-neighbor scale して window runtime へ渡す。loop summary には `source_width` / `source_height` / `output_width` / `output_height` / `scale_mode` / `window_visible` / `window_capture_candidate` を追加し、`bgra_payload_len` も scaled output surface 基準で追跡できるようにした
- `stream-sync-switcher --four-view-clean-output-window-loop all-renderable 900` による manual OBS Window Capture validation も成功記録済みで、OBS は `StreamSync 4-view Output` を選択でき、preview に clean output を表示できた。これは deterministic fixture を使う proof path であり、real server->switcher handoff video ではないが、dedicated clean output window を downstream の OBS Window Capture source として扱えることは確認できた
- 現在の dedicated clean output window は deterministic 4-view QuadView を表し、slot placement は `slot 0=top-left` / `slot 1=top-right` / `slot 2=bottom-left` / `slot 3=bottom-right` で固定される。`real_handoff=false` は引き続き true のままで、次の焦点は OBS capture 自体ではなく real server->switcher handoff をこの 4-view preview/output family へどう接続するかの planning に移る
- real server->switcher handoff + 4-view preview planning も更新済みで、最小の next path は existing named-pipe handoff wrapper/client と `SwitcherFourViewHandoffValidationBoundary` / clean output family を再利用しつつ、まず `1` real handoff slot + `3` deterministic non-real slots で始める方針にした。最初から `2` real slots や `4` real slots へ広げず、transport-to-preview wiring と per-slot no-frame / waiting / handoff-error semantics を狭い manual setup で確認する
- 最初の mixed real preview では intentionally non-real な 3 slot は fixture-backed placeholder content を維持し、configured real slot だけが named-pipe handoff を読む。configured real slot の missing frame は `NoFrameAvailable`、target timestamp より新しすぎる frame は `WaitingForFrameAtOrBeforeTarget`、transport/runtime failure は `HandoffError` として既存 4-view chain に流す。SourceError placeholder は transport failure 由来に限り、missing client を generic source error にしない
- first mixed real preview command `--four-view-real-handoff-preview-loop [pipe-name] [real-slot-index] [client-id] [run-id] [frames]` も実装済みで、`real-slot-index` を `0..3` に validate し、`frames` を positive bounded integer に validate した上で configured real slot だけ existing named-pipe handoff wrapper/client に流し、残り 3 slot は deterministic `NoFrameAvailable` placeholder content に固定する。proof window path は使わず、existing `SwitcherFourViewHandoffValidationBoundary` と dedicated clean output family / persistent output loop / fixed `1280x720` OBS profile を再利用する
- stdout summary には `command_name` / `real_handoff=true` / `real_slot_count=1` / `real_slot_index` / `pipe_name` / `client_id` / `run_id` / `frames_attempted` / `frames_rendered` / `render_failures` / `scheduler_status` / per-slot binding / per-slot result kind / `clean_output_render_result_kind` / `window_title=StreamSync 4-view Output` / `output_width` / `output_height` を追加し、default tests では fake named-pipe handoff / fake render runtime だけを使って real I/O と real OS window 非依存のまま保っている
- `1` real slot + `3` deterministic placeholder / no-frame slots の manual validation も成功記録済みで、client auth / real capture-encode-send / server receive-reassembly-queue / bounded named-pipe handoff serving / switcher named-pipe consumption / existing 4-view clean output family / OBS downstream Window Capture まで確認できた。switcher summary では `frames_attempted=5` / `frames_rendered=5` / `render_failures=0` / `scheduler_status=PartialSelected` / `slot_result_kinds=Selected|NoFrameAvailable|NoFrameAvailable|NoFrameAvailable` / `clean_output_render_result_kind=Rendered` を確認し、OBS は `StreamSync 4-view Output` を表示できた
- 次の planning baseline は `2` real slots + `2` deterministic placeholder / no-frame slots とし、まずは existing bounded server handoff service session 1 本・named pipe 1 本・2 distinct `client_id + run_id` scopes を同じ queue/service lifetime に載せる方針を優先する。最初の `2`-real-slot slice では current `--four-view-real-handoff-preview-loop` を optional positional args で拡張せず、validated 1-real-slot baseline を固定したまま dedicated な `2`-real-slot command shape を planning 候補として扱う
- dedicated `2`-real-slot command `--four-view-two-real-handoff-preview-loop [pipe-name] [slot0-index] [client0-id] [run0-id] [slot1-index] [client1-id] [run1-id] [frames]` も実装済みで、2つの real slot index を `0..3` に validate し、distinct であることを validate した上で、existing named-pipe handoff wrapper/client を 2 real slots に再利用し、残り 2 slot は deterministic `NoFrameAvailable` placeholder content に固定する。existing `SwitcherFourViewHandoffValidationBoundary` と dedicated clean output family / persistent output loop / fixed `1280x720` OBS profile も再利用し、validated 1-real-slot command は unchanged のまま維持する
- `2`-real-slot stdout summary には `command_name` / `real_handoff=true` / `real_slot_count=2` / `real_slot0_index` / `real_slot1_index` / `pipe_name` / `client0_id` / `run0_id` / `client1_id` / `run1_id` / `frames_attempted` / `frames_rendered` / `render_failures` / `scheduler_status` / `slot_bindings` / `slot_result_kinds` / `clean_output_render_result_kind` / `window_title=StreamSync 4-view Output` / `output_width` / `output_height` を出す。default tests では fake named-pipe handoff / fake render runtime のみを使い、real I/O と real OS window rendering には依存しない
- named-pipe handoff preview loop の切り分け強化 slice も実装済みで、server 側 bounded handoff request summary は `queue_len_before_read` / `queue_len_after_read` / `selected_client_id` / `selected_run_id` / `frame_id` / `frame_payload_len` / `no_frame_reason` を出す。ここで従来の `queue_len` は `read 前` ではなく `response 時点の client queue 観測` だったため、今後は before/after を明示して読む
- switcher 側 one-shot handoff summary と real preview loop summary も拡張済みで、named-pipe runtime の `handoff_response_kind` / `response_payload_len` / `parse_error` / `io_error` と、4-view preview の per-slot `slot_diagnostics` を出す。`slot_diagnostics` は `slot_index` / `client_id` / `run_id` / `request_id` / `handoff_response_kind` / `parse_error` / `io_error` / `decode_error` / `response_payload_len` / `frame_id` / `frame_payload_len` / `render_input_kind` / `final_slot_result_kind` を含む
- 直近の `2`-real-slot manual run では server が `FrameRead` / `NoFrame` を交互に返していた一方で switcher summary が `HandoffError|HandoffError|NoFrameAvailable|NoFrameAvailable` を示したため、次の判断は transport/runtime/response-parse failure と slot1 missing-client/no-frame を切り分け直すことになる。existing `--four-view-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 5` は player1 単独 isolation baseline として維持する
- rebuilt diagnostic binaries での rerun では `1`-real-slot baseline が成功し、slot0/player1 は `FrameRead -> Selected -> Rendered` を通過、`parse_error=none` / `io_error=none` / `decode_error=none` を確認できた
- rebuilt diagnostic binaries での `2`-real-slot rerun では switcher `HandoffError` は再現せず、server request order と switcher `slot_diagnostics` の両方で:
  - player1 scope -> `FrameRead`
  - player2 scope -> `NoFrame`
  - slot1 final result -> `NoFrameAvailable`
  であることを確認できた。現行の missing player2 は `HandoffError` ではなく `NoFrameAvailable` として扱われている
- 今回の rerun で分かった実際の blocker は transport parse/runtime failure ではなく server manual receive path の auth gating で、従来は first accepted auth 後の receive phase が追加 auth を受け付けず、`expected_reassembled_frames=2` が player1 単独の 2 frame で満たされると client2 が `AuthResponse(Receive(ConnectionReset))` になっていた
- server manual receive runtime は receive phase 中の追加 auth request も処理する最小修正まで完了し、bounded service session 1 本の中で player1 と player2 の両方を registry/queue へ参加させられる状態になった
- `2` distinct clients が実際に queue 参加する manual pass も成功記録済みで、最小 recipe では `player1=1 frame` / `player2=1 frame` / `expected_reassembled_frames=2`、preferred recipe では `player1=2 frames` / `player2=2 frames` / `expected_reassembled_frames=4` を使い、server receive summary で `registered_clients=2` と両 client scope の queue 参加を確認できた
- 成功した `2`-real-slot handoff/render pass では server bounded handoff request が player1/player2 の両 scope で `FrameRead` を返し、switcher summary では `slot_result_kinds=Selected|Selected|NoFrameAvailable|NoFrameAvailable`、`scheduler_status=PartialSelected`、`clean_output_render_result_kind=Rendered` を確認できた。`PartialSelected` は slot2/slot3 が placeholder のままであることに由来し、failure ではない
- optional client-aware manual stop condition も実装済みで、existing `expected_reassembled_frames` / `stop_after_expected_reassembled_frames` は維持したまま、追加の `expected_reassembled_clients` / `expected_reassembled_frames_per_client` を有効化できる。enabled stop conditions は全て満たされたときだけ receive phase が終了する
- guarded `2`-real-slot rerun も成功記録済みで、server summary では `manual_expected_reassembled_clients=2`、`manual_expected_reassembled_frames_per_client=2`、`observed_reassembled_clients=2`、`per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2`、`stop_reason=ReassembledFramesAndClientAwareThresholdReached` を確認できた。これにより operator sequencing ミスだけで片側 client のみが total-frame threshold を満たして receive phase を閉じる risk は抑えられている
- `4`-client manual configs も追加済みで、`configs/manual/client.player3.toml` と `configs/manual/client.player4.toml` を使って player1..player4 の guarded receive/handoff run を same run id `streamsync-dev-session` 上で起動できる
- dedicated all-real switcher command `--four-view-four-real-handoff-preview-loop [pipe-name] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [frames]` も実装済みで、fixed 4-view slot order `0..3` に real scopes を載せ、placeholder を介さず 4 real slots を existing handoff validation / clean output family に通せる
- guarded `4`-real-slot manual pass も成功記録済みで、server summary では `registered_clients=4`、`manual_expected_reassembled_frames=8`、`manual_expected_reassembled_clients=4`、`manual_expected_reassembled_frames_per_client=2`、`frames_reassembled=8`、`frames_queued=8`、`observed_reassembled_clients=4`、`per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`、`stop_reason=ReassembledFramesAndClientAwareThresholdReached` を確認できた。switcher summary では `slot_result_kinds=Selected|Selected|Selected|Selected`、`scheduler_status=AllSelected`、`frames_rendered=5`、`render_failures=0`、`clean_output_render_result_kind=Rendered` を確認でき、all-real 4-view baseline は成立した
- guarded `4`-real-slot repeated stability observation も `3` 回連続成功で記録済みで、各 run とも server 側 `registered_clients=4` / `frames_reassembled=8` / `frames_queued=8` / `observed_reassembled_clients=4` / `stop_reason=ReassembledFramesAndClientAwareThresholdReached` / `handoff_errors=0`、switcher 側 `frames_rendered=5` / `render_failures=0` / `scheduler_status=AllSelected` / `slot_result_kinds=Selected|Selected|Selected|Selected` / `clean_output_render_result_kind=Rendered` を維持した。変動したのは real capture/encode traffic に伴う `packets_received` / `fragments_received` / `frame_payload_len` 程度で、failure classification が必要な run は出ていない
- guarded `4`-client all-real preview は repeated stability observation `3/3` 成功の baseline として扱い、次の主タスクは full hotkey/UI 実装ではなく operator-facing control surface の最小設計と `Focused(slot_index)` の最小実装へ進める
- dedicated `--four-view-focused-handoff-preview-loop [pipe-name] [focused-slot-index] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [frames]` も実装済みで、validated な all-real `4`-slot handoff path と per-slot diagnostics を維持したまま `view_state=Focused` の full-window render を出せる。first slice では full-window placeholder renderer は追加せず、focused slot が renderable decoded frame を持たない場合は `clean_output_render_result_kind=NoRenderableFocusedView` として明示する
- guarded `4`-client all-real session での `Focused(0..3)` actual manual validation も記録済みで、successful runs では `focused_result_kind=Selected`、`clean_output_render_result_kind=Rendered`、`output_width=1280`、`output_height=720` を確認できた。初回 `Focused(2)` の `frames_rendered=4/5` と初回 `Focused(3)` の server-side `CreatePipe(os_error_231)` は transient runtime/lifecycle wobble として記録し、rerun で成功している
- nearby separate sessions による `AllView -> Focused(0..3) -> AllView` operator-flow validation も成功記録済みで、`12` session すべて `AllSelected` / `Rendered` を維持した。今回の operator-flow pass では short release delay を入れたことで `CreatePipe(os_error_231)` と `HandoffError` は再現していない
- operator-flow validation 後の設計判断も更新済みで、nearby-session command flow は fallback/manual proof path として残しつつ、MVP の次の最小 operator surface は same-session long-running control loop を優先する。hotkey/UI wrapper はその loop の後段で薄く載せる前提とする
- dedicated `--four-view-controlled-handoff-preview-loop [pipe-name] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [max-ticks-per-command] [--commands "..."]` も実装済みで、fixed `4`-real-slot handoff path と existing `AllView` / `Focused(slot_index)` render semantics を 1 process lifetime の control loop に載せられる。first control source は stdin line input または bounded scripted `--commands` で、stdout には `current_view_state` / `requested_transition` / `transition_result` / `selected_slot_result` / `frames_rendered` / `render_failures` / `scheduler_status` / `clean_output_render_result_kind` / `command_index` / `command_parse_error` / `exit_reason` を出す
- guarded real handoff session での same-session scripted manual validation も成功記録済みで、main success script `status -> focus 0 -> focus 1 -> focus 2 -> focus 3 -> all -> status -> quit` は `commands_rejected=0` / `frames_rendered=35` / `render_failures=0` / `scheduler_status=AllSelected` / `clean_output_render_result_kind=Rendered` を維持した。rejected path `focus 9 -> status -> quit` では `transition_result=Rejected` と `command_parse_error=invalid_focus_index:_expected_integer_0..3` を確認しつつ、`status` は `AllView` / `AllSelected` / `Rendered` を保った
- hotkey/UI wrapper の最小設計判断も更新済みで、stdin/scripted control loop は validation baseline として維持しつつ、MVP wrapper の本命は same-session switcher loop に対する separate local control channel とする。Windows では named-pipe 系の local control channel を第一候補とし、wrapper は existing command vocabulary `all` / `focus 0..3` / `status` / `quit` をそのまま送る thin shell に留める。direct hotkey capture inside switcher は後回しとする
- dedicated separate local control channel の最小実装も追加済みで、`--four-view-controlled-handoff-preview-loop ... --control-pipe streamsync-control-dev` が fixed `4`-real-slot same-session loop の第三 control source として動く。request は current command contract と同じ UTF-8 text `all` / `focus 0..3` / `status` / `quit`、response は `command` / `transition_result` / `current_view_state` / `selected_slot_result` / `clean_output_render_result_kind` / `command_parse_error` / `exit_reason` を含む 1 行 summary とし、handoff pipe とは別 pipe name・別 request/response shape に固定した
- minimal manual sender `--send-control-command [control-pipe-name] [command]` も追加済みで、wrapper 本体をまだ作らずに same-session control pipe を 1 request / 1 response で叩ける。stdin / `--commands` baseline と existing successful handoff recipes は維持している。guarded real `4`-client baseline での actual control-pipe validation も成功記録済みで、success path `status -> focus 0 -> focus 1 -> focus 2 -> focus 3 -> all -> status -> quit` は sender response line と loop summary の両方で `commands_rejected=0` / `frames_rendered=35` / `render_failures=0` / `scheduler_status=AllSelected` / `clean_output_render_result_kind=Rendered` を維持した。rejected path `focus 9 -> status -> quit` でも sender response line で `transition_result=Rejected` / `command_parse_error=invalid_focus_index:_expected_integer_0..3` を確認し、final `commands_rejected=1` を記録済み
- current practical note for the local control-pipe path: source 上の parser 変更後でも `target/debug` binary が stale だと `--control-pipe` を旧 parser で拒否する場合があるため、actual validation 前に rebuild しておく。rebuilt binary での rerun では server bounded handoff も `max_requests=140` / `requests_served=140` / `successful_responses=140` / `handoff_errors=0` で収束した
- thin wrapper / hotkey UI の docs-only MVP 設計も更新済みで、implementation form 比較は `A: stream-sync-switcher` 内 wrapper command、`B: separate operator-wrapper app`、`C: first CLI/TUI keyboard loop`、`D: GUI later` として整理した。現時点の推奨は `A + C` で、same-binary でも wrapper は switcher loop と別 process で起動し、control pipe に command を送るだけの thin operator shell に留める。`B` と `D` は後段へ回す
- guarded quit の MVP 仕様も docs に固定済みで、wrapper 側 local guard として `Q` を `2` 秒以内に `2` 回押したときだけ real `quit` command を送る。最初の `Q` は wrapper-local armed message のみ出し、non-`Q` key / timeout / wrapper restart で armed state を解除する。switcher parser 側に追加の quit state は持ち込まない
- same-binary wrapper command `--four-view-operator-wrapper [control-pipe-name] [--keys "s;1;2;3;4;0;q;q"]` も実装済みで、existing `--send-control-command` sender logic を再利用しつつ、wrapper は control pipe に command を送るだけの thin shell に留めている。interactive mode は 1 key token per stdin line、manual/test automation は `--keys` の同一路径を通る
- wrapper 側 stdout も最小実装済みで、per-key line に `wrapper_key` / `mapped_command` / `guard_state` / `send_result` / `response_line` / `command_parse_error` / `wrapper_error` / `exit_reason` を出す。first `Q` は `send_result=GuardArmed` で quit を送らず、second `Q` within `2s` だけ `quit` を送る。unknown key は local ignore として `wrapper_error=unknown_key` を出す
- wrapper code-level validation も追加済みで、key mapping、unknown key、`Q` once no-send、`Q` twice send、non-`Q` clear、guard timeout clear、scripted keys parser を `stream-sync-switcher` test 群で固定した
- optional `--raw-keys` も実装・actual manual validation 済みで、actual control pipe 越しに `AllView` / `Focused(0..3)` / `quit` を成功記録できた。controlled loop final では `commands_processed=11` / `commands_rejected=0` / `current_view_state=AllView` / `frames_rendered=50` / `render_failures=0` / `scheduler_status=AllSelected` / `slot_result_kinds=Selected|Selected|Selected|Selected` / `clean_output_render_result_kind=Rendered` / `output_width=1280` / `output_height=720` / `exit_reason=QuitRequested` を確認し、slot diagnostics final でも `player1..4` 全て `FrameRead` / `parse_error=none` / `io_error=none` / `decode_error=none` / `final_slot_result_kind=Selected` を確認済み
- wrapper raw-key exit freeze 向けの narrow polish も code-level で追加済みで、Windows console mode は raw-key session setup 後に RAII restore guard で必ず原状復帰を試みる。`--keys` / one-line stdin / control-pipe command contract はそのまま維持し、raw-key loop summary には `raw_console_restore_result` / `raw_console_restore_error` を追加した。quit / unknown key / control-pipe send failure / explicit restore failure の focused test を通して restore lifecycle を固定している
- AllView visual mismatch の fix 後 actual rerun と human visual confirmation も完了している。`s -> status` では `current_view_state=AllView` / `view_render_mode=AllView` / `output_layout=QuadView` / `rendered_slot_count=4` / `focused_slot_index=none` / `all_view_render_result_kind=Rendered` を確認し、`1..4` では `Focused(0..3)` + `FocusedFullWindow` + `rendered_slot_count=1` を確認、`0 -> all` では `Transitioned` と quad return、`a -> all` では `NoChange` と quad 維持、wrapper final では `input_source=raw_keys` / `raw_console_restore_result=restored` / `raw_console_restore_error=none` / `exit_reason=QuitRequested` を確認した。human visual confirmation でも `0` 後に 4画面へ戻ること、`a` 後に 4画面のまま維持されること、OBS / Window Capture で黒画面が出ないことを確認済みで、AllView visual mismatch は修正完了扱いにする
- raw-key actual validation 中の `Focused(3)` では `command_index=4` に一瞬 `scheduler_status=HandoffError` が見えたが、同じ command line で `selected_slot_result=Selected` / `clean_output_render_result_kind=Rendered` / `frames_rendered=5` を維持し、final summary では `scheduler_status=AllSelected` に戻っている。これは transient scheduler-status wobble として later narrow polish に留め、MVP blocker にはしない
- production H.264 encoder configuration / error logging policy の first implementation slice も実装済みで、client real encoded PoC は optional な `[video.encoder]` を読める。manual `client.player1..4.toml` には MVP `ffmpeg_libx264` profile (`1280x720` / `30fps` / `4500kbps` / `gop_frames=30` / `ultrafast` / `zerolatency` / `yuv420p` / `main` / `3.1`) を追加済みで、bounded sender stdout には encoder config visibility、FFmpeg preflight/runtime visibility、`last_encode_error` / `last_ffmpeg_error` / `last_payload_len` / `oversized_payload_count` / `fragmentation_pressure_count` を追加済み。未設定 config では current implicit defaults を維持する
- `[video.encoder]` profile manual rerun も成功記録済みで、`client.player1..4` 全てで `ffmpeg_libx264` / `1280x720` / `30fps` / `4500kbps` / `gop_frames=30` / `ultrafast` / `zerolatency` / `yuv420p` / `main` / `3.1`、`ffmpeg_path=ffmpeg`、`ffmpeg_version_detected=ffmpeg version 8.1-full_build-www.gyan.dev`、`ffmpeg_preflight_error=none`、`ffmpeg_spawn_error=none`、`frames_captured=2`、`frames_encoded=2`、`frames_sent=2`、`encode_failures=0`、`frame_build_failures=0`、`send_failures=0`、`last_encode_error=none`、`last_ffmpeg_error=none`、`last_payload_len=65363`、`oversized_payload_count=0`、`fragmentation_pressure_count=2` を確認した。同じ bounded rerun で switcher final `commands_processed=9` / `frames_rendered=40` / `scheduler_status=AllSelected` / `clean_output_render_result_kind=Rendered`、wrapper final `input_source=raw_keys` / `keys_processed=10` / `commands_sent=9` / `raw_console_restore_result=restored` / `exit_reason=QuitRequested`、human visual confirmation では `AllView` / `Focused(player1..4)` / `0` で AllView 復帰 / `a` で AllView 維持 / OBS 黒画面なしも確認済みで、profile wiring と production H.264 stdout visibility は MVP evidence 取得済みとして完了扱いにする
- client encoder wiring 後の workspace tests も green に戻してあり、server handoff summary tests は current `queue_len_before_read` / `queue_len_after_read` / `frame_payload_len` semantics に揃っている
- same-session bounded server lifecycle については、request-budget formula `render_command_count * max_ticks_per_command * real_slot_count` は引き続き docs に残すが、今回の rebuilt control-pipe rerun では追加 flush read なしで `140` request に収束した。flush/exit polish は wrapper 設計の blocker ではなく、request-budget calculation / extra flush read edge case / summary flush の later narrow polish 候補として扱う
- upcoming `2`-real-slot manual validation 用の dedicated test configs も追加済みで、`configs/manual/server.two-real-slots.toml`、`configs/manual/client.player1.toml`、`configs/manual/client.player2.toml` を使って existing examples を崩さずに 2-client same-run test を行える。現行 switcher command 群はこの path で switcher config を読まないため、`configs/manual/switcher.two-real-slots.toml` はこの slice では追加していない
- video path は server 側 accepted `VideoFrame` receive side-effect を caller-owned per-client queue へ保存し、client 側で placeholder encoded H.264 payload 付き `VideoFrame`、Windows Graphics Capture + FFmpeg による one-shot `RealCaptureH264` `VideoFrame`、認証済み same-source の bounded multi-frame `RealCaptureH264` sender、送信失敗時の detailed diagnostics、safe UDP datagram 前提の sender-side `VideoFrame` fragmentation、手動PoC向け fragment pacing まで完了し、manual E2E checklist も整備済み。server 側は accepted `VideoFrameFragment` の caller-owned reassembly state、duplicate / metadata rejection、完成 frame の既存 queue storage への接続、手動確認用の fragment / reassembly / queue stdout diagnostics、max packet / timeout / expected frame / stop condition の手動 policy、incomplete frame progress diagnostics、手動 receive path の UDP socket receive buffer tuning と requested/effective stdout diagnostics、client/run 指定の queued encoded frame inspect/dequeue 境界まで完了している。fragmented real encoded queue PoC は `8388608` byte effective receive buffer で manual 1-frame / 2-frame とも成功し、最新の `max_frames=2` run では client `fragments_sent=854/854`、server `fragments_received=854`、`frames_reassembled=2`、`frames_queued=2`、`incomplete_reassembly_frames=0`、`receive_timed_out=false` を確認済み。switcher 側の fragmented frame direct consumption は未実装。switcher 側は latest frame を FFmpeg で H.264 decode して 1 frame BMP dump し、Windows では decoded BGRA を normal window に one-shot 描画し、single-client latest-frame の bounded continuous decode/render loop 境界、client/run 指定の single-client queue source 境界、server queue を読む switcher-facing queued-frame source trait/interface と in-process adapter、transport-neutral / fallible queued-frame handoff contract と in-process implementation、その handoff result を既存 queue-source result shape へ変換しつつ handoff error を no-frame に潰さない consumer boundary、handoff error を no-frame / waiting に潰さない fallible single-client targetTime handoff source 境界、handoff error を partial/no-frame/waiting に潰さない fallible 2-view targetTime handoff scheduler 境界、fallible scheduler result から decode/render-facing instructions への adapter 境界、その adapter output から display-policy-facing decode/render result への fallible connection 境界、その connection output から update / hold / stale / no-display を決める fallible display policy 境界、その display policy output から composition-facing updated / held / stale / no-display / source-error placeholder instructions への fallible adapter 境界、queued-frame source 経由の single-client targetTime selection と 2-view targetTime source scheduler、scheduler result から既存 2-view decode/render input への adapter 境界、adapter output から既存 `SwitcherTwoViewDecodeRenderBoundary` へ渡す in-process connection 境界と live-like validation、2-view display policy 境界、display policy から既存 2-view composition input への adapter 境界、その adapter output を既存 composed canvas render path へ通す in-process validation 境界、one-client targetTime / jitter-buffer selection 境界、2-view targetTime selection orchestration 境界、2-view targetTime-selected decode/render connection 境界、2-view sync fixture/manual verification CLI、2-view side-by-side BGRA layout/composition 境界、composed 2-view canvas window render 境界、live-like 2-client queue/runtime integration 境界、bounded continuous 2-view scheduling 境界、real UDP socket-backed source adapter 境界、auth registry 生成込み live two-view switcher manual runtime、fallible server-mediated 2-view validation boundary、transport-neutral な server->switcher handoff request/response DTO、length-prefixed explicit binary codec、server 側 single-request handoff handler、switcher 側 DTO request builder / response mapper、Windows named-pipe one-request / one-response server/client runtime、existing `SwitcherQueuedFrameHandoff` に載せる thin wrapper と wrapper-owned monotonic request-id policy、one-shot named-pipe handoff の server/switcher manual CLI、plain pipe name `streamsync-handoff-dev` を使った localhost one-shot handoff 成功確認、bounded `max_requests` を前提にした continuous accept loop / reconnect / lifecycle planning、server 側 bounded named-pipe `serve_many(..., max_requests)` runtime と per-request summary aggregation、switcher 側 one-request handoff の per-request timeout config / elapsed summary / explicit runtime status plumbing まで完了している。`run_fallible_*` 専用の manual/runtime entry point は transport planning 前には追加しない方針とし、`--live-two-view-switcher-once` は direct receive diagnostic/legacy のまま main path へ戻さない。real server->switcher handoff の最初の production-like transport は Windows named pipe を含む local IPC byte-stream request/response とし、switcher-pull/read を維持し、client UDP ingest protocol や `VideoFrame` wire format とは分離した internal handoff codec を使う方針まで確定した。DTO/codec は `crates/net-core` に置き、server handler / named-pipe one-request runtime / bounded `serve_many` runtime は `apps/server`、switcher client adapter / named-pipe one-request runtime / thin handoff wrapper と one-shot CLI は `apps/switcher` に置く。named-pipe smoke test は Windows local test として isolate し、default handoff validation では fake runtime と focused non-I/O mapping test を使う。manual CLI では server が `--receive-auth-video-queue-and-serve-handoff-once` で queue-owning receive 後に one request を serve し、switcher が `--read-queued-frame-handoff-once` で one request を pull/read する。request_id は supplied 時は preserve、omitted 時は one-shot process の initial monotonic value `1` を使う。switcher 側 timeout は現時点では one request ごとの named-pipe connect/wait timeout のみを持ち、retry manager はまだ持たない。現行 CLI 引数では full pipe path `\\.\pipe\...` ではなく plain pipe name を使う。次の runtime/service slice は bounded server loop summary の CLI/manual 露出と、さらに小さい switcher reconnect/lifecycle policy の整理を前提にする。late-drop mutation、4-view sync、OBS は未着手

---

## 決定済み方針
- [x] プロジェクト名は `StreamSync`
- [x] リポジトリ名 / ルートフォルダ名は `stream-sync`
- [x] MVP は 4 人固定
- [x] 完全同期に近い映像同期基盤を最優先する
- [x] 初期標準品質は 720p / 30fps
- [x] 1080p / 60fps は条件付き上位運用モード
- [x] 言語は Rust
- [x] 映像処理は FFmpeg 系
- [x] 通信は UDP 独自プロトコル
- [x] コーデックは H.264
- [x] UI は Rust 製の最小 GUI
- [x] OBS 連携は switcher 専用ウィンドウの Window Capture
- [x] 設定ファイルは TOML
- [x] ログは JSON Lines 形式の構造化ログ
- [x] 認証は事前共有トークン方式 + clientId ホワイトリスト
- [x] `app_version` と `protocol_version` は分離管理
- [x] MVP の音声は Discord 継続使用
- [x] client 4 台が中央 server に直接 UDP 送信するスター構成
- [x] server が同期責任を持つ
- [x] switcher は表示専用
- [x] MVP 初期段階では server と switcher は同一 PC 運用でよい

---

## 直近でやること
1. 次 human rerun は `S:\stream-sync` で `--continuous-decoder-output-pipeline-experiment scaled-bgr24` の conversion optimization 後 evidence を取り、valid runtime は引き続き `C:\streamsync-target\stream-sync-rerun\debug\*.exe` を使う
2. rerun では conversion mode / reuse count / allocation count / bytes written / pixel convert avg-max-count と、output throughput / completed latency / pending age / output lag を default-bgra baseline と比較する。raw pipe bytes hypothesis PARTIAL PASS、default-bgra 継続、scaled-bgr24 adoption HOLD / FAIL は rerun まで維持する
3. one-shot fallback は正常 escape hatch として残す。one-shot suppression は strong contributor evidence だが、今回の主問題は pending correspondence / stdout reader full-frame latency / continuous output backlog / stale output として扱う
4. incremental quad compose / render/GDI は PASS として維持し、same-PC rerun 比較では `quad_view_incremental_update_count` / `quad_view_full_compose_count` / `quad_view_compose_elapsed_ms` / `gdi_paint_wait_elapsed_ms` / `placeholder_visual_changed_count` を regression guard として残す
5. request/response persistent decoder revive / slot1 continuous化 / 4-client widening / shared-memory / GPU backend / distributed-PC actual run には進まず、first slice の slot0-only opt-in evidence を先に読む

## 今後の大まかな指針
- 残り todo は `MVP クリティカルパス`、`安定化 / 運用`、`future task` に分けて扱う
- `logging sink config` や `failure injection` の follow-up は有用だが、同期安定化・queue policy・長時間 validation より後ろに置く
- まずは「4人を real handoff で安定表示し続ける」ことを基準に優先度を決める

## 残り todo から見た推定 step
- 目安は `5-6 step`。1 step は Codex と GPT の 1 往復で数える
1. 人間が `docs/operations/distributed-pc-validation.md` の `S:\stream-sync` command pack を最終確認する
2. distributed-PC `2`-client smoke の pasted-back evidence を回収して PASS/PARTIAL/FAIL を確定する
3. distributed-PC `2`-client OBS visible の pasted-back evidence を回収して runtime evidence と visual evidence を切り分ける
4. distributed-PC `4`-client summary-required run を実施して widening 可否を確定する
5. long OBS run を visual stability evidence として記録する
6. same-PC saturation と distributed failure-class-specific fixes を分離して詰める

## MVP closeout 時点で blocker ではなかった future task
- [ ] same-session bounded server lifecycle polish
- [ ] transient scheduler-status wobble
- [ ] wrapper stdin zero-gap wobble
- [ ] long-running quality / block noise / latency evaluation
- [ ] hardware encoder integration
- [ ] full GUI / `apps/operator-wrapper` split
- [ ] OBS WebSocket / advanced OBS control
- [ ] continuous receive/send runtime later expansion

上記は future task として TODO に残すが、現時点では closeout blocker にしない。
closeout / push 判断では以下を優先する。

- final regression green
- closeout docs updated
- current MVP scope と future scope の分離

continuous runtime phase へ移った後の non-blocker:

- same-session bounded server lifecycle polish
- transient scheduler-status wobble
- wrapper stdin zero-gap wobble
- long-running quality / block noise / latency evaluation
- hardware encoder integration
- full GUI / `apps/operator-wrapper` split
- OBS WebSocket / advanced OBS control

continuous runtime first slice の blocker:

- [x] repeated outer loop ownership
- [x] socket / auth registry / outbound queue / writer lifetime persistence
- [x] bounded stop policy
- [x] aggregate runtime summary
- [x] repeated auth / heartbeat / client-stats bounded smoke validation path
- [x] continuous receive/send runtime ownership
- [x] typed continuation / stop reason
- [x] runtime hardening summary visibility from continuous path
- [x] fragment reassembly / queue counters for long-run observation
- [x] heartbeat timeout summary visibility from continuous path

## 将来の polish 候補
- [ ] `--four-view-proof-window-once` / `--four-view-clean-output-window-once` に visual confirmation 用の `--hold-ms` / preview hold duration を追加するか後で判断する

---

## 仕様 / 設計
- [x] `docs/requirements/project-overview.md` を作成する
- [x] `docs/architecture/system-design.md` を作成する
- [x] `docs/architecture/protocol.md` を作成する
- [x] `docs/architecture/decisions.md` を作成する
- [x] README を作成する
- [x] PoC 完了条件を定義する
- [x] MVP 完了条件を定義する
- [x] MVP でやらないことを定義する
- [x] 将来拡張項目を整理する
- [x] コンポーネントごとの責務を定義する
- [x] protocol / net-core / server の受信 decode 境界を整理する
- [x] server inbound handler 境界を整理する
- [x] server UDP receive loop 境界を整理する
- [x] server auth handler 境界を整理する
- [x] client whitelist 読み込みと token 検証の設定入力境界を整理する
- [x] auth success / failure ログ出力境界を整理する
- [x] auth success / failure の JSON Lines ログイベント仕様を整理する
- [x] auth result writer を one-shot server stderr へ接続する
- [x] auth decision から `AuthResponse` outbound queue handoff までの server step を整理する
- [x] 認証済み送信元の登録 / 管理境界を整理する
- [x] accepted auth path で認証済み送信元を in-memory registry へ登録する
- [x] 未認証 / endpoint mismatch packet の破棄境界を整理する
- [x] receive loop から packet acceptance gate を呼ぶ接続境界を整理する
- [x] registered packet を heartbeat / video frame handler へ渡す接続方針を整理する
- [x] registered heartbeat packet から `HeartbeatAck` queue handoff までの最小接続方針を整理する
- [x] heartbeat state / RTT / offset 推定へ渡す入力境界を整理する
- [x] heartbeat liveness state commit と timeout evaluation の最小境界を整理する
- [x] timeout evaluation 結果を auth invalidation / timeout log / timeout notice へ接続する最小方針を整理する
- [x] timeout action plan を continuous loop から実適用する最小方針を整理する
- [x] timeout evaluation / action plan / apply boundary を future continuous loop から呼ぶ最小方針を整理する
- [x] RTT / offset estimate を server 側 state に commit する最小境界を整理する
- [x] RTT / offset smoothing / outlier policy の最小範囲を整理する
- [x] heartbeat state / RTT / offset 推定の本計算方針を整理する
- [x] heartbeat RTT / offset の小さな実計算単位を決める
- [x] heartbeat client ack observation flow を設計する
- [x] heartbeat observation carrier を設計する
- [x] `ClientStats` payload encode/decode 方針を決める
- [x] `ClientStats` payload encode/decode の最小実装を追加する
- [x] `ClientStats` receive route / gate / registered handler bridge を追加する
- [x] packet acceptance rejection を drop / log layer へ渡す境界を整理する
- [x] AuthResponse 生成 / 送信境界を整理する
- [x] outbound packet / queue 境界を整理する
- [x] outbound queue の最小実処理方針を整理する
- [x] outbound queue の backpressure / capacity 方針を整理する
- [x] net send layer / protocol encoder 境界を整理する
- [x] `HeartbeatAck` encode 入力境界を整理する
- [x] UDP socket 送信前の send error / log event 方針を整理する
- [x] receive rejection の JSON Lines ログイベント仕様を整理する
- [x] receive rejection ログ出力の最小実装を追加する
- [x] auth / receive JSON Lines writer 接続範囲を整理する
- [x] send JSON Lines writer の one-iteration 最小実接続範囲を整理する
- [x] UDP socket 受信 / 送信本体の最小実装を追加する
- [x] `VideoFrame` encode 方針と最小実装範囲を整理する
- [x] UDP socket を auth response PoC の起動処理へ最小接続する
- [x] auth response PoC の起動設定接続を追加する
- [x] client 側 AuthRequest one-shot PoC の flow と責務分離を整理する
- [x] server / client one-shot auth round trip の手動確認手順を追加する
- [x] server / client one-shot auth round trip の accepted path 用 helper config と手順を追加する
- [x] server / client one-shot auth round trip の accepted path 成功結果を記録する
- [x] `shared_token_env` を使う one-shot auth round trip 手順を追加する
- [x] `shared_token_env` one-shot auth round trip accepted path 成功結果を記録する
- [x] `--receive-send-once` accepted auth request の手動通し確認結果を記録する
- [x] secret 解決方式と token 保護方針を整理する
- [x] secret resolver 本実装範囲を確定する
- [x] `shared_token_env` secret resolver の最小本実装を追加する
- [x] `ServerNotice` payload layout と decode / encode 方針を決める
- [x] `ServerNotice` notice trigger policy の実装範囲を整理する
- [ ] 状態遷移を詳細化する
- [ ] 異常時の挙動を実装レベルに落とす
- [ ] ログイベント仕様を詳細化する
- [ ] 配信時の運用方針を手順書へ落とす
- [ ] バージョン互換性ルールを実装と運用手順へ反映する

---

## protocol / wire format
- [x] 共通型定義を作る
- [x] `ClientId`, `RunId`, `AppVersion`, `ProtocolVersion` を定義する
- [x] `TimestampMicros` を定義し、timestamp 単位をマイクロ秒に整理する
- [x] `AuthRequest` / `AuthResponse` の Rust 型を定義する
- [x] `Heartbeat` / `HeartbeatAck` の Rust 型を定義する
- [x] `VideoFrame` の最小構造を定義する
- [x] `ClientStats` / `ServerNotice` の最小型を定義する
- [x] `MessageType`, `Codec`, `NoticeType`, auth reason code を定義する
- [x] PoC / MVP 初期の最小 wire format を 16 byte fixed header として整理する
- [x] 数値フィールドを little-endian とする方針を整理する
- [x] `message_type`, `header_length`, `protocol_version`, `payload_length`, `flags`, `reserved` を fixed header に定義する
- [x] fixed header decode を実装する
- [x] `protocol_version` 期待値チェックを実装する
- [x] payload decoder dispatch helper を実装する
- [x] `AuthRequest` payload byte layout と decode を実装する
- [x] `AuthResponse` payload byte layout と decode を実装する
- [x] `Heartbeat` payload byte layout と decode を実装する
- [x] `HeartbeatAck` payload byte layout と decode を実装する
- [x] `VideoFrame` payload byte layout と decode を実装する
- [x] `AuthResponse` payload byte layout と encode input boundary を整理する
- [x] `HeartbeatAck` payload layout / encode 方針を決める
- [x] `ProtocolMessage::message_type()` と `ProtocolMessageEncoderBoundary` placeholder を追加する
- [x] `AuthRequest` encode 本実装を行う
- [x] `AuthResponse` encode 本実装を行う
- [x] `Heartbeat` encode 本実装を行う
- [x] `HeartbeatAck` encode 本実装を行う
- [x] `VideoFrame` encode 方針と最小実装範囲を整理する
- [x] `VideoFrame` encode 本実装を行う
- [x] fixed header encode 本実装を行う
- [x] `ClientStats` payload layout と decode / encode 方針を決める
- [x] `ClientStats` payload encode/decode 本実装を行う
- [x] `ServerNotice` の payload layout と decode / encode 方針を決める
- [x] `ServerNotice` の payload encode/decode 本実装を行う
- [x] `ProtocolMessageEncoderBoundary` と decode dispatch の `ServerNotice` 対応を追加する
- [x] payload fragmentation の要否と方式を決める
- [x] `VideoFrameFragment` server-side reassembly の最小方針を決める
- [ ] 再送制御 / 暗号化は MVP 初期で扱うか保留するか明記する

---

## net-core / server 境界
- [x] `InboundPacket` / `PacketSource` / `InboundPacketDecoder` / `DecodedInboundPacket` / `NetDecodeError` を追加する
- [x] raw packet bytes と送信元 metadata を protocol decode 結果へ変換する境界を定義する
- [x] server 側 `ServerInboundRouter` / `ServerInboundRoute` placeholder を追加する
- [x] `AuthRequest` / `Heartbeat` / `VideoFrame` の server route 分類を定義する
- [x] `ServerReceiveLoopStep` / `ServerReceiveLoopOutcome` / `ServerRejectedPacket` placeholder を追加する
- [x] `ServerContinuousReceiveLoopLifecycleBoundary` / continuous receive loop lifecycle placeholder を追加する
- [x] `ServerContinuousReceiveLoopTickBoundary` / continuous receive loop tick placeholder を追加する
- [x] `ServerContinuousReceiveLoopWriterHandoffBoundary` / operational・rejection writer handoff placeholder を追加する
- [x] `ServerContinuousReceiveLoopWriterRuntimeBoundary` / caller-owned writer runtime handoff placeholder を追加する
- [x] `ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary` / handler handoff runtime placeholder を追加する
- [x] `ServerContinuousReceiveLoopOneTickRuntimeBoundary` / minimal one-tick runtime execution placeholder を追加する
- [x] `ServerContinuousReceiveLoopBodyBoundary` / minimal loop body placeholder を追加する
- [x] `ServerContinuousReceiveLoopControllerBoundary` / outer controller lifecycle placeholder を追加する
- [x] `ServerContinuousReceiveLoopHandlerDispatchBoundary` / handler dispatch bridge placeholder を追加する
- [x] `ServerHandlerDispatchBoundary` / handler dispatch result placeholder を追加する
- [x] `ServerAuthDispatchRuntimeBoundary` / auth dispatch runtime placeholder を追加する
- [x] `ServerRegisteredPacketDispatchRuntimeBoundary` / registered packet dispatch runtime placeholder を追加する
- [x] `ServerVideoStatsHandlerRuntimeBoundary` / video stats handler input runtime placeholder を追加する
- [x] `ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary` / body dispatch runtime placeholder を追加する
- [x] `ServerDispatchRuntimeSideEffectApplyBoundary` / dispatch side effect apply placeholder を追加する
- [x] `ServerDispatchRuntimeOutputApplyBoundary` / accepted auth queue storage and auth log writer placeholder を追加する
- [x] `ServerOutboundQueueCollectionBoundary` / queue collection placeholder を追加する
- [x] `ServerOutboundSendOneRuntimeBoundary` / one-item encode and socket send runtime placeholder を追加する
- [x] `ServerReceiveSendOneIterationRuntimeBoundary` / receive-send one iteration integration placeholder を追加する
- [x] `ServerControllerReceiveSendRuntimeBoundary` / controller receive-send runtime placeholder を追加する
- [x] `ServerReceiveSendOneIterationLauncher` / completed one-iteration runtime CLI config entry placeholder を追加する
- [x] `ServerReceiveSendTwoIterationLauncher` / auth-then-heartbeat two-iteration runtime CLI config entry を追加する
- [x] `ServerReceiveSendThreeIterationLauncher` / heartbeat observation return three-iteration runtime CLI config entry を追加する
- [x] decode error / protocol error の分類方針を定義する
- [x] `OutboundPacket` / `OutboundQueueItem` / `OutboundPacketQueueBoundary` placeholder を追加する
- [x] `QueuedOutboundItem` / `OutboundQueueItemState` / `OutboundQueueLifecycleBoundary` placeholder を追加する
- [x] `OutboundQueueStorageState` / `OutboundQueueStorageBoundary` placeholder を追加する
- [x] `OutboundEncodeRequest` / `EncodedOutboundPacket` / `OutboundPacketEncoderBoundary` / `NetEncodeError` placeholder を追加する
- [x] `OutboundSendLogContext` / `SendLogEvent` / send failure classification placeholder を追加する
- [x] `OutboundSendLoopTickBoundary` / send loop tick state placeholder を追加する
- [x] `OutboundSendLoopLifecycleBoundary` / send loop lifecycle placeholder を追加する
- [x] `ServerSendLogOutputBoundary` / one-iteration send success/failure JSON Lines writer を追加する
- [x] `ServerSendErrorLogOutputBoundary` / send error JSON Lines writer placeholder を追加する
- [x] server 側 `ServerOutboundQueueBoundary` placeholder を追加する
- [x] server 側 `ServerHeartbeatAckBoundary` / `ServerOutboundHeartbeatAck` placeholder を追加する
- [x] server 側 `ServerNoticeBoundary` / `ServerOutboundNotice` placeholder を追加する
- [x] server 側 `ServerNoticeTriggerPolicyBoundary` / trigger plan placeholder を追加する
- [x] server 側 `ServerHeartbeatHandlerBoundary` / `ServerHeartbeatAckHandoff` placeholder を追加する
- [x] server 側 `ServerHeartbeatInputBoundary` / state input / timebase input placeholder を追加する
- [x] server 側 `ServerHeartbeatRttOffsetCommitBoundary` / `ServerHeartbeatRttOffsetState` placeholder を追加する
- [x] server 側 `ServerHeartbeatRttOffsetCandidatePolicyBoundary` placeholder を追加する
- [x] server 側 `ServerHeartbeatRttOffsetPolicyCommitBoundary` / rejected candidate skip result を追加する
- [x] server 側 `ServerHeartbeatRttOffsetRejectedCandidateHandoffBoundary` / rejected candidate JSON Lines event / metrics handoff placeholder を追加する
- [x] server 側 `ServerHeartbeatRttOffsetRejectedCandidateMetricsState` / commit boundary / snapshot export placeholder を追加する
- [x] server 側 `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary` / consumer placeholder を追加する
- [x] server 側 `ServerHeartbeatLivenessCommitBoundary` / `ServerHeartbeatLivenessState` / timeout evaluation boundary を追加する
- [x] server 側 `ServerHeartbeatTimeoutActionBoundary` / timeout log event / auth invalidation command placeholder を追加する
- [x] server 側 `ServerHeartbeatTimeoutApplyBoundary` / timeout log caller-owned writer / notice handoff placeholder を追加する
- [x] server 側 `ServerHeartbeatTimeoutNoticeQueueStorageBoundary` / timeout notice send wakeup plan placeholder を追加する
- [x] server 側 `ServerHeartbeatTimeoutLoopTickBoundary` / one-client timeout runtime placeholder を追加する
- [x] server 側 `AuthenticatedSenderRegistryBoundary` / `AuthenticatedSenderRegistry` placeholder を追加する
- [x] server 側 `PacketAcceptanceGateBoundary` / `PacketAcceptanceDecision` placeholder を追加する
- [x] server 側 `ServerRegisteredPacketBoundary` / registered handler input placeholder を追加する
- [x] `ServerReceiveLoopGateOutcome` / receive loop から gate を呼ぶ接続 helper を追加する
- [x] `ServerReceiveLoopLogOutputBoundary` / receive loop operational JSON Lines writer placeholder を追加する
- [x] `ServerRejectionDropLogHandoffBoundary` / drop-log handoff input placeholder を追加する
- [x] `ServerReceiveRejectionJsonLogEventBoundary` / receive rejection JSON Lines event input placeholder を追加する
- [x] `ServerReceiveRejectionLogOutputBoundary` / receive rejection JSON Lines writer を追加する
- [x] UDP socket の bind / receive / send 最小実装を行う
- [x] bind 済み UDP socket から 1 packet を受信する最小処理を追加する
- [x] encode 済み bytes と destination を UDP socket へ送信する最小処理を追加する
- [x] `ServerUdpSocketIoStep` で受信 packet を receive loop / gate 境界へ渡す
- [x] `ServerAuthResponsePocStep` で UDP socket から auth response send までを 1 回分接続する
- [x] `ServerAuthResponsePocLauncher` で server 設定から bind / auth config / registry 初期化 / PoC step 呼び出しを接続する
- [x] `ClientStats` を server inbound route / packet acceptance gate / registered handler bridge に接続する
- [ ] packet 受信継続 loop を実装する
- [x] continuous receive loop 本体の実装範囲を整理する
- [x] continuous receive loop の 1 tick 実接続範囲を整理する
- [x] continuous receive loop から operational / rejection writer への実接続範囲を整理する
- [x] continuous receive loop の writer 呼び出し実接続範囲を整理する
- [x] continuous receive loop 本体へ進む前の handler handoff 実接続範囲を整理する
- [x] continuous receive loop 本体の最小 1 tick 実行接続範囲を整理する
- [x] continuous receive loop の最小 loop body 実装を追加する
- [ ] packet 送信継続 loop を実装する
- [x] packet 送信継続 loop の最小接続範囲を整理する
- [x] packet 送信継続 loop 本体の実装範囲を整理する
- [x] receive rejection の最小 stderr JSON Lines 出力を実装する
- [x] receive loop の継続運用向けログ範囲を整理する
- [ ] receive loop の継続運用向けログ出力を実装する
- [ ] outbound queue の実処理を実装する
- [x] outbound queue の backpressure / capacity 方針を決める
- [x] outbound queue の実キュー実装範囲を送信継続 loop 前提で再確認する
- [x] send error の分類とログ方針を整理する
- [x] send error JSON Lines 出力範囲を整理する
- [ ] send error ログ出力を実装する
- [ ] async runtime 導入方針を決める

---

## 認証まわり
- [x] 認証方式を事前共有トークン + clientId ホワイトリストに決定する
- [x] `AuthRequest` / `AuthResponse` 型を定義する
- [x] `AuthRequest` payload decode を実装する
- [x] `AuthResponse` 生成 / 送信境界を定義する
- [x] `ServerAuthHandlerBoundary` / `ServerAuthCheck` / `ServerAuthBoundaryError` placeholder を追加する
- [x] `ServerAuthConfigInputBoundary` / `ServerAuthCheckInput` placeholder を追加する
- [x] `ServerAuthDecision` / `ServerAuthResponseBoundary` / `ServerOutboundAuthResponse` placeholder を追加する
- [x] `ServerAuthLogHandoffBoundary` / `ServerAuthLogInput` placeholder を追加する
- [x] `ServerAuthJsonLogEventBoundary` / `ServerAuthJsonLogEventInput` placeholder を追加する
- [x] `ServerAuthLogOutputBoundary` / auth result JSON Lines writer を追加する
- [x] one-shot auth response PoC の auth result JSON Lines stderr 出力を追加する
- [x] 認証判定入力として `shared_token` / `client_id` / `protocol_version` / `app_version` を参照できる形を定義する
- [x] client whitelist / token 情報を認証判定入力へ変換する設定入力境界を定義する
- [x] server auth decision の最小実装を追加する
- [x] `UnknownClient` / `InvalidToken` / `InternalError` の最小 rejected reason を返す
- [x] `ServerAuthFlowStep` で `ServerAuthCheckInput` -> `ServerAuthDecision` -> `ServerOutboundAuthResponse` -> `OutboundQueueItem` を接続する
- [x] server 設定 TOML から client whitelist / token 情報を読み込む
- [x] UDP socket から `AuthRequest` を 1 packet 受信し、`AuthResponse` を 1 packet 返す PoC 接続を追加する
- [x] server 設定から auth response PoC 起動入口を接続する
- [x] server / client one-shot auth round trip の手動確認手順を追加する
- [x] server / client one-shot auth round trip の accepted path 成功を確認する
- [x] client whitelist 読み込みを実装する
- [x] `shared_token_env` token reference placeholder を追加する
- [x] inline token debug redaction を追加する
- [x] secret resolution status placeholder を追加する
- [x] 認証済み送信元の登録 / 管理境界を設計する
- [x] accepted auth decision から registry registration への handoff を追加する
- [x] 未認証 / endpoint mismatch packet の破棄境界を設計する
- [x] registry 参照による packet 受理 / 拒否判定 helper を追加する
- [x] secret resolver 本実装範囲を確定する
- [x] `ServerSecretResolverBoundary` / secret resolution plan placeholder を追加する
- [x] `shared_token_env` の環境変数読み取りを `ServerSecretResolverBoundary` に追加する
- [x] secret 解決後の token material を auth decision input へ接続する
- [x] `shared_token_env` を使う one-shot auth round trip 手順を整理する
- [x] accepted auth path で in-memory registry 登録実処理を接続する
- [x] secret store 連携や token hashing / rotation 方針を設計する
- [x] future secret store 参照と token rotation policy placeholder を追加する
- [ ] 認証済み送信元の timeout / 失効 / 再認証を実装する
- [ ] 未認証送信元の `VideoFrame` 破棄を実装する
- [ ] `protocol_version` 不一致時の接続拒否を server 側に実装する
- [ ] `app_version` 差異時の warn ログを実装する
- [ ] 認証期限切れ / 再認証方針を実装する
- [ ] ログに secret を残さない処理を実装する

---

## heartbeat / 時刻同期
- [x] `Heartbeat` / `HeartbeatAck` 型を定義する
- [x] `Heartbeat` payload decode を実装する
- [x] `Heartbeat` encode 本実装を行う
- [x] `HeartbeatAck` payload decode を実装する
- [x] timestamp 単位をマイクロ秒に整理する
- [x] `HeartbeatAck` payload layout / encode 方針を決める
- [x] `HeartbeatAck` encode 本実装を行う
- [x] heartbeat state / RTT / offset 推定の入力境界を整理する
- [x] heartbeat state / RTT / offset 推定の本計算方針を整理する
- [x] heartbeat RTT / offset の小さな実計算単位を決める
- [x] heartbeat client ack observation flow を設計する
- [x] heartbeat observation carrier を設計する
- [x] `ClientStats` payload encode/decode 方針を決める
- [x] `ClientStats` heartbeat observation optional block の wire 変換を実装する
- [x] `ClientStats` optional heartbeat observation を server handler bridge から timebase 入力形へ変換する
- [x] `HeartbeatAckObservation` を client 側 `ClientStats` carrier に載せて 1 回送信する
- [x] `ClientStats` から返った observation を既存 timebase plan / stateless calculator へ渡す
- [x] RTT / offset estimate を server 側 `ServerHeartbeatRttOffsetState` へ 1 回 commit する
- [x] RTT / offset candidate の same-run delta threshold policy 境界を追加する
- [x] RTT / offset candidate policy を commit 前に接続し、rejected candidate を state commit しない
- [x] accepted auth 後の heartbeat one-shot 送信処理を client 側に実装する
- [x] registered heartbeat 受信から `HeartbeatAck` one-shot send までを server 側に接続する
- [x] registered heartbeat から `ServerHeartbeatLivenessState` へ 1 回 commit する最小境界を追加する
- [x] heartbeat timeout policy evaluation の最小境界を追加する
- [x] timeout evaluation 結果から auth invalidation / timeout log / timeout notice の action plan を作る最小境界を追加する
- [x] timeout action plan から registry invalidation / timeout log / notice handoff を適用する最小境界を追加する
- [x] timeout evaluation / action plan / apply を 1 client 分だけ呼ぶ loop tick 境界を追加する
- [x] heartbeat timeout notice queue storage / send wakeup 方針を整理する
- [x] continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する
- [x] client 側 `ClientHeartbeatLoopPolicyBoundary` を追加する
- [x] server 側 `ServerHeartbeatContinuousLoopPolicyBoundary` を追加する
- [x] continuous heartbeat loop 本体へ進む前の state ownership / socket receive timeout / retry 範囲を整理する
- [x] client 側 `ClientHeartbeatLoopOwnershipBoundary` / ack receive timeout / retry placeholder を追加する
- [x] server 側 `ServerHeartbeatContinuousLoopOwnershipBoundary` / socket receive timeout / retry placeholder を追加する
- [x] continuous heartbeat loop 本体へ進む前の 1 iteration body 接続範囲を整理する
- [x] client 側 `ClientHeartbeatLoopBodyBoundary` / send handoff を追加する
- [x] server 側 `ServerHeartbeatContinuousLoopBodyBoundary` / timeout tick・metrics handoff を追加する
- [x] continuous heartbeat loop 本体へ進む前の client heartbeat encode/send handoff 接続範囲を整理する
- [x] client 側 `ClientHeartbeatLoopEncodeSendBoundary` を追加する
- [x] continuous heartbeat loop 本体へ進む前の client ack receive / observation return 接続範囲を整理する
- [x] client 側 `ClientHeartbeatLoopAckObservationReturnBoundary` を追加する
- [x] continuous heartbeat loop 本体へ進む前の client stats return send handoff 接続範囲を整理する
- [x] client 側 `ClientHeartbeatLoopClientStatsReturnSendBoundary` を追加する
- [x] continuous heartbeat loop 本体へ進む前の client loop iteration result / counters 接続範囲を整理する
- [x] client 側 `ClientHeartbeatLoopCountersBoundary` / counters state を追加する
- [x] continuous heartbeat loop 本体へ進む前の client loop controller / retry execution / sleep integration 接続範囲を整理する
- [x] client 側 `ClientHeartbeatLoopControllerBoundary` / retry apply result / sleep decision を追加する
- [x] continuous heartbeat loop 本体へ進む前の client loop logging / shutdown integration 接続範囲を整理する
- [x] client 側 `ClientHeartbeatLoopControllerResultBoundary` / log handoff / shutdown decision を追加する
- [x] client 側 continuous heartbeat loop 本体の最小実装範囲を整理する
- [x] client 側 `ClientHeartbeatLoopOneTickRuntimeBoundary` を追加する
- [x] completed continuous heartbeat loop body の thin composition 実装を追加する
- [x] heartbeat timeout notice wakeup planning 境界を追加する
- [x] heartbeat timeout notice wakeup execution 境界を追加する
- [x] heartbeat timeout notice wakeup actual side-effect 境界を追加する
- [x] outer while-loop connection 境界を追加する
- [x] outer while-loop one-turn execution body 境界を追加する
- [x] outer while-loop actual timer wait / retry execution / reconnect 実行境界を追加する
- [x] client 側 continuous heartbeat loop の outer while-loop 反復本体を実装する
- [x] outer while-loop 反復本体から actual timer wait / retry execution / reconnect 実行境界を呼ぶ
- [x] outer while-loop reconnect policy 境界を追加する
- [x] outer while-loop actual socket 再確立 boundary / caller-owned hook を追加する
- [x] caller-owned socket 再確立 hook を実 UDP socket 差し替えへ接続する
- [x] future client continuous heartbeat loop runner に caller-owned UDP socket slot の live ownership を接続する
- [x] server 側 heartbeat timeout loop tick を複数 client に対して継続実行する loop 本体を実装する
- [x] RTT 計測 candidate を server 側 state に commit する
- [x] clock offset 推定 candidate を server 側 state に commit する
- [x] RTT / offset rejected candidate log / metrics 方針を整理する
- [x] RTT / offset rejected candidate metrics storage / export 方針を整理する
- [x] RTT / offset metrics snapshot の future loop / dashboard 連携方針を整理する
- [x] RTT / offset metrics state commit を継続 loop へ接続する
- [x] RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する
- [x] offset 平滑化を実装する
- [x] 補正後 timestamp へ変換する処理を実装する
- [x] targetTime 計算へ接続する
- [ ] 同期精度をログに出す

---

## video frame / 映像受信
- [x] `VideoFrame` の最小構造を定義する
- [x] H.264 payload を `Vec<u8>` として保持する方針を定義する
- [x] `VideoFrame` payload decode を実装する
- [x] `payload_size` と実際の H.264 byte 数の整合確認を実装する
- [x] 不正 bool / reserved / codec / payload 長の最小 error を実装する
- [x] `VideoFrame` encode 方針と最小実装範囲を整理する
- [x] `VideoFrame` encode を実装する
- [x] client 側で frame metadata を付与する
- [ ] client 側で H.264 encode を行う
- [x] client 側で placeholder encoded H.264 payload source を追加する
- [x] UDP で frame を送信する
- [x] server 側で認証済み client の frame だけ受理する
- [x] server 側で client ごとの受信キューを作る
- [ ] 不正 frame 破棄を実装する
- [ ] 受信遅延と drop を計測する
- [ ] sync-core のジッターバッファへ投入する
- [ ] frame 欠落時の代替表示方針を決める

---

## client 側
- [x] auth one-shot / heartbeat one-shot / stats one-shot / one-tick runtime までの client 起動経路を追加する
- [x] accepted path 用の one-shot client example config と手動確認手順を追加する
- [ ] 画面キャプチャに成功する
- [ ] Minecraft ウィンドウの取得確認をする
- [x] frame id / captureTimestamp / sendTimestamp を付与する
- [ ] H.264 encode 処理を実装する
- [ ] ハードウェア encode 優先処理を実装する
- [ ] ソフトウェア encode fallback を実装する
- [ ] 720p / 30fps を初期値にする
- [ ] 1080p / 60fps を将来有効化できる構造にする
- [x] UDP 送信処理を実装する
- [x] placeholder `VideoFrame` one-shot CLI/config launcher を追加する
- [ ] `ClientStats` 送信処理を継続 heartbeat loop に接続する

---

## switcher / 表示 / OBS
- [x] OBS 連携方法を Window Capture に決定する
- [x] switcher は表示専用とする方針を決定する
- [x] 4 分割表示と単独表示の切り替えを MVP 対象にする
- [x] 1 視点の placeholder decode/display handoff を作る
- [ ] 1 視点の real H.264 復号に成功する
- [ ] 1 視点の real window 表示に成功する
- [ ] 2x2 の 4 分割レイアウトを作る
- [ ] 単独表示モードを作る
- [ ] クリック / ダブルクリック / ホットキー切り替えを実装する
- [ ] 現在メイン視点を強調表示する
- [ ] 切断 / 準備中 / 復号不能 / frame 不足表示を作る
- [ ] client ごとの接続状態 / RTT / offset / 実効遅延 / fps / drop 率を表示する
- [ ] buffer 状態表示を作る
- [ ] デバッグ表示 ON/OFF を作る
- [x] OBS 向けの最初の出力は proof window ではなく dedicated clean output window に分ける方針を決める
- [x] 4-view clean output window boundary を追加する
- [x] 4-view clean output window を manual/runtime から開ける thin entry point を追加する
- [x] dedicated clean output continuous/runtime path を追加する
- [x] `--four-view-clean-output-window-loop [all-renderable] [frames]` を追加する
- [ ] OBS で映像表示に成功する
- [ ] 720p / 30fps で表示確認する
- [ ] 長時間表示でも安定することを確認する
- [ ] 不要 UI 非表示モードを作る

---

## ログ / 計測
- [x] ログ方針を JSON Lines 形式に決定する
- [x] `run_id` / `client_id` で追跡可能にする方針を決定する
- [x] switcher UI 上のリアルタイム簡易メトリクス表示方針を決定する
- [x] auth success / failure の JSON Lines ログイベント仕様を整理する
- [x] receive rejection の JSON Lines ログイベント仕様を整理する
- [x] receive rejection JSON Lines の最小 stderr 出力を実装する
- [x] auth result JSON Lines writer boundary を追加する
- [x] auth / receive JSON Lines writer 接続範囲を整理する
- [x] auth / receive JSON Lines の file sink 設定方針を整理する
- [x] send error JSON Lines 出力範囲を整理する
- [x] receive loop の継続運用向けログ範囲を整理する
- [ ] ログイベント型を定義する
- [ ] JSON Lines 形式でログ出力する
- [ ] `run_id` / `client_id` を各ログに付与する
- [ ] 接続 / 切断 / 再接続ログを実装する
- [ ] 受信数 / drop / 同期誤差ログを実装する
- [ ] protocol error / malformed packet / auth failure ログを実装する
- [ ] receive loop / send error のログを実装する
- [x] send error / log event の分類方針を整理する
- [ ] `app_version` / `protocol_version` を接続時ログへ記録する
- [ ] server 全体メトリクス表示を作る
- [ ] 720p / 30fps と 1080p / 60fps の負荷測定項目を整理する

---

## PoC に必要な最小ライン
1. [x] `AuthResponse` encode と fixed header encode が動く
2. [x] UDP socket の receive / send が最小で動く
3. [x] client が `AuthRequest` を送り、server が `AuthResponse` を返せる
4. [x] client が `Heartbeat` を送り、server が RTT / offset 推定に使える時刻情報を返せる
5. [x] client が 1 視点の placeholder encoded H.264 payload 付き `VideoFrame` を送れる
6. [x] server が 1 視点の frame を受信し、破棄 / 受理を判定し、accepted frame を queue に保存できる
7. [x] switcher が 1 視点の latest queued frame を選択し、placeholder display handoff を作れる
8. [ ] 2 視点で targetTime による簡易同期表示を確認できる
9. [ ] 4 視点で 2x2 表示を確認できる
10. [x] OBS Window Capture で switcher 表示を取り込める

---

## 検証 / テスト
- [x] 過去作業で `cargo fmt --check` が通ることを確認した
- [x] 過去作業で `cargo check --workspace` が通ることを確認した
- [x] one-shot auth round trip 手動確認手順を追加する
- [x] accepted path 用 one-shot auth round trip 手動確認手順を追加する
- [x] accepted path one-shot auth round trip 成功結果を記録する
- [x] `AuthResponse` encode の単体テストを追加する
- [x] `AuthResponse` decode と client one-shot receive の単体テストを追加する
- [x] `Heartbeat` encode / `HeartbeatAck` decode と client auth-then-heartbeat one-shot の単体テストを追加する
- [x] client auth-then-heartbeat-stats one-shot と server observation return 接続の単体テストを追加する
- [x] `HeartbeatAck` encode の単体テストを追加する
- [x] `VideoFrame` encode の単体テストを追加する
- [x] heartbeat liveness state commit / timeout evaluation の単体テストを追加する
- [x] heartbeat timeout action plan / auth invalidation / timeout log event の単体テストを追加する
- [x] heartbeat timeout apply boundary の単体テストを追加する
- [x] heartbeat timeout one-client loop tick boundary の単体テストを追加する
- [x] heartbeat timeout notice queue storage / send wakeup boundary の単体テストを追加する
- [x] heartbeat RTT / offset state commit boundary の単体テストを追加する
- [x] heartbeat RTT / offset candidate policy boundary の単体テストを追加する
- [x] heartbeat RTT / offset policy commit boundary の単体テストを追加する
- [x] heartbeat RTT / offset rejected candidate log / metrics handoff boundary の単体テストを追加する
- [x] heartbeat RTT / offset rejected candidate metrics state / snapshot export boundary の単体テストを追加する
- [x] heartbeat RTT / offset metrics snapshot loop / dashboard handoff boundary の単体テストを追加する
- [x] continuous heartbeat loop preflight policy boundary の単体テストを追加する
- [x] continuous heartbeat loop ownership / socket receive timeout / retry boundary の単体テストを追加する
- [x] continuous heartbeat loop one-iteration body boundary の単体テストを追加する
- [x] client heartbeat loop encode/send handoff boundary の単体テストを追加する
- [x] client heartbeat loop ack receive / observation return boundary の単体テストを追加する
- [x] client heartbeat loop client stats return send boundary の単体テストを追加する
- [x] client heartbeat loop iteration result / counters boundary の単体テストを追加する
- [x] client heartbeat loop controller / retry apply / sleep decision boundary の単体テストを追加する
- [x] client heartbeat loop logging / shutdown integration boundary の単体テストを追加する
- [x] client heartbeat loop one-tick minimal runtime boundary の単体テストを追加する
- [x] client one-tick heartbeat runtime launcher / config の単体テストを追加する
- [x] client one-tick runtime launcher / repeated-loop ownership 境界の単体テストを追加する
- [x] client future repeated loop body 境界の単体テストを追加する
- [x] client outer repeated loop controller / shutdown apply 境界の単体テストを追加する
- [x] client future completed loop lifecycle 境界の単体テストを追加する
- [x] client timer / retry / cleanup sequencing 境界の単体テストを追加する
- [x] client future completed loop body 実行順序境界の単体テストを追加する
- [x] client completed-loop 相当 1 step runtime 境界の単体テストを追加する
- [x] client while-loop ownership / caller contract 境界の単体テストを追加する
- [x] client repeated invocation skeleton / stop flag refresh 境界の単体テストを追加する
- [x] client actual timer / retry / cleanup apply call order 境界の単体テストを追加する
- [x] client completed continuous heartbeat loop outer shell 境界の単体テストを追加する
- [x] client caller-facing shell runner 境界の単体テストを追加する
- [x] client eventual repeated invocation 境界の単体テストを追加する
- [x] client future actual while-loop 境界の単体テストを追加する
- [x] client cleanup responsibility 境界の単体テストを追加する
- [x] client cleanup ordering 境界の単体テストを追加する
- [x] client cleanup execution planning 境界の単体テストを追加する
- [x] client cleanup actual side-effect apply 境界の単体テストを追加する
- [x] client cleanup completed-loop stop path 境界の単体テストを追加する
- [x] client actual while-loop termination 境界の単体テストを追加する
- [x] client completed continuous heartbeat loop body integration 境界の単体テストを追加する
- [x] client timer / retry / reconnect integration 境界の単体テストを追加する
- [x] client actual timer / retry / reconnect execution integration 境界の単体テストを追加する
- [x] client completed continuous heartbeat loop body connection 境界の単体テストを追加する
- [x] client completed continuous heartbeat loop body 境界の単体テストを追加する
- [x] client heartbeat timeout notice wakeup planning 境界の単体テストを追加する
- [x] client heartbeat timeout notice wakeup execution 境界の単体テストを追加する
- [x] client heartbeat timeout notice wakeup actual side-effect 境界の単体テストを追加する
- [x] client outer while-loop connection 境界の単体テストを追加する
- [x] client outer while-loop one-turn execution body 境界の単体テストを追加する
- [x] client outer while-loop actual timer wait / retry execution / reconnect 実行境界の単体テストを追加する
- [x] client outer while-loop 反復実行本体の単体テストを追加する
- [x] client outer while-loop reconnect policy / actual socket 再確立 boundary の単体テストを追加する
- [x] client real UDP socket 再確立 hook の単体テストを追加する
- [ ] fixed header encode / decode roundtrip test を追加する
- [ ] protocol error の単体テストを拡充する
- [ ] net-core inbound / outbound 境界の単体テストを追加する
- [ ] server inbound route の単体テストを追加する
- [ ] 疑似 client を作る
- [ ] 人工遅延 / jitter / frame 欠損テストを作る
- [ ] 1 人 PoC を 30 分連続確認する
- [ ] 2 人同期表示を確認する
- [ ] 4 人同期表示を確認する
- [ ] Minecraft 実機で確認する

---

## 後回し項目
- [ ] 音声統合
- [ ] 自動スイッチング
- [ ] 発話検知による自動強調
- [ ] Minecraft イベント連動演出
- [ ] 録画保存 / アーカイブ管理
- [ ] リプレイ機能
- [ ] クリップ自動生成
- [ ] 5 人以上への一般化
- [ ] 視点数の動的増減対応
- [ ] 高度な権限管理
- [ ] 一般公開向けの完成品品質への仕上げ
- [ ] OBS の高度な自動制御
- [ ] OBS WebSocket 連携
- [ ] WebRTC / TCP / SRT / RIST への変更
- [ ] Electron 中心構成への変更
- [ ] 本格的な retry / fragmentation / encryption

---

## 優先順ロードマップ

### フェーズ1: 仕様固定と土台
- [x] 目的 / PoC / MVP / 非対象範囲定義
- [x] 技術スタック / 通信 / codec / OBS / 音声 / 認証 / ログ方針決定
- [x] Cargo workspace 初期化
- [x] protocol crate の基本型定義
- [x] wire format 初期設計
- [x] decode 境界と主要 inbound payload decode
- [x] net-core / server の境界 placeholder

### フェーズ2: protocol encode と UDP PoC 準備
- [x] `AuthResponse` encode
- [x] fixed header encode
- [x] `HeartbeatAck` encode 方針
- [x] `HeartbeatAck` encode 本実装
- [x] `VideoFrame` encode
- [x] client whitelist / token 検証の設定入力境界整理
- [x] UDP receive / send 最小実装
- [x] UDP socket を auth response PoC の起動処理へ最小接続
- [x] auth response PoC の起動設定接続
- [x] server auth decision 最小実装
- [x] auth decision から AuthResponse outbound queue handoff までの server step 接続
- [x] send error / log event 方針整理
- [x] outbound queue 最小実処理方針整理
- [ ] receive / send ログ最小実装

### フェーズ3: 1 人送信・受信・表示 PoC
- [x] client capture / encode boundary with explicit real-capture and H.264-encode deferred results
- [x] client Windows capture backend selection/probe boundary with explicit not-configured / unsupported / unavailable results
- [x] client Windows capture target discovery boundary with descriptor/config conversion and explicit not-configured / unsupported / runtime-unavailable results
- [x] client capture target discovery runtime hook boundary for future Windows API-backed enumeration
- [x] client capture session config preparation boundary from selected descriptor / target config
- [x] client capture session runtime creation boundary with caller-owned hook and explicit deferred / unavailable / failed results
- [x] first minimal Windows Graphics Capture session creation hook for ready session runtime without frame acquisition
- [x] first minimal Windows Graphics Capture one-frame acquisition boundary from ready session runtime
- [x] H.264 encoder hook boundary from `ClientRawCapturedVideoFrame` to `RealCaptureH264` encoded source
- [x] minimal FFmpeg CLI software H.264 encoder runtime hook
- [x] one-shot real encoded `VideoFrame` path from ready capture runtime to UDP send
- [x] manual CLI/doc path for one-shot real encoded `VideoFrame` send
- [x] same-socket auth then real encoded `VideoFrame` one-shot CLI/config launcher
- [x] bounded continuous real encoded `VideoFrame` sender with frame-arrived wait/no-frame accounting
- [x] detailed UDP send failure diagnostics for bounded real encoded sender
- [x] manual E2E checklist for bounded authenticated real encoded sender and live two-view switcher
- [x] manual fragmented real encoded 1-frame queue path with server receive buffer tuning
- [x] manual fragmented real encoded 2-frame queue path with server receive buffer tuning
- [x] server queued encoded frame inspect/dequeue boundary keyed by client/run
- [x] switcher single-client queue source boundary over server queue read boundary
- [x] switcher single-client targetTime source boundary over queue source
- [x] switcher single-client targetTime source queue-like validation tests
- [x] switcher queue-backed 2-view targetTime source scheduler boundary
- [x] switcher queue-backed 2-view targetTime source scheduler live-like validation tests
- [x] switcher queue-backed scheduler result -> 2-view decode/render input adapter
- [x] switcher scheduler adapter output -> existing 2-view decode/render boundary connection validation
- [x] switcher scheduler adapter -> decode/render live-like queue validation
- [x] switcher two-view display policy boundary
- [x] switcher display policy -> 2-view composition input adapter
- [x] switcher display-composition adapter -> composed canvas render connection validation
- [x] switcher fallible 2-view scheduler result -> decode/render-facing instruction adapter
- [x] switcher fallible adapter output -> display-policy-facing decode/render connection
- [x] switcher fallible decode/render connection output -> display policy / placeholder decision boundary
- [x] switcher fallible display policy output -> composition adapter / placeholder detail boundary
- [x] production H.264 encoder configuration / error logging policy design fixed in docs
- [x] client encoder profile config wiring
- [x] encoder failure / FFmpeg summary field extension
- [x] FFmpeg availability / version preflight visibility
- [x] `[video.encoder]` profile manual evidence
- [x] production H.264 stdout visibility MVP evidence
- [ ] hardware encoder integration
- [x] `VideoFrame` encode
- [x] `VideoFrame` UDP send with explicit placeholder encoded H.264 payload
- [x] placeholder `VideoFrame` one-shot CLI/config launcher
- [x] same-socket auth then placeholder `VideoFrame` one-shot CLI/config launcher
- [x] server frame receive / queue
- [x] switcher placeholder decode / single view display handoff
- [x] switcher real H.264 decode / single-frame BMP dump
- [x] switcher decoded frame one-shot window rendering boundary
- [x] switcher single-client bounded continuous decode/render loop boundary
- [x] targetTime / jitter-buffer frame selection
- [x] 2-view targetTime selection orchestration
- [x] targetTime-selected frame -> decode/render connection
- [x] 2-view sync PoC runtime/manual verification
- [ ] 30 分連続確認

### フェーズ4: 2 人 / 4 人同期 PoC
- [x] RTT / offset 観測 return と最小 state commit
- [x] RTT / offset 平滑化と targetTime 接続
- [x] late frame queue mutation / drop policy の最小 slice
- [ ] ジッターバッファ
- [x] targetTime frame selection
- [x] 2-view targetTime-selected frame decode/render connection
- [x] 2-view layout/composition
- [x] composed 2-view canvas window render connection
- [x] live-like 2-client queue/runtime integration
- [x] bounded continuous 2-view scheduling
- [x] real UDP socket-backed source adapter for 2-view scheduling
- [x] live two-view switcher manual runtime with auth registry setup
- [ ] 2 人同期表示
- [ ] 4 人 2x2 表示
- [ ] OBS 取り込み確認

### フェーズ5: MVP 安定化
- [ ] switcher UI
- [ ] 認証 / reconnect / timeout
- [ ] 異常系対応
- [ ] ログ可視化
- [ ] 長時間試験
- [ ] 運用手順整備
---

## Current Focus
- client continuous heartbeat loop is complete through repeated body execution, caller-owned socket re-establishment hook injection, and a minimal runner that owns the live UDP socket slot.
- RTT / offset metrics state commit now has a minimal client loop boundary based only on explicit heartbeat ack observation / ClientStats observation / one-tick runtime result state.
- metrics snapshot export cadence now has a minimal client loop boundary based only on caller-owned metrics state, caller-owned cadence state, current time, and configured export interval.
- dashboard refresh consumer policy now has a minimal client loop boundary based only on explicit future dashboard refresh handoff / snapshot export output.
- the loop runner owns only socket-slot wiring and repeated-body execution coordination; socket replacement still happens through the injected hook and not inside the repeated body.
- the loop runner can now evaluate metrics snapshot export cadence from caller-owned metrics/cadence state after repeated-body execution while keeping metrics commit and dashboard refresh separate.
- the loop runner can now derive dashboard refresh policy input from snapshot cadence output and invoke a caller-owned dashboard refresh sink without rendering UI.
- server heartbeat timeout now has a thin multi-client loop boundary over the existing one-client timeout tick, with caller-owned registry / liveness state / queue / writer kept explicit.
- server video path now has a receive-side runtime wiring slice: accepted `VideoFrame` side effects can be stored in a caller-owned per-client encoded-frame queue, while rejected frames remain not queued.
- server video path now has a queue-owning manual auth-then-video launcher: `--receive-auth-video-queue-once [config-path]` receives `AuthRequest`, sends `AuthResponse`, keeps the authenticated sender registry alive, receives the next packet through the packet acceptance gate, and queues an accepted `VideoFrame` into caller-owned `ServerVideoFrameQueueState`.
- client video path now has a first send-side PoC slice: metadata construction, explicit placeholder encoded H.264 payload source, existing protocol encode, and one caller-owned UDP `send_to`.
- client video path now has a one-shot CLI/config launcher: `--placeholder-video-frame-poc-once [config-path]` sends one explicit placeholder `VideoFrame` and prints a compact stdout summary.
- client video path now has a same-socket manual E2E sender launcher: `--auth-placeholder-video-frame-poc-once [config-path]` sends `AuthRequest`, requires accepted `AuthResponse`, then sends one placeholder `VideoFrame` from the same UDP source.
- switcher video path now has a first placeholder slice: one client's latest queued encoded frame can be selected read-only and converted into an explicit decode-deferred display handoff.
- switcher video path now has a manual placeholder verification helper and fixture CLI path over caller-owned `ServerVideoFrameQueueState`; it verifies queue-to-switcher placeholder handoff without pretending to share a server process's in-memory queue.
- manual placeholder VideoFrame PoC status is now documented in `docs/operations/manual-placeholder-video-poc.md`: the client same-socket auth-then-video sender and server queue-owning auth-then-video receiver can be run as a two-command manual client-to-server queue PoC, and the switcher fixture helper can verify the queue-to-placeholder handoff separately.
- server-to-switcher placeholder bridge decision is now explicit: the next bridge should be a switcher-owned in-process integration launcher that calls the server queue launcher/boundary and then passes the returned caller-owned queue state to the existing switcher placeholder helper; file/socket/shared-memory queue sharing remains deferred.
- switcher now has the in-process manual bridge launcher `--receive-auth-video-placeholder-bridge-once [config-path] [client-id]`, which runs the server auth-then-video queue path in-process and then verifies the returned caller-owned queue state through the switcher placeholder bridge boundary.
- switcher now has a first real decode/display-substitute PoC: `SwitcherH264DecodeBoundary` can decode one Annex B H.264 payload with FFmpeg into BGRA, `SwitcherDecodeLatestFrameOnceBoundary` can select one latest queued frame and dump a decoded BMP, and `--receive-auth-video-decode-latest-once [config-path] [client-id] [output-path]` connects the in-process server queue result to that one-frame dump path.
- switcher now has a first real one-shot window rendering boundary: `SwitcherWindowRenderBoundary` validates `SwitcherDecodedFrame` BGRA input, the Windows GDI runtime can paint one frame in a normal window for a bounded hold duration, and `--receive-auth-video-render-decoded-once [config-path] [client-id] [hold-ms]` connects server queue -> decode -> one-shot render while leaving BMP dump intact.
- switcher now has a bounded single-client continuous render loop boundary: `SwitcherContinuousRenderLoopBoundary` repeatedly selects latest encoded frames from a caller-owned source, decodes through a caller-owned decode hook, renders through a caller-owned render hook, records no-frame/decode/render states explicitly, and stops by `max_iterations` or `max_rendered_frames`.
- switcher now has a deterministic targetTime / jitter-buffer selection boundary: `SwitcherTargetTimeBoundary` calculates targetTime from current switcher time, playout delay, and optional clock offset, while `SwitcherJitterBufferSelectionBoundary` reads one client's caller-owned queue and returns selected/no-frame/waiting/too-early/too-late states without decode/render.
- switcher now has a deterministic 2-view targetTime selection orchestration boundary: `SwitcherTwoViewTargetTimeSelectionBoundary` calculates one shared targetTime, applies per-client offset estimates independently during per-client jitter-buffer selection, and returns both-selected / partial / both-unavailable states without queue mutation, decode, render, 4-view layout, or OBS integration.
- switcher now has a 2-view targetTime-selected decode/render connection boundary: `SwitcherTwoViewDecodeRenderBoundary` consumes `SwitcherTwoViewTargetTimeSelectionResult`, decodes only selected encoded frames, renders decoded frames through caller-owned hooks, and returns both-rendered / one-rendered-one-skipped / both-skipped with per-side selection/decode/render reasons.
- switcher now has a 2-view sync fixture/manual verification path: `SwitcherTwoViewManualVerificationBoundary` runs targetTime selection -> decode/render over caller-owned queue state, and CLI `--two-view-sync-fixture-once [left-client-id] [right-client-id] [hold-ms]` prints targetTime and per-side selection/decode/render status without live networking, queue mutation, 4-view layout, or OBS work.
- switcher now has a pure 2-view layout/composition boundary: `SwitcherTwoViewCompositionBoundary` composes decoded BGRA left/right inputs into one side-by-side BGRA canvas, preserves per-side selected metadata when available, and keeps left-only / right-only / empty placeholder / invalid-dimensions states explicit without selecting, decoding, rendering, queue mutation, 4-view, or OBS work.
- switcher now has a composed 2-view canvas render boundary: `SwitcherTwoViewComposedCanvasRenderBoundary` validates `SwitcherTwoViewComposedFrame` and reuses the existing window render runtime hook to draw one composed canvas in a normal switcher window. CLI `--render-two-view-composed-fixture-once [hold-ms]` composes decoded fixture frames and renders once without live networking, 4-view, or OBS API work.
- switcher now has a bounded live-like 2-client queue/runtime integration boundary: `SwitcherLiveTwoViewRuntimeBoundary` consumes a caller-owned live queue source, stores accepted frames into `ServerVideoFrameQueueState`, then runs targetTime selection -> H.264 decode -> 2-view composition -> composed-canvas render once. Rejected frames are not queued, guard stops are explicit, and queue mutation for late drops remains deferred.
- switcher now has a bounded continuous 2-view scheduling boundary: `SwitcherContinuousTwoViewSchedulingBoundary` repeats the existing live-like one-pass runtime by logical tick, advances caller-owned switcher time cadence, records rendered-both / partial / no-frame / decode-failed / render-not-completed outcomes, and stops by max ticks, max rendered frames, or source end without owning sockets, late-drop mutation, 4-view, or OBS work.
- switcher now has a real UDP socket-backed source adapter: `SwitcherUdpLiveTwoViewQueueSource` binds or accepts a caller-owned UDP socket, receives bounded packets with timeout behavior, reuses the server receive loop and packet acceptance gate, maps accepted authenticated `VideoFrame` packets to `SwitcherLiveTwoViewQueueSourceItem`, and keeps unauthenticated/rejected packets, protocol decode failures, receive failures, non-video packets, timeout, and source end explicit. The adapter requires a caller-owned `AuthenticatedSenderRegistry`; it does not create fake authenticated frames.
- switcher now has a bounded live two-view manual runtime: `SwitcherLiveTwoViewManualRuntimeBoundary` binds or accepts one UDP socket, runs the existing server auth response step for bounded auth setup, owns the resulting caller-owned `AuthenticatedSenderRegistry`, passes it to `SwitcherUdpLiveTwoViewQueueSource`, and runs the existing continuous two-view scheduler. CLI `--live-two-view-switcher-once [config-path] [left-client-id] [right-client-id]` prints auth, packet, queue, tick, render, and stop summaries without adding 4-view, OBS API integration, or late-frame queue mutation. This direct receive path is diagnostic / legacy for complete `VideoFrame` packets and is not the main fragmented real encoded validation path.
- client video path now has an explicit real-capture / H.264-encode replacement boundary: capture returns `RealCaptureDeferred`, encode returns `RealH264EncodeDeferred`, and `ClientEncodedVideoFrameSource` can feed existing `VideoFrame` metadata/send wiring without pretending placeholder bytes are real capture output.
- client capture backend direction is now Windows Graphics Capture for MVP; the client can select/probe that backend and surface not-configured, unsupported, or unavailable results without producing fake pixels or coupling capture to UDP send.
- client capture target discovery now has a pre-session boundary: display/window target descriptors can be represented and converted to `ClientCaptureTargetConfig`, while real Windows enumeration remains deferred and explicit as runtime unavailable.
- client capture target discovery now has an injectable runtime hook, so future real Windows API enumeration can provide descriptors without changing discovery result types or touching frame acquisition.
- client capture session preparation now converts a selected display/window descriptor or target config into metadata-only `ClientCaptureSessionConfig` for future Windows Graphics Capture session creation without opening a session or acquiring frames.
- client capture session runtime creation now consumes `ClientCaptureSessionConfig` through `ClientCaptureSessionRuntimeInput` and a caller-owned runtime hook. The default placeholder-safe hook still reports unavailable/unsupported, while the Windows-only `ClientWindowsGraphicsCaptureSessionRuntimeHook` creates a ready Windows Graphics Capture item/frame-pool/session.
- client Windows Graphics Capture frame acquisition now has a separate one-frame boundary: `ClientCaptureFrameAcquisitionBoundary` consumes a ready `ClientCaptureSessionRuntime`, can explicitly start capture when requested, attempts one `TryGetNextFrame`, and returns a raw BGRA frame / no-frame / not-started / unavailable / failed result without encoding or UDP send.
- client raw BGRA frames now have a separate H.264 encoder hook boundary: `ClientH264EncoderInput::from_raw_frame` carries `ClientRawCapturedVideoFrame`, `ClientH264EncoderRuntimeHook` can provide real H.264 payload bytes, and the boundary produces `RealCaptureH264` only from non-empty hook output. The default hook remains explicit encode-deferred.
- client H.264 encoding now has a first real software runtime hook: `ClientFfmpegSoftwareH264EncoderRuntimeHook` invokes `ffmpeg` / `libx264` for one BGRA frame and returns an Annex B H.264 elementary stream, while missing FFmpeg and encode failures remain explicit.
- client real encoded video now has a one-shot send boundary: `ClientRealEncodedVideoFrameOneShotBoundary` composes a ready capture session runtime, one BGRA acquisition, H.264 encode, `RealCaptureH264` metadata construction, and one existing UDP `VideoFrame` send while preserving explicit capture/no-frame/encode/send failure states.
- client real encoded video now has manual verification wiring: `--real-encoded-video-frame-poc-once [config-path]` attempts a primary-display WGC frame, FFmpeg H.264 encode, and one `RealCaptureH264` `VideoFrame` UDP send, with explicit not-sent output for session/capture/encode/send failures.
- client real encoded video now has authenticated same-source manual E2E wiring: `--auth-real-encoded-video-frame-poc-once [config-path]` sends `AuthRequest`, requires accepted `AuthResponse`, then creates/captures/encodes/sends one `RealCaptureH264` `VideoFrame` from the same UDP source for server queue verification.
- client real encoded video now has bounded multi-frame manual sender wiring: `--auth-real-encoded-video-frame-poc-bounded [config-path] [max-frames] [fragment-pacing-every] [fragment-pacing-delay-ms]` sends `AuthRequest`, requires accepted `AuthResponse`, creates one capture session, repeatedly runs the existing one-shot capture/encode/send boundary on the same UDP socket, and reports configured guard values, runtime tick / capture-attempt counters, captured/encoded/sent counters, no-frame/failure counters, elapsed timing, pacing timing, and stop reason.
- bounded real encoded sender diagnostics now preserve destination, local socket address, frame id, encoded payload length, encoded packet length, and send error details; oversized packets are surfaced as `PacketTooLarge`.
- client send path now has a sender-side UDP fragmentation slice: direct `VideoFrame` send remains for packets within a conservative safe datagram limit, while larger encoded payloads are split into `VideoFrameFragment` packets carrying frame metadata plus explicit chunk metadata.
- server-side `VideoFrameFragment` reassembly now accepts authenticated fragments into caller-owned state keyed by client / run / frame, rejects inconsistent metadata, ignores duplicates explicitly, reconstructs complete payloads in chunk order, queues completed frames through the existing server video frame queue storage, and exposes manual stdout diagnostics for fragments received / frames reassembled / frames queued / incomplete per-frame progress. The manual server queue launcher now has CLI-overridable max packet, receive timeout, expected frame, stop-after-expected policy, and UDP receive buffer request with requested/effective diagnostics.
- server-side queued encoded frame consumption now has a minimal in-process read boundary: callers can inspect oldest/latest or dequeue oldest by client/run without changing receive, reassembly, protocol, decode, sync, 4-view orchestration, or OBS behavior.
- switcher/sync-facing single-client queue source now wraps the server queue read boundary with explicit `PreviewOldest`, `PreviewLatest`, and `ConsumeOldest` modes scoped by client/run, returning switcher encoded-frame handoff data without targetTime, decode, rendering, 4-view, or OBS behavior.
- switcher single-client targetTime source now wraps the queue source with explicit `PreviewLatestIfAtOrBefore` and `ConsumeOldestAtOrBefore` modes, selects only frames whose capture timestamp is at or before the target timestamp, and returns waiting/no-frame without unexpected queue mutation.
- switcher single-client targetTime source validation now covers empty queue and live-like queue progression in addition to select/wait/no-mutation/dequeue cases.
- switcher now has a queue-backed 2-view targetTime source scheduler: `SwitcherTwoViewTargetTimeSourceSchedulerBoundary` calls the single-client targetTime source once per configured `client_id + run_id` using one shared target timestamp and explicit preview/consume mode, returning per-view selected/no-frame/waiting plus all-selected / partial-selected / waiting / no-frames aggregate status. Scheduler-level consume is all-or-nothing via `ConsumeOldestAtOrBeforeAllSelected`; it previews both oldest candidates first and mutates neither queue unless both views are selected.
- switcher now has a minimal adapter from queue-backed scheduler results to the existing 2-view decode/render input path: `SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary` maps selected frames to renderable selected-frame input and keeps no-frame / waiting skip reasons explicit without deciding display fallback policy.
- switcher now has a minimal in-process connection validation boundary: `SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary` runs scheduler result -> adapter -> existing `SwitcherTwoViewDecodeRenderBoundary`, keeping selected/no-frame/waiting explicit and avoiding fake render input for skipped views.
- switcher scheduler adapter -> decode/render connection now has live-like queue validation over multiple timestamps, covering both-selected render, waiting skip, no-frame skip, all-or-nothing consume, and non-mutating preview behavior.
- switcher now has a minimal 2-view display policy boundary: `SwitcherTwoViewDisplayPolicyBoundary` maps decode/render connection results to update, hold previous, stale previous, or no-display placeholder decisions while preserving skip reasons and avoiding fake frames.
- switcher now has a minimal display policy -> 2-view composition adapter: `SwitcherTwoViewDisplayCompositionAdapterBoundary` maps update and hold decisions to decoded composition inputs, maps stale / no-display placeholder decisions to skipped composition sides, and keeps skip reasons visible without creating fake frames.
- switcher now has a minimal display-composition adapter -> composed canvas render connection: `SwitcherTwoViewDisplayCompositionRenderConnectionBoundary` runs adapter output through the existing 2-view composition boundary and composed-canvas render boundary, keeps adapter output / composition result / render result visible, renders only when composition produces a real composed frame, and keeps stale / no-display placeholders explicit without fake decoded frames.
- switcher now has a minimal server-mediated 2-view validation boundary: `SwitcherServerMediatedTwoViewValidationBoundary` can run from `SwitcherQueuedFrameSource` and keeps the caller-owned `ServerVideoFrameQueueState` entry point as the current in-process adapter path. It runs queue-backed targetTime scheduler -> scheduler decode/render connection -> display policy -> display-composition adapter -> composed canvas render connection while keeping each stage visible. Focused tests cover both-selected render, waiting placeholder, no-frame placeholder, all-or-nothing consume, preview no-mutation behavior, and direct execution over the queued-frame source abstraction.
- switcher now has a production-facing queued-frame source interface: `SwitcherQueuedFrameSource` reads queued encoded frames by explicit `client_id + run_id + mode`, and `SwitcherInProcessServerQueueFrameSource` wraps the existing server queue read path without adding transport, protocol, H.264, OBS, 4-view, or switcher-side fragment reassembly behavior.
- switcher now has a minimal transport-neutral / fallible queued-frame handoff contract: `SwitcherQueuedFrameHandoff` returns selected frame, explicit no-frame, or explicit handoff error. `SwitcherInProcessQueuedFrameHandoff` wraps the current in-process source, validates empty client/run scope as `InvalidScope`, and preserves selected/no-frame queue behavior. Focused tests cover selected frame, no-frame, invalid scope, fake source error propagation, metadata preservation, preview no-mutation, and consume scoped mutation.
- switcher now has a minimal fallible handoff consumer boundary: `SwitcherQueuedFrameHandoffConsumerBoundary` maps `FrameRead` / `NoFrameAvailable` into the existing `SwitcherSingleClientQueueSourceResult` shape and preserves `HandoffError` as a separate result. Focused tests cover frame conversion, no-frame preservation, all handoff error variants remaining distinct from no-frame, metadata preservation, preview no-mutation, and scoped consume mutation.
- switcher now has a fallible single-client targetTime handoff source: `SwitcherSingleClientTargetTimeHandoffSourceBoundary` consumes handoff results, applies targetTime selection in switcher, preserves selected / no-frame / waiting / handoff-error as distinct outcomes, and previews before dequeue in consume mode. Focused tests cover eligible selection, waiting, no-frame, every handoff error variant staying explicit, metadata preservation, preview no-mutation, consume mutation only when selected, and consume waiting without mutation.
- switcher now has a fallible 2-view targetTime handoff scheduler: `SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary` uses the fallible single-client handoff targetTime source per view, preserves selected / no-frame / waiting / handoff-error per side, adds aggregate `HandoffError`, and keeps consume all-or-nothing by previewing both sides before dequeue. Focused tests cover both selected, selected+waiting, selected+no-frame, selected+handoff-error, both handoff errors, error not treated as no-frame/waiting, consume all-or-nothing, consume no-mutation on handoff error, and metadata preservation.
- switcher now has a fallible 2-view scheduler decode/render-facing adapter: `SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary` maps selected sides to renderable frame instructions, maps no-frame / waiting to explicit skip instructions, maps handoff/source failures to `SkipHandoffError`, and only produces the existing `SwitcherTwoViewDecodeRenderInput` when no source error would be hidden by that shape. Focused tests cover both selected, selected+waiting, selected+no-frame, selected+handoff-error, both handoff errors, error not treated as no-frame/waiting, no fake frames for error sides, and selected metadata preservation.
- switcher now has a fallible adapter output -> display-policy-facing decode/render connection: `SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionBoundary` decodes/renders only `RenderFrame` instructions, preserves no-frame / waiting / handoff-error as distinct skipped side results, keeps aggregate `HandoffError`, and avoids fake decode/render input for skipped or source-error sides. Focused tests cover both rendered, render+no-frame, render+waiting, render+source-error, both source errors, source-error not no-frame/waiting, and no fake decode/render calls for source-error skips.
- switcher now has a fallible display policy boundary: `SwitcherTwoViewHandoffDisplayPolicyBoundary` consumes fallible decode/render connection output, produces update / hold-previous / stale-previous / no-display decisions, preserves no-frame / waiting / handoff-error / decode-render skipped side detail, and keeps aggregate `HandoffError`. Focused tests cover both updates, render+no-frame hold, render+waiting hold, render+source-error hold, source-error placeholders without previous frames, both source errors, source-error not no-frame/waiting, stale previous on source error, and no fake update frames for source-error placeholders.
- switcher now has a fallible display policy -> composition adapter: `SwitcherTwoViewHandoffDisplayCompositionAdapterBoundary` maps update to updated frame input, hold to held previous frame input, stale to explicit stale placeholder, no-display to explicit no-display placeholder, and source-error no-display to explicit source-error placeholder while preserving source-error detail in adapter instructions. The existing `SwitcherTwoViewCompositionInput` still only carries decoded or generic skipped sides, so the adapter output remains the place where source-error placeholder detail stays visible. Focused tests cover both updates, update+held previous, source-error hold detail, source-error placeholder without previous, stale placeholder, no-display placeholder, source-error not no-frame/waiting, no fake frames for skipped/error sides, and aggregate `HandoffError` preservation.
- switcher now has a fallible display-composition adapter -> composed-canvas render connection: `SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary` consumes `SwitcherTwoViewHandoffDisplayCompositionAdapterOutput`, preserves aggregate `HandoffError`, keeps adapter output / composition result / render result visible, renders updated and held-previous real decoded sides through the existing composer and composed-canvas render boundary, keeps stale / no-display / source-error placeholders explicit, and does not create fake decoded frames for placeholder/error sides. Focused tests cover both updates, update+held previous, update+stale placeholder, update+no-display placeholder, update+source-error placeholder, both source-error placeholders with no render, aggregate error preservation, and source-error not being treated as a generic no-display placeholder in the adapter output.
- switcher now has a fallible server-mediated 2-view validation path on `SwitcherServerMediatedTwoViewValidationBoundary`: `run_fallible_with_runtimes` wraps caller-owned `ServerVideoFrameQueueState` in `SwitcherInProcessQueuedFrameHandoff`, and `run_fallible_from_handoff_with_runtimes` accepts any `SwitcherQueuedFrameHandoff`. The output keeps fallible scheduler, decode/render adapter, decode/render connection, display policy, display-composition adapter, and composed-canvas render connection stages visible. Focused tests cover both eligible queues rendering, waiting, no-frame, source-error placeholder, both handoff errors without fake frames/render, consume all-or-nothing, preview no-mutation, and aggregate `HandoffError` preservation.
- 2-client manual validation planning is now documented in `docs/operations/manual-real-encoded-video-poc.md`: `--live-two-view-switcher-once` is clarified as a direct receive diagnostic path that uses a server-style config and does not use `configs/examples/switcher.example.toml` or a separate `stream-sync-server` process. Because this path treats `VideoFrameFragment` packets as non-video, it is not suitable as the main fragmented real encoded validation path. The main path is now client -> server -> switcher, with the next slice focused on server-mediated queue read into switcher targetTime / display / composition / render.
- `docs/operations/manual-real-encoded-video-poc.md` is now the step-by-step human E2E checklist for the bounded authenticated real encoded sender, one-client server queue verification, and two-client live switcher verification, including prerequisites, commands, expected stdout counters, diagnosis, pass/fail criteria, and recorded successful fragmented 1-frame / 2-frame queue runs.
- manual fragmented real encoded queue verification is now recorded as successful for both `max_frames=1` and `max_frames=2` when using the recommended `8388608` byte server receive buffer request and client fragment pacing. The latest `max_frames=2` localhost run observed `fragments_sent=854/854`, `fragments_received=854`, `frames_reassembled=2`, `frames_queued=2`, `incomplete_reassembly_frames=0`, and `receive_timed_out=false`.
- topology decision: main real encoded validation should use client -> server -> switcher. Server owns auth, UDP receive, receive-buffer tuning, `VideoFrameFragment` reassembly, and queueing. Switcher owns queue read, shared targetTime scheduling, decode, display policy, composition, and render. The next slice should add the smallest server-mediated switcher source validation instead of duplicating fragment reassembly in switcher.
- production handoff planning: initial server->switcher direction is switcher-pull/read, not server-push. The first interface mirrors `ServerVideoFrameQueueReadBoundary`, crossing only queued encoded frame metadata/payload plus queue read status. Waiting / no-frame / stale / placeholder decisions remain switcher-side downstream of queue read. Local IPC, TCP, UDP, shared memory, and protocol wire-format changes remain out of scope.
- production/manual handoff hook planning: the next useful hook is now a transport-neutral, fallible handoff contract around `SwitcherQueuedFrameSource`, not a new manual command and not a local IPC/TCP prototype. Switcher should request one latest/oldest/dequeue read per `client_id + run_id`; queue snapshots remain diagnostic-only, and targetTime-aware selection stays in switcher. Normal no-frame results are distinct from source unavailable, timeout, invalid scope, unsupported mode, malformed response, and source shutdown errors.
- server->switcher transport-neutral handoff codec is now implemented in `crates/net-core`: request/response DTOs and an explicit length-prefixed binary codec cover `request_id` echo, `FrameRead`, `NoFrame`, mapped handoff errors, metadata/payload preservation, and malformed/truncated frame rejection without adding named-pipe IO or touching the existing UDP `VideoFrame` wire format.
- server->switcher transport-neutral handler/adapter slice is now implemented: `apps/server` has a single-request queue-read handoff handler over `ServerVideoFrameQueueReadBoundary`, and `apps/switcher` has a DTO request builder / response mapper that converts DTO responses back into the existing `SwitcherQueuedFrameHandoffResult` / `SwitcherQueuedFrameHandoffError` shape while preserving frame metadata, payload bytes, and codec metadata.
- server->switcher Windows named-pipe one-request / one-response runtime slice is now implemented: `apps/server` can create one pipe instance, read one framed request, run the queue-read handoff handler, and write one framed response; `apps/switcher` can build one request, connect, write, read one framed response, and map IO/decode failure into explicit handoff errors. Local Windows smoke tests are isolated with `#[ignore]`, while default handoff validation uses focused non-I/O mapping tests.
- switcher now has a thin named-pipe-backed `SwitcherQueuedFrameHandoff` wrapper with a minimal request-id policy: callers may supply an explicit request id per read, or the wrapper may consume a caller-owned monotonic `u64` counter. Focused fake-runtime tests cover request-id preservation/generation and result propagation for `FrameRead`, `NoFrameAvailable`, explicit handoff errors, and local runtime encode failures staying explicit instead of becoming `NoFrame`.
- named-pipe one-shot manual CLI is now implemented. `--receive-auth-video-queue-and-serve-handoff-once` reuses the queue-owning server launcher and then serves one named-pipe handoff request, while `--read-queued-frame-handoff-once` issues one explicit switcher pull/read over named pipe. A localhost one-shot handoff run is now recorded as successful when using the plain pipe name `streamsync-handoff-dev`; the same manual session observed `SourceUnavailable` when the full `\\.\pipe\streamsync-handoff-dev` path was passed directly to the CLI.
- latest matched suppression OFF/ON rerun is `S:\stream-sync\manual-logs\two-client-ab-rerun-20260522-103943`. OFF and ON used the same `C:\streamsync-target\stream-sync-rerun\debug\*.exe`; source client fps mismatch is not noisy enough to reject the comparison, so the A/B evidence is VALID寄り. This remains separate opt-in evidence, but the current threshold verdict now comes from the reverse-order lag A/B rerun
- OFF without suppression kept slot0 one-shot load high: `continuous_decode_output_throughput_fps=20.129`, competing one-shot `37` attempts / `5401ms`, continuous render use `0`, bounded lookup hits `0`, and `effective_render_fps_after_first_render=11.594`
- ON with suppression improved the same comparison slice: suppression count `255`, suppression reasons `continuous_not_ready:27|stale:228|future:0|unknown:0`, render safety `decode_deferred_placeholder:255|unknown:0`, `continuous_decode_output_throughput_fps=26.814`, competing one-shot `13` attempts / `942ms`, continuous render use `11`, bounded lookup hits `11`, and render FPS `17.401`
- one-shot double-load is now a strong throughput contributor candidate in the slot0 / two-real / opt-in continuous slice, but suppression is still isolation evidence rather than a default policy decision
- stale and not-ready pressure remain visible even in ON evidence: suppression reasons still contain stale `228` and continuous-not-ready `27`; suppression alone is not a complete render-consumption solution
- bounded lookup threshold / stale guard docs-first review now lives in `docs/operations/continuous-decoded-lookup-plan.md`. Default allowed lag stays fixed at `5`; first experiment flag `--continuous-decoder-bounded-lookup-allowed-lag-frames <N>` is now wired for the two-real slot0 opt-in continuous path, with requested-future rejection and unbounded latest-decoded fallback still held. The latest reverse-order lag A/B says lag8 is a small PARTIAL PASS, but default `8` promotion is HOLD
- first threshold human comparison is the reverse-order lag A/B evidence for slot0 / two-real / opt-in continuous only: `8` vs `5` is VALID, with lag8 improving bounded hit, output lag, throughput, and reader average latency, while lag5 keeps a tiny render-FPS edge and slightly fewer not-ready rejects. Compare bounded hit lag, stale/not-ready rejects, continuous render use, render FPS, placeholder churn, and suppression counters
- continuous output throughput analysis remains tracked in `docs/operations/continuous-output-throughput-plan.md`; the A/B evidence and opt-in suppression boundary remain tracked in `docs/operations/continuous-one-shot-double-load-plan.md`
- slot0 one-shot fallback isolation first code slice remains opt-in via `--continuous-decoder-slot0-suppress-one-shot-fallback`. Default behavior is unchanged; slot1 stays one-shot, and the first render-safety path remains existing decode-deferred placeholder rather than unbounded stale decoded output
- metrics commit, snapshot export cadence, dashboard refresh consumer policy, and dashboard refresh runtime wiring remain separate from timer wait, retry, reconnect, socket ownership, cleanup, UI rendering, video, switcher, and OBS.
- server notice queue storage remains separate from notice send wakeup execution.
- actual dashboard UI rendering remains unimplemented.

## Next Items
1. keep `continuous_decode_bounded_lookup_allowed_lag_frames=5` as the default guard and treat lag8 as a small PARTIAL PASS / adoption candidate, not a default promotion
2. latest availability rerun makes output backlog the main line: client/server/feed PASS, output throughput `21.269fps` vs source about `29fps`, pending correspondence `115`, reader avg `46.430ms`, stale `238` vs not-ready `22`
3. if code resumes, keep the next slice diagnostics-only or opt-in experiment only, slot0 / two-real / opt-in continuous enabled only, with no default threshold, suppression, feed max, FFmpeg scale/pixel-format, slot1, 4-client, or protocol changes
4. completed correspondence latency diagnostics は VALID。次は stdout/raw BGRA pipe throughput、FFmpeg scale path split、reader blocking phase diagnostics を比較する; do not adopt source-size raw output by default
5. Production Readiness FAIL を維持し、targetTime-aware lookup 実装、latest decoded fallback、unbounded stale fallback、slot1 continuous 化、4-client 化、request/response persistent decoder 復活、GPU decode、one-shot fallback 削除には広げない
