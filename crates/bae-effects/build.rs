//! Generate the typed control tables for the shipped simulation effects.
use serde_json::Value;
use std::{collections::HashMap, env, fmt::Write, fs, path::PathBuf};

fn checkbox(effect: &str, id: u32) -> bool {
    matches!(
        (effect, id),
        ("CC Particle World", 49 | 51 | 52 | 53 | 55 | 60 | 61 | 92)
            | ("CC Particle Systems II", 29)
            | ("CSRainfall", 16)
            | ("CSSnowfall", 15 | 26)
            | ("APC Shatter", 22 | 96 | 144)
            | ("APC CardDanceCam", 90)
            | ("CC Hair", 4)
            | ("APC Foam", 12)
            | ("APC Wave World", 92)
    )
}

fn control(effect: &str, id: u32, p: &Value, english: &HashMap<&str, &str>) -> String {
    let v = &p["defaultValue"];
    let n = v.as_f64().unwrap_or_default();
    match p["propertyValueType"].as_str().unwrap() {
        "NO_VALUE" if p["name"] == "" => "GroupEnd".into(),
        "NO_VALUE" => "GroupStart".into(),
        "MASK_INDEX" => "Mask".into(),
        "LAYER_INDEX" => "Layer".into(),
        "COLOR" => format!(
            "Color({:?})",
            serde_json::from_value::<[f64; 4]>(v.clone()).unwrap()
        ),
        "TwoD_SPATIAL" => format!(
            "Point({:?})",
            serde_json::from_value::<[f64; 2]>(v.clone()).unwrap()
        ),
        _ if p["dropdownOptions"].is_array() => {
            let options = p["dropdownOptions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| {
                    let zh = item.as_str().unwrap();
                    (english.get(zh).copied().unwrap_or(zh), zh)
                })
                .collect::<Vec<_>>();
            format!("Popup({}, &{options:?})", n as i64)
        }
        _ if checkbox(effect, id) => format!("Checkbox({})", n != 0.),
        _ if p["unitsText"] == "度" && p["minValue"].is_null() => format!("Angle({n:?})"),
        _ => {
            let min = p["minValue"].as_f64().unwrap_or(-1_000_000.);
            let max = p["maxValue"].as_f64().unwrap_or(1_000_000.);
            let unit = match p["unitsText"].as_str() {
                Some("度") => "degrees",
                Some("像素") => "pixels",
                _ => "none",
            };
            format!("Slider({n:?}, {min:?}, {max:?}, {unit:?})")
        }
    }
}

fn main() {
    let path = "../../docs/reference/ae/effects/simulation.json";
    println!("cargo:rerun-if-changed={path}");
    let directory = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let reference: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let names = ["names.tsv", "options.tsv"].map(|file| {
        let path = format!("src/categories/simulation/particles/{file}");
        println!("cargo:rerun-if-changed={path}");
        fs::read_to_string(path).unwrap()
    });
    let [control_names, option_names] = names.each_ref().map(|table| {
        table
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .collect::<HashMap<_, _>>()
    });
    for effect in reference["effects"].as_array().unwrap() {
        let name = effect["matchName"].as_str().unwrap();
        if !matches!(
            name,
            "CC Particle World"
                | "CC Particle Systems II"
                | "CSRainfall"
                | "CSSnowfall"
                | "CC Bubbles"
                | "CC Drizzle"
                | "CC Star Burst"
                | "CC Scatterize"
                | "CC Pixel Polly"
                | "APC Shatter"
                | "APC CardDanceCam"
                | "ADBE Playgnd"
                | "CC Ball Action"
                | "CC Mr. Mercury"
                | "CC Hair"
                | "APC Foam"
                | "APC Wave World"
                | "APC Caustics"
        ) {
            continue;
        }
        let mut slots = String::new();
        let mut slot = 0;
        let mut rows = String::from("&[\n");
        for p in effect["properties"].as_array().unwrap() {
            let Some(id) = p["matchName"]
                .as_str()
                .and_then(|m| m.strip_prefix(name))
                .and_then(|m| m.strip_prefix('-'))
                .and_then(|m| m.parse::<u32>().ok())
            else {
                continue;
            };
            if name == "ADBE Playgnd" {
                writeln!(slots, "const P{id}:u32={slot}u;").unwrap();
                slot += 1;
            }
            let zh = p["name"].as_str().unwrap();
            let en = control_names.get(zh).copied().unwrap_or(zh);
            let kind = control(name, id, p, &option_names);
            let varies = p["canVaryOverTime"].as_bool().unwrap();
            writeln!(rows, "Control {{ id: {id}, en: {en:?}, zh: {zh:?}, kind: ControlKind::{kind}, varies: {varies} }},").unwrap();
        }
        if name == "ADBE Playgnd" {
            fs::write(directory.join("playground_slots.wgsl"), slots).unwrap();
        }
        rows.push_str("]\n");
        fs::write(directory.join(format!("{name}.rs")), rows).unwrap();
    }
}
