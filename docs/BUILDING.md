# Building

Requires Rust 1.75+ (developed with 1.97). Linux/macOS/Windows hosts.

```bash
cargo build --release                     # target/release/{tvcore, libtvcore_ffi.so|.dylib|.dll, libtvcore_ffi.a}
cargo test --workspace                    # unit + fixture + mock-HTTP + FFI tests
python3 bindings/python/test_tvcore.py    # needs the release build above
cargo build --release -p tvcore-ffi --no-default-features   # see "no HTTP" below
```

## Android (.so for Flutter / Kotlin)

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk                   # needs the Android NDK (ANDROID_NDK_HOME)
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o android/app/src/main/jniLibs build --release -p tvcore-ffi
```

This writes `jniLibs/<abi>/libtvcore_ffi.so`. For Flutter put them in `android/app/src/main/jniLibs/`.

## iOS

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo build --release -p tvcore-ffi --target aarch64-apple-ios
# link target/aarch64-apple-ios/release/libtvcore_ffi.a into the Runner; header: crates/tvcore-ffi/include/tvcore.h
```

## Desktop

Use `libtvcore_ffi.so` / `.dylib` / `tvcore_ffi.dll` from `target/release`. Flutter desktop: bundle it next to the executable.

## No HTTP (host does networking)

`--no-default-features` removes `ureq`/rustls. `refresh`, `probe`, `refresh_stream_url` then return error code `no_network`;
use `load_html` and your own HTTP client. Smaller binary, no TLS stack to audit.

## Size

Release profile uses `opt-level="z"`, LTO and strip. The shared library is about 3.4 MB on x86_64 Linux with HTTP.

## Troubleshooting

* `parse` error on refresh: the site changed or served a block page, see [SITE_PROTOCOL.md](SITE_PROTOCOL.md).
* `status` 403 on `play(verify)`: token rejected; the library already retried with a fresh token, so the stream may be geo/IP restricted.
* Dart `Failed to load dynamic library`: wrong ABI folder, or the file is not named `libtvcore_ffi.so`.
