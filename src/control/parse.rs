use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ControlReadOptions {
    pub(crate) timeout: Duration,
    pub(crate) since: Option<ControlSince>,
    pub(crate) until_hex: Option<Vec<u8>>,
    pub(crate) max_bytes: Option<usize>,
    pub(crate) fail_on_timeout: bool,
    pub(crate) raw_hex: bool,
    pub(crate) raw_text: bool,
    pub(crate) json: bool,
}

pub(crate) fn parse_control_read_args(
    input: &str,
    default_timeout: Duration,
) -> Result<ControlReadOptions> {
    let mut rest = input.trim_start();
    let mut timeout = default_timeout;
    let mut since = None;
    let mut until_hex = None;
    let mut max_bytes = None;
    let mut fail_on_timeout = false;
    let mut raw_hex = false;
    let mut raw_text = false;
    let mut json = false;

    loop {
        if rest.is_empty() {
            break;
        }
        if let Some(after) = rest.strip_prefix("--timeout ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --timeout value"))?;
            timeout = parse_optional_timeout_ms(value, default_timeout.as_millis() as u64)?;
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--timeout" {
            return Err(anyhow!("missing --timeout value"));
        }
        if let Some(after) = rest.strip_prefix("--since ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --since value"))?;
            since = Some(parse_control_since(value)?);
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--since" {
            return Err(anyhow!("missing --since value"));
        }
        if let Some(after) = rest.strip_prefix("--until-hex ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --until-hex value"))?;
            until_hex = Some(parse_hex_bytes(value)?);
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--until-hex" {
            return Err(anyhow!("missing --until-hex value"));
        }
        if let Some(after) = rest.strip_prefix("--max-bytes ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --max-bytes value"))?;
            max_bytes = Some(parse_control_max_bytes(value)?);
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--max-bytes" {
            return Err(anyhow!("missing --max-bytes value"));
        }
        if let Some(after) = rest.strip_prefix("--fail-on-timeout ") {
            fail_on_timeout = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--fail-on-timeout" {
            fail_on_timeout = true;
            break;
        }
        if let Some(after) = rest.strip_prefix("--raw-hex ") {
            raw_hex = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--raw-hex" {
            raw_hex = true;
            break;
        }
        if let Some(after) = rest.strip_prefix("--raw-text ") {
            raw_text = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--raw-text" {
            raw_text = true;
            break;
        }
        if let Some(after) = rest.strip_prefix("--json ") {
            json = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--json" {
            json = true;
            break;
        }
        return Err(anyhow!("unknown read option near {rest:?}"));
    }
    Ok(ControlReadOptions {
        timeout,
        since,
        until_hex,
        max_bytes,
        fail_on_timeout,
        raw_hex,
        raw_text,
        json,
    })
}

pub(crate) fn parse_control_since(input: &str) -> Result<ControlSince> {
    if input == "now" {
        return Ok(ControlSince::Now);
    }
    input
        .parse::<u64>()
        .map(ControlSince::Seq)
        .with_context(|| format!("invalid since {input:?}; expected byte cursor or now"))
}

impl ControlSource {
    pub(crate) fn as_ctl_str(self) -> &'static str {
        match self {
            ControlSource::Any => "any",
            ControlSource::Serial => "serial",
            ControlSource::Rtt => "rtt",
        }
    }
}

pub(crate) fn parse_optional_timeout_ms(input: &str, default_ms: u64) -> Result<Duration> {
    if input.is_empty() {
        return Ok(Duration::from_millis(default_ms));
    }
    let timeout_ms = input
        .parse::<u64>()
        .with_context(|| format!("invalid timeout {input:?}"))?;
    validate_control_timeout_ms(timeout_ms)?;
    Ok(Duration::from_millis(timeout_ms))
}

pub(crate) fn validate_control_timeout_ms(timeout_ms: u64) -> Result<()> {
    if timeout_ms > CONTROL_MAX_TIMEOUT_MS {
        return Err(anyhow!(
            "timeout {timeout_ms} ms exceeds maximum {CONTROL_MAX_TIMEOUT_MS} ms"
        ));
    }
    Ok(())
}

pub(crate) fn parse_control_max_bytes(input: &str) -> Result<usize> {
    let max_bytes = input
        .parse::<usize>()
        .with_context(|| format!("invalid max-bytes {input:?}"))?;
    validate_control_max_bytes(max_bytes)?;
    Ok(max_bytes)
}

pub(crate) fn validate_control_max_bytes(max_bytes: usize) -> Result<()> {
    if max_bytes == 0 {
        return Err(anyhow!("max-bytes must be greater than zero"));
    }
    if max_bytes > CONTROL_HISTORY_MAX_BYTES {
        return Err(anyhow!(
            "max-bytes {max_bytes} exceeds maximum {CONTROL_HISTORY_MAX_BYTES}"
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ControlWriteArgs<'a> {
    pub(crate) target: ControlTarget,
    pub(crate) json: bool,
    pub(crate) hex: bool,
    pub(crate) timeout: Duration,
    pub(crate) payload: &'a str,
}

pub(crate) fn parse_control_write_args(input: &str) -> Result<ControlWriteArgs<'_>> {
    let mut rest = input.trim_start();
    let mut target = ControlTarget::Current;
    let mut json = false;
    let mut hex = false;
    let mut timeout = Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS);

    loop {
        if rest.is_empty() {
            return Err(anyhow!("missing payload"));
        }
        if let Some(after) = rest.strip_prefix("--target ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --target value"))?;
            target = parse_control_target(value)?;
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--target" {
            return Err(anyhow!("missing --target value"));
        }
        if let Some(after) = rest.strip_prefix("--timeout ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --timeout value"))?;
            timeout = parse_optional_timeout_ms(value, CONTROL_WRITE_ACK_TIMEOUT_MS)?;
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--timeout" {
            return Err(anyhow!("missing --timeout value"));
        }
        if let Some(after) = rest.strip_prefix("--hex ") {
            hex = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--hex" {
            return Err(anyhow!("missing payload"));
        }
        if let Some(after) = rest.strip_prefix("--json ") {
            json = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--json" {
            return Err(anyhow!("missing payload"));
        }
        if let Some(after) = rest.strip_prefix("-- ") {
            let payload = after.trim_start();
            if payload.is_empty() {
                return Err(anyhow!("missing payload"));
            }
            return Ok(ControlWriteArgs {
                target,
                json,
                hex,
                timeout,
                payload,
            });
        }
        if rest == "--" {
            return Err(anyhow!("missing payload"));
        }
        if rest.starts_with("--") {
            return Err(anyhow!(
                "payload starting with '-' must follow the -- separator"
            ));
        }
        return Ok(ControlWriteArgs {
            target,
            json,
            hex,
            timeout,
            payload: rest,
        });
    }
}

pub(crate) fn parse_control_target(input: &str) -> Result<ControlTarget> {
    match input {
        "current" => Ok(ControlTarget::Current),
        "serial" => Ok(ControlTarget::Serial),
        "rtt" => Ok(ControlTarget::Rtt),
        _ => Err(anyhow!(
            "invalid target {input:?}; expected current, serial, or rtt"
        )),
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ControlRequestArgs {
    pub(crate) target: ControlTarget,
    pub(crate) timeout: Duration,
    pub(crate) since: Option<ControlSince>,
    pub(crate) until_hex: Option<Vec<u8>>,
    pub(crate) max_bytes: Option<usize>,
    pub(crate) fail_on_timeout: bool,
    pub(crate) raw_hex: bool,
    pub(crate) raw_text: bool,
    pub(crate) hex: bool,
    pub(crate) payload: String,
    pub(crate) json: bool,
}

pub(crate) fn parse_control_request_args(input: &str) -> Result<ControlRequestArgs> {
    let mut rest = input.trim_start();
    let mut target = ControlTarget::Current;
    let mut timeout = Duration::from_millis(500);
    let mut since = None;
    let mut until_hex = None;
    let mut max_bytes = None;
    let mut fail_on_timeout = false;
    let mut raw_hex = false;
    let mut raw_text = false;
    let mut hex = false;
    let mut json = false;
    let mut payload_after_separator = false;

    loop {
        if let Some(after) = rest.strip_prefix("--target ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --target value"))?;
            target = parse_control_target(value)?;
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--target" {
            return Err(anyhow!("missing --target value"));
        }
        if let Some(after) = rest.strip_prefix("--timeout ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --timeout value"))?;
            timeout = parse_optional_timeout_ms(value, 500)?;
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--timeout" {
            return Err(anyhow!("missing --timeout value"));
        }
        if let Some(after) = rest.strip_prefix("--since ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --since value"))?;
            since = Some(parse_control_since(value)?);
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--since" {
            return Err(anyhow!("missing --since value"));
        }
        if let Some(after) = rest.strip_prefix("--until-hex ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --until-hex value"))?;
            until_hex = Some(parse_hex_bytes(value)?);
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--until-hex" {
            return Err(anyhow!("missing --until-hex value"));
        }
        if let Some(after) = rest.strip_prefix("--max-bytes ") {
            let mut parts = after.trim_start().splitn(2, char::is_whitespace);
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("missing --max-bytes value"))?;
            max_bytes = Some(parse_control_max_bytes(value)?);
            rest = parts.next().unwrap_or_default().trim_start();
            continue;
        }
        if rest == "--max-bytes" {
            return Err(anyhow!("missing --max-bytes value"));
        }
        if let Some(after) = rest.strip_prefix("--fail-on-timeout ") {
            fail_on_timeout = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--fail-on-timeout" {
            return Err(anyhow!("missing request payload"));
        }
        if let Some(after) = rest.strip_prefix("--raw-hex ") {
            raw_hex = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--raw-hex" {
            return Err(anyhow!("missing request payload"));
        }
        if let Some(after) = rest.strip_prefix("--raw-text ") {
            raw_text = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--raw-text" {
            return Err(anyhow!("missing request payload"));
        }
        if let Some(after) = rest.strip_prefix("--hex ") {
            hex = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--hex" {
            return Err(anyhow!("missing request payload"));
        }
        if let Some(after) = rest.strip_prefix("--json ") {
            json = true;
            rest = after.trim_start();
            continue;
        }
        if rest == "--json" {
            return Err(anyhow!("missing request payload"));
        }
        if let Some(after) = rest.strip_prefix("-- ") {
            rest = after.trim_start();
            payload_after_separator = true;
            break;
        }
        if rest == "--" {
            return Err(anyhow!("missing request payload"));
        }
        break;
    }

    if rest.is_empty() {
        return Err(anyhow!("missing request payload"));
    }
    if !payload_after_separator && rest.starts_with("--") {
        return Err(anyhow!(
            "request payload starting with '-' must follow the -- separator"
        ));
    }
    Ok(ControlRequestArgs {
        target,
        timeout,
        since,
        until_hex,
        max_bytes,
        fail_on_timeout,
        raw_hex,
        raw_text,
        hex,
        payload: rest.to_string(),
        json,
    })
}

pub(crate) fn parse_hex_bytes(input: &str) -> Result<Vec<u8>> {
    let compact = input
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != ':' && *c != '-')
        .collect::<String>();
    if compact.is_empty() {
        return Err(anyhow!("missing hex payload"));
    }
    if compact.len() % 2 != 0 {
        return Err(anyhow!("hex payload must contain an even number of digits"));
    }

    let mut bytes = Vec::with_capacity(compact.len() / 2);
    for chunk in compact.as_bytes().chunks_exact(2) {
        let text = std::str::from_utf8(chunk).context("invalid hex payload")?;
        let byte =
            u8::from_str_radix(text, 16).with_context(|| format!("invalid hex byte {text:?}"))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

pub(crate) fn encode_hex(data: &[u8]) -> String {
    let mut rendered = String::with_capacity(data.len() * 2);
    for byte in data {
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

pub(crate) fn decode_control_utf8(data: &[u8]) -> Option<String> {
    std::str::from_utf8(data).ok().map(str::to_owned)
}

#[cfg(test)]
pub(crate) fn parse_control_read_response_next_seq(response: &str) -> Option<u64> {
    let mut parts = response.lines().next()?.split_whitespace();
    while let Some(part) = parts.next() {
        if part == "next_seq" {
            return parts.next()?.parse().ok();
        }
    }
    None
}

pub(crate) fn parse_control_flash_args(input: &str) -> Result<(PathBuf, Option<String>)> {
    let input = input.trim();
    if input.is_empty() {
        return Err(anyhow!("usage: flash <file> [addr]"));
    }

    let (path, rest) = if let Some(quote) = input.chars().next().filter(|c| *c == '"' || *c == '\'')
    {
        let mut end = None;
        for (index, ch) in input[quote.len_utf8()..].char_indices() {
            if ch == quote {
                end = Some(quote.len_utf8() + index);
                break;
            }
        }
        let Some(end) = end else {
            return Err(anyhow!("unterminated quoted flash path"));
        };
        (
            PathBuf::from(&input[quote.len_utf8()..end]),
            input[end + quote.len_utf8()..].trim(),
        )
    } else {
        let mut parts = input.splitn(2, char::is_whitespace);
        let path = parts.next().unwrap_or_default();
        (PathBuf::from(path), parts.next().unwrap_or_default().trim())
    };

    if path.as_os_str().is_empty() {
        return Err(anyhow!("usage: flash <file> [addr]"));
    }

    let addr = if rest.is_empty() {
        None
    } else if rest.split_whitespace().count() == 1 {
        Some(rest.to_string())
    } else {
        return Err(anyhow!("usage: flash <file> [addr]"));
    };

    Ok((path, addr))
}
