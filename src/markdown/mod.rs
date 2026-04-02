pub mod highlight;

use tree_sitter::{InputEdit, Point};
use tree_sitter_md::{MarkdownParser, MarkdownTree};

use crate::editor::buffer::Buffer;

pub struct MarkdownState {
    parser: MarkdownParser,
    tree: Option<MarkdownTree>,
}

impl MarkdownState {
    pub fn new() -> Self {
        Self {
            parser: MarkdownParser::default(),
            tree: None,
        }
    }

    /// Full parse of the buffer. Called on file open or after undo/redo.
    pub fn parse_full(&mut self, buffer: &Buffer) {
        let source = buffer.contents();
        self.tree = self.parser.parse(source.as_bytes(), None);
    }

    /// Incremental reparse after an edit.
    pub fn apply_edit(&mut self, edit: InputEdit, buffer: &Buffer) {
        if let Some(ref mut tree) = self.tree {
            tree.edit(&edit);
        }
        let source = buffer.contents();
        self.tree = self.parser.parse(source.as_bytes(), self.tree.as_ref());
    }

    pub fn tree(&self) -> Option<&MarkdownTree> {
        self.tree.as_ref()
    }

    pub fn compute_highlights(
        &self,
        buffer: &Buffer,
        visible_line_start: usize,
        visible_line_end: usize,
        cursor_byte: usize,
    ) -> highlight::HighlightMap {
        highlight::compute(self, buffer, visible_line_start, visible_line_end, cursor_byte)
    }
}

/// Build InputEdit for an insertion. Call AFTER the buffer mutation.
pub fn input_edit_for_insert(
    buffer: &Buffer,
    char_pos: usize,
    inserted_text: &str,
) -> InputEdit {
    let start_byte = buffer.char_to_byte(char_pos);
    let new_end_byte = start_byte + inserted_text.len();

    let (start_line, _start_col) = buffer.char_idx_to_line_col(char_pos);
    let start_line_byte_start = buffer.line_to_byte(start_line);
    let start_col_byte = start_byte - start_line_byte_start;

    let end_char = char_pos + inserted_text.chars().count();
    let (end_line, _end_col) = buffer.char_idx_to_line_col(end_char);
    let end_byte = buffer.char_to_byte(end_char);
    let end_line_byte_start = buffer.line_to_byte(end_line);
    let end_col_byte = end_byte - end_line_byte_start;

    InputEdit {
        start_byte,
        old_end_byte: start_byte,
        new_end_byte,
        start_position: Point {
            row: start_line,
            column: start_col_byte,
        },
        old_end_position: Point {
            row: start_line,
            column: start_col_byte,
        },
        new_end_position: Point {
            row: end_line,
            column: end_col_byte,
        },
    }
}

#[cfg(test)]
fn collect_node_kinds(cursor: &mut tree_sitter_md::MarkdownCursor<'_>) -> Vec<String> {
    let mut kinds = Vec::new();
    loop {
        let node = cursor.node();
        if node.is_named() {
            kinds.push(node.kind().to_string());
        }
        if cursor.goto_first_child() {
            kinds.extend(collect_node_kinds(cursor));
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    kinds
}

/// Build InputEdit for a deletion. Call AFTER the buffer mutation.
/// `deleted_text` is the text that was removed.
pub fn input_edit_for_delete(
    buffer: &Buffer,
    char_pos: usize,
    deleted_text: &str,
) -> InputEdit {
    let start_byte = buffer.char_to_byte(char_pos);
    let old_end_byte = start_byte + deleted_text.len();

    let (start_line, _) = buffer.char_idx_to_line_col(char_pos);
    let start_line_byte_start = buffer.line_to_byte(start_line);
    let start_col_byte = start_byte - start_line_byte_start;

    // Reconstruct old_end_position from the deleted text
    let mut old_end_line = start_line;
    let mut old_end_col_byte = start_col_byte;
    for byte in deleted_text.bytes() {
        if byte == b'\n' {
            old_end_line += 1;
            old_end_col_byte = 0;
        } else {
            old_end_col_byte += 1;
        }
    }

    InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte: start_byte,
        start_position: Point {
            row: start_line,
            column: start_col_byte,
        },
        old_end_position: Point {
            row: old_end_line,
            column: old_end_col_byte,
        },
        new_end_position: Point {
            row: start_line,
            column: start_col_byte,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_produces_tree() {
        let buffer = Buffer::from_str("# Hello\n\nSome **bold** text.\n");
        let mut state = MarkdownState::new();
        state.parse_full(&buffer);
        assert!(state.tree().is_some());
        let tree = state.tree().unwrap();
        let mut cursor = tree.walk();
        let kinds = collect_node_kinds(&mut cursor);
        assert!(kinds.contains(&"atx_heading".to_string()));
        assert!(kinds.contains(&"strong_emphasis".to_string()));
    }

    #[test]
    fn incremental_reparse_after_insert() {
        let mut buffer = Buffer::from_str("# Hello\n");
        let mut state = MarkdownState::new();
        state.parse_full(&buffer);

        // Insert " world" at end of heading (char pos 7)
        buffer.insert_str(0, 7, " world");
        let edit = input_edit_for_insert(&buffer, 7, " world");
        state.apply_edit(edit, &buffer);

        assert!(state.tree().is_some());
        let tree = state.tree().unwrap();
        let mut cursor = tree.walk();
        let kinds = collect_node_kinds(&mut cursor);
        assert!(kinds.contains(&"atx_heading".to_string()));
    }

    #[test]
    fn highlights_cover_visible_range() {
        let buffer = Buffer::from_str("# Heading\n\nSome **bold** text.\n");
        let mut state = MarkdownState::new();
        state.parse_full(&buffer);

        let highlights = state.compute_highlights(&buffer, 0, buffer.len_lines(), 0);
        // The heading marker `# ` should have a non-default style
        let style_at_hash = highlights.style_at(0);
        assert_ne!(style_at_hash, ratatui::style::Style::default());
    }
}
