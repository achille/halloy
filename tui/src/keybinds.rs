use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Keybinds {
    pub sidebar_toggle: String,
    pub command_palette: String,
    pub quit: String,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            sidebar_toggle: "ctrl+l".to_string(),
            command_palette: "ctrl+k".to_string(),
            quit: "ctrl+c".to_string(),
        }
    }
}
