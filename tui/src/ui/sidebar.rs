use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app
        .buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| {
            let marker = if index == app.active { ">" } else { " " };
            let unread = if buffer.unread > 0 {
                format!(" {}", buffer.unread)
            } else {
                String::new()
            };

            let style = if index == app.active {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if buffer.unread > 0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::raw(" "),
                Span::styled(buffer.name.clone(), style),
                Span::styled(unread, Style::default().fg(Color::Yellow)),
            ]))
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(Block::default().title("buffers").borders(Borders::RIGHT));

    frame.render_widget(list, area);
}
