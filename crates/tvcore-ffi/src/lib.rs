//! C ABI for `tvcore`.
//!
//! Conventions
//! * Every function that returns `char*` returns a JSON **envelope**:
//!   `{"ok":true,"data":...}` or `{"ok":false,"error":{"code":"...","message":"..."}}`.
//! * Returned strings are owned by Rust: free them with [`tvcore_string_free`].
//! * Input strings are UTF-8, NUL terminated, and only borrowed for the duration of the call.
//! * A `TvCore*` handle is thread-safe. Network calls block, so call them off the UI thread.
//! * Panics never cross the boundary: they become `{"code":"panic"}`.

use serde::Serialize;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use tvcore::{Config, Error, PlayOptions, SearchQuery, TvCore};

fn into_c(s: String) -> *mut c_char {
    // JSON never contains interior NULs (they are escaped), but stay safe anyway.
    CString::new(s.replace('\0', ""))
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

fn ok<T: Serialize>(data: T) -> String {
    json!({ "ok": true, "data": data }).to_string()
}

fn err(code: &str, message: impl ToString) -> String {
    json!({ "ok": false, "error": { "code": code, "message": message.to_string() } }).to_string()
}

/// Borrow an optional C string. `Err` when it is not valid UTF-8.
unsafe fn opt_str(p: *const c_char) -> Result<Option<String>, Error> {
    if p.is_null() {
        return Ok(None);
    }
    CStr::from_ptr(p)
        .to_str()
        .map(|s| Some(s.to_owned()))
        .map_err(|_| Error::Invalid("string is not valid UTF-8".into()))
}

unsafe fn req_str(p: *const c_char, what: &str) -> Result<String, Error> {
    opt_str(p)?.ok_or_else(|| Error::Invalid(format!("{what} must not be null")))
}

/// Run `f` against the handle, converting panics / null handles into error envelopes.
unsafe fn call<F>(handle: *const TvCore, f: F) -> *mut c_char
where
    F: FnOnce(&TvCore) -> Result<Value, Error>,
{
    if handle.is_null() {
        return into_c(err("invalid_input", "handle is null"));
    }
    let core = &*handle;
    let out = catch_unwind(AssertUnwindSafe(|| match f(core) {
        Ok(v) => ok(v),
        Err(e) => err(e.code(), e),
    }))
    .unwrap_or_else(|_| err("panic", "internal panic"));
    into_c(out)
}

fn to_value<T: Serialize>(v: T) -> Result<Value, Error> {
    serde_json::to_value(v).map_err(|e| Error::Parse(e.to_string()))
}

/// Library version, e.g. `"0.1.0"`. Static string: do **not** free it.
#[no_mangle]
pub extern "C" fn tvcore_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Create a handle. `config_json` may be NULL (defaults) or a JSON object such as
/// `{"base_url":"https://itcnbd.live/","timeout_secs":10}`. Returns NULL on invalid JSON.
/// Release with [`tvcore_free`].
#[no_mangle]
pub unsafe extern "C" fn tvcore_new(config_json: *const c_char) -> *mut TvCore {
    let r = catch_unwind(AssertUnwindSafe(|| {
        let cfg = match opt_str(config_json) {
            Ok(Some(s)) if !s.trim().is_empty() => serde_json::from_str::<Config>(&s).ok()?,
            Ok(_) => Config::default(),
            Err(_) => return None,
        };
        Some(Box::into_raw(Box::new(TvCore::new(cfg))))
    }));
    r.ok().flatten().unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn tvcore_free(handle: *mut TvCore) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

#[no_mangle]
pub unsafe extern "C" fn tvcore_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Fetch the site and rebuild the list. data: `{"count": n}`. Blocks on the network.
#[no_mangle]
pub unsafe extern "C" fn tvcore_refresh(handle: *const TvCore) -> *mut c_char {
    call(handle, |c| c.refresh().map(|n| json!({ "count": n })))
}

/// Build the list from HTML you downloaded yourself (no networking). data: `{"count": n}`.
#[no_mangle]
pub unsafe extern "C" fn tvcore_load_html(
    handle: *const TvCore,
    html: *const c_char,
) -> *mut c_char {
    call(handle, |c| {
        let html = req_str(html, "html")?;
        c.load_html(&html).map(|n| json!({ "count": n }))
    })
}

/// data: `[Channel]`. `category` may be NULL or empty for all.
#[no_mangle]
pub unsafe extern "C" fn tvcore_list(
    handle: *const TvCore,
    category: *const c_char,
) -> *mut c_char {
    call(handle, |c| {
        let cat = opt_str(category)?
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        let list: Vec<_> = c
            .channels()?
            .into_iter()
            .filter(|ch| {
                cat.as_ref()
                    .map_or(true, |k| ch.category.to_lowercase() == *k)
            })
            .collect();
        to_value(list)
    })
}

/// data: `[{"name","count"}]`.
#[no_mangle]
pub unsafe extern "C" fn tvcore_categories(handle: *const TvCore) -> *mut c_char {
    call(handle, |c| c.categories().and_then(to_value))
}

/// data: `[Channel]` ranked by relevance. `query`/`category` may be NULL; `limit <= 0` = unlimited.
#[no_mangle]
pub unsafe extern "C" fn tvcore_search(
    handle: *const TvCore,
    query: *const c_char,
    category: *const c_char,
    limit: i32,
) -> *mut c_char {
    call(handle, |c| {
        let q = SearchQuery {
            query: opt_str(query)?.unwrap_or_default(),
            category: opt_str(category)?,
            limit: (limit > 0).then_some(limit as usize),
        };
        to_value(c.search(&q))
    })
}

/// data: `Channel`.
#[no_mangle]
pub unsafe extern "C" fn tvcore_get(handle: *const TvCore, id: *const c_char) -> *mut c_char {
    call(handle, |c| c.get(&req_str(id, "id")?).and_then(to_value))
}

/// data: `PlayInfo` (url + headers). `options_json` may be NULL or e.g. `{"verify":true}`.
/// Re-fetches the page when the token is (almost) expired.
#[no_mangle]
pub unsafe extern "C" fn tvcore_play(
    handle: *const TvCore,
    id: *const c_char,
    options_json: *const c_char,
) -> *mut c_char {
    call(handle, |c| {
        let opts = match opt_str(options_json)? {
            Some(s) if !s.trim().is_empty() => serde_json::from_str::<PlayOptions>(&s)
                .map_err(|e| Error::Invalid(format!("bad options: {e}")))?,
            _ => PlayOptions::default(),
        };
        c.play(&req_str(id, "id")?, &opts).and_then(to_value)
    })
}

/// data: `ProbeResult`. Downloads the playlist and lists variants.
#[no_mangle]
pub unsafe extern "C" fn tvcore_probe(handle: *const TvCore, url: *const c_char) -> *mut c_char {
    call(handle, |c| {
        c.probe(&req_str(url, "url")?).and_then(to_value)
    })
}

/// data: the M3U text as a JSON string.
#[no_mangle]
pub unsafe extern "C" fn tvcore_export_m3u(handle: *const TvCore) -> *mut c_char {
    call(handle, |c| c.export_m3u().map(Value::String))
}
