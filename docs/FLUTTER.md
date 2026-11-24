# Flutter integration guide

TvCore ships a C ABI (`libtvcore.so` / `libtvcore.dylib` / `tvcore.dll` / `libtvcore.a`), so Flutter reaches it with plain `dart:ffi` — no platform channels, no codegen, JSON in / JSON out.

This guide gives you a **complete, copy-paste integration**:

1. [Build the library for your targets](#1-build-the-library)
2. [`lib/tvcore.dart` — full bindings file](#2-the-bindings-file)
3. [Using it in a widget](#3-using-it-in-a-widget)
4. [Playing the stream](#4-playing-the-stream)
5. [Threading rules](#5-threading-rules)
6. [Troubleshooting](#6-troubleshooting)

---

## 1. Build the library

Install [cargo-ndk](https://github.com/rust-mobile/cargo-ndk):

```sh
cargo install cargo-ndk
rustup target add arm64-v8a armeabi-v7a x86_64  # android targets
```

From the TvCore repo root:

```sh
# Android — drops .so files straight into your app
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o <your_flutter_app>/android/app/src/main/jniLibs \
  build --release -p tvcore-ffi
```

Desktops:

```sh
cargo build --release -p tvcore-ffi
# Linux:   target/release/libtvcore.so
# Windows: target/release/tvcore.dll
# macOS:   target/release/libtvcore.dylib
```

iOS (static library, linked into the Runner):

```sh
rustup target add aarch64-apple-ios
cargo build --release -p tvcore-ffi --target aarch64-apple-ios
cp target/aarch64-apple-ios/release/libtvcore.a <app>/ios/Runner/
# Xcode: add the .a + the include/tvcore.h header to Runner, link "libc++".
```

Windows nuance: load with `tvcore.dll` name on Windows and `libtvcore.so`/`libtvcore.dylib` elsewhere (handled in the file below).

---

## 2. The bindings file

Create `lib/tvcore.dart` in your Flutter app — this is the whole wrapper, no codegen:

```dart
import 'dart:convert';
import 'dart:ffi';
import 'dart:io' show Platform;

import 'package:ffi/ffi.dart';

// ---------------------------------------------------------------------------
// low-level C ABI declarations (mirrors include/tvcore.h)
// ---------------------------------------------------------------------------

final _lib = Platform.isWindows
    ? DynamicLibrary.open('tvcore.dll')
    : Platform.isMacOS
        ? DynamicLibrary.open('libtvcore.dylib')
        : DynamicLibrary.open('libtvcore.so');

typedef _NewC = Pointer<Void> Function();
typedef _NewDart = Pointer<Void> Function();
final _new = _lib.lookupFunction<_NewC, _NewDart>('tvcore_new');

typedef _FreeC = Void Function(Pointer<Void>);
typedef _FreeDart = void Function(Pointer<Void>);
final _free = _lib.lookupFunction<_FreeC, _FreeDart>('tvcore_free');

typedef _VersionC = Pointer<Utf8> Function();
typedef _VersionDart = Pointer<Utf8> Function();
final _version = _lib.lookupFunction<_VersionC, _VersionDart>('tvcore_version');

typedef _LoadItcnbdC = Int64 Function(Pointer<Void>);
typedef _LoadItcnbdDart = int Function(Pointer<Void>);
final _loadItcnbd = _lib.lookupFunction<_LoadItcnbdC, _LoadItcnbdDart>('tvcore_load_itcnbd');

typedef _LoadUrlC = Int64 Function(Pointer<Void>, Pointer<Utf8>);
typedef _LoadUrlDart = int Function(Pointer<Void>, Pointer<Utf8>);
final _loadUrl = _lib.lookupFunction<_LoadUrlC, _LoadUrlDart>('tvcore_load_playlist_url');

typedef _LoadFileC = Int64 Function(Pointer<Void>, Pointer<Utf8>);
typedef _LoadFileDart = int Function(Pointer<Void>, Pointer<Utf8>);
final _loadFile = _lib.lookupFunction<_LoadFileC, _LoadFileDart>('tvcore_load_playlist_file');

typedef _LoadStrC = Int64 Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef _LoadStrDart = int Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
final _loadStr = _lib.lookupFunction<_LoadStrC, _LoadStrDart>('tvcore_load_playlist_str');

typedef _ChannelsJsonC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Int64);
typedef _ChannelsJsonDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, int);
final _channelsJson =
    _lib.lookupFunction<_ChannelsJsonC, _ChannelsJsonDart>('tvcore_channels_json');

typedef _CategoriesJsonC = Pointer<Utf8> Function(Pointer<Void>);
typedef _CategoriesJsonDart = Pointer<Utf8> Function(Pointer<Void>);
final _categoriesJson =
    _lib.lookupFunction<_CategoriesJsonC, _CategoriesJsonDart>('tvcore_categories_json');

typedef _SearchJsonC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Int64);
typedef _SearchJsonDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, int);
final _searchJson = _lib.lookupFunction<_SearchJsonC, _SearchJsonDart>('tvcore_search_json');

typedef _PlayJsonC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _PlayJsonDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
final _playJson = _lib.lookupFunction<_PlayJsonC, _PlayJsonDart>('tvcore_play_json');

typedef _ProbeJsonC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _ProbeJsonDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
final _probeJson = _lib.lookupFunction<_ProbeJsonC, _ProbeJsonDart>('tvcore_probe_json');

typedef _LastErrorC = Pointer<Utf8> Function();
typedef _LastErrorDart = Pointer<Utf8> Function();
final _lastError = _lib.lookupFunction<_LastErrorC, _LastErrorDart>('tvcore_last_error');

typedef _StringFreeC = Void Function(Pointer<Utf8>);
typedef _StringFreeDart = void Function(Pointer<Utf8>);
final _stringFree = _lib.lookupFunction<_StringFreeC, _StringFreeDart>('tvcore_string_free');

// ---------------------------------------------------------------------------
// helpers: call, take the JSON string, free it, decode
// ---------------------------------------------------------------------------

String? _take(Pointer<Utf8> ptr) {
  if (ptr == nullptr) return null;
  final s = ptr.toDartString();
  _stringFree(ptr);
  return s;
}

String _err() => _lastError().toDartString();

// ---------------------------------------------------------------------------
// Dart-facing API
// ---------------------------------------------------------------------------

class Channel {
  final String id, name, category, url;
  final String? logo;
  final int? score; // present on search results

  Channel(Map<String, dynamic> j)
      : id = j['id'],
        name = j['name'],
        category = j['category'],
        url = j['url'],
        logo = j['logo'],
        score = j['score'];
}

class StreamTarget {
  final String channelId, channelName, url, kind;
  final Map<String, String> headers;

  StreamTarget(Map<String, dynamic> j)
      : channelId = j['channelId'],
        channelName = j['channelName'],
        url = j['url'],
        kind = j['kind'],
        headers = {
          for (final h in (j['headers'] as List))
            (h[0] as String): (h[1] as String),
        };
}

class TvCore {
  final Pointer<Void> _h;

  TvCore._(this._h);

  factory TvCore() {
    final h = _new();
    if (h == nullptr) throw TvCoreException('tvcore_new failed');
    return TvCore._(h);
  }

  static String version() => _version().toDartString();

  void dispose() {
    _free(_h);
  }

  /// Official ITCN BD playlist. Returns the number of newly added channels.
  int loadItcnbd() {
    final n = _loadItcnbd(_h);
    if (n < 0) throw TvCoreException(_err());
    return n;
  }

  /// Any remote M3U/M3U8 playlist URL.
  int loadPlaylistUrl(String url) {
    final n = _loadUrl(_h, url.toNativeUtf8());
    if (n < 0) throw TvCoreException(_err());
    return n;
  }

  /// A playlist file from local storage (asset path you copied out, cache).
  int loadPlaylistFile(String path) {
    final n = _loadFile(_h, path.toNativeUtf8());
    if (n < 0) throw TvCoreException(_err());
    return n;
  }

  /// An in-memory playlist text (offline bundle, your backend).
  int loadPlaylistString(String m3u, {String source = 'memory'}) {
    final n = _loadStr(_h, m3u.toNativeUtf8(), source.toNativeUtf8());
    if (n < 0) throw TvCoreException(_err());
    return n;
  }

  List<Channel> channels({String? category, int limit = -1}) {
    final cat = category?.toNativeUtf8() ?? nullptr;
    final json = _take(_channelsJson(_h, cat, limit));
    if (json == null) throw TvCoreException(_err());
    if (cat != nullptr) malloc.free(cat);
    final list = jsonDecode(json)['channels'] as List;
    return list.map((e) => Channel(e)).toList();
  }

  Map<String, int> categories() {
    final json = _take(_categoriesJson(_h));
    if (json == null) throw TvCoreException(_err());
    return {
      for (final c in jsonDecode(json)['categories'])
        c['name'] as String: c['channels'] as int,
    };
  }

  List<Channel> search(String query, {int limit = -1}) {
    final json = _take(_searchJson(_h, query.toNativeUtf8(), limit));
    if (json == null) throw TvCoreException(_err());
    final list = jsonDecode(json)['channels'] as List;
    return list.map((e) => Channel(e)).toList();
  }

  /// Resolve playback by id, exact name, or free text.
  StreamTarget play(String key) {
    final json = _take(_playJson(_h, key.toNativeUtf8()));
    if (json == null) throw TvCoreException(_err());
    return StreamTarget(jsonDecode(json));
  }

  /// Liveness probe. Never throws on network failure — check `reachable`.
  ({int httpStatus, bool reachable}) probe(String channelId) {
    final json = _take(_probeJson(_h, channelId.toNativeUtf8()));
    if (json == null) throw TvCoreException(_err());
    final j = jsonDecode(json);
    return (httpStatus: j['httpStatus'], reachable: j['reachable']);
  }
}

class TvCoreException implements Exception {
  final String message;
  TvCoreException(this.message);
  @override
  String toString() => 'TvCoreException: $message';
}
```

Add `ffi` to `pubspec.yaml`:

```yaml
dependencies:
  ffi: ^2.1.0
```

---

## 3. Using it in a widget

All TvCore calls are **blocking** → run them with `compute`/`Isolate.run`:

```dart
import 'package:flutter/foundation.dart';

class TvCoreService {
  TvCore? _tv;

  Future<void> init() async {
    // one handle for the whole app; keep it on a background isolate
    _tv = await Isolate.run(() {
      final tv = TvCore();
      try {
        tv.loadItcnbd();                 // BDIX network
      } on TvCoreException {
        // fallback: mirror or bundled playlist
        tv.loadPlaylistUrl('https://iptv-org.github.io/iptv/countries/bd.m3u');
      }
      return tv;
    });
  }

  Future<List<Channel>> search(String q) =>
      Isolate.run(() => _tv!.search(q, limit: 100));

  Future<StreamTarget> play(String key) =>
      Isolate.run(() => _tv!.play(key));
}
```

A minimal channel-grid page:

```dart
FutureBuilder<List<Channel>>(
  future: service.search('sports'),
  builder: (context, snap) => GridView.builder(
    gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
        maxCrossAxisExtent: 160),
    itemCount: snap.data?.length ?? 0,
    itemBuilder: (context, i) {
      final ch = snap.data![i];
      return Column(children: [
        Image.network(ch.logo ?? '', errorBuilder: (_, __, ___) =>
            const Icon(Icons.tv)),
        Text(ch.name, overflow: TextOverflow.ellipsis),
      ]);
    },
  ),
)
```

---

## 4. Playing the stream

TvCore gives you the URL + headers; the player is your choice:

**media_kit** (recommended — HLS with custom headers on every platform):

```dart
final target = await service.play('t sports');
final uri = Uri.parse(target.url);
final headerMap = target.headers;

await player.open(Media(
  target.url,
  httpHeaders: target.headers, // media_kit accepts a headers map
));
```

**video_player** (no custom headers on Android — pre-sign or mirror the URL; headers work on iOS/macOS via `AVPlayer` only with additional setup):

```dart
final controller = VideoPlayerController.networkUrl(
  Uri.parse(target.url),
  videoPlayerOptions: VideoPlayerOptions(mixWithOthers: true),
);
```

For BDIX streams that require the User-Agent/Referer on **every segment** request (not just the playlist), prefer `media_kit`/`mpv` or a tiny local proxy that injects the headers — the needed headers come from `target.headers`.

---

## 5. Threading rules

| Rule | Why |
| --- | --- |
| Create the handle anywhere; call `load*`/`search`/`play` off the UI thread | The built-in HTTP client is blocking (`ureq`). |
| One handle shared by many isolates is safe | The core is `Send + Sync`; per-call state is guarded internally. |
| Free returned JSON strings | The Dart wrapper above already does (`_take`). |
| `tvcore_last_error()` is per-thread | Read it right after the failing call (the wrapper does). |

---

## 6. Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `TvCoreException: http status error: 403` on `loadItcnbd` | itcnbd CDN serves BDIX (Bangladeshi ISP) networks only; it 403s datacenter/VPN IPs | Run from a BD ISP network, or `loadPlaylistUrl` a mirror / bundled asset |
| `loadItcnbd` count is 0 with no error | The playlist fetch returned HTML or empty body — the parser refuses non-playlist responses | Check the source URL / your network captive portal |
| Stream URL plays nothing | Player didn't send the required headers | Use `target.headers` with the player (or a header-injecting local proxy) |
| `Invalid argument(s)` on ffi call | Older `.so` in jniLibs (ABI mismatch) | Rebuild with `cargo ndk` for all targets and clean the app |

---

Next: [`docs/KOTLIN.md`](KOTLIN.md) for the native Android path, or [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) for how the core works.
