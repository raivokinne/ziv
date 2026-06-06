use crossterm::{
    cursor,
    event,
    style,
    terminal,
    ExecutableCommand,
    QueueableCommand,
};
use std::io::Write;
use ropey::Rope;

enum Action {
    Quit,
    Save,

    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,

    InsertChar(char),
    InsertNewline,
    DeleteChar,
    SwitchMode(Mode),
}

enum Mode {
    Normal,
    Insert,
}

struct Buffer {
    contents: Rope,
    file: Option<String>,
    pos: (usize, usize),
    dirty: bool,
}

impl Buffer {
    pub fn new(file: Option<String>, contents: String) -> Self {
        let contents = if contents.is_empty() {
            "\n".to_string()
        } else {
            contents
        };


        Self {
            file,
            contents: Rope::from_str(&contents),
            dirty: false,
            pos: (0, 0),
        }
    }

    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        Ok(Self::new(Some(path.to_string()), contents))
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        let path = self.file.as_deref().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "No file path set")
        })?;
        std::fs::write(path, self.contents.to_string())?;
        self.dirty = false;
        Ok(())
    }

    pub fn move_up(&mut self) {
        let (row, col) = self.pos;
        if row > 0 {
            let new_row = row - 1;
            self.pos = (new_row, col.min(self.line_len(new_row)));
        }
    }

    pub fn move_down(&mut self) {
        let (row, col) = self.pos;
        let last_row = self.contents.len_lines().saturating_sub(1);
        if row < last_row {
            let new_row = row + 1;
            self.pos = (new_row, col.min(self.line_len(new_row)));
        }
    }

    pub fn move_left(&mut self) {
        let (row, col) = self.pos;
        if col > 0 {
            self.pos = (row, col - 1);
        } else if row > 0 {
            let new_row = row - 1;
            self.pos = (new_row, self.line_len(new_row));
        }
    }

    pub fn move_right(&mut self) {
        let (row, col) = self.pos;
        if col < self.line_len(row); {
            self.pos = (row, col + 1);
        } else if row + 1 < self.contents.len_lines() {
            self.pos = (row + 1, 0);
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        let idx = self.char_idx();
        self.contents.insert_char(idx, ch);
        self.dirty = true;
        self.pos.1 += 1;
    }

    pub fn insert_newline(&mut self) {
        let idx = self.char_idx();
        self.contents.insert_char(idx, '\n');
        self.dirty = true;
        self.pos = (self.pos.0 + 1, 0);
    }

    pub fn delete_char_before(&mut self) {
        let (row, col) = self.pos;
        if row == 0 && col == 0 {
            return;
        }
        let idx = self.char_idx();
        self.contents.remove(idx - 1..idx);
        self.dirty = true;

        if col == 0 {
            let new_row = row - 1;
            let new_col = self.line_len(new_row);
            self.pos = (new_row, new_col);
        } else {
            self.pos.1 -= 1;
        }
    }

    pub fn line(&self, row: usize) -> String {
        self.contents
            .line(row)
            .to_string()
            .trim_end_matches('\n')
            .to_string()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.pos
    }

    pub fn line_count(&self) -> usize {
        self.contents.len_lines()
    }

    pub fn file_name(&self) -> Option<&str> {
        self.file.as_deref()
    }

    fn char_idx(&self) -> usize {
        let (row, col) = self.pos;
        self.contents.line_to_char(row) + col
    }

    fn line_len(&self, row: usize) -> usize {
        let line = self.contents.line(row).to_string();
        if line.ends_with('\n') {
            line.len() - 1
        } else {
            line.len()
        }
    }
}

struct Editor {
    stdout: std::io::Stdout,
    size: (u16, u16),
    dx: usize,
    mode: Mode,
    buffer: Buffer,
    status_msg: Option<String>,
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
            size: terminal::size()?,
            dx: 0,
            mode: Mode::Normal,
            buffer,
            status_msg: None,
        })
    }

    pub fn draw(&mut self) -> anyhow::Result<()> {
        self.stdout
            .queue(terminal::Clear(terminal::ClearType::All))?;

        let viewport_rows = self.size.1.saturating_sub(2) as usize;

        for i in 0..viewport_rows {
            self.stdout.queue(cursor::MoveTo(0, i as u16))?;
            if i < self.buffer.line_count() {
                self.stdout
                    .queue(style::Print(self.buffer.line(i)))?;
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
        self.stdout
            .queue(cursor::MoveTo(col as u16, row as u16))?;

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
        let file_name = self
            .buffer
            .file_name()
            .unwrap_or("[No Name]");
        let (row, col) = self.buffer.cursor();
        let position = format!(" {}:{} ", row + 1, col + 1);
        let file_part = format!(" {}{} ", file_name, dirty_flag);

        let left_len = mode_str.len();
        let right_len = file_part.len() + position.len();
        let padding_len = (self.size.0 as usize)
            .saturating_sub(left_len + right_len);
        let padding = " ".repeat(padding_len);

        self.stdout
            .queue(cursor::MoveTo(0, self.size.1 - 2))?;

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
            .queue(cursor::MoveTo(0, self.size.1 - 1))?
            .queue(terminal::Clear(terminal::ClearType::CurrentLine))?;

        if let Some(msg) = &self.status_msg {
            self.stdout.queue(style::Print(msg))?;
        }
        Ok(())
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            self.draw()?;

            if let Some(action) = self.handle_event(event::read()?)? {
                match action {
                    Action::Quit => break,

                    Action::Save => {
                        match self.buffer.save() {
                            Ok(()) => {
                                self.status_msg = Some("Saved!".to_string());
                            }
                            Err(e) => {
                                self.status_msg =
                                    Some(format!("Save failed: {}", e));
                            }
                        }
                    }

                    Action::MoveUp => {
                        self.buffer.pos.1 = self.dx;
                        self.buffer.move_up();
                    }
                    Action::MoveDown => {
                        self.buffer.pos.1 = self.dx;
                        self.buffer.move_down();
                    }

                    Action::MoveLeft => {
                        self.buffer.move_left();
                        self.dx = self.buffer.cursor().1;
                    }
                    Action::MoveRight => {
                        self.buffer.move_right();
                        self.dx = self.buffer.cursor().1;
                    }

                    Action::SwitchMode(mode) => {
                        self.mode = mode;
                        self.status_msg = None;
                    }

                    Action::InsertChar(c) => {
                        self.buffer.insert_char(c);
                        self.dx = self.buffer.cursor().1;
                    }
                    Action::InsertNewline => {
                        self.buffer.insert_newline();
                        self.dx = 0;
                    }
                    Action::DeleteChar => {
                        self.buffer.delete_char_before();
                        self.dx = self.buffer.cursor().1;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_event(
        &mut self,
        ev: event::Event,
    ) -> anyhow::Result<Option<Action>> {
        match self.mode {
            Mode::Normal => self.handle_normal_event(ev),
            Mode::Insert => self.handle_insert_event(ev),
        }
    }

    fn handle_normal_event(
        &mut self,
        ev: event::Event,
    ) -> anyhow::Result<Option<Action>> {
        let action = match ev {
            event::Event::Key(event::KeyEvent { code, .. }) => match code {
                event::KeyCode::Char('q') => Some(Action::Quit),
                event::KeyCode::Char('s') => Some(Action::Save),
                event::KeyCode::Char('h') | event::KeyCode::Left => {
                    Some(Action::MoveLeft)
                }
                event::KeyCode::Char('j') | event::KeyCode::Down => {
                    Some(Action::MoveDown)
                }
                event::KeyCode::Char('k') | event::KeyCode::Up => {
                    Some(Action::MoveUp)
                }
                event::KeyCode::Char('l') | event::KeyCode::Right => {
                    Some(Action::MoveRight)
                }
                event::KeyCode::Char('i') => {
                    Some(Action::SwitchMode(Mode::Insert))
                }
                _ => None,
            },
            _ => None,
        };
        Ok(action)
    }

    fn handle_insert_event(
        &mut self,
        ev: event::Event,
    ) -> anyhow::Result<Option<Action>> {
        let action = match ev {
            event::Event::Key(event::KeyEvent { code, .. }) => match code {
                event::KeyCode::Esc => Some(Action::SwitchMode(Mode::Normal)),
                event::KeyCode::Enter => Some(Action::InsertNewline),
                event::KeyCode::Backspace => Some(Action::DeleteChar),
                event::KeyCode::Char(c) => Some(Action::InsertChar(c)),
                _ => None,
            },
            _ => None,
        };
        Ok(action)
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = self.stdout.execute(terminal::LeaveAlternateScreen);
    }
}

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1);
    let mut editor = Editor::new(path.as_deref())?;
    editor.run()
}
