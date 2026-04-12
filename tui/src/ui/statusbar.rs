use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let active = app
        .buffers
        .get(app.active)
        .map(|buffer| buffer.name.as_str())
        .unwrap_or("none");
    let mode = if app.compose_mode { " COMPOSE" } else { "" };
    let sidebar = if app.sidebar_visible {
        "sidebar"
    } else {
        "full"
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" {active}{mode} "),
            Style::default().fg(Color::Black).bg(Color::White),
        ),
        Span::raw(format!(" {sidebar} | {} ", app.status)),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}
