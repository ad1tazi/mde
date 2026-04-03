use std::path::PathBuf;

use ratatui::{
    buffer::Buffer as RatatuiBuffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::filetree::{FileTree, NodeKind};

pub struct SidebarState {
    pub file_tree: FileTree,
    pub scroll_offset: usize,
}

impl SidebarState {
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            file_tree: FileTree::scan(root),
            scroll_offset: 0,
        }
    }

    pub fn adjust_scroll(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        let sel = self.file_tree.selected_index;
        if sel < self.scroll_offset {
            self.scroll_offset = sel;
        }
        if sel >= self.scroll_offset + viewport_height {
            self.scroll_offset = sel - viewport_height + 1;
        }
    }
}

pub struct SidebarWidget<'a> {
    pub state: &'a SidebarState,
    pub focused: bool,
    pub dirty_paths: &'a [Option<PathBuf>],
}

impl<'a> SidebarWidget<'a> {
    fn is_dirty(&self, path: &std::path::Path) -> bool {
        self.dirty_paths.iter().any(|p| p.as_deref() == Some(path))
    }
}

impl<'a> Widget for SidebarWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatatuiBuffer) {
        let height = area.height as usize;
        // Width is area.width minus 1 for the border
        let content_width = area.width.saturating_sub(1) as usize;

        let flat = &self.state.file_tree.flat_view;
        let scroll = self.state.scroll_offset;
        let selected = self.state.file_tree.selected_index;

        // Fill background
        let bg_style = Style::default().bg(Color::Indexed(235));
        for row in 0..area.height {
            for col in 0..area.width.saturating_sub(1) {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(
                    area.x + col,
                    area.y + row,
                )) {
                    cell.set_style(bg_style);
                    cell.set_char(' ');
                }
            }
        }

        // Render entries
        for i in 0..height {
            let entry_idx = scroll + i;
            if entry_idx >= flat.len() {
                break;
            }
            let entry = &flat[entry_idx];
            let y = area.y + i as u16;

            let is_selected = entry_idx == selected;

            let indent = entry.depth * 2;
            let prefix = match entry.kind {
                NodeKind::Directory => {
                    if entry.expanded {
                        "▼ "
                    } else {
                        "▶ "
                    }
                }
                NodeKind::File => "  ",
            };

            let dirty_marker = if entry.kind == NodeKind::File && self.is_dirty(&entry.path) {
                " ●"
            } else {
                ""
            };

            let text = format!(
                "{:indent$}{}{}{}",
                "",
                prefix,
                entry.name,
                dirty_marker,
                indent = indent
            );

            // Truncate to content_width
            let display: String = text.chars().take(content_width).collect();

            let style = if is_selected {
                let base = Style::default().bg(Color::Indexed(238)).fg(Color::White);
                if self.focused {
                    base.add_modifier(Modifier::BOLD)
                } else {
                    base
                }
            } else {
                match entry.kind {
                    NodeKind::Directory => {
                        Style::default().bg(Color::Indexed(235)).fg(Color::Indexed(75))
                    }
                    NodeKind::File => Style::default().bg(Color::Indexed(235)).fg(Color::White),
                }
            };

            buf.set_string(area.x, y, &display, style);

            // Fill remaining columns with background
            let display_len = display.chars().count();
            if display_len < content_width {
                let pad = " ".repeat(content_width - display_len);
                buf.set_string(area.x + display_len as u16, y, &pad, style);
            }
        }

        // Right border
        let border_style = Style::default().fg(Color::Indexed(240)).bg(Color::Indexed(235));
        let border_x = area.x + area.width - 1;
        for row in 0..area.height {
            buf.set_string(border_x, area.y + row, "│", border_style);
        }
    }
}
