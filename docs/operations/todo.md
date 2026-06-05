<!-- stream-sync/docs/operations/todo.md -->

# StreamSync TODO

最終更新: 2026-06-05

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
- ProgramOutput startup readiness semantics は `program_selection_configured` -> `program_selected_source_waiting` -> `program_selected_source_seen` -> `program_first_frame_bootstrapping` -> `program_first_frame_rendered` -> `program_steady_state` として扱う。将来 diagnostics は `program_startup_readiness_state`、`program_selected_source_wait_elapsed_ms`、`program_startup_waiting_for_selected_source_count`、`program_startup_bootstrap_after_source_seen_elapsed_ms` が候補。
- ProgramOutput は near-MVP closeout ではない。non-FPS blocker が残るため closeout は引き続き blocked とし、same-loop Preview tuning も paused のままにする。
- 新 diagnostics は bootstrap decode の elapsed / error class / FFmpeg exit+stderr / payload bytes / NAL kinds / SPS/PPS/IDR / frame_id / slot/client / actual invoke vs pre-invoke skip を読む。
- selected source identity の視認性、smooth-latest の latency / lag accept criteria、OBS capture safety も未整理のまま残す。
- 現在の詳細は `docs/operations/obs-capture-validation.md` と `docs/operations/session-log.md` を参照する。

## 次にやること
1. [ ] ProgramOutput startup readiness diagnostics を小さく実装するか判断する。実装する場合は `program_startup_readiness_state`、`program_selected_source_wait_elapsed_ms`、`program_startup_waiting_for_selected_source_count`、`program_startup_bootstrap_after_source_seen_elapsed_ms` に限定する
2. [ ] ProgramOutput non-FPS blocker audit を継続し、first render の次に selected identity / lag / OBS safety を確認する
3. [ ] selected source visual verification と player1 / player2 の見分けやすさを整理する
4. [ ] smooth-latest の latency / lag acceptance criteria を FPS とは別に定義する
5. [ ] OBS capture safety checklist を作る

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

## 参照メモ
- ProgramOutput の詳細な未解決点は `docs/operations/obs-capture-validation.md` を参照する。
- 検証の時系列や判断理由は `docs/operations/session-log.md` に残す。
- continuous decoder / output / lag / pixel conversion の長い経緯は個別の plan ドキュメントに寄せる。
