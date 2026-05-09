mod aligner;
mod config;
mod parser;

use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("Usage: align [global-flags] <pattern> [flags] [<pattern> [flags] ...]");
        eprintln!("       Lines are read from stdin.");
        eprintln!();
        eprintln!("Global flags:");
        eprintln!("  -g          Only align lines where every pattern matches");
        eprintln!("  -d          Delete lines with no match");
        eprintln!("  -D          Delete lines where not every pattern matches");
        eprintln!("  -E <engine> Specify regex engine (currently: fancy_regex)");
        eprintln!();
        eprintln!("Per-pattern flags:");
        eprintln!("  -f <char>   Filler character (default: space)");
        eprintln!("  -p <n>      Padding around match");
        eprintln!("  -pl <n>     Left padding");
        eprintln!("  -pr <n>     Right padding");
        eprintln!("  -l          Left-align matches (default)");
        eprintln!("  -r          Right-align matches");
        eprintln!("  -b          Require word boundaries (default)");
        eprintln!("  -B          Do not require word boundaries");
        eprintln!("  -n <n|*>    Repeat pattern n times (* = unlimited)");
        eprintln!("  -c <pat>    Align context (slice before match) by sub-pattern");
        eprintln!("  -C          Left-align entire slice as context");
        std::process::exit(1);
    }

    let cfg = config::load_config();
    let input_str = args.join(" ");

    let (global_flags, patterns) = match parser::parse_input(&input_str, &cfg) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("align: parse error: {e}");
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let mut lines: Vec<String> = stdin
        .lock()
        .lines()
        .map(|l| l.expect("Failed to read line"))
        .collect();

    lines = aligner::process(&lines, &global_flags, &patterns);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in &lines {
        writeln!(out, "{line}").ok();
    }
}
