use ropey::Rope;

pub struct Buffer {
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
