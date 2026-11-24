use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One channel as listed on the site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    /// Stable slug derived from the title (`"Star Sports 1 HD"` -> `"star-sports-1-hd"`).
    /// Duplicate titles get `-2`, `-3`, ... in page order.
    pub id: String,
    pub title: String,
    /// Upper case category as shown on the site (`BANGLA`, `SPORTS`, ...). May be empty.
    pub category: String,
    /// Absolute logo URL.
    pub logo: Option<String>,
    /// Stream URL exactly as found on the page. It is usually token signed and short lived.
    pub stream_url: String,
    /// Unix timestamp (seconds) at which the signed token stops working, if the URL has one.
    pub expires_at: Option<i64>,
}

impl Channel {
    /// True when the token is expired or will be within `skew_secs`.
    pub fn is_expired_at(&self, now: i64, skew_secs: i64) -> bool {
        matches!(self.expires_at, Some(exp) if exp - skew_secs <= now)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchQuery {
    pub query: String,
    /// Restrict to one category (case-insensitive). `None` or empty = all.
    pub category: Option<String>,
    /// Max results. `None` = unlimited.
    pub limit: Option<usize>,
}

/// Everything a player needs to start a channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayInfo {
    pub id: String,
    pub title: String,
    pub category: String,
    pub url: String,
    /// Request headers the player should send (Referer, User-Agent).
    pub headers: BTreeMap<String, String>,
    pub expires_at: Option<i64>,
    /// True when the page had to be re-fetched to get a fresh token.
    pub refreshed: bool,
    /// True when the token is already expired (only possible without networking).
    pub expired: bool,
    /// Always `"hls"` for this site.
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayOptions {
    /// Fetch the playlist first and make sure it is reachable (retries with a fresh token on 401/403).
    pub verify: bool,
    /// Always re-fetch the channel page before returning.
    pub force_refresh: bool,
}

/// Result of probing an HLS playlist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub status: u16,
    pub ok: bool,
    /// `master`, `media` or `invalid`.
    pub kind: String,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    pub url: String,
    pub bandwidth: Option<u64>,
    pub resolution: Option<String>,
}
