//! m5-wasmos アプリ用 SDK。
//!
//! ```ignore
//! use wasmos_sdk::{App, Event, gfx::Frame};
//!
//! struct Hello;
//! impl App for Hello {
//!     fn new(_args: &str) -> Self {
//!         Frame::new().clear(0xff000000).text(640, 340, 48, 0xffffffff, "Hello").submit();
//!         Hello
//!     }
//!     fn on_event(&mut self, _event: Event) {}
//! }
//! wasmos_sdk::app!(Hello);
//! ```
//!
//! ホストの API（WIT: `wit/wasmos.wit`）は小さく保ち、便利な部品はここに置く。
//! ここを変えてもファームウェアの焼き直しは要らない。

pub mod bindings {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "wasmos:os/app",
        generate_all,
        pub_export_macro: true,
        export_macro_name: "__wasmos_export",
        default_bindings_module: "wasmos_sdk::bindings",
    });
}

pub mod color;
pub mod gfx;
pub mod http;
pub mod power;
pub mod ui;

use std::cell::RefCell;

pub use bindings::wasmos::os::system::{self, AppInfo};
pub use bindings::wasmos::os::types::{Event, Rect, Touch, TouchPhase};

/// アプリ本体。`app!` マクロでエクスポートする。
pub trait App {
    /// 起動時に 1 回。`args` は `system::launch` に渡された文字列。
    fn new(args: &str) -> Self
    where
        Self: Sized;
    /// タッチ、フレーム、タイマー、HTTP の進捗などで呼ばれる。ブロックしないこと。
    fn on_event(&mut self, event: Event);
}

thread_local! {
    static APP: RefCell<Option<Box<dyn App>>> = const { RefCell::new(None) };
}

#[doc(hidden)]
pub fn __start<A: App + 'static>(args: &str) {
    std::panic::set_hook(Box::new(|info| system::log(&format!("panic: {info}"))));
    let app = A::new(args);
    APP.with(|cell| *cell.borrow_mut() = Some(Box::new(app)));
}

#[doc(hidden)]
pub fn __dispatch(event: Event) {
    APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.on_event(event);
        }
    });
}

/// `App` を実装した型をコンポーネントのエクスポートにする。
#[macro_export]
macro_rules! app {
    ($ty:ty) => {
        struct __WasmosApp;
        impl $crate::bindings::Guest for __WasmosApp {
            fn init(args: ::std::string::String) {
                $crate::__start::<$ty>(&args);
            }
            fn on_event(event: $crate::Event) {
                $crate::__dispatch(event);
            }
        }
        $crate::bindings::__wasmos_export!(__WasmosApp with_types_in $crate::bindings);
    };
}

/// ログ（実機ではシリアル、シミュレータでは console）
pub fn log(message: impl AsRef<str>) {
    system::log(message.as_ref());
}

/// 画面サイズ
pub fn screen_size() -> (u32, u32) {
    bindings::wasmos::os::gfx::screen_size()
}
