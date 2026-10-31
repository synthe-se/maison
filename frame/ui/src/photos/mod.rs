//! The family's photos on the frame: after `DIM_AFTER` without a finger the house gives way to
//! them, one a minute in random order, crossfaded by the GPU, the backlight going on by day and
//! night as it does. Three parts, none on the drawing thread but the last: `sync` keeps the cache
//! (`cache`) in step with Maison's list, so they go on with Maison away; `decode` reads one
//! photo at a time; `show` says, purely, what is on the panel and what comes next. The drawing
//! thread only uploads a decoded picture and frees the last one (`Order`): between two fades
//! nothing runs, nothing is drawn but the clock's minute.

pub mod cache;
pub mod decode;
pub mod show;
mod sync;

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use maison_api::photos::PhotoList;

use crate::client::Client;
pub use decode::Picture;
pub use show::{Order, Show, Step};

/// What the threads say.
pub enum Said {
    /// What the cache holds now.
    Kept(Vec<String>),
    Decoded(Picture),
    /// This photo could not be read: skipped, the log said why.
    Undecoded(String),
}

pub struct Photos {
    show: Show,
    /// What the cache holds, by id.
    kept: Vec<String>,
    /// The last list read from Maison: asked once per read that changed it.
    list: Option<Arc<PhotoList>>,
    syncing: Sender<sync::Ask>,
    decoding: Sender<String>,
    said: Receiver<Said>,
}

impl Photos {
    pub fn new(client: &Client, state_dir: &Path, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let cache = cache::Cache::new(state_dir);
        let kept = cache.ids();
        let (tx, said) = mpsc::channel();
        let decoding = decode::start(cache.clone(), tx.clone(), wake.clone());
        let syncing = sync::start(client.clone(), cache, tx, wake);
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |t| t.as_nanos() as u64);
        Self {
            show: Show::new(seed),
            kept,
            list: None,
            syncing,
            decoding,
            said,
        }
    }

    /// What the cache holds.
    #[cfg(test)]
    pub fn kept(&self) -> &[String] {
        &self.kept
    }

    /// The id of the photo on the panel, while the frame shows photos.
    pub fn on(&self) -> Option<&str> {
        self.show.on()
    }

    /// How far the photo on the panel has come in over the last one (0 to 1).
    pub fn fade(&self, now: Instant) -> f32 {
        self.show.fade(now)
    }

    /// Maison's list as last read: the cache follows it. A failed read is no list, so nothing
    /// kept goes while Maison is away.
    pub fn list(&mut self, list: Option<Arc<PhotoList>>) {
        let Some(list) = list else { return };
        if self.list.as_ref().is_some_and(|l| Arc::ptr_eq(l, &list)) {
            return;
        }
        let _ = self.syncing.send(sync::Ask::List(list.photos.clone()));
        self.list = Some(list);
    }

    /// The screen was removed: its photos go with it.
    pub fn forget(&mut self) {
        self.list = None;
        self.kept.clear();
        self.show.stop();
        let _ = self.syncing.send(sync::Ask::Forget);
    }

    /// What the threads said since.
    pub fn take(&mut self) {
        while let Ok(said) = self.said.try_recv() {
            match said {
                Said::Kept(ids) => self.kept = ids,
                Said::Decoded(picture) => self.show.came(picture),
                Said::Undecoded(id) => {
                    let next = self.show.failed(&id, &self.kept);
                    self.decode(next);
                }
            }
        }
    }

    fn decode(&self, id: Option<String>) {
        if let Some(id) = id {
            let _ = self.decoding.send(id);
        }
    }

    /// Time passed: `asleep` is the panel's state, `may` whether this screen may show them. The
    /// show starts, ends or goes on; what it asks to read goes to the reader.
    pub fn tick(&mut self, now: Instant, may: bool, asleep: bool) -> Step {
        if !may {
            let screen = self.show.on().is_some();
            self.show.stop();
            return Step {
                screen,
                ..Step::default()
            };
        }
        if asleep {
            let first = self.show.start(&self.kept);
            self.decode(first);
        }
        let mut step = self.show.tick(now, &self.kept);
        self.decode(step.ask.take());
        step
    }

    /// A finger: the house comes back.
    pub fn stop(&mut self) {
        self.show.stop();
    }

    /// When time alone changes something.
    pub fn next(&self, now: Instant) -> Option<Instant> {
        self.show.next(now)
    }

    pub fn orders(&mut self) -> Vec<Order> {
        self.show.orders()
    }

    /// The panel took the picture.
    pub fn staged(&mut self, now: Instant) {
        self.show.staged(now);
    }

    /// The panel could not take it.
    pub fn not_staged(&mut self, now: Instant) {
        let next = self.show.not_staged(now, &self.kept);
        self.decode(next);
    }

    /// A simulation's hurry: this photo's time is up.
    pub fn turn(&mut self, now: Instant) {
        self.show.turn(now);
    }
}
