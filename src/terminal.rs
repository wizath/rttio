use crate::*;

pub(crate) struct TerminalGuard;

static TERMINAL_PANIC_HOOK: Once = Once::new();
const TERMINAL_OUTPUT_BATCH_BYTES: usize = 256 * 1024;
const TERMINAL_OUTPUT_BATCH_EVENTS: usize = 256;

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
    if let Ok((_, rows)) = size() {
        if rows > 0 {
            let last = rows.saturating_sub(1);
            let _ = execute!(
                stdout,
                MoveTo(0, last),
                Clear(ClearType::CurrentLine),
                MoveTo(0, last)
            );
        }
    }
    let _ = stdout.flush();
}

pub(crate) async fn terminal_task(mut rx: mpsc::Receiver<TerminalEvent>) {
    let mut stdout = io::stdout();
    let _ = reset_scroll_region(&mut stdout);
    let _ = execute!(stdout, Clear(ClearType::All), MoveTo(0, 0));
    let _ = stdout.flush();
    let mut pending_event: Option<TerminalEvent> = None;
    let mut deferred_output = String::new();
    let mut command_view_visible = false;
    let mut command_view_restore_cursor: Option<(u16, u16)> = None;
    let mut stream_at_line_start = true;
    let mut pending_ansi_escape = String::new();
    let mut output_ansi_style = TerminalAnsiStyle::default();
    #[cfg(feature = "control")]
    let mut output_cursor = (0u16, 0u16);
    let mut ui_state = TerminalUiState::default();
    #[cfg(feature = "control")]
    let mut status_bar: Option<TerminalStatusBar> = None;
    #[cfg(feature = "control")]
    let mut flash_progress: Option<TerminalFlashProgress> = None;
    #[cfg(feature = "control")]
    let mut status_layout = StatusBarLayout::default();
    #[cfg(feature = "control")]
    let mut last_tx_activity: Option<Instant> = None;
    #[cfg(feature = "control")]
    let mut last_rx_activity: Option<Instant> = None;
    loop {
        let event = if let Some(event) = pending_event.take() {
            event
        } else {
            #[cfg(feature = "control")]
            if last_tx_activity.is_some() || last_rx_activity.is_some() {
                match tokio::time::timeout(STATUS_ACTIVITY_HOLD, rx.recv()).await {
                    Ok(Some(event)) => event,
                    Ok(None) => break,
                    Err(_) => {
                        last_tx_activity = None;
                        last_rx_activity = None;
                        if !command_view_visible {
                            draw_status_bar_if_visible(
                                &mut stdout,
                                &mut status_layout,
                                status_bar.as_ref(),
                                flash_progress.as_ref(),
                                false,
                                false,
                                output_cursor,
                                &output_ansi_style,
                            );
                            let _ = stdout.flush();
                        }
                        continue;
                    }
                }
            } else {
                let Some(event) = rx.recv().await else {
                    break;
                };
                event
            }
            #[cfg(not(feature = "control"))]
            {
                let Some(event) = rx.recv().await else {
                    break;
                };
                event
            }
        };

        match event {
            TerminalEvent::Output(text) => {
                let mut batch_bytes = 0usize;
                let mut batch_events = 0usize;
                let mut wrote = false;
                let mut current = Some(text);

                while let Some(text) = current.take() {
                    batch_bytes = batch_bytes.saturating_add(text.len());
                    batch_events = batch_events.saturating_add(1);
                    let text = take_complete_terminal_output(&mut pending_ansi_escape, &text);
                    if text.is_empty() {
                        continue;
                    }
                    if command_view_visible {
                        push_deferred_terminal_output(&mut deferred_output, &text);
                    } else if write!(stdout, "{text}").is_ok() {
                        update_stream_line_state(&text, &mut stream_at_line_start);
                        output_ansi_style.update(&text);
                        #[cfg(feature = "control")]
                        update_output_cursor_position(&text, &mut output_cursor);
                        wrote = true;
                    }

                    if batch_bytes >= TERMINAL_OUTPUT_BATCH_BYTES
                        || batch_events >= TERMINAL_OUTPUT_BATCH_EVENTS
                    {
                        break;
                    }

                    match rx.try_recv() {
                        Ok(TerminalEvent::Output(next)) => current = Some(next),
                        Ok(other) => {
                            pending_event = Some(other);
                            break;
                        }
                        Err(_) => break,
                    }
                }

                if wrote {
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
                    update_output_cursor_position(&rendered, &mut output_cursor);
                    #[cfg(feature = "control")]
                    draw_status_bar_if_visible(
                        &mut stdout,
                        &mut status_layout,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                        output_cursor,
                        &output_ansi_style,
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
                        &mut status_layout,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                        output_cursor,
                        &output_ansi_style,
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
                        &mut status_layout,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                        output_cursor,
                        &output_ansi_style,
                    );
                    let _ = stdout.flush();
                }
            }
            #[cfg(feature = "control")]
            TerminalEvent::Activity(source) => match source {
                Source::Tx => {
                    last_tx_activity = Some(Instant::now());
                    if !command_view_visible {
                        draw_status_bar_if_visible(
                            &mut stdout,
                            &mut status_layout,
                            status_bar.as_ref(),
                            flash_progress.as_ref(),
                            true,
                            status_activity_active(last_rx_activity),
                            output_cursor,
                            &output_ansi_style,
                        );
                        let _ = stdout.flush();
                    }
                }
                Source::Serial | Source::Rtt => {
                    last_rx_activity = Some(Instant::now());
                    if !command_view_visible {
                        draw_status_bar_if_visible(
                            &mut stdout,
                            &mut status_layout,
                            status_bar.as_ref(),
                            flash_progress.as_ref(),
                            status_activity_active(last_tx_activity),
                            true,
                            output_cursor,
                            &output_ansi_style,
                        );
                        let _ = stdout.flush();
                    }
                }
            },
            #[cfg(feature = "control")]
            TerminalEvent::Resize => {
                if command_view_visible {
                    status_layout.reset();
                    let _ = execute!(stdout, Clear(ClearType::All));
                } else {
                    let _ = reset_scroll_region(&mut stdout);
                    if let Ok((cols, rows)) = size() {
                        output_cursor = output_cursor_after_resize(
                            output_cursor,
                            status_layout.rows,
                            cols,
                            rows,
                        );
                    }
                    draw_status_bar_if_visible(
                        &mut stdout,
                        &mut status_layout,
                        status_bar.as_ref(),
                        flash_progress.as_ref(),
                        status_activity_active(last_tx_activity),
                        status_activity_active(last_rx_activity),
                        output_cursor,
                        &output_ansi_style,
                    );
                }
                let _ = stdout.flush();
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
                {
                    output_cursor = (0, 0);
                }
                #[cfg(feature = "control")]
                draw_status_bar_if_visible(
                    &mut stdout,
                    &mut status_layout,
                    status_bar.as_ref(),
                    flash_progress.as_ref(),
                    status_activity_active(last_tx_activity),
                    status_activity_active(last_rx_activity),
                    output_cursor,
                    &output_ansi_style,
                );
                let _ = stdout.flush();
            }
            TerminalEvent::ShowMenu(selected) => {
                if !command_view_visible {
                    command_view_restore_cursor = None;
                    #[cfg(feature = "control")]
                    {
                        status_layout.reset();
                    }
                    let _ = reset_scroll_region(&mut stdout);
                    let _ = execute!(stdout, EnterAlternateScreen, Clear(ClearType::All));
                }
                command_view_visible = true;
                let _ = draw_ctrl_t_menu_to(&mut stdout, selected, ui_state);
            }
            TerminalEvent::ShowHelp => {
                if !command_view_visible {
                    command_view_restore_cursor = None;
                    #[cfg(feature = "control")]
                    {
                        status_layout.reset();
                    }
                    let _ = reset_scroll_region(&mut stdout);
                    let _ = execute!(stdout, EnterAlternateScreen, Clear(ClearType::All));
                }
                command_view_visible = true;
                let _ = draw_ctrl_t_help_to(&mut stdout);
            }
            TerminalEvent::HideMenu => {
                let _ = clear_ctrl_t_menu_to(&mut stdout);
                command_view_visible = false;
                if let Some((col, row)) = command_view_restore_cursor.take() {
                    #[cfg(feature = "control")]
                    if status_bar.is_some() {
                        let _ = move_to_output_cursor(&mut stdout, col, row);
                    } else {
                        let _ = execute!(stdout, MoveTo(col, row));
                    }
                    #[cfg(not(feature = "control"))]
                    let _ = execute!(stdout, MoveTo(col, row));
                }
                flush_deferred_terminal_output(
                    &mut stdout,
                    &mut deferred_output,
                    &mut stream_at_line_start,
                    #[cfg(feature = "control")]
                    &mut output_cursor,
                    &mut output_ansi_style,
                );
                #[cfg(feature = "control")]
                draw_status_bar_if_visible(
                    &mut stdout,
                    &mut status_layout,
                    status_bar.as_ref(),
                    flash_progress.as_ref(),
                    status_activity_active(last_tx_activity),
                    status_activity_active(last_rx_activity),
                    output_cursor,
                    &output_ansi_style,
                );
                let _ = stdout.flush();
            }
            TerminalEvent::Exit => {
                #[cfg(feature = "control")]
                let _ = clear_status_bar_layout(&mut stdout, &mut status_layout);
                let _ = stdout.flush();
                break;
            }
        }
    }
}

#[cfg(feature = "control")]
#[allow(dead_code)]
pub(crate) fn draw_status_bar_if_visible(
    stdout: &mut impl Write,
    layout: &mut StatusBarLayout,
    status_bar: Option<&TerminalStatusBar>,
    flash_progress: Option<&TerminalFlashProgress>,
    tx_active: bool,
    rx_active: bool,
    output_cursor: (u16, u16),
    output_ansi_style: &TerminalAnsiStyle,
) {
    let Some(status_bar) = status_bar else {
        return;
    };
    let _ = draw_status_bar_to(
        stdout,
        layout,
        status_bar,
        flash_progress,
        tx_active,
        rx_active,
        output_cursor,
        output_ansi_style,
    );
}

#[cfg(feature = "control")]
#[allow(dead_code)]
pub(crate) fn draw_status_bar_to(
    stdout: &mut impl Write,
    layout: &mut StatusBarLayout,
    status_bar: &TerminalStatusBar,
    flash_progress: Option<&TerminalFlashProgress>,
    tx_active: bool,
    rx_active: bool,
    _output_cursor: (u16, u16),
    output_ansi_style: &TerminalAnsiStyle,
) -> Result<()> {
    let (cols, rows) = size()?;
    if cols == 0 || rows < 2 {
        reset_scroll_region(stdout)?;
        layout.reset();
        return Ok(());
    }

    ensure_status_bar_layout(stdout, layout, rows)?;
    clear_visible_stale_status_bars(stdout, layout, rows)?;
    let target_state = match status_bar.target {
        "serial" => {
            if status_bar.serial_running {
                status_bar.target_label.as_str()
            } else {
                "closed"
            }
        }
        "rtt" => {
            if status_bar.rtt_running {
                status_bar.target_label.as_str()
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
    let mut rendered = truncate_to_width(&left, cols as usize);
    let rendered_width = rendered.chars().count();
    if rendered_width < cols as usize {
        rendered.push_str(&" ".repeat(cols as usize - rendered_width));
    }
    execute!(
        stdout,
        MoveTo(0, status_bar_row(rows)),
        SetForegroundColor(Color::Black),
        SetBackgroundColor(Color::White),
        Clear(ClearType::CurrentLine),
        Print(rendered),
        ResetColor
    )?;
    write!(stdout, "{}", output_ansi_style.restore_sequence())?;
    move_to_output_cursor_with_size(stdout, _output_cursor.0, _output_cursor.1, cols, rows)?;
    Ok(())
}

#[cfg(feature = "control")]
#[allow(dead_code)]
pub(crate) fn format_flash_progress(progress: &TerminalFlashProgress) -> String {
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
#[allow(dead_code)]
const STATUS_ACTIVITY_HOLD: Duration = Duration::from_millis(100);
fn reset_scroll_region(stdout: &mut impl Write) -> io::Result<()> {
    write!(stdout, "\x1b[r")
}

#[cfg(feature = "control")]
#[allow(dead_code)]
#[derive(Default)]
pub(crate) struct StatusBarLayout {
    rows: Option<u16>,
    stale_rows: Vec<u16>,
}

#[cfg(feature = "control")]
#[allow(dead_code)]
impl StatusBarLayout {
    fn reset(&mut self) {
        self.rows = None;
        self.stale_rows.clear();
    }

    fn remember_stale_row(&mut self, row: u16) {
        if !self.stale_rows.contains(&row) {
            self.stale_rows.push(row);
        }
        const MAX_STALE_STATUS_ROWS: usize = 16;
        if self.stale_rows.len() > MAX_STALE_STATUS_ROWS {
            let excess = self.stale_rows.len() - MAX_STALE_STATUS_ROWS;
            self.stale_rows.drain(0..excess);
        }
    }
}

#[cfg(feature = "control")]
#[allow(dead_code)]
fn ensure_status_bar_layout(
    stdout: &mut impl Write,
    layout: &mut StatusBarLayout,
    rows: u16,
) -> io::Result<()> {
    if rows < 2 || layout.rows == Some(rows) {
        return Ok(());
    }

    if let Some(previous_rows) = layout.rows {
        layout.remember_stale_row(status_bar_row(previous_rows));
    }
    write!(stdout, "\x1b[1;{}r", rows - 1)?;
    layout.rows = Some(rows);
    Ok(())
}

#[cfg(feature = "control")]
#[allow(dead_code)]
fn status_bar_row(rows: u16) -> u16 {
    rows.saturating_sub(1)
}

#[cfg(feature = "control")]
#[allow(dead_code)]
fn clear_visible_stale_status_bars(
    stdout: &mut impl Write,
    layout: &mut StatusBarLayout,
    rows: u16,
) -> io::Result<()> {
    let current_row = status_bar_row(rows);
    let mut retained = Vec::new();
    for row in layout.stale_rows.drain(..) {
        if row == current_row {
            continue;
        }
        if row < rows {
            execute!(stdout, MoveTo(0, row), Clear(ClearType::CurrentLine))?;
        } else {
            retained.push(row);
        }
    }
    layout.stale_rows = retained;
    Ok(())
}

#[cfg(feature = "control")]
#[allow(dead_code)]
fn clear_status_bar_layout(
    stdout: &mut impl Write,
    layout: &mut StatusBarLayout,
) -> io::Result<()> {
    reset_scroll_region(stdout)?;
    let (_, rows) = size()?;
    if rows >= 2 {
        let current_row = status_bar_row(rows);
        execute!(
            stdout,
            MoveTo(0, current_row),
            Clear(ClearType::CurrentLine)
        )?;
        for row in layout.stale_rows.drain(..) {
            if row < rows && row != current_row {
                execute!(stdout, MoveTo(0, row), Clear(ClearType::CurrentLine))?;
            }
        }
    }
    layout.reset();
    if rows > 0 {
        execute!(
            stdout,
            MoveTo(0, rows - 1),
            Clear(ClearType::CurrentLine),
            Print("\r\n")
        )?;
    }
    Ok(())
}

#[cfg(feature = "control")]
#[allow(dead_code)]
fn move_to_output_cursor(stdout: &mut impl Write, col: u16, row: u16) -> io::Result<()> {
    let (cols, rows) = size()?;
    move_to_output_cursor_with_size(stdout, col, row, cols, rows)
}

#[cfg(feature = "control")]
fn move_to_output_cursor_with_size(
    stdout: &mut impl Write,
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
) -> io::Result<()> {
    let (col, row) = output_cursor_position(col, row, cols, rows);
    execute!(stdout, MoveTo(col, row))
}

#[cfg(feature = "control")]
pub(crate) fn output_cursor_position(col: u16, row: u16, cols: u16, rows: u16) -> (u16, u16) {
    let max_col = cols.saturating_sub(1);
    let max_output_row = rows.saturating_sub(2);
    (col.min(max_col), row.min(max_output_row))
}

#[cfg(feature = "control")]
pub(crate) fn output_cursor_after_resize(
    cursor: (u16, u16),
    previous_rows: Option<u16>,
    cols: u16,
    rows: u16,
) -> (u16, u16) {
    let max_col = cols.saturating_sub(1);
    let max_output_row = rows.saturating_sub(2);
    let was_at_bottom = previous_rows
        .map(|rows| cursor.1 >= rows.saturating_sub(2))
        .unwrap_or(false);
    let row = if was_at_bottom {
        max_output_row
    } else {
        cursor.1.min(max_output_row)
    };
    (cursor.0.min(max_col), row)
}

#[cfg(feature = "control")]
#[allow(dead_code)]
fn status_activity_active(last_activity: Option<Instant>) -> bool {
    last_activity.is_some_and(|last_activity| last_activity.elapsed() <= STATUS_ACTIVITY_HOLD)
}

#[derive(Default)]
pub(crate) struct TerminalAnsiStyle {
    restore: String,
}

impl TerminalAnsiStyle {
    pub(crate) fn update(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut idx = 0usize;
        while idx < bytes.len() {
            if bytes[idx] != b'\x1b' || idx + 1 >= bytes.len() || bytes[idx + 1] != b'[' {
                idx += 1;
                continue;
            }

            let start = idx;
            idx += 2;
            while idx < bytes.len() && !(0x40..=0x7e).contains(&bytes[idx]) {
                idx += 1;
            }
            if idx >= bytes.len() {
                break;
            }
            if bytes[idx] == b'm' {
                let params = &text[start + 2..idx];
                if sgr_resets_style(params) {
                    self.restore.clear();
                } else {
                    self.restore.clear();
                    self.restore.push_str(&text[start..=idx]);
                }
            }
            idx += 1;
        }
    }

    pub(crate) fn restore_sequence(&self) -> &str {
        &self.restore
    }
}

fn sgr_resets_style(params: &str) -> bool {
    params.is_empty() || params == "0"
}

pub(crate) fn flush_deferred_terminal_output(
    stdout: &mut impl Write,
    deferred_output: &mut String,
    stream_at_line_start: &mut bool,
    #[cfg(feature = "control")] output_cursor: &mut (u16, u16),
    output_ansi_style: &mut TerminalAnsiStyle,
) {
    if deferred_output.is_empty() {
        return;
    }
    if write!(stdout, "{deferred_output}").is_ok() {
        update_stream_line_state(deferred_output, stream_at_line_start);
        output_ansi_style.update(deferred_output);
        #[cfg(feature = "control")]
        update_output_cursor_position(deferred_output, output_cursor);
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

pub(crate) fn take_complete_terminal_output(pending_escape: &mut String, text: &str) -> String {
    let mut combined = String::new();
    if !pending_escape.is_empty() {
        combined.push_str(pending_escape);
        pending_escape.clear();
    }
    combined.push_str(text);

    let complete_len = complete_ansi_suffix_boundary(&combined);
    if complete_len == combined.len() {
        return combined;
    }

    let pending = combined.split_off(complete_len);
    pending_escape.push_str(&pending);
    combined
}

pub(crate) fn complete_ansi_suffix_boundary(text: &str) -> usize {
    let bytes = text.as_bytes();
    let Some(esc_index) = bytes.iter().rposition(|byte| *byte == b'\x1b') else {
        return text.len();
    };
    if esc_index + 1 == bytes.len() {
        return esc_index;
    }
    if bytes[esc_index + 1] != b'[' {
        return text.len();
    }

    let mut idx = esc_index + 2;
    while idx < bytes.len() {
        if (0x40..=0x7e).contains(&bytes[idx]) {
            return text.len();
        }
        idx += 1;
    }
    esc_index
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

#[cfg(feature = "control")]
pub(crate) fn update_output_cursor_position(text: &str, cursor: &mut (u16, u16)) {
    let (cols, rows) = size().unwrap_or((80, 24));
    update_output_cursor_position_with_size(text, cursor, cols, rows);
}

#[cfg(feature = "control")]
pub(crate) fn update_output_cursor_position_with_size(
    text: &str,
    cursor: &mut (u16, u16),
    cols: u16,
    rows: u16,
) {
    let max_col = cols.saturating_sub(1);
    let max_row = rows.saturating_sub(2);
    let bytes = text.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        match bytes[idx] {
            b'\x1b' => {
                idx += 1;
                if idx < bytes.len() && bytes[idx] == b'[' {
                    idx += 1;
                    while idx < bytes.len() && !(0x40..=0x7e).contains(&bytes[idx]) {
                        idx += 1;
                    }
                    idx += usize::from(idx < bytes.len());
                }
            }
            b'\r' => {
                cursor.0 = 0;
                idx += 1;
            }
            b'\n' => {
                cursor.0 = 0;
                cursor.1 = cursor.1.saturating_add(1).min(max_row);
                idx += 1;
            }
            byte if byte < 0x20 => {
                idx += 1;
            }
            _ => {
                if cursor.0 >= max_col {
                    cursor.0 = 0;
                    cursor.1 = cursor.1.saturating_add(1).min(max_row);
                } else {
                    cursor.0 += 1;
                }
                idx += 1;
            }
        }
    }
}

pub(crate) async fn terminal_status(tx: &mpsc::Sender<TerminalEvent>, text: &str) {
    let _ = tx.try_send(TerminalEvent::Status(format!("[rttio] {text}\n")));
}

pub(crate) fn terminal_status_blocking(tx: &mpsc::Sender<TerminalEvent>, text: &str) {
    let _ = tx.try_send(TerminalEvent::Status(format!("[rttio] {text}\n")));
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
