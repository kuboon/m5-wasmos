//! C 側（firmware/esp/main/wasmos_hw.h）の関数。変えたらヘッダも揃える
#![allow(dead_code)]

use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

extern "C" {
    fn wasmos_hw_log(msg: *const u8, len: usize);
    fn wasmos_hw_now_ms() -> u64;
    fn wasmos_hw_wall_ms() -> u64;
    fn wasmos_hw_delay_ms(ms: u32);
    pub fn wasmos_hw_alloc(size: usize, align: usize) -> *mut u8;
    pub fn wasmos_hw_free(p: *mut u8);
    fn wasmos_hw_draw(x0: c_int, y0: c_int, x1: c_int, y1: c_int, px: *const u16);
    fn wasmos_hw_touch(xs: *mut u16, ys: *mut u16, max: c_int) -> c_int;
    fn wasmos_hw_http_start(
        method: *const c_char,
        url: *const c_char,
        headers: *const c_char,
        body: *const u8,
        body_len: usize,
    ) -> u32;
    fn wasmos_hw_http_poll(
        h: u32,
        status: *mut c_int,
        body: *mut *const u8,
        len: *mut usize,
        err: *mut *const c_char,
    ) -> c_int;
    fn wasmos_hw_http_free(h: u32);
    fn wasmos_hw_file_read(name: *const c_char, buf: *mut *mut u8, len: *mut usize) -> c_int;
    fn wasmos_hw_file_write(name: *const c_char, buf: *const u8, len: usize) -> c_int;
    fn wasmos_hw_net_status(buf: *mut u8, cap: usize) -> c_int;
    fn wasmos_hw_battery(mv: *mut c_int, ma: *mut c_int) -> c_int;
    fn wasmos_hw_take_update_request() -> bool;
}

fn cstr(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.bytes().filter(|b| *b != 0).collect();
    v.push(0);
    v
}

pub fn log(msg: &str) {
    unsafe { wasmos_hw_log(msg.as_ptr(), msg.len()) }
}

pub fn now_ms() -> u64 {
    unsafe { wasmos_hw_now_ms() }
}

pub fn wall_ms() -> Option<u64> {
    match unsafe { wasmos_hw_wall_ms() } {
        0 => None,
        v => Some(v),
    }
}

pub fn delay_ms(ms: u32) {
    unsafe { wasmos_hw_delay_ms(ms) }
}

pub fn draw(x0: i32, y0: i32, x1: i32, y1: i32, px: &[u16]) {
    unsafe { wasmos_hw_draw(x0, y0, x1, y1, px.as_ptr()) }
}

/// 物理座標のタッチ点
pub fn touch(out: &mut Vec<(u16, u16)>) {
    let mut xs = [0u16; 10];
    let mut ys = [0u16; 10];
    let n = unsafe { wasmos_hw_touch(xs.as_mut_ptr(), ys.as_mut_ptr(), 10) };
    out.clear();
    for i in 0..(n.max(0) as usize).min(10) {
        out.push((xs[i], ys[i]));
    }
}

pub fn http_start(method: &str, url: &str, headers: &str, body: &[u8]) -> u32 {
    let (m, u, h) = (cstr(method), cstr(url), cstr(headers));
    unsafe {
        wasmos_hw_http_start(
            m.as_ptr().cast(),
            u.as_ptr().cast(),
            h.as_ptr().cast(),
            body.as_ptr(),
            body.len(),
        )
    }
}

pub enum HttpPoll {
    Pending,
    Done { status: u16, body: Vec<u8> },
    Failed(String),
}

pub fn http_poll(h: u32) -> HttpPoll {
    let mut status: c_int = 0;
    let mut body: *const u8 = core::ptr::null();
    let mut len: usize = 0;
    let mut err: *const c_char = core::ptr::null();
    match unsafe { wasmos_hw_http_poll(h, &mut status, &mut body, &mut len, &mut err) } {
        0 => HttpPoll::Pending,
        1 => HttpPoll::Done {
            status: status as u16,
            body: if body.is_null() {
                Vec::new()
            } else {
                unsafe { core::slice::from_raw_parts(body, len) }.to_vec()
            },
        },
        _ => {
            let msg = if err.is_null() {
                String::from("http error")
            } else {
                unsafe { core::ffi::CStr::from_ptr(err) }
                    .to_string_lossy()
                    .into()
            };
            HttpPoll::Failed(msg)
        }
    }
}

pub fn http_free(h: u32) {
    unsafe { wasmos_hw_http_free(h) }
}

pub fn file_read(name: &str) -> Option<Vec<u8>> {
    let n = cstr(name);
    let mut buf: *mut u8 = core::ptr::null_mut();
    let mut len = 0usize;
    if unsafe { wasmos_hw_file_read(n.as_ptr().cast(), &mut buf, &mut len) } != 0 {
        return None;
    }
    let v = unsafe { core::slice::from_raw_parts(buf, len) }.to_vec();
    unsafe { wasmos_hw_free(buf) };
    Some(v)
}

pub fn file_write(name: &str, data: &[u8]) -> bool {
    let n = cstr(name);
    unsafe { wasmos_hw_file_write(n.as_ptr().cast(), data.as_ptr(), data.len()) == 0 }
}

/// (接続済みか, 説明文)
pub fn net_status() -> (bool, String) {
    let mut buf = [0u8; 128];
    let ok = unsafe { wasmos_hw_net_status(buf.as_mut_ptr(), buf.len()) } != 0;
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    (ok, String::from_utf8_lossy(&buf[..end]).into())
}

/// (電池電圧 mV, 電流 mA（+ が充電）)。読めなければ None
pub fn battery() -> Option<(u32, i32)> {
    let (mut mv, mut ma): (c_int, c_int) = (0, 0);
    if unsafe { wasmos_hw_battery(&mut mv, &mut ma) } == 0 {
        return None;
    }
    Some((mv.max(0) as u32, ma))
}

pub fn take_update_request() -> bool {
    unsafe { wasmos_hw_take_update_request() }
}
