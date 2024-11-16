use std::io::{stdout, Write};

use crossterm::{
    cursor,
    event::{self, read},
    style::{self, Color, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};

use syntect::{easy::HighlightLines, highlighting::Theme};
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxSet, SyntaxReference};
use syntect::util::as_24_bit_terminal_escaped;

use crate::buffer::Buffer;

enum Action {
    Quit,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    AddChar(char),
    NewLine,
    DeleteChar,
    EnterMode(Mode),
    NextBuffer,
    PreviousBuffer,
}

#[derive(Debug)]
enum Mode {
    Normal,
    Insert,
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
}

impl Drop for Editor {
    fn drop(&mut self) {
        _ = self.stdout.flush();
        _ = self.stdout.execute(terminal::LeaveAlternateScreen);
        _ = terminal::disable_raw_mode();
    }
}

impl Editor {
    pub fn new(buffers: Vec<Buffer>) -> anyhow::Result<Self> {
        let mut stdout = stdout();
        terminal::enable_raw_mode()?;
        stdout
            .execute(terminal::EnterAlternateScreen)?
            .execute(terminal::Clear(terminal::ClearType::All))?;

        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = &theme_set.themes["base16-ocean.dark"];
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
            theme: theme.clone(),
        })
    }

    fn visible_lines(&self) -> u16 {
        self.size.1.saturating_sub(2)
    }

    fn get_screen_position(&self) -> (u16, u16) {
        let screen_y = if self.cy >= self.scroll_offset {
            self.cy - self.scroll_offset
        } else {
            0
        };
        (self.cx, screen_y)
    }

    fn adjust_scroll(&mut self) {
        let visible_lines = self.visible_lines();

        let max_cy = (self.buffers[self.active_buffer].len() as u16).saturating_sub(1);
        self.cy = self.cy.min(max_cy);

        if self.cy < self.scroll_offset {
            self.scroll_offset = self.cy;
        }

        if self.cy >= self.scroll_offset + visible_lines {
            self.scroll_offset = self.cy.saturating_sub(visible_lines).saturating_add(1);
        }

        let max_scroll = (self.buffers[self.active_buffer].len() as u16)
            .saturating_sub(visible_lines);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    pub fn draw(&mut self) -> anyhow::Result<()> {
        self.adjust_scroll();

        self.stdout
            .queue(terminal::Clear(terminal::ClearType::All))?;
        self.draw_buffer()?;
        self.draw_statusline()?;

        let (screen_x, screen_y) = self.get_screen_position();
        self.stdout.queue(cursor::MoveTo(screen_x, screen_y))?;
        self.stdout.flush()?;

        Ok(())
    }

    pub fn draw_buffer(&mut self) -> anyhow::Result<()> {
        let buffer = &self.buffers[self.active_buffer];
        let visible_lines = self.visible_lines() as usize;
        let start = self.scroll_offset as usize;
        let end = (start + visible_lines).min(buffer.len());
        let syntax = self
            .syntax_set
            .find_syntax_by_extension("rs")
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);

        for (i, line) in buffer.lines[start..end].iter().enumerate() {
            if i as u16 >= self.visible_lines() {
                break;
            }
            self.stdout.queue(cursor::MoveTo(0, i as u16))?;

            let regions: Vec<(Style, &str)> = highlighter.highlight_line(line, &self.syntax_set)?;
            let highlighted_line = as_24_bit_terminal_escaped(&regions, false);

            self.stdout.queue(style::Print(highlighted_line))?;
        }

        Ok(())
    }

    pub fn draw_statusline(&mut self) -> anyhow::Result<()> {
        let mode = format!(" {:?} ", self.mode).to_uppercase();
        let file = self.buffers[self.active_buffer]
            .file
            .as_deref()
            .unwrap_or("[No File]");
        let pos = format!(" {}:{} ", self.cx, self.cy);

        let file_width = self.size.0.saturating_sub(mode.len() as u16 + pos.len() as u16 + 2);

        self.stdout.queue(cursor::MoveTo(0, self.size.1 - 2))?;
        self.stdout
            .queue(style::PrintStyledContent(mode.with(Color::Green)))?;
        self.stdout.queue(style::PrintStyledContent(
            format!("{:<width$}", file, width = file_width as usize)
                .with(Color::Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                })
                .bold()
                .on(Color::Rgb { r: 0, g: 0, b: 0 }),
        ))?;
        self.stdout.queue(style::PrintStyledContent(
            pos.with(Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            })
            .bold()
            .on(Color::Rgb { r: 0, g: 0, b: 0 }),
        ))?;

        Ok(())
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        while !self.exit {
            self.draw()?;
            if let Some(action) = self.handle_event(read()?)? {
                self.handle_action(action)?;
            }
        }

        Ok(())
    }

    fn handle_event(&mut self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        if matches!(ev, event::Event::Resize(_, _)) {
            self.size = terminal::size()?;
        }

        match self.mode {
            Mode::Normal => self.handle_normal_event(ev),
            Mode::Insert => self.handle_insert_event(ev),
        }
    }

    fn handle_normal_event(&self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        let action = match ev {
            event::Event::Key(event) => match event.code {
                event::KeyCode::Char('q') => Some(Action::Quit),
                event::KeyCode::Up | event::KeyCode::Char('k') => Some(Action::MoveUp),
                event::KeyCode::Down | event::KeyCode::Char('j') => Some(Action::MoveDown),
                event::KeyCode::Left | event::KeyCode::Char('h') => Some(Action::MoveLeft),
                event::KeyCode::Right | event::KeyCode::Char('l') => Some(Action::MoveRight),
                event::KeyCode::Char('i') => Some(Action::EnterMode(Mode::Insert)),
                event::KeyCode::Char('n') => Some(Action::NextBuffer),
                event::KeyCode::Char('p') => Some(Action::PreviousBuffer),
                _ => None,
            },
            _ => None,
        };

        Ok(action)
    }

    fn handle_insert_event(&self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        let action = match ev {
            event::Event::Key(event) => match event.code {
                event::KeyCode::Esc => Some(Action::EnterMode(Mode::Normal)),
                event::KeyCode::Enter => Some(Action::NewLine),
                event::KeyCode::Backspace => Some(Action::DeleteChar),
                event::KeyCode::Char(c) => Some(Action::AddChar(c)),
                _ => None,
            },
            _ => None,
        };

        Ok(action)
    }

    fn handle_action(&mut self, action: Action) -> anyhow::Result<()> {
        let buffer = &self.buffers[self.active_buffer];
        let max_cy = buffer.len().saturating_sub(1) as u16;

        match action {
            Action::Quit => self.exit = true,
            Action::MoveUp => {
                if self.cy > 0 {
                    self.cy = self.cy.saturating_sub(1);
                    if let Some(line) = buffer.lines.get(self.cy as usize) {
                        self.cx = self.cx.min(line.len() as u16);
                    }
                }
            }
            Action::MoveDown => {
                if self.cy < max_cy {
                    self.cy = self.cy.saturating_add(1);
                    if let Some(line) = buffer.lines.get(self.cy as usize) {
                        self.cx = self.cx.min(line.len() as u16);
                    }
                }
            }
            Action::MoveLeft => {
                if self.cx > 0 {
                    self.cx = self.cx.saturating_sub(1);
                }
            }
            Action::MoveRight => {
                if (self.cy as usize) < buffer.len() {
                    let line_length = buffer.lines[self.cy as usize].len() as u16;
                    if self.cx < line_length {
                        self.cx = self.cx.saturating_add(1);
                    }
                }
            }
            Action::EnterMode(new_mode) => {
                self.mode = new_mode;
            }
            Action::NextBuffer => {
                self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
                self.cx = 0;
                self.cy = 0;
                self.scroll_offset = 0;
            }
            Action::PreviousBuffer => {
                self.active_buffer = (self.active_buffer + self.buffers.len() - 1) % self.buffers.len();
                self.cx = 0;
                self.cy = 0;
                self.scroll_offset = 0;
            }
            Action::AddChar(c) => {
                let buffer = &mut self.buffers[self.active_buffer];
                let line_idx = self.cy as usize;

                if line_idx >= buffer.len() {
                    buffer.lines.push(String::new());
                }

                let line = &mut buffer.lines[line_idx];
                if self.cx as usize > line.len() {
                    line.push(c);
                } else {
                    line.insert(self.cx as usize, c);
                }

                self.cx = self.cx.saturating_add(1);
            }
            Action::NewLine => {
                let buffer = &mut self.buffers[self.active_buffer];
                let line_idx = self.cy as usize;
                let current_line = if line_idx < buffer.len() {
                    let line = &mut buffer.lines[line_idx];
                    let remainder = line.split_off(self.cx as usize);
                    remainder
                } else {
                    String::new()
                };

                buffer.lines.insert(line_idx + 1, current_line);
                self.cx = 0;
                self.cy = self.cy.saturating_add(1);
            }
            Action::DeleteChar => {
                let buffer = &mut self.buffers[self.active_buffer];
                let line_idx = self.cy as usize;

                if line_idx < buffer.len() {
                    if self.cx > 0 {
                        if let Some(line) = buffer.lines.get_mut(line_idx) {
                            if self.cx as usize <= line.len() {
                                line.remove(self.cx as usize - 1);
                                self.cx = self.cx.saturating_sub(1);
                            }
                        }
                    } else if line_idx > 0 {
                        let current_line = buffer.lines.remove(line_idx);
                        let prev_line = &mut buffer.lines[line_idx - 1];
                        self.cx = prev_line.len() as u16;
                        prev_line.push_str(&current_line);
                        self.cy = self.cy.saturating_sub(1);
                    }
                }
            }
        }

        Ok(())
    }
}
