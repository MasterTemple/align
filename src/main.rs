mod aligner;
mod config;
mod engine;
mod parser;
#[cfg(test)]
mod tests;

use aligner::Aligner;
use config::Config;
use parser::parse_args;
use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("Usage: align [global-flags] <pattern> [flags] [<pattern> [flags] ...]");
        eprintln!("Reads lines from stdin and aligns matched patterns.");
        std::process::exit(1);
    }

    let config = Config::load();

    let command = match parse_args(&args, &config) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("align: parse error: {}", e);
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let mut lines: Vec<String> = stdin
        .lock()
        .lines()
        .map(|l| l.expect("Failed to read line"))
        .collect();

    let aligner = Aligner::new(command);
    let output = aligner.process(&mut lines);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in &output {
        writeln!(out, "{}", line).expect("Failed to write");
    }
}
