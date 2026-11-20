//! From storyboard to motion and edit: `board.make_animatic` (a `Sequence` with one
//! clip per panel) and `panel.promote_to_comp` (a motion comp linked to the panel
//! through a `Shot`).
//!
//! **Shot links.** `Panel.shot` → `Shot`; `Shot.comp` → the motion comp; animatic
//! clips carry the same `shot`. A clip's `source` is the panel itself, so the edit
//! workspace always shows the panel's latest picture: [`query::panel_picture`]
//! resolves it to the shot's comp once the panel was promoted, else to the panel's
//! artboard or image asset.

use bae_model::{Command, CommandError, LayerKind, PrimPath, PrimType, Time, Transaction, Value};
use bae_motion::comp::CreateCompParams;
use bae_motion::layer::CreateLayerParams;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, SEQUENCES_FOLDER, SHOTS_FOLDER, attr};
use crate::util::{child, copy_tree, create, ensure_folder, fail, path, set};

/// Name of the animatic's video track prim.
pub const ANIMATIC_TRACK: &str = "V1";

/// Output of `board.make_animatic`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnimaticOutput {
    /// The new sequence.
    pub path: PrimPath,
    /// Its video track.
    pub track: PrimPath,
    /// One clip per panel, in story order.
    pub clips: Vec<PrimPath>,
    /// Total length in frames of the board's frame rate.
    pub frames: i64,
    /// Total length in flicks.
    pub duration: Time,
}

pub struct MakeAnimatic;

#[derive(Deserialize, JsonSchema)]
pub struct MakeAnimaticParams {
    /// The storyboard.
    pub board: PrimPath,
    /// Sequence name (default "<storyboard name> Animatic").
    #[serde(default)]
    pub name: Option<String>,
    /// Folder for the sequence (default /sequences, created if missing).
    #[serde(default)]
    pub parent: Option<PrimPath>,
}

impl Command for MakeAnimatic {
    type Params = MakeAnimaticParams;
    type Output = AnimaticOutput;
    const ID: &'static str = "board.make_animatic";
    const TITLE: &'static str = "Make Animatic";
    const DESCRIPTION: &'static str = "Generate an animatic from a storyboard: a Sequence (board frame rate and size) with one video track and one clip per panel, back to back in story order, each as long as its panel. Clips reference their panel and its Shot, so they show the latest drawing or motion comp. Open it in the edit workspace to keep cutting.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<AnimaticOutput, CommandError> {
        let board = query::board_info(tx.stage(), &p.board)
            .ok_or_else(|| fail(format!("{} is not a storyboard", p.board)))?;
        let panels: Vec<_> = {
            let stage = tx.stage();
            query::board_panels(stage, &p.board)
                .iter()
                .filter_map(|x| query::panel_info(stage, x))
                .collect()
        };
        let parent = match p.parent {
            Some(parent) => {
                tx.stage().require_prim(&parent)?;
                parent
            }
            None => {
                let folder = path(SEQUENCES_FOLDER);
                ensure_folder(tx, &folder)?;
                folder
            }
        };
        let name = p
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("{} Animatic", board.name));
        let seq = child(
            &parent,
            &bae_motion::query::unique_child_name(tx.stage(), &parent, &name),
        )?;
        create(tx, &seq, PrimType::SEQUENCE, None)?;
        set(tx, &seq, attr::NAME, Value::String(name))?;
        set(
            tx,
            &seq,
            attr::FRAME_RATE,
            Value::FrameRate(board.frame_rate),
        )?;
        set(tx, &seq, attr::WIDTH, Value::Int(i64::from(board.width)))?;
        set(tx, &seq, attr::HEIGHT, Value::Int(i64::from(board.height)))?;
        set(
            tx,
            &seq,
            attr::SOURCE_BOARD,
            Value::PrimRef(p.board.clone()),
        )?;
        let track = child(&seq, ANIMATIC_TRACK)?;
        create(tx, &track, PrimType::TRACK, None)?;
        set(tx, &track, attr::NAME, Value::String("Video 1".to_owned()))?;
        set(tx, &track, "track_kind", Value::Enum("video".to_owned()))?;
        let rate = board.frame_rate;
        let mut frame = 0_i64;
        let mut clips = Vec::new();
        for panel in panels {
            let frames = query::frames_of(panel.duration, rate);
            let clip = child(
                &track,
                &bae_motion::query::unique_child_name(tx.stage(), &track, panel.path.name()),
            )?;
            create(tx, &clip, PrimType::CLIP, None)?;
            let label = if panel.shot_code.is_empty() {
                panel.path.name().to_owned()
            } else {
                panel.shot_code.clone()
            };
            set(tx, &clip, attr::NAME, Value::String(label))?;
            set(tx, &clip, "source", Value::PrimRef(panel.path.clone()))?;
            set(tx, &clip, "start", Value::Time(rate.frame_to_time(frame)))?;
            set(tx, &clip, "source_in", Value::Time(Time::ZERO))?;
            set(
                tx,
                &clip,
                "duration",
                Value::Time(rate.frame_to_time(frames)),
            )?;
            if let Some(shot) = panel.shot {
                set(tx, &clip, attr::SHOT, Value::PrimRef(shot))?;
            }
            frame += frames;
            clips.push(clip);
        }
        Ok(AnimaticOutput {
            path: seq,
            track,
            clips,
            frames: frame,
            duration: rate.frame_to_time(frame),
        })
    }
}

// ---------------------------------------------------------------------------

/// Output of `panel.promote_to_comp`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromoteOutput {
    /// The panel's shot.
    pub shot: PrimPath,
    /// The motion comp.
    pub comp: PrimPath,
    /// Layers made from the panel's picture (editable copies of the artboard's
    /// artwork, or one footage layer for an image asset).
    pub layers: Vec<PrimPath>,
    /// `false` when the panel already had a comp, which is returned unchanged.
    pub created: bool,
}

pub struct PromotePanel;

#[derive(Deserialize, JsonSchema)]
pub struct PromotePanelParams {
    /// The panel to promote.
    pub panel: PrimPath,
    /// Comp name (default the shot number, e.g. "SH010"; the guide's convention for
    /// more comps of one shot is "SH010_<purpose>").
    #[serde(default)]
    pub name: Option<String>,
    /// Folder for the comp (default /comps).
    #[serde(default)]
    pub folder: Option<PrimPath>,
}

impl Command for PromotePanel {
    type Params = PromotePanelParams;
    type Output = PromoteOutput;
    const ID: &'static str = "panel.promote_to_comp";
    const TITLE: &'static str = "Promote Panel to Composition";
    const DESCRIPTION: &'static str = "Turn a storyboard panel into a motion comp: creates (or reuses) the panel's Shot, a comp of the board's size and frame rate as long as the panel, and copies the panel's artboard artwork into it as editable shape and text layers (an image asset becomes a footage layer). The Shot links panel, comp and animatic clips. If the panel already has a comp, it is returned unchanged.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<PromoteOutput, CommandError> {
        let panel = query::panel_info(tx.stage(), &p.panel)
            .ok_or_else(|| fail(format!("{} is not a storyboard panel", p.panel)))?;
        let board_path = query::owning_board(tx.stage(), &p.panel)
            .ok_or_else(|| fail(format!("{} is not in a storyboard", p.panel)))?;
        let board = query::board_info(tx.stage(), &board_path)
            .ok_or_else(|| fail(format!("{board_path} is not a storyboard")))?;
        let code = if panel.shot_code.is_empty() {
            p.panel.name().to_uppercase()
        } else {
            panel.shot_code.clone()
        };

        // The shot: reuse the linked one, else make one under /shots.
        let existing = panel
            .shot
            .clone()
            .filter(|s| tx.stage().prim(s).is_some_and(|v| v.is_a(&PrimType::SHOT)));
        if let Some(shot) = &existing
            && let Some(comp) = query::shot_comp(tx.stage(), shot)
        {
            return Ok(PromoteOutput {
                shot: shot.clone(),
                comp,
                layers: Vec::new(),
                created: false,
            });
        }
        let shot = match existing {
            Some(shot) => shot,
            None => {
                let folder = path(SHOTS_FOLDER);
                ensure_folder(tx, &folder)?;
                let shot = child(
                    &folder,
                    &bae_motion::query::unique_child_name(tx.stage(), &folder, &code),
                )?;
                create(tx, &shot, PrimType::SHOT, None)?;
                set(tx, &shot, attr::NAME, Value::String(code.clone()))?;
                set(tx, &shot, attr::CODE, Value::String(code.clone()))?;
                set(tx, &p.panel, attr::SHOT, Value::PrimRef(shot.clone()))?;
                shot
            }
        };

        // The comp, through the motion command so it gets the usual defaults.
        let rate = board.frame_rate;
        let duration = rate.frame_to_time(query::frames_of(panel.duration, rate));
        let comp = bae_motion::CreateComp.run(
            tx,
            CreateCompParams {
                name: Some(p.name.filter(|n| !n.trim().is_empty()).unwrap_or(code)),
                parent: p.folder,
                width: board.width,
                height: board.height,
                fps: rate.as_f64(),
                duration: Some(duration),
                bg_color: None,
                pixel_aspect: None,
            },
        )?;
        // Keep the board's exact rational rate (from_fps rounds to NTSC rates).
        set(tx, &comp.path, "frame_rate", Value::FrameRate(rate))?;
        set(tx, &shot, attr::COMP, Value::PrimRef(comp.path.clone()))?;

        // The picture: artboard artwork is copied 1:1 (artboard coordinates become comp
        // coordinates, top-left aligned); an image asset becomes a footage layer.
        let mut layers = Vec::new();
        if let Some(image) = &panel.image {
            if query::artboard_info(tx.stage(), image).is_some() {
                for layer in query::artwork_layers(tx.stage(), image) {
                    let name = bae_motion::query::unique_child_name(
                        tx.stage(),
                        &comp.layers,
                        layer.name(),
                    );
                    let to = child(&comp.layers, &name)?;
                    copy_tree(tx, &layer, &to, None)?;
                    layers.push(to);
                }
            } else if tx
                .stage()
                .prim(image)
                .is_some_and(|v| v.is_a(&PrimType::ASSET))
            {
                let out = bae_motion::CreateLayer.run(
                    tx,
                    CreateLayerParams {
                        comp: comp.path.clone(),
                        kind: LayerKind::Footage,
                        name: None,
                        source: Some(image.clone()),
                        index: None,
                        color: None,
                        size: None,
                        text: None,
                        font_size: None,
                        shape: None,
                        position: None,
                        start: None,
                    },
                )?;
                layers.push(out.path);
            }
        }
        Ok(PromoteOutput {
            shot,
            comp: comp.path,
            layers,
            created: true,
        })
    }
}
