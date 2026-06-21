// Settings page overlay rendering

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Render the settings page as a centered overlay popup.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Center a 60×22 box
    let popup_width: u16 = 60;
    let popup_height: u16 = 22;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Clear background
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Settings — LLM Provider ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Layout: provider selector, fields, and help text
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 0: blank
            Constraint::Length(1), // 1: provider label
            Constraint::Length(1), // 2: provider value
            Constraint::Length(1), // 3: blank
            Constraint::Length(1), // 4: base_url label
            Constraint::Length(1), // 5: base_url value
            Constraint::Length(1), // 6: blank
            Constraint::Length(1), // 7: api_key label
            Constraint::Length(1), // 8: api_key value
            Constraint::Length(1), // 9: blank
            Constraint::Length(1), // 10: model label
            Constraint::Length(1), // 11: model value
            Constraint::Length(1), // 12: blank
            Constraint::Length(1), // 13: rag label
            Constraint::Length(1), // 14: rag value
            Constraint::Length(1), // 15: blank
            Constraint::Min(1),    // 16: help/status
        ])
        .split(inner);

    let settings = &app.settings;

    let highlight = Style::default()
        .fg(Color::Black)
        .bg(Color::LightGreen)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(Color::DarkGray);
    let value_style = Style::default().fg(Color::White);

    // Provider
    let provider_label = Paragraph::new(Line::from(Span::styled(
        "  Provider (←/→ to change):",
        label_style,
    )));
    frame.render_widget(provider_label, rows[1]);

    let provider_display = format!("  < {} >", settings.providers[settings.selected_provider].0);
    let provider_style = if settings.focused_field == 0 {
        highlight
    } else {
        value_style
    };
    let provider_widget =
        Paragraph::new(Line::from(Span::styled(provider_display, provider_style)));
    frame.render_widget(provider_widget, rows[2]);

    // Base URL
    let url_label = Paragraph::new(Line::from(Span::styled("  Base URL:", label_style)));
    frame.render_widget(url_label, rows[4]);

    let url_display = format!("  {}", settings.base_url_buffer);
    let url_style = if settings.focused_field == 1 {
        highlight
    } else {
        value_style
    };
    let url_widget = Paragraph::new(Line::from(Span::styled(url_display, url_style)));
    frame.render_widget(url_widget, rows[5]);

    // API Key
    let key_label = Paragraph::new(Line::from(Span::styled("  API Key:", label_style)));
    frame.render_widget(key_label, rows[7]);

    let key_display = if settings.api_key_buffer.is_empty() {
        "  (not set)".to_string()
    } else {
        format!("  {}", "*".repeat(settings.api_key_buffer.len().min(30)))
    };
    let key_style = if settings.focused_field == 2 {
        highlight
    } else {
        value_style
    };
    let key_widget = Paragraph::new(Line::from(Span::styled(key_display, key_style)));
    frame.render_widget(key_widget, rows[8]);

    // Model
    let model_label = Paragraph::new(Line::from(Span::styled("  Model:", label_style)));
    frame.render_widget(model_label, rows[10]);

    let model_display = format!("  {}", settings.model_buffer);
    let model_style = if settings.focused_field == 3 {
        highlight
    } else {
        value_style
    };
    let model_widget = Paragraph::new(Line::from(Span::styled(model_display, model_style)));
    frame.render_widget(model_widget, rows[11]);

    // RAG Indexing
    let rag_label = Paragraph::new(Line::from(Span::styled(
        "  RAG Indexing (←/→ to toggle):",
        label_style,
    )));
    frame.render_widget(rag_label, rows[13]);

    let rag_display = if settings.rag_enabled {
        "  < Enabled >"
    } else {
        "  < Disabled >"
    };
    let rag_style = if settings.focused_field == 4 {
        highlight
    } else {
        value_style
    };
    let rag_widget = Paragraph::new(Line::from(Span::styled(rag_display, rag_style)));
    frame.render_widget(rag_widget, rows[14]);

    // Help text
    let help = Paragraph::new(vec![Line::from(Span::styled(
        "  Tab/↑↓: navigate fields  ←/→: change provider  Enter: save  Esc: cancel",
        Style::default().fg(Color::DarkGray),
    ))])
    .alignment(Alignment::Left);
    frame.render_widget(help, rows[16]);
}
