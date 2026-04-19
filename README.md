<!-- stream-sync/README.md -->

# StreamSync

StreamSync は、4人のゲーム映像を受信し、共通の時間軸で同期したうえで、OBS に載せられる形で表示・手動切り替えできる多視点スイッチング配信ツールです。

主な用途は、Minecraft などのゲームを複数人で配信する際に、各参加者の映像をほぼ同じ時間基準でそろえ、4分割表示や単独表示を切り替えながら YouTube ライブ等へ配信することです。

---

## 目的

このプロジェクトの目的は、4人のゲーム映像を完全同期に近い形でそろえ、実際の配信で使える実用的なスイッチング基盤を作ることです。

特に以下を重視します。

- 4人分の映像を共通の時間軸でそろえること
- OBS に載せられる安定した表示を作ること
- 手動で視点を切り替えられること
- PoC から MVP へ段階的に育てられること
- 将来的に 1080p / 60fps へ拡張可能な構造を持つこと

---

## 現時点の技術方針

- 言語: Rust
- 映像処理: FFmpeg 系
- 通信: UDP 独自プロトコル
- コーデック: H.264
- UI: Rust 製の最小 GUI
- OBS 連携: switcher 専用表示ウィンドウを Window Capture
- 設定ファイル: TOML
- ログ: JSON Lines 形式の構造化ログ

---

## 標準品質と拡張方針

### 標準品質
- 720p / 30fps

### 将来拡張
- 1080p / 60fps へ上げられる設計にする
- 1080p / 60fps は条件付き上位運用モードとして扱う
- 実装は設定駆動で行い、固定値ベタ書きを避ける
- ハードウェアエンコードを強く前提にする

---

## ネットワーク構成

MVP 初期段階では、以下のスター構成を採用します。

- client 4台が中央 server に直接 UDP 送信する
- server が同期責任を持つ
- switcher は表示専用とする
- server と switcher は初期段階では同一 PC 運用でよい

構成イメージ:

    client1 ─┐
    client2 ─┼──> server ───> switcher ───> OBS
    client3 ─┤
    client4 ─┘

---

## 音声方針

MVP では音声を専用ツールへ統合せず、Discord を継続使用します。

- 配信用音声は 1 系統に固定する
- 視聴者向け映像はその基準音声に合わせて遅延調整する
- スイッチャー監視用音声は低遅延のまま扱う
- 音声統合は将来拡張とする

---

## 認証方針

MVP では以下の方式を採用します。

- 事前共有トークン方式
- clientId ホワイトリスト
- 認証済みクライアントの UDP パケットのみ受理

また、バージョン管理は以下の方針で行います。

- app_version と protocol_version を分離する
- protocol_version 不一致の client は接続拒否する
- app_version 差異は warn ログとして扱う

---

## リポジトリ構成方針

    stream-sync/
      apps/
        client/
        server/
        switcher/
      crates/
        protocol/
        config/
        logging/
        timebase/
        video-core/
        net-core/
        sync-core/
        ui-core/
      docs/
        requirements/
        architecture/
        operations/
      configs/
        examples/
      scripts/
      assets/

---

## ドキュメント

詳細は以下を参照します。

- `AGENTS.md`
- `docs/requirements/project-overview.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`
- `docs/operations/auth-roundtrip-manual-check.md`

今後、以下のドキュメントを追加予定です。

- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/architecture/decisions.md`

---

## 現在のフェーズ

現在は **仕様固定と土台づくりフェーズ** です。

完了済み:
- 目的定義
- PoC / MVP 条件定義
- 技術スタック決定
- 通信方式決定
- OBS 連携方式決定
- 音声暫定方針決定
- ネットワーク構成決定
- 認証方式決定
- ログ・計測方式決定
- バージョン管理方針決定
- ドキュメント初期セット作成
- Cargo workspace 初期化

次にやること:
- architecture ドキュメント作成
- 認証メッセージ / heartbeat メッセージ定義
- 共通型定義の着手

---

## MVP でやらないこと

- 音声を専用ツールに統合すること
- 自動スイッチング
- 発話検知による自動強調
- Minecraft イベント連動演出
- 録画保存やアーカイブ管理
- リプレイ機能
- クリップ自動生成
- 5人以上への一般化
- 視点数の動的増減対応
- 高度な権限管理
- 一般公開向けの完成品品質までの仕上げ
- OBS の高度な自動制御

---

## 開発方針

- 完全同期を最優先にする
- まずは動く最小構成を優先する
- PoC → MVP の順に段階的に進める
- 高画質化より同期安定性を優先する
- 変更時は TODO と session-log を更新する
