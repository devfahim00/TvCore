//! Networking. Compiled only with the `http` feature; otherwise stubs return `Error::NoNetwork`.

use crate::error::{Error, Result};
use crate::model::ProbeResult;
use crate::TvCore;

#[cfg(feature = "http")]
mod imp {
    use super::*;
    use crate::{playlist, Config};
    use std::time::Duration;

    pub fn build_agent(cfg: &Config) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(cfg.timeout_secs.max(1))))
            .http_status_as_error(false)
            .user_agent(cfg.user_agent.clone())
            .build()
            .into()
    }

    impl TvCore {
        fn get_text(&self, url: &str, with_referer: bool) -> Result<(u16, String)> {
            let mut req = self.agent.get(url);
            if with_referer {
                req = req.header("Referer", self.cfg.base_url.as_str());
            }
            let mut resp = req.call().map_err(|e| Error::Network(e.to_string()))?;
            let status = resp.status().as_u16();
            let body = resp
                .body_mut()
                .read_to_string()
                .map_err(|e| Error::Network(e.to_string()))?;
            Ok((status, body))
        }

        /// Fetch the home page and replace the channel list. Returns the channel count.
        pub fn refresh(&self) -> Result<usize> {
            let (status, body) = self.get_text(&self.cfg.base_url, false)?;
            if status != 200 {
                return Err(Error::Status(status));
            }
            self.load_html(&body)
        }

        /// Download a playlist and check that it is valid HLS.
        /// A non-2xx answer is reported in the result (`ok == false`), not as an error.
        pub fn probe(&self, url: &str) -> Result<ProbeResult> {
            let (status, body) = self.get_text(url, true)?;
            Ok(playlist::analyze(&body, url, status))
        }

        /// Ask the site for a freshly signed version of a stream URL (`/api/stream-url`).
        pub fn refresh_stream_url(&self, url: &str) -> Result<String> {
            let mut api = url::Url::parse(&self.cfg.base_url)
                .map_err(|e| Error::Invalid(e.to_string()))?
                .join("/api/stream-url")
                .map_err(|e| Error::Invalid(e.to_string()))?;
            api.query_pairs_mut().append_pair("url", url);
            let (status, body) = self.get_text(api.as_str(), true)?;
            if status != 200 {
                return Err(Error::Status(status));
            }
            let v: serde_json::Value =
                serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;
            v.get("url")
                .and_then(|u| u.as_str())
                .map(str::to_string)
                .ok_or_else(|| Error::Parse("response has no `url` field".into()))
        }
    }
}

#[cfg(feature = "http")]
pub(crate) use imp::build_agent;

#[cfg(not(feature = "http"))]
impl TvCore {
    pub fn refresh(&self) -> Result<usize> {
        Err(Error::NoNetwork)
    }
    pub fn probe(&self, _url: &str) -> Result<ProbeResult> {
        Err(Error::NoNetwork)
    }
    pub fn refresh_stream_url(&self, _url: &str) -> Result<String> {
        Err(Error::NoNetwork)
    }
}
