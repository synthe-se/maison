//! The edge: the public address's way in when no tunnel runs on this host. The public name
//! points at a reverse proxy elsewhere, which reaches this listener over a private network:
//! nothing is opened on the house's router, its address is never published.
//!
//! The listener is TLS both ways: it presents `cert.pem` (`key.pem`) and takes only a client
//! certificate one of `ca.pem`'s CAs signed. A request that came through it is the edge's
//! ([`Edge`]) and counts as the public address's, like one from cloudflared on loopback; the LAN
//! between the proxy's route and this host never sees a session in clear. Only the proxy (its
//! certificate signed by the house's own CA, `init`'s, the first of `ca.pem`) is believed for
//! the client's address (its `X-Forwarded-For`): any other client (`edge client`, a wall frame)
//! is its TCP address, whatever it says, for the counts, the pairing rule and the log alike.
//!
//! `maison-backend edge init <house dir> <proxy dir>` makes the certificates: a CA used once and
//! forgotten (its key is never written), this host's server certificate and the proxy's client
//! certificate. Run it again to replace both sides. `edge client <house dir> <client dir> <name>`
//! adds a client (a wall frame on the LAN, which reaches the house through this listener rather
//! than out and back in): a CA of its own, used once too, added to the house's `ca.pem`. The
//! listener trusts every CA there: removing a client's refuses that client alone.

use std::{
    io,
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::Arc,
    time::Duration,
};

use axum::{extract::ConnectInfo, middleware::Next, response::Response};
use rustls::{
    RootCertStore, ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    server::WebPkiClientVerifier,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use crate::store::{self, Access};

/// The name this host's certificate carries, the one its clients ask for; a client's files
/// (`ca.pem`, `client.pem`, `client-key.pem`). Said once in maison-api, for the frame's app too.
pub use maison_api::edge::{CA_FILE, CLIENT_CERT_FILE, CLIENT_KEY_FILE, SERVER_NAME};

/// The house's files in the edge's directory (`EDGE_DIR`, default `edge/`, 0700), next to its
/// `ca.pem`: every client CA it trusts, the house's own (`init`'s) first.
pub const CERT_FILE: &str = "cert.pem";
pub const KEY_FILE: &str = "key.pem";

/// A handshake that does not finish by then is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Handshaken connections waiting for the server to take them.
const BACKLOG: usize = 64;

/// A request that came in through the edge's listener.
#[derive(Debug, Clone, Copy)]
pub struct Edge;

/// A request through the edge from a client of the house's own (`edge client`, never the
/// proxy): reached from the LAN, with a certificate the house gave it.
#[derive(Debug, Clone, Copy)]
pub struct HouseClient;

/// The listener's TLS, and the proxy's own check: the certificates the house's CA signed.
#[derive(Clone)]
pub struct EdgeTls {
    config: Arc<ServerConfig>,
    proxy: Arc<dyn rustls::server::danger::ClientCertVerifier>,
}

/// Who came through the edge: the TCP peer, and whether it is the proxy (its certificate the
/// house's CA's), the one client believed for the address it forwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgePeer {
    pub addr: SocketAddr,
    pub proxy: bool,
}

/// The listener's TLS: this host's certificate, every client CA of `ca.pem`; and the proxy's
/// check, against the first of them alone (the house's own, `init`'s).
pub fn server_config(dir: &Path) -> Result<EdgeTls, String> {
    let read = |name: &str| dir.join(name);
    let certs = CertificateDer::pem_file_iter(read(CERT_FILE))
        .and_then(|certs| certs.collect::<Result<Vec<_>, _>>())
        .map_err(|e| format!("{}: {e}", read(CERT_FILE).display()))?;
    let key = PrivateKeyDer::from_pem_file(read(KEY_FILE))
        .map_err(|e| format!("{}: {e}", read(KEY_FILE).display()))?;
    let (mut roots, mut house) = (RootCertStore::empty(), RootCertStore::empty());
    for (n, ca) in CertificateDer::pem_file_iter(read(CA_FILE))
        .map_err(|e| format!("{}: {e}", read(CA_FILE).display()))?
        .enumerate()
    {
        let ca = ca.map_err(|e| format!("{}: {e}", read(CA_FILE).display()))?;
        if n == 0 {
            house
                .add(ca.clone())
                .map_err(|e| format!("{}: {e}", read(CA_FILE).display()))?;
        }
        roots
            .add(ca)
            .map_err(|e| format!("{}: {e}", read(CA_FILE).display()))?;
    }
    let verifier = |roots: RootCertStore| {
        WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|e| format!("edge client verifier: {e}"))
    };
    let proxy = verifier(house)?;
    let verifier = verifier(roots)?;
    let mut config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .map_err(|e| format!("edge certificate: {e}"))?;
    // both, as axum serves both (its `http2` feature): h2 to a client that offers it (Caddy's
    // transport, curl), HTTP/1.1 to one that does not (the frame's app)
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(EdgeTls {
        config: Arc::new(config),
        proxy,
    })
}

impl EdgeTls {
    /// Whether the client of `tls` is the proxy: its chain checks against the house's CA alone.
    fn is_proxy(&self, tls: &TlsStream<TcpStream>) -> bool {
        tls.get_ref().1.peer_certificates().is_some_and(|chain| {
            chain.split_first().is_some_and(|(leaf, intermediates)| {
                self.proxy
                    .verify_client_cert(leaf, intermediates, rustls::pki_types::UnixTime::now())
                    .is_ok()
            })
        })
    }
}

/// The edge's listener: TCP accepted on its own task, each handshake on its own (a slow or
/// silent client holds nobody else up), the finished ones handed to the server.
pub struct EdgeListener {
    ready: mpsc::Receiver<(TlsStream<TcpStream>, EdgePeer)>,
    local: SocketAddr,
    accepting: tokio::task::JoinHandle<()>,
}

/// What `axum::serve` takes with `ConnectInfo<EdgePeer>`: the listener behind axum's `TapIo`,
/// whose `Connected` is generic (an impl of ours would break the orphan rules). [`from_edge`]
/// turns it into the `ConnectInfo<SocketAddr>` every route reads.
pub type Edged = axum::serve::TapIo<EdgeListener, fn(&mut TlsStream<TcpStream>)>;

impl EdgeListener {
    /// The edge's listener, bound, ready for `axum::serve`.
    pub async fn bind(address: SocketAddr, config: EdgeTls) -> io::Result<Edged> {
        use axum::serve::ListenerExt;
        let untouched: fn(&mut TlsStream<TcpStream>) = |_| {};
        Ok(Self::accepting(address, config).await?.tap_io(untouched))
    }

    async fn accepting(address: SocketAddr, tls: EdgeTls) -> io::Result<Self> {
        let listener = TcpListener::bind(address).await?;
        let local = listener.local_addr()?;
        let acceptor = TlsAcceptor::from(tls.config.clone());
        let (sender, ready) = mpsc::channel(BACKLOG);
        let accepting = tokio::spawn(async move {
            loop {
                let (stream, peer) = match listener.accept().await {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        tracing::warn!(%error, "edge: accept failed");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };
                let (acceptor, sender, tls) = (acceptor.clone(), sender.clone(), tls.clone());
                tokio::spawn(async move {
                    match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
                        Ok(Ok(stream)) => {
                            let proxy = tls.is_proxy(&stream);
                            let _ = sender.send((stream, EdgePeer { addr: peer, proxy })).await;
                        }
                        // a scanner, a client without the proxy's certificate
                        Ok(Err(error)) => {
                            tracing::warn!(%peer, %error, "edge: handshake refused")
                        }
                        Err(_) => tracing::warn!(%peer, "edge: handshake timed out"),
                    }
                });
            }
        });
        Ok(Self {
            ready,
            local,
            accepting,
        })
    }
}

impl Drop for EdgeListener {
    fn drop(&mut self) {
        self.accepting.abort();
    }
}

impl axum::serve::Listener for EdgeListener {
    type Io = TlsStream<TcpStream>;
    type Addr = EdgePeer;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(ready) => ready,
            // the accepting task only ends when aborted, with the listener
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(EdgePeer {
            addr: self.local,
            proxy: false,
        })
    }
}

/// The edge's layer, outermost on its router: marks the request [`Edge`] and makes the
/// client's address the request's peer (`ConnectInfo<SocketAddr>`), for the rules, the counts
/// and the log that follow: the proxy's `X-Forwarded-For` (its last entry, the one the proxy
/// saw), any other client's own TCP address, whatever it forwards.
pub async fn from_edge(mut request: axum::extract::Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<EdgePeer>>()
        .map(|c| c.0);
    // read before the request moves on (a borrow of it held across the await is not Send)
    let forwarded = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|v| v.trim().parse::<IpAddr>().ok())
        .map(|ip| SocketAddr::new(ip.to_canonical(), 0));
    let client = match peer {
        Some(p) if p.proxy => forwarded.or(Some(p.addr)),
        Some(p) => Some(p.addr),
        None => None,
    };
    if let Some(client) = client {
        request.extensions_mut().insert(ConnectInfo(client));
    }
    if peer.is_some_and(|p| !p.proxy) {
        request.extensions_mut().insert(HouseClient);
    }
    request.extensions_mut().insert(Edge);
    next.run(request).await
}

/// Whether the request came through the edge.
pub fn is_edge(extensions: &axum::http::Extensions) -> bool {
    extensions.get::<Edge>().is_some()
}

/// A CA used once: what it signs, then forgotten (its key never written).
struct OneUseCa {
    pem: String,
    issuer: rcgen::Issuer<'static, rcgen::KeyPair>,
}

fn rcgen_error(e: rcgen::Error) -> String {
    e.to_string()
}

/// A certificate's parameters: `name` its common name, valid for 20 years from yesterday.
fn params(name: &str, sans: Vec<String>) -> Result<rcgen::CertificateParams, String> {
    let now = time::OffsetDateTime::now_utc();
    let mut params = rcgen::CertificateParams::new(sans).map_err(rcgen_error)?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, name);
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(20 * 365);
    Ok(params)
}

impl OneUseCa {
    fn new(name: &str) -> Result<Self, String> {
        use rcgen::{BasicConstraints, IsCa, KeyPair, KeyUsagePurpose};
        let key = KeyPair::generate().map_err(rcgen_error)?;
        let mut params = params(name, vec![])?;
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let pem = params.self_signed(&key).map_err(rcgen_error)?.pem();
        Ok(Self {
            pem,
            issuer: rcgen::Issuer::new(params, key),
        })
    }

    /// A certificate it signs, for `purpose`: its PEM and its key's.
    fn sign(
        &self,
        name: &str,
        sans: Vec<String>,
        purpose: rcgen::ExtendedKeyUsagePurpose,
    ) -> Result<(String, String), String> {
        let key = rcgen::KeyPair::generate().map_err(rcgen_error)?;
        let mut params = params(name, sans)?;
        params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![purpose];
        let cert = params.signed_by(&key, &self.issuer).map_err(rcgen_error)?;
        Ok((cert.pem(), key.serialize_pem()))
    }
}

/// `text` as `dir`'s file `name`, atomically (store.rs), `dir` made private.
fn write(dir: &Path, name: &str, text: &str, access: Access) -> Result<(), String> {
    store::private_dir(dir).map_err(|error| format!("{}: {error:?}", dir.display()))?;
    store::write_bytes(&dir.join(name), text.as_bytes(), access)
        .map_err(|error| format!("{}: {error:?}", dir.join(name).display()))
}

/// A client's files in `dir`: its certificate and key (signed for client authentication by
/// `ca`), and `house_ca`, the CA to check the house by.
fn write_client(dir: &Path, ca: &OneUseCa, name: &str, house_ca: &str) -> Result<(), String> {
    let (cert, key) = ca.sign(name, vec![], rcgen::ExtendedKeyUsagePurpose::ClientAuth)?;
    write(dir, CA_FILE, house_ca, Access::Shared)?;
    write(dir, CLIENT_CERT_FILE, &cert, Access::Shared)?;
    write(dir, CLIENT_KEY_FILE, &key, Access::Private)
}

/// The certificates (see the module notes): this host's in `house` (0700: `ca.pem` the
/// proxy's CA, `cert.pem`, `key.pem`), the proxy's in `proxy` (0700: `client.pem`,
/// `client-key.pem`, and `ca.pem`, the CA to trust this host by).
pub fn init(house: &Path, proxy: &Path) -> Result<(), String> {
    let ca = OneUseCa::new("maison edge CA")?;
    let (server, server_key) = ca.sign(
        SERVER_NAME,
        vec![SERVER_NAME.to_string()],
        rcgen::ExtendedKeyUsagePurpose::ServerAuth,
    )?;
    write(house, CA_FILE, &ca.pem, Access::Shared)?;
    write(house, CERT_FILE, &server, Access::Shared)?;
    write(house, KEY_FILE, &server_key, Access::Private)?;
    write_client(proxy, &ca, "maison edge proxy", &ca.pem)
}

/// Another client, `name` (see the module notes): its files in `client` (0700: `client.pem`,
/// `client-key.pem`, and `ca.pem`, the CA to trust this host by: the first of the house's), its
/// own one-use CA added to `house`'s `ca.pem`. The service reads `ca.pem` at its start.
pub fn client(house: &Path, client: &Path, name: &str) -> Result<(), String> {
    let cas = std::fs::read_to_string(house.join(CA_FILE))
        .map_err(|e| format!("{}: {e} (edge init first)", house.join(CA_FILE).display()))?;
    const END: &str = "-----END CERTIFICATE-----";
    let house_ca = cas
        .find(END)
        .map(|end| format!("{}\n", &cas[..end + END.len()]))
        .ok_or(format!("{}: no certificate", house.join(CA_FILE).display()))?;
    let ca = OneUseCa::new(&format!("maison edge CA for {name}"))?;
    write_client(client, &ca, name, &house_ca)?;
    let joined = format!("{}\n{}", cas.trim_end(), ca.pem);
    write(house, CA_FILE, &joined, Access::Shared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use rustls::{ClientConfig, pki_types::ServerName};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_rustls::TlsConnector;

    /// A proxy's TLS: the CA it trusts the house by (from `trust`'s files), and the client
    /// certificate it presents (`cert`'s files), or none.
    fn client_config(trust: &Path, cert: Option<&Path>) -> Arc<ClientConfig> {
        let mut roots = RootCertStore::empty();
        for ca in CertificateDer::pem_file_iter(trust.join(CA_FILE)).unwrap() {
            roots.add(ca.unwrap()).unwrap();
        }
        let builder = ClientConfig::builder().with_root_certificates(roots);
        let mut config = if let Some(proxy) = cert {
            let certs = CertificateDer::pem_file_iter(proxy.join(CLIENT_CERT_FILE))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let key = PrivateKeyDer::from_pem_file(proxy.join(CLIENT_KEY_FILE)).unwrap();
            builder.with_client_auth_cert(certs, key).unwrap()
        } else {
            builder.with_no_client_auth()
        };
        // HTTP/1.1, what get_through writes; h2_through offers h2
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Arc::new(config)
    }

    /// An HTTP/1.1 GET through the edge, the answer's text (empty when the edge refused).
    async fn get_through(port: u16, config: Arc<ClientConfig>, path: &str, xff: &str) -> String {
        let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let name = ServerName::try_from(SERVER_NAME).unwrap();
        let Ok(mut tls) = TlsConnector::from(config).connect(name, tcp).await else {
            return String::new();
        };
        let request = format!(
            "GET {path} HTTP/1.1\r\nhost: home.example\r\nx-forwarded-for: {xff}\r\nconnection: close\r\n\r\n"
        );
        if tls.write_all(request.as_bytes()).await.is_err() {
            return String::new();
        }
        let mut answer = String::new();
        let _ = tls.read_to_string(&mut answer).await;
        answer
    }

    /// A GET over HTTP/2, the client offering h2 as Caddy and curl do: the answer's body.
    async fn h2_through(port: u16, config: Arc<ClientConfig>, path: &str) -> String {
        h2_with(port, config, path, &[]).await
    }

    /// `h2_through`, with these headers too (each its own field, as HTTP/2 sends cookies).
    async fn h2_with(
        port: u16,
        config: Arc<ClientConfig>,
        path: &str,
        headers: &[(&str, &str)],
    ) -> String {
        let mut config = (*config).clone();
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let name = ServerName::try_from(SERVER_NAME).unwrap();
        let tls = TlsConnector::from(Arc::new(config))
            .connect(name, tcp)
            .await
            .unwrap();
        assert_eq!(
            tls.get_ref().1.alpn_protocol(),
            Some(&b"h2"[..]),
            "h2 negotiated"
        );
        let (mut client, connection) = h2::client::handshake(tls).await.unwrap();
        tokio::spawn(connection);
        let mut request = axum::http::Request::get(format!("https://{SERVER_NAME}{path}"))
            .header("x-forwarded-for", "203.0.113.20");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let request = request.body(()).unwrap();
        let (answer, _) = client.send_request(request, true).unwrap();
        let mut body = answer.await.unwrap().into_body();
        let mut text = Vec::new();
        while let Some(chunk) = body.data().await {
            text.extend_from_slice(&chunk.unwrap());
        }
        String::from_utf8(text).unwrap()
    }

    /// Only the proxy's certificate opens the edge, and its requests are the edge's, with the
    /// client's address from the proxy.
    #[tokio::test]
    async fn the_edge_takes_the_proxys_certificate_only() {
        let root = crate::util::test_dir();
        let (house, proxy) = (root.path().join("edge"), root.path().join("proxy"));
        init(&house, &proxy).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&house), 0o700);
            assert_eq!(mode(&house.join(KEY_FILE)), 0o600);
            assert_eq!(mode(&proxy.join(CLIENT_KEY_FILE)), 0o600);
        }

        let app = Router::new()
            .route(
                "/who",
                get(
                    |ConnectInfo(peer): ConnectInfo<SocketAddr>,
                     request: axum::extract::Request| async move {
                        format!("edge={} peer={}", is_edge(request.extensions()), peer.ip())
                    },
                ),
            )
            .layer(axum::middleware::from_fn(from_edge));
        let listener = EdgeListener::bind(
            "127.0.0.1:0".parse().unwrap(),
            server_config(&house).unwrap(),
        )
        .await
        .unwrap();
        let port = axum::serve::Listener::local_addr(&listener)
            .unwrap()
            .addr
            .port();
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<EdgePeer>(),
            )
            .await
            .unwrap();
        });

        let proxy_tls = client_config(&proxy, Some(&proxy));
        let answer =
            get_through(port, proxy_tls.clone(), "/who", "198.51.100.1, 203.0.113.9").await;
        assert!(answer.contains("edge=true peer=203.0.113.9"), "{answer}");

        // without the proxy's certificate: no handshake, no request
        let answer = get_through(port, client_config(&proxy, None), "/who", "203.0.113.9").await;
        assert!(!answer.contains("edge="), "{answer}");

        // a client certificate another CA signed: refused too
        let other = root.path().join("other-proxy");
        init(&root.path().join("other-edge"), &other).unwrap();
        let answer = get_through(
            port,
            client_config(&proxy, Some(&other)),
            "/who",
            "203.0.113.9",
        )
        .await;
        assert!(!answer.contains("edge="), "{answer}");

        // the edge still serves after those
        let answer = get_through(port, proxy_tls.clone(), "/who", "203.0.113.10").await;
        assert!(answer.contains("edge=true peer=203.0.113.10"), "{answer}");

        // and over HTTP/2, when the client offers it
        let answer = h2_through(port, proxy_tls, "/who").await;
        assert_eq!(answer, "edge=true peer=203.0.113.20");
    }

    /// An edge listener serving `/who` (whether the request is the edge's, and its peer's
    /// address as the routes see it), on its own port.
    async fn who(house: std::path::PathBuf) -> u16 {
        let app = Router::new()
            .route(
                "/who",
                get(
                    |ConnectInfo(peer): ConnectInfo<SocketAddr>,
                     request: axum::extract::Request| async move {
                        let house = request.extensions().get::<HouseClient>().is_some();
                        format!(
                            "edge={} peer={} house={house}",
                            is_edge(request.extensions()),
                            peer.ip()
                        )
                    },
                ),
            )
            .layer(axum::middleware::from_fn(from_edge));
        let listener = EdgeListener::bind(
            "127.0.0.1:0".parse().unwrap(),
            server_config(&house).unwrap(),
        )
        .await
        .unwrap();
        let port = axum::serve::Listener::local_addr(&listener)
            .unwrap()
            .addr
            .port();
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<EdgePeer>(),
            )
            .await
            .unwrap();
        });
        port
    }

    /// A browser's cookies over HTTP/2, one per header field: the session cookie is read
    /// whatever field it came in (the backend once read the first alone: signed out).
    #[tokio::test]
    async fn cookies_split_over_http2_are_all_read() {
        let root = crate::util::test_dir();
        let (house, proxy) = (root.path().join("edge"), root.path().join("proxy"));
        init(&house, &proxy).unwrap();
        let app = Router::new()
            .route(
                "/session",
                get(|request: axum::extract::Request| async move {
                    crate::auth::cookie_value(request.headers(), "__Host-session")
                        .unwrap_or("none")
                        .to_string()
                }),
            )
            .layer(axum::middleware::from_fn(from_edge));
        let listener = EdgeListener::bind(
            "127.0.0.1:0".parse().unwrap(),
            server_config(&house).unwrap(),
        )
        .await
        .unwrap();
        let port = axum::serve::Listener::local_addr(&listener)
            .unwrap()
            .addr
            .port();
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<EdgePeer>(),
            )
            .await
            .unwrap();
        });
        let cookies = [("cookie", "theme=dark"), ("cookie", "__Host-session=abc")];
        let answer = h2_with(
            port,
            client_config(&proxy, Some(&proxy)),
            "/session",
            &cookies,
        )
        .await;
        assert_eq!(answer, "abc");
    }

    /// `edge client`: the new client checks the house by the `ca.pem` it was given and is let
    /// in, the proxy still is; a client whose CA is taken out of the house's `ca.pem` is not.
    #[tokio::test]
    async fn a_client_is_added_and_taken_out_alone() {
        let root = crate::util::test_dir();
        let (house, proxy, frame) = (
            root.path().join("edge"),
            root.path().join("proxy"),
            root.path().join("frame"),
        );
        init(&house, &proxy).unwrap();
        let alone = std::fs::read_to_string(house.join(CA_FILE)).unwrap();
        client(&house, &frame, "frame").unwrap();
        assert_eq!(
            std::fs::read_to_string(frame.join(CA_FILE)).unwrap(),
            alone,
            "the client checks the house by the house's own CA"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&frame), 0o700);
            assert_eq!(mode(&frame.join(CLIENT_KEY_FILE)), 0o600);
        }

        let serve = |house: std::path::PathBuf| async move {
            let app = Router::new()
                .route("/who", get(|| async { "edge=true" }))
                .layer(axum::middleware::from_fn(from_edge));
            let listener = EdgeListener::bind(
                "127.0.0.1:0".parse().unwrap(),
                server_config(&house).unwrap(),
            )
            .await
            .unwrap();
            let port = axum::serve::Listener::local_addr(&listener)
                .unwrap()
                .addr
                .port();
            tokio::spawn(async move {
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<EdgePeer>(),
                )
                .await
                .unwrap();
            });
            port
        };
        let port = serve(house.clone()).await;
        let frame_tls = client_config(&frame, Some(&frame));
        assert!(
            get_through(port, frame_tls.clone(), "/who", "")
                .await
                .contains("edge=true")
        );

        // a frame is no proxy: what it forwards is not believed, its TCP address is its own
        let port_who = who(house.clone()).await;
        let answer = get_through(port_who, frame_tls.clone(), "/who", "203.0.113.66").await;
        assert!(
            answer.contains("edge=true peer=127.0.0.1 house=true"),
            "{answer}"
        );
        let proxy_tls = client_config(&proxy, Some(&proxy));
        let answer = get_through(port_who, proxy_tls, "/who", "203.0.113.66").await;
        assert!(
            answer.contains("edge=true peer=203.0.113.66 house=false"),
            "the proxy is: {answer}"
        );
        assert!(
            get_through(port, client_config(&proxy, Some(&proxy)), "/who", "")
                .await
                .contains("edge=true")
        );

        // the frame's CA taken out of the house's ca.pem (the service restarted): refused
        std::fs::write(house.join(CA_FILE), &alone).unwrap();
        let port = serve(house.clone()).await;
        assert!(
            !get_through(port, frame_tls, "/who", "")
                .await
                .contains("edge=true")
        );
        assert!(
            get_through(port, client_config(&proxy, Some(&proxy)), "/who", "")
                .await
                .contains("edge=true")
        );
    }
}
