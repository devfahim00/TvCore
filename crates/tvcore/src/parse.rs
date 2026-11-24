use crate::error::{Error, Result};
use crate::model::Channel;
use crate::token::expiry_from_url;
use scraper::{Html, Selector};
use std::collections::HashMap;
use url::Url;

/// `"Star Sports 1 HD"` -> `"star-sports-1-hd"`.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "channel".to_string()
    } else {
        out
    }
}

/// Parses the site's home page into channels.
///
/// The site renders every channel as
/// `<div class="channel-card" data-title=".." data-category=".." data-link="..."><img src=".."></div>`.
/// `base_url` is used to absolutize relative logo / stream URLs.
pub fn parse_channels(html: &str, base_url: &str) -> Result<Vec<Channel>> {
    let base = Url::parse(base_url).map_err(|e| Error::Invalid(format!("bad base_url: {e}")))?;
    let doc = Html::parse_document(html);
    let card_sel = Selector::parse(".channel-card").expect("static selector");
    let img_sel = Selector::parse("img").expect("static selector");

    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();

    for card in doc.select(&card_sel) {
        let v = card.value();
        let title = v.attr("data-title").map(str::trim).unwrap_or("");
        let link = v.attr("data-link").map(str::trim).unwrap_or("");
        if title.is_empty() || link.is_empty() {
            continue;
        }
        let category = v
            .attr("data-category")
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let stream_url = match base.join(link) {
            Ok(u) => u.to_string(),
            Err(_) => continue,
        };
        let logo = card
            .select(&img_sel)
            .next()
            .and_then(|i| i.value().attr("src"))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|s| base.join(s).ok())
            .map(|u| u.to_string());

        let slug = slugify(title);
        let n = seen.entry(slug.clone()).or_insert(0);
        *n += 1;
        let id = if *n == 1 { slug } else { format!("{slug}-{n}") };

        out.push(Channel {
            id,
            title: title.to_string(),
            category,
            logo,
            expires_at: expiry_from_url(&stream_url),
            stream_url,
        });
    }

    if out.is_empty() {
        return Err(Error::Parse(
            "no channel cards found (page structure changed, or the request was blocked)".into(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Star Sports 1 HD"), "star-sports-1-hd");
        assert_eq!(slugify("  9XM-Music "), "9xm-music");
        assert_eq!(slugify("!!!"), "channel");
    }

    #[test]
    fn duplicate_titles_get_suffix_and_relative_urls_resolve() {
        let html = r#"
        <div class="channel-card" data-title="Disney" data-category="KIDS" data-link="/a.m3u8"><img src="/images/x.png"></div>
        <div class="channel-card" data-title="Disney" data-category="KIDS" data-link="https://h/b.m3u8?x=1&amp;y=2"></div>"#;
        let ch = parse_channels(html, "https://itcnbd.live/").unwrap();
        assert_eq!(ch[0].id, "disney");
        assert_eq!(ch[1].id, "disney-2");
        assert_eq!(ch[0].stream_url, "https://itcnbd.live/a.m3u8");
        assert_eq!(
            ch[0].logo.as_deref(),
            Some("https://itcnbd.live/images/x.png")
        );
        assert_eq!(ch[1].stream_url, "https://h/b.m3u8?x=1&y=2");
        assert_eq!(ch[1].logo, None);
    }

    #[test]
    fn empty_page_is_an_error() {
        assert!(matches!(
            parse_channels("<html></html>", "https://itcnbd.live/"),
            Err(Error::Parse(_))
        ));
    }
}
