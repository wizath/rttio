use crate::*;

#[cfg(feature = "serial")]
pub(crate) async fn serial_task(
    path: PathBuf,
    baud: u32,
    flow_control: SerialFlowControl,
    mut rx: mpsc::Receiver<InterfaceCommand>,
    events: mpsc::Sender<InterfaceEvent>,
    no_reconnect: bool,
    reconnect_delay: Duration,
) {
    let path_text = path.display().to_string();
    let mut opening_announced = false;
    let mut last_open_problem: Option<String> = None;
    let mut reconnect_reply: Option<ControlReply> = None;

    loop {
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
                Ok(port) => {
                    last_open_problem = None;
                    let _ = events
                        .send(InterfaceEvent::Status {
                            source: Source::Serial,
                            text: "connected".to_string(),
                        })
                        .await;
                    send_optional_control_reply(reconnect_reply.take(), "OK reconnect\n");
                    let (mut reader, mut writer) = tokio::io::split(port);
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
                                        if let Err(e) = writer.write_all(&data).await {
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
                                    #[cfg(feature = "rtt")]
                                    Some(InterfaceCommand::Reset { reply }) => {
                                        send_optional_control_reply(
                                            reply,
                                            "ERR reset is only available for RTT/J-Link\n",
                                        );
                                        let _ = events.send(InterfaceEvent::Status {
                                            source: Source::Serial,
                                            text: "reset is only available for RTT/J-Link".to_string(),
                                        }).await;
                                    }
                                    #[cfg(feature = "rtt")]
                                    Some(InterfaceCommand::Flash { reply, .. }) => {
                                        send_optional_control_reply(
                                            reply,
                                            "ERR flash is only available for RTT/J-Link\n",
                                        );
                                        let _ = events.send(InterfaceEvent::Status {
                                            source: Source::Serial,
                                            text: "flash is only available for RTT/J-Link".to_string(),
                                        }).await;
                                    }
                                    #[cfg(all(feature = "rtt", feature = "control"))]
                                    Some(InterfaceCommand::Erase { reply }) => {
                                        send_optional_control_reply(
                                            reply,
                                            "ERR erase is only available for RTT/J-Link\n",
                                        );
                                        let _ = events.send(InterfaceEvent::Status {
                                            source: Source::Serial,
                                            text: "erase is only available for RTT/J-Link".to_string(),
                                        }).await;
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

        if no_reconnect {
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
                                    if let Err(e) = writer.write_all(&data).await {
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
                                #[cfg(feature = "control")]
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

                loop {
                    match rx.try_recv() {
                        Ok(InterfaceCommand::Write { data, reply }) => {
                            match jlink.rtt_write(down_channel, data).await {
                                Ok(bytes) => {
                                    send_optional_control_reply(
                                        reply,
                                        format!("OK rtt write {bytes} bytes\n"),
                                    );
                                }
                                Err(e) => {
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
                                    break;
                                }
                            }
                        }
                        Ok(InterfaceCommand::Reconnect { reply }) => {
                            reconnect_reply = reply;
                            reset_after_reconnect = true;
                            break;
                        }
                        Ok(InterfaceCommand::Reset { reply }) => {
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
                        Ok(InterfaceCommand::Flash { path, addr, reply }) => {
                            let _ = events
                                .send(InterfaceEvent::Status {
                                    source: Source::Rtt,
                                    text: format!("flashing {} at 0x{addr:08x}", path.display()),
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
                                    if let Err(e) = remember_flash_file(&path) {
                                        let _ = events
                                            .send(InterfaceEvent::Error {
                                                source: Source::Rtt,
                                                text: format!("failed to remember flash file: {e}"),
                                            })
                                            .await;
                                    }
                                    let text = format!("flash done, J-Link reported {bytes} bytes");
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
                                                    text: "target reset after flash".to_string(),
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
                                    break;
                                }
                            }
                        }
                        #[cfg(feature = "control")]
                        Ok(InterfaceCommand::Erase { reply }) => {
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
                                    break;
                                }
                            }
                        }
                        Ok(InterfaceCommand::Stop)
                        | Err(mpsc::error::TryRecvError::Disconnected) => {
                            let _ = jlink.rtt_stop().await;
                            let _ = jlink.close().await;
                            let _ = jlink.shutdown().await;
                            let _ = events.send(InterfaceEvent::Stopped(Source::Rtt)).await;
                            return;
                        }
                        Err(mpsc::error::TryRecvError::Empty) => {}
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
                    .send(InterfaceEvent::FlashProgress(Some(progress)))
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
