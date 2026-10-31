//! The frame's Wi-Fi, on its own screen: what `maison-wifid` says of it (one thread, over its socket:
//! the app stays unprivileged), the networks in range, one chosen, its password typed, joined. It
//! opens by itself on a frame with no network, or that lost its own (`Status::lost`: 10 minutes), and
//! leaves by itself when one is back; a door or Maison's silence offer it by hand. The socket read
//! every `STATUS_EVERY` draws nothing unless the answer changed. Nothing typed is kept past a page's
//! time, nor said in any log.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use maison_frame_tools::wifi::{
    Answer, Join, Network, Refusal, Request, Status, Why, password_fits,
};

use crate::i18n::{Words, k};
use crate::ui::icon::{self, Icon};
use crate::ui::keyboard::{Key, Keyboard};
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Password, Screen, Tile, WifiAct};

/// How often the service is asked how things stand.
const STATUS_EVERY: Duration = Duration::from_secs(15);
/// While a scan or a join runs.
const QUICK: Duration = Duration::from_secs(1);
/// A scan's results are read this many times, `QUICK` apart: they come within seconds.
const SCAN_READS: u32 = 6;
/// The wait for an answer: the service's own waits are 3 s a command, two a status.
const ANSWER_WAIT: Duration = Duration::from_secs(8);
/// A list older than this is read again when a finger comes back to it.
const RESCAN_AFTER: Duration = Duration::from_secs(30);
/// wpa_supplicant's limit (`password_fits`).
const PASSWORD_MAX: usize = 63;
/// dBm: what is said of a signal, strongest first (the usual thresholds of a phone's bars).
const SIGNAL: [(i32, &str); 3] = [
    (-55, k::WIFI_SIGNAL_STRONG),
    (-67, k::WIFI_SIGNAL_GOOD),
    (-75, k::WIFI_SIGNAL_FAIR),
];

/// What the service says.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Link {
    /// No service: a simulation's, or it is starting.
    #[default]
    Absent,
    /// The service is there, wpa_supplicant is not.
    Silent,
    Known(Status),
}

enum Order {
    Scan,
    Join {
        ssid: String,
        password: Option<String>,
    },
}

enum Said {
    Link(Link),
    Networks(Vec<Network>),
    /// A scan's reads are over.
    Scanned,
    Refused(Refusal),
}

/// One request over its own connection; `None`: no service to ask.
fn ask(socket: &Path, request: &Request) -> Option<Answer> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(ANSWER_WAIT)).ok()?;
    stream.set_write_timeout(Some(ANSWER_WAIT)).ok()?;
    let mut line = serde_json::to_string(request).ok()?;
    line.push('\n');
    stream.write_all(line.as_bytes()).ok()?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).ok()?;
    serde_json::from_str(&answer).ok()
}

fn read_link(socket: &Path) -> Link {
    match ask(socket, &Request::Status {}) {
        Some(Answer::Status(s)) => Link::Known(s),
        Some(_) => Link::Silent,
        None => Link::Absent,
    }
}

/// The asking, for the app's life: says only what changed, and only then wakes the loop.
fn run(
    socket: &Path,
    orders: &Receiver<Order>,
    says: &Sender<Said>,
    wake: &(dyn Fn() + Send + Sync),
) {
    let say = |said| {
        let _ = says.send(said);
        wake();
    };
    let (mut link, mut list) = (Link::Absent, None);
    let mut reads = 0;
    loop {
        let seen = read_link(socket);
        if seen != link {
            link = seen;
            say(Said::Link(link.clone()));
        }
        if reads > 0 {
            reads -= 1;
            if let Some(Answer::Networks(found)) = ask(socket, &Request::List {})
                && list.as_ref() != Some(&found)
            {
                list = Some(found.clone());
                say(Said::Networks(found));
            }
            if reads == 0 {
                say(Said::Scanned);
            }
        }
        let joining = matches!(&link, Link::Known(s) if matches!(s.join, Join::Joining { .. }));
        let wait = if reads > 0 || joining {
            QUICK
        } else {
            STATUS_EVERY
        };
        let request = match orders.recv_timeout(wait) {
            Ok(Order::Scan) => {
                reads = SCAN_READS;
                Request::Scan {}
            }
            Ok(Order::Join { ssid, password }) => Request::Connect { ssid, password },
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        match ask(socket, &request) {
            Some(Answer::Refused(why)) => say(Said::Refused(why)),
            Some(_) => {}
            None => say(Said::Refused(Refusal::Silent)),
        }
    }
}

/// A password being typed.
#[derive(Debug, Clone)]
struct Entry {
    ssid: String,
    keyboard: Keyboard,
    /// Shown as typed.
    plain: bool,
    /// « Rejoindre » found it could not go.
    refused: bool,
}

#[derive(Debug, Clone)]
enum Page {
    Closed,
    List,
    Entry(Box<Entry>),
}

pub struct Wifi {
    orders: Sender<Order>,
    said: Receiver<Said>,
    link: Link,
    /// None until a first list came.
    networks: Option<Vec<Network>>,
    scanning: bool,
    scanned: Option<Instant>,
    page: Page,
    /// A hand opened it, or joined from it: it stays until its way back or a page's time.
    by_hand: bool,
    /// The network tried from this page: its name, whether it wants a password.
    tried: Option<(String, bool)>,
    /// The service refused the last order.
    refused: bool,
}

impl Wifi {
    pub fn new(socket: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (orders, asks) = mpsc::channel();
        let (says, said) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("wifi".into())
            .spawn(move || run(&socket, &asks, &says, &*wake));
        if let Err(e) = spawned {
            eprintln!("the Wi-Fi's thread could not start: {e}");
        }
        Self {
            orders,
            said,
            link: Link::Absent,
            networks: None,
            scanning: false,
            scanned: None,
            page: Page::Closed,
            by_hand: false,
            tried: None,
            refused: false,
        }
    }

    fn status(&self) -> Option<&Status> {
        match &self.link {
            Link::Known(s) => Some(s),
            _ => None,
        }
    }

    /// There is a Wi-Fi to speak of: a service, the frame not its own access point.
    pub fn available(&self) -> bool {
        match &self.link {
            Link::Absent => false,
            Link::Silent => true,
            Link::Known(s) => !s.hotspot,
        }
    }

    /// It takes the screen whoever is there: no network, and a list to choose from.
    fn forced(&self) -> bool {
        self.status()
            .is_some_and(|s| !s.hotspot && s.connected.is_none() && (!s.configured || s.lost))
    }

    pub fn shown(&self) -> bool {
        self.available() && !matches!(self.page, Page::Closed)
    }

    /// It holds the screen for a hand that may have left: a page's time ends it.
    pub fn opened(&self) -> bool {
        self.shown() && (self.by_hand || matches!(self.page, Page::Entry(_)))
    }

    /// What the thread said since; true when the screen must be drawn again.
    pub fn take(&mut self, now: Instant) -> bool {
        let mut changed = false;
        while let Ok(said) = self.said.try_recv() {
            changed = true;
            match said {
                Said::Link(link) => self.link = link,
                Said::Networks(list) => self.networks = Some(list),
                Said::Scanned => self.scanning = false,
                Said::Refused(Refusal::Busy) => {}
                Said::Refused(_) => {
                    self.refused = true;
                    self.scanning = false;
                }
            }
        }
        if !changed {
            return false;
        }
        match self.page {
            Page::Closed if self.forced() => self.open(now),
            // the network came back: nobody chose this page
            Page::List if !self.forced() && !self.by_hand => self.page = Page::Closed,
            _ => {}
        }
        true
    }

    fn open(&mut self, now: Instant) {
        self.page = Page::List;
        self.scan(now);
    }

    fn scan(&mut self, now: Instant) {
        self.scanning = true;
        self.scanned = Some(now);
        self.refused = false;
        let _ = self.orders.send(Order::Scan);
    }

    fn join(&mut self, ssid: String, password: Option<String>) {
        self.tried = Some((ssid.clone(), password.is_some()));
        self.by_hand = true;
        self.refused = false;
        self.page = Page::List;
        let _ = self.orders.send(Order::Join { ssid, password });
    }

    pub fn act(&mut self, act: &WifiAct, now: Instant) {
        match act {
            WifiAct::Open => {
                self.by_hand = true;
                self.open(now);
            }
            WifiAct::Back => {
                if matches!(self.page, Page::Entry(_)) {
                    self.page = Page::List;
                } else {
                    self.leave(now);
                }
            }
            WifiAct::Rescan => self.scan(now),
            WifiAct::Choose { ssid, locked } => self.choose(ssid.clone(), *locked),
            WifiAct::Retry => {
                if let Some((ssid, locked)) = self.tried.clone() {
                    self.choose(ssid, locked);
                }
            }
            WifiAct::Show => {
                if let Page::Entry(e) = &mut self.page {
                    e.plain = !e.plain;
                }
            }
            WifiAct::Join => {
                let Page::Entry(e) = &mut self.page else {
                    return;
                };
                let password = e.keyboard.text().to_owned();
                if password_fits(&password) {
                    let ssid = e.ssid.clone();
                    self.join(ssid, Some(password));
                } else {
                    e.refused = true;
                }
            }
        }
    }

    fn choose(&mut self, ssid: String, locked: bool) {
        self.by_hand = true;
        if locked {
            self.page = Page::Entry(Box::new(Entry {
                ssid,
                keyboard: Keyboard::new(PASSWORD_MAX),
                plain: false,
                refused: false,
            }));
        } else {
            self.join(ssid, None);
        }
    }

    pub fn key(&mut self, key: Key) {
        if let Page::Entry(e) = &mut self.page {
            e.keyboard.press(key);
            e.refused = false;
        }
    }

    /// A simulation's fingers.
    pub fn type_text(&mut self, text: &str) {
        if let Page::Entry(e) = &mut self.page {
            e.keyboard.set(text);
        }
    }

    /// A finger came back: a list left a while ago is read again.
    pub fn touched(&mut self, now: Instant) {
        let old = self
            .scanned
            .is_none_or(|t| now.saturating_duration_since(t) >= RESCAN_AFTER);
        if matches!(self.page, Page::List) && self.shown() && old {
            self.scan(now);
        }
    }

    /// Nobody there, or the way back: what was typed goes, the list stays if nothing else can show.
    pub fn leave(&mut self, now: Instant) {
        self.by_hand = false;
        self.tried = None;
        self.refused = false;
        self.page = Page::Closed;
        if self.forced() {
            self.open(now);
        }
    }

    /// The network's state, in words: what the page's head row says.
    fn state(&self) -> (Option<String>, &'static str, bool) {
        let tried = self.tried.as_ref().map(|(ssid, _)| ssid.as_str());
        let Some(s) = self.status() else {
            return (None, k::WIFI_SILENT, true);
        };
        if self.refused {
            return (tried.map(Into::into), k::WIFI_SILENT, true);
        }
        match &s.join {
            Join::Joining { ssid } => return (Some(ssid.clone()), k::WIFI_JOINING, false),
            Join::Failed { ssid, why } if tried == Some(ssid) => {
                let said = match why {
                    Why::Password => k::WIFI_FAILED_PASSWORD,
                    Why::NoAnswer => k::WIFI_FAILED_NO_ANSWER,
                    Why::NotSaved => k::WIFI_FAILED_NOT_SAVED,
                };
                return (Some(ssid.clone()), said, true);
            }
            _ => {}
        }
        match &s.connected {
            Some(ssid) => (Some(ssid.clone()), k::WIFI_CONNECTED, false),
            None if !s.configured => (None, k::WIFI_NOT_CONFIGURED, true),
            None if s.lost => (None, k::WIFI_LOST, true),
            None => (None, k::WIFI_NOT_CONNECTED, false),
        }
    }

    /// What it shows, if it is shown.
    pub fn screen(&self, w: &Words) -> Option<Screen> {
        if !self.shown() {
            return None;
        }
        Some(match &self.page {
            Page::Entry(e) => Screen::Password(Box::new(password(w, e))),
            _ => self.list(w),
        })
    }

    fn list(&self, w: &Words) -> Screen {
        let (name, said, warn) = self.state();
        let word = w.s(said);
        let failed = warn && self.tried.is_some();
        let retry = Button::new(
            Some(icon::REFRESH_CW),
            w.s(k::COMMON_RETRY),
            Act::Wifi(WifiAct::Retry),
        );
        let glyph = if warn { icon::WIFI_OFF } else { icon::WIFI };
        let on = self.status().is_some_and(|s| s.connected.is_some());
        let lead = Lead::Glyph { icon: glyph, on };
        let frame = Tile::new(
            name.unwrap_or_else(|| w.s(k::WIFI_NO_NETWORK)),
            word.clone(),
            lead,
        )
        .warn(warn)
        .parts(if failed {
            vec![Part::Row(vec![retry])]
        } else {
            Vec::new()
        });
        let rescan = Button::new(
            Some(icon::REFRESH_CW),
            w.s(if self.scanning {
                k::PLACE_SEARCHING
            } else {
                k::WIFI_RESCAN
            }),
            Act::Wifi(WifiAct::Rescan),
        )
        .unavailable(self.scanning);
        let body = match (&self.link, &self.networks) {
            (Link::Silent, _) => Body::Failed {
                text: w.s(k::WIFI_SILENT),
                retry: Button::new(None, w.s(k::COMMON_RETRY), Act::Wifi(WifiAct::Rescan)),
            },
            (_, None) => Body::Loading,
            (_, Some(list)) if list.is_empty() => Body::Empty {
                text: w.s(k::WIFI_NONE),
                hint: w.s(k::WIFI_NONE_HINT),
            },
            (_, Some(list)) => Body::Tiles(list.iter().map(|n| tile(w, n)).collect()),
        };
        Screen::Wifi {
            title: w.s(k::WIFI_TITLE),
            word,
            back: (!self.forced()).then(|| {
                Box::new(Button::new(
                    Some(icon::ARROW_LEFT),
                    w.s(k::KEY_BACK),
                    Act::Wifi(WifiAct::Back),
                ))
            }),
            groups: vec![
                Group::new(w.s(k::WIFI_FRAME), Body::Tiles(vec![frame])),
                Group::new(w.s(k::WIFI_NETWORKS), body).actions(vec![rescan]),
            ],
        }
    }

    /// The way in, for a door or the band of Maison's silence.
    pub fn entry(&self, w: &Words) -> Option<Button> {
        self.available().then(|| {
            Button::new(
                Some(icon::WIFI),
                w.s(k::WIFI_ENTRY),
                Act::Wifi(WifiAct::Open),
            )
        })
    }
}

fn signal(w: &Words, dbm: i32) -> String {
    let said = SIGNAL
        .iter()
        .find(|(from, _)| dbm >= *from)
        .map_or(k::WIFI_SIGNAL_WEAK, |(_, key)| key);
    w.s(said)
}

/// One network of the list; the current one marked, nothing to do there.
fn tile(w: &Words, n: &Network) -> Tile {
    let (glyph, said): (Icon, _) = match (n.current, n.locked) {
        (true, _) => (icon::WIFI, k::WIFI_CONNECTED),
        (_, true) => (icon::LOCK, k::WIFI_LOCKED),
        (_, false) => (icon::WIFI, k::WIFI_OPEN),
    };
    let lead = Lead::Glyph {
        icon: glyph,
        on: n.current,
    };
    let choose = WifiAct::Choose {
        ssid: n.ssid.clone(),
        locked: n.locked,
    };
    Tile::new(n.ssid.clone(), w.s(said), lead)
        .fact(Some(signal(w, n.signal)))
        .open((!n.current).then_some(Act::Wifi(choose)))
}

fn password(w: &Words, e: &Entry) -> Password {
    let typed = e.keyboard.text();
    let field = if e.plain {
        typed.to_owned()
    } else {
        "•".repeat(typed.chars().count())
    };
    Password {
        title: e.ssid.clone(),
        back: Box::new(Button::new(
            Some(icon::ARROW_LEFT),
            w.s(k::KEY_BACK),
            Act::Wifi(WifiAct::Back),
        )),
        label: w.s(k::WIFI_PASSWORD),
        field,
        refused: e.refused.then(|| w.s(k::FRAME_SETUP_PAGE_BAD_PASSWORD)),
        show: Box::new(Button::new(
            None,
            w.s(if e.plain { k::WIFI_HIDE } else { k::WIFI_SHOW }),
            Act::Wifi(WifiAct::Show),
        )),
        join: Box::new(
            Button::new(None, w.s(k::WIFI_JOIN), Act::Wifi(WifiAct::Join)).primary(true),
        ),
        keyboard: e.keyboard.clone(),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::testing::wait_until;
    use std::os::unix::net::UnixListener;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fake service: answers from `status` and `list`, keeps what it was asked.
    pub struct Fake {
        pub socket: PathBuf,
        pub status: Arc<Mutex<Status>>,
        pub list: Arc<Mutex<Vec<Network>>>,
        pub asked: Arc<Mutex<Vec<Request>>>,
    }

    pub fn fake() -> Fake {
        static N: AtomicUsize = AtomicUsize::new(0);
        // a socket's path has a limit: never the temporary directory's
        let dir = PathBuf::from(format!(
            "/tmp/mfu-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("s");
        let listener = UnixListener::bind(&socket).unwrap();
        let fake = Fake {
            socket,
            status: Arc::default(),
            list: Arc::default(),
            asked: Arc::default(),
        };
        let (status, list, asked) = (fake.status.clone(), fake.list.clone(), fake.asked.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                let mut reader = BufReader::new(&stream);
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let request: Request = serde_json::from_str(&line).unwrap();
                let answer = match &request {
                    Request::Status {} => Answer::Status(status.lock().unwrap().clone()),
                    Request::List {} => Answer::Networks(list.lock().unwrap().clone()),
                    _ => Answer::Done,
                };
                asked.lock().unwrap().push(request);
                let mut out = serde_json::to_string(&answer).unwrap();
                out.push('\n');
                let _ = (&stream).write_all(out.as_bytes());
            }
        });
        fake
    }

    fn network(ssid: &str, signal: i32, locked: bool, current: bool) -> Network {
        Network {
            ssid: ssid.into(),
            signal,
            locked,
            current,
        }
    }

    impl Fake {
        fn wifi(&self) -> Wifi {
            Wifi::new(self.socket.clone(), Arc::new(|| {}))
        }
    }

    /// `wifi` once it heard `link`.
    fn heard(wifi: &mut Wifi, link: Link, now: Instant) {
        wifi.link = link;
        // what `take` does with a word from the thread
        let (tx, rx) = mpsc::channel();
        tx.send(Said::Link(wifi.link.clone())).unwrap();
        wifi.said = rx;
        wifi.take(now);
    }

    fn known(status: Status) -> Link {
        Link::Known(status)
    }

    fn lost() -> Status {
        Status {
            configured: true,
            lost: true,
            ..Status::default()
        }
    }

    fn fresh() -> Status {
        Status::default()
    }

    fn on(ssid: &str) -> Status {
        Status {
            connected: Some(ssid.into()),
            configured: true,
            ..Status::default()
        }
    }

    fn words() -> Words {
        Words::new(Lang::Fr)
    }

    fn word_of(wifi: &Wifi) -> String {
        match wifi.screen(&words()) {
            Some(Screen::Wifi { word, .. }) => word,
            other => panic!("the list: {other:?}"),
        }
    }

    fn absent() -> Wifi {
        Wifi::new(PathBuf::from("/nowhere/socket"), Arc::new(|| {}))
    }

    #[test]
    fn with_no_service_there_is_nothing_to_show_or_offer() {
        let mut wifi = absent();
        let now = Instant::now();
        wifi.take(now);
        assert!(!wifi.available() && !wifi.shown() && !wifi.opened());
        assert!(wifi.entry(&words()).is_none());
        assert!(wifi.screen(&words()).is_none());
    }

    #[test]
    fn it_opens_by_itself_with_no_network_or_a_lost_one_and_leaves_when_one_is_back() {
        let t0 = Instant::now();
        for (status, said) in [(fresh(), k::WIFI_NOT_CONFIGURED), (lost(), k::WIFI_LOST)] {
            let mut wifi = absent();
            heard(&mut wifi, known(status), t0);
            assert!(wifi.shown() && !wifi.opened());
            assert_eq!(word_of(&wifi), words().s(said));
            let Some(Screen::Wifi { back, .. }) = wifi.screen(&words()) else {
                panic!("the list")
            };
            assert!(back.is_none(), "nowhere to go back to");
            heard(&mut wifi, known(on("Ma Box")), t0);
            assert!(!wifi.shown(), "back by itself");
        }
    }

    #[test]
    fn a_frame_on_its_network_or_its_own_access_point_opens_nothing() {
        let t0 = Instant::now();
        let mut wifi = absent();
        heard(&mut wifi, known(on("Ma Box")), t0);
        assert!(wifi.available() && !wifi.shown());
        let hotspot = Status {
            hotspot: true,
            ..fresh()
        };
        heard(&mut wifi, known(hotspot), t0);
        assert!(!wifi.available() && !wifi.shown());
        assert!(wifi.entry(&words()).is_none());
    }

    #[test]
    fn by_hand_it_stays_until_its_way_back_or_a_pages_time() {
        let t0 = Instant::now();
        let mut wifi = absent();
        heard(&mut wifi, known(on("Ma Box")), t0);
        assert!(wifi.entry(&words()).is_some());
        wifi.act(&WifiAct::Open, t0);
        assert!(wifi.shown() && wifi.opened());
        assert_eq!(word_of(&wifi), words().s(k::WIFI_CONNECTED));
        heard(&mut wifi, known(on("Ma Box")), t0);
        assert!(wifi.shown(), "a status leaves it there");
        wifi.act(&WifiAct::Back, t0);
        assert!(!wifi.shown());
        wifi.act(&WifiAct::Open, t0);
        wifi.leave(t0);
        assert!(!wifi.shown() && !wifi.opened());
    }

    #[test]
    fn what_was_typed_goes_with_the_pages_time_and_a_lost_frame_keeps_its_list() {
        let t0 = Instant::now();
        let mut wifi = absent();
        heard(&mut wifi, known(lost()), t0);
        wifi.act(&ssid("Ma Box", true), t0);
        wifi.type_text("sel de mer");
        assert!(wifi.opened());
        wifi.leave(t0);
        assert!(wifi.shown() && !wifi.opened(), "the list, nobody's");
        assert!(matches!(wifi.page, Page::List));
        wifi.act(&ssid("Ma Box", true), t0);
        let Some(Screen::Password(p)) = wifi.screen(&words()) else {
            panic!("the password")
        };
        assert_eq!(p.field, "");
    }

    fn ssid(name: &str, locked: bool) -> WifiAct {
        WifiAct::Choose {
            ssid: name.into(),
            locked,
        }
    }

    #[test]
    fn a_password_is_hidden_until_asked_and_stays_out_of_the_screen() {
        let t0 = Instant::now();
        let mut wifi = absent();
        heard(&mut wifi, known(lost()), t0);
        wifi.act(&ssid("Ma Box", true), t0);
        wifi.key(Key::Char('a'));
        wifi.key(Key::Char('z'));
        let field = |wifi: &Wifi| match wifi.screen(&words()) {
            Some(Screen::Password(p)) => (p.field, p.show.label),
            other => panic!("{other:?}"),
        };
        assert_eq!(field(&wifi), ("••".into(), words().s(k::WIFI_SHOW)));
        wifi.act(&WifiAct::Show, t0);
        assert_eq!(field(&wifi), ("az".into(), words().s(k::WIFI_HIDE)));
        wifi.act(&WifiAct::Back, t0);
        assert!(matches!(wifi.page, Page::List), "back to the list");
    }

    #[test]
    fn a_password_that_cannot_go_is_said_and_never_sent() {
        let t0 = Instant::now();
        let f = fake();
        let mut wifi = f.wifi();
        heard(&mut wifi, known(lost()), t0);
        wifi.act(&ssid("Ma Box", true), t0);
        for typed in ["court", "avec\"guillemet", "anti\\slash1"] {
            wifi.type_text(typed);
            wifi.act(&WifiAct::Join, t0);
            let Some(Screen::Password(p)) = wifi.screen(&words()) else {
                panic!("still the password")
            };
            assert_eq!(p.refused, Some(words().s(k::FRAME_SETUP_PAGE_BAD_PASSWORD)));
        }
        wifi.key(Key::Back);
        let Some(Screen::Password(p)) = wifi.screen(&words()) else {
            panic!("still the password")
        };
        assert_eq!(p.refused, None, "a key clears it");
        let sent = f.asked.lock().unwrap();
        assert!(!sent.iter().any(|r| matches!(r, Request::Connect { .. })));
    }

    #[test]
    fn a_network_is_chosen_scanned_joined_refused_and_tried_again() {
        let f = fake();
        *f.status.lock().unwrap() = lost();
        *f.list.lock().unwrap() = vec![
            network("Ma Box", -48, true, false),
            network("Café", -80, false, false),
        ];
        let mut wifi = f.wifi();
        let t0 = Instant::now();
        // heard, opened, and the list read without any wait of `STATUS_EVERY`
        wait_until(|| {
            wifi.take(t0);
            wifi.shown()
        });
        wait_until(|| {
            wifi.take(t0);
            wifi.networks.is_some()
        });
        let names = |wifi: &Wifi| match wifi.screen(&words()) {
            Some(Screen::Wifi { groups, .. }) => match &groups[1].body {
                Body::Tiles(tiles) => tiles
                    .iter()
                    .map(|t| (t.name.clone(), t.state.clone(), t.fact.clone().unwrap()))
                    .collect::<Vec<_>>(),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        };
        let w = words();
        assert_eq!(
            names(&wifi),
            [
                (
                    "Ma Box".into(),
                    w.s(k::WIFI_LOCKED),
                    w.s(k::WIFI_SIGNAL_STRONG)
                ),
                ("Café".into(), w.s(k::WIFI_OPEN), w.s(k::WIFI_SIGNAL_WEAK)),
            ]
        );
        wifi.act(&ssid("Ma Box", true), t0);
        wifi.type_text("sel de mer");
        wifi.act(&WifiAct::Join, t0);
        wait_until(|| {
            f.asked.lock().unwrap().contains(&Request::Connect {
                ssid: "Ma Box".into(),
                password: Some("sel de mer".into()),
            })
        });
        assert!(
            matches!(wifi.page, Page::List),
            "back on the list to see it"
        );

        let refused = Join::Failed {
            ssid: "Ma Box".into(),
            why: Why::Password,
        };
        f.status.lock().unwrap().join = Join::Joining {
            ssid: "Ma Box".into(),
        };
        wifi.act(&WifiAct::Rescan, t0);
        wait_until(|| {
            wifi.take(t0);
            word_of(&wifi) == w.s(k::WIFI_JOINING)
        });
        f.status.lock().unwrap().join = refused;
        wait_until(|| {
            wifi.take(t0);
            word_of(&wifi) == w.s(k::WIFI_FAILED_PASSWORD)
        });
        wifi.act(&WifiAct::Retry, t0);
        assert!(matches!(wifi.page, Page::Entry(_)), "the password again");
    }

    #[test]
    fn an_open_network_is_joined_at_once_and_tried_again_the_same_way() {
        let f = fake();
        let mut wifi = f.wifi();
        let t0 = Instant::now();
        heard(&mut wifi, known(lost()), t0);
        wifi.act(&ssid("Libre", false), t0);
        assert!(matches!(wifi.page, Page::List));
        let joined = Request::Connect {
            ssid: "Libre".into(),
            password: None,
        };
        wait_until(|| f.asked.lock().unwrap().contains(&joined));
        wifi.act(&WifiAct::Retry, t0);
        wait_until(|| {
            f.asked
                .lock()
                .unwrap()
                .iter()
                .filter(|r| **r == joined)
                .count()
                == 2
        });
    }

    #[test]
    fn joined_the_word_stays_for_whoever_did_it() {
        let t0 = Instant::now();
        let mut wifi = absent();
        heard(&mut wifi, known(lost()), t0);
        wifi.act(&ssid("Libre", false), t0);
        let joined = Status {
            join: Join::Joined {
                ssid: "Libre".into(),
            },
            ..on("Libre")
        };
        heard(&mut wifi, known(joined), t0);
        assert!(wifi.shown(), "not gone from under the finger that joined");
        assert_eq!(word_of(&wifi), words().s(k::WIFI_CONNECTED));
        let Some(Screen::Wifi { back, .. }) = wifi.screen(&words()) else {
            panic!("the list")
        };
        assert!(back.is_some(), "and a way out, now");
    }

    #[test]
    fn someone_elses_failure_is_not_said() {
        let t0 = Instant::now();
        let mut wifi = absent();
        let old = Status {
            join: Join::Failed {
                ssid: "Hier".into(),
                why: Why::NoAnswer,
            },
            ..on("Ma Box")
        };
        heard(&mut wifi, known(old), t0);
        wifi.act(&WifiAct::Open, t0);
        assert_eq!(word_of(&wifi), words().s(k::WIFI_CONNECTED));
    }

    #[test]
    fn a_silent_wifi_is_said_with_a_way_to_ask_again() {
        let t0 = Instant::now();
        let mut wifi = absent();
        heard(&mut wifi, Link::Silent, t0);
        wifi.act(&WifiAct::Open, t0);
        let Some(Screen::Wifi { groups, word, .. }) = wifi.screen(&words()) else {
            panic!("the list")
        };
        assert_eq!(word, words().s(k::WIFI_SILENT));
        assert!(matches!(groups[1].body, Body::Failed { .. }));
    }

    #[test]
    fn nothing_to_list_is_said_as_such() {
        let t0 = Instant::now();
        let mut wifi = absent();
        heard(&mut wifi, known(lost()), t0);
        let body = |wifi: &Wifi| match wifi.screen(&words()) {
            Some(Screen::Wifi { mut groups, .. }) => groups.remove(1).body,
            other => panic!("{other:?}"),
        };
        assert!(matches!(body(&wifi), Body::Loading));
        wifi.networks = Some(Vec::new());
        assert!(matches!(body(&wifi), Body::Empty { .. }));
    }

    #[test]
    fn a_list_left_a_while_is_read_again_when_a_finger_returns() {
        let f = fake();
        let mut wifi = f.wifi();
        let t0 = Instant::now();
        heard(&mut wifi, known(lost()), t0);
        let scans = || {
            let asked = f.asked.lock().unwrap();
            asked.iter().filter(|r| **r == Request::Scan {}).count()
        };
        wait_until(|| scans() == 1);
        wifi.touched(t0 + RESCAN_AFTER / 2);
        wifi.touched(t0 + RESCAN_AFTER);
        wait_until(|| scans() == 2);
        // a password being typed is left alone
        wifi.act(&ssid("x", true), t0);
        wifi.touched(t0 + RESCAN_AFTER * 3);
        assert_eq!(scans(), 2);
    }

    #[test]
    fn the_signal_is_said_in_words() {
        let w = words();
        for (dbm, said) in [
            (-40, k::WIFI_SIGNAL_STRONG),
            (-60, k::WIFI_SIGNAL_GOOD),
            (-70, k::WIFI_SIGNAL_FAIR),
            (-90, k::WIFI_SIGNAL_WEAK),
        ] {
            assert_eq!(signal(&w, dbm), w.s(said));
        }
    }

    #[test]
    fn the_current_network_is_marked_and_leads_nowhere() {
        let t = tile(&words(), &network("Ma Box", -40, true, true));
        assert_eq!(t.state, words().s(k::WIFI_CONNECTED));
        assert!(t.open.is_none());
        assert!(
            tile(&words(), &network("x", -40, true, false))
                .open
                .is_some()
        );
    }
}
