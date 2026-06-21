// Shell pane rendering

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Pane};

/// Render the shell pane with a bordered block.
/// Displays a scrollable output log and a command input line at the bottom.
/// Shows a running indicator in the title when a command is executing.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.focus == Pane::Shell {
        Color::Red
    } else {
        Color::Green
    };

    let title = if app.shell.is_running {
        " Shell ⟳ Running... "
    } else {
        " Shell "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    // Calculate inner area to determine available height
    let inner_area = block.inner(area);

    // Split inner area: output log takes all but the last line, input at bottom
    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Output log (fills remaining space)
            Constraint::Length(1), // Input line
        ])
        .split(inner_area);

    let output_area = inner_layout[0];
    let input_area = inner_layout[1];

    // Render the outer block (border)
    frame.render_widget(block, area);

    // Build output lines with styling
    let output_lines: Vec<Line> = app
        .shell
        .output_lines
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                line.clone(),
                Style::default().fg(Color::White),
            ))
        })
        .collect();

    // Apply scroll offset for the output log (auto-scroll to bottom)
    let visible_height = output_area.height as usize;
    let total_lines = output_lines.len();
    let auto_scroll_offset = if total_lines > visible_height {
        total_lines.saturating_sub(visible_height)
    } else {
        0
    };

    // Use the app's scroll_offset if it's been manually set, otherwise auto-scroll
    let effective_offset = if app.shell.scroll_offset > 0 {
        app.shell
            .scroll_offset
            .min(total_lines.saturating_sub(visible_height))
    } else {
        auto_scroll_offset
    };

    let visible_lines: Vec<Line> = output_lines
        .into_iter()
        .skip(effective_offset)
        .take(visible_height)
        .collect();

    let output_widget = Paragraph::new(visible_lines);
    frame.render_widget(output_widget, output_area);

    // Render the input line: "$ " prompt followed by input_buffer
    let input_text = format!("$ {}", app.shell.input_buffer);
    let input_style = if app.focus == Pane::Shell {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let input_widget = Paragraph::new(Line::from(Span::styled(input_text, input_style)));
    frame.render_widget(input_widget, input_area);
}
