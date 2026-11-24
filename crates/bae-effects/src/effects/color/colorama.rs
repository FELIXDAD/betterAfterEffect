use crate::effects::image_grid;
use crate::{
    Builtin, EffectTarget, TargetEffect, gpu::UniformWriter, params::Params, rust_builtin,
};
use bae_model::{EffectDef, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use std::sync::Arc;

pub(super) const BYTES: usize = 192 + 64 * 32;
struct Colorama(EffectDef);

pub(crate) fn builtin() -> Builtin {
    let (def, names) = super::colorama_definition::definition();
    rust_builtin(
        (Arc::new(Colorama(def)), names),
        "colorama",
        "色光",
        &["彩色光", "palette", "调板"],
    )
}

impl Effect for Colorama {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        [4, 28]
            .into_iter()
            .filter_map(|id| match p.value(id) {
                Some(Value::PrimRef(layer)) => {
                    Some(LayerDependency::source(layer.clone(), p.comp_time))
                }
                _ => None,
            })
            .collect()
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}

impl TargetEffect for Colorama {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.0, p);
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let source = |id| match p.value(id) {
            Some(Value::PrimRef(layer)) => target.effect_input(layer).cloned(),
            _ => None,
        };
        let phase = source(4);
        let mask = source(28);
        let grid = image_grid::grid_scale(p, input);
        let mut u = UniformWriter::with_size(BYTES);
        image_grid::image_coords(&mut u, 0, input, grid);
        image_grid::image_coords(&mut u, 32, phase.as_ref().unwrap_or(input), grid);
        image_grid::image_coords(&mut u, 64, mask.as_ref().unwrap_or(input), grid);
        u.i32s(96, &[roi.x0, roi.y0])
            .u32(104, roi.width())
            .u32(108, roi.height());
        for (i, id) in [2, 6, 40, 16].into_iter().enumerate() {
            u.u32(112 + 4 * i, v.popup(id) as u32);
        }
        u.f32(128, (v.f(8) / 360.) as f32)
            .f32(132, v.f(38) as f32)
            .f32(136, v.f(22) as f32)
            .f32(140, v.f(24) as f32);
        if let Some(Value::Color(color)) = v.value(20) {
            u.f32s(144, &color.map(|x| x as f32));
        }
        let flags = u32::from(v.b(14))
            | u32::from(v.b(18)) << 1
            | u32::from(v.b(34)) << 2
            | u32::from(v.b(32)) << 3;
        u.u32(160, v.popup(26) as u32)
            .u32(164, u32::from(phase.is_some()))
            .u32(
                168,
                if mask.is_some() {
                    v.popup(30) as u32
                } else {
                    1
                },
            )
            .u32(172, flags)
            .f32(176, (v.f(36) / 100.) as f32);
        // Custom wheel stops are JSON [phase,r,g,b,a] tuples in cyclic order.
        let preset = v.popup(10);
        let custom = match v.value(12) {
            Some(Value::Bytes(bytes)) if preset == 1 && !bytes.is_empty() => Some(
                serde_json::from_slice::<Vec<[f32; 5]>>(bytes)
                    .map_err(|e| RenderError::Other(e.to_string()))?,
            ),
            _ => None,
        };
        let stops = custom.unwrap_or_else(|| {
            super::colorama_palettes::palette(preset)
                .iter()
                .map(|&(phase, rgba)| {
                    let [r, g, b, a] = rgba.to_be_bytes().map(|v| f32::from(v) / 255.);
                    [f32::from(phase) / 65536., r, g, b, a]
                })
                .collect()
        });
        if stops.is_empty() || stops.len() > 64 {
            return Err(RenderError::Other(
                "Colorama requires 1–64 palette stops".into(),
            ));
        }
        for (i, stop) in stops.iter().enumerate() {
            u.f32s(192 + 32 * i, &stop[1..]).f32(208 + 32 * i, stop[0]);
        }
        u.u32(180, stops.len() as u32);
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        super::dispatch(
            target,
            "colorama",
            &u,
            &[
                input,
                phase.as_ref().unwrap_or(input),
                mask.as_ref().unwrap_or(input),
            ],
            &out,
            roi.size(),
        )?;
        Ok(out)
    }
}
