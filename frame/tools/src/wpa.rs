//! wpa_supplicant's control socket, without wpa_cli: a Unix datagram socket, one command, one reply.
//! The replies' texts are read here, pure.

use std::collections::HashMap;
use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::wifi::Network;

/// wpa_ctrl's own reply limit is 4096 bytes: a scan past some forty networks is cut there.
const REPLY: usize = 8192;

#[derive(Debug, Clone)]
pub struct Wpa {
    /// The interface's socket (`/run/wpa_supplicant/wlan0`).
    ctrl: PathBuf,
    /// Where this end binds: wpa_supplicant replies to an address.
    dir: PathBuf,
    wait: Duration,
}

/// This end's socket file, gone with the exchange.
struct Bound(PathBuf);

impl Drop for Bound {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl Wpa {
    pub fn new(ctrl: &Path, dir: &Path, wait: Duration) -> Self {
        Self {
            ctrl: ctrl.into(),
            dir: dir.into(),
            wait,
        }
    }

    /// One command's reply; a reply of `FAIL` is an error.
    pub fn ask(&self, command: &str) -> io::Result<String> {
        static N: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "wpa-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        );
        let bound = Bound(self.dir.join(name));
        let socket = UnixDatagram::bind(&bound.0)?;
        socket.set_read_timeout(Some(self.wait))?;
        socket.set_write_timeout(Some(self.wait))?;
        socket.connect(&self.ctrl)?;
        socket.send(command.as_bytes())?;
        let mut reply = vec![0; REPLY];
        let read = socket.recv(&mut reply)?;
        let reply = String::from_utf8_lossy(&reply[..read]).into_owned();
        if reply.trim_end().starts_with("FAIL") {
            return Err(io::Error::other(format!(
                "{}: {}",
                verb(command),
                reply.trim_end()
            )));
        }
        Ok(reply)
    }
}

/// A command's first word: what a log may say of it (a password travels in some).
fn verb(command: &str) -> &str {
    command.split(' ').next().unwrap_or_default()
}

/// wpa_supplicant's `printf_decode`: a name's escapes back to bytes, as UTF-8 text. None: a name that
/// is not (it could be neither shown nor typed).
pub fn decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        if b != b'\\' {
            out.push(b);
            continue;
        }
        let escape = *bytes.get(i)?;
        i += 1;
        out.push(match escape {
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'e' => 0x1b,
            b'x' => {
                let pair = text.get(i..i + 2)?;
                i += 2;
                u8::from_str_radix(pair, 16).ok()?
            }
            other => other,
        });
    }
    String::from_utf8(out).ok()
}

/// What can be joined with a password or none: WPA-PSK, or nothing at all. Enterprise, WEP, OWE and
/// WPA3 alone are not (`network()` writes neither).
fn joinable(flags: &str) -> Option<bool> {
    if !flags.contains("[ESS]") {
        return None;
    }
    if flags.contains("-PSK") {
        return Some(true);
    }
    let secured = ["WPA", "RSN", "WEP", "OSEN", "OWE"];
    (!secured.iter().any(|s| flags.contains(s))).then_some(false)
}

/// `SCAN_RESULTS`' text: each network once, at its strongest, the current one first, then by signal.
pub fn networks(results: &str, current: Option<&str>) -> Vec<Network> {
    let mut found: Vec<Network> = Vec::new();
    for line in results.lines().skip(1) {
        let mut fields = line.splitn(5, '\t');
        let (Some(_), Some(_), Some(signal), Some(flags), Some(ssid)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        let (Some(ssid), Ok(signal), Some(locked)) =
            (decoded(ssid), signal.parse::<i32>(), joinable(flags))
        else {
            continue;
        };
        if ssid.is_empty() || ssid.chars().any(char::is_control) {
            continue; // a hidden network
        }
        match found.iter_mut().find(|n| n.ssid == ssid) {
            Some(n) => n.signal = n.signal.max(signal),
            None => found.push(Network {
                current: current == Some(ssid.as_str()),
                ssid,
                signal,
                locked,
            }),
        }
    }
    found.sort_by_key(|n| (!n.current, -n.signal));
    found
}

/// `STATUS`' `key=value` lines.
pub fn fields(status: &str) -> HashMap<&str, &str> {
    status.lines().filter_map(|l| l.split_once('=')).collect()
}

/// The network wpa_supplicant finished associating with, as `(id, name)`.
pub fn associated(status: &str) -> Option<(u32, String)> {
    let f = fields(status);
    if f.get("wpa_state") != Some(&"COMPLETED") {
        return None;
    }
    Some((f.get("id")?.parse().ok()?, decoded(f.get("ssid")?)?))
}

/// `LIST_NETWORKS`' lines, as `(id, flags)`.
pub fn known(list: &str) -> Vec<(u32, &str)> {
    list.lines()
        .skip(1)
        .filter_map(|l| {
            let id = l.split('\t').next()?.parse().ok()?;
            Some((id, l.rsplit('\t').next()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCAN: &str = "bssid / frequency / signal level / flags / ssid\n\
        aa:aa:aa:aa:aa:01\t2412\t-71\t[WPA2-PSK-CCMP][ESS]\tMa Box\n\
        aa:aa:aa:aa:aa:02\t5180\t-48\t[WPA2-PSK+SAE-CCMP][ESS]\tMa Box\n\
        aa:aa:aa:aa:aa:03\t2437\t-60\t[ESS]\tCaf\\xc3\\xa9 \\\"libre\\\"\n\
        aa:aa:aa:aa:aa:04\t2437\t-50\t[WPA2-EAP-CCMP][ESS]\tBureau\n\
        aa:aa:aa:aa:aa:05\t2437\t-50\t[WPA2-SAE-CCMP][ESS]\tSeul WPA3\n\
        aa:aa:aa:aa:aa:06\t2437\t-50\t[WEP][ESS]\tVieux\n\
        aa:aa:aa:aa:aa:07\t2437\t-50\t[ESS]\t\\x00\\x00\n\
        aa:aa:aa:aa:aa:08\t2437\t-50\t[ESS]\t\n\
        aa:aa:aa:aa:aa:09\t2437\t-50\t[WPA2-PSK-CCMP][ESS]\tlatin\\xe9\n\
        aa:aa:aa:aa:aa:0a\t2437\t-40\t[IBSS]\tPair\n\
        pas une ligne\n\
        aa:aa:aa:aa:aa:0b\t2437\t-80\t[WPA-PSK-TKIP][ESS]\tVoisin\n";

    #[test]
    fn a_scan_lists_each_network_once_that_can_be_joined() {
        let list = networks(SCAN, Some("Voisin"));
        let said: Vec<_> = list
            .iter()
            .map(|n| (n.ssid.as_str(), n.signal, n.locked, n.current))
            .collect();
        assert_eq!(
            said,
            [
                ("Voisin", -80, true, true),
                ("Ma Box", -48, true, false),
                ("Café \"libre\"", -60, false, false),
            ]
        );
        assert!(networks("bssid / frequency\n", None).is_empty());
    }

    #[test]
    fn a_name_is_read_back_as_wpa_supplicant_escaped_it() {
        assert_eq!(decoded(r"a\\b\nc\t\e").unwrap(), "a\\b\nc\t\x1b");
        assert_eq!(decoded(r"\xc3\xa9").unwrap(), "é");
        assert_eq!(decoded(r"\xe9"), None, "not text");
        assert_eq!(decoded(r"\"), None);
        assert_eq!(decoded(r"\xz"), None);
    }

    #[test]
    fn a_status_says_where_the_frame_is_only_once_it_is_there() {
        let done = "bssid=aa\nfreq=2412\nssid=Ma Box\nid=3\nmode=station\nwpa_state=COMPLETED\n";
        assert_eq!(associated(done), Some((3, "Ma Box".into())));
        assert_eq!(associated("wpa_state=SCANNING\n"), None);
        assert_eq!(associated("wpa_state=4WAY_HANDSHAKE\nid=1\nssid=x\n"), None);
    }

    #[test]
    fn a_list_says_each_networks_flags() {
        let list = "network id / ssid / bssid / flags\n\
            0\tMa Box\tany\t[CURRENT]\n\
            1\tAutre\tany\t[DISABLED]\n\
            2\tMauvais\tany\t[TEMP-DISABLED]\n";
        assert_eq!(
            known(list),
            [(0, "[CURRENT]"), (1, "[DISABLED]"), (2, "[TEMP-DISABLED]")]
        );
        assert!(known("network id / ssid / bssid / flags\n").is_empty());
    }

    #[test]
    fn one_command_one_reply_and_a_fail_is_an_error_that_keeps_its_secrets() {
        let dir = std::env::temp_dir().join(format!("wpa-ask-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ctrl = dir.join("wlan0");
        let fake = UnixDatagram::bind(&ctrl).unwrap();
        let wpa = Wpa::new(&ctrl, &dir, Duration::from_millis(200));
        let replying = std::thread::spawn(move || {
            let mut buffer = [0; 256];
            for reply in ["OK\n", "FAIL\n"] {
                // macOS says a reset once a client that was answered closed
                let (read, from) = loop {
                    if let Ok(got) = fake.recv_from(&mut buffer) {
                        break got;
                    }
                };
                assert!(buffer[..read].starts_with(b"SET_NETWORK"));
                fake.send_to_addr(reply.as_bytes(), &from).unwrap();
            }
        });
        assert_eq!(wpa.ask("SET_NETWORK 0 x y").unwrap(), "OK\n");
        let refused = wpa.ask("SET_NETWORK 0 psk \"secret\"").unwrap_err();
        assert!(!refused.to_string().contains("secret"), "{refused}");
        replying.join().unwrap();
        // nobody answers: bounded
        assert!(wpa.ask("STATUS").is_err());
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(left.len(), 1, "this end's sockets are gone: {left:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
