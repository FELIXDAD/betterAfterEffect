//! Custom UI: events, effect-controls custom areas, viewer overlays, Drawbot.
//!
//! Effects with custom UI receive `PF_Cmd_EVENT` with an event record that
//! names the window (effect controls, composition, layer), the area (param
//! title or control), the event type and its payload, plus callbacks to map
//! coordinates between layer, comp, source and screen space; they draw
//! through the Drawbot suites (a small 2D vector API: paths, pens, brushes,
//! images, text, transforms, clipping) on a host-supplied drawing surface
//! (SDK guide, "Effect UI & Events", "› Custom UI & Drawbot").
//!
//! BAE plan (requests R19-05, notes for 11/12): the host never hands a
//! native window to a plug-in. Drawbot calls are recorded into a
//! backend-neutral [`DisplayList`] that the UI replays with egui/Vello; the
//! same list serializes across the out-of-process boundary. Input events are
//! delivered as [`UiEvent`]s in the plug-in's expected coordinate space.

use crate::geometry::LRect;

/// Window an event belongs to (`PF_Window_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiWindow {
    /// Effect controls panel.
    EffectControls,
    /// Composition viewer.
    Comp,
    /// Layer viewer.
    Layer,
}

/// Area inside the effect-controls panel (`PF_EA_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectArea {
    /// Not in the effect controls panel.
    None,
    /// The param's title (topic) row.
    ParamTitle,
    /// The param's control area.
    Control,
}

/// Event types (`PF_Event_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiEventKind {
    /// A drawing context was created (window opened).
    NewContext,
    /// The context became active.
    Activate,
    /// Mouse down.
    DoClick,
    /// Mouse drag after a click the plug-in asked to track.
    Drag,
    /// Redraw request.
    Draw,
    /// Context deactivated.
    Deactivate,
    /// Context closed.
    CloseContext,
    /// Idle tick.
    Idle,
    /// Cursor may change.
    AdjustCursor,
    /// Key pressed.
    KeyDown,
    /// Mouse left the area.
    MouseExited,
}

/// Keyboard/mouse modifier state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Command (macOS) / Control (Windows).
    pub command: bool,
    /// Option / Alt.
    pub option: bool,
    /// Control (macOS).
    pub control: bool,
    /// Caps lock.
    pub caps_lock: bool,
}

/// An event as delivered to the plug-in (payload in *frame/screen* pixels of
/// the viewer; the host also provides the layer↔comp↔screen transforms).
#[derive(Clone, Debug, PartialEq)]
pub struct UiEvent {
    /// Window.
    pub window: UiWindow,
    /// Effect-controls area.
    pub area: EffectArea,
    /// Param the area belongs to (effect controls only).
    pub param: Option<crate::params::ParamIndex>,
    /// Kind.
    pub kind: UiEventKind,
    /// Pointer position, viewer pixels.
    pub point: (f32, f32),
    /// Modifiers.
    pub modifiers: Modifiers,
    /// Click count for `DoClick`.
    pub clicks: u8,
    /// Key (`KeyDown`): platform-neutral key code or character.
    pub key: Option<KeyInput>,
    /// Rect to redraw (`Draw`).
    pub update_rect: Option<LRect>,
}

/// Key payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyInput {
    /// Printable character.
    Char(char),
    /// Non-character key, by name (e.g. "Left", "Delete").
    Named(String),
}

/// What the plug-in reports back after an event (`PF_EO_*` and friends).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventOutcome {
    /// The plug-in consumed the event (`HANDLED_EVENT`).
    pub handled: bool,
    /// Keep sending drag events for this click.
    pub continue_drag: bool,
    /// Redraw now (`UPDATE_NOW`).
    pub update_now: bool,
    /// Always redraw on every event (`ALWAYS_UPDATE`).
    pub always_update: bool,
    /// Never redraw (`NEVER_UPDATE`).
    pub never_update: bool,
    /// Cursor the plug-in asked for (`AdjustCursor`).
    pub cursor: Option<Cursor>,
}

/// Cursor requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    /// Host default.
    Default,
    /// Hide.
    None,
    /// Crosshair.
    Crosshair,
    /// Hand.
    Hand,
    /// Move.
    Move,
    /// Plug-in drew its own cursor (`PF_Cursor_CUSTOM`).
    Custom,
}

/// RGBA colour with float channels (Drawbot colours are float RGBA).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub [f32; 4]);

/// Path segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathEl {
    /// Move to.
    MoveTo(f32, f32),
    /// Line to.
    LineTo(f32, f32),
    /// Cubic bezier (c1, c2, end).
    CubicTo(f32, f32, f32, f32, f32, f32),
    /// Arc around a centre (cx, cy, radius, start angle, sweep), degrees.
    Arc(f32, f32, f32, f32, f32),
    /// Close the sub-path.
    Close,
}

/// One recorded drawing command. Coordinates are in the surface's current
/// transform, exactly as the plug-in issued them.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCmd {
    /// Push a 2D affine transform (a, b, c, d, tx, ty).
    PushTransform([f32; 6]),
    /// Pop transform.
    PopTransform,
    /// Push a rectangular clip.
    PushClip(LRect),
    /// Pop clip.
    PopClip,
    /// Fill a path with a solid brush.
    FillPath {
        /// Geometry.
        path: Vec<PathEl>,
        /// Colour.
        color: Rgba,
        /// Even-odd instead of non-zero winding.
        even_odd: bool,
    },
    /// Stroke a path with a pen.
    StrokePath {
        /// Geometry.
        path: Vec<PathEl>,
        /// Colour.
        color: Rgba,
        /// Width in surface units.
        width: f32,
        /// Dash pattern (empty = solid).
        dashes: Vec<f32>,
    },
    /// Draw text with a host font.
    Text {
        /// Baseline origin.
        origin: (f32, f32),
        /// UTF-8 text.
        text: String,
        /// Size in surface units.
        size: f32,
        /// Colour.
        color: Rgba,
    },
    /// Draw an image (RGBA8 premultiplied, row-major).
    Image {
        /// Destination top-left.
        origin: (f32, f32),
        /// Image width.
        width: u32,
        /// Image height.
        height: u32,
        /// Pixels.
        rgba8: Vec<u8>,
        /// Global opacity.
        opacity: f32,
    },
}

/// A recorded drawing for one area/window, replayed by the UI (and shipped
/// over IPC in out-of-process mode). Antialiasing is the UI's default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    /// Commands in order.
    pub cmds: Vec<DrawCmd>,
}

impl DisplayList {
    /// Record a command.
    pub fn push(&mut self, c: DrawCmd) {
        self.cmds.push(c);
    }

    /// True when push/pop pairs balance (a malformed list is discarded
    /// rather than leaking a transform/clip into the host UI).
    pub fn is_balanced(&self) -> bool {
        let (mut t, mut c) = (0i32, 0i32);
        for cmd in &self.cmds {
            match cmd {
                DrawCmd::PushTransform(_) => t += 1,
                DrawCmd::PopTransform => t -= 1,
                DrawCmd::PushClip(_) => c += 1,
                DrawCmd::PopClip => c -= 1,
                _ => {}
            }
            if t < 0 || c < 0 {
                return false;
            }
        }
        t == 0 && c == 0
    }
}

/// Overlay theme the host must supply to plug-ins that draw comp-viewer
/// handles (colours and shadow settings matching the host UI theme).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayTheme {
    /// Foreground handle colour.
    pub foreground: Rgba,
    /// Shadow colour.
    pub shadow: Rgba,
    /// Shadow offset in pixels.
    pub shadow_offset: (f32, f32),
    /// Default stroke width.
    pub stroke_width: f32,
    /// Default vertex (handle) size.
    pub vertex_size: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_list_balance() {
        let mut d = DisplayList::default();
        d.push(DrawCmd::PushTransform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]));
        d.push(DrawCmd::FillPath {
            path: vec![
                PathEl::MoveTo(0.0, 0.0),
                PathEl::LineTo(1.0, 0.0),
                PathEl::Close,
            ],
            color: Rgba([1.0; 4]),
            even_odd: false,
        });
        assert!(!d.is_balanced());
        d.push(DrawCmd::PopTransform);
        assert!(d.is_balanced());
        d.push(DrawCmd::PopClip);
        assert!(!d.is_balanced());
    }
}
