<!-- stream-sync/docs/operations/todo.md -->

# StreamSync TODO

最終更新: 2026-04-17

このファイルは「現在どこまで終わっていて、次に何をやるか」を確認するための TODO です。時系列の作業履歴は `docs/operations/session-log.md` を正とします。

## 運用ルール
- このファイルを StreamSync の最新版 TODO として扱う
- 項目の状態が変わったら必ず更新する
- 大きな仕様変更があれば関連する `docs/requirements` や `docs/architecture` も更新する
- Codex 作業後は、この TODO と `docs/operations/session-log.md` を更新する
- 完了済みの細かい作業履歴はここに積まず、session-log に寄せる

---

## 現在位置
- 仕様固定と土台作りは概ね完了
- Cargo workspace と `apps/*` / `crates/*` の初期 scaffold は完了
- `crates/protocol` の基本型、主要 message 型、timestamp 型、fixed header decode、`AuthRequest` / `Heartbeat` / `VideoFrame` payload decode は完了
- `crates/net-core` の inbound decode 境界、outbound packet / queue 境界、protocol encoder 呼び出し境界は placeholder として完了
- `apps/server` の inbound router、UDP receive loop step、auth handler boundary、AuthResponse response boundary、outbound queue handoff は placeholder として完了
- 実ネットワーク送受信、実認証、encode 本実装、時刻同期本体、映像受信・復号・表示、switcher UI は未実装
- 次の中心は `AuthResponse` encode、HeartbeatAck / outbound encode 周辺、UDP socket 送受信、server 側の認証本体

---

## 決定済み方針
- [x] プロジェクト名は `StreamSync`
- [x] リポジトリ名 / ルートフォルダ名は `stream-sync`
- [x] MVP は 4 人固定
- [x] 完全同期に近い映像同期基盤を最優先する
- [x] 初期標準品質は 720p / 30fps
- [x] 1080p / 60fps は条件付き上位運用モード
- [x] 言語は Rust
- [x] 映像処理は FFmpeg 系
- [x] 通信は UDP 独自プロトコル
- [x] コーデックは H.264
- [x] UI は Rust 製の最小 GUI
- [x] OBS 連携は switcher 専用ウィンドウの Window Capture
- [x] 設定ファイルは TOML
- [x] ログは JSON Lines 形式の構造化ログ
- [x] 認証は事前共有トークン方式 + clientId ホワイトリスト
- [x] `app_version` と `protocol_version` は分離管理
- [x] MVP の音声は Discord 継続使用
- [x] client 4 台が中央 server に直接 UDP 送信するスター構成
- [x] server が同期責任を持つ
- [x] switcher は表示専用
- [x] MVP 初期段階では server と switcher は同一 PC 運用でよい

---

## 直近でやること
1. `AuthResponse` encode の最小実装を追加する
2. protocol encoder の fixed header / payload byte 生成を実装する
3. `HeartbeatAck` の payload layout / encode 方針を整理する
4. UDP socket 送信前の send error / log event 方針を整理する
5. outbound queue の最小実処理を設計する
6. client whitelist 読み込みと token 検証の設定入力境界を設計する
7. server 側の認証成功 / 失敗判定を実装する
8. UDP socket 受信 / 送信本体の実装に進む

---

## 仕様 / 設計
- [x] `docs/requirements/project-overview.md` を作成する
- [x] `docs/architecture/system-design.md` を作成する
- [x] `docs/architecture/protocol.md` を作成する
- [x] `docs/architecture/decisions.md` を作成する
- [x] README を作成する
- [x] PoC 完了条件を定義する
- [x] MVP 完了条件を定義する
- [x] MVP でやらないことを定義する
- [x] 将来拡張項目を整理する
- [x] コンポーネントごとの責務を定義する
- [x] protocol / net-core / server の受信 decode 境界を整理する
- [x] server inbound handler 境界を整理する
- [x] server UDP receive loop 境界を整理する
- [x] server auth handler 境界を整理する
- [x] AuthResponse 生成 / 送信境界を整理する
- [x] outbound packet / queue 境界を整理する
- [x] net send layer / protocol encoder 境界を整理する
- [ ] 状態遷移を詳細化する
- [ ] 異常時の挙動を実装レベルに落とす
- [ ] ログイベント仕様を詳細化する
- [ ] 配信時の運用方針を手順書へ落とす
- [ ] バージョン互換性ルールを実装と運用手順へ反映する

---

## protocol / wire format
- [x] 共通型定義を作る
- [x] `ClientId`, `RunId`, `AppVersion`, `ProtocolVersion` を定義する
- [x] `TimestampMicros` を定義し、timestamp 単位をマイクロ秒に整理する
- [x] `AuthRequest` / `AuthResponse` の Rust 型を定義する
- [x] `Heartbeat` / `HeartbeatAck` の Rust 型を定義する
- [x] `VideoFrame` の最小構造を定義する
- [x] `ClientStats` / `ServerNotice` の最小型を定義する
- [x] `MessageType`, `Codec`, `NoticeType`, auth reason code を定義する
- [x] PoC / MVP 初期の最小 wire format を 16 byte fixed header として整理する
- [x] 数値フィールドを little-endian とする方針を整理する
- [x] `message_type`, `header_length`, `protocol_version`, `payload_length`, `flags`, `reserved` を fixed header に定義する
- [x] fixed header decode を実装する
- [x] `protocol_version` 期待値チェックを実装する
- [x] payload decoder dispatch helper を実装する
- [x] `AuthRequest` payload byte layout と decode を実装する
- [x] `Heartbeat` payload byte layout と decode を実装する
- [x] `VideoFrame` payload byte layout と decode を実装する
- [x] `AuthResponse` payload byte layout と encode input boundary を整理する
- [x] `ProtocolMessage::message_type()` と `ProtocolMessageEncoderBoundary` placeholder を追加する
- [ ] `AuthResponse` encode 本実装を行う
- [ ] fixed header encode 本実装を行う
- [ ] message ごとの payload encode 本実装を行う
- [ ] `HeartbeatAck` payload layout / encode 方針を決める
- [ ] `ClientStats` / `ServerNotice` の payload layout と decode / encode 方針を決める
- [ ] payload fragmentation の要否と方式を決める
- [ ] 再送制御 / 暗号化は MVP 初期で扱うか保留するか明記する

---

## net-core / server 境界
- [x] `InboundPacket` / `PacketSource` / `InboundPacketDecoder` / `DecodedInboundPacket` / `NetDecodeError` を追加する
- [x] raw packet bytes と送信元 metadata を protocol decode 結果へ変換する境界を定義する
- [x] server 側 `ServerInboundRouter` / `ServerInboundRoute` placeholder を追加する
- [x] `AuthRequest` / `Heartbeat` / `VideoFrame` の server route 分類を定義する
- [x] `ServerReceiveLoopStep` / `ServerReceiveLoopOutcome` / `ServerRejectedPacket` placeholder を追加する
- [x] decode error / protocol error の分類方針を定義する
- [x] `OutboundPacket` / `OutboundQueueItem` / `OutboundPacketQueueBoundary` placeholder を追加する
- [x] `OutboundEncodeRequest` / `EncodedOutboundPacket` / `OutboundPacketEncoderBoundary` / `NetEncodeError` placeholder を追加する
- [x] server 側 `ServerOutboundQueueBoundary` placeholder を追加する
- [ ] UDP socket の bind / receive / send 本実装を行う
- [ ] packet 受信本体を実装する
- [ ] packet 送信本体を実装する
- [ ] receive loop のログ出力を実装する
- [ ] outbound queue の実処理を実装する
- [ ] send error の分類とログ方針を実装する
- [ ] async runtime 導入方針を決める

---

## 認証まわり
- [x] 認証方式を事前共有トークン + clientId ホワイトリストに決定する
- [x] `AuthRequest` / `AuthResponse` 型を定義する
- [x] `AuthRequest` payload decode を実装する
- [x] `AuthResponse` 生成 / 送信境界を定義する
- [x] `ServerAuthHandlerBoundary` / `ServerAuthCheck` / `ServerAuthBoundaryError` placeholder を追加する
- [x] `ServerAuthDecision` / `ServerAuthResponseBoundary` / `ServerOutboundAuthResponse` placeholder を追加する
- [x] 認証判定入力として `shared_token` / `client_id` / `protocol_version` / `app_version` を参照できる形を定義する
- [ ] client whitelist 読み込みを実装する
- [ ] token 検証を実装する
- [ ] 認証成功 / 失敗判定を実装する
- [ ] 認証済み送信元の登録 / 管理を実装する
- [ ] 未認証送信元の `VideoFrame` 破棄を実装する
- [ ] `protocol_version` 不一致時の接続拒否を server 側に実装する
- [ ] `app_version` 差異時の warn ログを実装する
- [ ] 認証期限切れ / 再認証方針を実装する
- [ ] ログに secret を残さない処理を実装する

---

## heartbeat / 時刻同期
- [x] `Heartbeat` / `HeartbeatAck` 型を定義する
- [x] `Heartbeat` payload decode を実装する
- [x] timestamp 単位をマイクロ秒に整理する
- [ ] `HeartbeatAck` payload layout / encode 方針を決める
- [ ] heartbeat 送信処理を client 側に実装する
- [ ] heartbeat 受信処理を server 側に実装する
- [ ] heartbeat timeout 管理を実装する
- [ ] RTT 計測を実装する
- [ ] clock offset 推定を実装する
- [ ] offset 平滑化を実装する
- [ ] 補正後 timestamp へ変換する処理を実装する
- [ ] targetTime 計算へ接続する
- [ ] 同期精度をログに出す

---

## video frame / 映像受信
- [x] `VideoFrame` の最小構造を定義する
- [x] H.264 payload を `Vec<u8>` として保持する方針を定義する
- [x] `VideoFrame` payload decode を実装する
- [x] `payload_size` と実際の H.264 byte 数の整合確認を実装する
- [x] 不正 bool / reserved / codec / payload 長の最小 error を実装する
- [ ] client 側で frame metadata を付与する
- [ ] client 側で H.264 encode を行う
- [ ] `VideoFrame` encode を実装する
- [ ] UDP で frame を送信する
- [ ] server 側で認証済み client の frame だけ受理する
- [ ] server 側で client ごとの受信キューを作る
- [ ] 不正 frame 破棄を実装する
- [ ] 受信遅延と drop を計測する
- [ ] sync-core のジッターバッファへ投入する
- [ ] frame 欠落時の代替表示方針を決める

---

## client 側
- [ ] クライアント起動処理を作る
- [ ] TOML 設定読み込み処理を作る
- [ ] `client_id` / `shared_token` を設定から読み込む
- [ ] `run_id` を受け取る、または生成する
- [ ] `app_version` / `protocol_version` を送信する
- [ ] 認証メッセージ送信処理を作る
- [ ] heartbeat 送信処理を作る
- [ ] 画面キャプチャに成功する
- [ ] Minecraft ウィンドウの取得確認をする
- [ ] frame id / captureTimestamp / sendTimestamp を付与する
- [ ] H.264 encode 処理を実装する
- [ ] ハードウェア encode 優先処理を実装する
- [ ] ソフトウェア encode fallback を実装する
- [ ] 720p / 30fps を初期値にする
- [ ] 1080p / 60fps を将来有効化できる構造にする
- [ ] UDP 送信処理を実装する
- [ ] stats 送信処理を実装する
- [ ] 切断 / 再接続処理を実装する

---

## switcher / 表示 / OBS
- [x] OBS 連携方法を Window Capture に決定する
- [x] switcher は表示専用とする方針を決定する
- [x] 4 分割表示と単独表示の切り替えを MVP 対象にする
- [ ] 1 視点の復号に成功する
- [ ] 1 視点の表示に成功する
- [ ] 2x2 の 4 分割レイアウトを作る
- [ ] 単独表示モードを作る
- [ ] クリック / ダブルクリック / ホットキー切り替えを実装する
- [ ] 現在メイン視点を強調表示する
- [ ] 切断 / 準備中 / 復号不能 / frame 不足表示を作る
- [ ] client ごとの接続状態 / RTT / offset / 実効遅延 / fps / drop 率を表示する
- [ ] buffer 状態表示を作る
- [ ] デバッグ表示 ON/OFF を作る
- [ ] 配信用表示と操作用表示を分けるか決める
- [ ] OBS で映像表示に成功する
- [ ] 720p / 30fps で表示確認する
- [ ] 長時間表示でも安定することを確認する
- [ ] 不要 UI 非表示モードを作る

---

## ログ / 計測
- [x] ログ方針を JSON Lines 形式に決定する
- [x] `run_id` / `client_id` で追跡可能にする方針を決定する
- [x] switcher UI 上のリアルタイム簡易メトリクス表示方針を決定する
- [ ] ログイベント型を定義する
- [ ] JSON Lines 形式でログ出力する
- [ ] `run_id` / `client_id` を各ログに付与する
- [ ] 接続 / 切断 / 再接続ログを実装する
- [ ] 受信数 / drop / 同期誤差ログを実装する
- [ ] protocol error / malformed packet / auth failure ログを実装する
- [ ] receive loop / send error のログを実装する
- [ ] `app_version` / `protocol_version` を接続時ログへ記録する
- [ ] server 全体メトリクス表示を作る
- [ ] 720p / 30fps と 1080p / 60fps の負荷測定項目を整理する

---

## PoC に必要な最小ライン
1. `AuthResponse` encode と fixed header encode が動く
2. UDP socket の receive / send が最小で動く
3. client が `AuthRequest` を送り、server が `AuthResponse` を返せる
4. client が `Heartbeat` を送り、server が RTT / offset 推定に使える時刻情報を返せる
5. client が 1 視点の H.264 `VideoFrame` を送れる
6. server が 1 視点の frame を受信し、破棄 / 受理を判定できる
7. switcher が 1 視点を復号・表示できる
8. 2 視点で targetTime による簡易同期表示を確認できる
9. 4 視点で 2x2 表示を確認できる
10. OBS Window Capture で switcher 表示を取り込める

---

## 検証 / テスト
- [x] 過去作業で `cargo fmt --check` が通ることを確認した
- [x] 過去作業で `cargo check --workspace` が通ることを確認した
- [ ] `AuthResponse` encode の単体テストを追加する
- [ ] fixed header encode / decode roundtrip test を追加する
- [ ] protocol error の単体テストを拡充する
- [ ] net-core inbound / outbound 境界の単体テストを追加する
- [ ] server inbound route の単体テストを追加する
- [ ] 疑似 client を作る
- [ ] 人工遅延 / jitter / frame 欠損テストを作る
- [ ] 1 人 PoC を 30 分連続確認する
- [ ] 2 人同期表示を確認する
- [ ] 4 人同期表示を確認する
- [ ] Minecraft 実機で確認する

---

## 後回し項目
- [ ] 音声統合
- [ ] 自動スイッチング
- [ ] 発話検知による自動強調
- [ ] Minecraft イベント連動演出
- [ ] 録画保存 / アーカイブ管理
- [ ] リプレイ機能
- [ ] クリップ自動生成
- [ ] 5 人以上への一般化
- [ ] 視点数の動的増減対応
- [ ] 高度な権限管理
- [ ] 一般公開向けの完成品品質への仕上げ
- [ ] OBS の高度な自動制御
- [ ] OBS WebSocket 連携
- [ ] WebRTC / TCP / SRT / RIST への変更
- [ ] Electron 中心構成への変更
- [ ] 本格的な retry / fragmentation / encryption

---

## 優先順ロードマップ

### フェーズ1: 仕様固定と土台
- [x] 目的 / PoC / MVP / 非対象範囲定義
- [x] 技術スタック / 通信 / codec / OBS / 音声 / 認証 / ログ方針決定
- [x] Cargo workspace 初期化
- [x] protocol crate の基本型定義
- [x] wire format 初期設計
- [x] decode 境界と主要 inbound payload decode
- [x] net-core / server の境界 placeholder

### フェーズ2: protocol encode と UDP PoC 準備
- [ ] `AuthResponse` encode
- [ ] fixed header encode
- [ ] `HeartbeatAck` encode 方針
- [ ] UDP receive / send 最小実装
- [ ] server auth decision 最小実装
- [ ] receive / send ログ最小実装

### フェーズ3: 1 人送信・受信・表示 PoC
- [ ] client capture / encode
- [ ] `VideoFrame` encode / UDP send
- [ ] server frame receive / queue
- [ ] switcher decode / single view display
- [ ] 30 分連続確認

### フェーズ4: 2 人 / 4 人同期 PoC
- [ ] RTT / offset 推定
- [ ] ジッターバッファ
- [ ] targetTime frame selection
- [ ] 2 人同期表示
- [ ] 4 人 2x2 表示
- [ ] OBS 取り込み確認

### フェーズ5: MVP 安定化
- [ ] switcher UI
- [ ] 認証 / reconnect / timeout
- [ ] 異常系対応
- [ ] ログ可視化
- [ ] 長時間試験
- [ ] 運用手順整備
