/// dart:ffi binding for TvCore (libtvcore_ffi).
///
/// Network calls (`refresh`, `play(verify: true)`, `probe`) BLOCK the calling isolate.
/// In Flutter use the `*Async` helpers (they run in a short-lived isolate) or your own
/// `Isolate.run`. The native handle is thread-safe, so sharing its address is fine.
library tvcore;

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';

class TvCoreException implements Exception {
  TvCoreException(this.code, this.message);
  final String code;
  final String message;
  @override
  String toString() => 'TvCoreException($code): $message';
}

class Channel {
  Channel(this.id, this.title, this.category, this.logo, this.streamUrl, this.expiresAt);
  final String id;
  final String title;
  final String category;
  final String? logo;
  final String streamUrl;
  final int? expiresAt;

  factory Channel.fromJson(Map<String, dynamic> j) => Channel(
        j['id'] as String,
        j['title'] as String,
        j['category'] as String,
        j['logo'] as String?,
        j['stream_url'] as String,
        j['expires_at'] as int?,
      );
}

class Category {
  Category(this.name, this.count);
  final String name;
  final int count;
  factory Category.fromJson(Map<String, dynamic> j) => Category(j['name'] as String, j['count'] as int);
}

class PlayInfo {
  PlayInfo(this.id, this.title, this.category, this.url, this.headers, this.expiresAt, this.refreshed, this.expired);
  final String id;
  final String title;
  final String category;

  /// Pass to the player together with [headers].
  final String url;
  final Map<String, String> headers;
  final int? expiresAt;
  final bool refreshed;
  final bool expired;

  factory PlayInfo.fromJson(Map<String, dynamic> j) => PlayInfo(
        j['id'] as String,
        j['title'] as String,
        j['category'] as String,
        j['url'] as String,
        Map<String, String>.from(j['headers'] as Map),
        j['expires_at'] as int?,
        j['refreshed'] as bool,
        j['expired'] as bool,
      );
}

typedef _NewC = Pointer<Void> Function(Pointer<Utf8>);
typedef _FreeC = Void Function(Pointer<Void>);
typedef _FreeD = void Function(Pointer<Void>);
typedef _StrFreeC = Void Function(Pointer<Utf8>);
typedef _StrFreeD = void Function(Pointer<Utf8>);
typedef _H = Pointer<Utf8> Function(Pointer<Void>);
typedef _HS = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _HSS = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef _Search = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, int);
typedef _HC = Pointer<Utf8> Function(Pointer<Void>);
typedef _HSC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _HSSC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>);
typedef _SearchC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Int32);

DynamicLibrary _open(String? path) {
  if (path != null) return DynamicLibrary.open(path);
  if (Platform.isAndroid || Platform.isLinux) return DynamicLibrary.open('libtvcore_ffi.so');
  if (Platform.isWindows) return DynamicLibrary.open('tvcore_ffi.dll');
  // iOS (static lib linked into the runner) and macOS (if statically linked)
  return DynamicLibrary.process();
}

class TvCore {
  TvCore._(this._lib, this._h) {
    _free = _lib.lookupFunction<_FreeC, _FreeD>('tvcore_free');
    _strFree = _lib.lookupFunction<_StrFreeC, _StrFreeD>('tvcore_string_free');
    _refresh = _lib.lookupFunction<_HC, _H>('tvcore_refresh');
    _loadHtml = _lib.lookupFunction<_HSC, _HS>('tvcore_load_html');
    _list = _lib.lookupFunction<_HSC, _HS>('tvcore_list');
    _categories = _lib.lookupFunction<_HC, _H>('tvcore_categories');
    _search = _lib.lookupFunction<_SearchC, _Search>('tvcore_search');
    _get = _lib.lookupFunction<_HSC, _HS>('tvcore_get');
    _play = _lib.lookupFunction<_HSSC, _HSS>('tvcore_play');
    _probe = _lib.lookupFunction<_HSC, _HS>('tvcore_probe');
    _m3u = _lib.lookupFunction<_HC, _H>('tvcore_export_m3u');
  }

  final DynamicLibrary _lib;
  Pointer<Void> _h;
  late final _FreeD _free;
  late final _StrFreeD _strFree;
  late final _H _refresh;
  late final _HS _loadHtml;
  late final _HS _list;
  late final _H _categories;
  late final _Search _search;
  late final _HS _get;
  late final _HSS _play;
  late final _HS _probe;
  late final _H _m3u;

  /// [config] e.g. `{'timeout_secs': 10}`; `null` = defaults.
  factory TvCore({Map<String, Object?>? config, String? libraryPath}) {
    final lib = _open(libraryPath);
    final create = lib.lookupFunction<_NewC, _NewC>('tvcore_new');
    final cfg = config == null ? nullptr : jsonEncode(config).toNativeUtf8();
    final h = create(cfg);
    if (cfg != nullptr) malloc.free(cfg);
    if (h == nullptr) throw TvCoreException('invalid_input', 'invalid config');
    return TvCore._(lib, h);
  }

  /// Re-attach to a handle created elsewhere (e.g. in another isolate) via [address].
  factory TvCore.fromAddress(int address, {String? libraryPath}) =>
      TvCore._(_open(libraryPath), Pointer<Void>.fromAddress(address));

  int get address => _h.address;

  void dispose() {
    if (_h != nullptr) {
      _free(_h);
      _h = nullptr;
    }
  }

  dynamic _unwrap(Pointer<Utf8> p) {
    final raw = p.toDartString();
    _strFree(p);
    final env = jsonDecode(raw) as Map<String, dynamic>;
    if (env['ok'] == true) return env['data'];
    final e = env['error'] as Map<String, dynamic>;
    throw TvCoreException(e['code'] as String, e['message'] as String);
  }

  T _withStr<T>(String? s, T Function(Pointer<Utf8>) f) {
    final p = s == null ? nullptr.cast<Utf8>() : s.toNativeUtf8();
    try {
      return f(p);
    } finally {
      if (p != nullptr) malloc.free(p);
    }
  }

  // ---- sync API -----------------------------------------------------------
  int refresh() => (_unwrap(_refresh(_h)) as Map)['count'] as int;

  int loadHtml(String html) => _withStr(html, (p) => (_unwrap(_loadHtml(_h, p)) as Map)['count'] as int);

  List<Channel> channels({String? category}) => _withStr(
        category,
        (p) => (_unwrap(_list(_h, p)) as List).map((e) => Channel.fromJson(e as Map<String, dynamic>)).toList(),
      );

  List<Category> categories() =>
      (_unwrap(_categories(_h)) as List).map((e) => Category.fromJson(e as Map<String, dynamic>)).toList();

  List<Channel> search(String query, {String? category, int limit = 0}) => _withStr(
        query,
        (q) => _withStr(
          category,
          (c) => (_unwrap(_search(_h, q, c, limit)) as List)
              .map((e) => Channel.fromJson(e as Map<String, dynamic>))
              .toList(),
        ),
      );

  Channel get(String id) =>
      _withStr(id, (p) => Channel.fromJson(_unwrap(_get(_h, p)) as Map<String, dynamic>));

  PlayInfo play(String id, {bool verify = false, bool forceRefresh = false}) {
    final opts = jsonEncode({'verify': verify, 'force_refresh': forceRefresh});
    return _withStr(
      id,
      (i) => _withStr(opts, (o) => PlayInfo.fromJson(_unwrap(_play(_h, i, o)) as Map<String, dynamic>)),
    );
  }

  Map<String, dynamic> probe(String url) =>
      _withStr(url, (p) => Map<String, dynamic>.from(_unwrap(_probe(_h, p)) as Map));

  String exportM3u() => _unwrap(_m3u(_h)) as String;

  // ---- async helpers (run the blocking call off the UI isolate) -----------
  Future<int> refreshAsync({String? libraryPath}) {
    final a = address;
    return Isolate.run(() => TvCore.fromAddress(a, libraryPath: libraryPath).refresh());
  }

  Future<PlayInfo> playAsync(String id, {bool verify = true, bool forceRefresh = false, String? libraryPath}) {
    final a = address;
    return Isolate.run(() =>
        TvCore.fromAddress(a, libraryPath: libraryPath).play(id, verify: verify, forceRefresh: forceRefresh));
  }
}
