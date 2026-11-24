//! # TvCore — live TV channel listing, search & playback core
//!
//! A tiny, dependency-light Rust core that powers TV apps:
//!
//! * **List** channels (the official ITCN BD playlist used by
//!   <https://itcnbd.live>, any remote M3U/M3U8 URL, or a local playlist file)
//! * **Search** channels — case-insensitive, multi-token, typo-tolerant
//! * **Play** channels — resolves the HLS/DASH/TS stream URL **plus the
//!   HTTP headers** (User-Agent/Referer) some CDNs require
//!
//! Designed from day one for embedding through FFI into
//! [Flutter](https://flutter.dev) (dart:ffi), Android/Kotlin (JNI),
//! iOS/Swift, or any C/C++ host — see the `tvcore-ffi` crate and the
//! `docs/` folder for ready-made bindings.
//!
//! ```no_run
//! use tvcore::TvCore;
//!
//! let mut tv = TvCore::new();
//! // default source: official ITCN BD playlist
//! tv.load_itcnbd().expect("playlist fetched");
//!
//! for ch in tv.search("sports", 20) {
//!     println!("{} [{}]", ch.name, ch.category);
//! }
//!
//! // everything a player needs:
//! let target = tv.play("star-sports-1").unwrap();
//! println!("url: {}", target.url);
//! println!("kind: {}", target.kind.as_str());
//! ```
//!
//! Threading: `TvCore` is `Send + Sync`. The built-in HTTP client is
//! blocking — call it from a background thread in UI apps
//! (the FFI layer documents the same rule for each platform).

#![forbid(unsafe_code)]

pub mod catalog;
pub mod error;
pub mod http;
pub mod m3u;
pub mod model;
pub mod provider;
pub mod search;

pub use catalog::{AddStats, Catalog};
pub use error::{TvError, TvResult};
pub use http::{Headers, HttpFetcher, HttpResponse, UreqFetcher};
pub use m3u::{parse_m3u, ParseStats};
pub use model::{Channel, StreamKind, StreamStatus, StreamTarget};
pub use provider::{ItcnbdProvider, M3uFileProvider, M3uUrlProvider, Provider};
pub use search::SearchResult;

pub use provider::{DEFAULT_USER_AGENT, ITCNBD_PLAYLIST_URL};

/// Crate version string (used by the FFI layer).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use std::path::Path;

/// The facade that UI layers talk to (directly from Rust or through FFI).
/// Not `Clone` on purpose: the HTTP layer may be shared state.
#[derive(Clone)]
pub struct TvCore {
    http: std::sync::Arc<dyn HttpFetcher>,
    catalog: Catalog,
}

impl Default for TvCore {
    fn default() -> Self {
        Self::new()
    }
}

impl TvCore {
    /// Create with the default blocking HTTP client (ureq, 30s timeout,
    /// browser headers).
    pub fn new() -> Self {
        TvCore {
            http: std::sync::Arc::new(UreqFetcher::new()),
            catalog: Catalog::new(),
        }
    }

    /// Create with a custom HTTP layer — useful for routing requests
    /// through the host platform's network stack or for tests.
    pub fn with_http(http: std::sync::Arc<dyn HttpFetcher>) -> Self {
        TvCore {
            http,
            catalog: Catalog::new(),
        }
    }

    // ------------------------------------------------------------------
    // loading
    // ------------------------------------------------------------------

    /// Load the official ITCN BD playlist (`s3.itcnbd.live/playlist.m3u`).
    /// Returns the number of **newly added** channels.
    pub fn load_itcnbd(&mut self) -> TvResult<usize> {
        self.load_provider(&ItcnbdProvider::new())
    }

    /// Load the official ITCN BD playlist from a mirror URL.
    pub fn load_itcnbd_from(&mut self, url: &str) -> TvResult<usize> {
        self.load_provider(&ItcnbdProvider::with_url(url))
    }

    /// Load any remote M3U/M3U8 playlist URL.
    pub fn load_playlist_url(&mut self, url: &str) -> TvResult<usize> {
        self.load_provider(&M3uUrlProvider::new(url))
    }

    /// Load a playlist file from local storage.
    pub fn load_playlist_file(&mut self, path: &str) -> TvResult<usize> {
        let p = Path::new(path);
        if !p.exists() {
            return Err(TvError::Io(format!("playlist file not found: {}", path)));
        }
        self.load_provider(&M3uFileProvider::new(path))
    }

    /// Parse an in-memory M3U/M3U8 playlist (tests, offline bundles, text
    /// you got from your own backend).
    pub fn load_playlist_str(&mut self, m3u_text: &str, source: &str) -> usize {
        match parse_m3u(m3u_text, source, None) {
            Ok(chans) => {
                let n = chans.len();
                self.catalog.add_channels(chans);
                n
            }
            Err(_) => 0,
        }
    }

    /// Load channels from any [`Provider`] (including your own trait impls).
    pub fn load_provider(&mut self, provider: &dyn Provider) -> TvResult<usize> {
        let chans = provider.load(self.http.as_ref())?;
        let stats = self.catalog.add_channels(chans);
        Ok(stats.added)
    }

    /// Replace the whole catalog (channels must have unique ids).
    pub fn set_channels(&mut self, channels: Vec<Channel>) -> AddStats {
        self.catalog.add_channels(channels)
    }

    // ------------------------------------------------------------------
    // listing
    // ------------------------------------------------------------------

    /// All channels.
    pub fn channels(&self) -> &[Channel] {
        self.catalog.channels()
    }

    /// Number of channels currently loaded.
    pub fn channel_count(&self) -> usize {
        self.catalog.len()
    }

    /// Category list with channel counts (sorted by count desc).
    pub fn categories(&self) -> Vec<(String, usize)> {
        self.catalog.categories()
    }

    /// Channels of one category (case-insensitive).
    pub fn channels_in_category(&self, category: &str) -> Vec<&Channel> {
        self.catalog.channels_in_category(category)
    }

    /// Find a channel by id.
    pub fn find(&self, id: &str) -> Option<&Channel> {
        self.catalog.find(id)
    }

    /// First channel whose id **or** name matches `key` (name match is
    /// case-insensitive; falls back to the best search hit).
    pub fn find_any(&self, key: &str) -> Option<&Channel> {
        if key.trim().is_empty() {
            return None;
        }
        if let Some(ch) = self.catalog.find(&key.trim().to_lowercase()) {
            return Some(ch);
        }
        let lower = key.trim().to_lowercase();
        if let Some(ch) = self
            .catalog
            .channels()
            .iter()
            .find(|c| c.name.trim().to_lowercase() == lower)
        {
            return Some(ch);
        }
        // best search hit
        let hits = self.catalog.search(key, 1);
        hits.first().map(|h| &self.catalog.channels()[h.index])
    }

    // ------------------------------------------------------------------
    // search
    // ------------------------------------------------------------------

    /// Ranked search; returns matching channels (clones) best-first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Channel> {
        self.catalog
            .search(query, limit)
            .iter()
            .map(|r| self.catalog.channels()[r.index].clone())
            .collect()
    }

    // ------------------------------------------------------------------
    // play
    // ------------------------------------------------------------------

    /// Resolve everything a player needs to start playback of the channel
    /// with this id.
    ///
    /// If the channel has alternative streams, the primary URL is returned;
    /// try [`Self::play_alternative`] for mirrors.
    pub fn play(&self, id: &str) -> TvResult<StreamTarget> {
        let ch = self
            .catalog
            .find(id)
            .ok_or_else(|| TvError::NotFound(format!("no channel with id {:?}", id)))?;
        Ok(StreamTarget::from_channel(ch, &ch.url, DEFAULT_USER_AGENT))
    }

    /// Like [`Self::play`] but resolves by id **or** name **or** search hit —
    /// convenient for command lines and voice input.
    pub fn play_any(&self, key: &str) -> TvResult<StreamTarget> {
        let ch = self
            .find_any(key)
            .ok_or_else(|| TvError::NotFound(format!("no channel matches {:?}", key)))?;
        Ok(StreamTarget::from_channel(ch, &ch.url, DEFAULT_USER_AGENT))
    }

    /// Resolve alternative stream `n` (0-based) of a channel.
    pub fn play_alternative(&self, id: &str, n: usize) -> TvResult<StreamTarget> {
        let ch = self
            .catalog
            .find(id)
            .ok_or_else(|| TvError::NotFound(format!("no channel with id {:?}", id)))?;
        let url = ch.alternatives.get(n).ok_or_else(|| {
            TvError::NotFound(format!("channel {} has no alternative #{}", id, n))
        })?;
        Ok(StreamTarget::from_channel(ch, url, DEFAULT_USER_AGENT))
    }

    /// Check whether the channel's stream currently answers.
    /// Never panics on network failure — `http_status` is 0 then.
    pub fn probe(&self, id: &str) -> TvResult<StreamStatus> {
        let ch = self
            .catalog
            .find(id)
            .ok_or_else(|| TvError::NotFound(format!("no channel with id {:?}", id)))?;
        let target = StreamTarget::from_channel(ch, &ch.url, DEFAULT_USER_AGENT);
        let status = self.http.probe(&target.url, &target.headers).unwrap_or(0);
        Ok(StreamStatus {
            channel_id: ch.id.clone(),
            http_status: status,
            reachable: (200..300).contains(&status),
        })
    }

    /// The HTTP layer in use (for advanced hosts that want to reuse it).
    pub fn http(&self) -> &dyn HttpFetcher {
        self.http.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYLIST: &str = "#EXTM3U\n\
        #EXTINF:-1 group-title=\"BANGLA\" tvg-logo=\"https://itcnbd.live/images/a.png\",Somoy TV\n\
        https://s3.itcnbd.live/server-6/stream/aW1nL3BsYXk.m3u8\n\
        #EXTINF:-1 group-title=\"SPORTS\",T Sports HD\n\
        https://s1.itcnbd.live/T-Sports-HD/tracks-v1a1/playlist.m3u8\n\
        #EXTINF:-1 group-title=\"SPORTS\",Star Sports 1 HD\n\
        https://s1.itcnbd.live/Star.Sports.1.HD/tracks-v1a1/playlist.m3u8\n";

    #[test]
    fn facade_flow() {
        let mut tv = TvCore::new();
        assert_eq!(tv.load_playlist_str(PLAYLIST, "itcnbd"), 3);
        assert_eq!(tv.channel_count(), 3);
        assert_eq!(tv.categories().len(), 2);

        let found = tv.find_any("t sports");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "t-sports-hd");

        let target = tv.play("somoy-tv").unwrap();
        assert_eq!(target.kind, StreamKind::Hls);
        assert_eq!(target.headers[0].0, "User-Agent");

        let miss = tv.play("nope");
        assert!(matches!(miss, Err(TvError::NotFound(_))));
    }

    #[test]
    fn search_via_facade() {
        let mut tv = TvCore::new();
        tv.load_playlist_str(PLAYLIST, "x");
        let out = tv.search("spor", 10);
        assert_eq!(out.len(), 2);
    }
}
