<!-- stream-sync/docs/operations/todo.md -->

# StreamSync TODO

最終更新: 2026-05-06

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
- 仕様固定、Cargo workspace 初期化、`apps/*` / `crates/*` の scaffold は完了している
- `crates/protocol` / `crates/config` / `crates/net-core` の最小実装は揃っており、主要 message 型、timestamp 型、fixed header decode / encode、server auth 設定読み込み、`shared_token_env` 解決、UDP 1 datagram receive / send adapter までは完了している
- server 側は auth one-shot、accepted auth registry 登録、heartbeat ack / liveness / timeout action plan / timeout apply / notice queue storage、RTT / offset state commit と metrics snapshot handoff までの最小境界が揃っている
- client 側は auth one-shot、heartbeat one-shot、`HeartbeatAckObservation` 付き `ClientStats` one-shot、one-tick runtime、accepted path 手動確認まで完了している
- client continuous heartbeat loop は thin composition の completed body まで実装済みで、heartbeat timeout notice wakeup planning 境界、wakeup execution 境界、wakeup actual side-effect 境界、outer while-loop connection 境界、outer while-loop one-turn execution body 境界、actual timer wait / retry execution / reconnect 実行境界、outer while-loop 反復実行本体、reconnect policy 境界、caller-owned hook 付き actual socket 再確立境界、real UDP socket 差し替え hook、repeated body からの hook 注入経路まで完了している
- 未完了の中心は production H.264 encoder configuration / error logging policy、late frame queue mutation / drop policy、4-view sync orchestration、dashboard UI rendering、continuous receive/send loop 本体、実キュー / 実送信 / 継続ログ出力
- outbound queue 実キュー、continuous receive/send loop 本体、send / receive の継続ログ出力、file sink open、process-wide logger、`ServerNotice` 実送信は未実装
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
- 次の最小 OBS-facing 実装方針は fixed `1280x720` validation profile を dedicated clean output window `StreamSync 4-view Output` に与え、fixture/composed frame をその output surface へ scale することを第一候補とする。free-form な size arguments や render-surface/window-style 調整は、その profile 実装後も OBS capture が失敗する場合の次段とする
- 次の stdout 拡張候補は `source_width` / `source_height` / `output_width` / `output_height` / `scale_mode` / `window_visible` / `window_capture_candidate` とし、OBS-friendly profile の manual validation で visible surface 条件を追跡しやすくする
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
1. persistent lifecycle を維持したまま `--four-view-clean-output-window-loop` に fixed `1280x720` OBS validation profile を追加し、deterministic `all-renderable` fixture をその output surface へ scale できるようにする
2. 上記 profile 実装後に manual actual runtime pass を再記録し、OBS Window Capture で `StreamSync 4-view Output` を stable capture target として選択・preview できるかを再手動確認する
3. production H.264 encoder configuration / error logging policy
4. Decide later whether `--live-two-view-switcher-once` should be renamed or deprecated after the transport-backed server-mediated path exists

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
- [ ] offset 平滑化を実装する
- [ ] 補正後 timestamp へ変換する処理を実装する
- [ ] targetTime 計算へ接続する
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
10. [ ] OBS Window Capture で switcher 表示を取り込める

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
- [ ] production H.264 encoder configuration / hardware encoder integration
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
- [ ] RTT / offset 平滑化と targetTime 接続
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
- client real encoded video now has bounded multi-frame manual sender wiring: `--auth-real-encoded-video-frame-poc-bounded [config-path] [max-frames] [fragment-pacing-every] [fragment-pacing-delay-ms]` sends `AuthRequest`, requires accepted `AuthResponse`, creates one capture session, repeatedly runs the existing one-shot capture/encode/send boundary on the same UDP socket, and reports attempted/captured/encoded/sent/no-frame/failure counters plus stop reason.
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
- metrics commit, snapshot export cadence, dashboard refresh consumer policy, and dashboard refresh runtime wiring remain separate from timer wait, retry, reconnect, socket ownership, cleanup, UI rendering, video, switcher, and OBS.
- server notice queue storage remains separate from notice send wakeup execution.
- actual dashboard UI rendering remains unimplemented.

## Next Items
1. continuous accept loop / reconnect / lifecycle/service orchestration planning
2. production H.264 encoder configuration / error logging policy
3. Decide later whether `--live-two-view-switcher-once` should be renamed or deprecated after the server-mediated path exists
