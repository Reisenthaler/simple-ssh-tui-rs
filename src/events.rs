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
            Some(Action::RsyncRemoteToLocal)
        },
        KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Tab => Some(Action::Tab),
        KeyCode::Enter => Some(Action::Enter),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Char(c) => Some(Action::Input(c)),
        _ => None,
    }
}
