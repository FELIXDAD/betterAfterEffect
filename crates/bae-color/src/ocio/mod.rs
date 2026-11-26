//! A subset of OpenColorIO: reads `config.ocio` (v1 and v2) and builds
//! [`ColorTransform`]s without linking the OpenColorIO library.
//!
//! Supported transforms: Group, Matrix, Exponent, ExponentWithLinear, Log,
//! LogAffine, LogCamera, Range, CDL, File (`.cube`, `.spi1d`, `.spi3d`),
//! ColorSpace, Builtin (see [`builtin`]) and Allocation (a no-op). Colour
//! spaces, display colour spaces, roles, aliases, view transforms (scene ↔
//! display references), displays / views / shared views and file rules
//! (glob patterns) and individual looks are understood. Not yet: named transforms, context
//! variables, FixedFunction / Grading / ExposureContrast transforms, and the
//! ACES output transforms (they report [`ColorError::Unsupported`]).

pub mod builtin;
pub mod yaml;

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::ColorError;
use crate::lut::Interpolation;
use crate::math::{Affine, Mat3};
use crate::ops::{CdlOp, LogOp, MoncurveOp, NegativeStyle, PowerOp};
use crate::space::{ColorSpace, ColorSpaceKind, Encoding, builtin as bi};
use crate::transform::{ColorOp, ColorTransform};
use yaml::Node;

/// Scene- or display-referred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    /// `colorspaces`: relative to the scene reference.
    Scene,
    /// `display_colorspaces`: relative to the display reference.
    Display,
}

/// A colour space of the config.
#[derive(Clone, Debug)]
pub struct OcioColorSpace {
    /// Name.
    pub name: String,
    /// Aliases.
    pub aliases: Vec<String>,
    /// Family.
    pub family: String,
    /// `encoding` (scene-linear, log, sdr-video …).
    pub encoding: String,
    /// `isdata`.
    pub is_data: bool,
    /// Which reference it is defined against.
    pub reference: Reference,
    /// Description.
    pub description: String,
    to_ref: Option<Node>,
    from_ref: Option<Node>,
}

/// A view transform.
#[derive(Clone, Debug)]
pub struct OcioViewTransform {
    /// Name.
    pub name: String,
    from_scene: Option<Node>,
    to_scene: Option<Node>,
    from_display: Option<Node>,
    to_display: Option<Node>,
}

/// A view of a display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// Name.
    pub name: String,
    /// v1-style / simple view: the target colour space.
    pub colorspace: Option<String>,
    /// v2 view: the view transform …
    pub view_transform: Option<String>,
    /// … and the display colour space (`<USE_DISPLAY_NAME>` resolved by the caller).
    pub display_colorspace: Option<String>,
    /// Looks (not applied yet; non-empty looks make the view unsupported).
    pub looks: Option<String>,
}

/// A display and its views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    /// Name.
    pub name: String,
    /// Views, with shared views resolved.
    pub views: Vec<View>,
}

/// A file rule (`file_rules`): glob pattern + extension → colour space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRule {
    /// Rule name.
    pub name: String,
    /// Colour space (or role) name.
    pub colorspace: String,
    /// Glob on the file name without extension (`*` and `?`).
    pub pattern: Option<String>,
    /// Extension glob (case-insensitive).
    pub extension: Option<String>,
    /// `true` for regex rules, which are skipped.
    pub regex: bool,
}

/// A parsed OCIO config.
#[derive(Debug)]
pub struct Config {
    /// `name`.
    pub name: String,
    /// `ocio_profile_version`.
    pub version: f64,
    /// `description`.
    pub description: String,
    /// Colour spaces (scene and display).
    pub colorspaces: Vec<OcioColorSpace>,
    /// Roles (role → colour space).
    pub roles: Vec<(String, String)>,
    /// View transforms.
    pub view_transforms: Vec<OcioViewTransform>,
    /// `default_view_transform`.
    pub default_view_transform: Option<String>,
    /// Displays.
    pub displays: Vec<Display>,
    /// `active_displays` (empty = all).
    pub active_displays: Vec<String>,
    /// `active_views` (empty = all).
    pub active_views: Vec<String>,
    /// File rules.
    pub file_rules: Vec<FileRule>,
    looks: Vec<(String, String, Option<Node>, Option<Node>)>,
    dir: Option<PathBuf>,
    search_path: Vec<String>,
    lut_cache: Mutex<HashMap<(PathBuf, u8), ColorTransform>>,
}

fn perr(what: impl Into<String>, message: impl Into<String>) -> ColorError {
    ColorError::Parse {
        what: what.into(),
        message: message.into(),
    }
}

fn s(n: Option<&Node>) -> Option<String> {
    n.and_then(|n| n.as_str()).map(str::to_string)
}

fn str_list(n: Option<&Node>) -> Vec<String> {
    match n {
        Some(n) => match n.as_seq() {
            Some(items) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            None => n
                .as_str()
                .map(|v| {
                    v.split([',', ':'])
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        },
        None => Vec::new(),
    }
}

fn first<'a>(n: &'a Node, keys: &[&str]) -> Option<&'a Node> {
    keys.iter().find_map(|k| n.get(k)).filter(|n| !n.is_null())
}

impl Config {
    /// Reads a config file; LUT paths resolve against its directory and `search_path`.
    pub fn load(path: &Path) -> Result<Config, ColorError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ColorError::Io(format!("{}: {e}", path.display())))?;
        let mut cfg = Self::parse(&text)?;
        cfg.dir = path.parent().map(Path::to_path_buf);
        Ok(cfg)
    }

    /// Parses config text (LUT paths then resolve against the current directory
    /// unless [`Config::set_dir`] is called).
    pub fn parse(text: &str) -> Result<Config, ColorError> {
        let doc = yaml::parse(text).map_err(|e| perr("config.ocio", e.to_string()))?;
        if doc.as_map().is_none() {
            return Err(perr("config.ocio", "not a mapping"));
        }
        let version = doc
            .get("ocio_profile_version")
            .and_then(Node::as_f64)
            .unwrap_or(1.0);
        let mut colorspaces = Vec::new();
        for (key, reference) in [
            ("colorspaces", Reference::Scene),
            ("display_colorspaces", Reference::Display),
        ] {
            for cs in doc.get(key).and_then(Node::as_seq).unwrap_or(&[]) {
                let name = s(cs.get("name")).ok_or_else(|| {
                    perr("config.ocio", format!("{key}: colour space without a name"))
                })?;
                let (to_keys, from_keys): (&[&str], &[&str]) = match reference {
                    Reference::Scene => (
                        &["to_scene_reference", "to_reference"],
                        &["from_scene_reference", "from_reference"],
                    ),
                    Reference::Display => (&["to_display_reference"], &["from_display_reference"]),
                };
                colorspaces.push(OcioColorSpace {
                    aliases: str_list(cs.get("aliases")),
                    family: s(cs.get("family")).unwrap_or_default(),
                    encoding: s(cs.get("encoding")).unwrap_or_default(),
                    is_data: cs.get("isdata").and_then(Node::as_bool).unwrap_or(false),
                    reference,
                    description: s(cs.get("description")).unwrap_or_default(),
                    to_ref: first(cs, to_keys).cloned(),
                    from_ref: first(cs, from_keys).cloned(),
                    name,
                });
            }
        }
        let roles = doc
            .get("roles")
            .and_then(Node::as_map)
            .unwrap_or(&[])
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
            .collect();
        let view_transforms = doc
            .get("view_transforms")
            .and_then(Node::as_seq)
            .unwrap_or(&[])
            .iter()
            .map(|vt| OcioViewTransform {
                name: s(vt.get("name")).unwrap_or_default(),
                from_scene: first(vt, &["from_scene_reference", "from_reference"]).cloned(),
                to_scene: first(vt, &["to_scene_reference", "to_reference"]).cloned(),
                from_display: first(vt, &["from_display_reference"]).cloned(),
                to_display: first(vt, &["to_display_reference"]).cloned(),
            })
            .collect();
        let parse_view = |v: &Node| View {
            name: s(v.get("name")).unwrap_or_default(),
            colorspace: s(v.get("colorspace")),
            view_transform: s(v.get("view_transform")),
            display_colorspace: s(v.get("display_colorspace")),
            looks: s(v.get("looks"))
                .or_else(|| s(v.get("look")))
                .filter(|l| !l.is_empty()),
        };
        let shared: Vec<View> = doc
            .get("shared_views")
            .and_then(Node::as_seq)
            .unwrap_or(&[])
            .iter()
            .map(parse_view)
            .collect();
        let mut displays = Vec::new();
        for (name, views) in doc.get("displays").and_then(Node::as_map).unwrap_or(&[]) {
            let mut list = Vec::new();
            for v in views.as_seq().unwrap_or(&[]) {
                if v.tag.as_deref() == Some("Views") {
                    for shared_name in v.as_seq().unwrap_or(&[]).iter().filter_map(Node::as_str) {
                        if let Some(sv) = shared.iter().find(|s| s.name == shared_name) {
                            list.push(sv.clone());
                        }
                    }
                } else {
                    list.push(parse_view(v));
                }
            }
            displays.push(Display {
                name: name.clone(),
                views: list,
            });
        }
        let file_rules = doc
            .get("file_rules")
            .and_then(Node::as_seq)
            .unwrap_or(&[])
            .iter()
            .map(|r| FileRule {
                name: s(r.get("name")).unwrap_or_default(),
                colorspace: s(r.get("colorspace")).unwrap_or_default(),
                pattern: s(r.get("pattern")),
                extension: s(r.get("extension")),
                regex: r.get("regex").is_some(),
            })
            .collect();
        Ok(Config {
            name: s(doc.get("name")).unwrap_or_default(),
            version,
            description: s(doc.get("description")).unwrap_or_default(),
            colorspaces,
            roles,
            view_transforms,
            default_view_transform: s(doc.get("default_view_transform")),
            displays,
            active_displays: str_list(doc.get("active_displays")),
            active_views: str_list(doc.get("active_views")),
            file_rules,
            looks: doc
                .get("looks")
                .and_then(Node::as_seq)
                .unwrap_or(&[])
                .iter()
                .filter_map(|n| {
                    Some((
                        s(n.get("name"))?,
                        s(n.get("process_space"))?,
                        n.get("transform").cloned(),
                        n.get("inverse_transform").cloned(),
                    ))
                })
                .collect(),
            dir: None,
            search_path: str_list(doc.get("search_path")),
            lut_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Named looks in config order.
    pub fn look_names(&self) -> impl Iterator<Item = &str> {
        self.looks.iter().map(|l| l.0.as_str())
    }

    /// Convert to the look's process space, apply its grade, then convert to the output.
    pub fn look_transform(
        &self,
        src: &str,
        name: &str,
        dst: &str,
    ) -> Result<ColorTransform, ColorError> {
        let (_, space, forward, inverse) = self
            .looks
            .iter()
            .find(|l| l.0 == name)
            .ok_or_else(|| perr("look", name))?;
        let op = if let Some(n) = forward {
            self.build(n, false)?
        } else if let Some(n) = inverse {
            self.build(n, true)?
        } else {
            ColorTransform::identity()
        };
        Ok(self
            .processor(src, space)?
            .then(&op)
            .then(&self.processor(space, dst)?))
    }

    /// Sets the directory LUT paths resolve against.
    pub fn set_dir(&mut self, dir: impl Into<PathBuf>) {
        self.dir = Some(dir.into());
    }

    /// Finds a colour space by name, alias or role (case-insensitive, as OCIO).
    pub fn space(&self, name: &str) -> Result<&OcioColorSpace, ColorError> {
        let eq = |a: &str| a.eq_ignore_ascii_case(name);
        if let Some(cs) = self
            .colorspaces
            .iter()
            .find(|c| eq(&c.name) || c.aliases.iter().any(|a| eq(a)))
        {
            return Ok(cs);
        }
        if let Some((_, target)) = self.roles.iter().find(|(r, _)| eq(r))
            && !target.eq_ignore_ascii_case(name)
        {
            return self.space(target);
        }
        Err(ColorError::UnknownSpace(format!("ocio:{name}")))
    }

    fn default_scene_to_display(&self) -> Result<ColorTransform, ColorError> {
        let vt = match &self.default_view_transform {
            Some(name) => self.view_transforms.iter().find(|v| &v.name == name),
            None => self
                .view_transforms
                .iter()
                .find(|v| v.from_scene.is_some() || v.to_scene.is_some()),
        }
        .ok_or_else(|| {
            ColorError::Unsupported("scene ↔ display conversion without a view transform".into())
        })?;
        self.vt_scene_to_display(vt)
    }

    fn vt_scene_to_display(&self, vt: &OcioViewTransform) -> Result<ColorTransform, ColorError> {
        if let Some(t) = &vt.from_scene {
            self.build(t, false)
        } else if let Some(t) = &vt.to_scene {
            self.build(t, true)
        } else if let Some(t) = &vt.from_display {
            // A display → display view transform (e.g. "Video (colorimetric)").
            self.build(t, false)
        } else if let Some(t) = &vt.to_display {
            self.build(t, true)
        } else {
            Ok(ColorTransform::identity())
        }
    }

    /// Colour space → its reference.
    pub fn to_reference(&self, cs: &OcioColorSpace) -> Result<ColorTransform, ColorError> {
        match (&cs.to_ref, &cs.from_ref) {
            (Some(t), _) => self.build(t, false),
            (None, Some(t)) => self.build(t, true),
            (None, None) => Ok(ColorTransform::identity()),
        }
    }

    /// Reference → colour space.
    pub fn from_reference(&self, cs: &OcioColorSpace) -> Result<ColorTransform, ColorError> {
        match (&cs.from_ref, &cs.to_ref) {
            (Some(t), _) => self.build(t, false),
            (None, Some(t)) => self.build(t, true),
            (None, None) => Ok(ColorTransform::identity()),
        }
    }

    /// The transform between two colour spaces of the config (OCIO
    /// `Config::getProcessor(src, dst)`). Scene ↔ display conversions go through
    /// the default view transform, as in OCIO.
    pub fn processor(&self, src: &str, dst: &str) -> Result<ColorTransform, ColorError> {
        // ColorSpaceTransforms recurse through here; stop cycles.
        thread_local!(static DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) });
        let depth = DEPTH.with(|d| {
            d.set(d.get() + 1);
            d.get()
        });
        let result = if depth > 16 {
            Err(perr("config.ocio", "ColorSpaceTransform cycle"))
        } else {
            self.processor_inner(src, dst)
        };
        DEPTH.with(|d| d.set(d.get() - 1));
        result
    }

    fn processor_inner(&self, src: &str, dst: &str) -> Result<ColorTransform, ColorError> {
        let s = self.space(src)?;
        let d = self.space(dst)?;
        if s.is_data || d.is_data {
            return Ok(ColorTransform::identity());
        }
        let mut t = self.to_reference(s)?;
        match (s.reference, d.reference) {
            (Reference::Scene, Reference::Display) => t = t.then(&self.default_scene_to_display()?),
            (Reference::Display, Reference::Scene) => {
                t = t.then(&self.default_scene_to_display()?.inverse()?)
            }
            _ => {}
        }
        Ok(t.then(&self.from_reference(d)?))
    }

    /// The transform from `src` through a display / view (OCIO
    /// `DisplayViewTransform`).
    pub fn display_view(
        &self,
        src: &str,
        display: &str,
        view: &str,
    ) -> Result<ColorTransform, ColorError> {
        let disp = self
            .displays
            .iter()
            .find(|d| d.name == display)
            .ok_or_else(|| ColorError::UnknownSpace(format!("ocio display `{display}`")))?;
        let v = disp
            .views
            .iter()
            .find(|v| v.name == view)
            .ok_or_else(|| ColorError::UnknownSpace(format!("ocio view `{display}/{view}`")))?;
        if v.looks.is_some() {
            return Err(ColorError::Unsupported(format!("looks in view `{view}`")));
        }
        if let Some(cs) = &v.colorspace {
            let cs = if cs == "<USE_DISPLAY_NAME>" {
                display
            } else {
                cs.as_str()
            };
            return self.processor(src, cs);
        }
        let vt_name = v.view_transform.as_deref().unwrap_or_default();
        let vt = self
            .view_transforms
            .iter()
            .find(|t| t.name == vt_name)
            .ok_or_else(|| ColorError::UnknownSpace(format!("ocio view transform `{vt_name}`")))?;
        let dcs_name = match v.display_colorspace.as_deref() {
            Some("<USE_DISPLAY_NAME>") | None => display,
            Some(n) => n,
        };
        let s = self.space(src)?;
        let d = self.space(dcs_name)?;
        if s.is_data {
            return Ok(ColorTransform::identity());
        }
        let mut t = self.to_reference(s)?;
        let scene_vt = vt.from_scene.is_some() || vt.to_scene.is_some();
        if s.reference == Reference::Scene {
            if scene_vt {
                t = t.then(&self.vt_scene_to_display(vt)?);
            } else {
                t = t
                    .then(&self.default_scene_to_display()?)
                    .then(&self.vt_scene_to_display(vt)?);
            }
        } else if scene_vt {
            return Err(ColorError::Unsupported(
                "display-referred source through a scene view transform".into(),
            ));
        } else {
            t = t.then(&self.vt_scene_to_display(vt)?);
        }
        Ok(t.then(&self.from_reference(d)?))
    }

    /// The colour space the file rules assign to `path` (glob rules only;
    /// regex rules are skipped). Returns the `Default` rule's space otherwise.
    pub fn colorspace_for_path(&self, path: &str) -> Option<&str> {
        let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let (stem, ext) = match file.rfind('.') {
            Some(i) => (&file[..i], &file[i + 1..]),
            None => (file, ""),
        };
        for r in &self.file_rules {
            if r.name == "Default" {
                return Some(&r.colorspace);
            }
            if r.regex || (r.pattern.is_none() && r.extension.is_none()) {
                continue;
            }
            let p_ok = r
                .pattern
                .as_deref()
                .is_none_or(|p| glob(p, stem) || glob(p, file));
            let e_ok = r
                .extension
                .as_deref()
                .is_none_or(|e| glob(&e.to_ascii_lowercase(), &ext.to_ascii_lowercase()));
            if p_ok && e_ok {
                return Some(&r.colorspace);
            }
        }
        None
    }

    /// The config colour space `name` as a bae-color [`ColorSpace`] (id
    /// `ocio:<name>`) that reaches the ACES2065-1 reference, so it mixes with
    /// built-in spaces. Scene spaces go through the `aces_interchange` role
    /// (or assume the reference is ACES2065-1); display spaces through
    /// `cie_xyz_d65_interchange` and a Bradford XYZ D65 → ACES matrix.
    pub fn color_space(&self, name: &str) -> Result<ColorSpace, ColorError> {
        let cs = self.space(name)?;
        let id = Cow::Owned(format!("ocio:{}", cs.name));
        let label = Cow::Owned(cs.name.clone());
        if cs.is_data {
            return Ok(ColorSpace {
                id,
                name: label,
                encoding: Encoding::Data,
                kind: ColorSpaceKind::Data,
            });
        }
        let ref_to_aces = self.reference_to_aces(cs.reference)?;
        let to = self.to_reference(cs)?.then(&ref_to_aces);
        let from = match (ref_to_aces.inverse(), self.from_reference(cs)) {
            (Ok(inv), Ok(f)) => Some(inv.then(&f)),
            _ => None,
        };
        let encoding = match cs.encoding.as_str() {
            "scene-linear" | "display-linear" => Encoding::SceneLinear,
            "log" => Encoding::Log,
            "hdr-video" | "edr-video" => Encoding::Hdr,
            "data" => Encoding::Data,
            _ => Encoding::Sdr,
        };
        Ok(ColorSpace {
            id,
            name: label,
            encoding,
            kind: ColorSpaceKind::Transform {
                to_reference: to,
                from_reference: from,
            },
        })
    }

    fn reference_to_aces(&self, reference: Reference) -> Result<ColorTransform, ColorError> {
        let role = |r: &str| {
            self.roles
                .iter()
                .find(|(k, _)| k == r)
                .map(|(_, v)| v.clone())
        };
        match reference {
            Reference::Scene => match role("aces_interchange") {
                Some(name) => self.from_reference(self.space(&name)?),
                None => Ok(ColorTransform::identity()),
            },
            Reference::Display => {
                let name = role("cie_xyz_d65_interchange").ok_or_else(|| {
                    ColorError::Unsupported(
                        "display colour space without a cie_xyz_d65_interchange role".into(),
                    )
                })?;
                let xyz = self.from_reference(self.space(&name)?)?;
                Ok(xyz.then(&bi::cie_xyz_d65().to_reference()))
            }
        }
    }

    /// A resolver for [`crate::WorkingSpace::from_project_attrs`] /
    /// [`crate::interpret::input_transform`]: `ocio:<name>` ids and plain config
    /// names resolve against this config, anything else against the built-ins.
    pub fn resolver(&self) -> impl Fn(&str) -> Option<ColorSpace> + '_ {
        move |id: &str| match id.strip_prefix("ocio:") {
            Some(name) => self.color_space(name).ok(),
            None => bi::by_id(id).or_else(|| self.color_space(id).ok()),
        }
    }

    fn find_file(&self, src: &str) -> Result<PathBuf, ColorError> {
        if src.contains('$') {
            return Err(ColorError::Unsupported(format!(
                "context variables in `{src}`"
            )));
        }
        let p = Path::new(src);
        if p.is_absolute() {
            return Ok(p.to_path_buf());
        }
        let base = self.dir.clone().unwrap_or_default();
        let mut dirs: Vec<PathBuf> = self.search_path.iter().map(|d| base.join(d)).collect();
        dirs.push(base);
        dirs.iter()
            .map(|d| d.join(src))
            .find(|c| c.is_file())
            .ok_or_else(|| ColorError::Io(format!("LUT `{src}` not found in the search path")))
    }

    /// Builds a transform node. `inverse` flips the node's own direction.
    fn build(&self, node: &Node, inverse: bool) -> Result<ColorTransform, ColorError> {
        let mut t = ColorTransform::identity();
        self.build_into(node, inverse, &mut t, 0)?;
        Ok(t)
    }

    fn build_into(
        &self,
        n: &Node,
        inverse: bool,
        out: &mut ColorTransform,
        depth: usize,
    ) -> Result<(), ColorError> {
        if depth > 32 {
            return Err(perr(
                "config.ocio",
                "transforms nested too deeply (ColorSpaceTransform cycle?)",
            ));
        }
        let tag = n.tag.as_deref().unwrap_or("");
        let inv = inverse ^ (n.get("direction").and_then(Node::as_str) == Some("inverse"));
        let f3 = |key: &str, default: f64| -> Result<[f64; 3], ColorError> {
            match n.get(key) {
                None => Ok([default; 3]),
                Some(v) => match (v.as_f64(), v.as_f64_seq()) {
                    (Some(x), _) => Ok([x; 3]),
                    (_, Some(xs)) if xs.len() >= 3 => Ok([xs[0], xs[1], xs[2]]),
                    _ => Err(perr(tag, format!("bad `{key}`"))),
                },
            }
        };
        let neg = |default: NegativeStyle| match n
            .get("style")
            .or(n.get("negative_style"))
            .and_then(Node::as_str)
        {
            Some("mirror") => NegativeStyle::Mirror,
            Some("pass_thru") | Some("passThru") => NegativeStyle::PassThru,
            Some("linear") => NegativeStyle::Linear,
            Some("clamp") | Some("basic") => NegativeStyle::Clamp,
            _ => default,
        };
        let push = |out: &mut ColorTransform, op: ColorOp| -> Result<(), ColorError> {
            out.push(if inv { op.inverse()? } else { op });
            Ok(())
        };
        match tag {
            "GroupTransform" => {
                let children = n.get("children").and_then(Node::as_seq).unwrap_or(&[]);
                if inv {
                    for c in children.iter().rev() {
                        self.build_into(c, true, out, depth + 1)?;
                    }
                } else {
                    for c in children {
                        self.build_into(c, false, out, depth + 1)?;
                    }
                }
            }
            "MatrixTransform" => {
                let m = n.get("matrix").and_then(Node::as_f64_seq);
                let off = n
                    .get("offset")
                    .and_then(Node::as_f64_seq)
                    .unwrap_or_else(|| vec![0.0; 4]);
                let m: Mat3 = match m.as_deref() {
                    Some(v) if v.len() == 16 => {
                        [[v[0], v[1], v[2]], [v[4], v[5], v[6]], [v[8], v[9], v[10]]]
                    }
                    Some(v) if v.len() == 9 => {
                        [[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]]
                    }
                    None => crate::math::IDENTITY,
                    _ => return Err(perr(tag, "matrix must have 16 values")),
                };
                if off.len() < 3 {
                    return Err(perr(tag, "offset must have 4 values"));
                }
                push(
                    out,
                    ColorOp::Matrix(Affine {
                        m,
                        offset: [off[0], off[1], off[2]],
                    }),
                )?;
            }
            "ExponentTransform" => {
                push(
                    out,
                    ColorOp::Power(PowerOp {
                        exponent: f3("value", 1.0)?,
                        negative: neg(NegativeStyle::Clamp),
                    }),
                )?;
            }
            "ExponentWithLinearTransform" => {
                push(
                    out,
                    ColorOp::Moncurve(MoncurveOp {
                        gamma: f3("gamma", 1.0)?,
                        offset: f3("offset", 0.0)?,
                        negative: neg(NegativeStyle::Linear),
                        inverse: false,
                    }),
                )?;
            }
            "LogTransform" | "LogAffineTransform" | "LogCameraTransform" => {
                let base = n.get("base").and_then(Node::as_f64).unwrap_or(2.0);
                let mut op = LogOp::log(base);
                if tag != "LogTransform" {
                    op.log_slope = f3("log_side_slope", 1.0)?;
                    op.log_offset = f3("log_side_offset", 0.0)?;
                    op.lin_slope = f3("lin_side_slope", 1.0)?;
                    op.lin_offset = f3("lin_side_offset", 0.0)?;
                }
                if tag == "LogCameraTransform" {
                    if n.get("lin_side_break").is_none() {
                        return Err(perr(tag, "missing lin_side_break"));
                    }
                    op.lin_break = Some(f3("lin_side_break", 0.0)?);
                    if n.get("linear_slope").is_some() {
                        op.linear_slope = Some(f3("linear_slope", 1.0)?);
                    }
                }
                push(out, ColorOp::Log(op))?;
            }
            "RangeTransform" => {
                let g = |k: &str| n.get(k).and_then(Node::as_f64);
                let (mut a0, mut a1, mut b0, mut b1) = (
                    g("min_in_value"),
                    g("max_in_value"),
                    g("min_out_value"),
                    g("max_out_value"),
                );
                if inv {
                    std::mem::swap(&mut a0, &mut b0);
                    std::mem::swap(&mut a1, &mut b1);
                }
                let clamp = n.get("style").and_then(Node::as_str) != Some("noClamp");
                let (scale, offset) = match (a0, a1, b0, b1) {
                    (Some(a0), Some(a1), Some(b0), Some(b1)) => {
                        let s = (b1 - b0) / (a1 - a0);
                        (s, b0 - a0 * s)
                    }
                    (Some(a0), _, Some(b0), _) => (1.0, b0 - a0),
                    (_, Some(a1), _, Some(b1)) => (1.0, b1 - a1),
                    _ => (1.0, 0.0),
                };
                out.push(ColorOp::Matrix(Affine {
                    m: crate::math::diag([scale; 3]),
                    offset: [offset; 3],
                }));
                if clamp {
                    out.push(ColorOp::Clamp {
                        min: if a0.is_some() {
                            b0.unwrap_or(f64::NEG_INFINITY)
                        } else {
                            f64::NEG_INFINITY
                        },
                        max: if a1.is_some() {
                            b1.unwrap_or(f64::INFINITY)
                        } else {
                            f64::INFINITY
                        },
                    });
                }
            }
            "CDLTransform" => {
                // OCIO 2's default style is noClamp; only `asc` clamps.
                let style = n.get("style").and_then(Node::as_str).unwrap_or("noClamp");
                push(
                    out,
                    ColorOp::Cdl(CdlOp {
                        slope: f3("slope", 1.0)?,
                        offset: f3("offset", 0.0)?,
                        power: f3("power", 1.0)?,
                        saturation: n.get("sat").and_then(Node::as_f64).unwrap_or(1.0),
                        clamp: style.eq_ignore_ascii_case("asc"),
                        inverse: false,
                    }),
                )?;
            }
            "FileTransform" => {
                let src = s(n.get("src")).ok_or_else(|| perr(tag, "missing src"))?;
                let interp = match n.get("interpolation").and_then(Node::as_str) {
                    Some("tetrahedral") | Some("best") => Interpolation::Tetrahedral,
                    _ => Interpolation::Linear,
                };
                let path = self.find_file(&src)?;
                let key = (path.clone(), interp as u8);
                let cached = self
                    .lut_cache
                    .lock()
                    .ok()
                    .and_then(|c| c.get(&key).cloned());
                let t = match cached {
                    Some(t) => t,
                    None => {
                        let t = crate::lutfile::load(&path, interp)?;
                        if let Ok(mut c) = self.lut_cache.lock() {
                            c.insert(key, t.clone());
                        }
                        t
                    }
                };
                let t = if inv { t.inverse()? } else { t };
                *out = out.then(&t);
            }
            "ColorSpaceTransform" => {
                let (a, b) = (
                    s(n.get("src")).unwrap_or_default(),
                    s(n.get("dst")).unwrap_or_default(),
                );
                let (a, b) = if inv { (b, a) } else { (a, b) };
                *out = out.then(&self.processor(&a, &b)?);
            }
            "BuiltinTransform" => {
                let style = s(n.get("style")).ok_or_else(|| perr(tag, "missing style"))?;
                let t = builtin::expand(&style)?;
                let t = if inv { t.inverse()? } else { t };
                *out = out.then(&t);
            }
            "AllocationTransform" => {}
            other => return Err(ColorError::Unsupported(format!("OCIO `{other}`"))),
        }
        Ok(())
    }
}

/// Glob with `*` and `?`.
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Shares a parsed config between threads.
pub type SharedConfig = Arc<Config>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globbing() {
        assert!(glob("*", "anything"));
        assert!(glob("shot_*_v?", "shot_010_v3"));
        assert!(!glob("shot_*_v?", "shot_010_v33"));
        assert!(glob("exr", "exr"));
    }

    const MINI: &str = r#"
ocio_profile_version: 2
roles:
  scene_linear: lin
  aces_interchange: aces
  data: raw
file_rules:
  - !<Rule> {name: EXR, colorspace: lin, pattern: "*", extension: exr}
  - !<Rule> {name: Default, colorspace: srgb}
default_view_transform: plain
view_transforms:
  - !<ViewTransform>
    name: plain
    from_scene_reference: !<MatrixTransform> {}
displays:
  Monitor:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Plain, view_transform: plain, display_colorspace: <USE_DISPLAY_NAME>}
colorspaces:
  - !<ColorSpace>
    name: aces
    isdata: false
  - !<ColorSpace>
    name: lin
    aliases: [linear]
    to_scene_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}
  - !<ColorSpace>
    name: srgb
    from_scene_reference: !<GroupTransform>
      children:
        - !<ColorSpaceTransform> {src: aces, dst: lin}
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}
  - !<ColorSpace>
    name: raw
    isdata: true
display_colorspaces:
  - !<ColorSpace>
    name: Monitor
    from_display_reference: !<ExponentTransform> {value: 2.2, style: mirror, direction: inverse}
"#;

    #[test]
    fn mini_config() {
        let c = Config::parse(MINI).unwrap();
        assert_eq!(c.space("LINEAR").unwrap().name, "lin");
        assert_eq!(c.space("scene_linear").unwrap().name, "lin");
        let t = c.processor("lin", "aces").unwrap();
        assert_eq!(t.apply_f64([0.25; 3]), [0.5; 3]);
        let t = c.processor("aces", "srgb").unwrap();
        let v = t.apply_f64([0.5; 3]);
        assert!((v[0] - crate::TransferFunction::Srgb.encode(0.25)).abs() < 1e-4);
        assert!(c.processor("raw", "srgb").unwrap().is_identity());
        // Scene → display through the default view transform.
        let t = c.processor("aces", "Monitor").unwrap();
        assert!((t.apply_f64([0.25; 3])[0] - 0.25f64.powf(1.0 / 2.2)).abs() < 1e-12);
        let t = c.display_view("lin", "Monitor", "Plain").unwrap();
        assert!((t.apply_f64([0.125; 3])[0] - 0.25f64.powf(1.0 / 2.2)).abs() < 1e-12);
        assert!(
            c.display_view("lin", "Monitor", "Raw")
                .unwrap()
                .is_identity()
        );
        assert_eq!(c.colorspace_for_path("/a/b/plate.0001.EXR"), Some("lin"));
        assert_eq!(c.colorspace_for_path("ref.png"), Some("srgb"));
        // As a bae-color space: lin reaches ACES with the ×2 matrix.
        let lin = c.color_space("lin").unwrap();
        let r = lin.to_reference().apply_f64([0.1; 3]);
        assert!((r[0] - 0.2).abs() < 1e-12);
        let resolve = c.resolver();
        assert!(resolve("ocio:srgb").is_some());
        assert!(resolve("acescg").is_some());
    }
}
