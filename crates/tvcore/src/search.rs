use crate::model::{Channel, SearchQuery};

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

/// Relevance of a channel for a query (0 = no match). Higher is better.
fn score(q_words: &[String], q_joined: &str, ch: &Channel) -> u32 {
    let t_words = words(&ch.title);
    let t_joined: String = t_words.concat();
    if t_joined == q_joined {
        return 100;
    }
    if t_joined.starts_with(q_joined) {
        return 85;
    }
    if t_words.iter().any(|w| w.starts_with(q_joined)) {
        return 70;
    }
    if t_joined.contains(q_joined) {
        return 55;
    }
    if q_words.len() > 1 && q_words.iter().all(|w| t_joined.contains(w.as_str())) {
        return 45;
    }
    let c_joined: String = words(&ch.category).concat();
    if !c_joined.is_empty() && c_joined.contains(q_joined) {
        return 30;
    }
    if q_joined.chars().count() >= 3 && is_subsequence(q_joined, &t_joined) {
        return 15;
    }
    0
}

/// Search channels. Empty query returns everything (still filtered by category / limit).
/// Matching ignores case, spaces and punctuation (`"starsports"` finds `"Star Sports 1 HD"`).
pub fn search(channels: &[Channel], q: &SearchQuery) -> Vec<Channel> {
    let cat = q
        .category
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_lowercase);
    let in_cat = |c: &Channel| {
        cat.as_ref()
            .map_or(true, |k| c.category.to_lowercase() == *k)
    };

    let q_words = words(&q.query);
    let q_joined: String = q_words.concat();

    let mut hits: Vec<(u32, usize, &Channel)> = channels
        .iter()
        .enumerate()
        .filter(|(_, c)| in_cat(c))
        .filter_map(|(i, c)| {
            if q_joined.is_empty() {
                Some((0, i, c))
            } else {
                let s = score(&q_words, &q_joined, c);
                (s > 0).then_some((s, i, c))
            }
        })
        .collect();

    // best score first, page order as tie-breaker
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let iter = hits.into_iter().map(|(_, _, c)| c.clone());
    match q.limit {
        Some(n) => iter.take(n).collect(),
        None => iter.collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(title: &str, cat: &str) -> Channel {
        Channel {
            id: crate::parse::slugify(title),
            title: title.into(),
            category: cat.into(),
            logo: None,
            stream_url: "https://x/y.m3u8".into(),
            expires_at: None,
        }
    }

    fn q(s: &str) -> SearchQuery {
        SearchQuery {
            query: s.into(),
            ..Default::default()
        }
    }

    #[test]
    fn ranking() {
        let list = vec![
            ch("Star Sports 1 HD", "SPORTS"),
            ch("Star Plus", "HINDI"),
            ch("Sony Sports 1", "SPORTS"),
            ch("NTV", "BANGLA"),
        ];
        let r = search(&list, &q("star"));
        assert_eq!(r[0].title, "Star Sports 1 HD"); // prefix, earlier in page
        assert_eq!(r[1].title, "Star Plus");
        assert_eq!(search(&list, &q("sports"))[0].title, "Star Sports 1 HD"); // word prefix
        assert_eq!(search(&list, &q("starsports"))[0].title, "Star Sports 1 HD"); // no spaces
        assert_eq!(search(&list, &q("ntv")).len(), 1);
        assert_eq!(search(&list, &q("sony 1"))[0].title, "Sony Sports 1"); // all words
        assert!(search(&list, &q("zzzz")).is_empty());
    }

    #[test]
    fn category_and_limit() {
        let list = vec![ch("A", "SPORTS"), ch("B", "SPORTS"), ch("C", "KIDS")];
        let r = search(
            &list,
            &SearchQuery {
                query: "".into(),
                category: Some("sports".into()),
                limit: Some(1),
            },
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "A");
        // category name as query
        assert_eq!(search(&list, &q("kids"))[0].title, "C");
    }
}
