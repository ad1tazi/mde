use ratatui::style::{Color, Modifier, Style};
use tree_sitter_md::MarkdownTree;
use unicode_width::UnicodeWidthStr;

use crate::editor::buffer::Buffer;
use crate::markdown::reveal::RevealSet;
use crate::render::plan::RenderSpan;

const BORDER_COLOR: Color = Color::DarkGray;

/// Render a single line that is part of a pipe_table.
///
/// When revealed (cursor inside table): show raw text.
/// When concealed: properly formatted box-drawn table with aligned columns.
pub fn render_pipe_table_line(
    buffer: &Buffer,
    tree: &MarkdownTree,
    reveal_set: &RevealSet,
    line_start_byte: usize,
    line_end_byte: usize,
    spans: &mut Vec<RenderSpan>,
) {
    // Find the pipe_table node containing this line
    let mut cursor = tree.walk();
    let mut table_start = 0;
    let mut table_end = 0;
    let mut found = false;

    loop {
        let node = cursor.node();
        if node.kind() == "pipe_table" {
            table_start = node.start_byte();
            table_end = node.end_byte();
            found = true;
            break;
        }
        if cursor.goto_first_child_for_byte(line_start_byte).is_none() {
            break;
        }
    }

    if !found || reveal_set.is_revealed(&(table_start..table_end)) {
        // Show raw text
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

    // Analyze the full table to get column widths
    let table_info = analyze_table(&mut cursor, buffer);

    // Find what row type this line belongs to and extract its cells
    let mut cursor2 = tree.walk();
    let row_info = identify_row(&mut cursor2, buffer, line_start_byte, line_end_byte);

    let raw_cc = buffer.char_count_for_byte_range(line_start_byte, line_end_byte);
    let border_style = Style::default().fg(BORDER_COLOR);

    let display_text = match row_info.kind {
        RowKind::Header => {
            format_content_row(&row_info.cells, &table_info.col_widths)
        }
        RowKind::Delimiter => {
            format_delimiter_row(&table_info.col_widths)
        }
        RowKind::Data => {
            format_content_row(&row_info.cells, &table_info.col_widths)
        }
    };

    let style = match row_info.kind {
        RowKind::Header => border_style.add_modifier(Modifier::BOLD),
        RowKind::Delimiter => border_style,
        RowKind::Data => border_style,
    };

    spans.push(RenderSpan {
        display_text,
        style,
        raw_byte_range: line_start_byte..line_end_byte,
        raw_char_count: raw_cc,
        is_decoration: false,
    });
}

// --- Table analysis ---

struct TableInfo {
    col_widths: Vec<usize>,
}

#[derive(Debug)]
enum RowKind {
    Header,
    Delimiter,
    Data,
}

struct RowInfo {
    kind: RowKind,
    cells: Vec<String>,
}

/// Walk the pipe_table node to compute max column widths.
/// `cursor` must be positioned at the `pipe_table` node.
fn analyze_table(cursor: &mut tree_sitter_md::MarkdownCursor<'_>, buffer: &Buffer) -> TableInfo {
    let mut col_widths: Vec<usize> = Vec::new();

    if !cursor.goto_first_child() {
        return TableInfo { col_widths };
    }

    loop {
        let row_node = cursor.node();
        let row_kind = row_node.kind();

        // Skip the delimiter row for width calculation
        if row_kind == "pipe_table_header" || row_kind == "pipe_table_row" {
            let cells = extract_cells(cursor, buffer);
            // Update column widths
            for (i, cell) in cells.iter().enumerate() {
                let width = UnicodeWidthStr::width(cell.as_str());
                if i >= col_widths.len() {
                    col_widths.push(width);
                } else if width > col_widths[i] {
                    col_widths[i] = width;
                }
            }
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    // Ensure minimum column width of 3 for aesthetics
    for w in col_widths.iter_mut() {
        if *w < 3 {
            *w = 3;
        }
    }

    TableInfo { col_widths }
}

/// Extract trimmed cell contents from a row node.
/// `cursor` must be positioned at a pipe_table_header or pipe_table_row.
fn extract_cells(
    cursor: &mut tree_sitter_md::MarkdownCursor<'_>,
    buffer: &Buffer,
) -> Vec<String> {
    let mut cells = Vec::new();

    if !cursor.goto_first_child() {
        return cells;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "pipe_table_cell" {
            let text = buffer.text_for_byte_range(child.start_byte(), child.end_byte());
            cells.push(text.trim().to_string());
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    cells
}

/// Identify which row this line belongs to and extract its cells.
fn identify_row(
    cursor: &mut tree_sitter_md::MarkdownCursor<'_>,
    buffer: &Buffer,
    line_start_byte: usize,
    line_end_byte: usize,
) -> RowInfo {
    // Navigate to the row containing this line
    loop {
        let node = cursor.node();
        match node.kind() {
            "pipe_table_header" => {
                if node.start_byte() <= line_start_byte && node.end_byte() >= line_end_byte {
                    let cells = extract_cells(cursor, buffer);
                    return RowInfo {
                        kind: RowKind::Header,
                        cells,
                    };
                }
            }
            "pipe_table_delimiter_row" => {
                if node.start_byte() <= line_start_byte && node.end_byte() >= line_end_byte {
                    return RowInfo {
                        kind: RowKind::Delimiter,
                        cells: vec![],
                    };
                }
            }
            "pipe_table_row" => {
                if node.start_byte() <= line_start_byte && node.end_byte() >= line_end_byte {
                    let cells = extract_cells(cursor, buffer);
                    return RowInfo {
                        kind: RowKind::Data,
                        cells,
                    };
                }
            }
            _ => {}
        }
        if cursor.goto_first_child_for_byte(line_start_byte).is_none() {
            break;
        }
    }

    // Fallback
    RowInfo {
        kind: RowKind::Data,
        cells: vec![],
    }
}

// --- Formatting ---

/// Format a content row (header or data): `│ Name  │ Age │`
fn format_content_row(cells: &[String], col_widths: &[usize]) -> String {
    let mut result = String::from("│");
    for (i, width) in col_widths.iter().enumerate() {
        let cell_text = cells.get(i).map(|s| s.as_str()).unwrap_or("");
        let cell_width = UnicodeWidthStr::width(cell_text);
        let padding = width.saturating_sub(cell_width);
        result.push(' ');
        result.push_str(cell_text);
        for _ in 0..padding {
            result.push(' ');
        }
        result.push(' ');
        result.push('│');
    }
    result
}

/// Format a delimiter row: `├───────┼─────┤`
fn format_delimiter_row(col_widths: &[usize]) -> String {
    let mut result = String::from("├");
    for (i, width) in col_widths.iter().enumerate() {
        // +2 for the spaces around the cell content
        for _ in 0..(width + 2) {
            result.push('─');
        }
        if i < col_widths.len() - 1 {
            result.push('┼');
        }
    }
    result.push('┤');
    result
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
    fn table_header_with_box_borders() {
        let text = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines(&state, &buffer, 999, 0, 3);
        let header = display_text(&lines[0]);
        assert!(header.contains('│'), "Expected │ in header '{}'", header);
        assert!(!header.contains('|'), "Should not contain raw | in '{}'", header);
        assert!(header.contains("Name"), "Expected Name in header '{}'", header);
        assert!(header.contains("Age"), "Expected Age in header '{}'", header);
    }

    #[test]
    fn table_delimiter_row_renders_as_separator() {
        let text = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines(&state, &buffer, 999, 0, 3);
        let delim = display_text(&lines[1]);
        assert!(delim.starts_with('├'), "Expected ├ at start: '{}'", delim);
        assert!(delim.ends_with('┤'), "Expected ┤ at end: '{}'", delim);
        assert!(delim.contains('┼'), "Expected ┼ in delimiter: '{}'", delim);
        assert!(delim.contains('─'), "Expected ─ in delimiter: '{}'", delim);
    }

    #[test]
    fn table_data_row_aligned() {
        let text = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines(&state, &buffer, 999, 0, 3);
        let row = display_text(&lines[2]);
        assert!(row.contains('│'), "Expected │ in row '{}'", row);
        assert!(row.contains("Alice"), "Expected Alice in row '{}'", row);
    }

    #[test]
    fn table_columns_aligned_across_rows() {
        let text = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines(&state, &buffer, 999, 0, 4);

        let header = display_text(&lines[0]);
        let delim = display_text(&lines[1]);
        let row1 = display_text(&lines[2]);
        let row2 = display_text(&lines[3]);

        // All rows should have the same total width
        assert_eq!(
            header.chars().count(),
            delim.chars().count(),
            "Header and delimiter width mismatch:\n  header: '{}'\n  delim:  '{}'",
            header,
            delim
        );
        assert_eq!(
            header.chars().count(),
            row1.chars().count(),
            "Header and row1 width mismatch:\n  header: '{}'\n  row1:   '{}'",
            header,
            row1
        );
        assert_eq!(
            header.chars().count(),
            row2.chars().count(),
            "Header and row2 width mismatch:\n  header: '{}'\n  row2:   '{}'",
            header,
            row2
        );
    }

    #[test]
    fn table_revealed_shows_raw() {
        let text = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n";
        let (buffer, state) = make_state(text);
        // Cursor inside table
        let lines = compute_render_lines(&state, &buffer, 5, 0, 3);
        let header = display_text(&lines[0]);
        assert!(
            header.contains('|'),
            "Expected raw | when revealed: '{}'",
            header
        );
    }

    #[test]
    fn table_proper_formatting() {
        // Test that formatting is visually correct
        let text = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |\n";
        let (buffer, state) = make_state(text);
        let lines = compute_render_lines(&state, &buffer, 999, 0, 4);

        let header = display_text(&lines[0]);
        let delim = display_text(&lines[1]);
        let row1 = display_text(&lines[2]);
        let row2 = display_text(&lines[3]);

        // "Alice" is the widest in col 1, so col 1 width = 5
        // "Age" and "30" and "25" — "Age" is widest at 3
        // Expected:
        // │ Name  │ Age │
        // ├───────┼─────┤
        // │ Alice │ 30  │
        // │ Bob   │ 25  │
        assert_eq!(header, "│ Name  │ Age │");
        assert_eq!(delim, "├───────┼─────┤");
        assert_eq!(row1, "│ Alice │ 30  │");
        assert_eq!(row2, "│ Bob   │ 25  │");
    }
}
