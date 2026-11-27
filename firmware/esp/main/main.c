// m5-wasmos ファームウェア（C 側）。ボード初期化と、Rust から呼ばれるハードウェア関数。
//
// - 画面・タッチ: espressif/m5stack_tab5 BSP（LVGL は使わない）
// - Wi-Fi: ESP32-C6 を esp_hosted / esp_wifi_remote 経由で使う。2 つまで保存し、
//          つながる方に自動で接続する（自宅 / テザリングの切り替え）
// - 設定: USB シリアルのコマンド（wifi / update / reboot など）。help で一覧
// - アプリ: SPIFFS（パーティション "apps"）に manifest.json と <id>.cwasm を置く

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>

#include "bsp/display.h"
#include "driver/usb_serial_jtag.h"
#include "driver/usb_serial_jtag_vfs.h"
#include "bsp/m5stack_tab5.h"
#include "bsp/touch.h"
#include "esp_crt_bundle.h"
#include "esp_event.h"
#include "esp_heap_caps.h"
#include "esp_http_client.h"
#include "esp_lcd_panel_ops.h"
#include "esp_lcd_touch.h"
#include "esp_lcd_touch_gt911.h"
#include "esp_lcd_touch_st7123.h"
#include "esp_lcd_panel_io.h"
#include "driver/i2c_master.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_netif_sntp.h"
#include "esp_spiffs.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "freertos/queue.h"
#include "freertos/semphr.h"
#include "freertos/idf_additions.h"
#include "freertos/task.h"
#include "nvs.h"
#include "nvs_flash.h"
#include "wasmos_hw.h"

static const char *TAG = "wasmos";

// ---------------------------------------------------------------------------
// 基本

void wasmos_hw_log(const char *msg, size_t len) { printf("%.*s\n", (int)len, msg); }

uint64_t wasmos_hw_now_ms(void) { return (uint64_t)(esp_timer_get_time() / 1000); }

uint64_t wasmos_hw_wall_ms(void) {
    struct timeval tv;
    gettimeofday(&tv, NULL);
    // 2024 年より前なら未同期とみなす
    if (tv.tv_sec < 1704067200) return 0;
    return (uint64_t)tv.tv_sec * 1000 + tv.tv_usec / 1000;
}

void wasmos_hw_delay_ms(uint32_t ms) { vTaskDelay(pdMS_TO_TICKS(ms ? ms : 1)); }

void *wasmos_hw_alloc(size_t size, size_t align) {
    if (align < sizeof(void *)) align = sizeof(void *);
    return heap_caps_aligned_alloc(align, size ? size : 1, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
}

void wasmos_hw_free(void *p) { heap_caps_free(p); }

// ---------------------------------------------------------------------------
// 画面・タッチ

static esp_lcd_panel_handle_t s_panel;
static esp_lcd_touch_handle_t s_touch;

// Tab5 は製造時期で画面とタッチが違う。BSP の bsp_get_board_version()（static で呼べない）と同じ判定
typedef enum { BOARD_UNKNOWN, BOARD_ILI9881C_GT911, BOARD_ST7123, BOARD_ST7121 } board_version_t;

static board_version_t detect_board(void) {
    if (bsp_feature_enable(BSP_FEATURE_TOUCH, true) != ESP_OK) return BOARD_UNKNOWN;
    vTaskDelay(pdMS_TO_TICKS(500));
    i2c_master_bus_handle_t bus = bsp_i2c_get_handle();
    if (i2c_master_probe(bus, ESP_LCD_TOUCH_IO_I2C_ST7123_ADDRESS, 100) == ESP_OK) {
        esp_lcd_panel_io_handle_t io = NULL;
        esp_lcd_panel_io_i2c_config_t cfg = ESP_LCD_TOUCH_IO_I2C_ST7123_CONFIG();
        uint8_t fw = 0;
        if (esp_lcd_new_panel_io_i2c(bus, &cfg, &io) != ESP_OK) return BOARD_UNKNOWN;
        esp_err_t err = esp_lcd_panel_io_rx_param(io, 0x0000, &fw, sizeof fw);
        esp_lcd_panel_io_del(io);
        if (err != ESP_OK) return BOARD_UNKNOWN;
        return fw == 1 ? BOARD_ST7121 : BOARD_ST7123;
    }
    if (i2c_master_probe(bus, ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS_BACKUP, 100) == ESP_OK) {
        return BOARD_ILI9881C_GT911;
    }
    return BOARD_UNKNOWN;
}

static void display_init(void) {
    ESP_ERROR_CHECK(bsp_i2c_init());
    // LVGL 版の bsp_display_start と同じ順: 画面の電源 → 機種判定 → DSI の速度を選んで初期化
    ESP_ERROR_CHECK(bsp_feature_enable(BSP_FEATURE_LCD, true));
    board_version_t board = detect_board();
    ESP_LOGI(TAG, "board version: %d", board);
    bsp_display_config_t cfg = {
        .dsi_bus = {
            .phy_clk_src = 0,  // ドライバの既定
            // ST7121 の版だけ 965Mbps（BSP の BSP_LCD_ST712X_MIPI_DSI_LANE_BITRATE_MBPS）
            .lane_bit_rate_mbps = board == BOARD_ST7121 ? 965 : BSP_LCD_MIPI_DSI_LANE_BITRATE_MBPS,
        },
    };
    esp_lcd_panel_io_handle_t io = NULL;
    ESP_ERROR_CHECK(bsp_display_new(&cfg, &s_panel, &io));
    ESP_ERROR_CHECK(esp_lcd_panel_disp_on_off(s_panel, true));
    ESP_ERROR_CHECK(bsp_display_brightness_init());
    wasmos_hw_backlight(true);

    bsp_touch_config_t tcfg = {0};
    if (bsp_touch_new(&tcfg, &s_touch) != ESP_OK) {
        ESP_LOGE(TAG, "touch init failed");
        s_touch = NULL;
    }
}

// バックライトだけを消す・点ける（スリープ）。bsp_display_enter_sleep はタッチも
// 眠らせてしまい、触って起こせなくなるので使わない
#define BRIGHTNESS 80
void wasmos_hw_backlight(bool on) { bsp_display_brightness_set(on ? BRIGHTNESS : 0); }

void wasmos_hw_draw(int x0, int y0, int x1, int y1, const uint16_t *pixels) {
    if (!s_panel) return;
    esp_lcd_panel_draw_bitmap(s_panel, x0, y0, x1, y1, pixels);
}

int wasmos_hw_touch(uint16_t *xs, uint16_t *ys, int max) {
    if (!s_touch) return 0;
    if (esp_lcd_touch_read_data(s_touch) != ESP_OK) return 0;
    uint16_t strength[10];
    uint8_t n = 0;
    if (max > 10) max = 10;
    if (!esp_lcd_touch_get_coordinates(s_touch, xs, ys, strength, &n, (uint8_t)max)) return 0;
    return n;
}

// ---------------------------------------------------------------------------
// 電源（充電と電池）
//
// Tab5 の充電 IC（IP2326）は I/O エキスパンダ 0x44 の P7（CHG_EN、High で充電）で止まる。
// BSP の expander ドライバは初期化でチップをリセットし、全ピンが Hi-Z + プルダウンになる
// ので、何もしないと充電しない。M5Tab5-UserDemo（setChargeEnable(true)）と M5Unified
// （Power_Class の Tab5 初期化）に合わせて、起動時に CHG_EN を High にする。
// P5（nCHG_QC_EN、Low で 1A / High で 0.5A）も両者と同じく Low にする。
//
// 電池の電圧・電流は INA226（0x41、シャント 5mΩ）。充電中は電流が負に出るので符号を反転する

#define CHG_QC_N_PIN IO_EXPANDER_PIN_NUM_5
#define CHG_EN_PIN IO_EXPANDER_PIN_NUM_7
#define INA226_ADDR 0x41

static i2c_master_dev_handle_t s_ina226;

static void power_init(void) {
    esp_io_expander_handle_t ioe = bsp_io_expander1_init();
    esp_err_t err = ESP_FAIL;
    if (ioe) {
        err = esp_io_expander_set_level(ioe, CHG_QC_N_PIN | CHG_EN_PIN, 0);
        err |= esp_io_expander_set_dir(ioe, CHG_QC_N_PIN | CHG_EN_PIN, IO_EXPANDER_OUTPUT);
        err |= esp_io_expander_set_output_mode(ioe, CHG_QC_N_PIN | CHG_EN_PIN,
                                               IO_EXPANDER_OUTPUT_MODE_PUSH_PULL);
        err |= esp_io_expander_set_level(ioe, CHG_EN_PIN, 1);
    }
    if (err != ESP_OK) ESP_LOGE(TAG, "charge enable failed");

    i2c_device_config_t dev = {
        .dev_addr_length = I2C_ADDR_BIT_LEN_7,
        .device_address = INA226_ADDR,
        .scl_speed_hz = 400000,
    };
    if (i2c_master_bus_add_device(bsp_i2c_get_handle(), &dev, &s_ina226) != ESP_OK) s_ina226 = NULL;
}

static bool ina226_read(uint8_t reg, uint16_t *out) {
    uint8_t buf[2];
    if (!s_ina226 || i2c_master_transmit_receive(s_ina226, &reg, 1, buf, 2, 50) != ESP_OK) return false;
    *out = (uint16_t)(buf[0] << 8 | buf[1]);
    return true;
}

// 電池電圧 [mV] と電流 [mA]（+ が充電、- が放電）
static bool battery_read(int *mv, int *ma) {
    uint16_t bus, shunt;
    if (!ina226_read(0x02, &bus) || !ina226_read(0x01, &shunt)) return false;
    *mv = bus * 5 / 4;               // 1.25mV/LSB
    *ma = -(int16_t)shunt / 2;       // 2.5uV/LSB ÷ 5mΩ = 0.5mA/LSB
    return true;
}

int wasmos_hw_battery(int *mv, int *ma) { return battery_read(mv, ma) ? 1 : 0; }

static void power_report(void) {
    int mv, ma;
    bool chg_en = false;
    esp_io_expander_handle_t ioe = bsp_io_expander1_init();
    uint32_t levels = 0;
    if (ioe && esp_io_expander_get_level(ioe, CHG_EN_PIN, &levels) == ESP_OK) chg_en = levels != 0;
    if (!battery_read(&mv, &ma)) {
        printf("power: CHG_EN=%d, battery: read failed\n", chg_en);
        return;
    }
    const char *state = ma > 10 ? "charging" : ma < -10 ? "discharging" : "idle";
    printf("power: CHG_EN=%d, battery %d.%03dV %dmA (%s)\n", chg_en, mv / 1000, mv % 1000, ma, state);
}

// ---------------------------------------------------------------------------
// 保存領域

#define APPS_DIR "/apps"

static void storage_init(void) {
    esp_vfs_spiffs_conf_t conf = {
        .base_path = APPS_DIR,
        .partition_label = "apps",
        .max_files = 4,
        .format_if_mount_failed = true,
    };
    esp_err_t err = esp_vfs_spiffs_register(&conf);
    if (err != ESP_OK) ESP_LOGE(TAG, "spiffs: %s", esp_err_to_name(err));
}

int wasmos_hw_file_read(const char *name, uint8_t **buf, size_t *len) {
    char path[64];
    snprintf(path, sizeof path, APPS_DIR "/%s", name);
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    uint8_t *p = wasmos_hw_alloc(size > 0 ? size : 1, 16);
    if (!p) {
        fclose(f);
        return -2;
    }
    size_t got = fread(p, 1, size, f);
    fclose(f);
    if ((long)got != size) {
        wasmos_hw_free(p);
        return -3;
    }
    *buf = p;
    *len = got;
    return 0;
}

int wasmos_hw_file_write(const char *name, const uint8_t *buf, size_t len) {
    char path[64], tmp[64];
    snprintf(path, sizeof path, APPS_DIR "/%s", name);
    snprintf(tmp, sizeof tmp, APPS_DIR "/%s.t", name);
    FILE *f = fopen(tmp, "wb");
    if (!f) return -1;
    size_t put = fwrite(buf, 1, len, f);
    fclose(f);
    if (put != len) {
        remove(tmp);
        return -2;
    }
    remove(path);
    return rename(tmp, path) == 0 ? 0 : -3;
}

// ---------------------------------------------------------------------------
// Wi-Fi（2 スロット）

#define WIFI_SLOTS 2
#define CONNECTED_BIT BIT0
#define FAIL_BIT BIT1

static EventGroupHandle_t s_wifi_events;
static char s_net_status[96] = "Wi-Fi 未設定";
static volatile bool s_connected;
static volatile bool s_update_requested;

typedef struct {
    char ssid[33];
    char pass[65];
} wifi_slot_t;

static bool slot_load(int i, wifi_slot_t *out) {
    nvs_handle_t h;
    if (nvs_open("wifi", NVS_READONLY, &h) != ESP_OK) return false;
    char key[8];
    size_t n = sizeof out->ssid;
    snprintf(key, sizeof key, "ssid%d", i);
    bool ok = nvs_get_str(h, key, out->ssid, &n) == ESP_OK && out->ssid[0];
    n = sizeof out->pass;
    snprintf(key, sizeof key, "pass%d", i);
    if (nvs_get_str(h, key, out->pass, &n) != ESP_OK) out->pass[0] = 0;
    nvs_close(h);
    return ok;
}

static void slot_save(int i, const char *ssid, const char *pass) {
    nvs_handle_t h;
    if (nvs_open("wifi", NVS_READWRITE, &h) != ESP_OK) return;
    char key[8];
    snprintf(key, sizeof key, "ssid%d", i);
    nvs_set_str(h, key, ssid);
    snprintf(key, sizeof key, "pass%d", i);
    nvs_set_str(h, key, pass);
    nvs_commit(h);
    nvs_close(h);
}

static void on_wifi_event(void *arg, esp_event_base_t base, int32_t id, void *data) {
    if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        s_connected = false;
        xEventGroupSetBits(s_wifi_events, FAIL_BIT);
    } else if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP) {
        ip_event_got_ip_t *e = data;
        wifi_config_t cfg;
        esp_wifi_get_config(WIFI_IF_STA, &cfg);
        snprintf(s_net_status, sizeof s_net_status, "%s  " IPSTR, (char *)cfg.sta.ssid, IP2STR(&e->ip_info.ip));
        s_connected = true;
        xEventGroupSetBits(s_wifi_events, CONNECTED_BIT);
    }
}

// 保存済みのネットワークを順に試し、切れたらまた順に試す
static void wifi_task(void *arg) {
    for (;;) {
        if (s_connected) {
            xEventGroupWaitBits(s_wifi_events, FAIL_BIT, pdTRUE, pdFALSE, portMAX_DELAY);
            continue;
        }
        bool any = false;
        for (int i = 0; i < WIFI_SLOTS && !s_connected; i++) {
            wifi_slot_t slot;
            if (!slot_load(i, &slot)) continue;
            any = true;
            snprintf(s_net_status, sizeof s_net_status, "%s に接続中…", slot.ssid);
            wifi_config_t cfg = {0};
            strlcpy((char *)cfg.sta.ssid, slot.ssid, sizeof cfg.sta.ssid);
            strlcpy((char *)cfg.sta.password, slot.pass, sizeof cfg.sta.password);
            esp_wifi_disconnect();
            esp_wifi_set_config(WIFI_IF_STA, &cfg);
            xEventGroupClearBits(s_wifi_events, CONNECTED_BIT | FAIL_BIT);
            esp_wifi_connect();
            EventBits_t bits = xEventGroupWaitBits(s_wifi_events, CONNECTED_BIT | FAIL_BIT, pdTRUE,
                                                   pdFALSE, pdMS_TO_TICKS(15000));
            if (!(bits & CONNECTED_BIT)) {
                ESP_LOGW(TAG, "wifi: %s failed", slot.ssid);
                xEventGroupClearBits(s_wifi_events, FAIL_BIT);
            }
        }
        if (!any) snprintf(s_net_status, sizeof s_net_status, "Wi-Fi 未設定（USB から wifi コマンドで登録）");
        if (!s_connected) vTaskDelay(pdMS_TO_TICKS(any ? 3000 : 1000));
    }
}

static void wifi_init(void) {
    // ESP32-C6 の電源は I/O エキスパンダの WIFI_EN。入れないと SDIO の初期化がタイムアウトする
    ESP_ERROR_CHECK(bsp_feature_enable(BSP_FEATURE_WIFI, true));
    vTaskDelay(pdMS_TO_TICKS(100));
    s_wifi_events = xEventGroupCreate();
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    esp_netif_create_default_wifi_sta();
    wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&cfg));
    ESP_ERROR_CHECK(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, on_wifi_event, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP, on_wifi_event, NULL));
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_start());

    esp_sntp_config_t sntp = ESP_NETIF_SNTP_DEFAULT_CONFIG("pool.ntp.org");
    esp_netif_sntp_init(&sntp);

    xTaskCreate(wifi_task, "wifi", 4096, NULL, 5, NULL);
}

int wasmos_hw_net_status(char *buf, size_t cap) {
    strlcpy(buf, s_net_status, cap);
    return s_connected ? 1 : 0;
}

bool wasmos_hw_take_update_request(void) {
    bool r = s_update_requested;
    s_update_requested = false;
    return r;
}

// ---------------------------------------------------------------------------
// HTTP（1 本のワーカータスクで順に処理）

#define HTTP_SLOTS 8

typedef enum { H_FREE, H_QUEUED, H_RUNNING, H_DONE, H_ERROR } http_state_t;

typedef struct {
    volatile http_state_t state;
    volatile bool cancelled;
    char *method, *url, *headers;
    uint8_t *req_body;
    size_t req_len;
    int status;
    uint8_t *body;
    size_t len, cap;
    char err[96];
} http_job_t;

static http_job_t s_jobs[HTTP_SLOTS];
static QueueHandle_t s_http_queue;

static char *dup_str(const char *s) {
    size_t n = strlen(s) + 1;
    char *p = heap_caps_malloc(n, MALLOC_CAP_SPIRAM);
    if (p) memcpy(p, s, n);
    return p;
}

static void job_clear(http_job_t *j) {
    heap_caps_free(j->method);
    heap_caps_free(j->url);
    heap_caps_free(j->headers);
    heap_caps_free(j->req_body);
    heap_caps_free(j->body);
    memset(j, 0, sizeof *j);
}

static esp_err_t on_http_event(esp_http_client_event_t *e) {
    http_job_t *j = e->user_data;
    if (e->event_id == HTTP_EVENT_ON_DATA && e->data_len > 0) {
        if (j->len + e->data_len > j->cap) {
            size_t cap = j->cap ? j->cap * 2 : 16384;
            while (cap < j->len + e->data_len) cap *= 2;
            uint8_t *p = heap_caps_realloc(j->body, cap, MALLOC_CAP_SPIRAM);
            if (!p) return ESP_ERR_NO_MEM;
            j->body = p;
            j->cap = cap;
        }
        memcpy(j->body + j->len, e->data, e->data_len);
        j->len += e->data_len;
    }
    return ESP_OK;
}

static esp_http_client_method_t method_of(const char *m) {
    if (!strcmp(m, "POST")) return HTTP_METHOD_POST;
    if (!strcmp(m, "PUT")) return HTTP_METHOD_PUT;
    if (!strcmp(m, "DELETE")) return HTTP_METHOD_DELETE;
    if (!strcmp(m, "PATCH")) return HTTP_METHOD_PATCH;
    if (!strcmp(m, "HEAD")) return HTTP_METHOD_HEAD;
    if (!strcmp(m, "OPTIONS")) return HTTP_METHOD_OPTIONS;
    return HTTP_METHOD_GET;
}

static void http_task(void *arg) {
    for (;;) {
        int idx;
        xQueueReceive(s_http_queue, &idx, portMAX_DELAY);
        http_job_t *j = &s_jobs[idx];
        if (j->cancelled) {
            job_clear(j);
            continue;
        }
        j->state = H_RUNNING;
        esp_http_client_config_t cfg = {
            .url = j->url,
            .method = method_of(j->method),
            .event_handler = on_http_event,
            .user_data = j,
            .crt_bundle_attach = esp_crt_bundle_attach,
            .timeout_ms = 20000,
            .buffer_size = 4096,
            .max_redirection_count = 5,
        };
        esp_http_client_handle_t c = esp_http_client_init(&cfg);
        // "name: value\r\n" を分解してヘッダに入れる
        for (char *line = j->headers; line && *line;) {
            char *end = strstr(line, "\r\n");
            if (end) *end = 0;
            char *colon = strchr(line, ':');
            if (colon) {
                *colon = 0;
                const char *v = colon + 1;
                while (*v == ' ') v++;
                esp_http_client_set_header(c, line, v);
            }
            line = end ? end + 2 : NULL;
        }
        if (j->req_len) esp_http_client_set_post_field(c, (const char *)j->req_body, j->req_len);
        esp_err_t err = esp_http_client_perform(c);
        if (err == ESP_OK) {
            j->status = esp_http_client_get_status_code(c);
            j->state = H_DONE;
        } else {
            snprintf(j->err, sizeof j->err, "%s", esp_err_to_name(err));
            j->state = H_ERROR;
        }
        esp_http_client_cleanup(c);
        if (j->cancelled) job_clear(j);
    }
}

uint32_t wasmos_hw_http_start(const char *method, const char *url, const char *headers,
                              const uint8_t *body, size_t body_len) {
    for (int i = 0; i < HTTP_SLOTS; i++) {
        http_job_t *j = &s_jobs[i];
        if (j->state != H_FREE) continue;
        memset(j, 0, sizeof *j);
        j->method = dup_str(method);
        j->url = dup_str(url);
        j->headers = dup_str(headers);
        if (body_len) {
            j->req_body = heap_caps_malloc(body_len, MALLOC_CAP_SPIRAM);
            memcpy(j->req_body, body, body_len);
            j->req_len = body_len;
        }
        j->state = H_QUEUED;
        xQueueSend(s_http_queue, &i, portMAX_DELAY);
        return (uint32_t)i + 1;
    }
    return 0;  // 空きなし（Rust 側は失敗として扱う）
}

int wasmos_hw_http_poll(uint32_t handle, int *status, const uint8_t **body, size_t *len,
                        const char **err) {
    if (handle == 0 || handle > HTTP_SLOTS) {
        *err = "no free http slot";
        return 2;
    }
    http_job_t *j = &s_jobs[handle - 1];
    switch (j->state) {
        case H_DONE:
            *status = j->status;
            *body = j->body;
            *len = j->len;
            return 1;
        case H_ERROR:
            *err = j->err;
            return 2;
        default:
            return 0;
    }
}

void wasmos_hw_http_free(uint32_t handle) {
    if (handle == 0 || handle > HTTP_SLOTS) return;
    http_job_t *j = &s_jobs[handle - 1];
    if (j->state == H_DONE || j->state == H_ERROR) {
        job_clear(j);
    } else if (j->state != H_FREE) {
        j->cancelled = true;  // ワーカーが終わったら片付ける
    }
}

// ---------------------------------------------------------------------------
// USB シリアルのコマンド（1 行 1 コマンド。区切りは空白、行にタブがあればタブ）
//
//   wifi                     保存済み一覧
//   wifi <0|1> <ssid> [pass] 保存（0: 自宅, 1: テザリング など。0 を先に試す）
//   wifi <0|1> clear         消去
//   update                   アプリを更新
//   power                    充電の設定と電池の電圧・電流
//   reboot
//
// SSID に空白がある（"xxx の iPhone" など）ときはタブ区切りで送る。サイトの書き込みページはそうする

static void console_init(void) {
    // stdin をブロッキングで読めるように USB Serial/JTAG のドライバを入れる
    usb_serial_jtag_driver_config_t cfg = USB_SERIAL_JTAG_DRIVER_CONFIG_DEFAULT();
    if (usb_serial_jtag_driver_install(&cfg) == ESP_OK) usb_serial_jtag_vfs_use_driver();
    setvbuf(stdin, NULL, _IONBF, 0);
}

static void console_task(void *arg) {
    char line[200];
    for (;;) {
        if (!fgets(line, sizeof line, stdin)) {
            vTaskDelay(pdMS_TO_TICKS(50));
            continue;
        }
        line[strcspn(line, "\r\n")] = 0;
        char *argv[4] = {0};
        int argc = 0;
        const char *sep = strchr(line, '\t') ? "\t" : " ";
        for (char *t = strtok(line, sep); t && argc < 4; t = strtok(NULL, sep)) argv[argc++] = t;
        if (argc == 0) continue;
        if (!strcmp(argv[0], "wifi")) {
            if (argc == 1) {
                for (int i = 0; i < WIFI_SLOTS; i++) {
                    wifi_slot_t s;
                    printf("wifi %d: %s\n", i, slot_load(i, &s) ? s.ssid : "(none)");
                }
                printf("status: %s\n", s_net_status);
            } else if (argc >= 3 && (argv[1][0] == '0' || argv[1][0] == '1')) {
                int i = argv[1][0] - '0';
                if (!strcmp(argv[2], "clear")) {
                    slot_save(i, "", "");
                    printf("wifi %d cleared\n", i);
                } else {
                    slot_save(i, argv[2], argc > 3 ? argv[3] : "");
                    printf("wifi %d saved: %s\n", i, argv[2]);
                }
                esp_wifi_disconnect();  // 新しい設定で接続し直す
            } else {
                printf("usage: wifi <0|1> <ssid> [password] | wifi <0|1> clear\n");
            }
        } else if (!strcmp(argv[0], "update")) {
            s_update_requested = true;
            printf("update requested\n");
        } else if (!strcmp(argv[0], "power")) {
            power_report();
        } else if (!strcmp(argv[0], "reboot")) {
            esp_restart();
        } else {
            printf("commands: wifi, update, power, reboot\n");
        }
    }
}

// ---------------------------------------------------------------------------

static void rust_task(void *arg) {
    wasmos_main();  // 戻らない
}

void app_main(void) {
    esp_err_t err = nvs_flash_init();
    if (err == ESP_ERR_NVS_NO_FREE_PAGES || err == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        nvs_flash_erase();
        nvs_flash_init();
    }
    display_init();
    power_init();
    power_report();
    storage_init();

    s_http_queue = xQueueCreate(HTTP_SLOTS, sizeof(int));
    xTaskCreate(http_task, "http", 8192, NULL, 5, NULL);
    wifi_init();
    console_init();
    xTaskCreate(console_task, "console", 4096, NULL, 3, NULL);

    // wasmtime（Pulley インタプリタ）は再帰が深いので大きめのスタックを PSRAM に取る
    // （SPIRAM_XIP_FROM_PSRAM なのでフラッシュ書き込み中も PSRAM は読める）
    xTaskCreatePinnedToCoreWithCaps(rust_task, "wasmos", 256 * 1024, NULL, 4, NULL, 1,
                                    MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
}
