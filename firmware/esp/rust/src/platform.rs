//! [`Platform`] の実機実装
use alloc::string::String;
use alloc::vec::Vec;

use wasmos_host::{Battery, HttpRequest, HttpResponse, IRect, Platform};

use crate::hw;

/// 論理画面（横 1280x720）→ 物理パネル（縦 720x1280）の回転。
/// 上下逆に映ったら true にする（タッチも一緒に反転する）
pub const ROTATE_180: bool = false;
pub const PANEL_W: i32 = 720;
pub const PANEL_H: i32 = 1280;

/// 論理 (x, y) → 物理 (px, py)
pub fn to_panel(x: i32, y: i32) -> (i32, i32) {
    if ROTATE_180 {
        (y, PANEL_H - 1 - x)
    } else {
        (PANEL_W - 1 - y, x)
    }
}

/// 物理 (px, py) → 論理 (x, y)
pub fn from_panel(px: i32, py: i32) -> (i32, i32) {
    if ROTATE_180 {
        (PANEL_H - 1 - py, px)
    } else {
        (py, PANEL_W - 1 - px)
    }
}

pub struct EspPlatform {
    buf: Vec<u16>,
}

impl EspPlatform {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }
}

impl Platform for EspPlatform {
    fn log(&mut self, message: &str) {
        hw::log(message);
    }
    fn now_ms(&self) -> u64 {
        hw::now_ms()
    }
    fn wall_ms(&self) -> Option<u64> {
        hw::wall_ms()
    }
    fn battery(&mut self) -> Option<Battery> {
        // Tab5 の電池は 2 セル直列（NP-F550 互換）
        hw::battery().map(|(voltage_mv, current_ma)| Battery {
            voltage_mv,
            current_ma,
            cells: 2,
        })
    }

    fn http_start(&mut self, req: HttpRequest) -> u32 {
        let mut headers = String::new();
        for (k, v) in &req.headers {
            headers.push_str(k);
            headers.push_str(": ");
            headers.push_str(&String::from_utf8_lossy(v));
            headers.push_str("\r\n");
        }
        hw::http_start(&req.method, &req.url, &headers, &req.body)
    }
    fn http_ready(&mut self, h: u32) -> bool {
        !matches!(hw::http_poll(h), hw::HttpPoll::Pending)
    }
    fn http_take(&mut self, h: u32) -> Option<Result<HttpResponse, String>> {
        let r = match hw::http_poll(h) {
            hw::HttpPoll::Pending => return None,
            hw::HttpPoll::Done { status, body } => Ok(HttpResponse {
                status,
                headers: Vec::new(),
                body,
            }),
            hw::HttpPoll::Failed(e) => Err(e),
        };
        hw::http_free(h);
        Some(r)
    }
    fn http_cancel(&mut self, h: u32) {
        hw::http_free(h);
    }

    fn present(&mut self, rgba: &[u8], width: u32, _height: u32, d: IRect) {
        // 論理の dirty 矩形を物理の矩形にして RGB565 に詰め替える
        let (ax, ay) = to_panel(d.x, d.y);
        let (bx, by) = to_panel(d.x + d.w - 1, d.y + d.h - 1);
        let (px0, px1) = (ax.min(bx), ax.max(bx) + 1);
        let (py0, py1) = (ay.min(by), ay.max(by) + 1);
        let pw = (px1 - px0) as usize;
        let ph = (py1 - py0) as usize;
        self.buf.resize(pw * ph, 0);
        let stride = width as usize * 4;
        for py in py0..py1 {
            let row = (py - py0) as usize * pw;
            for px in px0..px1 {
                let (x, y) = from_panel(px, py);
                let i = y as usize * stride + x as usize * 4;
                let (r, g, b) = (rgba[i] as u16, rgba[i + 1] as u16, rgba[i + 2] as u16);
                self.buf[row + (px - px0) as usize] = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
            }
        }
        hw::draw(px0, py0, px1, py1, &self.buf);
    }
}
