//! Who the frame is, as the web app's ScreenPairing and session say it: a screen holding its
//! cookie asks `POST /screens/me` (who it is, its cookie renewed); refused, or holding none, it shows a code
//! (`POST /screens/pairing`) and asks every `pollMs` whether an admin paired it
//! (`POST /screens/pairing/claim`, the secret kept in this thread's memory only), until the
//! code's time is over (a new one is one tap away). Paired, the house shows; a later 401 (the
//! screen removed from « Mon compte ») says so and pairs again.
//!
//! One thread does the asking; the drawing thread reads `Screen` and sends `Ask`.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use maison_api::codes::Code;
use maison_api::screens::{ClaimAnswer, ClaimRequest, Me, Pairing};

use crate::client::{Client, Failure};
use crate::paths;

/// How long before asking again after a failure (the Wi-Fi a moment away, a refusal).
const RETRY: [Duration; 4] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
];

/// What the screen shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    /// Asking who it is (its cookie held).
    Checking,
    /// The code to give from a phone, for how many minutes it was given.
    Code { code: String, minutes: u64 },
    /// The code's time is over: a new one on a tap.
    Expired,
    /// A failure, said (asked again by itself after a while).
    Failed(Failure),
    /// The house's screen.
    Paired,
    /// It was paired and the house no longer knows it: said, then a code.
    Revoked,
}

/// What the drawing thread asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// A new code (after it expired, or on a failure's « Nouveau code »).
    Again,
    /// A read said 401: the screen was removed.
    SignedOut,
    /// From the revoked notice: show a code.
    Pair,
}

pub struct Session {
    pub screen: Screen,
    rx: Receiver<Screen>,
    tx: Sender<Ask>,
}

impl Session {
    pub fn start(client: Client, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (screens, rx) = mpsc::channel();
        let (tx, asks) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("pairing".into())
            .spawn(move || run(&client, &screens, &asks, &*wake));
        if let Err(e) = spawned {
            eprintln!("the pairing could not start: {e}");
        }
        Self {
            screen: Screen::Checking,
            rx,
            tx,
        }
    }

    /// What changed; true when anything did.
    pub fn take(&mut self) -> bool {
        let mut changed = false;
        while let Ok(s) = self.rx.try_recv() {
            self.screen = s;
            changed = true;
        }
        changed
    }

    pub fn ask(&self, ask: Ask) {
        let _ = self.tx.send(ask);
    }
}

/// The asking, for the app's life.
fn run(
    client: &Client,
    screens: &Sender<Screen>,
    asks: &Receiver<Ask>,
    wake: &(dyn Fn() + Send + Sync),
) {
    let say = |s: Screen| {
        let ok = screens.send(s).is_ok();
        wake();
        ok
    };
    // whether a refusal means the screen was removed (it was paired once in this life)
    let mut was_paired = false;
    loop {
        // who it is
        if client.has_cookie() {
            let mut tries = 0;
            let me = loop {
                match client.post::<maison_api::Answer<Me>>(paths::SCREEN_ME, None::<&()>) {
                    Ok(_) => break true,
                    Err(Failure::SignedOut) => break false,
                    Err(f) => {
                        if !say(Screen::Failed(f)) {
                            return;
                        }
                        if wait(asks, RETRY[tries.min(RETRY.len() - 1)]) == Waited::Gone {
                            return;
                        }
                        tries += 1;
                    }
                }
            };
            if me {
                was_paired = true;
                if !say(Screen::Paired) {
                    return;
                }
                // the house shows until a read says 401
                loop {
                    match asks.recv() {
                        Ok(Ask::SignedOut) => break,
                        Ok(_) => {}
                        Err(_) => return,
                    }
                }
            }
            client.forget_cookie();
            if was_paired {
                if !say(Screen::Revoked) {
                    return;
                }
                loop {
                    match asks.recv() {
                        Ok(Ask::Pair | Ask::Again) => break,
                        Ok(Ask::SignedOut) => {}
                        Err(_) => return,
                    }
                }
            }
        }
        // a code, until paired
        match pair(client, &say, asks) {
            Paired::Yes => {}
            Paired::Gone => return,
        }
    }
}

enum Paired {
    Yes,
    Gone,
}

#[derive(PartialEq)]
enum Waited {
    Time,
    Asked(Ask),
    Gone,
}

fn wait(asks: &Receiver<Ask>, d: Duration) -> Waited {
    match asks.recv_timeout(d) {
        Ok(a) => Waited::Asked(a),
        Err(RecvTimeoutError::Timeout) => Waited::Time,
        Err(RecvTimeoutError::Disconnected) => Waited::Gone,
    }
}

/// Codes until one is claimed (the cookie then set by the claim's answer).
fn pair(client: &Client, say: &dyn Fn(Screen) -> bool, asks: &Receiver<Ask>) -> Paired {
    let mut tries = 0;
    loop {
        let p = match client.post::<maison_api::Answer<Pairing>>(paths::PAIRING, None::<&()>) {
            Ok(p) => p.body,
            Err(f) => {
                if !say(Screen::Failed(f)) {
                    return Paired::Gone;
                }
                // asked again by itself, or at once on « Nouveau code »
                if wait(asks, RETRY[tries.min(RETRY.len() - 1)]) == Waited::Gone {
                    return Paired::Gone;
                }
                tries += 1;
                continue;
            }
        };
        tries = 0;
        // a server's figure, bounded: never past a day
        let until = Instant::now() + Duration::from_millis(p.expires_in_ms.min(86_400_000));
        let code = Screen::Code {
            code: p.code.clone(),
            minutes: (p.expires_in_ms as f64 / 60_000.0).round() as u64,
        };
        if !say(code) {
            return Paired::Gone;
        }
        let claim = ClaimRequest { secret: p.secret };
        let every = Duration::from_millis(p.poll_ms.max(500));
        let expired = loop {
            match wait(asks, every) {
                Waited::Gone => return Paired::Gone,
                // a new code asked while one is shown: this one is dropped
                Waited::Asked(Ask::Again) => break false,
                Waited::Asked(_) | Waited::Time => {}
            }
            if Instant::now() >= until {
                break true;
            }
            match client.post::<maison_api::Answer<ClaimAnswer>>(paths::PAIRING_CLAIM, Some(&claim))
            {
                Ok(a) if a.body.paired => return Paired::Yes,
                Ok(_) => {}
                // its time is over on the server's side
                Err(Failure::Refused {
                    code: Some(Code::PairingUnknown),
                    ..
                }) => break true,
                // anything else (the Wi-Fi a moment away): asked again at the next round
                Err(f) => eprintln!("pairing claim: {f}"),
            }
        };
        if expired {
            if !say(Screen::Expired) {
                return Paired::Gone;
            }
            loop {
                match asks.recv() {
                    Ok(Ask::Again | Ask::Pair) => break,
                    Ok(Ask::SignedOut) => {}
                    Err(_) => return Paired::Gone,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::stub::serve;
    use crate::testing::{temp_dir, wait_until};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn session(origin: &str, dir: &std::path::Path) -> Session {
        let client = Client::new(origin, dir, Duration::from_secs(5));
        Session::start(client, Arc::new(|| {}))
    }

    /// A cookie kept from an earlier life, as the jar writes it (its origin first).
    fn keep_cookie(dir: &std::path::Path, origin: &str, cookie: &str) {
        std::fs::write(dir.join("screen-cookie"), format!("{origin}\n{cookie}\n")).unwrap();
    }

    /// Takes until the screen is `want`'s, or fails after five seconds.
    fn until(s: &mut Session, want: impl Fn(&Screen) -> bool) {
        wait_until(|| {
            s.take();
            want(&s.screen)
        });
    }

    const PAIRING: &str =
        r#"{"success":true,"code":"ABCD-2345","secret":"sec","expiresInMs":600000,"pollMs":500}"#;

    #[test]
    fn a_code_until_an_admin_pairs_then_the_house_until_removed() {
        let dir = temp_dir("pairing");
        let claims = Arc::new(AtomicUsize::new(0));
        let removed = Arc::new(Mutex::new(false));
        let (c, r) = (claims.clone(), removed.clone());
        let stub = serve(move |line| match line {
            l if l.starts_with("POST /api/screens/pairing/claim") => {
                if c.fetch_add(1, Ordering::SeqCst) == 0 {
                    (200, vec![], r#"{"success":true,"paired":false}"#.into())
                } else {
                    (
                        200,
                        vec!["set-cookie: maison_screen=t; Path=/".into()],
                        r#"{"success":true,"paired":true,"screen":{"name":"Salon"}}"#.into(),
                    )
                }
            }
            l if l.starts_with("POST /api/screens/pairing") => (200, vec![], PAIRING.into()),
            l if l.starts_with("POST /api/screens/me") => {
                if *r.lock().unwrap() {
                    (401, vec![], r#"{"code":"not_signed_in"}"#.into())
                } else {
                    (
                        200,
                        vec![],
                        r#"{"success":true,"screen":{"name":"Salon"}}"#.into(),
                    )
                }
            }
            _ => (404, vec![], "{}".into()),
        });
        let mut s = session(&stub.origin, &dir);
        until(
            &mut s,
            |s| matches!(s, Screen::Code { code, minutes: 10, .. } if code == "ABCD-2345"),
        );
        until(&mut s, |s| *s == Screen::Paired);
        assert_eq!(claims.load(Ordering::SeqCst), 2);
        // the secret went in the claim's body, never in an address
        let asked = stub.asked.lock().unwrap().clone();
        let claim = asked.iter().find(|a| a.line.contains("claim")).unwrap();
        assert_eq!(claim.body, r#"{"secret":"sec"}"#);
        assert!(asked.iter().all(|a| !a.line.contains("sec")));
        // removed in « Mon compte »: a read says 401, the screen says so, then a new code
        *removed.lock().unwrap() = true;
        s.ask(Ask::SignedOut);
        until(&mut s, |s| *s == Screen::Revoked);
        assert!(!dir.join("screen-cookie").exists());
        s.ask(Ask::Pair);
        until(&mut s, |s| matches!(s, Screen::Code { .. }));
    }

    #[test]
    fn a_kept_cookie_opens_the_house_and_a_refused_one_shows_a_code() {
        let dir = temp_dir("pairing-kept");
        let stub = serve(|line| match line {
            l if l.starts_with("POST /api/screens/me") => (
                200,
                vec![],
                r#"{"success":true,"screen":{"name":"Entrée"}}"#.into(),
            ),
            _ => (404, vec![], "{}".into()),
        });
        keep_cookie(&dir, &stub.origin, "maison_screen=old");
        let mut s = session(&stub.origin, &dir);
        until(&mut s, |s| *s == Screen::Paired);
        let asked = stub.asked.lock().unwrap().clone();
        assert!(
            asked[0]
                .headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case("cookie: maison_screen=old"))
        );

        let dir = temp_dir("pairing-refused");
        let stub = serve(|line| match line {
            l if l.starts_with("POST /api/screens/me") => (401, vec![], "{}".into()),
            l if l.starts_with("POST /api/screens/pairing/claim") => {
                (200, vec![], r#"{"success":true,"paired":false}"#.into())
            }
            l if l.starts_with("POST /api/screens/pairing") => (200, vec![], PAIRING.into()),
            _ => (404, vec![], "{}".into()),
        });
        keep_cookie(&dir, &stub.origin, "maison_screen=gone");
        let mut s = session(&stub.origin, &dir);
        // never paired in this life: straight to a code, no « removed » notice
        until(&mut s, |s| matches!(s, Screen::Code { .. }));
    }

    #[test]
    fn a_code_past_its_time_offers_a_new_one() {
        let dir = temp_dir("pairing-expired");
        let starts = Arc::new(AtomicUsize::new(0));
        let n = starts.clone();
        let stub = serve(move |line| match line {
            l if l.starts_with("POST /api/screens/pairing/claim") => {
                (404, vec![], r#"{"code":"pairing_unknown"}"#.into())
            }
            l if l.starts_with("POST /api/screens/pairing") => {
                n.fetch_add(1, Ordering::SeqCst);
                (200, vec![], PAIRING.into())
            }
            _ => (404, vec![], "{}".into()),
        });
        let mut s = session(&stub.origin, &dir);
        until(&mut s, |s| *s == Screen::Expired);
        s.ask(Ask::Again);
        until(&mut s, |s| matches!(s, Screen::Code { .. }));
        assert_eq!(starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn nobody_answering_is_said_and_asked_again() {
        let dir = temp_dir("pairing-offline");
        let client = Client::new("http://127.0.0.1:9", &dir, Duration::from_secs(1));
        let mut s = Session::start(client, Arc::new(|| {}));
        until(&mut s, |s| {
            matches!(s, Screen::Failed(Failure::Unreachable(_)))
        });
    }
}
