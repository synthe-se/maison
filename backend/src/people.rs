//! Who may come in, and with what: people, their passkeys, and pending invitations, in one
//! file (`auth.json`, 0600). There are no passwords: a person exists once an invitation has
//! registered their first passkey (`routes::passkeys`, after Ariane's passkey mode).
//!
//! The file is the truth, read on every use and replaced whole (`store::write_json`) under
//! an OS lock (`store::locked`): the server and `maison-backend invite` (run as root on the
//! Pi) both edit it. It lives in a directory the service user owns (`auth/`: the temp file
//! is made there); the replacement keeps the owner. An empty or torn file is an error,
//! never « nobody ».

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    error::AppError,
    store::{self, Access, Corrupt},
    util::constant_time_eq,
};

/// What someone may do: an admin invites and configures, a member uses the house.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
}

/// Characters in a name (a person's, a passkey's), at most.
pub const MAX_NAME: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: String,
    pub name: String,
    pub role: Role,
    /// The WebAuthn user handle (a random UUID, never shown): what a passkey says it is for.
    pub user_handle: String,
    /// Sessions issued before this (ms) are over: « sign out everywhere », a removed passkey.
    #[serde(default)]
    pub not_before_ms: i64,
}

/// What the admins see of someone.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersonInfo {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub passkeys: usize,
}

/// A registered passkey: webauthn-rs's credential as JSON, and what the list shows of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPasskey {
    pub id: String,
    pub person: String,
    /// base64url, as webauthn-rs writes it: the lookup key at sign-in.
    pub credential_id: String,
    pub passkey: serde_json::Value,
    pub name: String,
    pub backed_up: bool,
    pub created_ms: i64,
    pub last_used_ms: Option<i64>,
}

/// What the account page shows of a passkey.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyInfo {
    pub id: String,
    pub name: String,
    /// Synced by a keychain (iCloud, Google, a password manager).
    pub backed_up: bool,
    pub created_ms: i64,
    pub last_used_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredInvite {
    pub id: String,
    /// `hash_secret` of the link's token: the token itself is never kept.
    pub token_hash: String,
    pub person: String,
    pub name: String,
    pub role: Role,
    pub created_by: Option<String>,
    pub created_ms: i64,
    pub expires_ms: i64,
    pub used_ms: Option<i64>,
}

/// What the admins see of a pending invitation.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    pub id: String,
    pub person: String,
    pub name: String,
    pub role: Role,
    pub expires_ms: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Book {
    #[serde(default)]
    people: Vec<Person>,
    #[serde(default)]
    passkeys: Vec<StoredPasskey>,
    #[serde(default)]
    invites: Vec<StoredInvite>,
}

/// Why a passkey could not be removed.
#[derive(Debug, PartialEq, Eq)]
pub enum Removed {
    Yes,
    NotFound,
    /// A person keeps one passkey at least.
    Last,
}

/// A credential already registered (here, for anyone).
#[derive(Debug, PartialEq, Eq)]
pub struct PasskeyTaken;

pub struct People {
    path: PathBuf,
    /// The book as last read, and the file's (modified, length) then: every request asks
    /// who is signed in, the file is read again only when it changed.
    cache: Mutex<Option<(Stamp, Book)>>,
}

type Stamp = Option<(std::time::SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

impl People {
    pub fn new(path: &Path) -> Self {
        Self { path: path.to_path_buf(), cache: Mutex::new(None) }
    }

    async fn read<T>(&self, f: impl FnOnce(&Book) -> T) -> Result<T, AppError> {
        let mut cache = self.cache.lock().await;
        let now = stamp(&self.path);
        match cache.as_ref() {
            Some((at, book)) if *at == now && now.is_some() => Ok(f(book)),
            _ => {
                let book: Book = store::read_json(&self.path, Corrupt::Fail)?;
                let out = f(&book);
                *cache = Some((now, book));
                Ok(out)
            }
        }
    }

    /// Reads, changes, writes back (when `f` says so): one task at a time here, one process
    /// at a time on the file, off the runtime (the OS lock may wait, the write fsyncs).
    async fn edit<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Book) -> (T, bool) + Send + 'static,
    ) -> Result<T, AppError> {
        let mut cache = self.cache.lock().await;
        let path = self.path.clone();
        let (out, book, at) = store::locked_async(&self.path, move || {
            let mut book: Book = store::read_json(&path, Corrupt::Fail)?;
            let (out, changed) = f(&mut book);
            if changed {
                store::write_json(&path, &book, Access::Private)?;
            }
            Ok((out, book, stamp(&path)))
        })
        .await?;
        *cache = Some((at, book));
        Ok(out)
    }

    /// Everyone, for the admins.
    pub async fn people(&self) -> Result<Vec<PersonInfo>, AppError> {
        self.read(|b| {
            b.people
                .iter()
                .map(|p| PersonInfo {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    role: p.role,
                    passkeys: b.passkeys.iter().filter(|k| k.person == p.id).count(),
                })
                .collect()
        })
        .await
    }

    /// The person, their passkeys and pending invitations go. False when unknown.
    pub async fn remove_person(&self, id: &str) -> Result<bool, AppError> {
        let id = id.to_string();
        self.edit(move |b| {
            let before = b.people.len();
            b.people.retain(|p| p.id != id);
            let gone = b.people.len() != before;
            if gone {
                b.passkeys.retain(|k| k.person != id);
                b.invites.retain(|i| i.person != id);
            }
            (gone, gone)
        })
        .await
    }

    /// Every session of the person issued until now is over.
    pub async fn end_sessions(&self, id: &str, now_ms: i64) -> Result<(), AppError> {
        let id = id.to_string();
        self.edit(move |b| match b.people.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.not_before_ms = now_ms;
                ((), true)
            }
            None => ((), false),
        })
        .await
    }

    pub async fn person(&self, id: &str) -> Result<Option<Person>, AppError> {
        self.read(|b| b.people.iter().find(|p| p.id == id).cloned()).await
    }

    /// The credential ids a person already has (not to register twice on one device).
    pub async fn credential_ids_of(&self, person: &str) -> Result<Vec<String>, AppError> {
        self.read(|b| b.passkeys.iter().filter(|k| k.person == person).map(|k| k.credential_id.clone()).collect())
            .await
    }

    pub async fn passkeys_of(&self, person: &str) -> Result<Vec<PasskeyInfo>, AppError> {
        self.read(|b| b.passkeys.iter().filter(|k| k.person == person).map(info).collect()).await
    }

    /// The passkey with this credential id, and whose it is.
    pub async fn passkey_by_credential(&self, credential_id: &str) -> Result<Option<(StoredPasskey, Person)>, AppError> {
        self.read(|b| {
            let key = b.passkeys.iter().find(|k| k.credential_id == credential_id)?;
            let person = b.people.iter().find(|p| p.id == key.person)?;
            Some((key.clone(), person.clone()))
        })
        .await
    }

    /// After a sign-in: when, and the credential's new counter and backup state if they moved.
    pub async fn passkey_used(&self, id: &str, passkey: Option<(serde_json::Value, bool)>, now_ms: i64) -> Result<(), AppError> {
        let id = id.to_string();
        self.edit(move |b| {
            let Some(key) = b.passkeys.iter_mut().find(|k| k.id == id) else { return ((), false) };
            key.last_used_ms = Some(now_ms);
            if let Some((json, backed_up)) = passkey {
                key.passkey = json;
                key.backed_up = backed_up;
            }
            ((), true)
        })
        .await
    }

    /// Another passkey for someone who has one already.
    pub async fn add_passkey(&self, key: StoredPasskey) -> Result<Result<(), PasskeyTaken>, AppError> {
        self.edit(move |b| {
            if b.passkeys.iter().any(|k| k.credential_id == key.credential_id) {
                return (Err(PasskeyTaken), false);
            }
            b.passkeys.push(key);
            (Ok(()), true)
        })
        .await
    }

    pub async fn rename_passkey(&self, person: &str, id: &str, name: &str) -> Result<Option<PasskeyInfo>, AppError> {
        let (person, id, name) = (person.to_string(), id.to_string(), name.to_string());
        self.edit(move |b| match b.passkeys.iter_mut().find(|k| k.person == person && k.id == id) {
            Some(key) => {
                key.name = name;
                (Some(info(key)), true)
            }
            None => (None, false),
        })
        .await
    }

    pub async fn delete_passkey(&self, person: &str, id: &str) -> Result<Removed, AppError> {
        let (person, id) = (person.to_string(), id.to_string());
        self.edit(move |b| {
            let mine = b.passkeys.iter().filter(|k| k.person == person).count();
            match b.passkeys.iter().position(|k| k.person == person && k.id == id) {
                None => (Removed::NotFound, false),
                Some(_) if mine <= 1 => (Removed::Last, false),
                Some(i) => {
                    b.passkeys.remove(i);
                    (Removed::Yes, true)
                }
            }
        })
        .await
    }

    // ---------- invitations ----------

    /// A person's user handle: theirs if they exist, else `fresh` (kept at redemption).
    pub async fn user_handle(&self, person: &str, fresh: &str) -> Result<String, AppError> {
        self.read(|b| b.people.iter().find(|p| p.id == person).map_or_else(|| fresh.to_string(), |p| p.user_handle.clone()))
            .await
    }

    pub async fn create_invite(&self, invite: StoredInvite) -> Result<(), AppError> {
        self.edit(move |b| {
            let now = invite.created_ms;
            // the spent and the expired go; a new link for someone replaces their pending one
            b.invites.retain(|i| i.used_ms.is_none() && i.expires_ms > now && i.person != invite.person);
            b.invites.push(invite);
            ((), true)
        })
        .await
    }

    /// The pending invitation whose token hashes to `token_hash`.
    pub async fn invite_by_hash(&self, token_hash: &str, now_ms: i64) -> Result<Option<StoredInvite>, AppError> {
        self.read(|b| b.invites.iter().find(|i| pending(i, now_ms) && same_hash(&i.token_hash, token_hash)).cloned())
            .await
    }

    pub async fn invites(&self, now_ms: i64) -> Result<Vec<Invite>, AppError> {
        self.read(|b| {
            b.invites
                .iter()
                .filter(|i| pending(i, now_ms))
                .map(|i| Invite {
                    id: i.id.clone(),
                    person: i.person.clone(),
                    name: i.name.clone(),
                    role: i.role,
                    expires_ms: i.expires_ms,
                })
                .collect()
        })
        .await
    }

    pub async fn delete_invite(&self, id: &str) -> Result<bool, AppError> {
        let id = id.to_string();
        self.edit(move |b| {
            let before = b.invites.len();
            b.invites.retain(|i| i.id != id);
            let gone = b.invites.len() != before;
            (gone, gone)
        })
        .await
    }

    /// Spends the invitation and registers its passkey, at once: the person is created
    /// (or, coming back after a loss, keeps their name and handle; an admin stays one).
    /// `None` when the invitation is no longer pending (used meanwhile, expired, revoked).
    pub async fn redeem_invite(
        &self,
        invite_id: &str,
        user_handle: &str,
        key: StoredPasskey,
        now_ms: i64,
    ) -> Result<Option<Result<Person, PasskeyTaken>>, AppError> {
        let (invite_id, user_handle) = (invite_id.to_string(), user_handle.to_string());
        self.edit(move |b| {
            let Some(invite) = b.invites.iter_mut().find(|i| i.id == invite_id && pending(i, now_ms)) else {
                return (None, false);
            };
            if b.passkeys.iter().any(|k| k.credential_id == key.credential_id) {
                return (Some(Err(PasskeyTaken)), false);
            }
            invite.used_ms = Some(now_ms);
            let invite = invite.clone();
            let person = match b.people.iter_mut().find(|p| p.id == invite.person) {
                Some(p) => {
                    if invite.role == Role::Admin {
                        p.role = Role::Admin;
                    }
                    p.clone()
                }
                None => {
                    let p = Person {
                        id: invite.person.clone(),
                        name: invite.name.clone(),
                        role: invite.role,
                        user_handle,
                        not_before_ms: 0,
                    };
                    b.people.push(p.clone());
                    p
                }
            };
            b.passkeys.push(key);
            (Some(Ok(person)), true)
        })
        .await
    }
}

fn info(k: &StoredPasskey) -> PasskeyInfo {
    PasskeyInfo {
        id: k.id.clone(),
        name: k.name.clone(),
        backed_up: k.backed_up,
        created_ms: k.created_ms,
        last_used_ms: k.last_used_ms,
    }
}

fn pending(i: &StoredInvite, now_ms: i64) -> bool {
    i.used_ms.is_none() && i.expires_ms > now_ms
}

/// Two hex hashes compared in constant time (the lookup is by hash, which says nothing of
/// the token; constant time anyway).
fn same_hash(a: &str, b: &str) -> bool {
    constant_time_eq(a.as_bytes(), b.as_bytes())
}

/// A person's id from their name: lowercase ASCII letters and digits, dashes between
/// (« Léonard » → `leonard`).
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        let base = match c {
            'à' | 'á' | 'â' | 'ä' | 'À' | 'Á' | 'Â' | 'Ä' => 'a',
            'ç' | 'Ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'Î' | 'Ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' | 'Ô' | 'Ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ñ' => 'n',
            c => c.to_ascii_lowercase(),
        };
        if base.is_ascii_alphanumeric() {
            out.push(base);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// A name as kept: trimmed, not empty, `MAX_NAME` characters at most, no control characters.
pub fn clean_name(name: &str) -> Option<&str> {
    let name = name.trim();
    let ok = !name.is_empty() && name.chars().count() <= MAX_NAME && !name.chars().any(char::is_control);
    ok.then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{hash_secret, random_secret, test_dir};

    fn key(id: &str, person: &str, credential: &str) -> StoredPasskey {
        StoredPasskey {
            id: id.into(),
            person: person.into(),
            credential_id: credential.into(),
            passkey: serde_json::json!({}),
            name: String::new(),
            backed_up: false,
            created_ms: 0,
            last_used_ms: None,
        }
    }

    fn invite(id: &str, person: &str, role: Role, token: &str) -> StoredInvite {
        StoredInvite {
            id: id.into(),
            token_hash: hash_secret(token),
            person: person.into(),
            name: "Léonard".into(),
            role,
            created_by: None,
            created_ms: 0,
            expires_ms: 1_000,
            used_ms: None,
        }
    }

    fn people() -> (tempfile::TempDir, People) {
        let dir = test_dir();
        let people = People::new(&dir.path().join("auth").join("auth.json"));
        (dir, people)
    }

    #[tokio::test]
    async fn an_invitation_is_spent_once_and_creates_the_person() {
        let (_dir, p) = people();
        p.create_invite(invite("i1", "leonard", Role::Admin, "t")).await.unwrap();
        assert!(p.invite_by_hash(&hash_secret("t"), 10).await.unwrap().is_some());
        assert!(p.invite_by_hash(&hash_secret("t"), 1_000).await.unwrap().is_none(), "expired");
        let person = p.redeem_invite("i1", "h", key("k1", "leonard", "c1"), 10).await.unwrap().unwrap().unwrap();
        assert_eq!((person.id.as_str(), person.role, person.user_handle.as_str()), ("leonard", Role::Admin, "h"));
        assert!(p.redeem_invite("i1", "h", key("k2", "leonard", "c2"), 11).await.unwrap().is_none(), "single use");
        assert!(p.invite_by_hash(&hash_secret("t"), 10).await.unwrap().is_none());
        assert_eq!(p.passkeys_of("leonard").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn coming_back_keeps_the_person_and_an_admin_stays_one() {
        let (_dir, p) = people();
        p.create_invite(invite("i1", "leonard", Role::Admin, "a")).await.unwrap();
        p.redeem_invite("i1", "h1", key("k1", "leonard", "c1"), 1).await.unwrap().unwrap().unwrap();
        p.create_invite(invite("i2", "leonard", Role::Member, "b")).await.unwrap();
        let again = p.redeem_invite("i2", "h2", key("k2", "leonard", "c2"), 2).await.unwrap().unwrap().unwrap();
        assert_eq!((again.role, again.user_handle.as_str()), (Role::Admin, "h1"));
        assert_eq!(p.user_handle("leonard", "fresh").await.unwrap(), "h1");
        assert_eq!(p.user_handle("alex", "fresh").await.unwrap(), "fresh");
    }

    #[tokio::test]
    async fn a_credential_registers_once_and_the_last_passkey_stays() {
        let (_dir, p) = people();
        p.create_invite(invite("i1", "leonard", Role::Admin, "a")).await.unwrap();
        p.redeem_invite("i1", "h", key("k1", "leonard", "c1"), 1).await.unwrap().unwrap().unwrap();
        assert_eq!(p.add_passkey(key("k2", "leonard", "c1")).await.unwrap(), Err(PasskeyTaken));
        p.add_passkey(key("k2", "leonard", "c2")).await.unwrap().unwrap();
        assert_eq!(p.delete_passkey("someone", "k1").await.unwrap(), Removed::NotFound);
        assert_eq!(p.delete_passkey("leonard", "k1").await.unwrap(), Removed::Yes);
        assert_eq!(p.delete_passkey("leonard", "k2").await.unwrap(), Removed::Last);
        let (found, owner) = p.passkey_by_credential("c2").await.unwrap().unwrap();
        assert_eq!((found.id.as_str(), owner.id.as_str()), ("k2", "leonard"));
        assert!(p.rename_passkey("leonard", "k2", "iPhone").await.unwrap().is_some());
        assert_eq!(p.passkeys_of("leonard").await.unwrap()[0].name, "iPhone");
    }

    #[tokio::test]
    async fn a_new_link_replaces_the_pending_one_and_can_be_revoked() {
        let (_dir, p) = people();
        p.create_invite(invite("i1", "alex", Role::Member, "a")).await.unwrap();
        p.create_invite(invite("i2", "alex", Role::Member, "b")).await.unwrap();
        let pending = p.invites(10).await.unwrap();
        assert_eq!(pending.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["i2"]);
        assert!(p.delete_invite("i2").await.unwrap());
        assert!(!p.delete_invite("i2").await.unwrap());
        assert!(p.invites(10).await.unwrap().is_empty());
    }

    /// The file keeps the words it always had.
    #[test]
    fn roles_are_written_as_words() {
        assert_eq!(serde_json::to_value(Role::Admin).unwrap(), "admin");
        assert_eq!(serde_json::from_value::<Role>("member".into()).unwrap(), Role::Member);
        assert!(serde_json::from_value::<Role>("owner".into()).is_err());
    }

    #[test]
    fn names_and_ids() {
        assert_eq!(slug(" Léonard "), "leonard");
        assert_eq!(slug("Marie-Hélène D."), "marie-helene-d");
        assert_eq!(clean_name("  iPhone "), Some("iPhone"));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(MAX_NAME + 1)), None);
        assert_eq!(hash_secret("a").len(), 64);
        assert_ne!(random_secret(), random_secret());
    }
}
