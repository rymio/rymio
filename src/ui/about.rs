// About page overlay rendering

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Render the about page as a centered overlay popup.
pub fn render(frame: &mut Frame, _app: &App) {
    let area = frame.area();

    // Center a 40×7 box
    let popup_width: u16 = 40;
    let popup_height: u16 = 7;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Clear background
    frame.render_widget(Clear, popup_area);

    // Render bordered block with content
    let block = Block::default()
        .title(" About ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "litecode-agent",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("by Robert Rymarczyk"),
        Line::from(""),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, popup_area);
}
