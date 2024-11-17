use anyhow::Result;
use crossterm::{
    cursor,
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    style::{self, Color, Stylize},
    terminal::{self, ClearType},
    ExecutableCommand, QueueableCommand,
};
use std::io::{stdout, Write};
use std::time::Instant;
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
    util::as_24_bit_terminal_escaped,
};

use crate::buffer::Buffer;

#[derive(Debug, PartialEq)]
enum Mode {
    Normal,
    Insert,
    Command,
}

enum Action {
    Quit,
    Save,
    SaveAs(String),
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    MoveStartOfLine,
    MoveEndOfLine,
    PageUp,
    PageDown,
    AddChar(char),
    NewLine,
    DeleteChar,
    DeleteLine,
    EnterMode(Mode),
    NextBuffer,
    PreviousBuffer,
    ExecuteCommand(String),
}

pub struct Editor {
    buffers: Vec<Buffer>,
    active_buffer: usize,
    stdout: std::io::Stdout,
    size: (u16, u16),
    cx: u16,
    cy: u16,
    mode: Mode,
    exit: bool,
    scroll_offset: u16,
    syntax_set: SyntaxSet,
    theme: Theme,
    command_line: String,
    status_message: Option<(String, Instant)>,
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.stdout.execute(cursor::Show);
        let _ = self.stdout.execute(terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
        let _ = self.stdout.flush();
    }
}

impl Editor {
    pub fn new(buffers: Vec<Buffer>) -> Result<Self> {
        if buffers.is_empty() {
            anyhow::bail!("At least one buffer is required");
        }

        let mut stdout = stdout();
        terminal::enable_raw_mode()?;
        stdout
            .execute(terminal::EnterAlternateScreen)?
            .execute(terminal::Clear(ClearType::All))?;
        stdout.execute(cursor::Show)?;

        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set.themes["base16-ocean.dark"].clone();

        Ok(Editor {
            buffers,
            active_buffer: 0,
            stdout,
            cx: 0,
            cy: 0,
            mode: Mode::Normal,
            size: terminal::size()?,
            exit: false,
            scroll_offset: 0,
            syntax_set,
            theme,
            command_line: String::new(),
            status_message: None,
        })
    }

    fn current_buffer(&self) -> &Buffer {
        &self.buffers[self.active_buffer]
    }

    fn current_buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.active_buffer]
    }

    fn set_status_message(&mut self, msg: String) {
        self.status_message = Some((msg, Instant::now()));
    }

    fn visible_lines(&self) -> u16 {
        self.size.1.saturating_sub(2)
    }

    fn adjust_scroll(&mut self) {
        let visible_lines = self.visible_lines();

        if self.cy < self.scroll_offset {
            self.scroll_offset = self.cy;
        }

        if self.cy >= self.scroll_offset + visible_lines {
            self.scroll_offset = self.cy - visible_lines + 1;
        }
    }

    fn adjust_cursor_position(&mut self) {
        let max_cy = self.current_buffer().len().saturating_sub(1) as u16;
        self.cy = self.cy.min(max_cy);

        if self.mode != Mode::Insert {
            let line_len = self.current_buffer().get_line(self.cy as usize).len();
            self.cx = self.cx.min(line_len.saturating_sub(1) as u16);
        }

        self.adjust_scroll();
    }

    fn clear_screen(&mut self) -> Result<()> {
        self.stdout
            .queue(terminal::Clear(ClearType::All))?
            .queue(cursor::MoveTo(0, 0))?;
        Ok(())
    }

    fn draw_status_line(&mut self) -> Result<()> {
        let (width, height) = self.size;
        let file_name = self
            .current_buffer()
            .file_name()
            .unwrap_or_else(|| "[No Name]".to_string());
        let modified = if self.current_buffer().is_modified {
            "[+]"
        } else {
            ""
        };
        let status = match self.mode {
            Mode::Normal => format!("NORMAL {} {}", file_name, modified),
            Mode::Insert => format!("INSERT {} {}", file_name, modified),
            Mode::Command => format!(":{}", self.command_line),
        };

        let mut stdout = self.stdout.lock();

        stdout.queue(cursor::MoveTo(0, height - 1))?;

        stdout.queue(terminal::Clear(ClearType::CurrentLine))?;

        stdout.queue(style::PrintStyledContent(
            status.clone().bold().with(Color::White).on(Color::Blue),
        ))?;

        if let Some((msg, time)) = &self.status_message {
            if time.elapsed().as_secs() < 5 {
                let right_status = format!(" {}", msg);
                let padding = width as usize - status.len() - right_status.len();
                if padding > 0 {
                    stdout.queue(style::Print(" ".repeat(padding)))?.queue(
                        style::PrintStyledContent(right_status.with(Color::White).on(Color::Blue)),
                    )?;
                }
            }
        }

        stdout.flush()?;

        Ok(())
    }

    fn draw_buffer(&mut self) -> Result<()> {
        self.clear_screen()?;

        let visible_lines = self.visible_lines();
        let syntax = self
            .syntax_set
            .find_syntax_by_extension("rs")
            .or_else(|| self.syntax_set.find_syntax_by_extension("txt"))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);

        let mut stdout = self.stdout.lock();

        for y in 0..visible_lines {
            let line_index = (self.scroll_offset + y) as usize;

            stdout.queue(cursor::MoveTo(0, y))?;

            if line_index < self.current_buffer().len() {
                let line = self.current_buffer().get_line(line_index);
                let ranges = highlighter.highlight_line(line, &self.syntax_set);

                match ranges {
                    Ok(ranges) => {
                        let escaped = as_24_bit_terminal_escaped(&ranges[..], true);
                        stdout.queue(style::Print(escaped))?;
                    }
                    Err(e) => {
                        stdout.queue(style::Print(line))?;
                        eprintln!("Error highlighting line: {}", e);
                    }
                }
            } else {
                stdout.queue(style::Print("~"))?;
            }

            stdout.queue(terminal::Clear(ClearType::UntilNewLine))?;
        }

        stdout.flush()?;

        Ok(())
    }

    fn handle_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "w" | "write" => {
                if parts.len() > 1 {
                    self.handle_action(Action::SaveAs(parts[1].to_string()))?;
                } else {
                    self.handle_action(Action::Save)?;
                }
            }
            "q" | "quit" => {
                if !self.current_buffer().is_modified {
                    self.handle_action(Action::Quit)?;
                } else {
                    self.set_status_message(
                        "No write since last change (add ! to override)".to_string(),
                    );
                }
            }
            "q!" | "quit!" => {
                self.handle_action(Action::Quit)?;
            }
            "wq" => {
                self.handle_action(Action::Save)?;
                self.handle_action(Action::Quit)?;
            }
            _ => {
                self.set_status_message(format!("Unknown command: {}", command));
            }
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        loop {
            self.adjust_cursor_position();
            self.draw_buffer()?;
            self.draw_status_line()?;

            if self.exit {
                break;
            }

            self.stdout.flush()?;

            if let Event::Key(key) = read()? {
                match self.mode {
                    Mode::Normal => self.handle_normal_key(key)?,
                    Mode::Insert => self.handle_insert_key(key)?,
                    Mode::Command => self.handle_command_key(key)?,
                }
            }
        }
        Ok(())
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        let action = match (key.code, key.modifiers) {
            (KeyCode::Char(':'), _) => Some(Action::EnterMode(Mode::Command)),
            (KeyCode::Char('i'), _) => Some(Action::EnterMode(Mode::Insert)),
            (KeyCode::Up | KeyCode::Char('k'), _) => Some(Action::MoveUp),
            (KeyCode::Down | KeyCode::Char('j'), _) => Some(Action::MoveDown),
            (KeyCode::Left | KeyCode::Char('h'), _) => Some(Action::MoveLeft),
            (KeyCode::Right | KeyCode::Char('l'), _) => Some(Action::MoveRight),
            (KeyCode::Char('0'), _) => Some(Action::MoveStartOfLine),
            (KeyCode::Char('$'), _) => Some(Action::MoveEndOfLine),
            (KeyCode::Char('n'), _) => Some(Action::NextBuffer),
            (KeyCode::Char('p'), _) => Some(Action::PreviousBuffer),
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => Some(Action::PageDown),
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => Some(Action::PageUp),
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => Some(Action::Save),
            (KeyCode::Char('d'), _) => Some(Action::DeleteLine),
            _ => None,
        };

        if let Some(action) = action {
            self.handle_action(action)?;
        }
        Ok(())
    }

    fn handle_insert_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.handle_action(Action::EnterMode(Mode::Normal))?,
            KeyCode::Enter => self.handle_action(Action::NewLine)?,
            KeyCode::Backspace => {
                if self.cx > 0 {
                    self.cx -= 1;
                    self.handle_action(Action::DeleteChar)?;
                }
            }
            KeyCode::Char(c) => self.handle_action(Action::AddChar(c))?,
            _ => {}
        }
        Ok(())
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.command_line.clear();
                self.handle_action(Action::EnterMode(Mode::Normal))?;
            }
            KeyCode::Enter => {
                let command = std::mem::take(&mut self.command_line);
                self.handle_command(&command)?;
                self.handle_action(Action::EnterMode(Mode::Normal))?;
            }
            KeyCode::Backspace => {
                if !self.command_line.is_empty() {
                    self.command_line.pop();
                }
            }
            KeyCode::Char(c) => {
                self.command_line.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_action(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Quit => {
                self.exit = true;
            }
            Action::Save => {
                self.current_buffer_mut().save()?;
                self.set_status_message("File saved".to_string());
            }
            Action::SaveAs(path) => {
                self.current_buffer_mut().save_as(path)?;
                self.set_status_message("File saved as".to_string());
            }
            Action::MoveUp => {
                if self.cy > 0 {
                    self.cy -= 1;
                }
            }
            Action::MoveDown => {
                if self.cy < self.current_buffer().len() as u16 - 1 {
                    self.cy += 1;
                }
            }
            Action::MoveLeft => {
                if self.cx > 0 {
                    self.cx -= 1;
                }
            }
            Action::MoveRight => {
                let line_len = self.current_buffer().get_line(self.cy as usize).len() as u16;
                if self.cx < line_len {
                    self.cx += 1;
                }
            }
            Action::MoveStartOfLine => {
                self.cx = 0;
            }
            Action::MoveEndOfLine => {
                let line_len = self.current_buffer().get_line(self.cy as usize).len() as u16;
                self.cx = line_len;
            }
            Action::PageUp => {
                let visible_lines = self.visible_lines();
                self.cy = self.cy.saturating_sub(visible_lines);
            }
            Action::PageDown => {
                let visible_lines = self.visible_lines();
                self.cy = (self.cy + visible_lines).min(self.current_buffer().len() as u16 - 1);
            }
            Action::AddChar(c) => {
                let cy = self.cy as usize;
                let cx = self.cx as usize;
                self.current_buffer_mut().insert_char(cx, cy, c)?;
                self.cx += 1;
            }
            Action::NewLine => {
                let cy = self.cy as usize;
                let cx = self.cx as usize;
                self.current_buffer_mut().insert_new_line(cy, cx);
                self.cx = 0;
                self.cy += 1;
            }
            Action::DeleteChar => {
                let cy = self.cy as usize;
                let cx = self.cx as usize;
                self.current_buffer_mut().remove_char(cx, cy)?;
                if cx > 0 {
                    self.cx -= 1;
                }
            }
            Action::DeleteLine => {
                let cy = self.cy as usize;
                self.current_buffer_mut().remove_line(cy)?;
            }
            Action::EnterMode(mode) => {
                self.mode = mode;
            }
            Action::NextBuffer => {
                self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
            }
            Action::PreviousBuffer => {
                if self.active_buffer == 0 {
                    self.active_buffer = self.buffers.len() - 1;
                } else {
                    self.active_buffer -= 1;
                }
            }
            Action::ExecuteCommand(command) => {
                self.handle_command(&command)?;
            }
        }
        Ok(())
    }
}
