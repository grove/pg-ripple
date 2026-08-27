//! HTTP middleware composition for pg_ripple_http (HTTP-03, v0.91.0).
//!
//! Extracts CORS, rate-limiting (governor), and tracing middleware from
//! `main.rs` to a dedicated module so that `build_router()` and `main()` stay
//! focused on their own concerns.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::extract::ConnectInfo;
use axum::http::{HeaderValue, Request};
use tower::{Layer, Service};
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::cors::{AllowOrigin, CorsLayer};

/// A trusted IPv4 or IPv6 network accepted as a proxy peer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpCidr {
    network: IpAddr,
    prefix: u8,
}

impl IpCidr {
    fn contains(&self, address: IpAddr) -> bool {
        match (self.network, address) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix)
                };
                (u32::from(network) & mask) == (u32::from(address) & mask)
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix)
                };
                (u128::from(network) & mask) == (u128::from(address) & mask)
            }
            _ => false,
        }
    }
}

/// Parse `PG_RIPPLE_HTTP_TRUST_PROXY` as a comma-separated IP/CIDR list.
pub fn parse_trusted_proxy(value: Option<&str>) -> Result<Vec<IpCidr>, String> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value.split(',').map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return Err(
                    "invalid PG_RIPPLE_HTTP_TRUST_PROXY entry: empty CIDR after comma"
                        .to_owned(),
                );
            }
            let (address, prefix) = entry.split_once('/').map_or((entry, None), |(ip, prefix)| {
                (ip, Some(prefix))
            });
            let address = address.parse::<IpAddr>().map_err(|error| {
                format!(
                    "invalid PG_RIPPLE_HTTP_TRUST_PROXY entry '{entry}': invalid IP address ({error})"
                )
            })?;
            let max_prefix = if address.is_ipv4() { 32 } else { 128 };
            let prefix = prefix
                .map(|prefix| {
                    prefix.parse::<u8>().map_err(|error| {
                        format!(
                            "invalid PG_RIPPLE_HTTP_TRUST_PROXY entry '{entry}': invalid prefix ({error})"
                        )
                    })
                })
                .transpose()?
                .unwrap_or(max_prefix);
            if prefix > max_prefix {
                return Err(format!(
                    "invalid PG_RIPPLE_HTTP_TRUST_PROXY entry '{entry}': prefix must be between 0 and {max_prefix}"
                ));
            }
            Ok(IpCidr { network: address, prefix })
        })
        .collect()
}

/// Rewrites the client address from X-Forwarded-For only for trusted peers.
#[derive(Clone, Debug)]
pub struct TrustedProxyLayer {
    networks: Arc<[IpCidr]>,
}

impl TrustedProxyLayer {
    pub fn new(networks: Vec<IpCidr>) -> Self {
        Self {
            networks: networks.into(),
        }
    }
}

impl<S> Layer<S> for TrustedProxyLayer {
    type Service = TrustedProxyService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TrustedProxyService {
            inner,
            networks: self.networks.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TrustedProxyService<S> {
    inner: S,
    networks: Arc<[IpCidr]>,
}

impl<S, B> Service<Request<B>> for TrustedProxyService<S>
where
    S: Service<Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<B>) -> Self::Future {
        let direct_peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| *address);
        if let Some(direct_peer) = direct_peer
            && self
                .networks
                .iter()
                .any(|network| network.contains(direct_peer.ip()))
            && let Some(client_ip) = forwarded_client_ip(&request)
        {
            let client_address = SocketAddr::new(client_ip, 0);
            request.extensions_mut().insert(ConnectInfo(client_address));
            request.extensions_mut().insert(client_address);
        }
        self.inner.call(request)
    }
}

fn forwarded_client_ip<B>(request: &Request<B>) -> Option<IpAddr> {
    request
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next()?.trim().parse().ok())
}

/// Apply the standard pg_ripple_http middleware stack to a router.
///
/// Layers applied (outer → inner):
/// 1. Optional per-IP rate-limiting via `tower_governor` (when `rate_limit > 0`).
/// 2. CORS policy from `cors_origins` env-var.
///
/// `build_router()` in `routing/mod.rs` passes the already-constructed CORS layer
/// here so that the permissive-CORS warning can be logged once at startup in
/// `main()` before `apply_middleware()` is called.
///
/// HTTP-06 (v0.92.0): when rate-limiting fires, tower_governor 0.8 with the `axum`
/// feature automatically includes a `Retry-After` header in the 429 response
/// (computed from the wait_time in the GovernorError). No custom response
/// transformer is needed; the header is provided by GovernorError::IntoResponse.
pub fn apply_rate_limit(app: Router, rate_limit: u32) -> Router {
    if rate_limit == 0 {
        return app;
    }
    let governor_conf = match GovernorConfigBuilder::default()
        .per_second(rate_limit as u64)
        .burst_size(rate_limit)
        .finish()
    {
        Some(c) => c,
        None => {
            tracing::error!("invalid governor rate-limit configuration");
            std::process::exit(1);
        }
    };
    app.layer(GovernorLayer::new(Arc::new(governor_conf)))
}

/// Build the CORS layer from a comma-separated list of allowed origin strings.
///
/// - `"*"` — wildcard (permissive); logs a warning. Returns `CorsLayer::permissive()`.
/// - `""` — empty string; no cross-origin access. Returns `CorsLayer::new()`.
/// - `"https://a.example,https://b.example"` — explicit allowlist.
pub fn build_cors_layer(cors_origins: &str) -> CorsLayer {
    if cors_origins == "*" {
        tracing::warn!(
            "CORS is permissive (*). Set PG_RIPPLE_HTTP_CORS_ORIGINS to a comma-separated list \
             of allowed origins for production use."
        );
        CorsLayer::permissive()
    } else if cors_origins.is_empty() {
        CorsLayer::new()
    } else {
        let origins: Vec<HeaderValue> = cors_origins
            .split(',')
            .filter_map(|o| o.trim().parse().ok())
            .collect();
        CorsLayer::new().allow_origin(AllowOrigin::list(origins))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Response;
    use std::convert::Infallible;
    use tower::{ServiceExt, service_fn};

    fn request(peer: SocketAddr, forwarded_for: &str) -> Request<()> {
        let mut request = Request::new(());
        request.extensions_mut().insert(ConnectInfo(peer));
        request.extensions_mut().insert(peer);
        request
            .headers_mut()
            .insert("x-forwarded-for", forwarded_for.parse().unwrap());
        request
    }

    async fn observed_addresses(request: Request<()>) -> Response<()> {
        let connect_info = *request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .unwrap();
        let socket_addr = *request.extensions().get::<SocketAddr>().unwrap();
        let mut response = Response::new(());
        response.extensions_mut().insert(connect_info);
        response.extensions_mut().insert(socket_addr);
        response
    }

    #[tokio::test]
    async fn trusted_proxy_rewrites_forwarded_client() {
        let networks = parse_trusted_proxy(Some("10.0.0.0/8")).unwrap();
        let service = TrustedProxyLayer::new(networks).layer(service_fn(|request| async move {
            Ok::<_, Infallible>(observed_addresses(request).await)
        }));
        let response = service
            .oneshot(request(
                "10.1.2.3:8443".parse().unwrap(),
                "203.0.113.8, 10.1.2.3",
            ))
            .await
            .unwrap();

        let expected = SocketAddr::new("203.0.113.8".parse().unwrap(), 0);
        assert_eq!(
            response
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0),
            Some(expected)
        );
        assert_eq!(response.extensions().get::<SocketAddr>(), Some(&expected));
    }

    #[tokio::test]
    async fn untrusted_peer_ignores_forwarded_client() {
        let networks = parse_trusted_proxy(Some("10.0.0.0/8")).unwrap();
        let service = TrustedProxyLayer::new(networks).layer(service_fn(|request| async move {
            Ok::<_, Infallible>(observed_addresses(request).await)
        }));
        let response = service
            .oneshot(request("192.0.2.3:8443".parse().unwrap(), "203.0.113.8"))
            .await
            .unwrap();

        let expected = "192.0.2.3:8443".parse::<SocketAddr>().unwrap();
        assert_eq!(
            response
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0),
            Some(expected)
        );
        assert_eq!(response.extensions().get::<SocketAddr>(), Some(&expected));
    }

    #[test]
    fn invalid_cidr_is_rejected() {
        let error = parse_trusted_proxy(Some("10.0.0.0/33")).unwrap_err();
        assert!(error.contains("PG_RIPPLE_HTTP_TRUST_PROXY"));
        assert!(error.contains("prefix must be between 0 and 32"));
    }
}
