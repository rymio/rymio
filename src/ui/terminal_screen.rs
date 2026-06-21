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
        ])
        .split(inner);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "Interactive Terminal",
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

    let screen = app.terminal_screen.parser.screen();
    let contents = screen.contents();
    let visible_lines: Vec<Line> = contents
        .lines()
        .map(|line| {
            Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::White),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(visible_lines), rows[1]);

    let footer = if app.terminal_screen.is_connected {
        format!(
            "{}  Ctrl+Q back to main  PgUp/PgDn scroll",
            app.terminal_screen.status_message
        )
    } else {
        app.terminal_screen.status_message.clone()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            footer,
            Style::default().fg(Color::DarkGray),
        ))),
        rows[2],
    );

    let (cursor_row, cursor_col) = screen.cursor_position();
    let cursor_x = rows[1].x + cursor_col;
    let cursor_y = rows[1].y + cursor_row;
    if cursor_x < rows[1].x + rows[1].width && cursor_y < rows[1].y + rows[1].height {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
