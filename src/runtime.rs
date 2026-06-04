use crate::*;

#[cfg(feature = "control")]
pub(crate) async fn handle_control_request(
    request: ControlRequest,
    current_route: Route,
    serial_tx: &Option<mpsc::Sender<InterfaceCommand>>,
    rtt_tx: &Option<mpsc::Sender<InterfaceCommand>>,
) {
    match request {
        ControlRequest::Write {
            target,
            bytes,
            timeout,
            reply,
        } => {
            let response =
                route_write_control(target, current_route, &bytes, timeout, serial_tx, rtt_tx)
                    .await;
            let _ = reply.send(response);
        }
        ControlRequest::Reset { reply } => {
            #[cfg(feature = "rtt")]
            if let Some(tx) = rtt_tx {
                let command = InterfaceCommand::Reset { reply: Some(reply) };
                if let Err(e) = tx.send(command).await {
                    if let InterfaceCommand::Reset { reply: Some(reply) } = e.0 {
                        let _ = reply.send("ERR rtt transport is not running\n".to_string());
                    }
                }
            } else {
                let _ = reply.send("ERR reset requires RTT/J-Link\n".to_string());
            }
            #[cfg(not(feature = "rtt"))]
            let _ = reply.send("ERR reset requires RTT/J-Link\n".to_string());
        }
        ControlRequest::Flash { path, addr, reply } => {
            #[cfg(not(feature = "rtt"))]
            {
                let _ = (path, addr);
                let _ = reply.send("ERR flash requires RTT/J-Link\n".to_string());
            }
            #[cfg(feature = "rtt")]
            {
                if let Err(e) = validate_flash_file(&path) {
                    let _ = reply.send(format!("ERR {e}\n"));
                    return;
                }
                let Some(tx) = rtt_tx else {
                    let _ = reply.send("ERR flash requires RTT/J-Link\n".to_string());
                    return;
                };
                let command = InterfaceCommand::Flash {
                    path,
                    addr,
                    reply: Some(reply),
                };
                if let Err(e) = tx.send(command).await {
                    if let InterfaceCommand::Flash {
                        path,
                        addr,
                        reply: Some(reply),
                    } = e.0
                    {
                        let _ = (path, addr);
                        let _ = reply.send("ERR rtt transport is not running\n".to_string());
                    }
                }
            }
        }
        ControlRequest::Erase { reply } => {
            #[cfg(feature = "rtt")]
            if let Some(tx) = rtt_tx {
                let command = InterfaceCommand::Erase { reply: Some(reply) };
                if let Err(e) = tx.send(command).await {
                    if let InterfaceCommand::Erase { reply: Some(reply) } = e.0 {
                        let _ = reply.send("ERR rtt transport is not running\n".to_string());
                    }
                }
            } else {
                let _ = reply.send("ERR erase requires RTT/J-Link\n".to_string());
            }
            #[cfg(not(feature = "rtt"))]
            let _ = reply.send("ERR erase requires RTT/J-Link\n".to_string());
        }
        ControlRequest::Reconnect { reply } => {
            let command = InterfaceCommand::Reconnect { reply: Some(reply) };
            let result = match current_route {
                Route::Serial => {
                    if let Some(tx) = serial_tx {
                        tx.send(command).await
                    } else {
                        if let InterfaceCommand::Reconnect { reply: Some(reply) } = command {
                            let _ = reply.send("ERR serial transport is not running\n".to_string());
                        }
                        return;
                    }
                }
                Route::Rtt | Route::Both => {
                    if let Some(tx) = rtt_tx {
                        tx.send(command).await
                    } else {
                        if let InterfaceCommand::Reconnect { reply: Some(reply) } = command {
                            let _ = reply.send("ERR rtt transport is not running\n".to_string());
                        }
                        return;
                    }
                }
            };
            if let Err(e) = result {
                if let InterfaceCommand::Reconnect { reply: Some(reply) } = e.0 {
                    let _ = reply.send("ERR no transport is running\n".to_string());
                }
            }
        }
    }
}

#[cfg(feature = "control")]
pub(crate) async fn route_write_control(
    target: ControlTarget,
    current_route: Route,
    payload: &[u8],
    timeout: Duration,
    serial_tx: &Option<mpsc::Sender<InterfaceCommand>>,
    rtt_tx: &Option<mpsc::Sender<InterfaceCommand>>,
) -> String {
    let route = match target {
        ControlTarget::Current => current_route,
        ControlTarget::Serial => Route::Serial,
        ControlTarget::Rtt => Route::Rtt,
    };

    match route {
        Route::Serial => {
            let Some(tx) = serial_tx else {
                return "ERR serial transport is not running\n".to_string();
            };
            send_transport_write("serial", tx, payload, timeout).await
        }
        Route::Rtt => {
            let Some(tx) = rtt_tx else {
                return "ERR rtt transport is not running\n".to_string();
            };
            send_transport_write("rtt", tx, payload, timeout).await
        }
        Route::Both => {
            let mut responses = Vec::new();
            let serial_write = async {
                match serial_tx {
                    Some(tx) => Some((
                        "serial",
                        send_transport_write("serial", tx, payload, timeout).await,
                    )),
                    None => None,
                }
            };
            let rtt_write = async {
                match rtt_tx {
                    Some(tx) => Some((
                        "rtt",
                        send_transport_write("rtt", tx, payload, timeout).await,
                    )),
                    None => None,
                }
            };
            let (serial_response, rtt_response) = tokio::join!(serial_write, rtt_write);
            responses.extend(serial_response);
            responses.extend(rtt_response);
            if responses.is_empty() {
                return "ERR no selected transport is running\n".to_string();
            }
            let errors = responses
                .iter()
                .filter(|(_, response)| response.starts_with("ERR"))
                .map(|(name, response)| format!("{name}: {}", response.trim()))
                .collect::<Vec<_>>();
            if errors.is_empty() {
                let targets = responses
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("OK write {} bytes targets {}\n", payload.len(), targets)
            } else {
                format!("ERR write failed: {}\n", errors.join("; "))
            }
        }
    }
}

#[cfg(feature = "control")]
pub(crate) async fn send_transport_write(
    name: &str,
    tx: &mpsc::Sender<InterfaceCommand>,
    payload: &[u8],
    timeout: Duration,
) -> String {
    let (reply, rx) = tokio::sync::oneshot::channel();
    let command = InterfaceCommand::Write {
        data: payload.to_vec(),
        reply: Some(reply),
    };
    if let Err(e) = tx.send(command).await {
        if let InterfaceCommand::Write {
            reply: Some(reply), ..
        } = e.0
        {
            let _ = reply.send(format!("ERR {name} transport is not running\n"));
        }
    }
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => format!("ERR {name} transport dropped write response\n"),
        Err(_) => format!("ERR {name} transport write timed out\n"),
    }
}

pub(crate) async fn route_write(
    route: Route,
    payload: &[u8],
    serial_tx: &Option<mpsc::Sender<InterfaceCommand>>,
    rtt_tx: &Option<mpsc::Sender<InterfaceCommand>>,
) {
    match route {
        Route::Serial => {
            if let Some(tx) = serial_tx {
                tx.send(InterfaceCommand::Write {
                    data: payload.to_vec(),
                    reply: None,
                })
                .await
                .ok();
            }
        }
        Route::Rtt => {
            if let Some(tx) = rtt_tx {
                tx.send(InterfaceCommand::Write {
                    data: payload.to_vec(),
                    reply: None,
                })
                .await
                .ok();
            }
        }
        Route::Both => {
            if let Some(tx) = serial_tx {
                tx.send(InterfaceCommand::Write {
                    data: payload.to_vec(),
                    reply: None,
                })
                .await
                .ok();
            }
            if let Some(tx) = rtt_tx {
                tx.send(InterfaceCommand::Write {
                    data: payload.to_vec(),
                    reply: None,
                })
                .await
                .ok();
            }
        }
    }
}

pub(crate) fn send_optional_control_reply(
    reply: Option<ControlReply>,
    response: impl Into<String>,
) {
    if let Some(reply) = reply {
        let _ = reply.send(response.into());
    }
}

#[cfg(any(feature = "rtt", feature = "serial"))]
pub(crate) fn handle_reconnect_wait_command(
    command: Option<InterfaceCommand>,
    reason: &str,
) -> bool {
    match command {
        Some(InterfaceCommand::Stop) | None => true,
        #[cfg(all(feature = "rtt", feature = "control"))]
        Some(InterfaceCommand::Reset { reply }) | Some(InterfaceCommand::Erase { reply }) => {
            send_optional_control_reply(reply, format!("ERR {reason}\n"));
            false
        }
        #[cfg(all(feature = "rtt", not(feature = "control")))]
        Some(InterfaceCommand::Reset { reply }) => {
            send_optional_control_reply(reply, format!("ERR {reason}\n"));
            false
        }
        #[cfg(feature = "rtt")]
        Some(InterfaceCommand::Flash { reply, .. }) => {
            send_optional_control_reply(reply, format!("ERR {reason}\n"));
            false
        }
        Some(InterfaceCommand::Write { reply, .. }) => {
            send_optional_control_reply(reply, format!("ERR {reason}\n"));
            false
        }
        Some(InterfaceCommand::Reconnect { reply }) => {
            send_optional_control_reply(reply, format!("ERR {reason}\n"));
            false
        }
    }
}

pub(crate) fn render_data(
    source: Source,
    data: &[u8],
    output_mode: OutputMode,
    timestamp: bool,
    prefix: bool,
    line_state: &mut OutputLineState,
) -> String {
    let line_prefix = render_line_prefix(source, timestamp, prefix);
    if output_mode == OutputMode::Hex {
        return render_hex_data(data, &line_prefix);
    }

    render_normal_data(data, &line_prefix, line_state.at_line_start_mut(source))
}

pub(crate) fn render_line_prefix(source: Source, timestamp: bool, prefix: bool) -> String {
    let mut rendered = String::new();
    if timestamp {
        rendered.push_str(&format!("[{}] ", monotonic_timestamp()));
    }
    if prefix {
        rendered.push_str(&format!("[{}] ", source.label()));
    }
    rendered
}

fn monotonic_timestamp() -> String {
    static START: OnceLock<Instant> = OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed();
    format!("{:06}.{:03}", elapsed.as_secs(), elapsed.subsec_millis())
}

pub(crate) fn render_hex_data(data: &[u8], prefix: &str) -> String {
    let mut rendered = String::new();
    for (offset, chunk) in data.chunks(16).enumerate() {
        rendered.push_str(prefix);
        let _ = write!(rendered, "{:04x}  ", offset * 16);

        for i in 0..16 {
            if let Some(byte) = chunk.get(i) {
                let _ = write!(rendered, "{byte:02x} ");
            } else {
                rendered.push_str("   ");
            }
            if i == 7 {
                rendered.push(' ');
            }
        }

        rendered.push_str(" |");
        for &byte in chunk {
            let ch = if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '.'
            };
            rendered.push(ch);
        }
        rendered.push_str("|\r\n");
    }
    rendered
}

pub(crate) fn render_normal_data(data: &[u8], prefix: &str, at_line_start: &mut bool) -> String {
    if prefix.is_empty() {
        return String::from_utf8_lossy(data).into_owned();
    }

    let mut rendered = String::new();
    let mut segment_start = 0;

    for (index, byte) in data.iter().copied().enumerate() {
        if *at_line_start && byte != b'\r' && byte != b'\n' {
            if segment_start < index {
                rendered.push_str(&String::from_utf8_lossy(&data[segment_start..index]));
            }
            rendered.push_str(prefix);
            *at_line_start = false;
            segment_start = index;
        }

        match byte {
            b'\n' => *at_line_start = true,
            b'\r' => *at_line_start = false,
            _ => {}
        }
    }

    if segment_start < data.len() {
        rendered.push_str(&String::from_utf8_lossy(&data[segment_start..]));
    }
    rendered
}

pub(crate) fn default_route(has_serial: bool, has_rtt: bool) -> Route {
    match (has_serial, has_rtt) {
        (true, true) => Route::Serial,
        (true, false) => Route::Serial,
        (false, true) => Route::Rtt,
        (false, false) => Route::Both,
    }
}

#[cfg(feature = "rtt")]
pub(crate) fn map_jlink<T>(result: jlink_rs::JlinkResult<T>) -> Result<T> {
    result.map_err(|e| anyhow!(e.to_string()))
}

#[cfg(feature = "rtt")]
pub(crate) fn env_jlink_lib() -> Option<PathBuf> {
    std::env::var("JLINK_LIB").ok().map(PathBuf::from)
}

#[cfg(feature = "rtt")]
pub(crate) fn env_jlink_sn() -> Option<u32> {
    std::env::var("JLINK_SN")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
}

pub(crate) fn parse_u32(value: &str) -> Result<u32> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        Ok(u32::from_str_radix(hex, 16)?)
    } else {
        Ok(trimmed.parse::<u32>()?)
    }
}

#[cfg(feature = "rtt")]
pub(crate) fn parse_connect_speed(value: &str) -> std::result::Result<ConnectSpeed, String> {
    let value = value.trim().to_ascii_lowercase();
    if value == "auto" {
        return Ok(ConnectSpeed::Auto);
    }
    if value == "adaptive" {
        return Ok(ConnectSpeed::Adaptive);
    }
    let khz = value.parse::<u32>().map_err(|e| e.to_string())?;
    Ok(ConnectSpeed::Khz(khz))
}
