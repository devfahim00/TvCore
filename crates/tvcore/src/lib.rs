//! # tvcore
//!
//! Rust core for <https://itcnbd.live/>: **list**, **search** and **play** channels.
//!
//! ```no_run
//! use tvcore::{Config, PlayOptions, SearchQuery, TvCore};
//!
//! let core = TvCore::new(Config::default());
//! core.refresh().unwrap();                                   // fetch + parse the home page
//! let hits = core.search(&SearchQuery { query: "jamuna".into(), ..Default::default() });
//! let play = core.play(&hits[0].id, &PlayOptions::default()).unwrap();
//! println!("{} -> {}", play.title, play.url);                // hand `play.url` + `play.headers` to a player
//! ```
//!
//! Without the `http` feature the crate does no networking: feed the page HTML into
//! [`TvCore::load_html`] yourself.

mod error;
mod model;
mod net;
pub mod parse;
pub mod playlist;
pub mod search;
pub mod token;

pub use error::{Error, Result};
pub use model::*;

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Client configuration. Every field has a default, so JSON like `{}` is valid.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    /// Site root. Default `https://itcnbd.live`.
    pub base_url: String,
    pub user_agent: String,
    pub timeout_secs: u64,
    /// A token expiring within this many seconds counts as expired.
    pub token_skew_secs: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "https://itcnbd.live/".into(),
            user_agent: format!(
                "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) TvCore/{}",
                env!("CARGO_PKG_VERSION")
            ),
            timeout_secs: 15,
            token_skew_secs: 120,
        }
    }
}

#[derive(Default)]
struct State {
    channels: Vec<Channel>,
}

/// Thread-safe handle. Cheap to share behind an `Arc`.
pub struct TvCore {
    pub(crate) cfg: Config,
    state: Mutex<State>,
    #[cfg(feature = "http")]
    pub(crate) agent: ureq::Agent,
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl TvCore {
    pub fn new(cfg: Config) -> Self {
        #[cfg(feature = "http")]
        let agent = net::build_agent(&cfg);
        Self {
            cfg,
            state: Mutex::new(State::default()),
            #[cfg(feature = "http")]
            agent,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Replace the channel list by parsing page HTML. Returns the channel count.
    pub fn load_html(&self, html: &str) -> Result<usize> {
        let list = parse::parse_channels(html, &self.cfg.base_url)?;
        let n = list.len();
        self.lock().channels = list;
        Ok(n)
    }

    /// All channels in page order. Fetches the page first if nothing is loaded yet.
    pub fn channels(&self) -> Result<Vec<Channel>> {
        self.ensure_loaded()?;
        Ok(self.lock().channels.clone())
    }

    /// Categories with channel counts, in order of first appearance.
    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut out: Vec<Category> = Vec::new();
        for c in self.channels()? {
            if c.category.is_empty() {
                continue;
            }
            match out.iter_mut().find(|k| k.name == c.category) {
                Some(k) => k.count += 1,
                None => out.push(Category {
                    name: c.category,
                    count: 1,
                }),
            }
        }
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Result<Channel> {
        self.ensure_loaded()?;
        let id = id.trim().to_lowercase();
        self.lock()
            .channels
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or(Error::NotFound(id))
    }

    pub fn search(&self, q: &SearchQuery) -> Vec<Channel> {
        let _ = self.ensure_loaded();
        search::search(&self.lock().channels, q)
    }

    fn ensure_loaded(&self) -> Result<()> {
        if self.lock().channels.is_empty() {
            #[cfg(feature = "http")]
            {
                self.refresh()?;
            }
        }
        Ok(())
    }

    /// Resolve a channel into something a player can open, using the current clock.
    pub fn play(&self, id: &str, opts: &PlayOptions) -> Result<PlayInfo> {
        self.play_at(id, opts, now_unix())
    }

    /// Same as [`play`](Self::play) with an explicit clock (useful for tests).
    pub fn play_at(&self, id: &str, opts: &PlayOptions, now: i64) -> Result<PlayInfo> {
        let mut ch = self.get(id)?;
        let mut refreshed = false;

        if opts.force_refresh || ch.is_expired_at(now, self.cfg.token_skew_secs) {
            if cfg!(feature = "http") {
                self.refresh()?;
                ch = self.get(id)?;
                refreshed = true;
            }
        }

        if opts.verify {
            let mut probe = self.probe(&ch.stream_url)?;
            if !probe.ok && matches!(probe.status, 401 | 403) {
                // 1) new token via the site's own endpoint
                if let Ok(new_url) = self.refresh_stream_url(&ch.stream_url) {
                    if new_url != ch.stream_url {
                        ch.expires_at = token::expiry_from_url(&new_url);
                        ch.stream_url = new_url;
                        refreshed = true;
                        probe = self.probe(&ch.stream_url)?;
                    }
                }
                // 2) re-read the page
                if !probe.ok && !refreshed {
                    self.refresh()?;
                    ch = self.get(id)?;
                    refreshed = true;
                    probe = self.probe(&ch.stream_url)?;
                }
            }
            if !probe.ok {
                return Err(
                    if probe.status != 0 && !(200..300).contains(&probe.status) {
                        Error::Status(probe.status)
                    } else {
                        Error::Network(format!(
                            "stream is not a valid HLS playlist ({})",
                            probe.kind
                        ))
                    },
                );
            }
        }

        Ok(self.play_info(&ch, refreshed, now))
    }

    fn play_info(&self, ch: &Channel, refreshed: bool, now: i64) -> PlayInfo {
        let mut headers = BTreeMap::new();
        headers.insert("Referer".to_string(), self.cfg.base_url.clone());
        headers.insert("User-Agent".to_string(), self.cfg.user_agent.clone());
        PlayInfo {
            id: ch.id.clone(),
            title: ch.title.clone(),
            category: ch.category.clone(),
            url: ch.stream_url.clone(),
            headers,
            expires_at: ch.expires_at,
            refreshed,
            expired: ch.is_expired_at(now, 0),
            kind: "hls".into(),
        }
    }

    /// Export the current list as an extended M3U playlist (VLC / Kodi / TiviMate / OTT Navigator).
    pub fn export_m3u(&self) -> Result<String> {
        let mut out = String::from("#EXTM3U\n");
        for c in self.channels()? {
            out.push_str(&format!(
                "#EXTINF:-1 tvg-id=\"{}\" tvg-name=\"{}\" tvg-logo=\"{}\" group-title=\"{}\",{}\n",
                c.id,
                c.title.replace('"', "'"),
                c.logo.as_deref().unwrap_or(""),
                c.category.replace('"', "'"),
                c.title.replace(['\n', '\r'], " "),
            ));
            out.push_str(&format!("#EXTVLCOPT:http-referrer={}\n", self.cfg.base_url));
            out.push_str(&format!(
                "#EXTVLCOPT:http-user-agent={}\n",
                self.cfg.user_agent
            ));
            out.push_str(&c.stream_url);
            out.push('\n');
        }
        Ok(out)
    }
}
