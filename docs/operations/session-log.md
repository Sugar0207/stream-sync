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
