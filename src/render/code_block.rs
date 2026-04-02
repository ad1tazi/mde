use ratatui::style::{Color, Style};
use tree_sitter_md::MarkdownTree;

use crate::editor::buffer::Buffer;
use crate::markdown::reveal::RevealSet;
use crate::render::plan::RenderSpan;
use crate::render::syntax;

const CODE_BG: Color = Color::Indexed(235);
const BORDER_STYLE_COLOR: Color = Color::DarkGray;

/// Render a single line that is part of a fenced code block.
///
/// When the entire block is revealed (cursor inside), show raw text.
/// When concealed:
/// - Opening fence line → top border with language label: `┌─ rust ──────┐`
/// - Content lines → side borders: `│ content      │`
/// - Closing fence line → bottom border: `└─────────────┘`
pub fn render_fenced_code_block_line(
    buffer: &Buffer,
    tree: &MarkdownTree,
    reveal_set: &RevealSet,
    line_start_byte: usize,
    line_end_byte: usize,
    viewport_width: usize,
    spans: &mut Vec<RenderSpan>,
) {
    // Find the fenced_code_block node that contains this line
    let mut cursor = tree.walk();
    let mut block_start = 0;
    let mut block_end = 0;
    let mut found_block = false;

    // Navigate to the fenced_code_block containing this line
    loop {
        let node = cursor.node();
        if node.kind() == "fenced_code_block" {
            block_start = node.start_byte();
            block_end = node.end_byte();
            found_block = true;
            break;
        }
        if cursor.goto_first_child_for_byte(line_start_byte).is_none() {
            break;
        }
    }

    if !found_block {
        // Shouldn't happen — fallback to raw
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

    // Check if the entire block is revealed
    if reveal_set.is_revealed(&(block_start..block_end)) {
        // Show raw with highlighting
        render_raw_code_line(buffer, line_start_byte, line_end_byte, &mut cursor, spans);
        return;
    }

    // Collect block structure info by walking children
    let mut info_string = String::new();
    let mut open_fence_end_byte = block_start;
    let mut close_fence_start_byte = block_end;
    let mut _content_start_byte = block_start;
    let mut _content_end_byte = block_end;

    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match child.kind() {
                "fenced_code_block_delimiter" => {
                    if child.start_byte() == block_start {
                        // Opening fence
                        open_fence_end_byte = child.end_byte();
                    } else {
                        // Closing fence
                        close_fence_start_byte = child.start_byte();
                    }
                }
                "info_string" => {
                    info_string = buffer.text_for_byte_range(child.start_byte(), child.end_byte());
                }
                "code_fence_content" => {
                    _content_start_byte = child.start_byte();
                    _content_end_byte = child.end_byte();
                }
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }

    // Find the first line byte after the opening fence (skip the \n)
    let first_content_line_byte = {
        let text = buffer.text_for_byte_range(open_fence_end_byte, block_end);
        let newline_pos = text.find('\n').unwrap_or(0);
        open_fence_end_byte + newline_pos + 1
    };

    // Determine which part of the code block this line is
    let border_style = Style::default().fg(BORDER_STYLE_COLOR);
    let code_style = Style::default().bg(CODE_BG).fg(Color::Green);
    let inner_width = viewport_width.saturating_sub(4); // 2 for "│ " + 2 for " │"

    if line_start_byte < first_content_line_byte {
        // This is the opening fence line — render as top border
        let raw_cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
        let label = if info_string.is_empty() {
            String::new()
        } else {
            format!(" {} ", info_string)
        };
        let label_len = label.len();
        let remaining = viewport_width.saturating_sub(2 + label_len); // 1 for ┌, 1 for ┐
        let top_border = format!(
            "┌─{}{}┐",
            label,
            "─".repeat(remaining.saturating_sub(1))
        );
        spans.push(RenderSpan {
            display_text: top_border,
            style: border_style,
            raw_byte_range: line_start_byte..line_end_byte,
            raw_char_count: raw_cc,
            is_decoration: false,
        });
    } else if line_start_byte >= close_fence_start_byte {
        // This is the closing fence line — render as bottom border
        let raw_cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
        let bottom_border = format!(
            "└{}┘",
            "─".repeat(viewport_width.saturating_sub(2))
        );
        spans.push(RenderSpan {
            display_text: bottom_border,
            style: border_style,
            raw_byte_range: line_start_byte..line_end_byte,
            raw_char_count: raw_cc,
            is_decoration: false,
        });
    } else {
        // Content line — render with side borders
        let raw_text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
        let raw_cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);

        // Left border (decoration)
        spans.push(RenderSpan {
            display_text: "│ ".to_string(),
            style: border_style,
            raw_byte_range: 0..0,
            raw_char_count: 0,
            is_decoration: true,
        });

        // Try syntax highlighting
        let highlighted = if !info_string.is_empty() {
            highlight_content_line(buffer, &info_string, first_content_line_byte, line_start_byte, line_end_byte)
        } else {
            None
        };

        if let Some(tokens) = highlighted {
            // Emit one RenderSpan per token
            let mut byte_offset = line_start_byte;
            let mut display_width = 0usize;
            for token in tokens {
                let token_byte_len = token.text.len();
                let token_char_count = token.text.chars().count();
                let token_display_width: usize = token.text
                    .chars()
                    .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
                    .sum();
                display_width += token_display_width;
                spans.push(RenderSpan {
                    display_text: token.text,
                    style: token.style.bg(CODE_BG),
                    raw_byte_range: byte_offset..byte_offset + token_byte_len,
                    raw_char_count: token_char_count,
                    is_decoration: false,
                });
                byte_offset += token_byte_len;
            }
            // Padding to fill inner_width
            let padding = inner_width.saturating_sub(display_width);
            if padding > 0 {
                spans.push(RenderSpan {
                    display_text: " ".repeat(padding),
                    style: Style::default().bg(CODE_BG),
                    raw_byte_range: 0..0,
                    raw_char_count: 0,
                    is_decoration: true,
                });
            }
        } else {
            // Fallback: single span with uniform code style
            let content_display_width: usize = raw_text
                .chars()
                .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
                .sum();
            let padding = inner_width.saturating_sub(content_display_width);
            let padded_content = format!("{}{}", raw_text, " ".repeat(padding));
            spans.push(RenderSpan {
                display_text: padded_content,
                style: code_style,
                raw_byte_range: line_start_byte..line_end_byte,
                raw_char_count: raw_cc,
                is_decoration: false,
            });
        }

        // Right border (decoration)
        spans.push(RenderSpan {
            display_text: " │".to_string(),
            style: border_style,
            raw_byte_range: 0..0,
            raw_char_count: 0,
            is_decoration: true,
        });
    }
}

/// Highlight a content line using syntect.
///
/// Highlights all content lines from `content_start_byte` through `line_end_byte`
/// to propagate multi-line parse state (e.g., multi-line strings), then returns
/// tokens for only the last line (the one being rendered).
fn highlight_content_line(
    buffer: &Buffer,
    lang: &str,
    content_start_byte: usize,
    line_start_byte: usize,
    line_end_byte: usize,
) -> Option<Vec<syntax::StyledToken>> {
    // Extract all content from block start through this line
    let full_text = buffer.text_for_byte_range(content_start_byte, line_end_byte);
    // Split into newline-terminated lines for syntect
    let lines: Vec<String> = full_text.split('\n').enumerate().map(|(i, s)| {
        // All lines except possibly the last need \n appended back
        // (split removes the delimiter)
        if i < full_text.matches('\n').count() {
            format!("{}\n", s)
        } else {
            s.to_string()
        }
    }).collect();
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();

    let highlighted = syntax::highlight_code_lines(lang, &line_refs)?;
    let last = highlighted.into_iter().last()?;

    // Strip trailing \n from the last token if present, since the render engine
    // excludes trailing newlines from line byte ranges.
    let current_line_text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
    let mut tokens = last;
    if !current_line_text.ends_with('\n') {
        if let Some(last_token) = tokens.last_mut() {
            if last_token.text.ends_with('\n') {
                last_token.text.pop();
            }
        }
    }
    // Remove empty trailing token if stripping \n left it empty
    if tokens.last().map_or(false, |t| t.text.is_empty()) {
        tokens.pop();
    }

    Some(tokens)
}

/// Render a raw code line (when revealed).
fn render_raw_code_line(
    buffer: &Buffer,
    line_start_byte: usize,
    line_end_byte: usize,
    _cursor: &mut tree_sitter_md::MarkdownCursor<'_>,
    spans: &mut Vec<RenderSpan>,
) {
    // Just show with code block styling
    let text = buffer.text_for_byte_range(line_start_byte, line_end_byte);
    let cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
    let style = Style::default().fg(Color::Green);
    spans.push(RenderSpan {
        display_text: text,
        style,
        raw_byte_range: line_start_byte..line_end_byte,
        raw_char_count: cc,
        is_decoration: false,
    });
}

#[cfg(test)]
mod tests {
    use crate::editor::buffer::Buffer;
    use crate::markdown::MarkdownState;
    use crate::render::engine::compute_render_lines_with_width;

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
    fn code_block_top_border() {
        let text = "```rust\nfn main() {}\n```\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines_with_width(&state, &buffer, 999, 0, 3, 40, None);
        let top = display_text(&lines[0]);
        assert!(top.starts_with("┌─"), "Expected top border, got '{}'", top);
        assert!(top.contains("rust"), "Expected language label, got '{}'", top);
        assert!(top.ends_with('┐'), "Expected ┐ at end, got '{}'", top);
    }

    #[test]
    fn code_block_content_line() {
        let text = "```rust\nfn main() {}\n```\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines_with_width(&state, &buffer, 999, 0, 3, 40, None);
        let content = display_text(&lines[1]);
        assert!(content.starts_with("│ "), "Expected left border, got '{}'", content);
        assert!(content.contains("fn main()"), "Expected content, got '{}'", content);
        assert!(content.ends_with(" │"), "Expected right border, got '{}'", content);
    }

    #[test]
    fn code_block_bottom_border() {
        let text = "```rust\nfn main() {}\n```\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines_with_width(&state, &buffer, 999, 0, 3, 40, None);
        let bottom = display_text(&lines[2]);
        assert!(bottom.starts_with("└"), "Expected bottom border, got '{}'", bottom);
        assert!(bottom.ends_with('┘'), "Expected ┘ at end, got '{}'", bottom);
    }

    #[test]
    fn code_block_revealed() {
        let text = "```rust\nfn main() {}\n```\n";
        let (buffer, state) = make_state(text);
        // Cursor inside the code block (byte 10)
        let lines = compute_render_lines_with_width(&state, &buffer, 10, 0, 3, 40, None);
        let top = display_text(&lines[0]);
        // Should show raw opening fence
        assert!(top.contains("```"), "Expected raw fence, got '{}'", top);
    }

    #[test]
    fn code_block_syntax_highlighted_has_multiple_spans() {
        let text = "```rust\nfn main() {}\n```\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines_with_width(&state, &buffer, 999, 0, 3, 40, None);
        // Content line (line 1) should have more than 3 spans
        // (left border + multiple highlighted tokens + padding + right border)
        let content_line = &lines[1];
        assert!(
            content_line.spans.len() > 3,
            "Expected multiple spans from syntax highlighting, got {}",
            content_line.spans.len()
        );
        // Display text should still contain the code
        let dt = display_text(content_line);
        assert!(dt.contains("fn main()"), "Expected content, got '{}'", dt);
    }

    #[test]
    fn code_block_unknown_lang_falls_back() {
        let text = "```unknownlang123\nsome code\n```\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines_with_width(&state, &buffer, 999, 0, 3, 40, None);
        let content_line = &lines[1];
        // Should have exactly 3 spans: left border, content, right border
        assert_eq!(
            content_line.spans.len(),
            3,
            "Expected 3 spans for unknown lang, got {}",
            content_line.spans.len()
        );
    }

    #[test]
    fn code_block_no_lang_falls_back() {
        let text = "```\nsome code\n```\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines_with_width(&state, &buffer, 999, 0, 3, 40, None);
        let content_line = &lines[1];
        assert_eq!(
            content_line.spans.len(),
            3,
            "Expected 3 spans for no lang, got {}",
            content_line.spans.len()
        );
    }
}
