//! 東京都北区の天気。現在の気温と、過去 1 週間の気温（毎時）のグラフ。
//!
//! 履歴は Open-Meteo の `past_days=7` で毎回取ってくるので、端末側のストレージは要らない。

use serde_json::Value;
use wasmos_sdk::color::{self, rgb, with_alpha};
use wasmos_sdk::gfx::{rect, Frame, TextAlign};
use wasmos_sdk::http::Fetch;
use wasmos_sdk::ui::Button;
use wasmos_sdk::{screen_size, system, App, Event, Rect};

const PLACE: &str = "東京都北区";
const URL: &str = "https://api.open-meteo.com/v1/forecast\
    ?latitude=35.7528&longitude=139.7336\
    &current=temperature_2m,wind_speed_10m,weather_code\
    &hourly=temperature_2m&past_days=7&forecast_days=1\
    &timezone=Asia%2FTokyo";

const BG: u32 = rgb(0x0c, 0x4a, 0x6e);
const LINE: u32 = rgb(0xfb, 0xbf, 0x24);

/// 1 時間ぶんの気温。time は "2026-11-26T08:00"
struct Sample {
    time: String,
    temp: f64,
}

struct Current {
    temp: f64,
    wind: f64,
    code: i64,
    time: String,
}

enum Status {
    Loading,
    Ready {
        current: Current,
        history: Vec<Sample>,
    },
    Failed(String),
}

struct Weather {
    width: i32,
    height: i32,
    status: Status,
    pending: Option<Fetch>,
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

/// "2026-11-26" → (11, 26, "木")
fn date_label(date: &str) -> Option<(u32, u32, &'static str)> {
    let mut it = date.split('-').map(|s| s.parse::<i32>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    // Sakamoto の曜日計算
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yy = if m < 3 { y - 1 } else { y };
    let w = (yy + yy / 4 - yy / 100 + yy / 400 + T[(m - 1) as usize] + d).rem_euclid(7);
    let name = ["日", "月", "火", "水", "木", "金", "土"][w as usize];
    Some((m as u32, d as u32, name))
}

fn parse(body: &[u8]) -> Result<(Current, Vec<Sample>), String> {
    let v: Value = serde_json::from_slice(body).map_err(|e| format!("json: {e}"))?;
    let cur = &v["current"];
    let current = Current {
        temp: cur["temperature_2m"].as_f64().unwrap_or(f64::NAN),
        wind: cur["wind_speed_10m"].as_f64().unwrap_or(f64::NAN),
        code: cur["weather_code"].as_i64().unwrap_or(-1),
        time: cur["time"].as_str().unwrap_or("").to_string(),
    };
    let times = v["hourly"]["time"].as_array().ok_or("hourly.time がない")?;
    let temps = v["hourly"]["temperature_2m"]
        .as_array()
        .ok_or("hourly.temperature_2m がない")?;
    // forecast_days=1 で今日の残り（未来）も来るので、現在時刻までに切る
    let history = times
        .iter()
        .zip(temps)
        .filter_map(|(t, temp)| {
            Some(Sample {
                time: t.as_str()?.to_string(),
                temp: temp.as_f64()?,
            })
        })
        .filter(|s| current.time.is_empty() || s.time <= current.time)
        .collect();
    Ok((current, history))
}

impl Weather {
    fn fetch(&mut self) {
        self.status = Status::Loading;
        match Fetch::get(URL) {
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
        self.status = match result.and_then(|resp| {
            if resp.status != 200 {
                return Err(format!("HTTP {}", resp.status));
            }
            parse(&resp.body)
        }) {
            Ok((current, history)) => Status::Ready { current, history },
            Err(e) => Status::Failed(e),
        };
    }

    fn draw(&self) {
        let (w, h) = (self.width, self.height);
        let mut f = Frame::new()
            .clear(BG)
            .text(48, 40, 48, color::WHITE, PLACE)
            .with(|f| self.back.draw(f))
            .with(|f| self.reload.draw(f));

        f = match &self.status {
            Status::Loading => {
                f.text_aligned(w / 2, h / 2, 40, color::FG, TextAlign::Center, "取得中…")
            }
            Status::Failed(e) => f
                .text_aligned(
                    w / 2,
                    h / 2 - 40,
                    40,
                    color::RED,
                    TextAlign::Center,
                    "取得に失敗しました",
                )
                .text_aligned(
                    w / 2,
                    h / 2 + 20,
                    24,
                    color::FG,
                    TextAlign::Center,
                    e.clone(),
                ),
            Status::Ready { current, history } => {
                let f = draw_current(f, current);
                draw_chart(f, rect(400, 150, w - 448, h - 230), history)
            }
        };
        f.submit();
    }
}

fn draw_current(f: Frame, c: &Current) -> Frame {
    let cx = 200;
    let time = c.time.replace('T', " ");
    f.text_aligned(
        cx,
        190,
        120,
        color::WHITE,
        TextAlign::Center,
        format!("{:.1}°", c.temp),
    )
    .text_aligned(cx, 340, 40, color::FG, TextAlign::Center, describe(c.code))
    .text_aligned(
        cx,
        400,
        26,
        color::FG,
        TextAlign::Center,
        format!("風速 {:.1} km/h", c.wind),
    )
    .text_aligned(
        cx,
        640,
        20,
        color::MUTED,
        TextAlign::Center,
        format!("{time} / Open-Meteo"),
    )
}

/// 毎時の気温を折れ線で。日の境目に縦線と日付、5℃ ごとに横線
fn draw_chart(mut f: Frame, area: Rect, samples: &[Sample]) -> Frame {
    f = f.fill_rect(area, with_alpha(color::BLACK, 0x40), 16);
    if samples.len() < 2 {
        return f.text_centered(area, 28, color::MUTED, "データがありません");
    }

    let (lo, hi) = samples.iter().fold((f64::MAX, f64::MIN), |(lo, hi), s| {
        (lo.min(s.temp), hi.max(s.temp))
    });
    // 目盛りは 5℃ 刻みに丸める
    let lo_t = (lo / 5.0).floor() * 5.0;
    let hi_t = ((hi / 5.0).ceil() * 5.0).max(lo_t + 5.0);

    let plot = rect(area.x + 64, area.y + 52, area.w - 88, area.h - 100);
    let n = samples.len() as f64 - 1.0;
    let x_of = |i: usize| plot.x + (i as f64 / n * plot.w as f64).round() as i32;
    let y_of = |t: f64| plot.y + ((hi_t - t) / (hi_t - lo_t) * plot.h as f64).round() as i32;

    // 横の目盛り
    let mut t = lo_t;
    while t <= hi_t + 0.01 {
        let y = y_of(t);
        f = f
            .line(
                plot.x,
                y,
                plot.x + plot.w,
                y,
                with_alpha(color::WHITE, 0x30),
                1,
            )
            .text_aligned(
                plot.x - 12,
                y - 11,
                22,
                color::MUTED,
                TextAlign::Right,
                format!("{t:.0}°"),
            );
        t += 5.0;
    }

    // 日の境目（0 時）と日付ラベル（その日の正午の位置）
    for (i, s) in samples.iter().enumerate() {
        let (date, hour) = s.time.split_once('T').unwrap_or((&s.time, ""));
        if hour == "00:00" && i > 0 {
            let x = x_of(i);
            f = f.line(
                x,
                plot.y,
                x,
                plot.y + plot.h,
                with_alpha(color::WHITE, 0x30),
                1,
            );
        }
        if hour == "12:00" {
            if let Some((m, d, wd)) = date_label(date) {
                f = f.text_aligned(
                    x_of(i),
                    plot.y + plot.h + 14,
                    20,
                    color::FG,
                    TextAlign::Center,
                    format!("{m}/{d}({wd})"),
                );
            }
        }
    }

    // 折れ線
    for i in 1..samples.len() {
        f = f.line(
            x_of(i - 1),
            y_of(samples[i - 1].temp),
            x_of(i),
            y_of(samples[i].temp),
            LINE,
            3,
        );
    }

    // 期間の最高・最低に印
    let (imax, smax) = samples
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.temp.total_cmp(&b.1.temp))
        .unwrap();
    let (imin, smin) = samples
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.temp.total_cmp(&b.1.temp))
        .unwrap();
    f.circle(x_of(imax), y_of(smax.temp), 7, color::RED)
        .text_aligned(
            x_of(imax),
            y_of(smax.temp) - 34,
            22,
            color::WHITE,
            TextAlign::Center,
            format!("{:.1}°", smax.temp),
        )
        .circle(x_of(imin), y_of(smin.temp), 7, color::ACCENT)
        .text_aligned(
            x_of(imin),
            y_of(smin.temp) + 12,
            22,
            color::WHITE,
            TextAlign::Center,
            format!("{:.1}°", smin.temp),
        )
        .text(
            area.x + 20,
            area.y + 8,
            20,
            color::MUTED,
            "過去 7 日間（毎時）",
        )
}

impl App for Weather {
    fn new(_args: &str) -> Self {
        let (w, h) = screen_size();
        let (w, h) = (w as i32, h as i32);
        let mut app = Weather {
            width: w,
            height: h,
            status: Status::Loading,
            pending: None,
            back: Button::new(rect(w - 224, 28, 176, 64), "← Home"),
            reload: Button::new(rect(w - 424, 28, 176, 64), "更新"),
        };
        app.fetch();
        app.draw();
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
            }
            _ => return,
        }
        self.draw();
    }
}

wasmos_sdk::app!(Weather);
