//! ランチャー。インストール済みアプリをタイルで並べ、タップで起動する。
//! 右上に電池の残量と充電・放電の電力を出す（電池が読めないときは出さない）。

use wasmos_sdk::color::{self, with_alpha};
use wasmos_sdk::gfx::{measure_text, rect, Frame, TextAlign};
use wasmos_sdk::power::{self, Battery, Flow};
use wasmos_sdk::ui::Scroll;
use wasmos_sdk::{screen_size, system, App, AppInfo, Event, Rect, TouchPhase};

const HEADER: i32 = 120;
const TILE: i32 = 220;
const GAP: i32 = 40;
/// 電池を読み直す間隔
const BATTERY_MS: u32 = 5000;

struct Launcher {
    width: i32,
    height: i32,
    apps: Vec<AppInfo>,
    scroll: Scroll,
    pressed: Option<usize>,
    battery: Option<Battery>,
    battery_timer: u32,
}

impl Launcher {
    fn columns(&self) -> i32 {
        ((self.width - GAP) / (TILE + GAP)).max(1)
    }

    fn tile_rect(&self, i: usize) -> Rect {
        let cols = self.columns();
        let used = cols * TILE + (cols - 1) * GAP;
        let left = (self.width - used) / 2;
        let (col, row) = (i as i32 % cols, i as i32 / cols);
        rect(
            left + col * (TILE + GAP),
            HEADER + GAP + row * (TILE + 56 + GAP) - self.scroll.offset,
            TILE,
            TILE,
        )
    }

    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        if y < HEADER {
            return None;
        }
        (0..self.apps.len()).find(|&i| self.tile_rect(i).contains(x, y))
    }

    fn draw(&self) {
        let mut f = Frame::new().clear(color::BG).push_clip(rect(
            0,
            HEADER,
            self.width,
            self.height - HEADER,
        ));

        for (i, app) in self.apps.iter().enumerate() {
            let r = self.tile_rect(i);
            let c = if self.pressed == Some(i) {
                with_alpha(app.color, 0xb0)
            } else {
                app.color
            };
            let initial: String = app.name.chars().take(1).collect();
            f = f
                .fill_rect(r, c, 36)
                .text_centered(r, 120, color::WHITE, initial)
                .text_aligned(
                    r.x + TILE / 2,
                    r.y + TILE + 12,
                    28,
                    color::FG,
                    TextAlign::Center,
                    app.name.clone(),
                );
        }
        if self.apps.is_empty() {
            f = f.text_aligned(
                self.width / 2,
                self.height / 2,
                32,
                color::MUTED,
                TextAlign::Center,
                "アプリがありません。本体の更新ボタンで取得してください",
            );
        }

        let title_w = measure_text("m5-wasmos", 48) as i32;
        f.pop_clip()
            .fill_rect(rect(0, 0, self.width, HEADER), color::SURFACE, 0)
            .text(48, 36, 48, color::FG, "m5-wasmos")
            .text(
                48 + title_w + 24,
                56,
                24,
                color::MUTED,
                format!("{} apps", self.apps.len()),
            )
            .with(|f| match &self.battery {
                Some(b) => battery_status(f, self.width - 48, b),
                None => f,
            })
            .submit();
    }
}

/// 右端 `right` に、電池アイコン + 残量 % と、その下に電力
fn battery_status(f: Frame, right: i32, b: &Battery) -> Frame {
    let level = b.level();
    let flow = b.flow();
    let fill = match (flow, level) {
        (Flow::Charging, _) => color::GREEN,
        (_, 0..=9) => color::RED,
        (_, 10..=19) => color::YELLOW,
        _ => color::FG,
    };
    // 本体 60x30 + 右の端子 6x14
    let body = rect(right - 66, 24, 60, 30);
    let inner = body.inset(5);
    let watts = b.power_mw().unsigned_abs();
    let power = format!("{}.{}W", watts / 1000, watts % 1000 / 100);
    let detail = match flow {
        Flow::Charging => format!("充電中 {power}"),
        Flow::Discharging => format!("放電中 {power}"),
        Flow::Idle => "待機".into(),
    };
    f.stroke_rect(body, color::MUTED, 6, 3)
        .fill_rect(rect(right - 6, 32, 6, 14), color::MUTED, 2)
        .fill_rect(
            rect(inner.x, inner.y, inner.w * level as i32 / 100, inner.h),
            fill,
            3,
        )
        .text_aligned(
            body.x - 14,
            22,
            30,
            color::FG,
            TextAlign::Right,
            format!("{level}%"),
        )
        .text_aligned(right, 70, 22, color::MUTED, TextAlign::Right, detail)
}

impl App for Launcher {
    fn new(_args: &str) -> Self {
        let (w, h) = screen_size();
        let apps = system::installed();
        let mut app = Launcher {
            width: w as i32,
            height: h as i32,
            apps,
            scroll: Scroll::default(),
            pressed: None,
            battery: power::battery(),
            battery_timer: system::set_timer(BATTERY_MS),
        };
        let rows = (app.apps.len() as i32 + app.columns() - 1) / app.columns();
        app.scroll.max = HEADER + GAP + rows * (TILE + 56 + GAP) - app.height;
        app.draw();
        app
    }

    fn on_event(&mut self, event: Event) {
        let touches = match event {
            Event::Touch(t) => t,
            Event::Timer(id) if id == self.battery_timer => {
                self.battery_timer = system::set_timer(BATTERY_MS);
                let b = power::battery();
                let key = |b: &Option<Battery>| b.as_ref().map(|b| (b.level(), b.power_mw() / 100));
                if key(&b) != key(&self.battery) {
                    self.battery = b;
                    self.draw();
                }
                return;
            }
            _ => return,
        };
        self.scroll.handle(&touches);

        for t in &touches {
            match t.phase {
                TouchPhase::Down => self.pressed = self.hit(t.x, t.y),
                TouchPhase::Up => {
                    if let Some(i) = self.pressed.take() {
                        if self.scroll.moved < 16 && self.hit(t.x, t.y) == Some(i) {
                            system::launch(&self.apps[i].id, "");
                        }
                    }
                }
                TouchPhase::Cancel => self.pressed = None,
                TouchPhase::Move => {
                    if self.scroll.moved >= 16 {
                        self.pressed = None;
                    }
                }
            }
        }
        self.draw();
    }
}

wasmos_sdk::app!(Launcher);
