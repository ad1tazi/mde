use std::ops::Range;

use ratatui::style::Style;
use unicode_width::UnicodeWidthChar;

/// A single segment of rendered output for a line.
#[derive(Debug, Clone)]
pub struct RenderSpan {
    /// The text to display on screen (may differ from raw buffer text).
    pub display_text: String,
    /// Style to apply to this span.
    pub style: Style,
    /// The raw byte range in the buffer this span corresponds to.
    pub raw_byte_range: Range<usize>,
    /// Number of raw chars this span covers in the buffer.
    /// Used for position mapping (bytes can be multi-byte, so we need char count).
    pub raw_char_count: usize,
    /// If true, this span is a synthetic decoration not from the buffer
    /// (e.g., blockquote left border, code block frame).
    pub is_decoration: bool,
}

/// Bidirectional mapping between raw char column and rendered display column.
#[derive(Debug, Clone, Default)]
pub struct PositionMap {
    /// For each raw char column (index 0..=raw_char_count), the display column.
    /// Length = raw line char count + 1 (to map cursor-at-end-of-line).
    pub raw_to_display: Vec<usize>,
    /// For each display column (index 0..=display_width), the raw char column.
    /// Length = display width + 1.
    pub display_to_raw: Vec<usize>,
}

/// Metadata for a heading that should be rendered as an image.
#[derive(Debug, Clone)]
pub struct ImageHeader {
    /// Heading tier (1-6).
    pub tier: u8,
    /// Plain text content of the heading (for cache keying and rendering).
    pub text: String,
    /// How many terminal rows this image occupies.
    pub display_rows: u16,
}

/// A fully rendered line ready for display.
#[derive(Debug, Clone)]
pub struct RenderLine {
    /// The buffer line index this render line corresponds to.
    pub line_idx: usize,
    /// Ordered spans that make up this line's visible content.
    pub spans: Vec<RenderSpan>,
    /// Position mapping between raw char columns and display columns.
    pub position_map: PositionMap,
    /// If set, this line is a concealed heading that should be rendered as an image.
    /// The spans are still populated as a fallback.
    pub image_header: Option<ImageHeader>,
}

impl PositionMap {
    /// Build position map from a sequence of render spans.
    ///
    /// Walks through spans in order, tracking both the raw char column
    /// (buffer position) and display column (screen position).
    ///
    /// - For normal spans: each raw char maps 1:1 to display chars (accounting for unicode width)
    /// - For hidden text (display_text is empty, raw_char_count > 0): raw chars collapse to current display col
    /// - For replacement text (display_text differs from raw): replacement display maps to raw range start
    /// - For decorations (is_decoration=true): display cols map to current raw position (no raw chars consumed)
    pub fn build(spans: &[RenderSpan], raw_line_char_count: usize) -> Self {
        let mut raw_to_display = Vec::with_capacity(raw_line_char_count + 1);
        let mut display_to_raw = Vec::new();

        let mut raw_col: usize = 0;
        let mut display_col: usize = 0;

        for span in spans {
            if span.is_decoration {
                // Decoration: advances display but not raw
                let raw_at = raw_col;
                for ch in span.display_text.chars() {
                    let w = display_width(ch);
                    for _ in 0..w {
                        display_to_raw.push(raw_at);
                    }
                    display_col += w;
                }
                continue;
            }

            let display_text_empty = span.display_text.is_empty();
            let raw_chars = span.raw_char_count;

            if display_text_empty {
                // Hidden span: raw chars all map to current display col
                for _ in 0..raw_chars {
                    raw_to_display.push(display_col);
                    raw_col += 1;
                }
            } else {
                // Visible span (normal or replacement)
                let display_chars: Vec<char> = span.display_text.chars().collect();
                let display_char_count = display_chars.len();

                if raw_chars == display_char_count {
                    // 1:1 mapping (normal styled text)
                    for ch in &display_chars {
                        raw_to_display.push(display_col);
                        let w = display_width(*ch);
                        for _ in 0..w {
                            display_to_raw.push(raw_col);
                        }
                        display_col += w;
                        raw_col += 1;
                    }
                } else {
                    // Replacement: different char counts
                    let raw_start = raw_col;
                    let display_start = display_col;

                    // Map all raw chars to the display start
                    for _ in 0..raw_chars {
                        raw_to_display.push(display_start);
                        raw_col += 1;
                    }

                    // Map all display chars to the raw start
                    for ch in &display_chars {
                        let w = display_width(*ch);
                        for _ in 0..w {
                            display_to_raw.push(raw_start);
                        }
                        display_col += w;
                    }
                }
            }
        }

        // Sentinel entry for cursor-at-end-of-line
        raw_to_display.push(display_col);
        display_to_raw.push(raw_col);

        PositionMap {
            raw_to_display,
            display_to_raw,
        }
    }

    /// Convert a raw char column to a display column.
    /// Clamps to valid range.
    pub fn raw_to_display_col(&self, raw_col: usize) -> usize {
        if self.raw_to_display.is_empty() {
            return 0;
        }
        let idx = raw_col.min(self.raw_to_display.len() - 1);
        self.raw_to_display[idx]
    }

    /// Convert a display column to a raw char column.
    /// Clamps to valid range.
    pub fn display_to_raw_col(&self, display_col: usize) -> usize {
        if self.display_to_raw.is_empty() {
            return 0;
        }
        let idx = display_col.min(self.display_to_raw.len() - 1);
        self.display_to_raw[idx]
    }
}

impl RenderLine {
    /// Total display width of this rendered line.
    pub fn display_width(&self) -> usize {
        let mut w = 0;
        for span in &self.spans {
            for ch in span.display_text.chars() {
                w += display_width(ch);
            }
        }
        w
    }

    /// How many screen rows this line occupies at the given viewport width.
    /// Empty lines and lines that fit within the width occupy 1 row.
    /// Image headers use their own `display_rows`.
    pub fn screen_rows(&self, viewport_width: usize) -> usize {
        if viewport_width == 0 {
            return 1;
        }
        if let Some(ref img) = self.image_header {
            return img.display_rows as usize;
        }
        let w = self.display_width();
        if w == 0 {
            return 1;
        }
        (w + viewport_width - 1) / viewport_width
    }

    /// Given a display column and viewport width, return `(wrap_row, x_in_row)`.
    /// `wrap_row` is the 0-based row offset from the first screen row of this line.
    /// `x_in_row` is the column within that screen row.
    pub fn wrap_position(&self, display_col: usize, viewport_width: usize) -> (usize, usize) {
        if viewport_width == 0 {
            return (0, display_col);
        }
        (display_col / viewport_width, display_col % viewport_width)
    }
}

/// Display width of a character (unicode-aware, tab-aware).
fn display_width(ch: char) -> usize {
    if ch == '\t' {
        4 // simplified; real tab stops handled at render time
    } else {
        UnicodeWidthChar::width(ch).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(display: &str, raw_chars: usize, byte_start: usize, byte_end: usize) -> RenderSpan {
        RenderSpan {
            display_text: display.to_string(),
            style: Style::default(),
            raw_byte_range: byte_start..byte_end,
            raw_char_count: raw_chars,
            is_decoration: false,
        }
    }

    fn hidden(raw_chars: usize, byte_start: usize, byte_end: usize) -> RenderSpan {
        RenderSpan {
            display_text: String::new(),
            style: Style::default(),
            raw_byte_range: byte_start..byte_end,
            raw_char_count: raw_chars,
            is_decoration: false,
        }
    }

    fn decoration(display: &str) -> RenderSpan {
        RenderSpan {
            display_text: display.to_string(),
            style: Style::default(),
            raw_byte_range: 0..0,
            raw_char_count: 0,
            is_decoration: true,
        }
    }

    #[test]
    fn plain_text_identity_mapping() {
        // "hello" — no hidden markers, 1:1 mapping
        let spans = vec![span("hello", 5, 0, 5)];
        let map = PositionMap::build(&spans, 5);

        assert_eq!(map.raw_to_display, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(map.display_to_raw, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn hidden_markers_shift_mapping() {
        // Raw: "**bold**" (8 chars)
        // Rendered: "bold" (4 chars) — markers hidden
        // Spans: hidden(**) + visible(bold) + hidden(**)
        let spans = vec![
            hidden(2, 0, 2),         // "**"
            span("bold", 4, 2, 6),   // "bold"
            hidden(2, 6, 8),         // "**"
        ];
        let map = PositionMap::build(&spans, 8);

        // raw_to_display: ** maps to 0, bold maps to 0-3, ** maps to 4
        assert_eq!(map.raw_to_display, vec![0, 0, 0, 1, 2, 3, 4, 4, 4]);
        // display_to_raw: bold display cols map to raw cols 2-5
        assert_eq!(map.display_to_raw, vec![2, 3, 4, 5, 8]);
    }

    #[test]
    fn mixed_visible_and_hidden() {
        // Raw: "This is **bold** text" (21 chars)
        // Rendered: "This is bold text" (17 chars)
        let spans = vec![
            span("This is ", 8, 0, 8),
            hidden(2, 8, 10),          // "**"
            span("bold", 4, 10, 14),
            hidden(2, 14, 16),         // "**"
            span(" text", 5, 16, 21),
        ];
        let map = PositionMap::build(&spans, 21);

        // Verify key positions
        assert_eq!(map.raw_to_display_col(0), 0);   // 'T'
        assert_eq!(map.raw_to_display_col(7), 7);   // ' ' before **
        assert_eq!(map.raw_to_display_col(8), 8);   // first '*' → display 8 (hidden)
        assert_eq!(map.raw_to_display_col(9), 8);   // second '*' → display 8 (hidden)
        assert_eq!(map.raw_to_display_col(10), 8);  // 'b' → display 8
        assert_eq!(map.raw_to_display_col(13), 11); // 'd' → display 11
        assert_eq!(map.raw_to_display_col(14), 12); // first closing '*' → display 12
        assert_eq!(map.raw_to_display_col(15), 12); // second closing '*' → display 12
        assert_eq!(map.raw_to_display_col(16), 12); // ' ' after ** → display 12
        assert_eq!(map.raw_to_display_col(20), 16); // 't' → display 16
        assert_eq!(map.raw_to_display_col(21), 17); // end → display 17
    }

    #[test]
    fn replacement_text() {
        // Raw: "- item" (6 chars)
        // Rendered: "• item" (6 chars) — same length replacement
        // But consider "- " (2 raw chars) → "• " (2 display chars)
        let spans = vec![
            span("• ", 2, 0, 2),      // replacement: "- " → "• "
            span("item", 4, 2, 6),
        ];
        let map = PositionMap::build(&spans, 6);

        // Same-length replacement: 1:1 mapping still works
        assert_eq!(map.raw_to_display, vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn decoration_prefix() {
        // Blockquote: adds "│ " decoration before content
        // Raw: "> text" (6 chars), but > is replaced
        // Decoration "│ " + visible "text" for the content after "> "
        let spans = vec![
            decoration("│ "),                  // 2 display cols, 0 raw chars
            hidden(2, 0, 2),                   // "> " hidden
            span("text", 4, 2, 6),             // content
        ];
        let map = PositionMap::build(&spans, 6);

        // Decoration shifts display by 2, raw "> " is hidden
        assert_eq!(map.raw_to_display_col(0), 2); // '>' → display 2 (after decoration)
        assert_eq!(map.raw_to_display_col(1), 2); // ' ' → display 2 (hidden)
        assert_eq!(map.raw_to_display_col(2), 2); // 't' → display 2
        assert_eq!(map.raw_to_display_col(3), 3); // 'e' → display 3
        assert_eq!(map.raw_to_display_col(5), 5); // 't' → display 5

        // Display col 0,1 (decoration) → raw 0
        assert_eq!(map.display_to_raw_col(0), 0);
        assert_eq!(map.display_to_raw_col(1), 0);
        // Display col 2 (content 't') → raw 2
        assert_eq!(map.display_to_raw_col(2), 2);
    }

    #[test]
    fn empty_line() {
        let spans: Vec<RenderSpan> = vec![];
        let map = PositionMap::build(&spans, 0);
        assert_eq!(map.raw_to_display, vec![0]);
        assert_eq!(map.display_to_raw, vec![0]);
    }

    #[test]
    fn clamping_out_of_range() {
        let spans = vec![span("hi", 2, 0, 2)];
        let map = PositionMap::build(&spans, 2);

        // Out of range should clamp
        assert_eq!(map.raw_to_display_col(100), 2);
        assert_eq!(map.display_to_raw_col(100), 2);
    }

    // --- screen_rows / wrap_position tests ---

    fn make_render_line(display: &str) -> RenderLine {
        let spans = if display.is_empty() {
            vec![]
        } else {
            vec![span(display, display.len(), 0, display.len())]
        };
        let position_map = PositionMap::build(&spans, if display.is_empty() { 0 } else { display.len() });
        RenderLine {
            line_idx: 0,
            spans,
            position_map,
            image_header: None,
        }
    }

    #[test]
    fn screen_rows_empty_line() {
        let rl = make_render_line("");
        assert_eq!(rl.screen_rows(80), 1);
    }

    #[test]
    fn screen_rows_short_line() {
        let rl = make_render_line("hello");
        assert_eq!(rl.screen_rows(80), 1);
    }

    #[test]
    fn screen_rows_exact_fit() {
        let rl = make_render_line("12345");
        assert_eq!(rl.screen_rows(5), 1);
    }

    #[test]
    fn screen_rows_one_char_overflow() {
        let rl = make_render_line("123456");
        assert_eq!(rl.screen_rows(5), 2);
    }

    #[test]
    fn screen_rows_two_and_half_widths() {
        // 13 chars at width 5 = ceil(13/5) = 3 rows
        let rl = make_render_line("1234567890abc");
        assert_eq!(rl.screen_rows(5), 3);
    }

    #[test]
    fn screen_rows_zero_viewport_width() {
        let rl = make_render_line("hello");
        assert_eq!(rl.screen_rows(0), 1);
    }

    #[test]
    fn wrap_position_no_wrap() {
        let rl = make_render_line("hello");
        assert_eq!(rl.wrap_position(3, 80), (0, 3));
    }

    #[test]
    fn wrap_position_on_second_row() {
        let rl = make_render_line("hello world!!");
        // display_col 7 at viewport_width 5 → row 1, col 2
        assert_eq!(rl.wrap_position(7, 5), (1, 2));
    }

    #[test]
    fn wrap_position_at_wrap_boundary() {
        let rl = make_render_line("1234567890");
        // display_col 5 at viewport_width 5 → row 1, col 0
        assert_eq!(rl.wrap_position(5, 5), (1, 0));
    }

    #[test]
    fn wrap_position_zero_viewport() {
        let rl = make_render_line("hello");
        assert_eq!(rl.wrap_position(3, 0), (0, 3));
    }
}
