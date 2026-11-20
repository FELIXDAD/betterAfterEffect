//! Panels and panel registration.
//!
//! A feature crate registers a [`PanelKind`] in its `AppModule::build`:
//!
//! ```
//! use bae_app::{AppBuilder, AppModule};
//! use bae_ui::{Panel, PanelContext, PanelKind};
//!
//! #[derive(Default)]
//! struct Timeline { zoom: f32 }
//!
//! impl Panel for Timeline {
//!     fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
//!         ui.label(format!("{} sublayers", cx.stage.layer_stack().len()));
//!         if ui.button("Add solid").clicked() {
//!             // Data changes go through bae-model commands, run after this frame.
//!             cx.invoke_with("prim.create", serde_json::json!({ "path": "/Solid" }));
//!         }
//!     }
//! }
//!
//! struct MotionUi;
//! impl AppModule for MotionUi {
//!     fn build(&self, app: &mut AppBuilder) {
//!         app.register("timeline", PanelKind::new("Timeline", |_| Box::new(Timeline::default())).multi_instance())
//!             .expect("unique panel id");
//!     }
//! }
//! # let mut b = AppBuilder::new();
//! # b.add_module(MotionUi);
//! # let mut app = b.build();
//! # assert!(app.registry::<PanelKind>().contains("timeline"));
//! ```
//!
//! The registry id is the panel **kind** (`"timeline"`). An open panel is a
//! [`PanelId`]: the kind, or `kind:instance` for kinds that can be opened several
//! times (`"timeline:comp-1"`). Workspace layouts store panel ids.
//!
//! Panels only display and interact. They read the composed [`Stage`] and request
//! changes with [`PanelContext::invoke`]; the shell runs the request through the
//! command registry after the frame, so every change is a command (one undo step,
//! with author) and nothing a panel does is out of reach of agents and scripts.

use bae_app::Resources;
use bae_model::{AuthorId, Stage};
use egui::Ui;

use crate::action::Invocation;
use crate::dock::{PanelId, TabInfo};
use crate::i18n::tr;

/// The contents of one open panel. Lives on the UI thread (no `Send` bound).
pub trait Panel: 'static {
    /// What the tab shows (title, label color, lock). Default: the kind's title.
    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr(cx.kind_title))
    }

    /// Draw the panel contents. The `ui` is clipped to the panel.
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>);

    /// Panel-specific items appended to the panel's ☰ menu.
    fn menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let _ = (ui, cx);
    }

    /// Handle a command routed to this panel while it is focused (view actions such
    /// as zoom). Return `true` if handled; otherwise the shell tries its own
    /// actions and then the model command registry.
    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let _ = (action, cx);
        false
    }

    /// The tab's lock toggle was clicked (viewers; see [`TabInfo::locked`]).
    fn set_locked(&mut self, locked: bool) {
        let _ = locked;
    }

    /// View state saved with the workspace layout and passed back to the factory
    /// ([`PanelSpawn::state`]) when the panel is recreated.
    fn save_state(&self) -> Option<serde_json::Value> {
        None
    }

    /// The panel was closed and is about to be dropped.
    fn on_close(&mut self, cx: &mut PanelContext<'_>) {
        let _ = cx;
    }
}

/// Creates panel instances of one kind.
pub type PanelFactory = dyn Fn(&PanelSpawn<'_>) -> Box<dyn Panel> + Send + Sync;

/// A registered panel kind (`bae_app::Registry<PanelKind>`, id = kind).
pub struct PanelKind {
    /// English title; displayed through [`crate::i18n::tr`].
    pub title: String,
    /// Several instances may be open at once (`kind:instance` ids).
    pub multi_instance: bool,
    /// Listed in the Window menu (false for panels opened only by other panels).
    pub in_window_menu: bool,
    factory: Box<PanelFactory>,
}

impl PanelKind {
    pub fn new(
        title: impl Into<String>,
        factory: impl Fn(&PanelSpawn<'_>) -> Box<dyn Panel> + Send + Sync + 'static,
    ) -> Self {
        PanelKind {
            title: title.into(),
            multi_instance: false,
            in_window_menu: true,
            factory: Box::new(factory),
        }
    }

    pub fn multi_instance(mut self) -> Self {
        self.multi_instance = true;
        self
    }

    pub fn hidden_from_window_menu(mut self) -> Self {
        self.in_window_menu = false;
        self
    }

    /// A placeholder kind showing "Not implemented yet." (used for workspace panels
    /// whose owning workstream has not registered them yet).
    pub fn placeholder(title: impl Into<String>) -> Self {
        PanelKind::new(title, |_| Box::new(Placeholder))
    }

    pub fn create(&self, spawn: &PanelSpawn<'_>) -> Box<dyn Panel> {
        (self.factory)(spawn)
    }
}

impl std::fmt::Debug for PanelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanelKind")
            .field("title", &self.title)
            .field("multi_instance", &self.multi_instance)
            .finish_non_exhaustive()
    }
}

/// What a factory gets when a panel is opened.
pub struct PanelSpawn<'a> {
    pub id: &'a PanelId,
    /// State saved by [`Panel::save_state`] when the layout was stored.
    pub state: Option<&'a serde_json::Value>,
    pub resources: &'a Resources,
}

impl PanelSpawn<'_> {
    /// The instance part of `kind:instance`, if any.
    pub fn instance(&self) -> Option<&str> {
        self.id.instance()
    }
}

/// Requests a panel made during a frame; the workbench applies them afterwards.
#[derive(Debug, Default)]
pub struct PanelRequests {
    pub invocations: Vec<Invocation>,
    pub open: Vec<PanelId>,
    pub close: Vec<PanelId>,
}

impl PanelRequests {
    pub fn is_empty(&self) -> bool {
        self.invocations.is_empty() && self.open.is_empty() && self.close.is_empty()
    }
}

/// What a panel can see and do during a frame.
pub struct PanelContext<'a> {
    /// The panel being drawn.
    pub panel: &'a PanelId,
    /// English title of its kind.
    pub kind_title: &'a str,
    /// Whether this panel is the active panel of the focused frame (AE's blue box).
    pub focused: bool,
    /// The open project's composed stage (read-only; change it with [`Self::invoke`]).
    pub stage: &'a Stage,
    /// The local user; undo/redo and new commands are attributed to it.
    pub author: &'a AuthorId,
    /// App resources (`Arc<GpuContext>`, `Arc<ViewportTextureRegistry>`, services
    /// other modules inserted …).
    pub resources: &'a Resources,
    requests: &'a mut PanelRequests,
}

impl<'a> PanelContext<'a> {
    pub fn new(
        panel: &'a PanelId,
        kind_title: &'a str,
        stage: &'a Stage,
        author: &'a AuthorId,
        resources: &'a Resources,
        requests: &'a mut PanelRequests,
    ) -> Self {
        PanelContext {
            panel,
            kind_title,
            focused: false,
            stage,
            author,
            resources,
            requests,
        }
    }

    /// Run a command id without parameters after this frame.
    pub fn invoke(&mut self, id: &str) {
        self.requests.invocations.push(Invocation::new(id));
    }

    /// Run a command id with JSON parameters after this frame.
    pub fn invoke_with(&mut self, id: &str, params: serde_json::Value) {
        self.requests
            .invocations
            .push(Invocation::with_params(id, params));
    }

    /// Open (or focus) a panel after this frame.
    pub fn open_panel(&mut self, id: impl Into<PanelId>) {
        self.requests.open.push(id.into());
    }

    /// Close this panel after this frame.
    pub fn close(&mut self) {
        self.requests.close.push(self.panel.clone());
    }

    /// Short-lived access to the request queue for nested contexts.
    pub fn reborrow(&mut self) -> PanelContext<'_> {
        PanelContext {
            panel: self.panel,
            kind_title: self.kind_title,
            focused: self.focused,
            stage: self.stage,
            author: self.author,
            resources: self.resources,
            requests: self.requests,
        }
    }
}

struct Placeholder;

impl Panel for Placeholder {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
        // Short panels (Preview's 52pt strip) stay empty instead of clipping text.
        if ui.available_height() >= 40.0 {
            ui.centered_and_justified(|ui| {
                ui.weak(tr("Not implemented yet."));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(u32);

    impl Panel for Counter {
        fn ui(&mut self, _ui: &mut Ui, cx: &mut PanelContext<'_>) {
            self.0 += 1;
            cx.invoke_with("prim.create", serde_json::json!({ "path": "/A" }));
        }

        fn on_action(&mut self, action: &Invocation, _cx: &mut PanelContext<'_>) -> bool {
            action.id == "counter.reset" && {
                self.0 = 0;
                true
            }
        }

        fn save_state(&self) -> Option<serde_json::Value> {
            Some(self.0.into())
        }
    }

    #[test]
    fn factory_gets_instance_and_state() {
        let kind = PanelKind::new("Counter", |spawn| {
            let start = spawn.state.and_then(|s| s.as_u64()).unwrap_or(0) as u32;
            assert_eq!(spawn.instance(), Some("b"));
            Box::new(Counter(start))
        })
        .multi_instance();
        let resources = Resources::new();
        let id = PanelId::new("counter:b");
        let state = serde_json::json!(41);
        let mut panel = kind.create(&PanelSpawn {
            id: &id,
            state: Some(&state),
            resources: &resources,
        });
        let stage = Stage::new();
        let author = AuthorId::system();
        let mut requests = PanelRequests::default();
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut cx =
                PanelContext::new(&id, &kind.title, &stage, &author, &resources, &mut requests);
            assert_eq!(panel.tab(&cx).title, "Counter");
            panel.ui(ui, &mut cx);
            assert!(panel.on_action(&Invocation::new("counter.reset"), &mut cx));
            assert!(!panel.on_action(&Invocation::new("other"), &mut cx));
        });
        out.textures_delta.clear();
        assert_eq!(panel.save_state(), Some(serde_json::json!(0)));
        assert_eq!(requests.invocations.len(), 1);
        assert_eq!(requests.invocations[0].id, "prim.create");
    }
}
