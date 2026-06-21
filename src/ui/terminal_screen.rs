use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let outer = Block::default()
        .title(" Terminal ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "Full Screen Terminal",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            app.root_directory.display().to_string(),
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    frame.render_widget(header, rows[0]);

    let visible_height = rows[1].height as usize;
    let total_lines = app.terminal_screen.output_lines.len();
    let max_offset = total_lines.saturating_sub(visible_height);
    let offset = app.terminal_screen.scroll_offset.min(max_offset);
    let visible_lines: Vec<Line> = app
        .terminal_screen
        .output_lines
        .iter()
        .skip(offset)
        .take(visible_height)
        .map(|line| {
            Line::from(Span::styled(
                line.clone(),
                Style::default().fg(Color::White),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(visible_lines), rows[1]);

    let prompt = format!("$ {}", app.terminal_screen.input_buffer);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            prompt,
            Style::default().fg(Color::White),
        ))),
        rows[2],
    );

    let footer = if app.terminal_screen.is_running {
        "Enter run command  Ctrl+Q back to main  Ctrl+B back to main  Running..."
    } else {
        "Enter run command  Ctrl+Q back to main  Ctrl+B back to main"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            footer,
            Style::default().fg(Color::DarkGray),
        ))),
        rows[3],
    );

    let cursor_x = rows[2].x + 2 + app.terminal_screen.cursor_pos as u16;
    let cursor_y = rows[2].y;
    if cursor_x < rows[2].x + rows[2].width {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
