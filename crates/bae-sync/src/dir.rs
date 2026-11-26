//! Shared-folder transport (`docs/spec/sync.md` §4): no server, the project
//! directory on a NAS share is the remote.
//!
//! Every client appends only to its own `updates/<doc>/<client-id>.log` and reads
//! everyone else's. Logs are sequences of checksummed records; the first record is
//! a JSON header, the rest are encoded [`Message::Updates`] / [`Message::Snapshot`].
//! A client compacts only its own log (rewrite as one snapshot, then rename), so
//! concurrent compactions never interfere.

use std::collections::{BTreeMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use bae_model::loro::{LoroDoc, VersionVector};
use serde::{Deserialize, Serialize};

use crate::client::ClientId;
use crate::error::SyncError;
use crate::protocol::{DocKey, Message};
use crate::transport::Transport;

/// `format` of a log header.
pub const LOG_FORMAT: &str = "bae.sync.log";
/// Log format version written by this build.
pub const LOG_VERSION: u32 = 1;

const UPDATES: &str = "updates";
const PRESENCE: &str = "presence";
const PRESENCE_EXT: &str = "presence";
/// Presence files untouched this long are deleted by anyone (crashed clients).
const PRESENCE_GC: Duration = Duration::from_secs(24 * 3600);
const SUBLAYERS: &str = "sublayers";
const LOG_EXT: &str = "log";
const SNAPSHOT_EXT: &str = "loro";
/// `len: u32` + 8 checksum bytes.
const RECORD_HEADER: usize = 12;
/// Larger lengths mean a corrupt record.
const MAX_RECORD: usize = 1 << 30;
const MAX_HEADER: usize = 64 * 1024;
const MAX_DEPTH: usize = 16;
/// `client` of the `Hello`s the folder sends.
const FOLDER_CLIENT: &str = "shared-folder";

/// Settings of a [`DirTransport`].
#[derive(Clone, Debug)]
pub struct DirConfig {
    /// How often to look for changes. File-system notifications (feature `watch`)
    /// only make polls happen sooner; they are unreliable on SMB.
    pub poll_interval: Duration,
    /// Own logs smaller than this are never compacted.
    pub compact_min_bytes: u64,
    /// Compact when the own log is this many times larger than right after the
    /// last compaction.
    pub compact_ratio: u64,
    /// Every this many polls, re-open every log even if its size and modification
    /// time look unchanged (coarse timestamps, stale SMB attribute caches).
    pub full_check_every: u32,
    /// Watch the folder for changes (needs the `watch` feature; ignored without).
    pub watch: bool,
}

impl Default for DirConfig {
    fn default() -> Self {
        DirConfig {
            poll_interval: Duration::from_secs(1),
            compact_min_bytes: 1 << 20,
            compact_ratio: 4,
            full_check_every: 30,
            watch: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LogHeader {
    format: String,
    version: u32,
    doc: String,
    client: String,
    generation: String,
}

/// Snapshot requests for a document whose own log fell behind: the first goes
/// out at once, later ones (the request was lost, or the rewrite failed too)
/// back off, because the session exports a whole snapshot for each on the thread
/// that owns the stage, and a folder that is down may stay down for a while.
#[derive(Debug)]
struct Behind {
    next_request: Instant,
    delay: Duration,
}

/// Longest wait between two snapshot requests for a log that fell behind.
const BEHIND_MAX_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug)]
struct OwnLog {
    path: PathBuf,
    generation: String,
    /// Bytes written (and verified) so far.
    len: u64,
    /// Size right after the last compaction.
    compacted_len: u64,
    snapshot_requested: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceKind {
    /// Another client's log.
    Log,
    /// `sublayers/<doc>.loro` (written by `Project::save`) or `history.loro`.
    Base,
}

#[derive(Debug)]
struct Source {
    doc: DocKey,
    kind: SourceKind,
    generation: Option<String>,
    /// Logs: read position (a record boundary).
    offset: u64,
    /// `(len, mtime)` when last read.
    seen: Option<(u64, Option<SystemTime>)>,
}

/// Size, records and problems of one shared folder, for diagnostics and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirStats {
    pub polls: u64,
    pub records_read: u64,
    pub records_written: u64,
    pub compactions: u64,
    /// Own logs that had to be rewritten because they were changed underneath us.
    pub repairs: u64,
}

/// The shared-folder [`Transport`]. Its I/O is synchronous and may be slow on a
/// NAS: run it on a background thread with [`crate::ThreadedTransport`].
pub struct DirTransport {
    root: PathBuf,
    client: ClientId,
    config: DirConfig,
    /// Held while attached (one session per client ID). `None`: read-only.
    lock: Option<File>,
    own: BTreeMap<DocKey, OwnLog>,
    sources: BTreeMap<PathBuf, Source>,
    /// Union of the versions of everything read from or written to the folder.
    folder: BTreeMap<DocKey, VersionVector>,
    /// Documents whose own log lacks ops the session counts as written, because
    /// a write failed. Nothing is appended to them until the session's snapshot
    /// rewrites the log (see [`DirTransport::fell_behind`]).
    behind: BTreeMap<DocKey, Behind>,
    inbox: VecDeque<Message>,
    scanned: bool,
    poll_now: bool,
    last_poll: Option<Instant>,
    wake: Arc<AtomicBool>,
    last_error: Option<String>,
    stats: DirStats,
    /// `(len, mtime)` of other clients' presence files when last read.
    presence_seen: BTreeMap<PathBuf, (u64, Option<SystemTime>)>,
    #[cfg(feature = "watch")]
    _watcher: Option<notify::RecommendedWatcher>,
}

impl std::fmt::Debug for DirTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirTransport")
            .field("root", &self.root)
            .field("client", &self.client)
            .field("docs", &self.folder.len())
            .finish_non_exhaustive()
    }
}

impl DirTransport {
    /// Attach to the `.bae` project directory `project` as `client`.
    ///
    /// Takes an advisory lock on `updates/<client>.lock` so two sessions on one
    /// machine cannot share a client ID ([`SyncError::ClientIdInUse`]). File
    /// systems without locking are accepted as they are.
    pub fn open(
        project: impl AsRef<Path>,
        client: ClientId,
        config: DirConfig,
    ) -> Result<Self, SyncError> {
        let root = project.as_ref().to_owned();
        if !root.is_dir() {
            return Err(SyncError::NotADirectory(root));
        }
        let updates = root.join(UPDATES);
        fs::create_dir_all(&updates).map_err(SyncError::io(&updates))?;
        let presence = root.join(PRESENCE);
        fs::create_dir_all(&presence).map_err(SyncError::io(&presence))?;
        let lock_path = updates.join(format!("{client}.lock"));
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(SyncError::io(&lock_path))?;
        match lock.try_lock() {
            Ok(()) | Err(fs::TryLockError::Error(_)) => {}
            Err(fs::TryLockError::WouldBlock) => return Err(SyncError::ClientIdInUse(client)),
        }
        Ok(Self::new(root, client, config, Some(lock)))
    }

    /// Attach to `project` without writing anything, for a folder whose
    /// `updates/` this user cannot write ([`DirTransport::open`] fails with
    /// [`SyncError::Io`] of kind `PermissionDenied` or `ReadOnlyFilesystem`).
    /// Everyone's logs are read as usual, this client's own (from earlier
    /// sessions) like the others'. Nothing is written: sending edits
    /// (`Updates`, `Snapshot`) fails, our presence is not published, no lock is
    /// taken. Run the session with [`crate::SessionConfig::read_only`].
    pub fn open_read_only(
        project: impl AsRef<Path>,
        client: ClientId,
        config: DirConfig,
    ) -> Result<Self, SyncError> {
        let root = project.as_ref().to_owned();
        if !root.is_dir() {
            return Err(SyncError::NotADirectory(root));
        }
        Ok(Self::new(root, client, config, None))
    }

    fn new(root: PathBuf, client: ClientId, config: DirConfig, lock: Option<File>) -> Self {
        #[allow(unused_mut)]
        let mut t = DirTransport {
            root,
            client,
            config,
            lock,
            own: BTreeMap::new(),
            sources: BTreeMap::new(),
            folder: BTreeMap::new(),
            behind: BTreeMap::new(),
            inbox: VecDeque::new(),
            scanned: false,
            poll_now: true,
            last_poll: None,
            wake: Arc::new(AtomicBool::new(false)),
            last_error: None,
            stats: DirStats::default(),
            presence_seen: BTreeMap::new(),
            #[cfg(feature = "watch")]
            _watcher: None,
        };
        #[cfg(feature = "watch")]
        if t.config.watch {
            t.start_watcher();
        }
        t
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Opened with [`open_read_only`](Self::open_read_only).
    pub fn is_read_only(&self) -> bool {
        self.lock.is_none()
    }

    pub fn client(&self) -> &ClientId {
        &self.client
    }

    pub fn stats(&self) -> &DirStats {
        &self.stats
    }

    /// The most recent I/O problem that was skipped (unreadable file, network
    /// hiccup). Polling continues regardless.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Path of this client's log for `doc`.
    pub fn log_path(&self, doc: &DocKey) -> PathBuf {
        self.doc_dir(doc).join(format!("{}.{LOG_EXT}", self.client))
    }

    /// Look for changes on the next [`Transport::recv`] instead of waiting for the
    /// poll interval.
    pub fn poll_soon(&self) {
        self.wake.store(true, Ordering::Relaxed);
    }

    fn doc_dir(&self, doc: &DocKey) -> PathBuf {
        self.root.join(UPDATES).join(doc.as_str())
    }

    #[cfg(feature = "watch")]
    fn start_watcher(&mut self) {
        use notify::{RecursiveMode, Watcher};
        let wake = self.wake.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok() {
                wake.store(true, Ordering::Relaxed);
            }
        });
        match watcher {
            Ok(mut w) => {
                for dir in [
                    self.root.join(UPDATES),
                    self.root.join(SUBLAYERS),
                    self.root.join(PRESENCE),
                ] {
                    if dir.is_dir()
                        && let Err(e) = w.watch(&dir, RecursiveMode::Recursive)
                    {
                        self.last_error = Some(format!("watch {}: {e}", dir.display()));
                    }
                }
                self._watcher = Some(w);
            }
            Err(e) => self.last_error = Some(format!("file watcher: {e}")),
        }
    }

    // ---- reading ----

    fn poll(&mut self) {
        self.stats.polls += 1;
        let full = self.config.full_check_every > 0
            && self
                .stats
                .polls
                .is_multiple_of(u64::from(self.config.full_check_every));
        let mut found = Vec::new();
        walk(
            &self.root.join(UPDATES),
            String::new(),
            0,
            LOG_EXT,
            &mut found,
        );
        let mut bases = Vec::new();
        walk(
            &self.root.join(SUBLAYERS),
            String::new(),
            0,
            SNAPSHOT_EXT,
            &mut bases,
        );
        for (dir_rel, stem, path) in found {
            let Ok(doc) = DocKey::parse(&dir_rel) else {
                continue;
            };
            if stem == self.client.as_str() && !self.is_read_only() {
                if !self.own.contains_key(&doc) {
                    let r = self.adopt_own(&doc, &path);
                    self.note(r);
                }
                continue;
            }
            self.sources.entry(path.clone()).or_insert_with(|| Source {
                doc,
                kind: SourceKind::Log,
                generation: None,
                offset: 0,
                seen: None,
            });
            let r = self.read_source(&path, full);
            self.note(r);
        }
        for (dir_rel, stem, path) in bases {
            let id = if dir_rel.is_empty() {
                stem
            } else {
                format!("{dir_rel}/{stem}")
            };
            let Ok(doc @ DocKey::Sublayer(_)) = DocKey::parse(&id) else {
                continue;
            };
            self.sources.entry(path.clone()).or_insert_with(|| Source {
                doc,
                kind: SourceKind::Base,
                generation: None,
                offset: 0,
                seen: None,
            });
            let r = self.read_source(&path, full);
            self.note(r);
        }
        self.poll_presence();
        let history = self.root.join(crate::history::HISTORY_FILE);
        if history.is_file() {
            self.sources
                .entry(history.clone())
                .or_insert_with(|| Source {
                    doc: DocKey::History,
                    kind: SourceKind::Base,
                    generation: None,
                    offset: 0,
                    seen: None,
                });
            let r = self.read_source(&history, full);
            self.note(r);
        }
        if !self.scanned {
            self.scanned = true;
            self.inbox.push_back(self.hello());
        }
    }

    fn note(&mut self, r: Result<(), SyncError>) {
        if let Err(e) = r {
            self.last_error = Some(e.to_string());
        }
    }

    fn hello(&self) -> Message {
        Message::hello(
            ClientId::new(FOLDER_CLIENT).expect("valid"),
            self.folder
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }

    /// Read other clients' `presence/<client>.presence` heartbeat files that
    /// changed. Files older than twice their TTL belong to clients that are gone.
    fn poll_presence(&mut self) {
        let dir = self.root.join(PRESENCE);
        let Ok(entries) = fs::read_dir(&dir) else {
            return;
        };
        let own = format!("{}.{PRESENCE_EXT}", self.client);
        let now = SystemTime::now();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name == own || path.extension().and_then(|e| e.to_str()) != Some(PRESENCE_EXT) {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let mtime = meta.modified().ok();
            let stamp = (meta.len(), mtime);
            if self.presence_seen.get(&path) == Some(&stamp) {
                continue;
            }
            let age = mtime
                .and_then(|m| now.duration_since(m).ok())
                .unwrap_or_default();
            if age > PRESENCE_GC {
                let _ = fs::remove_file(&path);
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let (records, _) = parse_records(&bytes);
            let Some(msg) = records.first().and_then(|p| Message::decode(p).ok()) else {
                // Half-visible rewrite: try again on the next poll.
                continue;
            };
            self.presence_seen.insert(path, stamp);
            if let Message::Presence { ttl_ms, .. } = &msg {
                if age > Duration::from_millis(u64::from(*ttl_ms)) * 2 {
                    continue;
                }
                self.inbox.push_back(msg);
            }
        }
    }

    /// Publish our presence by rewriting our heartbeat file (temporary file +
    /// rename). Leaving writes a "left" record so others react at once instead of
    /// waiting for the TTL; untouched files are deleted after a day by anyone.
    fn write_presence(&self, msg: &Message) -> Result<(), SyncError> {
        let Message::Presence { client, .. } = msg else {
            return Ok(());
        };
        if client != &self.client {
            return Ok(());
        }
        let dir = self.root.join(PRESENCE);
        let path = dir.join(format!("{}.{PRESENCE_EXT}", self.client));
        fs::create_dir_all(&dir).map_err(SyncError::io(&dir))?;
        let tmp = dir.join(format!("{}.tmp-{}", self.client, new_generation()));
        let bytes = encode_record(&msg.encode());
        let write = || -> io::Result<()> {
            fs::write(&tmp, &bytes)?;
            fs::rename(&tmp, &path)
        };
        write().map_err(|e| {
            let _ = fs::remove_file(&tmp);
            SyncError::io(&path)(e)
        })
    }

    fn deliver(&mut self, msg: Message) {
        if let Message::Updates { doc, version, .. } | Message::Snapshot { doc, version, .. } = &msg
        {
            self.folder.entry(doc.clone()).or_default().merge(version);
            self.stats.records_read += 1;
            self.inbox.push_back(msg);
        }
    }

    fn read_source(&mut self, path: &Path, full: bool) -> Result<(), SyncError> {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.sources.remove(path);
                return Ok(());
            }
            Err(e) => return Err(SyncError::io(path)(e)),
        };
        let stamp = (meta.len(), meta.modified().ok());
        let src = self.sources.get_mut(path).expect("registered");
        if !full && src.seen == Some(stamp) {
            return Ok(());
        }
        let mut out = Vec::new();
        match src.kind {
            SourceKind::Base => {
                let bytes = fs::read(path).map_err(SyncError::io(path))?;
                // A half-visible file fails the checksum: retry on the next poll.
                let Ok(meta) = LoroDoc::decode_import_blob_meta(&bytes, true) else {
                    return Ok(());
                };
                src.seen = Some(stamp);
                out.push(Message::Snapshot {
                    doc: src.doc.clone(),
                    version: meta.partial_end_vv,
                    data: bytes,
                });
            }
            SourceKind::Log => {
                let mut f = File::open(path).map_err(SyncError::io(path))?;
                let Some((header, header_len)) =
                    read_header(&mut f).map_err(SyncError::io(path))?
                else {
                    // Header not readable yet (or a newer format): try again later.
                    return Ok(());
                };
                if header.doc != src.doc.as_str() {
                    return Ok(());
                }
                if src.generation.as_deref() != Some(header.generation.as_str()) {
                    src.generation = Some(header.generation);
                    src.offset = header_len;
                }
                f.seek(SeekFrom::Start(src.offset))
                    .map_err(SyncError::io(path))?;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).map_err(SyncError::io(path))?;
                let (payloads, consumed) = parse_records(&buf);
                src.offset += consumed as u64;
                src.seen = Some(stamp);
                out.extend(payloads.into_iter().filter_map(|p| Message::decode(p).ok()));
            }
        }
        for msg in out {
            self.deliver(msg);
        }
        Ok(())
    }

    /// Read this client's existing log for `doc` (left by an earlier session):
    /// deliver its records, cut off a torn tail, and continue appending to it.
    fn adopt_own(&mut self, doc: &DocKey, path: &Path) -> Result<(), SyncError> {
        let mut f = File::open(path).map_err(SyncError::io(path))?;
        let header = read_header(&mut f).map_err(SyncError::io(path))?;
        let Some((header, header_len)) = header.filter(|(h, _)| h.doc == doc.as_str()) else {
            // Unreadable: keep it for inspection, start a new log on the next write.
            let aside = path.with_extension(format!("corrupt-{}", new_generation()));
            fs::rename(path, &aside).map_err(SyncError::io(path))?;
            return Ok(());
        };
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).map_err(SyncError::io(path))?;
        drop(f);
        let (payloads, consumed) = parse_records(&buf);
        let msgs: Vec<Message> = payloads
            .iter()
            .filter_map(|p| Message::decode(p).ok())
            .collect();
        let first_snapshot = match (msgs.first(), payloads.first()) {
            (Some(Message::Snapshot { .. }), Some(p)) => (RECORD_HEADER + p.len()) as u64,
            _ => 0,
        };
        let good = header_len + consumed as u64;
        if consumed < buf.len() {
            OpenOptions::new()
                .write(true)
                .open(path)
                .and_then(|f| f.set_len(good))
                .map_err(SyncError::io(path))?;
            self.stats.repairs += 1;
        }
        self.own.insert(
            doc.clone(),
            OwnLog {
                path: path.to_owned(),
                generation: header.generation,
                len: good,
                compacted_len: header_len + first_snapshot,
                snapshot_requested: false,
            },
        );
        for msg in msgs {
            self.deliver(msg);
        }
        Ok(())
    }

    /// The session says what it has. Re-deliver every document it is missing
    /// something of (e.g. a sublayer the app just added), then answer with our own
    /// hello so a reset session knows what the folder has.
    fn on_hello(&mut self, versions: Vec<(DocKey, VersionVector)>) -> Result<(), SyncError> {
        if !self.scanned {
            // The first scan delivers everything anyway.
            return Ok(());
        }
        let theirs: BTreeMap<DocKey, VersionVector> = versions.into_iter().collect();
        let missing: Vec<DocKey> = self
            .folder
            .iter()
            .filter(|(doc, v)| !theirs.get(*doc).is_some_and(|t| t.includes_vv(v)))
            .map(|(doc, _)| doc.clone())
            .collect();
        for doc in &missing {
            for src in self.sources.values_mut().filter(|s| &s.doc == doc) {
                src.generation = None;
                src.seen = None;
            }
            if let Some(own) = self.own.get(doc) {
                let path = own.path.clone();
                let r = read_all_records(&path).map(|msgs| {
                    for m in msgs {
                        self.deliver(m);
                    }
                });
                self.note(r);
            }
        }
        if !missing.is_empty() {
            self.poll_now = true;
        }
        self.inbox.push_back(self.hello());
        Ok(())
    }

    // ---- writing ----

    fn append(
        &mut self,
        doc: &DocKey,
        version: &VersionVector,
        msg: &Message,
    ) -> Result<(), SyncError> {
        let record = encode_record(&msg.encode());
        let appended = match self.own.get_mut(doc) {
            Some(own) => append_checked(own, &record)?,
            None => false,
        };
        if !appended {
            let repaired = self.own.contains_key(doc);
            self.write_fresh(doc, &record)?;
            if repaired {
                // Our log vanished or was replaced: get the full state back in.
                self.stats.repairs += 1;
                self.request_snapshot(doc);
            }
        }
        self.stats.records_written += 1;
        self.folder.entry(doc.clone()).or_default().merge(version);
        let own = self.own.get(doc).expect("written");
        let threshold = self
            .config
            .compact_min_bytes
            .max(own.compacted_len.saturating_mul(self.config.compact_ratio));
        if own.len > threshold {
            self.request_snapshot(doc);
        }
        Ok(())
    }

    /// A write to our log for `doc` failed. The session already counts those ops
    /// as written (it cannot tell), so its next update for `doc` starts after
    /// them. Appending that would leave a gap the folder never admits to: a
    /// record's version covers everything before it, and our `Hello` reports the
    /// union, so no session would send the lost ops again. Instead nothing more
    /// is written for `doc` until a snapshot replaces the whole log; ask the
    /// session for one (again after every failure, a failed rewrite included).
    /// If the session goes away before answering, the log still claims only what
    /// it holds, and the next session on the folder writes what is missing.
    fn fell_behind(&mut self, doc: &DocKey) {
        let now = Instant::now();
        let first = self
            .config
            .poll_interval
            .clamp(Duration::from_millis(100), BEHIND_MAX_DELAY);
        match self.behind.get_mut(doc) {
            Some(b) => {
                // The rewrite failed too: ask again once the current delay
                // (doubled when the request went out) has passed since now.
                b.next_request = b.next_request.max(now + b.delay);
            }
            None => {
                self.behind.insert(
                    doc.clone(),
                    Behind {
                        next_request: now + first,
                        delay: first,
                    },
                );
                self.inbox
                    .push_back(Message::SnapshotRequest { doc: doc.clone() });
            }
        }
    }

    /// Ask again for the snapshots of logs that are still behind (the answer may
    /// have been lost with a session that went away), with backoff. Checked on
    /// every `recv`, not only when the folder is polled (which may be rare with
    /// file events).
    fn rerequest_behind(&mut self) {
        let now = Instant::now();
        for (doc, b) in &mut self.behind {
            if now >= b.next_request {
                b.next_request = now + b.delay;
                b.delay = (b.delay * 2).min(BEHIND_MAX_DELAY);
                self.inbox
                    .push_back(Message::SnapshotRequest { doc: doc.clone() });
            }
        }
    }

    fn request_snapshot(&mut self, doc: &DocKey) {
        if let Some(own) = self.own.get_mut(doc)
            && !own.snapshot_requested
        {
            own.snapshot_requested = true;
            self.inbox
                .push_back(Message::SnapshotRequest { doc: doc.clone() });
        }
    }

    /// Compaction: replace our log with one snapshot record.
    fn compact(
        &mut self,
        doc: &DocKey,
        version: &VersionVector,
        msg: &Message,
    ) -> Result<(), SyncError> {
        self.write_fresh(doc, &encode_record(&msg.encode()))?;
        let own = self.own.get_mut(doc).expect("written");
        own.compacted_len = own.len;
        self.stats.compactions += 1;
        self.stats.records_written += 1;
        self.folder.entry(doc.clone()).or_default().merge(version);
        Ok(())
    }

    /// Write a new log (new generation) holding `record`, via a temporary file
    /// and a rename so readers never see a partial header.
    fn write_fresh(&mut self, doc: &DocKey, record: &[u8]) -> Result<(), SyncError> {
        let dir = self.doc_dir(doc);
        fs::create_dir_all(&dir).map_err(SyncError::io(&dir))?;
        let generation = new_generation();
        let header = LogHeader {
            format: LOG_FORMAT.to_owned(),
            version: LOG_VERSION,
            doc: doc.as_str().to_owned(),
            client: self.client.as_str().to_owned(),
            generation: generation.clone(),
        };
        let mut bytes = encode_record(&serde_json::to_vec(&header).expect("header serializes"));
        bytes.extend_from_slice(record);
        let path = self.log_path(doc);
        let tmp = dir.join(format!("{}.tmp-{generation}", self.client));
        let write = || -> io::Result<()> {
            let mut f = File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, &path)
        };
        if let Err(e) = write() {
            let _ = fs::remove_file(&tmp);
            return Err(SyncError::io(&path)(e));
        }
        self.own.insert(
            doc.clone(),
            OwnLog {
                path,
                generation,
                len: bytes.len() as u64,
                compacted_len: bytes.len() as u64,
                snapshot_requested: false,
            },
        );
        Ok(())
    }
}

impl Transport for DirTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        if self.is_read_only() {
            return match &msg {
                Message::Hello { versions, .. } => self.on_hello(versions.clone()),
                Message::Updates { doc, .. } | Message::Snapshot { doc, .. } => {
                    Err(SyncError::io(&self.log_path(doc))(io::Error::new(
                        io::ErrorKind::ReadOnlyFilesystem,
                        "the shared folder is open read-only",
                    )))
                }
                Message::SnapshotRequest { .. } | Message::Presence { .. } => Ok(()),
            };
        }
        match &msg {
            Message::Hello { versions, .. } => self.on_hello(versions.clone()),
            Message::Updates { doc, version, .. } => {
                let doc = doc.clone();
                if self.behind.contains_key(&doc) {
                    // The snapshot we asked for covers this too.
                    return Ok(());
                }
                let r = self.append(&doc, &version.clone(), &msg);
                if r.is_err() {
                    self.fell_behind(&doc);
                }
                r
            }
            Message::Snapshot { doc, version, .. } => {
                let doc = doc.clone();
                let r = self.compact(&doc, &version.clone(), &msg);
                if r.is_ok() {
                    self.behind.remove(&doc);
                } else {
                    self.fell_behind(&doc);
                }
                r
            }
            Message::SnapshotRequest { .. } => Ok(()),
            Message::Presence { .. } => self.write_presence(&msg),
        }
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        if self.inbox.is_empty() {
            let now = Instant::now();
            let due = std::mem::take(&mut self.poll_now)
                | self.wake.swap(false, Ordering::Relaxed)
                | self
                    .last_poll
                    .is_none_or(|t| now.saturating_duration_since(t) >= self.config.poll_interval);
            if due {
                self.last_poll = Some(now);
                self.poll();
            }
            self.rerequest_behind();
        }
        Ok(self.inbox.pop_front())
    }

    /// Writes happen in `send`; what can still be missing is a log that fell
    /// behind (a write failed) and was not rewritten yet: the edits since are
    /// in no file. The next session on the folder writes them if the documents
    /// still hold them (see [`DirTransport::fell_behind`]).
    fn close(&mut self) -> Result<(), SyncError> {
        match self.behind.keys().next() {
            Some(doc) => Err(SyncError::io(&self.log_path(doc))(io::Error::other(
                "a write failed and the log was not rewritten yet; the edits since are not in it",
            ))),
            None => Ok(()),
        }
    }
}

/// Append `record` if the file is exactly as we left it (cutting off our own torn
/// write first). `false`: the log is gone or was replaced and must be rewritten.
fn append_checked(own: &mut OwnLog, record: &[u8]) -> Result<bool, SyncError> {
    let path = own.path.clone();
    let actual = match fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(SyncError::io(&path)(e)),
    };
    if actual < own.len {
        return Ok(false);
    }
    let mut f = OpenOptions::new()
        .read(true)
        .append(true)
        .open(&path)
        .map_err(SyncError::io(&path))?;
    if actual > own.len {
        let same = read_header(&mut f)
            .map_err(SyncError::io(&path))?
            .is_some_and(|(h, _)| h.generation == own.generation);
        if !same {
            return Ok(false);
        }
        // Cut through a handle of its own: on Windows an append handle lacks
        // FILE_WRITE_DATA, which setting the end of the file needs, so
        // `f.set_len` fails there with access denied. `f` then appends at the new
        // end of the file.
        OpenOptions::new()
            .write(true)
            .open(&path)
            .and_then(|w| w.set_len(own.len))
            .map_err(SyncError::io(&path))?;
    }
    // No fsync per append: closing the file hands the data to the SMB server, a
    // torn tail after a crash is cut off on the next start, and the edits are
    // still in the session's documents. Rewrites (compaction) are synced because
    // a rename must never expose a file whose data is not on disk.
    f.write_all(record).map_err(SyncError::io(&path))?;
    own.len += record.len() as u64;
    Ok(true)
}

/// Every record of the log at `path`, decoded.
fn read_all_records(path: &Path) -> Result<Vec<Message>, SyncError> {
    let mut f = File::open(path).map_err(SyncError::io(path))?;
    let Some((_, _)) = read_header(&mut f).map_err(SyncError::io(path))? else {
        return Ok(Vec::new());
    };
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(SyncError::io(path))?;
    let (payloads, _) = parse_records(&buf);
    Ok(payloads
        .into_iter()
        .filter_map(|p| Message::decode(p).ok())
        .collect())
}

/// Files with extension `ext` under `dir`, as `(directory relative to the walk
/// root with '/' separators, file stem, path)`.
fn walk(
    dir: &Path,
    rel: String,
    depth: usize,
    ext: &str,
    out: &mut Vec<(String, String, PathBuf)>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            continue;
        };
        if kind.is_dir() {
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            walk(&path, child, depth + 1, ext, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext)
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            out.push((rel.clone(), stem.to_owned(), path.clone()));
        }
    }
}

fn new_generation() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn checksum(payload: &[u8]) -> [u8; 8] {
    let hash = blake3::hash(payload);
    let mut sum = [0u8; 8];
    sum.copy_from_slice(&hash.as_bytes()[..8]);
    sum
}

/// `len: u32 LE`, `blake3(payload)[..8]`, payload.
pub(crate) fn encode_record(payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("record under 4 GiB");
    let mut out = Vec::with_capacity(RECORD_HEADER + payload.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&checksum(payload));
    out.extend_from_slice(payload);
    out
}

/// Complete, intact records at the start of `buf`, and how many bytes they span.
/// Stops at the first incomplete or corrupt record.
pub(crate) fn parse_records(buf: &[u8]) -> (Vec<&[u8]>, usize) {
    let mut out = Vec::new();
    let mut pos = 0;
    while buf.len() - pos >= RECORD_HEADER {
        let len = u32::from_le_bytes(buf[pos..pos + 4].try_into().expect("4 bytes")) as usize;
        if len > MAX_RECORD || buf.len() - pos - RECORD_HEADER < len {
            break;
        }
        let payload = &buf[pos + RECORD_HEADER..pos + RECORD_HEADER + len];
        if buf[pos + 4..pos + RECORD_HEADER] != checksum(payload) {
            break;
        }
        out.push(payload);
        pos += RECORD_HEADER + len;
    }
    (out, pos)
}

/// The header record at the start of `f`, and its size; `None` if it is not
/// (yet) complete and intact, or from a newer format version.
fn read_header(f: &mut File) -> io::Result<Option<(LogHeader, u64)>> {
    f.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; RECORD_HEADER];
    if let Err(e) = f.read_exact(&mut head) {
        return if e.kind() == io::ErrorKind::UnexpectedEof {
            Ok(None)
        } else {
            Err(e)
        };
    }
    let len = u32::from_le_bytes(head[..4].try_into().expect("4 bytes")) as usize;
    if len > MAX_HEADER {
        return Ok(None);
    }
    let mut payload = vec![0u8; len];
    if let Err(e) = f.read_exact(&mut payload) {
        return if e.kind() == io::ErrorKind::UnexpectedEof {
            Ok(None)
        } else {
            Err(e)
        };
    }
    if head[4..] != checksum(&payload) {
        return Ok(None);
    }
    let Ok(header) = serde_json::from_slice::<LogHeader>(&payload) else {
        return Ok(None);
    };
    if header.format != LOG_FORMAT || header.version > LOG_VERSION {
        return Ok(None);
    }
    Ok(Some((header, (RECORD_HEADER + len) as u64)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_stop_at_torn_or_corrupt_data() {
        let mut log = encode_record(b"one");
        log.extend(encode_record(b"two"));
        let full = log.len();
        log.extend(encode_record(b"three"));
        for cut in full..log.len() {
            let (records, used) = parse_records(&log[..cut]);
            assert_eq!(records, vec![&b"one"[..], b"two"]);
            assert_eq!(used, full);
        }
        let (records, used) = parse_records(&log);
        assert_eq!(records.len(), 3);
        assert_eq!(used, log.len());
        let mut bad = log.clone();
        bad[full + RECORD_HEADER] ^= 1;
        assert_eq!(parse_records(&bad).0.len(), 2);
    }
}
