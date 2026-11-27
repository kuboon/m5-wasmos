//! `gfx.submit` のコマンドを RGBA のピクセルバッファに描く。
//!
//! 図形は tiny-skia、文字は ab_glyph（グリフはサイズごとにキャッシュ）。
//! クリップは矩形だけなので、図形には tiny-skia の Mask、文字と画像には矩形で効かせる。

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont};
use tiny_skia::{
    FillRule, LineCap, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Rect as SkRect, Stroke,
    Transform,
};

use crate::bindings::wasmos::os::gfx::{Cmd, TextAlign};
use crate::bindings::wasmos::os::types::Rect;
use crate::platform::IRect;

/// 論理画面（縦置き。Tab5 のパネル本来の向き）
pub const WIDTH: u32 = 720;
pub const HEIGHT: u32 = 1280;

struct Glyph {
    /// グリフ原点（ペン位置, ベースライン）からの左上
    left: i32,
    top: i32,
    w: u32,
    h: u32,
    coverage: Vec<u8>,
}

pub struct Renderer {
    pub pixmap: Pixmap,
    font: FontRef<'static>,
    glyphs: BTreeMap<(u16, u16), Option<Glyph>>,
    images: BTreeMap<u32, Pixmap>,
    next_image: u32,
    clips: Vec<(IRect, Option<Mask>)>,
    /// 前回 present してから描いた範囲
    pub dirty: IRect,
}

fn paint_of(argb: u32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    );
    p.anti_alias = true;
    p
}

fn irect(r: &Rect) -> IRect {
    IRect::new(r.x, r.y, r.w, r.h)
}

/// 角丸矩形（radius は w/h の半分まで）
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<tiny_skia::Path> {
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    if r < 0.5 {
        return Some(PathBuilder::from_rect(SkRect::from_xywh(x, y, w, h)?));
    }
    // 円弧を 3 次ベジェで近似
    let k = r * 0.552_284_8;
    let (x1, y1) = (x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x1 - r, y);
    pb.cubic_to(x1 - r + k, y, x1, y + r - k, x1, y + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x + r, y1);
    pb.cubic_to(x + r - k, y1, x, y1 - r + k, x, y1 - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

impl Renderer {
    pub fn new(font: &'static [u8]) -> Self {
        Self {
            pixmap: Pixmap::new(WIDTH, HEIGHT).expect("pixmap"),
            font: FontRef::try_from_slice(font).expect("font"),
            glyphs: BTreeMap::new(),
            images: BTreeMap::new(),
            next_image: 1,
            clips: Vec::new(),
            dirty: IRect::new(0, 0, WIDTH as i32, HEIGHT as i32),
        }
    }

    /// アプリを切り替えるときに呼ぶ（画像を捨てる。フォントのキャッシュは残す）
    pub fn reset_app_state(&mut self) {
        self.images.clear();
        self.next_image = 1;
        self.clips.clear();
    }

    fn screen() -> IRect {
        IRect::new(0, 0, WIDTH as i32, HEIGHT as i32)
    }

    fn clip_rect(&self) -> IRect {
        self.clips.last().map(|c| c.0).unwrap_or(Self::screen())
    }

    fn touch(&mut self, r: IRect) {
        let r = r.intersect(&self.clip_rect());
        self.dirty = self.dirty.union(&r);
    }

    pub fn take_dirty(&mut self) -> IRect {
        core::mem::replace(&mut self.dirty, IRect::new(0, 0, 0, 0))
    }

    // --- 画像 ---------------------------------------------------------------

    pub fn add_rgba(&mut self, w: u32, h: u32, px: &[u8]) -> u32 {
        let id = self.next_image;
        self.next_image += 1;
        if let Some(mut pm) = Pixmap::new(w.max(1), h.max(1)) {
            // 非 premultiplied → premultiplied
            for (d, s) in pm.data_mut().chunks_exact_mut(4).zip(px.chunks_exact(4)) {
                let a = s[3] as u16;
                d[0] = ((s[0] as u16 * a + 127) / 255) as u8;
                d[1] = ((s[1] as u16 * a + 127) / 255) as u8;
                d[2] = ((s[2] as u16 * a + 127) / 255) as u8;
                d[3] = s[3];
            }
            self.images.insert(id, pm);
        }
        id
    }

    /// PNG / JPEG のデコードは未実装。id だけ払い出し、image-size は None になる
    pub fn reserve_image(&mut self) -> u32 {
        let id = self.next_image;
        self.next_image += 1;
        id
    }

    pub fn image_size(&self, id: u32) -> Option<(u32, u32)> {
        self.images.get(&id).map(|p| (p.width(), p.height()))
    }

    pub fn drop_image(&mut self, id: u32) {
        self.images.remove(&id);
    }

    // --- 文字 ---------------------------------------------------------------

    fn glyph<'a>(
        font: &FontRef<'static>,
        glyphs: &'a mut BTreeMap<(u16, u16), Option<Glyph>>,
        id: GlyphId,
        size: u16,
    ) -> Option<&'a Glyph> {
        glyphs
            .entry((id.0, size))
            .or_insert_with(|| {
                let g = id
                    .with_scale_and_position(PxScale::from(size as f32), ab_glyph::point(0.0, 0.0));
                let outlined = font.outline_glyph(g)?;
                let b = outlined.px_bounds();
                let (w, h) = (b.width() as u32, b.height() as u32);
                let mut coverage = alloc::vec![0u8; (w * h) as usize];
                outlined.draw(|x, y, c| {
                    if x < w && y < h {
                        coverage[(y * w + x) as usize] = (c.clamp(0.0, 1.0) * 255.0) as u8;
                    }
                });
                Some(Glyph {
                    left: b.min.x as i32,
                    top: b.min.y as i32,
                    w,
                    h,
                    coverage,
                })
            })
            .as_ref()
    }

    pub fn measure_text(&self, text: &str, size: u16) -> u32 {
        let sf = self.font.as_scaled(PxScale::from(size as f32));
        let mut x = 0.0f32;
        let mut prev: Option<GlyphId> = None;
        for c in text.chars() {
            let id = sf.glyph_id(c);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            x += sf.h_advance(id);
            prev = Some(id);
        }
        libm::ceilf(x) as u32
    }

    fn draw_text(&mut self, x: i32, y: i32, size: u16, argb: u32, align: TextAlign, text: &str) {
        let width = self.measure_text(text, size) as i32;
        let x0 = match align {
            TextAlign::Left => x,
            TextAlign::Center => x - width / 2,
            TextAlign::Right => x - width,
        };
        let (ascent, descent) = {
            let sf = self.font.as_scaled(PxScale::from(size as f32));
            (sf.ascent(), sf.descent())
        };
        let baseline = y + libm::roundf(ascent) as i32;
        self.touch(IRect::new(
            x0,
            y,
            width,
            libm::ceilf(ascent - descent) as i32,
        ));

        let clip = self.clip_rect();
        let (r, g, b, a) = (
            (argb >> 16) & 255,
            (argb >> 8) & 255,
            argb & 255,
            argb >> 24,
        );
        let mut pen = x0 as f32;
        let mut prev: Option<GlyphId> = None;
        let stride = WIDTH as i32;
        for c in text.chars() {
            let (id, adv, kern) = {
                let sf = self.font.as_scaled(PxScale::from(size as f32));
                let id = sf.glyph_id(c);
                (
                    id,
                    sf.h_advance(id),
                    prev.map(|p| sf.kern(p, id)).unwrap_or(0.0),
                )
            };
            pen += kern;
            let gx = libm::roundf(pen) as i32;
            if let Some(gl) = Self::glyph(&self.font, &mut self.glyphs, id, size) {
                let (left, top, w, h) = (gl.left, gl.top, gl.w as i32, gl.h as i32);
                let cov = &gl.coverage;
                let data = self.pixmap.data_mut();
                for yy in 0..h {
                    let py = baseline + top + yy;
                    if py < clip.y || py >= clip.y + clip.h {
                        continue;
                    }
                    for xx in 0..w {
                        let px = gx + left + xx;
                        if px < clip.x || px >= clip.x + clip.w {
                            continue;
                        }
                        let cv = cov[(yy * w + xx) as usize] as u32;
                        if cv == 0 {
                            continue;
                        }
                        let sa = cv * a / 255;
                        let i = ((py * stride + px) * 4) as usize;
                        let inv = 255 - sa;
                        data[i] = ((r * sa + data[i] as u32 * inv) / 255) as u8;
                        data[i + 1] = ((g * sa + data[i + 1] as u32 * inv) / 255) as u8;
                        data[i + 2] = ((b * sa + data[i + 2] as u32 * inv) / 255) as u8;
                        data[i + 3] = (sa + data[i + 3] as u32 * inv / 255) as u8;
                    }
                }
            }
            pen += adv;
            prev = Some(id);
        }
    }

    // --- コマンド -----------------------------------------------------------

    pub fn submit(&mut self, cmds: &[Cmd]) {
        for cmd in cmds {
            self.exec(cmd);
        }
        // 閉じ忘れの clip は持ち越さない
        self.clips.clear();
    }

    fn exec(&mut self, cmd: &Cmd) {
        let id = Transform::identity();
        match cmd {
            Cmd::Clear(c) => {
                let r = self.clip_rect();
                if self.clips.is_empty() {
                    let (a, rr, g, b) =
                        ((c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, *c as u8);
                    self.pixmap.fill(tiny_skia::Color::from_rgba8(rr, g, b, a));
                } else if let Some(sk) =
                    SkRect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
                {
                    self.pixmap.fill_rect(sk, &paint_of(*c), id, None);
                }
                self.touch(r);
            }
            Cmd::FillRect(f) => {
                let r = &f.rect;
                if let Some(path) = rounded_rect(
                    r.x as f32,
                    r.y as f32,
                    r.w as f32,
                    r.h as f32,
                    f.radius as f32,
                ) {
                    let mask = self.clips.last().and_then(|c| c.1.as_ref());
                    self.pixmap
                        .fill_path(&path, &paint_of(f.color), FillRule::Winding, id, mask);
                    self.touch(irect(r));
                }
            }
            Cmd::StrokeRect(s) => {
                let r = &s.rect;
                let half = s.width as f32 / 2.0;
                if let Some(path) = rounded_rect(
                    r.x as f32 + half,
                    r.y as f32 + half,
                    r.w as f32 - s.width as f32,
                    r.h as f32 - s.width as f32,
                    (s.radius as f32 - half).max(0.0),
                ) {
                    let stroke = Stroke {
                        width: s.width as f32,
                        ..Stroke::default()
                    };
                    let mask = self.clips.last().and_then(|c| c.1.as_ref());
                    self.pixmap
                        .stroke_path(&path, &paint_of(s.color), &stroke, id, mask);
                    self.touch(irect(r));
                }
            }
            Cmd::Line(l) => {
                let mut pb = PathBuilder::new();
                pb.move_to(l.x0 as f32, l.y0 as f32);
                pb.line_to(l.x1 as f32, l.y1 as f32);
                if let Some(path) = pb.finish() {
                    let stroke = Stroke {
                        width: l.width as f32,
                        line_cap: LineCap::Round,
                        ..Stroke::default()
                    };
                    let mask = self.clips.last().and_then(|c| c.1.as_ref());
                    self.pixmap
                        .stroke_path(&path, &paint_of(l.color), &stroke, id, mask);
                    let w = l.width as i32;
                    let (x0, y0) = (l.x0.min(l.x1) - w, l.y0.min(l.y1) - w);
                    self.touch(IRect::new(
                        x0,
                        y0,
                        (l.x1 - l.x0).abs() + 2 * w,
                        (l.y1 - l.y0).abs() + 2 * w,
                    ));
                }
            }
            Cmd::FillCircle(c) => {
                if let Some(path) = PathBuilder::from_circle(c.x as f32, c.y as f32, c.r as f32) {
                    let mask = self.clips.last().and_then(|c| c.1.as_ref());
                    self.pixmap
                        .fill_path(&path, &paint_of(c.color), FillRule::Winding, id, mask);
                    let r = c.r as i32;
                    self.touch(IRect::new(c.x - r - 1, c.y - r - 1, 2 * r + 2, 2 * r + 2));
                }
            }
            Cmd::Text(t) => self.draw_text(t.x, t.y, t.size, t.color, t.align, &t.text),
            Cmd::Image(im) => {
                let Some(src) = self.images.get(&im.id) else {
                    return;
                };
                let cropped;
                let src = match &im.src {
                    Some(r) => {
                        match tiny_skia::IntRect::from_xywh(r.x, r.y, r.w as u32, r.h as u32)
                            .and_then(|ir| src.clone_rect(ir))
                        {
                            Some(p) => {
                                cropped = p;
                                &cropped
                            }
                            None => return,
                        }
                    }
                    None => src,
                };
                let d = &im.dst;
                let sx = d.w as f32 / src.width() as f32;
                let sy = d.h as f32 / src.height() as f32;
                let paint = PixmapPaint {
                    opacity: im.alpha as f32 / 255.0,
                    ..PixmapPaint::default()
                };
                let tf = Transform::from_row(sx, 0.0, 0.0, sy, d.x as f32, d.y as f32);
                let mask = self.clips.last().and_then(|c| c.1.as_ref());
                self.pixmap
                    .draw_pixmap(0, 0, src.as_ref(), &paint, tf, mask);
                self.touch(irect(d));
            }
            Cmd::PushClip(r) => {
                let r = irect(r).intersect(&self.clip_rect());
                let mut mask = Mask::new(WIDTH, HEIGHT);
                if let (Some(m), Some(sk)) = (
                    mask.as_mut(),
                    SkRect::from_xywh(r.x as f32, r.y as f32, r.w.max(0) as f32, r.h.max(0) as f32),
                ) {
                    m.fill_path(&PathBuilder::from_rect(sk), FillRule::Winding, false, id);
                } else if let Some(m) = mask.as_mut() {
                    // 空のクリップ: 何も描かない
                    m.data_mut().fill(0);
                }
                self.clips.push((r, mask));
            }
            Cmd::PopClip => {
                self.clips.pop();
            }
        }
    }
}
