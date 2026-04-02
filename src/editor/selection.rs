use super::buffer::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

impl Selection {
    pub fn new(anchor: (usize, usize), head: (usize, usize)) -> Self {
        Self { anchor, head }
    }

    /// Returns (start, end) in document order.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor.0 < self.head.0
            || (self.anchor.0 == self.head.0 && self.anchor.1 <= self.head.1)
        {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn selected_text(&self, buf: &Buffer) -> String {
        if self.is_empty() {
            return String::new();
        }
        let (start, end) = self.ordered();
        let start_idx = buf.line_col_to_char_idx(start.0, start.1);
        let end_idx = buf.line_col_to_char_idx(end.0, end.1);
        buf.line(0); // ensure valid
        let rope_slice = &buf.contents()[..]; // less efficient but correct
        // Use char indices to extract
        rope_slice
            .chars()
            .skip(start_idx)
            .take(end_idx - start_idx)
            .collect()
    }

    /// Returns true if (line, col) is within the selection range.
    pub fn contains(&self, line: usize, col: usize, buf: &Buffer) -> bool {
        let (start, end) = self.ordered();
        let pos_idx = buf.line_col_to_char_idx(line, col);
        let start_idx = buf.line_col_to_char_idx(start.0, start.1);
        let end_idx = buf.line_col_to_char_idx(end.0, end.1);
        pos_idx >= start_idx && pos_idx < end_idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Buffer {
        Buffer::from_str(s)
    }

    #[test]
    fn ordered_forward_selection() {
        let sel = Selection::new((0, 2), (0, 5));
        assert_eq!(sel.ordered(), ((0, 2), (0, 5)));
    }

    #[test]
    fn ordered_backward_selection() {
        let sel = Selection::new((1, 3), (0, 2));
        assert_eq!(sel.ordered(), ((0, 2), (1, 3)));
    }

    #[test]
    fn selected_text_single_line() {
        let b = buf("hello world");
        let sel = Selection::new((0, 6), (0, 11));
        assert_eq!(sel.selected_text(&b), "world");
    }

    #[test]
    fn selected_text_multiline() {
        let b = buf("hello\nworld\nfoo");
        let sel = Selection::new((0, 3), (2, 2));
        assert_eq!(sel.selected_text(&b), "lo\nworld\nfo");
    }

    #[test]
    fn contains_point_inside() {
        let b = buf("hello world");
        let sel = Selection::new((0, 2), (0, 7));
        assert!(sel.contains(0, 3, &b));
        assert!(sel.contains(0, 2, &b));
    }

    #[test]
    fn contains_point_outside() {
        let b = buf("hello world");
        let sel = Selection::new((0, 2), (0, 7));
        assert!(!sel.contains(0, 7, &b)); // end is exclusive
        assert!(!sel.contains(0, 0, &b));
    }

    #[test]
    fn is_empty_when_anchor_equals_head() {
        let sel = Selection::new((1, 5), (1, 5));
        assert!(sel.is_empty());
    }

    #[test]
    fn is_not_empty() {
        let sel = Selection::new((0, 0), (0, 1));
        assert!(!sel.is_empty());
    }
}
