use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Highlighter.
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme_name: String,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            theme_name: "base16-ocean.dark".to_string(),
        }
    }
}

impl Highlighter {
    /// Highlight a code block, returning owned styled ratatui Lines.
    pub fn highlight(&self, code: &str, lang: &str) -> Vec<Line<'static>> {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = self
            .theme_set
            .themes
            .get(&self.theme_name)
            .unwrap_or_else(|| self.theme_set.themes.values().next().unwrap());

        let mut h = HighlightLines::new(syntax, theme);
        let mut lines = Vec::new();

        for line in LinesWithEndings::from(code) {
            let Ok(ranges) = h.highlight_line(line, &self.syntax_set) else {
                lines.push(Line::raw(line.trim_end_matches('\n').to_string()));
                continue;
            };
            let spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(style, text)| {
                    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                    let mut rs = Style::default().fg(fg);
                    if style.font_style.contains(FontStyle::BOLD) {
                        rs = rs.add_modifier(Modifier::BOLD);
                    }
                    if style.font_style.contains(FontStyle::ITALIC) {
                        rs = rs.add_modifier(Modifier::ITALIC);
                    }
                    Span::styled(text.trim_end_matches('\n').to_string(), rs)
                })
                .collect();
            lines.push(Line::from(spans));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_code() {
        let h = Highlighter::default();
        let lines = h.highlight("fn main() {}\n", "rs");
        assert!(!lines.is_empty());
    }

    #[test]
    fn falls_back_for_unknown_lang() {
        let h = Highlighter::default();
        let lines = h.highlight("hello world\n", "zzz_unknown");
        assert_eq!(lines.len(), 1);
    }
}
