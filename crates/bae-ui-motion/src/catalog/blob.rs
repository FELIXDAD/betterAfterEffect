//! The catalog's tables in the executable: `catalog.bin` is the menus, the macOS
//! application menu, the effect categories, the Chinese menu titles, AE's
//! default shortcuts and AE's effects as deflate-compressed text, decompressed
//! once on first use. `build.rs` writes it into `$OUT_DIR` from the source
//! tables (`menu.rs`, `zh.rs`, `keys.rs`, `effects.tsv`; compiled into tests
//! only) with `encode.rs`; it is not in the repository.
//!
//! Text format, one entry per line, fields separated by tabs:
//! `@menu <title>` / `@app` / `@categories` / `@zh` / `@keys` / `@mackeys` /
//! `@effects` start a section; an effect is `category-index matchName name
//! chinese-name`; a key is `context command keys action params owner status flags`, a
//! macOS exception `context command keys`; in a menu,
//! `I depth ae title action params owner status platform sorted` is an item,
//! `S depth title platform` a submenu (its entries follow one level deeper),
//! `- depth` a separator and `X depth` the effect categories.

use std::io::Read as _;
use std::sync::OnceLock;

use super::{AeEffect, Item, Key, Node, Owner, Platform, Status, Submenu};

const BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/catalog.bin"));

/// The decoded tables.
pub(super) struct Tables {
    pub menus: &'static [(&'static str, &'static [Node])],
    pub mac_app_menu: &'static [Node],
    pub effect_categories: &'static [&'static str],
    pub zh: &'static [(&'static str, &'static str)],
    pub keys: &'static [Key],
    pub mac_keys: &'static [(&'static str, &'static str, &'static str)],
    pub effects: &'static [AeEffect],
}

pub(super) fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut text = String::new();
        flate2::read::DeflateDecoder::new(BLOB)
            .read_to_string(&mut text)
            .expect("catalog.bin is deflate-compressed UTF-8");
        decode(Box::leak(text.into_boxed_str()))
    })
}

fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

fn owner(s: &str) -> Owner {
    match s {
        "A" => Owner::A,
        "B" => Owner::B,
        "C" => Owner::C,
        "D" => Owner::D,
        "E" => Owner::E,
        _ => Owner::F,
    }
}

fn status(s: &str) -> Status {
    match s {
        "Done" => Status::Done,
        "Partial" => Status::Partial,
        "Todo" => Status::Todo,
        "Later" => Status::Later,
        "Adobe" => Status::Adobe,
        _ => Status::Dynamic,
    }
}

fn platform(s: &str) -> Platform {
    if s == "W" {
        Platform::Windows
    } else {
        Platform::All
    }
}

/// One menu being read: the top-level entries, then one frame per open submenu.
struct Frames {
    stack: Vec<(&'static str, Platform, Vec<Node>)>,
}

impl Frames {
    fn new() -> Self {
        Frames {
            stack: vec![("", Platform::All, Vec::new())],
        }
    }

    /// Closes submenus deeper than `depth`.
    fn close_to(&mut self, depth: usize) {
        while self.stack.len() > depth + 1 {
            let (title, platform, children) = self.stack.pop().expect("frame");
            let sub = Node::Submenu(Submenu {
                title,
                children: leak(children),
                platform,
            });
            self.stack.last_mut().expect("parent").2.push(sub);
        }
    }

    fn finish(mut self) -> &'static [Node] {
        self.close_to(0);
        leak(self.stack.pop().expect("top").2)
    }
}

fn decode(text: &'static str) -> Tables {
    let mut menus: Vec<(&'static str, &'static [Node])> = Vec::new();
    let mut mac_app_menu: &'static [Node] = &[];
    let mut categories: Vec<&'static str> = Vec::new();
    let mut zh: Vec<(&'static str, &'static str)> = Vec::new();
    let mut keys: Vec<Key> = Vec::new();
    let mut mac_keys: Vec<(&'static str, &'static str, &'static str)> = Vec::new();
    let mut effects: Vec<AeEffect> = Vec::new();
    #[derive(PartialEq)]
    enum Section {
        Menu(&'static str),
        App,
        Categories,
        Zh,
        Keys,
        MacKeys,
        Effects,
    }
    let mut section: Option<Section> = None;
    let mut frames = Frames::new();
    let mut end = |section: Option<Section>, frames: Frames| match section {
        Some(Section::Menu(title)) => menus.push((title, frames.finish())),
        Some(Section::App) => mac_app_menu = frames.finish(),
        _ => {}
    };
    for line in text.lines() {
        if let Some(head) = line.strip_prefix('@') {
            end(
                section.take(),
                std::mem::replace(&mut frames, Frames::new()),
            );
            section = Some(match head.split_once('\t') {
                Some(("menu", title)) => Section::Menu(title),
                _ if head == "app" => Section::App,
                _ if head == "categories" => Section::Categories,
                _ if head == "keys" => Section::Keys,
                _ if head == "mackeys" => Section::MacKeys,
                _ if head == "effects" => Section::Effects,
                _ => Section::Zh,
            });
            continue;
        }
        match section {
            Some(Section::Categories) => categories.push(line),
            Some(Section::Zh) => {
                if let Some(pair) = line.split_once('\t') {
                    zh.push(pair);
                }
            }
            Some(Section::Keys) => {
                let f: Vec<&'static str> = line.split('\t').collect();
                if f.len() == 8 {
                    keys.push(Key {
                        context: f[0],
                        command: f[1],
                        keys: f[2],
                        action: f[3],
                        params: f[4],
                        owner: owner(f[5]),
                        status: status(f[6]),
                        flags: f[7].parse().unwrap_or(0),
                    });
                }
            }
            Some(Section::Effects) => {
                let f: Vec<&'static str> = line.split('\t').collect();
                effects.push(AeEffect {
                    category: categories[f[0].parse::<usize>().unwrap_or(0)],
                    match_name: f[1],
                    name: f[2],
                    name_zh: f[3],
                    capabilities: f[4].parse().expect("effect capability flags"),
                });
            }
            Some(Section::MacKeys) => {
                let f: Vec<&'static str> = line.split('\t').collect();
                if f.len() == 3 {
                    mac_keys.push((f[0], f[1], f[2]));
                }
            }
            Some(Section::Menu(_) | Section::App) => {
                let f: Vec<&'static str> = line.split('\t').collect();
                let depth: usize = f.get(1).and_then(|d| d.parse().ok()).unwrap_or(0);
                frames.close_to(depth);
                let node = match f[0] {
                    "I" => Node::Item(Item {
                        ae: f[2].parse().unwrap_or(0),
                        title: f[3],
                        action: f[4],
                        params: f[5],
                        owner: owner(f[6]),
                        status: status(f[7]),
                        platform: platform(f[8]),
                        sorted: f[9] == "1",
                    }),
                    "S" => {
                        frames.stack.push((f[2], platform(f[3]), Vec::new()));
                        continue;
                    }
                    "X" => Node::Effects,
                    _ => Node::Separator,
                };
                frames.stack.last_mut().expect("frame").2.push(node);
            }
            None => {}
        }
    }
    end(section, frames);
    Tables {
        menus: leak(menus),
        mac_app_menu,
        effect_categories: leak(categories),
        zh: leak(zh),
        keys: leak(keys),
        mac_keys: leak(mac_keys),
        effects: leak(effects),
    }
}

/// The generated `catalog.bin` decodes to the source tables.
#[test]
fn catalog_bin_holds_the_source_tables() {
    use super::encode::{encode, source_effects};
    use super::{EFFECT_CATEGORIES, KEYS, MAC_APP_MENU, MAC_KEYS, MENU_ZH, MENUS};
    let want = encode(
        MENUS,
        MAC_APP_MENU,
        EFFECT_CATEGORIES,
        MENU_ZH,
        KEYS,
        MAC_KEYS,
        &source_effects(),
    );
    let t = tables();
    let got = encode(
        t.menus,
        t.mac_app_menu,
        t.effect_categories,
        t.zh,
        t.keys,
        t.mac_keys,
        t.effects,
    );
    assert!(
        got == want,
        "catalog.bin does not decode to the source tables"
    );
}
