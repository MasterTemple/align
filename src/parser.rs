/// Parses the full CLI input string into global flags and a list of PatternSpecs.
///
/// Grammar (simplified):
///   input        = global_flags* (pattern per_pat_flags*)*
///   pattern      = regex | literal
///   regex        = '/' ... '/' regex_flags?
///   literal      = `'...'` | `"..."` | backtick`...`backtick | bare_word
use crate::config::Config;

/// How many times to repeat a pattern match per line.
#[derive(Debug, Clone)]
pub enum Repeat {
    Count(usize),
    Infinite,
}

/// A compiled-ish pattern: either regex source or literal string.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    Regex { source: String, flags: String },
    Literal(String),
}

/// Flags that modify how a single pattern is applied.
#[derive(Debug, Clone)]
pub struct PatternFlags {
    pub fill: Option<char>,
    pub pad_left: Option<usize>,
    pub pad_right: Option<usize>,
    pub left_align: bool,
    pub word_bound: Option<bool>, // None = inherit from config
    pub repeat: Repeat,
    /// context sub-pattern (applied to the slice before this match)
    pub context: Option<PatternKind>,
    pub context_whole: bool,
}

impl PatternFlags {
    pub fn default_with_config(cfg: &Config, is_regex: bool, pat_text: &str) -> Self {
        // Check if there's a per-character config for this pattern
        let char_cfg = cfg.char_configs.get(pat_text);

        let pad_left = char_cfg.and_then(|c| c.pad_left).unwrap_or(cfg.default_pad);
        let pad_right = char_cfg
            .and_then(|c| c.pad_right)
            .unwrap_or(cfg.default_pad);
        let fill = char_cfg.and_then(|c| c.fill).unwrap_or(cfg.default_fill);
        let left_align = char_cfg.and_then(|c| c.left_align).unwrap_or(true);
        let word_bound = char_cfg.and_then(|c| c.word_bound).or_else(|| {
            if is_regex {
                Some(cfg.default_word_bound_regex)
            } else {
                Some(cfg.default_word_bound_literal)
            }
        });
        let context = char_cfg
            .and_then(|c| c.context.as_ref())
            .map(|ctx| parse_pattern_kind(ctx).unwrap_or(PatternKind::Literal(ctx.clone())));

        PatternFlags {
            fill: Some(fill),
            pad_left: Some(pad_left),
            pad_right: Some(pad_right),
            left_align,
            word_bound,
            repeat: Repeat::Count(1),
            context,
            context_whole: false,
        }
    }
}

/// A pattern plus its alignment flags.
#[derive(Debug, Clone)]
pub struct PatternSpec {
    pub kind: PatternKind,
    pub flags: PatternFlags,
}

/// Global flags that affect the entire run.
#[derive(Debug, Default, Clone)]
pub struct GlobalFlags {
    /// Only align lines where every pattern matches.
    pub only_all_match: bool,
    /// Delete lines with no match.
    pub delete_no_match: bool,
    /// Delete lines where not every pattern matches.
    pub delete_not_all: bool,
    /// Global filler override (applied when set before any pattern).
    pub fill: Option<char>,
    pub pad_left: Option<usize>,
    pub pad_right: Option<usize>,
    pub left_align: Option<bool>,
    pub word_bound: Option<bool>,
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Token {
    Pattern(PatternKind),
    Flag(String),            // e.g. "-l", "-r", "-b", "-B", "-g", "-d", "-D", "-C"
    FlagVal(String, String), // flag + its value, e.g. ("-f", " "), ("-p", "2"), ("-n", "3")
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // skip whitespace
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        // regex: /pattern/flags
        if chars[i] == '/' {
            // but a lone `/` with nothing after might not be a regex – try to find closing /
            let start = i + 1;
            let mut j = start;
            let mut escaped = false;
            loop {
                if j >= chars.len() {
                    // No closing slash – treat `/` as a literal token
                    return Err("Unterminated regex (missing closing '/')".into());
                }
                if escaped {
                    escaped = false;
                    j += 1;
                    continue;
                }
                if chars[j] == '\\' {
                    escaped = true;
                    j += 1;
                    continue;
                }
                if chars[j] == '/' {
                    break;
                }
                j += 1;
            }
            let source: String = chars[start..j].iter().collect();
            i = j + 1;
            // consume optional regex flags (letters)
            let mut rflags = String::new();
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                rflags.push(chars[i]);
                i += 1;
            }
            tokens.push(Token::Pattern(PatternKind::Regex {
                source,
                flags: rflags,
            }));
            continue;
        }

        // quoted literal: `"..."`, `'...'`, or backtick`...`backtick
        if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
            let q = chars[i];
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != q {
                if chars[i] == '\\' {
                    i += 1;
                } // skip escaped char
                i += 1;
            }
            if i >= chars.len() {
                return Err(format!("Unterminated quoted literal (missing closing {q})"));
            }
            let lit: String = chars[start..i].iter().collect();
            i += 1; // closing quote
            tokens.push(Token::Pattern(PatternKind::Literal(unescape(&lit))));
            continue;
        }

        // flag or bare word
        if chars[i] == '-' {
            // check what follows
            let start = i;
            i += 1;

            // bare `-` or `--` or `->` etc. without a letter flag → treat as literal
            if i >= chars.len() || chars[i].is_whitespace() {
                tokens.push(Token::Pattern(PatternKind::Literal("-".into())));
                continue;
            }
            if chars[i] == '-' {
                // `--something` or just `--` → literal
                // collect the rest of the word
                let lit_start = start;
                while i < chars.len() && !chars[i].is_whitespace() {
                    i += 1;
                }
                let lit: String = chars[lit_start..i].iter().collect();
                tokens.push(Token::Pattern(PatternKind::Literal(lit)));
                continue;
            }

            // it's a flag; collect flag name
            let flag_start = i;
            // flags can be multi-char: `pl`, `pr`
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            let flag_name: String = chars[flag_start..i].iter().collect();

            // flags that take a value: f, p, pl, pr, n, c, E
            let value_flags = ["f", "p", "pl", "pr", "n", "c", "E"];
            if value_flags.contains(&flag_name.as_str()) {
                // skip whitespace then read value token
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(format!("Flag -{flag_name} requires a value"));
                }
                // read value: quoted or bare
                let val = read_value(&chars, &mut i)?;
                tokens.push(Token::FlagVal(flag_name, val));
            } else if flag_name.is_empty() {
                // `-` followed by non-letter: treat whole thing as literal
                while i < chars.len() && !chars[i].is_whitespace() {
                    i += 1;
                }
                let lit: String = chars[start..i].iter().collect();
                tokens.push(Token::Pattern(PatternKind::Literal(lit)));
            } else {
                tokens.push(Token::Flag(flag_name));
            }
            continue;
        }

        // bare word (no quotes, no `/`, no `-`)
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        tokens.push(Token::Pattern(PatternKind::Literal(word)));
    }

    Ok(tokens)
}

fn read_value(chars: &[char], i: &mut usize) -> Result<String, String> {
    if *i >= chars.len() {
        return Err("Expected value but got end of input".into());
    }
    let c = chars[*i];
    if c == '"' || c == '\'' || c == '`' {
        *i += 1;
        let start = *i;
        while *i < chars.len() && chars[*i] != c {
            if chars[*i] == '\\' {
                *i += 1;
            }
            *i += 1;
        }
        if *i >= chars.len() {
            return Err("Unterminated value string".into());
        }
        let val: String = chars[start..*i].iter().collect();
        *i += 1;
        Ok(unescape(&val))
    } else if c == '/' {
        // regex value for -c
        *i += 1;
        let start = *i;
        let mut escaped = false;
        loop {
            if *i >= chars.len() {
                return Err("Unterminated regex in -c value".into());
            }
            if escaped {
                escaped = false;
                *i += 1;
                continue;
            }
            if chars[*i] == '\\' {
                escaped = true;
                *i += 1;
                continue;
            }
            if chars[*i] == '/' {
                break;
            }
            *i += 1;
        }
        let src: String = chars[start..*i].iter().collect();
        *i += 1;
        let mut rflags = String::new();
        while *i < chars.len() && chars[*i].is_ascii_alphabetic() {
            rflags.push(chars[*i]);
            *i += 1;
        }
        // pack as a regex-style string so we can reconstruct
        Ok(format!("/{src}/{rflags}"))
    } else {
        // bare word
        let start = *i;
        while *i < chars.len() && !chars[*i].is_whitespace() {
            *i += 1;
        }
        let val: String = chars[start..*i].iter().collect();
        Ok(val)
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(c2) => out.push(c2),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse a pattern string for use in -c values.
pub fn parse_pattern_kind(s: &str) -> Option<PatternKind> {
    let s = s.trim();
    if s.starts_with('/') {
        let rest = &s[1..];
        if let Some(end) = rest.rfind('/') {
            let source = rest[..end].to_string();
            let flags = rest[end + 1..].to_string();
            return Some(PatternKind::Regex { source, flags });
        }
    }
    if s.is_empty() {
        return None;
    }
    Some(PatternKind::Literal(s.to_string()))
}

// ---------------------------------------------------------------------------
// Parser: tokens → GlobalFlags + Vec<PatternSpec>
// ---------------------------------------------------------------------------

pub fn parse_input(input: &str, cfg: &Config) -> Result<(GlobalFlags, Vec<PatternSpec>), String> {
    let tokens = tokenize(input)?;
    let mut global = GlobalFlags::default();
    let mut specs: Vec<PatternSpec> = Vec::new();
    let mut i = 0;

    // We do two passes over flags: flags before any pattern are treated as
    // global overrides. Once we see the first pattern we're in per-pattern mode.
    let first_pattern_idx = tokens.iter().position(|t| matches!(t, Token::Pattern(_)));

    // -- global flags (before first pattern) --
    let pre = match first_pattern_idx {
        Some(idx) => &tokens[..idx],
        None => &tokens[..],
    };
    apply_global_flags(pre, &mut global)?;
    i = first_pattern_idx.unwrap_or(tokens.len());

    // -- pattern + per-pattern flags --
    while i < tokens.len() {
        match &tokens[i] {
            Token::Pattern(kind) => {
                let is_regex = matches!(kind, PatternKind::Regex { .. });
                let pat_text = match kind {
                    PatternKind::Literal(s) => s.clone(),
                    PatternKind::Regex { source, .. } => source.clone(),
                };
                let mut flags = PatternFlags::default_with_config(cfg, is_regex, &pat_text);
                // Apply global overrides
                if let Some(f) = global.fill {
                    flags.fill = Some(f);
                }
                if let Some(p) = global.pad_left {
                    flags.pad_left = Some(p);
                }
                if let Some(p) = global.pad_right {
                    flags.pad_right = Some(p);
                }
                if let Some(l) = global.left_align {
                    flags.left_align = l;
                }
                if let Some(b) = global.word_bound {
                    flags.word_bound = Some(b);
                }

                i += 1;
                // consume per-pattern flags until next pattern or end
                while i < tokens.len() {
                    match &tokens[i] {
                        Token::Pattern(_) => break,
                        Token::Flag(name) => {
                            apply_per_pat_flag_bool(name, &mut flags)?;
                            i += 1;
                        }
                        Token::FlagVal(name, val) => {
                            apply_per_pat_flag_val(name, val, &mut flags)?;
                            i += 1;
                        }
                    }
                }
                specs.push(PatternSpec {
                    kind: kind.clone(),
                    flags,
                });
            }
            _ => {
                // stray flag after all patterns → ignore / warn
                i += 1;
            }
        }
    }

    Ok((global, specs))
}

fn apply_global_flags(tokens: &[Token], global: &mut GlobalFlags) -> Result<(), String> {
    for tok in tokens {
        match tok {
            Token::Flag(name) => match name.as_str() {
                "g" => global.only_all_match = true,
                "d" => global.delete_no_match = true,
                "D" => global.delete_not_all = true,
                "l" => global.left_align = Some(true),
                "r" => global.left_align = Some(false),
                "b" => global.word_bound = Some(true),
                "B" => global.word_bound = Some(false),
                _ => {}
            },
            Token::FlagVal(name, val) => match name.as_str() {
                "f" => {
                    global.fill = val.chars().next();
                }
                "p" => {
                    let n = val
                        .parse::<usize>()
                        .map_err(|_| format!("-p requires integer, got {val}"))?;
                    global.pad_left = Some(n);
                    global.pad_right = Some(n);
                }
                "pl" => {
                    let n = val
                        .parse::<usize>()
                        .map_err(|_| format!("-pl requires integer, got {val}"))?;
                    global.pad_left = Some(n);
                }
                "pr" => {
                    let n = val
                        .parse::<usize>()
                        .map_err(|_| format!("-pr requires integer, got {val}"))?;
                    global.pad_right = Some(n);
                }
                _ => {}
            },
            Token::Pattern(_) => unreachable!(),
        }
    }
    Ok(())
}

fn apply_per_pat_flag_bool(name: &str, flags: &mut PatternFlags) -> Result<(), String> {
    match name {
        "l" => flags.left_align = true,
        "r" => flags.left_align = false,
        "b" => flags.word_bound = Some(true),
        "B" => flags.word_bound = Some(false),
        "C" => flags.context_whole = true,
        _ => {} // unknown flag, ignore
    }
    Ok(())
}

fn apply_per_pat_flag_val(name: &str, val: &str, flags: &mut PatternFlags) -> Result<(), String> {
    match name {
        "f" => {
            flags.fill = val.chars().next();
        }
        "p" => {
            let n = val
                .parse::<usize>()
                .map_err(|_| format!("-p requires integer, got {val}"))?;
            flags.pad_left = Some(n);
            flags.pad_right = Some(n);
        }
        "pl" => {
            let n = val
                .parse::<usize>()
                .map_err(|_| format!("-pl requires integer, got {val}"))?;
            flags.pad_left = Some(n);
        }
        "pr" => {
            let n = val
                .parse::<usize>()
                .map_err(|_| format!("-pr requires integer, got {val}"))?;
            flags.pad_right = Some(n);
        }
        "n" => {
            if val == "*" {
                flags.repeat = Repeat::Infinite;
            } else {
                let n = val
                    .parse::<usize>()
                    .map_err(|_| format!("-n requires integer or *, got {val}"))?;
                flags.repeat = Repeat::Count(n);
            }
        }
        "c" => {
            // val may be a regex like "/\d+$/" or a literal
            if val.starts_with('/') {
                let rest = &val[1..];
                if let Some(end) = rest.rfind('/') {
                    let source = rest[..end].to_string();
                    let rflags = rest[end + 1..].to_string();
                    flags.context = Some(PatternKind::Regex {
                        source,
                        flags: rflags,
                    });
                    return Ok(());
                }
            }
            flags.context = Some(PatternKind::Literal(val.to_string()));
        }
        "E" => {} // engine selection - only one engine (fancy_regex)
        _ => {}
    }
    Ok(())
}
