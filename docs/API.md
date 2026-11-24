# Rust API reference

`tvcore` — the core crate. Everything below is public API.

## `TvCore` (facade)

```rust
use tvcore::TvCore;
```

| Method | Returns | Notes |
| --- | --- | --- |
| `new()` | `TvCore` | default ureq client, 30 s timeout, browser headers |
| `with_http(Arc<dyn HttpFetcher>)` | `TvCore` | inject your own transport (tests, platform stack) |
| `load_itcnbd()` | `TvResult<usize>` | official ITCN BD playlist; count = newly added |
| `load_itcnbd_from(url)` | `TvResult<usize>` | mirror playlist URL |
| `load_playlist_url(url)` | `TvResult<usize>` | any remote M3U/M3U8 |
| `load_playlist_file(path)` | `TvResult<usize>` | local playlist file |
| `load_playlist_str(text, source)` | `usize` | in-memory playlist (offline/test) |
| `load_provider(&dyn Provider)` | `TvResult<usize>` | any custom provider |
| `set_channels(Vec<Channel>)` | `AddStats` | replace/seed the catalog directly |
| `channels()` | `&[Channel]` | insertion order |
| `channel_count()` | `usize` | |
| `categories()` | `Vec<(String, usize)>` | display name + count, sorted desc |
| `channels_in_category(cat)` | `Vec<&Channel>` | case-insensitive |
| `find(id)` | `Option<&Channel>` | |
| `find_any(key)` | `Option<&Channel>` | id → exact name → best search hit |
| `search(query, limit)` | `Vec<Channel>` | ranked clones, best first |
| `play(id)` | `TvResult<StreamTarget>` | primary stream + headers |
| `play_any(key)` | `TvResult<StreamTarget>` | id/name/free-text |
| `play_alternative(id, n)` | `TvResult<StreamTarget>` | mirror stream `n` (0-based) |
| `probe(id)` | `TvResult<StreamStatus>` | liveness check; never fails on transport errors |
| `http()` | `&dyn HttpFetcher` | reuse the transport from the host |

## Data model

```rust
pub struct Channel {
    pub id: String,            // tvg-id slug or slugified name
    pub name: String,
    pub category: String,      // group-title / EXTGRP
    pub logo: Option<String>,  // tvg-logo
    pub url: String,           // primary stream URL
    pub alternatives: Vec<String>,
    pub source: String,        // "itcnbd" | "iptv-org" | your label
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub extra_headers: Vec<(String, String)>,
}
```

- `Channel::new(id, name, category, url)`, `Channel::slugify(name)`,
  `ch.all_urls()`.
- Serialization (serde): camelCase — `userAgent`, `extraHeaders`; optional
  fields are skipped when empty.

```rust
pub enum StreamKind { Hls, Dash, Ts, Unknown }   // from_url(&str) detector

pub struct StreamTarget {                        // from_channel(...)
    pub channel_id: String,
    pub channel_name: String,
    pub url: String,
    pub kind: StreamKind,
    pub headers: Vec<(String, String)>,          // UA + Referer + extras
}

pub struct StreamStatus { pub channel_id: String,
                          pub http_status: u16,   // 0 = transport failure
                          pub reachable: bool }   // 2xx
```

## M3U parsing

```rust
tvcore::parse_m3u(text, source, base_url: Option<&str>) -> TvResult<Vec<Channel>>
tvcore::parse_m3u_with_stats(...) -> TvResult<(Vec<Channel>, ParseStats)>
```

Understands: `#EXTM3U` headers, `#EXTINF` attributes (`tvg-id`,
`tvg-name`, `tvg-logo`, `group-title`, quoted values with embedded
commas/spaces), `#EXTGRP`, `#EXTVLCOPT:http-user-agent|http-referrer`,
Kodi pipe options (`url|User-Agent=..|Referer=..|`), relative URLs (joined
against the playlist base URL), CRLF/LF, URL lines without `#EXTINF`, and
returns `ParseStats.skipped` for `#EXTINF` entries missing a URL. HTML and
empty inputs are rejected with `TvError::Parse`.

## Search

```rust
tvcore::search_channels(&[Channel], query, limit) -> Vec<SearchResult>
```

- Normalizes case and whitespace; splits queries into tokens.
- AND semantics: every token must hit name **or** category.
- Scoring (higher = better): exact name 100, prefix 80, contains 60, whole
  name fuzzy 55−10·d, category contains 25, token exact 30 / prefix 24 /
  contains 18 / fuzzy 14−5·d; token hits in the category keep weak matches
  alive at 12. Typo tolerance ≈ distance ≤ 2–3.
- Empty query → all channels (score 0), original order.

## HTTP layer

```rust
pub trait HttpFetcher: Send + Sync {
    fn get_text(&self, url: &str, headers: &[(String, String)])
        -> Result<HttpResponse, TvError>;
    fn probe(&self, url: &str, headers: &[(String, String)])
        -> Result<u16, TvError>;           // HEAD, fallback GET Range
}

pub struct HttpResponse { pub status: u16, pub body: String }

UreqFetcher::new()                                    // 30 s default
UreqFetcher::with_timeout_and_headers(timeout, default_headers)
tvcore::http::default_headers()                      // browser UA etc.
```

## Providers

```rust
pub trait Provider {
    fn name(&self) -> &str;
    fn load(&self, http: &dyn HttpFetcher) -> TvResult<Vec<Channel>>;
}
```

- `ItcnbdProvider::new()` / `with_url(url)` — fetches
  `https://s3.itcnbd.live/playlist.m3u` with browser UA +
  `Referer: https://itcnbd.live/`.
- `M3uUrlProvider::new(url)` / `with_label(label, url)`.
- `M3uFileProvider::new(path)` / `with_label(label, path)`.

Constants: `ITCNBD_PLAYLIST_URL`, `DEFAULT_USER_AGENT` (browser UA that
itcnbd's nginx expects).

## Catalog

`Catalog::add_channels(iter)` merges by id: same id + same URL → duplicate;
same id + new URL → `alternatives` push + metadata enrichment
(`AddStats { added, merged, duplicates }`).

## Errors

```rust
pub enum TvError { Http(String), HttpStatus(u16), Parse(String),
                   NotFound(String), InvalidInput(String), Io(String) }
```

`impl Display`, `impl std::error::Error`, `From<ureq::Error>`,
`From<std::io::Error>`.

## Constants & version

`tvcore::VERSION`, `tvcore::ITCNBD_PLAYLIST_URL`, `tvcore::DEFAULT_USER_AGENT`.

## A complete example

```rust
use tvcore::TvCore;

fn main() -> anyhow-like Result<(), tvcore::TvError> {
    let mut tv = TvCore::new();

    // LIST (multi-source, merged, deduped)
    let n = tv.load_itcnbd()?;
    tv.load_playlist_url("https://iptv-org.github.io/iptv/countries/bd.m3u")?;
    println!("{} channels", n);

    for (cat, count) in tv.categories() {
        println!("{cat}: {count}");
    }

    // SEARCH
    for ch in tv.search("t spor", 10) {
        println!("{} [{}] -> {}", ch.name, ch.category, ch.url);
    }

    // PLAY + PROBE
    let target = tv.play_any("t sports")?;
    println!("{} ({}) headers={:?}", target.url, target.kind.as_str(), target.headers);
    let status = tv.probe(&target.channel_id)?;
    println!("reachable={} http={}", status.reachable, status.http_status);
    Ok(())
}
```

(`search` returns `Channel` clones; the ranking score itself is available
via `search_channels` or the FFI JSON — see `docs/ARCHITECTURE.md`.)
