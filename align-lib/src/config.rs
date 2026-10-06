use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Padding can be either a uniform amount or left/right independently
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PadConfig {
    Uniform(usize),
    Sides { left: usize, right: usize },
}

impl PadConfig {
    pub fn left(&self) -> usize {
        match self {
            PadConfig::Uniform(n) => *n,
            PadConfig::Sides { left, .. } => *left,
        }
    }
    pub fn right(&self) -> usize {
        match self {
            PadConfig::Uniform(n) => *n,
            PadConfig::Sides { right, .. } => *right,
        }
    }
}

/// Per-literal/pattern defaults in the config file
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PatternDefaults {
    pub fill: Option<char>,
    pub pad: Option<PadConfig>,
    pub align: Option<String>, // "left" or "right"
    pub word: Option<String>,  // word-boundary pattern string
    pub context: Option<String>,
}

/// Top-level config structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Default filler character
    #[serde(default = "default_fill")]
    pub fill: char,

    /// Default padding amount
    #[serde(default = "default_pad")]
    pub pad: usize,

    /// Default word boundary for literals
    #[serde(default = "default_word_bound")]
    pub word_bound_literal: String,

    /// Default word boundary for regex patterns
    #[serde(default = "default_word_bound")]
    pub word_bound_regex: String,

    /// Regex engine: "fancy_regex" | "regress"
    #[serde(default = "default_engine")]
    pub engine: String,

    /// Per-pattern defaults
    #[serde(default)]
    pub patterns: HashMap<String, PatternDefaults>,
}

fn default_fill() -> char {
    ' '
}
fn default_pad() -> usize {
    1
}
fn default_word_bound() -> String {
    r"/[^A-z0-9_]/".to_string()
}
fn default_engine() -> String {
    "fancy_regex".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fill: default_fill(),
            pad: default_pad(),
            word_bound_literal: default_word_bound(),
            word_bound_regex: default_word_bound(),
            engine: default_engine(),
            patterns: HashMap::new(),
        }
    }
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "Align").map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Self {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Config::default(),
        };

        if !path.exists() {
            // Write default config
            let default = Config::default();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(toml_str) = toml::to_string_pretty(&default) {
                let commented = format!(
                    "# align configuration file\n\
                     # Default filler character used when inserting padding\n\
                     # fill = ' '\n\n\
                     # Default padding amount (spaces around matched pattern)\n\
                     # pad = 1\n\n\
                     # Default word boundary regex for literals\n\
                     # word_bound_literal = '/[^A-z0-9_]/'\n\n\
                     # Default word boundary regex for regex patterns\n\
                     # word_bound_regex = '/[^A-z0-9_]/'\n\n\
                     # Regex engine: 'fancy_regex' or 'regress'\n\
                     # engine = 'fancy_regex'\n\n\
                     # Per-pattern defaults (examples):\n\
                     # [patterns.\"=\"]\n\
                     # fill = ' '\n\
                     # pad = 1\n\n\
                     # [patterns.\",\"]\n\
                     # pad = {{ left = 0, right = 1 }}\n\n\
                     # [patterns.\".\"]\n\
                     # pad = 0\n\
                     # context = '/\\d+$/'\n\n\
                     {}\n",
                    toml_str
                );
                let _ = fs::write(&path, commented);
            }
            return Config::default();
        }

        match fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("align: config parse error: {}", e);
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    /// Look up per-pattern defaults for a given pattern string
    pub fn pattern_defaults(&self, key: &str) -> Option<&PatternDefaults> {
        self.patterns.get(key)
    }
}
