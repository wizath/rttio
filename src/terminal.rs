use crate::*;

pub(crate) struct TerminalGuard;

static TERMINAL_PANIC_HOOK: Once = Once::new();

impl TerminalGuard {
    pub(crate) fn enter() -> Result<Self> {
        install_terminal_panic_hook();
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        cleanup_terminal();
    }
}

pub(crate) fn install_terminal_panic_hook() {
    TERMINAL_PANIC_HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            cleanup_terminal();
            previous(info);
        }));
    });
}

pub(crate) fn cleanup_terminal() {
    let mut stdout = io::stdout();
    let _ = disable_raw_mode();
    let _ = reset_scroll_region(&mut stdout);
    let _ = execute!(stdout, LeaveAlternateScreen);
    let _ = stdout.flush();
}

pub(crate) async fn terminal_task(mut rx: mpsc::Receiver<TerminalEvent>) {
    let mut stdout = io::stdout();
    let mut deferred_output = String::new();
    let mut command_view_visible = false;
    let mut command_view_restore_cursor: Option<(u16, u16)> = None;
    let mut stream_at_line_start = true;
    let mut ui_state = TerminalUiState::default();
    #[cfg(feature = "control")]
    let mut status_bar: Option<TerminalStatusBar> = None;
    #[cfg(feature = "control")]
    let mut flash_progress: Option<TerminalFlashProgress> = None;
    #[cfg(feature = "control")]
    let mut status_layout_rows: Option<u16> = None;
    #[cfg(feature = "control")]
    let mut last_tx_activity: Option<Instant> = None;
    #[cfg(feature = "control")]
    let mut last_rx_activity: Option<Instant> = None;
    let mut activity_tick = tokio::time::interval(STATUS_ACTIVITY_TICK);
    activity_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let event = tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else {
                    break;
                };
                event
            }
            _ = activity_tick.tick() => {
                #[cfg(feature = "control")]
                if !command_view_visible && status_bar.is_some() {
                    draw_status_bar_if_visible(
                        &mut stdout,
                        &mut status_layout_rows,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                    );
                    let _ = stdout.flush();
                }
                continue;
            }
        };

        match event {
            TerminalEvent::Output(text) => {
                if command_view_visible {
                    push_deferred_terminal_output(&mut deferred_output, &text);
                } else if write!(stdout, "{text}").is_ok() {
                    update_stream_line_state(&text, &mut stream_at_line_start);
                    #[cfg(feature = "control")]
                    draw_status_bar_if_visible(
                        &mut stdout,
                        &mut status_layout_rows,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                    );
                    let _ = stdout.flush();
                }
            }
            TerminalEvent::Status(text) => {
                let rendered = format_status_for_terminal(&text, stream_at_line_start);
                if command_view_visible {
                    push_deferred_terminal_output(&mut deferred_output, &rendered);
                } else if write!(stdout, "{rendered}").is_ok() {
                    update_stream_line_state(&rendered, &mut stream_at_line_start);
                    #[cfg(feature = "control")]
                    draw_status_bar_if_visible(
                        &mut stdout,
                        &mut status_layout_rows,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                    );
                    let _ = stdout.flush();
                }
            }
            TerminalEvent::SetUiState(state) => {
                ui_state = state;
            }
            #[cfg(feature = "control")]
            TerminalEvent::SetStatusBar(state) => {
                status_bar = Some(state);
                if !command_view_visible {
                    draw_status_bar_if_visible(
                        &mut stdout,
                        &mut status_layout_rows,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                    );
                    let _ = stdout.flush();
                }
            }
            #[cfg(feature = "control")]
            TerminalEvent::SetFlashProgress(progress) => {
                flash_progress = progress;
                if !command_view_visible {
                    draw_status_bar_if_visible(
                        &mut stdout,
                        &mut status_layout_rows,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                    );
                    let _ = stdout.flush();
                }
            }
            #[cfg(feature = "control")]
            TerminalEvent::Activity(source) => {
                match source {
                    Source::Tx => last_tx_activity = Some(Instant::now()),
                    Source::Serial | Source::Rtt => last_rx_activity = Some(Instant::now()),
                }
                if !command_view_visible {
                    draw_status_bar_if_visible(
                        &mut stdout,
                        &mut status_layout_rows,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                    );
                    let _ = stdout.flush();
                }
            }
            TerminalEvent::ClearScreen => {
                deferred_output.clear();
                command_view_visible = false;
                let _ = execute!(
                    stdout,
                    LeaveAlternateScreen,
                    Clear(ClearType::Purge),
                    Clear(ClearType::All),
                    MoveTo(0, 0)
                );
                #[cfg(feature = "control")]
                draw_status_bar_if_visible(
                    &mut stdout,
                    &mut status_layout_rows,
                    status_bar.as_ref(),
                    flash_progress.as_ref(),
                    status_activity_active(last_tx_activity),
                    status_activity_active(last_rx_activity),
                );
                let _ = stdout.flush();
            }
            TerminalEvent::ShowMenu(selected) => {
                if !command_view_visible {
                    command_view_restore_cursor = crossterm::cursor::position().ok();
                    #[cfg(feature = "control")]
                    {
                        let _ = reset_scroll_region(&mut stdout);
                        status_layout_rows = None;
                    }
                    let _ = execute!(stdout, EnterAlternateScreen, Clear(ClearType::All));
                }
                command_view_visible = true;
                let _ = draw_ctrl_t_menu_to(&mut stdout, selected, ui_state);
            }
            TerminalEvent::ShowHelp => {
                if !command_view_visible {
                    command_view_restore_cursor = crossterm::cursor::position().ok();
                    #[cfg(feature = "control")]
                    {
                        let _ = reset_scroll_region(&mut stdout);
                        status_layout_rows = None;
                    }
                    let _ = execute!(stdout, EnterAlternateScreen, Clear(ClearType::All));
                }
                command_view_visible = true;
                let _ = draw_ctrl_t_help_to(&mut stdout);
            }
            TerminalEvent::HideMenu => {
                let _ = clear_ctrl_t_menu_to(&mut stdout);
                command_view_visible = false;
                if let Some((col, row)) = command_view_restore_cursor.take() {
                    let _ = execute!(stdout, MoveTo(col, row));
                }
                flush_deferred_terminal_output(
                    &mut stdout,
                    &mut deferred_output,
                    &mut stream_at_line_start,
                );
                #[cfg(feature = "control")]
                draw_status_bar_if_visible(
                    &mut stdout,
                    &mut status_layout_rows,
                    status_bar.as_ref(),
                    flash_progress.as_ref(),
                    status_activity_active(last_tx_activity),
                    status_activity_active(last_rx_activity),
                );
                let _ = stdout.flush();
            }
            TerminalEvent::Exit => break,
        }
    }
}

#[cfg(feature = "control")]
pub(crate) fn draw_status_bar_if_visible(
    stdout: &mut impl Write,
    layout_rows: &mut Option<u16>,
    status_bar: Option<&TerminalStatusBar>,
    flash_progress: Option<&TerminalFlashProgress>,
    tx_active: bool,
    rx_active: bool,
) {
    let Some(status_bar) = status_bar else {
        return;
    };
    let _ = draw_status_bar_to(
        stdout,
        layout_rows,
        status_bar,
        flash_progress,
        tx_active,
        rx_active,
    );
}

#[cfg(feature = "control")]
pub(crate) fn draw_status_bar_to(
    stdout: &mut impl Write,
    layout_rows: &mut Option<u16>,
    status_bar: &TerminalStatusBar,
    flash_progress: Option<&TerminalFlashProgress>,
    tx_active: bool,
    rx_active: bool,
) -> Result<()> {
    let (cols, rows) = size()?;
    ensure_status_bar_layout(stdout, layout_rows, rows)?;
    let target_state = match status_bar.target {
        "serial" => {
            if status_bar.serial_running {
                "open"
            } else {
                "closed"
            }
        }
        "rtt" => {
            if status_bar.rtt_running {
                "open"
            } else {
                "closed"
            }
        }
        _ => {
            if status_bar.serial_running || status_bar.rtt_running {
                "open"
            } else {
                "closed"
            }
        }
    };
    let flags = format!(
        "{}{}{}",
        if status_bar.local_echo { " echo" } else { "" },
        if status_bar.timestamp { " ts" } else { "" },
        if status_bar.output_paused {
            " paused"
        } else {
            ""
        }
    );
    let buffer_pct = if status_bar.history_max_bytes == 0 {
        0
    } else {
        status_bar.history_bytes.saturating_mul(100) / status_bar.history_max_bytes
    };
    let output_mode = match status_bar.output_mode {
        OutputMode::Normal => "text",
        OutputMode::Hex => "hex",
    };
    let tx_led = if tx_active { "TX:●" } else { "TX:○" };
    let rx_led = if rx_active { "RX:●" } else { "RX:○" };
    let progress = flash_progress
        .map(format_flash_progress)
        .unwrap_or_default();
    let left = if progress.is_empty() {
        format!(
            " rttio  {}:{}  {}{}  {} {}  buffer:{}/{} KiB ({}%) ",
            status_bar.target,
            target_state,
            output_mode,
            flags,
            tx_led,
            rx_led,
            status_bar.history_bytes / 1024,
            status_bar.history_max_bytes / 1024,
            buffer_pct
        )
    } else {
        format!(
            " rttio  {}:{}  {}{}  {} {}  {}  buffer:{}/{} KiB ({}%) ",
            status_bar.target,
            target_state,
            output_mode,
            flags,
            tx_led,
            rx_led,
            progress,
            status_bar.history_bytes / 1024,
            status_bar.history_max_bytes / 1024,
            buffer_pct
        )
    };
    execute!(
        stdout,
        SavePosition,
        MoveTo(0, status_bar_row(rows)),
        SetForegroundColor(Color::Black),
        SetBackgroundColor(Color::White),
        Clear(ClearType::CurrentLine),
        Print(truncate_to_width(&left, cols as usize)),
        ResetColor,
        RestorePosition
    )?;
    Ok(())
}

#[cfg(feature = "control")]
fn format_flash_progress(progress: &TerminalFlashProgress) -> String {
    let percent = progress.percent.clamp(0, 100);
    let filled = (percent as usize) / 10;
    let empty = 10usize.saturating_sub(filled);
    format!(
        "flash:[{}{}] {:>3}% {}",
        "#".repeat(filled),
        "-".repeat(empty),
        percent,
        progress.action
    )
}

#[cfg(feature = "control")]
const STATUS_ACTIVITY_HOLD: Duration = Duration::from_millis(100);
const STATUS_ACTIVITY_TICK: Duration = Duration::from_millis(100);

fn reset_scroll_region(stdout: &mut impl Write) -> io::Result<()> {
    write!(stdout, "\x1b[r")
}

#[cfg(feature = "control")]
fn ensure_status_bar_layout(
    stdout: &mut impl Write,
    layout_rows: &mut Option<u16>,
    rows: u16,
) -> io::Result<()> {
    if rows < 2 || *layout_rows == Some(rows) {
        return Ok(());
    }

    execute!(stdout, SavePosition)?;
    write!(stdout, "\x1b[1;{}r", rows - 1)?;
    execute!(stdout, RestorePosition)?;
    *layout_rows = Some(rows);
    Ok(())
}

#[cfg(feature = "control")]
fn status_bar_row(rows: u16) -> u16 {
    rows.saturating_sub(1)
}

#[cfg(feature = "control")]
fn status_activity_active(last_activity: Option<Instant>) -> bool {
    last_activity.is_some_and(|last_activity| last_activity.elapsed() <= STATUS_ACTIVITY_HOLD)
}

pub(crate) async fn join_task_or_abort(
    name: &'static str,
    mut handle: tokio::task::JoinHandle<()>,
    timeout: Duration,
    terminal_tx: &mpsc::Sender<TerminalEvent>,
) {
    tokio::select! {
        result = &mut handle => {
            if let Err(e) = result {
                terminal_status(terminal_tx, &format!("{name} task ended with error: {e}")).await;
            }
        }
        _ = tokio::time::sleep(timeout) => {
            handle.abort();
            match handle.await {
                Ok(()) => {}
                Err(e) if e.is_cancelled() => {
                    terminal_status(terminal_tx, &format!("{name} task aborted during shutdown")).await;
                }
                Err(e) => {
                    terminal_status(terminal_tx, &format!("{name} task abort failed: {e}")).await;
                }
            }
        }
    }
}

#[cfg(feature = "control")]
pub(crate) async fn abort_task(
    name: &'static str,
    handle: tokio::task::JoinHandle<()>,
    terminal_tx: &mpsc::Sender<TerminalEvent>,
) {
    handle.abort();
    match handle.await {
        Ok(()) => {}
        Err(e) if e.is_cancelled() => {}
        Err(e) => {
            terminal_status(terminal_tx, &format!("{name} task abort failed: {e}")).await;
        }
    }
}

pub(crate) fn flush_deferred_terminal_output(
    stdout: &mut impl Write,
    deferred_output: &mut String,
    stream_at_line_start: &mut bool,
) {
    if deferred_output.is_empty() {
        return;
    }
    if write!(stdout, "{deferred_output}").is_ok() {
        update_stream_line_state(deferred_output, stream_at_line_start);
        let _ = stdout.flush();
    }
    deferred_output.clear();
}

pub(crate) const DEFERRED_OUTPUT_MAX_BYTES: usize = 1024 * 1024;

pub(crate) fn push_deferred_terminal_output(deferred_output: &mut String, text: &str) {
    deferred_output.push_str(text);
    if deferred_output.len() <= DEFERRED_OUTPUT_MAX_BYTES {
        return;
    }

    let excess = deferred_output.len() - DEFERRED_OUTPUT_MAX_BYTES;
    let split = deferred_output
        .char_indices()
        .map(|(idx, _)| idx)
        .find(|idx| *idx >= excess)
        .unwrap_or(deferred_output.len());
    deferred_output.drain(..split);
}

pub(crate) fn format_status_for_terminal(text: &str, at_line_start: bool) -> String {
    let mut rendered = String::new();
    if !at_line_start {
        rendered.push_str("\r\n");
    }
    rendered.push_str(&text.replace('\n', "\r\n"));
    rendered
}

pub(crate) fn update_stream_line_state(text: &str, at_line_start: &mut bool) {
    for byte in text.as_bytes() {
        match *byte {
            b'\n' => *at_line_start = true,
            b'\r' => {}
            _ => *at_line_start = false,
        }
    }
}

pub(crate) async fn terminal_status(tx: &mpsc::Sender<TerminalEvent>, text: &str) {
    let _ = tx
        .send(TerminalEvent::Status(format!("[rttio] {text}\n")))
        .await;
}

pub(crate) fn terminal_status_blocking(tx: &mpsc::Sender<TerminalEvent>, text: &str) {
    let _ = tx.blocking_send(TerminalEvent::Status(format!("[rttio] {text}\n")));
}

pub(crate) const LOG_FLUSH_BYTES: usize = 64 * 1024;
pub(crate) const LOG_FLUSH_RECORDS: usize = 128;
pub(crate) const LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) struct LogWriter {
    writer: Option<io::BufWriter<fs::File>>,
    bytes_since_flush: usize,
    records_since_flush: usize,
    last_flush: Instant,
}

impl LogWriter {
    pub(crate) fn open(path: &Path, append: bool) -> Result<Self> {
        let file = FsOpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(path)
            .with_context(|| format!("failed to open log file {}", path.display()))?;
        Ok(Self {
            writer: Some(io::BufWriter::with_capacity(LOG_FLUSH_BYTES, file)),
            bytes_since_flush: 0,
            records_since_flush: 0,
            last_flush: Instant::now(),
        })
    }

    pub(crate) fn disabled() -> Self {
        Self {
            writer: None,
            bytes_since_flush: 0,
            records_since_flush: 0,
            last_flush: Instant::now(),
        }
    }

    pub(crate) fn write_record(
        &mut self,
        terminal_tx: &mpsc::Sender<TerminalEvent>,
        rendered: &str,
    ) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };

        if let Err(e) = writer.write_all(rendered.as_bytes()) {
            self.disable_after_error(terminal_tx, e);
            return;
        }

        self.bytes_since_flush = self.bytes_since_flush.saturating_add(rendered.len());
        self.records_since_flush = self.records_since_flush.saturating_add(1);
        if self.should_flush() {
            self.flush_or_disable(terminal_tx);
        }
    }

    pub(crate) fn flush_or_disable(&mut self, terminal_tx: &mpsc::Sender<TerminalEvent>) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        if let Err(e) = writer.flush() {
            self.disable_after_error(terminal_tx, e);
            return;
        }
        self.bytes_since_flush = 0;
        self.records_since_flush = 0;
        self.last_flush = Instant::now();
    }

    fn should_flush(&self) -> bool {
        self.bytes_since_flush >= LOG_FLUSH_BYTES
            || self.records_since_flush >= LOG_FLUSH_RECORDS
            || self.last_flush.elapsed() >= LOG_FLUSH_INTERVAL
    }

    fn disable_after_error(&mut self, terminal_tx: &mpsc::Sender<TerminalEvent>, error: io::Error) {
        self.writer = None;
        self.bytes_since_flush = 0;
        self.records_since_flush = 0;
        let _ = terminal_tx.try_send(TerminalEvent::Status(format!(
            "[rttio] log disabled after write error: {error}\n"
        )));
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.as_mut() {
            let _ = writer.flush();
        }
    }
}
