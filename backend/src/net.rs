//! The network, said once: every HTTP client is built here (always with timeouts), and
//! every address a client may store for a LAN device is checked here.

use std::{net::Ipv4Addr, time::Duration};

use crate::error::AppError;

/// How long a TCP connection may take to open, for every client: a host that is gone
/// should not hold a request for the whole timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

// ------------------------------------------------------------------ clients --

/// The HTTP client for LAN devices: it never follows a redirect (a device must not send
/// the backend elsewhere).
pub fn device_client(timeout: Duration) -> Result<reqwest::Client, AppError> {
    Ok(reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT.min(timeout))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

/// The HTTP client for public web APIs (weather, geocoding, RTE): they may redirect (a few
/// hops), and say who asks.
pub fn web_client(user_agent: &str, timeout: Duration) -> Result<reqwest::Client, AppError> {
    Ok(reqwest::Client::builder()
        .user_agent(user_agent)
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT.min(timeout))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?)
}

/// A device's answer, refused past `max` bytes.
pub async fn read_capped(
    mut response: reqwest::Response,
    max: usize,
    device: &'static str,
) -> Result<Vec<u8>, AppError> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| unreachable(device, error))? {
        if body.len() + chunk.len() > max {
            return Err(AppError::service_unavailable(format!("{device} answered too much")));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// A device that did not answer, or answered nonsense. The client hears only that: the
/// library or network error stays in the log, so the answers cannot map which hosts and
/// ports exist.
pub fn unreachable(device: &'static str, error: impl std::fmt::Display) -> AppError {
    tracing::debug!(device, %error, "device unreachable");
    AppError::service_unavailable(format!("{device} unreachable"))
}

// ------------------------------------------------------------- LAN devices --
//
// Every address a client may store for a device goes through here, said once: only a
// private (RFC 1918) IPv4 or a `.local` name, never loopback, a public host, a URL or
// anything with `/ ? # @` in it. A member's setting must not turn the backend into a
// way to read other machines (SSRF).

const NOT_A_DEVICE: &str =
    "not a local device address (a private IPv4 such as 192.168.1.20, or a name ending in .local)";

/// A device host as kept in a setting, without a port.
pub fn device_host(value: &str) -> Result<String, AppError> {
    match device_address(value)? {
        (host, None) => Ok(host),
        (_, Some(_)) => Err(AppError::bad_request(format!("{value:?}: no port here"))),
    }
}

/// A device host with an optional port, for the protocols whose port varies.
pub fn device_address(value: &str) -> Result<(String, Option<u16>), AppError> {
    let refuse = || AppError::bad_request(format!("{value:?} is {NOT_A_DEVICE}"));
    let (host, port) = match value.rsplit_once(':') {
        Some((host, port)) => {
            let digits = !port.is_empty() && port.len() <= 5 && port.bytes().all(|b| b.is_ascii_digit());
            let port = port.parse::<u16>().ok().filter(|p| digits && *p != 0).ok_or_else(refuse)?;
            (host, Some(port))
        }
        None => (value, None),
    };
    if host.parse::<Ipv4Addr>().is_ok() {
        return device_ipv4(host).map(|ip| (ip.to_string(), port));
    }
    if is_local_name(host) {
        Ok((host.to_ascii_lowercase(), port))
    } else {
        Err(refuse())
    }
}

/// A private IPv4 (the protocols that speak IPv4 only: Broadlink, the plugs).
pub fn device_ipv4(value: &str) -> Result<Ipv4Addr, AppError> {
    value
        .parse::<Ipv4Addr>()
        .ok()
        .filter(Ipv4Addr::is_private)
        .ok_or_else(|| AppError::bad_request(format!("{value:?} is {NOT_A_DEVICE}")))
}

/// `name.local`, `living-room.tv.local`: DNS labels only.
fn is_local_name(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    let Some(name) = lower.strip_suffix(".local") else { return false };
    host.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// A hardware address as a key (a Bluetooth MAC, a Zigbee EUI-64, a platform peripheral
/// id): lowercase letters and digits only, so « AA:BB:… », « aa-bb-… » and « aabb… » are
/// the same device.
pub fn address_key(value: &str) -> String {
    value.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The single gate against SSRF: only what a LAN device can be gets through.
    #[test]
    fn device_addresses_accept_lan_devices_only() {
        for good in ["192.168.1.52", "10.0.0.3", "172.16.4.1", "172.31.255.254", "rabbit.local", "Living-Room.TV.local"] {
            assert!(device_host(good).is_ok(), "{good} should be accepted");
        }
        assert_eq!(device_host("Rabbit.LOCAL").unwrap(), "rabbit.local");
        assert_eq!(device_address("192.168.1.153:5555").unwrap(), ("192.168.1.153".into(), Some(5555)));
        assert_eq!(device_address("tv.local:1925").unwrap(), ("tv.local".into(), Some(1925)));
        for bad in [
            "", "127.0.0.1", "0.0.0.0", "169.254.169.254", "8.8.8.8", "172.32.0.1", "255.255.255.255",
            "224.0.0.1", "localhost", "example.com", "rabbit", ".local", "a..local", "-a.local",
            "http://192.168.1.2", "192.168.1.2/status", "192.168.1.2?x", "192.168.1.2#x",
            "user@192.168.1.2", "192.168.1.2 ", "x.local/../y", "[::1]", "::1", "192.168.1.2:",
            "192.168.1.2:0", "192.168.1.2:65536", "192.168.1.2:+80", "192.168.01.2",
        ] {
            assert!(device_address(bad).is_err(), "{bad:?} should be refused");
        }
        assert!(device_host("192.168.1.2:80").is_err(), "no port where none is expected");
        assert!(device_ipv4("rabbit.local").is_err(), "IPv4 only");
        assert_eq!(device_ipv4("192.168.1.73").unwrap(), Ipv4Addr::new(192, 168, 1, 73));
    }

    #[test]
    fn hardware_addresses_compare_as_keys() {
        assert_eq!(address_key(" AA:BB:cc-DD:ee:FF "), "aabbccddeeff");
        assert_eq!(address_key("00:11:22:33:44:55:66:77"), "0011223344556677");
        assert_eq!(address_key(":-"), "");
    }

    #[test]
    fn unreachable_says_nothing_about_the_network() {
        let error = unreachable("TV", "connection refused (os error 61) at 192.168.1.9:22");
        assert_eq!(error.to_string(), "TV unreachable");
    }

    /// A device that redirects is not followed: the 3xx comes back as the answer.
    #[tokio::test]
    async fn device_clients_never_follow_a_redirect() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 1024];
            let _ = socket.read(&mut buffer).await;
            let _ = socket
                .write_all(b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/\r\nContent-Length: 0\r\n\r\n")
                .await;
        });
        let client = device_client(Duration::from_secs(2)).unwrap();
        let response = client.get(format!("http://{address}/")).send().await.unwrap();
        assert_eq!(response.status(), 302);
    }

    #[tokio::test]
    async fn an_answer_past_the_cap_is_refused() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 1024];
            let _ = socket.read(&mut buffer).await;
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n0123456789").await;
        });
        let client = device_client(Duration::from_secs(2)).unwrap();
        let response = client.get(format!("http://{address}/")).send().await.unwrap();
        let error = read_capped(response, 4, "Plug").await.unwrap_err();
        assert_eq!(error.to_string(), "Plug answered too much");
    }
}
