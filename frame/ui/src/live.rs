//! Server state, as the web app's `live()` reads it: one worker thread per source, each at its
//! own pace (a function of what it last read: the shutters faster while a motor runs), its
//! latest value kept here with when it came. A source is read while a view wants it: each
//! drawing of the screen says what it shows (`want`), and a source nobody showed in the last
//! drawing stops (`sweep`), as a component unmounted. After a gesture, `refresh(prefix)` reads
//! a family again at once. Values reach the drawing thread over a channel, which then wakes.
//!
//! A value is stale once it is older than three times its pace (a source that stopped
//! answering), or when its last read failed: the screen says it is old, never blank.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
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

impl<T: Send + Sync + 'static> Source<T> {
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
        value: Arc<dyn Any + Send + Sync>,
        pace: Duration,
        /// When it was asked (a gesture settles on a read asked after its answer).
        asked: Instant,
    },
    Failed {
        key: String,
        failure: Failure,
        pace: Duration,
    },
}

/// A value as a view sees it (shared: a view reads several at once).
pub struct View<T> {
    pub data: Option<Arc<T>>,
    /// Asked, nothing back yet.
    pub loading: bool,
    /// The last read failed (and nothing else is known when `data` is none).
    pub failed: bool,
    /// Older than three paces, or its last read failed: shown, said old.
    pub stale: bool,
    /// When the value shown was read.
    pub at: Option<Instant>,
}

struct Entry {
    value: Option<Arc<dyn Any + Send + Sync>>,
    at: Option<Instant>,
    pace: Duration,
    failed: bool,
    /// The worker's ear: dropped, it ends; a message reads again at once.
    poke: Option<Sender<()>>,
    /// The drawing that last wanted it.
    wanted: u64,
}

pub struct Live {
    client: Client,
    entries: HashMap<String, Entry>,
    tx: Sender<Read>,
    rx: Receiver<Read>,
    wake: Arc<dyn Fn() + Send + Sync>,
    drawing: u64,
    /// The last read said the screen is not signed in.
    signed_out: bool,
}

impl Live {
    /// `wake`: called from a worker after each read (the drawing thread then calls `take`).
    pub fn new(client: Client, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            client,
            entries: HashMap::new(),
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
    pub fn want<T: Send + Sync + 'static>(&mut self, source: &Source<T>) -> View<T> {
        let drawing = self.drawing;
        if !self.entries.contains_key(&source.key) {
            let poke = self.spawn(source.clone());
            self.entries.insert(
                source.key.clone(),
                Entry {
                    value: None,
                    at: None,
                    pace: (source.pace)(None),
                    failed: false,
                    poke: Some(poke),
                    wanted: drawing,
                },
            );
        }
        let e = self.entries.get_mut(&source.key).expect("just inserted");
        e.wanted = drawing;
        if e.poke.is_none() {
            // wanted again after a sweep: its last value shown while it reads again
            e.poke = Some(spawn(
                &self.client,
                source.clone(),
                self.tx.clone(),
                self.wake.clone(),
            ));
        }
        let e = &self.entries[&source.key];
        let data = e.value.clone().and_then(|v| v.downcast::<T>().ok());
        let old =
            e.at.is_some_and(|at| at.elapsed() > e.pace.saturating_mul(STALE_CADENCES));
        let stale = data.is_some() && (old || e.failed);
        View {
            data,
            loading: e.at.is_none() && !e.failed,
            failed: e.failed,
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

    /// The reads that came in: each value's key and when it was asked (empty: nothing changed).
    pub fn take(&mut self) -> Vec<(String, Instant)> {
        let mut read_now = Vec::new();
        while let Ok(read) = self.rx.try_recv() {
            match read {
                Read::Value {
                    key,
                    value,
                    pace,
                    asked,
                } => {
                    read_now.push((key.clone(), asked));
                    if let Some(e) = self.entries.get_mut(&key) {
                        e.value = Some(value);
                        e.at = Some(Instant::now());
                        e.pace = pace;
                        e.failed = false;
                    }
                    self.signed_out = false;
                }
                Read::Failed { key, failure, pace } => {
                    read_now.push((key.clone(), Instant::now()));
                    if failure == Failure::SignedOut {
                        self.signed_out = true;
                    } else {
                        eprintln!("{key}: {failure}");
                    }
                    if let Some(e) = self.entries.get_mut(&key) {
                        e.failed = true;
                        e.pace = pace;
                    }
                }
            }
        }
        read_now
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

    /// When the soonest value turns stale: the screen is drawn again then, to say so.
    pub fn next_stale(&self) -> Option<Instant> {
        self.entries
            .values()
            .filter(|e| e.poke.is_some() && !e.failed)
            .filter_map(|e| e.at?.checked_add(e.pace.saturating_mul(STALE_CADENCES)))
            .filter(|t| *t > Instant::now())
            .min()
    }

    fn spawn<T: Send + Sync + 'static>(&self, source: Source<T>) -> Sender<()> {
        spawn(&self.client, source, self.tx.clone(), self.wake.clone())
    }
}

/// The worker: read, say it, wait the pace (or a poke), again; ends when its ear is dropped.
fn spawn<T: Send + Sync + 'static>(
    client: &Client,
    source: Source<T>,
    tx: Sender<Read>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Sender<()> {
    let (poke, ear) = mpsc::channel::<()>();
    let client = client.clone();
    let name = format!("live {}", source.key);
    let spawned = std::thread::Builder::new().name(name).spawn(move || {
        loop {
            let asked = Instant::now();
            let read = match (source.fetch)(&client) {
                Ok(v) => {
                    let pace = (source.pace)(Some(&v));
                    (
                        Read::Value {
                            key: source.key.clone(),
                            value: Arc::new(v),
                            pace,
                            asked,
                        },
                        pace,
                    )
                }
                Err(failure) => {
                    let pace = (source.pace)(None);
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
        assert!(!live.take().is_empty());
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
        assert!(v.failed && v.stale);
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
        assert!(live.next_stale().is_some());
    }
}
