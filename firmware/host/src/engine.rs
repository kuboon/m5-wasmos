//! wasmtime の設定。実機（no_std, riscv32）と事前コンパイル（tools/precompile）で
//! 完全に同じでないと .cwasm を読めないので、ここ 1 か所で決める。

use wasmtime::{Config, Engine, Result};

/// manifest に書くエンジン識別子。wasmtime の版か下の設定を変えたら上げる。
/// WIT（wit/wasmos.wit）を変えたときも上げる: 古い本体が、自分に無い import を使う
/// 新しいアプリを取ってきて起動できなくなるのを防ぐ（本体は一致しない manifest を読まない）
pub const ENGINE_ID: &str = "wasmtime-51.0.0/pulley32/fuel/1/wasmos-0.4";

/// 実機（ESP32-P4, 32bit）のターゲット
pub const TARGET: &str = "pulley32";

pub fn config() -> Result<Config> {
    config_for(TARGET)
}

/// Linux 上のテスト（firmware/sim）は 64bit なので pulley64 を使う。それ以外の設定は同じ
pub fn config_for(target: &str) -> Result<Config> {
    let mut c = Config::new();
    c.target(target)?;
    c.wasm_component_model(true);
    // 仮想メモリもシグナルも無い環境向け: malloc したメモリに境界チェック付きでアクセス
    c.signals_based_traps(false);
    c.memory_reservation(0);
    c.memory_reservation_for_growth(0);
    c.memory_guard_size(0);
    c.guard_before_linear_memory(false);
    c.memory_may_move(true);
    c.memory_init_cow(false);
    // 固まったアプリを止めるため（epoch は 64bit atomic が要るので使えない）
    c.consume_fuel(true);
    Ok(c)
}

pub fn engine() -> Result<Engine> {
    Engine::new(&config()?)
}

pub fn engine_for(target: &str) -> Result<Engine> {
    Engine::new(&config_for(target)?)
}
