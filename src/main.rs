use crossterm::{ExecutableCommand, QueueableCommand, cursor, event, style, terminal};
use ropey::Rope;
use std::io::Write;

enum Action {
    Quit,
    Save,

    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,

    InsertChar(char),
    InsertNewline,
    Backspace,
    DeleteForward,
    SetMode(Mode),
}

enum Mode {
    Normal,
    Insert,
}

struct Buffer {
    text: Rope,
    file_path: Option<String>,
    cursor_pos: (usize, usize),
    dirty: bool,
}

impl Buffer {
    pub fn new(file_path: Option<String>, text: String) -> Self {
        let text = if text.is_empty() {
            "\n".to_string()
        } else {
            text
        };

        Self {
            file_path,
            text: Rope::from_str(&text),
            dirty: false,
            cursor_pos: (0, 0),
        }
    }

    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self::new(Some(path.to_string()), text))
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        let path = self
            .file_path
            .as_deref()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "No file path set"))?;
        std::fs::write(path, self.text.to_string())?;
        self.dirty = false;
        Ok(())
    }

    pub fn move_up(&mut self, preferred_col: usize) {
        let (row, _) = self.cursor_pos;
        if row > 0 {
            let new_row = row - 1;
            self.cursor_pos = (new_row, preferred_col.min(self.line_char_len(new_row)));
        }
    }

    pub fn move_down(&mut self, preferred_col: usize) {
        let (row, _) = self.cursor_pos;
        let last_row = self.text.len_lines().saturating_sub(1);
        if row < last_row {
            let new_row = row + 1;
            self.cursor_pos = (new_row, preferred_col.min(self.line_char_len(new_row)));
        }
    }

    pub fn move_left(&mut self) {
        let (row, col) = self.cursor_pos;
        if col > 0 {
            self.cursor_pos = (row, col - 1);
        } else if row > 0 {
            let new_row = row - 1;
            self.cursor_pos = (new_row, self.line_char_len(new_row));
        }
    }

    pub fn move_right(&mut self) {
        let (row, col) = self.cursor_pos;
        if col < self.line_char_len(row) {
            self.cursor_pos = (row, col + 1);
        } else if row + 1 < self.text.len_lines() {
            self.cursor_pos = (row + 1, 0);
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        let idx = self.cursor_char_index();
        self.text.insert_char(idx, ch);
        self.dirty = true;
        self.cursor_pos.1 += 1;
    }

    pub fn insert_newline(&mut self) {
        let idx = self.cursor_char_index();
        self.text.insert_char(idx, '\n');
        self.dirty = true;
        self.cursor_pos = (self.cursor_pos.0 + 1, 0);
    }

    pub fn backspace(&mut self) {
        let (row, col) = self.cursor_pos;
        if row == 0 && col == 0 {
            return;
        }
        let idx = self.cursor_char_index();
        self.text.remove(idx - 1..idx);
        self.dirty = true;

        if col == 0 {
            let new_row = row - 1;
            let new_col = self.line_char_len(new_row);
            self.cursor_pos = (new_row, new_col);
        } else {
            self.cursor_pos.1 -= 1;
        }
    }

    pub fn delete_forward(&mut self) {
        let idx = self.cursor_char_index();

        if idx >= self.text.len_chars() {
            return;
        }

        self.text.remove(idx..idx + 1);
        self.dirty = true;
    }

    pub fn line(&self, row: usize) -> String {
        self.text
            .line(row)
            .to_string()
            .trim_end_matches(['\r', '\n'])
            .to_string()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor_pos
    }

    pub fn line_count(&self) -> usize {
        self.text.len_lines()
    }

    pub fn file_name(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    fn cursor_char_index(&self) -> usize {
        let (row, col) = self.cursor_pos;
        self.text.line_to_char(row) + col
    }

    fn line_char_len(&self, row: usize) -> usize {
        let line = self.text.line(row);
        let mut len = line.len_chars();
        if len > 0 && line.char(len - 1) == '\n' {
            len -= 1;
            if len > 0 && line.char(len - 1) == '\r' {
                len -= 1;
            }
        }
        len
    }
}

struct Editor {
    stdout: std::io::Stdout,
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

            if let Some(action) = self.handle_event(event::read()?)? {
                match action {
                    Action::Quit => break,

                    Action::Save => match self.buffer.save() {
                        Ok(()) => {
                            self.status_message = Some("Saved!".to_string());
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Save failed: {}", e));
                        }
                    },

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
            }
        }
        Ok(())
    }

    fn handle_event(&mut self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        match self.mode {
            Mode::Normal => self.handle_normal_event(ev),
            Mode::Insert => self.handle_insert_event(ev),
        }
    }

    fn handle_normal_event(&mut self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        let action = match ev {
            event::Event::Key(event::KeyEvent { code, .. }) => match code {
                event::KeyCode::Char('q') => Some(Action::Quit),
                event::KeyCode::Char('s') => Some(Action::Save),
                event::KeyCode::Char('h') | event::KeyCode::Left => Some(Action::MoveLeft),
                event::KeyCode::Char('j') | event::KeyCode::Down => Some(Action::MoveDown),
                event::KeyCode::Char('k') | event::KeyCode::Up => Some(Action::MoveUp),
                event::KeyCode::Char('l') | event::KeyCode::Right => Some(Action::MoveRight),
                event::KeyCode::Char('i') => Some(Action::SetMode(Mode::Insert)),
                event::KeyCode::Char('x') => Some(Action::DeleteForward),
                event::KeyCode::Char('d') => Some(Action::Backspace),
                _ => None,
            },
            _ => None,
        };
        Ok(action)
    }

    fn handle_insert_event(&mut self, ev: event::Event) -> anyhow::Result<Option<Action>> {
        let action = match ev {
            event::Event::Key(event::KeyEvent { code, .. }) => match code {
                event::KeyCode::Esc => Some(Action::SetMode(Mode::Normal)),
                event::KeyCode::Enter => Some(Action::InsertNewline),
                event::KeyCode::Backspace => Some(Action::Backspace),
                event::KeyCode::Delete => Some(Action::DeleteForward),
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
