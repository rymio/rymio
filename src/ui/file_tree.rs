// File tree widget state and rendering

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Frame;

use crate::app::{App, Pane};

/// Render the file tree pane with a bordered block.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.focus == Pane::FileTree {
        Color::Red
    } else {
        Color::Green
    };

    // Show current directory in the title
    let dir_name = app
        .root_directory
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("/");
    let title = format!(" {} ", dir_name);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    // Calculate the inner area (inside the border) for content height
    let inner_area = block.inner(area);
    let visible_height = inner_area.height as usize;

    let tree = &app.file_tree;

    // Build lines for visible entries based on scroll_offset
    let mut lines: Vec<Line> = Vec::new();
    let start = tree.scroll_offset;
    let end = (start + visible_height).min(tree.entries.len());

    for i in start..end {
        let entry = &tree.entries[i];

        // Indentation: 2 spaces per depth level
        let indent = "  ".repeat(entry.depth);

        // Prefix: directories show expand/collapse indicator, files show spaces for alignment
        let prefix = if entry.is_dir {
            if tree.expanded_dirs.contains(&entry.path) {
                "▼ "
            } else {
                "▶ "
            }
        } else {
            "  "
        };

        let content = format!("{}{}{}", indent, prefix, entry.name);

        let style = if i == tree.selected_index {
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD)
        } else if entry.is_dir {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(Span::styled(content, style)));
    }

    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, area);

    // Render scrollbar when content overflows the visible area
    if tree.entries.len() > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(tree.entries.len()).position(tree.scroll_offset);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, inner_area, &mut scrollbar_state);
    }
}
