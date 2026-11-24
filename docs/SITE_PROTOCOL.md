# How itcnbd.live exposes channels

Everything here was derived from a saved copy of `https://itcnbd.live/` (see `crates/tvcore/tests/fixtures/home.html`).

## Channel list

The home page is server-rendered. There is no JSON list endpoint; every channel is one element:

```html
<div class="channel-card ..."
     data-title="Jamuna TV"
     data-category="BANGLA"
     data-link="https://s1.itcnbd.live/JAMUNA/index.m3u8?token=<hash>-<hash>-<expiry>-<issued>&amp;remote=no_check_ip"
     onclick="playChannel(...)">
  <img src="/images/51f904786ddd9fb9ff0760c5c538ee71.png" ...>
</div>
```

* `data-title`, `data-category`, `data-link` are the data TvCore reads; the first `<img src>` inside is the logo.
* Logos are either absolute (`https://itcnbd.live/images/...`) or root relative (`/images/...`, sometimes with spaces).
  TvCore absolutizes and percent-encodes them.
* Categories seen: `SPORTS BANGLA "INDIAN BANGLA" HINDI MUSIC KIDS ISLAMIC ENGLISH DOCUMENTARY "ENGLISH NEWS"`.
  The category chips and the search box in the page are client-side filters over these same cards.
* Titles are not unique (two `Disney`). TvCore ids are slugs of the title with `-2`, `-3` on repeats, in page order.
  If the site reorders duplicates, their ids swap; everything else is stable.

## Streams

* HLS (`.m3u8`), played in the browser with hls.js.
* Most links point at `s1.itcnbd.live` and carry a signed `token`. Format of the token value:
  `<40 hex>-<16 hex>-<expiry unix>-<issued unix>`; `expiry - issued = 14400 s` (4 hours).
  TvCore takes the larger of the last two numbers as `expires_at`.
* A few channels (`Disney` on `s3.itcnbd.live/server-6/stream/<base64>.m3u8`) have no token.
* The query string `remote=no_check_ip` is part of the signed URL; keep it as is.

## Refreshing an expired link

1. Re-fetch the home page: every card has a freshly signed link (what `refresh()` does).
2. The page's own player, on a 401/403 from `s1.itcnbd.live`, calls
   `GET /api/stream-url?url=<urlencoded old url>` and expects `{"url": "<new url>"}`.
   TvCore tries this first when `play(verify=true)` sees a 401/403, then falls back to (1).

## Other endpoints used by the web page (not used by TvCore)

* `POST /api/log-play` (analytics), `POST /api/heartbeat` ("N online" badge), `/links`, `/v2`, two Android APK downloads.

## When the page changes

`parse_channels` fails with code `parse` when no `.channel-card` with `data-title` and `data-link` exists. Typical causes:
the markup changed, or the site served a bot-check/blocked page. Update `crates/tvcore/src/parse.rs` and refresh the
fixture, then run `cargo test`.
