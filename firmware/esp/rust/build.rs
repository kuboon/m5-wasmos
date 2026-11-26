// 保存領域が空のときに使うランチャーを埋め込む（deno task build:apps の出力）。無ければ空
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("launcher.cwasm");
    let src = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../../web/build/apps/launcher/launcher.cwasm");
    println!("cargo:rerun-if-changed={}", src.display());
    let data = std::fs::read(&src).unwrap_or_default();
    std::fs::write(out, data).unwrap();
}
