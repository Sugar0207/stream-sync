<!-- stream-sync/docs/operations/session-log.md -->

## 2026-04-19
### 種別
- Codex

### 今回の作業
- `shared_token_env` を使う one-shot auth round trip 手順を repo 内 docs に追加した。
- `configs/examples/server.env-token.example.toml` を追加し、server 側 token material を `STREAMSYNC_PLAYER*_TOKEN` から解決する確認用 config を用意した。
- `docs/operations/auth-roundtrip-manual-check.md` に PowerShell での環境変数設定、server / client 起動コマンド、成功時 / 失敗時の確認ポイントを追記した。

### 変更ファイル
- `configs/examples/server.env-token.example.toml`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- inline token の既存 accepted 手順は維持し、`shared_token_env` 用 server config は別ファイルに分ける。
- `player1` の env-token 手動確認では `STREAMSYNC_PLAYER1_TOKEN = "replace-with-shared-token-1"` を server 起動ターミナルに設定する。
- 成功確認は server stdout の `accepted=true reason_code=Ok` と、stderr の `server.auth_result` JSON Lines で行う。
- file sink / rotation / retention は今回も未実装に残す。

### 未解決事項
- `shared_token_env` 手順の実機実行結果の記録
- heartbeat / video frame handler へ accepted route を渡す接続
- auth / receive JSON Lines の file sink 設定方針
- secret store 連携や token rotation 方針

### 次にやる候補
- heartbeat / video frame handler へ registered packet を渡す接続方針を整理する
- auth / receive JSON Lines の file sink 設定方針を整理する
- `shared_token_env` one-shot auth round trip を実機手動確認して結果を記録する

### TODO更新
- 完了:
  - `shared_token_env` one-shot auth round trip 手順追加
  - env-token server helper config 追加
  - env token accepted / missing / empty / mismatch の確認ポイント整理
- 追加:
  - `shared_token_env` one-shot auth round trip の実機手動確認
- 保留:
  - secret store 連携
  - token rotation
  - heartbeat / video frame 処理本体
  - async runtime

### メモ
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- secret resolver の最小本実装を追加した。
- `ServerSecretResolverBoundary` が `shared_token_env` の環境変数を読み、inline PoC token と同じ resolved token material として auth decision input へ渡せるようにした。
- missing / empty / invalid environment variable を `ServerSecretResolutionError` の typed error として扱うようにした。
- `ServerAuthFlowStep` で config input -> secret resolver -> resolved auth decision input -> auth decision の順に接続した。
- docs に現在の実装範囲と未実装範囲を反映した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `shared_token_env` は named environment variable を同期的に 1 回読む最小 resolver とする。
- missing / empty / invalid env var は token 値を持たない typed error にする。
- auth decision は env を読まず、resolved token material と presented token の比較だけを行う。
- resolver error は auth flow 内で `InternalError` の `ServerAuthDecision` に変換する。
- secret store、hashing / KDF、rotation、cache / hot reload は今回の範囲外とする。

### 未解決事項
- secret store 連携や token rotation 方針
- `shared_token_env` を使う手動 round trip 手順
- heartbeat / video frame handler へ accepted route を渡す接続
- auth / receive JSON Lines の file sink 設定方針

### 次にやる候補
- heartbeat / video frame handler へ registered packet を渡す接続方針を整理する
- auth / receive JSON Lines の file sink 設定方針を整理する
- `shared_token_env` を使う one-shot auth round trip 手順を整理する

### TODO更新
- 完了:
  - `shared_token_env` secret resolver の最小本実装
  - missing / empty / invalid env var typed error
  - resolved token material から auth decision へ渡す flow 接続
- 追加:
  - `shared_token_env` を使う one-shot auth round trip 手順
  - secret store 連携や token hashing / rotation 方針
- 保留:
  - secret store 連携
  - token hashing / KDF / rotation
  - heartbeat / video frame 処理本体
  - async runtime

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-server secret_resolver`、`cargo test -p stream-sync-server environment_variable_token` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- auth result writer の有効化位置を、one-shot auth response PoC CLI の auth decision 後に決めた。
- `apps/server/src/main.rs` で `ServerAuthLogOutputBoundary` を呼び、auth success / failure を stderr へ JSON Lines 1 行として出すようにした。
- future loop は同じ writer boundary を auth decision point で呼ぶ方針に留め、file sink / rotation / async logging / 汎用 logging 基盤は未実装に残した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に current sink と future loop の接続位置を反映した。

### 変更ファイル
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- one-shot path の auth result log は stderr に出す。
- 出力タイミングは `ServerAuthResponsePocStep` が auth decision と auth log handoff input を返した後にする。
- receive rejection log と同じく、PoC CLI の観測用 sink として扱い、file sink や process-wide logger は作らない。
- future continuous loop は auth decision 作成直後に同じ `ServerAuthLogOutputBoundary` を呼ぶ。

### 未解決事項
- secret resolver 本実装
- heartbeat / video frame handler へ accepted route を渡す接続
- auth / receive JSON Lines の file sink 設定方針
- log rotation / retention / buffering

### 次にやる候補
- secret resolver 本実装を行う
- heartbeat / video frame handler へ registered packet を渡す接続方針を整理する
- auth / receive JSON Lines の file sink 設定方針を整理する

### TODO更新
- 完了:
  - auth result writer の one-shot CLI stderr 接続判断
  - one-shot auth response PoC の auth result JSON Lines stderr 出力
  - future loop の writer 呼び出し位置の docs 整理
- 追加:
  - auth / receive JSON Lines の file sink 設定方針
- 保留:
  - secret resolver 本実装
  - heartbeat / video frame 処理本体
  - async runtime
  - 大規模 logging 基盤

### メモ
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- 認証済み送信元登録の実処理を auth accepted path へ接続済みであることを確認し、責務を docs に反映した。
- `ServerAuthResponsePocStep` の責務コメントを、accepted registration を registry に適用する現在の実装に合わせた。
- accepted auth flow の `AuthenticatedSenderRegistration` を in-memory registry に登録し、後続 `PacketAcceptanceGateBoundary` が同一 client/source の `Heartbeat` を accepted にする最小テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- auth flow は accepted decision から registry registration handoff を作るだけに留める。
- one-shot auth response PoC step が、その handoff を `AuthenticatedSenderRegistryBoundary::register` に渡して in-memory registry を更新する。
- registry は `client_id` と source endpoint の対応を保持し、後続 packet acceptance gate の lookup に使う。
- timeout、失効、再認証、永続化は今回も未実装に残す。

### 未解決事項
- auth result writer の CLI 接続判断
- secret resolver 本実装
- 認証済み送信元の timeout / 失効 / 再認証
- heartbeat / video frame handler へ accepted route を渡す接続

### 次にやる候補
- auth result writer を one-shot / future loop のどこで有効化するか決める
- secret resolver 本実装を行う
- heartbeat / video frame handler へ registered packet を渡す接続方針を整理する

### TODO更新
- 完了:
  - accepted auth path の in-memory registry 登録実処理
  - accepted registration 後に packet acceptance gate が後続 packet を accepted にできるテスト追加
  - architecture docs の registry 実処理範囲更新
- 追加:
  - heartbeat / video frame handler へ registered packet を渡す接続方針
- 保留:
  - secret resolver 本実装
  - auth result writer の CLI 接続
  - timeout / 失効 / 再認証
  - heartbeat / video frame 処理本体

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-server accepted_auth_flow_registration_updates_registry_for_later_gate` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- secret resolver 本実装範囲を確定し、docs と placeholder に反映した。
- `apps/server` に `ServerSecretResolverBoundary`, `ServerSecretResolutionPlan`, `ServerResolvedSharedTokenAuthInput`, `ServerResolvedSharedTokenMaterial` を追加した。
- placeholder は inline PoC token を `AlreadyResolved`、`shared_token_env` を `NeedsEnvironmentVariable` として分類するだけに留め、環境変数の読み取りは実装しない。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に、最初の real resolver が扱う範囲、未対応範囲、config / resolver / auth input / auth decision の責務分離を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 最初の real resolver は `shared_token_env` の環境変数読み取りまでを対象にする。
- inline `shared_token` は PoC 互換の already-resolved material として残す。
- secret store、network call、cache / hot reload、rotation、hashing / KDF は最初の resolver から外す。
- config は reference parsing、resolver は reference resolution、auth input は context assembly、auth decision は prepared material との比較を担当する。
- 解決済み token material は Debug で redacted 表示にする。

### 未解決事項
- `shared_token_env` の実際の環境変数読み取り
- secret 解決後の auth decision input への接続
- 認証済み送信元登録の実処理接続
- auth result writer の CLI 接続判断
- heartbeat / video frame 処理本体

### 次にやる候補
- 認証済み送信元登録の実処理を auth accepted path へ接続する
- auth result writer を one-shot / future loop のどこで有効化するか決める
- secret resolver 本実装を行う

### TODO更新
- 完了:
  - secret resolver 本実装範囲の確定
  - `ServerSecretResolverBoundary` / secret resolution plan placeholder の追加
  - config / resolver / auth decision の責務分離更新
- 追加:
  - secret resolver 本実装
- 保留:
  - 本物の secret store 連携
  - async runtime
  - heartbeat / video frame 処理
  - 大規模 logging 基盤

### メモ
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。
- `cargo test -p stream-sync-server secret_resolver` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- auth success / failure と receive rejection の JSON Lines writer 接続範囲を整理した。
- `apps/server` に `ServerAuthLogOutputBoundary` と `ServerAuthJsonLineWriter` を追加し、既存の `ServerAuthJsonLogEventBoundary` から 1 行 JSON Lines を `io::Write` へ出せるようにした。
- receive rejection 側の既存 `ServerReceiveRejectionLogOutputBoundary` と並ぶ接続形として、auth result / receive rejection の handoff input、event schema input、writer boundary、current sink を docs に整理した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に、schema-specific writer までを現在の接続範囲とし、file sink / rotation / async logging / 汎用 logging crate API は未実装に残す方針を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- auth result と receive rejection は、typed handoff input -> event schema input -> schema-specific JSON Lines writer -> caller-owned `io::Write` sink の同じ接続形にする。
- receive rejection は one-shot server CLI の stderr に接続済みとする。
- auth result writer は boundary と writer まで追加し、CLI の既定出力にはまだ接続しない。
- file sink、rotation、retention、async logging、metrics fanout、汎用 logging crate API は今回の範囲外とする。

### 未解決事項
- auth result writer を one-shot / future loop のどこで有効化するか
- secret resolver 本実装範囲の確定
- 認証済み送信元登録の実処理接続
- file sink / rotation / retention
- heartbeat / video frame 処理本体

### 次にやる候補
- secret resolver 本実装範囲を確定する
- 認証済み送信元登録の実処理を auth accepted path へ接続する
- auth result writer を one-shot / future loop のどこで有効化するか決める

### TODO更新
- 完了:
  - auth / receive JSON Lines writer 接続範囲の整理
  - `ServerAuthLogOutputBoundary` / `ServerAuthJsonLineWriter` の追加
  - auth result writer の単体テスト
- 追加:
  - auth result writer の有効化位置の判断
- 保留:
  - 汎用 logging 基盤
  - async runtime
  - heartbeat / video frame 処理
  - secret resolver 本実装

### メモ
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。
- `cargo test -p stream-sync-server auth_json` と `cargo test -p stream-sync-server log_output_boundary` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- receive rejection ログ出力の最小実装を追加した。
- `apps/server` に `ServerReceiveRejectionLogOutputBoundary` と `ServerReceiveRejectionJsonLineWriter` を追加した。
- 既存の `ServerRejectionDropLogHandoffBoundary` と `ServerReceiveRejectionJsonLogEventBoundary` を接続し、receive rejection を 1 行 JSON Lines として `io::Write` へ出力できるようにした。
- server one-shot auth response PoC で `ServerAuthResponsePocError::Rejected` が返った場合、stderr へ receive rejection JSON Lines を 1 行出してから既存の error message を出すようにした。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に、出力先、出力 fields、今回も file writer / rotation / async logging へ広げない方針を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- receive rejection の最小出力先は、現時点では one-shot server CLI の stderr とする。
- 出力形式は `server.receive_rejection` の JSON Lines 1 行とする。
- 出力 fields は `event_name`, `run_id`, `client_id`, `source`, `message_type`, `rejection_reason`, `detail`, `timestamp` とする。
- file sink、rotation、buffering policy、async logging、汎用 JSON Lines writer は今回の範囲外とする。

### 未解決事項
- auth success / failure JSON Lines writer 接続
- receive rejection の file sink / rotation / retention
- secret resolver 本実装
- 認証済み送信元登録の実処理接続
- heartbeat / video frame 処理本体

### 次にやる候補
- auth success / failure と receive rejection の JSON Lines writer 接続範囲を決める
- secret resolver 本実装範囲を確定する
- 認証済み送信元登録の実処理を auth accepted path へ接続する

### TODO更新
- 完了:
  - receive rejection ログ出力の最小実装
  - one-shot server CLI の rejected path stderr JSON Lines 出力
  - receive rejection JSON Lines writer の単体テスト
- 追加:
  - auth / receive ログ writer 接続範囲の整理
  - 認証済み送信元登録の実処理接続
- 保留:
  - JSON Lines の大規模 writer 基盤
  - async runtime
  - heartbeat / video frame 処理
  - secret resolver 本実装

### メモ
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。
- `cargo test -p stream-sync-server receive_rejection` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- secret 解決方式と token 保護方針を docs に整理した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に、`shared_token` / `shared_token_env` の責務、secret resolution boundary、token 非露出方針を追記した。
- `crates/config` で `shared_token_env` を `SharedTokenSecretRef::EnvironmentVariable` として読める placeholder を追加した。
- `shared_token` と `shared_token_env` の同時指定を config error として扱うようにした。
- `SharedTokenSecretRef` の Debug 出力で inline token material を `<redacted>` にするようにした。
- `apps/server` に `ServerSharedTokenSecretResolutionStatus` placeholder を追加し、auth input の token reference が PoC inline か未解決 env ref か分類できるようにした。
- `configs/examples/server.example.toml` に PoC inline token と将来の `shared_token_env` 運用方針のコメントを追加した。

### 変更ファイル
- `crates/config/src/lib.rs`
- `apps/server/src/lib.rs`
- `configs/examples/server.example.toml`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- PoC の one-shot auth round trip は引き続き inline `shared_token` を使う。
- 本運用寄りの config では `shared_token_env` を優先し、config は環境変数名などの reference だけを保持する。
- `config` は secret reference の parse まで、auth input boundary は request context との組み合わせまで、secret resolver は将来の外部 lookup、auth decision は prepared material との比較までを責務とする。
- raw token は stdout、JSON Lines、auth response message、debug 出力へ出さない。

### 未解決事項
- 環境変数や secret store から token material を解決する本実装
- secret 解決後の token 検証への接続
- receive rejection ログ出力本実装
- auth / receive JSON Lines writer 接続
- heartbeat / video frame 処理本体

### 次にやる候補
- receive rejection ログ出力本実装を行う
- auth success / failure と receive rejection の JSON Lines writer 接続範囲を決める
- secret resolver 本実装範囲を確定する

### TODO更新
- 完了:
  - secret 解決方式と token 保護方針の整理
  - `shared_token_env` placeholder の追加
  - inline token debug redaction の追加
  - server secret resolution status placeholder の追加
- 追加:
  - secret resolver 本実装範囲の確定
- 保留:
  - 本物の secret store 連携
  - JSON Lines 出力本実装
  - heartbeat / video frame 処理
  - retry / fragmentation / encryption

### メモ
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。
- `cargo test -p stream-sync-config` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- server / client one-shot auth round trip の accepted path を実機手動確認した。
- `cargo build -p stream-sync-server -p stream-sync-client` が成功することを確認した。
- server を `--auth-response-poc-once configs/examples/server.example.toml`、client を `--auth-request-poc-once configs/examples/client.accepted.example.toml` で実行した。
- client stdout で 96 bytes の `AuthRequest` 送信を確認した。
- server stdout で 55 bytes の `AuthResponse` 送信、`accepted=true`, `reason_code=Ok` を確認した。
- `docs/operations/auth-roundtrip-manual-check.md` に accepted path 成功履歴を追記した。
- `docs/operations/todo.md` の次の実装優先順位を、secret 解決、receive rejection ログ出力、JSON Lines writer 接続中心へ更新した。

### 変更ファイル
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- one-shot auth round trip の accepted path は、server config `configs/examples/server.example.toml` と client config `configs/examples/client.accepted.example.toml` の組み合わせで確認済みとする。
- 次の実装優先順位は、secret 解決、receive rejection ログ出力本実装、auth / receive ログ writer 接続の順に寄せる。
- 継続 loop、async runtime、heartbeat / video frame、JSON Lines 出力本実装の広い拡張は今回も範囲外とする。

### 未解決事項
- secret 解決本実装
- JSON Lines writer 接続範囲の決定と本実装
- receive rejection ログ出力本実装
- heartbeat / video frame 処理本体
- 継続 receive / send loop

### 次にやる候補
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う
- auth success / failure と receive rejection の JSON Lines writer 接続範囲を決める

### TODO更新
- 完了:
  - accepted path 成功確認
  - accepted path 確認コマンドと観測結果の記録
- 追加:
  - auth / receive ログ writer 接続範囲の整理
- 保留:
  - 継続 loop / async runtime
  - heartbeat / video frame 処理
  - JSON Lines 出力本実装の広い拡張
  - retry / fragmentation / encryption

### メモ
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- server / client one-shot auth round trip の accepted path 手動確認を試行した。
- server を `--auth-response-poc-once configs/examples/server.example.toml`、client を `--auth-request-poc-once configs/examples/client.accepted.example.toml` で実行する確認を試した。
- 最初の試行では同時 `cargo run` により artifact directory の lock 待ちが発生したため、事前 build に切り替えて確認した。
- `cargo build -p stream-sync-server -p stream-sync-client` が MSVC linker `link.exe` 不足で失敗し、UDP 送受信前に停止した。
- `docs/operations/auth-roundtrip-manual-check.md` に確認履歴、観測結果、詰まり箇所を追記した。

### 変更ファイル
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 今回の accepted path 確認結果は未完了として扱う。
- 詰まり箇所は auth flow ではなく、`stream-sync-server` / `stream-sync-client` binary のリンク環境。
- 次回は MSVC linker `link.exe` が使える Visual Studio Build Tools 環境、または Rust target に合った linker が有効な shell で同じ手順を再実行する。

### 未解決事項
- accepted path の `accepted=true reason_code=Ok` 実機観測
- MSVC linker を使える実行環境の用意
- secret 解決本実装
- JSON Lines 出力本実装
- heartbeat / video frame 処理本体

### 次にやる候補
- MSVC linker が使える環境で accepted path 手順を再実行し、stdout の `accepted=true reason_code=Ok` を確認する
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う

### TODO更新
- 完了:
  - accepted path 手動確認の試行
  - link error による未完了結果の記録
- 追加:
  - MSVC linker が使える環境で accepted path を再実行する
- 保留:
  - 継続 loop / async runtime
  - heartbeat / video frame 処理
  - JSON Lines 出力本実装
  - retry / fragmentation / encryption

### メモ
- `cargo build -p stream-sync-server -p stream-sync-client` は `link.exe` 不足で失敗した。
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- server / client one-shot auth round trip の accepted path 用 client config を追加した。
- `docs/operations/auth-roundtrip-manual-check.md` を更新し、accepted path を `configs/examples/client.accepted.example.toml` で確認する手順に整理した。
- 既存の `configs/examples/client.example.toml` は token mismatch による rejected path 確認用として明記した。

### 変更ファイル
- `configs/examples/client.accepted.example.toml`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- accepted path の手動確認では、server に `configs/examples/server.example.toml`、client に `configs/examples/client.accepted.example.toml` を使う。
- rejected path の確認では、client に既存の `configs/examples/client.example.toml` を使える。
- 継続 loop、async runtime、heartbeat / video frame、JSON Lines 出力、retry、fragmentation、encryption は今回も範囲外とする。

### 未解決事項
- accepted path の実機手動実行確認
- secret 解決本実装
- JSON Lines 出力本実装
- heartbeat / video frame 処理本体
- 継続 receive / send loop

### 次にやる候補
- accepted path 手順を実際に server / client で実行し、stdout の `accepted=true reason_code=Ok` を確認する
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う

### TODO更新
- 完了:
  - accepted path 用 client config の追加
  - accepted path 手動確認手順の更新
- 追加:
  - accepted path の実機手動実行確認
- 保留:
  - 継続 loop / async runtime
  - heartbeat / video frame 処理
  - JSON Lines 出力本実装
  - retry / fragmentation / encryption

### メモ
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- AGENTS.md が軽量版になっていることを確認した。
- 重要ルールとして、技術方針、禁止事項、repo 内 docs を正とする運用、TODO / session-log 更新、Git 判断報告が維持されていることを確認した。
- `docs/operations/todo.md` に今回の運用更新を追記した。
- コード変更は行っていない。

### 変更ファイル
- `AGENTS.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 今後の Codex 運用では、軽量化された `AGENTS.md` を入口にし、詳細な進捗や判断は `docs/operations/todo.md` と `docs/operations/session-log.md` を正として確認する。
- 技術方針、MVP 対象外、禁止事項、Git 運用、docs 更新ルールの意味は変更しない。

### 未解決事項
- なし

### 次にやる候補
- server / client one-shot auth round trip の accepted path を手動確認する
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う

### TODO更新
- 完了:
  - AGENTS.md 軽量版への運用更新確認
- 追加:
  - なし
- 保留:
  - なし

### メモ
- cargo 系コマンドは今回の対象外のため実行していない。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- server / client one-shot auth round trip の手動確認手順を追加した。
- `docs/operations/auth-roundtrip-manual-check.md` を追加し、server / client の起動コマンド、使用 config path、成功時の stdout、失敗時に見る場所を整理した。
- server PoC の成功時 stdout に `client_id`, `run_id`, `accepted`, `reason_code` を表示する最小観測補助を追加した。
- client PoC の成功時 stdout に `client_id`, `run_id`, `protocol_version` を表示する最小観測補助を追加した。
- README のドキュメント一覧に手動確認手順を追加した。

### 変更ファイル
- `README.md`
- `apps/server/src/main.rs`
- `apps/client/src/main.rs`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 手動確認は既存の one-shot PoC をそのまま使い、ターミナル 2 つで server を先に起動してから client を起動する。
- 使用 config は `configs/examples/server.example.toml` と `configs/examples/client.example.toml` とする。
- 現在の example config は token が一致しないため、そのまま実行した場合は round trip 成功かつ auth decision は `accepted=false`, `reason_code=InvalidToken` になる。
- accepted path を見る場合は、作業用 client config copy の `shared_token` を server 側 `player1` と同じ `replace-with-shared-token-1` に合わせる。
- JSON Lines 出力、継続 loop、async runtime、heartbeat / video frame、retry、fragmentation、encryption は今回も範囲外とする。

### 未解決事項
- accepted path の手動実行確認
- secret 解決本実装
- JSON Lines 出力本実装
- heartbeat / video frame 処理本体
- 継続 receive / send loop

### 次にやる候補
- server / client one-shot auth round trip の accepted path を手動確認する
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う

### TODO更新
- 完了:
  - one-shot auth round trip 手動確認手順
  - server / client PoC stdout の最小観測補助
- 追加:
  - accepted path の手動確認
- 保留:
  - 継続 loop / async runtime
  - heartbeat / video frame 処理
  - JSON Lines 出力本実装
  - retry / fragmentation / encryption

### メモ
- 手動確認の責務は、既存 one-shot server/client PoC を順に起動し、stdout / stderr から UDP 1 往復と auth decision を確認できるようにすることまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- client 側 `AuthRequest` 送信 PoC を追加した。
- `crates/protocol` に `AuthRequest` payload encode と fixed header + payload encode を追加した。
- `ProtocolMessageEncoderBoundary` から `ProtocolMessage::AuthRequest` を encode できるようにした。
- `apps/client` に `ClientAuthRequestPocLauncher`, `ClientAuthRequestPocStartupConfig`, `ClientAuthRequestPocOutcome`, `ClientAuthRequestPocError` を追加した。
- client TOML から server destination、`client_id`, `shared_token`, optional `display_name`, `run_id`, `app_version`, `protocol_version` を読み、`AuthRequest` を 1 回だけ UDP 送信できるようにした。
- client binary に `--auth-request-poc-once [config-path]` の明示入口を追加した。
- docs に client 側 auth request one-shot PoC の flow と責務分離を追記した。

### 変更ファイル
- `apps/client/Cargo.toml`
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `crates/protocol/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- client auth request PoC は `configs/examples/client.example.toml` と同じ形の TOML を入力にする。
- client launcher は config 読み込み、destination 解決、`AuthRequest` 構築、protocol encode、ephemeral UDP bind、1 回の `send_to` だけを担当する。
- `AuthRequest` encode は既存 decode と同じ payload layout に合わせ、`client_id`, `run_id`, `app_version`, `shared_token`, `display_name` を書く。
- 継続 loop、heartbeat / video frame 送信、async runtime、retry、fragmentation、encryption、secret 解決本実装は今回の範囲外とする。

### 未解決事項
- server / client one-shot auth round trip の手動確認
- secret 解決本実装
- heartbeat / video frame 送信
- 継続 loop / reconnect
- JSON Lines 出力本実装

### 次にやる候補
- server / client one-shot auth round trip を手動確認する
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う

### TODO更新
- 完了:
  - client 側 `AuthRequest` 送信 PoC
  - `AuthRequest` encode 本実装
  - `--auth-request-poc-once [config-path]` 入口追加
- 追加:
  - server / client one-shot auth round trip 手動確認
- 保留:
  - 継続 loop / async runtime
  - heartbeat / video frame 送信
  - retry / fragmentation / encryption
  - secret 解決本実装

### メモ
- client 側 auth request PoC の責務は、client TOML から 1 回分の `AuthRequest` と destination を作り、protocol encoder で bytes 化して UDP に 1 datagram 送るところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- auth response PoC の起動設定接続を追加した。
- `apps/server` に `ServerAuthResponsePocLauncher`, `ServerAuthResponsePocStartupConfig`, `ServerAuthResponsePocStartupOutcome`, `ServerAuthResponsePocStartupError` を追加した。
- server TOML から `[server].bind_host`, `[server].bind_port`, `[session].protocol_version` を読み取り、bind address と expected protocol version を用意できるようにした。
- 同じ TOML content を `ServerAuthConfigBoundary` に渡し、allowed clients / shared token placeholder を読み込む形にした。
- `UdpSocketIoBoundary::bind`、空の `AuthenticatedSenderRegistry` 初期化、`ServerAuthResponsePocStep::run_one` 呼び出しまでを接続した。
- server binary に `--auth-response-poc-once [config-path]` の明示入口を追加した。
- docs に auth response PoC startup config entry の flow と責務分離を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 起動設定接続は `configs/examples/server.example.toml` と同じ形の TOML を入力にする。
- launcher は bind address 解決、UDP socket bind、auth config 読み込み、registry 初期化、one-shot PoC step 呼び出しだけを担当する。
- binary はデフォルトでは scaffold 表示のままとし、`--auth-response-poc-once` が指定された場合だけ 1 packet 待ち受けに入る。
- 継続 loop、async runtime、JSON Lines 出力、retry、fragmentation、encryption、heartbeat / video frame 処理本体は今回の範囲外とする。

### 未解決事項
- client 側 AuthRequest 送信 PoC
- secret 解決本実装
- receive rejection / auth / send の JSON Lines 出力本実装
- 継続 receive / send loop
- heartbeat / video frame 処理本体

### 次にやる候補
- client 側 AuthRequest 送信 PoC を追加する
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う

### TODO更新
- 完了:
  - auth response PoC の起動設定接続を追加する
  - `ServerAuthResponsePocLauncher` 追加
  - `--auth-response-poc-once [config-path]` 入口追加
- 追加:
  - client 側 AuthRequest 送信 PoC
- 保留:
  - 継続 loop / async runtime
  - JSON Lines 出力本実装
  - retry / fragmentation / encryption
  - heartbeat / video frame 処理本体

### メモ
- auth response PoC 起動入口の責務は、server TOML から bind / auth config / protocol version を用意し、UDP socket と registry を初期化して `ServerAuthResponsePocStep` を 1 回呼ぶところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- UDP socket を auth response PoC の起動処理へ最小接続した。
- `apps/server` に `ServerAuthResponsePocStep` / `ServerAuthResponsePocOutcome` / `ServerAuthResponsePocError` を追加した。
- 1 packet の UDP receive から receive loop / decode / gate / auth flow / outbound queue handoff / protocol encode / UDP send までを接続した。
- accepted auth decision の registry registration handoff を、既存の in-memory registry 境界へ反映できるようにした。
- UDP socket を使う最小テストで、`AuthRequest` を受けて encoded `AuthResponse` が返ることを確認する構造を追加した。
- docs に auth response PoC one-shot 起動フローと責務分離を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- auth response PoC 起動処理は同期 `UdpSocket` の 1 datagram receive / 1 datagram send に限定する。
- receive 側は既存の `ServerUdpSocketIoStep::receive_one_with_gate` を使い、accepted `AuthRequest` だけを auth flow へ渡す。
- send 側は `ServerAuthFlowStep` の `OutboundQueueItem` を `OutboundPacketEncoderBoundary` と `ProtocolMessageEncoderBoundary` で encode してから socket send へ渡す。
- 継続 loop、async runtime、retry、fragmentation、encryption、JSON Lines 出力、heartbeat / video frame handler は今回の範囲外とする。

### 未解決事項
- server 起動設定から socket bind / config 読み込み / PoC step 呼び出しを行う処理
- 継続 receive / send loop
- receive rejection / auth / send の JSON Lines 出力本実装
- secret 解決本実装
- heartbeat / video frame 処理本体

### 次にやる候補
- auth response PoC の起動設定接続を行う
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う

### TODO更新
- 完了:
  - UDP socket を auth response PoC の起動処理へ最小接続する
  - `ServerAuthResponsePocStep` 追加
  - receive -> auth flow -> outbound queue -> encoder -> socket send の 1 回分接続
- 追加:
  - auth response PoC の起動設定接続
- 保留:
  - 継続 loop / async runtime
  - retry / fragmentation / encryption
  - JSON Lines 出力本実装
  - heartbeat / video frame 処理本体

### メモ
- auth response PoC 接続の責務は、既存境界を合成して 1 packet の `AuthRequest` に対する encoded `AuthResponse` を同じ UDP socket から 1 回返すところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- `VideoFrame` encode 方針と最小実装範囲を整理した。
- `docs/architecture/protocol.md` に metadata encode 順、`payload_size` の決め方、H.264 bytes をそのまま載せる方針、fixed header + payload bytes の組み立て方を追記した。
- `crates/protocol` に `encode_video_frame` / `encode_video_frame_payload` を追加し、`ProtocolMessageEncoderBoundary` から `ProtocolMessage::VideoFrame` を encode できるようにした。
- `VideoFrame` payload encode、packet encode、`payload_size` mismatch、reserved metadata reject の単体テストを追加した。
- `docs/architecture/system-design.md` と operations docs に現在の encoder support 状態を反映した。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `VideoFrame` encode は frame metadata を docs の payload layout 順に書き、その直後に H.264 encoded bytes を無変換で連結する。
- `payload_size` は `VideoFrame.payload.len()` から決め、`VideoFrame.payload_size` と実 payload 長が一致しない場合は encode error とする。
- fixed header の `payload_length` は metadata と H.264 bytes を含む payload 全体の byte 長とする。
- protocol crate は H.264 圧縮、NAL unit 解釈、fragmentation、retry、encryption、UDP socket send を持たない。

### 未解決事項
- client 側の frame metadata 付与
- H.264 encode 本体
- `VideoFrame` UDP send 接続
- fragmentation / retry / encryption
- server 側 video frame handler / sync buffer 投入

### 次にやる候補
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う
- UDP socket を auth response PoC の起動処理へ接続する

### TODO更新
- 完了:
  - `VideoFrame` encode 方針と最小実装範囲を整理する
  - `VideoFrame` fixed header + payload bytes の最小 encode 実装を追加する
  - `VideoFrame` encode の単体テストを追加する
- 追加:
  - `VideoFrame` UDP send
  - `ClientStats` / `ServerNotice` の payload layout と decode / encode 方針整理
- 保留:
  - H.264 encode 本体
  - fragmentation / retry / encryption
  - video frame handler / sync buffer 投入

### メモ
- `VideoFrame` encode の責務は、typed metadata と既存 H.264 bytes を docs の wire layout どおりに fixed header + payload bytes へ変換するところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- UDP socket 受信 / 送信本体の最小実装を追加した。
- `crates/net-core` に同期 `std::net::UdpSocket` 用の `UdpSocketIoBoundary` と `UdpReceivedPacket` を追加した。
- bind 済み socket から 1 packet を `recv_from` し、受信 bytes と source を `PacketSource` 付きで返せるようにした。
- `EncodedOutboundPacket` の bytes と destination を `send_to` へ渡す最小送信処理を追加した。
- `apps/server` に `ServerUdpSocketIoStep` を追加し、受信した 1 packet を `ServerReceiveLoopStep::handle_received_packet_with_gate` へ接続した。
- docs に receive: socket -> receive loop -> decode -> gate、send: encoded outbound packet -> socket send の現在の実装状態を反映した。

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- UDP socket I/O は同期 `UdpSocket` の 1 datagram adapter として実装する。
- receive adapter は caller-owned buffer を借用し、受信 bytes と source を `UdpReceivedPacket` で返す。
- server adapter は socket I/O と既存 receive loop / gate 境界を接続するだけに留める。
- send adapter は encode 済み `EncodedOutboundPacket` だけを受け取り、typed `ProtocolMessage` は見ない。
- async runtime、retry、fragmentation、encryption、queue runtime、JSON Lines 出力は今回の範囲外とする。

### 未解決事項
- 継続 receive / send loop
- server 起動処理への socket 接続
- retry / fragmentation / encryption
- queue 実処理 / backpressure
- receive / send log writer
- heartbeat / video frame 処理本体

### 次にやる候補
- `VideoFrame` encode 方針と実装範囲を整理する
- secret 解決方式と token 保護方針を設計する
- UDP socket を auth response PoC の起動処理へ接続する

### TODO更新
- 完了:
  - UDP socket 受信 / 送信本体の最小実装を追加する
  - `UdpSocketIoBoundary` / `UdpReceivedPacket` 追加
  - `ServerUdpSocketIoStep` 追加
- 追加:
  - packet 受信継続 loop
  - packet 送信継続 loop
  - UDP socket を auth response PoC の起動処理へ接続する
- 保留:
  - async runtime
  - retry / fragmentation / encryption
  - queue 実処理
  - JSON Lines 出力本実装

### メモ
- UDP socket 最小実装の責務は、1 datagram を受けて既存 receive loop / gate へ渡すこと、または encode 済み bytes を destination へ 1 回 `send_to` することまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- receive rejection の JSON Lines ログイベント仕様を整理した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に receive loop / gate / rejection handoff / JSON Lines event schema / log writer の責務分離を追記した。
- event schema として `event_name`, `run_id`, `client_id`, `source`, `message_type`, `rejection_reason`, `detail`, `timestamp` を整理した。
- `apps/server` に `ServerReceiveRejectionJsonLogEventBoundary` と `ServerReceiveRejectionJsonLogEventInput` を追加し、`ServerPacketLogInput` から future JSON Lines event 入力へ変換できる placeholder を追加した。
- decode error 由来の rejection と `UnauthenticatedSource` / `UnknownClient` / `EndpointMismatch` を区別したまま handoff できる単体テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- receive rejection JSON Lines event name は `server.receive_rejection` とする。
- `run_id`, `client_id`, `message_type` は decode / gate の段階で常に取得できるとは限らないため optional field とする。
- `detail` は decode rejection では `ServerDecodeErrorAction` と `ProtocolError`、acceptance rejection では `PacketAcceptanceRejectReason` を保持する。
- JSON serialization、ファイル出力、packet drop 実行、metrics 更新、UDP socket I/O は今回の範囲外とする。

### 未解決事項
- 実際の JSON Lines 出力本実装
- UDP socket 受信 / 送信
- packet drop 実行
- receive / send log writer
- heartbeat / video frame 処理本体

### 次にやる候補
- UDP socket 受信 / 送信本体の最小実装に進む
- secret 解決方式と token 保護方針を設計する
- receive rejection ログ出力本実装を行う

### TODO更新
- 完了:
  - receive rejection の JSON Lines ログイベント仕様を整理する
  - `ServerReceiveRejectionJsonLogEventBoundary` / `ServerReceiveRejectionJsonLogEventInput` placeholder を追加する
- 追加:
  - receive rejection ログ出力本実装
- 保留:
  - JSON Lines 出力本実装
  - UDP socket 実装
  - packet drop 実行

### メモ
- receive rejection JSON Lines event schema の責務は、rejection handoff の文脈を `server.receive_rejection` event 入力へ変換し、writer がそのまま JSON Lines 化できる typed field set を固定するところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- auth success / failure の JSON Lines ログイベント仕様を整理した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に auth flow / auth log handoff / JSON Lines event schema / log writer の責務分離を追記した。
- event schema として `event_name`, `run_id`, `client_id`, `source`, `accepted`, `reason_code`, `message`, `app_version`, `protocol_version`, `timestamp`, `expected_protocol_version` を整理した。
- `apps/server` に `ServerAuthJsonLogEventBoundary` と `ServerAuthJsonLogEventInput` を追加し、`ServerAuthLogInput` から future JSON Lines event 入力へ変換できる placeholder を追加した。
- success / failure の共通フィールドと、failure detail として使う `message` / `expected_protocol_version` を区別した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- auth JSON Lines event name は `server.auth_result` とする。
- auth JSON Lines event schema 境界は typed auth log input を記録形式の入力へ写すだけに留める。
- log `timestamp` は境界の呼び出し側 / 将来の log layer から明示的に渡し、現在の境界では clock source を持たない。
- JSON serialization、ファイル出力、metrics 更新、UDP socket I/O は今回の範囲外とする。

### 未解決事項
- 実際の JSON Lines 出力本実装
- receive rejection の JSON Lines ログイベント仕様
- receive / send log writer
- UDP socket 受信 / 送信
- heartbeat / video frame 処理本体

### 次にやる候補
- receive rejection の JSON Lines ログイベント仕様を整理する
- UDP socket 受信 / 送信本体の最小実装に進む
- secret 解決方式と token 保護方針を設計する

### TODO更新
- 完了:
  - auth success / failure の JSON Lines ログイベント仕様を整理する
  - `ServerAuthJsonLogEventBoundary` / `ServerAuthJsonLogEventInput` placeholder を追加する
- 追加:
  - なし
- 保留:
  - JSON Lines 出力本実装
  - receive rejection の JSON Lines ログイベント仕様
  - UDP socket 実装

### メモ
- auth JSON Lines event schema の責務は、auth log handoff の文脈を `server.auth_result` event 入力へ変換し、writer がそのまま JSON Lines 化できる typed field set を固定するところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- auth success / failure ログ出力境界を設計した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に auth flow / auth decision / auth log handoff / log layer の責務分離を追記した。
- `apps/server` に `ServerAuthLogHandoffBoundary`, `ServerAuthLogInput`, `ServerAuthLogOutcome` を追加した。
- `ServerAuthDecision` に optional `app_version` を保持できるようにし、auth decision boundary からの decision では decoded `AuthRequest` の `app_version` を引き継ぐようにした。
- `ServerAuthFlowStep` が auth decision から log layer 用 typed input を作り、`ServerAuthFlowOutcome.auth_log_input` に含めるようにした。
- success / failure reason と `client_id` / `run_id` / source / `app_version` / `protocol_version` を保持する単体テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- auth decision は accepted / rejected と reason code を作り、ログ出力そのものは行わない。
- auth log handoff は `ServerAuthDecision` を `ServerAuthLogInput` に変換し、success / failure、reason code、context を保持する。
- `ServerAuthLogInput` は source、`client_id`、`run_id`、optional `app_version`、`protocol_version`、optional message、server time、expected protocol version を持つ。
- JSON Lines 出力、metrics 更新、UDP socket I/O、state 永続化は今回の境界に含めない。

### 未解決事項
- auth success / failure の JSON Lines ログイベント仕様
- JSON Lines 出力本実装
- UDP socket 送受信
- packet 破棄本体
- heartbeat / video frame 処理本体

### 次にやる候補
- auth success / failure の JSON Lines ログイベント仕様を整理する
- receive rejection の JSON Lines ログイベント仕様を整理する
- UDP socket 受信 / 送信本体の実装に進む

### TODO更新
- 完了:
  - auth success / failure ログ出力境界を整理する
  - `ServerAuthLogHandoffBoundary` 追加
  - `ServerAuthLogInput` / `ServerAuthLogOutcome` 追加
- 追加:
  - auth success / failure の JSON Lines ログイベント仕様を整理する
- 保留:
  - JSON Lines 出力本実装
  - UDP socket 実装
  - packet 破棄本体
  - heartbeat / video frame 処理本体

### メモ
- auth log handoff 境界の責務は、auth decision の success / failure と理由、client/run/source/version 文脈を log layer 用 typed input に変換し、実際の JSON Lines 出力は後段に残すところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- packet acceptance rejection を drop / log layer へ渡す境界を設計した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に receive loop / gate / drop layer / log layer の責務分離を追記した。
- `apps/server` に `ServerRejectionDropLogHandoffBoundary` を追加した。
- `ServerReceiveLoopGateRejection` を `ServerRejectionDropLogInput` に変換し、drop input と log input の両方へ同じ rejection reason を渡せるようにした。
- `UnauthenticatedSource` / `UnknownClient` / `EndpointMismatch` / decode error 由来の rejection reason を保持する単体テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- receive loop / gate は rejection decision を作るところまでを担当する。
- `ServerRejectionDropLogHandoffBoundary` は rejection decision を future drop layer と future log layer の typed input に変換する。
- `ServerRejectionHandoffReason` は decode error と acceptance rejection を分け、acceptance 側では `message_type`、optional `client_id`、`PacketAcceptanceRejectReason` を保持する。
- drop 実行、JSON Lines ログ出力、metrics 更新、UDP socket I/O は今回の境界に含めない。

### 未解決事項
- 実際の packet 破棄処理
- receive rejection の JSON Lines ログイベント仕様
- receive loop / packet acceptance rejection のログ出力本実装
- auth success / failure ログ出力
- UDP socket 送受信

### 次にやる候補
- auth success / failure ログ出力境界を設計する
- receive rejection の JSON Lines ログイベント仕様を整理する
- UDP socket 受信 / 送信本体の実装に進む

### TODO更新
- 完了:
  - packet acceptance rejection を drop / log layer へ渡す境界を整理する
  - `ServerRejectionDropLogHandoffBoundary` 追加
  - `ServerRejectionDropLogInput` / `ServerPacketDropInput` / `ServerPacketLogInput` / `ServerRejectionHandoffReason` 追加
- 追加:
  - receive rejection の JSON Lines ログイベント仕様を整理する
- 保留:
  - packet 破棄本体
  - ログ出力本実装
  - UDP socket 実装
  - heartbeat / video frame 処理本体

### メモ
- rejection handoff 境界の責務は、receive loop / gate の rejection decision を drop layer と log layer が使う typed input に変換し、rejection reason を失わず次段へ渡すところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- receive loop から packet acceptance gate を呼ぶ接続境界を設計した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に receive loop -> decode -> gate -> handler / drop の流れを追記した。
- `apps/server` の `ServerReceiveLoopStep` に gate 接続版の `handle_received_packet_with_gate` を追加した。
- accepted route と decode / acceptance rejection を分ける `ServerReceiveLoopGateOutcome` / `ServerReceiveLoopGateRejection` を追加した。
- 登録済み heartbeat が accepted になり、未認証 heartbeat と decode error が drop / log layer 用 decision になる単体テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- receive loop は raw packet decode 成功後に `ServerInboundRouter` で route を作り、その直後に `PacketAcceptanceGateBoundary` を呼ぶ。
- accepted の route だけが将来の handler / router 後続境界へ進む。
- decode error は `ServerRejectedPacket`、gate rejection は `PacketAcceptanceRejection` として分け、将来の drop / log layer へ渡す。
- gate は判定だけを行い、実際の packet 破棄、JSON Lines ログ出力、UDP socket I/O、heartbeat / video frame 処理本体は行わない。

### 未解決事項
- 実際の packet 破棄処理
- receive loop / packet acceptance rejection のログ出力
- auth success / failure ログ出力
- UDP socket 送受信
- timeout / 失効 / 再認証の本実装

### 次にやる候補
- auth success / failure ログ出力境界を設計する
- packet acceptance rejection を drop / log layer へ渡す境界を整理する
- UDP socket 受信 / 送信本体の実装に進む

### TODO更新
- 完了:
  - receive loop から packet acceptance gate を呼ぶ接続境界を整理する
  - `ServerReceiveLoopGateOutcome` / `ServerReceiveLoopGateRejection` 追加
  - receive loop から gate を呼ぶ接続 helper 追加
- 追加:
  - packet acceptance rejection を drop / log layer へ渡す境界を整理する
- 保留:
  - packet 破棄本体
  - receive loop / packet acceptance rejection のログ出力
  - UDP socket 実装
  - heartbeat / video frame 処理本体

### メモ
- receive loop と gate 接続境界の責務は、decode 済み route を handler に渡す前に registry ベースで受理判定し、accepted route または drop / log 用 rejection decision を返すところまで。

---

## 2026-04-18
### 種別
- Codex

### 今回の作業
- 未認証 / endpoint mismatch packet の破棄境界を設計した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に packet acceptance gate の flow と責務分離を追記した。
- `apps/server` に `PacketAcceptanceGateBoundary`, `PacketAcceptanceDecision`, `PacketAcceptanceRejection`, `PacketAcceptanceRejectReason` を追加した。
- registry 参照により `Heartbeat` / `VideoFrame` の `client_id` と source endpoint を受理 / 拒否判定できる最小 helper を追加した。
- `UnauthenticatedSource` / `UnknownClient` / `EndpointMismatch` を区別する単体テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- packet acceptance gate は decode / routing 後、heartbeat / video frame handler の前に置く。
- `AuthRequest` は registry 登録前の認証入口なので registry check を bypass する。
- auth success 後に `AuthenticatedSenderRegistry` へ登録された `client_id` / endpoint のみが client-scoped packet の受理対象になる。
- source endpoint が registry に無い場合は `UnauthenticatedSource`、endpoint は登録済みだが `client_id` が無い場合は `UnknownClient`、`client_id` はあるが endpoint が違う場合は `EndpointMismatch` とする。
- gate は decision を返すだけで、実際の packet 破棄、ログ出力、UDP socket I/O、timeout / 再認証は行わない。

### 未解決事項
- receive loop から packet acceptance gate を呼ぶ接続
- 実際の packet 破棄処理
- 未認証 / endpoint mismatch packet のログ出力
- timeout / 失効 / 再認証の本実装
- UDP socket 送受信

### 次にやる候補
- auth success / failure ログ出力境界を設計する
- receive loop から packet acceptance gate を呼ぶ接続境界を設計する
- UDP socket 受信 / 送信本体の実装に進む

### TODO更新
- 完了:
  - 未認証 / endpoint mismatch packet の破棄境界を整理する
  - `PacketAcceptanceGateBoundary` / `PacketAcceptanceDecision` placeholder 追加
  - registry 参照による packet 受理 / 拒否判定 helper 追加
- 追加:
  - receive loop から packet acceptance gate を呼ぶ接続境界を設計する
- 保留:
  - packet 破棄本体
  - ログ出力本実装
  - UDP socket 実装
  - timeout / 失効 / 再認証

### メモ
- packet acceptance / rejection 境界の責務は、registry を参照して client-scoped packet を handler 前に受理 / 拒否判定し、drop 実行やログ出力へ渡せる decision を作るところまで。

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- 認証済み送信元の登録 / 管理境界を設計した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に accepted auth decision から registry handoff までの流れを追記した。
- `apps/server` に `AuthenticatedSenderRegistry`, `AuthenticatedSenderRegistration`, `AuthenticatedSenderRegistryBoundary`, `AuthenticatedSenderCheck` を追加した。
- accepted decision から registration を作り、`client_id` と source endpoint の対応を in-memory registry に登録できるようにした。
- 後続 packet の `client_id` / source endpoint 受理判定用の最小 lookup を追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- registry は `client_id` と `PacketSource` を対応付ける server 側境界とする。
- `ServerAuthFlowStep` は accepted decision から `AuthenticatedSenderRegistration` を作るが、registry state の永続化や timeout 管理は行わない。
- 後続の heartbeat / video frame 受理判定は、decode 済み `client_id` と packet source endpoint を registry に問い合わせる方針にする。
- missing client / endpoint mismatch は後続 packet の reject/drop 候補とする。
- timeout、失効、再認証、state 永続化、UDP socket 実装は今回行わない。

### 未解決事項
- registry を receive loop / heartbeat / video frame handler に接続する処理
- timeout / 失効 / 再認証の本実装
- auth success / failure ログ出力
- 未認証 / endpoint mismatch packet の破棄ログ
- UDP socket 送受信

### 次にやる候補
- auth success / failure ログ出力境界を設計する
- 未認証 / endpoint mismatch packet の破棄境界を設計する
- UDP socket 受信 / 送信本体の実装に進む

### TODO更新
- 完了:
  - 認証済み送信元の登録 / 管理境界を整理する
  - `AuthenticatedSenderRegistryBoundary` / `AuthenticatedSenderRegistry` placeholder 追加
  - accepted auth decision から registry registration への handoff 追加
- 追加:
  - 未認証 / endpoint mismatch packet の破棄境界を設計する
- 保留:
  - state 永続化
  - timeout / 失効 / 再認証
  - registry と receive loop / packet handler の接続
  - UDP socket 実装

### メモ
- 認証済み送信元 registry 境界の責務は、accepted decision を `client_id` と source endpoint の対応として登録し、後続 packet の受理判定が参照できる最小 lookup を提供するところまで。

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 設定 TOML から client whitelist / token 情報を読み込む最小実装を追加した。
- `crates/config` に最小 auth-section parser を追加し、`ServerAuthConfigBoundary` が TOML file または string から `ServerAuthConfig` を作れるようにした。
- `[auth.clients.<client_id>]` を `AllowedClientConfig` と `SharedTokenConfig` へ変換する実装を追加した。
- `configs/examples/server.example.toml` と整合する読み込みテストを追加した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に auth config 読み込み境界の現在の責務を反映した。

### 変更ファイル
- `crates/config/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- config crate は server TOML の auth client table から typed auth config を作る責務に限定する。
- `[auth.clients.<client_id>]` の table key を whitelisted `client_id` と最小 `shared_token_id` に使う。
- TOML の `shared_token` は PoC 用の `SharedTokenSecretRef::InlinePlaceholder` として保持する。
- 環境変数や secret store からの secret 解決、本物の token 検証、auth state 更新、UDP socket 実装は今回行わない。

### 未解決事項
- secret 解決方式
- secret 解決後の本物の token 検証
- 認証済み送信元の登録 / 管理
- auth success / failure ログ出力
- UDP socket 送受信

### 次にやる候補
- 認証済み送信元の登録 / 管理境界を設計する
- auth success / failure ログ出力境界を設計する
- UDP socket 受信 / 送信本体の実装に進む

### TODO更新
- 完了:
  - server 設定 TOML から client whitelist / token 情報を読み込む
  - client whitelist 読み込みを実装する
  - `configs/examples/server.example.toml` と整合する auth config 読み込みテスト追加
- 追加:
  - secret 解決方式と token 保護方針を設計する
- 保留:
  - secret 解決
  - 本物の token 検証
  - 認証済み送信元登録
  - UDP socket 実装

### メモ
- auth config 読み込みの責務は、server TOML の auth client table を typed whitelist / token config へ変換し、server 側の auth input boundary に渡せる形にするところまで。

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- auth decision から `AuthResponse` outbound queue handoff までの server step を接続した。
- `apps/server` に `ServerAuthFlowStep` / `ServerAuthFlowOutcome` を追加した。
- `ServerAuthFlowStep` が `ServerInboundRoute::AuthRequest` から `ServerAuthCheck`、`ServerAuthCheckInput`、`ServerAuthDecision`、`ServerOutboundAuthResponse`、`OutboundQueueItem` まで既存 boundary を順番に呼ぶようにした。
- accepted / rejected の `AuthResponse` が outbound queue item へ handoff される単体テストを追加した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に server auth flow 接続を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ServerAuthFlowStep` は server 内の orchestration 境界とし、既存 boundary を接続するだけに留める。
- decode 済み `AuthRequest` は `ServerAuthHandlerBoundary` で `ServerAuthCheck` に変換する。
- auth config input boundary は `ServerAuthCheck` と `ServerAuthConfig` から `ServerAuthCheckInput` を作る。
- auth decision boundary は `ServerAuthDecision` を返し、response boundary が `ProtocolMessage::AuthResponse` を作る。
- outbound queue boundary は typed response を `OutboundQueueItem` に変換する。
- 認証済み送信元登録、実 queue、wire encode、UDP socket send、TOML 読み込み、secret 解決は今回実装しない。

### 未解決事項
- server 設定 TOML からの本物の client whitelist 読み込み
- secret 解決
- 認証済み送信元の登録 / 管理
- auth success / failure ログ出力
- outbound queue 実処理
- UDP socket 送受信

### 次にやる候補
- server 設定 TOML から client whitelist / token 情報を読み込む
- 認証済み送信元の登録 / 管理境界を設計する
- auth success / failure ログ出力境界を設計する

### TODO更新
- 完了:
  - auth decision から `AuthResponse` outbound queue handoff までの server step 接続
  - `ServerAuthFlowStep` / `ServerAuthFlowOutcome` 追加
  - server auth flow 接続 docs 反映
- 追加:
  - auth success / failure ログ出力境界を設計する
- 保留:
  - 本物の TOML 読み込み
  - secret 解決
  - 認証済み送信元登録
  - UDP socket 実装

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server auth decision の最小実装を追加した。
- `apps/server` に `ServerAuthDecisionBoundary` を追加し、`ServerAuthCheckInput` から `ServerAuthDecision` を返す流れを実装した。
- `client_id` whitelist、設定入力境界から渡された shared token 情報、提示された `shared_token` を使って accepted / rejected を判定する最小ロジックを追加した。
- `UnknownClient` / `InvalidToken` / `InternalError` の rejected reason を返せるようにした。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に auth decision 境界の責務を反映した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- auth decision は `ServerAuthCheckInput` を入力にし、`ServerAuthDecision` を出力する。
- `client_id` が allowed client に無い場合は `UnknownClient` で rejected にする。
- allowed client の `shared_token_id` に対応する token が無い場合は config 不整合として `InternalError` にする。
- `SharedTokenSecretRef::InlinePlaceholder` は PoC 用の比較可能な token 材料として扱い、一致すれば accepted、不一致なら `InvalidToken` にする。
- `SharedTokenSecretRef::EnvironmentVariable` はまだ secret 解決を実装しないため `InternalError` にする。
- 認証済み送信元登録、`AuthResponse` queue handoff、UDP socket send は既存境界または将来タスクに残す。

### 未解決事項
- server 設定 TOML からの本物の client whitelist 読み込み
- 環境変数などからの secret 解決
- secret 解決後の本物の token 検証
- 認証済み送信元の登録 / 管理
- auth failure / success ログ出力
- UDP socket 送受信

### 次にやる候補
- server 設定 TOML から client whitelist / token 情報を読み込む
- 認証済み送信元の登録 / 管理境界を設計する
- auth decision から AuthResponse outbound queue handoff までの server step を接続する

### TODO更新
- 完了:
  - server auth decision 最小実装
  - `UnknownClient` / `InvalidToken` / `InternalError` rejected reason 追加
  - auth decision 境界 docs 反映
- 追加:
  - 認証済み送信元の登録 / 管理境界を設計する
- 保留:
  - 本物の TOML 読み込み
  - secret 解決
  - UDP socket 実装

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- client whitelist 読み込みと token 検証の設定入力境界を整理した。
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に `config` / server auth handler / auth check input / auth decision の責務分離を追記した。
- `crates/config` に server auth config の placeholder 型と config loading boundary を追加した。
- `apps/server` に decode 済み `AuthRequest` と auth config を `ServerAuthCheckInput` へまとめる境界を追加した。
- 実 TOML 読み込み、secret 解決、token 比較、認証成功 / 失敗判定には進まなかった。

### 変更ファイル
- `apps/server/Cargo.toml`
- `apps/server/src/lib.rs`
- `crates/config/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `config` は許可済み client 一覧と token 参照を保持する設定形状を担当する。
- server auth handler は decode 済み `AuthRequest` と送信元 metadata を `ServerAuthCheck` として保持する。
- `ServerAuthConfigInputBoundary` は `ServerAuthCheck` と `ServerAuthConfig` を受け取り、将来の判定入力 `ServerAuthCheckInput` へ変換する。
- whitelist lookup、token verification、protocol/app version policy、accepted/rejected の生成は auth decision 層に残す。
- `ServerAuthConfigBoundary` は将来の TOML 読み込み境界名だけを固定し、現時点では `NotImplemented` を返す。

### 未解決事項
- server 設定 TOML からの本物の client whitelist 読み込み
- token secret の解決
- token 検証
- 認証成功 / 失敗判定
- 認証済み送信元の登録 / 管理
- UDP socket 送受信

### 次にやる候補
- server auth decision の最小実装を行う
- server auth config の TOML schema と読み込み実装を追加する
- UDP socket receive / send の最小実装へ進む

### TODO更新
- 完了:
  - client whitelist / token 検証の設定入力境界整理
  - `ServerAuthConfigInputBoundary` / `ServerAuthCheckInput` placeholder 追加
  - `ServerAuthConfig` / `AllowedClientConfig` / `SharedTokenConfig` placeholder 追加
- 追加:
  - server 設定 TOML から client whitelist / token 情報を読み込む
- 保留:
  - token 検証
  - 認証成功 / 失敗判定
  - UDP socket 実装

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- outbound queue の最小実処理方針を整理した。
- `docs/architecture/system-design.md` に `ServerOutboundQueueBoundary` から `OutboundQueueItem` が渡され、queue が item を保持して send layer に handoff する流れを追記した。
- encode 前 / encode 後 / send 後の責務境界と、`server` / `outbound queue` / `net send layer` / `socket send` の責務分離を docs に追記した。
- `crates/net-core` に `QueuedOutboundItem`, `OutboundQueueItemState`, `OutboundQueueSendHandoff`, `OutboundQueueLifecycleBoundary` placeholder を追加した。
- 1 item の hold / send-layer handoff / encoded / sent / dropped state の単体テストを追加した。

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- queue は `OutboundQueueItem` を保持し、選択した item を net send layer へ渡す責務に限定する。
- protocol encode は queue handoff 後に net send layer で行う。
- encode 後は `EncodedOutboundPacket` を net send layer / socket send 側が扱い、queue は encoded payload の中身を見ない。
- send 後の成功 / 失敗は将来 queue state へ反映できるが、今回の queue 境界は retry 実行を持たない。
- 現時点の code は 1 item lifecycle placeholder のみで、実 queue、capacity、backpressure、async wakeup、UDP socket send は実装しない。

### 未解決事項
- outbound queue 実処理本体
- queue capacity / backpressure 方針
- async runtime 導入
- UDP socket 送信本体
- retry 実行本体
- fragmentation / encryption

### 次にやる候補
- client whitelist 読み込みと token 検証の設定入力境界を設計する
- server 側の認証成功 / 失敗判定を実装する
- UDP socket 受信 / 送信本体の実装に進む

### TODO更新
- 完了:
  - outbound queue の最小実処理方針整理
  - `QueuedOutboundItem` / `OutboundQueueItemState` / `OutboundQueueLifecycleBoundary` placeholder 追加
- 追加:
  - outbound queue の backpressure / capacity 方針を決める
- 保留:
  - queue 実処理本体
  - async runtime
  - UDP socket send
  - retry 実行
  - fragmentation / encryption

### メモ
- outbound queue の責務は、送るべき `OutboundQueueItem` を保持し、選択した item を net send layer に handoff するところまで。

## 2026-04-17
### 種別
- Codex

### 今回の作業
- UDP socket 送信前の send error / log event 方針を整理した。
- `docs/architecture/system-design.md` に encode 成功後、socket send 前後で扱う error 分類と責務分離を追記した。
- `docs/architecture/protocol.md` に protocol encode 後の send log context は `net-core` が持つ方針を追記した。
- `crates/net-core` に `OutboundSendLogContext`, `SendLogStage`, `SendFailureKind`, `SendFailureDisposition`, `SendLogEvent` placeholder を追加した。
- `run_id` / `client_id` / destination / `message_type` を send log context として抽出する最小実装と単体テストを追加した。

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- send log context は `run_id`, optional `client_id`, destination, `message_type` を基本フィールドにする。
- encode 成功時は encoded byte length を記録できる形にする。
- encode failure / pre-socket failure / socket send failure は `SendLogStage` で区別する。
- `SocketWouldBlock` / `SocketInterrupted` は retry candidate、`EncodeFailed` / `DestinationUnavailable` / `PacketTooLarge` は drop candidate、その他 socket error は warning candidate とする。
- retry 実行、queue mutation、UDP socket send、実ログ出力は今回実装しない。

### 未解決事項
- UDP socket 送信本体
- outbound queue 実処理
- retry 実行本体
- receive / send ログ出力本体
- OS/socket error から `SendFailureKind` への実マッピング
- fragmentation / encryption

### 次にやる候補
- outbound queue の最小実処理を設計する
- client whitelist 読み込みと token 検証の設定入力境界を設計する
- server 側の認証成功 / 失敗判定を実装する

### TODO更新
- 完了:
  - UDP socket 送信前の send error / log event 方針整理
  - `OutboundSendLogContext` / `SendLogEvent` placeholder 追加
  - send failure classification placeholder 追加
- 追加:
  - send error ログ出力を実装する
  - receive / send ログ最小実装
- 保留:
  - UDP socket send
  - queue runtime
  - retry 実行
  - fragmentation / encryption

### メモ
- send error / log event 方針の責務は、送信失敗を分類し、`run_id` / `client_id` / destination / `message_type` 付きで将来 JSON Lines に載せやすい構造にするところまで。

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `HeartbeatAck` encode の最小実装を `crates/protocol` に追加した。
- `HeartbeatAck` payload を docs の順序どおり `client_id`, `run_id`, `echoed_sent_at`, `server_received_at`, `server_sent_at` として byte 化する処理を追加した。
- 既存の 16 byte fixed header encode 補助を再利用し、`ProtocolMessageEncoderBoundary` が `ProtocolMessage::HeartbeatAck` を fixed header + payload bytes に変換するようにした。
- `HeartbeatAck` encode の単体テストを追加した。
- `docs/architecture/protocol.md` / `docs/architecture/system-design.md` と TODO を今回の実装状態に合わせて更新した。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `HeartbeatAck` encode は `crates/protocol` の責務とし、destination metadata、queue、UDP socket send は扱わない。
- fixed header の `message_type` は `HeartbeatAck`、`protocol_version` は `EncodeContext.protocol_version`、`payload_length` は生成した payload byte 数から計算する。
- `client_id` / `run_id` は `u16 byte_length` + UTF-8 bytes とし、timestamp 3項目は `TimestampMicros` の内部値を `u64 little-endian` で encode する。
- `ProtocolMessageEncoderBoundary` は `AuthResponse` と `HeartbeatAck` を encode 対象とし、それ以外の outbound message では引き続き `EncodeNotImplemented` を返す。

### 未解決事項
- UDP socket 送信本体
- outbound queue 実処理
- heartbeat 管理 / timeout 管理
- RTT / offset 推定本体
- `VideoFrame` / `ClientStats` / `ServerNotice` の encode
- retry / fragmentation / encryption

### 次にやる候補
- UDP socket 送信前の send error / log event 方針を整理する
- outbound queue の最小実処理を設計する
- client whitelist 読み込みと token 検証の設定入力境界を設計する

### TODO更新
- 完了:
  - `HeartbeatAck` encode 本実装
  - `HeartbeatAck` encode の単体テスト追加
- 追加:
  - `VideoFrame` encode 方針と実装範囲を整理する
- 保留:
  - UDP socket send
  - queue runtime
  - heartbeat 管理 / timeout 管理
  - RTT / offset 推定
  - retry / fragmentation / encryption

### メモ
- `HeartbeatAck` encode の責務は、typed `HeartbeatAck` message を docs の payload layout に従って fixed header + payload bytes に変換するところまで。

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `HeartbeatAck` の payload byte layout と encode 入力境界を整理した。
- `docs/architecture/protocol.md` に `client_id`, `run_id`, `echoed_sent_at`, `server_received_at`, `server_sent_at` の wire 順序と型を追記した。
- `HeartbeatAck` を server 側 ack boundary から `ProtocolMessage::HeartbeatAck` として net send layer へ渡す流れを docs に反映した。
- `apps/server` に `ServerHeartbeatAckBoundary` / `ServerOutboundHeartbeatAck` / queue handoff placeholder を追加した。
- `HeartbeatAck` 境界の単体テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `HeartbeatAck` payload は fixed header の後ろに `client_id`, `run_id`, `echoed_sent_at`, `server_received_at`, `server_sent_at` の順で置く。
- `client_id` / `run_id` は既存 string 方針どおり `u16 byte_length` + UTF-8 bytes とする。
- timestamp は既存方針どおり `TimestampMicros` 相当の `u64` microseconds とし、wire 上は little-endian とする。
- server 側 ack boundary は、決定済み timestamp 群を typed `ProtocolMessage::HeartbeatAck` と宛先 metadata に変換するだけに留める。
- `HeartbeatAck` の wire encode、heartbeat 管理、timeout 管理、UDP socket send、queue 実処理は今回実装しない。

### 未解決事項
- `HeartbeatAck` encode 本実装
- UDP socket 送信本体
- outbound queue 実処理
- heartbeat 管理 / timeout 管理
- RTT / offset 推定本体
- retry / fragmentation / encryption

### 次にやる候補
- `HeartbeatAck` encode の最小実装を追加する
- UDP socket 送信前の send error / log event 方針を整理する
- outbound queue の最小実処理を設計する

### TODO更新
- 完了:
  - `HeartbeatAck` payload layout / encode 方針整理
  - `HeartbeatAck` encode 入力境界 docs 反映
  - `ServerHeartbeatAckBoundary` / `ServerOutboundHeartbeatAck` placeholder 追加
- 追加:
  - `HeartbeatAck` encode 本実装
- 保留:
  - UDP socket send
  - queue runtime
  - heartbeat 管理 / timeout 管理
  - retry / fragmentation / encryption

### メモ
- `HeartbeatAck` encode 境界の責務は、決定済み ack fields を typed `ProtocolMessage::HeartbeatAck` と宛先 metadata として net send layer へ渡すところまで。

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `AuthResponse` encode の最小実装を `crates/protocol` に追加した。
- `AuthResponse` payload を docs の順序どおり `client_id`, `run_id`, `accepted`, `reason_code`, `message`, `server_time`, `expected_protocol_version` として byte 化する処理を追加した。
- 16 byte fixed header encode の最小補助を追加し、`ProtocolMessageEncoderBoundary` が `ProtocolMessage::AuthResponse` だけを fixed header + payload bytes に変換するようにした。
- `AuthResponse` encode の単体テストを追加した。
- `docs/architecture/protocol.md` と TODO を今回の実装状態に合わせて更新した。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `AuthResponse` encode は `crates/protocol` の責務とし、destination metadata、queue、UDP socket send は扱わない。
- fixed header の `message_type` は `AuthResponse`、`protocol_version` は `EncodeContext.protocol_version`、`payload_length` は生成した payload byte 数から計算する。
- `accepted` は `u8`、`reason_code` は `u16 little-endian`、optional 項目は `u8 present + value` で encode する。
- `ProtocolMessageEncoderBoundary` は `AuthResponse` 以外の outbound message では引き続き `EncodeNotImplemented` を返す。

### 未解決事項
- `HeartbeatAck` / `VideoFrame` / `ClientStats` / `ServerNotice` の encode
- UDP socket 送信本体
- outbound queue 実処理
- 認証成功 / 失敗判定の本実装
- retry / fragmentation / encryption

### 次にやる候補
- `HeartbeatAck` payload layout / encode 方針を整理する
- UDP socket 送信前の send error / log event 方針を整理する
- outbound queue の最小実処理を設計する

### TODO更新
- 完了:
  - `AuthResponse` encode 本実装
  - fixed header encode 本実装
  - `AuthResponse` encode の単体テスト追加
- 追加:
  - なし
- 保留:
  - `AuthResponse` 以外の message encode
  - UDP socket send
  - queue runtime / retry / fragmentation / encryption

### メモ
- `cargo fmt --check` と `cargo check --workspace` は成功。
- `cargo test -p stream-sync-protocol` は MSVC linker `link.exe` が見つからない環境理由で失敗した。

## 2026-04-17
### 種別
- Codex

### 今回の作業
- net send layer から protocol encoder を呼ぶ境界が docs とコードに反映済みであることを確認した。
- `system-design.md` / `protocol.md` の response boundary、net send layer、protocol encoder、socket send の責務分離を確認した。
- `crates/protocol` の `ProtocolMessageEncoderBoundary` と `crates/net-core` の `OutboundPacketEncoderBoundary` が encode 本実装なしの境界 placeholder に留まっていることを確認した。
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。

### 変更ファイル
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- server 側の response boundary と将来の通知系は typed `ProtocolMessage` と宛先 metadata を `OutboundPacket` / `OutboundQueueItem` として net send layer へ渡す。
- net send layer は `ProtocolMessage` と宛先情報を保持し、`EncodeContext` とともに protocol encoder 境界へ handoff する。
- protocol encoder は将来 fixed header + payload bytes を生成する責務を持つが、現時点では `EncodeNotImplemented` placeholder に留める。
- socket send は将来 `EncodedOutboundPacket` の bytes と宛先だけを受け取り、typed message は解釈しない。

### 未解決事項
- `AuthResponse` encode 本実装
- fixed header / payload bytes 生成本体
- UDP socket 送信本体
- outbound queue 実処理
- retry / fragmentation / encryption

### 次にやる候補
- `AuthResponse` encode の最小実装を追加する
- UDP socket 送信前の send error / log event 方針を整理する
- outbound queue の最小実処理を設計する

### TODO更新
- 完了:
  - AuthResponse payload layout / encode boundary 節の「net send layer から protocol encoder を呼ぶ境界を設計する」を完了に更新
- 追加:
  - なし
- 保留:
  - encode 本実装
  - UDP socket send
  - queue runtime / retry / fragmentation / encryption

## 2026-04-17
### 種別
- Codex

### 今回の作業
- net send layer から protocol encoder を呼ぶ境界を設計した。
- `OutboundQueueItem` から `OutboundEncodeRequest` を作り、`MessageEncoder` へ `ProtocolMessage` と `EncodeContext` を渡す placeholder を追加した。
- protocol 側には `ProtocolMessage::message_type()` と、現時点では `EncodeNotImplemented` を返す `ProtocolMessageEncoderBoundary` を追加した。
- docs に response boundary / net send layer / protocol encoder / socket send の責務分離を追記した。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `crates/net-core/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- outbound path は typed `ProtocolMessage` と destination metadata を net send layer へ渡す。
- net send layer は destination metadata を保持したまま protocol encoder 境界を呼ぶ。
- protocol encoder は将来 fixed header + payload bytes を生成する責務を持つが、現時点では placeholder として `EncodeNotImplemented` を返す。
- socket send layer は encode 済み bytes と destination だけを受け取り、typed message を解釈しない。

### 未実装 / 保留
- `AuthResponse` encode 本実装
- fixed header / payload bytes 生成
- UDP socket 送信本体
- outbound queue 実処理
- retry / fragmentation / encryption

### 次にやる候補
- `AuthResponse` encode の最小実装を追加する
- UDP socket 送信前の send error / log event 方針を整理する
- outbound queue の最小実処理を設計する

### TODO更新
- 完了:
  - net send layer -> protocol encoder -> socket send 境界 docs 反映
  - `ProtocolMessageEncoderBoundary` placeholder 追加
  - `OutboundPacketEncoderBoundary` / `OutboundEncodeRequest` / `EncodedOutboundPacket` placeholder 追加
- 追加:
  - `AuthResponse` encode 本実装を行う
  - UDP socket 送信本体を実装する
  - outbound queue 実処理を実装する
- 保留:
  - encode 本実装
  - UDP socket send
  - queue runtime / retry / fragmentation / encryption

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `AuthResponse` の payload byte layout と encode input boundary を整理した。
- `docs/architecture/protocol.md` に `client_id`, `run_id`, `accepted`, `reason_code`, `message`, `server_time`, `expected_protocol_version` の wire 順序と型を追記した。
- `accepted` は `u8` bool、`reason_code` は `u16` little-endian の stable code として固定した。
- `message`, `server_time`, `expected_protocol_version` は `u8 present` tag 付き optional として整理した。
- `crates/protocol` に `AuthResponseReasonCode` の wire code placeholder と reason code 長さ定数を追加した。
- `AuthResponse` は `ProtocolMessage::AuthResponse` のまま `OutboundPacket` へ渡し、wire encode と UDP send は後続層に残す方針を docs に反映した。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `AuthResponse` payload は fixed header の後ろに `client_id`, `run_id`, `accepted`, `reason_code`, `message`, `server_time`, `expected_protocol_version` の順で置く。
- `protocol_version` は fixed header の値を使い、payload には重複して入れない。
- `reason_code` の wire 値は `Ok = 0`, `InvalidToken = 1`, `UnknownClient = 2`, `ProtocolMismatch = 3`, `AlreadyConnected = 4`, `InternalError = 5` とする。
- `expected_protocol_version` は主に `ProtocolMismatch` で present にする想定とし、それ以外では省略してよい。
- 今回は payload layout と encode input boundary の整理までで、byte buffer 生成や UDP 送信は実装しない。

### 未実装 / 保留
- `AuthResponse` encode 本実装
- protocol encoder 呼び出し境界
- UDP socket 送信本体
- outbound queue 実処理
- 認証成功 / 失敗判定本体
- fragmentation / retry / encryption

### 次にやる候補
- net send layer から protocol encoder を呼ぶ境界を設計する
- `AuthResponse` encode の最小実装を追加する
- UDP socket 送信前の send error / log event 方針を整理する

### TODO更新
- 完了:
  - `AuthResponse` payload byte layout docs 反映
  - `accepted` / `reason_code` / optional field wire rule 整理
  - `AuthResponseReasonCode` wire code placeholder 追加
- 追加:
  - net send layer から protocol encoder を呼ぶ境界を設計する
- 保留:
  - `AuthResponse` encode 本実装
  - UDP socket 送信本体
  - queue 実処理 / async runtime

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側の net send layer における outbound packet / queue 境界を設計した
- `ProtocolMessage` と宛先情報を `net-core::OutboundPacket` として保持し、future queue へ渡す `OutboundQueueItem` placeholder を追加した
- `apps/server` に `ServerOutboundQueueBoundary` を追加し、`ServerOutboundAuthResponse` を generic outbound handoff に変換できる形にした
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に server / response boundary / net send layer / socket send の責務分離を追記した

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- outbound send boundary は wire bytes ではなく、typed `ProtocolMessage` と destination metadata を受け取る
- response boundary は message 生成と宛先保持までを担当する
- `net-core` は generic outbound carrier と queue handoff item の形だけを担当する
- 実 queue、wire encode、UDP socket send、retry、fragmentation は後続タスクに残す

### 未実装 / 保留
- outbound queue の実装本体
- encode 本実装
- UDP socket 送信本体
- retry / fragmentation / encryption
- 認証成功 / 失敗判定
- heartbeat / video frame 処理本体

### 次にやる候補
- `AuthResponse` encode 境界と payload byte layout を整理する
- net send layer の encode 呼び出し境界を設計する
- UDP socket 送信本体前の send error / log event 方針を設計する

### TODO更新
- 完了:
  - outbound packet / queue 境界 docs 反映
  - `OutboundPacket` / `OutboundQueueItem` / `OutboundPacketQueueBoundary` placeholder 追加
  - `ServerOutboundQueueBoundary` placeholder 追加
- 追加:
  - outbound queue の実装本体
  - net send layer の encode 呼び出し境界設計
- 保留:
  - encode 本実装
  - UDP socket 送信本体
  - queue 実処理 / async runtime

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `docs/operations/todo.md` を、時系列の追記型から現在位置と次の優先順位が見える構成へ全体整理した。
- 完了済みの細かい作業ログは `docs/operations/session-log.md` に寄せる方針にし、`todo.md` には領域別の現状と未完了項目を残した。
- 決定済み方針、直近でやること、仕様 / 設計、protocol / wire format、net-core / server 境界、認証、heartbeat / 時刻同期、video frame、client、switcher / OBS、ログ / 計測、PoC 最小ライン、後回し項目、ロードマップの順に再編した。
- コードファイルは変更していない。

### 変更ファイル
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `todo.md` は履歴の倉庫ではなく、現在位置と次の順番を示す文書として運用する。
- 詳細な時系列履歴は `session-log.md` を正とする。
- 直近の優先は `AuthResponse` encode、protocol encoder の fixed header / payload byte 生成、`HeartbeatAck` 方針、UDP socket 送受信、server 認証本体とする。

### 未実装 / 保留
- コード変更は今回の対象外。
- `AuthResponse` encode 本実装
- fixed header / payload encode 本実装
- UDP socket 送受信本体
- server 認証成功 / 失敗判定
- heartbeat / timebase / video frame / switcher 実装本体

### 次にやる候補
- `AuthResponse` encode の最小実装を追加する
- fixed header encode / decode roundtrip test を追加する
- UDP socket 送信前の send error / log event 方針を整理する

### TODO更新
- 完了:
  - TODO の構造整理
  - 現在位置と直近優先順位の明確化
  - 領域別タスクへの重複統合
- 追加:
  - PoC に必要な最小ライン
  - protocol encode と UDP PoC 準備を中心にした優先順ロードマップ
- 保留:
  - 実装タスク本体
  - 設計判断の変更
  - コードファイルの変更

### メモ
- 今回は `docs/operations/todo.md` と `docs/operations/session-log.md` のみ変更した。
- 完了 / 未完了の状態は既存 TODO と session-log に記録済みの範囲をもとに整理し、技術スタックや通信方式の変更は行っていない。

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側で `AuthResponse` を生成し、送信レイヤへ渡す境界を設計した
- `ServerAuthDecision` から `ProtocolMessage::AuthResponse` を構築し、宛先 `PacketSource` と一緒に `ServerOutboundAuthResponse` として返す placeholder を追加した
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に `protocol` / server auth handler / response boundary / net send layer の責務分離を追記した

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `protocol` は `AuthResponse` message 型と reason code を持つ
- server auth handler / decision layer は将来、token / `client_id` / `protocol_version` / `app_version` を見て認証結果を返す
- response boundary は認証結果を `AuthResponse` message と送信先 metadata に変換するだけに留める
- wire encode と UDP socket 送信は future net send layer に残す

### 未実装 / 保留
- 認証成功 / 失敗判定の本実装
- client whitelist 読み込み
- 本物の token 検証
- `AuthResponse` encode 本実装
- UDP socket 送信本体
- heartbeat / video frame 処理本体

### 次にやる候補
- net send layer の outbound packet 型 / queue 境界を設計する
- `AuthResponse` payload byte layout と encode 境界を整理する
- server 側の認証状態更新境界を設計する

### TODO更新
- 完了:
  - `AuthResponse` 生成 / 送信境界 docs 反映
  - `ServerAuthDecision` / `ServerAuthResponseBoundary` / `ServerOutboundAuthResponse` placeholder 追加
  - auth decision -> `AuthResponse` -> send layer handoff の流れを定義
- 追加:
  - net send layer の outbound packet 型 / queue 境界を設計する
  - `AuthResponse` encode 本実装を行う
- 保留:
  - 認証成功 / 失敗判定
  - UDP socket 送信本体
  - encode / fragmentation / 再送制御 / 暗号化

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側 UDP 受信 loop の最小設計を行った
- `docs/architecture/system-design.md` に packet bytes 受信、送信元情報取得、`InboundPacket` 生成、decode、router 受け渡しの流れを追記した
- `docs/architecture/protocol.md` に receive loop 境界と decode error / protocol error の分類方針を追記した
- `apps/server` に `ServerReceiveLoopStep` / `ServerReceiveLoopOutcome` / `ServerRejectedPacket` / `ServerDecodeErrorAction` placeholder を追加した
- `ServerReceiveLoopStep` は既に受信済みの packet bytes と `PacketSource` を受け取り、`InboundPacketDecoder` と `ServerInboundRouter` を順番に呼ぶだけに留めた

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- UDP 受信 loop の責務は、packet bytes と送信元情報を受け取り、decode して server route へ渡すところまでに限定する
- `UnsupportedProtocolVersion` は `RejectProtocolVersion` として分類する
- `PayloadDecodeNotImplemented` は `UnsupportedInboundMessage` として分類する
- その他の `ProtocolError` は malformed packet として `DropPacket` に分類する
- socket 実装、非同期 runtime、packet 受信本体、認証判定、heartbeat 管理、video frame 処理本体は今回の範囲外とする

### 未実装 / 保留
- UDP socket の本実装
- 非同期 runtime 導入
- packet 受信本体
- receive loop のログ出力実装
- 認証成功 / 失敗判定の本実装
- heartbeat 管理 / timeout 管理
- video frame 受理 / 同期バッファ投入
- encode 本実装
- fragmentation / 再送制御 / 暗号化

### 次にやる候補
- server 側の認証 handler 境界を設計する
- receive loop のログイベント型を設計する
- UDP socket 実装前の設定値と bind address 方針を決める

### TODO反映
- 完了:
  - server UDP 受信 loop 境界 docs 反映
  - `ServerReceiveLoopStep` placeholder 追加
  - decode error / protocol error の分類方針追加
- 追加:
  - packet 受信本体を実装する
  - receive loop のログ出力方針を実装する
- 保留:
  - UDP socket の本実装
  - 認証 / heartbeat / video frame 処理本体
  - encode / fragmentation / 再送制御 / 暗号化

## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側 handler が `DecodedInboundPacket` を受け取る境界を設計した
- `docs/architecture/system-design.md` に server handler 境界と `AuthRequest` / `Heartbeat` / `VideoFrame` の分岐責務を追記した
- `docs/architecture/protocol.md` に `protocol` / `net-core` / `apps/server` の責務分離を追記した
- `apps/server` に `ServerInboundRouter` / `ServerInboundRoute` placeholder を追加した
- `ServerInboundRouter` は `DecodedInboundPacket` を受け取り、decode 済み message を server 側 route に分類するだけに留めた

### 変更ファイル
- `apps/server/Cargo.toml`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- server は `net-core` から `DecodedInboundPacket` を受け取る
- server 側は `ProtocolMessage` variant、つまり `message_type` 相当の意味を見て処理方針を分岐する
- 認証、heartbeat、video frame の処理責務は server 側に残す
- `protocol` は wire format と decode、`net-core` は raw packet から decode 済み packet 生成、server は app 状態へ反映するための分岐を担当する
- 今回は route 分類だけを置き、認証判定、heartbeat 管理、video frame 処理本体は実装しない

### 未実装 / 保留
- UDP socket 実装
- 認証成功 / 失敗判定の本実装
- heartbeat 管理 / timeout 管理
- video frame 受理 / 同期バッファ投入
- encode 本実装
- `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の decode / encode
- fragmentation / 再送制御 / 暗号化

### 次にやる候補
- server 側の認証 handler 境界を設計する
- heartbeat handler 境界と timeout 管理の最小状態型を設計する
- UDP 受信 loop の最小設計を行う

### TODO反映
- 完了:
  - server handler 境界 docs 反映
  - `ServerInboundRouter` / `ServerInboundRoute` placeholder 追加
  - `AuthRequest` / `Heartbeat` / `VideoFrame` の route 分類
- 追加:
  - 認証成功 / 失敗判定の本実装
  - heartbeat 管理 / timeout 管理の本実装
  - video frame 受理 / 同期バッファ投入の本実装
- 保留:
  - UDP socket 実装
  - encode 本実装
  - fragmentation / 再送制御 / 暗号化

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `net-core` と `protocol` の受信 decode 境界を設計した
- `docs/architecture/system-design.md` に raw packet bytes 受領から decode 済み message を app / server handler へ渡すまでの責務分担を追記した
- `docs/architecture/protocol.md` に fixed header decode -> protocol_version check -> payload decoder dispatch -> app 受け渡しの順序を反映した
- `crates/protocol` に `decode_payload_by_message_type` を追加し、既存の `AuthRequest` / `Heartbeat` / `VideoFrame` payload decoder を message type で dispatch できるようにした
- `crates/net-core` に `InboundPacket`, `PacketSource`, `InboundPacketDecoder`, `DecodedInboundPacket`, `NetDecodeError` の最小境界型を追加した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `crates/net-core/Cargo.toml`
- `crates/net-core/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `net-core` は raw packet bytes と送信元 metadata を受け取り、protocol crate の decode entry point を順番に呼ぶ橋渡しに留める
- fixed header decode、protocol_version 期待値チェック、payload decoder dispatch は protocol crate の責務とする
- decode 成功時は `DecodedInboundPacket` として送信元 metadata と `ProtocolMessage` を app / server handler 側へ返す
- UDP socket loop、送信処理、app handler 実行、認証済み client 管理は今回の範囲外とする

### 未実装 / 保留
- UDP socket 実装
- server / client / switcher 側 handler 実装
- encode 本実装
- `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の decode / encode
- fragmentation / 再送制御 / 暗号化

### 次にやる候補
- UDP 受信 loop の最小設計を行う
- server 側 handler が `DecodedInboundPacket` を受け取る境界を設計する
- `AuthResponse` / `HeartbeatAck` の payload byte layout を決める

### TODO反映
- 完了:
  - `net-core` / `protocol` の受信 decode 境界 docs 反映
  - `decode_payload_by_message_type` の追加
  - `net-core` の最小 decode 境界型追加
- 追加:
  - UDP socket 実装
  - server / client / switcher 側 handler 実装
- 保留:
  - encode 本実装
  - fragmentation / 再送制御 / 暗号化

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に `VideoFrame` payload decode の最小実装を追加した
- `VideoFramePayloadDecoder` / `decode_video_frame_payload` を追加し、fixed header decode と protocol_version 期待値チェック後に payload 部分を型へ落とす入口を用意した
- `client_id`, `run_id`, 46 byte numeric metadata, H.264 bytes を docs の byte layout どおりに読む処理を追加した
- `payload_size` と実際の残り H.264 byte 数の整合、不正 bool、不正 `metadata_reserved`、未対応 codec を最小 error として返すようにした
- `docs/architecture/protocol.md` と TODO を今回の実装状態に合わせて更新した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `VideoFrame` payload decode は metadata と H.264 bytes の境界確認までを protocol crate の責務とする
- H.264 bytes は中身を解釈せず、`payload_size` と残り byte 数が一致した場合にだけ `Vec<u8>` として復元する
- `metadata_reserved` は初期 wire format では全 byte `0` のみ受理する
- encode、UDP 通信、app handler、fragmentation / 再送制御 / 暗号化は今回の範囲外とする

### 未実装 / 保留
- encode 本実装
- UDP 通信実装
- server / client / switcher 側 handler 実装
- `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の payload layout と decode 方針
- fragmentation / 再送制御 / 暗号化

### 次にやる候補
- encode API の最小実装範囲を決める
- `AuthResponse` / `HeartbeatAck` の payload byte layout を決める
- `net-core` 側で fixed header decode と payload decoder を呼ぶ境界を設計する

### TODO反映
- 完了:
  - `VideoFrame` payload decode の最小実装
  - `payload_size` と H.264 bytes の境界検証
  - `VideoFrame` decode 実装状態の docs 反映
- 追加:
  - `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の payload layout と decode 方針を決める
- 保留:
  - encode 本実装
  - UDP 通信実装
  - app handler 実装

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に `Heartbeat` payload decode の最小実装を追加した
- docs の payload byte layout に従い、`client_id`, `run_id`, `sent_at`, `local_time`, `short_status` を復元できるようにした
- `local_time` を `optional<u64>` から `Option<TimestampMicros>` として、`short_status` を `optional<string>` から `Option<String>` として decode するようにした
- 不正 payload 長、未期待 message type、不正 optional tag の単体テストを追加した
- `docs/architecture/protocol.md` と TODO を今回の実装状態に合わせて更新した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `Heartbeat` payload decode は fixed header decode と protocol_version 期待値チェックが済んだ後に呼ぶ前提とする
- protocol crate は payload を `Heartbeat` 型へ落とす責務までに留める
- 生存確認更新、timeout 判定、RTT 計算、認証済み client 管理は app / server 側の責務とする
- `VideoFrame` / `AuthResponse` / encode / UDP / app handler は今回の範囲外とする

### 未解決事項
- `VideoFrame` payload decode の最小実装
- `AuthResponse` / `HeartbeatAck` / `ClientStats` / `ServerNotice` の payload layout と decode 方針
- encode 本実装
- UDP 通信、server / client / switcher handler 実装

### 次にやる候補
- `VideoFrame` payload decode の最小実装範囲を決める
- `AuthResponse` / `HeartbeatAck` payload byte layout を決める
- protocol decode 結果を server 側 handler に渡す境界を設計する

### TODO更新
- 完了:
  - `Heartbeat` payload decode の最小実装
  - optional timestamp / optional string decode
  - 不正 payload に対する最小 error と単体テスト
- 追加:
  - `VideoFrame` payload decode の最小実装
  - encode 本実装
- 保留:
  - UDP 通信と app handler 実装
  - fragmentation / 再送制御 / 暗号化

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に `AuthRequest` payload decode の最小実装を追加した
- docs の payload byte layout に従い、`client_id`, `run_id`, `app_version`, `shared_token`, `display_name` を復元できるようにした
- 可変長 string を `u16 byte_length` + UTF-8 bytes として読み、`display_name` は `u8 present` + optional string として読めるようにした
- 不正 payload 長、invalid UTF-8、不正 optional tag、想定外 message type の最小 error と単体テストを追加した
- `docs/architecture/protocol.md` と TODO を今回の実装状態に合わせて更新した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `AuthRequest` payload decode は fixed header decode と protocol_version 期待値チェックが済んだ後に呼ぶ前提とする
- protocol crate は payload を `AuthRequest` 型へ落とすだけに留め、認証成功 / 失敗判定は持たない
- 初期 wire layout に無い `capabilities` は空配列、`requested_video_profile` は `None` として復元する
- `Heartbeat` / `VideoFrame` / encode / UDP / app handler は今回の範囲外とする

### 未解決事項
- `Heartbeat` payload decode の最小実装
- `VideoFrame` payload decode の最小実装
- encode 本実装
- UDP 通信、server / client / switcher handler 実装

### 次にやる候補
- `Heartbeat` payload decode の最小実装を追加する
- `AuthRequest` decode 結果を server 側認証処理へ渡す境界を決める
- `AuthResponse` payload byte layout と decode / encode 方針を決める

### TODO更新
- 完了:
  - `AuthRequest` payload decode の最小実装
  - 可変長 string / optional string decode
  - 不正 payload に対する最小 error と単体テスト
- 追加:
  - `Heartbeat` payload decode の最小実装
  - `VideoFrame` payload decode の最小実装
- 保留:
  - encode 本実装
  - UDP 通信と app handler 実装

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に `protocol_version` 期待値チェックの最小実装を追加した
- fixed header decode 後の `FixedHeader.protocol_version` と `DecodeContext.expected_protocol_version` を照合できるようにした
- 不一致時に `ProtocolError::UnsupportedProtocolVersion` を返す単体テストを追加した
- `docs/architecture/protocol.md` に fixed header decode 後 / payload decode 前に検証する実装状態を反映した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `protocol_version` の期待値は app 側が `DecodeContext` として渡す
- protocol crate は fixed header の値を比較し、error を返す判定ロジックだけを持つ
- `protocol_version` 検証は fixed header decode 後、payload decode 前に行う
- payload の意味解釈、UDP 通信、app handler 側の接続拒否変換は今回の範囲外とする

### 未解決事項
- payload decode / encode の本実装
- app / server / client / switcher 側で protocol error を接続拒否や packet 破棄へ変換する処理
- AuthResponse / HeartbeatAck / ClientStats / ServerNotice の payload byte layout

### 次にやる候補
- AuthRequest payload decode の最小実装範囲を決める
- Heartbeat payload decode の最小実装範囲を決める
- app handler 側で `UnsupportedProtocolVersion` をどう扱うか決める

### TODO更新
- 完了:
  - protocol_version 期待値チェックの最小実装
  - fixed header decode 後 / payload decode 前の検証方針の docs 反映
- 追加:
  - server / client / switcher 側の handler で protocol error を接続拒否や破棄へ変換する
- 保留:
  - payload decode / encode の本実装

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `AuthRequest` / `Heartbeat` / `VideoFrame` の payload byte layout を設計した
- `docs/architecture/protocol.md` に各 payload のフィールド順、wire type、可変長 field の長さ情報を追記した
- `VideoFrame` の frame metadata と H.264 payload bytes の境界を明記した
- `crates/protocol` に payload layout 共有用の最小定数を追加した

### 変更ファイル
- `docs/architecture/protocol.md`
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- payload 内の数値は fixed header と同じく little-endian とする
- string は `u16 byte_length` + UTF-8 bytes とする
- optional field は `u8 present` の後に値を置く形式とする
- `VideoFrame` は `client_id` / `run_id` の後に 46 byte の numeric metadata を置き、その直後に `payload_size` byte の H.264 bytes を置く
- H.264 bytes には追加の長さ prefix を置かず、直前の `payload_size` で境界を決める

### 未解決事項
- payload decode / encode の本実装
- AuthResponse / HeartbeatAck / ClientStats / ServerNotice の payload byte layout
- UDP 通信、server / client / switcher handler、fragmentation / 再送制御 / 暗号化

### 次にやる候補
- AuthRequest payload decode の最小実装範囲を決める
- Heartbeat payload decode の最小実装範囲を決める
- VideoFrame metadata decode と H.264 bytes 境界検証の最小実装範囲を決める

### TODO更新
- 完了:
  - AuthRequest / Heartbeat / VideoFrame payload byte layout の docs 反映
  - 可変長 string / optional / bytes の長さ情報方針の明記
  - VideoFrame metadata と payload 境界の明記
- 追加:
  - AuthResponse / HeartbeatAck / ClientStats / ServerNotice の payload byte layout
- 保留:
  - payload decode / encode の本実装
  - UDP 通信実装
  - fragmentation / 再送制御 / 暗号化

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` に 16 byte fixed header decode の最小実装を追加した
- `message_type`, `header_length`, `protocol_version`, `payload_length`, `flags`, `reserved` を little-endian で読むようにした
- fixed header decode の責務を `docs/architecture/protocol.md` に反映した
- TODO を fixed header decode 完了状態へ更新した

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- fixed header decode は 16 byte fixed header の構造確認と raw payload slice の切り出しまでを責務にする
- `header_length` は現時点では `FIXED_HEADER_LEN` と一致する場合のみ受理する
- 未知の `message_type`、短すぎる packet、`payload_length` と実 byte 数の不一致は `ProtocolError` として返す
- `protocol_version` の期待値チェックと payload の意味解釈は fixed header decode では行わない

### 未解決事項
- payload decode / encode の本実装
- message ごとの payload byte layout 詳細
- UDP 通信、server / client / switcher handler、fragmentation / 再送制御 / 暗号化

### 次にやる候補
- `AuthRequest` / `Heartbeat` / `VideoFrame` の payload byte layout を決める
- payload decode / encode の単体テスト方針を決める
- fixed header encode の最小実装要否を判断する

### TODO更新
- 完了:
  - fixed header decode の最小実装
  - fixed header decode の docs 反映
- 追加:
  - payload decode / encode の本実装
  - message ごとの payload byte layout 詳細
- 保留:
  - UDP 通信実装
  - server / client / switcher handler 実装
  - fragmentation / 再送制御 / 暗号化

---

## 2026-04-17
### 種別
- Codex

### 今回の作業
- `crates/protocol` における encode / decode API 境界を設計した
- `docs/architecture/protocol.md` に fixed header decode、message dispatch、payload decode、encode、protocol_version check の位置を追記した
- protocol crate、`net-core`、app 側の責務分離を整理した
- `crates/protocol` に API 境界用の placeholder 型、trait、error 型を追加した

### 変更ファイル
- `docs/architecture/protocol.md`
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- protocol crate は message 型、wire layout、decode / encode の入口境界、wire error 型を持つ
- UDP socket、送受信 loop、address、fragmentation / retry は protocol crate に入れない
- `protocol_version` の期待値は app 側が決め、payload decode 前に検証する
- fixed header decode は packet 構造確認と payload slice の切り出しまでに限定する
- payload decode は `message_type` による分岐後の入口として扱う
- encode は 1 packet buffer 作成までを protocol crate の境界とし、送信処理は `net-core` 側に置く

### 未解決事項
- fixed header decode の本実装
- payload decode / encode の本実装
- message ごとの payload byte layout 詳細
- UDP 通信実装と server / client / switcher handler 実装

### 次にやる候補
- fixed header decode の最小実装を追加する
- `AuthRequest` / `Heartbeat` / `VideoFrame` payload layout を決める
- encode / decode の単体テスト方針を決める

### TODO更新
- 完了:
  - encode / decode API 境界の docs 反映
  - API 境界用 placeholder trait / enum / error 型の追加
- 追加:
  - fixed header decode の本実装
  - payload decode / encode の本実装
- 保留:
  - UDP 通信実装
  - server / client / switcher handler 実装
  - fragmentation / 再送制御 / 暗号化

---

## 2026-04-16
### 種別
- Codex

### 今回の作業
- PoC / MVP 初期で使う最小 wire format の byte layout を設計した
- `docs/architecture/protocol.md` に 16 byte fixed packet header と可変長 payload 方針を追記した
- `message_type`, `protocol_version`, `payload_length` の扱いを整理した
- `AuthRequest` と `VideoFrame` の共通ヘッダ化範囲を fixed packet header までに限定した
- `crates/protocol` に header length / offset 定数と `FixedHeader` placeholder を追加した

### 変更ファイル
- `docs/architecture/protocol.md`
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 初期 fixed packet header は 16 byte とする
- offset 0 に `message_type: u16`、offset 4 に `protocol_version: u32`、offset 8 に `payload_length: u32` を置く
- 数値フィールドは little-endian とする
- `payload_length` は fixed header を含まない payload byte 数とする
- 可変長 payload の中身は `message_type` ごとに定義する
- `client_id` / `run_id` / timestamp / frame metadata は初期 fixed header に入れず、payload 側に置く

### 未解決事項
- encode / decode 本実装
- payload 内の各 message byte layout の詳細
- fragmentation / 再送制御 / 暗号化
- UDP 通信実装と server / client / switcher handler 実装

### 次にやる候補
- payload 内の `AuthRequest` / `Heartbeat` / `VideoFrame` metadata layout を詰める
- encode / decode API の境界だけ設計する
- 1人送信・受信・表示 PoC の準備に進む

### TODO更新
- 完了:
  - 最小 wire format byte layout の docs 反映
  - fixed header 定数と placeholder 追加
- 追加:
  - encode / decode 本実装
  - UDP / handler / fragmentation などの未実装項目
- 保留:
  - payload 内の message 別 byte layout 詳細
  - fragmentation / 再送制御 / 暗号化

---

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

---

## 2026-04-16 23:23
### 種別
- Codex

### 今回の作業
- `crates/protocol` に MVP 通信基盤向けの基本識別型を追加
- 認証メッセージ `AuthRequest` / `AuthResponse` を Rust 型として定義
- heartbeat メッセージ `Heartbeat` / `HeartbeatAck` を Rust 型として定義
- message type 表現と認証応答 reason code を enum として定義
- `cargo check --workspace` が通ることを確認

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ClientId`, `RunId`, `AppVersion` は文字列 newtype として定義
- `ProtocolVersion` は整数 newtype として定義
- 本作業では serde 等のシリアライズ / デシリアライズ実装には進まない
- UDP 通信、handler、server / client / switcher 側の実装には進まない

### 未解決事項
- timestamp の単位と wire format は未確定
- `capabilities` / `requested_video_profile` の詳細構造は未確定
- VideoFrame / ClientStats / ServerNotice は未実装
- シリアライズ / デシリアライズ方針は未整理

### 次にやる候補
- 共通型のシリアライズ / デシリアライズ方針を整理する
- VideoFrame の最小構造を定義する
- 1人送信・受信・表示 PoC の着手準備をする

### TODO更新
- 完了:
  - 共通型定義を作る
  - 認証メッセージ形式を定義する
  - heartbeat メッセージ形式を定義する
  - `protocol_version` の共通定義を作る
  - `run_id` の共通定義を作る
  - 認証メッセージに `protocol_version` / `app_version` を含める
- 追加:
  - なし
- 保留:
  - シリアライズ / デシリアライズ処理
  - server 側の `protocol_version` 検証処理
  - app_version 差異時の warn ログ実装

### メモ
- `cargo check --workspace` は成功。
- docs と実装のズレとして、timestamp の単位は docs 側でもまだ詳細未確定のため、現時点では `u64` に留めた。

---

## 2026-04-16 23:39
### 種別
- Codex

### 今回の作業
- `crates/protocol` に映像送信用の最小構造 `VideoFrame` を追加
- 補助メッセージ `ClientStats` / `ServerNotice` を追加
- `MessageType` に `VideoFrame` / `ClientStats` / `ServerNotice` を追加
- 関連 enum として `Codec` / `NoticeType` を追加
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `VideoFrame` は MVP の最小構造に留め、payload は `Vec<u8>` として定義
- timestamp は既存 protocol crate の流儀に合わせて `u64` のまま扱う
- `Codec` は MVP 方針に合わせて現時点では `H264` のみ定義
- `ClientStats` は最小項目として `capture_fps` / `dropped_frames` / `bitrate_kbps` に絞る

### 未解決事項
- payload fragmentation / 再送制御は未実装
- シリアライズ / デシリアライズ方針は未整理
- `payload_size` と `payload.len()` の検証処理は未実装
- `ClientStats` の詳細項目と送信間隔は未確定

### 次にやる候補
- 共通型のシリアライズ / デシリアライズ方針を整理する
- protocol_version チェック方針を整理する
- 1人送信・受信・表示 PoC の着手準備をする

### TODO更新
- 完了:
  - VideoFrame の最小構造を定義する
  - stats用メッセージを定義する
  - 直近項目から VideoFrame の最小構造定義を外す
- 追加:
  - protocol_version チェック方針を整理する
- 保留:
  - シリアライズ / デシリアライズ処理
  - UDP 通信実装
  - server / client / switcher 側 handler 実装

### メモ
- docs と実装のズレとして、`VideoFrame` の任意フィールド `encode_duration_ms` / `color_format` / `profile_name` は MVP 最小構造から外した。
- `ClientStats` の docs 上の任意フィールドも、今回の最低限項目以外は未実装に留めた。

---

## 2026-04-16 23:43
### 種別
- Codex

### 今回の作業
- protocol timestamp の単位をマイクロ秒に統一
- `crates/protocol` に `TimestampMicros` newtype を追加
- 既存メッセージ型の timestamp 関連フィールドを `TimestampMicros` に変更
- `docs/architecture/protocol.md` に timestamp 単位と clock domain 方針を追記
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- protocol timestamp の単位はマイクロ秒とする
- Rust 側表現は `TimestampMicros(pub u64)` とする
- client 側 timestamp と server 側 timestamp は、それぞれの clock domain の値として扱う
- PoC / MVP では単調増加する時計を優先し、Unix epoch 固定は wire format 確定時に再検討する

### 未解決事項
- wire format 上のバイト列や endian は未確定
- clock source の具体実装は timebase / client / server 実装時に決める
- timestamp の妥当性検証、補正、offset 推定処理は未実装

### 次にやる候補
- 共通型のシリアライズ / デシリアライズ方針を整理する
- protocol_version チェック方針を整理する
- timestamp を使った RTT / offset 推定の型境界を整理する

### TODO更新
- 完了:
  - timestamp の単位と Rust 表現を整理する
- 追加:
  - なし
- 保留:
  - シリアライズ / デシリアライズ処理
  - UDP 通信実装
  - server / client / switcher 側 handler 実装

### メモ
- docs とコードのズレだった timestamp 単位未確定状態を解消した。
- `AuthResponse.server_time`, `Heartbeat.sent_at`, `Heartbeat.local_time`, `HeartbeatAck` の時刻群、`VideoFrame.capture_timestamp`, `VideoFrame.send_timestamp`, `ClientStats.sent_at` を `TimestampMicros` に変更した。

---

## 2026-04-16 23:47
### 種別
- Codex

### 今回の作業
- `docs/architecture/protocol.md` にシリアライズ / デシリアライズ方針を追記
- PoC / MVP の wire format 方針を、バイナリ寄りの独自形式として整理
- `protocol_version` と `message_type` を payload decode 前に読む方針を明記
- `MessageType` に初期 wire 識別子を割り当て
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- PoC / MVP では JSON ではなく、バイナリ寄りの独自 wire format を前提にする
- 完全な byte layout はまだ固定せず、最小 envelope の設計を次段階に残す
- envelope には最低限 `protocol_version` と `message_type` を含め、payload decode 前に検査する
- 数値型は実装時に little-endian へ統一する方針とする
- 未知の `message_type` や protocol mismatch は decode 失敗または packet 破棄として扱う

### 未解決事項
- encode / decode trait と実装は未追加
- 最小 wire format の byte layout は未確定
- fragmentation / 再送制御 / 暗号化は未設計
- payload 長や必須フィールドの具体的な検証実装は未着手

### 次にやる候補
- protocol_version チェック方針を整理する
- 最小 wire format の byte layout を設計する
- 1人送信・受信・表示 PoC の着手準備をする

### TODO更新
- 完了:
  - 共通型のシリアライズ / デシリアライズ方針を整理する
  - 直近項目からシリアライズ / デシリアライズ方針整理を外す
- 追加:
  - 最小 wire format の byte layout を設計する
- 保留:
  - シリアライズ / デシリアライズ処理の本格実装
  - UDP 通信実装
  - server / client / switcher 側 handler 実装

### メモ
- `crates/protocol` 側は `MessageType` の `#[repr(u16)]` と数値割り当てのみ追加し、encode / decode 本体は実装していない。
## 2026-04-17
### 種別
- Codex

### 今回の作業
- server 側の認証 handler 境界を設計した
- `ServerInboundRouter` が認識した `AuthRequest` route を auth handler boundary へ渡す形を追加した
- `ServerAuthHandlerBoundary` / `ServerAuthCheck` / `ServerAuthBoundaryError` placeholder を `apps/server` に追加した
- `docs/architecture/system-design.md` と `docs/architecture/protocol.md` に `protocol` / `net-core` / `ServerInboundRouter` / auth handler の責務分離を追記した

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `protocol` は wire decode と `AuthRequest` payload decode までを担当し、認証ビジネスロジックは持たない
- `net-core` は raw packet bytes と source metadata から `DecodedInboundPacket` を作る橋渡しに留める
- `ServerInboundRouter` は `AuthRequest` を認証 route として分類するだけに留める
- auth handler boundary は decoded `AuthRequest` から `shared_token` / `client_id` / `protocol_version` / `app_version` などの認証判定入力を準備する
- 認証結果による server 状態更新、認証済み送信元登録、`AuthResponse` 生成 / 送信は auth handler boundary の外側に残す

### 未実装 / 保留
- 認証成功 / 失敗判定の本実装
- client whitelist 読み込み
- 本物の token 検証
- `AuthResponse` 生成 / 送信境界
- UDP socket 実装
- heartbeat 管理 / timeout 管理
- video frame 受理 / 同期バッファ投入

### 次にやる候補
- `AuthResponse` 生成 / 送信境界を設計する
- client whitelist と token 検証の設定入力境界を設計する
- heartbeat handler 境界と timeout 管理の最小設計を行う

### TODO更新
- 完了:
  - server 認証 handler 境界 docs 反映
  - `ServerAuthHandlerBoundary` / `ServerAuthCheck` placeholder 追加
  - `AuthRequest` route から認証判定入力を準備する境界追加
- 追加:
  - 認証成功 / 失敗判定の本実装
  - client whitelist 読み込み
  - 本物の token 検証
  - `AuthResponse` 生成 / 送信境界設計
- 保留:
  - UDP socket 実装
  - heartbeat / video frame 処理本体
  - encode / fragmentation / 再送制御 / 暗号化

---
