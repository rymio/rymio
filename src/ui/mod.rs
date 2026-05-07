pub mod file_tree;
pub mod editor;
pub mod chat;
pub mod shell;
pub mod agent_output;
pub mod about;
pub mod settings;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

/// Render the full TUI layout into the given frame.
pub fn render(frame: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // Header
            Constraint::Percentage(65), // Main area (tree + editor + chat)
            Constraint::Percentage(10), // Agent output
            Constraint::Percentage(20), // Shell
            Constraint::Length(1),      // Footer
        ])
        .split(frame.area());

    // Main area: horizontal split
    let main_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // File tree
            Constraint::Percentage(75), // Editor + Chat vertical
        ])
        .split(outer[1]);

    // Right column: editor (80% height) + chat (20% height)
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(80), // Editor
            Constraint::Percentage(20), // Chat
        ])
        .split(main_cols[1]);

    // Render each pane
    render_header(frame, outer[0], app);
    file_tree::render(frame, main_cols[0], app);
    editor::render(frame, right_rows[0], app);
    chat::render(frame, right_rows[1], app);
    agent_output::render(frame, outer[2], app);
    shell::render(frame, outer[3], app);
    render_footer(frame, outer[4], app);

    // About page overlay (if active)
    if app.show_about {
        about::render(frame, app);
    }

    // Help page overlay (if active)
    if app.show_help {
        render_help(frame);
    }

    // Settings page overlay (if active)
    if app.show_settings {
        settings::render(frame, app);
    }

    // Create file input overlay (if active)
    if app.create_file_mode {
        render_create_file_prompt(frame, app);
    }

    // Delete confirmation overlay (if active)
    if app.confirm_delete {
        render_delete_confirm(frame, app);
    }
}

/// Render the header bar: "litecode-agent — {filename}" or "litecode-agent — {root}".
fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let title = if let Some(ref path) = app.editor.file_path {
        let full_path = app.root_directory.join(path);
        format!("litecode-agent — {}", full_path.display())
    } else {
        format!("litecode-agent — {}", app.root_directory.display())
    };

    let header = Paragraph::new(Line::from(Span::styled(
        title,
        Style::default().fg(Color::LightGreen),
    )));
    frame.render_widget(header, area);
}

/// Render the footer bar with key binding hints.
fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let hints = if app.create_file_mode {
        "Enter filename (Enter: create, Esc: cancel)"
    } else if app.confirm_delete {
        "Delete selected? (y: confirm, any other key: cancel)"
    } else {
        "[q Quit] [Tab Next] [F1 Shell] [F2 Chat] [F3 Editor] [F4 New] [F8 Refresh] [F10 Settings] [F11 Delete] [F12 Help]"
    };
    let footer = Paragraph::new(Line::from(Span::styled(
        hints,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(footer, area);
}

/// Render the "create file" input prompt as a centered overlay.
fn render_create_file_prompt(frame: &mut Frame, app: &App) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::{Block, Borders, Clear};

    let area = frame.area();
    let popup_width: u16 = 50;
    let popup_height: u16 = 5;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Create File (Esc to cancel) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Show the input buffer with a cursor indicator
    let display = if app.create_file_buffer.is_empty() {
        "Enter filename: _".to_string()
    } else {
        let mut s = format!("Enter filename: {}", app.create_file_buffer);
        s.push('_');
        s
    };

    let input = Paragraph::new(Line::from(Span::styled(
        display,
        Style::default().fg(Color::White),
    )))
    .alignment(Alignment::Left);
    frame.render_widget(input, inner);
}

/// Render the delete confirmation prompt as a centered overlay.
fn render_delete_confirm(frame: &mut Frame, app: &App) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::{Block, Borders, Clear};

    let area = frame.area();
    let popup_width: u16 = 50;
    let popup_height: u16 = 5;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Confirm Delete ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Show what will be deleted
    let target_name = app
        .file_tree
        .entries
        .get(app.file_tree.selected_index)
        .map(|e| e.name.as_str())
        .unwrap_or("(nothing selected)");

    let msg = format!("Delete \"{}\"? [y/N]", target_name);
    let prompt = Paragraph::new(Line::from(Span::styled(
        msg,
        Style::default().fg(Color::White),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(prompt, inner);
}

/// Render the help overlay showing all keybindings.
fn render_help(frame: &mut Frame) {
    use ratatui::layout::Alignment;
    use ratatui::style::Modifier;
    use ratatui::widgets::{Block, Borders, Clear};

    let area = frame.area();
    let popup_width: u16 = 56;
    let popup_height: u16 = 30;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Help — Keybindings (press any key to close) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Global",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from("  Ctrl+Q        Quit"),
        Line::from("  Ctrl+S        Save file"),
        Line::from("  Ctrl+R        Re-run last shell command"),
        Line::from("  Ctrl+D        Git diff"),
        Line::from("  Tab           Cycle focus between panes"),
        Line::from("  q             Quit (when not in input pane)"),
        Line::from(""),
        Line::from(Span::styled(
            " Function Keys",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from("  F1            Focus Shell pane"),
        Line::from("  F2            Focus Chat pane"),
        Line::from("  F3            Focus Editor pane"),
        Line::from("  F4            Create new file/directory"),
        Line::from("  F5            Accept pending patch"),
        Line::from("  F6            Refuse pending patch"),
        Line::from("  F7            Undo last applied patch"),
        Line::from("  F8            Refresh file tree"),
        Line::from("  F9            About"),
        Line::from("  F10           Settings (LLM provider)"),
        Line::from("  F11           Delete selected file"),
        Line::from("  F12           This help screen"),
        Line::from(""),
        Line::from(Span::styled(
            " File Tree",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from("  Enter         Open file / enter directory"),
        Line::from("  Backspace/←   Go to parent directory"),
        Line::from("  →             Expand/collapse directory"),
        Line::from("  /             Jump to filesystem root"),
        Line::from("  ~             Jump to home directory"),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);
}
