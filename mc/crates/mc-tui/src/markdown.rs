use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::highlight::Highlighter;

/// Render markdown text into owned styled ratatui Lines.
#[allow(clippy::similar_names)]
pub fn render_markdown(text: &str, highlighter: &Highlighter) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();

    for raw_line in text.lines() {
        if let Some(rest) = raw_line.strip_prefix("```") {
            if in_code_block {
                // End code block — highlight accumulated code
                let highlighted = highlighter.highlight(&code_buf, &code_lang);
                lines.push(Line::styled(
                    format!("  ┌─ {code_lang} ─"),
                    Style::default().fg(Color::DarkGray),
                ));
                for hl in highlighted {
                    let mut prefixed: Vec<Span<'static>> =
                        vec![Span::styled("  │ ", Style::default().fg(Color::DarkGray))];
                    prefixed.extend(hl.spans);
                    lines.push(Line::from(prefixed));
                }
                lines.push(Line::styled("  └─", Style::default().fg(Color::DarkGray)));
                code_buf.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                code_lang = rest.trim().to_string();
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code_buf.push_str(raw_line);
            code_buf.push('\n');
            continue;
        }

        // Headers
        if let Some(h) = raw_line.strip_prefix("### ") {
            lines.push(Line::styled(
                format!("   {h}"),
                Style::default().fg(Color::Cyan),
            ));
        } else if let Some(h) = raw_line.strip_prefix("## ") {
            lines.push(Line::styled(
                format!("  {h}"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else if let Some(h) = raw_line.strip_prefix("# ") {
            lines.push(Line::styled(
                h.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        }
        // Unordered list
        else if raw_line.starts_with("- ") || raw_line.starts_with("* ") {
            let item = &raw_line[2..];
            let mut spans: Vec<Span<'static>> =
                vec![Span::styled("  • ", Style::default().fg(Color::Yellow))];
            spans.extend(parse_inline(item));
            lines.push(Line::from(spans));
        }
        // Blockquote
        else if let Some(quote) = raw_line.strip_prefix("> ") {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    quote.to_string(),
                    Style::default()
                        .fg(Color::Gray)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }
        // Horizontal rule
        else if raw_line.trim() == "---" || raw_line.trim() == "***" {
            lines.push(Line::styled(
                "─".repeat(40),
                Style::default().fg(Color::DarkGray),
            ));
        }
        // Normal paragraph
        else {
            lines.push(Line::from(parse_inline(raw_line)));
        }
    }

    // Unclosed code block
    if in_code_block && !code_buf.is_empty() {
        for hl in highlighter.highlight(&code_buf, &code_lang) {
            lines.push(hl);
        }
    }

    lines
}

/// Parse inline markdown: **bold**, `code`, plain text. Returns owned Spans.
fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the earliest special marker
        let backtick = remaining.find('`');
        let bold = remaining.find("**");

        match (backtick, bold) {
            (Some(bt), _) if bold.is_none_or(|b| bt <= b) => {
                if bt > 0 {
                    spans.push(Span::raw(remaining[..bt].to_string()));
                }
                let after = &remaining[bt + 1..];
                if let Some(end) = after.find('`') {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default().fg(Color::Green),
                    ));
                    remaining = &after[end + 1..];
                } else {
                    spans.push(Span::raw(remaining[bt..].to_string()));
                    return spans;
                }
            }
            (_, Some(b)) => {
                if b > 0 {
                    spans.push(Span::raw(remaining[..b].to_string()));
                }
                let after = &remaining[b + 2..];
                if let Some(end) = after.find("**") {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                    remaining = &after[end + 2..];
                } else {
                    spans.push(Span::raw(remaining[b..].to_string()));
                    return spans;
                }
            }
            _ => {
                spans.push(Span::raw(remaining.to_string()));
                return spans;
            }
        }
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_headers() {
        let h = Highlighter::default();
        let lines = render_markdown("# Title\n## Sub\n### Small", &h);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn renders_code_block() {
        let h = Highlighter::default();
        let md = "```rust\nfn main() {}\n```";
        let lines = render_markdown(md, &h);
        assert!(lines.len() >= 3);
    }

    #[test]
    fn renders_inline_code() {
        let spans = parse_inline("use `foo` here");
        assert!(spans.len() >= 3);
    }

    #[test]
    fn renders_list() {
        let h = Highlighter::default();
        let lines = render_markdown("- item one\n- item two", &h);
        assert_eq!(lines.len(), 2);
    }
}
