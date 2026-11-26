//! Load and prepare a user LUT before changing either AE menu or its project asset.
use std::sync::{Arc, mpsc};
use std::time::Duration;

use bae_effects::LumetriLut;
use bae_model::{PrimPath, Time, Value, effect::param_attr};
use bae_ui::{
    Invocation, PanelContext,
    files::{OpenRequest, SharedFilePicker},
    tr,
};
use serde_json::json;

pub(super) const BROWSE: &str = "motion.effect.browse_lumetri_lut";

#[derive(Debug)]
pub(crate) struct Load {
    target: PrimPath,
    slot: LumetriLut,
    result: mpsc::Receiver<Result<Vec<u8>, String>>,
}

pub(super) fn commit(
    cx: &mut PanelContext<'_>,
    path: &PrimPath,
    slot: LumetriLut,
    choice: i64,
    bytes: Option<Vec<u8>>,
) {
    let [first, second, asset] = slot.parameters();
    let edit = |id, value: Value| json!({"path":path,"attr":param_attr(id),"time": Time::ZERO,"value":value});
    let mut edits = vec![
        edit(first, Value::Int(choice)),
        edit(second, Value::Int(choice)),
    ];
    if let Some(bytes) = bytes {
        edits.push(edit(asset, Value::Bytes(bytes)));
    }
    cx.invoke_with("attr.set_many", json!({"edits":edits}));
}

pub(super) fn browse(inv: &Invocation, cx: &PanelContext<'_>) {
    let Some(target) = inv.params["path"]
        .as_str()
        .and_then(|p| PrimPath::new(p).ok())
    else {
        return;
    };
    let Some(slot) = inv.params["menu"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .and_then(LumetriLut::from_menu)
    else {
        return;
    };
    if !cx
        .stage
        .prim(&target)
        .is_some_and(|p| p.value("match_name").and_then(Value::as_str) == Some("ADBE Lumetri"))
    {
        return;
    }
    let request = OpenRequest {
        title: tr("Browse...").into(),
        filters: vec![("LUT".into(), vec!["cube".into()])],
        ..Default::default()
    };
    let Some(path) = SharedFilePicker::open(cx.resources, &request)
        .into_iter()
        .next()
    else {
        return;
    };
    let gpu = cx.resources.get::<Arc<bae_gpu::GpuContext>>().cloned();
    let (send, result) = mpsc::channel();
    match std::thread::Builder::new()
        .name("bae-lut-import".into())
        .spawn(move || {
            let _ = send.send(bae_effects::prepare_lumetri_lut(&path, gpu.as_ref()));
        }) {
        Ok(_) => {
            crate::common::session(cx.resources).lock().loading_lut = Some(Load {
                target,
                slot,
                result,
            })
        }
        Err(e) => crate::dialogs::alert(cx, "Lumetri", e.to_string()),
    }
}

/// A project owns its pending load. Opening a different project replaces the motion session.
pub(crate) fn poll(ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
    let session = crate::common::session(cx.resources);
    let mut state = session.lock();
    let Some(load) = state.loading_lut.as_ref() else {
        return false;
    };
    let result = match load.result.try_recv() {
        Ok(result) => result,
        Err(mpsc::TryRecvError::Empty) => {
            drop(state);
            egui::Modal::new(egui::Id::new(BROWSE)).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("Loading LUT…"));
                });
            });
            ctx.request_repaint_after(Duration::from_millis(33));
            return true;
        }
        Err(e) => Err(e.to_string()),
    };
    let load = state.loading_lut.take().unwrap();
    drop(state);
    match result {
        Ok(bytes) => commit(cx, &load.target, load.slot, 2, Some(bytes)),
        Err(message) => crate::dialogs::alert(cx, "Lumetri", message),
    }
    true
}
