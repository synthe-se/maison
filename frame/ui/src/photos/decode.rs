//! Reading a kept JPEG, off the drawing thread: one thread, one photo at a time, its pixels handed
//! over once and dropped as soon as they are on the GPU.

use std::sync::Arc;
use std::sync::mpsc::{self, Sender};

use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use super::Said;
use super::cache::Cache;

/// The largest side a picture may have: Mali-400's textures end there, and an image that says it
/// is bigger is no reason to fill the frame's memory (the web app sends at most the panel's size).
const MAX_SIDE: usize = 4096;

/// A decoded photo: straight RGBA, as a texture takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// `bytes`' pixels, or why not.
pub fn decode(id: &str, bytes: &[u8]) -> Result<Picture, String> {
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGBA)
        .set_max_width(MAX_SIDE)
        .set_max_height(MAX_SIDE);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let rgba = decoder.decode().map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("no size in its headers")?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(format!("{} bytes for {width}×{height}", rgba.len()));
    }
    Ok(Picture {
        id: id.to_string(),
        width,
        height,
        rgba,
    })
}

/// The reader's thread, for the app's life: an id in, its picture out, then a word to the loop.
pub fn start(
    cache: Cache,
    said: Sender<Said>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Sender<String> {
    let (tx, jobs) = mpsc::channel::<String>();
    let spawned = std::thread::Builder::new()
        .name("photos: decode".into())
        .spawn(move || {
            for id in jobs {
                let read = cache.read(&id).map_err(|e| e.to_string());
                let out = match read.and_then(|bytes| decode(&id, &bytes)) {
                    Ok(picture) => Said::Decoded(picture),
                    Err(e) => {
                        eprintln!("photo {id} skipped: {e}");
                        Said::Undecoded(id)
                    }
                };
                if said.send(out).is_err() {
                    return;
                }
                wake();
            }
        });
    if let Err(e) = spawned {
        eprintln!("the photos' reader could not start: {e}");
    }
    tx
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testing::{temp_dir, wait_until};

    /// A real JPEG: 8×12, red.
    pub(crate) const RED: &[u8] = include_bytes!("red.jpg");

    #[test]
    fn a_jpeg_comes_out_as_opaque_rgba_of_its_size() {
        let p = decode("a", RED).unwrap();
        assert_eq!((p.id.as_str(), p.width, p.height), ("a", 8, 12));
        assert_eq!(p.rgba.len(), 8 * 12 * 4);
        let px = &p.rgba[..4];
        assert!(px[0] > 200 && px[1] < 60 && px[2] < 60, "{px:?}");
        assert!(p.rgba.chunks(4).all(|px| px[3] == 255));
    }

    #[test]
    fn what_is_no_jpeg_says_why_and_breaks_nothing() {
        assert!(decode("a", b"not a jpeg").is_err());
        assert!(decode("a", &[]).is_err());
        assert!(decode("a", &RED[..RED.len() / 3]).is_err(), "torn");
    }

    #[test]
    fn the_thread_hands_over_each_picture_and_says_the_ones_it_cannot_read() {
        let cache = Cache::new(&temp_dir("decode"));
        let id = "a".repeat(64);
        let (bad, missing) = ("b".repeat(64), "c".repeat(64));
        cache.save(&id, RED).unwrap();
        cache.save(&bad, b"junk").unwrap();
        let (tx, rx) = mpsc::channel();
        let jobs = start(cache, tx, Arc::new(|| {}));
        for job in [&id, &bad, &missing] {
            jobs.send(job.clone()).unwrap();
        }
        let mut said = Vec::new();
        wait_until(|| {
            said.extend(rx.try_iter());
            said.len() == 3
        });
        assert!(matches!(&said[0], Said::Decoded(p) if p.id == id));
        assert!(matches!(&said[1], Said::Undecoded(b) if *b == bad));
        assert!(matches!(&said[2], Said::Undecoded(m) if *m == missing));
    }
}
