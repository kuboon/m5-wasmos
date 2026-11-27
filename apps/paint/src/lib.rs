//! 指でお絵かき（マルチタッチ）。下のパレットで 6 色から選ぶ。
//!
//! 描いた線は点の列として storage に保存し、起動時に描き直す。ホームへ戻ると
//! アプリは予告なく終わるので、線を 1 本描き終えるたびに保存する。

use std::collections::HashMap;

use wasmos_sdk::color::{self, rgb, Color};
use wasmos_sdk::gfx::{rect, Frame};
use wasmos_sdk::ui::Button;
use wasmos_sdk::{log, screen_size, storage, system, App, Event, Rect, TouchPhase};

const PALETTE: [Color; 6] = [
    rgb(0x11, 0x18, 0x27),
    rgb(0xef, 0x44, 0x44),
    rgb(0xf9, 0x73, 0x16),
    rgb(0x22, 0xc5, 0x5e),
    rgb(0x3b, 0x82, 0xf6),
    rgb(0xa8, 0x55, 0xf7),
];

const TOOLBAR: i32 = 88;
const PALETTE_H: i32 = 112;
const PEN: u16 = 12;
const KEY: &str = "strokes";
/// 保存形式の版（先頭 1 バイト）
const FORMAT: u8 = 1;

struct Stroke {
    color: u8,
    points: Vec<(u16, u16)>,
}

impl Stroke {
    fn color(&self) -> Color {
        PALETTE[self.color as usize % PALETTE.len()]
    }

    /// 点に丸、点と点の間に太線（描いている最中と同じ描き方）
    fn draw(&self, mut f: Frame) -> Frame {
        let c = self.color();
        let mut prev: Option<(i32, i32)> = None;
        for &(x, y) in &self.points {
            let (x, y) = (x as i32, y as i32);
            if let Some((x0, y0)) = prev {
                f = f.line(x0, y0, x, y, c, PEN);
            }
            f = f.circle(x, y, PEN / 2, c);
            prev = Some((x, y));
        }
        f
    }
}

/// [版][色 u8, 点の数 u16, (x u16, y u16)...]...（数値はリトルエンディアン）
fn encode(strokes: &[Stroke]) -> Vec<u8> {
    let mut out = vec![FORMAT];
    for s in strokes {
        for chunk in s.points.chunks(u16::MAX as usize) {
            out.push(s.color);
            out.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            for &(x, y) in chunk {
                out.extend_from_slice(&x.to_le_bytes());
                out.extend_from_slice(&y.to_le_bytes());
            }
        }
    }
    out
}

fn decode(data: &[u8]) -> Vec<Stroke> {
    let mut strokes = Vec::new();
    let Some((&FORMAT, mut rest)) = data.split_first() else {
        return strokes;
    };
    while rest.len() >= 3 {
        let color = rest[0];
        let n = u16::from_le_bytes([rest[1], rest[2]]) as usize;
        rest = &rest[3..];
        if rest.len() < n * 4 {
            break;
        }
        let points = rest[..n * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                (
                    u16::from_le_bytes([p[0], p[1]]),
                    u16::from_le_bytes([p[2], p[3]]),
                )
            })
            .collect();
        strokes.push(Stroke { color, points });
        rest = &rest[n * 4..];
    }
    strokes
}

struct Paint {
    width: i32,
    height: i32,
    /// 選んでいる色（PALETTE の添字）
    color: u8,
    /// 描き終えた線
    strokes: Vec<Stroke>,
    /// 描いている途中の線（指ごと）
    active: HashMap<u8, Stroke>,
    clear: Button,
    back: Button,
}

impl Paint {
    fn canvas(&self) -> Rect {
        rect(0, TOOLBAR, self.width, self.height - TOOLBAR - PALETTE_H)
    }

    fn palette_bar(&self) -> Rect {
        rect(0, self.height - PALETTE_H, self.width, PALETTE_H)
    }

    fn swatch_center(&self, i: usize) -> (i32, i32) {
        let step = self.width / PALETTE.len() as i32;
        (step * i as i32 + step / 2, self.height - PALETTE_H / 2)
    }

    fn chrome(&self, f: Frame) -> Frame {
        f.fill_rect(rect(0, 0, self.width, TOOLBAR), color::SURFACE, 0)
            .text(24, 28, 32, color::FG, "Paint")
            .with(|f| self.clear.draw(f))
            .with(|f| self.back.draw(f))
    }

    fn palette(&self, mut f: Frame) -> Frame {
        f = f.fill_rect(self.palette_bar(), color::SURFACE, 0);
        for (i, &c) in PALETTE.iter().enumerate() {
            let (x, y) = self.swatch_center(i);
            // 選んでいる色は白い輪。暗い色も見えるよう全部に細い縁
            let ring = if i == self.color as usize { 40 } else { 34 };
            let edge = if i == self.color as usize {
                color::WHITE
            } else {
                color::BORDER
            };
            f = f.circle(x, y, ring, edge).circle(x, y, 32, c);
        }
        f
    }

    /// 全部描き直す（起動時と Clear）
    fn redraw(&self) {
        let mut f = Frame::new()
            .clear(color::WHITE)
            .with(|f| self.chrome(f))
            .with(|f| self.palette(f))
            .push_clip(self.canvas());
        for s in &self.strokes {
            f = s.draw(f);
        }
        f.pop_clip().submit();
    }

    fn save(&self) {
        let data = if self.strokes.is_empty() {
            Vec::new()
        } else {
            encode(&self.strokes)
        };
        if let Err(e) = storage::set(KEY, &data) {
            log(format!("save: {e}"));
        }
    }

    /// パレットの上の x に一番近い色
    fn pick(&self, x: i32) -> u8 {
        let step = self.width / PALETTE.len() as i32;
        (x / step).clamp(0, PALETTE.len() as i32 - 1) as u8
    }
}

fn point(x: i32, y: i32) -> (u16, u16) {
    (x.max(0) as u16, y.max(0) as u16)
}

impl App for Paint {
    fn new(_args: &str) -> Self {
        let (w, h) = screen_size();
        let (w, h) = (w as i32, h as i32);
        let app = Paint {
            width: w,
            height: h,
            color: 0,
            strokes: storage::get(KEY).map(|d| decode(&d)).unwrap_or_default(),
            active: HashMap::new(),
            clear: Button::new(rect(w - 380, 14, 160, 60), "Clear"),
            back: Button::new(rect(w - 200, 14, 176, 60), "← Home"),
        };
        app.redraw();
        app
    }

    fn on_event(&mut self, event: Event) {
        let Event::Touch(touches) = event else { return };

        let pressed = (self.clear.pressed(), self.back.pressed());
        if self.back.handle(&touches) {
            system::exit();
            return;
        }
        if self.clear.handle(&touches) {
            self.strokes.clear();
            self.active.clear();
            self.save();
            self.redraw();
            return;
        }

        let canvas = self.canvas();
        let mut f = Frame::new().push_clip(canvas);
        let mut picked = false;
        let mut finished = false;
        for t in &touches {
            match t.phase {
                TouchPhase::Down if self.palette_bar().contains(t.x, t.y) => {
                    let c = self.pick(t.x);
                    picked |= c != self.color;
                    self.color = c;
                }
                TouchPhase::Down if canvas.contains(t.x, t.y) => {
                    let s = Stroke {
                        color: self.color,
                        points: vec![point(t.x, t.y)],
                    };
                    f = f.circle(t.x, t.y, PEN / 2, s.color());
                    self.active.insert(t.id, s);
                }
                TouchPhase::Move => {
                    if let Some(s) = self.active.get_mut(&t.id) {
                        let (x0, y0) = *s.points.last().unwrap();
                        s.points.push(point(t.x, t.y));
                        let c = s.color();
                        f = f.line(x0 as i32, y0 as i32, t.x, t.y, c, PEN).circle(
                            t.x,
                            t.y,
                            PEN / 2,
                            c,
                        );
                    }
                }
                TouchPhase::Up | TouchPhase::Cancel => {
                    if let Some(s) = self.active.remove(&t.id) {
                        self.strokes.push(s);
                        finished = true;
                    }
                }
                _ => {}
            }
        }
        // ツールバーとパレットは変わったときだけ描き直す。毎回描くと描いた範囲が
        // 画面の上下いっぱいになり、パネルへの転送が重くなる
        f = f.pop_clip();
        if pressed != (self.clear.pressed(), self.back.pressed()) {
            f = f.with(|f| self.chrome(f));
        }
        if picked {
            f = f.with(|f| self.palette(f));
        }
        f.submit();
        if finished {
            self.save();
        }
    }
}

wasmos_sdk::app!(Paint);
