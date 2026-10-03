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

use std::{
    path::Path,
    sync::Arc,
    time::Duration,
};

use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, oneshot},
    time::timeout,
};
use tokio_rustls::{
    client::TlsStream,
    rustls::{
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
        ClientConfig, DigitallySignedStruct, SignatureScheme,
    },
    TlsConnector,
};

use crate::{error::AppError, net::unreachable, store};

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
pub struct Identity {
    pub certificate: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    /// Kept for the pairing hash, which needs the raw public numbers.
    modulus: Vec<u8>,
    exponent: Vec<u8>,
}

impl Identity {
    /// Loads the stored identity, minting one on first use. Generation is
    /// slow on an ARMv6 Pi, so callers should keep this off the async runtime.
    /// A torn file is replaced (pairing again is the price), never a dead end.
    pub fn load_or_create(path: &Path) -> Result<Self, AppError> {
        store::load_or_create_secret(
            path,
            |stored| Self::from_stored(stored).ok(),
            || {
                let stored = Self::mint()?;
                Ok((Self::from_stored(&stored)?, stored))
            },
        )
    }

    /// Stored form is the PKCS#8 key followed by the certificate, each
    /// length-prefixed — enough structure to reload, without pulling in PEM.
    fn mint() -> Result<Vec<u8>, AppError> {
        use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_RSA_SHA256};

        let rsa_key = crate::adb::generate_key()?;
        let key_der = crate::adb::encode_key(&rsa_key)?;

        let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(
            &rustls::pki_types::PrivatePkcs8KeyDer::from(key_der.clone()),
            &PKCS_RSA_SHA256,
        )
        .map_err(|error| {
            AppError::service_unavailable(format!("could not wrap the ADB key for TLS: {error}"))
        })?;

        let mut params = CertificateParams::new(vec!["maison".to_string()]).map_err(|error| {
            AppError::service_unavailable(format!("bad certificate parameters: {error}"))
        })?;
        let mut name = DistinguishedName::new();
        name.push(DnType::CommonName, "maison");
        params.distinguished_name = name;

        let certificate = params.self_signed(&key_pair).map_err(|error| {
            AppError::service_unavailable(format!("could not self-sign: {error}"))
        })?;

        let cert_der = certificate.der().to_vec();
        let mut stored = Vec::with_capacity(8 + key_der.len() + cert_der.len());
        let length = |bytes: &[u8]| {
            u32::try_from(bytes.len())
                .map(u32::to_le_bytes)
                .map_err(|_| AppError::service_unavailable("Android TV identity too large"))
        };
        stored.extend_from_slice(&length(&key_der)?);
        stored.extend_from_slice(&key_der);
        stored.extend_from_slice(&length(&cert_der)?);
        stored.extend_from_slice(&cert_der);
        Ok(stored)
    }

    fn from_stored(stored: &[u8]) -> Result<Self, AppError> {
        let read_chunk = |offset: usize| -> Option<(Vec<u8>, usize)> {
            let len_bytes: [u8; 4] = stored.get(offset..offset + 4)?.try_into().ok()?;
            let len = usize::try_from(u32::from_le_bytes(len_bytes)).ok()?;
            let start = offset + 4;
            let end = start.checked_add(len)?;
            Some((stored.get(start..end)?.to_vec(), end))
        };

        let (key_der, next) = read_chunk(0)
            .ok_or_else(|| AppError::service_unavailable("corrupt Android TV identity"))?;
        let (cert_der, _) = read_chunk(next)
            .ok_or_else(|| AppError::service_unavailable("corrupt Android TV identity"))?;

        // The pairing hash wants the public numbers, so read them back out of
        // the key we just loaded rather than re-deriving them from the cert.
        let rsa_key = crate::adb::decode_key(&key_der)?;
        let public = rsa::RsaPublicKey::from(&rsa_key);
        let (modulus, exponent) = public_numbers(&public);

        Ok(Self {
            certificate: CertificateDer::from(cert_der),
            key: PrivateKeyDer::try_from(key_der)
                .map_err(|error| AppError::service_unavailable(format!("bad key: {error}")))?,
            modulus,
            exponent,
        })
    }
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

/// Accepts whatever certificate the television presents.
///
/// It is self-signed by a device on the local network with no name we could
/// verify, so there is nothing to check against. Confidentiality still holds,
/// and authentication runs the other way: the TV verifies *us*, via the
/// client certificate it accepted during pairing.
#[derive(Debug)]
struct AcceptTelevisionCert;

impl ServerCertVerifier for AcceptTelevisionCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
        ]
    }
}

async fn connect_tls(
    host: &str,
    port: u16,
    identity: &Identity,
) -> Result<TlsStream<TcpStream>, AppError> {
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptTelevisionCert))
        .with_client_auth_cert(
            vec![identity.certificate.clone()],
            identity.key.clone_key(),
        )
        .map_err(|error| {
            AppError::service_unavailable(format!("TLS client setup failed: {error}"))
        })?;

    let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| unreachable("Android TV", format!("{host}:{port} timed out")))?
        .map_err(|error| unreachable("Android TV", error))?;
    // The name is irrelevant — the verifier above accepts anything — but
    // rustls requires one, so use a fixed placeholder rather than the IP,
    // which would not parse as a DNS name.
    let server_name = ServerName::try_from("androidtv").expect("static name is valid");

    // Bounded too: a TV that accepts the connection and then stalls would otherwise hold
    // the caller (and the manager's session lock) as long as it likes.
    timeout(IO_TIMEOUT, TlsConnector::from(Arc::new(config)).connect(server_name, stream))
        .await
        .map_err(|_| unreachable("Android TV", "TLS handshake timed out"))?
        .map_err(|error| unreachable("Android TV", format!("TLS handshake failed: {error}")))
}

// --------------------------------------------------------------- framing ---

async fn write_framed<W>(writer: &mut W, payload: Vec<u8>) -> Result<(), AppError>
where
    W: AsyncWriteExt + Unpin,
{
    timeout(IO_TIMEOUT, writer.write_all(&payload))
        .await
        .map_err(|_| AppError::service_unavailable("timed out writing to the TV"))??;
    Ok(())
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
            .map_err(|_| AppError::service_unavailable("timed out reading from the TV"))??;
        match decoder.push(byte[0]) {
            Ok(Some(length)) => break length,
            Ok(None) => {}
            Err(VarintOverflow) => return Err(AppError::service_unavailable("malformed message length")),
        }
    };

    let length = usize::try_from(length)
        .ok()
        .filter(|length| *length <= MAX_MESSAGE_BYTES)
        .ok_or_else(|| AppError::service_unavailable("oversized message from the TV"))?;
    let mut payload = vec![0u8; length];
    if length > 0 {
        timeout(budget, reader.read_exact(&mut payload))
            .await
            .map_err(|_| AppError::service_unavailable("timed out reading a message"))??;
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
                    402 => Err(AppError::service_unavailable(
                        "the TV rejected the pairing code",
                    )),
                    other => Err(AppError::service_unavailable(format!(
                        "the TV refused {step} (status {other})"
                    ))),
                };
            }
        }
    }
    Err(AppError::service_unavailable(format!(
        "no status in the TV's answer to {step}"
    )))
}

impl Pairing {
    /// Opens the pairing session; the TV displays a six hex-digit code once
    /// this returns.
    pub async fn start(host: &str, identity: &Identity) -> Result<Self, AppError> {
        let stream = connect_tls(host, PAIRING_PORT, identity).await?;

        // The pairing hash needs the TV's public numbers, which are only
        // available from the certificate it just presented.
        let server_cert = stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|chain| chain.first().cloned())
            .ok_or_else(|| {
                AppError::service_unavailable("the TV presented no certificate")
            })?;
        let server = server_public_numbers(&server_cert)?;

        let mut pairing = Self {
            stream,
            client: (identity.modulus.clone(), identity.exponent.clone()),
            server,
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
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AppError::bad_request("the pairing code is six hexadecimal digits"));
        }
        let code_bytes = decode_hex(code).ok_or_else(|| {
            AppError::bad_request("the pairing code is not valid hexadecimal")
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&self.client.0);
        hasher.update(&self.client.1);
        hasher.update(&self.server.0);
        hasher.update(&self.server.1);
        hasher.update(&code_bytes[1..]);
        let alpha = hasher.finalize();

        if alpha[0] != code_bytes[0] {
            return Err(AppError::bad_request("wrong pairing code"));
        }

        write_framed(
            &mut self.stream,
            outer_message(40, |m| m.bytes_field(1, &alpha)),
        )
        .await?;
        let answer = read_framed(&mut self.stream, PAIRING_TIMEOUT).await?;
        expect_ok(&answer, "the pairing secret")
    }
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    value.len().is_multiple_of(2)
        .then(|| {
            (0..value.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&value[i..i + 2], 16).ok())
                .collect::<Option<Vec<u8>>>()
        })
        .flatten()
}

/// Modulus and exponent from the TV's certificate, trimmed like the client's.
fn server_public_numbers(cert: &CertificateDer<'_>) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    use x509_parser::{prelude::*, public_key::PublicKey};

    let (_, parsed) = X509Certificate::from_der(cert.as_ref()).map_err(|error| {
        AppError::service_unavailable(format!("unreadable TV certificate: {error}"))
    })?;
    let public_key = parsed.public_key();
    let rsa = public_key.parsed().map_err(|error| {
        AppError::service_unavailable(format!("unreadable TV public key: {error}"))
    })?;

    match rsa {
        PublicKey::RSA(rsa) => {
            let trim = |bytes: &[u8]| {
                let first = bytes.iter().position(|b| *b != 0).unwrap_or(bytes.len());
                bytes[first..].to_vec()
            };
            Ok((trim(rsa.modulus), trim(rsa.exponent)))
        }
        _ => Err(AppError::service_unavailable(
            "the TV's certificate is not RSA",
        )),
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
}

impl Session {
    pub async fn connect(host: &str, identity: &Identity) -> Result<Self, AppError> {
        let stream = connect_tls(host, REMOTE_PORT, identity).await?;
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
                            let Some(answer) = answer_to(field, &value) else { continue };
                            if write_framed(&mut writer, answer).await.is_err() {
                                return;
                            }
                            // remote_set_active answered: the session is usable
                            if field == 2 {
                                if let Some(ready) = ready.take() {
                                    let _ = ready.send(());
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
            Ok(Ok(())) => Ok(Self { commands }),
            Ok(Err(_)) => Err(AppError::service_unavailable(
                "the remote session ended during negotiation",
            )),
            Err(_) => Err(AppError::service_unavailable(
                "the TV did not open a remote session (is it paired?)",
            )),
        }
    }

    async fn send(&self, payload: Vec<u8>) -> Result<(), AppError> {
        self.commands.send(payload).await.map_err(|_| {
            AppError::service_unavailable("the remote session is closed")
        })
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
        for value in [0, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX - 1, u64::MAX] {
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
        assert!(read_framed(&mut past_64.as_slice(), Duration::from_secs(1)).await.is_err());
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
            assert!(read_framed(&mut reader, Duration::from_secs(1)).await.is_err(), "{length}");
        }
        let mut frame = varint(3);
        frame.extend_from_slice(b"abc");
        assert_eq!(read_framed(&mut frame.as_slice(), Duration::from_secs(1)).await.unwrap(), b"abc");
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
        assert_eq!(read_framed(&mut client, Duration::from_secs(2)).await.unwrap(), payload);
        writer.await.unwrap();
    }

    /// The bug this guards: a key press arriving while a ping was half read cut the ping
    /// in two, and every message after it was garbage.
    #[tokio::test]
    async fn key_presses_mid_frame_keep_the_framing() {
        let (client, mut tv) = tokio::io::duplex(4096);
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
        tv.write_all(&ping[..2]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        session.key(20).await.unwrap();
        let key = read_framed(&mut tv, Duration::from_secs(2)).await.unwrap();
        assert_eq!(first_field(&key), (10, Some(20)), "the key went out");
        tv.write_all(&ping[2..]).await.unwrap();
        let pong = read_framed(&mut tv, Duration::from_secs(2)).await.unwrap();
        assert_eq!(first_field(&pong), (9, Some(42)), "the ping was read whole");
        assert!(session.is_open());

        // the TV going away ends the session
        drop(tv);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!session.is_open());
    }

    #[tokio::test]
    async fn a_torn_identity_is_replaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("atv-identity");
        std::fs::write(&path, b"torn").unwrap();
        let identity = Identity::load_or_create(&path).expect("a new identity");
        let again = Identity::load_or_create(&path).expect("kept");
        assert_eq!(identity.certificate, again.certificate);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
