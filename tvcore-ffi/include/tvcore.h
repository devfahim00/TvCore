/*
 * TvCore — live TV channel listing / search / playback core.
 * Plain C ABI shared by Flutter (dart:ffi), Android/Kotlin (JNI),
 * iOS/Swift, and native apps.
 *
 * Version: 1.0.0
 * Repo:    https://github.com/defahim00/TvCore
 *
 * Conventions
 * -----------
 *  - TvCore*         opaque handle; create with tvcore_new, free with
 *                    tvcore_free. Handle is thread-safe (Send + Sync).
 *  - JSON results    every *_json function returns a freshly allocated,
 *                    NUL-terminated UTF-8 string. Release it with
 *                    tvcore_string_free. NULL return = error.
 *  - Numeric returns channel counts on success; -1 on error.
 *  - Errors          call tvcore_last_error() right after a failure to get
 *                    a human-readable message (thread-local).
 *  - Threading       the built-in HTTP layer is blocking. Call these
 *                    functions from a worker thread / background isolate /
 *                    Kotlin coroutine on Dispatchers.IO.
 *
 * Free strings returned by *_json with tvcore_string_free.
 * tvcore_version() and tvcore_last_error() return borrowed pointers —
 * do NOT free them.
 */

#ifndef TVCORE_H
#define TVCORE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque library handle. */
typedef struct TvCore TvCore;

/* ------------------------------------------------------------------ */
/* lifecycle                                                           */
/* ------------------------------------------------------------------ */

/* Create a handle with the default HTTP client (30 s timeout). */
TvCore *tvcore_new(void);

/* Create a handle with a custom request timeout in seconds
 * (<= 0 falls back to the 30 s default). */
TvCore *tvcore_new_with_timeout(int32_t timeout_secs);

/* Release a handle. NULL is a no-op. */
void tvcore_free(TvCore *handle);

/* Library version string, e.g. "1.0.0". Borrowed — do not free. */
const char *tvcore_version(void);

/* Message of the last failed call on the current thread.
 * Borrowed — do not free. Empty string when the last call succeeded. */
const char *tvcore_last_error(void);

/* ------------------------------------------------------------------ */
/* loading                                                             */
/* ------------------------------------------------------------------ */

/* Load the official ITCN BD playlist (https://s3.itcnbd.live/playlist.m3u)
 * — the same channel list the https://itcnbd.live player uses.
 * Returns newly added channel count, or -1 on error. */
int64_t tvcore_load_itcnbd(TvCore *handle);

/* Load any remote M3U/M3U8 playlist URL (iptv-org, mirrors, your server).
 * Returns newly added channel count, or -1 on error. */
int64_t tvcore_load_playlist_url(TvCore *handle, const char *url);

/* Load a playlist file from the filesystem (bundled asset / cache).
 * Returns newly added channel count, or -1 on error. */
int64_t tvcore_load_playlist_file(TvCore *handle, const char *path);

/* Parse an in-memory playlist text (offline bundle / your backend).
 * `source` labels the channels (NULL => "memory").
 * Returns newly added channel count, or -1 on parse error. */
int64_t tvcore_load_playlist_str(TvCore *handle, const char *text,
                                 const char *source);

/* ------------------------------------------------------------------ */
/* listing                                                             */
/* ------------------------------------------------------------------ */

/* All channels as JSON:
 *   {"count":N,"channels":[{"id":"somoy-tv","name":"Somoy TV",
 *     "category":"BANGLA","logo":"https://...","url":"https://...m3u8",
 *     "source":"itcnbd", ...}]}
 *
 * `category`: NULL = all; otherwise a case-insensitive category filter.
 * `limit`   : -1 = all.
 * Returns NULL on error (tvcore_last_error). */
char *tvcore_channels_json(TvCore *handle, const char *category,
                           int64_t limit);

/* Category list with counts as JSON:
 *   {"count":N,"categories":[{"name":"SPORTS","channels":23}, ...]}
 * Sorted by channel count, descending. */
char *tvcore_categories_json(TvCore *handle);

/* ------------------------------------------------------------------ */
/* search                                                              */
/* ------------------------------------------------------------------ */

/* Ranked, case-insensitive, multi-token, typo-tolerant search as JSON:
 *   {"query":"sports","count":N,"channels":[{...channel fields...,
 *                                            "score":131}]}
 * `limit` = -1 for all hits. Returns NULL on error. */
char *tvcore_search_json(TvCore *handle, const char *query, int64_t limit);

/* ------------------------------------------------------------------ */
/* play                                                                */
/* ------------------------------------------------------------------ */

/* Resolve playback for a channel — by id, by exact name, or the best
 * search hit for a free-text key:
 *   {"channelId":"somoy-tv","channelName":"Somoy TV",
 *    "url":"https://s3.itcnbd.live/....m3u8","kind":"hls",
 *    "headers":[["User-Agent","Mozilla/5.0 ..."],["Referer","https://..."]]}
 *
 * Hand `url` to your player (ExoPlayer / AVPlayer / media_kit / mpv) and
 * send `headers` with the request. `kind` is one of "hls","dash","ts",
 * "unknown". Returns NULL on error. */
char *tvcore_play_json(TvCore *handle, const char *key);

/* Resolve alternative stream `index` (0-based) for a channel id —
 * mirrors merged from other sources. Returns NULL on error. */
char *tvcore_play_alternative_json(TvCore *handle, const char *id,
                                   int64_t index);

/* Liveness probe of the channel's primary stream:
 *   {"channelId":"...","httpStatus":200,"reachable":true}
 * Never fails on network errors — httpStatus is 0, reachable false. */
char *tvcore_probe_json(TvCore *handle, const char *id);

/* ------------------------------------------------------------------ */
/* memory                                                              */
/* ------------------------------------------------------------------ */

/* Free a string returned by any *_json function. NULL is a no-op. */
void tvcore_string_free(char *s);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* TVCORE_H */
