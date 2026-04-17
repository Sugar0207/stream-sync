<!-- stream-sync/docs/operations/session-log.md -->

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側の net send layer における outbound packet / queue 境界を設計した
- `ProtocolMessage` と宛先情報を `net-core::OutboundPacket` として保持し、future queue へ渡す `OutboundQueueItem` placeholder を追加した
- `apps/server` に `ServerOutboundQueueBoundary` を追加し、`ServerOutboundAuthResponse` を generic outbound handoff に変換できる形にした
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に server / response boundary / net send layer / socket send の責務分離を追記した

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- outbound send boundary は wire bytes ではなく、typed `ProtocolMessage` と destination metadata を受け取る
- response boundary は message 生成と宛先保持までを担当する
- `net-core` は generic outbound carrier と queue handoff item の形だけを担当する
- 実 queue、wire encode、UDP socket send、retry、fragmentation は後続タスクに残す

### 未実装 / 保留
- outbound queue の実装本体
- encode 本実装
- UDP socket 送信本体
- retry / fragmentation / encryption
- 認証成功 / 失敗判定
- heartbeat / video frame 処理本体

### 次にやる候補
- `AuthResponse` encode 境界と payload byte layout を整理する
- net send layer の encode 呼び出し境界を設計する
- UDP socket 送信本体前の send error / log event 方針を設計する

### TODO更新
- 完了:
  - outbound packet / queue 境界 docs 反映
  - `OutboundPacket` / `OutboundQueueItem` / `OutboundPacketQueueBoundary` placeholder 追加
  - `ServerOutboundQueueBoundary` placeholder 追加
- 追加:
  - outbound queue の実装本体
  - net send layer の encode 呼び出し境界設計
- 保留:
  - encode 本実装
  - UDP socket 送信本体
  - queue 実処理 / async runtime

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側で `AuthResponse` を生成し、送信レイヤへ渡す境界を設計した
- `ServerAuthDecision` から `ProtocolMessage::AuthResponse` を構築し、宛先 `PacketSource` と一緒に `ServerOutboundAuthResponse` として返す placeholder を追加した
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に `protocol` / server auth handler / response boundary / net send layer の責務分離を追記した

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `protocol` は `AuthResponse` message 型と reason code を持つ
- server auth handler / decision layer は将来、token / `client_id` / `protocol_version` / `app_version` を見て認証結果を返す
- response boundary は認証結果を `AuthResponse` message と送信先 metadata に変換するだけに留める
- wire encode と UDP socket 送信は future net send layer に残す

### 未実装 / 保留
- 認証成功 / 失敗判定の本実装
- client whitelist 読み込み
- 本物の token 検証
- `AuthResponse` encode 本実装
- UDP socket 送信本体
- heartbeat / video frame 処理本体

### 次にやる候補
- net send layer の outbound packet 型 / queue 境界を設計する
- `AuthResponse` payload byte layout と encode 境界を整理する
- server 側の認証状態更新境界を設計する

### TODO更新
- 完了:
  - `AuthResponse` 生成 / 送信境界 docs 反映
  - `ServerAuthDecision` / `ServerAuthResponseBoundary` / `ServerOutboundAuthResponse` placeholder 追加
  - auth decision -> `AuthResponse` -> send layer handoff の流れを定義
- 追加:
  - net send layer の outbound packet 型 / queue 境界を設計する
  - `AuthResponse` encode 本実装を行う
- 保留:
  - 認証成功 / 失敗判定
  - UDP socket 送信本体
  - encode / fragmentation / 再送制御 / 暗号化

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側 UDP 受信 loop の最小設計を行った
- `docs/architecture/system-design.md` に packet bytes 受信、送信元情報取得、`InboundPacket` 生成、decode、router 受け渡しの流れを追記した
- `docs/architecture/protocol.md` に receive loop 境界と decode error / protocol error の分類方針を追記した
- `apps/server` に `ServerReceiveLoopStep` / `ServerReceiveLoopOutcome` / `ServerRejectedPacket` / `ServerDecodeErrorAction` placeholder を追加した
- `ServerReceiveLoopStep` は既に受信済みの packet bytes と `PacketSource` を受け取り、`InboundPacketDecoder` と `ServerInboundRouter` を順番に呼ぶだけに留めた

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- UDP 受信 loop の責務は、packet bytes と送信元情報を受け取り、decode して server route へ渡すところまでに限定する
- `UnsupportedProtocolVersion` は `RejectProtocolVersion` として分類する
- `PayloadDecodeNotImplemented` は `UnsupportedInboundMessage` として分類する
- その他の `ProtocolError` は malformed packet として `DropPacket` に分類する
- socket 実装、非同期 runtime、packet 受信本体、認証判定、heartbeat 管理、video frame 処理本体は今回の範囲外とする

### 未実装 / 保留
- UDP socket の本実装
- 非同期 runtime 導入
- packet 受信本体
- receive loop のログ出力実装
- 認証成功 / 失敗判定の本実装
- heartbeat 管理 / timeout 管理
- video frame 受理 / 同期バッファ投入
- encode 本実装
- fragmentation / 再送制御 / 暗号化

### 次にやる候補
- server 側の認証 handler 境界を設計する
- receive loop のログイベント型を設計する
- UDP socket 実装前の設定値と bind address 方針を決める

### TODO反映
- 完了:
  - server UDP 受信 loop 境界 docs 反映
  - `ServerReceiveLoopStep` placeholder 追加
  - decode error / protocol error の分類方針追加
- 追加:
  - packet 受信本体を実装する
  - receive loop のログ出力方針を実装する
- 保留:
  - UDP socket の本実装
  - 認証 / heartbeat / video frame 処理本体
  - encode / fragmentation / 再送制御 / 暗号化

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側 handler が `DecodedInboundPacket` を受け取る境界を設計した
- `docs/architecture/system-design.md` に server handler 境界と `AuthRequest` / `Heartbeat` / `VideoFrame` の分岐責務を追記した
- `docs/architecture/protocol.md` に `protocol` / `net-core` / `apps/server` の責務分離を追記した
- `apps/server` に `ServerInboundRouter` / `ServerInboundRoute` placeholder を追加した
- `ServerInboundRouter` は `DecodedInboundPacket` を受け取り、decode 済み message を server 側 route に分類するだけに留めた

### 変更ファイル
- `apps/server/Cargo.toml`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- server は `net-core` から `DecodedInboundPacket` を受け取る
- server 側は `ProtocolMessage` variant、つまり `message_type` 相当の意味を見て処理方針を分岐する
- 認証、heartbeat、video frame の処理責務は server 側に残す
- `protocol` は wire format と decode、`net-core` は raw packet から decode 済み packet 生成、server は app 状態へ反映するための分岐を担当する
- 今回は route 分類だけを置き、認証判定、heartbeat 管理、video frame 処理本体は実装しない

### 未実装 / 保留
- UDP socket 実装
- 認証成功 / 失敗判定の本実装
- heartbeat 管理 / timeout 管理
- video frame 受理 / 同期バッファ投入
- encode 本実装
- `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の decode / encode
- fragmentation / 再送制御 / 暗号化

### 次にやる候補
- server 側の認証 handler 境界を設計する
- heartbeat handler 境界と timeout 管理の最小状態型を設計する
- UDP 受信 loop の最小設計を行う

### TODO反映
- 完了:
  - server handler 境界 docs 反映
  - `ServerInboundRouter` / `ServerInboundRoute` placeholder 追加
  - `AuthRequest` / `Heartbeat` / `VideoFrame` の route 分類
- 追加:
  - 認証成功 / 失敗判定の本実装
  - heartbeat 管理 / timeout 管理の本実装
  - video frame 受理 / 同期バッファ投入の本実装
- 保留:
  - UDP socket 実装
  - encode 本実装
  - fragmentation / 再送制御 / 暗号化

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `net-core` と `protocol` の受信 decode 境界を設計した
- `docs/architecture/system-design.md` に raw packet bytes 受領から decode 済み message を app / server handler へ渡すまでの責務分担を追記した
- `docs/architecture/protocol.md` に fixed header decode -> protocol_version check -> payload decoder dispatch -> app 受け渡しの順序を反映した
- `crates/protocol` に `decode_payload_by_message_type` を追加し、既存の `AuthRequest` / `Heartbeat` / `VideoFrame` payload decoder を message type で dispatch できるようにした
- `crates/net-core` に `InboundPacket`, `PacketSource`, `InboundPacketDecoder`, `DecodedInboundPacket`, `NetDecodeError` の最小境界型を追加した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `crates/net-core/Cargo.toml`
- `crates/net-core/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `net-core` は raw packet bytes と送信元 metadata を受け取り、protocol crate の decode entry point を順番に呼ぶ橋渡しに留める
- fixed header decode、protocol_version 期待値チェック、payload decoder dispatch は protocol crate の責務とする
- decode 成功時は `DecodedInboundPacket` として送信元 metadata と `ProtocolMessage` を app / server handler 側へ返す
- UDP socket loop、送信処理、app handler 実行、認証済み client 管理は今回の範囲外とする

### 未実装 / 保留
- UDP socket 実装
- server / client / switcher 側 handler 実装
- encode 本実装
- `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の decode / encode
- fragmentation / 再送制御 / 暗号化

### 次にやる候補
- UDP 受信 loop の最小設計を行う
- server 側 handler が `DecodedInboundPacket` を受け取る境界を設計する
- `AuthResponse` / `HeartbeatAck` の payload byte layout を決める

### TODO反映
- 完了:
  - `net-core` / `protocol` の受信 decode 境界 docs 反映
  - `decode_payload_by_message_type` の追加
  - `net-core` の最小 decode 境界型追加
- 追加:
  - UDP socket 実装
  - server / client / switcher 側 handler 実装
- 保留:
  - encode 本実装
  - fragmentation / 再送制御 / 暗号化

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に `VideoFrame` payload decode の最小実装を追加した
- `VideoFramePayloadDecoder` / `decode_video_frame_payload` を追加し、fixed header decode と protocol_version 期待値チェック後に payload 部分を型へ落とす入口を用意した
- `client_id`, `run_id`, 46 byte numeric metadata, H.264 bytes を docs の byte layout どおりに読む処理を追加した
- `payload_size` と実際の残り H.264 byte 数の整合、不正 bool、不正 `metadata_reserved`、未対応 codec を最小 error として返すようにした
- `docs/architecture/protocol.md` と TODO を今回の実装状態に合わせて更新した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `VideoFrame` payload decode は metadata と H.264 bytes の境界確認までを protocol crate の責務とする
- H.264 bytes は中身を解釈せず、`payload_size` と残り byte 数が一致した場合にだけ `Vec<u8>` として復元する
- `metadata_reserved` は初期 wire format では全 byte `0` のみ受理する
- encode、UDP 通信、app handler、fragmentation / 再送制御 / 暗号化は今回の範囲外とする

### 未実装 / 保留
- encode 本実装
- UDP 通信実装
- server / client / switcher 側 handler 実装
- `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の payload layout と decode 方針
- fragmentation / 再送制御 / 暗号化

### 次にやる候補
- encode API の最小実装範囲を決める
- `AuthResponse` / `HeartbeatAck` の payload byte layout を決める
- `net-core` 側で fixed header decode と payload decoder を呼ぶ境界を設計する

### TODO反映
- 完了:
  - `VideoFrame` payload decode の最小実装
  - `payload_size` と H.264 bytes の境界検証
  - `VideoFrame` decode 実装状態の docs 反映
- 追加:
  - `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の payload layout と decode 方針を決める
- 保留:
  - encode 本実装
  - UDP 通信実装
  - app handler 実装

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に `Heartbeat` payload decode の最小実装を追加した
- docs の payload byte layout に従い、`client_id`, `run_id`, `sent_at`, `local_time`, `short_status` を復元できるようにした
- `local_time` を `optional<u64>` から `Option<TimestampMicros>` として、`short_status` を `optional<string>` から `Option<String>` として decode するようにした
- 不正 payload 長、未期待 message type、不正 optional tag の単体テストを追加した
- `docs/architecture/protocol.md` と TODO を今回の実装状態に合わせて更新した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `Heartbeat` payload decode は fixed header decode と protocol_version 期待値チェックが済んだ後に呼ぶ前提とする
- protocol crate は payload を `Heartbeat` 型へ落とす責務までに留める
- 生存確認更新、timeout 判定、RTT 計算、認証済み client 管理は app / server 側の責務とする
- `VideoFrame` / `AuthResponse` / encode / UDP / app handler は今回の範囲外とする

### 未解決事項
- `VideoFrame` payload decode の最小実装
- `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の payload layout と decode 方針
- encode 本実装
- UDP 通信、server / client / switcher handler 実装

### 次にやる候補
- `VideoFrame` payload decode の最小実装範囲を決める
- `AuthResponse` / `HeartbeatAck` payload byte layout を決める
- protocol decode 結果を server 側 handler に渡す境界を設計する

### TODO更新
- 完了:
  - `Heartbeat` payload decode の最小実装
  - optional timestamp / optional string decode
  - 不正 payload に対する最小 error と単体テスト
- 追加:
  - `VideoFrame` payload decode の最小実装
  - encode 本実装
- 保留:
  - UDP 通信と app handler 実装
  - fragmentation / 再送制御 / 暗号化

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に `AuthRequest` payload decode の最小実装を追加した
- docs の payload byte layout に従い、`client_id`, `run_id`, `app_version`, `shared_token`, `display_name` を復元できるようにした
- 可変長 string を `u16 byte_length` + UTF-8 bytes として読み、`display_name` は `u8 present` + optional string として読めるようにした
- 不正 payload 長、invalid UTF-8、不正 optional tag、想定外 message type の最小 error と単体テストを追加した
- `docs/architecture/protocol.md` と TODO を今回の実装状態に合わせて更新した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `AuthRequest` payload decode は fixed header decode と protocol_version 期待値チェックが済んだ後に呼ぶ前提とする
- protocol crate は payload を `AuthRequest` 型へ落とすだけに留め、認証成功 / 失敗判定は持たない
- 初期 wire layout に無い `capabilities` は空配列、`requested_video_profile` は `None` として復元する
- `Heartbeat` / `VideoFrame` / encode / UDP / app handler は今回の範囲外とする

### 未解決事項
- `Heartbeat` payload decode の最小実装
- `VideoFrame` payload decode の最小実装
- encode 本実装
- UDP 通信、server / client / switcher handler 実装

### 次にやる候補
- `Heartbeat` payload decode の最小実装を追加する
- `AuthRequest` decode 結果を server 側認証処理へ渡す境界を決める
- `AuthResponse` payload byte layout と decode / encode 方針を決める

### TODO更新
- 完了:
  - `AuthRequest` payload decode の最小実装
  - 可変長 string / optional string decode
  - 不正 payload に対する最小 error と単体テスト
- 追加:
  - `Heartbeat` payload decode の最小実装
  - `VideoFrame` payload decode の最小実装
- 保留:
  - encode 本実装
  - UDP 通信と app handler 実装

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に `protocol_version` 期待値チェックの最小実装を追加した
- fixed header decode 後の `FixedHeader.protocol_version` と `DecodeContext.expected_protocol_version` を照合できるようにした
- 不一致時に `ProtocolError::UnsupportedProtocolVersion` を返す単体テストを追加した
- `docs/architecture/protocol.md` に fixed header decode 後 / payload decode 前に検証する実装状態を反映した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `protocol_version` の期待値は app 側が `DecodeContext` として渡す
- protocol crate は fixed header の値を比較し、error を返す判定ロジックだけを持つ
- `protocol_version` 検証は fixed header decode 後、payload decode 前に行う
- payload の意味解釈、UDP 通信、app handler 側の接続拒否変換は今回の範囲外とする

### 未解決事項
- payload decode / encode の本実装
- app / server / client / switcher 側で protocol error を接続拒否や packet 破棄へ変換する処理
- AuthResponse / HeartbeatAck / ClientStats / ServerNotice の payload byte layout

### 次にやる候補
- AuthRequest payload decode の最小実装範囲を決める
- Heartbeat payload decode の最小実装範囲を決める
- app handler 側で `UnsupportedProtocolVersion` をどう扱うか決める

### TODO更新
- 完了:
  - protocol_version 期待値チェックの最小実装
  - fixed header decode 後 / payload decode 前の検証方針の docs 反映
- 追加:
  - server / client / switcher 側の handler で protocol error を接続拒否や破棄へ変換する
- 保留:
  - payload decode / encode の本実装

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `AuthRequest` / `Heartbeat` / `VideoFrame` の payload byte layout を設計した
- `docs/architecture/protocol.md` に各 payload のフィールド順、wire type、可変長 field の長さ情報を追記した
- `VideoFrame` の frame metadata と H.264 payload bytes の境界を明記した
- `crates/protocol` に payload layout 共有用の最小定数を追加した

### 変更ファイル
- `docs/architecture/protocol.md`
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- payload 内の数値は fixed header と同じく little-endian とする
- string は `u16 byte_length` + UTF-8 bytes とする
- optional field は `u8 present` の後に値を置く形式とする
- `VideoFrame` は `client_id` / `run_id` の後に 46 byte の numeric metadata を置き、その直後に `payload_size` byte の H.264 bytes を置く
- H.264 bytes には追加の長さ prefix を置かず、直前の `payload_size` で境界を決める

### 未解決事項
- payload decode / encode の本実装
- AuthResponse / HeartbeatAck / ClientStats / ServerNotice の payload byte layout
- UDP 通信、server / client / switcher handler、fragmentation / 再送制御 / 暗号化

### 次にやる候補
- AuthRequest payload decode の最小実装範囲を決める
- Heartbeat payload decode の最小実装範囲を決める
- VideoFrame metadata decode と H.264 bytes 境界検証の最小実装範囲を決める

### TODO更新
- 完了:
  - AuthRequest / Heartbeat / VideoFrame payload byte layout の docs 反映
  - 可変長 string / optional / bytes の長さ情報方針の明記
  - VideoFrame metadata と payload 境界の明記
- 追加:
  - AuthResponse / HeartbeatAck / ClientStats / ServerNotice の payload byte layout
- 保留:
  - payload decode / encode の本実装
  - UDP 通信実装
  - fragmentation / 再送制御 / 暗号化

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に 16 byte fixed header decode の最小実装を追加した
- `message_type`, `header_length`, `protocol_version`, `payload_length`, `flags`, `reserved` を little-endian で読むようにした
- fixed header decode の責務を `docs/architecture/protocol.md` に反映した
- TODO を fixed header decode 完了状態へ更新した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- fixed header decode は 16 byte fixed header の構造確認と raw payload slice の切り出しまでを責務にする
- `header_length` は現時点では `FIXED_HEADER_LEN` と一致する場合のみ受理する
- 未知の `message_type`、短すぎる packet、`payload_length` と実 byte 数の不一致は `ProtocolError` として返す
- `protocol_version` の期待値チェックと payload の意味解釈は fixed header decode では行わない

### 未解決事項
- payload decode / encode の本実装
- message ごとの payload byte layout 詳細
- UDP 通信、server / client / switcher handler、fragmentation / 再送制御 / 暗号化

### 次にやる候補
- `AuthRequest` / `Heartbeat` / `VideoFrame` の payload byte layout を決める
- payload decode / encode の単体テスト方針を決める
- fixed header encode の最小実装要否を判断する

### TODO更新
- 完了:
  - fixed header decode の最小実装
  - fixed header decode の docs 反映
- 追加:
  - payload decode / encode の本実装
  - message ごとの payload byte layout 詳細
- 保留:
  - UDP 通信実装
  - server / client / switcher handler 実装
  - fragmentation / 再送制御 / 暗号化

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` における encode / decode API 境界を設計した
- `docs/architecture/protocol.md` に fixed header decode、message dispatch、payload decode、encode、protocol_version check の位置を追記した
- protocol crate、`net-core`、app 側の責務分離を整理した
- `crates/protocol` に API 境界用の placeholder 型、trait、error 型を追加した

### 変更ファイル
- `docs/architecture/protocol.md`
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- protocol crate は message 型、wire layout、decode / encode の入口境界、wire error 型を持つ
- UDP socket、送受信 loop、address、fragmentation / retry は protocol crate に入れない
- `protocol_version` の期待値は app 側が決め、payload decode 前に検証する
- fixed header decode は packet 構造確認と payload slice の切り出しまでに限定する
- payload decode は `message_type` による分岐後の入口として扱う
- encode は 1 packet buffer 作成までを protocol crate の境界とし、送信処理は `net-core` 側に置く

### 未解決事項
- fixed header decode の本実装
- payload decode / encode の本実装
- message ごとの payload byte layout 詳細
- UDP 通信実装と server / client / switcher handler 実装

### 次にやる候補
- fixed header decode の最小実装を追加する
- `AuthRequest` / `Heartbeat` / `VideoFrame` payload layout を決める
- encode / decode の単体テスト方針を決める

### TODO更新
- 完了:
  - encode / decode API 境界の docs 反映
  - API 境界用 placeholder trait / enum / error 型の追加
- 追加:
  - fixed header decode の本実装
  - payload decode / encode の本実装
- 保留:
  - UDP 通信実装
  - server / client / switcher handler 実装
  - fragmentation / 再送制御 / 暗号化

---

## 2026-04-16
### 種別
- Codex

### 今回の作業
- PoC / MVP 初期で使う最小 wire format の byte layout を設計した
- `docs/architecture/protocol.md` に 16 byte fixed packet header と可変長 payload 方針を追記した
- `message_type`, `protocol_version`, `payload_length` の扱いを整理した
- `AuthRequest` と `VideoFrame` の共通ヘッダ化範囲を fixed packet header までに限定した
- `crates/protocol` に header length / offset 定数と `FixedHeader` placeholder を追加した

### 変更ファイル
- `docs/architecture/protocol.md`
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 初期 fixed packet header は 16 byte とする
- offset 0 に `message_type: u16`、offset 4 に `protocol_version: u32`、offset 8 に `payload_length: u32` を置く
- 数値フィールドは little-endian とする
- `payload_length` は fixed header を含まない payload byte 数とする
- 可変長 payload の中身は `message_type` ごとに定義する
- `client_id` / `run_id` / timestamp / frame metadata は初期 fixed header に入れず、payload 側に置く

### 未解決事項
- encode / decode 本実装
- payload 内の各 message byte layout の詳細
- fragmentation / 再送制御 / 暗号化
- UDP 通信実装と server / client / switcher handler 実装

### 次にやる候補
- payload 内の `AuthRequest` / `Heartbeat` / `VideoFrame` metadata layout を詰める
- encode / decode API の境界だけ設計する
- 1人送信・受信・表示 PoC の準備に進む

### TODO更新
- 完了:
  - 最小 wire format byte layout の docs 反映
  - fixed header 定数と placeholder 追加
- 追加:
  - encode / decode 本実装
  - UDP / handler / fragmentation などの未実装項目
- 保留:
  - payload 内の message 別 byte layout 詳細
  - fragmentation / 再送制御 / 暗号化

---

# StreamSync Session Log

このファイルは、各作業セッションの記録を残すためのログです。

## 運用ルール
- 新しい作業をしたら、先頭または末尾に1件追記する
- Codex 作業後は必ず更新する
- 実装だけでなく、仕様変更・判断・保留事項も記録する
- `docs/operations/todo.md` の更新とセットで扱う
- 1セッションにつき、最低でも「今回の作業」「変更ファイル」「未解決」「次の候補」は記録する

---

## テンプレート

## YYYY-MM-DD HH:MM
### 種別
- GPT / Codex / Manual

### 今回の作業
- 

### 変更ファイル
- 

### 決定事項
- 

### 未解決事項
- 

### 次にやる候補
- 

### TODO更新
- 完了:
  - 
- 追加:
  - 
- 保留:
  - 

### メモ
- 

---

## 初回記録

## 2026-04-16
### 種別
- GPT

### 今回の作業
- プロジェクトの目的を定義
- PoC / MVP 条件を定義
- MVPでやらないことを整理
- 将来拡張項目を整理
- 技術スタックを決定
- OBS連携方式を決定
- 音声暫定方針を決定
- ネットワーク構成を決定
- 認証方式を決定
- ログ・計測方式を決定
- バージョン管理方針を決定
- プロジェクト名を `StreamSync` に決定
- `AGENTS.md` 初版を作成
- `docs/operations/todo.md` 初版を作成
- `docs/operations/session-log.md` テンプレを作成

### 変更ファイル
- `AGENTS.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- プロジェクト名は `StreamSync`
- リポジトリ名 / ルートフォルダ名は `stream-sync`
- 技術スタックは Rust + FFmpeg系 + UDP独自プロトコル + Rust製最小GUI
- コーデックは H.264
- 初期標準品質は 720p / 30fps
- 1080p / 60fps は条件付き上位運用モード
- OBS は switcher 専用ウィンドウを Window Capture
- MVP の音声は Discord 継続運用
- client は中央 server に直接 UDP 送信するスター構成
- 認証は事前共有トークン + clientId ホワイトリスト
- ログは JSON Lines + switcher UI メトリクス表示
- app_version と protocol_version を分離管理する

### 未解決事項
- `docs/requirements/project-overview.md` の初版作成
- Cargo workspace 初期化
- README 初版作成
- 認証メッセージ / heartbeat メッセージ定義

### 次にやる候補
- `docs/requirements/project-overview.md` 初版作成
- Codexで `stream-sync` の workspace 初期化
- README 初版作成
- 共通型定義の着手

### TODO更新
- 完了:
  - `AGENTS.md` 初版作成
  - `docs/operations/todo.md` 初版作成
  - `docs/operations/session-log.md` テンプレ作成
  - プロジェクト名決定
- 追加:
  - なし
- 保留:
  - なし

### メモ
- 今後は各項目決定後に TODO を最新版へ更新する運用
- Codex との受け渡しは `AGENTS.md` + `docs/operations/todo.md` + `docs/operations/session-log.md` を共通基盤とする

---

## 2026-04-16
### 種別
- Codex

### 今回の作業
- Cargo workspace を初期化
- ルート `Cargo.toml` / `.gitignore` / `rust-toolchain.toml` を作成
- `apps/client`, `apps/server`, `apps/switcher` を binary crate として作成
- `crates/protocol`, `crates/config`, `crates/logging`, `crates/timebase`, `crates/video-core`, `crates/net-core`, `crates/sync-core`, `crates/ui-core` を library crate として作成
- README の進捗欄を最小更新
- TODO のリポジトリ初期化項目を更新

### 変更ファイル
- `Cargo.toml`
- `rust-toolchain.toml`
- `.gitignore`
- `apps/client/Cargo.toml`
- `apps/client/src/main.rs`
- `apps/server/Cargo.toml`
- `apps/server/src/main.rs`
- `apps/switcher/Cargo.toml`
- `apps/switcher/src/main.rs`
- `crates/protocol/Cargo.toml`
- `crates/protocol/src/lib.rs`
- `crates/config/Cargo.toml`
- `crates/config/src/lib.rs`
- `crates/logging/Cargo.toml`
- `crates/logging/src/lib.rs`
- `crates/timebase/Cargo.toml`
- `crates/timebase/src/lib.rs`
- `crates/video-core/Cargo.toml`
- `crates/video-core/src/lib.rs`
- `crates/net-core/Cargo.toml`
- `crates/net-core/src/lib.rs`
- `crates/sync-core/Cargo.toml`
- `crates/sync-core/src/lib.rs`
- `crates/ui-core/Cargo.toml`
- `crates/ui-core/src/lib.rs`
- `readme.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- workspace は docs の方針通り 3 apps + 8 crates の構成にする
- 現段階では各 crate は scaffold のみに留め、protocol / 認証 / heartbeat / 映像処理の詳細実装は行わない
- Rust toolchain は `stable` を指定する

### 未解決事項
- `cargo` / `rustc` が現在の PATH に存在しないため、この環境では `cargo check --workspace` を実行できていない
- `docs/requirements/project-overview.md` は要求パスには存在せず、現状は `docs/operations/project-overview.md` にある
- README 初版作成 TODO は既存 `readme.md` があるため、扱いを次回整理する

### 次にやる候補
- Rust toolchain を利用できる状態にして `cargo check --workspace` を確認する
- `docs/requirements/project-overview.md` の配置を整理する
- 共通型定義に着手する

### TODO更新
- 完了:
  - Cargo workspace 作成
  - ルート `Cargo.toml` 作成
  - `.gitignore` 作成
  - `rust-toolchain.toml` 作成
  - `apps/*` 作成
  - `crates/*` 作成
  - `tmp` を git 管理外にする
  - リポジトリ初期化
- 追加:
  - Rust toolchain を PATH に追加して `cargo check --workspace` を確認する
- 保留:
  - `docs/requirements/project-overview.md` の配置整理

---

## 2026-04-16 23:23
### 種別
- Codex

### 今回の作業
- `crates/protocol` に MVP 通信基盤向けの基本識別型を追加
- 認証メッセージ `AuthRequest` / `AuthResponse` を Rust 型として定義
- heartbeat メッセージ `Heartbeat` / `HeartbeatAck` を Rust 型として定義
- message type 表現と認証応答 reason code を enum として定義
- `cargo check --workspace` が通ることを確認

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ClientId`, `RunId`, `AppVersion` は文字列 newtype として定義
- `ProtocolVersion` は整数 newtype として定義
- 本作業では serde 等のシリアライズ / デシリアライズ実装には進まない
- UDP 通信、handler、server / client / switcher 側の実装には進まない

### 未解決事項
- timestamp の単位と wire format は未確定
- `capabilities` / `requested_video_profile` の詳細構造は未確定
- VideoFrame / ClientStats / ServerNotice は未実装
- シリアライズ / デシリアライズ方針は未整理

### 次にやる候補
- 共通型のシリアライズ / デシリアライズ方針を整理する
- VideoFrame の最小構造を定義する
- 1人送信・受信・表示 PoC の着手準備をする

### TODO更新
- 完了:
  - 共通型定義を作る
  - 認証メッセージ形式を定義する
  - heartbeat メッセージ形式を定義する
  - `protocol_version` の共通定義を作る
  - `run_id` の共通定義を作る
  - 認証メッセージに `protocol_version` / `app_version` を含める
- 追加:
  - なし
- 保留:
  - シリアライズ / デシリアライズ処理
  - server 側の `protocol_version` 検証処理
  - app_version 差異時の warn ログ実装

### メモ
- `cargo check --workspace` は成功。
- docs と実装のズレとして、timestamp の単位は docs 側でもまだ詳細未確定のため、現時点では `u64` に留めた。

---

## 2026-04-16 23:39
### 種別
- Codex

### 今回の作業
- `crates/protocol` に映像送信用の最小構造 `VideoFrame` を追加
- 補助メッセージ `ClientStats` / `ServerNotice` を追加
- `MessageType` に `VideoFrame` / `ClientStats` / `ServerNotice` を追加
- 関連 enum として `Codec` / `NoticeType` を追加
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `VideoFrame` は MVP の最小構造に留め、payload は `Vec<u8>` として定義
- timestamp は既存 protocol crate の流儀に合わせて `u64` のまま扱う
- `Codec` は MVP 方針に合わせて現時点では `H264` のみ定義
- `ClientStats` は最小項目として `capture_fps` / `dropped_frames` / `bitrate_kbps` に絞る

### 未解決事項
- payload fragmentation / 再送制御は未実装
- シリアライズ / デシリアライズ方針は未整理
- `payload_size` と `payload.len()` の検証処理は未実装
- `ClientStats` の詳細項目と送信間隔は未確定

### 次にやる候補
- 共通型のシリアライズ / デシリアライズ方針を整理する
- protocol_version チェック方針を整理する
- 1人送信・受信・表示 PoC の着手準備をする

### TODO更新
- 完了:
  - VideoFrame の最小構造を定義する
  - stats用メッセージを定義する
  - 直近項目から VideoFrame の最小構造定義を外す
- 追加:
  - protocol_version チェック方針を整理する
- 保留:
  - シリアライズ / デシリアライズ処理
  - UDP 通信実装
  - server / client / switcher 側 handler 実装

### メモ
- docs と実装のズレとして、`VideoFrame` の任意フィールド `encode_duration_ms` / `color_format` / `profile_name` は MVP 最小構造から外した。
- `ClientStats` の docs 上の任意フィールドも、今回の最低限項目以外は未実装に留めた。

---

## 2026-04-16 23:43
### 種別
- Codex

### 今回の作業
- protocol timestamp の単位をマイクロ秒に統一
- `crates/protocol` に `TimestampMicros` newtype を追加
- 既存メッセージ型の timestamp 関連フィールドを `TimestampMicros` に変更
- `docs/architecture/protocol.md` に timestamp 単位と clock domain 方針を追記
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- protocol timestamp の単位はマイクロ秒とする
- Rust 側表現は `TimestampMicros(pub u64)` とする
- client 側 timestamp と server 側 timestamp は、それぞれの clock domain の値として扱う
- PoC / MVP では単調増加する時計を優先し、Unix epoch 固定は wire format 確定時に再検討する

### 未解決事項
- wire format 上のバイト列や endian は未確定
- clock source の具体実装は timebase / client / server 実装時に決める
- timestamp の妥当性検証、補正、offset 推定処理は未実装

### 次にやる候補
- 共通型のシリアライズ / デシリアライズ方針を整理する
- protocol_version チェック方針を整理する
- timestamp を使った RTT / offset 推定の型境界を整理する

### TODO更新
- 完了:
  - timestamp の単位と Rust 表現を整理する
- 追加:
  - なし
- 保留:
  - シリアライズ / デシリアライズ処理
  - UDP 通信実装
  - server / client / switcher 側 handler 実装

### メモ
- docs とコードのズレだった timestamp 単位未確定状態を解消した。
- `AuthResponse.server_time`, `Heartbeat.sent_at`, `Heartbeat.local_time`, `HeartbeatAck` の時刻群、`VideoFrame.capture_timestamp`, `VideoFrame.send_timestamp`, `ClientStats.sent_at` を `TimestampMicros` に変更した。

---

## 2026-04-16 23:47
### 種別
- Codex

### 今回の作業
- `docs/architecture/protocol.md` にシリアライズ / デシリアライズ方針を追記
- PoC / MVP の wire format 方針を、バイナリ寄りの独自形式として整理
- `protocol_version` と `message_type` を payload decode 前に読む方針を明記
- `MessageType` に初期 wire 識別子を割り当て
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- PoC / MVP では JSON ではなく、バイナリ寄りの独自 wire format を前提にする
- 完全な byte layout はまだ固定せず、最小 envelope の設計を次段階に残す
- envelope には最低限 `protocol_version` と `message_type` を含め、payload decode 前に検査する
- 数値型は実装時に little-endian へ統一する方針とする
- 未知の `message_type` や protocol mismatch は decode 失敗または packet 破棄として扱う

### 未解決事項
- encode / decode trait と実装は未追加
- 最小 wire format の byte layout は未確定
- fragmentation / 再送制御 / 暗号化は未設計
- payload 長や必須フィールドの具体的な検証実装は未着手

### 次にやる候補
- protocol_version チェック方針を整理する
- 最小 wire format の byte layout を設計する
- 1人送信・受信・表示 PoC の着手準備をする

### TODO更新
- 完了:
  - 共通型のシリアライズ / デシリアライズ方針を整理する
  - 直近項目からシリアライズ / デシリアライズ方針整理を外す
- 追加:
  - 最小 wire format の byte layout を設計する
- 保留:
  - シリアライズ / デシリアライズ処理の本格実装
  - UDP 通信実装
  - server / client / switcher 側 handler 実装

### メモ
- `crates/protocol` 側は `MessageType` の `#[repr(u16)]` と数値割り当てのみ追加し、encode / decode 本体は実装していない。
## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側の認証 handler 境界を設計した
- `ServerInboundRouter` が認識した `AuthRequest` route を auth handler boundary へ渡す形を追加した
- `ServerAuthHandlerBoundary` / `ServerAuthCheck` / `ServerAuthBoundaryError` placeholder を `apps/server` に追加した
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に `protocol` / `net-core` / `ServerInboundRouter` / auth handler の責務分離を追記した

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `protocol` は wire decode と `AuthRequest` payload decode までを担当し、認証ビジネスロジックは持たない
- `net-core` は raw packet bytes と source metadata から `DecodedInboundPacket` を作る橋渡しに留める
- `ServerInboundRouter` は `AuthRequest` を認証 route として分類するだけに留める
- auth handler boundary は decoded `AuthRequest` から `shared_token` / `client_id` / `protocol_version` / `app_version` などの認証判定入力を準備する
- 認証結果による server 状態更新、認証済み送信元登録、`AuthResponse` 生成 / 送信は auth handler boundary の外側に残す

### 未実装 / 保留
- 認証成功 / 失敗判定の本実装
- client whitelist 読み込み
- 本物の token 検証
- `AuthResponse` 生成 / 送信境界
- UDP socket 実装
- heartbeat 管理 / timeout 管理
- video frame 受理 / 同期バッファ投入

### 次にやる候補
- `AuthResponse` 生成 / 送信境界を設計する
- client whitelist と token 検証の設定入力境界を設計する
- heartbeat handler 境界と timeout 管理の最小設計を行う

### TODO更新
- 完了:
  - server 認証 handler 境界 docs 反映
  - `ServerAuthHandlerBoundary` / `ServerAuthCheck` placeholder 追加
  - `AuthRequest` route から認証判定入力を準備する境界追加
- 追加:
  - 認証成功 / 失敗判定の本実装
  - client whitelist 読み込み
  - 本物の token 検証
  - `AuthResponse` 生成 / 送信境界設計
- 保留:
  - UDP socket 実装
  - heartbeat / video frame 処理本体
  - encode / fragmentation / 再送制御 / 暗号化

---
