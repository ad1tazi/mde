use crossterm::event::{self, Event};
use ratatui::{
    layout::{Constraint, Direction, Layout, Position},
    Frame,
};
use std::time::Duration;

use crate::clipboard::Clipboard;
use crate::editor::Editor;
use crate::input::{map_key_event, EditorCommand};
use crate::render::engine::compute_render_lines_with_width;
use crate::render::header_image::HeaderImageSupport;
use crate::render::widget::{render_status_bar, EditorWidget};

/// Snapshot of the image layout from the previous frame, used to avoid
/// re-sending image protocol escape sequences when nothing changed.
#[derive(Clone, PartialEq)]
struct ImageLayoutEntry {
    screen_row: u16,
    rows: u16,
    text: String,
    tier: u8,
}

pub struct App {
    pub editor: Editor,
    pub clipboard: Clipboard,
    pub should_quit: bool,
    pub header_image: Option<HeaderImageSupport>,
    prev_image_layout: Vec<ImageLayoutEntry>,
}

impl App {
    pub fn new(file_path: Option<&str>) -> std::io::Result<Self> {
        let editor = if let Some(path) = file_path {
            Editor::open_file(std::path::Path::new(path))?
        } else {
            Editor::new()
        };

        // Initialize image support (must happen after entering alternate screen)
        let header_image = HeaderImageSupport::new();

        Ok(Self {
            editor,
            clipboard: Clipboard::new(),
            should_quit: false,
            header_image,
            prev_image_layout: Vec::new(),
        })
    }

    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        let mut needs_redraw = true;
        loop {
            if needs_redraw {
                terminal.draw(|frame| self.render(frame))?;
                needs_redraw = false;
            }

            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key_event) => {
                        if let Some(cmd) = map_key_event(key_event) {
                            self.handle_command(cmd)?;
                            needs_redraw = true;
                        }
                    }
                    Event::Resize(_, _) => {
                        needs_redraw = true;
                    }
                    _ => {}
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
        let viewport_height = editor_area.height as usize;
        let viewport_width = editor_area.width as usize;

        // Phase 1: Prefetch render lines for a generous range around the cursor
        let cursor_line = self.editor.cursor.line;
        let total_lines = self.editor.buffer.len_lines();
        let prefetch_start = cursor_line.saturating_sub(viewport_height + 10);
        let prefetch_end = (cursor_line + viewport_height + 10).min(total_lines);
        let cursor_byte = self.editor.cursor_byte_offset();
        let all_render_lines = compute_render_lines_with_width(
            &self.editor.markdown,
            &self.editor.buffer,
            cursor_byte,
            prefetch_start,
            prefetch_end,
            viewport_width,
            self.header_image.as_mut(),
        );

        // Phase 2: Adjust scroll using wrap-aware line heights
        self.editor.adjust_scroll_wrapped(viewport_height, |line_idx| {
            if line_idx >= prefetch_start && line_idx < prefetch_end {
                all_render_lines[line_idx - prefetch_start].screen_rows(viewport_width)
            } else {
                1
            }
        });

        // Phase 3: Determine actual visible range by filling screen rows
        let visible_start = self.editor.scroll_offset;
        let mut visible_end = visible_start;
        let mut rows_filled = 0;
        while visible_end < total_lines && rows_filled < viewport_height + 5 {
            if visible_end >= prefetch_start && visible_end < prefetch_end {
                rows_filled +=
                    all_render_lines[visible_end - prefetch_start].screen_rows(viewport_width);
            } else {
                rows_filled += 1;
            }
            visible_end += 1;
        }

        // Slice the prefetched render lines to the visible range
        let slice_start = visible_start.max(prefetch_start) - prefetch_start;
        let slice_end = visible_end.min(prefetch_end) - prefetch_start;
        let render_lines = &all_render_lines[slice_start..slice_end];

        // Compute cursor screen row accounting for wrapped lines and image headers,
        // and build the current image layout for skip-rendering.
        let cursor_line_index = cursor_line.saturating_sub(visible_start);

        let mut cursor_screen_row: usize = 0;
        let mut screen_row: usize = 0;
        let mut current_image_layout = Vec::new();
        for (i, rl) in render_lines.iter().enumerate() {
            let rows = rl.screen_rows(viewport_width);

            if let Some(ref img) = rl.image_header {
                current_image_layout.push(ImageLayoutEntry {
                    screen_row: screen_row as u16,
                    rows: img.display_rows,
                    text: img.text.clone(),
                    tier: img.tier,
                });
            }

            if i < cursor_line_index {
                cursor_screen_row += rows;
            }
            screen_row += rows;
        }

        // Only re-render images when the layout changed (heading
        // added/removed/edited, scroll, resize, cursor on/off heading).
        // Otherwise mark image cells as skip so ratatui preserves them
        // without re-sending escape sequences (which causes flashing).
        let images_changed = current_image_layout != self.prev_image_layout;
        self.prev_image_layout = current_image_layout;

        // Render editor content using render lines
        let widget = EditorWidget {
            editor: &self.editor,
            render_lines,
            image_support: if images_changed { self.header_image.as_mut() } else { None },
            skip_unchanged_images: !images_changed,
        };
        frame.render_widget(widget, editor_area);

        // Render status bar
        render_status_bar(&self.editor, status_area, frame.buffer_mut());

        // Set cursor position using position map, accounting for wrapping
        let cursor_display_col = if let Some(rl) = render_lines.get(cursor_line_index) {
            rl.position_map.raw_to_display_col(self.editor.cursor.col)
        } else {
            self.editor.cursor.display_col(&self.editor.buffer)
        };

        let (cursor_wrap_row, cursor_x) = if viewport_width > 0 {
            (cursor_display_col / viewport_width, cursor_display_col % viewport_width)
        } else {
            (0, cursor_display_col)
        };
        cursor_screen_row += cursor_wrap_row;

        // Clamp cursor to viewport
        let clamped_y = cursor_screen_row.min(editor_area.height.saturating_sub(1) as usize);
        frame.set_cursor_position(Position {
            x: editor_area.x + cursor_x as u16,
            y: editor_area.y + clamped_y as u16,
        });
    }
}
