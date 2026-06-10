<!-- stream-sync/docs/operations/todo.md -->

# StreamSync TODO

最終更新: 2026-06-08

このファイルは、現在位置と次の作業だけを確認するための TODO です。
時系列の作業履歴は `docs/operations/session-log.md` を正とし、検証の詳細は各運用ドキュメントへ寄せます。

参照先:
- `docs/operations/session-log.md`
- `docs/operations/obs-capture-validation.md`
- `docs/operations/continuous-output-pipeline-experiment-plan.md`
- `docs/operations/continuous-output-lag-plan.md`
- `docs/operations/continuous-output-throughput-plan.md`
- `docs/operations/continuous-pixel-conversion-plan.md`
- `docs/operations/continuous-stream-decoder-plan.md`
- `docs/operations/continuous-decoded-lookup-plan.md`
- `docs/operations/distributed-pc-validation.md`

---

## 現在位置
- ProgramOutput は OBS target separation が正しくなり、`StreamSync Program Output` を Window Capture する前提は整った。
- 最新の `5/90 + --operator-preview-snapshot-retention` で、Program black / placeholder は出ず、 perceived stutter も小さかった。
- Snapshot retention により Preview の black / flicker は解消し、client1 / client2 も両方表示された。
- ただし Preview update frequency は operator monitoring 用としてまだ低すぎるため、現行の same-loop low-cost Preview refresh tuning は limited / paused。
- Current Preview は stable snapshot-only とみなし、final monitoring Preview とは分けて扱う。
- `StreamSync 4-view Output` は production operator monitoring 用 Preview として引き続き必要。OBS Program scene は `StreamSync Program Output` だけを capture し、4-view Preview は Program scene に入れない。
- ProgramOutput は near-MVP closeout ではない。FPS 以外の blocker が残っているため、ProgramOutput non-FPS blocker audit は継続中。
- `NoDecodedFrameForSelection` を含む first render / missing selected source の問題は、startup diagnostics と clients-before-switcher rerun で、selection / source identity ではなく selected source frame 到着から continuous first output までの待ちが主要観測点になった。
- clients-before-switcher 起動順では `program_first_source_frame_seen_elapsed_ms=246`、`program_first_continuous_output_elapsed_ms=1964`、`program_output_first_render_elapsed_ms=1964`、`program_output_missing_before_first_render_count=29`、after-first missing / black / placeholder は `0`。process start order delay は分離できたが、continuous first output まで約 1.6s 残る。
- ProgramOutput startup one-shot bootstrap は opt-in `--program-startup-bootstrap-one-shot` として実装済み。既定動作は変更せず、ProgramOutput 初回 render 前、明示 `--program-selected-client-id`、continuous latest / last-valid / selected decoded がまだない場合だけ候補化する。
- 以前の clients-before-switcher bootstrap A/B で出ていた `decode_failed:27` は、実 decode failure ではなく `ContinuousOneShotSuppressed` へ誤配線される pre-invoke routing bug だった。bootstrap decode purpose / suppression bypass 修正後、この wiring bug は fixed と扱う。
- 最新の clients-before-switcher bootstrap bypass validation は PASS。使用コマンドは `--enable-program-output-window --program-selected-client-id player2 --enable-program-continuous-decode --program-continuous-decode-mode smooth-latest --program-first-validation-mode --program-startup-bootstrap-one-shot`。
- PASS run では `program_startup_bootstrap_enabled=true`、`program_startup_bootstrap_attempt_count=1`、`program_startup_bootstrap_success_count=1`、`program_startup_bootstrap_actual_decode_invoked_count=1`、`program_startup_bootstrap_decode_skipped_before_invoke_count=0`、`program_startup_bootstrap_decode_error_counts=failed:0|deferred_empty_payload:0|deferred_invalid_dimensions:0|deferred_ffmpeg_unavailable:0|deferred_continuous_one_shot_suppressed:0|unknown:0` を確認した。
- 初回 Program render は bootstrap frame を使って `program_startup_bootstrap_used_for_first_render=true`、`program_output_first_render_elapsed_ms=354`、`program_output_missing_selected_source_count=0`、`program_output_missing_before_first_render_count=0`、after-first missing / black / placeholder も `0` だった。
- continuous decoder 自体の初回出力はまだ `program_first_continuous_output_elapsed_ms=1928` / `continuous_decode_first_input_to_first_output_elapsed_ms=1688` と遅いが、clients-before-switcher 条件では bootstrap がその待ちを first Program render から隠せた。
- switcher-first cold start bootstrap validation も PASS。`program_output_first_render_elapsed_ms=3803`、`program_output_missing_before_first_render_count=102`、`program_output_missing_after_first_render_count=0`、`program_first_source_frame_seen_elapsed_ms=3590`、`program_first_continuous_input_elapsed_ms=3803`、`program_first_renderable_decoded_frame_elapsed_ms=3803`、`program_startup_bootstrap_attempt_count=1`、`program_startup_bootstrap_success_count=1`、`program_startup_bootstrap_actual_decode_invoked_count=1`、`program_startup_bootstrap_used_for_first_render=true`、black / placeholder は `0`。
- switcher-first cold start の残り待ちは、主に selected client/player2 frame の到着待ち。ProgramOutput は selected-only のため selected source frame が存在する前には描画できず、bootstrap は source frame 到着後の decode / continuous startup latency だけを短縮する。
- ProgramOutput startup readiness diagnostics は最小実装済み。summary は `program_startup_readiness_state`、`program_selected_source_wait_elapsed_ms`、`program_startup_waiting_for_selected_source_count`、`program_startup_bootstrap_after_source_seen_elapsed_ms`、`program_startup_selected_source_seen_count` を出す。
- ProgramOutput startup readiness semantics は `program_selection_configured` -> `program_selected_source_waiting` -> `program_selected_source_seen` -> `program_first_frame_bootstrapping` -> `program_first_frame_rendered` -> `program_steady_state` として扱う。ProgramOutput 無効時の summary 値だけは `disabled`。
- selected source visual verification 用の validation-only client/source-side marker は実装済み。最新 smooth-latest lag diagnostics rerun では `P2` が Program で human-visible、`P1` は Program に出ていないため selected-source visual verification は `PASS`。
- ProgramOutput は near-MVP closeout ではない。non-FPS blocker が残るため closeout は引き続き blocked とし、same-loop Preview tuning も paused のままにする。
- 新 diagnostics は bootstrap decode の elapsed / error class / FFmpeg exit+stderr / payload bytes / NAL kinds / SPS/PPS/IDR / frame_id / slot/client / actual invoke vs pre-invoke skip を読む。
- source-side marker approach は維持する。ProgramOutput に overlay / watermark / Preview label を足さず、client/source 側の validation-only marker を改善して selected source identity を再確認する。
- smooth-latest lag criteria の stable reference 値は `program_selected_source_frame_lag=5`、`program_continuous_selected_frame_lag=0`、`continuous_decode_latest_selected_to_output_frame_gap=5`、`program_render_effective_fps=22.285`、black / placeholder `0`。
- 最新 smooth-latest lag diagnostics rerun は `S:\stream-sync\manual-logs\program-output-smooth-latest-lag-rerun-20260607-002942` として記録済み。Program selected source は `player2`、client markers は `P1` / `P2`、ProgramOutput enabled、smooth-latest enabled、Program-first validation mode、startup bootstrap one-shot。
- 最新 rerun の補正後分類は、OBS safety `PASS`、Program cleanliness `PASS`、selected-source visual verification `PASS`、lag criteria `Warning`、overall ProgramOutput criteria-based validation `WARNING`。この run は marker ambiguity 補正後の `FAIL` ではなく、lag による `WARNING` とする。
- visible `P2` marker は source-side validation marker であり、Program overlay、debug UI、Preview label、4-view UI ではない。したがって「Program に border/debug UI/Preview label が混ざっていた」という手動欄は Program cleanliness failure として扱わない。
- 最新 metrics は `program_selected_source_frame_lag=16`、`program_continuous_selected_frame_lag=16`、`continuous_decode_latest_selected_to_output_frame_gap=16`、`program_render_effective_fps=23.779`。smooth-latest details は selected frame `3089`、rendered frame `3073`、latest continuous frame `3073`、selected-minus-rendered `16`、selected-minus-latest-continuous `16`、rendered-minus-latest-continuous `0`、source mismatch `0`、stale reuse `41`、cache age / frame age `1ms`。
- smooth-latest lag 原因分類は、Program render selection issue `unlikely`、source mismatch `unlikely`、stale / last-valid reuse `not primary` だが `program_smooth_latest_stale_reuse_count=41` は watch 継続、continuous decoder / feed backlog `likely`。
- continuous backlog の根拠は、Program が latest available continuous frame を render している一方で、その latest continuous frame が selected source より 16 frames 遅れていること。加えて `continuous_decode_output_throughput_fps=20.906`、`continuous_decode_latest_input_minus_latest_output_lag=41`、`continuous_decode_pending_correspondence_count=41`、pending age avg `1004.488ms`、completed latency avg `1486.485ms` / max `2228ms`、reader full-frame avg `47.295ms`、stdout reader blocked `2194`、no-output-after-input `2213`、output interval avg `46.466ms` / max `719ms` が backlog / throughput 側を示す。
- 次 rerun 用の最小 backlog diagnostics として `continuous_decode_input_throughput_fps`、`continuous_decode_output_to_input_fps_ratio`、`continuous_decode_backlog_frame_gap`、`continuous_decode_backlog_age_ms`、`continuous_decode_backlog_classification` を追加済み。これらは挙動を変えず、既存 counters から input/output throughput と backlog 状態を読みやすくする summary-only diagnostics。
- 最新 unbounded handoff rerun は `S:\stream-sync\manual-logs\program-output-backlog-rerun-unbounded-handoff-20260608-014106`。OBS safety と Program cleanliness / availability は維持されたが、lag criteria は `FAIL`、overall も `FAIL`。`program_render_effective_fps=10.865`、`program_selected_source_frame_lag=37`、`program_continuous_selected_frame_lag=20`、`continuous_decode_latest_input_to_output_frame_gap=37`、`continuous_decode_backlog_classification=pending_correspondence_backlog`。
- 同 rerun では smooth-latest rendered frame と latest continuous frame がともに `856`、selected frame は `876`、rendered-minus-latest-continuous gap は `0`、cache age は `1ms`。Program は最新 continuous decoded frame を拾えている可能性が高く、主因は Program selection ではなく continuous decode output / pending correspondence backlog 側。
- ただし `program_selected_source_frame_lag=37` は smooth-latest selected-minus-rendered / selected-minus-latest-continuous の `20` と基準が異なる可能性があるため、次 rerun 用に `program_selected_source_frame_lag_basis`、`program_selected_source_frame_lag_basis_frame_id`、`program_selected_source_frame_lag_matches_smooth_latest` を summary-only で追加済み。
- smooth-latest 専用 diagnostics として
  `program_smooth_latest_selected_frame_id`、
  `program_smooth_latest_rendered_frame_id`、
  `program_smooth_latest_latest_continuous_frame_id`、
  `program_smooth_latest_selected_minus_rendered_lag`、
  `program_smooth_latest_selected_minus_latest_continuous_lag`、
  `program_smooth_latest_rendered_minus_latest_continuous_gap`、
  `program_smooth_latest_source_mismatch_count`、
  `program_smooth_latest_stale_reuse_count`、
  `program_smooth_latest_cache_age_ms`、
  `program_smooth_latest_frame_age_ms` を追加済み。
- `validation_source_marker_style=large-corner-band-v2` の validation-only source marker は実装済み。P1 / P2 は大きな corner band、block glyph、位置差のある高コントラスト pattern で区別する。既定 behavior は marker disabled のまま。
- smooth-latest の latency / lag accept criteria と OBS safety checklist は docs
  に定義済みだが、最新 rerun では lag が `Warning` になったため、ProgramOutput
  closeout blocker は lag / continuous backlog investigation と operator Preview requirement として継続する。
- 現在の詳細は `docs/operations/obs-capture-validation.md` と `docs/operations/session-log.md` を参照する。

## 次にやること
1. [ ] basis diagnostics 付きで unbounded handoff / smooth-latest rerun を行い、`program_selected_source_frame_lag_basis`、basis frame id、smooth-latest lag との一致/不一致、`continuous_decode_backlog_classification`、input/output fps、output/input ratio、backlog frame gap、backlog age、pending correspondence、stdout reader / output interval を読む
2. [ ] continuous decoder / feed backlog を調査し、throughput below input、FFmpeg scale path、stdout read cadence、output interval、pending correspondence age、completed latency、reader blocked / no-output counts、decoded cache dropping、input feed vs output throughput、selected-source feed priority の要否を切り分ける
3. [ ] 4-view Preview requirement を維持したまま、ProgramOutput non-FPS blocker audit を更新し、closeout blocked を継続する

## 保留 / 限定
- same-loop low-cost Preview refresh tuning
- ProgramOutput closeout
- no-scale-bgra A/B
- scaled-bgr24 adoption
- request/response persistent decoder revival
- same-loop Preview interval tuning

## 未来の作業
- separate Preview cadence/runtime
- lighter renderer / GPU renderer
- hotkey/control pipe after ProgramOutput blockers
- OBS automation / WebSocket
- distributed-PC validation
- hardware encoder

## 現在の主要マイルストーン
- [x] OBS target separation は正しい
- [x] `5/90 + --operator-preview-snapshot-retention` の snapshot retention validation は完了
- [x] Preview black / flicker の解消は確認済み
- [x] current Preview は stable snapshot-only とみなす方針に更新済み
- [x] same-loop low-cost Preview refresh tuning は limited / paused に移行済み
- [x] ProgramOutput near-MVP closeout はまだ行わない方針に更新済み
- [x] clients-before-switcher bootstrap bypass validation は完了
- [x] switcher-first cold start bootstrap validation は完了
- [x] ProgramOutput startup readiness semantics は docs に定義済み
- [x] ProgramOutput startup readiness diagnostics は最小実装済み
- [x] selected source visual verification 方針は docs に定義済み
- [x] validation-only client/source-side visual marker は最小実装済み
- [x] validation-only source marker visibility improvement は実装済み
- [x] selected-source marker reference validation は draft `Good` lag criteria に暫定一致
- [x] OBS ProgramOutput capture safety checklist / operator preflight / manual validation template は docs に定義済み
- [x] `large-corner-band-v2` criteria-based ProgramOutput rerun は記録済み

## 参照メモ
- ProgramOutput の詳細な未解決点は `docs/operations/obs-capture-validation.md` を参照する。
- 検証の時系列や判断理由は `docs/operations/session-log.md` に残す。
- continuous decoder / output / lag / pixel conversion の長い経緯は個別の plan ドキュメントに寄せる。
