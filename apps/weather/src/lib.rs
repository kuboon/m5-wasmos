//! wasi:http のサンプル。Open-Meteo から現在の天気を取ってくる。

use serde_json::Value;
use wasmos_sdk::color::{self, rgb};
use wasmos_sdk::gfx::{rect, Frame, TextAlign};
use wasmos_sdk::http::Fetch;
use wasmos_sdk::ui::Button;
use wasmos_sdk::{screen_size, system, App, Event};

struct City {
    name: &'static str,
    lat: f64,
    lon: f64,
}

const CITIES: [City; 4] = [
    City {
        name: "東京",
        lat: 35.68,
        lon: 139.76,
    },
    City {
        name: "大阪",
        lat: 34.69,
        lon: 135.50,
    },
    City {
        name: "札幌",
        lat: 43.06,
        lon: 141.35,
    },
    City {
        name: "那覇",
        lat: 26.21,
        lon: 127.68,
    },
];

enum Status {
    Loading,
    Ready {
        temp: f64,
        wind: f64,
        code: i64,
        time: String,
    },
    Failed(String),
}

struct Weather {
    width: i32,
    height: i32,
    city: usize,
    status: Status,
    pending: Option<Fetch>,
    tabs: Vec<Button>,
    back: Button,
    reload: Button,
}

fn describe(code: i64) -> &'static str {
    match code {
        0 => "快晴",
        1..=3 => "晴れ / くもり",
        45 | 48 => "霧",
        51..=57 => "霧雨",
        61..=67 => "雨",
        71..=77 => "雪",
        80..=82 => "にわか雨",
        85 | 86 => "にわか雪",
        95..=99 => "雷雨",
        _ => "不明",
    }
}

impl Weather {
    fn fetch(&mut self) {
        let c = &CITIES[self.city];
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,wind_speed_10m,weather_code&timezone=auto",
            c.lat, c.lon
        );
        self.status = Status::Loading;
        match Fetch::get(&url) {
            Ok(f) => self.pending = Some(f),
            Err(e) => self.status = Status::Failed(e),
        }
    }

    fn poll(&mut self) {
        let Some(req) = self.pending.as_mut() else {
            return;
        };
        let Some(result) = req.poll() else { return };
        self.pending = None;
        self.status = match result {
            Err(e) => Status::Failed(e),
            Ok(resp) if resp.status != 200 => Status::Failed(format!("HTTP {}", resp.status)),
            Ok(resp) => match serde_json::from_slice::<Value>(&resp.body) {
                Err(e) => Status::Failed(format!("json: {e}")),
                Ok(v) => {
                    let cur = &v["current"];
                    Status::Ready {
                        temp: cur["temperature_2m"].as_f64().unwrap_or(f64::NAN),
                        wind: cur["wind_speed_10m"].as_f64().unwrap_or(f64::NAN),
                        code: cur["weather_code"].as_i64().unwrap_or(-1),
                        time: cur["time"].as_str().unwrap_or("").to_string(),
                    }
                }
            },
        };
    }

    fn draw(&self) {
        let (w, h) = (self.width, self.height);
        let mut f = Frame::new()
            .clear(rgb(0x0c, 0x4a, 0x6e))
            .text(48, 40, 48, color::WHITE, "Weather")
            .with(|f| self.back.draw(f))
            .with(|f| self.reload.draw(f));
        for (i, tab) in self.tabs.iter().enumerate() {
            f = if i == self.city {
                f.fill_rect(tab.rect, color::ACCENT, 14).text_centered(
                    tab.rect,
                    28,
                    color::BG,
                    tab.label.clone(),
                )
            } else {
                tab.draw(f)
            };
        }
        let center = w / 2;
        f = match &self.status {
            Status::Loading => {
                f.text_aligned(center, h / 2, 40, color::FG, TextAlign::Center, "取得中…")
            }
            Status::Failed(e) => f
                .text_aligned(
                    center,
                    h / 2 - 40,
                    40,
                    color::RED,
                    TextAlign::Center,
                    "取得に失敗しました",
                )
                .text_aligned(
                    center,
                    h / 2 + 20,
                    24,
                    color::FG,
                    TextAlign::Center,
                    e.clone(),
                ),
            Status::Ready {
                temp,
                wind,
                code,
                time,
            } => f
                .text_aligned(
                    center,
                    260,
                    160,
                    color::WHITE,
                    TextAlign::Center,
                    format!("{temp:.1}°"),
                )
                .text_aligned(
                    center,
                    450,
                    48,
                    color::FG,
                    TextAlign::Center,
                    describe(*code),
                )
                .text_aligned(
                    center,
                    530,
                    28,
                    color::FG,
                    TextAlign::Center,
                    format!("風速 {wind:.1} km/h"),
                )
                .text_aligned(
                    center,
                    640,
                    22,
                    color::MUTED,
                    TextAlign::Center,
                    format!("{time} / Open-Meteo"),
                ),
        };
        f.submit();
    }
}

impl App for Weather {
    fn new(_args: &str) -> Self {
        let (w, h) = screen_size();
        let (w, h) = (w as i32, h as i32);
        let tabs = CITIES
            .iter()
            .enumerate()
            .map(|(i, c)| Button::new(rect(48 + i as i32 * 180, 130, 160, 64), c.name))
            .collect();
        let mut app = Weather {
            width: w,
            height: h,
            city: 0,
            status: Status::Loading,
            pending: None,
            tabs,
            back: Button::new(rect(w - 224, 28, 176, 64), "← Home"),
            reload: Button::new(rect(w - 424, 28, 176, 64), "更新"),
        };
        app.fetch();
        app.draw();
        system::log("weather: started");
        app
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Io => self.poll(),
            Event::Touch(touches) => {
                if self.back.handle(&touches) {
                    system::exit();
                    return;
                }
                if self.reload.handle(&touches) {
                    self.fetch();
                }
                for i in 0..self.tabs.len() {
                    if self.tabs[i].handle(&touches) && i != self.city {
                        self.city = i;
                        self.fetch();
                    }
                }
            }
            _ => return,
        }
        self.draw();
    }
}

wasmos_sdk::app!(Weather);
