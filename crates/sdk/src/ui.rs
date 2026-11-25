//! 最小限の UI 部品。ジェスチャ判定はここ（ホストは生のタッチしか渡さない）。

use crate::color::{self, Color};
use crate::gfx::Frame;
use crate::{Rect, Touch, TouchPhase};

/// down したボタン上で up したらタップ
pub struct Button {
    pub rect: Rect,
    pub label: String,
    pub color: Color,
    pressed_by: Option<u8>,
}

impl Button {
    pub fn new(rect: Rect, label: impl Into<String>) -> Self {
        Self {
            rect,
            label: label.into(),
            color: color::SURFACE,
            pressed_by: None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn pressed(&self) -> bool {
        self.pressed_by.is_some()
    }

    /// タップが成立したら true。押下状態が変わったかは `pressed()` で見る
    pub fn handle(&mut self, touches: &[Touch]) -> bool {
        let mut tapped = false;
        for t in touches {
            match t.phase {
                TouchPhase::Down if self.rect.contains(t.x, t.y) => self.pressed_by = Some(t.id),
                TouchPhase::Move if self.pressed_by == Some(t.id) => {
                    if !self.rect.inset(-24).contains(t.x, t.y) {
                        self.pressed_by = None;
                    }
                }
                TouchPhase::Up if self.pressed_by == Some(t.id) => {
                    self.pressed_by = None;
                    tapped = self.rect.contains(t.x, t.y);
                }
                TouchPhase::Cancel if self.pressed_by == Some(t.id) => self.pressed_by = None,
                _ => {}
            }
        }
        tapped
    }

    pub fn draw(&self, f: Frame) -> Frame {
        let bg = if self.pressed() {
            color::BORDER
        } else {
            self.color
        };
        f.fill_rect(self.rect, bg, 14)
            .stroke_rect(self.rect, color::BORDER, 14, 2)
            .text_centered(self.rect, 28, color::FG, self.label.clone())
    }
}

/// 縦スワイプでのスクロール量を追う
#[derive(Default)]
pub struct Scroll {
    pub offset: i32,
    pub max: i32,
    drag: Option<(u8, i32, i32)>,
    /// ドラッグ距離（タップとの区別に使う）
    pub moved: i32,
}

impl Scroll {
    /// スクロールが変わったら true
    pub fn handle(&mut self, touches: &[Touch]) -> bool {
        let before = self.offset;
        for t in touches {
            match (t.phase, self.drag) {
                (TouchPhase::Down, None) => {
                    self.drag = Some((t.id, t.y, self.offset));
                    self.moved = 0;
                }
                (TouchPhase::Move, Some((id, y0, o0))) if id == t.id => {
                    self.moved = self.moved.max((t.y - y0).abs());
                    self.offset = (o0 - (t.y - y0)).clamp(0, self.max.max(0));
                }
                (TouchPhase::Up | TouchPhase::Cancel, Some((id, ..))) if id == t.id => {
                    self.drag = None;
                }
                _ => {}
            }
        }
        before != self.offset
    }
}
