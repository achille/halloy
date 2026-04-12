use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app
        .buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| {
            let style = if index == app.active {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>2}. ", index + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(buffer.name.clone(), style),
            ]))
        })
        .collect::<Vec<_>>();

    let list = List::new(items).block(
        Block::default()
            .title("switch buffer")
            .borders(Borders::ALL),
    );

    frame.render_widget(Clear, area);
    frame.render_widget(list, area);
}
