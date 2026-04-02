use crossterm::event::{self, Event};
use ratatui::{
    layout::{Constraint, Direction, Layout, Position},
    Frame,
};
use std::time::Duration;

use crate::clipboard::Clipboard;
use crate::editor::Editor;
use crate::input::{map_key_event, EditorCommand};
use crate::render::{render_status_bar, EditorWidget};

pub struct App {
    pub editor: Editor,
    pub clipboard: Clipboard,
    pub should_quit: bool,
}

impl App {
    pub fn new(file_path: Option<&str>) -> std::io::Result<Self> {
        let editor = if let Some(path) = file_path {
            Editor::open_file(std::path::Path::new(path))?
        } else {
            Editor::new()
        };

        Ok(Self {
            editor,
            clipboard: Clipboard::new(),
            should_quit: false,
        })
    }

    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key_event) = event::read()? {
                    if let Some(cmd) = map_key_event(key_event) {
                        self.handle_command(cmd)?;
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn handle_command(&mut self, cmd: EditorCommand) -> std::io::Result<()> {
        match cmd {
            EditorCommand::Quit => self.should_quit = true,
            EditorCommand::Save => self.editor.save()?,
            EditorCommand::InsertChar(c) => self.editor.insert_char(c),
            EditorCommand::InsertNewline => self.editor.insert_newline(),
            EditorCommand::DeleteForward => self.editor.delete_forward(),
            EditorCommand::DeleteBackward => self.editor.delete_backward(),
            EditorCommand::MoveLeft { shift } => self.editor.move_left(shift),
            EditorCommand::MoveRight { shift } => self.editor.move_right(shift),
            EditorCommand::MoveUp { shift } => self.editor.move_up(shift),
            EditorCommand::MoveDown { shift } => self.editor.move_down(shift),
            EditorCommand::MoveHome { shift } => self.editor.move_home(shift),
            EditorCommand::MoveEnd { shift } => self.editor.move_end(shift),
            EditorCommand::MoveWordLeft { shift } => self.editor.move_word_left(shift),
            EditorCommand::MoveWordRight { shift } => self.editor.move_word_right(shift),
            EditorCommand::SelectAll => self.editor.select_all(),
            EditorCommand::Copy => self.editor.copy(&mut self.clipboard),
            EditorCommand::Cut => self.editor.cut(&mut self.clipboard),
            EditorCommand::Paste => self.editor.paste(&mut self.clipboard),
            EditorCommand::Undo => self.editor.undo(),
            EditorCommand::Redo => self.editor.redo(),
        }
        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(1)])
            .split(frame.area());

        let editor_area = chunks[0];
        let status_area = chunks[1];

        // Adjust scroll before rendering
        self.editor.adjust_scroll(editor_area.height as usize);

        // Render editor content
        let widget = EditorWidget {
            editor: &self.editor,
        };
        frame.render_widget(widget, editor_area);

        // Render status bar
        render_status_bar(&self.editor, status_area, frame.buffer_mut());

        // Set cursor position
        let cursor_screen_line = self
            .editor
            .cursor
            .line
            .saturating_sub(self.editor.scroll_offset);
        let cursor_display_col = self.editor.cursor.display_col(&self.editor.buffer);

        frame.set_cursor_position(Position {
            x: editor_area.x + cursor_display_col as u16,
            y: editor_area.y + cursor_screen_line as u16,
        });
    }
}
