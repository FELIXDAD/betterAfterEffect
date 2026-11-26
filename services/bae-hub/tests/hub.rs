//! End to end: the app's own hub client (`bae_sync::HubTransport`) syncing real
//! stages through a running hub.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use bae_hub::{Access, Anonymous};
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

/// Records in one of the hub's logs (spec 4.2), not counting the header.
fn log_records(path: &Path) -> usize {
    let bytes = std::fs::read(path).unwrap();
    let (mut at, mut n) = (0, 0);
    while at + 12 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        at += 12 + len;
        n += 1;
    }
    assert_eq!(at, bytes.len(), "torn record in {}", path.display());
    n - 1
}

/// The hub writes the project directory at most every few hundred milliseconds,
/// so a drag becomes a few appends; shutting down right after the drag still
/// writes its last steps (they were held back by that interval).
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
    let records = log_records(
        &data
            .path()
            .join(format!("projects/{PROJECT}/updates/root/hub.log")),
    );
    println!("drag of {STEPS} steps: {records} records in the hub's log");
    assert!(
        records < STEPS / 4,
        "{records} log records for a drag of {STEPS} steps"
    );

    let hub = start(config(data.path()));
    let root = DocKey::sublayer(&SublayerId::root());
    let mut late = vec![Client::join(
        hub.local_addr(),
        PROJECT,
        WRITE,
        "dave",
        &base,
    )];
    pump_until(&mut late, Duration::from_secs(20), |c| {
        c[0].stage
            .version(&root)
            .is_some_and(|v| v.includes_vv(&want))
    });
    assert_eq!(late[0].fingerprint(), expected);
}

/// Unloading an idle project right after a drag still writes its last steps.
#[test]
fn a_drag_is_kept_when_the_project_is_unloaded() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.idle_unload = Some(Duration::from_millis(100));
    let hub = start(cfg);
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut clients = vec![
        Client::join(addr, PROJECT, WRITE, "alice", &base),
        Client::join(addr, PROJECT, WRITE, "bob", &base),
    ];
    pump_until(&mut clients, Duration::from_secs(10), |c| {
        c.iter().all(|c| c.session.is_connected())
    });
    let want = drag(&mut clients, 50, Duration::from_millis(3));
    let expected = clients[0].fingerprint();
    drop(clients);

    let start = Instant::now();
    loop {
        let list = http_get(addr, "/v1/projects", Some(WRITE));
        if list.body.contains("\"loaded\":false") {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "{}", list.body);
        std::thread::sleep(Duration::from_millis(10));
    }
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
