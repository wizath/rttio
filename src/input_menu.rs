use crate::*;

pub(crate) struct InputTask {
    running: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
}

impl InputTask {
    pub(crate) fn request_stop(&self) {
        self.running.store(false, Ordering::Release);
    }

    pub(crate) fn join_if_finished(self) -> bool {
        if !self.handle.is_finished() {
            return false;
        }
        self.handle.join().is_ok()
    }
}

pub(crate) fn spawn_input_task(
    tx: mpsc::Sender<InputEvent>,
    terminal_tx: mpsc::Sender<TerminalEvent>,
    jlink_actions: bool,
    command_view_active: Arc<AtomicBool>,
) -> InputTask {
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = Arc::clone(&running);
    let handle = thread::spawn(move || {
        let mut menu_mode = false;
        let mut help_mode = false;
        let mut menu_selected = 0usize;
        while thread_running.load(Ordering::Acquire) {
            if event::poll(Duration::from_millis(100)).ok() != Some(true) {
                continue;
            }
            let key = match event::read() {
                Ok(Event::Key(key)) => key,
                Ok(Event::Resize(_, _)) => {
                    if menu_mode {
                        if help_mode {
                            let _ = try_terminal_send(&terminal_tx, TerminalEvent::ShowHelp);
                        } else {
                            draw_ctrl_t_menu(&terminal_tx, menu_selected);
                        }
                    } else {
                        let _ = try_terminal_send(&terminal_tx, TerminalEvent::Resize);
                    }
                    continue;
                }
                _ => continue,
            };

            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('t') {
                menu_mode = true;
                help_mode = false;
                menu_selected = 0;
                command_view_active.store(true, Ordering::Release);
                match open_ctrl_t_menu(&terminal_tx, menu_selected) {
                    TerminalSend::Sent => {}
                    TerminalSend::Full => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                    }
                    TerminalSend::Closed => {
                        command_view_active.store(false, Ordering::Release);
                        return;
                    }
                }
                continue;
            }

            if menu_mode {
                if help_mode {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                            menu_mode = false;
                            help_mode = false;
                            command_view_active.store(false, Ordering::Release);
                            close_ctrl_t_menu(&terminal_tx);
                        }
                        KeyCode::Char('h') | KeyCode::Char('?') => {
                            help_mode = false;
                            draw_ctrl_t_menu(&terminal_tx, menu_selected);
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Up => {
                        menu_selected = menu_selected.saturating_sub(1);
                        draw_ctrl_t_menu(&terminal_tx, menu_selected);
                    }
                    KeyCode::Down => {
                        menu_selected = (menu_selected + 1).min(ctrl_t_menu_len(jlink_actions) - 1);
                        draw_ctrl_t_menu(&terminal_tx, menu_selected);
                    }
                    KeyCode::Home => {
                        menu_selected = 0;
                        draw_ctrl_t_menu(&terminal_tx, menu_selected);
                    }
                    KeyCode::End => {
                        menu_selected = ctrl_t_menu_len(jlink_actions) - 1;
                        draw_ctrl_t_menu(&terminal_tx, menu_selected);
                    }
                    KeyCode::Enter => {
                        let result = run_menu_selection(
                            menu_selected,
                            jlink_actions,
                            &tx,
                            &terminal_tx,
                            &thread_running,
                        );
                        if result.is_break() {
                            command_view_active.store(false, Ordering::Release);
                            return;
                        }
                        if result.closes_menu() {
                            menu_mode = false;
                            command_view_active.store(false, Ordering::Release);
                        } else if menu_selected == 0 {
                            help_mode = true;
                        }
                    }
                    KeyCode::Char('h') | KeyCode::Char('?') => {
                        help_mode = true;
                        match try_terminal_send(&terminal_tx, TerminalEvent::ShowHelp) {
                            TerminalSend::Sent => {}
                            TerminalSend::Full => {
                                help_mode = false;
                            }
                            TerminalSend::Closed => return,
                        }
                    }
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::ClearScreen),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::ClearControlBuffer),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::ToggleOutputPause),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('e') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::ToggleEcho),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('t') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::ToggleTimestamps),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('m') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::ToggleOutputMode),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('r') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        if !jlink_actions {
                            terminal_status_blocking(&terminal_tx, "reset requires target flasher");
                            continue;
                        }
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::Reset),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('R') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::Reconnect),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('f') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        if !jlink_actions {
                            close_ctrl_t_menu(&terminal_tx);
                            terminal_status_blocking(&terminal_tx, "flash requires target flasher");
                            continue;
                        }
                        if run_flash_prompt_from_menu(&tx, &terminal_tx, &thread_running).is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('x') | KeyCode::Char('X') => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        if !jlink_actions {
                            terminal_status_blocking(&terminal_tx, "erase requires target flasher");
                            continue;
                        }
                        if send_input_event(
                            &tx,
                            &terminal_tx,
                            InputEvent::MenuCommand(MenuCommand::Erase),
                        )
                        .is_break()
                        {
                            return;
                        }
                    }
                    KeyCode::Char('q') => {
                        thread_running.store(false, Ordering::Release);
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        match send_input_event(&tx, &terminal_tx, InputEvent::Quit) {
                            MenuRun::Break => return,
                            MenuRun::Continue | MenuRun::KeepOpen => {}
                        }
                    }
                    KeyCode::Char(c) => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                        terminal_status_blocking(
                            &terminal_tx,
                            &format!("unknown Ctrl-T command '{c}'"),
                        );
                    }
                    KeyCode::Esc => {
                        menu_mode = false;
                        command_view_active.store(false, Ordering::Release);
                        close_ctrl_t_menu(&terminal_tx);
                    }
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(byte) = control_byte(c) {
                        if send_input_event(&tx, &terminal_tx, InputEvent::Bytes(vec![byte]))
                            .is_break()
                        {
                            return;
                        }
                    }
                }
                KeyCode::Enter => {
                    if send_input_event(&tx, &terminal_tx, InputEvent::Line(String::new()))
                        .is_break()
                    {
                        return;
                    }
                }
                KeyCode::Char(c) => {
                    let mut encoded = [0; 4];
                    let bytes = c.encode_utf8(&mut encoded).as_bytes().to_vec();
                    if send_input_event(&tx, &terminal_tx, InputEvent::Bytes(bytes)).is_break() {
                        return;
                    }
                }
                KeyCode::Backspace => {
                    if send_input_event(&tx, &terminal_tx, InputEvent::Bytes(vec![0x08])).is_break()
                    {
                        return;
                    }
                }
                KeyCode::Tab => {
                    if send_input_event(&tx, &terminal_tx, InputEvent::Bytes(vec![b'\t']))
                        .is_break()
                    {
                        return;
                    }
                }
                KeyCode::Esc => {
                    if send_input_event(&tx, &terminal_tx, InputEvent::Bytes(vec![0x1b])).is_break()
                    {
                        return;
                    }
                }
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::Right
                | KeyCode::Left
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown => {}
                KeyCode::Delete => {
                    if send_input_event(&tx, &terminal_tx, InputEvent::Bytes(b"\x1b[3~".to_vec()))
                        .is_break()
                    {
                        return;
                    }
                }
                _ => {}
            }
        }
    });
    InputTask { running, handle }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CtrlTMenuAction {
    Help,
    ClearScreen,
    ClearControlBuffer,
    ToggleOutputPause,
    ToggleEcho,
    ToggleTimestamps,
    ToggleOutputMode,
    Reset,
    Reconnect,
    Flash,
    Erase,
    Quit,
    Cancel,
}

pub(crate) struct CtrlTMenuEntry {
    key: &'static str,
    desc: &'static str,
    action: CtrlTMenuAction,
    jlink_only: bool,
}

pub(crate) const CTRL_T_MENU: &[CtrlTMenuEntry] = &[
    CtrlTMenuEntry {
        key: "h / ?",
        desc: "help",
        action: CtrlTMenuAction::Help,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "l",
        desc: "clear screen",
        action: CtrlTMenuAction::ClearScreen,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "b",
        desc: "clear control buffer",
        action: CtrlTMenuAction::ClearControlBuffer,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "s",
        desc: "stop/resume output",
        action: CtrlTMenuAction::ToggleOutputPause,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "e",
        desc: "toggle local echo",
        action: CtrlTMenuAction::ToggleEcho,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "t",
        desc: "toggle timestamps",
        action: CtrlTMenuAction::ToggleTimestamps,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "m",
        desc: "toggle normal/hex output",
        action: CtrlTMenuAction::ToggleOutputMode,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "r",
        desc: "reset target",
        action: CtrlTMenuAction::Reset,
        jlink_only: true,
    },
    CtrlTMenuEntry {
        key: "R",
        desc: "reconnect",
        action: CtrlTMenuAction::Reconnect,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "f",
        desc: "flash file",
        action: CtrlTMenuAction::Flash,
        jlink_only: true,
    },
    CtrlTMenuEntry {
        key: "x",
        desc: "erase chip",
        action: CtrlTMenuAction::Erase,
        jlink_only: true,
    },
    CtrlTMenuEntry {
        key: "q",
        desc: "quit",
        action: CtrlTMenuAction::Quit,
        jlink_only: false,
    },
    CtrlTMenuEntry {
        key: "Esc",
        desc: "cancel",
        action: CtrlTMenuAction::Cancel,
        jlink_only: false,
    },
];

pub(crate) fn visible_ctrl_t_menu(
    jlink_actions: bool,
) -> impl Iterator<Item = &'static CtrlTMenuEntry> {
    CTRL_T_MENU
        .iter()
        .filter(move |entry| !entry.jlink_only || jlink_actions)
}

pub(crate) fn ctrl_t_menu_len(jlink_actions: bool) -> usize {
    visible_ctrl_t_menu(jlink_actions).count().max(1)
}

pub(crate) fn ctrl_t_menu_entry(
    index: usize,
    jlink_actions: bool,
) -> Option<&'static CtrlTMenuEntry> {
    visible_ctrl_t_menu(jlink_actions).nth(index)
}

pub(crate) enum MenuRun {
    Continue,
    KeepOpen,
    Break,
}

impl MenuRun {
    fn is_break(&self) -> bool {
        matches!(self, MenuRun::Break)
    }

    fn closes_menu(&self) -> bool {
        matches!(self, MenuRun::Continue | MenuRun::Break)
    }
}

pub(crate) fn run_menu_selection(
    index: usize,
    jlink_actions: bool,
    tx: &mpsc::Sender<InputEvent>,
    terminal_tx: &mpsc::Sender<TerminalEvent>,
    running: &AtomicBool,
) -> MenuRun {
    match ctrl_t_menu_entry(index, jlink_actions).map(|entry| entry.action) {
        Some(CtrlTMenuAction::Help) => {
            let _ = try_terminal_send(terminal_tx, TerminalEvent::ShowHelp);
            MenuRun::KeepOpen
        }
        Some(CtrlTMenuAction::ClearScreen) => send_menu_command(tx, MenuCommand::ClearScreen),
        Some(CtrlTMenuAction::ClearControlBuffer) => {
            close_then_send_menu_command(terminal_tx, tx, MenuCommand::ClearControlBuffer)
        }
        Some(CtrlTMenuAction::ToggleOutputPause) => {
            close_then_send_menu_command(terminal_tx, tx, MenuCommand::ToggleOutputPause)
        }
        Some(CtrlTMenuAction::ToggleEcho) => {
            close_then_send_menu_command(terminal_tx, tx, MenuCommand::ToggleEcho)
        }
        Some(CtrlTMenuAction::ToggleTimestamps) => {
            close_then_send_menu_command(terminal_tx, tx, MenuCommand::ToggleTimestamps)
        }
        Some(CtrlTMenuAction::ToggleOutputMode) => {
            close_then_send_menu_command(terminal_tx, tx, MenuCommand::ToggleOutputMode)
        }
        Some(CtrlTMenuAction::Reset) => {
            close_then_send_menu_command(terminal_tx, tx, MenuCommand::Reset)
        }
        Some(CtrlTMenuAction::Reconnect) => {
            close_then_send_menu_command(terminal_tx, tx, MenuCommand::Reconnect)
        }
        Some(CtrlTMenuAction::Flash) => run_flash_prompt_from_menu(tx, terminal_tx, running),
        Some(CtrlTMenuAction::Erase) => {
            close_then_send_menu_command(terminal_tx, tx, MenuCommand::Erase)
        }
        Some(CtrlTMenuAction::Quit) => {
            close_ctrl_t_menu(terminal_tx);
            let _ = send_input_event(tx, terminal_tx, InputEvent::Quit);
            MenuRun::Break
        }
        Some(CtrlTMenuAction::Cancel) => MenuRun::Continue,
        _ => MenuRun::Continue,
    }
}

pub(crate) fn send_menu_command(tx: &mpsc::Sender<InputEvent>, command: MenuCommand) -> MenuRun {
    match tx.try_send(InputEvent::MenuCommand(command)) {
        Ok(()) => MenuRun::Continue,
        Err(mpsc::error::TrySendError::Full(_)) => MenuRun::Continue,
        Err(mpsc::error::TrySendError::Closed(_)) => MenuRun::Break,
    }
}

pub(crate) fn close_then_send_menu_command(
    terminal_tx: &mpsc::Sender<TerminalEvent>,
    tx: &mpsc::Sender<InputEvent>,
    command: MenuCommand,
) -> MenuRun {
    close_ctrl_t_menu(terminal_tx);
    send_input_event(tx, terminal_tx, InputEvent::MenuCommand(command))
}

pub(crate) fn send_input_event(
    tx: &mpsc::Sender<InputEvent>,
    terminal_tx: &mpsc::Sender<TerminalEvent>,
    event: InputEvent,
) -> MenuRun {
    match tx.try_send(event) {
        Ok(()) => MenuRun::Continue,
        Err(mpsc::error::TrySendError::Full(_)) => {
            terminal_status_blocking(terminal_tx, "input queue full; dropped key");
            MenuRun::Continue
        }
        Err(mpsc::error::TrySendError::Closed(_)) => MenuRun::Break,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalSend {
    Sent,
    Full,
    Closed,
}

pub(crate) fn try_terminal_send(
    tx: &mpsc::Sender<TerminalEvent>,
    event: TerminalEvent,
) -> TerminalSend {
    match tx.try_send(event) {
        Ok(()) => TerminalSend::Sent,
        Err(mpsc::error::TrySendError::Full(_)) => TerminalSend::Full,
        Err(mpsc::error::TrySendError::Closed(_)) => TerminalSend::Closed,
    }
}

pub(crate) fn open_ctrl_t_menu(tx: &mpsc::Sender<TerminalEvent>, selected: usize) -> TerminalSend {
    try_terminal_send(tx, TerminalEvent::ShowMenu(selected))
}

pub(crate) fn close_ctrl_t_menu(tx: &mpsc::Sender<TerminalEvent>) {
    let _ = try_terminal_send(tx, TerminalEvent::HideMenu);
}

pub(crate) fn run_flash_prompt_from_menu(
    tx: &mpsc::Sender<InputEvent>,
    terminal_tx: &mpsc::Sender<TerminalEvent>,
    running: &AtomicBool,
) -> MenuRun {
    let command = prompt_flash_command(terminal_tx, running);
    close_ctrl_t_menu(terminal_tx);

    if let Some(command) = command {
        send_menu_command(tx, command)
    } else {
        MenuRun::Continue
    }
}

pub(crate) fn draw_ctrl_t_menu(tx: &mpsc::Sender<TerminalEvent>, selected: usize) {
    let _ = try_terminal_send(tx, TerminalEvent::ShowMenu(selected));
}

pub(crate) fn draw_ctrl_t_menu_to(
    stdout: &mut impl Write,
    selected: usize,
    ui_state: TerminalUiState,
) -> Result<()> {
    let Ok((cols, rows)) = size() else {
        writeln!(
            stdout,
            "\r[rttio] Ctrl-T h help, l clear screen, b clear buffer, s stop/resume, e echo, t timestamps, m mode, r reset, R reconnect, f flash, q quit\r"
        )?;
        return Ok(());
    };

    execute!(stdout, Clear(ClearType::All))?;
    execute!(
        stdout,
        MoveTo(0, 0),
        SetForegroundColor(Color::Cyan),
        Print(truncate_to_width("rttio command mode", cols as usize)),
        ResetColor,
        MoveTo(0, 1),
        SetForegroundColor(Color::DarkGrey),
        Print(truncate_to_width(
            "Target output is paused while this screen is open. Esc cancels.",
            cols as usize
        )),
        ResetColor
    )?;

    let list_start = 3u16;
    let footer_row = rows.saturating_sub(1);
    for (index, entry) in visible_ctrl_t_menu(ui_state.jlink_actions).enumerate() {
        let row = list_start + index as u16;
        if row >= footer_row {
            break;
        }
        let marker = if index == selected { ">" } else { " " };
        let desc = ctrl_t_menu_description(entry, ui_state);
        let line = format!("{marker} {:<7} {desc}", entry.key);
        execute!(stdout, MoveTo(0, row), Clear(ClearType::CurrentLine))?;
        if index == selected {
            execute!(
                stdout,
                SetForegroundColor(Color::Black),
                SetBackgroundColor(Color::White),
                Print(truncate_to_width(&line, cols as usize)),
                ResetColor
            )?;
        } else {
            execute!(
                stdout,
                SetForegroundColor(Color::White),
                Print(truncate_to_width(&line, cols as usize)),
                ResetColor
            )?;
        }
    }

    draw_command_footer(
        stdout,
        footer_row,
        cols,
        "Up/Down select   Enter run   h help   q quit",
    )?;
    stdout.flush()?;
    Ok(())
}

pub(crate) fn ctrl_t_menu_description(entry: &CtrlTMenuEntry, ui_state: TerminalUiState) -> String {
    match entry.action {
        CtrlTMenuAction::ToggleEcho => format!(
            "toggle local echo [{}]",
            if ui_state.local_echo { "on" } else { "off" }
        ),
        CtrlTMenuAction::ToggleOutputPause => format!(
            "stop/resume output [{}]",
            if ui_state.output_paused {
                "paused"
            } else {
                "running"
            }
        ),
        CtrlTMenuAction::ToggleTimestamps => format!(
            "toggle timestamps [{}]",
            if ui_state.timestamp { "on" } else { "off" }
        ),
        CtrlTMenuAction::ToggleOutputMode => format!(
            "toggle normal/hex output [{}]",
            ui_state.output_mode.as_ctl_str()
        ),
        _ => entry.desc.to_string(),
    }
}

pub(crate) fn draw_ctrl_t_help_to(stdout: &mut impl Write) -> Result<()> {
    let Ok((cols, rows)) = size() else {
        writeln!(
            stdout,
            "\r[rttio] Ctrl-T help: h menu/help, l clear screen, b clear buffer, s stop/resume, e echo, t timestamps, m mode, r reset, R reconnect, f flash, q quit\r"
        )?;
        return Ok(());
    };

    let lines = [
        " Ctrl-T help ",
        "",
        "h / ?  show menu / help",
        "l      clear screen and deferred output",
        "b      clear control read buffer",
        "s      stop/resume target output",
        "e      toggle local echo",
        "t      toggle timestamps",
        "m      toggle normal/hex output",
        "r      reset target",
        "R      reconnect transports",
        "f      flash file",
        "q      quit rttio",
        "Esc    close this help",
        "",
        "Target output is paused while this view is open.",
    ];
    execute!(stdout, Clear(ClearType::All))?;
    for (row, line) in lines.iter().enumerate() {
        if row as u16 >= rows {
            break;
        }
        let color = if row == 0 { Color::Cyan } else { Color::White };
        execute!(
            stdout,
            MoveTo(0, row as u16),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(color),
            Print(truncate_to_width(line, cols as usize)),
            ResetColor
        )?;
    }
    stdout.flush()?;
    Ok(())
}

pub(crate) fn clear_ctrl_t_menu_to(stdout: &mut impl Write) -> Result<()> {
    execute!(stdout, LeaveAlternateScreen)?;
    stdout.flush()?;
    Ok(())
}

pub(crate) fn truncate_to_width(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

pub(crate) fn control_byte(c: char) -> Option<u8> {
    let upper = c.to_ascii_uppercase();
    if upper.is_ascii_uppercase() {
        Some((upper as u8) - b'@')
    } else {
        None
    }
}

pub(crate) fn prompt_flash_command(
    terminal_tx: &mpsc::Sender<TerminalEvent>,
    running: &AtomicBool,
) -> Option<MenuCommand> {
    let path = prompt_flash_path(terminal_tx, running)?;
    let addr = if flash_file_uses_embedded_address(&path) {
        0
    } else {
        let config = load_config_or_default_for_ui(terminal_tx);
        prompt_flash_address(
            &path,
            recent_flash_addr(&config, &path),
            terminal_tx,
            running,
        )?
    };
    Some(MenuCommand::Flash { path, addr })
}

pub(crate) fn flash_file_uses_embedded_address(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("hex" | "elf" | "uf2")
    )
}

pub(crate) fn prompt_flash_address(
    path: &Path,
    default_addr: Option<u32>,
    terminal_tx: &mpsc::Sender<TerminalEvent>,
    running: &AtomicBool,
) -> Option<u32> {
    let mut input = default_addr
        .map(|addr| format!("0x{addr:08x}"))
        .unwrap_or_default();
    draw_flash_address_prompt(path, &input);

    loop {
        if !running.load(Ordering::Acquire) {
            return None;
        }
        if event::poll(Duration::from_millis(100)).ok() != Some(true) {
            continue;
        }
        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };

        match key.code {
            KeyCode::Enter => {
                let input = input.trim();
                if input.is_empty() {
                    return Some(0);
                }
                match parse_u32(input) {
                    Ok(addr) => return Some(addr),
                    Err(e) => {
                        terminal_status_blocking(
                            terminal_tx,
                            &format!("invalid flash address: {e}"),
                        );
                        return None;
                    }
                }
            }
            KeyCode::Backspace => {
                input.pop();
                draw_flash_address_prompt(path, &input);
            }
            KeyCode::Esc => {
                terminal_status_blocking(terminal_tx, "flash cancelled");
                return None;
            }
            KeyCode::Char(c) => {
                input.push(c);
                draw_flash_address_prompt(path, &input);
            }
            _ => {}
        }
    }
}

pub(crate) fn draw_flash_address_prompt(path: &Path, input: &str) {
    let Ok((cols, rows)) = size() else {
        return;
    };
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        Clear(ClearType::All),
        MoveTo(0, 0),
        SetForegroundColor(Color::Cyan),
        Print(truncate_to_width("flash raw address", cols as usize)),
        ResetColor,
        MoveTo(0, 2),
        Print(truncate_to_width(
            &format!("file: {}", display_flash_path(path)),
            cols as usize
        )),
        MoveTo(0, 4),
        SetForegroundColor(Color::Yellow),
        Print(truncate_to_width(&format!("addr> {input}"), cols as usize)),
        ResetColor,
    );
    let _ = draw_command_footer(
        &mut stdout,
        rows.saturating_sub(1),
        cols,
        "blank = 0   Enter accept   Esc cancel",
    );
    let _ = stdout.flush();
}

pub(crate) fn prompt_flash_path(
    terminal_tx: &mpsc::Sender<TerminalEvent>,
    running: &AtomicBool,
) -> Option<PathBuf> {
    let candidates = build_flash_candidates(terminal_tx);
    let mut shown = filter_flash_candidates(&candidates, "");

    let mut selected = 0usize;
    let mut scroll = 0usize;
    let mut input = String::new();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, EnterAlternateScreen, Clear(ClearType::All));
    draw_flash_picker(&shown, selected, scroll, &input);

    loop {
        if !running.load(Ordering::Acquire) {
            return None;
        }
        if event::poll(Duration::from_millis(100)).ok() != Some(true) {
            continue;
        }
        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };

        match key.code {
            KeyCode::Up => {
                selected = selected.saturating_sub(1);
                keep_flash_selection_visible(&shown, selected, &mut scroll);
                draw_flash_picker(&shown, selected, scroll, &input);
            }
            KeyCode::Down => {
                selected = (selected + 1).min(shown.len().saturating_sub(1));
                keep_flash_selection_visible(&shown, selected, &mut scroll);
                draw_flash_picker(&shown, selected, scroll, &input);
            }
            KeyCode::Home => {
                selected = 0;
                keep_flash_selection_visible(&shown, selected, &mut scroll);
                draw_flash_picker(&shown, selected, scroll, &input);
            }
            KeyCode::End => {
                selected = shown.len().saturating_sub(1);
                keep_flash_selection_visible(&shown, selected, &mut scroll);
                draw_flash_picker(&shown, selected, scroll, &input);
            }
            KeyCode::Enter => {
                if input.trim().is_empty() {
                    if let Some(path) = shown.get(selected) {
                        return Some(path.clone());
                    }
                } else {
                    let typed = PathBuf::from(input.trim());
                    if flash_input_is_path_like(input.trim()) {
                        return Some(typed);
                    }
                    if let Some(path) = shown.get(selected) {
                        return Some(path.clone());
                    }
                    return Some(typed);
                }
            }
            KeyCode::Backspace => {
                input.pop();
                shown = filter_flash_candidates(&candidates, &input);
                selected = selected.min(shown.len().saturating_sub(1));
                scroll = 0;
                keep_flash_selection_visible(&shown, selected, &mut scroll);
                draw_flash_picker(&shown, selected, scroll, &input);
            }
            KeyCode::Tab => {
                if let Some(completed) = complete_flash_input(&input, &candidates) {
                    input = completed;
                    shown = filter_flash_candidates(&candidates, &input);
                    selected = selected.min(shown.len().saturating_sub(1));
                    scroll = 0;
                    keep_flash_selection_visible(&shown, selected, &mut scroll);
                    draw_flash_picker(&shown, selected, scroll, &input);
                }
            }
            KeyCode::Esc => {
                terminal_status_blocking(terminal_tx, "flash cancelled");
                return None;
            }
            KeyCode::Char(c) => {
                input.push(c);
                shown = filter_flash_candidates(&candidates, &input);
                selected = selected.min(shown.len().saturating_sub(1));
                scroll = 0;
                keep_flash_selection_visible(&shown, selected, &mut scroll);
                draw_flash_picker(&shown, selected, scroll, &input);
            }
            _ => {}
        }
    }
}

pub(crate) fn flash_input_is_path_like(input: &str) -> bool {
    input.contains('/')
        || input.contains('\\')
        || input.starts_with('.')
        || input.starts_with('~')
        || is_flash_file(Path::new(input))
}

pub(crate) fn filter_flash_candidates(candidates: &[PathBuf], input: &str) -> Vec<PathBuf> {
    let needle = input.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return candidates.to_vec();
    }

    candidates
        .iter()
        .filter(|path| {
            let display = display_flash_path(path).to_ascii_lowercase();
            display.contains(&needle)
        })
        .cloned()
        .collect()
}

pub(crate) fn build_flash_candidates(terminal_tx: &mpsc::Sender<TerminalEvent>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for path in load_config_or_default_for_ui(terminal_tx).recent_flash {
        push_unique_path(&mut candidates, path);
    }

    scan_flash_candidates(&PathBuf::from("."), 0, &mut candidates);
    candidates.truncate(50);
    candidates
}

pub(crate) fn scan_flash_candidates(dir: &PathBuf, depth: usize, candidates: &mut Vec<PathBuf>) {
    if depth > 5 || candidates.len() >= 50 {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    let mut entries = entries.filter_map(|entry| entry.ok()).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "backups" {
            continue;
        }

        if path.is_dir() {
            scan_flash_candidates(&path, depth + 1, candidates);
        } else if is_flash_file(&path) {
            push_unique_path(candidates, path);
        }

        if candidates.len() >= 50 {
            break;
        }
    }
}

pub(crate) fn is_flash_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("hex" | "elf" | "bin" | "uf2")
    )
}

pub(crate) fn validate_flash_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("{} does not exist", path.display()));
    }
    if !path.is_file() {
        return Err(anyhow!("{} is not a file", path.display()));
    }
    if !is_flash_file(path) {
        return Err(anyhow!(
            "{} is not a supported .hex/.elf/.bin/.uf2 file",
            path.display()
        ));
    }
    Ok(())
}

pub(crate) fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

pub(crate) fn keep_flash_selection_visible(
    recent: &[PathBuf],
    selected: usize,
    scroll: &mut usize,
) {
    if recent.is_empty() {
        *scroll = 0;
        return;
    }

    let visible = flash_picker_visible_rows().min(recent.len()).max(1);
    if selected < *scroll {
        *scroll = selected;
    } else if selected >= *scroll + visible {
        *scroll = selected + 1 - visible;
    }

    let max_scroll = recent.len().saturating_sub(visible);
    *scroll = (*scroll).min(max_scroll);
}

pub(crate) fn flash_picker_visible_rows() -> usize {
    size()
        .map(|(_, rows)| rows.saturating_sub(6) as usize)
        .unwrap_or(1)
        .max(1)
}

pub(crate) fn complete_flash_input(input: &str, candidates: &[PathBuf]) -> Option<String> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let mut matches = candidates
        .iter()
        .map(|path| display_flash_path(path))
        .filter(|path| path.starts_with(input))
        .collect::<Vec<_>>();

    matches.extend(filesystem_completions(input));
    matches.sort();
    matches.dedup();

    match matches.len() {
        0 => None,
        1 => matches.pop(),
        _ => Some(longest_common_prefix(&matches).filter(|prefix| prefix.len() > input.len())?),
    }
}

pub(crate) fn filesystem_completions(input: &str) -> Vec<String> {
    let input_path = PathBuf::from(input);
    let parent = input_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let prefix = input_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let parent_prefix = flash_completion_parent_prefix(parent);

    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };

    let mut completions = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(prefix) {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            completions.push(format!("{parent_prefix}{name}/"));
        } else if is_flash_file(&path) {
            completions.push(format!("{parent_prefix}{name}"));
        }
    }
    completions
}

pub(crate) fn flash_completion_parent_prefix(parent: &Path) -> String {
    let Some(path) = parent.to_str() else {
        return String::new();
    };
    if path.is_empty() || path == "." {
        String::new()
    } else if parent.parent().is_none() {
        path.to_string()
    } else {
        format!("{path}/")
    }
}

pub(crate) fn longest_common_prefix(values: &[String]) -> Option<String> {
    let first = values.first()?;
    let mut prefix_len = first.len();
    for value in &values[1..] {
        let next_len = first
            .char_indices()
            .map(|(idx, _)| idx)
            .chain(std::iter::once(first.len()))
            .take_while(|idx| *idx <= prefix_len)
            .take_while(|idx| value.get(..*idx) == first.get(..*idx))
            .last()
            .unwrap_or(0);
        prefix_len = prefix_len.min(next_len);
    }
    Some(first[..prefix_len].to_string())
}

pub(crate) fn display_flash_path(path: &Path) -> String {
    path.strip_prefix(".")
        .ok()
        .unwrap_or(path)
        .display()
        .to_string()
}

pub(crate) fn draw_flash_picker(recent: &[PathBuf], selected: usize, scroll: usize, input: &str) {
    let Ok((cols, rows)) = size() else {
        return;
    };
    let item_count = recent.len().min(flash_picker_visible_rows());
    let visible_scroll = scroll.min(recent.len().saturating_sub(item_count));
    let mut stdout = io::stdout();

    let _ = execute!(stdout, Clear(ClearType::All));
    let _ = execute!(
        stdout,
        MoveTo(0, 0),
        SetForegroundColor(Color::Cyan),
        Print(truncate_to_width("flash file", cols as usize)),
        ResetColor,
        MoveTo(0, 1),
        SetForegroundColor(Color::DarkGrey),
        Print(truncate_to_width(
            "Target output is paused. Type to filter or enter a path.",
            cols as usize
        )),
        ResetColor
    );

    let list_start = 3u16;
    let footer_start = rows.saturating_sub(2);
    for row in 0..item_count.max(1) {
        let body = if recent.is_empty() {
            "no recent or local .hex/.elf/.bin/.uf2 files found".to_string()
        } else {
            let index = visible_scroll + row;
            let path = display_flash_path(&recent[index]);
            format!(
                "{} {:>2}. {}",
                if index == selected { ">" } else { " " },
                index + 1,
                path
            )
        };
        let screen_row = list_start + row as u16;
        if screen_row >= footer_start {
            break;
        }
        let _ = execute!(stdout, MoveTo(0, screen_row), Clear(ClearType::CurrentLine));
        if !recent.is_empty() && visible_scroll + row == selected {
            let _ = execute!(
                stdout,
                SetForegroundColor(Color::Black),
                SetBackgroundColor(Color::White),
                Print(truncate_to_width(&body, cols as usize)),
                ResetColor
            );
        } else {
            let _ = execute!(stdout, Print(truncate_to_width(&body, cols as usize)));
        }
    }

    let _ = execute!(
        stdout,
        MoveTo(0, rows.saturating_sub(3)),
        Clear(ClearType::CurrentLine),
        SetForegroundColor(Color::Yellow),
        Print(truncate_to_width(&format!("path> {input}"), cols as usize)),
        ResetColor
    );
    let _ = draw_command_footer(
        &mut stdout,
        rows.saturating_sub(1),
        cols,
        "Up/Down select   Tab complete   Enter select/use   Esc cancel",
    );
    let _ = stdout.flush();
}

pub(crate) fn draw_command_footer(
    stdout: &mut impl Write,
    row: u16,
    cols: u16,
    text: &str,
) -> Result<()> {
    let text = footer_line_with_version(text, cols as usize);
    execute!(
        stdout,
        MoveTo(0, row),
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::DarkGrey),
        Clear(ClearType::CurrentLine),
        Print(text),
        ResetColor
    )?;
    Ok(())
}

fn footer_line_with_version(text: &str, cols: usize) -> String {
    let right = format!(" rttio {RTTIO_VERSION} ");
    if cols == 0 {
        return String::new();
    }
    if right.len() >= cols {
        return truncate_to_width(&right, cols);
    }
    let left_width = cols.saturating_sub(right.len());
    let mut line = truncate_to_width(text, left_width);
    if line.len() < left_width {
        line.push_str(&" ".repeat(left_width - line.len()));
    }
    line.push_str(&right);
    line
}

pub(crate) struct MenuCommandContext<'a> {
    pub(crate) output_mode: &'a mut OutputMode,
    pub(crate) timestamp: &'a mut bool,
    pub(crate) local_echo: &'a mut bool,
    pub(crate) output_paused: &'a mut bool,
    pub(crate) serial_tx: &'a Option<mpsc::Sender<InterfaceCommand>>,
    pub(crate) rtt_tx: &'a Option<mpsc::Sender<InterfaceCommand>>,
    pub(crate) terminal_tx: &'a mpsc::Sender<TerminalEvent>,
    #[cfg(feature = "control")]
    pub(crate) control_history: Option<&'a Arc<Mutex<ControlHistory>>>,
}

pub(crate) async fn handle_menu_command(command: MenuCommand, context: MenuCommandContext<'_>) {
    let MenuCommandContext {
        output_mode,
        timestamp,
        local_echo,
        output_paused,
        serial_tx,
        rtt_tx,
        terminal_tx,
        #[cfg(feature = "control")]
        control_history,
    } = context;
    match command {
        MenuCommand::ClearScreen => {
            let _ = terminal_tx.try_send(TerminalEvent::ClearScreen);
        }
        MenuCommand::ClearControlBuffer => {
            #[cfg(feature = "control")]
            if let Some(history) = control_history {
                let cleared = history.lock().await.clear();
                terminal_status(
                    terminal_tx,
                    &format!("control buffer cleared ({cleared} bytes dropped)"),
                )
                .await;
            } else {
                terminal_status(terminal_tx, "control buffer is not available").await;
            }
            #[cfg(not(feature = "control"))]
            terminal_status(terminal_tx, "control buffer is not available").await;
        }
        MenuCommand::ToggleOutputMode => {
            *output_mode = match *output_mode {
                OutputMode::Normal => OutputMode::Hex,
                OutputMode::Hex => OutputMode::Normal,
            };
            terminal_status(terminal_tx, &format!("output mode: {output_mode:?}")).await;
        }
        MenuCommand::ToggleTimestamps => {
            *timestamp = !*timestamp;
            terminal_status(
                terminal_tx,
                &format!(
                    "timestamps {}",
                    if *timestamp { "enabled" } else { "disabled" }
                ),
            )
            .await;
        }
        MenuCommand::ToggleEcho => {
            *local_echo = !*local_echo;
            terminal_status(
                terminal_tx,
                &format!(
                    "local echo {}",
                    if *local_echo { "enabled" } else { "disabled" }
                ),
            )
            .await;
        }
        MenuCommand::ToggleOutputPause => {
            *output_paused = !*output_paused;
            terminal_status(
                terminal_tx,
                if *output_paused {
                    "target output paused"
                } else {
                    "target output resumed"
                },
            )
            .await;
        }
        MenuCommand::Reset => {
            if let Some(tx) = rtt_tx {
                let _ = tx.try_send(InterfaceCommand::Reset { reply: None });
            } else if let Some(tx) = serial_tx {
                let _ = tx.try_send(InterfaceCommand::Reset { reply: None });
            } else {
                terminal_status(terminal_tx, "reset requires target flasher").await;
            }
        }
        MenuCommand::Flash { path, addr } => {
            if let Some(tx) = rtt_tx {
                if let Err(e) = validate_flash_file(&path) {
                    terminal_status(terminal_tx, &format!("flash file rejected: {e}")).await;
                    return;
                }
                let _ = tx.try_send(InterfaceCommand::Flash {
                    path,
                    addr,
                    reply: None,
                });
            } else if let Some(tx) = serial_tx {
                if let Err(e) = validate_flash_file(&path) {
                    terminal_status(terminal_tx, &format!("flash file rejected: {e}")).await;
                    return;
                }
                let _ = tx.try_send(InterfaceCommand::Flash {
                    path,
                    addr,
                    reply: None,
                });
            } else {
                terminal_status(terminal_tx, "flash requires target flasher").await;
            }
        }
        MenuCommand::Erase => {
            if let Some(tx) = rtt_tx {
                let _ = tx.try_send(InterfaceCommand::Erase { reply: None });
            } else if let Some(tx) = serial_tx {
                let _ = tx.try_send(InterfaceCommand::Erase { reply: None });
            } else {
                terminal_status(terminal_tx, "erase requires target flasher").await;
            }
        }
        MenuCommand::Reconnect => {
            if let Some(tx) = serial_tx {
                let _ = tx.try_send(InterfaceCommand::Reconnect { reply: None });
            }
            if let Some(tx) = rtt_tx {
                let _ = tx.try_send(InterfaceCommand::Reconnect { reply: None });
            }
        }
    }
}
