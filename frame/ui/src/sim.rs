//! A simulation's hands and eyes (`MAISON_STDIN=1`, e2e/frame.ts): commands on stdin, one per
//! line, act as a finger would; the app says on stdout what it shows. Off on the frame: nothing
//! here runs, and the pairing code is never written to the journal.
//!
//! - `tap X Y`: a finger down and up there (the panel's pixels, as the picture shows them)
//! - `press KEY`: a tap on the control whose gesture is KEY (`hue-lamp:hue-01:power`, `open:tv`,
//!   `jump:lamps`, `ask:tuya:f1:water`, `dialog:sure`)
//! - `slide KEY SHARE`: the slider KEY taken from its far end and let go at SHARE (0..1) of its band
//! - `drag Y0 Y1`: a finger from Y0 to Y1 down the middle (the column scrolls)
//! - `shot PATH`: the next drawing saved there as PNG
//! - `keys`: every control's key, one line each
//! - `idle`: as if nobody had touched the panel for `DIM_AFTER` (no two-minute wait in a run)
//! - `turn`: the photo on the panel's time is up
//! - `type TEXT`: TEXT in the keyboard's field, as keys would type it (`press kb:a`, one key)
//!
//! Said: `screen house`, `screen photos ID`, `screen door TITLE`, `screen wifi WORD`, `screen password NETWORK`, `code CODE` (a pairing code shown), `shot PATH`,
//! `ok COMMAND` or `error COMMAND: why`.

use std::io::BufRead;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use crate::app::{App, FRAME};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Tap(f32, f32),
    Press(String),
    Slide(String, f32),
    Drag(f32, f32),
    Shot(String),
    Keys,
    Idle,
    Turn,
    Type(String),
}

pub fn parse(line: &str) -> Result<Command, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let num = |s: &str| s.parse::<f32>().map_err(|e| format!("{s}: {e}"));
    match words.as_slice() {
        ["tap", x, y] => Ok(Command::Tap(num(x)?, num(y)?)),
        ["press", key] => Ok(Command::Press((*key).into())),
        ["slide", key, share] => Ok(Command::Slide((*key).into(), num(share)?)),
        ["drag", y0, y1] => Ok(Command::Drag(num(y0)?, num(y1)?)),
        ["shot", path] => Ok(Command::Shot((*path).into())),
        ["keys"] => Ok(Command::Keys),
        ["idle"] => Ok(Command::Idle),
        ["turn"] => Ok(Command::Turn),
        ["type", text] => Ok(Command::Type((*text).into())),
        _ => Err(format!("unknown command: {line}")),
    }
}

/// Reads stdin on its own thread; each line wakes the loop.
pub fn listen(wake: Arc<dyn Fn() + Send + Sync>) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { return };
            if tx.send(line).is_err() {
                return;
            }
            wake();
        }
    });
    rx
}

/// Runs one command; a shot asked is returned (taken after the next drawing).
pub fn run(app: &mut App, c: Command) -> Result<Option<String>, String> {
    let now = Instant::now();
    // a finger's points a frame apart
    let step = FRAME;
    match c {
        Command::Tap(x, y) => {
            app.finger_down(x, y, now);
            app.finger_up(x, y, now + step);
        }
        Command::Press(key) => {
            let (x, y) = app.center_of(&key).ok_or(format!("no control {key}"))?;
            app.finger_down(x, y, now);
            app.finger_up(x, y, now + step);
        }
        Command::Slide(key, share) => {
            let (x0, x1, y) = app.band_of(&key).ok_or(format!("no slider {key}"))?;
            let to = x0 + (x1 - x0) * share.clamp(0.0, 1.0);
            // a slider moves only once the finger left the slop sideways
            let from = if share < 0.5 { x1 } else { x0 };
            app.finger_down(from, y, now);
            app.finger_motion(to, y, now + step);
            app.finger_up(to, y, now + step * 2);
        }
        Command::Drag(y0, y1) => {
            let x = 10.0;
            app.finger_down(x, y0, now);
            for i in 1..=10 {
                let y = y0 + (y1 - y0) * i as f32 / 10.0;
                app.finger_motion(x, y, now + step * i);
            }
            app.finger_up(x, y1, now + step * 11);
        }
        Command::Shot(path) => return Ok(Some(path)),
        Command::Keys => {
            for (key, x, y) in app.keys() {
                println!("key {key} {x:.0} {y:.0}");
            }
        }
        Command::Idle => app.nobody_since(now),
        Command::Turn => app.photo_turn(now),
        Command::Type(text) => app.type_text(&text),
    }
    app.dirty = true;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_read() {
        assert_eq!(parse("tap 10 20.5"), Ok(Command::Tap(10.0, 20.5)));
        assert_eq!(
            parse("press hue-lamp:1:power"),
            Ok(Command::Press("hue-lamp:1:power".into()))
        );
        assert_eq!(parse("slide k 0.5"), Ok(Command::Slide("k".into(), 0.5)));
        assert_eq!(parse("drag 900 300"), Ok(Command::Drag(900.0, 300.0)));
        assert_eq!(
            parse("shot /tmp/a.png"),
            Ok(Command::Shot("/tmp/a.png".into()))
        );
        assert_eq!(parse("keys"), Ok(Command::Keys));
        assert_eq!(parse("idle"), Ok(Command::Idle));
        assert_eq!(parse("turn"), Ok(Command::Turn));
        assert_eq!(parse("type abc"), Ok(Command::Type("abc".into())));
        assert!(parse("fly").is_err());
        assert!(parse("tap x 1").is_err());
    }
}
