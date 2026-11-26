//! Native font families, PostScript names and rasterized menu samples.
use bae_text::{FontFace, FontMatch, FontSystem};
use egui::{Color32, ColorImage, Context, Id, Rect, TextureHandle, TextureOptions, pos2, vec2};
use std::collections::HashMap;
use std::sync::{Arc, Once, OnceLock};

const WIDTH: usize = 72;
const HEIGHT: usize = 42;
const COLUMNS: usize = 8;
const ROWS: usize = 8;
const PER_PAGE: usize = COLUMNS * ROWS;

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
    sheets: Vec<OnceLock<Arc<ColorImage>>>,
}

pub(crate) static CATALOG: OnceLock<Catalog> = OnceLock::new();

impl Catalog {
    fn names(fonts: &mut FontSystem) -> Self {
        let families: Vec<_> = fonts
            .families()
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
        let sheets = (0..families.len().div_ceil(PER_PAGE))
            .map(|_| OnceLock::new())
            .collect();
        Self {
            families,
            by_family,
            by_face: OnceLock::new(),
            sheets,
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

    fn load_faces(&self, fonts: &mut FontSystem) {
        let mut by_face = HashMap::new();
        for (i, family) in self.families.iter().enumerate() {
            let faces = fonts.faces(&family.name);
            if let FontMatch::Found(face) = fonts.resolve(&family.name) {
                let _ = family.regular.set(face.postscript_name);
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

    fn load_samples(&self, fonts: &mut FontSystem) {
        let size = [COLUMNS * WIDTH, ROWS * HEIGHT];
        for (slot, families) in self.sheets.iter().zip(self.families.chunks(PER_PAGE)) {
            let mut sheet = ColorImage::filled(size, Color32::TRANSPARENT);
            for (i, family) in families.iter().enumerate() {
                let sample = bae_text::render::font_menu_sample(
                    fonts,
                    &family.name,
                    WIDTH as u16,
                    HEIGHT as u16,
                );
                let x = i % COLUMNS * WIDTH;
                let y = i / COLUMNS * HEIGHT;
                for (row, alpha) in sample.as_chunks::<WIDTH>().0.iter().enumerate() {
                    let start = (y + row) * size[0] + x;
                    for (pixel, &a) in sheet.pixels[start..start + WIDTH].iter_mut().zip(alpha) {
                        *pixel = Color32::from_white_alpha(a);
                    }
                }
            }
            let _ = slot.set(Arc::new(sheet));
            bae_gpu::warm_up::request_redraw();
        }
    }
}

/// The same low-priority CPU job serves headless workspaces and GPU warm-up.
pub(super) fn start() {
    static START: Once = Once::new();
    START.call_once(|| {
        if let Err(error) = std::thread::Builder::new()
            .name("bae-font-menu".into())
            .spawn(|| {
                bae_gpu::warm_up::lower_priority();
                let mut fonts = FontSystem::new();
                let catalog = CATALOG.get_or_init(|| Catalog::names(&mut fonts));
                bae_gpu::warm_up::request_redraw();
                catalog.load_faces(&mut fonts);
                bae_gpu::warm_up::request_redraw();
                catalog.load_samples(&mut fonts);
                bae_gpu::warm_up::request_redraw();
            })
        {
            tracing::warn!(%error, "could not start font menu warm-up");
        }
    });
}

pub(crate) struct WarmUp;
impl bae_gpu::WarmUp for WarmUp {
    fn name(&self) -> &str {
        "Character panel font previews"
    }
    fn priority(&self) -> u8 {
        85
    }
    fn run(&self, _: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        start();
        Ok(())
    }
}

fn sheets_id() -> Id {
    Id::new("bae.character.font-preview-textures")
}

/// Stage one prepared sheet per redraw, before the user opens the font menu.
pub(crate) fn upload(ctx: &Context) {
    let Some(catalog) = CATALOG.get() else { return };
    let id = sheets_id();
    let index = ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<TextureHandle>>(id).len());
    if let Some(sheet) = catalog.sheets.get(index).and_then(OnceLock::get) {
        let texture = ctx.load_texture(
            format!("font previews {index}"),
            sheet.clone(),
            TextureOptions::LINEAR,
        );
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<Vec<TextureHandle>>(id)
                .push(texture)
        });
        if index + 1 < catalog.sheets.len() {
            ctx.request_repaint();
        }
    }
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
    let size = vec2(height * WIDTH as f32 / HEIGHT as f32, height);
    bae_ui::widgets::clipped_text(
        ui.painter(),
        pos2(row.left() + 4.0, row.center().y),
        name,
        bae_ui::theme::body(),
        color,
        row.width() - size.x - 12.0,
    );
    let texture = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<TextureHandle>>(sheets_id())
            .get(index / PER_PAGE)
            .map(TextureHandle::id)
    });
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
