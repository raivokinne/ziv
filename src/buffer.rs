use std::path::Path;

pub struct Buffer {
    pub file: Option<String>,
    pub lines: Vec<String>,
}

impl Buffer {
    pub fn new(file: Option<String>, contents: String) -> Self {
        let lines = contents.lines().map(|s| s.to_string()).collect();
        Self {
            file,
            lines,
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn get_line(&self, idx: usize) -> &str {
        &self.lines[idx]
    }

    pub fn insert_char(&mut self, cx: usize, cy: usize, c: char) {
        self.lines[cy].insert(cx, c);
    }

    pub fn remove_char(&mut self, cx: usize, cy: usize) {
        self.lines[cy].remove(cx);
    }

    pub fn from_file(file: Option<String>) -> anyhow::Result<Self> {
        match &file {
            Some(file) => {
                let path = Path::new(file);
                if !path.exists() {
                    return Err(anyhow::anyhow!("file {:?} not found", file));
                }
                let contents = std::fs::read_to_string(file)?;
                Ok(Self::new(Some(file.to_string()), contents.to_string()))
            }
            None => Ok(Self::new(file, "\n".to_string())),
        }
    }
}

