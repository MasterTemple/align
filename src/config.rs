use directories::ProjectDirs;
use serde::Deserialize;
use std::collections::HashMap;

/// Raw TOML shape for a per-character/delimiter override.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct CharConfigRaw {
    pub fill: Option<String>,
    pub pad: Option<toml::Value>,
    pub left_align: Option<bool>,
    pub word_bound: Option<bool>,
    pub context: Option<String>,
}

/// Parsed per-character config.
#[derive(Debug, Clone)]
pub struct CharConfig {
    pub fill: Option<char>,
    pub pad_left: Option<usize>,
    pub pad_right: Option<usize>,
    pub left_align: Option<bool>,
    pub word_bound: Option<bool>,
    pub context: Option<String>,
}

impl CharConfig {
    fn from_raw(raw: &CharConfigRaw) -> Self {
        let fill = raw.fill.as_ref().and_then(|s| s.chars().next());
        let (pad_left, pad_right) = parse_pad_value(raw.pad.as_ref());
        CharConfig {
            fill,
            pad_left,
            pad_right,
            left_align: raw.left_align,
            word_bound: raw.word_bound,
            context: raw.context.clone(),
        }
    }
}

fn parse_pad_value(v: Option<&toml::Value>) -> (Option<usize>, Option<usize>) {
    match v {
        None => (None, None),
        Some(toml::Value::Integer(n)) => {
            let n = *n as usize;
            (Some(n), Some(n))
        }
        Some(toml::Value::Table(t)) => {
            let left = t
                .get("left")
                .and_then(|v| v.as_integer())
                .map(|n| n as usize);
            let right = t
                .get("right")
                .and_then(|v| v.as_integer())
                .map(|n| n as usize);
            (left, right)
        }
        _ => (None, None),
    }
}

#[derive(Debug, Default)]
pub struct Config {
    pub default_fill: char,
    pub default_pad: usize,
    pub default_word_bound_literal: bool,
    pub default_word_bound_regex: bool,
    pub char_configs: HashMap<String, CharConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    default_fill: Option<String>,
    default_pad: Option<usize>,
    word_bound_literal: Option<bool>,
    word_bound_regex: Option<bool>,
    #[serde(flatten)]
    chars: HashMap<String, toml::Value>,
}

pub fn load_config() -> Config {
    let mut cfg = Config {
        default_fill: ' ',
        default_pad: 1,
        default_word_bound_literal: true,
        default_word_bound_regex: false,
        char_configs: HashMap::new(),
    };

    let path = ProjectDirs::from("", "", "Align").map(|d| d.config_dir().join("config.toml"));

    let path = match path {
        Some(p) => p,
        None => return cfg,
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return cfg,
    };

    let raw: RawConfig = match toml::from_str(&content) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("align: config parse error: {e}");
            return cfg;
        }
    };

    if let Some(fill) = raw.default_fill.as_ref().and_then(|s| s.chars().next()) {
        cfg.default_fill = fill;
    }
    if let Some(p) = raw.default_pad {
        cfg.default_pad = p;
    }
    if let Some(b) = raw.word_bound_literal {
        cfg.default_word_bound_literal = b;
    }
    if let Some(b) = raw.word_bound_regex {
        cfg.default_word_bound_regex = b;
    }

    // parse per-character tables
    for (key, val) in &raw.chars {
        if let toml::Value::Table(_) = val {
            if let Ok(raw_char) = val.clone().try_into::<CharConfigRaw>() {
                cfg.char_configs
                    .insert(key.clone(), CharConfig::from_raw(&raw_char));
            }
        }
    }

    cfg
}
