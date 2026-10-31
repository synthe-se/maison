//! Server state, as the web app's `live()` reads it: one worker thread per source, each at its
//! own pace (a function of what it last read: the shutters faster while a motor runs), its
//! latest value kept here with when it came. A source is read while a view wants it: each
//! drawing of the screen says what it shows (`want`), and a source nobody showed in the last
//! drawing stops (`sweep`), as a component unmounted. After a gesture, `refresh(prefix)` reads
//! a family again at once. Values reach the drawing thread over a channel, which then wakes.
//!
//! A value is stale once it is older than `STALE_CADENCES` times its pace (a source that stopped
//! answering), or when its last read failed: the screen says it is old, never blank.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::client::{Client, Failure};

/// A value read late this many times its pace is said to be old (the web app's `STALE_CADENCES`).
const STALE_CADENCES: u32 = maison_api::paces::STALE_CADENCES as u32;

/// What a source reads.
pub type Fetch<T> = Arc<dyn Fn(&Client) -> Result<T, Failure> + Send + Sync>;
/// How long until it reads again, from what it last read (`None`: nothing yet, or a failure).
pub type Pace<T> = Arc<dyn Fn(Option<&T>) -> Duration + Send + Sync>;

/// A source: a key (a family's prefix first: `meross:list`), its fetch, its pace.
pub struct Source<T> {
    pub key: String,
    pub fetch: Fetch<T>,
    pub pace: Pace<T>,
}

impl<T> Clone for Source<T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            fetch: self.fetch.clone(),
            pace: self.pace.clone(),
        }
    }
}

impl<T: PartialEq + Send + Sync + 'static> Source<T> {
    /// Read every `every`.
    pub fn every(
        key: impl Into<String>,
        every: Duration,
        fetch: impl Fn(&Client) -> Result<T, Failure> + Send + Sync + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            fetch: Arc::new(fetch),
            pace: Arc::new(move |_| every),
        }
    }

    /// Read at a pace that depends on what was read.
    pub fn paced(
        key: impl Into<String>,
        pace: impl Fn(Option<&T>) -> Duration + Send + Sync + 'static,
        fetch: impl Fn(&Client) -> Result<T, Failure> + Send + Sync + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            fetch: Arc::new(fetch),
            pace: Arc::new(pace),
        }
    }
}

/// What a worker says.
pub enum Read {
    Value {
        key: String,
        /// `None`: the same as its last read.
        value: Option<Arc<dyn Any + Send + Sync>>,
        pace: Duration,
        /// When it was asked (a gesture settles on a read asked after its answer).
        asked: Instant,
    },
    Failed {
        key: String,
        failure: Failure,
        pace: Duration,
    },
    /// The server said what changed (its event stream): those sources read again now.
    Changed(String),
}

/// What `Live::take` found.
#[derive(Debug, Default)]
pub struct Came {
    /// The reads: each value's key and when it was asked.
    pub read: Vec<(String, Instant)>,
    /// One of them changes what a view shows.
    pub changed: bool,
}

/// A value as a view sees it (shared: a view reads several at once).
#[derive(Debug)]
pub struct View<T> {
    pub data: Option<Arc<T>>,
    /// Asked, nothing back yet.
    pub loading: bool,
    /// How the last read failed (and nothing else is known when `data` is none).
    pub failure: Option<Failure>,
    /// Older than `STALE_CADENCES` paces, or its last read failed: shown, said old.
    pub stale: bool,
    /// When the value shown was read.
    pub at: Option<Instant>,
}

impl<T> View<T> {
    /// A value read just now, without a failure (a test's, a family's own).
    #[cfg(test)]
    pub fn ready(data: T) -> Self {
        Self {
            data: Some(Arc::new(data)),
            loading: false,
            failure: None,
            stale: false,
            at: None,
        }
    }

    pub fn failed(&self) -> bool {
        self.failure.is_some()
    }

    /// The same read, its value seen through `f` (a list's items), its state kept.
    pub fn map<U>(self, f: impl FnOnce(&T) -> U) -> View<U> {
        View {
            data: self.data.map(|d| Arc::new(f(&d))),
            loading: self.loading,
            failure: self.failure,
            stale: self.stale,
            at: self.at,
        }
    }
}

struct Entry {
    value: Option<Arc<dyn Any + Send + Sync>>,
    at: Option<Instant>,
    pace: Duration,
    failure: Option<Failure>,
    /// The worker's ear: dropped, it ends; a message reads again at once.
    poke: Option<Sender<()>>,
    /// The drawing that last wanted it.
    wanted: u64,
}

impl Entry {
    /// Its value is older than `STALE_CADENCES` paces.
    fn old(&self) -> bool {
        self.at
            .is_some_and(|at| at.elapsed() > self.pace.saturating_mul(STALE_CADENCES))
    }
}

pub struct Live {
    client: Client,
    /// The server's event stream is open: pushed sources are read only as a safety net.
    pushed: Arc<AtomicBool>,
    entries: HashMap<String, Entry>,
    tx: Sender<Read>,
    rx: Receiver<Read>,
    wake: Arc<dyn Fn() + Send + Sync>,
    drawing: u64,
    /// The last read said the screen is not signed in.
    signed_out: bool,
    /// Wakes the event stream's thread from its wait (the screen was just paired).
    reopen: Option<Sender<()>>,
}

impl Live {
    /// `wake`: called from a worker after each read (the drawing thread then calls `take`).
    pub fn new(client: Client, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            client,
            pushed: Arc::new(AtomicBool::new(false)),
            entries: HashMap::new(),
            reopen: None,
            tx,
            rx,
            wake,
            drawing: 0,
            signed_out: false,
        }
    }

    /// A drawing starts: what it wants is counted from here.
    pub fn begin(&mut self) {
        self.drawing += 1;
    }

    /// The source's value now; its worker started on the first ask.
    pub fn want<T: PartialEq + Send + Sync + 'static>(&mut self, source: &Source<T>) -> View<T> {
        let drawing = self.drawing;
        // its worker started on the first ask, or again when wanted after a sweep (its last
        // value shown while it reads again)
        let running = self
            .entries
            .get(&source.key)
            .is_some_and(|e| e.poke.is_some());
        let poke = (!running).then(|| self.spawn(source.clone()));
        let e = self
            .entries
            .entry(source.key.clone())
            .or_insert_with(|| Entry {
                value: None,
                at: None,
                pace: (source.pace)(None),
                failure: None,
                poke: None,
                wanted: drawing,
            });
        e.wanted = drawing;
        if poke.is_some() {
            e.poke = poke;
        }
        let data = e.value.clone().and_then(|v| v.downcast::<T>().ok());
        let stale = data.is_some() && (e.old() || e.failure.is_some());
        View {
            data,
            loading: e.at.is_none() && e.failure.is_none(),
            failure: e.failure.clone(),
            stale,
            at: e.at,
        }
    }

    /// Without asking for it: the last value of a source some view reads.
    #[cfg(test)]
    pub fn peek<T: Send + Sync + 'static>(&self, key: &str) -> Option<Arc<T>> {
        self.entries.get(key)?.value.clone()?.downcast().ok()
    }

    /// A drawing ended: the sources it did not want stop (their values kept for a return).
    pub fn sweep(&mut self) {
        let drawing = self.drawing;
        for e in self.entries.values_mut() {
            if e.wanted != drawing {
                e.poke = None;
            }
        }
    }

    /// Every source under `prefix` reads again now (after a gesture).
    pub fn refresh(&self, prefix: &str) {
        for (k, e) in &self.entries {
            if k.starts_with(prefix)
                && let Some(p) = &e.poke
            {
                let _ = p.send(());
            }
        }
    }

    /// The reads that came in. An answer identical to the last changes nothing shown, unless it
    /// ends a failure or an old value.
    pub fn take(&mut self) -> Came {
        let mut came = Came::default();
        while let Ok(read) = self.rx.try_recv() {
            match read {
                Read::Value {
                    key,
                    value,
                    pace,
                    asked,
                } => {
                    came.read.push((key.clone(), asked));
                    if let Some(e) = self.entries.get_mut(&key) {
                        came.changed |= value.is_some() || e.failure.is_some() || e.old();
                        if value.is_some() {
                            e.value = value;
                        }
                        e.at = Some(Instant::now());
                        e.pace = pace;
                        e.failure = None;
                    }
                    self.signed_out = false;
                }
                Read::Changed(prefix) => self.refresh(&prefix),
                Read::Failed { key, failure, pace } => {
                    came.read.push((key.clone(), Instant::now()));
                    if failure == Failure::SignedOut {
                        self.signed_out = true;
                    } else {
                        eprintln!("{key}: {failure}");
                    }
                    if let Some(e) = self.entries.get_mut(&key) {
                        came.changed |= e.failure.as_ref() != Some(&failure);
                        e.failure = Some(failure);
                        e.pace = pace;
                    }
                }
            }
        }
        came
    }

    /// A read said 401: the screen was removed (or never paired).
    pub fn signed_out(&self) -> bool {
        self.signed_out
    }

    /// Everything stops and is forgotten (signed out: the house is no longer shown).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.signed_out = false;
    }

    /// When the first value turns stale after `after`: the screen is drawn again then, to say so.
    pub fn next_stale(&self, after: Instant) -> Option<Instant> {
        self.entries
            .values()
            .filter(|e| e.poke.is_some() && e.failure.is_none())
            .filter_map(|e| e.at?.checked_add(e.pace.saturating_mul(STALE_CADENCES)))
            .filter(|t| *t > after)
            .min()
    }

    fn spawn<T: PartialEq + Send + Sync + 'static>(&self, source: Source<T>) -> Sender<()> {
        spawn(
            &self.client,
            source,
            self.tx.clone(),
            self.wake.clone(),
            self.pushed.clone(),
        )
    }

    /// Opens the server's event stream on its own thread, for the app's life: each change
    /// read again at once; while it is open, the pushed sources' polls slow to a safety net.
    /// Closed or refused, it waits (doubling, 32 s at most) or until `reopen()`.
    pub fn listen(&mut self) {
        let (poke, poked) = mpsc::channel();
        self.reopen = Some(poke);
        let (client, tx, wake, pushed) = (
            self.client.clone(),
            self.tx.clone(),
            self.wake.clone(),
            self.pushed.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("events".into())
            .spawn(move || {
                let mut attempt = 0_u32;
                loop {
                    match client.events() {
                        Ok(stream) => {
                            attempt = 0;
                            pushed.store(true, Ordering::Release);
                            // what changed while it was closed: everything reads again
                            if tx.send(Read::Changed(String::new())).is_err() {
                                return;
                            }
                            wake();
                            for name in changes(stream) {
                                if tx.send(Read::Changed(name)).is_err() {
                                    return;
                                }
                                wake();
                            }
                            pushed.store(false, Ordering::Release);
                        }
                        Err(f) => {
                            pushed.store(false, Ordering::Release);
                            if f != Failure::SignedOut {
                                eprintln!("events: {f}");
                            }
                        }
                    }
                    attempt = attempt.saturating_add(1);
                    let wait = Duration::from_secs(2u64.saturating_pow(attempt.min(5)));
                    if let Err(mpsc::RecvTimeoutError::Disconnected) = poked.recv_timeout(wait) {
                        return;
                    }
                }
            });
        if let Err(e) = spawned {
            eprintln!("the event stream could not start: {e}");
        }
    }
}

impl Live {
    /// Opens the event stream again now, without its wait (the screen was just paired: until
    /// then it was refused).
    pub fn reopen(&self) {
        if let Some(poke) = &self.reopen {
            let _ = poke.send(());
        }
    }
}

/// The names an event stream says changed (`data: <name>` lines of `change` events).
pub fn changes(stream: impl std::io::BufRead) -> impl Iterator<Item = String> {
    stream.lines().map_while(Result::ok).filter_map(|line| {
        line.strip_prefix("data:")
            .map(|d| d.trim_start().to_string())
    })
}

/// Whether the server's event stream says when `key` changes.
fn is_pushed(key: &str) -> bool {
    maison_api::events::ALL
        .iter()
        .any(|(_, name)| key.starts_with(name))
        || key.starts_with("tuya:")
}

/// A source's pace, given whether the event stream is open.
fn effective(key: &str, pace: Duration, pushed: bool) -> Duration {
    if pushed && is_pushed(key) {
        pace.max(Duration::from_millis(maison_api::events::PUSHED_SAFETY_MS))
    } else {
        pace
    }
}

/// The worker: read, say it, wait the pace (or a poke), again; ends when its ear is dropped.
fn spawn<T: PartialEq + Send + Sync + 'static>(
    client: &Client,
    source: Source<T>,
    tx: Sender<Read>,
    wake: Arc<dyn Fn() + Send + Sync>,
    pushed: Arc<AtomicBool>,
) -> Sender<()> {
    let (poke, ear) = mpsc::channel::<()>();
    let client = client.clone();
    let name = format!("live {}", source.key);
    let spawned = std::thread::Builder::new().name(name).spawn(move || {
        // compared here, off the drawing thread: an identical answer draws nothing
        let mut last: Option<Arc<T>> = None;
        loop {
            let asked = Instant::now();
            let read = match (source.fetch)(&client) {
                Ok(v) => {
                    let pace = effective(
                        &source.key,
                        (source.pace)(Some(&v)),
                        pushed.load(Ordering::Acquire),
                    );
                    let value = (last.as_deref() != Some(&v)).then(|| Arc::new(v));
                    if value.is_some() {
                        last.clone_from(&value);
                    }
                    (
                        Read::Value {
                            key: source.key.clone(),
                            value: value.map(|v| v as Arc<dyn Any + Send + Sync>),
                            pace,
                            asked,
                        },
                        pace,
                    )
                }
                Err(failure) => {
                    let pace = effective(
                        &source.key,
                        (source.pace)(None),
                        pushed.load(Ordering::Acquire),
                    );
                    (
                        Read::Failed {
                            key: source.key.clone(),
                            failure,
                            pace,
                        },
                        pace,
                    )
                }
            };
            let (read, pace) = read;
            if tx.send(read).is_err() {
                return;
            }
            wake();
            match ear.recv_timeout(pace) {
                Ok(()) => {
                    // pokes that piled up while reading count once
                    while ear.try_recv().is_ok() {}
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("a live source could not start: {e}");
    }
    poke
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::stub::serve;
    use crate::testing::{temp_dir, wait_until};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn live(origin: &str) -> (Live, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let w = wakes.clone();
        let client = Client::new(origin, &temp_dir("live"), Duration::from_secs(5));
        (
            Live::new(
                client,
                Arc::new(move || {
                    w.fetch_add(1, Ordering::SeqCst);
                }),
            ),
            wakes,
        )
    }

    fn counter() -> Source<u32> {
        Source::every("n:count", Duration::from_secs(3600), |c: &Client| {
            c.get::<serde_json::Value>("/n")
                .map(|v| v["n"].as_u64().unwrap_or(0) as u32)
        })
    }

    #[test]
    fn an_event_stream_says_names_and_pushed_sources_slow_down() {
        let stream =
            ": keep-alive\n\nevent: change\ndata: zigbee-lamp\n\nevent: change\ndata: \n\n";
        let names: Vec<String> = changes(std::io::Cursor::new(stream)).collect();
        assert_eq!(names, ["zigbee-lamp", ""]);
        let fast = Duration::from_secs(1);
        assert_eq!(effective("shutters:list", fast, false), fast);
        assert_eq!(
            effective("shutters:list", fast, true),
            Duration::from_secs(60)
        );
        assert_eq!(
            effective("meross:electricity:p1", fast, true),
            Duration::from_secs(60)
        );
        assert_eq!(
            effective("hue-lamp:list", fast, true),
            fast,
            "Hue says nothing: still polled"
        );
    }

    #[test]
    fn a_wanted_source_reads_wakes_and_reads_again_when_refreshed() {
        let n = Arc::new(AtomicUsize::new(0));
        let served = n.clone();
        let stub = serve(move |_| {
            let i = served.fetch_add(1, Ordering::SeqCst) + 1;
            (200, vec![], format!(r#"{{"n":{i}}}"#))
        });
        let (mut live, wakes) = live(&stub.origin);
        let source = counter();
        live.begin();
        let first = live.want(&source);
        assert!(first.loading && first.data.is_none());
        wait_until(|| wakes.load(Ordering::SeqCst) == 1);
        assert!(live.take().changed);
        let v = live.want(&source);
        assert_eq!(v.data.as_deref(), Some(&1));
        assert!(!v.stale && !v.loading);
        // a gesture reads the family again, at once
        live.refresh("n:");
        wait_until(|| wakes.load(Ordering::SeqCst) == 2);
        live.take();
        assert_eq!(live.want(&source).data.as_deref(), Some(&2));
        assert_eq!(live.peek::<u32>("n:count").as_deref(), Some(&2));
    }

    #[test]
    fn an_identical_answer_changes_nothing_shown_but_still_counts_as_read() {
        let n = Arc::new(AtomicUsize::new(0));
        let served = n.clone();
        let stub = serve(move |_| match served.fetch_add(1, Ordering::SeqCst) {
            0 | 1 => (200, vec![], r#"{"n":1}"#.into()),
            2 => (500, vec![], "{}".into()),
            3 => (200, vec![], r#"{"n":1}"#.into()),
            _ => (200, vec![], r#"{"n":2}"#.into()),
        });
        let (mut live, wakes) = live(&stub.origin);
        let source = counter();
        live.begin();
        live.want(&source);
        let turn = |live: &mut Live, wanted: usize| {
            if wanted > 1 {
                live.refresh("n:");
            }
            wait_until(|| wakes.load(Ordering::SeqCst) == wanted);
            live.take()
        };
        assert!(turn(&mut live, 1).changed, "the first answer");
        let same = turn(&mut live, 2);
        assert!(!same.changed, "the same again");
        assert_eq!(same.read.len(), 1, "a gesture settles on it all the same");
        assert!(turn(&mut live, 3).changed, "a failure is said");
        assert!(
            turn(&mut live, 4).changed,
            "the same again ends the failure"
        );
        assert!(turn(&mut live, 5).changed, "another value");
        assert_eq!(live.want(&source).data.as_deref(), Some(&2));
    }

    #[test]
    fn a_source_nobody_shows_stops() {
        let stub = serve(|_| (200, vec![], r#"{"n":1}"#.into()));
        let (mut live, wakes) = live(&stub.origin);
        let source = counter();
        live.begin();
        live.want(&source);
        wait_until(|| wakes.load(Ordering::SeqCst) == 1);
        live.take();
        live.begin();
        live.sweep();
        live.refresh("n:");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            1,
            "stopped: a refresh reads nothing"
        );
        // its last value is still there, and it reads again once shown again
        live.begin();
        assert_eq!(live.want(&source).data.as_deref(), Some(&1));
        wait_until(|| wakes.load(Ordering::SeqCst) == 2);
    }

    #[test]
    fn a_failure_keeps_the_last_value_said_old_and_a_401_signs_out() {
        let n = Arc::new(AtomicUsize::new(0));
        let served = n.clone();
        let stub = serve(move |_| match served.fetch_add(1, Ordering::SeqCst) {
            0 => (200, vec![], r#"{"n":7}"#.into()),
            1 => (500, vec![], "{}".into()),
            _ => (401, vec![], "{}".into()),
        });
        let (mut live, wakes) = live(&stub.origin);
        let source = counter();
        live.begin();
        live.want(&source);
        wait_until(|| wakes.load(Ordering::SeqCst) == 1);
        live.take();
        live.refresh("n:");
        wait_until(|| wakes.load(Ordering::SeqCst) == 2);
        live.take();
        let v = live.want(&source);
        assert_eq!(v.data.as_deref(), Some(&7));
        assert!(v.failed() && v.stale);
        assert!(matches!(
            v.failure,
            Some(Failure::Refused { status: 500, .. })
        ));
        assert!(!live.signed_out());
        live.refresh("n:");
        wait_until(|| wakes.load(Ordering::SeqCst) == 3);
        live.take();
        assert!(live.signed_out());
        live.clear();
        assert!(live.peek::<u32>("n:count").is_none());
    }

    #[test]
    fn the_pace_follows_what_was_read() {
        let stub = serve(|_| (200, vec![], r#"{"n":5}"#.into()));
        let (mut live, wakes) = live(&stub.origin);
        let source = Source::paced(
            "p:x",
            |v: Option<&u32>| {
                if v == Some(&5) {
                    Duration::from_millis(50)
                } else {
                    Duration::from_secs(3600)
                }
            },
            |c: &Client| {
                c.get::<serde_json::Value>("/n")
                    .map(|v| v["n"].as_u64().unwrap_or(0) as u32)
            },
        );
        live.begin();
        live.want(&source);
        // read every 50 ms once it said 5
        wait_until(|| wakes.load(Ordering::SeqCst) >= 3);
        live.take();
        assert!(live.next_stale(Instant::now()).is_some());
    }
}
