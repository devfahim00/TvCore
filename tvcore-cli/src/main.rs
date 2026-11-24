//! `tvcore-cli` — command line front-end for TvCore.
//!
//! ```text
//! tvcore-cli list [--category SPORTS] [--limit N] [--json]
//! tvcore-cli categories [--json]
//! tvcore-cli search "t sports" [--json]
//! tvcore-cli play "somoy tv" [--probe]
//! tvcore-cli probe "somoy tv"
//!
//! options:
//!   --source <url|path>   load this playlist instead of the ITCN BD one
//!   --offline             demo mode using the bundled snapshot playlist
//!   --timeout <secs>     HTTP timeout (default 30)
//! ```

use std::process::ExitCode;
use tvcore::http::UreqFetcher;
use tvcore::{Channel, StreamKind, TvCore};

const DEMO_PLAYLIST: &str = include_str!("../../assets/demo_playlist.m3u");

const USAGE: &str = "\
tvcore-cli — live TV channel listing / search / play (TvCore)

USAGE:
  tvcore-cli [options] <command> [args]

COMMANDS:
  list [pattern]          list channels (optional substring filter)
  categories              list categories with channel counts
  search <query...>       ranked channel search
  play <channel>          resolve the playable stream (id or name)
  probe <channel>         check whether the stream answers

OPTIONS:
  --source <url|path>     remote M3U/M3U8 URL or local playlist file
  --category <name>       filter listing by category (case-insensitive)
  --limit <n>             cap the number of results (default 100)
  --timeout <secs>        HTTP timeout seconds (default 30)
  --json                  machine-readable JSON output
  --probe                 with `play`, also probe the stream
  --offline              demo mode: bundled snapshot playlist, no network
  --help, -h              this help

EXAMPLES:
  tvcore-cli list --category sports
  tvcore-cli search \"sony ten\"
  tvcore-cli play \"somoy tv\"
  tvcore-cli --source https://iptv-org.github.io/iptv/countries/bd.m3u search news
";

struct Args {
    command: String,
    command_args: Vec<String>,
    source: Option<String>,
    category: Option<String>,
    limit: usize,
    timeout: u64,
    json: bool,
    probe: bool,
    offline: bool,
}

fn parse_args<I: Iterator<Item = String>>(mut it: I) -> Result<Args, String> {
    let mut command = String::new();
    let mut command_args: Vec<String> = Vec::new();
    let mut source = None;
    let mut category = None;
    let mut limit = 100usize;
    let mut timeout = 30u64;
    let mut json = false;
    let mut probe = false;
    let mut offline = false;

    fn value<I: Iterator<Item = String>>(it: &mut I, flag: &str) -> Result<String, String> {
        it.next().ok_or_else(|| format!("{} expects a value", flag))
    }

    while let Some(arg) = it.next() {
        if arg == "--help" || arg == "-h" {
            return Err(String::new()); // prints usage
        }
        if let Some(flag) = arg.strip_prefix("--") {
            match flag {
                "json" => json = true,
                "probe" => probe = true,
                "offline" => offline = true,
                "source" => {
                    let v = value(&mut it, "--source")?;
                    source = Some(v);
                }
                "category" => {
                    let v = value(&mut it, "--category")?;
                    category = Some(v);
                }
                "limit" => {
                    let v = value(&mut it, "--limit")?;
                    limit = v
                        .parse()
                        .map_err(|_| format!("--limit expects a number, got {:?}", v))?;
                }
                "timeout" => {
                    let v = value(&mut it, "--timeout")?;
                    timeout = v
                        .parse()
                        .map_err(|_| format!("--timeout expects seconds, got {:?}", v))?;
                }
                other => return Err(format!("unknown option --{}", other)),
            }
            continue;
        }
        if command.is_empty() {
            command = arg;
        } else {
            command_args.push(arg);
        }
    }

    if command.is_empty() {
        return Err(String::new());
    }
    if command == "search" && command_args.is_empty() {
        return Err("search needs a query".into());
    }
    Ok(Args {
        command,
        command_args,
        source,
        category,
        limit,
        timeout,
        json,
        probe,
        offline,
    })
}

fn build_tvcore(args: &Args) -> TvCore {
    let fetcher = UreqFetcher::with_timeout_and_headers(
        std::time::Duration::from_secs(args.timeout),
        tvcore::http::default_headers(),
    );
    TvCore::with_http(std::sync::Arc::new(fetcher))
}

/// Load channels into a fresh TvCore according to --source / --offline.
fn load(args: &Args, tv: &mut TvCore) -> Result<(), String> {
    if args.offline {
        let n = tv.load_playlist_str(DEMO_PLAYLIST, "demo");
        eprintln!(
            "[offline demo] loaded {} channels from the bundled snapshot",
            n
        );
        return Ok(());
    }
    match &args.source {
        Some(s) => {
            if s.starts_with("http://") || s.starts_with("https://") {
                let n = tv.load_playlist_url(s).map_err(|e| e.to_string())?;
                eprintln!("[source {}] loaded {} channels", s, n);
            } else {
                let n = tv.load_playlist_file(s).map_err(|e| e.to_string())?;
                eprintln!("[file {}] loaded {} channels", s, n);
            }
            Ok(())
        }
        None => {
            let n = tv.load_itcnbd().map_err(|e| {
                format!(
                    "failed to load the ITCN BD playlist ({}).\n\
                     hint: itcnbd serves BDIX networks — use --source <playlist-url> \
                     or --offline for a demo, or run from a Bangladesh ISP network",
                    e
                )
            })?;
            eprintln!("[itcnbd] loaded {} channels", n);
            Ok(())
        }
    }
}

fn channel_json(ch: &Channel) -> serde_json::Value {
    serde_json::to_value(ch).unwrap_or(serde_json::Value::Null)
}

fn kind_emoji(kind: StreamKind) -> &'static str {
    match kind {
        StreamKind::Hls => "HLS",
        StreamKind::Dash => "DASH",
        StreamKind::Ts => "TS",
        StreamKind::Unknown => "?",
    }
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("error: {}", e);
                eprintln!();
            }
            println!("{}", USAGE);
            return ExitCode::from(if e.is_empty() { 0 } else { 2 });
        }
    };

    let mut tv = build_tvcore(&args);
    if let Err(e) = load(&args, &mut tv) {
        eprintln!("error: {}", e);
        return ExitCode::from(1);
    }

    match args.command.as_str() {
        "categories" => cmd_categories(&tv, &args),
        "list" => cmd_list(&tv, &args),
        "search" => cmd_search(&tv, &args),
        "play" => match cmd_play(&tv, &args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::from(1)
            }
        },
        "probe" => match cmd_probe(&tv, &args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::from(1)
            }
        },
        other => {
            eprintln!("error: unknown command {:?}", other);
            ExitCode::from(2)
        }
    }
}

fn cmd_categories(tv: &TvCore, args: &Args) -> ExitCode {
    let cats = tv.categories();
    if args.json {
        let json = serde_json::json!({
            "count": cats.len(),
            "categories": cats.iter()
                .map(|(n, c)| serde_json::json!({"name": n, "channels": c}))
                .collect::<Vec<_>>(),
        });
        println!("{}", json);
    } else {
        println!("CATEGORIES ({})", cats.len());
        for (name, count) in cats {
            println!("  {:<20} {:>4} channels", name, count);
        }
    }
    ExitCode::SUCCESS
}

fn cmd_list(tv: &TvCore, args: &Args) -> ExitCode {
    let pattern = args.command_args.first().map(|s| s.to_lowercase());
    let mut chans: Vec<&Channel> = match (&args.category, &pattern) {
        (Some(c), _) => tv.channels_in_category(c),
        (None, Some(p)) => tv
            .channels()
            .iter()
            .filter(|c| c.name.to_lowercase().contains(p.as_str()))
            .collect(),
        (None, None) => tv.channels().iter().collect(),
    };
    chans.truncate(args.limit);

    if args.json {
        let json = serde_json::json!({
            "count": chans.len(),
            "channels": chans.iter().map(|c| channel_json(c)).collect::<Vec<_>>(),
        });
        println!("{}", json);
    } else {
        println!("CHANNELS ({} shown of {})", chans.len(), tv.channel_count());
        println!("{:<32} {:<15} NAME", "ID", "CATEGORY");
        for ch in &chans {
            println!("{:<32} {:<15} {}", ch.id, ch.category, ch.name);
        }
    }
    ExitCode::SUCCESS
}

fn cmd_search(tv: &TvCore, args: &Args) -> ExitCode {
    let query = args.command_args.join(" ");
    let hits = tv.search(&query, args.limit);
    if args.json {
        let json = serde_json::json!({
            "query": query,
            "count": hits.len(),
            "channels": hits.iter().map(channel_json).collect::<Vec<_>>(),
        });
        println!("{}", json);
    } else {
        println!("SEARCH {:?} — {} hits", query, hits.len());
        for (rank, ch) in hits.iter().enumerate() {
            println!(
                "{:>3}. {:<32} {:<15} {}",
                rank + 1,
                ch.id,
                ch.category,
                ch.name
            );
        }
    }
    ExitCode::SUCCESS
}

fn cmd_play(tv: &TvCore, args: &Args) -> Result<(), String> {
    let key = args.command_args.join(" ");
    if key.trim().is_empty() {
        return Err("play needs a channel id or name".into());
    }
    let target = tv.play_any(&key).map_err(|e| e.to_string())?;
    if args.json {
        let mut v = serde_json::to_value(&target).unwrap_or(serde_json::Value::Null);
        if args.probe {
            let status = tv.probe(&target.channel_id).ok();
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "probe".into(),
                    status
                        .and_then(|s| serde_json::to_value(s).ok())
                        .unwrap_or(serde_json::Value::Null),
                );
            }
        }
        println!("{}", v);
        return Ok(());
    }
    println!("PLAY  {}  [{}]", target.channel_name, target.channel_id);
    println!("  url:     {}", target.url);
    println!("  kind:    {}", kind_emoji(target.kind));
    for (k, v) in &target.headers {
        println!("  header:  {}: {}", k, v);
    }
    if let Some(alts) = tv.find(&target.channel_id).map(|c| c.alternatives.len()) {
        if alts > 0 {
            println!("  mirrors: {} alternative stream(s)", alts);
        }
    }
    if args.probe {
        let status = tv.probe(&target.channel_id).map_err(|e| e.to_string())?;
        println!(
            "  probe:   HTTP {} ({})",
            status.http_status,
            if status.reachable {
                "reachable"
            } else {
                "unreachable"
            }
        );
    }
    Ok(())
}

fn cmd_probe(tv: &TvCore, args: &Args) -> Result<(), String> {
    let key = args.command_args.join(" ");
    if key.trim().is_empty() {
        return Err("probe needs a channel id or name".into());
    }
    let ch = tv
        .find_any(&key)
        .ok_or_else(|| format!("no channel matches {:?}", key))?
        .clone();
    let status = tv.probe(&ch.id).map_err(|e| e.to_string())?;
    if args.json {
        println!("{}", serde_json::to_string(&status).unwrap_or_default());
    } else {
        println!(
            "PROBE {} — HTTP {} ({})",
            ch.name,
            status.http_status,
            if status.reachable {
                "reachable"
            } else {
                "unreachable"
            }
        );
    }
    Ok(())
}
