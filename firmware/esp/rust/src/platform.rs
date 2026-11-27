//! [`Platform`] の実機実装
use alloc::string::String;
use alloc::vec::Vec;

use wasmos_host::{Battery, HttpRequest, HttpResponse, IRect, Platform};

use crate::hw;

/// 論理画面（縦 720x1280）と物理パネル（縦 720x1280）は同じ向き。
/// 上下逆に映ったら true にする（タッチも一緒に反転する）
pub const ROTATE_180: bool = false;
pub const PANEL_W: i32 = 720;
pub const PANEL_H: i32 = 1280;

/// 物理 (px, py) → 論理 (x, y)。180 度回転は自分自身が逆なので、論理 → 物理にも使える
pub fn from_panel(px: i32, py: i32) -> (i32, i32) {
    if ROTATE_180 {
        (PANEL_W - 1 - px, PANEL_H - 1 - py)
    } else {
        (px, py)
    }
}

fn rgb565(px: &[u8]) -> u16 {
    let (r, g, b) = (px[0] as u16, px[1] as u16, px[2] as u16);
    ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)
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
        // dirty 矩形を行ごとに RGB565 に詰め替える（論理と物理は同じ向きか 180 度）
        let (w, h) = (d.w as usize, d.h as usize);
        self.buf.resize(w * h, 0);
        let stride = width as usize * 4;
        for row in 0..h {
            let start = (d.y as usize + row) * stride + d.x as usize * 4;
            let src = rgba[start..start + w * 4].chunks_exact(4);
            if ROTATE_180 {
                let dst = &mut self.buf[(h - 1 - row) * w..][..w];
                for (o, px) in dst.iter_mut().rev().zip(src) {
                    *o = rgb565(px);
                }
            } else {
                let dst = &mut self.buf[row * w..][..w];
                for (o, px) in dst.iter_mut().zip(src) {
                    *o = rgb565(px);
                }
            }
        }
        let (px0, py0) = if ROTATE_180 {
            (PANEL_W - d.x - d.w, PANEL_H - d.y - d.h)
        } else {
            (d.x, d.y)
        };
        hw::draw(px0, py0, px0 + d.w, py0 + d.h, &self.buf);
    }
}
