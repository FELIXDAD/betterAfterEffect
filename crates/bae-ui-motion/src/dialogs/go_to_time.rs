//! Go to Time: AE's View ▸ Go to Time (Alt+Shift+J). The field starts with the
//! active comp's current time, selected; it takes a timecode, frames, AE's
//! shorthand (`2.` = 2 seconds) or an offset (`+10`, `-15` frames). The text
//! beside it shows what was typed as timecode and the timecode base. OK moves
//! the current time (view state, no undo step).

use bae_model::{PrimPath, Stage, Time};
use bae_motion::query::{self, CompInfo};
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};

use super::fields;
use crate::session::SharedSession;

/// The open Go to Time dialog.
#[derive(Clone, Debug)]
pub struct GoToTime {
    comp: PrimPath,
    /// Current frame when the dialog opened (offsets count from it).
    current: i64,
    pub text: String,
    focused: bool,
}

impl GoToTime {
    pub(crate) fn new(stage: &Stage, session: &SharedSession) -> Option<Self> {
        let s = session.lock();
        let comp = s.active_comp.clone()?;
        let info = query::comp_info(stage, &comp)?;
        let current = info.frame_rate.time_to_frame(s.time(&comp));
        Some(GoToTime {
            text: shown(&info, current),
            comp,
            current,
            focused: false,
        })
    }

    /// The comp frame the text names, if it parses.
    fn target(&self, info: &CompInfo) -> Option<i64> {
        let fps = info.frame_rate.as_f64();
        let start = info.frame_rate.time_to_frame(info.display_start);
        let t = self.text.trim();
        let relative = t.starts_with('+') || t.starts_with('-');
        let n = fields::parse_timecode(t, fps, false)?;
        Some(if relative {
            self.current + n
        } else {
            n - start
        })
    }

    /// Draw the dialog; returns whether it stays open.
    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        cx: &PanelContext<'_>,
        session: &SharedSession,
    ) -> bool {
        let Some(info) = query::comp_info(cx.stage, &self.comp) else {
            return false;
        };
        let id = Id::new("bae-ui-motion.go-to-time");
        let shown = dialog::window(ctx, id, tr("Go to Time"), |ui| self.body(ui, id, &info));
        let outcome = match shown {
            Some(o) => o,
            None if dialog::closed_by_title_bar(ctx, id) => Outcome::Cancel,
            None => Outcome::None,
        };
        match outcome {
            Outcome::None => true,
            Outcome::Cancel => false,
            Outcome::Ok => {
                if let Some(frame) = self.target(&info) {
                    let t: Time = info.frame_rate.frame_to_time(frame);
                    session.lock().set_time(&info, t);
                }
                false
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id, info: &CompInfo) -> Outcome {
        ui.set_width(380.0);
        let target = self.target(info);
        ui.horizontal(|ui| {
            fields::name_field(
                ui,
                id.with("time"),
                &mut self.text,
                120.0,
                tr("Time Input"),
                &mut self.focused,
            );
            let base = super::fields::timecode_base(info.frame_rate.as_f64());
            let is = target.map_or_else(String::new, |f| shown(info, f));
            ui.label(
                tr("is {time}  Base {base}")
                    .replace("{time}", &is)
                    .replace("{base}", &base.to_string()),
            );
        });
        ui.add_space(6.0);
        dialog::buttons(ui, target.is_some())
    }
}

/// `frame` as AE shows it in the comp (its start timecode applied).
fn shown(info: &CompInfo, frame: i64) -> String {
    let start = info.frame_rate.time_to_frame(info.display_start);
    bae_ui::widgets::timecode(frame + start, info.frame_rate.as_f64())
}

/// English strings of Go to Time (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &["Go to Time", "Time Input", "is {time}  Base {base}"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_times_read_like_ae() {
        let mut stage = Stage::new();
        let mut reg = bae_model::CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let author = bae_model::AuthorId::new("user:test").unwrap();
        let out = reg
            .execute(
                &mut stage,
                &author,
                "comp.create",
                serde_json::json!({"width": 64, "height": 64, "frame_rate": 30.0}),
            )
            .unwrap()
            .output;
        let comp = PrimPath::new(out["path"].as_str().unwrap()).unwrap();
        let info = query::comp_info(&stage, &comp).unwrap();
        let mut g = GoToTime {
            comp,
            current: 30,
            text: String::new(),
            focused: true,
        };
        let mut at = |text: &str| {
            g.text = text.into();
            g.target(&info)
        };
        assert_eq!(at("0:00:02:00"), Some(60));
        assert_eq!(at("215"), Some(75), "digits pair up from the right");
        assert_eq!(at("2."), Some(60), "2 seconds");
        assert_eq!(at("+10"), Some(40), "10 frames after the current time");
        assert_eq!(at("-15"), Some(15));
        assert_eq!(at("x"), None);
    }
}
