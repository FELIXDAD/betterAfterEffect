//! End to end: the app's own hub client (`bae_sync::HubTransport`) syncing real
//! stages through a running hub.

mod common;

use std::fs;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bae_hub::{Access, Anonymous};
use bae_model::{Op, PrimType, Specifier, SublayerId, Value};
use bae_sync::{DocKey, DocSet, HubLimits, HubTransport, MemDocs, PresenceState, SyncError};
use common::*;

const PROJECT: &str = "film";

#[test]
fn clients_converge_and_the_hub_keeps_the_project() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "alice", &base),
        Client::join(addr, PROJECT, WRITE, "bob", &base),
        Client::join(addr, PROJECT, WRITE, "carol", &base),
    ];
    let mut rng = Rng::new(7);
    for round in 0..60 {
        let i = rng.below(clients.len());
        clients[i].edit(&mut rng);
        if round % 3 == 0 {
            for c in &mut clients {
                c.pump();
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pump_until(&mut clients, Duration::from_secs(20), converged);
    let expected = clients[0].fingerprint();

    // Restart the hub; a newcomer gets everything from the hub's disk alone.
    drop(clients);
    hub.shutdown();
    // The project is on disk in the shared-folder log format (written in the
    // background; shutdown waits for it).
    for doc in ["root", "anim/a"] {
        let log = data
            .path()
            .join(format!("projects/{PROJECT}/updates/{doc}/hub.log"));
        assert!(log.is_file(), "{} missing", log.display());
    }
    let hub = start(config(data.path()));
    let mut late = vec![Client::join(
        hub.local_addr(),
        PROJECT,
        WRITE,
        "dave",
        &base,
    )];
    pump_until(&mut late, Duration::from_secs(20), |c| {
        c[0].fingerprint() == expected
    });
}

#[test]
fn http_endpoints_and_tokens() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.tokens.push(token(
        "other",
        "other-token-0123456789",
        Access::Write,
        Some(&["elsewhere"]),
    ));
    let hub = start(cfg);
    let addr = hub.local_addr();

    // Public endpoints.
    assert_eq!(http_get(addr, "/healthz", None).status, 200);
    let info = http_get(addr, "/v1/info", None);
    assert_eq!(info.status, 200);
    assert!(
        info.body.contains("\"service\":\"bae-hub\""),
        "{}",
        info.body
    );
    assert_eq!(http_get(addr, "/nope", None).status, 404);
    // Everything else needs a token.
    assert_eq!(http_get(addr, "/v1/status", None).status, 401);
    assert_eq!(
        http_get(addr, "/v1/status", Some("wrong-token-000000")).status,
        401
    );
    let status = http_get(addr, "/v1/status", Some(READ));
    assert_eq!(status.status, 200);
    assert!(
        status.body.contains("\"access\":\"read\""),
        "{}",
        status.body
    );
    // The sync endpoint is WebSocket only.
    assert_eq!(http_get(addr, "/projects/film", Some(WRITE)).status, 426);

    // Refused handshakes, as the app's client reports them.
    let e = refused(addr, PROJECT, None);
    assert!(e.contains("401"), "{e}");
    let e = refused(addr, PROJECT, Some("wrong-token-000000"));
    assert!(e.contains("401"), "{e}");
    // Read-only tokens cannot create projects.
    let e = refused(addr, PROJECT, Some(READ));
    assert!(e.contains("404"), "{e}");
    // Tokens limited to other projects.
    let e = refused(addr, PROJECT, Some("other-token-0123456789"));
    assert!(e.contains("403"), "{e}");
    let e = refused(addr, "bad.name", Some(WRITE));
    assert!(e.contains("400"), "{e}");
    // `?token=` works where headers cannot be set.
    let t = bae_sync::HubTransport::connect(
        &format!("{}?token={WRITE}", url(addr, PROJECT)),
        None,
        Duration::from_secs(5),
    );
    assert!(t.is_ok(), "{t:?}");
    drop(t);

    let list = http_get(addr, "/v1/projects", Some(WRITE));
    assert_eq!(list.status, 200);
    let json: serde_json::Value = serde_json::from_str(&list.body).unwrap();
    let projects = json["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["id"], PROJECT);
    assert_eq!(projects[0]["loaded"], true);
    // A token limited to other projects does not see it.
    let list = http_get(addr, "/v1/projects", Some("other-token-0123456789"));
    assert!(list.body.contains("\"projects\":[]"), "{}", list.body);
}

#[test]
fn read_only_clients_see_but_cannot_edit() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients = vec![Client::join(addr, PROJECT, WRITE, "editor", &base)];
    let mut rng = Rng::new(3);
    for _ in 0..10 {
        clients[0].edit(&mut rng);
    }
    clients[0].pump();
    clients.push(Client::join(addr, PROJECT, READ, "viewer", &base));
    // The viewer receives the editor's work...
    pump_until(&mut clients, Duration::from_secs(20), converged);
    // ...but its own edits go nowhere.
    let before = clients[0].fingerprint();
    for _ in 0..5 {
        clients[1].edit(&mut rng);
    }
    pump_for(&mut clients, Duration::from_millis(300));
    assert_eq!(clients[0].fingerprint(), before);
    assert_ne!(clients[1].fingerprint(), before);

    let list = http_get(addr, "/v1/projects", Some(WRITE));
    let json: serde_json::Value = serde_json::from_str(&list.body).unwrap();
    let peers = json["projects"][0]["peers"].as_array().unwrap().clone();
    assert_eq!(peers.len(), 2, "{peers:?}");
    let viewer = peers.iter().find(|p| p["client"] == "viewer").unwrap();
    assert_eq!(viewer["access"], "read");
    assert_eq!(viewer["token"], "viewer");
}

#[test]
fn anonymous_access_when_enabled() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.anonymous = Anonymous::Read;
    let hub = start(cfg);
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut editor = Client::join(addr, PROJECT, WRITE, "editor", &base);
    editor.edit(&mut Rng::new(1));
    editor.pump();
    assert!(connect(addr, PROJECT, None).is_ok());
    // Read access does not create projects, even anonymously.
    let e = refused(addr, "other", None);
    assert!(e.contains("404"), "{e}");
}

#[test]
fn presence_goes_through_the_hub() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "alice", &base),
        Client::join(addr, PROJECT, READ, "bob", &base),
    ];
    clients[0].session.set_presence(PresenceState {
        user: Some(author("user:alice")),
        name: "Alice".into(),
        color: [220, 60, 60],
        editing: vec![p("/comps/main")],
        ..PresenceState::default()
    });
    // A read-only client still publishes presence.
    clients[1].session.set_presence(PresenceState {
        name: "Bob".into(),
        ..PresenceState::default()
    });
    pump_until(&mut clients, Duration::from_secs(10), |c| {
        c[1].session
            .presence()
            .iter()
            .any(|p| p.state.name == "Alice")
            && c[0]
                .session
                .presence()
                .iter()
                .any(|p| p.state.name == "Bob")
    });
    let by_others = clients[1]
        .session
        .editing_by_others(&p("/comps/main/layers/title"));
    assert_eq!(by_others.len(), 1, "advisory lock visible to bob");

    // Alice disconnects: the hub tells Bob at once.
    drop(clients.remove(0));
    pump_until(&mut clients, Duration::from_secs(10), |c| {
        c[0].session.presence().is_empty()
    });
}

#[test]
fn idle_projects_are_unloaded_and_reloaded() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.idle_unload = Some(Duration::from_millis(200));
    let hub = start(cfg);
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients = vec![Client::join(addr, PROJECT, WRITE, "alice", &base)];
    let mut rng = Rng::new(11);
    for _ in 0..10 {
        clients[0].edit(&mut rng);
    }
    let root = DocKey::sublayer(&SublayerId::root());
    let want = clients[0].stage.version(&root).unwrap();
    pump_until(&mut clients, Duration::from_secs(10), |c| {
        c[0].session.unsynced(&c[0].stage).is_empty()
    });
    let expected = clients[0].fingerprint();
    drop(clients);

    let start = Instant::now();
    loop {
        let list = http_get(addr, "/v1/projects", Some(WRITE));
        if list.body.contains("\"loaded\":false") {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "{}", list.body);
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut again = vec![Client::join(addr, PROJECT, READ, "bob", &base)];
    pump_until(&mut again, Duration::from_secs(10), |c| {
        c[0].stage
            .version(&root)
            .is_some_and(|v| v.includes_vv(&want))
    });
    assert_eq!(again[0].fingerprint(), expected);
}

/// Upload `project` through the hub and wait until a second client has all of
/// it, so the hub's relay holds every document. Both clients then disconnect.
fn upload(addr: SocketAddr, project: MemDocs) -> MemDocs {
    let mut uploader = DocsClient::join(addr, PROJECT, "uploader", project);
    let mut watcher = DocsClient::join(addr, PROJECT, "watcher", MemDocs::new());
    let start = Instant::now();
    while !watcher.has(&uploader.docs) {
        uploader.pump();
        watcher.pump();
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "the upload did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    uploader.docs
}

/// Many large documents: 12 MiB in 24 documents of 512 KiB.
const SAVE_DOCS: usize = 24;
const SAVE_DOC_LEN: usize = 512 << 10;

/// Everything the relay holds is in the project directory once the hub has
/// stopped, however much arrived just before. The directory is local disk, not
/// a slow link, so the session's backpressure (nothing new while more than
/// 1 MiB is queued) does not hold its writes back, and the last pump before
/// closing writes it all. Before, the directory got about 1.5 MiB per 250 ms
/// and the final pump wrote only what fit under 1 MiB: most of the 24
/// documents were lost.
#[test]
fn stopping_the_hub_saves_every_edit() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let project = upload(hub.local_addr(), big_docs(SAVE_DOCS, SAVE_DOC_LEN, 1));
    hub.shutdown();
    let saved = read_project_dir(&data.path().join("projects").join(PROJECT));
    let lost = missing(&project, &saved);
    assert!(
        lost.is_empty(),
        "{} of {SAVE_DOCS} documents not saved: {lost:?}",
        lost.len()
    );
}

/// The same when a project is unloaded after a while without connections (the
/// same path as a hub shutdown).
#[test]
fn unloading_an_idle_project_saves_every_edit() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.idle_unload = Some(Duration::from_millis(100));
    let hub = start(cfg);
    let addr = hub.local_addr();
    let project = upload(addr, big_docs(SAVE_DOCS, SAVE_DOC_LEN, 2));
    let start = Instant::now();
    loop {
        let list = http_get(addr, "/v1/projects", Some(WRITE));
        if list.body.contains("\"loaded\":false") {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "{}", list.body);
        std::thread::sleep(Duration::from_millis(20));
    }
    // It shows as unloaded while it is still being unloaded; stopping the hub
    // waits for that (the project is no longer there to be stopped).
    hub.shutdown();
    let saved = read_project_dir(&data.path().join("projects").join(PROJECT));
    let lost = missing(&project, &saved);
    assert!(
        lost.is_empty(),
        "{} of {SAVE_DOCS} documents not saved: {lost:?}",
        lost.len()
    );
}

/// An edit larger than tungstenite's default 16 MiB frame limit (e.g. the first
/// upload of a big sublayer) goes client → hub → client: both ends use the same
/// `HubLimits` (256 MiB by default).
#[test]
fn updates_over_16_mib_go_through_the_hub() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "alice", &base),
        Client::join(addr, PROJECT, WRITE, "bob", &base),
    ];
    pump_until(&mut clients, Duration::from_secs(10), |c| {
        c.iter().all(|c| c.session.is_connected())
    });
    let root = DocKey::sublayer(&SublayerId::root());
    let mut rng = Rng::new(3);
    // Letters from a random sequence: Loro cannot squeeze it under 16 MiB.
    let big: String = (0..20 << 20)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect();
    let alice = &mut clients[0];
    let before = alice.stage.version(&root).unwrap();
    alice.stage.set_edit_target(&SublayerId::root()).unwrap();
    let who = alice.who.clone();
    alice
        .stage
        .transact(&who, "paste", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main/layers/title"),
                attr: "notes".into(),
                value: Some(Value::String(big.clone())),
            })
        })
        .unwrap();
    let size = alice.stage.export_updates(&root, &before).unwrap().len();
    assert!(size > 16 << 20, "the update is only {size} bytes");
    let want = alice.stage.version(&root).unwrap();
    // `Client::pump` panics if the hub drops the link (as it did with a 16 MiB
    // frame limit).
    pump_until(&mut clients, Duration::from_secs(30), |c| {
        c[1].stage
            .version(&root)
            .is_some_and(|v| v.includes_vv(&want))
    });
    let got = clients[1]
        .stage
        .prim(&p("/comps/main/layers/title"))
        .and_then(|t| t.value("notes").cloned());
    assert!(got == Some(Value::String(big)), "bob got a different value");
}

/// `max_message_mib` in `hub.toml` (`HubConfig::limits`) is what the hub's
/// WebSocket handshake uses: with 1 MiB, a client on the default limits that
/// sends a 2 MiB edit is dropped and the edit goes nowhere, while small edits
/// still go through. A client configured with the same limits refuses the edit
/// itself and stays connected.
#[test]
fn max_message_mib_reaches_the_handshake() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.limits = HubLimits::with_max_message_size(1 << 20);
    cfg.validate().unwrap();
    let hub = start(cfg);
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "alice", &base),
        Client::join(addr, PROJECT, WRITE, "bob", &base),
    ];
    let root = DocKey::sublayer(&SublayerId::root());
    // A small edit goes through.
    let small = paste(&mut clients[0], "small", 1000, 1);
    pump_until(&mut clients, Duration::from_secs(10), |c| {
        c[1].stage
            .version(&root)
            .is_some_and(|v| v.includes_vv(&small))
    });
    // A 2 MiB one is over the hub's limit: the hub drops the sender.
    let big = paste(&mut clients[0], "big", 2 << 20, 2);
    let (alice, others) = clients.split_at_mut(1);
    let e = pump_until_dropped(&mut alice[0], others, Duration::from_secs(10));
    eprintln!("alice: {e:?}");
    assert!(e.is_some(), "the hub accepted a message over its limit");
    pump_for(others, Duration::from_millis(300));
    let bob = others[0].stage.version(&root).unwrap();
    assert!(!bob.includes_vv(&big), "bob got the edit over the limit");

    // A client with the hub's limits does not send it at all.
    let link = HubTransport::connect_with(
        &url(addr, PROJECT),
        Some(WRITE),
        Duration::from_secs(5),
        HubLimits::with_max_message_size(1 << 20),
    )
    .unwrap();
    let mut carol = Client::join(addr, PROJECT, WRITE, "carol", &base);
    carol.link = bae_sync::ThreadedTransport::spawn(link, Duration::from_millis(1), || {});
    paste(&mut carol, "big", 2 << 20, 3);
    let start = Instant::now();
    let refused = loop {
        let out = carol.pump();
        if let Some(e) = out.errors.into_iter().next() {
            break e;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "carol sent it");
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(
        matches!(refused, SyncError::MessageTooLarge { max, .. } if max == 1 << 20),
        "{refused:?}"
    );
    pump_for(std::slice::from_mut(&mut carol), Duration::from_millis(200));
}

/// A large upload on a slow uplink takes longer than the hub's peer timeout,
/// and the client's hellos wait behind it: the hub counts the bytes that keep
/// arriving as signs of life and does not drop the client. (It used to count
/// only whole messages.)
///
/// The test thread does not pump either client while it makes the edit, while
/// the session exports it, or while the other client imports it (hundreds of
/// milliseconds on a loaded machine): the timeout leaves room for that, the
/// text is made before anyone connects, and the uplink is slow enough that the
/// upload still lasts several timeouts.
#[test]
fn a_slow_upload_is_not_taken_for_a_dead_link() {
    let timeout = Duration::from_secs(2);
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.peer_timeout = timeout;
    let hub = start(cfg);
    let addr = hub.local_addr();
    // 1 MB/s up: a 6 MiB edit takes about 6.3 s, three timeouts.
    let slow = throttled_proxy(addr, 1_000_000);
    let text = letters(6 << 20, 4);
    let base = base_snapshots();
    let mut clients = vec![
        Client::join(slow, PROJECT, WRITE, "alice", &base),
        Client::join(addr, PROJECT, WRITE, "bob", &base),
    ];
    for c in &mut clients {
        // Idle clients stay well within the timeout.
        c.session.config_mut().hello_interval = Some(Duration::from_millis(100));
    }
    pump_until(&mut clients, Duration::from_secs(10), |c| {
        c.iter().all(|c| c.session.is_connected())
    });
    let t = Instant::now();
    let want = set_text(&mut clients[0], "notes", text);
    let edit = t.elapsed();
    let root = DocKey::sublayer(&SublayerId::root());
    // `Client::pump` panics if the hub drops either client.
    let start = Instant::now();
    let (mut last, mut longest) = (start, Duration::ZERO);
    loop {
        for c in &mut clients {
            c.pump();
        }
        let now = Instant::now();
        longest = longest.max(now - last);
        last = now;
        if clients[1]
            .stage
            .version(&root)
            .is_some_and(|v| v.includes_vv(&want))
        {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "not delivered");
        std::thread::sleep(Duration::from_millis(1));
    }
    let took = start.elapsed();
    eprintln!(
        "upload took {took:?}; the edit took {edit:?}, the longest round of pumps {longest:?}"
    );
    assert!(took > timeout, "the upload was not slow: {took:?}");
}

/// Read-only connections may not send messages over 16 MiB: their edits are
/// dropped anyway, so the hub does not buffer large ones. A larger message
/// drops the connection; small edits are still only ignored.
#[test]
fn read_only_clients_cannot_send_large_messages() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut others = vec![Client::join(addr, PROJECT, WRITE, "editor", &base)];
    let mut viewer = Client::join(addr, PROJECT, READ, "viewer", &base);
    paste(&mut viewer, "small", 1000, 5);
    assert!(pump_until_dropped(&mut viewer, &mut others, Duration::from_millis(300)).is_none());
    paste(&mut viewer, "big", 17 << 20, 6);
    let e = pump_until_dropped(&mut viewer, &mut others, Duration::from_secs(20));
    assert!(
        e.is_some(),
        "the hub took a 17 MiB message from a read-only client"
    );
}

/// Acceptance (task book 10): ten clients, LAN latency < 100 ms. Here on
/// loopback: one client changes a value, measure until all nine others have it.
/// Clients poll their link every 1 ms (`ThreadedTransport`), as the app does.
#[test]
fn ten_clients_latency() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients: Vec<Client> = (0..10)
        .map(|i| Client::join(addr, PROJECT, WRITE, &format!("c{i}"), &base))
        .collect();
    pump_until(&mut clients, Duration::from_secs(20), |c| {
        c.iter().all(|c| c.session.is_connected())
    });
    let root = DocKey::sublayer(&SublayerId::root());
    let mut samples = Vec::new();
    for round in 0..40 {
        // Let the hub go quiet now and then, so some samples include its
        // back-off from 1 ms to 10 ms polling.
        if round % 4 == 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
        let c = &mut clients[round % 10];
        c.stage.set_edit_target(&SublayerId::root()).unwrap();
        let who = c.who.clone();
        c.stage
            .transact(&who, "edit", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main/layers/title"),
                    attr: "opacity".into(),
                    value: Some(Value::Float(round as f64)),
                })
            })
            .unwrap();
        let want = c.stage.version(&root).unwrap();
        let took = pump_until(&mut clients, Duration::from_secs(10), |cs| {
            cs.iter()
                .all(|c| c.stage.version(&root).is_some_and(|v| v.includes_vv(&want)))
        });
        samples.push(took);
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    let max = *samples.last().unwrap();
    println!("10 clients, edit → all others: median {median:?}, max {max:?}");
    assert!(median < Duration::from_millis(100), "median {median:?}");
}

#[test]
fn one_hub_per_data_directory() {
    let data = tempfile::tempdir().unwrap();
    let _hub = start(config(data.path()));
    let second = bae_hub::start(config(data.path()));
    assert!(
        matches!(second, Err(bae_hub::HubError::DataDirInUse(_))),
        "{second:?}"
    );
}

/// One record of the shared-folder log format (`docs/spec/sync.md` §4.2).
fn record(payload: &[u8]) -> Vec<u8> {
    let mut out = (payload.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&blake3::hash(payload).as_bytes()[..8]);
    out.extend_from_slice(payload);
    out
}

/// Create the layer `name` in sublayer `id` of `c`'s stage.
fn create_layer(c: &mut Client, id: &SublayerId, name: &str) {
    c.stage.set_edit_target(id).unwrap();
    let who = c.who.clone();
    c.stage
        .transact(&who, "create", |tx| {
            tx.apply(Op::CreatePrim {
                path: p(&format!("{LAYERS}/{name}")),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::new("Layer")),
                index: None,
            })
        })
        .unwrap();
}

/// The hub's own log of one document was written by a newer version of the
/// log format (the hub was downgraded after a newer build ran): only that
/// document's edits are not saved, and its log is left as it is. The project
/// directory stays attached (`saving` in `/v1/projects`) and the other
/// document is saved, up to the last edit before the hub stops. Before, every
/// write to that document failed the whole directory link: the hub detached
/// it ("project directory failed"), reopened it 5 s later, read everything
/// again and failed again; edits made while it was detached were not saved.
#[test]
fn a_newer_log_of_one_document_does_not_stop_saving_the_others() {
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("projects").join(PROJECT);
    let log = dir.join("updates/root/hub.log");
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let header =
        br#"{"format":"bae.sync.log","version":99,"doc":"root","client":"hub","generation":"g"}"#;
    let mut bytes = record(header);
    bytes.extend(record(b"a record of a newer kind"));
    fs::write(&log, &bytes).unwrap();

    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    // Bob only watches: once he has an edit, the hub has it.
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "alice", &base),
        Client::join(addr, PROJECT, READ, "bob", &base),
    ];
    let saving = || {
        let list = http_get(addr, "/v1/projects", Some(WRITE));
        let json: serde_json::Value = serde_json::from_str(&list.body).unwrap();
        json["projects"][0]["saving"] == true
    };
    let root = DocKey::sublayer(&SublayerId::root());
    let anim = DocKey::sublayer(&sl("anim/a"));
    for name in ["one", "two"] {
        for id in [SublayerId::root(), sl("anim/a")] {
            create_layer(&mut clients[0], &id, name);
        }
        let want = clients[0].stage.version(&anim).unwrap();
        pump_until(&mut clients, Duration::from_secs(30), |c| {
            c[1].stage
                .version(&anim)
                .is_some_and(|v| v.includes_vv(&want))
        });
        // The hub writes to the directory every 250 ms: give it time to try.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(1) {
            assert!(saving(), "the project directory was detached");
            pump_for(&mut clients, Duration::from_millis(50));
        }
    }
    let alice = &clients[0].stage;
    let (want_anim, want_root) = (alice.version(&anim).unwrap(), alice.version(&root).unwrap());
    drop(clients);
    hub.shutdown();
    assert_eq!(fs::read(&log).unwrap(), bytes, "the newer log was changed");
    let saved = read_project_dir(&dir);
    assert!(
        saved
            .version(&anim)
            .is_some_and(|v| v.includes_vv(&want_anim)),
        "the other document was not saved"
    );
    assert!(
        !saved
            .version(&root)
            .is_some_and(|v| v.includes_vv(&want_root))
    );
}
