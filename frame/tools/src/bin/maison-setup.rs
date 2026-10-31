//! The setup, while the frame is its own Wi-Fi: one page on 192.168.4.1 asks for the house's network
//! and Maison's address (the one kept, if any, already there), writes them where the system reads
//! them, and restarts. The words are i18n/messages/fr.json's. Only ever runs when the power key was
//! held (maison-power-key, maison-hotspot): the last resort of a dead touch panel. Each request
//! touches `SETUP`: someone at the form holds the watch's way back to the configured network.

use maison_frame_tools::wifi::{CONF, network, password_fits, private, ssid_fits};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;
use std::{fs, process};

const LISTEN: &str = "192.168.4.1:80";
const TARGET: &str = "/android-data/maison/target";
const SETUP: &str = "/run/maison-setup";
const MESSAGES: &str = include_str!("../../../../i18n/messages/fr.json");

#[derive(Debug, PartialEq)]
struct Answer {
    ssid: String,
    password: String,
    target: String,
}

fn words() -> Value {
    serde_json::from_str(MESSAGES).unwrap_or(Value::Null)
}

fn word<'a>(words: &'a Value, key: &'a str) -> &'a str {
    words[key].as_str().unwrap_or(key)
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

fn page(error: Option<&str>, target: &str) -> String {
    let w = words();
    let line = error
        .map(|k| format!("<p role=alert>{}</p>", escaped(word(&w, k))))
        .unwrap_or_default();
    format!(
        "<!doctype html><html lang=fr><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>{title}</title><style>body{{font:18px system-ui;margin:2rem auto;max-width:24rem;padding:0 1rem}}\
         label,input,button{{display:block;width:100%;box-sizing:border-box;margin-top:.75rem}}input,button{{font:inherit;padding:.75rem}}</style>\
         <h1>{title}</h1>{line}<form method=post>\
         <label>{ssid}<input name=ssid required autocomplete=off></label>\
         <label>{password}<input name=password type=password required minlength=8 maxlength=63></label>\
         <label>{target}<input name=target type=url required placeholder=https:// value=\"{kept}\"></label>\
         <button>{save}</button></form></html>",
        title = escaped(word(&w, "screen_setup_title")),
        ssid = escaped(word(&w, "frame_setup_page_ssid")),
        password = escaped(word(&w, "frame_setup_page_password")),
        target = escaped(word(&w, "frame_setup_page_target")),
        kept = escaped(target),
        save = escaped(word(&w, "common_save")),
    )
}

fn decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let pair = text.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(pair, 16).ok()?);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

/// What the form said, only if it can go into the files as it is: a line break or a quote there
/// would write configuration of its own, and the address must be HTTPS.
fn parsed(form: &str) -> Result<Answer, &'static str> {
    let field = |name: &str| {
        form.split('&')
            .find_map(|p| p.strip_prefix(name).and_then(|r| r.strip_prefix('=')))
            .and_then(decoded)
            .unwrap_or_default()
    };
    let (ssid, password, target) = (field("ssid"), field("password"), field("target"));
    if !ssid_fits(&ssid) {
        return Err("frame_setup_page_bad_ssid");
    }
    if !password_fits(&password) {
        return Err("frame_setup_page_bad_password");
    }
    let host = target.strip_prefix("https://").unwrap_or("");
    if host.is_empty() || target.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("frame_setup_page_bad_target");
    }
    Ok(Answer {
        ssid,
        password,
        target: target.trim_end_matches('/').to_string(),
    })
}

fn save(answer: &Answer) -> std::io::Result<()> {
    let wifi = network(&answer.ssid, Some(&answer.password));
    private(Path::new(CONF), &wifi)?;
    private(Path::new(TARGET), &answer.target)
}

/// The address a frame already has, for the form.
fn kept_target() -> String {
    fs::read_to_string(TARGET)
        .map(|t| t.trim().to_string())
        .unwrap_or_default()
}

fn seen() {
    let touched = fs::File::options()
        .write(true)
        .open(SETUP)
        .and_then(|f| f.set_modified(SystemTime::now()));
    if let Err(e) = touched {
        eprintln!("{SETUP}: {e}");
    }
}

fn serve(stream: TcpStream) -> std::io::Result<bool> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut writer = stream;
    let (body, done) = if request.starts_with("POST") && length <= 4096 {
        let mut form = vec![0; length];
        reader.read_exact(&mut form)?;
        match parsed(&String::from_utf8_lossy(&form)) {
            Ok(answer) => {
                save(&answer)?;
                (
                    format!(
                        "<!doctype html><meta charset=utf-8><p>{}</p>",
                        escaped(word(&words(), "frame_setup_page_done"))
                    ),
                    true,
                )
            }
            Err(key) => (page(Some(key), &kept_target()), false),
        }
    } else {
        (page(None, &kept_target()), false)
    };
    write!(
        writer,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    Ok(done)
}

fn main() {
    let listener = TcpListener::bind(LISTEN).unwrap_or_else(|e| {
        eprintln!("{LISTEN}: {e}");
        process::exit(1);
    });
    for stream in listener.incoming().flatten() {
        seen();
        match serve(stream) {
            Ok(true) => {
                let _ = Command::new("systemctl").arg("reboot").status();
                return;
            }
            Ok(false) => {}
            Err(e) => eprintln!("setup: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_is_read_with_its_spaces_and_accents() {
        let a = parsed(
            "ssid=Ma+Bo%C3%AEte&password=secret%21%2112&target=https%3A%2F%2Fhome.example%2F",
        )
        .unwrap();
        assert_eq!(
            a,
            Answer {
                ssid: "Ma Boîte".into(),
                password: "secret!!12".into(),
                target: "https://home.example".into()
            }
        );
    }

    #[test]
    fn nothing_can_write_configuration_of_its_own() {
        assert!(parsed("ssid=x&password=abc%22def%0Agh&target=https%3A%2F%2Fa.b").is_ok());
        assert_eq!(
            parsed("ssid=x&password=abcdefgh&target=http%3A%2F%2Fa.b"),
            Err("frame_setup_page_bad_target")
        );
        assert_eq!(
            parsed("ssid=x&password=abcdefgh&target=https%3A%2F%2Fa.b%0Aevil"),
            Err("frame_setup_page_bad_target")
        );
        assert_eq!(
            parsed("ssid=&password=abcdefgh&target=https%3A%2F%2Fa.b"),
            Err("frame_setup_page_bad_ssid")
        );
        assert_eq!(
            parsed("ssid=x&password=short&target=https%3A%2F%2Fa.b"),
            Err("frame_setup_page_bad_password")
        );
    }

    #[test]
    fn the_page_names_what_it_is_told_is_wrong() {
        assert!(page(None, "").contains("<form method=post>"));
        assert!(page(Some("frame_setup_page_bad_ssid"), "").contains("role=alert"));
    }

    #[test]
    fn an_address_already_kept_is_in_the_form_and_cannot_leave_its_attribute() {
        assert!(page(None, "").contains("value=\"\""));
        assert!(page(None, "https://home.example").contains("value=\"https://home.example\""));
        assert!(page(None, "https://a\"><b").contains("value=\"https://a&quot;>&lt;b\""));
    }
}
