use std::process::ExitCode;
use tvcore::{Config, PlayOptions, SearchQuery, TvCore};

const HELP: &str = "\
tvcore - list, search and play channels from itcnbd.live

USAGE:
  tvcore [--html FILE] [--base URL] [--json] <COMMAND>

COMMANDS:
  refresh                      fetch the site and print the channel count
  categories                   list categories with counts
  list [CATEGORY]              list channels
  search <QUERY> [--category C] [--limit N]
  play <ID> [--verify]         print the stream URL and headers (use as: mpv --http-header-fields=...)
  probe <URL>                  download an HLS playlist and show its variants
  m3u                          print an M3U playlist

OPTIONS:
  --html FILE   read the channel page from a saved HTML file instead of the network
  --base URL    site root (default https://itcnbd.live/)
  --json        machine readable output
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(i) = args.iter().position(|a| a == name) {
        args.remove(i);
        true
    } else {
        false
    }
}

fn take_opt(args: &mut Vec<String>, name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || take_flag(&mut args, "--help") || take_flag(&mut args, "-h") {
        print!("{HELP}");
        return Ok(());
    }
    let json = take_flag(&mut args, "--json");
    let html = take_opt(&mut args, "--html");
    let mut cfg = Config::default();
    if let Some(b) = take_opt(&mut args, "--base") {
        cfg.base_url = b;
    }
    let core = TvCore::new(cfg);
    if let Some(path) = html {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
        core.load_html(&text).map_err(|e| e.to_string())?;
    }

    let cmd = args.remove(0);
    let print_json =
        |v: serde_json::Value| println!("{}", serde_json::to_string_pretty(&v).unwrap());

    match cmd.as_str() {
        "refresh" => {
            let n = core.refresh().map_err(|e| e.to_string())?;
            println!("{n} channels");
        }
        "categories" => {
            let cats = core.categories().map_err(|e| e.to_string())?;
            if json {
                print_json(serde_json::to_value(&cats).unwrap());
            } else {
                for c in cats {
                    println!("{:<16} {}", c.name, c.count);
                }
            }
        }
        "list" | "search" => {
            let mut q = SearchQuery::default();
            if let Some(c) = take_opt(&mut args, "--category") {
                q.category = Some(c);
            }
            if let Some(n) = take_opt(&mut args, "--limit") {
                q.limit = Some(
                    n.parse()
                        .map_err(|_| "--limit needs a number".to_string())?,
                );
            }
            if let Some(first) = args.first() {
                if cmd == "search" {
                    q.query = first.clone();
                } else {
                    q.category = Some(first.clone());
                }
            } else if cmd == "search" {
                return Err("search needs a query".into());
            }
            let hits = core.search(&q);
            if json {
                print_json(serde_json::to_value(&hits).unwrap());
            } else {
                for c in &hits {
                    println!("{:<28} {:<14} {}", c.id, c.category, c.title);
                }
                eprintln!("{} result(s)", hits.len());
            }
        }
        "play" => {
            let verify = take_flag(&mut args, "--verify");
            let id = args.first().ok_or("play needs a channel id")?;
            let info = core
                .play(
                    id,
                    &PlayOptions {
                        verify,
                        force_refresh: false,
                    },
                )
                .map_err(|e| e.to_string())?;
            if json {
                print_json(serde_json::to_value(&info).unwrap());
            } else {
                println!("{}", info.url);
                for (k, v) in &info.headers {
                    println!("{k}: {v}");
                }
            }
        }
        "probe" => {
            let url = args.first().ok_or("probe needs a url")?;
            let r = core.probe(url).map_err(|e| e.to_string())?;
            print_json(serde_json::to_value(&r).unwrap());
        }
        "m3u" => print!("{}", core.export_m3u().map_err(|e| e.to_string())?),
        other => return Err(format!("unknown command `{other}` (try --help)")),
    }
    Ok(())
}
