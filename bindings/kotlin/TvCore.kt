package com.devfahim00.tvcore

import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import org.json.JSONArray
import org.json.JSONObject

/**
 * Kotlin / Android binding for libtvcore_ffi via JNA.
 *
 * Gradle (app):
 *   implementation("net.java.dev.jna:jna:5.14.0@aar")
 * Put libtvcore_ffi.so in app/src/main/jniLibs/<abi>/ (see docs/BUILDING.md).
 *
 * Network calls block: call them from a coroutine on Dispatchers.IO.
 */
internal interface TvCoreLib : Library {
    fun tvcore_version(): String
    fun tvcore_new(configJson: String?): Pointer?
    fun tvcore_free(h: Pointer)
    fun tvcore_string_free(s: Pointer)
    fun tvcore_refresh(h: Pointer): Pointer?
    fun tvcore_load_html(h: Pointer, html: String): Pointer?
    fun tvcore_list(h: Pointer, category: String?): Pointer?
    fun tvcore_categories(h: Pointer): Pointer?
    fun tvcore_search(h: Pointer, query: String?, category: String?, limit: Int): Pointer?
    fun tvcore_get(h: Pointer, id: String): Pointer?
    fun tvcore_play(h: Pointer, id: String, optionsJson: String?): Pointer?
    fun tvcore_probe(h: Pointer, url: String): Pointer?
    fun tvcore_export_m3u(h: Pointer): Pointer?
}

class TvCoreException(val code: String, message: String) : Exception("$code: $message")

data class Channel(
    val id: String,
    val title: String,
    val category: String,
    val logo: String?,
    val streamUrl: String,
    val expiresAt: Long?,
)

data class Category(val name: String, val count: Int)

data class PlayInfo(
    val id: String,
    val title: String,
    val category: String,
    val url: String,
    val headers: Map<String, String>,
    val expiresAt: Long?,
    val refreshed: Boolean,
    val expired: Boolean,
)

class TvCore(configJson: String? = null, libName: String = "tvcore_ffi") : AutoCloseable {
    private val lib: TvCoreLib = Native.load(libName, TvCoreLib::class.java)
    private var handle: Pointer? = lib.tvcore_new(configJson)
        ?: throw TvCoreException("invalid_input", "invalid config JSON")

    private fun h(): Pointer = handle ?: throw IllegalStateException("TvCore is closed")

    private fun data(p: Pointer?): Any {
        if (p == null) throw TvCoreException("panic", "null response")
        val raw = try { p.getString(0, "UTF-8") } finally { lib.tvcore_string_free(p) }
        val env = JSONObject(raw)
        if (!env.getBoolean("ok")) {
            val e = env.getJSONObject("error")
            throw TvCoreException(e.getString("code"), e.getString("message"))
        }
        return env.get("data")
    }

    private fun JSONObject.optStringOrNull(k: String): String? = if (isNull(k)) null else getString(k)
    private fun JSONObject.optLongOrNull(k: String): Long? = if (isNull(k)) null else getLong(k)

    private fun channel(o: JSONObject) = Channel(
        o.getString("id"), o.getString("title"), o.getString("category"),
        o.optStringOrNull("logo"), o.getString("stream_url"), o.optLongOrNull("expires_at"),
    )

    private fun channels(a: JSONArray) = List(a.length()) { channel(a.getJSONObject(it)) }

    fun refresh(): Int = (data(lib.tvcore_refresh(h())) as JSONObject).getInt("count")

    fun loadHtml(html: String): Int = (data(lib.tvcore_load_html(h(), html)) as JSONObject).getInt("count")

    fun channels(category: String? = null): List<Channel> =
        channels(data(lib.tvcore_list(h(), category)) as JSONArray)

    fun categories(): List<Category> {
        val a = data(lib.tvcore_categories(h())) as JSONArray
        return List(a.length()) { Category(a.getJSONObject(it).getString("name"), a.getJSONObject(it).getInt("count")) }
    }

    fun search(query: String, category: String? = null, limit: Int = 0): List<Channel> =
        channels(data(lib.tvcore_search(h(), query, category, limit)) as JSONArray)

    fun get(id: String): Channel = channel(data(lib.tvcore_get(h(), id)) as JSONObject)

    fun play(id: String, verify: Boolean = false, forceRefresh: Boolean = false): PlayInfo {
        val opts = JSONObject().put("verify", verify).put("force_refresh", forceRefresh).toString()
        val o = data(lib.tvcore_play(h(), id, opts)) as JSONObject
        val hd = o.getJSONObject("headers")
        val headers = hd.keys().asSequence().associateWith { hd.getString(it) }
        return PlayInfo(
            o.getString("id"), o.getString("title"), o.getString("category"), o.getString("url"),
            headers, o.optLongOrNull("expires_at"), o.getBoolean("refreshed"), o.getBoolean("expired"),
        )
    }

    fun exportM3u(): String = data(lib.tvcore_export_m3u(h())) as String

    override fun close() {
        handle?.let { lib.tvcore_free(it) }
        handle = null
    }
}

/*
 * ExoPlayer / Media3 usage:
 *
 *   val info = withContext(Dispatchers.IO) { core.play(channel.id, verify = true) }
 *   val factory = DefaultHttpDataSource.Factory().setDefaultRequestProperties(info.headers)
 *   val source = HlsMediaSource.Factory(factory).createMediaSource(MediaItem.fromUri(info.url))
 *   player.setMediaSource(source); player.prepare(); player.play()
 */
