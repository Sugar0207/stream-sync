<!-- stream-sync/docs/operations/todo.md -->

# StreamSync TODO

最終更新: 2026-04-25

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
- 未完了の中心は real H.264 decode、dashboard UI rendering、continuous receive/send loop 本体、実キュー / 実送信 / 継続ログ出力
- outbound queue 実キュー、continuous receive/send loop 本体、send / receive の継続ログ出力、file sink open、process-wide logger、`ServerNotice` 実送信は未実装
- video path は server 側 accepted `VideoFrame` receive side-effect を caller-owned per-client queue へ保存し、client 側で placeholder encoded H.264 payload 付き `VideoFrame` を構築・encode・UDP 送信する PoC slice まで完了。real capture / real H.264 encode、decode、display、OBS は未着手

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
1. real encoded `ClientEncodedVideoFrameSource` を使う明示 one-shot client path を placeholder send semantics と分けて追加する
2. production H.264 encoder configuration / error logging policy
3. real H.264 decode / switcher window rendering の最小境界を分けて設計する

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
- [ ] payload fragmentation の要否と方式を決める
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
- [ ] 配信用表示と操作用表示を分けるか決める
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
- [ ] production H.264 encoder configuration / hardware encoder integration
- [x] `VideoFrame` encode
- [x] `VideoFrame` UDP send with explicit placeholder encoded H.264 payload
- [x] placeholder `VideoFrame` one-shot CLI/config launcher
- [x] same-socket auth then placeholder `VideoFrame` one-shot CLI/config launcher
- [x] server frame receive / queue
- [x] switcher placeholder decode / single view display handoff
- [ ] switcher real decode / single view display
- [ ] 30 分連続確認

### フェーズ4: 2 人 / 4 人同期 PoC
- [x] RTT / offset 観測 return と最小 state commit
- [ ] RTT / offset 平滑化と targetTime 接続
- [ ] ジッターバッファ
- [ ] targetTime frame selection
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
- client video path now has an explicit real-capture / H.264-encode replacement boundary: capture returns `RealCaptureDeferred`, encode returns `RealH264EncodeDeferred`, and `ClientEncodedVideoFrameSource` can feed existing `VideoFrame` metadata/send wiring without pretending placeholder bytes are real capture output.
- client capture backend direction is now Windows Graphics Capture for MVP; the client can select/probe that backend and surface not-configured, unsupported, or unavailable results without producing fake pixels or coupling capture to UDP send.
- client capture target discovery now has a pre-session boundary: display/window target descriptors can be represented and converted to `ClientCaptureTargetConfig`, while real Windows enumeration remains deferred and explicit as runtime unavailable.
- client capture target discovery now has an injectable runtime hook, so future real Windows API enumeration can provide descriptors without changing discovery result types or touching frame acquisition.
- client capture session preparation now converts a selected display/window descriptor or target config into metadata-only `ClientCaptureSessionConfig` for future Windows Graphics Capture session creation without opening a session or acquiring frames.
- client capture session runtime creation now consumes `ClientCaptureSessionConfig` through `ClientCaptureSessionRuntimeInput` and a caller-owned runtime hook. The default placeholder-safe hook still reports unavailable/unsupported, while the Windows-only `ClientWindowsGraphicsCaptureSessionRuntimeHook` creates a ready Windows Graphics Capture item/frame-pool/session.
- client Windows Graphics Capture frame acquisition now has a separate one-frame boundary: `ClientCaptureFrameAcquisitionBoundary` consumes a ready `ClientCaptureSessionRuntime`, can explicitly start capture when requested, attempts one `TryGetNextFrame`, and returns a raw BGRA frame / no-frame / not-started / unavailable / failed result without encoding or UDP send.
- client raw BGRA frames now have a separate H.264 encoder hook boundary: `ClientH264EncoderInput::from_raw_frame` carries `ClientRawCapturedVideoFrame`, `ClientH264EncoderRuntimeHook` can provide real H.264 payload bytes, and the boundary produces `RealCaptureH264` only from non-empty hook output. The default hook remains explicit encode-deferred.
- client H.264 encoding now has a first real software runtime hook: `ClientFfmpegSoftwareH264EncoderRuntimeHook` invokes `ffmpeg` / `libx264` for one BGRA frame and returns an Annex B H.264 elementary stream, while missing FFmpeg and encode failures remain explicit.
- metrics commit, snapshot export cadence, dashboard refresh consumer policy, and dashboard refresh runtime wiring remain separate from timer wait, retry, reconnect, socket ownership, cleanup, UI rendering, video, switcher, and OBS.
- server notice queue storage remains separate from notice send wakeup execution.
- actual dashboard UI rendering remains unimplemented.

## Next Items
1. connect real encoded `ClientEncodedVideoFrameSource` to an explicit client one-shot path without changing placeholder send semantics
2. production H.264 encoder configuration / error logging policy
3. real H.264 decode / switcher window rendering boundary
4. targetTime / jitter-buffer selection design for the next 2-view sync PoC
