# TvCore

Rust core for **[itcnbd.live](https://itcnbd.live/)**: list every channel, search channels, and get a playable stream URL.
One native library, usable from Flutter, Kotlin/Android, Swift, Python, C/C++ and the command line.

```
            ┌───────────────┐   ┌─────────────────────────────────────────────┐
 Flutter ──▶│ dart:ffi      │   │                tvcore-ffi (C ABI)           │
 Kotlin  ──▶│ JNA / JNI     │──▶│  JSON in / JSON out, panic-safe, thread-safe │
 Swift   ──▶│ C header      │   └───────────────────────┬─────────────────────┘
 Python  ──▶│ ctypes        │                           │
 C / C++ ──▶│ tvcore.h      │   ┌───────────────────────▼─────────────────────┐
            └───────────────┘   │                  tvcore (Rust)              │
                                │ parse page · search · token expiry · HLS    │
                                │ probe · play resolution · M3U export · HTTP │
                                └──────────────────────────────────────────────┘
```

## What it does

| Feature | API |
|---|---|
| List all channels (75 at the time of writing, 10 categories) | `channels()` / `tvcore_list` |
| Categories with counts | `categories()` / `tvcore_categories` |
| Search (case, space and punctuation insensitive, ranked) | `search(query, category, limit)` |
| Play: stream URL + required headers, token refresh when expired | `play(id, {verify})` |
| Probe an HLS playlist, list quality variants | `probe(url)` |
| Export M3U for VLC / Kodi / TiviMate | `export_m3u()` |
| Offline mode: feed HTML yourself, no HTTP in the library | `load_html(html)`, build with `--no-default-features` |

## Quick start

```bash
cargo build --release                     # CLI + shared library
./target/release/tvcore refresh           # fetch itcnbd.live
./target/release/tvcore list BANGLA
./target/release/tvcore search "star sports" --limit 5
./target/release/tvcore play jamuna-tv --verify
./target/release/tvcore m3u > itcn.m3u
mpv "$(./target/release/tvcore play jamuna-tv | head -1)" --http-header-fields="Referer: https://itcnbd.live/"
```

Rust:

```rust
use tvcore::{Config, PlayOptions, SearchQuery, TvCore};

let core = TvCore::new(Config::default());
core.refresh()?;
let hit = &core.search(&SearchQuery { query: "jamuna".into(), ..Default::default() })[0];
let play = core.play(&hit.id, &PlayOptions { verify: true, ..Default::default() })?;
// give play.url and play.headers to your player
```

Python (`bindings/python/tvcore.py`), Dart (`bindings/dart`), Kotlin (`bindings/kotlin`) wrap the same C ABI.
See **[docs/INTEGRATION.md](docs/INTEGRATION.md)** for Flutter, Android/ExoPlayer, Swift and C examples.

## Layout

```
crates/tvcore        core library (no FFI)           tests: fixture, mock HTTP server
crates/tvcore-ffi    C ABI (cdylib + staticlib)      header: crates/tvcore-ffi/include/tvcore.h
crates/tvcore-cli    `tvcore` command
bindings/{dart,kotlin,python}
docs/                API, architecture, site protocol, integration, building
```

## Docs

* [docs/API.md](docs/API.md) — Rust API, C ABI, JSON shapes, error codes
* [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — design and decisions
* [docs/SITE_PROTOCOL.md](docs/SITE_PROTOCOL.md) — how itcnbd.live exposes channels and tokens
* [docs/INTEGRATION.md](docs/INTEGRATION.md) — Flutter, Kotlin, Swift, Python, C
* [docs/BUILDING.md](docs/BUILDING.md) — Android / iOS / desktop builds, features, tests

## Testing status

| Layer | How it is tested |
|---|---|
| Parsing, search, tokens, playlists, M3U | unit tests + a saved copy of the site's page (`crates/tvcore/tests/fixtures/home.html`, tokens zeroed) |
| HTTP client, 403 → token refresh, expiry refresh | integration tests against a local mock server imitating the site |
| C ABI | Rust test calling the exported functions, plus `bindings/python/test_tvcore.py` |
| Dart and Kotlin wrappers | written against the same ABI, **not compiled in CI yet** |
| Live site | ignored by default; run `tvcore refresh` from a machine that can reach the site |

Run everything: `cargo test --workspace && python3 bindings/python/test_tvcore.py`

## Notes

* Stream URLs on the site are signed with a token that lives about **4 hours**; TvCore reads the expiry from the URL and
  re-fetches the page when a channel is about to expire. Do not cache `url` for long, call `play()` each time.
* The library does not call the site's analytics endpoint (`/api/log-play`).
* The site may change its markup. If `refresh` fails with a `parse` error, see `docs/SITE_PROTOCOL.md`.
* You are responsible for having the right to use the streams you play.

## License

MIT
