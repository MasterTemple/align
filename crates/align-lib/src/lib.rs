//! Align text into columns by literal or regex patterns.
//!
//! ```
//! use align_lib::{align, Config};
//!
//! let out = align("=", &["foo = 1", "foobar = 2", "x = 3"], &Config::default()).unwrap();
//! assert_eq!(out, ["foo    = 1", "foobar = 2", "x      = 3"]);
//! ```
//!
//! The pattern string uses the same syntax as the `align` CLI:
//! `[global flags] <pattern> [flags] [<pattern> [flags] …]`.

mod config;
mod error;
mod layout;
mod matcher;
mod parse;

pub use config::{Config, PadConfig, PatternDefaults, RepeatConfig, DEFAULT_WORD_BOUND};
pub use error::{Error, Result};
pub use matcher::{Engine, Source};
pub use parse::{parse, Align, Context, PatternSpec, Repeat, Spec, WordBound};

use layout::{Compiled, CompiledContext};
use matcher::Matcher;

/// Parse `pattern` and align `lines` with it.
pub fn align<S: AsRef<str>>(pattern: &str, lines: &[S], config: &Config) -> Result<Vec<String>> {
    Ok(Command::parse(pattern, config)?.apply(lines))
}

/// A parsed and compiled pattern string, reusable across inputs.
#[derive(Debug)]
pub struct Command {
    spec: Spec,
    patterns: Vec<Compiled>,
    keep: Option<Matcher>,
    ignore: Option<Matcher>,
    tabstop: usize,
}

impl Command {
    pub fn parse(pattern: &str, config: &Config) -> Result<Command> {
        let spec = parse::parse(pattern, config)?;
        Command::compile(spec, config.tabstop)
    }

    /// Compile an already-parsed [`Spec`].
    pub fn compile(spec: Spec, tabstop: usize) -> Result<Command> {
        let engine = spec.engine;
        let compile = |s: &Source| s.compile(engine);
        let patterns = spec
            .patterns
            .iter()
            .map(|p| {
                let word = match &p.word {
                    WordBound::Off => None,
                    WordBound::On(s) => Some(compile(s).map_err(|e| prefix(e, "-w"))?),
                };
                let context = match &p.context {
                    Context::Off => CompiledContext::Off,
                    Context::Whole => CompiledContext::Whole,
                    Context::Pattern(s) => CompiledContext::Pattern(compile(s).map_err(|e| prefix(e, "-c"))?),
                };
                Ok(Compiled::new(p.clone(), compile(&p.source)?, word, context))
            })
            .collect::<Result<Vec<_>>>()?;
        let keep = spec.keep.as_ref().map(compile).transpose().map_err(|e| prefix(e, "-g"))?;
        let ignore = spec.ignore.as_ref().map(compile).transpose().map_err(|e| prefix(e, "-v"))?;
        Ok(Command {
            spec,
            patterns,
            keep,
            ignore,
            tabstop: tabstop.max(1),
        })
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    pub fn apply<S: AsRef<str>>(&self, lines: &[S]) -> Vec<String> {
        let lines: Vec<&str> = lines.iter().map(AsRef::as_ref).collect();
        let n_patterns = self.patterns.len();

        let all_matches: Vec<_> = lines.iter().map(|l| layout::match_line(l, &self.patterns)).collect();
        let pattern_count = |m: &[layout::LineMatch]| {
            let mut seen = vec![false; n_patterns];
            m.iter().for_each(|m| seen[m.col.0] = true);
            seen.iter().filter(|s| **s).count()
        };
        let keep_line: Vec<bool> = all_matches
            .iter()
            .map(|m| {
                let count = pattern_count(m);
                !(self.spec.delete_unmatched && count == 0 || self.spec.delete_incomplete && count < n_patterns)
            })
            .collect();
        let active: Vec<Option<Vec<layout::LineMatch>>> = lines
            .iter()
            .zip(all_matches)
            .map(|(line, m)| {
                let filtered = self.keep.as_ref().is_some_and(|k| !k.is_match(line))
                    || self.ignore.as_ref().is_some_and(|v| v.is_match(line))
                    || self.spec.every && pattern_count(&m) < n_patterns;
                (!filtered).then_some(m)
            })
            .collect();

        layout::layout(&lines, active, &self.patterns, self.tabstop)
            .into_iter()
            .zip(keep_line)
            .filter_map(|(line, keep)| keep.then_some(line))
            .collect()
    }
}

fn prefix(e: Error, flag: &str) -> Error {
    Error { message: format!("{flag}: {}", e.message), col: e.col }
}
