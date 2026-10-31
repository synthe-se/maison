//! The photos kept on the frame, `<state>/photos/<id>.jpg` (0700, each file 0600, written whole or
//! not at all): what shows while Maison is away. Named by the id, the hash of their bytes: a
//! photo kept is never fetched twice. Bounded: past `MAX_BYTES` the oldest of the list stay out.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use maison_api::photos::{Photo, is_id};

use crate::client::write_private;

const DIR: &str = "photos";
const EXTENSION: &str = ".jpg";
/// At most this much on the frame's storage: Maison's whole list at the size the web app sends.
pub const MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
}

/// What brings the cache in step with a list.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// In the list, not kept: fetched in its order (the latest added first).
    pub fetch: Vec<String>,
    /// Kept, out of the list or past the limit.
    pub remove: Vec<String>,
}

/// `have` (id, bytes) against `list`: the list's photos that fit in `cap`, in its order.
pub fn plan(list: &[Photo], have: &[(String, u64)], cap: u64) -> Plan {
    let kept: HashSet<&str> = have.iter().map(|(id, _)| id.as_str()).collect();
    let mut room = cap;
    let mut wanted = HashSet::new();
    let mut fetch = Vec::new();
    for photo in list.iter().filter(|p| is_id(&p.id)) {
        if photo.bytes <= room && wanted.insert(photo.id.as_str()) {
            room -= photo.bytes;
            if !kept.contains(photo.id.as_str()) {
                fetch.push(photo.id.clone());
            }
        }
    }
    let remove = have
        .iter()
        .filter(|(id, _)| !wanted.contains(id.as_str()))
        .map(|(id, _)| id.clone())
        .collect();
    Plan { fetch, remove }
}

impl Cache {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            dir: state_dir.join(DIR),
        }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}{EXTENSION}"))
    }

    /// What is kept, by id, with its size. Anything else there (a temp file a power cut left)
    /// goes.
    pub fn kept(&self) -> Vec<(String, u64)> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Vec::new(),
            Err(e) => {
                eprintln!("{}: {e}", self.dir.display());
                return Vec::new();
            }
        };
        let mut kept = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let id = name.to_str().and_then(|n| n.strip_suffix(EXTENSION));
            match (id.filter(|id| is_id(id)), entry.metadata()) {
                (Some(id), Ok(meta)) => kept.push((id.to_string(), meta.len())),
                _ => {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        kept.sort();
        kept
    }

    pub fn ids(&self) -> Vec<String> {
        self.kept().into_iter().map(|(id, _)| id).collect()
    }

    pub fn read(&self, id: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.file(id))
    }

    pub fn save(&self, id: &str, bytes: &[u8]) -> std::io::Result<()> {
        if !is_id(id) {
            return Err(ErrorKind::InvalidInput.into());
        }
        let mut dir = std::fs::DirBuilder::new();
        dir.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut dir, 0o700);
        dir.create(&self.dir)?;
        write_private(&self.file(id), bytes)
    }

    pub fn remove(&self, id: &str) {
        match std::fs::remove_file(self.file(id)) {
            Err(e) if e.kind() != ErrorKind::NotFound => {
                eprintln!("photo {id} not removed: {e}");
            }
            _ => {}
        }
    }

    /// Everything goes (the screen was removed).
    pub fn clear(&self) {
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != ErrorKind::NotFound => {
                eprintln!("{}: {e}", self.dir.display());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temp_dir;

    fn id(n: u8) -> String {
        format!("{n:0>64}")
    }

    fn photo(n: u8, bytes: u64) -> Photo {
        Photo { id: id(n), bytes }
    }

    #[test]
    fn what_is_missing_comes_in_the_lists_order_and_what_left_goes() {
        let list = [photo(1, 10), photo(2, 10), photo(3, 10)];
        let have = [(id(2), 10), (id(9), 10)];
        assert_eq!(
            plan(&list, &have, 1000),
            Plan {
                fetch: vec![id(1), id(3)],
                remove: vec![id(9)],
            }
        );
    }

    #[test]
    fn past_its_limit_the_oldest_of_the_list_stay_out() {
        let list = [photo(1, 60), photo(2, 60), photo(3, 30)];
        let have = [(id(2), 60)];
        assert_eq!(
            plan(&list, &have, 100),
            Plan {
                fetch: vec![id(1), id(3)],
                remove: vec![id(2)],
            },
            "the latest added fit first; a small one still fits after a large one"
        );
    }

    #[test]
    fn an_id_that_is_none_never_becomes_a_file() {
        let list = [Photo {
            id: "../../etc/passwd".into(),
            bytes: 1,
        }];
        assert_eq!(plan(&list, &[], 100), Plan::default());
        let cache = Cache::new(&temp_dir("cache-id"));
        assert!(cache.save("../x", b"j").is_err());
    }

    #[test]
    fn a_photo_is_kept_whole_private_and_read_back() {
        let state = temp_dir("cache");
        let cache = Cache::new(&state);
        assert!(cache.kept().is_empty(), "no directory yet");
        cache.save(&id(1), b"jpeg").unwrap();
        cache.save(&id(1), b"jpeg").unwrap();
        assert_eq!(cache.kept(), [(id(1), 4)]);
        assert_eq!(cache.read(&id(1)).unwrap(), b"jpeg");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&state.join(DIR)), 0o700);
            assert_eq!(mode(&cache.file(&id(1))), 0o600);
        }
        cache.remove(&id(1));
        cache.remove(&id(1));
        assert!(cache.ids().is_empty());
    }

    #[test]
    fn what_a_power_cut_left_goes_and_clear_leaves_nothing() {
        let state = temp_dir("cache-torn");
        let cache = Cache::new(&state);
        cache.save(&id(1), b"jpeg").unwrap();
        let torn = state.join(DIR).join(format!(".{}.jpg.tmp", id(2)));
        std::fs::write(&torn, b"jp").unwrap();
        assert_eq!(cache.ids(), [id(1)]);
        assert!(!torn.exists());
        cache.clear();
        assert!(!state.join(DIR).exists());
        cache.clear();
    }

    #[test]
    fn the_bound_holds_maisons_whole_list_at_the_size_the_web_app_sends() {
        const { assert!(MAX_BYTES >= maison_api::photos::MAX_PHOTOS as u64 * 500 * 1024) };
    }
}
