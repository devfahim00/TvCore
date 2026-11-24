# Architecture

## Goals

1. One implementation of "list / search / play" for every client (Flutter, Kotlin, Swift, CLI, bots).
2. Small, panic-safe C ABI so any language with FFI can use it.
3. Testable without the live site.

## Crates

| Crate | Responsibility |
|---|---|
| `tvcore` | `parse` (HTML → `Channel`), `search` (ranking), `token` (expiry), `playlist` (HLS analysis), `net` (HTTP, feature `http`), `TvCore` (state + `play`) |
| `tvcore-ffi` | `extern "C"` wrappers. Strings in, JSON envelope out |
| `tvcore-cli` | `tvcore` binary, thin front-end over the library |

## Data flow

```
refresh() ─▶ GET base_url ─▶ parse_channels() ─▶ Vec<Channel> (Mutex in TvCore)
search()  ─▶ rank in memory (no network)
play(id)  ─▶ get(id) ─▶ token expired? ─▶ refresh() ─▶ [verify? probe() ─▶ 401/403? ─▶ /api/stream-url ─▶ refresh()] ─▶ PlayInfo
```

## Decisions

* **JSON across the FFI** instead of a struct per type. One function signature style, nothing for the caller to free
  except one string type, and the shape can grow without breaking the ABI. The cost is a parse on the client side, which is negligible for 75 channels.
* **Envelope `{ok,data|error}`**: errors never need a second channel (no `errno`, no out parameters).
* **Blocking API**: networking is synchronous (`ureq`, rustls). It keeps the ABI simple and avoids shipping an async runtime
  to mobile; the host runs calls on a background thread/isolate/coroutine.
* **`http` feature**: with `--no-default-features` there is no TLS/HTTP code in the binary. The host downloads the page and calls `load_html`.
  `play()` then returns `expired: true` instead of refreshing.
* **Clock injection**: `play_at(id, opts, now)` makes expiry logic deterministic in tests.
* **Stable ids**: slug of the title. Chosen over the stream path because the path/token change, while titles rarely do.
* **No analytics calls**: the library only reads. It does not post to `/api/log-play` or `/api/heartbeat`.

## Thread safety

`TvCore` holds its list behind a `Mutex`; the handle can be shared across threads. A long network call does not hold
the lock (the lock is only taken to read/replace the list).

## Extending

* New field on the page → add it to `Channel`, `parse.rs`, a fixture assertion; JSON consumers ignore unknown fields.
* New source site → add a module next to `parse.rs` producing `Vec<Channel>`; `search`, `play` and the FFI stay unchanged.
