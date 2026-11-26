//! ホスト自身が描く画面（起動中・更新中・エラー）
use wasmos_host::bindings::wasmos::os::gfx::{Cmd, TextAlign, TextCmd};
use wasmos_host::Runtime;

use crate::platform::EspPlatform;

fn text(x: i32, y: i32, size: u16, color: u32, s: &str) -> Cmd {
    Cmd::Text(TextCmd {
        x,
        y,
        size,
        color,
        align: TextAlign::Center,
        text: s.into(),
    })
}

pub fn message(rt: &mut Runtime<EspPlatform>, title: &str, detail: &str) {
    let (online, net) = crate::hw::net_status();
    let net_color = if online { 0xff22_c55e } else { 0xff9c_a3af };
    rt.system_frame(&[
        Cmd::Clear(0xff11_1827),
        text(640, 250, 56, 0xffe5_e7eb, title),
        text(640, 340, 28, 0xff9c_a3af, detail),
        text(640, 640, 22, net_color, &net),
    ]);
}
