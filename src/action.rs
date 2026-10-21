#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
}

pub enum Action {
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
