use super::*;

pub(crate) fn discover_control_socket(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    discover_control_socket_from(std::env::current_dir()?)
}

pub(crate) fn discover_control_socket_from(start: PathBuf) -> Result<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join(DEFAULT_CONTROL_SOCKET);
        if is_unix_socket_path(&candidate).unwrap_or(false) {
            return Ok(candidate);
        }
    }
    Ok(PathBuf::from(DEFAULT_CONTROL_SOCKET))
}

pub(crate) async fn control_client(
    path: &PathBuf,
    command: &str,
    response_timeout: Option<Duration>,
) -> Result<()> {
    control_client_with_output(path, command, response_timeout, |bytes| {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        write_control_client_output(&mut stdout, bytes)
    })
    .await
}

pub(crate) async fn control_client_with_output(
    path: &PathBuf,
    command: &str,
    response_timeout: Option<Duration>,
    mut write_output: impl FnMut(&[u8]) -> io::Result<()>,
) -> Result<()> {
    validate_control_socket_for_client(path)?;
    let mut stream = UnixStream::connect(path)
        .await
        .with_context(|| format!("failed to connect control socket {}", path.display()))?;
    stream.write_all(command.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.shutdown().await?;

    let read_response = async { read_control_client_response(stream, &mut write_output).await };
    match response_timeout {
        Some(timeout) => tokio::time::timeout(timeout, read_response)
            .await
            .with_context(|| {
                format!(
                    "control response timed out after {} ms",
                    timeout.as_millis()
                )
            })?,
        None => read_response.await,
    }
}

pub(crate) async fn read_control_client_response(
    stream: UnixStream,
    write_output: &mut impl FnMut(&[u8]) -> io::Result<()>,
) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    if reader.read_line(&mut first_line).await? == 0 {
        return Err(anyhow!("control socket closed without response"));
    }
    write_output(first_line.as_bytes())?;
    if let Some(error) = control_response_error(&first_line) {
        return Err(anyhow!(error));
    }
    if !control_response_is_success(&first_line) {
        return Err(anyhow!(
            "invalid control response: {}",
            first_line.trim_end()
        ));
    }

    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        write_output(&chunk[..read])?;
    }
    Ok(())
}

pub(crate) fn write_control_client_output(output: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    output.write_all(bytes)?;
    output.flush()
}

pub(crate) fn control_response_error(first_line: &str) -> Option<String> {
    let line = first_line.trim();
    if line == "ERR" || line.starts_with("ERR ") {
        return Some(line.to_string());
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return None;
    };
    if value.get("ok").and_then(|ok| ok.as_bool()) == Some(false) {
        return Some(
            value
                .get("error")
                .and_then(|error| error.as_str())
                .unwrap_or("rttio control command failed")
                .to_string(),
        );
    }
    None
}

pub(crate) fn control_response_is_success(first_line: &str) -> bool {
    let line = first_line.trim();
    if line == "OK" || line.starts_with("OK ") {
        return true;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return false;
    };
    value.get("ok").and_then(|ok| ok.as_bool()) == Some(true)
}

pub(crate) fn ctl_command_response_timeout(command: &CtlCommand) -> Option<Duration> {
    let timeout_ms = match command {
        CtlCommand::Follow => return None,
        CtlCommand::Read { timeout, .. } => {
            timeout.saturating_add(CONTROL_CLIENT_RESPONSE_GRACE_MS)
        }
        CtlCommand::Write { timeout, .. } | CtlCommand::Writeln { timeout, .. } => {
            timeout.saturating_add(CONTROL_CLIENT_RESPONSE_GRACE_MS)
        }
        CtlCommand::Request { timeout, .. } => timeout
            .saturating_add(CONTROL_WRITE_ACK_TIMEOUT_MS)
            .saturating_add(CONTROL_CLIENT_RESPONSE_GRACE_MS),
        CtlCommand::Reset { timeout, .. }
        | CtlCommand::Reconnect { timeout, .. }
        | CtlCommand::Erase { timeout, .. } => {
            timeout.saturating_add(CONTROL_CLIENT_RESPONSE_GRACE_MS)
        }
        CtlCommand::Flash { timeout, .. } => {
            timeout.saturating_add(CONTROL_CLIENT_RESPONSE_GRACE_MS)
        }
        CtlCommand::Version { .. }
        | CtlCommand::Status { .. }
        | CtlCommand::Commands { .. }
        | CtlCommand::ClearBuffer { .. }
        | CtlCommand::Quit { .. } => CONTROL_CLIENT_DEFAULT_RESPONSE_TIMEOUT_MS,
    };
    Some(Duration::from_millis(timeout_ms))
}

pub(crate) fn ctl_command_to_wire(command: &CtlCommand) -> Result<String> {
    validate_ctl_command_timeouts(command)?;
    match command {
        CtlCommand::Version { json } => Ok(if *json {
            "version --json".to_string()
        } else {
            "version".to_string()
        }),
        CtlCommand::Status { json } => Ok(if *json {
            "status --json".to_string()
        } else {
            "status".to_string()
        }),
        CtlCommand::Commands { json } => Ok(if *json {
            "commands --json".to_string()
        } else {
            "commands".to_string()
        }),
        CtlCommand::ClearBuffer { json } => Ok(if *json {
            "clear-buffer --json".to_string()
        } else {
            "clear-buffer".to_string()
        }),
        CtlCommand::Read {
            timeout,
            since,
            until_hex,
            max_bytes,
            fail_on_timeout,
            raw_hex,
            raw_text,
            json,
        } => {
            let mut command = format!("read --timeout {timeout}");
            if let Some(since) = since {
                let since = parse_control_since(since)?;
                let _ = write!(command, " --since {}", since.as_wire());
            }
            if let Some(until_hex) = until_hex {
                validate_control_hex_arg("--until-hex", until_hex)?;
                command.push_str(" --until-hex ");
                command.push_str(until_hex);
            }
            if let Some(max_bytes) = max_bytes {
                validate_control_max_bytes(*max_bytes)?;
                let _ = write!(command, " --max-bytes {max_bytes}");
            }
            if *fail_on_timeout {
                command.push_str(" --fail-on-timeout");
            }
            if *raw_hex {
                command.push_str(" --raw-hex");
            }
            if *raw_text {
                command.push_str(" --raw-text");
            }
            if *json {
                command.push_str(" --json");
            }
            Ok(command)
        }
        CtlCommand::Follow => Ok("follow".to_string()),
        CtlCommand::Write {
            target,
            json,
            hex,
            timeout,
            text,
        } => {
            let payload = join_ctl_words(text)?;
            if *hex {
                validate_control_hex_arg("hex payload", &payload)?;
            }
            Ok(format!(
                "write --target {} --timeout {timeout} {}{}-- {}",
                target.as_ctl_str(),
                if *hex { "--hex " } else { "" },
                if *json { "--json " } else { "" },
                payload
            ))
        }
        CtlCommand::Writeln {
            target,
            json,
            timeout,
            text,
        } => Ok(format!(
            "writeln --target {} --timeout {timeout} {}-- {}",
            target.as_ctl_str(),
            if *json { "--json " } else { "" },
            join_ctl_words(text)?
        )),
        CtlCommand::Request {
            target,
            timeout,
            since,
            until_hex,
            max_bytes,
            fail_on_timeout,
            raw_hex,
            raw_text,
            hex,
            json,
            text,
        } => {
            let mut command = format!(
                "request --target {} --timeout {timeout}",
                target.as_ctl_str()
            );
            if let Some(since) = since {
                let since = parse_control_since(since)?;
                let _ = write!(command, " --since {}", since.as_wire());
            }
            if let Some(until_hex) = until_hex {
                validate_control_hex_arg("--until-hex", until_hex)?;
                command.push_str(" --until-hex ");
                command.push_str(until_hex);
            }
            if let Some(max_bytes) = max_bytes {
                validate_control_max_bytes(*max_bytes)?;
                let _ = write!(command, " --max-bytes {max_bytes}");
            }
            if *fail_on_timeout {
                command.push_str(" --fail-on-timeout");
            }
            if *raw_hex {
                command.push_str(" --raw-hex");
            }
            if *raw_text {
                command.push_str(" --raw-text");
            }
            if *hex {
                command.push_str(" --hex");
            }
            if *json {
                command.push_str(" --json");
            }
            command.push_str(" -- ");
            let payload = join_ctl_words(text)?;
            if *hex {
                validate_control_hex_arg("hex payload", &payload)?;
            }
            command.push_str(&payload);
            Ok(command)
        }
        CtlCommand::Reset { json, timeout } => Ok(if *json {
            format!("reset --json --timeout {timeout}")
        } else {
            format!("reset --timeout {timeout}")
        }),
        CtlCommand::Reconnect { json, timeout } => Ok(if *json {
            format!("reconnect --json --timeout {timeout}")
        } else {
            format!("reconnect --timeout {timeout}")
        }),
        CtlCommand::Flash {
            json,
            file,
            addr,
            timeout,
        } => {
            let file = fs::canonicalize(file)
                .with_context(|| format!("cannot resolve flash path {}", file.display()))?;
            let file = quote_control_path(&file)?;
            Ok(format!(
                "flash {}--timeout {timeout} {file} 0x{addr:08x}",
                if *json { "--json " } else { "" }
            ))
        }
        CtlCommand::Erase { json, timeout } => Ok(if *json {
            format!("erase --json --timeout {timeout}")
        } else {
            format!("erase --timeout {timeout}")
        }),
        CtlCommand::Quit { json } => Ok(if *json {
            "quit --json".to_string()
        } else {
            "quit".to_string()
        }),
    }
}

pub(crate) fn validate_ctl_command_timeouts(command: &CtlCommand) -> Result<()> {
    match command {
        CtlCommand::Read { timeout, .. }
        | CtlCommand::Write { timeout, .. }
        | CtlCommand::Writeln { timeout, .. }
        | CtlCommand::Reset { timeout, .. }
        | CtlCommand::Reconnect { timeout, .. }
        | CtlCommand::Flash { timeout, .. }
        | CtlCommand::Erase { timeout, .. } => validate_control_timeout_ms(*timeout),
        CtlCommand::Request { timeout, .. } => validate_control_timeout_ms(*timeout),
        CtlCommand::Version { .. }
        | CtlCommand::Status { .. }
        | CtlCommand::Commands { .. }
        | CtlCommand::ClearBuffer { .. }
        | CtlCommand::Follow
        | CtlCommand::Quit { .. } => Ok(()),
    }
}

pub(crate) fn quote_control_path(path: &Path) -> Result<String> {
    let path = path.display().to_string();
    if !path.contains('"') {
        return Ok(format!("\"{path}\""));
    }
    if !path.contains('\'') {
        return Ok(format!("'{path}'"));
    }
    Err(anyhow!(
        "cannot quote flash path containing both single and double quotes"
    ))
}

pub(crate) fn join_ctl_words(words: &[String]) -> Result<String> {
    if words.is_empty() {
        return Err(anyhow!("missing command payload"));
    }
    if words.iter().any(|word| word.contains(['\r', '\n'])) {
        return Err(anyhow!(
            "control payload cannot contain CR or LF; use write/request --hex for raw bytes"
        ));
    }
    Ok(words.join(" "))
}

pub(crate) fn validate_control_hex_arg(name: &str, value: &str) -> Result<()> {
    parse_hex_bytes(value)
        .with_context(|| format!("invalid {name}"))
        .map(|_| ())
}
