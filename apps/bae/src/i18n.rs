//! UI language for the shell's own strings (menus, About, status bar).
//!
//! Translation lives in `bae_ui::i18n`; this module adds the shell's strings to its
//! table and picks the language from the environment. Keys are the English text;
//! Chinese follows the AE 2026 Simplified Chinese UI (`docs/glossary.md`).

pub use bae_ui::i18n::tr;
use bae_ui::i18n::{self, Lang};

/// Environment variable forcing the UI language: `zh` or `en`.
pub const LANG_ENV: &str = "BAE_LANG";

const ZH: &[(&str, &str)] = &[
    (
        "Composition viewer: render output appears here.",
        "合成查看器：渲染结果显示在这里。",
    ),
    ("Edit", "编辑"),
    ("Exit", "退出"),
    // Edit > Undo / Redo with nothing to undo (AE 26).
    ("Can’t Undo", "无法撤消"),
    ("Can’t Redo", "无法重做"),
    ("File", "文件"),
    ("Help", "帮助"),
    ("Quit BAE", "退出 BAE"),
    ("Window", "窗口"),
    ("Workspace", "工作区"),
    ("Minimize", "最小化"),
    ("Full Screen", "全屏"),
    ("frame {ms} ms", "帧 {ms} 毫秒"),
    // About window
    ("debug", "调试版"),
    ("release", "发布版"),
    ("License: {license}", "许可证：{license}"),
    ("English", "English"),
    // File commands and their dialogs
    ("Open Project", "打开项目"),
    ("Save As", "另存为"),
    ("BAE Project", "BAE 项目"),
    ("BAE Package", "BAE 打包项目"),
    (
        "BAE Project (.baez, project.json)",
        "BAE 项目（.baez、project.json）",
    ),
    ("Format:", "格式："),
    (
        "Save changes to “{}” before closing?",
        "关闭前是否保存对“{}”的更改？",
    ),
    (
        "If you don't save, your changes will be lost.",
        "如果不保存，你的更改将会丢失。",
    ),
    ("Don't Save", "不保存"),
    (
        "“{}” already exists. Do you want to replace it?",
        "“{}”已存在。要替换它吗？",
    ),
    (
        "The existing project is replaced. This cannot be undone.",
        "已有的项目会被替换，无法撤消。",
    ),
    ("Replace", "替换"),
    (
        "“{file}” is a folder but not a BAE project. Choose another name.",
        "“{file}”是一个文件夹，但不是 BAE 项目。请换一个名字。",
    ),
    ("Could not open the project.", "无法打开项目。"),
    // Why a project did not open (open_error)
    ("“{file}” does not exist.", "“{file}”不存在。"),
    (
        "“{file}” is a folder but not a BAE project: it has no project.json.",
        "“{file}”是一个文件夹，但不是 BAE 项目：里面没有 project.json。",
    ),
    (
        "“{file}” is not a BAE project. Open a .bae project folder or a .baez file.",
        "“{file}”不是 BAE 项目。请打开 .bae 项目文件夹或 .baez 文件。",
    ),
    (
        "“{file}” is damaged or is not a .baez project package.",
        "“{file}”已损坏，或者不是 .baez 项目包。",
    ),
    (
        "The project description (project.json) in “{file}” is damaged.",
        "“{file}”里的项目描述（project.json）已损坏。",
    ),
    (
        "“{file}” was saved by a newer version of BAE (format {found}; this version reads up to {supported}). Update BAE to open it.",
        "“{file}”是用更新版本的 BAE 保存的（格式 {found}，当前版本最高支持 {supported}）。请更新 BAE 后再打开。",
    ),
    (
        "“{file}” is incomplete: the data of sublayer “{id}” is missing.",
        "“{file}”不完整：工程层“{id}”的数据缺失。",
    ),
    (
        "The sublayer data in “{file}” is damaged.",
        "“{file}”里的工程层数据已损坏。",
    ),
    (
        "You don't have permission to open “{file}”.",
        "没有权限打开“{file}”。",
    ),
    ("“{file}” cannot be read.", "无法读取“{file}”。"),
    (
        "You don't have permission to save in “{folder}”.",
        "没有权限保存到“{folder}”。",
    ),
    (
        "The folder “{folder}” does not exist.",
        "文件夹“{folder}”不存在。",
    ),
    (
        "You don't have permission to change “{file}”.",
        "没有权限修改“{file}”。",
    ),
    (
        "There is not enough space to save “{file}”.",
        "空间不足，无法保存“{file}”。",
    ),
    ("“{file}” could not be saved.", "无法保存“{file}”。"),
    ("“{file}” could not be opened.", "无法打开“{file}”。"),
    // bae pack
    ("packed", "已打包"),
    ("asset file(s)", "个素材文件"),
    ("warning", "警告"),
    (
        "“{file}” already exists; use --overwrite to replace it.",
        "“{file}”已存在；要替换请加 --overwrite。",
    ),
    (
        "“{file}” would overwrite the project or one of its assets.",
        "“{file}”会覆盖项目本身或它的素材。",
    ),
    (
        "{n} asset file(s) are missing; nothing was written.",
        "缺少 {n} 个素材文件，没有写出任何文件。",
    ),
    ("Could not save the project.", "无法保存项目。"),
    // Status bar: the kind of GPU
    ("Discrete GPU", "独立显卡"),
    ("Integrated GPU", "集成显卡"),
    ("Virtual GPU", "虚拟显卡"),
    ("Software Rendering", "软件渲染"),
    ("Other GPU", "其他显卡"),
    // Status bar: why the last command failed
    (
        "Layer “{name}” is locked. Unlock it first.",
        "图层“{name}”已锁定，请先解锁。",
    ),
    (
        "The last action failed: {reason}",
        "上一个操作失败：{reason}",
    ),
    (
        "GPU-first motion graphics and compositing.",
        "以 GPU 为基础的动态图形与合成应用。",
    ),
];

/// Adds the shell strings to the translation table (without changing the
/// language).
pub fn register_tables() {
    i18n::register(ZH);
}

/// Registers the shell strings and picks the language from `BAE_LANG`, then
/// `LC_ALL` / `LC_MESSAGES` / `LANG`.
pub fn init_from_env() {
    register_tables();
    let locale = std::env::var(LANG_ENV)
        .ok()
        .or_else(|| std::env::var("LC_ALL").ok())
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default();
    let lang = Lang::from_locale(&locale);
    i18n::set_lang(lang);
    // New folders, comps, layers and shape contents are named in it (AE).
    bae_motion::names::set_chinese(lang == Lang::Zh);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_menu_and_dialog_strings_have_chinese() {
        i18n::register(ZH);
        let mut strings: Vec<String> = crate::shell::file_menu_titles();
        strings.extend(
            [
                "File",
                "Exit",
                "Open Project",
                "Save As",
                "BAE Project",
                "BAE Package",
                "BAE Project (.baez, project.json)",
                "Format:",
                "Save changes to “{}” before closing?",
                "If you don't save, your changes will be lost.",
                "Save",
                "Don't Save",
                "Cancel",
                "Could not open the project.",
                "Could not save the project.",
                "Untitled Project",
                "Layer “{name}” is locked. Unlock it first.",
                "The last action failed: {reason}",
            ]
            .map(String::from),
        );
        let missing: Vec<_> = strings
            .iter()
            .filter(|s| !i18n::has_translation(s))
            .collect();
        assert!(missing.is_empty(), "no Chinese for {missing:?}");
    }

    #[test]
    fn every_gpu_kind_has_chinese() {
        use bae_gpu::wgpu::DeviceType;
        i18n::register(ZH);
        for kind in [
            DeviceType::DiscreteGpu,
            DeviceType::IntegratedGpu,
            DeviceType::VirtualGpu,
            DeviceType::Cpu,
            DeviceType::Other,
        ] {
            let en = crate::ui::workspace_gpu_kind(kind);
            assert!(i18n::has_translation(en), "{en}");
        }
    }

    #[test]
    fn a_locked_layer_failure_names_the_layer() {
        let error = bae_model::CommandError::Model(bae_model::ModelError::LayerLocked {
            layer: bae_model::PrimPath::new("/comps/c/layers/a").unwrap(),
            name: "背景".into(),
        });
        let text = crate::ui::failure_notice(&error);
        assert!(text.contains("“背景”") && text.contains("locked"), "{text}");
        let other = crate::ui::failure_notice(&bae_model::CommandError::Failed("nope".into()));
        assert!(other.ends_with("nope"), "{other}");
    }

    #[test]
    fn table_has_unique_keys() {
        let mut keys: Vec<_> = ZH.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len());
    }
}
