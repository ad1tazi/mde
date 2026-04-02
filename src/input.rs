use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug)]
pub enum EditorCommand {
    InsertChar(char),
    InsertNewline,
    DeleteForward,
    DeleteBackward,
    MoveLeft { shift: bool },
    MoveRight { shift: bool },
    MoveUp { shift: bool },
    MoveDown { shift: bool },
    MoveHome { shift: bool },
    MoveEnd { shift: bool },
    MoveWordLeft { shift: bool },
    MoveWordRight { shift: bool },
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    Save,
    Quit,
}

pub fn map_key_event(event: KeyEvent) -> Option<EditorCommand> {
    if event.kind != KeyEventKind::Press {
        return None;
    }

    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);

    match (event.code, ctrl, shift) {
        // Quit
        (KeyCode::Char('q'), true, false) => Some(EditorCommand::Quit),

        // Save
        (KeyCode::Char('s'), true, false) => Some(EditorCommand::Save),

        // Undo / Redo
        (KeyCode::Char('z'), true, false) => Some(EditorCommand::Undo),
        (KeyCode::Char('z'), true, true) => Some(EditorCommand::Redo),
        (KeyCode::Char('y'), true, false) => Some(EditorCommand::Redo),

        // Clipboard
        (KeyCode::Char('c'), true, false) => Some(EditorCommand::Copy),
        (KeyCode::Char('x'), true, false) => Some(EditorCommand::Cut),
        (KeyCode::Char('v'), true, false) => Some(EditorCommand::Paste),

        // Select all
        (KeyCode::Char('a'), true, false) => Some(EditorCommand::SelectAll),

        // Arrow keys
        (KeyCode::Left, true, _) => Some(EditorCommand::MoveWordLeft { shift }),
        (KeyCode::Right, true, _) => Some(EditorCommand::MoveWordRight { shift }),
        (KeyCode::Left, false, _) => Some(EditorCommand::MoveLeft { shift }),
        (KeyCode::Right, false, _) => Some(EditorCommand::MoveRight { shift }),
        (KeyCode::Up, false, _) => Some(EditorCommand::MoveUp { shift }),
        (KeyCode::Down, false, _) => Some(EditorCommand::MoveDown { shift }),

        // Home / End
        (KeyCode::Home, _, _) => Some(EditorCommand::MoveHome { shift }),
        (KeyCode::End, _, _) => Some(EditorCommand::MoveEnd { shift }),

        // Editing
        (KeyCode::Enter, false, false) => Some(EditorCommand::InsertNewline),
        (KeyCode::Backspace, _, false) => Some(EditorCommand::DeleteBackward),
        (KeyCode::Delete, _, false) => Some(EditorCommand::DeleteForward),
        (KeyCode::Tab, false, false) => Some(EditorCommand::InsertChar('\t')),

        // Character input
        (KeyCode::Char(c), false, _) => Some(EditorCommand::InsertChar(c)),

        _ => None,
    }
}
