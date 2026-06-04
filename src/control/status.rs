use super::*;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct ControlStatusResponse<'a> {
    ok: bool,
    protocol: &'static str,
    version: u32,
    rttio_version: &'static str,
    git_hash: &'static str,
    pid: u32,
    cwd: Option<String>,
    control_socket: String,
    cursor_unit: &'static str,
    serial_configured: bool,
    rtt_configured: bool,
    serial_path: Option<String>,
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
    route: &'a str,
    output_mode: &'a str,
    timestamp: bool,
    local_echo: bool,
    output_paused: bool,
    line_ending: &'a str,
    history_max_bytes: usize,
    next_seq: u64,
    dropped_before: u64,
    serial_next_seq: u64,
    serial_dropped_before: u64,
    rtt_next_seq: u64,
    rtt_dropped_before: u64,
}

#[derive(Serialize)]
pub(crate) struct ControlVersionResponse {
    ok: bool,
    protocol: &'static str,
    version: u32,
    rttio_version: &'static str,
    git_hash: &'static str,
}

pub(crate) fn collect_control_version_json() -> String {
    serialize_json_line(&ControlVersionResponse {
        ok: true,
        protocol: "rttio-control",
        version: CONTROL_PROTOCOL_VERSION,
        rttio_version: RTTIO_VERSION,
        git_hash: RTTIO_GIT_HASH,
    })
}

pub(crate) fn collect_control_version_text() -> String {
    format!(
        "OK version rttio {} git {} protocol {}\n",
        RTTIO_VERSION, RTTIO_GIT_HASH, CONTROL_PROTOCOL_VERSION
    )
}

pub(crate) async fn collect_control_status_json(
    history: &Arc<Mutex<ControlHistory>>,
    state: &Arc<Mutex<ControlRuntimeState>>,
) -> String {
    let response = collect_control_status(history, state).await;
    serialize_json_line(&response)
}

pub(crate) async fn collect_control_status_text(
    history: &Arc<Mutex<ControlHistory>>,
    state: &Arc<Mutex<ControlRuntimeState>>,
) -> String {
    let response = collect_control_status(history, state).await;
    format!(
        "OK status protocol {} version {} rttio_version {} git_hash {} pid {} cwd {} control_socket {} cursor_unit {} serial_configured {} rtt_configured {} serial_running {} rtt_running {} route {} output_mode {} timestamp {} local_echo {} output_paused {} line_ending {} history_max_bytes {} next_seq {} serial_next_seq {} rtt_next_seq {}\n",
        response.protocol,
        response.version,
        response.rttio_version,
        response.git_hash,
        response.pid,
        response.cwd.as_deref().unwrap_or("-"),
        response.control_socket,
        response.cursor_unit,
        response.serial_configured,
        response.rtt_configured,
        response.serial_running,
        response.rtt_running,
        response.route,
        response.output_mode,
        response.timestamp,
        response.local_echo,
        response.output_paused,
        response.line_ending,
        response.history_max_bytes,
        response.next_seq,
        response.serial_next_seq,
        response.rtt_next_seq
    )
}

pub(crate) async fn collect_control_status(
    history: &Arc<Mutex<ControlHistory>>,
    state: &Arc<Mutex<ControlRuntimeState>>,
) -> ControlStatusResponse<'static> {
    let (history_any, history_serial, history_rtt) = {
        let history = history.lock().await;
        (
            history.snapshot(ControlSource::Any, None),
            history.snapshot(ControlSource::Serial, None),
            history.snapshot(ControlSource::Rtt, None),
        )
    };
    let state = state.lock().await.clone();
    let response = ControlStatusResponse {
        ok: true,
        protocol: "rttio-control",
        version: CONTROL_PROTOCOL_VERSION,
        rttio_version: RTTIO_VERSION,
        git_hash: RTTIO_GIT_HASH,
        pid: std::process::id(),
        cwd: std::env::current_dir()
            .ok()
            .map(|path| path.display().to_string()),
        control_socket: state.control_socket.display().to_string(),
        cursor_unit: CONTROL_CURSOR_UNIT,
        serial_configured: state.serial_configured,
        rtt_configured: state.rtt_configured,
        serial_path: state
            .serial_path
            .as_ref()
            .map(|path| path.display().to_string()),
        baud: state.baud,
        jlink_sn: state.jlink_sn,
        jlink_ip: state.jlink_ip,
        device: state.device,
        rtt_tcp_host: state.rtt_tcp_host,
        rtt_tcp_port: state.rtt_tcp_port,
        rtt_up: state.rtt_up,
        rtt_down: state.rtt_down,
        serial_running: state.serial_running,
        rtt_running: state.rtt_running,
        route: state.route.as_ctl_str(),
        output_mode: state.output_mode.as_ctl_str(),
        timestamp: state.timestamp,
        local_echo: state.local_echo,
        output_paused: state.output_paused,
        line_ending: state.line_ending.as_ctl_str(),
        history_max_bytes: CONTROL_HISTORY_MAX_BYTES,
        next_seq: history_any.next_seq,
        dropped_before: history_any.dropped_before,
        serial_next_seq: history_serial.next_seq,
        serial_dropped_before: history_serial.dropped_before,
        rtt_next_seq: history_rtt.next_seq,
        rtt_dropped_before: history_rtt.dropped_before,
    };
    response
}
