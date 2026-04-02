use ratatui::{
    buffer::Buffer as RatatuiBuffer,
    layout::{Position, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{StatefulWidget as RatatuiStatefulWidget, Widget},
};
use unicode_width::UnicodeWidthChar;

use crate::editor::Editor;
use crate::render::header_image::HeaderImageSupport;
use crate::render::plan::RenderLine;

use ratatui_image::{Resize, StatefulImage};

pub struct EditorWidget<'a> {
    pub editor: &'a Editor,
    pub render_lines: &'a [RenderLine],
    pub image_support: Option<&'a mut HeaderImageSupport>,
    /// When true, image areas are marked as skip instead of re-rendered.
    pub skip_unchanged_images: bool,
}

impl<'a> Widget for EditorWidget<'a> {
    fn render(mut self, area: Rect, buf: &mut RatatuiBuffer) {
        let viewport_height = area.height as usize;
        let viewport_width = area.width as usize;

        let mut screen_row: usize = 0;
        let mut line_index: usize = 0;

        while screen_row < viewport_height && line_index < self.render_lines.len() {
            let render_line = &self.render_lines[line_index];

            if let Some(ref img_header) = render_line.image_header {
                let rows = img_header.display_rows as usize;
                let rows_available = viewport_height - screen_row;
                let rows_to_use = rows.min(rows_available);

                if self.skip_unchanged_images {
                    // Image hasn't changed since last frame — mark cells
                    // as skip so ratatui keeps the existing terminal content
                    // without re-sending image protocol escape sequences.
                    for row in 0..rows_to_use as u16 {
                        for col in 0..area.width {
                            if let Some(cell) = buf.cell_mut(Position::new(
                                area.x + col,
                                area.y + screen_row as u16 + row,
                            )) {
                                cell.set_skip(true);
                            }
                        }
                    }
                    screen_row += rows_to_use;
                } else if let Some(ref mut support) = self.image_support {
                    let protocol = support.get_or_render(
                        &img_header.text,
                        img_header.tier,
                        area.width,
                    );
                    let image_area = Rect {
                        x: area.x,
                        y: area.y + screen_row as u16,
                        width: area.width,
                        height: rows_to_use as u16,
                    };
                    let image_widget = StatefulImage::default().resize(Resize::Fit(None));
                    RatatuiStatefulWidget::render(image_widget, image_area, buf, protocol);
                    screen_row += rows_to_use;
                } else {
                    // No image support — fallback to text spans
                    let rows_used = render_text_line(
                        render_line,
                        screen_row,
                        area,
                        buf,
                        viewport_width,
                        viewport_height,
                        self.editor,
                    );
                    screen_row += rows_used;
                }
            } else {
                // Normal text line (may wrap across multiple screen rows)
                let rows_used = render_text_line(
                    render_line,
                    screen_row,
                    area,
                    buf,
                    viewport_width,
                    viewport_height,
                    self.editor,
                );
                screen_row += rows_used;
            }

            line_index += 1;
        }

        // Fill remaining rows with tildes
        while screen_row < viewport_height {
            buf.set_string(
                area.x,
                area.y + screen_row as u16,
                "~",
                Style::default().fg(Color::DarkGray),
            );
            screen_row += 1;
        }
    }
}

/// Render a text line with soft wrapping. Returns the number of screen rows consumed.
fn render_text_line(
    render_line: &RenderLine,
    start_screen_row: usize,
    area: Rect,
    buf: &mut RatatuiBuffer,
    viewport_width: usize,
    viewport_height: usize,
    editor: &Editor,
) -> usize {
    if viewport_width == 0 {
        return 1;
    }

    let line_idx = render_line.line_idx;
    let mut display_col: usize = 0;

    for span in &render_line.spans {
        let style = span.style;

        for ch in span.display_text.chars() {
            if ch == '\n' {
                break;
            }

            let ch_width = if ch == '\t' {
                4 - (display_col % 4)
            } else {
                UnicodeWidthChar::width(ch).unwrap_or(0)
            };

            // Handle wide characters (e.g. CJK) that would straddle the wrap boundary:
            // push them to the next row, leaving a gap.
            let mut x_in_row = display_col % viewport_width;
            if ch_width > 1 && x_in_row + ch_width > viewport_width {
                display_col += viewport_width - x_in_row;
                x_in_row = 0;
            }

            let wrap_row = display_col / viewport_width;
            let actual_row = start_screen_row + wrap_row;

            if actual_row >= viewport_height {
                // Off the bottom of the viewport — stop painting
                break;
            }

            let final_style = if !span.is_decoration {
                if let Some(ref sel) = editor.selection {
                    let raw_col = render_line
                        .position_map
                        .display_to_raw_col(display_col);
                    if sel.contains(line_idx, raw_col, &editor.buffer) {
                        Style::default().bg(Color::Indexed(24)).fg(Color::White)
                    } else {
                        style
                    }
                } else {
                    style
                }
            } else {
                style
            };

            if ch == '\t' {
                for i in 0..ch_width {
                    let col = display_col + i;
                    let tr = start_screen_row + col / viewport_width;
                    let tx = col % viewport_width;
                    if tr < viewport_height {
                        buf.set_string(
                            area.x + tx as u16,
                            area.y + tr as u16,
                            " ",
                            final_style,
                        );
                    }
                }
            } else {
                buf.set_string(
                    area.x + x_in_row as u16,
                    area.y + actual_row as u16,
                    &ch.to_string(),
                    final_style,
                );
            }

            display_col += ch_width;
        }
    }

    // Return screen rows consumed (minimum 1 for empty lines)
    let rows = if display_col > 0 {
        (display_col + viewport_width - 1) / viewport_width
    } else {
        1
    };
    rows
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
    let right = format!(
        "Ln {}, Col {}  UTF-8 ",
        editor.cursor.line + 1,
        editor.cursor.col + 1
    );

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
