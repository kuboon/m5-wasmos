# firmware（未着手）

ESP-IDF 上で wasm コンポーネントを動かすホスト。予定:

- ランタイム: wasmtime（no_std + Pulley インタプリタ, pulley32）で Component
  Model を そのまま扱う。ダメなら WAMR + 同じ形の core ABI。最初の spike
  で決める
- `wasi:http` のホスト実装は自前（`esp_http_client` /
  esp-tls、証明書バンドル）。 `wasmtime-wasi-http` は tokio/hyper
  依存なので使えない
- 描画: MIPI-DSI の DPI フレームバッファ 2 枚、`gfx.submit` のコマンドを PPA
  で描く。 日本語フォントを内蔵
- タッチ: GT911 → `event.touch`。上端からの下スワイプでランチャーへ
- 更新: ハードウェアボタンで `apps/manifest.json` を取得し、sha256 を確かめて
  flash のパーティションへ保存。wasm
  ではなくネイティブ実装、予備ランチャーを内蔵
- 事前コンパイル（.cwasm）は CI で作り、manifest にエンジン版を書く
