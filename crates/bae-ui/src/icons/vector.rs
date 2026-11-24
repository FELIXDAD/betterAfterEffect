//! Fill-path icons: loading, rasterizing and drawing through a tinted atlas.
//!
//! `tools/icons/build_icons.py` converts the icon SVGs listed in
//! `assets/icons/manifest.json` into `assets/icons/icons.bin`: per icon a square
//! design grid and one or more layers of filled outlines (moves, lines, quadratic
//! and cubic curves, nonzero rule). Nothing is parsed from SVG at run time.
//!
//! An icon is rasterized the first time it is drawn at a given pixel size, with an
//! exact-area coverage rasterizer, into a white-with-alpha atlas page. Drawing is
//! one textured quad per layer whose vertex color tints the coverage, so the theme
//! color applies at no cost and every scale factor (1×, 125 %, 150 %, 2×) gets
//! pixel-exact glyphs instead of a resampled bitmap.

use std::collections::HashMap;
use std::io::Read as _;
use std::sync::{Arc, Mutex, OnceLock};

use egui::epaint::{Mesh, TextureHandle};
use egui::{
    Color32, ColorImage, Context, Id, Painter, Pos2, Rect, Shape, TextureOptions, Vec2, pos2,
};

use super::Icon;

/// The generated icon data, zlib-compressed (see the module docs to rebuild it).
static PACKED: &[u8] = include_bytes!("../../../../assets/icons/icons.bin");

/// Inflates the icon data once; empty if it is unreadable.
fn blob() -> &'static [u8] {
    static RAW: OnceLock<Box<[u8]>> = OnceLock::new();
    RAW.get_or_init(|| {
        let mut out = Vec::new();
        match flate2::read::ZlibDecoder::new(PACKED).read_to_end(&mut out) {
            Ok(_) => out.into_boxed_slice(),
            Err(err) => {
                tracing::error!("icon data does not inflate ({err}); using fallback icons");
                Box::default()
            }
        }
    })
}

/// Sub-units per design unit in the path words.
const FIXED: f32 = 32.0;
const OP_MOVE: i16 = 0;
const OP_LINE: i16 = 1;
const OP_QUAD: i16 = 2;
const OP_CUBIC: i16 = 3;
const OP_CLOSE: i16 = 4;

/// Side of one atlas page in texels.
const PAGE: usize = 512;
/// Largest glyph cached in the atlas (in pixels); bigger requests draw at this size.
const MAX_PX: usize = 256;
/// Glyphs smaller than this many pixels get the coverage boost.
const BOOST_BELOW_PX: usize = 14;

pub(super) struct IconData {
    /// Side of the square design grid in design units.
    view: f32,
    layers: Vec<Layer>,
}

struct Layer {
    /// Fixed layer color; `None` takes the caller's (theme) color.
    color: Option<Color32>,
    /// Layer opacity, 0–255.
    alpha: u8,
    /// Path words (little-endian i16), borrowed from the blob.
    words: &'static [u8],
}

impl Layer {
    fn words(&self) -> impl Iterator<Item = i16> + '_ {
        self.words
            .as_chunks::<2>()
            .0
            .iter()
            .map(|w| i16::from_le_bytes(*w))
    }
}

/// Error from [`parse`]; the blob is generated, so this only fires on a broken build.
#[derive(Debug)]
pub(super) struct BadBlob(&'static str);

/// Parses the icon blob into a table indexed by `Icon as usize`.
pub(super) fn parse(blob: &'static [u8]) -> Result<Vec<Option<IconData>>, BadBlob> {
    let mut r = Reader { b: blob, at: 0 };
    if r.take(4)? != b"BAEI" {
        return Err(BadBlob("magic"));
    }
    if r.u8()? != 1 {
        return Err(BadBlob("version"));
    }
    let mut by_name: HashMap<&str, IconData> = HashMap::new();
    for _ in 0..r.u16()? {
        let len = r.u8()? as usize;
        let name = std::str::from_utf8(r.take(len)?).map_err(|_| BadBlob("name"))?;
        let view = f32::from(r.u16()?);
        let mut layers = Vec::new();
        for _ in 0..r.u8()? {
            let rgba = r.u32()?;
            let alpha = r.u8()?;
            let count = r.u16()? as usize;
            let words = r.take(count * 2)?;
            let color = (rgba != 0).then(|| {
                let [cr, cg, cb, _] = rgba.to_be_bytes();
                Color32::from_rgb(cr, cg, cb)
            });
            layers.push(Layer {
                color,
                alpha,
                words,
            });
        }
        if view <= 0.0 {
            return Err(BadBlob("view box"));
        }
        by_name.insert(name, IconData { view, layers });
    }
    Ok(Icon::ALL
        .iter()
        .map(|icon| by_name.remove(format!("{icon:?}").as_str()))
        .collect())
}

struct Reader {
    b: &'static [u8],
    at: usize,
}

impl Reader {
    fn take(&mut self, n: usize) -> Result<&'static [u8], BadBlob> {
        let s = self
            .b
            .get(self.at..self.at + n)
            .ok_or(BadBlob("truncated"))?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, BadBlob> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, BadBlob> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, BadBlob> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
}

/// The parsed icon table; empty (all painter fallbacks) if the blob is broken.
pub(super) fn table() -> &'static [Option<IconData>] {
    static TABLE: OnceLock<Vec<Option<IconData>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        parse(blob()).unwrap_or_else(|err| {
            tracing::error!("icon data is unreadable ({}); using fallback icons", err.0);
            Vec::new()
        })
    })
}

// ------------------------------------------------------------- rasterizer ----

/// Exact-area coverage accumulation (the font-rs algorithm): every edge adds its
/// signed area to the cells it crosses; a running sum along each row gives the
/// winding coverage, so overlapping same-direction outlines union and reversed
/// outlines cut holes (the nonzero rule).
struct Raster {
    w: usize,
    h: usize,
    acc: Vec<f32>,
}

impl Raster {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            acc: vec![0.0; w * h + 4],
        }
    }

    fn line(&mut self, p0: Pos2, p1: Pos2) {
        if (p0.y - p1.y).abs() <= f32::EPSILON {
            return;
        }
        let (dir, p0, p1) = if p0.y < p1.y {
            (1.0, p0, p1)
        } else {
            (-1.0, p1, p0)
        };
        let dxdy = (p1.x - p0.x) / (p1.y - p0.y);
        let mut x = p0.x;
        if p0.y < 0.0 {
            x -= p0.y * dxdy;
        }
        let max_x = self.w as f32 - 1e-3;
        let y_end = (p1.y.ceil().max(0.0) as usize).min(self.h);
        for y in (p0.y.max(0.0) as usize)..y_end {
            let row = y * self.w;
            let dy = ((y + 1) as f32).min(p1.y) - (y as f32).max(p0.y);
            let x_next = x + dxdy * dy;
            let d = dy * dir;
            let (x0, x1) = if x < x_next { (x, x_next) } else { (x_next, x) };
            let (x0, x1) = (x0.clamp(0.0, max_x), x1.clamp(0.0, max_x));
            let x0_floor = x0.floor();
            let x0i = x0_floor as usize;
            let x1_ceil = x1.ceil();
            let x1i = x1_ceil as usize;
            let a = &mut self.acc;
            if x1i <= x0i + 1 {
                let xmf = 0.5 * (x0 + x1) - x0_floor;
                a[row + x0i] += d - d * xmf;
                a[row + x0i + 1] += d * xmf;
            } else {
                let s = (x1 - x0).recip();
                let x0f = x0 - x0_floor;
                let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
                let x1f = x1 - x1_ceil + 1.0;
                let am = 0.5 * s * x1f * x1f;
                a[row + x0i] += d * a0;
                if x1i == x0i + 2 {
                    a[row + x0i + 1] += d * (1.0 - a0 - am);
                } else {
                    let a1 = s * (1.5 - x0f);
                    a[row + x0i + 1] += d * (a1 - a0);
                    for xi in x0i + 2..x1i - 1 {
                        a[row + xi] += d * s;
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                    a[row + x1i - 1] += d * (1.0 - a2 - am);
                }
                a[row + x1i] += d * am;
            }
            x = x_next;
        }
    }

    /// Flattens a cubic into lines with ~0.1 px tolerance.
    fn cubic(&mut self, p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2) {
        let dd = (p0 - 2.0 * p1.to_vec2() + p2.to_vec2())
            .to_vec2()
            .length()
            .max((p1 - 2.0 * p2.to_vec2() + p3.to_vec2()).to_vec2().length());
        let n = ((dd * 7.5).sqrt().ceil() as usize).clamp(1, 64);
        let mut prev = p0;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let mt = 1.0 - t;
            let p = (p0.to_vec2() * (mt * mt * mt)
                + p1.to_vec2() * (3.0 * mt * mt * t)
                + p2.to_vec2() * (3.0 * mt * t * t)
                + p3.to_vec2() * (t * t * t))
                .to_pos2();
            self.line(prev, p);
            prev = p;
        }
    }

    /// Coverage per pixel, 0–255. With `boost`, partial coverage is raised
    /// (`1 - (1 - c)²`, as egui does for text on dark backgrounds) so hairline
    /// strokes stay legible on very small glyphs.
    fn coverage(&self, boost: bool) -> Vec<u8> {
        let mut sum = 0.0f32;
        self.acc[..self.w * self.h]
            .iter()
            .map(|a| {
                sum += a;
                let c = sum.abs().min(1.0);
                let c = if boost { c * (2.0 - c) } else { c };
                (c * 255.0 + 0.5) as u8
            })
            .collect()
    }
}

/// Rasterizes one layer into a `px`×`px` coverage mask.
fn rasterize(layer: &Layer, view: f32, px: usize) -> Vec<u8> {
    let scale = px as f32 / (view * FIXED);
    let mut r = Raster::new(px, px);
    let mut words = layer.words();
    let next = |words: &mut dyn Iterator<Item = i16>| -> Option<Pos2> {
        let x = words.next()?;
        let y = words.next()?;
        Some(pos2(f32::from(x) * scale, f32::from(y) * scale))
    };
    let mut start = Pos2::ZERO;
    let mut cur = Pos2::ZERO;
    while let Some(op) = words.next() {
        match op {
            OP_MOVE => {
                if cur != start {
                    r.line(cur, start);
                }
                let Some(p) = next(&mut words) else { break };
                start = p;
                cur = p;
            }
            OP_LINE => {
                let Some(p) = next(&mut words) else { break };
                r.line(cur, p);
                cur = p;
            }
            OP_QUAD => {
                let (Some(c), Some(p)) = (next(&mut words), next(&mut words)) else {
                    break;
                };
                let c1 = cur + (c - cur) * (2.0 / 3.0);
                let c2 = p + (c - p) * (2.0 / 3.0);
                r.cubic(cur, c1, c2, p);
                cur = p;
            }
            OP_CUBIC => {
                let (Some(c1), Some(c2), Some(p)) =
                    (next(&mut words), next(&mut words), next(&mut words))
                else {
                    break;
                };
                r.cubic(cur, c1, c2, p);
                cur = p;
            }
            OP_CLOSE => {
                r.line(cur, start);
                cur = start;
            }
            _ => break,
        }
    }
    if cur != start {
        r.line(cur, start);
    }
    r.coverage(px < BOOST_BELOW_PX)
}

// ------------------------------------------------------------------ atlas ----

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    icon: u16,
    layer: u8,
    px: u16,
}

#[derive(Clone, Copy)]
struct Slot {
    page: usize,
    uv: Rect,
}

struct Page {
    texture: TextureHandle,
    /// Shelf packer: current shelf top, height and fill position.
    shelf_y: usize,
    shelf_h: usize,
    cursor_x: usize,
}

#[derive(Default)]
struct Atlas {
    pages: Vec<Page>,
    slots: HashMap<Key, Slot>,
}

impl Atlas {
    fn slot(&mut self, ctx: &Context, key: Key, data: &IconData) -> Slot {
        if let Some(slot) = self.slots.get(&key) {
            return *slot;
        }
        let px = usize::from(key.px);
        let mask = rasterize(&data.layers[usize::from(key.layer)], data.view, px);
        let pixels = mask.into_iter().map(Color32::from_white_alpha).collect();
        let image = ColorImage::new([px, px], pixels);
        let cell = px + 1; // one texel of transparent gutter for linear filtering
        let (page, x, y) = self.allocate(ctx, cell);
        self.pages[page]
            .texture
            .set_partial([x, y], image, TextureOptions::LINEAR);
        let uv = Rect::from_min_size(
            pos2(x as f32 / PAGE as f32, y as f32 / PAGE as f32),
            Vec2::splat(px as f32 / PAGE as f32),
        );
        let slot = Slot { page, uv };
        self.slots.insert(key, slot);
        slot
    }

    fn allocate(&mut self, ctx: &Context, cell: usize) -> (usize, usize, usize) {
        for (i, p) in self.pages.iter_mut().enumerate() {
            if p.cursor_x + cell <= PAGE && cell <= p.shelf_h {
                let at = (i, p.cursor_x, p.shelf_y);
                p.cursor_x += cell;
                return at;
            }
            // Open a new shelf below; the rest of the current one stays unused.
            let next_y = p.shelf_y + p.shelf_h;
            if next_y + cell <= PAGE {
                p.shelf_y = next_y;
                p.shelf_h = cell;
                p.cursor_x = cell;
                return (i, 0, next_y);
            }
        }
        let texture = ctx.load_texture(
            format!("bae-icons-{}", self.pages.len()),
            ColorImage::filled([PAGE, PAGE], Color32::TRANSPARENT),
            TextureOptions::LINEAR,
        );
        self.pages.push(Page {
            texture,
            shelf_y: 0,
            shelf_h: cell,
            cursor_x: cell,
        });
        (self.pages.len() - 1, 0, 0)
    }
}

fn atlas(ctx: &Context) -> Arc<Mutex<Atlas>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_insert_with(Id::new("bae_ui::icons::atlas"), || {
            Arc::new(Mutex::new(Atlas::default()))
        })
        .clone()
    })
}

/// Draws `icon` centered in `rect` from its fill-path data (nothing if the data
/// is missing, which only a broken build can cause).
pub(super) fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let Some(Some(data)) = table().get(icon as usize) else {
        return;
    };
    let ppp = painter.pixels_per_point();
    let side = rect.width().min(rect.height());
    let px = (side * ppp).round() as usize;
    if px == 0 {
        return;
    }
    // Snap to the pixel grid so each texel lands on exactly one screen pixel.
    let size = px as f32 / ppp;
    let min = rect.center() - Vec2::splat(size / 2.0);
    let min = pos2((min.x * ppp).round() / ppp, (min.y * ppp).round() / ppp);
    let quad = Rect::from_min_size(min, Vec2::splat(size));

    let ctx = painter.ctx();
    let atlas = atlas(ctx);
    let Ok(mut atlas) = atlas.lock() else {
        return;
    };
    let tint_alpha = f32::from(color.a()) / 255.0;
    for (i, layer) in data.layers.iter().enumerate() {
        let key = Key {
            icon: icon as u16,
            layer: i as u8,
            // Huge requests reuse the largest cached raster, scaled up.
            px: px.min(MAX_PX) as u16,
        };
        let slot = atlas.slot(ctx, key, data);
        let base = match layer.color {
            None => color,
            Some(c) => c.gamma_multiply(tint_alpha),
        };
        // The shell applies Preferences > Appearance only to meshes on egui's
        // own texture (`theme::apply_appearance`), so atlas quads adjust here.
        let tint = crate::theme::adjust(
            base.gamma_multiply(f32::from(layer.alpha) / 255.0),
            crate::theme::appearance(),
        );
        let mut mesh = Mesh::with_texture(atlas.pages[slot.page].texture.id());
        mesh.add_rect_with_uv(quad, slot.uv, tint);
        painter.add(Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_parses() {
        let table = parse(blob()).expect("assets/icons/icons.bin parses");
        assert_eq!(table.len(), Icon::ALL.len());
        let missing: Vec<_> = Icon::ALL
            .iter()
            .zip(&table)
            .filter(|(_, d)| d.is_none())
            .map(|(i, _)| *i)
            .collect();
        assert!(
            missing.is_empty(),
            "not in assets/icons/manifest.json: {missing:?}"
        );
    }

    /// Paints every icon once at `ppp` and returns the frame output.
    fn paint_all(ctx: &Context, ppp: f32, tint: Color32) -> egui::FullOutput {
        ctx.set_pixels_per_point(ppp);
        // An empty first frame applies the scale factor.
        output_drop(ctx.run_ui(egui::RawInput::default(), |_| {}));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            for (i, icon) in Icon::ALL.iter().enumerate() {
                let at = pos2((i % 20) as f32 * 24.0, (i / 20) as f32 * 24.0);
                let rect = Rect::from_min_size(at, Vec2::splat(16.0));
                super::super::paint(ui.painter(), rect, *icon, tint);
            }
        })
    }

    fn icon_meshes(out: &egui::FullOutput) -> Vec<&Mesh> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Mesh(m) if m.texture_id != egui::TextureId::default() => Some(&**m),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_icon_paints_at_every_scale() {
        let tint = Color32::from_rgb(10, 200, 30);
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            let ctx = crate::testing::ctx_with_fonts();
            let out = paint_all(&ctx, ppp, tint);
            let layers: usize = table().iter().flatten().map(|d| d.layers.len()).sum();
            let meshes = icon_meshes(&out);
            assert_eq!(meshes.len(), layers, "one quad per layer at {ppp}x");
            // The atlas page was uploaded and holds coverage.
            let uploaded = out
                .textures_delta
                .set
                .values()
                .flat_map(|d| d.iter())
                .any(|d| match &d.image {
                    egui::ImageData::Color(img) => img.pixels.iter().any(|p| p.a() > 0),
                });
            assert!(uploaded, "atlas has coverage at {ppp}x");
            // Quads are pixel-snapped and exactly 16 pt × ppp pixels wide.
            for m in &meshes {
                let r = Rect::from_points(&m.vertices.iter().map(|v| v.pos).collect::<Vec<_>>());
                assert!((r.width() * ppp - (16.0 * ppp).round()).abs() < 1e-3);
                assert!(((r.min.x * ppp) - (r.min.x * ppp).round()).abs() < 1e-3);
            }
            output_drop(out);
        }
    }

    #[test]
    fn theme_layers_take_the_tint_and_fixed_layers_keep_their_color() {
        let ctx = crate::testing::ctx_with_fonts();
        let tint = Color32::from_rgb(10, 200, 30);
        let out = paint_all(&ctx, 1.0, tint);
        let colors: Vec<Color32> = icon_meshes(&out)
            .iter()
            .map(|m| m.vertices[0].color)
            .collect();
        let mut expected = Vec::new();
        for data in table().iter().flatten() {
            for layer in &data.layers {
                let base = layer.color.unwrap_or(tint);
                expected.push(base.gamma_multiply(f32::from(layer.alpha) / 255.0));
            }
        }
        assert_eq!(colors, expected);
        output_drop(out);
    }

    #[test]
    fn second_frame_reuses_the_atlas() {
        let ctx = crate::testing::ctx_with_fonts();
        let first = paint_all(&ctx, 1.0, Color32::WHITE);
        output_drop(first);
        let again = ctx.run_ui(egui::RawInput::default(), |ui| {
            for icon in Icon::ALL {
                super::super::paint(
                    ui.painter(),
                    Rect::from_min_size(Pos2::ZERO, Vec2::splat(16.0)),
                    *icon,
                    Color32::RED,
                );
            }
        });
        assert!(
            again.textures_delta.set.is_empty(),
            "no re-upload for cached sizes or a new tint"
        );
        output_drop(again);
    }

    #[test]
    fn appearance_adjusts_theme_tints_but_not_fixed_colors() {
        use crate::theme::{Appearance, adjust, set_appearance};
        let ctx = crate::testing::ctx_with_fonts();
        let gray = Color32::from_gray(0xb1);
        set_appearance(Appearance::Light);
        let out = paint_all(&ctx, 1.0, gray);
        set_appearance(Appearance::Darkest);
        let colors: Vec<Color32> = icon_meshes(&out)
            .iter()
            .map(|m| m.vertices[0].color)
            .collect();
        output_drop(out);
        let mut expected = Vec::new();
        for data in table().iter().flatten() {
            for layer in &data.layers {
                let base = layer.color.unwrap_or(gray);
                let c = base.gamma_multiply(f32::from(layer.alpha) / 255.0);
                expected.push(adjust(c, Appearance::Light));
            }
        }
        assert_eq!(colors, expected);
        // A light gray icon becomes dark on the light theme.
        assert!(adjust(gray, Appearance::Light).r() < 0x80);
    }

    fn output_drop(out: egui::FullOutput) {
        out.drop_without_applying_deltas();
    }

    #[test]
    fn square_covers_its_area() {
        // A 2×2 px square at (1,1) in a 4 px raster, outline in pixel units.
        let mut r = Raster::new(4, 4);
        let pts = [
            pos2(1.0, 1.0),
            pos2(3.0, 1.0),
            pos2(3.0, 3.0),
            pos2(1.0, 3.0),
        ];
        for i in 0..4 {
            r.line(pts[i], pts[(i + 1) % 4]);
        }
        let cov = r.coverage(false);
        #[rustfmt::skip]
        let want = [
            0, 0, 0, 0,
            0, 255, 255, 0,
            0, 255, 255, 0,
            0, 0, 0, 0,
        ];
        assert_eq!(cov, want);
    }

    #[test]
    fn half_pixel_edges_are_antialiased() {
        let mut r = Raster::new(2, 1);
        let pts = [
            pos2(0.0, 0.0),
            pos2(1.5, 0.0),
            pos2(1.5, 1.0),
            pos2(0.0, 1.0),
        ];
        for i in 0..4 {
            r.line(pts[i], pts[(i + 1) % 4]);
        }
        assert_eq!(r.coverage(false), vec![255, 128]);
    }

    #[test]
    fn reversed_inner_outline_is_a_hole() {
        let mut r = Raster::new(5, 5);
        let outer = [
            pos2(0.0, 0.0),
            pos2(5.0, 0.0),
            pos2(5.0, 5.0),
            pos2(0.0, 5.0),
        ];
        let inner = [
            pos2(1.0, 1.0),
            pos2(1.0, 4.0),
            pos2(4.0, 4.0),
            pos2(4.0, 1.0),
        ];
        for poly in [outer, inner] {
            for i in 0..4 {
                r.line(poly[i], poly[(i + 1) % 4]);
            }
        }
        let cov = r.coverage(false);
        assert_eq!(cov[0], 255);
        assert_eq!(cov[2 * 5 + 2], 0);
    }
}
