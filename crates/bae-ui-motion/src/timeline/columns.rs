//! Timeline columns, in AE's menu order. Headers, rows and drag handles use the same bounds.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub(crate) enum Column {
    Av,
    Label,
    Index,
    Name,
    Comment,
    Switches,
    Modes,
    Parent,
    Keys,
    RenderTime,
    In,
    Out,
    Duration,
    Stretch,
}

impl Column {
    pub(crate) const ALL: [Self; 14] = [
        Self::Av,
        Self::Label,
        Self::Index,
        Self::Name,
        Self::Comment,
        Self::Switches,
        Self::Modes,
        Self::Parent,
        Self::Keys,
        Self::RenderTime,
        Self::In,
        Self::Out,
        Self::Duration,
        Self::Stretch,
    ];
    fn spec(self) -> (&'static str, f32, f32) {
        match self {
            Self::Av => ("A/V Features", 64.0, 64.0),
            Self::Label => ("Label", 18.0, 18.0),
            Self::Index => ("#", 28.0, 22.0),
            Self::Name => ("Source Name", 138.0, 65.0),
            Self::Comment => ("Comment", 120.0, 46.0),
            Self::Switches => ("Switches", 8.0 * SW + 14.0, 8.0 * SW),
            Self::Modes => ("Modes", modes::MODES_W, modes::MODES_W),
            Self::Parent => ("Parent & Link", 120.0, 84.0),
            Self::Keys => ("Keys", 55.0, 45.0),
            Self::RenderTime => ("Render Time", 88.0, 55.0),
            Self::In => ("In", 84.0, 38.0),
            Self::Out => ("Out", 84.0, 38.0),
            Self::Duration => ("Duration", 84.0, 38.0),
            Self::Stretch => ("Stretch", 65.0, 40.0),
        }
    }
    pub(crate) fn title(self) -> &'static str {
        self.spec().0
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Columns {
    pub(super) layer_names: bool,
    shown: BTreeMap<Column, bool>,
    widths: BTreeMap<Column, f32>,
}

impl Columns {
    pub(crate) fn visible(&self, c: Column) -> bool {
        c == Column::Name
            || self.shown.get(&c).copied().unwrap_or(matches!(
                c,
                Column::Av | Column::Label | Column::Index | Column::Switches | Column::Parent
            ))
    }
    pub(super) fn toggle(&mut self, c: Column) {
        if c != Column::Name {
            self.shown.insert(c, !self.visible(c));
        }
    }
    pub(super) fn toggle_switches_modes(&mut self) {
        let switches = self.visible(Column::Switches);
        let modes = self.visible(Column::Modes);
        self.shown.insert(Column::Switches, !switches || modes);
        self.shown.insert(Column::Modes, switches && !modes);
    }
    pub(super) fn layout(&self) -> Cols {
        let mut edges = [(0.0, 0.0); 14];
        let mut x = 5.0;
        for c in Column::ALL {
            if c == Column::Label {
                x += 15.0;
            } // The disclosure triangle always has its own cell.
            let (_, default, min) = c.spec();
            let width = if self.visible(c) {
                self.widths
                    .get(&c)
                    .copied()
                    .unwrap_or(default)
                    .clamp(min, 640.0)
            } else {
                0.0
            };
            edges[c as usize] = (x, width);
            x += width;
        }
        Cols { edges, end: x }
    }

    pub(super) fn header_controls(&mut self, ui: &Ui, row: Rect, cols: &Cols) {
        let outline = Rect::from_min_max(row.min, pos2(row.left() + cols.end, row.bottom()));
        let header = ui.interact(outline, ui.id().with("timeline_columns"), Sense::click());
        let on = header
            .ctx
            .input(|i| i.pointer.interact_pos())
            .map(|p| p.x - row.left());
        if header.clicked()
            && on.is_some_and(|x| cols.x(Column::Name) <= x && x < cols.right(Column::Name))
        {
            self.layer_names = !self.layer_names;
        }
        if header.secondary_clicked() {
            ui.data_mut(|d| {
                d.insert_temp(
                    header.id.with("target"),
                    on.and_then(|x| {
                        Column::ALL
                            .into_iter()
                            .find(|&c| cols.x(c) <= x && x < cols.right(c))
                    }),
                )
            });
        }
        let target = ui.data(|d| {
            d.get_temp::<Option<Column>>(header.id.with("target"))
                .flatten()
        });
        crate::common::context_menu(&header, &mut |ui| {
            if ui
                .add_enabled(
                    target.is_some_and(|c| c != Column::Name),
                    egui::Button::new(tr("Hide This")),
                )
                .clicked()
            {
                if let Some(c) = target {
                    self.toggle(c);
                }
                ui.close();
            }
            ui.add_enabled(false, egui::Button::new(tr("Rename This...")));
            ui.separator();
            crate::common::submenu(ui, tr("Columns"), &mut |ui| {
                for c in Column::ALL {
                    let mut on = self.visible(c);
                    if ui
                        .add_enabled(
                            c != Column::Name,
                            egui::Checkbox::new(&mut on, tr(c.title())),
                        )
                        .changed()
                    {
                        self.toggle(c);
                        ui.close();
                    }
                }
            });
        });
        for c in Column::ALL.into_iter().filter(|&c| cols.visible(c)) {
            let x = row.left() + cols.right(c);
            ui.painter()
                .vline(x - 0.5, row.y_range(), Stroke::new(1.0, color::SEPARATOR));
            let handle = Rect::from_center_size(pos2(x, row.center().y), vec2(6.0, row.height()));
            let resp = ui
                .interact(
                    handle,
                    ui.id().with(("column_width", c as usize)),
                    Sense::drag(),
                )
                .on_hover_cursor(CursorIcon::ResizeHorizontal);
            if resp.dragged() {
                let width = (cols.width(c) + resp.drag_delta().x).clamp(c.spec().2, 640.0);
                self.widths.insert(c, width);
            }
        }
    }
}

pub(super) struct Cols {
    edges: [(f32, f32); 14],
    pub(super) end: f32,
}
impl Cols {
    pub(super) fn x(&self, c: Column) -> f32 {
        self.edges[c as usize].0
    }
    pub(super) fn width(&self, c: Column) -> f32 {
        self.edges[c as usize].1
    }
    pub(super) fn right(&self, c: Column) -> f32 {
        self.x(c) + self.width(c)
    }
    pub(super) fn visible(&self, c: Column) -> bool {
        self.width(c) > 0.0
    }
    pub(super) fn extra_cells(
        &self,
        ui: &Ui,
        stage: &Stage,
        info: &CompInfo,
        layer: &PrimPath,
        row: Rect,
    ) {
        let times = [Column::In, Column::Out, Column::Duration, Column::Stretch];
        if !times.iter().any(|&c| self.visible(c)) {
            return;
        }
        let timing = query::layer_timing(stage, layer);
        let first = timing.in_point.min(timing.out_point);
        let end = timing.in_point.max(timing.out_point);
        for c in times.into_iter().filter(|&c| self.visible(c)) {
            let text = match c {
                Column::In => common::timecode(info, first),
                Column::Out => common::timecode(info, (end - info.frame_duration()).max(first)),
                Column::Duration => common::duration_code(info, end - first),
                Column::Stretch => format!("{}%", timing.stretch),
                _ => unreachable!(),
            };
            widgets::clipped_text(
                ui.painter(),
                pos2(row.left() + self.x(c) + 3.0, row.center().y),
                &text,
                theme::small(),
                color::TEXT,
                self.width(c) - 6.0,
            );
        }
    }
}
