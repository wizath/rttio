use super::*;

pub(crate) struct ControlServerContext {
    pub(crate) path: PathBuf,
    pub(crate) terminal_tx: mpsc::Sender<TerminalEvent>,
    pub(crate) output_tx: broadcast::Sender<String>,
    pub(crate) raw_tx: broadcast::Sender<ControlOutput>,
    pub(crate) history: Arc<Mutex<ControlHistory>>,
    pub(crate) state: Arc<Mutex<ControlRuntimeState>>,
    pub(crate) line_ending: LineEnding,
}

pub(crate) async fn control_server(
    input_tx: mpsc::Sender<InputEvent>,
    context: ControlServerContext,
) {
    let ControlServerContext {
        path,
        terminal_tx,
        output_tx,
        raw_tx,
        history,
        state,
        line_ending,
    } = context;
    if let Err(e) = validate_control_socket_parent(&path) {
        terminal_status(
            &terminal_tx,
            &format!("control socket {} parent check failed: {e}", path.display()),
        )
        .await;
        return;
    }
    if path.exists() {
        match UnixStream::connect(&path).await {
            Ok(_) => {
                terminal_status(
                    &terminal_tx,
                    &format!("control socket already active: {}", path.display()),
                )
                .await;
                return;
            }
            Err(_) => {
                if let Err(e) = remove_stale_control_socket(&path) {
                    terminal_status(
                        &terminal_tx,
                        &format!(
                            "failed to remove stale control socket {}: {e}",
                            path.display()
                        ),
                    )
                    .await;
                    return;
                }
            }
        }
    }

    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(e) => {
            terminal_status(
                &terminal_tx,
                &format!("control socket {} failed: {e}", path.display()),
            )
            .await;
            return;
        }
    };
    let _socket_guard = ControlSocketGuard::new(path.clone());
    if let Err(e) = harden_control_socket(&path) {
        terminal_status(
            &terminal_tx,
            &format!(
                "control socket {} permission setup failed: {e}",
                path.display()
            ),
        )
        .await;
        return;
    }

    let _ = output_tx.send(format!("[rttio] control socket: {}\n", path.display()));
    let client_limit = Arc::new(Semaphore::new(CONTROL_MAX_CLIENTS));
    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                let permit = match Arc::clone(&client_limit).try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        let _ = stream
                            .write_all(b"ERR control client limit reached\n")
                            .await;
                        continue;
                    }
                };
                let input_tx = input_tx.clone();
                let terminal_tx = terminal_tx.clone();
                let output_rx = output_tx.subscribe();
                let raw_rx = raw_tx.subscribe();
                let history = Arc::clone(&history);
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    let _permit = permit;
                    handle_control_client(
                        stream,
                        input_tx,
                        terminal_tx,
                        output_rx,
                        raw_rx,
                        history,
                        state,
                        line_ending,
                    )
                    .await;
                });
            }
            Err(e) => {
                terminal_status(&terminal_tx, &format!("control socket accept failed: {e}")).await;
                break;
            }
        }
    }
}

pub(crate) struct ControlSocketGuard {
    path: PathBuf,
}

impl ControlSocketGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for ControlSocketGuard {
    fn drop(&mut self) {
        if is_unix_socket_path(&self.path).unwrap_or(false) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(crate) fn remove_stale_control_socket(path: &PathBuf) -> Result<()> {
    let meta =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if !meta.file_type().is_socket() {
        return Err(anyhow!(
            "refusing to remove non-socket path {}",
            path.display()
        ));
    }
    fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
}

pub(crate) fn validate_control_socket_parent(path: &Path) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let meta = fs::symlink_metadata(parent).with_context(|| {
        format!(
            "failed to stat control socket directory {}",
            parent.display()
        )
    })?;
    if !meta.file_type().is_dir() {
        return Err(anyhow!(
            "control socket parent {} is not a directory",
            parent.display()
        ));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o022 != 0 {
        return Err(anyhow!(
            "control socket parent {} is writable by group or others (mode {mode:o})",
            parent.display()
        ));
    }
    Ok(())
}

pub(crate) fn harden_control_socket(path: &PathBuf) -> Result<()> {
    if !is_unix_socket_path(path)? {
        return Err(anyhow!("{} is not a Unix socket", path.display()));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("failed to chmod 0600 {}", path.display()))?;
    let mode = fs::symlink_metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions()
        .mode()
        & 0o777;
    if mode != 0o600 {
        return Err(anyhow!(
            "control socket {} permissions are {mode:o}, expected 600",
            path.display()
        ));
    }
    Ok(())
}

pub(crate) fn is_unix_socket_path(path: &PathBuf) -> Result<bool> {
    let meta =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    Ok(meta.file_type().is_socket())
}

pub(crate) fn validate_control_socket_for_client(path: &PathBuf) -> Result<()> {
    validate_control_socket_parent(path)?;
    let meta =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if !meta.file_type().is_socket() {
        return Err(anyhow!("{} is not a Unix socket", path.display()));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode != 0o600 {
        return Err(anyhow!(
            "control socket {} has insecure permissions (mode {mode:o}); expected 600",
            path.display()
        ));
    }
    Ok(())
}

pub(crate) async fn handle_control_client(
    stream: UnixStream,
    input_tx: mpsc::Sender<InputEvent>,
    terminal_tx: mpsc::Sender<TerminalEvent>,
    mut output_rx: broadcast::Receiver<String>,
    mut raw_rx: broadcast::Receiver<ControlOutput>,
    history: Arc<Mutex<ControlHistory>>,
    state: Arc<Mutex<ControlRuntimeState>>,
    line_ending: LineEnding,
) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    loop {
        match tokio::time::timeout(
            Duration::from_millis(CONTROL_CLIENT_IDLE_TIMEOUT_MS),
            read_control_command_line(&mut reader),
        )
        .await
        {
            Ok(Ok(Some(line))) => {
                if line.trim() == "follow" {
                    if writer.write_all(b"OK follow\n").await.is_err() {
                        break;
                    }
                    loop {
                        match output_rx.recv().await {
                            Ok(output) => {
                                if writer.write_all(output.as_bytes()).await.is_err() {
                                    return;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(broadcast::error::RecvError::Closed) => return,
                        }
                    }
                }
                let response = handle_control_line_inner(
                    &line,
                    &input_tx,
                    Some(&terminal_tx),
                    &mut output_rx,
                    &mut raw_rx,
                    &history,
                    &state,
                    line_ending,
                )
                .await;
                if writer.write_all(response.as_bytes()).await.is_err() {
                    break;
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => {
                let _ = writer.write_all(format!("ERR {e}\n").as_bytes()).await;
                break;
            }
            Err(_) => {
                let _ = writer.write_all(b"ERR control client idle timeout\n").await;
                break;
            }
        }
    }
}

pub(crate) async fn read_control_command_line<R>(reader: &mut R) -> Result<Option<String>>
where
    R: AsyncBufRead + Unpin,
{
    let mut bytes = Vec::new();

    loop {
        let (take, found_line) = {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                if bytes.is_empty() {
                    return Ok(None);
                }
                break;
            }
            let line_end = available.iter().position(|byte| *byte == b'\n');
            let take = line_end.map_or(available.len(), |position| position + 1);
            if bytes.len() + take > CONTROL_MAX_COMMAND_BYTES {
                return Err(anyhow!(
                    "control command exceeds {CONTROL_MAX_COMMAND_BYTES} bytes"
                ));
            }
            bytes.extend_from_slice(&available[..take]);
            (take, line_end.is_some())
        };
        reader.consume(take);
        if found_line {
            break;
        }
    }

    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }

    String::from_utf8(bytes)
        .context("control command is not utf-8")
        .map(Some)
}

#[cfg(test)]
pub(crate) async fn handle_control_line(
    line: &str,
    input_tx: &mpsc::Sender<InputEvent>,
    output_rx: &mut broadcast::Receiver<String>,
    raw_rx: &mut broadcast::Receiver<ControlOutput>,
    history: &Arc<Mutex<ControlHistory>>,
    state: &Arc<Mutex<ControlRuntimeState>>,
    line_ending: LineEnding,
) -> String {
    handle_control_line_inner(
        line,
        input_tx,
        None,
        output_rx,
        raw_rx,
        history,
        state,
        line_ending,
    )
    .await
}

async fn handle_control_line_inner(
    line: &str,
    input_tx: &mpsc::Sender<InputEvent>,
    terminal_tx: Option<&mpsc::Sender<TerminalEvent>>,
    output_rx: &mut broadcast::Receiver<String>,
    raw_rx: &mut broadcast::Receiver<ControlOutput>,
    history: &Arc<Mutex<ControlHistory>>,
    state: &Arc<Mutex<ControlRuntimeState>>,
    line_ending: LineEnding,
) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return "OK\n".to_string();
    }
    if trimmed == "version" {
        return collect_control_version_text();
    }
    if trimmed == "version --json" {
        return collect_control_version_json();
    }
    if let Some(rest) = trimmed.strip_prefix("version ") {
        return control_error_response(
            control_args_wants_json(rest),
            format!("unknown version option near {:?}", rest.trim()),
        );
    }
    if trimmed == "status" {
        return collect_control_status_text(history, state).await;
    }
    if trimmed == "status --json" {
        return collect_control_status_json(history, state).await;
    }
    if let Some(rest) = trimmed.strip_prefix("status ") {
        return control_error_response(
            control_args_wants_json(rest),
            format!("unknown status option near {:?}", rest.trim()),
        );
    }
    if trimmed == "commands" {
        return format!("OK {CONTROL_COMMANDS_HELP}\n");
    }
    if trimmed == "commands --json" {
        return collect_control_commands_json();
    }
    if let Some(rest) = trimmed.strip_prefix("commands ") {
        return control_error_response(
            control_args_wants_json(rest),
            format!("unknown commands option near {:?}", rest.trim()),
        );
    }
    if trimmed == "clear-buffer" || trimmed == "clear-buffer --json" {
        let json = trimmed.ends_with("--json");
        let cleared = history.lock().await.clear();
        if let Some(terminal_tx) = terminal_tx {
            refresh_status_bar_after_control_clear(terminal_tx, state).await;
        }
        let message = format!("cleared {cleared} bytes");
        return if json {
            serialize_json_line(&ControlActionResponse {
                ok: true,
                command: "clear-buffer",
                timeout_ms: None,
                reported_result: Some(cleared.to_string()),
                message,
            })
        } else {
            format!("OK clear-buffer {cleared} bytes\n")
        };
    }
    if let Some(rest) = trimmed.strip_prefix("clear-buffer ") {
        return control_error_response(
            control_args_wants_json(rest),
            format!("unknown clear-buffer option near {:?}", rest.trim()),
        );
    }
    if trimmed == "follow" {
        return collect_control_output(output_rx, None).await;
    }
    if let Some(rest) = control_command_rest(trimmed, "read") {
        let wants_json = control_args_wants_json(rest);
        let opts = match parse_control_read_args(rest.trim(), Duration::from_millis(200)) {
            Ok(opts) => opts,
            Err(e) => return control_error_response(wants_json, e.to_string()),
        };
        let source = current_control_source(state).await;
        return collect_control_raw_output(
            raw_rx,
            history,
            ControlRawReadParams {
                source,
                timeout: opts.timeout,
                since: resolve_control_since(opts.since, history, source).await,
                until_hex: opts.until_hex,
                max_bytes: opts.max_bytes,
                fail_on_timeout: opts.fail_on_timeout,
                raw_hex: opts.raw_hex,
                raw_text: opts.raw_text,
            },
            opts.json,
        )
        .await;
    }
    if let Some(args) = parse_control_action_args(trimmed, "reset", CONTROL_ACTION_TIMEOUT_MS) {
        let args = match args {
            Ok(args) => args,
            Err(e) => {
                return control_error_response(
                    control_json_only_action_wants_json(trimmed, "reset"),
                    e.to_string(),
                );
            }
        };
        if let Err(e) = require_active_rtt_target(state).await {
            return control_error_response(args.json, e.to_string());
        }
        let response = send_control_request(input_tx, args.timeout, |reply| {
            ControlRequest::Reset { reply }
        })
        .await;
        return control_action_response("reset", args.json, Some(args.timeout), &response);
    }
    if let Some(args) = parse_control_action_args(trimmed, "reconnect", CONTROL_ACTION_TIMEOUT_MS) {
        let args = match args {
            Ok(args) => args,
            Err(e) => {
                return control_error_response(
                    control_json_only_action_wants_json(trimmed, "reconnect"),
                    e.to_string(),
                );
            }
        };
        let response = send_control_request(input_tx, args.timeout, |reply| {
            ControlRequest::Reconnect { reply }
        })
        .await;
        return control_action_response("reconnect", args.json, Some(args.timeout), &response);
    }
    if let Some(args) = parse_control_quit_args(trimmed) {
        let args = match args {
            Ok(args) => args,
            Err(e) => {
                return control_error_response(
                    trimmed
                        .strip_prefix("quit")
                        .is_some_and(control_args_wants_json),
                    e.to_string(),
                );
            }
        };
        let _ = input_tx.send(InputEvent::Quit).await;
        return control_action_response("quit", args, None, "OK quit\n");
    }
    if let Some(rest) = control_command_rest(trimmed, "request") {
        let wants_json = control_args_wants_json(rest);
        let args = match parse_control_request_args(rest) {
            Ok(args) => args,
            Err(e) => return control_error_response(wants_json, e.to_string()),
        };
        let bytes = if args.hex {
            match parse_hex_bytes(&args.payload) {
                Ok(bytes) => bytes,
                Err(e) => return control_error_response(args.json, e.to_string()),
            }
        } else {
            let mut bytes = args.payload.into_bytes();
            bytes.extend_from_slice(line_ending.bytes());
            bytes
        };
        let written_bytes = bytes.len();
        let source = current_control_source(state).await;
        let since = match args.since {
            Some(ControlSince::Seq(since)) => Some(since),
            Some(ControlSince::Now) => Some(history.lock().await.snapshot(source, None).next_seq),
            None => Some(history.lock().await.snapshot(source, None).next_seq),
        };
        let response = send_control_request(
            input_tx,
            Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS),
            |reply| ControlRequest::Write {
                target: args.target,
                bytes,
                timeout: Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS),
                reply,
            },
        )
        .await;
        if !response.starts_with("OK") {
            return control_error_response(args.json, trim_control_error(&response));
        }
        let actual_targets = control_write_ack_targets(&response, args.target);
        return if args.json {
            collect_control_request_raw_output(
                raw_rx,
                history,
                ControlRequestRawMeta {
                    command: "request",
                    target: args.target,
                    written_bytes,
                    read_timeout: args.timeout,
                    actual_targets,
                },
                ControlRawReadParams {
                    source,
                    timeout: args.timeout,
                    since,
                    until_hex: args.until_hex,
                    max_bytes: args.max_bytes,
                    fail_on_timeout: args.fail_on_timeout,
                    raw_hex: args.raw_hex,
                    raw_text: args.raw_text,
                },
            )
            .await
        } else {
            collect_control_raw_output(
                raw_rx,
                history,
                ControlRawReadParams {
                    source,
                    timeout: args.timeout,
                    since,
                    until_hex: args.until_hex,
                    max_bytes: args.max_bytes,
                    fail_on_timeout: args.fail_on_timeout,
                    raw_hex: args.raw_hex,
                    raw_text: args.raw_text,
                },
                false,
            )
            .await
        };
    }
    if let Some(text) = control_command_rest(trimmed, "write") {
        let args = match parse_control_write_args(text) {
            Ok(args) => args,
            Err(e) => return control_error_response(control_args_wants_json(text), e.to_string()),
        };
        let bytes = if args.hex {
            match parse_hex_bytes(args.payload) {
                Ok(bytes) => bytes,
                Err(e) => return control_error_response(args.json, e.to_string()),
            }
        } else {
            args.payload.as_bytes().to_vec()
        };
        let bytes_len = bytes.len();
        let response =
            send_control_request(input_tx, args.timeout, |reply| ControlRequest::Write {
                target: args.target,
                bytes,
                timeout: args.timeout,
                reply,
            })
            .await;
        return control_write_response(
            "write",
            args.json,
            args.target,
            bytes_len,
            args.timeout,
            &response,
        );
    }
    if let Some(text) = control_command_rest(trimmed, "writeln") {
        let args = match parse_control_write_args(text) {
            Ok(args) => args,
            Err(e) => return control_error_response(control_args_wants_json(text), e.to_string()),
        };
        let mut bytes = args.payload.as_bytes().to_vec();
        bytes.extend_from_slice(line_ending.bytes());
        let bytes_len = bytes.len();
        let response =
            send_control_request(input_tx, args.timeout, |reply| ControlRequest::Write {
                target: args.target,
                bytes,
                timeout: args.timeout,
                reply,
            })
            .await;
        return control_write_response(
            "writeln",
            args.json,
            args.target,
            bytes_len,
            args.timeout,
            &response,
        );
    }
    if let Some(rest) = control_command_rest(trimmed, "flash") {
        let (action_args, rest) = match parse_control_flash_action_args(rest) {
            Ok(args) => args,
            Err(e) => {
                return control_error_response(control_args_wants_json(rest), e.to_string());
            }
        };
        let (path, addr_text) = match parse_control_flash_args(rest) {
            Ok(args) => args,
            Err(e) => return control_error_response(action_args.json, e.to_string()),
        };
        if let Err(e) = validate_flash_file(&path) {
            return control_error_response(action_args.json, e.to_string());
        }
        if let Err(e) = require_active_rtt_target(state).await {
            return control_error_response(action_args.json, e.to_string());
        }
        let addr = match addr_text.as_deref() {
            Some(value) => match parse_u32(value) {
                Ok(addr) => addr,
                Err(e) => {
                    return control_error_response(
                        action_args.json,
                        format!("invalid address: {e}"),
                    );
                }
            },
            None => 0,
        };
        let response = send_control_request(input_tx, action_args.timeout, |reply| {
            ControlRequest::Flash {
                path: path.clone(),
                addr,
                reply,
            }
        })
        .await;
        return control_flash_response(
            action_args.json,
            &path,
            addr,
            action_args.timeout,
            &response,
        );
    }
    if let Some(args) = parse_control_action_args(trimmed, "erase", CONTROL_ACTION_TIMEOUT_MS) {
        let args = match args {
            Ok(args) => args,
            Err(e) => {
                return control_error_response(
                    control_json_only_action_wants_json(trimmed, "erase"),
                    e.to_string(),
                );
            }
        };
        if let Err(e) = require_active_rtt_target(state).await {
            return control_error_response(args.json, e.to_string());
        }
        let response = send_control_request(input_tx, args.timeout, |reply| {
            ControlRequest::Erase { reply }
        })
        .await;
        return control_action_response("erase", args.json, Some(args.timeout), &response);
    }

    control_error_response(control_args_wants_json(trimmed), "unknown command")
}

async fn refresh_status_bar_after_control_clear(
    terminal_tx: &mpsc::Sender<TerminalEvent>,
    state: &Arc<Mutex<ControlRuntimeState>>,
) {
    let status_bar = {
        let state = state.lock().await;
        TerminalStatusBar {
            target: match state.route {
                Route::Serial => "serial",
                Route::Rtt => "rtt",
                Route::Both => "both",
            },
            serial_running: state.serial_running,
            rtt_running: state.rtt_running,
            output_mode: state.output_mode,
            timestamp: state.timestamp,
            local_echo: state.local_echo,
            output_paused: state.output_paused,
            history_bytes: 0,
            history_max_bytes: CONTROL_HISTORY_MAX_BYTES,
        }
    };
    let _ = terminal_tx
        .send(TerminalEvent::SetStatusBar(status_bar))
        .await;
}

async fn current_control_source(state: &Arc<Mutex<ControlRuntimeState>>) -> ControlSource {
    match state.lock().await.route {
        Route::Serial => ControlSource::Serial,
        Route::Rtt => ControlSource::Rtt,
        Route::Both => ControlSource::Any,
    }
}

async fn require_active_rtt_target(state: &Arc<Mutex<ControlRuntimeState>>) -> Result<()> {
    let state = state.lock().await;
    if matches!(state.route, Route::Rtt | Route::Both) && state.rtt_configured {
        Ok(())
    } else {
        Err(anyhow!("requires active RTT/J-Link target"))
    }
}

pub(crate) fn control_command_rest<'a>(line: &'a str, command: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(command)?;
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix(char::is_whitespace).map(str::trim_start)
}

pub(crate) async fn send_control_request(
    input_tx: &mpsc::Sender<InputEvent>,
    timeout: Duration,
    build: impl FnOnce(ControlReply) -> ControlRequest,
) -> String {
    let (reply, rx) = tokio::sync::oneshot::channel();
    if input_tx
        .send(InputEvent::Control(build(reply)))
        .await
        .is_err()
    {
        return "ERR rttio command loop is not running\n".to_string();
    }
    match tokio::time::timeout(timeout, rx).await {
        Ok(response) => {
            response.unwrap_or_else(|_| "ERR rttio command loop dropped response\n".to_string())
        }
        Err(_) => format!(
            "ERR rttio command timed out after {} ms\n",
            timeout.as_millis()
        ),
    }
}

pub(crate) fn control_args_wants_json(input: &str) -> bool {
    input.split_whitespace().any(|part| part == "--json")
}
