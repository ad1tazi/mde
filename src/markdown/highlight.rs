use ratatui::style::{Color, Modifier, Style};

use super::MarkdownState;
use crate::editor::buffer::Buffer;

#[derive(Debug, Clone)]
pub struct HighlightSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub style: Style,
}

#[derive(Debug, Default)]
pub struct HighlightMap {
    spans: Vec<HighlightSpan>,
}

impl HighlightMap {
    pub fn new() -> Self {
        Self { spans: Vec::new() }
    }

    pub fn push(&mut self, start_byte: usize, end_byte: usize, style: Style) {
        if start_byte < end_byte {
            self.spans.push(HighlightSpan {
                start_byte,
                end_byte,
                style,
            });
        }
    }

    pub fn finalize(&mut self) {
        self.spans.sort_by_key(|s| s.start_byte);
    }

    /// Get the merged style for a given byte offset.
    /// Overlapping spans are layered via Style::patch (deeper/later wins).
    pub fn style_at(&self, byte_offset: usize) -> Style {
        let mut result = Style::default();
        for span in &self.spans {
            if span.start_byte <= byte_offset && byte_offset < span.end_byte {
                result = result.patch(span.style);
            }
        }
        result
    }
}

pub fn compute(
    state: &MarkdownState,
    buffer: &Buffer,
    visible_line_start: usize,
    visible_line_end: usize,
    _cursor_byte: usize,
) -> HighlightMap {
    let mut map = HighlightMap::new();

    let tree = match state.tree() {
        Some(t) => t,
        None => return map,
    };

    let start_byte = buffer.line_to_byte(visible_line_start);
    let end_byte = if visible_line_end >= buffer.len_lines() {
        buffer.len_bytes()
    } else {
        buffer.line_to_byte(visible_line_end)
    };

    let mut cursor = tree.walk();
    walk_and_highlight(&mut cursor, &mut map, start_byte, end_byte);

    map.finalize();
    map
}

fn walk_and_highlight(
    cursor: &mut tree_sitter_md::MarkdownCursor<'_>,
    map: &mut HighlightMap,
    vis_start: usize,
    vis_end: usize,
) {
    loop {
        let node = cursor.node();
        let node_start = node.start_byte();
        let node_end = node.end_byte();

        // Skip nodes entirely outside the visible range
        if node_end <= vis_start || node_start >= vis_end {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        if let Some(style) = style_for_node(node.kind()) {
            map.push(node_start, node_end, style);
        }

        // Recurse into children
        if cursor.goto_first_child() {
            walk_and_highlight(cursor, map, vis_start, vis_end);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn style_for_node(kind: &str) -> Option<Style> {
    match kind {
        // Headings
        "atx_heading" | "setext_heading" => {
            Some(Style::default().add_modifier(Modifier::BOLD))
        }
        "atx_h1_marker" => {
            Some(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        }
        "atx_h2_marker" => {
            Some(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD))
        }
        "atx_h3_marker" => {
            Some(Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
        }
        "atx_h4_marker" | "atx_h5_marker" | "atx_h6_marker" => {
            Some(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD))
        }
        "setext_h1_underline" | "setext_h2_underline" => {
            Some(Style::default().fg(Color::DarkGray))
        }

        // Inline emphasis
        "emphasis" => Some(Style::default().add_modifier(Modifier::ITALIC)),
        "strong_emphasis" => Some(Style::default().add_modifier(Modifier::BOLD)),
        "strikethrough" => Some(Style::default().add_modifier(Modifier::CROSSED_OUT)),
        "emphasis_delimiter" => Some(Style::default().fg(Color::DarkGray)),

        // Code
        "code_span" => Some(Style::default().bg(Color::Indexed(236))),
        "code_span_delimiter" => Some(Style::default().fg(Color::DarkGray).bg(Color::Indexed(236))),
        "fenced_code_block" => Some(Style::default().fg(Color::Green)),
        "fenced_code_block_delimiter" | "code_fence_content" => {
            Some(Style::default().fg(Color::DarkGray))
        }
        "info_string" => Some(Style::default().fg(Color::Yellow)),

        // Block quote
        "block_quote_marker" => Some(Style::default().fg(Color::DarkGray)),
        "block_quote" => Some(Style::default().fg(Color::Indexed(245))),

        // Thematic break
        "thematic_break" => Some(Style::default().fg(Color::DarkGray)),

        // List markers
        "list_marker_minus" | "list_marker_plus" | "list_marker_star"
        | "list_marker_dot" | "list_marker_parenthesis" => {
            Some(Style::default().fg(Color::Yellow))
        }

        // Task list markers
        "task_list_marker_checked" => Some(Style::default().fg(Color::Green)),
        "task_list_marker_unchecked" => Some(Style::default().fg(Color::DarkGray)),

        // Links
        "link_text" | "image_description" => {
            Some(Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED))
        }
        "link_destination" => Some(Style::default().fg(Color::DarkGray)),
        "uri_autolink" | "email_autolink" => {
            Some(Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED))
        }

        _ => None,
    }
}
