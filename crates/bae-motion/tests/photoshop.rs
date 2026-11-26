use bae_interop_psd::{format::PsdFile, import::describe};
use bae_model::{
    AuthorId, CommandRegistry, PrimPath, Stage, Value,
    effect::param_attr,
    psd::{LayerImage, LayerKey},
};
use serde_json::json;
use std::path::Path;

#[test]
fn reload_reuses_the_asset_when_the_source_layer_moves_or_changes_name() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../bae-interop-psd/tests/fixtures/art.psd");
    let parsed = PsdFile::from_bytes(&std::fs::read(&file).unwrap()).unwrap();
    let mut doc = describe(&parsed).unwrap();
    let mut stage = Stage::new();
    let mut registry = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut registry);
    let author = AuthorId::new("test:psd").unwrap();
    let key = LayerKey::Id(2);
    let sel = LayerImage {
        key,
        canvas: true,
        mask: Default::default(),
    };
    let mut params = json!({"file":file,"document":doc,"kind":"footage","selection":sel,
        "editable":false,"ignore_styles":false,"fps":30.0});
    let out = registry
        .execute(&mut stage, &author, "item.import_photoshop", params.clone())
        .unwrap()
        .output;
    let item = PrimPath::new(out.as_str().unwrap()).unwrap();
    doc.layers.reverse();
    let leaf = doc
        .layers
        .iter_mut()
        .find_map(|l| l.children.as_mut())
        .unwrap();
    leaf.reverse();
    let base = leaf.iter_mut().find(|l| l.key == key).unwrap();
    base.name = "Renamed".into();
    base.styles.clear();
    assert_eq!(doc.replacement(key, Some("Base")), Some(key));
    assert_eq!(
        doc.replacement(LayerKey::Index(99), Some("Renamed")),
        Some(key)
    );
    assert_eq!(doc.replacement(LayerKey::Id(99), Some("Missing")), None);
    params["document"] = json!(doc);
    params["replace"] = json!(item);
    params["keep_name"] = json!(true);
    let reloaded = registry
        .execute(&mut stage, &author, "item.import_photoshop", params)
        .unwrap()
        .output;
    assert_eq!(reloaded, out);
    let asset = stage.prim(&item).unwrap();
    assert_eq!(asset.value("name").and_then(Value::as_str), Some("Base"));
    assert_eq!(
        asset.value("psd:layer_name").and_then(Value::as_str),
        Some("Renamed")
    );
    assert_eq!(LayerImage::from_asset(&asset), Some(sel));
    assert!(bae_model::asset::file_revision(&asset) > 0);
    assert!(!stage.has_prim(&item.child("layer_styles").unwrap()));
}

#[test]
fn import_preserves_stack_masks_type_and_source_styles_and_undoes_once() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../bae-interop-psd/tests/fixtures/art.psd");
    let parsed = PsdFile::from_bytes(&std::fs::read(&file).unwrap()).unwrap();
    let doc = describe(&parsed).unwrap();
    let mut stage = Stage::new();
    let mut registry = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut registry);
    let author = AuthorId::new("test:psd").unwrap();
    let result = registry
        .execute(
            &mut stage,
            &author,
            "item.import_photoshop",
            json!({"file":file,"document":doc,
        "kind":"retain_size","editable":true,"ignore_styles":false,"fps":30.0}),
        )
        .unwrap();
    let comp = PrimPath::new(result.output.as_str().unwrap()).unwrap();
    let layers = stage.children(&comp.child("layers").unwrap());
    assert_eq!(layers.len(), 2);
    let title = stage.prim(&layers[0]).unwrap();
    let Some(Value::TextDocument(text)) = title.value("text:source") else {
        panic!("expected an editable point text layer")
    };
    assert_eq!(
        (text.text.as_str(), text.style.font.as_str(), text.style.font_size),
        ("Hello", "TestPS", 18.)
    );
    let group = stage.prim(&layers[1]).unwrap();
    let nested = group.value("source").unwrap().as_prim_ref().unwrap();
    assert_eq!(
        stage.prim(nested).unwrap().value("width"),
        Some(&Value::Int(3))
    );
    assert_eq!(stage.children(&layers[1].child("masks").unwrap()).len(), 1);
    let inner = stage.children(&nested.child("layers").unwrap());
    assert_eq!(inner.len(), 2);
    let clipped = stage.prim(&inner[0]).unwrap();
    assert_eq!(clipped.value("enabled"), Some(&Value::Bool(false)));
    assert_eq!(
        clipped.value("track_matte_layer").unwrap().as_prim_ref(),
        Some(&inner[1])
    );
    let base = stage.prim(&inner[1]).unwrap();
    let asset = base.value("source").unwrap().as_prim_ref().unwrap();
    assert_eq!(
        LayerImage::from_asset(&stage.prim(asset).unwrap())
            .unwrap()
            .key,
        LayerKey::Id(2)
    );
    assert_eq!(stage.children(&inner[1].child("masks").unwrap()).len(), 2);
    for owner in [asset, &inner[1]] {
        let style = stage.prim(&owner.child("layer_styles").unwrap()).unwrap();
        assert_eq!(style.value(&param_attr(106)), Some(&Value::Float(17.5)));
    }
    assert!(stage.undo(&author).is_some());
    assert!(!stage.has_prim(&comp));
}
