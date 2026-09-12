use std::{
    collections::HashSet,
    future::Future,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use axum::http::{self, Request, Response};
use dashmap::DashMap;
use tower::{Layer, Service};

pub type BanList = Arc<DashMap<IpAddr, Instant>>;

const CLEANUP_INTERVAL_REQUESTS: usize = 1_024;

#[derive(Debug, Clone)]
pub struct BouncerConfig {
    pub blocked_paths: HashSet<String>,
    pub ban_duration: Duration,
    pub banned_status: http::StatusCode,
    pub blocked_status: http::StatusCode,
    pub log_level: tracing::Level,
    pub trust_proxy: bool,
}

impl BouncerConfig {
    pub fn from_rules(presets: &[&str], custom: &[&str]) -> Self {
        let blocked_paths = crate::bouncer::rules::from_rules(presets, custom);
        Self {
            blocked_paths,
            ban_duration: Duration::from_secs(3600),
            banned_status: http::StatusCode::FORBIDDEN,
            blocked_status: http::StatusCode::FORBIDDEN,
            log_level: tracing::Level::DEBUG,
            trust_proxy: false,
        }
    }

    pub fn from_preset_rules(presets: &[&str]) -> Self {
        Self::from_rules(presets, &[])
    }

    pub fn from_custom_rules(custom: &[&str]) -> Self {
        Self::from_rules(&[], custom)
    }

    pub fn banned_response(mut self, status: http::StatusCode) -> Self {
        self.banned_status = status;
        self
    }

    pub fn blocked_response(mut self, status: http::StatusCode) -> Self {
        self.blocked_status = status;
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.ban_duration = duration;
        self
    }

    pub fn log_level(mut self, level: tracing::Level) -> Self {
        self.log_level = level;
        self
    }

    pub fn trust_proxy(mut self, trust: bool) -> Self {
        self.trust_proxy = trust;
        self
    }
}

// BouncerLayer factory
#[derive(Debug, Clone)]
pub struct BouncerLayer {
    config: BouncerConfig,
    banlist: BanList,
    cleanup_counter: Arc<AtomicUsize>,
}

impl BouncerLayer {
    pub fn new(config: BouncerConfig) -> Self {
        Self {
            config,
            banlist: Arc::new(DashMap::new()),
            cleanup_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Expose banlist for observability
    pub fn banlist(&self) -> Arc<DashMap<IpAddr, Instant>> {
        self.banlist.clone()
    }
}

impl<S> Layer<S> for BouncerLayer {
    type Service = BouncerMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BouncerMiddleware {
            inner,
            config: self.config.clone(),
            banlist: self.banlist.clone(),
            cleanup_counter: self.cleanup_counter.clone(),
        }
    }
}

// A middleware that blocks paths and bans IPs
#[derive(Debug, Clone)]
pub struct BouncerMiddleware<S> {
    inner: S,
    config: BouncerConfig,
    banlist: BanList,
    cleanup_counter: Arc<AtomicUsize>,
}

impl<ReqBody, ResBody, S> Service<Request<ReqBody>> for BouncerMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let config = self.config.clone();
        let banlist = self.banlist.clone();
        let cleanup_counter = self.cleanup_counter.clone();

        let ip = extract_ip(&req, config.trust_proxy);
        let path = req.uri().path().to_owned();

        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            let now = Instant::now();
            if cleanup_counter
                .fetch_add(1, Ordering::Relaxed)
                .is_multiple_of(CLEANUP_INTERVAL_REQUESTS)
            {
                banlist.retain(|_, expiry| now < *expiry);
            }

            if let Some(ip) = ip {
                if let Some(expiry) = banlist.get(&ip).map(|entry| *entry) {
                    if now < expiry {
                        log_event(
                            config.log_level,
                            &ip,
                            &path,
                            "Banned IP attempted access",
                            true,
                            false,
                        );
                        let mut res = Response::default();
                        *res.status_mut() = config.banned_status;
                        return Ok(res);
                    } else {
                        banlist.remove(&ip);
                    }
                }

                if config.blocked_paths.contains(&path) {
                    banlist.insert(ip, Instant::now() + config.ban_duration);
                    log_event(
                        config.log_level,
                        &ip,
                        &path,
                        "Blocked path accessed, IP banned",
                        false,
                        true,
                    );
                    let mut res = Response::default();
                    *res.status_mut() = config.blocked_status;
                    return Ok(res);
                }
            }

            inner.call(req).await
        })
    }
}

fn log_event(
    level: tracing::Level,
    ip: &IpAddr,
    path: &str,
    msg: &str,
    banned: bool,
    blocked: bool,
) {
    match level {
        tracing::Level::ERROR => {
            tracing::error!(ip = %ip, path = %path, banned, blocked, "{msg}")
        }
        tracing::Level::WARN => {
            tracing::warn!(ip = %ip, path = %path, banned, blocked, "{msg}")
        }
        tracing::Level::INFO => {
            tracing::info!(ip = %ip, path = %path, banned, blocked, "{msg}")
        }
        tracing::Level::DEBUG => {
            tracing::debug!(ip = %ip, path = %path, banned, blocked, "{msg}")
        }
        tracing::Level::TRACE => {
            tracing::trace!(ip = %ip, path = %path, banned, blocked, "{msg}")
        }
    }
}

// Extract the real client IP
// Trust proxy must be set to use proxy headers as they can be spoofed
fn extract_ip<B>(req: &Request<B>, trust_proxy: bool) -> Option<IpAddr> {
    // Check proxy headers if configured to trust them, otherwise skip to direct connection IP
    if trust_proxy {
        // 1. Cloudflare
        if let Some(ip) = req
            .headers()
            .get("cf-connecting-ip")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok())
        {
            return Some(ip);
        }
        // 2. X-Forwarded-For (first in list)
        if let Some(ip) = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse().ok())
        {
            return Some(ip);
        }
        // 3. X-Real-IP
        if let Some(ip) = req
            .headers()
            .get("x-real-ip")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok())
        {
            return Some(ip);
        }
    }

    // Fallback to direct connection IP if available
    req.extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip())
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, net::Ipv4Addr};

    use axum::extract::ConnectInfo;
    use tower::service_fn;

    use super::*;

    #[tokio::test]
    async fn expired_ban_is_removed_without_blocking() {
        let layer = BouncerLayer::new(BouncerConfig::from_custom_rules(&[]));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        layer
            .banlist
            .insert(ip, Instant::now() - Duration::from_secs(1));
        let mut service = layer.layer(service_fn(|_| async {
            Ok::<_, Infallible>(Response::<()>::default())
        }));
        let mut request = Request::new(());
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(ip, 8_080)));

        tokio::time::timeout(Duration::from_secs(1), service.call(request))
            .await
            .expect("expired ban lookup should not block")
            .unwrap();

        assert!(!layer.banlist.contains_key(&ip));
    }

    #[tokio::test]
    async fn periodic_cleanup_removes_expired_bans() {
        let layer = BouncerLayer::new(BouncerConfig::from_custom_rules(&[]));
        let expired_ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        layer
            .banlist
            .insert(expired_ip, Instant::now() - Duration::from_secs(1));
        let mut service = layer.layer(service_fn(|_| async {
            Ok::<_, Infallible>(Response::<()>::default())
        }));

        service.call(Request::new(())).await.unwrap();

        assert!(!layer.banlist.contains_key(&expired_ip));
    }

    #[tokio::test]
    async fn proxy_headers_are_used_only_when_trusted() {
        let direct_ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));
        let proxy_ip = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 20));

        for (trust_proxy, expected_ip) in [(false, direct_ip), (true, proxy_ip)] {
            let layer = BouncerLayer::new(
                BouncerConfig::from_custom_rules(&["/blocked"]).trust_proxy(trust_proxy),
            );
            let mut service = layer.layer(service_fn(|_| async {
                Ok::<_, Infallible>(Response::<()>::default())
            }));
            let mut request = Request::builder()
                .uri("/blocked")
                .header("cf-connecting-ip", proxy_ip.to_string())
                .body(())
                .unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo(SocketAddr::new(direct_ip, 8_080)));

            service.call(request).await.unwrap();

            assert!(layer.banlist.contains_key(&expected_ip));
        }
    }
}
