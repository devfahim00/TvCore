"""Smoke test for the C ABI through ctypes. Run: python3 bindings/python/test_tvcore.py"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from tvcore import TvCore, TvCoreError  # noqa: E402

FIXTURE = os.path.join(os.path.dirname(__file__), "..", "..", "crates", "tvcore", "tests", "fixtures", "home.html")


def main() -> None:
    html = open(FIXTURE, encoding="utf-8").read()
    with TvCore() as core:
        assert core.load_html(html) == 75
        assert len(core.channels()) == 75
        assert len(core.channels("kids")) == 8
        cats = {c["name"]: c["count"] for c in core.categories()}
        assert cats["BANGLA"] == 23 and len(cats) == 10

        hits = core.search("jamuna", limit=3)
        assert hits[0]["id"] == "jamuna-tv", hits
        assert core.search("starsports")[0]["title"] == "Star Sports 1 HD"

        play = core.play("disney")  # token-less stream, no network needed
        assert play["url"].startswith("https://s3.itcnbd.live/")
        assert play["headers"]["Referer"] == "https://itcnbd.live/"

        assert core.export_m3u().startswith("#EXTM3U")

        try:
            core.get("nope")
        except TvCoreError as e:
            assert e.code == "not_found"
        else:
            raise AssertionError("expected TvCoreError")
    print("python binding OK")


if __name__ == "__main__":
    main()
