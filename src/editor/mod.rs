pub mod buffer;
pub mod cursor;
pub mod selection;
pub mod undo;

use std::path::PathBuf;

use buffer::Buffer;
use cursor::Cursor;
use selection::Selection;
use undo::UndoStack;

use crate::clipboard::Clipboard;
use crate::markdown::{self, MarkdownState};

pub struct Editor {
    pub buffer: Buffer,
    pub cursor: Cursor,
    pub selection: Option<Selection>,
    pub undo_stack: UndoStack,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
    pub scroll_offset: usize,
    pub markdown: MarkdownState,
}

impl Editor {
    pub fn new() -> Self {
        let buffer = Buffer::new();
        let mut markdown = MarkdownState::new();
        markdown.parse_full(&buffer);
        Self {
            buffer,
            cursor: Cursor::new(),
            selection: None,
            undo_stack: UndoStack::new(),
            file_path: None,
            dirty: false,
            scroll_offset: 0,
            markdown,
        }
    }

    pub fn open_file(path: &std::path::Path) -> std::io::Result<Self> {
        let buffer = Buffer::from_file(path)?;
        let mut markdown = MarkdownState::new();
        markdown.parse_full(&buffer);
        Ok(Self {
            buffer,
            cursor: Cursor::new(),
            selection: None,
            undo_stack: UndoStack::new(),
            file_path: Some(path.to_path_buf()),
            dirty: false,
            scroll_offset: 0,
            markdown,
        })
    }

    pub fn cursor_byte_offset(&self) -> usize {
        self.buffer.line_col_to_byte(self.cursor.line, self.cursor.col)
    }

    // --- Editing ---

    pub fn insert_char(&mut self, ch: char) {
        self.delete_selection_if_active();
        let cursor_before = self.cursor.position();
        let char_pos = self.buffer.line_col_to_char_idx(self.cursor.line, self.cursor.col);
        let op = self.buffer.insert_char(self.cursor.line, self.cursor.col, ch);
        if ch == '\n' {
            self.cursor.line += 1;
            self.cursor.col = 0;
        } else {
            self.cursor.col += 1;
        }
        self.cursor.reset_desired_col();

        let text = ch.to_string();
        let edit = markdown::input_edit_for_insert(&self.buffer, char_pos, &text);
        self.markdown.apply_edit(edit, &self.buffer);

        let cursor_after = self.cursor.position();
        self.undo_stack.record(op, cursor_before, cursor_after);
        self.dirty = true;
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn delete_forward(&mut self) {
        if self.delete_selection_if_active() {
            return;
        }
        let cursor_before = self.cursor.position();
        let char_pos = self.buffer.line_col_to_char_idx(self.cursor.line, self.cursor.col);
        if let Some(ref op) = self
            .buffer
            .delete_char_forward(self.cursor.line, self.cursor.col)
        {
            let deleted_text = match op {
                undo::Operation::Delete { text, .. } => text.clone(),
                _ => unreachable!(),
            };
            let edit = markdown::input_edit_for_delete(&self.buffer, char_pos, &deleted_text);
            self.markdown.apply_edit(edit, &self.buffer);

            self.cursor.clamp(&self.buffer);
            let cursor_after = self.cursor.position();
            self.undo_stack.record(op.clone(), cursor_before, cursor_after);
            self.dirty = true;
        }
    }

    pub fn delete_backward(&mut self) {
        if self.delete_selection_if_active() {
            return;
        }
        let cursor_before = self.cursor.position();
        if self.cursor.line == 0 && self.cursor.col == 0 {
            return;
        }
        // Calculate new cursor position before the delete
        let new_line;
        let new_col;
        if self.cursor.col > 0 {
            new_line = self.cursor.line;
            new_col = self.cursor.col - 1;
        } else {
            new_line = self.cursor.line - 1;
            new_col = self.buffer.line_len_chars(self.cursor.line - 1);
        }

        if let Some(ref op) = self
            .buffer
            .delete_char_backward(self.cursor.line, self.cursor.col)
        {
            let deleted_text = match op {
                undo::Operation::Delete { text, .. } => text.clone(),
                _ => unreachable!(),
            };
            // char_pos after deletion is the position where the char was removed
            let char_pos = self.buffer.line_col_to_char_idx(new_line, new_col);
            let edit = markdown::input_edit_for_delete(&self.buffer, char_pos, &deleted_text);
            self.markdown.apply_edit(edit, &self.buffer);

            self.cursor.line = new_line;
            self.cursor.col = new_col;
            self.cursor.reset_desired_col();
            let cursor_after = self.cursor.position();
            self.undo_stack.record(op.clone(), cursor_before, cursor_after);
            self.dirty = true;
        }
    }

    fn delete_selection_if_active(&mut self) -> bool {
        if let Some(sel) = self.selection.take() {
            if sel.is_empty() {
                return false;
            }
            let (start, end) = sel.ordered();
            let cursor_before = self.cursor.position();
            let char_pos = self.buffer.line_col_to_char_idx(start.0, start.1);
            if let Some(ref op) = self.buffer.delete_range(start, end) {
                let deleted_text = match op {
                    undo::Operation::Delete { text, .. } => text.clone(),
                    _ => unreachable!(),
                };
                let edit = markdown::input_edit_for_delete(&self.buffer, char_pos, &deleted_text);
                self.markdown.apply_edit(edit, &self.buffer);

                self.cursor.line = start.0;
                self.cursor.col = start.1;
                self.cursor.reset_desired_col();
                let cursor_after = self.cursor.position();
                self.undo_stack.seal();
                self.undo_stack.record(op.clone(), cursor_before, cursor_after);
                self.undo_stack.seal();
                self.dirty = true;
            }
            return true;
        }
        false
    }

    // --- Movement ---

    fn update_selection(&mut self, shift: bool, old_pos: (usize, usize)) {
        if shift {
            if self.selection.is_none() {
                self.selection = Some(Selection::new(old_pos, self.cursor.position()));
            } else {
                self.selection.as_mut().unwrap().head = self.cursor.position();
            }
        } else if let Some(sel) = self.selection.take() {
            // Move cursor to the appropriate end of selection
            if !sel.is_empty() {
                let (start, end) = sel.ordered();
                // For left/up movements, cursor goes to start; for right/down, to end
                // But since we already moved, we just clear the selection
                // The caller should handle this case
                let _ = (start, end);
            }
        }
    }

    pub fn move_left(&mut self, shift: bool) {
        self.undo_stack.seal();
        let old_pos = self.cursor.position();
        if !shift {
            if let Some(sel) = self.selection.take() {
                if !sel.is_empty() {
                    let (start, _) = sel.ordered();
                    self.cursor.line = start.0;
                    self.cursor.col = start.1;
                    self.cursor.reset_desired_col();
                    return;
                }
            }
        }
        self.cursor.move_left(&self.buffer);
        self.update_selection(shift, old_pos);
    }

    pub fn move_right(&mut self, shift: bool) {
        self.undo_stack.seal();
        let old_pos = self.cursor.position();
        if !shift {
            if let Some(sel) = self.selection.take() {
                if !sel.is_empty() {
                    let (_, end) = sel.ordered();
                    self.cursor.line = end.0;
                    self.cursor.col = end.1;
                    self.cursor.reset_desired_col();
                    return;
                }
            }
        }
        self.cursor.move_right(&self.buffer);
        self.update_selection(shift, old_pos);
    }

    pub fn move_up(&mut self, shift: bool) {
        self.undo_stack.seal();
        let old_pos = self.cursor.position();
        if !shift {
            self.selection = None;
        }
        self.cursor.move_up(&self.buffer);
        self.update_selection(shift, old_pos);
    }

    pub fn move_down(&mut self, shift: bool) {
        self.undo_stack.seal();
        let old_pos = self.cursor.position();
        if !shift {
            self.selection = None;
        }
        self.cursor.move_down(&self.buffer);
        self.update_selection(shift, old_pos);
    }

    pub fn move_home(&mut self, shift: bool) {
        self.undo_stack.seal();
        let old_pos = self.cursor.position();
        if !shift {
            self.selection = None;
        }
        self.cursor.move_home();
        self.update_selection(shift, old_pos);
    }

    pub fn move_end(&mut self, shift: bool) {
        self.undo_stack.seal();
        let old_pos = self.cursor.position();
        if !shift {
            self.selection = None;
        }
        self.cursor.move_end(&self.buffer);
        self.update_selection(shift, old_pos);
    }

    pub fn move_word_left(&mut self, shift: bool) {
        self.undo_stack.seal();
        let old_pos = self.cursor.position();
        if !shift {
            self.selection = None;
        }
        self.cursor.move_word_left(&self.buffer);
        self.update_selection(shift, old_pos);
    }

    pub fn move_word_right(&mut self, shift: bool) {
        self.undo_stack.seal();
        let old_pos = self.cursor.position();
        if !shift {
            self.selection = None;
        }
        self.cursor.move_word_right(&self.buffer);
        self.update_selection(shift, old_pos);
    }

    // --- Selection ---

    pub fn select_all(&mut self) {
        let last_line = self.buffer.len_lines().saturating_sub(1);
        let last_col = self.buffer.line_len_chars(last_line);
        self.selection = Some(Selection::new((0, 0), (last_line, last_col)));
        self.cursor.line = last_line;
        self.cursor.col = last_col;
        self.cursor.reset_desired_col();
    }

    // --- Clipboard ---

    pub fn copy(&self, clipboard: &mut Clipboard) {
        if let Some(ref sel) = self.selection {
            let text = sel.selected_text(&self.buffer);
            if !text.is_empty() {
                clipboard.set_text(&text);
            }
        }
    }

    pub fn cut(&mut self, clipboard: &mut Clipboard) {
        self.copy(clipboard);
        self.delete_selection_if_active();
    }

    pub fn paste(&mut self, clipboard: &mut Clipboard) {
        let text = clipboard.get_text();
        if text.is_empty() {
            return;
        }
        self.delete_selection_if_active();
        let cursor_before = self.cursor.position();
        let char_pos = self.buffer.line_col_to_char_idx(self.cursor.line, self.cursor.col);
        let op = self
            .buffer
            .insert_str(self.cursor.line, self.cursor.col, &text);

        let edit = markdown::input_edit_for_insert(&self.buffer, char_pos, &text);
        self.markdown.apply_edit(edit, &self.buffer);

        // Move cursor to end of pasted text
        let (end_line, end_col) = self.buffer.char_idx_to_line_col(
            self.buffer.line_col_to_char_idx(cursor_before.0, cursor_before.1)
                + text.chars().count(),
        );
        self.cursor.line = end_line;
        self.cursor.col = end_col;
        self.cursor.reset_desired_col();

        let cursor_after = self.cursor.position();
        self.undo_stack.seal();
        self.undo_stack.record(op, cursor_before, cursor_after);
        self.undo_stack.seal();
        self.dirty = true;
    }

    // --- Undo/Redo ---

    pub fn undo(&mut self) {
        self.selection = None;
        if let Some(group) = self.undo_stack.undo() {
            for op in group.ops.iter().rev() {
                self.buffer.reverse(op);
            }
            self.cursor.line = group.cursor_before.0;
            self.cursor.col = group.cursor_before.1;
            self.cursor.reset_desired_col();
            self.markdown.parse_full(&self.buffer);
            self.dirty = true;
        }
    }

    pub fn redo(&mut self) {
        self.selection = None;
        if let Some(group) = self.undo_stack.redo() {
            for op in &group.ops {
                self.buffer.apply(op);
            }
            self.cursor.line = group.cursor_after.0;
            self.cursor.col = group.cursor_after.1;
            self.cursor.reset_desired_col();
            self.markdown.parse_full(&self.buffer);
            self.dirty = true;
        }
    }

    // --- File I/O ---

    pub fn save(&mut self) -> std::io::Result<()> {
        if let Some(ref path) = self.file_path {
            self.buffer.save_to_file(path)?;
            self.dirty = false;
        }
        Ok(())
    }

    // --- Viewport ---

    const SCROLL_MARGIN: usize = 3;

    #[allow(dead_code)]
    pub fn adjust_scroll(&mut self, viewport_height: usize) {
        self.adjust_scroll_wrapped(viewport_height, |_| 1);
    }

    /// Adjust scroll offset to keep the cursor visible, accounting for
    /// wrapped lines. `line_screen_rows` returns how many screen rows
    /// a given buffer line occupies.
    pub fn adjust_scroll_wrapped<F>(&mut self, viewport_height: usize, line_screen_rows: F)
    where
        F: Fn(usize) -> usize,
    {
        if viewport_height == 0 {
            return;
        }

        // Ensure scroll_offset doesn't exceed cursor line
        if self.scroll_offset > self.cursor.line {
            self.scroll_offset = self.cursor.line;
        }

        // Scroll up to provide SCROLL_MARGIN screen rows above cursor
        let mut rows_above = 0;
        for line in self.scroll_offset..self.cursor.line {
            rows_above += line_screen_rows(line);
        }
        if rows_above < Self::SCROLL_MARGIN {
            while self.scroll_offset > 0 && rows_above < Self::SCROLL_MARGIN {
                self.scroll_offset -= 1;
                rows_above += line_screen_rows(self.scroll_offset);
            }
        }

        // Scroll down if cursor line (plus margin) doesn't fit in viewport
        loop {
            let mut rows_from_scroll = 0;
            for line in self.scroll_offset..=self.cursor.line {
                rows_from_scroll += line_screen_rows(line);
            }
            // Add margin below: count screen rows for SCROLL_MARGIN lines after cursor
            let mut margin_below = 0;
            let mut margin_line = self.cursor.line + 1;
            let total_lines = self.buffer.len_lines();
            while margin_line < total_lines && margin_below < Self::SCROLL_MARGIN {
                margin_below += line_screen_rows(margin_line);
                margin_line += 1;
            }
            let needed = rows_from_scroll + margin_below;
            if needed <= viewport_height || self.scroll_offset >= self.cursor.line {
                break;
            }
            self.scroll_offset += 1;
        }

        let max_scroll = self.buffer.len_lines().saturating_sub(1);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    #[allow(dead_code)]
    pub fn visible_lines(&self, viewport_height: usize) -> std::ops::Range<usize> {
        let start = self.scroll_offset;
        let end = (start + viewport_height).min(self.buffer.len_lines());
        start..end
    }
}
