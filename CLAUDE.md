# m5-wasmos

M5Stack Tab5 用の wasm アプリ OS。概要は README.md。

## 方針

- `wit/wasmos.wit` は実機ファームの焼き直しが要る唯一の境界。足す前に SDK
  （`crates/sdk`）で済まないか考える。変えたら `scripts/build_apps.ts` の
  `ABI`、 `web/client/sim/host.ts`、（いずれ）firmware を揃える
- アプリはイベント駆動でブロックしない。`wasi:io/poll.block` / `poll` は
  シミュレータでは使えない
- アプリは `wasm32-unknown-unknown` + wit-bindgen（wasip2 の std は wasi:cli や
  filesystem を import してしまうので使わない）

## ファームウェア（firmware/）

- `firmware/host` はハード非依存（no_std）。変更は `firmware/sim` で Linux
  上で確かめられる （`wasmos-sim web/build/apps firmware/sim/smoke.txt` → PNG）
- wasmtime の版や `host/src/engine.rs` の設定を変えたら `ENGINE_ID` を上げる
  （サイトの .cwasm と実機のエンジンが一致しないと読めない）
- ESP-IDF のビルドは CI のみ（このサンドボックスから Espressif
  のサーバに出られない）
- C と Rust の境界は `firmware/esp/main/wasmos_hw.h` と
  `firmware/esp/rust/src/hw.rs`。両方揃える

## サイト（web/）

kuboon/deno-remix-tmpl の static モード（SSG → GitHub Pages）を縮めたもの。

- `web/server/router.tsx` が唯一のルーター。`deno serve` が開発サーバ、
  `@remix-kbn/ssg` が同じ router をクロールして `web/dist` に書き出す
- `web/build/apps`（`deno task build:apps` の出力、git 管理外）を
  `${base}/apps/*` で配信し、全ファイルを `entryPoints` に入れている
- ブラウザへ渡るものは `web/client/`（`Deno.` を参照しない）
- 見た目は `@remix-run/component` の `css()` と `web/client/tokens.ts`
- 静的ビルドのリンクに `?query` や `#hash`
  を付けない（クローラがファイル名にしてしまう）。 アプリ別ページは `/sim/:app`

## 確認

```bash
cargo fmt --check && cargo clippy --release --target wasm32-unknown-unknown -- -D warnings
deno task build && deno task check
```
