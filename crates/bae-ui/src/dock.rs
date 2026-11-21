//! Docking: a tree of splitters and panel frames, shaped like AE's workspace files.
//!
//! A [`Node::Split`] lays its children out in a row or a column with a 4pt
//! black gap; the gaps are the drag handles. A [`Frame`] holds a group of
//! panels shown either as tabs or, when `stacked`, as a column of headers with
//! one panel expanded (AE's stacked panel groups). Panels are identified by a
//! [`PanelId`]; their titles and contents come from a [`PanelHost`].
//!
//! Tabs can be dragged onto another frame (center = add as a tab, edge =
//! split), or out of the window to float. The serializable part of the dock is
//! [`Layout`], which is what workspaces save and restore.

use egui::{
    Color32, CursorIcon, Id, Key, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    Vec2, ViewportBuilder, ViewportClass, ViewportId, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use crate::a11y::Accessible as _;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::theme::{self, color, metrics};

/// Stable panel identifier, e.g. `"project"` or `"timeline:main"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PanelId(pub String);

impl PanelId {
    pub fn new(id: impl Into<String>) -> Self {
        PanelId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The panel kind: the part before the first `:` (`"timeline:comp-1"` →
    /// `"timeline"`). This is the `Registry<PanelKind>` id.
    pub fn kind(&self) -> &str {
        self.0.split_once(':').map_or(&self.0, |(kind, _)| kind)
    }

    /// The instance part after the first `:`, if any.
    pub fn instance(&self) -> Option<&str> {
        self.0.split_once(':').map(|(_, instance)| instance)
    }
}

impl From<String> for PanelId {
    fn from(s: String) -> Self {
        PanelId(s)
    }
}

impl From<&str> for PanelId {
    fn from(s: &str) -> Self {
        PanelId(s.to_owned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct FrameId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    /// Children side by side, left to right.
    Row,
    /// Children stacked, top to bottom.
    Column,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Node {
    Split {
        axis: Axis,
        children: Vec<Node>,
        /// Relative sizes (any unit; usually the points they were designed at).
        sizes: Vec<f32>,
    },
    Frame(Frame),
}

/// A panel group.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub id: FrameId,
    pub tabs: Vec<PanelId>,
    pub active: usize,
    /// Show panels as a stack of headers instead of tabs.
    #[serde(default)]
    pub stacked: bool,
    /// Stacked: whether every panel is collapsed.
    #[serde(default)]
    pub collapsed: bool,
    /// Stacked: content height of the expanded panel; `None` fills the frame.
    #[serde(default)]
    pub stack_height: Option<f32>,
    #[serde(default)]
    pub small_tabs: bool,
}

impl Frame {
    pub fn new(tabs: &[&str]) -> Self {
        Frame {
            id: FrameId(0),
            tabs: tabs.iter().map(|t| PanelId::from(*t)).collect(),
            active: 0,
            stacked: false,
            collapsed: false,
            stack_height: None,
            small_tabs: false,
        }
    }

    pub fn active(mut self, i: usize) -> Self {
        self.active = i;
        self
    }

    pub fn stacked(mut self, expanded_height: Option<f32>) -> Self {
        self.stacked = true;
        self.stack_height = expanded_height;
        self
    }

    pub fn active_panel(&self) -> Option<&PanelId> {
        self.tabs.get(self.active)
    }
}

impl Node {
    pub fn row(children: Vec<(f32, Node)>) -> Node {
        Node::split(Axis::Row, children)
    }

    pub fn column(children: Vec<(f32, Node)>) -> Node {
        Node::split(Axis::Column, children)
    }

    fn split(axis: Axis, children: Vec<(f32, Node)>) -> Node {
        let (sizes, children) = children.into_iter().unzip();
        Node::Split {
            axis,
            children,
            sizes,
        }
    }

    pub fn frame(frame: Frame) -> Node {
        Node::Frame(frame)
    }

    pub fn tabs(tabs: &[&str]) -> Node {
        Node::Frame(Frame::new(tabs))
    }

    fn frames(&self, out: &mut Vec<FrameId>) {
        match self {
            Node::Frame(f) => out.push(f.id),
            Node::Split { children, .. } => children.iter().for_each(|c| c.frames(out)),
        }
    }

    fn find(&self, id: FrameId) -> Option<&Frame> {
        match self {
            Node::Frame(f) => (f.id == id).then_some(f),
            Node::Split { children, .. } => children.iter().find_map(|c| c.find(id)),
        }
    }

    fn find_mut(&mut self, id: FrameId) -> Option<&mut Frame> {
        match self {
            Node::Frame(f) => (f.id == id).then_some(f),
            Node::Split { children, .. } => children.iter_mut().find_map(|c| c.find_mut(id)),
        }
    }

    fn find_node_mut(&mut self, id: FrameId) -> Option<&mut Node> {
        if matches!(self, Node::Frame(f) if f.id == id) {
            return Some(self);
        }
        match self {
            Node::Split { children, .. } => children.iter_mut().find_map(|c| c.find_node_mut(id)),
            Node::Frame(_) => None,
        }
    }

    fn at_path_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        match path.split_first() {
            None => Some(self),
            Some((&i, rest)) => match self {
                Node::Split { children, .. } => children.get_mut(i)?.at_path_mut(rest),
                Node::Frame(_) => None,
            },
        }
    }

    fn max_id(&self) -> u32 {
        match self {
            Node::Frame(f) => f.id.0,
            Node::Split { children, .. } => children.iter().map(Node::max_id).max().unwrap_or(0),
        }
    }

    fn assign_ids(&mut self, next: &mut u32) {
        match self {
            Node::Frame(f) => {
                if f.id.0 == 0 {
                    *next += 1;
                    f.id = FrameId(*next);
                }
            }
            Node::Split { children, .. } => children.iter_mut().for_each(|c| c.assign_ids(next)),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Node::Frame(f) => f.tabs.is_empty(),
            Node::Split { children, .. } => children.is_empty(),
        }
    }

    /// Drop empty frames, unwrap single-child splits and merge nested splits
    /// that share the parent's axis.
    fn normalize(&mut self) {
        if let Node::Frame(f) = self {
            f.active = f.active.min(f.tabs.len().saturating_sub(1));
            return;
        }
        let Node::Split {
            axis,
            children,
            sizes,
        } = self
        else {
            return;
        };
        let mut new_children = Vec::with_capacity(children.len());
        let mut new_sizes = Vec::with_capacity(sizes.len());
        for (mut child, size) in children.drain(..).zip(sizes.drain(..)) {
            child.normalize();
            if child.is_empty() {
                continue;
            }
            match child {
                Node::Split {
                    axis: a,
                    children: gc,
                    sizes: gs,
                } if a == *axis => {
                    let total: f32 = gs.iter().sum::<f32>().max(1e-6);
                    new_sizes.extend(gs.iter().map(|s| size * s / total));
                    new_children.extend(gc);
                }
                other => {
                    new_children.push(other);
                    new_sizes.push(size);
                }
            }
        }
        *children = new_children;
        *sizes = new_sizes;
        if children.len() == 1 {
            *self = children.pop().unwrap_or_else(|| Node::tabs(&[]));
        }
    }

    /// Remove `panel` wherever it is; returns the frame it was in.
    fn take_panel(&mut self, panel: &PanelId) -> Option<FrameId> {
        match self {
            Node::Frame(f) => {
                let i = f.tabs.iter().position(|t| t == panel)?;
                f.tabs.remove(i);
                if f.active > i || f.active >= f.tabs.len() {
                    f.active = f.active.saturating_sub(1);
                }
                Some(f.id)
            }
            Node::Split { children, .. } => children.iter_mut().find_map(|c| c.take_panel(panel)),
        }
    }

    fn frame_of(&self, panel: &PanelId) -> Option<FrameId> {
        match self {
            Node::Frame(f) => f.tabs.contains(panel).then_some(f.id),
            Node::Split { children, .. } => children.iter().find_map(|c| c.frame_of(panel)),
        }
    }
}

/// A frame floating in its own OS window.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FloatingFrame {
    pub frame: Frame,
    pub pos: Option<[f32; 2]>,
    pub size: [f32; 2],
}

/// The saved form of a dock: what a workspace stores.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layout {
    pub root: Node,
    #[serde(default)]
    pub floating: Vec<FloatingFrame>,
}

// Compared through JSON (the tree holds floats); used for change detection.
impl PartialEq for Layout {
    fn eq(&self, other: &Self) -> bool {
        serde_json::to_value(self).ok() == serde_json::to_value(other).ok()
    }
}

impl Layout {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Result<Layout, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Every panel id in the layout, docked first, then floating.
    pub fn panel_ids(&self) -> Vec<PanelId> {
        fn walk(node: &Node, out: &mut Vec<PanelId>) {
            match node {
                Node::Frame(f) => out.extend(f.tabs.iter().cloned()),
                Node::Split { children, .. } => children.iter().for_each(|c| walk(c, out)),
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        for f in &self.floating {
            out.extend(f.frame.tabs.iter().cloned());
        }
        out
    }
}

/// Where a dragged tab will land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

/// What a frame shows on a tab, supplied by the host every frame.
#[derive(Clone, Debug, Default)]
pub struct TabInfo {
    pub title: String,
    /// Label color swatch before the title (Effect Controls, comps, timelines).
    pub label: Option<Color32>,
    /// Lock toggle on viewer tabs; `None` hides it.
    pub locked: Option<bool>,
    /// Viewers (Composition, Layer, Footage, Timeline) always show the close button.
    pub viewer: bool,
}

impl TabInfo {
    pub fn new(title: impl Into<String>) -> Self {
        TabInfo {
            title: title.into(),
            ..Default::default()
        }
    }

    pub fn label(mut self, c: Color32) -> Self {
        self.label = Some(c);
        self
    }

    pub fn viewer(mut self) -> Self {
        self.viewer = true;
        self
    }

    pub fn locked(mut self, locked: bool) -> Self {
        self.locked = Some(locked);
        self
    }
}

/// Supplies panel titles and contents to the dock.
pub trait PanelHost {
    fn tab(&mut self, panel: &PanelId) -> TabInfo;

    /// Draw the panel contents. The `ui` is clipped to the panel.
    fn ui(&mut self, ui: &mut Ui, panel: &PanelId);

    /// Panel-specific items appended to the ☰ menu.
    fn menu(&mut self, ui: &mut Ui, panel: &PanelId) {
        let _ = (ui, panel);
    }

    fn on_close(&mut self, panel: &PanelId) {
        let _ = panel;
    }

    fn set_locked(&mut self, panel: &PanelId, locked: bool) {
        let _ = (panel, locked);
    }
}

#[derive(Clone, Debug)]
struct TabDrag {
    panel: PanelId,
    from: FrameId,
    title: String,
}

#[derive(Clone, Debug)]
enum Action {
    Focus(FrameId),
    Activate(FrameId, usize),
    Close(PanelId),
    CloseOthers(FrameId, usize),
    CloseFrame(FrameId),
    Undock(PanelId, Option<Pos2>),
    UndockFrame(FrameId),
    ToggleMaximize(FrameId),
    ToggleStacked(FrameId),
    ToggleSmallTabs(FrameId),
    ToggleCollapsed(FrameId),
    StackHeight(FrameId, f32),
    Move(PanelId, FrameId, DropZone, Option<usize>),
}

/// Geometry constants measured from AE 2026.
mod geom {
    /// Left padding before the first tab element.
    pub const TAB_PAD: f32 = 12.0;
    /// Gap between two tabs (right pad of one + left pad of the next).
    pub const TAB_SPACING: f32 = 24.0;
    /// Element widths and the gap that follows each one.
    pub const CLOSE_W: f32 = 4.0;
    pub const CLOSE_GAP: f32 = 9.5;
    pub const SWATCH: f32 = 10.0;
    pub const SWATCH_GAP: f32 = 11.5;
    pub const LOCK_W: f32 = 7.5;
    pub const LOCK_GAP: f32 = 10.5;
    /// Gap between the title and the ☰ button.
    pub const MENU_GAP: f32 = 12.5;
    pub const MENU_W: f32 = 8.0;
    /// Text baseline below the frame top.
    pub const BASELINE: f32 = 16.5;
    /// Active-tab underline: top edge below the frame top.
    pub const UNDERLINE_Y: f32 = 22.0;
    /// Overflow chevron area at the right of the tab bar.
    pub const OVERFLOW_W: f32 = 24.0;
    pub const STACK_SEP: f32 = 2.0;
    /// Right margin of the black separators in a stack.
    pub const STACK_SEP_MARGIN: f32 = 18.0;
    pub const MIN_FRAME: f32 = 40.0;
}

struct Splitter {
    path: Vec<usize>,
    index: usize,
    axis: Axis,
    rect: Rect,
    /// Lengths of all children along the axis, in points.
    lengths: Vec<f32>,
}

/// The dock widget and its transient interaction state.
pub struct Dock {
    pub layout: Layout,
    focused: Option<FrameId>,
    maximized: Option<FrameId>,
    drag: Option<TabDrag>,
    actions: Vec<Action>,
    next_id: u32,
    id: Id,
}

impl Dock {
    pub fn new(root: Node) -> Self {
        Self::from_layout(Layout {
            root,
            floating: Vec::new(),
        })
    }

    pub fn from_layout(mut layout: Layout) -> Self {
        let mut next = layout
            .floating
            .iter()
            .map(|f| f.frame.id.0)
            .max()
            .unwrap_or(0)
            .max(layout.root.max_id());
        layout.root.assign_ids(&mut next);
        for f in &mut layout.floating {
            if f.frame.id.0 == 0 {
                next += 1;
                f.frame.id = FrameId(next);
            }
        }
        layout.root.normalize();
        Dock {
            layout,
            focused: None,
            maximized: None,
            drag: None,
            actions: Vec::new(),
            next_id: next,
            id: Id::new("bae-dock"),
        }
    }

    /// Replace the layout (switching or resetting a workspace).
    pub fn set_layout(&mut self, layout: Layout) {
        let id = self.id;
        *self = Dock::from_layout(layout);
        self.id = id;
    }

    pub fn with_id(mut self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = Id::new(id);
        self
    }

    pub fn focused_frame(&self) -> Option<FrameId> {
        self.focused
    }

    /// The active panel of the focused frame, used to route shortcuts.
    pub fn focused_panel(&self) -> Option<&PanelId> {
        self.frame(self.focused?)?.active_panel()
    }

    pub fn frame(&self, id: FrameId) -> Option<&Frame> {
        self.layout.root.find(id).or_else(|| {
            self.layout
                .floating
                .iter()
                .find(|f| f.frame.id == id)
                .map(|f| &f.frame)
        })
    }

    fn frame_mut(&mut self, id: FrameId) -> Option<&mut Frame> {
        if let Some(f) = self.layout.root.find_mut(id) {
            return Some(f);
        }
        self.layout
            .floating
            .iter_mut()
            .find(|f| f.frame.id == id)
            .map(|f| &mut f.frame)
    }

    pub fn frame_of(&self, panel: &PanelId) -> Option<FrameId> {
        self.layout.root.frame_of(panel).or_else(|| {
            self.layout
                .floating
                .iter()
                .find(|f| f.frame.tabs.contains(panel))
                .map(|f| f.frame.id)
        })
    }

    pub fn contains(&self, panel: &PanelId) -> bool {
        self.frame_of(panel).is_some()
    }

    pub fn frame_ids(&self) -> Vec<FrameId> {
        let mut out = Vec::new();
        self.layout.root.frames(&mut out);
        out
    }

    /// Focus the frame holding `panel` and make it the active tab.
    pub fn focus_panel(&mut self, panel: &PanelId) {
        if let Some(fid) = self.frame_of(panel) {
            if let Some(f) = self.frame_mut(fid)
                && let Some(i) = f.tabs.iter().position(|t| t == panel)
            {
                f.active = i;
            }
            self.focused = Some(fid);
        }
    }

    /// Bring a panel to front, or add it to the focused frame if it is closed.
    pub fn open(&mut self, panel: PanelId) {
        if let Some(fid) = self.frame_of(&panel) {
            if let Some(f) = self.frame_mut(fid)
                && let Some(i) = f.tabs.iter().position(|t| *t == panel)
            {
                f.active = i;
                f.collapsed = false;
            }
            self.focused = Some(fid);
            return;
        }
        let target = self
            .focused
            .filter(|f| self.layout.root.find(*f).is_some())
            .or_else(|| self.frame_ids().first().copied());
        match target.and_then(|t| self.layout.root.find_mut(t)) {
            Some(f) => {
                f.tabs.push(panel);
                f.active = f.tabs.len() - 1;
            }
            None => {
                self.next_id += 1;
                let mut f = Frame::new(&[]);
                f.id = FrameId(self.next_id);
                f.tabs.push(panel);
                self.layout.root = Node::Frame(f);
            }
        }
    }

    /// Maximize `frame` (default: the focused frame) to fill the dock, or restore
    /// it if it is already maximized (AE: `~`).
    pub fn toggle_maximize(&mut self, frame: Option<FrameId>) {
        if self.maximized.is_some() {
            self.maximized = None;
        } else {
            self.maximized = frame.or(self.focused);
        }
    }

    pub fn close(&mut self, panel: &PanelId) {
        self.layout.root.take_panel(panel);
        for f in &mut self.layout.floating {
            if let Some(i) = f.frame.tabs.iter().position(|t| t == panel) {
                f.frame.tabs.remove(i);
                f.frame.active = f.frame.active.min(f.frame.tabs.len().saturating_sub(1));
            }
        }
        self.layout.floating.retain(|f| !f.frame.tabs.is_empty());
        self.layout.root.normalize();
    }

    /// Move a panel next to or into another frame.
    pub fn move_panel(
        &mut self,
        panel: &PanelId,
        target: FrameId,
        zone: DropZone,
        index: Option<usize>,
    ) {
        let Some(source) = self.frame_of(panel) else {
            return;
        };
        let source_len = self.frame(source).map_or(0, |f| f.tabs.len());
        if source == target && (zone == DropZone::Center && index.is_none() || source_len == 1) {
            return;
        }
        if self.frame(target).is_none() {
            return;
        }
        // Remove from the source without normalizing, so `target` keeps its node.
        if self.layout.root.take_panel(panel).is_none() {
            for f in &mut self.layout.floating {
                if let Some(i) = f.frame.tabs.iter().position(|t| t == panel) {
                    f.frame.tabs.remove(i);
                    f.frame.active = f.frame.active.min(f.frame.tabs.len().saturating_sub(1));
                }
            }
        }
        match zone {
            DropZone::Center => {
                if let Some(f) = self.frame_mut(target) {
                    let i = index.unwrap_or(f.tabs.len()).min(f.tabs.len());
                    f.tabs.insert(i, panel.clone());
                    f.active = i;
                    f.collapsed = false;
                }
            }
            edge => {
                self.next_id += 1;
                let mut frame = Frame::new(&[]);
                frame.id = FrameId(self.next_id);
                frame.tabs.push(panel.clone());
                let new_id = frame.id;
                if let Some(node) = self.layout.root.find_node_mut(target) {
                    let old = std::mem::replace(node, Node::tabs(&[]));
                    let axis = match edge {
                        DropZone::Left | DropZone::Right => Axis::Row,
                        _ => Axis::Column,
                    };
                    let new = Node::Frame(frame);
                    let children = match edge {
                        DropZone::Left | DropZone::Top => vec![new, old],
                        _ => vec![old, new],
                    };
                    *node = Node::Split {
                        axis,
                        children,
                        sizes: vec![1.0, 1.0],
                    };
                    self.focused = Some(new_id);
                } else if let Some(fl) = self
                    .layout
                    .floating
                    .iter_mut()
                    .find(|f| f.frame.id == target)
                {
                    // Splitting inside a floating window: keep it simple and add as a tab.
                    fl.frame.tabs.push(panel.clone());
                    fl.frame.active = fl.frame.tabs.len() - 1;
                }
            }
        }
        self.layout.floating.retain(|f| !f.frame.tabs.is_empty());
        self.layout.root.normalize();
    }

    /// Show the dock in the remaining space of `ui`, plus any floating windows.
    pub fn show(&mut self, ui: &mut Ui, host: &mut impl PanelHost) {
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, Sense::hover());
        ui.painter().rect_filled(rect, 0.0, color::APP_BG);

        let mut frames = Vec::new();
        if let Some(m) = self
            .maximized
            .filter(|m| self.layout.root.find(*m).is_some())
        {
            frames.push((m, rect));
        } else {
            self.maximized = None;
            let mut splitters = Vec::new();
            layout_node(
                &self.layout.root,
                rect,
                &mut Vec::new(),
                &mut frames,
                &mut splitters,
            );
            if self.drag_splitters(ui, &splitters) {
                frames.clear();
                splitters.clear();
                layout_node(
                    &self.layout.root,
                    rect,
                    &mut Vec::new(),
                    &mut frames,
                    &mut splitters,
                );
            }
        }

        for &(fid, frect) in &frames {
            if let Some(frame) = self.layout.root.find(fid).cloned() {
                self.frame_ui(ui, frect, &frame, host);
            }
        }

        self.drag_and_drop(ui, &frames);
        self.floating_windows(ui, host);
        self.handle_keys(ui, &frames);
        self.apply(host);
    }

    fn handle_keys(&mut self, ui: &Ui, frames: &[(FrameId, Rect)]) {
        if ui.ctx().text_edit_focused() || crate::panel::modal_open(ui.ctx()) {
            return;
        }
        // AE: the ` (tilde) key maximizes the panel group under the pointer.
        if ui.input(|i| i.key_pressed(Key::Backtick)) {
            let hover = ui.input(|i| i.pointer.hover_pos());
            if let Some((fid, _)) = hover.and_then(|p| frames.iter().find(|(_, r)| r.contains(p))) {
                self.actions.push(Action::ToggleMaximize(*fid));
            }
        }
    }

    fn drag_splitters(&mut self, ui: &Ui, splitters: &[Splitter]) -> bool {
        let mut changed = false;
        for s in splitters {
            let id = self.id.with(("split", &s.path, s.index));
            let resp = ui
                .interact(s.rect.expand(1.0), id, Sense::drag())
                .labeled(egui::WidgetType::ResizeHandle, tr("Resize Panels"));
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(match s.axis {
                    Axis::Row => CursorIcon::ResizeHorizontal,
                    Axis::Column => CursorIcon::ResizeVertical,
                });
            }
            if !resp.dragged() {
                continue;
            }
            let delta = match s.axis {
                Axis::Row => resp.drag_delta().x,
                Axis::Column => resp.drag_delta().y,
            };
            if delta == 0.0 {
                continue;
            }
            let Some(Node::Split { sizes, .. }) = self.layout.root.at_path_mut(&s.path) else {
                continue;
            };
            let mut lengths = s.lengths.clone();
            let (a, b) = (s.index, s.index + 1);
            let pair = lengths[a] + lengths[b];
            let min = geom::MIN_FRAME.min(pair / 2.0);
            lengths[a] = (lengths[a] + delta).clamp(min, pair - min);
            lengths[b] = pair - lengths[a];
            *sizes = lengths;
            changed = true;
        }
        changed
    }

    fn frame_ui(&mut self, ui: &mut Ui, rect: Rect, frame: &Frame, host: &mut impl PanelHost) {
        let p = ui.painter();
        p.rect_filled(rect, 0.0, color::PANEL);

        let pressed_inside = ui.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| rect.contains(p))
        });
        if pressed_inside && self.focused != Some(frame.id) {
            self.actions.push(Action::Focus(frame.id));
        }

        if frame.tabs.is_empty() {
            return;
        }
        if frame.stacked {
            self.stack_ui(ui, rect, frame, host);
        } else {
            let content = self.tab_bar(ui, rect, frame, host);
            if let Some(panel) = frame.active_panel() {
                panel_contents(ui, content, panel, host);
            }
        }

        if self.focused == Some(frame.id) {
            ui.painter().rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, color::FOCUS),
                StrokeKind::Inside,
            );
        }
    }

    /// Draws the tab row; returns the content rect below it.
    fn tab_bar(&mut self, ui: &Ui, rect: Rect, frame: &Frame, host: &mut impl PanelHost) -> Rect {
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), metrics::TAB_BAR_H));
        let font = theme::body();
        let multi = frame.tabs.len() > 1;
        let tabs_right = if multi {
            bar.right() - geom::OVERFLOW_W - 4.0
        } else {
            bar.right()
        };
        let mut x = bar.left() + geom::TAB_PAD;
        let baseline = bar.top() + geom::BASELINE;
        let mut tab_rects = Vec::new();

        for (i, panel) in frame.tabs.iter().enumerate() {
            let info = host.tab(panel);
            let active = i == frame.active;
            let close_slot = !active || info.viewer;
            let galley =
                ui.painter()
                    .layout_no_wrap(tr(&info.title).to_owned(), font.clone(), color::TEXT);
            let mut w = galley.size().x;
            if close_slot {
                w += geom::CLOSE_W + geom::CLOSE_GAP;
            }
            if info.label.is_some() {
                w += geom::SWATCH + geom::SWATCH_GAP;
            }
            if info.locked.is_some() {
                w += geom::LOCK_W + geom::LOCK_GAP;
            }
            if active {
                w += geom::MENU_GAP + geom::MENU_W;
            }
            if x + w > tabs_right && i > 0 {
                break;
            }
            let tab_rect = Rect::from_min_max(
                pos2(x - geom::TAB_PAD / 2.0, bar.top()),
                pos2((x + w + geom::TAB_PAD / 2.0).min(tabs_right), bar.bottom()),
            );
            tab_rects.push((i, tab_rect));
            let tab_id = self.id.with(("tab", panel));
            let resp = ui
                .interact(tab_rect, tab_id, Sense::click_and_drag())
                .labeled_toggle(egui::WidgetType::SelectableLabel, tr(&info.title), active);
            let hovered = resp.hovered();

            if resp.clicked() || resp.secondary_clicked() {
                self.actions.push(Action::Activate(frame.id, i));
                self.actions.push(Action::Focus(frame.id));
            }
            if resp.drag_started() {
                self.drag = Some(TabDrag {
                    panel: panel.clone(),
                    from: frame.id,
                    title: info.title.clone(),
                });
            }
            self.panel_menu(&resp, frame, i, host, true);

            let painter = ui.painter_at(bar);
            let mid_y = bar.top() + 12.5;
            let mut cx = x;
            // Close button: always on viewers, on hover for the rest.
            let close_x = if close_slot {
                cx
            } else {
                x - geom::TAB_PAD + 3.0
            };
            if close_slot {
                cx += geom::CLOSE_W + geom::CLOSE_GAP;
            }
            let close_rect = Rect::from_center_size(pos2(close_x + 2.0, mid_y), Vec2::splat(10.0));
            let close_resp = ui
                .interact(close_rect, tab_id.with("close"), Sense::click())
                .labeled(egui::WidgetType::Button, tr("Close Panel"));
            if (active && info.viewer) || hovered || close_resp.hovered() {
                let c = if close_resp.hovered() {
                    color::TEXT_STRONG
                } else {
                    color::TAB_CLOSE
                };
                icons::paint(
                    &painter,
                    Rect::from_center_size(close_rect.center(), Vec2::splat(16.0)),
                    Icon::Close,
                    c,
                );
            }
            if close_resp.clicked() {
                self.actions.push(Action::Close(panel.clone()));
            }
            if let Some(label) = info.label {
                let r = Rect::from_min_size(pos2(cx, mid_y - 5.5), Vec2::splat(geom::SWATCH));
                painter.rect_filled(r, 0.0, label);
                cx += geom::SWATCH + geom::SWATCH_GAP;
            }
            if let Some(locked) = info.locked {
                let r = Rect::from_center_size(
                    pos2(cx + geom::LOCK_W / 2.0, mid_y - 0.5),
                    Vec2::splat(15.0),
                );
                let lr = ui
                    .interact(r, tab_id.with("lock"), Sense::click())
                    .labeled_toggle(egui::WidgetType::Checkbox, tr("Lock"), locked);
                let icon = if locked { Icon::Lock } else { Icon::Unlock };
                let c = if lr.hovered() {
                    color::TEXT_STRONG
                } else {
                    color::TAB_INACTIVE
                };
                icons::paint(&painter, r, icon, c);
                if lr.clicked() {
                    host.set_locked(panel, !locked);
                }
                cx += geom::LOCK_W + geom::LOCK_GAP;
            }
            let text_x = cx;
            let tc = if active || hovered {
                color::TAB_ACTIVE
            } else {
                color::TAB_INACTIVE
            };
            let text_top = baseline - theme::baseline_offset(&galley);
            painter.galley_with_override_text_color(pos2(text_x, text_top), galley.clone(), tc);
            cx += galley.size().x;
            if active {
                cx += geom::MENU_GAP;
                let mr = Rect::from_min_size(pos2(cx, mid_y - 6.0), vec2(geom::MENU_W, 12.0));
                let menu_resp = ui
                    .interact(mr.expand(3.0), tab_id.with("menu"), Sense::click())
                    .labeled(egui::WidgetType::Button, tr("Panel Menu"));
                let mc = if menu_resp.hovered() {
                    color::TEXT_STRONG
                } else {
                    tc
                };
                icons::paint(
                    &painter,
                    Rect::from_center_size(mr.center(), Vec2::splat(16.0)),
                    Icon::Menu,
                    mc,
                );
                self.panel_menu(&menu_resp, frame, i, host, false);
                cx += geom::MENU_W;
                if multi {
                    let ul = Rect::from_min_max(
                        pos2(text_x - 1.0, bar.top() + geom::UNDERLINE_Y),
                        pos2(cx + 1.0, bar.top() + geom::UNDERLINE_Y + 1.0),
                    );
                    painter.rect_filled(ul, 0.5, color::TAB_ACTIVE);
                }
            }
            x += w + geom::TAB_SPACING;
        }

        if multi {
            self.overflow_menu(ui, bar, frame, host);
        }
        self.store_tab_rects(ui, frame.id, tab_rects);

        Rect::from_min_max(pos2(rect.left(), bar.bottom()), rect.max)
    }

    fn overflow_menu(&mut self, ui: &Ui, bar: Rect, frame: &Frame, host: &mut impl PanelHost) {
        let p = ui.painter();
        let sep_x = bar.right() - geom::OVERFLOW_W;
        p.vline(
            sep_x,
            (bar.top() + 6.0)..=(bar.top() + 21.0),
            Stroke::new(1.0, color::TAB_SEPARATOR),
        );
        let r = Rect::from_center_size(
            pos2(bar.right() - 11.5, bar.top() + 12.5),
            Vec2::splat(14.0),
        );
        let resp = ui
            .interact(r, self.id.with(("overflow", frame.id)), Sense::click())
            .labeled(egui::WidgetType::Button, tr("More Panels"));
        let c = if resp.hovered() {
            color::TEXT_STRONG
        } else {
            color::TAB_INACTIVE
        };
        icons::paint(p, r, Icon::ChevronDoubleRight, c);
        egui::Popup::menu(&resp).show(|ui| {
            for (i, panel) in frame.tabs.iter().enumerate() {
                let title = host.tab(panel).title;
                if ui.selectable_label(i == frame.active, tr(&title)).clicked() {
                    self.actions.push(Action::Activate(frame.id, i));
                }
            }
        });
    }

    fn store_tab_rects(&self, ui: &Ui, frame: FrameId, rects: Vec<(usize, Rect)>) {
        ui.data_mut(|d| d.insert_temp(self.id.with(("tabrects", frame)), rects));
    }

    /// Tab hit rects from the last pass, used to pick the insertion index on drop.
    fn tab_rects(&self, ctx: &egui::Context, frame: FrameId) -> Vec<(usize, Rect)> {
        ctx.data(|d| d.get_temp(self.id.with(("tabrects", frame))))
            .unwrap_or_default()
    }

    /// The ☰ panel menu: on click of `resp`, or on right click when `context`.
    fn panel_menu(
        &mut self,
        resp: &Response,
        frame: &Frame,
        index: usize,
        host: &mut impl PanelHost,
        context: bool,
    ) {
        let popup = if context {
            egui::Popup::context_menu(resp)
        } else {
            egui::Popup::menu(resp)
        };
        let fid = frame.id;
        let panel = frame.tabs[index].clone();
        let maximized = self.maximized == Some(fid);
        let mut actions = Vec::new();
        popup.show(|ui| {
            ui.set_min_width(190.0);
            if ui.button(tr("Close Panel")).clicked() {
                actions.push(Action::Close(panel.clone()));
            }
            if ui.button(tr("Undock Panel")).clicked() {
                actions.push(Action::Undock(panel.clone(), None));
            }
            let others = ui.add_enabled(
                frame.tabs.len() > 1,
                egui::Button::new(tr("Close Other Panels in Group")),
            );
            if others.clicked() {
                actions.push(Action::CloseOthers(fid, index));
            }
            ui.menu_button(tr("Panel Group Settings"), |ui| {
                ui.set_min_width(170.0);
                if ui.button(tr("Close Panel Group")).clicked() {
                    actions.push(Action::CloseFrame(fid));
                }
                if ui.button(tr("Undock Panel Group")).clicked() {
                    actions.push(Action::UndockFrame(fid));
                }
                let max_label = if maximized {
                    "Restore Panel Group Size"
                } else {
                    "Maximize Panel Group"
                };
                if ui.button(tr(max_label)).clicked() {
                    actions.push(Action::ToggleMaximize(fid));
                }
                ui.separator();
                if check_item(ui, frame.stacked, tr("Stacked Panel Group")).clicked() {
                    actions.push(Action::ToggleStacked(fid));
                }
                if check_item(ui, frame.small_tabs, tr("Small Tabs")).clicked() {
                    actions.push(Action::ToggleSmallTabs(fid));
                }
            });
            ui.separator();
            host.menu(ui, &panel);
        });
        self.actions.extend(actions);
    }

    fn stack_ui(&mut self, ui: &mut Ui, rect: Rect, frame: &Frame, host: &mut impl PanelHost) {
        let n = frame.tabs.len();
        let expanded = (!frame.collapsed).then_some(frame.active);
        let font = theme::body();
        let headers_h = n as f32 * (metrics::STACK_HEADER_H + geom::STACK_SEP)
            + (metrics::TAB_BAR_H - metrics::STACK_HEADER_H) * expanded.is_some() as u8 as f32;
        let free = (rect.height() - headers_h).max(0.0);
        let content_h = frame.stack_height.map_or(free, |h| h.min(free));
        let sep_stroke_rect = |y: f32| {
            Rect::from_min_max(
                pos2(rect.left(), y),
                pos2(rect.right() - geom::STACK_SEP_MARGIN, y + geom::STACK_SEP),
            )
        };
        let mut y = rect.top();
        for (i, panel) in frame.tabs.iter().enumerate() {
            let info = host.tab(panel);
            let is_open = expanded == Some(i);
            let h = if is_open {
                metrics::TAB_BAR_H
            } else {
                metrics::STACK_HEADER_H
            };
            let header = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), h));
            let id = self.id.with(("tab", panel));
            let resp = ui
                .interact(header, id, Sense::click_and_drag())
                .labeled_toggle(egui::WidgetType::CollapsingHeader, tr(&info.title), is_open);
            if resp.clicked() {
                if is_open {
                    self.actions.push(Action::ToggleCollapsed(frame.id));
                } else {
                    self.actions.push(Action::Activate(frame.id, i));
                }
                self.actions.push(Action::Focus(frame.id));
            }
            if resp.drag_started() {
                self.drag = Some(TabDrag {
                    panel: panel.clone(),
                    from: frame.id,
                    title: info.title.clone(),
                });
            }
            self.panel_menu(&resp, frame, i, host, true);
            let painter = ui.painter_at(header);
            let tc = if is_open || resp.hovered() {
                color::TAB_ACTIVE
            } else {
                color::TAB_INACTIVE
            };
            let baseline_off = if is_open { geom::BASELINE } else { 15.0 };
            let galley = painter.layout_no_wrap(tr(&info.title).to_owned(), font.clone(), tc);
            let top = y + baseline_off - theme::baseline_offset(&galley);
            painter.galley(pos2(header.left() + 13.0, top), galley, tc);
            if is_open {
                let mr = Rect::from_center_size(
                    pos2(rect.right() - geom::STACK_SEP_MARGIN - 9.0, y + 12.5),
                    Vec2::splat(16.0),
                );
                let menu_resp = ui
                    .interact(mr, id.with("menu"), Sense::click())
                    .labeled(egui::WidgetType::Button, tr("Panel Menu"));
                let mc = if menu_resp.hovered() {
                    color::TEXT_STRONG
                } else {
                    tc
                };
                icons::paint(&painter, mr, Icon::Menu, mc);
                self.panel_menu(&menu_resp, frame, i, host, false);
            }
            y += h;
            if is_open {
                let content =
                    Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), content_h));
                panel_contents(ui, content, panel, host);
                y += content_h;
                // Drag the line under an expanded panel to change its height.
                let handle = sep_stroke_rect(y).expand2(vec2(0.0, 2.0));
                let hr = ui
                    .interact(handle, id.with("stack-resize"), Sense::drag())
                    .labeled(egui::WidgetType::ResizeHandle, tr("Resize Panels"));
                if hr.hovered() || hr.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
                }
                if hr.dragged() {
                    let h = (content_h + hr.drag_delta().y).clamp(20.0, free);
                    self.actions.push(Action::StackHeight(frame.id, h));
                }
                ui.painter()
                    .rect_filled(sep_stroke_rect(y), 0.0, color::APP_BG);
                y += geom::STACK_SEP / 2.0;
            } else {
                ui.painter()
                    .rect_filled(sep_stroke_rect(y), 0.0, color::APP_BG);
                y += geom::STACK_SEP;
            }
        }
    }

    fn drag_and_drop(&mut self, ui: &Ui, frames: &[(FrameId, Rect)]) {
        let Some(drag) = self.drag.clone() else {
            return;
        };
        let (pos, released, down) = ui.input(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.primary_released(),
                i.pointer.primary_down(),
            )
        });
        let Some(pos) = pos else {
            return;
        };
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);

        let target = frames
            .iter()
            .find(|(_, r)| r.contains(pos))
            .map(|&(fid, r)| {
                let zone = drop_zone(r, pos);
                (fid, r, zone)
            });
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            self.id.with("dnd"),
        ));
        let mut index = None;
        if let Some((fid, r, zone)) = target {
            let same_single =
                fid == drag.from && self.frame(fid).is_some_and(|f| f.tabs.len() == 1);
            if !same_single {
                let hl = zone_rect(r, zone);
                painter.rect_filled(hl, 0.0, color::DROP_ZONE);
                painter.rect_stroke(hl, 0.0, Stroke::new(2.0, color::ACCENT), StrokeKind::Inside);
            }
            if zone == DropZone::Center {
                let rects = self.tab_rects(ui.ctx(), fid);
                index = rects
                    .iter()
                    .find(|(_, tr)| pos.x < tr.center().x && pos.y < tr.bottom())
                    .map(|(i, _)| *i);
            }
        }
        // Ghost tab following the pointer.
        let galley =
            painter.layout_no_wrap(tr(&drag.title).to_owned(), theme::body(), color::TAB_ACTIVE);
        let ghost = Rect::from_min_size(pos + vec2(10.0, 8.0), galley.size() + vec2(16.0, 8.0));
        painter.rect_filled(ghost, 3.0, color::BUTTON_ON);
        painter.galley(ghost.min + vec2(8.0, 4.0), galley, color::TAB_ACTIVE);

        if released || !down {
            self.drag = None;
            match target {
                Some((fid, _, zone)) => {
                    self.actions
                        .push(Action::Move(drag.panel, fid, zone, index));
                }
                None if !ui.ctx().content_rect().contains(pos) => {
                    self.actions.push(Action::Undock(drag.panel, Some(pos)));
                }
                None => {}
            }
        }
    }

    fn floating_windows(&mut self, ui: &Ui, host: &mut impl PanelHost) {
        let mut closed = Vec::new();
        let floating = self.layout.floating.clone();
        for fl in floating {
            let fid = fl.frame.id;
            let title = fl
                .frame
                .active_panel()
                .map(|p| tr(&host.tab(p).title).to_owned())
                .unwrap_or_default();
            let mut builder = ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(fl.size)
                .with_min_inner_size([120.0, 80.0]);
            if let Some(pos) = fl.pos {
                builder = builder.with_position(pos);
            }
            let vid = ViewportId::from_hash_of(self.id.with(("float", fid)));
            ui.ctx().show_viewport_immediate(vid, builder, |ui, class| {
                if class == ViewportClass::EmbeddedWindow {
                    // Backend without multi-window support: egui draws a window for us.
                }
                let rect = ui.available_rect_before_wrap();
                ui.allocate_rect(rect, Sense::hover());
                if ui.input(|i| i.viewport().close_requested()) {
                    closed.push(fid);
                }
                if let Some(size) = ui.input(|i| i.viewport().inner_rect.map(|r| r.size()))
                    && let Some(f) = self.layout.floating.iter_mut().find(|f| f.frame.id == fid)
                {
                    f.size = [size.x, size.y];
                }
                if let Some(frame) = self
                    .layout
                    .floating
                    .iter()
                    .find(|f| f.frame.id == fid)
                    .map(|f| f.frame.clone())
                {
                    self.frame_ui(ui, rect, &frame, host);
                }
            });
        }
        for fid in closed {
            if let Some(i) = self.layout.floating.iter().position(|f| f.frame.id == fid) {
                let fl = self.layout.floating.remove(i);
                for p in &fl.frame.tabs {
                    host.on_close(p);
                }
            }
        }
    }

    fn apply(&mut self, host: &mut impl PanelHost) {
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            match a {
                Action::Focus(f) => self.focused = Some(f),
                Action::Activate(f, i) => {
                    if let Some(fr) = self.frame_mut(f) {
                        fr.active = i.min(fr.tabs.len().saturating_sub(1));
                        fr.collapsed = false;
                    }
                }
                Action::Close(p) => {
                    self.close(&p);
                    host.on_close(&p);
                }
                Action::CloseOthers(f, keep) => {
                    let others: Vec<PanelId> = self
                        .frame(f)
                        .map(|fr| {
                            fr.tabs
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| *i != keep)
                                .map(|(_, p)| p.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    for p in others {
                        self.close(&p);
                        host.on_close(&p);
                    }
                }
                Action::CloseFrame(f) => {
                    let tabs = self.frame(f).map(|fr| fr.tabs.clone()).unwrap_or_default();
                    for p in tabs {
                        self.close(&p);
                        host.on_close(&p);
                    }
                }
                Action::Undock(p, pos) => self.undock(&[p], pos),
                Action::UndockFrame(f) => {
                    let tabs = self.frame(f).map(|fr| fr.tabs.clone()).unwrap_or_default();
                    self.undock(&tabs, None);
                }
                Action::ToggleMaximize(f) => {
                    self.maximized = if self.maximized == Some(f) {
                        None
                    } else {
                        Some(f)
                    };
                }
                Action::ToggleStacked(f) => {
                    if let Some(fr) = self.frame_mut(f) {
                        fr.stacked = !fr.stacked;
                        fr.collapsed = false;
                    }
                }
                Action::ToggleSmallTabs(f) => {
                    if let Some(fr) = self.frame_mut(f) {
                        fr.small_tabs = !fr.small_tabs;
                    }
                }
                Action::ToggleCollapsed(f) => {
                    if let Some(fr) = self.frame_mut(f) {
                        fr.collapsed = !fr.collapsed;
                    }
                }
                Action::StackHeight(f, h) => {
                    if let Some(fr) = self.frame_mut(f) {
                        fr.stack_height = Some(h);
                    }
                }
                Action::Move(p, f, zone, index) => self.move_panel(&p, f, zone, index),
            }
        }
    }

    fn undock(&mut self, panels: &[PanelId], pos: Option<Pos2>) {
        if panels.is_empty() {
            return;
        }
        for p in panels {
            self.close(p);
        }
        self.next_id += 1;
        let mut frame = Frame::new(&[]);
        frame.id = FrameId(self.next_id);
        frame.tabs = panels.to_vec();
        self.layout.floating.push(FloatingFrame {
            frame,
            pos: pos.map(|p| [p.x, p.y]),
            size: [360.0, 420.0],
        });
    }
}

fn check_item(ui: &mut Ui, checked: bool, text: &str) -> Response {
    let mark = if checked { "✓  " } else { "    " };
    ui.button(format!("{mark}{text}"))
}

fn panel_contents(ui: &mut Ui, rect: Rect, panel: &PanelId, host: &mut impl PanelHost) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .id_salt(("bae-panel", panel))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    host.ui(&mut child, panel);
}

fn drop_zone(r: Rect, pos: Pos2) -> DropZone {
    if pos.y < r.top() + metrics::TAB_BAR_H {
        return DropZone::Center;
    }
    let u = (pos.x - r.left()) / r.width();
    let v = (pos.y - r.top()) / r.height();
    let edges = [
        (u, DropZone::Left),
        (1.0 - u, DropZone::Right),
        (v, DropZone::Top),
        (1.0 - v, DropZone::Bottom),
    ];
    let (d, zone) = edges
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((1.0, DropZone::Center));
    if d < 0.25 { zone } else { DropZone::Center }
}

fn zone_rect(r: Rect, zone: DropZone) -> Rect {
    let (w, h) = (r.width() / 2.0, r.height() / 2.0);
    match zone {
        DropZone::Center => r,
        DropZone::Left => Rect::from_min_size(r.min, vec2(w, r.height())),
        DropZone::Right => Rect::from_min_size(pos2(r.left() + w, r.top()), vec2(w, r.height())),
        DropZone::Top => Rect::from_min_size(r.min, vec2(r.width(), h)),
        DropZone::Bottom => Rect::from_min_size(pos2(r.left(), r.top() + h), vec2(r.width(), h)),
    }
}

fn layout_node(
    node: &Node,
    rect: Rect,
    path: &mut Vec<usize>,
    frames: &mut Vec<(FrameId, Rect)>,
    splitters: &mut Vec<Splitter>,
) {
    match node {
        Node::Frame(f) => frames.push((f.id, rect)),
        Node::Split {
            axis,
            children,
            sizes,
        } => {
            let n = children.len();
            if n == 0 {
                return;
            }
            let gap = metrics::PANEL_GAP;
            let span = match axis {
                Axis::Row => rect.width(),
                Axis::Column => rect.height(),
            };
            let avail = (span - gap * (n - 1) as f32).max(0.0);
            let total: f32 = sizes.iter().take(n).sum::<f32>().max(1e-6);
            // Round to whole points so frame edges stay on the pixel grid.
            let mut lengths: Vec<f32> = (0..n)
                .map(|i| (sizes.get(i).copied().unwrap_or(1.0) / total * avail).round())
                .collect();
            let used: f32 = lengths.iter().sum();
            if let Some(last) = lengths.last_mut() {
                *last += avail - used;
            }
            let mut pos = match axis {
                Axis::Row => rect.left(),
                Axis::Column => rect.top(),
            };
            for (i, child) in children.iter().enumerate() {
                let len = lengths[i];
                let r = match axis {
                    Axis::Row => {
                        Rect::from_min_size(pos2(pos, rect.top()), vec2(len, rect.height()))
                    }
                    Axis::Column => {
                        Rect::from_min_size(pos2(rect.left(), pos), vec2(rect.width(), len))
                    }
                };
                path.push(i);
                layout_node(child, r, path, frames, splitters);
                path.pop();
                pos += len;
                if i + 1 < n {
                    let handle = match axis {
                        Axis::Row => {
                            Rect::from_min_size(pos2(pos, rect.top()), vec2(gap, rect.height()))
                        }
                        Axis::Column => {
                            Rect::from_min_size(pos2(rect.left(), pos), vec2(rect.width(), gap))
                        }
                    };
                    splitters.push(Splitter {
                        path: path.clone(),
                        index: i,
                        axis: *axis,
                        rect: handle,
                        lengths: lengths.clone(),
                    });
                    pos += gap;
                }
            }
        }
    }
}

/// Lay out a node without drawing (for tests and for hosts that need frame rects).
pub fn frame_rects(root: &Node, rect: Rect) -> Vec<(FrameId, Rect)> {
    let mut frames = Vec::new();
    layout_node(root, rect, &mut Vec::new(), &mut frames, &mut Vec::new());
    frames
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, RawInput};

    /// AE 2026 "Default" workspace below the toolbar (1728×1000 window).
    fn ae_default() -> Node {
        Node::row(vec![
            (
                1406.0,
                Node::column(vec![
                    (
                        640.0,
                        Node::row(vec![
                            (320.0, Node::tabs(&["project", "effect_controls"])),
                            (1082.0, Node::tabs(&["composition"])),
                        ]),
                    ),
                    (295.0, Node::tabs(&["timeline"])),
                ]),
            ),
            (
                316.0,
                Node::column(vec![
                    (52.0, Node::tabs(&["preview"])),
                    (
                        883.0,
                        Node::frame(
                            Frame::new(&["properties", "align", "audio", "effects"]).stacked(None),
                        ),
                    ),
                ]),
            ),
        ])
    }

    fn rect_of(dock: &Dock, screen: Rect, panel: &str) -> Rect {
        let fid = dock.frame_of(&panel.into()).unwrap();
        frame_rects(&dock.layout.root, screen)
            .into_iter()
            .find(|(f, _)| *f == fid)
            .unwrap()
            .1
    }

    struct Host;
    impl PanelHost for Host {
        fn tab(&mut self, panel: &PanelId) -> TabInfo {
            TabInfo::new(panel.as_str())
        }
        fn ui(&mut self, ui: &mut Ui, panel: &PanelId) {
            ui.label(panel.as_str());
        }
    }

    #[test]
    fn layout_matches_ae_default() {
        let dock = Dock::new(ae_default());
        // Area below the toolbar: x 1..1727, y 61..1000 in AE.
        let screen = Rect::from_min_max(pos2(1.0, 61.0), pos2(1727.0, 1000.0));
        assert_eq!(frame_rects(&dock.layout.root, screen).len(), 5);
        let close = |a: f32, b: f32| (a - b).abs() <= 1.0;
        let project = rect_of(&dock, screen, "project");
        assert_eq!(project.left(), 1.0);
        assert!(
            close(project.right(), 321.0) && close(project.bottom(), 701.0),
            "{project:?}"
        );
        let comp = rect_of(&dock, screen, "composition");
        assert!(
            close(comp.left(), 325.0) && close(comp.right(), 1407.0),
            "{comp:?}"
        );
        let timeline = rect_of(&dock, screen, "timeline");
        assert!(
            close(timeline.top(), 705.0) && close(timeline.right(), 1407.0),
            "{timeline:?}"
        );
        let preview = rect_of(&dock, screen, "preview");
        assert!(
            close(preview.left(), 1411.0) && close(preview.bottom(), 113.0),
            "{preview:?}"
        );
        let stack = rect_of(&dock, screen, "properties");
        assert!(
            close(stack.top(), 117.0) && close(stack.bottom(), 1000.0),
            "{stack:?}"
        );
    }

    #[test]
    fn move_to_edge_splits_and_cleans_up() {
        let mut dock = Dock::new(ae_default());
        let comp = dock.frame_of(&"composition".into()).unwrap();
        dock.move_panel(&"preview".into(), comp, DropZone::Right, None);
        // Preview's old frame is gone and a new one sits right of the comp;
        // the right column now holds only the stack.
        assert_eq!(dock.frame_ids().len(), 5);
        let preview = dock.frame_of(&"preview".into()).unwrap();
        assert_ne!(preview, comp);
        // Moving into the center adds a tab.
        dock.move_panel(&"preview".into(), comp, DropZone::Center, None);
        assert_eq!(dock.frame_of(&"preview".into()), Some(comp));
        assert_eq!(dock.frame(comp).unwrap().tabs.len(), 2);
        assert_eq!(dock.frame_ids().len(), 4);
    }

    #[test]
    fn layout_roundtrips_through_json() {
        let mut dock = Dock::new(ae_default());
        dock.undock(&["align".into()], None);
        let json = dock.layout.to_json();
        let back = Dock::from_layout(Layout::from_json(&json).unwrap());
        assert_eq!(back.frame_ids(), dock.frame_ids());
        assert_eq!(back.layout.floating.len(), 1);
        assert!(back.contains(&"align".into()));
        let stack = back.frame_of(&"properties".into()).unwrap();
        assert!(back.frame(stack).unwrap().stacked);
        let ids = back.layout.panel_ids();
        assert_eq!(ids.last().unwrap().as_str(), "align");
        assert!(ids.iter().any(|p| p.as_str() == "timeline"));
    }

    #[test]
    fn panel_id_kind_and_instance() {
        let id = PanelId::new("timeline:comp-1");
        assert_eq!(id.kind(), "timeline");
        assert_eq!(id.instance(), Some("comp-1"));
        let id = PanelId::new("project");
        assert_eq!(id.kind(), "project");
        assert_eq!(id.instance(), None);
    }

    fn run(ctx: &egui::Context, dock: &mut Dock, events: Vec<Event>) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1728.0, 1000.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            dock.show(ui, &mut Host);
        });
        out.textures_delta.clear();
    }

    fn press(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn dragging_a_splitter_resizes() {
        let ctx = egui::Context::default();
        let mut dock = Dock::new(ae_default());
        run(&ctx, &mut dock, vec![]);
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1728.0, 1000.0));
        let project = rect_of(&dock, screen, "project");
        let x = project.right() + 2.0;
        let y = project.center().y;
        run(&ctx, &mut dock, vec![Event::PointerMoved(pos2(x, y))]);
        run(&ctx, &mut dock, vec![press(pos2(x, y), true)]);
        for dx in [10.0, 30.0, 50.0] {
            run(&ctx, &mut dock, vec![Event::PointerMoved(pos2(x + dx, y))]);
        }
        run(&ctx, &mut dock, vec![press(pos2(x + 50.0, y), false)]);
        let after = rect_of(&dock, screen, "project");
        assert!(
            (after.right() - (project.right() + 50.0)).abs() <= 1.0,
            "{after:?}"
        );
    }

    #[test]
    fn dragging_a_tab_onto_a_frame_edge_redocks() {
        let ctx = egui::Context::default();
        let mut dock = Dock::new(ae_default());
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1728.0, 1000.0));
        run(&ctx, &mut dock, vec![]);
        let project = rect_of(&dock, screen, "project");
        let timeline = rect_of(&dock, screen, "timeline");
        let project_frame = dock.frame_of(&"project".into()).unwrap();
        // Second tab ("effect_controls") sits right of "project".
        let tab = dock.tab_rects(&ctx, project_frame);
        let grab = tab
            .get(1)
            .map(|(_, r)| r.center())
            .unwrap_or(project.left_top() + vec2(120.0, 12.0));
        run(&ctx, &mut dock, vec![Event::PointerMoved(grab)]);
        run(&ctx, &mut dock, vec![press(grab, true)]);
        let target = pos2(timeline.right() - 20.0, timeline.center().y);
        for t in [0.1, 0.5, 1.0] {
            let p = grab + (target - grab) * t;
            run(&ctx, &mut dock, vec![Event::PointerMoved(p)]);
        }
        run(&ctx, &mut dock, vec![press(target, false)]);
        run(&ctx, &mut dock, vec![]);
        let ec = dock.frame_of(&"effect_controls".into()).unwrap();
        let tl = dock.frame_of(&"timeline".into()).unwrap();
        assert_ne!(ec, tl);
        let ec_rect = rect_of(&dock, screen, "effect_controls");
        let tl_rect = rect_of(&dock, screen, "timeline");
        assert!(ec_rect.left() > tl_rect.right(), "{ec_rect:?} {tl_rect:?}");
        assert_eq!(
            dock.frame(project_frame).unwrap().tabs,
            vec![PanelId::from("project")]
        );
    }
}
