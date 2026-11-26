//! Shared-folder transport (`docs/spec/sync.md` §4): no server, the project
//! directory on a NAS share is the remote.
//!
//! Every client appends only to its own `updates/<doc>/<client-id>.log` and reads
//! everyone else's. Logs are sequences of checksummed records; the first record is
//! a JSON header, the rest are encoded [`Message::Updates`] / [`Message::Snapshot`].
//! A client compacts only its own log (rewrite as one snapshot, then rename), so
//! concurrent compactions never interfere. Logs of clients that left long ago are
//! deleted once another self-contained log holds all of them (§4.6).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use bae_model::loro::{EncodedBlobMode, LoroDoc, VersionVector};
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
/// The cleanup of departed clients' logs runs at most this often.
const LOG_GC_EVERY: Duration = Duration::from_secs(3600);
/// What the cleanup writes into our lock file to read the folder's clock from
/// its modification time (the same bytes every time).
const LOCK_TEXT: &[u8] = b"bae sync session lock\n";

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
    /// Delete another client's log once a self-contained log (one that starts
    /// with a snapshot) holds all of it and neither the log nor its owner's
    /// presence file changed for this long (`docs/spec/sync.md` §4.6). Checked at
    /// most once an hour while polling. `None`: never delete other clients' logs.
    pub log_gc_after: Option<Duration>,
}

impl Default for DirConfig {
    fn default() -> Self {
        DirConfig {
            poll_interval: Duration::from_secs(1),
            compact_min_bytes: 1 << 20,
            compact_ratio: 4,
            full_check_every: 30,
            watch: true,
            log_gc_after: Some(Duration::from_secs(7 * 24 * 3600)),
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

#[derive(Debug)]
struct OwnLog {
    path: PathBuf,
    generation: String,
    /// Bytes written (and verified) so far.
    len: u64,
    /// Size right after the last compaction.
    compacted_len: u64,
    snapshot_requested: bool,
    /// The file vanished (another client's cleanup deleted it) and a snapshot to
    /// write it again has been requested.
    gone: bool,
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
    /// Records not written because our own log of their document is from a
    /// newer version of the format ([`SyncError::NewerLogVersion`]).
    pub records_refused: u64,
    pub compactions: u64,
    /// Own logs that had to be rewritten because they were changed underneath us.
    pub repairs: u64,
    /// Other clients' logs deleted because they were no longer needed (§4.6).
    pub logs_removed: u64,
    /// Logs read in full by that cleanup (it skips documents where nothing
    /// changed since a pass that found nothing to delete).
    pub logs_scanned: u64,
}

/// The shared-folder [`Transport`]. Its I/O is synchronous and may be slow on a
/// NAS: run it on a background thread with [`crate::ThreadedTransport`].
pub struct DirTransport {
    root: PathBuf,
    client: ClientId,
    config: DirConfig,
    /// `updates/<client>.lock`, held open (and locked) for the session.
    lock: File,
    own: BTreeMap<DocKey, OwnLog>,
    /// Documents whose own log is from a newer version of the format, with
    /// that version: left alone, and nothing is written for them (§4.3).
    newer: BTreeMap<DocKey, u32>,
    sources: BTreeMap<PathBuf, Source>,
    /// Union of the versions of everything read from or written to the folder.
    folder: BTreeMap<DocKey, VersionVector>,
    inbox: VecDeque<Message>,
    scanned: bool,
    poll_now: bool,
    last_poll: Option<Instant>,
    wake: Arc<AtomicBool>,
    last_error: Option<String>,
    stats: DirStats,
    /// `(len, mtime)` of other clients' presence files when last read.
    presence_seen: BTreeMap<PathBuf, (u64, Option<SystemTime>)>,
    /// When the cleanup of departed clients' logs last ran.
    last_gc: Option<Instant>,
    /// Per document, what the cleanup's last full read of its logs saw when it
    /// found nothing to delete: nothing to do until that changes.
    gc_idle: BTreeMap<DocKey, GcSeen>,
    #[cfg(feature = "watch")]
    _watcher: Option<notify::RecommendedWatcher>,
}

/// `(len, mtime)` of a file, to notice changes without reading it.
type Stamp = (u64, Option<SystemTime>);

/// What decides a cleanup pass over one document: every log's path and stamp
/// (`None`: unreadable) and which of them looked departed.
#[derive(Debug, PartialEq)]
struct GcSeen {
    stamps: Vec<(PathBuf, Option<Stamp>)>,
    candidates: Vec<usize>,
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
        #[allow(unused_mut)]
        let mut t = DirTransport {
            root,
            client,
            config,
            lock,
            own: BTreeMap::new(),
            newer: BTreeMap::new(),
            sources: BTreeMap::new(),
            folder: BTreeMap::new(),
            inbox: VecDeque::new(),
            scanned: false,
            poll_now: true,
            last_poll: None,
            wake: Arc::new(AtomicBool::new(false)),
            last_error: None,
            stats: DirStats::default(),
            presence_seen: BTreeMap::new(),
            last_gc: None,
            gc_idle: BTreeMap::new(),
            #[cfg(feature = "watch")]
            _watcher: None,
        };
        #[cfg(feature = "watch")]
        if t.config.watch {
            t.start_watcher();
        }
        Ok(t)
    }

    pub fn root(&self) -> &Path {
        &self.root
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

    /// Delete departed clients' logs now (`docs/spec/sync.md` §4.6) instead of
    /// waiting for the hourly pass while polling. Returns how many logs were
    /// deleted; problems are reported through [`DirTransport::last_error`]. Does
    /// nothing when [`DirConfig::log_gc_after`] is `None`.
    pub fn remove_departed_logs(&mut self) -> u64 {
        let mut found = Vec::new();
        walk(
            &self.root.join(UPDATES),
            String::new(),
            0,
            LOG_EXT,
            &mut found,
        );
        let logs: Vec<(DocKey, String, PathBuf)> = found
            .into_iter()
            .filter_map(|(rel, stem, path)| Some((DocKey::parse(&rel).ok()?, stem, path)))
            .collect();
        self.last_gc = Some(Instant::now());
        self.collect_logs(&logs)
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
        // `(document, owner, path)` of every log, for the cleanup.
        let mut logs = Vec::with_capacity(found.len());
        for (dir_rel, stem, path) in found {
            let Ok(doc) = DocKey::parse(&dir_rel) else {
                continue;
            };
            if stem == self.client.as_str() {
                // A newer log was reported when it was found: not again.
                if !self.own.contains_key(&doc) && !self.newer.contains_key(&doc) {
                    let r = self.adopt_own(&doc, &path);
                    self.note(r);
                }
            } else {
                self.sources.entry(path.clone()).or_insert_with(|| Source {
                    doc: doc.clone(),
                    kind: SourceKind::Log,
                    generation: None,
                    offset: 0,
                    seen: None,
                });
                let r = self.read_source(&path, full);
                self.note(r);
            }
            logs.push((doc, stem, path));
        }
        self.check_own(&logs);
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
        } else if self.last_gc.is_none_or(|t| t.elapsed() >= LOG_GC_EVERY) {
            // Not on the first poll: the session waits for its hello. This runs
            // on the I/O thread of a `ThreadedTransport`, never on the stage's.
            self.last_gc = Some(Instant::now());
            self.collect_logs(&logs);
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
    ///
    /// A log from a newer version of the format stays as it is (newer clients
    /// read it; it may hold ops nobody else has): the document is remembered,
    /// nothing is written for it while this transport is open, and this
    /// returns [`SyncError::NewerLogVersion`]. Only a log whose header is
    /// damaged is renamed aside: it holds nothing anyone can read.
    fn adopt_own(&mut self, doc: &DocKey, path: &Path) -> Result<(), SyncError> {
        let mut f = File::open(path).map_err(SyncError::io(path))?;
        let header = match read_header_record(&mut f).map_err(SyncError::io(path))? {
            Header::Read(h, len) if h.doc == doc.as_str() => Some((h, len)),
            Header::Newer(version) => {
                self.newer.insert(doc.clone(), version);
                return Err(self.newer_log(doc).expect("just noted"));
            }
            Header::Read(..) | Header::Damaged => None,
        };
        let Some((header, header_len)) = header else {
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
                gone: false,
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
            // A vanished log's ops are in the log that let it go (§4.6), which is
            // re-read above.
            if let Some(own) = self.own.get(doc).filter(|o| !o.gone) {
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

    // ---- cleanup of departed clients' logs (§4.6) ----

    /// Notice that one of our own logs vanished although we did not touch it:
    /// another client's cleanup deleted it while we looked inactive (or a person
    /// did). Its ops are in another log, but we ask the session for a snapshot
    /// anyway and write it as a new self-contained log, so the folder has all we
    /// have again even if we never edit, and an append that raced with the
    /// deletion is not lost. `logs` is this poll's listing.
    fn check_own(&mut self, logs: &[(DocKey, String, PathBuf)]) {
        let listed: BTreeSet<&DocKey> = logs
            .iter()
            .filter(|(_, owner, _)| owner == self.client.as_str())
            .map(|(doc, _, _)| doc)
            .collect();
        let vanished: Vec<DocKey> = self
            .own
            .iter()
            .filter(|(doc, own)| {
                // The listing can be stale or fail: only a definite "not found"
                // counts.
                !own.gone
                    && !listed.contains(doc)
                    && fs::metadata(&own.path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
            })
            .map(|(doc, _)| doc.clone())
            .collect();
        for doc in vanished {
            self.stats.repairs += 1;
            if let Some(own) = self.own.get_mut(&doc) {
                own.gone = true;
            }
            self.request_snapshot(&doc);
        }
    }

    /// Delete other clients' logs that are no longer needed. `logs` lists every
    /// log as `(document, owner, path)`. Returns how many were deleted; problems
    /// are noted and skipped, so this never stops polling.
    fn collect_logs(&mut self, logs: &[(DocKey, String, PathBuf)]) -> u64 {
        let Some(after) = self.config.log_gc_after else {
            return 0;
        };
        // Ages are measured with the folder's clock, not ours: a client whose
        // clock runs ahead (or a file server whose clock is behind) must not
        // see every log as old.
        let now = match self.share_clock() {
            Ok(now) => now,
            Err(e) => {
                self.note(Err(e));
                return 0;
            }
        };
        let mut by_doc: BTreeMap<&DocKey, Vec<(&str, &Path)>> = BTreeMap::new();
        for (doc, owner, path) in logs {
            by_doc
                .entry(doc)
                .or_default()
                .push((owner.as_str(), path.as_path()));
        }
        let mut removed = 0;
        for (doc, group) in by_doc {
            removed += self.collect_doc_logs(doc, &group, now, after);
        }
        removed
    }

    /// The cleanup of one document. `group` holds all its logs as `(owner,
    /// path)`, ours included: ours can hold other clients' ops too, but it is
    /// never deleted.
    fn collect_doc_logs(
        &mut self,
        doc: &DocKey,
        group: &[(&str, &Path)],
        now: SystemTime,
        after: Duration,
    ) -> u64 {
        // Cheap test first: only logs whose owner looks gone can go. Most passes
        // find none and read no file at all.
        let candidates: Vec<usize> = (0..group.len())
            .filter(|&i| {
                let (owner, path) = group[i];
                owner != self.client.as_str()
                    && ClientId::new(owner).is_ok()
                    && self.departed(owner, path, now, after)
            })
            .collect();
        if candidates.is_empty() {
            return 0;
        }
        // A pass that reads every log of the document and deletes nothing comes
        // to the same answer until a log changes or another one looks departed
        // (e.g. a log nobody can delete: a torn tail, ops in nobody's snapshot).
        let seen = GcSeen {
            stamps: group
                .iter()
                .map(|&(_, path)| {
                    let meta = fs::metadata(path).ok();
                    (path.to_owned(), meta.map(|m| (m.len(), m.modified().ok())))
                })
                .collect(),
            candidates: candidates.clone(),
        };
        if self.gc_idle.get(doc) == Some(&seen) {
            return 0;
        }
        let mut failed = false;
        let mut scans = Vec::with_capacity(group.len());
        for &(owner, path) in group {
            self.stats.logs_scanned += 1;
            scans.push(match scan_log(path, doc, owner) {
                Ok(scan) => scan,
                Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                Err(e) => {
                    failed = true;
                    self.note(Err(SyncError::io(path)(e)));
                    None
                }
            });
        }
        // A log deleted in this pass is never used as a cover afterwards, and a
        // log used as a cover is not deleted in this pass: whatever we delete is
        // in a log we leave in place. Other clients only delete a log for a
        // strictly larger one (`LogScan::may_go_for`), so following "deleted
        // because of" from any log never comes back to it (a strict order has no
        // cycles) and ends at a log nobody deletes, which holds it all.
        let mut removed = vec![false; group.len()];
        let mut covering = vec![false; group.len()];
        let mut count = 0;
        for &i in &candidates {
            let Some(scan) = &scans[i] else {
                continue;
            };
            if covering[i] {
                continue;
            }
            // Prefer a cover that stays anyway, so more candidates can go now.
            let Some(cover) = (0..group.len())
                .filter(|&j| j != i && !removed[j])
                .filter(|&j| scans[j].as_ref().is_some_and(|c| scan.may_go_for(c)))
                .min_by_key(|j| candidates.contains(j))
            else {
                continue;
            };
            let (owner, path) = group[i];
            // Check again right before deleting: unchanged since we read it, and
            // its owner is still away (it may have come back meanwhile).
            let unchanged =
                fs::metadata(path).is_ok_and(|m| (m.len(), m.modified().ok()) == scan.stamp);
            if !unchanged || !self.departed(owner, path, now, after) {
                continue;
            }
            match fs::remove_file(path) {
                Ok(()) => {
                    removed[i] = true;
                    covering[cover] = true;
                    count += 1;
                    self.stats.logs_removed += 1;
                    self.sources.remove(path);
                }
                // Someone else's cleanup was quicker.
                Err(e) if e.kind() == io::ErrorKind::NotFound => removed[i] = true,
                Err(e) => {
                    failed = true;
                    self.note(Err(SyncError::io(path)(e)));
                }
            }
        }
        if count == 0 && !failed {
            self.gc_idle.insert(doc.clone(), seen);
        } else {
            self.gc_idle.remove(doc);
        }
        count
    }

    /// The folder's clock: the modification time the file server gives our
    /// lock file when we write to it now. Only file times from the folder are
    /// compared with it. A time the server reports late (cached attributes)
    /// is older, which only keeps logs longer.
    fn share_clock(&self) -> Result<SystemTime, SyncError> {
        let path = self
            .root
            .join(UPDATES)
            .join(format!("{}.lock", self.client));
        let mut f = &self.lock;
        let stamp = f
            .seek(SeekFrom::Start(0))
            .and_then(|_| f.write_all(LOCK_TEXT))
            .and_then(|()| f.flush())
            .and_then(|()| fs::metadata(&path)?.modified());
        stamp.map_err(SyncError::io(&path))
    }

    /// Neither the log at `path` nor `owner`'s presence file changed for `after`
    /// before `now`, the folder's clock (no presence file counts as old). All
    /// three times come from the file server, so our own clock does not matter;
    /// a time we cannot read, or one after `now`, counts as recent.
    fn departed(&self, owner: &str, path: &Path, now: SystemTime, after: Duration) -> bool {
        let old = |meta: fs::Metadata| {
            meta.modified()
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .is_some_and(|age| age >= after)
        };
        if !fs::metadata(path).is_ok_and(old) {
            return false;
        }
        let presence = self
            .root
            .join(PRESENCE)
            .join(format!("{owner}.{PRESENCE_EXT}"));
        match fs::metadata(&presence) {
            Ok(meta) => old(meta),
            Err(e) => e.kind() == io::ErrorKind::NotFound,
        }
    }

    // ---- writing ----

    /// Before writing our log for `doc`: if a log of ours is there that no poll
    /// took over (the listing missed it, or reading it failed), take it over now
    /// instead of writing over it. It may be the only copy of ops, ours or (once
    /// a compaction took them in, §4.6) a departed client's.
    ///
    /// Fails with [`SyncError::NewerLogVersion`] if our log of `doc` is from a
    /// newer version of the format: nothing is written for that document
    /// (`DirStats::records_refused`), and the message's version is not added
    /// to what the folder has, so the folder's `Hello` still lacks it and a
    /// session sends it again once the folder is reopened. Other documents
    /// are not affected.
    fn adopt_before_write(&mut self, doc: &DocKey) -> Result<(), SyncError> {
        if self.own.contains_key(doc) {
            return Ok(());
        }
        let r = match self.newer_log(doc) {
            Some(e) => Err(e),
            None => {
                let path = self.log_path(doc);
                match fs::metadata(&path) {
                    Ok(_) => self.adopt_own(doc, &path),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(SyncError::io(&path)(e)),
                }
            }
        };
        if let Err(e) = &r
            && e.refused_doc().is_some()
        {
            self.stats.records_refused += 1;
            self.last_error = Some(e.to_string());
        }
        r
    }

    /// [`SyncError::NewerLogVersion`] if our log of `doc` is from a newer
    /// version of the format.
    fn newer_log(&self, doc: &DocKey) -> Option<SyncError> {
        self.newer
            .get(doc)
            .map(|&version| SyncError::NewerLogVersion {
                doc: doc.clone(),
                path: self.log_path(doc),
                version,
            })
    }

    fn append(
        &mut self,
        doc: &DocKey,
        version: &VersionVector,
        msg: &Message,
    ) -> Result<(), SyncError> {
        self.adopt_before_write(doc)?;
        let record = encode_record(&msg.encode());
        let appended = match self.own.get_mut(doc) {
            Some(own) => append_checked(own, &record)?,
            None => false,
        };
        if !appended {
            // `Some`: our log vanished or was replaced; `Some(true)`: a poll
            // already noticed (and counted) that.
            let noticed = self.own.get(doc).map(|o| o.gone);
            self.write_fresh(doc, &record)?;
            if let Some(noticed) = noticed {
                // Get the full state back in.
                if !noticed {
                    self.stats.repairs += 1;
                }
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
        self.adopt_before_write(doc)?;
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
                gone: false,
            },
        );
        Ok(())
    }
}

impl Transport for DirTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        match &msg {
            Message::Hello { versions, .. } => self.on_hello(versions.clone()),
            Message::Updates { doc, version, .. } => {
                self.append(&doc.clone(), &version.clone(), &msg)
            }
            Message::Snapshot { doc, version, .. } => {
                self.compact(&doc.clone(), &version.clone(), &msg)
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
        }
        Ok(self.inbox.pop_front())
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
    let mut f = match OpenOptions::new().read(true).append(true).open(&path) {
        Ok(f) => f,
        // Deleted since the check above (another client's cleanup, §4.6).
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(SyncError::io(&path)(e)),
    };
    if actual > own.len {
        let same = read_header(&mut f)
            .map_err(SyncError::io(&path))?
            .is_some_and(|(h, _)| h.generation == own.generation);
        if !same {
            return Ok(false);
        }
        f.set_len(own.len).map_err(SyncError::io(&path))?;
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

/// What the cleanup (§4.6) knows about one log, from its intact records only.
#[derive(Debug)]
struct LogScan {
    /// File stem, the same as `client` in the header.
    owner: String,
    /// `(len, mtime)` when it was read.
    stamp: (u64, Option<SystemTime>),
    /// Every op the log may hold: the `version` of each record and the end of
    /// each Loro blob in it, merged. An upper bound. `None` unless every byte is
    /// an intact record of the document that decodes: a torn or corrupt tail,
    /// a record kind from a newer version or a change while reading mean we do
    /// not know all it holds.
    upper: Option<VersionVector>,
    /// The ops the log certainly holds if it is self-contained: those in its
    /// first record, a full snapshot (taken from the Loro blob itself, not the
    /// `version` field). A lower bound. Later records are not counted: an
    /// `Updates` record holds only what its writer thought the folder lacked.
    held: Option<VersionVector>,
}

impl LogScan {
    /// This log may be deleted because of `cover`: `cover` holds every op this
    /// one may hold, and comes strictly after it in the order "version first,
    /// then the smaller client ID" (equal versions: the larger ID goes).
    ///
    /// The order stops two clients from deleting each other's logs. X going
    /// because of Y needs `held(Y) ⊇ upper(X)`, Y going because of X needs
    /// `held(X) ⊇ upper(Y)`. With `held ⊆ upper` for every log, both together
    /// mean all four are equal, and then only the larger ID may go. Candidates
    /// are logs nobody touched for days, so every client reads the same bytes
    /// and comes to the same answer.
    fn may_go_for(&self, cover: &LogScan) -> bool {
        let (Some(upper), Some(held)) = (&self.upper, &cover.held) else {
            return false;
        };
        held.includes_vv(upper) && (upper != held || self.owner > cover.owner)
    }
}

/// Read the whole log at `path` for the cleanup, with the parser polling uses.
/// `None`: the header is not one of `doc` written by `owner`.
fn scan_log(path: &Path, doc: &DocKey, owner: &str) -> io::Result<Option<LogScan>> {
    let mut f = File::open(path)?;
    let meta = f.metadata()?;
    let stamp = (meta.len(), meta.modified().ok());
    let Some((header, header_len)) = read_header(&mut f)? else {
        return Ok(None);
    };
    if header.doc != doc.as_str() || header.client != owner {
        return Ok(None);
    }
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let (payloads, consumed) = parse_records(&buf);
    let mut complete = consumed == buf.len() && header_len + buf.len() as u64 == stamp.0;
    let mut upper = VersionVector::default();
    let mut held = None;
    for (i, payload) in payloads.iter().enumerate() {
        let Ok(msg) = Message::decode(payload) else {
            complete = false;
            continue;
        };
        let (Message::Updates {
            doc: d,
            version,
            data,
        }
        | Message::Snapshot {
            doc: d,
            version,
            data,
        }) = &msg
        else {
            // No document data (readers skip these too).
            continue;
        };
        let blob = match LoroDoc::decode_import_blob_meta(data, true) {
            Ok(blob) if d == doc => blob,
            _ => {
                complete = false;
                continue;
            }
        };
        upper.merge(version);
        upper.merge(&blob.partial_end_vv);
        if i == 0
            && matches!(msg, Message::Snapshot { .. })
            && blob.mode == EncodedBlobMode::Snapshot
        {
            held = Some(blob.partial_end_vv);
        }
    }
    Ok(Some(LogScan {
        owner: owner.to_owned(),
        stamp,
        upper: complete.then_some(upper),
        held,
    }))
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
    Ok(match read_header_record(f)? {
        Header::Read(header, len) => Some((header, len)),
        Header::Newer(_) | Header::Damaged => None,
    })
}

/// What the start of a log holds.
enum Header {
    /// An intact header of this format version (or an older one), and its size.
    Read(LogHeader, u64),
    /// An intact header of a newer version of the format.
    Newer(u32),
    /// Too short, damaged, or not a log.
    Damaged,
}

fn read_header_record(f: &mut File) -> io::Result<Header> {
    f.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; RECORD_HEADER];
    if let Err(e) = f.read_exact(&mut head) {
        return if e.kind() == io::ErrorKind::UnexpectedEof {
            Ok(Header::Damaged)
        } else {
            Err(e)
        };
    }
    let len = u32::from_le_bytes(head[..4].try_into().expect("4 bytes")) as usize;
    if len > MAX_HEADER {
        return Ok(Header::Damaged);
    }
    let mut payload = vec![0u8; len];
    if let Err(e) = f.read_exact(&mut payload) {
        return if e.kind() == io::ErrorKind::UnexpectedEof {
            Ok(Header::Damaged)
        } else {
            Err(e)
        };
    }
    if head[4..] != checksum(&payload) {
        return Ok(Header::Damaged);
    }
    // Only the fields every version has: a newer header may differ in the rest.
    #[derive(Deserialize)]
    struct Version {
        format: String,
        version: u32,
    }
    match serde_json::from_slice::<Version>(&payload) {
        Ok(v) if v.format == LOG_FORMAT && v.version > LOG_VERSION => {
            return Ok(Header::Newer(v.version));
        }
        Ok(v) if v.format == LOG_FORMAT => {}
        _ => return Ok(Header::Damaged),
    }
    let Ok(header) = serde_json::from_slice::<LogHeader>(&payload) else {
        return Ok(Header::Damaged);
    };
    Ok(Header::Read(header, (RECORD_HEADER + len) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cleanup's "now" is the file server's: the time it stamps on our lock
    /// file when we write to it, whatever our own clock says. File times are
    /// compared only with it.
    #[test]
    fn cleanup_ages_logs_by_the_folder_clock() {
        let tmp = tempfile::tempdir().unwrap();
        let client = ClientId::new("me").unwrap();
        let t = DirTransport::open(tmp.path(), client, DirConfig::default()).unwrap();
        let day = Duration::from_secs(24 * 3600);
        let lock = tmp.path().join("updates/me.lock");
        File::options()
            .write(true)
            .open(&lock)
            .unwrap()
            .set_modified(SystemTime::now() - day * 1000)
            .unwrap();
        let before = SystemTime::now() - Duration::from_secs(1);
        let now = t.share_clock().unwrap();
        assert_eq!(now, fs::metadata(&lock).unwrap().modified().unwrap());
        assert!(now >= before, "the lock file was not written");

        let log = tmp.path().join("updates/root/gone.log");
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        fs::write(&log, b"").unwrap();
        File::options()
            .write(true)
            .open(&log)
            .unwrap()
            .set_modified(now - day * 3)
            .unwrap();
        let week = day * 7;
        assert!(!t.departed("gone", &log, now, week));
        // Whatever our clock says, only the folder's counts.
        assert!(t.departed("gone", &log, now + day * 5, week));
    }

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
