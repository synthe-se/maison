//! The cache kept in step with Maison's list, on its own thread: one read changed the list, what
//! left goes, what came is fetched one photo at a time. Maison gone halfway, what came stays and
//! the rest is asked again later, with a growing wait; a new list always wins.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use maison_api::photos::{MAX_PHOTO_BYTES, Photo};

use super::Said;
use super::cache::{Cache, MAX_BYTES, plan};
use crate::client::{Client, Failure};
use crate::paths;

/// After a failed fetch, the wait doubles from this, up to `MOST`.
#[cfg(not(test))]
const FIRST: Duration = Duration::from_secs(5);
#[cfg(test)]
const FIRST: Duration = Duration::from_millis(50);
const MOST: Duration = Duration::from_secs(300);

pub enum Ask {
    /// Maison's list as read now.
    List(Vec<Photo>),
    /// The screen was removed.
    Forget,
}

/// The wait after `attempts` failures in a row.
fn wait_after(attempts: u32) -> Duration {
    FIRST
        .saturating_mul(2u32.saturating_pow(attempts.saturating_sub(1)))
        .min(MOST)
}

pub fn start(
    client: Client,
    cache: Cache,
    said: Sender<Said>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Sender<Ask> {
    let (tx, asks) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("photos: sync".into())
        .spawn(move || run(&client, &cache, &asks, &said, &*wake));
    if let Err(e) = spawned {
        eprintln!("the photos' sync could not start: {e}");
    }
    tx
}

/// The next thing to do: the latest ask; `Ok(None)` when the wait ended with none.
fn next(asks: &Receiver<Ask>, wait: Option<Duration>) -> Result<Option<Ask>, ()> {
    let first = match wait {
        None => asks.recv().map_err(|_| ())?,
        Some(t) => match asks.recv_timeout(t) {
            Ok(ask) => ask,
            Err(RecvTimeoutError::Timeout) => return Ok(None),
            Err(RecvTimeoutError::Disconnected) => return Err(()),
        },
    };
    Ok(Some(asks.try_iter().last().unwrap_or(first)))
}

fn run(
    client: &Client,
    cache: &Cache,
    asks: &Receiver<Ask>,
    said: &Sender<Said>,
    wake: &(dyn Fn() + Send + Sync),
) {
    let mut wanted: Option<Vec<Photo>> = None;
    let mut attempts = 0_u32;
    loop {
        let wait = (attempts > 0).then(|| wait_after(attempts));
        match next(asks, wait) {
            Err(()) => return,
            Ok(None) => {}
            Ok(Some(Ask::List(list))) => {
                wanted = Some(list);
                attempts = 0;
            }
            Ok(Some(Ask::Forget)) => {
                wanted = None;
                attempts = 0;
                cache.clear();
            }
        }
        let say = || {
            let _ = said.send(Said::Kept(cache.ids()));
            wake();
        };
        let whole = match &wanted {
            Some(list) => step(client, cache, list, &say),
            None => true,
        };
        attempts = if whole { 0 } else { attempts.saturating_add(1) };
        say();
    }
}

/// One pass over `list`: true when the cache is in step with it.
fn step(client: &Client, cache: &Cache, list: &[Photo], say: &dyn Fn()) -> bool {
    let todo = plan(list, &cache.kept(), MAX_BYTES);
    for id in &todo.remove {
        cache.remove(id);
    }
    if !todo.remove.is_empty() {
        say();
    }
    for id in &todo.fetch {
        match client.bytes(&paths::photo(id), MAX_PHOTO_BYTES as u64) {
            Ok(bytes) => {
                if let Err(e) = cache.save(id, &bytes) {
                    eprintln!("photo {id} not kept: {e}");
                    return false;
                }
                say();
            }
            // removed since the list was read: the next list says so
            Err(Failure::Refused { status: 404, .. }) => {}
            Err(f) => {
                eprintln!("photo {id}: {f}");
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::stub::serve;
    use crate::testing::{temp_dir, wait_until};
    use std::sync::atomic::{AtomicBool, Ordering};

    fn id(n: u8) -> String {
        format!("{n:0>64}")
    }

    fn photo(n: u8) -> Photo {
        Photo {
            id: id(n),
            bytes: 4,
        }
    }

    /// The thread, with what it says gathered by `ids()`.
    fn started(origin: &str, cache: &Cache) -> (Sender<Ask>, Receiver<Said>) {
        let client = Client::new(origin, &temp_dir("sync-client"), Duration::from_secs(5));
        let (tx, rx) = mpsc::channel();
        (start(client, cache.clone(), tx, Arc::new(|| {})), rx)
    }

    #[test]
    fn a_failure_waits_longer_each_time_up_to_a_limit() {
        assert_eq!(wait_after(1), FIRST);
        assert_eq!(wait_after(2), FIRST * 2);
        assert_eq!(wait_after(5), FIRST * 16);
        assert_eq!(wait_after(40), MOST);
    }

    #[test]
    fn the_cache_follows_the_list_and_keeps_going_with_maison_away() {
        let away = Arc::new(AtomicBool::new(false));
        let gone = away.clone();
        let stub = serve(move |line| {
            if gone.load(Ordering::SeqCst) {
                (503, vec![], String::new())
            } else if line.contains("/api/photos/") {
                (200, vec![], "jpeg".into())
            } else {
                (404, vec![], String::new())
            }
        });
        let cache = Cache::new(&temp_dir("sync"));
        let (asks, said) = started(&stub.origin, &cache);

        asks.send(Ask::List(vec![photo(1), photo(2)])).unwrap();
        wait_until(|| cache.ids().len() == 2);
        assert_eq!(cache.read(&id(1)).unwrap(), b"jpeg");
        wait_until(|| {
            said.try_iter()
                .any(|s| matches!(s, Said::Kept(k) if k.len() == 2))
        });

        // kept for good: never fetched again
        let fetched = || stub.asked.lock().unwrap().len();
        let before = fetched();
        asks.send(Ask::List(vec![photo(2), photo(1)])).unwrap();
        // a removed one goes at the next list, Maison away or not: no fetch is needed
        away.store(true, Ordering::SeqCst);
        asks.send(Ask::List(vec![photo(2)])).unwrap();
        wait_until(|| cache.ids() == [id(2)]);
        assert_eq!(fetched(), before);

        // what cannot be fetched leaves the rest kept
        asks.send(Ask::List(vec![photo(2), photo(3)])).unwrap();
        wait_until(|| fetched() > before);
        assert_eq!(cache.ids(), [id(2)]);
        // back: asked again by itself
        away.store(false, Ordering::SeqCst);
        wait_until(|| cache.ids().len() == 2);
    }

    #[test]
    fn a_photo_removed_since_the_list_stops_nothing() {
        let stub = serve(|line| {
            if line.contains(&id(1)) {
                (404, vec![], r#"{"code":"not_found"}"#.into())
            } else {
                (200, vec![], "jpeg".into())
            }
        });
        let cache = Cache::new(&temp_dir("sync-404"));
        let (asks, _said) = started(&stub.origin, &cache);
        asks.send(Ask::List(vec![photo(1), photo(2)])).unwrap();
        wait_until(|| cache.ids() == [id(2)]);
    }

    #[test]
    fn a_removed_screen_keeps_nothing() {
        let stub = serve(|_| (200, vec![], "jpeg".into()));
        let cache = Cache::new(&temp_dir("sync-forget"));
        let (asks, said) = started(&stub.origin, &cache);
        asks.send(Ask::List(vec![photo(1)])).unwrap();
        wait_until(|| cache.ids().len() == 1);
        asks.send(Ask::Forget).unwrap();
        wait_until(|| cache.ids().is_empty());
        wait_until(|| {
            said.try_iter()
                .any(|s| matches!(s, Said::Kept(k) if k.is_empty()))
        });
    }
}
