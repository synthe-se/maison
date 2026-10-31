//! A sign of life for the update system (`MAISON_HEARTBEAT`, a path): a file touched once a frame
//! was presented, then at most every `EVERY` while the loop runs, frames or not (idle, the app
//! draws nothing and is well). It cannot write: said once, never fatal.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

pub const EVERY: Duration = Duration::from_secs(10);

pub struct Heartbeat {
    path: PathBuf,
    /// The last beat; none until a frame was presented.
    last: Option<Instant>,
    said: bool,
}

impl Heartbeat {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            last: None,
            said: false,
        }
    }

    /// A frame was presented.
    pub fn presented(&mut self, now: Instant) {
        if self.last.is_none_or(|t| now >= t + EVERY) {
            self.last = Some(now);
            if let Err(e) = touch(&self.path)
                && !std::mem::replace(&mut self.said, true)
            {
                eprintln!("the heartbeat {}: {e}", self.path.display());
            }
        }
    }

    /// The loop turned: a beat if one is due, once there was a first frame.
    pub fn idle(&mut self, now: Instant) {
        if self.last.is_some() {
            self.presented(now);
        }
    }

    /// When the loop must wake for the next beat.
    pub fn next(&self) -> Option<Instant> {
        self.last.map(|t| t + EVERY)
    }
}

/// The file's time now, the file made if it was not there.
fn touch(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .set_modified(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temp_dir;

    #[test]
    fn it_beats_from_the_first_frame_at_most_every_ten_seconds() {
        let file = temp_dir("heartbeat").join("beat");
        let mut h = Heartbeat::new(file.clone());
        let t0 = Instant::now();
        h.idle(t0);
        assert!(!file.exists(), "no frame yet");
        assert_eq!(h.next(), None, "nothing to wake for");
        h.presented(t0);
        assert!(file.exists());
        assert_eq!(h.next(), Some(t0 + EVERY));
        std::fs::remove_file(&file).unwrap();
        h.presented(t0 + Duration::from_secs(5));
        assert!(!file.exists(), "not so often");
        // idle: no frame, still a sign of life
        h.idle(t0 + EVERY);
        assert!(file.exists());
        assert_eq!(h.next(), Some(t0 + EVERY * 2));
    }

    #[test]
    fn a_touch_moves_the_files_time_and_keeps_its_contents() {
        let file = temp_dir("heartbeat-time").join("beat");
        std::fs::write(&file, "kept").unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(old)
            .unwrap();
        Heartbeat::new(file.clone()).presented(Instant::now());
        let meta = std::fs::metadata(&file).unwrap();
        assert!(meta.modified().unwrap() > old);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "kept");
    }

    #[test]
    fn a_failure_is_said_once_and_nothing_more() {
        let file = temp_dir("heartbeat-fail").join("no/such/dir/beat");
        let mut h = Heartbeat::new(file);
        let t0 = Instant::now();
        h.presented(t0);
        assert!(h.said);
        h.said = false;
        h.presented(t0 + Duration::from_secs(1));
        assert!(!h.said, "not due");
    }
}
