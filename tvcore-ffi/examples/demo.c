/*
 * demo.c — end-to-end check of the TvCore C ABI.
 *
 * Build & run (after `cargo build --release` in the workspace root):
 *
 *   cc examples/demo.c -o demo -Iinclude -Ltarget/release -ltvcore \
 *      -Wl,-rpath,target/release && ./demo
 *
 * It exercises: load playlist from memory -> list -> search -> play.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "tvcore.h"

static const char *PLAYLIST =
    "#EXTM3U\n"
    "#EXTINF:-1 group-title=\"BANGLA\" tvg-logo=\"https://itcnbd.live/images/a.png\",Somoy TV\n"
    "https://s3.itcnbd.live/server-6/stream/aW1nL3BsYXkucGhw.m3u8\n"
    "#EXTINF:-1 group-title=\"SPORTS\",T Sports HD\n"
    "https://s1.itcnbd.live/T-Sports-HD/tracks-v1a1/playlist.m3u8\n"
    "#EXTINF:-1 group-title=\"SPORTS\",Star Sports 1 HD\n"
    "https://s1.itcnbd.live/Star.Sports.1.HD/tracks-v1a1/playlist.m3u8\n";

int main(void) {
    printf("TvCore %s\n", tvcore_version());

    TvCore *tv = tvcore_new();
    if (!tv) {
        fprintf(stderr, "tvcore_new failed: %s\n", tvcore_last_error());
        return 1;
    }

    int64_t n = tvcore_load_playlist_str(tv, PLAYLIST, "demo");
    if (n < 0) {
        fprintf(stderr, "load failed: %s\n", tvcore_last_error());
        return 1;
    }
    printf("loaded %lld channels\n", (long long)n);

    char *cats = tvcore_categories_json(tv);
    printf("categories: %s\n", cats ? cats : "(null)");
    tvcore_string_free(cats);

    char *hits = tvcore_search_json(tv, "spor", 5);
    printf("search 'spor': %s\n", hits ? hits : "(null)");
    tvcore_string_free(hits);

    char *play = tvcore_play_json(tv, "t sports");
    if (play) {
        printf("play: %s\n", play);
        tvcore_string_free(play);
    } else {
        fprintf(stderr, "play failed: %s\n", tvcore_last_error());
    }

    /* error path: unknown channel */
    char *bad = tvcore_play_json(tv, "does-not-exist");
    if (!bad) {
        printf("expected failure ok: %s\n", tvcore_last_error());
    }
    tvcore_string_free(bad);

    tvcore_free(tv);
    return 0;
}
