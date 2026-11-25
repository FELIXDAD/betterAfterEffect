use super::*;

pub(super) fn effect() -> Builtin {
    let builder = DefBuilder::new("ADBE Aud Reverse", "Backwards", "Audio").checkbox(
        1,
        "Swap Channels",
        "互换声道",
        false,
    );
    audio_control("backwards", "倒放", builder, &[1])
}
