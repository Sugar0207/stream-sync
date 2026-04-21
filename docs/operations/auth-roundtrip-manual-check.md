<!-- stream-sync/docs/operations/auth-roundtrip-manual-check.md -->

# One-shot Auth Round Trip Manual Check

この手順は、server / client の one-shot auth PoC が UDP で 1 往復分つながることを手動確認するためのものです。

主対象は `AuthRequest` を 1 回送り、server が `AuthResponse` を 1 回返すところまでです。追加の auth-then-heartbeat 手順では、同じ UDP socket で accepted auth 後に `Heartbeat` を 1 回送り、server が `HeartbeatAck` を 1 回返すところまで確認します。継続 loop、async runtime、continuous heartbeat、video frame、retry、fragmentation、encryption は含みません。

## 使用する config

- server: `configs/examples/server.example.toml`
- server with env token: `configs/examples/server.env-token.example.toml`
- accepted path client: `configs/examples/client.accepted.example.toml`
- rejected path client: `configs/examples/client.example.toml`

`configs/examples/client.accepted.example.toml` は、server 側 `player1.shared_token = "replace-with-shared-token-1"` と同じ token を使う accepted path 用です。accepted path を見る場合は、この config を使います。

`configs/examples/client.example.toml` は、client 側 `shared_token = "replace-with-shared-token"`、server 側 `player1.shared_token = "replace-with-shared-token-1"` で値が異なるため、rejected path の確認に使えます。この場合も round trip 自体は確認できますが、server の auth decision は `accepted=false`, `reason_code=InvalidToken` になる想定です。

`configs/examples/server.env-token.example.toml` は、server 側 token を `shared_token_env` で指定する確認用です。この config は `player1` から `player4` までの token reference を持ち、現在の resolver は config 内の token reference を auth decision 前にまとめて解決します。そのため accepted path では server を起動するターミナルに 4 つすべての値を設定します。

```powershell
$env:STREAMSYNC_PLAYER1_TOKEN = "replace-with-shared-token-1"
$env:STREAMSYNC_PLAYER2_TOKEN = "replace-with-shared-token-2"
$env:STREAMSYNC_PLAYER3_TOKEN = "replace-with-shared-token-3"
$env:STREAMSYNC_PLAYER4_TOKEN = "replace-with-shared-token-4"
```

`player1` の値は client accepted config の `[client].shared_token` と一致させます。`player2` 以降を使う場合は、対応する `STREAMSYNC_PLAYER*_TOKEN` と client config の `client_id` / `shared_token` を合わせます。

## 手順

ターミナルを 2 つ開き、どちらも repo root で実行します。

### 1. server を起動する

```powershell
cargo run -p stream-sync-server -- --auth-response-poc-once configs/examples/server.example.toml
```

server は `configs/examples/server.example.toml` の `[server].bind_host` / `[server].bind_port` を使って UDP socket を bind し、1 packet だけ待ち受けます。デフォルトでは `0.0.0.0:5000` です。

このコマンドは client から 1 packet が届くまで戻りません。先に server を起動してから client を起動します。

### 2. accepted path client を起動する

別ターミナルで実行します。

```powershell
cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml
```

client は `configs/examples/client.accepted.example.toml` の `[client].server_host` / `[client].server_port` を destination として解決し、`AuthRequest` を fixed header + payload bytes に encode して 1 回だけ UDP send します。デフォルトでは `127.0.0.1:5000` に送ります。

rejected path を確認したい場合だけ、client command の config path を `configs/examples/client.example.toml` に置き換えます。

## `shared_token_env` accepted path 手順

server 側 config に token material を直接置かず、環境変数から解決する経路を確認する手順です。

### 1. server 用の環境変数を設定する

server を起動するターミナルで実行します。

```powershell
$env:STREAMSYNC_PLAYER1_TOKEN = "replace-with-shared-token-1"
$env:STREAMSYNC_PLAYER2_TOKEN = "replace-with-shared-token-2"
$env:STREAMSYNC_PLAYER3_TOKEN = "replace-with-shared-token-3"
$env:STREAMSYNC_PLAYER4_TOKEN = "replace-with-shared-token-4"
```

これらの環境変数は server process だけが読みます。client は従来通り `configs/examples/client.accepted.example.toml` の `shared_token` を `AuthRequest` に載せます。

### 2. env-token server を起動する

```powershell
cargo run -p stream-sync-server -- --auth-response-poc-once configs/examples/server.env-token.example.toml
```

server は `configs/examples/server.env-token.example.toml` の `player1.shared_token_env = "STREAMSYNC_PLAYER1_TOKEN"` を読み、`ServerSecretResolverBoundary` で環境変数から token material を解決してから auth decision に進みます。

### 3. accepted path client を起動する

別ターミナルで実行します。

```powershell
cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml
```

### 4. env-token 成功時の見方

server 側 stdout は inline token の accepted path と同じく、`accepted=true reason_code=Ok` になる想定です。

```text
auth response PoC handled one packet on 0.0.0.0:5000 and sent <bytes> bytes; client_id=player1 run_id=streamsync-dev-session accepted=true reason_code=Ok
```

server 側 stderr には auth result JSON Lines が 1 行出ます。成功時は `event_name` が `server.auth_result`、`accepted` が `true`、`reason_code` が `Ok` になります。

```json
{"event_name":"server.auth_result","run_id":"streamsync-dev-session","client_id":"player1","source":"127.0.0.1:<port>","accepted":true,"reason_code":"Ok","message":null,"app_version":"0.1.0","protocol_version":1,"timestamp":<timestamp>,"expected_protocol_version":null}
```

client 側 stdout は通常の accepted path と同じく、1 回送信した byte 数と destination を表示します。

### 5. env-token 失敗時の見方

- `STREAMSYNC_PLAYER1_TOKEN` など、config に含まれるいずれかの `STREAMSYNC_PLAYER*_TOKEN` を未設定にした場合
  - server は `accepted=false reason_code=InternalError` を返します。
  - stderr の auth result JSON Lines は `accepted=false`, `reason_code=InternalError`, `message="token secret environment variable is missing"` になります。
- `STREAMSYNC_PLAYER1_TOKEN` など、config に含まれるいずれかの `STREAMSYNC_PLAYER*_TOKEN` を空文字または空白だけにした場合
  - server は `accepted=false reason_code=InternalError` を返します。
  - stderr の auth result JSON Lines は `message="token secret environment variable is empty"` になります。
- `STREAMSYNC_PLAYER1_TOKEN` が client の `shared_token` と異なる場合
  - round trip は成立しますが、server は `accepted=false reason_code=InvalidToken` を返します。
  - stderr の auth result JSON Lines は `message="invalid shared_token"` になります。
- server が戻らない場合
  - inline token 手順と同じく、client destination、firewall、bind port、server 起動順を確認します。

## `--receive-send-once` accepted path 手動確認結果

completed one-iteration runtime の CLI / config 接続を確認する手順です。

この経路は `ServerReceiveSendOneIterationLauncher` から
`ServerControllerReceiveSendRuntimeBoundary` を 1 回だけ呼び、accepted
auth request を receive body -> dispatch -> side effect apply -> outbound
queue collection -> one-item send runtime へ渡します。継続 receive/send loop、
retry / requeue、file sink open、process-wide logger は含みません。

### 実行コマンド

server:

```powershell
cargo run -p stream-sync-server -- --receive-send-once configs/examples/server.example.toml
```

client:

```powershell
cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml
```

### 2026-04-21 実行結果

結果: 成功。

server stdout:

```text
receive/send one-iteration runtime handled one packet on 0.0.0.0:5000; sent_bytes=55 observation_state=BodyIterationCompleted observation_action=YieldToCaller
```

server stderr の要点:

```json
{"event_name":"server.receive_loop","source":"127.0.0.1:<client-port>","outcome":"Accepted","packet_len":96,"message_type":"AuthRequest","client_id":"player1","rejection_reason":null,"timestamp":<timestamp>}
{"event_name":"server.auth_result","run_id":"streamsync-dev-session","client_id":"player1","source":"127.0.0.1:<client-port>","accepted":true,"reason_code":"Ok","message":null,"app_version":"0.1.0","protocol_version":1,"timestamp":<timestamp>,"expected_protocol_version":null}
{"event_name":"server.send","outcome":"Success","run_id":"streamsync-dev-session","client_id":"player1","destination":"127.0.0.1:<client-port>","message_type":"AuthResponse","stage":"SocketSend","encoded_len":55,"bytes_sent":55,"failure":null,"disposition":null,"timestamp":<timestamp>}
```

client stdout:

```text
auth request PoC sent 96 bytes to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1 accepted=true reason_code=Ok message=null expected_protocol_version=null
```

client stderr は cargo の build / run 表示のみ。

この確認で、accepted auth request が `--receive-send-once` 入口から 1 回の
controller receive/send runtime に入り、server 側で `AuthResponse` 55 bytes
を UDP send し、`server.send` success observation を出力したことを確認した。client 側の
`--auth-request-poc-once` は同じ UDP socket で `AuthResponse` を 1 回だけ受信し、
stdout に `accepted` / `reason_code` / `message` / `expected_protocol_version` を表示する。

## `--receive-send-twice` auth-then-heartbeat 手動確認手順

accepted auth 後、同じ client UDP source から `Heartbeat` を 1 回送って `HeartbeatAck` を 1 回受ける確認手順です。

この経路は `ServerReceiveSendTwoIterationLauncher` から
`ServerControllerReceiveSendRuntimeBoundary` を 2 回だけ呼びます。1 回目で
accepted auth と `AuthenticatedSenderRegistry` 登録、2 回目で registered
heartbeat route から `ServerHeartbeatHandlerBoundary` -> `HeartbeatAck`
queue handoff -> one-item send runtime までを確認します。継続 receive/send loop、
continuous heartbeat loop、retry / requeue、file sink open、process-wide logger は含みません。

### 実行コマンド

server:

```powershell
cargo run -p stream-sync-server -- --receive-send-twice configs/examples/server.example.toml
```

client:

```powershell
cargo run -p stream-sync-client -- --auth-heartbeat-poc-once configs/examples/client.accepted.example.toml
```

### 成功時の見方

client stdout は、`AuthResponse` accepted と `HeartbeatAck` の時刻群を 1 行で表示します。

```text
auth heartbeat PoC sent AuthRequest <bytes> bytes to 127.0.0.1:5000 and received AuthResponse <bytes> bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; sent Heartbeat <bytes> bytes and received HeartbeatAck <bytes> bytes from 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1 heartbeat_sent_at=<client-sent-at> echoed_sent_at=<echoed-sent-at> server_received_at=<server-received-at> server_sent_at=<server-sent-at>
```

server stdout は、2 packet を処理し、1 回目と 2 回目の send byte 数を表示します。

```text
receive/send two-iteration runtime handled two packets on 0.0.0.0:5000; first_sent_bytes=<auth-response-bytes> second_sent_bytes=<heartbeat-ack-bytes> registered_clients=1
```

server stderr の要点:

```json
{"event_name":"server.receive_loop","source":"127.0.0.1:<client-port>","outcome":"Accepted","packet_len":<bytes>,"message_type":"AuthRequest","client_id":"player1","rejection_reason":null,"timestamp":<timestamp>}
{"event_name":"server.auth_result","run_id":"streamsync-dev-session","client_id":"player1","source":"127.0.0.1:<client-port>","accepted":true,"reason_code":"Ok","message":null,"app_version":"0.1.0","protocol_version":1,"timestamp":<timestamp>,"expected_protocol_version":null}
{"event_name":"server.send","outcome":"Success","run_id":"streamsync-dev-session","client_id":"player1","destination":"127.0.0.1:<client-port>","message_type":"AuthResponse","stage":"SocketSend","encoded_len":<bytes>,"bytes_sent":<bytes>,"failure":null,"disposition":null,"timestamp":<timestamp>}
{"event_name":"server.receive_loop","source":"127.0.0.1:<client-port>","outcome":"Accepted","packet_len":<bytes>,"message_type":"Heartbeat","client_id":"player1","rejection_reason":null,"timestamp":<timestamp>}
{"event_name":"server.send","outcome":"Success","run_id":"streamsync-dev-session","client_id":"player1","destination":"127.0.0.1:<client-port>","message_type":"HeartbeatAck","stage":"SocketSend","encoded_len":<bytes>,"bytes_sent":<bytes>,"failure":null,"disposition":null,"timestamp":<timestamp>}
```

確認の中心は、client が accepted auth 後も同じ UDP socket を使い、`Heartbeat` を 1 回だけ送ること、server が登録済み source として受理して `HeartbeatAck` を 1 回返すこと、client stdout で `echoed_sent_at` が `heartbeat_sent_at` と一致することです。

### 2026-04-22 Codex 環境 auth-then-heartbeat accepted path 成功

結果: 成功。

事前 build:

```powershell
cargo build -p stream-sync-server -p stream-sync-client
```

server:

```powershell
target/debug/stream-sync-server.exe --receive-send-twice configs/examples/server.example.toml
```

client:

```powershell
target/debug/stream-sync-client.exe --auth-heartbeat-poc-once configs/examples/client.accepted.example.toml
```

client stdout 観測結果:

```text
auth heartbeat PoC sent AuthRequest 96 bytes to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; sent Heartbeat 77 bytes and received HeartbeatAck 73 bytes from 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1 heartbeat_sent_at=<client-sent-at> echoed_sent_at=<same-client-sent-at> server_received_at=<server-received-at> server_sent_at=<server-sent-at>
```

server stdout 観測結果:

```text
receive/send two-iteration runtime handled two packets on 0.0.0.0:5000; first_sent_bytes=55 second_sent_bytes=73 registered_clients=1
```

server stderr 観測結果:

```json
{"event_name":"server.receive_loop","source":"127.0.0.1:<client-port>","outcome":"Accepted","packet_len":96,"message_type":"AuthRequest","client_id":"player1","rejection_reason":null,"timestamp":<timestamp>}
{"event_name":"server.auth_result","run_id":"streamsync-dev-session","client_id":"player1","source":"127.0.0.1:<client-port>","accepted":true,"reason_code":"Ok","message":null,"app_version":"0.1.0","protocol_version":1,"timestamp":<timestamp>,"expected_protocol_version":null}
{"event_name":"server.send","outcome":"Success","run_id":"streamsync-dev-session","client_id":"player1","destination":"127.0.0.1:<client-port>","message_type":"AuthResponse","stage":"SocketSend","encoded_len":55,"bytes_sent":55,"failure":null,"disposition":null,"timestamp":<timestamp>}
{"event_name":"server.receive_loop","source":"127.0.0.1:<client-port>","outcome":"Accepted","packet_len":77,"message_type":"Heartbeat","client_id":"player1","rejection_reason":null,"timestamp":<timestamp>}
{"event_name":"server.send","outcome":"Success","run_id":"streamsync-dev-session","client_id":"player1","destination":"127.0.0.1:<client-port>","message_type":"HeartbeatAck","stage":"SocketSend","encoded_len":73,"bytes_sent":73,"failure":null,"disposition":null,"timestamp":<timestamp>}
```

確認できたこと:

- client が accepted auth 後に同じ UDP socket で `Heartbeat` を 1 回送信できる。
- server が登録済み source からの `Heartbeat` を accepted として処理し、`HeartbeatAck` 73 bytes を 1 回返せる。
- client stdout で `heartbeat_sent_at` と `echoed_sent_at` が一致する。

## 成功時の見方

client 側には、送信 byte 数、destination、`client_id`、`run_id`、`protocol_version` が表示されます。

例:

```text
auth request PoC sent <request-bytes> bytes to 127.0.0.1:5000 and received AuthResponse <response-bytes> bytes from 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1 accepted=true reason_code=Ok message=null expected_protocol_version=null
```

rejected path の client stdout は、round trip が成立していれば同じ形式で
`accepted=false` と rejection reason を表示する。例:

```text
auth request PoC sent <request-bytes> bytes to 127.0.0.1:5000 and received AuthResponse <response-bytes> bytes from 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1 accepted=false reason_code=InvalidToken message=invalid shared_token expected_protocol_version=null
```

server 側には、受信後に `AuthResponse` を 1 回送ったこと、byte 数、`client_id`、`run_id`、auth decision が表示されます。

accepted path client を使った場合の想定:

```text
auth response PoC handled one packet on 0.0.0.0:5000 and sent <bytes> bytes; client_id=player1 run_id=streamsync-dev-session accepted=true reason_code=Ok
```

rejected path client を使った場合の想定:

```text
auth response PoC handled one packet on 0.0.0.0:5000 and sent <bytes> bytes; client_id=player1 run_id=streamsync-dev-session accepted=false reason_code=InvalidToken
```

byte 数は message の optional field 内容や今後の payload 変更で変わる可能性があります。確認の中心は、client が 1 回送信し、server が 1 packet を処理して `AuthResponse` を 1 回送ったこと、そして `accepted` / `reason_code` が想定通りであることです。

## 失敗時に見るところ

- server が戻らない
  - client を起動していない、client の destination が `127.0.0.1:5000` 以外を向いている、または firewall / OS 設定で UDP packet が届いていない可能性があります。
- server 起動時に `Bind` error が出る
  - `configs/examples/server.example.toml` の `bind_port = 5000` が他プロセスで使用中の可能性があります。別 port を使う場合は server config と client config の port を同じ値にした作業用コピーで確認します。
- client 側に `Destination` error が出る
  - `server_host` / `server_port` が解決できない値になっています。
- client 側に `Encode` error が出る
  - `client_id`, `run_id`, `app_version`, `shared_token`, `display_name` の文字列長や payload layout 周りを確認します。
- server 側に `Rejected` / `Protocol` 系 error が出る
  - `protocol_version` mismatch、malformed packet、未対応 message type、payload length 不一致の可能性があります。
- server 側が `accepted=false reason_code=InvalidToken` を出す
  - round trip は成功しています。認証 accepted path を見たい場合は、client command の config path が `configs/examples/client.accepted.example.toml` になっていることを確認します。rejected path 用の `configs/examples/client.example.toml` を使うと、この結果になります。
- JSON Lines ログが出ない
  - auth result と receive rejection の最小 JSON Lines は server 側 stderr に出ます。stdout だけを見ている場合は stderr も確認します。
  - file sink / rotation / retention はまだ未実装です。`logs/server` にファイルが作られることは期待しません。

## 現時点の責務

- server one-shot PoC
  - 1 packet を受信し、decode / gate / auth flow / `AuthResponse` encode / UDP send を 1 回だけ行います。
- client one-shot PoC
  - config から `AuthRequest` を作り、encode 済み bytes を UDP で 1 回だけ送り、同じ socket で `AuthResponse` を 1 回だけ受信して表示します。
- manual check
  - 2 つの CLI を順に起動し、stdout / stderr から one-shot round trip と auth decision を確認します。
- env-token manual check
  - server process の環境変数を token material として使い、config には `shared_token_env` の reference だけを置く経路を確認します。
  - secret store、rotation、file-based secret、hashing / KDF は含みません。

## 確認履歴

### 2026-04-22 Codex 環境 client AuthResponse receive accepted path 成功

結果: 成功

事前 build:

```powershell
cargo build -p stream-sync-server -p stream-sync-client
```

server:

```powershell
target/debug/stream-sync-server.exe --receive-send-once configs/examples/server.example.toml
```

client:

```powershell
target/debug/stream-sync-client.exe --auth-request-poc-once configs/examples/client.accepted.example.toml
```

client stdout 観測結果:

```text
auth request PoC sent 96 bytes to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1 accepted=true reason_code=Ok message=null expected_protocol_version=null
```

server stdout 観測結果:

```text
receive/send one-iteration runtime handled one packet on 0.0.0.0:5000; sent_bytes=55 observation_state=BodyIterationCompleted observation_action=YieldToCaller
```

server stderr 観測結果:

```json
{"event_name":"server.receive_loop","source":"127.0.0.1:<client-port>","outcome":"Accepted","packet_len":96,"message_type":"AuthRequest","client_id":"player1","rejection_reason":null,"timestamp":<timestamp>}
{"event_name":"server.auth_result","run_id":"streamsync-dev-session","client_id":"player1","source":"127.0.0.1:<client-port>","accepted":true,"reason_code":"Ok","message":null,"app_version":"0.1.0","protocol_version":1,"timestamp":<timestamp>,"expected_protocol_version":null}
{"event_name":"server.send","outcome":"Success","run_id":"streamsync-dev-session","client_id":"player1","destination":"127.0.0.1:<client-port>","message_type":"AuthResponse","stage":"SocketSend","encoded_len":55,"bytes_sent":55,"failure":null,"disposition":null,"timestamp":<timestamp>}
```

確認できたこと:

- client が `AuthRequest` を 1 回送信し、同じ UDP socket で `AuthResponse` を 1 回受信できる。
- client stdout で `accepted=true`, `reason_code=Ok`, `message=null`, `expected_protocol_version=null` を確認できる。
- server 側でも `AuthResponse` 55 bytes の UDP send と `server.send` success observation を確認できる。

### 2026-04-19 Codex 環境 shared_token_env accepted path 成功

結果: 成功

事前 build:

```powershell
cargo build -p stream-sync-server -p stream-sync-client
```

結果:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.03s
```

server 用環境変数:

```powershell
$env:STREAMSYNC_PLAYER1_TOKEN = "replace-with-shared-token-1"
$env:STREAMSYNC_PLAYER2_TOKEN = "replace-with-shared-token-2"
$env:STREAMSYNC_PLAYER3_TOKEN = "replace-with-shared-token-3"
$env:STREAMSYNC_PLAYER4_TOKEN = "replace-with-shared-token-4"
```

server:

```powershell
cargo run -p stream-sync-server -- --auth-response-poc-once configs/examples/server.env-token.example.toml
```

client:

```powershell
cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml
```

client 観測結果:

```text
auth request PoC sent 96 bytes to 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1
```

server stdout 観測結果:

```text
auth response PoC handled one packet on 0.0.0.0:5000 and sent 55 bytes; client_id=player1 run_id=streamsync-dev-session accepted=true reason_code=Ok
```

server stderr 観測結果:

```json
{"event_name":"server.auth_result","run_id":"streamsync-dev-session","client_id":"player1","source":"127.0.0.1:54171","accepted":true,"reason_code":"Ok","message":null,"app_version":"0.1.0","protocol_version":1,"timestamp":1776610138793019,"expected_protocol_version":null}
```

確認できたこと:

- `configs/examples/server.env-token.example.toml` と `configs/examples/client.accepted.example.toml` の組み合わせで `shared_token_env` accepted path が成立する
- server が `STREAMSYNC_PLAYER*_TOKEN` から token material を解決して auth decision へ進める
- client が `AuthRequest` を 1 回 UDP send できる
- server が 1 packet を受信し、decode / gate / secret resolver / auth flow / `AuthResponse` encode / UDP send まで進める
- auth decision が `accepted=true`, `reason_code=Ok` になる
- auth result JSON Lines が server stderr に 1 行出る

補足:

- `STREAMSYNC_PLAYER1_TOKEN` だけを設定した試行では、`player2` 以降の env var が未設定のため `accepted=false`, `reason_code=InternalError`, `message="token secret environment variable is missing"` を観測した。
- 現在の resolver は config 内の token reference を auth decision 前にまとめて解決するため、`configs/examples/server.env-token.example.toml` を使う場合は 4 つすべての `STREAMSYNC_PLAYER*_TOKEN` を設定する。

### 2026-04-19 Codex 環境 accepted path 成功

結果: 成功

事前 build:

```powershell
cargo build -p stream-sync-server -p stream-sync-client
```

結果:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

server:

```powershell
cargo run -p stream-sync-server -- --auth-response-poc-once configs/examples/server.example.toml
```

client:

```powershell
cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml
```

client 観測結果:

```text
auth request PoC sent 96 bytes to 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1
```

server 観測結果:

```text
auth response PoC handled one packet on 0.0.0.0:5000 and sent 55 bytes; client_id=player1 run_id=streamsync-dev-session accepted=true reason_code=Ok
```

確認できたこと:

- `configs/examples/server.example.toml` と `configs/examples/client.accepted.example.toml` の組み合わせで accepted path が成立する
- client が `AuthRequest` を 1 回 UDP send できる
- server が 1 packet を受信し、decode / gate / auth flow / `AuthResponse` encode / UDP send まで進める
- auth decision が `accepted=true`, `reason_code=Ok` になる

### 2026-04-19 Codex 環境

結果: 未完了

accepted path の実機手動確認を試行しましたが、UDP packet の送受信前に Rust binary のリンクで止まりました。

実行した確認:

```powershell
cargo run -p stream-sync-server -- --auth-response-poc-once configs/examples/server.example.toml
cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml
```

最初の試行では server / client の `cargo run` が同時に artifact directory を使い、client 側で次の待機が発生しました。

```text
Blocking waiting for file lock on artifact directory
```

その後、事前 build で lock 競合を避けるために次を実行しました。

```powershell
cargo build -p stream-sync-server -p stream-sync-client
```

観測結果:

```text
error: linker `link.exe` not found
note: the msvc targets depend on the msvc linker but `link.exe` was not found
```

詰まり箇所:

- `AuthRequest` send / `AuthResponse` receive の前段階
- `stream-sync-server` / `stream-sync-client` binary のリンク
- UDP socket bind / send、decode、gate、auth decision には未到達

次回確認では、MSVC linker `link.exe` が使える Visual Studio Build Tools 環境、または Rust target に合った linker が有効な shell で同じ手順を再実行します。期待する server 出力は `accepted=true reason_code=Ok` です。
