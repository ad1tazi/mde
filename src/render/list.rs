use ratatui::style::{Color, Style};
use tree_sitter_md::MarkdownCursor;

use crate::editor::buffer::Buffer;
use crate::markdown::reveal::RevealSet;
use crate::render::inline;
use crate::render::plan::RenderSpan;

/// Render a list_item node.
///
/// When concealed:
/// - Unordered markers (`- `, `* `, `+ `) → replaced with `• `
/// - Task list markers `[x]` → `☑`, `[ ]` → `☐`
/// - Content rendered with inline formatting
///
/// When revealed (cursor on marker): show raw markers.
///
/// `cursor` must be positioned at the `list_item` node.
pub fn render_list_item(
    cursor: &mut MarkdownCursor<'_>,
    buffer: &Buffer,
    reveal_set: &RevealSet,
    line_start_byte: usize,
    line_end_byte: usize,
    viewport_width: usize,
    spans: &mut Vec<RenderSpan>,
) {
    if !cursor.goto_first_child() {
        // No children — raw fallback
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

        // Gap before child (e.g., indentation)
        if byte_pos < child_start.min(line_end_byte) {
            let gap_end = child_start.min(line_end_byte);
            let text = buffer.text_for_byte_range(byte_pos, gap_end);
            let cc = buffer.char_count_for_byte_range(byte_pos, gap_end);
            spans.push(RenderSpan {
                display_text: text,
                style: Style::default(),
                raw_byte_range: byte_pos..gap_end,
                raw_char_count: cc,
                is_decoration: false,
            });
            byte_pos = gap_end;
        }

        match child_kind {
            "list_marker_minus" | "list_marker_plus" | "list_marker_star" => {
                let marker_range = child_start..child_end;
                if reveal_set.is_revealed(&marker_range) {
                    // Show raw marker
                    let text = buffer.text_for_byte_range(child_start, child_end.min(line_end_byte));
                    let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                    let style = crate::markdown::highlight::style_for_node(child_kind)
                        .unwrap_or_default();
                    spans.push(RenderSpan {
                        display_text: text,
                        style,
                        raw_byte_range: child_start..child_end.min(line_end_byte),
                        raw_char_count: cc,
                        is_decoration: false,
                    });
                } else {
                    // Replace with bullet
                    let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                    spans.push(RenderSpan {
                        display_text: "• ".to_string(),
                        style: Style::default(),
                        raw_byte_range: child_start..child_end.min(line_end_byte),
                        raw_char_count: cc,
                        is_decoration: false,
                    });
                }
                byte_pos = child_end.min(line_end_byte);
            }
            "list_marker_dot" | "list_marker_parenthesis" => {
                // Ordered list markers: show as-is with styling
                let text = buffer.text_for_byte_range(child_start, child_end.min(line_end_byte));
                let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                let style = crate::markdown::highlight::style_for_node(child_kind)
                    .unwrap_or_default();
                spans.push(RenderSpan {
                    display_text: text,
                    style,
                    raw_byte_range: child_start..child_end.min(line_end_byte),
                    raw_char_count: cc,
                    is_decoration: false,
                });
                byte_pos = child_end.min(line_end_byte);
            }
            "task_list_marker_checked" => {
                let marker_range = child_start..child_end;
                if reveal_set.is_revealed(&marker_range) {
                    let text = buffer.text_for_byte_range(child_start, child_end.min(line_end_byte));
                    let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                    let style = crate::markdown::highlight::style_for_node(child_kind)
                        .unwrap_or_default();
                    spans.push(RenderSpan {
                        display_text: text,
                        style,
                        raw_byte_range: child_start..child_end.min(line_end_byte),
                        raw_char_count: cc,
                        is_decoration: false,
                    });
                } else {
                    let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                    spans.push(RenderSpan {
                        display_text: "☑".to_string(),
                        style: Style::default().fg(Color::Green),
                        raw_byte_range: child_start..child_end.min(line_end_byte),
                        raw_char_count: cc,
                        is_decoration: false,
                    });
                }
                byte_pos = child_end.min(line_end_byte);
                // Skip the space after the task marker
                if byte_pos < line_end_byte {
                    let next = buffer.text_for_byte_range(byte_pos, (byte_pos + 1).min(line_end_byte));
                    if next == " " {
                        spans.push(RenderSpan {
                            display_text: " ".to_string(),
                            style: Style::default(),
                            raw_byte_range: byte_pos..byte_pos + 1,
                            raw_char_count: 1,
                            is_decoration: false,
                        });
                        byte_pos += 1;
                    }
                }
            }
            "task_list_marker_unchecked" => {
                let marker_range = child_start..child_end;
                if reveal_set.is_revealed(&marker_range) {
                    let text = buffer.text_for_byte_range(child_start, child_end.min(line_end_byte));
                    let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                    let style = crate::markdown::highlight::style_for_node(child_kind)
                        .unwrap_or_default();
                    spans.push(RenderSpan {
                        display_text: text,
                        style,
                        raw_byte_range: child_start..child_end.min(line_end_byte),
                        raw_char_count: cc,
                        is_decoration: false,
                    });
                } else {
                    let cc = buffer.char_count_for_byte_range(child_start, child_end.min(line_end_byte));
                    spans.push(RenderSpan {
                        display_text: "☐".to_string(),
                        style: Style::default().fg(Color::DarkGray),
                        raw_byte_range: child_start..child_end.min(line_end_byte),
                        raw_char_count: cc,
                        is_decoration: false,
                    });
                }
                byte_pos = child_end.min(line_end_byte);
                // Skip the space after the task marker
                if byte_pos < line_end_byte {
                    let next = buffer.text_for_byte_range(byte_pos, (byte_pos + 1).min(line_end_byte));
                    if next == " " {
                        spans.push(RenderSpan {
                            display_text: " ".to_string(),
                            style: Style::default(),
                            raw_byte_range: byte_pos..byte_pos + 1,
                            raw_char_count: 1,
                            is_decoration: false,
                        });
                        byte_pos += 1;
                    }
                }
            }
            "paragraph" => {
                // Descend into paragraph → inline for content rendering
                if cursor.goto_first_child() {
                    loop {
                        let inner = cursor.node();
                        if inner.kind() == "inline" {
                            inline::render_inline(
                                cursor,
                                buffer,
                                reveal_set,
                                line_start_byte,
                                line_end_byte,
                                Style::default(),
                                spans,
                            );
                            break;
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                    cursor.goto_parent();
                }
                byte_pos = child_end.min(line_end_byte);
            }
            _ => {
                // Other children: show raw
                let s = child_start.max(line_start_byte);
                let e = child_end.min(line_end_byte);
                let text = buffer.text_for_byte_range(s, e);
                let cc = buffer.char_count_for_byte_range(s, e);
                spans.push(RenderSpan {
                    display_text: text,
                    style: Style::default(),
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
            style: Style::default(),
            raw_byte_range: byte_pos..line_end_byte,
            raw_char_count: cc,
            is_decoration: false,
        });
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
    fn unordered_list_bullet_replacement() {
        let (buffer, state) = make_state("- item one\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        assert_eq!(display_text(&lines[0]), "• item one");
    }

    #[test]
    fn unordered_list_star_marker() {
        let (buffer, state) = make_state("* item\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        assert_eq!(display_text(&lines[0]), "• item");
    }

    #[test]
    fn task_list_checked() {
        let (buffer, state) = make_state("- [x] done\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        let text = display_text(&lines[0]);
        assert!(text.contains('☑'), "Expected ☑ in '{}'", text);
        assert!(text.contains("done"), "Expected 'done' in '{}'", text);
    }

    #[test]
    fn task_list_unchecked() {
        let (buffer, state) = make_state("- [ ] todo\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        let text = display_text(&lines[0]);
        assert!(text.contains('☐'), "Expected ☐ in '{}'", text);
        assert!(text.contains("todo"), "Expected 'todo' in '{}'", text);
    }

    #[test]
    fn list_with_inline_formatting() {
        let (buffer, state) = make_state("- **bold item**\n");
        let lines = compute_render_lines(&state, &buffer, 999, 0, 1);
        assert_eq!(display_text(&lines[0]), "• bold item");
    }
}
