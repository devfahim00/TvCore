//! Extended M3U / M3U8 playlist parser.
//!
//! TvCore reads the standard IPTV playlist format used by
//! `itcnbd.live` (`https://s3.itcnbd.live/playlist.m3u`), iptv-org,
//! TiviMate, Kodi and friends:
//!
//! ```text
//! #EXTM3U
//! #EXTINF:-1 group-title="BANGLA" tvg-logo="https://itcnbd.live/images/x.png",Somoy TV
//! https://s3.itcnbd.live/server-6/stream/aW1nL3BsYXkucGhw.m3u8
//! ```
//!
//! Supported extras:
//! * attributes with quoted values (commas and spaces inside quotes)
//! * `tvg-id`, `tvg-name`, `tvg-logo`, `group-title`
//! * `#EXTGRP:<group>` lines (alternate grouping)
//! * `#EXTVLCOPT:http-user-agent=...` / `#EXTVLCOPT:http-referrer=...`
//! * Kodi pipe options: `url|User-Agent=...|Referer=...`
//! * relative URLs (resolved against the playlist base URL)
//! * CRLF and LF line endings, blank lines, stray comments

use crate::error::{TvError, TvResult};
use crate::model::Channel;
use std::collections::HashMap;
use url::Url;

/// Statistics of a parse run (also used by the CLI).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseStats {
    /// Channels successfully parsed.
    pub channels: usize,
    /// `#EXTINF` entries dropped because they had no URL line after them.
    pub skipped: usize,
}

/// Parse an M3U/M3U8 playlist text into channels.
///
/// * `source` — label stored on every channel (e.g. `"itcnbd"`).
/// * `base_url` — playlist URL; used to resolve *relative* stream URLs
///   and logo URLs. Pass `None` for local files.
///
/// Returns the list of channels in playlist order.
pub fn parse_m3u(text: &str, source: &str, base_url: Option<&str>) -> TvResult<Vec<Channel>> {
    parse_m3u_with_stats(text, source, base_url).map(|(c, _)| c)
}

/// Like [`parse_m3u`] but also reports skip statistics.
pub fn parse_m3u_with_stats(
    text: &str,
    source: &str,
    base_url: Option<&str>,
) -> TvResult<(Vec<Channel>, ParseStats)> {
    let base = base_url.and_then(|u| Url::parse(u).ok());

    let has_playlist_marker = text.lines().any(|l| l.trim_start().starts_with("#EXTM3U"));
    let has_extinf = text.lines().any(|l| l.trim_start().starts_with("#EXTINF"));
    if !has_extinf {
        // Be lenient: some playlists omit the #EXTM3U header, but if there is
        // no #EXTINF either, this is not a playlist at all — for example an
        // HTML 403 error page.
        if text.trim().is_empty() {
            return Err(TvError::Parse("playlist is empty".into()));
        }
        let first: String = text.trim_start().chars().take(60).collect();
        if first.starts_with('<') {
            return Err(TvError::Parse(format!(
                "not an M3U playlist (looks like HTML, first bytes: {:?})",
                first
            )));
        }
    }
    let _ = has_playlist_marker;

    let mut channels: Vec<Channel> = Vec::new();
    let mut skipped = 0usize;

    // pending state between an #EXTINF line and its URL line
    struct Pending {
        name: String,
        attrs: HashMap<String, String>,
        group: Option<String>,
        user_agent: Option<String>,
        referer: Option<String>,
    }
    let mut pending: Option<Pending> = None;
    let mut pending_group: Option<String> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            // a second #EXTINF in a row means the previous one had no URL
            if pending.is_some() {
                skipped += 1;
            }
            let (head, name) = split_extinf(rest);
            let attrs = parse_attrs(head);
            pending = Some(Pending {
                name: name.trim().to_string(),
                attrs,
                group: pending_group.take(),
                user_agent: None,
                referer: None,
            });
        } else if let Some(rest) = line.strip_prefix("#EXTGRP:") {
            pending_group = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("#EXTVLCOPT:") {
            let rest = rest.trim();
            if let Some(v) = rest.strip_prefix("http-user-agent=") {
                if let Some(p) = pending.as_mut() {
                    p.user_agent = Some(v.to_string());
                }
            } else if let Some(v) = rest.strip_prefix("http-referrer=") {
                if let Some(p) = pending.as_mut() {
                    p.referer = Some(v.to_string());
                }
            }
            // other VLCOPTs (proxy, etc.) are ignored on purpose
        } else if line.starts_with('#') {
            // any other directive/comment — ignore
            continue;
        } else {
            // URL line
            let (url, opts) = split_pipe_options(line);
            let Some(mut p) = pending.take() else {
                // a URL without a preceding #EXTINF: synthesize a channel
                let resolved = resolve(&url, &base);
                let name = name_from_url(&resolved);
                let mut ch = Channel::new(
                    Channel::slugify(&name),
                    name,
                    pending_group
                        .take()
                        .unwrap_or_else(|| "Uncategorized".into()),
                    resolved,
                );
                apply_pipe_options(&mut ch, opts);
                ch.source = source.to_string();
                channels.push(ch);
                continue;
            };
            pending_group.take();
            if url.is_empty() {
                skipped += 1;
                continue;
            }
            let resolved = resolve(&url, &base);
            let name = if p.name.is_empty() {
                name_from_url(&resolved)
            } else {
                p.name.clone()
            };
            let category = p
                .group
                .clone()
                .or_else(|| p.attrs.get("group-title").cloned())
                .unwrap_or_else(|| "Uncategorized".into());
            let id = p
                .attrs
                .get("tvg-id")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(Channel::slugify)
                .unwrap_or_else(|| Channel::slugify(&name));
            let logo = p.attrs.get("tvg-logo").map(|s| resolve(s.trim(), &base));
            let mut ch = Channel::new(id, name, category, resolved);
            ch.logo = logo;
            if p.user_agent.is_none() {
                p.user_agent = opts.user_agent.clone();
            }
            if p.referer.is_none() {
                p.referer = opts.referer.clone();
            }
            ch.user_agent = p.user_agent.clone();
            ch.referer = p.referer.clone();
            ch.extra_headers = opts.extra_headers.clone();
            ch.source = source.to_string();
            channels.push(ch);
        }
    }
    if pending.is_some() {
        skipped += 1;
    }

    let stats = ParseStats {
        channels: channels.len(),
        skipped,
    };
    Ok((channels, stats))
}

// ---------------------------------------------------------------------------
// internal helpers
// ---------------------------------------------------------------------------

/// Split an `#EXTINF` payload into (attributes head, display name).
/// The display name is everything after the **first** comma that is not
/// inside a quoted attribute value.
fn split_extinf(payload: &str) -> (&str, &str) {
    let mut in_quote: Option<char> = None;
    for (i, ch) in payload.char_indices() {
        match in_quote {
            Some(q) => {
                if ch == q {
                    in_quote = None;
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    in_quote = Some(ch);
                } else if ch == ',' {
                    return (&payload[..i], &payload[i + 1..]);
                }
            }
        }
    }
    (payload, "")
}

/// Parse `key="value"` / `key='value'` / `key=value` attributes
/// out of the fragment between the duration and the display name.
fn parse_attrs(head: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    // drop the duration token (e.g. "-1" or "-1.0")
    let head = match head.find(char::is_whitespace) {
        Some(i) => &head[i..],
        None => "",
    };
    let bytes = head.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // skip whitespace
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // key
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !(bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            // malformed token without '=' — skip to next whitespace
            while i < bytes.len() && !(bytes[i] as char).is_whitespace() {
                i += 1;
            }
            continue;
        }
        let key = head[key_start..i].to_ascii_lowercase();
        i += 1; // consume '='
                // value
        let value: String = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
            let quote = bytes[i];
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let v = head[start..i].to_string();
            if i < bytes.len() {
                i += 1; // closing quote
            }
            v
        } else {
            let start = i;
            while i < bytes.len() && !(bytes[i] as char).is_whitespace() {
                i += 1;
            }
            head[start..i].to_string()
        };
        if !key.is_empty() {
            out.insert(key, value);
        }
    }
    out
}

struct PipeOptions {
    user_agent: Option<String>,
    referer: Option<String>,
    extra_headers: Vec<(String, String)>,
}

/// Split a URL line that may carry Kodi pipe options:
/// `https://host/stream.m3u8|User-Agent=Mozilla/5.0|Referer=https://x/`
fn split_pipe_options(line: &str) -> (String, PipeOptions) {
    let mut parts = line.split('|');
    let url = parts.next().unwrap_or("").trim().to_string();
    let mut opts = PipeOptions {
        user_agent: None,
        referer: None,
        extra_headers: Vec::new(),
    };
    for seg in parts {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        // some writers chain options with '&' inside one segment
        for kv in seg.split('&') {
            let kv = kv.trim();
            if let Some((k, v)) = kv.split_once('=') {
                let k = k.trim().to_ascii_lowercase();
                let v = v.trim();
                if v.is_empty() {
                    continue;
                }
                match k.as_str() {
                    "user-agent" => opts.user_agent = Some(v.to_string()),
                    "referer" | "referrer" => opts.referer = Some(v.to_string()),
                    _ => opts.extra_headers.push((k, v.to_string())),
                }
            }
        }
    }
    (url, opts)
}

fn apply_pipe_options(ch: &mut Channel, opts: PipeOptions) {
    if ch.user_agent.is_none() {
        ch.user_agent = opts.user_agent;
    }
    if ch.referer.is_none() {
        ch.referer = opts.referer;
    }
    ch.extra_headers.extend(opts.extra_headers);
}

/// Resolve a possibly relative URL against the playlist base.
fn resolve(target: &str, base: &Option<Url>) -> String {
    if target.is_empty() || target.starts_with("http://") || target.starts_with("https://") {
        return target.to_string();
    }
    if let Some(b) = base {
        if let Ok(joined) = b.join(target) {
            return joined.to_string();
        }
    }
    target.to_string()
}

/// Best-effort display name for URL-only entries.
fn name_from_url(url: &str) -> String {
    if let Ok(u) = Url::parse(url) {
        let last = u
            .path_segments()
            .and_then(|mut segs| segs.next_back().map(|s| s.to_string()))
            .unwrap_or_default();
        let cleaned = last
            .split('.')
            .next()
            .unwrap_or("")
            .replace(['-', '_'], " ");
        let name = cleaned.trim().to_string();
        if !name.is_empty() {
            return name;
        }
        if let Some(host) = u.host_str() {
            return host.to_string();
        }
    }
    "Unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#EXTM3U\n\
        #EXTINF:-1 group-title=\"BANGLA\" tvg-logo=\"https://itcnbd.live/images/a.png\",Somoy TV\n\
        https://s3.itcnbd.live/server-6/stream/aW1nL3BsYXkucGhwP3N0cmVhbT0yMDU.m3u8\n";

    fn parse(text: &str) -> Vec<Channel> {
        parse_m3u(text, "test", None).unwrap()
    }

    #[test]
    fn parses_basic_playlist() {
        let ch = parse(SAMPLE);
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].name, "Somoy TV");
        assert_eq!(ch[0].category, "BANGLA");
        assert_eq!(ch[0].id, "somoy-tv");
        assert_eq!(
            ch[0].logo.as_deref(),
            Some("https://itcnbd.live/images/a.png")
        );
        assert_eq!(ch[0].source, "test");
    }

    #[test]
    fn quoted_comma_in_attrs() {
        let text = "#EXTM3U\n#EXTINF:-1 group-title=\"A, B\" tvg-name=\"X, Y\",Real Name\nhttp://x/y.m3u8\n";
        let ch = parse(text);
        assert_eq!(ch[0].name, "Real Name");
        assert_eq!(ch[0].category, "A, B");
    }

    #[test]
    fn crlf_and_blanks() {
        let text = "#EXTM3U\r\n\r\n#EXTINF:-1,Chan A\r\nhttp://a/a.m3u8\r\n\r\n#EXTINF:-1 tvg-id=\"b.tv\",Chan B\r\nhttp://b/b.m3u8\r\n";
        let ch = parse(text);
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[1].id, "b-tv");
    }

    #[test]
    fn extgrp_and_vlcopt() {
        let text = "#EXTM3U\n#EXTGRP:SPORTS\n#EXTINF:-1,T Sports HD\n#EXTVLCOPT:http-user-agent=UA-X\n#EXTVLCOPT:http-referrer=https://r/\nhttps://s1.itcnbd.live/T-Sports-HD/tracks-v1a1/playlist.m3u8\n";
        let ch = parse(text);
        assert_eq!(ch[0].category, "SPORTS");
        assert_eq!(ch[0].user_agent.as_deref(), Some("UA-X"));
        assert_eq!(ch[0].referer.as_deref(), Some("https://r/"));
    }

    #[test]
    fn pipe_options() {
        let text = "#EXTM3U\n#EXTINF:-1 group-title=\"NEWS\",NTV\nhttps://tapmad/main.m3u8|user-agent=Mozilla/5.0|Referer=https://tapmad.com/\n";
        let ch = parse(text);
        assert_eq!(ch[0].url, "https://tapmad/main.m3u8");
        assert_eq!(ch[0].referer.as_deref(), Some("https://tapmad.com/"));
        assert_eq!(ch[0].user_agent.as_deref(), Some("Mozilla/5.0"));
    }

    #[test]
    fn pipe_options_ampersand_style() {
        let text = "#EXTM3U\n#EXTINF:-1,X\nhttp://a/b.m3u8|User-Agent=UA&Referer=rr\n";
        let ch = parse(text);
        assert_eq!(ch[0].user_agent.as_deref(), Some("UA"));
        assert_eq!(ch[0].referer.as_deref(), Some("rr"));
        assert_eq!(ch[0].url, "http://a/b.m3u8");
    }

    #[test]
    fn relative_urls_resolved() {
        let text = "#EXTM3U\n#EXTINF:-1,Rel\n/ch/abc.m3u8\n";
        let ch = parse_m3u(text, "t", Some("https://s3.itcnbd.live/playlist.m3u")).unwrap();
        assert_eq!(ch[0].url, "https://s3.itcnbd.live/ch/abc.m3u8");
    }

    #[test]
    fn missing_url_counts_skipped() {
        let text = "#EXTM3U\n#EXTINF:-1,Ghost\n#EXTINF:-1,Real\nhttp://x/y.m3u8\n";
        let (ch, stats) = parse_m3u_with_stats(text, "t", None).unwrap();
        assert_eq!(stats.skipped, 1);
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].name, "Real");
    }

    #[test]
    fn url_without_extinf_is_kept() {
        let text = "#EXTM3U\nhttp://cdn/live_one.ts\n";
        let ch = parse(text);
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].category, "Uncategorized");
        assert_eq!(ch[0].name, "live one");
    }

    #[test]
    fn html_is_rejected() {
        assert!(parse_m3u("<html><body>403</body></html>", "t", None).is_err());
    }

    #[test]
    fn empty_is_rejected() {
        assert!(parse_m3u("   \n\n  ", "t", None).is_err());
    }

    #[test]
    fn headerless_playlist_ok() {
        // some playlists skip #EXTM3U entirely
        let text = "#EXTINF:-1,A\nhttp://a/a.m3u8\n";
        let ch = parse(text);
        assert_eq!(ch.len(), 1);
    }
}
