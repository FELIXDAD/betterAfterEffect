//! Color channels and selection opacity of the AE 26 expression editor presets.

use crate::{Preferences, keys};
use serde_json::{Map, Value};

const COLORS: [&str; 13] = [
    "scripting_expressions.expr_color_default",
    "scripting_expressions.expr_color_background",
    "scripting_expressions.expr_color_line_numbers",
    "scripting_expressions.expr_color_keywords",
    "scripting_expressions.expr_color_identifiers",
    "scripting_expressions.expr_color_comments",
    "scripting_expressions.expr_color_numbers",
    "scripting_expressions.expr_color_strings",
    "scripting_expressions.expr_color_operators",
    "scripting_expressions.expr_color_indent_guides",
    "scripting_expressions.expr_color_correct_braces",
    "scripting_expressions.expr_color_incorrect_braces",
    "scripting_expressions.expr_color_selection",
];

const ALPHA: &str = "scripting_expressions.expr_selection_alpha";
const SAVED: &str = "scripting_expressions.saved_themes";
const SELECTED: &str = "scripting_expressions.selected_saved_theme";

// RGB entries follow COLORS. A theme also controls selection opacity.
#[rustfmt::skip]
fn preset(name: &str) -> Option<([u32; 13], f64)> {
    Some(match name {
        "ae_classic_dark" => ([0xcccccc, 0x1d1d1d, 0x4d4c4d, 0x348ee8, 0xeac885, 0x1d9e27, 0x3ed9ff, 0xf67476, 0x988a2e, 0x4d4c4d, 0x5bf677, 0xe50000, 0xa2e3f2], 0.3),
        "ae_classic_light" => ([0x004d77, 0xf2f2f2, 0x383e70, 0x1b2370, 0x4f2184, 0x2d781c, 0x0a85ff, 0xa8413d, 0x141414, 0xb8c8c8, 0x00eb00, 0xe60000, 0x2e2e32], 0.2),
        "ae_green_cache" => ([0xb1b1b1, 0x1d1d1d, 0x4d4c4d, 0x64bfd5, 0x22ba2e, 0xeac885, 0x348ee8, 0xf67476, 0x4d4c4d, 0x4d4c4d, 0x5bf677, 0xf3c42c, 0xe8e8e8], 0.24),
        "ae_high_contrast" => ([0xb1b1b1, 0x1d1d1d, 0x4d4c4d, 0x78e5ff, 0x2fff3f, 0xffe8ba, 0x57abff, 0xff9496, 0xad99ad, 0x4d4c4d, 0x5bf677, 0xf3c42c, 0xe8e8e8], 0.24),
        "ae_pastel" => ([0x93a1a1, 0x262626, 0x586e75, 0x2aa198, 0xe47ae4, 0xd87306, 0x348ee8, 0xfc363b, 0x8a8a8a, 0x586e75, 0x29fd42, 0xf3c42c, 0xeee8d5], 0.3),
        "basic_dark" => ([0xbee3ed, 0x1f1f1f, 0x4a5757, 0x15b8cf, 0x8e65d6, 0x697570, 0x4caf50, 0xe8ac56, 0x15b8cf, 0x738787, 0x4caf50, 0xdc322f, 0xa6b2eb], 0.1),
        "basic_light" => ([0x333333, 0xfffefd, 0x8b8b8b, 0x0b8dc6, 0x8b1a83, 0xa5a5a5, 0x02b15b, 0x1c2aa8, 0x1a1e23, 0x232323, 0x02b173, 0x7b5116, 0x808080], 0.45),
        "boundstone" => ([0xdcc7c2, 0x362c22, 0x635241, 0xaa9f47, 0xebb687, 0x7b6d59, 0xbd806a, 0x59b28f, 0xb2a5a1, 0x634a31, 0x27bb82, 0xd3ad44, 0x787159], 0.3),
        "classic" => ([0x3a3a3a, 0xf9f9f9, 0x919191, 0x0877bd, 0x614096, 0xaaaaaa, 0xb300b3, 0x0008ff, 0x3a312d, 0x919191, 0xa6e22e, 0xe96d3b, 0x77737d], 0.25),
        "cloak_o_nut" => ([0xb7a5a1, 0x374654, 0xb8cedf, 0x6aafe1, 0xd3adfa, 0x7c90aa, 0xb8cedf, 0xf4f3f5, 0xe7bd58, 0x606060, 0x72f524, 0xe16355, 0xe7bd58], 0.37),
        "down_n_away" => ([0xe6e6e0, 0x212327, 0x566280, 0xef5252, 0xe6db74, 0x818587, 0x4bd5f0, 0x75d640, 0xe6e6e0, 0x919191, 0xe6db74, 0xe72334, 0x634365], 0.35),
        "grayscale_dark" => ([0x8c8c8c, 0x1a1a1a, 0x4c4c4c, 0xcccccc, 0xffffff, 0x333333, 0xffffff, 0x999999, 0xffffff, 0x808080, 0x27a621, 0xe34c49, 0xb2b2b2], 0.35),
        "grayscale_light" => ([0x333333, 0xf2f2f2, 0x999999, 0x666666, 0x0d0d0d, 0x999999, 0x0d0d0d, 0x262626, 0x0d0d0d, 0x999999, 0x27a621, 0xe34c49, 0x000000], 0.2),
        "hough_dark" => ([0xb7b8b9, 0x0c0d0e, 0x4c4e4e, 0x37a257, 0x7069a8, 0x6e7070, 0xd65220, 0x80adcd, 0x9c9d9e, 0x4c4e4e, 0x51ed7f, 0xe01e26, 0x808080], 0.4),
        "hough_light" => ([0x515253, 0xfcfdfe, 0xb8b9ba, 0x38a358, 0x776fb0, 0x6e7070, 0xe45c28, 0x479bd4, 0x727475, 0xb8b9ba, 0x51ed7f, 0xe34c49, 0x808080], 0.4),
        "labels" => ([0xb3c7b3, 0x262626, 0xa89677, 0x3da2a5, 0xa9a9ca, 0x4aa44c, 0x677de0, 0xf46dd6, 0xe4d84c, 0xe4d84c, 0x4aa44c, 0xb53838, 0xb3c7b3], 0.3),
        "lowlight" => ([0xa7a7a7, 0x1e1e1e, 0x464b50, 0x9aab6f, 0xdbd28b, 0x595459, 0xb69663, 0xb2c6db, 0x8c8c8c, 0x464b50, 0x9df8a6, 0xcd6a50, 0x6b95c2], 0.25),
        "monakai" => ([0xf8f8f2, 0x212121, 0xeadeff, 0x66d9ef, 0xa6e22e, 0x837e69, 0xae81ff, 0xe6db74, 0xf8f8f2, 0x919191, 0xa6e22e, 0xf92672, 0x5c5c5c], 0.6),
        "neon" => ([0x0abcbd, 0x212121, 0x0abcbd, 0xd08132, 0xfff900, 0x094c4c, 0xff00ff, 0x0dfeff, 0xbb16c2, 0x0f8687, 0x0abcbd, 0xbd0a4b, 0xd08132], 0.4),
        "playfield" => ([0xd24041, 0x2d212c, 0xded4bb, 0xf1bd51, 0x1b8ba7, 0x5c636a, 0xef7c27, 0x87b43a, 0x969093, 0x969093, 0x87b43a, 0xd24041, 0xc2b2c0], 0.3),
        "raven" => ([0xdbdbdb, 0x202020, 0x919191, 0xa4eba9, 0xffffff, 0x919191, 0xfc8889, 0xffdc79, 0xa4eba9, 0x919191, 0xa4eba9, 0xfc8889, 0xffe8a9], 0.1),
        "slate" => ([0x4c5951, 0xecf4ee, 0x87928a, 0x4b9865, 0x9c5636, 0x7c8980, 0x855622, 0x208085, 0x4c5951, 0x87928a, 0x4b9865, 0xd95a4c, 0x7c9e9f], 0.3),
        "solarized_dark" => ([0x93a1a1, 0x102e36, 0x586e75, 0x2aa198, 0xd33682, 0x657b83, 0x268bd2, 0x6c71c4, 0x586e75, 0x586e75, 0x859900, 0xdc322f, 0xeee8d5], 0.3),
        "solarized_light" => ([0x323232, 0xfaf8f5, 0x494a00, 0x008080, 0x0000d7, 0x919191, 0xb300b3, 0x9b0000, 0x494a00, 0x919191, 0x008080, 0xff6162, 0x4a4a4a], 0.2),
        "terminal" => ([0x46cc29, 0x000000, 0x46cc29, 0x1b9900, 0x00ff17, 0x2c801a, 0x2ad039, 0x00ff17, 0x00ff17, 0x46cc29, 0xcafabf, 0xff0404, 0x0017ff], 0.5),
        "yellow_alert" => ([0xf0dd5f, 0x030200, 0xf0dd5f, 0xf0d100, 0xf0d100, 0x4c471e, 0xf0d100, 0xf0d100, 0xf0d100, 0xf0dd5f, 0x1bda11, 0xf0745f, 0x5f72f0], 0.25),
        "zakai" => ([0x83cfd7, 0x2a2a2a, 0x848484, 0xec7ba4, 0xfefefe, 0x6f705f, 0x9e78ee, 0xe2cc71, 0xbebec5, 0x848484, 0xa6df3f, 0xec3f59, 0xffffff], 0.2),
        _ => return None,
    })
}

impl Preferences {
    pub(crate) fn update_expression_palette(&mut self, key: &str) {
        if key == keys::EXPR_THEME {
            self.extra.remove(SELECTED);
            let Some((colors, alpha)) = preset(self.str(key)) else {
                return;
            };
            for (k, rgb) in COLORS.into_iter().zip(colors) {
                self.values.insert(k, Value::String(format!("#{rgb:06x}")));
            }
            self.values.insert(ALPHA, Value::from(alpha));
        } else if key == ALPHA || COLORS.contains(&key) {
            self.values.insert(keys::EXPR_THEME, Value::from("custom"));
            self.extra.remove(SELECTED);
        }
    }

    /// User palettes, shown after the built-in expression themes.
    pub fn saved_expression_themes(&self) -> impl Iterator<Item = &str> {
        self.extra
            .get(SAVED)
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|m| m.keys().map(String::as_str))
    }

    pub fn selected_expression_theme(&self) -> Option<&str> {
        self.extra.get(SELECTED).and_then(Value::as_str)
    }

    /// Whether this name is available for a new palette in the theme menu.
    pub fn can_save_expression_theme(&self, name: &str) -> bool {
        let name = name.trim();
        let Some(crate::Pref {
            kind: crate::Kind::Choice { options, .. },
            ..
        }) = crate::schema::find(keys::EXPR_THEME)
        else {
            return false;
        };
        !name.is_empty()
            && !options.iter().any(|o| {
                o.label.eq_ignore_ascii_case(name)
                    || crate::i18n::zh(o.label).is_some_and(|zh| zh == name)
            })
            && !self
                .saved_expression_themes()
                .any(|n| n.eq_ignore_ascii_case(name))
    }

    pub fn save_expression_theme(&mut self, name: &str) -> bool {
        let name = name.trim();
        if !self.can_save_expression_theme(name) {
            return false;
        }
        let colors: Map<_, _> = COLORS
            .into_iter()
            .chain([ALPHA])
            .map(|key| (key.to_owned(), self.values[key].clone()))
            .collect();
        let saved = self
            .extra
            .entry(SAVED)
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(saved) = saved.as_object_mut() else {
            return false;
        };
        saved.insert(name.to_owned(), Value::Object(colors));
        self.values.insert(keys::EXPR_THEME, Value::from("custom"));
        self.extra.insert(SELECTED.into(), Value::from(name));
        true
    }

    pub fn select_expression_theme(&mut self, name: &str) {
        let Some(saved) = self
            .extra
            .get(SAVED)
            .and_then(|v| v.get(name))
            .and_then(Value::as_object)
        else {
            return;
        };
        for key in COLORS.into_iter().chain([ALPHA]) {
            if let Some(v) = saved
                .get(key)
                .and_then(|v| crate::validate(crate::schema::find(key)?, v))
            {
                self.values.insert(key, v);
            }
        }
        self.values.insert(keys::EXPR_THEME, Value::from("custom"));
        self.extra.insert(SELECTED.into(), Value::from(name));
    }

    pub fn delete_selected_expression_theme(&mut self) {
        if let Some(Value::String(name)) = self.extra.remove(SELECTED)
            && let Some(saved) = self.extra.get_mut(SAVED).and_then(Value::as_object_mut)
        {
            saved.remove(&name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_factory_palettes_and_saved_colors_round_trip() {
        let mut p = Preferences::default();
        let crate::Kind::Choice { options, .. } =
            crate::schema::find(keys::EXPR_THEME).unwrap().kind
        else {
            panic!()
        };
        for option in options.iter().filter(|o| o.value != "custom") {
            assert!(preset(option.value).is_some(), "{}", option.value);
        }
        p.set(keys::EXPR_THEME, "monakai");
        assert_eq!(p.str(COLORS[1]), "#212121");
        assert_eq!(p.float(ALPHA), 0.6);
        p.set(COLORS[3], "#aabbcc");
        assert_eq!(p.str(keys::EXPR_THEME), "custom");
        assert!(p.save_expression_theme("夜间"));
        p.set(keys::EXPR_THEME, "ae_classic_dark");
        assert_eq!(p.str(COLORS[1]), "#1d1d1d");
        p.select_expression_theme("夜间");
        let (mut reopened, repaired) = Preferences::from_json(&p.to_json());
        assert!(repaired.is_empty());
        assert_eq!(reopened.selected_expression_theme(), Some("夜间"));
        assert_eq!(reopened.str(COLORS[3]), "#aabbcc");
        assert_eq!(reopened.float(ALPHA), 0.6);
        reopened.delete_selected_expression_theme();
        assert_eq!(reopened.saved_expression_themes().count(), 0);
    }
}
