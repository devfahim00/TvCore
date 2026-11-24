//! Live tests against the real ITCN BD playlist.
//!
//! These hit `https://s3.itcnbd.live/playlist.m3u` — the official ITCN BD
//! playlist used by <https://itcnbd.live>. The itcnbd servers answer
//! **only from Bangladesh ISP networks (BDIX)**; datacenter IPs get a 403.
//!
//! Run them from a BD network (or a machine that can reach the CDN):
//!
//! ```sh
//! cargo test -p tvcore --test live_itcnbd -- --ignored --nocapture
//! ```

use tvcore::{StreamKind, TvCore};

#[test]
#[ignore = "requires a network that can reach itcnbd.live (BDIX)"]
fn live_load_search_play() {
    let mut tv = TvCore::new();
    let n = tv
        .load_itcnbd()
        .expect("ITCN BD playlist must be reachable from BD networks");
    println!("loaded {} channels", n);
    assert!(n > 10, "playlist suspiciously small: {}", n);

    // listing sanity
    let cats = tv.categories();
    println!("categories: {:?}", cats);
    assert!(!cats.is_empty());

    // search sanity
    let sports = tv.search("sports", 20);
    println!(
        "sports hits: {:?}",
        sports.iter().map(|c| c.name.clone()).collect::<Vec<_>>()
    );
    assert!(!sports.is_empty());

    // play sanity: first channel resolves a playable HLS target
    let ch = tv
        .channels()
        .iter()
        .find(|c| c.url.contains(".m3u8"))
        .expect("an HLS channel");
    let target = tv.play(&ch.id).unwrap();
    println!(
        "play {} -> {} ({})",
        target.channel_name,
        target.url,
        target.kind.as_str()
    );
    assert!(matches!(
        target.kind,
        StreamKind::Hls | StreamKind::Ts | StreamKind::Unknown
    ));
    assert!(target.url.starts_with("http"));

    // probe: stream should be reachable from BD networks
    let status = tv.probe(&ch.id).unwrap();
    println!("probe {}: HTTP {}", ch.id, status.http_status);
    assert!(status.reachable, "stream probe failed: {:?}", status);
}

#[test]
#[ignore = "requires a network that can reach itcnbd.live (BDIX)"]
fn live_search_bangla() {
    let mut tv = TvCore::new();
    tv.load_itcnbd().unwrap();
    let hits = tv.search("bangla", 50);
    println!("bangla hits: {}", hits.len());
    assert!(!hits.is_empty());
}
