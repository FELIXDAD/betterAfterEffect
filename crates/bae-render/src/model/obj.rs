//! Wavefront OBJ with MTL materials: positions, texture coordinates and
//! normals, polygons fanned into triangles; `Kd` / `map_Kd` / `d` / `Ns`
//! (and the PBR extension's `Pm` / `Pr`) mapped onto a metallic-roughness
//! material.

use std::collections::HashMap;
use std::path::Path;

use super::{Material, Model, Part};

pub(super) fn load(path: &Path) -> Result<Model, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut v: Vec<[f32; 3]> = Vec::new();
    let mut vt: Vec<[f32; 2]> = Vec::new();
    let mut vn: Vec<[f32; 3]> = Vec::new();
    let mut materials: Vec<Material> = vec![Material::default()];
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut textures = Vec::new();
    let mut parts: Vec<Part> = Vec::new();
    // Vertex (position, uv, normal) → index, per part.
    let mut seen: Vec<HashMap<(i64, i64, i64), u32>> = Vec::new();
    let mut current = 0usize;
    let part_for =
        |material: usize, parts: &mut Vec<Part>, seen: &mut Vec<HashMap<_, _>>| -> usize {
            if let Some(i) = parts.iter().position(|p| p.material == material) {
                return i;
            }
            parts.push(Part {
                material,
                ..Part::default()
            });
            seen.push(HashMap::new());
            parts.len() - 1
        };
    let floats = |rest: &str| -> Vec<f32> {
        rest.split_whitespace()
            .filter_map(|s| s.parse::<f32>().ok())
            .collect()
    };
    for line in text.lines() {
        let line = line.trim();
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match key {
            "v" => {
                let f = floats(rest);
                if f.len() >= 3 {
                    v.push([f[0], f[1], f[2]]);
                }
            }
            "vt" => {
                let f = floats(rest);
                if f.len() >= 2 {
                    // OBJ's v grows upward.
                    vt.push([f[0], 1.0 - f[1]]);
                }
            }
            "vn" => {
                let f = floats(rest);
                if f.len() >= 3 {
                    vn.push([f[0], f[1], f[2]]);
                }
            }
            "mtllib" => {
                for name in rest.split_whitespace() {
                    if let Ok(mtl) = std::fs::read_to_string(dir.join(name)) {
                        read_mtl(&mtl, dir, &mut materials, &mut names, &mut textures);
                    }
                }
            }
            "usemtl" => current = names.get(rest.trim()).copied().unwrap_or(0),
            "f" => {
                let corners: Vec<(i64, i64, i64)> = rest
                    .split_whitespace()
                    .map(|c| {
                        let mut it = c.split('/');
                        let mut next =
                            || it.next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                        (next(), next(), next())
                    })
                    .collect();
                if corners.len() < 3 {
                    continue;
                }
                let p = part_for(current, &mut parts, &mut seen);
                let mut ids = Vec::with_capacity(corners.len());
                for &(pi, ti, ni) in &corners {
                    // 1-based; negative counts from the end.
                    let resolve = |i: i64, len: usize| -> Option<usize> {
                        if i > 0 {
                            Some(i as usize - 1)
                        } else if i < 0 {
                            len.checked_sub((-i) as usize)
                        } else {
                            None
                        }
                    };
                    let Some(pos) = resolve(pi, v.len()).and_then(|i| v.get(i)) else {
                        return Err(format!(
                            "{}: a face refers to a missing vertex",
                            path.display()
                        ));
                    };
                    let key = (pi, ti, ni);
                    let part = &mut parts[p];
                    let id = *seen[p].entry(key).or_insert_with(|| {
                        part.positions.push(*pos);
                        part.uvs.push(
                            resolve(ti, vt.len())
                                .and_then(|i| vt.get(i))
                                .copied()
                                .unwrap_or([0.0; 2]),
                        );
                        if let Some(n) = resolve(ni, vn.len()).and_then(|i| vn.get(i)) {
                            part.normals.push(*n);
                        }
                        part.positions.len() as u32 - 1
                    });
                    ids.push(id);
                }
                for k in 1..ids.len() - 1 {
                    parts[p].indices.extend([ids[0], ids[k], ids[k + 1]]);
                }
            }
            _ => {}
        }
    }
    // A part that lacks some normals gets them all computed.
    for part in &mut parts {
        if part.normals.len() != part.positions.len() {
            part.normals.clear();
        }
    }
    Ok(Model {
        parts,
        materials,
        textures,
    })
}

fn read_mtl(
    text: &str,
    dir: &Path,
    materials: &mut Vec<Material>,
    names: &mut HashMap<String, usize>,
    textures: &mut Vec<super::Texture>,
) {
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let line = line.trim();
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        if key == "newmtl" {
            materials.push(Material {
                roughness: 0.6,
                ..Material::default()
            });
            current = Some(materials.len() - 1);
            names.insert(rest.to_owned(), materials.len() - 1);
            continue;
        }
        let Some(m) = current.map(|i| &mut materials[i]) else {
            continue;
        };
        let f: Vec<f32> = rest
            .split_whitespace()
            .filter_map(|s| s.parse::<f32>().ok())
            .collect();
        match key {
            "Kd" if f.len() >= 3 => {
                m.base_color = [
                    srgb_to_linear(f[0]),
                    srgb_to_linear(f[1]),
                    srgb_to_linear(f[2]),
                    m.base_color[3],
                ];
            }
            "d" if !f.is_empty() => m.base_color[3] = f[0],
            "Tr" if !f.is_empty() => m.base_color[3] = 1.0 - f[0],
            // Phong exponent → roughness.
            "Ns" if !f.is_empty() => m.roughness = (2.0 / (f[0] + 2.0)).sqrt().clamp(0.04, 1.0),
            "Pm" if !f.is_empty() => m.metallic = f[0],
            "Pr" if !f.is_empty() => m.roughness = f[0],
            "Ke" if f.len() >= 3 => m.emissive = [f[0], f[1], f[2]],
            "map_Kd" => {
                let name = rest.split_whitespace().last().unwrap_or(rest);
                if let Ok(texture) = std::fs::read(dir.join(name))
                    .map_err(|e| e.to_string())
                    .and_then(|b| super::decode_texture(&b))
                {
                    textures.push(texture);
                    m.base_texture = Some(textures.len() - 1);
                    // The texture carries the colour.
                    m.base_color = [1.0, 1.0, 1.0, m.base_color[3]];
                }
            }
            _ => {}
        }
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_faces_with_materials() {
        let dir = std::env::temp_dir().join(format!("bae-obj-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("m.mtl"), "newmtl red\nKd 1 0 0\nNs 100\n").unwrap();
        std::fs::write(
            dir.join("q.obj"),
            "mtllib m.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nusemtl red\nf 1 2 3 4\n",
        )
        .unwrap();
        let model = load(&dir.join("q.obj")).unwrap();
        assert_eq!(model.parts.len(), 1);
        let part = &model.parts[0];
        assert_eq!(
            part.indices,
            [0, 1, 2, 0, 2, 3],
            "a quad fanned into triangles"
        );
        assert_eq!(model.materials[part.material].base_color[0], 1.0);
        std::fs::remove_dir_all(dir).ok();
    }
}
