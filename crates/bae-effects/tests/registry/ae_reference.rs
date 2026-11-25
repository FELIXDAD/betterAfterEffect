//! Compare built-ins to the AE 26 inventory. The baseline stores only differences.
//! Each JSON line contains [field path, captured AE value, built-in value].
//! Set BAE_UPDATE_AE_BASELINE=1 when regenerating and review the resulting diff.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use bae_effects::{Builtin, Implementation};
use bae_model::{EffectParamDef, ParamKind, Value};
use serde::Deserialize;
use serde_json::{Value as Json, json};

#[derive(Deserialize)]
struct Category {
    #[serde(rename = "categorySlug")]
    slug: String,
    effects: Vec<Reference>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    match_name: String,
    display_name: String,
    properties: Option<Vec<Json>>,
    #[serde(default)]
    skipped: bool,
}

fn inventory(root: &Path) -> BTreeMap<String, (Reference, bool)> {
    let mut map = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let category: Category = serde_json::from_slice(&fs::read(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for fx in category.effects {
            let name = fx.match_name.clone();
            assert!(
                map.insert(name.clone(), (fx, category.slug == "expression-controls"))
                    .is_none(),
                "duplicate reference {name}"
            );
        }
    }
    map
}

// AE's capture rounds numbers to six decimal places.
fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(a), Json::Number(b)) => {
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() <= 0.0000005
        }
        (Json::Array(a), Json::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        _ => a == b,
    }
}

#[derive(Default)]
struct Differences(BTreeSet<String>);

impl Differences {
    fn field(&mut self, path: &str, field: &str, reference: Json, builtin: Json) {
        if !same(&reference, &builtin) {
            self.0.insert(
                serde_json::to_string(&(format!("{path}/{field}"), reference, builtin)).unwrap(),
            );
        }
    }

    fn effect(&mut self, fx: &Builtin, reference: &Reference, control: bool) {
        let name = &reference.match_name;
        self.field(
            name,
            "name_zh",
            json!(reference.display_name),
            json!(fx.info.name_zh),
        );
        if control {
            assert!(
                !fx.effect().renders_pixels(),
                "{name}: parameter-only control"
            );
        }
        let Some(props) = &reference.properties else {
            assert!(reference.skipped, "{name}: missing parameter capture");
            return;
        };
        let props = props
            .iter()
            .filter(|p| p["matchName"] != "ADBE Effect Built In Params");
        let params: Vec<_> = fx
            .def()
            .params
            .iter()
            .filter_map(|p| Some((p.match_name.as_deref()?, p)))
            .collect();
        let mut builtin: BTreeMap<_, _> = params.iter().copied().collect();
        assert_eq!(
            builtin.len(),
            params.len(),
            "{name}: duplicate parameter matchName"
        );
        let mut ordered = Vec::new();
        for prop in props {
            let id = prop["matchName"].as_str().unwrap();
            if let Some(p) = builtin.remove(id) {
                ordered.push(id);
                self.param(fx, p, prop, id);
            } else {
                self.field(id, "present", json!(true), json!(false));
            }
        }
        for id in builtin.keys() {
            self.field(id, "present", json!(false), json!(true));
        }
        for (index, ((id, _), expected)) in params
            .iter()
            .filter(|(id, _)| !builtin.contains_key(id))
            .zip(ordered)
            .enumerate()
        {
            self.field(
                name,
                &format!("order[{}]", index + 1),
                json!(expected),
                json!(id),
            );
        }
    }

    fn param(&mut self, fx: &Builtin, p: &EffectParamDef, prop: &Json, id: &str) {
        if let Some(disk) = id.rsplit_once('-').and_then(|(_, n)| n.parse::<u32>().ok()) {
            self.field(id, "disk_id", json!(disk), json!(p.id));
        }
        // A group-end marker is never displayed. AE repeats a preceding label for it.
        if !matches!(p.kind, ParamKind::GroupEnd) {
            self.field(
                id,
                "name_zh",
                prop["name"].clone(),
                json!(fx.info.param_name_zh(p.id)),
            );
        }
        let ty = match p.kind {
            ParamKind::Slider { .. }
            | ParamKind::Angle
            | ParamKind::Checkbox
            | ParamKind::Popup { .. } => "OneD",
            ParamKind::Color => "COLOR",
            ParamKind::Point => "TwoD_SPATIAL",
            ParamKind::Point3d => "ThreeD_SPATIAL",
            ParamKind::Layer => "LAYER_INDEX",
            ParamKind::Path => "MASK_INDEX",
            ParamKind::CurvePoints | ParamKind::ArbitraryData => "CUSTOM_VALUE",
            ParamKind::GroupStart | ParamKind::GroupEnd | ParamKind::Button => "NO_VALUE",
        };
        self.field(id, "type", prop["propertyValueType"].clone(), json!(ty));
        let expected = &prop["defaultValue"];
        let implicit_layer = p.default.is_none()
            && matches!(p.kind, ParamKind::Layer | ParamKind::Path)
            && matches!(expected.as_i64(), Some(0 | 1));
        if !(implicit_layer || expected.is_null() && prop["propertyValueType"] == "CUSTOM_VALUE") {
            let value = match &p.default {
                Some(Value::Int(v)) => json!(v),
                Some(Value::Float(v)) => json!(v),
                Some(Value::Bool(v)) => json!(u8::from(*v)),
                Some(Value::Color(v)) => json!(v),
                Some(Value::Vec2([x, y])) if p.flags.relative_default => {
                    json!([x * 640., y * 360.])
                }
                Some(Value::Vec3([x, y, z])) if p.flags.relative_default => {
                    json!([x * 640., y * 360., z])
                }
                Some(Value::Vec2(v)) => json!(v),
                Some(Value::Vec3(v)) => json!(v),
                other => json!(other),
            };
            self.field(id, "default", expected.clone(), value);
        }
        if let ParamKind::Slider {
            valid_min,
            valid_max,
            ..
        } = p.kind
        {
            self.field(id, "min", prop["minValue"].clone(), json!(valid_min));
            self.field(id, "max", prop["maxValue"].clone(), json!(valid_max));
        }
        if let Some(choices) = prop["dropdownOptions"].as_array() {
            let ParamKind::Popup { options } = &p.kind else {
                self.field(id, "popup", json!(true), json!(false));
                return;
            };
            let labels = fx
                .info
                .param_options_zh
                .iter()
                .find(|(disk, _)| *disk == p.id)
                .map_or(options, |(_, v)| v);
            self.field(
                id,
                "choice_count",
                json!(choices.len()),
                json!(options.len()),
            );
            assert_eq!(
                labels.len(),
                options.len(),
                "{id}: incomplete popup translation"
            );
            for (index, (ae, label)) in choices.iter().zip(labels).enumerate() {
                self.field(
                    id,
                    &format!("choice[{}]", index + 1),
                    ae.clone(),
                    json!(label),
                );
            }
        }
    }
}

pub(super) fn check() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let references = inventory(&root.join("../../docs/reference/ae/effects"));
    let mut differences = Differences::default();
    let builtins = bae_effects::builtins();
    assert!(!builtins.is_empty());
    for fx in builtins {
        if let Some((reference, control)) = references.get(&fx.info.match_name) {
            differences.effect(fx, reference, *control);
        } else {
            // Native effects use their contribution id, declarative packages advertise
            // their AE identities, and obsolete aliases can lack a separate capture.
            let native = match &fx.implementation {
                Implementation::Declarative(effect) => effect.decl().ae_match_names.is_empty(),
                Implementation::Rust(_) => fx.info.full_id == fx.info.match_name,
            };
            assert!(
                native || fx.info.obsolete,
                "{}: missing AE reference",
                fx.info.match_name
            );
        }
    }
    let path = root.join("tests/registry/ae-differences.jsonl");
    if std::env::var("BAE_UPDATE_AE_BASELINE").as_deref() == Ok("1") {
        let mut text = String::new();
        for line in &differences.0 {
            text.push_str(line);
            text.push('\n');
        }
        fs::write(&path, text).unwrap();
    }
    let baseline: BTreeSet<String> = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let new: Vec<_> = differences.0.difference(&baseline).collect();
    let stale: Vec<_> = baseline.difference(&differences.0).collect();
    assert!(
        new.is_empty() && stale.is_empty(),
        "new AE differences: {new:#?}\nstale AE differences: {stale:#?}"
    );
}
