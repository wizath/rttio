# rttio Application Review

Scope: full Rust codebase (`src/`, `src/control/`, `src/bin/`, `build.rs`, `Cargo.toml`), reviewed against rust-best-practices (API guidelines, async/Tokio rules, performance, project structure). `venv/` excluded (Python tooling, not part of the crate).

Overall: the codebase is in good shape for a 1.0 terminal tool. Shutdown is handled carefully (panic hook, socket guard, task join-or-abort), the control socket is properly hardened (0600 perms, parent-dir writability check, stale-socket detection), atomic config writes with backup-on-corruption, and there is a large protocol test suite (~3900 lines). The findings below are mostly structural debt and a handful of real concurrency/latency issues.

---

## 1. Architecture

### 1.1 `main.rs` is a 1100-line shared header, not an entry point

`src/main.rs` contains every shared type, enum, constant, and the entire clap CLI definition. Every module then does `use crate::*;` (`app.rs:1`, `runtime.rs:1`, `transports.rs:1`, etc.), and `main.rs:1094-1101` glob-imports every module back. The result is one flat namespace across ~8500 lines:

- No module owns its types. `ControlHistory` lives in `main.rs:438` but is used only by control code; `InterfaceCommand`/`InterfaceEvent` (`main.rs:323-366`) belong to the transport layer; `MenuCommand` belongs to input.
- Any symbol rename touches everything; rustc cannot help you find the real dependency graph.
- Guideline: keep `main.rs` minimal (args + wiring), organize modules by feature, use explicit imports or a `prelude` module.

Recommendation: move types next to their owners (`ControlHistory` -> `control/`, `InterfaceEvent`/`InterfaceCommand` -> `transports.rs`, CLI structs -> a `cli.rs`), replace `use crate::*` with explicit imports. Consider a `lib.rs` + thin `main.rs` so integration tests in `tests/` become possible.

### 1.2 `control_json.rs` mounted via `#[path]` with trailing imports

`control.rs:5-6` mounts a sibling file with `#[path = "control_json.rs"]`, and that file has its `use super::*; use serde::Serialize;` at the **bottom** (`control_json.rs:440-441`). It compiles, but it is a trap for readers and tooling. Move the file into `src/control/json.rs` and put imports at the top.

### 1.3 `run_app` is a god function

`app.rs:18-750` (~730 lines) does CLI/config/env target resolution, config persistence, task spawning, the entire main event loop, and shutdown. The target-resolution section (`app.rs:60-196`) is a wall of `#[cfg(feature)]`-guarded `let` bindings with subtle precedence rules (argv > picker > env > config), already proven fragile enough that `select_jlink_sn` had to be extracted for testing.

Recommendation: extract a pure `ResolvedTarget::from(opts, config, env) -> Result<ResolvedTarget>` so the precedence rules are testable in one place, and a `Channels`/`AppState` struct for the loop state (`output_mode`, `timestamp`, `local_echo`, `output_paused`, `serial_running`, `rtt_running` are mutated across 300 lines).

### 1.4 Stringly-typed internal protocol

Transport tasks reply to control requests with formatted strings (`"OK rtt write 5 bytes\n"`, `"ERR ...\n"`), and the rest of the system parses them back:

- `app.rs:506` — `response.trim_start().starts_with("OK")` decides echo behavior.
- `actions.rs:249-274` — `control_write_ack_targets` re-parses the target list out of the human-readable ack.
- `actions.rs:125-136` — `parse_jlink_reported_bytes`/`parse_jlink_reported_result` scrape `"J-Link reported "` out of a status message.
- `actions.rs:156-189` — `control_error_code` classifies errors by substring matching on English text (`lower.contains("timed out")`...). Any wording change silently changes the JSON `code` field clients depend on.
- `config.rs:198-208` — `is_serial_connected_status(text == "connected")`, `is_rtt_connected_status(starts_with("connected up="))`: the connection state machine in `app.rs:586-668` is driven by display strings. You already needed a regression test (`rtt_connected_status_matches_only_success_lines`) to protect a UI string.

Recommendation (highest-leverage refactor in the codebase): make the internal reply a type, render strings only at the socket boundary.

```rust
enum TransportReply {
    WriteOk { target: Source, bytes: usize },
    ActionOk { kind: ActionKind, reported: Option<String> },
    Err { code: ControlErrorCode, message: String },
}
```

Same for `InterfaceEvent::Status` — add a `Connected { .. }` / `Disconnected` variant instead of inferring from text.

### 1.5 Hand-rolled option parser, four times

`parse.rs` implements the same `strip_prefix("--flag ")` loop four times (`parse_control_read_args`, `parse_control_write_args`, `parse_control_request_args`, plus `actions.rs:parse_control_action_options`) — ~450 lines of duplicated control flow with per-copy drift risk (e.g. `--fail-on-timeout` as last token is accepted in read but an error in request, `parse.rs:86-88` vs `parse.rs:388-390`). A single tokenizer (`fn next_flag(rest) -> Option<(Flag, &str)>`) driving a per-command spec table would collapse this and guarantee consistent error messages. clap is already a dependency; `Command::try_get_matches_from` over `shlex`-split words is another option.

### 1.6 Triplicated target/route enums and string maps

`Route` (`main.rs:143`), `ControlTarget` (`main.rs:243`), `CtlTargetArg` (`main.rs:1067`), `ControlSource` (`main.rs:262`) all model "serial / rtt / both-ish" with near-identical `as_ctl_str` methods, and `Route -> &'static str` is re-implemented in three places (`app.rs:779-783`, `server.rs:723-727`, `Route::as_ctl_str`). `TerminalStatusBar.target: &'static str` (`main.rs:398`) should just hold `Route`.

### 1.7 Two sources of truth for the protocol surface

`CONTROL_COMMANDS_HELP` (`main.rs:121`) is a hand-maintained one-line duplicate of `CONTROL_COMMAND_SPECS` (`control_json.rs:284-439`), and `collect_control_status_text` (`status.rs:77-107`) hand-formats 22 of the JSON struct's fields. Generate the text forms from the specs/struct so they cannot drift.

### 1.8 Duplicate Write handling

`InputEvent::Control(ControlRequest::Write { .. })` is handled inline in the main loop (`app.rs:497-526`, with echo/history side effects) while `runtime.rs:11-21` contains a second `Write` arm in `handle_control_request` that the main loop never reaches (it destructures Write first). The dead arm will silently diverge. Delete it or route both through one function.

---

## 2. Performance

### 2.1 RTT task drains one command per poll cycle — real throughput cap

`transports.rs:439-654`: the RTT loop calls `rx.try_recv()` **once**, then does an `rtt_read` and, if no data, sleeps `poll_ms` (`transports.rs:665`). Consequences:

- Control write throughput is capped at ~1 write per `poll_ms` (default 10 ms => ~100 writes/s) when the channel backs up; `rttio ctl request` latency includes up to one full poll interval.
- `Stop` latency is also up to read + poll interval.

Fix: drain the channel (`while let Ok(cmd) = rx.try_recv()`) before each read, or restructure with `tokio::select!` over `rx.recv()` and a read/poll future like the serial/TCP tasks do.

### 2.2 Status bar redraw and history lock per data chunk

Every `InterfaceEvent::Data` triggers `send_status_bar` (`app.rs:571-584`) — a full `TerminalStatusBar` clone through the channel and a repaint with `SavePosition`/scroll-region escapes (`terminal.rs:353-363`) per 1024-byte chunk. At high baud rates that is hundreds of repaints per second competing with actual output writes. The terminal task already has a 100 ms `activity_tick`; let the tick own status-bar refreshes and only push state changes (history bytes can ride on the tick by sharing an `AtomicUsize`). That also removes the per-chunk `control_history.lock().await` for `history.bytes()` in the MenuCommand/Status paths (`app.rs:490`, `app.rs:666`).

### 2.3 Per-chunk allocations

`app.rs:542-570`: each data chunk clones `data` twice (history push + `raw_tx` broadcast) and `rendered` twice (terminal + `control_output_tx`). Wrapping payloads in `bytes::Bytes`/`Arc<[u8]>` and `Arc<str>` makes the broadcast fan-out and history push reference-counted instead of copied. Not urgent at serial speeds; worth it if RTT throughput matters (RTT can do MB/s).

### 2.4 `ControlHistory::snapshot` copies the matching window

`main.rs:519-548` linearly scans all entries and `extend_from_slice`s every matching byte. `collect_control_status` (`status.rs:113-120`) calls it **three times** (Any/Serial/Rtt) just to read `next_seq`/`dropped_before` — copying up to 3 MiB per `status` command. Add a cheap `cursor(source) -> (next_seq, dropped_before)` that does not copy data.

### 2.5 Channel sizing / config

- `tokio = { features = ["full"] }` (`Cargo.toml:21`) pulls the entire runtime surface (process, signal, fs, parking_lot...). You need roughly `rt-multi-thread, macros, io-util, net, sync, time`. Smaller builds, fewer cargo-audit surfaces.
- `[profile.release] lto = "thin"` — `lto = "fat"` typically wins a few percent for a binary this size; you already pay `codegen-units = 1`.
- `--chunk` is documented as "Transport read chunk size" but only `rtt_task` honors it; serial (`transports.rs:58`) and RTT-TCP (`transports.rs:221`) hardcode `vec![0u8; 1024]`. Also `chunk`/`poll_ms` accept 0 with no validation (`chunk=0` => zero-length reads spinning at poll rate).

---

## 3. Antipatterns

| Location | Issue |
|---|---|
| `main.rs:610-624` | `OutputLineState` derives `Default` (all `false`) **and** has `new()` returning all `true`. The derive is a loaded gun: anyone writing `OutputLineState::default()` gets "mid-line" state and loses the first prefix. Only `new()` is currently used — remove the derive or make `Default` delegate to `new()`. |
| `actions.rs:24-35` | `parse_control_action_args` maps to `if rest.is_empty() { args } else { ControlActionArgs { json: args.json, timeout: args.timeout } }` — both branches are identical; dead leftover logic. |
| `app.rs:311` | `let prefix = false;` hardcoded forever; `render_line_prefix`'s `prefix` parameter (`runtime.rs:325`) is a dead feature. Either expose `--prefix` or delete the plumbing. |
| `server.rs:145`, `server.rs:186`, `server.rs:206`, `cli.rs:21` | Functions take `&PathBuf` instead of `&Path` (clippy `ptr_arg`). Same for `scan_flash_candidates(dir: &PathBuf)` (`input_menu.rs:882`) and `remember_flash_file(path: &PathBuf)` (`config.rs:190`). |
| `input_menu.rs:660-662` | `truncate_to_width` counts `chars()`, and `footer_line_with_version` (`input_menu.rs:1162-1177`) mixes byte `len()` with char truncation. Wide glyphs (CJK, emoji in device output paths) misalign the status bar and footer. Use `unicode-width` if non-ASCII matters, or document the ASCII assumption. |
| `Cargo.toml` | No `[lints]` table. Given the codebase's discipline elsewhere, `clippy::correctness = "deny"` + `suspicious/perf/style = "warn"` would be free wins; `ptr_arg` and the `Default` issue above are caught by default clippy. |
| repo root | `venv/` (Python site-packages), `dist/`, `test.txt` sit untracked next to the crate. Add to `.gitignore` so tooling (and this review's structure scan) stops crawling 3 MB of Python. |
| `main.rs:92-119` | Test-variant constants (`CONTROL_MAX_CLIENTS = 1` under `cfg(test)`, six timeout pairs) interleaved with production constants. Works, but a `mod tuning { #[cfg(test)] ... }` block would halve the noise. |

---

## 4. Issues & Hidden Bugs

### 4.1 Two threads write to stdout, synchronized only by convention

The terminal task owns stdout (`terminal.rs:39`), but the input thread **also** draws directly: `draw_flash_address_prompt` (`input_menu.rs:741-770`), `prompt_flash_path`/`draw_flash_picker` (`input_menu.rs:772-853`, `1069-1141`), including its own `EnterAlternateScreen`. Mutual exclusion relies on `command_view_visible` deferring output in the terminal task — but:

- `app.rs` keeps sending `SetStatusBar`/`Activity` events, and the terminal task's `activity_tick` redraw is gated only on `command_view_visible`, which is **its own** flag set by `ShowMenu`. The `f`-key path (`input_menu.rs:199-209`) clears `command_view_active` *before* opening the picker, and the picker never sends `ShowMenu` — so if the menu was closed by the time the picker draws (Enter-path vs key-path differ here), tick redraws interleave with picker draws.
- Even when gating holds, this is a data race on terminal state (cursor position, alternate screen) by design.

Fix: make the input thread send `TerminalEvent::DrawFlashPicker(state)` etc. so stdout has exactly one writer, or hold a shared `Mutex<Stdout>`.

### 4.2 Flash prompts block shutdown forever

`prompt_flash_address` and `prompt_flash_path` loop on bare `event::read()` (`input_menu.rs:703`, `784`) with no check of the `running` flag and no `event::poll` timeout. If `rttio ctl quit` (or transport EOF) stops the app while the picker is open, the input thread blocks in `event::read()` indefinitely; `run_app` proceeds after its 150 ms grace (`app.rs:740-743`), drops `TerminalGuard`, and exits with a detached thread mid-read — terminal state restore races with the final read. Use the same `poll(100ms)` + `running` pattern as the main input loop.

### 4.3 Blocking filesystem I/O (with fsync) on the async runtime

- `save_config` does `write_all` + `sync_all` + `rename` (`config.rs:96-131`) and is called from the **main select loop** on first connect (`app.rs:597`, `628`) — an fsync stall freezes input handling, rendering, and control responses.
- `remember_flash_file` (load + save + fsync) runs inside `rtt_task` right after flashing (`transports.rs:510`).
- `validate_flash_file` (3 stats) runs in async contexts (`server.rs:659`, `runtime.rs:45`).

Wrap config persistence in `tokio::task::spawn_blocking`. Per the async guidelines: never run blocking syscalls (especially fsync) on runtime workers.

### 4.4 `follow` clients permanently consume the client limit

`handle_control_client`'s follow branch (`server.rs:250-265`) loops forever with no idle timeout and no write deadline; each holds a `Semaphore` permit. 32 abandoned-but-open `follow` connections (`CONTROL_MAX_CLIENTS`, `main.rs:93`) permanently lock out all control clients — including `rttio ctl quit`. The socket is 0600 so this is self-DoS, not an attack, but stuck tooling (a wedged CI runner holding sockets) hits it. Consider a write timeout or a separate, smaller follow budget.

### 4.5 `quit` ack race

`server.rs:497-511` sends `InputEvent::Quit`, then returns `"OK quit\n"` to be written by the client task. The main loop breaks on Quit and `main` returns shortly after; the runtime drops and kills the spawned client task, so the ack write races process exit. `rttio ctl quit` will intermittently report `control socket closed without response` (`cli.rs:68`) despite succeeding. Either flush the ack before forwarding Quit, or have the ctl client treat EOF-after-quit as success.

### 4.6 Flash picker cannot accept a typed path that matches a candidate substring

`input_menu.rs:809-819`: on Enter with non-empty input, if the filtered list is non-empty the **selected candidate** always wins; the typed path is only used when the filter matches nothing. Typing `build/app2.bin` while `build/app.bin` is listed (substring match keeps the list non-empty) returns `build/app.bin`. Needs an explicit "use typed path" affordance or exact-match preference.

### 4.7 Erase forgets `FlashProgress` cleanup

The flash path clears the progress bar (`transports.rs:558-559` sends `FlashProgress(None)`), but the erase path (`transports.rs:588-643`) never does. Harmless today because erase never sends progress events, but if `erase_chip` ever reports progress (or a flash is interrupted into an erase), a stale `flash:[####------]` segment sticks in the status bar.

### 4.8 Minor

- `monotonic_timestamp` (`runtime.rs:336-340`) initializes its epoch on **first rendered line**, not app start; the first timestamp is always `000000.000` regardless of connect time. Initialize the `OnceLock` in `run_app`.
- `save_config_to_path` uses a fixed `.rttio.tmp` (`config.rs:103`); two rttio instances in the same directory saving concurrently can clobber each other's temp file mid-write (the rename itself stays atomic). Suffix with the PID.
- `read --since <seq>` with a sequence in the future of `next_seq` silently returns "everything new" semantics; an explicit error would catch cursor mix-ups between instances (seq counters reset on restart while clients may cache `next_seq`).
- `route_write` (`runtime.rs:219-265`) silently drops typed input when the routed transport isn't running — no status line, no error. With local echo off, keystrokes vanish without trace.

### 4.9 Security posture (brief)

Good: socket 0600 enforced and re-verified (`server.rs:186-204`), parent dir group/world-writable rejection (`server.rs:157-184`), client-side permission validation before connect, stale-socket removal refuses non-sockets, 1 MiB command cap, timeout caps (`CONTROL_MAX_TIMEOUT_MS`), payload CR/LF injection rejected client-side (`cli.rs:371-381`). No findings beyond the follow-permit exhaustion in 4.4. The upward socket discovery (`cli.rs:10-18`) could connect to an ancestor directory's instance unexpectedly, but the 0600 + ownership-implied check bounds the blast radius to the same user.

---

## 5. Recommendations

### Priority fixes (critical)

1. **Drain commands in `rtt_task`** (2.1) — direct functional impact on control write throughput and stop latency. Small, contained fix.
2. **Single stdout writer** (4.1) — move flash-prompt drawing into `terminal_task`; this is the only genuine data race in the program.
3. **Unblock shutdown from flash prompts** (4.2) — switch prompt loops to `poll(100ms)` + `running` check.
4. **`spawn_blocking` around config saves** (4.3) — one-line wrapper at two call sites; removes fsync stalls from the event loop.
5. **Remove the `OutputLineState` `Default` derive** (3, first row) — latent correctness trap.

### Suggested improvements (important)

6. **Typed transport replies** (1.4) — replaces `starts_with("OK")`, ack-target re-parsing, "J-Link reported" scraping, and substring error classification with one enum. Biggest robustness win per line changed; the JSON `code` taxonomy becomes compiler-enforced.
7. **Structured connection status** (1.4) — `InterfaceEvent::Connected/Disconnected` variants instead of string matching in `config.rs:198-208`.
8. **Break up `main.rs`** (1.1) — move types to owning modules, kill `use crate::*`. Mechanical but transformative for navigation.
9. **Unify the option parsers** (1.5) — one tokenizer + spec table.
10. **Throttle status-bar redraws to the activity tick** (2.2) and add a copy-free `ControlHistory::cursor` (2.4).
11. **Honor `--chunk` in serial/TCP transports or scope its docs to RTT; validate `chunk`/`poll_ms` nonzero** (2.5).
12. **Delete the dead `Write` arm in `handle_control_request`** (1.8) and the no-op branch in `parse_control_action_args`.
13. **Fix the `quit` ack race** (4.5) — flush before forwarding Quit.

### Nice-to-have

14. Trim tokio features; consider `lto = "fat"`; add a `[lints]` table (`clippy::correctness = deny`, `suspicious/perf = warn`).
15. `&Path` instead of `&PathBuf` parameters; `unicode-width` for `truncate_to_width` if non-ASCII output matters.
16. Generate `CONTROL_COMMANDS_HELP` and the status text format from `CONTROL_COMMAND_SPECS` (1.7).
17. `.gitignore` for `venv/`, `dist/`, `test.txt`; move `control_json.rs` into `src/control/json.rs` with imports at top.
18. Replace the four target-ish enums with two (`Route` + `ControlTarget`) and one `as_ctl_str` each (1.6).
19. Initialize the timestamp epoch at startup; PID-suffix the config temp file.
20. Split `tests.rs` (3890 lines) by area (`parse`, `socket`, `history`, `wire`) under `src/control/tests/`; note that all tests vanish when the `control` feature is off — the render/line-state helpers in `runtime.rs` deserve feature-independent coverage.
