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

const SOURCES: &[&str] = &[
    "types.rs",
    "encode.rs",
    "menu.rs",
    "keys.rs",
    "zh.rs",
    "effects.tsv",
];

fn main() {
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
