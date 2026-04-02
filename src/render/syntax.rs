use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use syntect::highlighting::{FontStyle, Highlighter, HighlightIterator, HighlightState, Theme, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        let ts = ThemeSet::load_defaults();
        ts.themes.into_iter()
            .find(|(name, _)| name == "base16-eighties.dark")
            .map(|(_, t)| t)
            .unwrap()
    })
}

/// A single highlighted token with its ratatui style.
pub struct StyledToken {
    pub text: String,
    pub style: Style,
}

/// Convert a syntect style to a ratatui style.
/// Only sets foreground and modifiers; background is controlled by the caller.
fn syntect_to_ratatui(style: syntect::highlighting::Style) -> Style {
    let fg = style.foreground;
    let mut s = Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b));
    if style.font_style.contains(FontStyle::BOLD) {
        s = s.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        s = s.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        s = s.add_modifier(Modifier::UNDERLINED);
    }
    s
}

/// Highlight code lines for a given language.
///
/// Takes all content lines from the start of the code block through the lines
/// we need. Each line must be newline-terminated (for `load_defaults_newlines`).
///
/// Returns `None` if the language is unrecognized or highlighting fails.
/// On success, returns one `Vec<StyledToken>` per input line.
pub fn highlight_code_lines(lang: &str, lines: &[&str]) -> Option<Vec<Vec<StyledToken>>> {
    let ss = syntax_set();
    let syntax = ss.find_syntax_by_token(lang)?;
    let t = theme();
    let highlighter = Highlighter::new(t);
    let mut parse_state = ParseState::new(syntax);
    let mut highlight_state = HighlightState::new(&highlighter, ScopeStack::new());

    let mut result = Vec::with_capacity(lines.len());
    for line in lines {
        let ops = parse_state.parse_line(line, ss).ok()?;
        let iter = HighlightIterator::new(&mut highlight_state, &ops, line, &highlighter);
        let tokens: Vec<StyledToken> = iter
            .map(|(style, text)| StyledToken {
                text: text.to_string(),
                style: syntect_to_ratatui(style),
            })
            .collect();
        result.push(tokens);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_rust_produces_tokens() {
        let lines = &["fn main() {\n", "    println!(\"hello\");\n", "}\n"];
        let result = highlight_code_lines("rust", lines);
        assert!(result.is_some());
        let highlighted = result.unwrap();
        assert_eq!(highlighted.len(), 3);
        // Each line should have at least one token
        for line_tokens in &highlighted {
            assert!(!line_tokens.is_empty());
        }
        // Concatenated text of first line should match input
        let first_line_text: String = highlighted[0].iter().map(|t| t.text.as_str()).collect();
        assert_eq!(first_line_text, "fn main() {\n");
    }

    #[test]
    fn unknown_language_returns_none() {
        let lines = &["some code\n"];
        let result = highlight_code_lines("not_a_real_language_xyz", lines);
        assert!(result.is_none());
    }

    #[test]
    fn empty_lang_returns_none() {
        let lines = &["some code\n"];
        let result = highlight_code_lines("", lines);
        assert!(result.is_none());
    }

    #[test]
    fn tokens_cover_full_line() {
        let lines = &["let x = 42;\n"];
        let result = highlight_code_lines("rust", lines).unwrap();
        let total: String = result[0].iter().map(|t| t.text.as_str()).collect();
        assert_eq!(total, "let x = 42;\n");
    }
}
