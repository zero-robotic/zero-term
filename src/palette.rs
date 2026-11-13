use alacritty_terminal::{
    term::color::Colors,
    vte::ansi::{Color as AnsiColor, Rgb},
};
use zui_types::Color;
pub const BG: Color = Color::rgb(20, 24, 34);
pub const FG: Color = Color::rgb(216, 222, 233);
pub const ACCENT: Color = Color::rgb(130, 170, 255);
const ANSI: [[u8; 3]; 16] = [
    [38, 43, 56],
    [240, 113, 120],
    [195, 232, 141],
    [255, 203, 107],
    [130, 170, 255],
    [199, 146, 234],
    [137, 221, 255],
    [208, 216, 230],
    [84, 92, 110],
    [255, 139, 146],
    [213, 242, 165],
    [255, 224, 150],
    [163, 191, 255],
    [220, 179, 255],
    [176, 235, 255],
    [255, 255, 255],
];
pub fn indexed(index: usize) -> Rgb {
    let [r, g, b] = match index {
        0..=15 => ANSI[index],
        16..=231 => {
            let n = index - 16;
            let level = |v| if v == 0 { 0 } else { 55 + v as u8 * 40 };
            [level(n / 36), level(n / 6 % 6), level(n % 6)]
        }
        232..=255 => [8 + (index - 232) as u8 * 10; 3],
        257 => [BG.r, BG.g, BG.b],
        258 => [ACCENT.r, ACCENT.g, ACCENT.b],
        259..=266 => ANSI[index - 259].map(|v| (f32::from(v) * 0.66) as u8),
        _ => [FG.r, FG.g, FG.b],
    };
    Rgb { r, g, b }
}
pub fn resolve(color: AnsiColor, colors: &Colors) -> Color {
    let rgb = match color {
        AnsiColor::Spec(rgb) => rgb,
        AnsiColor::Named(named) => {
            colors[named as usize].unwrap_or_else(|| indexed(named as usize))
        }
        AnsiColor::Indexed(i) => colors[i as usize].unwrap_or_else(|| indexed(i as usize)),
    };
    Color::rgb(rgb.r, rgb.g, rgb.b)
}
