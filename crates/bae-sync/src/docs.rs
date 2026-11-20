//! [`DocSet`]: the documents a [`crate::SyncSession`] synchronizes.

use std::collections::BTreeMap;

use bae_model::loro::{ExportMode, LoroDoc, VersionVector};
use bae_model::{Stage, StageEvent};

use crate::error::SyncError;
use crate::protocol::DocKey;

/// Outcome of [`DocSet::import`].
#[derive(Debug)]
pub enum Imported {
    /// The set does not hold this document; the session buffers the data until it
    /// does (e.g. a sublayer a collaborator created that is not in the local stack
    /// yet).
    UnknownDoc,
    /// Imported. Stage documents report what changed.
    Done(Option<StageEvent>),
}

/// A set of Loro documents keyed by [`DocKey`] that a session can read versions
/// from, export from and import into.
///
/// Implemented for [`Stage`] (its sublayers), [`MemDocs`] (plain documents, used by
/// the hub relay) and by [`crate::history`] wrappers that add the history document.
pub trait DocSet {
    /// Keys of the documents this set synchronizes.
    fn doc_keys(&self) -> Vec<DocKey>;
    /// Version of `doc`, or `None` if the set does not hold it.
    fn version(&self, doc: &DocKey) -> Option<VersionVector>;
    /// Ops of `doc` not in `since`.
    fn export_updates(&self, doc: &DocKey, since: &VersionVector) -> Result<Vec<u8>, SyncError>;
    /// Full snapshot of `doc`, history included.
    fn export_snapshot(&self, doc: &DocKey) -> Result<Vec<u8>, SyncError>;
    /// Import updates or a snapshot of `doc`.
    fn import(&mut self, doc: &DocKey, data: &[u8]) -> Result<Imported, SyncError>;

    /// Import several payloads of `doc` at once; each comes with its version (the
    /// sender had every op up to it). The default imports them one by one,
    /// skipping payloads the document already contains, and merges the events.
    /// Implementations override it when one batch is cheaper (one mirror rebuild).
    fn import_batch(
        &mut self,
        doc: &DocKey,
        batch: &[(VersionVector, Vec<u8>)],
    ) -> Result<Imported, SyncError> {
        let mut merged: Option<StageEvent> = None;
        for (version, data) in batch {
            if self
                .version(doc)
                .is_some_and(|have| have.includes_vv(version))
            {
                continue;
            }
            match self.import(doc, data)? {
                Imported::UnknownDoc => return Ok(Imported::UnknownDoc),
                Imported::Done(Some(ev)) => match &mut merged {
                    Some(m) => m.changes.extend(ev.changes),
                    None => merged = Some(ev),
                },
                Imported::Done(None) => {}
            }
        }
        Ok(Imported::Done(merged))
    }
}

impl DocSet for Stage {
    fn doc_keys(&self) -> Vec<DocKey> {
        self.layer_stack().iter().map(DocKey::sublayer).collect()
    }

    fn version(&self, doc: &DocKey) -> Option<VersionVector> {
        Some(self.sublayer(doc.as_sublayer()?)?.version())
    }

    fn export_updates(&self, doc: &DocKey, since: &VersionVector) -> Result<Vec<u8>, SyncError> {
        Ok(stage_layer(self, doc)?.export_updates(since)?)
    }

    fn export_snapshot(&self, doc: &DocKey) -> Result<Vec<u8>, SyncError> {
        Ok(stage_layer(self, doc)?.export_snapshot()?)
    }

    fn import(&mut self, doc: &DocKey, data: &[u8]) -> Result<Imported, SyncError> {
        match doc.as_sublayer() {
            Some(id) if self.sublayer(id).is_some() => {
                Ok(Imported::Done(Some(self.import_updates(id, data)?)))
            }
            _ => Ok(Imported::UnknownDoc),
        }
    }

    /// One Loro batch import, one mirror rebuild, one event.
    fn import_batch(
        &mut self,
        doc: &DocKey,
        batch: &[(VersionVector, Vec<u8>)],
    ) -> Result<Imported, SyncError> {
        match doc.as_sublayer() {
            Some(id) if self.sublayer(id).is_some() => {
                let blobs: Vec<Vec<u8>> = batch.iter().map(|(_, d)| d.clone()).collect();
                Ok(Imported::Done(Some(self.import_updates_batch(id, &blobs)?)))
            }
            _ => Ok(Imported::UnknownDoc),
        }
    }
}

fn stage_layer<'s>(stage: &'s Stage, doc: &DocKey) -> Result<&'s bae_model::Sublayer, SyncError> {
    doc.as_sublayer()
        .and_then(|id| stage.sublayer(id))
        .ok_or_else(|| SyncError::InvalidDocKey(doc.to_string()))
}

/// Plain Loro documents, created on demand for every key that arrives. The hub
/// relay stores the project this way; it never needs the prim mirror.
#[derive(Debug, Default)]
pub struct MemDocs {
    docs: BTreeMap<DocKey, LoroDoc>,
}

impl MemDocs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, doc: &DocKey) -> Option<&LoroDoc> {
        self.docs.get(doc)
    }

    /// Load a document from a snapshot (e.g. the hub's persisted state).
    pub fn load(&mut self, doc: DocKey, snapshot: &[u8]) -> Result<(), SyncError> {
        self.entry(doc).import(snapshot)?;
        Ok(())
    }

    fn entry(&mut self, doc: DocKey) -> &LoroDoc {
        self.docs.entry(doc).or_default()
    }
}

impl DocSet for MemDocs {
    fn doc_keys(&self) -> Vec<DocKey> {
        self.docs.keys().cloned().collect()
    }

    fn version(&self, doc: &DocKey) -> Option<VersionVector> {
        Some(self.docs.get(doc)?.oplog_vv())
    }

    fn export_updates(&self, doc: &DocKey, since: &VersionVector) -> Result<Vec<u8>, SyncError> {
        let d = self
            .docs
            .get(doc)
            .ok_or_else(|| SyncError::InvalidDocKey(doc.to_string()))?;
        Ok(d.export(ExportMode::updates(since))?)
    }

    fn export_snapshot(&self, doc: &DocKey) -> Result<Vec<u8>, SyncError> {
        let d = self
            .docs
            .get(doc)
            .ok_or_else(|| SyncError::InvalidDocKey(doc.to_string()))?;
        Ok(d.export(ExportMode::Snapshot)?)
    }

    fn import(&mut self, doc: &DocKey, data: &[u8]) -> Result<Imported, SyncError> {
        self.entry(doc.clone()).import(data)?;
        Ok(Imported::Done(None))
    }
}
