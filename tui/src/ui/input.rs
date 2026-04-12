use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let title = if app.compose_mode {
        "input [COMPOSE]"
    } else {
        "input"
    };

    let paragraph = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false })
        .block(Block::default().title(title).borders(Borders::ALL));

    frame.render_widget(paragraph, area);
}
