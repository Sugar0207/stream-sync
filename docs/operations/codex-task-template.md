<!-- stream-sync/docs/operations/codex-task-template.md -->

# Codex Task Template

このファイルは、StreamSync プロジェクトで Codex に作業を依頼する際のテンプレートです。

## 使い方
- その回の作業内容に応じて各項目を埋める
- `AGENTS.md` と `docs/` を前提資料として扱う
- 作業後は `docs/operations/todo.md` と `docs/operations/session-log.md` の更新を必須とする
- 実装範囲を明確にし、未実装にしてよい範囲も明記する
- 変更報告の形式を固定する

---

## テンプレート

### 1. 前提
- このリポジトリは `StreamSync`
- `AGENTS.md` の内容に従うこと
- `docs/requirements/project-overview.md`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/architecture/decisions.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

を読んでから作業すること

### 2. 今回の目的
- ここに今回の目的を書く

例:
- Cargo workspace の初期化
- protocol crate の基本型定義
- AuthRequest / AuthResponse の型実装
- client 設定読み込みの雛形実装

### 3. 今回やってよい範囲
- ここに変更してよいファイルや範囲を書く

例:
- `Cargo.toml`
- `apps/*`
- `crates/protocol/*`
- `README.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 4. 今回やってはいけないこと
- 技術スタックを変更しない
- 通信方式を変更しない
- WebRTC / TCP / SRT / RIST に切り替えない
- 音声統合を始めない
- 1080p / 60fps を標準前提で実装しない
- 未決定事項を勝手に確定しない
- 指示のない大規模リファクタをしない

必要に応じて、この回固有の禁止事項も追加する

### 5. 完了条件
- この回で何ができたら完了かを書く

例:
- workspace が作成される
- 各 crate が cargo check を通る
- protocol の基本型が定義される
- docs が更新される

### 6. 作業後に必ず行うこと
- `docs/operations/todo.md` を更新する
- `docs/operations/session-log.md` に追記する
- 必要に応じて `README.md` や `docs/architecture/*` を更新する
- 変更内容を最後に要約する

### 7. 最後に出力するフォーマット
以下の形式で必ず報告すること

#### 今回の作業
- 

#### 変更したファイル
- 

#### 実装したこと
- 

#### 未実装 / 保留
- 

#### 次にやる候補
1. 
2. 
3. 

#### TODO更新内容
- 完了:
  - 
- 追加:
  - 
- 保留:
  - 

---

## 依頼文テンプレート

以下をコピーして使う。

### Codex 依頼テンプレート

このリポジトリは `StreamSync` です。  
作業前に以下を読んでください。

- `AGENTS.md`
- `docs/requirements/project-overview.md`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/architecture/decisions.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 今回の目的
- ここに今回の目的を書く

### 今回やってよい範囲
- ここに今回変更してよい範囲を書く

### 今回やってはいけないこと
- 技術スタックを変更しない
- 通信方式を変更しない
- 音声統合を始めない
- 1080p / 60fps を標準前提で実装しない
- 未決定事項を勝手に確定しない
- 指示のない大規模リファクタをしない

### 完了条件
- ここに完了条件を書く

### 作業後に必ず行うこと
- `docs/operations/todo.md` を更新する
- `docs/operations/session-log.md` に追記する
- 必要に応じて関連 docs を更新する

### 最後に出力する形式
#### 今回の作業
- 

#### 変更したファイル
- 

#### 実装したこと
- 

#### 未実装 / 保留
- 

#### 次にやる候補
1. 
2. 
3. 

#### TODO更新内容
- 完了:
  - 
- 追加:
  - 
- 保留:
  - 

---

## 初回で使う想定の例

### 今回の目的
- Cargo workspace の初期化
- `apps/client`, `apps/server`, `apps/switcher` の作成
- `crates/protocol`, `crates/config`, `crates/logging`, `crates/timebase`, `crates/video-core`, `crates/net-core`, `crates/sync-core`, `crates/ui-core` の作成
- ルート `Cargo.toml`, `.gitignore`, `rust-toolchain.toml` の作成
- `README.md` と docs の整合を保つ

### 今回やってよい範囲
- ルートファイル
- `apps/*`
- `crates/*`
- `README.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 完了条件
- Cargo workspace が初期化されている
- 各 crate / app が作成されている
- ルート構成が docs の方針と一致している
- TODO と session-log が更新されている