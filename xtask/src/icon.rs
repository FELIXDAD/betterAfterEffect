//! `cargo xtask icons`: renders the BAE application icon and writes the platform files
//! into `assets/icons/app/`:
//!
//! - `bae.icns`: macOS bundle icon (`CFBundleIconFile`), 16-1024 px, with the entry types
//!   `iconutil` writes (ARGB for 16 and 32 px, PNG from `ic11` up).
//! - `bae.ico`: Windows icon, PNG entries 16-256 px.
//! - `bae.res`: the same icon as a compiled Windows resource (`RT_GROUP_ICON` 1 plus one
//!   `RT_ICON` per size). `apps/bae/build.rs` hands it to the MSVC linker, so `bae.exe`
//!   carries the icon without a resource compiler or any build dependency.
//! - `bae.svg`: a vector preview of the same artwork (not used by the build).
//!
//! The artwork is defined here, in code, so every size is rendered from one source and the
//! output is reproducible: an ease curve between two keyframes on a dark rounded square.
//! The files are committed; `icons_are_up_to_date` fails when they drift from this code.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};
use vello_cpu::kurbo::{
    Affine, BezPath, Cap, Circle, Join, Line, Point, RoundedRect, Shape as _, Stroke,
};
use vello_cpu::peniko::Color;
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::util::Workspace;

const USAGE: &str = "\
Usage: cargo xtask icons

Renders the application icon and writes assets/icons/app/{bae.icns,bae.ico,bae.res,bae.svg}.
";

/// Directory of the generated icon files, relative to the workspace root.
pub const ICON_DIR: &str = "assets/icons/app";
/// Resource ordinal of the icon group in `bae.res` (`Icon::from_resource` in the app).
pub const GROUP_ICON_ID: u16 = 1;

/// How an icns entry stores its image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IcnsData {
    /// `ARGB` + run-length encoded channels (what `iconutil` writes for 16 and 32 px;
    /// macOS does not read PNG data in the small `icp4` / `icp5` / `icp6` slots).
    Argb,
    Png,
}

/// macOS icon entries: (OSType, pixel size, encoding), the types `iconutil` writes. Each
/// pixel size appears once (`ic11` = 16@2x and `ic12` = 32@2x also serve 32 and 64 px);
/// macOS picks representations by pixel size.
const ICNS_ENTRIES: [(&[u8; 4], u16, IcnsData); 8] = [
    (b"ic04", 16, IcnsData::Argb),
    (b"ic05", 32, IcnsData::Argb),
    (b"ic11", 32, IcnsData::Png),
    (b"ic12", 64, IcnsData::Png),
    (b"ic07", 128, IcnsData::Png),
    (b"ic08", 256, IcnsData::Png),
    (b"ic09", 512, IcnsData::Png),
    (b"ic10", 1024, IcnsData::Png),
];
/// Windows icon sizes (Explorer views, taskbar, title bar at 100-250 % scaling).
const ICO_SIZES: [u16; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

// ---------------------------------------------------------------------------
// Artwork
// ---------------------------------------------------------------------------

/// The artwork is drawn in a 1024 x 1024 canvas; positions below are fractions of the
/// rounded-square body.
const CANVAS: f64 = 1024.0;

const BACKGROUND: [u8; 3] = [0x1b, 0x1f, 0x2e];
const CURVE: [u8; 3] = [0xf4, 0xf1, 0xea];
const HANDLE: [u8; 3] = [0x5c, 0x66, 0x84];
const KEY_IN: [u8; 3] = [0xff, 0x8a, 0x3d];
const KEY_OUT: [u8; 3] = [0x2f, 0xd2, 0xb8];

/// Keyframes (start, end) and Bezier handles of the ease curve, as fractions of the body.
const P0: (f64, f64) = (0.20, 0.74);
const C1: (f64, f64) = (0.58, 0.74);
const C2: (f64, f64) = (0.42, 0.26);
const P3: (f64, f64) = (0.80, 0.26);

/// Where the body sits in the canvas.
#[derive(Clone, Copy, Debug)]
struct Layout {
    /// Space around the body, in canvas units.
    margin: f64,
    /// Corner radius of the body, in canvas units.
    radius: f64,
}

/// Apple's macOS icon grid: an 824-unit body centred in the 1024 canvas.
const MACOS: Layout = Layout {
    margin: 100.0,
    radius: 185.0,
};
/// Windows icons fill more of their square.
const WINDOWS: Layout = Layout {
    margin: 40.0,
    radius: 176.0,
};

/// Everything drawn, in canvas units, for one output size.
struct Artwork {
    body: RoundedRect,
    curve: BezPath,
    curve_width: f64,
    /// Handle lines and knobs; left out at small sizes, where they would only blur.
    handles: Option<([Line; 2], [Circle; 2], f64)>,
    keys: [(BezPath, [u8; 3]); 2],
}

impl Artwork {
    /// `pixels` is the output size; strokes and keyframes keep a minimum on-screen size.
    fn new(layout: Layout, pixels: f64) -> Self {
        let side = CANVAS - 2.0 * layout.margin;
        let at =
            |(x, y): (f64, f64)| Point::new(layout.margin + x * side, layout.margin + y * side);
        let unit = CANVAS / pixels; // canvas units per output pixel
        let body = RoundedRect::new(
            layout.margin,
            layout.margin,
            CANVAS - layout.margin,
            CANVAS - layout.margin,
            layout.radius,
        );
        let mut curve = BezPath::new();
        curve.move_to(at(P0));
        curve.curve_to(at(C1), at(C2), at(P3));
        let curve_width = (0.07 * side).max(1.6 * unit);
        let handles = (pixels > 32.0).then(|| {
            (
                [Line::new(at(P0), at(C1)), Line::new(at(P3), at(C2))],
                [
                    Circle::new(at(C1), 0.03 * side),
                    Circle::new(at(C2), 0.03 * side),
                ],
                0.022 * side,
            )
        });
        let half = (0.085 * side).max(2.6 * unit);
        let diamond = |c: Point| {
            let mut path = BezPath::new();
            path.move_to((c.x, c.y - half));
            path.line_to((c.x + half, c.y));
            path.line_to((c.x, c.y + half));
            path.line_to((c.x - half, c.y));
            path.close_path();
            path
        };
        Artwork {
            body,
            curve,
            curve_width,
            handles,
            keys: [(diamond(at(P0)), KEY_IN), (diamond(at(P3)), KEY_OUT)],
        }
    }
}

fn color([r, g, b]: [u8; 3]) -> Color {
    Color::from_rgb8(r, g, b)
}

fn curve_stroke(width: f64) -> Stroke {
    Stroke::new(width)
        .with_caps(Cap::Round)
        .with_join(Join::Round)
}

/// Renders the icon at `size` x `size` pixels: straight (not premultiplied) RGBA8.
fn render(layout: Layout, size: u16) -> Vec<u8> {
    let art = Artwork::new(layout, f64::from(size));
    let mut ctx = RenderContext::new(size, size);
    ctx.set_transform(Affine::scale(f64::from(size) / CANVAS));

    ctx.set_paint(color(BACKGROUND));
    ctx.fill_path(&art.body.to_path(0.1));
    if let Some((lines, knobs, width)) = &art.handles {
        ctx.set_paint(color(HANDLE));
        ctx.set_stroke(Stroke::new(*width).with_caps(Cap::Round));
        for line in lines {
            ctx.stroke_path(&line.to_path(0.1));
        }
        for knob in knobs {
            ctx.fill_path(&knob.to_path(0.1));
        }
    }
    ctx.set_paint(color(CURVE));
    ctx.set_stroke(curve_stroke(art.curve_width));
    ctx.stroke_path(&art.curve);
    for (diamond, rgb) in &art.keys {
        ctx.set_paint(color(*rgb));
        ctx.fill_path(diamond);
    }

    ctx.flush();
    let mut pixmap = Pixmap::new(size, size);
    ctx.render(&mut pixmap, &mut Resources::new());
    pixmap
        .take_unpremultiplied()
        .into_iter()
        .flat_map(|p| [p.r, p.g, p.b, p.a])
        .collect()
}

/// The macOS-layout artwork as SVG (canvas units), for previews.
fn svg() -> String {
    let art = Artwork::new(MACOS, CANVAS);
    let hex = |[r, g, b]: [u8; 3]| format!("#{r:02x}{g:02x}{b:02x}");
    let r = art.body.rect();
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1024 1024\" width=\"1024\" height=\"1024\">\n\
         <!-- Generated by `cargo xtask icons` (xtask/src/icon.rs). Do not edit. -->\n\
         <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"/>\n",
        r.x0,
        r.y0,
        r.width(),
        r.height(),
        MACOS.radius,
        hex(BACKGROUND)
    );
    if let Some((lines, knobs, width)) = &art.handles {
        for line in lines {
            out.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{width}\" stroke-linecap=\"round\"/>\n",
                line.p0.x,
                line.p0.y,
                line.p1.x,
                line.p1.y,
                hex(HANDLE)
            ));
        }
        for knob in knobs {
            out.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>\n",
                knob.center.x,
                knob.center.y,
                knob.radius,
                hex(HANDLE)
            ));
        }
    }
    out.push_str(&format!(
        "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n",
        art.curve.to_svg(),
        hex(CURVE),
        art.curve_width
    ));
    for (diamond, rgb) in &art.keys {
        out.push_str(&format!(
            "<path d=\"{}\" fill=\"{}\"/>\n",
            diamond.to_svg(),
            hex(*rgb)
        ));
    }
    out.push_str("</svg>\n");
    out
}

// ---------------------------------------------------------------------------
// File formats
// ---------------------------------------------------------------------------

fn encode_png(size: u16, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, u32::from(size), u32::from(size));
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(out)
}

/// One channel in the icns run-length encoding: a control byte below 0x80 copies the next
/// `c + 1` bytes, one at or above 0x80 repeats the next byte `c - 0x80 + 3` times.
fn icns_rle(channel: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < channel.len() {
        let run = channel[i..]
            .iter()
            .take(130)
            .take_while(|&&b| b == channel[i])
            .count();
        if run >= 3 {
            out.push(u8::try_from(0x80 + run - 3).expect("run <= 130"));
            out.push(channel[i]);
            i += run;
            continue;
        }
        // Literal bytes up to the next run of three (or 128 bytes).
        let start = i;
        while i < channel.len() && i - start < 128 {
            if i + 2 < channel.len() && channel[i] == channel[i + 1] && channel[i] == channel[i + 2]
            {
                break;
            }
            i += 1;
        }
        out.push(u8::try_from(i - start - 1).expect("literal <= 128"));
        out.extend_from_slice(&channel[start..i]);
    }
}

/// `ARGB` entry data: the alpha, red, green and blue planes, each run-length encoded.
fn icns_argb(rgba: &[u8]) -> Vec<u8> {
    let mut out = b"ARGB".to_vec();
    for channel in [3, 0, 1, 2] {
        let plane: Vec<u8> = rgba.as_chunks::<4>().0.iter().map(|p| p[channel]).collect();
        icns_rle(&plane, &mut out);
    }
    out
}

/// Apple icon family: `icns` header, then (OSType, length incl. 8-byte header, data).
fn icns(entries: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let total = 8 + entries.iter().map(|(_, png)| 8 + png.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(
        &u32::try_from(total)
            .expect("icns fits in u32")
            .to_be_bytes(),
    );
    for (kind, png) in entries {
        out.extend_from_slice(&kind[..]);
        let len = u32::try_from(8 + png.len()).expect("icns entry fits in u32");
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(png);
    }
    out
}

/// Width/height byte of ICO and group-icon directory entries (0 means 256).
fn ico_dimension(size: u16) -> u8 {
    u8::try_from(size).unwrap_or(0)
}

/// Windows `.ico`: ICONDIR, one ICONDIRENTRY per image, then the PNG images.
fn ico(images: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let count = u16::try_from(images.len()).expect("icon count fits in u16");
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]); // reserved, type 1 = icon
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (size, png) in images {
        out.extend_from_slice(&[ico_dimension(*size), ico_dimension(*size), 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&u32::try_from(png.len()).expect("fits").to_le_bytes());
        out.extend_from_slice(&u32::try_from(offset).expect("fits").to_le_bytes());
        offset += png.len();
    }
    for (_, png) in images {
        out.extend_from_slice(png);
    }
    out
}

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// `MOVEABLE | DISCARDABLE`, and additionally `PURE` for the group, as rc.exe writes them.
const ICON_FLAGS: u16 = 0x1010;
const GROUP_FLAGS: u16 = 0x1030;

/// One entry of a compiled resource file (`.res`, the format rc.exe writes and the MSVC
/// linker accepts as input): RESOURCEHEADER with ordinal type and name, language neutral.
fn push_resource(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).expect("fits").to_le_bytes()); // DataSize
    out.extend_from_slice(&32u32.to_le_bytes()); // HeaderSize
    for ordinal in [kind, name] {
        out.extend_from_slice(&0xffffu16.to_le_bytes());
        out.extend_from_slice(&ordinal.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&flags.to_le_bytes()); // MemoryFlags
    out.extend_from_slice(&0u16.to_le_bytes()); // LanguageId: neutral
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// Compiled resources: the empty leading entry, `RT_ICON` 1..=n (the PNG images) and
/// `RT_GROUP_ICON` [`GROUP_ICON_ID`] listing them (GRPICONDIR + 14-byte entries).
fn res(images: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    push_resource(&mut out, 0, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&u16::try_from(images.len()).expect("fits").to_le_bytes());
    for (index, (size, png)) in images.iter().enumerate() {
        let id = u16::try_from(index + 1).expect("fits");
        push_resource(&mut out, RT_ICON, id, ICON_FLAGS, png);
        group.extend_from_slice(&[ico_dimension(*size), ico_dimension(*size), 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&u32::try_from(png.len()).expect("fits").to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    push_resource(&mut out, RT_GROUP_ICON, GROUP_ICON_ID, GROUP_FLAGS, &group);
    out
}

/// The generated files, keyed by file name.
struct IconFiles {
    files: Vec<(&'static str, Vec<u8>)>,
}

fn generate() -> Result<IconFiles> {
    let mut icns_entries = Vec::new();
    for (kind, size, data) in ICNS_ENTRIES {
        let rgba = render(MACOS, size);
        let bytes = match data {
            IcnsData::Argb => icns_argb(&rgba),
            IcnsData::Png => encode_png(size, &rgba)?,
        };
        icns_entries.push((kind, bytes));
    }
    let mut ico_images = Vec::new();
    for size in ICO_SIZES {
        ico_images.push((size, encode_png(size, &render(WINDOWS, size))?));
    }
    Ok(IconFiles {
        files: vec![
            ("bae.icns", icns(&icns_entries)),
            ("bae.ico", ico(&ico_images)),
            ("bae.res", res(&ico_images)),
            ("bae.svg", svg().into_bytes()),
        ],
    })
}

pub fn run(args: Vec<String>) -> Result<()> {
    if let Some(arg) = args.first() {
        if arg == "-h" || arg == "--help" {
            print!("{USAGE}");
            return Ok(());
        }
        bail!("unknown option `{arg}` for `icons`\n\n{USAGE}");
    }
    let ws = Workspace::load()?;
    let dir = ws.workspace_root.join(ICON_DIR);
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    for (name, bytes) in generate()?.files {
        let path = dir.join(name);
        fs::write(&path, &bytes).with_context(|| format!("failed to write {}", path.display()))?;
        println!("{}  ({} bytes)", path.display(), bytes.len());
    }
    Ok(())
}

/// `assets/icons/app/<name>` under `workspace_root`.
pub fn icon_file(workspace_root: &Path, name: &str) -> PathBuf {
    workspace_root.join(ICON_DIR).join(name)
}

// ---------------------------------------------------------------------------
// Checking a built bae.exe
// ---------------------------------------------------------------------------

fn read_u16(bytes: &[u8], at: usize) -> Result<u16> {
    let raw = bytes
        .get(at..at + 2)
        .with_context(|| format!("truncated PE file (offset {at})"))?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32> {
    let raw = bytes
        .get(at..at + 4)
        .with_context(|| format!("truncated PE file (offset {at})"))?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

/// Whether a PE image (`.exe`) has an `RT_GROUP_ICON` resource, i.e. an icon Explorer
/// shows. Reads the resource directory's first level only.
pub fn pe_has_group_icon(bytes: &[u8]) -> Result<bool> {
    ensure!(bytes.starts_with(b"MZ"), "not a PE file (no MZ header)");
    let pe = read_u32(bytes, 0x3c)? as usize;
    ensure!(
        bytes.get(pe..pe + 4) == Some(b"PE\0\0"),
        "not a PE file (no PE signature)"
    );
    let coff = pe + 4;
    let sections = usize::from(read_u16(bytes, coff + 2)?);
    let optional = coff + 20;
    let optional_size = usize::from(read_u16(bytes, coff + 16)?);
    let directories = match read_u16(bytes, optional)? {
        0x20b => optional + 112, // PE32+
        0x10b => optional + 96,  // PE32
        magic => bail!("unknown PE optional header magic {magic:#x}"),
    };
    let resource_rva = read_u32(bytes, directories + 2 * 8)?;
    if resource_rva == 0 {
        return Ok(false);
    }
    let table = optional + optional_size;
    let mut root = None;
    for index in 0..sections {
        let section = table + 40 * index;
        let virtual_size = read_u32(bytes, section + 8)?;
        let virtual_address = read_u32(bytes, section + 12)?;
        let raw_size = read_u32(bytes, section + 16)?;
        let raw_pointer = read_u32(bytes, section + 20)?;
        let end = virtual_address + virtual_size.max(raw_size);
        if (virtual_address..end).contains(&resource_rva) {
            root = Some((resource_rva - virtual_address + raw_pointer) as usize);
            break;
        }
    }
    let root = root.context("resource directory is outside every section")?;
    let named = usize::from(read_u16(bytes, root + 12)?);
    let ids = usize::from(read_u16(bytes, root + 14)?);
    for entry in named..named + ids {
        let id = read_u32(bytes, root + 16 + 8 * entry)?;
        if id == u32::from(RT_GROUP_ICON) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_png(bytes: &[u8]) -> (u32, Vec<u8>) {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.width, info.height);
        buf.truncate(info.buffer_size());
        (info.width, buf)
    }

    /// (OSType, PNG) of every icns entry.
    fn icns_entries(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(&bytes[..4], b"icns");
        let total = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(total, bytes.len());
        let mut out = Vec::new();
        let mut at = 8;
        while at < total {
            let kind: [u8; 4] = bytes[at..at + 4].try_into().unwrap();
            let len = u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            out.push((kind, bytes[at + 8..at + len].to_vec()));
            at += len;
        }
        assert_eq!(at, total);
        out
    }

    /// (size, PNG) of every ico image, checking the directory against the images.
    fn ico_images(bytes: &[u8]) -> Vec<(u16, Vec<u8>)> {
        assert_eq!(&bytes[..4], &[0, 0, 1, 0]);
        let count = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
        (0..count)
            .map(|i| {
                let e = &bytes[6 + 16 * i..6 + 16 * (i + 1)];
                let size = if e[0] == 0 { 256 } else { u16::from(e[0]) };
                assert_eq!(e[0], e[1]);
                let len = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
                let offset = u32::from_le_bytes(e[12..16].try_into().unwrap()) as usize;
                let png = bytes[offset..offset + len].to_vec();
                assert_eq!(decode_png(&png).0, u32::from(size));
                (size, png)
            })
            .collect()
    }

    /// (type, name, data) of every entry of a `.res` file.
    fn res_entries(bytes: &[u8]) -> Vec<(u16, u16, Vec<u8>)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let size = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let header = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            assert_eq!(header, 32);
            assert_eq!(&bytes[at + 8..at + 10], &[0xff, 0xff]);
            assert_eq!(&bytes[at + 12..at + 14], &[0xff, 0xff]);
            let kind = u16::from_le_bytes([bytes[at + 10], bytes[at + 11]]);
            let name = u16::from_le_bytes([bytes[at + 14], bytes[at + 15]]);
            out.push((kind, name, bytes[at + 32..at + 32 + size].to_vec()));
            at += 32 + size;
            at = at.next_multiple_of(4);
        }
        out
    }

    fn max_channel_diff(a: &[u8], b: &[u8]) -> u8 {
        assert_eq!(a.len(), b.len());
        a.iter()
            .zip(b)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0)
    }

    fn assert_same_images(name: &str, committed: &[(u16, Vec<u8>)], fresh: &[(u16, Vec<u8>)]) {
        let sizes = |v: &[(u16, Vec<u8>)]| v.iter().map(|e| e.0).collect::<Vec<_>>();
        assert_eq!(sizes(committed), sizes(fresh), "{name}: sizes differ");
        for ((size, old), (_, new)) in committed.iter().zip(fresh) {
            // Rasterisation may differ by a rounding step between SIMD back ends.
            let diff = max_channel_diff(&decode_png(old).1, &decode_png(new).1);
            assert!(
                diff <= 3,
                "{name} {size} px differs from the generator by {diff}; run `cargo xtask icons`"
            );
        }
    }

    fn committed(name: &str) -> Vec<u8> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        fs::read(icon_file(&root, name))
            .unwrap_or_else(|e| panic!("{name}: {e}; run `cargo xtask icons`"))
    }

    #[test]
    fn icons_are_up_to_date() {
        let fresh = generate().unwrap().files;
        let get = |name: &str| fresh.iter().find(|f| f.0 == name).unwrap().1.clone();

        assert_eq!(
            String::from_utf8(committed("bae.svg")).unwrap(),
            String::from_utf8(get("bae.svg")).unwrap(),
            "bae.svg is stale; run `cargo xtask icons`"
        );
        let icns_rgba = |bytes: &[u8]| {
            icns_entries(bytes)
                .into_iter()
                .zip(ICNS_ENTRIES)
                .map(|((kind, data), (expected, size, encoding))| {
                    assert_eq!(kind, *expected);
                    let rgba = match encoding {
                        IcnsData::Argb => argb_to_rgba(&data, size),
                        IcnsData::Png => decode_png(&data).1,
                    };
                    (size, rgba)
                })
                .collect::<Vec<_>>()
        };
        let (old, new) = (
            icns_rgba(&committed("bae.icns")),
            icns_rgba(&get("bae.icns")),
        );
        for ((size, old), (_, new)) in old.iter().zip(&new) {
            let diff = max_channel_diff(old, new);
            assert!(
                diff <= 3,
                "bae.icns {size} px differs by {diff}; run `cargo xtask icons`"
            );
        }
        assert_same_images(
            "bae.ico",
            &ico_images(&committed("bae.ico")),
            &ico_images(&get("bae.ico")),
        );
        // bae.res must carry exactly the committed bae.ico images (the exe icon and the
        // .ico are the same artwork).
        let res = res_entries(&committed("bae.res"));
        let ico = ico_images(&committed("bae.ico"));
        let icons: Vec<&Vec<u8>> = res
            .iter()
            .filter(|e| e.0 == RT_ICON)
            .map(|e| &e.2)
            .collect();
        assert_eq!(icons, ico.iter().map(|e| &e.1).collect::<Vec<_>>());
    }

    /// Decodes an `ARGB` icns entry back to RGBA.
    fn argb_to_rgba(data: &[u8], size: u16) -> Vec<u8> {
        assert_eq!(&data[..4], b"ARGB");
        let n = usize::from(size) * usize::from(size);
        let mut planes = Vec::new();
        let mut at = 4;
        for _ in 0..4 {
            let mut plane = Vec::with_capacity(n);
            while plane.len() < n {
                let c = data[at];
                if c < 0x80 {
                    let len = usize::from(c) + 1;
                    plane.extend_from_slice(&data[at + 1..at + 1 + len]);
                    at += 1 + len;
                } else {
                    plane.extend(std::iter::repeat_n(data[at + 1], usize::from(c) - 0x80 + 3));
                    at += 2;
                }
            }
            assert_eq!(plane.len(), n, "a run crosses the channel end");
            planes.push(plane);
        }
        assert_eq!(at, data.len());
        (0..n)
            .flat_map(|i| [planes[1][i], planes[2][i], planes[3][i], planes[0][i]])
            .collect()
    }

    #[test]
    fn icns_layout() {
        let entries = icns_entries(&generate().unwrap().files[0].1);
        let kinds: Vec<[u8; 4]> = entries.iter().map(|e| e.0).collect();
        let expected: Vec<[u8; 4]> = ICNS_ENTRIES.iter().map(|e| *e.0).collect();
        assert_eq!(kinds, expected);
        for ((_, data), (_, size, encoding)) in entries.iter().zip(ICNS_ENTRIES) {
            match encoding {
                IcnsData::Argb => assert_eq!(argb_to_rgba(data, size), render(MACOS, size)),
                IcnsData::Png => assert_eq!(decode_png(data).0, u32::from(size)),
            }
        }
    }

    #[test]
    fn icns_rle_round_trips() {
        let mut plane = vec![7u8; 300]; // long run, split at 130
        plane.extend([1, 2, 3, 4, 4, 5]); // literals with a pair inside
        plane.extend([9; 3]); // shortest run
        plane.extend((0..=255).cycle().take(300)); // literals longer than 128
        let mut encoded = Vec::new();
        icns_rle(&plane, &mut encoded);
        // Decode with the same rules as `argb_to_rgba`, one plane.
        let mut decoded = Vec::new();
        let mut at = 0;
        while at < encoded.len() {
            let c = encoded[at];
            if c < 0x80 {
                let len = usize::from(c) + 1;
                decoded.extend_from_slice(&encoded[at + 1..at + 1 + len]);
                at += 1 + len;
            } else {
                decoded.extend(std::iter::repeat_n(
                    encoded[at + 1],
                    usize::from(c) - 0x80 + 3,
                ));
                at += 2;
            }
        }
        assert_eq!(decoded, plane);
    }

    #[test]
    fn res_has_one_group_listing_every_icon() {
        let images: Vec<(u16, Vec<u8>)> = vec![
            (16, vec![1, 2, 3]),
            (32, vec![4, 5, 6, 7, 8]),
            (256, vec![9]),
        ];
        let entries = res_entries(&res(&images));
        // Empty leading entry, three icons, the group.
        assert_eq!(entries.len(), 5);
        assert_eq!((entries[0].0, entries[0].1, entries[0].2.len()), (0, 0, 0));
        for (i, (size, png)) in images.iter().enumerate() {
            let (kind, name, data) = &entries[i + 1];
            assert_eq!((*kind, usize::from(*name)), (RT_ICON, i + 1));
            assert_eq!(data, png);
            let _ = size;
        }
        let (kind, name, group) = &entries[4];
        assert_eq!((*kind, *name), (RT_GROUP_ICON, GROUP_ICON_ID));
        assert_eq!(&group[..6], &[0, 0, 1, 0, 3, 0]);
        let entry = |i: usize| &group[6 + 14 * i..6 + 14 * (i + 1)];
        assert_eq!(entry(0)[0], 16);
        assert_eq!(entry(2)[0], 0); // 256 is written as 0
        assert_eq!(u16::from_le_bytes([entry(1)[12], entry(1)[13]]), 2); // icon id
        assert_eq!(u32::from_le_bytes(entry(1)[8..12].try_into().unwrap()), 5); // bytes
    }

    #[test]
    fn small_sizes_drop_the_handles() {
        assert!(Artwork::new(MACOS, 16.0).handles.is_none());
        assert!(Artwork::new(MACOS, 32.0).handles.is_none());
        assert!(Artwork::new(MACOS, 64.0).handles.is_some());
        // Strokes never get thinner than ~1.6 px on screen.
        let tiny = Artwork::new(WINDOWS, 16.0);
        assert!(tiny.curve_width * 16.0 / CANVAS >= 1.59);
    }

    #[test]
    fn rendering_draws_the_body_and_keyframes() {
        let size = 64u16;
        let rgba = render(MACOS, size);
        let px = |x: usize, y: usize| {
            let i = 4 * (y * usize::from(size) + x);
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        assert_eq!(px(0, 0)[3], 0, "outside the body is transparent");
        let [r, g, b, a] = px(32, 52);
        assert_eq!(a, 255);
        assert_eq!([r, g, b], BACKGROUND, "body colour below the curve");
        let side = f64::from(size) * (CANVAS - 2.0 * MACOS.margin) / CANVAS;
        let key = |(x, y): (f64, f64)| {
            let at = |v: f64| (f64::from(size) * MACOS.margin / CANVAS + v * side) as usize;
            px(at(x), at(y))
        };
        assert_eq!(key(P0)[..3], KEY_IN);
        assert_eq!(key(P3)[..3], KEY_OUT);
    }

    /// A minimal PE32+ image: one `.rsrc` section whose root directory lists `ids`.
    fn fake_pe(ids: &[u32]) -> Vec<u8> {
        let mut pe = vec![0u8; 0x400];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        pe[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes()); // sections
        pe[coff + 16..coff + 18].copy_from_slice(&240u16.to_le_bytes()); // optional size
        let optional = coff + 20;
        pe[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        let rsrc_dir = optional + 112 + 16;
        pe[rsrc_dir..rsrc_dir + 4].copy_from_slice(&0x2000u32.to_le_bytes()); // RVA
        let section = optional + 240;
        pe[section..section + 5].copy_from_slice(b".rsrc");
        pe[section + 8..section + 12].copy_from_slice(&0x100u32.to_le_bytes()); // vsize
        pe[section + 12..section + 16].copy_from_slice(&0x2000u32.to_le_bytes()); // vaddr
        pe[section + 16..section + 20].copy_from_slice(&0x100u32.to_le_bytes()); // raw size
        pe[section + 20..section + 24].copy_from_slice(&0x200u32.to_le_bytes()); // raw ptr
        let root = 0x200;
        let n = u16::try_from(ids.len()).unwrap();
        pe[root + 14..root + 16].copy_from_slice(&n.to_le_bytes());
        for (i, id) in ids.iter().enumerate() {
            let at = root + 16 + 8 * i;
            pe[at..at + 4].copy_from_slice(&id.to_le_bytes());
        }
        pe
    }

    #[test]
    fn finds_the_group_icon_in_a_pe_image() {
        assert!(pe_has_group_icon(&fake_pe(&[3, 14, 16])).unwrap());
        assert!(!pe_has_group_icon(&fake_pe(&[16, 24])).unwrap());
        assert!(pe_has_group_icon(b"not an exe").is_err());
    }

    /// macOS's own tools accept the generated files.
    #[cfg(target_os = "macos")]
    #[test]
    fn iconutil_reads_the_icns() {
        let dir = std::env::temp_dir().join(format!("bae-icns-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let icns = dir.join("bae.icns");
        fs::write(&icns, &generate().unwrap().files[0].1).unwrap();
        let iconset = dir.join("bae.iconset");
        let status = std::process::Command::new("iconutil")
            .args(["-c", "iconset", "-o"])
            .arg(&iconset)
            .arg(&icns)
            .status()
            .unwrap();
        let listed = fs::read_dir(&iconset)
            .map(|d| d.count())
            .unwrap_or_default();
        fs::remove_dir_all(&dir).unwrap();
        assert!(status.success());
        // iconutil names one file per (size, scale) slot it found.
        assert!(listed >= 6, "iconutil wrote {listed} images");
    }

    #[test]
    fn rect_helper_matches_layout() {
        // The SVG preview's body rectangle is the macOS layout's body.
        let r: vello_cpu::kurbo::Rect = Artwork::new(MACOS, CANVAS).body.rect();
        assert_eq!((r.x0, r.y0, r.x1, r.y1), (100.0, 100.0, 924.0, 924.0));
    }
}
