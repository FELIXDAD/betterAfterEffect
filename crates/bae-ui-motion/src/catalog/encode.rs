//! The text format of `catalog.bin` (see `blob`), written from the source
//! tables. Shared with `build.rs`, which deflates the text into
//! `$OUT_DIR/catalog.bin`; the tests check that decoding gives the tables back.

use super::{AeEffect, Key, Node, Platform};

/// The effects of the Effect menu (`effects.tsv`: category, matchName, English
/// name, Chinese name; `#` lines are comments).
pub fn source_effects() -> Vec<AeEffect> {
    include_str!("effects.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let f: Vec<&'static str> = l.split('\t').collect();
            AeEffect {
                category: f[0],
                match_name: f[1],
                name: f[2],
                name_zh: f[3],
            }
        })
        .collect()
}

/// The text of the tables (what `catalog.bin` holds, uncompressed).
pub fn encode(
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
