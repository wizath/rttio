# Changelog

## 1.3.0 - 2026-07-05

### Added

- ESP serial flash progress reporting on the status bar.
- Remembered flash addresses for recent raw `.bin` files.
- TX/RX activity indicators in the status bar.

### Changed

- `ctl flash` and `ctl erase` for ESP serial targets now return only after the
  serial monitor has reopened, reset the target, and reported `connected`.
- Terminal rendering now preserves active ANSI SGR colors when the status bar is
  redrawn.

### Fixed

- Avoided stale status bars and cursor corruption during fast terminal resize.
- Buffered split ANSI escape sequences so target colors are not printed as
  partial text such as `0m`.
- Prevented resize from creating blank gaps in the terminal output stream.
- Reduced control/input backpressure paths that could delay menu and quit
  handling under heavy output.

## 1.2.0 - 2026-06-30

### Added

- Explicit `rtt` and `serial` transport commands.
- Async raw TCP serving with `--serve HOST:PORT` for serial and RTT sessions.
- Ser2net/raw TCP serial client support through `rttio serial tcp://HOST:PORT`.

### Changed

- Replaced the old top-level transport syntax with `rttio rtt ...` and
  `rttio serial ...`.
- Moved J-Link probe/device listing into `probes`, `devices`, and
  `pick-device` commands.

### Fixed

- Kept feature-gated builds working for serial-only, RTT-only, and
  control-enabled variants.

## 1.1.0 - 2026-06-15

### Added

- ESP serial flashing integration with reset, erase, and flash actions.
- Linux/macOS release build scripts.
- `--no-config` mode to ignore `.rttio` for a run and skip config updates.

### Fixed

- Kept the terminal cursor out of the status bar after resize.
- Cleared stale status bar lines left behind by tmux resize events.
- Kept startup transport errors visible after terminal cleanup.
- Logged when `.rttio` is loaded and which options come from config.

## 1.0.0 - 2026-06-04

Initial 1.0.0 release.

### Added

- Serial terminal mode with baud rate, line ending, local echo, timestamps,
  hex output, flow control, logging, and auto-reconnect.
- Direct SEGGER J-Link RTT terminal mode with target chip selection, probe
  serial selection, RTT channel selection, configurable speed, reset, erase,
  flash, and auto-reconnect.
- J-Link Remote Server/IP selection with `--jlink-ip HOST[:PORT]`.
- RTT TCP terminal mode for connecting to an existing RTT TCP server.
- Full-screen `Ctrl-T` command menu with help, clear screen, clear control
  buffer, echo/timestamp/output toggles, reset, reconnect, flash, and quit.
- Status bar showing selected target, transport state, output mode, toggles,
  and control buffer usage.
- Unix socket control API through `rttio ctl`.
- Control commands for status, protocol metadata, buffered reads, follow,
  writes, request/response exchanges, clear-buffer, reset, reconnect, flash,
  erase, and quit.
- JSON responses for automation and agent use.
- `.rttio` config persistence with versioning and repair of invalid config
  files.
- Feature-gated builds for `rtt`, `serial`, and `control`.

### Changed

- `jlink-rs` is consumed from a pinned git revision.
- The README is written for end users, with development commands moved to the
  development section.

### Notes

- Hardware-facing operations still require manual smoke testing on the intended
  probe, target, and serial hardware before final release.
