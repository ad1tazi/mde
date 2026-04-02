use std::time::Instant;

#[derive(Debug, Clone)]
pub enum Operation {
    Insert { pos: usize, text: String },
    Delete { pos: usize, text: String },
}

#[derive(Debug, Clone)]
pub struct ActionGroup {
    pub ops: Vec<Operation>,
    pub cursor_before: (usize, usize),
    pub cursor_after: (usize, usize),
}

pub struct UndoStack {
    undo_stack: Vec<ActionGroup>,
    redo_stack: Vec<ActionGroup>,
    current_group: Option<ActionGroup>,
    last_edit_instant: Option<Instant>,
}

const COALESCE_TIMEOUT_MS: u128 = 500;

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_group: None,
            last_edit_instant: None,
        }
    }

    pub fn record(
        &mut self,
        op: Operation,
        cursor_before: (usize, usize),
        cursor_after: (usize, usize),
    ) {
        self.clear_redo();
        let now = Instant::now();

        let should_coalesce = self.should_coalesce(&op, &now);

        if should_coalesce {
            if let Some(ref mut group) = self.current_group {
                group.ops.push(op);
                group.cursor_after = cursor_after;
            }
        } else {
            self.seal();
            self.current_group = Some(ActionGroup {
                ops: vec![op],
                cursor_before,
                cursor_after,
            });
        }

        self.last_edit_instant = Some(now);
    }

    fn should_coalesce(&self, op: &Operation, now: &Instant) -> bool {
        let group = match &self.current_group {
            Some(g) => g,
            None => return false,
        };

        // Check timeout
        if let Some(last) = self.last_edit_instant {
            if now.duration_since(last).as_millis() > COALESCE_TIMEOUT_MS {
                return false;
            }
        }

        // Only coalesce single-char inserts
        let (new_pos, new_text) = match op {
            Operation::Insert { pos, text } if text.len() == 1 => (*pos, text.as_str()),
            _ => return false,
        };

        // Don't coalesce whitespace
        if new_text.chars().next().map_or(false, |c| c.is_whitespace()) {
            return false;
        }

        // Check that previous op was also a single-char insert at adjacent position
        if let Some(last_op) = group.ops.last() {
            match last_op {
                Operation::Insert { pos, text } if text.len() == 1 => {
                    // Adjacent: new insert is right after the last one
                    *pos + 1 == new_pos
                }
                _ => false,
            }
        } else {
            false
        }
    }

    pub fn seal(&mut self) {
        if let Some(group) = self.current_group.take() {
            self.undo_stack.push(group);
        }
        self.last_edit_instant = None;
    }

    pub fn undo(&mut self) -> Option<ActionGroup> {
        self.seal();
        let group = self.undo_stack.pop()?;
        self.redo_stack.push(group.clone());
        Some(group)
    }

    pub fn redo(&mut self) -> Option<ActionGroup> {
        let group = self.redo_stack.pop()?;
        self.undo_stack.push(group.clone());
        Some(group)
    }

    fn clear_redo(&mut self) {
        self.redo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_op(pos: usize, text: &str) -> Operation {
        Operation::Insert {
            pos,
            text: text.to_string(),
        }
    }

    fn delete_op(pos: usize, text: &str) -> Operation {
        Operation::Delete {
            pos,
            text: text.to_string(),
        }
    }

    #[test]
    fn undo_single_insert() {
        let mut stack = UndoStack::new();
        stack.record(insert_op(0, "hello"), (0, 0), (0, 5));
        stack.seal();
        let group = stack.undo().unwrap();
        assert_eq!(group.ops.len(), 1);
        assert_eq!(group.cursor_before, (0, 0));
    }

    #[test]
    fn undo_restores_cursor_position() {
        let mut stack = UndoStack::new();
        stack.record(insert_op(0, "a"), (0, 0), (0, 1));
        stack.seal();
        let group = stack.undo().unwrap();
        assert_eq!(group.cursor_before, (0, 0));
        assert_eq!(group.cursor_after, (0, 1));
    }

    #[test]
    fn redo_after_undo() {
        let mut stack = UndoStack::new();
        stack.record(insert_op(0, "hello"), (0, 0), (0, 5));
        stack.seal();
        stack.undo();
        let group = stack.redo().unwrap();
        assert_eq!(group.ops.len(), 1);
        assert_eq!(group.cursor_after, (0, 5));
    }

    #[test]
    fn redo_cleared_on_new_edit() {
        let mut stack = UndoStack::new();
        stack.record(insert_op(0, "hello"), (0, 0), (0, 5));
        stack.seal();
        stack.undo();
        // New edit should clear redo
        stack.record(insert_op(0, "x"), (0, 0), (0, 1));
        assert!(stack.redo().is_none());
    }

    #[test]
    fn whitespace_breaks_coalescing() {
        let mut stack = UndoStack::new();
        stack.record(insert_op(0, "a"), (0, 0), (0, 1));
        stack.record(insert_op(1, " "), (0, 1), (0, 2)); // space breaks
        stack.seal();
        // Should have 2 groups: "a" and " "
        let g1 = stack.undo().unwrap();
        assert_eq!(g1.ops.len(), 1); // the space
        let g2 = stack.undo().unwrap();
        assert_eq!(g2.ops.len(), 1); // the "a"
    }

    #[test]
    fn undo_multiline_paste() {
        let mut stack = UndoStack::new();
        stack.record(insert_op(0, "line1\nline2\nline3"), (0, 0), (2, 5));
        stack.seal();
        let group = stack.undo().unwrap();
        assert_eq!(group.ops.len(), 1);
    }

    #[test]
    fn undo_delete_range() {
        let mut stack = UndoStack::new();
        stack.record(delete_op(5, " world"), (0, 11), (0, 5));
        stack.seal();
        let group = stack.undo().unwrap();
        assert_eq!(group.ops.len(), 1);
        match &group.ops[0] {
            Operation::Delete { pos, text } => {
                assert_eq!(*pos, 5);
                assert_eq!(text, " world");
            }
            _ => panic!("Expected Delete op"),
        }
    }

    #[test]
    fn multiple_undo_redo_cycles() {
        let mut stack = UndoStack::new();
        stack.record(insert_op(0, "a"), (0, 0), (0, 1));
        stack.seal();
        stack.record(insert_op(1, "b"), (0, 1), (0, 2));
        stack.seal();

        // Undo both
        stack.undo().unwrap();
        stack.undo().unwrap();

        // Redo both
        let g1 = stack.redo().unwrap();
        assert_eq!(g1.cursor_after, (0, 1));
        let g2 = stack.redo().unwrap();
        assert_eq!(g2.cursor_after, (0, 2));

        // No more redo
        assert!(stack.redo().is_none());
    }

    #[test]
    fn undo_returns_none_when_empty() {
        let mut stack = UndoStack::new();
        assert!(stack.undo().is_none());
    }
}
