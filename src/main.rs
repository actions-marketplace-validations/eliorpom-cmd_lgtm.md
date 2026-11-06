//! LGTM.md: animated pixel-art widgets for a GitHub profile README.
//!
//! Usage: lgtm-md <data.json> [names...] [--out DIR] [--stills] [--loop] [--raw DIR] [--raw-only]
//!        lgtm-md collect <login> [out.json] [friend...]
//!        lgtm-md duel-logins <names...>   (rivals to collect for these widget names)
//!        lgtm-md pycheck <cases.json>   (parity check of the Python primitives)

// The widgets were first drawn in Python, and the frames must match those prototypes pixel
// for pixel: min/max chains, float comparisons and argument lists are kept as they were written
// there, on purpose.
#![allow(
    clippy::manual_clamp,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::needless_range_loop,
    clippy::unnecessary_sort_by,
    clippy::field_reassign_with_default
)]

const USAGE: &str = "\
LGTM.md: animated pixel-art widgets for a GitHub profile README.

Usage:
  lgtm-md collect <login> [data.json] [friend...]   read an account's activity from GitHub
  lgtm-md <data.json> [widget...] [--out DIR]       draw the widgets as GIFs (default: out/)

Options:
  --out DIR    folder for the GIFs
  --stills     also write a still PNG of each widget
  --loop       loop every animation, including the hourglass that plays once
  -h, --help   show this help

Without widget names, every widget except the duels is drawn. Duels: duel-<login>,
duel-<login>-7 or duel-<login>-365, for a friend passed to collect.
The token comes from GITHUB_TOKEN, else GH_TOKEN, else `gh auth token`.
Widgets: flame counter hourglass ecg departures donut grass cherry-tree forest
         fireworks fireworks-commits records median duel-<login>
";

mod brand;
mod collect;
mod data;
mod dates;
mod export;
mod pixel;
mod pymath;
mod pycheck;
mod pyrand;
mod widgets;

use std::path::PathBuf;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("collect") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        if let Err(error) = collect::run(&args) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("duel-logins") {
        // The rivals to collect for these widget names, once each (used by action.yml).
        let mut logins: Vec<String> = Vec::new();
        for name in std::env::args().skip(2) {
            if let Some((login, _)) = widgets::parse_duel(&name)
                && !logins.contains(&login) {
                    logins.push(login);
                }
        }
        for login in logins {
            println!("{login}");
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("pycheck") {
        pycheck::run(&PathBuf::from(std::env::args().nth(2).expect("pycheck needs a cases file")));
        return;
    }
    if std::env::args().skip(1).any(|arg| arg == "--help" || arg == "-h") {
        print!("{USAGE}");
        return;
    }
    let mut args = std::env::args().skip(1);
    let mut data_path = None;
    let mut names = Vec::new();
    let mut out = PathBuf::from("out");
    let mut raw = None;
    let mut raw_only = false;
    let mut stills = false;
    let mut loop_all = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next().expect("--out needs a directory")),
            "--raw" => raw = Some(PathBuf::from(args.next().expect("--raw needs a directory"))),
            "--raw-only" => raw_only = true,
            "--stills" => stills = true,
            "--loop" => loop_all = true,
            _ if data_path.is_none() => data_path = Some(PathBuf::from(arg)),
            _ => names.push(arg),
        }
    }
    let data_path = data_path.unwrap_or_else(|| PathBuf::from("data.json"));
    let data = data::Data::load(&data_path).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    export::configure(export::Output { dir: out, raw, raw_only, stills, loop_all });
    let catalog = widgets::catalog();
    if names.is_empty() {
        // Duels need a rival collected in data.json: only drawn when named.
        names = catalog.iter().map(|(key, _)| key.to_string()).filter(|key| !key.starts_with("duel-")).collect();
    }
    for name in &names {
        if widgets::find(&catalog, name).is_none() {
            eprintln!("unknown widget: {name:?} (known: {}, or duel-<login>[-7|-30|-365])",
                catalog.iter().map(|(key, _)| *key).collect::<Vec<_>>().join(", "));
            std::process::exit(2);
        }
        if let Some((login, _)) = widgets::parse_duel(name).filter(|_| !catalog.iter().any(|(key, _)| key == name))
            && !data.friends.contains_key(&login) {
                eprintln!("{name}: {login} is not in data.json; collect it with `lgtm-md collect <you> data.json {login}`");
                std::process::exit(2);
            }
    }
    for name in &names {
        let started = std::time::Instant::now();
        widgets::find(&catalog, name).unwrap()(&data);
        println!("ok {name} ({} ms)", started.elapsed().as_millis());
    }
}
