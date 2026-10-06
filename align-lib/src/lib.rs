pub mod aligner;
pub mod config;
pub mod engine;
pub mod parser;
#[cfg(test)]
mod tests;

// ── Convenience top-level API ────────────────────────────────────────────────

pub use aligner::Aligner;
pub use config::Config;
pub use parser::{parse_args, AlignPattern, Command, GlobalFlags};

/// Process `lines` using the argument string `args` and the given config.
///
/// This is the primary entry-point for embedding the aligner in other tools
/// (e.g. editor plugins). `args` is parsed exactly as the CLI would parse
/// `argv[1..]`: literals, regex patterns `/…/`, flags, etc.
///
/// Returns the aligned lines, or an error string if parsing fails.
pub fn align(args: &[String], lines: &[String], config: &Config) -> Result<Vec<String>, String> {
    let cmd = parse_args(args, config)?;
    let aligner = Aligner::new(cmd);
    let mut owned: Vec<String> = lines.to_vec();
    Ok(aligner.process(&mut owned))
}

/// Same as [`align`] but accepts `&str` slices for ergonomics.
pub fn align_str(args: &[&str], lines: &[&str], config: &Config) -> Result<Vec<String>, String> {
    let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let lines_owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    align(&args_owned, &lines_owned, config)
}
