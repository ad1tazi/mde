use ratatui::style::Style;
use tree_sitter_md::MarkdownTree;

use crate::editor::buffer::Buffer;
use crate::markdown::reveal::RevealSet;
use crate::markdown::MarkdownState;
use crate::render::block;
use crate::render::code_block;
use crate::render::inline;
use crate::render::list;
use crate::render::table;
use crate::render::header_image::HeaderImageSupport;
use crate::render::plan::{ImageHeader, PositionMap, RenderLine, RenderSpan};

/// Compute render lines for the visible range.
///
/// This is the main entry point for the rendering engine. It walks the AST
/// for each visible line and produces `RenderLine`s with position maps.
pub fn compute_render_lines(
    markdown: &MarkdownState,
    buffer: &Buffer,
    cursor_byte: usize,
    visible_start: usize,
    visible_end: usize,
) -> Vec<RenderLine> {
    compute_render_lines_with_width(markdown, buffer, cursor_byte, visible_start, visible_end, 80, None)
}

/// Compute render lines with a specific viewport width.
pub fn compute_render_lines_with_width(
    markdown: &MarkdownState,
    buffer: &Buffer,
    cursor_byte: usize,
    visible_start: usize,
    visible_end: usize,
    viewport_width: usize,
    image_support: Option<&mut HeaderImageSupport>,
) -> Vec<RenderLine> {
    let tree = match markdown.tree() {
        Some(t) => t,
        None => {
            // No parse tree — render all lines as plain text
            return (visible_start..visible_end)
                .map(|line_idx| plain_render_line(line_idx, buffer))
                .collect();
        }
    };

    let reveal_set = RevealSet::from_cursor(tree, cursor_byte);
    let mut render_lines = Vec::with_capacity(visible_end - visible_start);

    for line_idx in visible_start..visible_end {
        let render_line = render_buffer_line(
            line_idx,
            buffer,
            tree,
            &reveal_set,
            viewport_width,
            &image_support,
        );
        render_lines.push(render_line);
    }

    render_lines
}

/// Render a single buffer line into a RenderLine.
fn render_buffer_line(
    line_idx: usize,
    buffer: &Buffer,
    tree: &MarkdownTree,
    reveal_set: &RevealSet,
    viewport_width: usize,
    image_support: &Option<&mut HeaderImageSupport>,
) -> RenderLine {
    let line_start_byte = buffer.line_to_byte(line_idx);
    let line_char_count = buffer.line_len_chars(line_idx);

    // Compute the end byte (excluding trailing newline)
    let line_end_byte = if line_char_count == 0 {
        line_start_byte
    } else {
        let char_idx = buffer.line_col_to_char_idx(line_idx, line_char_count);
        buffer.char_to_byte(char_idx)
    };

    if line_start_byte == line_end_byte {
        // Empty line
        return RenderLine {
            line_idx,
            spans: vec![],
            position_map: PositionMap::build(&[], 0),
            image_header: None,
        };
    }

    // Try to find what kind of block this line is in
    let mut cursor = tree.walk();
    let (spans, heading_meta) = render_line_with_context(
        &mut cursor,
        tree,
        line_idx,
        line_start_byte,
        line_end_byte,
        buffer,
        reveal_set,
        viewport_width,
    );

    // Populate image_header for concealed headings when image support is available
    let image_header = if let Some(meta) = heading_meta {
        if meta.is_concealed && meta.tier > 0 && image_support.is_some() {
            let display_rows = image_support
                .as_ref()
                .unwrap()
                .display_rows(meta.tier);
            Some(ImageHeader {
                tier: meta.tier,
                text: meta.plain_text,
                display_rows,
            })
        } else {
            None
        }
    } else {
        None
    };

    let position_map = PositionMap::build(&spans, line_char_count);
    RenderLine {
        line_idx,
        spans,
        position_map,
        image_header,
    }
}

/// Render a line by descending the AST to find its context.
/// Returns (spans, optional heading metadata).
fn render_line_with_context(
    cursor: &mut tree_sitter_md::MarkdownCursor<'_>,
    tree: &MarkdownTree,
    _line_idx: usize,
    line_start_byte: usize,
    line_end_byte: usize,
    buffer: &Buffer,
    reveal_set: &RevealSet,
    viewport_width: usize,
) -> (Vec<RenderSpan>, Option<block::HeadingMeta>) {
    // Navigate down to find the block context
    loop {
        let node = cursor.node();
        let kind = node.kind();

        match kind {
            "inline" => {
                // We've reached an inline node — render inline content
                let mut spans = Vec::new();
                inline::render_inline(
                    cursor,
                    buffer,
                    reveal_set,
                    line_start_byte,
                    line_end_byte,
                    Style::default(),
                    &mut spans,
                );
                return (spans, None);
            }
            "atx_heading" | "setext_heading" => {
                let mut spans = Vec::new();
                let meta = block::render_atx_heading(
                    cursor,
                    buffer,
                    reveal_set,
                    line_start_byte,
                    line_end_byte,
                    &mut spans,
                );
                return (spans, Some(meta));
            }
            "thematic_break" => {
                let mut spans = Vec::new();
                block::render_thematic_break(
                    buffer,
                    reveal_set,
                    line_start_byte,
                    line_end_byte,
                    viewport_width,
                    &mut spans,
                );
                return (spans, None);
            }
            "block_quote" => {
                let mut spans = Vec::new();
                block::render_block_quote(
                    cursor,
                    buffer,
                    reveal_set,
                    line_start_byte,
                    line_end_byte,
                    viewport_width,
                    &mut spans,
                );
                return (spans, None);
            }
            "list_item" => {
                let mut spans = Vec::new();
                list::render_list_item(
                    cursor,
                    buffer,
                    reveal_set,
                    line_start_byte,
                    line_end_byte,
                    viewport_width,
                    &mut spans,
                );
                return (spans, None);
            }
            "pipe_table" | "pipe_table_header" | "pipe_table_delimiter_row" | "pipe_table_row" => {
                let mut spans = Vec::new();
                table::render_pipe_table_line(
                    buffer,
                    tree,
                    reveal_set,
                    line_start_byte,
                    line_end_byte,
                    &mut spans,
                );
                return (spans, None);
            }
            "fenced_code_block" => {
                let mut spans = Vec::new();
                code_block::render_fenced_code_block_line(
                    buffer,
                    tree,
                    reveal_set,
                    line_start_byte,
                    line_end_byte,
                    viewport_width,
                    &mut spans,
                );
                return (spans, None);
            }
            _ => {}
        }

        if cursor.goto_first_child_for_byte(line_start_byte).is_none() {
            break;
        }
    }

    // Fallback: render as plain text
    (render_line_with_highlights(line_start_byte, line_end_byte, buffer), None)
}

/// Fallback renderer: show raw text with syntax highlighting.
/// Used for block types not yet handled by the render engine.
fn render_line_with_highlights(
    line_start_byte: usize,
    line_end_byte: usize,
    buffer: &Buffer,
) -> Vec<RenderSpan> {
    let text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
    let char_count = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
    vec![RenderSpan {
        display_text: text,
        style: Style::default(),
        raw_byte_range: line_start_byte..line_end_byte,
        raw_char_count: char_count,
        is_decoration: false,
    }]
}

/// Create a plain text RenderLine for a buffer line (no AST).
fn plain_render_line(line_idx: usize, buffer: &Buffer) -> RenderLine {
    let line_start_byte = buffer.line_to_byte(line_idx);
    let line_char_count = buffer.line_len_chars(line_idx);
    let line_end_byte = if line_char_count == 0 {
        line_start_byte
    } else {
        let char_idx = buffer.line_col_to_char_idx(line_idx, line_char_count);
        buffer.char_to_byte(char_idx)
    };

    let text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
    let spans = if text.is_empty() {
        vec![]
    } else {
        vec![RenderSpan {
            display_text: text,
            style: Style::default(),
            raw_byte_range: line_start_byte..line_end_byte,
            raw_char_count: line_char_count,
            is_decoration: false,
        }]
    };

    let position_map = PositionMap::build(&spans, line_char_count);
    RenderLine {
        line_idx,
        spans,
        position_map,
        image_header: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::MarkdownState;

    fn make_state(text: &str) -> (Buffer, MarkdownState) {
        let buffer = Buffer::from_str(text);
        let mut state = MarkdownState::new();
        state.parse_full(&buffer);
        (buffer, state)
    }

    fn display_text(line: &RenderLine) -> String {
        line.spans
            .iter()
            .map(|s| s.display_text.as_str())
            .collect()
    }

    #[test]
    fn plain_text_line() {
        let (buffer, state) = make_state("Hello world\n");
        let lines = compute_render_lines(&state, &buffer, 0, 0, 1);
        assert_eq!(lines.len(), 1);
        assert_eq!(display_text(&lines[0]), "Hello world");
    }

    #[test]
    fn bold_concealed_in_paragraph() {
        let (buffer, state) = make_state("This is **bold** text\n");
        // Cursor at byte 0 (outside bold)
        let lines = compute_render_lines(&state, &buffer, 0, 0, 1);
        assert_eq!(display_text(&lines[0]), "This is bold text");
    }

    #[test]
    fn bold_revealed_in_paragraph() {
        let (buffer, state) = make_state("This is **bold** text\n");
        // Cursor at byte 11 (inside "bold")
        let lines = compute_render_lines(&state, &buffer, 11, 0, 1);
        assert_eq!(display_text(&lines[0]), "This is **bold** text");
    }

    #[test]
    fn position_map_concealed_bold() {
        let (buffer, state) = make_state("This is **bold** text\n");
        let lines = compute_render_lines(&state, &buffer, 0, 0, 1);
        let map = &lines[0].position_map;

        // "This is " is 8 chars, display cols 0-7
        assert_eq!(map.raw_to_display_col(0), 0); // 'T'
        assert_eq!(map.raw_to_display_col(7), 7); // ' '

        // "**" at raw cols 8-9 are hidden → display col 8
        assert_eq!(map.raw_to_display_col(8), 8);
        assert_eq!(map.raw_to_display_col(9), 8);

        // "bold" at raw cols 10-13 → display cols 8-11
        assert_eq!(map.raw_to_display_col(10), 8);
        assert_eq!(map.raw_to_display_col(13), 11);

        // "**" at raw cols 14-15 are hidden → display col 12
        assert_eq!(map.raw_to_display_col(14), 12);

        // " text" at raw cols 16-20 → display cols 12-16
        assert_eq!(map.raw_to_display_col(16), 12);
        assert_eq!(map.raw_to_display_col(20), 16);

        // End
        assert_eq!(map.raw_to_display_col(21), 17);
    }

    #[test]
    fn position_map_revealed_bold() {
        let (buffer, state) = make_state("This is **bold** text\n");
        // Cursor inside bold
        let lines = compute_render_lines(&state, &buffer, 11, 0, 1);
        let map = &lines[0].position_map;

        // When revealed, it's 1:1 mapping
        assert_eq!(map.raw_to_display_col(0), 0);
        assert_eq!(map.raw_to_display_col(8), 8);   // first *
        assert_eq!(map.raw_to_display_col(10), 10);  // 'b'
        assert_eq!(map.raw_to_display_col(20), 20);  // 't'
        assert_eq!(map.raw_to_display_col(21), 21);  // end
    }

    #[test]
    fn empty_line() {
        let (buffer, state) = make_state("Hello\n\nWorld\n");
        let lines = compute_render_lines(&state, &buffer, 0, 0, 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(display_text(&lines[0]), "Hello");
        assert_eq!(display_text(&lines[1]), "");
        assert_eq!(display_text(&lines[2]), "World");
    }

    #[test]
    fn multiple_inline_elements() {
        let text = "Normal **bold** and *italic* end\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        assert_eq!(display_text(&lines[0]), "Normal bold and italic end");
    }

    #[test]
    fn code_span_concealed() {
        let text = "Use `code` here\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines(&state, &buffer, 0, 0, 1);
        assert_eq!(display_text(&lines[0]), "Use code here");
    }

    #[test]
    fn link_concealed() {
        let text = "Click [here](http://example.com) now\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        assert_eq!(display_text(&lines[0]), "Click here now");
    }
}
