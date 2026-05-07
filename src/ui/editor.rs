// Editor pane rendering

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Pane};

/// Render the editor pane with a bordered block.
/// Displays file content with vertical scrolling, filename in border title,
/// and highlights the cursor line.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.focus == Pane::Editor {
        Color::Red
    } else {
        Color::Green
    };

    // Build the title: show filename if a file is open, otherwise "Editor"
    let title = if let Some(ref path) = app.editor.file_path {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Editor");
        format!(" {} ", filename)
    } else {
        " Editor ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    // Calculate the inner area for content height
    let inner_area = block.inner(area);
    let visible_height = inner_area.height as usize;

    let editor = &app.editor;

    // Build lines for visible content based on scroll_offset
    let mut lines: Vec<Line> = Vec::new();
    let start = editor.scroll_offset;
    let end = (start + visible_height).min(editor.lines.len());

    for i in start..end {
        let line_content = &editor.lines[i];

        // Highlight the cursor line
        let style = if i == editor.cursor_row {
            Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        // Format with line number
        let display = format!("{:>4} │ {}", i + 1, line_content);
        lines.push(Line::from(Span::styled(display, style)));
    }

    // If no file is open, show a placeholder message
    if editor.lines.is_empty() && editor.file_path.is_none() {
        lines.push(Line::from(Span::styled(
            "  No file open. Select a file from the tree.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, area);

    // Show blinking cursor when editor is focused
    if app.focus == Pane::Editor {
        // Calculate cursor screen position
        let line_number_width = 6; // "{:>4} │ " = 6 chars
        let cursor_x = inner_area.x + line_number_width + (editor.cursor_col as u16);
        let cursor_y = inner_area.y + (editor.cursor_row.saturating_sub(editor.scroll_offset)) as u16;

        // Only show cursor if it's within the visible area
        if cursor_y < inner_area.y + inner_area.height && cursor_x < inner_area.x + inner_area.width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}
