# firmware

M5Stack Tab5（ESP32-P4）で wasm アプリを動かすファームウェア。

```
host/       ハード非依存のホスト（no_std）。wasmtime + Pulley で .cwasm を動かし、WIT を実装する。
            描画は tiny-skia + ab_glyph（Noto Sans JP の JIS 第1・第2水準サブセットを内蔵）
sim/        host を Linux で動かすテスト用ランナー。タップやドラッグを流し込み、画面を PNG に書く
esp/main/   ESP-IDF 側（C）: BSP で画面・タッチ、esp_hosted で Wi-Fi（ESP32-C6）、HTTP、SPIFFS、USB コンソール
esp/rust/   実機側の Rust（staticlib）: メインループ、タッチの追跡、アップデータ
```

## しくみ

- アプリは CI（`scripts/build_apps.ts` → `tools/precompile`）で pulley32
  向けに事前コンパイルされ、 サイトの `apps/manifest.json` に `cwasm`
  として載る。本体はこれを取ってきて `apps` パーティション
  （SPIFFS）に保存する。エンジン設定は `host/src/engine.rs` の 1
  か所で、`ENGINE_ID` が一致しないと読まない
- 起動時にネットにつながったら自動で更新を確認する（sha256
  が変わったアプリだけ取得）。 USB コンソールの `update`
  でも更新できる。リセットボタン = 再起動 = 更新確認
- 充電は起動時に有効にする（I/O エキスパンダ 0x44 の P7 = CHG_EN。BSP は触らず、
  リセット後は充電しない）。USB コンソールの `power` で CHG_EN と電池の電圧・電流（INA226）を表示する
- Wi-Fi は 2 か所まで保存（NVS）。0 → 1 の順に試し、切れたらまた順に試す
- 画面上端からの下スワイプでランチャーへ戻る。固まったアプリは fuel
  切れで止めてランチャーへ戻る
- 論理画面は横 1280x720。パネル（縦 720x1280）への回転は
  `esp/rust/src/platform.rs` （上下逆に映ったら `ROTATE_180 = true`）

## ビルド

ESP-IDF のビルドは GitHub Actions（`.github/workflows/firmware.yml`,
`espressif/idf:v5.5.6`）で行い、 main では Release `firmware-latest`
に置く。サイトの `/flash` からブラウザで書き込める。

手元でやるなら:

```bash
deno task build:apps                       # ランチャーの .cwasm（ファームに埋め込む）
cd firmware/esp
. $IDF_PATH/export.sh
./build_rust.sh                            # Rust の staticlib（soft-float の部品を除く。理由はスクリプト内）
idf.py build flash monitor       # ESP-IDF v5.5
```

ホストだけを Linux で試す:

```bash
cd firmware && cargo build --release -p wasmos-sim
./target/release/wasmos-sim ../web/build/apps sim/smoke.txt
```

## 未対応・既知の制限

- PNG / JPEG のデコード（`gfx.load-image`）は未実装（`image-size` が none
  になる）
- HTTP
  応答ヘッダはアプリに渡していない。リクエスト本文のストリーミング送信も未対応
- 実機での動作はまだ確認していない（画面の向き、タッチの対応、Wi-Fi の C6
  ファーム版など）
