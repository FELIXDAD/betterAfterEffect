//! Contribution kinds and implementation forms (spec §3, §4).
//!
//! A plugin provides 1..N *contributions*. Each has a [`ContributionKind`]
//! (which registry it goes into, = manifest table name) and an
//! [`Implementation`] (how it is executed). Native contributions are exported
//! with a [`NativeKind`] code (`BAE_KIND_*` in `bae_plugin.h`); a manifest
//! `[[codec]]` maps to up to two native kinds (decoder, encoder).

use core::fmt;

/// How a contribution is implemented (manifest key `implementation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Implementation {
    /// Declarative WGSL package executed by the host's generic shader node.
    Wgsl,
    /// Native code behind the C ABI (`.dylib` / `.dll`).
    Native,
    /// JavaScript module on the host's QuickJS runtime.
    Script,
    /// Pure data (workspaces, keymaps): nothing is executed.
    Data,
    /// Compiled into BAE and registered by an app module (never in a manifest).
    Builtin,
}

impl Implementation {
    /// Manifest spelling, `None` for forms that never appear in a manifest.
    pub const fn manifest_name(self) -> Option<&'static str> {
        match self {
            Implementation::Wgsl => Some("wgsl"),
            Implementation::Native => Some("native"),
            Implementation::Script => Some("script"),
            Implementation::Data | Implementation::Builtin => None,
        }
    }

    /// Parses the manifest spelling (`"wgsl"`, `"native"`, `"script"`).
    pub fn from_manifest_name(name: &str) -> Option<Self> {
        match name {
            "wgsl" => Some(Implementation::Wgsl),
            "native" => Some(Implementation::Native),
            "script" => Some(Implementation::Script),
            _ => None,
        }
    }
}

impl fmt::Display for Implementation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Implementation::Wgsl => "wgsl",
            Implementation::Native => "native",
            Implementation::Script => "script",
            Implementation::Data => "data",
            Implementation::Builtin => "builtin",
        })
    }
}

/// The kind of a contribution = the manifest table it is declared in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContributionKind {
    /// `[[effect]]`: effects, generators and transitions.
    Effect,
    /// `[[shape_operator]]`.
    ShapeOperator,
    /// `[[text_selector]]` (reserved: accepted by the parser, never loaded yet).
    TextSelector,
    /// `[[importer]]`.
    Importer,
    /// `[[exporter]]`.
    Exporter,
    /// `[[codec]]` with roles `decode` and/or `encode`.
    Codec,
    /// `[[command]]`: menus, shortcuts, MCP tools.
    Command,
    /// `[[script]]`: an automation script, registered as a command.
    Script,
    /// `[[panel]]`.
    Panel,
    /// `[[workspace]]` (data only).
    Workspace,
    /// `[[keymap]]` (data only).
    Keymap,
    /// `[[expression_library]]`.
    ExpressionLibrary,
}

impl ContributionKind {
    /// Every kind, in manifest documentation order.
    pub const ALL: [ContributionKind; 12] = [
        ContributionKind::Effect,
        ContributionKind::ShapeOperator,
        ContributionKind::TextSelector,
        ContributionKind::Importer,
        ContributionKind::Exporter,
        ContributionKind::Codec,
        ContributionKind::Command,
        ContributionKind::Script,
        ContributionKind::Panel,
        ContributionKind::Workspace,
        ContributionKind::Keymap,
        ContributionKind::ExpressionLibrary,
    ];

    /// Manifest table name (`[[effect]]` → `"effect"`).
    pub const fn manifest_table(self) -> &'static str {
        match self {
            ContributionKind::Effect => "effect",
            ContributionKind::ShapeOperator => "shape_operator",
            ContributionKind::TextSelector => "text_selector",
            ContributionKind::Importer => "importer",
            ContributionKind::Exporter => "exporter",
            ContributionKind::Codec => "codec",
            ContributionKind::Command => "command",
            ContributionKind::Script => "script",
            ContributionKind::Panel => "panel",
            ContributionKind::Workspace => "workspace",
            ContributionKind::Keymap => "keymap",
            ContributionKind::ExpressionLibrary => "expression_library",
        }
    }

    /// Inverse of [`ContributionKind::manifest_table`].
    pub fn from_manifest_table(table: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.manifest_table() == table)
    }

    /// Implementation forms a plugin may use for this kind (spec §3 table).
    /// [`Implementation::Builtin`] is always additionally allowed for app modules
    /// except for data-only kinds. Empty for reserved kinds.
    pub const fn plugin_forms(self) -> &'static [Implementation] {
        use Implementation as I;
        match self {
            ContributionKind::Effect => &[I::Wgsl, I::Native],
            ContributionKind::ShapeOperator | ContributionKind::Codec => &[I::Native],
            ContributionKind::TextSelector => &[],
            ContributionKind::Importer
            | ContributionKind::Exporter
            | ContributionKind::Command
            | ContributionKind::ExpressionLibrary => &[I::Native, I::Script],
            ContributionKind::Script | ContributionKind::Panel => &[I::Script],
            ContributionKind::Workspace | ContributionKind::Keymap => &[I::Data],
        }
    }

    /// `true` if the kind is reserved: manifests may declare it, the host lists
    /// it, but it is never registered in this API version.
    pub const fn is_reserved(self) -> bool {
        matches!(self, ContributionKind::TextSelector)
    }

    /// `true` if the manifest entry needs an explicit `implementation` key
    /// (more than one form is possible).
    pub const fn requires_implementation_key(self) -> bool {
        self.plugin_forms().len() > 1
    }

    /// Native export kinds that implement this manifest kind.
    pub const fn native_kinds(self) -> &'static [NativeKind] {
        match self {
            ContributionKind::Effect => &[NativeKind::Effect],
            ContributionKind::ShapeOperator => &[NativeKind::ShapeOperator],
            ContributionKind::TextSelector => &[NativeKind::TextSelector],
            ContributionKind::Importer => &[NativeKind::Importer],
            ContributionKind::Exporter => &[NativeKind::Exporter],
            ContributionKind::Codec => &[NativeKind::Decoder, NativeKind::Encoder],
            ContributionKind::Command => &[NativeKind::Command],
            ContributionKind::Panel => &[NativeKind::Panel],
            ContributionKind::ExpressionLibrary => &[NativeKind::ExpressionLibrary],
            ContributionKind::Script | ContributionKind::Workspace | ContributionKind::Keymap => {
                &[]
            }
        }
    }
}

impl fmt::Display for ContributionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.manifest_table())
    }
}

/// Kind code of a native export (`BaeContribution.kind`, `BAE_KIND_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum NativeKind {
    /// `BAE_KIND_EFFECT`, vtable `BaeEffectVTable1`.
    Effect = 1,
    /// `BAE_KIND_SHAPE_OPERATOR`, vtable `BaeShapeOperatorVTable1`.
    ShapeOperator = 2,
    /// `BAE_KIND_TEXT_SELECTOR` (reserved, no vtable yet).
    TextSelector = 3,
    /// `BAE_KIND_IMPORTER`, vtable `BaeImporterVTable1`.
    Importer = 4,
    /// `BAE_KIND_EXPORTER`, vtable `BaeExporterVTable1`.
    Exporter = 5,
    /// `BAE_KIND_DECODER`, vtable `BaeDecoderVTable1` (codec role `decode`).
    Decoder = 6,
    /// `BAE_KIND_ENCODER`, vtable `BaeEncoderVTable1` (codec role `encode`).
    Encoder = 7,
    /// `BAE_KIND_COMMAND`, vtable `BaeCommandVTable1`.
    Command = 8,
    /// `BAE_KIND_PANEL` (reserved for native panels).
    Panel = 9,
    /// `BAE_KIND_EXPRESSION_LIBRARY`, vtable `BaeExpressionLibraryVTable1`.
    ExpressionLibrary = 10,
    /// `BAE_KIND_AUDIO_EFFECT` (reserved).
    AudioEffect = 11,
}

impl NativeKind {
    /// Decodes `BaeContribution.kind`. Unknown codes return `None` and must be
    /// skipped with a warning (ABI rule R9), not treated as fatal.
    pub const fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            1 => NativeKind::Effect,
            2 => NativeKind::ShapeOperator,
            3 => NativeKind::TextSelector,
            4 => NativeKind::Importer,
            5 => NativeKind::Exporter,
            6 => NativeKind::Decoder,
            7 => NativeKind::Encoder,
            8 => NativeKind::Command,
            9 => NativeKind::Panel,
            10 => NativeKind::ExpressionLibrary,
            11 => NativeKind::AudioEffect,
            _ => return None,
        })
    }

    /// The C constant value.
    pub const fn raw(self) -> u32 {
        self as u32
    }

    /// `true` if a vtable is defined for this kind in the current ABI.
    pub const fn has_vtable(self) -> bool {
        !matches!(
            self,
            NativeKind::TextSelector | NativeKind::Panel | NativeKind::AudioEffect
        )
    }

    /// The manifest kind this native kind implements (`None` for reserved
    /// kinds without a manifest table).
    pub const fn contribution_kind(self) -> Option<ContributionKind> {
        Some(match self {
            NativeKind::Effect => ContributionKind::Effect,
            NativeKind::ShapeOperator => ContributionKind::ShapeOperator,
            NativeKind::TextSelector => ContributionKind::TextSelector,
            NativeKind::Importer => ContributionKind::Importer,
            NativeKind::Exporter => ContributionKind::Exporter,
            NativeKind::Decoder | NativeKind::Encoder => ContributionKind::Codec,
            NativeKind::Command => ContributionKind::Command,
            NativeKind::Panel => ContributionKind::Panel,
            NativeKind::ExpressionLibrary => ContributionKind::ExpressionLibrary,
            NativeKind::AudioEffect => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_tables_roundtrip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for k in ContributionKind::ALL {
            assert!(seen.insert(k.manifest_table()));
            assert_eq!(
                ContributionKind::from_manifest_table(k.manifest_table()),
                Some(k)
            );
        }
        assert_eq!(ContributionKind::from_manifest_table("effects"), None);
    }

    #[test]
    fn native_kind_codes_match_header() {
        // Values of BAE_KIND_* in sdk/c/bae_plugin.h.
        let expected = [
            (NativeKind::Effect, 1),
            (NativeKind::ShapeOperator, 2),
            (NativeKind::TextSelector, 3),
            (NativeKind::Importer, 4),
            (NativeKind::Exporter, 5),
            (NativeKind::Decoder, 6),
            (NativeKind::Encoder, 7),
            (NativeKind::Command, 8),
            (NativeKind::Panel, 9),
            (NativeKind::ExpressionLibrary, 10),
            (NativeKind::AudioEffect, 11),
        ];
        for (k, raw) in expected {
            assert_eq!(k.raw(), raw);
            assert_eq!(NativeKind::from_raw(raw), Some(k));
        }
        assert_eq!(NativeKind::from_raw(0), None);
        assert_eq!(NativeKind::from_raw(12), None);
    }

    #[test]
    fn native_kinds_consistent_with_forms() {
        for k in ContributionKind::ALL {
            let native_form = k.plugin_forms().contains(&Implementation::Native);
            for nk in k.native_kinds() {
                assert_eq!(nk.contribution_kind(), Some(k));
                if nk.has_vtable() {
                    assert!(native_form, "{k} exports native vtables but forbids native");
                }
            }
        }
    }

    #[test]
    fn implementation_names() {
        for i in [
            Implementation::Wgsl,
            Implementation::Native,
            Implementation::Script,
        ] {
            assert_eq!(
                Implementation::from_manifest_name(i.manifest_name().unwrap()),
                Some(i)
            );
        }
        assert_eq!(Implementation::from_manifest_name("rust"), None);
        assert!(ContributionKind::Effect.requires_implementation_key());
        assert!(!ContributionKind::Script.requires_implementation_key());
    }
}
