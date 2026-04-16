<!-- stream-sync/docs/architecture/protocol.md -->

# StreamSync Protocol Design

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

## 6. メッセージ種別定義

### 6.1 AuthRequest
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

#### 備考
- server はこのメッセージを受けて認証を行う
- 未認証状態では、映像フレームは受理しない

---

### 6.2 AuthResponse
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

### 6.3 Heartbeat
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

#### 備考
- server は heartbeat を受けて生存確認を更新する
- 一定時間 heartbeat が来なければ切断扱いにする

---

### 6.4 HeartbeatAck
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

### 6.5 VideoFrame
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

#### 備考
- payload は最も大きいデータとなる
- 必要に応じて fragmentation を後で導入する余地がある
- MVP ではまず「1メッセージ1フレーム」を基本として考える
- MTU を超える場合の扱いは実装段階で要検討

---

### 6.6 ClientStats
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

### 6.7 ServerNotice
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

## 7. 認証シーケンス

### 7.1 基本フロー
1. client 起動
2. client が `AuthRequest` を送信
3. server が token / client_id / protocol_version を検証
4. server が `AuthResponse` を返す
5. accepted = true の場合、client は映像送信と heartbeat を開始
6. accepted = false の場合、client は再試行または停止

### 7.2 server 側ルール
- 未認証送信元の `VideoFrame` は破棄する
- 認証済み送信元だけを受理対象にする
- protocol_version 不一致は拒否する
- heartbeat timeout で認証済み状態を解除する

---

## 8. 同期に必要な時刻情報

### 8.1 timestamp 単位
同期に使う protocol timestamp は、すべてマイクロ秒単位で扱う。

理由:
- 30fps / 60fps のフレーム間隔より十分細かく、丸め誤差を抑えやすい
- RTT / clock offset 推定に使いやすい
- `u64` ベースなら PoC / MVP でも扱いが単純で、将来の長時間運用でも十分な範囲を持てる

Rust 実装では `TimestampMicros` を使い、値の単位を型名で明示する。

### 8.2 client 側で送るべき時刻
- capture_timestamp
- send_timestamp

### 8.3 server 側で保持すべき時刻
- packet_received_at
- corrected_capture_time
- targetTime

### 8.4 方針
- client ごとの clock offset を推定する
- capture_timestamp を補正して共通時間軸へ変換する
- その上で targetTime に最も近いフレームを選ぶ

---

## 9. 受信後の server 内部処理

`VideoFrame` を受けた server はおおむね以下を行う。

1. 認証済み送信元か確認
2. protocol_version を確認
3. frame metadata を読み取る
4. packet_received_at を記録
5. capture_timestamp を共通時間軸へ補正
6. client ごとのバッファへ格納
7. targetTime に応じたフレーム選択候補にする

---

## 10. エラー処理方針

### 10.1 破棄するケース
- 未認証送信元
- protocol_version 不一致
- 必須フィールド不足
- payload_size 不正
- decode 不可能なフレーム
- 極端に古いフレーム

### 10.2 warn ログにするケース
- app_version 差異
- 一時的な heartbeat 遅延
- 軽微なフレーム欠損
- 短時間の jitter 増加

### 10.3 error ログにするケース
- 認証失敗
- デコード失敗の継続発生
- バッファ異常
- 同期不能状態の継続
- server 側内部処理異常

---

## 11. MVP でまだ固定しないもの

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

## 12. 今後の設計で詰める項目

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

## 13. 初期実装の優先順位

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
