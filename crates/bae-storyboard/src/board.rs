//! Storyboard and panel commands: `board.create`, `board.update`, `panel.create`,
//! `panel.update`, `panel.move`, `panel.delete`.
//!
//! Panel fields follow `docs/agent-guide/04-storyboard.md`: shot number, duration
//! (whole frames of the board's frame rate), shot size, dialogue, action, camera
//! movement, sound, transition and notes.

use bae_model::{Command, CommandError, FrameRate, PrimPath, PrimType, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, BOARDS_FOLDER, ShotSize, attr};
use crate::util::{apply_order, child, create, delete, ensure_folder, fail, move_to, path, set};

/// Output of `board.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardOutput {
    /// The new storyboard.
    pub path: PrimPath,
    /// Its panels folder.
    pub panels_folder: PrimPath,
    /// The panels created with it, in order.
    pub panels: Vec<PrimPath>,
}

/// Output of `panel.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PanelOutput {
    pub path: PrimPath,
    /// The panel's shot number.
    pub shot_code: String,
}

/// Output of commands that move panels.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PanelsOutput {
    /// The panels' paths afterwards, in the order given.
    pub paths: Vec<PrimPath>,
}

fn check_fps(fps: f64) -> Result<FrameRate, CommandError> {
    if fps.is_finite() && fps > 0.0 && fps <= 999.0 {
        Ok(FrameRate::from_fps(fps))
    } else {
        Err(fail("fps must be between 0 and 999"))
    }
}

fn check_dimension(v: u32) -> Result<i64, CommandError> {
    if (1..=30_000).contains(&v) {
        Ok(i64::from(v))
    } else {
        Err(fail("frame size must be between 1 and 30000 pixels"))
    }
}

fn require_board(tx: &Transaction<'_>, board: &PrimPath) -> Result<query::BoardInfo, CommandError> {
    query::board_info(tx.stage(), board).ok_or_else(|| fail(format!("{board} is not a storyboard")))
}

fn require_panel(tx: &Transaction<'_>, panel: &PrimPath) -> Result<PrimPath, CommandError> {
    if !query::is_panel(tx.stage(), panel) {
        return Err(fail(format!("{panel} is not a storyboard panel")));
    }
    query::owning_board(tx.stage(), panel)
        .ok_or_else(|| fail(format!("{panel} is not in a storyboard")))
}

fn board_codes(tx: &Transaction<'_>, board: &PrimPath) -> Vec<String> {
    let stage = tx.stage();
    query::board_panels(stage, board)
        .iter()
        .filter_map(|p| query::panel_info(stage, p))
        .map(|p| p.shot_code)
        .collect()
}

// ---------------------------------------------------------------------------
// Panel fields shared by panel.create and panel.update

/// Panel fields. Omitted fields are left unchanged (or default on creation).
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct PanelFields {
    /// Optional display name (the card shows the shot number).
    #[serde(default)]
    pub name: Option<String>,
    /// Picture: an Artboard (vector drawing) or an Asset (image).
    #[serde(default)]
    pub image: Option<PrimPath>,
    /// Time on screen in frames of the board's frame rate (preferred over `duration`).
    #[serde(default)]
    pub frames: Option<u32>,
    /// Time on screen in flicks (705600000 per second); rounded to whole frames.
    #[serde(default)]
    pub duration: Option<Time>,
    /// Framing: none, els, ls, fs, ms, mcu, cu or ecu.
    #[serde(default)]
    pub shot_size: Option<ShotSize>,
    /// Dialogue / voice-over, e.g. "12f  VO: Every frame, on the GPU."
    #[serde(default)]
    pub dialogue: Option<String>,
    /// What happens, one beat per line with frame numbers, e.g. "0–12f title rises".
    #[serde(default)]
    pub action: Option<String>,
    /// Camera movement, e.g. "push in 100%→112%, 0–72f, soft" or "static".
    #[serde(default)]
    pub camera_move: Option<String>,
    /// Sound effects and music cues, e.g. "SFX: whoosh @12f".
    #[serde(default)]
    pub sound: Option<String>,
    /// Transition to the next panel, e.g. "dissolve 12f"; empty = cut.
    #[serde(default)]
    pub transition: Option<String>,
    /// Production notes; open questions start with "TBD:".
    #[serde(default)]
    pub notes: Option<String>,
}

impl PanelFields {
    fn apply(
        self,
        tx: &mut Transaction<'_>,
        panel: &PrimPath,
        rate: FrameRate,
    ) -> Result<(), CommandError> {
        let duration = match (self.frames, self.duration) {
            (Some(_), Some(_)) => return Err(fail("give either frames or duration, not both")),
            (Some(0), None) => return Err(fail("a panel is at least one frame long")),
            (Some(f), None) => Some(rate.frame_to_time(i64::from(f))),
            (None, Some(d)) if d.0 <= 0 => {
                return Err(fail("a panel is at least one frame long"));
            }
            (None, Some(d)) => Some(rate.frame_to_time(query::frames_of(d, rate))),
            (None, None) => None,
        };
        if let Some(image) = &self.image {
            let ok = tx
                .stage()
                .prim(image)
                .is_some_and(|v| v.is_a(&PrimType::ARTBOARD) || v.is_a(&PrimType::ASSET));
            if !ok {
                return Err(fail(format!("{image} is not an artboard or an asset")));
            }
        }
        let strings = [
            (attr::NAME, self.name),
            (attr::DIALOGUE, self.dialogue),
            (attr::ACTION, self.action),
            (attr::CAMERA_MOVE, self.camera_move),
            (attr::SOUND, self.sound),
            (attr::TRANSITION, self.transition),
            (attr::NOTES, self.notes),
        ];
        for (name, value) in strings {
            if let Some(v) = value {
                set(tx, panel, name, Value::String(v))?;
            }
        }
        if let Some(image) = self.image {
            set(tx, panel, attr::IMAGE, Value::PrimRef(image))?;
        }
        if let Some(d) = duration {
            set(tx, panel, attr::DURATION, Value::Time(d))?;
        }
        if let Some(s) = self.shot_size {
            set(
                tx,
                panel,
                attr::SHOT_SIZE,
                Value::Enum(s.token().to_owned()),
            )?;
        }
        Ok(())
    }
}

/// Create one panel at `index` of `board` (`None` = at the end).
fn create_panel(
    tx: &mut Transaction<'_>,
    board: &PrimPath,
    index: Option<usize>,
    shot_code: Option<String>,
    fields: PanelFields,
) -> Result<PanelOutput, CommandError> {
    let info = require_board(tx, board)?;
    let folder = query::panels_folder(board);
    ensure_folder(tx, &folder)?;
    let codes = board_codes(tx, board);
    let at = index.unwrap_or(codes.len()).min(codes.len());
    let code = match shot_code {
        Some(c) if c.trim().is_empty() => return Err(fail("shot code must not be empty")),
        Some(c) if codes.contains(&c) => {
            return Err(fail(format!("shot code {c} is already used in {board}")));
        }
        Some(c) => c,
        None => query::next_shot_code(&codes, at),
    };
    let name = bae_motion::query::unique_child_name(tx.stage(), &folder, &code.to_lowercase());
    let panel = child(&folder, &name)?;
    // Panels from weaker sublayers may interleave; place by the composed order.
    let siblings = query::board_panels(tx.stage(), board);
    create(tx, &panel, PrimType::PANEL, None)?;
    if at < siblings.len() {
        let mut order = siblings;
        order.insert(at, panel.clone());
        apply_order(tx, &folder, &order)?;
    }
    set(tx, &panel, attr::SHOT_CODE, Value::String(code.clone()))?;
    if fields.frames.is_none() && fields.duration.is_none() {
        // Store the default explicitly so it is whole frames of this board.
        let d = Time::from_seconds(2.0);
        set(
            tx,
            &panel,
            attr::DURATION,
            Value::Time(
                info.frame_rate
                    .frame_to_time(query::frames_of(d, info.frame_rate)),
            ),
        )?;
    }
    fields.apply(tx, &panel, info.frame_rate)?;
    Ok(PanelOutput {
        path: panel,
        shot_code: code,
    })
}

// ---------------------------------------------------------------------------

pub struct CreateBoard;

fn default_fps() -> f64 {
    24.0
}
fn default_width() -> u32 {
    1920
}
fn default_height() -> u32 {
    1080
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateBoardParams {
    /// Display name (default the next free "Storyboard N").
    #[serde(default)]
    pub name: Option<String>,
    /// Frame rate of the board and its animatic (default 24).
    #[serde(default = "default_fps")]
    pub fps: f64,
    /// Frame width in pixels (default 1920).
    #[serde(default = "default_width")]
    pub width: u32,
    /// Frame height in pixels (default 1080).
    #[serde(default = "default_height")]
    pub height: u32,
    /// Number of empty panels to start with (SH010, SH020 …; default 0).
    #[serde(default)]
    pub panels: u32,
}

impl Command for CreateBoard {
    type Params = CreateBoardParams;
    type Output = BoardOutput;
    const ID: &'static str = "board.create";
    const TITLE: &'static str = "New Storyboard";
    const DESCRIPTION: &'static str = "Create a storyboard (frame rate and frame size of its animatic) under /boards, optionally with a number of empty panels numbered SH010, SH020 …";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<BoardOutput, CommandError> {
        let rate = check_fps(p.fps)?;
        let width = check_dimension(p.width)?;
        let height = check_dimension(p.height)?;
        if p.panels > 1000 {
            return Err(fail("at most 1000 panels at once"));
        }
        let folder = path(BOARDS_FOLDER);
        ensure_folder(tx, &folder)?;
        let stage = tx.stage();
        let name = p
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| bae_motion::query::next_numbered_name(stage, &folder, "Storyboard"));
        let board = child(
            &folder,
            &bae_motion::query::unique_child_name(stage, &folder, &name),
        )?;
        create(tx, &board, PrimType::BOARD, None)?;
        let panels_folder = query::panels_folder(&board);
        create(tx, &panels_folder, PrimType::FOLDER, None)?;
        set(tx, &board, attr::NAME, Value::String(name))?;
        set(tx, &board, attr::FRAME_RATE, Value::FrameRate(rate))?;
        set(tx, &board, attr::WIDTH, Value::Int(width))?;
        set(tx, &board, attr::HEIGHT, Value::Int(height))?;
        let mut panels = Vec::new();
        for _ in 0..p.panels {
            panels.push(create_panel(tx, &board, None, None, PanelFields::default())?.path);
        }
        Ok(BoardOutput {
            path: board,
            panels_folder,
            panels,
        })
    }
}

// ---------------------------------------------------------------------------

pub struct UpdateBoard;

#[derive(Deserialize, JsonSchema)]
pub struct UpdateBoardParams {
    pub board: PrimPath,
    /// New display name.
    #[serde(default)]
    pub name: Option<String>,
    /// New frame rate. Panel durations keep their frame counts.
    #[serde(default)]
    pub fps: Option<f64>,
    /// New frame width in pixels.
    #[serde(default)]
    pub width: Option<u32>,
    /// New frame height in pixels.
    #[serde(default)]
    pub height: Option<u32>,
}

impl Command for UpdateBoard {
    type Params = UpdateBoardParams;
    type Output = ();
    const ID: &'static str = "board.update";
    const TITLE: &'static str = "Storyboard Settings";
    const DESCRIPTION: &'static str = "Rename a storyboard or change its frame rate or frame size. Changing the frame rate keeps every panel's length in frames.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let info = require_board(tx, &p.board)?;
        if let Some(name) = p.name {
            if name.trim().is_empty() {
                return Err(fail("storyboard name must not be empty"));
            }
            set(tx, &p.board, attr::NAME, Value::String(name))?;
        }
        if let Some(fps) = p.fps {
            let rate = check_fps(fps)?;
            if rate != info.frame_rate {
                let stage = tx.stage();
                let lengths: Vec<(PrimPath, i64)> = query::board_panels(stage, &p.board)
                    .iter()
                    .filter_map(|panel| query::panel_info(stage, panel))
                    .map(|panel| {
                        (
                            panel.path,
                            query::frames_of(panel.duration, info.frame_rate),
                        )
                    })
                    .collect();
                set(tx, &p.board, attr::FRAME_RATE, Value::FrameRate(rate))?;
                for (panel, frames) in lengths {
                    set(
                        tx,
                        &panel,
                        attr::DURATION,
                        Value::Time(rate.frame_to_time(frames)),
                    )?;
                }
            }
        }
        if let Some(w) = p.width {
            set(tx, &p.board, attr::WIDTH, Value::Int(check_dimension(w)?))?;
        }
        if let Some(h) = p.height {
            set(tx, &p.board, attr::HEIGHT, Value::Int(check_dimension(h)?))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct CreatePanel;

#[derive(Deserialize, JsonSchema)]
pub struct CreatePanelParams {
    /// The storyboard to add the panel to.
    pub board: PrimPath,
    /// Position in the story, 0 = first (default last).
    #[serde(default)]
    pub index: Option<usize>,
    /// Shot number, e.g. "SH015" (default: SH010, SH020 … at the end, the midpoint
    /// when inserting between two shots). Must be unique in the storyboard.
    #[serde(default)]
    pub shot_code: Option<String>,
    #[serde(flatten)]
    pub fields: PanelFields,
}

impl Command for CreatePanel {
    type Params = CreatePanelParams;
    type Output = PanelOutput;
    const ID: &'static str = "panel.create";
    const TITLE: &'static str = "New Panel";
    const DESCRIPTION: &'static str = "Add a panel to a storyboard: shot number (auto: SH010, SH020 …, SH015 when inserted between), duration in frames (default 2 seconds), shot size, dialogue, action, camera movement, sound, transition, notes and picture.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PanelOutput, CommandError> {
        create_panel(tx, &p.board, p.index, p.shot_code, p.fields)
    }
}

// ---------------------------------------------------------------------------

pub struct UpdatePanel;

#[derive(Deserialize, JsonSchema)]
pub struct UpdatePanelParams {
    pub panel: PrimPath,
    /// New shot number. Shot numbers that were shared should not change
    /// (comps, clips and notes refer to them); must stay unique in the storyboard.
    #[serde(default)]
    pub shot_code: Option<String>,
    #[serde(flatten)]
    pub fields: PanelFields,
}

impl Command for UpdatePanel {
    type Params = UpdatePanelParams;
    type Output = ();
    const ID: &'static str = "panel.update";
    const TITLE: &'static str = "Panel Properties";
    const DESCRIPTION: &'static str = "Change a panel's fields (shot number, duration, shot size, dialogue, action, camera movement, sound, transition, notes, picture). Omitted fields are left unchanged.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let board = require_panel(tx, &p.panel)?;
        let info = require_board(tx, &board)?;
        if let Some(code) = p.shot_code {
            if code.trim().is_empty() {
                return Err(fail("shot code must not be empty"));
            }
            let current = query::panel_info(tx.stage(), &p.panel).map(|i| i.shot_code);
            if current.as_deref() != Some(code.as_str()) {
                if board_codes(tx, &board).contains(&code) {
                    return Err(fail(format!("shot code {code} is already used in {board}")));
                }
                set(tx, &p.panel, attr::SHOT_CODE, Value::String(code))?;
            }
        }
        p.fields.apply(tx, &p.panel, info.frame_rate)
    }
}

// ---------------------------------------------------------------------------

pub struct MovePanels;

#[derive(Deserialize, JsonSchema)]
pub struct MovePanelsParams {
    /// Panels to move; they keep their relative story order.
    pub panels: Vec<PrimPath>,
    /// Where the first moved panel ends up, 0 = first. Larger values move to the end.
    pub index: usize,
    /// Storyboard to move them to (default: the storyboard they are in). Moved
    /// panels keep their shot numbers; a clash with the target is an error.
    #[serde(default)]
    pub board: Option<PrimPath>,
}

impl Command for MovePanels {
    type Params = MovePanelsParams;
    type Output = PanelsOutput;
    const ID: &'static str = "panel.move";
    const TITLE: &'static str = "Move Panels";
    const DESCRIPTION: &'static str =
        "Reorder panels in their storyboard, or move them into another storyboard at a position.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PanelsOutput, CommandError> {
        if p.panels.is_empty() {
            return Ok(PanelsOutput { paths: Vec::new() });
        }
        let mut source: Option<PrimPath> = None;
        for panel in &p.panels {
            let board = require_panel(tx, panel)?;
            match &source {
                Some(s) if s != &board => {
                    return Err(fail("the panels must be in the same storyboard"));
                }
                _ => source = Some(board),
            }
        }
        let source = source.expect("non-empty");
        let target = p.board.unwrap_or_else(|| source.clone());
        require_board(tx, &target)?;
        let stage = tx.stage();
        // Story order of the moved panels.
        let moving: Vec<PrimPath> = query::board_panels(stage, &source)
            .into_iter()
            .filter(|x| p.panels.contains(x))
            .collect();
        let mut renamed: Vec<(PrimPath, PrimPath)> = Vec::new();
        if target != source {
            let codes = board_codes(tx, &target);
            for panel in &moving {
                let code = query::panel_info(tx.stage(), panel).map(|i| i.shot_code);
                if let Some(code) = code.filter(|c| !c.is_empty() && codes.contains(c)) {
                    return Err(fail(format!(
                        "shot code {code} is already used in {target}"
                    )));
                }
            }
            let folder = query::panels_folder(&target);
            ensure_folder(tx, &folder)?;
            for panel in &moving {
                let name = bae_motion::query::unique_child_name(tx.stage(), &folder, panel.name());
                let to = child(&folder, &name)?;
                move_to(tx, panel, &to)?;
                renamed.push((panel.clone(), to));
            }
        } else {
            renamed = moving.iter().map(|x| (x.clone(), x.clone())).collect();
        }
        let moved: Vec<PrimPath> = renamed.iter().map(|(_, to)| to.clone()).collect();
        let current = query::board_panels(tx.stage(), &target);
        let mut order: Vec<PrimPath> = current
            .iter()
            .filter(|x| !moved.contains(x))
            .cloned()
            .collect();
        let at = p.index.min(order.len());
        order.splice(at..at, moved);
        if order != current {
            apply_order(tx, &query::panels_folder(&target), &order)?;
        }
        let paths = p
            .panels
            .iter()
            .map(|x| {
                renamed
                    .iter()
                    .find(|(from, _)| from == x)
                    .map_or_else(|| x.clone(), |(_, to)| to.clone())
            })
            .collect();
        Ok(PanelsOutput { paths })
    }
}

// ---------------------------------------------------------------------------

pub struct DeletePanels;

#[derive(Deserialize, JsonSchema)]
pub struct DeletePanelsParams {
    /// Panels to delete. Their shot numbers are not reused or renumbered.
    pub panels: Vec<PrimPath>,
}

impl Command for DeletePanels {
    type Params = DeletePanelsParams;
    type Output = ();
    const ID: &'static str = "panel.delete";
    const TITLE: &'static str = "Delete Panels";
    const DESCRIPTION: &'static str = "Delete storyboard panels. Their shots and motion comps are kept; the other panels keep their shot numbers.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for panel in &p.panels {
            require_panel(tx, panel)?;
        }
        for panel in &p.panels {
            if tx.stage().has_prim(panel) {
                delete(tx, panel)?;
            }
        }
        Ok(())
    }
}
