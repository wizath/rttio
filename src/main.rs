use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
#[cfg(all(
    not(feature = "control"),
    not(any(feature = "rtt", feature = "serial"))
))]
compile_error!("enable at least one transport feature: \"rtt\" or \"serial\"");

#[cfg(all(feature = "control", not(any(feature = "rtt", feature = "serial"))))]
compile_error!("feature \"control\" requires feature \"rtt\" or \"serial\"");

#[cfg(feature = "rtt")]
use jlink_rs::{
    default_library_candidates, AsyncJLink, ConnectSpeed, JLink, JLinkHost, JlinkResult,
    OpenOptions, TargetInterface,
};
use serde::{Deserialize, Serialize};
#[cfg(feature = "control")]
use std::collections::VecDeque;
#[cfg(feature = "control")]
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::{
    fmt::Write as FmtWrite,
    fs,
    fs::OpenOptions as FsOpenOptions,
    io::{self, Write},
    panic,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Once, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(any(feature = "rtt", feature = "serial"))]
use tokio::net::{TcpListener, TcpStream};
#[cfg(any(feature = "control", feature = "rtt", feature = "serial"))]
use tokio::sync::broadcast;
#[cfg(feature = "control")]
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, BufReader},
    sync::Mutex,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};
#[cfg(feature = "control")]
use tokio::{
    net::{UnixListener, UnixStream},
    sync::Semaphore,
};
#[cfg(all(feature = "serial", feature = "espflash"))]
use tokio_serial::SerialPort;
#[cfg(feature = "serial")]
use tokio_serial::{FlowControl, SerialPortBuilderExt};

const CONFIG_FILE: &str = ".rttio";
const CONFIG_VERSION: u32 = 1;
const RTTIO_VERSION: &str = env!("CARGO_PKG_VERSION");
#[cfg_attr(not(feature = "control"), allow(dead_code))]
const RTTIO_GIT_HASH: &str = env!("RTTIO_GIT_HASH");
const RTTIO_FULL_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (git ",
    env!("RTTIO_GIT_HASH"),
    ")"
);
#[cfg(feature = "control")]
const DEFAULT_CONTROL_SOCKET: &str = ".rttio-sock";
#[cfg(feature = "control")]
const CONTROL_HISTORY_MAX_BYTES: usize = 1024 * 1024;
#[cfg(feature = "control")]
const CONTROL_MAX_COMMAND_BYTES: usize = 1024 * 1024;
#[cfg(feature = "control")]
const CONTROL_PROTOCOL_VERSION: u32 = 3;
#[cfg(feature = "control")]
const CONTROL_CURSOR_UNIT: &str = "byte";
#[cfg(all(feature = "control", not(test)))]
const CONTROL_MAX_CLIENTS: usize = 32;
#[cfg(all(feature = "control", test))]
const CONTROL_MAX_CLIENTS: usize = 1;
#[cfg(all(feature = "control", not(test)))]
const CONTROL_CLIENT_IDLE_TIMEOUT_MS: u64 = 60_000;
#[cfg(all(feature = "control", test))]
const CONTROL_CLIENT_IDLE_TIMEOUT_MS: u64 = 50;
#[cfg(all(feature = "control", not(test)))]
const CONTROL_CLIENT_DEFAULT_RESPONSE_TIMEOUT_MS: u64 = 5_000;
#[cfg(all(feature = "control", test))]
const CONTROL_CLIENT_DEFAULT_RESPONSE_TIMEOUT_MS: u64 = 250;
#[cfg(feature = "control")]
const CONTROL_CLIENT_RESPONSE_GRACE_MS: u64 = 1_000;
#[cfg(all(feature = "control", not(test)))]
const CONTROL_WRITE_ACK_TIMEOUT_MS: u64 = 5_000;
#[cfg(all(feature = "control", test))]
const CONTROL_WRITE_ACK_TIMEOUT_MS: u64 = 50;
#[cfg(all(feature = "control", not(test)))]
const CONTROL_ACTION_TIMEOUT_MS: u64 = 5_000;
#[cfg(all(feature = "control", test))]
const CONTROL_ACTION_TIMEOUT_MS: u64 = 50;
#[cfg(all(feature = "control", not(test)))]
const CONTROL_FLASH_TIMEOUT_MS: u64 = 120_000;
#[cfg(all(feature = "control", test))]
const CONTROL_FLASH_TIMEOUT_MS: u64 = 50;
#[cfg(feature = "control")]
const CONTROL_MAX_TIMEOUT_MS: u64 = 600_000;
#[cfg(feature = "control")]
const CONTROL_COMMANDS_HELP: &str = "commands: version [--json], status [--json], clear-buffer [--json], read [--timeout ms] [--since <seq|now>] [--until-hex hex] [--max-bytes n] [--fail-on-timeout] [--raw-hex] [--raw-text] [--json], follow, write [--target current|serial|rtt] [--timeout ms] [--hex] [--json] <text|hex>, writeln [--target ...] [--timeout ms] [--json] <text>, request [--target ...] [--timeout ms] [--since <seq|now>] [--until-hex hex] [--max-bytes n] [--fail-on-timeout] [--raw-hex] [--raw-text] [--hex] [--json] <text|hex>, reset [--json] [--timeout ms], reconnect [--json] [--timeout ms], flash [--json] [--timeout ms] <file|\"file with spaces\"> [addr], erase [--json] [--timeout ms], quit [--json]";

type ControlReply = tokio::sync::oneshot::Sender<String>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Source {
    Serial,
    Rtt,
    Tx,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Source::Serial => "serial",
            Source::Rtt => "rtt",
            Source::Tx => "tx",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Route {
    Serial,
    Rtt,
    Both,
}

impl Route {
    #[cfg(feature = "control")]
    fn as_ctl_str(self) -> &'static str {
        match self {
            Route::Serial => "serial",
            Route::Rtt => "rtt",
            Route::Both => "both",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputMode {
    Normal,
    Hex,
}

impl OutputMode {
    fn as_ctl_str(self) -> &'static str {
        match self {
            OutputMode::Normal => "normal",
            OutputMode::Hex => "hex",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LineEnding {
    Lf,
    CrLf,
    None,
}

#[cfg(feature = "serial")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

#[cfg(feature = "serial")]
impl From<SerialFlowControl> for FlowControl {
    fn from(value: SerialFlowControl) -> Self {
        match value {
            SerialFlowControl::None => FlowControl::None,
            SerialFlowControl::Software => FlowControl::Software,
            SerialFlowControl::Hardware => FlowControl::Hardware,
        }
    }
}

impl LineEnding {
    fn bytes(self) -> &'static [u8] {
        match self {
            LineEnding::Lf => b"\n",
            LineEnding::CrLf => b"\r\n",
            LineEnding::None => b"",
        }
    }

    #[cfg(feature = "control")]
    fn as_ctl_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "lf",
            LineEnding::CrLf => "crlf",
            LineEnding::None => "none",
        }
    }
}

#[cfg(feature = "control")]
impl ControlSource {
    fn matches(self, source: Source) -> bool {
        match self {
            ControlSource::Any => matches!(source, Source::Serial | Source::Rtt),
            ControlSource::Serial => source == Source::Serial,
            ControlSource::Rtt => source == Source::Rtt,
        }
    }
}

#[derive(Debug)]
enum InputEvent {
    Bytes(Vec<u8>),
    Line(String),
    MenuCommand(MenuCommand),
    #[cfg(feature = "control")]
    Control(ControlRequest),
    Quit,
}

#[cfg(feature = "control")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlTarget {
    Current,
    Serial,
    Rtt,
}

#[cfg(feature = "control")]
impl ControlTarget {
    fn as_ctl_str(self) -> &'static str {
        match self {
            ControlTarget::Current => "current",
            ControlTarget::Serial => "serial",
            ControlTarget::Rtt => "rtt",
        }
    }
}

#[cfg(feature = "control")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlSource {
    Any,
    Serial,
    Rtt,
}

#[cfg(feature = "control")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlSince {
    Seq(u64),
    Now,
}

#[cfg(feature = "control")]
impl ControlSince {
    fn as_wire(self) -> String {
        match self {
            ControlSince::Seq(seq) => seq.to_string(),
            ControlSince::Now => "now".to_string(),
        }
    }
}

#[cfg(feature = "control")]
#[derive(Debug)]
enum ControlRequest {
    Write {
        target: ControlTarget,
        bytes: Vec<u8>,
        timeout: Duration,
        reply: ControlReply,
    },
    Reset {
        reply: ControlReply,
    },
    Flash {
        path: PathBuf,
        addr: u32,
        reply: ControlReply,
    },
    Erase {
        reply: ControlReply,
    },
    Reconnect {
        reply: ControlReply,
    },
}

#[derive(Debug)]
enum MenuCommand {
    ClearScreen,
    ClearControlBuffer,
    ToggleOutputPause,
    ToggleEcho,
    ToggleTimestamps,
    ToggleOutputMode,
    Reconnect,
    Reset,
    Flash { path: PathBuf, addr: u32 },
    Erase,
}

#[derive(Debug)]
enum InterfaceCommand {
    Write {
        data: Vec<u8>,
        reply: Option<ControlReply>,
    },
    Reconnect {
        reply: Option<ControlReply>,
    },
    Reset {
        reply: Option<ControlReply>,
    },
    Flash {
        path: PathBuf,
        addr: u32,
        reply: Option<ControlReply>,
    },
    Erase {
        reply: Option<ControlReply>,
    },
    Stop,
}

#[derive(Debug)]
enum InterfaceEvent {
    Data {
        source: Source,
        data: Vec<u8>,
    },
    Status {
        source: Source,
        text: String,
    },
    Error {
        source: Source,
        text: String,
    },
    #[cfg(feature = "control")]
    FlashProgress(Option<TerminalFlashProgress>),
    Stopped(Source),
}

#[derive(Debug)]
enum TerminalEvent {
    Output(String),
    Status(String),
    SetUiState(TerminalUiState),
    ClearScreen,
    #[cfg(feature = "control")]
    SetStatusBar(TerminalStatusBar),
    #[cfg(feature = "control")]
    SetFlashProgress(Option<TerminalFlashProgress>),
    #[cfg(feature = "control")]
    Activity(Source),
    #[cfg(feature = "control")]
    Resize,
    ShowMenu(usize),
    ShowHelp,
    HideMenu,
    Exit,
}

#[derive(Clone, Copy, Debug)]
struct TerminalUiState {
    output_mode: OutputMode,
    timestamp: bool,
    local_echo: bool,
    output_paused: bool,
    jlink_actions: bool,
}

#[cfg(feature = "control")]
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct TerminalStatusBar {
    target: &'static str,
    target_label: String,
    serial_running: bool,
    rtt_running: bool,
    output_mode: OutputMode,
    timestamp: bool,
    local_echo: bool,
    output_paused: bool,
    history_bytes: usize,
    history_max_bytes: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct TerminalFlashProgress {
    action: String,
    percent: i32,
}

impl Default for TerminalUiState {
    fn default() -> Self {
        Self {
            output_mode: OutputMode::Normal,
            timestamp: false,
            local_echo: false,
            output_paused: false,
            jlink_actions: false,
        }
    }
}

#[cfg(feature = "control")]
#[derive(Clone, Debug)]
struct ControlOutput {
    seq: u64,
    source: Source,
    data: Vec<u8>,
}

#[cfg(feature = "control")]
#[derive(Debug)]
struct ControlHistory {
    next_seq: u64,
    max_bytes: usize,
    bytes: usize,
    entries: VecDeque<ControlHistoryEntry>,
}

#[cfg(feature = "control")]
#[derive(Clone, Debug)]
struct ControlHistoryEntry {
    seq: u64,
    source: Source,
    data: Vec<u8>,
}

#[cfg(feature = "control")]
#[derive(Debug)]
struct ControlHistorySnapshot {
    next_seq: u64,
    dropped_before: u64,
    data_seq: u64,
    data: Vec<u8>,
}

#[cfg(feature = "control")]
#[derive(Clone, Debug)]
struct ControlRuntimeState {
    control_socket: PathBuf,
    serial_configured: bool,
    rtt_configured: bool,
    serial_path: Option<PathBuf>,
    baud: Option<u32>,
    jlink_sn: Option<u32>,
    jlink_ip: Option<String>,
    device: Option<String>,
    rtt_tcp_host: Option<String>,
    rtt_tcp_port: Option<u16>,
    rtt_up: u32,
    rtt_down: u32,
    serial_running: bool,
    rtt_running: bool,
    route: Route,
    output_mode: OutputMode,
    timestamp: bool,
    local_echo: bool,
    output_paused: bool,
    line_ending: LineEnding,
}

#[cfg(feature = "control")]
impl ControlHistory {
    fn new(max_bytes: usize) -> Self {
        Self {
            next_seq: 1,
            max_bytes,
            bytes: 0,
            entries: VecDeque::new(),
        }
    }

    fn push(&mut self, source: Source, data: Vec<u8>) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(data.len().max(1) as u64);
        self.bytes = self.bytes.saturating_add(data.len());
        self.entries
            .push_back(ControlHistoryEntry { seq, source, data });
        self.trim();
        seq
    }

    fn clear(&mut self) -> usize {
        let cleared = self.bytes;
        self.entries.clear();
        self.bytes = 0;
        cleared
    }

    fn bytes(&self) -> usize {
        self.bytes
    }

    fn snapshot(&self, source: ControlSource, since: Option<u64>) -> ControlHistorySnapshot {
        let dropped_before = self
            .entries
            .iter()
            .find(|entry| source.matches(entry.source))
            .map(|entry| entry.seq)
            .unwrap_or(self.next_seq);
        let since = since.unwrap_or(self.next_seq);
        let mut data = Vec::new();
        let mut data_seq = self.next_seq;
        for entry in &self.entries {
            let entry_end = entry.seq.saturating_add(entry.data.len().max(1) as u64);
            if entry_end <= since || !source.matches(entry.source) {
                continue;
            }
            let offset = since.saturating_sub(entry.seq) as usize;
            if offset < entry.data.len() {
                if data.is_empty() {
                    data_seq = entry.seq.saturating_add(offset as u64);
                }
                data.extend_from_slice(&entry.data[offset..]);
            }
        }
        ControlHistorySnapshot {
            next_seq: self.next_seq,
            dropped_before,
            data_seq,
            data,
        }
    }

    fn trim(&mut self) {
        while self.bytes > self.max_bytes {
            let Some(entry) = self.entries.pop_front() else {
                self.bytes = 0;
                break;
            };
            self.bytes = self.bytes.saturating_sub(entry.data.len());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct RttioConfig {
    #[serde(default = "default_config_version")]
    version: u32,
    target: Option<ConfigTarget>,
    serial: Option<PathBuf>,
    baud: Option<u32>,
    jlink_sn: Option<u32>,
    jlink_ip: Option<String>,
    device: Option<String>,
    recent_flash: Vec<PathBuf>,
    recent_flash_addr: Vec<RecentFlashAddress>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
struct RecentFlashAddress {
    path: PathBuf,
    addr: u32,
}

fn default_config_version() -> u32 {
    CONFIG_VERSION
}

impl Default for RttioConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            target: None,
            serial: None,
            baud: None,
            jlink_sn: None,
            jlink_ip: None,
            device: None,
            recent_flash: Vec::new(),
            recent_flash_addr: Vec::new(),
        }
    }
}

impl RttioConfig {
    fn normalize(&mut self) {
        if self.version == 0 || self.version > CONFIG_VERSION {
            self.version = CONFIG_VERSION;
        }
        self.recent_flash.truncate(10);
        self.recent_flash_addr
            .retain(|entry| self.recent_flash.iter().any(|path| path == &entry.path));
        self.recent_flash_addr.truncate(10);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ConfigTarget {
    Serial,
    Rtt,
}

#[derive(Debug)]
struct OutputLineState {
    serial: bool,
    rtt: bool,
    tx: bool,
}

impl OutputLineState {
    fn new() -> Self {
        Self {
            serial: true,
            rtt: true,
            tx: true,
        }
    }

    fn at_line_start_mut(&mut self, source: Source) -> &mut bool {
        match source {
            Source::Serial => &mut self.serial,
            Source::Rtt => &mut self.rtt,
            Source::Tx => &mut self.tx,
        }
    }
}

impl Default for OutputLineState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "rttio",
    version = RTTIO_FULL_VERSION,
    about = "tio-like terminal for serial ports and J-Link RTT"
)]
struct Opts {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(
        long,
        global = true,
        default_value_t = 1024,
        value_parser = parse_nonzero_usize,
        help = "Transport read chunk size in bytes",
        help_heading = "Terminal"
    )]
    chunk: usize,

    #[arg(
        long,
        global = true,
        default_value_t = 10,
        value_parser = parse_nonzero_u64,
        help = "RTT/TCP poll interval in milliseconds",
        help_heading = "Terminal"
    )]
    poll_ms: u64,

    #[arg(long, global = true, value_enum, default_value_t = OutputMode::Normal, help = "Terminal output mode", help_heading = "Terminal")]
    output_mode: OutputMode,

    #[arg(long, global = true, value_enum, default_value_t = LineEnding::CrLf, help = "Line ending appended to typed lines", help_heading = "Terminal")]
    line_ending: LineEnding,

    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Prefix rendered output lines with local timestamps",
        help_heading = "Terminal"
    )]
    timestamp: bool,

    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Echo typed input locally",
        help_heading = "Terminal"
    )]
    local_echo: bool,

    #[arg(
        long = "log",
        global = true,
        value_name = "FILE",
        help = "Write rendered terminal output to file",
        help_heading = "Logging"
    )]
    log_file: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Append to log file instead of truncating",
        help_heading = "Logging"
    )]
    log_append: bool,

    #[cfg(feature = "serial")]
    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Disable automatic serial reconnect",
        help_heading = "Reconnect"
    )]
    no_reconnect: bool,

    #[cfg(feature = "rtt")]
    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Enable automatic RTT/J-Link reconnect after a successful connection drops",
        help_heading = "Reconnect"
    )]
    rtt_reconnect: bool,

    #[cfg(any(feature = "rtt", feature = "serial"))]
    #[arg(
        long,
        global = true,
        default_value_t = 1000,
        help = "Reconnect delay in milliseconds",
        help_heading = "Reconnect"
    )]
    reconnect_delay_ms: u64,

    #[arg(
        long = "no-config",
        global = true,
        default_value_t = false,
        help = "Ignore .rttio for this run and do not update it",
        help_heading = "Config"
    )]
    no_config: bool,

    /// Unix socket path for the local control API.
    #[cfg(feature = "control")]
    #[arg(long, global = true, default_value = DEFAULT_CONTROL_SOCKET, help_heading = "Control")]
    socket: PathBuf,
}

fn parse_nonzero_usize(input: &str) -> std::result::Result<usize, String> {
    let value = input
        .parse::<usize>()
        .map_err(|e| format!("invalid positive integer {input:?}: {e}"))?;
    if value == 0 {
        Err("value must be greater than zero".to_string())
    } else {
        Ok(value)
    }
}

fn parse_nonzero_u64(input: &str) -> std::result::Result<u64, String> {
    let value = input
        .parse::<u64>()
        .map_err(|e| format!("invalid positive integer {input:?}: {e}"))?;
    if value == 0 {
        Err("value must be greater than zero".to_string())
    } else {
        Ok(value)
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    #[cfg(feature = "rtt")]
    #[command(about = "Open a J-Link RTT terminal")]
    Rtt(RttOpts),
    #[cfg(feature = "serial")]
    #[command(about = "Open a serial terminal")]
    Serial(SerialOpts),
    #[cfg(feature = "rtt")]
    #[command(about = "List connected J-Link probes and exit")]
    Probes(ProbesOpts),
    #[cfg(feature = "rtt")]
    #[command(about = "List SEGGER J-Link target devices and exit")]
    Devices(DevicesOpts),
    #[cfg(feature = "rtt")]
    #[command(about = "Open SEGGER's native target-device picker and print the selected device")]
    PickDevice(PickDeviceOpts),
    #[cfg(feature = "control")]
    #[command(about = "Control an already running rttio instance through its Unix socket")]
    Ctl(CtlOpts),
}

#[cfg(feature = "rtt")]
#[derive(Args, Debug)]
struct RttOpts {
    /// J-Link target chip name. Can also be set with JLINK_CHIP.
    chip: Option<String>,

    #[arg(
        long,
        help = "J-Link probe serial number",
        help_heading = "RTT / J-Link"
    )]
    sn: Option<u32>,

    #[arg(
        long,
        help = "J-Link Remote Server address, optionally HOST:PORT",
        help_heading = "RTT / J-Link"
    )]
    jlink_ip: Option<String>,

    #[arg(
        long,
        help = "Explicit SEGGER J-Link shared library path",
        help_heading = "RTT / J-Link"
    )]
    jlink_lib: Option<PathBuf>,

    #[arg(
        long,
        default_value = "4000",
        value_parser = parse_connect_speed,
        help = "J-Link connection speed: auto, adaptive, or kHz value",
        help_heading = "RTT / J-Link"
    )]
    jlink_speed: ConnectSpeed,

    #[arg(
        long,
        help = "Configure the J-Link DLL RTT Telnet server port in direct J-Link mode",
        help_heading = "RTT / J-Link"
    )]
    jlink_rtt_port: Option<u16>,

    #[arg(
        long,
        default_value_t = 0,
        help = "RTT up channel to read",
        help_heading = "RTT / J-Link"
    )]
    rtt_up: u32,

    #[arg(
        long,
        default_value_t = 0,
        help = "RTT down channel to write",
        help_heading = "RTT / J-Link"
    )]
    rtt_down: u32,

    #[arg(
        long = "rtt-port",
        help = "Connect to an RTT stream server port instead of direct J-Link",
        help_heading = "RTT / J-Link"
    )]
    rtt_tcp_port: Option<u16>,

    #[arg(
        long = "rtt-host",
        default_value = "127.0.0.1",
        help = "RTT stream server host",
        help_heading = "RTT / J-Link"
    )]
    rtt_tcp_host: String,

    #[arg(
        long,
        value_name = "HOST:PORT",
        help = "Serve the active RTT terminal as raw TCP serial-over-IP",
        help_heading = "Network"
    )]
    serve: Option<String>,
}

#[cfg(feature = "serial")]
#[derive(Args, Debug)]
struct SerialOpts {
    /// Serial port path or raw TCP endpoint, for example /dev/ttyUSB0, COM7, or tcp://host:3001.
    port: PathBuf,

    #[arg(long, help = "Serial baud rate", help_heading = "Serial")]
    baud: Option<u32>,

    #[arg(
        long,
        value_enum,
        default_value_t = SerialFlowControl::None,
        help = "Serial flow control",
        help_heading = "Serial"
    )]
    flow_control: SerialFlowControl,

    #[cfg(feature = "espflash")]
    #[arg(
        long,
        value_parser = parse_esp_chip,
        help = "ESP chip for serial flashing, for example esp32s3; omitted = autodetect",
        help_heading = "Serial / ESP"
    )]
    esp_chip: Option<::espflash::target::Chip>,

    #[cfg(feature = "espflash")]
    #[arg(
        long,
        default_value_t = 921_600,
        help = "ESP flashing baud rate",
        help_heading = "Serial / ESP"
    )]
    espflash_baud: u32,

    #[arg(
        long,
        value_name = "HOST:PORT",
        help = "Serve the active serial terminal as raw TCP serial-over-IP",
        help_heading = "Network"
    )]
    serve: Option<String>,
}

#[cfg(feature = "rtt")]
#[derive(Args, Debug)]
struct ProbesOpts {
    #[arg(long, help = "Explicit SEGGER J-Link shared library path")]
    jlink_lib: Option<PathBuf>,
    #[arg(long, help = "Only show this J-Link probe serial number")]
    sn: Option<u32>,
}

#[cfg(feature = "rtt")]
#[derive(Args, Debug)]
struct DevicesOpts {
    #[arg(help = "Optional target name/manufacturer filter")]
    filter: Option<String>,
    #[arg(long, help = "Explicit SEGGER J-Link shared library path")]
    jlink_lib: Option<PathBuf>,
}

#[cfg(feature = "rtt")]
#[derive(Args, Debug)]
struct PickDeviceOpts {
    #[arg(long, help = "Explicit SEGGER J-Link shared library path")]
    jlink_lib: Option<PathBuf>,
}

#[cfg(feature = "control")]
#[derive(Args, Debug)]
#[command(
    long_about = "Control an already running rttio process through its local Unix socket.\n\nThe interactive rttio process owns the serial/RTT target and creates .rttio-sock. A second rttio process can use `rttio ctl ...` to inspect status, write bytes, read buffered output, clear the read buffer, perform request/response exchanges, flash, reset, erase, reconnect, or stop the running process.\n\nPayload rule: commands that send text or hex accept payload after `--`. Use `--hex` on write/request when the payload is hexadecimal bytes. Read cursors are byte sequence numbers returned as `next_seq` in JSON responses.",
    after_long_help = "Command reference:\n  version [--json]                        rttio binary version, git hash, and protocol version.\n  status [--json]                         Runtime state and selected target.\n  commands [--json]                       Machine-readable protocol metadata.\n  clear-buffer [--json]                   Drop buffered output history.\n  read [--raw-hex] [--raw-text] [...]     Read buffered output from the active transport.\n  follow                                  Stream rendered terminal output.\n  write [--target current|serial|rtt] [--hex] [--json] -- <payload>\n                                          Write text or hex bytes.\n  writeln [--target current|serial|rtt] [--json] -- <text>\n                                          Write text plus configured line ending.\n  request [--target ...] [--raw-hex] [--raw-text] [--until-hex HEX] [--hex] [--json] -- <payload>\n                                          Write, then read the response from the active transport.\n  reset [--json]                          Reset target through the active transport flasher.\n  reconnect [--json]                      Reconnect active transport.\n  flash [--json] [--timeout MS] <file> [addr]\n                                          Flash through the active transport: J-Link RTT or ESP serial.\n  erase [--json]                          Erase through the active transport flasher.\n  quit [--json]                           Stop running rttio.\n\nExamples:\n  rttio ctl version --json\n  rttio ctl status --json\n  rttio ctl commands --json\n  rttio ctl clear-buffer --json\n  rttio ctl read --timeout 200 --json\n  rttio ctl read --raw-hex --raw-text --timeout 200 --json\n  rttio ctl follow\n  rttio ctl write --target current -- \"AT+CFUN?\"\n  rttio ctl write --target rtt --hex -- 41 54 0d 0a\n  rttio ctl request --target rtt --timeout 1000 --until-hex 0d0a --json -- \"AT\"\n  rttio ctl request --target serial --hex --raw-hex --json -- 41 54 0d 0a\n  rttio ctl reset --json\n  rttio ctl flash --json --timeout 120000 build/app.hex 0x0\n  rttio ctl erase --json\n  rttio ctl quit\n\nSocket discovery:\n  By default ctl walks upward from the current directory looking for .rttio-sock.\n  Use --socket PATH to address a specific running instance.\n\nJSON contract:\n  Success responses contain ok=true. Error responses contain ok=false, code, and error.\n  read/request JSON responses include next_seq; pass that value back with --since.\n"
)]
struct CtlOpts {
    #[arg(
        long,
        help = "Socket path; defaults to upward auto-discovery of .rttio-sock"
    )]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: CtlCommand,
}

#[cfg(feature = "control")]
#[derive(Subcommand, Debug)]
enum CtlCommand {
    #[command(about = "Show rttio binary version and build git hash")]
    Version {
        #[arg(long, default_value_t = false, help = "Print JSON version metadata")]
        json: bool,
    },
    #[command(about = "Show runtime status and selected target")]
    Status {
        #[arg(long, default_value_t = false, help = "Print JSON status")]
        json: bool,
    },
    #[command(about = "Show machine-readable control protocol metadata")]
    Commands {
        #[arg(long, default_value_t = false, help = "Print JSON command metadata")]
        json: bool,
    },
    #[command(about = "Drop buffered output history")]
    ClearBuffer {
        #[arg(long, default_value_t = false, help = "Print JSON action result")]
        json: bool,
    },
    #[command(about = "Read buffered output without writing to the target")]
    Read {
        #[arg(long, default_value_t = 200, help = "Read timeout in milliseconds")]
        timeout: u64,
        #[arg(long, help = "Start cursor byte sequence, or 'now'")]
        since: Option<String>,
        #[arg(long, help = "Stop after this hex byte delimiter is received")]
        until_hex: Option<String>,
        #[arg(long, help = "Limit returned response bytes")]
        max_bytes: Option<usize>,
        #[arg(long, default_value_t = false, help = "Return error on read timeout")]
        fail_on_timeout: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Include hex field in JSON output"
        )]
        raw_hex: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Include lossy UTF-8 text field in JSON output"
        )]
        raw_text: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Print JSON response with text and cursor fields"
        )]
        json: bool,
    },
    #[command(about = "Stream rendered terminal output until interrupted")]
    Follow,
    #[command(about = "Write text or hex bytes to current, serial, or RTT target")]
    Write {
        #[arg(
            long,
            value_enum,
            default_value_t = CtlTargetArg::Current,
            help = "Write target: current, serial, or rtt"
        )]
        target: CtlTargetArg,
        #[arg(long, default_value_t = false, help = "Print JSON acknowledgement")]
        json: bool,
        #[arg(long, default_value_t = false, help = "Treat payload as hex bytes")]
        hex: bool,
        #[arg(long, default_value_t = CONTROL_WRITE_ACK_TIMEOUT_MS, help = "Write acknowledgement timeout in milliseconds")]
        timeout: u64,
        #[arg(help = "Payload words; use -- before payload that starts with '-'")]
        text: Vec<String>,
    },
    #[command(about = "Write text plus rttio's configured line ending")]
    Writeln {
        #[arg(
            long,
            value_enum,
            default_value_t = CtlTargetArg::Current,
            help = "Write target: current, serial, or rtt"
        )]
        target: CtlTargetArg,
        #[arg(long, default_value_t = false, help = "Print JSON acknowledgement")]
        json: bool,
        #[arg(long, default_value_t = CONTROL_WRITE_ACK_TIMEOUT_MS, help = "Write acknowledgement timeout in milliseconds")]
        timeout: u64,
        #[arg(help = "Text payload words; use -- before payload that starts with '-'")]
        text: Vec<String>,
    },
    #[command(about = "Write payload, then read response bytes from buffered output")]
    Request {
        #[arg(
            long,
            value_enum,
            default_value_t = CtlTargetArg::Current,
            help = "Write target: current, serial, or rtt"
        )]
        target: CtlTargetArg,
        #[arg(
            long,
            default_value_t = 500,
            help = "Read response timeout in milliseconds"
        )]
        timeout: u64,
        #[arg(long, help = "Start cursor byte sequence, or 'now'")]
        since: Option<String>,
        #[arg(long, help = "Stop after this hex byte delimiter is received")]
        until_hex: Option<String>,
        #[arg(long, help = "Limit returned response bytes")]
        max_bytes: Option<usize>,
        #[arg(long, default_value_t = false, help = "Return error on read timeout")]
        fail_on_timeout: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Include response hex field in JSON output"
        )]
        raw_hex: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Include response lossy UTF-8 text field in JSON output"
        )]
        raw_text: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Treat request payload as hex bytes"
        )]
        hex: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Print JSON response with write and read metadata"
        )]
        json: bool,
        #[arg(help = "Request payload words; use -- before payload that starts with '-'")]
        text: Vec<String>,
    },
    #[command(about = "Reset target through the active transport flasher")]
    Reset {
        #[arg(long, default_value_t = false, help = "Print JSON action result")]
        json: bool,
        #[arg(long, default_value_t = CONTROL_ACTION_TIMEOUT_MS, help = "Action timeout in milliseconds")]
        timeout: u64,
    },
    #[command(about = "Reconnect the active transport")]
    Reconnect {
        #[arg(long, default_value_t = false, help = "Print JSON action result")]
        json: bool,
        #[arg(long, default_value_t = CONTROL_ACTION_TIMEOUT_MS, help = "Action timeout in milliseconds")]
        timeout: u64,
    },
    #[command(about = "Flash through the active transport: J-Link RTT or ESP serial")]
    Flash {
        #[arg(long, default_value_t = false, help = "Print JSON flash result")]
        json: bool,
        #[arg(help = "Firmware file path")]
        file: PathBuf,
        #[arg(default_value = "0x0", value_parser = parse_u32, help = "Flash address for raw/bin/hex files")]
        addr: u32,
        #[arg(long, default_value_t = CONTROL_FLASH_TIMEOUT_MS, help = "Flash timeout in milliseconds")]
        timeout: u64,
    },
    #[command(about = "Erase through the active transport flasher")]
    Erase {
        #[arg(long, default_value_t = false, help = "Print JSON action result")]
        json: bool,
        #[arg(long, default_value_t = CONTROL_ACTION_TIMEOUT_MS, help = "Action timeout in milliseconds")]
        timeout: u64,
    },
    #[command(about = "Stop the running rttio process")]
    Quit {
        #[arg(long, default_value_t = false, help = "Print JSON action result")]
        json: bool,
    },
}

#[cfg(feature = "control")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CtlTargetArg {
    Current,
    Serial,
    Rtt,
}

#[cfg(feature = "control")]
impl CtlTargetArg {
    fn as_ctl_str(self) -> &'static str {
        match self {
            CtlTargetArg::Current => "current",
            CtlTargetArg::Serial => "serial",
            CtlTargetArg::Rtt => "rtt",
        }
    }
}

mod app;
mod config;
#[cfg(feature = "control")]
mod control;
#[cfg(feature = "espflash")]
mod espflash;
mod input_menu;
mod runtime;
mod terminal;
mod transports;

use app::*;
use config::*;
#[cfg(feature = "control")]
use control::*;
#[cfg(feature = "espflash")]
use espflash::*;
use input_menu::*;
use runtime::*;
use terminal::*;
use transports::*;

#[tokio::main]
async fn main() -> Result<()> {
    run_app(Opts::parse()).await
}

#[cfg(all(test, feature = "control"))]
mod tests;
