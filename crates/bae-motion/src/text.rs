//! Text created and edited with the Type tool (AE-TXT, Composition panel):
//! `text.create` makes a point or paragraph text layer with its text in one
//! step (what committing new text typed in the viewer does), `text.set_text`
//! replaces the characters of a text layer and keeps its style (committing an
//! edit). Character and Paragraph panel styling is separate (E's commands).

use bae_model::{
    Command, CommandError, LayerKind, PrimPath, TextDocument, Time, Transaction, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::keys::{SetAt, SetAtParams};
use crate::layer::{CreateLayer, CreateLayerParams, LayerOutput};
use crate::query;
use crate::util::{ensure_unlocked, fail, set};

/// The source text attribute of text layers.
pub const SOURCE: &str = "text:source";

/// Default font and size of new text until the Character panel provides the
/// current settings (AE uses the panel's last settings; first-run values 待核).
pub const DEFAULT_FONT: &str = "Arial";
pub const DEFAULT_SIZE: f64 = 72.0;

pub struct CreateText;

#[derive(Deserialize, JsonSchema)]
pub struct CreateTextParams {
    pub comp: PrimPath,
    /// The characters (line breaks as `\n`); empty makes an empty text layer
    /// (AE keeps those, named "<empty text layer>").
    pub text: String,
    /// Point text: where the first line's baseline starts (comp pixels), the
    /// layer's Position (its anchor stays at 0, 0). Paragraph text: the box's
    /// top-left corner (the layer's Position becomes the box's center, as AE
    /// places it).
    pub position: [f64; 2],
    /// Paragraph text: the box size in pixels; omitted = point text.
    #[serde(default)]
    pub box_size: Option<[f64; 2]>,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_size: Option<f64>,
    /// Straight RGBA 0–1 (default white).
    #[serde(default)]
    pub fill_color: Option<[f64; 4]>,
    /// left, center, right … (default left).
    #[serde(default)]
    pub justification: Option<String>,
    /// Layer name (default: the text, as AE names text layers).
    #[serde(default)]
    pub name: Option<String>,
    /// Position in the layer stack, 0 = top (default 0).
    #[serde(default)]
    pub index: Option<usize>,
}

impl Command for CreateText {
    type Params = CreateTextParams;
    type Output = LayerOutput;
    const ID: &'static str = "text.create";
    const TITLE: &'static str = "New Text";
    const DESCRIPTION: &'static str = "Create a point or paragraph text layer with its text, like typing with the Type tool in the Composition panel: point text starts its first baseline at `position`; paragraph text fills a box whose top-left corner is `position`.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<LayerOutput, CommandError> {
        let size = p.font_size.unwrap_or(DEFAULT_SIZE);
        if size.is_nan() || size <= 0.0 {
            return Err(fail("font_size must be positive"));
        }
        let out = CreateLayer.run(
            tx,
            CreateLayerParams {
                comp: p.comp,
                kind: LayerKind::Text,
                name: p.name.or_else(|| {
                    p.text
                        .trim()
                        .is_empty()
                        .then(|| crate::names::localized("<empty text layer>").to_owned())
                }),
                source: None,
                index: p.index,
                color: None,
                size: None,
                text: Some(p.text.clone()),
                font_size: Some(size),
                shape: None,
                position: Some(p.position),
                start: None,
                pixel_aspect: None,
                camera: None,
                light: None,
                label: None,
            },
        )?;
        let mut doc =
            TextDocument::new(p.text, p.font.unwrap_or_else(|| DEFAULT_FONT.into()), size);
        if let Some(c) = p.fill_color {
            doc.fill_color = c;
        }
        if let Some(j) = p.justification {
            doc.justification = j;
        }
        // Paragraph text (AE 26, R1): the layer sits at the box's center and the
        // box is offset by half its size.
        let mut position = p.position;
        if let Some(b) = p.box_size {
            let (w, h) = (b[0].abs(), b[1].abs());
            doc.box_size = Some([w, h]);
            doc.box_position = Some([-w / 2.0, -h / 2.0]);
            position = [p.position[0] + w / 2.0, p.position[1] + h / 2.0];
        }
        set(tx, &out.path, SOURCE, Value::TextDocument(Box::new(doc)))?;
        set(
            tx,
            &out.path,
            "transform:anchor_point",
            Value::Vec3([0.0, 0.0, 0.0]),
        )?;
        set(
            tx,
            &out.path,
            "transform:position",
            Value::Vec3([position[0], position[1], 0.0]),
        )?;
        Ok(out)
    }
}

pub struct SetText;

#[derive(Deserialize, JsonSchema)]
pub struct SetTextParams {
    /// The text layer.
    pub layer: PrimPath,
    /// The new characters (line breaks as `\n`).
    pub text: String,
    /// Keyframe time in flicks (layer time) when Source Text is animated.
    #[serde(default)]
    pub time: Option<Time>,
}

/// Output of `text.set_text`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetTextOutput {
    pub changed: bool,
}

impl Command for SetText {
    type Params = SetTextParams;
    type Output = SetTextOutput;
    const ID: &'static str = "text.set_text";
    const TITLE: &'static str = "Edit Text";
    const DESCRIPTION: &'static str = "Replace the characters of a text layer and keep its style (committing an edit made with the Type tool): a Source Text keyframe at `time` when it is animated.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<SetTextOutput, CommandError> {
        if query::layer_kind(tx.stage(), &p.layer).as_deref() != Some("text") {
            return Err(fail(format!("{} is not a text layer", p.layer)));
        }
        ensure_unlocked(tx.stage(), [&p.layer])?;
        let time = p.time.unwrap_or(Time::ZERO);
        let spec = query::attr_spec(tx.stage(), &p.layer, SOURCE).cloned();
        let current = spec.as_ref().and_then(|s| {
            if s.is_animated() {
                // The keyframe at or before `time` (Source Text holds).
                s.keyframes
                    .range(..=time)
                    .next_back()
                    .or_else(|| s.keyframes.iter().next())
                    .map(|(_, k)| k.value.clone())
            } else {
                s.value.clone()
            }
        });
        let mut doc = match current {
            Some(Value::TextDocument(d)) => *d,
            _ => TextDocument::new("", DEFAULT_FONT, DEFAULT_SIZE),
        };
        if doc.text == p.text {
            return Ok(SetTextOutput { changed: false });
        }
        doc.text = p.text;
        SetAt.run(
            tx,
            SetAtParams {
                path: p.layer,
                attr: SOURCE.into(),
                time,
                value: Value::TextDocument(Box::new(doc)),
            },
        )?;
        Ok(SetTextOutput { changed: true })
    }
}

/// Character / Paragraph panel changes to the style of text layers; every field
/// left out stays as it is.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TextStyleChange {
    /// PostScript font name (`ArialMT`).
    pub font: Option<String>,
    pub font_size: Option<f64>,
    /// Leading in pixels.
    pub leading: Option<f64>,
    /// `true`: Auto leading (120 % of the size).
    pub auto_leading: Option<bool>,
    /// Tracking in 1/1000 em.
    pub tracking: Option<f64>,
    pub baseline_shift: Option<f64>,
    pub fill_color: Option<[f64; 4]>,
    pub stroke_color: Option<[f64; 4]>,
    /// `false`: no stroke color (AE: the swatch with the red slash).
    pub stroke: Option<bool>,
    pub stroke_width: Option<f64>,
    pub stroke_over_fill: Option<bool>,
    pub faux_bold: Option<bool>,
    pub faux_italic: Option<bool>,
    pub all_caps: Option<bool>,
    pub small_caps: Option<bool>,
    /// Paragraph alignment (`left`, `center`, `right`, `justify_last_left`, …).
    pub justification: Option<String>,
}

impl TextStyleChange {
    /// `doc` with the change applied.
    pub fn apply(&self, doc: &mut TextDocument) {
        if let Some(f) = &self.font {
            doc.font = f.clone();
        }
        if let Some(v) = self.font_size {
            doc.font_size = v.clamp(0.1, 1296.0);
        }
        if let Some(v) = self.leading {
            doc.leading = Some(v.max(0.0));
        }
        if self.auto_leading == Some(true) {
            doc.leading = None;
        }
        if let Some(v) = self.tracking {
            doc.tracking = v;
        }
        if let Some(v) = self.baseline_shift {
            doc.baseline_shift = v;
        }
        if let Some(c) = self.fill_color {
            doc.fill_color = c;
        }
        if let Some(c) = self.stroke_color {
            doc.stroke_color = Some(c);
        }
        if self.stroke == Some(false) {
            doc.stroke_color = None;
        }
        if let Some(v) = self.stroke_width {
            doc.stroke_width = v.max(0.0);
        }
        if let Some(v) = self.stroke_over_fill {
            doc.stroke_over_fill = v;
        }
        if let Some(v) = self.faux_bold {
            doc.faux_bold = v;
        }
        if let Some(v) = self.faux_italic {
            doc.faux_italic = v;
        }
        if let Some(v) = self.all_caps {
            doc.all_caps = v;
        }
        if let Some(v) = self.small_caps {
            doc.small_caps = v;
        }
        if let Some(j) = &self.justification {
            doc.justification = j.clone();
        }
    }
}

pub struct SetTextStyle;

#[derive(Deserialize, JsonSchema)]
pub struct SetTextStyleParams {
    /// Text layers (others are ignored).
    pub layers: Vec<PrimPath>,
    /// Comp time of the change: a Source Text keyframe there when it is animated.
    #[serde(default)]
    pub time: Option<Time>,
    pub style: TextStyleChange,
}

impl Command for SetTextStyle {
    type Params = SetTextStyleParams;
    type Output = ();
    const ID: &'static str = "text.set_style";
    const TITLE: &'static str = "Change Text Style";
    const DESCRIPTION: &'static str = "Character and Paragraph panel settings (font, size, leading, tracking, colors, stroke, faux styles, caps, alignment) on whole text layers, one undo step; a Source Text keyframe at `time` when it is animated.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let layers: Vec<PrimPath> = p
            .layers
            .into_iter()
            .filter(|l| query::layer_kind(tx.stage(), l).as_deref() == Some("text"))
            .collect();
        ensure_unlocked(tx.stage(), layers.iter())?;
        for layer in layers {
            let time = query::to_key_time(tx.stage(), &layer, p.time.unwrap_or(Time::ZERO));
            let spec = query::attr_spec(tx.stage(), &layer, SOURCE).cloned();
            let current = spec.as_ref().and_then(|s| {
                if s.is_animated() {
                    s.keyframes
                        .range(..=time)
                        .next_back()
                        .or_else(|| s.keyframes.iter().next())
                        .map(|(_, k)| k.value.clone())
                } else {
                    s.value.clone()
                }
            });
            let mut doc = match current {
                Some(Value::TextDocument(d)) => *d,
                _ => TextDocument::new("", DEFAULT_FONT, DEFAULT_SIZE),
            };
            let before = doc.clone();
            p.style.apply(&mut doc);
            if doc == before {
                continue;
            }
            SetAt.run(
                tx,
                SetAtParams {
                    path: layer,
                    attr: SOURCE.into(),
                    time,
                    value: Value::TextDocument(Box::new(doc)),
                },
            )?;
        }
        Ok(())
    }
}
