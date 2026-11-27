//! アプリごとの小さな保存領域。電源を切っても残る。
//!
//! ホームへ戻るとアプリは予告なく終わるので、残したいものは変わるたびに `set` する。
//! 実機ではホストがメモリに置き、タッチが止んでからフラッシュに書くので、頻繁に呼んでよい。
//! キーは英小文字・数字・`_`・`-` で 1〜16 文字、値は 256KiB まで。

use crate::bindings::wasmos::os::storage as host;

/// 保存した値（無ければ None）
pub fn get(key: &str) -> Option<Vec<u8>> {
    host::get(key)
}

/// 保存する（空の値は削除）
pub fn set(key: &str, value: &[u8]) -> Result<(), String> {
    host::set(key, value)
}
