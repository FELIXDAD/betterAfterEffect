//! The catalog's tables in the executable: `catalog.bin` is the menus, the macOS
//! application menu, the effect categories, the Chinese menu titles and AE's
//! default shortcuts as deflate-compressed text, decompressed once on first use.
//! The tables in `menu.rs`, `zh.rs` and `keys.rs` are the source (compiled into
//! tests only);
//! `BAE_UPDATE_DOCS=1 cargo test -p bae-ui-motion catalog` rewrites the file and
//! a test fails while it is stale.
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

const BLOB: &[u8] = include_bytes!("catalog.bin");

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

/// The text of the tables (what `catalog.bin` holds, uncompressed).
#[cfg(test)]
pub(super) fn encode(
    menus: &[(&str, &[Node])],
    mac_app_menu: &[Node],
    categories: &[&str],
    zh: &[(&str, &str)],
    keys: &[Key],
    mac_keys: &[(&str, &str, &str)],
    effects: &[AeEffect],
) -> String {
    use std::fmt::Write as _;
    fn field(s: &str) -> &str {
        assert!(!s.contains(['\t', '\n']), "{s:?}");
        s
    }
    fn write_nodes(out: &mut String, depth: usize, nodes: &[Node]) {
        for n in nodes {
            match n {
                Node::Item(it) => {
                    let _ = writeln!(
                        out,
                        "I\t{depth}\t{}\t{}\t{}\t{}\t{:?}\t{:?}\t{}\t{}",
                        it.ae,
                        field(it.title),
                        field(it.action),
                        field(it.params),
                        it.owner,
                        it.status,
                        if it.platform == Platform::Windows {
                            "W"
                        } else {
                            "A"
                        },
                        u8::from(it.sorted)
                    );
                }
                Node::Submenu(s) => {
                    let p = if s.platform == Platform::Windows {
                        "W"
                    } else {
                        "A"
                    };
                    let _ = writeln!(out, "S\t{depth}\t{}\t{p}", field(s.title));
                    write_nodes(out, depth + 1, s.children);
                }
                Node::Separator => {
                    let _ = writeln!(out, "-\t{depth}");
                }
                Node::Effects => {
                    let _ = writeln!(out, "X\t{depth}");
                }
            }
        }
    }
    let mut out = String::new();
    for (title, entries) in menus {
        let _ = writeln!(out, "@menu\t{}", field(title));
        write_nodes(&mut out, 0, entries);
    }
    out.push_str("@app\n");
    write_nodes(&mut out, 0, mac_app_menu);
    out.push_str("@categories\n");
    for c in categories {
        let _ = writeln!(out, "{}", field(c));
    }
    out.push_str("@zh\n");
    for (en, zh) in zh {
        let _ = writeln!(out, "{}\t{}", field(en), field(zh));
    }
    out.push_str("@keys\n");
    for k in keys {
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{:?}\t{:?}\t{}",
            field(k.context),
            field(k.command),
            field(k.keys),
            field(k.action),
            field(k.params),
            k.owner,
            k.status,
            k.flags
        );
    }
    out.push_str("@mackeys\n");
    for (ctx, cmd, keys) in mac_keys {
        let _ = writeln!(out, "{}\t{}\t{}", field(ctx), field(cmd), field(keys));
    }
    out.push_str("@effects\n");
    for e in effects {
        let category = categories
            .iter()
            .position(|c| *c == e.category)
            .expect("effect category is listed");
        let _ = writeln!(
            out,
            "{category}\t{}\t{}\t{}",
            field(e.match_name),
            field(e.name),
            field(e.name_zh)
        );
    }
    out
}

/// `catalog.bin` holds the source tables: `BAE_UPDATE_DOCS=1` rewrites it.
#[test]
fn catalog_bin_is_up_to_date() {
    use super::{EFFECT_CATEGORIES, KEYS, MAC_APP_MENU, MAC_KEYS, MENU_ZH, MENUS};
    let want = encode(
        MENUS,
        MAC_APP_MENU,
        EFFECT_CATEGORIES,
        MENU_ZH,
        KEYS,
        MAC_KEYS,
        &super::source_effects(),
    );
    if std::env::var_os("BAE_UPDATE_DOCS").is_some() {
        use std::io::Write as _;
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(want.as_bytes()).unwrap();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/catalog/catalog.bin");
        std::fs::write(path, enc.finish().unwrap()).unwrap();
        return;
    }
    let mut text = String::new();
    flate2::read::DeflateDecoder::new(BLOB)
        .read_to_string(&mut text)
        .unwrap();
    assert!(
        text == want,
        "src/catalog/catalog.bin is stale: run BAE_UPDATE_DOCS=1 cargo test -p bae-ui-motion catalog"
    );
    // Decoding gives back the same tables.
    let t = tables();
    assert_eq!(
        encode(
            t.menus,
            t.mac_app_menu,
            t.effect_categories,
            t.zh,
            t.keys,
            t.mac_keys,
            t.effects
        ),
        want
    );
}
