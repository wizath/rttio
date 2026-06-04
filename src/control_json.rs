#[derive(Serialize)]
pub(crate) struct ControlErrorResponse {
    pub(crate) ok: bool,
    pub(crate) code: &'static str,
    pub(crate) error: String,
}

#[derive(Serialize)]
pub(crate) struct ControlActionResponse {
    pub(crate) ok: bool,
    pub(crate) command: &'static str,
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) reported_result: Option<String>,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct ControlWriteResponse {
    pub(crate) ok: bool,
    pub(crate) command: &'static str,
    pub(crate) target: &'static str,
    pub(crate) actual_targets: Vec<&'static str>,
    pub(crate) bytes: usize,
    pub(crate) timeout_ms: u64,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct ControlFlashResponse {
    pub(crate) ok: bool,
    pub(crate) command: &'static str,
    pub(crate) file: String,
    pub(crate) addr: u32,
    pub(crate) timeout_ms: u64,
    pub(crate) reported_bytes: Option<usize>,
    pub(crate) message: String,
}

pub(crate) fn collect_control_commands_json() -> String {
    let response = ControlCommandsResponse {
        ok: true,
        protocol: "rttio-control",
        version: CONTROL_PROTOCOL_VERSION,
        rttio_version: RTTIO_VERSION,
        git_hash: RTTIO_GIT_HASH,
        socket: DEFAULT_CONTROL_SOCKET,
        payload_separator: "--",
        error_codes: CONTROL_ERROR_CODES,
        error_fields: CONTROL_ERROR_FIELDS,
        default_timeouts_ms: ControlDefaultTimeouts {
            read: 200,
            request_read: 500,
            write: CONTROL_WRITE_ACK_TIMEOUT_MS,
            action: CONTROL_ACTION_TIMEOUT_MS,
            flash: CONTROL_FLASH_TIMEOUT_MS,
            client_response_grace: CONTROL_CLIENT_RESPONSE_GRACE_MS,
            client_idle: CONTROL_CLIENT_IDLE_TIMEOUT_MS,
        },
        commands: &CONTROL_COMMAND_SPECS,
        features: ControlFeatureFlags {
            json: true,
            raw_hex: true,
            raw_text: true,
            request_response: true,
            action_timeout: true,
            error_codes: true,
            until_hex: true,
            flash: true,
            erase: true,
            reset: true,
            follow: true,
            complete: true,
            timed_out: true,
            fail_on_timeout: true,
            byte_cursor: true,
            bounded_reads: true,
            structured_jlink_results: true,
            max_read_bytes: CONTROL_HISTORY_MAX_BYTES,
            max_timeout_ms: CONTROL_MAX_TIMEOUT_MS,
            auto_discover_socket: true,
        },
    };
    serialize_json_line(&response)
}

#[derive(Serialize)]
pub(crate) struct ControlCommandsResponse<'a> {
    ok: bool,
    protocol: &'static str,
    version: u32,
    rttio_version: &'static str,
    git_hash: &'static str,
    socket: &'static str,
    payload_separator: &'static str,
    error_codes: &'a [&'static str],
    error_fields: &'a [&'static str],
    default_timeouts_ms: ControlDefaultTimeouts,
    commands: &'a [ControlCommandSpec],
    features: ControlFeatureFlags,
}

#[derive(Serialize)]
pub(crate) struct ControlDefaultTimeouts {
    read: u64,
    request_read: u64,
    write: u64,
    action: u64,
    flash: u64,
    client_response_grace: u64,
    client_idle: u64,
}

#[derive(Serialize)]
pub(crate) struct ControlFeatureFlags {
    json: bool,
    raw_hex: bool,
    raw_text: bool,
    request_response: bool,
    action_timeout: bool,
    error_codes: bool,
    until_hex: bool,
    flash: bool,
    erase: bool,
    reset: bool,
    follow: bool,
    complete: bool,
    timed_out: bool,
    fail_on_timeout: bool,
    byte_cursor: bool,
    bounded_reads: bool,
    structured_jlink_results: bool,
    max_read_bytes: usize,
    max_timeout_ms: u64,
    auto_discover_socket: bool,
}

#[derive(Serialize)]
pub(crate) struct ControlCommandSpec {
    name: &'static str,
    json: bool,
    targets: &'static [&'static str],
    sources: &'static [&'static str],
    options: &'static [&'static str],
    payload: &'static str,
    response: &'static str,
    fields: &'static [&'static str],
    example: &'static str,
}

pub(crate) const CONTROL_ERROR_CODES: &[&str] = &[
    "timeout",
    "unknown_command",
    "unknown_option",
    "invalid_path",
    "invalid_argument",
    "not_running",
    "unavailable",
    "command_failed",
];
pub(crate) const CONTROL_ERROR_FIELDS: &[&str] = &["ok", "code", "error"];
pub(crate) const CONTROL_TARGETS: &[&str] = &["current", "serial", "rtt"];
pub(crate) const CONTROL_READ_SOURCES: &[&str] = &["active"];
pub(crate) const CONTROL_NO_TARGETS: &[&str] = &[];
pub(crate) const CONTROL_NO_SOURCES: &[&str] = &[];
pub(crate) const CONTROL_STATUS_FIELDS: &[&str] = &[
    "protocol",
    "version",
    "rttio_version",
    "git_hash",
    "pid",
    "cwd",
    "control_socket",
    "cursor_unit",
    "serial_configured",
    "rtt_configured",
    "serial_path",
    "baud",
    "jlink_sn",
    "jlink_ip",
    "device",
    "rtt_tcp_host",
    "rtt_tcp_port",
    "rtt_up",
    "rtt_down",
    "serial_running",
    "rtt_running",
    "route",
    "output_mode",
    "timestamp",
    "local_echo",
    "line_ending",
    "history_max_bytes",
    "next_seq",
    "dropped_before",
    "serial_next_seq",
    "serial_dropped_before",
    "rtt_next_seq",
    "rtt_dropped_before",
];
pub(crate) const CONTROL_RAW_READ_FIELDS: &[&str] = &[
    "ok",
    "source",
    "bytes",
    "text",
    "cursor_unit",
    "next_seq",
    "dropped_before",
    "complete",
    "matched_until_hex",
    "limited",
    "timed_out",
];
pub(crate) const CONTROL_REQUEST_READ_FIELDS: &[&str] = &[
    "ok",
    "command",
    "target",
    "actual_targets",
    "written_bytes",
    "write_timeout_ms",
    "read_timeout_ms",
    "response",
    "response.source",
    "response.bytes",
    "response.text",
    "response.cursor_unit",
    "response.next_seq",
    "response.dropped_before",
    "response.complete",
    "response.matched_until_hex",
    "response.limited",
    "response.timed_out",
];
pub(crate) const CONTROL_WRITE_FIELDS: &[&str] = &[
    "ok",
    "command",
    "target",
    "actual_targets",
    "bytes",
    "timeout_ms",
    "message",
];
pub(crate) const CONTROL_ACTION_FIELDS: &[&str] =
    &["ok", "command", "timeout_ms", "reported_result", "message"];
pub(crate) const CONTROL_FLASH_FIELDS: &[&str] = &[
    "ok",
    "command",
    "file",
    "addr",
    "timeout_ms",
    "reported_bytes",
    "message",
];
pub(crate) const CONTROL_NO_FIELDS: &[&str] = &[];
pub(crate) const CONTROL_STATUS_OPTIONS: &[&str] = &["--json"];
pub(crate) const CONTROL_COMMANDS_OPTIONS: &[&str] = &["--json"];
pub(crate) const CONTROL_CLEAR_BUFFER_OPTIONS: &[&str] = &["--json"];
pub(crate) const CONTROL_READ_OPTIONS: &[&str] = &[
    "--timeout",
    "--since <seq|now>",
    "--until-hex",
    "--max-bytes",
    "--fail-on-timeout",
    "--raw-hex",
    "--raw-text",
    "--json",
];
pub(crate) const CONTROL_WRITE_OPTIONS: &[&str] = &["--target", "--timeout", "--hex", "--json"];
pub(crate) const CONTROL_REQUEST_OPTIONS: &[&str] = &[
    "--target",
    "--timeout",
    "--since <seq|now>",
    "--until-hex",
    "--max-bytes",
    "--fail-on-timeout",
    "--raw-hex",
    "--raw-text",
    "--hex",
    "--json",
];
pub(crate) const CONTROL_ACTION_OPTIONS: &[&str] = &["--timeout", "--json"];
pub(crate) const CONTROL_FLASH_OPTIONS: &[&str] = &["--timeout", "--json"];
pub(crate) const CONTROL_NO_OPTIONS: &[&str] = &[];

pub(crate) const CONTROL_COMMAND_SPECS: [ControlCommandSpec; 14] = [
    ControlCommandSpec {
        name: "version",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: &["--json"],
        payload: "none",
        response: "version",
        fields: &["ok", "protocol", "version", "rttio_version", "git_hash"],
        example: "version --json",
    },
    ControlCommandSpec {
        name: "status",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_STATUS_OPTIONS,
        payload: "none",
        response: "status",
        fields: CONTROL_STATUS_FIELDS,
        example: "status --json",
    },
    ControlCommandSpec {
        name: "commands",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_COMMANDS_OPTIONS,
        payload: "none",
        response: "capabilities",
        fields: CONTROL_NO_FIELDS,
        example: "commands --json",
    },
    ControlCommandSpec {
        name: "clear-buffer",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_CLEAR_BUFFER_OPTIONS,
        payload: "none",
        response: "action",
        fields: CONTROL_ACTION_FIELDS,
        example: "clear-buffer --json",
    },
    ControlCommandSpec {
        name: "read",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_READ_SOURCES,
        options: CONTROL_READ_OPTIONS,
        payload: "none",
        response: "raw-read",
        fields: CONTROL_RAW_READ_FIELDS,
        example: "read --timeout 1000 --json",
    },
    ControlCommandSpec {
        name: "follow",
        json: false,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_NO_OPTIONS,
        payload: "none",
        response: "stream",
        fields: CONTROL_NO_FIELDS,
        example: "follow",
    },
    ControlCommandSpec {
        name: "write",
        json: true,
        targets: CONTROL_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_WRITE_OPTIONS,
        payload: "text-after-separator",
        response: "write",
        fields: CONTROL_WRITE_FIELDS,
        example: "write --target serial --json -- AT",
    },
    ControlCommandSpec {
        name: "writeln",
        json: true,
        targets: CONTROL_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_WRITE_OPTIONS,
        payload: "text-after-separator",
        response: "write",
        fields: CONTROL_WRITE_FIELDS,
        example: "writeln --target rtt --json -- help",
    },
    ControlCommandSpec {
        name: "request",
        json: true,
        targets: CONTROL_TARGETS,
        sources: CONTROL_READ_SOURCES,
        options: CONTROL_REQUEST_OPTIONS,
        payload: "text-or-hex-after-separator",
        response: "request-read",
        fields: CONTROL_REQUEST_READ_FIELDS,
        example: "request --target serial --timeout 1000 --json -- AT",
    },
    ControlCommandSpec {
        name: "reset",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_ACTION_OPTIONS,
        payload: "none",
        response: "action",
        fields: CONTROL_ACTION_FIELDS,
        example: "reset --json --timeout 5000",
    },
    ControlCommandSpec {
        name: "reconnect",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_ACTION_OPTIONS,
        payload: "none",
        response: "action",
        fields: CONTROL_ACTION_FIELDS,
        example: "reconnect --json --timeout 5000",
    },
    ControlCommandSpec {
        name: "flash",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_FLASH_OPTIONS,
        payload: "file-and-optional-address",
        response: "flash",
        fields: CONTROL_FLASH_FIELDS,
        example: "flash --json --timeout 120000 \"build/app.hex\" 0x0",
    },
    ControlCommandSpec {
        name: "erase",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_ACTION_OPTIONS,
        payload: "none",
        response: "action",
        fields: CONTROL_ACTION_FIELDS,
        example: "erase --json --timeout 5000",
    },
    ControlCommandSpec {
        name: "quit",
        json: true,
        targets: CONTROL_NO_TARGETS,
        sources: CONTROL_NO_SOURCES,
        options: CONTROL_COMMANDS_OPTIONS,
        payload: "none",
        response: "action",
        fields: CONTROL_ACTION_FIELDS,
        example: "quit --json",
    },
];
use super::*;
use serde::Serialize;
