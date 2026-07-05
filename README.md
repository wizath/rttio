# rttio

Tio-like terminal for serial ports and SEGGER J-Link RTT.

`rttio` opens one target per process through an explicit transport command:
`serial` or `rtt`. It provides an interactive `Ctrl-T` command menu, optional
file logging, timestamps, local echo, hex output, auto-reconnect, raw TCP
serving, and a Unix socket control API for automation.

## Install

Build from this repository:

```sh
cargo build --release --bin rttio
```

Then run the produced binary:

```sh
./target/release/rttio --help
./target/release/rttio --version
```

During development you can use:

```sh
cargo run --bin rttio -- --help
```

## Serial

Open a serial console:

```sh
rttio serial /dev/tty.usbmodem101 --baud 115200
```

Useful serial options:

```sh
rttio serial /dev/tty.usbmodem101 \
  --baud 115200 \
  --flow-control none \
  --line-ending cr-lf \
  --local-echo \
  --log serial.txt
```

Flow control values:

```text
none
software
hardware
```

Connect to a ser2net/raw TCP serial endpoint:

```sh
rttio serial tcp://192.168.1.50:3001
```

## Direct J-Link RTT

Open RTT through a local J-Link probe:

```sh
rttio rtt nRF9151_xxCA --sn 801013229
```

Open RTT through J-Link Remote Server/IP:

```sh
rttio rtt nRF9151_xxCA --jlink-ip 192.168.1.10:19020
```

Useful RTT options:

```sh
rttio rtt nRF9151_xxCA \
  --sn 801013229 \
  --rtt-up 0 \
  --rtt-down 0 \
  --jlink-rtt-port 19031 \
  --jlink-speed 4000 \
  --log rtt.txt
```

Environment variables:

```text
JLINK_CHIP
JLINK_SN
JLINK_LIB
```

Direct J-Link RTT supports reset, erase, and flash from the interactive menu or
control socket.

Serial sessions built with ESP support can reset, erase, and flash ESP raw
`.bin` images from the interactive menu or control socket. After ESP flash or
erase, `rttio` reopens the serial monitor before returning success to `ctl`.

## RTT Stream

Connect to an RTT stream server instead of opening J-Link directly:

```sh
rttio rtt --rtt-port 19021
```

Use a non-local host:

```sh
rttio rtt --rtt-host 192.168.1.10 --rtt-port 19021
```

RTT stream mode is terminal-only. Reset, erase, and flash require direct J-Link
RTT mode.

## Serial-over-IP

`rttio` can expose the currently opened transport as a raw TCP byte stream.
This is async and can be used with either serial or RTT:

```sh
rttio serial /dev/ttyUSB0 --baud 115200 --serve 127.0.0.1:3001
rttio rtt nRF54L15_M33 --serve 127.0.0.1:3002
```

Bytes received from TCP clients are written to the active transport. Bytes read
from the active transport are broadcast to connected TCP clients.

## Interactive Menu

Press `Ctrl-T` to open the command menu. Target output is paused while the menu
is open and resumes when the menu closes.

```text
h / ?  help
l      clear screen
b      clear control buffer
e      toggle local echo
t      toggle timestamps
m      toggle normal/hex output
r      reset target
R      reconnect
f      flash file
q      quit
```

`Ctrl-C` is passed to the target. Exit with `Ctrl-T q`.

The status bar shows the selected target, serial/RTT connection state, output
mode, echo/timestamp flags, TX/RX activity, and control buffer usage. It is
kept outside the scroll region and is redrawn across terminal resizes.

## Logging

Write rendered terminal output to a file:

```sh
rttio serial /dev/tty.usbmodem101 --log serial.txt
```

By default the log file is truncated on startup. Append instead:

```sh
rttio serial /dev/tty.usbmodem101 --log serial.txt --log-append
```

Logging is buffered and flushed periodically, at shutdown, and when the internal
buffer crosses its flush threshold. If writing fails, logging is disabled and a
status line is printed.

## Control Socket

An interactive `rttio` instance creates `.rttio-sock`. A second `rttio` process
can control the running instance:

```sh
rttio ctl status --json
rttio ctl version --json
rttio ctl clear-buffer --json
rttio ctl read --timeout 200 --json
rttio ctl read --raw-hex --raw-text --timeout 200 --json
rttio ctl follow
rttio ctl write --target rtt -- "AT"
rttio ctl write --hex --target rtt -- 41 54 0d 0a
rttio ctl request --target rtt --timeout 1000 --until-hex 0d0a --json -- "AT"
rttio ctl request --target serial --hex --raw-hex --json -- 41 54 0d 0a
rttio ctl reset --json
rttio ctl flash --json --timeout 120000 build/app.hex
rttio ctl erase --json
rttio ctl quit
```

The client auto-discovers `.rttio-sock` by walking upward from the current
directory. Use a specific socket:

```sh
rttio ctl --socket ./path/to/.rttio-sock status --json
```

Control commands:

```text
status [--json]
version [--json]
commands [--json]
clear-buffer [--json]
read [--timeout ms] [--since <seq|now>] [--until-hex hex] [--max-bytes n] [--fail-on-timeout] [--raw-hex] [--raw-text] [--json]
follow
write [--target current|serial|rtt] [--timeout ms] [--hex] [--json] -- <text|hex>
writeln [--target current|serial|rtt] [--timeout ms] [--json] -- <text>
request [--target current|serial|rtt] [--timeout ms] [--since <seq|now>] [--until-hex hex] [--max-bytes n] [--fail-on-timeout] [--raw-hex] [--raw-text] [--hex] [--json] -- <text|hex>
reset [--json] [--timeout ms]
reconnect [--json] [--timeout ms]
flash [--json] [--timeout ms] <file> [addr]
erase [--json] [--timeout ms]
quit [--json]
```

JSON responses include `ok`. Error responses use `ok: false`, `code`, and
`error`. `status --json`, `commands --json`, and `version --json` expose
`rttio_version` and build `git_hash`; the existing `version` field is the
control protocol version. `read` and `request` read from the active opened
transport. By default JSON read responses include decoded `text`; add
`--raw-hex` or `--raw-text` when an automation agent needs raw bytes. `read`
cursors use byte sequence numbers; pass `next_seq` back as `--since`.

The control read buffer keeps the most recent 1 MiB of raw serial/RTT output.
Use `rttio ctl clear-buffer` or `Ctrl-T b` to drop it.

## Config

Runtime state is saved in `.rttio`:

```text
version
device
baud
serial
jlink_sn
jlink_ip
recent_flash
recent_flash_addr
```

Invalid JSON is ignored safely. The invalid file content is copied to
`.rttio.invalid`, `.rttio.invalid.1`, and so on, then defaults are used.

## Build Features

Default build includes serial, RTT, and control socket support:

```sh
cargo build --release --bin rttio
```

Build only serial support:

```sh
cargo build --release --no-default-features --features serial --bin rttio
```

Build only RTT support:

```sh
cargo build --release --no-default-features --features rtt --bin rttio
```

Build with control socket and one transport:

```sh
cargo build --release --no-default-features --features 'serial,control' --bin rttio
cargo build --release --no-default-features --features 'rtt,control' --bin rttio
```

At least one transport feature must be enabled: `serial` or `rtt`. The
`control` feature must be paired with a transport.

## Development

Run checks:

```sh
cargo fmt --check
cargo test --bin rttio
cargo clippy --all-targets -- -D warnings
```

`rttio` currently depends on `jlink-rs` from
`https://github.com/wizath/jlink-rs.git`, pinned to a known commit.
