use std::io::{Stdout, Write};

use crossterm::{ExecutableCommand, QueueableCommand, cursor, event, style, terminal};

use crate::action::{Action, Mode};
use crate::buffer::Buffer;
use crate::input;

pub struct Editor {
    stdout: Stdout,
    terminal_size: (u16, u16),
    preferred_col: usize,
    mode: Mode,
    buffer: Buffer,
    status_message: Option<String>,
}

impl Editor {
    pub fn new(path: Option<&str>) -> anyhow::Result<Self> {
        let buffer = match path {
            Some(p) => Buffer::from_file(p)?,
            None => Buffer::new(None, String::new()),
        };

        let mut stdout = std::io::stdout();
        terminal::enable_raw_mode()?;
        stdout
            .execute(terminal::EnterAlternateScreen)?
            .execute(terminal::Clear(terminal::ClearType::All))?;

        Ok(Editor {
            stdout,
            terminal_size: terminal::size()?,
            preferred_col: 0,
            mode: Mode::Normal,
            buffer,
            status_message: None,
        })
    }

    pub fn draw(&mut self) -> anyhow::Result<()> {
        self.stdout
            .queue(terminal::Clear(terminal::ClearType::All))?;

        let viewport_rows = self.terminal_size.1.saturating_sub(2) as usize;

        for i in 0..viewport_rows {
            self.stdout.queue(cursor::MoveTo(0, i as u16))?;
            if i < self.buffer.line_count() {
                self.stdout.queue(style::Print(self.buffer.line(i)))?;
            } else {
                self.stdout
                    .queue(style::SetForegroundColor(style::Color::DarkGrey))?
                    .queue(style::Print("~"))?
                    .queue(style::SetForegroundColor(style::Color::Reset))?;
            }
        }

        self.draw_statusline()?;
        self.draw_message_bar()?;

        let (row, col) = self.buffer.cursor();
        self.stdout.queue(cursor::MoveTo(col as u16, row as u16))?;

        self.stdout.flush()?;
        Ok(())
    }

    fn draw_statusline(&mut self) -> anyhow::Result<()> {
        let mode_str = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Insert => " INSERT ",
        };
        let mode_bg = match self.mode {
            Mode::Normal => style::Color::Cyan,
            Mode::Insert => style::Color::Green,
        };

        let dirty_flag = if self.buffer.is_dirty() { " [+]" } else { "" };
        let file_name = self.buffer.file_name().unwrap_or("[No Name]");
        let (row, col) = self.buffer.cursor();
        let position = format!(" {}:{} ", row + 1, col + 1);
        let file_part = format!(" {}{} ", file_name, dirty_flag);

        let left_len = mode_str.len();
        let right_len = file_part.len() + position.len();
        let padding_len = (self.terminal_size.0 as usize).saturating_sub(left_len + right_len);
        let padding = " ".repeat(padding_len);

        self.stdout
            .queue(cursor::MoveTo(0, self.terminal_size.1 - 2))?;

        self.stdout
            .queue(style::SetBackgroundColor(mode_bg))?
            .queue(style::SetForegroundColor(style::Color::Black))?
            .queue(style::SetAttribute(style::Attribute::Bold))?
            .queue(style::Print(mode_str))?;

        self.stdout
            .queue(style::SetBackgroundColor(style::Color::DarkGrey))?
            .queue(style::SetForegroundColor(style::Color::White))?
            .queue(style::SetAttribute(style::Attribute::Reset))?
            .queue(style::SetBackgroundColor(style::Color::DarkGrey))?
            .queue(style::Print(padding))?
            .queue(style::Print(&file_part))?;

        self.stdout
            .queue(style::SetBackgroundColor(mode_bg))?
            .queue(style::SetForegroundColor(style::Color::Black))?
            .queue(style::SetAttribute(style::Attribute::Bold))?
            .queue(style::Print(position))?;

        self.stdout
            .queue(style::SetAttribute(style::Attribute::Reset))?
            .queue(style::SetBackgroundColor(style::Color::Reset))?
            .queue(style::SetForegroundColor(style::Color::Reset))?;

        Ok(())
    }

    fn draw_message_bar(&mut self) -> anyhow::Result<()> {
        self.stdout
            .queue(cursor::MoveTo(0, self.terminal_size.1 - 1))?
            .queue(terminal::Clear(terminal::ClearType::CurrentLine))?;

        if let Some(msg) = &self.status_message {
            self.stdout.queue(style::Print(msg))?;
        }
        Ok(())
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            self.draw()?;

            let event = event::read()?;

            if let Some(action) = input::handle_event(self.mode, event) {
                if self.execute_action(action) {
                    break;
                }
            }
        }

        Ok(())
    }

    fn execute_action(&mut self, action: Action) -> bool {
        match action {
            Action::Quit => return true,

            Action::Save => {
                self.status_message = Some(match self.buffer.save() {
                    Ok(()) => "Saved!".to_string(),
                    Err(e) => format!("Save failed: {e}"),
                });
            }

            Action::MoveUp => {
                self.buffer.move_up(self.preferred_col);
            }

            Action::MoveDown => {
                self.buffer.move_down(self.preferred_col);
            }

            Action::MoveLeft => {
                self.buffer.move_left();
                self.preferred_col = self.buffer.cursor().1;
            }

            Action::MoveRight => {
                self.buffer.move_right();
                self.preferred_col = self.buffer.cursor().1;
            }

            Action::SetMode(mode) => {
                self.mode = mode;
                self.status_message = None;
            }

            Action::InsertChar(c) => {
                self.buffer.insert_char(c);
                self.preferred_col = self.buffer.cursor().1;
            }

            Action::InsertNewline => {
                self.buffer.insert_newline();
                self.preferred_col = 0;
            }

            Action::Backspace => {
                self.buffer.backspace();
                self.preferred_col = self.buffer.cursor().1;
            }

            Action::DeleteForward => {
                self.buffer.delete_forward();
                self.preferred_col = self.buffer.cursor().1;
            }
        }

        false
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = self.stdout.execute(terminal::LeaveAlternateScreen);
    }
}
