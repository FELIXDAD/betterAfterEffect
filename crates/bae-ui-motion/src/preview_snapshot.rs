//! Captures immutable project versions for background rendering. Only document
//! handles and frontiers cross the UI thread. Rebuilding the read model happens
//! on the worker, including the stages reached by cross-project references.

use bae_model::loro::{ExportMode, Frontiers, LoroDoc};
use bae_model::{Stage, Sublayer, SublayerId};

pub(crate) struct PreviewSnapshot {
    layers: Vec<(SublayerId, LoroDoc, Frontiers)>,
    external: Vec<(String, Self)>,
}

impl PreviewSnapshot {
    pub(crate) fn take(stage: &Stage) -> Self {
        Self {
            layers: stage
                .sublayers()
                .map(|layer| {
                    let doc = layer.doc();
                    (layer.id().clone(), doc.clone(), doc.state_frontiers())
                })
                .collect(),
            external: stage
                .attached_projects()
                .map(|key| {
                    (
                        key.to_owned(),
                        Self::take(stage.attached_project(key).unwrap()),
                    )
                })
                .collect(),
        }
    }

    pub(crate) fn restore(&self) -> Result<Stage, String> {
        let layers = self
            .layers
            .iter()
            .map(|(id, doc, version)| {
                let bytes = doc
                    .export(ExportMode::StateOnly(Some(std::borrow::Cow::Borrowed(
                        version,
                    ))))
                    .map_err(|e| e.to_string())?;
                Sublayer::from_snapshot(id.clone(), &bytes).map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut stage = Stage::from_sublayers(layers).map_err(|e| e.to_string())?;
        for (name, project) in &self.external {
            stage.attach_project(name.clone(), project.restore()?);
        }
        Ok(stage)
    }
}
