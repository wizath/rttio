use crate::*;

#[cfg(feature = "serial")]
pub(crate) struct SerialTaskConfig {
    pub(crate) path: PathBuf,
    pub(crate) baud: u32,
    pub(crate) flow_control: SerialFlowControl,
    #[cfg(feature = "espflash")]
    pub(crate) espflash: EspFlashConfig,
}

#[cfg(any(feature = "serial", feature = "rtt"))]
const TRANSPORT_WRITE_TIMEOUT_MS: u64 = 1_000;
#[cfg(all(feature = "serial", feature = "espflash", feature = "control"))]
const ESP_FLASH_PROGRESS_HOLD_MS: u64 = 750;

#[cfg(any(feature = "serial", feature = "rtt"))]
async fn write_all_with_timeout<W>(writer: &mut W, data: &[u8]) -> io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    match tokio::time::timeout(
        Duration::from_millis(TRANSPORT_WRITE_TIMEOUT_MS),
        writer.write_all(data),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "transport write timed out",
        )),
    }
}

#[cfg(feature = "serial")]
pub(crate) async fn serial_task(
    config: SerialTaskConfig,
    mut rx: mpsc::Receiver<InterfaceCommand>,
    events: mpsc::Sender<InterfaceEvent>,
    no_reconnect: bool,
    reconnect_delay: Duration,
) {
    let SerialTaskConfig {
        path,
        baud,
        flow_control,
        #[cfg(feature = "espflash")]
            espflash: espflash_config,
    } = config;
    let path_text = path.display().to_string();
    let mut opening_announced = false;
    let mut last_open_problem: Option<String> = None;
    let mut reconnect_reply: Option<ControlReply> = None;
    let mut post_reopen_reply: Option<(Option<ControlReply>, String)> = None;
    #[cfg(feature = "espflash")]
    let mut reset_after_reopen = false;

    if let Some(addr) = path_text.strip_prefix("tcp://") {
        serial_tcp_task(addr.to_string(), rx, events, no_reconnect, reconnect_delay).await;
        return;
    }

    loop {
        let mut reopen_after_action = false;
        if !path.exists() {
            let problem = format!("{path_text} not found; waiting for it to appear");
            if last_open_problem.as_deref() != Some(problem.as_str()) {
                let _ = events
                    .send(InterfaceEvent::Error {
                        source: Source::Serial,
                        text: problem.clone(),
                    })
                    .await;
                last_open_problem = Some(problem);
            }
            opening_announced = false;
        } else {
            if !opening_announced {
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Serial,
                        text: format!(
                            "opening {path_text} at {baud} baud ({flow_control:?} flow control)"
                        ),
                    })
                    .await;
                opening_announced = true;
            }

            let builder =
                tokio_serial::new(path_text.clone(), baud).flow_control(flow_control.into());

            match builder.open_native_async() {
                Ok(mut port) => {
                    last_open_problem = None;
                    #[cfg(feature = "espflash")]
                    {
                        if reset_after_reopen {
                            reset_after_reopen = false;
                            let usb_pid = serial_usb_pid(&path_text);
                            if let Err(e) = reset_open_serial_after_flash(&mut port, usb_pid).await
                            {
                                let _ = events
                                    .send(InterfaceEvent::Error {
                                        source: Source::Serial,
                                        text: format!("ESP reset after serial reopen failed: {e}"),
                                    })
                                    .await;
                            } else {
                                let _ = events
                                    .send(InterfaceEvent::Status {
                                        source: Source::Serial,
                                        text: "ESP target reset after serial reopen".to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                    let _ = events
                        .send(InterfaceEvent::Status {
                            source: Source::Serial,
                            text: "connected".to_string(),
                        })
                        .await;
                    send_optional_control_reply(reconnect_reply.take(), "OK reconnect\n");
                    if let Some((reply, response)) = post_reopen_reply.take() {
                        send_optional_control_reply(reply, response);
                    }
                    let mut buf = vec![0u8; 1024];

                    loop {
                        tokio::select! {
                            read = port.read(&mut buf) => {
                                match read {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let _ = events.send(InterfaceEvent::Data {
                                            source: Source::Serial,
                                            data: buf[..n].to_vec(),
                                        }).await;
                                    }
                                    Err(e) => {
                                        let _ = events.send(InterfaceEvent::Error {
                                            source: Source::Serial,
                                            text: e.to_string(),
                                        }).await;
                                        break;
                                    }
                                }
                            }
                            command = rx.recv() => {
                                match command {
                                    Some(InterfaceCommand::Write { data, reply }) => {
                                        if let Err(e) = write_all_with_timeout(&mut port, &data).await {
                                            let text = e.to_string();
                                            send_optional_control_reply(
                                                reply,
                                                format!("ERR serial write failed: {text}\n"),
                                            );
                                            let _ = events.send(InterfaceEvent::Error {
                                                source: Source::Serial,
                                                text,
                                            }).await;
                                            break;
                                        } else {
                                            send_optional_control_reply(
                                                reply,
                                                format!("OK serial write {} bytes\n", data.len()),
                                            );
                                        }
                                    }
                                    Some(InterfaceCommand::Reconnect { reply }) => {
                                        reconnect_reply = reply;
                                        break;
                                    }
                                    Some(InterfaceCommand::Reset { reply }) => {
                                        let response = reset_open_serial_target(
                                            &mut port,
                                            &path_text,
                                            &events,
                                        ).await;
                                        send_optional_control_reply(reply, response);
                                    }
                                    Some(InterfaceCommand::Flash { path: file_path, addr, reply }) => {
                                        reopen_after_action = true;
                                        drop(port);
                                        let response = serial_flash_target(
                                            path.clone(),
                                            file_path,
                                            addr,
                                            #[cfg(feature = "espflash")]
                                            espflash_config.clone(),
                                            &events,
                                        ).await;
                                        #[cfg(feature = "espflash")]
                                        if response.starts_with("OK") {
                                            reset_after_reopen = true;
                                            post_reopen_reply = Some((reply, response));
                                        } else {
                                            send_optional_control_reply(reply, response);
                                        }
                                        #[cfg(not(feature = "espflash"))]
                                        send_optional_control_reply(reply, response);
                                        break;
                                    }
                                    Some(InterfaceCommand::Erase { reply }) => {
                                        reopen_after_action = true;
                                        drop(port);
                                        let response = serial_erase_target(
                                            path.clone(),
                                            #[cfg(feature = "espflash")]
                                            espflash_config.clone(),
                                            &events,
                                        ).await;
                                        if response.starts_with("OK") {
                                            post_reopen_reply = Some((reply, response));
                                        } else {
                                            send_optional_control_reply(reply, response);
                                        }
                                        break;
                                    }
                                    Some(InterfaceCommand::Stop) | None => {
                                        let _ = events.send(InterfaceEvent::Stopped(Source::Serial)).await;
                                        return;
                                    }
                                }
                            }
                        }
                    }

                    opening_announced = false;
                    let _ = events
                        .send(InterfaceEvent::Status {
                            source: Source::Serial,
                            text: "disconnected".to_string(),
                        })
                        .await;
                }
                Err(e) => {
                    send_optional_control_reply(
                        reconnect_reply.take(),
                        format!("ERR serial reconnect failed: {e}\n"),
                    );
                    let problem = format!("failed to open {path_text}: {e}");
                    if last_open_problem.as_deref() != Some(problem.as_str()) {
                        let _ = events
                            .send(InterfaceEvent::Error {
                                source: Source::Serial,
                                text: problem.clone(),
                            })
                            .await;
                        last_open_problem = Some(problem);
                    }
                }
            }
        }

        if no_reconnect && !reopen_after_action {
            if let Some((reply, response)) = post_reopen_reply.take() {
                send_optional_control_reply(
                    reply,
                    format!(
                        "ERR serial did not reopen after action; original result: {}",
                        response.trim_end()
                    ),
                );
            }
            let _ = events.send(InterfaceEvent::Stopped(Source::Serial)).await;
            return;
        }

        tokio::select! {
            _ = tokio::time::sleep(reconnect_delay) => {}
            command = rx.recv() => {
                if handle_reconnect_wait_command(command, "serial transport is reconnecting") {
                        let _ = events.send(InterfaceEvent::Stopped(Source::Serial)).await;
                        return;
                }
            }
        }
    }
}

#[cfg(any(feature = "serial", feature = "rtt"))]
pub(crate) async fn raw_tcp_bridge_server(
    addr: String,
    input_tx: mpsc::Sender<InputEvent>,
    output_rx: broadcast::Receiver<Vec<u8>>,
    events: mpsc::Sender<TerminalEvent>,
) {
    let listener = match TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            let _ = terminal_status(&events, &format!("serve failed to bind {addr}: {e}")).await;
            return;
        }
    };

    let _ = terminal_status(&events, &format!("serving raw TCP on {addr}")).await;

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let _ = terminal_status(&events, &format!("serve client connected {peer}")).await;
                tokio::spawn(raw_tcp_bridge_client(
                    peer.to_string(),
                    stream,
                    input_tx.clone(),
                    output_rx.resubscribe(),
                    events.clone(),
                ));
            }
            Err(e) => {
                let _ = terminal_status(&events, &format!("serve accept failed: {e}")).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

#[cfg(any(feature = "serial", feature = "rtt"))]
async fn raw_tcp_bridge_client(
    peer: String,
    stream: TcpStream,
    input_tx: mpsc::Sender<InputEvent>,
    mut output_rx: broadcast::Receiver<Vec<u8>>,
    events: mpsc::Sender<TerminalEvent>,
) {
    let (mut reader, mut writer) = stream.into_split();
    let mut read_buf = vec![0u8; 1024];

    loop {
        tokio::select! {
            read = reader.read(&mut read_buf) => {
                match read {
                    Ok(0) => break,
                    Ok(n) => {
                        if input_tx
                            .send(InputEvent::Bytes(read_buf[..n].to_vec()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = terminal_status(&events, &format!("serve client {peer} read failed: {e}")).await;
                        break;
                    }
                }
            }
            output = output_rx.recv() => {
                match output {
                    Ok(bytes) => {
                        if let Err(e) = write_all_with_timeout(&mut writer, &bytes).await {
                            let _ = terminal_status(&events, &format!("serve client {peer} write failed: {e}")).await;
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    let _ = terminal_status(&events, &format!("serve client disconnected {peer}")).await;
}

#[cfg(feature = "serial")]
async fn serial_tcp_task(
    addr: String,
    mut rx: mpsc::Receiver<InterfaceCommand>,
    events: mpsc::Sender<InterfaceEvent>,
    no_reconnect: bool,
    reconnect_delay: Duration,
) {
    let mut reconnect_reply: Option<ControlReply> = None;

    loop {
        let _ = events
            .send(InterfaceEvent::Status {
                source: Source::Serial,
                text: format!("connecting TCP serial {addr}"),
            })
            .await;

        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Serial,
                        text: format!("TCP serial connected {addr}"),
                    })
                    .await;
                send_optional_control_reply(reconnect_reply.take(), "OK reconnect\n");
                let (mut reader, mut writer) = stream.into_split();
                let mut buf = vec![0u8; 1024];

                loop {
                    tokio::select! {
                        read = reader.read(&mut buf) => {
                            match read {
                                Ok(0) => break,
                                Ok(n) => {
                                    let _ = events.send(InterfaceEvent::Data {
                                        source: Source::Serial,
                                        data: buf[..n].to_vec(),
                                    }).await;
                                }
                                Err(e) => {
                                    let _ = events.send(InterfaceEvent::Error {
                                        source: Source::Serial,
                                        text: e.to_string(),
                                    }).await;
                                    break;
                                }
                            }
                        }
                        command = rx.recv() => {
                            match command {
                                Some(InterfaceCommand::Write { data, reply }) => {
                                    if let Err(e) = write_all_with_timeout(&mut writer, &data).await {
                                        let text = e.to_string();
                                        send_optional_control_reply(
                                            reply,
                                            format!("ERR tcp serial write failed: {text}\n"),
                                        );
                                        let _ = events.send(InterfaceEvent::Error {
                                            source: Source::Serial,
                                            text,
                                        }).await;
                                        break;
                                    }
                                    send_optional_control_reply(
                                        reply,
                                        format!("OK serial write {} bytes\n", data.len()),
                                    );
                                }
                                Some(InterfaceCommand::Reconnect { reply }) => {
                                    reconnect_reply = reply;
                                    break;
                                }
                                Some(InterfaceCommand::Reset { reply }) => {
                                    send_optional_control_reply(
                                        reply,
                                        "ERR reset is not available in TCP serial mode\n",
                                    );
                                }
                                Some(InterfaceCommand::Flash { reply, .. }) => {
                                    send_optional_control_reply(
                                        reply,
                                        "ERR flash is not available in TCP serial mode\n",
                                    );
                                }
                                Some(InterfaceCommand::Erase { reply }) => {
                                    send_optional_control_reply(
                                        reply,
                                        "ERR erase is not available in TCP serial mode\n",
                                    );
                                }
                                Some(InterfaceCommand::Stop) | None => {
                                    let _ = events.send(InterfaceEvent::Stopped(Source::Serial)).await;
                                    return;
                                }
                            }
                        }
                    }
                }

                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Serial,
                        text: "TCP serial disconnected".to_string(),
                    })
                    .await;
            }
            Err(e) => {
                send_optional_control_reply(
                    reconnect_reply.take(),
                    format!("ERR tcp serial reconnect failed: {e}\n"),
                );
                let _ = events
                    .send(InterfaceEvent::Error {
                        source: Source::Serial,
                        text: format!("failed to connect TCP serial {addr}: {e}"),
                    })
                    .await;
            }
        }

        if no_reconnect {
            let _ = events.send(InterfaceEvent::Stopped(Source::Serial)).await;
            return;
        }

        tokio::select! {
            _ = tokio::time::sleep(reconnect_delay) => {}
            command = rx.recv() => {
                if handle_reconnect_wait_command(command, "TCP serial transport is reconnecting") {
                    let _ = events.send(InterfaceEvent::Stopped(Source::Serial)).await;
                    return;
                }
            }
        }
    }
}

#[cfg(all(feature = "serial", feature = "espflash"))]
const ESP_USB_SERIAL_JTAG_PID: u16 = 0x1001;

#[cfg(all(feature = "serial", feature = "espflash"))]
async fn reset_open_serial_after_flash(
    port: &mut tokio_serial::SerialStream,
    usb_pid: Option<u16>,
) -> Result<()> {
    tokio::time::sleep(Duration::from_millis(100)).await;

    if usb_pid == Some(ESP_USB_SERIAL_JTAG_PID) {
        port.write_data_terminal_ready(false)
            .context("failed to release DTR for ESP USB-Serial/JTAG reset")?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        port.write_request_to_send(true)
            .context("failed to assert RTS for ESP USB-Serial/JTAG reset")?;
        port.write_data_terminal_ready(false)
            .context("failed to release DTR for ESP USB-Serial/JTAG reset")?;
        port.write_request_to_send(true)
            .context("failed to assert RTS for ESP USB-Serial/JTAG reset")?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        port.write_request_to_send(false)
            .context("failed to release RTS for ESP USB-Serial/JTAG reset")?;
    } else {
        port.write_data_terminal_ready(false)
            .context("failed to release DTR for ESP reset")?;
        port.write_request_to_send(false)
            .context("failed to release RTS for ESP reset")?;
        tokio::time::sleep(Duration::from_millis(20)).await;
        port.write_request_to_send(true)
            .context("failed to assert RTS for ESP reset")?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        port.write_request_to_send(false)
            .context("failed to release RTS for ESP reset")?;
        port.write_data_terminal_ready(false)
            .context("failed to keep DTR released for ESP reset")?;
    }

    Ok(())
}

#[cfg(feature = "serial")]
async fn reset_open_serial_target(
    port: &mut tokio_serial::SerialStream,
    path_text: &str,
    events: &mpsc::Sender<InterfaceEvent>,
) -> String {
    #[cfg(feature = "espflash")]
    {
        let _ = events
            .send(InterfaceEvent::Status {
                source: Source::Serial,
                text: "resetting ESP target".to_string(),
            })
            .await;
        let usb_pid = serial_usb_pid(path_text);
        match reset_open_serial_after_flash(port, usb_pid).await {
            Ok(()) => {
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Serial,
                        text: "ESP target reset".to_string(),
                    })
                    .await;
                "OK reset\n".to_string()
            }
            Err(e) => {
                let text = format!("ESP reset failed: {e}");
                let _ = events
                    .send(InterfaceEvent::Error {
                        source: Source::Serial,
                        text: text.clone(),
                    })
                    .await;
                format!("ERR {text}\n")
            }
        }
    }
    #[cfg(not(feature = "espflash"))]
    {
        let _ = (port, path_text, events);
        "ERR reset requires ESP serial flasher support\n".to_string()
    }
}

#[cfg(all(feature = "serial", feature = "espflash"))]
fn serial_usb_pid(path: &str) -> Option<u16> {
    serialport::available_ports()
        .ok()?
        .into_iter()
        .find(|info| info.port_name == path)
        .and_then(|info| match info.port_type {
            serialport::SerialPortType::UsbPort(usb) => Some(usb.pid),
            _ => None,
        })
}

#[cfg(feature = "serial")]
async fn serial_flash_target(
    port_path: PathBuf,
    file_path: PathBuf,
    addr: u32,
    #[cfg(feature = "espflash")] config: EspFlashConfig,
    events: &mpsc::Sender<InterfaceEvent>,
) -> String {
    #[cfg(feature = "espflash")]
    {
        let _ = events
            .send(InterfaceEvent::Status {
                source: Source::Serial,
                text: format!("flashing ESP {} at 0x{addr:08x}", file_path.display()),
            })
            .await;
        let flash = esp_flash_bin_with_optional_progress(
            port_path,
            file_path.clone(),
            addr,
            config,
            events,
        )
        .await;
        match flash {
            Ok(bytes) => {
                let remembered_addr = if flash_file_uses_embedded_address(&file_path) {
                    None
                } else {
                    Some(addr)
                };
                if let Err(e) =
                    remember_flash_file_blocking(file_path.clone(), remembered_addr).await
                {
                    let _ = events
                        .send(InterfaceEvent::Error {
                            source: Source::Serial,
                            text: format!("failed to remember flash file: {e}"),
                        })
                        .await;
                }
                let text = format!("ESP flash done, wrote {bytes} bytes");
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Serial,
                        text: text.clone(),
                    })
                    .await;
                #[cfg(feature = "control")]
                clear_flash_progress_after_hold(events).await;
                format!("OK {text}\n")
            }
            Err(e) => {
                let text = format!("ESP flash failed: {e}");
                let _ = events
                    .send(InterfaceEvent::Error {
                        source: Source::Serial,
                        text: text.clone(),
                    })
                    .await;
                #[cfg(feature = "control")]
                clear_flash_progress_after_hold(events).await;
                format!("ERR {text}\n")
            }
        }
    }
    #[cfg(not(feature = "espflash"))]
    {
        let _ = (port_path, file_path, addr);
        let _ = events;
        "ERR flash requires ESP serial flasher support\n".to_string()
    }
}

#[cfg(all(feature = "serial", feature = "espflash", feature = "control"))]
async fn esp_flash_bin_with_optional_progress(
    port_path: PathBuf,
    file_path: PathBuf,
    addr: u32,
    config: EspFlashConfig,
    events: &mpsc::Sender<InterfaceEvent>,
) -> Result<usize> {
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
    let _ = events
        .send(InterfaceEvent::FlashProgress(Some(TerminalFlashProgress {
            action: "prepare".to_string(),
            percent: 0,
        })))
        .await;
    let flash = esp_flash_bin(port_path, file_path, addr, config, Some(progress_tx));
    tokio::pin!(flash);

    loop {
        tokio::select! {
            result = &mut flash => {
                let action = if result.is_ok() { "done" } else { "failed" };
                let _ = events
                    .send(InterfaceEvent::FlashProgress(Some(TerminalFlashProgress {
                        action: action.to_string(),
                        percent: if result.is_ok() { 100 } else { 0 },
                    })))
                    .await;
                break result;
            }
            Some(progress) = progress_rx.recv() => {
                let _ = events
                    .send(InterfaceEvent::FlashProgress(Some(progress)))
                    .await;
            }
        }
    }
}

#[cfg(all(feature = "serial", feature = "espflash", not(feature = "control")))]
async fn esp_flash_bin_with_optional_progress(
    port_path: PathBuf,
    file_path: PathBuf,
    addr: u32,
    config: EspFlashConfig,
    _events: &mpsc::Sender<InterfaceEvent>,
) -> Result<usize> {
    esp_flash_bin(port_path, file_path, addr, config, None).await
}

#[cfg(all(feature = "serial", feature = "espflash", feature = "control"))]
async fn clear_flash_progress_after_hold(events: &mpsc::Sender<InterfaceEvent>) {
    tokio::time::sleep(Duration::from_millis(ESP_FLASH_PROGRESS_HOLD_MS)).await;
    let _ = events.send(InterfaceEvent::FlashProgress(None)).await;
}

#[cfg(feature = "serial")]
async fn serial_erase_target(
    port_path: PathBuf,
    #[cfg(feature = "espflash")] config: EspFlashConfig,
    events: &mpsc::Sender<InterfaceEvent>,
) -> String {
    #[cfg(feature = "espflash")]
    {
        let _ = events
            .send(InterfaceEvent::Status {
                source: Source::Serial,
                text: "erasing ESP flash".to_string(),
            })
            .await;
        match esp_erase(port_path, config).await {
            Ok(()) => {
                let text = "ESP erase done".to_string();
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Serial,
                        text: text.clone(),
                    })
                    .await;
                format!("OK {text}\n")
            }
            Err(e) => {
                let text = format!("ESP erase failed: {e}");
                let _ = events
                    .send(InterfaceEvent::Error {
                        source: Source::Serial,
                        text: text.clone(),
                    })
                    .await;
                format!("ERR {text}\n")
            }
        }
    }
    #[cfg(not(feature = "espflash"))]
    {
        let _ = port_path;
        let _ = events;
        "ERR erase requires ESP serial flasher support\n".to_string()
    }
}

#[cfg(feature = "rtt")]
pub(crate) async fn rtt_tcp_task(
    host: String,
    port: u16,
    mut rx: mpsc::Receiver<InterfaceCommand>,
    events: mpsc::Sender<InterfaceEvent>,
    no_reconnect: bool,
    reconnect_delay: Duration,
) {
    let addr = format!("{host}:{port}");
    let mut reconnect_reply: Option<ControlReply> = None;

    loop {
        let _ = events
            .send(InterfaceEvent::Status {
                source: Source::Rtt,
                text: format!("connecting RTT stream {addr}"),
            })
            .await;

        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Rtt,
                        text: format!("RTT stream connected {addr}"),
                    })
                    .await;
                send_optional_control_reply(reconnect_reply.take(), "OK reconnect\n");
                let (mut reader, mut writer) = stream.into_split();
                let mut buf = vec![0u8; 1024];

                loop {
                    tokio::select! {
                        read = reader.read(&mut buf) => {
                            match read {
                                Ok(0) => break,
                                Ok(n) => {
                                    let _ = events.send(InterfaceEvent::Data {
                                        source: Source::Rtt,
                                        data: buf[..n].to_vec(),
                                    }).await;
                                }
                                Err(e) => {
                                    let _ = events.send(InterfaceEvent::Error {
                                        source: Source::Rtt,
                                        text: e.to_string(),
                                    }).await;
                                    break;
                                }
                            }
                        }
                        command = rx.recv() => {
                            match command {
                                Some(InterfaceCommand::Write { data, reply }) => {
                                    if let Err(e) = write_all_with_timeout(&mut writer, &data).await {
                                        let text = e.to_string();
                                        send_optional_control_reply(
                                            reply,
                                            format!("ERR tcp rtt write failed: {text}\n"),
                                        );
                                        let _ = events.send(InterfaceEvent::Error {
                                            source: Source::Rtt,
                                            text,
                                        }).await;
                                        break;
                                    } else {
                                        send_optional_control_reply(
                                            reply,
                                            format!("OK rtt write {} bytes\n", data.len()),
                                        );
                                    }
                                }
                                Some(InterfaceCommand::Reconnect { reply }) => {
                                    reconnect_reply = reply;
                                    break;
                                }
                                Some(InterfaceCommand::Reset { reply }) => {
                                    send_optional_control_reply(
                                        reply,
                                        "ERR reset is not available in RTT stream mode\n",
                                    );
                                    let _ = events.send(InterfaceEvent::Status {
                                        source: Source::Rtt,
                                            text: "reset is not available in RTT stream mode".to_string(),
                                    }).await;
                                }
                                Some(InterfaceCommand::Flash { reply, .. }) => {
                                    send_optional_control_reply(
                                        reply,
                                        "ERR flash is not available in RTT stream mode\n",
                                    );
                                    let _ = events.send(InterfaceEvent::Status {
                                        source: Source::Rtt,
                                            text: "flash is not available in RTT stream mode".to_string(),
                                    }).await;
                                }
                                Some(InterfaceCommand::Erase { reply }) => {
                                    send_optional_control_reply(
                                        reply,
                                        "ERR erase is not available in RTT stream mode\n",
                                    );
                                    let _ = events.send(InterfaceEvent::Status {
                                        source: Source::Rtt,
                                            text: "erase is not available in RTT stream mode".to_string(),
                                    }).await;
                                }
                                Some(InterfaceCommand::Stop) | None => {
                                    let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
                                    return;
                                }
                            }
                        }
                    }
                }
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Rtt,
                        text: "disconnected".to_string(),
                    })
                    .await;
            }
            Err(e) => {
                send_optional_control_reply(
                    reconnect_reply.take(),
                    format!("ERR RTT stream reconnect failed: {e}\n"),
                );
                let _ = events
                    .send(InterfaceEvent::Error {
                        source: Source::Rtt,
                        text: format!("failed to connect RTT stream {addr}: {e}"),
                    })
                    .await;
            }
        }

        if no_reconnect {
            let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
            return;
        }

        tokio::select! {
            _ = tokio::time::sleep(reconnect_delay) => {}
            command = rx.recv() => {
                if handle_reconnect_wait_command(command, "RTT stream transport is reconnecting") {
                        let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
                        return;
                }
            }
        }
    }
}

#[cfg(feature = "rtt")]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn rtt_task(
    chip: String,
    sn: Option<u32>,
    ip_addr: Option<String>,
    explicit_lib: Option<PathBuf>,
    speed: ConnectSpeed,
    rtt_telnet_port: Option<u16>,
    up_channel: u32,
    down_channel: u32,
    chunk: usize,
    poll_ms: u64,
    no_reconnect: bool,
    reconnect_delay: Duration,
    mut rx: mpsc::Receiver<InterfaceCommand>,
    events: mpsc::Sender<InterfaceEvent>,
) {
    let mut connected_once = false;
    let mut reset_after_reconnect = false;
    let mut reconnect_reply: Option<ControlReply> = None;

    loop {
        let _ = events
            .send(InterfaceEvent::Status {
                source: Source::Rtt,
                text: format!("connecting to {chip}"),
            })
            .await;

        match open_rtt_async(
            &chip,
            sn,
            ip_addr.clone(),
            explicit_lib.clone(),
            speed,
            rtt_telnet_port,
        )
        .await
        {
            Ok(jlink) => {
                if reset_after_reconnect {
                    reset_after_reconnect = false;
                    let reset_result = jlink.reset_target(0, false).await;
                    match reset_result {
                        Ok(()) => {
                            let _ = events
                                .send(InterfaceEvent::Status {
                                    source: Source::Rtt,
                                    text: "target reset after reconnect".to_string(),
                                })
                                .await;
                            let _ = jlink.rtt_stop().await;
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            match jlink.rtt_start(None).await {
                                Ok(()) => {
                                    send_optional_control_reply(
                                        reconnect_reply.take(),
                                        "OK reconnect\n",
                                    );
                                }
                                Err(e) => {
                                    send_optional_control_reply(
                                        reconnect_reply.take(),
                                        format!("ERR RTT restart after reconnect failed: {e}\n"),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            send_optional_control_reply(
                                reconnect_reply.take(),
                                format!("ERR reset after reconnect failed: {e}\n"),
                            );
                            let _ = events
                                .send(InterfaceEvent::Error {
                                    source: Source::Rtt,
                                    text: format!("reset after reconnect failed: {e}"),
                                })
                                .await;
                        }
                    }
                } else {
                    send_optional_control_reply(reconnect_reply.take(), "OK reconnect\n");
                }
                connected_once = true;
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Rtt,
                        text: format!("connected up={up_channel} down={down_channel}"),
                    })
                    .await;

                'connected: loop {
                    while let Ok(command) = rx.try_recv() {
                        match command {
                            InterfaceCommand::Write { data, reply } => {
                                match tokio::time::timeout(
                                    Duration::from_millis(TRANSPORT_WRITE_TIMEOUT_MS),
                                    jlink.rtt_write(down_channel, data),
                                )
                                .await
                                {
                                    Ok(Ok(bytes)) => {
                                        send_optional_control_reply(
                                            reply,
                                            format!("OK rtt write {bytes} bytes\n"),
                                        );
                                    }
                                    Ok(Err(e)) => {
                                        let text = e.to_string();
                                        send_optional_control_reply(
                                            reply,
                                            format!("ERR rtt write failed: {text}\n"),
                                        );
                                        let _ = events
                                            .send(InterfaceEvent::Error {
                                                source: Source::Rtt,
                                                text,
                                            })
                                            .await;
                                        break 'connected;
                                    }
                                    Err(_) => {
                                        let text = "rtt write timed out".to_string();
                                        send_optional_control_reply(reply, format!("ERR {text}\n"));
                                        let _ = events
                                            .send(InterfaceEvent::Error {
                                                source: Source::Rtt,
                                                text,
                                            })
                                            .await;
                                        break 'connected;
                                    }
                                }
                            }
                            InterfaceCommand::Reconnect { reply } => {
                                reconnect_reply = reply;
                                reset_after_reconnect = true;
                                break 'connected;
                            }
                            InterfaceCommand::Reset { reply } => {
                                let response = match jlink.reset_target(0, false).await {
                                    Ok(()) => {
                                        let _ = events
                                            .send(InterfaceEvent::Status {
                                                source: Source::Rtt,
                                                text: "target reset".to_string(),
                                            })
                                            .await;
                                        "OK reset\n".to_string()
                                    }
                                    Err(e) => {
                                        let text = e.to_string();
                                        let _ = events
                                            .send(InterfaceEvent::Error {
                                                source: Source::Rtt,
                                                text: text.clone(),
                                            })
                                            .await;
                                        format!("ERR {text}\n")
                                    }
                                };
                                send_optional_control_reply(reply, response);
                            }
                            InterfaceCommand::Flash { path, addr, reply } => {
                                let _ = events
                                    .send(InterfaceEvent::Status {
                                        source: Source::Rtt,
                                        text: format!(
                                            "flashing {} at 0x{addr:08x}",
                                            path.display()
                                        ),
                                    })
                                    .await;
                                let _ = jlink.rtt_stop().await;
                                let response = match flash_file_with_optional_progress(
                                    &jlink,
                                    path.clone(),
                                    addr,
                                    &events,
                                )
                                .await
                                {
                                    Ok(bytes) => {
                                        let remembered_addr =
                                            if flash_file_uses_embedded_address(&path) {
                                                None
                                            } else {
                                                Some(addr)
                                            };
                                        if let Err(e) = remember_flash_file_blocking(
                                            path.clone(),
                                            remembered_addr,
                                        )
                                        .await
                                        {
                                            let _ = events
                                                .send(InterfaceEvent::Error {
                                                    source: Source::Rtt,
                                                    text: format!(
                                                        "failed to remember flash file: {e}"
                                                    ),
                                                })
                                                .await;
                                        }
                                        let text =
                                            format!("flash done, J-Link reported {bytes} bytes");
                                        let _ = events
                                            .send(InterfaceEvent::Status {
                                                source: Source::Rtt,
                                                text: text.clone(),
                                            })
                                            .await;
                                        match jlink.reset_target(0, false).await {
                                            Ok(()) => {
                                                let _ = events
                                                    .send(InterfaceEvent::Status {
                                                        source: Source::Rtt,
                                                        text: "target reset after flash"
                                                            .to_string(),
                                                    })
                                                    .await;
                                                format!("OK {text}\n")
                                            }
                                            Err(e) => {
                                                let text = format!("flash done, reset failed: {e}");
                                                let _ = events
                                                    .send(InterfaceEvent::Error {
                                                        source: Source::Rtt,
                                                        text: text.clone(),
                                                    })
                                                    .await;
                                                format!("ERR {text}\n")
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let text = format!("flash failed: {e}");
                                        let _ = events
                                            .send(InterfaceEvent::Error {
                                                source: Source::Rtt,
                                                text: text.clone(),
                                            })
                                            .await;
                                        format!("ERR {text}\n")
                                    }
                                };
                                #[cfg(feature = "control")]
                                let _ = events.send(InterfaceEvent::FlashProgress(None)).await;
                                match jlink.rtt_start(None).await {
                                    Ok(()) => {
                                        let _ = events
                                            .send(InterfaceEvent::Status {
                                                source: Source::Rtt,
                                                text: "RTT restarted after flash".to_string(),
                                            })
                                            .await;
                                        send_optional_control_reply(reply, response);
                                    }
                                    Err(e) => {
                                        let response =
                                            format!("ERR failed to restart RTT after flash: {e}\n");
                                        let _ = events
                                            .send(InterfaceEvent::Error {
                                                source: Source::Rtt,
                                                text: response
                                                    .trim_start_matches("ERR ")
                                                    .trim_end()
                                                    .to_string(),
                                            })
                                            .await;
                                        send_optional_control_reply(reply, response);
                                        break 'connected;
                                    }
                                }
                            }
                            InterfaceCommand::Erase { reply } => {
                                let _ = events
                                    .send(InterfaceEvent::Status {
                                        source: Source::Rtt,
                                        text: "erasing target chip".to_string(),
                                    })
                                    .await;
                                let _ = jlink.rtt_stop().await;
                                let response = match jlink.erase_chip().await {
                                    Ok(result) => {
                                        let text = format!("erase done, J-Link reported {result}");
                                        let _ = events
                                            .send(InterfaceEvent::Status {
                                                source: Source::Rtt,
                                                text: text.clone(),
                                            })
                                            .await;
                                        format!("OK {text}\n")
                                    }
                                    Err(e) => {
                                        let text = format!("erase failed: {e}");
                                        let _ = events
                                            .send(InterfaceEvent::Error {
                                                source: Source::Rtt,
                                                text: text.clone(),
                                            })
                                            .await;
                                        format!("ERR {text}\n")
                                    }
                                };
                                #[cfg(feature = "control")]
                                let _ = events.send(InterfaceEvent::FlashProgress(None)).await;
                                match jlink.rtt_start(None).await {
                                    Ok(()) => {
                                        let _ = events
                                            .send(InterfaceEvent::Status {
                                                source: Source::Rtt,
                                                text: "RTT restarted after erase".to_string(),
                                            })
                                            .await;
                                        send_optional_control_reply(reply, response);
                                    }
                                    Err(e) => {
                                        let response =
                                            format!("ERR failed to restart RTT after erase: {e}\n");
                                        let _ = events
                                            .send(InterfaceEvent::Error {
                                                source: Source::Rtt,
                                                text: response
                                                    .trim_start_matches("ERR ")
                                                    .trim_end()
                                                    .to_string(),
                                            })
                                            .await;
                                        send_optional_control_reply(reply, response);
                                        break 'connected;
                                    }
                                }
                            }
                            InterfaceCommand::Stop => {
                                let _ = jlink.rtt_stop().await;
                                let _ = jlink.close().await;
                                let _ = jlink.shutdown().await;
                                let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
                                return;
                            }
                        }
                    }
                    if rx.is_closed() {
                        let _ = jlink.rtt_stop().await;
                        let _ = jlink.close().await;
                        let _ = jlink.shutdown().await;
                        let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
                        return;
                    }

                    match jlink.rtt_read(up_channel, chunk).await {
                        Ok(data) if !data.is_empty() => {
                            let _ = events
                                .send(InterfaceEvent::Data {
                                    source: Source::Rtt,
                                    data,
                                })
                                .await;
                        }
                        Ok(_) => tokio::time::sleep(Duration::from_millis(poll_ms)).await,
                        Err(e) => {
                            let _ = events
                                .send(InterfaceEvent::Error {
                                    source: Source::Rtt,
                                    text: e.to_string(),
                                })
                                .await;
                            break;
                        }
                    }
                }

                let _ = jlink.rtt_stop().await;
                let _ = jlink.close().await;
                let _ = jlink.shutdown().await;
                let _ = events
                    .send(InterfaceEvent::Status {
                        source: Source::Rtt,
                        text: "disconnected".to_string(),
                    })
                    .await;
            }
            Err(e) => {
                send_optional_control_reply(
                    reconnect_reply.take(),
                    format!("ERR RTT reconnect failed: {e}\n"),
                );
                let _ = events
                    .send(InterfaceEvent::Error {
                        source: Source::Rtt,
                        text: e.to_string(),
                    })
                    .await;
                if !connected_once {
                    let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
                    return;
                }
            }
        }

        if no_reconnect {
            let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
            return;
        }

        if wait_for_rtt_stop(&mut rx, reconnect_delay).await {
            let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
            return;
        }
    }
}

#[cfg(all(feature = "rtt", feature = "control"))]
async fn flash_file_with_optional_progress(
    jlink: &AsyncJLink,
    path: PathBuf,
    addr: u32,
    events: &mpsc::Sender<InterfaceEvent>,
) -> JlinkResult<i32> {
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
    let flash = jlink.flash_file_with_progress_events(path, addr, progress_tx);
    tokio::pin!(flash);

    loop {
        tokio::select! {
            result = &mut flash => break result,
            Some(progress) = progress_rx.recv() => {
                let _ = events
                    .send(InterfaceEvent::FlashProgress(Some(TerminalFlashProgress {
                        action: progress.action,
                        percent: progress.percent,
                    })))
                    .await;
            }
        }
    }
}

#[cfg(all(feature = "rtt", not(feature = "control")))]
async fn flash_file_with_optional_progress(
    jlink: &AsyncJLink,
    path: PathBuf,
    addr: u32,
    _events: &mpsc::Sender<InterfaceEvent>,
) -> JlinkResult<i32> {
    jlink.flash_file(path, addr, false).await
}

#[cfg(feature = "rtt")]
pub(crate) async fn wait_for_rtt_stop(
    rx: &mut mpsc::Receiver<InterfaceCommand>,
    delay: Duration,
) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        command = rx.recv() => {
            handle_reconnect_wait_command(command, "rtt transport is reconnecting")
        }
    }
}

#[cfg(feature = "rtt")]
pub(crate) async fn open_rtt_async(
    chip: &str,
    sn: Option<u32>,
    ip_addr: Option<String>,
    explicit_lib: Option<PathBuf>,
    speed: ConnectSpeed,
    rtt_telnet_port: Option<u16>,
) -> Result<AsyncJLink> {
    let (jlink, _) = AsyncJLink::from_default_candidates(explicit_lib.or_else(env_jlink_lib))
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    jlink
        .open(OpenOptions {
            serial_no: sn,
            ip_addr,
        })
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    jlink
        .connect_target(chip, speed, TargetInterface::Swd, false)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    if let Some(port) = rtt_telnet_port {
        jlink
            .set_rtt_telnet_port(port)
            .await
            .map_err(|e| anyhow!(e.to_string()))?;
    }
    jlink
        .rtt_start(None)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(jlink)
}
