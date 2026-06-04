use crate::*;

#[cfg(feature = "rtt")]
pub(crate) fn select_jlink_sn(
    serial_requested: bool,
    explicit_jlink_ip: bool,
    opts_sn: Option<u32>,
    env_sn: Option<u32>,
    config_sn: Option<u32>,
) -> Option<u32> {
    if serial_requested || explicit_jlink_ip {
        None
    } else {
        opts_sn.or(env_sn).or(config_sn)
    }
}

pub(crate) async fn run_app(opts: Opts) -> Result<()> {
    let config = match load_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("[rttio] failed to load {CONFIG_FILE}: {e}");
            RttioConfig::default()
        }
    };

    #[cfg(feature = "rtt")]
    if opts.list {
        list_jlinks(opts.jlink_lib, opts.sn)?;
        return Ok(());
    }

    #[cfg(feature = "rtt")]
    if let Some(filter) = &opts.devices {
        list_jlink_devices(opts.jlink_lib, filter)?;
        return Ok(());
    }

    #[cfg(feature = "control")]
    if let Some(Command::Ctl(ctl)) = &opts.command {
        if let CtlCommand::Commands { json } = &ctl.command {
            if *json {
                print!("{}", collect_control_commands_json());
            } else {
                println!("{CONTROL_COMMANDS_HELP}");
            }
            return Ok(());
        }
        let socket = discover_control_socket(ctl.socket.as_deref())?;
        let command = ctl_command_to_wire(&ctl.command)?;
        control_client(
            &socket,
            &command,
            ctl_command_response_timeout(&ctl.command),
        )
        .await?;
        return Ok(());
    }

    #[cfg(feature = "rtt")]
    let env_chip = std::env::var("JLINK_CHIP").ok();
    #[cfg(all(feature = "serial", feature = "rtt"))]
    let serial_requested = opts.serial.is_some();
    #[cfg(all(feature = "rtt", not(feature = "serial")))]
    let serial_requested = false;
    #[cfg(all(feature = "serial", feature = "rtt"))]
    if serial_requested && opts.pick_device {
        return Err(anyhow!(
            "--pick-device selects an RTT target; do not combine it with --serial"
        ));
    }
    #[cfg(feature = "rtt")]
    let rtt_requested = opts.target_chip.is_some()
        || opts.rtt_tcp_port.is_some()
        || opts.sn.is_some()
        || opts.jlink_ip.is_some()
        || opts.jlink_lib.is_some()
        || opts.pick_device;
    #[cfg(not(feature = "rtt"))]
    let rtt_requested = false;
    #[cfg(feature = "rtt")]
    let picked_chip = if opts.pick_device && !serial_requested {
        Some(pick_jlink_device(opts.jlink_lib.clone())?)
    } else {
        None
    };
    #[cfg(all(feature = "serial", feature = "rtt"))]
    let config_prefers_serial = !rtt_requested
        && (config.target == Some(ConfigTarget::Serial)
            || (config.target.is_none() && config.device.is_none()));
    #[cfg(all(feature = "rtt", not(feature = "serial")))]
    let config_prefers_serial = false;
    #[cfg(feature = "rtt")]
    let (target_chip, target_chip_source) = if serial_requested || config_prefers_serial {
        (None, None)
    } else if let Some(chip) = opts.target_chip.clone() {
        (Some(chip), Some("argv"))
    } else if let Some(chip) = picked_chip {
        (Some(chip), Some("SEGGER device picker"))
    } else if let Some(chip) = env_chip.clone() {
        (Some(chip), Some("JLINK_CHIP"))
    } else if !rtt_requested {
        if let Some(chip) = config.device.clone() {
            (Some(chip), Some(CONFIG_FILE))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    #[cfg(not(feature = "rtt"))]
    let (target_chip, target_chip_source): (Option<String>, Option<&str>) = (None, None);
    #[cfg(feature = "serial")]
    let serial_path = if rtt_requested || target_chip.is_some() {
        None
    } else if let Some(serial) = opts.serial.clone() {
        Some(serial)
    } else {
        config.serial.clone()
    };
    #[cfg(not(feature = "serial"))]
    let serial_path: Option<PathBuf> = None;
    #[cfg(feature = "serial")]
    let baud = opts.baud.or(config.baud).unwrap_or(115200);
    #[cfg(not(feature = "serial"))]
    let baud = 115200;
    #[cfg(feature = "rtt")]
    let env_sn = env_jlink_sn();
    #[cfg(feature = "rtt")]
    let mut jlink_sn = select_jlink_sn(
        serial_requested,
        opts.jlink_ip.is_some(),
        opts.sn,
        env_sn,
        config.jlink_sn,
    );
    #[cfg(not(feature = "rtt"))]
    let jlink_sn: Option<u32> = None;
    #[cfg(feature = "rtt")]
    let jlink_ip = if serial_requested {
        None
    } else {
        opts.jlink_ip.clone().or(config.jlink_ip.clone())
    };
    #[cfg(not(feature = "rtt"))]
    let jlink_ip: Option<String> = None;
    #[cfg(feature = "rtt")]
    let rtt_tcp_port = if serial_requested {
        None
    } else {
        opts.rtt_tcp_port
    };
    #[cfg(not(feature = "rtt"))]
    let rtt_tcp_port: Option<u16> = None;

    if serial_path.is_none() && target_chip.is_none() && rtt_tcp_port.is_none() {
        return Err(anyhow!(
            "nothing to connect: pass --serial <port>, --rtt-port <port>, a J-Link target chip, or JLINK_CHIP"
        ));
    }
    if serial_path.is_some() && (target_chip.is_some() || rtt_tcp_port.is_some()) {
        return Err(anyhow!(
            "choose exactly one transport: pass serial or RTT, not both"
        ));
    }
    #[cfg(feature = "rtt")]
    if target_chip.is_some() && jlink_sn.is_none() && jlink_ip.is_none() && rtt_tcp_port.is_none() {
        match pick_default_jlink_sn(opts.jlink_lib.clone()) {
            Ok(Some(sn)) => {
                eprintln!("[rttio] using first J-Link SN: {sn}");
                jlink_sn = Some(sn);
            }
            Ok(None) => {}
            Err(e) => eprintln!("[rttio] failed to list J-Link probes for SN: {e}"),
        }
    }

    if let (Some(device), Some(source)) = (&target_chip, target_chip_source) {
        eprintln!("[rttio] using device from {source}: {device}");
    }
    #[cfg(feature = "serial")]
    if opts.serial.is_none() {
        if let Some(serial) = &serial_path {
            eprintln!(
                "[rttio] using serial from {CONFIG_FILE}: {}",
                serial.display()
            );
        }
    }
    #[cfg(feature = "serial")]
    if opts.baud.is_none() {
        eprintln!("[rttio] using baud from {CONFIG_FILE}/default: {baud}");
    }
    let mut persisted_config = config;
    let mut serial_config_saved = false;
    let mut rtt_config_saved = false;

    let mut log_writer = if let Some(path) = &opts.log_file {
        LogWriter::open(path, opts.log_append)?
    } else {
        LogWriter::disabled()
    };

    let (event_tx, mut event_rx) = mpsc::channel::<InterfaceEvent>(256);
    let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(64);
    let (terminal_tx, terminal_rx) = mpsc::channel::<TerminalEvent>(512);
    #[cfg(feature = "control")]
    let (control_output_tx, _) = broadcast::channel::<String>(512);
    #[cfg(feature = "control")]
    let (control_raw_tx, _) = broadcast::channel::<ControlOutput>(512);
    #[cfg(feature = "control")]
    let control_history = Arc::new(Mutex::new(ControlHistory::new(CONTROL_HISTORY_MAX_BYTES)));
    let line_ending = opts.line_ending;
    let configured_serial_path = serial_path.clone();
    let configured_baud = serial_path.as_ref().map(|_| baud);
    let configured_device = target_chip.clone();
    #[cfg(feature = "control")]
    let control_rtt_tcp_host = {
        #[cfg(feature = "rtt")]
        {
            rtt_tcp_port.map(|_| opts.rtt_tcp_host.clone())
        }
        #[cfg(not(feature = "rtt"))]
        {
            None
        }
    };

    let _terminal = TerminalGuard::enter()?;
    let terminal_join = tokio::spawn(terminal_task(terminal_rx));
    terminal_status(
        &terminal_tx,
        "rttio started. Ctrl-T q exits. Ctrl-T opens menu.",
    )
    .await;

    let mut background_tasks = Vec::new();

    #[cfg(feature = "serial")]
    let mut serial_tx = None;
    #[cfg(not(feature = "serial"))]
    let serial_tx: Option<mpsc::Sender<InterfaceCommand>> = None;
    #[cfg(feature = "serial")]
    if let Some(path) = serial_path {
        let (tx, rx) = mpsc::channel::<InterfaceCommand>(128);
        serial_tx = Some(tx);
        let serial_join = tokio::spawn(serial_task(
            path,
            baud,
            opts.serial_flow_control,
            rx,
            event_tx.clone(),
            opts.no_reconnect,
            Duration::from_millis(opts.reconnect_delay_ms),
        ));
        background_tasks.push(("serial", serial_join));
    }

    #[cfg(feature = "rtt")]
    let mut rtt_tx = None;
    #[cfg(not(feature = "rtt"))]
    let rtt_tx: Option<mpsc::Sender<InterfaceCommand>> = None;
    #[cfg(feature = "rtt")]
    if let Some(port) = rtt_tcp_port {
        let (tx, rx) = mpsc::channel::<InterfaceCommand>(128);
        rtt_tx = Some(tx);
        let rtt_tcp_join = tokio::spawn(rtt_tcp_task(
            opts.rtt_tcp_host.clone(),
            port,
            rx,
            event_tx.clone(),
            opts.no_reconnect,
            Duration::from_millis(opts.reconnect_delay_ms),
        ));
        background_tasks.push(("rtt tcp", rtt_tcp_join));
    } else if let Some(chip) = target_chip {
        let (tx, rx) = mpsc::channel::<InterfaceCommand>(128);
        rtt_tx = Some(tx);
        let rtt_join = tokio::spawn(rtt_task(
            chip,
            jlink_sn,
            jlink_ip.clone(),
            opts.jlink_lib.clone(),
            opts.jlink_speed,
            opts.jlink_rtt_port,
            opts.rtt_up,
            opts.rtt_down,
            opts.chunk,
            opts.poll_ms,
            opts.no_reconnect,
            Duration::from_millis(opts.reconnect_delay_ms),
            rx,
            event_tx.clone(),
        ));
        background_tasks.push(("rtt", rtt_join));
    }

    let command_view_active = Arc::new(AtomicBool::new(false));
    let input_task = spawn_input_task(
        input_tx.clone(),
        terminal_tx.clone(),
        rtt_tx.is_some(),
        Arc::clone(&command_view_active),
    );

    let route = default_route(serial_tx.is_some(), rtt_tx.is_some());
    let mut output_mode = opts.output_mode;
    let mut timestamp = opts.timestamp;
    let mut local_echo = opts.local_echo || rtt_tx.is_some();
    let mut output_paused = false;
    let prefix = false;
    let mut output_line_state = OutputLineState::new();
    let serial_configured = serial_tx.is_some();
    let rtt_configured = rtt_tx.is_some();
    let mut serial_task_active = serial_configured;
    let mut rtt_task_active = rtt_configured;
    let mut serial_running = false;
    let mut rtt_running = false;
    let _ = terminal_tx
        .send(TerminalEvent::SetUiState(TerminalUiState {
            output_mode,
            timestamp,
            local_echo,
            output_paused,
            jlink_actions: rtt_tx.is_some(),
        }))
        .await;
    #[cfg(feature = "control")]
    let control_state = Arc::new(Mutex::new(ControlRuntimeState {
        control_socket: opts.socket.clone(),
        serial_configured,
        rtt_configured,
        serial_path: configured_serial_path.clone(),
        baud: configured_baud,
        jlink_sn,
        jlink_ip: jlink_ip.clone(),
        device: configured_device.clone(),
        rtt_tcp_host: control_rtt_tcp_host,
        rtt_tcp_port,
        rtt_up: {
            #[cfg(feature = "rtt")]
            {
                opts.rtt_up
            }
            #[cfg(not(feature = "rtt"))]
            {
                0
            }
        },
        rtt_down: {
            #[cfg(feature = "rtt")]
            {
                opts.rtt_down
            }
            #[cfg(not(feature = "rtt"))]
            {
                0
            }
        },
        serial_running,
        rtt_running,
        route,
        output_mode,
        timestamp,
        local_echo,
        output_paused,
        line_ending,
    }));

    #[cfg(feature = "control")]
    let control_join = tokio::spawn(control_server(
        input_tx.clone(),
        ControlServerContext {
            path: opts.socket.clone(),
            terminal_tx: terminal_tx.clone(),
            output_tx: control_output_tx.clone(),
            raw_tx: control_raw_tx.clone(),
            history: Arc::clone(&control_history),
            state: Arc::clone(&control_state),
            line_ending,
        },
    ));

    #[cfg(feature = "control")]
    send_status_bar(
        &terminal_tx,
        build_status_bar(
            route,
            serial_running,
            rtt_running,
            output_mode,
            timestamp,
            local_echo,
            output_paused,
            0,
        ),
    )
    .await;

    loop {
        tokio::select! {
            Some(input) = input_rx.recv() => {
                match input {
                    InputEvent::Bytes(bytes) => {
                        route_write(route, &bytes, &serial_tx, &rtt_tx).await;
                        #[cfg(feature = "control")]
                        let _ = terminal_tx.send(TerminalEvent::Activity(Source::Tx)).await;
                        if local_echo {
                            let rendered = render_data(
                                Source::Tx,
                                &bytes,
                                output_mode,
                                timestamp,
                                false,
                                &mut output_line_state,
                            );
                            if terminal_output_enabled(output_paused, &command_view_active) {
                                let _ = terminal_tx.send(TerminalEvent::Output(rendered.clone())).await;
                            }
                            log_writer.write_record(&terminal_tx, &rendered);
                        }
                    }
                    InputEvent::Line(line) => {
                        let mut payload = line.into_bytes();
                        payload.extend_from_slice(line_ending.bytes());
                        route_write(route, &payload, &serial_tx, &rtt_tx).await;
                        #[cfg(feature = "control")]
                        let _ = terminal_tx.send(TerminalEvent::Activity(Source::Tx)).await;
                        if local_echo {
                            let rendered = render_data(
                                Source::Tx,
                                &payload,
                                output_mode,
                                timestamp,
                                false,
                                &mut output_line_state,
                            );
                            if terminal_output_enabled(output_paused, &command_view_active) {
                                let _ = terminal_tx.send(TerminalEvent::Output(rendered.clone())).await;
                            }
                            log_writer.write_record(&terminal_tx, &rendered);
                        }
                    }
                    InputEvent::MenuCommand(command) => {
                        handle_menu_command(
                            command,
                            MenuCommandContext {
                                output_mode: &mut output_mode,
                                timestamp: &mut timestamp,
                                local_echo: &mut local_echo,
                                output_paused: &mut output_paused,
                                serial_tx: &serial_tx,
                                rtt_tx: &rtt_tx,
                                terminal_tx: &terminal_tx,
                                #[cfg(feature = "control")]
                                control_history: Some(&control_history),
                            },
                        ).await;
                        #[cfg(feature = "control")]
                        update_control_runtime_state(
                            &control_state,
                            serial_running,
                            rtt_running,
                            route,
                            output_mode,
                            timestamp,
                            local_echo,
                            output_paused,
                        ).await;
                        let _ = terminal_tx
                            .send(TerminalEvent::SetUiState(TerminalUiState {
                                output_mode,
                                timestamp,
                                local_echo,
                                output_paused,
                                jlink_actions: rtt_tx.is_some(),
                            }))
                            .await;
                        #[cfg(feature = "control")]
                        send_status_bar(
                            &terminal_tx,
                            build_status_bar(
                                route,
                                serial_running,
                                rtt_running,
                                output_mode,
                                timestamp,
                                local_echo,
                                output_paused,
                                control_history_bytes(&control_history).await,
                            ),
                        ).await;
                    }
                    #[cfg(feature = "control")]
                    InputEvent::Control(request) => {
                        match request {
                            ControlRequest::Write {
                                target,
                                bytes,
                                timeout,
                                reply,
                            } => {
                                let response =
                                    route_write_control(target, route, &bytes, timeout, &serial_tx, &rtt_tx)
                                        .await;
                                let write_ok = response.trim_start().starts_with("OK");
                                let _ = reply.send(response);
                                if write_ok {
                                    let _ = terminal_tx.send(TerminalEvent::Activity(Source::Tx)).await;
                                    let rendered = render_data(
                                        Source::Tx,
                                        &bytes,
                                        output_mode,
                                        timestamp,
                                        false,
                                        &mut output_line_state,
                                    );
                                    if terminal_output_enabled(output_paused, &command_view_active) {
                                        let _ = terminal_tx
                                            .send(TerminalEvent::Output(rendered.clone()))
                                            .await;
                                    }
                                    log_writer.write_record(&terminal_tx, &rendered);
                                    let _ = control_output_tx.send(rendered);
                                }
                            }
                            request => {
                                handle_control_request(
                                    request,
                                    route,
                                    &serial_tx,
                                    &rtt_tx,
                                ).await;
                            }
                        }
                    }
                    InputEvent::Quit => break,
                }
            }
            Some(event) = event_rx.recv() => {
                match event {
                    InterfaceEvent::Data { source, data } => {
                        #[cfg(feature = "control")]
                        let _ = terminal_tx.send(TerminalEvent::Activity(source)).await;
                        let rendered = render_data(
                            source,
                            &data,
                            output_mode,
                            timestamp,
                            prefix,
                            &mut output_line_state,
                        );
                        #[cfg(feature = "control")]
                        let (seq, history_bytes) = {
                            let mut history = control_history.lock().await;
                            let seq = history.push(source, data.clone());
                            (seq, history.bytes())
                        };
                        #[cfg(feature = "control")]
                        let _ = control_raw_tx.send(ControlOutput {
                            seq,
                            source,
                            data: data.clone(),
                        });
                        if terminal_output_enabled(output_paused, &command_view_active) {
                            let _ = terminal_tx.send(TerminalEvent::Output(rendered.clone())).await;
                        }
                        log_writer.write_record(&terminal_tx, &rendered);
                        #[cfg(feature = "control")]
                        let _ = control_output_tx.send(rendered);
                        #[cfg(feature = "control")]
                        send_status_bar(
                            &terminal_tx,
                            build_status_bar(
                                route,
                                serial_running,
                                rtt_running,
                                output_mode,
                                timestamp,
                                local_echo,
                                output_paused,
                                history_bytes,
                            ),
                        ).await;
                    }
                    InterfaceEvent::Status { source, text } => {
                        let mut connection_state_changed = false;
                        if source == Source::Serial && is_serial_connected_status(&text) {
                            if !serial_running {
                                serial_running = true;
                                connection_state_changed = true;
                            }
                            if !serial_config_saved {
                                persisted_config.target = Some(ConfigTarget::Serial);
                                persisted_config.serial.clone_from(&configured_serial_path);
                                persisted_config.baud = configured_baud;
                                if let Err(e) = save_config(&persisted_config) {
                                    let _ = terminal_tx
                                        .send(TerminalEvent::Status(format!(
                                            "[rttio] failed to save {CONFIG_FILE}: {e}\n"
                                        )))
                                        .await;
                                }
                                serial_config_saved = true;
                            }
                        }
                        if source == Source::Serial
                            && is_disconnected_status(&text)
                            && serial_running
                        {
                            serial_running = false;
                            connection_state_changed = true;
                        }
                        if source == Source::Rtt && is_rtt_connected_status(&text) {
                            if !rtt_running {
                                rtt_running = true;
                                connection_state_changed = true;
                            }
                            if !rtt_config_saved {
                                if configured_device.is_some()
                                    || jlink_sn.is_some()
                                    || jlink_ip.is_some()
                                {
                                    persisted_config.target = Some(ConfigTarget::Rtt);
                                    persisted_config.device.clone_from(&configured_device);
                                    persisted_config.jlink_sn = jlink_sn;
                                    persisted_config.jlink_ip.clone_from(&jlink_ip);
                                    if let Err(e) = save_config(&persisted_config) {
                                        let _ = terminal_tx
                                            .send(TerminalEvent::Status(format!(
                                                "[rttio] failed to save {CONFIG_FILE}: {e}\n"
                                            )))
                                            .await;
                                    }
                                }
                                rtt_config_saved = true;
                            }
                        }
                        if source == Source::Rtt && is_disconnected_status(&text) && rtt_running {
                            rtt_running = false;
                            connection_state_changed = true;
                        }
                        if connection_state_changed {
                            #[cfg(feature = "control")]
                            update_control_runtime_state(
                                &control_state,
                                serial_running,
                                rtt_running,
                                route,
                                output_mode,
                                timestamp,
                                local_echo,
                                output_paused,
                            ).await;
                            #[cfg(feature = "control")]
                            send_status_bar(
                                &terminal_tx,
                                build_status_bar(
                                    route,
                                    serial_running,
                                    rtt_running,
                                    output_mode,
                                    timestamp,
                                    local_echo,
                                    output_paused,
                                    control_history_bytes(&control_history).await,
                                ),
                            ).await;
                        }
                        let rendered = format!("[rttio] {}: {}\n", source.label(), text);
                        let _ = terminal_tx.send(TerminalEvent::Status(rendered.clone())).await;
                        #[cfg(feature = "control")]
                        let _ = control_output_tx.send(rendered);
                    }
                    InterfaceEvent::Error { source, text } => {
                        let rendered = format!("[rttio] {} error: {}\n", source.label(), text);
                        let _ = terminal_tx.send(TerminalEvent::Status(rendered.clone())).await;
                        #[cfg(feature = "control")]
                        let _ = control_output_tx.send(rendered);
                    }
                    #[cfg(all(feature = "rtt", feature = "control"))]
                    InterfaceEvent::FlashProgress(progress) => {
                        let progress = progress.map(|progress| TerminalFlashProgress {
                            action: progress.action,
                            percent: progress.percent,
                        });
                        let _ = terminal_tx
                            .send(TerminalEvent::SetFlashProgress(progress))
                            .await;
                    }
                    InterfaceEvent::Stopped(source) => {
                        match source {
                            Source::Serial => {
                                serial_task_active = false;
                                serial_running = false;
                            }
                            Source::Rtt => {
                                rtt_task_active = false;
                                rtt_running = false;
                            }
                            Source::Tx => {}
                        }
                        #[cfg(feature = "control")]
                        update_control_runtime_state(
                            &control_state,
                            serial_running,
                            rtt_running,
                            route,
                            output_mode,
                            timestamp,
                            local_echo,
                            output_paused,
                        ).await;
                        let rendered = format!("[rttio] {} stopped\n", source.label());
                        let _ = terminal_tx.send(TerminalEvent::Status(rendered.clone())).await;
                        #[cfg(feature = "control")]
                        let _ = control_output_tx.send(rendered);
                        if !serial_task_active && !rtt_task_active {
                            break;
                        }
                    }
                }
            }
            else => break,
        }
    }

    if let Some(tx) = &serial_tx {
        let _ = tx.send(InterfaceCommand::Stop).await;
    }
    if let Some(tx) = &rtt_tx {
        let _ = tx.send(InterfaceCommand::Stop).await;
    }
    input_task.request_stop();
    for (name, handle) in background_tasks {
        join_task_or_abort(name, handle, Duration::from_millis(750), &terminal_tx).await;
    }
    #[cfg(feature = "control")]
    abort_task("control socket", control_join, &terminal_tx).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    if !input_task.join_if_finished() {
        terminal_status(&terminal_tx, "input thread still active during shutdown").await;
    }

    terminal_status(&terminal_tx, "rttio stopped").await;
    log_writer.flush_or_disable(&terminal_tx);
    let _ = terminal_tx.send(TerminalEvent::Exit).await;
    let _ = terminal_join.await;
    Ok(())
}

#[cfg(feature = "control")]
async fn control_history_bytes(history: &Arc<Mutex<ControlHistory>>) -> usize {
    history.lock().await.bytes()
}

fn terminal_output_enabled(output_paused: bool, command_view_active: &Arc<AtomicBool>) -> bool {
    !output_paused && !command_view_active.load(Ordering::Acquire)
}

#[cfg(feature = "control")]
async fn send_status_bar(terminal_tx: &mpsc::Sender<TerminalEvent>, status_bar: TerminalStatusBar) {
    let _ = terminal_tx
        .send(TerminalEvent::SetStatusBar(status_bar))
        .await;
}

#[cfg(feature = "control")]
fn build_status_bar(
    route: Route,
    serial_running: bool,
    rtt_running: bool,
    output_mode: OutputMode,
    timestamp: bool,
    local_echo: bool,
    output_paused: bool,
    history_bytes: usize,
) -> TerminalStatusBar {
    let target = match route {
        Route::Serial => "serial",
        Route::Rtt => "rtt",
        Route::Both => "both",
    };
    TerminalStatusBar {
        target,
        serial_running,
        rtt_running,
        output_mode,
        timestamp,
        local_echo,
        output_paused,
        history_bytes,
        history_max_bytes: CONTROL_HISTORY_MAX_BYTES,
    }
}
