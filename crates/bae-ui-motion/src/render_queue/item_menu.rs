//! Render-item commands and the log written by the render worker.
use super::*;
use std::io::{BufWriter, Write};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum LogMode {
    #[default]
    Errors,
    Settings,
    Frames,
}

impl LogMode {
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Errors, "Errors Only"),
        (Self::Settings, "Plus Settings"),
        (Self::Frames, "Plus Per Frame Info"),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ItemAction {
    Duplicate,
    DuplicateWithName,
    AddOutput,
    Reveal,
}

impl ItemAction {
    pub const ALL: [(Self, &'static str); 4] = [
        (Self::Duplicate, "Duplicate (Render Item)"),
        (Self::DuplicateWithName, "Duplicate With File Name"),
        (Self::AddOutput, "Add Output Module"),
        (Self::Reveal, "Reveal Composition in Project"),
    ];

    pub fn action_id(self) -> &'static str {
        match self {
            Self::Duplicate | Self::DuplicateWithName => crate::actions::ids::DUPLICATE,
            Self::AddOutput => "motion.comp.add_output_module",
            Self::Reveal => "motion.layer.reveal_composition_in_project",
        }
    }

    pub(super) fn enabled(self, rendering: bool, has_comp: bool) -> bool {
        match self {
            Self::Reveal => has_comp,
            _ => !rendering,
        }
    }

    pub fn menu(ui: &mut egui::Ui, rendering: bool, has_comp: bool) -> Option<Self> {
        let mut chosen = None;
        for (action, title) in Self::ALL {
            let enabled = action.enabled(rendering, has_comp);
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(tr(title).trim_end_matches(" (Render Item)")),
                )
                .clicked()
            {
                chosen = Some(action);
                ui.close();
            }
        }
        chosen
    }
}

pub(super) struct RenderLog {
    file: Option<(PathBuf, BufWriter<std::fs::File>)>,
    previous: Instant,
}

impl RenderLog {
    fn create(job: &Job) -> std::io::Result<(PathBuf, BufWriter<std::fs::File>)> {
        let output = Path::new(&job.outputs[0].output);
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| std::io::Error::other("missing output directory"))?;
        let dir = parent.join("Logs");
        std::fs::create_dir_all(&dir)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        for suffix in 0.. {
            let path = dir.join(format!(
                "{}-{stamp}-{suffix}.txt",
                sanitize(job.comp.as_str())
            ));
            match std::fs::File::create_new(&path) {
                Ok(file) => return Ok((path, BufWriter::new(file))),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    pub fn new(job: &Job) -> Self {
        let mut log = Self {
            file: None,
            previous: Instant::now(),
        };
        if job.log != LogMode::Errors {
            match Self::create(job) {
                Ok((path, mut out)) => {
                    let _ = writeln!(
                        out,
                        "{} · {}–{} · {:.3} fps",
                        job.comp,
                        job.start,
                        job.end,
                        job.rate.as_f64()
                    );
                    for m in &job.outputs {
                        let _ = writeln!(
                            out,
                            "{} · {} · {}",
                            m.output,
                            format_label(m.format),
                            channels_label(m.channels)
                        );
                    }
                    log.file = Some((path, out));
                }
                Err(e) => tracing::warn!("Could not create render log: {e}"),
            }
        }
        log
    }

    pub fn progress(&mut self, job: &Job, done: usize, total: usize) {
        if job.log == LogMode::Frames
            && done > 0
            && let Some((_, out)) = &mut self.file
        {
            let count = (total / job.outputs.len()).max(1);
            let frame = job.start.max(0) + ((done - 1) % count) as i64;
            let _ = writeln!(
                out,
                "{} · {} · {:.3}s",
                (done - 1) / count + 1,
                frame,
                self.previous.elapsed().as_secs_f64()
            );
            self.previous = Instant::now();
        }
    }

    pub fn finish(mut self, job: &Job, status: &ItemStatus) -> Option<PathBuf> {
        if self.file.is_none() && matches!(status, ItemStatus::Failed(_)) {
            self.file = Self::create(job).ok();
        }
        let (path, mut out) = self.file?;
        writeln!(out, "{status:?}").and_then(|_| out.flush()).ok()?;
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_item_and_log_labels_and_availability() {
        bae_ui::i18n::register(crate::catalog::menu_zh());
        bae_ui::i18n::register(super::super::ZH);
        let zh = |key| bae_ui::i18n::translation(key).unwrap();
        assert_eq!(
            ItemAction::ALL.map(|(a, key)| (
                zh(key),
                a.enabled(false, true),
                a.enabled(true, false)
            )),
            [
                ("复制", true, false),
                ("与文件名一起复制", true, false),
                ("添加输出模块", true, false),
                ("在项目中显示合成", true, false)
            ]
        );
        assert!(ItemAction::Reveal.enabled(true, true));
        assert_eq!(
            LogMode::ALL.map(|(mode, key)| (zh(key), mode == LogMode::default())),
            [
                ("仅错误", true),
                ("增加设置", false),
                ("增加每帧信息", false)
            ]
        );
    }
}
