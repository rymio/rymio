// Chat pane rendering

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

use crate::app::{App, ChatRole, Pane};

/// Render the chat pane with a bordered block.
/// Displays a scrollable message log with role prefixes and a text input line at the bottom.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.focus == Pane::Chat {
        Color::Red
    } else {
        Color::Green
    };

    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    // Calculate inner area to determine available height
    let inner_area = block.inner(area);

    // Split inner area: message log takes all but the last line, input at bottom
    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Message log (fills remaining space)
            Constraint::Length(1), // Input line
        ])
        .split(inner_area);

    let message_area = inner_layout[0];
    let input_area = inner_layout[1];

    // Render the outer block (border)
    frame.render_widget(block, area);

    // Build message lines with role prefixes and colors, wrapping to available width
    let available_width = message_area.width as usize;
    let mut message_lines: Vec<Line> = Vec::new();

    for msg in &app.chat.messages {
        let (prefix, style) = match msg.role {
            ChatRole::User => ("You: ", Style::default().fg(Color::White)),
            ChatRole::Agent => ("Agent: ", Style::default().fg(Color::Green)),
            ChatRole::System => ("System: ", Style::default().fg(Color::Yellow)),
        };

        let full_text = format!("{}{}", prefix, msg.content);

        // Wrap each logical line to the available width
        for line in full_text.lines() {
            if available_width == 0 {
                message_lines.push(Line::from(Span::styled(line.to_string(), style)));
            } else {
                // Word-wrap the line
                let wrapped = wrap_text(line, available_width);
                for wrapped_line in wrapped {
                    message_lines.push(Line::from(Span::styled(wrapped_line, style)));
                }
            }
        }
        // Add a blank line between messages for readability
        message_lines.push(Line::from(""));
    }

    // Calculate scroll offset
    let visible_height = message_area.height as usize;
    let total_lines = message_lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);

    let effective_offset = if app.chat.scroll_offset == 0 {
        // Auto-scroll to bottom
        max_scroll
    } else if app.chat.scroll_offset == usize::MAX {
        // Sentinel: user just pressed Up from auto-scroll, go one page up
        max_scroll.saturating_sub(1)
    } else {
        // Manual scroll: scroll_offset stores "lines from bottom"
        max_scroll.saturating_sub(app.chat.scroll_offset)
    };

    let visible_lines: Vec<Line> = message_lines
        .into_iter()
        .skip(effective_offset)
        .take(visible_height)
        .collect();

    let messages_widget = Paragraph::new(visible_lines);
    frame.render_widget(messages_widget, message_area);

    // Render scrollbar when content overflows visible area
    if total_lines > visible_height {
        let mut scrollbar_state = ScrollbarState::new(total_lines)
            .position(effective_offset);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, message_area, &mut scrollbar_state);
    }

    // Render the input line: "> " prompt followed by input_buffer
    let input_text = format!("> {}", app.chat.input_buffer);
    let input_style = if app.focus == Pane::Chat {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let input_widget = Paragraph::new(Line::from(Span::styled(input_text, input_style)));
    frame.render_widget(input_widget, input_area);

    // Show cursor in input area when chat is focused
    if app.focus == Pane::Chat {
        let cursor_x = input_area.x + 2 + app.chat.cursor_pos as u16; // 2 for "> "
        let cursor_y = input_area.y;
        if cursor_x < input_area.x + input_area.width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

/// Wrap a single line of text to fit within the given width.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    if text.len() <= max_width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_width {
            lines.push(remaining.to_string());
            break;
        }

        // Try to break at a word boundary
        let break_at = remaining[..max_width]
            .rfind(|c: char| c.is_whitespace())
            .unwrap_or(max_width);

        // If we found a space, break there; otherwise hard-break at max_width
        let break_at = if break_at == 0 { max_width } else { break_at };

        lines.push(remaining[..break_at].to_string());
        remaining = remaining[break_at..].trim_start();
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}
