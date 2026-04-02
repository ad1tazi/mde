use std::ops::Range;

use tree_sitter_md::MarkdownTree;

/// Tracks which AST node byte ranges should be revealed (shown as raw syntax).
///
/// When the cursor is inside a markdown element, that element's raw syntax
/// (delimiters, markers) should be visible. The RevealSet determines which
/// byte ranges should show raw text vs rendered output.
#[derive(Debug, Default)]
pub struct RevealSet {
    /// Byte ranges that should show raw text.
    ranges: Vec<Range<usize>>,
}

impl RevealSet {
    /// Build from cursor byte position by walking the AST.
    ///
    /// Descends from root to the deepest node containing cursor_byte,
    /// collecting reveal ranges based on node type:
    /// - Span-level (emphasis, code_span, links): reveal just that span
    /// - Block-level (fenced_code_block, pipe_table): reveal entire block
    /// - Line-level (atx_heading, thematic_break): reveal that line/block
    pub fn from_cursor(tree: &MarkdownTree, cursor_byte: usize) -> Self {
        let mut reveal = RevealSet::default();
        let mut cursor = tree.walk();

        // Descend to the deepest node containing cursor_byte
        loop {
            let node = cursor.node();
            let node_start = node.start_byte();
            let node_end = node.end_byte();

            // Check if cursor is within this node
            if cursor_byte < node_start || cursor_byte >= node_end {
                break;
            }

            // Check if this node type is a reveal boundary
            match node.kind() {
                // Span-level: reveal just this span
                "emphasis" | "strong_emphasis" | "strikethrough" | "code_span" => {
                    reveal.ranges.push(node_start..node_end);
                }

                // Links: reveal the entire link construct
                "inline_link" | "full_reference_link" | "collapsed_reference_link"
                | "shortcut_link" | "image" => {
                    reveal.ranges.push(node_start..node_end);
                }

                // Block-level: reveal entire block
                "fenced_code_block" | "pipe_table" => {
                    reveal.ranges.push(node_start..node_end);
                }

                // Line-level: reveal the heading/thematic break
                "atx_heading" | "setext_heading" | "thematic_break" => {
                    reveal.ranges.push(node_start..node_end);
                }

                // List markers: reveal just the marker
                "list_marker_minus" | "list_marker_plus" | "list_marker_star"
                | "list_marker_dot" | "list_marker_parenthesis" => {
                    reveal.ranges.push(node_start..node_end);
                }

                // Task list markers: reveal just the checkbox
                "task_list_marker_checked" | "task_list_marker_unchecked" => {
                    reveal.ranges.push(node_start..node_end);
                }

                // Block quote markers: reveal on cursor line
                "block_quote_marker" => {
                    reveal.ranges.push(node_start..node_end);
                }

                _ => {}
            }

            // Try to descend to a child containing cursor_byte
            if cursor.goto_first_child_for_byte(cursor_byte).is_none() {
                break;
            }
        }

        reveal
    }

    /// Returns true if any part of the given byte range overlaps a revealed range.
    pub fn is_revealed(&self, byte_range: &Range<usize>) -> bool {
        self.ranges
            .iter()
            .any(|r| r.start < byte_range.end && byte_range.start < r.end)
    }

    /// Returns true if the given byte offset falls within any revealed range.
    pub fn is_byte_revealed(&self, byte: usize) -> bool {
        self.ranges.iter().any(|r| r.start <= byte && byte < r.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter_md::MarkdownParser;

    fn parse(text: &str) -> MarkdownTree {
        let mut parser = MarkdownParser::default();
        parser.parse(text.as_bytes(), None).unwrap()
    }

    #[test]
    fn cursor_in_plain_text_reveals_nothing() {
        let tree = parse("Hello world\n");
        let reveal = RevealSet::from_cursor(&tree, 3); // inside "Hello"
        assert!(reveal.ranges.is_empty());
    }

    #[test]
    fn cursor_in_bold_reveals_strong_emphasis() {
        let text = "This is **bold** text\n";
        let tree = parse(text);
        // Cursor at byte 11 = inside "bold"
        let reveal = RevealSet::from_cursor(&tree, 11);
        assert!(!reveal.ranges.is_empty());

        // The strong_emphasis node should cover "**bold**" (bytes 8..16)
        assert!(reveal.is_revealed(&(8..16)));
        // But not text outside
        assert!(!reveal.is_revealed(&(0..8)));
    }

    #[test]
    fn cursor_in_italic_reveals_emphasis() {
        let text = "This is *italic* text\n";
        let tree = parse(text);
        // Cursor at byte 10 = inside "italic"
        let reveal = RevealSet::from_cursor(&tree, 10);
        assert!(reveal.is_revealed(&(8..16)));
    }

    #[test]
    fn cursor_outside_bold_reveals_nothing_for_bold() {
        let text = "This is **bold** text\n";
        let tree = parse(text);
        // Cursor at byte 3 = inside "This"
        let reveal = RevealSet::from_cursor(&tree, 3);
        assert!(!reveal.is_revealed(&(8..16)));
    }

    #[test]
    fn cursor_in_code_span_reveals_it() {
        let text = "Use `code` here\n";
        let tree = parse(text);
        // Cursor at byte 5 = inside "code"
        let reveal = RevealSet::from_cursor(&tree, 5);
        assert!(reveal.is_revealed(&(4..10))); // `code`
    }

    #[test]
    fn cursor_in_heading_reveals_it() {
        let text = "# Hello World\n";
        let tree = parse(text);
        // Cursor at byte 5 = inside "Hello"
        let reveal = RevealSet::from_cursor(&tree, 5);
        // The atx_heading should be revealed
        assert!(reveal.is_revealed(&(0..14)));
    }

    #[test]
    fn cursor_in_fenced_code_block_reveals_entire_block() {
        let text = "```rust\nfn main() {}\n```\n";
        let tree = parse(text);
        // Cursor at byte 10 = inside "fn main"
        let reveal = RevealSet::from_cursor(&tree, 10);
        // Should reveal the entire fenced_code_block
        assert!(reveal.is_revealed(&(0..23)));
    }

    #[test]
    fn cursor_outside_formatting_reveals_nothing() {
        let text = "Plain text **bold** more text\n";
        let tree = parse(text);
        // Cursor at byte 25 = inside "more text"
        let reveal = RevealSet::from_cursor(&tree, 25);
        // Bold should NOT be revealed
        assert!(!reveal.is_revealed(&(11..19)));
    }
}
