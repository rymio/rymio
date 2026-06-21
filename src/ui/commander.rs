use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, CommanderEditorState, CommanderListState, CommanderPane};

const MC_BG: Color = Color::Rgb(18, 64, 145);
const MC_PANEL_BG: Color = Color::Rgb(21, 73, 164);
const MC_BORDER: Color = Color::Cyan;
const MC_TEXT: Color = Color::White;
const MC_DIM: Color = Color::Rgb(172, 205, 255);
const MC_HILITE_BG: Color = Color::Cyan;
const MC_HILITE_FG: Color = Color::Black;
const MC_MENU_BG: Color = Color::Rgb(44, 127, 214);
const MC_FKEY_BG: Color = Color::Rgb(210, 210, 210);
const MC_FKEY_NUM_BG: Color = Color::Rgb(35, 89, 180);

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(MC_BG).fg(MC_TEXT)),
        area,
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_menu_bar(frame, rows[0]);
    render_title_bar(frame, rows[1], app);
    if let Some(editor) = app.commander.editor.as_ref() {
        render_editor(frame, rows[2], editor);
    } else {
        render_panels(frame, rows[2], app);
    }
    render_status_bar(frame, rows[3], app);
    render_function_bar(frame, rows[4], app.commander.editor.is_some());
}

fn render_menu_bar(frame: &mut Frame, area: Rect) {
    let menu = Line::from(vec![
        menu_span("Left"),
        Span::raw("  "),
        menu_span("File"),
        Span::raw("  "),
        menu_span("Command"),
        Span::raw("  "),
        menu_span("Options"),
        Span::raw("  "),
        menu_span("Right"),
    ]);

    frame.render_widget(
        Paragraph::new(menu).style(Style::default().bg(MC_MENU_BG).fg(Color::Black)),
        area,
    );
}

fn render_title_bar(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(
        " litecode-agent Commander  Left: {}  Right: {} ",
        app.commander.left.current_dir.display(),
        app.commander.right.current_dir.display()
    );

    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(MC_BG).fg(Color::Yellow)),
        area,
    );
}

fn render_panels(frame: &mut Frame, area: Rect, app: &App) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_pane(
        frame,
        panels[0],
        &app.commander.left,
        app.commander.active_pane == CommanderPane::Left,
    );
    render_pane(
        frame,
        panels[1],
        &app.commander.right,
        app.commander.active_pane == CommanderPane::Right,
    );
}

fn render_pane(frame: &mut Frame, area: Rect, pane: &CommanderListState, active: bool) {
    let block = Block::default()
        .title(format!(" {} ", pane.current_dir.display()))
        .borders(Borders::ALL)
        .style(Style::default().bg(MC_PANEL_BG))
        .border_style(Style::default().fg(if active { Color::White } else { MC_BORDER }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(inner);

    render_header_row(frame, rows[0], active);

    let visible_height = rows[1].height as usize;
    let start = pane.scroll_offset;
    let end = (start + visible_height).min(pane.entries.len());
    let name_width = rows[1].width.saturating_sub(24) as usize;

    let mut lines = Vec::new();
    for i in start..end {
        let entry = &pane.entries[i];
        let selected = i == pane.selected_index;
        let style = if selected {
            Style::default()
                .fg(MC_HILITE_FG)
                .bg(MC_HILITE_BG)
                .add_modifier(Modifier::BOLD)
        } else if entry.is_dir {
            Style::default().fg(Color::Cyan).bg(MC_PANEL_BG)
        } else {
            Style::default().fg(MC_TEXT).bg(MC_PANEL_BG)
        };

        let info = describe_entry(entry.path.as_path(), entry.is_dir);
        let name = truncate_name(&entry.name, name_width.max(8));
        let row = format!(
            "{:<name_width$} {:>8} {:<5} {:>8}",
            name,
            info.size,
            info.kind,
            info.modified,
            name_width = name_width.max(8)
        );
        lines.push(Line::from(Span::styled(row, style)));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(MC_PANEL_BG)),
        rows[1],
    );

    let footer = format!(
        "{}/{}  {}",
        pane.selected_index
            .saturating_add(1)
            .min(pane.entries.len()),
        pane.entries.len(),
        pane.current_dir.display()
    );
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().bg(MC_BG).fg(MC_DIM)),
        rows[2],
    );
}

fn render_header_row(frame: &mut Frame, area: Rect, active: bool) {
    let style = if active {
        Style::default().bg(MC_BG).fg(Color::Yellow)
    } else {
        Style::default().bg(MC_BG).fg(MC_DIM)
    };
    let width = area.width.saturating_sub(24) as usize;
    let header = format!(
        "{:<width$} {:>8} {:<5} {:>8}",
        "Name",
        "Size",
        "Type",
        "Modified",
        width = width.max(8)
    );
    frame.render_widget(Paragraph::new(header).style(style), area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let message = if app.commander.editor.is_some() {
        "Commander editor: Ctrl+S save, Esc back to commander, Ctrl+Q/F10 back to main"
    } else {
        app.commander.status_message.as_str()
    };
    frame.render_widget(
        Paragraph::new(message).style(Style::default().bg(MC_BG).fg(Color::White)),
        area,
    );
}

fn render_function_bar(frame: &mut Frame, area: Rect, editing: bool) {
    let labels = if editing {
        vec![
            ("1", "Help"),
            ("2", "Menu"),
            ("3", "View"),
            ("4", "Edit"),
            ("5", "Save"),
            ("6", "Close"),
            ("7", "Search"),
            ("8", "Delete"),
            ("9", "PullDn"),
            ("10", "Main"),
        ]
    } else {
        vec![
            ("1", "Help"),
            ("2", "Menu"),
            ("3", "View"),
            ("4", "Edit"),
            ("5", "Copy"),
            ("6", "Move"),
            ("7", "Mkdir"),
            ("8", "Delete"),
            ("9", "PullDn"),
            ("10", "Main"),
        ]
    };

    let mut spans = Vec::new();
    for (index, label) in labels {
        spans.push(Span::styled(
            format!("{index}"),
            Style::default()
                .bg(MC_FKEY_NUM_BG)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label} "),
            Style::default().bg(MC_FKEY_BG).fg(Color::Black),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(MC_FKEY_BG)),
        area,
    );
}

fn render_editor(frame: &mut Frame, area: Rect, editor: &CommanderEditorState) {
    let block = Block::default()
        .title(format!(" {} ", editor.file_path.display()))
        .borders(Borders::ALL)
        .style(Style::default().bg(MC_PANEL_BG))
        .border_style(Style::default().fg(Color::White));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(4)])
        .split(inner);

    let header_width = rows[0].width.saturating_sub(8) as usize;
    let editor_header = format!("{:<width$} {}", "Text", "Line", width = header_width.max(8));
    frame.render_widget(
        Paragraph::new(editor_header).style(Style::default().bg(MC_BG).fg(Color::Yellow)),
        rows[0],
    );

    let visible_height = rows[1].height as usize;
    let start = editor.scroll_offset;
    let end = (start + visible_height).min(editor.lines.len());
    let mut lines = Vec::new();
    for i in start..end {
        let style = if i == editor.cursor_row {
            Style::default()
                .fg(MC_HILITE_FG)
                .bg(MC_HILITE_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MC_TEXT).bg(MC_PANEL_BG)
        };
        lines.push(Line::from(Span::styled(
            format!("{:>4} {}", i + 1, editor.lines[i]),
            style,
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(MC_PANEL_BG)),
        rows[1],
    );

    let line_number_width = 5;
    let cursor_x = rows[1].x + line_number_width + editor.cursor_col as u16;
    let cursor_y = rows[1].y + editor.cursor_row.saturating_sub(editor.scroll_offset) as u16;
    if cursor_y < rows[1].y + rows[1].height && cursor_x < rows[1].x + rows[1].width {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

fn menu_span(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    )
}

struct EntryInfo {
    size: String,
    kind: String,
    modified: String,
}

fn describe_entry(path: &Path, is_dir: bool) -> EntryInfo {
    if path == Path::new("..") {
        return EntryInfo {
            size: "UP-DIR".to_string(),
            kind: "DIR".to_string(),
            modified: String::new(),
        };
    }

    let metadata = fs::metadata(path).ok();
    let size = if is_dir {
        "<DIR>".to_string()
    } else {
        metadata
            .as_ref()
            .map(|meta| human_size(meta.len()))
            .unwrap_or_else(|| "?".to_string())
    };
    let kind = if is_dir { "DIR" } else { "FILE" }.to_string();
    let modified = metadata
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|dur| {
            let days = dur.as_secs() / 86_400;
            format!("{days:>8}")
        })
        .unwrap_or_default();

    EntryInfo {
        size,
        kind,
        modified,
    }
}

fn truncate_name(name: &str, width: usize) -> String {
    let mut chars = name.chars();
    let truncated: String = chars.by_ref().take(width).collect();
    if name.chars().count() > width && width > 1 {
        let mut visible: String = truncated.chars().take(width - 1).collect();
        visible.push('~');
        visible
    } else {
        truncated
    }
}

fn human_size(size: u64) -> String {
    if size >= 1_000_000_000 {
        format!("{:.1}G", size as f64 / 1_000_000_000.0)
    } else if size >= 1_000_000 {
        format!("{:.1}M", size as f64 / 1_000_000.0)
    } else if size >= 1_000 {
        format!("{:.1}K", size as f64 / 1_000.0)
    } else {
        size.to_string()
    }
}
