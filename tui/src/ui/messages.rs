use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, LineKind, MessageLine};
use crate::theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(buffer) = app.buffers.get(app.active) else {
        let paragraph = Paragraph::new("no buffer")
            .block(Block::default().title("messages").borders(Borders::ALL));
        frame.render_widget(paragraph, area);
        return;
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    let len = buffer.lines.len();
    let end = len.saturating_sub(app.scroll_back.min(len));
    let start = end.saturating_sub(visible_height);

    let lines = buffer
        .lines
        .iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(render_line)
        .collect::<Vec<_>>();

    let paragraph = Paragraph::new(Text::from(lines)).block(
        Block::default()
            .title(buffer.name.clone())
            .borders(Borders::ALL),
    );

    frame.render_widget(paragraph, area);
}

fn render_line(line: &MessageLine) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("[{}] ", line.timestamp),
        Style::default().fg(Color::DarkGray),
    )];

    match line.kind {
        LineKind::Status => {
            spans.push(Span::styled(
                format!("* {}", line.text),
                Style::default().fg(Color::DarkGray),
            ));
        }
        LineKind::Error => {
            spans.push(Span::styled(
                format!("! {}", line.text),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        }
        LineKind::Action => {
            if let Some(nick) = &line.nick {
                spans.push(Span::styled(
                    format!("* {nick} "),
                    Style::default().fg(theme::nick_color(nick)),
                ));
            } else {
                spans.push(Span::raw("* "));
            }
            spans.push(Span::styled(
                line.text.clone(),
                Style::default().fg(Color::Magenta),
            ));
        }
        LineKind::Own | LineKind::Normal | LineKind::Notice => {
            if let Some(nick) = &line.nick {
                let style = if matches!(line.kind, LineKind::Own) {
                    Style::default().fg(Color::LightGreen)
                } else {
                    Style::default().fg(theme::nick_color(nick))
                };
                spans.push(Span::styled(format!("<{nick}> "), style));
            }

            let style = if matches!(line.kind, LineKind::Notice) {
                Style::default().fg(Color::LightBlue)
            } else {
                Style::default()
            };
            spans.push(Span::styled(line.text.clone(), style));
        }
    }

    Line::from(spans)
}
