mod common;

use bae_media::{FrameCache, MediaFormats, MediaGpu, MediaLibrary};
use bae_model::{Time, Value};
use bae_render::{EffectParams, SourceRequest, footage::FootageSource, testing};
use common::{Pixels, context, instance};
use exr::prelude::*;
use std::sync::Arc;

#[test]
fn multipart_depth_reaches_registered_effects_without_half_quantization() {
    let Some(gpu) = context() else {
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("passes.exr");
    let part = |name: &str, samples: Vec<(&str, FlatSamples)>| {
        Layer::new(
            (2, 1),
            LayerAttributes::named(name),
            Encoding::FAST_LOSSLESS,
            AnyChannels::sort(
                samples
                    .into_iter()
                    .map(|(n, v)| AnyChannel::new(n, v))
                    .collect(),
            ),
        )
    };
    let beauty = part(
        "Beauty",
        ["R", "G", "B"]
            .map(|c| (c, FlatSamples::F32(vec![0.5, 0.75])))
            .to_vec(),
    );
    let depth = part(
        "Depth",
        vec![("Z", FlatSamples::F32(vec![100000., 100001.]))],
    );
    Image::from_layers(
        ImageAttributes::with_size((2, 1)),
        smallvec::smallvec![beauty, depth],
    )
    .write()
    .to_file(&path)
    .unwrap();
    let library = Arc::new(MediaLibrary::new(
        MediaFormats::builtin(),
        Arc::new(FrameCache::new(1 << 20)),
    ));
    let source =
        FootageSource::open(library, Arc::new(MediaGpu::new(gpu.clone())), &path, None).unwrap();
    let roi = bae_render::IRect::from_size(2, 1);
    let input = testing::render_source(
        &gpu,
        &source,
        &SourceRequest {
            roi,
            scale: 1.,
            time: Time::ZERO,
        },
    )
    .unwrap();
    assert_eq!(input.channels.as_ref().unwrap().find(&["Depth.Z"]), Some(3));
    for (name, values, expected) in [
        (
            "ADBE AUX CHANNEL EXTRACT",
            vec![(2, Value::Float(100000.)), (3, Value::Float(100001.))],
            [[0., 0., 0., 1.], [1.; 4]],
        ),
        (
            "ADBE DEPTH MATTE",
            vec![(1, Value::Float(100000.5))],
            [[0.; 4], [0.75, 0.75, 0.75, 1.]],
        ),
        (
            "ADBE FOG_3D",
            vec![(2, Value::Float(100000.)), (3, Value::Float(100001.))],
            [[0.5, 0.5, 0.5, 1.], [1.; 4]],
        ),
        (
            "ADBE DEPTH FIELD",
            vec![],
            [[0.5, 0.5, 0.5, 1.], [0.75, 0.75, 0.75, 1.]],
        ),
    ] {
        let inst = instance(name, values);
        let out = testing::render_effect(
            &gpu,
            bae_effects::get(name).unwrap().effect().as_ref(),
            &EffectParams::new(&inst),
            &input,
            roi,
        )
        .unwrap();
        assert_eq!(Pixels::download(&gpu, &out).data, expected, "{name}");
    }
}
