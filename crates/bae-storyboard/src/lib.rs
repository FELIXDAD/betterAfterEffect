//! BAE storyboard and design domain commands (workstream 13): the Illustrator-style
//! artboards and the storyboard of the design workspace, as `bae-model`
//! [`Command`](bae_model::Command)s.
//!
//! The design UI (`bae-ui-design`), shortcuts, menus, scripts and agents all run
//! these commands, so every operation is one undo step with an author and is
//! available to agents through MCP (ARCHITECTURE §2, rules 1 and 2). This crate has
//! no GPU, window or platform dependencies. **Framework stage:** the data layout and
//! command interface are meant to be built on; drawing tools come later.
//!
//! | Command id | Illustrator / storyboard equivalent |
//! |---|---|
//! | `artboard.create` | Artboards panel > New Artboard; Artboard tool (Shift+O) |
//! | `artboard.update` | Artboard Options (name, size, position) |
//! | `artboard.delete` | Artboards panel > Delete Artboard |
//! | `artboard.rearrange` | Object > Artboards > Rearrange All |
//! | `board.create` | new storyboard (optionally with N empty panels) |
//! | `board.update` | storyboard settings (name, frame rate, frame size) |
//! | `panel.create` | add a panel (auto shot number SH010, SH020 …) |
//! | `panel.update` | edit panel fields |
//! | `panel.move` | drag panels in the grid / strip, or to another storyboard |
//! | `panel.delete` | delete panels (shot numbers are never reused) |
//! | `board.make_animatic` | generate the animatic sequence for the edit workspace |
//! | `panel.promote_to_comp` | turn a panel into a motion comp linked by a Shot |
//!
//! **Layout** (see [`query`]): artboards under `/artboards` (artwork in each
//! artboard's `layers` folder, artboard coordinates), storyboards under `/boards`
//! with their panels in `panels`, shots under `/shots`, animatics under
//! `/sequences`. Panel fields follow `docs/agent-guide/04-storyboard.md`.
//!
//! Register the commands into a [`CommandRegistry`] with [`register_commands`], or
//! add [`StoryboardModule`] to the app: it publishes them in
//! `bae_app::Registry<dyn DynCommand>` (id = command id).

pub mod artboard;
pub mod board;
pub mod query;
pub mod shot;
mod util;

use std::sync::Arc;

use bae_app::{AppBuilder, AppModule};
use bae_model::{CommandRegistry, DynCommand};

pub use artboard::{
    ArtboardLayout, ArtboardOutput, CreateArtboard, DeleteArtboards, RearrangeArtboards,
    UpdateArtboard,
};
pub use board::{
    BoardOutput, CreateBoard, CreatePanel, DeletePanels, MovePanels, PanelFields, PanelOutput,
    PanelsOutput, UpdateBoard, UpdatePanel,
};
pub use query::{ArtboardInfo, BoardInfo, PanelInfo, ShotSize};
pub use shot::{AnimaticOutput, MakeAnimatic, PromoteOutput, PromotePanel};

/// Command ids, for menus, toolbars and key bindings.
pub mod ids {
    pub const ARTBOARD_CREATE: &str = "artboard.create";
    pub const ARTBOARD_UPDATE: &str = "artboard.update";
    pub const ARTBOARD_DELETE: &str = "artboard.delete";
    pub const ARTBOARD_REARRANGE: &str = "artboard.rearrange";
    pub const BOARD_CREATE: &str = "board.create";
    pub const BOARD_UPDATE: &str = "board.update";
    pub const PANEL_CREATE: &str = "panel.create";
    pub const PANEL_UPDATE: &str = "panel.update";
    pub const PANEL_MOVE: &str = "panel.move";
    pub const PANEL_DELETE: &str = "panel.delete";
    pub const BOARD_MAKE_ANIMATIC: &str = "board.make_animatic";
    pub const PANEL_PROMOTE_TO_COMP: &str = "panel.promote_to_comp";
}

/// Register every storyboard command.
pub fn register_commands(r: &mut CommandRegistry) {
    let results = [
        r.register(CreateArtboard),
        r.register(UpdateArtboard),
        r.register(DeleteArtboards),
        r.register(RearrangeArtboards),
        r.register(CreateBoard),
        r.register(UpdateBoard),
        r.register(CreatePanel),
        r.register(UpdatePanel),
        r.register(MovePanels),
        r.register(DeletePanels),
        r.register(MakeAnimatic),
        r.register(PromotePanel),
    ];
    for res in results {
        res.expect("storyboard command IDs are unique");
    }
}

/// The storyboard commands as type-erased commands, in registration order.
pub fn commands() -> Vec<Arc<dyn DynCommand>> {
    let mut r = CommandRegistry::new();
    register_commands(&mut r);
    let ids: Vec<String> = r.infos().map(|i| i.id.clone()).collect();
    ids.iter().filter_map(|id| r.get(id).cloned()).collect()
}

/// App module: publishes the storyboard commands in `Registry<dyn DynCommand>`.
#[derive(Debug, Default)]
pub struct StoryboardModule;

impl AppModule for StoryboardModule {
    fn name(&self) -> &str {
        "bae-storyboard"
    }

    fn build(&self, app: &mut AppBuilder) {
        let owner = app.current_owner();
        let registry = app.registry::<dyn DynCommand>();
        registry.batch(|tx| {
            for cmd in commands() {
                let id = cmd.info().id.clone();
                if !tx.contains(&id) {
                    let _ = tx.register_arc(owner.clone(), id, cmd);
                }
            }
        });
    }
}
