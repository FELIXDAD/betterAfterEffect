//! Insert the completed layer-space fill and its footage as one document edit.
use crate::{
    import::{ImportFile, ImportItems, ImportParams},
    layer::{CreateLayer, CreateLayerParams, LayerOutput},
    query,
    util::{fail, set},
};
use bae_model::{Command, CommandError, LayerKind, PrimPath, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::Deserialize;

pub struct AddFillLayer;
#[derive(Deserialize, JsonSchema)]
pub struct Params {
    layer: PrimPath,
    file: ImportFile,
    start: Time,
    duration: Time,
    origin: [f64; 2],
}
impl Command for AddFillLayer {
    type Params = Params;
    type Output = LayerOutput;
    const ID: &'static str = "vision.fill_layer";
    const TITLE: &'static str = "Generate Fill Layer";
    const DESCRIPTION: &'static str = "Add the generated image sequence above its source layer.";
    const FILESYSTEM: bool = true;
    fn run(&self, tx: &mut Transaction<'_>, p: Params) -> Result<LayerOutput, CommandError> {
        let comp = query::owning_comp(tx.stage(), &p.layer)
            .ok_or_else(|| fail("Select a source layer"))?;
        let index = query::comp_layers(tx.stage(), &comp)
            .iter()
            .position(|l| l == &p.layer)
            .ok_or_else(|| fail("Missing source layer"))?;
        if p.duration <= Time::ZERO || !p.origin.iter().all(|v| v.is_finite()) {
            return Err(fail("Invalid sequence range or origin"));
        }
        let result = ImportItems.run(
            tx,
            ImportParams {
                files: vec![p.file],
                folder_label: None,
                precompose: false,
                create_comp: false,
                parent: None,
                comp: None,
            },
        )?;
        let mut create =
            CreateLayerParams::with_source(&comp, LayerKind::Footage, &result.items[0], p.start);
        create.index = Some(index);
        let layer = CreateLayer.run(tx, create)?;
        for (k, v) in [
            ("parent", Value::PrimRef(p.layer)),
            ("transform:anchor_point", Value::Vec3([0.; 3])),
            (
                "transform:position",
                Value::Vec3([p.origin[0], p.origin[1], 0.]),
            ),
            ("in_point", Value::Time(p.start)),
            ("out_point", Value::Time(p.start + p.duration)),
        ] {
            set(tx, &layer.path, k, v)?;
        }
        Ok(layer)
    }
}
