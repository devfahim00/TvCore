# Integration guide

All clients load the same native library and use the same JSON shapes (see [API.md](API.md)).
Build instructions for each platform are in [BUILDING.md](BUILDING.md).

Rule for every client: **network calls block**. Run `refresh`, `play(verify)` and `probe` off the UI thread.
Call `play()` right before you start a channel so the token is fresh.

## Flutter (dart:ffi)

1. Build the library for Android ABIs and put the `.so` files in `android/app/src/main/jniLibs/<abi>/libtvcore_ffi.so`.
   (iOS: link `libtvcore_ffi.a` into the Runner and use `DynamicLibrary.process()`.)
2. Copy `bindings/dart/lib/tvcore.dart` into your app (or depend on it by path) and add `ffi: ^2.1.0`.

```dart
final core = TvCore(config: {'timeout_secs': 10});
await core.refreshAsync();                       // runs in a short-lived isolate
final cats = core.categories();                  // fast, no network
final results = core.search('star sports', limit: 10);

final info = await core.playAsync(results.first.id);   // verify=true by default, refreshes the token
// video_player
final controller = VideoPlayerController.networkUrl(
  Uri.parse(info.url),
  httpHeaders: info.headers,
);
await controller.initialize();
controller.play();
```

`media_kit`: `player.open(Media(info.url, httpHeaders: info.headers))`.
Keep one `TvCore` for the app lifetime; call `dispose()` on exit.

## Kotlin / Android

```kotlin
// build.gradle.kts
implementation("net.java.dev.jna:jna:5.14.0@aar")
implementation("androidx.media3:media3-exoplayer-hls:1.4.1")
```

Copy `bindings/kotlin/TvCore.kt`, put `libtvcore_ffi.so` in `jniLibs/<abi>/`.

```kotlin
val core = TvCore()                                   // keep in a ViewModel / singleton
viewModelScope.launch(Dispatchers.IO) {
    core.refresh()
    val hits = core.search("jamuna", limit = 5)
    val info = core.play(hits.first().id, verify = true)
    withContext(Dispatchers.Main) {
        val http = DefaultHttpDataSource.Factory().setDefaultRequestProperties(info.headers)
        val source = HlsMediaSource.Factory(http).createMediaSource(MediaItem.fromUri(info.url))
        player.setMediaSource(source); player.prepare(); player.play()
    }
}
```

Prefer your own HTTP stack (OkHttp) instead of the built-in one? Build with `--no-default-features`, download the page yourself
and call `core.loadHtml(html)`. Then `play()` returns `expired = true` when the token is old: re-download and `loadHtml` again.

## Swift / iOS

Link `libtvcore_ffi.a`, add `crates/tvcore-ffi/include/tvcore.h` to a bridging header:

```swift
let h = tvcore_new(nil)
defer { tvcore_free(h) }
let p = tvcore_search(h, "jamuna", nil, 5)!
let json = String(cString: p); tvcore_string_free(p)
// JSONDecoder -> envelope {ok, data}
```

## Python

```python
from tvcore import TvCore          # bindings/python/tvcore.py, set TVCORE_LIB or put the .so next to it
with TvCore() as core:
    core.refresh()
    ch = core.search("jamuna")[0]
    print(core.play(ch["id"])["url"])
```

## C / C++

```c
#include "tvcore.h"
TvCore *h = tvcore_new(NULL);
char *r = tvcore_search(h, "jamuna", NULL, 5);   /* JSON envelope */
puts(r);
tvcore_string_free(r);
tvcore_free(h);
```

## Command line / scripts

`tvcore list|search|play|m3u --json`. `play` prints the URL then the headers, or full JSON with `--json`.
`tvcore m3u > itcn.m3u` produces a playlist with referrer/user-agent hints for VLC; tokens in it last about 4 hours.

## Compatibility notes

* The library never needs to be told about categories or ids in advance; ids come from `channels()` / `search()`.
* Unknown JSON fields should be ignored by clients; new fields may be added in minor versions.
* Every response carries `ok`; check it before reading `data`.
