//! The .ai / PDF reader.
//!
//! - Synthetic files written here mimic what Illustrator writes (layers as
//!   optional content, gradients as "clip + sh", group opacity as a
//!   transparency group, an artboard clip, CID text…) in both cross-reference
//!   styles; they run everywhere (CI, Windows).
//! - Real Illustrator 30 files (`tests/data/illustrator/*.ai`, made by
//!   `bridges/ai/tests/make-fixtures.jsx`) are checked when present; the test
//!   says so and passes when they are not there yet. `BAE_AI_TEST_DATA`
//!   points at another folder with the same files.

use std::io::Write as _;
use std::path::PathBuf;

use bae_anim::evaluate_comp;
use bae_interop_ai::{
    AiDocument, AiItem, AiLayer, AiPath, ImportOptions, SourceFormat, plan_import, read_bytes,
};
use bae_model::{AuthorId, CommandRegistry, LayerKind, Stage, Time};
use bae_vector::kurbo::Point;
use bae_vector::{BlendMode, Brush, FillRule, GradientKind, LineCap, LineJoin, PaintKind};

// ---------------------------------------------------------------- PDF writer

#[derive(Clone, Copy, PartialEq)]
enum Xref {
    /// Uncompressed objects and a classic `xref` table.
    Table,
    /// Flate-compressed streams, non-stream objects in an object stream and
    /// a cross-reference stream (Illustrator's style).
    Stream,
}

struct Writer {
    objects: Vec<Obj>,
}

enum Obj {
    Plain(String),
    Stream(String, Vec<u8>),
}

impl Writer {
    fn new() -> Self {
        Writer {
            objects: Vec::new(),
        }
    }

    /// Reserves an object number (1-based).
    fn reserve(&mut self) -> usize {
        self.objects.push(Obj::Plain("null".into()));
        self.objects.len()
    }

    fn set(&mut self, id: usize, body: impl Into<String>) {
        self.objects[id - 1] = Obj::Plain(body.into());
    }

    fn add(&mut self, body: impl Into<String>) -> usize {
        self.objects.push(Obj::Plain(body.into()));
        self.objects.len()
    }

    fn stream(&mut self, dict: impl Into<String>, data: &[u8]) -> usize {
        self.objects.push(Obj::Stream(dict.into(), data.to_vec()));
        self.objects.len()
    }

    fn finish(&self, root: usize, mode: Xref) -> Vec<u8> {
        let mut out = b"%PDF-1.6\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let n = self.objects.len();
        let mut offsets = vec![0usize; n + 1];
        let flate = |data: &[u8]| {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        };
        let write_stream =
            |out: &mut Vec<u8>, id: usize, dict: &str, data: &[u8], compress: bool| {
                let (data, filter) = if compress {
                    (flate(data), "/Filter/FlateDecode")
                } else {
                    (data.to_vec(), "")
                };
                let dict = dict
                    .strip_prefix("<<")
                    .and_then(|d| d.strip_suffix(">>"))
                    .unwrap_or(dict);
                out.extend(
                    format!(
                        "{id} 0 obj\n<<{dict}{filter}/Length {}>>stream\n",
                        data.len()
                    )
                    .bytes(),
                );
                out.extend(&data);
                out.extend(b"\nendstream\nendobj\n");
            };
        match mode {
            Xref::Table => {
                for (i, o) in self.objects.iter().enumerate() {
                    let id = i + 1;
                    offsets[id] = out.len();
                    match o {
                        Obj::Plain(b) => out.extend(format!("{id} 0 obj\n{b}\nendobj\n").bytes()),
                        Obj::Stream(d, data) => write_stream(&mut out, id, d, data, false),
                    }
                }
                let xref = out.len();
                out.extend(format!("xref\n0 {}\n0000000000 65535 f\r\n", n + 1).bytes());
                for off in &offsets[1..] {
                    out.extend(format!("{off:010} 00000 n\r\n").bytes());
                }
                out.extend(
                    format!(
                        "trailer\n<</Size {}/Root {root} 0 R>>\nstartxref\n{xref}\n%%EOF\n",
                        n + 1
                    )
                    .bytes(),
                );
            }
            Xref::Stream => {
                // Plain objects into one object stream (object n + 1).
                let objstm = n + 1;
                let mut header = String::new();
                let mut body = String::new();
                let mut index_in_stm = vec![None; n + 1];
                for (i, o) in self.objects.iter().enumerate() {
                    if let Obj::Plain(b) = o {
                        index_in_stm[i + 1] = Some(header.split_whitespace().count() / 2);
                        header.push_str(&format!("{} {} ", i + 1, body.len()));
                        body.push_str(b);
                        body.push('\n');
                    }
                }
                for (i, o) in self.objects.iter().enumerate() {
                    if let Obj::Stream(d, data) = o {
                        offsets[i + 1] = out.len();
                        write_stream(&mut out, i + 1, d, data, true);
                    }
                }
                let count = index_in_stm.iter().flatten().count();
                let stm_off = out.len();
                let data = format!("{header}{body}");
                write_stream(
                    &mut out,
                    objstm,
                    &format!("<</Type/ObjStm/N {count}/First {}>>", header.len()),
                    data.as_bytes(),
                    true,
                );
                // Cross-reference stream (object n + 2), W [1 4 2].
                let xref_id = n + 2;
                let xref_off = out.len();
                let mut rows = vec![0u8, 0, 0, 0, 0, 0xff, 0xff];
                for id in 1..=n {
                    match index_in_stm[id] {
                        Some(k) => {
                            rows.push(2);
                            rows.extend((objstm as u32).to_be_bytes());
                            rows.extend((k as u16).to_be_bytes());
                        }
                        None => {
                            rows.push(1);
                            rows.extend((offsets[id] as u32).to_be_bytes());
                            rows.extend(0u16.to_be_bytes());
                        }
                    }
                }
                for (off, _) in [(stm_off, objstm), (xref_off, xref_id)] {
                    rows.push(1);
                    rows.extend((off as u32).to_be_bytes());
                    rows.extend(0u16.to_be_bytes());
                }
                write_stream(
                    &mut out,
                    xref_id,
                    &format!("<</Type/XRef/Size {}/W[1 4 2]/Root {root} 0 R>>", n + 3),
                    &rows,
                    true,
                );
                out.extend(format!("startxref\n{xref_off}\n%%EOF\n").bytes());
            }
        }
        out
    }
}

/// A two-artboard document in Illustrator's style.
fn illustrator_like(mode: Xref) -> Vec<u8> {
    let mut w = Writer::new();
    let catalog = w.reserve();
    let pages = w.reserve();
    // Layers: "图层 1" (UTF-16 name), "Masked", "Hidden" (off).
    let l1 = w.add("<</Type/OCG/Name<FEFF56FE5C4200200031>>>");
    let l2 = w.add("<</Type/OCG/Name(Masked)>>");
    let l3 = w.add("<</Type/OCG/Name(Hidden)>>");
    w.set(
        catalog,
        format!(
            "<</Type/Catalog/Pages {pages} 0 R/OCProperties<</OCGs[{l1} 0 R {l2} 0 R {l3} 0 R]/D<</Order[{l1} 0 R {l2} 0 R {l3} 0 R]/ON[{l1} 0 R {l2} 0 R]/OFF[{l3} 0 R]>>>>>>"
        ),
    );
    let gs0 = w.add("<</Type/ExtGState/ca 1.0/CA 1.0/BM/Normal/SMask/None>>");
    let gs1 = w.add("<</Type/ExtGState/ca 0.499994/CA 0.499994/BM/Multiply/SMask/None>>");
    // Linear gradient orange -> blue (40 %) -> black.
    let f1 = w.add("<</FunctionType 2/Domain[0 1]/C0[1 0.5 0]/C1[0 0.25 1]/N 1>>");
    let f2 = w.add("<</FunctionType 2/Domain[0 1]/C0[0 0.25 1]/C1[0 0 0]/N 1>>");
    let f = w.add(format!(
        "<</FunctionType 3/Domain[0 1]/Bounds[0.4]/Encode[0 1 0 1]/Functions[{f1} 0 R {f2} 0 R]>>"
    ));
    let sh_lin = w.add(format!(
        "<</ShadingType 2/ColorSpace/DeviceRGB/Coords[0 0 1 0]/Domain[0 1]/Extend[true true]/Function {f} 0 R>>"
    ));
    // Radial: focal point offset from the centre, white -> red.
    let fr = w.add("<</FunctionType 2/Domain[0 1]/C0[1 1 1]/C1[1 0 0]/N 1>>");
    let sh_rad = w.add(format!(
        "<</ShadingType 3/ColorSpace/DeviceRGB/Coords[0.5 0 0 0 0 1]/Domain[0 1]/Extend[true true]/Function {fr} 0 R>>"
    ));
    // Group with opacity 50 %, multiply: a triangle and a square.
    let form = w.stream(
        "<</Type/XObject/Subtype/Form/BBox[0 0 400 300]/Group<</S/Transparency>>/Resources<<>>>>",
        b"1 0 0 rg\n250 120 m\n300 120 l\n300 170 l\nf\n0 0 1 rg\n320 120 40 40 re\nf\n",
    );
    // Simple font with a ToUnicode map (glyph 1 = 'H' …) and a CID font as
    // Illustrator writes them (Adobe-GB1, no ToUnicode).
    let cmap = w.stream(
        "<<>>",
        b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n1 begincodespacerange <00> <FF> endcodespacerange\n2 beginbfchar <01> <0048> <02> <0069> endbfchar\nendcmap CMapName currentdict /CMap defineresource pop end end",
    );
    let font1 = w.add(format!(
        "<</Type/Font/Subtype/Type1/BaseFont/ABCDEF+MyriadPro-Regular/ToUnicode {cmap} 0 R>>"
    ));
    let cid = w.add("<</Type/Font/Subtype/CIDFontType0/BaseFont/QWERTY+AdobeSongStd-Light/CIDSystemInfo<</Registry(Adobe)/Ordering(GB1)/Supplement 4>>>>");
    let font2 = w.add(format!(
        "<</Type/Font/Subtype/Type0/BaseFont/QWERTY+AdobeSongStd-Light/Encoding/Identity-H/DescendantFonts[{cid} 0 R]>>"
    ));
    let content1 = "/OC /MC2 BDC\n0 1 1 rg\n/GS0 gs\n0 290 10 10 re\nf\nEMC\n\
         /OC /MC1 BDC\nq\n0 0 200 150 re\nW n\n0.1 0.7 0.2 rg\nq 1 0 0 1 10 75 cm\n0 0 m\n0 49.706 40.294 90 90 90 c\n139.706 90 180 49.706 180 0 c\n180 -49.706 139.706 -90 90 -90 c\n40.294 -90 0 -49.706 0 0 c\nf\nQ\nEMC\n\
         /OC /MC0 BDC\nQ\nq\n0 300 400 -300 re\nW n\n\
         0.8 0.1 0.1 rg\n0 0 1 RG\n4 w 10 M 2 j 1 J [6 3]0 d\n140 250 -80 -60 re\n140 190 m\n20 280 160 -120 re\nB*\n\
         q\n220 120 120 -90 re\nW n\nq\n100 0 0 -100 230 75 cm\nBX /Sh0 sh EX Q\nQ\n\
         q\n20 20 m\n80 20 l\n80 80 l\nW n\n60 0 0 60 50 50 cm\n/Sh1 sh\nQ\n\
         q\n/GS1 gs\n/Fm0 Do\nQ\n\
         EMC\nQ\n\
         BT\n/OC /MC0 BDC\n0.2 0.2 0.2 rg\n/T1 1 Tf\n36 0 0 36 20 20 Tm\n<0102>Tj\n\
         /T2 1 Tf\n24 0 0 24 200 20 Tm\n<00290046004D004D0050>Tj\n0 -1.2 Td\n<0029>Tj\nEMC\nET\n";
    let c1 = w.stream("<<>>", content1.as_bytes());
    let res1 = format!(
        "<</ExtGState<</GS0 {gs0} 0 R/GS1 {gs1} 0 R>>/Shading<</Sh0 {sh_lin} 0 R/Sh1 {sh_rad} 0 R>>/XObject<</Fm0 {form} 0 R>>/Font<</T1 {font1} 0 R/T2 {font2} 0 R>>/Properties<</MC0 {l1} 0 R/MC1 {l2} 0 R/MC2 {l3} 0 R>>>>"
    );
    let p1 = w.add(format!(
        "<</Type/Page/Parent {pages} 0 R/MediaBox[0 0 400 300]/CropBox[0 0 400 300]/Contents {c1} 0 R/Resources {res1}/PieceInfo<</Illustrator<<>>>>>>"
    ));
    let c2 = w.stream(
        "<<>>",
        b"/OC /MC0 BDC\n1 0.9 0 rg\n20 10 160 80 re\nf\nEMC\n",
    );
    let p2 = w.add(format!(
        "<</Type/Page/Parent {pages} 0 R/MediaBox[0 0 200 100]/Contents {c2} 0 R/Resources<</Properties<</MC0 {l1} 0 R>>>>>>"
    ));
    w.set(
        pages,
        format!("<</Type/Pages/Kids[{p1} 0 R {p2} 0 R]/Count 2>>"),
    );
    w.finish(catalog, mode)
}

fn paths(items: &[AiItem]) -> Vec<&AiPath> {
    items
        .iter()
        .filter_map(|i| match i {
            AiItem::Path(p) => Some(p),
            _ => None,
        })
        .collect()
}

fn layer<'a>(doc: &'a AiDocument, artboard: usize, name: &str) -> &'a AiLayer {
    doc.artboards[artboard]
        .layers
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no layer {name}"))
}

fn check_synthetic(doc: &AiDocument) {
    assert_eq!(doc.source, SourceFormat::AiPdf);
    assert_eq!(doc.artboards.len(), 2);
    assert_eq!(doc.artboards[0].size, [400.0, 300.0]);
    assert_eq!(doc.artboards[1].size, [200.0, 100.0]);
    let names: Vec<&str> = doc.artboards[0]
        .layers
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(names, ["图层 1", "Masked", "Hidden"], "Layers panel order");
    assert!(!layer(doc, 0, "Hidden").visible);

    let main = &layer(doc, 0, "图层 1").items;
    // Topmost first: the two text frames, the transparency group, the radial
    // and linear gradient paths, the compound path.
    let AiItem::Text(chinese) = &main[0] else {
        panic!("{:?}", main[0])
    };
    assert_eq!(chinese.text, "Hello\rH", "CID text decoded, two lines");
    assert_eq!(chinese.font, "AdobeSongStd-Light");
    assert_eq!(chinese.position, [200.0, 280.0]);
    let AiItem::Text(hi) = &main[1] else {
        panic!("{:?}", main[1])
    };
    assert_eq!(hi.text, "Hi");
    assert_eq!(hi.font, "MyriadPro-Regular");
    assert_eq!(hi.size, 36.0);
    assert_eq!(hi.fill, Some([0.2, 0.2, 0.2, 1.0]));

    let AiItem::Group(group) = &main[2] else {
        panic!("{:?}", main[2])
    };
    assert_eq!(group.opacity, 0.5);
    assert_eq!(group.blend, BlendMode::Multiply);
    assert_eq!(group.items.len(), 2);

    let p = paths(&main[3..]);
    assert_eq!(p.len(), 3);
    let Brush::Gradient(radial) = &p[0].paints[0].brush else {
        panic!()
    };
    let GradientKind::Radial {
        highlight_length,
        highlight_angle,
    } = radial.kind
    else {
        panic!()
    };
    assert_eq!(radial.start, Point::new(50.0, 250.0));
    assert_eq!(radial.end, Point::new(110.0, 250.0));
    assert_eq!((highlight_length, highlight_angle), (50.0, 0.0));
    assert_eq!(
        p[0].subpaths[0].vertices.len(),
        3,
        "clip triangle, closed implicitly"
    );
    assert!(p[0].subpaths[0].closed);

    let Brush::Gradient(linear) = &p[1].paints[0].brush else {
        panic!()
    };
    assert_eq!(linear.kind, GradientKind::Linear);
    assert_eq!(linear.start, Point::new(230.0, 225.0));
    assert_eq!(linear.end, Point::new(330.0, 225.0));
    let offsets: Vec<f32> = linear.stops.iter().map(|s| s.offset).collect();
    assert_eq!(offsets, [0.0, 0.4, 1.0]);
    assert_eq!(linear.stops[1].color, [0.0, 0.25, 1.0, 1.0]);
    assert_eq!(
        p[1].subpaths[0].vertices[0],
        [220.0, 180.0],
        "the clip path is the shape"
    );

    let frame = p[2];
    assert_eq!(
        frame.subpaths.len(),
        2,
        "compound path, empty moveto dropped"
    );
    assert!(matches!(frame.paints[0].kind, PaintKind::Stroke(ref s)
        if s.width == 4.0 && s.cap == LineCap::Round && s.join == LineJoin::Bevel
            && s.dashes == [6.0, 3.0] && s.miter_limit == 10.0));
    assert!(matches!(
        frame.paints[1].kind,
        PaintKind::Fill(FillRule::EvenOdd)
    ));
    assert_eq!(frame.paints[1].brush, Brush::Solid([0.8, 0.1, 0.1, 1.0]));

    // The clip group survives; the artboard clip does not create one.
    let masked = &layer(doc, 0, "Masked").items;
    let AiItem::Group(clip) = &masked[0] else {
        panic!("{masked:?}")
    };
    let clip_path = &clip.clip.as_ref().expect("clip")[0];
    assert_eq!(clip_path.vertices[0], [0.0, 300.0]);
    assert_eq!(clip.items.len(), 1);

    let second = &layer(doc, 1, "图层 1").items;
    assert_eq!(paths(second)[0].subpaths[0].vertices[0], [20.0, 90.0]);
}

#[test]
fn reads_illustrator_style_pdfs_in_both_xref_styles() {
    for mode in [Xref::Table, Xref::Stream] {
        let doc = read_bytes("synthetic", &illustrator_like(mode)).unwrap();
        check_synthetic(&doc);
    }
}

#[test]
fn synthetic_file_imports_as_shape_layers() {
    let doc = read_bytes("poster", &illustrator_like(Xref::Stream)).unwrap();
    let mut stage = Stage::new();
    let plan = plan_import(&doc, &stage, &ImportOptions::default()).unwrap();
    plan.batch
        .execute(
            &CommandRegistry::with_builtins(),
            &mut stage,
            &AuthorId::system(),
        )
        .unwrap();
    assert_eq!(plan.report.comps.len(), 2);
    let comp = &plan.report.comps[0];
    let evaluated = evaluate_comp(&stage, comp, Time::ZERO).unwrap();
    let kinds: Vec<(String, LayerKind)> = evaluated
        .layers
        .iter()
        .map(|l| (l.name.clone(), l.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("Hello".to_owned(), LayerKind::Text),
            ("Hi".to_owned(), LayerKind::Text),
            ("图层 1".to_owned(), LayerKind::Shape),
            ("Masked".to_owned(), LayerKind::Shape),
            ("Hidden".to_owned(), LayerKind::Shape),
        ]
    );
    let mut warnings = Vec::new();
    let scene = bae_vector::shape_layer_scene(&evaluated.layers[2], &mut warnings);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!scene.is_empty());
    let bounds = scene.bounds(false).unwrap();
    // Layer space is centred on the comp: the frame starts at (20, 20) - (200, 150).
    assert!(
        (bounds.x0 - -180.0).abs() < 1e-6 && (bounds.y0 - -130.0).abs() < 1e-6,
        "{bounds:?}"
    );
}

#[test]
fn damaged_files_never_panic() {
    let good = illustrator_like(Xref::Stream);
    for cut in (0..good.len()).step_by(97) {
        let _ = read_bytes("cut", &good[..cut]);
    }
    let mut flipped = good.clone();
    for i in (40..flipped.len()).step_by(53) {
        flipped[i] ^= 0x5a;
    }
    let _ = read_bytes("flipped", &flipped);
    // A wrong startxref is repaired by scanning the file.
    let table = illustrator_like(Xref::Table);
    let text = String::from_utf8_lossy(&table).into_owned();
    let at = text.rfind("startxref\n").unwrap() + "startxref\n".len();
    let mut broken = table[..at].to_vec();
    broken.extend(b"12\n%%EOF\n");
    check_synthetic(&read_bytes("repaired", &broken).unwrap());
}

#[test]
fn plain_pdf_without_layers() {
    let mut w = Writer::new();
    let catalog = w.reserve();
    let pages = w.reserve();
    let c = w.stream("<<>>", b"0.5 g\n10 10 100 50 re\nf\n");
    let page = w.add(format!(
        "<</Type/Page/Parent {pages} 0 R/MediaBox[0 0 300 200]/Contents {c} 0 R>>"
    ));
    w.set(pages, format!("<</Type/Pages/Kids[{page} 0 R]/Count 1>>"));
    w.set(catalog, format!("<</Type/Catalog/Pages {pages} 0 R>>"));
    let doc = read_bytes("plain", &w.finish(catalog, Xref::Table)).unwrap();
    assert_eq!(doc.source, SourceFormat::Pdf);
    assert_eq!(doc.artboards[0].layers.len(), 1);
    assert_eq!(doc.artboards[0].layers[0].name, "Layer 1");
    let p = paths(&doc.artboards[0].layers[0].items);
    assert_eq!(p[0].paints[0].brush, Brush::Solid([0.5, 0.5, 0.5, 1.0]));
    assert_eq!(p[0].subpaths[0].vertices[0], [10.0, 190.0]);
}

// ---------------------------------------------------------------- real files

fn data_dir() -> PathBuf {
    std::env::var_os("BAE_AI_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/illustrator"))
}

fn real(name: &str) -> Option<AiDocument> {
    let path = data_dir().join(format!("{name}.ai"));
    if !path.exists() {
        eprintln!(
            "skipped: {} not found (Illustrator test data not present)",
            path.display()
        );
        return None;
    }
    Some(bae_interop_ai::read_document(&path).unwrap_or_else(|e| panic!("{name}: {e}")))
}

fn all_paths(items: &[AiItem]) -> usize {
    items
        .iter()
        .map(|i| match i {
            AiItem::Path(_) => 1,
            AiItem::Group(g) => all_paths(&g.items),
            AiItem::Text(_) => 0,
        })
        .sum()
}

#[test]
fn real_illustrator_files() {
    if let Some(doc) = real("basic") {
        assert_eq!(doc.source, SourceFormat::AiPdf);
        assert_eq!(doc.artboards.len(), 2);
        assert_eq!(doc.artboards[0].size, [400.0, 300.0]);
        let names: Vec<&str> = doc.artboards[0]
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        assert_eq!(names, ["Text", "图层 1", "Masked", "Hidden"]);
        assert!(!layer(&doc, 0, "Hidden").visible);
        let AiItem::Text(t) = &layer(&doc, 0, "Text").items[0] else {
            panic!()
        };
        assert_eq!(
            (t.text.as_str(), t.size, t.position),
            ("Hello", 36.0, [20.0, 280.0])
        );
        let AiItem::Group(logo) = &layer(&doc, 0, "图层 1").items[0] else {
            panic!()
        };
        assert_eq!(logo.opacity, 0.9);
        let p = paths(&logo.items);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].subpaths.len(), 2, "compound path");
        assert_eq!(p[1].opacity, 0.5);
        assert_eq!(p[1].blend, BlendMode::Multiply);
        assert!(
            matches!(&p[1].paints[0].brush, Brush::Gradient(g) if g.kind == GradientKind::Linear && g.stops.len() == 3)
        );
        assert!(
            matches!(&p[2].paints[0].brush, Brush::Gradient(g) if matches!(g.kind, GradientKind::Radial { .. }))
        );
        assert!(matches!(&layer(&doc, 0, "Masked").items[0], AiItem::Group(g) if g.clip.is_some()));
        // Badge: fill and stroke written separately by Illustrator, merged back.
        let badge = paths(&layer(&doc, 1, "图层 1").items);
        assert_eq!(badge.len(), 1);
        assert_eq!(badge[0].paints.len(), 2);
    }
    if let Some(doc) = real("paths") {
        assert_eq!(all_paths(&doc.artboards[0].layers[0].items), 7);
    }
    if let Some(doc) = real("paint") {
        let p = paths(&doc.artboards[0].layers[0].items);
        assert_eq!(p.len(), 8);
        // Topmost first: CMYK, grey, spot, rotated linear, elliptical radial,
        // gradient stroke, screen, overlay.
        assert_eq!(
            p[0].paints[0].brush,
            Brush::Solid([1.0, 0.4, 0.0, 1.0]),
            "CMYK 0/60/100/0"
        );
        assert_eq!(
            p[1].paints[0].brush,
            Brush::Solid([0.7, 0.7, 0.7, 1.0]),
            "30 % grey"
        );
        assert_eq!(
            p[2].paints[0].brush,
            Brush::Solid([1.0, 0.8, 0.6, 1.0]),
            "40 % spot"
        );
        assert!(
            matches!(&p[3].paints[0].brush, Brush::Gradient(g) if g.kind == GradientKind::Linear)
        );
        assert!(
            matches!(&p[4].paints[0].brush, Brush::Gradient(g) if matches!(g.kind, GradientKind::Radial { .. }))
        );
        assert!(
            matches!(&p[5].paints[0], bae_vector::Paint { kind: PaintKind::Stroke(s), brush: Brush::Gradient(_), .. } if s.width == 12.0)
        );
        assert_eq!((p[6].blend, p[6].opacity), (BlendMode::Screen, 0.7));
        assert_eq!(p[7].blend, BlendMode::Overlay);
    }
    if let Some(doc) = real("structure") {
        assert_eq!(doc.artboards.len(), 2);
        let names: Vec<&str> = doc.artboards[0]
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        assert_eq!(names, ["Top", "Nested", "Template"]);
        assert!(!layer(&doc, 0, "Template").visible);
        assert!(doc.artboards[1].layers.iter().any(|l| l.name == "Symbols"));
    }
    if let Some(doc) = real("text") {
        let texts: Vec<&str> = doc.artboards[0].layers[0]
            .items
            .iter()
            .filter_map(|i| match i {
                AiItem::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            [
                "Big small\rcentred line",
                "Turn",
                "你好，世界",
                "Area text wraps inside its frame. \rSecond sentence.",
                "along the path"
            ]
        );
    }
    // Every file imports cleanly.
    for name in ["basic", "paths", "paint", "structure", "text"] {
        if let Some(doc) = real(name) {
            let mut stage = Stage::new();
            let plan = plan_import(&doc, &stage, &ImportOptions::default()).unwrap();
            plan.batch
                .execute(
                    &CommandRegistry::with_builtins(),
                    &mut stage,
                    &AuthorId::system(),
                )
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}
