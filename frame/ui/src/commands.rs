//! Orders to the house, as the web app's `Command` and `Gesture` send them (docs/ux.md § 2):
//! each on its own thread (a gesture never waits on another, nor blocks a frame); a toggle
//! shows its target at once, « slow » after `COMMAND_SLOW_MS`, and past its family's limit
//! (`maison_api::paces`: a lamp's, a plug's) goes back to what the device said, « Pas de
//! réponse · Réessayer » on its tile. An answer reads its family again (`refresh`); a group's
//! « Tout … » sends every order at once and says the outcome once, naming who did not answer.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use maison_api::paces;
use serde::Serialize;
use serde_json::Value;

use crate::client::{Client, Failure};
use crate::i18n::Words;

/// How long a family's device may take (`maison_api::paces`, the web app's `LIMIT`).
pub mod limit {
    use maison_api::paces;
    use std::time::Duration;
    pub const LAMP: Duration = Duration::from_millis(paces::LAMP_LIMIT_MS);
    pub const PLUG: Duration = Duration::from_millis(paces::PLUG_LIMIT_MS);
    pub const DEVICE: Duration = Duration::from_millis(paces::DEVICE_LIMIT_MS);
    /// The TV, and the box (waking it wakes the TV over CEC).
    pub const TV: Duration = Duration::from_millis(paces::TV_LIMIT_MS);
}

/// Without an answer this long, the gesture says it is slow.
pub const SLOW: Duration = Duration::from_millis(paces::COMMAND_SLOW_MS);

/// What an answer says once it came: a toast's words, warning or not.
pub type Say = fn(&Words, &Outcome) -> Option<Toast>;

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    pub warn: bool,
}

/// How an order (or a group's orders) went.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The names of the devices that did it (one order: its name, if it did).
    pub done: Vec<String>,
    /// Those that refused or did not answer.
    pub failed: Vec<String>,
    /// The single order's answer (a scene's results).
    pub answer: Option<Value>,
}

/// One request to the API that changes something: a POST, or a PUT (`put`: the TV's volume).
#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub path: String,
    pub body: Option<Value>,
    pub put: bool,
}

impl Post {
    /// A POST without a body.
    pub fn empty(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            body: None,
            put: false,
        }
    }

    /// A PUT of `body` (a setting set to a value).
    pub fn put(path: impl Into<String>, body: &impl Serialize) -> Self {
        Self {
            put: true,
            ..Self::json(path, body)
        }
    }

    /// A POST of one of the API's request types (`maison_api::lamps::PowerRequest`…): a field
    /// the API renames no longer builds here.
    pub fn json(path: impl Into<String>, body: &impl Serialize) -> Self {
        Self {
            path: path.into(),
            body: Some(json(body)),
            put: false,
        }
    }
}

/// A request type as JSON. The API's types are plain structs: they always serialize; were one
/// ever to fail, the order goes without a body and the server says what it lacks.
pub fn json(body: &impl Serialize) -> Value {
    serde_json::to_value(body).unwrap_or_else(|e| {
        eprintln!("a request body did not serialize: {e}");
        Value::Null
    })
}

/// A gesture: what it sends, what it reads again after, how it is shown and said.
#[derive(Debug, Clone)]
pub struct Order {
    /// What the gesture's state is kept under (`hue-lamp:1:power`, `scene:night`).
    pub key: String,
    /// One post per device, by its name; sent together.
    pub posts: Vec<(String, Post)>,
    /// A toggle's target, shown while it travels.
    pub target: Option<bool>,
    pub limit: Duration,
    /// The families read again once it is done.
    pub refresh: Vec<&'static str>,
    pub say: Option<Say>,
}

impl Order {
    /// One post, to the device `name`.
    pub fn one(key: impl Into<String>, name: impl Into<String>, post: Post) -> Self {
        Self::many(key, vec![(name.into(), post)])
    }

    /// One post per device, sent together (a group's « Tout … »), by the devices' names.
    pub fn many(key: impl Into<String>, posts: Vec<(String, Post)>) -> Self {
        Self {
            key: key.into(),
            posts,
            target: None,
            limit: limit::DEVICE,
            refresh: Vec::new(),
            say: None,
        }
    }

    pub fn toggle(mut self, target: bool) -> Self {
        self.target = Some(target);
        self
    }

    pub fn within(mut self, limit: Duration) -> Self {
        self.limit = limit;
        self
    }

    pub fn then_refresh(mut self, families: &[&'static str]) -> Self {
        self.refresh.extend_from_slice(families);
        self
    }

    pub fn said(mut self, say: Say) -> Self {
        self.say = Some(say);
        self
    }
}

/// A gesture's state, as its control shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    /// Travelling: its target (a toggle's), and whether it is slow.
    Pending {
        target: Option<bool>,
        slow: bool,
    },
    /// No answer within the limit, or a refusal: said on the tile until the next try.
    Late,
}

struct Running {
    generation: u64,
    started: Instant,
    target: Option<bool>,
    limit: Duration,
    late: bool,
    /// Answered: still shown travelling until its families are read again after `.1` (the web
    /// app's `settle`, inside the gesture).
    settling: Option<(Vec<&'static str>, Instant)>,
}

struct Done {
    key: String,
    generation: u64,
    outcome: Outcome,
    refresh: Vec<&'static str>,
    say: Option<Say>,
}

pub struct Commands {
    client: Client,
    running: HashMap<String, Running>,
    /// The keys with an order on the wire: one at a time each, so a slider's values and a
    /// toggle's turns arrive in the order they were given.
    on_wire: HashSet<String>,
    /// The latest order given while its key's was on the wire, sent once it answers.
    waiting: HashMap<String, (Order, u64)>,
    generation: u64,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Commands {
    pub fn new(client: Client, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            client,
            running: HashMap::new(),
            on_wire: HashSet::new(),
            waiting: HashMap::new(),
            generation: 0,
            tx,
            rx,
            wake,
        }
    }

    /// Sends the order; a newer gesture on the same key takes over from one travelling (shown
    /// at once, sent once the older one answered).
    pub fn run(&mut self, order: Order) {
        self.generation += 1;
        let generation = self.generation;
        self.running.insert(
            order.key.clone(),
            Running {
                generation,
                started: Instant::now(),
                target: order.target,
                limit: order.limit,
                late: false,
                settling: None,
            },
        );
        if self.on_wire.contains(&order.key) {
            // a newer value replaces one already waiting: only the latest goes
            self.waiting.insert(order.key.clone(), (order, generation));
            return;
        }
        self.send(order, generation);
    }

    fn send(&mut self, order: Order, generation: u64) {
        self.on_wire.insert(order.key.clone());
        let (key, names) = (
            order.key.clone(),
            order
                .posts
                .iter()
                .map(|(n, _)| n.clone())
                .collect::<Vec<_>>(),
        );
        let (refresh, say) = (order.refresh.clone(), order.say);
        let client = self.client.clone();
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("order {}", order.key))
            .spawn(move || {
                let sent: Vec<_> = order
                    .posts
                    .into_iter()
                    .map(|(name, post)| {
                        let client = client.clone();
                        let sent = std::thread::Builder::new()
                            .name(format!("post {name}"))
                            .spawn(move || {
                                if post.put {
                                    client.put::<Value>(&post.path, post.body.as_ref())
                                } else {
                                    client.post::<Value>(&post.path, post.body.as_ref())
                                }
                            });
                        (name, sent)
                    })
                    .collect();
                let mut outcome = Outcome {
                    done: Vec::new(),
                    failed: Vec::new(),
                    answer: None,
                };
                for (name, t) in sent {
                    let answer = match t {
                        Ok(t) => t
                            .join()
                            .unwrap_or_else(|_| Err(Failure::Unreadable("panicked".into()))),
                        Err(e) => Err(Failure::Unreadable(format!("could not start: {e}"))),
                    };
                    match answer {
                        Ok(v) => {
                            outcome.done.push(name);
                            outcome.answer = Some(v);
                        }
                        Err(f) => {
                            eprintln!("{} {name}: {f}", order.key);
                            outcome.failed.push(name);
                        }
                    }
                }
                let _ = tx.send(Done {
                    key: order.key,
                    generation,
                    outcome,
                    refresh: order.refresh,
                    say: order.say,
                });
                wake();
            });
        if let Err(e) = spawned {
            // said failed at once: the key is free again, the tile says « Pas de réponse »
            eprintln!("an order could not start: {e}");
            let _ = self.tx.send(Done {
                key,
                generation,
                outcome: Outcome {
                    done: Vec::new(),
                    failed: names,
                    answer: None,
                },
                refresh,
                say,
            });
            (self.wake)();
        }
    }

    /// The gesture's state now.
    pub fn state(&self, key: &str) -> State {
        match self.running.get(key) {
            None => State::Idle,
            Some(r) if r.late || r.started.elapsed() >= r.limit => State::Late,
            Some(r) => State::Pending {
                target: r.target,
                slow: r.started.elapsed() >= SLOW,
            },
        }
    }

    /// What a toggle shows: its target while it travels, else what the device said.
    pub fn shown(&self, key: &str, read: bool) -> bool {
        match self.state(key) {
            State::Pending {
                target: Some(t), ..
            } => t,
            _ => read,
        }
    }

    /// The answers that came; each reads its families again (through `refresh`) and may say
    /// something. True when anything changed.
    pub fn take(
        &mut self,
        words: &Words,
        refresh: &mut dyn FnMut(&str),
        toasts: &mut Vec<Toast>,
    ) -> bool {
        let mut changed = false;
        while let Ok(done) = self.rx.try_recv() {
            changed = true;
            self.on_wire.remove(&done.key);
            if let Some((order, generation)) = self.waiting.remove(&done.key) {
                self.send(order, generation);
            }
            for f in &done.refresh {
                refresh(f);
            }
            let Some(r) = self.running.get_mut(&done.key) else {
                continue;
            };
            // a newer gesture took over: its own answer will say
            if r.generation != done.generation {
                continue;
            }
            let ok = done.outcome.failed.is_empty();
            let past = r.started.elapsed() >= r.limit;
            if ok && !past {
                if done.refresh.is_empty() {
                    self.running.remove(&done.key);
                } else {
                    r.settling = Some((done.refresh.clone(), Instant::now()));
                }
            } else {
                // refused, or the answer came past its limit (the tile already said so)
                r.late = true;
                r.target = None;
            }
            if let Some(say) = done.say
                && let Some(t) = say(words, &done.outcome)
            {
                toasts.push(t);
            }
        }
        changed
    }

    /// A family's value came, asked at `asked`: the gestures answered before that are done.
    pub fn read(&mut self, key: &str, asked: Instant) {
        self.running.retain(|_, r| match &r.settling {
            Some((families, answered)) => {
                !(asked >= *answered && families.iter().any(|f| key.starts_with(f)))
            }
            None => true,
        });
    }

    /// The next moment a gesture's look changes by itself (slow, then late): the screen is
    /// drawn again then.
    pub fn next_change(&self) -> Option<Instant> {
        self.running
            .values()
            .filter(|r| !r.late)
            .flat_map(|r| [r.started + SLOW, r.started + r.limit])
            .filter(|t| *t > Instant::now())
            .min()
    }

    /// The late gesture forgotten (its tile tried again, or the device answered since).
    pub fn forget(&mut self, key: &str) {
        self.running.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::stub::serve;
    use crate::i18n::{Lang, Words};
    use crate::testing::{temp_dir, wait_until};
    use maison_api::lamps::{BrightnessRequest, PowerRequest};

    fn commands(origin: &str) -> Commands {
        let client = Client::new(origin, &temp_dir("commands"), Duration::from_secs(5));
        Commands::new(client, Arc::new(|| {}))
    }

    fn settle(c: &mut Commands, words: &Words) -> (Vec<String>, Vec<Toast>) {
        let mut refreshed = Vec::new();
        let mut toasts = Vec::new();
        wait_until(|| c.take(words, &mut |f| refreshed.push(f.to_string()), &mut toasts));
        (refreshed, toasts)
    }

    #[test]
    fn a_toggle_shows_its_target_then_what_the_device_says() {
        let stub = serve(|_| (200, vec![], r#"{"success":true}"#.into()));
        let words = Words::new(Lang::Fr);
        let mut c = commands(&stub.origin);
        let order = Order::one(
            "hue-lamp:1:power",
            "Salon",
            Post::json("/hue-lamps/1/power", &PowerRequest { enabled: true }),
        )
        .toggle(true)
        .within(limit::LAMP)
        .then_refresh(&["hue-lamp"]);
        c.run(order);
        assert!(c.shown("hue-lamp:1:power", false));
        assert_eq!(
            c.state("hue-lamp:1:power"),
            State::Pending {
                target: Some(true),
                slow: false
            }
        );
        assert!(c.next_change().is_some());
        let (refreshed, toasts) = settle(&mut c, &words);
        assert_eq!(refreshed, ["hue-lamp"]);
        assert!(toasts.is_empty());
        // answered, it still shows its target until the lamps are read again after the answer
        assert!(c.shown("hue-lamp:1:power", false));
        c.read("hue-lamp:list", Instant::now() - Duration::from_secs(10));
        assert!(
            c.shown("hue-lamp:1:power", false),
            "a read asked before the answer does not settle it"
        );
        c.read("hue-lamp:list", Instant::now());
        assert_eq!(c.state("hue-lamp:1:power"), State::Idle);
        assert!(!c.shown("hue-lamp:1:power", false));
        let asked = stub.asked.lock().unwrap().clone();
        assert_eq!(asked[0].line, "POST /api/hue-lamps/1/power HTTP/1.1");
        assert_eq!(asked[0].body, r#"{"enabled":true}"#);
    }

    #[test]
    fn a_refusal_is_said_late_until_tried_again() {
        let stub = serve(|_| (500, vec![], "{}".into()));
        let words = Words::new(Lang::Fr);
        let mut c = commands(&stub.origin);
        c.run(
            Order::one(
                "meross:p:toggle",
                "Bouilloire",
                Post::empty("/meross/p/toggle"),
            )
            .toggle(true),
        );
        settle(&mut c, &words);
        assert_eq!(c.state("meross:p:toggle"), State::Late);
        // the late toggle shows what the device said
        assert!(!c.shown("meross:p:toggle", false));
        c.forget("meross:p:toggle");
        assert_eq!(c.state("meross:p:toggle"), State::Idle);
    }

    #[test]
    fn past_its_limit_a_gesture_is_late() {
        let stub = serve(|_| {
            std::thread::sleep(Duration::from_millis(400));
            (200, vec![], "{}".into())
        });
        let mut c = commands(&stub.origin);
        c.run(
            Order::one("k", "x", Post::empty("/x"))
                .toggle(true)
                .within(Duration::from_millis(100)),
        );
        // late by itself, before any answer
        wait_until(|| c.state("k") == State::Late);
        let words = Words::new(Lang::Fr);
        settle(&mut c, &words);
        // the answer past its limit does not clear what the tile said
        assert_eq!(c.state("k"), State::Late);
    }

    #[test]
    fn one_order_on_the_wire_per_key_and_only_the_latest_waits() {
        let stub = serve(|_| {
            std::thread::sleep(Duration::from_millis(150));
            (200, vec![], "{}".into())
        });
        let words = Words::new(Lang::Fr);
        let mut c = commands(&stub.origin);
        let set = |v: u8| {
            Order::one(
                "lamp:1:brightness:set",
                "Salon",
                Post::json("/l/1/brightness", &BrightnessRequest { brightness: v }),
            )
        };
        c.run(set(1));
        c.run(set(10));
        c.run(set(26));
        for _ in 0..2 {
            settle(&mut c, &words);
        }
        let bodies: Vec<String> = stub
            .asked
            .lock()
            .unwrap()
            .iter()
            .map(|a| a.body.clone())
            .collect();
        assert_eq!(
            bodies,
            [r#"{"brightness":1}"#, r#"{"brightness":26}"#],
            "in order, 10 replaced by 26"
        );
        assert_eq!(c.state("lamp:1:brightness:set"), State::Idle);
    }

    #[test]
    fn everyone_at_once_and_the_outcome_said_once() {
        let stub = serve(|line| {
            if line.contains("/2/") {
                (500, vec![], "{}".into())
            } else {
                (200, vec![], "{}".into())
            }
        });
        let words = Words::new(Lang::Fr);
        let mut c = commands(&stub.origin);
        let post = |id: &str| {
            Post::json(
                format!("/zigbee/lamps/{id}/power"),
                &PowerRequest { enabled: false },
            )
        };
        fn say(w: &Words, o: &Outcome) -> Option<Toast> {
            Some(Toast {
                text: format!("{} / {}", o.done.len(), w.s(crate::i18n::k::COMMON_RETRY)),
                warn: !o.failed.is_empty(),
            })
        }
        c.run(
            Order::many(
                "lamps:off",
                vec![
                    ("Un".into(), post("1")),
                    ("Deux".into(), post("2")),
                    ("Trois".into(), post("3")),
                ],
            )
            .within(limit::LAMP)
            .then_refresh(&["hue-lamp", "zigbee-lamp"])
            .said(say),
        );
        let (refreshed, toasts) = settle(&mut c, &words);
        assert_eq!(refreshed, ["hue-lamp", "zigbee-lamp"]);
        assert_eq!(
            toasts,
            [Toast {
                text: "2 / Réessayer".into(),
                warn: true
            }]
        );
        assert_eq!(stub.asked.lock().unwrap().len(), 3);
    }
}
