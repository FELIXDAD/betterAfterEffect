//! Import diagnostics. Decode requested sequence frames sequentially off the UI thread.
use super::{Probed, formats};
use crate::dialogs::{Dialog, SharedDialogs};
use bae_ui::{PanelContext, tr};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) struct FormatWarmUp;
impl bae_gpu::WarmUp for FormatWarmUp {
    fn name(&self) -> &str {
        "import format registry"
    }
    fn run(&self, _: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        formats();
        Ok(())
    }
}

/// Compact contiguous frame numbers so a long missing range fits in the alert.
pub(super) fn ranges(numbers: &[i64]) -> String {
    let mut parts = Vec::new();
    let mut numbers = numbers.iter().copied().peekable();
    while let Some(start) = numbers.next() {
        let mut end = start;
        while let Some(n) = numbers.next_if(|n| n.checked_sub(1) == Some(end)) {
            end = n;
        }
        parts.push(if end == start {
            start.to_string()
        } else {
            format!("{start}–{end}")
        });
    }
    parts.join(", ")
}

#[derive(Default, Debug)]
pub(crate) struct FrameChecks {
    pending: VecDeque<(String, PathBuf, String)>,
    notices: VecDeque<(String, String)>,
    running: bool,
}

impl FrameChecks {
    pub(crate) fn next_notice(&mut self) -> Option<Dialog> {
        self.notices
            .pop_front()
            .map(|(title, message)| Dialog::Alert { title, message })
    }

    pub(crate) fn repaint_while_running(&self, ctx: &egui::Context) {
        if self.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

pub(super) fn report(cx: &PanelContext<'_>, title: &str, mut result: Probed) {
    if !result.failed.is_empty() {
        result.notices.insert(
            0,
            format!(
                "{}\n{}",
                tr("These files could not be imported because their format is not supported:"),
                result.failed.join("\n")
            ),
        );
    }
    if result.notices.is_empty() && result.sequences_to_verify.is_empty() {
        return;
    }
    let dialogs = crate::dialogs::shared(cx.resources);
    let mut d = dialogs.lock();
    if !result.notices.is_empty() {
        d.frame_checks
            .notices
            .push_back((title.to_owned(), result.notices.join("\n")));
        if d.open.is_none() {
            d.open = d.frame_checks.next_notice();
        }
    }
    d.frame_checks.pending.extend(
        result
            .sequences_to_verify
            .into_iter()
            .map(|(path, name)| (title.to_owned(), path, name)),
    );
    if d.frame_checks.running || d.frame_checks.pending.is_empty() {
        return;
    }
    d.frame_checks.running = true;
    drop(d);
    let worker = dialogs.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("bae-verify-sequences".into())
        .spawn(move || run(&worker))
    {
        let mut d = dialogs.lock();
        d.frame_checks.running = false;
        d.frame_checks.pending.clear();
        d.frame_checks.notices.push_back((
            title.to_owned(),
            format!("{} {error}", tr("Could not verify the image sequence.")),
        ));
    }
}

fn run(dialogs: &SharedDialogs) {
    bae_gpu::warm_up::lower_priority();
    loop {
        let (title, path, name) = {
            let mut d = dialogs.lock();
            let Some(job) = d.frame_checks.pending.pop_front() else {
                d.frame_checks.running = false;
                return;
            };
            job
        };
        if let Some(message) = verify(&path, &name) {
            dialogs
                .lock()
                .frame_checks
                .notices
                .push_back((title, message));
        }
    }
}

fn verify(path: &Path, name: &str) -> Option<String> {
    let Ok(Some(seq)) = bae_media::sequence::detect_sequence(path) else {
        return Some(format!(
            "{} — {}",
            path.display(),
            tr("Could not verify the image sequence.")
        ));
    };
    let bad: Vec<_> = seq
        .frames
        .iter()
        .copied()
        .filter(|&n| {
            let path = seq.path_for(n);
            formats()
                .open(&path, &Default::default())
                .and_then(|mut s| s.decode_cpu(&bae_media::FrameRequest::new(0)))
                .is_err()
        })
        .collect();
    (!bad.is_empty()).then(|| format!("{name} — {}: {}", tr("Unreadable frames"), ranges(&bad)))
}
