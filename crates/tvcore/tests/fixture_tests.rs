//! Offline tests against a saved copy of the site's home page (tokens sanitized).

use tvcore::{Config, PlayOptions, SearchQuery, TvCore};

const HTML: &str = include_str!("fixtures/home.html");

fn core() -> TvCore {
    let c = TvCore::new(Config::default());
    c.load_html(HTML).unwrap();
    c
}

#[test]
fn lists_every_channel() {
    let c = core();
    let all = c.channels().unwrap();
    assert_eq!(all.len(), 75);
    assert_eq!(all[0].title, "T Sports HD");
    assert_eq!(all[0].category, "SPORTS");
    assert!(all.iter().all(|ch| ch.stream_url.starts_with("https://")));
    // ids are unique
    let mut ids: Vec<_> = all.iter().map(|c| c.id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 75);
}

#[test]
fn logos_are_absolute() {
    let c = core();
    let atn = c.get("atn-bangla").unwrap();
    assert_eq!(
        atn.logo.as_deref(),
        Some("https://itcnbd.live/images/atn_bangla_4c35d9.png")
    );
    // names with spaces get percent-encoded
    let dbc = c.get("dbc-news").unwrap();
    assert_eq!(
        dbc.logo.as_deref(),
        Some("https://itcnbd.live/images/DBC%20News%20HD.png")
    );
}

#[test]
fn categories_have_counts() {
    let cats = core().categories().unwrap();
    let get = |n: &str| cats.iter().find(|c| c.name == n).unwrap().count;
    assert_eq!(cats.len(), 10);
    assert_eq!(get("BANGLA"), 23);
    assert_eq!(get("SPORTS"), 14);
    assert_eq!(get("HINDI"), 10);
    assert_eq!(get("ISLAMIC"), 1);
    assert_eq!(cats.iter().map(|c| c.count).sum::<usize>(), 75);
}

#[test]
fn search_finds_channels() {
    let c = core();
    let q = |s: &str| SearchQuery {
        query: s.into(),
        ..Default::default()
    };
    assert_eq!(c.search(&q("jamuna"))[0].id, "jamuna-tv");
    assert_eq!(c.search(&q("star sports"))[0].title, "Star Sports 1 HD");
    assert_eq!(c.search(&q("STARSPORTS"))[0].title, "Star Sports 1 HD");
    assert_eq!(c.search(&q("bbc"))[0].title, "BBC News");
    assert!(c.search(&q("definitely-not-a-channel")).is_empty());
    let sports = c.search(&SearchQuery {
        query: "".into(),
        category: Some("sports".into()),
        limit: Some(3),
    });
    assert_eq!(sports.len(), 3);
    assert!(sports.iter().all(|c| c.category == "SPORTS"));
    // duplicate "Disney" titles stay distinguishable
    let disney = c.search(&q("disney"));
    assert_eq!(disney.len(), 2);
    assert_ne!(disney[0].id, disney[1].id);
}

#[test]
fn token_expiry_is_parsed() {
    let c = core();
    assert_eq!(c.get("jamuna-tv").unwrap().expires_at, Some(1795508732));
    // the first (s3) "Disney" stream has no token
    assert_eq!(c.get("disney").unwrap().expires_at, None);
}

#[test]
fn play_returns_url_and_headers() {
    let c = core();
    let now = 1795494332 + 60; // freshly issued token
    let p = c
        .play_at("jamuna-tv", &PlayOptions::default(), now)
        .unwrap();
    assert!(p
        .url
        .starts_with("https://s1.itcnbd.live/JAMUNA/index.m3u8?token="));
    assert_eq!(p.headers.get("Referer").unwrap(), "https://itcnbd.live/");
    assert!(p.headers.contains_key("User-Agent"));
    assert!(!p.expired);
    assert!(!p.refreshed);
    assert_eq!(p.kind, "hls");
}

#[test]
fn unknown_channel_is_an_error() {
    let err = core().play("nope", &PlayOptions::default()).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn m3u_export() {
    let m3u = core().export_m3u().unwrap();
    assert!(m3u.starts_with("#EXTM3U\n"));
    assert_eq!(m3u.matches("#EXTINF:-1").count(), 75);
    assert!(m3u.contains("group-title=\"SPORTS\",T Sports HD"));
    assert!(m3u.contains("#EXTVLCOPT:http-referrer=https://itcnbd.live/"));
}
