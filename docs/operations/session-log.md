<!-- stream-sync/docs/operations/session-log.md -->

## 2026-05-07
### Type
- Codex

### Work
- Implemented the first continuous receive/send runtime code slice on the
  server side only.
- Added a new command:
  - `stream-sync-server --receive-send-runtime-bounded [config-path] [max-iterations] [receive-timeout-ms]`
- Kept the implementation bounded and thin:
  - one bound UDP socket across loop turns
  - one `AuthenticatedSenderRegistry` across loop turns
  - one `ServerOutboundQueueCollection` across loop turns
  - caller-owned writers across loop turns
  - repeated reuse of existing
    `ServerControllerReceiveSendRuntimeBoundary`
- Added the bounded runtime aggregate summary fields:
  - `command_name`
  - `config_path`
  - `max_iterations`
  - `receive_timeout_ms`
  - `iterations_attempted`
  - `iterations_completed`
  - `auth_requests_received`
  - `auth_responses_sent`
  - `heartbeats_received`
  - `heartbeat_acks_sent`
  - `client_stats_received`
  - `client_stats_returns_sent`
  - `accepted_packets`
  - `rejected_packets`
  - `decode_errors`
  - `send_failures`
  - `outbound_queue_len`
  - `registered_clients`
  - `stop_reason`
- Kept scope intentionally narrow:
  - no protocol change
  - no switcher change
  - no client change
  - no retry/requeue
  - no file sink open/rotation
  - no process-wide logger
  - no continuous video path
  - no OBS WebSocket
  - no GUI/operator split
  - no daemon lifecycle work
- Added code-level tests for:
  - command parser
  - summary formatter
  - `max_iterations` stop
  - repeated auth registry persistence
  - repeated heartbeat existing-registry reuse
  - `ClientStats` observation path count
  - one-iteration runtime non-regression

### Changed Files
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Treat the new bounded repeated runtime as the implemented first slice of the
  continuous receive/send phase.
- Keep the next step on manual validation of repeated auth / heartbeat /
  `ClientStats` flow before widening into later service/runtime concerns.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test -p stream-sync-server receive_send -- --test-threads=1`
- `cargo test --workspace`
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Reconfirmed the post-closeout baseline from repo state plus user-provided
  status:
  - `4`-client bounded real encoded video PoC validated
  - raw-key operator wrapper validated
  - `AllView` / `Focused(0..3)` / `AllView` return validated
  - raw console restore validated
  - `[video.encoder]` profile wiring validated
  - production H.264 stdout visibility validated
  - short OBS Window Capture validation complete
  - final regression passed
  - push completed
- Documented the boundary between current completed scope and future scope.
- Designed the next-phase continuous receive/send runtime first slice as a
  bounded server-owned repeated runtime rather than as a full daemon/service.
- Chose the smallest first runtime shape:
  - one process lifetime
  - one bound UDP socket
  - one authenticated sender registry
  - one in-memory outbound queue collection
  - one repeated call site for the existing
    `ServerControllerReceiveSendRuntimeBoundary`
  - bounded stop policy using `max_iterations` and `receive-timeout-ms`
- Documented the bounded-PoC-to-continuous-runtime differences:
  - repeated lifetime ownership instead of per-launcher reset
  - bounded outer loop instead of one/few explicit launcher calls
  - aggregate runtime summary instead of command-only summaries
  - no retry/requeue/daemon lifecycle in the first slice
- Separated first-slice blockers from later non-blockers.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Start the continuous runtime phase on the server side only.
- Keep the first slice bounded and small; do not widen immediately into
  switcher continuous runtime, OBS control, hardware encoder work, or daemon
  lifecycle.

### Validation
- docs-only design update
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Reorganized closeout docs only, without code changes, around:
  - OBS Window Capture-oriented operations guidance
  - final regression checklist
  - non-blocking known later polish
  - push judgment prerequisites
- Added concrete OBS-oriented operations guidance for the current bounded path:
  - `window_title=StreamSync 4-view Output`
  - `output_width=1280`
  - `output_height=720`
  - manual `Window Capture` as the current OBS integration mode
  - `AllView` / `Focused(0..3)` / `AllView` return checks
  - issue classification for black screen, placeholder, focused-view stuck,
    and `AllView` not returning
  - raw-key wrapper operation notes
  - double-`Q` guarded quit note
  - bounded `max_requests` headroom note for lingering server windows/processes
  - bounded PoC vs future continuous runtime boundary
- Added a final regression checklist covering:
  - `cargo fmt --check`
  - `cargo check --workspace`
  - `cargo test --workspace`
  - `git diff --check`
  - raw-key wrapper smoke summary
  - encoder-profile stdout evidence
  - OBS visual checklist
  - confirmation that known later polish remains non-blocking
- Reclassified known later polish explicitly as future work rather than
  closeout blockers:
  - same-session bounded server lifecycle polish
  - transient scheduler-status wobble
  - wrapper stdin zero-gap wobble
  - long-running quality / block-noise / latency evaluation
  - hardware encoder integration
  - full GUI / `apps/operator-wrapper` split
  - OBS WebSocket / advanced OBS control
  - continuous receive/send runtime
- Added push-judgment prerequisites to docs:
  - final regression green
  - closeout docs updated
  - current MVP scope and future scope separated

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Keep the current closeout pass documentation-only.
- Treat later polish and future runtime expansion as tracked future work, not
  as blockers for the current closeout decision.

### Validation
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Recorded the successful `[video.encoder]` profile manual rerun without code
  changes.
- Captured that all `4` manual client configs (`client.player1..4`) reflected
  the configured encoder profile and FFmpeg visibility in stdout:
  - `encoder_backend=ffmpeg_libx264`
  - `encoder_width=1280`
  - `encoder_height=720`
  - `encoder_fps=30`
  - `encoder_bitrate_kbps=4500`
  - `encoder_gop_frames=30`
  - `encoder_preset=ultrafast`
  - `encoder_tune=zerolatency`
  - `encoder_pixel_format=yuv420p`
  - `encoder_profile=main`
  - `encoder_level=3.1`
  - `ffmpeg_path=ffmpeg`
  - `ffmpeg_version_detected=ffmpeg version 8.1-full_build-www.gyan.dev`
  - `ffmpeg_preflight_error=none`
  - `ffmpeg_spawn_error=none`
  - `frames_captured=2`
  - `frames_encoded=2`
  - `frames_sent=2`
  - `encode_failures=0`
  - `frame_build_failures=0`
  - `send_failures=0`
  - `last_encode_error=none`
  - `last_ffmpeg_error=none`
  - `last_payload_len=65363`
  - `oversized_payload_count=0`
  - `fragmentation_pressure_count=2`
- Recorded the same bounded rerun summaries:
  - switcher final:
    - `commands_processed=9`
    - `commands_rejected=0`
    - `frames_rendered=40`
    - `render_failures=0`
    - `scheduler_status=AllSelected`
    - `slot_result_kinds=Selected|Selected|Selected|Selected`
    - `clean_output_render_result_kind=Rendered`
    - `exit_reason=QuitRequested`
  - wrapper final:
    - `input_source=raw_keys`
    - `keys_processed=10`
    - `commands_sent=9`
    - `ignored_keys=0`
    - `raw_console_restore_result=restored`
    - `raw_console_restore_error=none`
    - `exit_reason=QuitRequested`
- Recorded human visual confirmation:
  - `AllView` was visible
  - `1` / `2` / `3` / `4` switched to `player1..4`
  - `0` returned to `AllView`
  - `a` kept `AllView`
  - OBS / Window Capture showed no black frame
- Recorded scope notes:
  - server used `max_requests=240` headroom, so remaining alive after switcher
    quit is not failure for this run
  - switcher final ended around `request_id` `160`, so request-budget headroom
    explains the remaining server lifetime
  - this run is evidence for profile wiring, FFmpeg/stdout visibility, and
    short visual switching only
  - longer-run quality, block noise, and latency remain future continuous
    runtime validation topics
- Updated docs to treat:
  - `[video.encoder]` profile wiring as manual-evidence complete
  - production H.264 stdout visibility as MVP-evidence complete
  - the next task as OBS Window Capture-oriented operations guidance, final
    regression, and push judgment

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Close the current encoder-profile wiring evidence pass based on successful
  manual stdout confirmation across all `4` clients plus preserved switcher /
  wrapper / visual output behavior.
- Keep long-duration quality/latency evaluation out of this closeout and defer
  it to future continuous runtime validation.

### Validation
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Recorded the actual rerun and human visual confirmation after the AllView
  visual mismatch fix for `--four-view-operator-wrapper --raw-keys`.
- Captured the rerun control results:
  - `s -> status` returned:
    - `current_view_state=AllView`
    - `view_render_mode=AllView`
    - `output_layout=QuadView`
    - `rendered_slot_count=4`
    - `focused_slot_index=none`
    - `all_view_render_result_kind=Rendered`
  - `1` / `2` / `3` / `4` returned focused states with:
    - `view_render_mode=Focused`
    - `output_layout=FocusedFullWindow`
    - `rendered_slot_count=1`
    - `focused_slot_index=0..3`
    - `clean_output_render_result_kind=Rendered`
  - `0 -> all` returned:
    - `transition_result=Transitioned`
    - `current_view_state=AllView`
    - `view_render_mode=AllView`
    - `output_layout=QuadView`
    - `rendered_slot_count=4`
    - `focused_slot_index=none`
    - `all_view_render_result_kind=Rendered`
  - `a -> all` returned:
    - `transition_result=NoChange`
    - `current_view_state=AllView`
    - `output_layout=QuadView`
    - `rendered_slot_count=4`
  - `q` / `q` still returned `QuitRequested`
  - wrapper final summary returned:
    - `input_source=raw_keys`
    - `raw_console_restore_result=restored`
    - `raw_console_restore_error=none`
    - `exit_reason=QuitRequested`
- Recorded human visual confirmation:
  - after `0`, the visible output returned to the 4-view quad
  - after `a`, the visible output stayed on the 4-view quad
  - OBS / Window Capture showed no black frame on this path
- Updated docs to treat:
  - the AllView visual mismatch as fixed
  - the raw-key operator wrapper / AllView / Focused / quit / console restore
    slice as complete for the current MVP operator surface
- Moved the next task back to:
  - encoder profile manual evidence
  - production H.264 stdout visibility

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Close the AllView visual mismatch based on both actual rerun evidence and
  human visual confirmation, not only code-level diagnostics.
- Treat the current raw-key operator surface as sufficiently validated for the
  MVP slice and return focus to encoder-profile manual evidence.

### Validation
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Investigated the operator-wrapper visual mismatch reported after the successful
  `--four-view-operator-wrapper --raw-keys` validation:
  - `0` / `A` returned `mapped_command=all`
  - control response returned `current_view_state=AllView`
  - control response returned `clean_output_render_result_kind=Rendered`
  - visible output still looked focused instead of returning to the 4-view quad
- Reviewed the controlled loop path from `all` command parsing through:
  - `SwitcherFourViewControlledPreviewViewState`
  - `render_four_view_controlled_state_for_ticks`
  - `SwitcherFourViewCleanOutputWindowBoundary`
  - the Windows persistent window runtime
- Confirmed the code already selected the `AllView` render branch, then added
  narrower diagnostics so the render mode/layout are visible instead of only
  the high-level state string.
- Added controlled-loop diagnostics to command/loop summaries and control-pipe
  responses:
  - `view_render_mode`
  - `output_layout`
  - `rendered_slot_count`
  - `focused_slot_index`
  - `all_view_render_result_kind`
- Added focused code-level coverage for `Focused(3) -> AllView` using a
  recording persistent window runtime plus per-client frame colors so the test
  verifies the second render request is a quad-view surface, not another
  focused full-window frame.
- Updated the Windows persistent render path to invalidate the full client area
  on each repaint request instead of invalidating a zero-sized rect.
- Kept scope narrow:
  - no GUI expansion
  - no wrapper/control-pipe protocol redesign
  - no OBS control changes
  - no raw-console-restore rollback

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Treat the observed `AllView` mismatch as a render-surface/update-path issue,
  not as a raw-key, parser, or control-pipe problem.
- Make controlled-loop render layout explicit in stdout diagnostics so future
  operator sessions can distinguish `QuadView` from `FocusedFullWindow`
  directly from response lines.
- Use full-client invalidation in the Windows persistent renderer as the
  smallest redraw-side correction that stays within the existing architecture.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1`
- `cargo test --workspace`
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Narrow-polished the switcher operator wrapper raw-key path after the recorded
  successful `--four-view-operator-wrapper --raw-keys` validation still showed
  a post-exit child window hang.
- Reworked the raw-key runtime so `RawKeys` setup now yields both:
  - a reader
  - a restore tracker
- Added a dedicated Windows RAII console-mode restore guard and moved the
  actual `SetConsoleMode(original_mode)` restore attempt into `Drop` instead of
  silently relying on the reader struct alone.
- Made the wrapper raw-key loop explicitly drop the raw-key reader before
  returning its final loop summary, then inspect the restore tracker so success
  summaries can report restore status and restore failures stay explicit.
- Extended wrapper loop summary output with:
  - `raw_console_restore_result`
  - `raw_console_restore_error`
- Kept scope narrow:
  - no changes to the switcher controlled loop protocol
  - no changes to the control-pipe command vocabulary
  - no changes to `--keys` scripted mode
  - no changes to one-line stdin mode
- Expanded focused wrapper tests to cover:
  - raw-key restore success
  - raw-key setup failure
  - guarded quit with restore
  - unknown key with restore
  - control-pipe send failure with restore
  - explicit restore failure surfacing

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Treat Windows console-mode restore as wrapper-local lifecycle ownership for
  the optional raw-key path rather than as an implicit side effect.
- Surface raw console restore failure explicitly instead of silently ignoring
  it, while preserving the validated `--keys` and one-line stdin baselines.
- Keep the polish bounded to operator-wrapper raw input lifecycle only; do not
  expand into GUI, control-loop protocol, OBS control, or switcher transport
  changes.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1`
- `cargo test --workspace`
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Investigated the post-encoder-wiring workspace test failure in the server
  handoff summary tests.
- Confirmed the runtime summary implementation was already using the current
  field semantics:
  - `queue_len_before_read`
  - `queue_len_after_read`
  - `frame_payload_len`
- Updated the stale test expectations in
  `server_handoff_summary_includes_frame_read_fields` and
  `server_handoff_bounded_summary_includes_aggregate_and_request_fields` so
  they assert the current summary field names instead of the old
  `queue_len` / `encoded_payload_len` shape.
- Kept the fix limited to server test expectations; no server runtime,
  switcher, protocol, or client encoder wiring behavior was changed.

### Changed Files
- `apps/server/src/main.rs`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- The current server handoff summary formatter is the source of truth for the
  queue-length and payload-length field names.
- The failing tests were stale rather than evidence of a handoff runtime
  regression.
- Client encoder profile config wiring remains unchanged by this fix.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test --workspace`
- `cargo test -p stream-sync-client client_video_frame -- --test-threads=1`
- `cargo test -p stream-sync-server server_handoff --bin stream-sync-server -- --test-threads=1`
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Implemented the first client encoder profile config wiring slice for the real
  encoded video path.
- Added optional client TOML parsing for `[video.encoder]` with:
  - `backend`
  - `ffmpeg_path`
  - `width`
  - `height`
  - `fps`
  - `bitrate_kbps`
  - `gop_frames`
  - `preset`
  - `tune`
  - `pixel_format`
  - `profile`
  - `level`
- Preserved fallback behavior when `[video.encoder]` is absent:
  - keep the current implicit `ffmpeg` / `libx264` / `ultrafast` /
    `zerolatency` / `yuv420p` direction
  - keep capture-size output instead of forcing `1280x720`
  - keep bitrate / GOP / profile / level implicit
- Wired encoder config into the real encoded FFmpeg/libx264 invocation:
  - optional `scale=width:height`
  - explicit `fps`
  - explicit `bitrate_kbps`
  - explicit `gop_frames` with fixed-keyframe settings
  - explicit output pixel format / profile / level
- Updated encoded metadata shaping so configured output width / height / FPS
  are reflected in `VideoFrame` metadata instead of always reusing raw capture
  dimensions.
- Added bounded sender stdout visibility for:
  - `encoder_backend`
  - `encoder_width`
  - `encoder_height`
  - `encoder_fps`
  - `encoder_bitrate_kbps`
  - `encoder_gop_frames`
  - `encoder_preset`
  - `encoder_tune`
  - `encoder_pixel_format`
  - `encoder_profile`
  - `encoder_level`
  - `ffmpeg_path`
  - `ffmpeg_version_detected`
  - `ffmpeg_preflight_error`
  - `ffmpeg_spawn_error`
  - `last_encode_error`
  - `last_ffmpeg_error`
  - `last_payload_len`
  - `oversized_payload_count`
  - `fragmentation_pressure_count`
- Added one-shot FFmpeg preflight probing and per-run FFmpeg runtime
  visibility capture for the default software encoder path.
- Updated `configs/manual/client.player1.toml` through
  `client.player4.toml` with the MVP `ffmpeg_libx264` production profile.
- Added focused tests for:
  - fallback config load without `[video.encoder]`
  - manual config parse with `[video.encoder]`
  - encoder metadata honoring configured output width / height / FPS

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `configs/manual/client.player1.toml`
- `configs/manual/client.player2.toml`
- `configs/manual/client.player3.toml`
- `configs/manual/client.player4.toml`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- production encoder settings stay client-owned and optional
- `[video.encoder]` should make the MVP `ffmpeg_libx264` profile explicit
  without forcing it onto configs that do not declare the block
- the first implementation slice stops at software-profile/config wiring and
  visibility; hardware encoder integration remains later

### Validation
- `cargo fmt`
- `cargo check --workspace`
- `cargo test -p stream-sync-client client_video_frame -- --test-threads=1`

## 2026-05-07
### Type
- Codex

### Work
- Fixed the production H.264 encoder configuration / error logging policy in
  docs without changing code.
- Recorded the first production-profile direction as a client-owned
  `ffmpeg + libx264` low-latency profile rather than as a protocol or
  transport change.
- Recorded the recommended MVP profile:
  - `1280x720`
  - `30fps`
  - `bitrate_kbps=4500`
  - `gop_frames=30`
  - `preset=ultrafast`
  - `tune=zerolatency`
  - `pixel_format=yuv420p`
  - `profile=main`
  - `level=3.1`
- Recorded the future profile direction:
  - `1920x1080`
  - `60fps`
  - opt-in only after CPU/bandwidth/fragmentation re-validation
- Recorded the difference from the current PoC:
  - the validated FFmpeg path remains real and successful
  - encoder settings are still implicit in code today
  - config surfacing, explicit FFmpeg visibility, and structured encode error
    classification are the next implementation concerns
- Recorded the failure/logging policy split:
  - capture failure
  - encode failure
  - frame build failure
  - send failure
  - fragmentation pressure / oversized payload
  - FFmpeg availability / version / spawn failure
- Updated TODO so the first implementation slice becomes:
  - client `[video.encoder]` config surfacing
  - bounded sender summary field extension
  - FFmpeg availability/version visibility

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- production H.264 configuration should be a client-owned profile/config layer
  on top of the validated real-capture path
- the successful current recipe should stay valid while config wiring is added
- hardware encoder integration stays later than config surfacing and error
  visibility

### Validation
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Recorded the successful actual control-pipe validation for
  `--four-view-operator-wrapper --raw-keys`.
- Captured the confirmed command sequence:
  - `status`
  - `focus 0`
  - `focus 1`
  - `focus 2`
  - `focus 3`
  - `all`
  - `focus 0`
  - `focus 1`
  - `focus 2`
  - `all`
  - `quit`
- Recorded the controlled-loop final summary from the raw-key run:
  - `commands_processed=11`
  - `commands_rejected=0`
  - `current_view_state=AllView`
  - `frames_rendered=50`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
  - `exit_reason=QuitRequested`
- Recorded the final slot diagnostics:
  - `player1..4` all `FrameRead`
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`
  - `final_slot_result_kind=Selected`
- Recorded one observed transient note:
  - `command_index=4` on `Focused(3)` briefly surfaced
    `scheduler_status=HandoffError`
  - the same command still reported `selected_slot_result=Selected`,
    `clean_output_render_result_kind=Rendered`, and `frames_rendered=5`
  - the final summary returned to `scheduler_status=AllSelected`
- Updated docs so the next task moves to production H.264 encoder
  configuration / error logging policy instead of more wrapper validation.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- actual raw-key validation is complete and successful enough for the current
  MVP operator surface
- transient scheduler-status wobble during `Focused(3)` is later narrow polish,
  not a blocker
- production H.264 encoder configuration / error logging policy is now the next
  task

### Validation
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Implemented wrapper-local optional raw key capture for
  `--four-view-operator-wrapper`.
- Added optional raw mode flag:
  - `--raw-keys`
- Kept existing wrapper boundaries unchanged:
  - same control-pipe sender logic
  - same command vocabulary
  - same wrapper-local double-`Q` guarded quit
  - same wrapper summary stdout
  - same `--keys` scripted mode
  - same one-line stdin fallback mode
- Added Windows console raw-key setup/read handling with explicit setup failure
  reporting.
- Added code-level tests for:
  - raw input source parser
  - raw key mapping through the existing wrapper command path
  - raw `Q` once / `Q` twice guarded quit behavior
  - raw unknown key local ignore
  - raw setup failure path
- Updated docs to mark raw key capture implemented and moved the next task to
  actual raw-mode manual validation.

### Changed Files
- `apps/switcher/Cargo.toml`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- raw key capture stays wrapper-local and optional
- `--keys` and Enter-required stdin remain the validation baselines
- actual manual validation for `--raw-keys` is the next task

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1`
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Fixed the raw key capture decision in docs without adding code changes.
- Compared two positions:
  - keep Enter-required stdin as the final wrapper MVP input
  - add raw key capture as optional wrapper-local UX polish
- Chose the second position while keeping the current validated paths intact:
  - `--keys` remains the scripted/automation baseline
  - one-line stdin remains the fallback/manual baseline
  - switcher loop, control pipe, and command parser remain unchanged
- Recorded the smallest next raw-key shape:
  - optional wrapper flag such as `--raw-keys`
  - same key mappings
  - same wrapper-local double-`Q` guarded quit
  - same wrapper summary stdout
  - local ignore for unknown keys
  - fallback to stdin mode if Windows terminal raw-key capture is unavailable

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- raw key capture is useful and should be the next narrow operator UX slice
- raw key capture is not an MVP blocker
- the implementation must stay wrapper-local and preserve the current validated
  `--keys` / stdin / control-pipe baseline
- production H.264 encoder configuration / error logging policy remains after
  that

### Validation
- docs-only update
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Fixed the post-validation docs decision for the wrapper MVP without adding
  code changes.
- Marked the zero-gap stdin wobble as a non-blocking manual-harness issue:
  - it appeared once under zero-gap piped stdin
  - it did not reproduce under manual-like pacing
- Marked bounded server lifecycle flush/exit polish as non-blocking for the
  current wrapper MVP:
  - keep request-budget headroom guidance in docs
  - keep exact-budget-only operation out of the recommended manual recipe
- Updated the next task ordering:
  - decide whether raw key capture should be added
  - keep production H.264 encoder configuration / error logging policy after
    that

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- zero-gap stdin wobble is not an MVP blocker
- wrapper-side retry/pacing remains later narrow polish
- bounded lifecycle flush/exit polish is not an MVP blocker
- next task is the raw key capture decision rather than more lifecycle/wobble
  implementation

### Validation
- docs-only update
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Ran actual guarded real `4`-client interactive stdin validation for
  `--four-view-operator-wrapper`.
- Rebuilt first to avoid stale `target/debug` behavior:
  - `cargo build -p stream-sync-switcher -p stream-sync-server -p stream-sync-client`
- Recorded interactive stdin success path with one line per key token:
  - `s`
  - `1`
  - `2`
  - `3`
  - `4`
  - `0`
  - `q`
  - `q`
- Recorded success-path wrapper final summary:
  - `input_source=stdin`
  - `keys_processed=8`
  - `commands_sent=7`
  - `ignored_keys=0`
  - `exit_reason=QuitRequested`
- Recorded success-path switcher final summary:
  - `commands_processed=7`
  - `commands_rejected=0`
  - `frames_rendered=30`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `exit_reason=QuitRequested`
- Recorded interactive stdin unknown-key path too:
  - `x`
  - `s`
  - `q`
  - `q`
- Recorded unknown-key wrapper final summary:
  - `input_source=stdin`
  - `keys_processed=4`
  - `commands_sent=2`
  - `ignored_keys=1`
  - `exit_reason=QuitRequested`
- Recorded unknown-key switcher final summary:
  - `commands_processed=2`
  - `commands_rejected=0`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `exit_reason=QuitRequested`
- Recorded one interactive stdin wobble:
  - a zero-gap piped stdin attempt succeeded for `s` but failed on `1` with
    `wrapper_error=os_error_2`
  - a rerun with manual-like pacing between lines succeeded
- Recorded bounded handoff summaries:
  - success path:
    - `max_requests=160`
    - `requests_served=160`
    - `successful_responses=160`
    - `handoff_errors=0`
  - unknown-key path:
    - `max_requests=60`
    - `requests_served=60`
    - `successful_responses=60`
    - `handoff_errors=0`

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Keep interactive stdin mode validated as a minimal manual/operator path.
- Treat the zero-gap piped stdin failure as a narrow timing/reconnect wobble in
  the manual harness, not as a parser or mapping regression.
- Keep raw key capture, retry logic, and larger wrapper changes out of scope.

### Validation
- actual interactive stdin success-path stdout summary recorded
- actual interactive stdin unknown-key stdout summary recorded
- actual controlled-loop final summaries recorded
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Ran actual guarded real `4`-client manual validation for the same-binary
  wrapper command `--four-view-operator-wrapper` after rebuilding:
  - `cargo build -p stream-sync-switcher -p stream-sync-server -p stream-sync-client`
- Recorded scripted success-path wrapper validation with:
  - `s;1;2;3;4;0;q;q`
  - wrapper final summary:
    - `keys_processed=8`
    - `commands_sent=7`
    - `ignored_keys=0`
    - `exit_reason=QuitRequested`
  - switcher final summary:
    - `commands_processed=7`
    - `commands_rejected=0`
    - `frames_rendered=30`
    - `render_failures=0`
    - `scheduler_status=AllSelected`
    - `clean_output_render_result_kind=Rendered`
    - `exit_reason=QuitRequested`
- Recorded scripted unknown-key validation with:
  - `x;s;q;q`
  - first attempt using `max_requests=20` reproduced a real budget issue:
    - `x` stayed local with `send_result=Ignored`
    - `s` rendered successfully
    - second guarded `q` failed with `os_error_2` after the exact render budget
      was exhausted
  - corrected rerun using `max_requests=40` succeeded:
    - wrapper final summary:
      - `keys_processed=4`
      - `commands_sent=2`
      - `ignored_keys=1`
      - `exit_reason=QuitRequested`
    - switcher final summary:
      - `commands_processed=2`
      - `commands_rejected=0`
      - `frames_rendered=5`
      - `render_failures=0`
      - `scheduler_status=AllSelected`
      - `clean_output_render_result_kind=Rendered`
      - `exit_reason=QuitRequested`
- Recorded the bounded server summary nuance for wrapper sessions:
  - success-path exact render budget was `120`, but the recorded session used
    `max_requests=140`
  - unknown-key exact render budget was `20`, but the recorded rerun needed
    `max_requests=40`
  - both recorded sessions needed extra one-shot reads to consume the remaining
    bounded request budget and let the server print its final summary

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Treat the unknown-key first-attempt failure as a bounded request-budget issue
  in the manual recipe, not as a wrapper mapping/send regression.
- Keep the wrapper thin and unchanged.
- Keep bounded server lifecycle flush/exit polish as a later narrow task rather
  than a blocker for the wrapper MVP.

### Validation
- actual wrapper success-path stdout summary recorded
- actual wrapper unknown-key stdout summary recorded
- actual controlled-loop final summaries recorded
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Implemented the same-binary thin operator wrapper command in
  `stream-sync-switcher`:
  - `--four-view-operator-wrapper [control-pipe-name]`
  - optional scripted mode:
    `--keys "s;1;2;3;4;0;q;q"`
- Kept the wrapper thin:
  - reuses the existing `--send-control-command` sender logic
  - sends only the existing control-pipe command vocabulary
  - does not touch switcher render/control-loop/parser internals directly
- Implemented the wrapper-local key mapping:
  - `1 -> focus 0`
  - `2 -> focus 1`
  - `3 -> focus 2`
  - `4 -> focus 3`
  - `0` / `A` / `a -> all`
  - `S` / `s -> status`
  - `Q` / `q -> guarded quit`
- Implemented wrapper-local guarded quit:
  - first `Q` arms only
  - second `Q` within `2` seconds sends real `quit`
  - non-`Q` clears the guard
  - timeout clears the guard
- Added wrapper stdout summaries per key with:
  - `wrapper_key`
  - `mapped_command`
  - `guard_state`
  - `send_result`
  - `response_line`
  - `command_parse_error`
  - `wrapper_error`
  - `exit_reason`
- Added code-level tests for:
  - key mapping
  - unknown key
  - `Q` once does not send quit
  - `Q` twice within guard sends quit
  - non-`Q` clears quit guard
  - guard timeout clear
  - scripted keys parser

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Chose command name `--four-view-operator-wrapper`.
- Kept the first interaction mode minimal:
  - stdin mode reads one key token per line
  - scripted mode uses `--keys`
- Kept guarded quit fully wrapper-local.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1`
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Added the docs-only thin wrapper / hotkey UI MVP design after the successful
  separate local control-pipe manual validation.
- Compared the next implementation forms as:
  - `A`: add a wrapper command inside `stream-sync-switcher`
  - `B`: add a separate `apps/operator-wrapper` binary/app
  - `C`: implement the first wrapper as a CLI/TUI keyboard loop
  - `D`: defer full GUI until later
- Fixed the current recommendation as:
  - use the existing separate local control pipe
  - keep the wrapper thin and restartable
  - prefer `A + C` for the MVP
  - defer `B` and `D`
- Fixed the guarded quit choice for MVP:
  - wrapper-local double-`Q`
  - first `Q` arms only
  - second `Q` within `2` seconds sends the real `quit` command
- Added the wrapper MVP manual validation plan:
  - rebuild before validation
  - start the switcher controlled loop with `--control-pipe`
  - start the wrapper
  - press `1/2/3/4/0-or-A/S`
  - verify response lines
  - verify guarded `Q`
  - verify final switcher summary
- Narrowed the bounded server lifecycle follow-up:
  - keep the request-budget formula documented
  - do not treat extra flush read as part of the wrapper contract
  - keep flush/exit polish as a later narrow task only

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- The wrapper MVP should use the already validated separate local control pipe.
- The first implementation form should stay in `stream-sync-switcher` as a
  separate wrapper command/process rather than a new app.
- The first interaction mode should be a CLI/TUI keyboard loop, not a full
  GUI.
- Guarded quit should be wrapper-local double-`Q`, not `q then y` and not
  disabled.

### Validation
- docs-only update
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Ran actual guarded real `4`-client same-session separate local control-pipe
  validation for the validated switcher control loop.
- Verified the success path with:
  - `status`
  - `focus 0`
  - `focus 1`
  - `focus 2`
  - `focus 3`
  - `all`
  - `status`
  - `quit`
- Verified the rejected path with:
  - `focus 9`
  - `status`
  - `quit`
- Recorded sender-side one-request / one-response summary lines,
  switcher loop summaries, and server bounded handoff summaries.
- Updated the manual checklist and TODO to treat the separate local control
  pipe as manually validated rather than still pending.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Commands
- success-path server:
  `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 140 4096 5000 8 true 8388608 4 2`
- success-path clients:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 2 16 1`
- success-path switcher loop:
  `.\target\debug\stream-sync-switcher.exe --four-view-controlled-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5 --control-pipe streamsync-control-dev`
- success-path sender sequence:
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev status`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev "focus 0"`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev "focus 1"`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev "focus 2"`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev "focus 3"`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev all`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev status`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev quit`
- rejected-path server:
  `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 20 4096 5000 8 true 8388608 4 2`
- rejected-path switcher loop:
  `.\target\debug\stream-sync-switcher.exe --four-view-controlled-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5 --control-pipe streamsync-control-dev`
- rejected-path sender sequence:
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev "focus 9"`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev status`
  `.\target\debug\stream-sync-switcher.exe --send-control-command streamsync-control-dev quit`

### Success Result
- sender response `status` kept:
  - `transition_result=Observed`
  - `current_view_state=AllView`
  - `clean_output_render_result_kind=Rendered`
- sender responses `focus 0..3` each kept:
  - `transition_result=Transitioned`
  - `current_view_state=Focused(0..3)`
  - `selected_slot_result=Selected`
  - `clean_output_render_result_kind=Rendered`
- sender response `all` kept:
  - `transition_result=Transitioned`
  - `current_view_state=AllView`
  - `clean_output_render_result_kind=Rendered`
- sender response final `status` kept:
  - `transition_result=Observed`
  - `current_view_state=AllView`
  - `clean_output_render_result_kind=Rendered`
- sender response `quit` kept:
  - `transition_result=ExitRequested`
  - `current_view_state=AllView`
  - `exit_reason=QuitRequested`
- switcher final summary kept:
  - `commands_processed=8`
  - `commands_rejected=0`
  - `frames_rendered=35`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `window_title=StreamSync 4-view Output`
  - `output_width=1280`
  - `output_height=720`
  - `exit_reason=QuitRequested`
- server receive/handoff summary kept:
  - `registered_clients=4`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `max_requests=140`
  - `requests_served=140`
  - `successful_responses=140`
  - `handoff_errors=0`
- clients `player1..4` each kept:
  - `frames_captured=2`
  - `frames_encoded=2`
  - `frames_sent=2`
  - `capture_failures=0`
  - `encode_failures=0`
  - `send_failures=0`

### Rejected Result
- sender response `focus 9` kept:
  - `transition_result=Rejected`
  - `current_view_state=AllView`
  - `command_parse_error=invalid_focus_index:_expected_integer_0..3`
- sender response `status` kept:
  - `transition_result=Observed`
  - `current_view_state=AllView`
  - `clean_output_render_result_kind=Rendered`
- sender response `quit` kept:
  - `transition_result=ExitRequested`
  - `exit_reason=QuitRequested`
- switcher final summary kept:
  - `commands_processed=3`
  - `commands_rejected=1`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `exit_reason=QuitRequested`
- server bounded handoff kept:
  - `max_requests=20`
  - `requests_served=20`
  - `successful_responses=20`
  - `handoff_errors=0`

### Notes
- The first local attempt exposed a stale `target/debug/stream-sync-switcher.exe`:
  source already had `--control-pipe`, but the local binary still rejected it
  until rebuild.
- After rebuild, the clean rerun did not need the earlier scripted-mode extra
  flush read workaround; the bounded handoff session finished at
  `requests_served=140`.

### Validation
- actual control-pipe success-path sender responses recorded
- actual control-pipe rejected-path sender responses recorded
- actual switcher loop summaries recorded
- actual server bounded handoff summaries recorded

## 2026-05-07
### Type
- Codex

### Work
- Fixed the first minimal same-session separate local control channel shape for
  the `4`-view controlled switcher loop.
- Implemented the control channel as a Windows local named-pipe option on the
  existing loop instead of changing the render/handoff path:
  - loop side:
    `--four-view-controlled-handoff-preview-loop ... --control-pipe [pipe-name]`
  - sender side:
    `--send-control-command [control-pipe-name] [command]`
- Kept stdin and `--commands` unchanged as validation baseline and fallback.
- Reused the existing control parser / transition / render logic so the control
  pipe command contract stays identical to the current manual/scripted
  contract:
  - `all`
  - `focus 0`
  - `focus 1`
  - `focus 2`
  - `focus 3`
  - `status`
  - `quit`
- Kept handoff and control responsibilities separate:
  - separate pipe name
  - separate request/response payload shape
  - no reuse of the queue-read handoff DTO

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Chose implementation path `B`: docs plus minimal implementation.
- The first control transport is a one-request / one-response Windows named
  pipe dedicated to control, not the existing handoff pipe.
- The control response is intentionally small and manual-validation friendly:
  - `command`
  - `transition_result`
  - `current_view_state`
  - `selected_slot_result`
  - `clean_output_render_result_kind`
  - `command_parse_error`
  - `exit_reason`

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1`
- `git diff --check`
- added code-level tests for:
  - control source option parsing
  - control response formatting
  - manual sender runtime hook usage
  - length-prefixed UTF-8 control request/response framing

## 2026-05-07
### Type
- Codex

### Work
- Added the first docs-only hotkey/UI wrapper comparison and recommendation.
- Compared three control-channel directions for the validated same-session
  `4`-view control loop:
  - wrapper -> switcher stdin
  - wrapper -> separate local control channel
  - switcher reads hotkeys directly
- Fixed the next recommendation as:
  - keep stdin/scripted control as the validation baseline
  - keep nearby-session commands as fallback/manual proof
  - prefer a later thin wrapper over a separate local control channel, likely
    Windows named-pipe based
  - defer direct hotkey capture inside switcher
- Recorded the current same-session bounded server lifecycle decision:
  - keep the request-budget formula documented
  - accept the extra flush read in the current manual setup
  - do not block wrapper planning on bounded-summary polish

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- A separate local control channel is now the preferred MVP wrapper direction.
- The current stdin/scripted parser remains the baseline contract and fallback
  validation path, not the preferred final wrapper transport.
- Full hotkey/UI implementation remains out of scope for this slice.

### Notes
- The preferred wrapper should target the existing command vocabulary without
  inventing a new state model:
  - `all`
  - `focus 0`
  - `focus 1`
  - `focus 2`
  - `focus 3`
  - `status`
  - `quit`
- Suggested first keyboard mapping was recorded:
  - `1` / `2` / `3` / `4` for `focus 0..3`
  - `0` or `A` for `all`
  - `S` for `status`
  - `Q` for guarded `quit`
- Same-session bounded server polish remains a later narrow task rather than an
  immediate blocker for wrapper design.

### Validation
- docs-only update
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Ran guarded real handoff same-session scripted manual validation for the new
  `--four-view-controlled-handoff-preview-loop`.
- Recorded both:
  - main success script:
    `status -> focus 0 -> focus 1 -> focus 2 -> focus 3 -> all -> status -> quit`
  - rejected script:
    `focus 9 -> status -> quit`
- Noted one practical server-lifecycle detail: the same-session success path
  needed a larger bounded `max_requests` budget than the earlier one-shot
  commands, and one extra one-shot read was used to flush the bounded server
  summary after the successful scripted loop.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Commands
- success-path server:
  `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 140 4096 5000 8 true 8388608 4 2`
- success-path clients:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 2 16 1`
- success-path switcher:
  `.\target\debug\stream-sync-switcher.exe --four-view-controlled-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5 --commands "status;focus 0;focus 1;focus 2;focus 3;all;status;quit"`
- practical post-success flush read:
  `.\target\debug\stream-sync-switcher.exe --read-queued-frame-handoff-once streamsync-handoff-dev player1 streamsync-dev-session preview-latest 141`
- rejected-path server:
  `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 20 4096 5000 8 true 8388608 4 2`
- rejected-path switcher:
  `.\target\debug\stream-sync-switcher.exe --four-view-controlled-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5 --commands "focus 9;status;quit"`

### Main Success Result
- command `0` `status`:
  - `current_view_state=AllView`
  - `transition_result=Observed`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
- commands `1..4` `focus 0..3`:
  - `current_view_state=Focused(0..3)`
  - `transition_result=Transitioned`
  - `selected_slot_result=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
- command `5` `all`:
  - `current_view_state=AllView`
  - `transition_result=Transitioned`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
- command `6` `status`:
  - `current_view_state=AllView`
  - `transition_result=Observed`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
- command `7` `quit`:
  - `transition_result=ExitRequested`
  - `exit_reason=QuitRequested`
- final summary:
  - `commands_processed=8`
  - `commands_rejected=0`
  - `current_view_state=AllView`
  - `frames_rendered=35`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `window_title=StreamSync 4-view Output`
  - `output_width=1280`
  - `output_height=720`
  - `exit_reason=QuitRequested`

### Rejected-Path Result
- command `0` `focus 9`:
  - `transition_result=Rejected`
  - `current_view_state=AllView`
  - `command_parse_error=invalid_focus_index:_expected_integer_0..3`
- command `1` `status`:
  - `transition_result=Observed`
  - `current_view_state=AllView`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
- command `2` `quit`:
  - `transition_result=ExitRequested`
  - `exit_reason=QuitRequested`
- final summary:
  - `commands_processed=3`
  - `commands_rejected=1`
  - `current_view_state=AllView`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `exit_reason=QuitRequested`

### Server And Client Notes
- success-path server receive summary kept:
  - `registered_clients=4`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `receive_timed_out=false`
  - `max_packets_reached=false`
- success-path bounded handoff summary kept:
  - `max_requests=140`
  - `requests_served=140`
  - `successful_responses=140`
  - `handoff_errors=0`
- rejected-path bounded handoff summary kept:
  - `max_requests=20`
  - `requests_served=20`
  - `successful_responses=20`
  - `handoff_errors=0`
- clients `player1..4` kept:
  - `frames_captured=2`
  - `frames_encoded=2`
  - `frames_sent=2`
  - `capture_failures=0`
  - `encode_failures=0`
  - `send_failures=0`

### Decision
- same-session scripted control-loop manual validation is now recorded as
  successful for both the main success path and the rejected path.
- next work can move from control-loop feasibility/validation to wrapper-level
  operator ergonomics rather than more transport/view-state proof runs.

### Validation
- recorded actual same-session success stdout summary
- recorded actual same-session rejected-path stdout summary

## 2026-05-07
### Type
- Codex

### Work
- Implemented the first same-session fixed `4`-view control loop in
  `stream-sync-switcher`.
- Kept the validated all-view and focused commands unchanged while adding one
  dedicated control-loop command above the existing named-pipe handoff /
  validation / clean-output path.
- Added bounded scripted command input for manual automation and tests.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Command Shape
- new command:
  - `--four-view-controlled-handoff-preview-loop [pipe-name] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [max-ticks-per-command] [--commands "status;focus 0;all;quit"]`
- accepted control commands:
  - `all`
  - `focus 0`
  - `focus 1`
  - `focus 2`
  - `focus 3`
  - `status`
  - `quit`

### Decisions
- Kept the first same-session slice fixed to `4` real slots.
- Kept `AllView` and `Focused(slot_index)` render semantics by reusing the
  existing all-real and focused render paths instead of inventing a new layout.
- Added optional scripted `--commands` input immediately so bounded manual
  validation and tests do not depend on interactive stdin.
- Kept full hotkey/UI wrapper out of scope.

### Validation
- `cargo fmt`
- `cargo check -p stream-sync-switcher`
- focused scripted tests added for:
  - parser happy path
  - invalid `focus` rejection
  - `status -> focus -> all -> quit` loop progression
  - formatter output for command and loop summaries

### Next
- Run guarded same-session manual validation with scripted `--commands` against
  the existing `4`-client baseline.

## 2026-05-07
### Type
- Codex

### Work
- Reviewed the validated nearby-session operator-flow baseline against the next
  MVP operator-control need.
- Compared three options for the next control layer:
  - `A`: keep nearby-session command flow as the operational path
  - `B`: add a same-session long-running control loop
  - `C`: add a thin hotkey/UI wrapper first while keeping the current command
    family underneath
- Recorded the design decision in architecture/operations docs without changing
  code.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Keep option `A` as the validated fallback/manual proof path.
- Do not treat option `A` as the intended live operator surface because it
  recreates session/window lifecycle per transition and keeps command
  orchestration on the operator.
- Recommend option `B` as the next smallest implementation slice:
  - one same-session long-running control loop
  - one persistent `StreamSync 4-view Output` window lifecycle
  - fixed `AllView` / `Focused(slot_index)` state model
  - first control source = stdin text commands or an equivalently small
    internal parser
- Defer option `C` until after `B` exists so the wrapper can become a thin
  adapter over stable in-process transitions instead of wrapping process churn.

### Next Control Shape
- Minimum commands:
  - `all`
  - `focus 0`
  - `focus 1`
  - `focus 2`
  - `focus 3`
  - `status`
  - `quit`
- Minimum stdout diagnostics:
  - `current_view_state`
  - `requested_transition`
  - `transition_result`
  - `selected_slot_result`
  - `frames_rendered`
  - `render_failures`

### Validation
- docs-only update
- `git diff --check`

## 2026-05-07
### Type
- Codex

### Work
- Ran nearby-session operator-flow validation for:
  - `AllView -> Focused(0) -> AllView`
  - `AllView -> Focused(1) -> AllView`
  - `AllView -> Focused(2) -> AllView`
  - `AllView -> Focused(3) -> AllView`
- Recorded whether the earlier transient wobble reproduced when a short release
  delay was kept between guarded sessions.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Commands
- repeated guarded server/client recipe per session:
  - server:
    `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 20 4096 5000 8 true 8388608 4 2`
  - clients:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 2 16 1`
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 2 16 1`
- switcher per flow:
  - `AllView`:
    `.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  - `Focused(0)`:
    `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  - `Focused(1)`:
    `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 1 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  - `Focused(2)`:
    `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 2 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  - `Focused(3)`:
    `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 3 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`

### Operator-Flow Result
- all `12` nearby sessions succeeded:
  - `4` pre-`AllView`
  - `4` focused sessions
  - `4` post-`AllView`
- all `AllView` sessions kept:
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `frames_rendered=5`
  - `render_failures=0`
- all focused sessions kept:
  - `view_state=Focused`
  - requested `focused_slot_index`
  - slot-aligned `focused_client_id`
  - `focused_result_kind=Selected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
  - `frames_rendered=5`
  - `render_failures=0`
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`

### Flow Mapping
- Flow `0`:
  - pre `AllView`: pass
  - `Focused(0)`: `focused_client_id=player1`, pass
  - post `AllView`: pass
- Flow `1`:
  - pre `AllView`: pass
  - `Focused(1)`: `focused_client_id=player2`, pass
  - post `AllView`: pass
- Flow `2`:
  - pre `AllView`: pass
  - `Focused(2)`: `focused_client_id=player3`, pass
  - post `AllView`: pass
- Flow `3`:
  - pre `AllView`: pass
  - `Focused(3)`: `focused_client_id=player4`, pass
  - post `AllView`: pass

### Transient Wobble Classification
- `frames_rendered < 5`: not observed in this operator-flow pass
- server `CreatePipe(os_error_231)`: not observed in this operator-flow pass
- switcher `HandoffError`: not observed in this operator-flow pass
- named-pipe release delay:
  - a short release delay was kept between nearby sessions
  - with that delay, the earlier pipe-busy wobble did not reproduce
- client capture / encode / send failure: not observed
- server receive timeout / incomplete reassembly: not observed
- switcher parse / io / decode / render error: not observed

### Decision
- nearby-session `AllView -> Focused(slot_index) -> AllView` operator flow is
  now validated.
- the next decision should be whether a same-session long-running control loop
  is necessary before a hotkey/UI wrapper, or whether the current command
  family is already a sufficient wrapper target.

### Validation
- `12` nearby-session stdout summaries recorded

## 2026-05-07
### Type
- Codex

### Work
- Ran actual guarded `4`-client focused-view validation for
  `Focused(0)`, `Focused(1)`, `Focused(2)`, and `Focused(3)`.
- Reconfirmed `AllView` on the same guarded `4`-client baseline.
- Recorded two transient issues, then reran the affected focused sessions.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Commands
- server:
  `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 20 4096 5000 8 true 8388608 4 2`
- clients:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 2 16 1`
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 2 16 1`
- switcher focused:
  `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 1 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 2 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
  `.\target\debug\stream-sync-switcher.exe --four-view-focused-handoff-preview-loop streamsync-handoff-dev 3 player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`
- switcher all-view:
  `.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`

### Successful Focused Results
- `Focused(0)`:
  - `view_state=Focused`
  - `focused_slot_index=0`
  - `focused_client_id=player1`
  - `focused_result_kind=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
- `Focused(1)`:
  - `view_state=Focused`
  - `focused_slot_index=1`
  - `focused_client_id=player2`
  - `focused_result_kind=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
- `Focused(2)` successful rerun:
  - `view_state=Focused`
  - `focused_slot_index=2`
  - `focused_client_id=player3`
  - `focused_result_kind=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`
- `Focused(3)` successful rerun:
  - `view_state=Focused`
  - `focused_slot_index=3`
  - `focused_client_id=player4`
  - `focused_result_kind=Selected`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `output_width=1280`
  - `output_height=720`

### AllView Recheck
- `--four-view-four-real-handoff-preview-loop ...` still returned:
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `scheduler_status=AllSelected`
  - `clean_output_render_result_kind=Rendered`
  - `frames_rendered=5`
  - `render_failures=0`
  - `output_width=1280`
  - `output_height=720`

### Server And Client Notes
- successful focused sessions kept:
  - `registered_clients=4`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `receive_timed_out=false`
  - `max_packets_reached=false`
  - `handoff_errors=0`
- client summaries across the recorded successful sessions kept:
  - `frames_captured=2`
  - `frames_encoded=2`
  - `frames_sent=2`
  - `capture_failures=0`
  - `encode_failures=0`
  - `send_failures=0`
- successful focused/all-view switcher summaries kept:
  - `parse_error=none`
  - `io_error=none`
  - `decode_error=none`

### Transient Issues
- first `Focused(2)` attempt ended with:
  - `focused_result_kind=Selected`
  - `clean_output_render_result_kind=Rendered`
  - but `frames_rendered=4`
  - rerun succeeded at `5/5`
- first `Focused(3)` attempt failed before a valid handoff session formed:
  - server stderr:
    `receive auth/video queue and serve handoff many failed: Handoff(CreatePipe(os_error_231))`
  - switcher summary ended with:
    - `focused_result_kind=HandoffError`
    - `scheduler_status=HandoffError`
    - `clean_output_render_result_kind=NoRenderableFocusedView`
  - rerun after a short delay succeeded

### Decision
- `Focused(0..3)` actual manual validation is now recorded as successful on the
  guarded `4`-client baseline.
- Remaining next work should move from focused feasibility to
  `AllView -> Focused(slot_index) -> AllView` operator-flow validation and then
  a hotkey/UI wrapper discussion.

### Validation
- actual focused/all-view manual stdout summaries recorded

## 2026-05-06
### Type
- Codex

### Work
- Implemented the first focused-view control slice as a dedicated switcher
  command without changing the validated all-real command shape.
- Reused the existing guarded `4`-real-slot handoff/validation path and kept
  stdout diagnostics visible while rendering only one selected slot full-window.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decisions
- Added a dedicated command instead of widening the existing all-real command:
  `--four-view-focused-handoff-preview-loop [pipe-name] [focused-slot-index] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [frames]`
- Kept the existing all-real baseline command unchanged.
- Chose the smallest no-frame behavior for the first focused slice:
  - if the focused slot has a renderable decoded frame, render it full-window
  - if it does not, report `clean_output_render_result_kind=NoRenderableFocusedView`
    rather than inventing a new full-window placeholder renderer in this slice
- Kept `view_state=Focused`, `focused_slot_index`, `focused_client_id`,
  `focused_run_id`, `focused_result_kind`, `scheduler_status`,
  `slot_result_kinds`, and `slot_diagnostics` visible in stdout.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1`

### TODO Update
- Marked `Focused(slot_index)` minimal implementation as complete.
- Moved the next active task to all-view / focused-view manual validation.

## 2026-05-06
### Type
- Codex

### Work
- Documented the next operator-facing control surface scope after the guarded
  `4`-client all-real preview baseline succeeded in repeated stability
  observation `3/3`.
- Fixed the next-task order around design first, then `Focused(slot_index)`
  implementation, then manual validation, then hotkey/UI wrapper review.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decisions
- Do not jump into full hotkey UI yet.
- Keep the already validated all-real preview commands as low-level baselines.
- Treat the next minimal operator-facing state model as:
  - `AllView`
  - `Focused(slot_index)`
- Define the first focused slice as full-window rendering of one selected slot
  while preserving the existing `StreamSync 4-view Output` window identity and
  fixed `1280x720` output profile.
- Keep OBS downstream of manual Window Capture and keep OBS WebSocket /
  advanced OBS control out of scope.
- Keep failure classification explicit across client, server, handoff,
  switcher parse/io/decode, view-state transition, and render/output-window
  layers.

### TODO Update
- Moved the next active task to operator-facing control-surface design.
- Fixed the next sequence as:
  1. operator-facing control surface design
  2. `Focused(slot_index)` minimal implementation
  3. all-view / focused-view manual validation
  4. hotkey/UI wrapper review

### Validation
- `git diff --check`

## 2026-05-06
### Type
- Codex

### Work
- Ran the guarded `4`-client all-real recipe `3` times consecutively to measure
  repeatability instead of relying on one successful pass.
- Classified the observed variance and updated docs/TODO based on the outcome.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Commands
- Repeated `3` times:
  - server:
    `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 20 4096 5000 8 true 8388608 4 2`
  - client1:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
  - client2:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
  - client3:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 2 16 1`
  - client4:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 2 16 1`
  - switcher:
    `.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`

### Run 1
- Server summary:
  - `registered_clients=4`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `receive_timed_out=false`
  - `max_packets_reached=false`
  - `handoff_errors=0`
- Switcher summary:
  - `frames_attempted=5`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - all slot diagnostics kept `parse_error=none`, `io_error=none`,
    `decode_error=none`
- Clients:
  - player1..4 all showed `frames_captured=2`, `frames_encoded=2`,
    `frames_sent=2`
  - no capture / encode / send failures

### Run 2
- Server summary:
  - `registered_clients=4`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `receive_timed_out=false`
  - `max_packets_reached=false`
  - `handoff_errors=0`
- Switcher summary:
  - `frames_attempted=5`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - all slot diagnostics kept `parse_error=none`, `io_error=none`,
    `decode_error=none`
- Clients:
  - player1..4 all showed `frames_captured=2`, `frames_encoded=2`,
    `frames_sent=2`
  - no capture / encode / send failures

### Run 3
- Server summary:
  - `registered_clients=4`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `receive_timed_out=false`
  - `max_packets_reached=false`
  - `handoff_errors=0`
- Switcher summary:
  - `frames_attempted=5`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - all slot diagnostics kept `parse_error=none`, `io_error=none`,
    `decode_error=none`
- Clients:
  - player1..4 all showed `frames_captured=2`, `frames_encoded=2`,
    `frames_sent=2`
  - no capture / encode / send failures

### Variance Classification
- No failure classification was needed across the `3` runs.
- Observed variance existed only in non-failure numeric fields such as:
  - `packets_received`
  - `fragments_received`
  - `frame_payload_len`
- No instability was observed in:
  - client capture
  - client auth / send
  - server receive timeout
  - incomplete reassembly
  - named-pipe handoff
  - switcher parse / io / decode / render

### Decision
- The guarded `4`-client all-real path succeeded `3` times consecutively.
- This is sufficient to move the next work from stability proof to
  operator-facing control-surface design.

### TODO Update
- Recorded the repeated stability observation as successful and repeatable.
- Shifted the next task toward operator-facing control-surface planning instead
  of additional baseline feasibility runs.

### Validation
- repeated `4`-client guarded manual pass x `3`

## 2026-05-06
### Type
- Codex

### Work
- Prepared the `4`-real-slot manual pass by adding player3/player4 manual
  configs and a dedicated fixed-order all-real switcher preview command.
- Ran the guarded `4`-client receive/handoff path and confirmed an all-real
  `AllSelected` switcher pass using stdout only.

### Changed Files
- `configs/manual/client.player3.toml`
- `configs/manual/client.player4.toml`
- `apps/switcher/src/main.rs`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Existing commands could not express `4` real slots without mutating the
  already-validated `1`-real-slot / `2`-real-slot shapes.
- Added a dedicated command instead of widening older commands:
  - `--four-view-four-real-handoff-preview-loop [pipe-name] [client0-id] [run0-id] [client1-id] [run1-id] [client2-id] [run2-id] [client3-id] [run3-id] [frames]`
- Chose fixed slot order `0..3` instead of extra slot-index args because this
  slice is still explicitly fixed to a 4-view non-generic setup.

### Implemented
- Added:
  - `configs/manual/client.player3.toml`
  - `configs/manual/client.player4.toml`
- Kept:
  - same `run_id = "streamsync-dev-session"`
  - same localhost server target
  - distinct `client_id` / `shared_token` only
- Added the dedicated switcher all-real command and formatter.
- Reused:
  - existing named-pipe handoff wrapper/client
  - `SwitcherFourViewHandoffValidationBoundary`
  - clean output window family
  - persistent output loop semantics
  - fixed `1280x720` OBS-friendly output profile
- Added focused switcher tests for:
  - all four slots bound as real
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - formatted stdout fields for the new command

### Guarded `4`-Client Manual Pass
- Server:
  `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 20 4096 5000 8 true 8388608 4 2`
- Client1:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
- Client2:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
- Client3:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player3.toml 2 16 1`
- Client4:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player4.toml 2 16 1`
- Switcher:
  `.\target\debug\stream-sync-switcher.exe --four-view-four-real-handoff-preview-loop streamsync-handoff-dev player1 streamsync-dev-session player2 streamsync-dev-session player3 streamsync-dev-session player4 streamsync-dev-session 5`

### Observed Logs
- Server receive summary:
  - `registered_clients=4`
  - `manual_expected_reassembled_frames=8`
  - `manual_expected_reassembled_clients=4`
  - `manual_expected_reassembled_frames_per_client=2`
  - `frames_reassembled=8`
  - `frames_queued=8`
  - `observed_reassembled_clients=4`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2|player3/streamsync-dev-session:2|player4/streamsync-dev-session:2`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `receive_timed_out=false`
  - `max_packets_reached=false`
- Server bounded handoff:
  - `max_requests=20`
  - `requests_served=20`
  - `successful_responses=20`
  - `handoff_errors=0`
  - player1/player2/player3/player4 scopes all returned `FrameRead`
  - `queue_len_before_read=2`
  - `queue_len_after_read=2`
  - `frame_payload_len > 0` for all four scopes
- Clients:
  - all four auth requests accepted
  - all four clients sent `2` fragmented real encoded frames
  - all four clients had `send_failures=0`
- Switcher summary:
  - `real_handoff=true`
  - `real_slot_count=4`
  - `frames_attempted=5`
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=AllSelected`
  - `slot_result_kinds=Selected|Selected|Selected|Selected`
  - `clean_output_render_result_kind=Rendered`
  - `window_title=StreamSync 4-view Output`
  - `output_width=1280`
  - `output_height=720`
- Switcher slot diagnostics:
  - all four slots:
    - `handoff_response_kind=FrameRead`
    - `parse_error=none`
    - `io_error=none`
    - `decode_error=none`
    - `final_slot_result_kind=Selected`

### Conclusion
- The fixed-order all-real command is sufficient for the current fixed 4-view
  scope and does not require a generic N-view refactor.
- The guarded all-real baseline is now proven end-to-end:
  - client auth/send
  - server receive/reassembly/queue
  - named-pipe handoff serving
  - switcher 4-slot render
  - clean output render summary

### TODO Update
- Marked `4`-real-slot config prep, command shape, and actual manual pass as
  complete.
- Moved the next practical work toward repeated stability observation and the
  future operator-facing control surface rather than basic all-real feasibility.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1`
- `cargo test -p stream-sync-server client_aware_stop -- --test-threads=1`
- `cargo build -p stream-sync-server -p stream-sync-client -p stream-sync-switcher`
- guarded `4`-client manual rerun with rebuilt binaries

## 2026-05-06
### Type
- Codex

### Work
- Evaluated whether the successful `2`-client manual recipe was enough on its
  own before `4`-real-slot work.
- Chose to add a minimal optional client-aware stop condition instead of
  relying only on operator sequencing and total-frame thresholds.
- Re-ran the guarded `2`-client manual receive/handoff path and confirmed the
  stop reason and per-client reassembled-frame counts from stdout.

### Changed Files
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Decision
- Manual recipe alone is not a strong enough guard before moving toward `4`
  real slots.
- The root risk is not transport anymore; it is operator error around a
  frame-count-only receive stop condition.
- The smallest sane next step is to keep existing
  `expected_reassembled_frames` behavior intact and add optional client-aware
  thresholds that only affect manual receive/handoff commands.

### Implemented
- Added optional manual receive policy fields:
  - `expected_reassembled_clients`
  - `expected_reassembled_frames_per_client`
- Added CLI support for the optional fields on:
  - `--receive-auth-video-queue-once`
  - `--receive-auth-video-queue-and-serve-handoff-once`
  - `--receive-auth-video-queue-and-serve-handoff-many`
- Kept existing positional arguments valid by appending the new thresholds at
  the end.
- Implemented stop semantics as:
  - all enabled stop conditions must be satisfied
  - total-frame threshold still uses existing
    `expected_reassembled_frames` +
    `stop_after_expected_reassembled_frames`
  - client-aware threshold activates only when
    `expected_reassembled_clients > 0`
- Added receive summary stdout fields:
  - `manual_expected_reassembled_clients`
  - `manual_expected_reassembled_frames_per_client`
  - `observed_reassembled_clients`
  - `per_client_reassembled_frames`
  - `stop_reason`
- Added focused server tests for distinct-client and per-client threshold
  behavior.

### Guarded Manual Rerun
- Server:
  `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 10 4096 5000 4 true 8388608 2 2`
- Client1:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
- Client2:
  `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
- Switcher:
  `.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 5`

### Observed Logs
- Server receive summary:
  - `registered_clients=2`
  - `manual_expected_reassembled_frames=4`
  - `manual_expected_reassembled_clients=2`
  - `manual_expected_reassembled_frames_per_client=2`
  - `frames_reassembled=4`
  - `frames_queued=4`
  - `observed_reassembled_clients=2`
  - `per_client_reassembled_frames=player1/streamsync-dev-session:2|player2/streamsync-dev-session:2`
  - `stop_reason=ReassembledFramesAndClientAwareThresholdReached`
  - `receive_timed_out=false`
  - `max_packets_reached=false`
- Client1 and Client2:
  - auth accepted
  - `frames_captured=2`
  - `frames_encoded=2`
  - `frames_sent=2`
  - `send_failures=0`
- Server bounded handoff requests:
  - player1 scope -> `FrameRead`
  - player2 scope -> `FrameRead`
  - `queue_len_before_read=2`
  - `queue_len_after_read=2`
  - `frame_payload_len > 0` on both scopes
- Switcher summary:
  - `frames_attempted=5`
  - `frames_rendered=5`
  - `render_failures=0`
  - `slot_result_kinds=Selected|Selected|NoFrameAvailable|NoFrameAvailable`
  - `scheduler_status=PartialSelected`
  - `clean_output_render_result_kind=Rendered`
- Switcher slot diagnostics:
  - slot0/player1 -> `handoff_response_kind=FrameRead`, `parse_error=none`,
    `io_error=none`, `decode_error=none`, `final_slot_result_kind=Selected`
  - slot1/player2 -> `handoff_response_kind=FrameRead`, `parse_error=none`,
    `io_error=none`, `decode_error=none`, `final_slot_result_kind=Selected`

### Conclusion
- The optional client-aware stop condition is worth keeping before `4`
  real-slot work because it materially improves repeatability without changing
  protocol or transport behavior.
- The existing successful `2`-real-slot recipe still works and remains useful
  as a baseline record, but guarded reruns are now the recommended operational
  path.
- `scheduler_status=PartialSelected` remains correct for the current two-real
  command because slots `2` and `3` are still placeholders.

### TODO Update
- Marked the client-aware stop condition as implemented and manually validated.
- Shifted the next concrete work from stop-condition evaluation to `4`
  real-slot preparation:
  - add player3/player4 manual configs
  - define guarded `4`-client startup recipes
  - define expected server/switcher stdout for all-real success

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-server client_aware_stop -- --test-threads=1`
- `cargo test -p stream-sync-server server_handoff_service_session_summary_includes_receive_and_bounded_lines -- --test-threads=1`
- guarded manual rerun with rebuilt `target/debug` binaries

## 2026-05-06
### Type
- Codex

### Work
- Fixed the minimal server-side blocker that prevented `2` distinct clients
  from joining the same manual receive/handoff session.
- Re-ran the named-pipe handoff manual pass with the added diagnostics until
  both player1 and player2 were confirmed in the queue and both real switcher
  slots rendered successfully.

### Changed Files
- `apps/server/src/lib.rs`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Updated the manual server receive path so additional auth requests are still
  accepted during the video receive phase instead of only before it.
- Kept the change scoped to the manual receive/runtime path:
  - the first accepted auth still gates entry into the receive loop
  - later auth requests during that loop are now routed through the existing
    auth response / registry registration path
  - protocol, H.264 behavior, OBS, and switcher-side handoff protocol were not
    changed

### Manual Commands
- Minimal successful distinct-client proof (`1` frame each):
  - server:
    `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 10 4096 5000 2 true 8388608`
  - client1:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 1 16 1`
  - client2:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 1 16 1`
  - switcher:
    `.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 5`
- Preferred successful distinct-client proof (`2` frames each):
  - server:
    `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 10 4096 5000 4 true 8388608`
  - client1:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 2 16 1`
  - client2:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 2 16 1`
  - switcher:
    `.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 5`

### Observed Logs
- Before the fix, the practical blocker was not switcher transport:
  - player1-only runs could satisfy `expected_reassembled_frames=2`
  - the server then left receive/auth handling
  - client2 later failed with `AuthResponse(Receive(ConnectionReset))`
- Minimal successful distinct-client proof (`1` frame each):
  - server receive summary:
    - `registered_clients=2`
    - `frames_reassembled=2`
    - `frames_queued=2`
    - `receive_timed_out=false`
  - server bounded handoff requests alternated by client scope and both were
    `FrameRead`:
    - player1 request example:
      `queue_len_before_read=1 queue_len_after_read=1 result_kind=FrameRead selected_client_id=player1 selected_run_id=streamsync-dev-session frame_payload_len=199556`
    - player2 request example:
      `queue_len_before_read=1 queue_len_after_read=1 result_kind=FrameRead selected_client_id=player2 selected_run_id=streamsync-dev-session frame_payload_len=199556`
  - switcher summary:
    - `frames_attempted=5`
    - `frames_rendered=5`
    - `render_failures=0`
    - `scheduler_status=PartialSelected`
    - `slot_result_kinds=Selected|Selected|NoFrameAvailable|NoFrameAvailable`
    - `clean_output_render_result_kind=Rendered`
  - switcher slot diagnostics:
    - slot0/player1 -> `handoff_response_kind=FrameRead parse_error=none io_error=none decode_error=none render_input_kind=UseUpdatedFrame final_slot_result_kind=Selected`
    - slot1/player2 -> `handoff_response_kind=FrameRead parse_error=none io_error=none decode_error=none render_input_kind=UseUpdatedFrame final_slot_result_kind=Selected`
- Preferred successful distinct-client proof (`2` frames each):
  - both clients completed auth and sent `2` real encoded frames each
  - server receive summary:
    - `registered_clients=2`
    - `frames_reassembled=4`
    - `frames_queued=4`
    - `queue_len=4`
    - `receive_timed_out=false`
  - server bounded handoff requests:
    - player1 request example:
      `queue_len_before_read=2 queue_len_after_read=2 result_kind=FrameRead selected_client_id=player1 selected_run_id=streamsync-dev-session frame_payload_len=226069`
    - player2 request example:
      `queue_len_before_read=2 queue_len_after_read=2 result_kind=FrameRead selected_client_id=player2 selected_run_id=streamsync-dev-session frame_payload_len=226069`
  - switcher slot diagnostics showed both real slots healthy:
    - slot0/player1 -> `handoff_response_kind=FrameRead response_payload_len=226166 parse_error=none io_error=none decode_error=none frame_payload_len=226069 final_slot_result_kind=Selected`
    - slot1/player2 -> `handoff_response_kind=FrameRead response_payload_len=226166 parse_error=none io_error=none decode_error=none frame_payload_len=226069 final_slot_result_kind=Selected`

### Decisions
- The current manual runtime no longer needs player2 to win the first auth slot
  in order to join the same bounded receive/handoff lifetime.
- The minimal `2`-real-slot goal is now proven with two actual client scopes in
  the queue, not just request-order/no-frame diagnostics.
- `scheduler_status=PartialSelected` remains expected and healthy for this
  command because slots `2` and `3` are still deterministic placeholders.

### Unresolved
- Whether the manual stop condition should eventually become distinct
  client-aware instead of frame-count-only.
- Whether an additional manual knob such as
  `expected_reassembled_clients` or
  `expected_reassembled_frames_per_client`
  is worth adding, now that a stable documented recipe already exists.
- `4` real slots, `Focused(slot_index)`, full hotkey UI, generic N-view,
  protocol/H.264 changes, switcher-side fragment reassembly, and advanced OBS
  control remain out of scope.

### TODO Update
- Marked `2` distinct clients queue participation as validated.
- Updated the manual guide to use the successful `1`-frame-each and
  `2`-frames-each recipes.
- Moved the next decision from "make the pass possible" to "decide whether a
  client-aware manual stop condition is still needed."

### Validation
- `cargo fmt`
- `cargo check --workspace`
- `cargo build -p stream-sync-server`

## 2026-05-06
### Type
- Codex

### Work
- Added dedicated manual test configs for the upcoming `2`-real-slot
  validation instead of mutating the existing example configs.
- Kept the config slice limited to server + two client configs; no switcher
  config was added because the current switcher handoff commands still do not
  consume one for this path.

### Changed Files
- `configs/manual/server.two-real-slots.toml`
- `configs/manual/client.player1.toml`
- `configs/manual/client.player2.toml`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Created `configs/manual/` as the dedicated manual-validation config
  directory.
- Added:
  - `configs/manual/server.two-real-slots.toml`
  - `configs/manual/client.player1.toml`
  - `configs/manual/client.player2.toml`
- Config decisions:
  - server config stays compatible with existing manual tests on port `5000`
  - both manual client configs use `run_id = "streamsync-dev-session"`
  - `client.player1.toml` uses `client_id = "player1"`
  - `client.player2.toml` uses `client_id = "player2"`
  - no named-pipe field was invented in config because the current schema does
    not carry it
  - no `switcher.two-real-slots.toml` was added because the current switcher
    commands do not read one for this preview path
- Added the intended manual command sequence using the new configs to
  `docs/operations/manual-real-encoded-video-poc.md`.

### TODO Update
- Kept the next task on the `2`-real-slot manual pass itself.
- Updated the TODO text so that pass explicitly uses the new manual config
  files.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-06
### Type
- Codex

### Work
- Implemented the smallest `2` real slots + `2` deterministic placeholder /
  no-frame preview loop on the switcher side.
- Kept the validated `1`-real-slot command unchanged.
- Reused the existing named-pipe handoff wrapper, 4-view validation boundary,
  and dedicated clean output family instead of widening scope into `4` real
  slots or generic N-view work.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Added switcher CLI command:
  - `--four-view-two-real-handoff-preview-loop [pipe-name] [slot0-index] [client0-id] [run0-id] [slot1-index] [client1-id] [run1-id] [frames]`
- Added validation for:
  - both slot indices in `0..3`
  - distinct slot indices
  - positive bounded `frames`
- Reused the existing named-pipe handoff wrapper/client for both configured
  real slots.
- Kept the remaining `2` slots deterministic `NoFrameAvailable`
  placeholders.
- Reused:
  - `SwitcherFourViewHandoffValidationBoundary`
  - `StreamSync 4-view Output`
  - persistent output-loop semantics
  - fixed `1280x720` OBS-friendly output profile
- Added compact stdout summary fields for the `2`-real-slot command:
  - `command_name`
  - `real_handoff=true`
  - `real_slot_count=2`
  - `real_slot0_index`
  - `real_slot1_index`
  - `pipe_name`
  - `client0_id`
  - `run0_id`
  - `client1_id`
  - `run1_id`
  - `frames_attempted`
  - `frames_rendered`
  - `render_failures`
  - `scheduler_status`
  - `slot_bindings`
  - `slot_result_kinds`
  - `clean_output_render_result_kind`
  - `window_title`
  - `output_width`
  - `output_height`

### Test Coverage
- Added helper coverage for distinct real-slot validation.
- Added fake-handoff / fake-render-runtime coverage proving:
  - 2 real slots can be bound while the other 2 remain placeholders
  - summary formatting exposes the expected `2`-real-slot fields
  - default tests remain independent of real named-pipe I/O and real OS
    window rendering

### TODO Update
- Marked the `2`-real-slot command as implemented.
- Moved the immediate next task from `2`-real-slot planning to manual actual
  validation of that new command.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-06
### Type
- Codex

### Work
- Added a planning/docs-only slice for the next `2` real slots + `2`
  deterministic placeholder / no-frame preview path.
- Kept the validated `1`-real-slot command and transport path unchanged.
- Fixed the next design baseline before any implementation begins.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Planning Decisions
- The first `2`-real-slot preview should reuse:
  - one existing bounded server queue/handoff service session
  - one named pipe
  - two distinct real `client_id + run_id` scopes inside the same shared
    queue/service lifetime
- Do not start the `2`-real-slot slice with:
  - two separate pipe names
  - two separate server service sessions
  - optional positional widening of the validated `1`-real-slot command
- Preferred first switcher command shape:

```text
stream-sync-switcher --four-view-two-real-handoff-preview-loop [pipe-name] [slot0-index] [client0-id] [run0-id] [slot1-index] [client1-id] [run1-id] [frames]
```

- Reason for using a new command:
  - it keeps the validated `--four-view-real-handoff-preview-loop` baseline
    stable
  - it avoids ambiguous parsing for optional second-slot arguments
  - it is the smallest way to add the next real-slot count without generic
    N-view refactoring
- Per-slot binding representation for the `2`-real-slot slice:
  - keep one `slot_bindings` field covering all 4 slots in slot order
  - keep format `slot_index:client_id/run_id`
  - also add explicit per-real-slot stdout fields:
    - `real_slot0_index`
    - `real_slot0_client_id`
    - `real_slot0_run_id`
    - `real_slot1_index`
    - `real_slot1_client_id`
    - `real_slot1_run_id`
- Missing-one-real-client behavior:
  - preserve `Selected + NoFrameAvailable`
  - preserve `Selected + WaitingForFrameAtOrBeforeTarget`
  - use `HandoffError` only for named-pipe/runtime/transport failure
- The remaining `2` slots should stay deterministic placeholder / no-frame
  slots in the first `2`-real-slot slice.
- Recommended stdout summary additions for the first `2`-real-slot command:
  - `real_handoff=true`
  - `real_slot_count=2`
  - `pipe_name`
  - explicit per-real-slot index/client/run fields
  - `slot_bindings`
  - `slot_result_kinds`
  - `scheduler_status`
  - `frames_attempted`
  - `frames_rendered`
  - `render_failures`
  - `clean_output_render_result_kind`
  - `window_title=StreamSync 4-view Output`
  - `output_width`
  - `output_height`

### Scope Guardrails
- Still out of scope for the next slice:
  - `4` real slots
  - `Focused(slot_index)`
  - full hotkey UI
  - generic N-view refactor
  - protocol wire-format changes
  - H.264 behavior changes
  - switcher-side fragment reassembly
  - OBS WebSocket / advanced OBS control

### TODO Update
- Kept the `1`-real-slot path as the validated baseline.
- Moved the immediate next work to `2`-real-slot preview planning details
  rather than implementation.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-06
### Type
- Codex

### Work
- Recorded the successful manual validation of the first real
  server->switcher handoff driven 4-view preview path.
- Kept the scope at `1` real handoff slot plus `3` deterministic placeholder
  / no-frame slots.
- Updated tracking so the next task moves from first real-slot validation to
  2-real-slot preview planning.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Validation Record
- Server command behavior succeeded:
  - auth accepted for `client_id=player1`
  - one real frame was received, reassembled, and queued
  - bounded named-pipe service served `5/5` successful requests
  - all `5` named-pipe responses were `FrameRead`
- Recorded server stdout:

```text
receive auth/video queue runtime handled auth on 0.0.0.0:5000; auth_accepted=true auth_reason=Ok client_id=player1 run_id=streamsync-dev-session video=received queued=queued queue_len=1 dropped_oldest=false registered_clients=1 manual_max_video_packets=4096 manual_receive_timeout_ms=15000 manual_expected_reassembled_frames=1 manual_stop_after_expected_reassembled_frames=true manual_receive_buffer_requested_bytes=8388608 manual_receive_buffer_effective_bytes=8388608 manual_receive_buffer_set_error=none manual_receive_buffer_read_error=none packets_received=363 fragments_received=363 frames_reassembled=1 frames_queued=1 direct_frames_queued=0 rejected_packets=0 rejected_fragments=0 duplicate_fragments=0 non_video_packets=0 incomplete_reassembly_frames=0 incomplete_frame_progress=none receive_timed_out=false max_packets_reached=false
server named-pipe handoff bounded pipe_name=streamsync-handoff-dev max_requests=5 requests_served=5 successful_responses=5 handoff_errors=0
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=0 request_id=1 result_kind=FrameRead queue_len=1 handoff_error=none
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=1 request_id=2 result_kind=FrameRead queue_len=1 handoff_error=none
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=2 request_id=3 result_kind=FrameRead queue_len=1 handoff_error=none
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=3 request_id=4 result_kind=FrameRead queue_len=1 handoff_error=none
server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=4 request_id=5 result_kind=FrameRead queue_len=1 handoff_error=none
```

- Client command behavior succeeded:
  - auth accepted
  - bounded sender captured / encoded / sent `5` real frames
  - all sends used fragmented video packets
  - `send_failures=0`
- Recorded client stdout:

```text
auth real encoded video frame bounded PoC sent AuthRequest 96 bytes from 0.0.0.0:57498 to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; bounded_manual_runtime=true; fragment_pacing_every=16 fragment_pacing_delay_ms=1 frames_attempted=6 frames_captured=5 frames_encoded=5 frames_sent=5 direct_sends=0 fragmented_sends=5 fragments_attempted=1815 fragments_sent=1815 no_frame_count=1 capture_failures=0 encode_failures=0 frame_build_failures=0 send_failures=0 stop_reason=Some(MaxFramesReached) last_send_destination=none last_send_local_source=none last_send_frame_id=none last_send_payload_len=none last_send_packet_len=none last_send_error=none
```

- Switcher command behavior succeeded:
  - `real_handoff=true`
  - configured real slot `0` selected real handoff frames
  - the other `3` slots stayed deterministic placeholder / no-frame slots
  - `frames_rendered=5`
  - `render_failures=0`
  - `scheduler_status=PartialSelected` was observed as expected for one real
    selected slot plus three placeholder / no-frame slots
- Recorded switcher stdout:

```text
switcher four-view real handoff preview loop command_name=--four-view-real-handoff-preview-loop real_handoff=true real_slot_count=1 real_slot_index=0 pipe_name=streamsync-handoff-dev client_id=player1 run_id=streamsync-dev-session frames_attempted=5 frames_rendered=5 render_failures=0 scheduler_status=PartialSelected slot_bindings=0:player1/streamsync-dev-session|1:fixture-placeholder-slot-1/fixture-placeholder-run-1|2:fixture-placeholder-slot-2/fixture-placeholder-run-2|3:fixture-placeholder-slot-3/fixture-placeholder-run-3 slot_result_kinds=Selected|NoFrameAvailable|NoFrameAvailable|NoFrameAvailable clean_output_render_result_kind=Rendered window_title=StreamSync 4-view Output output_width=1280 output_height=720
```

- OBS observation succeeded:
  - OBS Window Capture displayed `StreamSync 4-view Output`
  - OBS preview showed output
  - a real-slot-like image was visible

### Conclusion
- The first real-handoff preview path is now manually validated for:
  - client auth
  - real capture / encode / send
  - server receive / reassembly / queue
  - bounded named-pipe handoff serving
  - switcher named-pipe consumption
  - one real selected slot inside the existing 4-view clean output family
  - OBS downstream Window Capture of `StreamSync 4-view Output`
- The validated scope remains exactly:
  - `1` real handoff slot
  - `3` deterministic placeholder / no-frame slots

### TODO Update
- Marked the 1-real-slot mixed preview manual validation complete.
- Moved the next task to planning the smallest 2-real-slot preview slice rather
  than re-validating the 1-real-slot path.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-06
### Type
- Codex

### Work
- Implemented the smallest mixed real 4-view preview runtime on the switcher
  side with one real named-pipe handoff slot and three deterministic non-real
  slots.
- Added the bounded CLI command
  `--four-view-real-handoff-preview-loop [pipe-name] [real-slot-index] [client-id] [run-id] [frames]`.
- Reused the existing named-pipe handoff wrapper, `SwitcherFourViewHandoffValidationBoundary`,
  and dedicated clean output window family instead of widening scope into
  multi-real-slot orchestration or generic N-view work.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Added CLI parsing and validation for:
  - `real-slot-index` in `0..3`
  - positive bounded `frames`
- Added a thin mixed handoff wrapper that:
  - forwards only the configured real slot to the existing named-pipe handoff
    client/wrapper
  - returns deterministic `NoFrameAvailable` for the other three slots
- Added the bounded real preview loop that:
  - reuses `SwitcherFourViewHandoffValidationBoundary`
  - keeps OBS downstream of `StreamSync 4-view Output`
  - reuses the persistent clean output loop semantics
  - reuses the fixed `1280x720` OBS-friendly output profile
- Added compact stdout summary formatting for:
  - `command_name`
  - `real_handoff=true`
  - `real_slot_count=1`
  - `real_slot_index`
  - `pipe_name`
  - `client_id`
  - `run_id`
  - `frames_attempted`
  - `frames_rendered`
  - `render_failures`
  - `scheduler_status`
  - per-slot binding / result kinds
  - `clean_output_render_result_kind`
  - `window_title`
  - `output_width`
  - `output_height`
- Kept deterministic fixture commands unchanged:
  - `--four-view-proof-fixture-once`
  - `--four-view-proof-window-once`
  - `--four-view-clean-output-window-once`
  - `--four-view-clean-output-window-loop`

### Test Coverage
- Added helper/formatter coverage for the new real-slot index parser and real
  preview summary formatter.
- Added fake-handoff / fake-render-runtime coverage proving:
  - one configured slot is real
  - the other three slots stay deterministic non-real placeholders
  - the clean output family still renders through the persistent loop path
  - default tests remain independent of real named-pipe IO and real OS window
    rendering

### TODO Update
- Marked the first mixed real preview command as implemented.
- Moved the immediate next work from command implementation to manual actual
  validation against the bounded server handoff session and OBS-downstream path.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-06
### Type
- Codex

### Work
- Added a planning/docs-only slice for the first real server->switcher handoff
  driven 4-view preview path after deterministic OBS Window Capture validation
  completed.
- Fixed the next runtime shape as a bounded mixed preview that reuses the
  existing named-pipe handoff wrapper, `SwitcherFourViewHandoffValidationBoundary`,
  and dedicated clean output window family.
- Chose a one-real-slot-first rollout to keep the first real preview narrow and
  observable.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Planning Decisions
- The smallest real handoff-driven 4-view preview path should:
  - reuse the existing server bounded named-pipe handoff service session
  - reuse the existing switcher named-pipe handoff wrapper/client
  - reuse `SwitcherFourViewHandoffValidationBoundary`
  - keep OBS downstream of `StreamSync 4-view Output`
- The first real preview should use:
  - `1` real handoff slot
  - `3` deterministic non-real slots
- Prefer one real slot first over two or four because it proves the real
  transport-to-preview wiring with the least setup and the clearest per-slot
  semantics.
- Real handoff results should feed the existing 4-view chain through the
  current `SwitcherQueuedFrameHandoff` abstraction; the validation boundary
  keeps owning scheduler / display / composition / clean-output decisions.
- Missing-client / missing-frame representation for the first slice:
  - no eligible queued frame in the configured real slot: `NoFrameAvailable`
  - frame newer than target timestamp: `WaitingForFrameAtOrBeforeTarget`
  - named-pipe/runtime failure: `HandoffError`
  - intentionally non-real slots: deterministic fixture-backed placeholder
    content
- Preferred first switcher command shape:

```text
stream-sync-switcher --four-view-real-handoff-preview-loop [pipe-name] [real-slot-index] [client-id] [run-id] [frames]
```

- Server side should keep reusing:

```text
--receive-auth-video-queue-and-serve-handoff-many
```

- Deterministic fixture-only commands should remain unchanged:
  - `--four-view-proof-fixture-once`
  - `--four-view-proof-window-once`
  - `--four-view-clean-output-window-once`
  - `--four-view-clean-output-window-loop`
- Recommended stdout summary for the first real preview command:
  - `real_handoff=true`
  - `real_slot_count`
  - `real_slot_index`
  - per-slot `client_id` / `run_id`
  - aggregate `scheduler_status`
  - per-slot result kind
  - clean output render result kind
  - `window_title=StreamSync 4-view Output`

### TODO Update
- Moved the next immediate task from generic real-handoff planning to
  implementing the bounded mixed real preview command.
- Fixed the first implementation target as one real slot plus three
  deterministic non-real slots.
- Kept `Focused(slot_index)`, full hotkey UI, generic N-view refactor,
  protocol/H.264 changes, switcher-side fragment reassembly, and OBS
  WebSocket/advanced control out of scope.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-06
### Type
- Codex

### Work
- Recorded the successful manual OBS Window Capture validation for the
  dedicated clean output loop after the fixed `1280x720` profile landed.
- Updated tracking so OBS Window Capture validation is now complete for the
  deterministic fixture proof path.
- Moved the next task from OBS capture troubleshooting to real
  server->switcher handoff + 4-view preview planning.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Validation Record
- Command:

```text
stream-sync-switcher --four-view-clean-output-window-loop all-renderable 900
```

- Observed result:
  - OBS was already open before running the command
  - OBS could select `StreamSync 4-view Output`
  - OBS preview showed the clean output window
  - the visible output was the deterministic 4-view QuadView clean output
  - the QuadView slot placement was:
    - slot 0 = top-left
    - slot 1 = top-right
    - slot 2 = bottom-left
    - slot 3 = bottom-right
  - `real_handoff=false` remained true

### Clarification
- This success proves the dedicated clean output window is now OBS-capturable
  through normal Window Capture.
- This does not yet prove real server->switcher handoff video.
- The observed output was still deterministic fixture output, not queued live
  handoff-driven 4-view video.

### TODO Update
- Marked OBS Window Capture validation complete.
- Moved the next immediate task to real server->switcher handoff + 4-view
  preview planning.
- Kept OBS API/WebSocket and broader UI/layout work out of scope.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-06
### Type
- Codex

### Work
- Implemented the smallest fixed `1280x720` OBS-friendly output profile on the
  existing bounded clean output loop command.
- Kept the same CLI shape
  `--four-view-clean-output-window-loop [all-renderable] [frames]`.
- Preserved the persistent window lifecycle while scaling the deterministic
  `all-renderable` source frame into a larger output surface before the window
  runtime sees it.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Fixed OBS validation output profile for the loop command:
  - `output_width=1280`
  - `output_height=720`
  - `scale_mode=nearest-neighbor`
- The loop still:
  - accepts only deterministic `all-renderable`
  - keeps stable title `StreamSync 4-view Output`
  - uses one create / many updates / one close
  - stays bounded by `frames`
  - uses fixed 30 fps cadence
- Added loop summary fields:
  - `source_width`
  - `source_height`
  - `output_width`
  - `output_height`
  - `scale_mode`
  - `window_visible`
  - `window_capture_candidate`
- Preserved existing loop summary fields:
  - `command_name`
  - `fixture_mode`
  - `clean_output_window=true`
  - `actual_window_render=true`
  - `real_handoff=false`
  - `window_title`
  - `frames_attempted`
  - `frames_rendered`
  - `render_failures`
  - `window_created`
  - `persistent_window`
  - `window_updates`
  - `window_closed`
  - `bgra_payload_len`
- Kept default tests independent of real OS window rendering by using the
  existing fake persistent runtime plus a loop-only scaling wrapper.

### Test Coverage
- Added focused coverage confirming:
  - source dimensions remain `4x2`
  - output dimensions become `1280x720`
  - `scale_mode=nearest-neighbor` is reported
  - `bgra_payload_len=1280*720*4`
  - persistent lifecycle still uses one create / many updates / one close
  - unsupported fixture modes remain rejected

### TODO Update
- Moved the next immediate task from profile implementation to rerunning manual
  actual validation against the new fixed `1280x720` loop profile and retrying
  OBS Window Capture.
- Kept render-surface/window-style work deferred until after that rerun.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-06
### Type
- Codex

### Work
- Recorded the mixed manual rerun result for the persistent clean output loop:
  the window lifecycle fix held, but OBS Window Capture validation still
  failed.
- Added a planning/docs-only slice for the next OBS-friendly clean output
  window profile rather than widening scope into OBS control APIs or real
  handoff preview.
- Fixed the next implementation target as a larger OBS-facing output surface
  while preserving the current bounded deterministic loop and persistent window
  identity.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Validation Record
- Persistent clean output loop stdout:

```text
switcher four-view clean output window loop command_name=--four-view-clean-output-window-loop fixture_mode=all-renderable clean_output_window=true actual_window_render=true real_handoff=false window_title=StreamSync 4-view Output frames_attempted=300 frames_rendered=300 render_failures=0 window_created=true persistent_window=true window_updates=300 window_closed=true width=4 height=2 bgra_payload_len=32
```

- OBS Window Capture retry result:
  - could not select the window
  - OBS preview did not show it
  - the window remained one persistent window
  - the visible window surface stayed black

### Planning Decisions
- The persistent window lifecycle fix is now considered successful:
  - `window_created=true`
  - `persistent_window=true`
  - `window_updates=300`
  - `window_closed=true`
- This is still not a successful OBS validation.
- `width=4` / `height=2` is too small to serve as a meaningful OBS Window
  Capture validation target, even if the runtime reports successful rendering.
- The next smallest OBS-facing slice should:
  - keep the persistent clean output window lifecycle
  - keep stable title `StreamSync 4-view Output`
  - keep deterministic `all-renderable` as the first fixture
  - add a fixed OBS validation profile with output size `1280x720`
  - scale the current deterministic fixture/composed frame into that output
    surface
- Prefer a fixed validation profile before general `output_width` /
  `output_height` arguments.
- Render-surface or window-style adjustments should stay secondary until after
  the larger output profile is tested.
- Next stdout additions should include:
  - `source_width`
  - `source_height`
  - `output_width`
  - `output_height`
  - `scale_mode`
  - `window_visible`
  - `window_capture_candidate`
- Out of scope remains:
  - OBS WebSocket / advanced OBS control
  - real server->switcher handoff/manual preview
  - `Focused(slot_index)`
  - full hotkey UI
  - generic N-view refactor
  - protocol wire-format / H.264 behavior changes
  - switcher-side fragment reassembly

### TODO Update
- Recorded that the persistent loop rerun fixed the recreate/flicker issue but
  still failed OBS capture because the visible surface remained black.
- Updated the next immediate task from lifecycle rerun to implementing an
  OBS-friendly validation profile on the existing bounded loop.
- Updated the manual checklist and architecture note so the next slice is now
  the fixed `1280x720` output-profile path with scaling and extra stdout
  visibility fields.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Recorded the mixed manual result for the first bounded clean output loop:
  rendering succeeded for all 300 frames, but OBS Window Capture validation
  failed because the loop appeared to recreate the window every frame.
- Implemented the smallest persistent clean output window lifecycle for the
  existing `--four-view-clean-output-window-loop` command.
- Kept the one-shot proof commands unchanged while changing only the bounded
  loop path to use one persistent window/session per loop.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Validation Record
- Previous clean output loop stdout:

```text
switcher four-view clean output window loop command_name=--four-view-clean-output-window-loop fixture_mode=all-renderable clean_output_window=true actual_window_render=true real_handoff=false window_title=StreamSync 4-view Output frames_attempted=300 frames_rendered=300 render_failures=0 width=4 height=2 bgra_payload_len=32
```

- OBS Window Capture result:
  - could not select the window
  - OBS preview did not show it
- Observed behavior:
  - a window appeared briefly
  - disappeared immediately
  - another window appeared briefly and disappeared
  - this repeated during the loop

### Implemented
- Persistent loop lifecycle change:
  - keep `--four-view-clean-output-window-loop [all-renderable] [frames]`
  - create one persistent clean output window for the loop
  - preserve title `StreamSync 4-view Output`
  - update the same window for each rendered frame
  - close the window once after the bounded loop completes
- Added lifecycle summary fields:
  - `window_created`
  - `persistent_window=true`
  - `window_updates`
  - `window_closed`
- Added focused fake-runtime tests proving:
  - one persistent window/session is created
  - multiple frame updates occur on that same session
  - the window/session closes once after the loop
  - the loop no longer models one-window-per-frame behavior

### TODO Update
- Recorded the manual 300/300 render success together with the failed OBS
  Window Capture result.
- Updated the next immediate task to rerun manual actual validation against the
  persistent lifecycle implementation.
- Updated the manual checklist and architecture note so persistent window
  identity is now a hard requirement for OBS Window Capture validation.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Implemented the smallest bounded clean output loop runtime and switcher CLI
  command for stable OBS Window Capture validation.
- Kept the existing proof-window command and one-shot clean-output command
  unchanged while adding a separate bounded frame-count runtime above the
  dedicated clean output window path.
- Preserved backend-free default tests by using fake render runtimes and a fake
  cadence-sleep hook instead of requiring real OS window rendering.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Added switcher CLI command:
  - `--four-view-clean-output-window-loop [all-renderable] [frames]`
- Current command behavior:
  - accepts only deterministic `all-renderable`
  - rejects unsupported fixture modes explicitly
  - validates `frames` as a positive bounded integer
  - reuses the dedicated clean output window path
  - preserves window title `StreamSync 4-view Output`
  - renders the deterministic fixture repeatedly for exactly `frames`
    iterations
  - uses a fixed 30 fps cadence between iterations
  - prints:
    - `command_name`
    - `fixture_mode`
    - `clean_output_window=true`
    - `actual_window_render=true`
    - `real_handoff=false`
    - `window_title`
    - `frames_attempted`
    - `frames_rendered`
    - `render_failures`
    - `width`
    - `height`
    - `bgra_payload_len`
- Added helper/formatter coverage for:
  - all-renderable-only fixture parsing
  - positive `frames` parsing
  - bounded loop rendered-frame counting
  - explicit render-failure counting
  - cadence sleep count / 30 fps interval preservation
  - loop stdout summary formatting

### TODO Update
- Marked the dedicated clean output continuous/runtime path and bounded loop
  command complete.
- Moved the next immediate task to a manual actual loop pass and OBS Window
  Capture validation using `StreamSync 4-view Output`.
- Updated the manual checklist so the bounded loop command is now the preferred
  stable OBS-facing runtime path.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Added the planning/docs-only slice for the smallest dedicated clean output
  continuous/runtime path that should follow the OBS Window Capture guidance
  decision.
- Fixed the next runtime shape as a bounded-frame loop over the dedicated clean
  output window rather than a duration-first daemon or `--hold-ms` workaround.
- Recorded the preferred CLI shape, stdout summary, OBS validation goal, and
  explicit out-of-scope items before runtime implementation starts.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Planning Decisions
- The smallest clean output continuous/runtime path should:
  - stay downstream of the existing dedicated clean output window boundary
  - repeatedly render only the dedicated clean output window
  - keep the proof/debug window path separate
- The first runtime should repeatedly render the deterministic
  `all-renderable` fixture.
- The first bounded control surface should be frame-count based, not
  duration-based.
  - use bounded `frames`
  - use a fixed 30 fps cadence
  - keep operator-visible lifetime roughly predictable without introducing an
    indefinite service
- The stable OBS-facing window title remains:
  - `StreamSync 4-view Output`
- The preferred next CLI shape is:
  - `--four-view-clean-output-window-loop [all-renderable] [frames]`
- The runtime stdout summary should include at least:
  - `frames_attempted`
  - `frames_rendered`
  - `render_failures`
  - `window_title`
  - `width`
  - `height`
  - `bgra_payload_len`
- This runtime should support OBS Window Capture validation by:
  - keeping the dedicated output window alive long enough to select it in OBS
  - preserving separation from the proof/debug window
  - keeping OBS downstream of the clean output window only
- Still out of scope:
  - OBS output implementation
  - OBS WebSocket / advanced OBS control
  - `--hold-ms` as the primary solution
  - real server->switcher handoff/manual preview
  - `Focused(slot_index)`
  - full hotkey UI
  - generic N-view refactor
  - protocol wire-format / H.264 behavior changes
  - switcher-side fragment reassembly
  - default tests that require real OS-window rendering
  - indefinite daemon/service mode

### TODO Update
- Updated the immediate next task from a generic clean output runtime path to a
  concrete bounded-loop command shape.
- Added the dedicated clean output continuous/runtime path and loop command as
  explicit pending TODO items.
- Updated the manual OBS checklist note so the current one-shot command remains
  identity proof only until the bounded loop runtime exists.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Added the planning/docs-only slice for OBS Window Capture guidance and
  validation after the successful dedicated clean output window proof.
- Fixed the first OBS validation path as manual Window Capture guidance against
  the dedicated clean output window identity `StreamSync 4-view Output`, not
  the proof window path and not OBS API control.
- Chose the next implementation slice as a longer-lived dedicated clean output
  continuous/runtime path rather than `--hold-ms`, because OBS validation needs
  a more stable capture target than the current one-shot close behavior.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Planning Decisions
- Minimal OBS validation path:
  - open the dedicated clean output window path
  - add OBS Window Capture manually
  - select `StreamSync 4-view Output`
  - confirm the capture source receives that window
  - confirm no proof/debug window is used
- First OBS validation stays manual guidance only.
- OBS should target the stable dedicated clean output window title/identity:
  - `StreamSync 4-view Output`
- The one-shot immediate close is:
  - not a blocker for planning
  - a practical limitation for manual OBS validation
  - not sufficient reason by itself to widen scope into OBS API work
- `--hold-ms` remains optional polish only.
- Next implementation slice after planning:
  - dedicated clean output continuous/runtime path
- Out of scope remains:
  - OBS output implementation
  - OBS WebSocket / advanced OBS control
  - real server->switcher handoff/manual preview
  - `Focused(slot_index)`
  - full hotkey UI
  - generic N-view refactor
  - protocol wire-format changes
  - H.264 behavior changes
  - switcher-side fragment reassembly

### TODO Update
- Marked OBS Window Capture guidance / validation planning complete.
- Moved the next immediate task to a dedicated clean output
  continuous/runtime path for manual OBS validation.
- Kept `--hold-ms` as future polish only.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Recorded the successful manual actual proof for the dedicated 4-view clean
  output window command.
- Captured stdout, observed one-shot window behavior, and the conclusion that
  the dedicated clean output path reached actual OS-window rendering while
  staying isolated from OBS output and real server->switcher handoff.
- Moved the next task from manual clean-output proof to OBS Window Capture
  guidance / validation planning.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Manual Proof
- Command:
  - `cargo run -p stream-sync-switcher -- --four-view-clean-output-window-once all-renderable`
- Stdout:

```text
switcher four-view clean output window command_name=--four-view-clean-output-window-once fixture_mode=all-renderable clean_output_window=true actual_window_render=true real_handoff=false window_title=StreamSync 4-view Output scheduler_status=AllSelected render_facing_result_kind=RenderReady output_window_result_kind=Rendered width=4 height=2 bgra_payload_len=32 placeholder_count=0 source_error_count=0
```

- Observed window behavior:
  - a window appeared
  - the title could not be visually confirmed because it closed immediately
  - it closed immediately

### Conclusion
- clean output command succeeded
- dedicated clean output path reached actual OS-window render
- `output_window_result_kind=Rendered`
- `clean_output_window=true`
- `actual_window_render=true`
- `real_handoff=false`
- stdout carried `window_title=StreamSync 4-view Output`
- `scheduler_status=AllSelected`
- `render_facing_result_kind=RenderReady`
- `width=4`
- `height=2`
- `bgra_payload_len=32`
- `placeholder_count=0`
- `source_error_count=0`
- proof window path remains separate
- one-shot immediate close is expected
- visual title confirmation is still blocked by the one-shot immediate close,
  but stdout identity is correct
- next step should be OBS Window Capture guidance / validation planning
- future `--hold-ms` remains optional polish only

### TODO Update
- Marked manual clean output window proof complete.
- Moved the next immediate task to OBS Window Capture guidance / validation
  planning.
- Kept `--hold-ms` as future polish only.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Added the smallest thin manual/runtime entry point for the dedicated 4-view
  clean output window path.
- Kept the existing deterministic proof command and actual proof-window command
  unchanged while adding a separate stable-title output window command for the
  first manual OBS-facing window path.
- Reused the dedicated clean output window boundary and fake/injected render
  runtimes instead of adding OBS API work or real server->switcher
  handoff/manual preview.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Added `SwitcherFourViewCleanOutputWindowProofBoundary`.
- Added `SwitcherFourViewCleanOutputWindowProofResult`.
- Added switcher CLI/manual command:
  - `--four-view-clean-output-window-once [all-renderable]`
- Command behavior:
  - uses deterministic `all-renderable` fixture first
  - keeps `real_handoff=false`
  - keeps proof-window render path separate
  - routes the fixture through the dedicated clean output window boundary
  - uses stable window title `StreamSync 4-view Output`
  - prints compact stdout summary with:
    - command name
    - fixture mode
    - `clean_output_window=true`
    - `actual_window_render=true`
    - `real_handoff=false`
    - `window_title`
    - `scheduler_status`
    - `render_facing_result_kind`
    - `output_window_result_kind`
    - width / height / `bgra_payload_len` when render-ready
    - `placeholder_count`
    - `source_error_count`
- Added formatter/helper coverage for:
  - stable title/backend-free helper path
  - rendered clean output summary formatting
  - explicit `RenderFailed` summary formatting

### TODO Update
- Marked the thin manual/runtime entry point for clean output complete.
- Moved the next immediate task to manual actual clean output window proof
  execution and stdout recording.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Implemented the smallest dedicated 4-view clean output window boundary for
  the OBS/output plan.
- Kept it separate from the existing proof-window path so deterministic proof
  commands and actual-window proof behavior remain unchanged.
- Reused the existing render-facing family and injected window render runtime
  instead of adding OBS API work or real server->switcher handoff/manual
  preview.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Added `SwitcherFourViewCleanOutputWindowIdentity`.
- Added stable clean output title:
  - `StreamSync 4-view Output`
- Added `SwitcherFourViewCleanOutputWindowInput`.
- Added `SwitcherFourViewCleanOutputWindowRenderResult`.
- Added `SwitcherFourViewCleanOutputWindowOutput`.
- Added `SwitcherFourViewCleanOutputWindowBoundary`.
- Boundary behavior:
  - consumes `SwitcherFourViewQuadRenderFacingConnectionOutput`
  - stays downstream of the render-facing / window-output family
  - uses the existing composed-canvas window render boundary internally
  - uses a stable dedicated output-window title, separate from the proof title
  - uses hold `0`, keeping `--hold-ms` out of scope
  - preserves width / height / `bgra_payload_len`
  - preserves render-facing result kind
  - preserves output-window result kind
  - preserves aggregate scheduler status
  - preserves four-slot metadata
  - preserves placeholder/source-error information
  - does not call the runtime for `NoRenderableQuadView` / `InvalidQuadView`
- Added focused tests covering:
  - stable clean output title and window identity
  - metadata preservation
  - aggregate scheduler status preservation
  - slot metadata preservation
  - placeholder/source-error count preservation
  - no-runtime behavior for no-render and invalid states
  - proof-window title/path remaining unchanged

### TODO Update
- Marked the dedicated 4-view clean output window boundary complete.
- Moved the next immediate task to a thin manual/runtime entry point above that
  boundary.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Added the planning/docs-only slice for the next OBS/output boundary after the
  successful 4-view actual OS-window proof.
- Fixed the first OBS/output direction as a dedicated clean output window
  downstream of the render-facing family, not direct composition-internal
  consumption and not reuse of the transient proof window.
- Kept deterministic proof commands, actual-window proof command, and real
  server->switcher handoff scope unchanged.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Recorded the OBS/output boundary planning decisions:
  - start with a separate clean output window rather than the existing
    one-shot proof window
  - keep OBS downstream of render-facing / window-output results
  - do not make OBS consume composition internals directly
  - keep handoff transport, decode runtime internals, and deterministic proof
    commands separate from OBS
  - preserve output metadata/logging including width, height,
    `bgra_payload_len`, render-facing result kind, output-window result kind,
    aggregate scheduler status, four-slot metadata, placeholder count, and
    source-error count
  - keep `--hold-ms` as optional polish instead of making it a prerequisite
- Fixed the smallest next implementation slice as:
  - dedicated 4-view clean output window boundary
  - `RenderReady` updates the dedicated output window from the composed BGRA
    payload
  - `NoRenderableQuadView` / `InvalidQuadView` stay explicit and do not become
    fake renderable frames
- Updated TODO so OBS/output boundary planning is no longer the immediate next
  task; the next task is now the dedicated clean output window boundary.

### TODO Update
- Added the OBS/output planning outcome to `現在位置`.
- Replaced `直近でやること` from planning to the next implementation slice.
- Marked the "separate output window vs operation window" decision as fixed in
  the switcher / 表示 / OBS section.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Recorded the successful manual pass for the isolated 4-view actual OS-window
  proof command.
- Confirmed the current 4-view path now has both backend-free deterministic
  proof and actual OS-window proof recorded before OBS/output boundary
  planning.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/session-log.md`
- `docs/operations/todo.md`

### Implemented
- Recorded actual OS-window proof stdout exactly as observed.

`[FOUR VIEW ACTUAL WINDOW PROOF]`

```text
switcher four-view proof window command_name=--four-view-proof-window-once fixture_mode=all-renderable deterministic_fixture=true real_handoff=false actual_window_render=true target_timestamp=1000004 scheduler_status=AllSelected bgra_composition_result_kind=ComposedFrame render_facing_result_kind=RenderReady window_render_result_kind=Rendered width=4 height=2 bgra_payload_len=32 placeholder_count=0 source_error_count=0
```

`[OBSERVED WINDOW]`

```text
A window appeared.
It closed immediately.
```

- Recorded conclusion:
  - actual OS window proof succeeded
  - deterministic all-renderable fixture succeeded
  - `scheduler_status=AllSelected`
  - BGRA composition succeeded
  - render-facing result was `RenderReady`
  - actual window render returned `Rendered`
  - `width=4` and `height=2` are consistent with the fixture
  - `bgra_payload_len=32` is correct for `4x2` BGRA
  - `placeholder_count=0`
  - `source_error_count=0`
  - `real_handoff=false`, so this proof remained isolated from
    server->switcher transport
  - OBS output remains unimplemented
  - immediate close is expected for the current one-shot proof, though a future
    `--hold-ms` option may be useful for visual confirmation

### TODO Update
- Marked the actual OS-window proof manual pass complete.
- Moved the next 4-view task to OBS/output boundary planning.
- Left future `--hold-ms` / preview hold duration as optional polish only.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Implemented the smallest isolated actual OS window proof command for 4-view.
- Kept the existing deterministic fixture proof command unchanged and
  backend-free.
- Reused the existing 4-view proof boundary and composed-canvas window-render
  path instead of adding OBS/output work or real server->switcher handoff.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `--four-view-proof-window-once [all-renderable]` to
  `stream-sync-switcher`.
- The command:
  - uses the deterministic `all-renderable` 4-view fixture only
  - reuses `SwitcherFourViewManualPreviewProofBoundary`
  - reuses `SwitcherFourViewHandoffValidationBoundary`
  - reuses the existing composed-canvas window render boundary
  - uses the real Windows GDI window render runtime hook when available
  - keeps `real_handoff=false`
  - prints a compact stdout summary with:
    - command name
    - fixture mode
    - `actual_window_render=true`
    - `real_handoff=false`
    - target timestamp
    - scheduler status
    - BGRA composition result kind
    - render-facing result kind
    - explicit window render result kind
    - width / height / BGRA payload length when render-ready
    - placeholder count
    - source-error count
- Kept `--four-view-proof-fixture-once [all-renderable|mixed-placeholder-source-error|placeholder-only]`
  unchanged as the backend-free deterministic proof command.
- Added formatter/helper coverage for:
  - actual-window fixture-mode parsing
  - rendered actual-window summary formatting with fake runtime
  - render-failed actual-window summary formatting with fake runtime

### TODO Update
- Marked the isolated actual OS window proof command implementation complete.
- Moved the next 4-view task to manual actual-window proof execution and stdout
  recording before OBS/output boundary planning.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Added the planning/docs-only slice for the next 4-view actual OS-window
  proof.
- Fixed the next step order as actual OS window proof first and OBS/output
  boundary planning later.
- Kept the deterministic fixture CLI unchanged and CI-safe while documenting a
  separate manual command path for real window proof.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`
- `docs/operations/manual-real-encoded-video-poc.md`

### Implemented
- Documented which existing runtime/path should be reused:
  - `SwitcherFourViewManualPreviewProofBoundary`
  - `SwitcherFourViewHandoffValidationBoundary`
  - existing composed-canvas window render boundary
  - existing `SwitcherWindowRenderRuntimeHook`
- Documented that the first actual OS proof should use the deterministic
  `all-renderable` fixture only.
- Documented that the actual OS proof should use a separate command rather
  than adding a flag to `--four-view-proof-fixture-once`, so the current
  deterministic CLI remains stable and `actual_window_render=false`.
- Documented that the proof should display one isolated fixed 2x2 `QuadView`
  window using the existing composed BGRA output.
- Documented the recommended stdout summary fields:
  - `fixture_mode`
  - `deterministic_fixture=true`
  - `real_handoff=false`
  - `actual_window_render=true`
  - `target_timestamp`
  - `scheduler_status`
  - `bgra_composition_result_kind`
  - `render_facing_result_kind`
  - `window_render_result_kind`
  - `placeholder_count`
  - `source_error_count`
  - composed width / height when renderable
  - rendered window title when available
- Documented render failure reporting policy:
  - keep BGRA/render-facing results visible
  - report explicit window render result kind
  - keep backend/runtime error detail separate from `NoRenderableQuadView`
- Documented out-of-scope items:
  - OBS output
  - real server->switcher handoff/manual preview
  - `Focused(slot_index)`
  - full hotkey UI
  - generic N-view refactor
  - protocol/H.264 changes
  - switcher-side fragment reassembly
  - default tests requiring real OS-window rendering

### TODO Update
- Replaced the undecided next-step question with a fixed next implementation
  target: isolated actual OS window proof.
- Recorded that OBS/output boundary planning stays downstream of that proof.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Recorded the manual stdout validation result for the deterministic 4-view
  proof fixture CLI.
- Confirmed the current 4-view internal proof path is complete enough to close
  this slice without starting actual OS-window proof, OBS/output work, or real
  server->switcher handoff/manual preview.

### Changed Files
- `docs/operations/todo.md`
- `docs/operations/session-log.md`
- `docs/operations/manual-real-encoded-video-poc.md`

### Implemented
- Recorded the three fixture stdout blocks exactly as observed.

`[ALL RENDERABLE]`

```text
switcher four-view proof fixture deterministic=true real_handoff=false actual_window_render=false target_timestamp=1000004 scheduler_status=AllSelected bgra_composition_result_kind=ComposedFrame render_facing_result_kind=RenderReady window_render_result_kind=BackendUnavailable placeholder_count=0 source_error_count=0 scheduler_slot_kinds=Selected|Selected|Selected|Selected display_slot_kinds=Update|Update|Update|Update composition_instruction_kinds=UpdatedFrame|UpdatedFrame|UpdatedFrame|UpdatedFrame
```

`[MIXED PLACEHOLDER SOURCE ERROR]`

```text
switcher four-view proof fixture deterministic=true real_handoff=false actual_window_render=false target_timestamp=1000004 scheduler_status=HandoffError bgra_composition_result_kind=ComposedFrame render_facing_result_kind=RenderReady window_render_result_kind=BackendUnavailable placeholder_count=2 source_error_count=1 scheduler_slot_kinds=Selected|WaitingForFrameAtOrBeforeTarget|NoFrameAvailable|HandoffError display_slot_kinds=Update|NoDisplayPlaceholder|NoDisplayPlaceholder|SourceErrorPlaceholder composition_instruction_kinds=UpdatedFrame|NoDisplayPlaceholder|NoDisplayPlaceholder|SourceErrorPlaceholder
```

`[PLACEHOLDER ONLY]`

```text
switcher four-view proof fixture deterministic=true real_handoff=false actual_window_render=false target_timestamp=1000004 scheduler_status=NoFrames bgra_composition_result_kind=NoRenderableQuadView render_facing_result_kind=NoRenderableQuadView window_render_result_kind=NoRenderableQuadView placeholder_count=4 source_error_count=0 scheduler_slot_kinds=NoFrameAvailable|NoFrameAvailable|NoFrameAvailable|NoFrameAvailable display_slot_kinds=NoDisplayPlaceholder|NoDisplayPlaceholder|NoDisplayPlaceholder|NoDisplayPlaceholder composition_instruction_kinds=NoDisplayPlaceholder|NoDisplayPlaceholder|NoDisplayPlaceholder|NoDisplayPlaceholder
```

- Recorded conclusion:
  - all-renderable fixture passed the full 4-view proof path
  - mixed placeholder/source-error fixture preserved
    `placeholder_count=2` and `source_error_count=1`
  - source-error remained `SourceErrorPlaceholder` and did not collapse into
    no-frame/waiting
  - placeholder-only fixture produced `NoRenderableQuadView` through BGRA,
    render-facing, and window-render result kinds
  - all expected summary fields were present
  - command stayed deterministic
  - `real_handoff=false`
  - `actual_window_render=false`
  - `BackendUnavailable` for all-renderable/mixed is expected because the
    unavailable/fake window render hook is used
  - the 4-view internal proof path is successful enough to record as complete

### TODO Update
- Marked deterministic 4-view proof fixture CLI validation complete in current
  position.
- Moved the next 4-view task to deciding between actual OS-window proof and
  OBS/output boundary planning.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-05-05
### Type
- Codex

### Work
- Added the smallest thin switcher CLI/manual entry point for the deterministic
  4-view proof wrapper.
- Kept the command bounded to deterministic in-process fixtures and a compact
  stdout summary, without real named-pipe handoff or actual OS-window render.
- Updated architecture/TODO tracking so the next 4-view step can move beyond
  the CLI wrapper instead of re-stitching the chain by hand.

### Changed Files
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `--four-view-proof-fixture-once` to `stream-sync-switcher`.
- The command:
  - uses deterministic fixture mode instead of real named-pipe handoff
  - calls `SwitcherFourViewManualPreviewProofBoundary`
  - uses a fake deterministic decode runtime
  - uses `SwitcherUnavailableWindowRenderRuntimeHook` so default validation
    stays free of actual OS-window dependency
  - prints a compact stdout summary with target timestamp, scheduler status,
    BGRA/render-facing/window-render result kinds, placeholder/source-error
    counts, and per-slot scheduler/display/composition kinds
- Added formatter/helper coverage for:
  - fixture-mode parsing
  - backend-free proof helper execution
  - compact stdout summary field presence

### TODO Update
- Marked the thin manual CLI/entry point above the 4-view proof wrapper
  complete.
- Moved the next 4-view task to deciding whether to widen the in-process proof
  payloads first or move to real server->switcher handoff/manual preview.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-05
### Type
- Codex

### Work
- Implemented the smallest deterministic in-process 4-view preview/proof
  wrapper on top of `SwitcherFourViewHandoffValidationBoundary`.
- Kept this slice smaller than a manual CLI, real server->switcher handoff, or
  actual OS-window proof by stopping at a fixture-backed wrapper plus compact
  summary.
- Updated architecture/TODO tracking so the next 4-view slice can be a thin
  manual entry point above the new wrapper.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `SwitcherFourViewManualPreviewProofFixtureMode`.
- Added `SwitcherFourViewManualPreviewProofInput`.
- Added `SwitcherFourViewManualPreviewSchedulerSlotKind`.
- Added `SwitcherFourViewManualPreviewDisplaySlotKind`.
- Added `SwitcherFourViewManualPreviewCompositionInstructionKind`.
- Added `SwitcherFourViewManualPreviewBgraCompositionKind`.
- Added `SwitcherFourViewManualPreviewRenderFacingKind`.
- Added `SwitcherFourViewManualPreviewWindowRenderKind`.
- Added `SwitcherFourViewManualPreviewProofSummary`.
- Added `SwitcherFourViewManualPreviewProofResult`.
- Added `SwitcherFourViewManualPreviewProofBoundary`.
- Proof-wrapper behavior:
  - builds a deterministic in-process fixture queue
  - thinly calls `SwitcherFourViewHandoffValidationBoundary`
  - keeps all 8 stage outputs visible through the validation output
  - returns a compact summary with target timestamp, scheduler status,
    per-slot stage kinds, BGRA/render-facing/window-render result kinds, and
    placeholder/source-error counts
  - does not require actual OS window rendering
  - does not use real named-pipe handoff

### Tests
- all-renderable fixture passes all 8 stages
- mixed placeholder/source-error fixture keeps counts
- proof summary has all expected stage/result fields
- fake window render hook is called only when render-ready
- no actual OS window render is required

### TODO Update
- Marked the deterministic in-process 4-view preview/proof wrapper complete.
- Moved the next 4-view task to a thin manual CLI/entry point above the new
  wrapper.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-05
### Type
- Codex

### Work
- Added a planning/docs-only slice for the next bounded one-shot manual
  preview/proof wrapper above `SwitcherFourViewHandoffValidationBoundary`.
- Fixed the first proof path as deterministic in-process handoff/queue fixture
  plus fake decode/window-render runtimes, ahead of any real
  server->switcher handoff or actual OS-window proof.
- Updated architecture/TODO tracking so the next implementation is a thin
  stdout-summary wrapper rather than another transport/render-policy change.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Documented that the next slice should be a bounded one-shot manual
  preview/proof wrapper, not real server->switcher handoff first and not an
  actual OS-window proof.
- Documented the first proof order:
  - in-process handoff/queue fixture + fake decode/window-render runtimes
  - optional in-process proof with more production-like payloads
  - later real server->switcher handoff/manual preview
- Documented what the smallest proof wrapper should validate:
  - all 8 stage outputs present
  - aggregate scheduler status
  - per-slot result kinds
  - BGRA composition result kind
  - render-facing result kind
  - window render result kind
  - placeholder/source-error counts
- Documented recommended wrapper inputs and stdout summary fields.
- Documented that actual OS-window proof remains out of scope for the first
  wrapper and default validation should keep using fake/unavailable window
  runtimes.

### TODO Update
- Replaced the generic next step of "manual preview/proof wrapper" with a
  deterministic in-process fixture wrapper that summarizes the full 4-view
  orchestration output.
- Kept real server->switcher handoff preview, actual OS proof, and OBS as
  later downstream work.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-05
### Type
- Codex

### Work
- Implemented the smallest dedicated 4-view orchestration/validation boundary
  above the existing render-facing and composed-canvas window render
  boundaries.
- Kept this slice smaller than a manual CLI or actual OS-window proof by
  stopping at a caller-owned orchestration wrapper that exposes every 4-view
  stage result.
- Updated architecture/TODO tracking so the next 4-view slice is a bounded
  one-shot manual preview/proof wrapper above the new boundary.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `SwitcherFourViewHandoffValidationInput`.
- Added `SwitcherFourViewHandoffValidationOutput`.
- Added `SwitcherFourViewHandoffValidationBoundary`.
- Validation boundary behavior:
  - runs the planned full 4-view chain in order
  - accepts caller-owned handoff / decode / window-render runtimes
  - keeps scheduler / adapter / display / composition instruction /
    composition render / BGRA composition / render-facing / window-render
    outputs visible
  - preserves four explicit slots and slot order
  - preserves aggregate scheduler status
  - preserves placeholder / source-error metadata through the full chain
  - does not collapse source errors into no-frame/waiting
  - does not create fake frames for skipped/error slots

### Tests
- all four fake renderable slots pass through the full orchestration chain
- mixed renderable / waiting / no-frame / source-error preserves per-slot
  metadata
- source-error survives through display/composition/render-facing/window-render
  stages
- placeholder-only case stays an explicit no-render path
- invalid quad path stays explicit
- fake window render hook is called only for render-ready output
- no actual OS window render is required

### TODO Update
- Marked the dedicated 4-view orchestration/validation boundary complete.
- Moved the next 4-view task to a bounded one-shot manual preview/proof wrapper
  above the new boundary.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-04
### Type
- Codex

### Work
- Added a planning/docs-only slice for the next thin orchestration step above
  the dedicated 4-view composed-canvas window render boundary.
- Fixed the next 4-view step as a dedicated orchestration/validation boundary
  before adding a manual CLI or attempting an actual OS window proof.
- Updated architecture/TODO tracking so the next implementation keeps every
  4-view stage visible instead of stitching boundaries together ad hoc.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Documented that the next slice should be a thin 4-view
  orchestration/validation boundary, not a direct manual preview command and
  not an actual OS proof yet.
- Documented that this boundary should run:
  - 4-view scheduler
  - scheduler decode/render adapter
  - display policy
  - `QuadView` composition adapter
  - composition render connection
  - fixed BGRA `QuadView` composition
  - render-facing connection
  - composed-canvas window render boundary
- Documented required caller-owned inputs:
  - four client/run slots
  - target timestamp
  - previous displayed slot state
  - handoff source/runtime
  - decode runtime
  - window render hook
  - current time / hold policy / title inputs
- Documented that the orchestration output should keep scheduler / adapter /
  display / composition instruction / BGRA composition / render-facing /
  window-render stage outputs visible.
- Documented that the first manual proof after this boundary should prefer fake
  runtimes first, then optional in-process queue/handoff proof, and only later
  real server->switcher handoff/manual preview.

### TODO Update
- Replaced the ambiguous next step of "manual proof or thin orchestration" with
  a dedicated 4-view orchestration/validation boundary first.
- Kept manual CLI and actual OS proof as downstream steps after that boundary
  exists.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-04
### Type
- Codex

### Work
- Implemented the smallest dedicated 4-view composed-canvas window render
  boundary on top of `SwitcherFourViewQuadRenderFacingConnectionOutput`.
- Kept this slice smaller than actual OS proof, continuous GUI/runtime
  ownership, and OBS output by stopping at an injected one-shot window-render
  boundary with explicit no-render / invalid states.
- Updated architecture/TODO tracking so the next 4-view slice can stay above
  the new boundary instead of reopening composition/render-facing concerns.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `SwitcherFourViewComposedCanvasWindowRenderInput`.
- Added `SwitcherFourViewComposedCanvasWindowRenderInvalidReason`.
- Added `SwitcherFourViewComposedCanvasRenderResult`.
- Added `SwitcherFourViewComposedCanvasWindowRenderConnectionRenderResult`.
- Added `SwitcherFourViewComposedCanvasWindowRenderConnectionOutput`.
- Added `SwitcherFourViewComposedCanvasWindowRenderBoundary`.
- Window-render boundary behavior:
  - consumes `SwitcherFourViewQuadRenderFacingConnectionOutput`
  - reuses `SwitcherWindowRenderRuntimeHook` and
    `SwitcherWindowRenderRequest`
  - calls the injected runtime only for `RenderReady`
  - preserves explicit `NoRenderableQuadView` and `InvalidQuadView` without
    runtime calls
  - keeps width / height / BGRA payload length / four-slot metadata /
    aggregate scheduler status / placeholder-source-error information visible
  - keeps runtime deferred / unavailable / invalid-frame / render-failed
    results explicit

### Tests
- render-ready quad calls fake window render hook with correct dimensions and
  payload length
- render-ready quad preserves slot metadata and aggregate status
- placeholder-only quad does not call runtime and stays explicit
- invalid quad does not call runtime and stays explicit
- source-error metadata survives
- placeholder metadata survives
- runtime hook failure remains explicit

### TODO Update
- Marked the dedicated 4-view composed-canvas window render boundary complete.
- Moved the next 4-view task to a higher-level bounded preview/manual proof or
  thin orchestration step above the new boundary.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-04
### Type
- Codex

### Work
- Added a planning/docs-only slice for the next isolated OS window render
  consumer of `SwitcherFourViewQuadRenderFacingConnectionOutput`.
- Fixed the next 4-view step as a dedicated composed-canvas window render
  boundary that reuses the existing one-shot switcher window render hook,
  before any OBS/output work.
- Updated architecture/TODO tracking so the next implementation preserves
  explicit no-render / invalid states and slot metadata through window-render
  planning too.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Documented that the next consumer for isolated preview/window rendering
  should be a dedicated 4-view composed-canvas window render boundary.
- Documented that the first window-render slice should be a render
  command/output boundary only, not OBS output and not a full GUI/runtime
  proof.
- Documented that the existing `SwitcherWindowRenderRuntimeHook`,
  `SwitcherWindowRenderRequest`, and one-shot switcher window render path
  should be reused.
- Documented per-result behavior:
  - `RenderReady` -> validate/convert and call the window render runtime
  - `NoRenderableQuadView` -> explicit no-render without runtime call
  - `InvalidQuadView` -> explicit invalid without runtime call
- Documented the metadata that stdout/logs should preserve:
  - width / height
  - BGRA payload length
  - slot metadata
  - aggregate scheduler status
  - placeholder / source-error slot information
- Documented that future OBS output stays downstream of the same render-facing
  or later window-render-adjacent result family rather than consuming
  composition internals directly.

### TODO Update
- Replaced the generic "optional isolated OS window render boundary" next step
  with a dedicated composed-canvas window render boundary that reuses the
  existing one-shot window render hook.
- Kept OBS, continuous GUI ownership, and layout polish as later downstream or
  out-of-scope work.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-04
### Type
- Codex

### Work
- Implemented the smallest dedicated 4-view render-facing adapter/connection on
  top of `SwitcherFourViewQuadCompositionBoundary`.
- Kept this slice smaller than OS-window rendering and OBS output by stopping
  at render-ready metadata plus explicit no-render / invalid result states.
- Updated architecture/TODO tracking so the next 4-view slice is an optional
  isolated window-render boundary on top of the new render-facing output.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `SwitcherFourViewComposedFrameRenderInput`.
- Added `SwitcherFourViewComposedFrameRenderInputError`.
- Added `SwitcherFourViewQuadRenderFacingInvalidReason`.
- Added `SwitcherFourViewQuadRenderFacingResult`.
- Added `SwitcherFourViewQuadRenderFacingConnectionOutput`.
- Added `SwitcherFourViewQuadRenderFacingConnectionBoundary`.
- Render-facing connection behavior:
  - consumes `SwitcherFourViewQuadCompositionOutput`
  - preserves upstream composition output visibility
  - validates composed BGRA frame metadata without creating a second pixel
    owner
  - preserves width / height / BGRA payload length
  - preserves fixed four-slot metadata
  - preserves aggregate scheduler status
  - preserves placeholder / source-error slot information
  - returns explicit `RenderReady`, `NoRenderableQuadView`, and
    `InvalidQuadView`
  - forwards composition-level no-render / invalid states explicitly

### Tests
- `ComposedFrame` -> render-ready result
- `NoRenderableQuadView` -> explicit no-render result
- `InvalidQuadView` -> explicit invalid result
- width/height preserved
- BGRA payload length preserved
- four-slot metadata preserved
- aggregate scheduler status preserved
- placeholder metadata preserved
- source-error metadata preserved

### TODO Update
- Marked the dedicated 4-view render-facing adapter/connection complete.
- Moved the next 4-view task to an optional isolated OS-window render boundary
  on top of the explicit render-facing result family.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-04
### Type
- Codex

### Work
- Added a planning/docs-only slice for the next consumer of
  `SwitcherFourViewComposedFrame`.
- Fixed the next 4-view step as a dedicated render-facing adapter/connection
  before any isolated OS window render or OBS/output work.
- Updated architecture/TODO tracking so the next implementation preserves
  explicit no-render states and slot metadata end to end.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Documented that the next consumer of `SwitcherFourViewComposedFrame` should
  be a dedicated 4-view render-facing adapter/connection.
- Documented that the next slice should be adapter/connection first, not OBS
  output and not a full OS-window render boundary yet.
- Documented the smallest recommended render-ready shape:
  - validated composed BGRA render input derived from
    `SwitcherFourViewComposedFrame`
  - frame width/height
  - BGRA payload length
  - fixed four-slot metadata in slot order
  - aggregate scheduler status
  - placeholder/source-error slot information
- Documented that `NoRenderableQuadView` and `InvalidQuadView` should remain
  explicit downstream no-render states rather than collapsing into generic
  failure.
- Documented that future OBS work should consume the same render-facing result
  family downstream of the render-facing adapter, not bypass composition.

### TODO Update
- Replaced the ambiguous next step of "render-facing connection or isolated
  window render path" with a dedicated render-facing adapter/connection first.
- Kept OS window render and OBS as later downstream steps after the metadata-
  preserving render-facing adapter is in place.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-04
### Type
- Codex

### Work
- Implemented the smallest fixed `QuadView` actual BGRA composition slice on
  top of the existing 4-view composition-ready decoded-slot connection.
- Kept this slice as pure in-memory BGRA composition only and did not add OS
  window rendering or OBS output.
- Updated architecture/TODO tracking so the next 4-view slice can attach a
  dedicated quad render-facing path to the composed BGRA frame.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `SwitcherFourViewQuadLayoutPolicy`.
- Added `SwitcherFourViewQuadCompositionInput`.
- Added `SwitcherFourViewQuadComposedSlotRect`.
- Added `SwitcherFourViewQuadComposedSlotKind`.
- Added `SwitcherFourViewQuadComposedSlotMetadata`.
- Added `SwitcherFourViewComposedFrame`.
- Added `SwitcherFourViewQuadCompositionInvalidReason`.
- Added `SwitcherFourViewQuadCompositionResult`.
- Added `SwitcherFourViewQuadCompositionOutput`.
- Added `SwitcherFourViewQuadCompositionBoundary`.
- Composition behavior:
  - consumes `SwitcherFourViewHandoffQuadCompositionRenderConnectionOutput`
  - composes one fixed 2x2 in-memory BGRA canvas
  - uses slot 0/1/2/3 as top-left/top-right/bottom-left/bottom-right
  - computes one slot size from the max renderable decoded width/height
  - fills placeholder BGRA first, then copies only real decoded renderable
    slots
  - preserves placeholder/source-error/decode-deferred/decode-failed slot
    metadata in output
  - preserves aggregate 4-view scheduler status
  - returns explicit `ComposedFrame`, `NoRenderableQuadView`, and
    `InvalidQuadView`
  - treats held-previous without decoded pixels as explicit invalid input
    instead of fabricating pixels

### Tests
- four renderable slots -> composed BGRA frame
- mixed update + held previous -> both appear in composed result
- placeholder slot metadata is preserved
- source-error placeholder metadata is preserved
- placeholder-only `QuadView` -> explicit no-renderable result
- fixed 2x2 placement is correct
- output width/height are correct
- aggregate scheduler status is preserved
- missing decoded pixels for held-previous slot -> explicit invalid result

### TODO Update
- Marked the smallest fixed `QuadView` actual BGRA composition slice complete.
- Moved the next task to a dedicated quad render-facing connection or isolated
  window-render path for `SwitcherFourViewComposedFrame`.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-04
### Type
- Codex

### Work
- Implemented the smallest dedicated 4-view `QuadView`
  composition/render-facing connection after the existing display/composition
  instruction path.
- Kept this slice smaller than real quad-canvas composition/render by stopping
  at composition-ready decoded-slot results plus an explicit top-level
  renderability result.
- Updated architecture/TODO tracking so the next 4-view slice is fixed
  `QuadView` actual BGRA composition/render on top of the new connection
  output.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Extended `SwitcherFourViewDisplayedSlot` so caller-owned previous slot state
  can carry optional decoded pixels for held-previous rendering.
- Added `SwitcherFourViewHandoffQuadCompositionRenderConnectionInput`.
- Added `SwitcherFourViewHandoffQuadCompositionRenderSlot`.
- Added
  `SwitcherFourViewHandoffQuadCompositionRenderConnectionCompositionResult`.
- Added
  `SwitcherFourViewHandoffQuadCompositionRenderConnectionRenderResult`.
- Added `SwitcherFourViewHandoffQuadCompositionRenderConnectionOutput`.
- Added `SwitcherFourViewHandoffQuadCompositionRenderConnectionBoundary`.
- Connection behavior:
  - consumes existing `SwitcherFourViewHandoffQuadCompositionAdapterOutput`
  - decodes only `UseUpdatedFrame` slots into real BGRA decoded frames
  - preserves `UseHeldPreviousFrame` as renderable when previous decoded
    pixels exist
  - preserves `UseNoDisplayPlaceholder` and
    `UseSourceErrorPlaceholder` without dropping those slots
  - preserves fixed 2x2 placement and explicit four-slot order
  - preserves aggregate
    `SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus`
  - reports `CompositionReady { renderable_slot_count }` when at least one
    real decoded slot exists
  - reports `NoRenderableQuadView` for placeholder-only output
- The boundary does not create fake decoded frames for placeholder/error
  slots.

### Tests
- all four update slots -> render/composition connection preserves four
  renderable updated slots
- mixed update + held previous -> both renderable forms are preserved
- no-display placeholder slot is preserved and not dropped
- source-error placeholder slot is preserved and not collapsed
- slot placement remains fixed 2x2
- aggregate scheduler status is preserved
- no fake decoded frames are created for placeholder/error slots
- placeholder-only `QuadView` returns explicit `NoRenderableQuadView`

### TODO Update
- Marked the smallest dedicated 4-view composition/render-facing connection
  slice complete.
- Moved the next task to fixed `QuadView` actual BGRA composition/render on
  top of the new composition-ready decoded-slot output.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with LF/CRLF conversion warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Implemented the smallest dedicated 4-view display policy and fixed
  `QuadView` composition-instruction slice.
- Kept the slice as instruction/result shaping only and did not add actual
  per-slot decode/render execution or quad-canvas rendering.
- Updated architecture/TODO tracking so the next step is connecting the 4-view
  instruction path to a real pixel path.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `SwitcherFourViewDisplayedSlot`.
- Added `SwitcherFourViewHandoffDisplayPolicyInput`.
- Added `SwitcherFourViewHandoffDisplayDecision`.
- Added `SwitcherFourViewHandoffDisplayPolicyOutput`.
- Added `SwitcherFourViewCompositionMode`.
- Added `SwitcherFourViewQuadSlotPlacement`.
- Added `SwitcherFourViewQuadCompositionSlotInstruction`.
- Added `SwitcherFourViewHandoffQuadCompositionAdapterInput`.
- Added `SwitcherFourViewHandoffQuadCompositionAdapterOutput`.
- Added `SwitcherFourViewHandoffDisplayPolicyBoundary`.
- Added `SwitcherFourViewHandoffQuadCompositionAdapterBoundary`.
- Per-slot display behavior:
  - `RenderFrame` -> `Update`
  - `SkipNoFrameAvailable` -> hold previous when available, otherwise explicit
    `NoDisplayPlaceholder`
  - `SkipWaitingForFrameAtOrBeforeTarget` -> hold previous when available,
    otherwise explicit `NoDisplayPlaceholder`
  - `SkipHandoffError` -> hold previous when available while preserving
    source-error detail, otherwise explicit `SourceErrorPlaceholder`
- Preserved aggregate 4-view scheduler status.
- Preserved explicit four-slot order and fixed `QuadView` 2x2 placement.
- Did not create fake frames for skipped/error slots.

### Tests
- all four render frames -> four update decisions
- no-frame with previous -> hold previous
- no-frame without previous -> no-display placeholder
- waiting with previous -> hold previous
- source-error with previous -> hold previous while preserving source-error
  detail
- source-error without previous -> source-error placeholder
- source-error is not treated as no-frame/waiting
- `QuadView` keeps four slots in order
- `QuadView` preserves placeholders/source-error placeholders
- aggregate scheduler status is preserved

### TODO Update
- Marked the smallest dedicated 4-view display policy and `QuadView`
  composition-instruction slice complete.
- Moved the next task to connecting the 4-view instruction path to a real
  decode/render or quad-composition pixel path.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Implemented the smallest dedicated 4-view render-facing adapter slice after
  the preview/read-only scheduler.
- Kept this slice intentionally smaller than full 4-view display policy or
  composition by stopping at explicit per-slot decode/render instructions.
- Updated architecture/TODO tracking to move the next task to per-slot display
  policy and fixed `QuadView` composition instructions.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `SwitcherFourViewHandoffSchedulerDecodeRenderSlotInstruction`.
- Added `SwitcherFourViewHandoffSchedulerDecodeRenderSlotOutput`.
- Added `SwitcherFourViewHandoffSchedulerDecodeRenderAdapterInput`.
- Added `SwitcherFourViewHandoffSchedulerDecodeRenderAdapterOutput`.
- Added `SwitcherFourViewHandoffSchedulerDecodeRenderAdapterBoundary`.
- Preserved four explicit slots and slot order.
- Preserved aggregate `SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus`.
- Per-slot mapping:
  - `Selected` -> `RenderFrame`
  - `NoFrameAvailable` -> `SkipNoFrameAvailable`
  - `WaitingForFrameAtOrBeforeTarget` -> `SkipWaitingForFrameAtOrBeforeTarget`
  - `HandoffError` -> `SkipHandoffError`
- Kept handoff/source error explicit and separate from no-frame or waiting.
- Did not create fake render/decode frames for skipped or error slots.

### Tests
- all four selected -> four render instructions
- selected + no-frame preserves no-frame skip
- selected + waiting preserves waiting skip
- selected + handoff error preserves source-error skip
- handoff error is not treated as no-frame
- handoff error is not treated as waiting
- slot order is preserved
- selected metadata survives adapter output

### TODO Update
- Marked the smallest dedicated 4-view render-facing adapter slice complete.
- Moved the next task to dedicated per-slot display policy and fixed `QuadView`
  composition instructions.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Implemented a planning/docs-only slice for the smallest 4-view
  decode/render/composition path after the dedicated 4-view scheduler result.
- Kept the decision aligned with the existing 2-view stage ordering while still
  avoiding a generic N-view refactor.
- Updated the next task from 4-view planning to the first dedicated 4-view
  adapter/display/composition implementation slice.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Do not force the existing 2-view decode/render/display/composition chain into
  a generic N-view abstraction first.
- Add dedicated 4-view boundaries that mirror the current 2-view fallible
  stage ordering:
  - scheduler-result adapter
  - decode/render connection
  - display policy
  - `QuadView` composition
- Keep one instruction per slot:
  - selected -> decode/render input
  - no-frame -> explicit no-frame skip
  - waiting -> explicit waiting skip
  - handoff/source error -> explicit source-error skip
- Do not create fake decoded/rendered frames for skipped or error slots.
- Keep placeholder-only slots as explicit composition instructions rather than
  silently dropping them.
- Keep the first composition mode fixed to `QuadView` with one 2x2 layout.

### Smallest Next Slice
- Add a dedicated fallible 4-view scheduler-result -> decode/render adapter.
- Add per-slot decode/render execution without fake frames.
- Add per-slot hold/placeholder/source-error display policy.
- Add fixed `QuadView` composition instructions in slot order.

### Out Of Scope
- OBS output
- full hotkey UI
- `Focused(slot_index)`
- final production layout polish
- generic N-view refactor
- protocol changes
- H.264 behavior changes
- switcher-side fragment reassembly

### TODO Update
- Recorded the first 4-view decode/render/composition plan.
- Moved the next task to the dedicated 4-view adapter/display/composition
  implementation slice.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Implemented the smallest dedicated fallible 4-view preview/read-only
  scheduler boundary in `apps/switcher`.
- Reused the existing fallible single-client targetTime handoff source for each
  of four explicit slots under one shared targetTime.
- Updated architecture/TODO tracking to move the next 4-view task from
  scheduler shape to 4-view decode/render/composition planning.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Implemented
- Added `SwitcherFourViewTargetTimeSourceSlotConfig`.
- Added `SwitcherFourViewTargetTimeHandoffSourceSchedulerInput`.
- Added `SwitcherFourViewTargetTimeHandoffSourceSlotResult`.
- Added `SwitcherFourViewTargetTimeHandoffSourceSchedulerStatus`.
- Added `SwitcherFourViewTargetTimeHandoffSourceSchedulerResult`.
- Added `SwitcherFourViewTargetTimeHandoffSourceSchedulerBoundary`.
- Kept the first 4-view boundary preview/read-only only through
  `select_quad_preview_from_handoff(...)`.
- Preserved per-slot outcomes:
  - `Selected`
  - `NoFrameAvailable`
  - `WaitingForFrameAtOrBeforeTarget`
  - `HandoffError`
- Added aggregate status precedence:
  - any handoff/source error -> `HandoffError`
  - all four selected -> `AllSelected`
  - any selected but not all four -> `PartialSelected`
  - otherwise any waiting -> `Waiting`
  - otherwise -> `NoFrames`
- Preserved slot order explicitly through fixed four-slot input/output arrays.
- Kept preview/read-only non-mutation semantics.

### Tests
- all four selected -> `AllSelected`
- selected + waiting + selected + selected -> `PartialSelected` with waiting
  preserved
- selected + no-frame preserved
- selected + handoff error -> `HandoffError`
- all no-frame -> `NoFrames`
- handoff error from fake source is not treated as no-frame or waiting
- preview/read-only does not mutate queue state
- slot order and selected metadata survive

### TODO Update
- Marked the smallest fallible 4-view preview/read-only scheduler slice
  complete.
- Moved the next task to 4-view decode/render/composition planning.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1` passed.

## 2026-05-02
### Type
- Codex

### Work
- Implemented a planning/docs-only slice for the next major phase and chose
  4-view orchestration planning before OBS/output boundary work.
- Documented the first 4-view orchestration shape as a separate 4-view boundary
  rather than an immediate generic N-view refactor.
- Moved the next task from major-phase selection to the smallest 4-view
  preview/read-only scheduler implementation.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Proceed with 4-view orchestration before OBS/output boundary.
- Add a separate 4-view boundary first instead of generalizing the current
  2-view path immediately.
- Use one shared targetTime across all four views.
- Use preview/read-only behavior first.
- Preserve per-view fallible outcomes:
  - selected/rendered
  - no-frame
  - waiting
  - handoff/source error
  - stale
  - placeholder
- Keep aggregate 4-view status explicit:
  - all selected
  - partial selected
  - waiting
  - no frames
  - handoff/source error
- Represent view mode minimally as `QuadView` first, with later
  `Focused(slot_index)` left for a follow-up slice.

### Smallest Next Slice
- Add a fallible 4-view preview/read-only scheduler boundary over
  `SwitcherQueuedFrameHandoff`.
- Keep one shared targetTime and explicit per-view outcomes.
- Validate first with caller-owned queue state and injected handoff fakes.

### Out Of Scope
- OBS output
- full hotkey UI
- final production layout polish
- reconnect/backoff
- protocol wire-format changes
- H.264 behavior changes
- switcher-side fragment reassembly
- generic N-view refactor

### TODO Update
- Marked the major-phase decision complete.
- Moved the next task to the smallest 4-view orchestration implementation.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Recorded the successful bounded service-session localhost manual pass.
- Updated tracking so the bounded service-session validation is complete and
  the next task moves to deciding the next major phase.
- Kept this slice docs-only and did not add retry, daemon lifecycle, Ctrl+C,
  idle-timeout, OBS, or 4-view implementation.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Treat the bounded service-session localhost run as successful.
- Treat the current transport/lifecycle phase as complete enough to close at
  the MVP level.
- Move the next task to deciding whether the next major phase should be
  4-view orchestration planning or OBS/output boundary planning.

### Observed Stdout
- Server aggregate:
  `server named-pipe handoff bounded pipe_name=streamsync-handoff-dev max_requests=2 requests_served=2 successful_responses=2 handoff_errors=0`
- Server request 0:
  `server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=0 request_id=1 result_kind=FrameRead queue_len=1 handoff_error=none`
- Server request 1:
  `server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=1 request_id=2 result_kind=FrameRead queue_len=1 handoff_error=none`
- Client:
  `auth real encoded video frame bounded PoC sent AuthRequest 96 bytes from 0.0.0.0:61364 to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; bounded_manual_runtime=true; fragment_pacing_every=16 fragment_pacing_delay_ms=1 frames_attempted=2 frames_captured=1 frames_encoded=1 frames_sent=1 direct_sends=0 fragmented_sends=1 fragments_attempted=257 fragments_sent=257 no_frame_count=1 capture_failures=0 encode_failures=0 frame_build_failures=0 send_failures=0 stop_reason=Some(MaxFramesReached) last_send_destination=none last_send_local_source=none last_send_frame_id=none last_send_payload_len=none last_send_packet_len=none last_send_error=none`
- Switcher read 1:
  `switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest attempt_count=1 timeout_millis=5000 elapsed_millis=2 request_status=sent response_status=decoded result_kind=FrameRead final_result=FrameRead last_error=none retry_classification=none queue_len=1 frame_id=2 capture_timestamp=1777670084106822 send_timestamp=1777670084106822 queued_at=1777670084284955 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=263025`
- Switcher read 2:
  `switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=2 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest attempt_count=1 timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead final_result=FrameRead last_error=none retry_classification=none queue_len=1 frame_id=2 capture_timestamp=1777670084106822 send_timestamp=1777670084106822 queued_at=1777670084284955 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=263025`

### Conclusion
- client auth succeeded
- client fragmented real encoded send succeeded
- server receive/reassembly/queue succeeded
- bounded service session served 2 named-pipe requests
- `max_requests=2`
- `requests_served=2`
- `successful_responses=2`
- `handoff_errors=0`
- both switcher reads returned `FrameRead`
- `attempt_count=1`
- `final_result=FrameRead`
- `last_error=none`
- `retry_classification=none`
- `request_id` 1 and 2 were preserved
- metadata survived server->switcher handoff
- `encoded_payload_len=263025` was preserved and non-zero
- repeated `inspect-latest` returned the same frame without queue mutation
- no error collapsed into `NoFrame`
- bounded service-session MVP appears complete enough to close the
  transport/lifecycle phase

### Next
- Decide the next major phase: 4-view orchestration planning or OBS/output
  boundary planning.

### TODO Update
- Marked the bounded service-session localhost manual pass complete.
- Moved the next task to choosing the next major phase.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Implemented the smallest bounded server-owned service session on top of the
  existing queue-owning receive launcher and bounded named-pipe handoff loop.
- Reused the existing `--receive-auth-video-queue-and-serve-handoff-many`
  command as the bounded service-session CLI instead of adding a new command.
- Extended the bounded service-session stdout to include both the
  receive/auth/video queue summary and the bounded handoff summary.

### Changed Files
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the bounded service session on the existing manual CLI surface rather
  than adding another command.
- Reuse `ServerReceiveAuthVideoQueueOnceLauncher` for UDP auth/video
  receive/reassembly/queue ownership.
- Reuse `ServerSwitcherNamedPipeOneRequestRuntimeBoundary::serve_many(...)` for
  bounded named-pipe handoff serving.
- Keep normal exit at `max_requests` and do not add daemon mode, Ctrl+C
  ownership, idle-timeout shutdown, or reconnect/backoff.

### Implemented
- Added
  `ServerReceiveAuthVideoQueueHandoffServiceSessionLauncher` in
  `apps/server`.
- Added
  `ServerReceiveAuthVideoQueueHandoffServiceSessionOutput` and
  `ServerReceiveAuthVideoQueueHandoffServiceSessionError`.
- Added focused server tests for:
  - service-session aggregation
  - receive-startup error preservation
- Added server CLI formatting coverage for the combined service-session summary.

### Unresolved
- bounded service session localhost manual pass
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists

### Next
- Take a localhost manual pass of the bounded service session and record the
  combined receive/auth/video queue summary plus bounded handoff summary.

### TODO Update
- Marked the bounded service-session implementation slice complete.
- Moved the next task to bounded service-session localhost validation.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Implemented a planning/docs-only slice for the smallest service lifecycle
  step after bounded named-pipe handoff success and MVP classification-only
  acceptance.
- Chose a bounded service session as the next lifecycle target instead of
  jumping to a full daemon, Ctrl+C mode, or idle-timeout mode.
- Moved the next task from generic service-lifecycle planning to the smallest
  bounded service-session implementation.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The smallest useful service lifecycle step is a bounded server-owned service
  session.
- The next service mode should be another intermediate shape:
  - one process lifetime
  - one queue owner
  - one bounded named-pipe handoff-serving loop
  - primary stop condition `max_requests`
- Do not jump to long-running `Ctrl+C` service mode yet.
- Do not jump to idle-timeout service mode yet.
- Implement server service lifecycle before 4-view orchestration or OBS
  planning so those later slices target a stable server-mediated runtime.
- In the first service slice the server owns:
  - UDP receive/reassembly/queue
  - named-pipe handoff serving
  - bounded lifecycle start/stop and summary output

### Shutdown Policy
- Do not add signal-driven shutdown in the first service slice.
- Do not add a new service idle-timeout policy in the first service slice.
- Stop naturally when `max_requests` is served.
- Stop early only on startup/setup failure or explicit bounded receive/session
  failure.

### Out Of Scope
- reconnect/backoff manager
- multi-client concurrency
- protocol wire format changes
- OBS output
- 4-view orchestration
- indefinite daemon mode
- Ctrl+C lifecycle
- idle-timeout service lifecycle
- switcher-side fragment reassembly

### Next
- Implement the smallest bounded service session that keeps UDP
  receive/reassembly/queue ownership and named-pipe handoff serving alive in
  one process lifetime while still exiting on `max_requests`.

### TODO Update
- Marked the service-lifecycle planning decision complete.
- Moved the next task to the bounded service-session implementation slice.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Implemented a planning/docs-only slice to decide whether the current
  switcher-side named-pipe lifecycle should stay classification-only or grow a
  bounded retry wrapper.
- Kept the result at classification-only for now and did not add retry
  execution.
- Moved the next task from retry consideration back to service lifecycle
  planning.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Classification-only is enough for the current MVP.
- Keep `1 scheduler read = 1 logical request = 1 transport attempt`.
- Do not add bounded retry just because retry classification exists.
- Future retry candidates remain:
  - `SourceUnavailable`
  - `Timeout`
- Future non-retryable errors remain:
  - `SourceShutdown`
  - `MalformedResponse`
  - `InvalidScope`
  - `UnsupportedMode`
- Immediate retry stays risky for future consume/dequeue modes because the
  current error shape cannot prove whether the server already processed the
  request before transport failure.

### Rationale
- The latest localhost rerun already proved the success path with
  `attempt_count=1`, `final_result=FrameRead`, `last_error=none`, and
  `retry_classification=none`.
- Repeated `inspect-latest` reads already preserved preview semantics without
  queue mutation.
- No manual evidence currently shows a transient transport failure that would
  justify immediate or bounded retry inside one scheduler read.

### Evidence Required For Future Retry
- repeated transient `SourceUnavailable` while the server process is otherwise
  healthy
- repeated transient `Timeout` during otherwise healthy manual or scheduler
  reads
- measured evidence that a second immediate transport attempt regularly
  succeeds without harming queue semantics
- for future consume/dequeue modes, stronger request/response evidence that can
  distinguish unprocessed versus already-processed failed attempts

### Next
- Return to the smallest service lifecycle planning for the current named-pipe
  handoff runtime instead of adding retry first.

### TODO Update
- Marked the classification-only versus bounded-retry decision complete.
- Moved the next task to service lifecycle planning.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Recorded the successful lifecycle-summary bounded localhost rerun for the
  named-pipe handoff manual path.
- Updated the manual guide and TODO tracking to treat the rerun as complete.
- Kept this slice docs-only and did not add retry execution, reconnect/backoff,
  or lifecycle manager behavior.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Treat the lifecycle-summary bounded localhost rerun as successful.
- Treat `attempt_count=1`, `final_result=FrameRead`, `last_error=none`, and
  `retry_classification=none` as confirmed visible success-path summary fields.
- Keep the next task as deciding whether classification-only is enough or
  whether a bounded retry wrapper is actually needed.

### Observed Stdout
- Server aggregate:
  `server named-pipe handoff bounded pipe_name=streamsync-handoff-dev max_requests=2 requests_served=2 successful_responses=2 handoff_errors=0`
- Server request 0:
  `server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=0 request_id=1 result_kind=FrameRead queue_len=1 handoff_error=none`
- Server request 1:
  `server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=1 request_id=2 result_kind=FrameRead queue_len=1 handoff_error=none`
- Client:
  `auth real encoded video frame bounded PoC sent AuthRequest 96 bytes from 0.0.0.0:54387 to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; bounded_manual_runtime=true; fragment_pacing_every=16 fragment_pacing_delay_ms=1 frames_attempted=2 frames_captured=1 frames_encoded=1 frames_sent=1 direct_sends=0 fragmented_sends=1 fragments_attempted=241 fragments_sent=241 no_frame_count=1 capture_failures=0 encode_failures=0 frame_build_failures=0 send_failures=0 stop_reason=Some(MaxFramesReached) last_send_destination=none last_send_local_source=none last_send_frame_id=none last_send_payload_len=none last_send_packet_len=none last_send_error=none`
- Switcher read 1:
  `switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest attempt_count=1 timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead final_result=FrameRead last_error=none retry_classification=none queue_len=1 frame_id=2 capture_timestamp=1777652932152093 send_timestamp=1777652932152093 queued_at=1777652932344643 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=246286`
- Switcher read 2:
  `switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=2 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest attempt_count=1 timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead final_result=FrameRead last_error=none retry_classification=none queue_len=1 frame_id=2 capture_timestamp=1777652932152093 send_timestamp=1777652932152093 queued_at=1777652932344643 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=246286`

### Conclusion
- client auth succeeded
- fragmented real encoded send succeeded
- server receive/reassembly/queue succeeded
- bounded named-pipe loop served 2 requests and respected `max_requests=2`
- `requests_served=2`, `successful_responses=2`, `handoff_errors=0`
- both switcher reads returned `FrameRead`
- `attempt_count=1` was visible
- `final_result=FrameRead` was visible
- `last_error=none` was visible
- `retry_classification=none` was visible
- `request_id` 1 and 2 were preserved
- metadata survived the server->switcher handoff
- `encoded_payload_len=246286` was preserved and non-zero
- repeated `inspect-latest` correctly returned the same frame twice without
  queue mutation
- no error collapsed into `NoFrame`
- classification-only appears sufficient for the successful path

### Next
- Decide whether classification-only is enough or whether a bounded retry
  wrapper is actually needed before adding retry behavior.

### TODO Update
- Marked the lifecycle-summary localhost rerun complete.
- Moved the next task to the classification-only versus bounded-retry decision.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Implemented the smallest switcher-side lifecycle classifier and summary
  extension above the existing one-request named-pipe handoff wrapper.
- Kept `attempt_count=1`, preserved the existing `request_id`, and did not add
  retry execution.
- Extended the one-shot switcher stdout contract with final-result / last-error
  / retry-classification visibility.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep lifecycle classification on the switcher wrapper output, not on the
  server runtime and not on the scheduler.
- Keep `FrameRead` and `NoFrame` unclassified for retry purposes.
- Classify explicit handoff errors as:
  - `SourceUnavailable`: retryable on later scheduler tick
  - `Timeout`: retryable on later scheduler tick
  - `SourceShutdown`: non-retryable
  - `MalformedResponse`: non-retryable
- Keep `attempt_count=1` and preserve the existing request-id behavior.

### Unresolved
- whether the updated lifecycle summary should be re-recorded through a fresh
  localhost manual pass
- whether a bounded retry wrapper is actually needed after classification-only
  behavior
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists

### Next
- Take a localhost manual rerun with the updated switcher lifecycle summary
  fields and record the stdout contract.
- Decide whether classification-only behavior is sufficient or whether a later
  bounded retry wrapper is justified.

### TODO Update
- Marked the lifecycle classifier / summary extension slice complete.
- Moved the next step to a summary-aware manual rerun plus bounded-retry
  necessity check.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Planned the smallest switcher-side reconnect/lifecycle policy after the
  successful bounded named-pipe localhost validation.
- Chose a no-auto-retry / classification-first first slice instead of adding
  immediate retry, bounded retry count, or backoff.
- Updated architecture and TODO docs to move the next implementation toward a
  lifecycle classifier and summary extension rather than a retry manager.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep `one scheduler read = one logical handoff request = one transport
  attempt` in the first reconnect/lifecycle slice.
- Do not auto-retry `HandoffError` in the first slice.
- Retry classification for the first slice:
  - `SourceUnavailable`: retryable on a later scheduler tick
  - `Timeout`: retryable on a later scheduler tick
  - `SourceShutdown`: non-retryable in the first slice
  - `MalformedResponse`: non-retryable
- Keep the existing `request_id` unchanged because the first lifecycle slice
  has no retries.
- If retries are added later, prefer a new transport-attempt request id per
  retry and only add a separate logical parent request id if summary/debugging
  truly needs it.

### Unresolved
- exact enum / type shape for retryable vs. non-retryable lifecycle
  classification
- whether the first lifecycle summary should be surfaced only in switcher
  wrapper output or also in the one-shot CLI text
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists

### Next
- Add the smallest switcher-side lifecycle classifier above the current
  per-request timeout summary.
- Expose `attempt_count=1`, final result, last error, elapsed milliseconds, and
  retryable/non-retryable classification through fake-runtime-testable output.

### TODO Update
- Replaced the generic reconnect/lifecycle next item with the concrete
  no-auto-retry / classification-first policy.
- Moved the next step from retry-manager planning to the smallest lifecycle
  classifier implementation.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Recorded the successful bounded localhost named-pipe handoff manual pass.
- Updated manual guidance to keep the bounded server summary command as a
  working localhost validation path.
- Updated TODO tracking so the next task moves to switcher-side minimal
  reconnect/lifecycle policy planning instead of bounded localhost validation.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Treat the bounded localhost named-pipe pass as successful.
- Keep plain pipe name usage unchanged.
- Treat repeated `inspect-latest` returning the same frame twice as expected
  preview semantics rather than a queue-mutation bug.

### Observed Stdout
- Server aggregate:
  `server named-pipe handoff bounded pipe_name=streamsync-handoff-dev max_requests=2 requests_served=2 successful_responses=2 handoff_errors=0`
- Server request 0:
  `server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=0 request_id=1 result_kind=FrameRead queue_len=1 handoff_error=none`
- Server request 1:
  `server named-pipe handoff bounded request pipe_name=streamsync-handoff-dev request_index=1 request_id=2 result_kind=FrameRead queue_len=1 handoff_error=none`
- Client:
  `auth real encoded video frame bounded PoC sent AuthRequest 96 bytes from 0.0.0.0:63648 to 127.0.0.1:5000 and received AuthResponse 55 bytes from 127.0.0.1:5000; accepted=true reason_code=Ok; bounded_manual_runtime=true; fragment_pacing_every=16 fragment_pacing_delay_ms=1 frames_attempted=2 frames_captured=1 frames_encoded=1 frames_sent=1 direct_sends=0 fragmented_sends=1 fragments_attempted=246 fragments_sent=246 no_frame_count=1 capture_failures=0 encode_failures=0 frame_build_failures=0 send_failures=0 stop_reason=Some(MaxFramesReached) last_send_destination=none last_send_local_source=none last_send_frame_id=none last_send_payload_len=none last_send_packet_len=none last_send_error=none`
- Switcher read 1:
  `switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead queue_len=1 frame_id=2 capture_timestamp=1777650284107351 send_timestamp=1777650284107351 queued_at=1777650284378630 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=251482`
- Switcher read 2:
  `switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=2 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest timeout_millis=5000 elapsed_millis=1 request_status=sent response_status=decoded result_kind=FrameRead queue_len=1 frame_id=2 capture_timestamp=1777650284107351 send_timestamp=1777650284107351 queued_at=1777650284378630 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=251482`

### Conclusion
- client auth succeeded
- fragmented real encoded send succeeded
- server receive/reassembly/queue succeeded
- bounded named-pipe loop served 2 requests and respected `max_requests=2`
- `requests_served=2`, `successful_responses=2`, `handoff_errors=0`
- both switcher reads returned `FrameRead`
- `request_id` 1 and 2 were preserved
- metadata survived the server->switcher handoff
- `encoded_payload_len=251482` was preserved and non-zero
- `elapsed_millis=1` was visible on both reads
- no handoff errors occurred
- repeated `inspect-latest` correctly returned the same frame twice without
  queue mutation

### Next
- Move to switcher-side minimal reconnect/lifecycle policy planning and the
  smallest follow-up implementation above the current per-request timeout layer.

### TODO Update
- Marked bounded localhost named-pipe manual validation complete.
- Moved the next task away from bounded manual validation and toward
  reconnect/lifecycle planning.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-02
### Type
- Codex

### Work
- Exposed the bounded server-side named-pipe handoff loop through a manual CLI
  command without adding daemon lifecycle behavior.
- Added `--receive-auth-video-queue-and-serve-handoff-many` on top of the
  existing queue-owning receive path and bounded `serve_many(..., max_requests)`
  runtime.
- Added bounded-loop stdout formatting for one aggregate summary line plus
  per-request summary lines.
- Added focused formatter tests instead of expanding real named-pipe test
  coverage.

### Changed Files
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Preserve `--receive-auth-video-queue-and-serve-handoff-once` unchanged and
  add a separate bounded manual command rather than overloading the one-shot
  surface.
- Keep bounded manual output transport-focused:
  aggregate counts plus per-request `request_id`, `result_kind`, `queue_len`,
  and observable `handoff_error`.
- Keep the command bounded by `max_requests` only; no Ctrl+C lifecycle,
  reconnect/backoff, or multi-client concurrency in this slice.

### Unresolved
- switcher-side minimal reconnect/lifecycle policy above the per-request
  timeout layer
- manual localhost pass for the bounded summary command
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists

### Next
- Take a manual localhost pass of the bounded summary command and record the
  observed stdout contract.
- Add only the smallest reconnect/lifecycle policy above the current
  switcher-side per-request timeout layer.

### TODO Update
- Marked bounded server loop summary CLI/manual exposure complete.
- Reordered the next items toward bounded localhost validation and minimal
  reconnect/lifecycle follow-up.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the smallest switcher-side per-request timeout/lifecycle
  plumbing for one named-pipe handoff request.
- Added one-request timeout config and per-request elapsed/runtime summary on
  top of the existing named-pipe handoff wrapper in `apps/switcher`.
- Kept the behavior one-request only, retry-free, and fake-runtime-testable.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep timeout scope per request only in this slice and apply it to the
  named-pipe connect/wait step rather than introducing a reconnect manager.
- Keep timeout, source unavailable, source shutdown, and malformed response as
  explicit `HandoffError` results rather than collapsing them into `NoFrame`.
- Keep elapsed/runtime summary on the switcher wrapper output so fake-runtime
  tests can verify it without local pipe I/O.

### Unresolved
- whether/how the bounded server loop summary should be exposed through a
  CLI/manual runtime command
- smallest reconnect/lifecycle policy above the new per-request timeout layer
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists

### Next
- Decide how to surface the bounded server loop summary in a manual/runtime
  command without expanding to a full daemon.
- Add only the smallest reconnect/lifecycle policy above the per-request
  timeout layer.

### TODO Update
- Marked the switcher-side per-request timeout/runtime summary slice complete
  in current position.
- Moved the next item from timeout plumbing to bounded-loop summary exposure
  plus minimal reconnect/lifecycle follow-up.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the smallest bounded server-side named-pipe accept loop over the
  existing one-shot runtime.
- Added a bounded `serve_many(..., max_requests)` runtime in `apps/server`
  that reuses the existing one-shot named-pipe runtime internally.
- Kept the same caller-owned `ServerVideoFrameQueueState` across the loop.
- Kept one client at a time and a fresh named-pipe instance per request by
  repeatedly calling the existing one-shot server runtime.
- Added aggregate loop output with:
  - `max_requests`
  - `requests_served`
  - `successful_responses`
  - `handoff_errors`
  - per-request `request_id`
  - per-request `result_kind`
  - per-request `queue_len`
- Added focused non-I/O tests for bounded summary aggregation and zero-request
  behavior.

### Changed Files
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Reuse the existing one-shot server runtime internally instead of duplicating
  pipe accept/read/write logic.
- Keep bounded-loop output as a summary structure rather than storing full
  frame payload copies for every request.
- Count a `HandoffError` response as a successful response transport-wise while
  also counting it in the explicit `handoff_errors` summary.

### Unresolved
- switcher-side per-request timeout/lifecycle plumbing
- whether/how the bounded loop summary should be exposed through a CLI/manual
  runtime command
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists

### Next
- Add the smallest switcher-side per-request timeout/lifecycle policy.
- Decide how to surface the bounded server loop summary in a manual/runtime
  command without expanding to a full daemon.

### TODO Update
- Marked the bounded server `serve_many(..., max_requests)` runtime complete in
  current position.
- Replaced the old bounded-loop implementation item with the next switcher
  timeout/lifecycle item.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the requested planning/docs slice for the continuous
  named-pipe accept loop / reconnect / lifecycle approach.
- Reviewed the successful one-shot localhost named-pipe handoff result and
  used it as the baseline for the next bounded runtime/service slice.
- Documented the smallest server-side loop as a bounded `max_requests`
  one-client-at-a-time accept loop over fresh named-pipe instances.
- Documented the smallest switcher-side lifecycle policy as one request per
  scheduler read with one bounded timeout per request and no automatic retry
  manager in the first slice.
- Documented per-request request-id correlation ownership and logging policy.
- Documented the out-of-scope items and the smallest next implementation slice.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The first continuous server loop should be bounded by `max_requests`, not
  indefinite `Ctrl+C` service mode.
- Keep one client at a time and create a fresh named-pipe instance per
  request.
- Keep `ServerVideoFrameQueueState` caller-owned and alive across repeated
  handoff reads; queue ownership does not move into a daemon/service layer.
- Keep `request_id` switcher-owned. The server loop echoes and logs it but does
  not allocate ids.
- Keep switcher lifecycle minimal: one connect/write/read/disconnect per
  scheduler read, one timeout per request, and no built-in retry loop in the
  first slice.

### Planned Answers
- smallest server continuous loop:
  bounded `serve_many(queue_state, pipe_name, max_requests)` over the existing
  one-shot runtime
- first loop control:
  fixed number of requests first; not Ctrl+C and not timeout-based shutdown as
  the primary policy
- pipe instance policy:
  fresh named-pipe instance per request
- request-id correlation:
  log `pipe_name`, loop request index, `request_id`, `client_id`, `run_id`,
  `read_mode`, request status, response status, and result kind per request
- server queue ownership:
  caller-owned queue state survives across the bounded loop
- smallest switcher lifecycle:
  one request per scheduler read, one timeout per request, zero automatic
  retries in the first slice, reconnect on the next caller/scheduler tick only
- failure mapping:
  - `Timeout`: one request write/read timeout
  - `SourceUnavailable`: connect/listen/open failure before an in-flight
    request
  - `SourceShutdown`: EOF / broken pipe / server close during an in-flight
    request
  - `MalformedResponse`: decode failure, bad length prefix, mismatched
    `request_id`, or invalid response shape
- out of scope:
  OBS, 4-view, protocol wire-format changes, H.264 behavior changes,
  switcher-side fragment reassembly, multi-client concurrency, persistent
  daemon/service installation
- smallest next implementation slice:
  bounded server `max_requests` loop + per-request summary/logging while
  keeping the switcher runtime essentially unchanged apart from minimal timeout
  plumbing if needed

### Next
- Implement the bounded `max_requests` server accept loop and per-request
  correlation logging.
- Then add only the smallest switcher-side timeout/lifecycle plumbing needed
  to exercise that bounded loop.

### TODO Update
- Replaced the generic lifecycle planning item with the concrete next bounded
  loop implementation slice.
- Recorded the bounded loop, fresh pipe instance policy, and switcher-owned
  request-id correlation policy in current position.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Recorded the first successful localhost one-shot named-pipe handoff manual
  pass.
- Reviewed the provided server and switcher stdout lines and confirmed that the
  one-shot named-pipe transport returned `FrameRead` successfully with matching
  request/response metadata on both sides.
- Updated the manual checklist to recommend plain pipe names for these CLI
  commands.
- Recorded the operational note that the full Windows pipe path
  `\\.\pipe\streamsync-handoff-dev` produced `SourceUnavailable` in the same
  manual testing, while the plain name `streamsync-handoff-dev` succeeded.
- Updated TODO tracking to mark the localhost one-shot handoff pass complete
  and move the next task to continuous accept-loop / lifecycle planning.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Treat the one-shot named-pipe localhost manual pass as successful.
- Recommend plain pipe names for the current server/switcher one-shot CLI
  commands.
- Keep this as a docs/tracking slice only; no code or architecture change is
  needed from the successful run.

### Observed Stdout
- Switcher:
  `switcher named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest request_status=sent response_status=decoded result_kind=FrameRead queue_len=1 frame_id=2 capture_timestamp=1777641579940665 send_timestamp=1777641579940665 queued_at=1777641580062096 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=383887`
- Server:
  `server named-pipe handoff once pipe_name=streamsync-handoff-dev request_id=1 client_id=player1 run_id=streamsync-dev-session read_mode=inspect-latest request_status=decoded response_status=written result_kind=FrameRead queue_len=1 frame_id=2 capture_timestamp=1777641579940665 send_timestamp=1777641579940665 queued_at=1777641580062096 width=1920 height=1080 fps_nominal=30 codec=H264 is_keyframe=false encoded_payload_len=383887`

### Conclusions
- named-pipe one-shot handoff succeeded
- switcher request was sent
- server decoded the request
- server returned `FrameRead`
- switcher decoded the response
- `request_id` matched
- `client_id` / `run_id` / `read_mode` matched
- `frame_id` matched
- `capture_timestamp` / `send_timestamp` / `queued_at` matched
- `width` / `height` / `fps_nominal` / `codec` / `is_keyframe` matched
- `encoded_payload_len` matched
- `queue_len` matched
- metadata survived the server->switcher handoff
- `encoded_payload_len=383887` was non-zero and realistic for a real H.264
  frame
- no `NoFrame` or `HandoffError` occurred in the successful run

### Next
- Plan the smallest continuous accept loop / reconnect / lifecycle slice over
  the existing successful one-shot named-pipe handoff path.

### TODO Update
- Marked the localhost one-shot named-pipe manual pass complete.
- Moved the next task to continuous accept loop / lifecycle planning.
- Recorded the plain pipe name guidance in operational docs.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Reviewed the submitted localhost one-shot named-pipe manual validation
  result.
- Confirmed that the pasted `[SERVER STDOUT]`, `[CLIENT STDOUT]`, and
  `[SWITCHER STDOUT]` blocks contained only `...`, so there was no observable
  auth, fragment-send, reassembly, queue, request-id, or handoff-result data
  to evaluate.
- Recorded the result as inconclusive in the manual real encoded video
  checklist.
- Updated TODO tracking so the next action remains rerunning the localhost
  manual validation and capturing real stdout before moving to continuous
  accept-loop / lifecycle planning.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- This turn is a docs/session-log update only.
- No tiny code fix is justified because no concrete failure output was
  provided.
- Do not start continuous accept loop / lifecycle implementation or planning
  from this result alone.

### Review Answers
- client auth succeeded: not proven
- client sent fragmented real encoded frames: not proven
- server received / reassembled / queued frames: not proven
- server served one named-pipe handoff request: not proven
- switcher connected and sent the handoff request: not proven
- request_id matched between request and response: not proven
- switcher result kind (`FrameRead` / `NoFrame` / `HandoffError`): not proven
- frame metadata survival / encoded payload length / queue length: not proven
- no-frame vs handoff-error classification: not proven
- next step: rerun localhost manual validation and paste real stdout; do not
  treat this as evidence for a bug or for lifecycle planning

### Next
- Rerun the localhost server/client/switcher one-shot named-pipe validation and
  record the actual stdout lines.
- After that, decide between a tiny fix and continuous accept-loop / lifecycle
  planning based on concrete evidence.

### TODO Update
- Kept the first todo item on recording a real localhost stdout-backed result.
- Updated stale lower-summary todo text that still described the one-shot
  named-pipe commands as docs-only.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the documented one-shot named-pipe manual CLI commands in the
  smallest possible shape.
- Added a new server CLI command,
  `--receive-auth-video-queue-and-serve-handoff-once`, that reuses the
  existing bounded auth/video queue launcher and then serves exactly one
  named-pipe handoff request from the resulting caller-owned queue state.
- Added a new switcher CLI command,
  `--read-queued-frame-handoff-once`, that builds one handoff input, performs
  one named-pipe request/response, and prints the manual stdout contract.
- Kept both commands one-shot only and did not add a continuous accept loop,
  reconnect policy, or broader lifecycle/service orchestration.
- Added focused CLI helper tests for handoff mode parsing and summary
  formatting so default validation does not depend on real pipe I/O.
- Updated the manual real encoded video checklist to include the new commands
  and their minimum stdout fields.

### Changed Files
- `apps/server/src/main.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Reuse the existing queue-owning server launcher for the server one-shot
  handoff command instead of creating a standalone named-pipe server command.
- Keep the switcher one-shot command config-free in the first CLI slice.
- Keep `request_id` optional on the switcher command. If omitted, use the
  one-shot wrapper default of `1`, which matches the current wrapper-owned
  monotonic initial value for a fresh process.
- Keep real named-pipe smoke tests isolated; default validation should rely on
  non-I/O handoff tests plus new CLI helper tests.

### Unresolved
- record a real localhost manual pass for the new one-shot named-pipe CLI pair
- continuous named-pipe accept loop / reconnect / lifecycle orchestration
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists.

### Next
- Record a real localhost manual pass using the new one-shot named-pipe server
  and switcher commands.
- Then move to the smallest continuous named-pipe accept loop / lifecycle
  slice.

### TODO Update
- Replaced the old "implement-or-defer one-shot command shape" item with the
  next manual-pass recording item.
- Updated current position to record that both one-shot named-pipe CLI commands
  now exist.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the requested planning/docs slice for the one-shot named-pipe
  manual invocation shape and request-id exposure.
- Reviewed the current named-pipe runtime/wrapper, existing server queue-owning
  manual command, and the direct-receive switcher diagnostic command.
- Documented the smallest useful server command shape as an extension of the
  existing queue-owning manual launcher rather than a standalone pipe server.
- Documented the smallest useful switcher command shape as a read-only
  one-shot named-pipe request command with explicit `pipe_name`, `client_id`,
  `run_id`, `read_mode`, and optional `request_id`.
- Documented that `request_id` should be optional: preserve it when supplied,
  and otherwise use the existing wrapper-owned monotonic policy from its
  initial value.
- Documented the minimum stdout contract for manual validation: request/response
  status, request_id, client/run/mode, result kind, queue length, and frame
  metadata when a frame is returned.
- Documented that these command shapes should remain docs-only for now, and
  should not be implemented until the stdout contract and ownership split are
  accepted.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`
- `docs/operations/manual-real-encoded-video-poc.md`

### Decisions
- Do not add real one-shot named-pipe manual commands in this slice.
- The first useful server one-shot handoff command must reuse the existing
  queue-owning receive launcher rather than pretending pipe service is useful
  without queued frames.
- The first useful switcher one-shot handoff command should not take a config
  path in the initial slice.
- Keep `--receive-auth-video-queue-once` as the queue-owning server diagnostic.
- Keep `--live-two-view-switcher-once` as the direct-receive diagnostic/legacy
  path and do not revive it as the main server-mediated path.

### Unresolved
- whether to implement the documented one-shot command shapes now or after one
  more docs/CLI review
- exact stdout wording and field order for the future commands
- request-id exposure in the eventual manual/runtime command
- continuous named-pipe service/client lifecycle
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists.

### Next
- Decide whether the documented one-shot named-pipe command shapes should now
  become real CLI commands.
- If yes, implement the smallest queue-owning server launcher extension and
  read-only switcher one-shot request command with the documented stdout
  contract.

### TODO Update
- Replaced the generic manual/request-id planning item with a concrete
  implement-or-defer decision for the documented command shapes.
- Recorded the documented command shape and stdout contract in current
  position.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the smallest thin switcher-side handoff wrapper over the
  one-request / one-response named-pipe runtime.
- Added a `SwitcherQueuedFrameHandoff` implementation that delegates one read
  to a named-pipe runtime, preserving the existing handoff abstraction for the
  downstream switcher pipeline.
- Added a minimal request-id policy for that wrapper:
  `read_handoff_frame_with_request_id` preserves a caller-supplied request id,
  while the trait-based `read_handoff_frame` consumes a wrapper-owned
  monotonic `u64`.
- Added a small runtime trait so default tests can use fake/stub runtimes
  instead of real named-pipe I/O.
- Added focused fake-runtime tests for request-id preservation, monotonic id
  generation, `FrameRead`, `NoFrameAvailable`, explicit `HandoffError`, and
  local runtime encode failure staying explicit rather than degrading into
  `NoFrame`.
- Updated architecture and operations docs to record the thin wrapper
  responsibility and the minimal request-id policy.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the thin wrapper in `apps/switcher` rather than moving request-id policy
  into `net-core`.
- Use a wrapper-owned monotonic `u64` counter as the default policy for the
  existing handoff trait method.
- Keep real named-pipe smoke tests ignored and rely on fake-runtime tests for
  default handoff validation.

### Unresolved
- manual invocation shape for the one-request / one-response named-pipe path
- how request-id policy should be exposed in a manual/runtime command
- continuous named-pipe service/client lifecycle
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists.

### Next
- Define the smallest manual invocation shape for the named-pipe one-shot
  handoff path.
- Decide how request-id should be provided or surfaced in that manual/runtime
  entry point.

### TODO Update
- Marked the thin named-pipe-backed handoff wrapper complete in the current
  position.
- Replaced the wrapper item with the next manual/request-id-shaping item.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the smallest Windows named-pipe one-request / one-response
  runtime connection for the server->switcher handoff path.
- Added a Windows-only server-side named-pipe runtime in `apps/server` that
  creates one pipe instance, accepts one client, reads one framed handoff
  request with the existing codec, runs
  `ServerSwitcherQueuedFrameHandoffHandlerBoundary`, writes one framed
  response, and returns.
- Added a Windows-only switcher-side named-pipe runtime in `apps/switcher`
  that builds one DTO request, connects to the named pipe, writes one framed
  request, reads one framed response, decodes it with the existing codec, maps
  it through `SwitcherServerQueuedFrameHandoffClientAdapterBoundary`, and
  returns the raw request/response plus mapped handoff result.
- Added IO failure mapping on the switcher runtime so named-pipe connect/read
  failures become explicit `SwitcherQueuedFrameHandoffError` results where
  applicable.
- Kept the runtime slice one-request / one-response only; no service loop,
  reconnect, lifecycle orchestration, or request-id generator was added.
- Added focused Windows-only non-I/O tests for named-pipe path validation and
  IO-error-to-handoff-error mapping. Real named-pipe smoke tests are present
  but isolated with `#[ignore]` because they are not stable enough for the
  default handoff validation command.
- Updated architecture and operations docs to record the new runtime
  responsibility split and move the next task to wrapper/manual invocation
  shaping.

### Changed Files
- `apps/server/Cargo.toml`
- `apps/server/src/lib.rs`
- `apps/switcher/Cargo.toml`
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep named-pipe runtime Windows-only and one-request / one-response for now.
- Keep smoke tests isolated with `#[ignore]` and rely on focused non-I/O
  mapping tests in default validation.
- Keep request/response visibility in runtime outputs so `request_id`
  correlation remains inspectable without adding a broader lifecycle layer.

### Unresolved
- thin wrapper from the named-pipe runtime into the existing
  `SwitcherQueuedFrameHandoff` abstraction
- request-id generation policy for runtime/manual use
- manual command shape over the named-pipe runtime
- continuous named-pipe service/client lifecycle
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists.

### Next
- Add a thin wrapper that lets existing switcher handoff consumers call the
  named-pipe runtime through the existing `SwitcherQueuedFrameHandoff`
  abstraction.
- Decide the smallest manual/runtime invocation shape and request-id policy for
  the named-pipe one-shot path.

### TODO Update
- Marked the Windows named-pipe one-request / one-response runtime slice
  complete in the current position.
- Replaced the old runtime-connection items with the next wrapper/manual-shape
  slice.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the smallest server-side single-request handoff handler and the
  smallest switcher-side DTO request/response adapter shape over the new
  server->switcher handoff DTO/codec.
- Added a transport-neutral server handoff handler in `apps/server` that
  consumes `ServerSwitcherQueuedFrameHandoffRequest`, delegates queue reads to
  `ServerVideoFrameQueueReadBoundary`, and returns one
  `ServerSwitcherQueuedFrameHandoffResponse`.
- Added a transport-neutral switcher client-adapter boundary in
  `apps/switcher` that builds DTO requests from the existing switcher handoff
  input shape and maps DTO responses back into the existing
  `SwitcherQueuedFrameHandoffResult` / `SwitcherQueuedFrameHandoffError`
  shape.
- Extended `SwitcherSingleViewSelectedEncodedFrame` to preserve codec metadata
  so the new server->switcher DTO path does not drop `codec`.
- Added focused handler/adapter tests for frame-read, no-frame, invalid scope,
  request-id echo preservation on the server side, and full handoff-error-code
  mapping on the switcher side.
- Updated architecture and operations docs to record the new handler/adapter
  responsibilities and move the next task to runtime/service connection.

### Changed Files
- `apps/server/src/lib.rs`
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the server handoff handler as a thin mapping boundary over
  `ServerVideoFrameQueueReadBoundary`; do not let it own named-pipe runtime,
  service lifecycle, or switcher scheduling.
- Keep the switcher client side as a DTO request builder / response mapper for
  now; do not implement named-pipe I/O in this slice.
- Preserve codec metadata in the existing switcher encoded-frame handoff shape
  rather than dropping it at the DTO-response mapping boundary.

### Unresolved
- Windows named-pipe one-request / one-response service runtime
- switcher named-pipe client runtime
- request-id generation / correlation policy in the eventual runtime layer
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists.

### Next
- Connect the transport-neutral codec / server handler / switcher client
  adapter to a Windows named-pipe one-request / one-response runtime.
- Define the minimum service/client lifecycle around that first named-pipe
  runtime.

### TODO Update
- Marked the server single-request handoff handler and switcher DTO
  request/response adapter complete in the current position.
- Replaced the old next-item code slice with the next runtime/service
  connection slice.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the smallest focused code slice for the planned real
  server->switcher handoff: transport-neutral DTOs plus an explicit
  length-prefixed binary codec.
- Added request/response DTOs for the server->switcher queued-frame handoff in
  `crates/net-core`, keeping them separate from the existing UDP protocol
  codec.
- Added framing-aware request/response encode/decode helpers that preserve
  `request_id`, frame metadata, payload bytes, and mapped handoff error codes.
- Added focused round-trip and malformed/truncated-frame tests for the handoff
  codec.
- Updated architecture and operations docs to record codec placement in
  `crates/net-core` and move the next task to the server single-request
  handler / switcher client adapter slice.

### Changed Files
- `crates/net-core/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the first server->switcher handoff DTO/codec transport-neutral and
  test-only for now.
- Place the shared DTO/codec in `crates/net-core` so both `apps/server` and
  `apps/switcher` can depend on one internal handoff codec without changing
  `crates/protocol`.
- Use explicit length-prefixed binary framing at the codec boundary, but do not
  add named-pipe I/O in this slice.

### Unresolved
- server-side single-request handoff handler implementation
- switcher-side client adapter implementation
- concrete Windows named-pipe runtime/service lifecycle
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists.

### Next
- Add the server single-request handoff handler around
  `ServerVideoFrameQueueReadBoundary`.
- Add the switcher-side client adapter that implements
  `SwitcherQueuedFrameHandoff` over the transport-neutral codec.

### TODO Update
- Marked the handoff DTO/codec slice complete in the current position.
- Replaced the old next-item planning entry with the concrete next code slice:
  server single-request handler and switcher client adapter.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-net-core handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
  passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the requested planning/docs slice for the first real
  server->switcher transport over the existing fallible queued-frame handoff
  contract.
- Reviewed the existing switcher-pull/read queue handoff contract, the current
  fallible validation boundary, and the current protocol documentation.
- Documented the first concrete transport choice as local IPC with a byte-
  stream request/response shape, using a Windows named pipe as the first
  production-like implementation.
- Documented that the first production-like path remains switcher-pull/read and
  does not move targetTime selection to the server.
- Documented the exact first request/response shape for server->switcher
  handoff and the mapping to `SwitcherQueuedFrameHandoffError`.
- Documented that the initial serialization should be a small length-prefixed
  binary codec, separate from the existing client/server UDP wire protocol.
- Documented the relation to `crates/protocol`, `VideoFrame`, and `net-core`.
- Documented the smallest implementation slice after planning: DTO/codec,
  server single-request handler, and switcher named-pipe client adapter.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- First real server->switcher transport: local IPC byte stream.
- First concrete implementation on Windows: named pipe.
- Keep switcher-pull/read as the first production-like direction.
- Do not include target timestamps in the first server->switcher request.
- Do not reuse the existing client/server UDP `ProtocolMessage` wire format for
  internal server->switcher handoff.
- Keep `crates/protocol` unchanged; use a separate internal handoff codec and
  put byte-stream runtime/framing helpers in `net-core`.
- Keep `--live-two-view-switcher-once` as direct receive diagnostic/legacy
  only.

### Unresolved
- exact code placement for the first handoff DTO/codec and byte-stream helpers
- first named-pipe service lifecycle boundaries beyond one request -> one
  response
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists.

### Next
- Define the server->switcher handoff DTO/error types and binary codec.
- Add the server single-request handoff handler and the switcher client
  adapter.

### TODO Update
- Replaced the generic transport-planning item with the concrete next code
  slice: DTO/codec plus server/client transport adapters.
- Recorded the local IPC / named-pipe decision in the current position.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed docs files.

## 2026-05-01
### Type
- Codex

### Work
- Implemented the requested planning/docs slice for the fallible
  server-mediated 2-view validation path.
- Reviewed the current `run_fallible_with_runtimes` /
  `run_fallible_from_handoff_with_runtimes` path, the existing direct
  switcher receive diagnostic command, the server queue manual runtime, and the
  manual real encoded video checklist.
- Documented that a dedicated manual/runtime entry point is not needed before
  real server->switcher transport planning.
- Documented that `--live-two-view-switcher-once` remains a direct receive
  diagnostic/legacy path and must not be revived as the main validation path.
- Documented that `--receive-auth-video-queue-once` proves only server
  auth/reassembly/queue and is not a reusable entry for the fallible switcher
  path.
- Recorded the smallest later debug-only command shape only as a fallback
  option, not as the current next task.
- Updated the manual checklist to keep command flow unchanged and point the
  next slice at transport planning.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`
- `docs/operations/manual-real-encoded-video-poc.md`

### Decisions
- Do not add a dedicated manual/runtime entry point for
  `SwitcherServerMediatedTwoViewValidationBoundary::run_fallible_*` before
  production server->switcher transport planning.
- Keep `--live-two-view-switcher-once` as direct switcher receive
  diagnostic/legacy only.
- Do not reuse the direct switcher receive diagnostic as the main
  server-mediated validation path.
- If a debug-only command is needed later, make it switcher-owned and feed it
  from the existing in-process `ServerVideoFrameQueueState`; keep fake/failing
  handoff sources in focused tests instead of manual runtime flow.

### Unresolved
- real server->switcher transport planning over the fallible queued-frame
  handoff contract
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or
  deprecated after the transport-backed server-mediated path exists.

### Next
- Plan the real server->switcher transport around the existing fallible
  handoff contract.

### TODO Update
- Replaced the manual/runtime entry-point decision item with real
  server->switcher transport planning.
- Recorded that no dedicated fallible manual/runtime command will be added in
  this step.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed docs files.

## 2026-05-01
### Type
- Codex

### Work
- Added the smallest fallible server-mediated 2-view validation path.
- Added `SwitcherServerMediatedTwoViewHandoffValidationOutput`.
- Added `run_fallible_with_runtimes` to `SwitcherServerMediatedTwoViewValidationBoundary`.
- Added `run_fallible_from_handoff_with_runtimes` to the same boundary.
- The queue-state entry point wraps caller-owned `ServerVideoFrameQueueState` in the existing `SwitcherInProcessQueuedFrameHandoff`.
- The generic handoff entry point accepts any `SwitcherQueuedFrameHandoff` for source-error validation and future transport adapters.
- The path runs fallible scheduler, fallible scheduler decode/render adapter, fallible decode/render connection, fallible display policy, fallible display-composition adapter, and fallible composed-canvas render connection.
- The output keeps every stage visible.
- Selected/rendered, no-frame, waiting, handoff/source error, stale, no-display placeholder, and source-error placeholder states remain distinct.
- Handoff/source errors are not collapsed into no-frame, waiting, partial selection, or generic placeholders.
- Skipped, error, stale, and placeholder sides do not create fake decoded frames.
- Added focused tests for both eligible queues rendering, eligible + future frame waiting, eligible + empty queue no-frame, eligible + handoff error source-error placeholder, both handoff errors with no render, consume all-or-nothing, preview no-mutation, aggregate `HandoffError` preservation, and visible stage outputs.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the fallible path on the existing server-mediated validation boundary instead of replacing the non-fallible `SwitcherQueuedFrameSource` path.
- Use the existing in-process handoff implementation for queue-state tests.
- Keep targetTime selection in switcher.
- Keep server as ingest/reassembly/queue owner.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Decide whether the fallible server-mediated validation path needs a manual/runtime entry point before real transport planning.
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or deprecated after the server-mediated path exists.

### Next
- Decide whether to add a manual/runtime entry point for the fallible server-mediated validation path.

### TODO Update
- Marked the fallible server-mediated validation path as complete.
- Updated the next item to decide whether a manual/runtime entry point is needed before real transport planning.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Added the smallest fallible display-composition adapter -> composed-canvas render connection.
- Added `SwitcherTwoViewHandoffDisplayCompositionRenderConnectionInput`.
- Added `SwitcherTwoViewHandoffDisplayCompositionRenderConnectionRenderResult`.
- Added `SwitcherTwoViewHandoffDisplayCompositionRenderConnectionOutput`.
- Added `SwitcherTwoViewHandoffDisplayCompositionRenderConnectionBoundary`.
- The connection consumes `SwitcherTwoViewHandoffDisplayCompositionAdapterOutput`.
- It preserves aggregate scheduler status, including aggregate `HandoffError`.
- It keeps adapter output, composition result, and render result visible in the connection output.
- It calls the existing `SwitcherTwoViewCompositionBoundary` only when at least one side has real decoded composition input.
- It calls the existing `SwitcherTwoViewComposedCanvasRenderBoundary` only when composition produces a real composed canvas.
- Updated and held-previous sides can render through the existing composition/render path.
- Stale, no-display, and source-error placeholders remain skipped sides and do not create fake decoded frames.
- Source-error placeholder detail remains explicit in the adapter output and is not collapsed into generic no-display there.
- Added focused tests for both updates, update + held previous, update + stale placeholder, update + no-display placeholder, update + source-error placeholder, both source-error placeholders, aggregate error preservation, no fake frames for placeholder/error sides, and no render for both source-error placeholders.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the fallible display-composition render connection parallel to the existing non-fallible connection because source-error detail is only visible in the fallible adapter output.
- For placeholder-only fallible output, synthesize the existing `EmptyPlaceholder` / `NoRenderableCanvas` result from adapter instructions instead of invoking render.
- Keep the existing `SwitcherTwoViewCompositionInput`, composer, and composed-canvas renderer unchanged.
- Keep targetTime selection in switcher.
- Keep server as ingest/reassembly/queue owner.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Connect the fallible handoff path into the server-mediated validation boundary without moving targetTime selection to server.
- production H.264 encoder configuration / error logging policy
- Decide later whether `--live-two-view-switcher-once` should be renamed or deprecated after the server-mediated path exists.

### Next
- Plan the smallest fallible handoff path connection into the server-mediated validation boundary.

### TODO Update
- Marked the fallible display-composition adapter -> composed-canvas render connection as complete.
- Updated the next item to plan the fallible handoff path connection into the server-mediated validation boundary.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Added the smallest fallible display policy -> composition adapter / placeholder boundary.
- Added `SwitcherTwoViewHandoffDisplayCompositionAdapterInput`.
- Added `SwitcherTwoViewHandoffDisplayCompositionSideInstruction`.
- Added `SwitcherTwoViewHandoffDisplayCompositionAdapterOutput`.
- Added `SwitcherTwoViewHandoffDisplayCompositionAdapterBoundary`.
- The adapter consumes `SwitcherTwoViewHandoffDisplayPolicyOutput`.
- It maps update to updated frame, hold to held previous frame, stale to explicit stale placeholder, generic no-display to explicit no-display placeholder, and source-error no-display to explicit source-error placeholder.
- Source-error placeholder detail remains explicit in the adapter output and is not collapsed into generic no-display there.
- The existing `SwitcherTwoViewCompositionInput` still narrows skipped sides to the current generic skipped-side reason shape for the unchanged composer.
- Added focused tests for both updates, update+held previous, source-error hold detail, source-error placeholder without previous frame, stale placeholder, no-display placeholder, source-error not no-frame/waiting, no fake frames for skipped/error sides, and aggregate `HandoffError` preservation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the fallible composition adapter separate from the existing non-fallible display composition adapter because source-error placeholder detail needs its own instruction shape.
- Preserve source-error placeholder detail in the adapter output even though the current composition input can only express generic skipped sides.
- Do not create fake decoded frames for skipped or source-error sides.
- Keep targetTime selection in switcher.
- Keep server as ingest/reassembly/queue owner.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Connect `SwitcherTwoViewHandoffDisplayCompositionAdapterOutput` to the composed-canvas render connection while keeping source-error placeholder detail visible in the adapter output.
- Decide whether the fallible render connection should mirror the non-fallible display-composition render connection or whether a more general adapter/render connection shape is warranted.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the smallest composed-canvas render connection for fallible display composition adapter output.

### TODO Update
- Marked the fallible display policy -> composition adapter boundary as complete.
- Updated the next item to plan composed-canvas render connection for `SwitcherTwoViewHandoffDisplayCompositionAdapterOutput`.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Added the smallest fallible display-policy / placeholder decision boundary.
- Added `SwitcherTwoViewHandoffDisplayPolicyInput`.
- Added `SwitcherTwoViewHandoffDisplayDecision`.
- Added `SwitcherTwoViewHandoffDisplayPolicyOutput`.
- Added `SwitcherTwoViewHandoffDisplayPolicyBoundary`.
- The boundary consumes `SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput`.
- Newly rendered sides become update decisions with real decoded frames.
- No-frame and waiting skips can hold previous frames or become no-display placeholders.
- Handoff/source-error skips preserve source-error detail, can hold previous frames, can become stale when max hold is exceeded, or can become explicit no-display placeholders when no previous frame exists.
- Aggregate `HandoffError` remains visible in the display policy output.
- Added focused tests for both updates, render+no-frame hold, render+waiting hold, render+source-error hold, source-error placeholders without previous frames, both source errors, source-error not no-frame/waiting, stale previous on source error, and no fake update frames for source-error placeholders.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the fallible display policy separate from the existing non-fallible display policy because source-error skipped side detail is a different result shape.
- Do not collapse source-error skips into no-frame or waiting.
- Do not create fake decoded frames for skipped or source-error sides.
- Keep targetTime selection in switcher.
- Keep server as ingest/reassembly/queue owner.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Connect `SwitcherTwoViewHandoffDisplayPolicyOutput` to a composition adapter / placeholder path without hiding source-error detail.
- Decide whether to add a parallel fallible display-composition adapter or generalize the existing composition adapter skipped-side detail.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the smallest composition adapter / placeholder connection for fallible display policy output.

### TODO Update
- Marked the fallible display policy boundary as complete.
- Updated the next item to plan composition adapter / placeholder connection for `SwitcherTwoViewHandoffDisplayPolicyOutput`.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Added the smallest fallible adapter output -> display-policy-facing decode/render connection.
- Added `SwitcherTwoViewHandoffDecodeRenderSkippedSide`.
- Added `SwitcherTwoViewHandoffDecodeRenderConnectionResult`.
- Added `SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionInput`.
- Added `SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput`.
- Added `SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionBoundary`.
- The connection decodes/renders only `RenderFrame` instructions through existing H.264 decode and window render boundaries.
- The connection preserves no-frame, waiting, and handoff/source error skips as distinct display-policy-facing results.
- The connection keeps aggregate `HandoffError` visible and does not force source errors into existing no-frame or waiting selection shapes.
- Added focused tests for both rendered, render+no-frame, render+waiting, render+source-error, both source errors, source-error not no-frame/waiting, no fake decode/render calls for source-error skips, and aggregate handoff error preservation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep this path separate from the existing non-fallible scheduler decode/render connection.
- Do not call decode/render hooks for no-frame, waiting, or handoff/source-error instructions.
- Do not convert `SkipHandoffError` into `SwitcherTwoViewDecodeRenderInput`.
- Keep targetTime selection in switcher.
- Keep server as ingest/reassembly/queue owner.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Connect `SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput` to display policy / placeholder handling without hiding source-error skips.
- Decide whether display policy should gain a parallel fallible input shape or an adapter from fallible skipped sides into placeholder detail.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the smallest display-policy / placeholder connection for fallible decode/render connection output.

### TODO Update
- Marked the fallible adapter output -> display-policy-facing decode/render connection as complete.
- Updated the next item to plan the display-policy / placeholder connection for `SwitcherTwoViewHandoffSchedulerDecodeRenderConnectionOutput`.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Added the smallest fallible 2-view scheduler decode/render-facing adapter.
- Added `SwitcherTwoViewHandoffSchedulerDecodeRenderSideInstruction`.
- Added `SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterInput`.
- Added `SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput`.
- Added `SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterBoundary`.
- The adapter maps selected sides to renderable frame instructions, no-frame sides to explicit no-frame skips, waiting sides to explicit waiting skips, and handoff/source failures to explicit `SkipHandoffError`.
- The adapter preserves aggregate `HandoffError` and does not collapse handoff errors into no-frame, waiting, partial selection, or fake selected frames.
- The existing `SwitcherTwoViewDecodeRenderInput` is produced only when no source error would be hidden by that shape.
- Added focused tests for both selected, selected+waiting, selected+no-frame, selected+handoff-error, both handoff errors, error not treated as no-frame/waiting, no fake frames for error sides, and selected metadata preservation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the fallible adapter separate from the existing non-fallible scheduler decode/render adapter.
- Do not synthesize `SwitcherTwoViewDecodeRenderInput` when either side has a handoff/source error, because the existing decode/render input shape cannot represent that error without hiding it as no-frame or waiting.
- Keep targetTime selection in switcher.
- Keep server as ingest/reassembly/queue owner.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Connect `SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput` to the next display/decode path without hiding source-error skips.
- Decide whether the next slice should add a fallible decode/render connection boundary or first adapt source-error skips into display policy placeholders.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the smallest follow-up connection for fallible adapter output, preserving source-error skip instructions before display policy.

### TODO Update
- Marked the fallible 2-view scheduler decode/render-facing adapter as complete.
- Updated the next item to plan the follow-up connection for `SwitcherTwoViewHandoffSchedulerDecodeRenderAdapterOutput`.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed after rerun with a longer timeout. The first run hit the 120-second command timeout before returning a result.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-05-01
### Type
- Codex

### Work
- Verified the requested fallible 2-view targetTime handoff scheduler slice is already present in the working tree.
- Confirmed `SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary` uses the fallible single-client targetTime handoff source per view.
- Confirmed per-view selected / no-frame / waiting / handoff-error outcomes remain visible.
- Confirmed aggregate `HandoffError` remains distinct from partial selected, no-frame, and waiting.
- Confirmed consume mode previews both sides first and does not mutate either side when one side has a handoff error.

### Changed Files
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- No duplicate implementation was added because the requested boundary, result type, status, docs, and tests already exist.
- Keep next task focused on the decode/render adapter path for fallible 2-view scheduler results.

### Unresolved
- Plan or implement the decode/render adapter path for `SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult`.
- Decide how handoff errors should surface through display policy without creating fake decoded frames.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the smallest decode/render adapter path for fallible 2-view scheduler results.

### TODO Update
- Refreshed `docs/operations/todo.md` timestamp for the May 1 verification.
- Kept the next item as the fallible scheduler -> decode/render adapter planning slice.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed.

## 2026-04-30
### Type
- Codex

### Work
- Added the smallest fallible 2-view targetTime handoff scheduler.
- Added `SwitcherTwoViewTargetTimeHandoffSourceSchedulerStatus`.
- Added `SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult`.
- Added `SwitcherTwoViewTargetTimeHandoffSourceSchedulerBoundary`.
- The scheduler calls the fallible single-client targetTime handoff source for each view.
- Per-view outcomes preserve selected, no-frame, waiting, and handoff/source error.
- Aggregate status adds explicit `HandoffError` and does not collapse handoff errors into partial selected, no-frame, or waiting.
- Consume mode previews both sides first and only consumes both when both preview results are selected.
- Added focused tests for both selected, selected+waiting, selected+no-frame, selected+handoff-error, both handoff errors, handoff error not no-frame/waiting, consume all-or-nothing, consume no-mutation on handoff error, and metadata preservation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep targetTime selection in switcher.
- Keep server as ingest/reassembly/queue owner.
- Preserve handoff/source errors through the scheduler instead of mapping them to existing non-error scheduler states.
- Keep this path separate from the existing non-fallible 2-view scheduler and decode/render adapter for this slice.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Plan or implement the decode/render adapter path for `SwitcherTwoViewTargetTimeHandoffSourceSchedulerResult`.
- Decide how handoff errors should surface through display policy without creating fake decoded frames.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the smallest decode/render adapter path for fallible 2-view scheduler results.

### TODO Update
- Marked the fallible 2-view targetTime handoff scheduler as implemented.
- Set the next task to planning the decode/render adapter path for fallible 2-view scheduler results.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed on rerun with a longer timeout. The first parallel run hit the 10-minute command timeout while waiting/compiling.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-30
### Type
- Codex

### Work
- Added the smallest targetTime-aware path for fallible handoff results.
- Added `SwitcherSingleClientTargetTimeHandoffSourceResult`.
- Added `SwitcherSingleClientTargetTimeHandoffSourceBoundary`.
- The boundary consumes `SwitcherQueuedFrameHandoffConsumerResult` through `SwitcherQueuedFrameHandoffConsumerBoundary`.
- Frame/no-frame outcomes reuse existing targetTime selection behavior.
- Handoff/source errors remain explicit and are not collapsed into no-frame or waiting.
- Consume mode previews oldest first and only dequeues when the candidate is eligible at or before target.
- Added focused tests for eligible selection, waiting, no-frame, every handoff error variant remaining explicit, metadata preservation, preview no-mutation, consume mutation only when selected, and consume waiting without mutation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep targetTime selection in switcher.
- Keep server as ingest/reassembly/queue owner.
- Keep handoff/source errors separate from no-frame and waiting.
- Keep the new fallible targetTime path single-client for this slice.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Plan or implement the 2-view scheduler path for `SwitcherSingleClientTargetTimeHandoffSourceResult`.
- Decide how handoff errors should surface through scheduler decode/render adapter and display policy without becoming partial/no-frame/waiting.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the smallest 2-view scheduler consumer for fallible targetTime handoff results.

### TODO Update
- Marked the fallible single-client targetTime handoff source as implemented.
- Set the next task to planning the 2-view scheduler path for fallible targetTime handoff results.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-30
### Type
- Codex

### Work
- Added the smallest switcher-side consumer for `SwitcherQueuedFrameHandoff` results.
- Added `SwitcherQueuedFrameHandoffConsumerResult`.
- Added `SwitcherQueuedFrameHandoffConsumerBoundary`.
- The consumer maps `FrameRead` into `SwitcherSingleClientQueueSourceResult::FrameAvailable`.
- The consumer maps `NoFrameAvailable` into `SwitcherSingleClientQueueSourceResult::NoFrameAvailable`.
- The consumer keeps `HandoffError` explicit and does not collapse it into no-frame.
- Added focused tests for frame conversion, no-frame preservation, all handoff error variants remaining distinct from no-frame, metadata preservation, preview no-mutation, and scoped consume mutation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep targetTime selection in switcher.
- Keep the handoff consumer transport-neutral and in-process testable.
- Reuse the existing queue-source result shape only for frame/no-frame outcomes.
- Keep source/handoff errors separate for later scheduler/display surfacing.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Plan or implement the next targetTime/scheduler path that can consume `SwitcherQueuedFrameHandoffConsumerResult`.
- Decide how handoff errors should surface in scheduler/display policy without becoming no-frame or waiting.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the smallest targetTime/scheduler consumer for fallible handoff results.

### TODO Update
- Marked the handoff consumer boundary as implemented.
- Set the next task to planning the targetTime/scheduler consumer path for fallible handoff results.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-30
### Type
- Codex

### Work
- Added the minimal transport-neutral / fallible server->switcher queued-frame handoff contract.
- Added `SwitcherQueuedFrameHandoffInput`.
- Added `SwitcherQueuedFrameHandoffResult` with selected frame, no-frame, and handoff-error outcomes.
- Added `SwitcherQueuedFrameHandoffError` with `SourceUnavailable`, `Timeout`, `InvalidScope`, `UnsupportedMode`, `MalformedResponse`, and `SourceShutdown`.
- Added `SwitcherQueuedFrameHandoff`.
- Added `SwitcherInProcessQueuedFrameHandoff` backed by `SwitcherInProcessServerQueueFrameSource`.
- Added focused tests for selected frame, no-frame, invalid scope, fake source error propagation, metadata preservation, preview no-mutation, and consume scoped mutation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the fallible handoff transport-neutral.
- Treat no-frame as a normal queue result, distinct from source/handoff failure.
- Keep targetTime selection in switcher.
- Keep the first implementation in-process over the existing server queue source.
- Do not add IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, or switcher-side fragment reassembly.

### Unresolved
- Decide where `SwitcherQueuedFrameHandoff` should first be consumed in the switcher path.
- Decide concrete production transport only after the fallible handoff contract is exercised by a consumer.
- production H.264 encoder configuration / error logging policy

### Next
- Add the smallest switcher-side consumer for `SwitcherQueuedFrameHandoff` results while keeping targetTime selection in switcher.

### TODO Update
- Marked the fallible handoff contract as implemented.
- Set the next task to deciding and implementing the smallest consumer for fallible handoff results.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-30
### Type
- Codex

### Work
- Planned the smallest production/manual server->switcher handoff hook over `SwitcherQueuedFrameSource`.
- Chose a transport-neutral, fallible handoff contract as the next slice.
- Decided not to add another manual command in the next slice because the current in-process validation already exercises the source-driven switcher pipeline.
- Decided not to start a local IPC/TCP pull source prototype yet because framing, serialization, lifecycle, timeout, and error-shaping should follow an explicit handoff contract.
- Confirmed the switcher should request one latest/oldest/dequeue read per `client_id + run_id`; queue snapshots are diagnostic-only and targetTime-aware selection remains switcher-owned.
- Listed future handoff failures separately from normal no-frame: source unavailable, request timeout, invalid/unknown client or run scope, unsupported read mode, malformed source response, and source shutdown.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep server as ingest/auth/UDP receive/buffer tuning/fragment reassembly/per-client queue owner.
- Keep switcher as queued-frame read, targetTime selection, decode/render, display policy, composition, and output owner.
- Keep pull/read direction.
- Do not implement IPC/TCP/UDP/shared-memory transport in the next slice.
- Keep OBS output, 4-view orchestration, retry/retransmit, protocol wire-format changes, switcher-side fragment reassembly, late-drop mutation, and H.264 behavior changes out of scope.

### Unresolved
- Implement the minimal transport-neutral / fallible handoff contract around `SwitcherQueuedFrameSource`.
- Decide concrete production transport only after the fallible handoff contract is tested.
- production H.264 encoder configuration / error logging policy

### Next
- Add the minimal fallible handoff contract and test it through `SwitcherInProcessServerQueueFrameSource`.

### TODO Update
- Set the next task to adding the transport-neutral / fallible server->switcher handoff contract.
- Kept manual command and transport implementation deferred.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-30
### Type
- Codex

### Work
- Routed the single-client targetTime source through `SwitcherQueuedFrameSource`.
- Added source-based selection to `SwitcherTwoViewTargetTimeSourceSchedulerBoundary`.
- Updated `SwitcherServerMediatedTwoViewValidationBoundary` with `run_from_source_with_runtimes`.
- Kept the existing `ServerVideoFrameQueueState` validation entry point by wrapping it in `SwitcherInProcessServerQueueFrameSource`.
- Added a focused server-mediated validation test that calls the queued-frame source abstraction directly.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep `SwitcherInProcessServerQueueFrameSource` as the current concrete source implementation.
- Keep all downstream stage outputs visible: scheduler, decode/render, display policy, display-composition adapter, and composed-canvas render.
- Keep IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, protocol wire-format changes, H.264 behavior changes, switcher-side fragment reassembly, and late-drop mutation out of scope.

### Unresolved
- Plan the smallest production/manual server->switcher handoff hook over `SwitcherQueuedFrameSource`.
- Decide whether the next validation should be a manual runtime command or 4-view expansion planning.
- production H.264 encoder configuration / error logging policy

### Next
- Plan the next server->switcher handoff hook now that the in-process validation path depends on `SwitcherQueuedFrameSource`.

### TODO Update
- Marked the server-mediated 2-view validation path as routed through `SwitcherQueuedFrameSource`.
- Set the next task to planning the smallest production/manual server->switcher handoff hook.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-30
### Type
- Codex

### Work
- Added the minimal switcher-facing queued-frame source interface.
- Added `SwitcherQueuedFrameSource` with `read_queued_frame(client_id, run_id, mode)` via the existing `SwitcherSingleClientQueueSourceInput` / `SwitcherSingleClientQueueSourceResult` shape.
- Added `SwitcherInProcessServerQueueFrameSource`, an in-process adapter over caller-owned `ServerVideoFrameQueueState`.
- The adapter delegates to `SwitcherSingleClientQueueSourceBoundary`, which continues to delegate to `ServerVideoFrameQueueReadBoundary`.
- Added focused tests for selected-frame read, missing-run no-frame, preview no-mutation, consume mutating only the requested run, and frame metadata preservation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the first production-facing server->switcher handoff in-process and pull/read based.
- Reuse existing queue input/result shapes so no-frame status, read mode, queue length, and encoded-frame metadata remain visible.
- Keep IPC/TCP/UDP/shared-memory transport, OBS output, 4-view orchestration, switcher-side fragment reassembly, protocol wire-format changes, late-drop mutation, and H.264 behavior changes out of scope.

### Unresolved
- Route `SwitcherServerMediatedTwoViewValidationBoundary` through `SwitcherQueuedFrameSource`.
- Decide cross-process server->switcher transport only after the in-process interface is proven.
- production H.264 encoder configuration / error logging policy
- 4-view expansion planning

### Next
- Use `SwitcherQueuedFrameSource` for the server-mediated 2-view validation path while preserving current scheduler / display / composition / render visibility.

### TODO Update
- Marked the queued-frame source trait/interface as completed in the current position.
- Set the next task to routing server-mediated 2-view validation through `SwitcherQueuedFrameSource`.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed on rerun with a longer timeout. The first run hit the 120s command timeout before returning a result.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-30
### Type
- Codex

### Work
- Planned the production server -> switcher handoff direction after the in-process server-mediated validation boundary.
- Chose switcher-pull/read as the initial production direction instead of server-push.
- Defined the smallest handoff interface shape as a switcher-facing queued-frame source over `ServerVideoFrameQueueReadBoundary`.
- Listed the data that should cross the boundary:
  - `client_id`
  - `run_id`
  - `frame_id`
  - `capture_timestamp`
  - `send_timestamp` when available
  - queued / observed timestamp
  - encoded H.264 payload bytes and length
  - width / height
  - nominal FPS
  - keyframe flag when available
  - codec
  - queue read mode
  - remaining/current per-client queue length
  - explicit no-frame result
- Clarified that waiting, late, stale, and placeholder status remain switcher-side downstream decisions, not server->switcher handoff fields.
- Documented the next implementation slice as an in-process trait/interface and adapter over `ServerVideoFrameQueueReadBoundary`.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Initial production handoff direction: switcher-pull/read.
- First implementation mechanism: in-process trait/interface plus adapter.
- Do not implement local IPC, TCP, UDP, shared memory, or a new protocol wire format in the next slice.
- Do not add another manual runtime command in this planning slice.
- Keep OBS output, 4-view orchestration, retransmit/retry, switcher-side fragment reassembly, late-frame queue mutation, and H.264 decode/render behavior changes out of scope.

### Unresolved
- Implement the minimal queued-frame source trait/interface over `ServerVideoFrameQueueReadBoundary`.
- Decide a concrete cross-process transport only after the in-process interface is proven.
- production H.264 encoder configuration / error logging policy
- 4-view expansion planning

### Next
- Add the minimal switcher queued-frame source trait/interface and in-process server queue adapter.

### TODO Update
- Set the next task to implementing the minimal switcher-pull/read queued-frame source interface.
- Kept production transport, OBS output, and 4-view orchestration deferred.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed docs.

## 2026-04-30
### Type
- Codex

### Work
- Added `SwitcherServerMediatedTwoViewValidationBoundary`.
- The boundary takes caller-owned `ServerVideoFrameQueueState` that may contain direct `VideoFrame` packets or server-reassembled `VideoFrameFragment` output.
- Connected the existing in-process path:
  - `SwitcherTwoViewTargetTimeSourceSchedulerBoundary`
  - `SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary`
  - `SwitcherTwoViewDisplayPolicyBoundary`
  - `SwitcherTwoViewDisplayCompositionAdapterBoundary`
  - `SwitcherTwoViewDisplayCompositionRenderConnectionBoundary`
- Kept scheduler result, decode/render connection output, display policy output, display-composition adapter output, and composed render connection output visible in the boundary result.
- Added focused tests for:
  - two eligible server queue frames rendering through the composed canvas path
  - one eligible frame plus one future frame preserving waiting/no-display placeholder without a fake decoded frame
  - one eligible frame plus one empty queue preserving no-frame/no-display placeholder
  - consume mode remaining all-or-nothing when one side is waiting
  - preview mode not mutating server queues
- Updated architecture and operations docs.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept the slice in-process and test-oriented.
- Reused the existing server queue-backed scheduler and display/composition/render boundaries.
- Did not add a manual command in this slice.
- Did not define production server->switcher transport.
- Did not implement OBS output, 4-view orchestration, protocol wire-format changes, switcher-side fragment reassembly, late-drop mutation, or H.264 decode/render behavior changes.

### Unresolved
- Decide whether the next server-mediated step is a manual/runtime command over this boundary or production transport planning.
- production H.264 encoder configuration / error logging policy
- 4-view expansion planning

### Next
- Decide and implement the next server-mediated validation step before expanding to 4-view.

### TODO Update
- Marked the server-mediated in-process validation boundary as completed.
- Set the next task to either manual/runtime command wiring over this boundary or production server->switcher transport planning.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed: 84 passed, 0 failed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed: 22 passed, 0 failed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed: 3 passed, 0 failed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed: 12 passed, 0 failed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-30
### Type
- Codex

### Work
- Recorded the topology decision that the main real encoded video path is client -> server -> switcher.
- Clarified that server owns ingest concerns: auth, UDP receive, receive-buffer tuning, `VideoFrameFragment` reassembly, queue insertion, and queue read boundaries.
- Clarified that switcher owns sync/display/output concerns: server queue consumption, shared targetTime selection, H.264 decode, display policy, composition, composed-canvas rendering, and later OBS-window presentation.
- Documented that `--live-two-view-switcher-once` remains a diagnostic / legacy direct receive path for complete `VideoFrame` packets and is not suitable for fragmented real encoded validation.
- Documented that fragment reassembly should not be duplicated in switcher while server already owns it.
- Identified the smallest next implementation slice as an in-process server-mediated two-view validation over reassembled queued frames:
  - server receive / reassembly / queue output
  - `ServerVideoFrameQueueReadBoundary`
  - `SwitcherSingleClientTargetTimeSourceBoundary`
  - `SwitcherTwoViewTargetTimeSourceSchedulerBoundary`
  - `SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary`
  - `SwitcherTwoViewDisplayPolicyBoundary`
  - `SwitcherTwoViewDisplayCompositionAdapterBoundary`
  - `SwitcherTwoViewDisplayCompositionRenderConnectionBoundary`

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Prefer server queue pull/read for the next minimal validation slice.
- Do not decide a production server push transport yet.
- Keep `--live-two-view-switcher-once` as diagnostic direct receive for now; consider rename/deprecation after server-mediated validation exists.
- Do not implement OBS output, 4-view orchestration, protocol wire-format changes, or switcher-side fragment reassembly.

### Unresolved
- Implement the smallest server-mediated two-view switcher source validation.
- Decide later whether the server-to-switcher production handoff is push, pull, or another transport after the in-process boundary is proven.
- production H.264 encoder configuration / error logging policy
- 4-view expansion planning

### Next
- Add a minimal in-process command or boundary that takes reassembled server queue output and drives switcher targetTime -> display policy -> composition -> composed render for two clients.

### TODO Update
- Set the next task to server-mediated 2-client switcher source validation.
- Marked direct switcher receive as diagnostic / legacy for complete `VideoFrame` packets, not the fragmented real encoded path.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed docs.

## 2026-04-30
### Type
- Codex

### Work
- Inspected `--live-two-view-switcher-once` implementation and example configs to clarify the real manual process topology.
- Confirmed the command loads a server-style config through `ServerAuthResponsePocLauncher`, not `configs/examples/switcher.example.toml`.
- Confirmed the switcher process binds `server.bind_host` / `server.bind_port` from that config, so with `configs/examples/server.example.toml` it owns `0.0.0.0:5000`.
- Confirmed the switcher validates `AuthRequest.shared_token` using the loaded `[auth.clients.*]` entries from the server-style config.
- Confirmed a separate `stream-sync-server` process is not required for this command and would conflict if it binds the same address.
- Confirmed clients must send to the switcher-owned manual socket, usually `127.0.0.1:5000`, using matching client ids and shared tokens from `configs/examples/server.example.toml`.
- Confirmed the direct switcher UDP source accepts complete authenticated `VideoFrame` packets; `VideoFrameFragment` packets remain non-video in this path, so server-side fragment reassembly is not part of `--live-two-view-switcher-once`.
- Updated the manual real encoded video PoC docs and TODO to clarify topology, ports, configs, and current limitation.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Docs/tracking updates only.
- Did not update architecture because the architecture docs already describe the switcher live manual runtime as switcher-owned and no new boundary was introduced.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, protocol wire-format changes, or H.264 decode/render behavior changes.

### Unresolved
- Run the corrected 2-client manual validation topology and record real stdout counters.
- If real captured frames are sent as `VideoFrameFragment`, decide whether the next minimal fix is switcher-side fragment reassembly or a client/manual setting that forces complete `VideoFrame` packets for this validation.
- Decide whether to add a minimal display-policy-chain manual diagnostic command after the corrected manual topology is validated.

### Next
- Start only the switcher manual runtime with `configs/examples/server.example.toml`, then start two clients targeting `127.0.0.1:5000` with matching `player1` / `player2` tokens, and record auth / packet / render counters.

### TODO Update
- Clarified the manual validation topology: no separate server process, server-style config is used by switcher, clients connect to the switcher-owned server bind port.
- Kept real-counter 2-client manual validation as the next task.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed docs.

## 2026-04-30
### Type
- Codex

### Work
- Reviewed the submitted 2-client manual validation stdout blocks.
- Determined the review is inconclusive because the switcher, client 1, and client 2 stdout blocks contained only `...`.
- Recorded that the following manual validation questions cannot be proven from the submitted text:
  - both clients authenticated successfully
  - both clients sent real encoded frames
  - switcher received / reassembled / queued frames for both clients
  - shared targetTime selection selected frames from both clients
  - H.264 decode succeeded for both selected frames
  - 2-view composition produced a composed frame
  - composed canvas render succeeded
  - waiting / no-frame / stale-like cases appeared
- Updated the manual checklist and TODO to require rerunning the same 2-client command flow with real stdout counters before deciding on a display-policy-chain diagnostic command.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- No tiny fix is indicated because no concrete failure output was provided.
- No new diagnostic command is justified yet because the current manual validation result is not observable from the submitted stdout.
- Did not update architecture because no architecture changed and no new boundary was introduced.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, protocol wire-format changes, or H.264 decode/render behavior changes.

### Unresolved
- Rerun the 2-client manual validation and record real switcher/client stdout counters.
- Decide whether to add a minimal display-policy-chain manual diagnostic command after real manual stdout is available.
- production H.264 encoder configuration / error logging policy
- 4-view expansion planning

### Next
- Rerun `--live-two-view-switcher-once` with two bounded real encoded clients and record accepted auth, sent frames, accepted / queued frames, targetTime selection, decode, composition, render, and stop counters.

### TODO Update
- Kept 2-client manual validation as the next task, but clarified that the submitted stdout review was inconclusive and the same validation must be rerun with real counters.
- Kept display-policy-chain diagnostic command as a decision after real manual stdout is available.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed: 79 passed, 0 failed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed: 22 passed, 0 failed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed: 3 passed, 0 failed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed: 12 passed, 0 failed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed docs.

## 2026-04-30
### Type
- Codex

### Work
- Inspected the existing manual client/server/switcher runtime hooks for the next 2-client validation step.
- Confirmed the current clean manual command is `--live-two-view-switcher-once` plus two bounded authenticated real encoded client senders.
- Confirmed that current manual runtime validates:
  - two clients authenticating against the switcher-owned manual runtime
  - accepted UDP video frames entering switcher-owned caller-local queues
  - shared targetTime selection
  - H.264 decode
  - 2-view composition
  - composed canvas render
- Confirmed the current manual runtime does not yet route live traffic through the newer queue-backed scheduler decode/render adapter -> display policy -> display-composition adapter -> display-composition render connection chain.
- Updated the manual real encoded video checklist with the smallest 2-client manual validation path and pass/fail criteria.
- Documented the smallest future diagnostic command shape if the display-policy chain needs manual live validation before 4-view expansion.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept this as a planning/docs slice.
- Did not add a new diagnostic command because the existing command already covers the next two-client manual queue/source/selection/decode/composition/render validation.
- Deferred a new display-policy-chain manual diagnostic command until after the documented 2-client manual run records a result.
- Did not update architecture because no new boundary was introduced.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, protocol wire-format changes, or H.264 decode/render behavior changes.

### Unresolved
- Run and record the 2-client manual validation.
- Decide whether to add a minimal display-policy-chain manual diagnostic command after the manual run.
- production H.264 encoder configuration / error logging policy
- 4-view expansion planning

### Next
- Run `--live-two-view-switcher-once` with two bounded real encoded clients and record accepted frame / targetTime selection / composed render counters.

### TODO Update
- Marked the display composition render connection as completed in the current focus.
- Set the next task to 2-client manual validation and recording.

### Validation
- `cargo fmt` passed.
- `cargo fmt --check` passed.
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1` passed: 79 passed, 0 failed.
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1` passed: 22 passed, 0 failed.
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1` passed: 3 passed, 0 failed.
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1` passed: 12 passed, 0 failed.
- `cargo check --workspace` passed.
- `git diff --check` passed with line-ending warnings for changed files.

## 2026-04-29
### Type
- Codex

### Work
- Added the minimal display-composition adapter -> composed canvas render connection.
- Added `SwitcherTwoViewDisplayCompositionRenderConnectionBoundary`.
- Added connection input/output types that keep adapter output, composition result, and render connection result visible together.
- Reused `SwitcherTwoViewCompositionBoundary` and `SwitcherTwoViewComposedCanvasRenderBoundary`.
- Rendered only when composition produced a real composed frame.
- Kept both-placeholder output explicit as `NoRenderableCanvas` without calling the render runtime.
- Kept invalid composition explicit as `CompositionInvalid`.
- Added focused tests for:
  - both updated sides rendering through the composed canvas path
  - updated + held previous sides rendering with source distinction preserved
  - stale placeholder remaining explicit without fake decoded input
  - no-display placeholders remaining explicit without render runtime calls
  - mixed renderable + placeholder preserving render result and placeholder detail

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept the connection in-process and testable.
- Reused the existing composition and composed-canvas render boundaries.
- Did not create fake decoded frames for stale or no-display placeholders.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, protocol wire-format changes, or H.264 decode/render behavior changes.

### Unresolved
- 4-view expansion planning.
- 2-client manual validation with bounded real encoded senders into live two-view switcher.
- production H.264 encoder configuration / error logging policy

### Next
- Plan 4-view expansion or run the next 2-client manual validation now that the 2-view display/composition/render path is validated.

### TODO Update
- Marked display policy -> composition adapter as completed and added the composed canvas render connection validation as completed.
- Updated the next task toward 4-view expansion planning or 2-client manual validation.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
  - first run timed out after compilation; rerun with longer timeout passed
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check` (passed with existing LF-to-CRLF conversion warnings)

## 2026-04-29
### Type
- Codex

### Work
- Added the minimal display policy -> 2-view composition adapter.
- Added `SwitcherTwoViewDisplayCompositionAdapterBoundary`.
- Added explicit adapter-side composition instructions:
  - `UseUpdatedFrame`
  - `UseHeldPreviousFrame`
  - `UseStalePlaceholder`
  - `UseNoDisplayPlaceholder`
- Mapped update and hold decisions to decoded `SwitcherTwoViewCompositionInput` sides using real decoded frames.
- Mapped stale and no-display placeholder decisions to skipped composition sides while keeping the original skip reason visible in the adapter output.
- Added focused tests for both updates, update + hold previous, stale previous placeholder, no-display placeholder, and skip reason preservation.
- Updated architecture and TODO docs with the display policy -> composition adapter boundary and next validation task.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept the adapter in-process and testable.
- Kept final composition/render behavior separate from display policy decisions.
- Did not create fake frames for stale or no-display placeholder decisions.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, protocol wire-format changes, or H.264 decode/render behavior changes.

### Unresolved
- Running the display policy -> composition adapter through the composed canvas render boundary.
- Deciding whether 4-view expansion should happen before or after composed render validation.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Validate the display policy -> composition adapter through the composed canvas render path, or plan 4-view expansion.

### TODO Update
- Marked the display policy -> 2-view composition input adapter complete.
- Set next task to composed canvas render validation or 4-view expansion planning.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check` (passed with existing LF-to-CRLF conversion warnings)

## 2026-04-29
### Type
- Codex

### Work
- Added the first minimal 2-view display policy boundary after scheduler/adapter/decode-render connection.
- Added `SwitcherTwoViewDisplayPolicyBoundary`.
- Added caller-owned previous displayed frame state with `SwitcherTwoViewDisplayedFrame`.
- Added explicit display decisions:
  - `Update`
  - `HoldPrevious`
  - `PreviousFrameStale`
  - `NoDisplayPlaceholder`
- Added optional `max_hold_duration_micros` handling for stale previous-frame decisions.
- Added tests for both newly rendered frames, selected + waiting with previous frame, selected + no-frame with previous frame, waiting/no-frame without previous frame, and stale previous frame past max hold duration.
- Updated architecture and TODO docs with the display policy boundary and next connection/4-view planning task.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept display policy in-process and testable.
- Kept previous displayed frame state caller-owned.
- Preserved waiting/no-frame/skip reasons inside display decisions.
- Did not create fake frames for skipped views.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, protocol wire-format changes, or H.264 decode/render behavior changes.

### Unresolved
- Connecting display policy decisions to existing 2-view composition/render.
- Deciding whether 4-view expansion should happen before or after display policy connection validation.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Validate display policy decisions against the 2-view composition/render path, or plan 4-view expansion.

### TODO Update
- Marked the two-view display policy boundary complete.
- Set next task to display policy connection validation or 4-view expansion planning.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher two_view_display_policy -- --test-threads=1`
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check` (passed with existing LF-to-CRLF conversion warnings)

---

## 2026-04-29
### Type
- Codex

### Work
- Added live-like in-process validation for the queue-backed scheduler -> adapter -> decode/render connection.
- Built two queue-backed views with multiple timestamps and ran them through:
  - `SwitcherTwoViewTargetTimeSourceSchedulerBoundary`
  - `SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary`
  - `SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary`
  - `SwitcherTwoViewDecodeRenderBoundary`
- Added tests verifying both selected views reach decode/render hooks.
- Added tests verifying a waiting view remains an explicit skip and does not create a fake frame.
- Added tests verifying a no-frame view remains an explicit skip.
- Added consume-mode validation that the scheduler remains all-or-nothing even when the connection renders the currently selected eligible preview side.
- Added preview-mode validation that queues are not mutated.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept this slice test-first and in-process; no manual runtime or CLI was added.
- Reused the existing connection and decode/render boundaries.
- Did not update architecture because no new boundary was introduced.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, final display policy, protocol wire-format changes, or H.264 decode/render behavior changes.

### Unresolved
- Display policy after explicit decode/render skips remains undecided: hold previous frame, black fallback, and partial render behavior.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Plan display policy after decode/render skip propagation, or decide whether 4-view expansion should come first.

### TODO Update
- Marked scheduler adapter -> decode/render live-like queue validation complete.
- Set next item to display policy planning.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher two_view_scheduler_decode_render_connection_live_like -- --test-threads=1`
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check` (passed with existing LF-to-CRLF conversion warnings)

---

## 2026-04-29
### Type
- Codex

### Work
- Added the smallest in-process connection from queue-backed scheduler result through the scheduler decode/render adapter into `SwitcherTwoViewDecodeRenderBoundary`.
- Added `SwitcherTwoViewSchedulerDecodeRenderConnectionBoundary`.
- Added connection output that keeps both adapter output and decode/render result visible for diagnostics.
- Added focused connection tests for:
  - both selected reaching the decode/render runtime hooks
  - selected + waiting preserving the waiting skip
  - selected + no-frame preserving the no-frame skip
  - waiting/no-frame not triggering decode or render fake input
- Updated architecture and TODO docs with the connection boundary and next display-policy planning task.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept this slice diagnostic and in-process.
- Reused the existing `SwitcherTwoViewDecodeRenderBoundary` unchanged.
- Kept selected/no-frame/waiting behavior explicit through adapter output and decode/render skip results.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, protocol wire-format changes, final display fallback policy, or H.264 decode/render behavior changes.

### Unresolved
- Final display policy after decode/render skips: hold previous frame, black fallback, and partial render behavior remain undecided.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Plan the next display policy slice or run manual live two-view verification with bounded clients.

### TODO Update
- Marked scheduler adapter -> existing decode/render connection validation complete.
- Updated next items toward display policy planning or manual live two-view verification.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher two_view_scheduler_decode_render_connection -- --test-threads=1`
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check` (passed with existing LF-to-CRLF conversion warnings)

---

## 2026-04-29
### Type
- Codex

### Work
- Added the minimal adapter from queue-backed 2-view scheduler results to the existing 2-view decode/render input path.
- Added `SwitcherTwoViewSchedulerDecodeRenderAdapterBoundary`.
- Added per-side adapter instructions that preserve explicit scheduler status mapping:
  - selected frames become renderable frame instructions
  - no-frame results become no-frame skip instructions
  - waiting-for-target results become waiting skip instructions
- Mapped adapter output into `SwitcherTwoViewDecodeRenderInput` without changing the existing decode/render boundary.
- Added focused tests for both-selected, selected+waiting, selected+no-frame, and waiting/no-frame no-fake-frame cases.
- Updated architecture and TODO docs with the new adapter boundary and next validation task.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept the adapter in-process and testable.
- Preserved the existing `SwitcherTwoViewDecodeRenderBoundary` input contract.
- Kept waiting/no-frame reasons explicit in adapter output because the existing decode/render selection type has no scheduler-specific waiting variant.
- Did not implement OBS output, 4-view orchestration, late-drop mutation, protocol wire-format changes, or H.264 decode/render behavior changes.

### Unresolved
- Minimal validation/connection slice that runs adapter output through the existing decode/render boundary.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Add a minimal adapter-to-decode/render validation or connection slice without late-drop mutation.

### TODO Update
- Marked the scheduler-result to 2-view decode/render input adapter complete.
- Updated next items toward adapter validation / minimal decode/render connection.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher two_view_scheduler_decode_render_adapter -- --test-threads=1`
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check` (passed with existing LF-to-CRLF conversion warnings)

---

## 2026-04-29
### Type
- Codex

### Work
- Added live-like queued-frame validation for the queue-backed 2-view targetTime source scheduler.
- Added scheduler-level mode `SwitcherTwoViewTargetTimeSourceSchedulerMode`.
- Chose all-or-nothing synchronized consumption for two-view consume mode through `ConsumeOldestAtOrBeforeAllSelected`.
- Added single-client `PreviewOldestIfAtOrBefore` so the scheduler can preview both oldest candidates before mutating queues.
- Updated consume behavior so no queue is mutated unless both views are selected for the shared target timestamp.
- Added tests for progressing preview target timestamps and progressing all-or-nothing consume target timestamps.
- Updated existing consume scheduler test to verify one eligible side plus one waiting side does not partially consume.
- Updated architecture and TODO docs with the consume policy decision and next adapter-planning task.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Selected Option B for two-view consume mode: all-or-nothing synchronized consumption.
- Kept preview mode non-mutating.
- Kept this slice in-process and test-only; no UDP live receive connection was added.
- Did not add 4-view orchestration, OBS output, H.264 decode/render changes, late-drop mutation, or protocol wire-format changes.

### Unresolved
- Adapter from queue-backed scheduler per-view results to the existing 2-view decode/render path.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Plan the smallest adapter from queue-backed scheduler results into the existing 2-view decode/render path without late-drop mutation.

### TODO Update
- Marked queue-backed 2-view targetTime source scheduler live-like validation complete.
- Updated next items toward adapter planning for the existing 2-view decode/render path.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check` (passed with existing LF-to-CRLF conversion warnings)

---

## 2026-04-29
### Type
- Codex

### Work
- Added the minimal queue-backed 2-view targetTime source scheduler boundary.
- Added `SwitcherTwoViewTargetTimeSourceSchedulerBoundary`, which calls `SwitcherSingleClientTargetTimeSourceBoundary` once per configured view.
- Kept the scheduler scoped to two explicit `client_id + run_id` view configs and one shared `target_timestamp`.
- Reused explicit single-client source modes so preview remains non-mutating and consume behavior is only available through `ConsumeOldestAtOrBefore`.
- Added per-view result preservation and aggregate scheduler status: all selected, partial selected, waiting, or no frames.
- Added focused tests for both-selected, selected+waiting, selected+no-frame, preview no-mutation, consume-only-eligible, and both-empty no-frames behavior.
- Updated architecture and TODO docs for the new boundary and next validation path.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Built the 2-view scheduler on the queue-backed single-client targetTime source instead of changing the older direct jitter-buffer selector.
- Kept this slice in-process and diagnostic.
- Did not add UDP live receive connection, 4-view orchestration, OBS output, H.264 decode/render changes, late-drop mutation, or protocol wire-format changes.

### Unresolved
- Live-like validation or fixture path for the queue-backed 2-view scheduler.
- How the queue-backed scheduler should feed the existing 2-view decode/render path without late-drop mutation.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Validate or connect the queue-backed 2-view targetTime source scheduler with a live-like queued-frame fixture.

### TODO Update
- Marked the queue-backed 2-view targetTime source scheduler boundary complete.
- Updated current focus and next items toward scheduler validation / connection to the existing 2-view path.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-switcher two_view -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check` (passed with existing LF-to-CRLF conversion warnings)

---

## 2026-04-29
### Type
- Codex

### Work
- Added focused queue-like validation tests for the single-client targetTime source boundary.
- Added an empty-queue `NoFrameAvailable` test.
- Added a live-like progression test that previews latest without mutation, consumes the oldest eligible frame, then verifies the remaining newer frame returns waiting without dequeue.
- Kept the validation tests in-process and did not add a CLI launcher.
- Updated TODO to mark targetTime source validation complete and move the next task toward multi-client sync planning.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Used focused tests instead of a manual launcher because the existing in-process boundary is directly testable.
- Did not connect UDP live receive directly to switcher.
- Did not change protocol wire format.
- Did not add 4-view orchestration, OBS output, H.264 decode/render changes, or late-drop mutation.

### Unresolved
- How the single-client targetTime source should feed the existing two-view scheduler without late-drop mutation.
- Smallest multi-client sync validation path over queued encoded frames.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Plan or implement the smallest multi-client sync validation path over queued encoded frames.

### TODO Update
- Marked single-client targetTime source queue-like validation tests complete.
- Updated next items toward two-view / multi-client sync planning.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-29
### Type
- Codex

### Work
- Added the smallest single-client targetTime-aware source boundary over the switcher queue source.
- Added `SwitcherSingleClientTargetTimeSourceBoundary`, scoped by `client_id + run_id`.
- Added explicit targetTime source modes:
  - `PreviewLatestIfAtOrBefore`
  - `ConsumeOldestAtOrBefore`
- Added `PreviewOldest` to `SwitcherSingleClientQueueSourceMode` so consume mode can inspect oldest before dequeue and avoid unexpected mutation.
- Added selected / no-frame / waiting result types with target timestamp, candidate diagnostics, queue length, and consumed flag.
- Added focused targetTime tests for preview selection, preview waiting, consume selection/dequeue, consume waiting without dequeue, and missing-run no-frame.
- Updated architecture and TODO docs to move the next task toward manual/live-like validation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept this slice single-client, in-process, and diagnostic/manual.
- `PreviewLatestIfAtOrBefore` does not mutate the queue.
- `ConsumeOldestAtOrBefore` mutates only after the oldest candidate is confirmed to be at or before the target timestamp.
- Waiting results do not mutate the queue.
- Did not change protocol wire format.
- Did not add H.264 decode/render changes, late-drop mutation, 4-view orchestration, or OBS output.

### Unresolved
- Manual fixture / live-like validation for the single-client targetTime source.
- Deciding how this source should feed the existing two-view scheduler without late-drop mutation.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Add manual fixture or live-like validation for the single-client targetTime source boundary.

### TODO Update
- Marked the switcher single-client targetTime source boundary as complete.
- Updated current position with queue-source-backed targetTime selection.
- Updated next items toward validation and future two-view scheduler integration.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-switcher target_time -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-28
### Type
- Codex

### Work
- Added the smallest in-process switcher/sync-facing source boundary over the server video frame queue read boundary.
- Added `SwitcherSingleClientQueueSourceBoundary`, scoped by `client_id + run_id`.
- Added explicit source modes: `PreviewLatest` maps to server `InspectLatest`, and `ConsumeOldest` maps to server `DequeueOldest`.
- Mapped successful queue reads into the existing `SwitcherSingleViewSelectedEncodedFrame` handoff shape.
- Added focused switcher tests for non-mutating latest preview, run-scoped oldest consume, and missing-run no-frame reporting.
- Updated architecture and TODO docs to move the next task toward targetTime / jitter-buffer integration.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept the first source boundary single-client and manual/diagnostic.
- Made preview vs consume behavior explicit in `SwitcherSingleClientQueueSourceMode`.
- Reused `ServerVideoFrameQueueReadBoundary` instead of duplicating queue access behavior in switcher code.
- Did not change protocol wire format.
- Did not add late-drop mutation, 4-view orchestration, OBS output, targetTime integration, H.264 decode, or rendering in this slice.

### Unresolved
- Connecting the single-client queue source boundary to targetTime / jitter-buffer selection.
- Deciding whether the first targetTime integration should use `PreviewLatest` or `ConsumeOldest`.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Connect the single-client queue source boundary to targetTime / jitter-buffer selection without adding late-drop mutation.

### TODO Update
- Marked the switcher single-client queue source boundary as complete.
- Updated current position with client/run scoped switcher source support.
- Updated next items toward targetTime / jitter-buffer integration over this source.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher single_client_queue_source -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-28
### Type
- Codex

### Work
- Added the smallest server-side queued encoded frame read/dequeue boundary for the next sync/switcher handoff.
- Added `ServerVideoFrameQueueReadBoundary`, keyed by `client_id + run_id`, with inspect-oldest, inspect-latest, and dequeue-oldest modes.
- Added queue-state helpers for read-only client/run iteration and oldest matching client/run dequeue.
- Added focused `video_frame_queue` tests for read-only oldest inspection, latest inspection, run-filtered dequeue, and no-frame reporting.
- Updated architecture and operations docs to mark the queue read boundary complete and move the next task to switcher/sync integration.

### Changed Files
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept the boundary in-process and diagnostic/manual.
- Kept received/reassembled queue insertion behavior unchanged.
- Did not change protocol wire format.
- Did not add OBS output, 4-view orchestration, targetTime integration, late-frame mutation, H.264 decode, or rendering in this slice.

### Unresolved
- Connecting the server queue read boundary to the next switcher/sync source path.
- Deciding whether the first targetTime integration should inspect or dequeue queued frames.
- production H.264 encoder configuration / error logging policy
- manual two-client bounded real encoded run into the live two-view switcher

### Next
- Connect the queue read boundary to the switcher/sync targetTime source path without adding late-drop mutation.

### TODO Update
- Marked the server queued encoded frame inspect/dequeue boundary as complete.
- Updated the current position to include client/run keyed queue consumption.
- Updated next items toward switcher/sync integration over this read boundary.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-28
### Type
- Codex

### Work
- Recorded the successful manual fragmented real encoded queue PoC results after the server UDP receive buffer tuning was added.
- Documented that both `max_frames=1` and `max_frames=2` fragmented real encoded queue runs succeeded with the recommended manual server receive buffer request.
- Added the latest successful `max_frames=2` observed stdout summaries to the manual checklist, including `fragments_sent=854/854`, `fragments_received=854`, `frames_reassembled=2`, `frames_queued=2`, `incomplete_reassembly_frames=0`, and `receive_timed_out=false`.
- Updated TODO current position and Current Focus to reflect that the fragmented real encoded 1-frame / 2-frame queue path is now manually confirmed.
- Changed the next task from re-running the queue PoC to moving queued encoded frames toward a switcher/sync-facing read boundary.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept this slice docs-only and did not change protocol wire format, client behavior, retransmit/retry policy, 4-view orchestration, or OBS integration.
- Treated the `8388608` requested/effective server UDP receive buffer plus client fragment pacing `16 1` as the current known-good localhost baseline for fragmented real encoded queue verification.
- Kept `frames_attempted=18` and `no_frame_count=16` recorded as capture-cadence diagnostics, not blockers, because the run still captured/encoded/sent 2 frames and the server reassembled/queued both frames.

### Unresolved
- switcher/sync-facing read boundary over queued encoded frames
- production H.264 encoder configuration / error logging policy
- late frame queue mutation / drop policy
- manual two-client bounded real encoded run into the live two-view switcher
- retransmit/retry, 4-view orchestration, and OBS integration

### Next
- Add the next read-only switcher/sync-facing boundary that consumes queued encoded frames without changing protocol or client behavior.
- Keep production H.264 encoder configuration / error logging policy as the next video-path policy task after the queue-to-reader bridge direction is fixed.

### TODO Update
- Updated current position with the successful manual fragmented 1-frame / 2-frame queue verification result.
- Marked the manual fragmented real encoded 1-frame / 2-frame queue path as completed in the Phase 3 checklist.
- Replaced the previous rerun task with the next switcher/sync-facing read-boundary task.

### Validation
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-28
### Type
- Codex

### Work
- Followed up on the manual fragmented real encoded PoC where the client sent all fragments but the server received only part of the frame.
- Recorded the observed result: client `fragments_attempted=411`, `fragments_sent=411`, `send_failures=0`; server `fragments_received=375`, `incomplete_frame_progress=player1/streamsync-dev-session/2:375/411:missing=36`, `frames_reassembled=0`, with no rejected or duplicate fragments.
- Added UDP socket receive buffer tuning to the server `--receive-auth-video-queue-once` manual path.
- Added one optional positional CLI arg, `receive_buffer_bytes`, after the existing manual policy args.
- Defaulted the manual receive buffer request to `8388608` bytes.
- Applied the receive buffer request immediately after socket bind, before auth/video receive on the manual path.
- Added stdout diagnostics for requested receive buffer bytes, effective receive buffer bytes, set error, and read error.
- Kept buffer set/read failures non-fatal for the manual PoC.

### Changed Files
- `apps/server/Cargo.toml`
- `Cargo.lock`
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Used `socket2` for portable receive buffer set/read access.
- Kept CLI compatibility by appending only one optional positional argument.
- Kept protocol wire format, `VideoFrameFragment`, reassembly behavior, and client behavior unchanged.
- Treated receive buffer tuning as manual PoC reliability support, not as production continuous receive-loop design.

### Unresolved
- Manual rerun with `receive_buffer_bytes=8388608`.
- Retransmit/retry.
- Fragment expiration policy.
- 4-view orchestration and OBS integration.

### Next
- Run `cargo run -p stream-sync-server -- --receive-auth-video-queue-once configs/examples/server.example.toml 4096 15000 1 true 8388608`.
- Confirm `manual_receive_buffer_effective_bytes` and then rerun the bounded client with `max_frames=1`.

### TODO Update
- Updated current position and next item to include UDP receive buffer tuning as the next validation focus.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server video_frame -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check -p stream-sync-server`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-28
### Type
- Codex

### Work
- Improved the fragmented real encoded manual PoC after a real run showed the client sent all fragments but the server timed out before completing a frame.
- Raised the server auth/video queue manual launcher defaults from a small fixed receive window to a manual policy tuned for fragmented PoC completion.
- Added CLI-overridable server manual policy values for max post-auth video packets, receive timeout, expected reassembled frames, and stop-after-expected behavior.
- Added a stop condition based on `frames_reassembled >= expected_reassembled_frames`.
- Added incomplete per-frame reassembly progress diagnostics showing received / expected / missing fragment counts.
- Added optional client-side fragment pacing for the bounded authenticated real encoded sender and exposed it on the manual CLI.
- Updated the manual verification checklist with the new recommended commands, defaults, and diagnosis steps.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept protocol wire format unchanged.
- Kept retransmit/retry and fragment expiration out of scope.
- Used bounded manual policy and sender pacing instead of redesigning fragmentation/reassembly.
- Kept the normal low-level send path default pacing-disabled; the bounded manual sender defaults to conservative pacing.

### Unresolved
- Actual rerun of the fragmented real encoded manual PoC with the new settings.
- Fragment retransmit/retry.
- Fragment expiration policy.
- UDP receive buffer sizing remains documented/operational rather than implemented in this slice.
- Production H.264 encoder configuration / rate control.

### Next
- Run the one-client manual queue check with `max_frames=1` or `2`, server policy `4096 15000 1 true`, and client pacing `16 1`.
- If still incomplete, compare `incomplete_frame_progress` against client `fragments_attempted` and then consider OS receive buffer tuning or larger safe fragment payload sizing in a separate slice.

### TODO Update
- Updated current position to include manual receive policy, incomplete frame progress diagnostics, and client fragment pacing.
- Kept the next item as re-running the fragmented sender -> server reassembly -> queue manual verification with the new conservative settings.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-28
### Type
- Codex

### Work
- Audited the manual fragmented real encoded `VideoFrame` verification path.
- Confirmed the bounded client sender had fragment send state internally but did not print successful fragment counters.
- Confirmed the server auth/video queue launcher only exposed queue status and needed manual diagnostics for fragmented receive/reassembly.
- Added bounded client stdout counters for `direct_sends`, `fragmented_sends`, `fragments_attempted`, and `fragments_sent`.
- Extended the server `--receive-auth-video-queue-once` path to receive a bounded sequence of post-auth video packets, apply `VideoFrameFragment` reassembly, and queue the completed frame.
- Added server stdout diagnostics for packets received, fragments received, frames reassembled, frames queued, rejected fragments, duplicate fragments, incomplete reassembly frames, queue length, receive timeout, and max-packet guard.
- Updated the real encoded manual checklist with exact two-command verification, expected client/server output, fragmented pass criteria, and failure diagnosis.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Reused the existing server queue launcher command instead of adding a new command.
- Kept the server receive extension bounded and diagnostic-only; it is not a production continuous receive loop.
- Used an idle receive timeout for the manual launcher to surface incomplete reassembly without implementing fragment expiration.
- Kept retransmit/retry, fragment expiration policy, late-frame queue mutation, 4-view orchestration, and OBS out of scope.

### Unresolved
- actual human run of the fragmented real encoded sender against the server queue launcher
- fragment retransmit/retry
- fragment expiration policy
- production H.264 encoder configuration / rate control
- late frame queue mutation / drop policy

### Next
- Run the documented two-command manual check on Windows with FFmpeg available.
- Record observed stdout if field names or counts differ from the checklist.
- Continue production H.264 encoder configuration / error logging policy after the fragmented path is manually confirmed.

### TODO Update
- Updated current focus to show manual stdout diagnostics are available.
- Kept the next item as executing the documented fragmented sender -> server reassembly -> queue manual verification.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-28
### Type
- Codex

### Work
- Added the smallest server-side `VideoFrameFragment` reassembly slice.
- Routed decoded `VideoFrameFragment` packets through server inbound routing and the existing authenticated packet acceptance gate.
- Added registered handler input for accepted/authenticated video frame fragments without changing auth policy.
- Added caller-owned `ServerVideoFrameReassemblyState` keyed by client id, run id, and frame id.
- Added fragment apply/reassembly results for stored fragment, duplicate ignored, rejected fragment, and completed frame.
- Added metadata consistency checks, duplicate accounting, missing-fragment summary, and chunk-index ordered payload reconstruction.
- Connected completed reassembled frames to the existing `ServerVideoFrameQueueStorageBoundary`.
- Added unit tests for in-order completion, out-of-order completion, duplicate handling, metadata rejection, incomplete missing-fragment state, completed queue insertion, fragment routing, and fragment acceptance.

### Changed Files
- `apps/server/src/lib.rs`
- `apps/switcher/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept packet authentication and endpoint acceptance unchanged by reusing the existing gate for `VideoFrameFragment`.
- Kept reassembly state separate from queue storage and caller-owned.
- Reassembled frames use the fragment-carried metadata and H.264 payload bytes; fragment metadata does not currently carry original `send_timestamp` or keyframe flag.
- Completed frames are queued through the existing storage boundary instead of adding a separate queue path.

### Unresolved
- fragment retry/retransmit and expiration policy
- late frame queue mutation / drop policy
- switcher-specific fragmented frame direct handling
- production H.264 encoder configuration / rate control
- live manual verification of fragmented real encoded sender into server queue

### Next
- Manually verify bounded real encoded fragmented sender -> server reassembly -> queue.
- Continue production H.264 encoder configuration / error logging policy.
- Keep late-drop mutation and 4-view orchestration separate.

### TODO Update
- Marked server-side `VideoFrameFragment` reassembly as complete.
- Updated current focus to show server reassembly / queue insertion is done and fragmented manual verification is next.
- Kept production encoder config, late-drop policy, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server video_frame -- --test-threads=1`
- `cargo test -p stream-sync-server video_frame_queue -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-28
### Type
- Codex

### Work
- Added the smallest sender-side `VideoFrame` UDP fragmentation slice.
- Extended `crates/protocol` with `VideoFrameFragment`, encode/decode support, and `MessageType::VideoFrameFragment`.
- Kept the existing direct `VideoFrame` UDP send path for packets within a conservative safe datagram limit.
- Added client-side fragmentation planning and chunking over encoded H.264 payload bytes without changing capture or encode boundaries.
- Added direct/fragmented send summaries plus fragment-attempt / fragment-sent / failed-fragment-index diagnostics.
- Updated real encoded one-shot and bounded sender flows to preserve the new send summary and failure context.
- Added unit tests for direct send, large-payload fragmentation, fragment metadata preservation, payload reconstruction, fragmented summary reporting, and failed fragment index/error reporting.
- Updated architecture and protocol docs to state that sender-side fragmentation is implemented while server-side reassembly is still pending.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `apps/server/src/lib.rs`
- `crates/net-core/src/lib.rs`
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Used a conservative safe UDP datagram limit instead of targeting the 65 KB maximum.
- Fragmentation is applied after H.264 encode and remains separate from capture/encode logic.
- `VideoFrameFragment` carries explicit frame/chunk metadata and chunk payload bytes; full server-side reassembly is deferred.
- Server auth and packet acceptance behavior remain unchanged; fragmented packets currently decode but are not reassembled into queued frames.

### Unresolved
- server-side `VideoFrameFragment` reassembly
- queue insertion / switcher consumption of reassembled frames
- production H.264 encoder configuration / rate control
- late frame queue mutation / drop policy

### Next
- Add the smallest server-side `VideoFrameFragment` reassembly slice.
- Decide the first queue/runtime handoff for reassembled frames without redesigning the rest of the protocol.
- Re-run the manual real encoded bounded sender against the future reassembly path.

### TODO Update
- Marked payload fragmentation design/implementation complete on the sender side.
- Added server-side fragment reassembly as the next explicit video-path task.
- Updated Current Focus to distinguish sender-side fragmentation complete vs. server-side reassembly pending.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`

---

## 2026-05-06
### Type
- Codex

### Work
- Added named-pipe handoff preview-loop diagnostic logging on both server and
  switcher sides to cut the current `2`-real-slot manual failure.
- Extended server named-pipe handoff request summaries with
  `queue_len_before_read`, `queue_len_after_read`, `selected_client_id`,
  `selected_run_id`, `frame_id`, `frame_payload_len`, and `no_frame_reason`.
- Extended switcher named-pipe handoff runtime/request summaries with
  `handoff_response_kind`, `response_payload_len`, `parse_error`, `io_error`,
  and local encode-stage error detail.
- Extended `--four-view-real-handoff-preview-loop` and
  `--four-view-two-real-handoff-preview-loop` stdout with per-slot
  `slot_diagnostics` carrying:
  - `slot_index`
  - `client_id`
  - `run_id`
  - `request_id`
  - `handoff_response_kind`
  - `parse_error`
  - `io_error`
  - `decode_error`
  - `response_payload_len`
  - `frame_id`
  - `frame_payload_len`
  - `render_input_kind`
  - `final_slot_result_kind`
- Kept the existing protocol shape, named-pipe transport shape, `1`-real-slot
  command, `2`-real-slot command, deterministic fixture commands, and clean
  output window family unchanged.
- Updated manual docs so the next rerun uses the new fields explicitly and
  treats `--four-view-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 5`
  as the player1-only isolation baseline.

### Decisions
- The old server `queue_len` field was not explicit enough for manual
  diagnosis; the meaningful split is `queue_len_before_read` versus
  `queue_len_after_read`.
- Apparent `FrameRead / NoFrame` alternation in the `2`-real-slot preview path
  should now be interpreted with `selected_client_id` / `selected_run_id`
  before assuming queue reinitialization.
- Missing-player investigation should prefer the already validated
  `1`-real-slot command before widening the `2`-real-slot path further.
- No new `MissingClient` result kind was added in this slice; the focus stayed
  on making transport/runtime/parse/decode failure modes visible in stdout.

### Unresolved
- rerun and record the failing `2`-real-slot manual pass with the new server /
  switcher diagnostics
- confirm whether the observed switcher `HandoffError` is:
  - named-pipe transport/runtime failure
  - framed response parse failure
  - downstream decode placeholder behavior
- determine why player2 auth timed out in the reported failing run
- record a fresh `1`-real-slot isolation rerun if needed

### Next
- Rerun the bounded `2`-real-slot manual path with:
  - `configs/manual/server.two-real-slots.toml`
  - `configs/manual/client.player1.toml`
  - `configs/manual/client.player2.toml`
  - `--four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 5`
- If player2 still fails auth or noframes, rerun:
  - `--four-view-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 5`
- Compare:
  - server `queue_len_before_read` / `queue_len_after_read`
  - server `selected_client_id` / `selected_run_id`
  - switcher `slot_diagnostics`

### TODO Update
- Updated `docs/operations/todo.md` to move the next action from generic
  `2`-real-slot manual validation to diagnostic-log-driven rerun and
  player1-only isolation.
- Recorded that `slot_diagnostics` and server before/after queue logging are
  now the primary stdout-based troubleshooting fields.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher four_view -- --test-threads=1`
- `cargo test -p stream-sync-switcher handoff -- --test-threads=1`
- `cargo check --workspace`

---

## 2026-05-06
### Type
- Codex

### Work
- Ran a fresh manual rerun with the new named-pipe handoff diagnostics.
- Verified the `1`-real-slot baseline first using:
  - server:
    `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 5 4096 15000 1 true 8388608`
  - client1:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 5 16 1`
  - switcher:
    `.\target\debug\stream-sync-switcher.exe --four-view-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 5`
- Ran the `2`-real-slot rerun only after the baseline passed using:
  - server:
    `.\target\debug\stream-sync-server.exe --receive-auth-video-queue-and-serve-handoff-many configs/manual/server.two-real-slots.toml streamsync-handoff-dev 10 4096 15000 2 true 8388608`
  - client1:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player1.toml 5 16 1`
  - client2:
    `.\target\debug\stream-sync-client.exe --auth-real-encoded-video-frame-poc-bounded configs/manual/client.player2.toml 5 16 1`
  - switcher:
    `.\target\debug\stream-sync-switcher.exe --four-view-two-real-handoff-preview-loop streamsync-handoff-dev 0 player1 streamsync-dev-session 1 player2 streamsync-dev-session 5`

### Observed Logs
- `1`-real-slot baseline server stdout:
  - `queue_len_before_read=1`
  - `queue_len_after_read=1`
  - `result_kind=FrameRead`
  - `selected_client_id=player1`
  - `selected_run_id=streamsync-dev-session`
  - `frame_id=2`
  - `frame_payload_len=242445`
  - `no_frame_reason=none`
- `1`-real-slot baseline switcher stdout:
  - `scheduler_status=PartialSelected`
  - `clean_output_render_result_kind=Rendered`
  - slot0 `slot_diagnostics`:
    - `client_id=player1`
    - `run_id=streamsync-dev-session`
    - `request_id=5`
    - `handoff_response_kind=FrameRead`
    - `response_payload_len=242542`
    - `parse_error=none`
    - `io_error=none`
    - `decode_error=none`
    - `frame_id=2`
    - `frame_payload_len=242445`
    - `render_input_kind=UseUpdatedFrame`
    - `final_slot_result_kind=Selected`
  - slots1-3 were deterministic placeholders with:
    - `request_id=none`
    - `handoff_response_kind=none`
    - `render_input_kind=UseNoDisplayPlaceholder`
    - `final_slot_result_kind=NoFrameAvailable`
- `2`-real-slot rerun server stdout:
  - receive phase still showed `registered_clients=1`
  - receive phase completed with `frames_reassembled=2` and `frames_queued=2`
  - bounded handoff request order alternated exactly:
    - odd requests:
      - `selected_client_id=player1`
      - `result_kind=FrameRead`
      - `queue_len_before_read=2`
      - `queue_len_after_read=2`
      - `frame_id=3`
      - `frame_payload_len=239261`
    - even requests:
      - `selected_client_id=player2`
      - `result_kind=NoFrame`
      - `queue_len_before_read=0`
      - `queue_len_after_read=0`
      - `frame_id=none`
      - `frame_payload_len=none`
      - `no_frame_reason=NoFramesQueuedForClient`
- `2`-real-slot rerun switcher stdout:
  - `scheduler_status=PartialSelected`
  - `slot_result_kinds=Selected|NoFrameAvailable|NoFrameAvailable|NoFrameAvailable`
  - `clean_output_render_result_kind=Rendered`
  - slot0 `slot_diagnostics`:
    - `client_id=player1`
    - `request_id=9`
    - `handoff_response_kind=FrameRead`
    - `parse_error=none`
    - `io_error=none`
    - `decode_error=none`
    - `frame_id=3`
    - `frame_payload_len=239261`
    - `render_input_kind=UseUpdatedFrame`
    - `final_slot_result_kind=Selected`
  - slot1 `slot_diagnostics`:
    - `client_id=player2`
    - `request_id=10`
    - `handoff_response_kind=NoFrame`
    - `response_payload_len=47`
    - `parse_error=none`
    - `io_error=none`
    - `decode_error=none`
    - `frame_id=none`
    - `frame_payload_len=none`
    - `render_input_kind=UseNoDisplayPlaceholder`
    - `final_slot_result_kind=NoFrameAvailable`
- client2 stderr:
  - `auth real encoded video frame bounded PoC failed: AuthResponse(Receive(ConnectionReset))`

### Decisions
- The `1`-real-slot baseline is confirmed good under the rebuilt diagnostic
  binaries: player1 reaches server queue -> named-pipe handoff -> switcher
  render without parse/io/decode error.
- The `FrameRead / NoFrame` alternation in the current `2`-real-slot rerun is
  confirmed to be request-order-driven:
  - player1 request -> `FrameRead`
  - player2 request -> `NoFrame`
- In the rebuilt diagnostic path, player2 absence is classified as
  `NoFrameAvailable`, not `HandoffError`.
- The current manual `2`-client server stop condition is too weak:
  `expected_reassembled_frames=2` can be satisfied entirely by player1 before
  player2 authenticates or sends anything.

### Root Cause Found
- The current `2`-real-slot manual procedure does not guarantee two distinct
  clients participate in the receive phase.
- Because the server stop condition is only `expected_reassembled_frames=2`,
  player1 alone can satisfy it by queueing two frames.
- After that point, the server leaves receive/auth handling and starts bounded
  named-pipe service.
- client2 then hits connection failure during auth receive:
  - observed as `AuthResponse(Receive(ConnectionReset))`
- switcher therefore sees:
  - player1 scope -> `FrameRead`
  - player2 scope -> `NoFrame`
  - not `HandoffError`

### Unresolved
- a real `2`-client manual validation where both player1 and player2 are
  authenticated and queued before the handoff loop starts
- whether the older reported switcher `HandoffError` came from an older binary,
  a different manual sequence, or a transient transport/runtime timing issue
- whether the next narrow fix should be:
  - manual sequencing only
  - or a server manual stop condition keyed to distinct client scopes

### Next
- Adjust the manual `2`-real-slot procedure so server receive completion cannot
  be satisfied by player1 alone.
- Keep using the `1`-real-slot command as the baseline isolation path before
  any future `2`-real-slot rerun.

### TODO Update
- Updated `todo.md` to record that the current `2`-real-slot manual command
  sequence proves request ordering but not two-client participation.
- Updated the next action to fix the manual gating / sequencing before another
  `2`-real-slot validation pass.

### Validation
- manual rerun with rebuilt `target/debug` binaries
- `git diff --check`

---

## 2026-04-27
### Type
- Codex

### Work
- Improved diagnostics for bounded authenticated real encoded client UDP send failures.
- Added detailed send failure context with destination, local socket address, frame id, encoded payload length, encoded packet length, and underlying send error.
- Preserved OS `send_to` error kind and message for non-size send failures.
- Added explicit `PacketTooLarge` error when the encoded protocol packet exceeds the current UDP datagram limit.
- Added the last send failure details to `ClientContinuousRealEncodedVideoFrameSummary`.
- Extended bounded sender CLI stdout with last send destination/source/frame/payload/packet/error fields.
- Added/updated tests for packet-too-large send failure and bounded summary diagnostics.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept capture/encode/send sequencing unchanged.
- Kept successful send behavior unchanged.
- Classified oversized encoded protocol packets before calling `send_to`, so the manual output is deterministic instead of platform-dependent.

### Unresolved
- packet fragmentation remains unimplemented.
- production encoder configuration is still needed to control H.264 packet size.
- manual E2E rerun is still needed with the new diagnostics.

### Next
- Re-run the bounded sender and inspect `last_send_error`, `last_send_payload_len`, and `last_send_packet_len`.
- If `PacketTooLarge`, reduce encoder output in a future production encoder config task or implement fragmentation.

### TODO Update
- Marked detailed UDP send failure diagnostics complete.
- Kept production H.264 encoder configuration / packet fragmentation as next unresolved work.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`
- `git diff --check` (passed; Git warned that LF will be replaced by CRLF for edited files)

---

## 2026-04-27
### Type
- Codex

### Work
- Reworked `docs/operations/manual-real-encoded-video-poc.md` into a step-by-step human E2E checklist.
- Added prerequisite checks for FFmpeg, `cargo check --workspace`, config file existence, and UDP/firewall setup.
- Added ordered command flows for one-client server queue verification and two-client live switcher verification using the bounded authenticated real encoded sender.
- Documented expected stdout counters for auth acceptance, frames attempted/captured/encoded/sent, no-frame count, accepted/queued source frames, scheduler ticks, and render outcomes.
- Added failure diagnosis for missing config, missing FFmpeg, auth rejection, `NoFrameAvailable`, encode failure, UDP/firewall problems, and decode/render failures.
- Added clear pass/fail criteria for one-client real encoded send and two-client live switcher manual verification.
- No runtime behavior was changed.

### Changed Files
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept bounded authenticated sender as the preferred manual path for the prior no-frame issue.
- Kept one-shot commands documented as lower-level checks, not the primary E2E path.
- Treated `rendered_partial` as an acceptable manual partial pass while `rendered_both` remains the strict two-client pass condition.

### Unresolved
- manual two-client live switcher run still needs to be performed and recorded.
- production H.264 encoder configuration and structured encoder stderr logging.
- OS event-driven frame-arrived wait.
- late frame queue mutation / actual drop policy.
- 4-view orchestration and OBS verification.

### Next
- Run the checklist manually on Windows with FFmpeg available.
- Capture the observed client/switcher stdout and update the manual notes if any field names differ.
- Continue with production encoder configuration / error logging policy.

### TODO Update
- Marked the manual E2E checklist as complete.
- Kept actual manual two-client run as a next item.
- Kept production encoder config, late-drop mutation, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check` (passed; Git warned that LF will be replaced by CRLF for edited docs)

---

## 2026-04-27
### Type
- Codex

### Work
- Added the smallest bounded client-side continuous acquisition / frame-arrived wait path for real encoded video.
- Added `ClientContinuousRealEncodedVideoFrameBoundary`, bounded policy/input/result/summary/stop-reason types, and repeated execution over the existing `ClientRealEncodedVideoFrameOneShotBoundary`.
- The bounded sender consumes a caller-owned ready capture session runtime and caller-owned UDP socket, then repeats acquisition -> FFmpeg H.264 encode hook -> `RealCaptureH264` metadata construction -> existing UDP send.
- Added `ClientAuthRealEncodedVideoFrameBoundedPocLauncher`, which sends `AuthRequest`, requires accepted `AuthResponse`, creates one capture session, and sends multiple `RealCaptureH264` `VideoFrame`s from the same UDP source.
- Added CLI `--auth-real-encoded-video-frame-poc-bounded [config-path] [max-frames]`.
- CLI stdout reports auth result, attempted/captured/encoded/sent counts, no-frame count, capture/encode/frame-build/send failure counts, stop reason, and `bounded_manual_runtime=true`.
- Added tests for max-frame stop, explicit no-frame counting, capture failure stop, encode failure not sending, accepted auth multi-frame same-source send, and rejected auth stopping before capture/encode/send.
- Kept one-shot real encoded sender, placeholder sender, switcher scheduling, 4-view, OBS, and late-frame mutation unchanged.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Reused the existing real encoded one-shot boundary per loop tick instead of moving capture/encode/send logic into the loop.
- Kept auth and same-source socket ownership in the bounded launcher.
- Used bounded max frames, max ticks, frame wait timeout, and optional cadence sleep for manual runtime safety.
- Treated auth rejection as an error that stops before capture session creation, capture, encode, or send.

### Unresolved
- production H.264 encoder configuration and error logging policy
- OS event-driven frame-arrived wait / production continuous acquisition loop
- packet fragmentation for large encoded frames
- late frame queue mutation / actual drop policy
- 4-view orchestration and 2x2 layout
- OBS Window Capture verification
- structured production logging

### Next
- Define production encoder configuration and failure logging.
- Manually run two bounded client senders into the live two-view switcher runtime.
- Define late-frame queue mutation/drop policy separately from read-only selection.

### TODO Update
- Marked bounded continuous real encoded client sender / frame-arrived wait slice complete.
- Moved next priority to production H.264 encoder configuration / error logging policy.
- Kept late-drop mutation, 4-view, OBS, and production logging deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`
- `git diff --check` (passed; Git warned that LF will be replaced by CRLF for edited files)

---

## 2026-04-27
### Type
- Codex

### Work
- Added the smallest bounded live two-view switcher manual runtime.
- Added `SwitcherLiveTwoViewManualRuntimeConfig`, `SwitcherLiveTwoViewManualRuntimeBoundary`, auth summary/result/error types, and runtime wiring from server auth setup to UDP source to continuous two-view scheduler.
- The runtime binds or accepts one UDP socket, runs the existing `ServerAuthResponsePocStep` for bounded auth setup, keeps the resulting caller-owned `AuthenticatedSenderRegistry`, passes it to `SwitcherUdpLiveTwoViewQueueSource`, and runs `SwitcherContinuousTwoViewSchedulingBoundary`.
- Added switcher CLI `--live-two-view-switcher-once [config-path] [left-client-id] [right-client-id]`.
- CLI stdout reports bind/client ids, auth processed/accepted/rejected/registered counts, packet and queue counts, tick/render outcome counts, stop reason, and `bounded_manual_runtime=true`.
- Added tests for accepted auth/video reaching scheduler summary, rejected auth plus unauthenticated video remaining explicit, and source-end stop reason surfacing.
- Kept UDP source adapter, continuous scheduler, selection/decode/render, late-drop mutation, 4-view orchestration, and OBS API integration separate.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-placeholder-video-poc.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Reused `ServerAuthResponsePocStep` for auth setup so the launcher does not implement a new auth policy.
- Kept auth registry ownership inside the manual runtime and passed the finished registry into the existing UDP source adapter.
- Kept the launcher bounded by auth packet count, UDP source packet count, receive timeout, max ticks, and max rendered frames.
- Added the runnable CLI now instead of another placeholder-only boundary.

### Unresolved
- continuous client acquisition / frame-arrived wait
- production H.264 encoder configuration and error logging policy
- late frame queue mutation / actual drop policy
- 4-view orchestration and 2x2 layout
- OBS Window Capture verification
- structured production logging

### Next
- Add continuous acquisition / frame-arrived wait on the client side.
- Define production encoder configuration and failure logging.
- Define late-frame queue mutation/drop policy separately from the read-only selector.

### TODO Update
- Marked live two-view switcher manual runtime complete.
- Moved next priority to continuous acquisition / frame-arrived wait.
- Kept production encoder config, late-drop mutation, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher live_two_view_manual_runtime -- --test-threads=1`
- `cargo test -p stream-sync-switcher -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-27
### Type
- Codex

### Work
- Added the smallest real UDP socket-backed source adapter for switcher two-view scheduling.
- Added `SwitcherUdpLiveTwoViewSourceConfig`, `SwitcherUdpLiveTwoViewQueueSource`, and bind/config error types.
- Extended `SwitcherLiveTwoViewQueueSourceItem` and queue summary accounting so protocol decode failure, socket receive failure, and non-video packets remain explicit alongside accepted video, rejected video, timeout, and source end.
- The UDP adapter binds or wraps a caller-owned UDP socket, applies bounded max-packet / read-timeout behavior, reuses `ServerReceiveLoopStep` and the server packet acceptance gate, then maps accepted authenticated `VideoFrame` packets into the existing live queue source interface.
- The adapter requires a caller-owned `AuthenticatedSenderRegistry`; it does not create authenticated entries or fake authenticated frames.
- Added UDP-backed tests for accepted `VideoFrame`, unauthenticated rejection, protocol decode failure, timeout/no packet, and scheduler consumption through the existing source trait.
- Kept auth registry creation launcher, late-frame queue mutation/drop, 4-view orchestration, and OBS-specific API integration out of scope.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Reuse server receive/decode/gate boundaries instead of adding switcher-specific auth or protocol parsing.
- Keep auth registry population outside the UDP source adapter.
- Treat allowed clients as the configured left/right client ids; authenticated frames from other clients are rejected as an explicit source item.
- Keep scheduler and live runtime unchanged; the adapter only implements `SwitcherLiveTwoViewQueueSource`.

### Unresolved
- auth registry generation / live launcher wiring for a complete manual runtime
- late frame queue mutation / actual drop policy
- 4-view orchestration and 2x2 layout
- OBS Window Capture verification
- production timing/decode/render policy and structured logging

### Next
- Add a live switcher launcher/manual runtime that creates or receives the authenticated sender registry and wires it to the UDP source adapter.
- Define late-frame queue mutation/drop policy separately.
- Extend to 4-view orchestration after live 2-client source ownership is stable.

### TODO Update
- Marked real UDP socket-backed source adapter complete.
- Moved the next switcher sync task to auth registry generation / live launcher wiring.
- Kept late-drop mutation, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher udp_live_two_view_source -- --test-threads=1`
- `cargo test -p stream-sync-switcher -- --test-threads=1`
- `cargo check --workspace`
- `git diff --check`
- `cargo test -p stream-sync-server video_frame_queue` was not run because no shared/server code was changed.

---

## 2026-04-27
### Type
- Codex

### Work
- Added the smallest bounded continuous 2-view scheduling boundary over the existing live-like one-pass runtime.
- Added `SwitcherContinuousTwoViewSchedulingBoundary`, scheduling policy/input/result/tick/outcome/summary types, and stop reasons.
- The scheduler repeatedly invokes `SwitcherLiveTwoViewRuntimeBoundary` by logical tick, advances `current_switcher_time` using a caller-owned tick interval, and preserves the full per-tick live runtime result.
- Scheduler-level outcomes now distinguish rendered-both, rendered-partial, no frames, decode failed, render not completed, source ended, max ticks, and max rendered frames.
- Added deterministic tests for multiple ticks over a scripted live source, max-rendered-frame guard stop, partial/no-frame accounting, explicit source end, and preserving one-pass runtime detail when one side decode fails.
- Kept real UDP socket-backed source ownership, late-frame queue mutation/drop, 4-view orchestration, and OBS-specific API integration out of scope.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Implement scheduling as a thin loop over `SwitcherLiveTwoViewRuntimeBoundary` instead of moving queue, selection, decode, composition, or render logic into the scheduler.
- Use logical cadence only; tests do not sleep or require a real window/backend.
- Preserve per-tick runtime output so scheduler summaries do not replace detailed queue and per-side pipeline status.
- Stop deterministically by max ticks, max rendered frames, or source end.

### Unresolved
- real UDP socket-backed `SwitcherLiveTwoViewQueueSource`
- late frame queue mutation / actual drop policy
- 4-view orchestration and 2x2 layout
- OBS Window Capture verification
- production timing/decode/render policy and structured logging

### Next
- Add a real UDP socket-backed source adapter for `SwitcherLiveTwoViewQueueSource`.
- Define late-frame queue mutation/drop policy separately.
- Extend from 2-view scheduler to 4-view orchestration after live source ownership is stable.

### TODO Update
- Marked bounded continuous 2-view scheduling complete.
- Moved the next switcher sync task to real UDP socket-backed live source ownership.
- Kept late-drop mutation, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`

---

## 2026-04-27
### Type
- Codex

### Work
- Added the smallest live-like 2-client switcher queue/runtime integration boundary.
- Added `SwitcherLiveTwoViewRuntimeBoundary`, `SwitcherLiveTwoViewQueueSource`, source item/result/status types, queue summary, and pipeline result types.
- The boundary consumes a caller-owned live queue source, stores accepted video frames into a fresh `ServerVideoFrameQueueState`, and then runs one existing pipeline pass: 2-view targetTime selection -> H.264 decode -> 2-view composition -> composed-canvas render.
- Rejected frames are counted and are not queued.
- Runtime guard / end-of-input states are explicit in the queue summary.
- Per-side pipeline status preserves selection unavailable, decoded, decode deferred, and decode failed states.
- Added deterministic tests for accepted two-client frames reaching queue state, partial/missing client behavior, rejected unauthenticated frame not queued, max-packet guard stop, and per-side decode failure with partial render.
- Kept real socket loop ownership, continuous 2-view scheduling, queue mutation / late drop, 4-view orchestration, and OBS-specific API integration out of scope.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Use a caller-owned source trait for the first live integration boundary rather than adding real socket receive ownership to switcher.
- Store only accepted video frames through the existing server queue storage boundary.
- Run the existing selection/decode/composition/render boundaries once after bounded ingestion.
- Keep late-frame queue mutation deferred; selection remains read-only.

### Unresolved
- real socket-backed 2-client source implementation
- continuous 2-view scheduling
- queue mutation / actual late-frame drop policy
- 4-view orchestration and 2x2 layout
- OBS Window Capture verification
- production timing/decode/render policy and structured logging

### Next
- Add continuous 2-view scheduling over the live queue/source boundary.
- Add a real socket-backed source after scheduling and ownership contracts are fixed.
- Extend to 4-view orchestration after 2-view scheduling is stable.

### TODO Update
- Marked live-like 2-client queue/runtime integration complete.
- Updated the next switcher sync task to continuous 2-view scheduling.
- Kept real socket loop ownership, queue mutation, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`

---

## 2026-04-26
### Type
- Codex

### Work
- Added the smallest composed 2-view canvas window render connection.
- Added `SwitcherTwoViewComposedFrameRenderInput`, `SwitcherTwoViewComposedFrameRenderInputError`, `SwitcherTwoViewComposedCanvasRenderResult`, and `SwitcherTwoViewComposedCanvasRenderBoundary`.
- The boundary validates `SwitcherTwoViewComposedFrame`, converts it to the existing decoded-frame window render input, and reuses caller-owned `SwitcherWindowRenderRuntimeHook`.
- Added switcher CLI `--render-two-view-composed-fixture-once [hold-ms]`.
- The fixture CLI composes two decoded BGRA fixture frames, then renders the composed canvas once through the platform render hook.
- Added deterministic tests for composed-frame render input conversion, invalid dimensions, backend unavailable, render hook payload/dimension handoff, and render/composition separation.
- Kept live 2-client socket receive integration, continuous scheduling, queue mutation / late drop, 4-view orchestration, and OBS-specific API integration out of scope.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Reuse the existing one-frame window render hook instead of adding another renderer.
- Treat composed canvas rendering as a separate boundary after composition.
- Non-Windows remains an explicit backend-unavailable result through the existing unavailable render hook.

### Unresolved
- live 2-client receive/socket integration
- continuous 2-view scheduling
- queue mutation / actual late-frame drop policy
- 4-view orchestration and 2x2 layout
- OBS Window Capture verification
- production timing/decode/render policy and structured logging

### Next
- Add live 2-client queue/runtime integration.
- Add continuous scheduling after live 2-view source ownership is isolated.
- Extend to 4-view orchestration after the 2-view live path is stable.

### TODO Update
- Marked composed 2-view canvas window render connection complete.
- Kept the next switcher sync task on live 2-client socket receive integration.
- Kept continuous scheduling, queue mutation, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher`
- `cargo check -p stream-sync-switcher`

---

## 2026-04-26
### Type
- Codex

### Work
- Added the smallest 2-view layout/composition boundary for switcher.
- Added `SwitcherTwoViewLayoutSideInput`, `SwitcherTwoViewLayoutPolicy`, `SwitcherTwoViewCompositionInput`, `SwitcherTwoViewComposedFrame`, `SwitcherTwoViewCompositionResult`, and `SwitcherTwoViewCompositionBoundary`.
- The boundary composes decoded BGRA left/right sides into one side-by-side BGRA canvas and keeps left-only, right-only, empty placeholder, and invalid-dimensions states explicit.
- Added `SwitcherTwoViewCompositionInput::from_decode_render_result` so targetTime-selected decode/render output can feed composition without coupling composition to selection or H.264 decode.
- Extended `SwitcherTwoViewRenderedSide` to carry the decoded BGRA frame forward for downstream layout/composition.
- Added deterministic tests for both-side composition, left-only, right-only, both-missing placeholder, invalid dimensions, and selected-frame metadata preservation.
- Kept live socket receive integration, queue mutation / late drop, continuous scheduling, 4-view orchestration, window rendering of the composed canvas, and OBS integration out of scope.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Use side-by-side horizontal layout for the first 2-view canvas.
- Partial composition produces a real canvas with an explicit placeholder-colored region for the missing side.
- Both-missing remains an explicit empty placeholder result instead of creating a fake frame.
- Composition consumes decoded BGRA frames only; it does not select frames, decode H.264, render windows, or own queues.

### Unresolved
- live 2-client receive/socket integration
- composed-canvas window render path
- queue mutation / actual late-frame drop policy
- 4-view orchestration and 2x2 layout
- OBS Window Capture verification
- production timing/decode/render policy and structured logging

### Next
- Add live 2-client queue/runtime integration after fixture and layout boundaries are stable.
- Add a render path for composed 2-view canvas.
- Extend composition to 4-view after 2-view live path is isolated.

### TODO Update
- Marked 2-view layout/composition complete.
- Updated the next switcher sync task to live 2-client socket receive integration.
- Kept continuous scheduling, queue mutation, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher`

---

## 2026-04-26
### Type
- Codex

### Work
- Added the smallest 2-view sync runtime/manual verification path.
- Added `SwitcherTwoViewManualVerificationBoundary`, input/result/summary types, and compact per-side selection/decode-render status enums.
- The boundary reads caller-owned `ServerVideoFrameQueueState`, runs `SwitcherTwoViewTargetTimeSelectionBoundary`, then runs `SwitcherTwoViewDecodeRenderBoundary` with caller-owned decode/render hooks.
- Added switcher CLI `--two-view-sync-fixture-once [left-client-id] [right-client-id] [hold-ms]`.
- The fixture CLI builds a deterministic two-client queue, runs one selection -> decode/render verification, and prints targetTime plus per-side selection/decode-render status, frame id, payload length, dimensions, and adjusted capture timestamp.
- Kept live two-client networking, queue mutation / late drop, continuous scheduling, 2-view layout/composition, 4-view orchestration, and OBS integration out of scope.
- Added deterministic tests for both sides selected/rendered, one side missing, decode failure, render failure, offset-influenced selection, and metadata/status preservation through the manual/runtime boundary.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Use a fixture CLI now because live two-client networking and queue sharing are broader than this slice.
- The fixture CLI uses the real FFmpeg/window hooks; invalid fixture payloads are reported as explicit decode/render states rather than fake rendered frames.
- Keep the reusable runtime boundary hook-based so tests can use mock decode/render hooks.

### Unresolved
- 2-view layout/composition
- live two-client receive/socket integration
- queue mutation / actual late-frame drop policy
- 4-view orchestration
- OBS Window Capture verification
- production timing/decode/render policy and structured logging

### Next
- Define 2-view layout/composition over selected/rendered sides.
- Add live two-client queue/runtime integration after fixture verification is stable.
- Add 4-view orchestration after the 2-view layout boundary is isolated.

### TODO Update
- Marked 2-view sync PoC runtime/manual verification complete.
- Updated the next switcher sync task to 2-view layout/composition.
- Kept live networking, continuous scheduling, queue mutation, 4-view, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-26
### Type
- Codex

### Work
- Added the smallest 2-view targetTime-selected decode/render connection boundary.
- Added `SwitcherTwoViewDecodeRenderInput`, `SwitcherTwoViewDecodeRenderBoundary`, `SwitcherTwoViewDecodeRenderResult`, `SwitcherTwoViewRenderedSide`, `SwitcherTwoViewSkippedSide`, and `SwitcherTwoViewSide`.
- The boundary consumes `SwitcherTwoViewTargetTimeSelectionResult`, decodes only selected sides through `SwitcherH264DecodeBoundary`, and renders decoded BGRA frames through `SwitcherWindowRenderBoundary`.
- Result variants distinguish both-rendered, left-rendered/right-skipped, right-rendered/left-skipped, and both-skipped outcomes.
- Per-side skipped results preserve selection unavailable, decode deferred, decode failed, render deferred, backend unavailable, invalid frame, and render failed states explicitly.
- Added deterministic mock-hook tests for both rendered, partial selection, decode failure, render failure, both unavailable, queue non-mutation, and decode-input metadata/payload preservation.
- Kept selection, decode, render, queue ownership, layout/composition, 4-view orchestration, and OBS integration separate.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Partial selection does not synthesize frames. Only `Selected` sides are decoded/rendered.
- Decode/render runtime hooks remain caller-owned so tests can run without FFmpeg or a real window.
- The boundary can render two selected sides as two one-frame render requests, but it does not define a 2-view layout or continuous scheduling.
- Queue mutation and late-frame dropping remain owned by a future queue/runtime boundary.

### Unresolved
- 2-view sync PoC runtime/manual verification using live or fixture queue state
- queue mutation / actual late-frame drop policy
- 4-view orchestration
- live receive/socket integration
- OBS Window Capture verification
- production timing/decode/render policy and structured logging

### Next
- Add a 2-view runtime/manual verification path that runs selection -> decode/render over caller-owned queue state.
- Define queue-owner late-drop policy separately.
- Add 4-view orchestration after the 2-view runtime/manual path is stable.

### TODO Update
- Marked targetTime-selected frame -> decode/render connection complete.
- Updated the next switcher sync/display task to 2-view runtime/manual verification.
- Kept 4-view, OBS, continuous scheduling, and queue mutation deferred.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest 2-view targetTime selection orchestration boundary for switcher.
- Added `SwitcherTwoViewTargetTimeSelectionPolicy`, `SwitcherTwoViewTargetTimeSelectionInput`, `SwitcherTwoViewTargetTimeSelectionResult`, and `SwitcherTwoViewTargetTimeSelectionBoundary`.
- Added `SwitcherJitterBufferSelectionBoundary::select_frame_at_target_time` so callers can reuse one-client jitter-buffer selection against an already-calculated shared targetTime.
- The 2-view selector calculates one shared targetTime, applies left/right clock offset estimates independently during per-client timestamp comparison, and returns both-selected / partial / both-unavailable outcomes explicitly.
- Kept queue ownership caller-side and read-only; the new boundary does not mutate queues, drop late frames, decode, render, compose 4-view, or integrate OBS.
- Added deterministic tests for both-selected, one-side waiting, one-side too early, one-side too late, per-client offset behavior, both-unavailable, and metadata/payload preservation.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The shared targetTime is calculated once for the pair. Per-client offsets adjust capture timestamps, not the pair's shared targetTime.
- Partial selection preserves each side's full one-client selection status instead of collapsing reasons into a lossy summary.
- Late frames remain reported as drop candidates only; actual queue mutation stays with a future queue owner.
- Decode/render connection remains a separate downstream boundary.

### Unresolved
- targetTime-selected frame -> decode/render connection
- queue mutation / actual late-frame drop policy
- 4-view orchestration
- live receive/socket integration
- OBS Window Capture verification
- production timing policy and structured selection/drop logging

### Next
- Connect selected encoded frames from targetTime selection into decode/render through a separate adapter.
- Define 4-view orchestration after 2-view selected-frame decode/render is isolated.
- Add queue-owner late-drop policy and structured timing logs.

### TODO Update
- Marked 2-view targetTime selection orchestration complete.
- Added targetTime-selected frame -> decode/render connection as the next switcher sync/display boundary.
- Kept 4-view, OBS, and queue mutation deferred.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest targetTime / jitter-buffer selection boundary for one switcher client.
- Added `SwitcherTargetTimeBoundary`, `SwitcherTargetTimeInput`, `SwitcherTargetTime`, `SwitcherJitterBufferSelectionPolicy`, `SwitcherJitterBufferSelectionInput`, `SwitcherJitterBufferSelectedFrame`, and `SwitcherJitterBufferSelectionResult`.
- The selector reads one client's frames from caller-owned `ServerVideoFrameQueueState` without mutation.
- Selection calculates targetTime from current switcher time, playout delay, and optional clock offset, then chooses the encoded frame closest to targetTime inside the configured early/late window.
- Explicit outcomes cover selected frame, no frame, waiting for buffer, frame too early, and frame too late/drop candidates.
- Added deterministic tests for closest-frame selection, insufficient buffer, no-frame, too-early, too-late/drop candidates, and metadata/payload preservation.
- Kept decode, render, continuous loop, multi-view sync orchestration, and OBS integration separate.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the first targetTime selector pure and read-only.
- Report late frames as drop candidates instead of mutating the server queue from the selector.
- Keep selected output encoded so future decode/render loop integration can remain a downstream step.
- Leave 2-view / 4-view orchestration for the next boundary.

### Unresolved
- 2-view / 4-view targetTime orchestration
- targetTime-selected decode/render loop integration
- live queue ownership / socket receive loop integration
- OBS Window Capture verification
- production timing policy and structured selection/drop logging

### Next
- Define 2-view targetTime selection orchestration.
- Connect targetTime-selected frames into decode/render after the selection contract is stable.
- Add production timing policy and structured drop/wait logging.

### TODO Update
- Marked targetTime / jitter-buffer frame selection complete.
- Updated Current Focus from targetTime selection to 2-view sync orchestration.
- Kept decode/render and OBS work separate from this selector.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest bounded continuous switcher decode/render loop boundary.
- Added `SwitcherContinuousRenderLoopPolicy`, `SwitcherContinuousRenderLoopInput`, `SwitcherContinuousFrameSource`, `SwitcherQueueLatestFrameSource`, loop events, loop summary, stop reasons, and `SwitcherContinuousRenderLoopBoundary`.
- The loop repeatedly performs latest-frame selection, H.264 decode through a caller-owned decode hook, and decoded-frame render through a caller-owned render hook.
- The loop records rendered frames, no-frame iterations, decode deferred/failed states, and render-not-completed states explicitly.
- The loop stops deterministically by `max_iterations` or `max_rendered_frames`.
- Added deterministic tests using scripted frame sources and mock decode/render hooks, without requiring a real window backend.
- Preserved one-shot decode, BMP dump, and one-shot render paths.

### Changed Files
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Keep the continuous loop single-client and latest-frame based only.
- Keep queue/source, decode, and render responsibilities separate through caller-owned traits/hooks.
- Do not add sleep/cadence timing, socket ownership, queue mutation, targetTime, jitter-buffer selection, 2-view/4-view layout, or OBS-specific behavior in this step.

### Unresolved
- targetTime / jitter-buffer frame selection
- 2-view / 4-view sync and layout
- live queue/runtime ownership beyond caller-owned source hooks
- OBS Window Capture verification
- production decode/render configuration and structured logging

### Next
- Define targetTime / jitter-buffer selection boundary.
- Define live receive/queue ownership around the bounded loop.
- Add production decode/render configuration and structured failure logging.

### TODO Update
- Marked switcher single-client bounded continuous decode/render loop boundary complete.
- Updated Current Focus from continuous rendering to targetTime / jitter-buffer selection.
- Kept OBS integration and multi-view sync deferred.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest switcher window rendering boundary for one decoded BGRA frame.
- Added `SwitcherDecodedFrameRenderInput`, render input validation errors, `SwitcherWindowRenderBoundary`, render runtime hook types, and explicit render result states.
- Added `SwitcherUnavailableWindowRenderRuntimeHook` for explicit backend-unavailable behavior.
- Added Windows-only `SwitcherWindowsGdiWindowRenderRuntimeHook`, which opens a normal window, paints one BGRA frame through GDI, keeps it visible for a bounded hold duration, and closes it.
- Added switcher CLI entry point `--receive-auth-video-render-decoded-once [config-path] [client-id] [hold-ms]`.
- Kept H.264 decode and BMP dump separate and unchanged; rendering consumes `SwitcherDecodedFrame` after decode.
- Added tests for render input validation, invalid frame result, unavailable backend result, caller-owned render success, and BMP dump separation.

### Changed Files
- `apps/switcher/Cargo.toml`
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Use the already-present `windows` crate for the first Windows renderer instead of adding a new windowing dependency.
- Keep the renderer one-shot and bounded by `hold-ms`; no continuous display loop or frame scheduling is introduced.
- Treat the normal switcher window as the future OBS Window Capture target, without adding OBS-specific API integration.
- Keep non-Windows behavior explicit as backend unavailable.

### Unresolved
- continuous receive/decode/render loop
- targetTime / jitter-buffer frame selection
- 2-view / 4-view sync and layout
- OBS Window Capture operational verification
- production decode/render configuration and structured logging

### Next
- Define switcher continuous receive/decode/render loop boundary.
- Define targetTime / jitter-buffer selection after one-shot render is stable.
- Add production decode/render configuration and structured failure logging.

### TODO Update
- Marked switcher decoded frame one-shot window rendering boundary complete.
- Added switcher continuous decoded frame window display as the remaining display task.
- Updated Current Focus and Next Items from one-shot rendering to continuous rendering/sync paths.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the first switcher-side H.264 decode boundary for one latest queued `VideoFrame`.
- Added `SwitcherH264DecodeBoundary`, `SwitcherDecodedFrame`, `SwitcherH264DecodeResult`, decode runtime hook types, and `SwitcherFfmpegH264DecodeRuntimeHook`.
- The FFmpeg decode runtime reads Annex B H.264 from stdin and emits one BGRA rawvideo frame on stdout.
- Extended `SwitcherSingleViewPlaceholderDisplayBoundary` with a decode-attempt path: decode success returns a real-frame handoff, while decode deferred/failed falls back to the existing placeholder handoff with an explicit decode status.
- Added `SwitcherDecodedFrameDumpBoundary` for writing one decoded BGRA frame as a 32-bit BMP file.
- Added `SwitcherDecodeLatestFrameOnceBoundary` for latest-frame selection -> decode -> BMP dump.
- Added switcher CLI entries:
  - `--decode-latest-frame-once [client-id] [output-path]`
  - `--receive-auth-video-decode-latest-once [config-path] [client-id] [output-path]`
- Added tests for decode success via hook, empty payload deferred, decode failure, BMP dump on decoded frame, and placeholder fallback.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Use FFmpeg CLI first for switcher H.264 decode, matching the current client-side FFmpeg encode direction.
- Use BMP file output as the first display substitute instead of adding GUI/window dependencies in this step.
- Keep the existing placeholder path intact and use it as fallback when decode is deferred or failed.
- Add an in-process receive/auth/video/decode CLI because the live server queue is still caller-owned and not shared cross-process.

### Unresolved
- real switcher window rendering from `SwitcherDecodedFrame`
- continuous receive/decode/display loop
- targetTime / jitter-buffer frame selection
- 2-view / 4-view sync
- OBS integration
- production decode configuration and structured decode logging

### Next
- Define decoded frame -> switcher window rendering boundary.
- Define continuous acquisition / receive / decode display loops separately from this one-shot path.
- Add targetTime / jitter-buffer selection after one-frame decode is stable.

### TODO Update
- Marked switcher real H.264 decode / single-frame BMP dump complete.
- Added decoded frame window display as the next switcher display task.
- Updated Current Focus and Next Items to move from decode to rendering/continuous paths.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-switcher`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Decided the auth + real encoded same-source launcher is needed now for manual server queue E2E verification.
- Added `ClientAuthRealEncodedVideoFramePocLauncher`, startup config, outcome, and error types.
- Added the client CLI entry point `--auth-real-encoded-video-frame-poc-once [config-path]`.
- Refactored `ClientRealEncodedVideoFramePocLauncher` so the existing real encoded one-shot path can send through a caller-provided UDP socket.
- The new launcher binds one UDP socket, sends `AuthRequest`, receives `AuthResponse`, requires `accepted=true`, then reuses `ClientRealEncodedVideoFrameOneShotBoundary` to capture, FFmpeg-encode, build `RealCaptureH264`, and send one `VideoFrame` from the same source.
- Added tests for config wiring, accepted auth reaching real encoded send from the same source, rejected auth stopping before capture/encode/send, capture unavailable, and encode failure.
- Updated manual real encoded PoC docs with the authenticated same-source command pair.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Add `--auth-real-encoded-video-frame-poc-once` because the server packet acceptance gate is keyed by the authenticated UDP source; the existing video-only real encoded CLI cannot prove accepted queue insertion.
- Do not weaken server authentication or bypass the packet acceptance gate.
- Keep the video-only real encoded CLI as the low-level capture/encode/send check.
- Keep continuous capture, decode/rendering, OBS integration, and 4-view sync out of this task.

### Unresolved
- production H.264 encoder configuration and structured error logging
- continuous acquisition / frame-arrived wait
- real target enumeration
- real H.264 decode and switcher rendering
- targetTime / jitter-buffer 2-view and 4-view sync
- OBS integration

### Next
- Add production encoder configuration / structured encode error logging.
- Define the real H.264 decode / switcher rendering boundary.
- Define continuous acquisition / frame-arrived wait separately from one-shot send.

### TODO Update
- Marked same-socket auth then real encoded `VideoFrame` one-shot CLI/config launcher complete.
- Updated Current Focus with `--auth-real-encoded-video-frame-poc-once`.
- Removed the auth + real encoded launcher decision from Next Items.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame -- --nocapture`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added manual verification wiring for the one-shot real encoded client `VideoFrame` path.
- Added `ClientRealEncodedVideoFramePocLauncher`, startup config, outcome, and error types.
- Added the client CLI entry point `--real-encoded-video-frame-poc-once [config-path]`.
- The CLI uses the existing client config for destination/client/run/protocol metadata, targets Windows Graphics Capture primary display, uses FFmpeg software H.264 encode, and sends one `RealCaptureH264` `VideoFrame`.
- Success output includes frame id, capture timestamp, dimensions, encoded payload length, destination, and `source_kind=RealCaptureH264`.
- Failure output remains explicit for session config/session creation, capture unavailable/no frame, encode unavailable/failed, frame build failure, and send failure.
- Added manual docs for the real encoded one-shot PoC and linked them from the placeholder manual PoC note.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/manual-placeholder-video-poc.md`
- `docs/operations/manual-real-encoded-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Add CLI wiring now because it can reuse existing config parsing and the one-shot boundary without changing placeholder behavior.
- Keep the initial real encoded manual target fixed to Windows Graphics Capture primary display.
- Do not authenticate in this launcher yet; it verifies client-side real capture/encode/metadata/send, not server queue insertion.
- Keep same-source auth + real video as a later decision.

### Unresolved
- auth + real encoded video same-source launcher
- production H.264 encoder configuration and error logging policy
- continuous acquisition / frame arrived wait
- real target enumeration
- UDP send loop using real encoded frames
- real H.264 decode, switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS integration

### Next
- Decide whether to add an auth + real encoded video same-source launcher.
- Add production encoder configuration and structured encode/send failure logging.
- Add real H.264 decode / switcher rendering boundary.

### TODO Update
- Marked manual CLI/doc path for one-shot real encoded `VideoFrame` send complete in Phase 3.
- Updated Current Focus with `--real-encoded-video-frame-poc-once`.
- Updated Next Items to put auth + real encoded video same-source launcher decision next.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest one-shot client path for sending a real encoded `VideoFrame`.
- Implemented `ClientRealEncodedVideoFrameOneShotBoundary`.
- The boundary composes a caller-owned ready `ClientCaptureSessionRuntime`, one BGRA frame acquisition hook, one H.264 encoder hook, existing encoded-source metadata construction, and existing UDP `VideoFrame` send.
- Kept stopped states explicit: capture unavailable, no frame available, encode unavailable/failed, frame build failure, and send failure.
- Kept placeholder H.264 send path unchanged and did not add continuous acquisition, switcher decode/rendering, OBS integration, or 4-view sync.
- Did not add CLI wiring in this step; the new path is a tested library boundary over caller-owned runtime/socket objects.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The one-shot real encoded path starts from a ready capture session runtime. Session creation and target discovery remain separate.
- Existing `ClientCaptureFrameAcquisitionBoundary`, `ClientH264EncoderBoundary`, `ClientVideoFrameMetadataConstructionBoundary`, and `ClientVideoFrameEncodeSendBoundary` are reused instead of duplicating send or metadata logic.
- `RealCaptureH264` is still produced only by the encoder boundary from non-empty H.264 payload bytes.
- CLI wiring is deferred until a manual verification flow is worth exposing.

### Unresolved
- production H.264 encoder configuration and error logging policy
- manual real encoded one-shot verification wiring / optional CLI
- continuous acquisition / frame arrived wait
- real target enumeration
- UDP send loop using real encoded frames
- real H.264 decode, switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS integration

### Next
- Add production encoder configuration and structured encode/send failure logging.
- Decide whether to add a manual CLI for the real encoded one-shot path.
- Add real H.264 decode / switcher rendering boundary.

### TODO Update
- Marked the one-shot real encoded `VideoFrame` path complete in Phase 3.
- Updated Current Focus with `ClientRealEncodedVideoFrameOneShotBoundary`.
- Updated Next Items to put production encoder configuration / logging and optional manual verification next.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the first minimal real client H.264 software encoder runtime hook.
- Implemented `ClientFfmpegSoftwareH264EncoderRuntimeHook` behind the existing `ClientH264EncoderRuntimeHook` contract.
- The hook invokes a caller-configured `ffmpeg` executable, feeds one BGRA rawvideo frame through stdin, and reads one Annex B H.264 elementary stream from stdout.
- Kept `ClientH264EncoderBoundary` responsible for converting only non-empty hook output into `RealCaptureH264`.
- Mapped missing `ffmpeg` to `EncoderUnavailable`; invalid dimensions, invalid BGRA buffer length, FFmpeg/libx264 failure, and empty output to `EncodeFailed`.
- Kept placeholder H.264 source behavior unchanged and did not change UDP send, switcher decode/rendering, OBS integration, continuous acquisition, or sync.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Use FFmpeg CLI first instead of adding a Rust FFmpeg binding dependency in this step.
- Default FFmpeg settings are `libx264`, `ultrafast`, `zerolatency`, and `yuv420p`.
- The expected encoded output is an H.264 Annex B elementary stream from `ffmpeg -f h264`.
- Hardware encoder support remains deferred behind the same hook boundary.

### Unresolved
- production encoder configuration and error logging policy
- real encoded-frame one-shot client path
- UDP send path using real encoded frames
- continuous acquisition / frame arrived wait
- real target enumeration
- real H.264 decode, switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS integration

### Next
- Connect `RealCaptureH264` encoded sources to an explicit one-shot client path without changing placeholder send semantics.
- Add production encoder configuration and structured encode failure logging.
- Add real H.264 decode / switcher rendering boundary.

### TODO Update
- Marked the minimal FFmpeg CLI software H.264 encoder runtime hook complete in Phase 3.
- Updated Current Focus with the FFmpeg software encoder hook and Annex B H.264 output format.
- Updated Next Items to make the real encoded one-shot client path the next video task.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest H.264 encoder boundary shape that consumes `ClientRawCapturedVideoFrame`.
- Added `ClientH264EncoderInput::from_raw_frame`, `ClientH264EncodedPayload`, `ClientH264EncoderHookResult`, and `ClientH264EncoderRuntimeHook`.
- Kept the default encoder behavior explicit as `RealH264EncodeDeferred`.
- Added `encode_once_with_runtime` so a caller-owned FFmpeg or hardware encoder can provide real H.264 bytes later.
- The boundary converts successful non-empty hook output into `ClientEncodedVideoFrameSource` with `source_kind=RealCaptureH264`.
- Kept unsupported pixel format, encoder unavailable, encode failed, and empty hook payload as explicit non-encoded results.
- Preserved placeholder H.264 payload source behavior and did not change UDP send, switcher decode/rendering, OBS, or continuous acquisition.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Hooks return encoded H.264 payload bytes only; the boundary owns conversion to `RealCaptureH264`.
- Empty encoded payload from a hook is treated as `EncodeFailed`.
- Placeholder H.264 bytes remain separate and cannot be labeled as real capture output by this boundary.
- Real FFmpeg/hardware encoder implementation remains a later hook implementation.

### Unresolved
- actual FFmpeg or hardware H.264 encoder implementation
- UDP send path using real encoded frame source
- continuous acquisition / frame arrived wait
- real target enumeration
- real H.264 decode, switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS integration

### Next
- Implement a concrete FFmpeg or hardware encoder behind `ClientH264EncoderRuntimeHook`.
- Add an explicit client one-shot path that uses real encoded source while preserving placeholder send behavior.
- Add real H.264 decode / switcher rendering boundary.

### TODO Update
- Marked the H.264 encoder hook boundary complete in Phase 3.
- Updated Current Focus with the raw BGRA frame -> H.264 hook boundary.
- Updated Next Items to put concrete FFmpeg/hardware encoder runtime implementation next.

### Validation
- `cargo fmt`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo fmt --check`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest client-side Windows Graphics Capture frame acquisition boundary after ready session runtime creation.
- Added `ClientCaptureFrameAcquisitionBoundary`, `ClientCaptureFrameAcquisitionInput`, `ClientCaptureFrameAcquisitionResult`, and explicit unavailable reasons.
- Added `ClientWindowsGraphicsCaptureFrameAcquisitionRuntimeHook` behind `cfg(target_os = "windows")`.
- The Windows hook can explicitly call `StartCapture` when requested, attempt one `Direct3D11CaptureFramePool::TryGetNextFrame`, and copy a BGRA8 D3D11 frame surface into `ClientRawCapturedVideoFrame`.
- Added `capture_started` to `ClientCaptureSessionRuntime` so session readiness and capture-start state remain explicit.
- Kept H.264 encode, UDP send changes, switcher rendering, OBS, continuous frame events/waiting, and fake frame generation out of scope.

### Changed Files
- `apps/client/Cargo.toml`
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Frame acquisition is a separate boundary after session runtime creation.
- A call that is not allowed to start capture returns `CaptureNotStarted` instead of implicitly mutating the runtime.
- A call that is allowed to start capture starts once, records `capture_started`, then attempts exactly one frame.
- `NoFrameAvailable` is distinct from acquisition failure.
- The raw frame handoff remains `ClientRawCapturedVideoFrame` with BGRA8 pixels for future H.264 encoder input.

### Unresolved
- real H.264 encoder implementation and configuration
- event/wait based continuous Windows Graphics Capture acquisition
- Windows API-backed target enumeration for non-primary display ids
- UDP send using real encoded frames
- real H.264 decode, switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS integration

### Next
- Implement the H.264 encoder boundary over `ClientRawCapturedVideoFrame` without changing UDP send yet.
- Add a continuous acquisition loop later that waits for frame availability instead of relying only on one immediate `TryGetNextFrame`.
- Add Windows target enumeration for display/window handles.

### TODO Update
- Added the one-frame acquisition boundary to Current Focus.
- Marked first minimal Windows Graphics Capture one-frame acquisition boundary complete in Phase 3.
- Split remaining capture/encode work so actual H.264 encoder implementation is the next video task.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`

---

## 2026-04-24
### 作業者 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、outer while-loop repeated body から caller-owned socket 再確立 hook を注入できる最小配線を追加した。
- real UDP socket 差し替えは既存の hook 抽象をそのまま使い、outer while-loop repeated body 自体は bind / connect / slot 置換を直接持たない形を維持した。
- future continuous heartbeat loop runner が caller-owned UDP socket slot を持ち、real hook を repeated body へ渡すだけで接続できる最小実装形を docs に反映した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyBoundary::run_with_hook(...)`
- repeated body で timer / retry と socket re-establishment hook 呼び出しを分離する単体テスト
- repeated body で stop path passthrough を hook 利用可能時も維持する単体テスト
- future runner から `ClientHeartbeatLoopRealUdpSocketReestablishmentHook` を渡す最小関係の設計追記

### 未実装 / 保留
- future client continuous heartbeat loop runner で caller-owned UDP socket slot を live socket 運用へ接続する本配線
- RTT / offset metrics state commit の continuous loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- server 側 heartbeat timeout loop tick の複数 client 継続実行

### 次にやる候補
- RTT / offset metrics state commit を client continuous heartbeat loop へ接続する
- metrics snapshot export cadence / dashboard refresh 方針を詰める
- future continuous heartbeat loop runner の live socket ownership 配線を最小境界で足す

### TODO更新内容
- 現在位置に repeated body から caller-owned socket 再確立 hook を注入できる状態を反映した。
- heartbeat / client 継続 loop タスクに future runner 側 live socket ownership 配線の保留項目を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 作業者 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、caller-owned socket 再確立 hook を real UDP socket 差し替えへ接続する最小実装を追加した。
- reconnect policy handoff だけを入力源にし、hook 入力から destination / bind address を導出して `bind -> connect -> caller-owned slot 置換` を行う形にした。
- outer while-loop repeated body は変更せず、reconnect flow は `actual reconnect execution result -> reconnect policy -> actual socket re-establishment hook -> continuation state` の explicit な分離を維持した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopRealUdpSocketReplacementInput`
- `ClientHeartbeatLoopRealUdpSocketReplacementRuntime`
- `ClientHeartbeatLoopRealUdpSocketReestablishmentHook`
- `ClientHeartbeatLoopSocketReestablishmentFailureKind` へ bind / connect failure 種別を追加
- real hook 成功 / slot なし deferred / bind failure / connect failure / continuation state carry の単体テストを追加

### 未実装 / 保留
- RTT / offset metrics state commit の continuous loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- server 側 heartbeat timeout loop tick の複数 client 継続実行
- video path / switcher / OBS の本実装

### 次にやる候補
- RTT / offset metrics state commit を client continuous heartbeat loop へ接続する
- metrics snapshot export cadence / dashboard refresh 方針を詰める
- server 側 heartbeat timeout loop tick の複数 client 継続実行へ戻る

### TODO更新内容
- 現在位置に real UDP socket 差し替え hook 完了を反映した。
- 直近でやることを metrics 接続 / cadence / server loop 側へ更新した。
- heartbeat / 検証タスクに real UDP socket 再確立 hook の完了と関連単体テストを追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、actual socket 再確立の最小実装形を caller-owned hook 境界として追加した。
- reconnect policy handoff だけを入力源にし、actual socket 再確立が applied / deferred / failed / stop passthrough を explicit に返す形へ更新した。
- repeated body continuation state が reconnect state を維持したまま進める形は崩さず、default path は deferred のままにして loop 側の責務を増やさないようにした。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopSocketReestablishmentFailureKind`
- `ClientHeartbeatLoopSocketReestablishmentError`
- `ClientHeartbeatLoopSocketReestablishmentHookResult`
- `ClientHeartbeatLoopSocketReestablishmentHook`
- `ClientHeartbeatLoopDeferredSocketReestablishmentHook`
- outer while-loop reconnect flow の actual socket 再確立を caller-owned hook へ委譲する `apply_with_hook(...)`
- actual socket 再確立の applied / deferred / failed / stop passthrough を明示する reconnect result 形
- no reconnect / reconnect-planned input/result / deferred / failed / timer-wait-retry non-reinterpretation / stop passthrough を固定する単体テスト

### 未実装 / 保留
- caller-owned socket 再確立 hook を実 UDP socket 差し替えへ接続する本実装
- RTT / offset metrics state commit の継続 loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- video path / switcher / OBS の本実装

### 次にやる候補
- caller-owned socket 再確立 hook を実 UDP socket 差し替えへ接続する
- RTT / offset metrics state commit を client continuous heartbeat loop へ接続する
- metrics snapshot export cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に caller-owned hook 付き actual socket 再確立境界の完了を反映した。
- 直近でやることを実 UDP socket 差し替えと metrics 接続側へ更新した。
- heartbeat / 検証タスクに actual socket 再確立 boundary と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、actual reconnect policy / socket 再確立 placeholder を outer while-loop path に接続する最小実装形を追加した。
- reconnect policy は actual reconnect execution result だけを読み、timer wait / retry execution を再解釈せずに no-reconnect / reconnect-planned を返す形にした。
- socket 再確立は full 実装にせず deferred placeholder のまま分離し、repeated body continuation state へ explicit reconnect state を保持できるようにした。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopReconnectReason`
- `ClientHeartbeatLoopOuterWhileLoopReconnectPolicyInput`
- `ClientHeartbeatLoopFutureSocketReestablishmentPlan`
- `ClientHeartbeatLoopOuterWhileLoopReconnectPolicyHandoff`
- `ClientHeartbeatLoopOuterWhileLoopReconnectPolicyResult`
- `ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentInput`
- `ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentApplyResult`
- `ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentOutput`
- `ClientHeartbeatLoopOuterWhileLoopReconnectResult`
- `ClientHeartbeatLoopOuterWhileLoopReconnectState`
- `ClientHeartbeatLoopOuterWhileLoopReconnectPolicyBoundary`
- `ClientHeartbeatLoopOuterWhileLoopSocketReestablishmentBoundary`
- `ClientHeartbeatLoopOuterWhileLoopReconnectBoundary`
- reconnect action/result の最小拡張と、repeated body continuation state へ explicit reconnect state を保持する outer while-loop path 接続
- no reconnect / reconnect planned / timer-wait-retry separation / stop passthrough / continuation carry を固定する単体テスト

### 未実装 / 保留
- actual socket 再確立の本実装
- RTT / offset metrics state commit の継続 loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- video path / switcher / OBS の本実装

### 次にやる候補
- actual socket 再確立の最小本実装を reconnect policy handoff へ接続する
- RTT / offset metrics state commit を client continuous heartbeat loop へ接続する
- metrics snapshot export cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に reconnect policy / socket 再確立 placeholder 境界の完了を反映した。
- 直近でやることを actual socket 再確立と metrics 接続側へ更新した。
- heartbeat / 検証タスクに reconnect policy / socket 再確立 placeholder 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、outer while-loop 反復実行本体の最小実装形を追加した。
- repeated body は connection -> one-turn execution -> actual timer/retry/reconnect execution を薄く繰り返すだけにし、continue path では next carry を更新し、stop path では terminal output をそのまま返す形に揃えた。
- caller-owned `max_turns` guard を追加し、継続 state と last explicit execution output を返せるようにしてテストを deterministic にした。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyInput`
- `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyContinuationState`
- `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyResult`
- `ClientHeartbeatLoopOuterWhileLoopRepeatedBodyBoundary`
- repeated body が existing boundary を順に呼び、continue では `next carry` と `last_execution` を保持し、stop では `stop_reason` / `cleanup_completed` / `applied_actions` を再解釈しない薄い runner
- continue 1 turn / stop passthrough / stop terminal output preservation / wakeup-timer-retry-reconnect separation / caller-owned max-turn guard を固定する単体テスト

### 未実装 / 保留
- actual reconnect policy / socket 再確立の本実装
- RTT / offset metrics state commit の継続 loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- video path / switcher / OBS の本実装

### 次にやる候補
- actual reconnect policy / socket 再確立の最小本実装を outer while-loop 経路へ接続する
- RTT / offset metrics state commit を client continuous heartbeat loop へ接続する
- metrics snapshot export cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に outer while-loop 反復実行本体の完了を反映した。
- 直近でやることを reconnect policy と metrics 接続側へ更新した。
- heartbeat / 検証タスクに outer while-loop 反復実行本体と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、outer while-loop one-turn execution result から actual timer wait / retry execution / reconnect execution を分離して適用する最小実装形を追加した。
- stop path では timer / retry / reconnect execution input を作らず、`stop_reason` / `cleanup_completed` / `applied_actions` をそのまま passthrough する形を維持した。
- wakeup を timer / retry / reconnect から分離したまま、future repeated outer while-loop body が順番に呼べる explicit execution result を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionInput`
- `ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionApplyResult`
- `ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionOutput`
- `ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionResult`
- `ClientHeartbeatLoopOuterWhileLoopActualTimerWaitExecutionBoundary`
- `ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionInput`
- `ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionApplyResult`
- `ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionOutput`
- `ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionResult`
- `ClientHeartbeatLoopOuterWhileLoopActualRetryExecutionBoundary`
- `ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionInput`
- `ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionApplyResult`
- `ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionOutput`
- `ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionResult`
- `ClientHeartbeatLoopOuterWhileLoopActualReconnectExecutionBoundary`
- `ClientHeartbeatLoopOuterWhileLoopActualExecutionOutput`
- `ClientHeartbeatLoopOuterWhileLoopActualExecutionResult`
- `ClientHeartbeatLoopOuterWhileLoopActualExecutionBoundary`
- one-turn execution result を single source of truth とし、continue では wakeup / timer wait / retry execution / reconnect execution / next carry を分離したまま actual execution result へ変換する実行境界
- timer wait / retry execution / reconnect explicit / stop passthrough / aggregate separation を固定する単体テスト

### 未実装 / 保留
- client 側 continuous heartbeat loop の outer while-loop 反復本体
- actual reconnect policy / socket 再確立の本実装
- RTT / offset metrics state commit の継続 loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- video path / switcher / OBS の本実装

### 次にやる候補
- client 側 outer while-loop の反復実行本体を one-turn execution body と actual execution 境界に接続する
- actual reconnect policy / socket 再確立の最小本実装を outer while-loop 経路へ接続する
- RTT / offset metrics state commit / cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に actual timer wait / retry execution / reconnect 実行境界の完了を反映した。
- 直近でやることを outer while-loop 反復本体と reconnect policy の残作業へ更新した。
- heartbeat / 検証タスクに actual timer wait / retry execution / reconnect 実行境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、outer while-loop one-turn execution body の最小実装形を追加した。
- outer while-loop connection result だけを入力源にし、continue path の wakeup / timer wait / retry execution / reconnect execution 分離を保ったまま next-step carry を返す薄い boundary を追加した。
- stop path では `stop_reason` / `cleanup_completed` / `applied_actions` を再解釈せず、そのまま passthrough する形に揃えた。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionInput`
- `ClientHeartbeatLoopOuterWhileLoopOneTurnNextStepState`
- `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionOutput`
- `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionResult`
- `ClientHeartbeatLoopOuterWhileLoopOneTurnExecutionBoundary`
- outer while-loop connection result を single source of truth として consume し、continue では wakeup / timer wait / retry execution / reconnect execution / next carry を explicit なまま返す one-turn execution body
- continue path と stop path の意味を崩さない単体テスト

### 未実装 / 保留
- client 側 continuous heartbeat loop の outer while-loop 反復本体
- actual timer wait / retry execution / reconnect の実処理
- RTT / offset metrics state commit の継続 loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- video path / switcher / OBS の本実装

### 次にやる候補
- actual timer wait / retry execution / reconnect の実処理を最小単位へ分解する
- client 側 outer while-loop の反復実行本体を one-turn execution body に接続する
- RTT / offset metrics state commit / cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に outer while-loop one-turn execution body 境界の完了を反映した。
- 直近でやることを actual timer / retry / reconnect 実処理と outer while-loop 反復本体へ更新した。
- heartbeat / 検証タスクに outer while-loop one-turn execution body 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、outer while-loop connection の最小実装形を追加した。
- completed continuous heartbeat loop body から wakeup planning / execution / actual side effect を順に配線し、continue path と stop path を崩さない接続 boundary を追加した。
- wakeup state を timer / retry / reconnect から分離したまま、future outer while-loop runner が受け取る explicit continue output だけを整えた。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopOuterWhileLoopWakeupState`
- `ClientHeartbeatLoopOuterWhileLoopConnectionOutput`
- `ClientHeartbeatLoopOuterWhileLoopConnectionResult`
- `ClientHeartbeatLoopOuterWhileLoopConnectionBoundary`
- completed body -> wakeup planning -> wakeup execution -> wakeup actual side effect の固定順配線
- continue path が wakeup / timer wait / retry execution / reconnect execution を別 field のまま保持する単体テスト
- stop path が completed body の terminal output を再解釈しない単体テスト

### 未実装 / 保留
- client 側 continuous heartbeat loop の outer while-loop 本体
- actual timer wait / retry execution / reconnect の実処理
- RTT / offset metrics state commit の継続 loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- video path / switcher / OBS の本実装

### 次にやる候補
- client 側 outer while-loop 本体の最小 turn 実行形を整理する
- actual timer wait / retry execution / reconnect の実処理を最小単位へ分解する
- RTT / offset metrics state commit / cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に outer while-loop connection 境界の完了を反映した。
- 直近でやることを outer while-loop 本体の最小 turn 実行形へ更新した。
- heartbeat / 検証タスクに outer while-loop connection 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- client continuous heartbeat loop execution path に戻り、heartbeat timeout notice wakeup actual side effect の最小実装形を追加した。
- wakeup execution result だけを入力源にし、continue without wakeup / continue with wakeup / stop を崩さない actual side-effect boundary を追加した。
- timer / retry / reconnect execution には触れず、wakeup responsibility だけを 1 段分離した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectInput`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectApplyResult`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectOutput`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectResult`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupActualSideEffectBoundary`
- wakeup execution result だけから continue-with-wakeup actual side-effect input を作る最小変換
- continue without wakeup execution / continue with wakeup execution applied / stop passthrough を分離した単体テスト
- wakeup actual side-effect result が timer / retry / reconnect concern と混ざらないことの単体テスト

### 未実装 / 保留
- client continuous heartbeat loop の outer while-loop 本体
- actual timer wait / retry execution / reconnect の実処理
- RTT / offset metrics state commit の継続 loop 接続
- metrics snapshot export cadence / dashboard refresh 方針
- video path / switcher / OBS の本実装

### 次にやる候補
- client continuous heartbeat loop の outer while-loop と wakeup / timer / retry / reconnect の接続整理
- actual timer wait / retry execution / reconnect の実処理を最小単位へ分解
- RTT / offset metrics state commit / cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に wakeup actual side-effect 境界の完了を反映した。
- 直近でやることを outer while-loop と actual timer / retry / reconnect 側へ更新した。
- heartbeat / 検証タスクに wakeup actual side-effect 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- `docs/operations/todo.md` を session-log と突き合わせて監査し、誤解しやすい重複と古い現在位置を整理した。
- client continuous heartbeat loop まわりの完了済み最小境界と、未完了の実 side effect / actual timer wait / retry / reconnect / metrics cadence を TODO 上で分離した。

### 変更ファイル
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `現在位置` を直近の session-log ベースに要約し直した
- `直近でやること` を wakeup 実 side effect / outer while-loop と実処理接続 / metrics cadence に更新した
- heartbeat / client セクションの重複タスクを統合し、heartbeat timeout notice wakeup 実 side effect と actual timer wait / retry execution / reconnect 実処理を明示 TODO に追加した
- ロードマップの `RTT / offset 推定` を最小 state commit 完了と残課題に分けた

### 未実装 / 保留
- heartbeat timeout notice wakeup の実 side effect
- actual timer wait / retry execution / reconnect の実処理
- RTT / offset metrics state commit の継続 loop 接続と export cadence / dashboard refresh 方針
- video path / switcher / OBS の本実装

### 次にやる候補
- heartbeat timeout notice wakeup の実 side effect 範囲確定
- client continuous heartbeat loop の outer while-loop と actual timer / retry / reconnect 実処理接続整理
- RTT / offset metrics state commit / cadence / dashboard refresh 方針整理

### TODO更新内容
- TODO の `現在位置` と `直近でやること` を更新した。
- heartbeat / client / ロードマップの重複と古い表現を整理した。

### 検証
- docs のみ更新のためビルド / テストは未実施

## 2026-04-24
### 担当 - Codex

### 今回の作業
- heartbeat timeout notice wakeup planning から wakeup execution へ進む最小境界を追加した。
- `ContinueWithoutWakeup` / `ContinueWithWakeup` / `Stop` を維持したまま、execution 側で `ContinueWithoutWakeupExecution` / `ContinueWithWakeupExecutionApplied` / `Stop` に分離した。
- wakeup execution は timer wait / retry / reconnect execution とは別責務のまま、real wakeup side effect なしで explicit result shape だけを定義した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionInput`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupApplyResult`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionOutput`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionResult`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupExecutionBoundary`
- wakeup planning result だけを入力源にして wakeup execution input を作る最小変換
- continue with wakeup の execution input / applied result、continue without wakeup の explicit passthrough、stop passthrough の単体テスト
- wakeup execution result が timer / retry / reconnect concern と混ざらないことの単体テスト

### 未実装 / 保留
- heartbeat timeout notice wakeup の実 side effect
- continuous heartbeat loop 本体
- actual timer wait / retry execution / reconnect の実処理
- stats metrics state commit
- completed smoothing / outlier model
- dashboard 本体
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- heartbeat timeout notice wakeup の実 side effect 最小範囲整理
- actual timer wait / retry / reconnect の実行本体に進む前の境界整理
- RTT / offset metrics snapshot の export cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に heartbeat timeout notice wakeup planning から wakeup execution への最小境界完了を反映した。
- 直近でやることを wakeup execution から wakeup の実 side effect 最小範囲整理へ更新した。
- client / 検証タスクに heartbeat timeout notice wakeup execution 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- completed continuous heartbeat loop body result から heartbeat timeout notice wakeup planning へ進む最小境界を追加した。
- stop path はそのまま passthrough し、continue path だけを `ContinueWithoutWakeup` と `ContinueWithWakeup` に分離して、wakeup follow-up の必要性を explicit にした。
- wakeup planning は timer / retry / reconnect execution 本体と分離したまま、timer wait がある continue path だけを wakeup-ready handoff にする shape で固定した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupInput`
- `ClientHeartbeatLoopFutureHeartbeatTimeoutNoticeWakeupPlan`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupHandoff`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupResult`
- `ClientHeartbeatLoopHeartbeatTimeoutNoticeWakeupBoundary`
- completed continuous heartbeat loop body result から continue-path output のみを wakeup planning input に変換する最小境界
- continue without wakeup / continue with wakeup-ready handoff / stop passthrough を分離する単体テスト
- timer wait / retry / reconnect execution concern と wakeup planning concern を分離したまま保持する単体テスト

### 未実装 / 保留
- heartbeat timeout notice wakeup execution 本体
- continuous heartbeat loop 本体
- actual timer wait / retry execution / reconnect の実処理
- stats metrics state commit
- completed smoothing / outlier model
- dashboard 本体
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体の最小範囲整理
- actual timer wait / retry / reconnect の実行本体に進む前の境界整理
- RTT / offset metrics snapshot の export cadence / dashboard refresh 方針整理

### TODO更新内容
- 現在位置に completed continuous heartbeat loop body result から heartbeat timeout notice wakeup planning への最小境界完了を反映した。
- 直近でやることを heartbeat timeout notice wakeup 実行本体の最小範囲整理へ更新した。
- client / 検証タスクに heartbeat timeout notice wakeup planning 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当 - Codex

### 今回の作業
- repeated invocation result から completed continuous heartbeat loop body まで既存 boundary を薄く配線する最小 composition を completed continuous heartbeat loop body として整理した。
- continue path は `carry` / `timer_wait` / `retry_execution` / `reconnect_execution`、stop path は `stop_reason` / `cleanup_completed` / `applied_actions` をそのまま保持する shape を確認した。
- completed continuous heartbeat loop body 自体の単体テストと architecture / todo の更新を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCompletedContinuousBodyBoundary`
- `ClientHeartbeatLoopCompletedContinuousBodyResult`
- repeated invocation -> actual while-loop -> cleanup responsibility -> cleanup ordering -> cleanup execution planning -> cleanup actual side-effect apply -> completed-loop stop-path output -> actual while-loop termination -> completed body integration -> timer / retry / reconnect integration -> actual execution integration -> completed body connection を 1 回だけ配線する最小 completed body composition
- continue path で explicit future execution actions を保持する completed continuous heartbeat loop body の単体テスト
- stop path で explicit cleanup-completed terminal output を保持する completed continuous heartbeat loop body の単体テスト
- continue / stop を vague result に潰さない completed continuous heartbeat loop body の単体テスト

### 未実装 / 保留
- continuous heartbeat loop 本体
- actual timer wait / retry execution / reconnect の実処理
- heartbeat timeout wakeup execution
- stats metrics state commit
- completed smoothing / outlier model
- dashboard 本体
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける
- actual timer wait / retry / reconnect の実行本体に進む前の境界整理を続ける
- RTT / offset metrics snapshot の export cadence / dashboard refresh 方針を整理する

### TODO更新内容
- 現在位置に completed continuous heartbeat loop body の最小実装完了を反映した。
- 直近でやることを completed continuous heartbeat loop 本体の最小実装整理から次の未実装項目へ更新した。
- client / 検証タスクに completed continuous heartbeat loop body 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当
- Codex

### 今回の作業
- future actual timer wait / retry execution / reconnect actions から completed continuous heartbeat loop body へ進む最小範囲を整理した。
- actual execution integration result だけから completed loop body connection input を作る最小境界を追加した。
- continue execution handoff、stop result、completed loop body connection result を分離し、future execution actions を explicit に保つ接続 boundary を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCompletedContinuousBodyConnectionInput`
- `ClientHeartbeatLoopCompletedContinuousBodyConnectionOutput`
- `ClientHeartbeatLoopCompletedContinuousBodyConnectionResult`
- `ClientHeartbeatLoopCompletedContinuousBodyConnectionInput::from_actual_execution_integration(...)`
- `ClientHeartbeatLoopCompletedContinuousBodyConnectionBoundary`
- continue path から completed loop body connection input を作る単体テスト
- stop path では completed loop body connection input を作らない単体テスト
- continue / stop separation を保つ単体テスト
- timer wait / retry / reconnect を explicit future execution actions のまま保つ単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop 本体
- actual timer wait / retry execution / reconnect の実処理
- heartbeat timeout wakeup execution
- future full completed continuous heartbeat loop implementation
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- completed continuous heartbeat loop 本体の最小実装整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に future actual timer wait / retry execution / reconnect actions から completed continuous heartbeat loop body への最小境界完了を反映した。
- 直近でやることを completed continuous heartbeat loop 本体の最小実装整理へ更新した。
- client / 検証タスクに completed continuous heartbeat loop body connection 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当
- Codex

### 今回の作業
- future timer / retry / reconnect planning handoff から future actual timer wait / retry execution / reconnect integration へ進む最小範囲を整理した。
- planning handoff だけから actual execution integration input を作る最小境界を追加した。
- continue execution handoff と stop passthrough を分離し、timer wait / retry / reconnect scope を explicit actions として固定した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput`
- `ClientHeartbeatLoopFutureActualTimerWaitAction`
- `ClientHeartbeatLoopFutureActualRetryExecutionAction`
- `ClientHeartbeatLoopFutureActualReconnectExecutionAction`
- `ClientHeartbeatLoopActualTimerRetryReconnectExecutionHandoff`
- `ClientHeartbeatLoopActualTimerRetryReconnectExecutionResult`
- `ClientHeartbeatLoopActualTimerRetryReconnectExecutionInput::from_planning_handoff(...)`
- `ClientHeartbeatLoopActualTimerRetryReconnectExecutionBoundary`
- continue path から actual execution integration input を作る単体テスト
- stop path では actual execution integration input を作らない単体テスト
- continue / stop separation を保つ単体テスト
- timer wait / retry / reconnect を explicit future execution actions として保つ単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop 本体
- actual timer wait / retry execution / reconnect の実処理
- heartbeat timeout wakeup execution
- future actual execution actions から completed continuous heartbeat loop 本体への接続
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- future actual timer wait / retry execution / reconnect actions から completed continuous heartbeat loop 本体へつなぐ最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に future timer / retry / reconnect planning handoff から future actual timer wait / retry execution / reconnect integration への最小境界完了を反映した。
- 直近でやることを future actual timer wait / retry execution / reconnect actions から completed continuous heartbeat loop 本体への接続へ更新した。
- client / 検証タスクに actual timer / retry / reconnect execution integration 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当
- Codex

### 今回の作業
- completed loop body result から future timer / retry / reconnect integration へ進む最小範囲を整理した。
- completed loop body result だけから continue-path planning input を作る最小境界を追加した。
- continue carry、stop result、future planning result を分離した timer / retry / reconnect integration boundary を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopTimerRetryReconnectIntegrationInput`
- `ClientHeartbeatLoopFutureTimerRetryReconnectPlanningHandoff`
- `ClientHeartbeatLoopTimerRetryReconnectIntegrationResult`
- `ClientHeartbeatLoopTimerRetryReconnectIntegrationInput::from_completed_body_result(...)`
- `ClientHeartbeatLoopTimerRetryReconnectIntegrationBoundary`
- continue path から timer / retry / reconnect integration input を作る単体テスト
- stop path では timer / retry / reconnect integration input を作らない単体テスト
- stop path を continue planning に畳み込まない単体テスト
- stop-only semantics を保った integration result の単体テスト
- continue / stop / future planning の分離を保つ単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop 本体
- actual timer wait / retry execution / reconnect
- heartbeat timeout wakeup execution
- future actual timer wait / retry execution / reconnect integration
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- future actual timer wait / retry execution / reconnect integration へ planning handoff を接続する最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に completed loop body result から future timer / retry / reconnect integration への最小境界完了を反映した。
- 直近でやることを future actual timer wait / retry execution / reconnect integration への planning handoff 接続へ更新した。
- client / 検証タスクに timer / retry / reconnect integration 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当
- Codex

### 今回の作業
- actual while-loop termination result から future completed continuous heartbeat loop body へ進む最小範囲を整理した。
- actual while-loop termination result だけから completed loop body stop-path input を作る stop-only 境界を追加した。
- continue carry、termination result、completed loop body result を分離した completed-body integration boundary を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCompletedBodyInput`
- `ClientHeartbeatLoopCompletedBodyTerminalOutput`
- `ClientHeartbeatLoopCompletedBodyIntegrationResult`
- `ClientHeartbeatLoopCompletedBodyInput::from_actual_while_loop_termination(...)`
- `ClientHeartbeatLoopCompletedBodyIntegrationBoundary`
- stop path から completed loop body input を作る単体テスト
- continue path では completed loop body stop-path input を作らない単体テスト
- continue carry を completed loop body result に畳み込まない単体テスト
- stop-only semantics を保った completed loop body integration result の単体テスト
- stop_reason / cleanup_completed / applied_actions を再解釈せず保持する単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop 本体
- completed continuous heartbeat loop body result を future timer / retry / reconnect integration へつなぐ統合
- actual timer wait / retry execution / reconnect
- heartbeat timeout wakeup execution
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- future timer / retry / reconnect integration へ completed continuous heartbeat loop body result を接続する最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に actual while-loop termination result から future completed continuous heartbeat loop body への最小境界完了を反映した。
- 直近でやることを completed loop body result から future timer / retry / reconnect integration への接続へ更新した。
- client / 検証タスクに completed continuous heartbeat loop body integration 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当
- Codex

### 今回の作業
- completed-loop terminal stop-path output から future actual while-loop termination へ進む最小範囲を整理した。
- completed-loop stop-path result だけから actual while-loop termination input を作る stop-only 境界を追加した。
- continue carry、terminal stop-path output、actual while-loop termination result を分離した termination boundary を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopActualWhileLoopTerminationInput`
- `ClientHeartbeatLoopActualWhileLoopTerminalOutput`
- `ClientHeartbeatLoopActualWhileLoopTerminationResult`
- `ClientHeartbeatLoopActualWhileLoopTerminationInput::from_completed_loop_stop_path(...)`
- `ClientHeartbeatLoopActualWhileLoopTerminationBoundary`
- stop path から actual while-loop termination input を作る単体テスト
- continue path では actual while-loop termination input を作らない単体テスト
- continue carry を termination output に変換しない単体テスト
- stop-only semantics を保った actual while-loop termination result の単体テスト
- stop_reason / cleanup_completed / applied_actions を再解釈せず保持する単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop 本体
- actual while-loop termination result を future completed continuous heartbeat loop body へつなぐ統合
- actual timer wait / retry execution / reconnect
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- future completed continuous heartbeat loop body へ actual while-loop termination result を接続する最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に completed-loop terminal stop-path output から future actual while-loop termination への最小境界完了を反映した。
- 直近でやることを actual while-loop termination result から future completed continuous heartbeat loop body への接続へ更新した。
- client / 検証タスクに actual while-loop termination 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当
- Codex

### 今回の作業
- cleanup actual side-effect result から future completed continuous heartbeat loop stop path へ進む最小範囲を整理した。
- cleanup side-effect result だけから terminal stop-path input を作る stop-only 境界を追加した。
- continue carry と terminal stop-path output を分離した completed-loop stop-path boundary を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCompletedLoopStopPathInput`
- `ClientHeartbeatLoopTerminalStopPathOutput`
- `ClientHeartbeatLoopCompletedLoopStopPathHandoff`
- `ClientHeartbeatLoopCompletedLoopStopPathResult`
- `ClientHeartbeatLoopCompletedLoopStopPathInput::from_cleanup_side_effect(...)`
- `ClientHeartbeatLoopCompletedLoopStopPathBoundary`
- stop path から terminal stop-path input を作る単体テスト
- continue path では terminal stop-path input を作らない単体テスト
- continue carry を terminal output に変換しない単体テスト
- stop-only semantics を保った terminal stop-path output の単体テスト
- cleanup ordering / execution planning を再解釈しない単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop 本体
- terminal stop-path output を future actual while-loop termination へつなぐ統合
- actual timer wait / retry execution / reconnect
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- completed continuous heartbeat loop の terminal stop-path output を future actual while-loop termination へ接続する最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に cleanup actual side-effect result から future completed continuous heartbeat loop stop path への最小境界完了を反映した。
- 直近でやることを terminal stop-path output から future actual while-loop termination への接続へ更新した。
- client / 検証タスクに completed-loop stop-path 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

## 2026-04-24
### 担当
- Codex

### 今回の作業
- cleanup execution planning から future actual cleanup side effects へ進む最小範囲を整理した。
- cleanup execution planning result だけから actual cleanup side-effect input を作る stop-only 境界を追加した。
- final flush / log writer invocation / resource release を stop-path ordered apply result としてだけ返す最小 side-effect apply 境界を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCleanupSideEffectInput`
- `ClientHeartbeatLoopCleanupAppliedAction`
- `ClientHeartbeatLoopCleanupSideEffectApplyResult`
- `ClientHeartbeatLoopCleanupSideEffectResult`
- `ClientHeartbeatLoopCleanupSideEffectInput::from_execution_planning(...)`
- `ClientHeartbeatLoopCleanupSideEffectBoundary`
- stop path から actual cleanup side-effect input を作る単体テスト
- continue path では side-effect input を作らない単体テスト
- stop-only semantics を保った side-effect apply result の単体テスト
- flush / log / release の apply 順序を explicit に保つ単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- cleanup side-effect result を future completed continuous heartbeat loop stop path へつなぐ統合
- actual timer wait / retry execution / reconnect
- final flush / log writer invocation / resource release の複雑な実処理

### 次にやる候補
- future completed continuous heartbeat loop stop path へ cleanup side-effect result を接続する最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に cleanup actual side-effect apply の最小境界完了を反映した。
- 直近でやることを future completed continuous heartbeat loop stop path 接続へ更新した。
- client / 検証タスクに cleanup actual side-effect apply 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- cleanup ordering から future actual cleanup execution へ進む最小範囲を整理した。
- ordered cleanup handoff だけから execution planning input を作る stop-only 境界を追加した。
- final flush / log writer invocation / resource release を future ordered actions としてだけ表現する execution planning を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCleanupExecutionInput`
- `ClientHeartbeatLoopFutureCleanupAction`
- `ClientHeartbeatLoopCleanupExecutionPlan`
- `ClientHeartbeatLoopCleanupExecutionPlanningHandoff`
- `ClientHeartbeatLoopCleanupExecutionResult`
- `ClientHeartbeatLoopCleanupExecutionInput::from_ordering(...)`
- `ClientHeartbeatLoopCleanupExecutionBoundary`
- stop path から execution input を作る単体テスト
- continue path では execution planning input を作らない単体テスト
- stop-only semantics を保った execution planning result の単体テスト
- flush / log / release を future ordered actions のみで保持する単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- cleanup execution planning の次段になる future actual cleanup side effects
- actual timer wait / retry execution / reconnect
- actual cleanup / final flush / log writer invocation / resource release の実処理

### 次にやる候補
- cleanup execution planning から future actual cleanup side effects へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に cleanup execution planning の最小境界完了を反映した。
- 直近でやることを future actual cleanup side effects 整理へ更新した。
- client / 検証タスクに cleanup execution planning 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- cleanup responsibility から future actual cleanup ordering へ進む最小範囲を整理した。
- cleanup responsibility の stop-only input を cleanup ordering input / ordered handoff に変換する境界を追加した。
- cleanup ordering は continue path では何も生成せず、stop path のみ ordered cleanup plan を返す方針に固定した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCleanupOrderingInput`
- `ClientHeartbeatLoopOrderedCleanupPlan`
- `ClientHeartbeatLoopCleanupOrderingHandoff`
- `ClientHeartbeatLoopCleanupOrderingResult`
- `ClientHeartbeatLoopCleanupOrderingBoundary`
- `ClientHeartbeatLoopCleanupOrderingInput::from_responsibility(...)`
- stop path から cleanup ordering input を作る単体テスト
- continue path では cleanup ordering を作らない単体テスト
- stop-only semantics を保った ordered cleanup handoff の単体テスト
- cleanup execution boundary を ordered handoff 入力へ更新

### 未実装 / 保留
- completed continuous heartbeat loop
- cleanup ordering の次段になる future actual cleanup execution
- actual timer wait / retry execution / reconnect
- actual cleanup / final flush / log writer invocation

### 次にやる候補
- cleanup ordering から future actual cleanup execution へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に cleanup ordering の最小境界完了を反映した。
- 直近でやることを future actual cleanup execution 整理へ更新した。
- client / 検証タスクに cleanup ordering 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- future actual while-loop から cleanup responsibility へ進む最小範囲を整理した。
- stop handoff から明示的な cleanup input / cleanup plan を作る responsibility / execution 境界を追加した。
- cleanup は stop 時のみ起動し、retry や通常 iteration では起動しない最小方針を docs と code に固定した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCleanupPlan`
- `ClientHeartbeatLoopCleanupResponsibilityInput`
- `ClientHeartbeatLoopCleanupResponsibilityResult`
- `ClientHeartbeatLoopCleanupExecutionResult`
- `ClientHeartbeatLoopCleanupResponsibilityBoundary`
- `ClientHeartbeatLoopCleanupExecutionBoundary`
- continue carry をそのまま返す cleanup responsibility 単体テスト
- stop handoff から explicit cleanup input を作る単体テスト
- cleanup execution が side effect なしで plan を返す単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- cleanup responsibility の次段になる future actual cleanup ordering
- actual timer wait / retry execution / reconnect
- actual cleanup / final flush / log writer invocation

### 次にやる候補
- cleanup responsibility から future actual cleanup ordering へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に cleanup responsibility の最小境界完了を反映した。
- 直近でやることを future actual cleanup ordering 整理へ更新した。
- client / 検証タスクに cleanup responsibility 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_cleanup`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- eventual repeated invocation から future actual while-loop へ進む最小範囲を整理した。
- repeated invocation の continue / stop を caller-facing な while-loop step result に落とす境界を追加した。
- actual timer / retry / cleanup / final flush は実行せず、typed result の返却だけに留めた。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopActualWhileLoopStopHandoff`
- `ClientHeartbeatLoopInvocationStepResult`
- `ClientHeartbeatLoopActualWhileLoopBoundary`
- continue carry を保持する future actual while-loop 単体テスト
- stop を loop stop handoff に変換する future actual while-loop 単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- future actual while-loop の次段になる cleanup responsibility
- actual timer wait / retry execution / reconnect
- actual cleanup / final flush / log writer invocation

### 次にやる候補
- future actual while-loop から cleanup responsibility へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に future actual while-loop の最小境界完了を反映した。
- 直近でやることを cleanup responsibility 整理へ更新した。
- client / 検証タスクに future actual while-loop 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_actual_while_loop`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- caller-facing shell runner から eventual repeated invocation へ進む最小範囲を整理した。
- shell runner の continue / stop を next-step carry / cleanup handoff に落とす repeated invocation 境界を追加した。
- actual timer / retry / cleanup / final flush は実行せず、typed result の返却だけに留めた。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopRepeatedInvocationStopReason`
- `ClientHeartbeatLoopRepeatedInvocationNextStepCarry`
- `ClientHeartbeatLoopRepeatedInvocationResult`
- `ClientHeartbeatLoopRepeatedInvocationBoundary`
- next-step carry を保持する repeated invocation 単体テスト
- stop を cleanup handoff へ変換する repeated invocation 単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- eventual repeated invocation の次段になる future actual while-loop
- actual timer wait / retry execution / reconnect
- actual cleanup / final flush / log writer invocation

### 次にやる候補
- eventual repeated invocation から future actual while-loop へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に eventual repeated invocation の最小境界完了を反映した。
- 直近でやることを future actual while-loop 整理へ更新した。
- client / 検証タスクに repeated invocation 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_repeated_invocation`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- completed continuous heartbeat loop outer shell から caller-facing shell runner へ進む最小範囲を整理した。
- outer shell を 1 回呼んで caller-facing な continue / stop result を返す shell runner 境界を追加した。
- actual timer / retry / cleanup / final flush は実行せず、typed result の返却だけに留めた。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopShellRunnerStopReason`
- `ClientHeartbeatLoopShellRunnerResult`
- `ClientHeartbeatLoopShellRunnerBoundary`
- continue apply-order を保持する shell runner 単体テスト
- cleanup trigger を runner stop reason へ変換する単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- shell runner の次段になる eventual repeated invocation
- actual timer wait / retry execution / reconnect
- actual cleanup / final flush / log writer invocation

### 次にやる候補
- caller-facing shell runner から eventual repeated invocation へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に caller-facing shell runner の最小境界完了を反映した。
- 直近でやることを eventual repeated invocation 整理へ更新した。
- client / 検証タスクに shell runner 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_shell_runner`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- completed continuous heartbeat loop outer shell の最小範囲を整理した。
- apply-order の結果を caller-facing な continue / stop に変換する outer shell 境界を追加した。
- actual timer / retry / cleanup / final flush は実行せず、typed result の返却だけに留めた。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopShellStopReason`
- `ClientHeartbeatLoopShellResult`
- `ClientHeartbeatLoopOuterShellBoundary`
- continue apply-order をそのまま保持する単体テスト
- cleanup trigger を shell stop reason へ変換する単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- caller-facing shell runner / repeated loop entry
- actual timer wait / retry execution / reconnect
- actual cleanup / final flush / log writer invocation

### 次にやる候補
- completed continuous heartbeat loop outer shell から caller-facing shell runner へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に completed continuous heartbeat loop outer shell の最小境界完了を反映した。
- 直近でやることを caller-facing shell runner 整理へ更新した。
- client / 検証タスクに outer shell 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_outer_shell`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- future actual timer / retry / cleanup apply call order の最小範囲を整理した。
- repeated invocation skeleton の結果から、次に timer / retry / cleanup のどれを呼ぶべきかだけを返す apply-order 境界を追加した。
- 実 timer / retry / cleanup / final flush は実装しなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCleanupTrigger`
- `ClientHeartbeatLoopApplyOrderResult`
- `ClientHeartbeatLoopApplyOrderBoundary`
- timer apply order テスト
- retry apply order テスト
- cleanup trigger テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- completed continuous heartbeat loop outer shell
- actual timer wait / retry execution / reconnect
- actual cleanup / final flush / log writer invocation
- future completed loop body の実処理

### 次にやる候補
- completed continuous heartbeat loop outer shell の最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に actual timer / retry / cleanup apply call order 境界完了を反映した。
- 直近でやることを completed continuous heartbeat loop outer shell 整理へ更新した。
- client / 検証タスクに apply-order 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_apply_order`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- eventual while-loop repeated invocation skeleton / stop flag refresh の最小範囲を整理した。
- caller contract の continue / stop を受けて、次 iteration の carry state か stop handoff を返す skeleton 境界を追加した。
- 実 repeated invocation、while-loop、本 sleep / retry / reconnect / cleanup 実行には進まなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopStopRefreshInput`
- `ClientHeartbeatLoopIterationCarryState`
- `ClientHeartbeatLoopSkeletonResult`
- `ClientHeartbeatLoopSkeletonBoundary`
- wait contract から次 iteration carry を組むテスト
- retry attempt を carry するテスト

### 未実装 / 保留
- completed continuous heartbeat loop
- future actual timer / retry / cleanup apply call order
- actual repeated invocation / stop flag refresh 実行本体
- actual timer wait / retry execution / reconnect
- shutdown cleanup / final flush / log writer invocation

### 次にやる候補
- future actual timer / retry / cleanup apply call order の最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に repeated invocation skeleton / stop flag refresh 境界完了を反映した。
- 直近でやることを future actual timer / retry / cleanup apply call order 整理へ更新した。
- client / 検証タスクに skeleton 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_skeleton`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- completed-step runtime から eventual while-loop ownership / caller contract へ進む最小範囲を整理した。
- 1 step runtime の結果から、caller が次 step の所有権を維持するか、cleanup へ stop handoff を渡すかだけを返す最小境界を追加した。
- 実 while-loop、stop flag refresh、本 sleep / retry / reconnect / cleanup 実行には進まなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopWhileLoopStopHandoff`
- `ClientHeartbeatLoopCallerContractResult`
- `ClientHeartbeatLoopWhileLoopOwnershipBoundary`
- continue caller contract テスト
- stop handoff テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- eventual while-loop repeated invocation skeleton / stop flag refresh
- actual timer wait / retry execution / reconnect
- shutdown cleanup / final flush / log writer invocation
- future completed loop body の実処理

### 次にやる候補
- eventual while-loop repeated invocation skeleton / stop flag refresh の最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に eventual while-loop ownership / caller contract の最小境界完了を反映した。
- 直近でやることを repeated invocation skeleton / stop flag refresh 整理へ更新した。
- client / 検証タスクに while-loop ownership / caller contract 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_while_loop_ownership`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- client 側 continuous heartbeat loop の completed 本体に入る前の最小実装として、completed-loop 相当 1 step runtime 境界を追加した。
- repeated body -> outer controller / shutdown apply -> lifecycle -> sequencing -> ordering を 1 回だけつなぎ、caller-owned input から typed decision を返す最小 runtime に留めた。
- 実 sleep / timer / retry / reconnect / shutdown cleanup / final flush / 無限 while-loop には進まなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopCompletedStepRuntimeInput`
- `ClientHeartbeatLoopCompletedStepRuntimeResult`
- `ClientHeartbeatLoopCompletedStepRuntimeBoundary`
- wait ordering を返す completed-step runtime テスト
- caller stop で cleanup stop を返す completed-step runtime テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- eventual while-loop ownership / caller contract
- actual timer wait / retry execution / reconnect
- shutdown cleanup / final flush / log writer invocation
- future completed loop body の実処理

### 次にやる候補
- completed-loop 相当 1 step runtime から eventual while-loop ownership / caller contract へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に completed-loop 相当 1 step runtime 境界の完了を反映した。
- 直近でやることを eventual while-loop ownership / caller contract 整理へ更新した。
- client / 検証タスクに completed-step runtime 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_completed_step_runtime`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- actual timer / retry / cleanup sequencing から future completed loop body の実行順序へ進む最小範囲を整理した。
- sequencing の typed handoff を受けて、completed body が stop / retry / wait / immediate continue のどれを先に呼ぶかだけを返す ordering 境界を追加した。
- actual timer wait、retry 実行、cleanup 実行、while-loop 本体には進まなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopStepOrdering`
- `ClientHeartbeatLoopCompletedBodySequencingHandoff`
- `ClientHeartbeatLoopCompletedBodyStopResult`
- `ClientHeartbeatLoopStepOrderingResult`
- `ClientHeartbeatLoopStepOrderingBoundary`
- retry 優先 / wait path / stop for cleanup の ordering 単体テスト

### 未実装 / 保留
- completed continuous heartbeat loop
- future completed loop body の実行本体
- eventual while-loop ownership / caller contract
- actual timer wait / retry execution / reconnect
- shutdown cleanup / final flush / log writer invocation

### 次にやる候補
- future completed loop body から eventual while-loop ownership / caller contract へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に future completed loop body 実行順序境界の完了を反映した。
- 直近でやることを eventual while-loop ownership / caller contract 整理へ更新した。
- client / 検証タスクに ordering 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_step_ordering`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-24
### 担当
- Codex

### 今回の作業
- future completed loop lifecycle から actual timer / retry / cleanup sequencing へ進む最小範囲を整理した。
- lifecycle の continue / stop 判定を、timer wait / retry execution / cleanup sequencing の typed handoff に落とす最小境界を追加した。
- actual sleep、retry 再実行、reconnect、cleanup 実行には進まず、future completed loop body が消費する sequencing までに止めた。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopTimerWaitDecision`
- `ClientHeartbeatLoopRetryExecutionResult`
- `ClientHeartbeatLoopCleanupSequencingResult`
- `ClientHeartbeatLoopSequencingResult`
- `ClientHeartbeatLoopSequencingBoundary`
- retry sleep を controller sleep より優先する sequencing 判定
- lifecycle stop から cleanup 開始 handoff へ落とす sequencing 判定
- sequencing 境界の単体テストを追加

### 未実装 / 保留
- completed continuous heartbeat loop
- actual timer wait 実行
- retry execution / reconnect
- shutdown cleanup / final flush / log writer invocation
- future completed loop body の while-loop 本体

### 次にやる候補
- actual timer / retry / cleanup sequencing から future completed loop body の実行順序へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新内容
- 現在位置に actual timer / retry / cleanup sequencing の最小境界完了を反映した。
- 直近でやることを future completed loop body の実行順序整理へ更新した。
- client / 検証タスクに sequencing 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_sequencing`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 種別
- Codex

### 今回の作業
- client 側 continuous heartbeat loop 本体の最小実装範囲を整理した。
- completed loop には進まず、caller-owned socket / counters を使う 1 tick runtime 境界を追加した。
- controller / body / encode-send / ack receive / stats return / counters / sleep-retry / logging / shutdown を 1 回だけ接続した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- client 側の最小実装は `ClientHeartbeatLoopOneTickRuntimeBoundary::run_one` とし、繰り返し loop ではなく 1 tick の同期実行境界に限定する。
- one-tick runtime は caller-owned `UdpSocket` と caller-owned `ClientHeartbeatLoopCountersState` を受け取る。
- one-tick runtime は body -> controller -> controller result -> heartbeat send -> ack receive -> optional stats return send -> counters commit -> retry plan の順に接続する。
- ack receive timeout は `AckMissed` として counters に反映し、retry apply の failure result も返す。
- 実 sleep、socket timeout 設定、JSON Lines writer 呼び出し、shutdown cleanup、retry execution、completed continuous loop は今回の対象外に残す。

### 実装したこと
- `ClientHeartbeatLoopOneTickRuntimeInput` を追加した。
- `ClientHeartbeatLoopOneTickRuntimeFailure` を追加した。
- `ClientHeartbeatLoopOneTickRuntimeResult` を追加した。
- `ClientHeartbeatLoopOneTickRuntimeBoundary::run_one` を追加した。
- wait path と heartbeat send -> ack receive -> ClientStats return send path の単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- actual sleep / timer integration
- socket timeout application
- retry execution / reconnect
- JSON Lines writer invocation / file sink open / process-wide logger
- shutdown cleanup / final flush
- video / switcher 側接続

### 次にやる候補
- client one-tick runtime の CLI / config 接続範囲を整理する。
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。

### TODO 更新
- 現在位置に client one-tick minimal runtime 境界の完了を反映した。
- 直近でやることを client one-tick runtime の CLI / config 接続範囲整理へ更新した。
- heartbeat / client / 検証タスクに `ClientHeartbeatLoopOneTickRuntimeBoundary` と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_one_tick_runtime`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 担当
- Codex

### 今回の作業
- outer repeated loop controller / shutdown apply から future completed loop lifecycle へどう進むかの最小範囲を整理した。
- caller の継続要求と 1 step 結果から、continue / stop / cleanup 開始要否を決める lifecycle 境界を追加した。
- launcher ownership / repeated loop body / outer controller / shutdown apply / future completed loop lifecycle の責務分離を docs に追記した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopLifecycleStopReason`
- `ClientHeartbeatLoopLifecycleInput`
- `ClientHeartbeatLoopLifecycleResult`
- `ClientHeartbeatLoopLifecycleBoundary`
- lifecycle continue path / caller-stop path の単体テストを追加

### 未実装 / 保留
- completed continuous heartbeat loop
- actual timer / retry / cleanup sequencing
- reconnect / shutdown cleanup / log writer invocation
- process lifetime control

### 次にやる候補
- future completed loop lifecycle から actual timer / retry / cleanup sequencing へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新
- 現在位置に future completed loop lifecycle の最小境界完了を反映した。
- 直近でやることを actual timer / retry / cleanup sequencing へ進む最小範囲整理へ更新した。
- client / 検証タスクに lifecycle 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_lifecycle`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 担当
- Codex

### 今回の作業
- future repeated loop body から outer repeated loop controller / shutdown apply をどう呼ぶかの最小範囲を整理した。
- repeated body の結果を outer controller が観測し、shutdown apply が typed result を返すだけの 1 step 境界を追加した。
- launcher ownership / repeated loop body / outer controller / shutdown apply / future completed loop の責務分離を docs に追記した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopOuterControllerAction`
- `ClientHeartbeatLoopOuterControllerResult`
- `ClientHeartbeatLoopOuterControllerBoundary`
- `ClientHeartbeatLoopShutdownApplyResult`
- `ClientHeartbeatLoopShutdownApplyBoundary`
- `ClientHeartbeatLoopRepeatedRuntimeLoopStepResult`
- `ClientHeartbeatLoopRepeatedRuntimeLoopStepBoundary`
- outer controller continue path と loop step stop path の単体テストを追加

### 未実装 / 保留
- completed continuous heartbeat loop
- future completed loop lifecycle 本体
- 実 sleep / timer / retry / reconnect
- shutdown cleanup / log writer invocation / process lifetime control

### 次にやる候補
- outer repeated loop controller / shutdown apply から future completed loop lifecycle へ進む最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新
- 現在位置に outer repeated loop controller / shutdown apply の最小境界完了を反映した。
- 直近でやることを future completed loop lifecycle へ進む最小範囲整理へ更新した。
- client / 検証タスクに outer controller / shutdown apply 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_outer_controller`
- `cargo test -p stream-sync-client client_heartbeat_loop_repeated_runtime_loop_step`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 担当
- Codex

### 今回の作業
- client one-tick runtime から future repeated loop body をどう呼ぶかの最小範囲を整理した。
- future repeated loop body が持つ動的入力と、one-tick runtime に委譲する 1 回分の bridge を追加した。
- launcher ownership / one-tick runtime / repeated-loop body / shutdown responsibility の責務分離を docs に追記した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopRepeatedRuntimeBodyInput`
- `ClientHeartbeatLoopRepeatedRuntimeBodyResult`
- `ClientHeartbeatLoopRepeatedRuntimeBodyBoundary`
- launcher が repeated-loop handoff を作り、repeated body がその handoff と
  dynamic per-step input から one-tick runtime を 1 回呼ぶ接続へ更新
- repeated loop body wait path / stop path の単体テストを追加

### 未実装 / 保留
- completed continuous heartbeat loop
- outer repeated loop controller / shutdown apply 本体
- 実 sleep / timer / retry execution
- reconnect / shutdown cleanup / log writer invocation

### 次にやる候補
- future repeated loop body から outer repeated loop controller / shutdown apply を呼ぶ最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新
- 現在位置に future repeated loop body の最小境界完了を反映した。
- 直近でやることを outer repeated loop controller / shutdown apply の最小範囲整理へ更新した。
- client / 検証タスクに repeated loop body 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_repeated_runtime_body`
- `cargo test -p stream-sync-client client_heartbeat_one_tick_runtime_launcher`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 担当
- Codex

### 今回の作業
- client one-tick runtime の launcher / repeated-loop ownership 方針を整理した。
- continuous heartbeat loop 本体へ進む前に、launcher が持つ責務と future repeated loop が持つ責務の境界を固定した。
- docs に config load / socket ownership / one-tick runtime / future repeated loop / shutdown responsibility の責務分離を追記した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatLoopRepeatedRuntimeHandoff`
- `ClientHeartbeatLoopLauncherOwnershipInput`
- `ClientHeartbeatLoopLauncherOwnershipResult`
- `ClientHeartbeatLoopLauncherOwnershipBoundary`
- `ClientHeartbeatLoopRepeatedRuntimeHandoff::build_one_tick_input(...)`
- `ClientHeartbeatOneTickRuntimeOutcome` に repeated-loop handoff を追加
- one-tick launcher が accepted auth 後に ownership boundary を通し、その handoff から one-tick input を組み立てるように接続
- launcher ownership boundary / repeated-loop handoff の単体テストを追加

### 未実装 / 保留
- completed continuous heartbeat loop
- future repeated loop body 本体
- 実 sleep / timer / retry execution
- reconnect / shutdown cleanup / log writer invocation

### 次にやる候補
- client one-tick runtime から future repeated loop body を呼ぶ最小範囲整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新
- 現在位置に client launcher / repeated-loop ownership 境界完了を反映した。
- 直近でやることを future repeated loop body を呼ぶ最小範囲整理へ更新した。
- client / 検証タスクに launcher / repeated-loop ownership 境界と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_launcher_ownership`
- `cargo test -p stream-sync-client client_heartbeat_one_tick_runtime_launcher`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 担当
- Codex

### 今回の作業
- client one-tick heartbeat runtime の accepted path を実機手動確認した。
- `--receive-send-twice` と `--auth-heartbeat-one-tick-runtime` の組み合わせを確認した。
- `--receive-send-three` と `--auth-heartbeat-stats-one-tick-runtime` の組み合わせも確認した。
- stdout / stderr の要点を manual check docs と operations docs に反映した。

### 変更ファイル
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実施結果
- `target/debug/stream-sync-server.exe --receive-send-twice configs/examples/server.example.toml`
  と
  `target/debug/stream-sync-client.exe --auth-heartbeat-one-tick-runtime configs/examples/client.accepted.example.toml`
  の accepted path は成功した。
- client stdout では `accepted=true`, `controller_action=SendHeartbeat`,
  `shutdown=Continue`, `sent_heartbeats=1`, `received_acks=1`,
  `stats_returns_sent=0` を確認した。
- server stdout では `first_sent_bytes=55`, `second_sent_bytes=73`,
  `registered_clients=1`, `heartbeat_liveness_entries=1` を確認した。
- server stderr では accepted `AuthRequest` -> accepted `Heartbeat` ->
  `HeartbeatAck` send の JSON Lines を確認した。
- `target/debug/stream-sync-server.exe --receive-send-three configs/examples/server.example.toml`
  と
  `target/debug/stream-sync-client.exe --auth-heartbeat-stats-one-tick-runtime configs/examples/client.accepted.example.toml`
  の accepted path も成功した。
- client stdout では `stats_returns_sent=1` まで進み、`ClientStats 106 bytes`
  の observation return を確認した。
- server stdout では `third_sent_bytes=0`, `heartbeat_rtt_offset_entries=1`,
  `heartbeat_rtt_offset_samples=1`, `heartbeat_rtt_micros=117646`,
  `heartbeat_clock_offset_micros=41535` を確認した。
- server stderr では accepted `ClientStats` 受信までの JSON Lines を確認した。

### 未実装 / 保留
- launcher / repeated-loop ownership 方針の明文化
- completed continuous heartbeat loop
- 実 sleep / timer / retry execution
- JSON Lines writer invocation / shutdown cleanup

### 次にやる候補
- client one-tick runtime の launcher / repeated-loop ownership 方針整理
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot export cadence / dashboard refresh 方針整理

### TODO 更新
- 現在位置に one-tick runtime accepted path 手動確認の成功結果を反映した。
- 直近でやることから accepted path manual check を外し、launcher /
  repeated-loop ownership 方針整理へ更新した。

### 検証
- `cargo build -p stream-sync-server -p stream-sync-client`
- `target/debug/stream-sync-server.exe --receive-send-twice configs/examples/server.example.toml`
- `target/debug/stream-sync-client.exe --auth-heartbeat-one-tick-runtime configs/examples/client.accepted.example.toml`
- `target/debug/stream-sync-server.exe --receive-send-three configs/examples/server.example.toml`
- `target/debug/stream-sync-client.exe --auth-heartbeat-stats-one-tick-runtime configs/examples/client.accepted.example.toml`

---

## 2026-04-23
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop 本体へ進む前の client loop logging / shutdown integration 接続範囲を整理した。
- client controller plan から typed log handoff、shutdown decision、controller result へ変換する最小境界を追加した。
- heartbeat policy / encode-send / ack receive / stats return / counters update / sleep-retry / logging / shutdown / future loop body の責務分離を architecture docs に反映した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- client loop logging は、現段階では `ClientHeartbeatLoopControllerLogHandoffBoundary` が typed handoff を作るところまでに限定する。
- shutdown integration は、現段階では `ClientHeartbeatLoopShutdownDecisionBoundary` が controller `Stop` plan を `Stop` decision に変換するところまでに限定する。
- `OwnershipNotReady` は今回の client loop log handoff 対象外とし、将来の startup / precondition failure 側で扱う余地を残す。
- JSON Lines writer、file sink open、process-wide logger、実 shutdown、実 sleep、retry execution、continuous loop 本体は今回の対象外に残す。

### 実装したこと
- `ClientHeartbeatLoopControllerAction` を追加した。
- `ClientHeartbeatLoopControllerLogHandoff` / `ClientHeartbeatLoopControllerLogHandoffBoundary` を追加した。
- `ClientHeartbeatLoopShutdownDecision` / `ClientHeartbeatLoopShutdownDecisionBoundary` を追加した。
- `ClientHeartbeatLoopControllerResult` / `ClientHeartbeatLoopControllerResultBoundary` を追加した。
- controller result の stop / send / ownership-not-ready の単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- JSON Lines event schema / caller-owned writer / sink 接続
- actual shutdown execution / final flush / resource cleanup
- actual sleep / timer integration
- retry execution / socket timeout application
- stats metrics state commit

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- client 側 continuous heartbeat loop 本体の最小実装範囲を整理する。

### TODO 更新
- 現在位置に client loop logging / shutdown integration 境界の完了を反映した。
- 直近でやることから client loop logging / shutdown integration 整理を外し、client 側 continuous heartbeat loop 本体の最小実装範囲整理へ更新した。
- heartbeat / client / 検証タスクに `ClientHeartbeatLoopControllerResultBoundary` と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_heartbeat_loop_controller_result`
- `cargo check --workspace`

---

## 2026-04-23
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop 本体へ進む前の client loop controller / retry execution / sleep integration 接続範囲を整理した。
- `ClientHeartbeatLoopBodyResult` を send handoff / sleep plan / stop result へ変換する最小 controller 境界を追加した。
- retry decision を failure iteration result と bounded sleep decision へ接続する最小 retry apply 境界を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- controller 境界は body result を次 step plan へ変換するだけにし、socket I/O、実 sleep、retry 実行、loop 実行は行わない。
- wait は `ClientHeartbeatLoopSleepDecision` と `Waited` iteration result に分ける。
- retry apply は classified failure を `Failed` iteration result と retry / sleep plan へ分ける。
- counters mutation は引き続き `ClientHeartbeatLoopCountersBoundary` にだけ置く。
- `SleepBoundary` は wake timestamp と max sleep duration から bounded sleep を返すだけにする。

### 実装したこと
- `ClientHeartbeatLoopSleepReason` を追加した。
- `ClientHeartbeatLoopSleepInput` を追加した。
- `ClientHeartbeatLoopSleepDecision` を追加した。
- `ClientHeartbeatLoopSleepBoundary::plan_sleep` を追加した。
- `ClientHeartbeatLoopRetryApplyInput` / `ClientHeartbeatLoopRetryApplyResult` を追加した。
- `ClientHeartbeatLoopRetryApplyBoundary::apply_failure` を追加した。
- `ClientHeartbeatLoopControllerInput` / `ClientHeartbeatLoopControllerPlan` を追加した。
- `ClientHeartbeatLoopControllerBoundary::plan_next` を追加した。
- sleep clamp、retry sleep、retry exhausted、controller wait plan の単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- actual retry execution
- actual sleep / timer integration
- socket timeout application
- client loop logging
- shutdown integration
- timeout notice wakeup 実行本体
- metrics snapshot の具体的な export cadence / dashboard refresh

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- continuous heartbeat loop 本体へ進む前の client loop logging / shutdown integration 接続範囲を整理する。

### TODO 更新
- 現在位置に client loop controller / retry apply / sleep decision 境界の完了を反映した。
- 直近でやることを timeout notice wakeup 実行本体前の境界整理、metrics snapshot cadence / dashboard refresh 方針、client loop logging / shutdown integration 接続範囲整理へ更新した。
- heartbeat / client / 検証タスクに controller / retry apply / sleep decision boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_sleep`
- `cargo test -p stream-sync-client client_heartbeat_loop_retry_apply`
- `cargo test -p stream-sync-client client_heartbeat_loop_controller`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop 本体へ進む前の client loop iteration result / counters 接続範囲を整理した。
- heartbeat send / ack receive / observation return / ClientStats send の各 step 結果を、client-local counters state に反映する最小境界を追加した。
- counters は future loop body の実行順序を決めず、成功または分類済み failure を受けて状態を更新するだけにした。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ClientHeartbeatLoopIterationRuntimeResult` は future loop body が各 step 実行後に emit する runtime-shaped result とする。
- `ClientHeartbeatLoopCountersState` は sent heartbeat / received ack / missed ack / stats return sent / step failure counters と last timestamp を保持する。
- `ClientHeartbeatLoopCountersBoundary` は caller-owned counters へ 1 result だけ commit する。
- policy へ戻す情報は `as_policy_snapshot` で `ClientHeartbeatLoopStateSnapshot` に絞る。
- Ack receive failure は failure counter として扱い、missed ack は `AckMissed` の明示 result でだけ増やす。

### 実装したこと
- `ClientHeartbeatLoopCountersState` を追加した。
- `ClientHeartbeatLoopIterationFailureKind` を追加した。
- `ClientHeartbeatLoopIterationRuntimeResult` を追加した。
- 既存 send / ack / stats return runtime result から iteration result を作る helper を追加した。
- `ClientHeartbeatLoopCountersUpdateOutcome` を追加した。
- `ClientHeartbeatLoopCountersBoundary::commit_result` を追加した。
- counters update と policy snapshot の単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- loop controller / iteration orchestration
- retry execution / backoff / sleep integration
- log output handoff for client loop counters
- shutdown integration
- metrics snapshot の具体的な export cadence / dashboard refresh
- timeout notice wakeup 実行本体

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- continuous heartbeat loop 本体へ進む前の client loop controller / retry execution / sleep integration 接続範囲を整理する。

### TODO 更新
- 現在位置に client loop iteration result / counters 境界の完了を反映した。
- 直近でやることを timeout notice wakeup 実行本体前の境界整理、metrics snapshot cadence / dashboard refresh 方針、client loop controller / retry execution / sleep integration 接続範囲整理へ更新した。
- heartbeat / client / 検証タスクに `ClientHeartbeatLoopCountersBoundary` と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_counters`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop 本体へ進む前の client stats return send handoff 接続範囲を整理した。
- ack observation return 境界が作った encoded `ClientStats` handoff を caller-owned UDP socket へ 1 回送る最小境界を追加した。
- `ClientStats` encode は既存 ack observation return 境界に残し、今回の send 境界では送信のみを担当する形にした。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ClientStats` encode は `ClientHeartbeatLoopAckObservationReturnBoundary` が担当する。
- `ClientHeartbeatLoopClientStatsReturnSendBoundary` は encoded bytes を 1 回 `send_to` するだけにする。
- send result は元の handoff と sent byte count を保持する。
- send error の retry execution、loop counter 更新、sleep / shutdown integration は future loop body に残す。

### 実装したこと
- `ClientHeartbeatLoopClientStatsReturnSendRuntimeResult` を追加した。
- `ClientHeartbeatLoopClientStatsReturnSendError` を追加した。
- `ClientHeartbeatLoopClientStatsReturnSendBoundary::send_one` を追加した。
- encoded `ClientStats` return handoff を 1 UDP datagram として送信し、受信側で decode できる単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- send error retry execution / backoff
- loop counters / missed ack counters / stats-return sent counters の更新
- sleep / timer / shutdown integration
- metrics snapshot の具体的な export cadence / dashboard refresh
- timeout notice wakeup 実行本体

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- continuous heartbeat loop 本体へ進む前の client loop iteration result / counters 接続範囲を整理する。

### TODO 更新
- 現在位置に client stats return send handoff 境界の完了を反映した。
- 直近でやることを timeout notice wakeup 実行本体前の境界整理、metrics snapshot cadence / dashboard refresh 方針、client loop iteration result / counters 接続範囲整理へ更新した。
- heartbeat / client / 検証タスクに `ClientHeartbeatLoopClientStatsReturnSendBoundary` と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_client_stats_return_send`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop 本体へ進む前の client ack receive / observation return 接続範囲を整理した。
- 送信済み heartbeat handoff から `HeartbeatAck` receive / decode / correlation check / `HeartbeatAckObservation` build へつなぐ最小境界を追加した。
- observation return mode が `ClientStatsOncePerAck` の場合に、返送用 `ClientStats` datagram を encode する handoff を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- ack receive / decode は caller-owned `UdpSocket` から 1 回だけ受信する境界に留める。
- ack correlation は `client_id` / `run_id` / `echoed_sent_at` で確認する。
- `HeartbeatAckObservation` build は既存 `ClientHeartbeatAckObservationBoundary` に委譲する。
- `ClientStats` return は typed handoff と encoded bytes を作るだけにし、実 UDP send は次段に残す。
- retry、sleep、socket timeout 設定、loop counter 更新、shutdown integration は future loop body に残す。

### 実装したこと
- `ClientHeartbeatLoopAckObservationReturnInput` を追加した。
- `ClientHeartbeatLoopClientStatsReturnHandoff` を追加した。
- `ClientHeartbeatLoopAckObservationReturnRuntimeResult` を追加した。
- `ClientHeartbeatLoopAckObservationReturnError` を追加した。
- `ClientHeartbeatLoopAckObservationReturnBoundary::receive_one` を追加した。
- `ClientHeartbeatLoopAckObservationReturnBoundary::prepare_return` を追加した。
- ack observation から `ClientStats` return handoff を作る単体テストを追加した。
- caller-owned UDP socket から `HeartbeatAck` を 1 回受信する単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- `ClientStats` return datagram の UDP send 接続
- ack receive timeout / retry execution の実接続
- loop counters / missed ack counters の更新
- sleep / timer / shutdown integration
- metrics snapshot の具体的な export cadence / dashboard refresh
- timeout notice wakeup 実行本体

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- continuous heartbeat loop 本体へ進む前の client stats return send handoff 接続範囲を整理する。

### TODO 更新
- 現在位置に client ack receive / observation return handoff 境界の完了を反映した。
- 直近でやることを timeout notice wakeup 実行本体前の境界整理、metrics snapshot cadence / dashboard refresh 方針、client stats return send handoff 接続範囲整理へ更新した。
- heartbeat / client / 検証タスクに `ClientHeartbeatLoopAckObservationReturnBoundary` と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_ack_return`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop 本体へ進む前の client heartbeat encode/send handoff 接続範囲を整理した。
- `ClientHeartbeatLoopBodySendHandoff` から `Heartbeat` build / protocol encode / 1 回の UDP send へつなぐ最小境界を追加した。
- ack wait / observation return は handoff に保持し、`HeartbeatAck` receive / `ClientStats` return / retry 実行には進まなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- heartbeat build は `ClientHeartbeatLoopEncodeSendBoundary` が担当する。
- protocol encode は既存 `ProtocolMessageEncoderBoundary` に委譲し、client 境界は `ProtocolMessage::Heartbeat` の選択だけを担当する。
- UDP send は caller-owned `UdpSocket` に対して 1 回 `send_to` するだけに留める。
- ack wait decision、ack deadline、observation return mode は encode/send result に保持し、次段の future loop body へ渡す。
- ack receive、observation 生成、`ClientStats` 返送、retry execution、sleep / shutdown integration は future work に残す。

### 実装したこと
- `ClientHeartbeatLoopEncodeSendInput` を追加した。
- `ClientHeartbeatLoopEncodedSendHandoff` を追加した。
- `ClientHeartbeatLoopEncodeSendRuntimeResult` を追加した。
- `ClientHeartbeatLoopEncodeSendError` を追加した。
- `ClientHeartbeatLoopEncodeSendBoundary::encode_handoff` を追加した。
- `ClientHeartbeatLoopEncodeSendBoundary::send_one` を追加した。
- heartbeat encode と 1 UDP datagram send の単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- ack receive / decode の loop body 接続
- `HeartbeatAckObservation` 生成と `ClientStats` 継続返送
- retry execution / backoff / requeue
- sleep / timer / shutdown integration
- metrics snapshot の具体的な export cadence / dashboard refresh
- timeout notice wakeup 実行本体

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- continuous heartbeat loop 本体へ進む前の client ack receive / observation return 接続範囲を整理する。

### TODO 更新
- 現在位置に client heartbeat encode/send handoff 境界の完了を反映した。
- 直近でやることを timeout notice wakeup 実行本体前の境界整理、metrics snapshot cadence / dashboard refresh 方針、client ack receive / observation return 接続範囲整理へ更新した。
- heartbeat / client / 検証タスクに `ClientHeartbeatLoopEncodeSendBoundary` と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_encode_send`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-23
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop 本体へ進む前の 1 iteration body 接続範囲を整理した。
- client 側に auth precondition / send cadence / ack wait timeout を束ねる `ClientHeartbeatLoopBodyBoundary` を追加した。
- server 側に ownership / cadence / socket wait / timeout tick handoff / metrics snapshot handoff を束ねる `ServerHeartbeatContinuousLoopBodyBoundary` を追加した。
- completed continuous heartbeat loop 本体、実 socket I/O、実 state mutation、retry 実行には進まなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- client 1 iteration body は ownership readiness を先に確認し、未成立なら body work に進まない。
- client `SendHeartbeat` decision は `ClientHeartbeatLoopBodySendHandoff` に変換し、実際の `Heartbeat` 構築 / encode / UDP send は future body work に残す。
- client body は ack wait timeout decision と observation return mode だけを handoff に載せる。
- server 1 iteration body は ownership readiness を先に確認し、policy の `Wait` を socket receive timeout decision に変換する。
- server `Run` decision は timeout tick handoff と metrics snapshot handoff だけを作り、client iteration / timeout apply / metrics export 実行は future body work に残す。

### 実装したこと
- `ClientHeartbeatLoopBodyInput` を追加した。
- `ClientHeartbeatLoopBodySendHandoff` を追加した。
- `ClientHeartbeatLoopBodyResult` を追加した。
- `ClientHeartbeatLoopBodyBoundary::run_one` を追加した。
- `ServerHeartbeatContinuousLoopBodyInput` を追加した。
- `ServerHeartbeatContinuousLoopTimeoutTickHandoff` / `ServerHeartbeatContinuousLoopMetricsSnapshotHandoff` を追加した。
- `ServerHeartbeatContinuousLoopBodyHandoff` / `ServerHeartbeatContinuousLoopBodyResult` を追加した。
- `ServerHeartbeatContinuousLoopBodyBoundary::run_one` を追加した。
- client / server の one-iteration body boundary 単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- `Heartbeat` 構築 / protocol encode / UDP send の loop body 接続
- `HeartbeatAck` receive / decode の loop body 接続
- `HeartbeatAckObservation` 生成と `ClientStats` 継続返送
- server 側の複数 client iteration
- `ServerHeartbeatTimeoutLoopTickBoundary` の実呼び出し
- metrics snapshot export / consumer routing の実呼び出し
- retry execution / sleep / shutdown integration

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- continuous heartbeat loop 本体へ進む前の client heartbeat encode/send handoff 接続範囲を整理する。

### TODO 更新
- 現在位置に continuous heartbeat loop one-iteration body 境界の完了を反映した。
- 直近でやることを timeout notice wakeup 実行本体前の境界整理、metrics snapshot cadence / dashboard refresh 方針、client heartbeat encode/send handoff 接続範囲整理へ更新した。
- heartbeat / client / 検証タスクに one-iteration body boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_body`
- `cargo test -p stream-sync-server heartbeat_continuous_loop_body`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop 本体へ進む前の state ownership / socket receive timeout / retry 範囲を整理した。
- client 側に heartbeat loop ownership、ack receive timeout、retry policy placeholder の最小境界を追加した。
- server 側に heartbeat continuous loop ownership、socket receive timeout、retry policy placeholder の最小境界を追加した。
- completed continuous heartbeat loop 本体、実 socket 操作、retry 実行、sleep / timer には進まなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- client loop は accepted auth と bound UDP socket がそろった後に、UDP socket use / loop state / ack wait / stats return を所有する想定にする。
- client ack receive timeout は ack deadline と max socket wait の小さい方へ clamp する。
- server loop は authenticated sender registry、liveness state、outbound queue、timeout log writer、rejected-candidate metrics state を caller-owned holder として受け取る想定にする。
- server socket receive timeout は次の heartbeat work due と max socket receive wait の小さい方へ clamp し、timeout tick / metrics handoff を blocking receive で遅らせない。
- retry boundary は `RetryLater` / `GiveUp` の decision だけを返し、sleep、再送、requeue、再実行は future loop body に残す。

### 実装したこと
- `ClientHeartbeatLoopOwnershipBoundary` を追加した。
- `ClientHeartbeatAckReceiveTimeoutBoundary` を追加した。
- `ClientHeartbeatLoopRetryBoundary` を追加した。
- `ServerHeartbeatContinuousLoopOwnershipBoundary` を追加した。
- `ServerHeartbeatContinuousLoopSocketReceiveTimeoutBoundary` を追加した。
- `ServerHeartbeatContinuousLoopRetryBoundary` を追加した。
- ownership / timeout / retry の client / server 単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- 実際の `UdpSocket` 所有移譲 / socket 設定 / receive 呼び出し
- 実際の sleep / timer / retry execution
- heartbeat packet 継続送信
- ack observation の継続的な `ClientStats` 返送
- 複数 client timeout scan
- timeout notice wakeup 実行本体
- metrics snapshot の具体的な export cadence / dashboard refresh

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- continuous heartbeat loop 本体へ進む前の 1 iteration body 接続範囲を整理する。

### TODO 更新
- 現在位置に continuous heartbeat loop ownership / socket receive timeout / retry 境界の完了を反映した。
- 直近でやることを timeout notice wakeup 実行本体前の境界整理、metrics snapshot の cadence / dashboard refresh 方針、continuous heartbeat loop 1 iteration body 接続範囲整理へ更新した。
- heartbeat / client / 検証タスクに ownership / timeout / retry boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client`
- `cargo test -p stream-sync-server heartbeat_continuous_loop`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理した。
- client 側 heartbeat send cadence / ack observation return / stop / log handoff の最小 policy 境界を追加した。
- server 側 timeout tick / metrics snapshot handoff cadence / stop / log handoff の最小 policy 境界を追加した。
- completed continuous heartbeat loop 本体、実際の sleep / socket I/O / retry / wakeup 実行には進まなかった。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- client 側 continuous heartbeat loop の事前境界は `Stop` / `Wait` / `SendHeartbeat` の decision だけを返す。
- client 側の ack observation return は `ClientHeartbeatAckObservationReturnMode` として policy decision に載せるが、実際の `HeartbeatAckObservation` 生成と `ClientStats` 送信は既存境界と future loop body に残す。
- server 側 continuous heartbeat loop の事前境界は timeout tick と metrics snapshot export の due 判定だけを返す。
- timeout evaluation / action apply / notice queue storage / metrics snapshot handoff は既存の個別境界に残し、policy boundary からは直接実行しない。
- log は caller-owned writer へ渡す前の typed handoff だけを作る。JSON Lines event schema / file sink / process-wide logger は future work に残す。

### 実装したこと
- `ClientHeartbeatLoopCadenceInput` を追加した。
- `ClientHeartbeatLoopStopCondition` / `ClientHeartbeatLoopPolicyAction` / `ClientHeartbeatLoopLogHandoff` を追加した。
- `ClientHeartbeatLoopPolicyBoundary::evaluate` を追加した。
- `ServerHeartbeatContinuousLoopCadenceInput` を追加した。
- `ServerHeartbeatContinuousLoopStopCondition` / `ServerHeartbeatContinuousLoopPolicyAction` / `ServerHeartbeatContinuousLoopLogHandoff` を追加した。
- `ServerHeartbeatContinuousLoopPolicyBoundary::evaluate` を追加した。
- client / server の policy boundary 単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- 実際の heartbeat packet 継続送信
- 実際の sleep / timer / socket receive timeout / retry
- ack observation の継続的な `ClientStats` 返送
- 複数 client の timeout scan
- timeout notice wakeup 実行本体
- metrics snapshot の具体的な export cadence / dashboard refresh
- JSON Lines event schema / writer runtime / file sink

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。
- continuous heartbeat loop 本体へ進む前の state ownership / socket receive timeout / retry 範囲を整理する。

### TODO 更新
- 現在位置に continuous heartbeat loop preflight policy 境界の完了を反映した。
- 直近でやることを timeout notice wakeup 実行本体前の境界整理、metrics snapshot の export cadence / dashboard refresh 方針、continuous loop 本体前の state ownership / timeout / retry 整理へ更新した。
- heartbeat / client / 検証タスクに client/server policy boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_loop_policy`
- `cargo test -p stream-sync-server heartbeat_continuous_loop_policy`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- RTT / offset metrics snapshot を future loop / dashboard へどう連携するかを整理した。
- rejected candidate metrics snapshot を future loop / dashboard consumer へ渡す export handoff の最小境界を追加した。
- dashboard 本体や completed metrics pipeline には進まず、consumer placeholder の型だけを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- metrics state は caller-owned in-memory aggregation のままにする。
- snapshot export は現在の state を immutable record snapshot に変換するだけにする。
- export handoff は consumer と `exported_at` を付けて future loop / dashboard へ渡す型だけを担当する。
- empty snapshot は `NoRecords` として扱い、空 dashboard update や loop event は作らない。
- dashboard consumer は input shape だけを受け取り、UI rendering / refresh transport / storage は future work に残す。

### 実装したこと
- `ServerHeartbeatRttOffsetMetricsSnapshotConsumer` を追加した。
- `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoff` を追加した。
- `ServerHeartbeatRttOffsetMetricsSnapshotExportRuntimeResult` を追加した。
- `ServerHeartbeatRttOffsetMetricsSnapshotExportHandoffBoundary::export_for_consumer` を追加した。
- `ServerHeartbeatRttOffsetMetricsDashboardSnapshotInput` を追加した。
- `ServerHeartbeatRttOffsetMetricsSnapshotConsumerBoundary::consume` を追加した。
- empty snapshot、dashboard consumer、future loop consumer の単体テストを追加した。

### 未実装 / 保留
- completed metrics pipeline
- dashboard 本体
- dashboard refresh transport / storage
- export cadence / retention / time-series history
- JSON / file / network export
- continuous heartbeat loop からの定期呼び出し

### 次にやる候補
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。
- RTT / offset metrics snapshot の具体的な export cadence / dashboard refresh 方針を整理する。

### TODO 更新
- 現在位置に RTT / offset metrics snapshot loop / dashboard handoff 境界の完了を反映した。
- 直近でやることを continuous heartbeat loop 前の境界整理、timeout notice wakeup 実行本体前の境界整理、metrics snapshot の export cadence / dashboard refresh 方針へ更新した。
- heartbeat / 検証タスクに metrics snapshot loop / dashboard handoff boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_rtt_offset_metrics_snapshot`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- heartbeat timeout loop tick の notice queue storage / send wakeup 方針を整理した。
- timeout apply が作る `AuthExpired` notice handoff を caller-owned outbound queue collection へ保存する最小境界を追加した。
- notice が実際に queue へ保存された場合だけ future send loop wakeup placeholder を返す形にした。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- timeout apply は registry invalidation / timeout log / typed notice handoff 作成までを担当する。
- notice queue storage は apply result を受け取り、`notice_handoff.queue_item` だけを caller-owned queue collection へ保存する。
- send wakeup は `ServerHeartbeatTimeoutNoticeSendWakeupPlan` の typed placeholder に留める。
- wakeup は notice の storage が成功した場合だけ request し、NoNotice / dropped の場合は request しない。
- 実際の wakeup 実行、send loop 起動、encode / UDP send、retry は future work に残す。

### 実装したこと
- `ServerHeartbeatTimeoutNoticeSendWakeupPlan` と wakeup reason を追加した。
- `ServerHeartbeatTimeoutNoticeQueueStorageResult` / Stored / Dropped result 型を追加した。
- `ServerHeartbeatTimeoutNoticeQueueStorageBoundary::store_notice` を追加した。
- timeout notice storage 成功時に `QueuedOutboundItem` を `ServerOutboundQueueCollection` へ push する最小処理を追加した。
- notice storage 成功時と no-notice 時の単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- 実際の send wakeup 通知本体
- send loop scheduling / retry / requeue
- notice duplicate suppression / rate limit
- file sink open / process-wide logger
- 複数 client timeout scan

### 次にやる候補
- RTT / offset metrics snapshot の future loop / dashboard 連携方針を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理を続ける。

### TODO 更新
- 現在位置に heartbeat timeout notice queue storage / send wakeup plan 境界の完了を反映した。
- 直近でやることを RTT / offset metrics snapshot の future loop / dashboard 連携、continuous heartbeat loop 前の境界整理、notice wakeup 実行本体前の境界整理へ更新した。
- heartbeat / 検証タスクに notice queue storage / send wakeup boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_timeout_notice_queue_storage`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- RTT / offset rejected candidate metrics の storage / aggregation / export 方針を整理した。
- rejected candidate handoff が作る metrics counter delta を、caller-owned in-memory state へ集約する最小境界を追加した。
- future exporter / dashboard が読むための snapshot export placeholder を追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- rejected candidate handoff は counter delta を作るだけに留める。
- metrics state は `(client_id, run_id)` ごとに rejected candidate count / skipped commit count / reason-specific count を集約する。
- `run_id` が変わる場合は別 entry とし、過去 run と merge しない。
- export は typed snapshot を作るだけに留め、JSON serialization、file sink、network export、dashboard 表示は future work に残す。
- continuous heartbeat loop は metrics state の所有、snapshot export の呼び出しタイミング、backpressure を後で決める。

### 実装したこと
- `ServerHeartbeatRttOffsetRejectedCandidateMetricsState` と state entry を追加した。
- `ServerHeartbeatRttOffsetRejectedCandidateMetricsCommitBoundary::commit` を追加した。
- `ServerHeartbeatRttOffsetRejectedCandidateMetricsSnapshot` / export record を追加した。
- `ServerHeartbeatRttOffsetRejectedCandidateMetricsExportBoundary::snapshot` を追加した。
- metrics state commit、reason 別 aggregation、snapshot export の単体テストを追加した。

### 未実装 / 保留
- completed metrics pipeline
- metrics snapshot の JSON / file / network export
- process-wide metrics registry
- dashboard / switcher UI 連携
- retention / time-series history
- continuous heartbeat loop からの commit / export 呼び出し

### 次にやる候補
- heartbeat timeout loop tick の notice queue storage / send wakeup 方針を整理する。
- RTT / offset metrics snapshot の future loop / dashboard 連携方針を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に RTT / offset rejected candidate metrics state / snapshot export 境界の完了を反映した。
- 直近でやることを timeout loop tick の notice queue storage / send wakeup、RTT / offset metrics snapshot の future loop / dashboard 連携、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / net-core / 検証タスクに metrics state / snapshot export boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_rtt_offset_rejected_candidate_metrics`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- RTT / offset rejected candidate の log / metrics 方針を整理した。
- policy commit で `Skipped(RejectedOutlier)` になった candidate だけを、後段の log / metrics handoff 入力へ変換する最小境界を追加した。
- accepted candidate / committed candidate では rejected-candidate handoff を発生させない形にした。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- candidate policy reject は outlier 判定と理由だけを担当する。
- state commit skip は latest estimate state を変更しない責務だけを担当する。
- rejected candidate の JSON Lines event と metrics counter delta は、policy commit 後の handoff 境界で作る。
- metrics handoff は counter delta の型だけを持ち、metrics storage / aggregation / export は future work に残す。
- log output は caller-owned writer への 1 record 出力までとし、file sink open / process-wide logger は実装しない。

### 実装したこと
- `ServerHeartbeatRttOffsetRejectedCandidateLogInput` を追加した。
- `ServerHeartbeatRttOffsetRejectedCandidateMetricsHandoff` を追加した。
- `ServerHeartbeatRttOffsetRejectedCandidateHandoffBoundary::prepare` を追加した。
- `server.heartbeat_rtt_offset_rejected_candidate` JSON Lines event boundary / writer / output boundary を追加した。
- rejected candidate handoff、committed candidate no-op、JSON Lines writer の単体テストを追加した。

### 未実装 / 保留
- rejected candidate metrics storage / aggregation / export
- rejected candidate log の continuous loop からの writer 選択
- candidate policy threshold の設定化
- EWMA などの smoothing 本体
- corrected timestamp publish
- continuous heartbeat loop からの継続 observation commit

### 次にやる候補
- heartbeat timeout loop tick の notice queue storage / send wakeup 方針を整理する。
- RTT / offset rejected candidate metrics storage / export 方針を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に RTT / offset rejected candidate log / metrics handoff 境界の完了を反映した。
- 直近でやることを timeout loop tick の notice queue storage / send wakeup、RTT / offset rejected candidate metrics storage / export、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / net-core / 検証タスクに rejected candidate log / metrics handoff boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_rtt_offset_rejected_candidate`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- RTT / offset candidate policy を commit 前に接続した。
- policy で rejected outlier になった candidate を `ServerHeartbeatRttOffsetState` に保存しない最小実装を追加した。
- `--receive-send-three` の RTT / offset commit 経路を policy commit 境界経由に変更した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- RTT / offset の commit 経路は stateless calculation -> candidate policy -> policy commit -> latest estimate state の順にする。
- default candidate policy は threshold 無効なので accepted candidate を従来通り commit する。
- `RejectOutlier` の candidate は commit を skip し、previous latest estimate を保持する。
- rejected candidate の log / metrics は今回は実装せず、次の方針整理候補に残す。
- smoothing / corrected timestamp publish は今回も future work に残す。

### 実装したこと
- `ServerHeartbeatRttOffsetCommitSkipReason` を追加した。
- `ServerHeartbeatRttOffsetPolicyCommitResult` と `ServerHeartbeatRttOffsetPolicyCommitOutcome` を追加した。
- `ServerHeartbeatRttOffsetPolicyCommitBoundary::evaluate_and_commit` を追加した。
- `ServerReceiveSendThreeIterationLauncher` で RTT / offset commit を policy commit 境界経由に変更した。
- accepted candidate が commit される単体テストを追加した。
- rejected candidate が state を変えず skip される単体テストを追加した。

### 未実装 / 保留
- rejected candidate の log / metrics
- candidate policy の設定化
- EWMA などの smoothing 本体
- outlier history / confidence / warm-up
- corrected timestamp publish
- sync-core / targetTime への接続
- continuous heartbeat loop からの継続 observation commit

### 次にやる候補
- heartbeat timeout loop tick の notice queue storage / send wakeup 方針を整理する。
- RTT / offset rejected candidate log / metrics 方針を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に RTT / offset policy commit 境界と rejected candidate skip の完了を反映した。
- 直近でやることを timeout loop tick の notice queue storage / send wakeup、RTT / offset rejected candidate log / metrics、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / net-core / 検証タスクに RTT / offset policy commit boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_rtt_offset_policy_commit`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- RTT / offset smoothing / outlier policy の最小範囲を整理した。
- completed smoothing には進めず、latest estimate commit 前に置ける candidate policy 境界を追加した。
- optional same-run delta threshold による outlier reject と、smoothing deferred の decision shape を追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- stateless calculator は previous estimate を見ず、1 exchange の numeric candidate だけを作る。
- candidate policy boundary は latest same-run estimate との差分だけを見る。履歴、confidence、EWMA、補正 timestamp 公開はまだ持たない。
- default policy は threshold 無効で candidate を accept し、smoothing は `Deferred` として返す。
- `run_id` が変わった candidate は cross-run outlier comparison をせず accept し、sample count reset は commit boundary 側に任せる。
- latest estimate commit は accepted candidate を保存する責務に留め、outlier 判定や smoothing は行わない。

### 実装したこと
- `ServerHeartbeatRttOffsetSmoothingMode` を追加した。
- `ServerHeartbeatRttOffsetOutlierPolicy` と `ServerHeartbeatRttOffsetCandidatePolicy` を追加した。
- `ServerHeartbeatRttOffsetOutlierReason`、`ServerHeartbeatRttOffsetCandidatePolicyDecision`、`ServerHeartbeatRttOffsetCandidatePolicyResult` を追加した。
- `ServerHeartbeatRttOffsetCandidatePolicyBoundary::evaluate` を追加した。
- threshold 無効時の accept、RTT delta reject、clock offset delta reject、new-run accept の単体テストを追加した。

### 未実装 / 保留
- candidate policy を `--receive-send-three` の commit 前へ接続する処理
- EWMA などの smoothing 本体
- outlier history / confidence / warm-up
- corrected timestamp publish
- sync-core / targetTime への接続
- continuous heartbeat loop からの継続 observation commit

### 次にやる候補
- heartbeat timeout loop tick の notice queue storage / send wakeup 方針を整理する。
- RTT / offset candidate policy を commit 前に接続する方針を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に RTT / offset candidate policy 境界の完了を反映した。
- 直近でやることを timeout loop tick の notice queue storage / send wakeup、RTT / offset candidate policy の commit 前接続、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / net-core / 検証タスクに RTT / offset candidate policy boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_rtt_offset_candidate_policy`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- RTT / offset estimate を server 側 state に commit する最小範囲を整理した。
- stateless calculator の結果を per-client latest estimate state に保存する `ServerHeartbeatRttOffsetCommitBoundary` を追加した。
- `--receive-send-three` で returned observation の RTT / offset candidate を 1 回 commit し、stdout に state entry 数と sample count を表示するようにした。

### 変更ファイル
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- stateless calculator は 1 exchange の RTT / offset candidate 算出だけを担当する。
- state commit boundary は latest estimate と same-run sample count の保持だけを担当する。
- 同じ `client_id` で `run_id` が変わった場合は sample count を 1 に戻し、previous run replacement として outcome に残す。
- smoothing、outlier rejection、confidence、history、補正後 timestamp の公開は future estimator state に残す。
- timeout loop は liveness / timeout を担当し、RTT / offset state commit とは分離する。

### 実装したこと
- `ServerHeartbeatRttOffsetStateEntry` と `ServerHeartbeatRttOffsetState` を追加した。
- `ServerHeartbeatRttOffsetCommitInput` と `ServerHeartbeatRttOffsetCommitOutcome` を追加した。
- `ServerHeartbeatRttOffsetCommitBoundary::commit` を追加した。
- `ServerReceiveSendThreeIterationLauncher` で one-shot calculation を state に commit し、outcome に state / commit result を載せた。
- server CLI `--receive-send-three` stdout に `heartbeat_rtt_offset_entries` と `heartbeat_rtt_offset_samples` を追加した。
- first commit、same-run sample increment、new-run reset の単体テストを追加した。

### 未実装 / 保留
- smoothing / outlier policy
- estimate history / confidence
- corrected timestamp を sync-core / targetTime へ公開する処理
- continuous heartbeat loop からの継続 observation commit
- metrics state commit
- video / switcher 側への拡張

### 次にやる候補
- heartbeat timeout loop tick の notice queue storage / send wakeup 方針を整理する。
- RTT / offset smoothing / outlier policy の範囲を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に heartbeat RTT / offset state commit 境界と `--receive-send-three` の commit 表示完了を反映した。
- 直近でやることを timeout loop tick の notice queue storage / send wakeup、RTT / offset smoothing / outlier policy、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / net-core / 検証タスクに RTT / offset state commit boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_rtt_offset_commit`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- timeout evaluation / action plan / apply boundary を future continuous loop からどう呼ぶかを整理した。
- future loop が選んだ 1 client 分だけ timeout evaluation -> action plan -> apply を実行する最小 loop tick 境界を追加した。
- continuous heartbeat loop 本体、client scan、sleep、notice 送信本体、file sink open には進めなかった。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- future loop は client iteration / cadence / stop condition / timeout policy selection を所有する。
- `ServerHeartbeatTimeoutLoopTickBoundary` は caller-selected `client_id` 1 件だけを処理し、timeout evaluation、action planning、apply を順番に呼ぶ。
- `Alive` / `NoHeartbeat` の tick は registry invalidation、timeout log、notice handoff を発生させない。
- `TimedOut` の tick は既存 apply boundary を通じて registry invalidation、caller-owned timeout log writer、typed `AuthExpired` notice handoff まで行う。
- notice queue storage、send-loop wakeup、encode、UDP send、retry、重複抑制、rate limit は future loop / send layer 側へ残す。

### 実装したこと
- `ServerHeartbeatTimeoutLoopTickInput` を追加した。
- `ServerHeartbeatTimeoutLoopTickResult` を追加した。
- `ServerHeartbeatTimeoutLoopTickBoundary::run_one_client` を追加した。
- timed-out client の one-client loop tick が invalidation / timeout log / notice handoff まで進む単体テストを追加した。
- missing client の one-client loop tick が no-op result になる単体テストを追加した。

### 未実装 / 保留
- 複数 client を走査する heartbeat timeout loop 本体
- loop cadence / sleep / stop condition
- timeout policy の設定化
- notice queue item の queue collection storage と send-loop wakeup
- timeout notice の encode / UDP send / retry / duplicate suppression / rate limit
- timeout log file sink open / rotation / process-wide logger
- RTT / offset estimate の durable state commit と smoothing

### 次にやる候補
- RTT / offset estimate を server 側 state に commit する最小境界を整理する。
- heartbeat timeout loop tick の notice queue storage / send wakeup 方針を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に heartbeat timeout one-client loop tick 境界の完了を反映した。
- 直近でやることを RTT / offset state commit、timeout loop tick の notice queue storage / send wakeup、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / net-core / 検証タスクに timeout loop tick boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_timeout_loop_tick`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- timeout action plan を continuous loop からどう実適用するかの方針を整理した。
- future continuous loop が呼べる最小 apply boundary と apply result 型を追加した。
- timeout notice は typed queue item handoff までに留め、送信本体や file sink open / process-wide logger には進めなかった。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- future loop の順序は timeout evaluation -> timeout action plan -> timeout apply boundary とする。
- apply boundary は明示 invalidation command の適用、caller-owned writer への `server.heartbeat_timeout` 1 行出力、`AuthExpired` notice の typed queue item handoff だけを担当する。
- notice は `OutboundQueueItem` 作成までで止め、queue collection への storage、encode、UDP send、retry、重複抑制、rate limit は future work に残す。
- timeout log は caller-owned writer への最小 JSON Lines 出力までで、file sink open、rotation、process-wide logger は future work に残す。

### 実装したこと
- `ServerHeartbeatTimeoutLogOutputBoundary` と `ServerHeartbeatTimeoutJsonLineWriter` を追加した。
- `ServerHeartbeatTimeoutNoticeHandoff` と `ServerHeartbeatTimeoutApplyResult` を追加した。
- `ServerHeartbeatTimeoutApplyBoundary::apply_plan` を追加した。
- timeout apply boundary が `TimedOut` plan で registry invalidation、timeout log write、notice queue item handoff を行う単体テストを追加した。
- `Alive` plan では registry / log / notice に副作用を出さない単体テストを追加した。

### 未実装 / 保留
- continuous heartbeat loop から timeout evaluation / action plan / apply boundary を呼ぶ処理
- notice queue item の queue collection storage
- timeout notice の encode / UDP send / retry / duplicate suppression / rate limit
- timeout log file sink open / rotation / process-wide logger
- reauthentication policy
- RTT / offset estimate の durable state commit と smoothing

### 次にやる候補
- timeout evaluation / action plan / apply boundary を continuous loop から呼ぶ方針を整理する。
- RTT / offset estimate を server 側 state に commit する最小境界を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に heartbeat timeout apply 境界、caller-owned timeout log writer 境界、notice queue item handoff の完了を反映した。
- 直近でやることを timeout evaluation / action plan / apply boundary の continuous loop 呼び出し方針へ更新した。
- heartbeat / net-core / 検証タスクに timeout apply boundary と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_timeout_apply`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- timeout evaluation 結果を auth 失効 / ログ / notice へ接続する方針を整理した。
- `TimedOut` evaluation から auth registry invalidation command、timeout log event input、`AuthExpired` notice plan を作る最小 action boundary を追加した。
- registry invalidation は timeout 判定側で直接決めず、明示 command を `AuthenticatedSenderRegistryBoundary` が適用する形に分離した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- liveness evaluation は `Alive` / `TimedOut` / `NoHeartbeat` の分類だけを担当し、auth 失効、ログ、notice は実行しない。
- `ServerHeartbeatTimeoutActionBoundary` は `TimedOut` かつ liveness entry が残っている場合だけ、後続 action plan を作る。
- auth registry 失効は `AuthenticatedSenderInvalidation` command として表現し、`AuthenticatedSenderRegistryBoundary::invalidate` が明示的に適用する。
- timeout notice は既存 `ServerNoticeTriggerPolicyBoundary` を使い、最小実装では `ServerNoticeTriggerSource::AuthExpired` に写像する。
- timeout log は `server.heartbeat_timeout` event input までを作り、writer / file sink / process-wide logger 接続は future work に残す。

### 実装したこと
- `AuthenticatedSenderInvalidationReason`、`AuthenticatedSenderInvalidation`、`AuthenticatedSenderInvalidationOutcome` を追加した。
- `AuthenticatedSenderRegistryBoundary::invalidate` を追加した。
- `ServerHeartbeatTimeoutLogInput`、`SERVER_HEARTBEAT_TIMEOUT_JSON_LOG_EVENT_NAME`、`ServerHeartbeatTimeoutJsonLogEventInput`、`ServerHeartbeatTimeoutJsonLogEventBoundary` を追加した。
- `ServerHeartbeatTimeoutActionPlan` と `ServerHeartbeatTimeoutActionBoundary` を追加した。
- timeout action plan、Alive / NoHeartbeat no-op、timeout log event、explicit registry invalidation の単体テストを追加した。

### 未実装 / 保留
- continuous heartbeat loop から timeout evaluation / action plan を呼ぶ処理
- timeout action plan の実適用順序制御
- timeout log JSON Lines writer / file sink / process-wide logger 接続
- timeout notice の queue storage / rate limit / duplicate suppression / UDP send
- reauthentication policy
- RTT / offset estimate の durable state commit と smoothing

### 次にやる候補
- timeout action plan を continuous loop から実適用する方針を整理する。
- RTT / offset estimate を server 側 state に commit する最小境界を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に heartbeat timeout action plan 境界、timeout log event 境界、auth invalidation command 境界の完了を反映した。
- 直近でやることを timeout action plan の continuous loop 適用方針、RTT / offset state commit、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / net-core / 検証タスクに timeout action plan と関連単体テストの完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_timeout`
- `cargo test -p stream-sync-server authenticated_sender_registry_boundary_applies_explicit_timeout_invalidation`
- `cargo check --workspace`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- heartbeat timeout / liveness state commit の実装範囲を整理した。
- registered heartbeat から作られた `ServerHeartbeatStateInput` を server 側 `ServerHeartbeatLivenessState` へ 1 回 commit する最小境界を追加した。
- timeout は continuous loop での自動失効ではなく、caller supplied timestamp で 1 client 分を評価する policy boundary として追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `AuthenticatedSenderRegistry` は accepted auth の source binding を保持するだけに留め、heartbeat freshness / count / timeout evaluation は `ServerHeartbeatLivenessState` 側へ分離する。
- `ServerHeartbeatLivenessCommitBoundary::commit` は registered heartbeat observation を in-memory state に保存し、entry を `Alive` として扱う。
- `ServerHeartbeatTimeoutPolicy` / `evaluate_timeout` は `Alive` / `TimedOut` / `NoHeartbeat` を返すだけで、auth registry 失効、notice 送信、ログ出力、packet drop には接続しない。
- `--receive-send-twice` と `--receive-send-three` では preserved heartbeat handoff から liveness state を 1 回だけ commit し、continuous heartbeat loop には進めない。

### 実装したこと
- `ServerHeartbeatLivenessStatus`、`ServerHeartbeatLivenessEntry`、`ServerHeartbeatLivenessState`、`ServerHeartbeatLivenessCommitOutcome` を追加した。
- `ServerHeartbeatTimeoutPolicy` と `ServerHeartbeatTimeoutEvaluation` を追加した。
- `ServerHeartbeatLivenessCommitBoundary` に commit と timeout evaluation を追加した。
- `ServerReceiveSendTwoIterationLauncher` / `ServerReceiveSendThreeIterationLauncher` の outcome に liveness state commit 結果を載せた。
- server CLI の `--receive-send-twice` / `--receive-send-three` stdout に liveness entry 数を追加した。
- heartbeat liveness commit / update / timeout evaluation の単体テストを追加した。

### 未実装 / 保留
- continuous heartbeat loop
- timeout evaluation 結果による auth registry 失効 / 再認証要求
- timeout / disconnect の JSON Lines ログ出力
- timeout notice / ServerNotice 送信 policy
- RTT / offset estimate の durable state commit と smoothing
- video / switcher 側への拡張

### 次にやる候補
- timeout evaluation 結果を auth 失効 / ログ / notice へ接続する方針を整理する。
- RTT / offset estimate を server 側 state に commit する最小境界を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に heartbeat liveness state commit 境界と timeout policy evaluation 境界の完了を反映した。
- 直近でやることを timeout evaluation 結果の失効 / ログ / notice 接続方針、RTT / offset state commit、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / net-core / 検証タスクに liveness commit と timeout evaluation の完了を追加した。

### 検証
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server heartbeat_liveness`
- `cargo check --workspace`
- `git diff --check`

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- `HeartbeatAck` 受信後に client 側で `HeartbeatAckObservation` を作り、`ClientStats` の optional heartbeat observation block に載せて 1 回送信する入口を追加した。
- server 側で returned `ClientStats` から observation を取り出し、直前の heartbeat timebase plan と照合して stateless RTT / offset calculator へ渡す最小接続を追加した。
- auth -> heartbeat -> stats observation return を 3 packet だけ処理する manual check 入口を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- observation return の手動確認入口は continuous loop ではなく、`--auth-heartbeat-stats-poc-once` と `--receive-send-three` の組み合わせに留める。
- client は `HeartbeatAck` 受信直後に `client_received_at` を記録し、`HeartbeatAckObservation` を `ClientStats.heartbeat_observation` に載せる。
- server は 2 回目の heartbeat handoff に含まれる `timebase_plan` と、3 回目の `ClientStats` から得た observation を突き合わせ、1 回だけ stateless calculator を呼ぶ。
- metrics state commit、RTT / offset state commit、smoothing、heartbeat timeout、continuous heartbeat / stats loop は今回も対象外に残す。

### 実装したこと
- `ClientAuthHeartbeatStatsPocLauncher` と `run_auth_heartbeat_stats_poc_once_from_path` を追加した。
- client CLI に `--auth-heartbeat-stats-poc-once` を追加した。
- `ServerHeartbeatObservationReturnBoundary` を追加し、`ServerHeartbeatAckHandoff` + `ServerClientStatsHandlerInput` から `ServerHeartbeatRttOffsetCalculationBoundary` へ接続した。
- `ServerReceiveSendThreeIterationLauncher` と server CLI `--receive-send-three` を追加した。
- client / server の関連単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- continuous stats send loop
- heartbeat timeout / liveness state commit
- RTT / offset estimate の durable state commit と smoothing
- metrics state commit
- video / switcher 側への拡張
- retry / requeue / file sink open / process-wide logger

### 次にやる候補
- heartbeat timeout / liveness state commit の実装範囲を整理する。
- RTT / offset estimate を server 側 state に commit する最小境界を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に `HeartbeatAckObservation` を `ClientStats` で 1 回返す client 入口と、server 側 calculator 接続完了を反映した。
- 直近でやることを timeout / liveness state commit、RTT / offset state commit、continuous heartbeat loop 前の境界整理へ更新した。
- heartbeat / client / 検証タスクに observation return one-shot と関連単体テストを完了として追加した。
- manual check docs に `--receive-send-three` + `--auth-heartbeat-stats-poc-once` 手順を追加した。

### 検証
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo build -p stream-sync-server -p stream-sync-client`
- `cargo test -p stream-sync-client auth_heartbeat_stats`
- `cargo test -p stream-sync-server heartbeat_observation`
- `cargo test -p stream-sync-client`
- `cargo test -p stream-sync-server`
- `target/debug/stream-sync-server.exe --receive-send-three configs/examples/server.example.toml`
- `target/debug/stream-sync-client.exe --auth-heartbeat-stats-poc-once configs/examples/client.accepted.example.toml`
- 手動確認で client stdout に `sent ClientStats 106 bytes with HeartbeatAckObservation` を観測した。
- 手動確認で server stdout に `third_sent_bytes=0`, `registered_clients=1`, `heartbeat_rtt_micros=<value>` を観測した。
- 手動確認で server stderr に `message_type="ClientStats"` の accepted receive log を観測した。

### 補足
- server テスト実行時に `target` artifact 書き込みが `os error 112` で一度失敗したため、承認済みの `cargo clean` で Cargo build artifacts を削除してから再実行した。

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- client 側で accepted auth 後に `Heartbeat` を 1 回だけ送信し、`HeartbeatAck` を 1 回受信して stdout 表示する最小入口を追加した。
- protocol 側に `Heartbeat` encode と `HeartbeatAck` decode を追加し、client が Heartbeat round trip を扱えるようにした。
- server 側の既存 heartbeat ack handoff を one-iteration send path へ渡し、auth-then-heartbeat を 2 iteration で確認できる入口を追加した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `crates/protocol/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/architecture/system-design.md`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- heartbeat の手動確認入口は continuous loop ではなく、`--auth-heartbeat-poc-once` と `--receive-send-twice` の組み合わせに留める。
- `--auth-heartbeat-poc-once` は同じ UDP socket で `AuthRequest` -> `AuthResponse` -> `Heartbeat` -> `HeartbeatAck` の順に 1 回ずつ処理する。
- server 側は `--receive-send-twice` で同じ socket / registry / queue collection を 2 iteration だけ共有し、accepted auth で登録された source からの `Heartbeat` だけを `HeartbeatAck` 送信へ進める。
- heartbeat timeout、continuous heartbeat loop、RTT / offset state commit、video / switcher 連携は今回も対象外に残す。

### 実装したこと
- `encode_heartbeat` / `encode_heartbeat_payload` を追加し、`ProtocolMessageEncoderBoundary` が `ProtocolMessage::Heartbeat` を encode できるようにした。
- `decode_heartbeat_ack_payload` / `HeartbeatAckPayloadDecoder` を追加し、decode dispatch が `HeartbeatAck` を返せるようにした。
- `ClientAuthHeartbeatPocLauncher` と `run_auth_heartbeat_poc_once_from_path` を追加し、client CLI に `--auth-heartbeat-poc-once` を追加した。
- `ServerOutboundQueueCollectionBoundary` が preserved `ServerHeartbeatAckHandoff` を one-item queue に載せられるようにした。
- `ServerReceiveSendTwoIterationLauncher` と server CLI `--receive-send-twice` を追加した。
- protocol / client / server の関連単体テストを追加した。

### 未実装 / 保留
- completed continuous heartbeat loop
- heartbeat timeout / liveness state commit
- RTT / offset 推定結果の durable state commit
- `HeartbeatAckObservation` を client から `ClientStats` に載せて server に返す実送信経路
- video / switcher 側への拡張
- retry / requeue / file sink open / process-wide logger

### 次にやる候補
- `HeartbeatAckObservation` を client 側 `ClientStats` carrier に載せ、server 側 timebase 入力へ返す最小経路を実装する。
- heartbeat timeout / liveness state commit の実装範囲を整理する。
- continuous heartbeat loop に進む前の送信間隔、停止条件、ログ出力範囲を整理する。

### TODO 更新
- 現在位置に `Heartbeat` encode / `HeartbeatAck` decode 完了と client auth-then-heartbeat one-shot 入口完了を反映した。
- heartbeat / client 側タスクで one-shot heartbeat 送信と registered heartbeat -> `HeartbeatAck` one-shot send を完了にした。
- PoC 最小ラインの `client が Heartbeat を送り、server が RTT / offset 推定に使える時刻情報を返せる` を完了にした。
- 直近でやることを heartbeat observation return path、timeout / liveness state commit、continuous heartbeat loop 前の境界整理へ更新した。

### 検証
- `cargo fmt`
- `cargo check --workspace`
- `cargo test -p stream-sync-protocol`
- `cargo test -p stream-sync-client`
- `cargo test -p stream-sync-server`
- `cargo test -p stream-sync-net-core`
- `cargo fmt --check`
- `cargo build -p stream-sync-server -p stream-sync-client`
- `target/debug/stream-sync-server.exe --receive-send-twice configs/examples/server.example.toml`
- `target/debug/stream-sync-client.exe --auth-heartbeat-poc-once configs/examples/client.accepted.example.toml`
- 手動確認で client stdout に `accepted=true`, `reason_code=Ok`, `sent Heartbeat 77 bytes`, `received HeartbeatAck 73 bytes` を観測した。
- 手動確認で server stdout に `first_sent_bytes=55`, `second_sent_bytes=73`, `registered_clients=1` を観測した。
- 手動確認で server stderr に `message_type="Heartbeat"` の accepted receive log と `message_type="HeartbeatAck"` の send success log を観測した。

---

## 2026-04-22
### 種別
- Codex

### 今回の作業
- client 側の `--auth-request-poc-once` で、`AuthRequest` 送信後に同じ UDP socket から `AuthResponse` を 1 回受信して stdout 表示する最小実装を追加した。
- `crates/protocol` に `AuthResponse` payload decode と decode dispatch 対応を追加した。
- `--receive-send-once` と accepted client config の手動通し確認を再実行し、client stdout でも `accepted=true`, `reason_code=Ok` を観測した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `crates/protocol/src/lib.rs`
- `crates/net-core/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `--auth-request-poc-once` は送信専用ではなく、送信後に `AuthResponse` を 1 packet だけ待つ。
- client stdout は accepted / rejected の判断に必要な `accepted`, `reason_code`, `message`, `expected_protocol_version` を最小表示する。
- read timeout は既存 client config の `[network].connect_timeout_ms` を使い、未指定時は 5000ms とする。
- 継続 receive/send loop、heartbeat、video、switcher、retry、requeue、secret store 連携は今回も対象外のまま残す。

### 実装したこと
- `decode_auth_response_payload` / `AuthResponsePayloadDecoder` / `AuthResponseReasonCode::try_from` を追加した。
- `decode_payload_by_message_type` と `InboundPacketDecoder` が `AuthResponse` を decode できるようにした。
- client one-shot launcher が `AuthRequest` encode/send 後、read timeout 付きで `AuthResponse` を 1 回 receive/decode するようにした。
- client CLI の stdout に response byte 数、source、accepted、reason_code、message、expected_protocol_version を表示するようにした。
- protocol / net-core / client の関連単体テストを追加・更新した。

### 手動確認
- `cargo build -p stream-sync-server -p stream-sync-client`
- server: `target/debug/stream-sync-server.exe --receive-send-once configs/examples/server.example.toml`
- client: `target/debug/stream-sync-client.exe --auth-request-poc-once configs/examples/client.accepted.example.toml`
- client stdout は `received AuthResponse 55 bytes`, `accepted=true`, `reason_code=Ok`, `message=null`, `expected_protocol_version=null` を表示した。
- server stdout は `sent_bytes=55`, `BodyIterationCompleted`, `YieldToCaller` を表示した。
- server stderr は `server.receive_loop`, `server.auth_result`, `server.send` を出力し、`server.send` は `message_type="AuthResponse"`, `bytes_sent=55` だった。

### 未実装 / 保留
- completed continuous receive/send loop
- heartbeat / video / switcher 側への拡張
- retry / requeue
- auth / receive / send JSON Lines file sink open / rotation / retention
- process-wide logger
- secret store 連携

### 次にやる候補
- heartbeat 送信処理を client 側に最小実装する
- auth / receive / send JSON Lines file sink の実 file open 範囲を再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要時に整理する

### TODO 更新
- 現在位置に `AuthResponse` payload decode 完了と client one-shot receive / stdout 表示完了を反映した。
- client 側タスクに `--auth-request-poc-once` の `AuthResponse` 1 回受信表示完了を追加した。
- 検証タスクに `AuthResponse` decode と client one-shot receive の関連単体テスト追加を反映した。
- `auth-roundtrip-manual-check.md` に client 側 accepted / rejected 表示方針と 2026-04-22 の accepted path 手動確認結果を追加した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-protocol auth_response`
- `cargo test -p stream-sync-net-core auth_response`
- `cargo test -p stream-sync-client auth_request_poc`
- `cargo check --workspace`
- `cargo build -p stream-sync-server -p stream-sync-client`
- `target/debug/stream-sync-server.exe --receive-send-once configs/examples/server.example.toml`
- `target/debug/stream-sync-client.exe --auth-request-poc-once configs/examples/client.accepted.example.toml`

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- send JSON Lines writer の one-iteration 実接続範囲を整理した。
- one-item send runtime の success / failure observation を `server.send` JSON Lines として caller-owned writer へ渡す最小接続を追加した。
- `--receive-send-once` の accepted auth 手動確認を再実行し、`server.send` success observation を観測した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `server.send_error` は既存どおり failure-only の send error boundary として残す。
- 新しい `server.send` は one-iteration receive/send runtime の success / failure observation 用とする。
- success は `outcome="Success"`, `stage="SocketSend"`, `encoded_len`, `bytes_sent`, `failure=null` を記録する。
- failure は `outcome="Failure"`, `stage`, `encoded_len`, `failure`, `disposition` を記録し、`bytes_sent=null` とする。
- writer は caller-owned `io::Write` のみを受け取り、file open / rotation / process-wide logger / retry / requeue は持たない。

### 実装したこと
- `ServerSendJsonLogEventInput`, `ServerSendJsonLogEventBoundary`, `ServerSendJsonLineWriter`, `ServerSendLogOutputBoundary` を追加した。
- `ServerReceiveSendOneIterationRuntimeBoundary` に send log writer と send log timestamp を渡し、send success/failure 時に `server.send` を 1 行書くようにした。
- `ServerControllerReceiveSendRuntimeBoundary` と `ServerReceiveSendOneIterationLauncher` から send log writer を引き回した。
- server CLI `--receive-send-once` で send log writer を stderr へ接続した。
- success / failure writer と receive-send one-iteration runtime の関連テストを更新した。

### 手動確認
- 最初に `cargo run` 同士で確認した際は、server 側の再コンパイル中に client が先に送信し、server が packet を受け取れなかった。
- 先に `cargo build -p stream-sync-server -p stream-sync-client` を実行してから binary を直接起動し、accepted path を確認した。
- server stdout は `sent_bytes=55`, `BodyIterationCompleted`, `YieldToCaller` を表示した。
- server stderr には `server.receive_loop`, `server.auth_result`, `server.send` の 3 行が出力された。
- `server.send` は `outcome="Success"`, `message_type="AuthResponse"`, `encoded_len=55`, `bytes_sent=55` を記録した。

### 未実装 / 保留
- completed continuous send loop
- continuous send loop から send log writer へ渡す本接続
- retry / requeue
- send log file sink open / rotation / retention
- process-wide logger
- heartbeat / video / switcher 側の拡張
- secret store 連携

### 次にやる候補
- auth / receive / send JSON Lines file sink の実 file open 範囲を再確認する
- `ServerNotice` trigger の state transition 接続範囲を再確認する
- continuous send loop から send log writer へ渡す範囲を必要時に整理する

### TODO 更新
- 現在位置に send JSON Lines writer の one-iteration 最小実接続完了を反映した。
- net-core / server 境界に `ServerSendLogOutputBoundary` / one-iteration send success/failure JSON Lines writer 追加完了を反映した。
- 直近でやることを auth / receive / send file sink 範囲、ServerNotice trigger、continuous send loop から send log writer への接続範囲へ更新した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-server send_log`
- `cargo test -p stream-sync-server receive_send_one_iteration`
- `cargo test -p stream-sync-server send_`
- `cargo build -p stream-sync-server -p stream-sync-client`
- `target/debug/stream-sync-server.exe --receive-send-once configs/examples/server.example.toml`
- `target/debug/stream-sync-client.exe --auth-request-poc-once configs/examples/client.accepted.example.toml`
- `cargo fmt --check`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- `--receive-send-once` を使って accepted auth request の手動通し確認を実行した。
- server / client example config の組み合わせで、accepted AuthRequest が one-iteration receive/send runtime から UDP send 側へ流れることを確認した。
- 観測した stdout / stderr の要点を `docs/operations/auth-roundtrip-manual-check.md` に記録した。

### 変更ファイル
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実行コマンド
- `cargo build -p stream-sync-server -p stream-sync-client`
- server: `cargo run -p stream-sync-server -- --receive-send-once configs/examples/server.example.toml`
- client: `cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml`

### 観測結果
- server は 1 packet を処理して終了した。
- server stdout は `sent_bytes=55`, `observation_state=BodyIterationCompleted`, `observation_action=YieldToCaller` を表示した。
- server stderr には `server.receive_loop` JSON Lines が出力され、`outcome="Accepted"`, `message_type="AuthRequest"`, `client_id="player1"` を確認した。
- server stderr には `server.auth_result` JSON Lines が出力され、`accepted=true`, `reason_code="Ok"`, `protocol_version=1` を確認した。
- client stdout は `auth request PoC sent 96 bytes to 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1` を表示した。
- client stderr は cargo の build / run 表示のみだった。

### 決定事項
- `--receive-send-once` は accepted auth request の手動通し確認入口として成立した。
- 現行 client の `--auth-request-poc-once` は送信専用 PoC のため、client stdout には `AuthResponse` 受信結果は表示されない。
- `sent_bytes=55` は server 側で accepted `AuthResponse` を encode / UDP send まで渡した確認値として扱う。

### 未実装 / 保留
- completed continuous receive/send loop
- client 側での `AuthResponse` 受信表示
- retry / requeue
- send JSON Lines writer 実接続
- file sink open / process-wide logger
- heartbeat / video / stats 本体拡張
- secret store 連携

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する
- send JSON Lines writer の実接続範囲を必要時に整理する

### TODO 更新
- 現在位置に `--receive-send-once` accepted path 手動通し確認成功を反映した。
- 仕様 / 設計に `--receive-send-once` accepted auth request の手動通し確認結果記録完了を追加した。
- 直近でやることから `--receive-send-once` 手動通し確認を外し、file sink / ServerNotice / send log writer 側へ更新した。

### 検証
- `cargo build -p stream-sync-server -p stream-sync-client`
- `cargo run -p stream-sync-server -- --receive-send-once configs/examples/server.example.toml`
- `cargo run -p stream-sync-client -- --auth-request-poc-once configs/examples/client.accepted.example.toml`

---

### 種別
- Codex

### 今回の作業
- completed one-iteration runtime の CLI / config 接続を追加した。
- `apps/server` から one-iteration receive/send runtime を呼べる手動確認入口を追加した。
- 既存 server / client example config を使う accepted auth round trip の手動確認手順を docs に反映した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- CLI 入口は `--receive-send-once [config-path]` とし、既定値は `configs/examples/server.example.toml` とする。
- launcher は既存の server TOML shape を読み、UDP socket bind、in-memory registry、outbound queue collection を初期化して `ServerControllerReceiveSendRuntimeBoundary` を 1 回だけ呼ぶ。
- CLI は caller-owned stderr writer へ receive / rejection / auth JSON Lines を渡し、stdout には短い summary だけを出す。
- この入口は accepted auth response が queue から encode / send まで流れる手動確認用であり、continuous receive/send loop ではない。
- retry / requeue、file sink open、process-wide logger、secret store、heartbeat / video / stats 本体拡張は今回も未実装のまま残す。

### 未実装 / 保留
- 完成した continuous receive loop
- 完成した continuous send loop
- retry / requeue
- send JSON Lines writer 実接続
- rejection response 送信 policy
- heartbeat ack の queue storage / send 接続
- video buffer / sync-core handoff 本体
- stats metrics state commit / heartbeat observation commit
- packet drop 本体
- file sink open / process-wide logger
- secret store 連携

### 次にやる候補
- `--receive-send-once` と accepted auth client config の組み合わせで手動通し確認を行う
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する

### TODO 更新
- 現在位置に completed one-iteration runtime の CLI / config 接続範囲整理完了を反映した。
- net-core / server 境界に `ServerReceiveSendOneIterationLauncher` / completed one-iteration runtime CLI config entry placeholder 追加完了を反映した。
- 直近でやることを `--receive-send-once` の手動通し確認中心へ更新した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-server receive_send_one_iteration`
- `cargo fmt --check`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- controller が one-iteration receive/send runtime を呼ぶ最小実装を追加した。
- stop 判定と 1 iteration 実行を `ServerControllerReceiveSendRuntimeBoundary` で接続した。
- accepted auth request を起点に controller から UDP response send まで通す近い統合テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- controller receive-send runtime は `ServerControllerReceiveSendRuntimeBoundary` として、controller plan を 1 回作る。
- `continue_requested=false` の場合は `Stopped` を返し、receive / dispatch / queue / encode / send / log writer を呼ばない。
- `RunBodyOnce` の場合は `ServerReceiveSendOneIterationRuntimeBoundary` を 1 回だけ呼び、body result を controller boundary で observe する。
- 戻り値は controller plan、one-iteration outcome、controller observation を保持し、future loop controller が次の判断に使える形にする。
- 反復、shutdown policy、retry / requeue、file sink open、process-wide logger、packet drop policy は今回も未実装のまま残す。

### 未実装 / 保留
- 完成した continuous receive loop
- 完成した continuous send loop
- controller による反復 / shutdown policy
- retry / requeue
- send JSON Lines writer 実接続
- rejection response 送信 policy
- heartbeat ack の queue storage / send 接続
- video buffer / sync-core handoff 本体
- stats metrics state commit / heartbeat observation commit
- packet drop 本体
- file sink open / process-wide logger

### 次にやる候補
- completed one-iteration runtime の CLI / config 接続範囲を必要時に整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する

### TODO 更新
- 現在位置に controller が one-iteration receive/send runtime を呼ぶ最小範囲整理完了を反映した。
- net-core / server 境界に `ServerControllerReceiveSendRuntimeBoundary` / controller receive-send runtime placeholder 追加完了を反映した。
- 直近でやることを completed one-iteration runtime の CLI / config 接続範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server controller_receive_send_runtime`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- continuous receive loop と one-item send runtime の結合範囲を docs に明記した。
- `apps/server` に receive-send one iteration integration placeholder を追加した。
- accepted auth request を起点に receive body から dispatch / side effect / queue / one-item send runtime まで通す近い統合テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- receive-send integration は `ServerReceiveSendOneIterationRuntimeBoundary` として、1 receive body iteration と optional 1 send attempt だけを接続する。
- boundary は body result、dispatch、side effect、output apply、queue push、dequeue、send outcome をすべて返し、future controller が次の判断をできるようにする。
- caller-owned socket / receive buffer / registry / queue collection / writers を受け取り、境界内部で file open や process-wide logger を持たない。
- queue collection は accepted auth response の queued item を push し、最大 1 item だけ dequeue する。
- send runtime は 1 item の encode + UDP send attempt だけを行い、retry / requeue / continuous send loop は持たない。

### 未実装 / 保留
- 完成した continuous receive loop
- 完成した continuous send loop
- controller による反復 / shutdown policy
- retry / requeue
- send JSON Lines writer 実接続
- rejection response 送信 policy
- heartbeat ack の queue storage / send 接続
- video buffer / sync-core handoff 本体
- stats metrics state commit / heartbeat observation commit
- packet drop 本体
- file sink open / process-wide logger

### 次にやる候補
- controller が one-iteration receive/send runtime を呼ぶ範囲を必要時に整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する

### TODO 更新
- 現在位置に continuous receive loop と one-item send runtime の最小結合範囲整理完了を反映した。
- net-core / server 境界に `ServerReceiveSendOneIterationRuntimeBoundary` / receive-send one iteration integration placeholder 追加完了を反映した。
- 直近でやることを controller が one-iteration receive/send runtime を呼ぶ範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server receive_send_one_iteration_runtime_sends_accepted_auth_response`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- send loop / queue collection の最小接続を追加した。
- accepted auth response が queue collection から dequeue され、encode / socket send 側へ流れる最小統合経路を追加した。
- accepted auth request を起点に receive body / dispatch / side effect / output apply / queue collection / send one runtime まで通す近い統合テストを追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- queue collection は `ServerOutboundQueueCollection` と `ServerOutboundQueueCollectionBoundary` として caller-owned FIFO-compatible collection に限定する。
- dequeue は `ServerOutboundQueueDequeueRuntimeResult` として 1 item または empty を返すだけにする。
- send runtime は `ServerOutboundSendOneRuntimeBoundary` として 1 queued item を `OutboundQueueSendHandoff`、encode、`EncodedOutboundPacket`、`ServerUdpSocketIoStep::send_encoded` へ同期接続する。
- send runtime は encode / socket send の typed event を返すが、send JSON Lines 書き込みは行わない。
- continuous send loop、retry、requeue、queue eviction、file sink open、process-wide logger、async runtime は今回も未実装のまま残す。

### 未実装 / 保留
- 完成した continuous send loop
- retry / requeue
- queue eviction / backpressure side effect
- send JSON Lines writer 実接続
- rejection response 送信 policy
- heartbeat ack の queue storage / send 接続
- video buffer / sync-core handoff 本体
- stats metrics state commit / heartbeat observation commit
- packet drop 本体
- file sink open / process-wide logger

### 次にやる候補
- continuous receive loop と one-item send runtime の結合範囲を必要時に整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する

### TODO 更新
- 現在位置に send loop / queue collection の最小接続範囲整理完了を反映した。
- net-core / server 境界に `ServerOutboundQueueCollectionBoundary` / queue collection placeholder 追加完了を反映した。
- net-core / server 境界に `ServerOutboundSendOneRuntimeBoundary` / one-item encode and socket send runtime placeholder 追加完了を反映した。
- 直近でやることを continuous receive loop と one-item send runtime の結合範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server queue_collection_dequeues_accepted_auth_response_for_send_runtime`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- dispatch runtime side effect apply から outbound queue storage / auth log writer への最小実接続を追加した。
- accepted auth の `AuthResponse` queue item を outbound queue storage planning / one-item queued placeholder へ渡す境界を追加した。
- auth log input を既存 `ServerAuthLogOutputBoundary` へ渡して caller-owned writer に JSON Lines 1 行を書けるようにした。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- output apply boundary は `ServerDispatchRuntimeOutputApplyBoundary` として、`ServerDispatchRuntimeSideEffectApplyOutcome` を受け取る。
- auth result は `ServerAuthLogOutputBoundary` で caller-owned writer へ JSON Lines を書く。
- accepted auth の `AuthResponse` `OutboundQueueItem` だけを `ServerOutboundQueueBoundary::evaluate_storage_push` に渡し、accepted storage decision の場合に `OutboundQueueLifecycleBoundary::hold_for_send` で `QueuedOutboundItem` にする。
- rejected auth は auth log 書き込みのみ行い、rejection response を queue storage へ渡すかは future continuous loop policy に残す。
- registry registration は前段の side effect apply boundary の責務に残し、この output apply boundary では registry を変更しない。
- heartbeat / video / stats handoff は保持のみで、heartbeat ack queue storage、video buffer handoff、stats state commit は未実装のまま残す。

### 未実装 / 保留
- 実 queue collection / dequeue
- send loop への実接続
- rejection response 送信 policy
- heartbeat ack の queue storage 接続
- video buffer / sync-core handoff 本体
- stats metrics state commit / heartbeat observation commit
- packet drop 本体
- file sink open / process-wide logger
- 完成した continuous receive loop / while loop

### 次にやる候補
- send loop / queue collection の最小接続範囲を必要時に整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する

### TODO 更新
- 現在位置に accepted auth の outbound queue storage / auth log writer 最小接続範囲整理完了を反映した。
- net-core / server 境界に `ServerDispatchRuntimeOutputApplyBoundary` / accepted auth queue storage and auth log writer placeholder 追加完了を反映した。
- 直近でやることを send loop / queue collection の最小接続範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server dispatch_output_apply`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- dispatch runtime 結果の side effect 適用範囲を docs に明記した。
- `apps/server` に dispatch side effect apply placeholder を追加した。
- auth flow result / registry registration / outbound enqueue / stats prepare result / future state commit の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- side effect apply boundary は `ServerDispatchRuntimeSideEffectApplyBoundary` として、dispatch runtime output を受け取る。
- 現時点で実適用する side effect は accepted auth の `AuthenticatedSenderRegistration` を caller-owned `AuthenticatedSenderRegistry` へ反映することだけに限定する。
- auth log input と `AuthResponse` `OutboundQueueItem` は `ServerAuthFlowOutcome` 内の handoff として保持し、log 書き込みや queue storage は行わない。
- heartbeat は `ServerHeartbeatAckHandoff` を保持するだけで、heartbeat state commit、queue storage、encode、UDP send は行わない。
- video は `ServerVideoFrameHandlerInput`、stats は `ServerClientStatsHandlerInput` を保持するだけで、video buffer / sync handoff、metrics commit、heartbeat observation commit、RTT / offset state commit は行わない。
- unsupported / error / no-dispatch lane は packet drop policy や error policy を実行せず保持する。

### 未実装 / 保留
- outbound queue storage / send loop への実接続
- auth log writer への continuous loop 内実接続
- heartbeat state commit / RTT offset state commit
- video buffer / sync-core handoff 本体
- stats metrics state commit / heartbeat observation commit
- packet drop 本体
- file sink open / process-wide logger
- 完成した continuous receive loop / while loop

### 次にやる候補
- outbound queue storage / log writer 実接続範囲を必要時に整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する
- video buffer / sync-core handoff の最小境界を必要時に整理する

### TODO 更新
- 現在位置に dispatch runtime 結果の side effect 適用範囲整理完了を反映した。
- net-core / server 境界に `ServerDispatchRuntimeSideEffectApplyBoundary` / dispatch side effect apply placeholder 追加完了を反映した。
- 直近でやることを outbound queue storage / log writer 実接続範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server dispatch_side_effect_apply`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- continuous receive loop body から auth / registered / video stats dispatch runtime を呼ぶ最小実接続範囲を docs に明記した。
- `apps/server` に body dispatch runtime placeholder を追加した。
- receive loop body / auth dispatch / registered packet dispatch / video stats handler runtime / future loop 本体の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- body dispatch runtime は `ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary` として、1 つの `ServerContinuousReceiveLoopBodyResult` を既存の dispatch runtime chain へ接続する。
- body result は `ServerContinuousReceiveLoopHandlerDispatchBoundary` と `ServerHandlerDispatchBoundary` で lane 分類してから、auth / registered / video stats runtime のいずれかへ 1 回だけ渡す。
- Auth lane は `ServerAuthDispatchRuntimeBoundary` を 1 回呼ぶ。
- registered heartbeat lane は `ServerRegisteredPacketDispatchRuntimeBoundary` を 1 回呼び、HeartbeatAck handoff までで止める。
- registered video / stats lane は `ServerRegisteredPacketDispatchRuntimeBoundary` の後に `ServerVideoStatsHandlerRuntimeBoundary` を 1 回呼び、typed input 準備までで止める。
- stopped / socket receive failure / rejected outcome / unsupported / handoff error は no-dispatch result として保持し、future policy へ残す。
- registry registration 適用、auth log 書き込み、queue storage、heartbeat/video/stats state commit、packet encode、UDP send、packet drop、loop 反復は今回の runtime では実行しない。

### 未実装 / 保留
- dispatch runtime 結果の side effect 適用
- registry registration の continuous loop 内適用
- auth log writer への continuous loop 内実接続
- outbound queue storage / send loop への実接続
- heartbeat state commit / RTT offset state commit
- video buffer / sync-core handoff 本体
- stats metrics state commit / heartbeat observation commit
- packet drop 本体
- file sink open / process-wide logger
- 完成した continuous receive loop / while loop

### 次にやる候補
- dispatch runtime 結果の side effect 適用範囲を必要時に整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する
- video buffer / sync-core handoff の最小境界を必要時に整理する

### TODO 更新
- 現在位置に continuous receive loop body から dispatch runtime を呼ぶ最小範囲整理完了を反映した。
- net-core / server 境界に `ServerContinuousReceiveLoopBodyDispatchRuntimeBoundary` / body dispatch runtime placeholder 追加完了を反映した。
- 直近でやることを dispatch runtime 結果の side effect 適用範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server body_dispatch_runtime`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- video / stats handler の最小実接続範囲を docs に明記した。
- `apps/server` に video stats handler input runtime placeholder を追加した。
- registered packet dispatch / future video handler / future stats handling / heartbeat state commit / outbound enqueue の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- video / stats handler runtime は `ServerVideoStatsHandlerRuntimeBoundary` として、registered packet dispatch runtime の `FutureVideoFrame` / `FutureClientStats` だけを typed handler input へ変換する。
- video は `ServerVideoFrameHandlerInput` として registered packet と payload byte length を保持するだけに留め、H.264 decode、frame buffer、sync scheduling、file sink、drop policy は行わない。
- stats は既存の `ServerClientStatsHandlerBoundary::prepare_input` を呼び、metrics state commit、heartbeat observation commit、durable RTT / offset state update、stats log output は行わない。
- heartbeat ack result とその他 lane は `NotVideoOrStats` として保持し、heartbeat state commit と outbound enqueue の責務を混ぜない。
- この runtime は outbound queue item 生成、packet encode、UDP send、sink open、continuous loop body 制御を持たない。

### 未実装 / 保留
- video buffer / sync-core handoff 本体
- stats metrics state commit / heartbeat observation commit
- heartbeat state commit / RTT offset state commit
- outbound queue storage / send loop への実接続
- packet drop 本体
- file sink open / process-wide logger
- 完成した continuous receive loop / while loop

### 次にやる候補
- continuous receive loop body から auth / registered / video stats dispatch runtime を呼ぶ範囲を必要時に整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する
- video buffer / sync-core handoff の最小境界を必要時に整理する

### TODO 更新
- 現在位置に video / stats handler の最小 input 接続範囲整理完了を反映した。
- net-core / server 境界に `ServerVideoStatsHandlerRuntimeBoundary` / video stats handler input runtime placeholder 追加完了を反映した。
- 直近でやることから video / stats handler 範囲整理を外し、continuous receive loop body から dispatch runtime を呼ぶ範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server video_stats_handler_runtime`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- registered packet handler の最小実接続範囲を docs に明記した。
- `apps/server` に registered packet dispatch runtime placeholder を追加した。
- registered packet dispatch / heartbeat handler / future video handler / future stats handling / outbound enqueue の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- registered packet dispatch runtime は `ServerRegisteredPacketDispatchRuntimeBoundary` として、`ServerHandlerDispatchOutcome` の registered lanes だけを扱う。
- `RegisteredHeartbeat` は既存の `ServerHeartbeatHandlerBoundary::handoff_ack` へ接続し、`HeartbeatAck` の one-item outbound handoff まで行う。
- heartbeat timing は caller-owned とし、この runtime では clock / runtime policy を持たない。
- `RegisteredVideoFrame` は `FutureVideoFrame` として保持し、video frame buffering、sync scheduling、decoder handoff、drop policy は後段へ残す。
- `RegisteredClientStats` は `FutureClientStats` として保持し、metrics state commit、heartbeat observation commit、stats log output は後段へ残す。
- queue storage、packet encode、UDP send、retry、send-loop scheduling は今回の runtime では実行しない。

### 未実装 / 保留
- video handler 本体
- stats handler 本体
- heartbeat state commit / RTT offset state commit
- outbound queue storage / send loop への実接続
- packet drop 本体
- file sink open / process-wide logger
- 完成した continuous receive loop / while loop

### 次にやる候補
- video / stats handler の最小実接続範囲を必要時に整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する
- continuous receive loop body から auth / registered dispatch runtime を呼ぶ範囲を必要時に整理する

### TODO 更新
- 現在位置に registered packet handler の最小実接続範囲整理完了を反映した。
- net-core / server 境界に `ServerRegisteredPacketDispatchRuntimeBoundary` / registered packet dispatch runtime placeholder 追加完了を反映した。
- 直近でやることから registered packet handler 範囲整理を外し、video / stats handler の最小実接続範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server registered_packet_dispatch_runtime`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- auth dispatch の最小実接続範囲を docs に明記した。
- `apps/server` に auth dispatch runtime placeholder を追加した。
- auth dispatch / auth decision / outbound response handoff / future loop 本体の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- auth dispatch runtime は `ServerAuthDispatchRuntimeBoundary` として、`ServerHandlerDispatchOutcome` のうち `Auth` だけを既存の `ServerAuthFlowStep` へ渡す。
- auth decision は `ServerAuthFlowStep` / `ServerAuthDecisionBoundary` の責務に残す。
- AuthResponse 生成と outbound queue item handoff は `ServerAuthResponseBoundary` / `ServerOutboundQueueBoundary` の責務に残す。
- registry registration の適用、auth log 書き込み、queue storage、packet encode、UDP send、retry、future loop body 制御は今回の runtime では実行しない。
- 非 Auth の handler dispatch result は `NotAuth` として保持し、registered packet dispatch 側へ残す。

### 未実装 / 保留
- registered packet handler 本体
- registry registration の continuous loop 内適用
- auth log writer への continuous loop 内実接続
- outbound queue storage / send loop への実接続
- packet drop 本体
- file sink open / process-wide logger
- 完成した continuous receive loop / while loop

### 次にやる候補
- registered packet handler の最小実接続範囲を整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する
- continuous receive loop body から auth dispatch runtime を呼ぶ範囲を必要時に整理する

### TODO 更新
- 現在位置に auth dispatch の最小実接続範囲整理完了を反映した。
- net-core / server 境界に `ServerAuthDispatchRuntimeBoundary` / auth dispatch runtime placeholder 追加完了を反映した。
- 直近でやることから auth dispatch 範囲整理を外し、registered packet handler の最小実接続範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server auth_dispatch_runtime`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- handler dispatch 本体の最小実装範囲を docs に明記した。
- `apps/server` に handler dispatch result placeholder を追加した。
- auth dispatch / registered packet dispatch / future outbound enqueue / future stats handling の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- handler dispatch 本体の現在範囲は `ServerHandlerDispatchBoundary` として、dispatch bridge の handoff を handler lane へ分類するところまでとする。
- `Auth` は `ServerHandlerDispatchResult::Auth` として `ServerAuthCheck` を保持するだけで、auth decision、registry mutation、AuthResponse enqueue は行わない。
- `RegisteredClient` は heartbeat / video frame / client stats の dispatch result に分けるだけで、heartbeat ack/state、video buffering、stats state commit、timebase update は行わない。
- unsupported route、skip、handoff error は dispatch result として保持し、packet drop policy や error logging policy は後段へ残す。
- future outbound enqueue と future stats handling は generic dispatch classification から分離する。

### 未実装 / 保留
- auth dispatch 本体
- registered packet handler 本体
- outbound enqueue への handler output 実接続
- stats metrics state commit / heartbeat observation commit
- packet drop 本体
- file sink open / process-wide logger
- 完成した continuous receive loop / while loop

### 次にやる候補
- auth dispatch の最小実接続範囲を整理する
- registered packet handler の最小実接続範囲を整理する
- auth / receive JSON Lines file sink の実 file open 範囲を再確認する
- ServerNotice trigger の state transition 接続範囲を再確認する

### TODO 更新
- 現在位置に handler dispatch 本体の最小分類範囲整理完了を反映した。
- net-core / server 境界に `ServerHandlerDispatchBoundary` / handler dispatch result placeholder 追加完了を反映した。
- 直近でやることから handler dispatch 本体の範囲整理を外し、auth dispatch / registered packet handler の最小実接続範囲整理へ更新した。

### 検証
- `cargo fmt --check`
- `cargo test -p stream-sync-server handler_dispatch_body`
- `cargo check --workspace`

---

### 種別
- Codex

### 今回の作業
- continuous receive loop から handler dispatch への最小実接続範囲を docs に明記した。
- `apps/server` に handler dispatch bridge placeholder を追加した。
- controller / body / one-tick runtime / handler handoff runtime / future handler dispatch 本体の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- dispatch bridge は `ServerContinuousReceiveLoopHandlerDispatchBoundary` として、body result から future handler dispatch への typed handoff 計画だけを担当する。
- stopped loop、socket receive failure、rejected outcome は `NotRequired` として handler 実行へ進めない。
- accepted `AuthRequest` は `ServerAuthCheck` を future auth dispatch input として保持する。
- accepted `Heartbeat` / `VideoFrame` / `ClientStats` は `ServerRegisteredClientPacket` を future registered-client dispatch input として保持する。
- unsupported route と handoff preparation error は marker として保持し、policy 実行は後段に残す。
- auth decision、heartbeat / video / stats handler 本体、outbound enqueue、packet drop、file sink open、process-wide logger、retry/backoff、async runtime は今回も未実装のまま残す。

### 未実装 / 保留
- handler dispatch 本体
- auth decision / outbound response queue への continuous loop 内実接続
- heartbeat / video / stats handler 本体
- packet drop 本体
- 完成した continuous receive loop / while loop
- shutdown signal / retry / backoff policy
- file sink open / process-wide logger

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する。
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する。
- handler dispatch 本体の最小実装範囲を必要になった時点で整理する。

### TODO更新
- 現在位置に handler dispatch bridge placeholder 追加済み、handler dispatch 本体は未実装であることを反映した。
- 直近でやることから handler dispatch bridge 整理を外し、file sink / ServerNotice trigger / handler dispatch 本体の範囲整理へ更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopHandlerDispatchBoundary` 追加完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop_handler_dispatch` は成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- continuous receive loop controller の継続実行範囲を docs に明記した。
- `apps/server` に outer controller lifecycle placeholder を追加した。
- controller / run_once body / one-tick runtime / handler dispatch / shutdown policy の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- controller は `ServerContinuousReceiveLoopControllerBoundary` として、外側の iteration checkpoint だけを担当する。
- controller は caller-owned の `continue_requested` を消費して、次に `run_once` body を 1 回実行するか停止するかを計画する。
- controller は body 結果を stopped / completed / error-policy-deferred として分類し、次の判断は caller に返す。
- `run_once` body は 1 回分の stop check と one-tick runtime delegation のみを担当する。
- one-tick runtime は 1 datagram receive、decode / gate、writer runtime、handler handoff preparation までを担当する。
- handler dispatch 本体、packet drop 本体、shutdown policy、retry/backoff、file sink open、process-wide logger、async runtime は今回も未実装のまま残す。

### 未実装 / 保留
- 完成した continuous receive loop controller / while loop
- handler dispatch 本体
- auth decision / outbound response queue への continuous loop 内実接続
- heartbeat / video / stats handler 本体
- packet drop 本体
- shutdown signal / retry / backoff policy
- file sink open / process-wide logger

### 次にやる候補
- continuous receive loop から handler dispatch への最小実接続範囲を整理する。
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する。
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する。

### TODO更新
- 現在位置に controller placeholder 追加済み、完成した継続 loop は未実装であることを反映した。
- 直近でやることから controller 整理を外し、handler dispatch 実接続範囲整理を次優先に更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopControllerBoundary` 追加完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop_controller` は成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- continuous receive loop の最小 loop body 実装範囲を docs に明記した。
- `apps/server` に 1 iteration だけの minimal loop body placeholder を追加した。
- stop 判定、one-tick runtime 呼び出し、writer runtime / handler handoff runtime との接続を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- minimal loop body は `ServerContinuousReceiveLoopBodyBoundary::run_once` として、1 回分の body iteration だけを実行する。
- body は stop flag を評価し、`Stop` または `ExecuteOneTick` の action を記録する。
- 実際の socket receive、decode / gate、writer runtime、handler handoff runtime は既存の `ServerContinuousReceiveLoopOneTickRuntimeBoundary` に委譲する。
- stop requested の場合は one-tick runtime 側で socket receive 前に停止する。
- body は自動繰り返し、時刻生成、shutdown signal 管理、handler dispatch、packet drop、file sink open、process-wide logger、retry / backoff、async runtime を持たない。

### 未実装 / 保留
- continuous receive loop controller の継続実行本体
- handler dispatch 本体
- auth decision / outbound response queue への loop 内接続
- heartbeat / video / stats handler 本体
- packet drop 本体
- file sink open / process-wide logger
- retry / backoff / shutdown policy 本体

### 次にやる候補
- continuous receive loop controller の継続実行範囲を整理する
- continuous receive loop から handler dispatch への最小実接続範囲を整理する
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する

### TODO更新
- 現在位置に continuous receive loop の最小 loop body 実装追加完了を反映した。
- 直近でやることを continuous receive loop controller の継続実行範囲整理と handler dispatch 最小接続範囲整理へ更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopBodyBoundary` / minimal loop body placeholder 追加完了を反映した。
- net-core / server 境界に continuous receive loop の最小 loop body 実装追加完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop_body` は成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- continuous receive loop 本体の最小 1 tick 実行接続範囲を docs に明記した。
- `apps/server` に one-tick runtime execution placeholder を追加した。
- socket receive / tick plan / writer runtime / handler handoff runtime / future loop 本体の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- one-tick runtime は stop 判定、1 datagram receive、decode / gate、writer runtime、handler handoff runtime を 1 回分だけ同期的に接続する。
- `ServerUdpSocketIoStep::receive_one_with_gate_details` は `ServerReceiveLoopGateOutcome` と packet length を返し、writer runtime の packet length 入力を満たす。
- stop requested の場合は socket receive と writer 呼び出しを行わず `Stopped` を返す。
- socket receive error は `SocketReceiveFailed` outcome として tick checkpoint と `io::ErrorKind` を返す。
- writer error は caller-owned writer runtime の `io::Result` error として返す。
- one-tick runtime は継続 loop、handler dispatch、packet drop、file sink open、process-wide logger、retry、async runtime を持たない。

### 未実装 / 保留
- continuous receive loop の継続実行本体
- handler dispatch 本体
- auth decision / outbound response queue への loop 内接続
- heartbeat / video / stats handler 本体
- packet drop 本体
- file sink open / process-wide logger
- retry / backoff / shutdown policy 本体

### 次にやる候補
- continuous receive loop の継続実行 loop body 着手前の範囲を再確認する
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する

### TODO更新
- 現在位置に continuous receive loop の最小 1 tick 実行接続範囲整理完了を反映した。
- 直近でやることを continuous receive loop の継続実行 loop body 着手前の範囲再確認へ更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopOneTickRuntimeBoundary` / minimal one-tick runtime execution placeholder 追加完了を反映した。
- net-core / server 境界に continuous receive loop 本体の最小 1 tick 実行接続範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop_one_tick_runtime` は成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- continuous receive loop 本体へ進む前の handler handoff 実接続範囲を docs に明記した。
- `apps/server` に writer runtime 後の handler handoff runtime placeholder を追加した。
- receive tick / writer runtime / handler handoff runtime / future loop 本体の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- handler handoff runtime は、まず caller-owned writer runtime を実行して operational / rejection JSON Lines 出力を処理する。
- rejected outcome では handler input を作らず、`NotRequired` とする。
- accepted `AuthRequest` は `ServerAuthHandlerBoundary` で `ServerAuthCheck` に変換する。
- accepted `Heartbeat` / `VideoFrame` / `ClientStats` は `ServerRegisteredPacketBoundary` で `ServerRegisteredClientPacket` に変換し、authenticated sender binding を保持する。
- server unsupported route は source と `MessageType` の marker だけを返し、handler 本体には踏み込まない。
- handler handoff runtime は auth decision、heartbeat / video / stats handler 実行、outbound enqueue、packet drop、retry、sink 選択、file open、continuous loop 実行を持たない。

### 未実装 / 保留
- continuous receive loop 実行本体
- handler dispatch 本体
- auth decision / outbound response queue への loop 内接続
- heartbeat / video / stats handler 本体
- packet drop 本体
- file sink open / process-wide logger

### 次にやる候補
- continuous receive loop 本体の最小 1 tick 実行接続範囲を整理する
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する

### TODO更新
- 現在位置に continuous receive loop の handler handoff 実接続範囲整理完了を反映した。
- 直近でやることを continuous receive loop 本体の最小 1 tick 実行接続範囲中心へ更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopHandlerHandoffRuntimeBoundary` / handler handoff runtime placeholder 追加完了を反映した。
- net-core / server 境界に continuous receive loop 本体前の handler handoff 実接続範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop_handler_handoff_runtime` は成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- continuous receive loop の writer 呼び出し実接続範囲を docs に明記した。
- `apps/server` に caller-owned writer runtime handoff placeholder を追加した。
- receive tick / writer handoff / caller-owned writer / sink plan の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- writer 呼び出し実接続は caller-owned `io::Write` に対して既存 output boundary を呼ぶ範囲までとする。
- operational logging が必要な場合は `ServerReceiveLoopLogOutputBoundary` を呼び、`server.receive_loop` を 1 行出力する。
- rejection logging が必要な場合は `ServerReceiveRejectionLogOutputBoundary` を呼び、`server.receive_rejection` を 1 行出力する。
- runtime boundary は sink 選択、file open、process-wide logger、continuous loop 実行、handler dispatch、packet drop を持たない。

### 未実装 / 保留
- continuous receive loop 実行本体
- socket receive から writer runtime boundary への loop 内接続
- caller-owned writer の server runtime 注入
- file sink open / process-wide logger
- handler dispatch 本体
- packet drop 本体

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- continuous receive loop 本体へ進む前の handler handoff 実接続範囲を必要になった時点で整理する

### TODO更新
- 現在位置に continuous receive loop の writer 呼び出し実接続範囲整理完了を反映した。
- 直近でやることを continuous receive loop 本体へ進む前の handler handoff 実接続範囲整理へ更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopWriterRuntimeBoundary` / caller-owned writer runtime handoff placeholder 追加完了を反映した。
- net-core / server 境界に continuous receive loop の writer 呼び出し実接続範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop_writer_runtime` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- continuous receive loop から operational / rejection writer への実接続範囲を docs に明記した。
- `apps/server` に continuous receive loop writer handoff placeholder を追加した。
- receive tick / operational logging / rejection logging / sink plan の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- writer handoff は、`ServerReceiveLoopGateOutcome` と packet length から operational log input と rejection log input を準備する範囲までとする。
- accepted outcome では `server.receive_loop` 用 operational log input と handler handoff required flag を作る。
- rejected outcome では `server.receive_loop` 用 operational log input と詳細 `server.receive_rejection` 用 rejection input を作る。
- writer handoff boundary は JSON Lines writer 呼び出し、sink 選択、file open、handler dispatch、packet drop、continuous loop 実行を持たない。

### 未実装 / 保留
- continuous receive loop 実行本体
- operational / rejection writer の loop 内呼び出し実接続
- caller-owned writer の runtime 注入
- file sink open / process-wide logger
- handler dispatch 本体
- packet drop 本体

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- continuous receive loop の writer 呼び出し実接続範囲を必要になった時点で整理する

### TODO更新
- 現在位置に continuous receive loop から operational / rejection writer への handoff 範囲整理完了を反映した。
- 直近でやることを continuous receive loop の writer 呼び出し実接続範囲整理へ更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopWriterHandoffBoundary` / writer handoff placeholder 追加完了を反映した。
- net-core / server 境界に continuous receive loop から operational / rejection writer への実接続範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop_writer` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- continuous receive loop の 1 tick 実接続範囲を docs に明記した。
- `apps/server` に continuous receive loop tick placeholder を追加した。
- socket receive / lifecycle / operational logging / rejection logging / handler handoff の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 1 tick の範囲は、stop / receive-one-datagram 判定、受信済み packet の decode/gate 計画、gate outcome 後の operational log / rejection log / handler handoff 要否の計画までとする。
- `ServerContinuousReceiveLoopTickBoundary` は socket call、receive buffer 管理、JSON Lines writer 呼び出し、handler dispatch、packet drop、retry、runtime orchestration を持たない。
- accepted outcome は operational log と future handler handoff を要求する。
- rejected outcome は operational log と detailed receive rejection log handoff を要求する。

### 未実装 / 保留
- continuous receive loop 実行本体
- socket receive の実呼び出しから tick boundary への接続
- operational / rejection writer の loop 内実接続
- handler dispatch 本体
- packet drop 本体
- async runtime

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- continuous receive loop から operational / rejection writer への実接続範囲を必要になった時点で整理する

### TODO更新
- 現在位置に continuous receive loop の 1 tick 実接続範囲整理完了を反映した。
- 直近でやることを continuous receive loop から operational / rejection writer への実接続範囲整理へ更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopTickBoundary` / continuous receive loop tick placeholder 追加完了を反映した。
- net-core / server 境界に continuous receive loop の 1 tick 実接続範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop_tick` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- continuous receive loop 本体の実装範囲を docs に明記した。
- `apps/server` に continuous receive loop lifecycle placeholder を追加した。
- socket receive / decode / gate / handler handoff / operational logging / rejection logging の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- continuous receive loop 本体の最小範囲は、stop 判定、1 datagram receive、既存 `ServerReceiveLoopStep` による decode / route / gate、accepted / rejected outcome の次アクション計画までとする。
- accepted outcome は operational log と future handler handoff を要求する。
- rejected outcome は operational log と detailed receive rejection log handoff を要求する。
- lifecycle boundary は socket 呼び出し、実 loop、handler 実行、packet drop、JSON Lines 書き込み、async runtime を持たない。

### 未実装 / 保留
- continuous receive loop 実行本体
- socket receive から lifecycle / logging / handler dispatch への 1 tick 実接続
- handler dispatch 本体
- packet drop 本体
- receive loop operational / rejection writer の loop 内実接続
- async runtime

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- continuous receive loop の 1 tick 実接続範囲を必要になった時点で整理する

### TODO更新
- 現在位置に continuous receive loop 本体の実装範囲整理完了を反映した。
- 直近でやることを continuous receive loop の 1 tick 実接続範囲整理へ更新した。
- net-core / server 境界に `ServerContinuousReceiveLoopLifecycleBoundary` / continuous receive loop lifecycle placeholder 追加完了を反映した。
- net-core / server 境界に continuous receive loop 本体の実装範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server continuous_receive_loop` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- receive loop の継続運用向けログ範囲を docs に明記した。
- `apps/server` に `server.receive_loop` の operational handoff / JSON Lines event schema / caller-owned writer boundary を追加した。
- receive loop / decode rejection / acceptance rejection / JSON Lines writer / sink plan の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- receive loop 継続運用ログは `server.receive_loop` とし、1 packet の `Accepted` / `DecodeRejected` / `AcceptanceRejected` outcome を記録する。
- 詳細な decode / gate rejection 情報は既存の `server.receive_rejection` に残し、`server.receive_loop` は operational counters 用の軽量 event とする。
- `packet_len`、source、message_type、client_id、rejection_reason、timestamp を保持するが、handler 実行、packet drop、metrics 集約は行わない。
- sink plan は `crates/logging` の既存 JSON Lines sink plan を使うが、file open、rotation、retention、process-wide logger は今回の範囲に含めない。

### 未実装 / 保留
- continuous receive loop 本体
- receive loop から operational writer への実接続
- packet drop 本体
- file sink open / directory creation
- log rotation / retention / compression
- process-wide logger / async logging

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- continuous receive loop 本体の実装範囲を必要になった時点で整理する

### TODO更新
- 現在位置に receive loop 継続運用向けログ範囲整理完了を反映した。
- 直近でやることから receive loop 継続運用向けログ範囲整理を外し、continuous receive loop 本体の実装範囲整理へ更新した。
- net-core / server 境界に `ServerReceiveLoopLogOutputBoundary` / receive loop operational JSON Lines writer placeholder 追加完了を反映した。
- ログ / 計測に receive loop 継続運用向けログ範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server receive_loop` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- send error JSON Lines 出力範囲を docs に明記した。
- `apps/server` に send error の failure-only handoff / JSON Lines event schema / caller-owned writer boundary を追加した。
- send loop / send failure classification / JSON Lines writer / sink plan の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- send error JSON Lines の初期対象は failure `SendLogEvent` のみとし、encode success など非 error event は handoff で無視する。
- event name は `server.send_error` とし、`run_id`、`client_id`、destination、`message_type`、stage、`encoded_len`、failure、disposition、timestamp を保持する。
- `net-core` は send context と failure classification を担当し、JSON Lines schema / writer は `apps/server` が担当する。
- sink plan は `crates/logging` の既存 JSON Lines sink plan を使うが、file open、rotation、retention、process-wide logger は今回の範囲に含めない。

### 未実装 / 保留
- send loop から send error writer への実接続
- file sink open / directory creation
- log rotation / retention / compression
- process-wide logger
- async logging
- retry 実行 / requeue

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- receive loop の継続運用向けログ範囲を必要になった時点で整理する

### TODO更新
- 現在位置に send error JSON Lines 出力範囲整理完了を反映した。
- 直近でやることから send error JSON Lines 出力範囲整理を外し、receive loop 継続運用向けログ範囲整理へ更新した。
- net-core / server 境界に `ServerSendErrorLogOutputBoundary` / send error JSON Lines writer placeholder 追加完了を反映した。
- ログ / 計測に send error JSON Lines 出力範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server send_error` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- packet 送信継続 loop 本体の実装範囲を docs に明記した。
- `crates/net-core` に send loop lifecycle placeholder を追加した。
- queue dequeue / encode / socket send / send log / retry defer の責務分離を整理した。

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- continuous send loop 本体の最小範囲は、dequeue status を見て stop / wait / process-one-item を決め、1 item だけを既存 tick boundary へ渡すところまでとする。
- socket send 後の retryable failure は `RetryDeferred` として扱い、retry 実行、retry budget、timer、requeue は今回の範囲に含めない。
- lifecycle boundary は queue collection、protocol encode、UDP socket send、JSON Lines writer、scheduler を持たない。
- async runtime、blocking worker loop、heartbeat / video frame 処理本体には進まない。

### 未実装 / 保留
- continuous send loop 本実装
- 実 queue collection からの dequeue
- UDP socket send の継続 loop 接続
- retry 実行 / requeue / retry budget
- send error JSON Lines 出力
- async runtime

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- send error JSON Lines 出力範囲を必要になった時点で整理する

### TODO更新
- 現在位置に packet 送信継続 loop 本体の実装範囲整理完了を反映した。
- 直近でやることから packet 送信継続 loop 本体の範囲整理を外し、send error JSON Lines 出力範囲整理へ更新した。
- net-core / server 境界に `OutboundSendLoopLifecycleBoundary` / send loop lifecycle placeholder 追加完了を反映した。
- net-core / server 境界に packet 送信継続 loop 本体の実装範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-net-core send_loop` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- packet 送信継続 loop の最小接続範囲を docs に明記した。
- `crates/net-core` に one-tick send loop placeholder を追加した。
- queue storage / encoder handoff / socket send / send error logging の責務分離を整理した。

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- continuous send loop 本体ではなく、queue-selected item を 1 tick で encode 計画へ渡す範囲だけを扱う。
- `OutboundSendLoopTickBoundary` は `OutboundQueueSendHandoff` から `OutboundEncodeRequest` と send log context を作る。
- encode success / encode failure / socket send success / socket send failure は state 名と `SendLogEvent` 候補として観測する。
- socket send 実行、retry、requeue、blocking loop、async runtime、log writer はこの boundary の責務に含めない。

### 未実装 / 保留
- continuous send loop 本体
- queue collection からの dequeue 実装
- UDP socket send の loop 接続
- retry 実行 / requeue
- send error JSON Lines 出力
- async runtime

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- packet 送信継続 loop 本体の実装範囲を必要になった時点で整理する

### TODO更新
- 現在位置に packet 送信継続 loop の最小接続範囲整理完了を反映した。
- 直近でやることを packet 送信継続 loop 本体の範囲整理へ更新した。
- net-core / server 境界に `OutboundSendLoopTickBoundary` / send loop tick state placeholder 追加完了を反映した。
- net-core / server 境界に packet 送信継続 loop の最小接続範囲整理完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-net-core send_loop` と `cargo test -p stream-sync-net-core outbound_queue` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- outbound queue の実キュー実装範囲を、送信継続 loop 前提で docs に明記した。
- `crates/net-core` に queue storage state / push decision の最小 placeholder を追加した。
- `apps/server` の `ServerOutboundQueueBoundary` から storage push plan を確認できる helper を追加した。
- queue storage / admission / encoder handoff / socket send loop の責務分離を整理した。

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- outbound queue storage は protocol encode 前の typed `OutboundQueueItem` を保持する。
- 送信継続 loop 前に必要な実キュー範囲は、bounded storage、admission、FIFO-compatible ordering、one-item dequeue handoff までとする。
- queue は protocol encode、encoded byte 検査、UDP socket send、retry 実行、ログ出力を持たない。
- admission は receive / handler path を block せず、現在長と capacity policy から即時 decision を返す。
- encoder handoff は queue が選んだ 1 item を `OutboundPacketEncoderBoundary` へ渡す境界とする。

### 未実装 / 保留
- 実 `VecDeque` などの queue collection
- FIFO / per-destination / per-class ordering の実装
- dequeue loop / continuous send loop
- retry 実行と queue 再投入
- send error ログ出力
- async runtime

### 次にやる候補
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する
- packet 送信継続 loop の最小接続範囲を必要になった時点で整理する

### TODO更新
- 現在位置に outbound queue の bounded storage / encoder handoff 範囲整理完了を反映した。
- 直近でやることから outbound queue 実キュー範囲の再確認を外した。
- net-core / server 境界に `OutboundQueueStorageState` / `OutboundQueueStorageBoundary` placeholder 追加完了を反映した。
- net-core / server 境界に outbound queue 実キュー実装範囲の再確認完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-net-core outbound_queue` と `cargo test -p stream-sync-server outbound_queue` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- `ServerNotice` notice trigger policy の実装範囲を docs に明記した。
- `apps/server` に `ServerNoticeTriggerPolicyBoundary` / trigger input / trigger source / trigger plan placeholder を追加した。
- server state transition / notice generation / outbound handoff の責務分離を整理した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- trigger policy boundary は、明示的な trigger source を `NoticeType` に写像するだけに限定する。
- trigger source は `Warning`, `Disconnect`, `ProtocolError`, `AuthExpired`, `ServerShutdown` を最初の placeholder 範囲とする。
- state transition handler が将来 trigger source を作る。trigger policy boundary は状態検知しない。
- `ServerNoticeTriggerPlan` は `ServerNoticeInput` を保持し、既存 `ServerNoticeBoundary` へ渡せる形にする。
- trigger policy boundary は重複抑制、rate limit、queue 投入、encode、socket send、ログ出力を行わない。

### 未実装 / 保留
- state transition 検知
- duplicate suppression / rate limit
- trigger から outbound queue までの運用接続
- continuous send loop / UDP socket send 接続
- notice log output

### 次にやる候補
- outbound queue の実キュー実装範囲を送信継続 loop 前に再確認する
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- `ServerNotice` trigger の state transition 接続範囲を必要になった時点で再確認する

### TODO更新
- 現在位置に `ServerNotice` notice trigger policy 範囲整理完了を反映した。
- 直近でやることから notice trigger policy 範囲整理を外した。
- 仕様 / 設計に `ServerNotice` notice trigger policy 範囲整理完了を追加した。
- net-core / server 境界に `ServerNoticeTriggerPolicyBoundary` / trigger plan placeholder 追加完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-server server_notice` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- secret store / token rotation 方針を docs に明記した。
- `crates/config` に future secret store 参照型と token rotation policy placeholder を追加した。
- `apps/server` の secret resolver / auth decision / rotation boundary に future secret store と rotation placeholder の扱いを追加した。
- inline token / `shared_token_env` / future secret store の責務分離を整理した。

### 変更ファイル
- `crates/config/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`
- `configs/examples/server.example.toml`
- `configs/examples/server.env-token.example.toml`

### 決定事項
- `shared_token` は PoC 用 inline placeholder として維持する。
- `shared_token_env` は現行の推奨参照で、`ServerSecretResolverBoundary` が環境変数を読む。
- future secret store は `SharedTokenSecretRef::SecretStore` / `SecretStoreSecretRef` の参照型だけ追加し、provider 連携は実装しない。
- secret store 参照は `store_id`, `secret_id`, optional `version` を持つが、これらは token material ではない。
- MVP の token rotation は disabled とし、将来の manual overlap window だけ placeholder として残す。
- rotation は UDP wire protocol / `AuthRequest` payload を変更しない方針とする。

### 未実装 / 保留
- secret store provider 連携
- secret store TOML parsing
- token hashing / KDF
- token rotation 実行
- hot reload / caching / background refresh
- 複数 token material の同時比較

### 次にやる候補
- `ServerNotice` notice trigger policy の実装範囲を整理する
- outbound queue の実キュー実装範囲を送信継続 loop 前に再確認する
- auth / receive JSON Lines file sink の実 file open 範囲を必要になった時点で再確認する
- secret store provider 連携または token rotation 実行範囲を必要になった時点で再確認する

### TODO更新
- 現在位置に secret store / token rotation 方針整理完了を反映した。
- 直近でやることから secret store / rotation 方針整理を外した。
- 認証まわりの `secret store 連携や token hashing / rotation 方針を設計する` を完了にした。
- future secret store 参照と token rotation policy placeholder の完了項目を追加した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-config secret`, `cargo test -p stream-sync-config secret_store`, `cargo test -p stream-sync-config token_rotation`, `cargo test -p stream-sync-server secret`, `cargo test -p stream-sync-server secret_store`, `cargo test -p stream-sync-server rotation` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- auth / receive JSON Lines file sink 方針を docs に明記した。
- `crates/logging` に JSON Lines sink config / plan の最小 placeholder を追加した。
- `apps/server` に auth result と receive rejection の sink plan boundary を追加した。
- stderr 出力、file sink plan、future logging 基盤の責務分離を整理した。

### 変更ファイル
- `crates/logging/src/lib.rs`
- `apps/server/Cargo.toml`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- stderr は PoC / one-shot の既定 sink として維持する。
- file sink は現時点では config / plan placeholder までとし、実 file open は行わない。
- auth result と receive rejection は別 file path を持てる方針にする。
- file sink の将来実装は append-create を基本とする。
- rotation、retention、compression、directory creation、async logging、process-wide logger は future work とする。
- schema-specific writer は引き続き caller-owned `io::Write` に 1 JSON object + newline を書く。

### 未実装 / 保留
- TOML からの logging sink 設定読み込み
- 実 file open / directory creation
- log rotation / retention / compression
- async logging / buffering policy
- process-wide logger
- auth / receive 以外の共通ログイベント型への統合

### 次にやる候補
- secret store / token rotation 方針を整理する
- `ServerNotice` notice trigger policy の実装範囲を整理する
- outbound queue の実キュー実装範囲を送信継続 loop 前に再確認する
- file sink の実 file open 範囲を必要になった時点で再確認する

### TODO更新
- 現在位置に auth / receive JSON Lines file sink 方針の整理完了を反映した。
- 直近でやることから file sink 方針整理を外し、実 file open の再確認を後続候補へ移した。
- ログ / 計測の `auth / receive JSON Lines の file sink 設定方針を整理する` を完了にした。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-logging` と `cargo test -p stream-sync-server json_lines_sink` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- `ServerNotice` payload encode/decode の最小実装範囲を確認し、`crates/protocol` に実装した。
- `ProtocolMessageEncoderBoundary` と `decode_payload_by_message_type` の `ServerNotice` 対応を追加した。
- protocol / server / outbound notice handoff の責務分離を docs に反映した。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ServerNotice` payload の最小実装範囲は `run_id` string、`notice_type` u16 little-endian、`message` string の encode/decode までとする。
- unknown `notice_type` は `ProtocolError::UnsupportedNoticeType` として扱う。
- `protocol` は wire 変換と decoder dispatch / encoder boundary までを持つ。
- `server` は typed outbound notice handoff までを持ち、通知発火 policy、継続送信 loop、UDP socket send、ログ出力は持たない。

### 未実装 / 保留
- notice trigger policy
- continuous send loop / UDP socket send 接続
- notice log output
- `ServerNotice` をどの server state transition で発火するかの詳細化

### 次にやる候補
- auth / receive JSON Lines file sink 方針を整理する
- secret store / token rotation 方針を整理する
- `ServerNotice` notice trigger policy の実装範囲を整理する
- outbound queue の実キュー実装範囲を、送信継続 loop 着手前に再確認する

### TODO更新
- 現在位置から `ServerNotice` payload encode/decode 本体の未実装記述を外した。
- 直近でやることを `ServerNotice` payload encode/decode 確認から notice trigger policy の実装範囲整理へ更新した。
- protocol / wire format に `ServerNotice` payload encode/decode と encoder / decode dispatch 対応の完了を反映した。

### メモ
- `cargo fmt --check` は成功した。
- `cargo check --workspace` は成功した。
- 追加確認として `cargo test -p stream-sync-protocol server_notice` と `cargo test -p stream-sync-server server_notice` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- `ServerNotice` payload layout と decode / encode 方針を docs に明記した。
- `ServerNotice` payload は fixed header + `run_id` string + `notice_type` u16 + `message` string とする方針にした。
- `crates/protocol` に `SERVER_NOTICE_TYPE_LEN`, `NoticeType::wire_code`, `ServerNoticePayloadPlanBoundary` を追加した。
- `apps/server` に `ServerNoticeBoundary` / `ServerOutboundNotice` と outbound queue handoff helper を追加した。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ServerNotice` の destination は payload ではなく net send layer の destination metadata で保持する。
- payload は `run_id`, `notice_type`, `message` の順にする。
- `notice_type` は `u16 little-endian` とし、`Warning = 1`, `Disconnect = 2`, `ProtocolError = 3`, `AuthExpired = 4`, `ServerShutdown = 5` とする。
- `message` は人間向けの短い説明であり、機械処理は `notice_type` を基準にする。
- 現時点では payload plan と server outbound handoff までに留め、実 encode/decode は次以降に残す。

### 未実装 / 保留
- `ServerNotice` payload encode/decode 本体
- `ProtocolMessageEncoderBoundary` の `ServerNotice` encode 対応
- `decode_payload_by_message_type` の `ServerNotice` decode 対応
- notice trigger policy
- continuous send loop / UDP send / notice log output

### 次にやる候補
- auth / receive JSON Lines file sink 方針を整理する
- secret store / token rotation 方針を整理する
- `ServerNotice` payload encode/decode 最小実装範囲を確認する

### TODO更新
- 現在位置に `ServerNotice` payload layout と decode / encode 方針の整理完了を反映した。
- 直近でやることを `ServerNotice` payload 方針決定から payload encode/decode 最小実装範囲の確認へ更新した。
- 仕様 / 設計、protocol / wire format、net-core / server 境界に今回の完了項目を反映した。

### メモ
- `cargo fmt --check` と `cargo check --workspace` は今回の変更後に成功した。
- 追加確認として `cargo test -p stream-sync-protocol server_notice_payload_plan` と `cargo test -p stream-sync-server server_notice` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- outbound queue の実処理範囲を、bounded in-memory handoff / admission policy / one-item lifecycle までに限定して docs に明記した。
- backpressure 方針として、bounded capacity、non-blocking admission、control drop-incoming、time-sensitive video drop-oldest-then-accept、telemetry drop-incoming を整理した。
- `crates/net-core` に queue admission / capacity / drop policy の placeholder 型を追加した。
- `apps/server` の `ServerOutboundQueueBoundary` から admission policy を評価できる最小 helper を追加した。

### 変更ファイル
- `crates/net-core/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- outbound queue は protocol encode 前の typed `OutboundQueueItem` を扱う。
- protocol encode は net send layer の責務であり、queue は `protocol::MessageEncoder` を直接呼ばない。
- UDP socket send は socket send layer の責務であり、queue は `send_to` を呼ばない。
- queue pressure は受信 / handler path を block せず、即時の admission decision として扱う。
- MVP 初期 placeholder capacity は `max_items = 64` とし、実運用値のチューニングは future queue 実装時に再確認する。

### 未実装 / 保留
- 実キュー collection / FIFO ordering / per-destination queue
- packet 送信継続 loop
- retry 実行
- send error log output
- queue admission decision から実際に item を evict / drop する処理

### 次にやる候補
- auth / receive JSON Lines file sink 方針を整理する
- secret store / token rotation 方針を整理する
- `ServerNotice` payload layout と decode / encode 方針を決める

### TODO更新
- 現在位置に outbound queue の実処理範囲と backpressure / capacity 方針の整理完了を反映した。
- 直近でやることから今回完了した outbound queue 方針整理を外し、`ServerNotice` payload 方針を次候補に上げた。
- 仕様 / 設計と net-core / server 境界の backpressure / capacity 方針項目を完了にした。

### メモ
- `cargo fmt --check` と `cargo check --workspace` は今回の変更後に成功した。
- 追加確認として `cargo test -p stream-sync-net-core outbound_queue_admission` と `cargo test -p stream-sync-server outbound_queue_boundary_exposes_capacity_policy_for_handoff_items` も成功した。

---

## 2026-04-21
### 種別
- Codex

### 今回の作業
- `ClientStats` receive route / handler 接続方針を docs に明記した。
- `apps/server` に `ClientStats` route / gate / registered handler bridge を追加した。
- decoded `ClientStats` の optional heartbeat observation を server timebase 入力形へ変換する最小 boundary を追加した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ClientStats` は `Heartbeat` / `VideoFrame` と同じ client-scoped inbound packet として扱う。
- `ServerInboundRouter` は `ProtocolMessage::ClientStats` を `ServerInboundRoute::ClientStats` へ分類する。
- `PacketAcceptanceGateBoundary` は `ClientStats.client_id` と source endpoint を `AuthenticatedSenderRegistry` で検証する。
- `ServerRegisteredPacketBoundary` は accepted `ClientStats` に authenticated sender を付与して handler 境界へ渡す。
- `ServerClientStatsHandlerBoundary` は stats fields と optional heartbeat observation を抽出するだけで、metrics state commit や RTT / offset state commit は行わない。

### 未実装 / 保留
- `ClientStats` の client 継続送信 loop
- server metrics state commit
- heartbeat observation を使った RTT / offset state commit と smoothing
- receive loop の継続運用、drop/log 本体、async runtime

### 次にやる候補
- auth / receive JSON Lines file sink 方針を整理する
- secret store / token rotation 方針を整理する
- outbound queue の実処理範囲と backpressure 方針を詰める

### TODO更新
- 現在位置に `ClientStats` receive route / gate / registered handler bridge 完了を反映した。
- 直近タスクから `ClientStats` receive route / handler 接続を外した。
- net-core / server 境界と heartbeat / 時刻同期の完了項目に今回の bridge を追加した。

### メモ
- `ClientStats` の継続送信 loop、heartbeat / video frame 処理本体、secret store 連携、async runtime は今回の範囲外。

---

## 2026-04-20
### 種別
- Codex

### 今回の作業
- `ClientStats` payload encode/decode の最小実装を追加した。
- heartbeat observation optional block を含む `ClientStats` wire 変換を追加した。
- `ProtocolMessageEncoderBoundary` から `ProtocolMessage::ClientStats` を encode できるようにした。
- docs と TODO を現在の実装範囲に合わせて更新した。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `apps/client/src/lib.rs`
- `apps/server/src/lib.rs`
- `docs/architecture/protocol.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ClientStats` payload は `client_id`, `run_id`, `sent_at`, `capture_fps`, `dropped_frames`, `bitrate_kbps`, `heartbeat_observation_present` の順に encode/decode する。
- `heartbeat_observation_present = 1` の場合だけ `echoed_sent_at`, `server_received_at`, `server_sent_at`, `client_received_at` を `u64 little-endian` で続ける。
- `heartbeat_observation_present = 0` の場合は observation を `None` とする。
- present tag が `0` / `1` 以外なら `InvalidOptionalTag` とする。

### 未解決事項
- `ClientStats` の client 継続送信 loop
- server receive route / gate / handler への `ClientStats` 接続
- heartbeat observation を使った RTT / offset state commit と smoothing
- `ServerNotice` payload layout / encode / decode

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- `ClientStats` receive route / handler 接続を設計する

### TODO更新
- 完了:
  - `ClientStats` payload encode/decode の最小実装
  - heartbeat observation optional block の wire 変換
  - `ProtocolMessageEncoderBoundary` の `ClientStats` encode 対応
- 追加:
  - `ClientStats` receive route / handler 接続を次候補へ移動
- 保留:
  - 継続 stats 送信
  - RTT / offset state commit
  - `ServerNotice` payload encode/decode

### メモ
- `cargo test -p stream-sync-protocol client_stats`、`cargo fmt --check`、`cargo check --workspace` が通ることを確認した。

---

## 2026-04-20
### 種別
- Codex

### 今回の作業
- `ClientStats` payload encode / decode 方針を決めた。
- heartbeat observation optional block を含む `ClientStats` payload 順序を docs に明記した。
- `crates/protocol` に `ClientStatsPayloadPlanBoundary` と payload length constants を追加した。
- `ClientStats` 型に optional `heartbeat_observation` を追加し、wire 実装前の payload plan を確認できるようにした。

### 変更ファイル
- `crates/protocol/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `ClientStats` payload は `client_id`, `run_id`, `sent_at`, `capture_fps`, `dropped_frames`, `bitrate_kbps`, `heartbeat_observation_present` の順にする。
- `heartbeat_observation_present = 1` の場合だけ、`echoed_sent_at`, `server_received_at`, `server_sent_at`, `client_received_at` を `u64 little-endian` で続ける。
- `heartbeat_observation_present = 0` の場合は optional block を書かない。
- decode 時、present tag が `0` / `1` 以外なら `InvalidOptionalTag` とする方針にする。
- 今回は payload plan までで、`ProtocolMessageEncoderBoundary` の `ClientStats` encode / decode 本実装はまだ行わない。

### 未解決事項
- `ClientStats` payload encode / decode 最小実装
- `ClientStats` receive route / gate / handler 接続
- heartbeat observation を使った RTT / offset state commit
- `ServerNotice` payload layout / encode / decode 方針

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- `ClientStats` payload encode/decode 最小実装を行う

### TODO更新
- 完了:
  - `ClientStats` payload encode/decode 方針決定
  - heartbeat observation optional block を含む payload 順序 docs 反映
  - `ClientStatsPayloadPlanBoundary` placeholder 追加
- 追加:
  - `ClientStats` payload encode/decode 最小実装を次候補へ移動
- 保留:
  - `ClientStats` payload encode/decode 本実装
  - `ClientStats` receive route 接続
  - RTT / offset state commit

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-protocol client_stats_payload_plan` が通ることを確認した。

---

## 2026-04-20
### 種別
- Codex

### 今回の作業
- heartbeat observation carrier を設計した。
- `HeartbeatAckObservation` を `ClientStats` carrier に載せる typed boundary を追加した。
- `apps/client` に observation を future `ClientStats` carrier へ wrap する boundary を追加した。
- `apps/server` に future carrier から server calculator input を取り出す boundary を追加した。
- docs に `ClientStats` optional heartbeat observation block の payload 方針と責務分離を追記した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/server/src/lib.rs`
- `crates/protocol/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- heartbeat observation carrier は `ClientStats` の optional block として扱う方針にする。
- optional block は `heartbeat_observation_present: u8` の後ろに `echoed_sent_at`, `server_received_at`, `server_sent_at`, `client_received_at` を `u64 little-endian` で置く。
- `client_id` と `run_id` は `ClientStats` 共通 field を使う。
- 今回は typed carrier のみで、`ClientStats` payload encode / decode や UDP send/receive 接続は実装しない。

### 未解決事項
- `ClientStats` payload encode / decode 本実装
- `ClientStats` receive route / gate / handler 接続
- heartbeat observation を継続送信する client loop
- server 側 RTT / offset state commit と smoothing

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- `ClientStats` payload encode/decode 方針を決める

### TODO更新
- 完了:
  - heartbeat observation carrier 設計
  - `HeartbeatAckObservation` を `ClientStats` carrier に載せる typed boundary 追加
  - `ClientStats` optional observation block の payload 方針 docs 反映
- 追加:
  - `ClientStats` payload encode/decode 方針を次候補へ移動
- 保留:
  - observation の wire encode / decode
  - continuous heartbeat loop
  - RTT / offset の state commit

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-protocol heartbeat_observation_carrier`、`cargo test -p stream-sync-client heartbeat_observation_carrier`、`cargo test -p stream-sync-server heartbeat_observation_carrier` が通ることを確認した。

---

## 2026-04-20
### 種別
- Codex

### 今回の作業
- heartbeat client ack observation flow を設計した。
- `crates/protocol` に `HeartbeatAck` と `client_received_at` から `HeartbeatAckObservation` を作る typed boundary を追加した。
- `apps/client` に client 側で `HeartbeatAckObservation` を作る boundary を追加した。
- `apps/server` に protocol-level observation を server calculator input へ変換する boundary を追加し、calculator 境界で server timestamps も照合するようにした。
- docs に client / protocol / server / timebase の責務分離と、wire carrier を今後決める方針を追記した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/server/src/lib.rs`
- `crates/protocol/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `client_received_at` は client が `HeartbeatAck` を受信した直後に client clock domain で観測する。
- client は `HeartbeatAckObservation` に `client_id`, `run_id`, `echoed_sent_at`, `server_received_at`, `server_sent_at`, `client_received_at` を保持する。
- server は observation を stored `ServerHeartbeatTimebasePlan` と照合してから RTT / offset calculator に渡す。
- observation の wire carrier は今回固定しない。候補は `ClientStats` extension または dedicated observation message とし、現時点では encode / decode しない。

### 未解決事項
- heartbeat observation carrier の payload layout / decode / encode
- client から server へ observation を送る実処理
- server receive loop で observation を route / gate / handler へ接続する処理
- RTT / offset state commit と smoothing

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- heartbeat observation carrier を設計する

### TODO更新
- 完了:
  - heartbeat client ack observation flow 設計
  - `HeartbeatAckObservation` boundary 追加
  - server observation と timebase plan の照合方針反映
- 追加:
  - heartbeat observation carrier 設計を次候補へ移動
- 保留:
  - observation の wire encode / decode
  - continuous heartbeat loop
  - RTT / offset の state commit

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-protocol heartbeat_ack_observation`、`cargo test -p stream-sync-client heartbeat_ack_observation`、`cargo test -p stream-sync-server heartbeat_client_ack_observation` が通ることを確認した。

---

## 2026-04-20
### 種別
- Codex

### 今回の作業
- heartbeat RTT / offset の小さな実計算単位を決めた。
- `crates/timebase` に four-timestamp exchange を入力にした stateless RTT / offset calculator を追加した。
- `apps/server` に `ServerHeartbeatTimebasePlan` と future client ack observation を照合して calculator へ渡す boundary を追加した。
- docs に state input / timebase input / plan / minimal calculation unit / future estimator state の責務分離を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `crates/timebase/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- 最小実計算単位は `client_sent_at`, `server_received_at`, `server_sent_at`, `client_received_at` の 4 timestamp exchange とする。
- `rtt = (client_received_at - client_sent_at) - (server_sent_at - server_received_at)` とする。
- `clock_offset = ((server_received_at - client_sent_at) + (server_sent_at - client_received_at)) / 2` とし、server clock minus client clock として扱う。
- この単位は stateless helper に留め、smoothing、履歴、outlier policy、timeout、補正後 timestamp 生成は future estimator state に残す。

### 未解決事項
- client ack receive observation を protocol / client / server flow でどう返すか
- RTT / offset の per-client state 更新
- smoothing / outlier handling
- heartbeat timeout と sync-core への補正時刻接続

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- heartbeat client ack observation flow を設計する

### TODO更新
- 完了:
  - heartbeat RTT / offset の小さな実計算単位決定
  - four-timestamp exchange の stateless calculator 追加
  - server plan と future client ack observation の calculation boundary 追加
- 追加:
  - heartbeat client ack observation flow を次候補へ移動
- 保留:
  - RTT / offset の大きな完成実装
  - smoothing / per-client state
  - async runtime

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-timebase heartbeat_rtt_offset`、`cargo test -p stream-sync-server heartbeat_rtt_offset` が通ることを確認した。

---

## 2026-04-20
### 種別
- Codex

### 今回の作業
- heartbeat state / RTT / offset 推定の本計算方針を整理した。
- `crates/timebase` に heartbeat timebase sample から RTT / offset / smoothing の計算 plan を作る placeholder を追加した。
- `apps/server` に heartbeat timebase input から timebase plan へ橋渡しする `ServerHeartbeatTimebasePlanBoundary` を追加した。
- docs に state input / timebase input / timebase plan / 将来の計算層の責務分離を追記した。

### 変更ファイル
- `apps/server/Cargo.toml`
- `apps/server/src/lib.rs`
- `crates/timebase/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- RTT は server 側の heartbeat 受信 sample だけでは完了しないため、`RequiresClientAckObservation` として client 側 ack 観測待ちの plan にする。
- offset は `Heartbeat.local_time` がある場合だけ候補化し、delay / RTT 補償を future estimator に残す。
- `local_time` がない heartbeat では `MissingClientLocalTime` とし、offset 更新を試みない。
- smoothing は `Deferred` とし、平滑化係数、外れ値処理、warm-up、per-client estimate state は future timebase calculation layer に残す。

### 未解決事項
- RTT completion の実計算
- delay compensation を含む clock offset 推定
- offset smoothing / outlier handling
- heartbeat state / timeout 更新

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- heartbeat RTT / offset の小さな実計算単位を決める

### TODO更新
- 完了:
  - heartbeat state / RTT / offset 推定の本計算方針整理
  - `HeartbeatTimebaseEstimatePlan` / `HeartbeatTimebasePlanBoundary` 追加
  - server heartbeat timebase input から timebase plan への bridge 追加
- 追加:
  - heartbeat RTT / offset の小さな実計算単位を次候補へ移動
- 保留:
  - RTT / offset 本計算
  - heartbeat state 更新
  - async runtime

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-timebase heartbeat_timebase_plan`、`cargo test -p stream-sync-server heartbeat_input_boundary` が通ることを確認した。

---

## 2026-04-20
### 種別
- Codex

### 今回の作業
- heartbeat state / RTT / offset 推定へ渡す入力境界を整理した。
- `apps/server` に `ServerHeartbeatInputBoundary`, `ServerHeartbeatProcessingInputs`, `ServerHeartbeatStateInput`, `ServerHeartbeatTimebaseInput` を追加した。
- registered heartbeat packet と explicit ack timing から state input / timebase input を作り、`ServerHeartbeatAckHandoff` に同梱するようにした。
- docs に registered heartbeat packet / ack timing / heartbeat state input / timebase input の責務分離を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- state input は生存確認 / timeout 管理の将来入力として、source、authenticated sender、client/run/protocol、heartbeat sent_at、server_received_at、short_status を保持する。
- timebase input は RTT / offset 推定の将来入力として、client_sent_at、client_local_time、server_received_at、server_sent_at を保持する。
- `ServerHeartbeatInputBoundary` は入力 shape を作るだけで、state mutation、RTT / offset 計算、平滑化、timeout 判定は行わない。
- `crates/timebase` の本計算は今回も未実装に残す。

### 未解決事項
- heartbeat state / timeout 更新の本実装
- RTT / offset 推定と smoothing の本実装
- auth / receive JSON Lines の file sink 設定方針
- secret store 連携や token rotation 方針

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- heartbeat state / RTT / offset 推定の本計算方針を整理する

### TODO更新
- 完了:
  - heartbeat state / RTT / offset 推定の入力境界整理
  - `ServerHeartbeatInputBoundary` / state input / timebase input placeholder 追加
  - registered heartbeat packet / ack timing / timebase 入力の責務分離更新
- 追加:
  - heartbeat state / RTT / offset 推定の本計算方針
- 保留:
  - RTT / offset 本計算
  - heartbeat state 更新
  - async runtime
  - secret store 連携

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-server heartbeat_input_boundary`、`cargo test -p stream-sync-server heartbeat_handler_handoff` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- heartbeat handler の最小 ack 接続範囲を整理した。
- `apps/server` に `ServerHeartbeatHandlerBoundary`, `ServerHeartbeatAckTiming`, `ServerHeartbeatAckHandoff` を追加した。
- registered heartbeat packet から `ServerHeartbeatAckInput`、typed `HeartbeatAck`、`OutboundQueueItem` までをつなぐ bridge を追加した。
- docs に receive loop / gate / registered packet / heartbeat handler / ack queue handoff の責務分離を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- heartbeat handler boundary は registered heartbeat packet と explicit timing input から ack handoff を作る。
- `Heartbeat.sent_at` を `HeartbeatAck.echoed_sent_at` として返す。
- `server_received_at` / `server_sent_at` は handler 内で時計を読まず、外から渡された `ServerHeartbeatAckTiming` を使う。
- heartbeat state 更新、timeout、RTT / offset 計算、UDP send 実行、queue runtime は今回も未実装に残す。

### 未解決事項
- heartbeat state / timeout 管理
- RTT / offset 推定の入力境界
- continuous receive loop への heartbeat handler 接続
- auth / receive JSON Lines の file sink 設定方針
- secret store 連携や token rotation 方針

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- heartbeat state / RTT / offset 推定の入力境界を整理する

### TODO更新
- 完了:
  - heartbeat handler の最小 ack 接続範囲整理
  - `ServerHeartbeatHandlerBoundary` / `ServerHeartbeatAckHandoff` placeholder 追加
  - registered heartbeat packet から `HeartbeatAck` outbound queue handoff までの docs 反映
- 追加:
  - heartbeat state / RTT / offset 推定の入力境界
- 保留:
  - heartbeat state 更新
  - RTT / offset 本計算
  - async runtime
  - UDP send loop

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-server heartbeat_handler_handoff` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- heartbeat / video frame handler へ registered packet を渡す接続方針を整理した。
- `apps/server` に `ServerRegisteredPacketBoundary`, `ServerRegisteredClientPacket`, `ServerRegisteredHeartbeatPacket`, `ServerRegisteredVideoFramePacket` を追加した。
- accepted route と authenticated sender registry から、handler 用の decoded message + authenticated sender binding を作る bridge を追加した。
- docs に receive loop / gate / registry / registered packet boundary / handler の責務分離を追記した。

### 変更ファイル
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/architecture/protocol.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- packet acceptance gate は accept / reject の判定までを担当し、handler input は作らない。
- registered packet boundary が `Heartbeat` / `VideoFrame` route に `AuthenticatedSenderEntry` を添えて handler input にする。
- `AuthRequest` と unsupported route は registered client packet boundary では `NotClientScoped` とする。
- heartbeat state 更新、RTT / offset 計算、`HeartbeatAck` queue handoff、video frame buffering は今回も未実装に残す。

### 未解決事項
- heartbeat handler の最小 ack 接続
- video frame handler の最小 buffer handoff
- auth / receive JSON Lines の file sink 設定方針
- secret store 連携や token rotation 方針

### 次にやる候補
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する
- heartbeat handler の最小 ack 接続範囲を整理する

### TODO更新
- 完了:
  - registered packet handler handoff 方針
  - `ServerRegisteredPacketBoundary` / registered handler input placeholder 追加
  - receive loop / gate / registry / handler の責務分離更新
- 追加:
  - heartbeat handler の最小 ack 接続範囲
- 保留:
  - heartbeat / video frame 処理本体
  - async runtime
  - secret store 連携
  - file sink / rotation / retention

### メモ
- `cargo fmt --check`、`cargo check --workspace`、`cargo test -p stream-sync-server registered_packet_boundary` が通ることを確認した。

---

## 2026-04-19
### 種別
- Codex

### 今回の作業
- `shared_token_env` one-shot auth round trip を実機手動確認し、結果を repo 内 docs に記録した。
- `docs/operations/auth-roundtrip-manual-check.md` に実行コマンド、server 環境変数、client / server の観測結果を追記した。
- env-token helper config では `player1` から `player4` までの token reference を resolver がまとめて解決するため、4 つすべての `STREAMSYNC_PLAYER*_TOKEN` を設定する必要があることを手順へ反映した。

### 変更ファイル
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 決定事項
- `configs/examples/server.env-token.example.toml` を使う accepted path 確認では、server 起動ターミナルに `STREAMSYNC_PLAYER1_TOKEN` から `STREAMSYNC_PLAYER4_TOKEN` までを設定する。
- 成功確認は server stdout の `accepted=true reason_code=Ok` と、server stderr の `server.auth_result` JSON Lines で行う。
- `STREAMSYNC_PLAYER1_TOKEN` だけの設定では、未設定 token reference が残るため `InternalError` になることを補足として残す。

### 確認結果
- 結果: 成功
- client stdout: `auth request PoC sent 96 bytes to 127.0.0.1:5000; client_id=player1 run_id=streamsync-dev-session protocol_version=1`
- server stdout: `auth response PoC handled one packet on 0.0.0.0:5000 and sent 55 bytes; client_id=player1 run_id=streamsync-dev-session accepted=true reason_code=Ok`
- server stderr: `server.auth_result` JSON Lines で `accepted=true`, `reason_code=Ok`

### 未解決事項
- heartbeat / video frame handler へ accepted route を渡す接続
- auth / receive JSON Lines の file sink 設定方針
- secret store 連携や token rotation 方針

### 次にやる候補
- heartbeat / video frame handler へ registered packet を渡す接続方針を整理する
- auth / receive JSON Lines の file sink 設定方針を整理する
- secret store 連携や token rotation の方針を整理する

### TODO更新
- 完了:
  - `shared_token_env` one-shot auth round trip accepted path の実機確認
  - 実行コマンド、環境変数設定、観測結果の記録
  - env-token helper config の必要 env var 補足
- 追加:
  - なし
- 保留:
  - secret store 連携
  - token rotation
  - heartbeat / video frame 処理本体
  - file sink / rotation / retention

### メモ
- `cargo build -p stream-sync-server -p stream-sync-client` 後に server / client one-shot を実行した。
- `cargo fmt --check` と `cargo check --workspace` が通ることを確認した。

---

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
## 2026-04-23
### 担当
- Codex

### 今回の作業
- client one-tick heartbeat runtime の CLI / config 入口を追加した。
- accepted auth 後に one-tick runtime を 1 回だけ起動する minimal launcher を追加した。
- `--receive-send-twice` / `--receive-send-three` と組み合わせる手動確認手順を docs に追記した。

### 変更ファイル
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/auth-roundtrip-manual-check.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### 実装したこと
- `ClientHeartbeatOneTickRuntimeMode`
- `ClientHeartbeatOneTickRuntimeStartupConfig`
- `ClientHeartbeatOneTickRuntimeOutcome`
- `ClientHeartbeatOneTickRuntimeLauncher`
- `run_auth_heartbeat_one_tick_runtime_from_path`
- `run_auth_heartbeat_stats_one_tick_runtime_from_path`
- client config から `network.heartbeat_interval_ms` を読み取り、one-tick runtime cadence / retry delay へ接続
- client CLI に `--auth-heartbeat-one-tick-runtime` と `--auth-heartbeat-stats-one-tick-runtime` を追加
- launcher config load test と auth + one heartbeat tick の最小 socket test を追加

### 未実装 / 保留
- completed continuous heartbeat loop
- 実 sleep / timer 実行
- reconnect / repeated retry execution
- JSON Lines writer invocation / file sink open / process-wide logger
- shutdown cleanup / final flush
- accepted path の実機 manual run 結果記録

### 次にやる候補
- heartbeat timeout notice wakeup 実行本体に進む前の境界整理
- RTT / offset metrics snapshot の export cadence / dashboard refresh 方針整理
- client one-tick runtime accepted path の実機 manual check と launcher / repeated-loop ownership 整理

### TODO 更新
- 現在位置に client one-tick heartbeat runtime CLI / config 入口の完了を反映した。
- 直近でやることを accepted path manual check と launcher / repeated-loop ownership 整理へ更新した。
- client / 検証タスクに one-tick launcher / CLI / config 完了と関連単体テスト完了を追加した。

### 検証
- `cargo fmt`
- `cargo test -p stream-sync-client client_heartbeat_one_tick_runtime_launcher`
- `cargo test -p stream-sync-client client_heartbeat_loop_one_tick_runtime`
- `cargo fmt --check`
- `cargo check --workspace`

---
---

## 2026-04-24
### Type
- Codex

### Work
- Defined the minimal client-side RTT / offset metrics state commit boundary for the continuous heartbeat loop.
- Added commit input derivation from `HeartbeatAckObservation`, `ClientStats.heartbeat_observation`, and `ClientHeartbeatLoopOneTickRuntimeResult`.
- Added explicit commit results for applied, no commit needed, deferred, and stop passthrough.
- Kept metrics state commit separate from timer wait, retry, reconnect, socket re-establishment, metrics snapshot export cadence, and dashboard refresh.

### Changed Files
- `apps/client/Cargo.toml`
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Metrics commit input is derived only from explicit observation/state already surfaced by the heartbeat loop.
- Missing observation is represented as an explicit no-commit result.
- Missing caller-owned metrics state or invalid RTT / offset calculation is represented as deferred commit, not as retry/reconnect/timer behavior.
- Snapshot export cadence and dashboard refresh remain future boundaries, not side effects of per-sample commit.

### Unresolved
- live socket ownership wiring for the future continuous loop runner
- metrics snapshot export cadence policy
- dashboard refresh handoff policy
- video path / switcher / OBS integration

### Next
- Wire live socket ownership into the future client continuous heartbeat loop runner.
- Define metrics snapshot export cadence and dashboard refresh policy as separate boundaries.

### TODO Update
- Current focus updated to reflect completed metrics commit boundary.
- Next items reordered around live socket ownership, metrics snapshot cadence, dashboard refresh, server timeout loop, and later video/switcher/OBS work.
---

## 2026-04-24
### Type
- Codex

### Work
- Defined the minimal client-side RTT / offset metrics snapshot export cadence boundary.
- Added caller-owned cadence state with start time, last export time, export count, and last exported sample count.
- Added snapshot records and snapshot export handoff for a future dashboard refresh consumer.
- Kept metrics commit, snapshot cadence, and dashboard refresh as separate boundaries.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Snapshot cadence consumes only explicit metrics state, cadence state, current time, and configured export interval.
- Snapshot cadence does not calculate or commit RTT / offset samples.
- Snapshot cadence does not execute dashboard refresh. It only emits an explicit future dashboard refresh handoff when export is due.
- Missing metrics state, missing cadence state, zero interval, and empty metrics state are explicit deferred results.

### Unresolved
- dashboard refresh consumer policy
- live socket ownership wiring for the future continuous loop runner
- runtime wiring of snapshot cadence into the future loop owner
- video path / switcher / OBS integration

### Next
- Define dashboard refresh consumer policy as a separate boundary.
- Wire live socket ownership into the future client continuous heartbeat loop runner.

### TODO Update
- Current focus updated to include completed snapshot export cadence boundary.
- Next items reordered around dashboard refresh consumer policy, live socket ownership, and future loop runtime wiring.
---

## 2026-04-24
### Type
- Codex

### Work
- Defined the minimal client-side dashboard refresh consumer policy boundary for heartbeat RTT / offset metrics snapshots.
- Added dashboard refresh consumer input derivation from explicit future dashboard handoff / snapshot export result.
- Added explicit refresh requested, refresh skipped, and refresh deferred results.
- Kept snapshot export, refresh policy, and actual dashboard UI rendering separate.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Refresh consumer policy consumes only explicit dashboard refresh handoff or snapshot export output.
- Snapshot export not due maps to refresh skipped, not to cadence re-evaluation.
- Snapshot export deferred maps to refresh deferred with the original reason preserved.
- Actual dashboard UI rendering remains out of scope; the policy only emits a typed refresh request.

### Unresolved
- live socket ownership wiring for the future continuous loop runner
- runtime wiring of snapshot cadence into the future loop owner
- runtime wiring of dashboard refresh into the future metrics consumer owner
- video path / switcher / OBS integration

### Next
- Wire live socket ownership into the future client continuous heartbeat loop runner.
- Connect snapshot cadence and dashboard refresh policy to future caller-owned runtime state.

### TODO Update
- Current focus updated to include completed dashboard refresh consumer policy boundary.
- Next items reordered around live socket ownership and future runtime wiring.

---

## 2026-04-24
### Type
- Codex

### Work
- Defined the minimal future client continuous heartbeat loop runner with live UDP socket slot ownership.
- Added runner output for completed repeated-body execution, stop passthrough, and runner-owned error.
- Wired the runner to inject `ClientHeartbeatLoopRealUdpSocketReestablishmentHook` into `run_with_hook(...)`.
- Kept socket replacement in the hook and outside the repeated heartbeat loop body.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The runner owns an `Arc<Mutex<Option<UdpSocket>>>` socket slot.
- The repeated body receives only the existing socket re-establishment hook abstraction.
- Runner output reports socket ownership state as `has_socket` without exposing or moving the socket.
- Stop output remains an explicit passthrough from repeated body to runner result.
- Metrics cadence, dashboard refresh, video, switcher, and OBS remain out of this runner boundary.

### Unresolved
- runtime wiring of snapshot cadence into the future loop owner
- runtime wiring of dashboard refresh into the future metrics consumer owner
- server heartbeat timeout loop tick multi-client continuous execution
- video path / switcher / OBS integration

### Next
- Connect snapshot cadence and dashboard refresh policy to future caller-owned runtime state.
- Return to server heartbeat timeout loop tick multi-client continuous execution.

### TODO Update
- Current focus updated to include completed minimal runner live socket ownership wiring.
- Next items reordered around metrics snapshot cadence runtime wiring, dashboard refresh runtime wiring, server timeout loop, and later video/switcher/OBS work.

---

## 2026-04-24
### Type
- Codex

### Work
- Wired metrics snapshot export cadence into the client continuous heartbeat loop runner.
- Added caller-owned metrics/cadence runtime input for runner cadence evaluation.
- Added runner cadence runtime result that keeps loop output and snapshot export result side by side.
- Kept metrics commit, snapshot cadence, dashboard refresh policy, and repeated body responsibilities separate.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The runner borrows metrics state and receives cadence state explicitly from the caller.
- Snapshot cadence is evaluated after normal runner execution and does not alter repeated-body continuation or stop output.
- Snapshot cadence result remains the existing due / not-due / deferred enum.
- Future dashboard refresh remains an explicit handoff from snapshot export; the runner does not evaluate refresh policy or render UI.

### Unresolved
- runtime wiring of dashboard refresh into the future metrics consumer owner
- server heartbeat timeout loop tick multi-client continuous execution
- video path / switcher / OBS integration

### Next
- Wire dashboard refresh runtime handling from explicit snapshot export handoff.
- Return to server heartbeat timeout loop tick multi-client continuous execution.

### TODO Update
- Current focus updated to include completed metrics snapshot export cadence runtime wiring in the runner.
- Next items reordered around dashboard refresh runtime wiring, server timeout loop, and later video/switcher/OBS work.

---

## 2026-04-24
### Type
- Codex

### Work
- Wired dashboard refresh runtime handling into the client continuous heartbeat loop runner.
- Added a caller-owned dashboard refresh runtime sink abstraction.
- Added runtime result states for refresh applied, skipped, and deferred.
- Kept dashboard refresh runtime outside the repeated body and separate from metrics commit and snapshot cadence.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Dashboard refresh runtime consumes only dashboard refresh policy result.
- Snapshot cadence result is preserved beside dashboard refresh runtime result and is not reinterpreted by the sink boundary.
- The runner invokes the caller-owned sink only when refresh policy returns requested.
- Missing sink and sink-side deferral are explicit deferred runtime results.
- No dashboard UI rendering, dashboard storage, video, switcher, or OBS behavior is implemented.

### Unresolved
- server heartbeat timeout loop tick multi-client continuous execution
- video path / switcher / OBS integration

### Next
- Return to server heartbeat timeout loop tick multi-client continuous execution.
- Keep video / switcher / OBS integration for a later phase.

### TODO Update
- Current focus updated to include completed dashboard refresh runtime wiring in the runner.
- Next items reordered around server timeout loop and later video/switcher/OBS work.

---

## 2026-04-24
### Type
- Codex

### Work
- Added a thin server heartbeat timeout multi-client loop boundary over the existing one-client timeout tick.
- The multi-client loop snapshots authenticated client ids, runs one-client timeout tick per client, and stores timeout notice handoffs into caller-owned queue storage.
- Added explicit no-client and all-clients-processed results with per-client tick and notice queue storage details.
- Kept notice queue storage separate from notice send wakeup execution and continuous receive/send loop ownership.

### Changed Files
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Authenticated sender registry, liveness state, outbound notice queue, and timeout log writer remain caller-owned.
- The multi-client loop does not reinterpret timeout evaluation, action planning, or apply semantics from the one-client tick.
- Notice queue storage may request a future send wakeup, but the loop does not execute that wakeup.
- Video, switcher, OBS, and dashboard UI remain out of scope.

### Unresolved
- video path / switcher / OBS integration
- dashboard UI rendering
- real continuous server loop cadence / sleep / stop ownership beyond this timeout pass

### Next
- Move toward video path / switcher / OBS integration planning or implementation.
- Keep dashboard UI rendering for a later phase.

### TODO Update
- Marked server heartbeat timeout multi-client loop body complete.
- Current focus updated with the completed multi-client timeout loop boundary.
- Next items reduced to later video/switcher/OBS integration.

---

## 2026-04-24
### Type
- Codex

### Work
- Audited the current video path and chose the smallest safe first single-view PoC slice.
- Added server-side `VideoFrame` queue storage after the existing authenticated video handler input.
- Added caller-owned per-client encoded-frame queue state and a small live-video capacity policy that drops the oldest frame when full.
- Kept authentication, protocol decode/encode, frame queue storage, H.264 decode, display, switcher, and OBS responsibilities separate.

### Changed Files
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The first video PoC implementation slice is server-side queue storage, not client capture/encode or switcher display.
- The queue stores encoded `VideoFrame` data and metadata only; it does not decode H.264 or choose display frames.
- Queue state remains caller-owned so the future continuous receive/send loop can decide when to store and drain frames.
- Full switcher UI, OBS integration, 2-view/4-view sync, and dashboard UI remain out of scope.

### Unresolved
- client-side `VideoFrame` metadata construction / placeholder H.264 payload / UDP send
- receive-loop-to-video-queue runtime wiring
- single-view decode/display placeholder
- real H.264 capture/encode/decode
- 4-view sync and OBS integration

### Next
- Add client-side `VideoFrame` metadata / placeholder payload / UDP send boundary.
- Connect accepted server video handler side effect to queue storage in the future receive-loop owner.
- Add a switcher-side single-view decode/display placeholder later.

### TODO Update
- Marked server authenticated frame acceptance and per-client receive queue tasks complete.
- Updated Current Focus and Next Items to the next one-client video PoC steps.

---

## 2026-04-24
### Type
- Codex

### Work
- Added the smallest client-side one-client video send PoC slice.
- Added explicit placeholder encoded H.264 payload source handling.
- Added `VideoFrame` metadata construction from caller-owned ids, timestamps, dimensions, fps, and frame id.
- Added a protocol encode handoff and one-shot UDP send boundary using a caller-owned socket.
- Kept real capture, real H.264 encoding, server receive-loop queue wiring, switcher display, 4-view sync, and OBS out of scope.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The placeholder payload source is named explicitly and rejects empty payloads instead of hiding fake capture/encode behavior.
- Metadata construction remains separate from UDP send.
- The encode/send boundary uses the existing `ProtocolMessageEncoderBoundary` and preserves caller-owned socket and destination ownership.
- No CLI/config launcher was added in this slice; the boundary is library-level until the next runtime wiring step.

### Unresolved
- real screen capture / real H.264 encode
- server receive-loop-to-video-queue runtime wiring
- switcher single-view decode/display placeholder
- 2-view / 4-view sync and OBS integration

### Next
- Wire accepted server receive-loop video side effects into `ServerVideoFrameQueueStorageBoundary`.
- Add switcher-side single-view decode/display placeholder.
- Later replace the placeholder encoded payload source with a real capture/encode boundary.

### TODO Update
- Marked client-side `VideoFrame` metadata construction complete.
- Marked explicit placeholder encoded H.264 payload source complete.
- Marked client-side `VideoFrame` UDP send complete for the placeholder-payload PoC.
- Reordered Next Items around server receive-loop-to-queue wiring, switcher placeholder display, and later real capture/encode.

---

## 2026-04-24
### Type
- Codex

### Work
- Added server-side runtime wiring from accepted `VideoFrame` receive side effects into caller-owned video frame queue storage.
- Added an explicit queue runtime result for queued frames versus not-queued paths.
- Surfaced rejected / unauthenticated `VideoFrame` packets as not queued instead of letting them reach storage.
- Preserved queue storage policy behavior, including drop-oldest when a per-client queue is full.
- Kept H.264 decode, sync scheduling, switcher display, 4-view sync, and OBS out of scope.

### Changed Files
- `apps/server/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The queue runtime consumes `ServerDispatchRuntimeSideEffectApplyOutcome` rather than changing packet acceptance or registered handler boundaries.
- `ServerVideoFrameQueueState` remains caller-owned.
- `ServerVideoFrameQueueStorageBoundary` remains the only mutating storage boundary.
- Rejected / unauthenticated video packets are reported as skipped runtime results and do not enter the queue.

### Unresolved
- switcher single-view decode/display placeholder
- real screen capture / real H.264 encode
- optional video send CLI/config launcher
- 2-view / 4-view sync and OBS integration

### Next
- Add switcher-side single-view decode/display placeholder.
- Decide whether a video send CLI/config launcher is needed for manual PoC runs.
- Later replace the placeholder encoded payload source with a real capture/encode boundary.

### TODO Update
- Marked the server one-view receive / accept-drop / queue PoC line complete.
- Updated Current Focus to say receive-side runtime wiring now stores accepted `VideoFrame` side effects in caller-owned queues.
- Reordered Next Items around switcher display placeholder, real capture/encode, and optional video launcher work.

---

## 2026-04-24
### Type
- Codex

### Work
- Added the smallest switcher-side single-view placeholder path.
- Added read-only latest-frame selection from `ServerVideoFrameQueueState` for one `ClientId`.
- Added a selected encoded-frame handoff that preserves frame metadata, encoded payload length, and encoded payload bytes.
- Added an explicit placeholder display handoff with decode status `DeferredPlaceholder`.
- Kept real H.264 decode, real window rendering, sync scheduling, full switcher UI, 4-view sync, and OBS out of scope.

### Changed Files
- `apps/switcher/Cargo.toml`
- `apps/switcher/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The switcher placeholder path reads the server queue state without mutating it.
- Decode remains explicit placeholder behavior, not hidden fake decode.
- The display handoff carries encoded frame data and metadata only; it does not represent decoded pixels.
- The switcher crate depends on existing server queue types for this PoC slice rather than moving queue types to a shared crate in this task.

### Unresolved
- video send CLI/config launcher decision
- real screen capture / real H.264 encode
- real H.264 decode
- real switcher window rendering
- sync scheduling, 4-view sync, and OBS integration

### Next
- Decide whether a video send CLI/config launcher is needed for manual one-client PoC runs.
- Add real capture / H.264 encode boundary later.
- Add real H.264 decode and switcher window rendering boundaries separately.

### TODO Update
- Marked switcher placeholder decode/display handoff complete.
- Updated the one-view PoC line to say switcher can select latest queued frame and create a placeholder display handoff.
- Reordered Next Items around optional video launcher, real capture/encode, and real decode/window rendering.

---

## 2026-04-24
### Type
- Codex

### Work
- Added a one-shot client CLI/config launcher for sending one placeholder `VideoFrame`.
- Reused the existing client PoC TOML fields for client id, run id, protocol version, and server destination.
- Constructed the frame through the existing metadata construction boundary.
- Constructed the placeholder encoded H.264 payload through the existing explicit placeholder payload source boundary.
- Sent the frame through the existing `ClientVideoFrameEncodeSendBoundary`.
- Added a compact stdout summary for manual verification.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The CLI path is `--placeholder-video-frame-poc-once [config-path]`.
- The launcher sends exactly one datagram and does not authenticate, retry, receive responses, capture the screen, or run a real encoder.
- Placeholder payload remains explicit and uses a small fixed H.264-shaped byte sequence for manual PoC traffic.
- Frame dimensions default to 1280x720 at 30 fps for this placeholder launcher.

### Unresolved
- real screen capture / real H.264 encode
- real H.264 decode
- real switcher window rendering
- sync scheduling, 4-view sync, and OBS integration

### Next
- Add real capture / H.264 encode boundary later.
- Add real H.264 decode and switcher window rendering boundaries separately.
- Document or script a manual server / client / switcher placeholder PoC path.

### TODO Update
- Marked placeholder `VideoFrame` one-shot CLI/config launcher complete.
- Updated Current Focus with the new client launcher flag.
- Reordered Next Items around real capture/encode, real decode/window rendering, and manual PoC path documentation.

---

## 2026-04-24
### Type
- Codex

### Work
- Audited the current manual one-client placeholder `VideoFrame` PoC path across client, server, and switcher.
- Documented that the implemented slices exist separately: client placeholder frame send, server accepted-frame queue storage, and switcher latest-frame placeholder handoff.
- Confirmed that the current client `--placeholder-video-frame-poc-once` command sends a `VideoFrame` only and does not authenticate first.
- Confirmed that running a separate auth command first does not satisfy server `VideoFrame` acceptance, because the existing commands own separate UDP sockets/source ports.
- Added a manual verification note with the exact current limitation and the smallest missing wiring for full manual end-to-end verification.
- Kept real capture, real H.264 encode/decode, switcher rendering, 4-view sync, and OBS out of scope.

### Changed Files
- `docs/operations/manual-placeholder-video-poc.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Did not weaken or bypass the server authenticated packet acceptance rule for manual convenience.
- Did not hide authentication inside the existing placeholder video CLI.
- Treated the next full manual verification step as explicit same-socket auth-then-video client wiring plus a queue-owning server launcher.
- Kept switcher placeholder selection as a library boundary until there is a shared runtime state handoff or a dedicated helper.

### Unresolved
- same-socket client auth-then-placeholder-`VideoFrame` one-shot launcher
- queue-owning server auth-then-video manual launcher with queued/rejected stdout
- switcher helper or runtime bridge for selecting from a server-owned queue after a manual receive
- real capture / real H.264 encode
- real H.264 decode / switcher window rendering
- sync scheduling, 4-view sync, and OBS integration

### Next
- Add a same-socket client auth-then-placeholder-video one-shot launcher.
- Add a server auth-then-video queue launcher that owns registry and queue state for the manual PoC.
- Add optional switcher placeholder selection helper after server queue state can be surfaced.

### TODO Update
- Updated Current Focus with the documented manual placeholder PoC status and limitation.
- Replaced the generic manual path item with the concrete missing client and server launcher steps.

---

## 2026-04-24
### Type
- Codex

### Work
- Added the smallest client-side same-socket auth-then-placeholder-video launcher.
- Added `--auth-placeholder-video-frame-poc-once [config-path]`.
- The launcher binds one UDP socket, sends `AuthRequest`, receives `AuthResponse`, requires `accepted=true`, then sends one explicit placeholder `VideoFrame` from the same socket/source.
- Reused the existing auth config loading and placeholder `VideoFrame` metadata/payload/send boundaries.
- Added focused client tests for config wiring, accepted auth sending video from the same source, and rejected auth stopping before video send.
- Kept server authentication unchanged and did not implement real capture, real H.264 encode/decode, switcher rendering, or OBS.

### Changed Files
- `apps/client/src/lib.rs`
- `apps/client/src/main.rs`
- `docs/operations/manual-placeholder-video-poc.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The existing `--placeholder-video-frame-poc-once` remains as a low-level video-only send test.
- Same-source auth is explicit in the new launcher and stdout summary instead of being hidden in the video-only path.
- Rejected auth returns before frame construction/send, preserving server packet acceptance assumptions.
- Placeholder payload remains explicit and uses the existing placeholder payload boundary.

### Unresolved
- queue-owning server auth-then-video manual launcher with queued/rejected stdout
- switcher helper or runtime bridge for selecting from a server-owned queue after a manual receive
- real capture / real H.264 encode
- real H.264 decode / switcher window rendering
- sync scheduling, 4-view sync, and OBS integration

### Next
- Add a server auth-then-video queue launcher that owns registry and queue state for the manual PoC.
- Add optional switcher placeholder selection helper after server queue state can be surfaced.

### TODO Update
- Marked the same-socket auth-then-placeholder-video client launcher complete.
- Updated Current Focus to say only the queue-owning server manual launcher blocks a full CLI-driven manual E2E path.
- Reordered Next Items around the server manual queue launcher, optional switcher helper, and later real capture/decode work.

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest server-side queue-owning auth-then-video manual launcher.
- Added `--receive-auth-video-queue-once [config-path]`.
- The launcher receives one `AuthRequest`, sends `AuthResponse` through the existing auth response PoC path, keeps the authenticated sender registry alive when auth is accepted, receives the next packet through the existing controller receive/send runtime and packet acceptance gate, then stores only an accepted `VideoFrame` side effect into caller-owned `ServerVideoFrameQueueState`.
- Added stdout summary fields for auth accepted/rejected, video received/not received, queued/not queued, queue length, drop-oldest, and registered client count.
- Added focused server tests for accepted auth then video queueing, rejected auth keeping later video out of the queue, and unexpected second packet staying not queued.
- Kept server authentication, packet acceptance gate, placeholder payload behavior, queue caller ownership, and H.264 decode/display/OBS separation unchanged.

### Changed Files
- `apps/server/src/lib.rs`
- `apps/server/src/main.rs`
- `docs/operations/manual-placeholder-video-poc.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Auth response send remains owned by the existing auth response PoC step so rejected auth can still receive an explicit `AuthResponse`.
- The server does not receive a second packet after rejected auth in the new CLI launcher; rejected-auth follow-up video behavior remains covered by the receive/gate test path and stays not queued.
- Queue insertion is performed only from `ServerVideoFrameQueueRuntimeBoundary::store_from_receive_side_effect`.
- Queue capacity uses the existing default `ServerVideoFrameQueuePolicy`; full queues surface the existing drop-oldest storage result.
- Receive timeout behavior was not added because the existing launcher/runtime patterns use blocking `UdpSocket` receives unless the caller configures a socket timeout.

### Unresolved
- switcher CLI or shared runtime bridge that can select from the server-owned queue after a manual receive
- real capture / real H.264 encode
- real H.264 decode / switcher window rendering
- sync scheduling, 4-view sync, and OBS integration

### Next
- Add an optional switcher placeholder selection helper or runtime bridge.
- Add real capture / H.264 encode boundary later.
- Add real H.264 decode and switcher window rendering boundaries separately.

### TODO Update
- Updated Current Focus with the completed server queue-owning auth-then-video launcher.
- Updated Next Items so the switcher placeholder helper is next, followed by real capture/encode and real decode/rendering.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-server receive_auth_video_queue_once`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest switcher-side manual placeholder verification helper over caller-owned `ServerVideoFrameQueueState`.
- Added `SwitcherPlaceholderManualVerificationBoundary` and summary/result types that compose the existing latest-frame selection and decode-deferred placeholder display handoff boundaries.
- Added switcher fixture CLI paths: `--placeholder-fixture-once [client-id]` and `--placeholder-empty-once [client-id]`.
- The helper reports selected client id, frame id, payload length, decode-deferred placeholder status, and no-frame state.
- Added focused switcher tests for latest selection through the helper, empty queue, metadata/payload length preservation, decode-deferred status, and read-only queue behavior.
- Documented that this verifies queue-to-switcher placeholder handoff only and does not share the server manual launcher's in-memory queue across processes.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/operations/manual-placeholder-video-poc.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Kept the helper in-process and caller-owned because no cross-process queue bridge exists yet.
- Used a fixture CLI for manual switcher verification instead of pretending the switcher can read a running server process's queue.
- Kept decode status explicit as `DeferredPlaceholder`.
- Did not implement H.264 decode, rendering, OBS integration, or 4-view sync.

### Unresolved
- explicit server-to-switcher runtime bridge if live cross-process queue consumption becomes necessary
- real capture / real H.264 encode
- real H.264 decode / switcher window rendering
- sync scheduling, 4-view sync, and OBS integration

### Next
- Decide whether a real cross-process queue bridge is needed for the next manual workflow.
- Add real capture / H.264 encode boundary later.
- Add real H.264 decode and switcher window rendering boundaries separately.

### TODO Update
- Updated Current Focus with the completed switcher manual placeholder helper and fixture CLI.
- Updated Next Items to replace the helper task with an explicit bridge decision, followed by real capture/encode and real decode/rendering.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher`
- `cargo check --workspace`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the first client-side boundary for moving from placeholder `VideoFrame`
  payloads toward real capture and real H.264 encode.
- Added explicit capture source result types; current capture returns
  `RealCaptureDeferred` and does not call OS/window/game capture APIs.
- Added explicit H.264 encoder result types; current encode returns
  `RealH264EncodeDeferred` for the supported raw handoff or
  `UnsupportedCaptureFormat` for unsupported raw formats, and does not produce
  fake real H.264 bytes.
- Added `ClientEncodedVideoFrameSource` so future real encoded frames can feed
  existing `VideoFrame` metadata construction and UDP send wiring without
  rewriting the send boundary.
- Kept the placeholder path available and explicitly marked placeholder bytes
  with `PlaceholderH264`.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Implemented capture and encode boundaries first, not real capture/encode
  backends.
- Preserved the existing placeholder PoC path and UDP send boundary.
- Kept capture, encode, metadata construction, and send responsibilities
  separate.
- Did not label placeholder payload bytes as real capture output.

### Unresolved
- actual capture backend
- actual H.264 encoder implementation and configuration
- real H.264 decode
- switcher window rendering
- targetTime / jitter-buffer selection, 4-view sync, and OBS integration

### Next
- Add an actual capture backend behind `ClientCaptureSourceBoundary`.
- Add an actual H.264 encoder behind `ClientH264EncoderBoundary`.
- Then connect real encoded output to the existing auth/video send PoC path.

### TODO Update
- Marked the client capture/encode boundary as complete.
- Replaced the next capture/encode task with actual capture backend and actual
  H.264 encoder implementation behind the new boundaries.
- Kept real decode/rendering, targetTime/jitter, and later cross-process bridge
  work as separate future items.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`





---

## 2026-04-25
### Type
- Codex

### Work
- Audited the client dependencies and confirmed the client crate still has no
  Windows API binding dependency for safe real enumeration.
- Kept default Windows Graphics Capture discovery explicit as runtime
  unavailable on Windows and backend unsupported on non-Windows.
- Added `ClientCaptureTargetDiscoveryRuntimeHook` so a future Windows
  implementation can provide real display/window descriptors behind the
  existing discovery boundary.
- Added `ClientUnavailableCaptureTargetDiscoveryRuntimeHook` as the default
  runtime hook.
- Added `discover_targets_with_runtime` for hook-backed discovery without
  changing existing `discover_targets` behavior.
- Added tests for hook-provided descriptors, no-targets results, descriptor to
  config conversion, unsupported/unavailable paths, and placeholder independence.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Real Windows enumeration is still deferred because adding Windows API binding
  and runtime permission/session behavior is broader than this slice.
- The hook boundary is the smallest safe step: real descriptors can only appear
  from a runtime hook that actually enumerates them later.
- Default discovery does not fake display/window targets.

### Unresolved
- actual Windows Graphics Capture display/window enumeration
- capture permission/session/runtime wiring
- real frame acquisition
- real H.264 encode/decode
- switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS

### Next
- Add a Windows API-backed implementation of
  `ClientCaptureTargetDiscoveryRuntimeHook`.
- Keep capture session creation and frame acquisition as separate follow-up
  slices.

### TODO Update
- Marked the capture target discovery runtime hook boundary as complete.
- Kept actual Windows display/window enumeration and frame acquisition as future
  work.
- Kept encode/decode, rendering, sync, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest client Windows capture target discovery boundary.
- Added metadata-only target descriptors for display and window targets.
- Added conversion from discovered target descriptors to
  `ClientCaptureTargetConfig`.
- Added discovery input and result types for targets found, no targets found,
  backend unsupported, discovery unavailable, permission unavailable, runtime
  unavailable, and backend not configured states.
- Kept Windows Graphics Capture target enumeration deferred; discovery currently
  reports runtime unavailable on Windows and backend unsupported on non-Windows.
- Kept discovery separate from capture session creation, frame acquisition,
  H.264 encode/decode, UDP send, switcher rendering, and OBS.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Target discovery is a pre-session boundary.
- Real Windows display/window enumeration is deferred until the Windows runtime
  dependency/API wiring is added.
- Target descriptors are metadata-only and cannot produce capture frames.
- Placeholder payload behavior remains independent from discovery.

### Unresolved
- actual Windows Graphics Capture display/window enumeration
- capture permission/session/runtime wiring
- real frame acquisition
- real H.264 encode/decode
- switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS

### Next
- Add real Windows display/window enumeration behind
  `ClientCaptureTargetDiscoveryBoundary`.
- Use selected descriptors to build `ClientCaptureTargetConfig` for future
  capture session creation.
- Keep frame acquisition and H.264 encode as separate follow-up slices.

### TODO Update
- Marked Windows capture target discovery boundary as complete.
- Updated Next Items to start with real Windows display/window enumeration,
  then frame acquisition.
- Kept actual capture frames, encode/decode, rendering, sync, and OBS deferred.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`
- `cargo run -p stream-sync-switcher -- --placeholder-empty-once client-1`
- `cargo run -p stream-sync-switcher -- --placeholder-fixture-once client-1`

---

## 2026-04-25
### Type
- Codex

### Work
- Audited the current server queue and switcher placeholder helper boundaries for the next one-client placeholder PoC bridge.
- Decided that the next bridge should be a switcher-owned in-process integration launcher.
- Documented that the bridge should call the existing server auth-then-video queue launcher/boundary in-process, then pass the returned caller-owned `ServerVideoFrameQueueState` to `SwitcherPlaceholderManualVerificationBoundary`.
- Documented that file/socket/shared-memory queue sharing and server-side export endpoints are deferred for this PoC step.
- Kept the decision scoped to bridge design; no real H.264 decode, rendering, OBS, or 4-view sync was added.

### Changed Files
- `docs/architecture/system-design.md`
- `docs/operations/manual-placeholder-video-poc.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Chosen bridge: in-process integration helper owned by switcher.
- Rejected for this step: fake cross-process queue sharing, shared memory, file/socket queue bridge, and server-owned export/log fixture.
- Dependency direction should remain `switcher -> server`; `apps/server` should not depend on `apps/switcher`.
- The bridge should verify encoded queue-to-placeholder handoff only.

### Unresolved
- actual switcher-owned manual bridge launcher command
- real capture / real H.264 encode
- real H.264 decode / switcher window rendering
- sync scheduling, 4-view sync, and OBS integration

### Next
- Add a switcher-owned manual bridge launcher, shaped like `--receive-auth-video-placeholder-bridge-once [config-path] [client-id]`.
- Keep file/socket/shared-memory queue sharing deferred until a continuous runtime or real renderer needs it.

### TODO Update
- Replaced the bridge decision item with the chosen next implementation: a switcher-owned in-process manual bridge launcher.
- Preserved real capture/encode, real decode/rendering, and targetTime/jitter work as later items.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo check --workspace`

---

## 2026-04-25
### Type
- Codex

### Work
- Implemented the smallest switcher-owned in-process manual bridge launcher.
- Added `SwitcherAuthVideoPlaceholderBridgeBoundary` and summary/result types.
- Added `--receive-auth-video-placeholder-bridge-once [config-path] [client-id]` to the switcher CLI.
- The CLI runs `ServerReceiveAuthVideoQueueOnceLauncher` in-process, receives the caller-owned `ServerVideoFrameQueueState`, then passes it to the switcher placeholder bridge boundary.
- Added focused switcher tests for queued handoff composition, client-id selection, no-frame, rejected/not queued video, and read-only queue behavior.
- Kept cross-process queue sharing, H.264 decode, rendering, OBS, and 4-view sync out of scope.

### Changed Files
- `apps/switcher/src/lib.rs`
- `apps/switcher/src/main.rs`
- `docs/operations/manual-placeholder-video-poc.md`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The bridge remains switcher-owned and in-process.
- The bridge reuses the server queue launcher outcome instead of duplicating packet acceptance or queue storage logic.
- The switcher placeholder helper remains read-only and decode-deferred.
- File/socket/shared-memory queue sharing remains deferred.

### Unresolved
- real capture / real H.264 encode
- real H.264 decode / switcher window rendering
- sync scheduling, 4-view sync, and OBS integration
- future continuous-runtime server-to-switcher queue transport, if needed

### Next
- Move to real capture / H.264 encode boundary, or real decode/window rendering if display proof is the next priority.

### TODO Update
- Marked the in-process bridge launcher as complete in Current Focus.
- Removed the bridge launcher from Next Items.
- Kept real capture/encode, real decode/rendering, targetTime/jitter, and later cross-process bridge decision as remaining items.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-switcher`
- `cargo check --workspace`

---

## 2026-04-25
### Type
- Codex

### Work
- Audited the client crate dependencies and kept the first Windows capture slice
  dependency-free.
- Chose Windows Graphics Capture as the Windows MVP capture backend direction.
- Added client capture backend selection/config types for backend and target.
- Added a capture backend probe boundary that surfaces not configured,
  unsupported, unavailable, and future available states explicitly.
- Routed configured capture attempts through the backend probe while still
  refusing to produce fake raw pixels.
- Kept capture, encode, metadata, and UDP send boundaries separate.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Windows MVP capture direction: Windows Graphics Capture.
- No new Windows API or capture dependency was added in this slice.
- On Windows, a configured Windows Graphics Capture probe currently reports
  `BackendUnavailable` until runtime integration is wired.
- On non-Windows targets, the Windows backend reports `BackendUnsupported`.
- Missing backend or target configuration reports `BackendNotConfigured`.

### Unresolved
- actual Windows Graphics Capture frame acquisition
- capture permission/session/runtime wiring
- window/display enumeration
- real H.264 encoder implementation and configuration
- real decode, switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS

### Next
- Add the actual Windows Graphics Capture runtime behind the probe boundary.
- Add a target discovery/config path for display/window selection.
- Keep H.264 encode as a separate next boundary after real raw frame capture.

### TODO Update
- Marked the Windows capture backend selection/probe boundary as complete.
- Replaced generic actual capture work with actual Windows Graphics Capture
  frame acquisition behind the new boundary.
- Kept actual H.264 encode, decode/rendering, and sync work separate.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest client capture session configuration preparation boundary.
- Added metadata-only `ClientCaptureSessionConfig` and
  `ClientWindowsGraphicsCaptureSessionTargetConfig` for future
  WindowsGraphicsCapture session runtime wiring.
- Added conversion from selected `ClientCaptureTargetDescriptor` and
  `ClientCaptureTargetConfig` into prepared session config without opening a
  capture session.
- Added explicit not-prepared reasons for backend not configured and missing
  target details.
- Added focused `client_video_frame` tests for display/window descriptor
  conversion, target-config conversion, missing details, no-runtime conversion,
  and placeholder payload independence.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Session configuration is metadata-only and remains separate from target
  discovery, permission handling, session/runtime creation, frame acquisition,
  encoding, and UDP send.
- Empty display ids and window titles are treated as explicit missing target
  details instead of being accepted as usable runtime inputs.
- No Windows API dependency or fake capture output was added.

### Unresolved
- Windows API-backed display/window enumeration
- capture permission/session/runtime wiring
- capture session creation
- frame acquisition
- real H.264 encode/decode
- switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS

### Next
- Add actual Windows Graphics Capture display/window enumeration behind the
  discovery hook.
- Add the session runtime creation boundary that consumes
  `ClientCaptureSessionConfig`.
- Keep frame acquisition and H.264 encode as separate follow-up slices.

### TODO Update
- Marked capture session config preparation as complete in Phase 3.
- Added the session config boundary to Current Focus.
- Updated Next Items to put session runtime creation before frame acquisition.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`

---

## 2026-04-25
### Type
- Codex

### Work
- Added the smallest client capture session runtime creation boundary.
- Added `ClientCaptureSessionRuntimeInput` derived only from
  `ClientCaptureSessionConfig`.
- Added `ClientCaptureSessionRuntimeBoundary` and caller-owned
  `ClientCaptureSessionRuntimeHook`.
- Added a default unavailable runtime hook that keeps real Windows API session
  creation deferred: runtime-unavailable on Windows and backend-unsupported on
  non-Windows.
- Added explicit runtime creation results for created, creation deferred,
  permission unavailable, runtime unavailable, backend unsupported,
  unsupported target, and creation failed.
- Added focused `client_video_frame` tests for runtime input construction,
  default unavailable behavior, hook-created runtime handoff, deferred /
  permission-unavailable / failed hook results, unsupported target rejection,
  and placeholder payload independence.

### Changed Files
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- Runtime creation consumes only prepared `ClientCaptureSessionConfig`.
- The boundary delegates OS-specific creation to a caller-owned hook so real
  Windows API wiring can be added later without changing discovery, session
  config, frame acquisition, encode, or UDP send boundaries.
- The default hook does not create a real session or fake one.
- Frame acquisition remains a later boundary that will consume a created
  runtime handoff.

### Unresolved
- Windows API-backed display/window enumeration
- real Windows Graphics Capture session creation inside the runtime hook
- capture permission/runtime wiring details
- frame acquisition
- real H.264 encode/decode
- switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS

### Next
- Add actual Windows Graphics Capture display/window enumeration behind the
  discovery hook, or wire real session creation into the runtime hook if a
  selected target is already available.
- Keep frame acquisition separate from session creation.
- Keep H.264 encode as a separate follow-up after raw frame acquisition.

### TODO Update
- Marked capture session runtime creation boundary as complete in Phase 3.
- Added runtime creation to Current Focus.
- Updated Next Items to distinguish real session creation inside the hook from
  frame acquisition.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`
- `git diff --check`
---

## 2026-04-25
### Type
- Codex

### Work
- Added the first Windows-only real `ClientCaptureSessionRuntimeHook` for Windows Graphics Capture session creation.
- Added `ClientWindowsGraphicsCaptureSessionRuntimeHook`, which creates a `GraphicsCaptureItem`, `Direct3D11CaptureFramePool`, and `GraphicsCaptureSession` for a ready session runtime without starting capture or acquiring frames.
- Kept `ClientUnavailableCaptureSessionRuntimeHook` as the default placeholder-safe path, so existing callers still get explicit runtime-unavailable on Windows or backend-unsupported on non-Windows unless they inject the real hook.
- Added Windows-only runtime storage on `ClientCaptureSessionRuntime` so the created item/frame-pool/session/device stay alive while the runtime handoff exists.
- Mapped Windows setup errors into explicit session creation reasons, including permission-unavailable, runtime-unavailable, invalid-target, creation-deferred, and creation-failed.
- Kept frame acquisition, H.264 encode, UDP send changes, and placeholder path changes out of scope.

### Changed Files
- `apps/client/Cargo.toml`
- `apps/client/src/lib.rs`
- `docs/architecture/system-design.md`
- `docs/operations/todo.md`
- `docs/operations/session-log.md`

### Decisions
- The real Windows hook is caller-owned and injected explicitly; the default boundary path remains unavailable/unsupported for compatibility.
- Primary display can create a monitor item now. Window title targets resolve through HWND lookup. Non-primary display stable ids remain creation-deferred until real Windows display enumeration provides a handle-backed descriptor path.
- Session creation owns only readiness objects. It does not call `StartCapture`, read frames, encode, or send.
- The client crate now allows unsafe code locally because Windows Graphics Capture desktop interop and D3D device creation require unsafe Windows FFI calls.

### Unresolved
- actual Windows Graphics Capture frame acquisition from a ready runtime
- Windows API-backed target enumeration for display/window descriptors beyond the current metadata placeholders
- real H.264 encoder implementation and configuration
- real H.264 decode, switcher rendering, targetTime / jitter-buffer, 4-view sync, and OBS integration

### Next
- Add a frame acquisition boundary that consumes `ClientCaptureSessionRuntime` and returns raw BGRA frames without touching encode/send.
- Add Windows target enumeration for display/window handles so non-primary display ids are not deferred.
- Add the H.264 encoder behind the existing encoder boundary after raw frame acquisition exists.

### TODO Update
- Updated Current Focus to record that the Windows-only real session hook can create a ready session runtime while the default placeholder path remains unchanged.
- Added a completed Phase 3 item for first minimal Windows Graphics Capture session creation.
- Updated Next Items so frame acquisition from a ready runtime is the next capture task.

### Validation
- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p stream-sync-client client_video_frame`
- `cargo check --workspace`
