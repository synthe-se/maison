//! Android TV Remote v2 — the protocol Google's own remote app speaks.
//!
//! Worth the trouble purely for latency. Sending a key over ADB costs ~150 ms,
//! and measurement pins the blame precisely: `adb shell true` alone is 65 ms,
//! so the remaining ~85 ms is `input` booting a JVM on every single press.
//! This protocol has no such cost — the key goes as a protobuf message over an
//! already-open TLS session straight to the system's remote service.
//!
//! It carries more than keys: `RemoteAppLinkLaunchRequest` launches apps —
//! that is what the Netflix and Prime buttons on a physical remote do — and
//! power and volume are first-class too. ADB stays for what genuinely has no
//! equivalent here: CEC (hence the TV's input), detailed box state, and
//! sideloading an APK.
//!
//! Two ports, both TLS with a *client* certificate we generate and keep:
//!
//!   * **6467, pairing.** Runs once. The TV shows a six hex-digit code, and
//!     the client proves it saw it by hashing both public keys together with
//!     the code's last two bytes. The first byte of that hash must equal the
//!     code's first byte — which is how the client detects a typo before
//!     bothering the TV with it.
//!   * **6466, session.** The TV drives: it sends `remote_configure`, then
//!     `remote_set_active`, then pings forever. Each wants an answer, and a
//!     missed ping drops the connection. Keys are injected into that stream.
//!
//! Messages are length-delimited protobuf: a varint byte count, then the
//! payload. Both schemas are small and stable, so they are encoded by hand
//! rather than dragging in `prost` and a `protoc` build dependency — the same
//! call made for the ADB client next door.
//!
//! The TV's certificate is self-signed, so nothing vouches for it: trust on first use.
//! Pairing (itself proof against a man in the middle: its secret hashes both keys) forgets
//! any earlier one; the first session after it pins the certificate the TV presents
//! (SHA-256, kept with our identity), and every session after refuses another, its
//! handshake signatures checked against the pinned key (ASVS 12.3.2).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, oneshot, watch},
    time::timeout,
};
use tokio_rustls::{
    client::TlsStream,
    rustls::{
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        crypto::WebPkiSupportedAlgorithms,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
        ClientConfig, DigitallySignedStruct, SignatureScheme,
    },
    TlsConnector,
};

use crate::{
    error::{AppError, Refusal},
    net::unreachable,
    store::{self, Access},
    util::refused,
};

/// How errors name the box to the client (the detail is logged).
const TV: &str = "Android TV";

/// Our own failure (an identity that cannot be made or read): the detail in the log.
fn internal(what: &str, error: impl std::fmt::Display) -> AppError {
    tracing::error!(%error, "{what}");
    Refusal::Internal.into()
}

/// A certificate's SHA-256: what is pinned.
pub type Fingerprint = [u8; 32];

fn fingerprint(certificate: &CertificateDer<'_>) -> Fingerprint {
    Sha256::digest(certificate.as_ref()).into()
}

pub const PAIRING_PORT: u16 = 6467;
pub const REMOTE_PORT: u16 = 6466;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// The pairing code has to be read off the screen and typed, so this step is
/// paced by a human, not by the network.
const PAIRING_TIMEOUT: Duration = Duration::from_secs(120);
/// The TV pings a session every few seconds: this long without a word means it is gone
/// (asleep, unplugged) even though no TCP error says so.
const SESSION_SILENCE: Duration = Duration::from_secs(20);
/// Negotiation is short; anything longer means the TV is not playing.
const NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(15);
/// Largest message taken from the TV. Its real ones are a few hundred bytes; a length
/// read from a desynchronised or hostile stream must not size an allocation.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// PING | KEY | POWER | VOLUME | APP_LINK. IME and voice are deliberately left
/// out: IME makes some devices show "use the keyboard on your phone", and
/// neither adds anything to a remote. APP_LINK is what allows launching apps.
const ACTIVE_FEATURES: i64 = 1 | 2 | 32 | 64 | 512;

// ---------------------------------------------------------------- protobuf --

/// Minimal protobuf writer: varints, length-delimited fields and nesting are
/// all these two schemas use.
#[derive(Default)]
struct ProtoBuf {
    bytes: Vec<u8>,
}

impl ProtoBuf {
    fn varint(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(byte);
                return;
            }
            self.bytes.push(byte | 0x80);
        }
    }

    fn tag(&mut self, field: u32, wire_type: u32) {
        self.varint(u64::from(field) << 3 | u64::from(wire_type));
    }

    fn int(&mut self, field: u32, value: i64) {
        self.tag(field, 0);
        self.varint(value as u64);
    }

    fn bytes_field(&mut self, field: u32, value: &[u8]) {
        self.tag(field, 2);
        self.varint(value.len() as u64);
        self.bytes.extend_from_slice(value);
    }

    fn string(&mut self, field: u32, value: &str) {
        self.bytes_field(field, value.as_bytes());
    }

    fn message(&mut self, field: u32, build: impl FnOnce(&mut ProtoBuf)) {
        let mut nested = ProtoBuf::default();
        build(&mut nested);
        self.bytes_field(field, &nested.bytes);
    }

    /// Frames the message the way both services expect: length, then payload.
    fn frame(self) -> Vec<u8> {
        let mut out = ProtoBuf::default();
        out.varint(self.bytes.len() as u64);
        out.bytes.extend_from_slice(&self.bytes);
        out.bytes
    }
}

/// Reads just enough of a message to know which field it carries and to pull
/// out the scalars we answer with. Anything unrecognised is skipped, which is
/// what keeps this robust against the parts of the schema we ignore.
struct ProtoReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ProtoReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn varint(&mut self) -> Option<u64> {
        let mut decoder = Varint::default();
        loop {
            let byte = *self.bytes.get(self.pos)?;
            self.pos += 1;
            if let Some(value) = decoder.push(byte).ok()? {
                return Some(value);
            }
        }
    }

    /// Yields `(field number, payload)` for each field, where a payload is
    /// either the varint value or the delimited bytes.
    fn next_field(&mut self) -> Option<(u32, Field<'a>)> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let key = self.varint()?;
        let field = u32::try_from(key >> 3).ok()?;
        match key & 0x7 {
            0 => Some((field, Field::Varint(self.varint()?))),
            2 => {
                let len = usize::try_from(self.varint()?).ok()?;
                let end = self.pos.checked_add(len)?;
                let slice = self.bytes.get(self.pos..end)?;
                self.pos = end;
                Some((field, Field::Bytes(slice)))
            }
            // 64-bit and 32-bit: skipped, neither schema uses them here.
            1 => {
                self.pos += 8;
                Some((field, Field::Skipped))
            }
            5 => {
                self.pos += 4;
                Some((field, Field::Skipped))
            }
            _ => None,
        }
    }
}

/// A varint read a byte at a time, the one decoder for both the messages and their
/// length prefix. More than 64 bits is malformed, not wrapped.
#[derive(Default)]
struct Varint {
    value: u64,
    shift: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct VarintOverflow;

impl Varint {
    /// The value once its last byte is in, `None` while more are to come.
    fn push(&mut self, byte: u8) -> Result<Option<u64>, VarintOverflow> {
        let bits = u64::from(byte & 0x7f);
        // the tenth byte holds the 64th bit and nothing more
        if self.shift > 63 || (self.shift == 63 && bits > 1) {
            return Err(VarintOverflow);
        }
        self.value |= bits << self.shift;
        if byte & 0x80 == 0 {
            return Ok(Some(self.value));
        }
        self.shift += 7;
        Ok(None)
    }
}

enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Skipped,
}

// -------------------------------------------------------------- identity ---

/// The client certificate identifying this host to the TV.
///
/// It must be RSA: pairing proves possession by hashing the modulus and
/// exponent of *both* certificates, so an ECDSA key has nothing to contribute.
/// rcgen cannot generate RSA with the ring backend, so the key comes from the
/// `rsa` crate — the same one the ADB client already uses — and rcgen only
/// wraps it into a self-signed X.509.
#[derive(Clone)]
pub struct Identity {
    pub certificate: CertificateDer<'static>,
    /// PKCS#8.
    key_der: Vec<u8>,
    /// Kept for the pairing hash, which needs the raw public numbers.
    modulus: Vec<u8>,
    exponent: Vec<u8>,
    /// The TV's certificate since the first session after pairing: another is refused.
    pub pinned: Option<Fingerprint>,
    /// Where it is kept (the pin is written there too).
    path: PathBuf,
}

impl Identity {
    /// Loads the stored identity, minting one on first use. Generation is
    /// slow on an ARMv6 Pi, so callers should keep this off the async runtime.
    /// A torn file is replaced (pairing again is the price), never a dead end.
    pub fn load_or_create(path: &Path) -> Result<Self, AppError> {
        store::load_or_create_secret(
            path,
            |stored| Self::from_stored(stored, path).ok(),
            || {
                let stored = Self::mint()?;
                Ok((Self::from_stored(&stored, path)?, stored))
            },
        )
    }

    fn key(&self) -> PrivateKeyDer<'static> {
        PrivatePkcs8KeyDer::from(self.key_der.clone()).into()
    }

    /// Pins the TV's certificate (`None`: forgets it, as pairing again does), kept with
    /// the identity.
    pub async fn pin(&mut self, pinned: Option<Fingerprint>) -> Result<(), AppError> {
        let stored = stored_form(&self.key_der, self.certificate.as_ref(), pinned)?;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || store::write_bytes(&path, &stored, Access::Private))
            .await??;
        self.pinned = pinned;
        Ok(())
    }

    /// Stored form is the PKCS#8 key followed by the certificate, then the pinned TV
    /// certificate's fingerprint when there is one, each length-prefixed — enough structure
    /// to reload, without pulling in PEM.
    fn mint() -> Result<Vec<u8>, AppError> {
        use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_RSA_SHA256};

        let rsa_key = crate::adb::generate_key()?;
        let key_der = crate::adb::encode_key(&rsa_key)?;

        let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(
            &PrivatePkcs8KeyDer::from(key_der.clone()),
            &PKCS_RSA_SHA256,
        )
        .map_err(|error| internal("could not wrap the ADB key for TLS", error))?;

        let mut params = CertificateParams::new(vec!["maison".to_string()])
            .map_err(|error| internal("bad certificate parameters", error))?;
        let mut name = DistinguishedName::new();
        name.push(DnType::CommonName, "maison");
        params.distinguished_name = name;

        let certificate = params
            .self_signed(&key_pair)
            .map_err(|error| internal("could not self-sign", error))?;
        stored_form(&key_der, certificate.der(), None)
    }

    fn from_stored(stored: &[u8], path: &Path) -> Result<Self, AppError> {
        let read_chunk = |offset: usize| -> Option<(Vec<u8>, usize)> {
            let len_bytes: [u8; 4] = stored.get(offset..offset + 4)?.try_into().ok()?;
            let len = usize::try_from(u32::from_le_bytes(len_bytes)).ok()?;
            let start = offset + 4;
            let end = start.checked_add(len)?;
            Some((stored.get(start..end)?.to_vec(), end))
        };

        let corrupt = || internal("corrupt Android TV identity", path.display());
        let (key_der, next) = read_chunk(0).ok_or_else(corrupt)?;
        let (cert_der, next) = read_chunk(next).ok_or_else(corrupt)?;
        // absent from identities kept before pinning, and until the first session
        let pinned = match read_chunk(next) {
            Some((pin, _)) => Some(Fingerprint::try_from(pin.as_slice()).map_err(|_| corrupt())?),
            None if next == stored.len() => None,
            None => return Err(corrupt()),
        };

        // The pairing hash wants the public numbers, so read them back out of
        // the key we just loaded rather than re-deriving them from the cert.
        let rsa_key = crate::adb::decode_key(&key_der)?;
        let public = rsa::RsaPublicKey::from(&rsa_key);
        let (modulus, exponent) = public_numbers(&public);

        Ok(Self {
            certificate: CertificateDer::from(cert_der),
            key_der,
            modulus,
            exponent,
            pinned,
            path: path.to_path_buf(),
        })
    }
}

/// The identity as kept: each part length-prefixed (see `Identity::mint`).
fn stored_form(
    key_der: &[u8],
    cert_der: &[u8],
    pinned: Option<Fingerprint>,
) -> Result<Vec<u8>, AppError> {
    let mut stored = Vec::with_capacity(12 + key_der.len() + cert_der.len() + 32);
    let parts: [Option<&[u8]>; 3] = [
        Some(key_der),
        Some(cert_der),
        pinned.as_ref().map(<[u8; 32]>::as_slice),
    ];
    for part in parts.into_iter().flatten() {
        let length = u32::try_from(part.len())
            .map_err(|_| internal("Android TV identity too large", part.len()))?;
        stored.extend_from_slice(&length.to_le_bytes());
        stored.extend_from_slice(part);
    }
    Ok(stored)
}

/// Big-endian modulus and exponent, trimmed of leading zeroes.
///
/// The trimming matters: the reference implementation formats them as
/// uppercase hex without padding, so a leading zero byte here would change the
/// hash and make every pairing attempt fail its checksum.
fn public_numbers(public: &rsa::RsaPublicKey) -> (Vec<u8>, Vec<u8>) {
    use rsa::traits::PublicKeyParts;

    let trim = |bytes: Box<[u8]>| {
        let first = bytes.iter().position(|b| *b != 0).unwrap_or(bytes.len());
        bytes[first..].to_vec()
    };
    (
        trim(public.n().as_ref().to_be_bytes()),
        trim(public.e().to_be_bytes()),
    )
}

// ------------------------------------------------------------------- TLS ---

/// The television's certificate: self-signed by a device on the local network with no
/// name to verify, so the one it presented after pairing is pinned (trust on first use)
/// and its handshake signatures are checked against it. Unpinned (pairing, or the first
/// session after it), any is taken: pairing proves itself through its secret.
#[derive(Debug)]
struct TelevisionCert {
    pinned: Option<Fingerprint>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl TelevisionCert {
    fn new(pinned: Option<Fingerprint>) -> Self {
        Self {
            pinned,
            algorithms: rustls::crypto::aws_lc_rs::default_provider()
                .signature_verification_algorithms,
        }
    }
}

impl ServerCertVerifier for TelevisionCert {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.pinned {
            Some(pinned) if fingerprint(end_entity) != pinned => Err(rustls::Error::General(
                "not the certificate the TV presented after pairing".into(),
            )),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// A TLS session to the TV at `host:port` (its certificate `pinned` when given), and the
/// fingerprint of the certificate it presented.
async fn connect_tls(
    host: &str,
    port: u16,
    identity: &Identity,
    pinned: Option<Fingerprint>,
) -> Result<(TlsStream<TcpStream>, Fingerprint), AppError> {
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TelevisionCert::new(pinned)))
        .with_client_auth_cert(vec![identity.certificate.clone()], identity.key())
        .map_err(|error| internal("TLS client setup failed", error))?;

    let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| unreachable(TV, format!("{host}:{port} timed out")))?
        .map_err(|error| unreachable(TV, error))?;
    // The name is irrelevant (the certificate is pinned, not named), but rustls requires
    // one, so use a fixed placeholder rather than the IP, which would not parse as a DNS
    // name.
    let server_name = ServerName::from(
        rustls::pki_types::DnsName::try_from_str("androidtv")
            .map_err(|error| internal("TLS name", error))?,
    );

    // Bounded too: a TV that accepts the connection and then stalls would otherwise hold
    // the caller (and the manager's session lock) as long as it likes.
    let stream = timeout(
        IO_TIMEOUT,
        TlsConnector::from(Arc::new(config)).connect(server_name, stream),
    )
    .await
    .map_err(|_| unreachable(TV, "TLS handshake timed out"))?
    .map_err(|error| unreachable(TV, format!("TLS handshake failed: {error}")))?;
    let presented = stream
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|chain| chain.first())
        .map(fingerprint)
        .ok_or_else(|| unreachable(TV, "the TV presented no certificate"))?;
    Ok((stream, presented))
}

// --------------------------------------------------------------- framing ---

async fn write_framed<W>(writer: &mut W, payload: Vec<u8>) -> Result<(), AppError>
where
    W: AsyncWriteExt + Unpin,
{
    timeout(IO_TIMEOUT, writer.write_all(&payload))
        .await
        .map_err(|_| unreachable(TV, "timed out writing to the TV"))?
        .map_err(|error| unreachable(TV, error))
}

/// Reads one length-delimited message. The length is a varint, so it is read
/// a byte at a time before the payload can be sized. Not cancellation safe: a
/// message read half-way is lost, so a session reads from one task that nothing
/// interrupts.
async fn read_framed<R>(reader: &mut R, budget: Duration) -> Result<Vec<u8>, AppError>
where
    R: AsyncReadExt + Unpin,
{
    let mut decoder = Varint::default();
    let length = loop {
        let mut byte = [0u8; 1];
        timeout(budget, reader.read_exact(&mut byte))
            .await
            .map_err(|_| unreachable(TV, "timed out reading from the TV"))?
            .map_err(|error| unreachable(TV, error))?;
        match decoder.push(byte[0]) {
            Ok(Some(length)) => break length,
            Ok(None) => {}
            Err(VarintOverflow) => return Err(unreachable(TV, "malformed message length")),
        }
    };

    let length = usize::try_from(length)
        .ok()
        .filter(|length| *length <= MAX_MESSAGE_BYTES)
        .ok_or_else(|| unreachable(TV, "oversized message from the TV"))?;
    let mut payload = vec![0u8; length];
    if length > 0 {
        timeout(budget, reader.read_exact(&mut payload))
            .await
            .map_err(|_| unreachable(TV, "timed out reading a message"))?
            .map_err(|error| unreachable(TV, error))?;
    }
    Ok(payload)
}

// --------------------------------------------------------------- pairing ---

/// A pairing in progress: the TV is showing its code and waiting for proof
/// that someone can read the screen.
pub struct Pairing {
    stream: TlsStream<TcpStream>,
    client: (Vec<u8>, Vec<u8>),
    server: (Vec<u8>, Vec<u8>),
    /// Ours, to forget the TV's certificate once paired (the next session pins its own).
    identity: Identity,
}

fn outer_message(field: u32, build: impl FnOnce(&mut ProtoBuf)) -> Vec<u8> {
    let mut msg = ProtoBuf::default();
    // protocol_version 2. The schema defaults it to 1, but a TV answers
    // STATUS_ERROR to anything that claims version 1.
    msg.int(1, 2);
    msg.int(2, 200); // STATUS_OK
    msg.message(field, build);
    msg.frame()
}

/// Fails unless the message carries STATUS_OK, so a rejected step surfaces
/// where it happens rather than as a puzzling silence two exchanges later.
fn expect_ok(payload: &[u8], step: &str) -> Result<(), AppError> {
    let mut reader = ProtoReader::new(payload);
    while let Some((field, value)) = reader.next_field() {
        if field == 2 {
            if let Field::Varint(status) = value {
                return match status {
                    200 => Ok(()),
                    402 => Err(refused(
                        Refusal::Invalid,
                        "the TV rejected the pairing code",
                    )),
                    other => Err(unreachable(
                        TV,
                        format!("the TV refused {step} (status {other})"),
                    )),
                };
            }
        }
    }
    Err(unreachable(
        TV,
        format!("no status in the TV's answer to {step}"),
    ))
}

impl Pairing {
    /// Opens the pairing session; the TV displays a six hex-digit code once
    /// this returns.
    pub async fn start(host: &str, identity: &Identity) -> Result<Self, AppError> {
        let (stream, _) = connect_tls(host, PAIRING_PORT, identity, None).await?;

        // The pairing hash needs the TV's public numbers, which are only
        // available from the certificate it just presented.
        let server_cert = stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|chain| chain.first().cloned())
            .ok_or_else(|| unreachable(TV, "the TV presented no certificate"))?;
        let server = server_public_numbers(&server_cert)?;

        let mut pairing = Self {
            stream,
            client: (identity.modulus.clone(), identity.exponent.clone()),
            server,
            identity: identity.clone(),
        };

        pairing
            .exchange(
                outer_message(10, |m| {
                    // The service name is not free-form: the TV matches it
                    // against the remote service it expects to be talking to.
                    m.string(1, "atvremote");
                    m.string(2, "maison");
                }),
                "the pairing request",
            )
            .await?;

        pairing
            .exchange(
                outer_message(20, |m| {
                    // input_encodings: six hexadecimal symbols, which is what
                    // the TV shows on screen.
                    m.message(1, |enc| {
                        enc.int(1, 3); // ENCODING_TYPE_HEXADECIMAL
                        enc.int(2, 6);
                    });
                    m.int(3, 1); // ROLE_TYPE_INPUT
                }),
                "the pairing options",
            )
            .await?;

        pairing
            .exchange(
                outer_message(30, |m| {
                    m.message(1, |enc| {
                        enc.int(1, 3);
                        enc.int(2, 6);
                    });
                    m.int(2, 1); // client_role
                }),
                "the pairing configuration",
            )
            .await?;

        Ok(pairing)
    }

    async fn exchange(&mut self, payload: Vec<u8>, step: &str) -> Result<(), AppError> {
        write_framed(&mut self.stream, payload).await?;
        let answer = read_framed(&mut self.stream, IO_TIMEOUT).await?;
        expect_ok(&answer, step)
    }

    /// Completes pairing with the code shown on the TV.
    ///
    /// The proof is a SHA-256 over both public keys and the code's last two
    /// bytes. Its first byte must equal the code's first byte — which lets a
    /// mistyped code be caught here, before the TV is asked to reject it.
    pub async fn finish(mut self, code: &str) -> Result<(), AppError> {
        let code = code.trim();
        let code_bytes = crate::util::unhex(code)
            .filter(|bytes| bytes.len() == 3)
            .ok_or_else(|| refused(Refusal::Invalid, "a pairing code is six hexadecimal digits"))?;

        let mut hasher = Sha256::new();
        hasher.update(&self.client.0);
        hasher.update(&self.client.1);
        hasher.update(&self.server.0);
        hasher.update(&self.server.1);
        hasher.update(&code_bytes[1..]);
        let alpha = hasher.finalize();

        if alpha[0] != code_bytes[0] {
            return Err(refused(Refusal::Invalid, "wrong pairing code"));
        }

        write_framed(
            &mut self.stream,
            outer_message(40, |m| m.bytes_field(1, &alpha)),
        )
        .await?;
        let answer = read_framed(&mut self.stream, PAIRING_TIMEOUT).await?;
        expect_ok(&answer, "the pairing secret")?;
        // paired anew, maybe with another TV: its certificate is pinned at the next session
        self.identity.pin(None).await
    }
}

/// Modulus and exponent from the TV's certificate, trimmed like the client's.
fn server_public_numbers(cert: &CertificateDer<'_>) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    use x509_parser::{prelude::*, public_key::PublicKey};

    let (_, parsed) = X509Certificate::from_der(cert.as_ref())
        .map_err(|error| unreachable(TV, format!("unreadable TV certificate: {error}")))?;
    let public_key = parsed.public_key();
    let rsa = public_key
        .parsed()
        .map_err(|error| unreachable(TV, format!("unreadable TV public key: {error}")))?;

    match rsa {
        PublicKey::RSA(rsa) => {
            let trim = |bytes: &[u8]| {
                let first = bytes.iter().position(|b| *b != 0).unwrap_or(bytes.len());
                bytes[first..].to_vec()
            };
            Ok((trim(rsa.modulus), trim(rsa.exponent)))
        }
        _ => Err(unreachable(TV, "the TV's certificate is not RSA")),
    }
}

// --------------------------------------------------------------- session ---

/// Stops the session's reader with the session.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// What the client answers to one field of the TV's message, if anything.
fn answer_to(field: u32, value: &Field<'_>) -> Option<Vec<u8>> {
    let mut m = ProtoBuf::default();
    match (field, value) {
        // remote_configure: echo our features and a device description back.
        (1, _) => m.message(1, |cfg| {
            cfg.int(1, ACTIVE_FEATURES);
            cfg.message(2, |info| {
                info.string(1, "maison");
                info.string(2, "uplg");
                info.int(3, 1);
                info.string(4, "1");
                info.string(5, "maison");
                info.string(6, "1.0.0");
            });
        }),
        // remote_set_active: acknowledge, and the session is usable from here on.
        (2, _) => m.message(2, |active| active.int(1, ACTIVE_FEATURES)),
        // remote_ping_request: echo val1 back. A missed ping drops the connection.
        (8, Field::Bytes(body)) => {
            let mut inner = ProtoReader::new(body);
            let mut val1 = 0i64;
            while let Some((f, v)) = inner.next_field() {
                if let (1, Field::Varint(value)) = (f, v) {
                    val1 = value as i64;
                }
            }
            m.message(9, |pong| pong.int(1, val1));
        }
        _ => return None,
    }
    Some(m.frame())
}

/// What `remote_start` (field 40) says: whether the TV is on. It comes once the session is
/// active, then at each power change.
fn started(field: u32, value: &Field<'_>) -> Option<bool> {
    let (40, Field::Bytes(body)) = (field, value) else {
        return None;
    };
    let mut inner = ProtoReader::new(body);
    let mut on = false;
    while let Some((f, v)) = inner.next_field() {
        if let (1, Field::Varint(value)) = (f, v) {
            on = value != 0;
        }
    }
    Some(on)
}

/// Direction SHORT: a press and release in one message, which is what a tap
/// on a remote is. Long presses would need START_LONG/END_LONG around a hold.
const DIRECTION_SHORT: i64 = 3;

/// An open remote session.
///
/// The TV drives the conversation — it asks for configuration, then declares
/// the session active, then pings for as long as it lives — so a background
/// task owns the stream and answers on its own. Commands reach that task
/// through a channel, which is also what keeps sends ordered without a lock.
#[derive(Clone)]
pub struct Session {
    commands: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// Whether the TV says it is on (`remote_start`), once it said so.
    power: watch::Receiver<Option<bool>>,
}

impl Session {
    /// A session to the TV at `host`, its certificate checked against the pinned one; the
    /// first after pairing pins what it presents.
    pub async fn connect(host: &str, identity: &mut Identity) -> Result<Self, AppError> {
        let (stream, presented) = connect_tls(host, REMOTE_PORT, identity, identity.pinned).await?;
        if identity.pinned.is_none() {
            tracing::info!(%host, "the TV's certificate pinned: another will be refused");
            identity.pin(Some(presented)).await?;
        }
        let (reader, writer) = tokio::io::split(stream);
        Self::start(reader, writer).await
    }

    /// Runs the session over an open stream; ready once the TV has made it active.
    async fn start<R, W>(mut reader: R, mut writer: W) -> Result<Self, AppError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (commands, mut rx) = mpsc::channel::<Vec<u8>>(64);
        let (ready, wait_ready) = oneshot::channel::<()>();
        let (power_tx, power) = watch::channel(None);

        // The reader owns the read half and is never interrupted, so a key press
        // arriving mid-message cannot cut a frame in two. Its read budget is also the
        // liveness check: no ping for SESSION_SILENCE ends the session.
        let (incoming_tx, mut incoming) = mpsc::channel::<Vec<u8>>(8);
        let reader_task = tokio::spawn(async move {
            loop {
                match read_framed(&mut reader, SESSION_SILENCE).await {
                    Ok(payload) => {
                        if incoming_tx.send(payload).await.is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        tracing::debug!(%error, "remote session over");
                        return;
                    }
                }
            }
        });

        tokio::spawn(async move {
            let _reader = AbortOnDrop(reader_task);
            let mut ready = Some(ready);
            loop {
                tokio::select! {
                    // Anything the TV says; `None` once the reader has stopped.
                    payload = incoming.recv() => {
                        let Some(payload) = payload else { return };
                        let mut parser = ProtoReader::new(&payload);
                        while let Some((field, value)) = parser.next_field() {
                            if let Some(on) = started(field, &value) {
                                power_tx.send_replace(Some(on));
                            }
                            let Some(answer) = answer_to(field, &value) else { continue };
                            if write_framed(&mut writer, answer).await.is_err() {
                                return;
                            }
                            // remote_set_active answered: the session is usable
                            if field == 2 {
                                if let Some(ready) = ready.take() {
                                    // nobody waits any more: the caller gave up, the session goes on
                                    ready.send(()).ok();
                                }
                            }
                        }
                    }

                    // Something for the TV.
                    command = rx.recv() => {
                        let Some(command) = command else { return };
                        if write_framed(&mut writer, command).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        match timeout(NEGOTIATION_TIMEOUT, wait_ready).await {
            Ok(Ok(())) => Ok(Self { commands, power }),
            Ok(Err(_)) => Err(unreachable(
                TV,
                "the remote session ended during negotiation",
            )),
            Err(_) => Err(unreachable(
                TV,
                "the TV did not open a remote session (is it paired?)",
            )),
        }
    }

    async fn send(&self, payload: Vec<u8>) -> Result<(), AppError> {
        self.commands
            .send(payload)
            .await
            .map_err(|_| unreachable(TV, "the remote session is closed"))
    }

    /// Whether the TV is on as it says it (`None` until it said), as it changes.
    pub fn power(&self) -> watch::Receiver<Option<bool>> {
        self.power.clone()
    }

    /// Taps a key. Codes are the standard Android ones, shared with ADB.
    pub async fn key(&self, key_code: i64) -> Result<(), AppError> {
        let mut m = ProtoBuf::default();
        m.message(10, |inject| {
            inject.int(1, key_code);
            inject.int(2, DIRECTION_SHORT);
        });
        self.send(m.frame()).await
    }

    /// Launches an app link. A bare package name is turned into the
    /// `market://launch?id=…` form the TV expects — this is the mechanism
    /// behind the Netflix and Prime buttons on a physical remote.
    pub async fn launch(&self, app_link_or_package: &str) -> Result<(), AppError> {
        let link = if app_link_or_package.contains("://") {
            app_link_or_package.to_string()
        } else {
            format!("market://launch?id={app_link_or_package}")
        };
        let mut m = ProtoBuf::default();
        m.message(90, |request| request.string(1, &link));
        self.send(m.frame()).await
    }

    /// False once the background task has stopped, so callers can reconnect.
    pub fn is_open(&self) -> bool {
        !self.commands.is_closed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_field(payload: &[u8]) -> (u32, Option<u64>) {
        let mut reader = ProtoReader::new(payload);
        let (field, value) = reader.next_field().expect("a field");
        let scalar = match value {
            Field::Bytes(body) => match ProtoReader::new(body).next_field() {
                Some((_, Field::Varint(v))) => Some(v),
                _ => None,
            },
            _ => None,
        };
        (field, scalar)
    }

    fn varint(value: u64) -> Vec<u8> {
        let mut m = ProtoBuf::default();
        m.varint(value);
        m.bytes
    }

    /// One decoder for the messages and their length prefix: every u64 round-trips, and
    /// more than 64 bits is refused, never wrapped.
    #[tokio::test]
    async fn the_varint_decoder_round_trips_and_refuses_overflow() {
        for value in [
            0,
            1,
            127,
            128,
            300,
            u64::from(u32::MAX),
            u64::MAX - 1,
            u64::MAX,
        ] {
            let bytes = varint(value);
            assert_eq!(ProtoReader::new(&bytes).varint(), Some(value), "{value}");
        }
        // eleven bytes, or a tenth byte past the 64th bit
        let mut too_long = vec![0xff; 10];
        too_long.push(0x01);
        assert_eq!(ProtoReader::new(&too_long).varint(), None);
        let mut past_64 = vec![0xff; 9];
        past_64.push(0x02);
        assert_eq!(ProtoReader::new(&past_64).varint(), None);
        assert!(read_framed(&mut past_64.as_slice(), Duration::from_secs(1))
            .await
            .is_err());
        // a field number past u32 is malformed, not truncated
        let key = varint((u64::from(u32::MAX) + 1) << 3);
        assert!(ProtoReader::new(&key).next_field().is_none());
        // a length past the buffer is refused
        let mut field = varint(2 << 3 | 2);
        field.extend(varint(u64::MAX));
        assert!(ProtoReader::new(&field).next_field().is_none());
    }

    #[tokio::test]
    async fn an_oversized_length_is_refused_before_allocating() {
        for length in [MAX_MESSAGE_BYTES as u64 + 1, 1 << 32, u64::MAX >> 1] {
            let bytes = varint(length);
            let mut reader = bytes.as_slice();
            assert!(
                read_framed(&mut reader, Duration::from_secs(1))
                    .await
                    .is_err(),
                "{length}"
            );
        }
        let mut frame = varint(3);
        frame.extend_from_slice(b"abc");
        assert_eq!(
            read_framed(&mut frame.as_slice(), Duration::from_secs(1))
                .await
                .unwrap(),
            b"abc"
        );
    }

    #[tokio::test]
    async fn a_frame_split_across_reads_decodes() {
        let (mut tv, mut client) = tokio::io::duplex(1024);
        let payload = vec![7u8; 300];
        let mut frame = varint(payload.len() as u64);
        frame.extend_from_slice(&payload);
        let (head, tail) = frame.split_at(1);
        let (head, tail) = (head.to_vec(), tail.to_vec());
        let writer = tokio::spawn(async move {
            tv.write_all(&head).await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            tv.write_all(&tail[..100]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            tv.write_all(&tail[100..]).await.unwrap();
            tv
        });
        assert_eq!(
            read_framed(&mut client, Duration::from_secs(2))
                .await
                .unwrap(),
            payload
        );
        writer.await.unwrap();
    }

    /// The bug this guards: a key press arriving while a ping was half read cut the ping
    /// in two, and every message after it was garbage.
    #[tokio::test]
    async fn key_presses_mid_frame_keep_the_framing() {
        // a pipe of 2 bytes: a write past them waits for the reader
        let (client, mut tv) = tokio::io::duplex(2);
        let (reader, writer) = tokio::io::split(client);
        let starting = tokio::spawn(Session::start(reader, writer));

        let mut active = ProtoBuf::default();
        active.message(2, |m| m.int(1, ACTIVE_FEATURES));
        tv.write_all(&active.frame()).await.unwrap();
        let answer = read_framed(&mut tv, Duration::from_secs(2)).await.unwrap();
        assert_eq!(first_field(&answer).0, 2, "set_active acknowledged");
        let session = starting.await.unwrap().expect("session ready");

        let mut ping = ProtoBuf::default();
        ping.message(8, |m| m.int(1, 42));
        let ping = ping.frame();
        // the pipe holds 2 bytes: once the third is in, the reader has the ping's head
        tv.write_all(&ping[..2]).await.unwrap();
        tv.write_all(&ping[2..3]).await.unwrap();
        session.key(20).await.unwrap();
        let key = read_framed(&mut tv, Duration::from_secs(2)).await.unwrap();
        assert_eq!(first_field(&key), (10, Some(20)), "the key went out");
        tv.write_all(&ping[3..]).await.unwrap();
        let pong = read_framed(&mut tv, Duration::from_secs(2)).await.unwrap();
        assert_eq!(first_field(&pong), (9, Some(42)), "the ping was read whole");
        assert!(session.is_open());

        // the TV says it is on, then off
        let mut power = session.power();
        for on in [1, 0] {
            let mut start = ProtoBuf::default();
            start.message(40, |m| m.int(1, on));
            tv.write_all(&start.frame()).await.unwrap();
            power.changed().await.unwrap();
            assert_eq!(*power.borrow_and_update(), Some(on == 1));
        }

        // the TV going away ends the session
        drop(tv);
        session.commands.closed().await;
        assert!(!session.is_open());
    }

    /// A local TLS server standing in for the TV, presenting `certificate`.
    async fn tv_stub(certificate: rcgen::CertifiedKey<rcgen::KeyPair>) -> u16 {
        use tokio_rustls::rustls::ServerConfig;
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.cert.der().clone()],
                PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
            )
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move { acceptor.accept(stream).await });
            }
        });
        port
    }

    /// Trust on first use: unpinned, any certificate is taken and its fingerprint said;
    /// pinned, only that one (another TV, a man in the middle: refused at the handshake).
    #[tokio::test]
    async fn the_tv_certificate_is_pinned() {
        let dir = crate::util::test_dir();
        let mut identity = Identity::load_or_create(&dir.path().join("atv-identity")).unwrap();
        let tv = rcgen::generate_simple_self_signed(vec!["androidtv".into()]).unwrap();
        let tv_fingerprint = fingerprint(tv.cert.der());
        let port = tv_stub(tv).await;

        let (_, presented) = connect_tls("127.0.0.1", port, &identity, None)
            .await
            .unwrap();
        assert_eq!(presented, tv_fingerprint);
        connect_tls("127.0.0.1", port, &identity, Some(tv_fingerprint))
            .await
            .expect("the pinned TV");
        let other = rcgen::generate_simple_self_signed(vec!["androidtv".into()]).unwrap();
        let impostor = tv_stub(other).await;
        assert!(
            connect_tls("127.0.0.1", impostor, &identity, Some(tv_fingerprint))
                .await
                .is_err(),
            "another certificate is refused"
        );

        // the pin is kept with the identity, and pairing again forgets it
        identity.pin(Some(tv_fingerprint)).await.unwrap();
        let reloaded = Identity::load_or_create(&dir.path().join("atv-identity")).unwrap();
        assert_eq!(reloaded.pinned, Some(tv_fingerprint));
        assert_eq!(
            reloaded.certificate, identity.certificate,
            "the same identity"
        );
        identity.pin(None).await.unwrap();
        let reloaded = Identity::load_or_create(&dir.path().join("atv-identity")).unwrap();
        assert_eq!(reloaded.pinned, None);
    }

    #[tokio::test]
    async fn a_torn_identity_is_replaced() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("atv-identity");
        std::fs::write(&path, b"torn").unwrap();
        let identity = Identity::load_or_create(&path).expect("a new identity");
        let again = Identity::load_or_create(&path).expect("kept");
        assert_eq!(identity.certificate, again.certificate);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
