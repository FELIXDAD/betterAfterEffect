//! Which compositions were open when the project was saved, so opening it shows
//! them again (AE reopens the compositions whose Timeline panels were open, at
//! their current times).
//!
//! The motion workspace keeps this as session state (`bae_ui_motion::Session`:
//! open comps, active comp, current time per comp). On save it is written into
//! `project.json` under `"view"` (the manifest keeps fields it does not know, so
//! older builds preserve it); it is not project data, so it does not go through
//! a command and does not mark the project as changed. On open it is restored;
//! a project without a record (or whose recorded comps are gone) opens its first
//! composition.
//!
//! The shell and the acceptance harness both run file commands through
//! [`run_file_command`] / [`confirm_quit`], so they cannot drift apart.

use bae_app::App;
use bae_model::{PrimPath, Stage, Time};
use bae_motion::query;
use bae_ui::Invocation;
use bae_ui_motion::SharedSession;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::files::{self, FileDialogs, FileOutcome};

/// `project.json` key of the saved view.
pub const MANIFEST_KEY: &str = "view";

/// The saved view.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewState {
    /// Open compositions (Timeline tabs) in tab order, with their current times.
    #[serde(default)]
    pub comps: Vec<OpenComp>,
    /// The composition shown in the viewer and the Timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<PrimPath>,
}

/// One open composition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpenComp {
    pub path: PrimPath,
    /// Current time (comp time, flicks).
    #[serde(default)]
    pub time: Time,
}

/// The motion workspace's view now (`None` without a motion session).
pub fn capture(app: &App) -> Option<ViewState> {
    let session = app.resource::<SharedSession>()?;
    let s = session.lock();
    Some(ViewState {
        comps: s
            .open_comps
            .iter()
            .map(|c| OpenComp {
                path: c.clone(),
                time: s.time(c),
            })
            .collect(),
        active: s.active_comp.clone(),
    })
}

/// A fresh motion session for the open project: the saved view where its comps
/// still exist, otherwise the project's first composition (none for an empty
/// project).
pub fn restore(app: &mut App, doc: &Document) {
    let stage = doc.stage();
    let saved = doc
        .manifest_field(MANIFEST_KEY)
        .and_then(|v| serde_json::from_value::<ViewState>(v.clone()).ok());
    let session = SharedSession::default();
    {
        let mut s = session.lock();
        let mut opened = false;
        for comp in saved.iter().flat_map(|v| &v.comps) {
            if let Some(info) = query::comp_info(stage, &comp.path) {
                s.open_comp(&comp.path);
                s.set_time(&info, comp.time);
                opened = true;
            }
        }
        match saved.as_ref().and_then(|v| v.active.clone()) {
            Some(active) if opened && s.open_comps.contains(&active) => s.open_comp(&active),
            _ if opened => {}
            _ => {
                if let Some(first) = first_comp(stage) {
                    s.open_comp(&first);
                }
            }
        }
    }
    app.insert_resource(session);
}

fn first_comp(stage: &Stage) -> Option<PrimPath> {
    query::comps(stage).into_iter().next()
}

/// Run a file command with the view saved into the project and, when another
/// project is open afterwards, restored from it.
pub fn run_file_command(
    app: &mut App,
    doc: &mut Document,
    dialogs: &mut dyn FileDialogs,
    inv: &Invocation,
) -> Option<FileOutcome> {
    remember(app, doc);
    let outcome = files::run(doc, dialogs, inv);
    if outcome == Some(FileOutcome::Replaced) {
        restore(app, doc);
        // The render queue belongs to the project (AE, #231).
        if let Some(queue) = app.resource::<bae_ui_motion::SharedRenderQueue>() {
            queue.lock().clear();
        }
    }
    outcome
}

/// The unsaved-changes question before quitting (a Save there keeps the view).
pub fn confirm_quit(app: &App, doc: &mut Document, dialogs: &mut dyn FileDialogs) -> bool {
    remember(app, doc);
    files::confirm_discard(doc, dialogs)
}

fn remember(app: &App, doc: &mut Document) {
    if let Some(view) = capture(app) {
        doc.set_manifest_field(MANIFEST_KEY, serde_json::to_value(view).ok());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::document::Outcome;

    fn app_and_doc() -> (App, Document) {
        let mut builder = bae_app::AppBuilder::new();
        builder.add_module(bae_ui_motion::MotionUiModule);
        let app = builder.build();
        let mut doc = Document::new(None);
        doc.add_module_commands(&app);
        (app, doc)
    }

    fn run(doc: &mut Document, id: &str, params: serde_json::Value) {
        assert!(matches!(
            doc.run(&Invocation::with_params(id, params)),
            Outcome::Changed
        ));
    }

    fn active(app: &App) -> Option<String> {
        let s = app.resource::<SharedSession>().unwrap().lock();
        s.active_comp.as_ref().map(ToString::to_string)
    }

    #[test]
    fn saved_comps_and_times_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("View.bae");
        let (app, mut doc) = app_and_doc();
        for name in ["A", "B", "C"] {
            run(&mut doc, "comp.create", json!({"name": name, "fps": 30}));
        }
        let (a, b): (PrimPath, PrimPath) =
            ("/comps/A".parse().unwrap(), "/comps/B".parse().unwrap());
        {
            let session = app.resource::<SharedSession>().unwrap().clone();
            let mut s = session.lock();
            let info = query::comp_info(doc.stage(), &b).unwrap();
            s.open_comp(&b);
            s.set_time(&info, Time(Time::FLICKS_PER_SECOND));
            s.open_comp(&a);
        }
        remember(&app, &mut doc);
        doc.save_as(&path).unwrap();
        assert!(!doc.is_dirty(), "the view is not project data");

        let (mut app2, mut doc2) = app_and_doc();
        doc2.open(&path).unwrap();
        restore(&mut app2, &doc2);
        let s = app2.resource::<SharedSession>().unwrap().lock();
        assert_eq!(s.open_comps, [b.clone(), a.clone()]);
        assert_eq!(s.active_comp.as_ref(), Some(&a));
        assert_eq!(s.time(&b), Time(Time::FLICKS_PER_SECOND));
    }

    /// Issue #131: `.bae` → `bae pack` → `.baez` → open: the same comps, tab
    /// order, active comp and current times.
    #[test]
    fn packing_keeps_the_open_comps() {
        let dir = tempfile::tempdir().unwrap();
        let bae = dir.path().join("打包 视图.bae");
        let baez = dir.path().join("打包 视图.baez");
        let (app, mut doc) = app_and_doc();
        for name in ["A", "B", "C"] {
            run(&mut doc, "comp.create", json!({"name": name, "fps": 30}));
        }
        let comp = |n: &str| -> PrimPath { format!("/comps/{n}").parse().unwrap() };
        let sec = Time::FLICKS_PER_SECOND;
        {
            let session = app.resource::<SharedSession>().unwrap().clone();
            let mut s = session.lock();
            for (name, t) in [("B", sec), ("C", 2 * sec), ("A", 0)] {
                let info = query::comp_info(doc.stage(), &comp(name)).unwrap();
                s.open_comp(&comp(name));
                s.set_time(&info, Time(t));
            }
            s.open_comp(&comp("C"));
        }
        remember(&app, &mut doc);
        doc.save_as(&bae).unwrap();
        crate::pack_cli::pack(
            &bae,
            &baez,
            &bae_package::PackOptions::default(),
            bae_package::WriteOptions::default(),
        )
        .unwrap();

        let (mut app2, mut doc2) = app_and_doc();
        doc2.open(&baez).unwrap();
        restore(&mut app2, &doc2);
        let s = app2.resource::<SharedSession>().unwrap().lock();
        assert_eq!(s.open_comps, [comp("B"), comp("C"), comp("A")], "tab order");
        assert_eq!(s.active_comp.as_ref(), Some(&comp("C")));
        assert_eq!(s.time(&comp("B")), Time(sec));
        assert_eq!(s.time(&comp("C")), Time(2 * sec));
        assert_eq!(s.time(&comp("A")), Time(0));
    }

    #[test]
    fn no_record_or_missing_comps_open_the_first_comp() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, mut doc) = app_and_doc();
        // Empty project: nothing to open.
        restore(&mut app, &doc);
        assert_eq!(active(&app), None);
        run(&mut doc, "comp.create", json!({"name": "First", "fps": 30}));
        run(
            &mut doc,
            "comp.create",
            json!({"name": "Second", "fps": 30}),
        );
        // Saved without a view (e.g. by an agent or an older build).
        let path = dir.path().join("Plain.bae");
        doc.save_as(&path).unwrap();
        let (mut app2, mut doc2) = app_and_doc();
        doc2.open(&path).unwrap();
        restore(&mut app2, &doc2);
        assert_eq!(active(&app2).as_deref(), Some("/comps/First"));
        // A view naming a comp that no longer exists.
        doc.set_manifest_field(
            MANIFEST_KEY,
            Some(json!({"comps": [{"path": "/comps/Gone", "time": 0}], "active": "/comps/Gone"})),
        );
        doc.save().unwrap();
        doc2.open(&path).unwrap();
        restore(&mut app2, &doc2);
        assert_eq!(active(&app2).as_deref(), Some("/comps/First"));
        // A malformed view is ignored the same way.
        doc.set_manifest_field(MANIFEST_KEY, Some(json!("not a view")));
        doc.save().unwrap();
        doc2.open(&path).unwrap();
        restore(&mut app2, &doc2);
        assert_eq!(active(&app2).as_deref(), Some("/comps/First"));
    }
}
