//! The wall frame's photos (« Cadre photo »): what an admin adds from the web app and every
//! frame shows while nobody touches it. A photo's id is the SHA-256 of its bytes, in lowercase
//! hexadecimal: the same bytes are the same photo, so a frame caches by id for good. The browser
//! resizes before it sends; the server never decodes a picture.

use serde::{Deserialize, Serialize};

/// The multipart field `POST /api/photos` reads the JPEG from.
pub const UPLOAD_FIELD: &str = "photo";

/// One photo's bytes at most: well above a JPEG the size of the frame's panel at quality 0.85
/// (under 1 MB), and held in memory while it comes in on a 512 MB Pi.
pub const MAX_PHOTO_BYTES: usize = 2 * 1024 * 1024;

/// Photos the house keeps at most: the Pi's SD card and the frames' caches stay bounded.
pub const MAX_PHOTOS: usize = 500;

/// Hexadecimal characters of a SHA-256.
const ID_LEN: usize = 64;

/// Whether `id` is a photo's id: nothing else ever reaches a file's name.
pub fn is_id(id: &str) -> bool {
    id.len() == ID_LEN && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A kept photo: its id, and how heavy it is (what a frame's bounded cache counts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub id: String,
    pub bytes: u64,
}

/// `GET /api/photos`, the latest added first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoList {
    pub photos: Vec<Photo>,
}

/// `POST /api/photos`: the photo kept (the one already there, for the same bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Added {
    pub photo: Photo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_a_lowercase_sha256_and_nothing_else() {
        let good = "0123456789abcdef".repeat(4);
        assert!(is_id(&good));
        assert!(!is_id(&good[1..]), "too short");
        assert!(!is_id(&good.to_uppercase()));
        assert!(!is_id(&format!("../{}", &good[3..])), "never a path");
        assert!(!is_id(""));
    }

    #[test]
    fn the_list_is_camel_case_and_reads_back() {
        let list = PhotoList {
            photos: vec![Photo {
                id: "ab".into(),
                bytes: 12,
            }],
        };
        let json = serde_json::to_value(&list).unwrap();
        assert_eq!(json["photos"][0]["bytes"], 12);
        assert_eq!(serde_json::from_value::<PhotoList>(json).unwrap(), list);
    }
}
