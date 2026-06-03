<!-- stream-sync/docs/operations/todo.md -->

# StreamSync TODO

最終更新: 2026-06-03

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
- `NoDecodedFrameForSelection` を含む first render / missing selected source の問題は、startup diagnostics を追加済み。次は rerun evidence で selection/source/input/output/renderable のどこが詰まるかを読む。
- ProgramOutput startup diagnostics は、selection 解決、continuous source 解決、first source frame、first continuous input/output、first renderable decoded frame、startup missing reason、fallback / pending / source mismatch を stdout summary に出す。
- selected source identity の視認性、smooth-latest の latency / lag accept criteria、OBS capture safety も未整理のまま残す。
- 現在の詳細は `docs/operations/obs-capture-validation.md` と `docs/operations/session-log.md` を参照する。

## 次にやること
1. [ ] ProgramOutput startup diagnostics 付き rerun を実施し、first render delay / `NoDecodedFrameForSelection` の原因を evidence で切り分ける
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

## 参照メモ
- ProgramOutput の詳細な未解決点は `docs/operations/obs-capture-validation.md` を参照する。
- 検証の時系列や判断理由は `docs/operations/session-log.md` に残す。
- continuous decoder / output / lag / pixel conversion の長い経緯は個別の plan ドキュメントに寄せる。
