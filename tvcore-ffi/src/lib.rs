//! TvCore FFI — a plain C ABI over the [`tvcore`] core.
//!
//! Design rules (they make embedding from Flutter/Kotlin/Swift painless):
//!
//! * **JSON in, JSON out** — every result function returns a
//!   NUL-terminated UTF-8 JSON string. Every mainstream platform has a
//!   built-in JSON decoder, so no struct-layout coupling.
//! * **Opaque handle** — `TvCore*` is created with [`tvcore_new`] and
//!   released with [`tvcore_free`]; pass it to the worker thread you like,
//!   the core is `Send + Sync`.
//! * **Explicit lifetime** — strings returned by the `*_json` functions are
//!   heap-allocated; free them with [`tvcore_string_free`].
//!   [`tvcore_version`] and [`tvcore_last_error`] return borrowed statics.
//! * **Two-channel error reporting** — numeric returns are `-1` on error and
//!   string returns are `NULL` on error; the human-readable reason is always
//!   available through [`tvcore_last_error`] (thread-local, per FFI call).
//!
//! The matching header ships in `include/tvcore.h`; full platform guides
//! with copy-paste bindings live in `docs/FLUTTER.md` and `docs/KOTLIN.md`.

#![allow(clippy::missing_safety_doc)] // C ABI: documented in include/tvcore.h
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use tvcore::http::UreqFetcher;
use tvcore::search::search_channels;
use tvcore::{Channel, TvCore};

// ---------------------------------------------------------------------------
// handle & error plumbing
// ---------------------------------------------------------------------------

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(msg: String) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = CString::new(msg).ok();
    });
}

fn set_err<T>(r: Result<T, tvcore::TvError>) -> Result<T, ()> {
    match r {
        Ok(v) => Ok(v),
        Err(e) => {
            set_last_error(e.to_string());
            Err(())
        }
    }
}

/// Read a C string into an owned UTF-8 String (lossy, never panics).
unsafe fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(ptr) }.to_bytes();
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Leak a Rust `String` into a C-compatible heap string the caller owns.
fn into_c_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

// ---------------------------------------------------------------------------
// exported API
// ---------------------------------------------------------------------------

/// Create a TvCore handle with the default (ureq) HTTP layer.
///
/// Returns NULL on error — see [`tvcore_last_error`].
#[no_mangle]
pub unsafe extern "C" fn tvcore_new() -> *mut TvCore {
    let boxed = Box::new(TvCore::new());
    Box::into_raw(boxed)
}

/// Create a TvCore handle that uses its own `ureq` client with a custom
/// request timeout (seconds). `timeout_secs <= 0` means the 30s default.
#[no_mangle]
pub unsafe extern "C" fn tvcore_new_with_timeout(timeout_secs: i32) -> *mut TvCore {
    let timeout = if timeout_secs <= 0 {
        std::time::Duration::from_secs(30)
    } else {
        std::time::Duration::from_secs(timeout_secs as u64)
    };
    let fetcher = UreqFetcher::with_timeout_and_headers(timeout, tvcore::http::default_headers());
    let boxed = Box::new(TvCore::with_http(Arc::new(fetcher)));
    Box::into_raw(boxed)
}

/// Release a TvCore handle. Passing NULL is a no-op.
#[no_mangle]
pub unsafe extern "C" fn tvcore_free(handle: *mut TvCore) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Library version, e.g. `"1.0.0"`. Borrowed static — do not free.
#[no_mangle]
pub extern "C" fn tvcore_version() -> *const c_char {
    // compile-time string literal — safe and stable for the process lifetime
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Human-readable reason of the last failed call on this thread.
/// Borrowed static per thread — do not free. Empty string if no error.
#[no_mangle]
pub extern "C" fn tvcore_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| {
        let b = cell.borrow_mut();
        b.as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(c"".as_ptr() as *const c_char)
    })
}

/// Load the official ITCN BD playlist into the catalog.
/// Returns the number of newly added channels, or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn tvcore_load_itcnbd(handle: *mut TvCore) -> i64 {
    let tv = unsafe { &mut *handle };
    match set_err(tv.load_itcnbd()) {
        Ok(n) => n as i64,
        Err(()) => -1,
    }
}

/// Load a remote M3U/M3U8 playlist URL (iptv-org, mirrors, your server).
/// `url` must be NUL-terminated UTF-8. Returns new channel count or -1.
#[no_mangle]
pub unsafe extern "C" fn tvcore_load_playlist_url(handle: *mut TvCore, url: *const c_char) -> i64 {
    let tv = unsafe { &mut *handle };
    let Some(url) = (unsafe { cstr_to_string(url) }) else {
        set_last_error("url is NULL".into());
        return -1;
    };
    match set_err(tv.load_playlist_url(&url)) {
        Ok(n) => n as i64,
        Err(()) => -1,
    }
}

/// Load a playlist file from the filesystem (bundled asset, cache).
/// Returns new channel count or -1.
#[no_mangle]
pub unsafe extern "C" fn tvcore_load_playlist_file(
    handle: *mut TvCore,
    path: *const c_char,
) -> i64 {
    let tv = unsafe { &mut *handle };
    let Some(path) = (unsafe { cstr_to_string(path) }) else {
        set_last_error("path is NULL".into());
        return -1;
    };
    match set_err(tv.load_playlist_file(&path)) {
        Ok(n) => n as i64,
        Err(()) => -1,
    }
}

/// Parse an in-memory playlist text (offline bundles, your own backend).
/// `source` labels the channels; NULL means `"memory"`.
/// Returns the number of newly added channels or -1 on parse error.
#[no_mangle]
pub unsafe extern "C" fn tvcore_load_playlist_str(
    handle: *mut TvCore,
    text: *const c_char,
    source: *const c_char,
) -> i64 {
    let tv = unsafe { &mut *handle };
    let Some(text) = (unsafe { cstr_to_string(text) }) else {
        set_last_error("text is NULL".into());
        return -1;
    };
    let source = (unsafe { cstr_to_string(source) }).unwrap_or_else(|| "memory".into());
    match tvcore::m3u::parse_m3u(&text, &source, None) {
        Ok(chans) => {
            let n = chans.len() as i64;
            tv.set_channels(chans);
            n
        }
        Err(e) => {
            set_last_error(e.to_string());
            -1
        }
    }
}

/// All channels as JSON: `{"count":N,"channels":[{...}]}`
///
/// * `category` — NULL for all categories, otherwise a case-insensitive
///   filter (e.g. `"SPORTS"`)
/// * `limit` — `-1` for all channels
///
/// Returns NULL on error (see [`tvcore_last_error`]).
#[no_mangle]
pub unsafe extern "C" fn tvcore_channels_json(
    handle: *mut TvCore,
    category: *const c_char,
    limit: i64,
) -> *mut c_char {
    let tv = unsafe { &*handle };
    let cat_opt = unsafe { cstr_to_string(category) };
    let limit = if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    };

    let chans: Vec<Channel> = match cat_opt {
        Some(cat) if !cat.trim().is_empty() => {
            tv.channels_in_category(&cat).into_iter().cloned().collect()
        }
        _ => tv.channels().to_vec(),
    };
    let truncated: Vec<Channel> = chans.into_iter().take(limit).collect();
    let json = serde_json::json!({
        "count": truncated.len(),
        "channels": truncated,
    });
    into_c_string(json.to_string())
}

/// Category list with counts as JSON:
/// `{"count":N,"categories":[{"name":"SPORTS","channels":23},...]}`
#[no_mangle]
pub unsafe extern "C" fn tvcore_categories_json(handle: *mut TvCore) -> *mut c_char {
    let tv = unsafe { &*handle };
    let cats = tv.categories();
    let json = serde_json::json!({
        "count": cats.len(),
        "categories": cats.into_iter()
            .map(|(name, count)| serde_json::json!({"name": name, "channels": count}))
            .collect::<Vec<_>>(),
    });
    into_c_string(json.to_string())
}

/// Ranked search as JSON:
/// `{"query":"sports","count":N,"channels":[{... channel fields ..., "score":131}]}`
///
/// `limit` = `-1` for all hits. NULL on error.
#[no_mangle]
pub unsafe extern "C" fn tvcore_search_json(
    handle: *mut TvCore,
    query: *const c_char,
    limit: i64,
) -> *mut c_char {
    let tv = unsafe { &*handle };
    let Some(query) = (unsafe { cstr_to_string(query) }) else {
        set_last_error("query is NULL".into());
        return std::ptr::null_mut();
    };
    let limit = if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    };
    let hits = search_channels(tv.channels(), &query, limit);
    let mut out = Vec::with_capacity(hits.len());
    for h in &hits {
        let ch = &tv.channels()[h.index];
        let mut v = serde_json::to_value(ch).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = v.as_object_mut() {
            obj.insert("score".into(), serde_json::json!(h.score));
        }
        out.push(v);
    }
    let json = serde_json::json!({
        "query": query,
        "count": out.len(),
        "channels": out,
    });
    into_c_string(json.to_string())
}

/// Resolve playback for a channel, by id or by name (falls back to the
/// best search hit). JSON payload (camelCase):
///
/// ```json
/// {
///   "channelId": "somoy-tv",
///   "channelName": "Somoy TV",
///   "url": "https://s3.itcnbd.live/...m3u8",
///   "kind": "hls",
///   "headers": [["User-Agent", "Mozilla/5.0 ..."]]
/// }
/// ```
#[no_mangle]
pub unsafe extern "C" fn tvcore_play_json(handle: *mut TvCore, key: *const c_char) -> *mut c_char {
    let tv = unsafe { &*handle };
    let Some(key) = (unsafe { cstr_to_string(key) }) else {
        set_last_error("channel key is NULL".into());
        return std::ptr::null_mut();
    };
    match set_err(tv.play_any(&key)) {
        Ok(target) => match serde_json::to_string(&target) {
            Ok(s) => into_c_string(s),
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        },
        Err(()) => std::ptr::null_mut(),
    }
}

/// Resolve alternative stream `index` (0-based) of a channel id.
#[no_mangle]
pub unsafe extern "C" fn tvcore_play_alternative_json(
    handle: *mut TvCore,
    id: *const c_char,
    index: i64,
) -> *mut c_char {
    let tv = unsafe { &*handle };
    let Some(id) = (unsafe { cstr_to_string(id) }) else {
        set_last_error("channel id is NULL".into());
        return std::ptr::null_mut();
    };
    if index < 0 {
        set_last_error("alternative index must be >= 0".into());
        return std::ptr::null_mut();
    }
    match set_err(tv.play_alternative(&id, index as usize)) {
        Ok(target) => match serde_json::to_string(&target) {
            Ok(s) => into_c_string(s),
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        },
        Err(()) => std::ptr::null_mut(),
    }
}

/// Probe the channel's stream: `{"channelId":"...","httpStatus":200,"reachable":true}`
#[no_mangle]
pub unsafe extern "C" fn tvcore_probe_json(handle: *mut TvCore, id: *const c_char) -> *mut c_char {
    let tv = unsafe { &*handle };
    let Some(id) = (unsafe { cstr_to_string(id) }) else {
        set_last_error("channel id is NULL".into());
        return std::ptr::null_mut();
    };
    match set_err(tv.probe(&id)) {
        Ok(status) => match serde_json::to_string(&status) {
            Ok(s) => into_c_string(s),
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        },
        Err(()) => std::ptr::null_mut(),
    }
}

/// Free a string returned by the `*_json` functions. NULL is a no-op.
#[no_mangle]
pub unsafe extern "C" fn tvcore_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // The FFI surface is exercised end-to-end from C in
    // tvcore-ffi/examples/demo.c; these Rust tests cover the ABI basics.

    #[test]
    fn version_string_is_sane() {
        let v = unsafe { std::ffi::CStr::from_ptr(tvcore_version()) }
            .to_string_lossy()
            .to_string();
        assert!(v.chars().next().unwrap().is_ascii_digit());
    }

    #[test]
    fn last_error_starts_empty() {
        let e = unsafe { std::ffi::CStr::from_ptr(tvcore_last_error()) };
        assert!(e.to_bytes().is_empty());
    }

    #[test]
    fn new_free_roundtrip() {
        for _ in 0..100 {
            let h = unsafe { tvcore_new() };
            assert!(!h.is_null());
            unsafe { tvcore_free(h) };
        }
        // double free of NULL is a no-op
        unsafe { tvcore_free(std::ptr::null_mut()) };
    }

    #[test]
    fn load_str_and_play_json() {
        let h = unsafe { tvcore_new() };
        let text = CString::new(
            "#EXTM3U\n#EXTINF:-1 group-title=\"BANGLA\",Somoy TV\nhttps://x/somoy.m3u8\n",
        )
        .unwrap();
        let src = CString::new("test").unwrap();
        let n = unsafe { tvcore_load_playlist_str(h, text.as_ptr(), src.as_ptr()) };
        assert_eq!(n, 1);

        let key = CString::new("somoy tv").unwrap();
        let json_ptr = unsafe { tvcore_play_json(h, key.as_ptr()) };
        assert!(!json_ptr.is_null());
        let json = unsafe { CStr::from_ptr(json_ptr) }
            .to_string_lossy()
            .to_string();
        assert!(json.contains("\"channelId\":\"somoy-tv\""));
        assert!(json.contains("\"kind\":\"hls\""));
        unsafe { tvcore_string_free(json_ptr) };

        // unknown channel -> NULL + last error set
        let bad = CString::new("nope-zzz").unwrap();
        let null = unsafe { tvcore_play_json(h, bad.as_ptr()) };
        assert!(null.is_null());
        let err = unsafe { CStr::from_ptr(tvcore_last_error()) };
        assert!(err.to_string_lossy().contains("no channel"));

        unsafe { tvcore_free(h) };
    }

    #[test]
    fn null_arguments_are_handled() {
        let h = unsafe { tvcore_new() };
        assert_eq!(unsafe { tvcore_load_playlist_url(h, std::ptr::null()) }, -1);
        assert!(unsafe { tvcore_play_json(h, std::ptr::null()) }.is_null());
        assert!(unsafe { tvcore_search_json(h, std::ptr::null(), 5) }.is_null());
        unsafe { tvcore_free(h) };
    }
}
