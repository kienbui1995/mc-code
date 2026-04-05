use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::highlight::Highlighter;
use crate::markdown::render_markdown;

// Thread-local highlighter to avoid re-loading syntect on every frame.
thread_local! {
    static HIGHLIGHTER: Highlighter = Highlighter::default();
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.viewport_height = area.height.saturating_sub(4); // input + status

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // output
            Constraint::Length(3), // input
            Constraint::Length(1), // status bar
        ])
        .split(area);

    draw_output(frame, app, chunks[0]);
    draw_input(frame, app, chunks[1]);
    draw_status(frame, app, chunks[2]);
}

fn draw_output(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line<'_>> = HIGHLIGHTER.with(|h| {
        let full_text = app.output_lines.join("\n");
        render_markdown(&full_text, h)
    });

    let total_lines = lines.len();
    let visible = area.height.saturating_sub(2) as usize; // minus borders

    // Scroll indicator
    let scroll_info = if total_lines > visible {
        let pct = if total_lines == 0 {
            100
        } else {
            ((app.scroll_offset as usize + visible) * 100 / total_lines).min(100)
        };
        format!(
            " {}/{} ({pct}%) ",
            app.scroll_offset as usize + visible,
            total_lines
        )
    } else {
        String::new()
    };

    let title = format!(" magic-code {scroll_info}");
    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));
    frame.render_widget(para, area);
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_text = app.input.as_str();
    let display = if input_text.is_empty() && !app.waiting {
        "› Type your prompt here..."
    } else {
        input_text
    };
    let style = if app.waiting {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    let para = Paragraph::new(display)
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(" Input "));
    frame.render_widget(para, area);

    // Show cursor position
    if !app.waiting {
        let cursor_x = area.x + 1 + app.input.cursor_pos() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x.min(area.right() - 2), cursor_y));
    }
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    // Show permission prompt if pending
    if let Some((ref tool, ref input)) = app.permission_pending {
        let prompt = Line::from(vec![
            Span::styled(
                " ⚠ Allow ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{tool}: "),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if input.len() > 60 {
                    format!("{}...", &input[..57])
                } else {
                    input.clone()
                },
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                " [Y/n/A] ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(prompt).style(Style::default().bg(Color::DarkGray)),
            area,
        );
        return;
    }

    let cost = app.session_cost;
    let status = Line::from(vec![
        Span::styled(
            format!(" {} ", app.model),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ "),
        Span::styled(
            format!(
                "{}↓ {}↑ ${cost:.4}",
                app.total_input_tokens, app.total_output_tokens
            ),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" │ "),
        if app.plan_mode {
            Span::styled(
                "PLAN",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )
        } else if app.waiting {
            Span::styled("⟳ thinking...", Style::default().fg(Color::Yellow))
        } else {
            Span::styled("ready", Style::default().fg(Color::Green))
        },
        Span::raw(" │ "),
        Span::styled(
            "^C cancel  PgUp/PgDn scroll",
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(" │ "),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(status), area);
}
