# Kotlin / Android integration guide

TvCore compiles to `libtvcore.so`, a plain C ABI — Android reaches it with
**System.loadLibrary + JNI**. No Kotlin-Native, no coroutine bridge, JSON in /
JSON out.

This guide gives you a **complete, copy-paste integration**:

1. [Build the `.so` for Android](#1-build-the-so)
2. [The Kotlin facade](#2-the-kotlin-facade)
3. [The JNI C glue (one small file)](#3-the-jni-c-glue)
4. [Playing with ExoPlayer (headers included)](#4-playing-with-exoplayer)
5. [Gradle wiring](#5-gradle-wiring)
6. [Threading rules](#6-threading-rules)

---

## 1. Build the .so

Install [cargo-ndk](https://github.com/rust-mobile/cargo-ndk) and the targets:

```sh
cargo install cargo-ndk
rustup target add arm64-v8a armeabi-v7a x86_64 x86
```

From the TvCore repo root:

```sh
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 \
  -o app/src/main/jniLibs \
  build --release -p tvcore-ffi
```

You end up with `app/src/main/jniLibs/<abi>/libtvcore.so`.

(Automate it in Gradle later — see [section 5](#5-gradle-wiring).)

---

## 2. The Kotlin facade

```kotlin
// TvCore.kt — everything your app talks to.
package com.example.tvcore

import org.json.JSONArray
import org.json.JSONObject

class TvCoreException(message: String) : RuntimeException(message)

data class Channel(
    val id: String,
    val name: String,
    val category: String,
    val logo: String?,
    val url: String,
    val score: Int? = null,
)

data class StreamTarget(
    val channelId: String,
    val channelName: String,
    val url: String,
    val kind: String,                 // hls | dash | ts | unknown
    val headers: Map<String, String>, // send with every request to the stream
)

class TvCore private constructor(private val handle: Long) {

    companion object {
        init { System.loadLibrary("tvcore") }

        /** Blocking call — build on a background thread. */
        fun create(): TvCore {
            val h = nativeNew()
            if (h == 0L) throw TvCoreException(lastError())
            return TvCore(h)
        }

        external fun nativeNew(): Long
        external fun nativeFree(handle: Long)
        external fun nativeVersion(): String
        external fun nativeLoadItcnbd(handle: Long): Int
        external fun nativeLoadPlaylistUrl(handle: Long, url: String): Int
        external fun nativeLoadPlaylistFile(handle: Long, path: String): Int
        external fun nativeLoadPlaylistStr(handle: Long, text: String, source: String): Int
        external fun nativeChannelsJson(handle: Long, category: String?, limit: Int): String?
        external fun nativeCategoriesJson(handle: Long): String?
        external fun nativeSearchJson(handle: Long, query: String, limit: Int): String?
        external fun nativePlayJson(handle: Long, key: String): String?
        external fun nativeProbeJson(handle: Long, id: String): String?
        external fun lastError(): String
    }

    fun version(): String = nativeVersion(handle)

    /** Official ITCN BD playlist; returns newly added channel count. */
    fun loadItcnbd(): Int {
        val n = nativeLoadItcnbd(handle)
        if (n < 0) throw TvCoreException(lastError())
        return n
    }

    fun loadPlaylistUrl(url: String): Int {
        val n = nativeLoadPlaylistUrl(handle, url)
        if (n < 0) throw TvCoreException(lastError())
        return n
    }

    fun loadPlaylistFile(path: String): Int {
        val n = nativeLoadPlaylistFile(handle, path)
        if (n < 0) throw TvCoreException(lastError())
        return n
    }

    fun loadPlaylistString(m3u: String, source: String = "memory"): Int {
        val n = nativeLoadPlaylistStr(handle, m3u, source)
        if (n < 0) throw TvCoreException(lastError())
        return n
    }

    fun channels(category: String? = null, limit: Int = -1): List<Channel> {
        val json = nativeChannelsJson(handle, category, limit)
            ?: throw TvCoreException(lastError())
        val arr = JSONObject(json).getJSONArray("channels")
        return (0 until arr.length()).map { Channel(arr.getJSONObject(it)) }
    }

    fun categories(): Map<String, Int> {
        val json = nativeCategoriesJson(handle)
            ?: throw TvCoreException(lastError())
        val arr = JSONObject(json).getJSONArray("categories")
        val out = LinkedHashMap<String, Int>()
        for (i in 0 until arr.length()) {
            val o = arr.getJSONObject(i)
            out[o.getString("name")] = o.getInt("channels")
        }
        return out
    }

    fun search(query: String, limit: Int = -1): List<Channel> {
        val json = nativeSearchJson(handle, query, limit)
            ?: throw TvCoreException(lastError())
        val arr = JSONObject(json).getJSONArray("channels")
        return (0 until arr.length()).map { Channel(arr.getJSONObject(it)) }
    }

    /** By id, by exact name, or free text (falls back to best search hit). */
    fun play(key: String): StreamTarget {
        val json = nativePlayJson(handle, key)
            ?: throw TvCoreException(lastError())
        val o = JSONObject(json)
        val headers = LinkedHashMap<String, String>()
        val hs = o.getJSONArray("headers")
        for (i in 0 until hs.length()) {
            headers[hs.getJSONArray(i).getString(0)] = hs.getJSONArray(i).getString(1)
        }
        return StreamTarget(
            channelId = o.getString("channelId"),
            channelName = o.getString("channelName"),
            url = o.getString("url"),
            kind = o.getString("kind"),
            headers = headers,
        )
    }

    /** Liveness probe — never throws on network failure. */
    fun probe(channelId: String): Pair<Int, Boolean> {
        val json = nativeProbeJson(handle, channelId)
            ?: throw TvCoreException(lastError())
        val o = JSONObject(json)
        return o.getInt("httpStatus") to o.getBoolean("reachable")
    }

    fun dispose() {
        if (handle != 0L) {
            nativeFree(handle)
        }
    }

    // -- JSON helpers ------------------------------------------------------
    private fun Channel(o: JSONObject) = Channel(
        id = o.getString("id"),
        name = o.getString("name"),
        category = o.optString("category", "Uncategorized"),
        logo = if (o.has("logo")) o.getString("logo") else null,
        url = o.getString("url"),
        score = if (o.has("score")) o.getInt("score") else null,
    )
}
```

---

## 3. The JNI C glue

One small C file, `app/src/main/cpp/tvcore-jni.c`, bridges
`System.loadLibrary("tvcore")` to the C ABI (which TvCore already exports —
`tvcore_*` are plain C symbols, so we only wrap string plumbing):

```c
#include <jni.h>
#include <string.h>
#include <stdlib.h>
#include <stdint.h>

#include "tvcore.h"   /* copy from TvCore/tvcore-ffi/include/tvcore.h */

static jstring take_string(JNIEnv *env, char *cstr) {
    if (!cstr) return NULL;
    jstring out = (*env)->NewStringUTF(env, cstr);
    tvcore_string_free(cstr);
    return out;
}

JNIEXPORT jlong JNICALL
Java_com_example_tvcore_TvCore_nativeNew(JNIEnv *env, jclass cls) {
    (void)env; (void)cls;
    return (jlong)(intptr_t)tvcore_new();
}

JNIEXPORT void JNICALL
Java_com_example_tvcore_TvCore_nativeFree(JNIEnv *env, jclass cls, jlong h) {
    (void)env; (void)cls;
    tvcore_free((TvCore *)(intptr_t)h);
}

JNIEXPORT jstring JNICALL
Java_com_example_tvcore_TvCore_nativeVersion(JNIEnv *env, jclass cls) {
    (void)cls;
    return (*env)->NewStringUTF(env, tvcore_version());
}

JNIEXPORT jint JNICALL
Java_com_example_tvcore_TvCore_nativeLoadItcnbd(JNIEnv *env, jclass cls, jlong h) {
    (void)env; (void)cls;
    return (jint)tvcore_load_itcnbd((TvCore *)(intptr_t)h);
}

JNIEXPORT jint JNICALL
Java_com_example_tvcore_TvCore_nativeLoadPlaylistUrl(
        JNIEnv *env, jclass cls, jlong h, jstring url) {
    (void)cls;
    const char *u = (*env)->GetStringUTFChars(env, url, NULL);
    jint n = (jint)tvcore_load_playlist_url((TvCore *)(intptr_t)h, u);
    (*env)->ReleaseStringUTFChars(env, url, u);
    return n;
}

JNIEXPORT jint JNICALL
Java_com_example_tvcore_TvCore_nativeLoadPlaylistFile(
        JNIEnv *env, jclass cls, jlong h, jstring path) {
    (void)cls;
    const char *p = (*env)->GetStringUTFChars(env, path, NULL);
    jint n = (jint)tvcore_load_playlist_file((TvCore *)(intptr_t)h, p);
    (*env)->ReleaseStringUTFChars(env, path, p);
    return n;
}

JNIEXPORT jint JNICALL
Java_com_example_tvcore_TvCore_nativeLoadPlaylistStr(
        JNIEnv *env, jclass cls, jlong h, jstring text, jstring source) {
    (void)cls;
    const char *t = (*env)->GetStringUTFChars(env, text, NULL);
    const char *s = (*env)->GetStringUTFChars(env, source, NULL);
    jint n = (jint)tvcore_load_playlist_str((TvCore *)(intptr_t)h, t, s);
    (*env)->ReleaseStringUTFChars(env, text, t);
    (*env)->ReleaseStringUTFChars(env, source, s);
    return n;
}

JNIEXPORT jstring JNICALL
Java_com_example_tvcore_TvCore_nativeChannelsJson(
        JNIEnv *env, jclass cls, jlong h, jstring category, jint limit) {
    (void)cls;
    const char *cat = category ? (*env)->GetStringUTFChars(env, category, NULL) : NULL;
    char *json = tvcore_channels_json((TvCore *)(intptr_t)h, cat, (int64_t)limit);
    if (cat) (*env)->ReleaseStringUTFChars(env, category, cat);
    return take_string(env, json);
}

JNIEXPORT jstring JNICALL
Java_com_example_tvcore_TvCore_nativeCategoriesJson(
        JNIEnv *env, jclass cls, jlong h) {
    (void)env; (void)cls;
    return take_string(env, tvcore_categories_json((TvCore *)(intptr_t)h));
}

JNIEXPORT jstring JNICALL
Java_com_example_tvcore_TvCore_nativeSearchJson(
        JNIEnv *env, jclass cls, jlong h, jstring query, jint limit) {
    (void)cls;
    const char *q = (*env)->GetStringUTFChars(env, query, NULL);
    char *json = tvcore_search_json((TvCore *)(intptr_t)h, q, (int64_t)limit);
    (*env)->ReleaseStringUTFChars(env, query, q);
    return take_string(env, json);
}

JNIEXPORT jstring JNICALL
Java_com_example_tvcore_TvCore_nativePlayJson(
        JNIEnv *env, jclass cls, jlong h, jstring key) {
    (void)cls;
    const char *k = (*env)->GetStringUTFChars(env, key, NULL);
    char *json = tvcore_play_json((TvCore *)(intptr_t)h, k);
    (*env)->ReleaseStringUTFChars(env, key, k);
    return take_string(env, json);
}

JNIEXPORT jstring JNICALL
Java_com_example_tvcore_TvCore_nativeProbeJson(
        JNIEnv *env, jclass cls, jlong h, jstring id) {
    (void)cls;
    const char *i = (*env)->GetStringUTFChars(env, id, NULL);
    char *json = tvcore_probe_json((TvCore *)(intptr_t)h, i);
    (*env)->ReleaseStringUTFChars(env, id, i);
    return take_string(env, json);
}

JNIEXPORT jstring JNICALL
Java_com_example_tvcore_TvCore_lastError(JNIEnv *env, jclass cls) {
    (void)cls;
    return (*env)->NewStringUTF(env, tvcore_last_error());
}
```

The `tvcore.h` header is at `tvcore-ffi/include/tvcore.h` — copy it next to
this file. JNI symbol names must match your package
(`Java_com_example_tvcore_TvCore_...`): change `com_example_tvcore` to your
package path.

---

## 4. Playing with ExoPlayer

ExoPlayer (Media3) is the natural player for the HLS targets TvCore returns:

```kotlin
class PlayerViewModel(app: Application) : AndroidViewModel(app) {

    private val tvCore = TvCore.create()      // blocking — see threading notes
    private val player = ExoPlayer.Builder(app).build()

    fun channels(query: String): List<Channel> = tvCore.search(query, limit = 100)

    /** id, exact name, or free text — resolves the stream for the player. */
    fun play(channelKey: String) {
        val target = tvCore.play(channelKey)

        val dataSourceFactory = DefaultHttpDataSource.Factory()
            .setUserAgent(target.headers["User-Agent"])
            .setDefaultRequestProperties(
                target.headers.filterKeys { it != "User-Agent" }
            )

        val source = when (target.kind) {
            "dash" -> DashMediaSource.Factory(dataSourceFactory)
                .createMediaSource(MediaItem.fromUri(target.url))
            else -> HlsMediaSource.Factory(dataSourceFactory)
                .createMediaSource(MediaItem.fromUri(target.url))   // hls + ts
        }

        player.setMediaSource(source)
        player.prepare()
        player.playWhenReady = true
    }

    override fun onCleared() {
        player.release()
        tvCore.dispose()
    }
}
```

The important part: the User-Agent / Referer from `target.headers` go into
`DefaultHttpDataSource` — BDIX CDNs reject (or misroute) requests without
them, and the headers must be on **every** segment request, which the data
source factory handles automatically.

---

## 5. Gradle wiring

`app/build.gradle.kts` — build the Rust lib + JNI glue with every build:

```kotlin
android {
    defaultConfig {
        externalNativeBuild { cmake { } }
        ndkVersion = "26.1.7099652"      // or newer
    }
    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }
}

// optional task to always rebuild the rust library before assemble:
val rustTargets = listOf("arm64-v8a", "armeabi-v7a", "x86_64")
tasks.matching { it.name.startsWith("externalNativeBuild") }.configureEach {
    dependsOn("buildRust")
}
tasks.register<Exec>("buildRust") {
    workingDir = file("../../TvCore")            // TvCore repo checkout
    commandLine = listOf(
        "cargo", "ndk",
        "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64",
        "-o", file("src/main/jniLibs").absolutePath,
        "build", "--release", "-p", "tvcore-ffi"
    )
}
```

`app/src/main/cpp/CMakeLists.txt` for the JNI glue:

```cmake
cmake_minimum_required(VERSION 3.22)
project(tvcore-jni)

add_library(tvcore-jni SHARED tvcore-jni.c)

# The libtvcore.so produced by cargo-ndk already ships in jniLibs; do not
# link it twice — we only declare it so find_library resolves:
find_library(tvcore-lib tvcore PATHS ${CMAKE_SOURCE_DIR}/../jniLibs/${ANDROID_ABI})
target_link_libraries(tvcore-jni log ${tvcore-lib})
```

Alternative without CMake: put `tvcore-jni.c` in the NDK toolchain build via
`System.loadLibrary("tvcore-jni")` and a prebuilt `.so` made with
`cc` from the NDK (`aarch64-linux-android24-clang -shared ...`).

---

## 6. Threading rules

| Rule | Why |
| --- | --- |
| `TvCore.create()` / `load*` / `search` / `play` on `Dispatchers.IO` (or your own `THREAD_POOL`) | The built-in HTTP layer is blocking. |
| Share one handle across threads freely | The core is `Send + Sync`. |
| Read `lastError()` immediately after a failure | It is thread-local (the Kotlin facade does it for you). |
| `dispose()` exactly once | The handle owns a heap allocation. |

Kotlin-coroutines usage:

```kotlin
val channels = withContext(Dispatchers.IO) { tvCore.search("sports") }
val target   = withContext(Dispatchers.IO) { tvCore.play("t sports") }
```

---

## Smoke test

```kotlin
runBlocking {
    withContext(Dispatchers.IO) {
        val tv = TvCore.create()
        tv.loadPlaylistString(demoPlaylistAsset, source = "demo")
        println(tv.categories())               // {BANGLA=15, SPORTS=8, ...}
        println(tv.search("sprots").take(3))   // typo tolerant
        val t = tv.play("t sports")
        println("${t.url} [${t.kind}]")         // .../T-Sports-HD/... .m3u8 [hls]
        tv.dispose()
    }
}
```

Next: [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) explains what happens under
the hood, and [`docs/API.md`](API.md) documents the Rust API in detail.
