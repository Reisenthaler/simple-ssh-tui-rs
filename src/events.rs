use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::Action;

pub fn key_to_action(key_event: KeyEvent) -> Option<Action> {
    match key_event.code {
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::Quit)
        }
        KeyCode::Char('r') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::ToggleAppMode)
        },
        KeyCode::Char('d') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::Download)
        },
        KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::DownloadSync)
        },
        KeyCode::Char('u') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::Upload)
        },
        KeyCode::Char('z') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::UploadSync)
        },
        
        KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Left => Some(Action::MoveLeft),
        KeyCode::Right => Some(Action::MoveRight),
        KeyCode::Tab => Some(Action::Tab),
        KeyCode::Enter => Some(Action::Enter),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Char(c) => Some(Action::Input(c)),
        _ => None,
    }
}
