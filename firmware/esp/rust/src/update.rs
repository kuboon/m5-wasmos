//! アプリの更新。GitHub Pages の manifest.json を読み、変わった .cwasm だけ取ってくる。
//!
//! wasm ではなくホスト側で実装しているので、ランチャーが壊れていても更新できる。

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use wasmos_host::{AppMeta, Runtime};

use crate::hw;
use crate::platform::EspPlatform;
use crate::screen;

/// 更新元。main ブランチの GitHub Pages
pub const MANIFEST_URL: &str = "https://kuboon.github.io/m5-wasmos/apps/manifest.json";
const LOCAL_MANIFEST: &str = "manifest.json";

#[derive(Deserialize)]
pub struct Manifest {
    pub engine: String,
    pub apps: Vec<App>,
}

#[derive(Deserialize)]
pub struct App {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub color: String,
    #[serde(default)]
    pub hidden: bool,
    pub cwasm: String,
    #[serde(rename = "cwasmSha256")]
    pub cwasm_sha256: String,
}

impl App {
    pub fn meta(&self) -> AppMeta {
        let rgb = u32::from_str_radix(self.color.trim_start_matches('#'), 16).unwrap_or(0x64748b);
        AppMeta {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            description: self.description.clone(),
            color: 0xff00_0000 | rgb,
            hidden: self.hidden,
        }
    }
}

pub fn local_manifest() -> Option<Manifest> {
    serde_json::from_slice(&hw::file_read(LOCAL_MANIFEST)?).ok()
}

/// ブロックして 1 つ取ってくる（更新中は他に何も動かさない）
fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let h = hw::http_start("GET", url, "", &[]);
    let deadline = hw::now_ms() + 60_000;
    loop {
        match hw::http_poll(h) {
            hw::HttpPoll::Pending if hw::now_ms() < deadline => hw::delay_ms(20),
            hw::HttpPoll::Pending => {
                hw::http_free(h);
                return Err(format!("timeout: {url}"));
            }
            hw::HttpPoll::Done { status, body } => {
                hw::http_free(h);
                return if status == 200 {
                    Ok(body)
                } else {
                    Err(format!("HTTP {status}: {url}"))
                };
            }
            hw::HttpPoll::Failed(e) => {
                hw::http_free(h);
                return Err(format!("{e}: {url}"));
            }
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// manifest からの相対 URL を解決する
fn resolve(rel: &str) -> String {
    if rel.contains("://") {
        return rel.to_string();
    }
    let base = &MANIFEST_URL[..MANIFEST_URL.rfind('/').map(|i| i + 1).unwrap_or(0)];
    format!("{base}{rel}")
}

/// 戻り値は取得したアプリの数
pub fn run(rt: &mut Runtime<EspPlatform>) -> Result<usize, String> {
    screen::message(rt, "更新を確認しています", MANIFEST_URL);
    let raw = fetch(MANIFEST_URL)?;
    let remote: Manifest = serde_json::from_slice(&raw).map_err(|e| format!("manifest: {e}"))?;
    if remote.engine != wasmos_host::engine::ENGINE_ID {
        return Err(format!(
            "ファームウェアの更新が必要です（サイト: {} / 本体: {}）",
            remote.engine,
            wasmos_host::engine::ENGINE_ID
        ));
    }
    let local = local_manifest();
    let have = |app: &App| {
        local
            .as_ref()
            .and_then(|m| m.apps.iter().find(|a| a.id == app.id))
            .is_some_and(|a| a.cwasm_sha256 == app.cwasm_sha256)
            && hw::file_read(&format!("{}.cwasm", app.id)).is_some()
    };

    let mut fetched = 0;
    let total = remote.apps.len();
    for (i, app) in remote.apps.iter().enumerate() {
        if have(app) {
            continue;
        }
        screen::message(
            rt,
            "アプリを取得しています",
            &format!("{} ({}/{total})", app.name, i + 1),
        );
        let bin = fetch(&resolve(&app.cwasm))?;
        let digest = hex(&Sha256::digest(&bin));
        if digest != app.cwasm_sha256 {
            return Err(format!("{}: sha256 が一致しません", app.id));
        }
        if !hw::file_write(&format!("{}.cwasm", app.id), &bin) {
            return Err(format!("{}: 保存に失敗しました", app.id));
        }
        fetched += 1;
    }
    if !hw::file_write(LOCAL_MANIFEST, &raw) {
        return Err("manifest の保存に失敗しました".into());
    }
    Ok(fetched)
}
