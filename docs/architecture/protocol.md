<!-- stream-sync/docs/architecture/protocol.md -->

# StreamSync Protocol Design

## PoC / MVP initial wire byte layout

PoC / MVP 初期の wire format は、UDP packet の先頭に 16 byte の固定ヘッダを置き、その後ろに message type ごとの可変長 payload を続ける。

この段階では encode / decode の本実装、fragmentation、再送制御、暗号化は行わない。byte layout の目的は、受信直後に `message_type` と `protocol_version` を小さい固定領域だけで判定できるようにすることに限定する。

### Fixed packet header

数値フィールドは little-endian とする。

| Offset | Size | Field | Type | Notes |
| ---: | ---: | --- | --- | --- |
| 0 | 2 | `message_type` | `u16` | packet 種別。payload decode 前に読む。 |
| 2 | 2 | `header_length` | `u16` | 初期値は `16`。将来ヘッダ拡張時に payload 開始位置を保つ。 |
| 4 | 4 | `protocol_version` | `u32` | wire format の互換性判定用。payload decode 前に読む。 |
| 8 | 4 | `payload_length` | `u32` | header 後続の payload byte 数。 |
| 12 | 2 | `flags` | `u16` | 初期値は `0`。将来用。 |
| 14 | 2 | `reserved` | `u16` | 初期値は `0`。受信時は現時点で意味を持たせない。 |

固定ヘッダの合計は 16 byte とする。payload は `header_length` の位置から始まり、`payload_length` byte だけ続く。

### Field policy

- `message_type`
  - packet 先頭 2 byte に置く。
  - Rust 側の `MessageType` は `repr(u16)` の識別子と対応させる。
  - 未知の値は payload decode に進まず、packet 破棄または protocol error として扱う。
- `protocol_version`
  - offset 4 の `u32` として置く。
  - 受信側は payload decode 前に対応 version と一致するか確認する。
  - MVP では複数 protocol version の同時対応はしない。
- `payload_length`
  - offset 8 の `u32` として置く。
  - `payload_length` は固定ヘッダを含まない。
  - 実受信 byte 数が `header_length + payload_length` と一致しない packet は不正として扱う。
- 可変長 payload
  - payload の中身は `message_type` ごとに定義する。
  - 文字列や配列などの可変長データは payload 内で長さ情報を持つ方針とする。
  - `VideoFrame` の H.264 data は payload 内の frame metadata の後ろに置き、protocol crate は H.264 の中身を解釈しない。

### Payload primitive encoding

各 message payload 内の数値フィールドは fixed header と同じく little-endian とする。

初期 payload layout では、可変長データの扱いを以下に統一する。

- `string`
  - `u16 byte_length` の直後に UTF-8 bytes を置く。
  - `byte_length` は終端 NUL を含まない byte 数とする。
  - `byte_length = 0` は空文字列を表す。
- `optional<T>`
  - `u8 present` を先に置く。
  - `present = 0` は値なし、`present = 1` は直後に値ありとする。
  - `present` が `0` の場合、後続の値領域は置かない。
- `bytes`
  - `u32 byte_length` の直後に bytes を置く。
  - `VideoFrame` の H.264 payload は、専用 field の `payload_size: u32` をこの length として使い、追加の length prefix は置かない。
- `bool`
  - `u8` として置き、`0 = false`, `1 = true` とする。
- `timestamp`
  - `u64` の microseconds として置く。
- `codec`
  - `u16` として置く。初期値は `1 = H264` のみ定義する。

### Common header scope

共通ヘッダ化する範囲は、初期段階では上記の 16 byte fixed packet header までに限定する。

理由:
- `message_type` と `protocol_version` は全 packet で最初に必要になる。
- `payload_length` は packet boundary と可変長 payload の検証に必要になる。
- `client_id` / `run_id` / `sent_at` / `sequence_number` は message によって必要性や意味が異なるため、初期 fixed header には入れない。
- ヘッダを大きくしすぎると PoC の encode / decode 実装開始時に負担が増える。

### AuthRequest and VideoFrame split

`AuthRequest` と `VideoFrame` は、固定ヘッダのみを共通化する。

`AuthRequest`:
- fixed header の `message_type = AuthRequest` で識別する。
- `protocol_version` は fixed header で先に検証する。
- `client_id`, `run_id`, `app_version`, `shared_token` などの認証情報は payload に置く。
- 未認証 client の packet でも、server は固定ヘッダだけ読めば protocol mismatch と message type を判定できる。

`VideoFrame`:
- fixed header の `message_type = VideoFrame` で識別する。
- `protocol_version` は fixed header で先に検証する。
- `client_id`, `run_id`, `frame_id`, `capture_timestamp`, `send_timestamp`, `is_keyframe`, `width`, `height`, `fps_nominal`, `codec` は payload 先頭側の frame metadata に置く。
- H.264 encoded bytes は frame metadata の後ろに可変長 data として置く。
- `payload_length` は frame metadata と H.264 data の合計 byte 数を表す。

### Rust support

`crates/protocol` には、docs とズレないように最小限の補助として以下を置く。

- `FIXED_HEADER_LEN = 16`
- fixed header 各フィールドの offset 定数
- `FixedHeader` 型
- `PacketView<'a>` 型
- 16 byte fixed header decode の最小実装
- `AuthRequest` payload decode の最小実装
- `Heartbeat` payload decode の最小実装
- `VideoFrame` payload decode の最小実装
- fixed header encode の最小補助
- `AuthRequest` payload encode の最小実装
- `AuthResponse` payload encode の最小実装
- `HeartbeatAck` payload encode の最小実装
- `VideoFrame` payload encode の最小実装
- payload 用の長さ prefix / optional tag / H.264 codec 値 / VideoFrame numeric metadata 長の定数

fixed header decode は `message_type`, `header_length`, `protocol_version`, `payload_length`, `flags`, `reserved` を little-endian で読む。`header_length` は現時点では `16` のみを受理し、未知の `message_type`、短すぎる packet、`payload_length` と実 byte 数の不一致は `ProtocolError` とする。

`protocol_version` の期待値チェックは、fixed header decode 後、payload decode 前に `DecodeContext.expected_protocol_version` と `FixedHeader.protocol_version` を比較する最小実装を置く。不一致時は `ProtocolError::UnsupportedProtocolVersion` を返す。
payload decode は現時点では `AuthRequest`, `Heartbeat`, `VideoFrame` に限定する。`AuthRequest` payload decode は fixed header decode と protocol_version 期待値チェックが済んだ後に、`client_id`, `run_id`, `app_version`, `shared_token`, `display_name` を docs の byte layout どおりに読む。`Heartbeat` payload decode は同じ前提で、`client_id`, `run_id`, `sent_at`, `local_time`, `short_status` を読む。`VideoFrame` payload decode は同じ前提で、`client_id`, `run_id`, 46 byte numeric metadata, H.264 bytes を読む。H.264 bytes は `payload_size` と実際の残り byte 数が一致する場合だけ `Vec<u8>` として復元し、codec decode や映像内容の解釈は行わない。
payload encode は現時点では `AuthRequest`, `AuthResponse`, `HeartbeatAck`, `VideoFrame` に限定する。`AuthRequest` encode は fixed header の `message_type = AuthRequest`、`protocol_version`、`payload_length` を書き、payload に `client_id`, `run_id`, `app_version`, `shared_token`, `display_name` を docs の byte layout どおりに書く。`AuthResponse` encode は fixed header の `message_type = AuthResponse`、`protocol_version`、`payload_length` を書き、payload に `client_id`, `run_id`, `accepted`, `reason_code`, `message`, `server_time`, `expected_protocol_version` を docs の byte layout どおりに書く。`HeartbeatAck` encode は fixed header の `message_type = HeartbeatAck`、`protocol_version`、`payload_length` を書き、payload に `client_id`, `run_id`, `echoed_sent_at`, `server_received_at`, `server_sent_at` を docs の byte layout どおりに書く。`VideoFrame` encode は fixed header の `message_type = VideoFrame`、`protocol_version`、`payload_length` を書き、payload に `client_id`, `run_id`, 46 byte numeric metadata, H.264 bytes を docs の byte layout どおりに書く。H.264 bytes は protocol crate では変換せず、`payload_size` は実 payload byte 長から決める。その他 message の payload decode / encode はまだ本実装しない。

## Encode / decode API boundary

PoC / MVP 初期では、`crates/protocol` は wire format と message 型の境界を定義する。UDP socket、送受信ループ、認証済み client 管理、server / client / switcher 側 handler は持たない。

この段階では fixed header decode、protocol_version 期待値チェック、`AuthRequest` / `Heartbeat` / `VideoFrame` payload decode、`AuthRequest` / `AuthResponse` / `HeartbeatAck` / `VideoFrame` payload encode の最小実装だけを置く。その他 payload decode と encode の本実装は行わない。

### Responsibility split

`crates/protocol` の責務:
- fixed header layout、offset、message type、protocol version 型を定義する
- fixed header decode の入口を定義する
- `message_type` による payload decode 分岐の入口を定義する
- payload decoder の dispatch helper を定義する
- encode の入口を定義する
- wire format 上の error 型を定義する
- H.264 payload の中身は解釈しない

`crates/net-core` の責務:
- 将来の UDP 受信層から raw packet bytes と送信元 address を受け取る
- raw packet bytes と送信元 address を `InboundPacket` として保持する
- protocol crate の fixed header decode、protocol_version check、payload decoder dispatch を順番に呼ぶ
- decode 済み message と送信元 metadata を `DecodedInboundPacket` として app / server handler 側へ返す
- 実際の UDP socket loop や handler 実行はまだ持たない
- 将来の fragmentation / 再送制御を扱う場合も protocol crate ではなく net 側で境界を持つ

`apps/client` / `apps/server` / `apps/switcher` の責務:
- `protocol_version` の期待値を決める
- protocol crate が返した message を app の状態に反映する
- 認証状態、clientId whitelist、heartbeat timeout、buffer 管理、UI 表示を扱う
- protocol error をログや切断判断へ変換する

`apps/server` の inbound handler 境界:
- `net-core` から `DecodedInboundPacket` を受け取る
- `DecodedInboundPacket.message` の `ProtocolMessage` variant、つまり `message_type` 相当の意味で分岐する
- `AuthRequest` は認証処理へ、`Heartbeat` は生存確認 / RTT 材料の処理へ、`VideoFrame` は受理判定 / 同期バッファ処理へ渡す
- 認証成功 / 失敗判定、heartbeat timeout 管理、video frame の受理 / 破棄 / buffer 投入は server 側の責務とし、`protocol` / `net-core` には入れない

### Decode boundary

受信時の想定順序:

1. UDP 受信層が raw packet bytes と送信元 address を得る。
2. app 側は `DecodeContext { expected_protocol_version }` を用意する。
3. `net-core` が raw packet bytes と送信元 address を `InboundPacket` として受け取る。
4. `net-core` が protocol crate の `decode_fixed_header` を呼び、16 byte fixed header を読む。
5. fixed header decode は `message_type`, `header_length`, `protocol_version`, `payload_length`, `flags`, `reserved` と payload slice の境界だけを返す。
6. `net-core` が `DecodeContext` と fixed header を使い、protocol crate の `validate_protocol_version` で payload decode 前に一致を確認する。
7. `net-core` が protocol crate の `decode_payload_by_message_type` を呼び、`message_type` に応じた payload decoder を dispatch する。
8. payload decoder は `AuthRequest`, `Heartbeat`, `VideoFrame` などの message 型へ変換する。
9. `net-core` は送信元 metadata と decode 済み message を `DecodedInboundPacket` として app / server handler 側へ返す。
10. app / server handler が認証、同期、buffer、表示などの処理を行う。

fixed header decode は packet の構造確認だけを行う入口とする。認証済み client かどうか、VideoFrame を受け入れるかどうか、古い frame を破棄するかどうかは app / sync 側の責務とする。

### Encode boundary

送信時の想定順序:

1. app 側が送信したい `ProtocolMessage` 相当の message 型を作る。
2. app 側が `EncodeContext { protocol_version }` を用意する。
3. protocol crate の encode 入口が fixed header と payload を 1 packet buffer に書く。
4. `payload_length` は protocol crate の encode 入口で決める。
5. `net-core` が完成した byte buffer を UDP datagram として送信する。

encode 入口は byte buffer 作成までに限定する。送信先 address、retry、送信間隔、socket error の扱いは protocol crate に入れない。

### Boundary types

`crates/protocol` には、本実装前の境界として以下を置く。

- `PacketView<'a>`
- `DecodeContext`
- `EncodeContext`
- `ProtocolMessage`
- `ProtocolError`
- `FixedHeaderDecoder`
- `PayloadDecoder`
- `MessageEncoder`
- `decode_payload_by_message_type`

`FixedHeaderCodec` と `decode_fixed_header` は、16 byte fixed header の byte parsing と payload slice の切り出しだけを行う。`decode_payload_by_message_type` は fixed header の `message_type` に応じて payload decoder を選ぶ dispatch helper とする。`AuthRequestPayloadDecoder` / `decode_auth_request_payload` は `AuthRequest` payload のみを、`HeartbeatPayloadDecoder` / `decode_heartbeat_payload` は `Heartbeat` payload のみを、`VideoFramePayloadDecoder` / `decode_video_frame_payload` は `VideoFrame` payload のみを型へ変換する。`MessageEncoder` は境界名と責務を固定するための入口であり、現時点では `AuthRequest`, `AuthResponse`, `HeartbeatAck`, `VideoFrame` の fixed header + payload bytes を書く。その他 message は `EncodeNotImplemented` を返す。

### net-core call boundary

`crates/net-core` は `protocol` crate の decode entry point を呼ぶ橋渡しを担当する。`InboundPacket`, `PacketSource`, `InboundPacketDecoder`, `DecodedInboundPacket`, `NetDecodeError` に加え、現在は同期 `UdpSocket` の 1 packet receive / send adapter だけを持つ。継続 receive loop、async runtime、retry、fragmentation、encryption、app handler 呼び出しは実装しない。

`InboundPacketDecoder` の最小処理順:

1. `InboundPacket.bytes` を `protocol::decode_fixed_header` に渡す。
2. 得られた `FixedHeader` を `protocol::validate_protocol_version` で検証する。
3. `protocol::decode_payload_by_message_type` で `message_type` に応じた payload decoder を呼ぶ。
4. 成功時は `PacketSource` と `ProtocolMessage` を `DecodedInboundPacket` にまとめる。
5. 失敗時は `PacketSource` と `ProtocolError` を `NetDecodeError::Protocol` として返す。

### server handler boundary

`apps/server` は `net-core` から `DecodedInboundPacket` を受け取り、decode 済み message の種類に応じて server 側処理へ分岐する。初期実装では `ServerInboundRouter` / `ServerInboundRoute` を境界 placeholder とし、`AuthRequest`, `Heartbeat`, `VideoFrame` の route だけを定義する。

責務分離:

- `protocol`
  - fixed header と payload を decode し、`ProtocolMessage` を返す。
  - 認証状態、送信元 address、heartbeat timeout、frame buffer は扱わない。
- `net-core`
  - raw packet bytes と送信元 metadata を受け取り、protocol decode を呼ぶ。
  - 成功時に `DecodedInboundPacket` を返す。
  - app handler の実行や server 状態変更は行わない。
- `apps/server`
  - `DecodedInboundPacket` を受け取り、message 種別ごとの server handler に振り分ける。
  - 認証、heartbeat、video frame の処理責務を持つ。
  - 現時点では route 分類のみを置き、認証判定、timeout 管理、video frame 処理本体は実装しない。

### server UDP receive loop boundary

`apps/server` の UDP 受信 loop は、将来の socket 層で packet bytes と送信元 address を受け取った後の制御境界とする。現時点では `ServerReceiveLoopStep` を 1 packet 分の placeholder として置く。

最小 flow:

1. socket 層が packet bytes を受信する。
2. socket 層が送信元 address を取得する。
3. server receive loop が `PacketSource` と `InboundPacket` を作る。
4. server receive loop が `InboundPacketDecoder` を呼ぶ。
5. decode 成功時、server receive loop が `DecodedInboundPacket` を `ServerInboundRouter` へ渡す。
6. `ServerInboundRouter` が `ServerInboundRoute` を返す。
7. decode 成功時、server receive loop が `PacketAcceptanceGateBoundary` を呼ぶ。
8. accepted の route だけが handler / router 後続境界へ進む。
9. rejected は handler 本体へ渡さず、将来の drop / log layer へ渡す decision として返す。

decode error / protocol error の初期方針:

- `UnsupportedProtocolVersion`
  - `RejectProtocolVersion` として分類する。
  - packet は破棄する。拒否応答は encode 実装後に検討する。
- `PayloadDecodeNotImplemented`
  - `UnsupportedInboundMessage` として分類する。
  - packet は破棄する。server inbound として扱う message は別タスクで増やす。
- その他の `ProtocolError`
  - `DropPacket` として分類する。
  - malformed packet とみなし、handler 本体へは渡さない。

receive loop と gate の接続境界:

- `ServerReceiveLoopStep`
  - raw packet bytes と `PacketSource` を受け取る 1 packet 分の placeholder。
  - decode 成功後に `ServerInboundRouter` で `ServerInboundRoute` を作る。
  - gate 接続版では route 作成後に `PacketAcceptanceGateBoundary` を呼ぶ。
- `PacketAcceptanceGateBoundary`
  - decode 済み route と `AuthenticatedSenderRegistry` から accepted / rejected を判断する。
  - `AuthRequest` は registry 登録前の入口として accepted にする。
  - `Heartbeat` / `VideoFrame` は `client_id` と endpoint を registry に照合する。
- handler / router 後続境界
  - accepted route だけを受け取る。
  - heartbeat 処理や video frame 処理本体はまだ実装しない。
- drop / log layer
  - decode rejection と acceptance rejection を区別して受け取る将来境界。
  - 実際の packet 破棄と JSON Lines ログ出力はまだ実装しない。

Current implementation: `apps/server::ServerReceiveLoopStep` has
`handle_received_packet_with_gate`, which returns
`ServerReceiveLoopGateOutcome::Accepted` for accepted routes and
`ServerReceiveLoopGateOutcome::Rejected` with either decode rejection or packet
acceptance rejection. この境界では継続 socket loop、非同期 runtime 導入、
実際の packet 破棄、ログ出力、認証成功 / 失敗判定、heartbeat
管理、video frame 処理本体は行わない。

### UDP Socket Minimal I/O Boundary

The current UDP implementation is a synchronous one-datagram adapter. It
connects OS socket I/O to the existing receive and send boundaries without
introducing an async runtime, retry, fragmentation, encryption, or queue
runtime behavior.

Receive path:

1. A caller owns a bound `std::net::UdpSocket` and a mutable receive buffer.
2. `net-core::UdpSocketIoBoundary::receive_one` calls `recv_from` once.
3. The socket adapter returns `UdpReceivedPacket`, carrying `PacketSource` and
   the received byte slice.
4. `apps/server::ServerUdpSocketIoStep::receive_one_with_gate` passes those
   bytes and source into `ServerReceiveLoopStep::handle_received_packet_with_gate`.
5. The existing receive loop performs decode, route, gate, and accepted /
   rejected outcome selection.

Send path:

1. The outbound queue / encoder boundary produces `EncodedOutboundPacket`.
2. `net-core::UdpSocketIoBoundary::send_encoded` receives encoded bytes plus
   `PacketDestination`.
3. The socket adapter calls `send_to` once and returns the sent byte count.
4. `apps/server::ServerUdpSocketIoStep::send_encoded` is the server-side thin
   adapter over that generic socket send.

Responsibility split:

- UDP socket adapter
  - Owns bind helper, one `recv_from`, and one `send_to`.
  - Does not decode messages, route handlers, retry, fragment, encrypt, or log.
- receive loop
  - Owns decode -> route -> gate after bytes and source are available.
  - Does not own OS socket setup or blocking I/O.
- protocol encoder / net send layer
  - Owns typed message to encoded packet conversion before socket send.
  - Does not own the OS socket.
- future runtime / queue policy
  - Will own continuous receive/send orchestration, backpressure, retry, and
    async integration later.

Current implementation: `net-core::UdpSocketIoBoundary`,
`net-core::UdpReceivedPacket`, and `apps/server::ServerUdpSocketIoStep`.
Continuous socket loops, async runtime, retry, fragmentation, encryption,
actual packet drop, and JSON Lines output remain future work.

## 1. 目的

このドキュメントは、StreamSync の MVP 段階における通信プロトコルの初期設計を定義するものです。

主な目的は以下です。

- client と server の間でやり取りするメッセージの種類を定義する
- 認証、heartbeat、映像フレーム、stats の基本構造を定義する
- protocol_version を使った互換性管理の土台を作る
- 実装前に責務とメッセージ境界を明確にする

この段階では、完全なバイナリ仕様を固定しきることよりも、MVP に必要なメッセージ構造と流れを明確にすることを優先する。

---

## 2. 前提

- 通信方式は UDP 独自プロトコルを採用する
- client は中央 server に直接 UDP 送信する
- server は認証済み client のパケットのみ受理する
- protocol_version が一致しない client は接続拒否する
- app_version 差異は warn ログとして扱う
- MVP では 4人固定を前提とする
- 音声はプロトコル対象外とする

---

## 3. 通信の基本方針

### 3.1 方針
- 低遅延を優先する
- 古いフレームは再送より破棄を優先する
- 受信後に server 側で同期する
- メッセージ種別ごとに責務を明確に分ける
- MVP では複雑な再送制御や輻輳制御は実装しない

### 3.2 想定する主なメッセージ種別
- 認証メッセージ
- 認証応答メッセージ
- heartbeat メッセージ
- 映像フレームメッセージ
- stats メッセージ
- 任意のエラーメッセージまたは拒否通知

---

## 4. バージョン管理

### 4.1 app_version
各アプリの配布物バージョン。

用途:
- ログ
- 警告表示
- 開発時の整合確認

### 4.2 protocol_version
通信仕様の互換性を表すバージョン。

用途:
- 認証時の互換性確認
- 仕様差分の切り分け
- 破壊的変更時の拒否判定

### 4.3 MVP ルール
- protocol_version 不一致は接続拒否
- app_version 差異は warn ログ
- protocol_version は整数で管理する

---

## 5. メッセージ共通ヘッダ

すべてのメッセージに、少なくとも以下の共通情報を含める想定とする。

### 共通フィールド
- message_type
- protocol_version
- client_id
- run_id
- sequence_number
- sent_at

### フィールド概要
- `message_type`
  - メッセージ種別
- `protocol_version`
  - 通信仕様バージョン
- `client_id`
  - 送信元クライアント識別子
- `run_id`
  - セッション識別子
- `sequence_number`
  - 各メッセージ系列の順序確認用
- `sent_at`
  - 送信時刻

### timestamp 方針
- protocol 内の timestamp 単位は **マイクロ秒** に統一する
- Rust 側では `TimestampMicros` として扱い、生の `u64` を直接 timestamp として扱うことを避ける
- `TimestampMicros` の内部値は、該当する clock domain におけるマイクロ秒単位のカウント値とする
- `capture_timestamp`, `send_timestamp`, `sent_at`, `local_time` は、送信元 client 側の時刻を表す
- `server_time`, `server_received_at`, `server_sent_at` は、server 側の時刻を表す
- `echoed_sent_at` は、heartbeat で受け取った `sent_at` をそのまま返す
- PoC / MVP では単調増加する時計を優先し、Unix epoch などの絶対時刻への固定は wire format 確定時に再検討する
- RTT / offset / targetTime 計算では、どの clock domain の timestamp かを区別して扱う

### 備考
実装段階では、共通ヘッダを全メッセージで完全に同一形式にするか、メッセージごとに軽量化するかを調整してよい。ただし、以下は常に取れるようにする。

- 送信元識別
- protocol_version 識別
- 順序の概算確認
- 時刻情報の取得

---

## 6. シリアライズ / デシリアライズ方針

### 6.1 基本方針
PoC / MVP では、JSON ではなく **バイナリ寄りの独自 wire format** を前提にする。

理由:
- UDP で送るため、payload 以外のオーバーヘッドを小さくしたい
- 映像フレームでは payload が大きく、message metadata は軽量に扱いたい
- `protocol_version` と `message_type` を早い段階で読める構造にしたい
- 将来、手書きバイナリ実装へ移行しやすい形にしたい

ただし、現時点では完全な byte layout は固定しない。まず Rust 型と message 境界を固め、PoC の実装段階で最小 wire format を確定する。

### 6.2 PoC / MVP の wire format 方針
- すべての packet は、先頭に固定長の最小 envelope を持つ方針とする
- envelope には最低限 `protocol_version` と `message_type` を含める
- `protocol_version` と `message_type` は payload decode 前に読める位置に置く
- 数値型は、実装時に little-endian に統一する方針とする
- 可変長文字列や `payload` は、長さ情報を付けて読む方針とする
- `VideoFrame.payload` は圧縮済み H.264 データをそのまま保持し、protocol crate では映像データの中身を解釈しない
- payload fragmentation / 再送制御 / 暗号化は、この段階では wire format に組み込まない
- 不正な長さ、未知の message type、protocol mismatch は decode 失敗または破棄対象とする

### 6.3 protocol_version の扱い
- `protocol_version` は wire format の互換性を表す整数値とする
- 受信側は、payload 本体を解釈する前に `protocol_version` を確認する
- `protocol_version` が一致しない packet は、原則として接続拒否または破棄する
- MVP では複数 protocol_version の同時サポートは行わない
- `app_version` は wire format 互換性判定には使わず、ログや警告表示に使う

### 6.4 message_type の扱い
- `message_type` は wire 上の message 種別を表す数値として扱う
- Rust 側では `MessageType` enum で表現する
- `MessageType` の wire 識別子は以下を初期値とする
  - `1`: `AuthRequest`
  - `2`: `AuthResponse`
  - `3`: `Heartbeat`
  - `4`: `HeartbeatAck`
  - `5`: `VideoFrame`
  - `6`: `ClientStats`
  - `7`: `ServerNotice`
- 未知の `message_type` は decode 失敗として扱う
- message ごとの必須フィールド不足は decode 失敗または packet 破棄として扱う

### 6.5 実装段階の責務分離
- `crates/protocol` は message 型、wire 識別子、将来の encode / decode 境界を持つ
- UDP socket の送受信、再送、fragmentation、送信元認証状態の管理は `crates/protocol` では行わない
- server / client / switcher 側 handler の分岐処理は、各 app または net / sync 系 crate で扱う
- 本ドキュメント時点では encode / decode trait や実装の詳細は固定しない

---

## 7. メッセージ種別定義

### 7.1 AuthRequest
client が server に対して送る初期認証メッセージ。

#### 目的
- client_id の提示
- shared_token の提示
- protocol_version の提示
- app_version の提示
- 表示名などの任意情報の提示

#### 必須フィールド
- message_type = `auth_request`
- protocol_version
- client_id
- run_id
- app_version
- shared_token

#### 任意フィールド
- display_name
- capabilities
- requested_video_profile

#### Payload byte layout

fixed header の `message_type = AuthRequest` の後続 payload は以下の順序とする。

| Order | Field | Wire type | Notes |
| ---: | --- | --- | --- |
| 1 | `client_id` | `string` | `u16 byte_length` + UTF-8 bytes。server whitelist と照合する識別子。 |
| 2 | `run_id` | `string` | `u16 byte_length` + UTF-8 bytes。client 起動単位または session 単位の識別子。 |
| 3 | `app_version` | `string` | `u16 byte_length` + UTF-8 bytes。互換性判定には使わず warn / log 用。 |
| 4 | `shared_token` | `string` | `u16 byte_length` + UTF-8 bytes。MVP の事前共有 token。 |
| 5 | `display_name` | `optional<string>` | `u8 present` の後、存在する場合だけ `u16 byte_length` + UTF-8 bytes。 |

PoC / MVP 初期の wire layout では、`capabilities` と `requested_video_profile` は Rust 型に残すが payload にはまだ置かない。必要になった段階で `protocol_version` を意識して追加する。

`crates/protocol` の最小実装では、上記 payload を `AuthRequest` 型へ復元し、同じ順序で fixed header + payload bytes へ encode する。`capabilities` は空配列、`requested_video_profile` は `None` として扱い、認証成功 / 失敗判定は app / server 側に残す。

#### 備考
- server はこのメッセージを受けて認証を行う
- 未認証状態では、映像フレームは受理しない

---

### 7.2 AuthResponse
server が client に返す認証応答。

#### 目的
- 認証成功 / 失敗の通知
- protocol_version 不一致の通知
- 必要に応じたエラー理由の通知

#### 必須フィールド
- message_type = `auth_response`
- protocol_version
- client_id
- run_id
- accepted
- reason_code

#### 任意フィールド
- message
- server_time
- expected_protocol_version

#### reason_code 例
- `ok`
- `invalid_token`
- `unknown_client`
- `protocol_mismatch`
- `already_connected`
- `internal_error`

#### Payload byte layout

`AuthResponse` payload follows the same primitive rules as `AuthRequest`: all
numeric fields are little-endian, `string` is `u16 byte_length` plus UTF-8
bytes, and optional fields start with a `u8 present` tag.

The fixed header carries `message_type = AuthResponse`, `protocol_version`, and
`payload_length`. `protocol_version` is not repeated inside the payload.

| Order | Field | Wire type | Notes |
| ---: | --- | --- | --- |
| 1 | `client_id` | `string` | `u16 byte_length` + UTF-8 bytes. Mirrors the request identity. |
| 2 | `run_id` | `string` | `u16 byte_length` + UTF-8 bytes. Mirrors the request run/session. |
| 3 | `accepted` | `bool` / `u8` | `0 = false`, `1 = true`. Any other value is invalid when decode is implemented. |
| 4 | `reason_code` | `u16` | Stable little-endian code: `0 = Ok`, `1 = InvalidToken`, `2 = UnknownClient`, `3 = ProtocolMismatch`, `4 = AlreadyConnected`, `5 = InternalError`. |
| 5 | `message` | `optional<string>` | `u8 present`; when `1`, followed by `u16 byte_length` + UTF-8 bytes. Usually omitted for accepted responses. |
| 6 | `server_time` | `optional<u64>` | `u8 present`; when `1`, followed by server timestamp in microseconds. Useful for diagnostics or future time sync hints. |
| 7 | `expected_protocol_version` | `optional<u32>` | `u8 present`; when `1`, followed by the server expected protocol version. Present mainly for `ProtocolMismatch`. |

Optional field rules:

- `present = 0` means the value is absent and no value bytes follow.
- `present = 1` means the value bytes immediately follow the tag.
- Other `present` values are invalid for future decode.
- `expected_protocol_version` should be present when `reason_code =
  ProtocolMismatch`; otherwise it may be omitted.

Encode boundary:

1. server auth decision code produces a `ServerAuthDecision`.
2. `ServerAuthResponseBoundary` constructs `ProtocolMessage::AuthResponse`.
3. `ServerOutboundQueueBoundary` hands the typed message and destination to
   `net-core::OutboundPacket`.
4. The protocol encoder writes fixed header + this payload layout into bytes.
5. The UDP socket adapter can transmit those encoded bytes over UDP.

`AuthResponse` wire encode is implemented in `crates/protocol` as a minimal
fixed header + payload byte writer. Queue processing, destination management,
and UDP send remain outside `crates/protocol`; continuous send orchestration is
still unimplemented.

---

### 7.3 Heartbeat
client が定期送信する生存確認メッセージ。

#### 目的
- 認証済み状態の維持
- 接続監視
- RTT 計測補助
- 状態確認

#### 必須フィールド
- message_type = `heartbeat`
- protocol_version
- client_id
- run_id
- sent_at

#### 任意フィールド
- local_time
- short_status

#### Payload byte layout

fixed header の `message_type = Heartbeat` の後続 payload は以下の順序とする。

| Order | Field | Wire type | Notes |
| ---: | --- | --- | --- |
| 1 | `client_id` | `string` | `u16 byte_length` + UTF-8 bytes。送信元 address だけに依存しない識別用。 |
| 2 | `run_id` | `string` | `u16 byte_length` + UTF-8 bytes。古い session の heartbeat と区別する。 |
| 3 | `sent_at` | `u64` | client clock domain の microseconds。RTT 計測の基準。 |
| 4 | `local_time` | `optional<u64>` | `u8 present` の後、存在する場合だけ `u64`。初期実装では `sent_at` と同値でもよい。 |
| 5 | `short_status` | `optional<string>` | `u8 present` の後、存在する場合だけ `u16 byte_length` + UTF-8 bytes。通常は省略する。 |

Heartbeat は軽量性を優先する。PoC 初期では `local_time` と `short_status` は送らなくてもよく、その場合は各 `present = 0` だけを置く。
`crates/protocol` の最小実装では、上記 payload を `Heartbeat` 型へ復元し、`local_time` は `Option<TimestampMicros>`、`short_status` は `Option<String>` として扱う。生存確認の更新、timeout 判定、RTT 計算は app / server 側の責務とする。

#### 備考
- server は heartbeat を受けて生存確認を更新する
- 一定時間 heartbeat が来なければ切断扱いにする

---

### 7.4 HeartbeatAck
server が heartbeat に応答するメッセージ。

#### 目的
- RTT 計測補助
- server 側時刻通知
- オフセット推定の材料提供

#### 必須フィールド
- message_type = `heartbeat_ack`
- protocol_version
- client_id
- run_id
- echoed_sent_at
- server_received_at
- server_sent_at

#### Payload byte layout

fixed header の `message_type = HeartbeatAck` の後続 payload は以下の順序とする。
数値フィールドは fixed header と同じく little-endian とし、timestamp は既存方針どおり `u64` microseconds として置く。

| Order | Field | Wire type | Notes |
| ---: | --- | --- | --- |
| 1 | `client_id` | `string` | `u16 byte_length` + UTF-8 bytes。heartbeat の送信元 client を示す。 |
| 2 | `run_id` | `string` | `u16 byte_length` + UTF-8 bytes。heartbeat の session / run と照合する。 |
| 3 | `echoed_sent_at` | `u64` | 受信した `Heartbeat.sent_at` をそのまま返す。client clock domain の microseconds。 |
| 4 | `server_received_at` | `u64` | server が heartbeat を受信した時刻。server clock domain の microseconds。 |
| 5 | `server_sent_at` | `u64` | server が ack を送信層へ渡す時刻。server clock domain の microseconds。 |

`HeartbeatAck` は `Heartbeat` の受信に対して server が返す typed response として扱う。server 側の heartbeat handler / timebase 層が `HeartbeatAck` の各 timestamp 値を決め、response / ack boundary が `ProtocolMessage::HeartbeatAck` と宛先 metadata を net send layer へ渡す。protocol encoder はこの payload layout を fixed header の後ろに書く。

Encode input boundary:

1. server heartbeat handler が `Heartbeat` の受信結果と server-side timestamps を用意する。
2. `ServerHeartbeatAckBoundary` が `ProtocolMessage::HeartbeatAck` を構築する。
3. `ServerOutboundQueueBoundary` が typed message と destination を `net-core::OutboundPacket` / `OutboundQueueItem` として net send layer へ渡す。
4. protocol encoder が `HeartbeatAck` を fixed header + payload bytes に変換する。
5. UDP socket adapter が encode 済み bytes と destination を受け取り、1 datagram を送信できる。

#### 備考
- client はこれを使って RTT の概算を取れる
- server 側でも受信時刻を保持して offset 推定に使える
- 現時点の code は typed handoff、protocol encode、encode 済み datagram の最小 UDP send までで、heartbeat 管理、timeout 管理、継続 send loop は行わない

---

### 7.5 VideoFrame
client が送信する映像フレームメッセージ。

#### 目的
- ゲーム画面の映像データ送信
- 同期に必要な metadata の送信

#### 必須フィールド
- message_type = `video_frame`
- protocol_version
- client_id
- run_id
- frame_id
- capture_timestamp
- send_timestamp
- is_keyframe
- width
- height
- fps_nominal
- codec
- payload_size
- payload

#### 任意フィールド
- encode_duration_ms
- color_format
- profile_name

#### フィールド概要
- `frame_id`
  - フレーム識別子
- `capture_timestamp`
  - client 側でフレームを取得した時刻
- `send_timestamp`
  - client 側で送信した時刻
- `is_keyframe`
  - キーフレームかどうか
- `width`, `height`
  - エンコード時解像度
- `fps_nominal`
  - 設定上の目標 fps
- `codec`
  - 例: `h264`
- `payload_size`
  - バイト数
- `payload`
  - 実フレームデータ

#### Payload byte layout

fixed header の `message_type = VideoFrame` の後続 payload は、先頭に frame metadata を置き、その直後に H.264 encoded bytes を置く。

| Order | Field | Wire type | Notes |
| ---: | --- | --- | --- |
| 1 | `client_id` | `string` | `u16 byte_length` + UTF-8 bytes。認証済み client と照合する。 |
| 2 | `run_id` | `string` | `u16 byte_length` + UTF-8 bytes。session 単位の混線を避ける。 |
| 3 | `frame_id` | `u64` | client 内で単調増加する frame identifier。 |
| 4 | `capture_timestamp` | `u64` | client clock domain の microseconds。同期の主基準。 |
| 5 | `send_timestamp` | `u64` | client clock domain の microseconds。送信遅延の観測用。 |
| 6 | `is_keyframe` | `bool` / `u8` | `0 = false`, `1 = true`。 |
| 7 | `metadata_reserved` | `u8[3]` | 初期値はすべて `0`。後続 `u32` を 4 byte 境界に寄せるための予約領域。 |
| 8 | `width` | `u32` | encoded frame の幅。 |
| 9 | `height` | `u32` | encoded frame の高さ。 |
| 10 | `fps_nominal` | `u32` | 設定上の nominal fps。30fps は `30`。 |
| 11 | `codec` | `u16` | 初期値は `1 = H264`。 |
| 12 | `payload_size` | `u32` | 後続 H.264 bytes の長さ。fixed header の `payload_length` とは別。 |
| 13 | `payload` | `bytes` | `payload_size` byte の H.264 encoded bytes。追加の長さ prefix は置かない。 |

`VideoFrame` の numeric metadata 部分は、`frame_id` から `payload_size` までで 46 byte とする。`client_id` と `run_id` は長さ付き可変長のため、この 46 byte には含めない。

境界:
- `payload_size` の直後の byte から H.264 data が始まる。
- fixed header の `payload_length` は、`client_id` / `run_id` の長さ prefix と bytes、46 byte の numeric metadata、H.264 data の合計 byte 数と一致しなければならない。
- `payload_size` は H.264 data の byte 数だけを表す。`payload_size` が実際の残り byte 数と一致しない場合は payload decode 失敗とする。
- protocol crate は H.264 data の中身を解釈しない。

`crates/protocol` の最小実装では、`is_keyframe` は `0` / `1` のみ、`metadata_reserved` は `[0, 0, 0]` のみ、`codec` は `1 = H264` のみを受理する。`payload_size` と実際の残り byte 数が一致した場合にだけ、H.264 encoded bytes をそのまま `Vec<u8>` として `VideoFrame.payload` に復元する。

#### Encode policy

`VideoFrame` encode は、client 側がすでに持っている H.264 encoded bytes を protocol wire packet に載せる境界とする。protocol crate は映像圧縮、NAL unit 解釈、fragmentation、retry、暗号化を行わない。

metadata は payload byte layout と同じ順序で書く。

1. `client_id` (`u16 byte_length` + UTF-8 bytes)
2. `run_id` (`u16 byte_length` + UTF-8 bytes)
3. `frame_id` (`u64` little-endian)
4. `capture_timestamp` (`u64` microseconds, little-endian)
5. `send_timestamp` (`u64` microseconds, little-endian)
6. `is_keyframe` (`u8`, `0 = false`, `1 = true`)
7. `metadata_reserved` (`u8[3]`, 現時点では `[0, 0, 0]`)
8. `width` (`u32` little-endian)
9. `height` (`u32` little-endian)
10. `fps_nominal` (`u32` little-endian)
11. `codec` (`u16` little-endian, `1 = H264`)
12. `payload_size` (`u32` little-endian)
13. `payload` (`payload_size` byte の H.264 encoded bytes)

`payload_size` は `VideoFrame.payload.len()`、つまり後続 H.264 bytes の実 byte 数から決める。`VideoFrame.payload_size` と実 payload 長が一致しない場合は encode error とし、wire bytes を生成しない。H.264 bytes 自体には追加の length prefix を置かず、`payload_size` の直後にそのまま連結する。

fixed header + payload bytes の組み立ては以下とする。

1. `ProtocolMessage::VideoFrame` を `ProtocolMessageEncoderBoundary` へ渡す。
2. encoder は上記順序で payload bytes を作る。
3. encoder は fixed header に `message_type = VideoFrame`、`protocol_version = EncodeContext.protocol_version`、`payload_length = payload bytes の合計長` を書く。
4. output buffer は `16 byte fixed header` + `payload bytes` の 1 packet buffer になる。
5. net send layer は encode 済み bytes と宛先 metadata を保持し、socket send layer へ渡す。

この最小実装では 1 `VideoFrame` message を 1 UDP datagram 用 buffer として扱う。MTU 超過時の分割、再送制御、暗号化、映像エンコード本体は別タスクに残す。

#### 備考
- payload は最も大きいデータとなる
- 必要に応じて fragmentation を後で導入する余地がある
- MVP ではまず「1メッセージ1フレーム」を基本として考える
- MTU を超える場合の扱いは実装段階で要検討

---

### 7.6 ClientStats
client が定期送信する状態メッセージ。

#### 目的
- 送信側状態の共有
- トラブルシュート支援
- UI 表示補助

#### 必須フィールド
- message_type = `client_stats`
- protocol_version
- client_id
- run_id
- sent_at

#### 任意フィールド
- capture_fps
- encode_fps
- send_fps
- dropped_frames
- encoder_name
- bitrate_kbps
- cpu_usage
- gpu_usage
- queue_depth

#### 備考
- MVP では必須項目を絞ってよい
- 最初は capture_fps / dropped_frames / bitrate_kbps 程度から始めてもよい

---

### 7.7 ServerNotice
server が client または switcher に送る通知系メッセージ。

#### 目的
- 切断通知
- protocol mismatch 通知
- warning 通知
- 状態変更通知

#### 必須フィールド
- message_type = `server_notice`
- protocol_version
- run_id
- notice_type
- message

#### notice_type 例
- `warning`
- `disconnect`
- `protocol_error`
- `auth_expired`
- `server_shutdown`

---

## 8. 認証シーケンス

### 8.1 基本フロー
1. client 起動
2. client が `AuthRequest` を送信
3. server が token / client_id / protocol_version を検証
4. server が `AuthResponse` を返す
5. accepted = true の場合、client は映像送信と heartbeat を開始
6. accepted = false の場合、client は再試行または停止

### 8.2 server 側ルール
- 未認証送信元の `VideoFrame` は破棄する
- 認証済み送信元だけを受理対象にする
- protocol_version 不一致は拒否する
- heartbeat timeout で認証済み状態を解除する

### 8.3 client AuthRequest one-shot PoC

client 側の auth request PoC は、server 側 one-shot auth response PoC に最小接続するための起動入口とする。継続 loop、heartbeat、video frame 送信、retry、fragmentation、encryption、async runtime は含めない。

Flow:

1. `apps/client` が client TOML を読む。
2. `[client].server_host` / `[client].server_port` を `SocketAddr` に解決する。
3. `[client].client_id`, `[client].shared_token`, optional `[client].display_name`, `[session].run_id`, `[session].app_version`, `[session].protocol_version` から `ProtocolMessage::AuthRequest` を構築する。
4. `ProtocolMessageEncoderBoundary` が `AuthRequest` を fixed header + payload bytes に変換する。
5. client PoC launcher が ephemeral local UDP socket を bind し、encoded bytes を destination へ `send_to` で 1 回送信する。

Responsibility split:

- client config loading
  - TOML から one-shot AuthRequest に必要な destination と auth fields を取り出す。
  - token の secret store 解決や安全な保管はまだ行わない。
- client PoC launcher
  - config、destination 解決、message 構築、encode、1 回の UDP send を接続する。
  - 継続 loop、reconnect、heartbeat、video frame、ログ writer は持たない。
- protocol encoder
  - `AuthRequest` を docs の payload layout どおりに fixed header + payload bytes へ変換する。
  - destination、socket、retry、認証判定は扱わない。
- socket send
  - encode 済み bytes と destination だけを受け、1 datagram を送る。

Current implementation: `apps/client::ClientAuthRequestPocLauncher` and
`run_auth_request_poc_once_from_path`. The client binary exposes this path with
`--auth-request-poc-once [config-path]`.

---

## 9. 同期に必要な時刻情報

### 9.1 timestamp 単位
同期に使う protocol timestamp は、すべてマイクロ秒単位で扱う。

理由:
- 30fps / 60fps のフレーム間隔より十分細かく、丸め誤差を抑えやすい
- RTT / clock offset 推定に使いやすい
- `u64` ベースなら PoC / MVP でも扱いが単純で、将来の長時間運用でも十分な範囲を持てる

Rust 実装では `TimestampMicros` を使い、値の単位を型名で明示する。

### 9.2 client 側で送るべき時刻
- capture_timestamp
- send_timestamp

### 9.3 server 側で保持すべき時刻
- packet_received_at
- corrected_capture_time
- targetTime

### 9.4 方針
- client ごとの clock offset を推定する
- capture_timestamp を補正して共通時間軸へ変換する
- その上で targetTime に最も近いフレームを選ぶ

---

## 10. 受信後の server 内部処理

`VideoFrame` を受けた server はおおむね以下を行う。

1. 認証済み送信元か確認
2. protocol_version を確認
3. frame metadata を読み取る
4. packet_received_at を記録
5. capture_timestamp を共通時間軸へ補正
6. client ごとのバッファへ格納
7. targetTime に応じたフレーム選択候補にする

---

## 11. エラー処理方針

### 11.1 破棄するケース
- 未認証送信元
- protocol_version 不一致
- 必須フィールド不足
- payload_size 不正
- decode 不可能なフレーム
- 極端に古いフレーム

### 11.2 warn ログにするケース
- app_version 差異
- 一時的な heartbeat 遅延
- 軽微なフレーム欠損
- 短時間の jitter 増加

### 11.3 error ログにするケース
- 認証失敗
- デコード失敗の継続発生
- バッファ異常
- 同期不能状態の継続
- server 側内部処理異常

---

## 12. MVP でまだ固定しないもの

以下は MVP の初期段階では詳細固定しない。

- 完全なバイナリレイアウト
- パケット分割方式
- 大きなフレームの fragmentation 仕様
- 再送要求仕様
- 暗号化仕様
- 圧縮済み payload の細かな profile
- 複数 protocol_version 同時サポート
- client 間の直接通信

---

## 13. 今後の設計で詰める項目

- バイナリ形式の確定
- payload fragmentation の要否
- frame header の厳密サイズ
- sequence_number のスコープ
- heartbeat 間隔
- timeout 間隔
- stats 送信間隔
- keyframe 再要求の要否
- switcher への受け渡し形式
- server と switcher 間通信の具体方式

---

## 14. 初期実装の優先順位

### 優先度高
- AuthRequest
- AuthResponse
- Heartbeat
- HeartbeatAck
- VideoFrame の最小構造
- protocol_version チェック

### 優先度中
- ClientStats
- ServerNotice
- sequence_number の詳細運用

### 優先度低
- 再送制御
- fragmentation 最適化
- 詳細な notice 種別拡張
---

## Server Auth Handler Boundary

`protocol` stops at decoding `AuthRequest` from bytes. Authentication itself is
owned by the server side.

Receive path:

1. `net-core` receives raw packet bytes and source metadata.
2. `protocol` decodes the fixed header.
3. `protocol` checks the app-provided expected `protocol_version` after fixed
   header decode and before payload decode.
4. `protocol` dispatches by `message_type`.
5. For `AuthRequest`, `protocol` decodes the payload into `AuthRequest`.
6. `net-core` wraps the decoded message and source metadata as
   `DecodedInboundPacket`.
7. `ServerInboundRouter` recognizes `AuthRequest` and hands it to the auth
   handler boundary.

Responsibility split:

- `protocol`
  - Fixed header decode, version check helper, message dispatch, and payload
    decode.
  - No token checking, whitelist lookup, server state updates, or response
    generation.
- `net-core`
  - Packet/source carrier and protocol decode caller.
  - No auth business logic.
- `ServerInboundRouter`
  - Maps decoded `ProtocolMessage::AuthRequest` to the server auth route.
  - No auth decision logic.
- auth handler boundary
  - Receives decoded `AuthRequest`.
  - Prepares the decision input from `shared_token`, `client_id`,
    `protocol_version`, `app_version`, `run_id`, and optional `display_name`.
  - Does not apply success/failure to server state and does not send
    `AuthResponse`.
- server state / response layer
  - Future owner of authenticated-client state updates and outbound
    `AuthResponse` generation/sending.

Current placeholder: `apps/server::ServerAuthHandlerBoundary` accepts
`ServerInboundRoute::AuthRequest` and produces `ServerAuthCheck`. This is only a
handoff shape; real token verification, whitelist loading, success/failure
decisions, and response sending remain out of scope.

### Auth Configuration Input Boundary

`AuthRequest` decode and auth configuration loading are separate inputs to the
server auth decision layer. `crates/protocol` only restores the presented
request fields from bytes. `crates/config` owns minimal TOML loading for server
auth settings: allowed client entries and shared token references.

Flow:

1. `ServerAuthConfigBoundary` reads server TOML and produces
   `ServerAuthConfig`.
2. `[auth.clients.<client_id>]` table names become whitelisted `client_id`
   values.
3. Each table's `shared_token` becomes a `SharedTokenConfig` using
   `SharedTokenSecretRef::InlinePlaceholder`.
4. `ServerAuthConfig` carries the client whitelist and token reference list.
5. `ServerAuthHandlerBoundary` prepares `ServerAuthCheck` from decode済み
   `AuthRequest`.
6. `ServerAuthConfigInputBoundary` combines `ServerAuthCheck` and
   `ServerAuthConfig` into `ServerAuthCheckInput`.
7. `ServerAuthDecisionBoundary` consumes `ServerAuthCheckInput` to perform the
   minimal whitelist lookup and inline placeholder token comparison.

Responsibility split:

- `config`
  - Defines and loads the minimal whitelist/token configuration from TOML.
  - Does not resolve environment variables, secret stores, or other external
    secret references.
  - Does not inspect packets or decide authentication.
- server auth handler
  - Owns the handoff from decode済み `AuthRequest` to auth input.
  - Does not load TOML or compare tokens.
- auth check input
  - Carries request fields, source metadata, allowed client entries, and token
    references together.
  - Does not produce accepted/rejected output.
- auth decision
  - Owns the current minimal whitelist match, inline placeholder token
    comparison, and decision result.
  - Future owner of fuller protocol/app version policy and external secret
    verification.

Current implementation: `stream-sync-config::ServerAuthConfigBoundary` reads the
auth portion of `configs/examples/server.example.toml`-compatible TOML and
produces `ServerAuthConfig`. `apps/server::ServerAuthConfigInputBoundary`
converts config values into server-side auth check input without performing the
actual decision.

### Secret Resolution and Token Protection

`shared_token` is kept as a PoC-only inline placeholder so the one-shot auth
round trip can be run from repository example configs. For future operation,
server config may use `shared_token_env` to point at an environment variable
name. This stage defines the reference shape and token handling policy only; it
does not read environment variables or secret stores.

Config rules:

1. Each `[auth.clients.<client_id>]` must provide exactly one token reference.
2. `shared_token = "..."` is parsed as
   `SharedTokenSecretRef::InlinePlaceholder`.
3. `shared_token_env = "ENV_NAME"` is parsed as
   `SharedTokenSecretRef::EnvironmentVariable`.
4. Empty or conflicting token references are config errors.

Boundary flow:

1. `ServerAuthConfigBoundary` parses the token reference.
2. `ServerAuthConfigInputBoundary` copies the reference into
   `ServerAuthCheckInput`.
3. A future secret resolver will turn external references into token material.
4. `ServerAuthDecisionBoundary` compares only prepared token material. In the
   current PoC, inline placeholders are the only comparable material.
5. Unresolved external references reject with `InternalError` until the resolver
   is implemented.

Protection rules:

- Raw token material must not appear in auth logs, receive rejection logs,
  `AuthResponse.message`, operator stdout, or debug dumps.
- Inline token debug output is redacted by `SharedTokenSecretRef`.
- Environment variable names may appear as references; resolved values must not.
- `config` owns reference parsing, auth input owns context assembly, secret
  resolution owns external lookup, and auth decision owns comparison.

First real resolver scope:

- Input: `ServerSharedTokenAuthInput` / token id plus `SharedTokenSecretRef`.
- Output: resolved token material suitable for auth decision input, with debug
  output redacted.
- Supported source: environment variables named by `shared_token_env`.
- PoC compatibility: inline placeholder material stays supported as already
  resolved token material.
- Errors: missing environment variable, empty environment variable, unsupported
  reference type, and internal resolver error. Error messages must not include
  token values.
- Not included: secret store integrations, network calls, caching, hot reload,
  rotation, hashing/KDF, auth decision, response generation, logging, or socket
  I/O.

Current implementation: `ServerSecretResolverBoundary::resolve_auth_input`
turns `ServerAuthCheckInput` into resolved auth decision input. Inline tokens
remain supported as already-available PoC material, and `shared_token_env`
reads exactly the named environment variable. Missing, empty, and invalid env
values return typed `ServerSecretResolutionError` variants without carrying
token values. `plan_resolution` remains available as a non-reading planning
helper for docs/tests.

### Minimal Auth Decision Boundary

The server auth decision boundary converts prepared auth input into
`ServerAuthDecision`. This is the first minimal decision implementation; it is
not full production auth.

Decision rules:

1. Find `requested_client_id` in the prepared allowed client list.
2. If missing, reject with `AuthResponseReasonCode::UnknownClient`.
3. Find the allowed client's `shared_token_id` in the prepared token list.
4. If missing, reject with `AuthResponseReasonCode::InternalError`.
5. If secret resolution failed before the decision, the auth flow returns an
   `InternalError` decision with a non-secret failure message.
6. If token material is resolved from inline PoC config or `shared_token_env`,
   compare it with the presented `shared_token`.
7. Matching token accepts with `AuthResponseReasonCode::Ok`; mismatch rejects
   with `AuthResponseReasonCode::InvalidToken`.

Responsibility split:

- auth decision
  - Produces accepted/rejected `ServerAuthDecision`.
  - Uses only resolved auth input.
  - Preserves auth context such as source, `client_id`, `run_id`, optional
    `app_version`, `protocol_version`, and reason code for downstream
    handoffs.
  - Does not read TOML, resolve external secrets, mutate authenticated-source
    state, build `AuthResponse`, enqueue packets, write logs, or send UDP.
- response boundary
  - Receives `ServerAuthDecision` and builds `ProtocolMessage::AuthResponse`.
  - Does not perform auth checks.

Current implementation: `apps/server::ServerAuthDecisionBoundary`.

### Auth Success / Failure Log Handoff Boundary

Auth success / failure logging is a server-side handoff boundary, not protocol
wire format. The server keeps the auth result typed until a future log layer
formats JSON Lines events.

Flow:

1. `ServerAuthDecisionBoundary` returns `ServerAuthDecision`.
2. `ServerAuthLogHandoffBoundary` receives the decision by reference.
3. The boundary produces `ServerAuthLogInput`.
4. `ServerAuthLogInput` keeps source endpoint, `client_id`, `run_id`, optional
   `app_version`, `protocol_version`, success / failure outcome,
   `AuthResponseReasonCode`, optional message, server time, and expected
   protocol version.
5. The future log layer will consume this typed input and decide the JSON Lines
   event shape.

Responsibility split:

- auth decision
  - Decides accepted / rejected and reason code.
  - Does not emit logs.
- auth log handoff
  - Converts `ServerAuthDecision` into typed log input while preserving context.
  - Does not serialize JSON, write files, update metrics, mutate registry
    state, or send UDP.
- log layer
  - Future owner of JSON Lines formatting and output.

Current implementation: `apps/server::ServerAuthLogHandoffBoundary`,
`ServerAuthLogInput`, and `ServerAuthLogOutcome`.

### Auth JSON Lines Event Schema Boundary

Auth success / failure logs are JSON Lines application events, not protocol
wire messages. The server keeps the result typed until the log layer writes a
record.

Flow:

1. `ServerAuthLogHandoffBoundary` produces `ServerAuthLogInput`.
2. `ServerAuthJsonLogEventBoundary` receives that typed input plus an explicit
   log timestamp.
3. The boundary produces `ServerAuthJsonLogEventInput`.
4. A future JSON Lines writer serializes the event input. The current boundary
   does not write JSON Lines.

Auth result event schema:

| Field | Type | Policy |
| --- | --- | --- |
| `event_name` | string | Always `server.auth_result`. |
| `run_id` | `RunId` | Common success / failure field. |
| `client_id` | `ClientId` | Common success / failure field. |
| `source` | endpoint | Common source endpoint field from `PacketSource`. |
| `accepted` | bool | Common field; true for success and false for failure. |
| `reason_code` | `AuthResponseReasonCode` | Common field; `Ok` for success, rejection reason for failure. |
| `message` | optional string | Failure detail; normally omitted for success. |
| `app_version` | optional `AppVersion` | Common context from decoded `AuthRequest` when present. |
| `protocol_version` | `ProtocolVersion` | Common context from decoded `AuthRequest`. |
| `timestamp` | `TimestampMicros` | Log event timestamp supplied by the caller / future log layer. |
| `expected_protocol_version` | optional `ProtocolVersion` | Failure-only detail for protocol mismatch style rejections. |

Responsibility split:

- auth flow / auth decision
  - Decide accepted / rejected and preserve context.
  - Do not write logs.
- auth log handoff
  - Converts `ServerAuthDecision` into `ServerAuthLogInput`.
  - Does not define JSON serialization behavior.
- JSON Lines event schema boundary
  - Converts auth log handoff input into event-schema input.
  - Preserves `run_id`, `client_id`, source, accepted flag, reason, message,
    versions, and timestamp.
  - Does not choose sinks, retention, or metrics behavior.
- log writer
  - Current minimal writer serializes this one auth event shape to an
    `io::Write` sink.
  - Future owner of file sinks, rotation, buffering, and a broader JSON Lines
    framework.

Current implementation: `apps/server::ServerAuthJsonLogEventBoundary` and
`ServerAuthJsonLogEventInput`. `ServerAuthLogOutputBoundary` connects that
event schema to `ServerAuthJsonLineWriter`. The one-shot server CLI emits auth
result JSON Lines to stderr after the auth response PoC step returns an auth
decision. A future continuous loop should use the same boundary at the auth
decision point, but file rotation, configured sinks, async logging, and metrics
updates remain future work.

Example shape:

```json
{"event_name":"server.auth_result","run_id":"run-1","client_id":"client-1","source":"127.0.0.1:5000","accepted":false,"reason_code":"InvalidToken","message":"invalid shared_token","app_version":"0.1.0","protocol_version":1,"timestamp":2000300,"expected_protocol_version":2}
```

### JSON Lines Writer Connection Scope

Auth result and receive rejection logs use parallel connection boundaries:

| Log family | Handoff input | Event schema input | Writer boundary | Current default sink |
| --- | --- | --- | --- | --- |
| Auth result | `ServerAuthLogInput` | `ServerAuthJsonLogEventInput` | `ServerAuthLogOutputBoundary` | one-shot server stderr |
| Receive rejection | `ServerPacketLogInput` | `ServerReceiveRejectionJsonLogEventInput` | `ServerReceiveRejectionLogOutputBoundary` | one-shot server stderr |

Both writers are schema-specific and synchronous over caller-owned
`io::Write`. They do not define process-wide logger setup, file paths, rotation,
retention, async logging, metrics fanout, or a generic logging crate API.
The future receive loop should call the auth writer at the same logical point:
after auth decision creation and before/around response handoff, without
changing the JSON Lines schema.

### Connected Server Auth Flow Step

The server auth flow step connects decoded `AuthRequest` handling to outbound
`AuthResponse` queue handoff and auth log handoff. It composes existing
boundaries and does not add network I/O or log output.

Flow:

1. `ServerInboundRoute::AuthRequest` carries decoded `AuthRequest` plus source
   metadata.
2. `ServerAuthFlowStep` converts the route into `ServerAuthCheck` through
   `ServerAuthHandlerBoundary`.
3. `ServerAuthFlowStep` combines the check with `ServerAuthConfig` through
   `ServerAuthConfigInputBoundary`.
4. `ServerAuthFlowStep` runs `ServerAuthDecisionBoundary`.
5. `ServerAuthFlowStep` passes the decision to
   `ServerAuthLogHandoffBoundary`.
6. `ServerAuthFlowStep` passes the decision to `ServerAuthResponseBoundary`.
7. `ServerAuthFlowStep` hands the resulting `ServerOutboundAuthResponse` to
   `ServerOutboundQueueBoundary`.
8. The output is `ServerAuthFlowOutcome`, containing the `ServerAuthDecision`,
   auth log input, typed outbound response, and `OutboundQueueItem`.

Responsibility split:

- server auth flow step
  - Owns orchestration from decoded auth route to log and queue handoff.
  - Does not read TOML, resolve secrets, register authenticated sources, encode
    bytes, run a queue, write logs, or send UDP.
- auth log handoff boundary
  - Receives `ServerAuthDecision` and produces `ServerAuthLogInput`.
  - Does not perform JSON Lines output.
- outbound queue boundary
  - Receives typed `AuthResponse` and destination metadata.
  - Produces an `OutboundQueueItem` handoff only.
- net send layer
  - Future owner of encoding the queued `ProtocolMessage::AuthResponse`.

Current implementation: `apps/server::ServerAuthFlowStep` and
`ServerAuthFlowOutcome`.

---

### AuthResponse PoC Socket Startup Boundary

The auth response PoC startup path connects the existing receive and send
boundaries for one packet. It proves the minimal round trip without introducing
a continuous loop or async runtime.

Flow:

1. A bound synchronous UDP socket receives one datagram.
2. The received bytes and source endpoint pass through receive loop -> decode
   -> packet acceptance gate.
3. An accepted `AuthRequest` route is passed to `ServerAuthFlowStep`.
4. The auth flow produces `ServerAuthDecision`, auth log handoff input,
   optional authenticated-sender registration, typed `AuthResponse`, and
   `OutboundQueueItem`.
5. The outbound queue handoff item is passed to `OutboundPacketEncoderBoundary`.
6. `ProtocolMessageEncoderBoundary` encodes `ProtocolMessage::AuthResponse`
   into fixed header + payload bytes.
7. The UDP socket adapter sends the encoded datagram to the request source.

The boundary keeps responsibilities unchanged:

- `protocol` owns wire encode / decode only.
- `net-core` owns inbound packet decode, outbound encode handoff, and encoded
  packet shape.
- `apps/server` owns the one-shot PoC composition and auth decision flow.
- socket I/O owns one `recv_from` and one `send_to`.

Current implementation: `apps/server::ServerAuthResponsePocStep`. It does not
run heartbeat / video frame handlers, JSON Lines output, retry, fragmentation,
encryption, or a long-running receive loop.

### AuthResponse PoC Startup Config Entry

The one-shot auth response PoC can now be launched from server configuration.
This startup entry is an app-side adapter around existing protocol and net
boundaries.

Flow:

1. `apps/server` reads a server TOML file.
2. The launcher extracts `bind_host`, `bind_port`, and `protocol_version`.
3. `ServerAuthConfigBoundary` loads allowed clients and shared token placeholder
   values from the same TOML content.
4. The launcher resolves and binds the UDP socket.
5. The launcher creates an empty `AuthenticatedSenderRegistry`.
6. The launcher calls `ServerAuthResponsePocStep::run_one`.
7. The protocol encoder still only receives typed `ProtocolMessage` plus
   `EncodeContext`; it does not read config or bind sockets.

Current implementation: `apps/server::ServerAuthResponsePocLauncher` and
`run_auth_response_poc_once_from_path`. The binary exposes this path with
`--auth-response-poc-once [config-path]`. It remains one-shot and does not
introduce a continuous receive loop or async runtime.

---

### Authenticated Sender Registry Boundary

After an accepted auth decision, the server needs a separate boundary for
remembering which source endpoint is allowed to send later client-scoped
packets. This registry is server state, not protocol wire format.

Flow:

1. `ServerAuthDecisionBoundary` returns `ServerAuthDecision`.
2. `ServerAuthFlowStep` passes accepted decisions to
   `AuthenticatedSenderRegistryBoundary`.
3. The registry handoff stores `client_id`, source endpoint, `run_id`, and
   `protocol_version`.
4. The one-shot auth response PoC step applies that handoff to the in-memory
   `AuthenticatedSenderRegistry` when the auth decision is accepted.
5. Later receive paths for `Heartbeat` and `VideoFrame` use decoded
   `client_id` plus packet source endpoint to query the registry.
6. A missing `client_id` binding or endpoint mismatch is a reject/drop
   candidate for that later packet.
7. Timeout, expiration, revocation, and reauthentication remain future design
   work and are not executed by the current boundary.

Responsibility split:

- protocol
  - Decodes `client_id` and other payload fields.
  - Does not know whether the sender endpoint is authenticated.
- receive loop / server routing
  - Preserves packet source metadata and decoded message fields.
  - Calls the packet acceptance gate before accepting heartbeat or video frame
    work.
- server auth flow
  - Produces accepted/rejected auth decisions and the accepted registration
    handoff.
  - Does not mutate registry state, persist state, or enforce timeout.
- authenticated sender registry
  - Owns the `client_id` to endpoint binding lookup.
  - Does not verify tokens, decode packets, build `AuthResponse`, run UDP
    sockets, or implement reauthentication.

Current implementation: `apps/server::AuthenticatedSenderRegistryBoundary`
creates registrations from accepted decisions, `ServerAuthResponsePocStep`
registers accepted senders into an in-memory `AuthenticatedSenderRegistry`, and
the packet acceptance gate checks later `client_id` / source endpoint pairs
against that registry.

---

### Packet Acceptance / Rejection Boundary

The packet acceptance gate sits after decode and routing, but before
client-scoped handlers. It is the early decision point for unauthenticated or
endpoint-mismatched packets.

Flow:

1. `protocol` decodes client-scoped payload fields such as `client_id`.
2. `net-core` preserves the source endpoint as `PacketSource`.
3. `ServerInboundRouter` returns a decoded route.
4. `AuthRequest` routes bypass the registry check so that initial auth can run.
5. After auth success registers a sender, later `Heartbeat` and `VideoFrame`
   routes are checked against `AuthenticatedSenderRegistry`.
6. If the source endpoint is not registered for any accepted client, the gate
   returns `UnauthenticatedSource`.
7. If the endpoint is known but the decoded `client_id` is not registered, the
   gate returns `UnknownClient`.
8. If the `client_id` is registered but the packet source endpoint differs, the
   gate returns `EndpointMismatch`.
9. Accepted routes may proceed to future heartbeat / video frame handlers.

Responsibility split:

- protocol
  - Decodes `client_id`; does not decide authentication state.
- receive loop
  - Preserves source endpoint and will call the gate early in the receive path.
  - Future owner of turning rejections into real packet drops.
- authenticated sender registry
  - Stores and looks up accepted `client_id` to endpoint bindings.
  - Does not emit logs or mutate packet flow.
- packet acceptance gate
  - Produces accept / reject decisions from decoded route plus registry state.
  - Does not execute drop, log output, timeout, or reauthentication.
- handler
  - Runs only after acceptance in the future flow.
  - Owns heartbeat and video frame processing, not source authentication.

Current implementation: `apps/server::PacketAcceptanceGateBoundary` evaluates
`ServerInboundRoute` values against `AuthenticatedSenderRegistry` and returns
`PacketAcceptanceDecision`. `apps/server::ServerReceiveLoopStep` now calls this
gate after decode and route classification through
`handle_received_packet_with_gate`. Accepted routes are returned as the only
handler candidates, while rejected routes are returned as decisions for future
drop / log handling. UDP socket integration is limited to the one-datagram
adapter; actual packet discard and logging are still out of scope.

---

### Registered Packet Handler Handoff Boundary

Accepted heartbeat and video frame routes are still not enough for handler
execution because handlers also need the authenticated sender binding that
proved the packet is allowed. The registered packet handoff boundary attaches
that binding to decoded handler input.

Flow:

1. Receive loop returns an accepted `ServerInboundRoute` after decode, routing,
   and packet acceptance gate evaluation.
2. `ServerRegisteredPacketBoundary` receives the accepted route plus
   `AuthenticatedSenderRegistry`.
3. `Heartbeat` routes become `ServerRegisteredHeartbeatPacket` with source,
   `AuthenticatedSenderEntry`, and decoded `Heartbeat`.
4. `VideoFrame` routes become `ServerRegisteredVideoFramePacket` with source,
   `AuthenticatedSenderEntry`, and decoded `VideoFrame`.
5. The boundary rejects `AuthRequest` and unsupported routes as
   `NotClientScoped`.
6. If a caller passes an unregistered or endpoint-mismatched client packet, the
   boundary preserves the typed packet acceptance rejection.

Responsibility split:

- receive loop / gate
  - Decide whether the packet may reach a handler.
  - Do not run handler logic.
- authenticated sender registry
  - Supplies the registered sender entry.
  - Does not own heartbeat or video semantics.
- registered packet boundary
  - Converts accepted client-scoped routes into handler inputs.
  - Does not calculate RTT, build `HeartbeatAck`, buffer frames, drop late
    frames, write logs, or manage timeout.
- heartbeat handler
  - Future owner of heartbeat state updates and ack input creation.
- video frame handler
  - Future owner of frame acceptance beyond source auth and sync buffer handoff.

Current implementation: `apps/server::ServerRegisteredPacketBoundary`,
`ServerRegisteredClientPacket`, `ServerRegisteredHeartbeatPacket`, and
`ServerRegisteredVideoFramePacket`. These are typed bridge values only; handler
business logic remains unimplemented.

---

### Heartbeat Handler Ack Handoff Boundary

The first heartbeat handler connection turns a registered heartbeat packet into
a typed `HeartbeatAck` outbound queue item. It intentionally stops before
heartbeat state management or RTT / offset calculation.

Flow:

1. `ServerRegisteredPacketBoundary` produces `ServerRegisteredHeartbeatPacket`.
2. The heartbeat handler boundary receives the registered packet and explicit
   `ServerHeartbeatAckTiming`.
3. It maps `Heartbeat.sent_at` to `HeartbeatAck.echoed_sent_at`.
4. It maps supplied timing to `server_received_at` and `server_sent_at`.
5. `ServerHeartbeatAckBoundary` builds `ProtocolMessage::HeartbeatAck`.
6. `ServerOutboundQueueBoundary` turns the typed ack into `OutboundQueueItem`.

Responsibility split:

- registered packet boundary
  - Supplies source-authenticated heartbeat input.
- heartbeat handler boundary
  - Builds ack handoff input from registered heartbeat and explicit timing.
  - Does not read clocks, mutate heartbeat state, calculate RTT / offset, or
    decide timeout.
- ack / queue boundaries
  - Preserve typed outbound handoff shape for the send layer.
  - Do not encode bytes or send UDP.

Current implementation: `apps/server::ServerHeartbeatHandlerBoundary`,
`ServerHeartbeatAckTiming`, and `ServerHeartbeatAckHandoff`. This is a minimal
bridge to the existing `ServerHeartbeatAckBoundary` and outbound queue handoff;
continuous loop integration and heartbeat processing remain future work.

---

### Receive Rejection Drop / Log Handoff Boundary

Receive-side rejections remain typed when they leave the receive loop / gate.
The handoff boundary is the point where a rejection decision becomes input for
future packet drop and receive log layers.

Flow:

1. Decode failure produces `ServerReceiveLoopGateRejection::Decode`.
2. Packet acceptance failure produces
   `ServerReceiveLoopGateRejection::Acceptance`.
3. `ServerRejectionDropLogHandoffBoundary` converts the rejection into
   `ServerRejectionDropLogInput`.
4. The `drop_input` side carries source endpoint plus the exact rejection
   reason for the future drop layer.
5. The `log_input` side carries the same source endpoint and reason for the
   future log layer.
6. `UnauthenticatedSource`, `UnknownClient`, `EndpointMismatch`, and decode
   error rejections are preserved as separate typed cases.

Responsibility split:

- receive loop
  - Calls decode, route, and gate.
  - Produces rejection decisions.
  - Does not drop packets or write logs.
- packet acceptance gate
  - Owns registry-based rejection reason selection.
  - Does not know log schema or drop execution.
- drop / log handoff boundary
  - Converts rejection decisions into typed drop and log inputs.
  - Does not execute packet discard, JSON Lines logging, metrics updates, or
    handler behavior.
- drop layer
  - Future owner of actual discard.
- log layer
  - Future owner of receive rejection JSON Lines events.

Current implementation: `apps/server::ServerRejectionDropLogHandoffBoundary`
preserves decode errors as `ServerRejectionHandoffReason::Decode` and packet
acceptance failures as `ServerRejectionHandoffReason::Acceptance`. The
acceptance variant keeps `message_type`, optional `client_id`, and
`PacketAcceptanceRejectReason` unchanged. Actual packet discard, receive log
output, heartbeat handling, and video frame handling are still out of scope;
UDP socket integration is limited to the one-datagram adapter.

---

### Receive Rejection JSON Lines Event Schema Boundary

Receive rejection logging uses a typed event schema before any JSON Lines
writer exists. `ServerPacketLogInput` is converted into an event input that a
future writer can serialize without reinterpreting gate reasons.

Flow:

1. Receive loop / gate yields a rejection decision.
2. Rejection handoff converts it to `ServerPacketLogInput`.
3. `ServerReceiveRejectionJsonLogEventBoundary` builds
   `ServerReceiveRejectionJsonLogEventInput`.
4. A future writer serializes the event input as JSON Lines.

Event schema:

| Field | Type | Notes |
| --- | --- | --- |
| `event_name` | string | Always `server.receive_rejection`. |
| `run_id` | optional `RunId` | Reserved for correlation. Current decode / gate handoff may not know it, so it can be absent. |
| `client_id` | optional `ClientId` | Preserved when the gate or decoded packet knows it. |
| `source` | `PacketSource` | Source endpoint of the rejected packet. |
| `message_type` | optional `MessageType` | Present for acceptance rejections; absent for decode failures without a decoded type. |
| `rejection_reason` | enum | `DecodeError`, `UnauthenticatedSource`, `UnknownClient`, or `EndpointMismatch`. |
| `detail` | enum/object | Decode detail keeps `ServerDecodeErrorAction` and `ProtocolError`; acceptance detail keeps `PacketAcceptanceRejectReason`. |
| `timestamp` | `TimestampMicros` | Server-side event timestamp supplied by the caller. |

Responsibility split:

- receive loop
  - Calls decode and gate, then produces accepted route or rejected decision.
  - Does not write logs.
- packet acceptance gate
  - Selects rejection reasons from registry lookup.
  - Does not know JSON Lines schema.
- rejection handoff
  - Preserves source, optional client id, message type, and rejection detail.
  - Does not serialize events.
- JSON Lines event schema boundary
  - Maps handoff reasons into `server.receive_rejection` event fields.
  - Does not choose sinks or retention policy.
- log writer
  - Current minimal writer serializes this one event shape to an `io::Write`
    sink.
  - Future owner of file sinks, rotation, buffering, and a broader JSON Lines
    framework.

Current implementation: `apps/server::ServerReceiveRejectionJsonLogEventBoundary`
builds `ServerReceiveRejectionJsonLogEventInput` from `ServerPacketLogInput`.
It preserves `UnauthenticatedSource`, `UnknownClient`, `EndpointMismatch`, and
decode-error detail. `ServerReceiveRejectionLogOutputBoundary` connects the
handoff, event schema, and `ServerReceiveRejectionJsonLineWriter` to write one
JSON Lines record. The server one-shot auth response PoC uses it only when a
receive rejection reaches `ServerAuthResponsePocError::Rejected`, writing to
stderr before the existing error message.

Example shape:

```json
{"event_name":"server.receive_rejection","run_id":null,"client_id":"client-1","source":"127.0.0.1:5000","message_type":"Heartbeat","rejection_reason":"UnauthenticatedSource","detail":{"kind":"Acceptance","reason":"UnauthenticatedSource"},"timestamp":345678}
```

---

## AuthResponse Generation / Send Boundary

`AuthResponse` is generated from a server-side auth decision, not directly from
packet decode. The response generation boundary creates the typed protocol
message, but does not encode or send it.

Flow:

1. `protocol` decodes `AuthRequest` and `net-core` passes it to server routing
   as `DecodedInboundPacket`.
2. `ServerInboundRouter` routes the decoded `AuthRequest` to the auth handler
   boundary.
3. The auth flow step prepares auth input from decoded `AuthRequest` and
   config input.
4. The auth decision boundary evaluates the prepared `client_id` and
   `shared_token` input and returns an auth decision.
5. The server response boundary receives the auth decision as
   `ServerAuthDecision`.
6. The response boundary builds `ProtocolMessage::AuthResponse`.
7. The response boundary returns an outbound handoff containing the destination
   source metadata and typed message.
8. The outbound queue boundary converts it into `OutboundQueueItem`.
9. The net send layer performs wire encode, then the UDP socket adapter can
   send the encoded datagram.

Responsibility split:

- `protocol`
  - Defines `AuthResponse`, `AuthResponseReasonCode`, and
    `ProtocolMessage::AuthResponse`.
  - Does not own server auth policy, outbound queueing, socket sends, or real
    packet encode in this step.
- server auth handler / decision layer
  - Owns the minimal authentication judgement.
  - Produces accepted/rejected result, reason code, and optional response
    metadata.
  - Leaves real config loading, external secret resolution, authenticated
    source state, and UDP send to later layers.
- response boundary
  - Converts the decision into a typed `AuthResponse` message.
  - Keeps destination metadata so the send layer knows where to send.
  - Does not update authenticated-client state, encode bytes, or send packets.
- net send layer
  - Owner of converting the typed message into wire bytes before the UDP socket
    adapter sends the encoded datagram.

Current placeholder: `apps/server::ServerAuthResponseBoundary` converts
`ServerAuthDecision` into `ServerOutboundAuthResponse`. This is only the
message-generation and send-layer handoff shape. Minimal auth decisions are
implemented by `ServerAuthDecisionBoundary`; authenticated source registration
and full send orchestration remain out of scope. `ServerAuthFlowStep` now
connects the decision and response boundaries to the outbound queue handoff.

---

## Outbound Packet / Queue Boundary

Outbound server messages stay typed until the future wire encode step. The send
boundary receives a `ProtocolMessage` and destination metadata; it does not
receive already-encoded bytes.

Flow:

1. Server response boundaries or future notification paths decide that an
   outbound message should be sent.
2. The response path builds a `ProtocolMessage`, for example
   `ProtocolMessage::AuthResponse`.
3. The response path attaches the destination address as send metadata.
4. The server outbound boundary converts the server-specific response handoff
   into `net-core::OutboundPacket`.
5. `net-core::OutboundPacketQueueBoundary` returns an
   `OutboundQueueItem` for a future queue implementation.
6. Future queue code may buffer or schedule the item.
7. Future encode code will convert the `ProtocolMessage` into fixed header plus
   payload bytes.
8. Future socket code will send those bytes through UDP.

Responsibility split:

- `protocol`
  - Owns `ProtocolMessage` and future encode rules.
  - Does not own destination metadata, queues, socket sends, retries, or server
    response policy.
- server response boundary
  - Builds typed outbound messages from server decisions.
  - Keeps enough destination metadata to hand the message to the send layer.
  - Does not encode bytes or send sockets.
- `net-core` send boundary
  - Owns generic outbound carriers: destination plus `ProtocolMessage`.
  - Owns the shape of the queue handoff item.
  - Does not implement the real queue, async runtime, wire encode, or UDP send.
- socket send layer
  - Future owner of UDP transmission and send errors.

Current placeholders: `OutboundPacket`, `OutboundQueueItem`, and
`OutboundPacketQueueBoundary` in `crates/net-core`, plus
`ServerOutboundQueueBoundary` in `apps/server`. `apps/server` also has typed
outbound handoff placeholders for `AuthResponse` and `HeartbeatAck`. These
types keep the send-layer contract visible while leaving queue implementation,
fragmentation, retry, and encryption out of scope. A minimal socket adapter can
send an already encoded datagram.

Outbound queue minimal processing is defined as a handoff lifecycle, not as a
real queue runtime. `ServerOutboundQueueBoundary` produces `OutboundQueueItem`;
the future queue holds that item, selects it for send, and hands it to the net
send layer. Protocol encode happens after this queue handoff in the net send
layer. Encode result handling and socket send result handling may update future
queue state, but retry execution remains a later task.

Send error / log event policy is owned by `net-core` after protocol encode.
`protocol` returns encode errors, while `net-core` keeps destination metadata
and extracts `run_id`, optional `client_id`, destination, and `message_type` for
future JSON Lines send logs. Retry execution and queue mutation remain outside
this protocol boundary; encoded datagram socket send is handled by the UDP
socket adapter.

---

## Net Send Layer / Protocol Encoder Boundary

Outbound messages remain typed until the net send layer explicitly calls the
protocol encoder boundary. This boundary is the point where a
`ProtocolMessage` becomes fixed header plus payload bytes. The current code
encodes `AuthResponse`, `HeartbeatAck`, and `VideoFrame`; unsupported outbound
messages still return `EncodeNotImplemented`.

Send path:

1. Server response boundaries or future notification paths create an outbound
   `ProtocolMessage`.
2. The server side attaches destination metadata and hands it to `net-core` as
   `OutboundPacket` / `OutboundQueueItem`.
3. `net-core` prepares `OutboundEncodeRequest`, which carries:
   - destination metadata
   - `EncodeContext { protocol_version }`
   - typed `ProtocolMessage`
4. `net-core` calls a `protocol::MessageEncoder` implementation with
   `EncodeContext`, `ProtocolMessage`, and an output byte buffer.
5. The protocol encoder writes the 16 byte fixed header first, then the
   message-specific payload bytes for supported outbound messages.
6. `net-core` pairs the encoded byte buffer with destination metadata as
   `EncodedOutboundPacket`.
7. The UDP socket adapter receives encoded bytes plus destination and sends one
   UDP datagram.

Responsibility split:

- `protocol`
  - Defines `ProtocolMessage`, `EncodeContext`, `MessageEncoder`, and protocol
    encode errors.
  - Owns fixed header writing, `message_type` selection, `protocol_version`
    writing, `payload_length` calculation, and implemented payload layout
    encoding.
  - Does not know destination addresses, queues, retries, or socket errors.
- `net-core`
  - Receives `ProtocolMessage` plus destination metadata from server-side
    response boundaries or future queues.
  - Calls the protocol encoder boundary.
  - Preserves destination metadata before and after encode.
  - Does not implement message-specific wire encoding or UDP socket sends.
- server response boundary
  - Builds outbound typed messages such as `AuthResponse`.
  - Does not call socket send and does not write bytes.
- socket send layer
  - Owner of sending `EncodedOutboundPacket.bytes` to
    `EncodedOutboundPacket.destination`.
  - Does not inspect or route typed messages.

Current code:

- `crates/protocol::ProtocolMessage::message_type()` exposes the stable
  message kind for encoder boundary and errors.
- `crates/protocol::ProtocolMessageEncoderBoundary` implements
  `MessageEncoder`, encodes `ProtocolMessage::AuthRequest`,
  `ProtocolMessage::AuthResponse`, `ProtocolMessage::HeartbeatAck`, and
  `ProtocolMessage::VideoFrame`, and returns
  `ProtocolError::EncodeNotImplemented` for other message types.
- `crates/net-core::OutboundPacketEncoderBoundary` prepares encode requests and
  maps protocol encode errors while keeping the destination attached.
- `crates/net-core::OutboundQueueLifecycleBoundary` defines the one-item queue
  lifecycle placeholder from queued item to send-layer handoff and terminal
  states.
- `crates/net-core::OutboundSendLogContext` and `SendLogEvent` define the
  future send log context shape for encode success / failure and socket send
  failures.
- `apps/server::ServerHeartbeatAckBoundary` builds
  `ProtocolMessage::HeartbeatAck` and hands it to the same outbound queue
  boundary shape as other typed responses.
- `crates/net-core::UdpSocketIoBoundary` sends already encoded packets with one
  `send_to` call.

`AuthRequest`, `AuthResponse`, `HeartbeatAck`, and `VideoFrame` encode now
write the 16 byte fixed header and the documented payload bytes. Other message
encoders, outbound queue processing, continuous send orchestration, retry, and
fragmentation are still future tasks.
