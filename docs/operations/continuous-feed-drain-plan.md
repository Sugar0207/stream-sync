<!-- stream-sync/docs/operations/continuous-feed-drain-plan.md -->

# Slot0 Continuous Feed / Drain Plan

最終更新: 2026-05-19

## 目的
- slot0 continuous decoder を render-demand selected-frame feed から切り離し、per-client stream として連続 access unit を feed する最小方針を整理する
- latest decoded fallback ではなく、decoder input が selected frame に追いつけない構造を先に直す
- first implementation slice は slot0 / two-real preview loop / opt-in continuous decoder に限定する
- exact selected frame lookup と one-shot fallback は維持し、古い decoded frame の誤表示を避ける

## 背景 evidence
- latest rerun:
  - `S:\stream-sync\manual-logs\two-client-render-rerun-20260519-171331`
- low-latency / probe args variant により continuous stdout output は PARTIAL PASS:
  - `continuous_decode_output_frame_count=11`
  - `continuous_decode_queue_len=11`
  - `continuous_decode_stdout_first_byte_seen=true`
  - `continuous_decode_first_input_to_first_output_elapsed_ms=5322`
- render consumption は FAIL:
  - `render_used_continuous_decoded_count=0`
  - `continuous_decode_lookup_hit_count=0`
  - `continuous_decode_fallback_to_one_shot_count=15`
- stale decoded frame / lag:
  - `continuous_decode_requested_frame_id=535`
  - `continuous_decode_latest_decoded_frame_id=386`
  - `continuous_decode_requested_minus_latest_lag=149`
  - `continuous_decode_frame_id_lag=173`
  - `continuous_decode_stale_frame_available_count=11`
- sparse render-demand feed:
  - `continuous_decode_input_frame_id_min=4`
  - `continuous_decode_input_frame_id_max=535`
  - `continuous_decode_input_frame_id_gap_max=66`
  - `continuous_decode_input_frame_id_gap_total=531`
  - `continuous_decode_input_non_consecutive_count=14`

## current render-demand feed の問題
- current path:
  - scheduler / targetTime source が current tick の selected encoded frame を決める
  - decode hook が selected frame の exact cache key を作る
  - slot0 continuous 対象なら、まず decoded output を drain して exact key を探す
  - exact miss の場合だけ、その selected access unit を continuous input queue へ enqueue する
  - 同 tick で exact key がまだ無ければ one-shot fallback へ進む
- 問題:
  - feed が render request の副作用なので、continuous decoder は render が欲しい frame を後追いで受け取る
  - selected frame が `4 -> 535` のように飛ぶと、continuous input は連続 stream ではなく sparse samples になる
  - decoded output が数 tick 後に出ても、render requested frame はすでに先へ進んでいるため exact lookup が当たらない
  - current output queue に stale decoded frames があっても、latest decoded fallback を使うと古い映像を表示する危険がある
  - one-shot fallback は安全だが、continuous path の hot path 化にはつながっていない

## policy goal
- feed は render-demand decode miss ではなく、slot0 configured `client_id + run_id` の per-client stream cursor として扱う
- render loop は continuous decoder に「今欲しい frame を今投げる」のではなく、「すでに近い frame が decoded cache にあるか」を見る
- continuous decoder runtime は targetTime を決めない
- targetTime selection / stale policy / placeholder policy は switcher display side に残す
- first slice の success criterion は latest decoded fallback 表示ではなく、exact selected frame lookup hit が出始めることと one-shot fallback が減ること

## handoff/source からの連続 access unit 取得
- first design は switcher-side feed helper とする
  - server / client / protocol code は変更しない
  - server push stream や queue snapshot hot path はまだ作らない
  - existing named-pipe handoff / `SwitcherQueuedFrameHandoff` abstraction を使う
- feed helper は slot0 configured source だけを対象にする
  - `client0_id + run0_id`
  - slot1 / second real source は current one-shot path のまま
- feed read mode の第一候補:
  - `PreviewOldest` で head frame を観測する
  - feed cursor より古い / 重複 frame は enqueue せず、必要なら bounded drop candidate として記録する
  - enqueue する場合は `ConsumeOldest` で同じ head frame を進める案を第一候補にする
- `PreviewLatest` / `PreviewLatestDecodable` は render selection には有用だが、continuous feed の stream continuity には弱い
  - latest 系は中間 frame を飛ばしやすい
  - first feed/drain policy では、decoder input continuity を優先して oldest-driven feed を候補にする
- targetTime より未来の frame:
  - feed helper は decoder warmup のために未来 frame を enqueue してよいかをまだ確定しない
  - first implementation は safer に、oldest head が targetTime より未来の場合は feed skip / waiting として扱う候補を優先する
  - 未来 frame を先読み feed する案は、B-frame / reorder / future-frame display risk とは別に、queue mutation risk があるため保留する

## render loop と feeder の resource 競合
- current named-pipe handoff は one request / one response で、render selection も feed helper も同じ server queue source を読む可能性がある
- first design の競合方針:
  - render selection is authoritative for display
  - feeder is opportunistic and bounded
  - feeder must not block render loop
  - feeder must not consume a frame that render selection still needs unless policy explicitly accepts that queue mutation
- safest first option:
  - feed helper runs before render selection with a small per-tick budget
  - feed helper uses `PreviewOldest` first
  - it consumes only frames that are at or before the current targetTime and are not already fed
  - render selection continues to use the configured preview mode for display
- risk:
  - consuming oldest for feed can remove frames before render selection sees them
  - this is acceptable only if consumed frames are older than or equal to targetTime and are intended for decoder warmup, while render display still requires exact decoded key or one-shot fallback
- alternative held:
  - add a non-mutating feed read mode or queue snapshot
  - useful later, but out of scope for first docs-first slice because it likely touches server/handoff protocol or hot-path response shape

## queue / backpressure / drop policy
- feed helper queue:
  - no separate large queue in first slice if existing continuous input channel can be used directly
  - feed helper should maintain lightweight state:
    - latest accepted feed frame_id
    - skipped duplicate count
    - skipped old count
    - source waiting/no-frame/error counts
- continuous input queue:
  - remains bounded
  - first candidate bound remains near existing continuous decode queue bound, e.g. `30` frames
  - `try_send` failure must not block render
- drop policy:
  - duplicate frame_id: skip
  - frame_id lower than latest accepted feed frame_id: skip as old
  - input queue full: drop/skip feed candidate and count `feed_input_queue_full_drop_count`
  - runtime disabled: skip feed and count disabled skip
  - handoff error: do not convert to no-frame; count explicit source error and leave render path unchanged
- decoded output queue/cache:
  - keep existing bounded decoded cache/key order
  - old decoded frames may be discarded for memory pressure
  - discard must not imply they were safe to display

## feed cadence
- first cadence:
  - run once per preview-loop frame, before validation/decode/render path
  - drain continuous outputs before and after feed where the current runtime can do so without blocking
  - enqueue at most a small bounded batch per tick
- initial batch proposal:
  - `max_feed_per_tick=2` or `3` for slot0 only
  - never unbounded catch-up in one render tick
  - if target lag remains high and pipe/write diagnostics stay healthy, a later step can raise the bound
- feed stop conditions per tick:
  - source no-frame
  - source waiting because head frame is after targetTime
  - handoff/source error
  - duplicate/old frame observed
  - continuous input queue full
  - per-tick feed budget exhausted
- do not add a separate feeder thread in first implementation
  - a synchronous bounded helper is easier to reason about
  - it avoids new lifetime / cancellation / pipe concurrency policy

## frame_id / targetTime / exact lookup
- frame_id:
  - feed cursor uses monotonic `frame_id` per `client_id + run_id`
  - gaps are diagnostic, not automatic fatal errors
  - repeated large gaps mean source read mode is still effectively sparse
- targetTime:
  - render targetTime remains the display-side selection input
  - feed helper may use targetTime only to avoid consuming/fetching frames too far in the future
  - decoder runtime does not choose display eligibility
- exact lookup:
  - first implementation keeps exact selected `client_id + run_id + frame_id` cache hit as the only continuous render consumption path
  - success is measured by `continuous_decode_lookup_hit_count` and `render_used_continuous_decoded_count`
- latest decoded fallback:
  - explicitly held
  - latest decoded frame can lag requested frame by `149` frame ids in current evidence
  - using it without a staleness guard risks showing old video and undermines sync priority
- targetTime-aware decoded queue lookup:
  - held for a later policy step
  - it must define max staleness, no-future-frame guard, and display policy interaction before it can render decoded frames

## one-shot fallback
- one-shot fallback remains mandatory
- first feed/drain implementation should still do:
  - exact continuous decoded cache lookup
  - if miss, one-shot fallback
- fallback reasons should remain visible:
  - exact miss
  - output pending
  - frame_id lagging
  - queue empty
  - runtime disabled
  - feed skipped / feed source error
- do not reduce safety by replacing fallback with stale latest decoded display

## diagnostics
- feed/source counters:
  - `continuous_feed_enabled`
  - `continuous_feed_attempt_count`
  - `continuous_feed_source_read_count`
  - `continuous_feed_frame_accepted_count`
  - `continuous_feed_frame_skipped_duplicate_count`
  - `continuous_feed_frame_skipped_old_count`
  - `continuous_feed_source_no_frame_count`
  - `continuous_feed_source_waiting_count`
  - `continuous_feed_source_error_count`
  - `continuous_feed_input_queue_full_drop_count`
- feed identity / lag:
  - `continuous_feed_last_requested_target_time`
  - `continuous_feed_latest_accepted_frame_id`
  - `continuous_feed_last_seen_frame_id`
  - `continuous_feed_frame_id_gap_max`
  - `continuous_feed_frame_id_gap_total`
  - `continuous_feed_non_consecutive_count`
- queue / cadence:
  - `continuous_feed_batch_size_max`
  - `continuous_feed_batch_size_last`
  - `continuous_feed_elapsed_ms`
  - `continuous_feed_budget_exhausted_count`
  - `continuous_feed_stop_reason_counts`
- render correlation:
  - keep existing `continuous_decode_requested_frame_id`
  - keep existing `continuous_decode_latest_decoded_frame_id`
  - keep existing `continuous_decode_requested_minus_latest_lag`
  - keep existing `continuous_decode_lookup_hit_count`
  - keep existing `render_used_continuous_decoded_count`
  - keep existing `continuous_decode_fallback_to_one_shot_count`

## first implementation slice
- scope:
  - slot0 only
  - two-real preview loop only
  - requires `--enable-continuous-stream-decoder`
  - may keep `--continuous-decoder-low-latency-args` as opt-in runtime variant
  - slot1 remains one-shot
  - 4-client path remains unchanged
- implementation shape:
  - add a loop-local slot0 feed helper before validation/decode/render
  - feed helper reads at most a small bounded batch from handoff/source
  - feed helper enqueues accepted access units into the existing slot0 continuous runtime input path
  - exact lookup and one-shot fallback remain unchanged
  - diagnostics are added before tuning behavior
- not included:
  - latest decoded fallback
  - targetTime-aware decoded render consumption
  - separate feeder thread
  - server queue snapshot
  - protocol or named-pipe DTO changes
  - slot1 continuous decode
  - 4-client continuous decode
  - GPU decode

## open questions for implementation step
- Should feed consume oldest frames, or should it require a non-mutating source read first?
- Should feed skip future frames entirely, or allow small future predecode while preventing display?
- Is `max_feed_per_tick=2` enough to reduce lag, or should the first slice expose it as an internal constant only?
- Should continuous runtime expose an enqueue-only method that does not also perform render lookup side effects?
- Which diagnostics should be added to the existing summary line first without making it too noisy?

## readiness
- Production Readiness remains FAIL
- This plan is a first implementation boundary, not a production architecture
- Success for the next code slice means:
  - feed diagnostics prove slot0 is no longer sparse render-demand only
  - continuous decoded latest frame stays closer to requested frame
  - at least some exact lookup hits appear
  - one-shot fallback remains available and visible
