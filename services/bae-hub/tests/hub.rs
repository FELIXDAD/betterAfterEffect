//! End to end: the app's own hub client (`bae_sync::HubTransport`) syncing real
//! stages through a running hub.

mod common;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bae_hub::{Access, Anonymous, HubConfig};
use bae_model::loro::VersionVector;
use bae_model::{Op, SublayerId, Value};
use bae_sync::{DocKey, DocSet, PresenceState};
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
    // What each client said it takes (`x-bae-max-incoming`), which the
    // hub's warnings about layers that do not fit go by.
    for peer in &peers {
        assert_eq!(
            peer["max_incoming"],
            bae_sync::HubLimits::default().max_incoming as u64,
            "{peer}"
        );
    }
}

/// A note of `len` random letters in `/comps/main` of `layer` (Loro does
/// not get them much smaller).
fn note(client: &mut Client, layer: &SublayerId, attr: &str, len: usize, rng: &mut Rng) {
    let text: String = (0..len)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect();
    client.stage.set_edit_target(layer).unwrap();
    client
        .stage
        .transact(&client.who, "note", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main"),
                attr: attr.into(),
                value: Some(Value::String(text)),
            })
        })
        .unwrap();
}

/// Whether `to` has everything `from` has of `layer`.
fn has_all(from: &Client, to: &Client, layer: &SublayerId) -> bool {
    let doc = DocKey::sublayer(layer);
    let want = DocSet::version(&from.stage, &doc).unwrap();
    DocSet::version(&to.stage, &doc).is_some_and(|v| v.includes_vv(&want))
}

/// The hub's sessions skip a document that does not fit in one message for
/// a client, as the clients' sessions do (spec 2.4, 3.1). Here `root` grew to
/// more than the hub's `max_message` in smaller edits, and a newcomer lacks
/// all of them: the hub skips `root` for it and goes on with `anim/a`, and
/// the newcomer keeps its link (it reports it has not caught up). The relay
/// used to drop the newcomer on the failed send; it reconnected and was
/// dropped again, forever, and got no other document either.
#[test]
fn a_document_too_large_for_one_client_does_not_drop_its_link() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.limits.max_message = 256 << 10;
    cfg.limits.max_incoming = 256 << 10;
    let hub = start(cfg);
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut rng = Rng::new(11);
    let root = SublayerId::root();
    let anim = sl("anim/a");
    // The editor writes four notes of 100 KiB, each in its own message; a
    // second client gets each as it comes (so the hub has them all).
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "editor", &base),
        Client::join(addr, PROJECT, WRITE, "early", &base),
    ];
    for i in 0..4 {
        note(
            &mut clients[0],
            &root,
            &format!("note{i}"),
            100 << 10,
            &mut rng,
        );
        pump_until(&mut clients, Duration::from_secs(20), |c| {
            has_all(&c[0], &c[1], &root)
        });
    }
    clients.push(Client::join(addr, PROJECT, WRITE, "late", &base));
    // The editor keeps editing both documents; `anim/a` reaches the
    // newcomer, `root` does not, and nobody is dropped (`Client::pump`
    // fails the test on a link error).
    for _ in 0..5 {
        note(&mut clients[0], &anim, "small", 100, &mut rng);
        note(&mut clients[0], &root, "small", 100, &mut rng);
        pump_until(&mut clients, Duration::from_secs(20), |c| {
            has_all(&c[0], &c[2], &anim) && has_all(&c[0], &c[1], &root)
        });
    }
    pump_for(&mut clients, Duration::from_millis(100));
    let late = &clients[2];
    assert!(!has_all(&clients[0], late, &root));
    // Only the hub knows why: the newcomer's session lists the layer as one
    // the hub has more of (and is never caught up on this link).
    assert!(!late.session.is_caught_up(&late.stage));
    assert_eq!(late.session.behind(&late.stage), [DocKey::sublayer(&root)]);
    assert!(late.session.stats().bytes_received > 0);
    let list = http_get(addr, "/v1/projects", Some(WRITE));
    let json: serde_json::Value = serde_json::from_str(&list.body).unwrap();
    let peers = json["projects"][0]["peers"].as_array().unwrap();
    assert_eq!(peers.len(), 3, "{peers:?}");
}

/// A WebSocket handshake for the test project written by hand, as a client
/// of another or an older build would: the socket, and the response head.
fn raw_websocket(addr: SocketAddr, token: &str) -> (std::net::TcpStream, String) {
    use std::io::{Read, Write};
    let mut tcp = std::net::TcpStream::connect(addr).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(
        tcp,
        "GET {}{PROJECT} HTTP/1.1\r\nHost: hub\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\nAuthorization: Bearer {token}\r\n\r\n",
        bae_hub::SYNC_PREFIX
    )
    .unwrap();
    let mut response = Vec::new();
    while !response.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        tcp.read_exact(&mut byte).unwrap();
        response.push(byte[0]);
    }
    let response = String::from_utf8_lossy(&response).into_owned();
    assert!(response.starts_with("HTTP/1.1 101"), "{response}");
    (tcp, response)
}

/// The hub takes at most `read_only_max_incoming` from read-only connections:
/// a read-only token cannot make it take in messages as large as writers may
/// send (it used to take 256 MiB from anyone and drop the edits only once
/// decoded). They are still sent large ones. The handshake tells the client
/// the limit, so this build's client does not send larger ones; one that
/// does not know it (another or an older build) is dropped.
#[test]
fn read_only_connections_cannot_send_large_messages() {
    use bae_sync::{Message, SyncError, Transport};
    use std::io::{Read, Write};
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.read_only_max_incoming = 256 << 10;
    let hub = start(cfg);
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "editor", &base),
        Client::join(addr, PROJECT, READ, "viewer", &base),
    ];
    // A writer may send a message of 1 MiB, and the viewer gets it.
    let mut rng = Rng::new(5);
    let big: String = (0..1 << 20)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect();
    let editor = &mut clients[0];
    editor.stage.set_edit_target(&SublayerId::root()).unwrap();
    editor
        .stage
        .transact(&editor.who, "note", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main"),
                attr: "note".into(),
                value: Some(Value::String(big)),
            })
        })
        .unwrap();
    pump_until(&mut clients, Duration::from_secs(20), converged);
    let viewer = &mut clients[1];
    assert!(viewer.session.stats().bytes_received >= 1 << 20);

    // The viewer's client knows its limit from the handshake: it refuses a
    // larger message itself, and the link stays.
    assert_eq!(viewer.link.max_message_size(), Some(256 << 10));
    let refused = viewer.link.send(Message::Updates {
        doc: DocKey::sublayer(&SublayerId::root()),
        version: Default::default(),
        data: vec![0; 512 << 10],
    });
    assert!(
        matches!(refused, Err(SyncError::MessageTooLarge { max, .. }) if max == 256 << 10),
        "{refused:?}"
    );
    pump_for(&mut clients, Duration::from_millis(100));
    // A client that does not know sends one anyway: the hub drops the
    // connection as soon as the frame header arrives.
    let (mut tcp, response) = raw_websocket(addr, READ);
    assert!(
        response
            .to_ascii_lowercase()
            .contains(&format!("x-bae-max-incoming: {}", 256 << 10)),
        "{response}"
    );
    let len: u64 = 512 << 10;
    let mut header = vec![0x82, 0x80 | 127];
    header.extend_from_slice(&len.to_be_bytes());
    header.extend_from_slice(&[0; 4]);
    tcp.write_all(&header).unwrap();
    let _ = tcp.write_all(&[0; 4096]);
    let start = Instant::now();
    let mut buf = vec![0; 1 << 16];
    loop {
        match tcp.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => assert!(
                start.elapsed() < Duration::from_secs(10),
                "the hub took a large message from a read-only connection"
            ),
        }
    }
    assert!(start.elapsed() < Duration::from_secs(10));
    // Writers are not affected.
    let mut editor = clients.remove(0);
    for _ in 0..3 {
        editor.edit(&mut rng);
    }
    editor.pump();
    let mut late = vec![editor, Client::join(addr, PROJECT, WRITE, "late", &base)];
    pump_until(&mut late, Duration::from_secs(20), converged);
}

/// Viewers edit locally; their edits just go nowhere. A viewer whose session
/// does not know it is read-only sends them after every `Hello` of the hub and
/// every reconnect, so with a 1 MiB limit for read-only connections one with
/// more than that in one document was dropped every time, forever. The default
/// limit leaves room for that (and for `Hello`s, which grow with documents ×
/// Loro peers), and a viewer whose session sends no document data
/// (`SessionConfig::send_data`, as the app sets it for a read-only token) is
/// never dropped for its edits, however small the limit.
#[test]
fn viewers_with_large_local_edits_stay_connected() {
    for (limit, send_data) in [(None, true), (Some(64 << 10), false)] {
        let data = tempfile::tempdir().unwrap();
        let mut cfg = config(data.path());
        if let Some(limit) = limit {
            cfg.read_only_max_incoming = limit;
        }
        let hub = start(cfg);
        let addr = hub.local_addr();
        let base = base_snapshots();
        let mut clients = vec![
            Client::join(addr, PROJECT, WRITE, "editor", &base),
            Client::join(addr, PROJECT, READ, "viewer", &base),
        ];
        clients[1].session.config_mut().send_data = send_data;
        pump_until(&mut clients, Duration::from_secs(20), converged);
        let mut rng = Rng::new(9);
        let big: String = (0..3 << 19)
            .map(|_| char::from(b'a' + rng.below(26) as u8))
            .collect();
        let viewer = &mut clients[1];
        viewer.stage.set_edit_target(&SublayerId::root()).unwrap();
        viewer
            .stage
            .transact(&viewer.who, "note", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main"),
                    attr: "note".into(),
                    value: Some(Value::String(big)),
                })
            })
            .unwrap();
        let imports = viewer.session.stats().imports;
        // The editor goes on editing and the viewer keeps up; a dropped link
        // fails its pump.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            clients[0].edit(&mut rng);
            pump_for(&mut clients, Duration::from_millis(50));
        }
        let viewer = &clients[1];
        assert!(viewer.session.stats().imports > imports + 10);
        if send_data {
            assert!(viewer.session.stats().bytes_sent >= 3 << 19);
        } else {
            assert_eq!(viewer.session.stats().bytes_sent, 0);
        }
        let list = http_get(addr, "/v1/projects", Some(WRITE));
        let json: serde_json::Value = serde_json::from_str(&list.body).unwrap();
        let peers = json["projects"][0]["peers"].as_array().unwrap();
        assert_eq!(peers.len(), 2, "{peers:?}");
    }
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

/// A drag by `clients[0]`: `steps` changes of one value, each sent at once,
/// about `every` apart. Returns the root version after the last step, once
/// `clients[1]` has it (so the hub has every step in memory).
fn drag(clients: &mut [Client], steps: usize, every: Duration) -> VersionVector {
    let root = DocKey::sublayer(&SublayerId::root());
    for i in 0..steps {
        let c = &mut clients[0];
        c.stage.set_edit_target(&SublayerId::root()).unwrap();
        let who = c.who.clone();
        c.stage
            .transact(&who, "drag", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main/layers/title"),
                    attr: "opacity".into(),
                    value: Some(Value::Float(i as f64)),
                })
            })
            .unwrap();
        for c in clients.iter_mut() {
            c.pump();
        }
        std::thread::sleep(every);
    }
    let want = clients[0].stage.version(&root).unwrap();
    pump_until(clients, Duration::from_secs(10), |c| {
        c[1].stage
            .version(&root)
            .is_some_and(|v| v.includes_vv(&want))
    });
    want
}

/// Complete records in one of the hub's logs (spec 4.2), not counting the
/// header, and how many bytes they take.
fn records_in(bytes: &[u8]) -> (usize, usize) {
    let (mut at, mut n) = (0, 0usize);
    while at + 12 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        if at + 12 + len > bytes.len() {
            break;
        }
        at += 12 + len;
        n += 1;
    }
    (n.saturating_sub(1), at)
}

/// Records in a hub log the hub is done with.
fn log_records(path: &Path) -> usize {
    let bytes = std::fs::read(path).unwrap();
    let (n, at) = records_in(&bytes);
    assert_eq!(at, bytes.len(), "torn record in {}", path.display());
    n
}

/// Records in a hub log the hub may be writing to (0 before it exists).
fn records_now(path: &Path) -> usize {
    std::fs::read(path).map_or(0, |bytes| records_in(&bytes).0)
}

fn root_log(data: &Path) -> PathBuf {
    data.join(format!("projects/{PROJECT}/updates/root/hub.log"))
}

/// Writes to the project directory are held back for a minute: whatever is on
/// disk afterwards was written by one of the flushes.
fn slow_store(data: &Path) -> HubConfig {
    let mut cfg = config(data);
    cfg.store_send_interval = Duration::from_secs(60);
    cfg
}

/// Two writers on `PROJECT`, once the store's first write is on disk (it is
/// never held back). Returns the clients and how many records the root log has
/// then: 0 if that write only had `anim/a` (the base project arrives as one
/// message per sublayer, and the relay may read them in different pumps); the
/// root's part then waits with the rest for a flush, which writes it as one
/// record.
fn two_writers(
    addr: SocketAddr,
    data: &Path,
    base: &[(SublayerId, Vec<u8>)],
) -> (Vec<Client>, usize) {
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "alice", base),
        Client::join(addr, PROJECT, WRITE, "bob", base),
    ];
    let log = root_log(data);
    let anim_log = data.join(format!("projects/{PROJECT}/updates/anim/a/hub.log"));
    pump_until(&mut clients, Duration::from_secs(10), |c| {
        c.iter().all(|c| c.session.is_connected()) && records_now(&log) + records_now(&anim_log) > 0
    });
    // The store's thread writes one document after the other: wait until the
    // rest of that write (if any) is on disk too. Each new log is written to a
    // temporary file that is synced before it is renamed into place, and
    // `File::sync_all` is a full device flush on macOS (`F_FULLFSYNC`) and
    // Windows (`FlushFileBuffers`): tens of milliseconds or more on a busy CI
    // machine. So do not assume it lands within a few milliseconds: wait while a
    // temporary file is there, and until the root log stayed the same for half a
    // second (nothing else reaches the disk for a minute, see `slow_store`).
    let root_dir = log.parent().expect("a directory").to_owned();
    let writing = || {
        std::fs::read_dir(&root_dir).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        })
    };
    let start = Instant::now();
    let mut records = records_now(&log);
    loop {
        pump_for(&mut clients, Duration::from_millis(500));
        let now = records_now(&log);
        if now == records && !writing() {
            return (clients, records);
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the store's first write did not finish"
        );
        records = now;
    }
}

/// Wait until the project is (`true`) or is no longer (`false`) loaded.
fn wait_loaded(addr: SocketAddr, loaded: bool) {
    let want = format!("\"loaded\":{loaded}");
    let start = Instant::now();
    loop {
        let list = http_get(addr, "/v1/projects", Some(WRITE));
        if list.body.contains(&want) {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "{}", list.body);
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A newcomer on a hub over `data` gets `want` of the root sublayer, all from
/// the hub's disk; returns its fingerprint.
fn reload(data: &Path, base: &[(SublayerId, Vec<u8>)], want: &VersionVector) -> String {
    let hub = start(config(data));
    let root = DocKey::sublayer(&SublayerId::root());
    let mut late = vec![Client::join(hub.local_addr(), PROJECT, READ, "dave", base)];
    pump_until(&mut late, Duration::from_secs(20), |c| {
        c[0].stage
            .version(&root)
            .is_some_and(|v| v.includes_vv(want))
    });
    late[0].fingerprint()
}

/// The hub writes the project directory at most every few hundred milliseconds,
/// so a drag becomes a few appends; shutting down right after the drag still
/// writes its last steps.
#[test]
fn a_drag_is_written_in_batches_and_kept_on_shutdown() {
    const STEPS: usize = 100;
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
    let want = drag(&mut clients, STEPS, Duration::from_millis(3));
    let expected = clients[0].fingerprint();

    hub.shutdown();
    drop(clients);
    let records = log_records(&root_log(data.path()));
    println!("drag of {STEPS} steps: {records} records in the hub's log");
    assert!(
        records < STEPS / 4,
        "{records} log records for a drag of {STEPS} steps"
    );
    assert_eq!(reload(data.path(), &base, &want), expected);
}

/// Shutting the hub down while clients are still connected writes what the
/// store's interval held back (`Relay::flush`, then waiting for the store's
/// thread).
#[test]
fn held_back_writes_are_flushed_on_shutdown() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(slow_store(data.path()));
    let base = base_snapshots();
    let (mut clients, before) = two_writers(hub.local_addr(), data.path(), &base);
    let want = drag(&mut clients, 20, Duration::from_millis(3));
    let expected = clients[0].fingerprint();
    let log = root_log(data.path());
    assert_eq!(records_now(&log), before, "the drag is held back");

    hub.shutdown();
    assert_eq!(
        log_records(&log),
        before + 1,
        "the whole drag in one append"
    );
    drop(clients);
    assert_eq!(reload(data.path(), &base, &want), expected);
}

/// Once the last client is gone there is nothing to batch: what the store's
/// interval held back is written at once, not when the next write is due (here
/// a minute later) or when the project is unloaded.
#[test]
fn held_back_writes_are_flushed_when_the_last_client_leaves() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = slow_store(data.path());
    cfg.idle_unload = None;
    let hub = start(cfg);
    let addr = hub.local_addr();
    let base = base_snapshots();
    let (mut clients, before) = two_writers(addr, data.path(), &base);
    let want = drag(&mut clients, 20, Duration::from_millis(3));
    let expected = clients[0].fingerprint();
    let log = root_log(data.path());
    assert_eq!(records_now(&log), before, "the drag is held back");

    drop(clients);
    let start = Instant::now();
    while records_now(&log) == before {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the drag was not written after the last client left"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    wait_loaded(addr, true);
    hub.shutdown();
    assert_eq!(log_records(&log), before + 1);
    assert_eq!(reload(data.path(), &base, &want), expected);
}

/// Unloading an idle project right after a drag keeps its last steps (written
/// when the last client left; the flush before the relay closes would too).
#[test]
fn a_drag_is_kept_when_the_project_is_unloaded() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = slow_store(data.path());
    cfg.idle_unload = Some(Duration::from_millis(100));
    let hub = start(cfg);
    let addr = hub.local_addr();
    let base = base_snapshots();
    let (mut clients, before) = two_writers(addr, data.path(), &base);
    let want = drag(&mut clients, 50, Duration::from_millis(3));
    let expected = clients[0].fingerprint();
    assert_eq!(
        records_now(&root_log(data.path())),
        before,
        "the drag is held back"
    );
    drop(clients);

    wait_loaded(addr, false);
    let root = DocKey::sublayer(&SublayerId::root());
    let mut again = vec![Client::join(addr, PROJECT, READ, "carol", &base)];
    pump_until(&mut again, Duration::from_secs(10), |c| {
        c[0].stage
            .version(&root)
            .is_some_and(|v| v.includes_vv(&want))
    });
    assert_eq!(again[0].fingerprint(), expected);
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

/// The hub's peer timeout counts bytes, not whole messages: a client whose
/// one large message takes longer than `peer_timeout` to upload (a first
/// share over a slow VPN) is not cut off while its bytes keep arriving, or
/// it would send the same message again after every reconnect. A client
/// that stops sending is still dropped.
#[test]
fn a_slow_upload_of_one_large_message_is_not_cut_off() {
    use std::io::{Read, Write};

    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.peer_timeout = Duration::from_millis(500);
    let hub = start(cfg);
    let (mut tcp, _) = raw_websocket(hub.local_addr(), WRITE);

    // One masked binary frame of 1 MiB, sent at about 40 KB/s for 2 s (four
    // peer timeouts); the hub's own messages are not read meanwhile.
    let len: u64 = 1 << 20;
    let mut header = vec![0x82, 0x80 | 127];
    header.extend_from_slice(&len.to_be_bytes());
    header.extend_from_slice(&[0; 4]); // mask key 0: the payload as it is
    tcp.write_all(&header).unwrap();
    let chunk = [0u8; 2000];
    let start = Instant::now();
    let mut sent = 0;
    while start.elapsed() < Duration::from_secs(2) {
        if let Err(e) = tcp.write_all(&chunk) {
            panic!("cut off after {:?}, {sent} bytes: {e}", start.elapsed());
        }
        sent += chunk.len();
        std::thread::sleep(Duration::from_millis(50));
    }
    // Silent from now on: dropped after the timeout (end of stream or reset).
    let silent = Instant::now();
    let mut buf = vec![0; 1 << 16];
    loop {
        match tcp.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => assert!(
                silent.elapsed() < Duration::from_secs(10),
                "the hub kept a silent client"
            ),
        }
    }
    assert!(silent.elapsed() < Duration::from_secs(10));
}
