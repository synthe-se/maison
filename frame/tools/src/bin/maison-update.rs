//! Over the air: from a timer at night, a newer signed release goes to the free slot, resumed if it
//! was cut, and the frame restarts to try it; the initramfs and maison-commit keep it or come back.
//! What failed once is never tried again. Failing here changes nothing: the next night tries again.

use maison_frame_tools::{Manifest, RELEASE_KEY_HEX, sha256_of, sums, verified, wanted};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use std::{fs, process};

const SLOTS: &str = "/android-data/maison/slots";
const CONFIG: &str = "/etc/maison-update";
const VERSION: &str = "/etc/maison-version";

fn main() {
    match run() {
        Ok(said) => println!("{said}"),
        Err(why) => {
            eprintln!("update: {why}");
            process::exit(1);
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(None)
        .timeout_per_call(None)
        .build()
        .into()
}

fn bytes(agent: &ureq::Agent, url: &str, cap: u64) -> Result<Vec<u8>, String> {
    let mut answer = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    answer
        .body_mut()
        .with_config()
        .limit(cap)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

fn slot_command(args: &[&str]) -> Result<String, String> {
    let out = Command::new("/usr/local/sbin/maison-slot")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "maison-slot {args:?}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn state_key(state: &str, key: &str) -> String {
    state
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")))
        .unwrap_or("")
        .to_string()
}

/// A failed slot's version, moved to `refused` once: `failed` is the initramfs's word for it.
fn refused(slots: &Path) -> Vec<u64> {
    let list = slots.join("refused");
    if let Ok(slot) = fs::read_to_string(slots.join("failed"))
        && let Ok(version) = fs::read_to_string(slots.join(slot.trim()).join("version"))
    {
        let known = fs::read_to_string(&list).unwrap_or_default();
        let _ = fs::write(&list, format!("{known}{}\n", version.trim()));
        let _ = fs::remove_file(slots.join("failed"));
    }
    fs::read_to_string(&list)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

fn run() -> Result<String, String> {
    let base = fs::read_to_string(CONFIG).map_err(|e| format!("{CONFIG}: {e}"))?;
    let base = base.trim().trim_end_matches('/').to_string();
    let running: u64 = fs::read_to_string(VERSION)
        .map_err(|e| format!("{VERSION}: {e}"))?
        .trim()
        .parse()
        .map_err(|_| format!("{VERSION}: not a number"))?;
    let slots = PathBuf::from(SLOTS);
    let state = fs::read_to_string(slots.join("state")).map_err(|e| e.to_string())?;
    if !state_key(&state, "pending").is_empty() {
        return Ok("an update is on trial".into());
    }
    let agent = agent();
    let manifest_bytes = bytes(&agent, &format!("{base}/manifest.json"), 1 << 16)?;
    let signature = bytes(&agent, &format!("{base}/manifest.sig"), 256)?;
    let manifest =
        verified(&manifest_bytes, &signature, RELEASE_KEY_HEX).map_err(|e| format!("{e:?}"))?;
    if !wanted(manifest.version, running, &refused(&slots)) {
        return Ok(format!("nothing newer than {running}"));
    }
    let slot = slot_command(&["free"])?;
    let dir = slots.join(&slot);
    stage(&agent, &base, &manifest, &dir)?;
    slot_command(&["offer", &slot])?;
    Command::new("systemctl")
        .arg("reboot")
        .status()
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "{} in slot {slot}: restarting to try it",
        manifest.version
    ))
}

/// The slot holds this release, whole, sums last so that `whole` never passes a half of it. What
/// another version left there goes; the same version's cut download goes on.
fn stage(agent: &ureq::Agent, base: &str, manifest: &Manifest, dir: &Path) -> Result<(), String> {
    let marker = dir.join("version");
    if fs::read_to_string(&marker).ok().as_deref().map(str::trim)
        != Some(&manifest.version.to_string())
    {
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        fs::write(&marker, manifest.version.to_string()).map_err(|e| e.to_string())?;
    }
    let _ = fs::remove_file(dir.join("sums"));
    for file in &manifest.files {
        let path = dir.join(&file.name);
        if sha256_of(&path).ok().as_deref() == Some(file.sha256.as_str()) {
            continue;
        }
        let url = format!("{base}/{}/{}", manifest.version, file.name);
        let part = dir.join(format!("{}.part", file.name));
        fetch(agent, &url, &part, file.size)?;
        if sha256_of(&part).map_err(|e| e.to_string())? != file.sha256 {
            let _ = fs::remove_file(&part);
            return Err(format!("{}: not the release's", file.name));
        }
        fs::rename(&part, &path).map_err(|e| e.to_string())?;
    }
    fs::write(dir.join("sums"), sums(&manifest.files)).map_err(|e| e.to_string())?;
    Command::new("sync").status().map_err(|e| e.to_string())?;
    Ok(())
}

fn fetch(agent: &ureq::Agent, url: &str, part: &Path, size: u64) -> Result<(), String> {
    let have = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if have >= size {
        return Ok(());
    }
    let mut answer = agent
        .get(url)
        .header("Range", format!("bytes={have}-"))
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    // a server without ranges starts over
    let resumed = answer.status().as_u16() == 206;
    let mut out = fs::OpenOptions::new()
        .create(true)
        .append(resumed)
        .write(true)
        .truncate(!resumed)
        .open(part)
        .map_err(|e| e.to_string())?;
    let mut body = answer.body_mut().with_config().limit(size + 1).reader();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = body.read(&mut buffer).map_err(|e| format!("{url}: {e}"))?;
        if read == 0 {
            return Ok(());
        }
        out.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("maison-update-{name}-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// One file over plain HTTP, ranges honoured when `ranges`.
    fn serve(content: Vec<u8>, ranges: bool, answers: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().take(answers) {
                let mut stream = stream.unwrap();
                let mut head = [0; 2048];
                let read = stream.read(&mut head).unwrap();
                let head = String::from_utf8_lossy(&head[..read]).to_string();
                let from = head
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("range: bytes=")
                            .or(l.strip_prefix("Range: bytes="))
                    })
                    .and_then(|r| r.trim_end_matches('-').parse::<usize>().ok())
                    .filter(|_| ranges);
                let body = &content[from.unwrap_or(0)..];
                let status = if from.is_some() {
                    "206 Partial Content"
                } else {
                    "200 OK"
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            }
        });
        address
    }

    #[test]
    fn a_cut_download_goes_on_where_it_stopped() {
        let dir = temp("resume");
        let part = dir.join("x.part");
        fs::write(&part, b"hello ").unwrap();
        let url = serve(b"hello world".to_vec(), true, 1);
        fetch(&agent(), &url, &part, 11).unwrap();
        assert_eq!(fs::read(&part).unwrap(), b"hello world");
    }

    #[test]
    fn a_server_without_ranges_starts_over() {
        let dir = temp("over");
        let part = dir.join("x.part");
        fs::write(&part, b"junk").unwrap();
        let url = serve(b"hello world".to_vec(), false, 1);
        fetch(&agent(), &url, &part, 11).unwrap();
        assert_eq!(fs::read(&part).unwrap(), b"hello world");
    }

    #[test]
    fn a_file_that_is_not_the_releases_stops_the_stage_and_no_sums_are_written() {
        let dir = temp("stage");
        let slot = dir.join("b");
        let url = serve(b"bad".to_vec(), false, 1);
        let manifest = Manifest {
            version: 9,
            files: vec![maison_frame_tools::File {
                name: "rootfs.squashfs".into(),
                sha256: "00".repeat(32),
                size: 3,
            }],
        };
        // the URL is the file's: base + version + name lead to the same one-file server
        assert!(stage(&agent(), &url, &manifest, &slot).is_err());
        assert!(!slot.join("sums").exists());
        assert!(!slot.join("rootfs.squashfs").exists());
    }

    #[test]
    fn a_failed_slots_version_is_refused_for_good() {
        let dir = temp("refused");
        fs::create_dir_all(dir.join("b")).unwrap();
        fs::write(dir.join("b/version"), "12").unwrap();
        fs::write(dir.join("failed"), "b\n").unwrap();
        assert_eq!(refused(&dir), vec![12]);
        assert!(!dir.join("failed").exists());
        assert_eq!(refused(&dir), vec![12]);
    }

    #[test]
    fn the_state_reads_as_the_slots_write_it() {
        let state = "active=a\npending=\ntries=0\nboots=1\n";
        assert_eq!(state_key(state, "active"), "a");
        assert_eq!(state_key(state, "pending"), "");
    }
}
