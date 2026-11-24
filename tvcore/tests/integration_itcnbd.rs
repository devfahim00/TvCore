//! Offline integration test: the full TvCore pipeline against a fixture
//! snapshot of the ITCN BD playlist (same URL patterns as
//! `https://s3.itcnbd.live/playlist.m3u`), with an injected mock HTTP layer.

use std::sync::Arc;
use tvcore::http::{Headers, HttpFetcher, HttpResponse};
use tvcore::{StreamKind, TvCore, TvError};

const PLAYLIST_FIXTURE: &str = include_str!("fixtures/itcnbd_playlist.m3u");

/// Mock HTTP layer: serves the fixture for the itcnbd playlist URL,
/// 403-with-HTML for blocked hosts, and a configurable probe status.
#[derive(Debug, Clone)]
struct MockHttp {
    probe_status: u16,
    fail_probe: bool,
}

impl MockHttp {
    fn new() -> Self {
        MockHttp {
            probe_status: 200,
            fail_probe: false,
        }
    }
}

impl HttpFetcher for MockHttp {
    fn get_text(&self, url: &str, _headers: &Headers) -> tvcore::TvResult<HttpResponse> {
        if url == tvcore::ITCNBD_PLAYLIST_URL {
            Ok(HttpResponse {
                status: 200,
                body: PLAYLIST_FIXTURE.to_string(),
            })
        } else if url.contains("blocked.example") {
            Ok(HttpResponse {
                status: 403,
                body: "<html><head><title>403 Forbidden</title></head></html>".to_string(),
            })
        } else {
            Err(TvError::HttpStatus(404))
        }
    }

    fn probe(&self, _url: &str, _headers: &Headers) -> tvcore::TvResult<u16> {
        if self.fail_probe {
            Err(TvError::Http("simulated transport failure".into()))
        } else {
            Ok(self.probe_status)
        }
    }
}

fn tv() -> TvCore {
    TvCore::with_http(Arc::new(MockHttp::new()))
}

// ---------------------------------------------------------------------------
// listing
// ---------------------------------------------------------------------------

#[test]
fn loads_full_itcnbd_playlist() {
    let mut tv = tv();
    let n = tv.load_itcnbd().unwrap();
    assert!(n >= 60, "expected a realistic channel count, got {}", n);
    assert_eq!(tv.channel_count(), n);
}

#[test]
fn channels_have_itcnbd_metadata() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let chans = tv.channels();
    assert!(chans.iter().all(|c| c.source == "itcnbd"));
    // every channel has name, category and a stream URL
    assert!(chans.iter().all(|c| !c.name.is_empty()));
    assert!(chans.iter().all(|c| !c.category.is_empty()));
    assert!(chans
        .iter()
        .all(|c| c.url.starts_with("http://") || c.url.starts_with("https://")));
    // ids are stable slugs, unique
    let mut ids: Vec<&str> = chans.iter().map(|c| c.id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), chans.len());
}

#[test]
fn categories_cover_site_groups() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let cats = tv.categories();
    let names: Vec<String> = cats.iter().map(|(c, _)| c.to_uppercase()).collect();
    for expected in ["BANGLA", "SPORTS", "NEWS", "KIDS", "ENGLISH"] {
        assert!(
            names.iter().any(|c| c.contains(expected)),
            "missing category {} in {:?}",
            expected,
            names
        );
    }
}

#[test]
fn category_listing_and_counts() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let all = tv.channels();
    let sports = tv.channels_in_category("sports");
    assert!(!sports.is_empty());
    assert!(sports
        .iter()
        .all(|c| c.category.to_uppercase().contains("SPORTS")));
    let count = tv
        .categories()
        .into_iter()
        .find(|(c, _)| c.to_uppercase().contains("SPORTS"))
        .map(|(_, n)| n)
        .unwrap();
    assert_eq!(count, sports.len());
    assert_eq!(
        all.len(),
        tv.categories().iter().map(|(_, n)| n).sum::<usize>()
    );
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------

#[test]
fn search_sports_channels() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let hits = tv.search("sports", 50);
    assert!(!hits.is_empty());
    assert!(hits.iter().any(|c| c.name.to_lowercase().contains("sports")
        || c.category.to_lowercase().contains("sports")));
}

#[test]
fn search_bangla_matches_category_and_names() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let hits = tv.search("bangla", 100);
    assert!(hits.len() >= 3);
    assert!(hits.iter().all(|c| c.name.to_lowercase().contains("bangla")
        || c.category.to_lowercase().contains("bangla")));
}

#[test]
fn search_typo_tolerance() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    // "somoy" mistyped should still find Somoy TV if present
    let hits = tv.search("somoy", 10);
    if !hits.is_empty() {
        assert!(hits[0].name.to_lowercase().contains("somoy"));
    }
    // channel-name typo against known fixture channels
    let hits = tv.search("chanal i", 10);
    assert!(hits
        .iter()
        .any(|c| c.name.to_lowercase().contains("channel i")));
}

#[test]
fn search_case_insensitive_and_limited() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let a = tv.search("TV", 5);
    let b = tv.search("tv", 5);
    assert_eq!(a.len(), b.len());
    assert!(a.len() <= 5);
}

#[test]
fn search_empty_query_lists_all() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    assert_eq!(tv.search("", 10).len(), 10);
    assert_eq!(tv.search("", usize::MAX).len(), tv.channel_count());
}

#[test]
fn search_no_result_is_empty() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    assert!(tv.search("zzz nonexistent channel xyzzy", 10).is_empty());
}

// ---------------------------------------------------------------------------
// play
// ---------------------------------------------------------------------------

#[test]
fn play_resolves_hls_target_with_headers() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let some = tv
        .channels()
        .iter()
        .find(|c| c.url.contains(".m3u8"))
        .unwrap()
        .clone();
    let t = tv.play(&some.id).unwrap();
    assert_eq!(t.channel_id, some.id);
    assert_eq!(t.channel_name, some.name);
    assert_eq!(t.url, some.url);
    assert_eq!(t.kind, StreamKind::Hls);
    let ua = t
        .headers
        .iter()
        .find(|(k, _)| k == "User-Agent")
        .expect("user agent header present");
    assert!(ua.1.contains("Mozilla/5.0"));
}

#[test]
fn play_by_name_or_search() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let chans = tv.channels().to_vec();
    let probe = chans.iter().find(|c| c.name == "Somoy TV");
    if let Some(ch) = probe {
        let t = tv.play_any("somoy tv").unwrap();
        assert_eq!(t.channel_id, ch.id);
    }
    // arbitrary unique name substring resolves through search
    if let Some(ch) = chans
        .iter()
        .find(|c| c.category.to_uppercase().contains("SPORTS"))
    {
        let t = tv.play_any(&ch.name).unwrap();
        assert_eq!(t.channel_id, ch.id);
    }
}

#[test]
fn play_unknown_id_errors() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let err = tv.play("definitely-not-here").unwrap_err();
    assert!(matches!(err, TvError::NotFound(_)));
}

#[test]
fn play_empty_catalog_errors() {
    let tv = tv();
    assert!(matches!(tv.play("any"), Err(TvError::NotFound(_))));
}

#[test]
fn probe_reports_reachability() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let ch = tv.channels()[0].clone();
    let status = tv.probe(&ch.id).unwrap();
    assert_eq!(status.channel_id, ch.id);
    assert!(status.reachable);
    assert_eq!(status.http_status, 200);

    // transport failure -> status 0, reachable false (must not panic)
    let tv2 = TvCore::with_http(Arc::new(MockHttp {
        probe_status: 0,
        fail_probe: true,
    }));
    let mut tv2 = tv2;
    tv2.load_itcnbd().unwrap();
    let status = tv2.probe(&ch.id).unwrap();
    assert_eq!(status.http_status, 0);
    assert!(!status.reachable);
}

// ---------------------------------------------------------------------------
// error paths
// ---------------------------------------------------------------------------

#[test]
fn blocked_source_surfaces_http_error() {
    struct Blocked;
    impl HttpFetcher for Blocked {
        fn get_text(&self, _u: &str, _h: &Headers) -> tvcore::TvResult<HttpResponse> {
            Ok(HttpResponse {
                status: 403,
                body: "<html>403</html>".into(),
            })
        }
        fn probe(&self, _u: &str, _h: &Headers) -> tvcore::TvResult<u16> {
            Ok(403)
        }
    }
    let mut tv = TvCore::with_http(Arc::new(Blocked));
    let err = tv.load_itcnbd().unwrap_err();
    assert!(matches!(err, TvError::HttpStatus(403)));
    assert_eq!(tv.channel_count(), 0);
}

#[test]
fn html_instead_of_playlist_is_rejected() {
    let mut tv = TvCore::new();
    // an HTML error page must never create phantom channels
    assert_eq!(
        tv.load_playlist_str("<html><body>403</body></html>", "x"),
        0
    );
    assert_eq!(tv.load_playlist_str("   ", "x"), 0);
    assert!(tv.channels().is_empty());
}

// ---------------------------------------------------------------------------
// multi-source merge
// ---------------------------------------------------------------------------

#[test]
fn merging_sources_adds_alternatives() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let before = tv.channel_count();

    // second source: overlapping Somoy TV (backup URL + exact duplicate)
    // plus a brand new Al Jazeera English
    let n = tv.load_playlist_str(include_str!("fixtures/merge_source.m3u"), "backup");
    let after = tv.channel_count();
    assert!(n >= 1, "Al Jazeera English is a brand new channel");
    assert_eq!(after, before + 1, "exactly one new channel (Al Jazeera)");

    // the overlapping Somoy TV gained the backup mirror as alternative
    let somoy = tv
        .channels()
        .iter()
        .find(|c| c.name == "Somoy TV")
        .expect("Somoy TV from the primary source");
    assert!(somoy
        .alternatives
        .contains(&"https://backup.example/somoy-backup.m3u8".to_string()));

    // the exact duplicate did not grow the catalog, and re-loading the whole
    // second source again changes nothing either
    let stable = tv.channel_count();
    tv.load_playlist_str(include_str!("fixtures/merge_source.m3u"), "backup-2");
    assert_eq!(tv.channel_count(), stable);
}

#[test]
fn find_by_id_works() {
    let mut tv = tv();
    tv.load_itcnbd().unwrap();
    let ch = tv.channels()[0].clone();
    assert_eq!(tv.find(&ch.id).map(|c| c.id.clone()), Some(ch.id));
    assert!(tv.find("zzz").is_none());
}
