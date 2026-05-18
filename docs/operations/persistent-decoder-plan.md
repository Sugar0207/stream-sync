<!-- stream-sync/docs/operations/persistent-decoder-plan.md -->

# Persistent Decoder Minimal Plan

最終更新: 2026-05-18

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

## Continuous Stream Decoder Separation
- 2026-05-18 の docs-first slice で、continuous-stream decoder は current request/response persistent decoder とは別設計として `docs/operations/continuous-stream-decoder-plan.md` に切り出した
- request/response persistent decoder は `1 request -> 1 stdout response` を render loop が待つ形であり、過去 rerun では `persistent_decode_stdout_read_timeout` により runtime FAIL したため凍結候補を維持する
- continuous-stream decoder は render loop から FFmpeg stdout wait を外し、per-slot access unit input queue、stdout reader thread、decoded frame queue/cache、frame_id correspondence queue を持つ別候補として扱う
- continuous-stream decoder の first target は two-real preview loop 限定で、server / client / protocol / 4-client / GPU decode には広げない
- この分離により、persistent request/response path を revive せずに、one-shot decode の render-loop blocking を外す設計候補だけを次 step 以降へ渡せる

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

## Incremental Compose Rerun Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260517-183552` では、`--disable-persistent-decoder` を維持したまま incremental quad compose の runtime 効果を確認できた
- persistent decoder config-disabled は引き続き正常動作し、`persistent_decode_config_enabled=false`、`persistent_decode_attempt_count=0`、`persistent_decode_timeout_count=0`、`persistent_decode_skipped_by_config_count=30` を確認した
- compose-side は明確に改善した
  - `quad_view_compose_elapsed_ms=3899 -> 704`
  - `quad_view_full_compose_count=57 -> 1`
  - `quad_view_incremental_update_count=0 -> 24`
  - `avg_quad_view_compose_elapsed_ms=62.887 -> 14.080`
  - `effective_render_fps_after_first_render=7.760 -> 11.587`
- `quad_view_incremental_skip_reason_counts=previous_output_missing:1|profile_or_size_mismatch:0|all_slots_changed:0|unknown:0` なので、latest rerun では first render 相当の 1 回以外は incremental path が使えていた
- `gdi_paint_wait_elapsed_ms=15` のため、latest rerun でも GDI wait は主犯扱いしない
- ただし switcher はまだ `effective_render_fps=9.592` / `effective_render_fps_after_first_render=11.587` で 30fps target 未達なので、compose PASS 後の next dominant candidate は one-shot decode I/O に戻す
- current request/response persistent decoder はこの結果でも revive せず、凍結候補のまま維持する

## One-Shot Decode I/O Diagnostics Rerun Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260517-194136` では、`--disable-persistent-decoder` を維持したまま one-shot decode I/O diagnostics を回収できた
- persistent decoder config-disabled は引き続き正常動作し、`persistent_decode_config_enabled=false`、`persistent_decode_attempt_count=0`、`persistent_decode_timeout_count=0`、`persistent_decode_skipped_by_config_count=26` を確認した
- transport / compose / render side も current rerun では成立している
  - `effective_render_fps=10.521`
  - `effective_render_fps_after_first_render=13.201`
  - `quad_view_compose_elapsed_ms=578`
  - `quad_view_incremental_update_count=27`
  - `quad_view_full_compose_count=1`
  - `avg_quad_view_compose_elapsed_ms=15.211`
  - `gdi_paint_wait_elapsed_ms=8`
- latest one-shot decode diagnostics は次の shape だった
  - `one_shot_decode_elapsed_ms=2287`
  - `one_shot_decode_elapsed_ms_max=144`
  - `one_shot_decode_input_write_elapsed_ms=1051`
  - `one_shot_decode_input_write_elapsed_ms_max=94`
  - `one_shot_decode_output_read_elapsed_ms=952`
  - `one_shot_decode_output_read_elapsed_ms_max=118`
  - `one_shot_decode_output_read_exact_elapsed_ms=815`
  - `one_shot_decode_output_read_exact_elapsed_ms_max=112`
  - `one_shot_decode_extra_output_probe_elapsed_ms=124`
  - `decode_process_spawn_elapsed_ms=129`
  - `decode_process_wait_elapsed_ms=93`
  - `one_shot_decode_input_payload_bytes_min=56049`
  - `one_shot_decode_input_payload_bytes_max=122774`
  - `one_shot_decode_input_payload_bytes_avg=96744.115`
  - `decode_input_payload_bytes_total=2515347`
  - `decode_stdout_expected_bytes_total=95846400`
  - `one_shot_decode_expected_output_bytes_per_frame=3686400`
- current evidence では extra-output probe は主犯ではない。`one_shot_decode_extra_output_probe_elapsed_ms=124` に留まり、`stdin write` と `stdout raw BGRA read` のほうが支配的だった
- current evidence では `decode_process_spawn_elapsed_ms` / `decode_process_wait_elapsed_ms` も first-order culprit ではない
- current dominant candidates は以下に絞る
  - `stdin write`
  - `stdout raw BGRA read / read_exact volume`
- latest rerun は `one_shot_decode_keyframe_attempt_count=0`、`one_shot_decode_non_keyframe_attempt_count=26`、`one_shot_decode_keyframe_elapsed_ms=0`、`one_shot_decode_non_keyframe_elapsed_ms=2287` だったため、keyframe/non-keyframe split の優劣はまだ判断保留にする
- current request/response persistent decoder はこの rerun 後も revive せず、凍結候補のまま維持する
- current additive code slice は two-real preview loop 限定の scaled decode output に置いた。loop-local `TimedSwitcherH264DecodeRuntime` は decode input を `640x360` と `scaled_output_enabled=true` / `scaled_output_reason=two_real_slot_size` に override し、FFmpeg one-shot process は `scale=640:360:flags=neighbor` を通して raw BGRA stdout を返す
- compile fix として、`scaled_output_enabled=true` の decode input は request/response persistent decoder path を試行せず one-shot decode path に直行させる。persistent helper / spawn path は scaled-output 対応へ広げ直さず、凍結候補の persistent runtime は current slice でも revive しない
- この slice の狙いは `one_shot_decode_expected_output_bytes_per_frame` を `3686400 -> 921600` 相当に落とし、`decode_stdout_expected_bytes_total` を縮めることにある

## Scaled Decode Output PASS Rerun Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260517-223121` では、two-real preview loop 限定 scaled decode output は runtime PASS した
- persistent decoder config-disabled は引き続き正常動作し、`persistent_decode_config_enabled=false`、`persistent_decode_attempt_count=0`、`persistent_decode_timeout_count=0`、`persistent_decode_skipped_by_config_count=26` を確認した
- scaled one-shot decode output shape は狙いどおりだった
  - `one_shot_decode_output_width=640`
  - `one_shot_decode_output_height=360`
  - `one_shot_decode_output_pixel_format=Bgra8`
  - `one_shot_decode_scaled_output_enabled=true`
  - `one_shot_decode_scaled_output_reason=two_real_slot_size`
  - `one_shot_decode_expected_output_bytes_per_frame=921600`
- stdout raw BGRA read volume は baseline `manual-logs/two-client-render-rerun-20260517-194136` から大きく下がった
  - `decode_stdout_expected_bytes_total=95846400 -> 23961600`
  - `decode_output_bytes_total=23961600`
  - `one_shot_decode_output_read_elapsed_ms=952 -> 816`
  - `one_shot_decode_output_read_exact_elapsed_ms=815 -> 676`
- switcher FPS も改善し、`effective_render_fps_after_first_render=13.201 -> 16.579`、`effective_render_fps=12.942` を確認した
- ただし `one_shot_decode_elapsed_ms=2029` と `one_shot_decode_input_write_elapsed_ms=937` はまだ大きく、30fps target には遠い
- latest rerun では payload bytes も増えており、`one_shot_decode_input_payload_bytes_min=96690`、`max=230687`、`avg=195142.192` だった。したがって next dominant candidate は `stdin write` そのものだけに固定せず、FFmpeg stdin consumption wait と payload size impact も含めて整理する
- final diagnostics には `handoff_error_count=15` と `scheduler_status=HandoffError` が混ざったが、server/client transport、scaled decode output runtime PASS、persistent config-disabled toggle の evidence 自体は有効扱いにする
- request/response persistent decoder はこの rerun 後も revive せず、凍結候補のまま維持する

## One-Shot Stdin Write Analysis Update
- latest one-shot path を `apps/switcher/src/lib.rs` で再確認すると、current `decode_input_write_elapsed_ms` は `stdin.write_all(&input.encoded_payload)` だけを測っている
- したがって current `stdin write` elapsed は単純な parent-side memory copy ではなく、少なくとも次を含み得る
  - anonymous pipe の空き容量待ち
  - FFmpeg process が stdin を読み始めるまでの待ち
  - FFmpeg が decode / parser / filter graph 初期化を進めながら stdin を断続的に消費することによる backpressure
- 一方で current timer には以下は含まれていない
  - `stdin` handle drop / EOF close の時間
  - `stdin` write 完了後から stdout first byte が返るまでの待ち
- latest rerun `manual-logs/two-client-render-rerun-20260517-223121` では `one_shot_decode_input_payload_bytes_avg=195142.192` と baseline `manual-logs/two-client-render-rerun-20260517-194136` の `96744.115` より payload avg がほぼ倍化している。stdout raw BGRA read volume は `95846400 -> 23961600` まで下がった一方、`one_shot_decode_input_write_elapsed_ms=937` がまだ大きいので、payload size impact を next dominant candidate に含める
- current evidence だけでは `stdin write` の内訳を `OS pipe backpressure` と `FFmpeg 側 consumption wait` に分け切れない。current summary は write完了後から stdout first byte までの待ちを独立計測していないため、write中に詰まったのか、write後に decode/scale/output 準備で待ったのかを確定できない
- したがって next safer slice は挙動変更ではなく diagnostics 追加を優先する。request/response persistent decoder revive や continuous-stream decoder rewrite には戻らない

## Noisy Scaled-Pass Rerun Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260518-022141` では、two-real preview loop 限定 scaled decode output は runtime PASS 継続だった
- persistent decoder config-disabled も引き続き PASS で、`persistent_decode_config_enabled=false`、`persistent_decode_attempt_count=0`、`persistent_decode_timeout_count=0` を維持した
- scaled decode output failure shape は出ていない
  - `one_shot_decode_output_width=640`
  - `one_shot_decode_output_height=360`
  - `one_shot_decode_output_pixel_format=Bgra8`
  - `one_shot_decode_scaled_output_enabled=true`
  - `one_shot_decode_expected_output_bytes_per_frame=921600`
- ただし latest run は noisy で、previous scaled-pass rerun `manual-logs/two-client-render-rerun-20260517-223121` より悪化した
  - `effective_render_fps_after_first_render=16.579 -> 9.469`
  - `decode_attempt_count=26 -> 42`
  - `one_shot_decode_elapsed_ms=2029 -> 8809`
  - `one_shot_decode_input_write_elapsed_ms=937 -> 3132`
  - `one_shot_decode_output_read_elapsed_ms=816 -> 4655`
  - `one_shot_decode_output_read_exact_elapsed_ms=676 -> 3729`
- first-byte diagnostics も重かった
  - `one_shot_decode_stdin_write_to_stdout_first_byte_elapsed_ms=3781`
  - `one_shot_decode_stdout_first_byte_elapsed_ms=3687`
- payload bytes はむしろ previous scaled-pass rerun より小さかった
  - `one_shot_decode_input_payload_bytes_avg=195142.192 -> 91042.357`
  - したがって latest regression は payload size 増加だけでは説明しにくい
- compose/display side も同時に重かった
  - `quad_view_compose_elapsed_ms=1667`
  - `gdi_paint_wait_elapsed_ms=132`
  - `placeholder_visual_changed_count=61`
  - `scheduler_status=PartialSelected`
- current interpretation は「scaled output は効いたまま、decode attempt frequency と per-attempt first-byte/read wait が同時に悪化した noisy run」である

## Decode Attempt Frequency Code-Path Findings
- `apps/switcher/src/main.rs` の two-real preview loop decode runtime では、actual `decode_attempt_count` は decode cache miss 時にだけ増える
- preview-loop cache key は `width` / `height` / `source_identity(client_id, run_id, frame_id)` を使う
- `source_identity` がある current real-slot path では payload bytes そのものは cache key に入らない
- `apps/switcher/src/lib.rs` の unchanged-frame reuse は、slot が `UseUpdatedFrame` を維持しつつ decoded-slot render identity の `client_id/run_id/frame_id` が前回と一致した場合にだけ成立する
- したがって `decode_attempt_count=26 -> 42` の増加候補は以下に置く
  - real slot の `frame_id` churn 増加
  - `Selected` 継続より `NoFrameAvailable` / `HandoffError` / held-previous / placeholder 遷移が増え、unchanged-frame reuse が減った
  - partial selection や placeholder visual churn により stable `UseUpdatedFrame` が続きにくかった
- `selected_source_changed_count` は queue / retained_keyframe 切り替えなどの visual churn を読む補助にはなるが、source kind 自体は decode cache key ではない
- next candidate は「one decode をさらに速くする」前に、「なぜ actual decode 回数が増えたか」を slot/frame-identity diagnostics と一緒に読む方向へ移す
- latest diagnostics-only slice では、この比較を next rerun で直接読めるように `one_shot_decode_attempt_source_counts`、`one_shot_decode_attempt_slot_counts`、`one_shot_decode_attempt_reason_counts`、`decode_cache_miss_slot0_count`、`decode_cache_miss_slot1_count`、`skipped_decode_unchanged_slot0_count`、`skipped_decode_unchanged_slot1_count` を summary に追加した
- 同時に per-attempt variance 用に `one_shot_decode_first_byte_elapsed_ms_max`、`one_shot_decode_stdin_write_to_stdout_first_byte_elapsed_ms_max`、`one_shot_decode_first_byte_slow_count`、`one_shot_decode_output_read_slow_count` と fixed threshold `66ms` も追加し、attempt count 増加だけでは説明できない noisy outlier を見やすくした
- この slice は two-real preview loop の observability 追加だけに閉じており、one-shot FFmpeg args、decode routing、scaled output shape、persistent config-disabled toggle は変えていない
- 補足として、current two-real preview loop は `source_identity` 文字列を `selected` 固定で入れている。したがって new `queue` / `retained_keyframe` attempt source counts は cache key divergence を直接示すものではなく、「どの handoff source 由来の decode attempt が多かったか」を見る補助観測として扱う

## Balanced Scaled-Pass Rerun Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260518-080637` では、two-real preview loop 限定 scaled decode output は runtime PASS 継続だった
- persistent decoder config-disabled も引き続き PASS で、`persistent_decode_config_enabled=false`、`persistent_decode_attempt_count=0`、`persistent_decode_timeout_count=0` を維持した
- transport / queue / client send も成立している
  - server `packets_received=37374`
  - server `frames_queued=1800`
  - server `per_client_queued_frames=player1/streamsync-dev-session:900|player2/streamsync-dev-session:900`
  - client `frames_sent=900|900`
  - client `effective_output_fps=29.157|28.887`
- latest switcher rerun は previous scaled-pass rerun `manual-logs/two-client-render-rerun-20260517-223121` よりは遅いが、noisy rerun `manual-logs/two-client-render-rerun-20260518-022141` よりは改善した
  - `effective_render_fps_after_first_render=16.579 -> 13.760`
  - noisy rerun reference: `9.469`
- current interpretation では decode attempt frequency は今回の主犯ではない
  - `decode_attempt_count=28`
  - `one_shot_decode_attempt_slot_counts=slot0:14|slot1:14`
  - `one_shot_decode_attempt_reason_counts=frame_id_changed:26|cache_miss:0|previous_unavailable:0|source_recovered:2|unknown:0`
  - `decode_cache_miss_slot0_count=14`
  - `decode_cache_miss_slot1_count=14`
- heavier evidence は per-attempt variance 側に寄った
  - `one_shot_decode_input_write_elapsed_ms=1648`
  - `one_shot_decode_input_write_elapsed_ms_max=401`
  - `one_shot_decode_stdin_write_to_stdout_first_byte_elapsed_ms=1474`
  - `one_shot_decode_stdin_write_to_stdout_first_byte_elapsed_ms_max=186`
  - `one_shot_decode_stdout_first_byte_elapsed_ms=1472`
  - `one_shot_decode_first_byte_elapsed_ms_max=186`
  - `one_shot_decode_first_byte_slow_count=7`
  - `one_shot_decode_output_read_elapsed_ms=1775`
  - `one_shot_decode_output_read_elapsed_ms_max=221`
  - `one_shot_decode_output_read_slow_count=7`
- payload bytes は previous scaled-pass rerun より小さかった
  - `one_shot_decode_input_payload_bytes_avg=195142.192 -> 83475.286`
  - したがって latest rerun の variance は payload size 増加だけでは説明しにくい
- compose / GDI も secondary note として残す
  - `quad_view_incremental_update_count=46`
  - `quad_view_full_compose_count=1`
  - `quad_view_compose_elapsed_ms=969`
  - `gdi_paint_wait_elapsed_ms=80`
  - `placeholder_visual_changed_count=44`
  - `scheduler_status=PartialSelected`
- したがって next dominant candidate は persistent decoder revive ではなく、one-shot path の `first-byte wait variance` / `output-read slow variance` / `input-write outlier` へ移す

## Slow Correlation Rerun Update
- same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260518-111013` では、implemented slow correlation diagnostics を初めて runtime で読めた
- scaled decode output は引き続き PASS で、`one_shot_decode_output_width=640`、`one_shot_decode_output_height=360`、`one_shot_decode_scaled_output_enabled=true`、`one_shot_decode_expected_output_bytes_per_frame=921600` を維持した
- persistent decoder config-disabled も引き続き PASS で、`persistent_decode_config_enabled=false`、`persistent_decode_attempt_count=0` を維持した
- after-first FPS は改善した
  - `effective_render_fps_after_first_render=17.749`
  - previous correlation rerun `manual-logs/two-client-render-rerun-20260518-080637`: `13.760`
  - previous scaled-pass rerun `manual-logs/two-client-render-rerun-20260517-223121`: `16.579`
- current evidence では decode attempt frequency / slot bias / input-write outlier は主犯ではない
  - `decode_attempt_count=20`
  - `one_shot_decode_attempt_slot_counts=slot0:10|slot1:10`
  - `one_shot_decode_attempt_source_counts=queue:20|retained_keyframe:0|unknown:0`
  - `one_shot_decode_input_write_outlier_count=0`
- slow first-byte / slow output-read は low count に留まり、reason bias は `source_recovered` に寄った
  - `one_shot_decode_first_byte_slow_count=3`
  - `one_shot_decode_output_read_slow_count=3`
  - `one_shot_decode_slow_first_byte_reason_counts=frame_id_changed:1|cache_miss:0|previous_unavailable:0|source_recovered:2|unknown:0`
  - `one_shot_decode_slow_output_read_reason_counts=frame_id_changed:1|cache_miss:0|previous_unavailable:0|source_recovered:2|unknown:0`
- input-write outlier payload correlation は current rerun では読めなかった
  - `one_shot_decode_input_write_outlier_payload_bytes_avg=n/a`
- 一方で startup / availability 側はなお重い
  - `first_render_attempt_index=137`
  - `first_render_elapsed_ms=5025`
  - `no_render_before_first_render=136`
  - `no_frame_count=864`
  - `handoff_error_count=22`
  - server `no_frame_count=264`
- したがって next candidate は one-shot decode I/O broad regression ではなく、`source_recovered slow path` と `startup/no-frame availability` に寄せる

## Startup / Source-Recovered Code-Path Findings
- `apps/switcher/src/main.rs` の two-real preview loop では、`first_render_attempt_index` と `first_render_elapsed_ms` は clean output が `RenderReady` かつ inner render が `Rendered` になった最初の tick でのみ確定する
- `no_render_before_first_render` は独立 counter ではなく、first rendered attempt index から導出される。したがって startup 中に non-render tick が続いた本数をそのまま読む項目として扱える
- current startup chain は `scheduler -> decode/render adapter -> display policy -> quad composition -> render-facing -> clean output render`
- `apps/switcher/src/lib.rs` の four-view scheduler では、slot ごとに `Selected` / `NoFrameAvailable` / `WaitingForFrameAtOrBeforeTarget` / `HandoffError` を保持したまま downstream へ流す
- display policy では、`SkipNoFrameAvailable` と `SkipWaitingForFrameAtOrBeforeTarget` は previous displayed slot が無い間 `NoDisplayPlaceholder` になり、`SkipHandoffError` は `SourceErrorPlaceholder` になる
- composition/render-facing では renderable slot count が `0` の tick は `NoRenderableQuadView` のまま終わる。つまり server aggregate で `frames_queued=1800` が見えていても、switcher startup では target-time eligible な selected slot が renderable になるまで first render は始まらない
- latest rerun の `no_frame_count=864` と server `no_frame_count=264` は意味が違う。switcher 側は per-slot/per-tick 集計、server 側は handoff/queue read 層の no-frame 観測なので 1:1 比較ではなく availability trend として読む
- `source_recovered` は previous slot diagnostic で `selected_frame_available != true`、current slot diagnostic で `selected_frame_available == true` になったときに付く
- previous unavailable は `NoFrameAvailable` だけでなく `WaitingForFrameAtOrBeforeTarget` や `HandoffError` でも起こり得る。したがって latest rerun の `source_recovered:2` slow bias は「post-gap / post-error recovery decode」の候補として読むが、single-cause 断定には使わない
- `handoff_error_count` もこの transition に関与し得る。previous tick が `HandoffError` で selected frame unavailable、current tick で selected frame available に戻ると、その次の decode attempt reason は `source_recovered` になり得る
- current summary は slow bias の coarse comparison には十分だが、pre-first-render の non-render reason を aggregate では持っていない
- 追加するなら next safer slice は diagnostics-only に留め、最小候補を以下へ絞る
  - `first_render_wait_reason_counts`
  - `no_render_before_first_render_reason_counts`
  - `startup_no_frame_count`
  - `startup_handoff_error_count`
  - `startup_selected_but_not_rendered_count`
  - `source_recovered_after_no_frame_count`
  - `source_recovered_after_handoff_error_count`

## Safer Diagnostics Candidates
- high-value candidates
  - `one_shot_decode_stdin_close_elapsed_ms`
    - current timer外の EOF close cost を切り分けられる
  - `one_shot_decode_stdin_write_to_stdout_first_byte_elapsed_ms`
    - write完了後に FFmpeg 側でどれだけ待ってから first output byte が出るかを見られる
  - `one_shot_decode_stdout_first_byte_elapsed_ms`
    - spawn 以後の first output visibility を見られる
  - derived `one_shot_decode_write_throughput_bytes_per_ms`
    - current `input_payload_bytes` と `input_write_elapsed_ms` から挙動変更なしで算出できる
  - derived `one_shot_decode_input_write_elapsed_per_payload_kb`
    - payload size impact の比較に使える
- medium-value candidates
  - `one_shot_decode_read_throughput_bytes_per_ms`
    - scaled stdout read volume 改善後の比較用に残せる
  - `one_shot_decode_spawn_to_stdin_write_start_elapsed_ms`
    - parent-side gap が実質ゼロか確認できるが、child readiness の本丸切り分けにはなりにくい
- low-value or risky-first candidates
  - `one_shot_decode_stdin_flush_elapsed_ms`
    - pipe write path では有意な差分が出ない可能性が高く、優先度は低い
  - `one_shot_decode_payload_bytes_per_write_max`
    - syscall単位の write size を取るには `write_all` を manual loop 化する必要があり、first safer slice としては挙動変更が増える
- next implementation slice が必要になっても、まずは high-value candidates のうち derived fields と first-byte timing に寄せる
- latest comparison rerun を受けた docs-only minimal design として、slow-attempt correlation 追加を検討する場合は以下に限定する
  - `one_shot_decode_slow_first_byte_slot_counts`
  - `one_shot_decode_slow_first_byte_source_counts`
  - `one_shot_decode_slow_first_byte_reason_counts`
  - `one_shot_decode_slow_output_read_slot_counts`
  - `one_shot_decode_slow_output_read_source_counts`
  - `one_shot_decode_slow_output_read_reason_counts`
  - `one_shot_decode_input_write_outlier_count`
  - `one_shot_decode_input_write_outlier_threshold_ms`
  - `one_shot_decode_input_write_outlier_slot_counts`
  - `one_shot_decode_input_write_outlier_payload_bytes_avg`
- この候補も observability-only に留め、FFmpeg args、pixel format、one-shot decode routing、persistent decoder state machine には触れない
- latest implementation slice で上記 correlation fields は `apps/switcher/src/main.rs` に実装済みになった
- current implementation は actual decode miss ごとの per-attempt observation を existing source identity / frame identity に後結合しており、one-shot decode behavior 自体は変えていない
- したがって next step は新 field 追加ではなく、`S:\stream-sync` rerun で bias を読む比較に移す

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
- continuous-stream decoder rewrite は `docs/operations/continuous-stream-decoder-plan.md` に切り出した別設計候補として扱い、request/response persistent decoder の再挑戦とは分ける
- latest good-ish comparison baseline は `manual-logs/two-client-render-rerun-20260518-124418` とし、previous scaled/correlation reruns は必要に応じて `manual-logs/two-client-render-rerun-20260518-111013` / `manual-logs/two-client-render-rerun-20260517-223121` を参照する
- current one-shot fallback path の next comparison axis は decode attempt frequency そのものではなく、implemented slow correlation fields の `source_recovered` 偏りと startup/no-frame availability に寄せる
- next rerun で優先して見る summary は以下
  - `first_render_attempt_index`
  - `first_render_elapsed_ms`
  - `no_render_before_first_render`
  - `no_frame_count`
  - `handoff_error_count`
  - server `no_frame_count`
  - `one_shot_decode_attempt_reason_counts`
  - `one_shot_decode_slow_first_byte_reason_counts`
  - `one_shot_decode_slow_output_read_reason_counts`
- current one-shot code path では以下の境界を前提に safer slice を選ぶ
  - `decode_process_spawn_elapsed_ms`: `Command::spawn()` だけ
  - `decode_input_write_elapsed_ms`: `stdin.write_all(&input.encoded_payload)` だけ
  - `decode_output_read_exact_elapsed_ms`: `stdout.take(expected_len).read_to_end(...)` だけ
  - `decode_output_read_elapsed_ms`: bounded read + extra-output probe まで
  - `decode_process_wait_elapsed_ms`: `child.wait()` だけ
- current one-shot observability には payload-size min/max/avg、keyframe vs non-keyframe attempt/elapsed split、per-phase max elapsed、expected output bytes per frame、extra-output probe elapsed に加え、slow first-byte / slow output-read / input-write outlier correlation fields も追加済みで、next rerun では request/response persistent decoder を revive せずに bias を比較する
- next safer slice は extra-output probe や persistent decoder revive ではなく、`source_recovered` 時の slow path と startup/no-frame availability の関係を narrow に整理する docs-first / diagnostics-first follow-up に置く
- first additive implementation は observability-only に限定したまま維持し、two-real preview loop 以外へ広げない
- raw BGRA 以外の intermediate pixel format 比較は current latest rerun の主候補から外し、availability 側の切り分け後の次候補としてだけ残す
- stdin write は current evidence では pipe backpressure か FFmpeg側 consumption wait の可能性を疑うが、persistent request/response path には戻らず one-shot path 内だけで判断する
- compose / GDI variance も regression guard として残し、decode-side だけに原因を断定しない
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
