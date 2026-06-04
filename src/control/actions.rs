use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ControlActionArgs {
    pub(crate) json: bool,
    pub(crate) timeout: Duration,
}

pub(crate) fn parse_control_action_args(
    line: &str,
    command: &str,
    default_timeout_ms: u64,
) -> Option<Result<ControlActionArgs>> {
    let rest = line.strip_prefix(command)?;
    if rest.is_empty() {
        return Some(Ok(ControlActionArgs {
            json: false,
            timeout: Duration::from_millis(default_timeout_ms),
        }));
    }
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(
        parse_control_action_options(rest, default_timeout_ms, true).map(|(args, rest)| {
            if rest.is_empty() {
                args
            } else {
                ControlActionArgs {
                    json: args.json,
                    timeout: args.timeout,
                }
            }
        }),
    )
}

pub(crate) fn parse_control_quit_args(line: &str) -> Option<Result<bool>> {
    let rest = line.strip_prefix("quit")?;
    if rest.is_empty() {
        return Some(Ok(false));
    }
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    if rest == "--json" {
        return Some(Ok(true));
    }
    Some(Err(anyhow!("unknown quit option near {rest:?}")))
}

pub(crate) fn parse_control_flash_action_args(input: &str) -> Result<(ControlActionArgs, &str)> {
    let (args, rest) = parse_control_action_options(input, CONTROL_FLASH_TIMEOUT_MS, false)?;
    if rest.starts_with("--") {
        return Err(anyhow!("unknown flash option near {rest:?}"));
    }
    Ok((args, rest))
}

pub(crate) fn parse_control_action_options(
    input: &str,
    default_timeout_ms: u64,
    require_no_payload: bool,
) -> Result<(ControlActionArgs, &str)> {
    let mut rest = input.trim_start();
    let mut json = false;
    let mut timeout = Duration::from_millis(default_timeout_ms);

    loop {
        if rest.is_empty() {
            break;
        }
        if let Some(after) = rest.strip_prefix("--json ") {
            json = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--json" {
            json = true;
            rest = "";
            break;
        }
        if let Some(after) = rest.strip_prefix("--timeout ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --timeout value"))?;
            timeout = parse_optional_timeout_ms(value, default_timeout_ms)?;
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--timeout" {
            return Err(anyhow!("missing --timeout value"));
        }
        break;
    }
    if require_no_payload && !rest.is_empty() {
        return Err(anyhow!("unknown action option near {rest:?}"));
    }
    Ok((ControlActionArgs { json, timeout }, rest))
}

pub(crate) fn control_json_only_action_wants_json(line: &str, command: &str) -> bool {
    line.strip_prefix(command)
        .is_some_and(control_args_wants_json)
}

pub(crate) fn trim_control_error(response: &str) -> String {
    response
        .trim()
        .strip_prefix("ERR ")
        .unwrap_or_else(|| response.trim())
        .to_string()
}

pub(crate) fn trim_control_ok(response: &str) -> String {
    response
        .trim()
        .strip_prefix("OK ")
        .unwrap_or_else(|| response.trim())
        .to_string()
}

pub(crate) fn parse_jlink_reported_bytes(response: &str) -> Option<usize> {
    let marker = "J-Link reported ";
    let after_marker = response.split_once(marker)?.1;
    let bytes = after_marker.split_whitespace().next()?;
    bytes.parse().ok()
}

pub(crate) fn parse_jlink_reported_result(response: &str) -> Option<String> {
    let marker = "J-Link reported ";
    let after_marker = response.split_once(marker)?.1;
    Some(after_marker.trim().to_string()).filter(|value| !value.is_empty())
}

pub(crate) fn control_error_response(json: bool, error: impl Into<String>) -> String {
    let error = error.into();
    if !json {
        return format!("ERR {error}\n");
    }
    let response = ControlErrorResponse {
        ok: false,
        code: control_error_code(&error),
        error,
    };
    serde_json::to_string(&response)
        .map(|mut value| {
            value.push('\n');
            value
        })
        .unwrap_or_else(|e| format!("ERR failed to serialize json error response: {e}\n"))
}

pub(crate) fn control_error_code(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if error == "unknown command" {
        "unknown_command"
    } else if lower.contains("unknown") {
        "unknown_option"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("does not exist")
        || lower.contains("is not a file")
        || lower.contains("not a supported")
        || lower.contains("cannot resolve flash path")
        || lower.contains("cannot quote flash path")
    {
        "invalid_path"
    } else if error.contains("missing") || error.contains("invalid") || error.contains("usage:") {
        "invalid_argument"
    } else if error.contains("not running")
        || error.contains("no transport is running")
        || error.contains("no selected transport is running")
        || error.contains("transport is reconnecting")
        || error.contains("dropped response")
        || error.contains("dropped write response")
    {
        "not_running"
    } else if error.contains("requires RTT/J-Link")
        || error.contains("requires active RTT/J-Link")
        || error.contains("not available")
    {
        "unavailable"
    } else {
        "command_failed"
    }
}

pub(crate) fn control_action_response(
    command: &'static str,
    json: bool,
    timeout: Option<Duration>,
    response: &str,
) -> String {
    if !json {
        return response.to_string();
    }
    if response.trim_start().starts_with("OK") {
        let response = ControlActionResponse {
            ok: true,
            command,
            timeout_ms: timeout.map(|timeout| timeout.as_millis() as u64),
            reported_result: parse_jlink_reported_result(response),
            message: trim_control_ok(response),
        };
        return serde_json::to_string(&response)
            .map(|mut value| {
                value.push('\n');
                value
            })
            .unwrap_or_else(|e| format!("ERR failed to serialize json response: {e}\n"));
    }
    control_error_response(true, trim_control_error(response))
}

pub(crate) fn control_write_response(
    command: &'static str,
    json: bool,
    target: ControlTarget,
    bytes: usize,
    timeout: Duration,
    response: &str,
) -> String {
    if !json {
        return response.to_string();
    }
    if response.trim_start().starts_with("OK") {
        let response = ControlWriteResponse {
            ok: true,
            command,
            target: target.as_ctl_str(),
            actual_targets: control_write_ack_targets(response, target),
            bytes,
            timeout_ms: timeout.as_millis() as u64,
            message: trim_control_ok(response),
        };
        return serde_json::to_string(&response)
            .map(|mut value| {
                value.push('\n');
                value
            })
            .unwrap_or_else(|e| format!("ERR failed to serialize json response: {e}\n"));
    }
    control_error_response(true, trim_control_error(response))
}

pub(crate) fn control_write_ack_targets(
    response: &str,
    fallback: ControlTarget,
) -> Vec<&'static str> {
    let line = response.trim();
    if line.starts_with("OK serial write ") {
        return vec!["serial"];
    }
    if line.starts_with("OK rtt write ") {
        return vec!["rtt"];
    }
    if let Some(targets) = line
        .strip_prefix("OK write ")
        .and_then(|line| line.split_once(" targets ").map(|(_, targets)| targets))
    {
        return targets
            .split(',')
            .filter_map(|target| match target {
                "serial" => Some("serial"),
                "rtt" => Some("rtt"),
                _ => None,
            })
            .collect();
    }
    fallback_control_targets(fallback)
}

pub(crate) fn fallback_control_targets(target: ControlTarget) -> Vec<&'static str> {
    match target {
        ControlTarget::Current => Vec::new(),
        ControlTarget::Serial => vec!["serial"],
        ControlTarget::Rtt => vec!["rtt"],
    }
}

pub(crate) fn control_flash_response(
    json: bool,
    path: &Path,
    addr: u32,
    timeout: Duration,
    response: &str,
) -> String {
    if !json {
        return response.to_string();
    }
    if response.trim_start().starts_with("OK") {
        let response = ControlFlashResponse {
            ok: true,
            command: "flash",
            file: path.display().to_string(),
            addr,
            timeout_ms: timeout.as_millis() as u64,
            reported_bytes: parse_jlink_reported_bytes(response),
            message: trim_control_ok(response),
        };
        return serde_json::to_string(&response)
            .map(|mut value| {
                value.push('\n');
                value
            })
            .unwrap_or_else(|e| format!("ERR failed to serialize json response: {e}\n"));
    }
    control_error_response(true, trim_control_error(response))
}
