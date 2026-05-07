// Agent output pane rendering

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Pane};

/// Render the agent output pane with a bordered block.
/// Displays a scrollable list of output lines (diffs, search results, status).
/// Diff lines are color-coded: "+" green, "-" red, "@@" cyan.
/// Auto-scrolls to the bottom to show the latest output.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.focus == Pane::AgentOutput {
        Color::Red
    } else {
        Color::Green
    };

    let block = Block::default()
        .title(" Agent Output ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    // Calculate inner area to determine available height for scrolling
    let inner_area = block.inner(area);
    let visible_height = inner_area.height as usize;

    // Build styled output lines with diff coloring
    let output_lines: Vec<Line> = app
        .agent_output
        .iter()
        .map(|line| {
            let color = if line.starts_with('+') {
                Color::Green
            } else if line.starts_with('-') {
                Color::Red
            } else if line.starts_with("@@") {
                Color::Cyan
            } else {
                Color::White
            };
            Line::from(Span::styled(line.clone(), Style::default().fg(color)))
        })
        .collect();

    // Auto-scroll to bottom
    let total_lines = output_lines.len();
    let scroll_offset = if total_lines > visible_height {
        total_lines.saturating_sub(visible_height)
    } else {
        0
    };

    let visible_lines: Vec<Line> = output_lines
        .into_iter()
        .skip(scroll_offset)
        .take(visible_height)
        .collect();

    let widget = Paragraph::new(visible_lines).block(block);
    frame.render_widget(widget, area);
}
