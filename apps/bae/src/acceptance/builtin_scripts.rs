use super::*;
use bae_model::{PrimType, Time};
use bae_motion::query;
use egui_kittest::kittest::Queryable as _;
use serde_json::json;

fn panel_gui() -> Ui {
    let app = crate::app_builder().build();
    assert!(
        app.get_registry::<dyn bae_gpu::WarmUp>()
            .unwrap()
            .get("bae.script.builtin_panels")
            .is_some()
    );
    crate::shell::builtin_scripts::load(app.resource::<bae_plugin::PluginRuntime>().unwrap())
        .unwrap();
    gui_with(Gui::with_app(app))
}

fn click_button(h: &mut Ui, title: &str) {
    let center = h.get_by_label(bae_ui::tr(title)).rect().center();
    click_at(h, center);
    settle(h);
}

#[test]
fn path_null_buttons_bind_evaluated_vertices_and_undo_in_one_step() {
    let mut h = panel_gui();
    let run =
        |h: &mut Ui, id: &str, args| h.state_mut().dispatch(Invocation::with_params(id, args));
    run(
        &mut h,
        "comp.create",
        json!({"name":"Paths","width":640,"height":360}),
    );
    let comp = query::comps(h.state().stage())[0].clone();
    h.state().session().lock().open_comp(&comp);
    run(&mut h, "layer.create", json!({"comp":comp,"kind":"solid"}));
    let solid = h.state().layers()[0].clone();
    run(
        &mut h,
        "mask.create",
        json!({"layer":solid,"kind":"rect","position":[200,150],"size":[200,100]}),
    );
    let mask = h.state().stage().children(&solid.child("masks").unwrap())[0].clone();
    run(
        &mut h,
        "shape.add",
        json!({"comp":comp,"kind":"rect","bezier":true,"size":[90,50]}),
    );
    let shape = h
        .state()
        .layers()
        .into_iter()
        .find(|p| p != &solid)
        .unwrap();
    let group = h
        .state()
        .stage()
        .children(&shape.child("contents").unwrap())[0]
        .clone();
    let path = h
        .state()
        .stage()
        .children(&group.child("contents").unwrap())
        .into_iter()
        .find(|p| h.state().stage().prim(p).unwrap().is_a(&PrimType::PATH))
        .unwrap();
    assert!(
        h.state_mut()
            .menu("menu.window.create_nulls_from_paths_jsx")
    );
    settle(&mut h);
    for (title, count) in [
        ("Points Follow Nulls", 8),
        ("Nulls Follow Points", 8),
        ("Trace Path", 2),
    ] {
        h.state().session().lock().selected_props =
            vec![(mask.clone(), "path".into()), (path.clone(), "path".into())];
        click_button(&mut h, title);
        assert_eq!(
            h.state().layers().len(),
            2 + count,
            "{title}: {:?}",
            h.state().outcomes
        );
        let eval = |h: &Ui, t| {
            bae_anim::Evaluator::new()
                .evaluate_comp(h.state().stage(), &comp, Time::from_seconds(t))
                .unwrap()
        };
        let frame = eval(&h, 0.0);
        assert!(frame.errors.is_empty(), "{title}: {:?}", frame.errors);
        let vertex = frame.layer(&solid).unwrap().masks[0].shape.vertices[0];
        let position = frame
            .layer(&solid)
            .unwrap()
            .world_matrix
            .transform_point3(bae_anim::glam::DVec3::new(vertex[0], vertex[1], 0.0));
        let node = frame
            .layers
            .iter()
            .find(|l| {
                l.name.starts_with(&*frame.layer(&solid).unwrap().name) && l.name.ends_with("[0]")
            })
            .or_else(|| {
                frame.layers.iter().find(|l| {
                    l.name
                        .starts_with(&format!("Trace {}:", frame.layer(&solid).unwrap().name))
                })
            })
            .unwrap();
        assert!(
            (node.transform.position.truncate() - position.truncate()).length() < 0.1,
            "{title}: {:?} vs {position:?}",
            node.transform.position
        );
        if title == "Trace Path" {
            let later = eval(&h, 0.25);
            assert!(later.errors.is_empty(), "{:?}", later.errors);
            assert_ne!(
                later.layer(&node.path).unwrap().transform.position,
                node.transform.position
            );
        } else if title == "Points Follow Nulls" {
            run(
                &mut h,
                "attr.set",
                json!({"path":node.path,"attr":"transform:position","value":{"type":"vec3","value":[300,140,0]}}),
            );
            let moved = eval(&h, 0.0);
            assert_ne!(
                moved.layer(&solid).unwrap().masks[0].shape.vertices[0],
                vertex,
                "position {:?}, vertices {:?}, errors {:?}, last command {:?}",
                moved.layer(&node.path).unwrap().transform.position,
                moved.layer(&solid).unwrap().masks[0].shape.vertices,
                moved.errors,
                h.state().outcomes.last()
            );
            h.state_mut().dispatch(Invocation::new(ids::UNDO));
        }
        h.state_mut().dispatch(Invocation::new(ids::UNDO));
        settle(&mut h);
        assert_eq!(h.state().layers().len(), 2);
        assert!(
            query::attr_spec(h.state().stage(), &mask, "path")
                .unwrap()
                .expression
                .is_none()
        );
    }
    assert!(
        h.state_mut()
            .menu("menu.window.create_nulls_from_paths_jsx")
    );
    settle(&mut h);
    assert!(h.query_by_label(bae_ui::tr("Trace Path")).is_none());
}
