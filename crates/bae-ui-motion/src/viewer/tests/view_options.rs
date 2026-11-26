//! View Options, Panel Background Color, Custom Resolution and Create Camera
//! from 3D View.

use super::*;

/// View Options, Panel Background Color, Resolution ▸ Custom… and Create
/// Camera from 3D View.
#[test]
fn view_options_background_custom_resolution_and_camera_from_view() {
    use crate::dialogs::Dialog;
    use crate::viewer::view_options::{Background, ids as vo};
    use crate::viewer::view3d::{View3d, ViewCamera, ids};
    let mut h = H::new();
    let open = |h: &H| crate::dialogs::shared(&h.resources).lock().open.take();
    tool_action(&h, vo::VIEW_OPTIONS, json!({}));
    assert!(matches!(open(&h), Some(Dialog::ViewOptions(v)) if v.handles && v.masks));
    // Panel Background Color: the pasteboard and the menu's check mark.
    tool_action(&h, vo::PANEL_BACKGROUND, json!({"color": "white"}));
    assert_eq!(
        h.session().lock().panel_background.choice,
        Background::White
    );
    assert!(crate::viewer::view_options::background_checked(
        &h.resources,
        "white"
    ));
    assert!(!crate::viewer::view_options::background_checked(
        &h.resources,
        "medium_gray"
    ));
    // Custom Resolution: every 2nd pixel across, every 3rd down.
    tool_action(&h, vo::CUSTOM_RESOLUTION, json!({}));
    assert!(matches!(
        open(&h),
        Some(Dialog::CustomResolution {
            steps: [1, 1],
            nested: false
        })
    ));
    let mut requests = PanelRequests::default();
    {
        let pid = PanelId::new("composition");
        let mut cx = PanelContext::new(
            &pid,
            "Composition",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        crate::viewer::set_resolution(&mut cx, &h.session(), [2, 3], false);
    }
    h.apply(&requests);
    assert_eq!(crate::viewer::comp_steps(&h.stage, &h.comp), [2, 3]);
    // Create Camera from 3D View: only away from Active Camera; the camera
    // stands where Custom View 1 looks from.
    assert!(!crate::viewer::view3d::can_create_camera(&h.resources));
    tool_action(&h, ids::SWITCH, json!({"view": "custom_1"}));
    assert!(crate::viewer::view3d::can_create_camera(&h.resources));
    let r = tool_action(&h, ids::CREATE_CAMERA, json!({}));
    h.apply(&r);
    let camera = query::comp_layers(&h.stage, &h.comp)
        .into_iter()
        .find(|l| query::layer_kind(&h.stage, l).as_deref() == Some("camera"))
        .expect("a camera");
    let size = DVec2::new(640.0, 360.0);
    let ViewCamera::Perspective {
        position, target, ..
    } = ViewCamera::initial(View3d::Custom(1), size)
    else {
        panic!("perspective");
    };
    assert!(close3(
        h.v3(&camera, "transform:position"),
        position.to_array()
    ));
    assert!(close3(
        h.v3(&camera, "transform:point_of_interest"),
        target.to_array()
    ));
}
