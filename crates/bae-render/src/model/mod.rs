//! 3D model layers (AE's Advanced 3D: imported glTF / GLB / OBJ models): the
//! models read from disk ([`gltf`], [`obj`]) into one triangle list per
//! material, in AE's comp axes (x right, y down, z away from the viewer),
//! centred on the origin and scaled to a unit size (the layer places them).
//! Files load on a background thread ([`get`]); a viewer frame waits for
//! nothing (it is incomplete until the model is there).

pub mod extrude;
mod gltf;
mod obj;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use rustc_hash::FxHashMap;

/// A model ready to draw.
#[derive(Debug)]
pub struct Model {
    /// Triangle lists, one per material.
    pub parts: Vec<Part>,
    pub materials: Vec<Material>,
    /// Decoded textures (straight RGBA8, sRGB).
    pub textures: Vec<Texture>,
    /// [`Self::bounds`], once computed.
    pub bounds_cache: std::sync::OnceLock<[[f32; 3]; 2]>,
}

impl Model {
    /// The box around its triangles (model space): min and max corners.
    pub fn bounds(&self) -> [[f32; 3]; 2] {
        *self.bounds_cache.get_or_init(|| self.compute_bounds())
    }

    fn compute_bounds(&self) -> [[f32; 3]; 2] {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in self.parts.iter().flat_map(|p| p.positions.iter()) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if lo[0] > hi[0] {
            return [[0.0; 3]; 2];
        }
        [lo, hi]
    }

    /// The 8 corners of [`Self::bounds`], in comp space by `matrix`.
    pub fn corners(&self, matrix: &glam::DMat4) -> [glam::DVec3; 8] {
        let [lo, hi] = self.bounds();
        std::array::from_fn(|k| {
            matrix.transform_point3(glam::DVec3::new(
                f64::from(if k & 1 == 0 { lo[0] } else { hi[0] }),
                f64::from(if k & 2 == 0 { lo[1] } else { hi[1] }),
                f64::from(if k & 4 == 0 { lo[2] } else { hi[2] }),
            ))
        })
    }
}

/// Triangles sharing a material.
#[derive(Debug, Default)]
pub struct Part {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub material: usize,
}

/// A metallic-roughness material (glTF's PBR model; OBJ materials map onto it).
#[derive(Clone, Debug)]
pub struct Material {
    /// Linear RGBA.
    pub base_color: [f32; 4],
    pub base_texture: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
    /// Linear RGB.
    pub emissive: [f32; 3],
    pub double_sided: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [0.8, 0.8, 0.8, 1.0],
            base_texture: None,
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0; 3],
            double_sided: false,
        }
    }
}

/// A decoded texture.
#[derive(Debug)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Whether `path` names a model file this module reads.
pub fn is_model_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("glb" | "gltf" | "obj")
    )
}

/// Reads a model file (blocking).
pub fn load(path: &Path) -> Result<Model, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let mut model = match ext.as_str() {
        "glb" | "gltf" => gltf::load(path)?,
        "obj" => obj::load(path)?,
        _ => return Err(format!("{}: not a 3D model", path.display())),
    };
    model.normalize();
    Ok(model)
}

impl Model {
    /// AE's axes (glTF and OBJ are y-up with z toward the viewer: a half turn
    /// about x), centred on the bounding box and scaled to a largest side of
    /// 1; normals filled in where the file has none.
    fn normalize(&mut self) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for part in &mut self.parts {
            for p in &mut part.positions {
                *p = [p[0], -p[1], -p[2]];
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            for n in &mut part.normals {
                *n = [n[0], -n[1], -n[2]];
            }
            if part.normals.len() != part.positions.len() {
                part.normals = smooth_normals(&part.positions, &part.indices);
            }
            if part.uvs.len() != part.positions.len() {
                part.uvs = vec![[0.0; 2]; part.positions.len()];
            }
        }
        if lo[0] > hi[0] {
            return;
        }
        self.bounds_cache = Default::default();
        let centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
        let size = (0..3)
            .map(|k| hi[k] - lo[k])
            .fold(0.0f32, f32::max)
            .max(1e-9);
        for part in &mut self.parts {
            for p in &mut part.positions {
                *p = [0, 1, 2].map(|k| (p[k] - centre[k]) / size);
            }
        }
    }
}

/// Per-vertex normals averaged over the triangles around each vertex.
fn smooth_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0f32; 3]; positions.len()];
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| positions[i as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &i in t {
            let m = &mut normals[i as usize];
            for k in 0..3 {
                m[k] += n[k];
            }
        }
    }
    for n in &mut normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 0.0 {
            *n = n.map(|v| v / len);
        } else {
            *n = [0.0, 0.0, -1.0];
        }
    }
    normals
}

/// Decodes a PNG or JPEG texture.
fn decode_texture(bytes: &[u8]) -> Result<Texture, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    Ok(Texture {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

/// A model of the shared cache.
#[derive(Clone, Debug)]
pub enum State {
    Ready(Arc<Model>),
    Loading,
    Failed(String),
}

#[derive(Default)]
struct Cache {
    models: Mutex<FxHashMap<PathBuf, State>>,
    loaded: Condvar,
}

fn cache() -> &'static Cache {
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    CACHE.get_or_init(Cache::default)
}

/// The model at `path`: read once on a background thread and shared by
/// every renderer. `wait`: block until it is read (exports and the render
/// queue); otherwise `Loading` until then (viewers redraw when it is ready).
pub fn get(path: &Path, wait: bool) -> State {
    let shared = cache();
    let mut models = shared.models.lock().unwrap();
    if !models.contains_key(path) {
        models.insert(path.to_owned(), State::Loading);
        let owned = path.to_owned();
        let spawned = std::thread::Builder::new()
            .name("bae-model-load".into())
            .spawn(move || {
                let state = read(&owned);
                cache().models.lock().unwrap().insert(owned, state);
                cache().loaded.notify_all();
                bae_gpu::warm_up::request_redraw();
            });
        if spawned.is_err() {
            drop(models);
            let state = read(path);
            shared
                .models
                .lock()
                .unwrap()
                .insert(path.to_owned(), state.clone());
            return state;
        }
    }
    loop {
        let state = models.get(path).cloned().unwrap_or(State::Loading);
        if !wait || !matches!(state, State::Loading) {
            return state;
        }
        models = shared.loaded.wait(models).unwrap();
    }
}

fn read(path: &Path) -> State {
    match load(path) {
        Ok(model) => State::Ready(Arc::new(model)),
        Err(e) => State::Failed(e),
    }
}
