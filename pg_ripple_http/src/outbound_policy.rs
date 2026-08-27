//! Shared outbound policy for HTTP companion integrations.
//!
//! The safest default is public HTTPS/HTTP only, no redirects, bounded
//! response bodies, and DNS validation before the socket is opened.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::Url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// Fetch a bounded text response after applying the outbound policy.
pub async fn fetch_text(raw_url: &str) -> Result<String, String> {
    let url = validate_url(raw_url)?;
    let host = url
        .host_str()
        .ok_or_else(|| "outbound URL has no host".to_owned())?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("outbound DNS lookup failed: {error}"))?;
    let addresses: Vec<IpAddr> = addresses.map(|address| address.ip()).collect();
    if addresses.is_empty() {
        return Err("outbound DNS lookup returned no addresses".to_owned());
    }
    if let Some(blocked) = addresses.iter().find(|ip| is_blocked(**ip)) {
        return Err(format!(
            "outbound address is blocked by network policy: {blocked}"
        ));
    }

    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        // Pin the connection to the already-validated address so a DNS
        // rebinding cannot change the destination between validation and I/O.
        .resolve(host, SocketAddr::new(addresses[0], port))
        .build()
        .map_err(|error| format!("could not build outbound client: {error}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("outbound request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("remote returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BODY_BYTES as u64)
    {
        return Err(format!("outbound response exceeds {MAX_BODY_BYTES} bytes"));
    }

    let mut body = Vec::new();
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("outbound response read failed: {error}"))?
    {
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(format!("outbound response exceeds {MAX_BODY_BYTES} bytes"));
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| "outbound response is not UTF-8".to_owned())
}

fn validate_url(raw_url: &str) -> Result<Url, String> {
    let url = Url::parse(raw_url).map_err(|error| format!("invalid outbound URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("outbound URL scheme must be http or https".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("outbound URL must not contain credentials".to_owned());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let allowed_ports =
        std::env::var("PG_RIPPLE_HTTP_OUTBOUND_PORTS").unwrap_or_else(|_| "80,443".to_owned());
    if !allowed_ports
        .split(',')
        .filter_map(|value| value.trim().parse::<u16>().ok())
        .any(|allowed| allowed == port)
    {
        return Err(format!("outbound port {port} is not allowed"));
    }
    if let Ok(allowlist) = std::env::var("PG_RIPPLE_HTTP_OUTBOUND_ALLOWLIST") {
        let mut allowlist = allowlist
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty());
        if !allowlist.clone().any(|entry| entry == url.as_str()) {
            let origin = match url.port() {
                Some(port) => format!(
                    "{}://{}:{port}",
                    url.scheme(),
                    url.host_str().unwrap_or_default()
                ),
                None => format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default()),
            };
            if !allowlist.any(|entry| entry.trim_end_matches('/') == origin) {
                return Err("outbound URL is not in PG_RIPPLE_HTTP_OUTBOUND_ALLOWLIST".to_owned());
            }
        }
    }
    Ok(url)
}

fn is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (100..=127).contains(&ip.octets()[0])
                    && ip.octets()[0] == 100
                    && (64..=127).contains(&ip.octets()[1])
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || is_ipv6_link_local(ip)
                || is_ipv6_unique_local(ip)
                || ip
                    .to_ipv4()
                    .is_some_and(|mapped| is_blocked(IpAddr::V4(mapped)))
        }
    }
}

fn is_ipv6_link_local(ip: Ipv6Addr) -> bool {
    ip.segments()[0] & 0xffc0 == 0xfe80
}

fn is_ipv6_unique_local(ip: Ipv6Addr) -> bool {
    ip.segments()[0] & 0xfe00 == 0xfc00
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn blocks_private_and_metadata_ranges() {
        for ip in [
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
            IpAddr::V6("fc00::1".parse().unwrap()),
        ] {
            assert!(is_blocked(ip));
        }
        assert!(!is_blocked(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("https://example.test:8443").is_err());
    }
}
