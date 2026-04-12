use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::App;

pub mod command_bar;
pub mod input;
pub mod messages;
pub mod sidebar;
pub mod statusbar;

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(input_height(app)),
            Constraint::Length(1),
        ])
        .split(area);

    let body = if app.sidebar_visible {
        let width = app.tui_config.sidebar_width.max(12);
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(width), Constraint::Min(1)])
            .split(vertical[0]);
        sidebar::render(frame, horizontal[0], app);
        horizontal[1]
    } else {
        vertical[0]
    };

    messages::render(frame, body, app);
    input::render(frame, vertical[1], app);
    statusbar::render(frame, vertical[2], app);

    if app.command_palette {
        command_bar::render(frame, centered_rect(60, 50, area), app);
    }
}

fn input_height(app: &App) -> u16 {
    if app.compose_mode { 5 } else { 3 }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}
