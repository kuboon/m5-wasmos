//! 指でお絵かき。指ごとに色が変わる（マルチタッチ）。

use std::collections::HashMap;

use wasmos_sdk::color::{self, rgb, Color};
use wasmos_sdk::gfx::{rect, Frame};
use wasmos_sdk::ui::Button;
use wasmos_sdk::{screen_size, system, App, Event, TouchPhase};

const PALETTE: [Color; 6] = [
    rgb(0xf9, 0x73, 0x16),
    rgb(0x22, 0xc5, 0x5e),
    rgb(0x3b, 0x82, 0xf6),
    rgb(0xec, 0x48, 0x99),
    rgb(0xea, 0xb3, 0x08),
    rgb(0xa8, 0x55, 0xf7),
];

struct Paint {
    width: i32,
    last: HashMap<u8, (i32, i32)>,
    clear: Button,
    back: Button,
}

const TOOLBAR: i32 = 88;

impl Paint {
    fn chrome(&self, f: Frame) -> Frame {
        f.fill_rect(rect(0, 0, self.width, TOOLBAR), color::SURFACE, 0)
            .text(24, 28, 32, color::FG, "Paint")
            .with(|f| self.clear.draw(f))
            .with(|f| self.back.draw(f))
    }

    fn redraw(&self) {
        Frame::new()
            .clear(color::WHITE)
            .with(|f| self.chrome(f))
            .submit();
    }
}

impl App for Paint {
    fn new(_args: &str) -> Self {
        let (w, _) = screen_size();
        let w = w as i32;
        let app = Paint {
            width: w,
            last: HashMap::new(),
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
            self.redraw();
            return;
        }

        let mut f = Frame::new().push_clip(rect(0, TOOLBAR, self.width, 10_000));
        for t in &touches {
            let c = PALETTE[t.id as usize % PALETTE.len()];
            match t.phase {
                TouchPhase::Down if t.y > TOOLBAR => {
                    f = f.circle(t.x, t.y, 6, c);
                    self.last.insert(t.id, (t.x, t.y));
                }
                TouchPhase::Move => {
                    if let Some((x0, y0)) = self.last.insert(t.id, (t.x, t.y)) {
                        f = f.line(x0, y0, t.x, t.y, c, 12).circle(t.x, t.y, 6, c);
                    }
                }
                TouchPhase::Up | TouchPhase::Cancel => {
                    self.last.remove(&t.id);
                }
                _ => {}
            }
        }
        // ツールバーは押下表示が変わったときだけ描き直す。毎回描くと描いた範囲が
        // 画面上端からストロークまでの全幅になり、パネルへの転送が重くなる
        f = f.pop_clip();
        if pressed != (self.clear.pressed(), self.back.pressed()) {
            f = f.with(|f| self.chrome(f));
        }
        f.submit();
    }
}

wasmos_sdk::app!(Paint);
