//! File ▸ Interpret Footage ▸ Main (AE: Cmd/Ctrl+Alt+G): how footage items are
//! read — alpha (ignore / straight / premultiplied with a matte color), frame
//! rate (the file's or an assumed one), pixel aspect ratio and loop count.
//! `footage.interpret` changes the selected items in one undo step.

use bae_model::{Command, CommandError, FrameRate, PrimPath, PrimType, Transaction, Value};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::util::{ensure_unlocked, fail, set};

/// The file's own frame rate, kept while an assumed rate is in use.
pub const FILE_FRAME_RATE: &str = "file:frame_rate";

pub struct InterpretFootage;

#[derive(Default, Deserialize, JsonSchema)]
pub struct InterpretFootageParams {
    /// Footage items (Asset prims of kind footage or sequence).
    pub items: Vec<PrimPath>,
    /// `ignore`, `straight` or `premultiplied`.
    #[serde(default)]
    pub alpha_mode: Option<String>,
    /// Matte color of premultiplied alpha, straight RGBA 0–1.
    #[serde(default)]
    pub premultiply_color: Option<[f64; 4]>,
    /// Assume this frame rate (fps); see `use_file_frame_rate`.
    #[serde(default)]
    pub frame_rate: Option<f64>,
    /// Use the frame rate from the file again (drops an assumed rate).
    #[serde(default)]
    pub use_file_frame_rate: bool,
    #[serde(default)]
    pub pixel_aspect: Option<f64>,
    /// Times the footage plays (1 or more).
    #[serde(default)]
    pub loop_count: Option<i64>,
}

impl Command for InterpretFootage {
    type Params = InterpretFootageParams;
    type Output = ();
    const ID: &'static str = "footage.interpret";
    const TITLE: &'static str = "Interpret Footage";
    const DESCRIPTION: &'static str = "Change how footage items are interpreted (AE File > Interpret Footage > Main): alpha (ignore, straight, premultiplied with a matte color), frame rate (the file's or an assumed one), pixel aspect ratio, loop count. One undo step.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        if p.items.is_empty() {
            return Err(fail("no items given"));
        }
        if let Some(a) = &p.alpha_mode
            && !matches!(a.as_str(), "ignore" | "straight" | "premultiplied")
        {
            return Err(fail("alpha_mode must be ignore, straight or premultiplied"));
        }
        if p.frame_rate.is_some_and(|f| !(f.is_finite() && f > 0.0)) {
            return Err(fail("frame_rate must be positive"));
        }
        if p.pixel_aspect.is_some_and(|a| !(a.is_finite() && a > 0.0)) {
            return Err(fail("pixel_aspect must be positive"));
        }
        if p.loop_count.is_some_and(|n| n < 1) {
            return Err(fail("loop_count must be at least 1"));
        }
        ensure_unlocked(tx.stage(), &p.items)?;
        for item in &p.items {
            let view = tx
                .stage()
                .prim(item)
                .filter(|v| v.is_a(&PrimType::ASSET))
                .ok_or_else(|| fail(format!("{item} is not a footage item")))?;
            let kind = view
                .value("kind")
                .and_then(Value::as_str)
                .unwrap_or("footage");
            if !matches!(kind, "footage" | "sequence") {
                return Err(fail(format!("{item} is not footage")));
            }
            let current_rate = view.value("frame_rate").cloned();
            let file_rate = view.value(FILE_FRAME_RATE).cloned();
            if let Some(a) = &p.alpha_mode {
                set(tx, item, "alpha_mode", Value::Enum(a.clone()))?;
            }
            if let Some(c) = p.premultiply_color {
                set(tx, item, "premultiply_color", Value::Color(c))?;
            }
            if p.use_file_frame_rate {
                if let Some(rate) = file_rate {
                    set(tx, item, "frame_rate", rate)?;
                }
            } else if let Some(fps) = p.frame_rate {
                // Keep the file's rate the first time another is assumed.
                if file_rate.is_none()
                    && let Some(rate) = current_rate
                {
                    set(tx, item, FILE_FRAME_RATE, rate)?;
                }
                set(
                    tx,
                    item,
                    "frame_rate",
                    Value::FrameRate(FrameRate::from_fps(fps)),
                )?;
            }
            if let Some(a) = p.pixel_aspect {
                set(tx, item, "pixel_aspect", Value::Float(a))?;
            }
            if let Some(n) = p.loop_count {
                set(tx, item, "loop", Value::Int(n))?;
            }
        }
        Ok(())
    }
}
