use ropey::Rope;
use std::path::Path;

use super::undo::Operation;

pub struct Buffer {
    rope: Rope,
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
        }
    }

    pub fn from_str(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
        }
    }

    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self::from_str(&text))
    }

    // --- Queries ---

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn line(&self, line_idx: usize) -> ropey::RopeSlice<'_> {
        self.rope.line(line_idx)
    }

    /// Line length in chars, excluding trailing newline.
    pub fn line_len_chars(&self, line_idx: usize) -> usize {
        let line = self.rope.line(line_idx);
        let len = line.len_chars();
        if len > 0 && line.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    pub fn char_at(&self, line: usize, col: usize) -> Option<char> {
        let line_slice = self.rope.line(line);
        if col < line_slice.len_chars() {
            Some(line_slice.char(col))
        } else {
            None
        }
    }

    // --- Index conversions ---

    pub fn line_col_to_char_idx(&self, line: usize, col: usize) -> usize {
        self.rope.line_to_char(line) + col
    }

    pub fn char_idx_to_line_col(&self, char_idx: usize) -> (usize, usize) {
        let line = self.rope.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        (line, char_idx - line_start)
    }

    // --- Mutations ---

    pub fn insert_char(&mut self, line: usize, col: usize, ch: char) -> Operation {
        let pos = self.line_col_to_char_idx(line, col);
        self.rope.insert_char(pos, ch);
        Operation::Insert {
            pos,
            text: ch.to_string(),
        }
    }

    pub fn insert_str(&mut self, line: usize, col: usize, text: &str) -> Operation {
        let pos = self.line_col_to_char_idx(line, col);
        self.rope.insert(pos, text);
        Operation::Insert {
            pos,
            text: text.to_string(),
        }
    }

    pub fn delete_char_forward(&mut self, line: usize, col: usize) -> Option<Operation> {
        let pos = self.line_col_to_char_idx(line, col);
        if pos >= self.rope.len_chars() {
            return None;
        }
        let ch = self.rope.char(pos);
        self.rope.remove(pos..pos + 1);
        Some(Operation::Delete {
            pos,
            text: ch.to_string(),
        })
    }

    pub fn delete_char_backward(&mut self, line: usize, col: usize) -> Option<Operation> {
        if line == 0 && col == 0 {
            return None;
        }
        let pos = self.line_col_to_char_idx(line, col);
        if pos == 0 {
            return None;
        }
        let ch = self.rope.char(pos - 1);
        self.rope.remove(pos - 1..pos);
        Some(Operation::Delete {
            pos: pos - 1,
            text: ch.to_string(),
        })
    }

    pub fn delete_range(
        &mut self,
        start: (usize, usize),
        end: (usize, usize),
    ) -> Option<Operation> {
        let start_idx = self.line_col_to_char_idx(start.0, start.1);
        let end_idx = self.line_col_to_char_idx(end.0, end.1);
        if start_idx >= end_idx {
            return None;
        }
        let text: String = self.rope.slice(start_idx..end_idx).to_string();
        self.rope.remove(start_idx..end_idx);
        Some(Operation::Delete {
            pos: start_idx,
            text,
        })
    }

    pub fn insert_newline(&mut self, line: usize, col: usize) -> Operation {
        self.insert_char(line, col, '\n')
    }

    // --- Apply/reverse for undo/redo ---

    pub fn apply(&mut self, op: &Operation) {
        match op {
            Operation::Insert { pos, text } => {
                self.rope.insert(*pos, text);
            }
            Operation::Delete { pos, text } => {
                let end = *pos + text.chars().count();
                self.rope.remove(*pos..end);
            }
        }
    }

    pub fn reverse(&mut self, op: &Operation) {
        match op {
            Operation::Insert { pos, text } => {
                let end = *pos + text.chars().count();
                self.rope.remove(*pos..end);
            }
            Operation::Delete { pos, text } => {
                self.rope.insert(*pos, text);
            }
        }
    }

    // --- Byte-level conversions (for tree-sitter interop) ---

    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.rope.char_to_byte(char_idx)
    }

    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        self.rope.byte_to_char(byte_idx)
    }

    pub fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        let char_idx = self.line_col_to_char_idx(line, col);
        self.rope.char_to_byte(char_idx)
    }

    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn line_to_byte(&self, line_idx: usize) -> usize {
        self.rope.line_to_byte(line_idx)
    }

    /// Extract text for a byte range as a String.
    pub fn text_for_byte_range(&self, start_byte: usize, end_byte: usize) -> String {
        let start_char = self.rope.byte_to_char(start_byte);
        let end_char = self.rope.byte_to_char(end_byte);
        self.rope.slice(start_char..end_char).to_string()
    }

    /// Count chars in a byte range.
    pub fn char_count_for_byte_range(&self, start_byte: usize, end_byte: usize) -> usize {
        let start_char = self.rope.byte_to_char(start_byte);
        let end_char = self.rope.byte_to_char(end_byte);
        end_char - start_char
    }

    // --- File I/O ---

    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.rope.to_string())
    }

    pub fn contents(&self) -> String {
        self.rope.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_empty() {
        let buf = Buffer::new();
        assert_eq!(buf.len_chars(), 0);
        assert_eq!(buf.len_lines(), 1);
    }

    #[test]
    fn from_str_single_line() {
        let buf = Buffer::from_str("hello");
        assert_eq!(buf.len_lines(), 1);
        assert_eq!(buf.line_len_chars(0), 5);
        assert_eq!(buf.contents(), "hello");
    }

    #[test]
    fn from_str_multiple_lines() {
        let buf = Buffer::from_str("hello\nworld\n");
        assert_eq!(buf.len_lines(), 3); // ropey counts trailing empty line
        assert_eq!(buf.line_len_chars(0), 5);
        assert_eq!(buf.line_len_chars(1), 5);
    }

    #[test]
    fn insert_char_at_start() {
        let mut buf = Buffer::from_str("hello");
        buf.insert_char(0, 0, 'X');
        assert_eq!(buf.contents(), "Xhello");
    }

    #[test]
    fn insert_char_at_end() {
        let mut buf = Buffer::from_str("hello");
        buf.insert_char(0, 5, '!');
        assert_eq!(buf.contents(), "hello!");
    }

    #[test]
    fn insert_char_in_middle() {
        let mut buf = Buffer::from_str("hllo");
        buf.insert_char(0, 1, 'e');
        assert_eq!(buf.contents(), "hello");
    }

    #[test]
    fn insert_char_unicode_emoji() {
        let mut buf = Buffer::from_str("hi");
        buf.insert_char(0, 2, '😀');
        assert_eq!(buf.contents(), "hi😀");
        assert_eq!(buf.line_len_chars(0), 3);
    }

    #[test]
    fn insert_str_multiline() {
        let mut buf = Buffer::from_str("ab");
        buf.insert_str(0, 1, "XY\nZ");
        assert_eq!(buf.contents(), "aXY\nZb");
        assert_eq!(buf.len_lines(), 2);
    }

    #[test]
    fn delete_char_forward_middle() {
        let mut buf = Buffer::from_str("hello");
        buf.delete_char_forward(0, 2);
        assert_eq!(buf.contents(), "helo");
    }

    #[test]
    fn delete_char_forward_at_end_of_line_joins_lines() {
        let mut buf = Buffer::from_str("hello\nworld");
        buf.delete_char_forward(0, 5); // delete the \n
        assert_eq!(buf.contents(), "helloworld");
        assert_eq!(buf.len_lines(), 1);
    }

    #[test]
    fn delete_char_forward_at_end_of_buffer_is_noop() {
        let mut buf = Buffer::from_str("hi");
        let op = buf.delete_char_forward(0, 2);
        assert!(op.is_none());
        assert_eq!(buf.contents(), "hi");
    }

    #[test]
    fn delete_char_backward_middle() {
        let mut buf = Buffer::from_str("hello");
        buf.delete_char_backward(0, 3);
        assert_eq!(buf.contents(), "helo");
    }

    #[test]
    fn delete_char_backward_at_start_of_line_joins_with_previous() {
        let mut buf = Buffer::from_str("hello\nworld");
        buf.delete_char_backward(1, 0); // backspace at start of "world"
        assert_eq!(buf.contents(), "helloworld");
    }

    #[test]
    fn delete_char_backward_at_start_of_buffer_is_noop() {
        let mut buf = Buffer::from_str("hi");
        let op = buf.delete_char_backward(0, 0);
        assert!(op.is_none());
        assert_eq!(buf.contents(), "hi");
    }

    #[test]
    fn insert_newline_splits_line() {
        let mut buf = Buffer::from_str("helloworld");
        buf.insert_newline(0, 5);
        assert_eq!(buf.contents(), "hello\nworld");
        assert_eq!(buf.len_lines(), 2);
    }

    #[test]
    fn insert_newline_at_start_creates_empty_line_above() {
        let mut buf = Buffer::from_str("hello");
        buf.insert_newline(0, 0);
        assert_eq!(buf.contents(), "\nhello");
        assert_eq!(buf.line_len_chars(0), 0);
    }

    #[test]
    fn insert_newline_at_end_creates_empty_line_below() {
        let mut buf = Buffer::from_str("hello");
        buf.insert_newline(0, 5);
        assert_eq!(buf.contents(), "hello\n");
    }

    #[test]
    fn delete_range_within_single_line() {
        let mut buf = Buffer::from_str("hello world");
        buf.delete_range((0, 2), (0, 8));
        assert_eq!(buf.contents(), "herld");
    }

    #[test]
    fn delete_range_spanning_multiple_lines() {
        let mut buf = Buffer::from_str("hello\nworld\nfoo");
        buf.delete_range((0, 3), (2, 1));
        assert_eq!(buf.contents(), "heloo");
    }

    #[test]
    fn line_len_chars_excludes_newline() {
        let buf = Buffer::from_str("hello\nworld\n");
        assert_eq!(buf.line_len_chars(0), 5);
        assert_eq!(buf.line_len_chars(1), 5);
    }

    #[test]
    fn line_len_chars_last_line_no_newline() {
        let buf = Buffer::from_str("hello\nworld");
        assert_eq!(buf.line_len_chars(1), 5);
    }

    #[test]
    fn line_col_to_char_idx_first_line() {
        let buf = Buffer::from_str("hello\nworld");
        assert_eq!(buf.line_col_to_char_idx(0, 3), 3);
    }

    #[test]
    fn line_col_to_char_idx_later_line() {
        let buf = Buffer::from_str("hello\nworld");
        assert_eq!(buf.line_col_to_char_idx(1, 2), 8); // 6 (hello\n) + 2
    }

    #[test]
    fn char_idx_to_line_col_roundtrip() {
        let buf = Buffer::from_str("hello\nworld\nfoo");
        for line in 0..buf.len_lines() {
            for col in 0..buf.line_len_chars(line) {
                let idx = buf.line_col_to_char_idx(line, col);
                let (l, c) = buf.char_idx_to_line_col(idx);
                assert_eq!((l, c), (line, col));
            }
        }
    }

    #[test]
    fn char_to_byte_ascii() {
        let buf = Buffer::from_str("hello");
        assert_eq!(buf.char_to_byte(0), 0);
        assert_eq!(buf.char_to_byte(3), 3);
        assert_eq!(buf.char_to_byte(5), 5);
    }

    #[test]
    fn char_to_byte_multibyte() {
        let buf = Buffer::from_str("héllo"); // é is 2 bytes in UTF-8
        assert_eq!(buf.char_to_byte(0), 0); // 'h'
        assert_eq!(buf.char_to_byte(1), 1); // 'é' starts at byte 1
        assert_eq!(buf.char_to_byte(2), 3); // 'l' starts at byte 3
    }

    #[test]
    fn line_col_to_byte_multiline() {
        let buf = Buffer::from_str("hello\nworld");
        assert_eq!(buf.line_col_to_byte(0, 0), 0);
        assert_eq!(buf.line_col_to_byte(0, 5), 5);
        assert_eq!(buf.line_col_to_byte(1, 0), 6); // after \n
        assert_eq!(buf.line_col_to_byte(1, 3), 9);
    }

    #[test]
    fn byte_char_roundtrip_multibyte() {
        let buf = Buffer::from_str("a😀b");
        // 😀 is 4 bytes in UTF-8
        assert_eq!(buf.char_to_byte(0), 0); // 'a'
        assert_eq!(buf.char_to_byte(1), 1); // '😀'
        assert_eq!(buf.char_to_byte(2), 5); // 'b'
        assert_eq!(buf.byte_to_char(0), 0);
        assert_eq!(buf.byte_to_char(1), 1);
        assert_eq!(buf.byte_to_char(5), 2);
    }

    #[test]
    fn len_bytes_multibyte() {
        let buf = Buffer::from_str("héllo");
        assert_eq!(buf.len_bytes(), 6); // h(1) + é(2) + l(1) + l(1) + o(1)
    }

    #[test]
    fn apply_and_reverse_insert() {
        let mut buf = Buffer::from_str("hello");
        let op = Operation::Insert {
            pos: 5,
            text: " world".to_string(),
        };
        buf.apply(&op);
        assert_eq!(buf.contents(), "hello world");
        buf.reverse(&op);
        assert_eq!(buf.contents(), "hello");
    }

    #[test]
    fn apply_and_reverse_delete() {
        let mut buf = Buffer::from_str("hello world");
        let op = Operation::Delete {
            pos: 5,
            text: " world".to_string(),
        };
        buf.apply(&op);
        assert_eq!(buf.contents(), "hello");
        buf.reverse(&op);
        assert_eq!(buf.contents(), "hello world");
    }
}
