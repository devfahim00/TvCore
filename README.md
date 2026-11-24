# TvCore

[![CI](https://github.com/defahim00/TvCore/actions/workflows/ci.yml/badge.svg)](https://github.com/defahim00/TvCore/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange?logo=rust)](Cargo.toml)

**A tiny, embeddable Rust core for live TV: list channels, search channels, play channels.**

TvCore powers TV apps that talk to [itcnbd.live](https://itcnbd.live) (ITCN BD — the popular Bangladeshi BDIX live-TV service) and *any* M3U/M3U8 playlist source:

- **List** — load the official ITCN BD playlist (`https://s3.itcnbd.live/playlist.m3u`, the same channel list the itcnbd.live player uses), any remote playlist URL (iptv-org, mirrors, your server), or a bundled local file. Multi-source catalogs with automatic merge/dedupe and alternative-stream resolution.
- **Search** — case-insensitive, multi-token, category-aware and typo-tolerant ranking (`"t spor"` → *T Sports HD*).
- **Play** — resolve the exact HLS/DASH/TS stream URL **plus the HTTP headers** (User-Agent / Referer) that BDIX CDNs require, and hand them straight to ExoPlayer / AVPlayer / media_kit / mpv.

Built from day one for FFI embedding into **Flutter (dart:ffi)**, **Android/Kotlin (JNI)**, iOS/Swift, desktops and servers — JSON in, JSON out across the boundary, blocking-free for the UI thread.

```
┌──────────────┐     ┌─────────────────────────────────────────────┐
│ Flutter / KT │     │                 TvCore (Rust)               │
│  dart:ffi /  │◄────┤  tvcore-ffi  ── C ABI, JSON in / JSON out    │
│    JNI / C   │     │  tvcore      ── catalog · search · play      │
└──────────────┘     │  providers   ── itcnbd · any M3U · local     │
                     └─────────────────────────────────────────────┘
```

---

## Repository layout

| Path | What it is |
| --- | --- |
| `tvcore/` | The core library crate (pure Rust, no unsafe). |
| `tvcore-ffi/` | C ABI bindings crate → `libtvcore.so` / `libtvcore.a` / `libtvcore.dylib`; ships `include/tvcore.h` + a runnable C demo (`examples/demo.c`). |
| `tvcore-cli/` | Command-line front-end: `tvcore-cli list / categories / search / play / probe`. |
| `assets/demo_playlist.m3u` | Bundled offline snapshot for demos and smoke tests. |
| `docs/` | Platform integration guides (Flutter, Kotlin), architecture, Rust API reference. |

## Quick start (Rust)

```toml
[dependencies]
tvcore = "1.0.0"
```

```rust
use tvcore::TvCore;

let mut tv = TvCore::new();

// 1) LIST — official ITCN BD playlist
tv.load_itcnbd()?;                       // or load_playlist_url / _file / _str

for (cat, count) in tv.categories() {
    println!("{cat}: {count}");
}

// 2) SEARCH — ranked, typo-tolerant
for ch in tv.search("t spor", 20) {
    println!("{} [{}] -> {}", ch.name, ch.category, ch.url);
}

// 3) PLAY — everything the player needs
let target = tv.play("t-sports-hd")?;    // id, or play_any("t sports") by name
println!("{} {} {:?}", target.url, target.kind.as_str(), target.headers);
```

The built-in HTTP client is blocking — call these from a worker thread / background isolate / `Dispatchers.IO`. TvCore is `Send + Sync`, so sharing one handle across threads is fine.

## Quick start (CLI)

```sh
cargo run -p tvcore-cli -- list --category sports
cargo run -p tvcore-cli -- search "somoy"
cargo run -p tvcore-cli -- play "t sports" --probe

# demo without any network:
cargo run -p tvcore-cli -- --offline categories

# any other playlist source:
cargo run -p tvcore-cli -- --source https://iptv-org.github.io/iptv/countries/bd.m3u search news
```

## Quick start (Flutter / Kotlin / C)

1. Build the shared library for your target:

```sh
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o android/app/src/main/jniLibs build -p tvcore-ffi
cargo build -p tvcore-ffi --release   # desktop: libtvcore.so / tvcore.dll
```

2. Load it and call the C ABI (full copy-paste guides):

- **Flutter:** [`docs/FLUTTER.md`](docs/FLUTTER.md) — complete `dart:ffi` bindings + channel grid example.
- **Android / Kotlin:** [`docs/KOTLIN.md`](docs/KOTLIN.md) — JNI wrapper + ExoPlayer playback with headers.
- **C / C++ / Swift:** `tvcore-ffi/include/tvcore.h` and `tvcore-ffi/examples/demo.c`.

The boundary contract is JSON, e.g.:

```json
{"channelId":"t-sports-hd","channelName":"T Sports HD",
 "url":"https://s1.itcnbd.live/T-Sports-HD/tracks-v1a1/playlist.m3u8",
 "kind":"hls",
 "headers":[["User-Agent","Mozilla/5.0 ..."]]}
```

## What the FFI surface looks like

```c
TvCore *tv     = tvcore_new();
int64_t count  = tvcore_load_itcnbd(tv);              // official ITCN BD list
char   *chans  = tvcore_channels_json(tv, NULL, -1);  // or filter by category
char   *hits   = tvcore_search_json(tv, "sports", 50);
char   *target = tvcore_play_json(tv, "t sports");    // id / name / free text
char   *status = tvcore_probe_json(tv, "t-sports-hd"); // is it live?
tvcore_string_free(chans); tvcore_string_free(hits);
tvcore_string_free(target); tvcore_string_free(status);
tvcore_free(tv);
```

## Data sources

| Source | API | Notes |
| --- | --- | --- |
| ITCN BD official playlist | `load_itcnbd()` / `load_itcnbd_from(url)` | `https://s3.itcnbd.live/playlist.m3u` — direct high-quality streams; needs a BDIX/Bangladesh ISP network (the CDN answers 403 from foreign datacenter IPs; TvCore sends browser headers by default). |
| Any remote playlist | `load_playlist_url(url)` | iptv-org, Mrgify-BDIX, your own server, mirrors. |
| Local playlist | `load_playlist_file(path)` | Bundled asset / cache. |
| In-memory text | `load_playlist_str(text, source)` | Offline bundles, your backend's JSON→M3U. |
| Custom provider | implement `Provider` trait | Bring your own scraping/API logic; TvCore handles merge, dedupe, search, play. |

Stream metadata the parser understands: `tvg-id`, `tvg-name`, `tvg-logo`, `group-title`, `#EXTGRP`, `#EXTVLCOPT:http-user-agent/http-referrer`, Kodi pipe options (`url|User-Agent=..|Referer=..`), relative URLs, CRLF, and it refuses HTML error pages instead of creating phantom channels.

## Testing

```sh
cargo test            # 60+ unit & integration tests, fully offline (fixtures)
cargo test -- --ignored   # live tests against s3.itcnbd.live — run from a BD network
cd tvcore-ffi && cc examples/demo.c -o demo -Iinclude -L../target/release -ltvcore && ./demo
```

- Unit tests: M3U parsing edge cases, search ranking/typo tolerance, catalog merge.
- Integration tests: the whole list→search→play→probe pipeline against a realistic fixture snapshot of the ITCN BD playlist data.
- Live tests (`#[ignore]`d by default) hit the real CDN — run them from a Bangladesh ISP network (`cargo test -p tvcore --test live_itcnbd -- --ignored`).
- `tvcore-ffi/examples/demo.c` proves the C ABI end-to-end from C.

## Platform integration guides

| Platform | Guide |
| --- | --- |
| Flutter (Android/iOS/Linux/macOS/Windows) | [`docs/FLUTTER.md`](docs/FLUTTER.md) |
| Android Kotlin / Java | [`docs/KOTLIN.md`](docs/KOTLIN.md) |
| Rust applications | [`docs/API.md`](docs/API.md) |
| Design notes, module map, data flow | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) |

## Notes on itcnbd.live access

`itcnbd.live` and its CDN (`s1./s2./s3.itcnbd.live`) are BDIX services: they serve Bangladeshi ISP networks and return `403` from most foreign/datacenter IPs. TvCore therefore always sends a browser `User-Agent` (and a `Referer: https://itcnbd.live/` for the playlist fetch), and surfaces a clear `HttpStatus(403)` error when access is blocked — so apps can fall back to `load_playlist_url` mirrors or a bundled playlist. The stream URLs themselves (HLS `.m3u8`) play in any standard player once reachable.

## License

MIT © 2026 [defahim00](https://github.com/defahim00). TvCore hosts no content — it only lists and resolves publicly available playlist URLs, exactly like the source website does. Respect the rights of content owners in your deployment.

---

## বাংলায় সংক্ষেপে (Quick summary in Bangla)

TvCore হলো একটা ছোট Rust লাইব্রেরি যেটা দিয়ে যেকোনো অ্যাপ (Flutter / Kotlin / অন্য কিছু) থেকে লাইভ টিভি চ্যানেল **লিস্ট**, **সার্চ** আর **প্লে** করা যায় — মূল সোর্স [itcnbd.live](https://itcnbd.live)-এর অফিসিয়াল প্লেলিস্ট সহ যেকোনো M3U প্লেলিস্ট থেকে। প্লে করার সময় ঠিক URL-এর সাথে দরকারি HTTP হেডার (User-Agent/Referer) ও দিয়ে দেয়, যেগুলো BDIX স্ট্রিমে লাগে। সব রেজাল্ট FFI সীমান্তে JSON আকারে আসে — Flutter-এ `dart:ffi`, Android-এ JNI দিয়ে সহজেই বসানো যায় (`docs/FLUTTER.md`, `docs/KOTLIN.md`)। টেস্ট সম্পূর্ণ অফলাইন ফিক্সচারে চলে; BD নেটওয়ার্ক থেকে `cargo test -- --ignored` চালালে লাইভ টেস্টও যায়। লাইসেন্স MIT।
