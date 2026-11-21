//! The house's edge over the LAN (the backend's `edge.rs`): a frame that holds an edge identity
//! (`maison-backend edge client`) reaches the house straight at its edge listener, the target's
//! address (`https://<house>:3443`), rather than out to the public address and back in. TLS both
//! ways: it trusts nothing but the edge's CA, and presents its own certificate.
//!
//! The house's certificate names `maison_api::edge::SERVER_NAME`, never an address: requests go
//! to that name, and [`ToHouse`] resolves it to the target's host. Through
//! `ureq::unversioned::resolver`, whose API ureq may change in a minor release.

use std::fmt;
use std::path::Path;

use maison_api::edge::{CA_FILE, CLIENT_CERT_FILE, CLIENT_KEY_FILE, SERVER_NAME};
use ureq::http::Uri;
use ureq::tls::{Certificate, ClientCert, PrivateKey, RootCerts, TlsConfig};
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::NextTimeout;

/// The frame's edge identity.
#[derive(Clone)]
pub struct Edge {
    ca: Vec<Certificate<'static>>,
    /// Its chain, said apart for the comparison (the client certificate holds it with the key)
    chain: Vec<Certificate<'static>>,
    client: ClientCert,
}

impl fmt::Debug for Edge {
    // never the key
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Edge")
            .field("ca", &self.ca.len())
            .field("chain", &self.chain.len())
            .finish_non_exhaustive()
    }
}

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        let der = |certs: &[Certificate<'static>]| {
            certs.iter().map(|c| c.der().to_vec()).collect::<Vec<_>>()
        };
        der(&self.ca) == der(&other.ca) && der(&self.chain) == der(&other.chain)
    }
}

impl Edge {
    /// The identity in `dir`, its files named `<prefix><file>` (`edge_ca.pem`… in a systemd
    /// credentials directory, `ca.pem`… in a plain one); none when `dir` holds none of them,
    /// an error when it holds some but not all, or one that does not parse.
    pub fn load(dir: &Path, prefix: &str) -> Result<Option<Self>, String> {
        let files =
            [CA_FILE, CLIENT_CERT_FILE, CLIENT_KEY_FILE].map(|f| dir.join(format!("{prefix}{f}")));
        let read: Vec<Option<Vec<u8>>> = files
            .iter()
            .map(|p| match std::fs::read(p) {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("{}: {e}", p.display())),
            })
            .collect::<Result<_, _>>()?;
        let [Some(ca), Some(chain), Some(key)] =
            <[Option<Vec<u8>>; 3]>::try_from(read).map_err(|_| "three files")?
        else {
            return match files.iter().find(|p| p.exists()) {
                None => Ok(None),
                Some(one) => Err(format!(
                    "{}: the edge identity is incomplete ({CA_FILE}, {CLIENT_CERT_FILE}, {CLIENT_KEY_FILE})",
                    one.display()
                )),
            };
        };
        let certs = |pem: &[u8], file: &Path| -> Result<Vec<Certificate<'static>>, String> {
            let certs: Vec<_> = ureq::tls::parse_pem(pem)
                .filter_map(|item| match item {
                    Ok(ureq::tls::PemItem::Certificate(c)) => Some(Ok(c)),
                    Ok(_) => None,
                    Err(e) => Some(Err(format!("{}: {e}", file.display()))),
                })
                .collect::<Result<_, _>>()?;
            if certs.is_empty() {
                return Err(format!("{}: no certificate", file.display()));
            }
            Ok(certs)
        };
        let chain = certs(&chain, &files[1])?;
        let key = PrivateKey::from_pem(&key).map_err(|e| format!("{}: {e}", files[2].display()))?;
        Ok(Some(Self {
            ca: certs(&ca, &files[0])?,
            client: ClientCert::new_with_certs(&chain, key),
            chain,
        }))
    }

    /// The TLS the frame speaks to the edge: the edge's CA alone, its own certificate.
    pub fn tls(&self) -> TlsConfig {
        TlsConfig::builder()
            .root_certs(RootCerts::new_with_certs(&self.ca))
            .client_cert(Some(self.client.clone()))
            .build()
    }
}

/// `https://192.0.2.10:3443` → the origin the frame asks (`https://maison.home.arpa:3443`) and
/// the address it is sent to (`192.0.2.10:3443`); none for an origin with no port nor host.
pub fn through(origin: &str) -> Option<(String, String)> {
    let rest = origin.strip_prefix("https://")?;
    // `[v6]` or `[v6]:port`, else `host` or `host:port` (one colon at most)
    let (host, port) = match rest.strip_prefix('[') {
        Some(v6) => {
            let (addr, after) = v6.split_once(']')?;
            (
                format!("[{addr}]"),
                after.strip_prefix(':').unwrap_or("443"),
            )
        }
        None => match rest.split_once(':') {
            Some((h, p)) => (h.to_string(), p),
            None => (rest.to_string(), "443"),
        },
    };
    let port: u16 = port.parse().ok()?;
    (!host.is_empty() && host != "[]").then(|| {
        (
            format!("https://{SERVER_NAME}:{port}"),
            format!("{host}:{port}"),
        )
    })
}

/// Resolves the edge's name to the house's address (a name resolved as any, or an address);
/// every other name as usual.
pub struct ToHouse {
    house: Uri,
    usual: DefaultResolver,
}

impl ToHouse {
    /// `house`: `192.0.2.10:3443`, `[fd00::1]:3443`, `maison.lan:3443`.
    pub fn new(house: &str) -> Result<Self, String> {
        Ok(Self {
            house: format!("https://{house}/")
                .parse()
                .map_err(|e| format!("{house}: {e}"))?,
            usual: DefaultResolver::default(),
        })
    }
}

impl fmt::Debug for ToHouse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToHouse")
            .field("house", &self.house)
            .finish()
    }
}

impl Resolver for ToHouse {
    fn resolve(
        &self,
        uri: &Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        if uri.host() == Some(SERVER_NAME) {
            self.usual.resolve(&self.house, config, timeout)
        } else {
            self.usual.resolve(uri, config, timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_reached_by_the_edges_name() {
        assert_eq!(
            through("https://192.0.2.10:3443"),
            Some((
                "https://maison.home.arpa:3443".into(),
                "192.0.2.10:3443".into()
            ))
        );
        assert_eq!(
            through("https://[fd00::1]:3443"),
            Some((
                "https://maison.home.arpa:3443".into(),
                "[fd00::1]:3443".into()
            ))
        );
        assert_eq!(
            through("https://maison.lan"),
            Some((
                "https://maison.home.arpa:443".into(),
                "maison.lan:443".into()
            ))
        );
        assert_eq!(through("http://192.0.2.10:3443"), None, "TLS only");
    }

    #[test]
    fn the_edges_name_goes_to_the_house_and_nothing_else() {
        let to_house = ToHouse::new("127.0.0.7:3443").unwrap();
        let config = ureq::config::Config::default();
        let resolve = |uri: &str| {
            to_house
                .resolve(&uri.parse().unwrap(), &config, NextTimeout::default())
                .map(|addrs| addrs.iter().map(ToString::to_string).collect::<Vec<_>>())
        };
        assert_eq!(
            resolve(&format!("https://{SERVER_NAME}:3443/api/screens/me")).unwrap(),
            ["127.0.0.7:3443"]
        );
        assert_eq!(
            resolve("https://127.0.0.9:8443/").unwrap(),
            ["127.0.0.9:8443"]
        );
    }

    #[test]
    fn no_identity_is_none_and_half_of_one_an_error() {
        let dir = crate::testing::temp_dir("edge");
        assert_eq!(Edge::load(&dir, "edge_").unwrap(), None);
        std::fs::write(dir.join("edge_ca.pem"), "x").unwrap();
        assert!(Edge::load(&dir, "edge_").is_err());
    }
}
