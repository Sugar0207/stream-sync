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
6. Accepted decisions register the source in the in-memory authenticated
   sender registry through the existing registry boundary before the response
   is encoded and sent.
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
  - Does not mutate registry state, encode, or send bytes.
- authenticated sender registry
  - Stores the accepted `client_id` to source endpoint binding in memory when
    the PoC step applies the registration handoff.
  - Does not persist state, expire entries, or perform reauthentication.
- net send / protocol encoder
  - Converts the queued typed `AuthResponse` into fixed header + payload bytes.
- socket send
  - Sends the already encoded datagram once.

Current code reflects this with `apps/server::ServerAuthResponsePocStep`,
`ServerAuthResponsePocOutcome`, and `ServerAuthResponsePocError`. Continuous
looping, async runtime integration, real queue scheduling, retry,
fragmentation, encryption, JSON Lines output, heartbeat handling, video frame
handling, timeout, revocation, and registry persistence remain unimplemented.

### 5.8 AuthResponse PoC startup config entry

The auth response PoC startup config entry connects the example server TOML to
the one-shot socket/auth step. This is the smallest runnable server-side entry
for the auth response round trip.

Flow:

1. The caller passes a server TOML path to the PoC launcher.
2. `ServerAuthResponsePocLauncher` reads the TOML file.
3. The launcher extracts `[server].bind_host`, `[server].bind_port`, and
   `[session].protocol_version` for socket bind and protocol validation.
4. The same TOML content is passed to `ServerAuthConfigBoundary` to build
   `ServerAuthConfig` from the allowed clients and shared token placeholders.
5. The launcher resolves `bind_host + bind_port` into a `SocketAddr`.
6. The launcher binds one synchronous UDP socket through `UdpSocketIoBoundary`.
7. The launcher initializes an empty `AuthenticatedSenderRegistry`.
8. The launcher calls `ServerAuthResponsePocStep::run_one`.
9. `run_one` waits for one packet, handles one accepted `AuthRequest`, encodes
   one `AuthResponse`, and sends one UDP datagram back to the request source.
10. The one-shot CLI writes the resulting auth success / failure event to
    stderr through `ServerAuthLogOutputBoundary`.

Responsibility split:

- config loading
  - Owns TOML file reading and minimal extraction of bind address and protocol
    version.
  - Uses `ServerAuthConfigBoundary` for auth config shape.
- launcher
  - Owns binding the UDP socket, allocating the receive buffer, creating the
    in-memory registry, and calling the one-shot PoC step.
  - Does not run a loop, write logs, retry, fragment, encrypt, or handle
    heartbeat / video frame packets.
- PoC step
  - Owns one packet of receive -> auth flow -> encode -> send composition.
- binary entry
  - Exposes the launcher through an explicit `--auth-response-poc-once`
    command-line flag.
  - Owns the PoC default auth result log sink: one JSON Lines event to stderr
    after a decision exists.
  - Does not open file sinks, rotate logs, apply retention, run async logging,
    or install process-wide logging.

Current code reflects this with `apps/server::ServerAuthResponsePocLauncher`,
`ServerAuthResponsePocStartupConfig`,
`ServerAuthResponsePocStartupOutcome`,
`ServerAuthResponsePocStartupError`, and
`run_auth_response_poc_once_from_path`. The default server binary still prints
a scaffold message unless the explicit one-shot PoC flag is supplied.

### 5.9 Client AuthRequest one-shot PoC startup entry

The client auth request PoC startup entry connects the example client TOML to a
single UDP `AuthRequest` send. This is the smallest runnable client-side entry
for the server auth response round trip.

Flow:

1. The caller passes a client TOML path to the PoC launcher.
2. `ClientAuthRequestPocLauncher` reads the TOML file.
3. The launcher extracts `[client].server_host`, `[client].server_port`,
   `[client].client_id`, `[client].shared_token`, optional
   `[client].display_name`, `[session].run_id`, `[session].app_version`, and
   `[session].protocol_version`.
4. The launcher resolves `server_host + server_port` into a destination
   `SocketAddr`.
5. The launcher builds `ProtocolMessage::AuthRequest`.
6. `ProtocolMessageEncoderBoundary` converts the typed request into fixed
   header + payload bytes.
7. The launcher binds an ephemeral local synchronous UDP socket.
8. The launcher sends one encoded datagram to the destination.

Responsibility split:

- config loading
  - Owns TOML file reading and minimal extraction of the destination and auth
    request fields.
  - Does not resolve secrets from environment variables or secret stores.
- client PoC launcher
  - Owns destination resolution, `AuthRequest` construction, protocol encode,
    ephemeral UDP bind, and one `send_to`.
  - Does not run a loop, reconnect, send heartbeat / video frames, write JSON
    Lines logs, retry, fragment, or encrypt.
- protocol encoder
  - Owns converting `AuthRequest` to the documented fixed header + payload
    bytes.
  - Does not know socket addresses or connection state.
- socket send
  - Owns sending already encoded bytes to the resolved destination once.

Current code reflects this with `apps/client::ClientAuthRequestPocLauncher`,
`ClientAuthRequestPocStartupConfig`, `ClientAuthRequestPocOutcome`,
`ClientAuthRequestPocError`, and `run_auth_request_poc_once_from_path`. The
default client binary still prints a scaffold message unless the explicit
`--auth-request-poc-once` flag is supplied.

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

## Server Secret Resolution and Token Protection Boundary

PoC keeps inline `shared_token` support so the one-shot auth round trip remains
easy to verify. MVP / production settings should treat inline token material as
a placeholder and prefer a reference such as `shared_token_env`, which points to
an environment variable name. The current implementation resolves
`shared_token_env`; external secret stores and token rotation are planned
boundaries only.

Flow:

1. `ServerAuthConfigBoundary` reads each `[auth.clients.<client_id>]` entry.
2. Exactly one token reference may be set for a client.
3. `shared_token` becomes `SharedTokenSecretRef::InlinePlaceholder`.
4. `shared_token_env` becomes `SharedTokenSecretRef::EnvironmentVariable`.
5. A future secret store config will become
   `SharedTokenSecretRef::SecretStore`, but the current TOML parser does not
   construct it yet.
6. `ServerAuthConfigInputBoundary` carries those references into
   `ServerAuthCheckInput` without resolving them.
7. `ServerSecretResolverBoundary` resolves inline PoC material and
   `shared_token_env` before auth decision.
8. Secret store references are classified as future work and rejected before
   auth decision with a non-secret `InternalError` message.

Token protection rules:

- Do not log raw `shared_token` values from config or `AuthRequest`.
- Do not include token material in JSON Lines auth events, receive rejection
  events, panic messages, or debug-oriented operator messages.
- `SharedTokenSecretRef` debug output redacts inline token material.
- Environment variable names may be recorded as references, but their resolved
  values must stay out of logs and auth responses.
- Secret store ids, secret ids, and version labels are references only; they
  must not contain token material.
- `AuthResponse.message` must describe the failure class, not echo presented or
  expected token material.
- Config owns references only; auth input carries references; auth decision
  compares only resolved or PoC inline material.

Responsibility split:

- config
  - Parses `shared_token` and `shared_token_env` into typed secret references.
  - Defines the future `SecretStore` reference shape.
  - Rejects missing, empty, or conflicting token references.
  - Does not resolve environment variables, secret stores, or validate a
    presented token.
- auth input boundary
  - Moves configured secret references next to decoded request context.
  - Does not reveal, log, or resolve secret material.
- secret resolution boundary
  - Owns reading environment variables in the current MVP path.
  - Future owner of reading secret stores.
  - Produces token material for verification without exposing it to logs.
- auth decision
  - Owns whitelist lookup and token comparison against prepared material.
  - Does not read TOML, environment variables, or external secret stores.
- token rotation boundary
  - Current policy is `DisabledForMvp`: one active token per client.
  - Future manual overlap may allow previous and current tokens during an
    operator-defined window.
  - Does not hot-reload, cache, or compare multiple token materials now.

Current code reflects this with `SharedTokenSecretRef::InlinePlaceholder`,
`SharedTokenSecretRef::EnvironmentVariable`, and
`SharedTokenSecretRef::SecretStore`, plus
`apps/server::ServerSharedTokenSecretResolutionStatus`. The resolver supports
inline PoC values and environment variables; secret store integration remains
unimplemented.

Secret resolver implementation scope:

- In scope for the first real resolver:
  - Receive `ServerSharedTokenAuthInput` values that contain token ids and
    `SharedTokenSecretRef` references.
  - Treat `InlinePlaceholder` as already available PoC token material.
  - Resolve `EnvironmentVariable` by reading exactly the named environment
    variable.
  - Reject missing or empty resolved values before auth decision.
  - Produce redacted-debug `ServerResolvedSharedTokenAuthInput` values for auth
    decision input.
  - Return typed resolution errors that do not include token values.
- Out of scope for the first real resolver:
  - External secret stores.
  - Network calls.
  - Token hashing / KDF / rotation.
  - Caching and hot reload.
  - Deciding whether a client is accepted.
  - Logging resolved token material.

Secret store / rotation policy:

- `SecretStoreSecretRef` carries `store_id`, `secret_id`, and optional
  `version` as future reference metadata only.
- `ServerSecretResolverBoundary::plan_resolution` may report
  `NeedsSecretStore`, but `resolve_token` returns
  `UnsupportedSecretStore` until a provider is selected.
- MVP token rotation is disabled. Each client has one active token reference.
- Future rotation should use explicit operator-driven manual overlap: add a new
  token, accept previous and current token for a bounded window, then remove
  the previous token.
- Rotation must not change the UDP protocol or AuthRequest payload in MVP; the
  client still presents one `shared_token`.
- Automatic hot reload, token caching, token hashing/KDF, provider-specific
  APIs, and background refresh are not part of this step.

Updated responsibility split:

- config
  - Parses TOML into `SharedTokenSecretRef`.
  - Validates only reference presence, emptiness, and conflicts.
  - Does not read environment variables or produce resolved material.
- resolver
  - Owns reference resolution and resolved-token error classification.
  - Produces resolved token material with redacted debug output.
  - Does not inspect `AuthRequest`, check client whitelist, or build
    `AuthResponse`.
- auth input boundary
  - Combines decoded request context, allowed clients, and configured token
    references before resolver execution.
- auth decision
  - Owns whitelist lookup and comparison against already-prepared material.
  - Does not read TOML, environment variables, or secret stores.

Current implementation: `apps/server::ServerSecretResolverBoundary` resolves
PoC inline tokens as already-available material and reads `shared_token_env`
from the named environment variable. Missing, empty, and invalid environment
variables return typed `ServerSecretResolutionError` values without token
material. Secret store references return `UnsupportedSecretStore`.
`ServerAuthFlowStep` resolves configured token references before calling
`ServerAuthDecisionBoundary`; resolver failures become rejected
`ServerAuthDecision` values with `InternalError`. `SharedTokenRotationConfig`
and `ServerSharedTokenRotationBoundary` document disabled MVP rotation and the
future manual-overlap placeholder. Secret stores, hashing, rotation execution,
caching, and hot reload remain unimplemented.

---

## Server Auth Decision Boundary

PoC / MVP initial implementation now includes the smallest server auth decision
step. It consumes already-prepared auth input and produces a
`ServerAuthDecision` for the existing response boundary.

Flow:

1. `ServerAuthConfigInputBoundary` produces `ServerAuthCheckInput`.
2. `ServerSecretResolverBoundary` converts configured token references into
   resolved token material or returns a typed resolution error.
3. `ServerAuthDecisionBoundary` looks up the requested `client_id` in
   `allowed_clients`.
4. If no entry exists, it returns rejected `ServerAuthDecision` with
   `UnknownClient`.
5. If the client entry exists, it finds the configured shared token entry by
   `shared_token_id`.
6. If the token entry is missing, it returns rejected `ServerAuthDecision` with
   `InternalError`.
7. If the token entry contains resolved material from inline PoC config or
   `shared_token_env`, it compares the presented `shared_token` with that
   material.
8. Matching token returns accepted `ServerAuthDecision`; mismatch returns
   rejected `ServerAuthDecision` with `InvalidToken`.

Responsibility split:

- auth config input boundary
  - Carries decoded request fields plus configured whitelist/token references.
  - Does not decide accepted/rejected.
- secret resolver
  - Resolves inline PoC token material and `shared_token_env` before auth
    decision.
  - Produces typed missing / empty / invalid environment variable errors.
  - Does not compare tokens or decide accepted/rejected.
- auth decision boundary
  - Owns minimal `client_id` lookup and resolved token comparison.
  - Produces `ServerAuthDecision` with `Ok`, `UnknownClient`, `InvalidToken`,
    or `InternalError`.
  - Does not read TOML, resolve environment variables, register authenticated
    sources, build `AuthResponse`, enqueue packets, or send UDP.
- response boundary
  - Converts `ServerAuthDecision` into `ProtocolMessage::AuthResponse`.
  - Does not repeat auth checks.

Current code reflects this with `apps/server::ServerAuthFlowStep`,
`ServerSecretResolverBoundary`, and `ServerAuthDecisionBoundary`. This is still
a minimal PoC decision path: secret store integrations, token hashing /
rotation, authenticated source timeout, heartbeat handling, video frame
handling, and async runtime integration remain future tasks.

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
  - Does not choose sinks, retention, metrics, UDP I/O, or auth state mutation.
- log writer
  - Current minimal writer serializes the auth result event to one JSON Lines
    record for an `io::Write` sink.
  - Does not own file opening, rotation, retention, async logging, or
    process-wide logging configuration.

Current code reflects this with `apps/server::ServerAuthJsonLogEventBoundary`
and `ServerAuthJsonLogEventInput`. Minimal JSON Lines output is available
through `ServerAuthLogOutputBoundary` and `ServerAuthJsonLineWriter`. The
one-shot server CLI emits auth result logs to stderr after
`ServerAuthResponsePocStep` returns a decision. This is the current default sink
for the one-shot path only; a future continuous loop should call the same
boundary at its auth decision point without changing the event schema.

---

## Server JSON Lines Writer Connection Scope

Auth result, receive rejection, receive loop, and send error logs share the same
connection shape: typed handoff input -> event schema boundary ->
schema-specific JSON Lines writer -> caller-owned `io::Write` sink.

Current connection scope:

- auth result
  - Handoff: `ServerAuthLogInput`
  - Event schema: `ServerAuthJsonLogEventInput`
  - Writer boundary: `ServerAuthLogOutputBoundary`
  - Writer: `ServerAuthJsonLineWriter`
  - Current sink: one-shot server CLI writes auth result events to stderr after
    `AuthResponse` handling completes; the boundary can also write to any
    caller-provided `io::Write`.
- receive rejection
  - Handoff: `ServerPacketLogInput`
  - Event schema: `ServerReceiveRejectionJsonLogEventInput`
  - Writer boundary: `ServerReceiveRejectionLogOutputBoundary`
  - Writer: `ServerReceiveRejectionJsonLineWriter`
  - Current sink: one-shot server CLI writes rejected receive events to stderr.
- receive loop
  - Handoff: `ServerReceiveLoopLogInput`
  - Event schema: `ServerReceiveLoopJsonLogEventInput`
  - Writer boundary: `ServerReceiveLoopLogOutputBoundary`
  - Writer: `ServerReceiveLoopJsonLineWriter`
  - Current sink: caller-owned `io::Write` only until a continuous receive loop
    exists.
- send error
  - Handoff: `ServerSendErrorLogInput`
  - Event schema: `ServerSendErrorJsonLogEventInput`
  - Writer boundary: `ServerSendErrorLogOutputBoundary`
  - Writer: `ServerSendErrorJsonLineWriter`
  - Current sink: caller-owned `io::Write` only until a send loop exists.
- sink planning
  - Shared config shape: `logging::JsonLinesSinkConfig`
  - Shared plan boundary: `logging::JsonLinesSinkPlanBoundary`
  - Server auth/receive plan boundary:
    `ServerAuthReceiveJsonLinesSinkBoundary`
  - Server receive-loop plan boundary: `ServerReceiveLoopJsonLinesSinkBoundary`
  - Server send-error plan boundary: `ServerSendErrorJsonLinesSinkBoundary`

File sink policy:

- stderr remains the PoC default sink for auth result, receive rejection,
  planned receive loop, and planned send error events.
- file sink configuration is represented as a plan only; it does not open the
  file yet.
- auth result, receive rejection, receive loop, and send error may use separate
  file paths.
- file sinks use append-create semantics when implemented later.
- each event remains one JSON object plus newline.
- rotation, retention, compression, directory creation, and async logging are
  explicitly future work.
- schema-specific writers keep accepting caller-owned `io::Write`; the future
  file sink layer will provide that writer after opening the configured file.

Out of scope for this stage:

- process-wide logger initialization
- actual file opening / directory creation
- log rotation / retention / compression
- buffering and flush policy beyond caller-owned writes
- async logging
- metrics fanout
- replacing schema-specific event writers with a generic logging writer
- moving auth result or send error output into a continuous loop sink before
  that loop exists

This keeps the current log families aligned without forcing a broad logging
infrastructure before the PoC receive/auth path is stable.

Current code reflects the sink planning boundary with
`crates/logging::JsonLinesSinkConfig`, `JsonLinesSinkDestination`,
`JsonLinesFileSinkConfig`, `JsonLinesSinkPlan`, and
`JsonLinesSinkPlanBoundary`, plus
`apps/server::ServerAuthReceiveJsonLinesSinkConfig`,
`ServerAuthReceiveJsonLinesSinkPlan`, and
`ServerAuthReceiveJsonLinesSinkBoundary`,
`ServerReceiveLoopJsonLinesSinkConfig`,
`ServerReceiveLoopJsonLinesSinkPlan`, `ServerReceiveLoopJsonLinesSinkBoundary`,
`ServerSendErrorJsonLinesSinkConfig`, `ServerSendErrorJsonLinesSinkPlan`, and
`ServerSendErrorJsonLinesSinkBoundary`. The current one-shot server CLI still
writes auth/receive events to stderr. Send error output is available only as a
caller-owned writer boundary until a send loop exists; no file is opened by
these boundaries.

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
5. Later heartbeat / video frame receive paths check the packet's
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
`AuthenticatedSenderCheck`. `ServerAuthResponsePocStep` applies accepted
registrations to the in-memory registry on the auth accepted path, and the
packet acceptance gate uses the registry for later `Heartbeat` / `VideoFrame`
routes. It does not implement real state persistence, timeout, revocation,
reauthentication, heartbeat handling, or video frame handling.

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

## Registered Packet Handler Handoff Boundary

After packet acceptance succeeds, heartbeat and video frame handlers need both
the decoded message and the authenticated sender binding. The registered packet
handoff boundary is the bridge from an accepted route to handler input.

Flow:

1. `ServerReceiveLoopStep` decodes packet bytes and routes them into
   `ServerInboundRoute`.
2. `PacketAcceptanceGateBoundary` accepts only packets whose `client_id` and
   source endpoint match `AuthenticatedSenderRegistry`.
3. `ServerRegisteredPacketBoundary` receives the accepted route plus the
   registry.
4. For `Heartbeat`, it produces `ServerRegisteredHeartbeatPacket` containing
   source endpoint, `AuthenticatedSenderEntry`, and decoded `Heartbeat`.
5. For `VideoFrame`, it produces `ServerRegisteredVideoFramePacket` containing
   source endpoint, `AuthenticatedSenderEntry`, and decoded `VideoFrame`.
6. `AuthRequest` and unsupported routes are not client-scoped handler inputs
   for this boundary.
7. The boundary may return the same typed acceptance rejection if a caller
   invokes it with an unregistered or mismatched packet.

Responsibility split:

- receive loop
  - Owns decode, routing, and calling the packet acceptance gate.
  - Does not build heartbeat ack data or enqueue video frames.
- packet acceptance gate
  - Owns accept / reject policy before handler execution.
  - Does not construct handler input.
- authenticated sender registry
  - Owns the `client_id` to endpoint binding and returns the registered sender
    entry.
  - Does not run handler behavior.
- registered packet boundary
  - Attaches the authenticated sender entry to accepted heartbeat / video frame
    routes.
  - Does not update heartbeat state, calculate RTT, buffer video, write logs,
    drop packets, manage timeout, or reauthenticate.
- heartbeat / video frame handlers
  - Future owner of heartbeat ack decisions, RTT / offset input, video frame
    buffer handoff, and frame drop policy after source acceptance.

Current code reflects this with `apps/server::ServerRegisteredPacketBoundary`,
`ServerRegisteredClientPacket`, `ServerRegisteredHeartbeatPacket`, and
`ServerRegisteredVideoFramePacket`. This is a bridge only; heartbeat processing,
video buffering, sync decisions, and outbound ack execution remain
unimplemented.

---

## Heartbeat Handler Ack Handoff Boundary

The minimal heartbeat handler connection starts from a registered heartbeat
packet and ends at an outbound queue item for `HeartbeatAck`. It is not the
heartbeat state machine.

Flow:

1. Receive loop decodes a `Heartbeat` and preserves source endpoint metadata.
2. Packet acceptance gate checks the decoded `client_id` and source endpoint
   against `AuthenticatedSenderRegistry`.
3. `ServerRegisteredPacketBoundary` produces `ServerRegisteredHeartbeatPacket`
   containing source, `AuthenticatedSenderEntry`, and decoded `Heartbeat`.
4. The heartbeat handler boundary receives that registered packet plus explicit
   `ServerHeartbeatAckTiming`.
5. The handler boundary creates `ServerHeartbeatAckInput`:
   - destination = registered packet source
   - `client_id` / `run_id` / `protocol_version` = decoded heartbeat fields
   - `echoed_sent_at` = `Heartbeat.sent_at`
   - `server_received_at` / `server_sent_at` = supplied timing input
6. `ServerHeartbeatAckBoundary` builds typed `ProtocolMessage::HeartbeatAck`.
7. `ServerOutboundQueueBoundary` hands that typed ack to the outbound queue as
   `OutboundQueueItem`.

Responsibility split:

- receive loop / gate
  - Decode, route, and accept/reject the packet before handler execution.
- registered packet boundary
  - Attaches authenticated sender context to the decoded heartbeat.
- heartbeat handler boundary
  - Converts registered heartbeat + explicit timing into ack handoff.
  - Does not update heartbeat state, read clocks, calculate RTT / offset,
    manage timeout, encode bytes, send UDP, or run a queue.
- ack boundary / outbound queue boundary
  - Build typed `HeartbeatAck` and hand it to the send queue shape.
  - Do not decide heartbeat policy or source authentication.

Current code reflects this with `apps/server::ServerHeartbeatHandlerBoundary`,
`ServerHeartbeatAckTiming`, and `ServerHeartbeatAckHandoff`. Heartbeat liveness
state, timeout, RTT / offset estimation, continuous loop integration, and UDP
send execution remain unimplemented.

---

## Heartbeat State / Timebase Input Boundary

Heartbeat state updates and RTT / offset estimation need the same registered
heartbeat packet, but they are separate responsibilities from ack generation.
The input boundary prepares those data shapes without performing calculations.

Flow:

1. `ServerRegisteredPacketBoundary` produces `ServerRegisteredHeartbeatPacket`.
2. The caller supplies `ServerHeartbeatAckTiming` with server receive / send
   timestamps.
3. `ServerHeartbeatInputBoundary` prepares `ServerHeartbeatProcessingInputs`.
4. `ServerHeartbeatStateInput` carries liveness-state material:
   source endpoint, authenticated sender entry, `client_id`, `run_id`,
   `protocol_version`, heartbeat `sent_at`, server receive time, and optional
   `short_status`.
5. `ServerHeartbeatTimebaseInput` carries future RTT / offset material:
   `client_sent_at`, optional `client_local_time`, `server_received_at`, and
   `server_sent_at`.
6. `ServerHeartbeatTimebasePlanBoundary` converts the timebase input into a
   `crates/timebase` estimation plan.
7. `ServerHeartbeatHandlerBoundary` includes these processing inputs in the
   ack handoff result so future heartbeat state and timebase layers can consume
   them without re-parsing the packet.

Responsibility split:

- registered heartbeat packet
  - Proves source authentication and carries decoded heartbeat fields.
- ack timing
  - Supplies server receive / send timestamps.
  - Does not decide how to calculate RTT / offset.
- heartbeat input boundary
  - Splits registered heartbeat data into state input and timebase input.
  - Does not mutate state, estimate offset, smooth values, or decide timeout.
- timebase plan boundary
  - Converts raw heartbeat timing input into an explicit calculation plan.
  - Does not produce numeric RTT, offset, or smoothed values.
- heartbeat handler boundary
  - Uses the same registered packet and timing to build ack handoff.
  - Carries processing inputs forward for future state / timebase layers.
- timebase / heartbeat state layers
  - Future owners of RTT, offset estimation, smoothing, liveness updates, and
    timeout policy.

Current code reflects this with `apps/server::ServerHeartbeatInputBoundary`,
`ServerHeartbeatProcessingInputs`, `ServerHeartbeatStateInput`, and
`ServerHeartbeatTimebaseInput`. `apps/server::ServerHeartbeatTimebasePlanBoundary`
bridges to `crates/timebase::HeartbeatTimebasePlanBoundary` and stores the
result as `ServerHeartbeatTimebasePlan`.

### Heartbeat RTT / Offset Calculation Policy

The current implementation records the calculation plan only. It deliberately
does not complete RTT, clock offset, smoothing, timeout, or state mutation.

Calculation policy:

1. RTT requires a later client-side ack observation. A server-side heartbeat
   receive sample alone has `client_sent_at`, `server_received_at`, and
   `server_sent_at`, but it does not know when the client receives the ack.
   Therefore `RttEstimatePlan::RequiresClientAckObservation` carries the
   echoed client send timestamp and server ack send timestamp for a future
   estimator.
2. Offset estimation can start only when `Heartbeat.local_time` is present.
   If it is present, `ClockOffsetEstimatePlan::CandidateRequiresDelayCompensation`
   records `client_time_micros` and `server_time_micros`. A future estimator must
   compensate this candidate with the current delay / RTT assumption before it
   becomes an accepted offset.
3. If `Heartbeat.local_time` is absent, the plan records
   `ClockOffsetEstimatePlan::MissingClientLocalTime` and the state layer should
   continue without updating offset.
4. Smoothing is `OffsetSmoothingPlan::Deferred`. The numeric smoothing factor,
   outlier rejection, warm-up behavior, and per-client state update policy are
   left to the future estimator implementation.

Responsibility split:

- state input
  - Carries liveness and timeout material for the heartbeat state layer.
  - Does not own timebase math.
- timebase input
  - Carries raw timestamp observations and clock-domain-specific fields.
  - Does not mix client and server clock domains.
- timebase plan boundary
  - Selects which future calculation path is possible for the sample.
  - Does not compute final RTT, offset, or smoothed state.
- timebase calculation layer
  - Future owner of RTT completion, delay compensation, offset smoothing,
    outlier handling, and per-client estimate state.

### Heartbeat RTT / Offset Minimal Calculation Unit

The first real calculation unit is intentionally small and stateless. It takes
one four-timestamp heartbeat exchange and returns one RTT / offset estimate
candidate. It does not update heartbeat state, smooth estimates, reject network
outliers beyond impossible timestamp ordering, or decide timeout policy.

Required timestamps:

| Field | Clock domain | Source |
| --- | --- | --- |
| `client_sent_at` | client | Original `Heartbeat.sent_at`. |
| `server_received_at` | server | Server timestamp captured when the heartbeat packet was received. |
| `server_sent_at` | server | Server timestamp captured when the `HeartbeatAck` is sent. |
| `client_received_at` | client | Future client-side observation of when the ack was received. |

Formulas for the minimal unit:

- `server_processing = server_sent_at - server_received_at`
- `rtt = (client_received_at - client_sent_at) - server_processing`
- `clock_offset = ((server_received_at - client_sent_at) + (server_sent_at - client_received_at)) / 2`

`clock_offset` is server clock minus client clock. A positive value means the
server clock is ahead for that sample. The calculation rejects samples where
the client receive time is before client send time, server send time is before
server receive time, or total client elapsed time is shorter than server
processing time.

Responsibility split:

- state input
  - Supplies liveness material only.
- timebase input
  - Supplies server-side heartbeat timestamps.
- timebase plan
  - Records that a client ack receive observation is needed.
- minimal calculation unit
  - Computes one stateless `rtt_micros` and `clock_offset_micros` candidate once
    all four timestamps exist.
- future estimator state
  - Owns smoothing, history, outlier policy, timeout integration, and exposing
    corrected timestamps to sync-core.

Current code reflects this with `crates/timebase::HeartbeatExchangeObservation`,
`HeartbeatRttOffsetCalculator`, and `HeartbeatRttOffsetEstimate`.
`apps/server::ServerHeartbeatRttOffsetCalculationBoundary` validates client /
run correlation and echoed `sent_at` before calling the stateless calculator.

### Heartbeat Client Ack Observation Flow

The client ack observation flow returns the missing `client_received_at`
timestamp to the server-side timebase calculator. This is a typed flow design
only; the current code does not add a wire payload, continuous heartbeat loop,
or client-to-server report sender.

Flow:

1. Server receives an authenticated `Heartbeat` and records
   `server_received_at`.
2. Server builds and sends `HeartbeatAck` with:
   - `echoed_sent_at`
   - `server_received_at`
   - `server_sent_at`
3. Client receives the `HeartbeatAck` and immediately records
   `client_received_at` in the client clock domain.
4. Client creates `HeartbeatAckObservation` from the received ack plus
   `client_received_at`.
5. A future client-to-server report path sends that observation back to the
   server. The carrier is intentionally not fixed yet; likely candidates are a
   small extension to `ClientStats` or a dedicated later message.
6. Server converts the protocol-level observation into
   `ServerHeartbeatClientAckObservation`.
7. Server matches it with the stored `ServerHeartbeatTimebasePlan` using
   `client_id`, `run_id`, `echoed_sent_at`, `server_received_at`, and
   `server_sent_at`.
8. After the match, `ServerHeartbeatRttOffsetCalculationBoundary` calls the
   stateless timebase calculator with the complete four-timestamp exchange.

Responsibility split:

- client
  - Owns capturing `client_received_at` as close as possible to ack receive.
  - Does not calculate final server-side estimate state.
- protocol
  - Owns the typed `HeartbeatAckObservation` field set.
  - Does not currently encode or decode it as a wire payload.
- server
  - Owns correlation against the stored heartbeat timebase plan.
  - Does not smooth or commit estimates in this boundary.
- timebase
  - Owns the stateless four-timestamp calculation only.

Current code reflects this with
`protocol::HeartbeatAckObservationBoundary`,
`apps/client::ClientHeartbeatAckObservationBoundary`, and
`apps/server::ServerHeartbeatClientAckObservationBoundary`. The actual report
carrier, send / receive loop integration, state update, and smoothing remain
future work.

### Heartbeat Observation Carrier

The heartbeat observation carrier is the client-to-server message flow that
will carry `HeartbeatAckObservation` back to the server. The selected carrier
for MVP planning is `ClientStats` with an optional heartbeat observation block.
This keeps heartbeat timing feedback tied to the existing client telemetry path
and avoids adding a new wire message type before the rest of `ClientStats`
payload layout is finalized.

Message flow:

1. Client observes `HeartbeatAck` and creates `HeartbeatAckObservation`.
2. Client wraps that observation in a typed `HeartbeatObservationCarrier` with
   `message_type = ClientStats`.
3. A future `ClientStats` sender uses the protocol encoder and sends the
   carrier to the server.
4. Server receives and decodes `ClientStats`.
5. Server extracts the optional heartbeat observation block and maps it to
   `ServerHeartbeatClientAckObservation`.
6. Server correlates it with the stored timebase plan and calls the RTT /
   offset calculator.

Payload direction for the `ClientStats` extension:

| Order | Field | Type | Notes |
| --- | --- | --- | --- |
| 1 | `client_id` | string | ClientStats common field. |
| 2 | `run_id` | string | ClientStats common field. |
| 3 | `sent_at` | `u64 little-endian` | Client stats sample time in client clock domain. |
| 4 | `capture_fps` | `u32 little-endian` | Minimal MVP stats field. |
| 5 | `dropped_frames` | `u64 little-endian` | Minimal MVP stats field. |
| 6 | `bitrate_kbps` | `u32 little-endian` | Minimal MVP stats field. |
| 7 | `heartbeat_observation_present` | `u8` | `0` = absent, `1` = observation block follows. |
| 8 | `echoed_sent_at` | optional `u64 little-endian` | Present only when observation flag is `1`. |
| 9 | `server_received_at` | optional `u64 little-endian` | Present only when observation flag is `1`. |
| 10 | `server_sent_at` | optional `u64 little-endian` | Present only when observation flag is `1`. |
| 11 | `client_received_at` | optional `u64 little-endian` | Present only when observation flag is `1`. |

Responsibility split:

- client observation boundary
  - Captures `client_received_at` and creates `HeartbeatAckObservation`.
- protocol carrier boundary
  - Wraps the observation in a typed `ClientStats` carrier.
  - Owns the minimal `ClientStats` payload bytes, including the optional
    observation block.
  - Does not send UDP packets.
- server carrier boundary
  - Extracts the typed observation for calculator input.
  - Does not run receive loop, gate, smoothing, or state commit.
- `ClientStats` encode/decode
  - Owns the actual payload bytes for the optional observation block.
  - Writes `heartbeat_observation_present = 0` when no ack observation is
    available.
  - Rejects presence tags other than `0` or `1` during decode.

Current code reflects this with `protocol::HeartbeatObservationCarrier`,
`protocol::HeartbeatObservationCarrierBoundary`,
`apps/client::ClientHeartbeatObservationCarrierBoundary`, and
`apps/server::ServerHeartbeatObservationCarrierBoundary`.
`crates/protocol` also implements minimal `ClientStats` payload encode/decode
and `ProtocolMessageEncoderBoundary` support. UDP send/receive connection,
server route / handler wiring, and timebase state update remain future work.
`protocol::ClientStatsPayloadPlanBoundary` records the fixed numeric length and
optional observation block length used by the encoder/decoder.

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
  - Does not choose sinks, retention, or process-wide logging policy.
- log writer
  - Current minimal writer serializes the receive rejection event to one JSON
    Lines record for an `io::Write` sink.
  - Does not own file opening, rotation, retention, async logging, or
    process-wide logging configuration.

Current code reflects this with
`apps/server::ServerReceiveRejectionJsonLogEventBoundary`,
`ServerReceiveRejectionJsonLogEventInput`,
`ServerReceiveRejectionReason`, and `ServerReceiveRejectionDetail`. Minimal
JSON Lines output is connected through
`ServerReceiveRejectionLogOutputBoundary` and
`ServerReceiveRejectionJsonLineWriter`. The one-shot server CLI writes one
receive rejection JSON Lines record to stderr when
`ServerAuthResponsePocError::Rejected` occurs, then prints the existing PoC
error message. This remains synchronous and schema-specific; file output,
rotation, buffering policy, async logging, and a general JSON Lines writer
remain future work. File sink configuration is represented by the shared
`crates/logging` sink plan described in the Server JSON Lines Writer Connection
Scope section.

Minimal emitted fields:

- `event_name`
- `run_id`
- `client_id`
- `source`
- `message_type`
- `rejection_reason`
- `detail`
- `timestamp`

---

## Receive Loop Operational JSON Lines Boundary

Receive loop operational logs are lightweight per-packet observations for a
future continuous receive loop. They are separate from
`server.receive_rejection`: rejection logs keep detailed decode / gate failure
diagnostics, while receive loop logs record the loop outcome needed for
operations and counters.

Flow:

1. `ServerReceiveLoopStep` processes one received packet through decode, route,
   and optional packet acceptance gate.
2. The result is `ServerReceiveLoopGateOutcome::Accepted` or
   `ServerReceiveLoopGateOutcome::Rejected`.
3. `ServerReceiveLoopLogHandoffBoundary` converts the outcome plus packet
   length into `ServerReceiveLoopLogInput`.
4. `ServerReceiveLoopJsonLogEventBoundary` maps that input into
   `ServerReceiveLoopJsonLogEventInput`.
5. `ServerReceiveLoopLogOutputBoundary` can write one `server.receive_loop`
   JSON Lines record to a caller-owned `io::Write` sink.

Event schema:

| Field | Type | Notes |
| --- | --- | --- |
| `event_name` | string | Always `server.receive_loop`. |
| `source` | `PacketSource` | UDP source endpoint captured before decode / gate processing. |
| `outcome` | enum | `Accepted`, `DecodeRejected`, or `AcceptanceRejected`. |
| `packet_len` | integer | Received datagram length supplied by the caller. |
| `message_type` | optional `MessageType` | Present after successful decode / route or acceptance rejection; absent for decode failures without a message type. |
| `client_id` | optional `ClientId` | Present when the decoded route or gate rejection carries a client id. |
| `rejection_reason` | optional enum | Present only for rejected outcomes. Detailed rejection data remains in `server.receive_rejection`. |
| `timestamp` | `TimestampMicros` | Server-side event timestamp supplied by the caller. |

Responsibility split:

- receive loop
  - Owns raw packet intake, decode, route, and gate invocation.
  - Does not decide JSON Lines field names, open sinks, or write logs.
- receive loop operational log handoff
  - Converts accepted / decode rejected / acceptance rejected outcomes into a
    small observation event.
  - Does not drop packets, call handlers, update metrics, or perform auth.
- decode / acceptance rejection logging
  - Keeps detailed failure reason and detail in `server.receive_rejection`.
  - Does not own operational loop counters.
- JSON Lines writer
  - Serializes one `server.receive_loop` record to a caller-owned writer.
  - Does not own file opening, rotation, retention, async logging, or
    process-wide logging.
- sink plan
  - Uses `crates/logging::JsonLinesSinkPlanBoundary` to normalize stderr/file
    destinations for future runtime wiring.
  - Does not open files or write records.

Current code reflects this with
`apps/server::ServerReceiveLoopLogHandoffBoundary`,
`ServerReceiveLoopLogInput`, `ServerReceiveLoopLogOutcome`,
`ServerReceiveLoopJsonLogEventBoundary`,
`ServerReceiveLoopJsonLogEventInput`,
`ServerReceiveLoopLogOutputBoundary`, `ServerReceiveLoopJsonLineWriter`,
`ServerReceiveLoopJsonLinesSinkConfig`,
`ServerReceiveLoopJsonLinesSinkPlan`, and
`ServerReceiveLoopJsonLinesSinkBoundary`. Continuous receive loop execution,
packet drop execution, metrics aggregation, process-wide logging, file opening,
and async logging remain future work.

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
  - Owns the minimal `client_id` and resolved token checks.
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
6. The queue admission boundary applies the bounded-capacity policy before an
   item becomes queue-owned. Current code only returns the admission decision;
   it does not store a collection.
7. A later send implementation will wire-encode the `ProtocolMessage` and send
   the resulting bytes through UDP.

Outbound queue MVP processing scope:

- The outbound queue is a bounded in-memory handoff between server response
  generation and the net send layer.
- The queue storage scope before a continuous send loop is limited to bounded
  typed `OutboundQueueItem` storage, FIFO-compatible ordering, and one-item
  dequeue handoff to the encoder.
- The queue may own typed `OutboundQueueItem` values and select one item for
  the encoder.
- The queue does not encode protocol bytes, inspect encoded payload bytes, call
  UDP sockets, perform retry, sleep, spawn tasks, or write logs.
- The queue must not block receive / handler work. On pressure it returns a
  typed admission decision immediately.
- The current implementation models storage state metadata, one-item lifecycle,
  and admission policy. It does not implement a FIFO collection or a continuous
  send loop.

Send-loop precondition scope:

1. Server handlers create typed outbound items through `ServerOutboundQueueBoundary`.
2. Queue storage admission runs before inserting an item.
3. A future queue collection stores accepted typed items, still pre-encode.
4. A future send loop asks the queue for one ready item.
5. The queue hands that item to the net send layer as `OutboundQueueSendHandoff`.
6. `OutboundSendLoopTickBoundary` prepares one encode request and the send log
   context for that selected item.
7. The net send layer encodes the item using `OutboundPacketEncoderBoundary`.
8. The socket send layer sends an already encoded datagram.
9. The send-loop tick boundary may turn observed encode/socket results into
   `SendLogEvent` candidates.
10. Queue mutation after encode/send errors is future retry/drop policy, not part
   of this step.

Backpressure / drop policy:

| Item class | Messages | Full queue action | Reason |
| --- | --- | --- | --- |
| Control | `AuthResponse`, `HeartbeatAck`, `ServerNotice` and other control messages | Drop incoming item | Control responses are useful only if timely; blocking the receive path would hurt synchronization. |
| Time-sensitive video | `VideoFrame` | Drop oldest queued video item, then accept incoming item | Newer frames are more useful than stale frames for live sync. |
| Telemetry | `ClientStats` if used on an outbound path later | Drop incoming item | Telemetry can be sampled; it must not compete with control or video delivery. |

The initial capacity placeholder is `max_items = 64`. This is a policy marker,
not a tuned production value. Future implementation may split queues per
destination or per message class, but the MVP rule remains bounded and
non-blocking.

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
- outbound queue
  - Owns future storage, admission, bounded capacity, ordering, and item
    selection.
  - Before the send loop exists, exposes storage state and admission decisions
    only.
  - Does not own protocol encode, socket send, retry execution, or log writing.
- encoder handoff
  - Receives one selected `OutboundQueueItem` from queue storage.
  - Owns conversion to `OutboundEncodeRequest` and `EncodedOutboundPacket`.
  - Does not inspect queue capacity or call sockets.
- send-loop tick boundary
  - Connects one dequeued item to encoder input and send log context.
  - Observes encode success/failure and socket send success/failure as state
    names and log event candidates.
  - Does not run a loop, block, sleep, retry, requeue, or write logs.
- send-loop lifecycle boundary
  - Names the future loop body's stop / wait / process-one-item / retry-defer
    decisions.
  - Does not dequeue real items, schedule work, execute retry, or requeue.
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
Backpressure policy is represented by `OutboundQueueCapacityPolicy`,
`OutboundQueueAdmissionPolicyBoundary`, `OutboundQueueAdmissionDecision`, and
the server-side `ServerOutboundQueueBoundary::evaluate_admission` helper.
Queue storage planning is represented by `OutboundQueueStorageState`,
`OutboundQueueStorageDecision`, `OutboundQueueStorageBoundary`, and
`ServerOutboundQueueBoundary::evaluate_storage_push`. These do not store items;
they document the state and decision that a future bounded collection must use.
The minimal packet send-loop connection is represented by
`OutboundSendLoopTickState`, `OutboundSendLoopTickPlan`,
`OutboundSendLoopEvent`, and `OutboundSendLoopTickBoundary`. These types model
one selected item moving toward encode/socket-send observation; they do not
implement a continuous send loop.
The future loop body lifecycle is represented by
`OutboundSendLoopLifecycleState`, `OutboundSendLoopDequeueStatus`,
`OutboundSendLoopLifecycleAction`, `OutboundSendLoopLifecycleInput`,
`OutboundSendLoopLifecyclePlan`, and `OutboundSendLoopLifecycleBoundary`.
These types document loop-body decisions only.

Packet send-loop body implementation scope is intentionally narrow. The future
loop body may ask queue storage for one ready item, use
`OutboundSendLoopLifecycleBoundary` to decide stop / wait / process-one-item,
run the existing one-tick encode and socket-send handoff for that item, emit
send log event candidates, and then return to the next lifecycle decision.
Retry is represented only as `RetryDeferred`; the current scope does not
execute retry, requeue an item, sleep, block a handler, or introduce an async
runtime.

Send-loop body responsibility split:

- queue dequeue
  - Future queue collection owns item ordering and selecting one ready item.
  - Current code only names `NoReadyItem` / `ReadyItem` as dequeue status.
- send-loop lifecycle
  - Decides whether to stop, wait, process one item, or defer retry policy.
  - Does not own collection mutation, encode, socket send, log writing, or
    scheduling.
- one-tick send processing
  - Uses `OutboundSendLoopTickBoundary` to prepare encoder input and observe
    encode/socket outcomes.
  - Does not run a continuous loop or retry.
- socket send
  - Sends one already encoded datagram through the UDP socket adapter.
  - Does not inspect typed protocol messages or queue policy.
- send log
  - Receives `SendLogEvent` candidates from the tick boundary.
  - JSON Lines writer connection remains a later task.
- retry defer
  - `RetryCandidate` failures are marked as deferred policy work.
  - Requeue timing, retry budget, and retry execution remain unimplemented.

---

## ClientStats Receive Route / Handler Boundary

`ClientStats` is a client-scoped inbound server packet. It uses the same decode
and acceptance path as `Heartbeat` and `VideoFrame`, but its handler bridge is
limited to metrics/timebase input extraction.

Flow:

1. UDP receive loop receives packet bytes and source metadata.
2. `net-core::InboundPacketDecoder` decodes fixed header, protocol version, and
   `ClientStats` payload into `ProtocolMessage::ClientStats`.
3. `ServerInboundRouter` maps it to `ServerInboundRoute::ClientStats`.
4. `PacketAcceptanceGateBoundary` checks the decoded `client_id` against
   `AuthenticatedSenderRegistry`.
5. `ServerRegisteredPacketBoundary` attaches the authenticated sender and
   returns `ServerRegisteredClientPacket::ClientStats`.
6. `ServerClientStatsHandlerBoundary` prepares `ServerClientStatsHandlerInput`.
7. If the decoded stats packet contains a heartbeat observation block, the
   handler bridge converts it into `ServerHeartbeatClientAckObservation`.
8. Future metrics and timebase state layers consume the prepared input.

Responsibility split:

- protocol decode owns wire validation and `ClientStats` payload decode only.
- receive loop owns packet intake, decode handoff, route handoff, and gate
  invocation only.
- packet acceptance gate owns authenticated source lookup for client-scoped
  packets, including `ClientStats`.
- handler bridge owns typed input extraction for future stats/timebase state.
- timebase calculator remains separate and only runs after future state
  correlation provides a stored `ServerHeartbeatTimebasePlan`.

Current code reflects this with `ServerInboundRoute::ClientStats`,
`ServerRegisteredClientStatsPacket`, and
`ServerClientStatsHandlerBoundary`. Continuous client stats sending, metrics
state commit, heartbeat observation smoothing, timeout handling, log output,
and async runtime behavior remain unimplemented.

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
backpressure execution. Admission/backpressure decisions are named separately
by `OutboundQueueAdmissionPolicyBoundary`.

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
`protocol::ProtocolMessageEncoderBoundary`. The protocol encoder currently
encodes `AuthRequest`, `AuthResponse`, `HeartbeatAck`, `VideoFrame`,
`ClientStats`, and `ServerNotice`; remaining unsupported messages return
`EncodeNotImplemented`. `UdpSocketIoBoundary` can send an already encoded
packet; queue processing and continuous send orchestration remain unimplemented.

---

## ServerNotice Payload / Handling Boundary

`ServerNotice` is a future lightweight outbound control message from server to
client or switcher. It is not part of heartbeat, video frame, auth secret, or
sync-buffer processing.

Payload layout policy:

1. Fixed header carries `message_type = ServerNotice`, `protocol_version`, and
   `payload_length`.
2. Payload writes `run_id` as the existing length-prefixed UTF-8 string.
3. Payload writes `notice_type` as `u16 little-endian`.
4. Payload writes `message` as the existing length-prefixed UTF-8 string.

Trigger policy scope:

1. Server state transition handlers may later emit explicit notice trigger
   sources.
2. The trigger policy boundary maps only already-decided trigger sources to
   `NoticeType`.
3. Trigger sources in scope for the first placeholder are `Warning`,
   `Disconnect`, `ProtocolError`, `AuthExpired`, and `ServerShutdown`.
4. The policy boundary preserves destination metadata, `run_id`,
   `protocol_version`, and an operator/client-facing short message.
5. The policy boundary does not detect state transitions, suppress duplicate
   notices, rate-limit, enqueue, encode, send sockets, or write logs.

Responsibility split:

- `protocol`
  - Owns `ServerNotice`, `NoticeType`, `NoticeType::wire_code`, payload layout,
    and minimal payload encode/decode.
  - Exposes `ServerNoticePayloadPlanBoundary`,
    `encode_server_notice_payload`, `decode_server_notice_payload`,
    `ServerNoticePayloadDecoder`, and `ProtocolMessageEncoderBoundary`
    support.
- server notice boundary
  - Builds typed `ProtocolMessage::ServerNotice` plus destination metadata from
    already-decided notice fields.
  - Does not decide when to notify, encode bytes, write logs, or send sockets.
- server notice trigger policy
  - Maps explicit trigger sources to `NoticeType`.
  - Produces a typed `ServerNoticeInput` plan for the notice boundary.
  - Does not inspect server state, decide state transitions, rate-limit, log,
    enqueue, encode, or send.
- server state transition handlers
  - Future owner of deciding that a trigger happened, such as protocol error,
    auth expiry, disconnect, warning, or shutdown.
  - Must pass explicit trigger input to the notice trigger policy boundary.
- outbound queue / net send
  - Receives the typed notice through the same `OutboundQueueItem` handoff as
    other outbound control messages.
  - May encode the typed notice through the protocol encoder once a send step
    calls the net send layer.

Current code reflects this with `ServerNoticePayloadPlanBoundary`, minimal
payload encode/decode, decode dispatch, and encoder-boundary support in
`crates/protocol`, plus `ServerNoticeBoundary` / `ServerOutboundNotice` in
`apps/server`. `ServerNoticeTriggerPolicyBoundary`,
`ServerNoticeTriggerInput`, `ServerNoticeTriggerSource`, and
`ServerNoticeTriggerPlan` define the trigger planning boundary. Actual state
transition detection, duplicate suppression, rate limiting, continuous send
loop, socket send, and log output remain future work.

---

## Send Error / Log Event Boundary

PoC / MVP initial implementation keeps send error handling split into
classification, server-side JSON Lines event shaping, and a caller-owned writer
boundary. Minimal UDP `send_to` exists for already encoded packets, but this
boundary does not implement retry, queue mutation, file sink opening, process
wide logging, or async runtime behavior.

Send path checkpoints:

1. `server` response / ack boundaries create typed outbound messages and
   destination metadata.
2. `outbound queue` boundary hands `OutboundQueueItem` to the net send layer
   without executing a real queue.
3. `OutboundSendLoopTickBoundary` plans one selected item for encode and
   extracts log context from the typed message before encode:
   `run_id`, optional `client_id`, destination, and `message_type`.
4. `protocol encoder` converts supported messages into fixed header + payload
   bytes.
5. After encode success, the net send layer can log `encode_succeeded` with the
   encoded byte length before handing bytes to the future socket send layer.
6. Before socket send, the net send layer can reject local send-preparation
   problems such as missing destination metadata or packet size policy.
7. The future socket send layer reports socket-level errors after a real
   `send_to` attempt.
8. `ServerSendErrorLogHandoffBoundary` accepts failure `SendLogEvent` values
   and ignores non-error send observations.
9. `ServerSendErrorJsonLogEventBoundary` maps the handoff into
   `ServerSendErrorJsonLogEventInput`.
10. `ServerSendErrorLogOutputBoundary` writes one `server.send_error` JSON
    Lines record to a caller-owned `io::Write` sink.

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

Send error JSON Lines output scope:

- Event name is `server.send_error`.
- Only failure events are in scope. Encode success events may still be observed
  by `net-core::SendLogEvent`, but the server send-error handoff ignores them.
- Output uses schema-specific writer code in `apps/server`, matching auth and
  receive rejection logging.
- The writer accepts a caller-owned `io::Write` and emits exactly one JSON
  object plus newline.
- Sink config uses the shared `crates/logging::JsonLinesSinkPlanBoundary`, but
  opening files, creating directories, rotation, retention, buffering beyond
  caller-owned writes, and process-wide logger setup remain future work.

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
  - Owns the one-tick placeholder that connects a dequeued item to encoder
    input and send log event candidates.
  - Does not write JSON Lines, choose sinks, execute retry, requeue,
    continuous loops, or socket I/O in this step.
- `socket send`
  - Future owner of calling UDP `send_to` and mapping OS/socket errors into
    the send failure categories.
  - Does not inspect typed protocol messages.
- `server send error JSON Lines boundary`
  - Filters failure `SendLogEvent` values into `ServerSendErrorLogInput`.
  - Builds `ServerSendErrorJsonLogEventInput` and writes one JSON Lines record
    to a caller-owned writer.
  - Does not own queue mutation, retry policy, file opening, rotation, async
    logging, or process-wide logging.
- `logging` sink plan
  - Normalizes stderr/file destination plans for future runtime wiring.
  - Does not open files or write records.

Current code reflects this with `net-core::OutboundSendLogContext`,
`net-core::SendLogStage`, `net-core::SendFailureKind`,
`net-core::SendFailureDisposition`, `net-core::SendLogEvent`,
`net-core::OutboundSendLoopTickState`, `net-core::OutboundSendLoopTickPlan`,
`net-core::OutboundSendLoopEvent`, and
`net-core::OutboundSendLoopTickBoundary`. These are classification and one-tick
connection placeholder types only. Server-side JSON Lines output scope is
represented by `apps/server::ServerSendErrorLogHandoffBoundary`,
`ServerSendErrorLogInput`, `ServerSendErrorJsonLogEventBoundary`,
`ServerSendErrorJsonLogEventInput`, `ServerSendErrorLogOutputBoundary`,
`ServerSendErrorJsonLineWriter`, `ServerSendErrorJsonLinesSinkConfig`,
`ServerSendErrorJsonLinesSinkPlan`, and
`ServerSendErrorJsonLinesSinkBoundary`.
