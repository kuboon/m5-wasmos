//! アプリの実行管理: 起動・切り替え・イベント配送・タイマー・HTTP 進捗の通知。
//!
//! メインループ（実機 / sim）は次を繰り返す:
//!
//! ```ignore
//! rt.dispatch(Event::Touch(..));   // 入力があれば
//! rt.tick();                       // タイマー・frame・io
//! if let Some((id, args)) = rt.take_switch() { rt.start(&id, &load_cwasm(&id), &args) }
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Engine, Result, Store};

use crate::bindings::wasmos::os::system::AppInfo;
use crate::bindings::wasmos::os::types::Event;
use crate::bindings::App;
use crate::platform::Platform;
use crate::render::{Renderer, HEIGHT, WIDTH};

/// 1 回のコールバックに許す命令数（fuel）。使い切ると trap してランチャーへ戻る。
/// riscv32 は 64bit atomic が無く epoch 割り込みが使えないので fuel にしている
pub const CALLBACK_FUEL: u64 = 400_000_000;

#[derive(Clone)]
pub struct AppMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub color: u32,
    pub hidden: bool,
}

pub struct State<P: Platform> {
    pub p: P,
    pub table: ResourceTable,
    pub renderer: Renderer,
    pub(crate) timers: Vec<(u32, u64)>,
    pub(crate) next_timer: u32,
    pub(crate) frame_requested: bool,
    /// (http ハンドル, io を通知済みか)
    pub(crate) pending_http: Vec<(u32, bool)>,
    pub(crate) switch: Option<(String, String)>,
    /// アプリが `system.update` を呼んだ
    pub(crate) update_requested: bool,
    pub(crate) apps: Vec<AppMeta>,
    pub(crate) launcher: String,
    pub(crate) current: String,
}

impl<P: Platform> State<P> {
    pub(crate) fn log(&mut self, msg: &str) {
        let line = alloc::format!("[{}] {}", self.current, msg);
        self.p.log(&line);
    }

    pub(crate) fn installed(&self) -> Vec<AppInfo> {
        self.apps
            .iter()
            .filter(|a| !a.hidden)
            .map(|a| AppInfo {
                id: a.id.clone(),
                name: a.name.clone(),
                version: a.version.clone(),
                description: a.description.clone(),
                color: a.color,
            })
            .collect()
    }
}

pub struct Runtime<P: Platform> {
    engine: Engine,
    linker: Linker<State<P>>,
    /// start で作り直すので Option（常に Some）
    store: Option<Store<State<P>>>,
    app: Option<App>,
}

impl<P: Platform> Runtime<P> {
    pub fn new(platform: P, font: &'static [u8], launcher: &str) -> Result<Self> {
        Self::with_engine(crate::engine::engine()?, platform, font, launcher)
    }

    pub fn with_engine(
        engine: Engine,
        platform: P,
        font: &'static [u8],
        launcher: &str,
    ) -> Result<Self> {
        let mut linker = Linker::new(&engine);
        App::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
        let state = State {
            p: platform,
            table: ResourceTable::new(),
            renderer: Renderer::new(font),
            timers: Vec::new(),
            next_timer: 1,
            frame_requested: false,
            pending_http: Vec::new(),
            switch: None,
            update_requested: false,
            apps: Vec::new(),
            launcher: launcher.to_string(),
            current: String::new(),
        };
        let store = Store::new(&engine, state);
        Ok(Self {
            engine,
            linker,
            store: Some(store),
            app: None,
        })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    fn store(&mut self) -> &mut Store<State<P>> {
        self.store.as_mut().expect("store")
    }

    fn state(&self) -> &State<P> {
        self.store.as_ref().expect("store").data()
    }

    fn state_mut(&mut self) -> &mut State<P> {
        self.store().data_mut()
    }

    pub fn platform(&mut self) -> &mut P {
        &mut self.state_mut().p
    }

    pub fn current(&self) -> &str {
        &self.state().current
    }

    pub fn launcher(&self) -> &str {
        &self.state().launcher
    }

    pub fn set_installed(&mut self, apps: Vec<AppMeta>) {
        self.state_mut().apps = apps;
    }

    /// アプリではなくホスト自身が画面を描く（起動・更新の状態表示など）
    pub fn system_frame(&mut self, cmds: &[crate::bindings::wasmos::os::gfx::Cmd]) {
        self.state_mut().renderer.submit(cmds);
        self.present();
    }

    /// 今のアプリを止める（更新の前など）
    pub fn stop(&mut self) {
        self.app = None;
        let s = self.state_mut();
        for (h, _) in s.pending_http.drain(..) {
            s.p.http_cancel(h);
        }
        s.timers.clear();
        s.current = String::new();
    }

    /// 次に起動すべきアプリ（launch / exit / trap の結果）
    pub fn take_switch(&mut self) -> Option<(String, String)> {
        self.state_mut().switch.take()
    }

    /// アプリが更新を求めたか（読むと消える）。更新はプラットフォームの仕事
    pub fn take_update_request(&mut self) -> bool {
        core::mem::take(&mut self.state_mut().update_requested)
    }

    /// ランチャーへ戻す（システムジェスチャ）
    pub fn go_home(&mut self) {
        let s = self.state_mut();
        if s.current != s.launcher {
            s.switch = Some((s.launcher.clone(), String::new()));
        }
    }

    /// cwasm（tools/precompile で作ったもの）を起動する。今のアプリは捨てる
    pub fn start(&mut self, id: &str, cwasm: &[u8], args: &str) -> Result<()> {
        self.app = None;
        // Store を作り直して前のアプリのメモリとリソースを解放する
        let mut state = self.store.take().expect("store").into_data();
        for (h, _) in state.pending_http.drain(..) {
            state.p.http_cancel(h);
        }
        state.table = ResourceTable::new();
        state.timers.clear();
        state.frame_requested = false;
        state.switch = None;
        state.current = id.to_string();
        state.renderer.reset_app_state();
        let mut store = Store::new(&self.engine, state);
        store.set_fuel(CALLBACK_FUEL)?;
        self.store = Some(store);

        let component = unsafe { Component::deserialize(&self.engine, cwasm)? };
        let store = self.store.as_mut().expect("store");
        let app = App::instantiate(store, &component, &self.linker)?;
        self.app = Some(app);
        self.call(|app, store| app.call_init(store, args))
    }

    pub fn dispatch(&mut self, event: Event) -> Result<()> {
        self.call(|app, store| app.call_on_event(store, &event))
    }

    fn call(&mut self, f: impl FnOnce(&App, &mut Store<State<P>>) -> Result<()>) -> Result<()> {
        let Some(app) = self.app.as_ref() else {
            return Ok(());
        };
        let store = self.store.as_mut().expect("store");
        let result = store.set_fuel(CALLBACK_FUEL).and_then(|()| f(app, store));
        self.present();
        if let Err(e) = &result {
            let msg = alloc::format!("trap: {e:?}");
            let s = self.state_mut();
            s.log(&msg);
            // 壊れたインスタンスは使わない。ランチャー自身が落ちたら作り直す
            self.app = None;
            let s = self.state_mut();
            s.switch = Some((s.launcher.clone(), String::new()));
        }
        result
    }

    fn present(&mut self) {
        let s = self.state_mut();
        let dirty = s.renderer.take_dirty();
        if !dirty.is_empty() {
            s.p.present(s.renderer.pixmap.data(), WIDTH, HEIGHT, dirty);
        }
    }

    /// タイマー・frame・HTTP の進捗をイベントにして配る。メインループから頻繁に呼ぶ
    pub fn tick(&mut self) -> Result<()> {
        if self.app.is_none() {
            return Ok(());
        }
        let now = self.state().p.now_ms();

        // HTTP: 完了したものがあれば io を 1 回
        let mut io = false;
        {
            let s = self.state_mut();
            for i in 0..s.pending_http.len() {
                let (h, notified) = s.pending_http[i];
                if !notified && s.p.http_ready(h) {
                    s.pending_http[i].1 = true;
                    io = true;
                }
            }
        }
        if io {
            self.dispatch(Event::Io)?;
        }

        // タイマー
        let due: Vec<u32> = {
            let s = self.state_mut();
            let due = s
                .timers
                .iter()
                .filter(|t| t.1 <= now)
                .map(|t| t.0)
                .collect();
            s.timers.retain(|t| t.1 > now);
            due
        };
        for id in due {
            self.dispatch(Event::Timer(id))?;
        }

        // frame（vsync の代わりに tick ごと）
        if core::mem::take(&mut self.state_mut().frame_requested) {
            self.dispatch(Event::Frame(now))?;
        }
        Ok(())
    }
}
