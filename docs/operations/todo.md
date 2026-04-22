<!-- stream-sync/docs/operations/todo.md -->

# StreamSync TODO

最終更新: 2026-04-22

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
- 仕様固定と土台作りは概ね完了
- Cargo workspace と `apps/*` / `crates/*` の初期 scaffold は完了
- `crates/protocol` の基本型、主要 message 型、timestamp 型、fixed header decode、`AuthRequest` / `AuthResponse` / `Heartbeat` / `HeartbeatAck` / `VideoFrame` / `ClientStats` / `ServerNotice` payload decode / encode は完了
- `crates/config` の server auth 設定 TOML 読み込み最小実装は完了
- `crates/config` の `shared_token` / `shared_token_env` token reference 読み分けと inline secret debug redaction は完了
- `crates/net-core` の inbound decode 境界、outbound packet / queue 境界、outbound queue lifecycle 境界、protocol encoder 呼び出し境界、send error / log event 分類 placeholder、UDP socket 1 datagram receive / send adapter は完了
- `apps/server` の inbound router、UDP receive loop step、UDP socket adapter 接続、auth response PoC one-shot 起動接続、auth response PoC 起動設定接続、receive loop から packet acceptance gate への接続境界、registered packet handler handoff 境界、heartbeat handler ack handoff 境界、heartbeat state / timebase input 境界、heartbeat liveness state commit 境界、heartbeat timeout policy evaluation 境界、heartbeat timebase plan、heartbeat RTT / offset stateless calculator、heartbeat ack observation flow、heartbeat observation carrier、packet acceptance rejection の drop / log handoff 境界、receive rejection JSON Lines event schema 境界、receive rejection stderr JSON Lines 最小出力、auth handler boundary、auth config input boundary、server auth decision 最小実装、`shared_token_env` secret resolver 最小実装、auth success / failure log handoff 境界、auth JSON Lines event schema 境界、auth result stderr JSON Lines 最小出力、auth flow step、認証済み送信元 registry 境界、packet acceptance gate 境界、AuthResponse response boundary、HeartbeatAck ack boundary、outbound queue handoff、`--receive-send-twice` による auth-then-heartbeat 2 iteration 入口、`--receive-send-three` による heartbeat observation return 入口は完了
- accepted auth path で `AuthenticatedSenderRegistry` へ in-memory 登録する実処理は完了
- `apps/client` の client 設定読み込み、AuthRequest 構築、protocol encoder、UDP one-shot send、AuthResponse one-shot receive / stdout 表示、accepted auth 後の Heartbeat one-shot send / HeartbeatAck receive stdout 表示、HeartbeatAckObservation を載せた ClientStats one-shot send の PoC 入口は完了
- server / client one-shot auth round trip の手動確認手順と accepted path 用 helper config は完了
- `shared_token_env` を使う one-shot auth round trip 手順と server helper config は完了
- accepted path の手動確認は成功し、`configs/examples/server.example.toml` と `configs/examples/client.accepted.example.toml` の組み合わせで `accepted=true`, `reason_code=Ok` を観測済み
- `shared_token_env` accepted path の手動確認は成功し、`configs/examples/server.env-token.example.toml` と `configs/examples/client.accepted.example.toml` の組み合わせで `accepted=true`, `reason_code=Ok` を観測済み
- `--receive-send-once` accepted path の手動通し確認は成功し、`configs/examples/server.example.toml` と `configs/examples/client.accepted.example.toml` の組み合わせで server 側 `sent_bytes=55`, `accepted=true`, `reason_code=Ok` を観測済み
- `--auth-request-poc-once` は accepted path で client 側 `AuthResponse` を 1 回受信して stdout に表示できる。`accepted=true`, `reason_code=Ok` を client stdout で観測済み
- `--auth-heartbeat-poc-once` は accepted auth 後に同じ UDP socket で `Heartbeat` を 1 回送り、`HeartbeatAck` を 1 回受信して stdout に表示する入口として追加済み。`--receive-send-twice` と組み合わせる手順は docs に反映済み
- `--auth-heartbeat-stats-poc-once` は `HeartbeatAck` 受信後に `HeartbeatAckObservation` を `ClientStats` optional block へ載せて 1 回送信できる。`--receive-send-three` はそれを受信して既存 timebase plan / stateless calculator へ渡す入口として追加済み
- `ServerHeartbeatLivenessCommitBoundary` は registered heartbeat から作られた `ServerHeartbeatStateInput` を `ServerHeartbeatLivenessState` へ 1 回 commit できる。`--receive-send-twice` / `--receive-send-three` は preserved heartbeat handoff から liveness state を 1 回更新し、stdout に entry 数を表示できる
- heartbeat timeout は `ServerHeartbeatTimeoutPolicy` と `evaluate_timeout` で 1 client 分を `Alive` / `TimedOut` / `NoHeartbeat` に分類できるが、continuous scan、auth registry 失効、通知、ログ出力への接続は未実装
- `ClientStats` payload encode/decode と heartbeat observation optional block の最小 wire 変換は完了
- secret store provider 連携、token hashing、rotation 実行、認証済み送信元の timeout / 失効 / 再認証、実際の packet 破棄、時刻同期本体、映像受信・復号・表示、switcher UI は未実装
- `ClientStats` receive route / gate / registered handler bridge と、`HeartbeatAckObservation` を既存 timebase plan / stateless calculator へ渡す最小接続は完了。継続送信 loop、metrics state commit、RTT / offset state commit は未実装
- outbound queue の実処理範囲、backpressure / capacity 方針、送信継続 loop 前の bounded storage / encoder handoff 範囲、packet 送信継続 loop の最小接続範囲と loop 本体の実装範囲は整理済み。実キュー collection、送信継続 loop 本実装、retry 実行 / requeue は未実装
- `ServerNotice` payload layout、decode / encode 最小実装、notice trigger policy の実装範囲整理は完了。state transition 検知、重複抑制、rate limit、送信継続 loop、socket send 接続は未実装
- auth / receive JSON Lines file sink 方針は整理済み。実 file open、rotation、retention、async logging、process-wide logger は未実装
- send JSON Lines writer の実接続範囲は整理済み。failure-only の `server.send_error` event schema / caller-owned writer / sink plan placeholder と、one-iteration receive/send runtime から `server.send` success/failure observation を caller-owned writer へ書く最小接続は追加済みだが、continuous send loop からの実接続、file sink open、process-wide logger は未実装
- receive loop の継続運用向けログ範囲は整理済み。`server.receive_loop` の event schema / caller-owned writer / sink plan placeholder は追加済みだが、continuous receive loop からの実接続、file sink open、process-wide logger は未実装
- continuous receive loop 本体の実装範囲、1 tick 実接続範囲、operational / rejection writer への handoff 範囲、caller-owned writer 呼び出し範囲、handler handoff 実接続範囲、最小 1 tick 実行接続範囲、継続 loop controller の外枠範囲、handler dispatch への最小 handoff 範囲、handler dispatch 本体の最小分類範囲、auth dispatch の最小実接続範囲、registered packet handler の最小実接続範囲、video / stats handler の最小 input 接続範囲、continuous receive loop body から dispatch runtime を呼ぶ最小範囲、dispatch runtime 結果の side effect 適用範囲、accepted auth の outbound queue storage / auth log writer 最小接続範囲、send loop / queue collection の最小接続範囲、send JSON Lines writer の one-iteration 最小実接続範囲、continuous receive loop と one-item send runtime の最小結合範囲、controller が one-iteration receive/send runtime を呼ぶ最小範囲、completed one-iteration runtime の CLI / config 接続範囲は整理済み。loop lifecycle / tick / writer handoff / writer runtime / handler handoff runtime / one-tick runtime / controller / handler dispatch bridge / handler dispatch result / auth dispatch runtime / registered packet dispatch runtime / video stats handler runtime / body dispatch runtime / side effect apply / output apply / queue collection / send one runtime / send log output / receive-send one iteration runtime / controller receive-send runtime placeholder、one-iteration launcher、1 iteration だけの最小 loop body は追加済みだが、完成した継続 receive/send loop、retry / requeue、rejection response 送信 policy、video buffer / sync handoff 本体、stats state commit 本体、packet drop 本体、file sink open、process-wide logger は未実装
- secret store / token rotation 方針は整理済み。SecretStore 参照と rotation policy placeholder は追加済みだが、provider 連携、rotation 実行、hot reload は未実装
- 次の中心は timeout evaluation 結果を auth 失効 / ログ / notice へ接続する方針、RTT / offset estimate の state commit 方針、continuous heartbeat loop に進む前の境界整理

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
1. timeout evaluation 結果を auth 失効 / ログ / notice へ接続する方針を整理する
2. RTT / offset estimate を server 側 state に commit する最小境界を整理する
3. continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する

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
- [x] server 側 `ServerHeartbeatLivenessCommitBoundary` / `ServerHeartbeatLivenessState` / timeout evaluation boundary を追加する
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
- [x] accepted auth 後の heartbeat one-shot 送信処理を client 側に実装する
- [x] registered heartbeat 受信から `HeartbeatAck` one-shot send までを server 側に接続する
- [x] registered heartbeat から `ServerHeartbeatLivenessState` へ 1 回 commit する最小境界を追加する
- [x] heartbeat timeout policy evaluation の最小境界を追加する
- [ ] continuous heartbeat loop を client 側に実装する
- [ ] heartbeat timeout evaluation 結果を失効 / ログ / notice へ接続する
- [ ] RTT 計測を実装する
- [ ] clock offset 推定を実装する
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
- [ ] client 側で frame metadata を付与する
- [ ] client 側で H.264 encode を行う
- [ ] UDP で frame を送信する
- [ ] server 側で認証済み client の frame だけ受理する
- [ ] server 側で client ごとの受信キューを作る
- [ ] 不正 frame 破棄を実装する
- [ ] 受信遅延と drop を計測する
- [ ] sync-core のジッターバッファへ投入する
- [ ] frame 欠落時の代替表示方針を決める

---

## client 側
- [x] AuthRequest one-shot PoC 用のクライアント起動処理を作る
- [x] AuthRequest one-shot PoC 用の TOML 設定読み込み処理を作る
- [x] `client_id` / `shared_token` を設定から読み込む
- [x] `run_id` を設定から受け取る
- [x] `app_version` / `protocol_version` を `AuthRequest` に入れて送信する
- [x] 認証メッセージを 1 回だけ送信する PoC 処理を作る
- [x] `--auth-request-poc-once` で `AuthResponse` を 1 回だけ受信して stdout に表示する
- [x] `--auth-heartbeat-poc-once` で accepted auth 後に `Heartbeat` を 1 回だけ送信し、`HeartbeatAck` を stdout に表示する
- [x] `--auth-heartbeat-stats-poc-once` で `HeartbeatAckObservation` を `ClientStats` に載せて 1 回だけ送信する
- [x] server / client one-shot auth round trip の手動確認手順を追加する
- [x] accepted path 用の one-shot client example config を追加する
- [x] heartbeat one-shot 送信処理を作る
- [ ] continuous heartbeat loop を作る
- [ ] 画面キャプチャに成功する
- [ ] Minecraft ウィンドウの取得確認をする
- [ ] frame id / captureTimestamp / sendTimestamp を付与する
- [ ] H.264 encode 処理を実装する
- [ ] ハードウェア encode 優先処理を実装する
- [ ] ソフトウェア encode fallback を実装する
- [ ] 720p / 30fps を初期値にする
- [ ] 1080p / 60fps を将来有効化できる構造にする
- [ ] UDP 送信処理を実装する
- [ ] stats 送信処理を実装する
- [ ] 切断 / 再接続処理を実装する

---

## switcher / 表示 / OBS
- [x] OBS 連携方法を Window Capture に決定する
- [x] switcher は表示専用とする方針を決定する
- [x] 4 分割表示と単独表示の切り替えを MVP 対象にする
- [ ] 1 視点の復号に成功する
- [ ] 1 視点の表示に成功する
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
5. [ ] client が 1 視点の H.264 `VideoFrame` を送れる
6. [ ] server が 1 視点の frame を受信し、破棄 / 受理を判定できる
7. [ ] switcher が 1 視点を復号・表示できる
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
- [ ] client capture / encode
- [x] `VideoFrame` encode
- [ ] `VideoFrame` UDP send
- [ ] server frame receive / queue
- [ ] switcher decode / single view display
- [ ] 30 分連続確認

### フェーズ4: 2 人 / 4 人同期 PoC
- [ ] RTT / offset 推定
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
