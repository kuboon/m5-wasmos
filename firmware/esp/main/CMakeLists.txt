idf_component_register(
    SRCS "main.c"
    INCLUDE_DIRS "."
    REQUIRES nvs_flash esp_http_client esp_timer esp_netif esp_event esp_wifi spiffs mbedtls esp_lcd esp_driver_usb_serial_jtag vfs
)

# Rust 側（firmware/esp/rust）。firmware/esp/build_rust.sh で作っておく
set(WASMOS_RUST_LIB "${CMAKE_CURRENT_SOURCE_DIR}/../../target/riscv32imafc-unknown-none-elf/release/libwasmos_esp.a")
if(NOT EXISTS ${WASMOS_RUST_LIB})
    message(FATAL_ERROR "${WASMOS_RUST_LIB} がありません。firmware/esp/build_rust.sh を実行してください")
endif()
add_prebuilt_library(wasmos_rust ${WASMOS_RUST_LIB})
target_link_libraries(${COMPONENT_LIB} PRIVATE wasmos_rust)
# Rust ライブラリから C 側の wasmos_hw_* を参照するので、main をもう一度後ろに並べる
target_link_libraries(wasmos_rust INTERFACE ${COMPONENT_LIB})
