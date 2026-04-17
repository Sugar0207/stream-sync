<!-- stream-sync/docs/architecture/system-design.md -->

# StreamSync System Design

## 1. 目的

StreamSync は、4人のゲーム映像を受信し、共通の時間軸で同期したうえで、OBS に載せられる形で表示・手動切り替えできる多視点スイッチング配信ツールです。

このドキュメントでは、MVP 段階におけるシステム全体の責務分担、コンポーネント構成、データフロー、および同期の考え方を整理します。

---

## 2. システム全体像

MVP では、以下の構成を採用します。

    client1 ─┐
    client2 ─┼──> server ───> switcher ───> OBS
    client3 ─┤
    client4 ─┘

- 各 client はゲーム画面をキャプチャし、エンコードし、UDP で server に送信する
- server は各 client から受けた映像を共通の時間軸にそろえる責任を持つ
- switcher は server から受けた同期済み映像を表示し、4分割表示や単独表示を切り替える
- OBS は switcher の表示ウィンドウを Window Capture で取り込む
- MVP では server と switcher は同一 PC 上で動作してよい

---

## 3. コンポーネントと責務

### 3.1 client
各配信メンバーの PC で動作する送信クライアント。

#### 主な責務
- 対象ゲーム画面のキャプチャ
- フレーム取得
- captureTimestamp 付与
- エンコード
- frameId / sendTimestamp / keyframe 情報付与
- server への UDP 送信
- 認証メッセージ送信
- heartbeat 送信
- stats 送信

#### 責務に含めないもの
- 複数視点の同期
- 配信用レイアウト制御
- OBS 連携
- 音声統合

---

### 3.2 server
受信、同期、時刻補正、バッファ制御の中核。

#### 主な責務
- client からの認証受付
- 認証済み送信元管理
- 映像フレーム受信
- RTT 計測
- clock offset 推定
- 共通時間軸への変換
- client ごとのジッターバッファ管理
- targetTime 計算
- 表示すべきフレーム選択
- switcher へ同期済み映像を渡す
- 状態 / メトリクス集計
- ログ出力

#### 責務に含めないもの
- 配信画面の最終UI制御
- OBS への直接出力
- 自動スイッチング
- 音声ミックス

---

### 3.3 switcher
表示と手動スイッチングを担当する UI アプリ。

#### 主な責務
- server から受けた映像の表示
- 4分割表示
- 単独表示
- メイン視点切り替え
- ホットキー操作
- 接続状態表示
- RTT / offset / 実効遅延 / fps / drop率 の表示
- 配信用表示ウィンドウの提供
- 必要に応じた不要UI非表示

#### 責務に含めないもの
- 同期の責任
- RTT 推定や offset 計算
- 映像送信
- 音声経路管理

---

### 3.4 OBS
配信用の最終出力先。

#### 主な責務
- switcher 専用表示ウィンドウの取り込み
- 配信シーン管理
- YouTube 等への配信出力

#### 責務に含めないもの
- 多視点同期
- 受信側バッファ制御
- 映像時間軸の補正

---

## 4. 音声の扱い

MVP では、音声は StreamSync の中核責務に含めない。

### 方針
- 通話は Discord を継続使用する
- 配信用音声は 1 系統に固定する
- 視聴者向け映像は、その基準音声に合わせて遅延調整する
- スイッチャー監視用音声は低遅延のまま扱う
- 音声統合は将来拡張とする

---

## 5. データフロー

### 5.1 映像フロー
1. client がゲーム画面をキャプチャする
2. client がフレームを取得する
3. client がフレームに metadata を付与する
4. client がフレームをエンコードする
5. client が UDP で server に送信する
6. server がフレームを受信する
7. server が時刻補正を適用する
8. server が client ごとのバッファへ格納する
9. server が targetTime に対して各 client の表示フレームを選ぶ
10. switcher が同期済み映像を表示する
11. OBS が switcher の専用表示ウィンドウを取り込む

### 5.2 制御フロー
1. client 起動
2. client が認証メッセージ送信
3. server が token / client_id / protocol_version を確認
4. server が認証済み扱いにする
5. client が heartbeat を定期送信
6. server が timeout を監視
7. 切断または timeout 時に認証済み状態を解除する

### 5.3 net-core / protocol receive boundary

PoC / MVP 初期では、UDP socket 実装に進む前に、受信 packet bytes から decode 済み message を app / server handler へ渡す境界だけを固定する。

受信時の責務分担:

- `crates/net-core`
  - 将来の UDP 受信層から生の packet bytes と送信元 address を受け取る。
  - packet bytes と送信元 metadata を `InboundPacket` として保持する。
  - `protocol` crate の fixed header decode、protocol_version 検証、message_type dispatch を順番に呼ぶ。
  - decode に成功した場合は、送信元 metadata と `ProtocolMessage` を `DecodedInboundPacket` として app / server handler 側へ渡す。
  - decode error は送信元 metadata と合わせて返す。
- `crates/protocol`
  - 16 byte fixed header を decode し、payload slice の境界を決める。
  - `DecodeContext.expected_protocol_version` と fixed header の `protocol_version` を比較する。
  - `message_type` に応じて payload decoder を選び、payload bytes を message 型へ変換する。
  - UDP socket、認証済み送信元管理、handler 実行、ログ出力判断は持たない。
- app / server handler
  - decode 済み message と送信元 metadata を受け取る。
  - 認証、client whitelist、heartbeat timeout、VideoFrame 受理、同期バッファ投入などの app 状態変更を行う。
  - protocol error を接続拒否、packet 破棄、ログ出力へ変換する。

呼び出し順序:

1. UDP 受信層が raw packet bytes と送信元 address を得る。
2. `net-core` が `InboundPacket` と `DecodeContext` を受け取る。
3. `net-core` が `protocol::decode_fixed_header` を呼ぶ。
4. `net-core` が `protocol::validate_protocol_version` を呼ぶ。
5. `net-core` が `protocol::decode_payload_by_message_type` を呼ぶ。
6. `net-core` が `DecodedInboundPacket` を app / server handler 側へ返す。
7. app / server handler が認証、同期、buffer、表示などの処理を行う。

この境界は packet decode の接続点を決めるためのものであり、実際の UDP socket loop、送信処理、server / client / switcher handler 実装はまだ行わない。

### 5.4 server handler boundary

server 側 handler は、`net-core` から `DecodedInboundPacket` を受け取った後の app 境界とする。`protocol` と `net-core` は packet を decode 済み message に変換するまでを担当し、server は decode 済み message の種類を見て、認証、heartbeat、video frame の各処理へ分岐する。

server 側の入力:
- `DecodedInboundPacket.source`
  - raw UDP packet の送信元 metadata。
  - 認証済み送信元管理やログ付与の材料にする。
- `DecodedInboundPacket.message`
  - `protocol` が復元した `ProtocolMessage`。
  - server は message variant と `message_type` 相当の意味を見て処理方針を選ぶ。

server 側の分岐:
- `AuthRequest`
  - 認証処理へ渡す。
  - shared token 検証、client whitelist 照合、認証済み送信元登録、AuthResponse 生成判断は server 側の責務とする。
- `Heartbeat`
  - heartbeat 処理へ渡す。
  - 生存確認更新、timeout 管理、RTT / offset 計測材料としての扱い、HeartbeatAck 生成判断は server 側の責務とする。
- `VideoFrame`
  - video frame 処理へ渡す。
  - 認証済み client かどうかの確認、古い frame の破棄、時刻補正、ジッターバッファ投入、同期処理は server / sync 側の責務とする。
- その他 message
  - 現時点では server inbound の対象外として扱う。
  - `AuthResponse`, `HeartbeatAck`, `ClientStats`, `ServerNotice` の decode / encode 本実装や扱いは別タスクで決める。

実装上は `apps/server` に `ServerInboundRouter` を置き、`DecodedInboundPacket` を `ServerInboundRoute` に分類する。これは handler 本体ではなく、server handler へ渡す境界名を固定するための placeholder とする。認証成功 / 失敗判定、heartbeat 管理、video frame 処理本体はまだ実装しない。

### 5.5 server UDP receive loop boundary

server 側の UDP 受信 loop は、将来の UDP socket 実装で packet bytes と送信元情報を受け取った後、decode と server route へ渡すまでを担当する。現時点では socket の `bind`、`recv_from`、非同期 runtime、packet 受信本体は実装しない。

最小処理順:

1. UDP socket 層が packet bytes を受信する。
2. UDP socket 層が送信元 address を取得する。
3. server receive loop が送信元 address を `PacketSource` に変換する。
4. server receive loop が packet bytes と `PacketSource` から `InboundPacket` を生成する。
5. server receive loop が `InboundPacketDecoder` を呼ぶ。
6. `InboundPacketDecoder` が fixed header decode、`protocol_version` check、payload decode を行い、成功時に `DecodedInboundPacket` を返す。
7. server receive loop が `DecodedInboundPacket` を `ServerInboundRouter` に渡す。
8. `ServerInboundRouter` が `ServerInboundRoute` を返す。
9. server handler 本体が route ごとの処理を行う。

decode error / protocol error の扱い:

- `UnsupportedProtocolVersion`
  - protocol version mismatch として分類する。
  - 初期実装では packet を破棄し、将来 `AuthResponse` / `ServerNotice` の encode が入った段階で拒否通知を検討する。
- `PayloadDecodeNotImplemented`
  - server inbound として未対応の message として分類する。
  - 初期実装では packet を破棄し、warn ログ候補とする。
- その他の `ProtocolError`
  - malformed packet として分類する。
  - 初期実装では packet を破棄する。

実装上は `apps/server` に `ServerReceiveLoopStep` を置く。これは 1 packet 分の placeholder であり、既に受信済みの packet bytes と `PacketSource` を受け取り、`InboundPacketDecoder` と `ServerInboundRouter` を順番に呼ぶ。成功時は `ServerReceiveLoopOutcome::Routed`、失敗時は `ServerReceiveLoopOutcome::Rejected` を返す。実際の socket loop、ログ出力、認証判定、heartbeat 管理、video frame 処理本体はまだ持たない。

---

## 6. 同期の考え方

### 6.1 基本方針
完全同期を目標にするが、MVP では以下の考え方で進める。

- client ごとの captureTimestamp を基準にする
- server が RTT と clock offset を推定する
- 受信後、各フレームを共通時間軸へ変換する
- 表示時は targetTime を基準に各 client のフレームを選ぶ
- 「早く届いたものをそのまま出す」のではなく、「同じ時間のものを出す」ことを優先する

### 6.2 targetTime
targetTime は、現在時刻から固定遅延量を引いた「表示対象の共通時刻」とする。

例:
- 現在時刻が T
- 固定遅延が D
- 表示対象は `T - D`

各 client について、
- `T - D` に最も近い共通時刻フレームを選ぶ

### 6.3 バッファ
server は client ごとにバッファを持つ。

バッファの役割:
- ジッター吸収
- 遅延揺れの平準化
- targetTime に合わせたフレーム選択

### 6.4 欠損時
表示対象に適切なフレームが無い場合は、次のいずれかを取る。

- 前フレーム保持
- 切断中 / 欠落中表示
- 再接続待ち表示

詳細な優先順位は protocol / runtime 仕様で定義する。

---

## 7. 状態管理の考え方

各 client はおおむね以下の状態を取る。

- 未接続
- 認証待ち
- 認証済み
- 映像受信中
- 同期待ち
- 安定表示中
- 遅延悪化
- 切断
- 再接続中

switcher では最低限、以下を表示できるようにする。

- 接続状態
- RTT
- offset
- 実効遅延
- fps
- drop率
- buffer 状態

---

## 8. バージョン管理

### app_version
- 各アプリの配布物バージョン
- UI変更や内部改善も含む

### protocol_version
- 通信仕様のバージョン
- 認証メッセージ、フレームヘッダ、heartbeat、stats などの互換性を表す

### MVP 方針
- protocol_version 不一致は接続拒否
- app_version 差異は warn ログ

---

## 9. ログとメトリクス

### ログ
- JSON Lines 形式
- client / server / switcher ごとに出力
- run_id と client_id を付与する

### メトリクス
switcher 上でリアルタイム表示する。

最低限:
- 接続状態
- RTT
- offset
- 実効遅延
- fps
- drop率
- buffer 状態

---

## 10. 初期実装の優先順位

### フェーズ1
- workspace 初期化
- docs 初版作成
- 共通型定義
- 認証メッセージ / heartbeat メッセージ定義

### フェーズ2
- 1人送信 / 受信 / 表示
- timestamp 付与
- RTT / offset 推定

### フェーズ3
- 2人同期表示
- 4人同期表示
- switcher UI
- OBS 取り込み確認

### フェーズ4
- 長時間試験
- ログ可視化
- 異常系対応
- MVP 安定化

---

## 11. 将来拡張
- 音声統合
- 発話検知
- 自動スイッチング
- Minecraft イベント連動
- 録画 / リプレイ / クリップ生成
- 5人以上対応
- 視点数動的増減
- 1080p / 60fps の本格運用
