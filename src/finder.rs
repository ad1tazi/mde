use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::{
    buffer::Buffer as RatatuiBuffer,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    widgets::Widget,
};

pub struct FilteredEntry {
    pub path: PathBuf,
    pub display: String,
    pub score: u32,
}

pub struct FinderState {
    pub query: String,
    pub all_files: Vec<PathBuf>,
    pub filtered: Vec<FilteredEntry>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    root: PathBuf,
}

impl FinderState {
    pub fn new(root: &Path) -> Self {
        let mut all_files = Vec::new();

        let walker = WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path().to_path_buf();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "md" {
                        all_files.push(path);
                    }
                }
            }
        }

        all_files.sort();

        let root_path = root.to_path_buf();
        let filtered = all_files
            .iter()
            .map(|p| {
                let display = p
                    .strip_prefix(&root_path)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned();
                FilteredEntry {
                    path: p.clone(),
                    display,
                    score: 0,
                }
            })
            .collect();

        FinderState {
            query: String::new(),
            all_files,
            filtered,
            selected_index: 0,
            scroll_offset: 0,
            root: root_path,
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        self.query.push(ch);
        self.refilter();
    }

    pub fn delete_backward(&mut self) {
        self.query.pop();
        self.refilter();
    }

    fn refilter(&mut self) {
        if self.query.is_empty() {
            self.filtered = self
                .all_files
                .iter()
                .map(|p| {
                    let display = p
                        .strip_prefix(&self.root)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .into_owned();
                    FilteredEntry {
                        path: p.clone(),
                        display,
                        score: 0,
                    }
                })
                .collect();
            self.selected_index = 0;
            self.scroll_offset = 0;
            return;
        }

        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let pattern = Pattern::new(
            &self.query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );

        let mut results: Vec<FilteredEntry> = Vec::new();

        for file in &self.all_files {
            let display = file
                .strip_prefix(&self.root)
                .unwrap_or(file)
                .to_string_lossy()
                .into_owned();

            let mut buf = Vec::new();
            let haystack = Utf32Str::new(&display, &mut buf);

            if let Some(score) = pattern.score(haystack, &mut matcher) {
                results.push(FilteredEntry {
                    path: file.clone(),
                    display,
                    score,
                });
            }
        }

        results.sort_by(|a, b| b.score.cmp(&a.score));

        self.filtered = results;
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() && self.selected_index < self.filtered.len() - 1 {
            self.selected_index += 1;
        }
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.filtered
            .get(self.selected_index)
            .map(|e| e.path.as_path())
    }
}

pub struct FinderWidget<'a> {
    pub state: &'a FinderState,
}

impl<'a> FinderWidget<'a> {
    fn popup_area(area: Rect) -> Rect {
        let width = (area.width * 60 / 100).max(30).min(area.width);
        let height = (area.height * 50 / 100).max(10).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 3; // slightly above center
        Rect::new(x, y, width, height)
    }
}

impl<'a> Widget for FinderWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatatuiBuffer) {
        let popup = Self::popup_area(area);

        let border_style = Style::default().fg(Color::Indexed(75)).bg(Color::Indexed(236));
        let bg_style = Style::default().bg(Color::Indexed(236)).fg(Color::White);

        // Fill background
        for row in 0..popup.height {
            for col in 0..popup.width {
                if let Some(cell) = buf.cell_mut(Position::new(
                    popup.x + col,
                    popup.y + row,
                )) {
                    cell.set_style(bg_style);
                    cell.set_char(' ');
                }
            }
        }

        // Top border
        let top = format!(
            "┌{}┐",
            "─".repeat(popup.width.saturating_sub(2) as usize)
        );
        buf.set_string(popup.x, popup.y, &top, border_style);

        // Bottom border
        let bottom = format!(
            "└{}┘",
            "─".repeat(popup.width.saturating_sub(2) as usize)
        );
        buf.set_string(popup.x, popup.y + popup.height - 1, &bottom, border_style);

        // Side borders
        for row in 1..popup.height.saturating_sub(1) {
            buf.set_string(popup.x, popup.y + row, "│", border_style);
            buf.set_string(popup.x + popup.width - 1, popup.y + row, "│", border_style);
        }

        // Input line (row 1 inside border)
        let inner_width = popup.width.saturating_sub(4) as usize;
        let input_line = format!(" > {}", self.state.query);
        let display_input: String = input_line.chars().take(inner_width + 2).collect();
        let input_style = Style::default().bg(Color::Indexed(238)).fg(Color::White);
        // Fill input row
        let input_y = popup.y + 1;
        for col in 1..popup.width.saturating_sub(1) {
            if let Some(cell) = buf.cell_mut(Position::new(popup.x + col, input_y)) {
                cell.set_style(input_style);
                cell.set_char(' ');
            }
        }
        buf.set_string(popup.x + 1, input_y, &display_input, input_style);

        // Separator
        let sep_y = popup.y + 2;
        let sep = "─".repeat(popup.width.saturating_sub(2) as usize);
        buf.set_string(popup.x + 1, sep_y, &sep, border_style);

        // Results
        let results_start = 3u16;
        let results_height = popup.height.saturating_sub(results_start + 1) as usize;

        // Adjust scroll to keep selection visible
        let mut scroll = self.state.scroll_offset;
        if self.state.selected_index < scroll {
            scroll = self.state.selected_index;
        }
        if self.state.selected_index >= scroll + results_height {
            scroll = self.state.selected_index - results_height + 1;
        }

        let count_text = format!("{} files", self.state.filtered.len());
        let count_style = Style::default()
            .bg(Color::Indexed(236))
            .fg(Color::Indexed(245));
        buf.set_string(
            popup.x + popup.width - 1 - count_text.len() as u16,
            input_y,
            &count_text,
            count_style,
        );

        for i in 0..results_height {
            let entry_idx = scroll + i;
            if entry_idx >= self.state.filtered.len() {
                break;
            }
            let entry = &self.state.filtered[entry_idx];
            let y = popup.y + results_start + i as u16;
            let is_selected = entry_idx == self.state.selected_index;

            let style = if is_selected {
                Style::default()
                    .bg(Color::Indexed(238))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(Color::Indexed(236)).fg(Color::White)
            };

            let text: String = format!(" {}", entry.display)
                .chars()
                .take(inner_width + 2)
                .collect();

            // Fill row
            for col in 1..popup.width.saturating_sub(1) {
                if let Some(cell) = buf.cell_mut(Position::new(popup.x + col, y)) {
                    cell.set_style(style);
                    cell.set_char(' ');
                }
            }
            buf.set_string(popup.x + 1, y, &text, style);
        }
    }
}
