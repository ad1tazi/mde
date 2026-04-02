use ratatui::{
    buffer::Buffer as RatatuiBuffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Widget,
};
use unicode_width::UnicodeWidthChar;

use crate::editor::Editor;

pub struct EditorWidget<'a> {
    pub editor: &'a Editor,
}

impl<'a> Widget for EditorWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatatuiBuffer) {
        let viewport_height = area.height as usize;
        let viewport_width = area.width as usize;
        let scroll = self.editor.scroll_offset;

        for screen_row in 0..viewport_height {
            let line_idx = scroll + screen_row;
            if line_idx >= self.editor.buffer.len_lines() {
                // Draw tilde for empty lines past end of file
                buf.set_string(
                    area.x,
                    area.y + screen_row as u16,
                    "~",
                    Style::default().fg(Color::DarkGray),
                );
                continue;
            }

            let line = self.editor.buffer.line(line_idx);
            let mut display_col: usize = 0;

            for (char_idx, ch) in line.chars().enumerate() {
                if ch == '\n' {
                    break;
                }

                let ch_width = if ch == '\t' {
                    4 - (display_col % 4)
                } else {
                    UnicodeWidthChar::width(ch).unwrap_or(0)
                };

                if display_col + ch_width > viewport_width {
                    break;
                }

                // Determine style (selection highlighting)
                let style = if let Some(ref sel) = self.editor.selection {
                    if sel.contains(line_idx, char_idx, &self.editor.buffer) {
                        Style::default().bg(Color::Indexed(24)).fg(Color::White)
                    } else {
                        Style::default()
                    }
                } else {
                    Style::default()
                };

                if ch == '\t' {
                    // Render tab as spaces
                    for i in 0..ch_width {
                        if display_col + i < viewport_width {
                            buf.set_string(
                                area.x + (display_col + i) as u16,
                                area.y + screen_row as u16,
                                " ",
                                style,
                            );
                        }
                    }
                } else {
                    buf.set_string(
                        area.x + display_col as u16,
                        area.y + screen_row as u16,
                        &ch.to_string(),
                        style,
                    );
                }

                display_col += ch_width;
            }
        }
    }
}

pub fn render_status_bar(editor: &Editor, area: Rect, buf: &mut RatatuiBuffer) {
    let filename = editor
        .file_path
        .as_ref()
        .map(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| "[untitled]".into());

    let dirty = if editor.dirty { " [+]" } else { "" };

    let left = format!(" {}{}", filename, dirty);
    let right = format!("Ln {}, Col {}  UTF-8 ", editor.cursor.line + 1, editor.cursor.col + 1);

    let width = area.width as usize;
    let padding = width.saturating_sub(left.len() + right.len());

    let style = Style::default().bg(Color::Indexed(236)).fg(Color::White);

    let status_line = Line::from(vec![
        Span::styled(&left, style),
        Span::styled(" ".repeat(padding), style),
        Span::styled(&right, style),
    ]);

    buf.set_line(area.x, area.y, &status_line, area.width);
}
