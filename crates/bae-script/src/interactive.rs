//! A suspended JSX file owns its QuickJS thread, never a stage transaction.
//! Each user event resumes it against the transaction of that event.
use crate::host::{self, RunState};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

type Reply = Result<Value, String>;

pub(crate) enum Request {
    Host(String, Value, mpsc::SyncSender<(Reply, u32)>),
    Display(Value),
    Done(Reply),
}

pub(crate) struct Endpoint {
    pub requests: mpsc::SyncSender<Request>,
    pub events: mpsc::Receiver<Value>,
}

struct Worker {
    events: mpsc::SyncSender<Value>,
    requests: mpsc::Receiver<Request>,
}

#[derive(Clone, Debug)]
pub struct ScriptWindow {
    pub id: u64,
    pub panel: Option<String>,
    pub tree: Arc<Value>,
}

#[derive(Default)]
struct Sessions {
    next: u64,
    programs: BTreeMap<String, Arc<crate::PanelProgram>>,
    pending: BTreeMap<u64, (Worker, ScriptWindow)>,
}

#[derive(Default, Clone)]
pub struct ScriptWindows(Arc<Mutex<Sessions>>);

impl ScriptWindows {
    pub fn register_panel(&self, id: String, program: Arc<crate::PanelProgram>) {
        self.0.lock().unwrap().programs.insert(id, program);
    }

    /// Compile installed panel files on the application's warm-up thread.
    pub fn prewarm_panels(&self) {
        let programs: Vec<_> = self.0.lock().unwrap().programs.values().cloned().collect();
        for program in programs {
            program.prepare();
        }
    }

    pub fn windows(&self) -> Vec<ScriptWindow> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending
            .values()
            .map(|(_, window)| window.clone())
            .collect()
    }

    pub fn dismiss(&self, id: u64) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending
            .remove(&id);
    }

    pub(crate) fn start(
        &self,
        state: &mut RunState<'_, '_, '_>,
        source: String,
        panel: Option<String>,
    ) -> Reply {
        let (id, source) = {
            let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if s.pending.len() >= 32 {
                return Err("close a script window before opening another".into());
            }
            s.next += 1;
            let source = match &panel {
                Some(id) => crate::panel_program::InteractiveSource::Panel(
                    s.programs.get(id).ok_or("unknown ScriptUI panel")?.clone(),
                ),
                None => crate::panel_program::InteractiveSource::Text(source),
            };
            (s.next, source)
        };
        let (requests, receive) = mpsc::sync_channel(0);
        let (send, events) = mpsc::sync_channel(1);
        let limit = state.env.time_limit.unwrap_or(crate::TIME_LIMIT);
        std::thread::Builder::new()
            .name("bae-jsx".into())
            .spawn(move || {
                let result = crate::engine::interactive_file(
                    source,
                    Endpoint {
                        requests: requests.clone(),
                        events,
                    },
                    limit,
                );
                let _ = requests.send(Request::Done(result));
            })
            .map_err(|e| e.to_string())?;
        self.segment(
            state,
            Worker {
                events: send,
                requests: receive,
            },
            id,
            panel,
            limit,
        )
    }

    pub(crate) fn resume(&self, state: &mut RunState<'_, '_, '_>, id: u64, event: Value) -> Reply {
        let Some((worker, window)) = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending
            .remove(&id)
        else {
            return Err("script window is closed".into());
        };
        worker.events.send(event).map_err(|_| "script has ended")?;
        self.segment(
            state,
            worker,
            id,
            window.panel,
            state.env.time_limit.unwrap_or(crate::TIME_LIMIT),
        )
    }

    fn segment(
        &self,
        state: &mut RunState<'_, '_, '_>,
        worker: Worker,
        id: u64,
        panel: Option<String>,
        limit: Duration,
    ) -> Reply {
        let start = Instant::now();
        loop {
            let request = worker
                .requests
                .recv_timeout((limit + Duration::from_secs(1)).saturating_sub(start.elapsed()))
                .map_err(|_| "script stopped before completing the event")?;
            match request {
                Request::Done(result) => return result,
                Request::Host(method, args, reply) => {
                    let result = host::call(state, &method, &args);
                    reply
                        .send((result, state.generation))
                        .map_err(|_| "script has ended")?;
                }
                Request::Display(mut tree) => {
                    let error = tree
                        .as_object_mut()
                        .and_then(|t| t.remove("error"))
                        .and_then(|e| e.as_str().map(str::to_owned));
                    let window = ScriptWindow {
                        id,
                        panel,
                        tree: Arc::new(tree),
                    };
                    self.0
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .pending
                        .insert(id, (worker, window));
                    return error.map_or(Ok(json!(null)), Err);
                }
            }
        }
    }
}
