# m5-wasmos

M5Stack Tab5（ESP32-P4）用の小さな OS。アプリは全部 **wasm コンポーネント**で、
ランチャーもその 1 つ。ファームウェアを焼き直さずにアプリを差し替えられる。

- サイト / シミュレータ: https://kuboon.github.io/m5-wasmos/
- 実機への書き込み（Chrome / Edge）: https://kuboon.github.io/m5-wasmos/flash
- アプリ一覧（実機が読む）:
  https://kuboon.github.io/m5-wasmos/apps/manifest.json

## 構成

```
wit/            ホスト API（wasmos:os@0.1.0）＋ wasi:http@0.2.3 の依存
crates/sdk/     Rust 用 SDK（wit-bindgen、描画ビルダー、非ブロッキング HTTP、UI 部品）
apps/*/         アプリ（launcher / paint / weather）
scripts/        build_apps.ts — wasm コンポーネント化、jco 変換、manifest 生成
web/            サイト（Remix v3 + @remix-kbn/ssg、kuboon/deno-remix-tmpl の static モード）
  client/sim/   ブラウザ版ホスト（canvas / pointer events / fetch で WIT を実装）
firmware/       実機ファームウェア（ESP-IDF + wasmtime/Pulley）。firmware/README.md
tools/          precompile（実機用 .cwasm を作る）、make_font.py
```

## ホスト API の考え方（`wit/wasmos.wit`）

ファームを焼き直さないと変えられないのはこの WIT だけなので、小さく保つ。

- **描画**: プリミティブ（矩形・角丸・線・円・テキスト・画像・クリップ）の
  コマンドリストを 1 フレーム分まとめて
  `gfx.submit`。テキストはホストのフォントで描く。 ウィジェットは SDK 側。
- **イベント駆動**: ホストが `init(args)` を 1 回、以後 `on-event(event)`
  を呼ぶ。 event は `touch`（生のタッチ点、最大 5 点）/ `frame` / `timer` / `io`
  / `image-ready`。 コールバック内でブロックしない。
- **HTTP**: `wasi:http/outgoing-handler`。`future.get()`
  をポーリングし、ホストは 進捗があれば `io` イベントを送る（SDK の
  `http::Fetch`）。TLS はホストが担当。
- **アプリ起動**: `system.installed` / `launch` / `exit`。前面は常に 1 アプリ。
- **ファイルシステムは無し**。ハードウェアボタンはアプリから使えない（更新・リセット専用）。
- 画面上端から下スワイプはシステムジェスチャ（ランチャーへ戻る）。

## 開発

必要なもの: Rust（`rust-toolchain.toml` が `wasm32-unknown-unknown`
を入れる）、Deno 2。

```bash
deno task build:apps   # apps → web/build/apps（.wasm, シミュレータ用 JS, manifest.json）
deno task dev          # build:apps してから http://localhost:8000/sim
deno task build        # GitHub Pages 用に web/dist へ
deno task check        # 型チェック + lint + fmt
```

アプリを足すには `apps/<id>/` に cdylib クレートを作り、`Cargo.toml` に

```toml
[package.metadata.wasmos]
name = "表示名"
description = "説明"
color = "#0ea5e9"
```

を書く。`wasmos_sdk::App` を実装して `wasmos_sdk::app!(MyApp);`。

## デプロイ

`.github/workflows/pages.yml` が `kuboon/workflows` の
`github-page-with-preview` を呼ぶ（`mise run build` → `web/dist`）。main は
Pages の ルート、PR はプレビューのサブパス。manifest の URL は manifest
からの相対なので プレビューでもそのまま動く。

## ライセンス

MIT
