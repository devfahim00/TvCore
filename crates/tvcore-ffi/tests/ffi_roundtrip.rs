//! Calls the exported C ABI the same way Dart/Kotlin/Python will.

use serde_json::Value;
use std::ffi::{c_char, CStr, CString};
use tvcore_ffi::*;

const HTML: &str = include_str!("../../tvcore/tests/fixtures/home.html");

fn s(v: &str) -> CString {
    CString::new(v).unwrap()
}

/// Read + free an envelope returned by the library.
unsafe fn take(p: *mut c_char) -> Value {
    assert!(!p.is_null());
    let v: Value = serde_json::from_str(CStr::from_ptr(p).to_str().unwrap()).unwrap();
    tvcore_string_free(p);
    v
}

#[test]
fn full_flow_through_the_c_abi() {
    unsafe {
        let h = tvcore_new(std::ptr::null());
        assert!(!h.is_null());

        let r = take(tvcore_load_html(h, s(HTML).as_ptr()));
        assert_eq!(r["ok"], true);
        assert_eq!(r["data"]["count"], 75);

        let list = take(tvcore_list(h, std::ptr::null()));
        assert_eq!(list["data"].as_array().unwrap().len(), 75);
        let kids = take(tvcore_list(h, s("kids").as_ptr()));
        assert_eq!(kids["data"].as_array().unwrap().len(), 8);

        let cats = take(tvcore_categories(h));
        assert_eq!(cats["data"].as_array().unwrap().len(), 10);

        let hits = take(tvcore_search(h, s("jamuna").as_ptr(), std::ptr::null(), 5));
        assert_eq!(hits["data"][0]["id"], "jamuna-tv");

        let limited = take(tvcore_search(h, s("").as_ptr(), s("SPORTS").as_ptr(), 2));
        assert_eq!(limited["data"].as_array().unwrap().len(), 2);

        let ch = take(tvcore_get(h, s("jamuna-tv").as_ptr()));
        assert_eq!(ch["data"]["title"], "Jamuna TV");

        // "disney" (the s3 one) has no token, so playing it never needs the network
        let play = take(tvcore_play(h, s("disney").as_ptr(), std::ptr::null()));
        assert_eq!(play["ok"], true);
        assert!(play["data"]["url"]
            .as_str()
            .unwrap()
            .contains("s3.itcnbd.live"));
        assert_eq!(play["data"]["headers"]["Referer"], "https://itcnbd.live/");

        let m3u = take(tvcore_export_m3u(h));
        assert!(m3u["data"].as_str().unwrap().starts_with("#EXTM3U"));

        tvcore_free(h);
    }
}

#[test]
fn errors_are_envelopes_not_crashes() {
    unsafe {
        let h = tvcore_new(s("{\"timeout_secs\":1}").as_ptr());
        assert!(!h.is_null());
        take(tvcore_load_html(h, s(HTML).as_ptr()));
        let r = take(tvcore_get(h, s("missing").as_ptr()));
        assert_eq!(r["ok"], false);
        assert_eq!(r["error"]["code"], "not_found");

        let r = take(tvcore_get(h, std::ptr::null()));
        assert_eq!(r["error"]["code"], "invalid_input");

        let r = take(tvcore_play(
            h,
            s("jamuna-tv").as_ptr(),
            s("{bad json").as_ptr(),
        ));
        assert_eq!(r["error"]["code"], "invalid_input");

        let r = take(tvcore_load_html(h, s("<html></html>").as_ptr()));
        assert_eq!(r["error"]["code"], "parse");

        let r = take(tvcore_list(std::ptr::null(), std::ptr::null()));
        assert_eq!(r["error"]["code"], "invalid_input");

        assert!(tvcore_new(s("{not json").as_ptr()).is_null());
        tvcore_free(std::ptr::null_mut());
        tvcore_string_free(std::ptr::null_mut());
        tvcore_free(h);
    }
}

#[test]
fn version_string() {
    unsafe {
        let v = CStr::from_ptr(tvcore_version()).to_str().unwrap();
        assert_eq!(v, env!("CARGO_PKG_VERSION"));
    }
}
