# Changelog

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
