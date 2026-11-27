//! Captures immutable project versions for background rendering. Only document
//! handles and frontiers cross the UI thread. Rebuilding the read model happens
//! on the worker, including the stages reached by cross-project references.

use std::collections::HashMap;

use bae_model::loro::{ExportMode, Frontiers, LoroDoc, PeerID};
use bae_model::{Stage, Sublayer, SublayerId};

/// A background thread's own copies of the project documents.
///
/// A whole-state export reads every container of the document while holding
/// its lock, and the UI thread reads the same document to draw. Restoring
/// through replicas takes only the operations added since the last restore
/// from the live document (a short read) and exports the state from the
/// replica, which nothing else touches.
#[derive(Default)]
pub(crate) struct Replicas {
    docs: HashMap<(SublayerId, PeerID), LoroDoc>,
}

impl Replicas {
    /// The state of `live` at `version`, exported from the replica of `live`.
    fn state(
        &mut self,
        id: &SublayerId,
        live: &LoroDoc,
        version: &Frontiers,
    ) -> Result<Vec<u8>, String> {
        let key = (id.clone(), live.peer_id());
        let at = || ExportMode::StateOnly(Some(std::borrow::Cow::Borrowed(version)));
        let synced = self.docs.get(&key).is_some_and(|replica| {
            live.export(ExportMode::updates(&replica.oplog_vv()))
                .is_ok_and(|ops| replica.import(&ops).is_ok())
        });
        if !synced {
            // First restore of this document (or the replica fell out of
            // step): start from the asked state.
            let bytes = live.export(at()).map_err(|e| e.to_string())?;
            let replica = LoroDoc::new();
            replica.import(&bytes).map_err(|e| e.to_string())?;
            self.docs.insert(key.clone(), replica);
            return Ok(bytes);
        }
        let replica = &self.docs[&key];
        match replica.export(at()) {
            Ok(bytes) => Ok(bytes),
            Err(_) => {
                self.docs.remove(&key);
                live.export(at()).map_err(|e| e.to_string())
            }
        }
    }
}

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

    /// Stable across reopening the same project. Includes linked project versions.
    pub(crate) fn disk_revision(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut state = std::collections::hash_map::DefaultHasher::new();
        for (id, _, frontiers) in &self.layers {
            id.hash(&mut state);
            let mut ids: Vec<_> = frontiers.iter().map(|id| (id.peer, id.counter)).collect();
            ids.sort_unstable();
            ids.hash(&mut state);
        }
        for (name, project) in &self.external {
            name.hash(&mut state);
            project.disk_revision().hash(&mut state);
        }
        state.finish()
    }

    pub(crate) fn restore(&self) -> Result<Stage, String> {
        self.restore_with(&mut Replicas::default())
    }

    /// [`Self::restore`] through a background thread's [`Replicas`]: after
    /// the first restore, the live documents are only asked for new
    /// operations.
    pub(crate) fn restore_with(&self, replicas: &mut Replicas) -> Result<Stage, String> {
        let mut used = Vec::new();
        let stage = self.restore_into(replicas, &mut used);
        // Documents of closed projects.
        replicas.docs.retain(|key, _| used.contains(key));
        stage
    }

    fn restore_into(
        &self,
        replicas: &mut Replicas,
        used: &mut Vec<(SublayerId, PeerID)>,
    ) -> Result<Stage, String> {
        let layers = self
            .layers
            .iter()
            .map(|(id, doc, version)| {
                used.push((id.clone(), doc.peer_id()));
                let bytes = replicas.state(id, doc, version)?;
                Sublayer::from_snapshot(id.clone(), &bytes).map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut stage = Stage::from_sublayers(layers).map_err(|e| e.to_string())?;
        for (name, project) in &self.external {
            stage.attach_project(name.clone(), project.restore_into(replicas, used)?);
        }
        Ok(stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::PrimPath;

    /// Restoring through replicas gives each snapshot's own version: after
    /// later edits, also for a snapshot taken before them.
    #[test]
    fn replicas_restore_each_snapshot_version() {
        let mut stage = Stage::new();
        let mut reg = bae_model::CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let author = bae_model::AuthorId::new("user:test").unwrap();
        let comp = reg
            .execute(
                &mut stage,
                &author,
                "comp.create",
                serde_json::json!({"name": "A"}),
            )
            .unwrap()
            .output;
        let comp = PrimPath::new(comp["path"].as_str().unwrap()).unwrap();
        let rename = |stage: &mut Stage, name: &str| {
            reg.execute(
                stage,
                &author,
                "attr.set",
                serde_json::json!({"path": comp, "attr": "name",
                                   "value": {"type": "string", "value": name}}),
            )
            .unwrap();
        };
        let name = |stage: &Stage| bae_motion::query::display_name_of(stage, &comp);
        let mut replicas = Replicas::default();
        let first = PreviewSnapshot::take(&stage);
        assert_eq!(name(&first.restore_with(&mut replicas).unwrap()), "A");
        rename(&mut stage, "B");
        let second = PreviewSnapshot::take(&stage);
        rename(&mut stage, "C");
        assert_eq!(name(&second.restore_with(&mut replicas).unwrap()), "B");
        assert_eq!(name(&first.restore_with(&mut replicas).unwrap()), "A");
        let third = PreviewSnapshot::take(&stage);
        assert_eq!(name(&third.restore_with(&mut replicas).unwrap()), "C");
        assert_eq!(name(&third.restore().unwrap()), "C");
    }
}
