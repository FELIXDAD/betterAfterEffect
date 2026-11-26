use bae_interop_psd::{bounds, decode, format::PsdFile, layer_key, resolve};
use bae_model::psd::{LayerImage, LayerKey};

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
        };
        assert_eq!(bounds(&file, Some(sel)).unwrap().left, 1);
        let raw = decode(&file, Some(sel)).unwrap();
        assert_eq!(raw.width, 1);
        assert!((raw.rgba[0][0] - red).abs() < 0.00002);
        assert!((raw.rgba[0][3] - 0.75).abs() < 0.00002);
        sel.canvas = true;
        let placed = decode(&file, Some(sel)).unwrap();
        assert_eq!(placed.rgba, vec![[0.; 4], raw.rgba[0]]);
        assert_eq!(decode(&file, None).unwrap().width, 2);
        assert!(resolve(&file, LayerKey::Id(12)).is_err());
    }
}
