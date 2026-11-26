//! Shared-folder transport (`docs/spec/sync.md` §4): no server, the project
//! directory on a NAS share is the remote.
//!
//! Every client appends only to its own `updates/<doc>/<client-id>.log` and reads
//! everyone else's. Logs are sequences of checksummed records; the first record is
//! a JSON header, the rest are encoded [`Message::Updates`] / [`Message::Snapshot`].
//! A client compacts only its own log (rewrite as one snapshot, then rename), so
//! concurrent compactions never interfere.
//!
//! Logs of clients that left are deleted once another log holds everything in
//! them (§4.7, [`DirTransport`]'s cleanup section below).

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
/// Presence files untouched this long (on the file server's clock) are deleted
/// by anyone (crashed clients).
const PRESENCE_GC: Duration = Duration::from_secs(24 * 3600);
/// How long a reading of the file server's clock is reused (plus the time
/// since) before the probe file is written again.
const SERVER_CLOCK_REUSE: Duration = Duration::from_secs(60);
const SUBLAYERS: &str = "sublayers";
const LOG_EXT: &str = "log";
const LOCK_EXT: &str = "lock";
/// Temporary files are named `<client>.tmp-<random>` (client IDs have no dots).
const TMP_MARK: &str = ".tmp-";
const SNAPSHOT_EXT: &str = "loro";
/// A cleanup pass holds a departed client's lock for a moment while it deletes
/// one of that client's files (a few round trips to the file server, which may
/// be far away over a VPN), so `open` retries a held lock for this long before
/// it reports [`SyncError::ClientIdInUse`].
const LOCK_WAIT: Duration = Duration::from_secs(2);
const LOCK_RETRY: Duration = Duration::from_millis(50);
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
    /// Clients that showed no sign of life for this long (no write to any of their
    /// logs, no presence heartbeat, lock not held) count as departed: their logs
    /// are deleted once another log holds everything in them (§4.7). Temporary
    /// files this old are leftovers of crashed writers and are deleted too.
    /// `None`: never delete anything.
    pub inactive_after: Option<Duration>,
    /// How often to look for departed clients' files. The first pass runs this
    /// long after the first scan.
    pub cleanup_interval: Duration,
    /// When a departed client's log is not held by any other log, ask our session
    /// for a snapshot of that document (at most once per pass, and again only
    /// after the folder changed) so our own log holds it and a later pass can
    /// delete it.
    pub absorb_departed: bool,
}

impl Default for DirConfig {
    fn default() -> Self {
        DirConfig {
            poll_interval: Duration::from_secs(1),
            compact_min_bytes: 1 << 20,
            compact_ratio: 4,
            full_check_every: 30,
            watch: true,
            inactive_after: Some(Duration::from_secs(3 * 24 * 3600)),
            cleanup_interval: Duration::from_secs(10 * 60),
            absorb_departed: true,
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
    /// Our length when we asked our session for a snapshot to compact with, until
    /// one arrives. Asked again once the log doubled since: the answer may have
    /// been lost (a session that failed or went away), and one lost answer must
    /// not stop compaction for the rest of the session.
    snapshot_requested: Option<u64>,
    /// Union of the versions of the records in this generation of the log, ours
    /// and those adopted from an earlier session: an upper bound of its ops. A
    /// snapshot replaces the log only if its version includes this.
    vmax: VersionVector,
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
    /// Snapshots from our session that did not hold everything in our own log
    /// (records it had not imported), so they were appended to the log instead
    /// of replacing it.
    pub snapshots_appended: u64,
    /// Cleanup passes run (§4.7).
    pub cleanups: u64,
    /// Departed clients' logs deleted because another log holds all of them.
    pub logs_removed: u64,
    /// Leftover temporary files of crashed writers and lock files of departed
    /// clients deleted.
    pub stale_files_removed: u64,
    /// Snapshots asked of our session so our own log takes over a departed
    /// client's log.
    pub absorb_requests: u64,
}

/// What a log holds, as far as the cleanup is concerned (§4.7).
#[derive(Clone, Debug)]
struct LogSummary {
    /// `(len, mtime)` of the bytes summarized.
    stamp: (u64, Option<SystemTime>),
    /// `generation` of its header; `None` if the header is unreadable or not of
    /// this document.
    generation: Option<String>,
    /// End of the last intact record. A later read of the same generation goes
    /// on from here: its owner only ever appends to it.
    end: u64,
    /// Records after the header so far.
    records: u64,
    /// The log is self-contained up to this version: its first record is a full
    /// (not shallow) snapshot holding exactly these ops. Later records are deltas
    /// against what the folder had and may rely on ops in other logs, so they
    /// add nothing here.
    base: Option<VersionVector>,
    /// Upper bound of the ops in the log: the union of its records' versions
    /// (each is the writer's whole version at export time) and of the end
    /// versions the Loro blobs themselves declare.
    vmax: VersionVector,
    /// Every intact record so far is one we understand: `Updates` / `Snapshot`
    /// of this document whose Loro blobs decode.
    understood: bool,
    /// ... and nothing follows the last of them (no torn or corrupt tail). Only
    /// then is `vmax` known to bound everything in the log.
    complete: bool,
}

/// Whether a cleanup may run while holding a departed client's lock.
enum OwnerLock {
    /// We hold it (shared): the owner cannot be running, nor start until we
    /// drop this.
    Held(File),
    /// Held by someone: the owner is running.
    Busy,
    /// No lock file, or no locking on this file system: decide by time alone.
    Unknown,
}

/// The shared-folder [`Transport`]. Its I/O is synchronous and may be slow on a
/// NAS: run it on a background thread with [`crate::ThreadedTransport`].
pub struct DirTransport {
    root: PathBuf,
    client: ClientId,
    config: DirConfig,
    _lock: File,
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
    /// When the last cleanup pass ran (set at the first scan).
    last_cleanup: Option<Instant>,
    /// Cleanup summaries of logs, reused while a log's `(len, mtime)` is unchanged.
    summaries: BTreeMap<PathBuf, LogSummary>,
    /// Documents we asked our session to snapshot to take over a departed log,
    /// with the folder's version at the time: asked again only once it changed.
    absorbed: BTreeMap<DocKey, VersionVector>,
    /// The last reading of the file server's clock (`server_now`), and when.
    server_clock: Option<(Instant, SystemTime)>,
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
        let lock = lock_client(&updates.join(format!("{client}.{LOCK_EXT}")), &client)?;
        #[allow(unused_mut)]
        let mut t = DirTransport {
            root,
            client,
            config,
            _lock: lock,
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
            last_cleanup: None,
            summaries: BTreeMap::new(),
            absorbed: BTreeMap::new(),
            server_clock: None,
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
        let found = walk(&self.root.join(UPDATES), LOG_EXT);
        let bases = walk(&self.root.join(SUBLAYERS), SNAPSHOT_EXT);
        for (dir_rel, stem, path) in found {
            let Ok(doc) = DocKey::parse(&dir_rel) else {
                continue;
            };
            if stem == self.client.as_str() {
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
            self.last_cleanup = Some(Instant::now());
            self.inbox.push_back(self.hello());
        } else {
            self.maybe_cleanup();
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
        let now = local_now();
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
                // Only by the file server's clock: one of ours that runs a day
                // fast must not delete running clients' heartbeats, which the
                // cleanup (§4.7) takes as their sign of life.
                let old = self.server_now_cached().is_some_and(|server| {
                    mtime
                        .and_then(|m| server.duration_since(m).ok())
                        .is_some_and(|age| age > PRESENCE_GC)
                });
                if old {
                    let _ = fs::remove_file(&path);
                    continue;
                }
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
        let mut vmax = VersionVector::new();
        for msg in &msgs {
            if let Message::Updates { version, .. } | Message::Snapshot { version, .. } = msg {
                vmax.merge(version);
            }
        }
        self.own.insert(
            doc.clone(),
            OwnLog {
                path: path.to_owned(),
                generation: header.generation,
                len: good,
                compacted_len: header_len + first_snapshot,
                snapshot_requested: None,
                vmax,
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
                match read_all_records(&path) {
                    Ok(msgs) => {
                        for m in msgs {
                            self.deliver(m);
                        }
                    }
                    // Deleted underneath us (§4.7): the next append notices and
                    // asks for a full snapshot to rewrite it with.
                    Err(SyncError::Io { source, .. })
                        if source.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => self.note(Err(e)),
                }
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
        if !self.put(doc, version, &encode_record(&msg.encode()))? {
            // Our log vanished or was replaced (§4.7 took us for gone): get the
            // full state back in. The record alone is not written: it claims our
            // whole version, and a log holding only it would claim ops it lacks,
            // which nobody would send again and newcomers could not import. As
            // after a failed write, nothing is appended until our session's
            // snapshot rewrites the log, and the request is repeated (with
            // backoff) until one does, so a lost answer costs nothing either.
            self.stats.repairs += 1;
            self.fell_behind(doc);
            return Ok(());
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

    /// A write to our log for `doc` failed (or the log was deleted or replaced
    /// underneath us, see [`DirTransport::append`]). The session already counts
    /// those ops as written (it cannot tell), so its next update for `doc` starts
    /// after them. Appending that would leave a gap the folder never admits to: a
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

    /// Ask our session for a snapshot to compact our log of `doc` with, unless
    /// we did and the log has not doubled since (see
    /// [`OwnLog::snapshot_requested`]).
    fn request_snapshot(&mut self, doc: &DocKey) {
        if let Some(own) = self.own.get_mut(doc)
            && own
                .snapshot_requested
                .is_none_or(|asked_at| own.len > asked_at.saturating_mul(2))
        {
            own.snapshot_requested = Some(own.len);
            self.inbox
                .push_back(Message::SnapshotRequest { doc: doc.clone() });
        }
    }

    /// Append `record` (of `version`) to our log of `doc`, or start the log with
    /// it if we have none. `Ok(false)`: our log vanished or was replaced
    /// underneath us, and nothing was written.
    fn put(
        &mut self,
        doc: &DocKey,
        version: &VersionVector,
        record: &[u8],
    ) -> Result<bool, SyncError> {
        self.read_back_own(doc)?;
        match self.own.get_mut(doc) {
            Some(own) => {
                let appended = append_checked(own, record)?;
                if appended {
                    own.vmax.merge(version);
                }
                Ok(appended)
            }
            None => self.write_fresh(doc, version, record).map(|()| true),
        }
    }

    /// Compaction: replace our log with one snapshot record.
    ///
    /// Only if the snapshot holds everything in the log: our own log may be all
    /// that holds some ops (edits nobody saved, or a departed client's log that
    /// was deleted because ours held it, §4.7), and a snapshot our session made
    /// before importing records of the log (it starts from the saved project)
    /// would lose them. Such a snapshot is appended instead: nothing is lost,
    /// and the next compaction is due only once the log grew by the ratio again.
    fn compact(
        &mut self,
        doc: &DocKey,
        version: &VersionVector,
        msg: &Message,
    ) -> Result<(), SyncError> {
        let record = encode_record(&msg.encode());
        let written = self.read_back_own(doc).and_then(|()| {
            // No own log (none on disk either): nothing to cover.
            let covers = self
                .own
                .get(doc)
                .is_none_or(|own| version.includes_vv(&own.vmax));
            if covers {
                return self.write_fresh(doc, version, &record).map(|()| true);
            }
            if self.put(doc, version, &record)? {
                return Ok(false);
            }
            // The log was gone anyway: start it with the snapshot (which holds
            // exactly what it claims).
            self.write_fresh(doc, version, &record).map(|()| true)
        });
        let replaced = match written {
            Ok(replaced) => replaced,
            Err(e) => {
                // Ask again once the log grows, not never again this session.
                if let Some(own) = self.own.get_mut(doc) {
                    own.snapshot_requested = None;
                }
                return Err(e);
            }
        };
        let own = self.own.get_mut(doc).expect("written");
        own.compacted_len = own.len;
        own.snapshot_requested = None;
        if replaced {
            self.stats.compactions += 1;
        } else {
            self.stats.snapshots_appended += 1;
        }
        self.stats.records_written += 1;
        self.folder.entry(doc.clone()).or_default().merge(version);
        Ok(())
    }

    /// Read back our log of `doc` left by an earlier session if we have not yet
    /// ([`adopt_own`](Self::adopt_own)), before anything is written to it. The
    /// first scan does that, but it may have missed the log (a listing that
    /// failed on a network hiccup), and the log may hold edits nobody saved and,
    /// since departed clients' logs are deleted in favour of it (§4.7), the only
    /// copy of their ops: replacing it unread would lose them. `Err` if it cannot
    /// be read; nothing is written then.
    fn read_back_own(&mut self, doc: &DocKey) -> Result<(), SyncError> {
        if self.own.contains_key(doc) {
            return Ok(());
        }
        let path = self.log_path(doc);
        match fs::symlink_metadata(&path) {
            Ok(_) => self.adopt_own(doc, &path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SyncError::io(&path)(e)),
        }
    }

    /// Write a new log (new generation) holding `record` (of `version`), via a
    /// temporary file and a rename so readers never see a partial header.
    fn write_fresh(
        &mut self,
        doc: &DocKey,
        version: &VersionVector,
        record: &[u8],
    ) -> Result<(), SyncError> {
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
                snapshot_requested: None,
                vmax: version.clone(),
            },
        );
        Ok(())
    }

    // ---- cleanup of departed clients (§4.7) ----
    //
    // Every client ID that ever wrote (each app install, each agent process)
    // leaves `updates/<doc>/<client>.log` behind, and every poll stats all of
    // them. A pass every `cleanup_interval` deletes another client's log L (owner
    // X) only if all of these hold:
    //
    // 1. X is gone: none of X's logs nor `presence/X.presence` was modified for
    //    `inactive_after` (on the file server's clock, `server_now`), and X's
    //    lock `updates/X.lock` is free. We take that lock (without waiting) for
    //    the moment of the deletion, so X cannot start a session meanwhile; then
    //    L's length and time are checked once more.
    // 2. L is `complete` (every record understood), so `vmax(L)` bounds every op
    //    in it. A record we cannot decode might hold anything.
    // 3. Another log M (owner Y != X, our own included) starts with a full
    //    snapshot whose version `base(M)` includes `vmax(L)`: M holds everything
    //    L holds. M is checked on disk right before the deletion, not taken from
    //    the cache: its header (still the same generation, so still the same
    //    first record: an owner only appends to a generation; a rewritten log,
    //    e.g. of an owner that found its log vanished or corrupt, has a new one
    //    and may hold less) and whatever was appended since. Base snapshots
    //    (`sublayers/*.loro`, `history.loro`) are never used: `Project::save` of
    //    a client that is behind can overwrite them.
    // 4. If L could in turn replace M (base(L) includes vmax(M), so by (3) both
    //    hold exactly the same), only the one with the smaller client ID goes:
    //    X < Y.
    //
    // Why several cleaners cannot lose data between them: follow each deleted
    // log to the container it was deleted for. Ops move up that chain, ops(L) ⊆
    // vmax(L) ⊆ base(M) ⊆ ops(M) ⊆ vmax(M) ⊆ base(N) ..., and the chain cannot
    // go round in a cycle, so it ends at a log that survives. A cycle L1 → L2 →
    // ... → L1 would need vmax(L1) ⊆ base(L2) ⊆ vmax(L2) ⊆ ... ⊆ vmax(L1): all
    // equal, every log a container of the one before it, so (4) would need
    // X1 < X2 < ... < X1. So the order is only needed in that equal case: with
    // different versions, (3) already lets only the smaller log go, and a log
    // that is not self-contained can never be a container. (Only logs unchanged
    // for days get deleted, so cleaners running at the same time see the same
    // summaries of them. A container that is still written to only grows: its
    // owner appends to it, and replaces it only with a snapshot whose version
    // includes everything in it (`compact`). The only other rewrite is of a log
    // that vanished or was replaced, which no longer is that container.)

    fn maybe_cleanup(&mut self) {
        let Some(after) = self.config.inactive_after else {
            return;
        };
        let now = Instant::now();
        if self
            .last_cleanup
            .is_some_and(|t| now.saturating_duration_since(t) < self.config.cleanup_interval)
        {
            return;
        }
        self.last_cleanup = Some(now);
        self.stats.cleanups += 1;
        self.cleanup(after);
    }

    /// One cleanup pass. File-system errors only skip the file in question (and
    /// end up in `last_error`); the transport keeps working.
    fn cleanup(&mut self, after: Duration) {
        let Some(now) = self.server_now() else {
            return;
        };
        // Logs by document, lock files, and how long each client has been idle
        // (the youngest of its logs and presence file).
        let mut logs: BTreeMap<DocKey, Vec<(String, PathBuf)>> = BTreeMap::new();
        let mut locks: Vec<(String, PathBuf)> = Vec::new();
        let mut idle: BTreeMap<String, Duration> = BTreeMap::new();
        let mut files = Vec::new();
        walk_files(&self.root.join(UPDATES), String::new(), 0, &mut files);
        for (rel, name, path) in files {
            if name.contains(TMP_MARK) {
                self.remove_stale_file(&path, now, after);
                continue;
            }
            let (Some(stem), Some(ext)) = (
                path.file_stem().and_then(|s| s.to_str()).map(str::to_owned),
                path.extension().and_then(|e| e.to_str()),
            ) else {
                continue;
            };
            if rel.is_empty() {
                if ext == LOCK_EXT {
                    locks.push((stem, path));
                }
            } else if ext == LOG_EXT
                && let Ok(doc) = DocKey::parse(&rel)
                && let Some(age) = file_age(&path, now)
            {
                note_idle(&mut idle, &stem, age);
                logs.entry(doc).or_default().push((stem, path));
            }
        }
        if let Ok(entries) = fs::read_dir(self.root.join(PRESENCE)) {
            let suffix = format!(".{PRESENCE_EXT}");
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.contains(TMP_MARK) {
                    self.remove_stale_file(&path, now, after);
                } else if let Some(owner) = name.strip_suffix(&suffix)
                    && let Some(age) = file_age(&path, now)
                {
                    note_idle(&mut idle, owner, age);
                }
            }
        }
        let us = self.client.as_str().to_owned();
        let gone = |owner: &str| {
            owner != us
                && ClientId::new(owner).is_ok()
                && idle.get(owner).is_none_or(|&age| age > after)
        };
        let all: BTreeSet<&PathBuf> = logs.values().flatten().map(|(_, p)| p).collect();
        self.summaries.retain(|p, _| all.contains(p));
        let mut left: BTreeMap<String, usize> = BTreeMap::new();
        for (owner, _) in logs.values().flatten() {
            *left.entry(owner.clone()).or_default() += 1;
        }
        for (doc, entries) in logs {
            if !entries.iter().any(|(owner, _)| gone(owner)) {
                continue;
            }
            for owner in self.cleanup_doc(&doc, entries, &gone, now, after) {
                if let Some(n) = left.get_mut(&owner) {
                    *n -= 1;
                }
            }
        }
        // Lock files of departed clients without logs: only while we hold the
        // lock, so no session of theirs runs. One that opened the file just
        // before we delete it finds it gone once it gets the lock, and takes a
        // new one (`lock_client`).
        for (owner, path) in locks {
            if !gone(&owner)
                || left.get(&owner).is_some_and(|&n| n > 0)
                || file_age(&path, now).is_none_or(|age| age <= after)
            {
                continue;
            }
            if let OwnerLock::Held(guard) = self.lock_owner(&owner) {
                match fs::remove_file(&path) {
                    Ok(()) => self.stats.stale_files_removed += 1,
                    Err(e) => self.note_cleanup_error(&path, e),
                }
                drop(guard);
            }
        }
    }

    /// The part of a cleanup pass for `doc`, whose logs are `entries` (owner,
    /// path). Returns the owners whose log was deleted.
    fn cleanup_doc(
        &mut self,
        doc: &DocKey,
        entries: Vec<(String, PathBuf)>,
        gone: &dyn Fn(&str) -> bool,
        now: SystemTime,
        after: Duration,
    ) -> Vec<String> {
        // The departed owners' logs first: unless one of them could go, the
        // others need not be read at all.
        let (departed, others): (Vec<_>, Vec<_>) =
            entries.into_iter().partition(|(owner, _)| gone(owner));
        let mut logs: Vec<(String, PathBuf, LogSummary)> = Vec::new();
        for (owner, path) in departed {
            if let Some(summary) = self.summary(doc, &path, false) {
                logs.push((owner, path, summary));
            }
        }
        if !logs.iter().any(|(.., l)| l.complete) {
            return Vec::new();
        }
        for (owner, path) in others {
            if let Some(summary) = self.summary(doc, &path, false) {
                logs.push((owner, path, summary));
            }
        }
        let mut deleted = vec![false; logs.len()];
        let mut removed = Vec::new();
        for i in 0..logs.len() {
            let (x, l_path, l) = logs[i].clone();
            if !gone(&x) || !l.complete {
                continue;
            }
            let containers: Vec<usize> = (0..logs.len())
                .filter(|&j| {
                    !deleted[j] && logs[j].0 != x && may_replace(&logs[j].2, &logs[j].0, &l, &x)
                })
                .collect();
            if containers.is_empty() {
                let ours = logs
                    .iter()
                    .zip(&deleted)
                    .find(|((owner, ..), del)| !**del && owner == self.client.as_str())
                    .map(|((.., summary), _)| summary.clone());
                self.maybe_absorb(doc, &l.vmax, ours.as_ref());
                continue;
            }
            // The first container that still holds L on disk (rule 3): one that
            // changed while being read, or no longer holds it, makes way for the
            // next. Each check reads its header and what was appended since the
            // last read, not the whole log.
            for j in containers {
                let m_path = logs[j].1.clone();
                #[cfg(test)]
                seam::hit(seam::Point::ContainerCheck, &m_path);
                let Some(fresh) = self.summary(doc, &m_path, true) else {
                    continue;
                };
                let holds = may_replace(&fresh, &logs[j].0, &l, &x);
                logs[j].2 = fresh;
                if holds {
                    if self.delete_departed_log(&x, &l_path, l.stamp, now, after) {
                        deleted[i] = true;
                        removed.push(x.clone());
                    }
                    break;
                }
            }
        }
        removed
    }

    /// Delete the departed client `owner`'s log at `path` (summarized at `stamp`)
    /// if it is still unchanged and `owner` still looks gone at `now`.
    fn delete_departed_log(
        &mut self,
        owner: &str,
        path: &Path,
        stamp: (u64, Option<SystemTime>),
        now: SystemTime,
        after: Duration,
    ) -> bool {
        let log_idle = stamp
            .1
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > after);
        let presence = self
            .root
            .join(PRESENCE)
            .join(format!("{owner}.{PRESENCE_EXT}"));
        if !log_idle || file_age(&presence, now).is_some_and(|age| age <= after) {
            return false;
        }
        // Held until the log is gone: a running owner holds its lock all session,
        // and a starting one waits for us (`lock_client`), so only the last look
        // and the unlink happen while we hold it. An owner that ran and quit
        // since the checks above either wrote to the log (the last look sees it)
        // or left it as it was (still held by its container).
        let guard = match self.lock_owner(owner) {
            OwnerLock::Busy => return false,
            OwnerLock::Held(f) => Some(f),
            OwnerLock::Unknown => None,
        };
        // The last look at the log, right before unlinking it: through a new
        // handle, because opening revalidates cached attributes (NFS
        // close-to-open), where a bare stat may be answered from a cache.
        let unchanged = File::open(path)
            .and_then(|f| f.metadata())
            .is_ok_and(|m| (m.len(), m.modified().ok()) == stamp);
        if !unchanged {
            return false;
        }
        let removed = fs::remove_file(path);
        drop(guard);
        match removed {
            Ok(()) => self.stats.logs_removed += 1,
            // Another cleaner was quicker.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                self.note_cleanup_error(path, e);
                return false;
            }
        }
        self.sources.remove(path);
        self.summaries.remove(path);
        true
    }

    /// Take over a departed client's log that no other log holds: ask our
    /// session for a snapshot of `doc`, so our own log starts with one that holds
    /// `vmax` (if our session has all of it) and a later pass can delete the
    /// departed log. Asked again only once the folder has changed, so a request
    /// that cannot help (our session lacks some of it, or does not have `doc`
    /// at all and never answers) is not repeated every pass; nothing waits for
    /// the answer. The answer replaces our log only if it holds everything in it
    /// (`compact`).
    fn maybe_absorb(&mut self, doc: &DocKey, vmax: &VersionVector, ours: Option<&LogSummary>) {
        if !self.config.absorb_departed {
            return;
        }
        // Our log already holds it and only rule 4 stands in the way (the departed
        // log holds the same and has a larger ID): a new snapshot would be the
        // same until someone edits, and then the edit itself unblocks it.
        if ours
            .and_then(|s| s.base.as_ref())
            .is_some_and(|base| base.includes_vv(vmax))
        {
            return;
        }
        let folder = self.folder.get(doc).cloned().unwrap_or_default();
        if self.absorbed.get(doc) == Some(&folder) {
            return;
        }
        self.absorbed.insert(doc.clone(), folder);
        self.stats.absorb_requests += 1;
        self.inbox
            .push_back(Message::SnapshotRequest { doc: doc.clone() });
    }

    /// Summary of the log of `doc` at `path`: from the cache while its length
    /// and time are unchanged (unless `fresh`), otherwise from disk, reading only
    /// what follows the cached part if the log is still the same generation.
    fn summary(&mut self, doc: &DocKey, path: &Path, fresh: bool) -> Option<LogSummary> {
        if !fresh && let Some(cached) = self.summaries.get(path) {
            let stamp = match fs::metadata(path) {
                Ok(m) => (m.len(), m.modified().ok()),
                Err(e) => {
                    self.note_cleanup_error(path, e);
                    return None;
                }
            };
            if stamp.1.is_some() && cached.stamp == stamp {
                return Some(cached.clone());
            }
        }
        match summarize_log(path, doc, self.summaries.get(path)) {
            Ok(Some(summary)) => {
                self.summaries.insert(path.to_owned(), summary.clone());
                Some(summary)
            }
            Ok(None) => None,
            Err(e) => {
                self.note_cleanup_error(path, e);
                None
            }
        }
    }

    /// Best-effort check whether `owner` runs a session: try to take its lock.
    ///
    /// A shared lock on a read-only handle: it conflicts with the owner's
    /// exclusive lock both ways (`flock`, NFS's emulation of it with POSIX
    /// locks, `LockFileEx`), so a running owner shows as busy and a starting one
    /// waits for us, and it needs only read access to another user's lock file
    /// (on NFS an exclusive lock needs a writable handle, a shared one a readable
    /// one; lock files are typically not writable by other users).
    fn lock_owner(&mut self, owner: &str) -> OwnerLock {
        let path = self.root.join(UPDATES).join(format!("{owner}.{LOCK_EXT}"));
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return OwnerLock::Unknown,
            // Cannot tell: assume it is running, and say why nothing is deleted.
            Err(e) => {
                self.note_cleanup_error(&path, e);
                return OwnerLock::Busy;
            }
        };
        match file.try_lock_shared() {
            Ok(()) => OwnerLock::Held(file),
            Err(fs::TryLockError::WouldBlock) => OwnerLock::Busy,
            Err(fs::TryLockError::Error(_)) => OwnerLock::Unknown,
        }
    }

    /// "Now" for measuring how old files are: the time the file server stamps on
    /// a file we write now, or our own clock if that is earlier. A cleaner whose
    /// clock is days ahead (a restored VM, a dead clock battery) then neither
    /// takes everyone for gone nor deletes temporary files still being written;
    /// the earlier of the two only ever makes files look younger, so a server
    /// that stamps odd times cannot make anything look older either. `None` (no
    /// pass) if the probe cannot be written.
    fn server_now(&mut self) -> Option<SystemTime> {
        let path =
            self.root
                .join(UPDATES)
                .join(format!("{}{TMP_MARK}{}", self.client, new_generation()));
        let probe = || -> io::Result<SystemTime> {
            fs::write(&path, b"clock")?;
            fs::metadata(&path)?.modified()
        };
        let now = probe();
        let _ = fs::remove_file(&path);
        match now {
            Ok(t) => {
                let t = t.min(local_now());
                self.server_clock = Some((Instant::now(), t));
                Some(t)
            }
            Err(e) => {
                self.note_cleanup_error(&path, e);
                None
            }
        }
    }

    /// [`server_now`](Self::server_now), from a reading at most
    /// `SERVER_CLOCK_REUSE` old (plus the time since; never later than our own
    /// clock).
    fn server_now_cached(&mut self) -> Option<SystemTime> {
        if let Some((at, t)) = self.server_clock {
            let since = at.elapsed();
            if since < SERVER_CLOCK_REUSE {
                return Some((t + since).min(local_now()));
            }
        }
        self.server_now()
    }

    /// Delete a leftover temporary file of a crashed writer.
    fn remove_stale_file(&mut self, path: &Path, now: SystemTime, after: Duration) {
        if file_age(path, now).is_some_and(|age| age > after) {
            match fs::remove_file(path) {
                Ok(()) => self.stats.stale_files_removed += 1,
                Err(e) => self.note_cleanup_error(path, e),
            }
        }
    }

    /// Cleanup problems are reported like poll problems; a file that vanished
    /// meanwhile (another cleaner, its owner compacting) is not a problem.
    fn note_cleanup_error(&mut self, path: &Path, e: io::Error) {
        if e.kind() != io::ErrorKind::NotFound {
            self.last_error = Some(format!("cleanup {}: {e}", path.display()));
        }
    }
}

impl Transport for DirTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
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
    /// behind (a write failed, or the log was deleted underneath us) and was not
    /// rewritten yet: the edits since are in no file. The next session on the
    /// folder writes them if the documents still hold them (see
    /// [`DirTransport::fell_behind`]).
    fn close(&mut self) -> Result<(), SyncError> {
        match self.behind.keys().next() {
            Some(doc) => Err(SyncError::io(&self.log_path(doc))(io::Error::other(
                "the log lacks edits (a write failed, or it was deleted underneath us) and was not rewritten yet",
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
    let mut f = match OpenOptions::new().read(true).append(true).open(&path) {
        Ok(f) => f,
        // Deleted since the check above (by a cleanup pass that took us for gone).
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
    #[cfg(test)]
    seam::hit(seam::Point::AppendOpened, &path);
    // No fsync per append: closing the file hands the data to the SMB server, a
    // torn tail after a crash is cut off on the next start, and the edits are
    // still in the session's documents. Rewrites (compaction) are synced because
    // a rename must never expose a file whose data is not on disk.
    f.write_all(record).map_err(SyncError::io(&path))?;
    drop(f);
    // Defence in depth against a cleanup pass elsewhere (§4.7) that took us for
    // gone and deleted the log between the length check above and the write: on
    // POSIX the record then went to an unlinked file nobody will ever read, while
    // we would count it as written. So check that the path still has our bytes;
    // if not, the caller asks for a full snapshot to rewrite the log with, as
    // for any repair. What remains is a cleaner that unlinks the log after this
    // check, with our open, write, close and check all inside its own window
    // between its last look at the log (through a new handle) and the unlink,
    // a few round trips. The cleaner only gets that far if for `inactive_after`
    // (days, on the server's clock) we wrote nothing and sent no heartbeat and
    // it could take our lock, which we hold all session; and even then our next
    // append finds the log missing and repairs it with a full snapshot, so only
    // a last append before quitting without saving could be lost. That is
    // acceptable. Unverifiable (another error): assume it is fine; the next
    // append checks again.
    let expected = own.len + record.len() as u64;
    match fs::metadata(&path) {
        Ok(m) if m.len() < expected => return Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        _ => {}
    }
    own.len = expected;
    Ok(true)
}

/// Take the advisory lock at `path` for `client`'s session. File systems without
/// locking are accepted as they are.
fn lock_client(path: &Path, client: &ClientId) -> Result<File, SyncError> {
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(SyncError::io(path))?;
        match lock.try_lock() {
            Ok(()) => match fs::metadata(path) {
                // A cleanup pass deleted the file we opened while it held the lock
                // (§4.7): lock the file that is there now.
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                _ => return Ok(lock),
            },
            Err(fs::TryLockError::Error(_)) => return Ok(lock),
            // Possibly a cleanup pass holding it for a moment.
            Err(fs::TryLockError::WouldBlock) => {}
        }
        if Instant::now() >= deadline {
            return Err(SyncError::ClientIdInUse(client.clone()));
        }
        std::thread::sleep(LOCK_RETRY);
    }
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

/// Files with extension `ext` under `dir`, as `(directory relative to `dir` with
/// '/' separators, file stem, path)`.
fn walk(dir: &Path, ext: &str) -> Vec<(String, String, PathBuf)> {
    let mut files = Vec::new();
    walk_files(dir, String::new(), 0, &mut files);
    files
        .into_iter()
        .filter_map(|(rel, _, path)| {
            if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                return None;
            }
            let stem = path.file_stem()?.to_str()?.to_owned();
            Some((rel, stem, path))
        })
        .collect()
}

/// Every file under `dir`, as `(directory relative to the walk root with '/'
/// separators, file name, path)`.
fn walk_files(dir: &Path, rel: String, depth: usize, out: &mut Vec<(String, String, PathBuf)>) {
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
            walk_files(&path, child, depth + 1, out);
        } else {
            out.push((rel.clone(), name, path));
        }
    }
}

/// Our own clock. Tests can set it ahead (`seam::set_clock_ahead`).
fn local_now() -> SystemTime {
    let now = SystemTime::now();
    #[cfg(test)]
    let now = now + seam::clock_ahead();
    now
}

/// How long ago `path` was modified; `None` if it does not exist. Unknown or
/// future times count as "just now".
fn file_age(path: &Path, now: SystemTime) -> Option<Duration> {
    match fs::metadata(path) {
        Ok(m) => Some(
            m.modified()
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .unwrap_or_default(),
        ),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(_) => Some(Duration::ZERO),
    }
}

/// Record a sign of life of `owner` `age` ago (keeping the youngest).
fn note_idle(idle: &mut BTreeMap<String, Duration>, owner: &str, age: Duration) {
    idle.entry(owner.to_owned())
        .and_modify(|a| *a = (*a).min(age))
        .or_insert(age);
}

/// Whether the log `m` (owner `y`) holds everything in the log `l` (owner `x`), so
/// that `l` may be deleted in its favour (§4.7, rules 3 and 4). `l` must be
/// `complete` for its `vmax` to mean anything; the caller checks that.
fn may_replace(m: &LogSummary, y: &str, l: &LogSummary, x: &str) -> bool {
    let holds = |container: &LogSummary, vmax: &VersionVector| {
        container
            .base
            .as_ref()
            .is_some_and(|base| base.includes_vv(vmax))
    };
    // `m.vmax` only counts the records we understand, so `l` may look like a
    // container of `m` when it is not: that only makes us stricter.
    holds(m, &l.vmax) && (!holds(l, &m.vmax) || x < y)
}

/// Read the log of `doc` at `path` for the cleanup. If it is still the
/// generation `prev` (an earlier summary of it) summarized, only the bytes
/// after `prev` are read: the owner only ever appends to a generation, so what
/// `prev` covers is unchanged. `Ok(None)`: it changed while being read.
fn summarize_log(
    path: &Path,
    doc: &DocKey,
    prev: Option<&LogSummary>,
) -> io::Result<Option<LogSummary>> {
    let stamp = |m: fs::Metadata| (m.len(), m.modified().ok());
    let mut f = File::open(path)?;
    // The handle's attributes: opening revalidates them (NFS close-to-open).
    let before = stamp(f.metadata()?);
    let mut cont = None;
    if let Some(prev) = prev.filter(|p| p.end <= before.0)
        && let Some(generation) = &prev.generation
        && read_header(&mut f)?
            .is_some_and(|(h, _)| &h.generation == generation && h.doc == doc.as_str())
    {
        cont = Some(prev.clone());
    }
    let start = cont.as_ref().map_or(0, |p| p.end);
    f.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    let after = stamp(f.metadata()?);
    if before != after || after.0 != start + bytes.len() as u64 {
        return Ok(None);
    }
    Ok(Some(match cont {
        Some(mut summary) => {
            summary.stamp = after;
            summary.extend(&bytes, doc);
            summary
        }
        None => summarize(&bytes, doc, after),
    }))
}

/// Summary of the whole log `bytes` of `doc` (read at `stamp`).
fn summarize(bytes: &[u8], doc: &DocKey, stamp: (u64, Option<SystemTime>)) -> LogSummary {
    let mut summary = LogSummary {
        stamp,
        generation: None,
        end: 0,
        records: 0,
        base: None,
        vmax: VersionVector::new(),
        understood: false,
        complete: false,
    };
    let Some((payload, size)) = next_record(bytes) else {
        return summary;
    };
    let header = (payload.len() <= MAX_HEADER)
        .then(|| parse_header(payload))
        .flatten();
    let Some(header) = header.filter(|h| h.doc == doc.as_str()) else {
        return summary;
    };
    summary.generation = Some(header.generation);
    summary.end = size as u64;
    summary.understood = true;
    summary.extend(&bytes[size..], doc);
    summary
}

impl LogSummary {
    /// Take in `bytes`, the part of the log that follows `end`.
    fn extend(&mut self, bytes: &[u8], doc: &DocKey) {
        let (records, used) = parse_records(bytes);
        for payload in records {
            let first = self.records == 0;
            self.records += 1;
            let (snapshot, version, data) = match Message::decode(payload) {
                Ok(Message::Updates {
                    doc: d,
                    version,
                    data,
                }) if d == *doc => (false, version, data),
                Ok(Message::Snapshot {
                    doc: d,
                    version,
                    data,
                }) if d == *doc => (true, version, data),
                _ => {
                    self.understood = false;
                    continue;
                }
            };
            self.vmax.merge(&version);
            match LoroDoc::decode_import_blob_meta(&data, true) {
                Ok(meta) => {
                    self.vmax.merge(&meta.partial_end_vv);
                    if first && snapshot && meta.mode == EncodedBlobMode::Snapshot {
                        self.base = Some(meta.partial_end_vv);
                    }
                }
                Err(_) => self.understood = false,
            }
        }
        self.end += used as u64;
        self.complete = self.understood && used == bytes.len();
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
    while let Some((payload, size)) = next_record(&buf[pos..]) {
        out.push(payload);
        pos += size;
    }
    (out, pos)
}

/// The record at the start of `buf` and its size, if it is complete and intact.
fn next_record(buf: &[u8]) -> Option<(&[u8], usize)> {
    if buf.len() < RECORD_HEADER {
        return None;
    }
    let len = u32::from_le_bytes(buf[..4].try_into().expect("4 bytes")) as usize;
    if len > MAX_RECORD || buf.len() - RECORD_HEADER < len {
        return None;
    }
    let payload = &buf[RECORD_HEADER..RECORD_HEADER + len];
    (buf[4..RECORD_HEADER] == checksum(payload)).then_some((payload, RECORD_HEADER + len))
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
    Ok(parse_header(&payload).map(|header| (header, (RECORD_HEADER + len) as u64)))
}

/// A header record's payload, if it is one this build can read.
fn parse_header(payload: &[u8]) -> Option<LogHeader> {
    let header = serde_json::from_slice::<LogHeader>(payload).ok()?;
    (header.format == LOG_FORMAT && header.version <= LOG_VERSION).then_some(header)
}

/// Test seams: unit tests act at the moments another process could interfere.
#[cfg(test)]
mod seam {
    use std::cell::{Cell, RefCell};
    use std::path::Path;
    use std::time::Duration;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Point {
        /// `append_checked` opened our log and is about to write to it.
        AppendOpened,
        /// A cleanup pass is about to check the container of a departed log on
        /// disk.
        ContainerCheck,
    }

    type Hook = Box<dyn FnMut(Point, &Path)>;

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
        static CLOCK_AHEAD: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    }

    /// Our own clock (`local_now`) runs this far ahead on this thread.
    pub(super) fn set_clock_ahead(by: Duration) {
        CLOCK_AHEAD.with(|c| c.set(by));
    }

    pub(super) fn clock_ahead() -> Duration {
        CLOCK_AHEAD.with(Cell::get)
    }

    /// Run `hook` at every seam this thread passes, until `clear`.
    pub(super) fn set(hook: impl FnMut(Point, &Path) + 'static) {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
    }

    pub(super) fn clear() {
        HOOK.with(|h| *h.borrow_mut() = None);
    }

    /// Seams passed while the hook runs (it may drive another transport) are
    /// not hooked again.
    pub(super) fn hit(point: Point, path: &Path) {
        HOOK.with(|h| {
            if let Ok(mut hook) = h.try_borrow_mut()
                && let Some(hook) = hook.as_mut()
            {
                hook(point, path);
            }
        });
    }
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

    fn header(doc: &str) -> Vec<u8> {
        let header = LogHeader {
            format: LOG_FORMAT.to_owned(),
            version: LOG_VERSION,
            doc: doc.to_owned(),
            client: "x".to_owned(),
            generation: new_generation(),
        };
        encode_record(&serde_json::to_vec(&header).unwrap())
    }

    fn record(msg: Message) -> Vec<u8> {
        encode_record(&msg.encode())
    }

    /// A Loro document at two versions: its snapshot at the first, its update
    /// from the first to the second.
    fn two_versions() -> (VersionVector, Vec<u8>, VersionVector, Vec<u8>) {
        use bae_model::loro::ExportMode;
        let doc = LoroDoc::new();
        doc.get_map("m").insert("a", 1).unwrap();
        doc.commit();
        let v1 = doc.oplog_vv();
        let snapshot = doc.export(ExportMode::Snapshot).unwrap();
        doc.get_map("m").insert("b", 2).unwrap();
        doc.commit();
        let v2 = doc.oplog_vv();
        let update = doc.export(ExportMode::updates(&v1)).unwrap();
        (v1, snapshot, v2, update)
    }

    #[test]
    fn summaries_bound_what_a_log_holds() {
        let root = DocKey::parse("root").unwrap();
        let other = DocKey::parse("anim/a").unwrap();
        let (v1, snapshot, v2, update) = two_versions();
        let snap = |doc: &DocKey| {
            record(Message::Snapshot {
                doc: doc.clone(),
                version: v1.clone(),
                data: snapshot.clone(),
            })
        };
        let upd = |doc: &DocKey, data: &[u8]| {
            record(Message::Updates {
                doc: doc.clone(),
                version: v2.clone(),
                data: data.to_vec(),
            })
        };
        let summary = |parts: &[Vec<u8>]| summarize(&parts.concat(), &root, (0, None));

        // Starts with a snapshot: self-contained up to exactly the snapshot; the
        // later delta only raises the upper bound.
        let s = summary(&[header("root"), snap(&root), upd(&root, &update)]);
        assert!(s.complete);
        assert_eq!(s.base, Some(v1.clone()));
        assert_eq!(s.vmax, v2);
        // Starts with a delta: not self-contained, even with a snapshot later.
        let s = summary(&[header("root"), upd(&root, &update), snap(&root)]);
        assert!(s.complete);
        assert_eq!(s.base, None);
        assert_eq!(s.vmax, v2);

        // Anything we do not understand makes the log incomplete.
        let full = [header("root"), snap(&root), upd(&root, &update)].concat();
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("unknown message type", encode_record(&[0x7f, 1, 2, 3])),
            ("not a Loro blob", upd(&root, b"not loro")),
            ("record of another document", upd(&other, &update)),
            ("torn tail", upd(&root, &update)[..20].to_vec()),
        ];
        for (what, extra) in cases {
            let s = summary(&[full.clone(), extra]);
            assert!(!s.complete, "{what}");
            assert_eq!(s.base, Some(v1.clone()), "{what}");
        }
        let s = summary(&[header("anim/a"), snap(&root)]);
        assert!(!s.complete, "header of another document");
        assert_eq!(s.base, None);
        let mut corrupt = header("root");
        corrupt[RECORD_HEADER] ^= 1;
        assert!(!summary(&[corrupt, snap(&root)]).complete, "corrupt header");
    }

    #[test]
    fn a_log_is_only_replaced_by_one_that_holds_it() {
        let (v1, _, v2, _) = two_versions();
        let log = |base: Option<&VersionVector>, vmax: &VersionVector| LogSummary {
            stamp: (0, None),
            generation: Some(new_generation()),
            end: 0,
            records: 1,
            base: base.cloned(),
            vmax: vmax.clone(),
            understood: true,
            complete: true,
        };
        let small = log(Some(&v1), &v1);
        let full = log(Some(&v2), &v2);
        // Strictly smaller: only that one goes, whatever the IDs.
        assert!(may_replace(&full, "a", &small, "z"));
        assert!(!may_replace(&small, "z", &full, "a"));
        // A snapshot followed by more: its base is what counts as held.
        let more = log(Some(&v1), &v2);
        assert!(!may_replace(&more, "a", &full, "z"));
        assert!(may_replace(&full, "z", &more, "a"));
        assert!(may_replace(&full, "a", &more, "z"));
        // Not self-contained: never a container, so it may go even when its
        // versions equal the container's and its ID is larger.
        let deltas = log(None, &v2);
        assert!(!may_replace(&deltas, "a", &small, "z"));
        assert!(may_replace(&full, "a", &deltas, "z"));
        // Holding exactly the same: only the smaller ID goes, so two cleaners can
        // never delete both.
        assert!(may_replace(&full, "b", &full.clone(), "a"));
        assert!(!may_replace(&full, "a", &full.clone(), "b"));
    }

    /// Snapshots of one Loro document at two versions, and the update between.
    struct Blobs {
        v1: VersionVector,
        snap1: Vec<u8>,
        v2: VersionVector,
        snap2: Vec<u8>,
        update: Vec<u8>,
    }

    fn blobs() -> Blobs {
        use bae_model::loro::ExportMode;
        let doc = LoroDoc::new();
        doc.get_map("m").insert("a", 1).unwrap();
        doc.commit();
        let v1 = doc.oplog_vv();
        let snap1 = doc.export(ExportMode::Snapshot).unwrap();
        doc.get_map("m").insert("b", 2).unwrap();
        doc.commit();
        Blobs {
            update: doc.export(ExportMode::updates(&v1)).unwrap(),
            v1,
            snap1,
            v2: doc.oplog_vv(),
            snap2: doc.export(ExportMode::Snapshot).unwrap(),
        }
    }

    fn root() -> DocKey {
        DocKey::parse("root").unwrap()
    }

    fn updates(version: &VersionVector, data: &[u8]) -> Message {
        Message::Updates {
            doc: root(),
            version: version.clone(),
            data: data.to_vec(),
        }
    }

    fn snapshot(version: &VersionVector, data: &[u8]) -> Message {
        Message::Snapshot {
            doc: root(),
            version: version.clone(),
            data: data.to_vec(),
        }
    }

    fn quick() -> DirConfig {
        DirConfig {
            poll_interval: Duration::ZERO,
            watch: false,
            ..DirConfig::default()
        }
    }

    fn transport(dir: &Path, id: &str, config: DirConfig) -> DirTransport {
        DirTransport::open(dir, ClientId::new(id).unwrap(), config).unwrap()
    }

    /// Everything the transport has for its session now, polling as needed.
    fn drain(t: &mut DirTransport) -> Vec<Message> {
        std::iter::from_fn(|| t.recv().unwrap()).collect()
    }

    fn summary_of(path: &Path) -> LogSummary {
        summarize(&fs::read(path).unwrap(), &root(), (0, None))
    }

    fn append_bytes(path: &Path, bytes: &[u8]) {
        OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
    }

    /// Replace the file at `path` as an owner rewriting its log does.
    fn replace(path: &Path, bytes: &[u8]) {
        let tmp = path.with_extension("tmp-test");
        fs::write(&tmp, bytes).unwrap();
        fs::rename(&tmp, path).unwrap();
    }

    #[test]
    fn a_snapshot_replaces_the_own_log_only_if_it_holds_all_of_it() {
        let tmp = tempfile::tempdir().unwrap();
        let b = blobs();
        // An earlier session wrote an update at v2.
        let mut t = transport(tmp.path(), "a", quick());
        t.send(updates(&b.v2, &b.update)).unwrap();
        let path = t.log_path(&root());
        drop(t);
        // The next session starts from an older state (the saved project). Its
        // snapshot lacks what the log has: appended to the log, not put in its
        // place.
        let mut t = transport(tmp.path(), "a", quick());
        drain(&mut t);
        let before = fs::read(&path).unwrap();
        t.send(snapshot(&b.v1, &b.snap1)).unwrap();
        let after = fs::read(&path).unwrap();
        assert!(after.len() > before.len() && after.starts_with(&before));
        assert_eq!(t.stats().compactions, 0);
        assert_eq!(t.stats().snapshots_appended, 1);
        assert_eq!(summary_of(&path).records, 2);
        // One that holds everything does replace it.
        t.send(snapshot(&b.v2, &b.snap2)).unwrap();
        let s = summary_of(&path);
        assert_eq!((s.records, s.base), (1, Some(b.v2.clone())));
        assert_eq!(t.stats().compactions, 1);
        // So does one after a later append, which only had to cover more.
        t.send(updates(&b.v2, &b.update)).unwrap();
        t.send(snapshot(&b.v2, &b.snap2)).unwrap();
        assert_eq!(summary_of(&path).records, 1);
        assert_eq!(t.stats().compactions, 2);
        assert_eq!(t.stats().snapshots_appended, 1);
    }

    #[test]
    fn a_record_lost_to_a_log_deleted_while_appending_is_noticed() {
        let tmp = tempfile::tempdir().unwrap();
        let b = blobs();
        let mut t = transport(tmp.path(), "a", quick());
        drain(&mut t);
        t.send(updates(&b.v1, &b.snap1)).unwrap();
        let path = t.log_path(&root());
        // Between our length check and our write, a cleanup pass elsewhere took
        // us for gone: the log is deleted (on POSIX the write then goes to a file
        // nobody can open any more) or cut short.
        type Interfere = fn(&Path);
        let cases: [(&str, Interfere); 2] = [
            ("deleted", |p| fs::remove_file(p).unwrap()),
            ("truncated", |p| {
                OpenOptions::new()
                    .write(true)
                    .open(p)
                    .unwrap()
                    .set_len(0)
                    .unwrap()
            }),
        ];
        for (n, (what, interfere)) in cases.into_iter().enumerate() {
            let mut armed = true;
            seam::set(move |point, p| {
                if point == seam::Point::AppendOpened && std::mem::take(&mut armed) {
                    interfere(p);
                }
            });
            let sent = t.send(updates(&b.v2, &b.update));
            seam::clear();
            sent.unwrap();
            // Noticed at once: our session is asked for everything. The record
            // alone is not written again (a log holding only it would claim
            // more than it holds): no log readers can parse holds it.
            assert_eq!(t.stats().repairs, n as u64 + 1, "{what}");
            let left = fs::read(&path).unwrap_or_default();
            assert_eq!(summarize(&left, &root(), (0, None)).records, 0, "{what}");
            assert!(
                matches!(t.recv().unwrap(), Some(Message::SnapshotRequest { .. })),
                "{what}"
            );
            t.send(snapshot(&b.v2, &b.snap2)).unwrap();
            let s = summary_of(&path);
            assert_eq!((s.records, s.base), (1, Some(b.v2.clone())), "{what}");
        }
    }

    #[test]
    fn a_container_is_checked_on_disk_right_before_the_deletion() {
        let tmp = tempfile::tempdir().unwrap();
        let b = blobs();
        let dir = tmp.path().join(UPDATES).join("root");
        fs::create_dir_all(&dir).unwrap();
        let (x, c) = (dir.join("x.log"), dir.join("c.log"));
        // x left long ago; c's log starts with a snapshot that holds all of x's.
        fs::write(
            &x,
            [header("root"), record(updates(&b.v1, &b.snap1))].concat(),
        )
        .unwrap();
        File::options()
            .write(true)
            .open(&x)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(7200))
            .unwrap();
        let container = [header("root"), record(snapshot(&b.v2, &b.snap2))].concat();
        let deltas = [header("root"), record(updates(&b.v2, &b.update))].concat();
        fs::write(&c, &container).unwrap();
        let config = DirConfig {
            inactive_after: Some(Duration::from_secs(3600)),
            cleanup_interval: Duration::ZERO,
            absorb_departed: false,
            ..quick()
        };
        let mut a = transport(tmp.path(), "a", config);
        // Between choosing c's log and deleting x's, c rewrites its log, and the
        // new one is no container: x's stays.
        let mut armed = true;
        let target = c.clone();
        seam::set(move |point, p| {
            if point == seam::Point::ContainerCheck && p == target && std::mem::take(&mut armed) {
                replace(p, &deltas);
            }
        });
        drain(&mut a);
        drain(&mut a);
        seam::clear();
        assert!(a.stats().cleanups >= 2);
        assert!(x.exists());
        assert_eq!(a.stats().logs_removed, 0);
        // Once c's log holds x's again, x's goes.
        replace(&c, &container);
        drain(&mut a);
        assert!(!x.exists());
        assert_eq!(a.stats().logs_removed, 1);
        assert_eq!(a.last_error(), None);
    }

    #[test]
    fn summaries_go_on_where_the_last_read_stopped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.log");
        let b = blobs();
        let doc = root();
        let snap = record(snapshot(&b.v1, &b.snap1));
        let upd = record(updates(&b.v2, &b.update));
        let read = |prev: Option<&LogSummary>| summarize_log(&path, &doc, prev).unwrap().unwrap();
        let fields = |s: &LogSummary| {
            (
                s.generation.clone(),
                s.end,
                s.records,
                s.base.clone(),
                s.vmax.clone(),
                s.complete,
            )
        };
        fs::write(&path, [header("root"), snap.clone()].concat()).unwrap();
        let first = read(None);
        assert_eq!((first.records, first.base.clone()), (1, Some(b.v1.clone())));

        // Appended to: the same as reading it all, but only the new part is read
        // (what the cached part says is kept as it is).
        append_bytes(&path, &upd);
        let next = read(Some(&first));
        assert_eq!(fields(&next), fields(&read(None)));
        assert_eq!((next.records, next.vmax.clone()), (2, b.v2.clone()));
        let mut marked = first.clone();
        marked.base = None;
        assert_eq!(read(Some(&marked)).base, None);

        // A torn tail: not complete until the record is whole.
        append_bytes(&path, &upd[..10]);
        let torn = read(Some(&next));
        assert!(!torn.complete && torn.end == next.end && torn.records == 2);
        append_bytes(&path, &upd[10..]);
        let whole = read(Some(&torn));
        assert!(whole.complete && whole.records == 3);
        assert_eq!(fields(&whole), fields(&read(None)));

        // A new generation (even a longer one), or a log shorter than what was
        // read: read from the start.
        let rewritten = [header("root"), upd.clone(), snap.clone(), upd.clone(), upd].concat();
        fs::write(&path, rewritten).unwrap();
        let fresh = read(Some(&whole));
        assert_eq!((fresh.records, fresh.base.clone()), (4, None));
        assert_eq!(fields(&fresh), fields(&read(None)));
        let mut same_generation = whole.clone();
        same_generation.generation = fresh.generation.clone();
        let header_len = next_record(&fs::read(&path).unwrap()).unwrap().1 as u64;
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(header_len)
            .unwrap();
        let cut = read(Some(&same_generation));
        assert_eq!((cut.records, cut.end, cut.complete), (0, header_len, true));
    }

    fn requests(t: &mut DirTransport) -> usize {
        drain(t)
            .iter()
            .filter(|m| matches!(m, Message::SnapshotRequest { .. }))
            .count()
    }

    #[test]
    fn a_repair_is_asked_for_until_a_snapshot_arrives() {
        let tmp = tempfile::tempdir().unwrap();
        let b = blobs();
        let mut t = transport(tmp.path(), "a", quick());
        drain(&mut t);
        t.send(updates(&b.v1, &b.snap1)).unwrap();
        let path = t.log_path(&root());
        // Deleted underneath us: the next append notices and asks for
        // everything. Its record alone is not written: it claims our whole
        // version, and a log holding only it would claim ops it lacks.
        fs::remove_file(&path).unwrap();
        t.send(updates(&b.v2, &b.update)).unwrap();
        assert_eq!(t.stats().repairs, 1);
        assert_eq!(requests(&mut t), 1);
        assert!(!path.exists());
        // That request is lost (our session failed before answering). Nothing
        // is written meanwhile, closing says the log lacks edits, and the
        // request comes again.
        t.send(updates(&b.v2, &b.update)).unwrap();
        assert!(!path.exists());
        assert!(t.close().is_err());
        let deadline = Instant::now() + Duration::from_secs(10);
        while requests(&mut t) == 0 {
            assert!(Instant::now() < deadline, "never asked again");
            std::thread::sleep(Duration::from_millis(10));
        }
        t.send(snapshot(&b.v2, &b.snap2)).unwrap();
        let s = summary_of(&path);
        assert_eq!((s.records, s.base), (1, Some(b.v2.clone())));
        assert!(t.close().is_ok());
    }

    #[test]
    fn a_lost_compaction_request_is_asked_again_once_the_log_doubled() {
        let tmp = tempfile::tempdir().unwrap();
        let b = blobs();
        let config = DirConfig {
            compact_min_bytes: 1,
            compact_ratio: 1,
            ..quick()
        };
        let mut t = transport(tmp.path(), "a", config);
        drain(&mut t);
        let path = t.log_path(&root());
        // Starts the log: nothing to compact yet. The next append asks.
        t.send(updates(&b.v2, &b.update)).unwrap();
        assert_eq!(requests(&mut t), 0);
        t.send(updates(&b.v2, &b.update)).unwrap();
        assert_eq!(requests(&mut t), 1);
        // That request is lost: not asked again on every append, but once the
        // log doubled since.
        let asked_at = fs::metadata(&path).unwrap().len();
        loop {
            t.send(updates(&b.v2, &b.update)).unwrap();
            let doubled = fs::metadata(&path).unwrap().len() > 2 * asked_at;
            assert_eq!(requests(&mut t), usize::from(doubled));
            if doubled {
                break;
            }
        }
        t.send(snapshot(&b.v2, &b.snap2)).unwrap();
        assert_eq!(summary_of(&path).records, 1);
        assert_eq!(t.stats().compactions, 1);
    }

    /// The first scan may miss our own log (a listing that failed on a network
    /// hiccup). It may hold edits nobody saved, or the only copy of a departed
    /// client's ops (§4.7): a write before the next poll reads it back first
    /// instead of replacing it.
    #[test]
    fn an_own_log_the_first_scan_missed_is_read_back_before_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let b = blobs();
        let mut t = transport(tmp.path(), "a", quick());
        t.send(updates(&b.v2, &b.update)).unwrap();
        let path = t.log_path(&root());
        drop(t);
        let before = fs::read(&path).unwrap();
        let doc_dir = path.parent().unwrap().to_owned();
        let aside = tmp.path().join("aside");
        fs::rename(&doc_dir, &aside).unwrap();
        let config = DirConfig {
            poll_interval: Duration::from_secs(3600),
            ..quick()
        };
        let mut t = transport(tmp.path(), "a", config);
        drain(&mut t);
        fs::rename(&aside, &doc_dir).unwrap();
        // Our session's snapshot lacks what the log holds: appended, and the
        // log's records reach the session.
        t.send(snapshot(&b.v1, &b.snap1)).unwrap();
        let after = fs::read(&path).unwrap();
        assert!(after.len() > before.len() && after.starts_with(&before));
        assert_eq!(t.stats().snapshots_appended, 1);
        assert_eq!(t.stats().compactions, 0);
        let read_back = drain(&mut t);
        assert!(
            read_back
                .iter()
                .any(|m| matches!(m, Message::Updates { version, .. } if *version == b.v2)),
            "{read_back:?}"
        );
        // An update is appended after it too.
        t.send(updates(&b.v2, &b.update)).unwrap();
        assert_eq!(summary_of(&path).records, 3);
        assert_eq!(t.stats().repairs, 0);
    }

    /// Running clients rewrite their presence file every few seconds; the
    /// cleanup takes it as a sign of life. A client whose own clock runs days
    /// ahead must not take those files for a day old and delete them.
    #[test]
    fn presence_files_are_collected_by_the_file_servers_clock() {
        let tmp = tempfile::tempdir().unwrap();
        let mut t = transport(tmp.path(), "a", quick());
        drain(&mut t);
        let heartbeat = tmp.path().join(PRESENCE).join("r.presence");
        fs::write(&heartbeat, b"written just now").unwrap();
        seam::set_clock_ahead(Duration::from_secs(2 * 24 * 3600));
        drain(&mut t);
        assert!(heartbeat.exists());
        // One that is a day old on the server's clock too goes.
        File::options()
            .write(true)
            .open(&heartbeat)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(25 * 3600))
            .unwrap();
        drain(&mut t);
        seam::set_clock_ahead(Duration::ZERO);
        assert!(!heartbeat.exists());
    }

    /// Two cleaners in one pass each, interleaved: b's whole pass runs while a
    /// is about to check the container it chose, so both act on summaries taken
    /// before either deleted anything. Of two departed logs holding exactly the
    /// same, only the one with the smaller ID goes (rule 4).
    #[test]
    fn two_cleaners_at_once_remove_only_one_of_two_equal_logs() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let tmp = tempfile::tempdir().unwrap();
        let b = blobs();
        let dir = tmp.path().join(UPDATES).join("root");
        fs::create_dir_all(&dir).unwrap();
        let same = [header("root"), record(snapshot(&b.v2, &b.snap2))].concat();
        let (x, y) = (dir.join("x.log"), dir.join("y.log"));
        for path in [&x, &y] {
            fs::write(path, &same).unwrap();
            File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(SystemTime::now() - Duration::from_secs(7200))
                .unwrap();
        }
        let config = DirConfig {
            inactive_after: Some(Duration::from_secs(3600)),
            cleanup_interval: Duration::ZERO,
            absorb_departed: false,
            ..quick()
        };
        let mut a = transport(tmp.path(), "a", config.clone());
        let other = Rc::new(RefCell::new(transport(tmp.path(), "b", config)));
        let mut armed = true;
        let hooked = other.clone();
        seam::set(move |point, _| {
            if point == seam::Point::ContainerCheck && std::mem::take(&mut armed) {
                drain(&mut hooked.borrow_mut());
            }
        });
        drain(&mut a);
        drain(&mut a);
        seam::clear();
        let other = other.borrow();
        assert!(!x.exists() && y.exists());
        // b deleted x in the middle of a's pass; a, which had chosen x too,
        // found it gone, and neither took y.
        assert_eq!((a.stats().logs_removed, other.stats().logs_removed), (0, 1));
        assert_eq!((a.last_error(), other.last_error()), (None, None));
    }
}
