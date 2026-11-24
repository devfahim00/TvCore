//! In-memory channel catalog with dedupe/merge and category index.

use crate::model::Channel;
use crate::search::{search_channels, SearchResult};
use std::collections::HashMap;

/// Result of adding channels to the catalog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AddStats {
    /// Completely new channels added.
    pub added: usize,
    /// Known channels whose stream was merged as an alternative.
    pub merged: usize,
    /// Exact duplicates skipped (same id + same primary URL).
    pub duplicates: usize,
}

/// The catalog holds channels from one or more providers.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    channels: Vec<Channel>,
    index: HashMap<String, usize>, // id -> position in channels
    // lower-case key -> (display name, count)
    categories: HashMap<String, (String, usize)>,
}

impl Catalog {
    pub fn new() -> Self {
        Catalog::default()
    }

    /// Add channels in order. When a channel with the same `id` already
    /// exists:
    /// * same primary URL → counted as duplicate and skipped
    /// * different URL   → merged as an alternative stream
    pub fn add_channels<I: IntoIterator<Item = Channel>>(&mut self, chans: I) -> AddStats {
        let mut stats = AddStats::default();
        for ch in chans {
            if let Some(&pos) = self.index.get(&ch.id) {
                let existing = &mut self.channels[pos];
                let new_url = ch.url.clone();
                if existing.url == new_url || existing.alternatives.contains(&new_url) {
                    stats.duplicates += 1;
                    // keep richer metadata if the new entry has it
                    if existing.logo.is_none() && ch.logo.is_some() {
                        existing.logo = ch.logo.clone();
                    }
                    if existing.referer.is_none() && ch.referer.is_some() {
                        existing.referer = ch.referer.clone();
                    }
                    if existing.user_agent.is_none() && ch.user_agent.is_some() {
                        existing.user_agent = ch.user_agent.clone();
                    }
                    continue;
                }
                existing.alternatives.push(new_url);
                if existing.logo.is_none() && ch.logo.is_some() {
                    existing.logo = ch.logo.clone();
                }
                if existing.referer.is_none() && ch.referer.is_some() {
                    existing.referer = ch.referer.clone();
                }
                if existing.user_agent.is_none() && ch.user_agent.is_some() {
                    existing.user_agent = ch.user_agent.clone();
                }
                stats.merged += 1;
            } else {
                let key = category_key(&ch.category);
                match self.categories.get_mut(&key) {
                    Some((_, count)) => *count += 1,
                    None => {
                        self.categories.insert(key, (ch.category.clone(), 1));
                    }
                }
                self.index.insert(ch.id.clone(), self.channels.len());
                self.channels.push(ch);
                stats.added += 1;
            }
        }
        stats
    }

    /// All channels in insertion order.
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    /// Find a channel by id.
    pub fn find(&self, id: &str) -> Option<&Channel> {
        self.index.get(id).map(|&i| &self.channels[i])
    }

    /// Category names (display form preserved) with counts, sorted by
    /// count descending then name.
    pub fn categories(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = self
            .categories
            .iter()
            .map(|(_, (name, count))| (name.clone(), *count))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// Channels of one category (case-insensitive).
    pub fn channels_in_category(&self, category: &str) -> Vec<&Channel> {
        let key = category_key(category);
        self.channels
            .iter()
            .filter(|c| category_key(&c.category) == key)
            .collect()
    }

    /// Ranked search (see [`crate::search`]).
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        search_channels(&self.channels, query, limit)
    }

    /// Resolve the search result list into channel references.
    pub fn resolve<'a>(&'a self, results: &[SearchResult]) -> Vec<&'a Channel> {
        results.iter().map(|r| &self.channels[r.index]).collect()
    }
}

/// Case-insensitive, whitespace-normalized category key.
fn category_key(cat: &str) -> String {
    cat.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(id: &str, url: &str) -> Channel {
        Channel::new(id, "Some TV", "BANGLA", url)
    }

    #[test]
    fn add_and_find() {
        let mut c = Catalog::new();
        let s = c.add_channels(vec![ch("somoy", "https://x/1.m3u8")]);
        assert_eq!(s.added, 1);
        assert_eq!(c.len(), 1);
        assert!(c.find("somoy").is_some());
        assert!(c.find("nope").is_none());
    }

    #[test]
    fn merge_alternatives() {
        let mut c = Catalog::new();
        let s = c.add_channels(vec![
            ch("somoy", "https://x/1.m3u8"),
            ch("somoy", "https://y/2.m3u8"),
        ]);
        assert_eq!(s.added, 1);
        assert_eq!(s.merged, 1);
        assert_eq!(c.len(), 1);
        assert_eq!(c.channels()[0].alternatives, vec!["https://y/2.m3u8"]);
    }

    #[test]
    fn duplicate_skipped() {
        let mut c = Catalog::new();
        let s = c.add_channels(vec![
            ch("somoy", "https://x/1.m3u8"),
            ch("somoy", "https://x/1.m3u8"),
        ]);
        assert_eq!(s.added, 1);
        assert_eq!(s.duplicates, 1);
    }

    #[test]
    fn categories_sorted() {
        let mut c = Catalog::new();
        let mut a = ch("1", "u1");
        a.category = "SPORTS".into();
        let mut b = ch("2", "u2");
        b.category = "SPORTS".into();
        let mut d = ch("3", "u3");
        d.category = "BANGLA".into();
        c.add_channels(vec![a, b, d]);
        let cats = c.categories();
        assert_eq!(cats[0], ("SPORTS".to_string(), 2));
        assert_eq!(cats[1], ("BANGLA".to_string(), 1));
    }

    #[test]
    fn category_filter_case_insensitive() {
        let mut c = Catalog::new();
        let mut a = ch("1", "u1");
        a.category = "Sports".into();
        c.add_channels(vec![a]);
        assert_eq!(c.channels_in_category("SPORTS").len(), 1);
        assert_eq!(c.channels_in_category("news").len(), 0);
    }
}
