use super::{Kind, Scene};
use crate::{
    EffectTarget,
    gpu::{self, Binding, KernelKey, KernelSource, Resource, UniformWriter},
};
use bae_render::{GpuImage, IRect, RenderError};
use std::sync::Arc;

pub(super) fn frame(
    scene: &Scene<'_>,
    input: &GpuImage,
    roi: IRect,
    maps: &[Option<GpuImage>],
) -> UniformWriter {
    let scale = scene.p.p.scale as f32;
    let mut u = UniformWriter::with_size(272);
    u.f32s(0, &[scene.p.p.time.as_seconds().max(0.) as f32, scale]);
    u.f32s(
        16,
        &[
            roi.x0 as f32 / scale - scene.origin[0],
            roi.y0 as f32 / scale - scene.origin[1],
            roi.width() as f32 / scale,
            roi.height() as f32 / scale,
        ],
    );
    u.f32s(
        32,
        &[
            scene.origin[0] * scale - input.rect.x0 as f32,
            scene.origin[1] * scale - input.rect.y0 as f32,
            input.rect.width() as f32,
            input.rect.height() as f32,
        ],
    );
    for i in 0..4 {
        let r = maps.get(i).and_then(Option::as_ref).unwrap_or(input).rect;
        u.f32s(48 + 16 * i, &[0., 0., r.width() as f32, r.height() as f32]);
    }
    let side = maps.get(4).and_then(Option::as_ref).unwrap_or(input).rect;
    u.f32s(208, &[side.width() as f32, side.height() as f32]);
    if let Some(view) = scene.p.p.scene {
        if let Some(camera) = view.camera {
            let inv = view.placement.inverse();
            let origin = glam::DVec2::from_array(scene.origin.map(f64::from));
            let center = glam::DVec2::from_array(view.size.map(f64::from)) * 0.5;
            let project = |v: glam::DVec3| {
                let xy = view.placement.transform_point2(v.truncate() + origin);
                let c = camera.view().transform_point3(xy.extend(v.z));
                let (w, zoom) = match camera.projection {
                    bae_render::camera::Projection::Perspective => (c.z, camera.zoom),
                    bae_render::camera::Projection::Orthographic { scale } => (1., scale),
                };
                let xy = inv.matrix2 * (c.truncate() * zoom + center * w)
                    + (inv.translation - origin) * w;
                glam::DVec4::new(xy.x, xy.y, 0., w)
            };
            let base = project(glam::DVec3::ZERO);
            for (i, col) in [
                project(glam::DVec3::X) - base,
                project(glam::DVec3::Y) - base,
                project(glam::DVec3::Z) - base,
                base,
            ]
            .iter()
            .enumerate()
            {
                u.f32s(112 + i * 16, &col.as_vec4().to_array());
            }
            u.f32(8, 1.);
        }
        if let Some(light) = view.layer_light {
            let xy = view
                .placement
                .inverse()
                .transform_point2(light.position().truncate());
            u.f32s(
                176,
                &[
                    xy.x as f32 - scene.origin[0],
                    xy.y as f32 - scene.origin[1],
                    light.position().z as f32,
                    light.intensity * 0.01,
                ],
            );
            u.f32s(192, &light.color);
            u.f32(12, 1.);
        }
    }
    u
}

pub(super) fn composite_kernel(
    rt: &gpu::Runtime,
    format: bae_gpu::wgpu::TextureFormat,
) -> Result<Arc<gpu::Kernel>, RenderError> {
    rt.kernel(
        &KernelKey {
            owner: Arc::from("bae.effects/simulation"),
            entry: Arc::from("composite"),
            variant: gpu::format_key(format),
        },
        || {
            Ok(KernelSource {
                label: "bae.effects.particle_composite",
                entry: "composite",
                wgsl: include_str!("composite.wgsl")
                    .replace("WORKING_FORMAT", crate::blur::format_name(format)?),
                bindings: vec![
                    (0, Binding::Uniform(128)),
                    (1, Binding::Texture),
                    (2, Binding::Texture),
                    (3, Binding::Storage(format)),
                ],
                workgroup: [8, 8, 1],
            })
        },
    )
}

pub(super) fn composite(
    kind: Kind,
    target: &mut dyn EffectTarget,
    scene: &Scene<'_>,
    input: &GpuImage,
    overlay: &GpuImage,
) -> Result<GpuImage, RenderError> {
    let p = scene.p;
    let (original, mode) = match kind {
        Kind::World => (p.f(92) as f32, p.popup(39)),
        Kind::Systems => (0., p.popup(28)),
        Kind::Snow => (p.f(26) as f32, if p.popup(25) == 2 { 5 } else { 1 }),
        Kind::Rain => (p.f(16) as f32, if p.popup(15) == 2 { 5 } else { 1 }),
        Kind::Hair => (1., 1),
        Kind::Mercury => (0., 4),
        Kind::Foam if p.popup(2) == 2 => (1., 1),
        Kind::Stars => (p.f(6) as f32, 1),
        Kind::Bubbles | Kind::Drizzle => (1., 1),
        _ => (0., 1),
    };
    let format = target.format();
    let force_guides = kind == Kind::Shatter && p.popup(2) >= 4;
    if original == 0. && mode == 1 && overlay.texture.format() == format && !force_guides {
        return Ok(overlay.clone());
    }
    let rt = gpu::runtime(target.gpu());
    let roi = overlay.rect;
    let kernel = composite_kernel(&rt, format)?;
    let mut u = UniformWriter::with_size(128);
    u.i32s(
        0,
        &[
            roi.x0 - input.rect.x0,
            roi.y0 - input.rect.y0,
            roi.width() as i32,
            roi.height() as i32,
        ],
    );
    u.u32(16, kind as u32)
        .u32(20, mode as u32)
        .f32(24, original);
    if kind == Kind::Mercury {
        let mut light = p
            .value(18)
            .and_then(|v| {
                if let bae_model::Value::Color(c) = v {
                    Some([c[0] as f32, c[1] as f32, c[2] as f32])
                } else {
                    None
                }
            })
            .unwrap_or([1.; 3]);
        let mut intensity = p.f(17) as f32 * 0.01;
        let mut height = p.f(20) as f32 * 0.01;
        let mut pos =
            p.p.point(21)
                .unwrap_or([scene.size[0] as f64 * 0.25, scene.size[1] as f64 * 0.25]);
        let mut point = p.popup(19) == 2;
        if p.popup(31) == 2
            && let Some(view) = p.p.scene
            && let Some(l) = view.layer_light
        {
            light = l.color;
            intensity = l.intensity * 0.01;
            let xy = view
                .placement
                .inverse()
                .transform_point2(l.position().truncate());
            pos = xy.to_array();
            height = (l.position().z.abs() as f32 / scene.size[0]).max(0.001);
            point = true;
        }
        u.f32s(32, &[0., 0., intensity, height])
            .f32s(
                48,
                &[
                    p.f(25) as f32 * 0.01,
                    p.f(26) as f32 * 0.01,
                    p.f(27) as f32 * 0.01,
                    p.f(28) as f32,
                ],
            )
            .f32s(64, &light)
            .f32s(
                80,
                &[
                    (pos[0] * p.p.scale) as f32 - input.rect.x0 as f32,
                    (pos[1] * p.p.scale) as f32 - input.rect.y0 as f32,
                    (p.f(22) as f32).to_radians(),
                    if point { 2. } else { 1. },
                ],
            )
            .f32s(
                96,
                &[
                    p.f(29) as f32 * 0.01,
                    input.rect.width() as f32,
                    input.rect.height() as f32,
                    p.f(32) as f32 * 0.01,
                ],
            );
    }
    if kind == Kind::Drizzle {
        let scale = p.p.scale as f32;
        u.f32s(
            32,
            &[
                p.f(4) as f32 * scale,
                p.f(5) as f32 * 0.01,
                p.f(8) as f32 * 0.01,
                p.f(11) as f32 * 0.01,
            ],
        );
        u.f32s(
            48,
            &[
                p.f(16) as f32 * 0.01,
                p.f(17) as f32 * 0.01,
                p.f(18) as f32 * 0.01,
                p.f(19) as f32,
            ],
        );
        if let Some(bae_model::Value::Color(c)) = p.value(9) {
            u.f32s(64, &c.map(|v| v as f32));
        }
        let position = match p.p.point(12) {
            Some(v) => [v[0] as f32, v[1] as f32],
            None => [
                scene.size[0] * 0.25 + scene.origin[0],
                scene.size[1] * 0.25 + scene.origin[1],
            ],
        };
        u.f32s(
            80,
            &[
                position[0] * scale - input.rect.x0 as f32,
                position[1] * scale - input.rect.y0 as f32,
                (p.f(13) as f32).to_radians(),
                p.f(10) as f32,
            ],
        );
        u.f32s(
            96,
            &[
                p.f(20) as f32 * 0.01,
                input.rect.width() as f32,
                input.rect.height() as f32,
            ],
        );
    }
    if kind == Kind::Drizzle
        && p.popup(22) == 2
        && let Some(view) = p.p.scene
        && let Some(light) = view.layer_light
    {
        let xy = view
            .placement
            .inverse()
            .transform_point2(light.position().truncate())
            * p.p.scale;
        u.f32(40, light.intensity * 0.01)
            .f32(
                44,
                (light.position().z.abs() as f32 / scene.size[0]).max(0.01),
            )
            .f32s(64, &light.color)
            .f32s(
                80,
                &[
                    xy.x as f32 - input.rect.x0 as f32,
                    xy.y as f32 - input.rect.y0 as f32,
                ],
            )
            .f32(92, 2.);
    }
    if force_guides {
        let scale = p.p.scale as f32;
        for (index, id) in [24u32, 32].into_iter().enumerate() {
            let default_x = if index == 0 { 0.5 } else { 0.75 };
            let center = p.p.point(id).map(|v| v.map(|x| x as f32)).unwrap_or([
                scene.origin[0] + scene.size[0] * default_x,
                scene.origin[1] + scene.size[1] * 0.5,
            ]);
            let radius = p.f(id + 4).powi(2) - p.f(id + 2).powi(2);
            u.f32s(
                32 + index * 16,
                &[
                    center[0] * scale - roi.x0 as f32,
                    center[1] * scale - roi.y0 as f32,
                    radius.max(0.).sqrt() as f32 * scene.size[0] * scale,
                    1.,
                ],
            );
        }
    }
    let out = target.allocate(roi, input.pixel_scale)?;
    let offset = rt.uniform(&u.bytes);
    let bind = rt.bind_group(
        &kernel,
        &[
            Resource::Uniform,
            Resource::View(&input.view),
            Resource::View(&overlay.view),
            Resource::View(&out.view),
        ],
    );
    let mut pass = target.encoder().begin_compute_pass(&Default::default());
    gpu::dispatch(&mut pass, &kernel, &bind, &[offset], roi.size());
    drop(pass);
    Ok(out)
}
