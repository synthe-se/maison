//! The edge (the backend's `edge.rs`): its listener's TLS, both ways. A client presents a
//! certificate one of the edge's CAs signed (`maison-backend edge init` for a reverse proxy,
//! `edge client` for another, a wall frame on the LAN), and checks the house's certificate by the
//! edge's CA and this name: the house is reached by address, and an address is no name to check.

/// The name the house's edge certificate carries.
pub const SERVER_NAME: &str = "maison.home.arpa";

/// What `edge init` and `edge client` give a client: the CA to check the house by, its own
/// certificate and key (PEM).
pub const CA_FILE: &str = "ca.pem";
pub const CLIENT_CERT_FILE: &str = "client.pem";
pub const CLIENT_KEY_FILE: &str = "client-key.pem";
