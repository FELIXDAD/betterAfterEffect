//! Native font families, PostScript names and menu samples. The list is indexed
//! once in the background on the Character panel's font system; a page of
//! samples is drawn there when the menu first shows it, at the row's pixel size
//! (sharp on Retina), and only its texture is kept.
use bae_text::{FontFace, FontMatch, FontSystem};
use egui::{Color32, ColorImage, Context, Id, Rect, TextureHandle, TextureOptions, pos2, vec2};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, OnceLock, mpsc};

/// A sample's width over its height.
const ASPECT: f32 = 72.0 / 42.0;
const COLUMNS: usize = 8;
const ROWS: usize = 8;
const PER_PAGE: usize = COLUMNS * ROWS;
/// Sample heights in pixels (a page stays within 2048 pixels wide).
const MIN_PX: u32 = 8;
const MAX_PX: u32 = 140;

pub(crate) struct Family {
    pub name: String,
    faces: OnceLock<Vec<FontFace>>,
    regular: OnceLock<String>,
}

impl Family {
    pub fn faces(&self) -> &[FontFace] {
        self.faces.get().map(Vec::as_slice).unwrap_or_default()
    }

    pub fn default_face(&self) -> &str {
        self.regular
            .get()
            .map_or(self.name.as_str(), String::as_str)
    }
}

pub(crate) struct Catalog {
    pub families: Vec<Family>,
    by_family: HashMap<String, usize>,
    by_face: OnceLock<HashMap<String, (usize, usize)>>,
    /// Pages to draw: (page, pixel height).
    requests: Mutex<mpsc::Sender<(usize, u32)>>,
    asked: Mutex<HashSet<(usize, u32)>>,
    /// Drawn pages waiting for their upload, with their pixel height.
    drawn: Mutex<HashMap<usize, (u32, ColorImage)>>,
}

pub(crate) static CATALOG: OnceLock<Catalog> = OnceLock::new();

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Catalog {
    fn new(names: Vec<String>, requests: mpsc::Sender<(usize, u32)>) -> Self {
        let families: Vec<_> = names
            .into_iter()
            .map(|name| Family {
                name,
                faces: OnceLock::new(),
                regular: OnceLock::new(),
            })
            .collect();
        let by_family = families
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.to_lowercase(), i))
            .collect();
        Self {
            families,
            by_family,
            by_face: OnceLock::new(),
            requests: Mutex::new(requests),
            asked: Mutex::default(),
            drawn: Mutex::default(),
        }
    }

    pub fn indexed(&self) -> bool {
        self.by_face.get().is_some()
    }

    pub fn lookup(&self, name: &str) -> Option<(usize, Option<&FontFace>)> {
        let key = name.to_lowercase();
        if let Some(&i) = self.by_family.get(&key) {
            let f = &self.families[i];
            return Some((
                i,
                f.faces()
                    .iter()
                    .find(|face| face.postscript_name == f.default_face()),
            ));
        }
        let &(i, j) = self.by_face.get()?.get(&key)?;
        Some((i, self.families[i].faces().get(j)))
    }

    /// The font system is locked per family: text hit tests never wait long.
    fn load_faces(&self, fonts: &Mutex<FontSystem>) {
        let mut by_face = HashMap::new();
        for (i, family) in self.families.iter().enumerate() {
            let (faces, regular) = {
                let mut fonts = lock(fonts);
                let regular = match fonts.resolve(&family.name) {
                    FontMatch::Found(face) => Some(face.postscript_name),
                    FontMatch::Missing => None,
                };
                (fonts.faces(&family.name), regular)
            };
            if let Some(regular) = regular {
                let _ = family.regular.set(regular);
            }
            for (j, face) in faces.iter().enumerate() {
                by_face
                    .entry(face.postscript_name.to_lowercase())
                    .or_insert((i, j));
            }
            let _ = family.faces.set(faces);
        }
        let _ = self.by_face.set(by_face);
    }

    fn draw(&self, fonts: &Mutex<FontSystem>, page: usize, px: u32) {
        let (w, h) = (((px as f32) * ASPECT).round() as usize, px as usize);
        let size = [COLUMNS * w, ROWS * h];
        let mut sheet = ColorImage::filled(size, Color32::TRANSPARENT);
        let families = self.families.iter().skip(page * PER_PAGE).take(PER_PAGE);
        for (i, family) in families.enumerate() {
            let sample = bae_text::render::font_menu_sample(
                &mut lock(fonts),
                &family.name,
                w as u16,
                h as u16,
            );
            let (x, y) = (i % COLUMNS * w, i / COLUMNS * h);
            for (row, alpha) in sample.chunks_exact(w).enumerate() {
                let start = (y + row) * size[0] + x;
                for (pixel, &a) in sheet.pixels[start..start + w].iter_mut().zip(alpha) {
                    *pixel = Color32::from_white_alpha(a);
                }
            }
        }
        lock(&self.drawn).insert(page, (px, sheet));
        bae_gpu::warm_up::request_redraw();
    }

    fn request(&self, page: usize, px: u32) {
        if lock(&self.asked).insert((page, px)) {
            let _ = lock(&self.requests).send((page, px));
        }
    }
}

/// Indexes the families on `fonts` (the font scan thread), then draws the
/// sample pages the menu asks for.
pub(super) fn index(fonts: &Mutex<FontSystem>) {
    let (requests, pages) = mpsc::channel();
    let names = lock(fonts).families();
    let catalog = CATALOG.get_or_init(|| Catalog::new(names, requests));
    bae_gpu::warm_up::request_redraw();
    catalog.load_faces(fonts);
    bae_gpu::warm_up::request_redraw();
    for (page, px) in pages {
        catalog.draw(fonts, page, px);
    }
}

fn pages_id() -> Id {
    Id::new("bae.character.font-preview-textures")
}

/// Uploaded sample pages: page → (pixel height, texture).
type Pages = HashMap<usize, (u32, TextureHandle)>;

/// The texture of `page` at `px` pixels per sample row; an older size is shown
/// until that one is drawn.
fn page_texture(ctx: &Context, page: usize, px: u32) -> Option<egui::TextureId> {
    let catalog = CATALOG.get()?;
    let id = pages_id();
    let have = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Pages>(id)
            .get(&page)
            .map(|(h, t)| (*h, t.id()))
    });
    if let Some((h, texture)) = have
        && h == px
    {
        return Some(texture);
    }
    let drawn = {
        let mut drawn = lock(&catalog.drawn);
        match drawn.get(&page) {
            Some((h, _)) if *h == px => drawn.remove(&page),
            _ => None,
        }
    };
    if let Some((_, sheet)) = drawn {
        lock(&catalog.asked).remove(&(page, px));
        let texture = ctx.load_texture(
            format!("font previews {page}"),
            sheet,
            TextureOptions::LINEAR,
        );
        let texture_id = texture.id();
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<Pages>(id)
                .insert(page, (px, texture))
        });
        return Some(texture_id);
    }
    catalog.request(page, px);
    have.map(|(_, texture)| texture)
}

pub(super) fn row(
    ui: &mut egui::Ui,
    name: &str,
    selected: bool,
    index: usize,
    height: f32,
) -> egui::Response {
    use bae_ui::a11y::Accessible as _;
    let (row, response) = ui.allocate_exact_size(vec2(250.0, height), egui::Sense::click());
    let response = response.labeled_toggle(egui::WidgetType::SelectableLabel, name, selected);
    let color = ui.visuals().text_color();
    if selected || response.hovered() || response.has_focus() {
        let visuals = ui.style().interact_selectable(&response, selected);
        ui.painter().rect_filled(row, 3.0, visuals.bg_fill);
    }
    let size = vec2(height * ASPECT, height);
    bae_ui::widgets::clipped_text(
        ui.painter(),
        pos2(row.left() + 4.0, row.center().y),
        name,
        bae_ui::theme::body(),
        color,
        row.width() - size.x - 12.0,
    );
    let px = ((height * ui.ctx().pixels_per_point()).round() as u32).clamp(MIN_PX, MAX_PX);
    let texture = page_texture(ui.ctx(), index / PER_PAGE, px);
    if let Some(texture) = texture {
        let x = (index % COLUMNS) as f32 / COLUMNS as f32;
        let y = (index % PER_PAGE / COLUMNS) as f32 / ROWS as f32;
        let uv = Rect::from_min_size(pos2(x, y), vec2(1.0 / COLUMNS as f32, 1.0 / ROWS as f32));
        let rect =
            Rect::from_center_size(pos2(row.right() - size.x * 0.5 - 3.0, row.center().y), size);
        ui.painter().image(texture, rect, uv, color);
    }
    response
}
