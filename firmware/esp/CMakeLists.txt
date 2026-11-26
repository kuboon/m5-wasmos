# m5-wasmos ファームウェア（ESP-IDF v5.5, esp32p4）
# 先に Rust の静的ライブラリを作っておく: ./build_rust.sh
cmake_minimum_required(VERSION 3.16)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(m5-wasmos)
