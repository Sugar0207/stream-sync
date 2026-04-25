<!-- stream-sync/AGENTS.md -->

# AGENTS.md

## 概要

このリポジトリは、多視点スイッチング配信ツール `StreamSync` を開発するためのものです。

目的は、4人のゲーム映像をできるだけ高精度に同期し、OBS に載せられる実用的な配信基盤を作ることです。

---

## 最重要方針

* 完全同期を最優先にする
* MVP では 4 人固定で進める
* 映像同期基盤を主役にする
* 音声統合は MVP の対象外とする
* 標準品質は 720p / 30fps とする
* 1080p / 60fps は将来拡張かつ条件付き上位運用とする
* OBS は最終出力先とし、switcher の専用表示ウィンドウを Window Capture で取り込む
* 既存の決定事項を勝手に変更しない
* 未決定事項を勝手に確定しすぎない
* 必要なら提案として残す

---

## Step の定義

このプロジェクトにおける「step」は以下とする。

* 1 step = Codex と GPT の 1 往復（1ラリー）

  * GPT が次の実装方針・指示を提示する
  * Codex がそれに基づいて実装・変更を行い、結果を報告する
  * この一連のやり取りを 1 step と数える

### 補足

* step は「機能の大きさ」ではなく「やり取り単位」でカウントする
* 1 step の中で複数ファイル変更や複数境界実装が含まれてもよい
* 実装が小さくても、やり取りが1回発生すれば 1 step とする
* 「推定残りステップ数」は、この定義に基づいて見積もる

### 目的

* 進捗を現実的な粒度で可視化するため
* 作業量ではなく「対話ベースの開発コスト」を管理するため

---

## 決定済み技術方針

* 言語: Rust
* 映像処理: FFmpeg 系
* 通信: UDP 独自プロトコル
* コーデック: H.264
* UI: Rust 製の最小 GUI
* 設定ファイル: TOML
* ログ: JSON Lines
* 認証: 事前共有トークン + clientId ホワイトリスト
* バージョン管理: `app_version` と `protocol_version` を分離

---

## ネットワーク構成

* client 4 台が中央 server に直接 UDP 送信するスター構成
* server が同期責任を持つ
* switcher は表示専用
* MVP 初期段階では server と switcher は同一 PC 運用でよい

---

## MVP でやらないこと

* 音声の専用統合
* 自動スイッチング
* 発話検知による自動強調
* Minecraft イベント連動演出
* 録画保存 / アーカイブ管理
* リプレイ / クリップ自動生成
* 5 人以上への一般化
* 視点数の動的増減
* 高度な権限管理
* OBS の高度な自動制御
* WebRTC / Electron 中心構成への変更
* 音声経路の大幅変更

---

## ディレクトリ構成方針

* `apps/client`: 送信クライアント
* `apps/server`: 受信・同期・バッファ管理
* `apps/switcher`: 表示・切り替え・OBS 出力
* `crates/protocol`: 通信メッセージ・ヘッダ・共通型
* `crates/config`: TOML 設定読み込み
* `crates/logging`: 構造化ログ
* `crates/timebase`: RTT / offset / 時刻補正
* `crates/video-core`: 映像処理共通基盤
* `crates/net-core`: UDP 通信共通基盤
* `crates/sync-core`: ジッターバッファ / targetTime / 同期処理
* `crates/ui-core`: UI 共通処理
* `docs/requirements`: 要件整理
* `docs/architecture`: 設計資料
* `docs/operations`: TODO、作業ログ、運用メモ

---

## 実装ルール

* 一度に大きく作り込みすぎず、PoC → MVP の順で進める
* まず動く最小構成を優先する
* 高画質化より同期安定性を優先する
* プロトコル互換性に関わる変更は `protocol_version` を意識する
* アプリ変更と通信仕様変更を区別する
* リファクタ時は責務分離を意識する
* ログとメトリクスを軽視しない

---

## 引き継ぎ方針

* 会話履歴を共通記憶として前提にしない
* PC や Codex セッションが変わっても repo 内ファイルだけで再開できる状態を維持する
* 共通認識は必ず repo 内に残す
* 仕様、判断、進捗、未解決事項は `docs/` と `configs/` に反映する
* 最新の作業状態は `docs/operations/todo.md` と `docs/operations/session-log.md` を正とする
* 作業前に最低限以下を確認する

  * `AGENTS.md`
  * `README.md`
  * `docs/requirements/project-overview.md`
  * `docs/architecture/system-design.md`
  * `docs/architecture/protocol.md`
  * `docs/architecture/decisions.md`
  * `docs/operations/todo.md`
  * `docs/operations/session-log.md`

---

## todo.md の運用ルール

* `docs/operations/todo.md` は現在位置とタスク一覧を書く
* `docs/operations/todo.md` に時系列の作業履歴を書かない
* 時系列の作業履歴、判断理由、各回の作業メモは `docs/operations/session-log.md` にだけ残す
* `Codex update` のような履歴見出しを `todo.md` に追加しない
* 同じ意味のタスクを `todo.md` の複数箇所に重複して書かない
* 完了タスクは `[x]` のまま `todo.md` に残してよい
* 未完了タスクは `[ ]` として管理する
* 完了タスクを残す場合でも、現在も参照価値があるものだけを残す
* 参照価値の低い完了タスクや、他の箇所と重複する完了タスクを増やしすぎない
* `現在位置` に要約済みの内容と、チェックタスクの内容を重複して増やしすぎない
* `session-log.md` に書いた履歴を `todo.md` に重複して残さない
* `直近でやること` は常に最新の優先順位に更新する
* `直近でやること` は、可能なら「再確認」だけでなく最小実装または最小動作確認の形で更新する
* ロードマップには詳細タスクを重複して書かず、フェーズの状態と残りの要点だけを書く
* `現在位置` は要約、チェックリストは状態管理、ロードマップはフェーズ管理として役割を分ける

---

## Git運用ルール

* 作業ごとに Git checkpoint を意識する
* `cargo check` や `cargo fmt --check` が通っている場合はコミット推奨を明示する
* docs と code の両方が更新された場合はコミット候補を提示する
* push は人間確認後を前提とし、勝手に main へ push しない
* 最後の報告には以下を含める

  * コミット推奨: Yes / No
  * push 推奨: Yes / No
  * 推奨コミットメッセージ

---

## Codex が必ず守ること

* 会話履歴ではなく repo 内ファイルを基準にする
* 作業後に `docs/operations/todo.md` を更新する
* 作業後に `docs/operations/session-log.md` に追記する
* 仕様変更があれば `docs/requirements` または `docs/architecture` も更新する
* `todo.md` は履歴置き場にせず、現在位置とタスク一覧として更新する
* `session-log.md` に書いた履歴を `todo.md` に重複して残さない
* 同じ意味のタスクを `todo.md` の複数箇所に重複して書かない
* 完了タスクは消さなくてよいが、参照価値の低い完了タスクや重複した完了タスクを増やさない
* 最後に以下を報告する

  * 変更ファイル一覧
  * 実装したこと
  * 未実装事項
  * 次にやる候補
  * TODO 更新内容
  * Git判断

---

## 禁止事項

* 採用済み技術スタックを無断で変更しない
* Web UI へ勝手に置き換えない
* 通信方式を勝手に TCP / WebRTC / SRT / RIST に変更しない
* 音声統合を先に始めない
* 1080p / 60fps を標準前提で実装しない
* TODO や session-log を更新せずに終わらない
* 重要な判断を repo 内ファイルへ反映せずに放置しない
* 勝手に main へ push しない
