//! UI responsiveness on a heavy project: how long the UI thread takes per
//! frame (input, layout, painting the panels, dispatching what they asked
//! for) while scrubbing, scrolling the timeline, clicking layers, opening
//! context menus and dragging a layer in the viewer. A benchmark, run in an
//! optimized build:
//!
//! `cargo test --release -p bae --bin bae responsiveness -- --ignored --nocapture`
//!
//! `BAE_HEAVY_VIDEO=<file.mp4>` makes 40 of the layers video layers.

use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::kittest::Queryable as _;
use serde_json::json;

use super::*;

const LAYERS: usize = 200;

/// A comp with [`LAYERS`] layers: video (or solids), text with expressions,
/// shapes with expressions and keyframes, solids with effects, adjustment
/// layers and nulls parenting others.
fn heavy_project(h: &mut Ui) {
    new_comp(h);
    let comp = h.state().active_comp().expect("comp");
    let video = std::env::var_os("BAE_HEAVY_VIDEO").map(std::path::PathBuf::from);
    let asset = video.as_ref().map(|path| {
        let source = bae_media::MediaFormats::builtin()
            .find(path)
            .and_then(|f| f.open(path))
            .expect("open BAE_HEAVY_VIDEO");
        let attrs: serde_json::Map<String, serde_json::Value> = source
            .info()
            .asset_attributes()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), serde_json::to_value(v).unwrap()))
            .collect();
        h.state_mut().invoke(Invocation::with_params(
            "item.import",
            json!({"files": [{"path": path.to_string_lossy(), "attrs": attrs}]}),
        ));
        settle(h);
        let assets = PrimPath::new(bae_motion::query::ASSETS_FOLDER).unwrap();
        h.state().stage().children(&assets)[0].clone()
    });
    let create = |h: &mut Ui, params: serde_json::Value| {
        let mut params = params;
        params["comp"] = json!(comp);
        h.state_mut()
            .invoke(Invocation::with_params("layer.create", params));
    };
    for i in 0..LAYERS {
        let (x, y) = (
            100.0 + (i * 137 % 1720) as f64,
            80.0 + (i * 71 % 920) as f64,
        );
        match i % 5 {
            0 if asset.is_some() => create(
                h,
                json!({"kind": "footage", "source": asset, "name": format!("Video {i}")}),
            ),
            0 | 3 => create(
                h,
                json!({"kind": "solid", "name": format!("Solid {i}"), "size": [320, 180],
                       "color": [0.2, 0.4 + (i % 7) as f64 * 0.05, 0.8, 1.0]}),
            ),
            1 => create(
                h,
                json!({"kind": "text", "name": format!("Title {i}"),
                       "text": format!("Heavy project line {i}"), "font_size": 48.0,
                       "position": [x, y]}),
            ),
            2 => create(
                h,
                json!({"kind": "shape", "name": format!("Shape {i}"),
                       "shape": (["rect", "ellipse", "star"][i % 3]),
                       "color": [1.0, 0.6, 0.2, 1.0], "position": [x, y]}),
            ),
            _ => create(
                h,
                json!({"kind": if i % 2 == 0 { "adjustment" } else { "null" },
                       "name": format!("Control {i}")}),
            ),
        }
    }
    settle(h);
    let layers = h.state().layers();
    assert!(layers.len() >= LAYERS, "{} layers", layers.len());
    let names: Vec<(PrimPath, String)> = layers
        .iter()
        .map(|l| (l.clone(), h.state().name(l)))
        .collect();
    let named = |prefix: &str| -> Vec<PrimPath> {
        names
            .iter()
            .filter(|(_, n)| n.starts_with(prefix))
            .map(|(l, _)| l.clone())
            .collect()
    };
    let solids = named("Solid ");
    for effect in ["ADBE Gaussian Blur 2", "ADBE Drop Shadow", "ADBE Glo2"] {
        h.state_mut().invoke(Invocation::with_params(
            "effect.apply",
            json!({"layers": solids, "match_name": effect}),
        ));
    }
    let sec = bae_model::Time::FLICKS_PER_SECOND;
    for layer in named("Title ") {
        h.state_mut().invoke(Invocation::with_params(
            "attr.set_expression",
            json!({"path": layer, "attr": "transform:position", "source": "wiggle(2, 40)"}),
        ));
    }
    for layer in named("Shape ") {
        h.state_mut().invoke(Invocation::with_params(
            "attr.set_expression",
            json!({"path": layer, "attr": "transform:rotation", "source": "time * 45 + index"}),
        ));
        h.state_mut().invoke(Invocation::with_params(
            "attr.set_animated",
            json!({"path": layer, "attr": "transform:scale", "animated": true, "time": 0}),
        ));
        h.state_mut().invoke(Invocation::with_params(
            "attr.set_at",
            json!({"path": layer, "attr": "transform:scale", "time": 3 * sec,
                   "value": {"type": "vec3", "value": [160.0, 160.0, 100.0]}}),
        ));
    }
    settle(h);
    let failed: Vec<_> = h
        .state()
        .outcomes
        .iter()
        .filter(|(_, o)| o.starts_with("failed"))
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}

/// Frame times of one interaction.
struct Times {
    name: &'static str,
    frames: Vec<Duration>,
}

impl Times {
    fn new(name: &'static str) -> Self {
        Times {
            name,
            frames: Vec::new(),
        }
    }

    /// One frame with `events` as its input.
    fn frame(&mut self, h: &mut Ui, events: impl IntoIterator<Item = Event>) {
        h.input_mut().events.extend(events);
        let t = Instant::now();
        frame(h);
        self.frames.push(t.elapsed());
    }

    fn report(&self) -> String {
        let mut ms: Vec<f64> = self.frames.iter().map(|d| d.as_secs_f64() * 1e3).collect();
        ms.sort_by(f64::total_cmp);
        let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
        format!(
            "{:<10} {:>4} frames  p50 {:>7.2} ms  p99 {:>7.2} ms  max {:>7.2} ms",
            self.name,
            ms.len(),
            at(0.5),
            at(0.99),
            ms[ms.len() - 1]
        )
    }
}

fn button(pos: Pos2, pressed: bool, button: PointerButton) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Timeline rows of the layers painted in the last frame (name cells).
fn rows(h: &Ui) -> Vec<Rect> {
    let head = timeline_header(h).expect("timeline");
    find_texts(h, |t| {
        ["Video ", "Solid ", "Title ", "Shape ", "Control "]
            .iter()
            .any(|p| t.starts_with(p))
    })
    .into_iter()
    .filter(|r| r.top() > head.bottom() && r.left() < head.left() + 400.0)
    .collect()
}

#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn ui_frame_times_on_a_heavy_project() {
    let mut app = Gui::new();
    if !app.with_gpu() {
        return;
    }
    app.app.insert_resource(std::sync::Arc::new(
        bae_gpu::viewport::ViewportTextureRegistry::new(),
    ));
    let mut h = gui_with(app);
    let t = Instant::now();
    heavy_project(&mut h);
    let built = t.elapsed();
    pause(&mut h);
    let mut report = vec![format!(
        "{} layers, built in {built:.2?}",
        h.state().layers().len()
    )];

    // Idle: nothing changes (the window repaints for an animation or a timer).
    let mut idle = Times::new("idle");
    for _ in 0..60 {
        idle.frame(&mut h, []);
    }
    report.push(idle.report());

    // Scrub: drag the current-time indicator along the time ruler.
    let head = timeline_header(&h).expect("timeline");
    let tick = find_texts(&h, |t| t == "02s" || t == "02:00f")
        .into_iter()
        .find(|r| r.left() > head.right())
        .unwrap_or_else(|| panic!("no ruler label in {:?}", painted_texts(&h)));
    // From just left of the 2 s label to the right along the ruler.
    let (y, x0) = (tick.center().y, tick.center().x - 120.0);
    let mut scrub = Times::new("scrub");
    let start = egui::pos2(x0, y);
    scrub.frame(&mut h, [Event::PointerMoved(start)]);
    scrub.frame(&mut h, [button(start, true, PointerButton::Primary)]);
    for i in 1..=120 {
        let x = x0 + (i % 60) as f32 * 8.0;
        scrub.frame(&mut h, [Event::PointerMoved(egui::pos2(x, y))]);
    }
    let moved = h.state().now();
    scrub.frame(
        &mut h,
        [button(egui::pos2(x0, y), false, PointerButton::Primary)],
    );
    assert!(moved > bae_model::Time::ZERO, "the scrub moved the time");
    report.push(scrub.report());
    pause(&mut h);

    // Scroll the layer list down and back up.
    let first = rows(&h).first().copied().expect("layer rows");
    let over = first.center() + egui::vec2(100.0, 40.0);
    let mut scroll = Times::new("scroll");
    scroll.frame(&mut h, [Event::PointerMoved(over)]);
    for i in 0..120 {
        let dy = if i < 60 { -60.0 } else { 60.0 };
        scroll.frame(
            &mut h,
            [Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    report.push(scroll.report());
    pause(&mut h);

    // Click layers one after another (selection: Effect Controls, viewer
    // handles, timeline highlight).
    let mut click = Times::new("click");
    for i in 0..30 {
        let rows = rows(&h);
        let at = rows[i % rows.len()].center();
        click.frame(&mut h, [Event::PointerMoved(at)]);
        click.frame(&mut h, [button(at, true, PointerButton::Primary)]);
        click.frame(&mut h, [button(at, false, PointerButton::Primary)]);
        for _ in 0..3 {
            click.frame(&mut h, []);
        }
    }
    report.push(click.report());
    pause(&mut h);

    // Right-click a layer (its context menu) and close it with Escape.
    let mut menu = Times::new("menu");
    for i in 0..20 {
        let rows = rows(&h);
        let at = rows[(i * 3) % rows.len()].center();
        menu.frame(&mut h, [Event::PointerMoved(at)]);
        menu.frame(&mut h, [button(at, true, PointerButton::Secondary)]);
        menu.frame(&mut h, [button(at, false, PointerButton::Secondary)]);
        for _ in 0..3 {
            menu.frame(&mut h, []);
        }
        menu.frame(&mut h, key_events(Modifiers::NONE, Key::Escape));
        menu.frame(&mut h, []);
    }
    report.push(menu.report());
    pause(&mut h);

    // Drag the selected layer in the Composition viewer.
    let target = h.state().layers()[0].clone();
    h.state().session().lock().selected_layers = vec![target];
    settle(&mut h);
    let canvas = h
        .root()
        .query_all_by_label("Composition")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("composition viewer");
    let from = canvas.center();
    let mut drag = Times::new("drag");
    drag.frame(&mut h, [Event::PointerMoved(from)]);
    drag.frame(&mut h, [button(from, true, PointerButton::Primary)]);
    for i in 1..=90 {
        let d = egui::vec2((i % 30) as f32 * 4.0, (i % 15) as f32 * 3.0);
        drag.frame(&mut h, [Event::PointerMoved(from + d)]);
    }
    drag.frame(&mut h, [button(from, false, PointerButton::Primary)]);
    report.push(drag.report());

    pause(&mut h);

    // Nudge the selected layers with the arrow keys: one edit per press.
    let some: Vec<PrimPath> = h.state().layers().into_iter().take(5).collect();
    h.state().session().lock().selected_layers = some;
    settle(&mut h);
    let mut nudge = Times::new("nudge");
    for i in 0..30 {
        let k = if i % 2 == 0 {
            Key::ArrowRight
        } else {
            Key::ArrowDown
        };
        for e in key_events(Modifiers::NONE, k) {
            nudge.frame(&mut h, [e]);
        }
        nudge.frame(&mut h, []);
    }
    report.push(nudge.report());

    // Undo and redo those edits.
    let mut undo = Times::new("undo/redo");
    for i in 0..40 {
        let m = if i < 20 {
            Modifiers::COMMAND
        } else {
            Modifiers::COMMAND | Modifiers::SHIFT
        };
        for e in key_events(m, Key::Z) {
            undo.frame(&mut h, [e]);
        }
        undo.frame(&mut h, []);
    }
    report.push(undo.report());

    // Every layer selected with its effects revealed (E), then scroll.
    key(&mut h, Modifiers::COMMAND, Key::A);
    let mut reveal = Times::new("reveal E");
    for e in key_events(Modifiers::NONE, Key::E) {
        reveal.frame(&mut h, [e]);
    }
    for _ in 0..10 {
        reveal.frame(&mut h, []);
    }
    let over = timeline_header(&h).expect("timeline").center() + egui::vec2(100.0, 80.0);
    reveal.frame(&mut h, [Event::PointerMoved(over)]);
    for i in 0..60 {
        let dy = if i < 30 { -120.0 } else { 120.0 };
        reveal.frame(
            &mut h,
            [Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    report.push(reveal.report());

    println!("UI frame times, heavy project:\n  {}", report.join("\n  "));
}
