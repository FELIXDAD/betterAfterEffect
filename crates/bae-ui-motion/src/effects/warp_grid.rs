//! Editable normalized displacement lattice for Mesh Warp.
use crate::common::Edit;
use bae_model::Value;
use bae_ui::theme::color;
use egui::{Pos2, Sense, Ui, Vec2, pos2, vec2};

fn read_field(value: Option<&Value>, cols: usize, rows: usize) -> Vec<Vec2> {
    let mut field = vec![Vec2::ZERO; cols * rows];
    let Some(Value::Bytes(bytes)) = value else {
        return field;
    };
    let [w, h, ..] = bytes.as_slice() else {
        return field;
    };
    let (w, h) = (usize::from(*w), usize::from(*h));
    if !(2..=32).contains(&w) || !(2..=32).contains(&h) || bytes.len() != 2 + 8 * w * h {
        return field;
    }
    let at = |x: usize, y: usize| {
        let offset = 2 + 8 * (y * w + x);
        let a = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let b = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
        if a.is_finite() && b.is_finite() {
            vec2(a, b)
        } else {
            Vec2::ZERO
        }
    };
    for (i, delta) in field.iter_mut().enumerate() {
        let x = (i % cols * (w - 1)) as f32 / (cols - 1) as f32;
        let y = (i / cols * (h - 1)) as f32 / (rows - 1) as f32;
        let (ix, iy) = (x as usize, y as usize);
        let nx = (ix + 1).min(w - 1);
        let ny = (iy + 1).min(h - 1);
        let top = at(ix, iy) * (1. - x.fract()) + at(nx, iy) * x.fract();
        let bottom = at(ix, ny) * (1. - x.fract()) + at(nx, ny) * x.fract();
        *delta = top * (1. - y.fract()) + bottom * y.fract();
    }
    field
}

pub(super) fn edit(ui: &mut Ui, value: Option<&Value>, cells: [usize; 2]) -> Edit {
    let [cols, rows] = cells.map(|n| n.clamp(1, 31) + 1);
    let mut field = read_field(value, cols, rows);
    let width = ui.available_width().clamp(48., 300.);
    let (rect, response) = ui.allocate_exact_size(vec2(width, width * 0.5625), Sense::drag());
    let plot = rect.shrink(4.);
    let base = |i: usize| {
        vec2(
            (i % cols) as f32 / (cols - 1) as f32,
            (i / cols) as f32 / (rows - 1) as f32,
        )
    };
    let nodes: Vec<Pos2> = field
        .iter()
        .enumerate()
        .map(|(i, d)| plot.min + plot.size() * (base(i) + *d))
        .collect();
    let painter = ui.painter_at(rect);
    let stroke = egui::Stroke::new(1., color::DIVIDER);
    for (i, p) in nodes.iter().copied().enumerate() {
        if i % cols > 0 {
            painter.line_segment([nodes[i - 1], p], stroke);
        }
        if i >= cols {
            painter.line_segment([nodes[i - cols], p], stroke);
        }
        painter.circle_filled(p, 2., color::ICON);
    }
    let Some(cursor) = response.interact_pointer_pos() else {
        return Edit::None;
    };
    let selection = response.id.with("vertex");
    if response.drag_started() {
        let start = ui
            .input(|input| input.pointer.press_origin())
            .unwrap_or(cursor);
        let idx = nodes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.distance_sq(start).total_cmp(&b.distance_sq(start)))
            .map_or(0, |(i, _)| i);
        ui.data_mut(|data| data.insert_temp(selection, idx));
    }
    if !(response.dragged() || response.drag_stopped()) {
        return Edit::None;
    }
    if let Some(i) = ui.data(|data| data.get_temp::<usize>(selection))
        && i < field.len()
    {
        let p = pos2(cursor.x, cursor.y);
        field[i] = (p - plot.min) / plot.size() - base(i);
    }
    let mut bytes = Vec::with_capacity(2 + field.len() * 8);
    bytes.extend([cols as u8, rows as u8]);
    for d in field {
        bytes.extend(d.x.to_le_bytes());
        bytes.extend(d.y.to_le_bytes());
    }
    if response.drag_stopped() {
        Edit::Commit(Value::Bytes(bytes))
    } else {
        Edit::Live(Value::Bytes(bytes))
    }
}
