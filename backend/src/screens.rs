//! Wall screens (docs/passkeys.md « Wall screens »): a browser paired once by an admin, that
//! then drives the devices as a service token does (`TokenKind::Screen`), its token in an
//! HttpOnly cookie no script can read.
//!
//! Pairing goes as a TV signs in to an account (RFC 8628's device grant, in a page): the screen
//! asks for a pairing and shows its short code; an admin, signed in with a passkey, gives that
//! code and a name; the screen, asking every few seconds with a secret only it holds, gets its
//! cookie. No token is ever typed, copied, or put in an address. Pending pairings live in
//! memory, a few minutes, like passkey ceremonies: a restart only means pairing again.
//!
//! The admin pairs from the screen's own network: the same public address (an IPv6 one counts
//! as its /64). A code sent from elsewhere (« type this code in Maison ») pairs nothing: a
//! browser across the internet never becomes one of the house's screens.

use std::{net::IpAddr, sync::Mutex, time::Duration};

use crate::{
    passkey::Limiter,
    util::{hash_secret, ip_group, lock, random_secret, Bounded, Caps},
};

/// How long a screen waits for an admin.
pub const PAIRING_TTL: Duration = Duration::from_secs(10 * 60);
/// How often the screen asks whether it is paired.
pub const POLL: Duration = Duration::from_secs(3);
/// Pairings waiting at once: in all, from one address (an IPv6 address counts as its /64), and
/// from one prefix (an IPv4 /24, an IPv6 /48: neighbours behind one operator's NAT share it).
/// Full, the most crowded prefix loses its oldest (`util::Bounded`): someone with many addresses
/// evicts their own pairings, not the one the house's screen waits with.
const CAPS: Caps = Caps {
    total: 100,
    per_address: PER_ADDRESS,
    per_prefix: 20,
};
const PER_ADDRESS: usize = 3;
/// Letters and digits nobody misreads (no 0/O, no 1/I/L): 8 of them, about 40 bits. Guessing
/// one gains nothing: only an admin can use a code, and it pairs the screen that showed it.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 8;
/// Pairings started, and claims asked, per address and minute: a screen polling every 3 s asks
/// 20 times a minute.
const STARTS_PER_MINUTE: u32 = 10;
const CLAIMS_PER_MINUTE: u32 = 40;

struct Pending {
    code: String,
    /// Once an admin paired it: the token and the screen's name, for its next claim.
    granted: Option<(String, String)>,
}

struct Waits {
    map: Bounded<Pending>,
    /// The names of screens granted then pushed out before their claim: their tokens to revoke.
    orphans: Vec<String>,
}

impl Waits {
    /// Pairings that went (stale or pushed out) while granted: their screens' names, kept.
    fn forget(&mut self, gone: Vec<Pending>) {
        self.orphans.extend(
            gone.into_iter()
                .filter_map(|p| p.granted.map(|(_, name)| name)),
        );
    }
}

/// Whether a screen waits with a code, for an admin asking from an address.
#[derive(Debug, PartialEq, Eq)]
pub enum Waiting {
    Here,
    /// It waits, but asked from another network than the admin's.
    Elsewhere,
    No,
}

/// What a screen's claim finds.
#[derive(Debug, PartialEq, Eq)]
pub enum Claim {
    /// No admin has given the code yet.
    Waiting,
    /// Paired: the token for its cookie (handed once), and the name it was given.
    Paired { token: String, name: String },
    /// Unknown, or too late: the screen starts again.
    Gone,
}

/// The pairings waiting for an admin, by `hash_secret` of the screen's secret.
pub struct Pairings {
    waits: Mutex<Waits>,
    starts: Limiter,
    claims: Limiter,
}

impl Default for Pairings {
    fn default() -> Self {
        Self::new(PAIRING_TTL)
    }
}

impl Pairings {
    pub fn new(ttl: Duration) -> Self {
        let minute = Duration::from_secs(60);
        Self {
            waits: Mutex::new(Waits {
                map: Bounded::new(ttl, CAPS),
                orphans: Vec::new(),
            }),
            starts: Limiter::new(STARTS_PER_MINUTE, minute),
            claims: Limiter::new(CLAIMS_PER_MINUTE, minute),
        }
    }

    /// A new pairing asked from `ip`: the screen's secret and the code it shows (`ABCD-EFGH`).
    /// `Err(seconds)` when the address asks too often.
    pub fn start(&self, ip: IpAddr) -> Result<(String, String), u64> {
        self.starts.hit(ip)?;
        let secret = random_secret();
        let mut waits = lock(&self.waits);
        let code = loop {
            let code = new_code();
            if waits.map.find(|h| h.value.code == code).is_none() {
                break code;
            }
        };
        let gone = waits.map.insert(
            hash_secret(&secret),
            ip,
            Pending {
                code: code.clone(),
                granted: None,
            },
        );
        waits.forget(gone);
        Ok((secret, shown(&code)))
    }

    /// Whether a screen waits with `code` (as typed: any case, dashes and spaces) for an admin
    /// asking from `admin`.
    pub fn waiting(&self, code: &str, admin: IpAddr) -> Waiting {
        let code = typed(code);
        let waits = lock(&self.waits);
        match waits
            .map
            .find(|h| h.value.code == code && h.value.granted.is_none())
        {
            Some(h) if h.from == ip_group(admin) => Waiting::Here,
            Some(_) => Waiting::Elsewhere,
            None => Waiting::No,
        }
    }

    /// The admin's answer, from `admin`: the screen waiting there with `code` is paired as
    /// `name` with `token`. False when none waits there (gone in between): the token is then
    /// the caller's to revoke.
    pub fn grant(&self, code: &str, admin: IpAddr, token: String, name: String) -> bool {
        let code = typed(code);
        let admin = ip_group(admin);
        let mut waits = lock(&self.waits);
        match waits
            .map
            .find_mut(|h| h.value.code == code && h.from == admin && h.value.granted.is_none())
        {
            Some(h) => {
                h.value.granted = Some((token, name));
                true
            }
            None => false,
        }
    }

    /// The screen asking, with its secret, from `ip`. `Err(seconds)` when it asks too often.
    pub fn claim(&self, ip: IpAddr, secret: &str) -> Result<Claim, u64> {
        self.claims.hit(ip)?;
        let key = hash_secret(secret);
        let mut waits = lock(&self.waits);
        match waits.map.get_mut(&key) {
            None => return Ok(Claim::Gone),
            Some(h) if h.value.granted.is_none() => return Ok(Claim::Waiting),
            Some(_) => {}
        }
        Ok(match waits.map.take(&key).and_then(|h| h.value.granted) {
            Some((token, name)) => Claim::Paired { token, name },
            None => Claim::Gone,
        })
    }

    /// Every token granted and never claimed, now forgotten: the caller revokes them. A paired
    /// screen that never came back for its cookie leaves no token behind.
    pub fn expired_grants(&self) -> Vec<String> {
        let mut waits = lock(&self.waits);
        let gone = waits.map.expired();
        waits.forget(gone);
        std::mem::take(&mut waits.orphans)
    }
}

fn new_code() -> String {
    (0..CODE_LEN)
        .map(|_| char::from(CODE_ALPHABET[rand::random_range(0..CODE_ALPHABET.len())]))
        .collect()
}

/// `ABCDEFGH` as the screen shows it: `ABCD-EFGH`.
fn shown(code: &str) -> String {
    format!("{}-{}", &code[..CODE_LEN / 2], &code[CODE_LEN / 2..])
}

/// A code as an admin types it: any case, with or without its dash, spaces around.
fn typed(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn a_screen_waits_until_an_admin_gives_its_code_then_gets_its_token_once() {
        let pairings = Pairings::default();
        let frame = ip("198.51.100.7");
        let (secret, code) = pairings.start(frame).unwrap();
        assert_eq!(code.len(), CODE_LEN + 1);
        assert_eq!(code.as_bytes()[CODE_LEN / 2], b'-');
        assert!(code
            .chars()
            .filter(|c| *c != '-')
            .all(|c| CODE_ALPHABET.contains(&(c as u8))));
        assert_eq!(pairings.claim(frame, &secret), Ok(Claim::Waiting));

        // typed by hand: lowercase, without the dash
        let typed_code = code.replace('-', " ").to_lowercase();
        let phone = ip("198.51.100.7");
        assert_eq!(pairings.waiting(&typed_code, phone), Waiting::Here);
        assert!(pairings.grant(&typed_code, phone, "maison_x".into(), "Salon".into()));
        assert_eq!(
            pairings.waiting(&code, phone),
            Waiting::No,
            "paired already"
        );
        assert!(
            !pairings.grant(&code, phone, "maison_y".into(), "Other".into()),
            "a code pairs once"
        );

        assert_eq!(
            pairings.claim(frame, &secret),
            Ok(Claim::Paired {
                token: "maison_x".into(),
                name: "Salon".into()
            })
        );
        assert_eq!(
            pairings.claim(frame, &secret),
            Ok(Claim::Gone),
            "handed once"
        );
    }

    #[test]
    fn only_the_secret_claims_and_a_late_one_is_gone() {
        let pairings = Pairings::new(Duration::ZERO);
        let (secret, code) = pairings.start(ip("198.51.100.7")).unwrap();
        assert_eq!(
            pairings.waiting(&code, ip("198.51.100.7")),
            Waiting::No,
            "too late"
        );
        assert!(!pairings.grant(&code, ip("198.51.100.7"), "maison_x".into(), "Salon".into()));
        assert_eq!(pairings.claim(ip("198.51.100.7"), &secret), Ok(Claim::Gone));

        let pairings = Pairings::default();
        pairings.start(ip("198.51.100.7")).unwrap();
        assert_eq!(
            pairings.claim(ip("198.51.100.7"), "not-the-secret"),
            Ok(Claim::Gone)
        );
    }

    #[test]
    fn an_address_keeps_a_few_pairings_and_is_limited() {
        let pairings = Pairings::default();
        let a = ip("2001:db8:1:2::1");
        let same_prefix = ip("2001:db8:1:2::2");
        let first = pairings.start(a).unwrap();
        for _ in 0..PER_ADDRESS {
            pairings.start(same_prefix).unwrap();
        }
        assert_eq!(
            pairings.claim(a, &first.0),
            Ok(Claim::Gone),
            "the oldest of its /64 went"
        );
        let mut refused = false;
        for _ in 0..STARTS_PER_MINUTE {
            refused |= pairings.start(a).is_err();
        }
        assert!(refused, "too many starts in a minute");
        assert!(
            pairings.start(ip("198.51.100.8")).is_ok(),
            "another address is not limited by it"
        );
    }

    #[test]
    fn only_an_admin_on_the_screens_network_pairs_it() {
        let pairings = Pairings::default();
        let (secret, code) = pairings.start(ip("2001:db8:1:2::10")).unwrap();
        let across_the_internet = ip("203.0.113.9");
        assert_eq!(
            pairings.waiting(&code, across_the_internet),
            Waiting::Elsewhere
        );
        assert!(!pairings.grant(
            &code,
            across_the_internet,
            "maison_x".into(),
            "Salon".into()
        ));
        assert_eq!(
            pairings.claim(ip("2001:db8:1:2::10"), &secret),
            Ok(Claim::Waiting),
            "still waiting"
        );
        let phone_on_the_same_lan = ip("2001:db8:1:2::abcd");
        assert_eq!(
            pairings.waiting(&code, phone_on_the_same_lan),
            Waiting::Here,
            "the same /64"
        );
        assert!(pairings.grant(
            &code,
            phone_on_the_same_lan,
            "maison_x".into(),
            "Salon".into()
        ));
    }

    #[test]
    fn a_flood_from_many_addresses_pushes_out_its_own_pairings_not_the_screens() {
        let pairings = Pairings::default();
        let (secret, _) = pairings.start(ip("198.51.100.7")).unwrap();
        // many addresses in one /24, each under its own start limit
        for n in 0..=255u8 {
            for _ in 0..2 {
                pairings.start(IpAddr::from([203, 0, 113, n])).unwrap();
            }
        }
        // and many /24s, one pairing each, past the cap: the crowded /24 pays. (More distinct
        // prefixes than the cap would evict the oldest of all: a residual, a pairing DoS only.)
        for n in 0..90u8 {
            pairings.start(IpAddr::from([192, 0, n, 1])).unwrap();
        }
        assert_eq!(
            pairings.claim(ip("198.51.100.7"), &secret),
            Ok(Claim::Waiting),
            "the house's screen still waits"
        );
    }

    #[test]
    fn a_granted_pairing_pushed_out_before_its_claim_has_its_token_revoked() {
        let pairings = Pairings::default();
        let home = ip("198.51.100.7");
        let (_, code) = pairings.start(home).unwrap();
        assert!(pairings.grant(&code, home, "maison_x".into(), "Salon".into()));
        // the same address starts three more: its oldest (the granted one) goes
        for _ in 0..PER_ADDRESS {
            pairings.start(home).unwrap();
        }
        assert_eq!(pairings.expired_grants(), vec!["Salon".to_string()]);
    }

    #[test]
    fn a_grant_never_claimed_is_handed_back_to_be_revoked() {
        let pairings = Pairings::new(Duration::from_millis(30));
        let (_, code) = pairings.start(ip("198.51.100.7")).unwrap();
        assert!(pairings.grant(&code, ip("198.51.100.7"), "maison_x".into(), "Salon".into()));
        assert!(pairings.expired_grants().is_empty(), "still fresh");
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(pairings.expired_grants(), vec!["Salon".to_string()]);
        assert!(pairings.expired_grants().is_empty(), "handed back once");
    }
}
