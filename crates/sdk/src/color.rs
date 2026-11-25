//! 色（0xAARRGGBB）

pub type Color = u32;

pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    0xff00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// アルファだけ差し替える
pub const fn with_alpha(c: Color, a: u8) -> Color {
    (c & 0x00ff_ffff) | (a as u32) << 24
}

pub const BLACK: Color = rgb(0, 0, 0);
pub const WHITE: Color = rgb(255, 255, 255);
pub const BG: Color = rgb(0x11, 0x18, 0x27);
pub const SURFACE: Color = rgb(0x1f, 0x29, 0x37);
pub const BORDER: Color = rgb(0x37, 0x41, 0x51);
pub const FG: Color = rgb(0xe5, 0xe7, 0xeb);
pub const MUTED: Color = rgb(0x9c, 0xa3, 0xaf);
pub const ACCENT: Color = rgb(0x60, 0xa5, 0xfa);
pub const RED: Color = rgb(0xef, 0x44, 0x44);
pub const GREEN: Color = rgb(0x22, 0xc5, 0x5e);
pub const YELLOW: Color = rgb(0xea, 0xb3, 0x08);
