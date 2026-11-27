//! 実機側の Rust。C の app_main が初期化を済ませてから [`wasmos_main`] を呼ぶ。
//!
//! メインループ: タッチを読む → イベント配送 → tick → アプリ切り替え → 必要なら更新。
#![no_std]
extern crate alloc;

mod hw;
mod input;
mod platform;
mod screen;
mod update;

use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};

use wasmos_host::{AppMeta, Runtime};

use crate::platform::EspPlatform;

static FONT: &[u8] = include_bytes!("../../../host/assets/NotoSansJP-Regular-jis.ttf");
/// 保存領域にアプリが無いときのランチャー（ビルド時に web/build/apps から埋め込む。無ければ空）
static FALLBACK_LAUNCHER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/launcher.cwasm"));

// --- ランタイム支援 -------------------------------------------------------------

struct PsramAlloc;

unsafe impl GlobalAlloc for PsramAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        hw::wasmos_hw_alloc(l.size(), l.align())
    }
    unsafe fn dealloc(&self, p: *mut u8, _l: Layout) {
        hw::wasmos_hw_free(p)
    }
}

#[global_allocator]
static ALLOC: PsramAlloc = PsramAlloc;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    hw::log(&alloc::format!("rust panic: {info}"));
    loop {
        hw::delay_ms(1000);
    }
}

// wasmtime（no_std）が求めるスレッドローカル。wasm を動かすのは 1 タスクだけ。
// wasmtime 51 から slot 引数付き（0 = ランタイム、1 = component-model-async）
static mut TLS: [*mut u8; 2] = [core::ptr::null_mut(); 2];

#[no_mangle]
extern "C" fn wasmtime_tls_get(slot: usize) -> *mut u8 {
    unsafe { TLS[slot] }
}

#[no_mangle]
extern "C" fn wasmtime_tls_set(slot: usize, p: *mut u8) {
    unsafe { TLS[slot] = p }
}

// --- メイン -------------------------------------------------------------------

pub(crate) const LAUNCHER: &str = "launcher";
/// この間タッチが無ければバックライトを消す（スリープ）
const SLEEP_AFTER_MS: u64 = 60_000;

fn load_installed() -> Vec<AppMeta> {
    update::local_manifest()
        .map(|m| m.apps.iter().map(|a| a.meta()).collect())
        .unwrap_or_default()
}

fn load_cwasm(id: &str) -> Option<Vec<u8>> {
    hw::file_read(&alloc::format!("{id}.cwasm"))
}

fn start(rt: &mut Runtime<EspPlatform>, id: &str, args: &str) {
    let cwasm = match load_cwasm(id) {
        Some(c) => c,
        None if id == LAUNCHER && !FALLBACK_LAUNCHER.is_empty() => FALLBACK_LAUNCHER.to_vec(),
        None => {
            hw::log(&alloc::format!("no such app: {id}"));
            if id != LAUNCHER {
                start(rt, LAUNCHER, "");
            } else {
                screen::message(
                    rt,
                    "ランチャーがありません",
                    "Wi-Fi を設定して update してください",
                );
            }
            return;
        }
    };
    let t = hw::now_ms();
    match rt.start(id, &cwasm, args) {
        Ok(()) => hw::log(&alloc::format!("started {id} in {} ms", hw::now_ms() - t)),
        Err(e) => {
            hw::log(&alloc::format!("start {id} failed: {e:?}"));
            if id != LAUNCHER {
                start(rt, LAUNCHER, "");
            } else {
                screen::message(rt, "ランチャーを起動できません", &alloc::format!("{e}"));
            }
        }
    }
}

fn run_update(rt: &mut Runtime<EspPlatform>) {
    rt.stop();
    rt.platform().flush_storage();
    let result = update::run(rt);
    let (title, detail) = match &result {
        Ok(n) => ("更新しました", alloc::format!("{n} 個のアプリを取得")),
        Err(e) => ("更新できませんでした", e.clone()),
    };
    hw::log(&alloc::format!("update: {title} {detail}"));
    screen::message(rt, title, &detail);
    hw::delay_ms(1500);
    rt.set_installed(load_installed());
    start(rt, LAUNCHER, "");
}

#[no_mangle]
pub extern "C" fn wasmos_main() {
    hw::log("wasmos: rust main");
    let mut rt = match Runtime::new(EspPlatform::new(), FONT, LAUNCHER) {
        Ok(rt) => rt,
        Err(e) => {
            hw::log(&alloc::format!("runtime init failed: {e:?}"));
            loop {
                hw::delay_ms(1000);
            }
        }
    };
    screen::message(&mut rt, "m5-wasmos", "起動中…");
    rt.set_installed(load_installed());
    start(&mut rt, LAUNCHER, "");

    // 起動後、最初にネットにつながったら 1 回だけ自動で更新を確認する
    let mut auto_update_done = false;
    let mut input = input::Tracker::new();
    let mut points = Vec::new();
    let mut last_status = String::new();
    let mut last_touch = hw::now_ms();
    // スリープ中（バックライトを消している）
    let mut asleep = false;
    // 起こしたタッチは指を離すまでアプリに渡さない（うっかりタップしないように）
    let mut waking = false;

    loop {
        hw::touch(&mut points);
        let now = hw::now_ms();
        if !points.is_empty() {
            last_touch = now;
            if asleep {
                hw::backlight(true);
                asleep = false;
                waking = true;
                hw::log("wake");
            }
        } else {
            waking = false;
            if now - last_touch > 1000 {
                // storage はタッチが 1 秒止んでからフラッシュに書く（描画を止めないように）
                rt.platform().flush_storage();
            }
            if !asleep && now - last_touch > SLEEP_AFTER_MS {
                hw::backlight(false);
                asleep = true;
                hw::log("sleep");
            }
        }
        if waking {
            points.clear();
        }
        let (events, home) = input.update(&points);
        if home {
            rt.go_home();
        }
        for ev in events {
            let _ = rt.dispatch(ev);
        }
        let _ = rt.tick();
        while let Some((id, args)) = rt.take_switch() {
            start(&mut rt, &id, &args);
        }

        let (online, status) = hw::net_status();
        if status != last_status {
            hw::log(&alloc::format!("net: {status}"));
            last_status = status;
        }
        // シリアルの update コマンド、アプリ（ランチャーの更新ボタン）、起動後の自動確認
        let asked = hw::take_update_request() | rt.take_update_request();
        if asked || (online && !auto_update_done) {
            auto_update_done = true;
            if online {
                run_update(&mut rt);
            } else {
                hw::log("update: offline");
            }
        }
        // スリープ中はタッチの読み取りを間引く（アプリのタイマーなどは動かし続ける）
        hw::delay_ms(if asleep { 50 } else { 8 });
    }
}
