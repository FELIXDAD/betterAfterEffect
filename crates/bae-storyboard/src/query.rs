//! Read-only helpers over the composed stage: where storyboard data lives and how to
//! read it. The design UI, the edit workspace and agents use these instead of
//! hard-coding attribute names.
//!
//! Layout (see the crate docs):
//!
//! ```text
//! /artboards/<name>                Artboard   name, size, origin
//! /artboards/<name>/layers/<l>     Layer      the artwork (shape/text layers, first = top)
//! /boards/<name>                   Board      name, frame_rate, width, height
//! /boards/<name>/panels/<p>        Panel      shot_code, duration, shot_size, … (order = story order)
//! /shots/<code>                    Shot       name, code, comp
//! /sequences/<name>                Sequence   animatic made by board.make_animatic
//! /sequences/<name>/<track>/<clip> Track / Clip
//! ```

use bae_model::schema::attr_def;
use bae_model::{FrameRate, PrimPath, PrimType, PrimView, Stage, Time, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Folder of the `Artboard` prims (shared with 16's Illustrator import).
pub const ARTBOARDS_FOLDER: &str = "/artboards";
/// Folder of the `Board` (storyboard) prims.
pub const BOARDS_FOLDER: &str = "/boards";
/// Folder of the `Shot` prims.
pub const SHOTS_FOLDER: &str = "/shots";
/// Folder of the `Sequence` prims made from storyboards.
pub const SEQUENCES_FOLDER: &str = "/sequences";
/// Child folder of an artboard holding its artwork layers (same model as a comp's
/// `layers`: shape and text `Layer` prims, first child = topmost).
pub const ARTWORK: &str = "layers";
/// Child folder of a board holding its panels, in story order.
pub const PANELS: &str = "panels";

/// Attribute names of storyboard prims (`bae_model::schema` has their types and
/// defaults).
pub mod attr {
    /// Display name (artboards, boards, panels, shots, sequences).
    pub const NAME: &str = "name";
    /// Artboard size in pixels, `vec2`.
    pub const SIZE: &str = "size";
    /// Artboard top-left corner on the canvas, `vec2`.
    pub const ORIGIN: &str = "origin";
    /// Board / sequence frame rate.
    pub const FRAME_RATE: &str = "frame_rate";
    /// Board / sequence frame width in pixels.
    pub const WIDTH: &str = "width";
    /// Board / sequence frame height in pixels.
    pub const HEIGHT: &str = "height";
    /// Panel: shot number, e.g. `SH010` (镜头号).
    pub const SHOT_CODE: &str = "shot_code";
    /// Panel: the picture, an `Artboard` (vector) or an `Asset` (image).
    pub const IMAGE: &str = "image";
    /// Panel: how long it is on screen (时长), flicks.
    pub const DURATION: &str = "duration";
    /// Panel: framing (景别), see [`super::ShotSize`].
    pub const SHOT_SIZE: &str = "shot_size";
    /// Panel: dialogue / voice-over (台词).
    pub const DIALOGUE: &str = "dialogue";
    /// Panel: what happens (动作).
    pub const ACTION: &str = "action";
    /// Panel: camera movement (镜头运动).
    pub const CAMERA_MOVE: &str = "camera_move";
    /// Panel: sound effects and music cues (声音).
    pub const SOUND: &str = "sound";
    /// Panel: transition to the next panel (转场); empty = cut.
    pub const TRANSITION: &str = "transition";
    /// Panel: production notes (备注).
    pub const NOTES: &str = "notes";
    /// Panel / clip: the `Shot` linking panel, motion comp and edit clips.
    pub const SHOT: &str = "shot";
    /// Shot: shot number (same as the panel's `shot_code`).
    pub const CODE: &str = "code";
    /// Shot: the motion comp made from the panel.
    pub const COMP: &str = "comp";
    /// Sequence: the board it was generated from (`prim_ref`).
    pub const SOURCE_BOARD: &str = "storyboard:board";
}

/// Framing (景别), from wide to tight. `None` for graphics without people.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShotSize {
    #[default]
    None,
    /// Extreme long shot (大远景).
    Els,
    /// Long shot (远景).
    Ls,
    /// Full shot (全景).
    Fs,
    /// Medium shot (中景).
    Ms,
    /// Medium close-up (近景).
    Mcu,
    /// Close-up (特写).
    Cu,
    /// Extreme close-up (大特写).
    Ecu,
}

impl ShotSize {
    /// All values, wide to tight.
    pub const ALL: [ShotSize; 8] = [
        ShotSize::None,
        ShotSize::Els,
        ShotSize::Ls,
        ShotSize::Fs,
        ShotSize::Ms,
        ShotSize::Mcu,
        ShotSize::Cu,
        ShotSize::Ecu,
    ];

    /// The enum token stored in the `shot_size` attribute.
    pub fn token(self) -> &'static str {
        match self {
            ShotSize::None => "none",
            ShotSize::Els => "els",
            ShotSize::Ls => "ls",
            ShotSize::Fs => "fs",
            ShotSize::Ms => "ms",
            ShotSize::Mcu => "mcu",
            ShotSize::Cu => "cu",
            ShotSize::Ecu => "ecu",
        }
    }

    pub fn from_token(token: &str) -> Option<ShotSize> {
        ShotSize::ALL.into_iter().find(|s| s.token() == token)
    }

    /// Abbreviation shown on cards (`MS`); empty for `None`.
    pub fn abbreviation(self) -> &'static str {
        match self {
            ShotSize::None => "",
            ShotSize::Els => "ELS",
            ShotSize::Ls => "LS",
            ShotSize::Fs => "FS",
            ShotSize::Ms => "MS",
            ShotSize::Mcu => "MCU",
            ShotSize::Cu => "CU",
            ShotSize::Ecu => "ECU",
        }
    }

    /// English name (the UI translates it).
    pub fn title(self) -> &'static str {
        match self {
            ShotSize::None => "None",
            ShotSize::Els => "Extreme Long Shot",
            ShotSize::Ls => "Long Shot",
            ShotSize::Fs => "Full Shot",
            ShotSize::Ms => "Medium Shot",
            ShotSize::Mcu => "Medium Close-up",
            ShotSize::Cu => "Close-up",
            ShotSize::Ecu => "Extreme Close-up",
        }
    }
}

/// Static value of `attr` on `view`, falling back to the schema default.
pub fn static_value(view: &PrimView<'_>, attr: &str) -> Option<Value> {
    view.value(attr).cloned().or_else(|| {
        view.prim_type()
            .and_then(|t| attr_def(t, attr))
            .and_then(|d| d.default.to_value())
    })
}

fn string(view: &PrimView<'_>, attr: &str) -> String {
    match view.value(attr) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn vec2(view: &PrimView<'_>, attr: &str, default: [f64; 2]) -> [f64; 2] {
    match static_value(view, attr) {
        Some(Value::Vec2(v)) => v,
        _ => default,
    }
}

fn prim_ref(view: &PrimView<'_>, attr: &str) -> Option<PrimPath> {
    view.value(attr).and_then(Value::as_prim_ref).cloned()
}

fn children_of_type(stage: &Stage, folder: &PrimPath, ty: &PrimType) -> Vec<PrimPath> {
    stage
        .children(folder)
        .into_iter()
        .filter(|c| stage.prim(c).is_some_and(|v| v.is_a(ty)))
        .collect()
}

/// Display name: the `name` attribute, else the prim name.
pub fn display_name(stage: &Stage, path: &PrimPath) -> String {
    match stage.prim(path) {
        Some(view) => match view.value(attr::NAME) {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            _ => view.name().to_owned(),
        },
        None => path.name().to_owned(),
    }
}

fn folder(path: &str) -> PrimPath {
    PrimPath::new(path).expect("valid constant path")
}

// ---------------------------------------------------------------------------
// Artboards

/// An artboard as the design workspace shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArtboardInfo {
    pub path: PrimPath,
    pub name: String,
    /// Width and height in pixels.
    pub size: [f64; 2],
    /// Top-left corner on the canvas, pixels (y down).
    pub origin: [f64; 2],
}

impl ArtboardInfo {
    /// `[x0, y0, x1, y1]` on the canvas.
    pub fn rect(&self) -> [f64; 4] {
        [
            self.origin[0],
            self.origin[1],
            self.origin[0] + self.size[0],
            self.origin[1] + self.size[1],
        ]
    }
}

/// The artboards under `/artboards`, in Artboards panel order.
pub fn artboards(stage: &Stage) -> Vec<PrimPath> {
    children_of_type(stage, &folder(ARTBOARDS_FOLDER), &PrimType::ARTBOARD)
}

/// Settings of an artboard, or `None` when `path` is not an `Artboard`.
pub fn artboard_info(stage: &Stage, path: &PrimPath) -> Option<ArtboardInfo> {
    let view = stage.prim(path)?;
    if !view.is_a(&PrimType::ARTBOARD) {
        return None;
    }
    Some(ArtboardInfo {
        path: path.clone(),
        name: display_name(stage, path),
        size: vec2(&view, attr::SIZE, [1920.0, 1080.0]),
        origin: vec2(&view, attr::ORIGIN, [0.0, 0.0]),
    })
}

/// The artwork folder of an artboard.
pub fn artwork_folder(artboard: &PrimPath) -> PrimPath {
    artboard.child(ARTWORK).expect("valid name")
}

/// The artwork layers of an artboard, topmost first.
pub fn artwork_layers(stage: &Stage, artboard: &PrimPath) -> Vec<PrimPath> {
    children_of_type(stage, &artwork_folder(artboard), &PrimType::LAYER)
}

// ---------------------------------------------------------------------------
// Boards and panels

/// Storyboard settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoardInfo {
    pub path: PrimPath,
    pub name: String,
    pub frame_rate: FrameRate,
    pub width: u32,
    pub height: u32,
}

/// The storyboards under `/boards`.
pub fn boards(stage: &Stage) -> Vec<PrimPath> {
    children_of_type(stage, &folder(BOARDS_FOLDER), &PrimType::BOARD)
}

/// Settings of a storyboard, or `None` when `path` is not a `Board`.
pub fn board_info(stage: &Stage, path: &PrimPath) -> Option<BoardInfo> {
    let view = stage.prim(path)?;
    if !view.is_a(&PrimType::BOARD) {
        return None;
    }
    let int = |attr: &str, default: u32| match static_value(&view, attr) {
        Some(Value::Int(v)) => u32::try_from(v).unwrap_or(default),
        _ => default,
    };
    let frame_rate = match static_value(&view, attr::FRAME_RATE) {
        Some(Value::FrameRate(r)) => r,
        _ => FrameRate::new(24, 1),
    };
    Some(BoardInfo {
        path: path.clone(),
        name: display_name(stage, path),
        frame_rate,
        width: int(attr::WIDTH, 1920),
        height: int(attr::HEIGHT, 1080),
    })
}

/// The panels folder of a board.
pub fn panels_folder(board: &PrimPath) -> PrimPath {
    board.child(PANELS).expect("valid name")
}

/// The panels of a board, in story order.
pub fn board_panels(stage: &Stage, board: &PrimPath) -> Vec<PrimPath> {
    children_of_type(stage, &panels_folder(board), &PrimType::PANEL)
}

/// The board a panel belongs to.
pub fn owning_board(stage: &Stage, panel: &PrimPath) -> Option<PrimPath> {
    let board = panel.parent()?.parent()?;
    stage
        .prim(&board)
        .is_some_and(|v| v.is_a(&PrimType::BOARD))
        .then_some(board)
}

/// Whether `path` is a `Panel` prim.
pub fn is_panel(stage: &Stage, path: &PrimPath) -> bool {
    stage.prim(path).is_some_and(|v| v.is_a(&PrimType::PANEL))
}

/// Everything a panel card shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PanelInfo {
    pub path: PrimPath,
    /// Shot number (镜头号), e.g. `SH010`.
    pub shot_code: String,
    /// Optional display name.
    pub name: String,
    /// The picture: an `Artboard` or an `Asset`.
    pub image: Option<PrimPath>,
    /// Time on screen (时长), flicks.
    pub duration: Time,
    pub shot_size: ShotSize,
    pub dialogue: String,
    pub action: String,
    pub camera_move: String,
    pub sound: String,
    pub transition: String,
    pub notes: String,
    /// The linked `Shot`, once the panel was promoted to a comp.
    pub shot: Option<PrimPath>,
}

/// The fields of a panel, or `None` when `path` is not a `Panel`.
pub fn panel_info(stage: &Stage, path: &PrimPath) -> Option<PanelInfo> {
    let view = stage.prim(path)?;
    if !view.is_a(&PrimType::PANEL) {
        return None;
    }
    let duration = static_value(&view, attr::DURATION)
        .and_then(|v| v.as_time())
        .unwrap_or(Time::from_seconds(2.0));
    let shot_size = match view.value(attr::SHOT_SIZE) {
        Some(Value::Enum(t)) => ShotSize::from_token(t).unwrap_or_default(),
        _ => ShotSize::None,
    };
    Some(PanelInfo {
        path: path.clone(),
        shot_code: string(&view, attr::SHOT_CODE),
        name: string(&view, attr::NAME),
        image: prim_ref(&view, attr::IMAGE),
        duration,
        shot_size,
        dialogue: string(&view, attr::DIALOGUE),
        action: string(&view, attr::ACTION),
        camera_move: string(&view, attr::CAMERA_MOVE),
        sound: string(&view, attr::SOUND),
        transition: string(&view, attr::TRANSITION),
        notes: string(&view, attr::NOTES),
        shot: prim_ref(&view, attr::SHOT),
    })
}

/// A panel duration in whole frames of `rate`, at least one frame.
pub fn frames_of(duration: Time, rate: FrameRate) -> i64 {
    let frame = rate.frame_duration().0.max(1);
    ((duration.0 + frame / 2) / frame).max(1)
}

/// Total running time of a board: the sum of its panels' durations, each rounded to
/// whole frames of the board's frame rate.
pub fn board_duration(stage: &Stage, board: &PrimPath) -> Time {
    let Some(info) = board_info(stage, board) else {
        return Time::ZERO;
    };
    let frames: i64 = board_panels(stage, board)
        .iter()
        .filter_map(|p| panel_info(stage, p))
        .map(|p| frames_of(p.duration, info.frame_rate))
        .sum();
    info.frame_rate.frame_to_time(frames)
}

// ---------------------------------------------------------------------------
// Shots

/// The comp a shot points at, if it still exists.
pub fn shot_comp(stage: &Stage, shot: &PrimPath) -> Option<PrimPath> {
    let view = stage.prim(shot)?;
    let comp = prim_ref(&view, attr::COMP)?;
    stage.has_prim(&comp).then_some(comp)
}

/// The motion comp a panel was promoted to (panel → shot → comp).
pub fn panel_comp(stage: &Stage, panel: &PrimPath) -> Option<PrimPath> {
    let shot = prim_ref(&stage.prim(panel)?, attr::SHOT)?;
    shot_comp(stage, &shot)
}

/// What a panel shows (in its card, the animatic and the edit workspace): the
/// motion comp of its shot once it was promoted, else its picture (an `Artboard` or
/// an `Asset`). `None` for an empty panel.
pub fn panel_picture(stage: &Stage, panel: &PrimPath) -> Option<PrimPath> {
    panel_comp(stage, panel).or_else(|| {
        let image = prim_ref(&stage.prim(panel)?, attr::IMAGE)?;
        stage.has_prim(&image).then_some(image)
    })
}

/// The shots under `/shots`.
pub fn shots(stage: &Stage) -> Vec<PrimPath> {
    children_of_type(stage, &folder(SHOTS_FOLDER), &PrimType::SHOT)
}

/// The shot whose `comp` is `comp` (motion workspace → storyboard).
pub fn shot_of_comp(stage: &Stage, comp: &PrimPath) -> Option<PrimPath> {
    shots(stage)
        .into_iter()
        .find(|s| shot_comp(stage, s).as_ref() == Some(comp))
}

/// The panels linked to `shot`, in board order.
pub fn shot_panels(stage: &Stage, shot: &PrimPath) -> Vec<PrimPath> {
    boards(stage)
        .iter()
        .flat_map(|b| board_panels(stage, b))
        .filter(|p| {
            stage
                .prim(p)
                .and_then(|v| prim_ref(&v, attr::SHOT))
                .as_ref()
                == Some(shot)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shot codes (docs/agent-guide/04-storyboard.md §1)

/// A parsed shot code: `<prefix>SH<number><suffix>`, e.g. `SC01_SH015A` →
/// (`SC01_`, 15, `A`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShotCode {
    pub prefix: String,
    pub number: u32,
    pub suffix: String,
}

impl ShotCode {
    pub fn parse(code: &str) -> Option<ShotCode> {
        let at = code.rfind("SH")?;
        let rest = &code[at + 2..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return None;
        }
        Some(ShotCode {
            prefix: code[..at].to_owned(),
            number: digits.parse().ok()?,
            suffix: rest[digits.len()..].to_owned(),
        })
    }

    pub fn format(&self) -> String {
        format!("{}SH{:03}{}", self.prefix, self.number, self.suffix)
    }
}

/// The shot code for a new panel inserted at `index` among `codes` (the board's
/// codes in story order): steps of 10 at the end (`SH010`, `SH020` …), the midpoint
/// when inserting between two shots (`SH015`), then a letter (`SH010A`). The scene
/// prefix of the neighbouring shot is kept. Never returns a code in `codes`.
pub fn next_shot_code(codes: &[String], index: usize) -> String {
    let index = index.min(codes.len());
    let parsed: Vec<Option<ShotCode>> = codes.iter().map(|c| ShotCode::parse(c)).collect();
    let before = parsed[..index].iter().rev().flatten().next();
    let after = parsed[index..].iter().flatten().next();
    let prefix = before
        .or(after)
        .map(|c| c.prefix.clone())
        .unwrap_or_default();
    let taken = |c: &str| codes.iter().any(|x| x == c);
    let lo = before.map_or(0, |c| c.number);
    let candidate = match after {
        None => {
            // Appending: continue in steps of 10 after the highest number so far.
            let max = parsed.iter().flatten().map(|c| c.number).max().unwrap_or(0);
            let base = if index == codes.len() {
                max.max(lo)
            } else {
                lo
            };
            (base / 10 + 1) * 10
        }
        Some(hi) if hi.number > lo + 1 => lo + (hi.number - lo) / 2,
        Some(_) => lo,
    };
    let code = ShotCode {
        prefix: prefix.clone(),
        number: candidate,
        suffix: String::new(),
    }
    .format();
    if !taken(&code) && candidate != 0 {
        return code;
    }
    // No room between the neighbours: SH010A, SH010B …
    let number = candidate.max(1);
    for letter in 'A'..='Z' {
        let code = ShotCode {
            prefix: prefix.clone(),
            number,
            suffix: letter.to_string(),
        }
        .format();
        if !taken(&code) {
            return code;
        }
    }
    // Pathological: fall back to the next free step of 10 at the end.
    let mut n = (parsed.iter().flatten().map(|c| c.number).max().unwrap_or(0) / 10 + 1) * 10;
    loop {
        let code = ShotCode {
            prefix: prefix.clone(),
            number: n,
            suffix: String::new(),
        }
        .format();
        if !taken(&code) {
            return code;
        }
        n += 10;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(c: &[&str]) -> Vec<String> {
        c.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn shot_codes_follow_the_guide() {
        assert_eq!(next_shot_code(&[], 0), "SH010");
        assert_eq!(next_shot_code(&codes(&["SH010"]), 1), "SH020");
        assert_eq!(next_shot_code(&codes(&["SH010", "SH020"]), 1), "SH015");
        assert_eq!(next_shot_code(&codes(&["SH010", "SH015"]), 1), "SH012");
        assert_eq!(next_shot_code(&codes(&["SH010", "SH011"]), 1), "SH010A");
        assert_eq!(next_shot_code(&codes(&["SH010"]), 0), "SH005");
        assert_eq!(
            next_shot_code(&codes(&["SC01_SH010", "SC01_SH020"]), 2),
            "SC01_SH030"
        );
        // Deleted shots keep their number free; appending continues after the max.
        assert_eq!(next_shot_code(&codes(&["SH010", "SH040"]), 2), "SH050");
        // Free text codes are kept, new ones still follow the pattern.
        assert_eq!(next_shot_code(&codes(&["Opening"]), 1), "SH010");
    }

    #[test]
    fn shot_code_parse_round_trips() {
        let c = ShotCode::parse("SC02_SH015B").unwrap();
        assert_eq!(
            (c.prefix.as_str(), c.number, c.suffix.as_str()),
            ("SC02_", 15, "B")
        );
        assert_eq!(c.format(), "SC02_SH015B");
        assert!(ShotCode::parse("Opening").is_none());
    }

    #[test]
    fn frames_round_to_nearest_and_at_least_one() {
        let r = FrameRate::new(24, 1);
        assert_eq!(frames_of(Time::from_seconds(3.0), r), 72);
        assert_eq!(frames_of(Time::ZERO, r), 1);
        assert_eq!(frames_of(Time(r.frame_duration().0 * 3 / 2 + 1), r), 2);
    }
}
