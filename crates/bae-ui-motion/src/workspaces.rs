//! AE 26's predefined workspaces (Window ▸ Workspace) besides Default and
//! Review (`bae_ui::presets`): the panels and how they are arranged follow AE's
//! factory layouts, sizes scaled to the Default workspace's 1722 × 935 points.
//! Panel ids no module registers yet show AE's panel outline ([`crate::ae_panels`]).

use bae_ui::WorkspacePreset;
use bae_ui::dock::{FloatingFrame, Frame, Node};
use bae_ui::presets::AE_KEYMAP;

/// `(id, preset)` in AE's order (after Default and Review).
pub(crate) fn presets() -> Vec<(&'static str, WorkspacePreset)> {
    vec![
        ("ae_learn", learn()),
        ("ae_small_screen", small_screen()),
        ("ae_standard", standard()),
        ("ae_libraries", libraries()),
        ("ae_all_panels", all_panels()),
        ("ae_animation", animation()),
        ("ae_essential_graphics", essential_graphics()),
        ("ae_color", color()),
        ("ae_effects", effects()),
        ("ae_minimal", minimal()),
        ("ae_paint", paint()),
        ("ae_text", text()),
        ("ae_motion_tracking", motion_tracking()),
        ("ae_undocked_panels", undocked_panels()),
    ]
}

fn learn() -> WorkspacePreset {
    WorkspacePreset::new(
        "Learn",
        AE_KEYMAP,
        Node::row(vec![
            (373.0, Node::tabs(&["learn"])),
            (
                1349.0,
                Node::column(vec![
                    (
                        600.0,
                        Node::row(vec![
                            (218.0, Node::tabs(&["project"])),
                            (927.0, Node::tabs(&["composition"])),
                            (204.0, Node::tabs(&["properties"])),
                        ]),
                    ),
                    (335.0, Node::tabs(&["timeline"])),
                ]),
            ),
        ]),
    )
    .order(10)
}

fn small_screen() -> WorkspacePreset {
    WorkspacePreset::new(
        "Small Screen",
        AE_KEYMAP,
        Node::row(vec![
            (
                1417.0,
                Node::column(vec![
                    (
                        639.0,
                        Node::row(vec![
                            (309.0, Node::tabs(&["project", "effect_controls"])),
                            (1108.0, Node::tabs(&["composition"])),
                        ]),
                    ),
                    (296.0, Node::tabs(&["timeline"])),
                ]),
            ),
            (
                305.0,
                Node::column(vec![
                    (49.0, Node::tabs(&["preview"])),
                    (
                        886.0,
                        Node::frame(
                            Frame::new(&["properties", "align", "audio", "effects"]).stacked(None),
                        ),
                    ),
                ]),
            ),
        ]),
    )
    .order(11)
}

fn standard() -> WorkspacePreset {
    WorkspacePreset::new(
        "Standard",
        AE_KEYMAP,
        Node::column(vec![
            (
                630.0,
                Node::row(vec![
                    (217.0, Node::tabs(&["project", "effect_controls"])),
                    (1301.0, Node::tabs(&["composition"])),
                    (
                        204.0,
                        Node::column(vec![
                            (108.0, Node::tabs(&["info", "preview", "align", "audio"])),
                            (522.0, Node::tabs(&["properties", "effects", "libraries"])),
                        ]),
                    ),
                ]),
            ),
            (305.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(12)
}

fn libraries() -> WorkspacePreset {
    WorkspacePreset::new(
        "Libraries",
        AE_KEYMAP,
        Node::row(vec![
            (
                1522.0,
                Node::column(vec![
                    (
                        600.0,
                        Node::row(vec![
                            (309.0, Node::tabs(&["project"])),
                            (1213.0, Node::tabs(&["composition", "metadata"])),
                        ]),
                    ),
                    (335.0, Node::tabs(&["timeline"])),
                ]),
            ),
            (200.0, Node::tabs(&["libraries"])),
        ]),
    )
    .order(13)
}

fn all_panels() -> WorkspacePreset {
    WorkspacePreset::new(
        "All Panels",
        AE_KEYMAP,
        Node::column(vec![
            (
                600.0,
                Node::row(vec![
                    (218.0, Node::tabs(&["project"])),
                    (1276.0, Node::tabs(&["composition"])),
                    (
                        228.0,
                        Node::frame(
                            Frame::new(&[
                                "properties",
                                "info",
                                "audio",
                                "effects",
                                "libraries",
                                "align",
                                "character",
                                "paragraph",
                                "tracker",
                                "content_aware_fill",
                                "paint",
                                "brushes",
                                "motion_sketch",
                                "smoother",
                                "wiggler",
                                "mask_interpolation",
                            ])
                            .stacked(None),
                        ),
                    ),
                ]),
            ),
            (335.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(14)
}

fn animation() -> WorkspacePreset {
    WorkspacePreset::new(
        "Animation",
        AE_KEYMAP,
        Node::column(vec![
            (
                598.0,
                Node::row(vec![
                    (145.0, Node::tabs(&["project"])),
                    (1436.0, Node::tabs(&["composition"])),
                    (
                        141.0,
                        Node::frame(
                            Frame::new(&[
                                "properties",
                                "info",
                                "preview",
                                "effects",
                                "motion_sketch",
                                "wiggler",
                                "smoother",
                                "audio",
                            ])
                            .stacked(None),
                        ),
                    ),
                ]),
            ),
            (337.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(15)
}

fn essential_graphics() -> WorkspacePreset {
    WorkspacePreset::new(
        "Essential Graphics",
        AE_KEYMAP,
        Node::column(vec![
            (
                599.0,
                Node::row(vec![
                    (218.0, Node::tabs(&["project"])),
                    (460.0, Node::tabs(&["essential_graphics"])),
                    (832.0, Node::tabs(&["composition"])),
                    (
                        212.0,
                        Node::frame(
                            Frame::new(&[
                                "properties",
                                "info",
                                "audio",
                                "preview",
                                "effects",
                                "align",
                                "libraries",
                                "character",
                                "paragraph",
                                "tracker",
                                "content_aware_fill",
                            ])
                            .stacked(None),
                        ),
                    ),
                ]),
            ),
            (336.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(16)
}

fn color() -> WorkspacePreset {
    WorkspacePreset::new(
        "Color",
        AE_KEYMAP,
        Node::column(vec![
            (
                646.0,
                Node::row(vec![
                    (
                        347.0,
                        Node::frame(Frame::new(&["project", "effect_controls"]).active(1)),
                    ),
                    (466.0, Node::tabs(&["lumetri_scopes"])),
                    (909.0, Node::tabs(&["composition"])),
                ]),
            ),
            (
                289.0,
                Node::row(vec![
                    (1507.0, Node::tabs(&["timeline"])),
                    (
                        215.0,
                        Node::frame(
                            Frame::new(&["info", "preview", "effects", "libraries"])
                                .active(2)
                                .stacked(None),
                        ),
                    ),
                ]),
            ),
        ]),
    )
    .order(17)
}

fn effects() -> WorkspacePreset {
    WorkspacePreset::new(
        "Effects",
        AE_KEYMAP,
        Node::column(vec![
            (
                598.0,
                Node::row(vec![
                    (245.0, Node::tabs(&["effect_controls", "project"])),
                    (1242.0, Node::tabs(&["composition"])),
                    (
                        236.0,
                        Node::frame(
                            Frame::new(&["properties", "info", "preview", "effects"]).stacked(None),
                        ),
                    ),
                ]),
            ),
            (337.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(18)
}

fn minimal() -> WorkspacePreset {
    WorkspacePreset::new(
        "Minimal",
        AE_KEYMAP,
        Node::column(vec![
            (599.0, Node::tabs(&["composition"])),
            (336.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(19)
}

fn paint() -> WorkspacePreset {
    WorkspacePreset::new(
        "Paint",
        AE_KEYMAP,
        Node::column(vec![
            (
                598.0,
                Node::row(vec![
                    (899.0, Node::tabs(&["composition", "project"])),
                    (674.0, Node::tabs(&["layer"])),
                    (
                        149.0,
                        Node::frame(
                            Frame::new(&["preview", "paint", "brushes"])
                                .active(1)
                                .stacked(None),
                        ),
                    ),
                ]),
            ),
            (337.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(20)
}

fn text() -> WorkspacePreset {
    WorkspacePreset::new(
        "Text",
        AE_KEYMAP,
        Node::column(vec![
            (
                601.0,
                Node::row(vec![
                    (218.0, Node::tabs(&["project"])),
                    (1356.0, Node::tabs(&["composition"])),
                    (
                        149.0,
                        Node::frame(
                            Frame::new(&["preview", "character", "paragraph", "effects"])
                                .active(3)
                                .stacked(None),
                        ),
                    ),
                ]),
            ),
            (334.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(21)
}

fn motion_tracking() -> WorkspacePreset {
    WorkspacePreset::new(
        "Motion Tracking",
        AE_KEYMAP,
        Node::column(vec![
            (
                599.0,
                Node::row(vec![
                    (218.0, Node::tabs(&["project"])),
                    (1356.0, Node::tabs(&["composition"])),
                    (
                        149.0,
                        Node::frame(
                            Frame::new(&["info", "preview", "tracker", "content_aware_fill"])
                                .active(2)
                                .stacked(None),
                        ),
                    ),
                ]),
            ),
            (336.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(22)
}

fn undocked_panels() -> WorkspacePreset {
    let mut preset = WorkspacePreset::new(
        "Undocked Panels",
        AE_KEYMAP,
        Node::column(vec![
            (
                600.0,
                Node::row(vec![
                    (309.0, Node::tabs(&["project"])),
                    (1413.0, Node::tabs(&["composition"])),
                ]),
            ),
            (335.0, Node::tabs(&["timeline"])),
        ]),
    )
    .order(23);
    preset.layout.floating = vec![
        FloatingFrame {
            frame: Frame::new(&["info"]),
            pos: Some([1156.0, 73.0]),
            size: [210.0, 161.0],
        },
        FloatingFrame {
            frame: Frame::new(&["preview"]),
            pos: Some([1156.0, 235.0]),
            size: [210.0, 167.0],
        },
        FloatingFrame {
            frame: Frame::new(&["effects"]),
            pos: Some([1157.0, 403.0]),
            size: [210.0, 327.0],
        },
        FloatingFrame {
            frame: Frame::new(&["properties"]),
            pos: Some([837.0, 73.0]),
            size: [318.0, 270.0],
        },
    ];
    preset
}
