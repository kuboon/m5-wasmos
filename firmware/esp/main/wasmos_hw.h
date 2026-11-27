// Rust（firmware/esp/rust）と C（このディレクトリ）の境界。
// C がハードウェアを触る関数を実装し、Rust の wasmos_main() がメインループを回す。
// 変えたら firmware/esp/rust/src/hw.rs も揃える。
#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// 物理パネルは縦長 720x1280（RGB565）。論理画面も同じ向き（180 度回転の切り替えは Rust 側）
#define WASMOS_PANEL_W 720
#define WASMOS_PANEL_H 1280

// --- Rust が呼ぶ（C が実装） ---
void wasmos_hw_log(const char *msg, size_t len);
uint64_t wasmos_hw_now_ms(void);
uint64_t wasmos_hw_wall_ms(void);  // 未同期なら 0
void wasmos_hw_delay_ms(uint32_t ms);
void *wasmos_hw_alloc(size_t size, size_t align);  // PSRAM
void wasmos_hw_free(void *p);

// 物理座標 [x0,x1) x [y0,y1) に RGB565 を描く
void wasmos_hw_draw(int x0, int y0, int x1, int y1, const uint16_t *pixels);
// バックライト（false で消す。画面の中身とタッチはそのまま）
void wasmos_hw_backlight(bool on);
// 物理座標のタッチ点（最大 max 個）。戻り値は点の数
int wasmos_hw_touch(uint16_t *xs, uint16_t *ys, int max);

// HTTP（別タスクで実行）。headers は "name: value\r\n" の連結（NUL 終端）
uint32_t wasmos_hw_http_start(const char *method, const char *url, const char *headers,
                              const uint8_t *body, size_t body_len);
// 0: 実行中 / 1: 完了（status, body）/ 2: 失敗（err）
int wasmos_hw_http_poll(uint32_t handle, int *status, const uint8_t **body, size_t *len,
                        const char **err);
void wasmos_hw_http_free(uint32_t handle);

// アプリ保存領域（SPIFFS /apps）。read の buf は wasmos_hw_free で解放
int wasmos_hw_file_read(const char *name, uint8_t **buf, size_t *len);  // 0: OK
int wasmos_hw_file_write(const char *name, const uint8_t *buf, size_t len);  // 0: OK

// ネットワーク状態の説明文。接続済みなら 1
int wasmos_hw_net_status(char *buf, size_t cap);
// 電池の電圧 [mV] と電流 [mA]（+ が充電、- が放電）。読めたら 1
int wasmos_hw_battery(int *mv, int *ma);
// シリアルの "update" コマンドなど。読むと消える
bool wasmos_hw_take_update_request(void);

// --- C が呼ぶ（Rust が実装） ---
void wasmos_main(void);
