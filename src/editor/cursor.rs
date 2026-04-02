use unicode_width::UnicodeWidthChar;

use super::buffer::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    pub desired_col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Whitespace,
    Word,
    Punctuation,
}

fn classify(ch: char) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Whitespace
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

impl Cursor {
    pub fn new() -> Self {
        Self {
            line: 0,
            col: 0,
            desired_col: 0,
        }
    }

    pub fn move_left(&mut self, buf: &Buffer) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.col = buf.line_len_chars(self.line);
        }
        self.reset_desired_col();
    }

    pub fn move_right(&mut self, buf: &Buffer) {
        let line_len = buf.line_len_chars(self.line);
        if self.col < line_len {
            self.col += 1;
        } else if self.line + 1 < buf.len_lines() {
            self.line += 1;
            self.col = 0;
        }
        self.reset_desired_col();
    }

    pub fn move_up(&mut self, buf: &Buffer) {
        if self.line > 0 {
            self.line -= 1;
            let line_len = buf.line_len_chars(self.line);
            self.col = self.desired_col.min(line_len);
        }
    }

    pub fn move_down(&mut self, buf: &Buffer) {
        if self.line + 1 < buf.len_lines() {
            self.line += 1;
            let line_len = buf.line_len_chars(self.line);
            self.col = self.desired_col.min(line_len);
        }
    }

    pub fn move_home(&mut self) {
        self.col = 0;
        self.reset_desired_col();
    }

    pub fn move_end(&mut self, buf: &Buffer) {
        self.col = buf.line_len_chars(self.line);
        self.reset_desired_col();
    }

    pub fn move_word_left(&mut self, buf: &Buffer) {
        // At start of line, wrap to previous line end
        if self.col == 0 {
            if self.line > 0 {
                self.line -= 1;
                self.col = buf.line_len_chars(self.line);
            }
            self.reset_desired_col();
            return;
        }

        let line = buf.line(self.line);
        let mut col = self.col;

        // Skip whitespace backwards
        while col > 0 {
            let ch = line.char(col - 1);
            if !ch.is_whitespace() {
                break;
            }
            col -= 1;
        }

        if col == 0 {
            self.col = 0;
            self.reset_desired_col();
            return;
        }

        // Determine class of char we're on
        let class = classify(line.char(col - 1));

        // Skip chars of same class
        while col > 0 {
            let ch = line.char(col - 1);
            if classify(ch) != class {
                break;
            }
            col -= 1;
        }

        self.col = col;
        self.reset_desired_col();
    }

    pub fn move_word_right(&mut self, buf: &Buffer) {
        let line_len = buf.line_len_chars(self.line);

        // At end of line, wrap to next line start
        if self.col >= line_len {
            if self.line + 1 < buf.len_lines() {
                self.line += 1;
                self.col = 0;
            }
            self.reset_desired_col();
            return;
        }

        let line = buf.line(self.line);
        let mut col = self.col;

        // Determine class of char we're on
        let class = classify(line.char(col));

        // Skip chars of same class
        while col < line_len {
            let ch = line.char(col);
            if classify(ch) != class {
                break;
            }
            col += 1;
        }

        // Skip whitespace
        while col < line_len {
            let ch = line.char(col);
            if !ch.is_whitespace() {
                break;
            }
            col += 1;
        }

        self.col = col;
        self.reset_desired_col();
    }

    pub fn clamp(&mut self, buf: &Buffer) {
        if buf.len_chars() == 0 {
            self.line = 0;
            self.col = 0;
            self.desired_col = 0;
            return;
        }
        let max_line = buf.len_lines().saturating_sub(1);
        self.line = self.line.min(max_line);
        let line_len = buf.line_len_chars(self.line);
        self.col = self.col.min(line_len);
    }

    pub fn display_col(&self, buf: &Buffer) -> usize {
        if self.line >= buf.len_lines() {
            return 0;
        }
        let line = buf.line(self.line);
        let mut display = 0;
        for (i, ch) in line.chars().enumerate() {
            if i >= self.col {
                break;
            }
            if ch == '\t' {
                let tab_stop = 4 - (display % 4);
                display += tab_stop;
            } else {
                display += UnicodeWidthChar::width(ch).unwrap_or(0);
            }
        }
        display
    }

    pub fn reset_desired_col(&mut self) {
        self.desired_col = self.col;
    }

    pub fn position(&self) -> (usize, usize) {
        (self.line, self.col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Buffer {
        Buffer::from_str(s)
    }

    #[test]
    fn move_right_within_line() {
        let b = buf("hello");
        let mut c = Cursor::new();
        c.move_right(&b);
        assert_eq!(c.position(), (0, 1));
    }

    #[test]
    fn move_right_at_end_of_line_wraps_to_next() {
        let b = buf("hi\nworld");
        let mut c = Cursor::new();
        c.col = 2;
        c.move_right(&b);
        assert_eq!(c.position(), (1, 0));
    }

    #[test]
    fn move_right_at_end_of_buffer_stays() {
        let b = buf("hi");
        let mut c = Cursor::new();
        c.col = 2;
        c.move_right(&b);
        assert_eq!(c.position(), (0, 2));
    }

    #[test]
    fn move_left_within_line() {
        let b = buf("hello");
        let mut c = Cursor::new();
        c.col = 3;
        c.move_left(&b);
        assert_eq!(c.position(), (0, 2));
    }

    #[test]
    fn move_left_at_start_of_line_wraps_to_previous() {
        let b = buf("hi\nworld");
        let mut c = Cursor::new();
        c.line = 1;
        c.col = 0;
        c.move_left(&b);
        assert_eq!(c.position(), (0, 2));
    }

    #[test]
    fn move_left_at_start_of_buffer_stays() {
        let b = buf("hi");
        let mut c = Cursor::new();
        c.move_left(&b);
        assert_eq!(c.position(), (0, 0));
    }

    #[test]
    fn move_down_same_col() {
        let b = buf("hello\nworld");
        let mut c = Cursor::new();
        c.col = 3;
        c.desired_col = 3;
        c.move_down(&b);
        assert_eq!(c.position(), (1, 3));
    }

    #[test]
    fn move_down_clamps_to_shorter_line() {
        let b = buf("hello\nhi");
        let mut c = Cursor::new();
        c.col = 4;
        c.desired_col = 4;
        c.move_down(&b);
        assert_eq!(c.position(), (1, 2)); // "hi" is len 2
    }

    #[test]
    fn move_down_restores_desired_col_on_longer_line() {
        let b = buf("hello\nhi\nworld!");
        let mut c = Cursor::new();
        c.col = 4;
        c.desired_col = 4;
        c.move_down(&b); // to "hi", col clamped to 2
        assert_eq!(c.position(), (1, 2));
        c.move_down(&b); // to "world!", desired_col restores to 4
        assert_eq!(c.position(), (2, 4));
    }

    #[test]
    fn move_up_at_first_line_stays() {
        let b = buf("hello");
        let mut c = Cursor::new();
        c.col = 3;
        c.move_up(&b);
        assert_eq!(c.position(), (0, 3));
    }

    #[test]
    fn move_home_goes_to_col_zero() {
        let b = buf("hello");
        let mut c = Cursor::new();
        c.col = 3;
        c.move_home();
        assert_eq!(c.col, 0);
        let _ = b;
    }

    #[test]
    fn move_end_goes_to_line_length() {
        let b = buf("hello\nworld");
        let mut c = Cursor::new();
        c.move_end(&b);
        assert_eq!(c.col, 5);
    }

    #[test]
    fn move_word_right_skips_word_then_whitespace() {
        let b = buf("hello world");
        let mut c = Cursor::new();
        c.move_word_right(&b);
        assert_eq!(c.col, 6); // after "hello " -> at 'w'
    }

    #[test]
    fn move_word_right_at_end_of_line_wraps() {
        let b = buf("hi\nworld");
        let mut c = Cursor::new();
        c.col = 2;
        c.move_word_right(&b);
        assert_eq!(c.position(), (1, 0));
    }

    #[test]
    fn move_word_left_skips_whitespace_then_word() {
        let b = buf("hello world");
        let mut c = Cursor::new();
        c.col = 8;
        c.desired_col = 8;
        c.move_word_left(&b);
        assert_eq!(c.col, 6); // start of "world"
    }

    #[test]
    fn display_col_ascii() {
        let b = buf("hello");
        let mut c = Cursor::new();
        c.col = 3;
        assert_eq!(c.display_col(&b), 3);
    }

    #[test]
    fn display_col_with_cjk_characters() {
        // CJK chars are 2 display columns wide
        let b = buf("你好world");
        let mut c = Cursor::new();
        c.col = 2; // after the two CJK chars
        assert_eq!(c.display_col(&b), 4); // 2 + 2
        c.col = 3; // after "你好w"
        assert_eq!(c.display_col(&b), 5); // 2 + 2 + 1
    }

    #[test]
    fn display_col_with_tab() {
        let b = buf("\thello");
        let mut c = Cursor::new();
        c.col = 1; // after the tab
        assert_eq!(c.display_col(&b), 4); // tab at col 0 -> next tab stop is 4
    }

    #[test]
    fn clamp_to_buffer_bounds() {
        let b = buf("hi\nworld");
        let mut c = Cursor::new();
        c.line = 5;
        c.col = 100;
        c.clamp(&b);
        assert_eq!(c.line, 1);
        assert_eq!(c.col, 5);
    }
}
