//! m5-wasmos のホスト（ハード非依存部分）。
//!
//! wasmtime（Pulley インタプリタ）で wasm コンポーネントを動かし、`wit/wasmos.wit` の
//! import を実装する。画面・HTTP・時計などハードに触る部分は [`Platform`] として外から渡す。
//! 実機（firmware/esp）と Linux 上のテスト（firmware/sim）が同じコードを使う。
#![no_std]
extern crate alloc;

pub mod bindings;
pub mod engine;
pub mod http;
mod imports;
pub mod platform;
pub mod render;
pub mod runtime;

pub use platform::{Battery, HttpRequest, HttpResponse, IRect, Platform};
pub use runtime::{AppMeta, Runtime};
