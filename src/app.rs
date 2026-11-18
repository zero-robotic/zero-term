use crate::{
    input,
    palette::{self, ACCENT, BG, FG},
    terminal::Session,
    text::TextPainter,
};
use alacritty_terminal::{
    event::WindowSize,
    grid::Scroll,
    index::{Column, Line, Point as TermPoint, Side},
    selection::{Selection, SelectionType},
    term::{cell::Flags, TermMode},
    vte::ansi::CursorShape,
};
use std::time::Duration;
use zui_api::clipboard::Clipboard;
use zui_app::{clipboard::NativeClipboard, Application, ApplicationError};
use zui_input::{InputEvent, Key, KeyInput, Modifiers, PointerButton, ScrollDelta};
use zui_paint::{Painter, Path, Scene};
use zui_platform::{ImeRequest, ImeSessionId, WindowAction, WindowEvent, WindowMetrics};
use zui_types::{Color, Point, Rect, Size};

const BAR: f64 = 42.0;
const FOOTER: f64 = 26.0;
const PAD: f64 = 12.0;
const POLL: Duration = Duration::from_millis(16);

pub struct TerminalApp {
    tabs: Vec<Session>,
    active: usize,
    next_id: usize,
    ime_generation: u64,
    shell: Option<alacritty_terminal::tty::Shell>,
    mouse_button: Option<u8>,
    scroll_remainder: f64,
    text: TextPainter,
    viewport: Size,
    pointer: Point,
    selecting: bool,
    modifiers: Modifiers,
    focused: bool,
    preedit: String,
    notice: String,
    blink: bool,
    clipboard: NativeClipboard,
    smoke: Option<Duration>,
}
impl TerminalApp {
    pub fn new(smoke: Option<Duration>) -> Result<Self, ApplicationError> {
        Self::with_shell(smoke, None)
    }
    fn with_shell(
        smoke: Option<Duration>,
        shell: Option<alacritty_terminal::tty::Shell>,
    ) -> Result<Self, ApplicationError> {
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            ime_generation: 0,
            shell,
            mouse_button: None,
            scroll_remainder: 0.0,
            text: TextPainter::new()?,
            viewport: Size::new(1000.0, 680.0),
            pointer: Point::ZERO,
            selecting: false,
            modifiers: Modifiers::default(),
            focused: true,
            preedit: String::new(),
            notice: String::new(),
            blink: true,
            clipboard: NativeClipboard::default(),
            smoke,
        };
        app.add_tab()?;
        Ok(app)
    }
    fn dimensions(&self) -> WindowSize {
        WindowSize {
            num_cols: ((self.viewport.width - PAD * 2.0) / self.text.cell_width)
                .floor()
                .clamp(2.0, 1000.0) as u16,
            num_lines: ((self.viewport.height - BAR - FOOTER - PAD * 2.0) / self.text.cell_height)
                .floor()
                .clamp(1.0, 500.0) as u16,
            cell_width: self.text.cell_width as u16,
            cell_height: self.text.cell_height as u16,
        }
    }
    fn add_tab(&mut self) -> Result<(), ApplicationError> {
        let session = Session::new(
            format!("Shell {}", self.next_id),
            self.dimensions(),
            self.shell.clone(),
        )?;
        self.next_id += 1;
        self.tabs.push(session);
        self.switch(self.tabs.len() - 1);
        Ok(())
    }
    fn switch(&mut self, index: usize) {
        self.active = index;
        self.ime_generation += 1;
        self.mouse_button = None;
        self.scroll_remainder = 0.0;
        self.preedit.clear();
        self.selecting = false;
        self.notice.clear();
    }
    fn close(&mut self, index: usize) {
        self.tabs.remove(index);
        let next = if index < self.active {
            self.active - 1
        } else {
            self.active.min(self.tabs.len().saturating_sub(1))
        };
        self.switch(next);
    }
    fn tab_width(&self) -> f64 {
        ((self.viewport.width - 52.0).max(1.0) / self.tabs.len().max(1) as f64).clamp(1.0, 210.0)
    }
    fn term_rect(&self) -> Rect {
        Rect::new(
            PAD,
            BAR + PAD,
            (self.viewport.width - PAD * 2.0).max(0.0),
            (self.viewport.height - BAR - FOOTER - PAD * 2.0).max(0.0),
        )
    }
    fn point_in_grid(&self, p: Point) -> (TermPoint, Side) {
        let size = self.dimensions();
        let x = ((p.x - PAD) / self.text.cell_width).max(0.0);
        let row = ((p.y - BAR - PAD) / self.text.cell_height)
            .floor()
            .clamp(0.0, f64::from(size.num_lines - 1)) as i32;
        let offset = self.tabs[self.active].term.lock().grid().display_offset() as i32;
        (
            TermPoint::new(
                Line(row - offset),
                Column((x as usize).min(usize::from(size.num_cols - 1))),
            ),
            if x.fract() < 0.5 {
                Side::Left
            } else {
                Side::Right
            },
        )
    }
    fn copy(&mut self) {
        let text = self.tabs[self.active].term.lock().selection_to_string();
        if let Some(text) = text {
            self.notice = match self.clipboard.write_text(&text) {
                Ok(()) => "Selection copied".into(),
                Err(e) => e.to_string(),
            };
        }
    }
    fn paste(&mut self) {
        // Preserve the existing unrestricted paste size; zui requires an explicit limit.
        match self.clipboard.read_text(usize::MAX) {
            Ok(text) => {
                let bracketed = self.tabs[self.active]
                    .term
                    .lock()
                    .mode()
                    .contains(TermMode::BRACKETED_PASTE);
                self.tabs[self.active].input(input::paste(&text, bracketed));
            }
            Err(e) => self.notice = e.to_string(),
        }
    }
    fn keyboard(&mut self, key: KeyInput) {
        self.modifiers = key.modifiers;
        if !key.pressed {
            return;
        }
        let m = key.modifiers;
        let shortcut = m.super_key || (m.control && m.shift);
        if shortcut {
            if let Key::Character(s) = &key.key {
                match s.to_ascii_lowercase().as_str() {
                    "t" => {
                        if !key.repeat {
                            if let Err(e) = self.add_tab() {
                                self.notice = e.to_string();
                            }
                        }
                        return;
                    }
                    "w" => {
                        if !key.repeat {
                            self.close(self.active);
                        }
                        return;
                    }
                    "c" => {
                        self.copy();
                        return;
                    }
                    "v" => {
                        self.paste();
                        return;
                    }
                    n if n.len() == 1 && ("1"..="9").contains(&n) => {
                        let index = n.parse::<usize>().unwrap() - 1;
                        if index < self.tabs.len() {
                            self.switch(index);
                        }
                        return;
                    }
                    _ => {}
                }
            }
        }
        if m.control && key.key == Key::Tab {
            let count = self.tabs.len();
            self.switch((self.active + if m.shift { count - 1 } else { 1 }) % count);
            return;
        }
        if m.shift && matches!(key.key, Key::PageUp | Key::PageDown) {
            let scroll = if key.key == Key::PageUp {
                Scroll::PageUp
            } else {
                Scroll::PageDown
            };
            self.tabs[self.active].term.lock().scroll_display(scroll);
            return;
        }
        if !self.preedit.is_empty() {
            return;
        }
        let mode = *self.tabs[self.active].term.lock().mode();
        if let Some(bytes) = input::encode(&key, mode) {
            self.tabs[self.active].input(bytes);
        }
    }
    fn mouse_report(&self, button: u8, pressed: bool) -> bool {
        let term = self.tabs[self.active].term.lock();
        let mode = *term.mode();
        if self.modifiers.shift
            || !mode.intersects(TermMode::MOUSE_MODE)
            || !self.term_rect().contains(self.pointer)
        {
            return false;
        }
        let col = ((self.pointer.x - PAD) / self.text.cell_width) as usize + 1;
        let row = ((self.pointer.y - BAR - PAD) / self.text.cell_height) as usize + 1;
        let modifiers = u8::from(self.modifiers.shift) * 4
            + u8::from(self.modifiers.alt) * 8
            + u8::from(self.modifiers.control) * 16;
        let bytes = if mode.contains(TermMode::SGR_MOUSE) {
            format!(
                "\x1b[<{};{col};{row}{}",
                button + modifiers,
                if pressed { 'M' } else { 'm' }
            )
            .into_bytes()
        } else if col < 224 && row < 224 {
            vec![
                27,
                b'[',
                b'M',
                32 + if pressed {
                    button + modifiers
                } else {
                    3 + modifiers
                },
                32 + col as u8,
                32 + row as u8,
            ]
        } else {
            return true;
        };
        drop(term);
        self.tabs[self.active].write(bytes);
        true
    }
    fn input(&mut self, event: InputEvent) {
        match event {
            InputEvent::Key(key) => self.keyboard(key),
            InputEvent::ModifiersChanged(m) => self.modifiers = m,
            InputEvent::ImePreedit { text, .. } => self.preedit = text,
            InputEvent::ImeDisabled => self.preedit.clear(),
            InputEvent::ImeCommit(text) => {
                self.preedit.clear();
                self.tabs[self.active].input(text.into_bytes());
            }
            InputEvent::PointerMoved(p) => {
                self.pointer = p;
                let mode = *self.tabs[self.active].term.lock().mode();
                if mode.contains(TermMode::MOUSE_MOTION)
                    || (mode.contains(TermMode::MOUSE_DRAG) && self.mouse_button.is_some())
                {
                    self.mouse_report(32 + self.mouse_button.unwrap_or(3), true);
                }
                if self.selecting {
                    let (point, side) = self.point_in_grid(p);
                    if let Some(selection) = self.tabs[self.active].term.lock().selection.as_mut() {
                        selection.update(point, side);
                    }
                }
            }
            InputEvent::PointerButton {
                button,
                pressed,
                position,
            } => {
                if let Some(p) = position {
                    self.pointer = p;
                }
                let code = match button {
                    PointerButton::Primary => 0,
                    PointerButton::Middle => 1,
                    _ => 2,
                };
                self.mouse_button = if pressed { Some(code) } else { None };
                if self.pointer.y < BAR {
                    if pressed && button == PointerButton::Primary {
                        let width = self.tab_width();
                        let i = (self.pointer.x / width).floor() as usize;
                        if i < self.tabs.len() {
                            if width >= 64.0 && self.pointer.x > (i + 1) as f64 * width - 28.0 {
                                self.close(i);
                            } else {
                                self.switch(i);
                            }
                        } else if self.pointer.x < width * self.tabs.len() as f64 + 48.0 {
                            if let Err(e) = self.add_tab() {
                                self.notice = e.to_string();
                            }
                        }
                    }
                } else if self.mouse_report(
                    match button {
                        PointerButton::Primary => 0,
                        PointerButton::Middle => 1,
                        _ => 2,
                    },
                    pressed,
                ) {
                    self.selecting = false;
                } else if button == PointerButton::Primary {
                    self.selecting = pressed && self.term_rect().contains(self.pointer);
                    if self.selecting {
                        let (point, side) = self.point_in_grid(self.pointer);
                        self.tabs[self.active].term.lock().selection =
                            Some(Selection::new(SelectionType::Simple, point, side));
                    }
                }
                if !pressed {
                    self.selecting = false;
                }
            }
            InputEvent::Scroll(delta) => {
                let lines = match delta {
                    ScrollDelta::Lines { y, .. } => y * 3.0,
                    ScrollDelta::Pixels { y, .. } => y / self.text.cell_height,
                };
                self.scroll_remainder += lines;
                let lines = self.scroll_remainder.trunc();
                self.scroll_remainder -= lines;
                if lines != 0.0 {
                    if self.mouse_report(if lines > 0.0 { 64 } else { 65 }, true) {
                        return;
                    }
                    let mut term = self.tabs[self.active].term.lock();
                    if term.mode().contains(TermMode::ALT_SCREEN) {
                        drop(term);
                        let key = if lines > 0.0 { b"\x1bOA" } else { b"\x1bOB" };
                        self.tabs[self.active]
                            .write(key.repeat(lines.abs().ceil().min(20.0) as usize));
                    } else {
                        term.scroll_display(Scroll::Delta(lines.round() as i32));
                    }
                }
            }
            _ => {}
        }
    }
    fn paint_terminal(&mut self, painter: &mut Painter<'_>) -> Result<(), ApplicationError> {
        let session = &self.tabs[self.active];
        // Copy visible cells, then release the lock before shaping text or submitting graphics.
        let (cells, cursor, offset, selection, colors) = {
            let term = session.term.lock();
            let content = term.renderable_content();
            (
                content
                    .display_iter
                    .map(|c| (c.point, c.cell.clone()))
                    .collect::<Vec<_>>(),
                content.cursor,
                content.display_offset,
                content.selection,
                *content.colors,
            )
        };
        let cw = self.text.cell_width;
        let ch = self.text.cell_height;
        for (point, cell) in &cells {
            let row = point.line.0 + offset as i32;
            if row < 0 {
                continue;
            }
            let mut bg = palette::resolve(
                if cell.flags.contains(Flags::INVERSE) {
                    cell.fg
                } else {
                    cell.bg
                },
                &colors,
            );
            if selection.as_ref().is_some_and(|s| s.contains(*point)) {
                bg = Color::rgb(57, 77, 112);
            }
            if bg != BG {
                painter.fill_rect(
                    Rect::new(
                        PAD + point.column.0 as f64 * cw,
                        BAR + PAD + f64::from(row) * ch,
                        cw,
                        ch,
                    ),
                    bg,
                );
            }
        }
        for (point, cell) in cells {
            let row = point.line.0 + offset as i32;
            if row < 0 {
                continue;
            }
            let x = PAD + point.column.0 as f64 * cw;
            let y = BAR + PAD + f64::from(row) * ch;
            let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                2.0 * cw
            } else {
                cw
            };
            let mut fg = palette::resolve(cell.fg, &colors);
            let mut bg = palette::resolve(cell.bg, &colors);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags.contains(Flags::DIM) {
                fg = Color::rgb(fg.r / 2, fg.g / 2, fg.b / 2);
            }
            if !cell.flags.intersects(
                Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
            ) && cell.c != ' '
            {
                let mut value = cell.c.to_string();
                if let Some(extra) = cell.zerowidth() {
                    value.extend(extra);
                }
                if cell.flags.contains(Flags::WIDE_CHAR) {
                    self.text.paint_wide_cell(painter, &value, x, y, fg)?;
                } else {
                    self.text.paint(painter, &value, x, y, fg)?;
                }
            }
            if cell.flags.intersects(Flags::ALL_UNDERLINES) {
                painter.fill_rect(Rect::new(x, y + ch - 2.0, width, 1.0), fg);
            }
            if cell.flags.contains(Flags::STRIKEOUT) {
                painter.fill_rect(Rect::new(x, y + ch * 0.55, width, 1.0), fg);
            }
        }
        let row = cursor.point.line.0 + offset as i32;
        if row >= 0
            && row < i32::from(session.size.num_lines)
            && cursor.shape != CursorShape::Hidden
            && (self.blink || !self.focused || !self.preedit.is_empty())
        {
            let x = PAD + cursor.point.column.0 as f64 * cw;
            let y = BAR + PAD + f64::from(row) * ch;
            match cursor.shape {
                CursorShape::Beam if self.focused => {
                    painter.fill_rect(Rect::new(x, y, 2.0, ch), ACCENT)
                }
                CursorShape::Underline if self.focused => {
                    painter.fill_rect(Rect::new(x, y + ch - 2.0, cw, 2.0), ACCENT)
                }
                _ => painter.stroke(
                    Path::rectangle(Rect::new(x + 0.5, y + 0.5, cw - 1.0, ch - 1.0)),
                    ACCENT,
                    1.0,
                ),
            }
            if !self.preedit.is_empty() {
                painter.fill_rect(
                    Rect::new(
                        x,
                        y,
                        (self.preedit.chars().count() as f64 * cw * 2.0)
                            .min(self.viewport.width - x),
                        ch,
                    ),
                    BG,
                );
                self.text.paint(painter, &self.preedit, x, y, ACCENT)?;
            }
        }
        Ok(())
    }
}
impl Application for TerminalApp {
    fn background(&self) -> Color {
        BG
    }
    fn ime_request(&self) -> Option<ImeRequest> {
        if !self.focused || self.tabs.is_empty() {
            return None;
        }
        let term = self.tabs[self.active].term.lock();
        let point = term.grid().cursor.point;
        Some(ImeRequest {
            session: ImeSessionId {
                owner: self.active as u64 + 1,
                generation: self.ime_generation,
            },
            cursor_area: Rect::new(
                PAD + point.column.0 as f64 * self.text.cell_width,
                BAR + PAD + f64::from(point.line.0) * self.text.cell_height,
                self.text.cell_width,
                self.text.cell_height,
            ),
        })
    }
    fn update(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<WindowAction, ApplicationError> {
        if self.tabs.is_empty()
            || self.smoke.is_some_and(|deadline| elapsed >= deadline)
            || event == WindowEvent::CloseRequested
        {
            return Ok(WindowAction::exit());
        }
        let mut redraw = false;
        let size = metrics.logical_size();
        if size != self.viewport && size.width > 0.0 && size.height > 0.0 {
            self.viewport = size;
            let dimensions = self.dimensions();
            for tab in &mut self.tabs {
                tab.resize(dimensions);
            }
            redraw = true;
        }
        match event {
            WindowEvent::Input(input) => {
                self.input(input);
                redraw = true;
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.selecting = false;
                self.mouse_button = None;
                if !focused {
                    self.preedit.clear();
                }
                if self.tabs[self.active]
                    .term
                    .lock()
                    .mode()
                    .contains(TermMode::FOCUS_IN_OUT)
                {
                    self.tabs[self.active]
                        .write(if focused { b"\x1b[I" } else { b"\x1b[O" }.to_vec());
                }
                redraw = true;
            }
            _ => {}
        }
        if self.tabs.is_empty() {
            return Ok(WindowAction::exit());
        }
        for tab in &mut self.tabs {
            redraw |= tab.poll();
        }
        let blink = elapsed.as_millis() % 1000 < 500;
        redraw |= blink != self.blink;
        self.blink = blink;
        Ok(WindowAction {
            redraw,
            exit: false,
            wake_at: Some(elapsed + POLL),
        })
    }
    fn draw(
        &mut self,
        scene: &mut Scene,
        _metrics: WindowMetrics,
        _elapsed: Duration,
    ) -> Result<(), ApplicationError> {
        scene.clear();
        if self.tabs.is_empty() {
            return Ok(());
        }
        let mut painter = scene.painter();
        painter.fill_rect(
            Rect::new(0.0, 0.0, self.viewport.width, BAR),
            Color::rgb(28, 33, 45),
        );
        let width = self.tab_width();
        for (i, tab) in self.tabs.iter().enumerate() {
            let x = i as f64 * width;
            if i == self.active {
                painter.fill_rect(Rect::new(x, 0.0, width, BAR), BG);
                painter.fill_rect(
                    Rect::new(x + 8.0, BAR - 2.0, (width - 16.0).max(0.0), 2.0),
                    ACCENT,
                );
            }
            let max_chars = ((width - 52.0) / self.text.cell_width).max(0.0) as usize;
            let title: String = tab
                .title
                .chars()
                .filter(|c| !c.is_control())
                .take(max_chars)
                .collect();
            let label = if tab.exited {
                format!("{title} · done")
            } else {
                title
            };
            let mut error = None;
            painter.with_clip(
                Path::rectangle(Rect::new(x + 8.0, 0.0, (width - 38.0).max(0.0), BAR)),
                |p| {
                    if let Err(e) = self.text.paint(
                        p,
                        &label,
                        x + 12.0,
                        10.0,
                        if i == self.active {
                            FG
                        } else {
                            Color::rgb(133, 145, 165)
                        },
                    ) {
                        error = Some(e);
                    }
                },
            );
            if let Some(e) = error {
                return Err(e);
            }
            if width >= 64.0 {
                self.text.paint(
                    &mut painter,
                    "×",
                    x + width - 23.0,
                    10.0,
                    Color::rgb(133, 145, 165),
                )?;
            }
        }
        self.text.paint(
            &mut painter,
            "+",
            width * self.tabs.len() as f64 + 17.0,
            9.0,
            ACCENT,
        )?;
        let mut error = None;
        painter.with_clip(Path::rectangle(self.term_rect()), |p| {
            if let Err(e) = self.paint_terminal(p) {
                error = Some(e);
            }
        });
        if let Some(e) = error {
            return Err(e);
        }
        let y = (self.viewport.height - FOOTER).max(BAR);
        painter.fill_rect(
            Rect::new(0.0, y, self.viewport.width, FOOTER),
            Color::rgb(28, 33, 45),
        );
        let tab = &self.tabs[self.active];
        let status = if !self.notice.is_empty() {
            self.notice.clone()
        } else if tab.exited {
            "Process exited · close this tab or create a new one".into()
        } else {
            format!("{} / {}    {} × {}    New: Cmd/Ctrl+Shift+T   Switch: Ctrl+Tab   Copy/Paste: Cmd/Ctrl+Shift+C/V", self.active + 1, self.tabs.len(), tab.size.num_cols, tab.size.num_lines)
        };
        self.text.paint(
            &mut painter,
            &status,
            PAD,
            y + 3.0,
            Color::rgb(133, 145, 165),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{grid::Dimensions, tty::Shell, vte::ansi::Processor};
    use zui_paint::{Command, RenderLimits};
    use zui_types::Extent;

    #[test]
    fn tabs_keep_independent_state_resize_and_close_safely() {
        let mut app =
            TerminalApp::with_shell(None, Some(Shell::new("/bin/sh".into(), vec![]))).unwrap();
        app.add_tab().unwrap();
        app.add_tab().unwrap();
        assert_eq!(app.active, 2);
        assert_eq!(app.tabs.len(), 3);
        app.tabs[0].term.lock().grid_mut()[TermPoint::new(Line(0), Column(0))].c = 'A';
        app.tabs[1].term.lock().grid_mut()[TermPoint::new(Line(0), Column(0))].c = 'B';
        app.switch(1);
        assert_eq!(
            app.tabs[app.active].term.lock().grid()[TermPoint::new(Line(0), Column(0))].c,
            'B'
        );
        // Closing a background tab preserves the current shell identity.
        let current = app.tabs[1].term.clone();
        app.close(0);
        assert!(std::sync::Arc::ptr_eq(&app.tabs[app.active].term, &current));
        let metrics = WindowMetrics {
            extent: Extent::new(720, 440),
            scale: 1.0,
            occluded: false,
        };
        app.update(WindowEvent::Resized, metrics, Duration::ZERO)
            .unwrap();
        for tab in &app.tabs {
            assert_eq!(
                tab.term.lock().columns(),
                usize::from(app.dimensions().num_cols)
            );
            assert_eq!(
                tab.term.lock().screen_lines(),
                usize::from(app.dimensions().num_lines)
            );
        }
        app.close(1);
        app.close(0);
        assert!(
            app.update(WindowEvent::Wake, metrics, Duration::ZERO)
                .unwrap()
                .exit
        );
    }

    #[test]
    fn ansi_wide_text_and_selection_produce_valid_scene() {
        let mut app = TerminalApp::with_shell(
            None,
            Some(Shell::new(
                "/bin/sh".into(),
                vec!["-c".into(), "exit".into()],
            )),
        )
        .unwrap();
        // Wait for EOF before replacing the grid with deterministic test output.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !app.tabs[0].exited && std::time::Instant::now() < deadline {
            app.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(app.tabs[0].exited);
        let mut parser: Processor = Processor::new();
        {
            let mut term = app.tabs[0].term.lock();
            parser.advance(
                &mut *term,
                "\x1b[2J\x1b[H\x1b[31;44m中文\x1b[0m e\u{301}".as_bytes(),
            );
            let mut selection = Selection::new(
                SelectionType::Simple,
                TermPoint::new(Line(0), Column(0)),
                Side::Left,
            );
            selection.update(TermPoint::new(Line(0), Column(3)), Side::Right);
            term.selection = Some(selection);
            assert_eq!(term.selection_to_string().as_deref(), Some("中文"));
        }
        let mut scene = Scene::new();
        app.draw(
            &mut scene,
            WindowMetrics {
                extent: Extent::new(1000, 680),
                scale: 1.0,
                occluded: false,
            },
            Duration::ZERO,
        )
        .unwrap();
        scene.validate(RenderLimits::default()).unwrap();
        assert!(scene.commands().iter().any(
            |c| matches!(c, Command::Glyphs { color, .. } if *color == Color::rgb(240, 113, 120))
        ));
    }
}
