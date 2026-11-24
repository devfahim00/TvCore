/// Extracts the expiry (unix seconds) from a signed stream URL.
///
/// URLs look like `...?token=<hash>-<hash>-<expiry>-<issued>&remote=no_check_ip`.
/// The last two dash separated parts are timestamps (observed gap: 4 hours), the larger one
/// is the expiry. Only the last two parts are inspected because the hash parts may
/// accidentally consist of digits only.
pub fn expiry_from_url(url: &str) -> Option<i64> {
    let parsed = url::Url::parse(url).ok()?;
    let token = parsed
        .query_pairs()
        .find(|(k, _)| k == "token")?
        .1
        .into_owned();
    let parts: Vec<&str> = token.split('-').collect();
    if parts.len() < 2 {
        return None;
    }
    let ts: Vec<i64> = parts[parts.len() - 2..]
        .iter()
        .filter_map(|p| {
            if (9..=11).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()) {
                p.parse::<i64>().ok()
            } else {
                None
            }
        })
        .filter(|v| (1_000_000_000..4_000_000_000).contains(v))
        .collect();
    ts.into_iter().max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_expiry() {
        let u = "https://s1.itcnbd.live/JAMUNA/index.m3u8?token=c50740477a0d796ee76c658955bec8106ff7a5bd-e79342afa3b2a114-1795508732-1795494332&remote=no_check_ip";
        assert_eq!(expiry_from_url(u), Some(1795508732));
    }

    #[test]
    fn no_token_means_none() {
        assert_eq!(
            expiry_from_url("https://s3.itcnbd.live/server-6/stream/abc.m3u8"),
            None
        );
        assert_eq!(expiry_from_url("not a url"), None);
    }

    #[test]
    fn digit_only_hash_is_ignored() {
        let u = "https://x/y.m3u8?token=1234567890123456-9999999999999999-1795508732-1795494332";
        assert_eq!(expiry_from_url(u), Some(1795508732));
    }
}
