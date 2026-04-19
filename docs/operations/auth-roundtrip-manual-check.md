<!-- stream-sync/docs/operations/auth-roundtrip-manual-check.md -->

# One-shot Auth Round Trip Manual Check

この手順は、server / client の one-shot auth PoC が UDP で 1 往復分つながることを手動確認するためのものです。

対象は `AuthRequest` を 1 回送り、server が `AuthResponse` を 1 回返すところまでです。継続 loop、async runtime、heartbeat、video frame、JSON Lines 出力、retry、fragmentation、encryption は含みません。

## 使用する config

- server: `configs/examples/server.example.toml`
- client: `configs/examples/client.example.toml`

注意: 現在の example config は、client 側 `shared_token = "replace-with-shared-token"`、server 側 `player1.shared_token = "replace-with-shared-token-1"` で値が異なります。このまま実行すると round trip 自体は確認できますが、server の auth decision は `accepted=false`, `reason_code=InvalidToken` になる想定です。

accepted path を確認したい場合は、作業用の client config コピーを作り、`[client].shared_token` を `replace-with-shared-token-1` に合わせてから実行します。example config 本体を変える必要はありません。

## 手順

ターミナルを 2 つ開き、どちらも repo root で実行します。

### 1. server を起動する

```powershell
cargo run -p stream-sync-server -- --auth-response-poc-once configs/examples/server.example.toml
```

server は `configs/examples/server.example.toml` の `[server].bind_host` / `[server].bind_port` を使って UDP socket を bind し、1 packet だけ待ち受けます。デフォルトでは `0.0.0.0:5000` です。

このコマンドは client から 1 packet が届くまで戻りません。先に server を起動してから client を起動します。

### 2. client を起動する

別ターミナルで実行します。

```powershell
cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.example.toml
```

client は `configs/examples/client.example.toml` の `[client].server_host` / `[client].server_port` を destination として解決し、`AuthRequest` を fixed header + payload bytes に encode して 1 回だけ UDP send します。デフォルトでは `127.0.0.1:5000` に送ります。

## 成功時の見方

client 側には、送信 byte 数、destination、`client_id`、`run_id`、`protocol_version` が表示されます。

例:

```text
auth request PoC sent <bytes> bytes to 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1
```

server 側には、受信後に `AuthResponse` を 1 回送ったこと、byte 数、`client_id`、`run_id`、auth decision が表示されます。

example config をそのまま使った場合の想定:

```text
auth response PoC handled one packet on 0.0.0.0:5000 and sent <bytes> bytes; client_id=player1 run_id=streamsync-dev-session accepted=false reason_code=InvalidToken
```

accepted path 用に client token を `replace-with-shared-token-1` に合わせた場合の想定:

```text
auth response PoC handled one packet on 0.0.0.0:5000 and sent <bytes> bytes; client_id=player1 run_id=streamsync-dev-session accepted=true reason_code=Ok
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
  - round trip は成功しています。認証 accepted path を見たい場合は、client config の `shared_token` を server 側の `[auth.clients.player1].shared_token` と同じ値にした作業用コピーで再実行します。
- JSON Lines ログが出ない
  - 現時点では仕様と typed boundary のみで、JSON Lines 出力本実装はまだありません。確認は stdout / stderr で行います。

## 現時点の責務

- server one-shot PoC
  - 1 packet を受信し、decode / gate / auth flow / `AuthResponse` encode / UDP send を 1 回だけ行います。
- client one-shot PoC
  - config から `AuthRequest` を作り、encode 済み bytes を UDP で 1 回だけ送ります。
- manual check
  - 2 つの CLI を順に起動し、stdout / stderr から one-shot round trip と auth decision を確認します。
