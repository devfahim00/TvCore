//! Pluggable HTTP layer.
//!
//! TvCore talks to the network only through the [`HttpFetcher`] trait:
//!
//! * [`UreqFetcher`] — the default blocking client (ureq + rustls).
//! * your own implementation — inject it with [`crate::TvCore::with_http`]
//!   to route requests through the platform's network stack (OkHttp /
//!   cronet / Dart HTTP), a proxy, or a mock in tests.

use crate::error::{TvError, TvResult};
use std::time::Duration;

/// A fetched text response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Headers passed to every fetch call, `(name, value)` pairs.
pub type Headers = Vec<(String, String)>;

/// Anything that can fetch a URL.
pub trait HttpFetcher: Send + Sync {
    /// GET a URL and return the body (any 2xx/3xx status is OK).
    fn get_text(&self, url: &str, headers: &Headers) -> TvResult<HttpResponse>;

    /// Cheap reachability probe for stream URLs.
    /// Returns the HTTP status code, or an error for transport failures.
    /// Implementations should prefer `HEAD` and fall back to
    /// `GET` with `Range: bytes=0-1`.
    fn probe(&self, url: &str, headers: &Headers) -> TvResult<u16>;
}

/// Blocking HTTP client based on `ureq` (TLS via rustls, no native deps —
/// cross-compiles cleanly to Android/iOS/desktop).
#[derive(Debug, Clone)]
pub struct UreqFetcher {
    agent: ureq::Agent,
    default_headers: Headers,
}

impl UreqFetcher {
    /// A fetcher with a browser User-Agent (what itcnbd and most CDNs want)
    /// and a 30s timeout.
    pub fn new() -> Self {
        Self::with_timeout_and_headers(Duration::from_secs(30), default_headers())
    }

    /// Fully configurable constructor.
    pub fn with_timeout_and_headers(timeout: Duration, default_headers: Headers) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout(timeout)
            .redirects(5)
            .build();
        UreqFetcher {
            agent,
            default_headers,
        }
    }

    fn merged(&self, headers: &Headers) -> Headers {
        // caller headers win over defaults
        let mut out = self.default_headers.clone();
        for (k, v) in headers {
            if let Some(pos) = out.iter().position(|(ek, _)| ek.eq_ignore_ascii_case(k)) {
                out[pos].1 = v.clone();
            } else {
                out.push((k.clone(), v.clone()));
            }
        }
        out
    }
}

impl Default for UreqFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher for UreqFetcher {
    fn get_text(&self, url: &str, headers: &Headers) -> TvResult<HttpResponse> {
        let mut req = self.agent.get(url);
        for (k, v) in self.merged(headers) {
            req = req.set(&k, &v);
        }
        match req.call() {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.into_string().map_err(|e| TvError::Io(e.to_string()))?;
                Ok(HttpResponse { status, body })
            }
            Err(ureq::Error::Status(code, _)) => Err(TvError::HttpStatus(code)),
            Err(ureq::Error::Transport(t)) => Err(TvError::Http(t.to_string())),
        }
    }

    fn probe(&self, url: &str, headers: &Headers) -> TvResult<u16> {
        let merged = self.merged(headers);
        // 1) HEAD
        let mut req = self.agent.head(url);
        for (k, v) in &merged {
            req = req.set(k, v);
        }
        match req.call() {
            Ok(resp) => return Ok(resp.status()),
            Err(ureq::Error::Status(405, _)) | Err(ureq::Error::Status(400, _)) => {}
            Err(ureq::Error::Status(code, _)) => return Err(TvError::HttpStatus(code)),
            Err(ureq::Error::Transport(t)) => return Err(TvError::Http(t.to_string())),
        }
        // 2) ranged GET
        let mut req = self.agent.get(url).set("Range", "bytes=0-1");
        for (k, v) in &merged {
            if !k.eq_ignore_ascii_case("Range") {
                req = req.set(k, v);
            }
        }
        match req.call() {
            Ok(resp) => Ok(resp.status()),
            Err(ureq::Error::Status(code, _)) => Err(TvError::HttpStatus(code)),
            Err(ureq::Error::Transport(t)) => Err(TvError::Http(t.to_string())),
        }
    }
}

/// Browser-like default headers (itcnbd returns 403 for bare clients).
pub fn default_headers() -> Headers {
    vec![
        (
            "User-Agent".to_string(),
            crate::DEFAULT_USER_AGENT.to_string(),
        ),
        ("Accept".to_string(), "*/*".to_string()),
        (
            "Accept-Language".to_string(),
            "en-US,en;q=0.9,bn;q=0.8".to_string(),
        ),
    ]
}
