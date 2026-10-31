//! The frame's Wi-Fi, said once: what a network's name and password may be, the file the system
//! reads them from (`maison-setup` and `maison-wifid` write it), and the one protocol between the
//! interface, which runs as `kiosk`, and `maison-wifid`, which runs as root: one JSON request on one
//! line, one JSON answer on one line, per connection. Nothing else crosses that socket.

use std::fs;
use std::io::Write;
use std::num::NonZeroU32;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use ring::pbkdf2;
use serde::{Deserialize, Serialize};

use crate::hex;

/// `maison-wifid`'s socket: its directory is root's, group `kiosk`.
pub const SOCKET: &str = "/run/maison-wifi/socket";
/// What the system reads the frame's network from.
pub const CONF: &str = "/android-data/maison/wifi.conf";
/// The longest request read (a name and a password, each escaped at worst).
pub const MAX_REQUEST: u64 = 1024;

/// A name can be any 1 to 32 bytes: it goes in hex.
pub fn ssid_fits(ssid: &str) -> bool {
    !ssid.is_empty() && ssid.len() <= 32
}

/// What WPA takes. Any character will do: only `psk` leaves here.
pub fn password_fits(password: &str) -> bool {
    (8..=63).contains(&password.len())
}

/// The key a passphrase makes on a network (IEEE 802.11i): what wpa_supplicant keeps, in hex and
/// unquoted, so that no password is ever written down or has anything to escape.
pub fn psk(ssid: &str, password: &str) -> String {
    let mut key = [0; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA1,
        NonZeroU32::new(4096).expect("not zero"),
        ssid.as_bytes(),
        password.as_bytes(),
        &mut key,
    );
    hex(&key)
}

/// The file's text for one network; no password: an open one.
pub fn network(ssid: &str, password: Option<&str>) -> String {
    let name = hex(ssid.as_bytes());
    match password {
        Some(password) => format!(
            "network={{\n\tssid={name}\n\tpsk={}\n}}\n",
            psk(ssid, password)
        ),
        None => format!("network={{\n\tssid={name}\n\tkey_mgmt=NONE\n}}\n"),
    }
}

/// `text` in `path`: 0600, whole or not at all.
pub fn private(path: &Path, text: &str) -> std::io::Result<()> {
    let next = path.with_extension("new");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&next)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::set_permissions(&next, fs::Permissions::from_mode(0o600))?;
    fs::rename(&next, path)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    // `{}`: serde checks a unit variant for nothing it does not know
    Scan {},
    List {},
    Connect {
        ssid: String,
        /// None: an open network.
        password: Option<String>,
    },
    Status {},
}

/// A network in range, one line of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub ssid: String,
    /// dBm, the strongest of its access points.
    pub signal: i32,
    pub locked: bool,
    /// The one the frame is on.
    pub current: bool,
}

/// Why a network was not joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// The handshake failed.
    Password,
    /// Nothing came of it in time.
    NoAnswer,
    /// Joined, but the file could not be written.
    NotSaved,
}

/// The last `connect`, as far as it went.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Join {
    #[default]
    Idle,
    Joining {
        ssid: String,
    },
    Joined {
        ssid: String,
    },
    Failed {
        ssid: String,
        why: Why,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The network the frame is on.
    pub connected: Option<String>,
    /// wpa_supplicant knows a network.
    pub configured: bool,
    /// `maison-wifi-watch` says no network for 10 minutes.
    pub lost: bool,
    /// The frame is its own Wi-Fi (the power key held): nothing to scan or join.
    pub hotspot: bool,
    pub join: Join,
}

/// Why a request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// Not a request: not one line of the protocol.
    Unreadable,
    BadSsid,
    BadPassword,
    /// A `connect` is on its way.
    Busy,
    Hotspot,
    /// wpa_supplicant did not answer.
    Silent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", content = "value", rename_all = "snake_case")]
pub enum Answer {
    Done,
    Status(Status),
    Networks(Vec<Network>),
    Refused(Refusal),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_ieee_80211i_s_own_test_vector() {
        assert_eq!(
            psk("IEEE", "password"),
            "f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e"
        );
    }

    #[test]
    fn a_name_goes_in_hex_and_a_password_never_goes_at_all() {
        let key = psk("a\"b", "abc\"d\\gh");
        assert_eq!(
            network("a\"b", Some("abc\"d\\gh")),
            format!("network={{\n\tssid=612262\n\tpsk={key}\n}}\n")
        );
        assert_eq!(
            network("a\nb", None),
            "network={\n\tssid=610a62\n\tkey_mgmt=NONE\n}\n"
        );
        assert!(ssid_fits("x") && ssid_fits(&"x".repeat(32)));
        assert!(!ssid_fits("") && !ssid_fits(&"x".repeat(33)));
        assert!(password_fits("abc\"d\\gh") && !password_fits("short"));
        assert!(!password_fits(&"x".repeat(64)));
    }

    #[test]
    fn nothing_can_write_configuration_of_its_own() {
        for password in [
            "abc\"defgh",
            "abc\ndefgh\nnetwork={",
            "abc\rdefgh",
            "abc\\defgh",
            "abc\0defgh",
        ] {
            assert!(password_fits(password));
            let key = psk("x", password);
            assert_eq!(key.len(), 64);
            assert!(key.bytes().all(|b| b.is_ascii_hexdigit()), "{password:?}");
        }
    }

    #[test]
    fn the_file_is_whole_and_private() {
        let dir = std::env::temp_dir().join(format!("maison-wifi-file-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("wifi.conf");
        fs::write(&file, "old").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        private(&file, "new").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        let mode = fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!file.with_extension("new").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_protocol_reads_what_it_writes_and_nothing_else() {
        let connect = r#"{"op":"connect","ssid":"Ma Box","password":"abcdefgh"}"#;
        assert_eq!(
            serde_json::from_str::<Request>(connect).unwrap(),
            Request::Connect {
                ssid: "Ma Box".into(),
                password: Some("abcdefgh".into())
            }
        );
        assert_eq!(
            serde_json::to_string(&Request::Status {}).unwrap(),
            r#"{"op":"status"}"#
        );
        for refused in [
            r#"{"op":"reboot"}"#,
            r#"{"op":"status","extra":1}"#,
            r#"{"op":"connect","ssid":"x","password":"abcdefgh","priority":9}"#,
            "status",
        ] {
            assert!(
                serde_json::from_str::<Request>(refused).is_err(),
                "{refused}"
            );
        }
        let answer = Answer::Status(Status {
            join: Join::Failed {
                ssid: "x".into(),
                why: Why::Password,
            },
            ..Status::default()
        });
        let line = serde_json::to_string(&answer).unwrap();
        assert_eq!(serde_json::from_str::<Answer>(&line).unwrap(), answer);
        assert!(line.contains(r#""state":"failed""#), "{line}");
    }
}
