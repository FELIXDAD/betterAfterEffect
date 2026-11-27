//! Generates `$OUT_DIR/catalog.bin`, the AE command catalog the app embeds (see
//! `src/catalog/blob.rs`): the source tables encoded as text and deflated. The
//! tables and their types are included from `src/catalog/` as they are; nothing
//! generated is kept in the repository.

// The included files carry the whole catalog API; this script uses the tables.
#![allow(dead_code)]

use std::io::Write as _;
use std::path::PathBuf;

#[path = "src/catalog/encode.rs"]
mod encode;
#[path = "src/catalog/keys.rs"]
mod keys;
#[path = "src/catalog/menu.rs"]
mod menu;
#[path = "src/catalog/types.rs"]
mod types;
#[path = "src/catalog/zh.rs"]
mod zh;

use types::*;

// Win32 menu handles and ExtendScript command IDs are different namespaces.
// Join the catalog to the measured findMenuCommandId results by menu path.
fn script_command_ids(out: &std::path::Path) {
    use std::collections::BTreeMap;
    const REFERENCE: &str = "../../docs/reference/ae/mac/menus.json";
    println!("cargo:rerun-if-changed={REFERENCE}");
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(REFERENCE).expect("AE menus"))
            .expect("menu reference JSON");
    let mut found = BTreeMap::new();
    fn reference(nodes: &serde_json::Value, parent: &str, found: &mut BTreeMap<String, u32>) {
        for n in nodes.as_array().into_iter().flatten() {
            let label = n["label"].as_str().unwrap_or_default().replace('…', "...");
            let path = format!("{parent}/{label}");
            if let Some(id) = n["aeCommandId"]
                .as_u64()
                .filter(|v| *v > 0 && *v <= u64::from(u32::MAX))
            {
                found.insert(path.clone(), id as u32);
            }
            if n["items"].is_array() {
                reference(&n["items"], &path, found);
            }
        }
    }
    fn catalog(
        nodes: &[Node],
        parent: &str,
        found: &BTreeMap<String, u32>,
        ids: &mut BTreeMap<u32, u32>,
    ) {
        for n in nodes {
            match n {
                Node::Item(i) if i.ae != 0 => {
                    if let Some(&id) =
                        found.get(&format!("{parent}/{}", i.title.replace('…', "...")))
                    {
                        ids.insert(i.ae, id);
                    }
                }
                Node::Submenu(s) => catalog(
                    s.children,
                    &format!("{parent}/{}", s.title.replace('…', "...")),
                    found,
                    ids,
                ),
                _ => {}
            }
        }
    }
    reference(&data["menus"], "", &mut found);
    let mut ids = BTreeMap::new();
    for &(name, nodes) in menu::MENUS {
        catalog(nodes, &format!("/{name}"), &found, &mut ids);
    }
    catalog(menu::MAC_APP_MENU, "/After Effects", &found, &mut ids);
    let pairs: Vec<_> = ids.into_iter().collect();
    std::fs::write(out.join("script_ids.rs"), format!("&{pairs:?}")).expect("script command IDs");
}

const SOURCES: &[&str] = &[
    "types.rs",
    "encode.rs",
    "menu.rs",
    "keys.rs",
    "zh.rs",
    "effects.tsv",
];

fn main() {
    script_command_ids(&PathBuf::from(
        std::env::var_os("OUT_DIR").expect("OUT_DIR"),
    ));
    for file in SOURCES {
        println!("cargo:rerun-if-changed=src/catalog/{file}");
    }
    let text = encode::encode(
        menu::MENUS,
        menu::MAC_APP_MENU,
        menu::EFFECT_CATEGORIES,
        zh::MENU_ZH,
        keys::KEYS,
        keys::MAC_KEYS,
        &encode::source_effects(),
    );
    let mut deflate = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    deflate.write_all(text.as_bytes()).expect("deflate");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("catalog.bin");
    std::fs::write(out, deflate.finish().expect("deflate")).expect("write catalog.bin");
}
