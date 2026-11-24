use super::packages;
use crate::Registry;
use crate::{
    TargetEffect,
    effects::{BoxBlur, GaussianBlur, spatial},
    rust_builtin,
};
use std::sync::Arc;

pub(crate) fn register(registry: &mut Registry) {
    registry.prewarm.push(crate::blur::prewarm);
    registry.prewarm.push(spatial::prewarm);
    registry.prewarm.push(crate::effects::compound::prewarm);
    registry
        .effects
        .push(crate::effects::compound::compound_blur());
    packages(registry, &[embedded!("sharpen", ["shaders/sharpen.wgsl"])]);
    let blur = |fx: GaussianBlur| {
        let names = fx.names_zh.clone();
        (Arc::new(fx) as Arc<dyn TargetEffect>, names)
    };
    let boxed = |fx: BoxBlur| {
        let names = fx.names_zh.clone();
        (Arc::new(fx) as Arc<dyn TargetEffect>, names)
    };
    registry.effects.push(rust_builtin(
        blur(GaussianBlur::new()),
        "gaussian-blur",
        "高斯模糊",
        &["blur", "gaussian", "soften", "模糊", "高斯", "柔化"],
    ));
    registry.effects.push(rust_builtin(
        blur(GaussianBlur::legacy()),
        "gaussian-blur-legacy",
        "高斯模糊（旧版）",
        &["blur", "gaussian", "legacy", "模糊", "高斯", "旧版"],
    ));
    registry.effects.push(rust_builtin(
        boxed(BoxBlur::new()),
        "fast-box-blur",
        "快速方框模糊",
        &["blur", "box", "fast", "模糊", "方框", "快速"],
    ));
    registry.effects.push(rust_builtin(
        boxed(BoxBlur::legacy()),
        "box-blur",
        "方框模糊",
        &["blur", "box", "legacy", "模糊", "方框"],
    ));
    registry
        .effects
        .extend(spatial::definitions::blur_sharpen());
}
