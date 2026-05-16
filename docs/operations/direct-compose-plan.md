<!-- stream-sync/docs/operations/direct-compose-plan.md -->

# Direct 1280x720 Compose Plan

最終更新: 2026-05-17

この文書は、switcher の current `2560x1440 -> 1280x720` half-scale path を「速くする」のではなく、「OBS clean output 向けには縮小そのものを不要にする」ための docs-first 設計メモです。  
この step では code change は行わず、`apps/switcher/src/lib.rs` の composition boundary と `apps/switcher/src/main.rs` の OBS-friendly render/materialization path をまたぐ最小 follow-up の形だけを固定します。

## 目的
- latest rerun `manual-logs/two-client-render-rerun-20260517-004552` では direct `1280x720` compose が two-real clean output path に効き、`render_buffer_half_scale_count=0`、`render_buffer_same_size_copy_count=32`、`render_buffer_scale_loop_elapsed_ms=0` になった
- render materialization は大幅に軽くなった一方で、残りの主課題は decoder-side follow-up へ移った
- half-scale row/chunk fast path の regression は direct compose で迂回できたため、次の docs-first slice は direct compose を広げる前に decoder-side bottleneck を整理する
- branch diagnostics、materialization diagnostics、source-only `selected_source` churn fix は維持する

## 現在の path
1. `apps/switcher/src/lib.rs`
   - `SwitcherFourViewQuadCompositionBoundary::compose_fixed_quad_view` が `compose_four_view_quad_canvas` を呼ぶ
   - `four_view_quad_slot_size` は renderable slot の最大 width / height を採用する
   - current `1280x720` decoded frame では `slot_width=1280`、`slot_height=720` になり、composed canvas は `2560x1440`
   - canvas 全体を `placeholder_bgra=[16,16,16,255]` で埋め、renderable slot だけ `copy_bgra_frame_into_canvas` で native size copy する
2. `apps/switcher/src/lib.rs`
   - `SwitcherFourViewComposedCanvasWindowRenderBoundary::render_ready_quad_view` は composed frame metadata を validate したうえで `frame.pixels` を render input に載せる
3. `apps/switcher/src/main.rs`
   - `ObsFriendlyFourViewLoopWindowRenderRuntime` が clean output 向けに `scale_four_view_bgra_to_obs_validation_profile_from_slice` を通す
   - current dominant branch は `2560x1440 -> 1280x720` half-scale
   - half-scale semantics は `2x2` block の左上 pixel を採る nearest-neighbor

## 現在の visual semantics
- slot placement は固定 `2x2`
- slot rect は `row * slot_height` / `column * slot_width`
- renderable slot は scale せず native frame をそのまま slot rect に copy
- placeholder / source-error / no-frame slot は background placeholder 色のまま残る
- current OBS half-scale helper は composed `2560x1440` frame を `1280x720` へ縮小するとき、各 `2x2` block の左上 pixel を採る

## direct compose で守ること
- source-only `selected_source` churn fix を壊さない
- materialization reason diagnostics を壊さない
- branch diagnostics を壊さない
- placeholder behavior を壊さない
- `4`-client all-real path を壊さない
- focused / four-real preview path を壊さない
- existing `2560x1440` compose path を壊さない
- OBS clean output の visual semantics は current half-scale と一致させる

## 最小設計方針
- existing `2560x1440` compose path は残す
- direct compose は additive path として追加する
- direct compose の対象は first step では OBS validation profile 用 `1280x720` clean output に限定する
- generic N-view refactor にはしない
- direct compose は current half-scale semantics と一致するように、各 slot を `640x360` 相当へ書く
- `1280x720` output 上の slot placement は current `2560x1440` composed canvas を half-scale した見た目に合わせる

## 提案する path shape
1. `lib.rs` に existing composed-canvas path とは別の OBS-specific direct compose boundary を追加する
   - 名前は実装時に最小でよいが、役割は「4-view state から `1280x720` BGRA を直接 materialize する」こと
2. current `slot_width * 2` / `slot_height * 2` compose を置き換えるのではなく、clean output path だけ opt-in で direct compose を使う
3. `main.rs` の OBS-friendly runtime には、可能なら direct-compose 済み `1280x720` input を渡す
4. その場合、render runtime 側の branch diagnostics は `passthrough` または `same-size copy` へ寄る想定で、`half_scale` は減る

## direct compose の具体的な意味
- current composed frame が `2560x1440` で、その half-scale 結果が `1280x720`
- よって direct compose は「`2560x1440` canvas を作ってから half-scale する」のではなく、「各 renderable slot を最初から `640x360` slot へ書く」
- ただし visual semantics は current half-scale helper と合わせるため、slot 内縮小でも各 `2x2` source block の左上 pixel を採る必要がある
- これは current helper の pixel rule を slot copy 側へ移すだけで、補間品質を変える設計ではない

## 最小実装 slice
1. `lib.rs` に OBS clean output 専用の direct-compose helper / boundary を追加する
2. 対象は current `4`-view fixed layout のみ
3. first step では current `1280x720 x 4` 入力から `1280x720` output を作るケースだけを主対象にする
4. `main.rs` の two-real clean output loop で、その additive path を opt-in 利用する
5. existing composed-canvas render path、focused preview path、four-real path は据え置く
6. branch/materialization diagnostics は current summary field を維持したまま比較可能にする

## 段階案
### Phase 1
- direct compose を two-real clean output path のみに接続する
- current `2560x1440` compose path は全維持
- rerun で `render_buffer_half_scale_count` が減るか確認する

### Phase 2
- same semantics を `4`-client all-real clean output path に広げられるか確認する
- この時点でも focused preview / generic path の統合は急がない

## リスク
- `lib.rs` composition boundary と `main.rs` render/materialization boundary をまたぐため、境界責務が曖昧になるリスクがある
- current render-facing path は composed frame metadata と actual pixels の整合チェックを持つため、parallel path を入れると metadata mismatch の扱いを整理する必要がある
- placeholder slot が current background fill と完全一致しないと visual parity を崩す
- direct compose を generic 化しすぎると、今回の narrow goal を超えて `4`-view orchestration 全体を触る危険がある

## 実装前に確認すること
- new path の output ownership をどこに置くか
- clean output only で呼ぶ boundary をどこに差し込むか
- current focused tests に加え、どの parity test を追加するか
- rerun 比較で見る summary field
  - `render_buffer_half_scale_count`
  - `render_buffer_passthrough_count`
  - `render_buffer_same_size_copy_count`
  - `render_buffer_scale_loop_elapsed_ms`
  - `effective_render_fps_after_first_render`

## 今回の結論
- direct `1280x720` compose は、current `2560x1440` composed canvas path を置き換えるのではなく、OBS clean output 向けの additive path として入れるのが最小
- first implementation slice は two-real clean output path 限定で十分
- generic refactor や backend 変更に進まず、next step は「最小 additive path 実装 + rerun 比較」に留める

## 実装メモ
- 2026-05-17 の Phase 1 実装は `apps/switcher/src/main.rs` の two-real clean output compose helper に landed した
- existing `apps/switcher/src/lib.rs` の full-size fixed quad composition boundary はそのまま残している
- current additive path は full-size intermediate を作らず、fixed `2x2` layout と current nearest-neighbor semantics を保ったまま OBS profile `1280x720` を直接 materialize する

## 実装後の確認
- latest same-PC `2`-client rerun `manual-logs/two-client-render-rerun-20260517-004552` で direct compose の有効性を確認した
- runtime summary では `render_buffer_half_scale_count=0` / `render_buffer_same_size_copy_count=32` / `render_buffer_scale_loop_elapsed_ms=0` となり、half-scale branch は消えた
- render materialization も `avg_render_elapsed_ms=1.143` / `render_elapsed_ms=120` まで軽くなった
- `decode_elapsed_ms=2180` / `decode_output_read_elapsed_ms=1654` / `decode_output_buffer_reuse_count=0` から、次のフォローアップは decoder-side と整理する
- direct compose は two-real clean output path で有効、4-client 展開は次段階の候補として残す
