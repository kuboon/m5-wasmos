#!/usr/bin/env bash
# Rust 側（firmware/esp/rust）を ESP-IDF に link できる静的ライブラリにする。
#
# Rust の compiler_builtins には C（compiler-rt）からビルドされたオブジェクトが少し入っていて、
# それらは soft-float ABI になっている。ESP32-P4 の ESP-IDF は single-float（ilp32f）なので
# 「can't link soft-float modules with single-float modules」で link できない。
# それらは libgcc にも同じ関数があるので、soft-float のオブジェクトだけ取り除く。
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p wasmos-esp --target riscv32imafc-unknown-none-elf
lib=target/riscv32imafc-unknown-none-elf/release/libwasmos_esp.a

READELF=$(command -v riscv32-esp-elf-readelf || command -v readelf)
AR=$(command -v riscv32-esp-elf-ar || command -v ar)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
(cd "$work" && "$AR" x "$OLDPWD/$lib")
removed=()
for o in "$work"/*.o; do
  if "$READELF" -h "$o" 2>/dev/null | grep -q "soft-float ABI"; then
    removed+=("$(basename "$o")")
  fi
done
if [ ${#removed[@]} -gt 0 ]; then
  "$AR" d "$lib" "${removed[@]}"
  echo "removed ${#removed[@]} soft-float objects: ${removed[*]}"
fi
