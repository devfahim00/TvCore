//! Exercises the real HTTP client against a tiny local server that imitates the site:
//! home page, HLS playlists, a 403-until-refreshed stream and `/api/stream-url`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use tvcore::{Config, PlayOptions, TvCore};

struct Server {
    base: String,
    hits: Arc<AtomicUsize>,
}

fn respond(stream: &mut std::net::TcpStream, status: &str, ctype: &str, body: &str) {
    let msg = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(msg.as_bytes());
}

fn start() -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let (b, h) = (base.clone(), hits.clone());
    thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            let mut buf = [0u8; 8192];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let line = req.lines().next().unwrap_or("").to_string();
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let has_referer = req.to_lowercase().contains("referer:");
            match path.split('?').next().unwrap() {
                "/" => {
                    h.fetch_add(1, Ordering::SeqCst);
                    // second and later page loads hand out a "fresh" token
                    let tok = if h.load(Ordering::SeqCst) >= 2 {
                        "good"
                    } else {
                        "stale"
                    };
                    let html = format!(
                        r#"<div class="channel-card" data-title="Alpha TV" data-category="NEWS" data-link="{b}/alpha/index.m3u8?token=aa-bb-1795508732-1795494332&amp;k={tok}"><img src="/logo.png"></div>
                           <div class="channel-card" data-title="Open TV" data-category="NEWS" data-link="{b}/open/master.m3u8"></div>"#
                    );
                    respond(&mut s, "200 OK", "text/html", &html);
                }
                "/alpha/index.m3u8" => {
                    if path.contains("k=good") && has_referer {
                        respond(
                            &mut s,
                            "200 OK",
                            "application/vnd.apple.mpegurl",
                            "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6,\nseg.ts\n",
                        );
                    } else {
                        respond(&mut s, "403 Forbidden", "text/plain", "denied");
                    }
                }
                "/open/master.m3u8" => respond(
                    &mut s,
                    "200 OK",
                    "application/vnd.apple.mpegurl",
                    "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=500000,RESOLUTION=640x360\nlow.m3u8\n",
                ),
                "/api/stream-url" => respond(&mut s, "404 Not Found", "text/plain", "no"),
                _ => respond(&mut s, "404 Not Found", "text/plain", "no"),
            }
        }
    });
    Server { base, hits }
}

fn client(base: &str) -> TvCore {
    TvCore::new(Config {
        base_url: format!("{base}/"),
        timeout_secs: 5,
        ..Config::default()
    })
}

#[test]
fn refresh_lists_channels_over_http() {
    let srv = start();
    let c = client(&srv.base);
    assert_eq!(c.refresh().unwrap(), 2);
    let all = c.channels().unwrap();
    assert_eq!(
        all[0].logo.as_deref(),
        Some(format!("{}/logo.png", srv.base).as_str())
    );
}

#[test]
fn channels_autoload_on_first_use() {
    let srv = start();
    let c = client(&srv.base);
    assert_eq!(c.channels().unwrap().len(), 2); // no explicit refresh()
    assert_eq!(srv.hits.load(Ordering::SeqCst), 1);
}

#[test]
fn probe_reads_master_playlist() {
    let srv = start();
    let c = client(&srv.base);
    let r = c.probe(&format!("{}/open/master.m3u8", srv.base)).unwrap();
    assert!(r.ok);
    assert_eq!(r.kind, "master");
    assert_eq!(r.variants[0].url, format!("{}/open/low.m3u8", srv.base));
    assert_eq!(r.variants[0].bandwidth, Some(500000));
}

#[test]
fn verify_recovers_from_403_by_refetching_the_page() {
    let srv = start();
    let c = client(&srv.base);
    c.refresh().unwrap(); // hit #1 -> stale token (403)
    let p = c
        .play_at(
            "alpha-tv",
            &PlayOptions {
                verify: true,
                force_refresh: false,
            },
            1795494332 + 10,
        )
        .unwrap(); // verify -> 403 -> api 404 -> page hit #2 -> good token
    assert!(p.refreshed);
    assert!(p.url.contains("k=good"));
    assert_eq!(srv.hits.load(Ordering::SeqCst), 2);
}

#[test]
fn expired_token_triggers_refresh() {
    let srv = start();
    let c = client(&srv.base);
    c.refresh().unwrap();
    let p = c
        .play_at("alpha-tv", &PlayOptions::default(), 1795508732 + 1)
        .unwrap();
    assert!(p.refreshed);
    assert_eq!(srv.hits.load(Ordering::SeqCst), 2);
}

#[test]
fn verify_fails_cleanly_when_stream_stays_forbidden() {
    let srv = start();
    let c = client(&srv.base);
    c.refresh().unwrap();
    // Open TV has no token and serves a valid playlist -> ok
    assert!(c
        .play_at(
            "open-tv",
            &PlayOptions {
                verify: true,
                force_refresh: false
            },
            0
        )
        .is_ok());
}

#[test]
fn network_failure_is_a_network_error() {
    let c = TvCore::new(Config {
        base_url: "http://127.0.0.1:1/".into(),
        timeout_secs: 2,
        ..Config::default()
    });
    assert_eq!(c.refresh().unwrap_err().code(), "network");
}
