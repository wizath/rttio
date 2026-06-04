use super::*;
use serde::Serialize;

pub(crate) async fn update_control_runtime_state(
    state: &Arc<Mutex<ControlRuntimeState>>,
    serial_running: bool,
    rtt_running: bool,
    route: Route,
    output_mode: OutputMode,
    timestamp: bool,
    local_echo: bool,
    output_paused: bool,
) {
    let mut state = state.lock().await;
    state.serial_running = serial_running;
    state.rtt_running = rtt_running;
    state.route = route;
    state.output_mode = output_mode;
    state.timestamp = timestamp;
    state.local_echo = local_echo;
    state.output_paused = output_paused;
}

pub(crate) async fn collect_control_output(
    output_rx: &mut broadcast::Receiver<String>,
    timeout: Option<Duration>,
) -> String {
    let Some(timeout) = timeout else {
        return "ERR follow mode is not implemented in this RPC path\n".to_string();
    };

    let deadline = tokio::time::Instant::now() + timeout;
    let mut output = String::new();
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout_at(deadline, output_rx.recv()).await {
            Ok(Ok(chunk)) => output.push_str(&chunk),
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    format!("OK read {} bytes\n{output}", output.len())
}

pub(crate) async fn resolve_control_since(
    since: Option<ControlSince>,
    history: &Arc<Mutex<ControlHistory>>,
    source: ControlSource,
) -> Option<u64> {
    match since {
        Some(ControlSince::Seq(seq)) => Some(seq),
        Some(ControlSince::Now) => Some(history.lock().await.snapshot(source, None).next_seq),
        None => None,
    }
}

pub(crate) async fn collect_control_raw_output(
    raw_rx: &mut broadcast::Receiver<ControlOutput>,
    history: &Arc<Mutex<ControlHistory>>,
    params: ControlRawReadParams,
    json: bool,
) -> String {
    let response = collect_control_raw_read(
        raw_rx,
        history,
        params.source,
        params.timeout,
        params.since,
        params.until_hex.as_deref(),
        params.max_bytes,
        params.raw_hex,
        params.raw_text,
    )
    .await;
    if params.fail_on_timeout && response.timed_out {
        return control_error_response(json, control_timeout_error(params.until_hex.as_deref()));
    }
    if json {
        return serialize_json_line(&response);
    }
    format!(
        "OK read {} bytes cursor_unit {} next_seq {} dropped_before {} complete {} matched_until_hex {} limited {} timed_out {}\n{}\n",
        response.bytes,
        response.cursor_unit,
        response.next_seq,
        response.dropped_before,
        response.complete,
        response.matched_until_hex,
        response.limited,
        response.timed_out,
        response
            .hex
            .as_deref()
            .or(response.text.as_deref())
            .or(response.text_lossy.as_deref())
            .unwrap_or("")
    )
}

pub(crate) async fn collect_control_request_raw_output(
    raw_rx: &mut broadcast::Receiver<ControlOutput>,
    history: &Arc<Mutex<ControlHistory>>,
    meta: ControlRequestRawMeta,
    params: ControlRawReadParams,
) -> String {
    let raw_response = collect_control_raw_read(
        raw_rx,
        history,
        params.source,
        params.timeout,
        params.since,
        params.until_hex.as_deref(),
        params.max_bytes,
        params.raw_hex,
        params.raw_text,
    )
    .await;
    if params.fail_on_timeout && raw_response.timed_out {
        return control_error_response(true, control_timeout_error(params.until_hex.as_deref()));
    }
    let response = ControlRequestRawResponse {
        ok: true,
        command: meta.command,
        target: meta.target.as_ctl_str(),
        actual_targets: meta.actual_targets,
        written_bytes: meta.written_bytes,
        write_timeout_ms: CONTROL_WRITE_ACK_TIMEOUT_MS,
        read_timeout_ms: meta.read_timeout.as_millis() as u64,
        response: raw_response,
    };
    serialize_json_line(&response)
}

pub(crate) fn control_timeout_error(until: Option<&[u8]>) -> &'static str {
    if until.is_some() {
        "read timed out before until-hex match"
    } else {
        "read timed out waiting for data"
    }
}

pub(crate) struct ControlRawReadParams {
    pub(crate) source: ControlSource,
    pub(crate) timeout: Duration,
    pub(crate) since: Option<u64>,
    pub(crate) until_hex: Option<Vec<u8>>,
    pub(crate) max_bytes: Option<usize>,
    pub(crate) fail_on_timeout: bool,
    pub(crate) raw_hex: bool,
    pub(crate) raw_text: bool,
}

#[derive(Clone)]
pub(crate) struct ControlRequestRawMeta {
    pub(crate) command: &'static str,
    pub(crate) target: ControlTarget,
    pub(crate) written_bytes: usize,
    pub(crate) read_timeout: Duration,
    pub(crate) actual_targets: Vec<&'static str>,
}

pub(crate) async fn collect_control_raw_read(
    raw_rx: &mut broadcast::Receiver<ControlOutput>,
    history: &Arc<Mutex<ControlHistory>>,
    source: ControlSource,
    timeout: Duration,
    since: Option<u64>,
    until: Option<&[u8]>,
    max_bytes: Option<usize>,
    raw_hex: bool,
    raw_text: bool,
) -> ControlRawReadResponse {
    let snapshot = history.lock().await.snapshot(source, since);
    let mut next_expected_seq = snapshot.next_seq;
    let mut dropped_before = snapshot.dropped_before;
    let mut complete = control_history_range_complete(since, dropped_before);
    let mut data = Vec::new();
    let mut matched_until_hex = false;
    let mut limited = false;
    if !snapshot.data.is_empty() {
        let (consumed, outcome) =
            append_control_read_bytes(&mut data, &snapshot.data, until, max_bytes);
        next_expected_seq = snapshot.data_seq.saturating_add(consumed as u64);
        matched_until_hex = outcome.matched_until_hex;
        limited = outcome.limited;
    }
    let mut timed_out = false;
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if matched_until_hex || limited {
            break;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            timed_out = true;
            break;
        }
        match tokio::time::timeout_at(deadline, raw_rx.recv()).await {
            Ok(Ok(output)) => {
                if output.seq > next_expected_seq {
                    let snapshot = history
                        .lock()
                        .await
                        .snapshot(source, Some(next_expected_seq));
                    if !control_history_range_complete(
                        Some(next_expected_seq),
                        snapshot.dropped_before,
                    ) {
                        complete = false;
                    }
                    dropped_before = dropped_before.max(snapshot.dropped_before);
                    let (consumed, outcome) =
                        append_control_read_bytes(&mut data, &snapshot.data, until, max_bytes);
                    if consumed > 0 {
                        next_expected_seq = snapshot.data_seq.saturating_add(consumed as u64);
                    }
                    matched_until_hex = outcome.matched_until_hex;
                    limited = outcome.limited;
                    if matched_until_hex || limited || output.seq < next_expected_seq {
                        continue;
                    }
                }
                if !source.matches(output.source) {
                    continue;
                }
                let output_end = output.seq.saturating_add(output.data.len().max(1) as u64);
                if output_end <= next_expected_seq {
                    continue;
                }
                let offset = next_expected_seq.saturating_sub(output.seq) as usize;
                let bytes = output.data.get(offset..).unwrap_or_default();
                let (consumed, outcome) =
                    append_control_read_bytes(&mut data, bytes, until, max_bytes);
                next_expected_seq = next_expected_seq.saturating_add(consumed as u64);
                matched_until_hex = outcome.matched_until_hex;
                limited = outcome.limited;
            }
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                let snapshot = history
                    .lock()
                    .await
                    .snapshot(source, Some(next_expected_seq));
                if !control_history_range_complete(Some(next_expected_seq), snapshot.dropped_before)
                {
                    complete = false;
                }
                dropped_before = dropped_before.max(snapshot.dropped_before);
                let (consumed, outcome) =
                    append_control_read_bytes(&mut data, &snapshot.data, until, max_bytes);
                if consumed > 0 {
                    next_expected_seq = snapshot.data_seq.saturating_add(consumed as u64);
                }
                matched_until_hex = outcome.matched_until_hex;
                limited = outcome.limited;
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => break,
            Err(_) => {
                timed_out = true;
                break;
            }
        }
    }
    let history_next_seq = history.lock().await.snapshot(source, None).next_seq;
    let next_seq = if matched_until_hex || limited {
        next_expected_seq
    } else {
        history_next_seq
    };
    ControlRawReadResponse {
        ok: true,
        source: source.as_ctl_str(),
        bytes: data.len(),
        hex: raw_hex.then(|| encode_hex(&data)),
        text: decode_control_utf8(&data),
        text_lossy: raw_text.then(|| String::from_utf8_lossy(&data).into_owned()),
        cursor_unit: CONTROL_CURSOR_UNIT,
        next_seq,
        dropped_before,
        complete,
        matched_until_hex,
        limited,
        timed_out,
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ControlReadOutcome {
    matched_until_hex: bool,
    limited: bool,
}

pub(crate) fn append_control_read_bytes(
    data: &mut Vec<u8>,
    bytes: &[u8],
    until: Option<&[u8]>,
    max_bytes: Option<usize>,
) -> (usize, ControlReadOutcome) {
    let remaining = max_bytes
        .map(|max_bytes| max_bytes.saturating_sub(data.len()))
        .unwrap_or(usize::MAX);
    if remaining == 0 {
        return (
            0,
            ControlReadOutcome {
                matched_until_hex: false,
                limited: true,
            },
        );
    }
    let bytes = if bytes.len() > remaining {
        &bytes[..remaining]
    } else {
        bytes
    };
    let Some(until) = until.filter(|until| !until.is_empty()) else {
        data.extend_from_slice(bytes);
        return (
            bytes.len(),
            ControlReadOutcome {
                matched_until_hex: false,
                limited: max_bytes.is_some_and(|max_bytes| data.len() >= max_bytes),
            },
        );
    };
    let original_len = data.len();
    data.extend_from_slice(bytes);
    if let Some(end) = find_subslice(data, until).map(|start| start + until.len()) {
        data.truncate(end);
        return (
            end.saturating_sub(original_len),
            ControlReadOutcome {
                matched_until_hex: true,
                limited: false,
            },
        );
    }
    (
        bytes.len(),
        ControlReadOutcome {
            matched_until_hex: false,
            limited: max_bytes.is_some_and(|max_bytes| data.len() >= max_bytes),
        },
    )
}

pub(crate) fn control_history_range_complete(since: Option<u64>, dropped_before: u64) -> bool {
    since.is_none_or(|since| since == 0 || since >= dropped_before)
}

pub(crate) fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(crate) fn serialize_json_line<T: Serialize>(response: &T) -> String {
    serde_json::to_string(response)
        .map(|mut value| {
            value.push('\n');
            value
        })
        .unwrap_or_else(|e| format!("ERR failed to serialize json response: {e}\n"))
}

#[derive(Serialize)]
pub(crate) struct ControlRawReadResponse {
    pub(crate) ok: bool,
    pub(crate) source: &'static str,
    pub(crate) bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hex: Option<String>,
    pub(crate) text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text_lossy: Option<String>,
    pub(crate) cursor_unit: &'static str,
    pub(crate) next_seq: u64,
    pub(crate) dropped_before: u64,
    pub(crate) complete: bool,
    pub(crate) matched_until_hex: bool,
    pub(crate) limited: bool,
    pub(crate) timed_out: bool,
}

#[derive(Serialize)]
pub(crate) struct ControlRequestRawResponse {
    ok: bool,
    command: &'static str,
    target: &'static str,
    actual_targets: Vec<&'static str>,
    written_bytes: usize,
    write_timeout_ms: u64,
    read_timeout_ms: u64,
    response: ControlRawReadResponse,
}
