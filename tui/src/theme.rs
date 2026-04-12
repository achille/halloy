use ratatui::style::Color;

pub fn nick_color(nick: &str) -> Color {
    const COLORS: &[Color] = &[
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::Magenta,
        Color::LightBlue,
        Color::LightGreen,
        Color::LightRed,
        Color::LightCyan,
        Color::LightMagenta,
    ];

    let hash = seahash(nick.as_bytes());
    COLORS[hash as usize % COLORS.len()]
}

fn seahash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
