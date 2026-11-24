#ifndef TVCORE_H
#define TVCORE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque, thread-safe handle. */
typedef struct TvCore TvCore;

/*
 * Every function returning char* returns a JSON envelope owned by the library:
 *   {"ok":true,"data":...}  |  {"ok":false,"error":{"code":"...","message":"..."}}
 * Free it with tvcore_string_free(). Error codes:
 *   parse, network, status, not_found, invalid_input, no_network, panic
 */

/* Static string, do NOT free. */
const char *tvcore_version(void);

/* config_json: NULL for defaults, or e.g. {"base_url":"https://itcnbd.live/","timeout_secs":10}.
 * Returns NULL if the JSON is invalid. */
TvCore *tvcore_new(const char *config_json);
void tvcore_free(TvCore *handle);
void tvcore_string_free(char *s);

/* Fetch the site, rebuild the list.            data: {"count":n}   (blocks on network) */
char *tvcore_refresh(const TvCore *handle);
/* Build the list from HTML you fetched.        data: {"count":n}   (no network) */
char *tvcore_load_html(const TvCore *handle, const char *html);

/* data: [Channel]   category may be NULL/"" for all */
char *tvcore_list(const TvCore *handle, const char *category);
/* data: [{"name","count"}] */
char *tvcore_categories(const TvCore *handle);
/* data: [Channel] ranked; query/category may be NULL; limit <= 0 = unlimited */
char *tvcore_search(const TvCore *handle, const char *query, const char *category, int32_t limit);
/* data: Channel */
char *tvcore_get(const TvCore *handle, const char *id);
/* data: PlayInfo {id,title,category,url,headers,expires_at,refreshed,expired,kind}
 * options_json: NULL or {"verify":bool,"force_refresh":bool} */
char *tvcore_play(const TvCore *handle, const char *id, const char *options_json);
/* data: ProbeResult {status,ok,kind,variants[]} */
char *tvcore_probe(const TvCore *handle, const char *url);
/* data: M3U text (JSON string) */
char *tvcore_export_m3u(const TvCore *handle);

#ifdef __cplusplus
}
#endif

#endif /* TVCORE_H */
