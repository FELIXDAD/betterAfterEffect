//! Prints what the importer reads from an Illustrator / PDF file.
//!
//! `cargo run -p bae-interop-ai --example ai_dump -- file.ai`

use bae_interop_ai::{AiItem, read_document};
use bae_vector::{Brush, PaintKind};

fn item(i: &AiItem, depth: usize) {
    let pad = "  ".repeat(depth);
    match i {
        AiItem::Group(g) => {
            println!(
                "{pad}group {:?} opacity {} blend {:?}{}{}",
                g.name.as_deref().unwrap_or(""),
                g.opacity,
                g.blend,
                if g.clip.is_some() { " CLIP" } else { "" },
                if g.hidden { " hidden" } else { "" },
            );
            for c in &g.items {
                item(c, depth + 1);
            }
        }
        AiItem::Path(p) => {
            let paints: Vec<String> = p
                .paints
                .iter()
                .map(|pt| {
                    let kind = match &pt.kind {
                        PaintKind::Fill(r) => format!("fill {r:?}"),
                        PaintKind::Stroke(s) => format!(
                            "stroke w{} {:?} {:?} dash{:?}",
                            s.width, s.cap, s.join, s.dashes
                        ),
                    };
                    let brush = match &pt.brush {
                        Brush::Solid(c) => format!("{c:?}"),
                        Brush::Gradient(g) => format!(
                            "{:?} {:?}->{:?} {} stops",
                            g.kind,
                            g.start,
                            g.end,
                            g.stops.len()
                        ),
                    };
                    format!("[{kind} {brush} a{}]", pt.opacity)
                })
                .collect();
            let first = p.subpaths.first().map(|s| s.vertices.first().copied());
            println!(
                "{pad}path {} sub, {} verts, first {:?}, opacity {} {:?} {}",
                p.subpaths.len(),
                p.subpaths.iter().map(|s| s.vertices.len()).sum::<usize>(),
                first.flatten(),
                p.opacity,
                p.blend,
                paints.join(" ")
            );
        }
        AiItem::Text(t) => println!(
            "{pad}text {:?} font {} size {} at {:?} rot {} fill {:?}",
            t.text, t.font, t.size, t.position, t.rotation, t.fill
        ),
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: ai_dump <file>");
    let doc = read_document(std::path::Path::new(&path)).unwrap_or_else(|e| panic!("{e}"));
    println!("{} ({:?})", doc.name, doc.source);
    for a in &doc.artboards {
        println!(
            "artboard {:?} size {:?} origin {:?}",
            a.name, a.size, a.origin
        );
        for l in &a.layers {
            println!(
                " layer {:?} visible {} locked {}",
                l.name, l.visible, l.locked
            );
            for i in &l.items {
                item(i, 2);
            }
        }
    }
    for w in &doc.warnings {
        println!("warning: {w}");
    }
}
