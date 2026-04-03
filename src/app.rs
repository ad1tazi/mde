use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Position},
    Frame,
};
use std::path::PathBuf;
use std::time::Duration;

use crate::clipboard::Clipboard;
use crate::editor::Editor;
use crate::finder::{FinderState, FinderWidget};
use crate::input::{map_app_key_event, AppCommand, EditorCommand};
use crate::render::engine::compute_render_lines_with_width;
use crate::render::header_image::HeaderImageSupport;
use crate::render::widget::{render_status_bar, render_tab_bar, EditorWidget};
use crate::sidebar::{SidebarState, SidebarWidget};

/// Snapshot of the image layout from the previous frame, used to avoid
/// re-sending image protocol escape sequences when nothing changed.
#[derive(Clone, PartialEq)]
struct ImageLayoutEntry {
    screen_row: u16,
    rows: u16,
    text: String,
    tier: u8,
}

pub struct TabEntry {
    pub editor: Editor,
}

#[derive(Clone, Copy, PartialEq)]
pub enum FocusZone {
    Editor,
    Sidebar,
    Finder,
}

pub struct App {
    pub tabs: Vec<TabEntry>,
    pub active_tab: usize,
    pub clipboard: Clipboard,
    pub should_quit: bool,
    pub header_image: Option<HeaderImageSupport>,
    prev_image_layout: Vec<ImageLayoutEntry>,
    pub sidebar: Option<SidebarState>,
    pub sidebar_visible: bool,
    pub sidebar_width: u16,
    pub focus: FocusZone,
    pub finder: Option<FinderState>,
}

impl App {
    pub fn new(file_path: Option<&str>) -> std::io::Result<Self> {
        let editor = if let Some(path) = file_path {
            Editor::open_file(std::path::Path::new(path))?
        } else {
            Editor::new()
        };

        // Initialize image support (must happen after entering alternate screen)
        let header_image = HeaderImageSupport::new();

        // Initialize sidebar from cwd
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let sidebar = Some(SidebarState::new(&cwd));

        Ok(Self {
            tabs: vec![TabEntry { editor }],
            active_tab: 0,
            clipboard: Clipboard::new(),
            should_quit: false,
            header_image,
            prev_image_layout: Vec::new(),
            sidebar,
            sidebar_visible: false,
            sidebar_width: 25,
            focus: FocusZone::Editor,
            finder: None,
        })
    }

    pub fn active_editor(&self) -> &Editor {
        &self.tabs[self.active_tab].editor
    }

    pub fn active_editor_mut(&mut self) -> &mut Editor {
        &mut self.tabs[self.active_tab].editor
    }

    pub fn open_file_in_tab(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        // Check if file is already open — switch to that tab
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some(ref fp) = tab.editor.file_path {
                let tab_canonical = fp.canonicalize().unwrap_or_else(|_| fp.clone());
                if tab_canonical == canonical {
                    self.active_tab = i;
                    self.prev_image_layout.clear();
                    return Ok(());
                }
            }
        }
        // Open new tab
        let editor = Editor::open_file(path)?;
        self.tabs.push(TabEntry { editor });
        self.active_tab = self.tabs.len() - 1;
        self.prev_image_layout.clear();
        Ok(())
    }

    pub fn close_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 {
            return; // Don't close the last tab
        }
        self.tabs.remove(index);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.prev_image_layout.clear();
    }

    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            self.prev_image_layout.clear();
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
            self.prev_image_layout.clear();
        }
    }

    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        let mut needs_redraw = true;
        loop {
            if needs_redraw {
                terminal.draw(|frame| self.render(frame))?;
                needs_redraw = false;
            }

            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key_event) => {
                        self.handle_key_event(key_event)?;
                        needs_redraw = true;
                    }
                    Event::Resize(_, _) => {
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: crossterm::event::KeyEvent) -> std::io::Result<()> {
        match self.focus {
            FocusZone::Finder => self.handle_finder_key(key_event),
            FocusZone::Sidebar => self.handle_sidebar_key(key_event),
            FocusZone::Editor => self.handle_editor_key(key_event),
        }
    }

    fn handle_editor_key(
        &mut self,
        key_event: crossterm::event::KeyEvent,
    ) -> std::io::Result<()> {
        if let Some(cmd) = map_app_key_event(key_event) {
            match cmd {
                AppCommand::ToggleSidebar => {
                    self.sidebar_visible = !self.sidebar_visible;
                    if self.sidebar_visible {
                        self.focus = FocusZone::Sidebar;
                    }
                }
                AppCommand::OpenFinder => {
                    let cwd =
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                    self.finder = Some(FinderState::new(&cwd));
                    self.focus = FocusZone::Finder;
                }
                AppCommand::NextTab => self.next_tab(),
                AppCommand::PrevTab => self.prev_tab(),
                AppCommand::CloseTab => self.close_tab(self.active_tab),
                AppCommand::Quit => self.should_quit = true,
                AppCommand::Save => self.active_editor_mut().save()?,
                AppCommand::Editor(editor_cmd) => {
                    self.handle_editor_command(editor_cmd)?;
                }
            }
        }
        Ok(())
    }

    fn handle_sidebar_key(
        &mut self,
        key_event: crossterm::event::KeyEvent,
    ) -> std::io::Result<()> {
        if key_event.kind != KeyEventKind::Press {
            return Ok(());
        }

        let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);

        match (key_event.code, ctrl) {
            (KeyCode::Esc, _) => {
                self.focus = FocusZone::Editor;
            }
            (KeyCode::Char('b'), true) => {
                self.sidebar_visible = false;
                self.focus = FocusZone::Editor;
            }
            (KeyCode::Char('q'), true) => {
                self.should_quit = true;
            }
            (KeyCode::Char('p'), true) => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                self.finder = Some(FinderState::new(&cwd));
                self.focus = FocusZone::Finder;
            }
            (KeyCode::Up, _) => {
                if let Some(ref mut sidebar) = self.sidebar {
                    sidebar.file_tree.move_up();
                }
            }
            (KeyCode::Down, _) => {
                if let Some(ref mut sidebar) = self.sidebar {
                    sidebar.file_tree.move_down();
                }
            }
            (KeyCode::Left, _) => {
                // Collapse directory
                if let Some(ref mut sidebar) = self.sidebar {
                    if sidebar.file_tree.selected_kind()
                        == Some(crate::filetree::NodeKind::Directory)
                    {
                        // If expanded, collapse. If collapsed, move to parent (just move up for now).
                        let entry = &sidebar.file_tree.flat_view
                            [sidebar.file_tree.selected_index];
                        if entry.expanded {
                            sidebar.file_tree.toggle_expand();
                        } else {
                            sidebar.file_tree.move_up();
                        }
                    } else {
                        sidebar.file_tree.move_up();
                    }
                }
            }
            (KeyCode::Right, _) => {
                // Expand directory
                if let Some(ref mut sidebar) = self.sidebar {
                    if sidebar.file_tree.selected_kind()
                        == Some(crate::filetree::NodeKind::Directory)
                    {
                        let entry = &sidebar.file_tree.flat_view
                            [sidebar.file_tree.selected_index];
                        if !entry.expanded {
                            sidebar.file_tree.toggle_expand();
                        } else {
                            sidebar.file_tree.move_down();
                        }
                    }
                }
            }
            (KeyCode::Enter, _) => {
                if let Some(ref sidebar) = self.sidebar {
                    let kind = sidebar.file_tree.selected_kind();
                    let path = sidebar.file_tree.selected_path().map(|p| p.to_path_buf());
                    match (kind, path) {
                        (Some(crate::filetree::NodeKind::File), Some(path)) => {
                            self.open_file_in_tab(&path)?;
                            self.focus = FocusZone::Editor;
                        }
                        (Some(crate::filetree::NodeKind::Directory), _) => {
                            if let Some(ref mut sidebar) = self.sidebar {
                                sidebar.file_tree.toggle_expand();
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_finder_key(
        &mut self,
        key_event: crossterm::event::KeyEvent,
    ) -> std::io::Result<()> {
        if key_event.kind != KeyEventKind::Press {
            return Ok(());
        }

        let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);

        match (key_event.code, ctrl) {
            (KeyCode::Esc, _) | (KeyCode::Char('p'), true) => {
                self.finder = None;
                self.focus = FocusZone::Editor;
            }
            (KeyCode::Char('q'), true) => {
                self.should_quit = true;
            }
            (KeyCode::Enter, _) => {
                let path = self
                    .finder
                    .as_ref()
                    .and_then(|f| f.selected_path().map(|p| p.to_path_buf()));
                if let Some(path) = path {
                    self.open_file_in_tab(&path)?;
                }
                self.finder = None;
                self.focus = FocusZone::Editor;
            }
            (KeyCode::Up, _) => {
                if let Some(ref mut finder) = self.finder {
                    finder.move_up();
                }
            }
            (KeyCode::Down, _) => {
                if let Some(ref mut finder) = self.finder {
                    finder.move_down();
                }
            }
            (KeyCode::Backspace, _) => {
                if let Some(ref mut finder) = self.finder {
                    finder.delete_backward();
                }
            }
            (KeyCode::Char(c), false) => {
                if let Some(ref mut finder) = self.finder {
                    finder.insert_char(c);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_editor_command(&mut self, cmd: EditorCommand) -> std::io::Result<()> {
        let editor = self.active_editor_mut();
        match cmd {
            EditorCommand::Quit => self.should_quit = true,
            EditorCommand::Save => editor.save()?,
            EditorCommand::InsertChar(c) => editor.insert_char(c),
            EditorCommand::InsertNewline => editor.insert_newline(),
            EditorCommand::DeleteForward => editor.delete_forward(),
            EditorCommand::DeleteBackward => editor.delete_backward(),
            EditorCommand::MoveLeft { shift } => editor.move_left(shift),
            EditorCommand::MoveRight { shift } => editor.move_right(shift),
            EditorCommand::MoveUp { shift } => editor.move_up(shift),
            EditorCommand::MoveDown { shift } => editor.move_down(shift),
            EditorCommand::MoveHome { shift } => editor.move_home(shift),
            EditorCommand::MoveEnd { shift } => editor.move_end(shift),
            EditorCommand::MoveWordLeft { shift } => editor.move_word_left(shift),
            EditorCommand::MoveWordRight { shift } => editor.move_word_right(shift),
            EditorCommand::SelectAll => editor.select_all(),
            EditorCommand::Copy => {
                self.tabs[self.active_tab].editor.copy(&mut self.clipboard);
            }
            EditorCommand::Cut => {
                self.tabs[self.active_tab].editor.cut(&mut self.clipboard);
            }
            EditorCommand::Paste => {
                self.tabs[self.active_tab].editor.paste(&mut self.clipboard);
            }
            EditorCommand::Undo => editor.undo(),
            EditorCommand::Redo => editor.redo(),
        }
        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        let full_area = frame.area();

        // Horizontal split: sidebar | main
        let (sidebar_area, main_area) = if self.sidebar_visible {
            let h_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(self.sidebar_width),
                    Constraint::Fill(1),
                ])
                .split(full_area);
            (Some(h_chunks[0]), h_chunks[1])
        } else {
            (None, full_area)
        };

        // Vertical split: [tab_bar] + editor + status_bar
        let show_tabs = self.tabs.len() > 1;
        let mut v_constraints = Vec::new();
        if show_tabs {
            v_constraints.push(Constraint::Length(1));
        }
        v_constraints.push(Constraint::Fill(1));
        v_constraints.push(Constraint::Length(1));

        let v_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(v_constraints)
            .split(main_area);

        let mut idx = 0;
        let tab_area = if show_tabs {
            let a = v_chunks[idx];
            idx += 1;
            Some(a)
        } else {
            None
        };
        let editor_area = v_chunks[idx];
        idx += 1;
        let status_area = v_chunks[idx];

        // Render sidebar
        if let Some(area) = sidebar_area {
            if let Some(ref mut sidebar) = self.sidebar {
                sidebar.adjust_scroll(area.height as usize);

                // Collect dirty file paths for indicators
                let dirty_paths: Vec<Option<PathBuf>> = self
                    .tabs
                    .iter()
                    .filter(|t| t.editor.dirty)
                    .map(|t| t.editor.file_path.clone())
                    .collect();

                let widget = SidebarWidget {
                    state: sidebar,
                    focused: self.focus == FocusZone::Sidebar,
                    dirty_paths: &dirty_paths,
                };
                frame.render_widget(widget, area);
            }
        }

        // Render tab bar
        if let Some(area) = tab_area {
            render_tab_bar(&self.tabs, self.active_tab, area, frame.buffer_mut());
        }

        // Render editor content
        let editor = &self.tabs[self.active_tab].editor;
        let viewport_height = editor_area.height as usize;
        let viewport_width = editor_area.width as usize;

        let cursor_line = editor.cursor.line;
        let total_lines = editor.buffer.len_lines();
        let prefetch_start = cursor_line.saturating_sub(viewport_height + 10);
        let prefetch_end = (cursor_line + viewport_height + 10).min(total_lines);
        let cursor_byte = editor.cursor_byte_offset();
        let all_render_lines = compute_render_lines_with_width(
            &editor.markdown,
            &editor.buffer,
            cursor_byte,
            prefetch_start,
            prefetch_end,
            viewport_width,
            self.header_image.as_mut(),
        );

        // Adjust scroll using wrap-aware line heights
        self.tabs[self.active_tab]
            .editor
            .adjust_scroll_wrapped(viewport_height, |line_idx| {
                if line_idx >= prefetch_start && line_idx < prefetch_end {
                    all_render_lines[line_idx - prefetch_start].screen_rows(viewport_width)
                } else {
                    1
                }
            });

        let editor = &self.tabs[self.active_tab].editor;

        // Determine visible range
        let visible_start = editor.scroll_offset;
        let mut visible_end = visible_start;
        let mut rows_filled = 0;
        while visible_end < total_lines && rows_filled < viewport_height + 5 {
            if visible_end >= prefetch_start && visible_end < prefetch_end {
                rows_filled +=
                    all_render_lines[visible_end - prefetch_start].screen_rows(viewport_width);
            } else {
                rows_filled += 1;
            }
            visible_end += 1;
        }

        let slice_start = visible_start.max(prefetch_start) - prefetch_start;
        let slice_end = visible_end.min(prefetch_end) - prefetch_start;
        let render_lines = &all_render_lines[slice_start..slice_end];

        // Compute cursor screen row and image layout
        let cursor_line_index = cursor_line.saturating_sub(visible_start);

        let mut cursor_screen_row: usize = 0;
        let mut screen_row: usize = 0;
        let mut current_image_layout = Vec::new();
        for (i, rl) in render_lines.iter().enumerate() {
            let rows = rl.screen_rows(viewport_width);

            if let Some(ref img) = rl.image_header {
                current_image_layout.push(ImageLayoutEntry {
                    screen_row: screen_row as u16,
                    rows: img.display_rows,
                    text: img.text.clone(),
                    tier: img.tier,
                });
            }

            if i < cursor_line_index {
                cursor_screen_row += rows;
            }
            screen_row += rows;
        }

        let images_changed = current_image_layout != self.prev_image_layout;
        self.prev_image_layout = current_image_layout;

        let widget = EditorWidget {
            editor,
            render_lines,
            image_support: if images_changed {
                self.header_image.as_mut()
            } else {
                None
            },
            skip_unchanged_images: !images_changed,
        };
        frame.render_widget(widget, editor_area);

        // Render status bar
        render_status_bar(
            &self.tabs[self.active_tab].editor,
            status_area,
            frame.buffer_mut(),
        );

        // Set cursor position (only when editor is focused)
        if self.focus == FocusZone::Editor {
            let editor = &self.tabs[self.active_tab].editor;
            let cursor_display_col = if let Some(rl) = render_lines.get(cursor_line_index) {
                rl.position_map.raw_to_display_col(editor.cursor.col)
            } else {
                editor.cursor.display_col(&editor.buffer)
            };

            let (cursor_wrap_row, cursor_x) = if viewport_width > 0 {
                (
                    cursor_display_col / viewport_width,
                    cursor_display_col % viewport_width,
                )
            } else {
                (0, cursor_display_col)
            };
            cursor_screen_row += cursor_wrap_row;

            let clamped_y =
                cursor_screen_row.min(editor_area.height.saturating_sub(1) as usize);
            frame.set_cursor_position(Position {
                x: editor_area.x + cursor_x as u16,
                y: editor_area.y + clamped_y as u16,
            });
        } else if self.focus == FocusZone::Finder {
            // Place cursor in finder input
            if let Some(ref finder) = self.finder {
                let popup_area = frame.area();
                let width =
                    (popup_area.width * 60 / 100).max(30).min(popup_area.width);
                let height = (popup_area.height * 50 / 100)
                    .max(10)
                    .min(popup_area.height);
                let x = popup_area.x + (popup_area.width.saturating_sub(width)) / 2;
                let y = popup_area.y + (popup_area.height.saturating_sub(height)) / 3;
                // Cursor after " > " + query
                let cursor_x = x + 1 + 3 + finder.query.len() as u16;
                let cursor_y = y + 1;
                frame.set_cursor_position(Position {
                    x: cursor_x.min(x + width - 2),
                    y: cursor_y,
                });
            }
        }

        // Render finder overlay (last, on top of everything)
        if let Some(ref finder) = self.finder {
            let widget = FinderWidget { state: finder };
            frame.render_widget(widget, full_area);
        }
    }
}
