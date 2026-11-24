//! Core data model: [`Channel`], [`StreamTarget`] and helpers.

use serde::Serialize;

/// One playable TV channel.
///
/// A channel always has a primary stream `url` and may carry
/// `alternatives` (mirror streams merged from other sources),
/// plus HTTP headers that some CDNs require (User-Agent / Referer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    /// Stable identifier (from `tvg-id`, or a slug of the name).
    pub id: String,
    /// Human readable display name, e.g. "Somoy TV".
    pub name: String,
    /// Category / group, e.g. "BANGLA", "SPORTS".
    pub category: String,
    /// Optional channel logo URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    /// Primary stream URL (HLS `.m3u8`, DASH `.mpd`, or raw `.ts`).
    pub url: String,
    /// Additional mirror stream URLs for the same channel.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub alternatives: Vec<String>,
    /// Source that produced this channel ("itcnbd", "iptv-org", "local", ...).
    pub source: String,
    /// Custom User-Agent required by the stream (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Custom Referer required by the stream (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    /// Extra HTTP headers required by the stream (e.g. cookies).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extra_headers: Vec<(String, String)>,
}

impl Channel {
    /// Build a channel with only the essential fields; optional
    /// fields start empty.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        category: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Channel {
            id: id.into(),
            name: name.into(),
            category: category.into(),
            logo: None,
            url: url.into(),
            alternatives: Vec::new(),
            source: String::new(),
            user_agent: None,
            referer: None,
            extra_headers: Vec::new(),
        }
    }

    /// All known stream URLs for this channel (primary + alternatives).
    pub fn all_urls(&self) -> Vec<&str> {
        let mut v = vec![self.url.as_str()];
        v.extend(self.alternatives.iter().map(|s| s.as_str()));
        v
    }

    /// Turn a display name into a stable, URL-safe slug used as id when the
    /// playlist does not provide a `tvg-id`.
    pub fn slugify(name: &str) -> String {
        let mut out = String::with_capacity(name.len());
        let mut prev_dash = false;
        for ch in name.trim().chars() {
            if ch.is_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
        }
        while out.ends_with('-') {
            out.pop();
        }
        if out.is_empty() {
            out.push_str("channel");
        }
        out
    }
}

/// What kind of stream a URL points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    /// Apple HTTP Live Streaming (`.m3u8`) — play with ExoPlayer / AVPlayer /
    /// media_kit / mpv.
    Hls,
    /// MPEG-DASH (`.mpd`).
    Dash,
    /// Raw MPEG-TS (`.ts`), common on BDIX servers.
    Ts,
    /// Unknown container — hand the URL to a generic player.
    Unknown,
}

impl StreamKind {
    /// Guess the stream kind from a URL (extension based, ignores query).
    pub fn from_url(url: &str) -> StreamKind {
        // strip query/fragment
        let path = url.split(['?', '#']).next().unwrap_or(url);
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".m3u8") {
            StreamKind::Hls
        } else if lower.ends_with(".mpd") {
            StreamKind::Dash
        } else if lower.ends_with(".ts") {
            StreamKind::Ts
        } else {
            StreamKind::Unknown
        }
    }

    /// Lowercase name, handy for JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            StreamKind::Hls => "hls",
            StreamKind::Dash => "dash",
            StreamKind::Ts => "ts",
            StreamKind::Unknown => "unknown",
        }
    }
}

/// Everything a player needs in order to start playback of a channel.
///
/// `headers` must be sent with the request for the `url`
/// (and with requests for HLS segments on the same host).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamTarget {
    /// Channel id this target was resolved from.
    pub channel_id: String,
    /// Channel display name.
    pub channel_name: String,
    /// The URL to hand to a player.
    pub url: String,
    /// Detected stream kind.
    pub kind: StreamKind,
    /// HTTP headers required by the stream.
    pub headers: Vec<(String, String)>,
}

impl StreamTarget {
    /// Build a target from a channel and one of its URLs.
    pub fn from_channel(channel: &Channel, url: &str, default_user_agent: &str) -> Self {
        let mut headers: Vec<(String, String)> = Vec::new();
        let ua = channel
            .user_agent
            .clone()
            .unwrap_or_else(|| default_user_agent.to_string());
        headers.push(("User-Agent".to_string(), ua));
        if let Some(r) = &channel.referer {
            headers.push(("Referer".to_string(), r.clone()));
        }
        for (k, v) in &channel.extra_headers {
            headers.push((k.clone(), v.clone()));
        }
        StreamTarget {
            channel_id: channel.id.clone(),
            channel_name: channel.name.clone(),
            url: url.to_string(),
            kind: StreamKind::from_url(url),
            headers,
        }
    }
}

/// Result of a playback probe (is the stream currently reachable?).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamStatus {
    pub channel_id: String,
    /// HTTP status code of the probe request (0 = transport failure).
    pub http_status: u16,
    /// True when the server answered 2xx.
    pub reachable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(Channel::slugify("Somoy TV"), "somoy-tv");
        assert_eq!(Channel::slugify("T Sports HD!"), "t-sports-hd");
        assert_eq!(Channel::slugify("  -- "), "channel");
    }

    #[test]
    fn stream_kind_detection() {
        assert_eq!(
            StreamKind::from_url("https://s3.itcnbd.live/channel/abc.m3u8"),
            StreamKind::Hls
        );
        assert_eq!(
            StreamKind::from_url("https://x/dash.mpd?tok=1"),
            StreamKind::Dash
        );
        assert_eq!(StreamKind::from_url("http://a/b/mono.ts"), StreamKind::Ts);
        assert_eq!(
            StreamKind::from_url("https://a/live/1011/master"),
            StreamKind::Unknown
        );
    }

    #[test]
    fn channel_serde_camel_case() {
        let mut ch = Channel::new("somoy-tv", "Somoy TV", "BANGLA", "https://x/somoy.m3u8");
        ch.user_agent = Some("UA".into());
        ch.alternatives = vec!["https://x/2.m3u8".into()];
        let json = serde_json::to_string(&ch).unwrap();
        assert!(json.contains("\"userAgent\":\"UA\""));
        assert!(!json.contains("\"user_agent\""));
        assert!(json.contains("\"alternatives\""));
        // optional fields are skipped when empty
        let bare = Channel::new("a", "A", "B", "https://x/a");
        let bare_json = serde_json::to_string(&bare).unwrap();
        assert!(!bare_json.contains("userAgent"));
        assert!(!bare_json.contains("referer"));
    }
}
