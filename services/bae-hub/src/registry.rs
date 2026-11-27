//! The hub's projects: `<data>/projects/<id>/`, loaded on first connection and
//! unloaded again after a while without connections.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::HubConfig;
use crate::error::HubError;
use crate::project::{PeerStatus, Project, ProjectId};

/// A project's slot: its lock serializes loading and unloading that project
/// without blocking the others.
type Slot = Arc<Mutex<Option<Arc<Project>>>>;

/// One entry of the project list.
#[derive(Clone, Debug, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    /// In memory right now.
    pub loaded: bool,
    /// Documents (sublayers and history) the hub holds; only known while loaded.
    pub docs: Option<usize>,
    pub peers: Vec<PeerStatus>,
    /// Size of the project directory.
    pub disk_bytes: u64,
    /// `false` when the project directory could not be written recently.
    pub saving: bool,
    /// Documents whose edits are relayed but not saved (only while loaded):
    /// the hub's own log of them is of a newer format version, written by a
    /// newer bae-hub on this data directory (spec 4.3). Run that one again.
    pub not_saved: Vec<String>,
    /// Protocol messages received from / sent to clients since loaded.
    pub messages_received: u64,
    pub messages_sent: u64,
}

pub(crate) struct Registry {
    root: PathBuf,
    config: Arc<HubConfig>,
    slots: Mutex<HashMap<ProjectId, Slot>>,
}

impl Registry {
    pub fn new(config: Arc<HubConfig>) -> Result<Self, HubError> {
        let root = config.projects_dir();
        fs::create_dir_all(&root).map_err(HubError::io(&root))?;
        Ok(Registry {
            root,
            config,
            slots: Mutex::new(HashMap::new()),
        })
    }

    fn slot(&self, id: &ProjectId) -> Slot {
        self.slots
            .lock()
            .expect("slots lock")
            .entry(id.clone())
            .or_default()
            .clone()
    }

    fn all_slots(&self) -> Vec<(ProjectId, Slot)> {
        self.slots
            .lock()
            .expect("slots lock")
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// The running project `id`, loading it if needed (blocks while it loads).
    /// A missing project is created when `create` is set (write access),
    /// otherwise it is an error.
    pub fn open(&self, id: &ProjectId, create: bool) -> Result<Arc<Project>, HubError> {
        let slot = self.slot(id);
        let mut guard = slot.lock().expect("slot lock");
        if let Some(p) = guard.as_ref() {
            if p.is_running() {
                return Ok(p.clone());
            }
            tracing::warn!(project = %id, "project relay stopped unexpectedly; reloading");
        }
        if let Some(old) = guard.take() {
            old.shutdown();
        }
        let dir = self.root.join(id.as_str());
        if !dir.is_dir() {
            if !create {
                return Err(HubError::NoSuchProject(id.to_string()));
            }
            fs::create_dir_all(&dir).map_err(HubError::io(&dir))?;
            tracing::info!(project = %id, "project created");
        }
        let project = Project::open(id.clone(), dir, &self.config)?;
        *guard = Some(project.clone());
        Ok(project)
    }

    /// Unload projects nobody has been connected to for `idle`.
    pub fn unload_idle(&self, idle: Duration) {
        let now = Instant::now();
        for (id, slot) in self.all_slots() {
            // Someone is loading it right now: not idle.
            let Ok(mut guard) = slot.try_lock() else {
                continue;
            };
            let expired = guard.as_ref().is_some_and(|p| {
                p.status()
                    .idle_since
                    .is_some_and(|t| now.saturating_duration_since(t) >= idle)
            });
            if expired && let Some(p) = guard.take() {
                p.shutdown();
                tracing::info!(project = %id, "idle project unloaded");
            }
        }
    }

    /// Stop every project (hub shutdown).
    pub fn shutdown(&self) {
        for (_, slot) in self.all_slots() {
            let mut guard = slot.lock().expect("slot lock");
            if let Some(p) = guard.take() {
                p.shutdown();
            }
        }
    }

    /// The running project `id`, if loaded (and not being loaded right now).
    fn loaded(&self, id: &ProjectId) -> Option<Arc<Project>> {
        let slot = self.slots.lock().expect("slots lock").get(id).cloned()?;
        let guard = slot.try_lock().ok()?;
        guard.as_ref().filter(|p| p.is_running()).cloned()
    }

    /// Every project on disk, sorted by ID.
    pub fn list(&self) -> Result<Vec<ProjectSummary>, HubError> {
        Ok(scan(&self.root)?
            .into_iter()
            .map(|(id, disk_bytes)| {
                let status = self.loaded(&id).map(|p| p.status());
                ProjectSummary {
                    id: id.to_string(),
                    loaded: status.is_some(),
                    docs: status.as_ref().map(|s| s.docs),
                    peers: status.as_ref().map(|s| s.peers.clone()).unwrap_or_default(),
                    disk_bytes,
                    saving: status.as_ref().is_none_or(|s| s.store_ok),
                    not_saved: status
                        .as_ref()
                        .map(|s| s.not_saved.clone())
                        .unwrap_or_default(),
                    messages_received: status.as_ref().map_or(0, |s| s.messages_received),
                    messages_sent: status.as_ref().map_or(0, |s| s.messages_sent),
                }
            })
            .collect())
    }

    /// Number of loaded projects and connected clients.
    pub fn counts(&self) -> (usize, usize) {
        let mut projects = 0;
        let mut peers = 0;
        for (id, _) in self.all_slots() {
            if let Some(p) = self.loaded(&id) {
                projects += 1;
                peers += p.status().peers.len();
            }
        }
        (projects, peers)
    }
}

fn scan(root: &Path) -> Result<Vec<(ProjectId, u64)>, HubError> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root).map_err(HubError::io(root))? {
        let Ok(entry) = entry else { continue };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        if let Ok(id) = ProjectId::new(&name) {
            out.push((id, dir_size(&entry.path(), 0)));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn dir_size(dir: &Path, depth: usize) -> u64 {
    if depth > 16 {
        return 0;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path(), depth + 1),
            Ok(t) if t.is_file() => e.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}
