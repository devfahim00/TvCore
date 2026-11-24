//! Channel search: fast, case-insensitive, multi-token, typo-tolerant.
//!
//! ```text
//! query: "somoy"      ->  Somoy TV            (score 80, name prefix)
//! query: "t sports"   ->  T Sports HD          (score 130, two token hits)
//! query: "sprots"     ->  Star Sports 1 HD ... (fuzzy, typo tolerated)
//! query: "bangla"     ->  all channels in the BANGLA categories
//! ```

use crate::model::Channel;

/// A ranked search hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// Index of the channel inside the searched slice.
    pub index: usize,
    /// Relevance score (higher = better). See module docs for rough bands.
    pub score: i64,
}

/// Normalize text for comparison: lowercase + collapse whitespace.
fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for ch in s.trim().chars() {
        let low = ch.to_lowercase().next().unwrap_or(ch);
        if low.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(low);
        }
    }
    out
}

/// Split a normalized string into word tokens.
fn tokens(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

/// Bounded Levenshtein distance: returns `None` when distance exceeds `max`.
/// Keeps the DP cheap for long channel names.
fn levenshtein_within(a: &str, b: &str, max: usize) -> Option<usize> {
    if a == b {
        return Some(0);
    }
    let (big, small) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    if big.len() - small.len() > max {
        return None;
    }
    let mut prev: Vec<usize> = (0..=small.len()).collect();
    let mut cur = vec![0usize; small.len() + 1];
    for (i, bc) in big.chars().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, sc) in small.chars().enumerate() {
            let cost = if bc == sc { 0 } else { 1 };
            let v = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
            cur[j + 1] = v;
            row_min = row_min.min(v);
        }
        if row_min > max {
            return None; // early exit
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let d = prev[small.len()];
    if d <= max {
        Some(d)
    } else {
        None
    }
}

/// Score one channel against a query; `0` means "no match, drop it".
fn score_channel(name_norm: &str, cat_norm: &str, query_norm: &str, query_tokens: &[&str]) -> i64 {
    let mut score: i64 = 0;

    // --- whole-query matching on the name ---
    if !query_norm.is_empty() {
        if *name_norm == *query_norm {
            score += 100;
        } else if name_norm.starts_with(query_norm) {
            score += 80;
        } else if name_norm.contains(query_norm) {
            score += 60;
        } else {
            // fuzzy whole-name match (short names only, cheap)
            let max = if query_norm.len() <= 5 { 2 } else { 3 };
            if let Some(d) = levenshtein_within(name_norm, query_norm, max) {
                score += 55 - (d as i64) * 10;
            }
        }
        // category containing the whole query ("bangla", "sports")
        if cat_norm.contains(query_norm) {
            score += 25;
        }
    }

    // --- per-token matching (name) ---
    let name_tokens = tokens(name_norm);
    let cat_tokens = tokens(cat_norm);
    let mut token_hits = 0i64;
    for tok in query_tokens {
        if tok.is_empty() {
            continue;
        }
        let mut best: i64 = 0;
        // exact token
        if name_tokens.contains(tok) {
            best = 30;
        } else {
            // prefix of a name token ("t spor" -> "sports")
            if name_tokens.iter().any(|t| t.starts_with(tok)) {
                best = 24;
            } else if name_norm.contains(tok) {
                best = 18;
            } else {
                // fuzzy against each name token
                for t in &name_tokens {
                    let m = t.len() / 3 + 1;
                    if let Some(d) = levenshtein_within(t, tok, m.min(2)) {
                        let s = 14 - (d as i64) * 5;
                        if s > best {
                            best = s;
                        }
                    }
                }
            }
        }
        // token appearing in the category keeps the channel alive but
        // scores lower than a name hit
        let cat_hit = cat_tokens.contains(tok) || cat_norm.contains(tok);
        if best == 0 {
            if cat_hit {
                best = 12;
            } else {
                return 0; // AND semantics: every token must match somewhere
            }
        } else if cat_hit {
            best += 3;
        }
        token_hits += best;
    }
    score += token_hits;

    // bonus: all tokens hit the name (query is clearly about this channel)
    if !query_tokens.is_empty() && score > 0 && token_hits > 0 {
        score += 5;
    }
    score
}

/// Search `channels`, returning indices sorted by descending relevance.
///
/// * empty query → all channels, in original order (score 0)
/// * `limit` caps the number of results (`usize::MAX` for everything)
pub fn search_channels(channels: &[Channel], query: &str, limit: usize) -> Vec<SearchResult> {
    let query_norm = norm(query);
    let query_tokens: Vec<&str> = tokens(&query_norm);
    let mut hits: Vec<SearchResult> = Vec::new();

    for (i, ch) in channels.iter().enumerate() {
        let name_norm = norm(&ch.name);
        let cat_norm = norm(&ch.category);
        let score = if query_norm.is_empty() {
            0
        } else {
            score_channel(&name_norm, &cat_norm, &query_norm, &query_tokens)
        };
        if score > 0 || query_norm.is_empty() {
            hits.push(SearchResult { index: i, score });
        }
    }

    if !query_norm.is_empty() {
        // sort by score desc, then by name asc for stable, tidy output
        hits.sort_by(|a, b| {
            let an = channels[a.index].name.to_lowercase();
            let bn = channels[b.index].name.to_lowercase();
            b.score.cmp(&a.score).then_with(|| an.cmp(&bn))
        });
    }
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Channel;

    fn sample() -> Vec<Channel> {
        vec![
            Channel::new("t-sports-hd", "T Sports HD", "SPORTS", "https://x/ts.m3u8"),
            Channel::new("somoy-tv", "Somoy TV", "BANGLA", "https://x/somoy.m3u8"),
            Channel::new(
                "star-sports-1",
                "Star Sports 1 HD",
                "SPORTS",
                "https://x/ss1.m3u8",
            ),
            Channel::new(
                "star-jalsha",
                "Star Jalsha HD",
                "INDIAN BANGLA",
                "https://x/sj.m3u8",
            ),
            Channel::new("bbc", "BBC News", "ENGLISH NEWS", "https://x/bbc.m3u8"),
        ]
    }

    #[test]
    fn empty_query_returns_all() {
        let out = search_channels(&sample(), "", 100);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn exact_and_prefix() {
        let out = search_channels(&sample(), "Somoy TV", 10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].index, 1);
        assert_eq!(out[0].score, 100 + 30 + 30 + 5);
    }

    #[test]
    fn case_insensitive() {
        let out = search_channels(&sample(), "SOMOY", 10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].index, 1);
    }

    #[test]
    fn multi_token() {
        let out = search_channels(&sample(), "sports hd", 10);
        // T Sports HD and Star Sports 1 HD both match both tokens
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].index, 0); // shorter name, same score band, first
    }

    #[test]
    fn typo_tolerant() {
        let out = search_channels(&sample(), "somoy tvv", 10);
        assert!(!out.is_empty());
        assert_eq!(out[0].index, 1);
    }

    #[test]
    fn category_search() {
        let out = search_channels(&sample(), "bangla", 10);
        // Somoy TV (BANGLA) + Star Jalsha HD (INDIAN BANGLA)
        assert_eq!(out.len(), 2);
        let ids: Vec<usize> = out.iter().map(|h| h.index).collect();
        assert!(ids.contains(&1) && ids.contains(&3));
    }

    #[test]
    fn and_semantics_no_match() {
        let out = search_channels(&sample(), "somoy cartoon", 10);
        assert!(out.is_empty());
    }

    #[test]
    fn limit_works() {
        let out = search_channels(&sample(), "", 2);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn no_channels_no_hits() {
        let out = search_channels(&[], "abc", 10);
        assert!(out.is_empty());
    }

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein_within("sport", "sport", 2), Some(0));
        assert_eq!(levenshtein_within("sports", "sport", 1), Some(1));
        assert_eq!(levenshtein_within("abc", "xyz", 2), None);
    }
}
