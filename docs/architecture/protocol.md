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
- payload 用の長さ prefix / optional tag / H.264 codec 値 / VideoFrame numeric metadata 長の定数

fixed header decode は `message_type`, `header_length`, `protocol_version`, `payload_length`, `flags`, `reserved` を little-endian で読む。`header_length` は現時点では `16` のみを受理し、未知の `message_type`、短すぎる packet、`payload_length` と実 byte 数の不一致は `ProtocolError` とする。

`protocol_version` の期待値チェックは、fixed header decode 後、payload decode 前に `DecodeContext.expected_protocol_version` と `FixedHeader.protocol_version` を比較する最小実装を置く。不一致時は `ProtocolError::UnsupportedProtocolVersion` を返す。
payload decode は現時点では `AuthRequest` に限定する。`AuthRequest` payload decode は fixed header decode と protocol_version 期待値チェックが済んだ後に、`client_id`, `run_id`, `app_version`, `shared_token`, `display_name` を docs の byte layout どおりに読む。その他 message の payload decode と encode はまだ本実装しない。

## Encode / decode API boundary

PoC / MVP 初期では、`crates/protocol` は wire format と message 型の境界を定義する。UDP socket、送受信ループ、認証済み client 管理、server / client / switcher 側 handler は持たない。

この段階では fixed header decode、protocol_version 期待値チェック、`AuthRequest` payload decode の最小実装だけを置く。その他 payload decode と encode の本実装は行わない。

### Responsibility split

`crates/protocol` の責務:
- fixed header layout、offset、message type、protocol version 型を定義する
- fixed header decode の入口を定義する
- `message_type` による payload decode 分岐の入口を定義する
- encode の入口を定義する
- wire format 上の error 型を定義する
- H.264 payload の中身は解釈しない

`crates/net-core` の責務:
- UDP socket の送受信を扱う
- datagram buffer を確保し、protocol crate に byte slice を渡す
- packet の送信先 / 受信元 address を扱う
- 将来の fragmentation / 再送制御を扱う場合も protocol crate ではなく net 側で境界を持つ

`apps/client` / `apps/server` / `apps/switcher` の責務:
- `protocol_version` の期待値を決める
- protocol crate が返した message を app の状態に反映する
- 認証状態、clientId whitelist、heartbeat timeout、buffer 管理、UI 表示を扱う
- protocol error をログや切断判断へ変換する

### Decode boundary

受信時の想定順序:

1. `net-core` が UDP datagram を受け取り、byte slice と送信元 address を app 側へ渡す。
2. app 側は `DecodeContext { expected_protocol_version }` を用意する。
3. protocol crate の fixed header decode 入口で 16 byte fixed header を読む。
4. fixed header decode は `message_type`, `header_length`, `protocol_version`, `payload_length`, `flags`, `reserved` と payload slice の境界だけを返す。
5. app 側は `DecodeContext` で期待する `protocol_version` を渡し、protocol crate の `validate_protocol_version` で payload decode 前に一致を確認する。
6. `message_type` で payload decoder を分岐する。
7. payload decoder は `AuthRequest`, `Heartbeat`, `VideoFrame` などの message 型へ変換する。
8. app 側 handler が認証、同期、buffer、表示などの処理を行う。

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

`FixedHeaderCodec` と `decode_fixed_header` は、16 byte fixed header の byte parsing と payload slice の切り出しだけを行う。`AuthRequestPayloadDecoder` と `decode_auth_request_payload` は `AuthRequest` payload のみを型へ変換する。`MessageEncoder` は境界名と責務を固定するための placeholder であり、現時点では byte writing は実装しない。

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

`crates/protocol` の最小実装では、上記 payload を `AuthRequest` 型へ復元する。`capabilities` は空配列、`requested_video_profile` は `None` として扱い、認証成功 / 失敗判定は app / server 側に残す。

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

#### 備考
- client はこれを使って RTT の概算を取れる
- server 側でも受信時刻を保持して offset 推定に使える

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
