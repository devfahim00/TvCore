"""ctypes binding for tvcore-ffi (libtvcore_ffi.so / .dylib / tvcore_ffi.dll).

    from tvcore import TvCore
    core = TvCore()                      # or TvCore(lib_path="...", config={"timeout_secs": 10})
    core.refresh()
    for ch in core.search("jamuna", limit=3):
        print(ch["title"])
    play = core.play(ch["id"])           # {"url": ..., "headers": {...}, ...}
"""
import ctypes
import json
import os
import sys
from typing import Any, Dict, List, Optional


class TvCoreError(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message


def _default_lib() -> str:
    name = {"win32": "tvcore_ffi.dll", "darwin": "libtvcore_ffi.dylib"}.get(sys.platform, "libtvcore_ffi.so")
    here = os.path.dirname(os.path.abspath(__file__))
    for p in (os.environ.get("TVCORE_LIB"), os.path.join(here, name),
              os.path.join(here, "..", "..", "target", "release", name)):
        if p and os.path.exists(p):
            return p
    return name  # let the loader search


class TvCore:
    def __init__(self, config: Optional[Dict[str, Any]] = None, lib_path: Optional[str] = None):
        lib = ctypes.CDLL(lib_path or _default_lib())
        c_str, c_ptr, c_int = ctypes.c_char_p, ctypes.c_void_p, ctypes.c_int32
        sigs = {
            "tvcore_new": ([c_str], c_ptr),
            "tvcore_free": ([c_ptr], None),
            "tvcore_refresh": ([c_ptr], c_ptr),
            "tvcore_load_html": ([c_ptr, c_str], c_ptr),
            "tvcore_list": ([c_ptr, c_str], c_ptr),
            "tvcore_categories": ([c_ptr], c_ptr),
            "tvcore_search": ([c_ptr, c_str, c_str, c_int], c_ptr),
            "tvcore_get": ([c_ptr, c_str], c_ptr),
            "tvcore_play": ([c_ptr, c_str, c_str], c_ptr),
            "tvcore_probe": ([c_ptr, c_str], c_ptr),
            "tvcore_export_m3u": ([c_ptr], c_ptr),
            "tvcore_string_free": ([c_ptr], None),
        }
        for fn, (args, res) in sigs.items():
            f = getattr(lib, fn)
            f.argtypes, f.restype = args, res
        self._lib = lib
        cfg = json.dumps(config).encode() if config else None
        self._h = lib.tvcore_new(cfg)
        if not self._h:
            raise TvCoreError("invalid_input", "invalid config JSON")

    # -- plumbing ---------------------------------------------------------
    def _call(self, ptr: int) -> Any:
        try:
            raw = ctypes.string_at(ptr).decode("utf-8")
        finally:
            self._lib.tvcore_string_free(ptr)
        env = json.loads(raw)
        if not env["ok"]:
            raise TvCoreError(env["error"]["code"], env["error"]["message"])
        return env["data"]

    @staticmethod
    def _b(s: Optional[str]) -> Optional[bytes]:
        return None if s is None else s.encode("utf-8")

    def close(self) -> None:
        if self._h:
            self._lib.tvcore_free(self._h)
            self._h = None

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass

    # -- API ----------------------------------------------------------------
    def refresh(self) -> int:
        return self._call(self._lib.tvcore_refresh(self._h))["count"]

    def load_html(self, html: str) -> int:
        return self._call(self._lib.tvcore_load_html(self._h, self._b(html)))["count"]

    def channels(self, category: Optional[str] = None) -> List[dict]:
        return self._call(self._lib.tvcore_list(self._h, self._b(category)))

    def categories(self) -> List[dict]:
        return self._call(self._lib.tvcore_categories(self._h))

    def search(self, query: str = "", category: Optional[str] = None, limit: int = 0) -> List[dict]:
        return self._call(self._lib.tvcore_search(self._h, self._b(query), self._b(category), limit))

    def get(self, channel_id: str) -> dict:
        return self._call(self._lib.tvcore_get(self._h, self._b(channel_id)))

    def play(self, channel_id: str, verify: bool = False, force_refresh: bool = False) -> dict:
        opts = json.dumps({"verify": verify, "force_refresh": force_refresh}).encode()
        return self._call(self._lib.tvcore_play(self._h, self._b(channel_id), opts))

    def probe(self, url: str) -> dict:
        return self._call(self._lib.tvcore_probe(self._h, self._b(url)))

    def export_m3u(self) -> str:
        return self._call(self._lib.tvcore_export_m3u(self._h))
