//! Storyboard commands through the command registry, as the UI and agents run them.

use bae_model::{AuthorId, CommandRegistry, DynCommand, FrameRate, PrimPath, Stage, Time, Value};
use bae_storyboard::query::{self, ShotSize};
use serde_json::{Value as Json, json};

struct Fx {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn path_of(out: &Json, key: &str) -> PrimPath {
    p(out[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} in {out}")))
}

impl Fx {
    fn new() -> Self {
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        bae_storyboard::register_commands(&mut reg);
        Fx {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
        }
    }

    fn run(&mut self, id: &str, params: Json) -> Json {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    }

    fn try_run(&mut self, id: &str, params: Json) -> Result<Json, String> {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .map(|o| o.output)
            .map_err(|e| e.to_string())
    }

    fn board(&mut self, panels: u32) -> (PrimPath, Vec<PrimPath>) {
        let out = self.run("board.create", json!({"name": "sb01", "panels": panels}));
        let panels = out["panels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| p(v.as_str().unwrap()))
            .collect();
        (path_of(&out, "path"), panels)
    }

    fn codes(&self, board: &PrimPath) -> Vec<String> {
        query::board_panels(&self.stage, board)
            .iter()
            .map(|x| query::panel_info(&self.stage, x).unwrap().shot_code)
            .collect()
    }

    fn value(&self, path: &PrimPath, attr: &str) -> Option<Value> {
        query::static_value(&self.stage.prim(path).unwrap(), attr)
    }
}

const SEC: i64 = Time::FLICKS_PER_SECOND;

#[test]
fn twelve_panel_board_with_shot_numbers_and_frames() {
    let mut fx = Fx::new();
    let (board, panels) = fx.board(12);
    assert_eq!(board, p("/boards/sb01"));
    assert_eq!(panels.len(), 12);
    assert_eq!(panels[0], p("/boards/sb01/panels/sh010"));
    let codes = fx.codes(&board);
    assert_eq!(codes.first().unwrap(), "SH010");
    assert_eq!(codes.last().unwrap(), "SH120");
    let info = query::board_info(&fx.stage, &board).unwrap();
    assert_eq!(info.frame_rate, FrameRate::new(24, 1));
    assert_eq!((info.width, info.height), (1920, 1080));
    // Default 2 s per panel: 12 × 48 frames.
    assert_eq!(query::board_duration(&fx.stage, &board), Time(24 * SEC));
    // The whole board is one undo step.
    assert!(fx.stage.undo(&fx.author).is_some());
    assert!(query::boards(&fx.stage).is_empty());
}

#[test]
fn panels_insert_update_and_validate() {
    let mut fx = Fx::new();
    let (board, panels) = fx.board(2);
    let out = fx.run(
        "panel.create",
        json!({"board": board, "index": 1, "frames": 72, "shot_size": "ms",
               "dialogue": "12f  VO: every frame", "camera_move": "push in 100%→112%, 0–72f"}),
    );
    assert_eq!(out["shot_code"], "SH015");
    let new = path_of(&out, "path");
    assert_eq!(fx.codes(&board), ["SH010", "SH015", "SH020"]);
    let info = query::panel_info(&fx.stage, &new).unwrap();
    assert_eq!(info.duration, Time(3 * SEC));
    assert_eq!(info.shot_size, ShotSize::Ms);
    assert_eq!(info.camera_move, "push in 100%→112%, 0–72f");

    fx.run(
        "panel.update",
        json!({"panel": panels[0], "action": "0–12f title rises", "sound": "SFX: whoosh @0f",
               "transition": "dissolve 12f", "notes": "TBD: title copy",
               "duration": SEC / 2 + 1}),
    );
    let info = query::panel_info(&fx.stage, &panels[0]).unwrap();
    assert_eq!(info.action, "0–12f title rises");
    assert_eq!(info.transition, "dissolve 12f");
    assert_eq!(info.duration, Time(SEC / 2), "rounded to whole frames");

    // Shot numbers stay unique; frames and duration are exclusive; panels are ≥ 1 frame.
    assert!(
        fx.try_run(
            "panel.update",
            json!({"panel": panels[0], "shot_code": "SH020"})
        )
        .unwrap_err()
        .contains("already used")
    );
    assert!(
        fx.try_run(
            "panel.update",
            json!({"panel": panels[0], "frames": 2, "duration": SEC})
        )
        .is_err()
    );
    assert!(
        fx.try_run("panel.update", json!({"panel": panels[0], "frames": 0}))
            .is_err()
    );
    // The schema checks enum tokens for generic attr.set too.
    assert!(
        fx.try_run(
            "attr.set",
            json!({"path": panels[0], "attr": "shot_size", "value": {"type": "enum", "value": "huge"}})
        )
        .is_err()
    );
    // A picture must be an artboard or an asset.
    assert!(
        fx.try_run("panel.update", json!({"panel": panels[0], "image": board}))
            .is_err()
    );
}

#[test]
fn panels_move_between_and_within_boards_and_delete() {
    let mut fx = Fx::new();
    let (board, panels) = fx.board(3);
    let out = fx.run("panel.move", json!({"panels": [panels[2]], "index": 0}));
    assert_eq!(out["paths"][0], json!(panels[2]));
    assert_eq!(fx.codes(&board), ["SH030", "SH010", "SH020"]);

    let other = path_of(&fx.run("board.create", json!({"name": "sb02"})), "path");
    let out = fx.run(
        "panel.move",
        json!({"panels": [panels[0]], "index": 0, "board": other}),
    );
    let moved = p(out["paths"][0].as_str().unwrap());
    assert_eq!(moved, p("/boards/sb02/panels/sh010"));
    assert_eq!(fx.codes(&other), ["SH010"]);
    assert_eq!(fx.codes(&board), ["SH030", "SH020"]);

    fx.run("panel.delete", json!({"panels": [panels[1]]}));
    assert_eq!(fx.codes(&board), ["SH030"]);
    // Deleted numbers are not reused when appending.
    let out = fx.run("panel.create", json!({"board": board}));
    assert_eq!(out["shot_code"], "SH040");
}

#[test]
fn artboards_create_place_rearrange_and_delete() {
    let mut fx = Fx::new();
    let a = path_of(&fx.run("artboard.create", json!({})), "path");
    let b = path_of(
        &fx.run("artboard.create", json!({"size": [800.0, 600.0]})),
        "path",
    );
    let c = path_of(&fx.run("artboard.create", json!({"name": "Logo"})), "path");
    assert_eq!(a, p("/artboards/Artboard_1"));
    let info = |fx: &Fx, x: &PrimPath| query::artboard_info(&fx.stage, x).unwrap();
    assert_eq!(info(&fx, &a).name, "Artboard 1");
    assert_eq!(info(&fx, &b).name, "Artboard 2");
    // New artboards go 20 px to the right of the rightmost one.
    assert_eq!(info(&fx, &b).origin, [1940.0, 0.0]);
    assert_eq!(info(&fx, &c).origin, [2760.0, 0.0]);
    assert!(fx.stage.has_prim(&query::artwork_folder(&a)));

    fx.run(
        "artboard.rearrange",
        json!({"layout": "grid_by_row", "columns": 2, "spacing": 10.0}),
    );
    assert_eq!(info(&fx, &a).origin, [0.0, 0.0]);
    assert_eq!(info(&fx, &b).origin, [1930.0, 0.0]);
    assert_eq!(info(&fx, &c).origin, [0.0, 1090.0]);
    fx.run(
        "artboard.rearrange",
        json!({"layout": "column", "order": [c, a, b]}),
    );
    assert_eq!(
        query::artboards(&fx.stage),
        [c.clone(), a.clone(), b.clone()]
    );
    assert_eq!(info(&fx, &a).origin, [0.0, 1100.0]);

    fx.run(
        "artboard.update",
        json!({"artboard": b, "name": "Phone", "size": [390.0, 844.0]}),
    );
    assert_eq!(info(&fx, &b).name, "Phone");
    assert_eq!(info(&fx, &b).size, [390.0, 844.0]);
    assert!(
        fx.try_run(
            "artboard.update",
            json!({"artboard": b, "size": [0.0, 10.0]})
        )
        .is_err()
    );
    fx.run("artboard.delete", json!({"artboards": [b]}));
    assert_eq!(query::artboards(&fx.stage).len(), 2);
}

#[test]
fn animatic_has_one_clip_per_panel_back_to_back() {
    let mut fx = Fx::new();
    let (board, panels) = fx.board(3);
    fx.run("panel.update", json!({"panel": panels[1], "frames": 36}));
    let out = fx.run("board.make_animatic", json!({"board": board}));
    let seq = path_of(&out, "path");
    assert_eq!(seq, p("/sequences/sb01_Animatic"));
    assert_eq!(out["frames"], 48 + 36 + 48);
    let clips: Vec<PrimPath> = out["clips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| p(v.as_str().unwrap()))
        .collect();
    assert_eq!(clips.len(), 3);
    let start = |fx: &Fx, c: &PrimPath| fx.value(c, "start").unwrap().as_time().unwrap();
    assert_eq!(start(&fx, &clips[0]), Time::ZERO);
    assert_eq!(start(&fx, &clips[1]), Time(2 * SEC));
    assert_eq!(start(&fx, &clips[2]), Time(2 * SEC + 3 * SEC / 2));
    assert_eq!(
        fx.value(&clips[1], "source"),
        Some(Value::PrimRef(panels[1].clone()))
    );
    assert_eq!(
        fx.value(&seq, "frame_rate"),
        Some(Value::FrameRate(FrameRate::new(24, 1)))
    );
    assert_eq!(
        fx.value(&seq, query::attr::SOURCE_BOARD),
        Some(Value::PrimRef(board))
    );
}

#[test]
fn promoting_a_panel_copies_editable_artwork_and_links_the_shot() {
    let mut fx = Fx::new();
    let (board, panels) = fx.board(2);
    let art = fx.run("artboard.create", json!({"name": "SH010 drawing"}));
    let artboard = path_of(&art, "path");
    let artwork = path_of(&art, "artwork");
    // Artwork: a shape layer with a rectangle path (as the drawing tools will make).
    let layer = artwork.child("logo").unwrap();
    fx.run("prim.create", json!({"path": layer, "prim_type": "Layer"}));
    fx.run(
        "attr.set",
        json!({"path": layer, "attr": "kind", "value": {"type": "enum", "value": "shape"}}),
    );
    fx.run(
        "attr.set",
        json!({"path": layer, "attr": "parent", "value": {"type": "prim_ref", "value": layer}}),
    );
    fx.run(
        "panel.update",
        json!({"panel": panels[0], "image": artboard, "frames": 72}),
    );

    let out = fx.run("panel.promote_to_comp", json!({"panel": panels[0]}));
    assert_eq!(out["created"], true);
    let comp = path_of(&out, "comp");
    let shot = path_of(&out, "shot");
    assert_eq!(comp, p("/comps/SH010"));
    assert_eq!(shot, p("/shots/SH010"));
    let info = bae_motion::query::comp_info(&fx.stage, &comp).unwrap();
    assert_eq!((info.width, info.height), (1920, 1080));
    assert_eq!(info.frame_rate, FrameRate::new(24, 1));
    assert_eq!(info.duration, Time(3 * SEC));
    let layers = bae_motion::query::comp_layers(&fx.stage, &comp);
    assert_eq!(layers, [p("/comps/SH010/layers/logo")]);
    // References inside the copied artwork point at the copy.
    assert_eq!(
        fx.value(&layers[0], "parent"),
        Some(Value::PrimRef(layers[0].clone()))
    );
    // Links: panel → shot → comp, and back.
    assert_eq!(query::panel_comp(&fx.stage, &panels[0]), Some(comp.clone()));
    assert_eq!(query::shot_of_comp(&fx.stage, &comp), Some(shot.clone()));
    assert_eq!(query::shot_panels(&fx.stage, &shot), [panels[0].clone()]);
    assert_eq!(
        query::panel_picture(&fx.stage, &panels[0]),
        Some(comp.clone())
    );
    assert_eq!(query::panel_picture(&fx.stage, &panels[1]), None);
    // The original artwork is untouched; promoting again returns the same comp.
    assert!(fx.stage.has_prim(&layer));
    let again = fx.run("panel.promote_to_comp", json!({"panel": panels[0]}));
    assert_eq!(again["created"], false);
    assert_eq!(path_of(&again, "comp"), comp);
    // The animatic clip carries the shot.
    let anim = fx.run("board.make_animatic", json!({"board": board}));
    let clip = p(anim["clips"][0].as_str().unwrap());
    assert_eq!(fx.value(&clip, "shot"), Some(Value::PrimRef(shot)));
}

#[test]
fn module_publishes_commands_with_schemas() {
    let mut b = bae_app::AppBuilder::new();
    b.add_module(bae_storyboard::StoryboardModule);
    let mut app = b.build();
    let reg = app.registry::<dyn DynCommand>();
    for id in [
        bae_storyboard::ids::ARTBOARD_CREATE,
        bae_storyboard::ids::BOARD_CREATE,
        bae_storyboard::ids::PANEL_UPDATE,
        bae_storyboard::ids::BOARD_MAKE_ANIMATIC,
        bae_storyboard::ids::PANEL_PROMOTE_TO_COMP,
    ] {
        assert!(reg.contains(id), "{id}");
    }
    // Flattened panel fields show up in the parameter schema agents read.
    let info = reg.get("panel.update").unwrap().info().clone();
    let props = &info.params_schema["properties"];
    for field in [
        "panel",
        "shot_code",
        "frames",
        "shot_size",
        "camera_move",
        "transition",
    ] {
        assert!(props.get(field).is_some(), "{field} in {props}");
    }
}
