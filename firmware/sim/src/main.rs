//! firmware/host を Linux で動かす。実機と同じホストコードで、画面だけ PNG に書き出す。
//!
//!   wasmos-sim <apps dir> <script>
//!
//! apps dir は web/build/apps（manifest.json と <id>/<id>.cwasm）。script は 1 行 1 命令:
//!
//!   launch <id>            アプリを起動（最初は manifest の launcher）
//!   tap <x> <y>            down → up
//!   drag <x0> <y0> <x1> <y1>
//!   home                   システムジェスチャ
//!   wait <ms>              tick を回す
//!   mock <url-prefix> <file>   HTTP 応答を差し替え（その URL で始まるリクエストに file の中身を 200 で返す）
//!   battery <mV> <mA>      電池の状態を差し替え（2 セル。mA は + が充電）。既定は電池なし
//!   shot <file.png>        画面を保存

use std::collections::HashMap;
use std::time::{Duration, Instant};

use wasmos_host::bindings::wasmos::os::types::{Event, Touch, TouchPhase};
use wasmos_host::{AppMeta, Battery, HttpRequest, HttpResponse, IRect, Platform, Runtime};

static FONT: &[u8] = include_bytes!("../../host/assets/NotoSansJP-Regular-jis.ttf");

struct Sim {
    start: Instant,
    screen: Vec<u8>,
    mocks: Vec<(String, Vec<u8>)>,
    http: HashMap<u32, Result<HttpResponse, String>>,
    next: u32,
    battery: Option<Battery>,
}

impl Platform for Sim {
    fn log(&mut self, message: &str) {
        println!("{message}");
    }
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
    fn wall_ms(&self) -> Option<u64> {
        None
    }
    fn battery(&mut self) -> Option<Battery> {
        self.battery
    }
    fn http_start(&mut self, req: HttpRequest) -> u32 {
        let h = self.next;
        self.next += 1;
        let result = match self.mocks.iter().find(|(p, _)| req.url.starts_with(p)) {
            Some((_, body)) => Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: body.clone(),
            }),
            None => Err(format!("no mock for {}", req.url)),
        };
        self.http.insert(h, result);
        h
    }
    fn http_ready(&mut self, h: u32) -> bool {
        self.http.contains_key(&h)
    }
    fn http_take(&mut self, h: u32) -> Option<Result<HttpResponse, String>> {
        self.http.remove(&h)
    }
    fn http_cancel(&mut self, h: u32) {
        self.http.remove(&h);
    }
    fn present(&mut self, rgba: &[u8], _w: u32, _h: u32, _dirty: IRect) {
        self.screen.clear();
        self.screen.extend_from_slice(rgba);
    }
}

/// 64bit ホストでは pulley32 の .cwasm を実行できないので、.wasm を pulley64 で
/// その場でコンパイルする（エンジン設定は実機と同じ）
fn load(dir: &str, id: &str) -> Vec<u8> {
    let wasm =
        std::fs::read(format!("{dir}/{id}/{id}.wasm")).unwrap_or_else(|e| panic!("{id}.wasm: {e}"));
    let engine = wasmos_host::engine::engine_for("pulley64").unwrap();
    engine.precompile_component(&wasm).unwrap()
}

fn run_switches(rt: &mut Runtime<Sim>, dir: &str) {
    while let Some((id, args)) = rt.take_switch() {
        let cwasm = load(dir, &id);
        if let Err(e) = rt.start(&id, &cwasm, &args) {
            println!("start {id}: {e:?}");
        }
    }
}

fn main() -> wasmtime::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, dir, script] = args.as_slice() else {
        eprintln!("usage: wasmos-sim <apps dir> <script>");
        std::process::exit(2);
    };
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(format!("{dir}/manifest.json"))?)?;
    let launcher = manifest["launcher"]
        .as_str()
        .unwrap_or("launcher")
        .to_string();
    let apps = manifest["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| AppMeta {
            id: a["id"].as_str().unwrap().into(),
            name: a["name"].as_str().unwrap().into(),
            version: a["version"].as_str().unwrap().into(),
            description: a["description"].as_str().unwrap_or("").into(),
            color: 0xff00_0000
                | u32::from_str_radix(a["color"].as_str().unwrap().trim_start_matches('#'), 16)
                    .unwrap(),
            hidden: a["hidden"].as_bool().unwrap_or(false),
        })
        .collect();

    let sim = Sim {
        start: Instant::now(),
        screen: vec![],
        mocks: vec![],
        http: HashMap::new(),
        next: 1,
        battery: None,
    };
    let engine = wasmos_host::engine::engine_for("pulley64")?;
    let mut rt = Runtime::with_engine(engine, sim, FONT, &launcher)?;
    rt.set_installed(apps);

    let t0 = Instant::now();
    rt.start(&launcher, &load(dir, &launcher), "")?;
    println!("boot: {:?}", t0.elapsed());

    let touch = |rt: &mut Runtime<Sim>, phase, x, y| {
        let t = Instant::now();
        let _ = rt.dispatch(Event::Touch(vec![Touch { id: 0, phase, x, y }]));
        t.elapsed()
    };

    for line in std::fs::read_to_string(script)?.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        let n = |i: usize| w[i].parse::<i32>().unwrap();
        match w.first().copied() {
            None | Some("#") => {}
            Some("launch") => {
                let t = Instant::now();
                rt.start(w[1], &load(dir, w[1]), "")?;
                println!("launch {}: {:?}", w[1], t.elapsed());
            }
            Some("tap") => {
                touch(&mut rt, TouchPhase::Down, n(1), n(2));
                let d = touch(&mut rt, TouchPhase::Up, n(1), n(2));
                println!("tap {} {}: {:?}", n(1), n(2), d);
            }
            Some("drag") => {
                touch(&mut rt, TouchPhase::Down, n(1), n(2));
                for i in 1..=10 {
                    let x = n(1) + (n(3) - n(1)) * i / 10;
                    let y = n(2) + (n(4) - n(2)) * i / 10;
                    touch(&mut rt, TouchPhase::Move, x, y);
                }
                touch(&mut rt, TouchPhase::Up, n(3), n(4));
            }
            Some("home") => rt.go_home(),
            Some("wait") => {
                let until = Instant::now() + Duration::from_millis(n(1) as u64);
                while Instant::now() < until {
                    rt.tick()?;
                    run_switches(&mut rt, dir);
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            Some("mock") => {
                let body = std::fs::read(w[2])?;
                rt.platform().mocks.push((w[1].to_string(), body));
            }
            Some("battery") => {
                rt.platform().battery = Some(Battery {
                    voltage_mv: n(1) as u32,
                    current_ma: n(2),
                    cells: 2,
                });
            }
            Some("shot") => {
                let p = rt.platform();
                let file = std::fs::File::create(w[1])?;
                let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 1280, 720);
                enc.set_color(png::ColorType::Rgba);
                enc.write_header()?.write_image_data(&p.screen)?;
                println!("shot {}", w[1]);
            }
            Some(other) => panic!("unknown command {other}"),
        }
        run_switches(&mut rt, dir);
    }
    Ok(())
}
