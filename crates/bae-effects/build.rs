//! Pack the shipped simulation controls from the checked-in AE reference.
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};

fn main() {
    let reference = "../../docs/reference/ae/effects/simulation.json";
    println!("cargo:rerun-if-changed={reference}");
    let directory = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let reference: Value = serde_json::from_slice(&fs::read(reference).unwrap()).unwrap();
    for effect in reference["effects"].as_array().unwrap() {
        let name = effect["matchName"].as_str().unwrap();
        let mut rows = Vec::new();
        for p in effect["properties"].as_array().unwrap() {
            let Some(id) = p["matchName"]
                .as_str()
                .and_then(|m| m.strip_prefix(name))
                .and_then(|m| m.strip_prefix('-'))
                .and_then(|m| m.parse::<u32>().ok())
            else {
                continue;
            };
            rows.push(json!([
                id,
                p["name"],
                p["propertyValueType"],
                p["defaultValue"],
                p["minValue"],
                p["maxValue"],
                p["dropdownOptions"],
                p["unitsText"],
                p["canVaryOverTime"]
            ]));
        }
        fs::write(
            directory.join(format!("{name}.json")),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
    }
}
