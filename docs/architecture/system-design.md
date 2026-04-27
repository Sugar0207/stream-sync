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
- `ClientStats`
  - stats / heartbeat observation 処理へ渡す。
  - metrics commit、HeartbeatAck observation の照合、RTT / offset state commit は server / timebase 側の責務とする。
- その他 message
  - 現時点では server inbound の対象外として扱う。
  - `AuthResponse`, `HeartbeatAck`, `ServerNotice` の server inbound としての扱いは別タスクで決める。

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

Continuous receive loop body implementation scope:

1. Check whether a stop has been requested.
2. If not stopping, receive at most one UDP datagram through the existing
   synchronous socket adapter.
3. Pass the received bytes and source endpoint to `ServerReceiveLoopStep` for
   decode, route, and packet acceptance gate evaluation.
4. For `Accepted`, prepare operational receive-loop logging and hand the
   accepted route to future handler dispatch.
5. For `Rejected`, prepare operational receive-loop logging and receive
   rejection logging handoff.
6. Return to the next lifecycle decision.

Current lifecycle placeholder:

- `ServerContinuousReceiveLoopLifecycleBoundary::plan_next` decides only
  stop vs receive-one-datagram.
- `plan_received_packet` marks the point where one received datagram should be
  decoded and gated by `ServerReceiveLoopStep`.
- `plan_after_gate_outcome` marks whether an accepted route should be handed to
  handlers or a rejection should be prepared for logs.
- `plan_socket_receive_error` records a socket receive failure checkpoint only.

Responsibility split for the future loop body:

- socket receive
  - Owns one blocking `recv_from` through the existing synchronous adapter.
  - Does not decode, authenticate, dispatch handlers, or write logs.
- decode / gate
  - Owned by `ServerReceiveLoopStep` and `PacketAcceptanceGateBoundary`.
  - Does not run an outer loop or write JSON Lines.
- handler handoff
  - Future owner of dispatching accepted routes to auth / registered packet
    handlers.
  - Not implemented by the lifecycle placeholder.
- operational logging
  - Uses `ServerReceiveLoopLogOutputBoundary` shape for `server.receive_loop`.
  - Not connected to a real continuous loop yet.
- rejection logging
  - Uses `ServerReceiveRejectionLogOutputBoundary` shape for detailed
    `server.receive_rejection`.
  - Packet drop execution remains future work.

Current code reflects this with
`apps/server::ServerContinuousReceiveLoopLifecycleState`,
`ServerContinuousReceiveLoopAction`,
`ServerContinuousReceiveLoopLifecycleInput`,
`ServerContinuousReceiveLoopLifecyclePlan`, and
`ServerContinuousReceiveLoopLifecycleBoundary`. These are loop-body planning
types only. They do not call sockets, run a continuous loop, invoke handlers,
drop packets, write logs, sleep, block beyond a socket adapter call, or
introduce an async runtime.

Continuous receive loop 1 tick connection scope:

1. `plan_next` decides whether the next tick should stop or wait for one
   datagram.
2. `observe_received_packet` records the received datagram length and moves the
   tick to decode / gate planning.
3. The caller uses `ServerReceiveLoopStep` to decode, route, and evaluate the
   packet acceptance gate.
4. `observe_gate_outcome` records the gate outcome and marks whether
   operational logging, rejection logging, or handler handoff is needed.
5. `observe_socket_receive_error` records the socket receive error checkpoint
   without deciding retry or logging policy.

The 1 tick placeholder does not own the actual socket call, receive buffer,
registry mutation, handler execution, packet drop, JSON Lines writes, or retry.
It only makes the connection between socket receive result, lifecycle decision,
operational logging requirement, detailed rejection logging requirement, and
future handler handoff explicit.

Current code reflects the 1 tick connection with
`ServerContinuousReceiveLoopTickState`,
`ServerContinuousReceiveLoopTickPlan`, and
`ServerContinuousReceiveLoopTickBoundary`. These types are intentionally
separate from `ServerReceiveLoopStep`: the step performs one-packet
decode/route/gate when called, while the tick boundary only describes how a
future loop will connect that step to socket receive and downstream handoffs.

Continuous receive loop writer handoff scope:

1. A future receive tick obtains `ServerReceiveLoopGateOutcome` and packet
   length after decode / gate.
2. `ServerContinuousReceiveLoopWriterHandoffBoundary` observes that outcome and
   reuses the tick boundary to determine whether operational logging, rejection
   logging, or handler handoff is required.
3. For accepted outcomes, it prepares `ServerReceiveLoopLogInput` for
   `server.receive_loop` and marks handler handoff as required.
4. For rejected outcomes, it prepares `ServerReceiveLoopLogInput` for
   `server.receive_loop` and preserves `ServerReceiveLoopGateRejection` for the
   detailed `server.receive_rejection` writer.
5. It does not call either writer. The caller remains responsible for passing
   the prepared inputs to `ServerReceiveLoopJsonLogEventBoundary` /
   `ServerReceiveLoopJsonLineWriter` and
   `ServerReceiveRejectionLogOutputBoundary` when the real loop exists.

Responsibility split:

- receive tick
  - Produces or observes one `ServerReceiveLoopGateOutcome`.
  - Does not own writer calls or sink choice.
- operational logging handoff
  - Prepares lightweight `server.receive_loop` input for accepted and rejected
    outcomes.
  - Does not write JSON Lines.
- rejection logging handoff
  - Preserves detailed rejection input only for rejected outcomes.
  - Does not execute packet drop.
- sink plan
  - Remains a config/runtime wiring concern; no file is opened by this handoff.

Current code reflects this with
`ServerContinuousReceiveLoopWriterHandoffPlan` and
`ServerContinuousReceiveLoopWriterHandoffBoundary`.

Continuous receive loop writer call scope:

1. A future receive tick supplies `ServerReceiveLoopGateOutcome`, packet length,
   timestamp, and caller-owned operational / rejection writers.
2. `ServerContinuousReceiveLoopWriterRuntimeBoundary` builds the same writer
   handoff plan described above.
3. If operational logging is required, it calls
   `ServerReceiveLoopLogOutputBoundary` to write one `server.receive_loop`
   record to the caller-owned operational writer.
4. If rejection logging is required, it calls
   `ServerReceiveRejectionLogOutputBoundary` to write one
   `server.receive_rejection` record to the caller-owned rejection writer.
5. It returns the handoff plan and the emitted event inputs for the caller.

This is the maximum current writer connection scope. It does not own the
receive loop, socket receive, receive buffer lifetime, sink selection, file
opening, directory creation, process-wide logger setup, async logging, handler
dispatch, or packet drop. File and stderr choices remain sink-plan/runtime
wiring concerns outside this boundary.

Current code reflects this with
`ServerContinuousReceiveLoopWriterRuntimeResult` and
`ServerContinuousReceiveLoopWriterRuntimeBoundary`.

Continuous receive loop handler handoff runtime scope:

1. A future receive tick supplies `ServerReceiveLoopGateOutcome`, packet length,
   timestamp, caller-owned writers, and the current in-memory
   `AuthenticatedSenderRegistry`.
2. `ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary` first delegates
   operational / rejection JSON Lines output to
   `ServerContinuousReceiveLoopWriterRuntimeBoundary`.
3. If the outcome was rejected, the handler handoff plan is `NotRequired` and
   no handler input is created.
4. If the accepted route is `AuthRequest`, the boundary converts it to
   `ServerAuthCheck` through `ServerAuthHandlerBoundary`.
5. If the accepted route is `Heartbeat`, `VideoFrame`, or `ClientStats`, the
   boundary converts it to `ServerRegisteredClientPacket` through
   `ServerRegisteredPacketBoundary`, preserving the authenticated sender
   binding.
6. If the accepted route is unsupported for the server, the boundary records an
   unsupported handoff plan with source and `MessageType` only.

This is the maximum current handler handoff connection before the continuous
receive loop body. It does not execute auth decisions, call heartbeat / video /
stats handlers, enqueue outbound responses, update state, drop packets, choose
log sinks, open files, install a process-wide logger, retry, or run a loop.

Responsibility split:

- receive tick
  - Owns socket receive and decode / gate composition in the future loop.
- writer runtime
  - Writes one operational event and, when rejected, one detailed rejection
    event to caller-owned writers.
- handler handoff runtime
  - Converts only accepted routes into the next handler input shape.
- future loop body
  - Owns sequencing socket receive, writer runtime, handler execution, packet
    drop, shutdown, and retry/defer policy.

Current code reflects this with
`ServerContinuousReceiveLoopHandlerHandoffRuntimePlan`,
`ServerContinuousReceiveLoopHandlerHandoffRuntimeResult`, and
`ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary`.

Continuous receive loop minimal one-tick runtime execution scope:

1. The caller owns a bound synchronous `UdpSocket`, receive buffer, current
   `AuthenticatedSenderRegistry`, expected `ProtocolVersion`, timestamp, and
   caller-owned operational / rejection writers.
2. `ServerContinuousReceiveLoopOneTickRuntimeBoundary` asks
   `ServerContinuousReceiveLoopTickBoundary` for the start tick plan.
3. If stop is requested, the boundary returns `Stopped` before any socket
   receive or writer call.
4. Otherwise it calls `ServerUdpSocketIoStep::receive_one_with_gate_details`,
   which performs one blocking datagram receive and returns both packet length
   and `ServerReceiveLoopGateOutcome`.
5. The boundary passes that outcome to
   `ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary`, which writes
   operational / rejection logs through caller-owned writers and prepares the
   handler handoff plan.
6. Socket receive errors are returned as a one-tick `SocketReceiveFailed`
   outcome with the socket-error tick plan. Writer errors remain `io::Result`
   errors from the runtime call.

This is still not the continuous receive loop body. It executes exactly one
tick and does not repeat, sleep, own shutdown policy beyond one stop flag,
dispatch handlers, apply packet drop side effects, mutate auth/session state,
open file sinks, install a process-wide logger, retry, or spawn async work.

Responsibility split:

- socket receive
  - `ServerUdpSocketIoStep` owns one synchronous datagram receive and
    decode/gate connection.
- tick plan
  - `ServerContinuousReceiveLoopTickBoundary` owns stop / receive /
    socket-error checkpoint naming.
- writer runtime
  - Caller-owned writers receive at most one operational event and one
    rejection event.
- handler handoff runtime
  - Accepted routes are converted only into the next handler input.
- future loop body
  - Owns repeated ticks, shutdown policy, backoff, handler execution, packet
    drop, metrics state commits, and sink lifecycle.

Current code reflects this with
`ServerUdpSocketGateReceiveOutcome`,
`ServerContinuousReceiveLoopOneTickRuntimeInput`,
`ServerContinuousReceiveLoopOneTickRuntimeOutcome`,
`ServerContinuousReceiveLoopOneTickRuntimeResult`, and
`ServerContinuousReceiveLoopOneTickRuntimeBoundary`.

Continuous receive loop minimal body implementation scope:

1. A caller owns the bound synchronous `UdpSocket`, receive buffer,
   `AuthenticatedSenderRegistry`, expected `ProtocolVersion`, timestamp, stop
   flag, and caller-owned operational / rejection writers.
2. `ServerContinuousReceiveLoopBodyBoundary::run_once` evaluates the stop flag
   and records whether this body iteration should stop or execute one tick.
3. The body delegates to `ServerContinuousReceiveLoopOneTickRuntimeBoundary`
   with the same socket, buffer, registry, timestamp, protocol version, and
   writers.
4. If stop was requested, the one-tick runtime stops before socket receive.
5. If stop was not requested, the one-tick runtime performs one datagram
   receive, decode / gate, writer runtime, and handler handoff runtime.
6. The body returns both the selected body action and the one-tick runtime
   result to the caller.

This is the current maximum loop body implementation. It is not a complete
continuous receive loop: it does not repeat automatically, calculate wall-clock
timestamps, own shutdown signals, open file sinks, install process-wide logging,
dispatch handlers, mutate auth/session/heartbeat/video state, execute packet
drop side effects, retry, back off, or spawn async work.

Responsibility split:

- loop body
  - Owns one stop check and one delegation to the one-tick runtime.
- one-tick runtime
  - Owns one socket receive plus decode / gate / writer / handoff connection.
- future continuous loop controller
  - Will own repeated calls, timing, shutdown, retry/backoff, and stateful
    handler dispatch.

Current code reflects this with
`ServerContinuousReceiveLoopBodyInput`,
`ServerContinuousReceiveLoopBodyAction`,
`ServerContinuousReceiveLoopBodyResult`, and
`ServerContinuousReceiveLoopBodyBoundary`.

Continuous receive loop controller scope:

1. The future controller is the outer orchestration boundary above
   `ServerContinuousReceiveLoopBodyBoundary::run_once`.
2. The controller may ask the caller to execute one body iteration when the
   caller has already decided that the server should continue.
3. The controller consumes a caller-owned `continue_requested` decision instead
   of reading OS signals, channels, config, or operator state directly.
4. The controller passes caller-supplied protocol version and timestamp into a
   `ServerContinuousReceiveLoopBodyInput` for exactly one body iteration.
5. After the body returns, the controller classifies the result as stopped,
   completed, or error-policy-deferred, then yields back to the caller.
6. Repeating the next iteration is still caller-owned. The current controller
   placeholder does not implement a `while` loop.

Responsibility split:

- controller
  - Owns the future outer checkpoint: stop vs run one body iteration.
  - Owns observation of body results for caller-level orchestration.
  - Does not own socket receive, decode, gate, writer calls, handler dispatch,
    packet drop side effects, retry/backoff, timestamp generation, file sink
    lifecycle, process-wide logging, or async runtime.
- `run_once` body
  - Owns one stop check and one delegation to the one-tick runtime.
  - Does not repeat by itself.
- one-tick runtime
  - Owns one synchronous datagram receive plus decode / gate / writer /
    handler-handoff preparation.
  - Does not execute real handlers or mutate heartbeat/video state.
- handler dispatch
  - Remains future work after the handler handoff result.
  - Auth decision, outbound enqueue, heartbeat handling, video frame handling,
    stats state commits, and packet drop side effects are not part of the
    controller placeholder.
- shutdown policy
  - Remains outside the controller placeholder.
  - A future shutdown policy may convert signals, config, operator actions, or
    error policy into `continue_requested`; the controller only consumes that
    decision.

Current code reflects this with
`ServerContinuousReceiveLoopControllerInput`,
`ServerContinuousReceiveLoopControllerPlan`,
`ServerContinuousReceiveLoopControllerObservation`, and
`ServerContinuousReceiveLoopControllerBoundary`. This is the current maximum
continuous receive-loop controller scope; it is not the completed continuous
receive loop implementation.

Continuous receive loop to handler dispatch bridge scope:

1. After one `run_once` body iteration returns, the future controller may pass
   the body result to a handler dispatch bridge.
2. The bridge reads only the existing one-tick outcome and the
   `ServerContinuousReceiveLoopHandlerHandoffRuntimePlan` prepared by the
   one-tick runtime.
3. If the loop stopped or socket receive failed, the bridge produces
   `NotRequired` and preserves that no handler should run for this iteration.
4. If a packet completed and the handler handoff contains `Auth`, the bridge
   exposes `ServerAuthCheck` as the future auth dispatch input.
5. If the handoff contains `RegisteredClient`, the bridge exposes
   `ServerRegisteredClientPacket` as the future registered-client dispatch
   input.
6. Unsupported routes and handoff preparation errors are preserved as dispatch
   handoff plans, but no policy is executed at this layer.

Responsibility split:

- controller
  - Decides whether to request one body iteration and observes the body result.
  - Does not call concrete handlers.
- `run_once` body
  - Produces one body result containing one-tick runtime output.
- one-tick runtime
  - Produces writer output and handler handoff preparation for one packet.
- handler handoff runtime
  - Converts accepted routes into typed handler inputs only.
- handler dispatch bridge
  - Converts the body / handoff result into a future dispatch plan.
  - Does not run auth decisions, heartbeat / video / stats handlers, outbound
    enqueue, packet drop, state mutation, retry/backoff, sink selection, file
    open, process-wide logging, or async runtime.
- future handler dispatch body
  - Will own actual handler execution and any resulting state changes or
    outbound queue handoffs.

Current code reflects this with
`ServerContinuousReceiveLoopHandlerDispatchPlan`,
`ServerContinuousReceiveLoopHandlerDispatchHandoff`, and
`ServerContinuousReceiveLoopHandlerDispatchBoundary`.

Handler dispatch minimal body scope:

1. The minimal dispatch body receives
   `ServerContinuousReceiveLoopHandlerDispatchHandoff` from the bridge and
   returns `ServerHandlerDispatchOutcome`.
2. It classifies `Auth` into `ServerHandlerDispatchResult::Auth` and preserves
   `ServerAuthCheck` for a future auth flow step. It does not decide accept /
   reject, mutate the authenticated sender registry, or enqueue `AuthResponse`.
3. It splits `RegisteredClient` into heartbeat, video frame, and client stats
   result lanes. It does not run heartbeat ack/state handling, video frame
   buffering, sync scheduling, stats state commit, or timebase updates.
4. Unsupported routes, skipped iterations, and handoff preparation errors are
   preserved as dispatch results. Packet drop policy and error logging policy
   remain outside this boundary.
5. Future outbound enqueue is not part of the dispatch body. Handler execution
   may later produce outbound handoff items, but the queue lifecycle and send
   loop remain separate responsibilities.
6. Future stats handling is separate from dispatch classification. Dispatch
   only preserves `ServerRegisteredClientStatsPacket`; conversion into metrics
   state or heartbeat observation state remains handler work.

Responsibility split after the handler dispatch bridge:

- auth dispatch
  - Owns future auth flow execution from `ServerAuthCheck`.
  - May later call auth decision, registry update, auth log output, and outbound
    response handoff.
- registered packet dispatch
  - Owns future heartbeat / video / client stats handler calls from typed
    registered packets.
  - Does not own packet acceptance or sender lookup; those already happened in
    the handler handoff runtime.
- future outbound enqueue
  - Owns conversion from handler outputs to outbound queue items.
  - Remains separate from classifying inbound handler work.
- future stats handling
  - Owns stats metrics state commit and optional heartbeat observation commit.
  - Remains separate from generic dispatch routing.

Current code reflects this with `ServerHandlerDispatchResult`,
`ServerHandlerDispatchOutcome`, and `ServerHandlerDispatchBoundary`.

Auth dispatch minimal runtime scope:

1. The auth dispatch runtime receives `ServerHandlerDispatchOutcome` from the
   generic handler dispatch body.
2. If the result is `ServerHandlerDispatchResult::Auth`, it calls the existing
   `ServerAuthFlowStep::handle_auth_check` with `ServerAuthConfig`.
3. It returns `ServerAuthDispatchRuntimeOutcome`, preserving `packet_len` and
   the `ServerAuthFlowOutcome`.
4. If the result is not auth, it returns `NotAuth` with the original
   `ServerHandlerDispatchResult` so another future dispatch runtime can handle
   that lane.
5. The auth dispatch runtime does not register authenticated senders, write
   JSON Lines, persist queue items, encode packets, send UDP, run packet drop
   policy, or own the continuous loop body.

Responsibility split for auth dispatch:

- auth dispatch runtime
  - Selects only the auth lane from handler dispatch output.
  - Calls the existing auth flow step and returns its typed result.
- auth decision
  - Remains inside `ServerAuthFlowStep` / `ServerAuthDecisionBoundary`.
  - Owns config input preparation, secret resolution result handling, and
    accepted / rejected decision construction.
- outbound response handoff
  - Remains inside `ServerAuthResponseBoundary` and
    `ServerOutboundQueueBoundary`.
  - Produces an `OutboundQueueItem` only; it does not push into durable or
    in-memory queue storage from this runtime.
- future loop body
  - Will decide when to call auth dispatch, where to store queue items, when to
    apply registry registration, and when to hand logs to a writer.
  - Remains synchronous for MVP; no async runtime is introduced here.

Current code reflects this with `ServerAuthDispatchRuntimeResult`,
`ServerAuthDispatchRuntimeOutcome`, and `ServerAuthDispatchRuntimeBoundary`.

Registered packet dispatch minimal runtime scope:

1. The registered packet dispatch runtime receives `ServerHandlerDispatchOutcome`
   from the generic handler dispatch body.
2. If the result is `RegisteredHeartbeat`, it calls the existing
   `ServerHeartbeatHandlerBoundary::handoff_ack` with caller-owned
   `ServerHeartbeatAckTiming`.
3. It returns `ServerRegisteredPacketDispatchRuntimeOutcome`, preserving
   `packet_len` and the heartbeat ack handoff result.
4. `RegisteredVideoFrame` is preserved as `FutureVideoFrame`; no video decode,
   frame buffer, sync scheduling, or file sink work is performed.
5. `RegisteredClientStats` is preserved as `FutureClientStats`; no metrics
   state commit, RTT / offset state commit, or stats log output is performed.
6. Non-registered handler dispatch results are returned as `NotRegistered` so
   auth and other future dispatch runtimes remain separate.

Responsibility split for registered packet dispatch:

- registered packet dispatch runtime
  - Selects registered heartbeat / video / stats lanes from handler dispatch
    output.
  - Connects heartbeat to the minimal ack handoff only.
- heartbeat handler
  - Owns heartbeat state/timebase input preparation and `HeartbeatAck` handoff.
  - Does not mutate heartbeat state, calculate committed RTT / offset state,
    store queue items, encode bytes, or send UDP.
- future video handler
  - Will own video frame validation beyond packet acceptance, frame buffering,
    sync scheduling, decoder handoff, and drop policy.
- future stats handling
  - Will own metrics state commit and optional heartbeat observation commit.
  - The current dispatch runtime only preserves the typed stats packet.
- outbound enqueue
  - Is limited here to the existing one-item `OutboundQueueItem` handoff from
    the heartbeat ack boundary.
  - Queue storage, admission side effects, send-loop scheduling, encoding, and
    socket send remain future responsibilities.

Current code reflects this with
`ServerRegisteredPacketDispatchRuntimeResult`,
`ServerRegisteredPacketDispatchRuntimeOutcome`, and
`ServerRegisteredPacketDispatchRuntimeBoundary`.

Video / stats handler minimal runtime scope:

1. The video / stats handler runtime receives
   `ServerRegisteredPacketDispatchRuntimeOutcome`.
2. If the result is `FutureVideoFrame`, it produces
   `ServerVideoFrameHandlerInput` from the registered packet and records the
   H.264 payload byte length. It does not decode video, buffer frames, schedule
   sync, write files, or apply video drop policy.
3. If the result is `FutureClientStats`, it calls the existing
   `ServerClientStatsHandlerBoundary::prepare_input` and produces
   `ServerClientStatsHandlerInput`. It does not commit metrics state, commit
   heartbeat observation state, calculate durable RTT / offset state, or write
   stats logs.
4. Heartbeat ack results and unrelated lanes are returned as `NotVideoOrStats`
   so heartbeat state commit and outbound work remain owned by their own
   layers.

Responsibility split for video / stats:

- registered packet dispatch
  - Classifies registered packets into heartbeat ack, future video, and future
    stats lanes.
  - Does not own video buffering, stats commit, or send-loop side effects.
- future video handler
  - Owns later validation beyond packet acceptance, frame buffering, sync-core
    handoff, decoder handoff, and frame drop policy.
  - Current placeholder only preserves the registered packet and payload length.
- future stats handling
  - Owns later metrics state commit and optional heartbeat observation commit.
  - Current runtime only prepares `ServerClientStatsHandlerInput`.
- heartbeat state commit
  - Remains separate from video / stats handling. Heartbeat state/timebase input
    is produced by heartbeat handling but not committed here.
- outbound enqueue
  - Remains separate from video / stats handling. This runtime creates no
    outbound queue items.

Current code reflects this with `ServerVideoFrameHandlerInput`,
`ServerVideoFrameHandlerBoundary`, `ServerVideoStatsHandlerRuntimeResult`,
`ServerVideoStatsHandlerRuntimeOutcome`, and
`ServerVideoStatsHandlerRuntimeBoundary`.

Continuous receive loop body to dispatch runtime scope:

1. The body dispatch runtime receives one `ServerContinuousReceiveLoopBodyResult`.
2. It uses `ServerContinuousReceiveLoopHandlerDispatchBoundary` to turn the
   body result into a handler dispatch handoff.
3. It uses `ServerHandlerDispatchBoundary` to classify the handoff into an auth
   lane, registered packet lane, or no-dispatch result.
4. If the lane is auth, it calls `ServerAuthDispatchRuntimeBoundary` once and
   returns the auth dispatch runtime outcome.
5. If the lane is registered heartbeat, it calls
   `ServerRegisteredPacketDispatchRuntimeBoundary` once and returns the
   heartbeat ack handoff outcome.
6. If the lane is registered video or stats, it calls
   `ServerRegisteredPacketDispatchRuntimeBoundary` once and then
   `ServerVideoStatsHandlerRuntimeBoundary` once to prepare typed video / stats
   handler input.
7. Stopped loops, socket receive failures, rejected outcomes, unsupported
   routes, and handoff errors remain no-dispatch results for future policy.

Responsibility split for body dispatch:

- receive loop body
  - Owns one synchronous `run_once` iteration and produces
    `ServerContinuousReceiveLoopBodyResult`.
  - Does not run handler runtimes itself.
- body dispatch runtime
  - Owns the one-result orchestration from body output to the existing handler
    runtime chain.
  - Does not repeat, sleep, back off, open sinks, write logs, mutate registry
    state, persist queue items, encode packets, send UDP, or drop packets.
- auth dispatch
  - Owns the existing auth flow result for one auth handler input.
  - Registry registration application and auth log writing remain future loop
    responsibilities.
- registered packet dispatch
  - Owns heartbeat ack handoff and the transition to future video / stats lanes.
  - Heartbeat state commit and outbound queue storage remain separate.
- video stats handler runtime
  - Owns typed video / stats input preparation only.
  - Video buffering, sync handoff, stats state commit, and RTT / offset state
    commit remain future work.
- future loop body
  - Will own repetition, shutdown policy, side-effect application, queue storage,
    log writing, packet drop policy, and error handling.

Current code reflects this with
`ServerContinuousReceiveLoopBodyDispatchRuntimeResult`,
`ServerContinuousReceiveLoopBodyDispatchRuntimeOutcome`, and
`ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary`.

Dispatch runtime side-effect apply scope:

1. The side-effect apply boundary receives
   `ServerContinuousReceiveLoopBodyDispatchRuntimeOutcome`.
2. For auth dispatch results, it applies only
   `AuthenticatedSenderRegistration` to the in-memory
   `AuthenticatedSenderRegistry` through `AuthenticatedSenderRegistryBoundary`.
3. Auth log input and the `AuthResponse` `OutboundQueueItem` remain part of the
   returned `ServerAuthFlowOutcome`; this boundary does not write logs or store
   queue items.
4. For heartbeat results, it preserves `ServerHeartbeatAckHandoff`, including
   its `OutboundQueueItem`, but does not commit heartbeat state, store queue
   items, encode packets, or send UDP.
5. For video results, it preserves `ServerVideoFrameHandlerInput` only. Frame
   buffering, sync-core handoff, decoder handoff, and drop policy remain future
   work.
6. For stats results, it preserves `ServerClientStatsHandlerInput` only.
   Metrics commit, heartbeat observation commit, and durable RTT / offset state
   commit remain future work.
7. No-dispatch, unsupported, and error lanes are preserved without packet drop
   policy or error policy execution.

Responsibility split for side-effect apply:

- auth flow result
  - Owns the decision, auth log handoff input, optional registry registration,
    outbound response, and outbound queue item.
  - Does not mutate process state by itself.
- registry registration
  - The only side effect currently applied by this boundary.
  - Applies accepted auth sender binding to the caller-owned in-memory registry.
- outbound enqueue
  - Remains a typed handoff only. Queue storage, admission side effects,
    send-loop scheduling, encoding, retry, and UDP send stay outside this layer.
- stats prepare result
  - Remains a typed input for future stats state work.
  - No metrics or heartbeat observation state is committed here.
- future state commit
  - Will own heartbeat state, RTT / offset state, video buffer state, stats
    state, and packet drop policy.

Current code reflects this with `ServerDispatchRuntimeSideEffectApplyResult`,
`ServerDispatchRuntimeSideEffectApplyOutcome`, and
`ServerDispatchRuntimeSideEffectApplyBoundary`.

Dispatch runtime output apply scope:

1. The output apply boundary receives
   `ServerDispatchRuntimeSideEffectApplyOutcome`.
2. For auth results, it writes `ServerAuthLogInput` through the existing
   `ServerAuthLogOutputBoundary` to a caller-owned `io::Write`.
3. For accepted auth results only, it passes the `AuthResponse`
   `OutboundQueueItem` to `ServerOutboundQueueBoundary::evaluate_storage_push`
   and then to `OutboundQueueLifecycleBoundary::hold_for_send` when the storage
   decision accepts the candidate.
4. Rejected auth results currently write auth logs but do not enter outbound
   queue storage from this boundary. Rejection response sending policy remains a
   future loop decision.
5. Registry registration remains owned by the previous side-effect apply
   boundary. This output boundary does not mutate the registry.
6. Heartbeat ack handoffs are preserved by output apply and routed by the
   following queue collection bridge. Video and stats handoffs remain preserved
   and are not routed to queue storage or log writers here.

Responsibility split for output apply:

- registry registration
  - Already applied before this boundary for accepted auth only.
- outbound queue
  - Receives accepted auth `AuthResponse` as typed `OutboundQueueItem` storage
    planning and one-item queued placeholder only.
  - Does not own a collection, dequeue, encode, send, retry, or wake a send
    loop.
- auth log writer
  - Uses the existing auth log event schema and JSON Lines writer with a
    caller-owned writer.
  - Does not open files, rotate sinks, buffer globally, or install a
    process-wide logger.
- heartbeat / video / stats handoff
  - Heartbeat ack handoff remains typed and is picked up by the queue
    collection bridge for the one-shot receive/send confirmation path.
  - Video buffer handoff and stats state commit are future work.

Current code reflects this with `ServerOutboundQueueStorageApplyResult`,
`ServerDispatchRuntimeOutputApplyResult`,
`ServerDispatchRuntimeOutputApplyOutcome`, and
`ServerDispatchRuntimeOutputApplyBoundary`.

Minimal queue collection and send runtime scope:

1. Accepted auth output apply can produce a one-item `QueuedOutboundItem`.
2. `ServerOutboundQueueCollectionBoundary` may push that queued item into a
   caller-owned `ServerOutboundQueueCollection`. It also accepts a preserved
   `ServerHeartbeatAckHandoff` and turns its typed `OutboundQueueItem` into a
   one-item queued placeholder for the one-shot heartbeat ack confirmation
   path.
3. The same boundary can dequeue one item and hand it to
   `ServerOutboundSendOneRuntimeBoundary`.
4. The send runtime uses `OutboundQueueLifecycleBoundary` to create an
   `OutboundQueueSendHandoff`, `OutboundSendLoopTickBoundary` to plan one
   encode step, `OutboundPacketEncoderBoundary` /
   `ProtocolMessageEncoderBoundary` to produce `EncodedOutboundPacket`, and
   `ServerUdpSocketIoStep::send_encoded` to perform one synchronous UDP send.
5. The send runtime records encode and socket-send observations but does not
   write logs, retry, requeue, or continue a loop.

Responsibility split for the minimal send path:

- queue storage
  - Owns caller-provided collection storage for typed queued items.
  - Current collection is FIFO-compatible and minimal; it does not implement
    backpressure, eviction, wakeups, persistence, or retries.
- dequeue
  - Selects at most one queued item for this send step.
  - Empty queue remains a no-op result.
- encode
  - Converts one typed `OutboundQueueItem` into `EncodedOutboundPacket`.
  - Does not inspect queue policy or socket behavior.
- socket send
  - Sends one encoded UDP datagram through the existing synchronous socket
    adapter.
  - Does not retry or fragment.
- send log
  - `ServerSendLogOutputBoundary` may write one `server.send` JSON Lines
    record for one-item send success or failure.
  - File sink open, process-wide logger integration, buffering policy, retry,
    and requeue remain future work.

Current code reflects this with `ServerOutboundQueueCollection`,
`ServerOutboundQueueCollectionBoundary`,
`ServerOutboundQueueCollectionPushOutcome`,
`ServerOutboundQueueDequeueRuntimeResult`,
`ServerOutboundSendOneRuntimeOutcome`, `ServerOutboundSendOneRuntimeError`,
`ServerOutboundSendOneRuntimeBoundary`, `ServerSendJsonLogEventInput`,
`ServerSendLogOutputBoundary`, and `ServerSendJsonLineWriter`.

Receive/send one-iteration integration scope:

1. `ServerReceiveSendOneIterationRuntimeBoundary` receives caller-owned socket,
   receive buffer, authenticated sender registry, outbound queue collection,
   auth config, and log writers.
2. It executes exactly one `ServerContinuousReceiveLoopBodyBoundary::run_once`.
3. It passes the body result through body dispatch, side-effect apply, and
   output apply.
4. It pushes any accepted auth response queued item or preserved heartbeat ack
   handoff into the caller-owned queue collection, dequeues at most one item,
   and passes that item to `ServerOutboundSendOneRuntimeBoundary`.
5. If one item is sent, it writes a single `server.send` JSON Lines observation
   to the caller-owned send log writer.
6. It returns the body, dispatch, side-effect, output, queue push, dequeue,
   optional send outcome, and optional send log event so a future controller can
   decide what to do next.

Responsibility split for receive/send integration:

- receive body
  - Owns one synchronous UDP receive and receive-side writer handoff.
- dispatch
  - Owns handler lane classification and one handler runtime chain.
- side-effect apply
  - Owns accepted auth registry registration only.
- output apply
  - Owns auth log writer handoff and accepted auth queue storage planning.
- queue collection / dequeue
  - Owns caller-provided typed queue storage and one-item selection for
    accepted auth responses and heartbeat ack handoffs.
- one-item send runtime
  - Owns one encode + socket send attempt.
- send log writer
  - Owns one caller-owned JSON Lines write for send success/failure
    observation.
- future controller
  - Owns repetition, shutdown policy, retry/requeue, queue retention, file sink
    open, process-wide logger, and packet drop policy.

Current code reflects this with `ServerReceiveSendOneIterationRuntimeInput`,
`ServerReceiveSendOneIterationRuntimeOutcome`,
`ServerReceiveSendOneIterationRuntimeError`, and
`ServerReceiveSendOneIterationRuntimeBoundary`.

Controller to receive/send one-iteration scope:

1. `ServerControllerReceiveSendRuntimeBoundary` receives controller input and
   caller-owned socket, receive buffer, registry, queue collection, auth config,
   and writers.
2. It calls `ServerContinuousReceiveLoopControllerBoundary::plan_next_iteration`.
3. If the controller action is `Stop`, it returns `Stopped` without receiving,
   dispatching, queueing, writing, encoding, or sending.
4. If the controller action is `RunBodyOnce`, it calls
   `ServerReceiveSendOneIterationRuntimeBoundary` exactly once.
5. It observes the returned body result with
   `ServerContinuousReceiveLoopControllerBoundary::observe_body_result` and
   returns the plan, iteration outcome, and observation.

Responsibility split for controller receive/send runtime:

- controller
  - Owns stop vs run-one-iteration decision and body-result observation.
- one-iteration receive/send runtime
  - Owns one receive body execution and optional one send attempt.
- caller/future loop
  - Owns repeated invocation, timestamp generation, shutdown policy, retry /
    requeue, sink lifecycle, process-wide logger, and error policy.

Current code reflects this with `ServerControllerReceiveSendRuntimeInput`,
`ServerControllerReceiveSendRuntimeResult`,
`ServerControllerReceiveSendRuntimeError`, and
`ServerControllerReceiveSendRuntimeBoundary`.

Completed one-iteration CLI / config entry:

1. `ServerReceiveSendOneIterationLauncher` loads the same server TOML shape used
   by the auth response PoC: `[server].bind_host`, `[server].bind_port`,
   `[session].protocol_version`, and `[auth]`.
2. The launcher binds one UDP socket, initializes an in-memory
   `AuthenticatedSenderRegistry` and `ServerOutboundQueueCollection`, and calls
   `ServerControllerReceiveSendRuntimeBoundary` once.
3. `apps/server` exposes this through
   `--receive-send-once [config-path]`.
4. The CLI writes receive-loop / rejection / auth / send JSON Lines records to
   caller-owned stderr handles and prints a short summary to stdout.
5. The CLI waits for exactly one packet and exits after that one controller
   step. It does not repeat, retry, requeue, rotate files, or install a global
   logger.

Manual check shape:

1. In one terminal, run:
   `cargo run -p stream-sync-server -- --receive-send-once configs/examples/server.example.toml`
2. In another terminal, send an accepted auth request with the accepted client
   example:
   `cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml`
3. Expected result: server stderr includes auth / receive JSON Lines, server
   stdout reports one handled packet with non-zero `sent_bytes`, and the client
   receives an accepted `AuthResponse`.

Current code reflects this with `ServerReceiveSendOneIterationLauncher`,
`ServerReceiveSendOneIterationStartupOutcome`,
`ServerReceiveSendOneIterationStartupError`, and the server CLI flag
`--receive-send-once`.

Completed two-iteration auth-then-heartbeat CLI / config entry:

1. `ServerReceiveSendTwoIterationLauncher` loads the same server TOML shape as
   `--receive-send-once`.
2. The launcher binds one UDP socket and keeps one in-memory
   `AuthenticatedSenderRegistry` and `ServerOutboundQueueCollection` across
   exactly two controller receive/send runtime calls.
3. The expected manual shape is accepted auth request first, then one
   `Heartbeat` from the same client UDP source.
4. The first iteration can send `AuthResponse`; the second iteration can route
   the registered heartbeat to `ServerHeartbeatHandlerBoundary`, queue one
   `HeartbeatAck`, commit one liveness state update from the preserved
   heartbeat handoff, and send the ack through the one-item send runtime.
5. `apps/server` exposes this through
   `--receive-send-twice [config-path]`. It does not run a continuous loop,
   timeout scanner, retry, requeue, open file sinks, or install a global
   logger.

Manual check shape:

1. In one terminal, run:
   `cargo run -p stream-sync-server -- --receive-send-twice configs/examples/server.example.toml`
2. In another terminal, run:
   `cargo run -p stream-sync-client -- --auth-heartbeat-poc-once configs/examples/client.accepted.example.toml`
3. Expected result: server stderr includes receive / auth / send JSON Lines for
   `AuthResponse` and `HeartbeatAck`, server stdout reports two handled packets
   with non-zero sent bytes and one liveness entry, and the client stdout displays the received
   `HeartbeatAck` timestamps.

Completed three-iteration auth-then-heartbeat-observation CLI / config entry:

1. `ServerReceiveSendThreeIterationLauncher` loads the same server TOML shape as
   `--receive-send-once`.
2. The launcher binds one UDP socket and keeps one in-memory
   `AuthenticatedSenderRegistry` and `ServerOutboundQueueCollection` across
   exactly three controller receive/send runtime calls.
3. The expected manual shape is accepted auth request first, one `Heartbeat`
   from the same client UDP source second, then one `ClientStats` packet with
   `HeartbeatAckObservation` third.
4. The first iteration can send `AuthResponse`; the second iteration can send
   `HeartbeatAck` and preserves the heartbeat `timebase_plan`; the third
   iteration extracts `HeartbeatAckObservation` from `ClientStats`.
5. `ServerHeartbeatObservationReturnBoundary` connects the preserved
   `ServerHeartbeatAckHandoff` and returned `ServerClientStatsHandlerInput` to
   `ServerHeartbeatRttOffsetCalculationBoundary`.
6. `apps/server` exposes this through
   `--receive-send-three [config-path]`. It calculates one stateless RTT /
   offset candidate for stdout, but does not commit heartbeat state, smooth
   offset, run a continuous loop, retry, requeue, open file sinks, or install a
   global logger.

Manual check shape:

1. In one terminal, run:
   `cargo run -p stream-sync-server -- --receive-send-three configs/examples/server.example.toml`
2. In another terminal, run:
   `cargo run -p stream-sync-client -- --auth-heartbeat-stats-poc-once configs/examples/client.accepted.example.toml`
3. Expected result: server stderr includes receive / auth / send JSON Lines for
   `AuthResponse`, `HeartbeatAck`, and the returned `ClientStats`; server
   stdout reports three handled packets plus one stateless heartbeat RTT /
   offset calculation; client stdout displays the sent `ClientStats` observation
   fields.

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
state commit is handled by a separate boundary. Timeout enforcement, durable
RTT / offset state, continuous loop integration, and UDP send execution beyond
the current one-item runtime remain unimplemented.

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

### Heartbeat Liveness State Commit Boundary

The heartbeat liveness state commit boundary records that a registered heartbeat
was observed by the server. It is intentionally separate from auth registry
management and from ack generation.

Current implementation scope:

1. `ServerHeartbeatHandlerBoundary` receives a registered heartbeat and returns
   `ServerHeartbeatAckHandoff`.
2. The handoff includes `ServerHeartbeatProcessingInputs.state`, which is a
   `ServerHeartbeatStateInput`.
3. `ServerHeartbeatLivenessCommitBoundary::commit` writes that input into
   `ServerHeartbeatLivenessState`.
4. `ServerHeartbeatLivenessState` is an in-memory map keyed by `client_id`.
5. Each `ServerHeartbeatLivenessEntry` stores:
   - source endpoint
   - authenticated sender snapshot
   - `client_id`
   - `run_id`
   - `protocol_version`
   - last heartbeat client `sent_at`
   - last server receive timestamp
   - optional short status
   - received heartbeat count
   - liveness status
6. A commit always marks the entry `Alive` and increments the per-client
   received heartbeat count.
7. The two-iteration and three-iteration manual runtimes commit the heartbeat
   state once from the preserved heartbeat handoff, then expose the resulting
   liveness entry count in stdout.

Responsibility split:

- authenticated sender registry
  - Owns accepted auth source binding.
  - Does not own heartbeat freshness, heartbeat counters, or timeout state.
- registered heartbeat / ack timing
  - Supplies authenticated heartbeat data and explicit server timestamps.
  - Does not persist liveness state.
- heartbeat ack handoff
  - Builds the outbound `HeartbeatAck` queue item.
  - Carries state/timebase inputs for later layers.
- liveness commit boundary
  - Persists the latest registered heartbeat observation in memory.
  - Does not send `ServerNotice`, revoke auth entries, drop packets, or run
    periodic scans.
- timebase / stats path
  - Uses heartbeat timestamps and returned client observation for RTT / offset.
  - Does not own liveness freshness or auth expiry.

Timeout policy is represented as an explicit evaluation, not as an automatic
loop. `ServerHeartbeatTimeoutPolicy` supplies `timeout_after_micros`, and
`ServerHeartbeatLivenessCommitBoundary::evaluate_timeout` classifies one client
at a caller-supplied server timestamp as `Alive`, `TimedOut`, or `NoHeartbeat`.
This does not mutate the auth registry, remove liveness entries, emit logs, or
notify clients. The future continuous loop should be the owner of when timeout
evaluation runs and what state transition / log / notice follows.

### Heartbeat Timeout Action Boundary

Timeout evaluation is separated from the actions that should follow a timeout.
The current implementation adds a decision boundary that plans these actions
without running a continuous scanner or performing socket I/O.

Current implementation scope:

1. `ServerHeartbeatLivenessCommitBoundary::evaluate_timeout` returns one
   `ServerHeartbeatTimeoutEvaluation`.
2. `ServerHeartbeatTimeoutActionBoundary::plan_actions` consumes that
   evaluation, the current `ServerHeartbeatLivenessState`, and the server
   timestamp used for the evaluation.
3. For `Alive`, the action plan contains no registry invalidation, no timeout
   log input, and no notice.
4. For `NoHeartbeat`, the action plan also contains no side effects because
   there is no source endpoint / run context to invalidate or notify.
5. For `TimedOut` with a matching liveness entry, the action plan includes:
   - `AuthenticatedSenderInvalidation` with reason `HeartbeatTimeout`
   - `ServerHeartbeatTimeoutLogInput`
   - `ServerNoticeTriggerPlan` using `ServerNoticeTriggerSource::AuthExpired`
6. `AuthenticatedSenderRegistryBoundary::invalidate` can apply the explicit
   invalidation command by removing the client entry from the in-memory
   registry.
7. `ServerHeartbeatTimeoutJsonLogEventBoundary` maps timeout log handoff input
   to the future `server.heartbeat_timeout` JSON Lines event shape.

Responsibility split:

- liveness evaluation
  - Decides only whether a client is `Alive`, `TimedOut`, or has no heartbeat
    state at a caller-supplied timestamp.
  - Does not mutate auth, log, or send notices.
- timeout action boundary
  - Converts a `TimedOut` result into typed action plans.
  - Does not apply those actions.
- registry invalidation
  - Applies an explicit invalidation command to the in-memory auth registry.
  - Does not decide timeout policy or reauthentication policy.
- timeout log
  - Receives a typed log event input with elapsed time and timeout threshold.
  - Writer / file sink / process-wide logger wiring remains future work.
- timeout notice
  - Uses the existing `ServerNotice` trigger policy and maps heartbeat timeout
    to `AuthExpired`.
  - Queue storage, rate limiting, duplicate suppression, and UDP send remain
    future work.

This preserves the current policy that a timeout can be reasoned about in one
small step, while the continuous heartbeat loop remains responsible for when to
run timeout evaluation and which planned effects it actually applies.

### Heartbeat Timeout Apply Boundary

The timeout apply boundary is the smallest point a future continuous heartbeat
loop can call after evaluation and action planning. It applies the already
planned effects in a deterministic order, but it still does not run the loop or
send packets.

Current implementation scope:

1. The future loop chooses a client and calls
   `ServerHeartbeatLivenessCommitBoundary::evaluate_timeout`.
2. The loop passes that evaluation to
   `ServerHeartbeatTimeoutActionBoundary::plan_actions`.
3. The loop passes the action plan to
   `ServerHeartbeatTimeoutApplyBoundary::apply_plan`.
4. `apply_plan` applies only these minimal effects:
   - remove one auth registry entry through
     `AuthenticatedSenderRegistryBoundary::invalidate`
   - write one `server.heartbeat_timeout` JSON Lines record to a caller-owned
     writer
   - convert the `AuthExpired` notice plan into typed `ServerNotice` outbound
     handoff and `OutboundQueueItem`
5. The apply result reports:
   - original timeout evaluation
   - optional registry invalidation outcome
   - optional timeout log event
   - optional notice handoff

Responsibility split:

- timeout evaluation
  - Reads liveness state and returns `Alive`, `TimedOut`, or `NoHeartbeat`.
  - Does not decide or apply effects.
- action plan
  - Decides which effects should happen for one `TimedOut` evaluation.
  - Does not mutate state or write logs.
- apply boundary
  - Applies the explicit invalidation command, writes to a caller-owned writer,
    and prepares the typed notice queue item.
  - Does not scan clients, run a continuous loop, open file sinks, install a
    global logger, store the notice in the queue collection, encode, send,
    retry, rate-limit, or suppress duplicates.
- future continuous loop
  - Owns when to evaluate clients, which timeout policy to use, and when to
    call the apply boundary.
  - Later work must decide queue storage, send-loop wakeup, notice duplicate
    suppression, disconnect metrics, and reauthentication policy.

### Heartbeat Timeout Loop Tick Boundary

The timeout loop tick boundary is the current connection point for a future
continuous heartbeat loop. It composes the already-separated timeout stages for
one caller-selected client, without becoming a completed loop.

Current implementation scope:

1. The future loop selects one `client_id`, a server timestamp, and a
   `ServerHeartbeatTimeoutPolicy`.
2. It passes those values to
   `ServerHeartbeatTimeoutLoopTickBoundary::run_one_client`.
3. The tick boundary calls:
   - `ServerHeartbeatLivenessCommitBoundary::evaluate_timeout`
   - `ServerHeartbeatTimeoutActionBoundary::plan_actions`
   - `ServerHeartbeatTimeoutApplyBoundary::apply_plan`
4. The tick result preserves:
   - original one-client tick input
   - action plan
   - apply result
5. `TimedOut` can therefore remove the in-memory auth registry entry, write one
   timeout JSON Lines record to a caller-owned writer, and produce one typed
   `AuthExpired` notice handoff.
6. `Alive` and `NoHeartbeat` return a result without registry invalidation,
   timeout log event, or notice handoff.
7. If the caller wants to store the timeout notice for a future send loop, it
   passes the apply result to
   `ServerHeartbeatTimeoutNoticeQueueStorageBoundary::store_notice`.
8. Notice queue storage may push the typed notice item into caller-owned
   `ServerOutboundQueueCollection` and return a send wakeup placeholder.

Responsibility split:

- liveness commit
  - Updates `ServerHeartbeatLivenessState` only when registered heartbeat input
    is observed.
  - Does not run timeout scans.
- timeout evaluation
  - Reads liveness state for one explicit client at one explicit timestamp.
  - Does not apply effects.
- action planning
  - Converts a timed-out evaluation into explicit invalidation/log/notice
    plans.
  - Does not mutate state.
- apply
  - Applies only the explicit one-client plan.
  - Produces a typed notice handoff only.
  - Does not store the notice in queue storage, wake a sender, choose the next
    client, or repeat.
- notice queue storage
  - Receives the timeout apply result and caller-owned outbound queue
    collection.
  - Stores only `notice_handoff.queue_item` when storage admission accepts it.
  - Does not encode, send, retry, rate-limit, or wake a real task.
- future send wakeup
  - Is represented by `ServerHeartbeatTimeoutNoticeSendWakeupPlan`.
  - Is requested only when a timeout notice is actually queued.
  - Does not signal a condvar, spawn a task, or call UDP sockets.
- future loop body
  - Still owns client iteration order, cadence, stop condition, timeout policy
    selection, queue ownership, wakeup execution, and metrics.

This keeps the completed continuous heartbeat loop out of scope while fixing
the call shape that the future loop should use.

### Heartbeat Timeout Multi-Client Loop Boundary

The multi-client heartbeat timeout loop is now a thin caller over the existing
one-client timeout tick. It snapshots authenticated client ids from the
caller-owned registry, calls the one-client tick once per registered client,
then stores any timeout notice handoff into caller-owned outbound queue
storage. It does not execute notice send wakeups or become the completed
continuous server loop.

Current implementation scope:

1. The caller supplies:
   - immutable `ServerHeartbeatLivenessState`
   - mutable `AuthenticatedSenderRegistry`
   - mutable `ServerOutboundQueueCollection`
   - `ServerHeartbeatTimeoutMultiClientLoopInput`
   - caller-owned timeout log writer
2. `ServerHeartbeatTimeoutMultiClientLoopBoundary::run_all_registered`
   snapshots `client_id` values from the authenticated registry before
   mutation.
3. For each registered client, it calls
   `ServerHeartbeatTimeoutLoopTickBoundary::run_one_client`.
4. It preserves each one-client tick result in
   `ServerHeartbeatTimeoutMultiClientLoopClientResult`.
5. It passes each tick apply result to
   `ServerHeartbeatTimeoutNoticeQueueStorageBoundary::store_notice`.
6. It returns:
   - `NoClientsAvailable` when the authenticated registry has no clients
   - `AllClientsProcessed` with per-client results and timeout action count
7. Notice queue storage remains separate from wakeup execution:
   - the boundary may store timeout notice items
   - it returns storage results that include wakeup plans
   - it does not signal, spawn, encode, send, retry, or run a send loop

Responsibility split:

- authenticated sender registry
  - Provides the client ids to scan.
  - Receives explicit invalidation through the existing one-client apply path.
  - Remains caller-owned.
- heartbeat liveness state
  - Is read by one-client timeout evaluation.
  - Is not mutated by the multi-client loop.
- timeout policy
  - Is supplied once for the loop pass and reused for each one-client tick.
  - Is not reinterpreted by the multi-client loop.
- timeout action plan/apply
  - Remains owned by the existing one-client tick boundary.
  - Produces invalidation/log/notice handoff exactly as before.
- notice queue storage
  - Stores optional notice handoffs into caller-owned queue collection.
  - Keeps send wakeup as a typed plan only.
- future continuous server loop owner
  - Still owns cadence, sleeping, stop condition, writer lifetime, queue
    lifetime, wakeup execution, receive/send loop coordination, retry policy,
    and metrics.

Current code reflects this with
`ServerHeartbeatTimeoutMultiClientLoopInput`,
`ServerHeartbeatTimeoutMultiClientLoopClientResult`,
`ServerHeartbeatTimeoutMultiClientLoopResult`, and
`ServerHeartbeatTimeoutMultiClientLoopBoundary`.

### Single-View Video PoC: Client Placeholder Frame Send

The first client-side video send slice is intentionally narrow. It proves that
one client can build a `VideoFrame`, wrap caller-provided bytes as an explicit
placeholder encoded H.264 payload, encode the frame through the existing
protocol encoder, and send one UDP datagram through a caller-owned socket.

Current implementation scope:

1. `ClientPlaceholderEncodedH264PayloadSourceBoundary` accepts caller-provided
   non-empty bytes as placeholder encoded H.264 payload.
2. `ClientVideoFrameMetadataConstructionBoundary` combines explicit
   `client_id`, `run_id`, `frame_id`, capture timestamp, send timestamp,
   dimensions, fps, keyframe flag, and payload into one `VideoFrame`.
3. The constructed frame uses `Codec::H264`, `metadata_reserved = [0; 3]`, and
   `payload_size = payload.len()`.
4. `ClientVideoFrameEncodeSendBoundary::encode_handoff` uses the existing
   `ProtocolMessageEncoderBoundary` and produces encoded packet bytes without
   sending them.
5. `ClientVideoFrameEncodeSendBoundary::send_one` performs exactly one UDP
   `send_to` using a caller-owned `UdpSocket` and explicit destination.
6. `ClientPlaceholderVideoFramePocLauncher` reuses the existing client PoC TOML
   fields for `client_id`, `run_id`, protocol version, and server destination,
   then sends one placeholder `VideoFrame` for manual verification.
7. The client binary exposes this through
   `--placeholder-video-frame-poc-once [config-path]` and prints a compact
   stdout summary including destination, frame id, timestamps, dimensions, and
   payload length.
8. `ClientAuthPlaceholderVideoFramePocLauncher` keeps one UDP socket, sends
   `AuthRequest`, waits for an accepted `AuthResponse`, then sends one
   placeholder `VideoFrame` from the same local source.
9. The client binary exposes this through
   `--auth-placeholder-video-frame-poc-once [config-path]` and prints a compact
   stdout summary including local source, auth response status, frame metadata,
   payload length, `same_source=true`, and `placeholder_payload=true`.

Responsibility split:

- placeholder payload source
  - Names the fake input as placeholder encoded H.264 bytes.
  - Rejects empty payloads explicitly.
  - Does not capture the screen, call FFmpeg, encode real H.264, or inspect NAL
    units.
- metadata construction
  - Owns only `VideoFrame` field assembly.
  - Does not send UDP or own runtime/socket state.
- encode/send boundary
  - Owns protocol encode handoff and optional one-shot UDP send.
  - Does not authenticate, retry, run a receive loop, update queues, decode
    H.264, schedule sync, display frames, or touch OBS.
- caller / future continuous client loop owner
  - Will own real capture source, real encoder, frame cadence, socket lifetime,
    destination selection, retry policy, and integration with auth/heartbeat
    state.
- same-socket auth + placeholder launcher
  - Owns only the short manual verification sequence for authentication
    followed by one placeholder frame send from the same UDP source.
  - Does not weaken server authentication, retry, run a continuous loop, decode
    H.264, schedule sync, display frames, or touch OBS.

Current code reflects this with
`ClientPlaceholderEncodedH264PayloadSourceBoundary`,
`ClientVideoFrameMetadataConstructionBoundary`,
`ClientVideoFrameEncodeSendInput`,
`ClientVideoFrameEncodedSendHandoff`,
`ClientVideoFrameEncodeSendBoundary`, and
`ClientPlaceholderVideoFramePocLauncher`. The same-socket manual sender is
`ClientAuthPlaceholderVideoFramePocLauncher`.

Deferred work:

- real screen capture or frame source
- real H.264 encoding
- H.264 decode and real single-view display
- 2-view / 4-view sync, switcher UI, and OBS integration

### Single-View Video PoC: Server Frame Queue Storage

The first video-path PoC slice is intentionally server-side and narrow. The
existing receive/router/authentication path can already produce an accepted
`ServerVideoFrameHandlerInput`. The new boundary stores that accepted encoded
frame into caller-owned per-client queue state so later steps can wire client
UDP send and switcher display without changing authentication or protocol
boundaries.

Current implementation scope:

1. `ServerRegisteredPacketBoundary` still owns authenticated sender lookup for
   `VideoFrame`.
2. `ServerVideoFrameHandlerBoundary` still converts the accepted registered
   packet into `ServerVideoFrameHandlerInput` and records payload length.
3. `ServerDispatchRuntimeSideEffectApplyBoundary` preserves accepted video as
   `ServerDispatchRuntimeSideEffectApplyResult::VideoFrame`.
4. `ServerVideoFrameQueueRuntimeBoundary::store_from_receive_side_effect`
   connects that accepted receive side effect to
   `ServerVideoFrameQueueStorageBoundary::store_frame`.
5. Rejected / unauthenticated `VideoFrame` packets remain not queued and are
   surfaced as `ServerVideoFrameQueueRuntimeSkipReason::RejectedVideoFrame`.
6. The queue is keyed by `ClientId` and stores encoded `VideoFrame` payloads
   as `ServerQueuedVideoFrame`.
7. `ServerVideoFrameQueuePolicy` controls per-client queue capacity. The
   initial live-video policy drops the oldest frame when a client's queue is
   full, then stores the newest frame.

Responsibility split:

- protocol
  - Owns `VideoFrame` payload encode/decode and does not inspect H.264 content.
- net-core / receive route
  - Owns packet decode handoff and source metadata preservation.
- authenticated sender registry
  - Owns accept/drop decision before a frame can be queued.
- video handler input boundary
  - Preserves accepted packet metadata and payload length only.
- video frame queue storage
  - Mutates caller-owned encoded-frame queue state.
  - Does not decode H.264, select target time, sync multiple clients, notify a
    switcher, render UI, send UDP, or touch OBS.
- video frame queue runtime
  - Consumes only the receive side-effect output and caller-owned queue state.
  - Stores accepted authenticated frames.
  - Keeps rejected / unauthenticated frames out of the queue and reports that
    skip separately from normal non-video side effects.
- future continuous server loop owner
  - Will decide cadence, longer-lived queue ownership, draining, switcher/sync
    handoff, and file/process logging policy.

Current code reflects this with
`ServerVideoFrameQueueState`, `ServerVideoFrameQueueStorageBoundary`,
`ServerVideoFrameQueueRuntimeBoundary`, and
`ServerVideoFrameQueueRuntimeResult`.

Deferred work:

- real client-side capture or frame source
- real client-side H.264 encoding
- video send CLI/config launcher
- H.264 decode and single-view display placeholder
- 2-view / 4-view sync, switcher UI, and OBS integration

### Single-View Video PoC: Switcher Placeholder Handoff

The first switcher-side video slice is a read-only placeholder path from the
server's per-client encoded-frame queue to a display handoff. It selects one
client's newest queued frame and preserves the encoded payload plus metadata
while explicitly marking H.264 decode as deferred.

Current implementation scope:

1. `SwitcherSingleViewLatestFrameSelectionBoundary::select_latest` borrows
   `ServerVideoFrameQueueState` and a `ClientId`.
2. It reads the requested client's queue without mutating it and selects the
   newest queued frame.
3. If no frame exists, it returns `NoFrameAvailable` with the requested
   `ClientId`.
4. If a frame exists, it returns `SwitcherSingleViewSelectedEncodedFrame` with
   frame id, timestamps, dimensions, fps, keyframe flag, encoded payload length,
   and encoded payload bytes.
5. `SwitcherSingleViewPlaceholderDisplayBoundary::prepare_handoff` wraps the
   selected encoded frame in `SwitcherSingleViewDisplayPlaceholderHandoff` with
   `SwitcherSingleViewDecodeStatus::DeferredPlaceholder`.
6. `SwitcherSingleViewPlaceholderPathBoundary` composes selection and
   placeholder display handoff for the current one-client PoC path.

Responsibility split:

- per-client video frame queue
  - Remains owned and mutated by server-side queue storage/runtime boundaries.
- switcher queue read
  - Borrows queue state and selects a latest encoded frame for one client.
  - Does not pop, drain, reorder, or mutate queues.
- placeholder decode/display handoff
  - Preserves encoded frame metadata and payload bytes.
  - Marks real H.264 decode as deferred.
  - Does not allocate decoded pixel buffers, render a window, run sync
    scheduling, or integrate with OBS.
- future H.264 decode / display owner
  - Will replace the placeholder status with real decode output and renderable
    frame data.
- future sync scheduling
  - Will decide target time and multi-client frame selection before display.

Current code reflects this with
`SwitcherSingleViewLatestFrameSelectionBoundary`,
`SwitcherSingleViewSelectedEncodedFrame`,
`SwitcherSingleViewPlaceholderDisplayBoundary`,
`SwitcherSingleViewDisplayHandoffResult`, and
`SwitcherSingleViewPlaceholderPathBoundary`.

Manual verification status:

- `--placeholder-video-frame-poc-once [config-path]` sends one placeholder
  `VideoFrame` only. It does not authenticate, wait for `AuthResponse`, or
  reuse a socket from another client PoC command.
- `--auth-placeholder-video-frame-poc-once [config-path]` keeps one UDP socket,
  sends `AuthRequest`, requires an accepted `AuthResponse`, and then sends one
  placeholder `VideoFrame` from the same source.
- The server's authenticated receive path still requires `VideoFrame` packets
  to come from a source already registered by an accepted `AuthRequest`.
- Running the existing auth CLI and then running the placeholder video CLI does
  not establish that condition, because each command owns a separate UDP socket
  and normally uses a different source port.
- The server queue runtime boundary and switcher placeholder selection boundary
  can be verified through focused tests.
- `--receive-auth-video-queue-once [config-path]` owns the server registry and
  queue state for the manual client-to-server queue PoC. A complete
  client-to-server-to-switcher command sequence still needs a switcher helper
  or shared runtime queue bridge.
- The current manual verification notes live in
  `docs/operations/manual-placeholder-video-poc.md`.

Deferred work:

- real H.264 decode
- real switcher window / UI rendering
- target-time sync scheduling
- 2-view / 4-view selection and layout
- OBS integration

### Heartbeat Timeout Notice Queue Storage / Send Wakeup

Timeout notice queue storage is the narrow boundary after timeout apply. It is
not the completed send loop and it does not perform a real wakeup. It only
makes the storage and wakeup contract explicit for the future continuous loop.

Current implementation scope:

1. `ServerHeartbeatTimeoutApplyBoundary::apply_plan` may produce
   `ServerHeartbeatTimeoutNoticeHandoff`.
2. `ServerHeartbeatTimeoutNoticeQueueStorageBoundary::store_notice` receives:
   - caller-owned `ServerOutboundQueueCollection`
   - `ServerHeartbeatTimeoutApplyResult`
3. If the apply result has no notice handoff, it returns `NoNotice` with
   `ServerHeartbeatTimeoutNoticeSendWakeupPlan::NotRequested`.
4. If a notice exists, the boundary evaluates existing outbound queue storage
   admission with `ServerOutboundQueueBoundary::evaluate_storage_push`.
5. If storage rejects the notice, it returns `Dropped` and does not request a
   wakeup.
6. If storage accepts the notice, it holds the item as `QueuedOutboundItem`,
   pushes it to the caller-owned collection, and returns `Stored` with
   `RequestSendLoopWakeup`.

Responsibility split:

- timeout apply
  - Owns registry invalidation, timeout log writer handoff, and typed notice
    handoff creation.
  - Does not decide queue storage or wakeup execution.
- notice queue handoff
  - Preserves `ServerNotice`, `OutboundQueueItem`, and trigger metadata from
    timeout apply.
  - Does not store the item by itself.
- notice queue storage boundary
  - Owns one synchronous push into caller-owned in-memory queue collection.
  - Uses the existing outbound queue admission policy.
  - Does not create a continuous queue worker.
- future send wakeup
  - Is a typed plan attached to successful storage.
  - Later loop/controller code must choose the actual wake mechanism.
- future continuous loop
  - Owns client scanning, repeated timeout ticks, queue collection ownership,
    wakeup execution, send-loop scheduling, retry, and backpressure policy.

Current code reflects this with
`ServerHeartbeatTimeoutNoticeQueueStorageBoundary`,
`ServerHeartbeatTimeoutNoticeQueueStorageResult`, and
`ServerHeartbeatTimeoutNoticeSendWakeupPlan`. The wakeup plan is intentionally
data only; no thread, async runtime, or socket send is started here.

### Heartbeat RTT / Offset Calculation Policy

The current implementation records the calculation plan only. It deliberately
does not complete durable RTT / offset state, smoothing, automatic timeout
enforcement, or auth revocation.

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

### Heartbeat RTT / Offset State Commit Boundary

The RTT / offset state commit boundary records the latest stateless estimate in
server memory. It is deliberately smaller than the future estimator state: it
does not smooth, reject outliers, or expose corrected timestamps.

Current implementation scope:

1. `ServerHeartbeatRttOffsetCalculationBoundary` produces one
   `ServerHeartbeatRttOffsetCalculation`.
2. `ServerHeartbeatRttOffsetCandidatePolicyBoundary::evaluate` evaluates the
   calculation before commit.
3. `ServerHeartbeatRttOffsetPolicyCommitBoundary::evaluate_and_commit` commits
   accepted candidates and skips rejected candidates.
4. `ServerHeartbeatRttOffsetCommitBoundary::commit` accepts an approved
   calculation through `ServerHeartbeatRttOffsetCommitInput`.
5. `ServerHeartbeatRttOffsetState` stores one
   `ServerHeartbeatRttOffsetStateEntry` per `client_id`.
6. Each entry stores:
   - `client_id`
   - `run_id`
   - latest `HeartbeatRttOffsetEstimate`
   - committed sample count
   - optional server commit timestamp
7. A same-run commit overwrites the latest estimate and increments the sample
   count.
8. A new `run_id` for the same `client_id` overwrites the latest estimate and
   resets the sample count to 1. The outcome records that the previous run was
   replaced.
9. `--receive-send-three` runs the default candidate policy before committing
   the one returned observation calculation into this state and reports the
   entry count / sample count in stdout.

Responsibility split:

- stateless calculator
  - Validates one returned observation and computes one RTT / offset candidate.
  - Does not retain history or mutate server state.
- state commit boundary
  - Stores the latest candidate and simple per-run sample count.
  - Does not calculate, smooth, reject outliers, alter timeout state, log, or
    notify clients.
- policy commit boundary
  - Connects candidate policy to latest estimate commit.
  - Does not commit rejected candidates.
- future smoothing / estimator state
  - Owns smoothing factor, warm-up, outlier policy, confidence, history, and
    corrected timestamp exposure to sync-core.
- future timeout loop
  - Owns liveness / timeout decisions.
  - Does not depend on RTT / offset smoothing being complete.

### Heartbeat RTT / Offset Candidate Policy Boundary

The candidate policy boundary sits between stateless calculation and latest
estimate commit. It is not the completed smoothing / outlier implementation; it
only fixes the decision shape for future estimator work.

Current implementation scope:

1. `ServerHeartbeatRttOffsetCandidatePolicyBoundary::evaluate` receives:
   - current `ServerHeartbeatRttOffsetState`
   - one `ServerHeartbeatRttOffsetCalculation`
   - `ServerHeartbeatRttOffsetCandidatePolicy`
2. The policy contains:
   - `ServerHeartbeatRttOffsetSmoothingMode::Deferred`
   - optional RTT delta threshold
   - optional clock offset delta threshold
3. If no same-run previous estimate exists, the candidate is accepted.
4. If the previous estimate is from a different `run_id`, cross-run outlier
   comparison is skipped and the candidate is accepted.
5. If optional thresholds are configured for the same run, the boundary can
   reject a candidate as:
   - `RttDeltaExceeded`
   - `ClockOffsetDeltaExceeded`
6. Accepted candidates still report smoothing as `Deferred`.

Responsibility split:

- stateless calculator
  - Produces one numeric candidate from one heartbeat exchange.
  - Does not inspect previous estimates.
- candidate policy boundary
  - Performs only optional same-run delta checks against the latest committed
    estimate.
  - Does not mutate state, smooth values, keep history, calculate confidence,
    or publish corrected timestamps.
- latest estimate commit
  - Stores the accepted candidate and sample count.
  - Does not decide whether a candidate is an outlier.
- policy commit
  - Calls candidate policy before commit and skips rejected candidates.
  - Does not smooth, publish corrected timestamps, write logs, or update
    metrics.
- rejected candidate log / metrics handoff
  - Runs after policy commit returns `Skipped(RejectedOutlier)`.
  - Builds one typed log input and one metrics counter handoff.
  - Does not decide policy or mutate RTT / offset state.
- rejected candidate metrics state
  - Aggregates rejected candidate counters by `client_id` and `run_id`.
  - Does not write logs, export records, or own loop cadence.
- future smoothing / corrected timestamp publisher
  - Owns EWMA or other smoothing, outlier model, warm-up, confidence, and
    publishing corrected timestamps to sync-core / targetTime.

### Heartbeat RTT / Offset Rejected Candidate Log / Metrics

Rejected RTT / offset candidates are operational observations, not state
updates. The current implementation keeps this split explicit so an outlier
does not accidentally enter the latest estimate state while still being visible
to future logs and metrics.

Current implementation scope:

1. `ServerHeartbeatRttOffsetPolicyCommitBoundary::evaluate_and_commit`
   evaluates candidate policy first.
2. If policy accepts the candidate, latest estimate state commit proceeds and
   no rejected-candidate handoff is produced.
3. If policy rejects the candidate, state commit is skipped.
4. `ServerHeartbeatRttOffsetRejectedCandidateHandoffBoundary::prepare` can be
   called with the policy commit outcome.
5. For `Skipped(RejectedOutlier)`, it prepares:
   - `ServerHeartbeatRttOffsetRejectedCandidateLogInput`
   - `ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff`
6. For committed candidates, it returns `NotRejected`.
7. `ServerHeartbeatRttOffsetRejectedCandidateJsonLogEventBoundary` maps the log
   input to the `server.heartbeat_rtt_offset_rejected_candidate` JSON Lines
   event shape.
8. `ServerHeartbeatRttOffsetRejectedCandidateLogOutputBoundary` can write one
   event to a caller-owned writer.
9. `ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitBoundary` can commit
   the metrics handoff to in-memory per-client-run counters.
10. `ServerHeartbeatRttOffsetRejectedCandidateMetricsExportBoundary` can create
    a typed snapshot for future exporters or dashboards.

Responsibility split:

- candidate policy reject
  - Owns only the outlier decision and reason.
  - Does not write logs, update metrics, or mutate state.
- state commit skip
  - Preserves the previous latest estimate when policy rejects a candidate.
  - Does not decide whether to emit operational output.
- rejected candidate log boundary
  - Converts a skipped rejected candidate into one typed JSON Lines event.
  - Uses caller-owned writers only.
  - Does not open files, install a process-wide logger, or run a loop.
- metrics handoff
  - Names the future counter deltas:
    `rejected_candidates_delta = 1` and `skipped_commits_delta = 1`.
  - Does not store, aggregate, export, or display metrics.
- metrics state
  - Stores aggregated counters keyed by `(client_id, run_id)`.
  - Tracks total rejected candidates, total skipped commits, RTT-delta
    rejections, clock-offset-delta rejections, and the last update timestamp.
  - Does not inspect candidate values, write logs, export over a socket, persist
    records, or drive timeout decisions.
- metrics export placeholder
  - Creates a typed snapshot from the current in-memory metrics state.
  - Does not serialize, push to a backend, render a dashboard, or retain
    historical time series.
- export handoff
  - Wraps a non-empty snapshot with a selected consumer and optional export
    timestamp.
  - Returns `NoRecords` for empty state instead of creating an empty dashboard
    or loop event.
- future loop consumer
  - Receives the typed snapshot handoff for later cadence, logging, or fanout
    decisions.
  - Does not own the metrics state or dashboard rendering.
- future dashboard consumer
  - Receives a dashboard input placeholder with exported timestamp and records.
  - Does not render UI, store dashboard state, or define refresh transport.
- future timeout / heartbeat loop
  - May call the handoff boundary after each policy commit outcome.
  - Owns loop cadence, writer selection, metrics state ownership, export
    trigger timing, and backpressure.
- future dashboard
  - May consume snapshot records to show per-client / per-run outlier counts.
  - Does not belong to the current server-side metrics boundary.

Current storage / aggregation / export policy:

- Storage is caller-owned in-memory state:
  `ServerHeartbeatRttOffsetRejectedCandidateMetricsState`.
- Aggregation is additive and per client run. A new `run_id` creates a separate
  entry rather than merging with earlier runs.
- Reason-specific counters are intentionally minimal:
  `rtt_delta_rejections` and `clock_offset_delta_rejections`.
- Export is a snapshot placeholder:
  `ServerHeartbeatRttOffsetRejectedCandidateMetricsSnapshot`.
- Export handoff is explicit:
  `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary` selects a
  consumer and returns either `NoRecords` or a typed handoff.
- Snapshot consumption is still placeholder-only:
  `ServerHeartbeatRttOffsetMetricsSnapshotConsumerBoundary` routes a handoff to
  either a future loop handoff or a future dashboard input shape.
- No completed metrics pipeline exists yet. File sinks, process-wide metrics,
  network export, UI dashboard, alert thresholds, retention, and time-series
  history remain future work.

### Heartbeat RTT / Offset Metrics Snapshot Loop / Dashboard Handoff

The RTT / offset metrics snapshot handoff connects current in-memory
rejected-candidate metrics to future loop and dashboard consumers without
implementing a metrics pipeline or dashboard.

Current implementation scope:

1. `ServerHeartbeatRttOffsetRejectedCandidateMetricsState` remains the
   caller-owned in-memory aggregation state.
2. `ServerHeartbeatRttOffsetRejectedCandidateMetricsExportBoundary::snapshot`
   produces a typed snapshot.
3. `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary::export_for_consumer`
   receives:
   - current metrics state
   - `ServerHeartbeatRttOffsetMetricsSnapshotConsumer`
   - optional `exported_at`
4. If the snapshot is empty, it returns `NoRecords`.
5. If the snapshot has records, it returns
   `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoff`.
6. `ServerHeartbeatRttOffsetMetricsSnapshotConsumerBoundary::consume` routes
   the handoff to either:
   - `FutureLoop`, preserving the handoff for future loop fanout decisions
   - `FutureDashboard`, converting it to
     `ServerHeartbeatRttOffsetMetricsDashboardSnapshotInput`

Responsibility split:

- rejected candidate metrics state
  - Owns in-memory counters only.
  - Does not choose export cadence or dashboard refresh policy.
- snapshot export
  - Converts current state into immutable records.
  - Does not serialize, write, send, or render.
- export handoff
  - Names the intended consumer for one snapshot.
  - Does not run periodically or retain history.
- future loop consumer
  - Later owns export cadence, fanout, logging decisions, and backpressure.
  - Current placeholder only preserves the handoff.
- future dashboard consumer
  - Later owns dashboard storage, refresh transport, and UI rendering.
  - Current placeholder only names the dashboard input shape.

This keeps both completed metrics pipeline and dashboard implementation out of
scope while fixing the type-level connection the future loop can call.

### Continuous Heartbeat Loop Preflight Policy

The continuous heartbeat loop is still out of scope. Before implementing it,
the current boundary fixes only the cadence, stop condition, and log handoff
decisions that a future loop body will consume.

Client-side send cadence:

1. The future client loop supplies `ClientHeartbeatLoopCadenceInput`:
   - `heartbeat_interval_micros`
   - `ack_receive_timeout_micros`
   - `ClientHeartbeatAckObservationReturnMode`
2. `ClientHeartbeatLoopPolicyBoundary::evaluate` receives the cadence,
   caller-owned loop state, stop condition, client id, run id, and current
   client timestamp.
3. The boundary returns exactly one of:
   - `Stop`
   - `Wait`
   - `SendHeartbeat`
4. `SendHeartbeat` includes:
   - `send_at`
   - `ack_deadline_at`
   - selected ack observation return mode
   - one `ClientHeartbeatLoopLogHandoff`
5. The boundary does not encode or send `Heartbeat`, receive `HeartbeatAck`,
   create `HeartbeatAckObservation`, send `ClientStats`, sleep, retry, or start
   a loop.

Server-side timeout / metrics cadence:

1. The future server loop supplies
   `ServerHeartbeatContinuousLoopCadenceInput`:
   - `timeout_tick_interval_micros`
   - optional `metrics_snapshot_interval_micros`
2. `ServerHeartbeatContinuousLoopPolicyBoundary::evaluate` receives the
   cadence, caller-owned loop state, stop condition, and current server
   timestamp.
3. The boundary returns exactly one of:
   - `Stop`
   - `Wait`
   - `Run`
4. `Run` only says whether the future body should call:
   - `ServerHeartbeatTimeoutLoopTickBoundary`
   - `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary`
5. The boundary does not iterate clients, evaluate timeouts, apply timeout
   actions, store timeout notices, wake the send loop, export metrics, render a
   dashboard, write logs, sleep, or start a loop.

Stop conditions:

- Client:
  - external stop request
  - max sent heartbeat count
  - max missed ack count
  - otherwise run until stopped
- Server:
  - external stop request
  - max completed timeout tick count
  - otherwise run until stopped

Log output scope:

- The new policy boundaries produce typed log handoffs only.
- Client log handoff records `client_id`, `run_id`, observed timestamp,
  decision reason, heartbeat interval, ack timeout, sent heartbeat count,
  received ack count, and missed ack count.
- Server log handoff records observed timestamp, decision reason, timeout tick
  interval, optional metrics snapshot interval, completed timeout tick count,
  and exported metrics snapshot count.
- JSON Lines event names, file sinks, process-wide logger setup, and actual
  writer calls remain future work.

Responsibility split:

- heartbeat send cadence
  - Client policy chooses stop / wait / send for one future loop decision.
  - It does not construct or send the heartbeat packet.
- ack observation return
  - Client policy carries the chosen observation return mode.
  - `ClientHeartbeatAckObservationBoundary` and
    `ClientHeartbeatObservationCarrierBoundary` still own observation and
    `ClientStats` carrier construction after an ack is actually received.
- timeout loop tick
  - Server policy decides only whether timeout tick work is due.
  - `ServerHeartbeatTimeoutLoopTickBoundary` still owns one selected client's
    timeout evaluation / action plan / apply sequence.
- metrics snapshot handoff
  - Server policy decides only whether a metrics snapshot export is due.
  - `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary` still owns
    snapshot handoff creation for the selected consumer.
- future continuous loop body
  - Owns sleeping, iteration, socket I/O, state mutation order, writer
    selection, queue ownership, send wakeup execution, retry, and shutdown
    integration.

Current code reflects this with
`apps/client::ClientHeartbeatLoopPolicyBoundary` and
`apps/server::ServerHeartbeatContinuousLoopPolicyBoundary`. Both are
placeholder policy boundaries only; no completed continuous heartbeat loop is
implemented.

### Continuous Heartbeat Loop Ownership / Timeout / Retry Boundary

Before a completed continuous heartbeat loop is implemented, state ownership,
socket receive timeout, and retry handling are fixed as separate inputs. These
boundaries are intentionally data-only and do not move real sockets or execute
retry.

Client state ownership:

1. The future client heartbeat loop may start only after accepted auth and a
   bound UDP socket exist.
2. `ClientHeartbeatLoopOwnershipBoundary::evaluate` receives:
   - `client_id`
   - `run_id`
   - `protocol_version`
   - whether auth was accepted
   - whether the socket is bound
3. If auth is not accepted or no bound socket is available, it returns
   `NotReady`.
4. If ready, it returns a `ClientHeartbeatLoopOwnershipPlan` that names the
   future loop body as owner of:
   - UDP socket use for heartbeat/ack work
   - heartbeat loop counters
   - ack wait state
   - stats return state
5. The boundary does not move a `UdpSocket`, send heartbeats, wait for acks, or
   start the loop.

Client socket receive timeout:

1. `ClientHeartbeatAckReceiveTimeoutBoundary::plan_wait` receives:
   - current client timestamp
   - ack deadline timestamp
   - max socket wait duration
2. If the ack deadline has already elapsed, it returns `DeadlineElapsed`.
3. Otherwise it returns a receive timeout clamped to the smaller of:
   - remaining time before ack deadline
   - max socket wait duration
4. The boundary does not call `set_read_timeout`, receive UDP packets, decode
   `HeartbeatAck`, or classify socket errors.

Client retry placeholder:

1. `ClientHeartbeatLoopRetryBoundary::decide` receives a reason, attempts used,
   retry policy, and current timestamp.
2. If attempts are exhausted, it returns `GiveUp`.
3. Otherwise it returns `RetryLater` with the next attempt number and retry
   timestamp.
4. The boundary does not sleep, resend, recreate observations, or mutate loop
   state.

Server state ownership:

1. The future server heartbeat loop must be given caller-owned holders for:
   - authenticated sender registry
   - liveness state
   - outbound queue collection
   - timeout log writer
   - rejected-candidate metrics state
2. `ServerHeartbeatContinuousLoopOwnershipBoundary::evaluate` checks whether
   those holders are available.
3. If any holder is missing, it returns `NotReady` with the missing list.
4. If ready, it returns a `ServerHeartbeatContinuousLoopOwnershipPlan` naming
   those state holders as future loop-owned for the duration of the loop body.
5. The boundary does not scan clients, mutate registry state, store notices,
   write logs, export metrics, or start a loop.

Server socket receive timeout:

1. `ServerHeartbeatContinuousLoopSocketReceiveTimeoutBoundary::plan_wait`
   receives:
   - current server timestamp
   - next heartbeat work due timestamp
   - max socket receive wait duration
2. If heartbeat work is already due, it returns `HeartbeatWorkDueNow`.
3. Otherwise it returns a receive timeout clamped to the smaller of:
   - remaining time before heartbeat work is due
   - max socket receive wait duration
4. This prevents a future blocking receive wait from delaying timeout ticks or
   metrics snapshot handoff decisions beyond their due time.
5. The boundary does not call socket APIs, receive packets, run timeout ticks,
   or export metrics.

Server retry placeholder:

1. `ServerHeartbeatContinuousLoopRetryBoundary::decide` receives a reason,
   attempts used, retry policy, and current timestamp.
2. Retry reasons are placeholders for interrupted socket receive, timeout tick
   apply failure, notice queue storage failure, and metrics snapshot handoff
   failure.
3. The boundary returns either `RetryLater` or `GiveUp`.
4. It does not re-run timeout evaluation, requeue notices, wake send loops,
   export metrics, or sleep.

Responsibility split:

- heartbeat send cadence
  - Still belongs to `ClientHeartbeatLoopPolicyBoundary`.
  - Ownership / timeout / retry boundaries only prepare the surrounding loop
    state and failure handling shape.
- socket wait
  - Client ack wait and server receive wait are explicit timeout calculations.
  - Real socket configuration and `recv_from` calls remain future loop body
    work.
- ack receive
  - Future client loop body owns receiving and decoding `HeartbeatAck`.
  - Ack observation construction remains in `ClientHeartbeatAckObservationBoundary`.
- stats return
  - Future client loop body owns deciding when to send `ClientStats`.
  - Carrier construction remains in `ClientHeartbeatObservationCarrierBoundary`.
- timeout tick
  - Server ownership boundary ensures liveness/registry/log/queue state is
    available.
  - `ServerHeartbeatTimeoutLoopTickBoundary` still owns one-client timeout
    evaluation/action/apply once the loop selects a client.
- metrics handoff
  - Server ownership boundary ensures metrics state is available.
  - `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary` still owns
    snapshot handoff creation.
- future loop body
  - Owns real state mutation order, socket calls, sleeping, retry execution,
    shutdown integration, and backpressure.

Current code reflects this with
`apps/client::ClientHeartbeatLoopOwnershipBoundary`,
`apps/client::ClientHeartbeatAckReceiveTimeoutBoundary`,
`apps/client::ClientHeartbeatLoopRetryBoundary`,
`apps/server::ServerHeartbeatContinuousLoopOwnershipBoundary`,
`apps/server::ServerHeartbeatContinuousLoopSocketReceiveTimeoutBoundary`, and
`apps/server::ServerHeartbeatContinuousLoopRetryBoundary`.

### Continuous Heartbeat Loop One-Iteration Body Boundary

The one-iteration body boundary is the final pre-loop connection point before
implementing a completed continuous heartbeat loop. It composes the existing
preflight boundaries and emits typed handoffs for the future loop body, but it
still does not perform socket I/O or mutate long-lived runtime state.

Client one-iteration body:

1. `ClientHeartbeatLoopBodyBoundary::run_one` receives:
   - `ClientHeartbeatLoopOwnershipInput`
   - `ClientHeartbeatLoopPolicyInput`
   - max ack socket wait duration
2. The body first checks auth/socket readiness with
   `ClientHeartbeatLoopOwnershipBoundary`.
3. If ownership is not ready, it returns `OwnershipNotReady`.
4. If ownership is ready, it evaluates cadence and stop state with
   `ClientHeartbeatLoopPolicyBoundary`.
5. `Stop` and `Wait` are returned as runtime-shaped results without sending.
6. `SendHeartbeat` is converted into `ClientHeartbeatLoopBodySendHandoff`
   carrying:
   - `client_id`
   - `run_id`
   - `protocol_version`
   - heartbeat `send_at`
   - `ack_deadline_at`
   - `ClientHeartbeatAckReceiveTimeoutDecision`
   - ack observation return mode
7. The body uses `ClientHeartbeatAckReceiveTimeoutBoundary` only to calculate
   the future ack wait timeout.
8. It does not construct a `Heartbeat`, call protocol encode, send UDP,
   receive `HeartbeatAck`, create `HeartbeatAckObservation`, send
   `ClientStats`, sleep, or retry.

Server one-iteration body:

1. `ServerHeartbeatContinuousLoopBodyBoundary::run_one` receives:
   - `ServerHeartbeatContinuousLoopOwnershipInput`
   - `ServerHeartbeatContinuousLoopPolicyInput`
   - max socket receive wait duration
   - selected metrics snapshot consumer
2. The body first checks loop state holder availability with
   `ServerHeartbeatContinuousLoopOwnershipBoundary`.
3. If ownership is not ready, it returns `OwnershipNotReady`.
4. If ownership is ready, it evaluates cadence and stop state with
   `ServerHeartbeatContinuousLoopPolicyBoundary`.
5. `Stop` is returned without side effects.
6. `Wait` is converted into a socket wait decision through
   `ServerHeartbeatContinuousLoopSocketReceiveTimeoutBoundary`.
7. `Run` is converted into `ServerHeartbeatContinuousLoopBodyHandoff` with
   optional handoffs for:
   - timeout tick work
   - rejected-candidate metrics snapshot work
8. The timeout handoff records only the evaluation timestamp. The future body
   still selects clients and calls `ServerHeartbeatTimeoutLoopTickBoundary`.
9. The metrics handoff records the export timestamp and selected consumer. The
   future body still calls
   `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary`.
10. The body does not scan clients, apply timeout actions, store notices, wake
    send loops, receive packets, write logs, export metrics, sleep, or retry.

Responsibility split:

- client send cadence
  - `ClientHeartbeatLoopPolicyBoundary` decides stop / wait / send.
  - `ClientHeartbeatLoopBodyBoundary` only wraps the send decision in a typed
    handoff.
- auth precondition
  - `ClientHeartbeatLoopOwnershipBoundary` blocks body work when accepted auth
    or a bound socket is missing.
- ack receive
  - The body computes the future socket wait timeout only.
  - Actual receive/decode remains future loop body work.
- observation return
  - The body carries the selected return mode.
  - `HeartbeatAckObservation` creation and `ClientStats` send remain separate
    future body steps.
- server timeout tick
  - The server body emits a timeout tick handoff only.
  - Client iteration and `ServerHeartbeatTimeoutLoopTickBoundary` invocation
    remain future body work.
- metrics handoff
  - The server body emits a metrics snapshot handoff only.
  - Snapshot export and consumer routing remain in the existing metrics
    boundaries.
- future continuous loop body
  - Owns real socket calls, state mutation order, selected client iteration,
    actual `Heartbeat` / `ClientStats` send, `HeartbeatAck` receive, timeout
    apply, notice queue storage, send wakeup execution, retry execution,
    sleeping, and shutdown integration.

Current code reflects this with
`apps/client::ClientHeartbeatLoopBodyBoundary` and
`apps/server::ServerHeartbeatContinuousLoopBodyBoundary`. These are
one-iteration body boundaries only; no completed continuous heartbeat loop is
implemented.

### Client Heartbeat Encode / Send Handoff Boundary

The client heartbeat encode/send handoff connects the one-iteration body send
decision to one concrete heartbeat datagram send. It is still not a completed
continuous heartbeat loop and it does not wait for the ack.

Current implementation scope:

1. `ClientHeartbeatLoopBodyBoundary::run_one` may emit
   `ClientHeartbeatLoopBodySendHandoff`.
2. `ClientHeartbeatLoopEncodeSendBoundary::encode_handoff` receives:
   - destination socket address
   - body send handoff
   - optional client local time
   - optional short status
3. The boundary builds one `Heartbeat` using:
   - `client_id`
   - `run_id`
   - `protocol_version`
   - body `send_at`
   - supplied `local_time`
   - supplied `short_status`
4. The boundary encodes the heartbeat through `ProtocolMessageEncoderBoundary`.
5. The encoded handoff preserves:
   - destination
   - typed `Heartbeat`
   - encoded bytes
   - ack deadline
   - ack wait decision
   - ack observation return mode
6. `ClientHeartbeatLoopEncodeSendBoundary::send_one` performs one UDP
   `send_to` using the caller-owned socket and returns
   `ClientHeartbeatLoopEncodeSendRuntimeResult`.
7. The send result reports only the encoded handoff and sent byte count.

Responsibility split:

- heartbeat build
  - Owned by `ClientHeartbeatLoopEncodeSendBoundary`.
  - Uses timestamps and identity from the body handoff.
  - Does not decide cadence or auth readiness.
- protocol encode
  - Still owned by `ProtocolMessageEncoderBoundary`.
  - The client boundary only selects `ProtocolMessage::Heartbeat`.
- UDP send
  - `send_one` performs one caller-owned socket `send_to`.
  - It does not bind sockets, loop, retry, fragment, or encrypt.
- ack wait
  - The encode/send result carries `ack_wait` and `ack_deadline_at`.
  - It does not call `recv_from` or decode `HeartbeatAck`.
- observation return
  - The result carries `ClientHeartbeatAckObservationReturnMode`.
  - `HeartbeatAckObservation` creation and `ClientStats` return remain later
    future loop body steps.
- future loop body
  - Owns calling this boundary repeatedly, handling send failures, waiting for
    acks, creating observations, returning stats, retry execution, sleeping,
    and shutdown integration.

Current code reflects this with
`apps/client::ClientHeartbeatLoopEncodeSendBoundary`,
`ClientHeartbeatLoopEncodeSendInput`,
`ClientHeartbeatLoopEncodedSendHandoff`, and
`ClientHeartbeatLoopEncodeSendRuntimeResult`.

### Client Ack Receive / Observation Return Handoff Boundary

The client ack receive / observation return handoff connects one sent heartbeat
to the feedback path needed by server-side RTT / offset calculation. It is the
next step after heartbeat encode/send, but it is still not a completed
continuous heartbeat loop.

Current implementation scope:

1. `ClientHeartbeatLoopAckObservationReturnBoundary::receive_one` receives and
   decodes one `HeartbeatAck` from a caller-owned `UdpSocket`.
2. The boundary validates that the ack matches the sent heartbeat by checking:
   - `client_id`
   - `run_id`
   - `echoed_sent_at`
3. The boundary records a client receive timestamp and creates one
   `HeartbeatAckObservation` through
   `ClientHeartbeatAckObservationBoundary`.
4. If the original send handoff selected
   `ClientHeartbeatAckObservationReturnMode::Disabled`, the runtime result
   contains no `ClientStats` return.
5. If the original send handoff selected
   `ClientStatsOncePerAck`, the boundary:
   - wraps the observation with `ClientHeartbeatObservationCarrierBoundary`
   - builds one `ClientStats` payload with zeroed non-heartbeat stats fields
   - encodes the payload through `ProtocolMessageEncoderBoundary`
   - returns `ClientHeartbeatLoopClientStatsReturnHandoff`
6. The return handoff preserves the destination, typed `ClientStats`, and
   encoded bytes for a later send step.

Responsibility split:

- ack receive
  - `receive_one` owns one blocking `recv_from` through the existing client
    response decode helper.
  - It does not loop, retry, or alter socket timeout settings.
- decode
  - Existing protocol decode validates fixed header, protocol version, message
    type, and `HeartbeatAck` payload shape.
- observation build
  - `ClientHeartbeatAckObservationBoundary` owns conversion from ack timestamps
    plus client receive timestamp into `HeartbeatAckObservation`.
- `ClientStats` return
  - This boundary can build and encode a return datagram when the mode requests
    one.
  - It does not send the datagram.
- future loop body
  - Owns when to call ack receive, how to handle timeout/error retry, when to
    send the encoded `ClientStats` return, and how to update loop counters or
    shutdown state.

Current code reflects this with
`apps/client::ClientHeartbeatLoopAckObservationReturnBoundary`,
`ClientHeartbeatLoopAckObservationReturnInput`,
`ClientHeartbeatLoopClientStatsReturnHandoff`, and
`ClientHeartbeatLoopAckObservationReturnRuntimeResult`.

### Client Stats Return Send Handoff Boundary

The client stats return send handoff is the narrow send step after ack
observation return has already built and encoded a `ClientStats` datagram. It
does not encode telemetry and it does not continue the heartbeat loop.

Current implementation scope:

1. `ClientHeartbeatLoopAckObservationReturnBoundary` may produce
   `ClientHeartbeatLoopClientStatsReturnHandoff`.
2. The handoff contains:
   - destination socket address
   - typed `ClientStats`
   - encoded fixed-header + payload bytes
3. `ClientHeartbeatLoopClientStatsReturnSendBoundary::send_one` receives a
   caller-owned `UdpSocket` and the handoff.
4. It performs exactly one UDP `send_to`.
5. It returns `ClientHeartbeatLoopClientStatsReturnSendRuntimeResult` with the
   original handoff and sent byte count.

Responsibility split:

- `ClientStats` encode
  - Owned by `ClientHeartbeatLoopAckObservationReturnBoundary`.
  - The send boundary trusts the encoded bytes in the handoff.
- UDP send
  - Owned by `ClientHeartbeatLoopClientStatsReturnSendBoundary` for one
    datagram only.
  - It does not bind sockets, set timeouts, retry, fragment, or encrypt.
- ack observation return
  - Owns deciding whether a `ClientStats` return exists for the ack.
  - The send boundary does not inspect `HeartbeatAckObservation`.
- future loop body
  - Owns deciding when to call this send boundary, how to handle send errors,
    loop counter updates, retry execution, sleep, and shutdown integration.

Current code reflects this with
`apps/client::ClientHeartbeatLoopClientStatsReturnSendBoundary`,
`ClientHeartbeatLoopClientStatsReturnSendRuntimeResult`, and
`ClientHeartbeatLoopClientStatsReturnSendError`.

### Client Loop Iteration Result / Counters Boundary

The client loop iteration result / counters boundary records what happened in
one future heartbeat loop iteration after the already-separated step
boundaries have run. It is the state commit point for client-local counters
only; it is not the continuous loop body.

Current implementation scope:

1. `ClientHeartbeatLoopIterationRuntimeResult` can represent:
   - wait / stop decisions
   - one successful heartbeat send
   - one received `HeartbeatAck`
   - one missed ack
   - one sent `ClientStats` return
   - one classified step failure
2. `ClientHeartbeatLoopCountersState` records:
   - sent heartbeats
   - received acks
   - missed acks
   - sent `ClientStats` returns
   - heartbeat send / ack receive / stats return send failures
   - last heartbeat send / ack receive / stats return timestamps
3. `ClientHeartbeatLoopCountersBoundary::commit_result` applies exactly one
   iteration result to caller-owned counters and returns the before/after
   state.
4. `ClientHeartbeatLoopCountersState::as_policy_snapshot` exposes only the
   subset needed by `ClientHeartbeatLoopPolicyBoundary` for the next policy
   decision.

Responsibility split:

- heartbeat send
  - `ClientHeartbeatLoopEncodeSendBoundary` still builds, encodes, and sends
    one heartbeat datagram.
  - The counters boundary records `HeartbeatSent` only after that send step
    has succeeded.
- ack receive
  - `ClientHeartbeatLoopAckObservationReturnBoundary` still receives, decodes,
    correlates, and observes one `HeartbeatAck`.
  - The counters boundary records `AckReceived` only after that boundary
    returns a successful runtime result.
- observation return
  - `ClientHeartbeatLoopAckObservationReturnBoundary` still decides whether a
    `ClientStats` return handoff should be prepared for the ack.
  - The counters boundary records ack receipt separately from the later stats
    return send.
- `ClientStats` send
  - `ClientHeartbeatLoopClientStatsReturnSendBoundary` still sends one
    already-encoded `ClientStats` datagram.
  - The counters boundary records `ClientStatsReturnSent` only after that
    send succeeds.
- counters update
  - `ClientHeartbeatLoopCountersBoundary` owns the state mutation from a typed
    result to counters.
  - It does not call sockets, encode/decode, build observations, retry, sleep,
    write logs, or decide shutdown.
- future loop body
  - Owns the execution order across heartbeat send, ack wait, ack observation
    return, optional stats return send, timeout/error classification, retry
    execution, and when to feed the next policy snapshot back into
    `ClientHeartbeatLoopPolicyBoundary`.

Current code reflects this with
`apps/client::ClientHeartbeatLoopIterationRuntimeResult`,
`ClientHeartbeatLoopIterationFailureKind`,
`ClientHeartbeatLoopCountersState`,
`ClientHeartbeatLoopCountersUpdateOutcome`, and
`ClientHeartbeatLoopCountersBoundary`. A completed continuous heartbeat loop,
controller, sleep/timer integration, retry execution, and log output remain
future work.

### Client Loop Controller / Retry / Sleep Integration Boundary

The client loop controller / retry / sleep integration boundary fixes how the
already-separated client heartbeat steps will be connected before implementing
the completed continuous heartbeat loop. It produces typed plans only; it does
not run a loop or block the thread.

Current implementation scope:

1. `ClientHeartbeatLoopControllerBoundary::plan_next` receives one
   `ClientHeartbeatLoopBodyResult`.
2. `OwnershipNotReady` is returned as-is so the future loop controller can
   stop before socket work.
3. `Stop` becomes a `Stopped` iteration result that may be committed through
   `ClientHeartbeatLoopCountersBoundary`.
4. `Wait` becomes:
   - a bounded `ClientHeartbeatLoopSleepDecision`
   - a `Waited` iteration result
5. `SendHeartbeat` is passed through as the next typed handoff for
   `ClientHeartbeatLoopEncodeSendBoundary`.
6. `ClientHeartbeatLoopRetryApplyBoundary::apply_failure` receives one
   classified failure and connects it to:
   - a `Failed` iteration result for counters
   - a `ClientHeartbeatLoopRetryDecision`
   - a bounded retry sleep decision when retry is still allowed
7. `ClientHeartbeatLoopSleepBoundary::plan_sleep` converts a planned wake time
   into either `NoSleep` or one bounded sleep duration.

Responsibility split:

- heartbeat policy
  - `ClientHeartbeatLoopPolicyBoundary` still decides stop / wait / send from
    cadence and snapshot state.
  - The controller only consumes the body result derived from that policy.
- encode-send
  - `ClientHeartbeatLoopEncodeSendBoundary` still builds, encodes, and sends
    one heartbeat datagram.
  - The controller only passes the send handoff forward.
- ack receive
  - `ClientHeartbeatLoopAckObservationReturnBoundary` still receives and
    decodes one `HeartbeatAck`.
  - Retry apply can classify an ack timeout/decode failure after that step
    fails, but it does not perform the receive.
- stats return
  - `ClientHeartbeatLoopClientStatsReturnSendBoundary` still sends one encoded
    `ClientStats` return.
  - Retry apply can classify a stats return send failure, but it does not
    resend.
- counters update
  - `ClientHeartbeatLoopCountersBoundary` remains the only boundary that
    mutates caller-owned counters.
  - Controller and retry apply only produce iteration results that the caller
    may commit.
- sleep-retry
  - `ClientHeartbeatLoopSleepBoundary` plans sleep duration.
  - `ClientHeartbeatLoopRetryApplyBoundary` connects retry timing to that
    sleep plan.
  - Neither boundary calls `sleep`, `set_read_timeout`, or socket APIs.
- future loop body
  - Owns the actual order of controller planning, heartbeat send, ack wait,
    optional stats return, counters commit, retry execution, sleeping, stop
    handling, shutdown integration, and repeated iteration.

Current code reflects this with
`apps/client::ClientHeartbeatLoopControllerBoundary`,
`ClientHeartbeatLoopControllerPlan`,
`ClientHeartbeatLoopSleepBoundary`,
`ClientHeartbeatLoopSleepDecision`,
`ClientHeartbeatLoopRetryApplyBoundary`, and
`ClientHeartbeatLoopRetryApplyResult`. The completed continuous heartbeat loop,
actual sleeping, repeated retry execution, and socket timeout application
remain future work.

### Client Loop Logging / Shutdown Integration Boundary

Before implementing the completed continuous heartbeat loop, the client loop
now has a final pure connection point that turns a controller plan into:

- a controller action class
- an optional logging handoff
- a shutdown decision
- an optional iteration result that may be committed to counters

Current implementation scope:

1. `ClientHeartbeatLoopControllerBoundary::plan_next` still produces
   `ClientHeartbeatLoopControllerPlan`.
2. `ClientHeartbeatLoopControllerLogHandoffBoundary::prepare` observes that
   plan and prepares `ClientHeartbeatLoopControllerLogHandoff` for stop, wait,
   and send-heartbeat decisions.
3. `OwnershipNotReady` intentionally produces no client loop log handoff in
   this boundary; the future loop body may treat it as startup/precondition
   failure separately.
4. `ClientHeartbeatLoopShutdownDecisionBoundary::decide` maps only a controller
   `Stop` plan to `ClientHeartbeatLoopShutdownDecision::Stop`.
5. `Sleep`, `SendHeartbeat`, and `OwnershipNotReady` currently map to
   `Continue`; they do not execute shutdown.
6. `ClientHeartbeatLoopControllerResultBoundary::finalize` combines the
   original plan, action, optional log handoff, shutdown decision, and optional
   iteration result into `ClientHeartbeatLoopControllerResult`.
7. The boundary does not write JSON Lines, open sinks, install a process-wide
   logger, sleep, retry, mutate counters, execute shutdown, call sockets, or
   repeat.

Responsibility split:

- heartbeat policy
  - `ClientHeartbeatLoopPolicyBoundary` owns stop / wait / send selection from
    cadence, stop condition, and policy snapshot.
  - It produces the policy-level `ClientHeartbeatLoopLogHandoff`.
- one-iteration body
  - `ClientHeartbeatLoopBodyBoundary` owns auth/socket precondition checks and
    turns send policy into `ClientHeartbeatLoopBodySendHandoff`.
- encode-send
  - `ClientHeartbeatLoopEncodeSendBoundary` owns one heartbeat build, protocol
    encode, and one UDP `send_to`.
  - It does not write loop logs or decide shutdown.
- ack receive / observation return
  - `ClientHeartbeatLoopAckObservationReturnBoundary` owns one
    `HeartbeatAck` receive/decode/correlation step and optional encoded
    `ClientStats` return handoff.
  - It does not write loop logs or decide shutdown.
- stats return send
  - `ClientHeartbeatLoopClientStatsReturnSendBoundary` owns one send of an
    already encoded `ClientStats` return datagram.
  - It does not retry, log, or decide shutdown.
- counters update
  - `ClientHeartbeatLoopCountersBoundary` remains the only boundary that
    mutates caller-owned client loop counters.
  - It consumes typed iteration results after the future loop body decides to
    commit them.
- sleep-retry
  - `ClientHeartbeatLoopSleepBoundary` and
    `ClientHeartbeatLoopRetryApplyBoundary` only produce sleep / retry plans.
  - They do not execute timers or retry socket operations.
- logging
  - `ClientHeartbeatLoopControllerLogHandoffBoundary` prepares typed handoffs
    only.
  - JSON Lines event schema, caller-owned writer, sink selection, file open,
    rotation, buffering, and process-wide logger setup remain future work.
- shutdown
  - `ClientHeartbeatLoopShutdownDecisionBoundary` names the stop decision from
    controller output only.
  - Actual loop exit, resource cleanup, final log flush, and operator signal
    integration remain future work.
- future loop body
  - Owns repeated controller calls, heartbeat send, ack wait, optional stats
    return send, counters commit order, retry execution, real sleeping,
    shutdown execution, and log writer invocation.

Current code reflects this with
`apps/client::ClientHeartbeatLoopControllerAction`,
`ClientHeartbeatLoopControllerLogHandoff`,
`ClientHeartbeatLoopControllerLogHandoffBoundary`,
`ClientHeartbeatLoopShutdownDecision`,
`ClientHeartbeatLoopShutdownDecisionBoundary`,
`ClientHeartbeatLoopControllerResult`, and
`ClientHeartbeatLoopControllerResultBoundary`. A completed continuous heartbeat
loop, JSON Lines writer connection, actual shutdown execution, real timer
sleep, repeated retry execution, and socket timeout application remain future
work.

### Client Continuous Heartbeat Loop Minimal Runtime Scope

The client-side continuous heartbeat loop is not implemented as a completed
loop yet. The current minimal runtime scope is a single synchronous tick that
connects the existing boundaries once, using caller-owned state and sockets.
It is intentionally a one-call bridge, not a repeating loop.

Current implementation scope:

1. `ClientHeartbeatLoopOneTickRuntimeBoundary::run_one` receives:
   - caller-owned `UdpSocket`
   - caller-owned `ClientHeartbeatLoopCountersState`
   - `ClientHeartbeatLoopOneTickRuntimeInput`
2. It calls `ClientHeartbeatLoopBodyBoundary::run_one` to evaluate ownership,
   cadence, stop condition, and ack wait handoff.
3. It calls `ClientHeartbeatLoopControllerBoundary::plan_next` and
   `ClientHeartbeatLoopControllerResultBoundary::finalize` to produce the
   controller result, typed log handoff, shutdown decision, and optional
   controller-level iteration result.
4. For `Stop` and `Sleep`, it commits the controller-level iteration result to
   counters and returns without sleeping or exiting a process.
5. For `SendHeartbeat`, it calls
   `ClientHeartbeatLoopEncodeSendBoundary::send_one` once.
6. On heartbeat send success, it commits `HeartbeatSent` to counters and then
   calls `ClientHeartbeatLoopAckObservationReturnBoundary::receive_one` unless
   the ack deadline was already elapsed.
7. On ack success, it commits `AckReceived` to counters and, if a
   `ClientStats` return handoff exists, calls
   `ClientHeartbeatLoopClientStatsReturnSendBoundary::send_one` once.
8. On stats return send success, it commits `ClientStatsReturnSent` to
   counters.
9. On heartbeat send, ack receive/decode/correlation, or stats return send
   failure, it calls `ClientHeartbeatLoopRetryApplyBoundary::apply_failure`,
   commits the produced failure iteration result to counters, and returns the
   retry plan without executing it.
10. Ack receive timeout additionally commits `AckMissed` before the retry
    failure result, so missed ack counters remain visible to later policy
    snapshots.

Responsibility split:

- controller / body
  - Own precondition, policy, send handoff, bounded sleep decision, log handoff,
    and shutdown decision.
  - They do not run the repeated loop.
- encode-send
  - Owns one heartbeat build / encode / UDP send.
- ack receive
  - Owns one blocking ack receive / decode / correlation through the caller
    socket.
  - The one-tick runtime does not set socket timeout; the caller remains
    responsible for socket timeout configuration.
- stats return
  - Owns one optional already-encoded `ClientStats` return send.
- counters
  - The one-tick runtime is allowed to call
    `ClientHeartbeatLoopCountersBoundary::commit_result` in the order each
    one-step operation completes.
- sleep-retry
  - Retry apply returns typed retry and sleep decisions only.
  - No retry operation or timer sleep is executed by the one-tick runtime.
- logging
  - The runtime returns the controller log handoff inside
    `ClientHeartbeatLoopControllerResult`.
  - It does not serialize JSON Lines or choose sinks.
- shutdown
  - The runtime returns `ClientHeartbeatLoopShutdownDecision`.
  - It does not clean up resources, flush logs, or stop an outer loop.
- future completed loop body
  - Will own repeated calls to the one-tick runtime, real sleep/timer
    execution, socket timeout application, retry execution, log writer
    invocation, shutdown cleanup, and reconnect behavior.

Current code reflects this with
`apps/client::ClientHeartbeatLoopOneTickRuntimeInput`,
`ClientHeartbeatLoopOneTickRuntimeFailure`,
`ClientHeartbeatLoopOneTickRuntimeResult`, and
`ClientHeartbeatLoopOneTickRuntimeBoundary`. Completed continuous looping,
async runtime integration, real timer sleep, shutdown cleanup, reconnect,
JSON Lines writer invocation, and video sending remain future work.

### Client One-Tick Runtime CLI / Config Entry

The client-side one-tick runtime now has a minimal launcher and CLI/config
entry for manual checks. This still does not create a completed continuous
heartbeat loop.

Current launcher scope:

1. `ClientHeartbeatOneTickRuntimeLauncher` reads the existing client auth/PoC
   TOML shape and reuses:
   - `client.server_host`
   - `client.server_port`
   - `client.client_id`
   - `client.shared_token`
   - `session.run_id`
   - `session.app_version`
   - `session.protocol_version`
   - `network.heartbeat_interval_ms`
   - `network.connect_timeout_ms`
2. The launcher binds one caller-local UDP socket, sends one `AuthRequest`,
   waits for one accepted `AuthResponse`, and only then delegates one tick to
   `ClientHeartbeatLoopOneTickRuntimeBoundary`.
3. `network.heartbeat_interval_ms` feeds:
   - heartbeat cadence
   - one-tick `max_sleep_micros`
   - placeholder retry delay
4. `network.connect_timeout_ms` feeds:
   - auth response socket timeout
   - one-tick ack wait clamp
5. The launcher supports two explicit modes:
   - `--auth-heartbeat-one-tick-runtime`
     - pairs with server `--receive-send-twice`
     - executes auth + one heartbeat send + one ack receive
   - `--auth-heartbeat-stats-one-tick-runtime`
     - pairs with server `--receive-send-three`
     - executes auth + one heartbeat send + one ack receive + one
       `ClientStats` observation return send
6. CLI stdout reports only the one-tick runtime outcome:
   - auth request/response byte counts
   - controller action / shutdown decision
   - heartbeat / ack / optional stats byte counts
   - final one-tick counters snapshot

Responsibility split:

- launcher / config entry
  - Owns config load, destination resolution, auth bootstrap, and the single
    delegation to the one-tick runtime.
  - Does not repeat, reconnect, sleep, flush logs, or clean up a completed
    loop.
- one-tick runtime
  - Owns one body/controller/send/ack/stats/counters/retry pass.
- future completed loop
  - Will own repeated launcher/runtime calls, reconnect, timer execution,
    shutdown cleanup, and log writer invocation.

### Client Launcher / Repeated Loop Ownership

Before implementing the completed continuous heartbeat loop, launcher
ownership and repeated-loop ownership are separated explicitly so the one-tick
entry does not silently become the loop owner.

Minimal ownership boundary:

1. `ClientHeartbeatLoopLauncherOwnershipBoundary` receives:
   - accepted-auth bootstrap result
   - socket bound readiness
   - static cadence / retry / short-status config
2. It reuses `ClientHeartbeatLoopOwnershipBoundary` only to prove:
   - auth was accepted
   - a UDP socket is already bound
3. On success it produces `ClientHeartbeatLoopRepeatedRuntimeHandoff`.
4. On failure it returns `ClientHeartbeatLoopOwnershipDecision::NotReady` and
   does not produce repeated-loop handoff state.

Handoff scope:

- `ClientHeartbeatLoopRepeatedRuntimeHandoff`
  - Owns only static runtime material:
    - destination
    - client/run/protocol identity
    - cadence
    - stop condition
    - retry policy
    - short-status / local-time mode
  - Does not own:
    - real `UdpSocket`
    - counters state
    - current timestamp
    - retry attempt count
    - shutdown execution
- `build_one_tick_input(now, state, retry_attempts_used)`
  - Exists only to show how a future repeated loop converts its own current
    time, current counters snapshot, and retry state into one
    `ClientHeartbeatLoopOneTickRuntimeInput`.

Responsibility split:

- config load
  - `ClientHeartbeatOneTickRuntimeLauncher` owns TOML load and destination
    resolution.
- socket ownership
  - Launcher owns the first ephemeral UDP bind and auth bootstrap on that
    socket.
  - Future repeated loop will own continuing use of the already-bound socket
    after launcher/bootstrap work is complete.
- one-tick runtime
  - Owns one synchronous loop step only.
  - Receives caller-owned socket/counters and never claims repeated ownership.
- future repeated loop
  - Will own:
    - persistent socket lifetime
    - counters state lifetime
    - current time generation for each tick
    - repeated calls to `build_one_tick_input(...)`
    - stop/shutdown orchestration
    - retry execution, reconnect, sleep/timer execution
- shutdown responsibility
  - Launcher owns auth-bootstrap failure handling and may return without ever
    producing a repeated-loop handoff.
  - One-tick runtime returns only `ClientHeartbeatLoopShutdownDecision`.
  - Future repeated loop will decide whether to stop the process/worker,
    flush logs, and clean up resources.

### Client Repeated Loop Body Minimal Scope

Before implementing the completed continuous heartbeat loop, the future
repeated loop body is fixed as one narrow bridge that delegates exactly one
step to the existing one-tick runtime.

Current minimal scope:

1. `ClientHeartbeatLoopRepeatedRuntimeBodyBoundary` receives:
   - caller-owned `UdpSocket`
   - caller-owned `ClientHeartbeatLoopCountersState`
   - `ClientHeartbeatLoopRepeatedRuntimeBodyInput`
2. `ClientHeartbeatLoopRepeatedRuntimeBodyInput` carries only dynamic per-step
   values that the launcher does not own:
   - `now`
   - `stop_requested`
   - `retry_attempts_used`
   - previously prepared `ClientHeartbeatLoopRepeatedRuntimeHandoff`
3. The body snapshots counters through
   `ClientHeartbeatLoopCountersState::as_policy_snapshot(stop_requested)`.
4. It calls `ClientHeartbeatLoopRepeatedRuntimeHandoff::build_one_tick_input`
   once.
5. It delegates that one input to
   `ClientHeartbeatLoopOneTickRuntimeBoundary::run_one`.
6. It returns `ClientHeartbeatLoopRepeatedRuntimeBodyResult` containing:
   - the repeated-loop handoff
   - the exact one-tick input used for the call
   - the one-tick runtime result
   - the returned shutdown decision

Responsibility split:

- launcher ownership
  - Produces static repeated-loop handoff after accepted auth/bootstrap.
  - Does not own per-iteration `now`, stop flags, or retry-attempt counters.
- repeated-loop body
  - Owns only one-iteration bridging from dynamic loop state to one-tick
    runtime input.
  - Does not repeat, sleep, reconnect, mutate process lifetime, or execute
    shutdown.
- one-tick runtime
  - Owns the existing body/controller/send/ack/stats/counters/retry sequence.
- shutdown responsibility
  - Repeated-loop body returns `ClientHeartbeatLoopShutdownDecision` unchanged.
  - A future outer repeated loop will decide whether to stop iteration, flush
    logs, close sockets, or exit a worker/process.

Current code reflects this with
`ClientHeartbeatLoopRepeatedRuntimeBodyInput`,
`ClientHeartbeatLoopRepeatedRuntimeBodyResult`, and
`ClientHeartbeatLoopRepeatedRuntimeBodyBoundary`. The completed continuous
heartbeat loop, timer execution, repeated retry execution, reconnect, and
shutdown cleanup remain future work.

### Client Outer Repeated Loop Controller / Shutdown Apply Minimal Scope

After one repeated-loop body step returns, the future outer repeated loop still
needs one minimal orchestration layer that decides whether iteration may
continue and whether shutdown work must be applied. The current scope fixes
that bridge without implementing the completed loop.

Current minimal scope:

1. `ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary` receives:
   - caller-owned `UdpSocket`
   - caller-owned `ClientHeartbeatLoopCountersState`
   - `ClientHeartbeatLoopRepeatedRuntimeBodyInput`
2. It calls `ClientHeartbeatLoopRepeatedRuntimeBodyBoundary::run_one`.
3. It passes the returned body result to
   `ClientHeartbeatLoopOuterControllerBoundary::observe`.
4. The outer controller maps only:
   - `ClientHeartbeatLoopShutdownDecision::Continue` ->
     `ContinueLoop`
   - `ClientHeartbeatLoopShutdownDecision::Stop` ->
     `StopLoop`
5. It passes that same shutdown decision to
   `ClientHeartbeatLoopShutdownApplyBoundary::apply`.
6. Shutdown apply returns typed work only:
   - `ContinueLoop`
   - `StopLoop { reason, cleanup_required }`
7. The step returns `ClientHeartbeatLoopRepeatedRuntimeLoopStepResult`
   containing:
   - repeated body result
   - outer controller result
   - shutdown apply result

Responsibility split:

- launcher ownership
  - Produces static repeated-loop handoff after accepted auth/bootstrap.
- repeated-loop body
  - Produces one body/runtime/shutdown result from one dynamic iteration input.
- outer controller
  - Classifies one body result as continue-loop or stop-loop.
  - Does not sleep, retry, reconnect, or execute cleanup.
- shutdown apply
  - Converts one shutdown decision into typed future apply work.
  - Does not flush logs, close sockets, or stop a real worker/process.
- future completed loop
  - Will own repetition, backoff/timer execution, reconnect, cleanup ordering,
    and the actual application of shutdown work.

Current code reflects this with
`ClientHeartbeatLoopOuterControllerBoundary`,
`ClientHeartbeatLoopShutdownApplyResult`,
`ClientHeartbeatLoopShutdownApplyBoundary`,
`ClientHeartbeatLoopRepeatedRuntimeLoopStepResult`, and
`ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary`.

### Client Completed Loop Lifecycle Minimal Scope

After the outer controller and shutdown-apply step finish, the future completed
loop still needs one lifecycle boundary that decides whether the next
iteration may begin or whether stop/cleanup flow should start. The current
scope fixes that decision without implementing the completed loop.

Current minimal scope:

1. `ClientHeartbeatLoopLifecycleBoundary` receives
   `ClientHeartbeatLoopLifecycleInput`:
   - caller-owned `continue_requested`
   - one `ClientHeartbeatLoopRepeatedRuntimeLoopStepResult`
2. If `continue_requested = false`, lifecycle returns stop immediately with
   `CallerRequestedStop`.
3. Otherwise it inspects `shutdown_apply` from the step result:
   - `ContinueLoop` keeps lifecycle in continue state
   - `StopLoop { reason, .. }` becomes
     `PolicyRequestedStop { reason }`
4. It returns `ClientHeartbeatLoopLifecycleResult` containing:
   - the preserved loop-step result
   - `continue_loop`
   - optional `stop_reason`
   - `cleanup_required`

Responsibility split:

- launcher ownership
  - Produces static repeated-loop handoff after accepted auth/bootstrap.
- repeated-loop body
  - Delegates one dynamic iteration to one-tick runtime.
- outer controller / shutdown apply
  - Classify one step and name future stop/apply work.
- lifecycle
  - Decides only whether the future completed loop would start another
    iteration or enter stop/cleanup flow.
  - Does not perform cleanup, close sockets, flush logs, or run a real loop.
- future completed loop lifecycle
  - Will own actual while-loop repetition, stop sequencing, cleanup ordering,
    and worker/process lifetime transitions.

Current code reflects this with
`ClientHeartbeatLoopLifecycleStopReason`,
`ClientHeartbeatLoopLifecycleInput`,
`ClientHeartbeatLoopLifecycleResult`, and
`ClientHeartbeatLoopLifecycleBoundary`.

### Client Timer / Retry / Cleanup Sequencing Minimal Scope

After lifecycle decides continue or stop, the future completed loop still
needs one thin sequencing layer that tells the caller which follow-up branch
would run next. The current scope fixes that handoff without introducing real
timers, retry execution, reconnects, or cleanup work.

Current minimal scope:

1. `ClientHeartbeatLoopSequencingBoundary` receives one
   `ClientHeartbeatLoopLifecycleResult`.
2. If lifecycle already stopped:
   - `timer_wait = NoWait`
   - `retry_execution = NoRetryScheduled`
   - `cleanup = BeginCleanup { stop_reason }`
3. If lifecycle continues and one-tick runtime produced retry work:
   - sequencing preserves `ClientHeartbeatLoopRetryApplyResult`
   - retry sleep wins over controller cadence sleep
4. If lifecycle continues and no retry is scheduled:
   - sequencing inspects controller `Sleep` plan
   - bounded sleep becomes `Wait { sleep }`
   - non-sleep plans stay `NoWait`
5. It returns `ClientHeartbeatLoopSequencingResult` containing:
   - the preserved lifecycle result
   - `timer_wait`
   - `retry_execution`
   - `cleanup`

Responsibility split:

- lifecycle
  - Decides continue vs stop and whether cleanup is required.
- timer wait
  - Selects only the next wait handoff for cadence or retry backoff.
  - Does not block the thread or own a timer implementation.
- retry execution
  - Carries typed retry work from one-tick runtime into future completed-loop
    orchestration.
  - Does not re-run the failed operation.
- cleanup sequencing
  - Carries typed stop reason into future shutdown / flush / socket-close work.
  - Does not execute cleanup.
- future completed loop body
  - Will consume lifecycle plus sequencing output to run actual sleep, retry,
    reconnect, cleanup ordering, and process lifetime transitions.

Current code reflects this with `ClientHeartbeatLoopTimerWaitDecision`,
`ClientHeartbeatLoopRetryExecutionResult`,
`ClientHeartbeatLoopCleanupSequencingResult`,
`ClientHeartbeatLoopSequencingResult`, and
`ClientHeartbeatLoopSequencingBoundary`.

### Client Completed Loop Body Ordering Minimal Scope

After sequencing decides stop, retry, wait, or no-wait, the future completed
loop still needs one ordering layer that tells the completed body what to call
next. The current scope fixes that order without adding a real while-loop or
executing timer / retry / cleanup work.

Current minimal scope:

1. `ClientHeartbeatLoopStepOrderingBoundary` receives one
   `ClientHeartbeatLoopSequencingResult`.
2. If sequencing already entered cleanup:
   - ordering returns `Stop`
   - stop result preserves `stop_reason`
   - caller can later hand this into real cleanup execution
3. If retry work is scheduled:
   - ordering returns `RetryThenContinue { retry }`
   - retry ordering wins over timer-wait ordering
4. If no retry is scheduled but `timer_wait = Wait { sleep }`:
   - ordering returns `WaitThenContinue { sleep }`
5. Otherwise:
   - ordering returns `ContinueImmediately`
6. It returns either:
   - `ClientHeartbeatLoopStepOrderingResult::Continue { handoff }`
   - `ClientHeartbeatLoopStepOrderingResult::Stop { result }`

Responsibility split:

- lifecycle
  - Decides continue vs stop and cleanup requirement.
- sequencing
  - Names typed timer / retry / cleanup follow-up work.
- future completed loop body
  - Consumes ordering output and invokes the next concrete branch in order.
  - Will later own actual timer wait call, retry execution call, cleanup call,
    and one-step repetition contract.
- eventual while-loop
  - Will own repeated invocation, caller stop flag refresh, socket lifetime,
    and process shutdown boundary.

Current code reflects this with `ClientHeartbeatLoopStepOrdering`,
`ClientHeartbeatLoopCompletedBodySequencingHandoff`,
`ClientHeartbeatLoopCompletedBodyStopResult`,
`ClientHeartbeatLoopStepOrderingResult`, and
`ClientHeartbeatLoopStepOrderingBoundary`.

### Client Completed Step Runtime Minimal Scope

Before a real completed continuous heartbeat loop exists, the client side
still needs one thin runtime that connects the already-separated boundaries in
the same order the future loop will use. The current scope runs exactly one
completed-loop-equivalent step and returns the typed decision to the caller.

Current minimal scope:

1. `ClientHeartbeatLoopCompletedStepRuntimeBoundary` receives:
   - caller-owned `UdpSocket`
   - caller-owned `ClientHeartbeatLoopCountersState`
   - `ClientHeartbeatLoopCompletedStepRuntimeInput`
2. It runs exactly one `ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary`.
3. It passes that step result through:
   - `ClientHeartbeatLoopLifecycleBoundary`
   - `ClientHeartbeatLoopSequencingBoundary`
   - `ClientHeartbeatLoopStepOrderingBoundary`
4. It returns `ClientHeartbeatLoopCompletedStepRuntimeResult` containing:
   - preserved repeated-loop step result
   - lifecycle result
   - sequencing result
   - ordering result
   - final counters snapshot

Responsibility split:

- launcher ownership
  - Produces static repeated-loop handoff and caller-owned socket/counters.
- repeated body
  - Executes one dynamic iteration through one-tick runtime.
- outer controller / shutdown apply
  - Classify one repeated step and name stop/apply work.
- lifecycle
  - Decide continue vs stop.
- sequencing
  - Name typed timer / retry / cleanup follow-up work.
- ordering
  - Fix the next branch the future completed body would call.
- future completed loop body
  - Will own actual timer / retry / cleanup invocation using the ordered
    result.
- future completed loop body / eventual while-loop
  - Will later own repeated invocation, stop flag refresh, and worker/process
    lifetime.

Current code reflects this with `ClientHeartbeatLoopCompletedStepRuntimeInput`,
`ClientHeartbeatLoopCompletedStepRuntimeResult`, and
`ClientHeartbeatLoopCompletedStepRuntimeBoundary`.

### Client While-Loop Ownership / Caller Contract Minimal Scope

After one completed-step runtime finishes, the future eventual while-loop
still needs one caller contract boundary that says whether the caller keeps
ownership for another step or hands stop state into cleanup flow. The current
scope fixes that contract without implementing repeated invocation.

Current minimal scope:

1. `ClientHeartbeatLoopWhileLoopOwnershipBoundary` receives one
   `ClientHeartbeatLoopCompletedStepRuntimeResult`.
2. If completed-step ordering returned `Continue { handoff }`:
   - caller contract returns `Continue`
   - it preserves the ordering handoff
   - it preserves final counters snapshot
3. If completed-step ordering returned `Stop { result }`:
   - caller contract returns `Stop`
   - it wraps stop state into `ClientHeartbeatLoopWhileLoopStopHandoff`
   - cleanup responsibility moves to the future caller/cleanup layer

Responsibility split:

- launcher ownership
  - Produces static loop handoff and initial caller-owned socket/counters.
- completed-step runtime
  - Executes exactly one repeated step plus lifecycle / sequencing / ordering.
- eventual while-loop
  - Will own repeated invocation, stop-flag refresh, and next-step scheduling.
- caller contract
  - Tells the caller whether it still owns another step or must hand off stop
    state.
- cleanup responsibility
  - Starts only after stop handoff is returned.
  - Does not execute inside the current ownership boundary.

Current code reflects this with `ClientHeartbeatLoopWhileLoopStopHandoff`,
`ClientHeartbeatLoopCallerContractResult`, and
`ClientHeartbeatLoopWhileLoopOwnershipBoundary`.

### Client Repeated Invocation Skeleton Minimal Scope

After caller contract says continue or stop, the eventual while-loop still
needs one tiny skeleton layer that refreshes caller-owned stop input and
builds the next iteration carry state. The current scope fixes that data flow
without implementing repeated invocation.

Current minimal scope:

1. `ClientHeartbeatLoopSkeletonBoundary` receives:
   - one `ClientHeartbeatLoopCallerContractResult`
   - one `ClientHeartbeatLoopStopRefreshInput`
2. If caller contract is `Stop`:
   - skeleton returns `Stop`
   - stop handoff is preserved unchanged for future cleanup ownership
3. If caller contract is `Continue`:
   - skeleton returns `Continue { carry }`
   - carry preserves the prior ordering
   - carry preserves final counters snapshot
   - carry builds the next `ClientHeartbeatLoopCompletedStepRuntimeInput`
4. Retry attempt carry rules:
   - `ContinueImmediately` and `WaitThenContinue` reset
     `retry_attempts_used = 0`
   - `RetryThenContinue` carries the next retry attempt count forward
5. Stop flag refresh rules:
   - `continue_requested = !stop_requested`
   - `body.stop_requested = stop_requested`
   - `body.now = refresh.now`

Responsibility split:

- completed-step runtime
  - Produces one typed step result.
- caller contract
  - Converts one step result into continue vs stop ownership.
- repeated invocation skeleton
  - Refreshes caller stop input and builds next carry state only.
  - Does not run another iteration by itself.
- future cleanup responsibility
  - Starts only after skeleton returns `Stop`.
  - Remains outside the current boundary.

Current code reflects this with `ClientHeartbeatLoopStopRefreshInput`,
`ClientHeartbeatLoopIterationCarryState`,
`ClientHeartbeatLoopSkeletonResult`, and
`ClientHeartbeatLoopSkeletonBoundary`.

### Client Actual Apply Call Order Minimal Scope

After repeated invocation skeleton builds carry or stop state, the future
runtime still needs one thin layer that decides which apply branch would run
next. The current scope fixes that call order without executing timer waits,
retry work, or cleanup.

Current minimal scope:

1. `ClientHeartbeatLoopApplyOrderBoundary` receives one
   `ClientHeartbeatLoopSkeletonResult`.
2. If skeleton returns `Stop { handoff }`:
   - apply order returns `TriggerCleanup`
   - stop handoff is wrapped into `ClientHeartbeatLoopCleanupTrigger`
3. If skeleton returns `Continue { carry }`:
   - `ContinueImmediately` becomes `ContinueWithoutApply`
   - `WaitThenContinue { sleep }` becomes `ApplyTimerThenContinue`
   - `RetryThenContinue { retry }` becomes `ApplyRetryThenContinue`
4. The returned result is typed only:
   - no timer wait is executed
   - no retry is executed
   - no cleanup is executed

Responsibility split:

- sequencing
  - Names typed timer / retry / cleanup work.
- ordering
  - Chooses the next logical branch.
- caller contract
  - Hands that branch to eventual while-loop ownership.
- repeated invocation skeleton
  - Refreshes stop flag and builds next carry state.
- future actual apply order
  - Decides which apply branch would run next.
  - Does not perform the apply itself.

Current code reflects this with `ClientHeartbeatLoopCleanupTrigger`,
`ClientHeartbeatLoopApplyOrderResult`, and
`ClientHeartbeatLoopApplyOrderBoundary`.

### Client Completed Continuous Loop Outer Shell Minimal Scope

After apply order decides the next branch, the caller still needs one thin
outer shell result that says whether a future completed continuous heartbeat
loop would continue or stop. The current scope fixes that caller-facing
handoff without implementing repeated execution.

Current minimal scope:

1. `ClientHeartbeatLoopOuterShellBoundary` receives one
   `ClientHeartbeatLoopApplyOrderResult`.
2. If apply order returns:
   - `ContinueWithoutApply`
   - `ApplyTimerThenContinue`
   - `ApplyRetryThenContinue`
   then outer shell returns `Continue` and preserves that apply-order result.
3. If apply order returns `TriggerCleanup { trigger }`:
   - outer shell returns `Stop`
   - stop reason becomes `CleanupRequested`
   - cleanup trigger is preserved unchanged for future cleanup ownership
4. The returned shell result is typed only:
   - no loop repetition is executed
   - no timer wait is executed
   - no retry is executed
   - no cleanup is executed

Responsibility split:

- lifecycle
  - Decides whether one repeated step continues or stops.
- sequencing
  - Names timer / retry / cleanup follow-up work.
- ordering
  - Fixes the next logical branch.
- caller contract
  - Converts one completed step into caller-owned continue vs stop state.
- repeated invocation skeleton
  - Refreshes stop input and builds next iteration carry state.
- apply order
  - Decides which apply branch would run next.
- outer shell
  - Converts apply order into caller-facing continue vs stop.
  - Does not execute the loop body or cleanup.

Current code reflects this with `ClientHeartbeatLoopShellStopReason`,
`ClientHeartbeatLoopShellResult`, and
`ClientHeartbeatLoopOuterShellBoundary`.

### Client Caller-Facing Shell Runner Minimal Scope

After outer shell produces a typed continue-or-stop result, the caller still
needs one minimal runner entry that it can invoke directly before a real
completed continuous loop exists. The current scope keeps that entry thin: it
invokes outer shell exactly once and returns a caller-facing result.

Current minimal scope:

1. `ClientHeartbeatLoopShellRunnerBoundary` receives one
   `ClientHeartbeatLoopApplyOrderResult`.
2. Runner calls `ClientHeartbeatLoopOuterShellBoundary` exactly once.
3. If outer shell returns `Continue { apply_order }`:
   - runner returns `Continue`
   - apply-order result is preserved unchanged for future repeated invocation
4. If outer shell returns `Stop { reason, trigger }`:
   - runner returns `Stop`
   - stop reason is converted into runner-owned stop reason
   - cleanup trigger is preserved unchanged for future cleanup ownership
5. The runner remains typed only:
   - no repeated invocation is executed
   - no timer wait is executed
   - no retry is executed
   - no cleanup is executed

Responsibility split:

- outer shell
  - Converts apply order into typed continue vs stop.
- caller-facing shell runner
  - Owns the direct caller entry above outer shell.
  - Returns the result that future caller-owned loop orchestration will consume.
- eventual repeated invocation
  - Will decide whether and when to call the next shell runner turn.
  - Remains outside the current boundary.
- cleanup responsibility
  - Starts only after runner returns `Stop`.
  - Remains outside the current boundary.

Current code reflects this with `ClientHeartbeatLoopShellRunnerStopReason`,
`ClientHeartbeatLoopShellRunnerResult`, and
`ClientHeartbeatLoopShellRunnerBoundary`.

### Client Eventual Repeated Invocation Minimal Scope

After shell runner returns a caller-facing continue-or-stop result, the future
runtime still needs one thin layer that turns that result into repeated
invocation carry or cleanup stop handoff. The current scope fixes that mapping
without implementing a real while-loop.

Current minimal scope:

1. `ClientHeartbeatLoopRepeatedInvocationBoundary` receives one
   `ClientHeartbeatLoopShellRunnerResult`.
2. If shell runner returns `Continue { apply_order }`:
   - repeated invocation returns `Continue`
   - continue state is narrowed into
     `ClientHeartbeatLoopRepeatedInvocationNextStepCarry`
   - next-step carry preserves the branch-specific data:
     - immediate continue
     - timer-then-continue
     - retry-then-continue
3. If shell runner returns `Stop { reason, trigger }`:
   - repeated invocation returns `Stop`
   - stop reason is converted into repeated-invocation-owned stop reason
   - cleanup trigger is preserved unchanged
4. The returned state is typed only:
   - no actual repeated invocation is executed
   - no timer wait is executed
   - no retry is executed
   - no cleanup is executed

Responsibility split:

- shell runner
  - Exposes one caller-facing turn above outer shell.
- repeated invocation
  - Converts one runner turn into next-step carry or stop handoff.
  - Does not run the next turn by itself.
- future actual while-loop
  - Will own repeated execution and decide when to call the next runner turn.
  - Remains outside the current boundary.
- cleanup responsibility
  - Starts only after repeated invocation returns `Stop`.
  - Remains outside the current boundary.

Current code reflects this with
`ClientHeartbeatLoopRepeatedInvocationStopReason`,
`ClientHeartbeatLoopRepeatedInvocationNextStepCarry`,
`ClientHeartbeatLoopRepeatedInvocationResult`, and
`ClientHeartbeatLoopRepeatedInvocationBoundary`.

### Client Future Actual While-Loop Minimal Scope

After repeated invocation returns typed continue-or-stop state, the future
runtime still needs one smallest caller-facing while-loop step that says
whether the next iteration is still caller-owned or cleanup ownership should
begin. The current scope fixes that handoff without implementing real loop
repetition.

Current minimal scope:

1. `ClientHeartbeatLoopActualWhileLoopBoundary` receives one
   `ClientHeartbeatLoopRepeatedInvocationResult`.
2. If repeated invocation returns `Continue { carry }`:
   - while-loop boundary returns `Continue`
   - next-step carry is preserved unchanged for the future actual loop owner
3. If repeated invocation returns `Stop { reason, trigger }`:
   - while-loop boundary returns `Stop`
   - stop state is wrapped into `ClientHeartbeatLoopActualWhileLoopStopHandoff`
   - cleanup trigger is preserved unchanged
4. The returned step result is typed only:
   - no real repeated invocation is executed
   - no timer wait is executed
   - no retry is executed
   - no cleanup is executed

Responsibility split:

- shell runner
  - Exposes one caller-facing turn above outer shell.
- repeated invocation
  - Converts one runner turn into next-step carry or stop handoff.
- future actual while-loop
  - Owns the caller-facing step result consumed by a later real loop shell.
  - Does not perform repeated execution yet.
- cleanup responsibility
  - Starts only after the while-loop boundary returns `Stop`.
  - Remains outside the current boundary.

Current code reflects this with
`ClientHeartbeatLoopActualWhileLoopStopHandoff`,
`ClientHeartbeatLoopInvocationStepResult`, and
`ClientHeartbeatLoopActualWhileLoopBoundary`.

### Client Cleanup Responsibility Minimal Scope

After the future actual while-loop returns a typed step result, cleanup must be
entered only through an explicit boundary. The current scope fixes that
ownership handoff without implementing real cleanup.

Current minimal scope:

1. `ClientHeartbeatLoopCleanupResponsibilityBoundary` receives one
   `ClientHeartbeatLoopInvocationStepResult`.
2. If the while-loop step returns `Continue { carry }`:
   - cleanup responsibility returns `Continue`
   - next-step carry is preserved unchanged
   - cleanup is not triggered
3. If the while-loop step returns `Stop { handoff }`:
   - cleanup responsibility returns `Cleanup { input }`
   - input preserves the stop handoff
   - input adds explicit `ClientHeartbeatLoopCleanupPlan::CleanupOnStop`
4. Minimal trigger policy:
   - cleanup runs on stop only
   - cleanup does not run on retry planning
   - cleanup does not run on every iteration

Relationship between stop handoff, retry plan, and cleanup plan:

- stop handoff
  - Is the only source that can trigger cleanup responsibility.
- retry plan
  - Remains entirely in continue-path carry.
  - Never triggers cleanup in the current minimal scope.
- cleanup plan
  - Is created only after stop handoff reaches cleanup responsibility.
  - Remains explicit and side-effect-free until cleanup ordering consumes it.

Responsibility split:

- loop control
  - Covers shell runner, repeated invocation, and future actual while-loop.
  - Decides continue vs stop and preserves next-step carry.
- cleanup responsibility
  - Converts stop-only loop output into explicit cleanup input.
  - Does not execute cleanup implicitly.
- cleanup ordering
  - Remains outside the current boundary.
- cleanup execution
  - Remains outside the current boundary.

Current code reflects this with
`ClientHeartbeatLoopCleanupPlan`,
`ClientHeartbeatLoopCleanupResponsibilityInput`,
`ClientHeartbeatLoopCleanupResponsibilityResult`,
and `ClientHeartbeatLoopCleanupResponsibilityBoundary`.

### Client Cleanup Ordering Minimal Scope

After cleanup responsibility returns explicit stop-only cleanup input, a
separate ordering layer still decides what future cleanup execution will
consume. The current scope fixes that ordering handoff without implementing any
cleanup side effects.

Current minimal scope:

1. `ClientHeartbeatLoopCleanupOrderingInput::from_responsibility(...)`
   converts `ClientHeartbeatLoopCleanupResponsibilityResult` into:
   - `Ok(input)` for stop-only cleanup input
   - `Err(carry)` for continue-path carry
2. `ClientHeartbeatLoopCleanupOrderingBoundary` receives one
   `ClientHeartbeatLoopCleanupResponsibilityResult`.
3. If cleanup responsibility returns `Continue { carry }`:
   - ordering returns `Continue`
   - no cleanup ordering is produced
4. If cleanup responsibility returns `Cleanup { input }`:
   - ordering returns `Ordered { handoff }`
   - handoff preserves `stop_reason`
   - handoff converts `ClientHeartbeatLoopCleanupPlan` into
     `ClientHeartbeatLoopOrderedCleanupPlan`
5. Minimal safe ordering scope:
   - stop path only
   - no retry-triggered cleanup ordering
   - no per-iteration cleanup ordering

Relationship between stop handoff, cleanup plan, cleanup ordering, and future
cleanup execution:

- stop handoff
  - Reaches cleanup responsibility from loop control.
- cleanup plan
  - Is created by cleanup responsibility on stop only.
- cleanup ordering
  - Converts explicit cleanup plan into ordered cleanup handoff.
  - Does not execute cleanup.
- future cleanup execution
  - Will later consume ordered cleanup handoff.
  - Does not exist as side effects in the current scope.

Responsibility split:

- loop control
  - Stops or continues; never orders cleanup directly.
- cleanup responsibility
  - Creates explicit stop-only cleanup input.
- cleanup ordering
  - Converts explicit cleanup input into ordered cleanup handoff.
  - Does not collapse into execution.
- cleanup execution
  - Names the cleanup work a later implementation must run.
  - Does not flush logs, close sockets, or perform final cleanup yet.

Current code reflects this with
`ClientHeartbeatLoopCleanupOrderingInput`,
`ClientHeartbeatLoopOrderedCleanupPlan`,
`ClientHeartbeatLoopCleanupOrderingHandoff`,
`ClientHeartbeatLoopCleanupOrderingResult`,
and `ClientHeartbeatLoopCleanupOrderingBoundary`.

### Client Cleanup Execution Planning Minimal Scope

After cleanup ordering returns an ordered stop-only handoff, execution planning
still remains separate from any real cleanup side effects. The current scope
fixes only the execution-side planning boundary and the minimal future action
order it returns.

Current minimal scope:

1. `ClientHeartbeatLoopCleanupExecutionInput::from_ordering(...)`
   converts `ClientHeartbeatLoopCleanupOrderingResult` into:
   - `Ok(input)` for ordered stop-only cleanup handoff
   - `Err(carry)` for continue-path carry
2. `ClientHeartbeatLoopCleanupExecutionBoundary` receives one
   `ClientHeartbeatLoopCleanupOrderingResult`.
3. If cleanup ordering returns `Continue { carry }`:
   - execution planning returns `Continue`
   - no execution planning input is produced
4. If cleanup ordering returns `Ordered { handoff }`:
   - execution planning returns `Planned { handoff }`
   - planned handoff preserves `stop_reason`
   - planned handoff converts `ClientHeartbeatLoopOrderedCleanupPlan` into
     `ClientHeartbeatLoopCleanupExecutionPlan`
5. The stop-only execution plan keeps future ordered actions explicit:
   - `FinalFlush`
   - `LogWriterInvocation`
   - `ResourceRelease`
6. Minimal safe execution-planning scope:
   - stop path only
   - no retry-triggered cleanup planning
   - no per-iteration cleanup planning
   - no real flush/log/release side effects

Relationship between cleanup ordering handoff, execution input, execution
planning result, and future actual cleanup side effects:

- cleanup ordering handoff
  - Is the only stop-path source for execution planning.
- execution input
  - Wraps the ordered cleanup handoff explicitly.
  - Is not created for continue-path carry.
- execution planning result
  - Returns only continue carry or a stop-only cleanup execution plan.
  - Preserves future action order without running side effects.
- future actual cleanup side effects
  - Will later consume the planned execution handoff.
  - Are still separate from planning in this scope.

Responsibility split:

- cleanup ordering
  - Produces ordered cleanup handoff only.
  - Does not decide future flush/log/release execution order.
- cleanup execution planning
  - Converts ordered cleanup handoff into a stop-only execution plan.
  - Keeps final flush, log writer invocation, and resource release as future
    ordered actions only.
  - Does not collapse into side-effect execution.
- future actual cleanup side effects
  - Will later run flush/log/release in the planned order.
  - Do not exist in the current implementation.

Current code reflects this with
`ClientHeartbeatLoopCleanupExecutionInput`,
`ClientHeartbeatLoopFutureCleanupAction`,
`ClientHeartbeatLoopCleanupExecutionPlan`,
`ClientHeartbeatLoopCleanupExecutionPlanningHandoff`,
`ClientHeartbeatLoopCleanupExecutionResult`, and
`ClientHeartbeatLoopCleanupExecutionBoundary`.

### Client Cleanup Side-Effect Apply Minimal Scope

After cleanup execution planning returns a stop-only planned handoff, actual
cleanup side-effect apply remains a separate step. The current scope adds only
the minimal stop-path apply boundary and explicit ordered apply result; it does
not introduce complex final flush, log writer, or resource release bodies.

Current minimal scope:

1. `ClientHeartbeatLoopCleanupSideEffectInput::from_execution_planning(...)`
   converts `ClientHeartbeatLoopCleanupExecutionResult` into:
   - `Ok(input)` for stop-only planned cleanup handoff
   - `Err(carry)` for continue-path carry
2. `ClientHeartbeatLoopCleanupSideEffectBoundary` receives one
   `ClientHeartbeatLoopCleanupExecutionResult`.
3. If cleanup execution planning returns `Continue { carry }`:
   - side-effect apply returns `Continue`
   - no side-effect input is produced
4. If cleanup execution planning returns `Planned { handoff }`:
   - side-effect apply returns `Applied { result }`
   - result preserves `stop_reason`
   - result marks cleanup completion explicitly
   - result keeps applied action order explicit
5. The stop-only apply scope is limited to ordered placeholder application of:
   - `FinalFlush`
   - `LogWriterInvocation`
   - `ResourceRelease`
6. Minimal safe side-effect scope:
   - stop path only
   - no retry-triggered cleanup apply
   - no per-iteration cleanup apply
   - no ordering logic moved into side-effect apply

Relationship between cleanup execution planning handoff, actual cleanup
side-effect input, actual cleanup side-effect result, and the future completed
continuous heartbeat loop stop path:

- cleanup execution planning handoff
  - Is the only stop-path source for actual cleanup side-effect input.
- actual cleanup side-effect input
  - Wraps the planned cleanup handoff explicitly.
  - Is not created for continue-path carry.
- actual cleanup side-effect result
  - Returns only continue carry or an explicit stop-path apply result.
  - Keeps flush/log/release apply order visible.
- future completed continuous heartbeat loop stop path
  - Will later consume the side-effect result as the terminal stop-path output.
  - Remains separate from the dumb actual while-loop.

Responsibility split:

- cleanup execution planning
  - Produces a stop-only planned cleanup handoff.
  - Does not apply side effects.
- cleanup side-effect apply
  - Applies only planned stop-path actions.
  - Preserves explicit flush/log/release order.
  - Does not add retry-triggered cleanup or per-iteration cleanup.
- future completed continuous heartbeat loop stop path
  - Will later own terminal stop-path wiring after side-effect apply finishes.
  - Does not exist as a completed integration in the current scope.

Current code reflects this with
`ClientHeartbeatLoopCleanupSideEffectInput`,
`ClientHeartbeatLoopCleanupAppliedAction`,
`ClientHeartbeatLoopCleanupSideEffectApplyResult`,
`ClientHeartbeatLoopCleanupSideEffectResult`, and
`ClientHeartbeatLoopCleanupSideEffectBoundary`.

### Client Completed Loop Stop-Path Output Minimal Scope

After stop-path cleanup side-effect apply finishes, terminal stop-path output
for a future completed continuous heartbeat loop remains a separate boundary.
The current scope adds only the minimal conversion from cleanup side-effect
result into terminal stop-path output; it does not implement the full
completed loop body or actual while-loop termination.

Current minimal scope:

1. `ClientHeartbeatLoopCompletedLoopStopPathInput::from_cleanup_side_effect(...)`
   converts `ClientHeartbeatLoopCleanupSideEffectResult` into:
   - `Ok(input)` for stop-path cleanup apply result
   - `Err(carry)` for continue-path carry
2. `ClientHeartbeatLoopCompletedLoopStopPathBoundary` receives one
   `ClientHeartbeatLoopCleanupSideEffectResult`.
3. If cleanup side-effect apply returns `Continue { carry }`:
   - completed-loop stop-path boundary returns `Continue`
   - no terminal stop-path output is produced
4. If cleanup side-effect apply returns `Applied { result }`:
   - completed-loop stop-path boundary returns `Stop { handoff }`
   - terminal output preserves `stop_reason`
   - terminal output preserves `cleanup_completed`
   - terminal output preserves explicit flush/log/release apply order
5. Minimal safe terminal stop-path scope:
   - terminal output is created from cleanup side-effect result only
   - continue carry stays separate from terminal stop-path output
   - no cleanup ordering or execution planning logic is re-interpreted here
   - no actual while-loop business logic is added here

Relationship between cleanup actual side-effect result, completed continuous
heartbeat loop stop-path output, and future actual while-loop termination:

- cleanup actual side-effect result
  - Is the only source for terminal stop-path input.
- completed continuous heartbeat loop stop-path output
  - Is created only after cleanup side-effect apply completes.
  - Keeps stop-only semantics explicit.
  - Remains separate from continue carry.
- future actual while-loop termination
  - Will later consume the terminal stop-path output.
  - Remains outside this minimal boundary.

Responsibility split:

- cleanup actual side-effect apply
  - Produces explicit stop-path apply result only.
  - Does not own terminal completed-loop stop output.
- completed-loop stop-path output boundary
  - Converts side-effect result into terminal stop-path output only.
  - Does not re-order cleanup or execute new side effects.
- future actual while-loop termination
  - Will later own final stop-path termination wiring.
  - Is not implemented in the current scope.

Current code reflects this with
`ClientHeartbeatLoopCompletedLoopStopPathInput`,
`ClientHeartbeatLoopTerminalStopPathOutput`,
`ClientHeartbeatLoopCompletedLoopStopPathHandoff`,
`ClientHeartbeatLoopCompletedLoopStopPathResult`, and
`ClientHeartbeatLoopCompletedLoopStopPathBoundary`.

### Client Actual While-Loop Termination Minimal Scope

After completed-loop terminal stop-path output becomes available, actual
while-loop termination remains a separate boundary. The current scope adds
only the minimal conversion from completed-loop stop-path result into explicit
actual while-loop termination output; it does not implement the full completed
continuous heartbeat loop body.

Current minimal scope:

1. `ClientHeartbeatLoopActualWhileLoopTerminationInput::from_completed_loop_stop_path(...)`
   converts `ClientHeartbeatLoopCompletedLoopStopPathResult` into:
   - `Ok(input)` for stop-path completed-loop handoff
   - `Err(carry)` for continue-path carry
2. `ClientHeartbeatLoopActualWhileLoopTerminationBoundary` receives one
   `ClientHeartbeatLoopCompletedLoopStopPathResult`.
3. If completed-loop stop-path boundary returns `Continue { carry }`:
   - actual while-loop termination returns `Continue`
   - no actual while-loop termination input is produced
4. If completed-loop stop-path boundary returns `Stop { handoff }`:
   - actual while-loop termination returns `Terminated { output }`
   - termination output preserves `stop_reason`
   - termination output preserves `cleanup_completed`
   - termination output preserves explicit flush/log/release apply order
5. Minimal safe termination scope:
   - completed-loop stop-path result is the only source for termination input
   - continue carry, terminal stop-path output, and actual termination result stay separate
   - no cleanup ordering / execution planning / side-effect apply logic is re-interpreted here
   - no business logic is moved into the actual while-loop body

Relationship between completed-loop stop-path output, actual while-loop
termination input, actual while-loop terminal output, and the future completed
continuous heartbeat loop body:

- completed-loop stop-path output
  - Is the only stop-path source for actual while-loop termination input.
- actual while-loop termination input
  - Wraps only completed-loop stop-path handoff.
  - Is not created for continue-path carry.
- actual while-loop terminal output
  - Is produced only after completed-loop stop-path output is available.
  - Preserves `stop_reason`, `cleanup_completed`, and `applied_actions`.
- future completed continuous heartbeat loop body
  - Will later consume the termination result while keeping the while-loop dumb.
  - Remains outside this minimal boundary.

Responsibility split:

- completed-loop stop-path output boundary
  - Produces terminal stop-path output only.
  - Does not own actual while-loop termination.
- actual while-loop termination boundary
  - Converts completed-loop stop-path output into actual termination output only.
  - Does not re-run or reinterpret cleanup logic.
- future completed continuous heartbeat loop body
  - Will later own final termination wiring around the dumb while-loop.
  - Is not implemented in the current scope.

Current code reflects this with
`ClientHeartbeatLoopActualWhileLoopTerminationInput`,
`ClientHeartbeatLoopActualWhileLoopTerminalOutput`,
`ClientHeartbeatLoopActualWhileLoopTerminationResult`, and
`ClientHeartbeatLoopActualWhileLoopTerminationBoundary`.

### Client Completed Loop Body Integration Minimal Scope

After actual while-loop termination becomes available, completed continuous
heartbeat loop body integration remains a separate boundary. The current scope
adds only the minimal conversion from actual while-loop termination result
into completed loop body result; it does not implement future timer wait,
retry execution, reconnect, or timeout wakeup execution.

Current minimal scope:

1. `ClientHeartbeatLoopCompletedBodyInput::from_actual_while_loop_termination(...)`
   converts `ClientHeartbeatLoopActualWhileLoopTerminationResult` into:
   - `Ok(input)` for stop-path actual termination output
   - `Err(carry)` for continue-path carry
2. `ClientHeartbeatLoopCompletedBodyIntegrationBoundary` receives one
   `ClientHeartbeatLoopActualWhileLoopTerminationResult`.
3. If actual while-loop termination returns `Continue { carry }`:
   - completed loop body integration returns `Continue`
   - no completed loop body stop-path input is produced
4. If actual while-loop termination returns `Terminated { output }`:
   - completed loop body integration returns `Stop { output }`
   - completed loop body result preserves `stop_reason`
   - completed loop body result preserves `cleanup_completed`
   - completed loop body result preserves explicit flush/log/release apply order
5. Minimal safe completed-body scope:
   - actual while-loop termination result is the only source for completed body input
   - continue carry, termination result, and completed loop body result stay separate
   - no cleanup ordering / execution planning / side-effect apply / termination logic is re-interpreted here
   - future timer / retry / reconnect integration remains outside this boundary

Relationship between actual while-loop termination result, completed
continuous heartbeat loop body input, completed continuous heartbeat loop body
result, and future timer / retry / reconnect integration:

- actual while-loop termination result
  - Is the only stop-path source for completed loop body input.
- completed continuous heartbeat loop body input
  - Wraps only actual while-loop terminal output.
  - Is not created for continue-path carry.
- completed continuous heartbeat loop body result
  - Is produced only after actual while-loop termination is available.
  - Preserves `stop_reason`, `cleanup_completed`, and `applied_actions`.
- future timer / retry / reconnect integration
  - Will later consume the completed body result while keeping the actual while-loop dumb.
  - Is not implemented in the current scope.

Responsibility split:

- actual while-loop termination boundary
  - Produces explicit termination output only.
  - Does not own completed continuous heartbeat loop body integration.
- completed loop body integration boundary
  - Converts actual while-loop termination result into completed body result only.
  - Does not reinterpret cleanup logic or add runtime behavior.
- future timer / retry / reconnect integration
  - Will later own non-stop-path integration around completed loop body flow.
  - Is not implemented in the current scope.

Current code reflects this with
`ClientHeartbeatLoopCompletedBodyInput`,
`ClientHeartbeatLoopCompletedBodyTerminalOutput`,
`ClientHeartbeatLoopCompletedBodyIntegrationResult`, and
`ClientHeartbeatLoopCompletedBodyIntegrationBoundary`.

### Client Timer / Retry / Reconnect Integration Minimal Scope

After completed loop body output becomes available, future timer / retry /
reconnect integration remains a separate boundary. The current scope adds only
the minimal conversion from completed loop body result into explicit future
planning handoff or explicit stop passthrough; it does not execute timer wait,
retry execution, reconnect, or timeout wakeup behavior.

Current minimal scope:

1. `ClientHeartbeatLoopTimerRetryReconnectIntegrationInput::from_completed_body_result(...)`
   converts `ClientHeartbeatLoopCompletedBodyIntegrationResult` into:
   - `Ok(input)` for continue-path carry
   - `Err(output)` for explicit stop-path output
2. `ClientHeartbeatLoopTimerRetryReconnectIntegrationBoundary` receives one
   `ClientHeartbeatLoopCompletedBodyIntegrationResult`.
3. If completed loop body integration returns `Continue { carry }`:
   - timer / retry / reconnect integration returns `ContinuePlanning { handoff }`
   - future planning handoff preserves continue carry explicitly
4. If completed loop body integration returns `Stop { output }`:
   - timer / retry / reconnect integration returns `Stop { output }`
   - stop path remains explicit and is not collapsed into continue planning
5. Minimal safe integration scope:
   - completed loop body result is the only source for planning input
   - continue carry, stop result, and future planning result stay separate
   - no cleanup logic is re-interpreted here
   - no timer wait / retry execution / reconnect / timeout wakeup is executed here

Relationship between completed loop body result, timer / retry / reconnect
integration input, timer / retry / reconnect integration result, and future
completed continuous heartbeat loop body:

- completed loop body result
  - Is the only source for timer / retry / reconnect integration input.
- timer / retry / reconnect integration input
  - Wraps only continue-path carry.
  - Is not created for explicit stop-path output.
- timer / retry / reconnect integration result
  - Returns either future planning handoff for continue path or explicit stop passthrough.
  - Keeps continue / stop / future planning distinct.
- future completed continuous heartbeat loop body
  - Will later consume this integration result without moving business logic into the dumb actual while-loop.
  - Remains outside the current scope.

Responsibility split:

- completed loop body integration boundary
  - Produces explicit continue carry or explicit stop output only.
  - Does not own timer / retry / reconnect planning.
- timer / retry / reconnect integration boundary
  - Converts completed loop body result into future planning handoff or stop passthrough only.
  - Does not reinterpret cleanup logic or execute runtime behavior.
- future completed continuous heartbeat loop body
  - Will later own the next-stage integration around timer / retry / reconnect.
  - Is not implemented in the current scope.

Current code reflects this with
`ClientHeartbeatLoopTimerRetryReconnectIntegrationInput`,
`ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff`,
`ClientHeartbeatLoopTimerRetryReconnectIntegrationResult`, and
`ClientHeartbeatLoopTimerRetryReconnectIntegrationBoundary`.

### Client Actual Timer / Retry / Reconnect Execution Integration Minimal Scope

After future timer / retry / reconnect planning handoff becomes available,
actual execution integration remains a separate boundary. The current scope
adds only the minimal conversion from planning handoff into explicit future
actual execution actions or explicit stop passthrough; it does not execute
timer wait, retry execution, reconnect, or timeout wakeup behavior.

Current minimal scope:

1. `ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput::from_planning_handoff(...)`
   converts `ClientHeartbeatLoopTimerRetryReconnectIntegrationResult` into:
   - `Ok(input)` for continue-path planning handoff
   - `Err(output)` for explicit stop-path output
2. `ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary` receives one
   `ClientHeartbeatLoopTimerRetryReconnectIntegrationResult`.
3. If timer / retry / reconnect integration returns `ContinuePlanning { handoff }`:
   - actual execution integration returns `ContinueExecution { handoff }`
   - continue path preserves explicit future actual execution actions
4. If timer / retry / reconnect integration returns `Stop { output }`:
   - actual execution integration returns `Stop { output }`
   - stop path remains explicit and is not collapsed into continue execution
5. Minimal safe actual-execution scope:
   - planning handoff is the only source for actual execution input
   - continue path preserves explicit future actions for `TimerWait`, `RetryExecution`, and `ReconnectExecution`
   - reconnect stays explicit even when only `NoReconnectExecution` exists today
   - no heartbeat timeout wakeup is executed here

Relationship between timer / retry / reconnect planning handoff, actual
execution integration input, actual execution integration result, and future
completed continuous heartbeat loop body:

- timer / retry / reconnect planning handoff
  - Is the only source for actual execution integration input.
- actual execution integration input
  - Wraps only continue-path planning handoff.
  - Is not created for explicit stop-path output.
- actual execution integration result
  - Returns either explicit future actual execution handoff for continue path or explicit stop passthrough.
  - Keeps timer wait / retry execution / reconnect execution visible.
- future completed continuous heartbeat loop body
  - Will later consume the actual execution integration result without hiding side effects inside the dumb while-loop.
  - Remains outside the current scope.

Responsibility split:

- timer / retry / reconnect integration boundary
  - Produces explicit planning handoff or explicit stop output only.
  - Does not own actual execution integration.
- actual timer / retry / reconnect execution integration boundary
  - Converts planning handoff into explicit future actual execution actions only.
  - Does not execute runtime behavior or reinterpret cleanup logic.
- future completed continuous heartbeat loop body
  - Will later own the next-stage actual execution wiring around this handoff.
  - Is not implemented in the current scope.

Current code reflects this with
`ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput`,
`ClientHeartbeatLoopFutureActualTimerWaitAction`,
`ClientHeartbeatLoopFutureActualRetryExecutionAction`,
`ClientHeartbeatLoopFutureActualReconnectExecutionAction`,
`ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff`,
`ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult`, and
`ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary`.

### Client Completed Continuous Heartbeat Loop Body Connection Minimal Scope

After future actual timer / retry / reconnect execution actions become
available, completed continuous heartbeat loop body connection remains a
separate boundary. The current scope adds only the minimal conversion from
actual execution integration result into completed loop body connection
result; it does not execute timer wait, retry execution, reconnect, or timeout
wakeup behavior.

Current minimal scope:

1. `ClientHeartbeatLoopCompletedContinuousBodyConnectionInput::from_actual_execution_integration(...)`
   converts `ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult` into:
   - `Ok(input)` for continue-path execution handoff
   - `Err(output)` for explicit stop-path output
2. `ClientHeartbeatLoopCompletedContinuousBodyConnectionBoundary` receives one
   `ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult`.
3. If actual execution integration returns `ContinueExecution { handoff }`:
   - completed loop body connection returns `Continue { output }`
   - continue path preserves explicit future execution actions
4. If actual execution integration returns `Stop { output }`:
   - completed loop body connection returns `Stop { output }`
   - stop path remains explicit and is not collapsed into continue connection
5. Minimal safe completed-body-connection scope:
   - actual execution integration result is the only source for completed loop body connection input
   - continue execution handoff, stop result, and completed loop body connection result stay separate
   - timer wait / retry / reconnect remain explicit future execution actions
   - no cleanup logic or stop-path semantics are re-interpreted here

Relationship between actual timer / retry / reconnect execution integration
result, completed continuous heartbeat loop body connection input, completed
continuous heartbeat loop body connection result, and the future full
completed continuous heartbeat loop implementation:

- actual timer / retry / reconnect execution integration result
  - Is the only source for completed loop body connection input.
- completed continuous heartbeat loop body connection input
  - Wraps only continue-path execution handoff.
  - Is not created for explicit stop-path output.
- completed continuous heartbeat loop body connection result
  - Returns either explicit continue output with future execution state or explicit stop passthrough.
  - Keeps timer wait / retry / reconnect visible.
- future full completed continuous heartbeat loop implementation
  - Will later consume this connection result without hiding side effects in the dumb actual while-loop.
  - Remains outside the current scope.

Responsibility split:

- actual timer / retry / reconnect execution integration boundary
  - Produces explicit continue execution handoff or explicit stop output only.
  - Does not own completed continuous heartbeat loop body connection.
- completed continuous heartbeat loop body connection boundary
  - Converts actual execution integration result into completed loop body connection result only.
  - Does not execute runtime behavior or reinterpret cleanup logic.
- future full completed continuous heartbeat loop implementation
  - Will later own the final loop-body wiring around this connected result.
  - Is not implemented in the current scope.

Current code reflects this with
`ClientHeartbeatLoopCompletedContinuousBodyConnectionInput`,
`ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput`,
`ClientHeartbeatLoopCompletedContinuousBodyConnectionResult`, and
`ClientHeartbeatLoopCompletedContinuousBodyConnectionBoundary`.

### Client Completed Continuous Heartbeat Loop Body Minimal Scope

Once repeated invocation, stop-path cleanup flow, actual while-loop
termination, completed body integration, timer / retry / reconnect planning,
actual execution integration, and completed body connection all exist as
separate boundaries, completed continuous heartbeat loop body can stay a thin
composition layer. The current scope adds only that minimal composition; it
does not execute timer wait, retry execution, reconnect execution, timeout
wakeup, or a real while-loop.

Current minimal scope:

1. `ClientHeartbeatLoopCompletedContinuousBodyBoundary` receives one
   `ClientHeartbeatLoopRepeatedInvocationResult`.
2. It wires existing boundaries once in this order:
   - actual while-loop
   - cleanup responsibility
   - cleanup ordering
   - cleanup execution planning
   - cleanup actual side-effect apply
   - completed-loop stop-path output
   - actual while-loop termination
   - completed body integration
   - timer / retry / reconnect integration
   - actual timer / retry / reconnect execution integration
   - completed continuous body connection
3. If repeated invocation returns `Continue { carry }`:
   - completed body returns `Continue { output }`
   - continue path preserves `carry`, `timer_wait`, `retry_execution`, and
     `reconnect_execution` without executing them
4. If repeated invocation returns `Stop { reason, trigger }`:
   - completed body returns `Stop { output }`
   - stop path preserves `stop_reason`, `cleanup_completed`, and
     `applied_actions` after cleanup side-effect apply completes
5. Minimal safe completed-body scope:
   - repeated invocation result is the only entry into completed continuous
     heartbeat loop body
   - stop path remains explicit end-to-end
   - continue path remains explicit and does not hide future execution actions
   - completed body does not reinterpret cleanup ordering, cleanup execution
     planning, cleanup side-effect apply, or termination logic

Relationship between repeated invocation result, stop-path cleanup flow,
actual while-loop termination result, completed body integration / connection
result, and the final completed continuous heartbeat loop body result:

- repeated invocation result
  - Is the only entry into completed continuous heartbeat loop body.
  - Starts either explicit continue flow or explicit stop cleanup flow.
- stop-path cleanup flow
  - Runs through responsibility, ordering, execution planning, and
    side-effect apply before any terminal output exists.
  - Remains separate from continue-path planning.
- actual while-loop termination result
  - Converts completed-loop stop-path output into explicit terminal
    termination only.
  - Is passed forward without reinterpretation.
- completed body integration / connection result
  - Keeps continue planning and future execution actions explicit.
  - Preserves explicit stop passthrough.
- final completed continuous heartbeat loop body result
  - Returns either explicit continue output with unapplied future execution
    actions or explicit stop output with cleanup completion details.
  - Does not execute runtime side effects.

Responsibility split:

- upstream existing boundaries
  - Own stop-path cleanup flow, termination shaping, and continue-path
    planning / execution-action shaping.
  - Do not own completed continuous heartbeat loop body composition.
- completed continuous heartbeat loop body boundary
  - Wires existing boundaries together once and exposes the final explicit
    continue / stop result.
  - Does not execute timer wait, retry, reconnect, timeout wakeup, or a real
    while-loop.
- future full completed continuous heartbeat loop implementation
  - Will later own actual repeated execution around this composed body.
  - Is outside the current scope.

Current code reflects this with
`ClientHeartbeatLoopCompletedContinuousBodyResult` and
`ClientHeartbeatLoopCompletedContinuousBodyBoundary`.

### Client Heartbeat Timeout Notice Wakeup Minimal Scope

Before future heartbeat timeout notice wakeup execution exists, the client
still needs one explicit boundary that decides whether wakeup-related
follow-up is needed. The current scope adds only that planning boundary; it
does not execute wakeup, timer wait, retry execution, reconnect execution, or
reinterpret cleanup logic.

Current minimal scope:

1. `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput::from_completed_continuous_body(...)`
   converts `ClientHeartbeatLoopCompletedContinuousBodyResult` into:
   - `Ok(input)` for continue-path completed body output
   - `Err(output)` for explicit stop-path output
2. `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary` receives one
   `ClientHeartbeatLoopCompletedContinuousBodyResult`.
3. If completed body returns `Continue { output }` with
   `timer_wait = NoTimerWait`:
   - wakeup boundary returns `ContinueWithoutWakeup { output }`
   - continue path stays explicit without creating wakeup follow-up
4. If completed body returns `Continue { output }` with
   `timer_wait = TimerWait { sleep }`:
   - wakeup boundary returns `ContinueWithWakeup { handoff }`
   - wakeup-ready handoff preserves the existing continue output and adds an
     explicit `WakeupDuringTimerWait { sleep }` plan
5. If completed body returns `Stop { output }`:
   - wakeup boundary returns `Stop { output }`
   - stop path remains explicit and unchanged
6. Minimal safe wakeup-planning scope:
   - completed continuous heartbeat loop body result is the only source for
     wakeup input
   - continue without wakeup, continue with wakeup-ready handoff, and stop
     passthrough stay separate
   - wakeup logic stays outside timer / retry / reconnect execution concerns
   - no metrics cadence or dashboard logic is introduced here

Relationship between continue carry / current loop result, timeout notice
wakeup triggerability, wakeup handoff / passthrough, and future actual wakeup
execution:

- completed continuous heartbeat loop body result
  - Is the only entry into wakeup planning.
  - Already preserves explicit continue output or explicit stop output.
- timeout notice wakeup triggerability
  - Is determined only from explicit continue output, currently by whether a
    future timer wait exists.
  - Does not reinterpret cleanup state or stop semantics.
- wakeup handoff / passthrough
  - Returns explicit continue passthrough when no wakeup is needed.
  - Returns explicit wakeup-ready handoff when timer wait could later be made
    interruptible.
  - Returns explicit stop passthrough unchanged.
- future actual wakeup execution
  - Will later consume only wakeup-ready handoff.
  - Remains outside the current scope.

Responsibility split:

- completed continuous heartbeat loop body boundary
  - Produces explicit continue output or explicit stop output only.
  - Does not decide wakeup follow-up.
- heartbeat timeout notice wakeup boundary
  - Decides only whether continue output needs explicit wakeup-ready follow-up.
  - Does not execute wakeup, timer wait, retry, reconnect, or metrics cadence.
- future actual wakeup execution
  - Will later own the runtime side effect for wakeup-ready handoff.
  - Is not implemented in the current scope.

Current code reflects this with
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput`,
`ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan`,
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff`,
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult`, and
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary`.

### Client Heartbeat Timeout Notice Wakeup Execution Minimal Scope

Once wakeup planning exists, the next client-side step can remain a minimal
execution boundary that still does not perform a real wakeup side effect. The
current scope adds only that explicit execution shaping; it does not execute
timer wait, retry execution, reconnect execution, metrics cadence, or cleanup
logic.

Current minimal scope:

1. `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput::from_wakeup_planning(...)`
   converts `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult` into:
   - `Ok(input)` for `ContinueWithWakeup { handoff }`
   - `Err(planning)` for `ContinueWithoutWakeup { output }`
   - `Err(planning)` for `Stop { output }`
2. `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary` receives
   one `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult`.
3. If wakeup planning returns `ContinueWithoutWakeup { output }`:
   - wakeup execution returns `ContinueWithoutWakeupExecution { output }`
   - continue path remains explicit and no wakeup execution input is created
4. If wakeup planning returns `ContinueWithWakeup { handoff }`:
   - wakeup execution returns `ContinueWithWakeupExecutionApplied { output }`
   - wakeup-applied output preserves the original continue output and the
     explicit wakeup plan as `WakeupApplied`
5. If wakeup planning returns `Stop { output }`:
   - wakeup execution returns `Stop { output }`
   - stop path remains explicit and unchanged
6. Minimal safe wakeup-execution scope:
   - wakeup planning result is the only source for wakeup execution input
   - continue without wakeup execution, continue with wakeup execution
     applied, and stop passthrough stay separate
   - wakeup execution remains separate from timer / retry / reconnect concerns
   - no metrics cadence or dashboard logic is introduced here

Relationship between `ContinueWithoutWakeup`, `ContinueWithWakeup`, wakeup
execution input, wakeup execution result, and stop passthrough:

- `ContinueWithoutWakeup`
  - Does not create wakeup execution input.
  - Passes through as explicit continue without wakeup execution.
- `ContinueWithWakeup`
  - Is the only source that creates wakeup execution input.
  - Preserves explicit wakeup-ready handoff into execution.
- wakeup execution input
  - Wraps only wakeup-ready handoff.
  - Is not created for continue-without-wakeup or stop.
- wakeup execution result
  - Returns either explicit continue without wakeup execution, explicit
    continue with wakeup execution applied, or explicit stop passthrough.
  - Keeps timer wait / retry / reconnect state visible and unchanged.
- stop passthrough
  - Remains outside wakeup execution input and is not collapsed into continue.

Responsibility split:

- wakeup planning boundary
  - Determines only whether wakeup-related follow-up is needed.
  - Does not own wakeup execution shaping.
- wakeup execution boundary
  - Converts wakeup planning result into explicit execution result only.
  - Does not perform a real wakeup side effect or own timer/retry/reconnect
    execution.
- future actual wakeup execution
  - Will later own the runtime side effect behind wakeup-applied output.
  - Is not implemented in the current scope.

Current code reflects this with
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput`,
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupApplyResult`,
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionOutput`,
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult`, and
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary`.

### Client Heartbeat Timeout Notice Wakeup Actual Side-Effect Minimal Scope

The next minimal step keeps wakeup responsibility separate from timer wait,
retry execution, reconnect execution, and the actual while-loop. This scope
adds only an explicit actual-side-effect boundary after wakeup execution; it
does not add timer/retry/reconnect runtime behavior, metrics cadence, or
dashboard refresh logic.

Current minimal scope:

1. `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectInput::from_wakeup_execution(...)`
   converts `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult`
   into:
   - `Ok(input)` for `ContinueWithWakeupExecutionApplied { output }`
   - `Err(execution)` for `ContinueWithoutWakeupExecution { output }`
   - `Err(execution)` for `Stop { output }`
2. `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectBoundary`
   receives one wakeup execution result only.
3. If wakeup execution returns `ContinueWithoutWakeupExecution { output }`:
   - actual side effect returns `ContinueWithoutWakeupSideEffect { output }`
   - continue path remains explicit and no actual-side-effect input is created
4. If wakeup execution returns `ContinueWithWakeupExecutionApplied { output }`:
   - actual side effect returns
     `ContinueWithWakeupSideEffectApplied { output }`
   - the result preserves:
     - the original continue output
     - the explicit wakeup execution apply result
     - the explicit wakeup actual-side-effect apply result
5. If wakeup execution returns `Stop { output }`:
   - actual side effect returns `Stop { output }`
   - stop path remains explicit and unchanged
6. Minimal safe actual-side-effect scope:
   - wakeup execution result is the only source for actual-side-effect input
   - `ContinueWithoutWakeupExecution` does not create actual-side-effect input
   - `ContinueWithWakeupExecutionApplied` is the only source that creates
     actual-side-effect input
   - actual side effect remains separate from timer / retry / reconnect
     concerns
   - no metrics cadence, dashboard refresh, video, switcher, or OBS logic is
     introduced here

Relationship between `ContinueWithoutWakeupExecution`,
`ContinueWithWakeupExecutionApplied`, wakeup actual-side-effect input, wakeup
actual-side-effect result, and stop passthrough:

- `ContinueWithoutWakeupExecution`
  - Does not create wakeup actual-side-effect input.
  - Passes through as explicit continue without wakeup side effect.
- `ContinueWithWakeupExecutionApplied`
  - Is the only source that creates wakeup actual-side-effect input.
  - Preserves explicit wakeup execution output into actual side-effect apply.
- wakeup actual-side-effect input
  - Wraps only execution-applied wakeup output.
  - Is not created for continue-without-wakeup execution or stop.
- wakeup actual-side-effect result
  - Returns either explicit continue without wakeup side effect, explicit
    continue with wakeup side effect applied, or explicit stop passthrough.
  - Keeps timer wait / retry / reconnect state visible and unchanged.
- stop passthrough
  - Remains outside wakeup actual-side-effect input and is not collapsed into
    continue.

Responsibility split:

- wakeup execution boundary
  - Converts wakeup planning result into explicit execution result only.
  - Does not own runtime side effects.
- wakeup actual-side-effect boundary
  - Converts wakeup execution result into explicit actual-side-effect result
    only.
  - Does not own timer wait, retry execution, reconnect execution, or stop
    path reinterpretation.
- future timer / retry / reconnect execution
  - Remains outside wakeup actual-side-effect apply.
  - Will continue to own its separate execution concerns later.

Current code reflects this with
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectInput`,
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectApplyResult`,
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectOutput`,
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectResult`, and
`ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectBoundary`.

### Client Outer While-Loop Connection Minimal Scope

After completed body composition and the wakeup chain exist, the next minimal
step is not a real while-loop body yet. Instead, one thin connection boundary
can wire:

1. completed continuous heartbeat loop body result
2. wakeup planning
3. wakeup execution
4. wakeup actual side effect
5. explicit timer wait / retry execution / reconnect execution actions
6. stop passthrough

This keeps the future outer while-loop dumb: it can delegate one turn to the
existing boundaries, receive one explicit continue or stop result, and avoid
merging wakeup / timer / retry / reconnect into one vague side effect.

Current minimal scope:

1. `ClientHeartbeatLoopOuterWhileLoopConnectionBoundary` receives one
   `ClientHeartbeatLoopRepeatedInvocationResult`.
2. It calls, in order:
   - `ClientHeartbeatLoopCompletedContinuousBodyBoundary`
   - `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary`
   - `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary`
   - `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectBoundary`
3. If the final result is `ContinueWithoutWakeupSideEffect { output }`:
   - outer connection returns `Continue { output }`
   - continue output keeps:
     - `carry`
     - `NoWakeupSideEffect`
     - explicit `timer_wait`
     - explicit `retry_execution`
     - explicit `reconnect_execution`
4. If the final result is `ContinueWithWakeupSideEffectApplied { output }`:
   - outer connection returns `Continue { output }`
   - continue output keeps:
     - `carry`
     - explicit wakeup state
     - explicit `timer_wait`
     - explicit `retry_execution`
     - explicit `reconnect_execution`
5. If the final result is `Stop { output }`:
   - outer connection returns `Stop { output }`
   - stop path remains explicit and unchanged

Relationship between completed body, wakeup actual-side-effect boundary,
timer/retry/reconnect actions, and the future actual while-loop runner:

- completed continuous heartbeat loop body boundary
  - Produces explicit continue or stop only.
  - Does not own wakeup follow-up.
- wakeup actual-side-effect boundary
  - Finalizes wakeup-related shaping only.
  - Does not own timer wait, retry execution, reconnect execution, or stop
    semantics.
- outer while-loop connection boundary
  - Wires completed body and wakeup boundaries in order.
  - Re-exposes continue output as separate wakeup / timer / retry / reconnect
    fields.
  - Does not run real timer wait, retry execution, reconnect execution, or a
    real while-loop.
- future actual while-loop runner
  - Will later consume the explicit continue output from the connection
    boundary.
  - Should remain a thin delegate that applies returned actions in order
    without reinterpreting cleanup or stop output.

Current code reflects this with
`ClientHeartbeatLoopOuterWhileLoopWakeupState`,
`ClientHeartbeatLoopOuterWhileLoopConnectionOutput`,
`ClientHeartbeatLoopOuterWhileLoopConnectionResult`, and
`ClientHeartbeatLoopOuterWhileLoopConnectionBoundary`.

### Client Outer While-Loop One-Turn Execution Body Minimal Scope

After the outer while-loop connection boundary exists, the next minimal step is
still not a real repeated while-loop. Instead, one thin one-turn execution body
can consume only the connection result and surface an explicit continue-or-stop
shape for a future outer runner.

Current minimal scope:

1. `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionInput::from_connection(...)`
   converts `ClientHeartbeatLoopOuterWhileLoopConnectionResult` into:
   - `Ok(input)` for `Continue { output }`
   - `Err(output)` for `Stop { output }`
2. `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionBoundary` receives one
   outer while-loop connection result only.
3. If connection returns `Continue { output }`:
   - one-turn execution returns `Continue { output }`
   - continue output keeps, in explicit order:
     - wakeup passthrough or applied marker
     - timer wait action
     - retry execution action
     - reconnect execution action
     - next-step carry
4. If connection returns `Stop { output }`:
   - one-turn execution returns `Stop { output }`
   - stop path preserves:
     - `stop_reason`
     - `cleanup_completed`
     - `applied_actions`
5. Minimal safe one-turn scope:
   - outer while-loop connection result is the single source of truth
   - continue path stays explicit and keeps wakeup / timer / retry /
     reconnect separation
   - stop path is passthrough and is not collapsed into continue execution
   - no timer wait runtime, retry runtime, reconnect runtime, metrics cadence,
     dashboard refresh, video, switcher, or OBS logic is introduced here

Relationship between completed continuous heartbeat loop body, wakeup actual
side-effect boundary, timer / retry / reconnect actions, and the future actual
while-loop runner:

- outer while-loop connection boundary
  - Produces the only input that one-turn execution consumes.
  - Preserves explicit wakeup / timer / retry / reconnect actions.
- outer while-loop one-turn execution boundary
  - Re-exposes the connection result as explicit continue next-step state or
    explicit stop terminal output.
  - Does not reinterpret cleanup or stop semantics.
- future actual while-loop runner
  - Will later own repetition only.
  - Should stay a thin delegate that calls connection, then one-turn
    execution, then applies the returned actions in order.

Current code reflects this with
`ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionInput`,
`ClientHeartbeatLoopOuterWhileLoopOneTurnNextStepState`,
`ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionOutput`,
`ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionResult`, and
`ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionBoundary`.

### Client Outer While-Loop Actual Timer / Retry / Reconnect Execution Minimal Scope

After one-turn execution body exists, the next minimal step is still not a full
repeated outer while-loop. Instead, explicit actual execution boundaries can
consume only the one-turn execution result and keep timer wait, retry
execution, reconnect execution, and next carry separated.

Current minimal scope:

1. `ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionInput::from_one_turn_execution(...)`
   converts `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionResult` into:
   - `Ok(input)` for `Continue { output }`
   - `Err(output)` for `Stop { output }`
2. `ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionInput::from_one_turn_execution(...)`
   converts `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionResult` into:
   - `Ok(input)` for `Continue { output }`
   - `Err(output)` for `Stop { output }`
3. `ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionInput::from_one_turn_execution(...)`
   converts `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionResult` into:
   - `Ok(input)` for `Continue { output }`
   - `Err(output)` for `Stop { output }`
4. The explicit continue-path application order is:
   - wakeup result already available from one-turn execution
   - actual timer wait execution
   - actual retry execution
   - actual reconnect execution
   - next carry passthrough
5. `ClientHeartbeatLoopOuterWhileLoopActualExecutionBoundary` receives one
   one-turn execution result and applies:
   - `ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionBoundary`
   - `ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionBoundary`
   - `ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionBoundary`
6. If one-turn execution returns `Continue { output }`:
   - timer wait execution returns explicit applied or no-op timer wait result
   - retry execution returns explicit applied or no-op retry result
   - reconnect execution returns explicit applied or no-op reconnect result
   - aggregate actual execution returns explicit continue output with:
     - wakeup state
     - timer wait execution result
     - retry execution result
     - reconnect execution result
     - next carry
7. If one-turn execution returns `Stop { output }`:
   - timer / retry / reconnect execution inputs are not created
   - aggregate actual execution returns `Stop { output }`
   - stop path preserves `stop_reason`, `cleanup_completed`, and
     `applied_actions` unchanged
8. Minimal safe actual-execution scope:
   - one-turn execution result is the only source for timer / retry /
     reconnect execution inputs
   - timer wait, retry execution, and reconnect execution remain separate
   - wakeup remains outside timer / retry / reconnect execution
   - no metrics cadence, dashboard refresh, video, switcher, OBS, or full
     repeated outer while-loop logic is introduced here
   - no broad reconnect policy is introduced here

Relationship between one-turn execution result, timer wait execution, retry
execution, reconnect execution, and the future repeated outer while-loop body:

- outer while-loop one-turn execution result
  - Is the only source for actual timer / retry / reconnect execution inputs.
  - Already contains wakeup state and next carry.
- actual timer wait execution boundary
  - Applies only timer wait execution shape.
  - Preserves wakeup, retry execution, reconnect execution, and next carry.
- actual retry execution boundary
  - Applies only retry execution shape.
  - Preserves wakeup, timer wait, reconnect execution, and next carry.
- actual reconnect execution boundary
  - Applies only reconnect execution shape.
  - Preserves wakeup, timer wait, retry execution, and next carry.
- future repeated outer while-loop body
  - Will later own repetition only.
  - Should stay a thin delegate that consumes one-turn execution result, runs
    actual execution in order, and then advances with the returned next carry.

Current code reflects this with
`ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionInput`,
`ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionApplyResult`,
`ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionOutput`,
`ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionResult`,
`ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionBoundary`,
`ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionInput`,
`ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionApplyResult`,
`ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionOutput`,
`ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionResult`,
`ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionBoundary`,
`ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionInput`,
`ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionApplyResult`,
`ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionOutput`,
`ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionResult`,
`ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionBoundary`,
`ClientHeartbeatLoopOuterWhileLoopActualExecutionOutput`,
`ClientHeartbeatLoopOuterWhileLoopActualExecutionResult`, and
`ClientHeartbeatLoopOuterWhileLoopActualExecutionBoundary`.

### Client Outer While-Loop Repeated Body Minimal Scope

After one-turn execution body and actual timer / retry / reconnect execution
boundaries exist, the next minimal step can finally add a repeated outer
while-loop body. This repeated body must remain a thin loop over the existing
boundaries; it should not hide wakeup, timer wait, retry execution, reconnect
execution, or stop semantics inside one vague side effect.

Current minimal scope:

1. `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyInput` receives:
   - one caller-owned current `ClientHeartbeatLoopRepeatedInvocationResult`
   - one caller-owned `max_turns` guard
2. `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyBoundary` runs, in order, for
   each turn:
   - `ClientHeartbeatLoopOuterWhileLoopConnectionBoundary`
   - `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionBoundary`
   - `ClientHeartbeatLoopOuterWhileLoopActualExecutionBoundary`
   - `ClientHeartbeatLoopOuterWhileLoopReconnectBoundary`
3. If reconnect connection returns continue output:
   - repeated body updates next carry from the carried execution output
   - repeated body preserves last explicit execution output
   - repeated body preserves explicit reconnect state separately
   - repeated body loops again until caller-owned `max_turns` is reached
4. If reconnect connection returns `Stop { output }`:
   - repeated body returns `Stopped { output }`
   - stop path preserves:
     - `stop_reason`
     - `cleanup_completed`
     - `applied_actions`
5. If continue path reaches caller-owned `max_turns` first:
   - repeated body returns `ReachedTurnGuard { state }`
   - continuation state preserves:
     - `turns_completed`
     - explicit next carry
     - last explicit execution output
     - last explicit reconnect state
6. Minimal safe repeated-body scope:
   - repeated body stays a thin loop over existing boundaries only
   - stop path remains passthrough and is not reinterpreted
   - continue path updates carry only from actual execution result
   - wakeup / timer wait / retry execution / reconnect execution stay visible
   - no metrics cadence, dashboard refresh, video, switcher, OBS, or full
     reconnect policy is introduced here

Relationship between one-turn execution result, actual timer / retry /
reconnect execution result, next carry, and future reconnect policy / socket
re-establishment:

- outer while-loop one-turn execution result
  - Remains the only source for actual execution input.
  - Keeps wakeup and future actions explicit.
- outer while-loop actual execution result
  - Remains the source for reconnect policy input.
  - Preserves separated timer / retry / reconnect execution output.
- outer while-loop reconnect result
  - Becomes the source for repeated-body next carry updates.
  - Preserves explicit reconnect state separately from last execution output.
- next carry
  - Is returned from repeated body only through explicit continuation state.
  - Is used to build the next repeated invocation turn.
- future reconnect policy / socket re-establishment
  - Remains outside the repeated-body minimal scope.
  - Will later refine reconnect execution behavior without changing the
    repeated-body call order.

Current code reflects this with
`ClientHeartbeatLoopOuterWhileLoopRepeatedBodyInput`,
`ClientHeartbeatLoopOuterWhileLoopRepeatedBodyContinuationState`,
`ClientHeartbeatLoopOuterWhileLoopRepeatedBodyResult`, and
`ClientHeartbeatLoopOuterWhileLoopRepeatedBodyBoundary`.

A future client continuous heartbeat loop runner can keep socket ownership
caller-owned by invoking `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyBoundary::run_with_hook(...)`
with a caller-provided socket re-establishment hook. This keeps bind/connect
and socket-slot replacement outside the repeated body itself.

### Client Outer While-Loop Reconnect Policy / Actual Socket Re-Establishment Minimal Scope

After repeated outer while-loop body exists, reconnect still must remain a
separate concern from timer wait and retry execution. The next minimal step is
not a broad recovery system. Instead, one explicit reconnect policy boundary
and one actual socket re-establishment boundary with a caller-owned hook can
consume only the reconnect execution result that already exists in the outer
while-loop path.

Current minimal scope:

1. `ClientHeartbeatLoopOuterWhileLoopReconnectPolicyInput::from_actual_execution(...)`
   converts `ClientHeartbeatLoopOuterWhileLoopActualExecutionResult` into:
   - `Ok(input)` for `Continue { output }`
   - `Err(output)` for `Stop { output }`
2. `ClientHeartbeatLoopOuterWhileLoopReconnectPolicyBoundary` consumes only the
   explicit reconnect execution state from actual execution output.
3. If actual execution returns `Continue { output }` with
   `NoReconnectExecutionApplied`:
   - reconnect policy returns `ContinueWithoutReconnect { output }`
   - timer wait and retry execution remain unchanged
4. If actual execution returns `Continue { output }` with
   `ReconnectExecutionApplied { reason }`:
   - reconnect policy returns `ContinueWithReconnectPlanned { handoff }`
   - handoff preserves:
     - original actual execution output
     - explicit reconnect plan for future socket re-establishment
5. `ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentBoundary` consumes
   reconnect policy result only and delegates socket replacement to a
   caller-owned hook.
6. If reconnect policy returns `ContinueWithReconnectPlanned { handoff }`:
   - actual socket re-establishment returns an explicit applied, deferred, or
     failed result
   - the default path stays deferred
   - a real caller-owned hook can bind a fresh UDP socket, connect it to the
     next destination, and replace the caller-owned socket slot explicitly
7. If reconnect policy returns `ContinueWithoutReconnect { output }`:
   - socket re-establishment returns `ContinueWithoutReconnect { output }`
8. If reconnect policy returns `Stop { output }`:
   - socket re-establishment returns `Stop { output }`
   - stop path preserves `stop_reason`, `cleanup_completed`, and
     `applied_actions` unchanged
9. `ClientHeartbeatLoopOuterWhileLoopReconnectBoundary` keeps outer while-loop
   integration thin by composing reconnect policy and actual socket
   re-establishment boundaries only
10. A future client continuous heartbeat loop runner can keep a caller-owned
    live UDP socket slot, construct
    `ClientHeartbeatLoopRealUdpSocketReestablishmentHook`, and pass that hook
    into repeated-body `run_with_hook(...)` without moving bind/connect logic
    into outer while-loop control
11. Minimal safe reconnect scope:
   - reconnect policy consumes only explicit reconnect execution state/result
   - timer wait and retry execution are not reinterpreted by reconnect policy
   - actual socket re-establishment consumes only explicit reconnect/socket
     plan state
   - real UDP socket replacement stays inside the caller-owned hook
   - the outer while-loop repeated body does not directly own bind/connect or
     socket slot replacement logic
   - broader live-socket borrowing and future socket option reapplication
     remain runner-owned follow-up work
   - no metrics cadence, dashboard refresh, video, switcher, OBS, or broad
     generic error recovery is introduced here

Relationship between actual reconnect execution result, reconnect policy,
future socket re-establishment, and outer while-loop repeated body
continuation state:

- actual reconnect execution result
  - Remains the only source for reconnect policy input.
  - Keeps wakeup / timer wait / retry execution separate.
- reconnect policy
  - Decides only whether socket re-establishment should be planned.
  - Does not reinterpret timer wait or retry execution.
- actual socket re-establishment
  - Receives explicit reconnect plan only.
  - Derives minimal real replacement input from the reconnect handoff only.
  - Delegates the concrete socket replacement attempt to a caller-owned hook.
  - Returns applied, deferred, or failed explicit output.
  - Can map real UDP bind/connect failure into explicit socket
    re-establishment error kinds without reinterpreting timer/retry concerns.
- outer while-loop repeated body continuation state
  - Can carry explicit reconnect result separately from last execution output.
  - Keeps next carry visible without hiding reconnect state inside it.

Current code reflects this with
`ClientHeartbeatLoopReconnectReason`,
`ClientHeartbeatLoopSocketReestablishmentFailureKind`,
`ClientHeartbeatLoopSocketReestablishmentError`,
`ClientHeartbeatLoopOuterWhileLoopReconnectPolicyInput`,
`ClientHeartbeatLoopFutureSocketReestablishmentPlan`,
`ClientHeartbeatLoopOuterWhileLoopReconnectPolicyHandoff`,
`ClientHeartbeatLoopOuterWhileLoopReconnectPolicyResult`,
`ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentInput`,
`ClientHeartbeatLoopSocketReestablishmentHookResult`,
`ClientHeartbeatLoopSocketReestablishmentHook`,
`ClientHeartbeatLoopDeferredSocketReestablishmentHook`,
`ClientHeartbeatLoopRealUdpSocketReplacementInput`,
`ClientHeartbeatLoopRealUdpSocketReplacementRuntime`,
`ClientHeartbeatLoopRealUdpSocketReestablishmentHook`,
`ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentApplyResult`,
`ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentOutput`,
`ClientHeartbeatLoopOuterWhileLoopReconnectResult`,
`ClientHeartbeatLoopOuterWhileLoopReconnectState`,
`ClientHeartbeatLoopOuterWhileLoopReconnectPolicyBoundary`,
`ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentBoundary`, and
`ClientHeartbeatLoopOuterWhileLoopReconnectBoundary`, plus
`ClientHeartbeatLoopOuterWhileLoopRepeatedBodyBoundary::run_with_hook(...)`
for future caller-owned runner wiring.

### Client Continuous Heartbeat Loop Runner Live Socket Ownership Boundary

The client continuous heartbeat loop runner now has a minimal ownership
boundary for live UDP socket wiring. The runner owns the socket slot and drives
the existing repeated outer while-loop body, while concrete socket replacement
still happens only through the injected socket re-establishment hook.

Minimal scope:

1. `ClientHeartbeatLoopRunner` owns an
   `Arc<Mutex<Option<UdpSocket>>>` live socket slot.
2. The runner constructs
   `ClientHeartbeatLoopRealUdpSocketReestablishmentHook` from that slot.
3. The runner calls
   `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyBoundary::run_with_hook(...)`
   with the real hook.
4. If the repeated body reaches its caller-owned turn guard, the runner returns
   `ClientHeartbeatLoopRunnerResult::Completed { state, socket }`.
5. If the repeated body stops, the runner returns
   `ClientHeartbeatLoopRunnerResult::Stopped { output, socket }` without
   changing stop reason, cleanup completion, or applied cleanup actions.
6. If the runner cannot read its caller-owned socket slot, it returns
   `ClientHeartbeatLoopRunnerResult::Error { error }`.
7. Socket replacement remains hook-owned:
   - the runner exposes the hook for the slot it owns
   - the hook binds, connects, and replaces the slot
   - the repeated body never owns the socket and never mutates the slot
8. The runner does not reinterpret timer wait, retry execution, reconnect
   policy output, metrics commit, snapshot cadence, dashboard refresh, video,
   switcher, or OBS behavior.

Relationship between runner, socket ownership, reconnect hook, repeated body,
and future metrics cadence wiring:

- loop runner
  - Owns the live UDP socket slot.
  - Coordinates one repeated-body run through `run_with_hook(...)`.
  - Surfaces completed, stopped, or error results explicitly.
- socket ownership
  - Lives in the runner-owned slot.
  - Is observed by the runner only as `has_socket` in the runner result.
  - Is not embedded in repeated-body continuation state.
- reconnect hook
  - Is constructed from the runner-owned socket slot.
  - Performs the actual bind/connect/replace operation.
  - Returns applied, deferred, or failed output through the existing reconnect
    path.
- repeated loop execution
  - Remains unchanged.
  - Receives only the hook abstraction.
  - Continues to keep timer, retry, reconnect, and stop concerns explicit.
- future metrics cadence wiring
  - Remains a later caller-owned runtime concern.
  - Can be driven after runner output is available.
  - Must consume only explicit metrics state/cadence state and not socket
    ownership internals.

Current code reflects this with
`ClientHeartbeatLoopRunnerSocketOwnershipState`,
`ClientHeartbeatLoopRunnerErrorKind`, `ClientHeartbeatLoopRunnerError`,
`ClientHeartbeatLoopRunnerResult`, and `ClientHeartbeatLoopRunner`.

### Client Runner Metrics Snapshot Cadence Runtime Wiring

The client continuous heartbeat loop runner now has a minimal runtime wiring
point for metrics snapshot export cadence. The repeated body remains unaware
of metrics cadence. The runner evaluates cadence only from caller-owned
metrics state, caller-owned cadence state, current time, export interval, and
selected snapshot consumer.

Minimal scope:

1. `ClientHeartbeatLoopRunnerMetricsSnapshotCadenceRuntimeInput` carries:
   - optional borrowed `ClientHeartbeatRttOffsetMetricsState`
   - optional `ClientHeartbeatRttOffsetMetricsSnapshotCadenceState`
   - current time
   - export interval
   - selected `ClientHeartbeatRttOffsetMetricsSnapshotConsumer`
2. `ClientHeartbeatLoopRunner::run_with_metrics_snapshot_cadence(...)` first
   drives normal runner execution through the existing runner path.
3. After runner execution, it calls
   `ClientHeartbeatRttOffsetMetricsSnapshotExportCadenceBoundary::build_input`
   and `evaluate`.
4. It returns `ClientHeartbeatLoopRunnerMetricsSnapshotCadenceRuntimeResult`
   containing:
   - unchanged `ClientHeartbeatLoopRunnerResult`
   - explicit `ClientHeartbeatRttOffsetMetricsSnapshotExportCadenceResult`
5. Snapshot export cadence result remains one of:
   - `SnapshotExportDue`
   - `SnapshotExportNotDue`
   - `SnapshotExportDeferred`
6. Dashboard refresh remains a future owner concern:
   - cadence may emit `future_dashboard_refresh` handoff
   - the runner does not evaluate dashboard refresh policy
   - no UI rendering, dashboard storage, or transport is introduced
7. Metrics commit remains separate:
   - runner cadence wiring does not derive commit input
   - runner cadence wiring does not calculate RTT / offset estimates
   - runner cadence wiring does not mutate metrics state

Relationship between runner, metrics state, cadence state, snapshot export,
and future dashboard refresh owner:

- `ClientHeartbeatLoopRunner`
  - Owns socket slot and repeated-body execution coordination.
  - Borrows metrics state only for snapshot cadence evaluation.
  - Does not own metrics commit state.
- metrics state
  - Remains caller-owned.
  - Is read only by snapshot creation/cadence.
- cadence state
  - Remains caller-owned and is passed explicitly.
  - Advances only through `SnapshotExportDue` result's next cadence state.
- snapshot export result
  - Is returned beside runner output.
  - Does not alter stop passthrough or repeated-body continuation state.
- future dashboard refresh owner
  - May later consume the explicit dashboard handoff from snapshot export.
  - Remains separate from runner cadence wiring.

Current code reflects this with
`ClientHeartbeatLoopRunnerMetricsSnapshotCadenceRuntimeInput`,
`ClientHeartbeatLoopRunnerMetricsSnapshotCadenceRuntimeResult`, and
`ClientHeartbeatLoopRunner::run_with_metrics_snapshot_cadence(...)`.

### Client Runner Dashboard Refresh Runtime Wiring

The client continuous heartbeat loop runner now has a minimal runtime wiring
point for dashboard refresh. This wiring consumes only the explicit snapshot
cadence result and dashboard refresh policy result. The repeated body remains
unaware of dashboard refresh, and no dashboard UI rendering is implemented.

Minimal scope:

1. `ClientHeartbeatRttOffsetMetricsDashboardRefreshConsumerInputBoundary`
   derives refresh input only from:
   - optional dashboard refresh policy
   - explicit `ClientHeartbeatRttOffsetMetricsSnapshotExportCadenceResult`
2. `ClientHeartbeatRttOffsetMetricsDashboardRefreshConsumerPolicyBoundary`
   evaluates that input into:
   - refresh requested
   - refresh skipped
   - refresh deferred
3. `ClientHeartbeatRttOffsetMetricsDashboardRefreshRuntimeBoundary` consumes
   only the policy result and an optional caller-owned sink.
4. If refresh is requested and a sink is available, the runtime boundary calls
   `ClientHeartbeatRttOffsetMetricsDashboardRefreshRuntimeSink`.
5. If refresh is requested and no sink is available, the runtime result is an
   explicit sink-unavailable deferred result.
6. If policy skipped or deferred refresh, the runtime boundary preserves that
   result without invoking the sink.
7. `ClientHeartbeatLoopRunner::run_with_dashboard_refresh_runtime(...)`
   returns `ClientHeartbeatLoopRunnerDashboardRefreshRuntimeResult` containing:
   - the existing runner + snapshot cadence observation
   - explicit dashboard refresh runtime result

Runtime result shape:

- `RefreshApplied(request)`
  - Caller-owned sink accepted the refresh request.
  - The request contains the snapshot that a future dashboard UI may render.
- `RefreshSkipped(reason)`
  - No dashboard handoff was available, usually because snapshot export was not
    due.
- `RefreshDeferred(reason)`
  - Refresh policy deferred, sink was unavailable, or the sink explicitly
    deferred handling.

Relationship between runner, snapshot cadence, refresh policy, refresh sink,
and future dashboard UI:

- `ClientHeartbeatLoopRunner`
  - Coordinates repeated-body execution, snapshot cadence observation, refresh
    policy evaluation, and refresh sink invocation.
  - Does not render UI or store dashboard state.
- metrics snapshot cadence result
  - Remains the only source for dashboard refresh handoff.
  - Is preserved beside refresh runtime result.
- dashboard refresh policy
  - Consumes only explicit handoff / export result state.
  - Does not reinterpret metrics commit or cadence decisions.
- dashboard refresh runtime sink
  - Is caller-owned.
  - Receives a refresh request only after policy returns requested.
  - May apply or defer refresh without changing runner loop output.
- future dashboard UI implementation
  - May later sit behind the sink.
  - Must consume only explicit refresh requests and not reach into runner,
    cadence, metrics commit, video, switcher, or OBS state.

Current code reflects this with
`ClientHeartbeatRttOffsetMetricsDashboardRefreshRuntimeSink`,
`ClientHeartbeatRttOffsetMetricsDashboardRefreshSinkResult`,
`ClientHeartbeatRttOffsetMetricsDashboardRefreshRuntimeResult`,
`ClientHeartbeatRttOffsetMetricsDashboardRefreshRuntimeBoundary`,
`ClientHeartbeatLoopRunnerDashboardRefreshRuntimeInput`,
`ClientHeartbeatLoopRunnerDashboardRefreshRuntimeResult`, and
`ClientHeartbeatLoopRunner::run_with_dashboard_refresh_runtime(...)`.

### Heartbeat Client Ack Observation Flow

The client ack observation flow returns the missing `client_received_at`
timestamp to the server-side timebase calculator. The current implementation
supports one explicit client-to-server report through `ClientStats`; it does
not add a continuous heartbeat loop or commit estimator state.

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
5. The one-shot client report path sends that observation back to the server in
   the optional heartbeat observation block of `ClientStats`.
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
`apps/server::ServerHeartbeatClientAckObservationBoundary`. The one-shot report
sender is `apps/client::ClientAuthHeartbeatStatsPocLauncher`, and the one-shot
server bridge is `apps/server::ServerHeartbeatObservationReturnBoundary`.
Continuous report loops, durable state update, and smoothing remain future
work.

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
3. The one-shot `ClientStats` sender uses the protocol encoder and sends the
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
and `ProtocolMessageEncoderBoundary` support. `apps/client` can send one
`ClientStats` observation through `--auth-heartbeat-stats-poc-once`, and
`apps/server` can receive it through `--receive-send-three` and pass it to the
stateless RTT / offset calculator. Continuous stats send loops, durable
timebase state update, and smoothing remain future work.
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
encodes `AuthRequest`, `AuthResponse`, `Heartbeat`, `HeartbeatAck`,
`VideoFrame`, `ClientStats`, and `ServerNotice`. `UdpSocketIoBoundary` can send
an already encoded packet; queue processing and continuous send orchestration
remain unimplemented.

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

One-iteration send JSON Lines output scope:

- Event name is `server.send`.
- The event records success and failure observations for one-item send runtime.
- Success events use `outcome="Success"`, `stage="SocketSend"`,
  `encoded_len=<bytes>`, `bytes_sent=<bytes>`, and `failure=null`.
- Failure events use `outcome="Failure"`, preserve `stage`, `encoded_len`
  when available, set `bytes_sent=null`, and include `failure` /
  `disposition`.
- The writer accepts a caller-owned `io::Write` and emits exactly one JSON
  object plus newline per observed one-item send result.
- This scope is intentionally narrower than a completed send loop: it does not
  open file sinks, choose retention, buffer globally, retry, requeue, or install
  a process-wide logger.

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
- `server send JSON Lines boundary`
  - Builds `server.send` success/failure observations from one-item send
    runtime outcome or error.
  - Writes to a caller-owned writer from receive/send one-iteration runtime.
  - Does not replace the failure-only `server.send_error` boundary and does not
    own file opening, retry, requeue, or continuous loop scheduling.
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
`ServerSendErrorJsonLinesSinkBoundary`. One-iteration success/failure send
observation is represented by `apps/server::ServerSendJsonLogEventInput`,
`ServerSendJsonLogEventBoundary`, `ServerSendLogOutputBoundary`, and
`ServerSendJsonLineWriter`.
---

## Client Heartbeat RTT / Offset Metrics Commit Boundary

The client continuous heartbeat loop keeps RTT / offset metrics state commit as a separate boundary from loop execution concerns.

- Commit input is created only from explicit heartbeat ack observation state:
  - `HeartbeatAckObservation` produced by the ack observation return path
  - `ClientStats.heartbeat_observation` when the stats return path carries an observation
  - `ClientHeartbeatLoopOneTickRuntimeResult.ack_return` when a one-tick loop result observed an ack
- The commit boundary calculates one RTT / offset estimate and updates caller-owned client metrics state.
- The commit boundary does not reinterpret timer wait, retry, reconnect, socket re-establishment, cleanup, or stop decisions.
- Stop remains an explicit passthrough result when the loop result is already stopping.
- Missing observation remains an explicit no-commit result.
- Missing caller-owned metrics state or invalid RTT / offset calculation remains an explicit deferred commit result.

Metrics snapshot export cadence is a later policy boundary. Dashboard refresh is a later consumer/refresh boundary. Neither is implemented as part of per-sample metrics state commit.

## Client Heartbeat RTT / Offset Metrics Snapshot Export Cadence Boundary

The client heartbeat metrics snapshot export cadence is separate from per-sample metrics commit and separate from dashboard refresh.

- Cadence input is derived from:
  - caller-owned `ClientHeartbeatRttOffsetMetricsState`
  - caller-owned `ClientHeartbeatRttOffsetMetricsSnapshotCadenceState`
  - current loop time
  - configured export interval
- The cadence boundary decides only whether a snapshot export is due.
- The cadence boundary can return:
  - snapshot export due with a typed snapshot export handoff
  - snapshot export not due with the next due time
  - snapshot export deferred with an explicit reason
- Deferred reasons cover missing metrics state, missing cadence state, missing interval, and empty metrics state.
- The snapshot handoff can name a future dashboard refresh consumer, but cadence does not execute dashboard refresh.
- Dashboard refresh receives only an explicit future handoff. UI rendering, dashboard storage, transport, and refresh policy remain out of scope.
- Cadence does not recalculate RTT / offset, commit metrics samples, inspect timer / retry / reconnect state, or own metrics state.

## Client Heartbeat RTT / Offset Dashboard Refresh Consumer Policy Boundary

The client dashboard refresh consumer policy is separate from snapshot export cadence and separate from actual dashboard UI rendering.

- Consumer input is derived only from:
  - `ClientHeartbeatRttOffsetMetricsFutureDashboardRefreshHandoff`
  - or `ClientHeartbeatRttOffsetMetricsSnapshotExportCadenceResult`
- The policy boundary can return:
  - refresh requested with a typed snapshot refresh request
  - refresh skipped when no dashboard handoff is available
  - refresh deferred when policy or upstream export state requires deferral
- Snapshot export not due becomes refresh skipped.
- Snapshot export deferred becomes refresh deferred with the upstream reason preserved.
- The refresh policy does not recalculate RTT / offset, commit metrics samples, evaluate snapshot cadence, render UI, write dashboard state, send network updates, or touch video / switcher / OBS paths.
- Actual dashboard UI rendering remains a later implementation that consumes only the explicit refresh request.

## Server Manual Auth-Then-Video Queue Launcher Boundary

The one-client placeholder video PoC now has a server-side manual launcher for
auth followed by one video packet:

```text
AuthRequest -> AuthResponse / accepted registry -> VideoFrame acceptance gate -> ServerVideoFrameQueueState
```

- CLI entry point: `--receive-auth-video-queue-once [config-path]`.
- The launcher owns one UDP socket, one authenticated sender registry, one
  outbound queue collection for the existing receive/send runtime, and one
  caller-owned `ServerVideoFrameQueueState`.
- The first packet is handled by the existing auth response PoC step so
  accepted and rejected auth decisions both produce an explicit `AuthResponse`.
- Only accepted auth decisions register a sender. Rejected auth does not
  register a sender and the CLI does not wait for a second packet.
- The second packet, when auth is accepted, is received through the existing
  controller receive/send runtime and packet acceptance gate.
- Queue insertion happens only by passing the resulting side effect to
  `ServerVideoFrameQueueRuntimeBoundary::store_from_receive_side_effect`.
- Accepted `VideoFrame` side effects store encoded frame metadata/payload into
  `ServerVideoFrameQueueState`; rejected, unauthenticated, or unexpected second
  packets are surfaced as not queued.
- Queue capacity uses the existing `ServerVideoFrameQueuePolicy`; when full,
  drop-oldest remains explicit in the storage result.

This launcher does not decode H.264, select target time, render switcher UI,
run a continuous loop, implement 4-view sync, or touch OBS.

## Switcher Manual Placeholder Verification Helper Boundary

The one-client placeholder video PoC now has a switcher-side helper for
verifying the in-process queue-to-placeholder handoff:

```text
caller-owned ServerVideoFrameQueueState -> latest-frame selection -> decode-deferred placeholder handoff
```

- Library entry point:
  `SwitcherPlaceholderManualVerificationBoundary::verify_latest_placeholder`.
- CLI fixture entry points:
  `--placeholder-fixture-once [client-id]` and
  `--placeholder-empty-once [client-id]`.
- The helper borrows caller-owned `ServerVideoFrameQueueState` read-only and
  composes the existing latest-frame selection and placeholder display handoff
  boundaries.
- The summary surfaces selected client id, frame id, encoded payload length,
  `decode_status=DeferredPlaceholder`, and the explicit no-frame state.
- The fixture CLI creates local queue state only for manual verification. It is
  not a bridge into a running server process.
- Server-owned in-memory queue state is not shared across processes unless a
  later explicit runtime bridge is designed.

This helper does not decode H.264, render a switcher window, select target time,
mutate queue storage, implement 4-view sync, or touch OBS.

## Server-To-Switcher Placeholder Bridge Boundary

The one-client placeholder PoC bridge is an in-process integration launcher
owned from the switcher side.

Chosen shape:

```text
switcher manual bridge launcher
  -> calls server auth-then-video queue launcher/boundary in-process
  -> receives caller-owned ServerVideoFrameQueueState
  -> calls SwitcherPlaceholderManualVerificationBoundary
  -> prints auth / queue / placeholder handoff summary
```

Implemented decision:

- Use an in-process integration helper for this PoC step.
- Do not add file, socket, or shared-memory queue sharing for this step.
- Do not make the running server process expose its private in-memory queue yet.
- Do not make `apps/server` depend on `apps/switcher`; current dependency
  direction is `switcher -> server`, so the bridge owner should be switcher or
  a later shared runtime crate.

Reasoning:

- The server queue state is intentionally caller-owned. A switcher-owned
  in-process launcher can own the server launcher outcome and pass that queue
  state directly to the existing switcher helper without pretending to share
  state across processes.
- File/socket/shared-memory export would introduce a new serialization or
  transport contract before there is a real renderer or continuous runtime that
  needs it.
- A server-side export endpoint/log fixture would verify serialization or log
  output, not the real in-memory queue-to-switcher placeholder boundary.
- This preserves the existing packet acceptance gate, server queue storage, and
  switcher placeholder boundaries.

CLI entry point:
`--receive-auth-video-placeholder-bridge-once [config-path] [client-id]`.

The bridge reuses `ServerReceiveAuthVideoQueueOnceLauncher` and then calls
`SwitcherAuthVideoPlaceholderBridgeBoundary::verify_server_outcome` on the
returned `video_queue_state`. It prints auth accepted/rejected, video
received/accepted/rejected, queued/not queued, queue length, drop-oldest,
selected client id, frame id, payload length, and
`decode_status=DeferredPlaceholder` or no-frame.

This bridge still must not decode H.264, render a window, integrate OBS, add
4-view sync, or claim cross-process queue sharing.

## Switcher H.264 Decode / Single-Frame Dump Boundary

The switcher now has the first real decode PoC boundary for one latest frame:

```text
caller-owned ServerVideoFrameQueueState
  -> latest-frame selection
  -> Annex B H.264 decode
  -> decoded BGRA frame
  -> BMP file dump
```

Current decode/display substitute behavior:

- `SwitcherH264DecodeBoundary` consumes encoded Annex B H.264 bytes plus the
  frame width/height carried by `VideoFrame` metadata.
- `SwitcherH264DecodeRuntimeHook` owns the caller-provided decode runtime.
  `SwitcherFfmpegH264DecodeRuntimeHook` is the first real implementation and
  shells out to FFmpeg with H.264 on stdin and BGRA rawvideo on stdout.
- `SwitcherDecodedFrame` carries width, height, pixel format, and raw BGRA
  pixels. It is the future input shape for real switcher rendering.
- `SwitcherH264DecodeResult` keeps decoded, deferred, and failed states
  explicit. Empty payload, invalid dimensions, and missing FFmpeg are deferred;
  FFmpeg decode failure or invalid output length is failed.
- `SwitcherSingleViewPlaceholderDisplayBoundary` can now attempt decode when a
  decode runtime is supplied. Decode success returns a real-frame handoff;
  decode deferred/failed falls back to the existing placeholder handoff and
  preserves an explicit decode status.
- `SwitcherDecodedFrameDumpBoundary` writes a single decoded BGRA frame as a
  32-bit BMP. This is the current minimal display substitute and does not open a
  window.
- `SwitcherWindowRenderBoundary` is separate from decode and BMP dump. It
  validates one `SwitcherDecodedFrame` into render input and delegates the
  actual one-shot window operation to a caller-owned render runtime hook.
- `SwitcherWindowsGdiWindowRenderRuntimeHook` is the first platform renderer. On
  Windows it opens a normal switcher window, paints one BGRA frame through GDI,
  keeps it visible for a bounded hold duration, and closes it. On non-Windows
  the default unavailable runtime returns an explicit backend-unavailable
  result.
- `SwitcherDecodeLatestFrameOnceBoundary` composes latest-frame selection,
  decode, and BMP dump for exactly one selected client frame.
- CLI `--decode-latest-frame-once [client-id] [output-path]` runs the decode
  boundary over a fixture queue.
- CLI `--receive-auth-video-decode-latest-once [config-path] [client-id]
  [output-path]` runs the existing in-process server auth/video queue launcher,
  then decodes and dumps the selected latest frame from the returned
  caller-owned queue state.
- CLI `--receive-auth-video-render-decoded-once [config-path] [client-id]
  [hold-ms]` runs the same in-process queue and decode path, then attempts to
  render the decoded frame in a normal switcher window. This is only a one-shot
  window rendering PoC and not a continuous display loop.
- The normal switcher window produced by the renderer is the future OBS Window
  Capture target. No OBS API integration is introduced here.
- `SwitcherContinuousRenderLoopBoundary` is the first bounded single-client
  continuous render loop boundary. It accepts a caller-owned latest-frame source,
  decode runtime hook, render runtime hook, and loop policy.
- The continuous render loop repeats only:
  latest-frame selection -> H.264 decode -> decoded-frame render. It records
  rendered frames, no-frame iterations, decode deferred/failed states, and
  render-not-completed states explicitly.
- The loop stops deterministically by caller-owned policy:
  `max_iterations` or `max_rendered_frames`. It does not sleep, own sockets,
  mutate queues, or keep rendering forever.
- `SwitcherQueueLatestFrameSource` is a read-only adapter over caller-owned
  `ServerVideoFrameQueueState`. Future live queue providers can implement the
  same source trait without changing decode or render boundaries.
- `SwitcherTargetTimeBoundary` calculates one targetTime from current switcher
  time, configured playout delay, and an optional clock offset estimate.
- `SwitcherJitterBufferSelectionBoundary` is the first read-only targetTime /
  jitter-buffer selector for one client. It reads that client's caller-owned
  queued encoded frames, adjusts capture timestamps by the optional offset, and
  selects the encoded frame closest to targetTime inside the configured
  early/late window.
- The jitter-buffer selector can return selected frame, no frame, waiting for
  buffer, frame too early, or frame too late/dropped states explicitly. It does
  not mutate `ServerVideoFrameQueueState`; late frames are reported as drop
  candidates for a future queue owner.
- The selected frame is still encoded H.264 plus metadata. Decode and render
  remain separate downstream boundaries.
- `SwitcherJitterBufferSelectionBoundary::select_frame_at_target_time` is a
  small adapter for callers that have already calculated a shared targetTime.
  The original one-client `select_frame` path still calculates its own
  targetTime and remains available.
- `SwitcherTwoViewTargetTimeSelectionBoundary` is the first pure 2-view
  orchestration boundary. It calculates one shared targetTime from switcher time
  and playout delay, then runs per-client jitter-buffer selection for left and
  right clients against that same targetTime.
- `SwitcherTwoViewTargetTimeSelectionPolicy` carries shared timing windows and
  independent left/right clock offset estimates. Offsets are applied to each
  client's capture timestamps during selection; they do not create different
  targetTimes for each side.
- The 2-view result is explicit:
  both selected, partial selected/unavailable, or both unavailable. Each side
  preserves the underlying one-client selected/no-frame/waiting/too-early/
  too-late status plus encoded payload and metadata.
- The 2-view orchestration is read-only over caller-owned
  `ServerVideoFrameQueueState`. It does not drop late frames, mutate queues,
  decode H.264, render windows, compose a 2-view layout, or integrate OBS.
- `SwitcherTwoViewDecodeRenderBoundary` is the first connection from 2-view
  targetTime selection to decode/render. It consumes
  `SwitcherTwoViewTargetTimeSelectionResult`, decodes only sides whose
  selection status is `Selected`, and sends each decoded BGRA frame to the
  existing one-frame window render boundary through caller-owned runtime hooks.
- The 2-view decode/render result keeps per-side outcomes explicit:
  both rendered, left rendered / right skipped, right rendered / left skipped,
  or both skipped. Skipped sides preserve the reason as selection unavailable,
  decode deferred, decode failed, render deferred, backend unavailable, invalid
  frame, or render failed.
- The 2-view decode/render connection still does not read or mutate queues,
  drop late frames, create fake placeholder frames, compose a 2-view layout,
  schedule a continuous loop, perform 4-view orchestration, or integrate OBS.
- `SwitcherTwoViewManualVerificationBoundary` is the first 2-view
  runtime/manual verification wrapper. It reads a caller-owned
  `ServerVideoFrameQueueState`, runs 2-view targetTime selection, then runs the
  2-view decode/render connection, and returns a compact per-side summary.
- CLI `--two-view-sync-fixture-once [left-client-id] [right-client-id]
  [hold-ms]` builds a deterministic two-client fixture queue and runs that
  wrapper once. It prints targetTime, left/right selection status, left/right
  decode/render status, frame id, payload length, dimensions, and adjusted
  capture timestamp. The fixture uses the real decode/render hooks, so invalid
  fixture payloads remain explicit decode failures instead of becoming fake
  rendered frames.
- The manual verification wrapper and fixture CLI still do not use live
  two-client networking, mutate queues, drop late frames, define a 2-view
  layout, schedule continuously, perform 4-view orchestration, or integrate OBS.
- `SwitcherTwoViewCompositionBoundary` is the first pure 2-view layout boundary.
  It consumes decoded/renderable left and right side inputs, not queues or H.264
  payloads, and composes BGRA frames into one side-by-side BGRA canvas.
- `SwitcherTwoViewCompositionInput::from_decode_render_result` is the adapter
  from the existing 2-view decode/render result to the composition boundary.
  Rendered sides carry their decoded BGRA frame forward; skipped sides remain
  explicit placeholder regions.
- The 2-view composition result is explicit: both composed, left only, right
  only, empty placeholder, or invalid dimensions. The composed frame preserves
  per-side selected-frame metadata when available so a future render step can
  trace the canvas back to targetTime-selected inputs.
- The 2-view composition boundary does not select targetTime frames, decode
  H.264, render a window, read or mutate queues, schedule a loop, perform
  4-view orchestration, or integrate OBS.
- `SwitcherTwoViewComposedCanvasRenderBoundary` is the first render connection
  for a composed 2-view canvas. It validates `SwitcherTwoViewComposedFrame`,
  converts the canvas to the existing one-frame window render input, and uses
  the caller-owned `SwitcherWindowRenderRuntimeHook`.
- The composed-canvas render result is explicit: rendered, render deferred,
  backend unavailable, invalid composed frame, or render failed. It reuses the
  existing Windows GDI renderer behind `cfg(target_os = "windows")` and keeps
  non-Windows as an explicit backend-unavailable result.
- CLI `--render-two-view-composed-fixture-once [hold-ms]` composes two decoded
  fixture BGRA frames and renders the resulting canvas once. It does not use
  live two-client sockets, H.264 decode, queue mutation, 4-view orchestration,
  or OBS APIs.
- `SwitcherLiveTwoViewRuntimeBoundary` is the first bounded live-like
  2-client queue/runtime integration boundary. It consumes a caller-owned
  `SwitcherLiveTwoViewQueueSource`, stores only accepted video frames into a
  fresh caller-owned `ServerVideoFrameQueueState`, and then runs one existing
  pipeline pass: 2-view targetTime selection -> H.264 decode -> 2-view
  composition -> composed-canvas render.
- The live-like queue source can be backed later by real socket receive/auth
  ownership. For this boundary, source ownership stays outside switcher
  selection/decode/render logic, and tests use deterministic scripted source
  items.
- The live 2-view runtime result keeps queue and pipeline outcomes separate:
  observed/accepted/rejected/timeout/guard counts, final queue state, per-side
  selection/decode status, composition kind, and render/deferred/failure state.
- Rejected or unauthenticated frames are not queued. Late-frame mutation/drop is
  not performed; targetTime selection still reports late candidates read-only.
- `SwitcherContinuousTwoViewSchedulingBoundary` is the first bounded
  continuous 2-view scheduler over that live-like one-pass runtime. It owns only
  logical tick cadence and guard policy, then repeatedly invokes
  `SwitcherLiveTwoViewRuntimeBoundary`.
- The continuous 2-view scheduler advances `current_switcher_time` by a
  caller-owned tick interval and records scheduler-level outcomes:
  rendered-both, rendered-partial, no frames, decode failed, render not
  completed, source ended, max ticks, and max rendered frames. The full
  per-tick live runtime result is preserved for callers that need queue and
  per-side details.
- The scheduler does not reinterpret targetTime selection, decode, composition,
  or render semantics. It does not own real UDP sockets, share queues with a
  server process, mutate late frames, drop queue entries, perform 4-view
  orchestration, or integrate OBS APIs.
- `SwitcherUdpLiveTwoViewQueueSource` is the first real UDP socket-backed
  source adapter for the live 2-view path. It can bind a UDP socket or wrap a
  caller-owned socket, receive bounded packets with timeout behavior, and emit
  `SwitcherLiveTwoViewQueueSourceItem` values for the existing live runtime and
  scheduler.
- The UDP source adapter reuses `ServerReceiveLoopStep` and the existing server
  packet acceptance gate. It does not define a new authentication policy or
  weaken server authentication. Instead, callers provide the
  `AuthenticatedSenderRegistry` that was populated by the existing auth path.
- Adapter outcomes remain explicit: accepted authenticated `VideoFrame`,
  rejected/unauthenticated video, protocol decode failure, receive failure,
  non-video packet, timeout, or source end. Accepted frames are additionally
  checked against the configured left/right client ids before they are handed
  to the queue runtime.
- The adapter owns only UDP receive/decode/gate mapping. It does not select
  targetTime frames, decode H.264, compose layouts, render windows, mutate
  queues, create authenticated registry entries, schedule ticks, perform
  4-view orchestration, or integrate OBS APIs.
- `SwitcherLiveTwoViewManualRuntimeBoundary` is the first runnable live
  two-view switcher launcher. It owns one bounded manual runtime: load the
  existing server auth config, bind or receive a caller-provided UDP socket,
  run the existing server auth response step for a bounded number of auth
  packets, keep the resulting caller-owned `AuthenticatedSenderRegistry`, pass
  that registry into `SwitcherUdpLiveTwoViewQueueSource`, and then run
  `SwitcherContinuousTwoViewSchedulingBoundary`.
- CLI `--live-two-view-switcher-once [config-path] [left-client-id]
  [right-client-id]` is the manual verification entry point for this path. It
  prints bind/client ids, auth accepted/rejected/registered counts, packet and
  queue counts, tick/render outcome counts, stop reason, and
  `bounded_manual_runtime=true`.
- The manual launcher connects existing pieces only. It does not weaken auth,
  bypass packet acceptance, hide registry state globally, redesign the UDP
  source adapter, move selection/decode/render into auth handling, implement
  late-frame queue mutation, add 4-view orchestration, or integrate OBS APIs.
- Future 4-view sync should build on the same shared-targetTime pattern and
  extend the isolated layout/composition responsibility after real 2-client
  source ownership and bounded scheduling are stable.

This decode PoC does not add a continuous loop, targetTime selection,
multi-view sync, OBS integration, decode acceleration, or packet fragmentation.
The one-shot renderer does not add continuous repaint, frame scheduling, 4-view
layout, or OBS-specific control.
The continuous loop still does not add targetTime / jitter-buffer selection,
2-view / 4-view layout, OBS-specific control, or production scheduling; those
remain separate future boundaries.
The targetTime selectors still do not decode, render, own queues, drop late
frames, perform 4-view orchestration, or integrate OBS. The 2-view decode/render
connection does decode and render selected sides, but remains separate from
selection, queue ownership, 2-view composition, continuous scheduling, 4-view,
and OBS. The 2-view fixture/manual verification wrapper only composes existing
boundaries once; it is not live networking or continuous display scheduling.
The 2-view composition boundary produces one canvas, but still remains separate
from live networking, queue mutation, continuous rendering, 4-view sync, and OBS.
The composed-canvas render boundary can display that canvas once in a normal
window, but does not own composition, scheduling, synchronization, OBS control,
or live queue integration.
The bounded live 2-view runtime composes queue ingestion and one pipeline pass,
but it still does not own real socket loops, late-frame queue mutation,
continuous scheduling, 4-view sync, or OBS control.
The continuous 2-view scheduler repeats that one-pass runtime by logical tick,
but it still does not own real socket loops, late-frame queue mutation, 4-view
sync, or OBS control.
The UDP-backed source adapter owns one-packet socket receive and server gate
mapping, but it still does not own auth registry creation, scheduling, decode,
render, queue mutation, 4-view sync, or OBS control.
The live two-view manual runtime owns bounded auth registry setup and launcher
wiring, but it still does not own continuous client acquisition, late-frame
queue mutation, 4-view sync, structured production logging, or OBS control.

## Client Real Capture / H.264 Encode Boundary

The client video path now has an explicit first boundary for replacing the
placeholder payload source later:

```text
capture source -> raw captured frame -> H.264 encoder -> encoded frame source -> VideoFrame metadata/send
```

Current implementation:

- `ClientCaptureSourceBoundary::capture_once` returns
  `Unavailable(RealCaptureDeferred)`.
- Windows MVP capture backend direction is `WindowsGraphicsCapture`.
- `ClientCaptureBackendConfig` and `ClientCaptureTargetConfig` describe the
  selected backend and target before any pixels are captured.
- `ClientCaptureTargetDiscoveryBoundary::discover_targets` is the pre-session
  target discovery boundary. It currently does not call Windows APIs or
  enumerate real targets; it returns explicit not-configured, unsupported, or
  runtime-unavailable results for the selected backend.
- `ClientCaptureTargetDiscoveryBoundary::discover_targets_with_runtime` accepts
  a `ClientCaptureTargetDiscoveryRuntimeHook`. The default hook preserves the
  current explicit unavailable behavior; a later Windows implementation can
  provide real display/window descriptors through the same result type.
- `ClientCaptureTargetDescriptor` can represent future display and window
  targets and convert them into `ClientCaptureTargetConfig` for later capture
  session creation.
- `ClientCaptureSessionConfigBoundary` converts a selected
  `ClientCaptureTargetDescriptor` or `ClientCaptureTargetConfig` into a
  metadata-only `ClientCaptureSessionConfig` for a future
  WindowsGraphicsCapture session runtime.
- Capture session config preparation can return prepared,
  backend-not-configured, unsupported-target-kind, backend-unsupported, or
  missing-target-details states explicitly. It does not require a Windows
  runtime and does not open a session.
- `ClientCaptureSessionRuntimeInput` is derived only from
  `ClientCaptureSessionConfig`.
- `ClientCaptureSessionRuntimeBoundary` consumes that input and delegates future
  WindowsGraphicsCapture session creation to a caller-owned
  `ClientCaptureSessionRuntimeHook`.
- The default session runtime hook remains the placeholder-safe path. It returns
  runtime-unavailable on Windows and backend-unsupported on non-Windows.
- `ClientWindowsGraphicsCaptureSessionRuntimeHook` is the Windows-only real
  session-creation hook. It creates a `GraphicsCaptureItem`, a
  `Direct3D11CaptureFramePool`, and a `GraphicsCaptureSession`, then returns a
  `ClientCaptureSessionRuntime` that only means "session is ready".
- The Windows session hook does not call `StartCapture`, register frame
  callbacks, call `TryGetNextFrame`, encode H.264, or send UDP packets.
- `ClientCaptureFrameAcquisitionBoundary` is the next boundary after a ready
  session runtime. It consumes a mutable `ClientCaptureSessionRuntime` and can
  attempt exactly one BGRA frame acquisition.
- `ClientCaptureFrameAcquisitionInput` carries:
  - the ready session runtime,
  - caller-provided capture timestamp,
  - nominal FPS,
  - and whether this call may start capture if the session is not started yet.
- `ClientCaptureFrameAcquisitionResult` can return:
  - one `ClientRawCapturedVideoFrame`,
  - no frame available,
  - capture not started,
  - runtime unavailable,
  - backend unsupported,
  - or acquisition failed.
- The Windows acquisition hook owns the minimal live acquisition step:
  `StartCapture` when explicitly allowed, then one `TryGetNextFrame` attempt,
  then GPU surface copy into a tightly packed BGRA8 pixel buffer. It does not
  loop, wait for frame events, encode, send, render, or touch OBS.
- `ClientRawCapturedVideoFrame` remains the raw encoder input shape: capture
  timestamp, width, height, nominal FPS, pixel format, and pixel buffer.
- Future H.264 encoder work must consume `ClientRawCapturedVideoFrame` from the
  acquisition boundary. It must not reach back into the Windows session runtime
  or frame pool directly.
- WindowsGraphicsCapture lifecycle positioning is now:
  discovery descriptor -> session config -> session runtime creation -> frame
  acquisition -> future H.264 encode.
- Primary display session creation can create a monitor capture item directly.
  Window session creation resolves the configured title to an HWND first.
  Non-primary display stable ids still require real Windows display enumeration,
  so the runtime hook returns explicit creation-deferred for that path.
- Session runtime creation can surface created, creation-deferred,
  permission-unavailable, runtime-unavailable, backend-unsupported,
  unsupported-target, invalid-target, and creation-failed states explicitly. It
  still does not acquire frames.
- `ClientCaptureSourceBoundary::probe_backend` reports:
  - capture backend not configured,
  - backend unsupported on non-Windows targets,
  - backend unavailable on Windows while the Windows Graphics Capture runtime
    integration is not wired yet,
  - or a future capture-available state.
- `ClientCaptureSourceBoundary::capture_once_with_backend` routes through the
  backend probe and returns explicit unavailable reasons; it still does not
  produce raw pixels or fake capture output.
- `ClientH264EncoderBoundary::encode_once` returns
  `Deferred(RealH264EncodeDeferred)` through the default hook for the current
  supported raw handoff, or `Deferred(UnsupportedCaptureFormat)` when a caller
  supplies an unsupported raw format.
- `ClientH264EncoderInput::from_raw_frame` converts
  `ClientRawCapturedVideoFrame` into the encoder input shape without changing
  capture ownership.
- `ClientH264EncoderRuntimeHook` is the caller-owned hook for future FFmpeg or
  hardware encoder integration. The hook returns only encoded H.264 payload
  bytes or an explicit deferred reason.
- `ClientFfmpegSoftwareH264EncoderRuntimeHook` is the first minimal real
  software encoder runtime. It invokes a caller-configured `ffmpeg` executable,
  feeds one BGRA rawvideo frame on stdin, and reads one H.264 elementary stream
  from stdout.
- The FFmpeg software hook uses `libx264`, `ultrafast`, `zerolatency`, and
  `yuv420p` by default. Its output is an Annex B H.264 elementary stream as
  produced by `ffmpeg -f h264`.
- If `ffmpeg` is not available, the hook returns `EncoderUnavailable`. Invalid
  dimensions, invalid BGRA buffer length, FFmpeg process failure, unavailable
  `libx264`, or empty stdout return `EncodeFailed`.
- Hardware encoder integration remains deferred. It should use the same
  `ClientH264EncoderRuntimeHook` contract or a compatible caller-owned runtime
  hook without changing capture or UDP send boundaries.
- `ClientH264EncoderBoundary::encode_once_with_runtime` wraps successful hook
  output into `ClientEncodedVideoFrameSource` with
  `source_kind=RealCaptureH264`. Empty hook payloads become explicit
  `EncodeFailed`.
- H.264 encoder states are explicit:
  - encoded,
  - real encode deferred,
  - unsupported pixel format,
  - encoder unavailable,
  - encode failed.
- `ClientEncodedVideoFrameSource` carries capture timestamp, dimensions,
  nominal FPS, codec, payload bytes, and source kind.
- Source kind is explicit:
  - `PlaceholderH264` for caller-provided placeholder bytes.
  - `RealCaptureH264` for future real capture + H.264 encoder output.
- `ClientVideoFrameMetadataConstructionBoundary::build_frame_from_encoded_source`
  can construct an existing protocol `VideoFrame` from an encoded source without
  changing the UDP send boundary.
- `ClientRealEncodedVideoFrameOneShotBoundary` is the first one-shot send path
  for real encoded capture output. It consumes a caller-owned ready
  `ClientCaptureSessionRuntime`, a caller-owned UDP socket, one frame
  acquisition runtime hook, and one H.264 encoder runtime hook.
- The real encoded one-shot path composes existing boundaries only once:
  capture session runtime -> one BGRA frame acquisition -> H.264 encode ->
  `RealCaptureH264` encoded source -> `VideoFrame` metadata construction -> one
  UDP send.
- Its result keeps stopped states explicit: sent, capture unavailable, no frame
  available, encode unavailable/failed, frame build failed, or send failed.
- This one-shot path is not a continuous streaming loop and does not create
  capture sessions, enumerate targets, retry, decode, render, integrate OBS, or
  run 4-view sync.
- The manual CLI entry point
  `--real-encoded-video-frame-poc-once [config-path]` wires this path for
  primary-display verification. It prints sent frame id, capture timestamp,
  dimensions, encoded payload length, destination, and
  `source_kind=RealCaptureH264` on success, and explicit not-sent reasons on
  failure.
- The authenticated manual CLI entry point
  `--auth-real-encoded-video-frame-poc-once [config-path]` is the queue-E2E
  verification shape for real encoded frames. It binds one UDP socket, sends
  `AuthRequest`, requires `AuthResponse.accepted=true`, then creates the
  capture session and sends one `RealCaptureH264` `VideoFrame` through the same
  socket/source. It does not bypass or weaken the server packet acceptance gate.
- `ClientContinuousRealEncodedVideoFrameBoundary` is the smallest bounded
  repeated sender over the existing one-shot real encoded boundary. It consumes
  a caller-owned ready `ClientCaptureSessionRuntime`, caller-owned UDP socket,
  frame acquisition hook, and H.264 encoder hook, then repeats acquisition,
  encode, metadata construction, and send until max frames, max ticks, frame
  wait timeout, capture failure, or send failure.
- Its per-run summary keeps attempted, captured, encoded, sent, no-frame,
  capture-failure, encode-failure, frame-build-failure, send-failure, and stop
  reason values explicit. It does not implement selection, decode, render,
  switcher scheduling, late-drop mutation, OBS, or 4-view logic.
- The authenticated bounded manual CLI entry point
  `--auth-real-encoded-video-frame-poc-bounded [config-path] [max-frames]`
  owns one same-source auth round trip, creates one capture session after auth
  acceptance, then runs the bounded repeated sender on the same UDP socket. Auth
  rejection stops before session creation, capture, encode, and video send.
- The video-only real encoded CLI remains a low-level capture/encode/send check.
  The authenticated CLI is required when the goal is to prove accepted server
  queue insertion, because the server registry is keyed by authenticated source.
- The existing placeholder PoC remains available and continues to use explicit
  placeholder payload behavior.

Responsibility split:

- capture source
  - Owns backend selection/probe and is the future owner of OS/window/game
    capture and raw pixel frame production.
  - Does not encode H.264, construct protocol messages, or send UDP packets.
- target discovery
  - Future owner of enumerating display/window targets for
    WindowsGraphicsCapture.
  - Produces descriptors/config references only.
  - Uses an injectable runtime hook for future Windows API-backed enumeration.
  - Does not create capture sessions, acquire frames, encode video, construct
    protocol messages, or send UDP packets.
- capture session config
  - Converts the selected descriptor or target config into future
    WindowsGraphicsCapture session metadata.
  - Keeps missing display/window details explicit before runtime creation.
  - Does not create sessions, request permissions, acquire frames, encode
    video, construct protocol messages, or send UDP packets.
- capture session runtime
  - Consumes only prepared `ClientCaptureSessionConfig` through
    `ClientCaptureSessionRuntimeInput`.
  - Delegates OS-specific session creation to a caller-owned runtime hook.
  - Produces an opaque future session runtime handoff or an explicit
    not-created reason.
  - Does not enumerate targets, acquire frames, encode video, construct
    protocol messages, or send UDP packets.
- H.264 encoder
  - Owns the boundary and hook contract for converting raw captured frames into
    encoded H.264 payloads.
  - The default hook remains deferred for placeholder-safe behavior.
  - The first real software implementation is
    `ClientFfmpegSoftwareH264EncoderRuntimeHook`, which shells out to FFmpeg for
    one BGRA frame -> Annex B H.264 elementary stream encode.
  - Future hardware integration should sit behind
    `ClientH264EncoderRuntimeHook`.
  - Converts successful hook output to `RealCaptureH264` only after non-empty
    encoded bytes are returned.
  - Does not capture pixels, choose frame ids, or send packets.
- encoded frame source / metadata boundary
  - Preserves capture timestamp, frame id relationship, dimensions, payload
    length, codec, and existing `VideoFrame` metadata construction.
  - Does not claim placeholder bytes are real capture output.
- real encoded one-shot send
  - Composes the ready capture runtime, acquisition hook, H.264 encoder hook,
    metadata boundary, and existing send boundary for exactly one frame.
  - Stops before encode/send when capture is unavailable or no frame exists.
  - Stops before send when encode is unavailable or failed.
  - Does not own session creation, target enumeration, continuous acquisition,
    retry, decode, rendering, sync, or OBS.
- auth + real encoded one-shot launcher
  - Owns only the manual same-source composition: one UDP socket, auth request /
    accepted auth response, then the existing real encoded one-shot sender on
    the same socket.
  - Reuses the real encoded one-shot boundary; it does not implement its own
    capture, encode, metadata, or send behavior.
  - Stops before session creation, capture, encode, and video send when auth is
    rejected or times out.
- bounded real encoded sender
  - Reuses the existing real encoded one-shot boundary for each tick.
  - Owns only bounded loop policy and summary accounting: max frames, max ticks,
    frame wait timeout, optional cadence sleep, and explicit stop reason.
  - Does not reinterpret capture, encode, metadata, or send failures.
- auth + bounded real encoded launcher
  - Owns only same-source auth, one capture-session creation, and bounded sender
    startup.
  - Keeps auth failure before capture/encode/send and preserves the one-shot
    auth CLI.
- send boundary
  - Continues to encode and send `VideoFrame` over caller-owned UDP sockets.
  - Does not know whether payload bytes came from placeholder or future real
    capture/encode.

Windows API-backed target enumeration, OS event-driven continuous frame
acquisition, production encoder configuration, hardware encoder integration,
packet fragmentation, decode, switcher rendering, late-frame drop mutation,
4-view sync, and OBS integration remain future work.
