//! What the interface may ask of the frame's Wi-Fi, as root, over a socket of group `kiosk`: the
//! networks in range, one to join, how that went; nothing else (`maison_frame_tools::wifi`). What
//! crosses the socket is checked here: it is the boundary between `kiosk` and root.
//!
//! A network is joined live, wpa_supplicant's others left aside, and written to `wifi.conf` only once
//! associated: a wrong password loses nothing, the network before is taken back.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use maison_frame_tools::hex;
use maison_frame_tools::wifi::{
    Answer, CONF, Join, MAX_REQUEST, Network, Refusal, Request, SOCKET, Status, Why, network,
    password_fits, private, ssid_fits,
};
use maison_frame_tools::wpa::{Wpa, associated, known, networks};

const CTRL: &str = "/run/wpa_supplicant/wlan0";
/// `maison-wifi-watch`: no network for 10 minutes.
const LOST: &str = "/run/maison-lost";
/// `maison-hotspot`: the frame is its own Wi-Fi.
const HOTSPOT: &str = "/run/maison-hotspot";
/// A client that says nothing is let go.
const CLIENT_WAIT: Duration = Duration::from_secs(2);
const WPA_WAIT: Duration = Duration::from_secs(3);
/// How long a network has to take a frame, and how often it is looked at meanwhile.
const JOIN_LIMIT: Duration = Duration::from_secs(40);
const JOIN_STEP: Duration = Duration::from_secs(1);

/// wpa_supplicant did not answer: said in the log, refused to the client.
fn silent(e: std::io::Error) -> Refusal {
    eprintln!("wpa_supplicant: {e}");
    Refusal::Silent
}

#[derive(Clone)]
struct Service {
    wpa: Wpa,
    conf: PathBuf,
    lost: PathBuf,
    hotspot: PathBuf,
    limit: Duration,
    step: Duration,
    join: Arc<Mutex<Join>>,
}

impl Service {
    fn hotspot_on(&self) -> bool {
        self.hotspot.exists()
    }

    /// The join's state, whatever became of a thread that held it.
    fn join(&self) -> MutexGuard<'_, Join> {
        self.join.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn status(&self) -> Result<Status, Refusal> {
        let hotspot = self.hotspot_on();
        let status = self.wpa.ask("STATUS").map_err(silent)?;
        let list = self.wpa.ask("LIST_NETWORKS").map_err(silent)?;
        let connected = associated(&status)
            .map(|(_, ssid)| ssid)
            .filter(|_| !hotspot);
        Ok(Status {
            lost: self.lost.exists() && connected.is_none(),
            connected,
            configured: !known(&list).is_empty(),
            hotspot,
            join: self.join().clone(),
        })
    }

    fn scan(&self) -> Result<(), Refusal> {
        if self.hotspot_on() {
            return Err(Refusal::Hotspot);
        }
        match self.wpa.ask("SCAN") {
            Ok(_) => Ok(()),
            // one already runs: its results are what was asked
            Err(e) if e.to_string().contains("FAIL-BUSY") => Ok(()),
            Err(e) => Err(silent(e)),
        }
    }

    fn list(&self) -> Result<Vec<Network>, Refusal> {
        let status = self.wpa.ask("STATUS").map_err(silent)?;
        let current = associated(&status).map(|(_, ssid)| ssid);
        let results = self.wpa.ask("SCAN_RESULTS").map_err(silent)?;
        Ok(networks(&results, current.as_deref()))
    }

    /// The network added and selected; `follow` takes it from there.
    fn select(&self, ssid: &str, password: Option<&str>) -> std::io::Result<u32> {
        let id: u32 = self
            .wpa
            .ask("ADD_NETWORK")?
            .trim()
            .parse()
            .map_err(std::io::Error::other)?;
        let set = |what: String| self.wpa.ask(&format!("SET_NETWORK {id} {what}"));
        let selected = set(format!("ssid {}", hex(ssid.as_bytes())))
            .and_then(|_| match password {
                Some(p) => set(format!("psk \"{p}\"")),
                None => set("key_mgmt NONE".into()),
            })
            .and_then(|_| self.wpa.ask(&format!("SELECT_NETWORK {id}")));
        if let Err(e) = selected {
            let _ = self.wpa.ask(&format!("REMOVE_NETWORK {id}"));
            return Err(e);
        }
        Ok(id)
    }

    /// Takes a checked request to join: only one at a time.
    fn connect(&self, ssid: String, password: Option<String>) -> Result<(), Refusal> {
        if !ssid_fits(&ssid) {
            return Err(Refusal::BadSsid);
        }
        if password.as_deref().is_some_and(|p| !password_fits(p)) {
            return Err(Refusal::BadPassword);
        }
        if self.hotspot_on() {
            return Err(Refusal::Hotspot);
        }
        {
            let mut join = self.join();
            if matches!(*join, Join::Joining { .. }) {
                return Err(Refusal::Busy);
            }
            *join = Join::Joining { ssid: ssid.clone() };
        }
        match self.select(&ssid, password.as_deref()) {
            Ok(id) => {
                let service = self.clone();
                std::thread::spawn(move || {
                    let outcome = service.follow(id, &ssid, password.as_deref());
                    *service.join() = outcome;
                });
                Ok(())
            }
            Err(e) => {
                *self.join() = Join::Idle;
                Err(silent(e))
            }
        }
    }

    /// What became of the network `id`: kept and written, or forgotten and the others taken back.
    fn follow(&self, id: u32, ssid: &str, password: Option<&str>) -> Join {
        let failed = |why| {
            eprintln!("{ssid} not joined: {why:?}");
            let _ = self.wpa.ask(&format!("REMOVE_NETWORK {id}"));
            let _ = self.wpa.ask("ENABLE_NETWORK all");
            Join::Failed {
                ssid: ssid.into(),
                why,
            }
        };
        let started = Instant::now();
        while started.elapsed() < self.limit {
            let status = self.wpa.ask("STATUS").unwrap_or_default();
            if associated(&status).is_some_and(|(on, _)| on == id) {
                if let Err(e) = private(&self.conf, &network(ssid, password)) {
                    eprintln!("{}: {e}", self.conf.display());
                    return failed(Why::NotSaved);
                }
                self.forget_others(id);
                return Join::Joined { ssid: ssid.into() };
            }
            let list = self.wpa.ask("LIST_NETWORKS").unwrap_or_default();
            let refused = known(&list)
                .iter()
                .any(|(n, flags)| *n == id && flags.contains("TEMP-DISABLED"));
            if refused && password.is_some() {
                return failed(Why::Password);
            }
            std::thread::sleep(self.step);
        }
        failed(Why::NoAnswer)
    }

    /// wpa_supplicant now holds what the file says: one network.
    fn forget_others(&self, id: u32) {
        let list = self.wpa.ask("LIST_NETWORKS").unwrap_or_default();
        for (other, _) in known(&list).into_iter().filter(|(n, _)| *n != id) {
            if let Err(e) = self.wpa.ask(&format!("REMOVE_NETWORK {other}")) {
                eprintln!("wpa_supplicant: {e}");
            }
        }
    }

    fn answer(&self, request: Request) -> Answer {
        let done = |r: Result<(), Refusal>| r.map_or_else(Answer::Refused, |()| Answer::Done);
        match request {
            Request::Scan {} => done(self.scan()),
            Request::List {} => self.list().map_or_else(Answer::Refused, Answer::Networks),
            Request::Connect { ssid, password } => done(self.connect(ssid, password)),
            Request::Status {} => self.status().map_or_else(Answer::Refused, Answer::Status),
        }
    }

    /// One client: one bounded line in, one answer out.
    fn serve(&self, stream: UnixStream) -> std::io::Result<()> {
        stream.set_read_timeout(Some(CLIENT_WAIT))?;
        stream.set_write_timeout(Some(CLIENT_WAIT))?;
        let mut line = String::new();
        BufReader::new((&stream).take(MAX_REQUEST)).read_line(&mut line)?;
        let answer = serde_json::from_str(&line)
            .map_or(Answer::Refused(Refusal::Unreadable), |r| self.answer(r));
        let mut out = serde_json::to_string(&answer).map_err(std::io::Error::other)?;
        out.push('\n');
        (&stream).write_all(out.as_bytes())
    }
}

/// The socket, for its directory's group: root's umask would shut `kiosk` out.
fn listen(path: &Path) -> std::io::Result<UnixListener> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o660))?;
    Ok(listener)
}

fn main() {
    let socket = Path::new(SOCKET);
    let dir = socket.parent().unwrap_or(Path::new("/run"));
    let listener = listen(socket).unwrap_or_else(|e| {
        eprintln!("{SOCKET}: {e}");
        process::exit(1);
    });
    let service = Service {
        wpa: Wpa::new(Path::new(CTRL), dir, WPA_WAIT),
        conf: CONF.into(),
        lost: LOST.into(),
        hotspot: HOTSPOT.into(),
        limit: JOIN_LIMIT,
        step: JOIN_STEP,
        join: Arc::default(),
    };
    for stream in listener.incoming() {
        let served = stream.and_then(|s| service.serve(s));
        if let Err(e) = served {
            eprintln!("a client: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixDatagram;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A short directory: a socket's path has a limit.
    fn dir() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = PathBuf::from(format!(
            "/tmp/mwd-{}-{}",
            process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const STATUS_OFF: &str = "wpa_state=SCANNING\n";
    const LIST_HEAD: &str = "network id / ssid / bssid / flags\n";

    /// A fake wpa_supplicant: a line of its log per command, each answered by `reply` (`None`: `OK`).
    fn fake(
        dir: &Path,
        reply: impl Fn(&str, &[String]) -> Option<String> + Send + 'static,
    ) -> Arc<Mutex<Vec<String>>> {
        let socket = UnixDatagram::bind(dir.join("wlan0")).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let kept = log.clone();
        std::thread::spawn(move || {
            let mut buffer = [0; 1024];
            // macOS says a reset once a client that was answered closed: only that client is gone
            loop {
                let Ok((read, from)) = socket.recv_from(&mut buffer) else {
                    continue;
                };
                let command = String::from_utf8_lossy(&buffer[..read]).into_owned();
                let said = kept.lock().unwrap().clone();
                let answer = reply(&command, &said).unwrap_or_else(|| "OK\n".into());
                kept.lock().unwrap().push(command);
                let _ = socket.send_to_addr(answer.as_bytes(), &from);
            }
        });
        log
    }

    fn service(dir: &Path) -> Service {
        Service {
            wpa: Wpa::new(&dir.join("wlan0"), dir, Duration::from_millis(500)),
            conf: dir.join("wifi.conf"),
            lost: dir.join("lost"),
            hotspot: dir.join("hotspot"),
            limit: Duration::from_millis(300),
            step: Duration::from_millis(10),
            join: Arc::default(),
        }
    }

    fn settled(s: &Service) -> Join {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let join = s.join().clone();
            if !matches!(join, Join::Joining { .. }) || Instant::now() > deadline {
                return join;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn connect(ssid: &str, password: Option<&str>) -> Request {
        Request::Connect {
            ssid: ssid.into(),
            password: password.map(Into::into),
        }
    }

    #[test]
    fn a_good_password_is_written_once_associated_and_the_others_forgotten() {
        let d = dir();
        let log = fake(&d, |command, said| match command {
            "ADD_NETWORK" => Some("4\n".into()),
            // associated from the second look
            "STATUS" if said.iter().filter(|c| *c == "STATUS").count() > 0 => {
                Some("id=4\nssid=Ma Box\nwpa_state=COMPLETED\n".into())
            }
            "STATUS" => Some(STATUS_OFF.into()),
            "LIST_NETWORKS" => Some(format!(
                "{LIST_HEAD}1\tVieux\tany\t[DISABLED]\n4\tMa Box\tany\t[CURRENT]\n"
            )),
            _ => None,
        });
        let mut s = service(&d);
        s.limit = Duration::from_secs(4);
        assert_eq!(
            s.answer(connect("Ma Box", Some("sel de mer"))),
            Answer::Done
        );
        assert_eq!(
            settled(&s),
            Join::Joined {
                ssid: "Ma Box".into()
            }
        );
        assert_eq!(
            fs::read_to_string(d.join("wifi.conf")).unwrap(),
            "network={\n\tssid=4d6120426f78\n\tpsk=\"sel de mer\"\n}\n"
        );
        let mode = fs::metadata(d.join("wifi.conf"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let log = log.lock().unwrap();
        for said in [
            "SET_NETWORK 4 ssid 4d6120426f78",
            "SET_NETWORK 4 psk \"sel de mer\"",
            "SELECT_NETWORK 4",
            "REMOVE_NETWORK 1",
        ] {
            assert!(log.iter().any(|c| c == said), "{said}: {log:?}");
        }
        assert!(!log.iter().any(|c| c == "REMOVE_NETWORK 4"));
    }

    #[test]
    fn an_open_network_has_no_password_to_set() {
        let d = dir();
        let log = fake(&d, |command, _| match command {
            "ADD_NETWORK" => Some("0\n".into()),
            "STATUS" => Some("id=0\nssid=Libre\nwpa_state=COMPLETED\n".into()),
            "LIST_NETWORKS" => Some(LIST_HEAD.into()),
            _ => None,
        });
        let s = service(&d);
        assert_eq!(s.answer(connect("Libre", None)), Answer::Done);
        assert_eq!(
            settled(&s),
            Join::Joined {
                ssid: "Libre".into()
            }
        );
        assert!(
            log.lock()
                .unwrap()
                .iter()
                .any(|c| c == "SET_NETWORK 0 key_mgmt NONE")
        );
        assert!(
            fs::read_to_string(d.join("wifi.conf"))
                .unwrap()
                .contains("key_mgmt=NONE")
        );
    }

    #[test]
    fn a_wrong_password_writes_nothing_and_gives_the_network_before_back() {
        let d = dir();
        fs::write(d.join("wifi.conf"), "before").unwrap();
        let log = fake(&d, |command, _| match command {
            "ADD_NETWORK" => Some("2\n".into()),
            "STATUS" => Some(STATUS_OFF.into()),
            "LIST_NETWORKS" => Some(format!("{LIST_HEAD}2\tMa Box\tany\t[TEMP-DISABLED]\n")),
            _ => None,
        });
        let s = service(&d);
        assert_eq!(
            s.answer(connect("Ma Box", Some("mauvais mot"))),
            Answer::Done
        );
        let failed = Join::Failed {
            ssid: "Ma Box".into(),
            why: Why::Password,
        };
        assert_eq!(settled(&s), failed);
        assert_eq!(fs::read_to_string(d.join("wifi.conf")).unwrap(), "before");
        let log = log.lock().unwrap();
        let removed = log.iter().position(|c| c == "REMOVE_NETWORK 2").unwrap();
        let enabled = log.iter().position(|c| c == "ENABLE_NETWORK all").unwrap();
        assert!(removed < enabled);
    }

    #[test]
    fn a_network_that_never_answers_gives_up_in_time() {
        let d = dir();
        fake(&d, |command, _| match command {
            "ADD_NETWORK" => Some("0\n".into()),
            "STATUS" => Some(STATUS_OFF.into()),
            "LIST_NETWORKS" => Some(LIST_HEAD.into()),
            _ => None,
        });
        let s = service(&d);
        assert_eq!(s.answer(connect("Loin", None)), Answer::Done);
        let failed = Join::Failed {
            ssid: "Loin".into(),
            why: Why::NoAnswer,
        };
        assert_eq!(settled(&s), failed);
        assert!(!d.join("wifi.conf").exists());
    }

    #[test]
    fn a_join_that_cannot_be_saved_is_no_success() {
        let d = dir();
        fake(&d, |command, _| match command {
            "ADD_NETWORK" => Some("0\n".into()),
            "STATUS" => Some("id=0\nssid=x\nwpa_state=COMPLETED\n".into()),
            _ => None,
        });
        let mut s = service(&d);
        s.conf = d.join("nowhere/wifi.conf");
        s.answer(connect("x", None));
        let failed = Join::Failed {
            ssid: "x".into(),
            why: Why::NotSaved,
        };
        assert_eq!(settled(&s), failed);
    }

    #[test]
    fn what_crosses_the_socket_is_checked_before_anything_is_said_to_wpa_supplicant() {
        let d = dir();
        let log = fake(&d, |_, _| None);
        let s = service(&d);
        for (request, why) in [
            (connect("", Some("abcdefgh")), Refusal::BadSsid),
            (connect(&"x".repeat(33), None), Refusal::BadSsid),
            (connect("x", Some("short")), Refusal::BadPassword),
            (
                connect("x", Some("abc\"defgh\nnetwork={")),
                Refusal::BadPassword,
            ),
            (connect("x", Some("abcdefgh\\")), Refusal::BadPassword),
        ] {
            assert_eq!(s.answer(request), Answer::Refused(why));
        }
        assert!(log.lock().unwrap().is_empty());
        assert_eq!(*s.join(), Join::Idle);
    }

    #[test]
    fn a_name_with_a_space_or_a_quote_reaches_wpa_supplicant_in_hex() {
        let d = dir();
        let log = fake(&d, |command, _| match command {
            "ADD_NETWORK" => Some("0\n".into()),
            "STATUS" => Some(STATUS_OFF.into()),
            "LIST_NETWORKS" => Some(LIST_HEAD.into()),
            _ => None,
        });
        let s = service(&d);
        s.answer(connect("a\" b\nSCAN", None));
        settled(&s);
        let log = log.lock().unwrap();
        assert!(
            log.iter()
                .any(|c| c == "SET_NETWORK 0 ssid 612220620a5343414e"),
            "{log:?}"
        );
        assert!(!log.iter().any(|c| c == "SCAN"));
    }

    #[test]
    fn one_join_at_a_time() {
        let d = dir();
        fake(&d, |command, _| match command {
            "ADD_NETWORK" => Some("0\n".into()),
            "STATUS" => Some(STATUS_OFF.into()),
            "LIST_NETWORKS" => Some(LIST_HEAD.into()),
            _ => None,
        });
        let s = service(&d);
        assert_eq!(s.answer(connect("a", None)), Answer::Done);
        assert_eq!(s.answer(connect("b", None)), Answer::Refused(Refusal::Busy));
        settled(&s);
        assert_eq!(s.answer(connect("b", None)), Answer::Done);
        settled(&s);
    }

    #[test]
    fn a_status_says_what_the_frame_is_on_and_what_the_watch_said() {
        let d = dir();
        let on = Arc::new(Mutex::new(false));
        let is_on = on.clone();
        fake(&d, move |command, _| match command {
            "STATUS" if *is_on.lock().unwrap() => {
                Some("id=0\nssid=Ma Box\nwpa_state=COMPLETED\n".into())
            }
            "STATUS" => Some(STATUS_OFF.into()),
            "LIST_NETWORKS" => Some(format!("{LIST_HEAD}0\tMa Box\tany\t[CURRENT]\n")),
            _ => None,
        });
        let s = service(&d);
        fs::write(d.join("lost"), "").unwrap();
        let status = |s: &Service| match s.answer(Request::Status {}) {
            Answer::Status(status) => status,
            other => panic!("{other:?}"),
        };
        let lost = status(&s);
        assert!(lost.lost && lost.configured && lost.connected.is_none());
        // the watch's word is 15 s behind: back on the network, nothing is lost
        *on.lock().unwrap() = true;
        let back = status(&s);
        assert!(!back.lost);
        assert_eq!(back.connected.as_deref(), Some("Ma Box"));
        // its own Wi-Fi is no network it is on
        fs::write(d.join("hotspot"), "").unwrap();
        let own = status(&s);
        assert!(own.hotspot && own.connected.is_none());
        assert_eq!(
            s.answer(Request::Scan {}),
            Answer::Refused(Refusal::Hotspot)
        );
        assert_eq!(
            s.answer(connect("x", None)),
            Answer::Refused(Refusal::Hotspot)
        );
    }

    #[test]
    fn nothing_configured_is_said_and_a_silent_wpa_supplicant_too() {
        let d = dir();
        fake(&d, |command, _| match command {
            "STATUS" => Some(STATUS_OFF.into()),
            "LIST_NETWORKS" => Some(LIST_HEAD.into()),
            _ => None,
        });
        let s = service(&d);
        let Answer::Status(status) = s.answer(Request::Status {}) else {
            panic!("a status")
        };
        assert!(!status.configured);
        let gone = service(&dir());
        assert_eq!(
            gone.answer(Request::Status {}),
            Answer::Refused(Refusal::Silent)
        );
    }

    #[test]
    fn a_scan_already_running_is_the_scan_asked() {
        let d = dir();
        fake(&d, |command, _| match command {
            "SCAN" => Some("FAIL-BUSY\n".into()),
            "STATUS" => Some("id=1\nssid=Ma Box\nwpa_state=COMPLETED\n".into()),
            "SCAN_RESULTS" => Some(
                "bssid / frequency / signal level / flags / ssid\n\
                 aa\t2412\t-40\t[ESS]\tLibre\n\
                 ab\t2412\t-60\t[WPA2-PSK-CCMP][ESS]\tMa Box\n"
                    .into(),
            ),
            _ => None,
        });
        let s = service(&d);
        assert_eq!(s.answer(Request::Scan {}), Answer::Done);
        let Answer::Networks(list) = s.answer(Request::List {}) else {
            panic!("a list")
        };
        assert_eq!(list[0].ssid, "Ma Box", "the current one first");
        assert!(list[0].current && list[0].locked);
        assert!(!list[1].current && !list[1].locked);
    }

    /// A line sent through the socket, the line answered.
    fn exchange(listener: &UnixListener, path: &Path, s: &Service, line: &[u8]) -> String {
        let mut client = UnixStream::connect(path).unwrap();
        client.write_all(line).unwrap();
        let (stream, _) = listener.accept().unwrap();
        s.serve(stream).unwrap();
        let mut said = String::new();
        BufReader::new(client).read_line(&mut said).unwrap();
        said
    }

    #[test]
    fn the_socket_takes_one_bounded_line_and_is_its_groups() {
        let d = dir();
        fake(&d, |_, _| None);
        let s = service(&d);
        let path = d.join("socket");
        fs::write(&path, "left by a crash").unwrap();
        let listener = listen(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o660);
        let unreadable = "{\"answer\":\"refused\",\"value\":\"unreadable\"}\n";
        assert_eq!(
            exchange(&listener, &path, &s, b"{\"op\":\"reboot\"}\n"),
            unreadable
        );
        // far past the limit, cut and refused: never held whole
        let long = format!("{{\"op\":\"scan\",\"x\":\"{}\"}}\n", "x".repeat(1000));
        assert_eq!(exchange(&listener, &path, &s, long.as_bytes()), unreadable);
        assert_eq!(
            exchange(&listener, &path, &s, b"{\"op\":\"scan\"}\n"),
            "{\"answer\":\"done\"}\n"
        );
    }
}
