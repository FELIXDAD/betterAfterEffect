//! Extruded text and shape layers (AE's Advanced 3D Geometry Options:
//! Extrusion Depth): the layer's filled outlines ([`crate::source::Outlines`],
//! layer pixels) turned into a closed solid, the front face on the layer's
//! plane (z = 0, facing the camera along −z), the back face `depth` pixels
//! behind it and sides along every contour. Faces are triangulated by ear
//! clipping, holes bridged into their outer contour.

use std::sync::Arc;

use super::{Material, Model, Part};
use crate::source::Outlines;

/// The solid of `outlines` extruded `depth` layer pixels (one part per fill,
/// coloured by it). `None` when nothing is filled.
pub fn extrude(outlines: &Outlines, depth: f64) -> Option<Model> {
    let mut parts = Vec::new();
    let mut materials = Vec::new();
    for fill in &outlines.fills {
        let mut part = Part {
            material: materials.len(),
            ..Part::default()
        };
        for (outer, holes) in nest(&fill.contours) {
            add_solid(&mut part, &outer, &holes, depth);
        }
        if part.indices.is_empty() {
            continue;
        }
        let [r, g, b, a] = fill.color;
        materials.push(Material {
            base_color: [srgb(r), srgb(g), srgb(b), a],
            roughness: 0.6,
            double_sided: true,
            ..Material::default()
        });
        parts.push(part);
    }
    (!parts.is_empty()).then(|| Model {
        parts,
        materials,
        textures: Vec::new(),
        bounds_cache: Default::default(),
    })
}

/// Shared extruded solids, by the content they come from (text animates;
/// the newest few are kept).
pub fn cached(key: u64, make: impl FnOnce() -> Option<Model>) -> Option<Arc<Model>> {
    use std::sync::{Mutex, OnceLock};
    const KEPT: usize = 32;
    type Kept = Mutex<Vec<(u64, Arc<Model>)>>;
    static CACHE: OnceLock<Kept> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache
        .lock()
        .unwrap()
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, m)| m.clone())
    {
        return Some(hit);
    }
    let model = Arc::new(make()?);
    let mut cache = cache.lock().unwrap();
    if cache.len() >= KEPT {
        cache.remove(0);
    }
    cache.push((key, model.clone()));
    Some(model)
}

fn srgb(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

type P = [f64; 2];

fn area(c: &[P]) -> f64 {
    let n = c.len();
    (0..n)
        .map(|i| {
            let (a, b) = (c[i], c[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn inside(p: P, c: &[P]) -> bool {
    let mut odd = false;
    let n = c.len();
    for i in 0..n {
        let (a, b) = (c[i], c[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                odd = !odd;
            }
        }
    }
    odd
}

/// Contours grouped into outer contours (even nesting) and their holes (the
/// odd ones directly inside), outer counter-clockwise and holes clockwise
/// (in y-down layer pixels: positive area outer).
fn nest(contours: &[Vec<P>]) -> Vec<(Vec<P>, Vec<Vec<P>>)> {
    let cs: Vec<Vec<P>> = contours
        .iter()
        .map(|c| dedup(c))
        .filter(|c| c.len() >= 3 && area(c).abs() > 1e-9)
        .collect();
    let depth: Vec<usize> = (0..cs.len())
        .map(|i| {
            (0..cs.len())
                .filter(|&j| j != i && inside(cs[i][0], &cs[j]))
                .count()
        })
        .collect();
    let oriented = |c: &[P], positive: bool| {
        let mut c = c.to_vec();
        if (area(&c) > 0.0) != positive {
            c.reverse();
        }
        c
    };
    let mut out: Vec<(usize, Vec<P>, Vec<Vec<P>>)> = (0..cs.len())
        .filter(|&i| depth[i].is_multiple_of(2))
        .map(|i| (i, oriented(&cs[i], true), Vec::new()))
        .collect();
    for i in (0..cs.len()).filter(|&i| depth[i] % 2 == 1) {
        // The innermost outer contour around it.
        if let Some(o) = out
            .iter_mut()
            .filter(|(j, ..)| depth[*j] + 1 == depth[i] && inside(cs[i][0], &cs[*j]))
            .min_by(|a, b| area(&a.1).abs().total_cmp(&area(&b.1).abs()))
        {
            o.2.push(oriented(&cs[i], false));
        }
    }
    out.into_iter().map(|(_, o, h)| (o, h)).collect()
}

fn dedup(c: &[P]) -> Vec<P> {
    let mut out: Vec<P> = Vec::with_capacity(c.len());
    for &p in c {
        if out
            .last()
            .is_none_or(|q| (q[0] - p[0]).abs() + (q[1] - p[1]).abs() > 1e-6)
        {
            out.push(p);
        }
    }
    while out.len() > 1 && {
        let (a, b) = (out[0], out[out.len() - 1]);
        (a[0] - b[0]).abs() + (a[1] - b[1]).abs() <= 1e-6
    } {
        out.pop();
    }
    out
}

/// One simple polygon of `outer` with `holes` spliced in through bridges
/// (Eberly's method: each hole, rightmost first, joins a visible outer
/// vertex to the right of it).
fn bridge(outer: &[P], holes: &[Vec<P>]) -> Vec<P> {
    let mut poly = outer.to_vec();
    let mut holes: Vec<&Vec<P>> = holes.iter().collect();
    holes.sort_by(|a, b| {
        let max = |h: &Vec<P>| h.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        max(b).total_cmp(&max(a))
    });
    for hole in holes {
        let (hi, m) = hole
            .iter()
            .copied()
            .enumerate()
            .max_by(|a, b| a.1[0].total_cmp(&b.1[0]))
            .expect("contours have points");
        // Nearest edge crossed by the ray from m to +x.
        let n = poly.len();
        let mut best: Option<(f64, usize)> = None;
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if (a[1] > m[1]) == (b[1] > m[1]) {
                continue;
            }
            let x = a[0] + (m[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if x >= m[0] && best.is_none_or(|(bx, _)| x < bx) {
                // The edge's endpoint farther right.
                best = Some((x, if a[0] > b[0] { i } else { (i + 1) % n }));
            }
        }
        let Some((_, mut vi)) = best else {
            continue;
        };
        // A reflex vertex inside the triangle (m, hit, poly[vi]) would block
        // the view: take the one closest in angle.
        let v = poly[vi];
        let mut closest = f64::MAX;
        for (i, &p) in poly.iter().enumerate() {
            if i == vi || p[0] < m[0] {
                continue;
            }
            let (dx, dy) = (p[0] - m[0], p[1] - m[1]);
            let in_tri = triangle_contains([m, [v[0].max(m[0]), m[1]], v], p);
            if in_tri {
                let angle = dy.atan2(dx).abs();
                if angle < closest {
                    closest = angle;
                    vi = i;
                }
            }
        }
        let mut joined = Vec::with_capacity(poly.len() + hole.len() + 2);
        joined.extend_from_slice(&poly[..=vi]);
        joined.extend(hole[hi..].iter().chain(&hole[..=hi]).copied());
        joined.extend_from_slice(&poly[vi..]);
        poly = joined;
    }
    poly
}

fn cross(o: P, a: P, b: P) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn triangle_contains(t: [P; 3], p: P) -> bool {
    let (d1, d2, d3) = (
        cross(t[0], t[1], p),
        cross(t[1], t[2], p),
        cross(t[2], t[0], p),
    );
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Triangles (indices into `poly`, counter-clockwise like it) by ear
/// clipping.
pub(crate) fn triangulate(poly: &[P]) -> Vec<[usize; 3]> {
    let mut idx: Vec<usize> = (0..poly.len()).collect();
    let mut out = Vec::with_capacity(poly.len().saturating_sub(2));
    let mut guard = 0;
    while idx.len() > 3 && guard < idx.len() * 2 {
        let n = idx.len();
        let mut clipped = false;
        for k in 0..n {
            let (a, b, c) = (idx[(k + n - 1) % n], idx[k], idx[(k + 1) % n]);
            if cross(poly[a], poly[b], poly[c]) <= 1e-12 {
                continue;
            }
            let t = [poly[a], poly[b], poly[c]];
            let blocked = idx.iter().any(|&j| {
                j != a
                    && j != b
                    && j != c
                    && poly[j] != poly[a]
                    && poly[j] != poly[b]
                    && poly[j] != poly[c]
                    && triangle_contains(t, poly[j])
            });
            if blocked {
                continue;
            }
            out.push([a, b, c]);
            idx.remove(k);
            clipped = true;
            break;
        }
        if clipped {
            guard = 0;
        } else {
            // Degenerate leftovers (collinear runs): drop a vertex.
            guard += 1;
            idx.remove(0);
        }
    }
    if idx.len() == 3 && cross(poly[idx[0]], poly[idx[1]], poly[idx[2]]) > 0.0 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

/// The front and back faces and the sides of one outer contour with holes.
fn add_solid(part: &mut Part, outer: &[P], holes: &[Vec<P>], depth: f64) {
    let poly = bridge(outer, holes);
    let tris = triangulate(&poly);
    let z = depth as f32;
    let push = |part: &mut Part, p: [f32; 3], n: [f32; 3], uv: [f32; 2]| {
        part.positions.push(p);
        part.normals.push(n);
        part.uvs.push(uv);
        part.positions.len() as u32 - 1
    };
    // Front (z = 0, toward the camera at −z) and back (z = depth).
    for (zz, nz, flip) in [(0.0f32, -1.0f32, false), (z, 1.0, true)] {
        let base = part.positions.len() as u32;
        for p in &poly {
            push(
                part,
                [p[0] as f32, p[1] as f32, zz],
                [0.0, 0.0, nz],
                [0.0; 2],
            );
        }
        for t in &tris {
            let [a, b, c] = t.map(|i| base + i as u32);
            if flip {
                part.indices.extend([a, c, b]);
            } else {
                part.indices.extend([a, b, c]);
            }
        }
    }
    if depth <= 0.0 {
        return;
    }
    // Sides, flat-shaded per edge.
    for contour in std::iter::once(outer).chain(holes.iter().map(Vec::as_slice)) {
        let n = contour.len();
        for i in 0..n {
            let (a, b) = (contour[i], contour[(i + 1) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                continue;
            }
            // Outward for a positive-area outer (and an inner hole edge).
            let nrm = [(dy / len) as f32, (-dx / len) as f32, 0.0];
            let v = [
                push(part, [a[0] as f32, a[1] as f32, 0.0], nrm, [0.0; 2]),
                push(part, [b[0] as f32, b[1] as f32, 0.0], nrm, [0.0; 2]),
                push(part, [b[0] as f32, b[1] as f32, z], nrm, [0.0; 2]),
                push(part, [a[0] as f32, a[1] as f32, z], nrm, [0.0; 2]),
            ];
            part.indices.extend([v[0], v[2], v[1], v[0], v[3], v[2]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::OutlineFill;

    fn tri_area(poly: &[P], tris: &[[usize; 3]]) -> f64 {
        tris.iter()
            .map(|t| cross(poly[t[0]], poly[t[1]], poly[t[2]]) / 2.0)
            .sum()
    }

    #[test]
    fn concave_polygons_triangulate_to_their_area() {
        // An L shape.
        let l = vec![
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 10.0],
            [10.0, 10.0],
            [10.0, 20.0],
            [0.0, 20.0],
        ];
        let poly = if area(&l) > 0.0 {
            l.clone()
        } else {
            l.iter().rev().copied().collect()
        };
        let tris = triangulate(&poly);
        assert_eq!(tris.len(), 4);
        assert!((tri_area(&poly, &tris) - 300.0).abs() < 1e-9);
    }

    #[test]
    fn holes_are_left_open() {
        // A 30 × 30 square with a 10 × 10 hole (the letter O, squarely).
        let outer = vec![[0.0, 0.0], [30.0, 0.0], [30.0, 30.0], [0.0, 30.0]];
        let hole = vec![[10.0, 10.0], [20.0, 10.0], [20.0, 20.0], [10.0, 20.0]];
        let nested = nest(&[outer, hole]);
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].1.len(), 1);
        let poly = bridge(&nested[0].0, &nested[0].1);
        let tris = triangulate(&poly);
        assert!(
            (tri_area(&poly, &tris) - 800.0).abs() < 1e-6,
            "{}",
            tri_area(&poly, &tris)
        );
    }

    #[test]
    fn extruded_solids_have_faces_and_sides() {
        let outlines = Outlines {
            fills: vec![OutlineFill {
                color: [1.0, 0.0, 0.0, 1.0],
                contours: vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]],
            }],
        };
        let model = extrude(&outlines, 5.0).unwrap();
        let part = &model.parts[0];
        // 2 faces of 2 triangles and 4 sides of 2.
        assert_eq!(part.indices.len() / 3, 12);
        let zs: Vec<f32> = part.positions.iter().map(|p| p[2]).collect();
        assert!(zs.contains(&0.0) && zs.contains(&5.0));
        assert_eq!(model.materials[0].base_color[0], 1.0);
    }
}
