mod action;
mod buffer;
mod editor;
mod input;

use editor::Editor;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1);
    let mut editor = Editor::new(path.as_deref())?;
    editor.run()
}
