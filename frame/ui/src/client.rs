//! Maison's API over HTTP (ureq, rustls; no async runtime): JSON both ways, the screen's cookie
//! kept and sent, nothing followed (a redirect is an answer), every wait bounded. A failure is
//! one of four, as the web app's `api.ts` tells them apart: not signed in (401: the screen was
//! removed, or never paired), refused (a coded refusal), unreachable (no answer in time, or
//! none at all), unreadable (an answer that is not the type's).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use maison_api::codes::Code;
use maison_api::screens::{COOKIE, COOKIE_SECURE};
use serde::Serialize;
use serde::de::DeserializeOwned;
use ureq::Agent;
use ureq::config::IpFamily;

/// The cookie a paired screen holds, by the backend's names for it (`__Host-` over HTTPS).
const COOKIE_NAMES: [&str; 2] = [COOKIE_SECURE, COOKIE];
/// Where the cookie is kept, in the state directory: the origin that set it, then the cookie.
const COOKIE_FILE: &str = "screen-cookie";
/// No answer is bigger than this (a plug's consumption: a few kilobytes).
const MAX_ANSWER: u64 = 4 * 1024 * 1024;
/// The event stream says nothing for this long: dead.
const EVENTS_SILENCE: Duration = Duration::from_millis(3 * maison_api::events::KEEP_ALIVE_MS);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// 401: the screen is not paired (any more).
    SignedOut,
    /// Another refusal, its `code` when the backend said one the API knows.
    Refused { status: u16, code: Option<Code> },
    /// Nothing came back in time (the detail goes to the journal).
    Unreachable(String),
    /// An answer that does not read as expected.
    Unreadable(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SignedOut => write!(f, "not signed in"),
            Self::Refused { status, code } => {
                write!(f, "refused {status} {}", code.map_or("", Code::as_str))
            }
            Self::Unreachable(e) => write!(f, "unreachable: {e}"),
            Self::Unreadable(e) => write!(f, "unreadable: {e}"),
        }
    }
}

/// The screen's cookie, in memory and in its file (0600, replaced whole), bound to the origin
/// that set it: one kept for another (the address changed, or was changed under it) is never
/// sent, and the screen pairs again.
#[derive(Debug)]
struct Jar {
    file: PathBuf,
    origin: String,
    cookie: Mutex<Option<String>>,
}

impl Jar {
    fn open(dir: &Path, origin: &str) -> Self {
        let file = dir.join(COOKIE_FILE);
        let cookie = match std::fs::read_to_string(&file) {
            Ok(kept) => Self::read(&kept, origin, &file),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                eprintln!(
                    "the screen's cookie could not be read from {}: {e}; it pairs again",
                    file.display()
                );
                None
            }
        };
        Self {
            file,
            origin: origin.to_string(),
            cookie: Mutex::new(cookie),
        }
    }

    /// The kept file's cookie, if `origin` set it.
    fn read(kept: &str, origin: &str, file: &Path) -> Option<String> {
        let mut lines = kept.lines().map(str::trim);
        let (Some(by), Some(cookie)) = (lines.next(), lines.next()) else {
            eprintln!(
                "{}: no origin with the cookie (an older file); it pairs again",
                file.display()
            );
            return None;
        };
        if by != origin {
            eprintln!(
                "{}: a cookie set by {by}, not by {origin}: never sent elsewhere; it pairs again",
                file.display()
            );
            return None;
        }
        let named = COOKIE_NAMES
            .iter()
            .any(|n| cookie.strip_prefix(n).is_some_and(|r| r.starts_with('=')));
        if !named {
            eprintln!(
                "{}: not the screen's cookie; it pairs again",
                file.display()
            );
        }
        named.then(|| cookie.to_string())
    }

    fn get(&self) -> Option<String> {
        self.cookie
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// A `Set-Cookie` the backend sent: the screen's kept (or forgotten when it expires it).
    fn take(&self, set_cookie: &str) {
        let pair = set_cookie.split(';').next().unwrap_or_default().trim();
        let Some((name, value)) = pair.split_once('=') else {
            return;
        };
        if !COOKIE_NAMES.contains(&name) {
            return;
        }
        let gone = value.is_empty()
            || set_cookie
                .split(';')
                .any(|a| a.trim().eq_ignore_ascii_case("max-age=0"));
        self.keep((!gone).then(|| pair.to_string()));
    }

    /// The screen was removed: its cookie goes, from memory and from its file.
    fn forget(&self) {
        self.keep(None);
    }

    /// `next` in memory and in the file (none: the file removed); a file that cannot be
    /// written is said, the cookie in memory used until the app ends.
    fn keep(&self, next: Option<String>) {
        let mut cookie = self.cookie.lock().unwrap_or_else(|e| e.into_inner());
        if *cookie == next {
            return;
        }
        if let Err(e) = self.save(next.as_deref()) {
            eprintln!(
                "the screen's cookie could not be kept in {}: {e}",
                self.file.display()
            );
        }
        *cookie = next;
    }

    fn save(&self, cookie: Option<&str>) -> std::io::Result<()> {
        let Some(cookie) = cookie else {
            return match std::fs::remove_file(&self.file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        };
        let dir = self.file.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let temp = dir.join(format!(".{COOKIE_FILE}.tmp"));
        let mut f = open_private(&temp)?;
        writeln!(f, "{}", self.origin)?;
        writeln!(f, "{cookie}")?;
        f.sync_all()?;
        std::fs::rename(&temp, &self.file)
    }
}

#[cfg(unix)]
fn open_private(p: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(p)
}

/// One per app, cloned into every thread (the connections are shared).
#[derive(Debug, Clone)]
pub struct Client {
    agent: Agent,
    api: String,
    jar: Arc<Jar>,
    timeout: Duration,
}

/// The agent every request goes through, over `family` (`Any` but for a waiting screen's
/// claims, which prove both its addresses).
fn agent(timeout: Duration, family: IpFamily) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .timeout_connect(Some(timeout.min(Duration::from_secs(5))))
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent(concat!("maison-frame-ui/", env!("CARGO_PKG_VERSION")))
        // straight to Maison: no proxy from the environment sees the screen's cookie
        .proxy(None)
        .ip_family(family)
        .build()
        .into()
}

impl Client {
    /// `origin`: `https://maison.example`; the cookie kept in `state_dir`.
    pub fn new(origin: &str, state_dir: &Path, timeout: Duration) -> Self {
        Self {
            agent: agent(timeout, IpFamily::Any),
            api: format!("{origin}/api"),
            jar: Arc::new(Jar::open(state_dir, origin)),
            timeout,
        }
    }

    /// A client of no Maison, no cookie: the setup view's, which asks nothing.
    pub fn nowhere(timeout: Duration) -> Self {
        Self {
            agent: agent(timeout, IpFamily::Any),
            api: String::new(),
            jar: Arc::new(Jar {
                file: PathBuf::new(),
                origin: String::new(),
                cookie: Mutex::new(None),
            }),
            timeout,
        }
    }

    /// The same client (its cookie too) over `family` alone.
    pub fn over(&self, family: IpFamily) -> Self {
        Self {
            agent: agent(self.timeout, family),
            ..self.clone()
        }
    }

    /// Whether the screen holds a cookie (it may still have been removed: `/screens/me` says).
    pub fn has_cookie(&self) -> bool {
        self.jar.get().is_some()
    }

    /// The screen was removed: its cookie goes, it pairs again.
    pub fn forget_cookie(&self) {
        self.jar.forget();
    }

    /// The request, with the screen's cookie if it holds one.
    fn with_cookie<B>(&self, req: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        match self.jar.get() {
            Some(c) => req.header("Cookie", c),
            None => req,
        }
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Failure> {
        let req = self.agent.get(format!("{}{path}", self.api));
        self.answer(self.with_cookie(req).call())
    }

    /// The server's event stream (`GET /api/events`), read line by line as it comes: no total
    /// timeout (a healthy stream stays open for days), but it says a keep-alive every
    /// `KEEP_ALIVE_MS`, so a stream silent for `EVENTS_SILENCE`, before its headers or after, is a
    /// dead one and is opened again.
    pub fn events(&self) -> Result<Box<dyn std::io::BufRead + Send>, Failure> {
        self.events_within(EVENTS_SILENCE)
    }

    fn events_within(
        &self,
        silence: Duration,
    ) -> Result<Box<dyn std::io::BufRead + Send>, Failure> {
        let req = self
            .agent
            .get(format!("{}{}", self.api, crate::paths::EVENTS))
            .header("Accept", "text/event-stream")
            .config()
            .timeout_global(None)
            .timeout_per_read(Some(silence))
            .build();
        let res = self
            .with_cookie(req)
            .call()
            .map_err(|e| Failure::Unreachable(e.to_string()))?;
        match res.status().as_u16() {
            200 => Ok(Box::new(std::io::BufReader::new(
                res.into_body().into_reader(),
            ))),
            status => Err(refused(status, None)),
        }
    }

    /// A POST with a JSON body, or none.
    pub fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<T, Failure> {
        self.send(self.agent.post(format!("{}{path}", self.api)), body)
    }

    /// A PUT with a JSON body, or none.
    pub fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<T, Failure> {
        self.send(self.agent.put(format!("{}{path}", self.api)), body)
    }

    fn send<T: DeserializeOwned>(
        &self,
        req: ureq::RequestBuilder<ureq::typestate::WithBody>,
        body: Option<&impl Serialize>,
    ) -> Result<T, Failure> {
        let req = self.with_cookie(req);
        let sent = match body {
            Some(b) => {
                let json =
                    serde_json::to_string(b).map_err(|e| Failure::Unreadable(e.to_string()))?;
                req.content_type("application/json").send(json)
            }
            None => req.send_empty(),
        };
        self.answer(sent)
    }

    fn answer<T: DeserializeOwned>(
        &self,
        sent: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<T, Failure> {
        let mut res = sent.map_err(|e| Failure::Unreachable(e.to_string()))?;
        for c in res.headers().get_all("set-cookie") {
            if let Ok(c) = c.to_str() {
                self.jar.take(c);
            }
        }
        let status = res.status().as_u16();
        let text = res
            .body_mut()
            .with_config()
            .limit(MAX_ANSWER)
            .read_to_string()
            .map_err(|e| Failure::Unreachable(e.to_string()))?;
        match status {
            200..=299 => serde_json::from_str(&text)
                .map_err(|e| Failure::Unreadable(format!("{e}: {}", clip(&text)))),
            _ => Err(refused(status, code_of(&text))),
        }
    }
}

/// A status that is no success: 401 means only « not signed in ».
fn refused(status: u16, code: Option<Code>) -> Failure {
    match status {
        401 => Failure::SignedOut,
        _ => Failure::Refused { status, code },
    }
}

/// A refusal's `code`, when it is one the API says (another is said in the journal).
fn code_of(text: &str) -> Option<Code> {
    let v = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let name = v.get("code")?.as_str()?;
    let code = Code::parse(name);
    if code.is_none() {
        eprintln!("a refusal with a code the API does not say: {name}");
    }
    code
}

/// The start of an answer, for the journal.
fn clip(s: &str) -> &str {
    let end = s.char_indices().nth(200).map_or(s.len(), |(i, _)| i);
    &s[..end]
}

#[cfg(test)]
pub mod stub {
    //! A one-thread HTTP server answering what a test says, recording what it was asked.

    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// What was asked: the request line, its headers and body.
    #[derive(Debug, Clone)]
    pub struct Asked {
        pub line: String,
        pub headers: Vec<String>,
        pub body: String,
    }

    pub struct Stub {
        pub origin: String,
        pub asked: Arc<Mutex<Vec<Asked>>>,
    }

    /// `answer(request line) → (status, extra headers, body)`.
    pub fn serve(answer: impl Fn(&str) -> (u16, Vec<String>, String) + Send + 'static) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut headers = Vec::new();
                let mut length = 0;
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    let h = h.trim_end().to_string();
                    if h.is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    headers.push(h);
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let line = line.trim_end().to_string();
                let (status, extra, text) = answer(&line);
                log.lock().unwrap().push(Asked {
                    line,
                    headers,
                    body: String::from_utf8(body).unwrap(),
                });
                let mut out = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
                    text.len()
                );
                for h in extra {
                    out.push_str(&h);
                    out.push_str("\r\n");
                }
                out.push_str("\r\n");
                out.push_str(&text);
                let _ = stream.write_all(out.as_bytes());
            }
        });
        Stub { origin, asked }
    }
}

#[cfg(test)]
mod tests {
    use super::stub::serve;
    use super::*;
    use crate::testing::temp_dir;

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Hello {
        hello: String,
    }

    #[test]
    fn the_screen_cookie_is_kept_private_and_sent() {
        let dir = temp_dir("cookie");
        let stub = serve(|line| match line {
            l if l.starts_with("POST /api/screens/pairing/claim") => (
                200,
                vec![
                    "set-cookie: maison_screen=tok1; Path=/; HttpOnly".into(),
                    "set-cookie: other=x".into(),
                ],
                r#"{"hello":"paired"}"#.into(),
            ),
            _ => (200, vec![], r#"{"hello":"again"}"#.into()),
        });
        let c = Client::new(&stub.origin, &dir, Duration::from_secs(5));
        assert!(!c.has_cookie());
        let h: Hello = c
            .post(
                "/screens/pairing/claim",
                Some(&serde_json::json!({"secret": "s"})),
            )
            .unwrap();
        assert_eq!(h.hello, "paired");
        assert!(c.has_cookie());
        let _: Hello = c.get("/screens/me").unwrap();
        let asked = stub.asked.lock().unwrap().clone();
        assert_eq!(asked[0].body, r#"{"secret":"s"}"#);
        assert!(
            asked[0]
                .headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case("content-type: application/json"))
        );
        assert!(
            asked[1]
                .headers
                .iter()
                .any(|h| h == "cookie: maison_screen=tok1" || h == "Cookie: maison_screen=tok1")
        );
        // kept for the next start, readable by the service alone
        let file = dir.join(COOKIE_FILE);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!("{}\nmaison_screen=tok1\n", stub.origin)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(Client::new(&stub.origin, &dir, Duration::from_secs(5)).has_cookie());
        // bound to its origin: another address never gets it
        assert!(
            !Client::new("https://elsewhere.example", &dir, Duration::from_secs(5)).has_cookie()
        );
        c.forget_cookie();
        assert!(!file.exists());
        assert!(!c.has_cookie());
    }

    #[test]
    fn an_expired_cookie_is_forgotten() {
        let dir = temp_dir("cookie-expired");
        let origin = "https://maison.example";
        let jar = Jar::open(&dir, origin);
        jar.take("__Host-maison_screen=abc; Secure; Path=/");
        assert_eq!(jar.get().as_deref(), Some("__Host-maison_screen=abc"));
        jar.take("__Host-maison_screen=; Max-Age=0");
        assert_eq!(jar.get(), None);
        assert!(!dir.join(COOKIE_FILE).exists());
        // a file that holds anything else is no cookie, nor one without its origin (older)
        let file = dir.join(COOKIE_FILE);
        std::fs::write(&file, format!("{origin}\njunk\n")).unwrap();
        assert_eq!(Jar::open(&dir, origin).get(), None);
        std::fs::write(&file, "maison_screen=old").unwrap();
        assert_eq!(Jar::open(&dir, origin).get(), None);
        std::fs::write(&file, format!("{origin}\nmaison_screen=kept\n")).unwrap();
        assert_eq!(
            Jar::open(&dir, origin).get().as_deref(),
            Some("maison_screen=kept")
        );
        assert_eq!(Jar::open(&dir, "https://other.example").get(), None);
    }

    /// A server that takes each connection and has `talk` hold it (dropped: closed).
    fn listening(talk: fn(std::net::TcpStream)) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                std::thread::spawn(move || talk(stream));
            }
        });
        origin
    }

    #[test]
    fn a_peer_lost_before_its_headers_does_not_hang_the_stream() {
        let origin = listening(|stream| {
            // connected, then nothing, ever: the socket stays open
            std::thread::sleep(Duration::from_secs(30));
            drop(stream);
        });
        let c = Client::new(&origin, &temp_dir("events-lost"), Duration::from_secs(5));
        let started = std::time::Instant::now();
        let silence = Duration::from_millis(200);
        assert!(matches!(
            c.events_within(silence).map(|_| ()),
            Err(Failure::Unreachable(_))
        ));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_healthy_stream_outlives_its_silence_and_a_mute_one_ends() {
        use std::io::{BufRead, Read, Write};
        let origin = listening(|mut stream| {
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n";
            stream.write_all(head.as_bytes()).unwrap();
            // keep-alives for six times the silence, then mute, the socket open
            for i in 0..6 {
                write!(stream, ": keep-alive {i}\n\n").unwrap();
                std::thread::sleep(Duration::from_millis(100));
            }
            std::thread::sleep(Duration::from_secs(30));
        });
        let c = Client::new(&origin, &temp_dir("events-mute"), Duration::from_secs(5));
        let stream = c.events_within(Duration::from_millis(300)).unwrap();
        let started = std::time::Instant::now();
        let lines: Vec<String> = stream.lines().map_while(Result::ok).collect();
        assert_eq!(
            lines.iter().filter(|l| !l.is_empty()).count(),
            6,
            "{lines:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "ended by itself"
        );
    }

    #[test]
    fn failures_are_told_apart() {
        let dir = temp_dir("failures");
        let stub = serve(|line| match line {
            l if l.contains("/me") => (
                401,
                vec![],
                r#"{"success":false,"code":"not_signed_in"}"#.into(),
            ),
            l if l.contains("/coded") => (
                409,
                vec![],
                r#"{"success":false,"error":"x","code":"screen_exists"}"#.into(),
            ),
            l if l.contains("/moved") => (
                302,
                vec!["location: https://elsewhere/".into()],
                String::new(),
            ),
            _ => (200, vec![], "not json".into()),
        });
        let c = Client::new(&stub.origin, &dir, Duration::from_secs(5));
        assert_eq!(c.get::<Hello>("/screens/me"), Err(Failure::SignedOut));
        assert_eq!(
            c.get::<Hello>("/coded"),
            Err(Failure::Refused {
                status: 409,
                code: Some(Code::ScreenExists)
            })
        );
        // a redirect is never followed
        assert_eq!(
            c.get::<Hello>("/moved"),
            Err(Failure::Refused {
                status: 302,
                code: None
            })
        );
        assert!(matches!(c.get::<Hello>("/x"), Err(Failure::Unreadable(_))));
        // nobody there
        let gone = Client::new("http://127.0.0.1:9", &dir, Duration::from_secs(2));
        assert!(matches!(
            gone.get::<Hello>("/x"),
            Err(Failure::Unreachable(_))
        ));
    }
}
