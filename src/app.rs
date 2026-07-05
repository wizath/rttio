use crate::*;

#[cfg(feature = "rtt")]
#[cfg(test)]
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
    init_timestamp_epoch();
    set_config_updates_disabled(opts.no_config);

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
    if let Some(Command::Probes(probes)) = &opts.command {
        list_jlinks(probes.jlink_lib.clone(), probes.sn)?;
        return Ok(());
    }

    #[cfg(feature = "rtt")]
    if let Some(Command::Devices(devices)) = &opts.command {
        list_jlink_devices(
            devices.jlink_lib.clone(),
            devices.filter.as_deref().unwrap_or(""),
        )?;
        return Ok(());
    }

    #[cfg(feature = "rtt")]
    if let Some(Command::PickDevice(pick)) = &opts.command {
        println!("{}", pick_jlink_device(pick.jlink_lib.clone())?);
        return Ok(());
    }

    let command = opts.command.as_ref().ok_or_else(|| {
        anyhow!("choose a transport command: rttio rtt <chip> or rttio serial <port>")
    })?;

    let mut startup_logs = Vec::new();
    let config_file_present = Path::new(CONFIG_FILE).exists();
    let config = if opts.no_config {
        startup_logs.push(format!("--no-config: ignoring {CONFIG_FILE}"));
        RttioConfig::default()
    } else {
        match load_config() {
            Ok(config) => {
                if config_file_present {
                    startup_logs.push(format!("loaded config from {CONFIG_FILE}"));
                } else {
                    startup_logs.push(format!("no {CONFIG_FILE}; using defaults"));
                }
                config
            }
            Err(e) => {
                startup_logs.push(format!("failed to load {CONFIG_FILE}: {e}"));
                RttioConfig::default()
            }
        }
    };

    #[cfg(feature = "serial")]
    #[allow(unreachable_patterns)]
    let serial_cmd = match command {
        Command::Serial(serial) => Some(serial),
        _ => None,
    };

    #[cfg(feature = "rtt")]
    let rtt_cmd = match command {
        Command::Rtt(rtt) => Some(rtt),
        _ => None,
    };

    #[cfg(any(feature = "serial", feature = "rtt"))]
    #[allow(unreachable_patterns)]
    let serve_addr = match command {
        #[cfg(feature = "serial")]
        Command::Serial(serial) => serial.serve.clone(),
        #[cfg(feature = "rtt")]
        Command::Rtt(rtt) => rtt.serve.clone(),
        _ => None,
    };

    #[cfg(feature = "rtt")]
    let env_chip = std::env::var("JLINK_CHIP").ok();
    #[cfg(feature = "rtt")]
    let (target_chip, target_chip_source) = if let Some(rtt) = rtt_cmd {
        if let Some(chip) = rtt.chip.clone() {
            (Some(chip), Some("argv"))
        } else if let Some(chip) = env_chip.clone() {
            (Some(chip), Some("JLINK_CHIP"))
        } else if rtt.rtt_tcp_port.is_none() {
            return Err(anyhow!("missing RTT target chip: use rttio rtt <chip>, set JLINK_CHIP, or pass --rtt-port for an RTT stream"));
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    #[cfg(not(feature = "rtt"))]
    let (target_chip, target_chip_source): (Option<String>, Option<&str>) = (None, None);

    #[cfg(feature = "serial")]
    let serial_path = serial_cmd.map(|serial| serial.port.clone());
    #[cfg(not(feature = "serial"))]
    let serial_path: Option<PathBuf> = None;

    #[cfg(feature = "serial")]
    let (baud, baud_source) = if let Some(serial) = serial_cmd {
        if let Some(baud) = serial.baud {
            (baud, "argv")
        } else if let Some(baud) = config.baud {
            (baud, CONFIG_FILE)
        } else {
            (115200, "default")
        }
    } else {
        (115200, "default")
    };
    #[cfg(not(feature = "serial"))]
    let baud = 115200;
    #[cfg(not(feature = "serial"))]
    let _baud_source = "default";

    #[cfg(feature = "rtt")]
    let env_sn = env_jlink_sn();
    #[cfg(feature = "rtt")]
    let (mut jlink_sn, mut jlink_sn_source) = if let Some(rtt) = rtt_cmd {
        if rtt.jlink_ip.is_some() {
            (None, None)
        } else if let Some(sn) = rtt.sn {
            (Some(sn), Some("argv"))
        } else if let Some(sn) = env_sn {
            (Some(sn), Some("JLINK_SN"))
        } else if let Some(sn) = config.jlink_sn {
            (Some(sn), Some(CONFIG_FILE))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    #[cfg(not(feature = "rtt"))]
    let jlink_sn: Option<u32> = None;
    #[cfg(not(feature = "rtt"))]
    let _jlink_sn_source: Option<&str> = None;

    #[cfg(feature = "rtt")]
    let (jlink_ip, jlink_ip_source) = if let Some(rtt) = rtt_cmd {
        if let Some(ip) = rtt.jlink_ip.clone() {
            (Some(ip), Some("argv"))
        } else if let Some(ip) = config.jlink_ip.clone() {
            (Some(ip), Some(CONFIG_FILE))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    #[cfg(not(feature = "rtt"))]
    let jlink_ip: Option<String> = None;
    #[cfg(not(feature = "rtt"))]
    let _jlink_ip_source: Option<&str> = None;

    #[cfg(feature = "rtt")]
    let rtt_tcp_port = rtt_cmd.and_then(|rtt| rtt.rtt_tcp_port);
    #[cfg(not(feature = "rtt"))]
    #[cfg_attr(not(feature = "control"), allow(unused_variables))]
    let rtt_tcp_port: Option<u16> = None;

    #[cfg(feature = "rtt")]
    if target_chip.is_some() && jlink_sn.is_none() && jlink_ip.is_none() && rtt_tcp_port.is_none() {
        let jlink_lib = rtt_cmd.and_then(|rtt| rtt.jlink_lib.clone());
        match pick_default_jlink_sn(jlink_lib) {
            Ok(Some(sn)) => {
                startup_logs.push(format!("using first J-Link SN: {sn}"));
                jlink_sn = Some(sn);
                jlink_sn_source = Some("first J-Link");
            }
            Ok(None) => {}
            Err(e) => startup_logs.push(format!("failed to list J-Link probes for SN: {e}")),
        }
    }

    if let (Some(device), Some(source)) = (&target_chip, target_chip_source) {
        startup_logs.push(format!("using device from {source}: {device}"));
    }
    #[cfg(feature = "rtt")]
    if let (Some(ip), Some(source)) = (&jlink_ip, jlink_ip_source) {
        startup_logs.push(format!("using J-Link IP from {source}: {ip}"));
    }
    #[cfg(feature = "rtt")]
    if let Some(sn) = jlink_sn {
        let source = jlink_sn_source.unwrap_or("unknown");
        startup_logs.push(format!("using J-Link SN from {source}: {sn}"));
    }
    #[cfg(feature = "serial")]
    if let Some(serial) = &serial_path {
        startup_logs.push(format!("using serial from argv: {}", serial.display()));
    }
    #[cfg(feature = "serial")]
    if serial_cmd.and_then(|serial| serial.baud).is_none() {
        startup_logs.push(format!("using baud from {baud_source}: {baud}"));
    }
    let mut persisted_config = config;
    let mut serial_config_saved = opts.no_config;
    let mut rtt_config_saved = opts.no_config;

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
    #[cfg(any(feature = "serial", feature = "rtt"))]
    let (serve_raw_tx, _) = broadcast::channel::<Vec<u8>>(512);
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
            rtt_tcp_port.and_then(|_| rtt_cmd.map(|rtt| rtt.rtt_tcp_host.clone()))
        }
        #[cfg(not(feature = "rtt"))]
        {
            None
        }
    };

    let _terminal = TerminalGuard::enter()?;
    let terminal_join = tokio::spawn(terminal_task(terminal_rx));
    for log in startup_logs {
        terminal_status(&terminal_tx, &log).await;
    }
    terminal_status(
        &terminal_tx,
        "rttio started. Ctrl-T q exits. Ctrl-T opens menu.",
    )
    .await;

    let mut background_tasks = Vec::new();

    #[cfg(any(feature = "serial", feature = "rtt"))]
    if let Some(addr) = serve_addr {
        let serve_join = tokio::spawn(raw_tcp_bridge_server(
            addr,
            input_tx.clone(),
            serve_raw_tx.subscribe(),
            terminal_tx.clone(),
        ));
        background_tasks.push(("serve", serve_join));
    }

    #[cfg(feature = "serial")]
    let mut serial_tx = None;
    #[cfg(not(feature = "serial"))]
    let serial_tx: Option<mpsc::Sender<InterfaceCommand>> = None;
    #[cfg(feature = "serial")]
    if let Some(path) = serial_path {
        let serial = serial_cmd.expect("serial command exists when serial path is set");
        let (tx, rx) = mpsc::channel::<InterfaceCommand>(128);
        serial_tx = Some(tx);
        let serial_join = tokio::spawn(serial_task(
            SerialTaskConfig {
                path,
                baud,
                flow_control: serial.flow_control,
                #[cfg(feature = "espflash")]
                espflash: EspFlashConfig {
                    chip: serial.esp_chip,
                    baud: serial.espflash_baud,
                },
            },
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
        let rtt = rtt_cmd.expect("rtt command exists when RTT TCP port is set");
        let (tx, rx) = mpsc::channel::<InterfaceCommand>(128);
        rtt_tx = Some(tx);
        let rtt_tcp_join = tokio::spawn(rtt_tcp_task(
            rtt.rtt_tcp_host.clone(),
            port,
            rx,
            event_tx.clone(),
            !opts.rtt_reconnect,
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
            rtt_cmd.and_then(|rtt| rtt.jlink_lib.clone()),
            rtt_cmd
                .map(|rtt| rtt.jlink_speed)
                .unwrap_or(ConnectSpeed::Khz(4000)),
            rtt_cmd.and_then(|rtt| rtt.jlink_rtt_port),
            rtt_cmd.map(|rtt| rtt.rtt_up).unwrap_or(0),
            rtt_cmd.map(|rtt| rtt.rtt_down).unwrap_or(0),
            opts.chunk,
            opts.poll_ms,
            !opts.rtt_reconnect,
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
        rtt_tx.is_some() || serial_target_actions_available(serial_tx.is_some()),
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
    let mut any_transport_connected = false;
    let mut last_transport_error: Option<String> = None;
    #[cfg(feature = "control")]
    let mut last_status_bar_update = Instant::now();
    let _ = terminal_tx.try_send(TerminalEvent::SetUiState(TerminalUiState {
        output_mode,
        timestamp,
        local_echo,
        output_paused,
        jlink_actions: rtt_tx.is_some() || serial_target_actions_available(serial_tx.is_some()),
    }));
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
                rtt_cmd.map(|rtt| rtt.rtt_up).unwrap_or(0)
            }
            #[cfg(not(feature = "rtt"))]
            {
                0
            }
        },
        rtt_down: {
            #[cfg(feature = "rtt")]
            {
                rtt_cmd.map(|rtt| rtt.rtt_down).unwrap_or(0)
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
            app_runtime_snapshot(
                route,
                serial_running,
                rtt_running,
                output_mode,
                timestamp,
                local_echo,
                output_paused,
            ),
            0,
            status_target_label(
                route,
                configured_serial_path.as_deref(),
                configured_device.as_deref(),
            ),
        ),
    )
    .await;

    loop {
        tokio::select! {
            biased;
            Some(input) = input_rx.recv() => {
                match input {
                    InputEvent::Bytes(bytes) => {
                        route_write(route, &bytes, &serial_tx, &rtt_tx).await;
                        #[cfg(feature = "control")]
                        let _ = terminal_tx.try_send(TerminalEvent::Activity(Source::Tx));
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
                                let _ = terminal_tx.try_send(TerminalEvent::Output(rendered.clone()));
                            }
                            log_writer.write_record(&terminal_tx, &rendered);
                        }
                    }
                    InputEvent::Line(line) => {
                        let mut payload = line.into_bytes();
                        payload.extend_from_slice(line_ending.bytes());
                        route_write(route, &payload, &serial_tx, &rtt_tx).await;
                        #[cfg(feature = "control")]
                        let _ = terminal_tx.try_send(TerminalEvent::Activity(Source::Tx));
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
                                let _ = terminal_tx.try_send(TerminalEvent::Output(rendered.clone()));
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
                        let snapshot = app_runtime_snapshot(
                            route,
                            serial_running,
                            rtt_running,
                            output_mode,
                            timestamp,
                            local_echo,
                            output_paused,
                        );
                        #[cfg(feature = "control")]
                        update_control_runtime_state(&control_state, snapshot.control()).await;
                        let _ = terminal_tx
                            .try_send(TerminalEvent::SetUiState(TerminalUiState {
                                output_mode,
                                timestamp,
                                local_echo,
                                output_paused,
                                jlink_actions: rtt_tx.is_some()
                                    || serial_target_actions_available(serial_tx.is_some()),
                            }));
                        #[cfg(feature = "control")]
                        send_status_bar(
                            &terminal_tx,
                            build_status_bar(
                                snapshot,
                                control_history_bytes(&control_history).await,
                                status_target_label(
                                    route,
                                    configured_serial_path.as_deref(),
                                    configured_device.as_deref(),
                                ),
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
                                    let _ = terminal_tx.try_send(TerminalEvent::Activity(Source::Tx));
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
                                            .try_send(TerminalEvent::Output(rendered.clone()));
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
                        let _ = terminal_tx.try_send(TerminalEvent::Activity(source));
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
                        #[cfg(any(feature = "serial", feature = "rtt"))]
                        let _ = serve_raw_tx.send(data.clone());
                        if terminal_output_enabled(output_paused, &command_view_active) {
                            let _ = terminal_tx.try_send(TerminalEvent::Output(rendered.clone()));
                        }
                        log_writer.write_record(&terminal_tx, &rendered);
                        #[cfg(feature = "control")]
                        let _ = control_output_tx.send(rendered);
                        #[cfg(feature = "control")]
                        if last_status_bar_update.elapsed() >= Duration::from_millis(100) {
                            let _ = terminal_tx.try_send(TerminalEvent::SetStatusBar(
                                build_status_bar(
                                    app_runtime_snapshot(
                                        route,
                                        serial_running,
                                        rtt_running,
                                        output_mode,
                                        timestamp,
                                        local_echo,
                                        output_paused,
                                    ),
                                    history_bytes,
                                    status_target_label(
                                        route,
                                        configured_serial_path.as_deref(),
                                        configured_device.as_deref(),
                                    ),
                                ),
                            ));
                            last_status_bar_update = Instant::now();
                        }
                    }
                    InterfaceEvent::Status { source, text } => {
                        let mut connection_state_changed = false;
                        if source == Source::Serial && is_serial_connected_status(&text) {
                            if !serial_running {
                                serial_running = true;
                                any_transport_connected = true;
                                connection_state_changed = true;
                            }
                            if !serial_config_saved {
                                persisted_config.target = Some(ConfigTarget::Serial);
                                persisted_config.serial.clone_from(&configured_serial_path);
                                persisted_config.baud = configured_baud;
                                if let Err(e) = save_config_blocking(persisted_config.clone()).await {
                                    let _ = terminal_tx
                                        .try_send(TerminalEvent::Status(format!(
                                            "[rttio] failed to save {CONFIG_FILE}: {e}\n"
                                        )));
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
                                any_transport_connected = true;
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
                                    if let Err(e) = save_config_blocking(persisted_config.clone()).await {
                                        let _ = terminal_tx
                                            .try_send(TerminalEvent::Status(format!(
                                                "[rttio] failed to save {CONFIG_FILE}: {e}\n"
                                            )));
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
                            let snapshot = app_runtime_snapshot(
                                route,
                                serial_running,
                                rtt_running,
                                output_mode,
                                timestamp,
                                local_echo,
                                output_paused,
                            );
                            #[cfg(feature = "control")]
                            update_control_runtime_state(&control_state, snapshot.control()).await;
                            #[cfg(feature = "control")]
                            send_status_bar(
                                &terminal_tx,
                                build_status_bar(
                                    snapshot,
                                    control_history_bytes(&control_history).await,
                                    status_target_label(
                                        route,
                                        configured_serial_path.as_deref(),
                                        configured_device.as_deref(),
                                    ),
                                ),
                            ).await;
                        }
                        let rendered = format!("[rttio] {}: {}\n", source.label(), text);
                        let _ = terminal_tx
                            .send(TerminalEvent::Status(rendered.clone()))
                            .await;
                        #[cfg(feature = "control")]
                        let _ = control_output_tx.send(rendered);
                    }
                    InterfaceEvent::Error { source, text } => {
                        let rendered = format!("[rttio] {} error: {}\n", source.label(), text);
                        last_transport_error = Some(rendered.trim_end().to_string());
                        let _ = terminal_tx
                            .send(TerminalEvent::Status(rendered.clone()))
                            .await;
                        #[cfg(feature = "control")]
                        let _ = control_output_tx.send(rendered);
                    }
                    #[cfg(feature = "control")]
                    InterfaceEvent::FlashProgress(progress) => {
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
                            app_runtime_snapshot(
                                route,
                                serial_running,
                                rtt_running,
                                output_mode,
                                timestamp,
                                local_echo,
                                output_paused,
                            ).control(),
                        ).await;
                        let rendered = format!("[rttio] {} stopped\n", source.label());
                        let _ = terminal_tx.try_send(TerminalEvent::Status(rendered.clone()));
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
        let _ = tx.try_send(InterfaceCommand::Stop);
    }
    if let Some(tx) = &rtt_tx {
        let _ = tx.try_send(InterfaceCommand::Stop);
    }
    input_task.request_stop();
    for (_, handle) in background_tasks {
        handle.abort();
    }
    #[cfg(feature = "control")]
    control_join.abort();
    let _ = input_task.join_if_finished();

    log_writer.flush_or_disable(&terminal_tx);
    let _ = terminal_tx.try_send(TerminalEvent::Exit);
    terminal_join.abort();
    let _ = terminal_join.await;
    drop(_terminal);
    let _ = write!(io::stdout(), "\r\x1b[2K[rttio] rttio stopped\r\n");
    let _ = io::stdout().flush();
    if !any_transport_connected {
        if let Some(error) = last_transport_error {
            eprintln!("{error}");
        }
    }
    Ok(())
}

#[cfg(feature = "control")]
async fn control_history_bytes(history: &Arc<Mutex<ControlHistory>>) -> usize {
    history.lock().await.bytes()
}

fn terminal_output_enabled(output_paused: bool, command_view_active: &Arc<AtomicBool>) -> bool {
    !output_paused && !command_view_active.load(Ordering::Acquire)
}

fn serial_target_actions_available(serial_running: bool) -> bool {
    #[cfg(feature = "espflash")]
    {
        serial_running
    }
    #[cfg(not(feature = "espflash"))]
    {
        let _ = serial_running;
        false
    }
}

#[cfg(feature = "control")]
async fn send_status_bar(terminal_tx: &mpsc::Sender<TerminalEvent>, status_bar: TerminalStatusBar) {
    let _ = terminal_tx.try_send(TerminalEvent::SetStatusBar(status_bar));
}

#[cfg(feature = "control")]
#[derive(Clone, Copy)]
struct AppRuntimeSnapshot {
    route: Route,
    serial_running: bool,
    rtt_running: bool,
    output_mode: OutputMode,
    timestamp: bool,
    local_echo: bool,
    output_paused: bool,
}

#[cfg(feature = "control")]
impl AppRuntimeSnapshot {
    fn control(self) -> ControlRuntimeSnapshot {
        ControlRuntimeSnapshot {
            serial_running: self.serial_running,
            rtt_running: self.rtt_running,
            route: self.route,
            output_mode: self.output_mode,
            timestamp: self.timestamp,
            local_echo: self.local_echo,
            output_paused: self.output_paused,
        }
    }
}

#[cfg(feature = "control")]
fn app_runtime_snapshot(
    route: Route,
    serial_running: bool,
    rtt_running: bool,
    output_mode: OutputMode,
    timestamp: bool,
    local_echo: bool,
    output_paused: bool,
) -> AppRuntimeSnapshot {
    AppRuntimeSnapshot {
        route,
        serial_running,
        rtt_running,
        output_mode,
        timestamp,
        local_echo,
        output_paused,
    }
}

#[cfg(feature = "control")]
fn build_status_bar(
    snapshot: AppRuntimeSnapshot,
    history_bytes: usize,
    target_label: String,
) -> TerminalStatusBar {
    let target = match snapshot.route {
        Route::Serial => "serial",
        Route::Rtt => "rtt",
        Route::Both => "both",
    };
    TerminalStatusBar {
        target,
        target_label,
        serial_running: snapshot.serial_running,
        rtt_running: snapshot.rtt_running,
        output_mode: snapshot.output_mode,
        timestamp: snapshot.timestamp,
        local_echo: snapshot.local_echo,
        output_paused: snapshot.output_paused,
        history_bytes,
        history_max_bytes: CONTROL_HISTORY_MAX_BYTES,
    }
}

#[cfg(feature = "control")]
fn status_target_label(route: Route, serial_path: Option<&Path>, device: Option<&str>) -> String {
    match route {
        Route::Serial => serial_path
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("serial")
            .to_string(),
        Route::Rtt => device.unwrap_or("rtt").to_string(),
        Route::Both => "both".to_string(),
    }
}
