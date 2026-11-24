use crate::model::{ProbeResult, Variant};
use url::Url;

fn attr<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("{key}=");
    let start = line.find(&pat)? + pat.len();
    let rest = &line[start..];
    if let Some(stripped) = rest.strip_prefix('"') {
        stripped.split('"').next()
    } else {
        rest.split(',').next()
    }
}

/// Classifies an HLS playlist body and lists the variants of a master playlist.
/// `status` is passed through into the result.
pub fn analyze(body: &str, base_url: &str, status: u16) -> ProbeResult {
    let ok_status = (200..300).contains(&status);
    let text = body.trim_start_matches('\u{feff}').trim_start();
    if !text.starts_with("#EXTM3U") {
        return ProbeResult {
            status,
            ok: false,
            kind: "invalid".into(),
            variants: vec![],
        };
    }
    let base = Url::parse(base_url).ok();
    let mut variants = Vec::new();
    let mut lines = text.lines().map(str::trim);
    let mut is_master = false;
    let mut is_media = false;
    while let Some(line) = lines.next() {
        if line.starts_with("#EXT-X-STREAM-INF") {
            is_master = true;
            let bandwidth = attr(line, "BANDWIDTH").and_then(|v| v.parse().ok());
            let resolution = attr(line, "RESOLUTION").map(str::to_string);
            // next non-comment, non-empty line is the URI
            for next in lines.by_ref() {
                if next.is_empty() || next.starts_with('#') {
                    continue;
                }
                let url = base
                    .as_ref()
                    .and_then(|b| b.join(next).ok())
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| next.to_string());
                variants.push(Variant {
                    url,
                    bandwidth,
                    resolution,
                });
                break;
            }
        } else if line.starts_with("#EXTINF") || line.starts_with("#EXT-X-TARGETDURATION") {
            is_media = true;
        }
    }
    let kind = if is_master {
        "master"
    } else if is_media {
        "media"
    } else {
        "invalid"
    };
    ProbeResult {
        status,
        ok: ok_status && kind != "invalid",
        kind: kind.into(),
        variants,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_playlist() {
        let body = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\nlow/index.m3u8?t=1\n#EXT-X-STREAM-INF:BANDWIDTH=2400000,RESOLUTION=\"1280x720\"\n/hi/index.m3u8\n";
        let r = analyze(body, "https://s1.itcnbd.live/A/index.m3u8?token=x", 200);
        assert!(r.ok);
        assert_eq!(r.kind, "master");
        assert_eq!(r.variants.len(), 2);
        assert_eq!(
            r.variants[0].url,
            "https://s1.itcnbd.live/A/low/index.m3u8?t=1"
        );
        assert_eq!(r.variants[0].bandwidth, Some(800000));
        assert_eq!(r.variants[1].url, "https://s1.itcnbd.live/hi/index.m3u8");
        assert_eq!(r.variants[1].resolution.as_deref(), Some("1280x720"));
    }

    #[test]
    fn media_and_invalid() {
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\nseg1.ts\n";
        assert_eq!(analyze(media, "https://h/p.m3u8", 200).kind, "media");
        let r = analyze("<html>denied</html>", "https://h/p.m3u8", 200);
        assert!(!r.ok);
        assert_eq!(r.kind, "invalid");
        assert!(!analyze(media, "https://h/p.m3u8", 403).ok);
    }
}
