//! ランチャー。インストール済みアプリをタイルで並べ、タップで起動する。

use wasmos_sdk::color::{self, with_alpha};
use wasmos_sdk::gfx::{rect, Frame, TextAlign};
use wasmos_sdk::ui::Scroll;
use wasmos_sdk::{screen_size, system, App, AppInfo, Event, Rect, TouchPhase};

const HEADER: i32 = 120;
const TILE: i32 = 220;
const GAP: i32 = 40;

struct Launcher {
    width: i32,
    height: i32,
    apps: Vec<AppInfo>,
    scroll: Scroll,
    pressed: Option<usize>,
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

        f.pop_clip()
            .fill_rect(rect(0, 0, self.width, HEADER), color::SURFACE, 0)
            .text(48, 36, 48, color::FG, "m5-wasmos")
            .text_aligned(
                self.width - 48,
                52,
                24,
                color::MUTED,
                TextAlign::Right,
                format!("{} apps", self.apps.len()),
            )
            .submit();
    }
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
        };
        let rows = (app.apps.len() as i32 + app.columns() - 1) / app.columns();
        app.scroll.max = HEADER + GAP + rows * (TILE + 56 + GAP) - app.height;
        app.draw();
        app
    }

    fn on_event(&mut self, event: Event) {
        let Event::Touch(touches) = event else { return };
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
