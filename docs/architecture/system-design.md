<!-- stream-sync/docs/architecture/system-design.md -->

# StreamSync System Design

## 1. 目的

StreamSync は、4人のゲーム映像を受信し、共通の時間軸で同期したうえで、OBS に載せられる形で表示・手動切り替えできる多視点スイッチング配信ツールです。

このドキュメントでは、MVP 段階におけるシステム全体の責務分担、コンポーネント構成、データフロー、および同期の考え方を整理します。

---

## 2. システム全体像

MVP では、以下の構成を採用します。

    client1 ─┐
    client2 ─┼──> server ───> switcher ───> OBS
    client3 ─┤
    client4 ─┘

- 各 client はゲーム画面をキャプチャし、エンコードし、UDP で server に送信する
- server は各 client から受けた映像を共通の時間軸にそろえる責任を持つ
- switcher は server から受けた同期済み映像を表示し、4分割表示や単独表示を切り替える
- OBS は switcher の表示ウィンドウを Window Capture で取り込む
- MVP では server と switcher は同一 PC 上で動作してよい

---

## 3. コンポーネントと責務

### 3.1 client
各配信メンバーの PC で動作する送信クライアント。

#### 主な責務
- 対象ゲーム画面のキャプチャ
- フレーム取得
- captureTimestamp 付与
- エンコード
- frameId / sendTimestamp / keyframe 情報付与
- server への UDP 送信
- 認証メッセージ送信
- heartbeat 送信
- stats 送信

#### 責務に含めないもの
- 複数視点の同期
- 配信用レイアウト制御
- OBS 連携
- 音声統合

---

### 3.2 server
受信、同期、時刻補正、バッファ制御の中核。

#### 主な責務
- client からの認証受付
- 認証済み送信元管理
- 映像フレーム受信
- RTT 計測
- clock offset 推定
- 共通時間軸への変換
- client ごとのジッターバッファ管理
- targetTime 計算
- 表示すべきフレーム選択
- switcher へ同期済み映像を渡す
- 状態 / メトリクス集計
- ログ出力

#### 責務に含めないもの
- 配信画面の最終UI制御
- OBS への直接出力
- 自動スイッチング
- 音声ミックス

---

### 3.3 switcher
表示と手動スイッチングを担当する UI アプリ。

#### 主な責務
- server から受けた映像の表示
- 4分割表示
- 単独表示
- メイン視点切り替え
- ホットキー操作
- 接続状態表示
- RTT / offset / 実効遅延 / fps / drop率 の表示
- 配信用表示ウィンドウの提供
- 必要に応じた不要UI非表示

#### 責務に含めないもの
- 同期の責任
- RTT 推定や offset 計算
- 映像送信
- 音声経路管理

---

### 3.4 OBS
配信用の最終出力先。

#### 主な責務
- switcher 専用表示ウィンドウの取り込み
- 配信シーン管理
- YouTube 等への配信出力

#### 責務に含めないもの
- 多視点同期
- 受信側バッファ制御
- 映像時間軸の補正

---

## 4. 音声の扱い

MVP では、音声は StreamSync の中核責務に含めない。

### 方針
- 通話は Discord を継続使用する
- 配信用音声は 1 系統に固定する
- 視聴者向け映像は、その基準音声に合わせて遅延調整する
- スイッチャー監視用音声は低遅延のまま扱う
- 音声統合は将来拡張とする

---

## 5. データフロー

### 5.1 映像フロー
1. client がゲーム画面をキャプチャする
2. client がフレームを取得する
3. client がフレームに metadata を付与する
4. client がフレームをエンコードする
5. client が UDP で server に送信する
6. server がフレームを受信する
7. server が時刻補正を適用する
8. server が client ごとのバッファへ格納する
9. server が targetTime に対して各 client の表示フレームを選ぶ
10. switcher が同期済み映像を表示する
11. OBS が switcher の専用表示ウィンドウを取り込む

### 5.2 制御フロー
1. client 起動
2. client が認証メッセージ送信
3. server が token / client_id / protocol_version を確認
4. server が認証済み扱いにする
5. client が heartbeat を定期送信
6. server が timeout を監視
7. 切断または timeout 時に認証済み状態を解除する

### 5.3 net-core / protocol receive boundary

PoC / MVP 初期では、UDP socket 実装に進む前に、受信 packet bytes から decode 済み message を app / server handler へ渡す境界だけを固定する。

受信時の責務分担:

- `crates/net-core`
  - 将来の UDP 受信層から生の packet bytes と送信元 address を受け取る。
  - packet bytes と送信元 metadata を `InboundPacket` として保持する。
  - `protocol` crate の fixed header decode、protocol_version 検証、message_type dispatch を順番に呼ぶ。
  - decode に成功した場合は、送信元 metadata と `ProtocolMessage` を `DecodedInboundPacket` として app / server handler 側へ渡す。
  - decode error は送信元 metadata と合わせて返す。
- `crates/protocol`
  - 16 byte fixed header を decode し、payload slice の境界を決める。
  - `DecodeContext.expected_protocol_version` と fixed header の `protocol_version` を比較する。
  - `message_type` に応じて payload decoder を選び、payload bytes を message 型へ変換する。
  - UDP socket、認証済み送信元管理、handler 実行、ログ出力判断は持たない。
- app / server handler
  - decode 済み message と送信元 metadata を受け取る。
  - 認証、client whitelist、heartbeat timeout、VideoFrame 受理、同期バッファ投入などの app 状態変更を行う。
  - protocol error を接続拒否、packet 破棄、ログ出力へ変換する。

呼び出し順序:

1. UDP 受信層が raw packet bytes と送信元 address を得る。
2. `net-core` が `InboundPacket` と `DecodeContext` を受け取る。
3. `net-core` が `protocol::decode_fixed_header` を呼ぶ。
4. `net-core` が `protocol::validate_protocol_version` を呼ぶ。
5. `net-core` が `protocol::decode_payload_by_message_type` を呼ぶ。
6. `net-core` が `DecodedInboundPacket` を app / server handler 側へ返す。
7. app / server handler が認証、同期、buffer、表示などの処理を行う。

この境界は packet decode の接続点を決めるためのものであり、実際の UDP socket loop、送信処理、server / client / switcher handler 実装はまだ行わない。

### 5.4 server handler boundary

server 側 handler は、`net-core` から `DecodedInboundPacket` を受け取った後の app 境界とする。`protocol` と `net-core` は packet を decode 済み message に変換するまでを担当し、server は decode 済み message の種類を見て、認証、heartbeat、video frame の各処理へ分岐する。

server 側の入力:
- `DecodedInboundPacket.source`
  - raw UDP packet の送信元 metadata。
  - 認証済み送信元管理やログ付与の材料にする。
- `DecodedInboundPacket.message`
  - `protocol` が復元した `ProtocolMessage`。
  - server は message variant と `message_type` 相当の意味を見て処理方針を選ぶ。

server 側の分岐:
- `AuthRequest`
  - 認証処理へ渡す。
  - shared token 検証、client whitelist 照合、認証済み送信元登録、AuthResponse 生成判断は server 側の責務とする。
- `Heartbeat`
  - heartbeat 処理へ渡す。
  - 生存確認更新、timeout 管理、RTT / offset 計測材料としての扱い、HeartbeatAck 生成判断は server 側の責務とする。
- `VideoFrame`
  - video frame 処理へ渡す。
  - 認証済み client かどうかの確認、古い frame の破棄、時刻補正、ジッターバッファ投入、同期処理は server / sync 側の責務とする。
- その他 message
  - 現時点では server inbound の対象外として扱う。
  - `AuthResponse`, `HeartbeatAck`, `ClientStats`, `ServerNotice` の server inbound としての扱いは別タスクで決める。

実装上は `apps/server` に `ServerInboundRouter` を置き、`DecodedInboundPacket` を `ServerInboundRoute` に分類する。これは handler 本体ではなく、server handler へ渡す境界名を固定するための placeholder とする。認証成功 / 失敗判定、heartbeat 管理、video frame 処理本体はまだ実装しない。

### 5.5 server UDP receive loop boundary

server 側の UDP 受信 loop は、UDP socket で packet bytes と送信元情報を受け取った後、decode と server route へ渡すまでを担当する。現在は同期 `UdpSocket` で 1 packet を受信して既存の receive loop 境界へ渡す最小 adapter だけを実装し、継続 loop、非同期 runtime、handler 本体、packet 破棄、ログ出力はまだ実装しない。

最小処理順:

1. UDP socket 層が packet bytes を受信する。
2. UDP socket 層が送信元 address を取得する。
3. server receive loop が送信元 address を `PacketSource` に変換する。
4. server receive loop が packet bytes と `PacketSource` から `InboundPacket` を生成する。
5. server receive loop が `InboundPacketDecoder` を呼ぶ。
6. `InboundPacketDecoder` が fixed header decode、`protocol_version` check、payload decode を行い、成功時に `DecodedInboundPacket` を返す。
7. server receive loop が `DecodedInboundPacket` を `ServerInboundRouter` に渡す。
8. `ServerInboundRouter` が `ServerInboundRoute` を返す。
9. decode 成功時、server receive loop が packet acceptance gate を呼ぶ。
10. accepted の route だけが server handler / router 後続境界へ進む。
11. rejected は実際の handler を呼ばず、将来の drop / log layer へ渡す decision として保持する。

decode error / protocol error の扱い:

- `UnsupportedProtocolVersion`
  - protocol version mismatch として分類する。
  - 初期実装では packet を破棄し、将来 `AuthResponse` / `ServerNotice` の encode が入った段階で拒否通知を検討する。
- `PayloadDecodeNotImplemented`
  - server inbound として未対応の message として分類する。
  - 初期実装では packet を破棄し、warn ログ候補とする。
- その他の `ProtocolError`
  - malformed packet として分類する。
  - 初期実装では packet を破棄する。

receive loop / gate 接続の責務分離:

- receive loop
  - raw packet bytes と source metadata を受け取る future socket 層の直後に置く。
  - `InboundPacketDecoder` を呼び、decode error は `ServerRejectedPacket` として分類する。
  - decode 成功後、`ServerInboundRouter` で route を作り、`PacketAcceptanceGateBoundary` を呼ぶ。
  - gate の rejected decision を drop / log layer へ渡す形に留め、実際の破棄やログ出力は行わない。
- decode / protocol
  - fixed header、`protocol_version`、payload layout を検証し、decode 済み message を返す。
  - 認証済み endpoint かどうかは判断しない。
- packet acceptance gate
  - route と `AuthenticatedSenderRegistry` を見て accepted / rejected を返す。
  - `AuthRequest` は認証前の入口として通し、`Heartbeat` / `VideoFrame` は registry lookup 対象にする。
- handler
  - accepted の route だけを受け取る将来境界とする。
  - heartbeat 管理、video frame 処理、state 更新はここより後段に残す。
- drop / log layer
  - rejected decision を受け取り、将来の実破棄と JSON Lines ログ出力を担当する。

実装上は `apps/server` に `ServerReceiveLoopStep` を置く。これは 1 packet 分の境界であり、受信済みの packet bytes と `PacketSource` を受け取り、`InboundPacketDecoder` と `ServerInboundRouter` を順番に呼ぶ。既存の decode / route だけの結果は `ServerReceiveLoopOutcome` で返し、gate 接続版は `ServerReceiveLoopGateOutcome` で accepted route または decode / acceptance rejection を返す。実際の継続 socket loop、packet 破棄、ログ出力、認証判定、heartbeat 管理、video frame 処理本体はまだ持たない。

### 5.6 UDP socket minimal I/O

UDP socket I/O の最小実装は、同期 `std::net::UdpSocket` を使った 1 datagram 単位の adapter とする。これは PoC の socket 接続確認を進めるための最小層であり、async runtime、retry、fragmentation、encryption、queue 実処理は含めない。

Receive flow:

1. 呼び出し側が bind 済み `UdpSocket` と受信用 buffer を用意する。
2. `net-core::UdpSocketIoBoundary::receive_one` が `recv_from` を 1 回呼ぶ。
3. 受信した source address を `PacketSource` に変換し、buffer slice と合わせて `UdpReceivedPacket` として返す。
4. `apps/server::ServerUdpSocketIoStep::receive_one_with_gate` が `UdpReceivedPacket` を `ServerReceiveLoopStep::handle_received_packet_with_gate` へ渡す。
5. accepted route は handler 後続境界の候補になり、rejected decision は drop / log handoff の候補になる。

Send flow:

1. outbound item は既存の net send layer / protocol encoder 境界で `EncodedOutboundPacket` になる。
2. `net-core::UdpSocketIoBoundary::send_encoded` が encode 済み bytes と destination を受け取る。
3. `send_to` を 1 回呼び、送信できた byte 数を返す。
4. `apps/server::ServerUdpSocketIoStep::send_encoded` は server 側から同じ送信 adapter を呼ぶ薄い接続境界とする。

Responsibility split:

- UDP socket I/O
  - Owns `bind`, one-packet `recv_from`, and one-packet `send_to`.
  - Does not decode protocol payloads, run handlers, retry, fragment, encrypt,
    or write logs.
- receive loop
  - Receives bytes plus `PacketSource` from socket I/O and calls decode / route
    / gate.
  - Does not call `recv_from` directly in its core boundary.
- net send layer / protocol encoder
  - Produces `EncodedOutboundPacket` before socket send.
  - Does not own the OS socket.
- queue / retry policy
  - Still future work. The current socket adapter sends one encoded datagram
    and returns the OS result.

Current code reflects this with `net-core::UdpSocketIoBoundary`,
`net-core::UdpReceivedPacket`, `net-core::DEFAULT_UDP_PACKET_BUFFER_LEN`, and
`apps/server::ServerUdpSocketIoStep`. Continuous receive loop orchestration,
async runtime integration, retry, fragmentation, encryption, queue runtime,
packet drop execution, and JSON Lines log output remain unimplemented.

### 5.7 AuthResponse PoC one-shot startup step

AuthResponse PoC startup uses the existing boundaries as a one-packet
connection path. It is not a long-running server loop.

Flow:

1. A caller owns a bound synchronous `UdpSocket`, a receive buffer,
   `ServerAuthConfig`, and an in-memory `AuthenticatedSenderRegistry`.
2. `ServerAuthResponsePocStep::run_one` calls
   `ServerUdpSocketIoStep::receive_one_with_gate`.
3. The receive side performs socket receive -> receive loop -> fixed header
   decode -> payload decode -> packet acceptance gate.
4. Only an accepted `AuthRequest` route continues to `ServerAuthFlowStep`.
5. `ServerAuthFlowStep` prepares auth input, runs the minimal auth decision,
   creates typed `AuthResponse`, and hands it to `ServerOutboundQueueBoundary`
   as `OutboundQueueItem`.
6. Accepted decisions may register the source in the in-memory authenticated
   sender registry through the existing registry boundary.
7. `OutboundPacketEncoderBoundary` prepares the encode request and calls
   `ProtocolMessageEncoderBoundary`.
8. The encoded `AuthResponse` bytes and destination are passed to
   `ServerUdpSocketIoStep::send_encoded`, which performs one `send_to`.

Responsibility split:

- auth response PoC step
  - Composes existing receive, auth, queue handoff, encode, and socket send
    boundaries for one packet.
  - Does not own auth policy, queue runtime, retry, fragmentation, encryption,
    JSON Lines output, heartbeat handling, or video frame handling.
- receive loop / gate
  - Produces an accepted route or typed rejection.
  - Does not execute the auth decision.
- auth flow
  - Produces `ServerAuthDecision`, auth log handoff input, registry
    registration handoff, `AuthResponse`, and `OutboundQueueItem`.
  - Does not encode or send bytes.
- net send / protocol encoder
  - Converts the queued typed `AuthResponse` into fixed header + payload bytes.
- socket send
  - Sends the already encoded datagram once.

Current code reflects this with `apps/server::ServerAuthResponsePocStep`,
`ServerAuthResponsePocOutcome`, and `ServerAuthResponsePocError`. Continuous
looping, async runtime integration, real queue scheduling, retry,
fragmentation, encryption, JSON Lines output, heartbeat handling, and video
frame handling remain unimplemented.

---

## 6. 同期の考え方

### 6.1 基本方針
完全同期を目標にするが、MVP では以下の考え方で進める。

- client ごとの captureTimestamp を基準にする
- server が RTT と clock offset を推定する
- 受信後、各フレームを共通時間軸へ変換する
- 表示時は targetTime を基準に各 client のフレームを選ぶ
- 「早く届いたものをそのまま出す」のではなく、「同じ時間のものを出す」ことを優先する

### 6.2 targetTime
targetTime は、現在時刻から固定遅延量を引いた「表示対象の共通時刻」とする。

例:
- 現在時刻が T
- 固定遅延が D
- 表示対象は `T - D`

各 client について、
- `T - D` に最も近い共通時刻フレームを選ぶ

### 6.3 バッファ
server は client ごとにバッファを持つ。

バッファの役割:
- ジッター吸収
- 遅延揺れの平準化
- targetTime に合わせたフレーム選択

### 6.4 欠損時
表示対象に適切なフレームが無い場合は、次のいずれかを取る。

- 前フレーム保持
- 切断中 / 欠落中表示
- 再接続待ち表示

詳細な優先順位は protocol / runtime 仕様で定義する。

---

## 7. 状態管理の考え方

各 client はおおむね以下の状態を取る。

- 未接続
- 認証待ち
- 認証済み
- 映像受信中
- 同期待ち
- 安定表示中
- 遅延悪化
- 切断
- 再接続中

switcher では最低限、以下を表示できるようにする。

- 接続状態
- RTT
- offset
- 実効遅延
- fps
- drop率
- buffer 状態

---

## 8. バージョン管理

### app_version
- 各アプリの配布物バージョン
- UI変更や内部改善も含む

### protocol_version
- 通信仕様のバージョン
- 認証メッセージ、フレームヘッダ、heartbeat、stats などの互換性を表す

### MVP 方針
- protocol_version 不一致は接続拒否
- app_version 差異は warn ログ

---

## 9. ログとメトリクス

### ログ
- JSON Lines 形式
- client / server / switcher ごとに出力
- run_id と client_id を付与する

### メトリクス
switcher 上でリアルタイム表示する。

最低限:
- 接続状態
- RTT
- offset
- 実効遅延
- fps
- drop率
- buffer 状態

---

## 10. 初期実装の優先順位

### フェーズ1
- workspace 初期化
- docs 初版作成
- 共通型定義
- 認証メッセージ / heartbeat メッセージ定義

### フェーズ2
- 1人送信 / 受信 / 表示
- timestamp 付与
- RTT / offset 推定

### フェーズ3
- 2人同期表示
- 4人同期表示
- switcher UI
- OBS 取り込み確認

### フェーズ4
- 長時間試験
- ログ可視化
- 異常系対応
- MVP 安定化

---

## 11. 将来拡張
- 音声統合
- 発話検知
- 自動スイッチング
- Minecraft イベント連動
- 録画 / リプレイ / クリップ生成
- 5人以上対応
- 視点数動的増減
- 1080p / 60fps の本格運用
---

## Server Auth Handler Boundary

PoC / MVP initial implementation keeps server authentication as a boundary,
not as completed business logic.

Flow:

1. `net-core` receives raw packet bytes and source metadata through the future
   UDP receive layer.
2. `net-core` calls `protocol` to decode the fixed header, validate
   `protocol_version`, dispatch by `message_type`, and produce
   `DecodedInboundPacket`.
3. `ServerInboundRouter` receives `DecodedInboundPacket`.
4. `ServerInboundRouter` recognizes `ProtocolMessage::AuthRequest` and returns
   `ServerInboundRoute::AuthRequest`.
5. The auth handler boundary receives the decoded `AuthRequest` and prepares
   auth decision input.
6. Applying the auth result to server state and generating an `AuthResponse`
   is handled outside the auth handler boundary.

Responsibility split:

- `protocol`
  - Owns wire format, fixed header decode, `protocol_version` validation helper,
    `message_type` dispatch, and `AuthRequest` payload decode.
  - Does not know token validity, client whitelist, source trust, server state,
    or response policy.
- `net-core`
  - Owns raw packet bytes plus source metadata and calls protocol decode.
  - Produces `DecodedInboundPacket`.
  - Does not run authentication logic or server state transitions.
- `ServerInboundRouter`
  - Owns routing decoded messages to server-side boundaries.
  - Recognizes `AuthRequest` and passes the decoded message to the auth route.
  - Does not validate token, whitelist, `app_version`, or connection state.
- auth handler boundary
  - Receives decoded `AuthRequest`.
  - Owns the future decision inputs: `shared_token`, `client_id`,
    `protocol_version`, `app_version`, plus `run_id` and `display_name` when
    useful for logging or state correlation.
  - Does not update authenticated-client state, send responses, or manage UDP
    source registration in this step.
- server state / response layer
  - Will consume the future auth result.
  - Owns connection allow/reject state updates, authenticated source
    registration, and outbound `AuthResponse` generation/sending.

Current code reflects this with `ServerAuthHandlerBoundary` and
`ServerAuthCheck` placeholders in `apps/server`. These types only prepare the
decoded `AuthRequest` for future auth decision logic. Real token verification,
client whitelist loading, auth success/failure decisions, and response sending
remain unimplemented.

---

## Server Auth Config Input Boundary

PoC / MVP initial implementation separates auth configuration loading from the
actual authentication decision. The config crate now performs the minimal TOML
read for allowed clients and shared token entries, while the server keeps token
comparison and auth state changes in later boundaries.

Flow:

1. `ServerAuthConfigBoundary` reads the auth portion of server TOML and
   produces `ServerAuthConfig`.
2. For each `[auth.clients.<client_id>]` entry, config creates one
   `AllowedClientConfig` and one `SharedTokenConfig`.
3. The auth client table key becomes both the whitelisted `client_id` and the
   minimal `shared_token_id`.
4. The TOML `shared_token` value is stored as
   `SharedTokenSecretRef::InlinePlaceholder` for PoC comparison.
5. `protocol` / `net-core` decode an inbound `AuthRequest`.
6. `ServerAuthHandlerBoundary` receives the decoded `AuthRequest` and produces
   `ServerAuthCheck`.
7. `ServerAuthConfigInputBoundary` receives `ServerAuthCheck` plus
   `ServerAuthConfig`.
8. The boundary converts whitelist and token configuration into
   `ServerAuthCheckInput`, which is the input shape for future auth decision
   logic.
9. The minimal auth decision boundary consumes `ServerAuthCheckInput` and
   returns `ServerAuthDecision`.
10. Source registration, external secret resolution, and UDP sends remain in
    later stages.

Responsibility split:

- `config`
  - Owns the server auth setting shape: allowed client entries and shared token
    references.
  - Owns minimal TOML loading from `[auth.clients.<client_id>]`.
  - Maps TOML client entries into typed `ServerAuthConfig`.
  - Does not resolve environment variables, secret stores, or other external
    secret references.
  - Does not decode packets, inspect `AuthRequest`, verify presented tokens, or
    decide authentication.
- server auth handler
  - Receives decode済み `AuthRequest` from server routing.
  - Preserves source metadata and request fields as `ServerAuthCheck`.
  - Does not read files, resolve secrets, or decide allow / reject.
- auth check input boundary
  - Combines `ServerAuthCheck` with `ServerAuthConfig`.
  - Converts configured client IDs and token references into server-side check
    input types.
  - Does not compare the presented token with configured token material.
- auth decision
  - Owns the minimal accepted/rejected result generation.
  - Checks the prepared client whitelist and token input.
  - Does not update authenticated-client state or send responses.

Current code reflects this with `stream-sync-config::ServerAuthConfig`,
`AllowedClientConfig`, `SharedTokenConfig`, `ServerAuthConfigBoundary`, and
`apps/server::ServerAuthConfigInputBoundary`. The config boundary can read the
minimal auth portion of `configs/examples/server.example.toml` and produce typed
auth config. Minimal auth decision logic is implemented separately in
`apps/server::ServerAuthDecisionBoundary`. External secret resolution,
authenticated source state, and UDP sending remain unimplemented.

---

## Server Auth Decision Boundary

PoC / MVP initial implementation now includes the smallest server auth decision
step. It consumes already-prepared auth input and produces a
`ServerAuthDecision` for the existing response boundary.

Flow:

1. `ServerAuthConfigInputBoundary` produces `ServerAuthCheckInput`.
2. `ServerAuthDecisionBoundary` looks up the requested `client_id` in
   `allowed_clients`.
3. If no entry exists, it returns rejected `ServerAuthDecision` with
   `UnknownClient`.
4. If the client entry exists, it finds the configured shared token entry by
   `shared_token_id`.
5. If the token entry is missing, or if the token material is only an unresolved
   external reference, it returns rejected `ServerAuthDecision` with
   `InternalError`.
6. If the token entry contains inline placeholder token material, it compares
   the presented `shared_token` with that placeholder.
7. Matching token returns accepted `ServerAuthDecision`; mismatch returns
   rejected `ServerAuthDecision` with `InvalidToken`.

Responsibility split:

- auth config input boundary
  - Carries decoded request fields plus configured whitelist/token references.
  - Does not decide accepted/rejected.
- auth decision boundary
  - Owns minimal `client_id` lookup and inline placeholder token comparison.
  - Produces `ServerAuthDecision` with `Ok`, `UnknownClient`, `InvalidToken`,
    or `InternalError`.
  - Does not read TOML, resolve environment variables, register authenticated
    sources, build `AuthResponse`, enqueue packets, or send UDP.
- response boundary
  - Converts `ServerAuthDecision` into `ProtocolMessage::AuthResponse`.
  - Does not repeat auth checks.

Current code reflects this with `apps/server::ServerAuthDecisionBoundary`. This
is a minimal PoC decision only; real config loading, real secret resolution,
authenticated source management, JSON Lines output, and UDP sending remain
future tasks.

---

## Server Auth Log Handoff Boundary

PoC / MVP initial implementation keeps auth result logging as a typed handoff
only. The auth decision layer decides success / failure, and the auth log
handoff boundary preserves the context needed by a future JSON Lines log layer.
It does not write log files.

Flow:

1. `ServerAuthConfigInputBoundary` prepares `ServerAuthCheckInput` from decoded
   `AuthRequest` plus auth config.
2. `ServerAuthDecisionBoundary` returns `ServerAuthDecision`.
3. `ServerAuthDecision` carries success / failure, `reason_code`,
   `client_id`, `run_id`, source endpoint, optional `app_version`,
   `protocol_version`, optional message, server time, and expected protocol
   version.
4. `ServerAuthLogHandoffBoundary` receives the decision by reference.
5. The boundary converts it into `ServerAuthLogInput` for the future log layer.
6. The log input preserves whether the result is success or failure and keeps
   the original auth reason without formatting or writing JSON Lines.
7. `ServerAuthFlowStep` produces this log handoff alongside the auth response
   queue handoff.

Responsibility split:

- auth flow
  - Orchestrates decoded auth route, config input, decision, log handoff,
    response generation, and queue handoff.
  - Does not write logs or run UDP sockets.
- auth decision
  - Owns the minimal accepted / rejected result.
  - Preserves auth context for downstream handoff.
  - Does not decide log schema or output destination.
- auth log handoff
  - Converts `ServerAuthDecision` into `ServerAuthLogInput`.
  - Keeps `client_id`, `run_id`, source, optional `app_version`,
    `protocol_version`, success / failure, reason code, and optional message.
  - Does not emit JSON Lines, update metrics, mutate authenticated state, or
    send packets.
- log layer
  - Future owner of JSON Lines formatting and output.
  - Will consume typed auth log input and apply the final log schema.

Current code reflects this with `apps/server::ServerAuthLogHandoffBoundary`,
`ServerAuthLogInput`, and `ServerAuthLogOutcome`. `ServerAuthFlowOutcome`
now carries `auth_log_input` so the auth result can be handed to a future log
layer without losing success / failure reason or auth context. JSON Lines
output, metrics updates, UDP socket I/O, and state persistence remain
unimplemented.

---

## Auth JSON Lines Event Schema Boundary

PoC / MVP initial implementation keeps auth result logging split into typed
auth log handoff, JSON Lines event schema input, and the future log writer. The
current code can build the event input shape, but it does not serialize JSON or
write log files.

Flow:

1. `ServerAuthDecisionBoundary` returns accepted / rejected
   `ServerAuthDecision`.
2. `ServerAuthLogHandoffBoundary` converts it into `ServerAuthLogInput`.
3. `ServerAuthJsonLogEventBoundary` converts `ServerAuthLogInput` plus an
   explicit log timestamp into `ServerAuthJsonLogEventInput`.
4. The future JSON Lines writer will serialize that event input as one log
   record.

Event schema:

| Field | Type | Success / failure policy |
| --- | --- | --- |
| `event_name` | string | Always `server.auth_result`. |
| `run_id` | `RunId` | Common field. Preserved from decoded auth request. |
| `client_id` | `ClientId` | Common field. Preserved from decoded auth request. |
| `source` | endpoint | Common field. Preserved from packet source metadata. |
| `accepted` | bool | Common field. `true` for success, `false` for failure. |
| `reason_code` | `AuthResponseReasonCode` | Common field. `Ok` for success; rejection reason for failure. |
| `message` | optional string | Present mainly for failure detail; normally absent for success. |
| `app_version` | optional `AppVersion` | Common context from `AuthRequest` when available. |
| `protocol_version` | `ProtocolVersion` | Common context from `AuthRequest`. |
| `timestamp` | `TimestampMicros` | Common log timestamp supplied by the caller / future log layer. |
| `expected_protocol_version` | optional `ProtocolVersion` | Failure-only detail for protocol mismatch style rejections. |

Responsibility split:

- auth flow
  - Produces auth decisions and auth log handoff input.
  - Does not serialize or write log records.
- auth log handoff
  - Preserves auth context and success / failure reason in a server-owned typed
    input.
  - Does not decide final JSON Lines field names.
- JSON Lines event schema boundary
  - Maps typed auth log input to the auth result event schema.
  - Adds the explicit log `timestamp`.
  - Does not perform JSON serialization, file I/O, metrics, UDP I/O, or auth
    state mutation.
- log writer
  - Future owner of JSON Lines serialization and output.

Current code reflects this with `apps/server::ServerAuthJsonLogEventBoundary`
and `ServerAuthJsonLogEventInput`. These are event-schema inputs only; actual
JSON Lines output remains unimplemented.

---

## Server Auth Flow Step

PoC / MVP initial implementation connects the existing auth boundaries inside
the server process. This step starts from a decoded auth route and ends at the
outbound queue handoff shape. It does not perform socket I/O or update
authenticated source state.

Flow:

1. `ServerInboundRouter` returns `ServerInboundRoute::AuthRequest` with decoded
   `AuthRequest` and source metadata.
2. `ServerAuthFlowStep` calls `ServerAuthHandlerBoundary` to convert the route
   into `ServerAuthCheck`.
3. `ServerAuthFlowStep` calls `ServerAuthConfigInputBoundary` with
   `ServerAuthCheck` and `ServerAuthConfig` to produce `ServerAuthCheckInput`.
4. `ServerAuthFlowStep` calls `ServerAuthDecisionBoundary` to produce
   `ServerAuthDecision`.
5. `ServerAuthFlowStep` calls `ServerAuthLogHandoffBoundary` to convert the
   decision into `ServerAuthLogInput`.
6. `ServerAuthFlowStep` calls `ServerAuthResponseBoundary` to convert the
   decision into `ServerOutboundAuthResponse`.
7. `ServerAuthFlowStep` calls `ServerOutboundQueueBoundary` to hand the
   response to the outbound queue as `OutboundQueueItem`.
8. Later net send code may encode the queued `ProtocolMessage::AuthResponse`
   and later socket code may send the bytes.

Responsibility split:

- auth flow step
  - Orchestrates existing server boundaries in order.
  - Returns the decision, auth log input, typed outbound response, and queue
    handoff item for inspection by future server code.
  - Does not load config from disk, resolve secrets, register authenticated
    sources, write logs, run a queue, encode bytes, or send UDP.
- auth decision boundary
  - Produces accepted/rejected `ServerAuthDecision`.
- auth log handoff boundary
  - Converts `ServerAuthDecision` into typed log input.
  - Does not write JSON Lines.
- response boundary
  - Converts `ServerAuthDecision` into typed `AuthResponse`.
- outbound queue boundary
  - Produces the `OutboundQueueItem` handoff shape only.

Current code reflects this with `apps/server::ServerAuthFlowStep` and
`ServerAuthFlowOutcome`. This is the first connected server auth path from
decoded request to auth log handoff and outbound queue item.

---

## Authenticated Sender Registry Boundary

PoC / MVP initial implementation separates accepted auth decisions from later
packet acceptance checks. The authenticated sender registry is the server-side
boundary that records which UDP source endpoint is allowed to send packets for
which `client_id`.

Flow:

1. `ServerAuthFlowStep` receives `ServerAuthDecision` from
   `ServerAuthDecisionBoundary`.
2. If `ServerAuthDecision.accepted = true`,
   `AuthenticatedSenderRegistryBoundary` creates
   `AuthenticatedSenderRegistration`.
3. The registration carries `client_id`, source `PacketSource`, `run_id`,
   `protocol_version`, and optional registration timestamp.
4. The registry stores a binding from `client_id` to the authenticated source
   endpoint.
5. Later heartbeat / video frame receive paths will check the packet's
   `client_id` and source endpoint against this registry before accepting the
   packet.
6. If no matching `client_id` exists, or if the endpoint does not match, the
   later packet is a reject/drop candidate.
7. Timeout, expiration, revocation, and reauthentication policy are design
   placeholders only at this stage.

Responsibility split:

- server auth flow
  - Produces `ServerAuthDecision`.
  - On accepted decisions, creates the registry registration handoff.
  - Still builds `AuthResponse` and outbound queue handoff separately.
  - Does not run UDP sockets, heartbeat timeout, revocation, or persistence.
- authenticated sender registry
  - Owns the in-memory mapping of `client_id` to source endpoint.
  - Provides a minimal lookup for later packet acceptance checks.
  - Does not decode packets, verify tokens, send responses, persist state, or
    enforce timeout / reauthentication.
- receive loop / later handlers
  - Decode incoming packets and route them by message type.
  - Before accepting heartbeat or video frame data, consult the authenticated
    sender registry using `client_id` plus source endpoint.
  - Own future drop / log behavior for unauthenticated or mismatched packets.

Current code reflects this with `apps/server::AuthenticatedSenderRegistry`,
`AuthenticatedSenderRegistration`, `AuthenticatedSenderRegistryBoundary`, and
`AuthenticatedSenderCheck`. This is an in-memory boundary shape only. It does
not implement real state persistence, timeout, revocation, reauthentication,
heartbeat handling, video frame handling, or UDP socket I/O.

---

## Packet Acceptance / Rejection Boundary

PoC / MVP initial implementation adds a gate between server routing and later
packet handlers. The gate decides whether a decoded, client-scoped packet may
reach its handler by consulting the authenticated sender registry.

Flow:

1. The receive loop decodes packet bytes and preserves source endpoint metadata.
2. `ServerInboundRouter` classifies the decoded message as `AuthRequest`,
   `Heartbeat`, `VideoFrame`, or unsupported.
3. `AuthRequest` is allowed to reach the auth flow before registry lookup,
   because the registry is populated only after auth success.
4. After auth success, `AuthenticatedSenderRegistry` contains accepted
   `client_id` to endpoint bindings.
5. For later `Heartbeat` and `VideoFrame` routes,
   `PacketAcceptanceGateBoundary` checks decoded `client_id` plus source
   endpoint against the registry before handler execution.
6. If the source endpoint has no authenticated binding, the gate returns
   `UnauthenticatedSource`.
7. If the endpoint is authenticated but the decoded `client_id` is unknown to
   the registry, the gate returns `UnknownClient`.
8. If the decoded `client_id` is registered but the endpoint differs, the gate
   returns `EndpointMismatch`.
9. The current gate returns a decision only. Future receive code will decide how
   to drop and log rejected packets.

Responsibility split:

- receive loop
  - Owns raw packet receive, decode call order, source metadata preservation,
    and routing into server-side boundaries.
  - Future owner of applying the gate result to real packet drop behavior.
  - Does not own registry state or handler business logic.
- authenticated sender registry
  - Owns the accepted `client_id` to endpoint mapping.
  - Provides lookup only.
  - Does not drop packets or emit logs.
- packet acceptance gate
  - Owns early accept / reject decisions for client-scoped decoded routes.
  - Distinguishes unauthenticated source, unknown client, and endpoint mismatch.
  - Does not run heartbeat handling, video frame handling, UDP socket I/O, or
    log output.
- handler
  - Receives only accepted routes in the future flow.
  - Owns heartbeat and video frame business behavior after acceptance.

Current code reflects this with
`apps/server::PacketAcceptanceGateBoundary`,
`PacketAcceptanceDecision`, `PacketAcceptanceRejection`, and
`PacketAcceptanceRejectReason`. `ServerReceiveLoopStep` now has a connected
gate path that returns `ServerReceiveLoopGateOutcome`: accepted routes are the
only ones eligible for future handler execution, while decode errors and gate
rejections are separated for future drop / log handling. This is still a
boundary helper only; actual packet drop execution and JSON Lines logging
remain future tasks.

---

## Receive Rejection Drop / Log Handoff Boundary

PoC / MVP initial implementation keeps packet rejection handling split into
decision, drop input, and log input. The receive loop and packet acceptance gate
only decide that a packet must not reach a handler. They do not execute the
drop and do not write JSON Lines logs.

Flow:

1. `ServerReceiveLoopStep` decodes packet bytes and routes decoded messages.
2. Decode failures become `ServerReceiveLoopGateRejection::Decode`.
3. Gate failures become `ServerReceiveLoopGateRejection::Acceptance`.
4. `ServerRejectionDropLogHandoffBoundary` receives the rejection decision.
5. The boundary converts it into `ServerRejectionDropLogInput`.
6. `ServerRejectionDropLogInput.drop_input` is the future packet drop layer
   input.
7. `ServerRejectionDropLogInput.log_input` is the future receive log layer
   input.
8. Both inputs preserve the same reason so `UnauthenticatedSource`,
   `UnknownClient`, `EndpointMismatch`, and decode error rejections remain
   distinguishable.

Responsibility split:

- receive loop
  - Owns decode and route call order.
  - Produces decode rejection decisions for malformed packet, unsupported
    protocol version, or unsupported inbound payload.
  - Does not drop packets or emit logs.
- packet acceptance gate
  - Produces acceptance rejection decisions after registry lookup.
  - Preserves `message_type`, optional `client_id`, source endpoint, and
    `PacketAcceptanceRejectReason`.
  - Does not drop packets or emit logs.
- drop handoff boundary
  - Converts receive / gate rejection decisions into typed drop and log inputs.
  - Preserves the rejection reason without deciding final log formatting or
    metrics policy.
- drop layer
  - Future owner of the actual packet discard behavior.
  - Does not exist as runtime code in the current step.
- log layer
  - Future owner of JSON Lines receive rejection events.
  - Will use the typed reason to include source, message type, client_id, and
    decode error context where available.

Current code reflects this with
`apps/server::ServerRejectionDropLogHandoffBoundary`,
`ServerRejectionDropLogInput`, `ServerPacketDropInput`,
`ServerPacketLogInput`, and `ServerRejectionHandoffReason`. These are typed
handoff placeholders only. Packet drop execution, log output, heartbeat
handling, and video frame handling remain unimplemented; UDP socket I/O is
limited to the one-datagram adapter described above.

---

## Receive Rejection JSON Lines Event Schema Boundary

Receive rejection logs use a typed event input before any JSON Lines writer is
introduced. The boundary converts `ServerPacketLogInput` into a stable field
set for future structured logging, while preserving the exact rejection reason
and detail.

Flow:

1. Receive loop / packet acceptance gate produces a rejection decision.
2. `ServerRejectionDropLogHandoffBoundary` converts the rejection into
   `ServerPacketLogInput`.
3. `ServerReceiveRejectionJsonLogEventBoundary` converts that log handoff input
   into `ServerReceiveRejectionJsonLogEventInput`.
4. A future JSON Lines writer serializes the event input as one receive
   rejection log record.
5. The current boundary does not write JSON Lines, drop packets, update
   metrics, or call sockets.

Event schema:

| Field | Type | Notes |
| --- | --- | --- |
| `event_name` | string | Always `server.receive_rejection`. |
| `run_id` | optional `RunId` | Included in the schema for correlation. Current decode / gate rejections do not always know it, so it is `None` until a later boundary carries it. |
| `client_id` | optional `ClientId` | Present when the packet was decoded far enough or the gate extracted it. Decode errors may leave it absent. |
| `source` | `PacketSource` | UDP source endpoint captured before decode / gate processing. |
| `message_type` | optional `MessageType` | Present for packet acceptance rejections; absent for decode failures that cannot identify a message type. |
| `rejection_reason` | enum | `DecodeError`, `UnauthenticatedSource`, `UnknownClient`, or `EndpointMismatch`. |
| `detail` | enum/object | Decode detail keeps `ServerDecodeErrorAction` and `ProtocolError`; acceptance detail keeps `PacketAcceptanceRejectReason`. |
| `timestamp` | `TimestampMicros` | Server-side event timestamp supplied by the caller. |

Responsibility split:

- receive loop
  - Owns raw packet intake, decode handoff, and gate invocation.
  - Produces rejection decisions.
  - Does not format or write JSON Lines.
- packet acceptance gate
  - Owns registry lookup and acceptance rejection reason selection.
  - Does not know log event field names.
- rejection handoff boundary
  - Preserves source, message type, optional client id, decode error, and
    rejection reason for downstream layers.
  - Does not choose a JSON Lines event name.
- JSON Lines event schema boundary
  - Owns the receive rejection event field set and maps typed handoff reasons
    to log-event reasons.
  - Does not serialize JSON Lines or write to disk/stdout.
- log writer
  - Future owner of JSON Lines serialization, sinks, rotation, and flushing.

Current code reflects this with
`apps/server::ServerReceiveRejectionJsonLogEventBoundary`,
`ServerReceiveRejectionJsonLogEventInput`,
`ServerReceiveRejectionReason`, and `ServerReceiveRejectionDetail`. JSON Lines
output remains unimplemented.

---

## Server AuthResponse Boundary

PoC / MVP initial implementation separates the future auth decision from
response generation and network sending.

Flow:

1. The auth flow step or auth decision layer returns a `ServerAuthDecision`.
2. `ServerAuthDecision` carries the source address, `client_id`, `run_id`,
   `protocol_version`, accepted/rejected result, reason code, optional message,
   optional server time, and optional expected protocol version.
3. `ServerAuthResponseBoundary` receives the decision.
4. `ServerAuthResponseBoundary` builds `ProtocolMessage::AuthResponse` from the
   decision.
5. The boundary returns `ServerOutboundAuthResponse`, which contains the
   destination `PacketSource` and typed protocol message.
6. `ServerOutboundQueueBoundary` can hand the typed response to the outbound
   queue as `OutboundQueueItem`.
7. A future net send layer will wire-encode the message and send it over UDP.

Responsibility split:

- `protocol`
  - Owns the `AuthResponse` message shape and `AuthResponseReasonCode`.
  - Does not decide authentication, update server state, encode packets, or
    send sockets.
- server auth handler / auth decision layer
  - Owns the minimal `client_id` and inline placeholder token checks.
  - Future owner of fuller `protocol_version` and `app_version` policy.
  - Returns the auth decision data used to construct an `AuthResponse`.
  - Does not perform wire encoding or socket sending.
- response boundary
  - Converts `ServerAuthDecision` into `ProtocolMessage::AuthResponse`.
  - Preserves the destination source metadata for the send layer.
  - Does not perform real auth checks, server state updates, wire encoding, or
    UDP sends.
- net send layer
  - Future owner of outbound packet encoding and socket transmission.
  - Will receive typed outbound intent from server-side response boundaries.

Current code reflects this with `ServerAuthDecision`,
`ServerAuthResponseBoundary`, and `ServerOutboundAuthResponse` placeholders in
`apps/server`. Minimal success/failure judgement now exists in
`ServerAuthDecisionBoundary`, and `ServerAuthFlowStep` connects the decision to
`AuthResponse` queue handoff. Whitelist loading, external secret resolution,
authenticated source registration, and UDP send remain unimplemented.

---

## Server Outbound Packet / Queue Boundary

PoC / MVP initial implementation keeps outbound sending split into typed
message generation, queue handoff, wire encode, and socket send.

Flow:

1. Server response boundaries create outbound typed messages such as
   `ProtocolMessage::AuthResponse`.
2. Server ack boundaries can create other outbound typed messages such as
   `ProtocolMessage::HeartbeatAck` using the same send-layer shape.
   Future notification paths can do the same for messages such as
   `ServerNotice`.
3. The server response boundary attaches destination metadata to the
   `ProtocolMessage`.
4. The server outbound queue boundary converts the server-specific response
   handoff into the generic `net-core` `OutboundPacket`.
5. `net-core` `OutboundPacketQueueBoundary` returns an `OutboundQueueItem` as
   the handoff shape for a future queue.
6. A later queue implementation will decide buffering, ordering, backpressure,
   logging, and retry policy if needed.
7. A later send implementation will wire-encode the `ProtocolMessage` and send
   the resulting bytes through UDP.

Responsibility split:

- server handlers
  - Decide that a response or notice should be sent.
  - Do not encode bytes or write sockets.
- response boundary
  - Builds typed `ProtocolMessage` values and preserves destination metadata.
  - Does not own generic queue behavior, encode, or socket send.
- net send layer boundary
  - Receives `ProtocolMessage` plus destination information as `OutboundPacket`.
  - Provides a queue handoff item without implementing a real queue.
  - Does not encode wire bytes or call UDP sockets in this step.
- socket send layer
  - Future owner of byte encode result transmission over UDP.
  - Will handle send errors and runtime/socket details.

Current code reflects this with `net-core::OutboundPacket`,
`net-core::OutboundQueueItem`, `net-core::OutboundPacketQueueBoundary`, and
`apps/server::ServerOutboundQueueBoundary`. `apps/server` currently has typed
handoff placeholders for `AuthResponse` and `HeartbeatAck`. These are carrier
and handoff types only. One encoded datagram can now be sent through
`UdpSocketIoBoundary`; queue implementation, async runtime, retry,
fragmentation, encryption, and full send orchestration remain unimplemented.

Outbound queue minimal processing policy:

1. `ServerOutboundQueueBoundary` receives a typed server outbound response or
   ack and converts it into `net-core::OutboundQueueItem`.
2. The outbound queue owns the item while it is waiting to be sent. In the
   current code this is represented only as one-item state, not as a real
   collection.
3. When the next item is ready, the queue hands `OutboundQueueItem` to the net
   send layer.
4. The net send layer performs protocol encode after queue handoff. The queue
   does not call `protocol::MessageEncoder` directly.
5. After encode success, the net send layer owns `EncodedOutboundPacket` and can
   pass encoded bytes plus destination to the future socket send layer.
6. After socket send success or failure, future code may notify the queue of a
   terminal state or retry candidate, but retry execution is not part of the
   current queue boundary.

Queue item states:

| State | Owner / meaning | Current implementation |
| --- | --- | --- |
| `Queued` | Queue has accepted the item and is holding it for later send. | Placeholder only. |
| `ReadyForEncode` | Queue has selected the item for the send layer. | Named state for future queue policy. |
| `Encoded` | Net send layer encoded the item into bytes. | Placeholder mark only. |
| `Sent` | Future socket send layer reported success. | Placeholder mark only. |
| `Dropped` | Item should not be retried by the current policy. | Placeholder mark only. |

Encode / send responsibility boundaries:

- encode前
  - server creates `ProtocolMessage` plus destination metadata.
  - `ServerOutboundQueueBoundary` produces `OutboundQueueItem`.
  - outbound queue may hold and later select the item.
- encode後
  - net send layer owns `EncodedOutboundPacket`.
  - outbound queue no longer inspects typed message bytes or encoded payload.
  - send log context may record encoded length and message metadata.
- send後
  - socket send layer reports success or categorized failure.
  - outbound queue may later consume retry / drop hints, but current code only
    names terminal states and does not retry.

Current code reflects the one-item lifecycle with
`net-core::QueuedOutboundItem`, `net-core::OutboundQueueItemState`,
`net-core::OutboundQueueSendHandoff`, and
`net-core::OutboundQueueLifecycleBoundary`. These types do not implement
buffering, ordering, async wakeups, retry, encode, UDP socket send, or
backpressure.

AuthResponse-specific encode boundary:

1. Future auth decision logic returns `ServerAuthDecision`.
2. `ServerAuthResponseBoundary` converts the decision into
   `ProtocolMessage::AuthResponse`.
3. `ServerOutboundQueueBoundary` hands the typed response and destination to the
   generic net send layer as `OutboundPacket`.
4. The typed `AuthResponse` is the encode input. The response boundary does not
   write fixed headers, payload bytes, or UDP packets.
5. Future protocol encode code will use the `AuthResponse` payload layout
   defined in `docs/architecture/protocol.md`; future socket code will send the
   encoded bytes.

HeartbeatAck-specific encode input boundary:

1. Future heartbeat handling decides the ack timestamps after receiving
   `ProtocolMessage::Heartbeat`.
2. `ServerHeartbeatAckBoundary` converts the already-decided fields into
   `ProtocolMessage::HeartbeatAck`.
3. `ServerOutboundQueueBoundary` hands the typed ack and destination to the
   generic net send layer as `OutboundPacket`.
4. The typed `HeartbeatAck` is the encode input. The ack boundary does
   not calculate heartbeat state, write fixed headers, write payload bytes, or
   send UDP packets.
5. Protocol encode code uses the `HeartbeatAck` payload layout defined in
   `docs/architecture/protocol.md`; future socket code will send the encoded
   bytes.

---

## Net Send Layer / Protocol Encoder Boundary

PoC / MVP initial implementation keeps the send path split at the point where
typed outbound messages are handed to the protocol encoder. This step fixes the
boundary only; it does not implement real wire encoding, a queue runtime, or UDP
socket sends.

Flow:

1. Server response boundaries such as `ServerAuthResponseBoundary`, and future
   notification paths such as `HeartbeatAck` or `ServerNotice`, create typed
   `ProtocolMessage` values.
2. The response path attaches destination metadata and hands the message to the
   net send layer as `OutboundPacket` / `OutboundQueueItem`.
3. The net send layer receives the destination and `ProtocolMessage`, then
   prepares an `OutboundEncodeRequest` with `EncodeContext`.
4. The protocol encoder boundary receives `ProtocolMessage` plus
   `EncodeContext`.
5. The protocol encoder boundary is responsible for converting the message into
   one UDP packet buffer: fixed header followed by payload bytes.
6. The net send layer keeps the encoded bytes paired with destination metadata
   as `EncodedOutboundPacket`.
7. A future socket send layer receives encoded bytes plus destination and writes
   them to UDP.

Responsibility split:

- server handlers / response boundary
  - Decide that a response or notice should be sent.
  - Build typed `ProtocolMessage` values and destination metadata.
  - Do not encode fixed headers, payload bytes, or send sockets.
- net send layer
  - Owns the generic outbound carrier and handoff shape.
  - Calls the protocol encoder boundary with `ProtocolMessage` and
    `EncodeContext`.
  - Preserves destination metadata across encode.
  - Does not implement message-specific encode rules or UDP socket sends.
- protocol encoder
  - Owns future conversion from `ProtocolMessage` to fixed header plus payload
    bytes.
  - Uses `protocol_version`, `message_type`, and payload layout rules from the
    protocol crate.
  - Does not own destination metadata, queue policy, retry, or socket errors.
- socket send layer
  - Future owner of sending encoded bytes to the destination over UDP.
  - Does not inspect typed protocol messages.

Current code reflects this with `net-core::OutboundEncodeRequest`,
`net-core::EncodedOutboundPacket`, `net-core::OutboundPacketEncoderBoundary`,
`net-core::NetEncodeError`, and
`protocol::ProtocolMessageEncoderBoundary`. The protocol encoder placeholder
currently encodes `AuthResponse`, `HeartbeatAck`, and `VideoFrame`, and returns
`EncodeNotImplemented` for other outbound messages. `UdpSocketIoBoundary` can
send an already encoded packet; queue processing and continuous send
orchestration remain unimplemented.

---

## Send Error / Log Event Boundary

PoC / MVP initial implementation keeps send error handling as classification
and structured log context only. Minimal UDP `send_to` exists for already
encoded packets, but this boundary does not implement retry, queue mutation,
JSON Lines output, or async runtime behavior.

Send path checkpoints:

1. `server` response / ack boundaries create typed outbound messages and
   destination metadata.
2. `outbound queue` boundary hands `OutboundQueueItem` to the net send layer
   without executing a real queue.
3. `net send layer` extracts log context from the typed message before encode:
   `run_id`, optional `client_id`, destination, and `message_type`.
4. `protocol encoder` converts supported messages into fixed header + payload
   bytes.
5. After encode success, the net send layer can log `encode_succeeded` with the
   encoded byte length before handing bytes to the future socket send layer.
6. Before socket send, the net send layer can reject local send-preparation
   problems such as missing destination metadata or packet size policy.
7. The future socket send layer will report socket-level errors after a real
   `send_to` attempt.

Error classification:

| Stage | Error kind | Initial disposition | Notes |
| --- | --- | --- | --- |
| encode | `EncodeFailed` | drop candidate | Message cannot be represented by current protocol encoder. This is a code / compatibility issue, not a UDP retry issue. |
| before socket send | `DestinationUnavailable` | drop candidate | Required destination metadata is absent or unusable before socket call. |
| before socket send | `PacketTooLarge` | drop candidate | Encoded datagram violates current size policy. Fragmentation is not implemented yet. |
| socket send | `SocketWouldBlock` | retry candidate | Future nonblocking socket path may retry or requeue later. |
| socket send | `SocketInterrupted` | retry candidate | Future socket path may retry immediately or requeue. |
| socket send | `ConnectionRefused` | warning candidate | UDP may surface ICMP/refused depending on platform; log with context and let higher-level state decide later. |
| socket send | `NetworkUnreachable` | warning candidate | Log with destination; future policy may mark client degraded or disconnected. |
| socket send | `PermissionDenied` | warning candidate | Usually configuration / OS policy issue; log clearly. |
| socket send | `OtherSocketError` | warning candidate | Preserve error class for future logging and diagnostics. |

Log event field policy:

- `run_id`
  - Extract from `ProtocolMessage` whenever present.
  - Required for `AuthRequest`, `AuthResponse`, `Heartbeat`, `HeartbeatAck`,
    `VideoFrame`, `ClientStats`, and `ServerNotice`.
- `client_id`
  - Extract from `ProtocolMessage` whenever present.
  - Present for client-scoped messages such as `AuthResponse` and
    `HeartbeatAck`; absent for server-wide notices.
- `destination`
  - Use `PacketDestination.address`.
  - Include it in every send event, including encode failures, because the
    destination remains attached before and after encode.
- `message_type`
  - Use `ProtocolMessage::message_type()` before encode.
  - Include it even when encode fails.
- `encoded_len`
  - Include after encode success and on socket send failures.
  - Omit for encode failures that produced no packet buffer.
- `stage`, `failure`, `disposition`
  - Include on failures so logs can distinguish encode, pre-socket, and socket
    send problems.

Responsibility split:

- `server`
  - Decides that a response / ack / notice should be sent.
  - Builds typed `ProtocolMessage` values and destination metadata.
  - Does not classify socket errors, retry, or write sockets.
- `outbound queue`
  - Future owner of buffering, ordering, and backpressure.
  - May use disposition hints later, but no queue behavior is implemented now.
- `net send layer`
  - Owns send log context extraction and send failure classification.
  - Keeps `run_id`, `client_id`, destination, and `message_type` attached to
    encode and future socket-send results.
  - Does not execute retry or socket I/O in this step.
- `socket send`
  - Future owner of calling UDP `send_to` and mapping OS/socket errors into
    the send failure categories.
  - Does not inspect typed protocol messages.

Current code reflects this with `net-core::OutboundSendLogContext`,
`net-core::SendLogStage`, `net-core::SendFailureKind`,
`net-core::SendFailureDisposition`, and `net-core::SendLogEvent`. These are
classification and structured event placeholder types only.
