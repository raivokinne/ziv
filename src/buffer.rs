use anyhow::Context;
use std::path::PathBuf;

pub struct Buffer {
    pub file: Option<PathBuf>,
    pub lines: Vec<String>,
    pub is_modified: bool,
}

impl Buffer {
    pub fn new(file: Option<impl Into<PathBuf>>, contents: &str) -> Self {
        let lines = if contents.is_empty() {
            vec![String::new()]
        } else {
            contents.lines().map(String::from).collect()
        };

        Self {
            file: file.map(Into::into),
            lines,
            is_modified: false,
        }
    }

    pub fn insert_new_line(&mut self, cy: usize, _cx: usize) {
        self.lines.insert(cy, String::new());
        self.is_modified = true;
    }

    pub fn from_file(file: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = file.into();

        if !path.exists() {
            return Ok(Self {
                file: Some(path),
                lines: vec![String::new()],
                is_modified: false,
            });
        }

        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read file: {:?}", path))?;

        Ok(Self {
            file: Some(path),
            lines: if contents.is_empty() {
                vec![String::new()]
            } else {
                contents.lines().map(String::from).collect()
            },
            is_modified: false,
        })
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        if let Some(path) = &self.file {
            let contents = self.lines.join("\n");
            std::fs::write(path, contents)
                .with_context(|| format!("Failed to save file: {:?}", path))?;
            self.is_modified = false;
            Ok(())
        } else {
            Err(anyhow::anyhow!("No file associated with this buffer"))
        }
    }

    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> anyhow::Result<()> {
        let path = path.into();
        let contents = self.lines.join("\n");
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to save file: {:?}", path))?;
        self.file = Some(path);
        self.is_modified = false;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn get_line(&self, idx: usize) -> &str {
        &self.lines[idx]
    }

    pub fn insert_char(&mut self, cx: usize, cy: usize, c: char) -> anyhow::Result<()> {
        if cy >= self.lines.len() {
            return Err(anyhow::anyhow!("Invalid line index: {}", cy));
        }

        let line = &mut self.lines[cy];
        if cx > line.len() {
            return Err(anyhow::anyhow!("Invalid column index: {}", cx));
        }

        line.insert(cx, c);
        Ok(())
    }

    pub fn remove_char(&mut self, cx: usize, cy: usize) -> anyhow::Result<()> {
        if cy >= self.lines.len() {
            return Err(anyhow::anyhow!("Invalid line index: {}", cy));
        }

        let line = &mut self.lines[cy];
        if cx >= line.len() {
            return Err(anyhow::anyhow!("Invalid column index: {}", cx));
        }

        line.remove(cx);
        self.is_modified = true;
        Ok(())
    }

    pub fn remove_line(&mut self, cy: usize) -> anyhow::Result<String> {
        if cy >= self.lines.len() {
            return Err(anyhow::anyhow!("Invalid line index: {}", cy));
        }

        if self.lines.len() == 1 {
            let line = std::mem::take(&mut self.lines[0]);
            self.is_modified = true;
            return Ok(line);
        }

        self.is_modified = true;
        Ok(self.lines.remove(cy))
    }

    pub fn file_name(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(String::from)
    }
}

