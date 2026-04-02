use ratatui::style::{Color, Modifier, Style};
use tree_sitter_md::MarkdownCursor;

use crate::editor::buffer::Buffer;
use crate::markdown::reveal::RevealSet;
use crate::render::inline;
use crate::render::plan::RenderSpan;

/// Metadata extracted from heading rendering.
pub struct HeadingMeta {
    /// Heading tier (1-6), or 0 if unknown.
    pub tier: u8,
    /// Plain text content (markers stripped), empty if revealed.
    pub plain_text: String,
    /// Whether the heading is concealed (eligible for image rendering).
    pub is_concealed: bool,
}

/// Render an atx_heading node (e.g., `# Heading`).
///
/// When concealed: hides the `# ` marker, applies heading style to content.
/// When revealed: shows raw text with syntax highlighting.
///
/// `cursor` must be positioned at the `atx_heading` node.
/// Returns heading metadata for image rendering.
pub fn render_atx_heading(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    reveal_set: &RevealSet,
    line_start_byte: usize,
    line_end_byte: usize,
    spans: &mut Vec<RenderSpan>,
) -> HeadingMeta {
    let node = cursor.node();
    let node_range = node.start_byte()..node.end_byte();

    if reveal_set.is_revealed(&node_range) {
        // Revealed: show raw with highlighting
        render_heading_revealed(cursor, buffer, line_start_byte, line_end_byte, spans);
        return HeadingMeta {
            tier: 0,
            plain_text: String::new(),
            is_concealed: false,
        };
    }

    // Concealed: determine heading level and style
    if !cursor.goto_first_child() {
        // Shouldn't happen — emit raw
        let text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
        let char_count = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
        spans.push(RenderSpan {
            display_text: text,
            style: Style::default(),
            raw_byte_range: line_start_byte..line_end_byte,
            raw_char_count: char_count,
            is_decoration: false,
        });
        return HeadingMeta {
            tier: 0,
            plain_text: String::new(),
            is_concealed: false,
        };
    }

    let mut heading_style = Style::default().add_modifier(Modifier::BOLD);
    let mut byte_pos = line_start_byte;
    let mut tier: u8 = 0;

    loop {
        let child = cursor.node();
        let child_start = child.start_byte();
        let child_end = child.end_byte();
        let child_kind = child.kind();

        // Skip children outside our line range
        if child_end <= line_start_byte {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }
        if child_start >= line_end_byte {
            break;
        }

        match child_kind {
            "atx_h1_marker" => {
                tier = 1;
                heading_style = Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
                // Hide the marker and the space after it
                let hide_end = child_end.min(line_end_byte);
                spans.push(hidden_span(byte_pos, hide_end, buffer));
                byte_pos = hide_end;
                // Also hide the space between marker and content
                if byte_pos < line_end_byte {
                    let next_byte = buffer.text_for_byte_range(byte_pos, (byte_pos + 1).min(line_end_byte));
                    if next_byte == " " {
                        spans.push(hidden_span(byte_pos, byte_pos + 1, buffer));
                        byte_pos += 1;
                    }
                }
            }
            "atx_h2_marker" => {
                tier = 2;
                heading_style = Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD);
                let hide_end = child_end.min(line_end_byte);
                spans.push(hidden_span(byte_pos, hide_end, buffer));
                byte_pos = hide_end;
                if byte_pos < line_end_byte {
                    let next_byte = buffer.text_for_byte_range(byte_pos, (byte_pos + 1).min(line_end_byte));
                    if next_byte == " " {
                        spans.push(hidden_span(byte_pos, byte_pos + 1, buffer));
                        byte_pos += 1;
                    }
                }
            }
            "atx_h3_marker" => {
                tier = 3;
                heading_style = Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD);
                let hide_end = child_end.min(line_end_byte);
                spans.push(hidden_span(byte_pos, hide_end, buffer));
                byte_pos = hide_end;
                if byte_pos < line_end_byte {
                    let next_byte = buffer.text_for_byte_range(byte_pos, (byte_pos + 1).min(line_end_byte));
                    if next_byte == " " {
                        spans.push(hidden_span(byte_pos, byte_pos + 1, buffer));
                        byte_pos += 1;
                    }
                }
            }
            "atx_h4_marker" | "atx_h5_marker" | "atx_h6_marker" => {
                tier = match child_kind {
                    "atx_h4_marker" => 4,
                    "atx_h5_marker" => 5,
                    _ => 6,
                };
                heading_style = Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD);
                let hide_end = child_end.min(line_end_byte);
                spans.push(hidden_span(byte_pos, hide_end, buffer));
                byte_pos = hide_end;
                if byte_pos < line_end_byte {
                    let next_byte = buffer.text_for_byte_range(byte_pos, (byte_pos + 1).min(line_end_byte));
                    if next_byte == " " {
                        spans.push(hidden_span(byte_pos, byte_pos + 1, buffer));
                        byte_pos += 1;
                    }
                }
            }
            "inline" => {
                // Gap before inline
                if byte_pos < child_start {
                    spans.push(hidden_span(byte_pos, child_start, buffer));
                }
                // Render inline content with heading style
                inline::render_inline(
                    cursor,
                    buffer,
                    reveal_set,
                    line_start_byte,
                    line_end_byte,
                    heading_style,
                    spans,
                );
                byte_pos = child_end.min(line_end_byte);
            }
            _ => {
                // Unknown child — show with heading style
                if byte_pos < child_start {
                    let text = buffer.text_for_byte_range(byte_pos, child_start);
                    let cc = buffer.char_count_for_byte_range(byte_pos, child_start);
                    spans.push(RenderSpan {
                        display_text: text,
                        style: heading_style,
                        raw_byte_range: byte_pos..child_start,
                        raw_char_count: cc,
                        is_decoration: false,
                    });
                }
                let text = buffer.text_for_byte_range(child_start, child_end.min(line_end_byte));
                let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                spans.push(RenderSpan {
                    display_text: text,
                    style: heading_style,
                    raw_byte_range: child_start..child_end.min(line_end_byte),
                    raw_char_count: cc,
                    is_decoration: false,
                });
                byte_pos = child_end.min(line_end_byte);
            }
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    // Trailing content
    if byte_pos < line_end_byte {
        let text = buffer.text_for_byte_range(byte_pos, line_end_byte);
        let cc = buffer.char_count_for_byte_range(byte_pos, line_end_byte);
        spans.push(RenderSpan {
            display_text: text,
            style: heading_style,
            raw_byte_range: byte_pos..line_end_byte,
            raw_char_count: cc,
            is_decoration: false,
        });
    }

    // Extract plain text from display spans (strips markdown syntax)
    let plain_text: String = spans
        .iter()
        .filter(|s| !s.is_decoration && !s.display_text.is_empty())
        .map(|s| s.display_text.as_str())
        .collect();

    HeadingMeta {
        tier,
        plain_text,
        is_concealed: true,
    }
}

/// Render a revealed heading: show all raw text with highlighting.
fn render_heading_revealed(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    line_start_byte: usize,
    line_end_byte: usize,
    spans: &mut Vec<RenderSpan>,
) {
    if !cursor.goto_first_child() {
        let text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
        let cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
        spans.push(RenderSpan {
            display_text: text,
            style: Style::default(),
            raw_byte_range: line_start_byte..line_end_byte,
            raw_char_count: cc,
            is_decoration: false,
        });
        return;
    }

    let mut byte_pos = line_start_byte;
    loop {
        let child = cursor.node();
        let child_start = child.start_byte();
        let child_end = child.end_byte().min(line_end_byte);
        let child_kind = child.kind();

        if child_end <= line_start_byte {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }
        if child_start >= line_end_byte {
            break;
        }

        // Gap before child
        if byte_pos < child_start {
            let text = buffer.text_for_byte_range(byte_pos, child_start);
            let cc = buffer.char_count_for_byte_range(byte_pos, child_start);
            spans.push(RenderSpan {
                display_text: text,
                style: Style::default().add_modifier(Modifier::BOLD),
                raw_byte_range: byte_pos..child_start,
                raw_char_count: cc,
                is_decoration: false,
            });
        }

        let style = crate::markdown::highlight::style_for_node(child_kind)
            .unwrap_or(Style::default().add_modifier(Modifier::BOLD));

        if child_kind == "inline" {
            // Render inline content with reveal (all revealed since heading is revealed)
            let reveal_all = RevealSet::default(); // empty = nothing extra revealed
            inline::render_inline(
                cursor,
                buffer,
                &reveal_all,
                line_start_byte,
                line_end_byte,
                style,
                spans,
            );
        } else {
            let text = buffer.text_for_byte_range(child_start.max(line_start_byte), child_end);
            let cc = buffer.char_count_for_byte_range(child_start.max(line_start_byte), child_end);
            spans.push(RenderSpan {
                display_text: text,
                style,
                raw_byte_range: child_start.max(line_start_byte)..child_end,
                raw_char_count: cc,
                is_decoration: false,
            });
        }

        byte_pos = child_end;
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    if byte_pos < line_end_byte {
        let text = buffer.text_for_byte_range(byte_pos, line_end_byte);
        let cc = buffer.char_count_for_byte_range(byte_pos, line_end_byte);
        spans.push(RenderSpan {
            display_text: text,
            style: Style::default().add_modifier(Modifier::BOLD),
            raw_byte_range: byte_pos..line_end_byte,
            raw_char_count: cc,
            is_decoration: false,
        });
    }
}

/// Render a thematic break (horizontal rule).
///
/// When concealed: replace with full-width line of ─ chars.
/// When revealed: show raw `---`/`***`/`___`.
pub fn render_thematic_break(
    buffer: &Buffer,
    reveal_set: &RevealSet,
    line_start_byte: usize,
    line_end_byte: usize,
    viewport_width: usize,
    spans: &mut Vec<RenderSpan>,
) {
    let node_range = line_start_byte..line_end_byte;

    if reveal_set.is_revealed(&node_range) {
        // Show raw
        let text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
        let cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
        let style = crate::markdown::highlight::style_for_node("thematic_break")
            .unwrap_or_default();
        spans.push(RenderSpan {
            display_text: text,
            style,
            raw_byte_range: line_start_byte..line_end_byte,
            raw_char_count: cc,
            is_decoration: false,
        });
    } else {
        // Replace with horizontal line
        let raw_cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
        let line_text = "─".repeat(viewport_width);
        spans.push(RenderSpan {
            display_text: line_text,
            style: Style::default().fg(Color::DarkGray),
            raw_byte_range: line_start_byte..line_end_byte,
            raw_char_count: raw_cc,
            is_decoration: false,
        });
    }
}

/// Render a block_quote line.
///
/// When concealed: replace `> ` marker with `│ ` decoration, render inner content
/// with dimmed style. When the marker is revealed (cursor on it), show raw `> `.
///
/// `cursor` must be positioned at the `block_quote` node.
pub fn render_block_quote(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    reveal_set: &RevealSet,
    line_start_byte: usize,
    line_end_byte: usize,
    viewport_width: usize,
    spans: &mut Vec<RenderSpan>,
) {
    let quote_style = Style::default().fg(Color::Indexed(245));
    let border_style = Style::default().fg(Color::DarkGray);

    if !cursor.goto_first_child() {
        // No children — emit raw
        let text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
        let cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
        spans.push(RenderSpan {
            display_text: text,
            style: quote_style,
            raw_byte_range: line_start_byte..line_end_byte,
            raw_char_count: cc,
            is_decoration: false,
        });
        return;
    }

    let mut byte_pos = line_start_byte;
    loop {
        let child = cursor.node();
        let child_start = child.start_byte();
        let child_end = child.end_byte();
        let child_kind = child.kind();

        if child_end <= line_start_byte {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }
        if child_start >= line_end_byte {
            break;
        }

        // Gap before child
        if byte_pos < child_start.min(line_end_byte) {
            let gap_end = child_start.min(line_end_byte);
            spans.push(hidden_span(byte_pos, gap_end, buffer));
        }

        match child_kind {
            "block_quote_marker" => {
                let marker_range = child_start..child_end;
                if reveal_set.is_revealed(&marker_range) {
                    // Show raw marker with highlighting
                    let text = buffer.text_for_byte_range(child_start, child_end.min(line_end_byte));
                    let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                    let style = crate::markdown::highlight::style_for_node("block_quote_marker")
                        .unwrap_or(border_style);
                    spans.push(RenderSpan {
                        display_text: text,
                        style,
                        raw_byte_range: child_start..child_end.min(line_end_byte),
                        raw_char_count: cc,
                        is_decoration: false,
                    });
                } else {
                    // Replace "> " with "│ " decoration
                    spans.push(hidden_span(child_start, child_end.min(line_end_byte), buffer));
                    // Add decoration
                    spans.push(RenderSpan {
                        display_text: "│ ".to_string(),
                        style: border_style,
                        raw_byte_range: 0..0,
                        raw_char_count: 0,
                        is_decoration: true,
                    });
                }
                byte_pos = child_end.min(line_end_byte);
            }
            "paragraph" | "list" | "block_quote" => {
                // Recurse into inner content
                // For paragraphs, descend to the inline node
                if child_kind == "block_quote" {
                    // Nested blockquote
                    render_block_quote(
                        cursor,
                        buffer,
                        reveal_set,
                        line_start_byte.max(child_start),
                        line_end_byte.min(child_end),
                        viewport_width,
                        spans,
                    );
                } else {
                    // Descend into paragraph → inline
                    if cursor.goto_first_child() {
                        // Find the inline child
                        loop {
                            let inner = cursor.node();
                            if inner.kind() == "inline" {
                                inline::render_inline(
                                    cursor,
                                    buffer,
                                    reveal_set,
                                    line_start_byte,
                                    line_end_byte,
                                    quote_style,
                                    spans,
                                );
                                break;
                            }
                            if !cursor.goto_next_sibling() {
                                // No inline found, emit raw
                                let s = child_start.max(line_start_byte);
                                let e = child_end.min(line_end_byte);
                                let text = buffer.text_for_byte_range(s, e);
                                let cc = buffer.char_count_for_byte_range(s, e);
                                spans.push(RenderSpan {
                                    display_text: text,
                                    style: quote_style,
                                    raw_byte_range: s..e,
                                    raw_char_count: cc,
                                    is_decoration: false,
                                });
                                break;
                            }
                        }
                        cursor.goto_parent();
                    }
                }
                byte_pos = child_end.min(line_end_byte);
            }
            _ => {
                // Other children: show with quote style
                let s = child_start.max(line_start_byte);
                let e = child_end.min(line_end_byte);
                let text = buffer.text_for_byte_range(s, e);
                let cc = buffer.char_count_for_byte_range(s, e);
                spans.push(RenderSpan {
                    display_text: text,
                    style: quote_style,
                    raw_byte_range: s..e,
                    raw_char_count: cc,
                    is_decoration: false,
                });
                byte_pos = e;
            }
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    if byte_pos < line_end_byte {
        let text = buffer.text_for_byte_range(byte_pos, line_end_byte);
        let cc = buffer.char_count_for_byte_range(byte_pos, line_end_byte);
        spans.push(RenderSpan {
            display_text: text,
            style: quote_style,
            raw_byte_range: byte_pos..line_end_byte,
            raw_char_count: cc,
            is_decoration: false,
        });
    }
}

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
    use crate::editor::buffer::Buffer;
    use crate::markdown::MarkdownState;
    use crate::render::engine::compute_render_lines;

    fn make_state(text: &str) -> (Buffer, MarkdownState) {
        let buffer = Buffer::from_str(text);
        let mut state = MarkdownState::new();
        state.parse_full(&buffer);
        (buffer, state)
    }

    fn display_text(line: &crate::render::plan::RenderLine) -> String {
        line.spans.iter().map(|s| s.display_text.as_str()).collect()
    }

    #[test]
    fn h1_concealed_hides_marker() {
        let (buffer, state) = make_state("# Hello World\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        assert_eq!(display_text(&lines[0]), "Hello World");
    }

    #[test]
    fn h1_revealed_shows_marker() {
        let (buffer, state) = make_state("# Hello World\n");
        // Cursor at byte 5 (inside "Hello")
        let lines = compute_render_lines(&state, &buffer, 5, 0, 1);
        let text = display_text(&lines[0]);
        assert!(text.contains("# "), "Expected '# ' in '{}'", text);
    }

    #[test]
    fn h2_concealed() {
        let (buffer, state) = make_state("## Section\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        assert_eq!(display_text(&lines[0]), "Section");
    }

    #[test]
    fn h3_concealed() {
        let (buffer, state) = make_state("### Subsection\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        assert_eq!(display_text(&lines[0]), "Subsection");
    }

    #[test]
    fn heading_with_inline_formatting() {
        let (buffer, state) = make_state("# Hello **World**\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        // Both # marker and ** markers should be hidden
        assert_eq!(display_text(&lines[0]), "Hello World");
    }

    #[test]
    fn thematic_break_concealed() {
        let (buffer, state) = make_state("---\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        let text = display_text(&lines[0]);
        // Should contain horizontal line chars
        assert!(text.contains('─'), "Expected ─ in '{}'", text);
    }

    #[test]
    fn blockquote_concealed_shows_border() {
        let (buffer, state) = make_state("> quoted text\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        let text = display_text(&lines[0]);
        assert!(text.contains('│'), "Expected │ in '{}'", text);
        assert!(text.contains("quoted text"), "Expected content in '{}'", text);
    }

    #[test]
    fn blockquote_hides_marker() {
        let (buffer, state) = make_state("> hello\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        let text = display_text(&lines[0]);
        // Should not show the raw >
        assert!(!text.contains('>'), "Should not contain raw > in '{}'", text);
    }
}
