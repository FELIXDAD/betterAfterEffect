//! Images for agents: `render.frame` and `render.contact_sheet`.
//!
//! Rendering itself is behind [`FrameRenderer`] so this crate stays GPU-free: the app
//! injects the compositor (workstream 04) when it lands. Until then
//! [`PlaceholderRenderer`] draws a labelled test card so the tools, budgets and
//! contact-sheet layout can be exercised end to end.

use bae_model::{PrimPath, Stage, Time};

/// Straight (non-premultiplied) sRGB RGBA8 pixels, row-major, no padding.
#[derive(Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl std::fmt::Debug for RgbaImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RgbaImage({}x{})", self.width, self.height)
    }
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, fill: [u8; 4]) -> Self {
        let n = width as usize * height as usize;
        let mut pixels = Vec::with_capacity(n * 4);
        for _ in 0..n {
            pixels.extend_from_slice(&fill);
        }
        RgbaImage {
            width,
            height,
            pixels,
        }
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = self.index(x, y);
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Fill a rectangle, clipped to the image.
    pub fn fill_rect(&mut self, x: i64, y: i64, w: i64, h: i64, color: [u8; 4]) {
        let x0 = x.clamp(0, self.width as i64) as u32;
        let y0 = y.clamp(0, self.height as i64) as u32;
        let x1 = (x + w).clamp(0, self.width as i64) as u32;
        let y1 = (y + h).clamp(0, self.height as i64) as u32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let i = self.index(xx, yy);
                self.pixels[i..i + 4].copy_from_slice(&color);
            }
        }
    }

    /// Copy `src` with its top-left corner at (`x`, `y`), clipped.
    pub fn blit(&mut self, src: &RgbaImage, x: u32, y: u32) {
        let w = src.width.min(self.width.saturating_sub(x));
        let h = src.height.min(self.height.saturating_sub(y));
        for row in 0..h {
            let s = src.index(0, row);
            let d = self.index(x, y + row);
            let n = w as usize * 4;
            self.pixels[d..d + n].copy_from_slice(&src.pixels[s..s + n]);
        }
    }

    /// Encode as PNG.
    pub fn to_png(&self) -> Result<Vec<u8>, RenderError> {
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, self.width, self.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc
                .write_header()
                .map_err(|e| RenderError::Failed(e.to_string()))?;
            writer
                .write_image_data(&self.pixels)
                .map_err(|e| RenderError::Failed(e.to_string()))?;
        }
        Ok(buf)
    }
}

/// One frame to render.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameRequest {
    pub comp: PrimPath,
    /// Comp frame number (from 0).
    pub frame: i64,
    /// The same instant in comp time.
    pub time: Time,
    /// Output size in pixels (comp size × scale).
    pub width: u32,
    pub height: u32,
    /// Total frames in the comp (for progress indicators).
    pub frame_count: i64,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RenderError {
    #[error("cannot render: {0}")]
    Unsupported(String),
    #[error("render failed: {0}")]
    Failed(String),
}

/// Renders comp frames for agents. Implemented by the compositor (04) in the app.
pub trait FrameRenderer: Send + Sync {
    fn render(&self, stage: &Stage, request: &FrameRequest) -> Result<RgbaImage, RenderError>;

    /// True for [`PlaceholderRenderer`]: tool results then tell the agent the image
    /// is not a real render.
    fn is_placeholder(&self) -> bool {
        false
    }
}

/// Test card: checkerboard, the frame number, and a progress bar along the bottom
/// (so frames of a contact sheet differ visibly).
#[derive(Clone, Copy, Debug, Default)]
pub struct PlaceholderRenderer;

const BORDER: [u8; 4] = [255, 140, 0, 255];

impl FrameRenderer for PlaceholderRenderer {
    fn render(&self, _stage: &Stage, req: &FrameRequest) -> Result<RgbaImage, RenderError> {
        let (w, h) = (req.width.max(1), req.height.max(1));
        let mut img = RgbaImage::new(w, h, [40, 40, 40, 255]);
        let cell = (w.min(h) / 8).max(4) as i64;
        for cy in 0..(h as i64 / cell + 1) {
            for cx in 0..(w as i64 / cell + 1) {
                if (cx + cy) % 2 == 0 {
                    img.fill_rect(cx * cell, cy * cell, cell, cell, [56, 56, 56, 255]);
                }
            }
        }
        let b = (w.min(h) / 100).max(1) as i64;
        img.fill_rect(0, 0, w as i64, b, BORDER);
        img.fill_rect(0, h as i64 - b, w as i64, b, BORDER);
        img.fill_rect(0, 0, b, h as i64, BORDER);
        img.fill_rect(w as i64 - b, 0, b, h as i64, BORDER);
        let progress = if req.frame_count > 1 {
            (req.frame.clamp(0, req.frame_count) as f64 / (req.frame_count - 1) as f64).min(1.0)
        } else {
            0.0
        };
        let bar_h = (h as i64 / 20).max(2);
        img.fill_rect(
            b,
            h as i64 - b - bar_h,
            ((w as i64 - 2 * b) as f64 * progress) as i64,
            bar_h,
            [80, 160, 255, 255],
        );
        let label = format!("f{}", req.frame);
        let scale = (h / 6 / GLYPH_H).max(1);
        let (lw, lh) = text_size(&label, scale);
        draw_text(
            &mut img,
            &label,
            (w as i64 - lw as i64) / 2,
            (h as i64 - lh as i64) / 2,
            scale,
            [235, 235, 235, 255],
        );
        Ok(img)
    }

    fn is_placeholder(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Tiny bitmap font for frame labels (digits and `f`).

const GLYPH_W: u32 = 3;
const GLYPH_H: u32 = 5;

fn glyph(c: char) -> Option<[u8; 5]> {
    Some(match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'f' => [0b011, 0b010, 0b111, 0b010, 0b010],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        _ => return None,
    })
}

/// Size of `text` drawn at `scale` (one-glyph-pixel gaps between characters).
pub fn text_size(text: &str, scale: u32) -> (u32, u32) {
    let n = text.chars().count() as u32;
    let w = if n == 0 {
        0
    } else {
        (n * (GLYPH_W + 1) - 1) * scale
    };
    (w, GLYPH_H * scale)
}

/// Draw digits, `f` and `-` (other characters leave a gap).
pub fn draw_text(img: &mut RgbaImage, text: &str, x: i64, y: i64, scale: u32, color: [u8; 4]) {
    let s = scale as i64;
    for (i, c) in text.chars().enumerate() {
        let Some(rows) = glyph(c) else { continue };
        let gx = x + i as i64 * (GLYPH_W as i64 + 1) * s;
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..GLYPH_W {
                if bits & (1 << (GLYPH_W - 1 - col)) != 0 {
                    img.fill_rect(gx + col as i64 * s, y + row as i64 * s, s, s, color);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Contact sheet

/// Lay frames out in a grid (row-major), each labelled with its frame number in the
/// top-left corner. All tiles are placed on cells the size of the largest tile.
pub fn contact_sheet(tiles: &[(i64, RgbaImage)], columns: u32) -> RgbaImage {
    const GAP: u32 = 4;
    let columns = columns.clamp(1, tiles.len().max(1) as u32);
    let rows = (tiles.len() as u32).div_ceil(columns).max(1);
    let cw = tiles.iter().map(|(_, t)| t.width).max().unwrap_or(1);
    let ch = tiles.iter().map(|(_, t)| t.height).max().unwrap_or(1);
    let mut sheet = RgbaImage::new(
        columns * cw + (columns + 1) * GAP,
        rows * ch + (rows + 1) * GAP,
        [16, 16, 16, 255],
    );
    let scale = (ch / 10 / GLYPH_H).clamp(1, 4);
    for (i, (frame, tile)) in tiles.iter().enumerate() {
        let (c, r) = (i as u32 % columns, i as u32 / columns);
        let x = GAP + c * (cw + GAP);
        let y = GAP + r * (ch + GAP);
        sheet.blit(tile, x, y);
        let label = format!("{frame}");
        let (lw, lh) = text_size(&label, scale);
        let pad = scale as i64 * 2;
        sheet.fill_rect(
            x as i64,
            y as i64,
            lw as i64 + 2 * pad,
            lh as i64 + 2 * pad,
            [0, 0, 0, 255],
        );
        draw_text(
            &mut sheet,
            &label,
            x as i64 + pad,
            y as i64 + pad,
            scale,
            [255, 255, 255, 255],
        );
    }
    sheet
}

// ---------------------------------------------------------------------------

/// Standard base64 with padding (MCP image content).
pub fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(frame: i64, w: u32, h: u32) -> FrameRequest {
        FrameRequest {
            comp: PrimPath::new("/c").unwrap(),
            frame,
            time: Time::ZERO,
            width: w,
            height: h,
            frame_count: 10,
        }
    }

    #[test]
    fn base64_matches_rfc4648() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }

    #[test]
    fn placeholder_png_round_trips() {
        let stage = Stage::new();
        let img = PlaceholderRenderer
            .render(&stage, &request(3, 64, 36))
            .unwrap();
        assert_eq!((img.width, img.height), (64, 36));
        assert_eq!(img.pixel(0, 0), BORDER);
        let png = img.to_png().unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let reader = decoder.read_info().unwrap();
        assert_eq!(reader.info().width, 64);
    }

    #[test]
    fn contact_sheet_grid() {
        let stage = Stage::new();
        let tiles: Vec<(i64, RgbaImage)> = (0..5)
            .map(|f| {
                (
                    f,
                    PlaceholderRenderer
                        .render(&stage, &request(f, 40, 20))
                        .unwrap(),
                )
            })
            .collect();
        let sheet = contact_sheet(&tiles, 3);
        assert_eq!(sheet.width, 3 * 40 + 4 * 4);
        assert_eq!(sheet.height, 2 * 20 + 3 * 4);
        // Label box in the corner of the first tile.
        assert_eq!(sheet.pixel(4, 4), [0, 0, 0, 255]);
    }
}
