//! 描画コマンドを 1 フレーム分ためて `submit` で送る。
//!
//! ホスト呼び出しは 1 フレーム 1 回なので、インタプリタ実行でも境界越えのコストが小さい。

use crate::bindings::wasmos::os::gfx as host;
pub use host::{Cmd, TextAlign};

use crate::color::Color;
use crate::Rect;

pub const fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn inset(&self, d: i32) -> Rect {
        rect(self.x + d, self.y + d, self.w - 2 * d, self.h - 2 * d)
    }
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

#[derive(Default)]
pub struct Frame {
    cmds: Vec<Cmd>,
}

impl Frame {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(mut self, cmd: Cmd) -> Self {
        self.cmds.push(cmd);
        self
    }

    pub fn clear(self, color: Color) -> Self {
        self.push(Cmd::Clear(color))
    }

    pub fn fill_rect(self, rect: Rect, color: Color, radius: u16) -> Self {
        self.push(Cmd::FillRect(host::FillRectCmd {
            rect,
            color,
            radius,
        }))
    }

    pub fn stroke_rect(self, rect: Rect, color: Color, radius: u16, width: u16) -> Self {
        self.push(Cmd::StrokeRect(host::StrokeRectCmd {
            rect,
            color,
            radius,
            width,
        }))
    }

    pub fn line(self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color, width: u16) -> Self {
        self.push(Cmd::Line(host::LineCmd {
            x0,
            y0,
            x1,
            y1,
            color,
            width,
        }))
    }

    pub fn circle(self, x: i32, y: i32, r: u16, color: Color) -> Self {
        self.push(Cmd::FillCircle(host::CircleCmd { x, y, r, color }))
    }

    /// 左揃え。(x, y) は左上
    pub fn text(self, x: i32, y: i32, size: u16, color: Color, text: impl Into<String>) -> Self {
        self.text_aligned(x, y, size, color, TextAlign::Left, text)
    }

    pub fn text_aligned(
        self,
        x: i32,
        y: i32,
        size: u16,
        color: Color,
        align: TextAlign,
        text: impl Into<String>,
    ) -> Self {
        self.push(Cmd::Text(host::TextCmd {
            x,
            y,
            size,
            color,
            align,
            text: text.into(),
        }))
    }

    /// 矩形の中央にテキストを置く
    pub fn text_centered(self, r: Rect, size: u16, color: Color, text: impl Into<String>) -> Self {
        let (cx, cy) = r.center();
        self.text_aligned(
            cx,
            cy - size as i32 / 2,
            size,
            color,
            TextAlign::Center,
            text,
        )
    }

    pub fn image(self, id: u32, src: Option<Rect>, dst: Rect, alpha: u8) -> Self {
        self.push(Cmd::Image(host::ImageCmd {
            id,
            src,
            dst,
            alpha,
        }))
    }

    pub fn push_clip(self, r: Rect) -> Self {
        self.push(Cmd::PushClip(r))
    }

    pub fn pop_clip(self) -> Self {
        self.push(Cmd::PopClip)
    }

    /// 他の部品（`ui::Button::draw` など）に描かせる
    pub fn with(self, f: impl FnOnce(Self) -> Self) -> Self {
        f(self)
    }

    pub fn submit(self) {
        host::submit(&self.cmds);
    }
}

pub fn measure_text(text: &str, size: u16) -> u32 {
    host::measure_text(text, size)
}

pub fn load_image(data: &[u8]) -> u32 {
    host::load_image(data)
}

pub fn image_from_rgba(width: u32, height: u32, pixels: &[u8]) -> u32 {
    host::image_from_rgba(width, height, pixels)
}

pub fn image_size(id: u32) -> Option<(u32, u32)> {
    host::image_size(id)
}

pub fn drop_image(id: u32) {
    host::drop_image(id)
}
