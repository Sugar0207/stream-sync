<!-- stream-sync/docs/operations/session-log.md -->

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
