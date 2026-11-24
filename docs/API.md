# API reference

## Types (JSON field names)

```jsonc
// Channel
{ "id": "jamuna-tv", "title": "Jamuna TV", "category": "BANGLA",
  "logo": "https://itcnbd.live/images/51f9...png",      // or null
  "stream_url": "https://s1.itcnbd.live/JAMUNA/index.m3u8?token=...",
  "expires_at": 1795508732 }                             // unix seconds, or null (no token)

// Category
{ "name": "BANGLA", "count": 23 }

// PlayInfo
{ "id": "jamuna-tv", "title": "Jamuna TV", "category": "BANGLA",
  "url": "https://s1.itcnbd.live/JAMUNA/index.m3u8?token=...",
  "headers": { "Referer": "https://itcnbd.live/", "User-Agent": "..." },
  "expires_at": 1795508732, "refreshed": false, "expired": false, "kind": "hls" }

// ProbeResult
{ "status": 200, "ok": true, "kind": "master",           // master | media | invalid
  "variants": [ { "url": "...", "bandwidth": 800000, "resolution": "640x360" } ] }

// Config (all optional)
{ "base_url": "https://itcnbd.live/", "user_agent": "...", "timeout_secs": 15, "token_skew_secs": 120 }

// PlayOptions (all optional)
{ "verify": false, "force_refresh": false }
```

## Rust (`tvcore`)

| Item | Description |
|---|---|
| `TvCore::new(Config)` | create a client |
| `refresh() -> Result<usize>` | fetch + parse the page, replace the list (needs `http`) |
| `load_html(&str) -> Result<usize>` | parse HTML you fetched yourself |
| `channels() -> Result<Vec<Channel>>` | all channels; auto-refreshes the first time |
| `categories() -> Result<Vec<Category>>` | counts, in order of appearance |
| `get(id) -> Result<Channel>` | by id (case-insensitive) |
| `search(&SearchQuery) -> Vec<Channel>` | ranked; empty query = everything |
| `play(id, &PlayOptions) -> Result<PlayInfo>` | refreshes when the token expires within `token_skew_secs` |
| `play_at(id, opts, now)` | same with an explicit clock |
| `probe(url) -> Result<ProbeResult>` | fetch playlist, classify, list variants |
| `refresh_stream_url(url) -> Result<String>` | ask `/api/stream-url` for a new signed URL |
| `export_m3u() -> Result<String>` | M3U with `group-title`, `tvg-logo`, referrer/user-agent |

### Search ranking

Case, spaces and punctuation are ignored. Order of scores: exact title (100) > title prefix (85) > word prefix (70) >
substring (55) > all query words present (45) > category match (30) > fuzzy subsequence, 3+ chars (15). Ties keep page order.

## C ABI (`tvcore.h`)

| Function | Returns `data` |
|---|---|
| `tvcore_new(config_json)` | handle (NULL on invalid JSON) |
| `tvcore_free(h)`, `tvcore_string_free(s)` | – |
| `tvcore_version()` | static string, do not free |
| `tvcore_refresh(h)` / `tvcore_load_html(h, html)` | `{"count": n}` |
| `tvcore_list(h, category)` | `[Channel]` |
| `tvcore_categories(h)` | `[Category]` |
| `tvcore_search(h, query, category, limit)` | `[Channel]` (`limit <= 0` = no limit) |
| `tvcore_get(h, id)` | `Channel` |
| `tvcore_play(h, id, options_json)` | `PlayInfo` |
| `tvcore_probe(h, url)` | `ProbeResult` |
| `tvcore_export_m3u(h)` | string |

Envelope: `{"ok":true,"data":...}` or `{"ok":false,"error":{"code":"...","message":"..."}}`.
Returned `char*` must be released with `tvcore_string_free`. Inputs are UTF-8 and only borrowed during the call.

### Error codes

| code | meaning |
|---|---|
| `parse` | page had no channels / unexpected response body |
| `network` | connection, TLS, timeout, or stream is not a valid playlist |
| `status` | non-success HTTP status (message has the number) |
| `not_found` | unknown channel id |
| `invalid_input` | null/invalid argument, bad JSON options |
| `no_network` | built without the `http` feature |
| `panic` | internal bug, call did not complete |
