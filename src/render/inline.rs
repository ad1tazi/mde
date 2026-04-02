use ratatui::style::{Color, Modifier, Style};
use tree_sitter_md::MarkdownCursor;

use crate::editor::buffer::Buffer;
use crate::markdown::highlight;
use crate::markdown::reveal::RevealSet;
use crate::render::plan::RenderSpan;

/// Style for emphasis (italic) text.
const STYLE_ITALIC: Style = Style::new().add_modifier(Modifier::ITALIC);
/// Style for strong emphasis (bold) text.
const STYLE_BOLD: Style = Style::new().add_modifier(Modifier::BOLD);
/// Style for strikethrough text.
const STYLE_STRIKETHROUGH: Style = Style::new().add_modifier(Modifier::CROSSED_OUT);

/// Render an inline node's children into spans.
///
/// The inline node is a container for paragraph text content. Its children
/// are formatting elements (emphasis, strong_emphasis, code_span, etc.)
/// with implicit text gaps between them.
///
/// `cursor` must be positioned at the `inline` node. After this call,
/// the cursor is still at the `inline` node.
pub fn render_inline(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    reveal_set: &RevealSet,
    line_start_byte: usize,
    line_end_byte: usize,
    parent_style: Style,
    spans: &mut Vec<RenderSpan>,
) {
    let inline_node = cursor.node();
    let inline_start = inline_node.start_byte().max(line_start_byte);
    let inline_end = inline_node.end_byte().min(line_end_byte);

    if inline_start >= inline_end {
        return;
    }

    let mut byte_pos = inline_start;

    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let child_start = child.start_byte();
            let child_end = child.end_byte();
            let child_kind = child.kind();

            // Skip children entirely before our line range
            if child_end <= line_start_byte {
                if !cursor.goto_next_sibling() {
                    break;
                }
                continue;
            }
            // Stop if child is entirely after our line range
            if child_start >= line_end_byte {
                break;
            }

            // Emit gap text before this child
            if byte_pos < child_start {
                let gap_end = child_start.min(line_end_byte);
                spans.push(text_span(byte_pos, gap_end, buffer, parent_style));
                byte_pos = gap_end;
            }

            // Render the child based on reveal status
            let child_range = child_start..child_end;
            if reveal_set.is_revealed(&child_range) {
                // Revealed: show raw text with syntax highlighting styles
                render_revealed(cursor, buffer, spans);
            } else {
                // Concealed: hide markers, apply styles
                render_concealed(cursor, child_kind, buffer, reveal_set, parent_style, spans);
            }

            byte_pos = child_end.min(line_end_byte);

            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }

    // Trailing gap after last child
    if byte_pos < inline_end {
        spans.push(text_span(byte_pos, inline_end, buffer, parent_style));
    }
}

/// Render a revealed node: show raw text with syntax highlighting.
fn render_revealed(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    spans: &mut Vec<RenderSpan>,
) {
    let node = cursor.node();
    let start = node.start_byte();
    let end = node.end_byte();

    // Walk children to apply per-child highlighting
    if cursor.goto_first_child() {
        let mut pos = start;
        loop {
            let child = cursor.node();
            let child_start = child.start_byte();
            let child_end = child.end_byte();

            // Gap before child
            if pos < child_start {
                spans.push(text_span(pos, child_start, buffer, Style::default()));
            }

            // Style based on child kind
            let style = highlight::style_for_node(child.kind()).unwrap_or_default();
            spans.push(text_span(child_start, child_end, buffer, style));
            pos = child_end;

            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();

        // Trailing gap
        if pos < end {
            spans.push(text_span(pos, end, buffer, Style::default()));
        }
    } else {
        // Leaf node
        let style = highlight::style_for_node(node.kind()).unwrap_or_default();
        spans.push(text_span(start, end, buffer, style));
    }
}

/// Render a concealed inline element: hide delimiters, apply formatting styles.
fn render_concealed(
    cursor: &mut MarkdownCursor<'_>,
    kind: &str,
    buffer: &Buffer,
    reveal_set: &RevealSet,
    parent_style: Style,
    spans: &mut Vec<RenderSpan>,
) {
    match kind {
        "emphasis" => {
            render_concealed_emphasis(cursor, buffer, reveal_set, parent_style, STYLE_ITALIC, spans);
        }
        "strong_emphasis" => {
            render_concealed_emphasis(cursor, buffer, reveal_set, parent_style, STYLE_BOLD, spans);
        }
        "strikethrough" => {
            render_concealed_emphasis(
                cursor,
                buffer,
                reveal_set,
                parent_style,
                STYLE_STRIKETHROUGH,
                spans,
            );
        }
        "code_span" => {
            render_concealed_code_span(cursor, buffer, spans);
        }
        "inline_link" | "full_reference_link" | "collapsed_reference_link" | "image" => {
            render_concealed_link(cursor, buffer, spans);
        }
        _ => {
            // Unknown inline element: show as-is with parent style
            let node = cursor.node();
            spans.push(text_span(
                node.start_byte(),
                node.end_byte(),
                buffer,
                parent_style,
            ));
        }
    }
}

/// Render concealed emphasis/strong/strikethrough: hide delimiters, style content.
///
/// Handles nested emphasis (e.g., `***bold italic***`).
fn render_concealed_emphasis(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    reveal_set: &RevealSet,
    parent_style: Style,
    own_style: Style,
    spans: &mut Vec<RenderSpan>,
) {
    let node = cursor.node();
    let start = node.start_byte();
    let end = node.end_byte();
    let merged_style = parent_style.patch(own_style);

    if !cursor.goto_first_child() {
        // No children — shouldn't happen, but emit styled raw text
        spans.push(text_span(start, end, buffer, merged_style));
        return;
    }

    let mut byte_pos = start;
    loop {
        let child = cursor.node();
        let child_start = child.start_byte();
        let child_end = child.end_byte();
        let child_kind = child.kind();

        // Gap before child: this is content text
        if byte_pos < child_start {
            spans.push(text_span(byte_pos, child_start, buffer, merged_style));
        }

        match child_kind {
            "emphasis_delimiter" => {
                // Hide the delimiter
                spans.push(hidden_span(child_start, child_end, buffer));
            }
            "emphasis" | "strong_emphasis" | "strikethrough" => {
                // Nested formatting: check reveal and recurse
                let child_range = child_start..child_end;
                if reveal_set.is_revealed(&child_range) {
                    render_revealed(cursor, buffer, spans);
                } else {
                    render_concealed(cursor, child_kind, buffer, reveal_set, merged_style, spans);
                }
            }
            _ => {
                // Other child (e.g., code_span inside bold): render with merged style
                let child_range = child_start..child_end;
                if reveal_set.is_revealed(&child_range) {
                    render_revealed(cursor, buffer, spans);
                } else {
                    render_concealed(cursor, child_kind, buffer, reveal_set, merged_style, spans);
                }
            }
        }

        byte_pos = child_end;
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    // Trailing gap
    if byte_pos < end {
        spans.push(text_span(byte_pos, end, buffer, merged_style));
    }
}

/// Render concealed code span: hide backtick delimiters, show content with background.
fn render_concealed_code_span(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    spans: &mut Vec<RenderSpan>,
) {
    let node = cursor.node();
    let start = node.start_byte();
    let end = node.end_byte();
    let code_style = Style::default().bg(Color::Indexed(236));

    if !cursor.goto_first_child() {
        spans.push(text_span(start, end, buffer, code_style));
        return;
    }

    let mut byte_pos = start;
    loop {
        let child = cursor.node();
        let child_start = child.start_byte();
        let child_end = child.end_byte();

        // Gap = code content
        if byte_pos < child_start {
            spans.push(text_span(byte_pos, child_start, buffer, code_style));
        }

        if child.kind() == "code_span_delimiter" {
            spans.push(hidden_span(child_start, child_end, buffer));
        } else {
            spans.push(text_span(child_start, child_end, buffer, code_style));
        }

        byte_pos = child_end;
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    if byte_pos < end {
        spans.push(text_span(byte_pos, end, buffer, code_style));
    }
}

/// Render concealed link: show link text with style, hide URL parts.
fn render_concealed_link(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    spans: &mut Vec<RenderSpan>,
) {
    let node = cursor.node();
    let start = node.start_byte();
    let end = node.end_byte();
    let link_style = Style::default()
        .fg(Color::Blue)
        .add_modifier(Modifier::UNDERLINED);

    if !cursor.goto_first_child() {
        spans.push(text_span(start, end, buffer, link_style));
        return;
    }

    let mut byte_pos = start;
    loop {
        let child = cursor.node();
        let child_start = child.start_byte();
        let child_end = child.end_byte();

        // Gap before child
        if byte_pos < child_start {
            // Hide gaps (these are typically brackets/parens)
            spans.push(hidden_span(byte_pos, child_start, buffer));
        }

        match child.kind() {
            "link_text" | "image_description" => {
                // Show the visible text with link styling
                spans.push(text_span(child_start, child_end, buffer, link_style));
            }
            "[" | "]" | "(" | ")" | "!" => {
                // Hide bracket/paren delimiters
                spans.push(hidden_span(child_start, child_end, buffer));
            }
            "link_destination" | "link_title" => {
                // Hide URL and title
                spans.push(hidden_span(child_start, child_end, buffer));
            }
            _ => {
                // Other children: hide
                spans.push(hidden_span(child_start, child_end, buffer));
            }
        }

        byte_pos = child_end;
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    if byte_pos < end {
        spans.push(hidden_span(byte_pos, end, buffer));
    }
}

// --- Span construction helpers ---

/// Create a span showing buffer text with a style.
fn text_span(start_byte: usize, end_byte: usize, buffer: &Buffer, style: Style) -> RenderSpan {
    let text = buffer.text_for_byte_range(start_byte, end_byte);
    let char_count = buffer.char_count_for_byte_range(start_byte, end_byte);
    RenderSpan {
        display_text: text,
        style,
        raw_byte_range: start_byte..end_byte,
        raw_char_count: char_count,
        is_decoration: false,
    }
}

/// Create a hidden span (raw text hidden, zero display width).
fn hidden_span(start_byte: usize, end_byte: usize, buffer: &Buffer) -> RenderSpan {
    let char_count = buffer.char_count_for_byte_range(start_byte, end_byte);
    RenderSpan {
        display_text: String::new(),
        style: Style::default(),
        raw_byte_range: start_byte..end_byte,
        raw_char_count: char_count,
        is_decoration: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter_md::MarkdownParser;

    fn parse(text: &str) -> tree_sitter_md::MarkdownTree {
        let mut parser = MarkdownParser::default();
        parser.parse(text.as_bytes(), None).unwrap()
    }

    fn render_test(text: &str, cursor_byte: usize) -> Vec<RenderSpan> {
        let buffer = Buffer::from_str(text);
        let tree = parse(text);
        let reveal_set = RevealSet::from_cursor(&tree, cursor_byte);
        let mut cursor = tree.walk();

        let line_start = 0usize;
        let line_end = text.find('\n').unwrap_or(text.len());
        let line_end_byte = line_end; // ASCII text in tests

        // Navigate to the inline node
        // document → section → paragraph → inline
        let mut spans = Vec::new();
        if descend_to_inline_node(&mut cursor, line_start) {
            render_inline(
                &mut cursor,
                &buffer,
                &reveal_set,
                line_start,
                line_end_byte,
                Style::default(),
                &mut spans,
            );
        } else {
            // No inline found, emit plain text
            spans.push(text_span(line_start, line_end_byte, &buffer, Style::default()));
        }
        spans
    }

    /// Concatenate display text from spans.
    fn display_text(spans: &[RenderSpan]) -> String {
        spans.iter().map(|s| s.display_text.as_str()).collect()
    }

    /// Check total raw char count matches expected.
    fn total_raw_chars(spans: &[RenderSpan]) -> usize {
        spans.iter().map(|s| s.raw_char_count).sum()
    }

    #[test]
    fn plain_text_unchanged() {
        let spans = render_test("Hello world\n", 999); // cursor far away
        assert_eq!(display_text(&spans), "Hello world");
    }

    #[test]
    fn bold_concealed_hides_markers() {
        let spans = render_test("This is **bold** text\n", 0); // cursor on "T"
        assert_eq!(display_text(&spans), "This is bold text");
        assert_eq!(total_raw_chars(&spans), 21);
    }

    #[test]
    fn bold_revealed_shows_markers() {
        let spans = render_test("This is **bold** text\n", 11); // cursor inside "bold"
        let text = display_text(&spans);
        assert!(text.contains("**"));
        assert_eq!(text, "This is **bold** text");
    }

    #[test]
    fn italic_concealed() {
        let spans = render_test("This is *italic* text\n", 0);
        assert_eq!(display_text(&spans), "This is italic text");
        // Check that italic content has ITALIC modifier
        let italic_span = spans.iter().find(|s| s.display_text == "italic").unwrap();
        assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn code_span_concealed() {
        let spans = render_test("Use `code` here\n", 0);
        assert_eq!(display_text(&spans), "Use code here");
        // Check code span has background
        let code_span_s = spans.iter().find(|s| s.display_text == "code").unwrap();
        assert!(code_span_s.style.bg.is_some());
    }

    #[test]
    fn link_concealed_shows_text_only() {
        let spans = render_test("[link text](http://example.com)\n", 999);
        assert_eq!(display_text(&spans), "link text");
    }

    #[test]
    fn mixed_inline_concealed() {
        let spans = render_test("Normal **bold** and *italic* end\n", 999);
        assert_eq!(display_text(&spans), "Normal bold and italic end");
    }

    #[test]
    fn raw_chars_preserved() {
        // Even when markers are hidden, total raw_char_count should match line length
        let text = "This is **bold** text\n";
        let spans = render_test(text, 0);
        assert_eq!(total_raw_chars(&spans), 21); // 21 chars (excluding newline)
    }
}

/// Navigate cursor down from root to an `inline` node containing the given byte offset.
/// Returns true if an inline node was found.
pub fn descend_to_inline_node(cursor: &mut MarkdownCursor<'_>, byte_offset: usize) -> bool {
    // Descend through the tree looking for an inline node
    loop {
        let node = cursor.node();
        let kind = node.kind();

        if kind == "inline" {
            return true;
        }

        // Try to descend to a child that covers this byte offset
        if cursor.goto_first_child_for_byte(byte_offset).is_none() {
            return false;
        }
    }
}
