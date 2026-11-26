# m5-wasmos ファームウェア（ESP-IDF v5.4, esp32p4）
# 先に Rust の静的ライブラリを作っておく:
#   cargo build --release -p wasmos-esp --target riscv32imafc-unknown-none-elf  （firmware/ で）
cmake_minimum_required(VERSION 3.16)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(m5-wasmos)
