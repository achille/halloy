use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::App;

impl App {
    pub(crate) fn handle_terminal_event(
        &mut self,
        event: Event,
    ) -> Result<bool> {
        match event {
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    || key.kind == KeyEventKind::Repeat =>
            {
                self.handle_key(key)
            }
            Event::Resize(_, _) => Ok(false),
            _ => Ok(false),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.command_palette {
            return self.handle_palette_key(key);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), modifiers)
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                Ok(self.handle_ctrl_c())
            }
            (KeyCode::Char('d'), modifiers)
                if modifiers.contains(KeyModifiers::CONTROL)
                    && self.compose_mode =>
            {
                self.compose_mode = false;
                self.submit_input()
            }
            (KeyCode::Char('k'), modifiers)
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.command_palette = true;
                self.status = "channel switcher".to_string();
                Ok(false)
            }
            (KeyCode::Char('l'), modifiers)
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.sidebar_visible = !self.sidebar_visible;
                Ok(false)
            }
            (KeyCode::Char(ch), modifiers)
                if modifiers.contains(KeyModifiers::ALT)
                    && ch.is_ascii_digit() =>
            {
                if let Some(digit) = ch.to_digit(10) {
                    let index = digit.saturating_sub(1) as usize;
                    self.switch_to(index);
                }
                Ok(false)
            }
            (KeyCode::Up, modifiers)
                if modifiers.contains(KeyModifiers::ALT) =>
            {
                self.switch_relative(-1);
                Ok(false)
            }
            (KeyCode::Down, modifiers)
                if modifiers.contains(KeyModifiers::ALT) =>
            {
                self.switch_relative(1);
                Ok(false)
            }
            (KeyCode::PageUp, _) => {
                self.scroll_back = self.scroll_back.saturating_add(10);
                Ok(false)
            }
            (KeyCode::PageDown, _) => {
                self.scroll_back = self.scroll_back.saturating_sub(10);
                Ok(false)
            }
            (KeyCode::Home, _) => {
                if let Some(buffer) = self.buffers.get(self.active) {
                    self.scroll_back = buffer.lines.len();
                }
                Ok(false)
            }
            (KeyCode::End, _) => {
                self.scroll_back = 0;
                Ok(false)
            }
            (KeyCode::Esc, _) => {
                self.command_palette = false;
                if self.compose_mode {
                    self.compose_mode = false;
                    self.input.clear();
                    self.status = "compose cancelled".to_string();
                }
                Ok(false)
            }
            (KeyCode::Enter, modifiers)
                if self.compose_mode
                    || modifiers.contains(KeyModifiers::SHIFT) =>
            {
                self.input.push('\n');
                Ok(false)
            }
            (KeyCode::Enter, _) => self.submit_input(),
            (KeyCode::Backspace, _) => {
                self.input.pop();
                Ok(false)
            }
            (KeyCode::Delete, _) => Ok(false),
            (KeyCode::Tab, _) => {
                self.complete_current_word();
                Ok(false)
            }
            (KeyCode::Char(ch), modifiers)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.input.push(ch);
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn handle_palette_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.command_palette = false;
            }
            KeyCode::Up => {
                self.switch_relative(-1);
                self.command_palette = true;
            }
            KeyCode::Down => {
                self.switch_relative(1);
                self.command_palette = true;
            }
            KeyCode::Enter => {
                self.command_palette = false;
            }
            KeyCode::Char(ch) if ch.is_ascii_digit() => {
                if let Some(digit) = ch.to_digit(10) {
                    self.switch_to(digit.saturating_sub(1) as usize);
                }
            }
            _ => {}
        }

        Ok(false)
    }

    fn complete_current_word(&mut self) {
        let Some(active) = self.active_upstream() else {
            return;
        };

        let names = self
            .buffers
            .iter()
            .filter(|buffer| buffer.upstream.server() == active.server())
            .map(|buffer| buffer.name.as_str());

        let word_start = self
            .input
            .rfind(char::is_whitespace)
            .map_or(0, |index| index + 1);
        let prefix = &self.input[word_start..];

        if prefix.is_empty() {
            return;
        }

        if let Some(match_name) = names
            .filter(|name| name.starts_with(prefix))
            .min_by_key(|name| name.len())
        {
            self.input.truncate(word_start);
            self.input.push_str(match_name);
        }
    }
}
