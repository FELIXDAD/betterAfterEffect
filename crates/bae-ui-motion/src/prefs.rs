//! The Preferences the motion workspace reads (bae-prefs' `Arc<PrefsStore>`
//! app resource; AE's factory defaults without one, e.g. in tests): default
//! label colors, Create Layers at Composition Start Time, and the import
//! defaults. Values are passed to the commands, which stay preference-free.

use std::sync::Arc;

use bae_app::Resources;
use bae_model::{LayerKind, Time};
use bae_prefs::{Preferences, PrefsStore};

/// The current preferences.
pub(crate) fn current(resources: &Resources) -> Arc<Preferences> {
    resources
        .get::<Arc<PrefsStore>>()
        .map(|s| s.current())
        .unwrap_or_default()
}

/// Labels page: the default label of a new layer of `kind`.
pub(crate) fn layer_label(p: &Preferences, kind: LayerKind) -> i64 {
    let key = match kind {
        LayerKind::Solid => "labels.label_default_solid",
        LayerKind::Text => "labels.label_default_text",
        LayerKind::Shape => "labels.label_default_shape",
        LayerKind::Null => "labels.label_default_null",
        LayerKind::Camera => "labels.label_default_camera",
        LayerKind::Light => "labels.label_default_light",
        LayerKind::Adjustment => "labels.label_default_adjustment",
        LayerKind::PreComp => "labels.label_default_comp",
        LayerKind::Footage => "labels.label_default_video",
        LayerKind::Audio => "labels.label_default_audio",
    };
    p.int(key)
}

/// Labels page: the default label of new compositions.
pub(crate) fn comp_label(p: &Preferences) -> i64 {
    p.int("labels.label_default_comp")
}

/// Composition page: new layers start at the comp's start (else at the
/// current time).
pub(crate) fn layers_at_comp_start(p: &Preferences) -> bool {
    p.bool("composition.create_layers_at_comp_start")
}

/// Import page: frames per second of image sequences.
pub(crate) fn sequence_fps(p: &Preferences) -> f64 {
    p.float("import.sequence_footage_fps")
}

/// Import page: the duration of still footage when it is fixed (else the
/// length of the composition: `None`). The timecode counts frames at 30 fps.
pub(crate) fn still_duration(p: &Preferences) -> Option<Time> {
    if p.str("import.still_footage_duration_mode") != "fixed_duration" {
        return None;
    }
    let [h, m, s, f] = bae_prefs::parse_timecode(p.str("import.still_footage_duration_timecode"))?;
    let seconds = f64::from(h * 3600 + m * 60 + s) + f64::from(f) / 30.0;
    (seconds > 0.0).then(|| Time::from_seconds(seconds))
}

/// Labels page: the default label of imported footage.
pub(crate) fn footage_label(p: &Preferences, kind: bae_media::MediaKind) -> i64 {
    use bae_media::MediaKind;
    p.int(match kind {
        MediaKind::Still => "labels.label_default_still",
        MediaKind::Audio => "labels.label_default_audio",
        MediaKind::ImageSequence | MediaKind::Movie => "labels.label_default_video",
    })
}
