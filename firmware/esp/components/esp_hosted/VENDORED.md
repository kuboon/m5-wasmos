# esp_hosted 1.4.0（同梱）

- 元: https://github.com/espressif/esp-hosted-mcu コミット 0f3f4c7（idf_component.yml の version: "1.4.0"）
- protobuf-c: https://github.com/protobuf-c/protobuf-c コミット abc67a11（元リポジトリの submodule と同じ）
- ホスト側（CMakeLists.txt, Kconfig, common/, host/）だけ。slave/ docs/ examples/ は省いた
- ライセンス: Apache-2.0（LICENSE）、protobuf-c は BSD-2-Clause（common/protobuf-c/LICENSE）

同梱している理由: Tab5 の ESP32-C6 の工場出荷ファーム（esp_hosted slave 1.4.x）に合わせて
ホスト側も 1.4.0 にしたいが、レジストリ経由だと esp_wifi_remote の依存解決で esp_hosted 3.x の
manifest まで評価され、そこにある Kconfig 条件で ESP-IDF の configure が止まる。
プロジェクトの components/ にあるものはレジストリより優先されるので、ここに置いている。
