//! glTF 2.0 (`.gltf` with external, embedded or data-URI buffers, and binary
//! `.glb`): the default scene's meshes through the node hierarchy, their
//! metallic-roughness materials and base colour textures. Animation, skins and
//! morph targets are not read (the model is drawn at rest).

use std::path::Path;

use glam::{Mat3, Mat4, Quat, Vec3};
use serde_json::Value as Json;

use super::{Material, Model, Part, Texture};

const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

pub(super) fn load(path: &Path) -> Result<Model, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let (json, bin) = if bytes.len() >= 12 && u32_at(&bytes, 0) == GLB_MAGIC {
        glb_chunks(&bytes)?
    } else {
        (bytes.as_slice(), None)
    };
    let doc: Json = serde_json::from_slice(json).map_err(|e| format!("glTF: {e}"))?;
    let reader = Reader::new(&doc, dir, bin)?;
    reader.model()
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The JSON and BIN chunks of a `.glb`.
fn glb_chunks(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), String> {
    let mut at = 12;
    let (mut json, mut bin) = (None, None);
    while at + 8 <= bytes.len() {
        let len = u32_at(bytes, at) as usize;
        let kind = u32_at(bytes, at + 4);
        let data = bytes
            .get(at + 8..at + 8 + len)
            .ok_or("GLB: chunk past the end of the file")?;
        match kind {
            CHUNK_JSON => json = Some(data),
            CHUNK_BIN => bin = Some(data),
            _ => {}
        }
        at += 8 + len;
    }
    Ok((json.ok_or("GLB: no JSON chunk")?, bin))
}

/// Standard base64 (data URIs).
fn base64(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return Err("glTF: bad base64 data".into()),
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

struct Reader<'a> {
    doc: &'a Json,
    dir: &'a Path,
    buffers: Vec<Vec<u8>>,
}

fn arr<'a>(j: &'a Json, key: &str) -> &'a [Json] {
    j.get(key)
        .and_then(Json::as_array)
        .map_or(&[], Vec::as_slice)
}

fn num(j: &Json, key: &str, default: f64) -> f64 {
    j.get(key).and_then(Json::as_f64).unwrap_or(default)
}

fn index(j: &Json, key: &str) -> Option<usize> {
    j.get(key).and_then(Json::as_u64).map(|v| v as usize)
}

impl<'a> Reader<'a> {
    fn new(doc: &'a Json, dir: &'a Path, bin: Option<&[u8]>) -> Result<Self, String> {
        let mut buffers = Vec::new();
        for (i, b) in arr(doc, "buffers").iter().enumerate() {
            let data = match b.get("uri").and_then(Json::as_str) {
                Some(uri) => Self::uri(dir, uri)?,
                None if i == 0 => bin.ok_or("glTF: buffer 0 without data")?.to_vec(),
                None => return Err("glTF: a buffer without data".into()),
            };
            buffers.push(data);
        }
        Ok(Self { doc, dir, buffers })
    }

    fn uri(dir: &Path, uri: &str) -> Result<Vec<u8>, String> {
        if let Some(rest) = uri.strip_prefix("data:") {
            let (_, data) = rest
                .split_once(";base64,")
                .ok_or("glTF: unsupported data URI")?;
            return base64(data);
        }
        let name = uri.replace("%20", " ");
        std::fs::read(dir.join(&name)).map_err(|e| format!("{name}: {e}"))
    }

    /// The bytes of buffer view `i`, and its stride.
    fn view(&self, i: usize) -> Result<(&[u8], Option<usize>), String> {
        let v = arr(self.doc, "bufferViews")
            .get(i)
            .ok_or("glTF: bad buffer view")?;
        let buffer = self
            .buffers
            .get(index(v, "buffer").unwrap_or(0))
            .ok_or("glTF: bad buffer")?;
        let at = num(v, "byteOffset", 0.0) as usize;
        let len = num(v, "byteLength", 0.0) as usize;
        let data = buffer
            .get(at..at + len)
            .ok_or("glTF: buffer view past its buffer")?;
        Ok((data, index(v, "byteStride")))
    }

    /// Accessor `i` as floats, `n` per element (normalised integers scaled
    /// to 0–1 or −1–1).
    fn floats(&self, i: usize) -> Result<(Vec<f32>, usize), String> {
        let a = arr(self.doc, "accessors")
            .get(i)
            .ok_or("glTF: bad accessor")?;
        let count = num(a, "count", 0.0) as usize;
        let n = match a.get("type").and_then(Json::as_str).unwrap_or("SCALAR") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            other => return Err(format!("glTF: accessor type {other}")),
        };
        let kind = num(a, "componentType", 5126.0) as u32;
        let normalized = a.get("normalized").and_then(Json::as_bool).unwrap_or(false);
        let size = match kind {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(format!("glTF: component type {kind}")),
        };
        let mut out = vec![0.0f32; count * n];
        let Some(view) = index(a, "bufferView") else {
            return Ok((out, n));
        };
        let (data, stride) = self.view(view)?;
        let offset = num(a, "byteOffset", 0.0) as usize;
        let stride = stride.unwrap_or(size * n);
        for e in 0..count {
            for c in 0..n {
                let at = offset + e * stride + c * size;
                let b = data
                    .get(at..at + size)
                    .ok_or("glTF: accessor past its view")?;
                out[e * n + c] = match kind {
                    5126 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                    5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
                    5123 => {
                        let v = f32::from(u16::from_le_bytes([b[0], b[1]]));
                        if normalized { v / 65535.0 } else { v }
                    }
                    5122 => {
                        let v = f32::from(i16::from_le_bytes([b[0], b[1]]));
                        if normalized {
                            (v / 32767.0).max(-1.0)
                        } else {
                            v
                        }
                    }
                    5121 => {
                        let v = f32::from(b[0]);
                        if normalized { v / 255.0 } else { v }
                    }
                    _ => {
                        let v = f32::from(b[0] as i8);
                        if normalized { (v / 127.0).max(-1.0) } else { v }
                    }
                };
            }
        }
        Ok((out, n))
    }

    /// Accessor `i` as vertex indices (exact for 32-bit indices).
    fn indices(&self, i: usize) -> Result<Vec<u32>, String> {
        let a = arr(self.doc, "accessors")
            .get(i)
            .ok_or("glTF: bad accessor")?;
        if num(a, "componentType", 5125.0) as u32 == 5125 {
            let count = num(a, "count", 0.0) as usize;
            let view = index(a, "bufferView").ok_or("glTF: indices without data")?;
            let (data, stride) = self.view(view)?;
            let offset = num(a, "byteOffset", 0.0) as usize;
            let stride = stride.unwrap_or(4);
            return (0..count)
                .map(|e| {
                    let at = offset + e * stride;
                    let b = data.get(at..at + 4).ok_or("glTF: accessor past its view")?;
                    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                })
                .collect();
        }
        Ok(self.floats(i)?.0.into_iter().map(|v| v as u32).collect())
    }

    fn materials(&self) -> Vec<Material> {
        arr(self.doc, "materials")
            .iter()
            .map(|m| {
                let pbr = m.get("pbrMetallicRoughness").cloned().unwrap_or(Json::Null);
                let base = arr(&pbr, "baseColorFactor");
                let base_color = if base.len() == 4 {
                    [0, 1, 2, 3].map(|k| base[k].as_f64().unwrap_or(1.0) as f32)
                } else {
                    [1.0; 4]
                };
                let emissive = arr(m, "emissiveFactor");
                Material {
                    base_color,
                    base_texture: pbr.get("baseColorTexture").and_then(|t| index(t, "index")),
                    metallic: num(&pbr, "metallicFactor", 1.0) as f32,
                    roughness: num(&pbr, "roughnessFactor", 1.0) as f32,
                    emissive: if emissive.len() == 3 {
                        [0, 1, 2].map(|k| emissive[k].as_f64().unwrap_or(0.0) as f32)
                    } else {
                        [0.0; 3]
                    },
                    double_sided: m
                        .get("doubleSided")
                        .and_then(Json::as_bool)
                        .unwrap_or(false),
                }
            })
            .collect()
    }

    /// Textures by glTF texture index (unreadable images become white).
    fn textures(&self) -> Vec<Texture> {
        arr(self.doc, "textures")
            .iter()
            .map(|t| {
                let image = index(t, "source").and_then(|i| arr(self.doc, "images").get(i));
                let bytes = image.and_then(|img| {
                    if let Some(uri) = img.get("uri").and_then(Json::as_str) {
                        Self::uri(self.dir, uri).ok()
                    } else {
                        let view = index(img, "bufferView")?;
                        self.view(view).ok().map(|(d, _)| d.to_vec())
                    }
                });
                bytes
                    .and_then(|b| super::decode_texture(&b).ok())
                    .unwrap_or_else(|| Texture {
                        width: 1,
                        height: 1,
                        rgba: vec![255; 4],
                    })
            })
            .collect()
    }

    fn model(&self) -> Result<Model, String> {
        let mut materials = self.materials();
        let default_material = materials.len();
        materials.push(Material::default());
        let mut parts: Vec<Part> = Vec::new();
        let scene = index(self.doc, "scene").unwrap_or(0);
        let roots: Vec<usize> = match arr(self.doc, "scenes").get(scene) {
            Some(s) => arr(s, "nodes")
                .iter()
                .filter_map(|n| n.as_u64().map(|v| v as usize))
                .collect(),
            None => (0..arr(self.doc, "nodes").len()).collect(),
        };
        let mut stack: Vec<(usize, Mat4, usize)> =
            roots.into_iter().map(|n| (n, Mat4::IDENTITY, 0)).collect();
        while let Some((n, parent, depth)) = stack.pop() {
            let Some(node) = arr(self.doc, "nodes").get(n) else {
                continue;
            };
            if depth > 64 {
                return Err("glTF: node hierarchy too deep".into());
            }
            let world = parent * local_matrix(node);
            if let Some(mesh) = index(node, "mesh").and_then(|m| arr(self.doc, "meshes").get(m)) {
                for prim in arr(mesh, "primitives") {
                    if num(prim, "mode", 4.0) as u32 != 4 {
                        continue;
                    }
                    let material = index(prim, "material")
                        .filter(|&m| m < default_material)
                        .unwrap_or(default_material);
                    parts.push(self.primitive(prim, &world, material)?);
                }
            }
            for c in arr(node, "children").iter().filter_map(Json::as_u64) {
                stack.push((c as usize, world, depth + 1));
            }
        }
        Ok(Model {
            parts,
            materials,
            textures: self.textures(),
            bounds_cache: Default::default(),
        })
    }

    fn primitive(&self, prim: &Json, world: &Mat4, material: usize) -> Result<Part, String> {
        let attrs = prim.get("attributes").cloned().unwrap_or(Json::Null);
        let position = index(&attrs, "POSITION").ok_or("glTF: a primitive without positions")?;
        let (p, _) = self.floats(position)?;
        let normal_matrix = Mat3::from_mat4(*world).inverse().transpose();
        let positions: Vec<[f32; 3]> = p
            .as_chunks::<3>()
            .0
            .iter()
            .map(|v| {
                world
                    .transform_point3(Vec3::new(v[0], v[1], v[2]))
                    .to_array()
            })
            .collect();
        let normals = match index(&attrs, "NORMAL") {
            Some(i) => self
                .floats(i)?
                .0
                .as_chunks::<3>()
                .0
                .iter()
                .map(|v| {
                    (normal_matrix * Vec3::new(v[0], v[1], v[2]))
                        .normalize_or_zero()
                        .to_array()
                })
                .collect(),
            None => Vec::new(),
        };
        let uvs = match index(&attrs, "TEXCOORD_0") {
            Some(i) => self
                .floats(i)?
                .0
                .as_chunks::<2>()
                .0
                .iter()
                .map(|v| [v[0], v[1]])
                .collect(),
            None => Vec::new(),
        };
        let indices = match index(prim, "indices") {
            Some(i) => self.indices(i)?,
            None => (0..positions.len() as u32).collect(),
        };
        if indices.iter().any(|&i| i as usize >= positions.len()) {
            return Err("glTF: an index past the vertices".into());
        }
        Ok(Part {
            positions,
            normals,
            uvs,
            indices,
            material,
        })
    }
}

/// A node's own transform: `matrix`, or translation · rotation · scale.
fn local_matrix(node: &Json) -> Mat4 {
    let m = arr(node, "matrix");
    if m.len() == 16 {
        let v: Vec<f32> = m.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
        return Mat4::from_cols_slice(&v);
    }
    let vec = |key: &str, default: [f32; 4], n: usize| {
        let a = arr(node, key);
        let mut out = default;
        if a.len() == n {
            for (k, x) in a.iter().enumerate() {
                out[k] = x.as_f64().unwrap_or(0.0) as f32;
            }
        }
        out
    };
    let t = vec("translation", [0.0; 4], 3);
    let r = vec("rotation", [0.0, 0.0, 0.0, 1.0], 4);
    let s = vec("scale", [1.0; 4], 3);
    Mat4::from_scale_rotation_translation(
        Vec3::new(s[0], s[1], s[2]),
        Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize(),
        Vec3::new(t[0], t[1], t[2]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_triangle_through_its_node() {
        // Three positions (float32) then three u16 indices, in a data URI.
        let mut bin: Vec<u8> = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend(v.to_le_bytes());
        }
        for i in [0u16, 1, 2] {
            bin.extend(i.to_le_bytes());
        }
        const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut text = String::new();
        for chunk in bin.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for k in 0..4 {
                if k <= chunk.len() {
                    text.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
                } else {
                    text.push('=');
                }
            }
        }
        let doc = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[0,0,5]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],
            "buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{text}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":6}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                         {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}]}}"#,
            len = bin.len()
        );
        let dir = std::env::temp_dir().join(format!("bae-gltf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.gltf");
        std::fs::write(&path, doc).unwrap();
        let model = load(&path).unwrap();
        assert_eq!(model.parts.len(), 1);
        assert_eq!(model.parts[0].indices, [0, 1, 2]);
        assert_eq!(
            model.parts[0].positions[1],
            [1.0, 0.0, 5.0],
            "placed by its node"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn base64_decodes_data_uris() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("AAECAw==").unwrap(), [0, 1, 2, 3]);
    }
}
