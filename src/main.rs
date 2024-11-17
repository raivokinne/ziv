mod buffer;
mod editor;

use buffer::Buffer;
use editor::Editor;

fn main() -> anyhow::Result<()> {
    let files = std::env::args();
    let mut buffers: Vec<Buffer> = Vec::new();

    if files.len() < 2 {
        let buffer = Buffer::new(Some("Untitled"), "\n");
        buffers.push(buffer);
    } else {
        for file in files.skip(1) {
            let buffer = Buffer::from_file(file)?;
            buffers.push(buffer);
        }
    }

    let mut editor = Editor::new(buffers)?;
    editor.run()
}