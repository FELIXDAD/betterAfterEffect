//! Lumetri secondary curve controls in the Effect Controls panel.
use std::sync::Arc;

use crate::common::Edit;
use bae_effects::LumetriCurve;
use bae_model::Value;
use bae_ui::a11y::Accessible as _;
use bae_ui::color_picker::{self, ColorEdit};
use bae_ui::{theme::color, tr};
use egui::{Id, Pos2, Sense, Stroke, Ui, pos2, vec2};

#[derive(Clone)]
struct Preview {
    points: Vec<[f64; 2]>,
    samples: Arc<[f64; 256]>,
}

pub(super) fn picker(ui: &mut Ui, id: Id, value: Option<&Value>, label: &str) -> Edit {
    let mut rgba = match value {
        Some(Value::Color(rgba)) => *rgba,
        _ => [0., 0., 0., 1.],
    };
    if let ColorEdit::Commit(c) =
        color_picker::color_button(ui, id, &mut rgba, vec2(32., 16.), label, label)
    {
        return Edit::Commit(Value::Color(c));
    }
    if let Some([r, g, b]) = color_picker::eyedropper_button(ui, id.with("dropper")) {
        return Edit::Commit(Value::Color([r, g, b, 1.]));
    }
    Edit::None
}

pub(super) fn graph(
    ui: &mut Ui,
    id: Id,
    curve: LumetriCurve,
    value: Option<&Value>,
    label: &str,
) -> Edit {
    let width = ui.available_width().clamp(24., 320.);
    let (_, rect) = ui.allocate_space(vec2(width, 70.));
    let response = ui.interact(rect, id, Sense::click_and_drag())
        .labeled(egui::WidgetType::Other, label)
        .on_hover_text(tr("Click to add a point. Drag to adjust. Shift-drag vertically. Ctrl/Cmd-click to remove."));
    if !ui.is_rect_visible(rect) && !response.dragged() && !response.drag_stopped() {
        return Edit::None;
    }
    let plot = rect.shrink(4.);
    let mut points = curve.points(value);
    let range = points
        .iter()
        .map(|p| p[1].abs())
        .fold(if curve.id() == 106 { 0.5 } else { 1.0 }, f64::max);
    // Imported points can extend the scale. Keep the axis fixed for the whole drag.
    let scale_id = id.with("drag-scale");
    let range = if response.drag_started() {
        ui.data_mut(|d| d.insert_temp(scale_id, range));
        range
    } else if response.dragged() || response.drag_stopped() {
        ui.data(|d| d.get_temp::<f64>(scale_id)).unwrap_or(range)
    } else {
        range
    };
    let screen = |[x, y]: [f64; 2]| {
        pos2(
            plot.left() + x as f32 * plot.width(),
            plot.center().y - (y / range) as f32 * plot.height() * 0.5,
        )
    };
    let samples = ui.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<Option<Preview>>(id.with("samples"));
        if let Some(preview) = cache
            && preview.points == points
        {
            return Arc::clone(&preview.samples);
        }
        let samples = Arc::new(curve.samples(&points));
        *cache = Some(Preview {
            points: points.clone(),
            samples: Arc::clone(&samples),
        });
        samples
    });
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2., color::PANEL);
    painter.hline(
        plot.x_range(),
        plot.center().y,
        Stroke::new(1., color::DIVIDER),
    );
    let line = (0..=64)
        .map(|i| {
            let k = (i * 255 + 32) / 64;
            screen([k as f64 / 255., samples[k]])
        })
        .collect();
    painter.add(egui::Shape::line(line, Stroke::new(1.5, color::ACCENT)));
    for &point in &points {
        painter.circle_filled(screen(point), 3., color::ICON);
    }

    let nearest = |at: Pos2| {
        points
            .iter()
            .enumerate()
            .map(|(i, &p)| (i, screen(p).distance_sq(at)))
            .filter(|(_, d)| *d <= 7. * 7.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let Some(cursor) = response.interact_pointer_pos() else {
        return Edit::None;
    };
    let x = f64::from(((cursor.x - plot.left()) / plot.width()).clamp(0., 1.));
    let y = f64::from(((plot.center().y - cursor.y) * 2. / plot.height()).clamp(-1., 1.)) * range;
    let selection = id.with("drag");
    if response.drag_started() {
        let start = ui
            .input(|input| input.pointer.press_origin())
            .unwrap_or(cursor);
        ui.data_mut(|d| d.insert_temp(selection, nearest(start)));
    }
    if response.clicked() {
        if ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
            let Some(i) = nearest(cursor) else {
                return Edit::None;
            };
            points.remove(i);
        } else {
            let axis_x = if curve.is_hue_axis() {
                x.rem_euclid(1.)
            } else {
                x
            };
            if nearest(cursor).is_some() || points.iter().any(|p| (p[0] - axis_x).abs() < 1e-6) {
                return Edit::None;
            }
            points.push([axis_x, y]);
            points.sort_by(|a, b| a[0].total_cmp(&b[0]));
        }
        return Edit::Commit(LumetriCurve::value(&points));
    }
    if response.dragged() || response.drag_stopped() {
        let Some(i) = ui
            .data(|d| d.get_temp::<Option<usize>>(selection))
            .flatten()
            .filter(|&i| i < points.len())
        else {
            return Edit::None;
        };
        let lo = i.checked_sub(1).map_or(0., |j| points[j][0] + 1e-6);
        let end = if curve.is_hue_axis() { 1. - 1e-6 } else { 1. };
        let hi = points.get(i + 1).map_or(end, |p| p[0] - 1e-6);
        if lo <= hi && !ui.input(|input| input.modifiers.shift) {
            points[i][0] = x.clamp(lo, hi);
        }
        points[i][1] = y;
        let value = LumetriCurve::value(&points);
        return if response.drag_stopped() {
            Edit::Commit(value)
        } else {
            Edit::Live(value)
        };
    }
    Edit::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, RawInput, Rect};

    #[test]
    fn pointer_drag_previews_then_commits_and_command_click_removes_the_point() {
        let ctx = egui::Context::default();
        let id = Id::new("curve test");
        let curve = LumetriCurve::from_param(113).unwrap();
        let original = curve.pick([0., 1., 0.], None);
        let frame = |mut events: Vec<Event>, modifiers, value: &Value| {
            events.insert(0, Event::ModifiersChanged(modifiers));
            let raw = RawInput {
                events,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500., 200.))),
                ..Default::default()
            };
            let mut edit = Edit::None;
            let mut output = ctx.run_ui(raw, |ui| {
                edit = graph(ui, id, curve, Some(value), "Hue vs Sat");
            });
            output.textures_delta.clear();
            edit
        };
        let none = Modifiers::default();
        let press = |pos, pressed, modifiers| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        frame(vec![], none, &original);
        let plot = ctx.read_response(id).unwrap().rect.shrink(4.);
        let center = pos2(plot.left() + plot.width() / 3., plot.center().y);
        let up = center - vec2(0., plot.height() / 4.);
        frame(
            vec![Event::PointerMoved(center), press(center, true, none)],
            none,
            &original,
        );
        let Edit::Live(preview) = frame(vec![Event::PointerMoved(up)], none, &original) else {
            panic!("missing drag preview");
        };
        let Edit::Commit(commit) = frame(vec![press(up, false, none)], none, &preview) else {
            panic!("missing completed edit");
        };
        let knots = curve.points(Some(&commit));
        assert_eq!(knots.len(), 3);
        assert!((knots[1][0] - 1. / 3.).abs() < 1e-5 && (knots[1][1] - 0.5).abs() < 1e-5);
        let command = Modifiers {
            command: true,
            ctrl: true,
            ..Default::default()
        };
        frame(vec![press(up, true, command)], command, &commit);
        let Edit::Commit(removed) = frame(vec![press(up, false, command)], command, &commit) else {
            panic!("missing point removal");
        };
        assert_eq!(curve.points(Some(&removed)), vec![knots[0], knots[2]]);
    }
}
