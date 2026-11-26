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
    assert_eq!(
        input.channels.as_ref().unwrap().info.find(&["Depth.Z"]),
        Some(3)
    );
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

#[test]
fn ranked_mattes_and_integer_ids_survive_the_import_path() {
    use bae_effects::exr_controls::{self, CryptoSelection, IdChannel, Routing};
    let Some(gpu) = context() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("channels.0001.exr");
    let mut attrs = LayerAttributes::named("Scene");
    attrs.other.insert(
        Text::from("cryptomatte/1234567/name"),
        AttributeValue::Text(Text::from("CryptoObject")),
    );
    attrs.other.insert(
        Text::from("cryptomatte/1234567/manifest"),
        AttributeValue::Text(Text::from(r#"{"Ball":"3f800000","Cube":"40000000"}"#)),
    );
    let mut channels = smallvec::smallvec![
        AnyChannel::new("ObjectID", FlatSamples::U32(vec![16777216, 16777217])),
        AnyChannel::new("MaterialID", FlatSamples::U32(vec![5, 6])),
    ];
    for (name, values) in [
        ("R", [1., 1.]),
        ("G", [1., 1.]),
        ("B", [1., 1.]),
        ("A", [1., 1.]),
        ("CryptoObject00.R", [1., 2.]),
        ("CryptoObject00.G", [0.7, 1.]),
        ("CryptoObject00.B", [2., 0.]),
        ("CryptoObject00.A", [0.2, 0.]),
        ("CryptoObject01.R", [2., 0.]),
        ("CryptoObject01.G", [0.1, 0.]),
    ] {
        channels.push(AnyChannel::new(name, FlatSamples::F32(values.to_vec())));
    }
    let mut image = Image::from_layer(Layer::new(
        (2, 1),
        attrs,
        Encoding::FAST_LOSSLESS,
        AnyChannels::sort(channels),
    ));
    image.write().to_file(&path).unwrap();
    for channel in &mut image.layer_data.channel_data.list {
        match &mut channel.sample_data {
            FlatSamples::F32(v) => v.reverse(),
            FlatSamples::U32(v) => v.reverse(),
            FlatSamples::F16(v) => v.reverse(),
        }
    }
    image
        .write()
        .to_file(dir.path().join("channels.0002.exr"))
        .unwrap();
    let lib = Arc::new(MediaLibrary::new(
        MediaFormats::builtin(),
        Arc::new(FrameCache::new(1 << 20)),
    ));
    let source = FootageSource::open(
        lib,
        Arc::new(MediaGpu::new(gpu.clone())),
        &path,
        Some(bae_model::FrameRate::FPS_30),
    )
    .unwrap();
    let roi = bae_render::IRect::from_size(2, 1);
    let input = testing::render_source(
        &gpu,
        &source,
        &SourceRequest {
            roi,
            time: Time::ZERO,
            scale: 1.,
        },
    )
    .unwrap();
    let info = &input.channels.as_ref().unwrap().info;
    let group = &info.crypto[0];
    let selected = info.pick(group, [1, 0]).unwrap();
    assert_eq!(group.manifest[&selected], "Cube");
    let crypto = CryptoSelection {
        layer: group.name.clone(),
        object: Some(selected),
    };
    let routing = Routing {
        channels: [
            "Scene.MaterialID".into(),
            String::new(),
            String::new(),
            String::new(),
        ],
    };
    for (name, params, expected) in [
        (
            "Cryptomatte",
            vec![(1, exr_controls::value(&crypto)), (2, Value::Int(4))],
            [0.3, 1.],
        ),
        (
            "IDentifier",
            vec![
                (
                    5,
                    exr_controls::value(&IdChannel {
                        name: "Scene.ObjectID".into(),
                    }),
                ),
                (3, Value::Int(2)),
                (4, Value::Int(16777217)),
            ],
            [0., 1.],
        ),
        (
            "ADBE ID MATTE",
            vec![(1, Value::Int(1)), (2, Value::Int(5))],
            [1., 0.],
        ),
        (
            "EXtractoR",
            vec![(13, exr_controls::value(&routing)), (6, Value::Float(10.))],
            [0.5, 0.6],
        ),
    ] {
        let inst = instance(name, params);
        let out = testing::render_effect(
            &gpu,
            bae_effects::get(name).unwrap().effect().as_ref(),
            &EffectParams::new(&inst),
            &input,
            roi,
        )
        .unwrap();
        for (rgba, expected) in Pixels::download(&gpu, &out).data.iter().zip(expected) {
            assert!(
                (rgba[0] - expected).abs() < 0.001,
                "{name}: {rgba:?}, expected {expected}"
            );
        }
    }
    let next = testing::render_source(
        &gpu,
        &source,
        &SourceRequest {
            roi,
            scale: 1.,
            time: bae_model::FrameRate::FPS_30.frame_to_time(1),
        },
    )
    .unwrap();
    let inst = instance(
        "Cryptomatte",
        vec![(1, exr_controls::value(&crypto)), (2, Value::Int(4))],
    );
    let out = testing::render_effect(
        &gpu,
        bae_effects::get("Cryptomatte").unwrap().effect().as_ref(),
        &EffectParams::new(&inst),
        &next,
        roi,
    )
    .unwrap();
    let matte = Pixels::download(&gpu, &out);
    assert_eq!(matte.data[0], [1.; 4]);
    assert!((matte.data[1][0] - 0.3).abs() < 0.001);
}
