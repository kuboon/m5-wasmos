//! ホストがハードウェア（または Linux のテスト環境）に求めるもの
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn intersect(&self, o: &IRect) -> IRect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        IRect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }
    pub fn union(&self, o: &IRect) -> IRect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        let x1 = (self.x + self.w).max(o.x + o.w);
        let y1 = (self.y + self.h).max(o.y + o.h);
        IRect::new(x0, y0, x1 - x0, y1 - y0)
    }
}

pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

/// 電池の状態（WIT の `system.battery` と同じ）
#[derive(Clone, Copy, Debug)]
pub struct Battery {
    pub voltage_mv: u32,
    /// + が充電、- が放電
    pub current_ma: i32,
    pub cells: u8,
}

pub trait Platform: 'static {
    fn log(&mut self, message: &str);
    /// 起動からの ms
    fn now_ms(&self) -> u64;
    /// UNIX epoch ms（未同期なら None）
    fn wall_ms(&self) -> Option<u64>;

    /// 電池（無い・読めないなら None）
    fn battery(&mut self) -> Option<Battery> {
        None
    }

    /// HTTP を非同期に開始してハンドルを返す。TLS はプラットフォームが担当
    fn http_start(&mut self, req: HttpRequest) -> u32;
    /// 完了（成功・失敗とも）していれば true
    fn http_ready(&mut self, handle: u32) -> bool;
    /// 完了していれば結果を取り出す（ハンドルは解放される）
    fn http_take(&mut self, handle: u32) -> Option<Result<HttpResponse, String>>;
    fn http_cancel(&mut self, handle: u32);

    /// 論理画面（width x height, RGBA8 premultiplied）の dirty 部分を表示する
    fn present(&mut self, rgba: &[u8], width: u32, height: u32, dirty: IRect);
}
