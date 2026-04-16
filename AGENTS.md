<!-- stream-sync/AGENTS.md -->

# AGENTS.md

## プロジェクト概要
このリポジトリは、4人のゲーム映像を受信し、共通の時間軸で同期したうえで、OBS に載せられる形で表示・手動切り替えできる多視点スイッチング配信ツールを開発するためのものです。

目的は「4人の映像を完全同期に近い形でそろえ、配信で使える実用的なスイッチング基盤を作ること」です。

---

## 最重要方針
- 完全同期を最優先にする
- MVP では 4 人固定で進める
- 映像同期基盤を主役にする
- 音声統合は MVP の対象外とする
- 最初の標準品質は 720p / 30fps とする
- 将来的に 1080p / 60fps へ拡張できる設計にする
- 1080p / 60fps は条件付き上位運用モードとして扱う
- OBS は最終出力先として扱う
- switcher の専用表示ウィンドウを OBS の Window Capture で取り込む
- 技術的な判断で未決定事項を勝手に変更しない
- 既存の決定事項を覆す変更は行わず、必要なら提案として残す

---

## 決定済み技術方針
- 言語: Rust
- 映像処理: FFmpeg 系
- 通信: UDP 独自プロトコル
- コーデック: H.264
- UI: Rust 製の最小 GUI
- OBS 連携: switcher 専用ウィンドウを Window Capture
- 設定ファイル: TOML
- ログ: JSON Lines 形式の構造化ログ
- 認証: 事前共有トークン方式 + clientId ホワイトリスト
- バージョン管理: app_version と protocol_version を分離

---

## ネットワーク構成
- client 4 台が中央 server に直接 UDP 送信するスター構成
- server が同期責任を持つ
- switcher は表示専用
- MVP 初期段階では server と switcher は同一 PC 運用でよい

---

## 音声方針
- MVP では Discord を継続使用する
- 配信用音声は 1 系統に固定する
- 視聴者向け映像は基準音声に合わせて遅延調整する
- スイッチャー監視用音声は低遅延のまま扱う
- 音声統合は将来拡張とする

---

## MVP でやらないこと
- 音声を専用ツールに統合すること
- 自動スイッチング
- 発話検知による自動強調
- Minecraft イベント連動演出
- 録画保存やアーカイブ管理
- リプレイ機能
- クリップ自動生成
- 5 人以上への一般化
- 視点数の動的増減対応
- 高度な権限管理
- 一般公開向けの完成品品質への仕上げ
- OBS の高度な自動制御
- WebRTC への変更
- Electron 中心構成への変更
- 音声経路の大幅変更

---

## ディレクトリ構成方針
- `apps/client`: 各配信メンバー用送信クライアント
- `apps/server`: 受信・同期・バッファ管理を行う中核
- `apps/switcher`: 表示・切り替え・OBS 出力用画面
- `crates/protocol`: 通信メッセージ・ヘッダ・共通型
- `crates/config`: TOML 設定読み込み
- `crates/logging`: 構造化ログ
- `crates/timebase`: RTT / offset / 時刻補正
- `crates/video-core`: 映像処理共通基盤
- `crates/net-core`: UDP 通信共通基盤
- `crates/sync-core`: ジッターバッファ / targetTime / 同期処理
- `crates/ui-core`: UI 共通処理
- `docs/requirements`: 要件整理
- `docs/architecture`: 設計資料
- `docs/operations`: TODO、作業ログ、運用メモ

---

## 実装時のルール
- 一度に大きく作り込みすぎず、PoC → MVP の順で進める
- まず動く最小構成を優先する
- 未決定事項を勝手に補完しすぎない
- プロトコル互換性に関わる変更は `protocol_version` を意識する
- アプリの変更と通信仕様の変更を区別する
- 高画質化より同期安定性を優先する
- リファクタ時は責務分離を意識する
- ログとメトリクスを軽視しない

---

## 引き継ぎ方針
- 会話履歴を共通記憶として前提にしない
- PC を変えた場合や新しい Codex セッションでは、過去会話が引き継がれない前提で動く
- 共通認識は必ず repo 内ファイルに残す
- 仕様、判断、進捗、未解決事項は `docs/` と `configs/` に反映する
- 最新の作業状態は `docs/operations/todo.md` と `docs/operations/session-log.md` を正とする
- 新しい作業を始める前に、最低限以下を読むこと
  - `AGENTS.md`
  - `README.md`
  - `docs/requirements/project-overview.md`
  - `docs/architecture/system-design.md`
  - `docs/architecture/protocol.md`
  - `docs/architecture/decisions.md`
  - `docs/operations/todo.md`
  - `docs/operations/session-log.md`
- 重要な判断を会話の中だけで終わらせない
- 次の Codex や別 PC の作業者が repo だけ見て再開できる状態を維持する

---

## Git運用ルール
- 作業ごとに Git checkpoint を意識する
- タスク完了時には、コミットに適した状態かを必ず報告する
- `cargo check` や `cargo fmt --check` が通っている場合は、コミット推奨を明示する
- docs と code の両方が更新された場合は、コミット候補を提示する
- push は人間確認後を前提とし、勝手に main へ push しない
- 最後の報告には、必要に応じて以下を含める
  - コミット推奨かどうか
  - push 推奨かどうか
  - 推奨コミットメッセージ

---

## Codex が作業時に必ず守ること
- 作業前にこの `AGENTS.md` と `docs/` を確認する
- 会話履歴ではなく repo 内ファイルを基準にする
- 作業後に `docs/operations/todo.md` を更新する
- 作業後に `docs/operations/session-log.md` に追記する
- 仕様変更があった場合は `docs/requirements` または `docs/architecture` も更新する
- 最後に以下を報告する
  - 変更ファイル一覧
  - 実装したこと
  - 未実装事項
  - 次にやる候補
  - TODO 更新内容
  - Git判断
    - コミット推奨: Yes / No
    - push 推奨: Yes / No
    - 推奨コミットメッセージ

---

## 今の優先順
1. protocol crate の基本型定義
2. 認証メッセージ / heartbeat メッセージの Rust 実装
3. 共通型のシリアライズ / デシリアライズ方針の整理
4. 1人送信・受信・表示の PoC

---

## 禁止事項
- 採用済み技術スタックを無断で変更しない
- Web UI へ勝手に置き換えない
- 通信方式を勝手に TCP / WebRTC / SRT / RIST に変更しない
- 音声統合を先に始めない
- 1080p / 60fps を標準前提で実装しない
- TODO や session-log を更新せずに終わらない
- 重要な判断を repo 内ファイルへ反映せずに放置しない
- 勝手に main へ push しない