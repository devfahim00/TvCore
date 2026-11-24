//! Channel providers: where the channel list comes from.
//!
//! * [`ItcnbdProvider`] — the official ITCN BD playlist
//!   (`https://s3.itcnbd.live/playlist.m3u`), the same source the
//!   <https://itcnbd.live> website player uses.
//! * [`M3uUrlProvider`] — any remote M3U/M3U8 playlist URL
//!   (iptv-org, Mrgify-BDIX, your own server, ...).
//! * [`M3uFileProvider`] — a playlist file on local storage.

use crate::error::{TvError, TvResult};
use crate::http::{Headers, HttpFetcher};
use crate::m3u::parse_m3u;
use crate::model::Channel;
use std::path::Path;

/// Official ITCN BD playlist (high quality direct HLS streams).
pub const ITCNBD_PLAYLIST_URL: &str = "https://s3.itcnbd.live/playlist.m3u";

/// Browser User-Agent — itcnbd's nginx rejects non-browser clients
/// with 403, so TvCore always identifies itself this way by default.
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

/// A source of channels.
pub trait Provider {
    /// Short lowercase label, e.g. `"itcnbd"` — stored on each channel.
    fn name(&self) -> &str;

    /// Fetch (or read) the playlist and parse it into channels.
    fn load(&self, http: &dyn HttpFetcher) -> TvResult<Vec<Channel>>;
}

/// The official ITCN BD playlist provider (default source).
#[derive(Debug, Clone)]
pub struct ItcnbdProvider {
    pub url: String,
}

impl ItcnbdProvider {
    /// Points at [`ITCNBD_PLAYLIST_URL`].
    pub fn new() -> Self {
        ItcnbdProvider {
            url: ITCNBD_PLAYLIST_URL.to_string(),
        }
    }

    /// Custom playlist URL (mirror server, `s1.`/`s2.` hosts, ...).
    pub fn with_url(url: impl Into<String>) -> Self {
        ItcnbdProvider { url: url.into() }
    }
}

impl Default for ItcnbdProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for ItcnbdProvider {
    fn name(&self) -> &str {
        "itcnbd"
    }

    fn load(&self, http: &dyn HttpFetcher) -> TvResult<Vec<Channel>> {
        let headers: Headers = vec![
            ("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string()),
            ("Referer".to_string(), "https://itcnbd.live/".to_string()),
        ];
        let resp = http.get_text(&self.url, &headers)?;
        if resp.status >= 400 {
            return Err(TvError::HttpStatus(resp.status));
        }
        parse_m3u(&resp.body, self.name(), Some(&self.url))
    }
}

/// Any remote M3U/M3U8 playlist, fetched over HTTP(S).
#[derive(Debug, Clone)]
pub struct M3uUrlProvider {
    label: String,
    pub url: String,
}

impl M3uUrlProvider {
    /// `label` is stored as the channel `source` (e.g. `"iptv-org"`).
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        let label = host_label(&url);
        M3uUrlProvider { label, url }
    }

    pub fn with_label(label: impl Into<String>, url: impl Into<String>) -> Self {
        M3uUrlProvider {
            label: label.into(),
            url: url.into(),
        }
    }
}

impl Provider for M3uUrlProvider {
    fn name(&self) -> &str {
        &self.label
    }

    fn load(&self, http: &dyn HttpFetcher) -> TvResult<Vec<Channel>> {
        let headers: Headers = vec![("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string())];
        let resp = http.get_text(&self.url, &headers)?;
        if resp.status >= 400 {
            return Err(TvError::HttpStatus(resp.status));
        }
        parse_m3u(&resp.body, self.name(), Some(&self.url))
    }
}

/// A playlist file from local storage (bundled app asset, downloaded cache).
#[derive(Debug, Clone)]
pub struct M3uFileProvider {
    label: String,
    pub path: String,
}

impl M3uFileProvider {
    pub fn new(path: impl Into<String>) -> Self {
        M3uFileProvider {
            label: "local".to_string(),
            path: path.into(),
        }
    }

    pub fn with_label(label: impl Into<String>, path: impl Into<String>) -> Self {
        M3uFileProvider {
            label: label.into(),
            path: path.into(),
        }
    }
}

impl Provider for M3uFileProvider {
    fn name(&self) -> &str {
        &self.label
    }

    fn load(&self, _http: &dyn HttpFetcher) -> TvResult<Vec<Channel>> {
        let path = Path::new(&self.path);
        if !path.exists() {
            return Err(TvError::Io(format!(
                "playlist file not found: {}",
                self.path
            )));
        }
        let text = std::fs::read_to_string(path)?;
        parse_m3u(&text, self.name(), None)
    }
}

fn host_label(url: &str) -> String {
    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        let host = rest.split('/').next().unwrap_or(rest);
        return host.split('.').next().unwrap_or(host).to_string();
    }
    "playlist".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;

    struct MockHttp {
        body: String,
    }
    impl HttpFetcher for MockHttp {
        fn get_text(&self, _url: &str, _h: &Headers) -> TvResult<HttpResponse> {
            Ok(HttpResponse {
                status: 200,
                body: self.body.clone(),
            })
        }
        fn probe(&self, _url: &str, _h: &Headers) -> TvResult<u16> {
            Ok(200)
        }
    }

    #[test]
    fn itcnbd_provider_parses() {
        let http = MockHttp {
            body: "#EXTM3U\n#EXTINF:-1 group-title=\"BANGLA\",Somoy TV\nhttps://s3.itcnbd.live/channel/abc.m3u8\n".into(),
        };
        let p = ItcnbdProvider::new();
        let chans = p.load(&http).unwrap();
        assert_eq!(chans.len(), 1);
        assert_eq!(chans[0].source, "itcnbd");
        assert_eq!(chans[0].category, "BANGLA");
    }

    #[test]
    fn url_provider_label_from_host() {
        let p = M3uUrlProvider::new("https://iptv-org.github.io/iptv/countries/bd.m3u");
        assert_eq!(p.name(), "iptv-org");
    }

    #[test]
    fn file_provider_missing_file() {
        let p = M3uFileProvider::new("/definitely/not/here.m3u");
        let http = MockHttp {
            body: String::new(),
        };
        let err = p.load(&http).unwrap_err();
        assert!(matches!(err, TvError::Io(_)));
    }
}
