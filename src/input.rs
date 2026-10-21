use crossterm::event::{Event, KeyCode, KeyEventKind};

use crate::action::{Action, Mode};

pub fn handle_event(mode: Mode, event: Event) -> Option<Action> {
    let Event::Key(key) = event else {
        return None;
    };

    if key.kind != KeyEventKind::Press {
        return None;
    }

    match mode {
        Mode::Normal => match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Char('s') => Some(Action::Save),

            KeyCode::Char('h') | KeyCode::Left => Some(Action::MoveLeft),
            KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
            KeyCode::Char('l') | KeyCode::Right => Some(Action::MoveRight),

            KeyCode::Char('i') => Some(Action::SetMode(Mode::Insert)),
            KeyCode::Char('x') => Some(Action::DeleteForward),
            KeyCode::Char('d') => Some(Action::Backspace),

            _ => None,
        },

        Mode::Insert => match key.code {
            KeyCode::Esc => Some(Action::SetMode(Mode::Normal)),
            KeyCode::Enter => Some(Action::InsertNewline),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Delete => Some(Action::DeleteForward),
            KeyCode::Char(c) => Some(Action::InsertChar(c)),

            _ => None,
        },
    }
}
