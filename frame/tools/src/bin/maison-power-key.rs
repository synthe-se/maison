//! The power key held 10 s opens the frame's own Wi-Fi: the last resort of a dead touch panel, the
//! only way it ever starts (`maison-hotspot open`). Reads the gpio-keys device's events; a short
//! press means nothing (logind ignores the key too: logind.conf.d/maison.conf).

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// The device tree's node.
const DEVICE: &str = "gpio-keys";
const INPUT: &str = "/sys/class/input";
const OPEN: &str = "/usr/local/sbin/maison-hotspot";
const HOLD: Duration = Duration::from_secs(10);

const EV_KEY: u16 = 1;
const KEY_POWER: u16 = 116;
/// linux/input.h's `struct input_event`: two longs of time, the type, the code, the value.
const EVENT: usize = 2 * size_of::<usize>() + 8;

/// A press of the power key (true) or its release: a key's repeats say neither.
fn power(event: &[u8]) -> Option<bool> {
    let head = event.get(EVENT - 8..EVENT)?;
    let kind = u16::from_ne_bytes([head[0], head[1]]);
    let code = u16::from_ne_bytes([head[2], head[3]]);
    let value = i32::from_ne_bytes([head[4], head[5], head[6], head[7]]);
    (kind == EV_KEY && code == KEY_POWER && value != 2).then_some(value == 1)
}

/// One press, followed: due once when it has lasted `HOLD`.
#[derive(Debug, Default)]
struct Held(Option<Instant>);

impl Held {
    fn key(&mut self, down: bool, now: Instant) {
        self.0 = down.then_some(now);
    }

    /// How long before it is due, if a press is held.
    fn left(&self, now: Instant) -> Option<Duration> {
        self.0.map(|t| (t + HOLD).saturating_duration_since(now))
    }

    fn due(&mut self, now: Instant) -> bool {
        let due = self.left(now) == Some(Duration::ZERO);
        if due {
            self.0 = None;
        }
        due
    }
}

/// The input device of that name's event node.
fn device(input: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(input).ok()?.flatten().find_map(|entry| {
        let node = entry.file_name().into_string().ok()?;
        let named = fs::read_to_string(entry.path().join("device/name")).ok()?;
        (node.starts_with("event") && named.trim() == name)
            .then(|| Path::new("/dev/input").join(node))
    })
}

fn open_hotspot() {
    match Command::new(OPEN).arg("open").status() {
        Ok(s) if s.success() => println!("the power key held: the frame's own Wi-Fi"),
        Ok(s) => eprintln!("{OPEN}: {s}"),
        Err(e) => eprintln!("{OPEN}: {e}"),
    }
}

fn run(path: &Path) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut event = [0; EVENT];
        while file.read_exact(&mut event).is_ok() {
            if let Some(down) = power(&event)
                && tx.send(down).is_err()
            {
                return;
            }
        }
    });
    let mut held = Held::default();
    loop {
        let next = match held.left(Instant::now()) {
            Some(left) => rx.recv_timeout(left),
            None => rx.recv().map_err(RecvTimeoutError::from),
        };
        match next {
            Ok(down) => held.key(down, Instant::now()),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(std::io::Error::other("the device stopped"));
            }
        }
        if held.due(Instant::now()) {
            open_hotspot();
        }
    }
}

fn main() {
    let Some(path) = device(Path::new(INPUT), DEVICE) else {
        eprintln!("no {DEVICE} input device");
        process::exit(1);
    };
    if let Err(e) = run(&path) {
        eprintln!("{}: {e}", path.display());
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: u16, code: u16, value: i32) -> Vec<u8> {
        let mut e = vec![0xaa; EVENT - 8];
        e.extend(kind.to_ne_bytes());
        e.extend(code.to_ne_bytes());
        e.extend(value.to_ne_bytes());
        e
    }

    #[test]
    fn only_the_power_key_is_heard_and_never_its_repeats() {
        assert_eq!(power(&event(EV_KEY, KEY_POWER, 1)), Some(true));
        assert_eq!(power(&event(EV_KEY, KEY_POWER, 0)), Some(false));
        assert_eq!(power(&event(EV_KEY, KEY_POWER, 2)), None);
        assert_eq!(power(&event(EV_KEY, 30, 1)), None);
        assert_eq!(power(&event(0, 0, 0)), None, "a report's end");
        assert_eq!(power(&[0; 4]), None);
    }

    #[test]
    fn ten_seconds_held_count_once_and_a_short_press_for_nothing() {
        let t0 = Instant::now();
        let mut held = Held::default();
        assert_eq!(held.left(t0), None);
        held.key(true, t0);
        assert_eq!(held.left(t0 + HOLD / 2), Some(HOLD / 2));
        assert!(!held.due(t0 + HOLD / 2));
        held.key(false, t0 + HOLD / 2);
        assert_eq!(held.left(t0 + HOLD), None);
        assert!(!held.due(t0 + HOLD), "let go in time");

        held.key(true, t0);
        assert!(held.due(t0 + HOLD));
        assert!(!held.due(t0 + HOLD * 2), "once, however long it is held");
        assert_eq!(held.left(t0 + HOLD * 2), None);
    }

    #[test]
    fn the_device_is_found_by_its_name() {
        let root = std::env::temp_dir().join(format!("power-key-{}", process::id()));
        for (node, name) in [
            ("event0", "gsl_ts\n"),
            ("event1", "gpio-keys\n"),
            ("input1", "gpio-keys\n"),
        ] {
            let dir = root.join(node).join("device");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("name"), name).unwrap();
        }
        assert_eq!(
            device(&root, DEVICE),
            Some(PathBuf::from("/dev/input/event1"))
        );
        assert_eq!(device(&root, "nothing"), None);
        fs::remove_dir_all(&root).unwrap();
    }
}
