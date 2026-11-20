//! Identifier rules (spec §5.2).
//!
//! Plugin ids and contribution ids are what documents store, so they are
//! strict ASCII and never change across plugin versions.

/// Maximum length of a plugin id in bytes.
pub const MAX_PLUGIN_ID_LEN: usize = 128;
/// Maximum length of a contribution id or parameter key in bytes.
pub const MAX_ID_LEN: usize = 64;
/// Maximum length of an intermediate texture name in bytes.
pub const MAX_TEXTURE_NAME_LEN: usize = 32;

/// Platform keys a manifest may name (`<os>-<arch>`).
pub const PLATFORM_KEYS: &[&str] = &[
    "macos-aarch64",
    "macos-x86_64",
    "windows-x86_64",
    "windows-aarch64",
    "linux-x86_64",
    "linux-aarch64",
    "ios-aarch64",
    "android-aarch64",
];

/// The platform key of the running host, `None` on platforms BAE does not ship for.
pub fn current_platform() -> Option<&'static str> {
    let key = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        _ => return None,
    };
    Some(key)
}

/// Checks a plugin id: reverse-DNS, ≥ 2 dot-separated segments, each
/// `[a-z0-9]([a-z0-9-]*[a-z0-9])?`, ≤ 128 bytes.
pub fn check_plugin_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > MAX_PLUGIN_ID_LEN {
        return Err(format!(
            "plugin id must be 1..={MAX_PLUGIN_ID_LEN} bytes long"
        ));
    }
    let segments: Vec<&str> = id.split('.').collect();
    if segments.len() < 2 {
        return Err(
            "plugin id must be reverse-DNS with at least two segments, e.g. `com.example.glow`"
                .into(),
        );
    }
    for segment in segments {
        let bytes = segment.as_bytes();
        let ok = !bytes.is_empty()
            && bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            && bytes[0] != b'-'
            && bytes[bytes.len() - 1] != b'-';
        if !ok {
            return Err(format!(
                "plugin id segment `{segment}` must match [a-z0-9]([a-z0-9-]*[a-z0-9])?"
            ));
        }
    }
    Ok(())
}

/// `true` for ids reserved for first-party packages (`bae.*`).
pub fn is_first_party(plugin_id: &str) -> bool {
    plugin_id == "bae" || plugin_id.starts_with("bae.")
}

/// Checks a contribution id: `[a-z][a-z0-9_-]*`, ≤ 64 bytes.
pub fn check_contribution_id(id: &str) -> Result<(), String> {
    check_ident(id, MAX_ID_LEN, true)
        .map_err(|why| format!("id `{id}` {why} (expected [a-z][a-z0-9_-]*, ≤ {MAX_ID_LEN} bytes)"))
}

/// Checks a parameter key: `[a-z][a-z0-9_]*`, ≤ 64 bytes, not starting with `bae_`.
/// Keys become WGSL struct fields and JS property names.
pub fn check_param_key(key: &str) -> Result<(), String> {
    check_ident(key, MAX_ID_LEN, false).map_err(|why| {
        format!("key `{key}` {why} (expected [a-z][a-z0-9_]*, ≤ {MAX_ID_LEN} bytes)")
    })?;
    if key.starts_with("bae_") {
        return Err(format!("key `{key}`: the prefix `bae_` is reserved"));
    }
    if WGSL_RESERVED.contains(&key) {
        return Err(format!("key `{key}` is a WGSL keyword or reserved word"));
    }
    Ok(())
}

/// Checks an intermediate texture name: `[a-z][a-z0-9_]*`, ≤ 32 bytes.
pub fn check_texture_name(name: &str) -> Result<(), String> {
    check_ident(name, MAX_TEXTURE_NAME_LEN, false).map_err(|why| {
        format!(
            "texture name `{name}` {why} (expected [a-z][a-z0-9_]*, ≤ {MAX_TEXTURE_NAME_LEN} bytes)"
        )
    })?;
    if matches!(name, "input" | "output" | "to") {
        return Err(format!("texture name `{name}` is reserved"));
    }
    Ok(())
}

fn check_ident(s: &str, max: usize, allow_dash: bool) -> Result<(), &'static str> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > max {
        return Err("has an invalid length");
    }
    if !bytes[0].is_ascii_lowercase() {
        return Err("must start with a lowercase letter");
    }
    let ok = bytes.iter().all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || (allow_dash && *b == b'-')
    });
    if ok {
        Ok(())
    } else {
        Err("contains invalid characters")
    }
}

/// Full id of a contribution, as stored in documents (`com.example.glow/glow`).
/// For effects this is the `match_name` of the `bae_model::EffectDef`.
pub fn full_id(plugin_id: &str, contribution_id: &str) -> String {
    format!("{plugin_id}/{contribution_id}")
}

/// Checks a package-relative path: `/`-separated, not absolute, no `..`, no
/// backslashes, no empty segments.
pub fn check_package_path(path: &str) -> Result<(), String> {
    let bad = path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..");
    if bad {
        Err(format!(
            "path `{path}` must be relative to the package root, use `/` and contain no `.`/`..` segments"
        ))
    } else {
        Ok(())
    }
}

/// WGSL keywords and reserved words that cannot be struct field names.
const WGSL_RESERVED: &[&str] = &[
    "alias",
    "break",
    "case",
    "const",
    "const_assert",
    "continue",
    "continuing",
    "default",
    "diagnostic",
    "discard",
    "else",
    "enable",
    "false",
    "fn",
    "for",
    "if",
    "let",
    "loop",
    "override",
    "requires",
    "return",
    "struct",
    "switch",
    "true",
    "var",
    "while",
    "array",
    "bool",
    "f16",
    "f32",
    "i32",
    "u32",
    "mat2x2",
    "mat3x3",
    "mat4x4",
    "vec2",
    "vec3",
    "vec4",
    "ptr",
    "sampler",
    "texture_2d",
    "atomic",
    "self",
    "super",
    "type",
    "mod",
    "match",
    "loop",
    "in",
    "out",
    "inout",
    "uniform",
    "storage",
    "function",
    "private",
    "workgroup",
    "module",
    "macro",
    "new",
    "null",
    "this",
    "static",
    "virtual",
    "where",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_ids() {
        check_plugin_id("com.example.glow").unwrap();
        check_plugin_id("com.example.box-blur").unwrap();
        check_plugin_id("bae.effects").unwrap();
        assert!(check_plugin_id("glow").is_err());
        assert!(check_plugin_id("com.Example.glow").is_err());
        assert!(check_plugin_id("com..glow").is_err());
        assert!(check_plugin_id("com.-glow").is_err());
        assert!(check_plugin_id(&format!("com.{}", "a".repeat(130))).is_err());
        assert!(is_first_party("bae.effects"));
        assert!(!is_first_party("baex.effects"));
    }

    #[test]
    fn contribution_ids_and_keys() {
        check_contribution_id("box-blur").unwrap();
        check_contribution_id("a_b").unwrap();
        assert!(check_contribution_id("Blur").is_err());
        assert!(check_contribution_id("1blur").is_err());
        check_param_key("amount").unwrap();
        assert!(check_param_key("box-size").is_err());
        assert!(check_param_key("bae_x").is_err());
        assert!(check_param_key("loop").is_err());
        check_texture_name("tmp").unwrap();
        assert!(check_texture_name("output").is_err());
        assert_eq!(full_id("com.example.glow", "glow"), "com.example.glow/glow");
    }

    #[test]
    fn package_paths() {
        check_package_path("shaders/a.wgsl").unwrap();
        for bad in ["", "/abs", "../x", "a/../b", "a\\b", "C:/x", "a//b", "./a"] {
            assert!(check_package_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn platform_key_is_known() {
        if let Some(key) = current_platform() {
            assert!(PLATFORM_KEYS.contains(&key));
        }
    }
}
