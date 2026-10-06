use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::matcher::Source;

pub const DEFAULT_WORD_BOUND: &str = "/[^A-Za-z0-9_]/";
/// Shipped by older versions; `A-z` also covers `[ \ ] ^ _` and backtick.
const LEGACY_WORD_BOUND: &str = "/[^A-z0-9_]/";

/// Padding: one number for both sides, or `{ left, right }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PadConfig {
    Uniform(usize),
    Sides { left: usize, right: usize },
}

impl PadConfig {
    pub fn left(&self) -> usize {
        match *self {
            PadConfig::Uniform(n) | PadConfig::Sides { left: n, .. } => n,
        }
    }
    pub fn right(&self) -> usize {
        match *self {
            PadConfig::Uniform(n) | PadConfig::Sides { right: n, .. } => n,
        }
    }
}

/// `repeat = 2` or `repeat = "*"`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RepeatConfig {
    Count(usize),
    Text(String),
}

/// Defaults applied whenever a given pattern is used, keyed by the pattern
/// as written (`"="`, `"/\\d+/"`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PatternDefaults {
    pub fill: Option<char>,
    pub pad: Option<PadConfig>,
    /// `"left"` or `"right"` (same as `-l` / `-r`).
    pub align: Option<String>,
    /// Word-boundary pattern; `""` disables word bounds (same as `-W`).
    pub word: Option<String>,
    /// Context pattern; `""` uses the whole slice (same as `-j` / `-C`).
    pub context: Option<String>,
    /// Number of occurrences, or `"*"` for unlimited (same as `-n`).
    pub repeat: Option<RepeatConfig>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Character used to fill the alignment gap.
    #[serde(default = "default_fill")]
    pub fill: char,
    /// Spaces kept between a match and the text around it.
    #[serde(default = "default_pad")]
    pub pad: PadConfig,
    #[serde(default = "default_word_bound")]
    pub word_bound_literal: String,
    #[serde(default = "default_word_bound")]
    pub word_bound_regex: String,
    /// `"fancy_regex"` or `"regress"`.
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Display width of a tab character.
    #[serde(default = "default_tabstop")]
    pub tabstop: usize,
    #[serde(default)]
    pub patterns: HashMap<String, PatternDefaults>,
}

fn default_fill() -> char {
    ' '
}
fn default_pad() -> PadConfig {
    PadConfig::Uniform(1)
}
fn default_word_bound() -> String {
    DEFAULT_WORD_BOUND.to_string()
}
fn default_engine() -> String {
    "fancy_regex".to_string()
}
fn default_tabstop() -> usize {
    8
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fill: default_fill(),
            pad: default_pad(),
            word_bound_literal: default_word_bound(),
            word_bound_regex: default_word_bound(),
            engine: default_engine(),
            tabstop: default_tabstop(),
            patterns: HashMap::new(),
        }
    }
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Config> {
        let mut config: Config =
            toml::from_str(text).map_err(|e| Error::new(format!("config: {e}")))?;
        for wb in [&mut config.word_bound_literal, &mut config.word_bound_regex] {
            if wb == LEGACY_WORD_BOUND {
                *wb = default_word_bound();
            }
        }
        if config.tabstop == 0 {
            return Err(Error::new("config: tabstop must be at least 1"));
        }
        Ok(config)
    }

    /// Per-pattern defaults for `source`. Regexes may be keyed as
    /// `/src/flags`, `/src/` or plain `src`.
    pub fn pattern_defaults(&self, source: &Source) -> Option<&PatternDefaults> {
        match source {
            Source::Literal(s) => self.patterns.get(s),
            Source::Regex { src, flags } => self
                .patterns
                .get(&format!("/{src}/{flags}"))
                .or_else(|| self.patterns.get(&format!("/{src}/")))
                .or_else(|| self.patterns.get(src)),
        }
    }

    /// Contents written to a fresh config file: every setting documented and
    /// commented out, so the built-in defaults can keep evolving.
    pub fn template() -> &'static str {
        r#"# align configuration
# Every setting is optional; the values shown are the defaults.

# Character that fills the alignment gap (padding is always spaces).
# fill = ' '

# Spaces between a match and its neighbours: a number or { left = 0, right = 1 }.
# pad = 1

# Word boundary used by literal / regex patterns.
# word_bound_literal = '/[^A-Za-z0-9_]/'
# word_bound_regex = '/[^A-Za-z0-9_]/'

# Regex engine: 'fancy_regex' (Rust syntax + lookaround) or 'regress' (JavaScript syntax).
# engine = 'fancy_regex'

# Display width of a tab character.
# tabstop = 8

# Per-pattern defaults, keyed by the pattern as you type it.
# Keys: fill, pad, align ('left'|'right'), word ('' disables), context ('' = whole slice), repeat (n or '*').
#
# [patterns]
# "," = { pad = { left = 0, right = 1 } }
# "." = { pad = 0, context = '/\d+$/' }
# '/\d+/' = { align = 'right' }
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses_to_default() {
        assert_eq!(Config::from_toml(Config::template()).unwrap(), Config::default());
    }

    #[test]
    fn legacy_word_bound_is_migrated() {
        let c = Config::from_toml("word_bound_literal = '/[^A-z0-9_]/'").unwrap();
        assert_eq!(c.word_bound_literal, DEFAULT_WORD_BOUND);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::from_toml("padd = 2").is_err());
        assert!(Config::from_toml("[patterns]\n\"=\" = { paad = 1 }").is_err());
    }

    #[test]
    fn regex_keys() {
        let c = Config::from_toml("[patterns]\n'/\\d+/' = { fill = '0' }").unwrap();
        let src = Source::from_value(r"/\d+/");
        assert_eq!(c.pattern_defaults(&src).unwrap().fill, Some('0'));
    }
}
