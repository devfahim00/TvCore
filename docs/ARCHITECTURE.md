# Architecture

TvCore is three crates with one job split cleanly across them:

```
tvcore-cli (bin)          humans on a terminal
tvcore-ffi  (cdylib/…)    C ABI — JSON in, JSON out   ┐ FFI boundary
tvcore     (lib)          the actual engine           ┘
```

## Module map (tvcore)

| Module | Responsibility | Key types |
| --- | --- | --- |
| `model` | Data shapes for channels and playback | `Channel`, `StreamTarget`, `StreamKind`, `StreamStatus` |
| `m3u` | Extended-M3U parsing (the playlist format itcnbd/iptv-org emit) | `parse_m3u`, `ParseStats` |
| `catalog` | In-memory channel store, id index, category index, merge/dedupe | `Catalog`, `AddStats` |
| `search` | Ranked matching against name+category | `search_channels`, `SearchResult` |
| `http` | Pluggable transport + the default ureq client | `HttpFetcher`, `UreqFetcher`, `HttpResponse` |
| `provider` | Where playlists come from | `ItcnbdProvider`, `M3uUrlProvider`, `M3uFileProvider`, `Provider` trait |
| `error` | One error enum for everything | `TvError`, `TvResult` |
| `lib.rs` | The `TvCore` facade users call | `TvCore` |

## Data flow

```
load_itcnbd()
   │
   ▼
ItcnbdProvider ── HttpFetcher.get_text(s3.itcnbd.live/playlist.m3u,
                                        browser UA + Referer)
   │                        (or M3uUrlProvider / M3uFileProvider / your Provider impl)
   ▼
parse_m3u(text, source, base_url)
   │  - #EXTINF attributes: tvg-id/-name/-logo, group-title
   │  - #EXTGRP, #EXTVLCOPT headers, Kodi pipe options
   │  - relative URL resolution against the playlist base
   │  - refuses HTML error pages (no phantom channels)
   ▼
Catalog.add_channels(...)          search / play:
   │  - dedupe by id                   Catalog.search ─ score_channel
   │  - merge mirrors as alternatives   (case-insens, tokens, fuzzy, typo)
   │  - category index counts          TvCore.play(id) ─ StreamTarget
   ▼                                       (url + kind + required headers)
[Channel; N]                              TvCore.probe(id) ─ StreamStatus
```

## Why these design choices

**JSON at the FFI boundary.** Struct layout, ownership and memory rules are
where most FFI stacks break. Every `tvcore-ffi` call returns a
NUL-terminated UTF-8 JSON string and every platform decodes JSON natively —
the surface stays tiny (`include/tvcore.h`) and version-proof.

**A blocking core.** `ureq` + rustls, no async runtime: the library stays
small (no tokio), cross-compiles trivially for Android/iOS, and FFI hosts —
which always have their own threading model — simply call from a worker
thread. This is documented as a rule for each platform guide.

**Injectable transport.** All network I/O goes through the `HttpFetcher`
trait. TvCore's default is `UreqFetcher`, tests use fixtures, and hosts can
route requests through their own stack (`TvCore::with_http`) — a proxy, an
OkHttp bridge, or a pinned-CA corporate client.

**The playlist is the contract.** itcnbd's own player is a static frontend
over `https://s3.itcnbd.live/playlist.m3u`; parsing that same public M3U is
more stable than scraping HTML — and the parser (generic M3U/M3U8) works
with iptv-org, Mrgify-BDIX or any mirror as fallback when BDIX blocks the
primary host. The `Provider` trait is the extension point if you need a
different scraper someday.

**Merge, don't overwrite.** Loading a second source with the same channels
keeps the first id, adds the new stream URL as an `alternative`, and enriches
missing metadata (logo/UA/Referer). `play_alternative` lets players fail over
without re-searching.

## Error model

| Variant | Raised when |
| --- | --- |
| `Http` | transport failure (DNS, TLS, timeout) |
| `HttpStatus` | server answered 4xx/5xx (403 = BDIX geo-block) |
| `Parse` | not a playlist (HTML/empty), malformed beyond repair |
| `NotFound` | id/name didn't match any channel |
| `InvalidInput` | caller passed empty/NULL arguments |
| `Io` | local file problems (missing, unreadable) |

FFI mapping: `-1` / `NULL` returns + `tvcore_last_error()` (thread-local,
read immediately after the failing call).

## Threading & safety

- Core is `Send + Sync`; `TvCore` is one handle shareable across threads.
- `tvcore` crate is `#![forbid(unsafe_code)]` — all unsafe lives in
  `tvcore-ffi` (pointer marshalling) and is exercised by tests + the C demo.
- FFI strings are heap-owned and freed only via `tvcore_string_free`.

## Testing strategy

| Layer | How |
| --- | --- |
| Parser | unit tests: quoted commas, CRLF, EXTGRP, VLCOPT, pipes, relative URLs, HTML rejection |
| Search | unit tests: exact/prefix/contains/token/fuzzy bands, AND semantics, limits |
| Catalog | unit tests: merge alternatives, duplicates, category counts |
| End-to-end | integration tests against a **realistic fixture snapshot** of the ITCN BD playlist (same URL/logo/category patterns) through a mock `HttpFetcher` |
| C ABI | `tvcore-ffi` Rust tests + `examples/demo.c` compiled and run against the real `.so` |
| Live CDN | `#[ignore]`d tests hitting `s3.itcnbd.live` — run from a BDIX network: `cargo test -- --ignored` |

## Extending

- **New source:** implement `Provider` (one method, `load(&dyn HttpFetcher) -> Vec<Channel>`) and `TvCore::load_provider`.
- **New host platform:** the C ABI is the whole surface — a Swift/`dlopen`/`ctypes` binding is ~100 lines; see `include/tvcore.h`.
- **Custom player headers:** already supported per-channel (UA/Referer/cookies parsed from the playlist or set on `Channel`).
