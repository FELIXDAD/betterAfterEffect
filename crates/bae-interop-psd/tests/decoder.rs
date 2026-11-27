use bae_interop_psd::{bounds, decode, format::PsdFile, layer_key, resolve};
use bae_model::psd::{LayerImage, LayerKey, PixelMask};

#[test]
fn raster_masks_preserve_soft_edges_and_the_group_origin() {
    let file = PsdFile::from_bytes(include_bytes!("fixtures/art.psd")).unwrap();
    let mut sel = LayerImage {
        key: LayerKey::Id(2),
        canvas: false,
        mask: PixelMask::AsSaved,
    };
    let base = decode(&file, Some(sel)).unwrap();
    let alpha = 230. / 255.;
    assert_eq!(base.rgba[0][3], 0.);
    assert!((base.rgba[1][3] - alpha * 128. / 255.).abs() < 1e-6);
    sel.mask = PixelMask::Disabled;
    assert!((decode(&file, Some(sel)).unwrap().rgba[0][3] - alpha).abs() < 1e-6);
    sel.key = LayerKey::Id(4);
    sel.mask = PixelMask::Group {
        origin: [1, 1],
        size: [3, 2],
        inverted: false,
        density: 65535,
    };
    let group = decode(&file, Some(sel)).unwrap();
    assert_eq!(group.rgba[0], [1.; 4]);
    assert_eq!(group.rgba[4][3], 0.);
    assert!((group.rgba[3][3] - 128. / 255.).abs() < 1e-6);
}

#[test]
fn layer_precision_alpha_and_bounds_survive_both_containers() {
    for (bytes, red) in [
        (include_bytes!("fixtures/ink16.psd").as_slice(), 0.12345),
        (include_bytes!("fixtures/ink32.psb").as_slice(), 1.5),
    ] {
        let file = PsdFile::from_bytes(bytes).unwrap();
        let layer = resolve(&file, LayerKey::Id(37)).unwrap();
        assert_eq!(layer.name(), "Ink");
        assert_eq!(layer.opacity, 192);
        let mut sel = LayerImage {
            key: layer_key(layer, 0),
            canvas: false,
            mask: Default::default(),
        };
        assert_eq!(bounds(&file, Some(sel)).unwrap().left, 1);
        let raw = decode(&file, Some(sel)).unwrap();
        assert_eq!(raw.width, 1);
        assert!((raw.rgba[0][0] - red).abs() < 0.00002);
        assert!((raw.rgba[0][3] - 0.75).abs() < 0.00002);
        sel.canvas = true;
        let placed = decode(&file, Some(sel)).unwrap();
        assert_eq!(placed.rgba, vec![[0.; 4], raw.rgba[0]]);
        // The composite (alpha 0.75) comes un-matted from white at 16 and 32 bit.
        let merged = decode(&file, None).unwrap();
        assert_eq!(merged.width, 2);
        let unmatte = |v: f32| (v - 0.25) / 0.75;
        let expected = [unmatte(red).max(0.), 0., unmatte(0.5), 0.75];
        for (got, want) in merged.rgba[1].iter().zip(expected) {
            assert!((got - want).abs() < 0.0001, "{:?}", merged.rgba[1]);
        }
        assert!(resolve(&file, LayerKey::Id(12)).is_err());
    }
}
