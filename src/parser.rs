use crate::config::Config;
use crate::engine::RegexEngine;

/// A single alignment target (one pattern + its flags)
#[derive(Debug, Clone)]
pub struct AlignPattern {
    /// The raw pattern string (literal text or regex source)
    pub raw: String,
    /// Whether this is a regex (true) or literal (false)
    pub is_regex: bool,
    /// Regex flags (e.g. "gi")
    pub regex_flags: String,
    /// Filler character
    pub fill: char,
    /// Left padding
    pub pad_left: usize,
    /// Right padding
    pub pad_right: usize,
    /// Right-align the matched column (false = left-align)
    pub right_align: bool,
    /// Word-boundary pattern (None = use default)
    pub word_bound: Option<String>,
    /// Disable word-boundary checking
    pub no_word_bound: bool,
    /// Max number of repetitions (None = infinite)
    pub repeat: Option<usize>,
    /// Context pattern
    pub context: Option<String>,
    /// Align whole slice as context
    pub context_whole: bool,
}

impl AlignPattern {
    fn with_defaults(fill: char, pad_left: usize, pad_right: usize) -> Self {
        AlignPattern {
            raw: String::new(),
            is_regex: false,
            regex_flags: String::new(),
            fill,
            pad_left,
            pad_right,
            right_align: false,
            word_bound: None,
            no_word_bound: false,
            repeat: None,
            context: None,
            context_whole: false,
        }
    }
}

/// Global flags that apply to the whole command
#[derive(Debug, Clone, Default)]
pub struct GlobalFlags {
    /// Only align lines where ALL patterns match
    pub global_match_all: bool,
    /// Delete lines with no match
    pub delete_no_match: bool,
    /// Delete lines that don't have ALL matches
    pub delete_missing_match: bool,
    /// Which regex engine to use
    pub engine: RegexEngine,
}

impl Default for RegexEngine {
    fn default() -> Self {
        RegexEngine::FancyRegex
    }
}

/// Parsed command ready for the aligner
#[derive(Debug)]
pub struct Command {
    pub global: GlobalFlags,
    pub patterns: Vec<AlignPattern>,
}

/// Parse the full argument list into a Command.
pub fn parse_args(args: &[String], config: &Config) -> Result<Command, String> {
    let tokens = tokenize(args);
    parse_tokens(&tokens, config)
}

// ─── tokenizer ──────────────────────────────────────────────────────────────

/// A single token produced by the tokenizer
#[derive(Debug, Clone)]
enum Token {
    /// A flag like `-l`, `-r`, `-g`, etc.
    Flag(String),
    /// A flag that takes a value: `-p 2` → FlagVal("p", "2")
    FlagVal(String, String),
    /// A literal pattern (already unquoted)
    Literal(String),
    /// A regex pattern with its flags
    Regex(String, String),
}

fn tokenize(args: &[String]) -> Vec<Token> {
    // Join everything into one string so we can handle quoting across args
    let joined = args.join(" ");
    let chars: Vec<char> = joined.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        // Skip whitespace
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        if chars[i] == '-' {
            // Try to parse a flag
            let (tok, consumed) = parse_flag(&chars, i);
            tokens.push(tok);
            i += consumed;
        } else if chars[i] == '/' {
            // Regex pattern
            let (pat, flags, consumed) = parse_regex(&chars, i);
            tokens.push(Token::Regex(pat, flags));
            i += consumed;
        } else if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
            // Quoted literal
            let delim = chars[i];
            let (lit, consumed) = parse_quoted(&chars, i, delim);
            tokens.push(Token::Literal(lit));
            i += consumed;
        } else {
            // Bare word (unquoted literal)
            let (lit, consumed) = parse_bare(&chars, i);
            tokens.push(Token::Literal(lit));
            i += consumed;
        }
    }

    tokens
}

fn parse_flag(chars: &[char], start: usize) -> (Token, usize) {
    // We know chars[start] == '-'
    let mut i = start + 1;

    // Handle `--` or bare `-`
    if i >= chars.len() || chars[i].is_whitespace() {
        return (Token::Literal("-".to_string()), 1);
    }
    // Second `-`: `--something`
    if chars[i] == '-' {
        i += 1;
        if i >= chars.len() || chars[i].is_whitespace() {
            return (Token::Literal("--".to_string()), 2);
        }
    }

    // Collect flag name chars
    let flag_start = i;
    while i < chars.len() && !chars[i].is_whitespace() {
        i += 1;
    }
    let flag_name: String = chars[flag_start..i].iter().collect();

    // Flags that take a value argument
    let takes_value = matches!(
        flag_name.as_str(),
        "f" | "p" | "pl" | "pr" | "n" | "w" | "c" | "E"
    );

    if takes_value {
        // Skip whitespace
        let mut j = i;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if j < chars.len() {
            // Read value token
            let (val, val_consumed) = if chars[j] == '/' {
                let (pat, flags, c) = parse_regex(chars, j);
                (format!("/{}/{}", pat, flags), c)
            } else if chars[j] == '"' || chars[j] == '\'' || chars[j] == '`' {
                let delim = chars[j];
                let (lit, c) = parse_quoted(chars, j, delim);
                (lit, c)
            } else {
                let (lit, c) = parse_bare(chars, j);
                (lit, c)
            };
            let total = (j - start) + val_consumed;
            return (Token::FlagVal(flag_name, val), total);
        }
    }

    // If flag_name contains non-alphabetic chars (e.g. ">", "->"),
    // it's not a real flag — treat the whole token as a literal.
    let is_known_flag = matches!(
        flag_name.as_str(),
        "g" | "d" | "D" | "E" | "f" | "p" | "pl" | "pr" | "l" | "r" | "W" | "w" | "n" | "c" | "C"
    ) || flag_name.chars().all(|c| c.is_ascii_alphabetic());

    if !is_known_flag {
        // Reconstruct the literal: the '-' plus whatever followed
        let literal: String = std::iter::once('-')
            .chain(chars[flag_start..i].iter().copied())
            .collect();
        return (Token::Literal(literal), i - start);
    }

    (Token::Flag(flag_name), i - start)
}

fn parse_regex(chars: &[char], start: usize) -> (String, String, usize) {
    // chars[start] == '/'
    let mut i = start + 1;
    let mut pat = String::new();
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            pat.push(chars[i]);
            pat.push(chars[i + 1]);
            i += 2;
        } else if chars[i] == '/' {
            i += 1;
            break;
        } else {
            pat.push(chars[i]);
            i += 1;
        }
    }
    // Collect flags after closing slash
    let mut flags = String::new();
    while i < chars.len() && chars[i].is_ascii_alphabetic() {
        flags.push(chars[i]);
        i += 1;
    }
    (pat, flags, i - start)
}

fn parse_quoted(chars: &[char], start: usize, delim: char) -> (String, usize) {
    let mut i = start + 1;
    let mut lit = String::new();
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == delim {
            lit.push(delim);
            i += 2;
        } else if chars[i] == delim {
            i += 1;
            break;
        } else {
            lit.push(chars[i]);
            i += 1;
        }
    }
    (lit, i - start)
}

fn parse_bare(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    let mut lit = String::new();
    while i < chars.len() && !chars[i].is_whitespace() {
        lit.push(chars[i]);
        i += 1;
    }
    (lit, i - start)
}

// ─── token → Command ────────────────────────────────────────────────────────

fn parse_tokens(tokens: &[Token], config: &Config) -> Result<Command, String> {
    let mut global = GlobalFlags::default();
    global.engine = RegexEngine::from_str(&config.engine);

    let default_fill = config.fill;
    let default_pad = config.pad;

    // We do two passes: collect global flags first (flags before any pattern),
    // then collect patterns with their per-pattern flags.

    let mut patterns: Vec<AlignPattern> = Vec::new();

    // Global-override defaults (may be overridden by pre-pattern flags)
    let mut g_fill = default_fill;
    let mut g_pad_left = default_pad;
    let mut g_pad_right = default_pad;
    let mut g_right_align = false;

    let mut i = 0;
    let mut seen_pattern = false;

    while i < tokens.len() {
        match &tokens[i] {
            Token::Regex(pat, flags) => {
                seen_pattern = true;
                let mut ap = AlignPattern::with_defaults(g_fill, g_pad_left, g_pad_right);
                ap.right_align = g_right_align;
                ap.raw = pat.clone();
                ap.is_regex = true;
                ap.regex_flags = flags.clone();
                // Apply config pattern defaults
                apply_config_defaults_regex(&mut ap, pat, config);
                i += 1;
                // Consume trailing flags
                i = consume_pattern_flags(tokens, i, &mut ap)?;
                patterns.push(ap);
            }
            Token::Literal(lit) => {
                seen_pattern = true;
                let mut ap = AlignPattern::with_defaults(g_fill, g_pad_left, g_pad_right);
                ap.right_align = g_right_align;
                ap.raw = lit.clone();
                ap.is_regex = false;
                // Apply config pattern defaults
                apply_config_defaults_literal(&mut ap, lit, config);
                i += 1;
                i = consume_pattern_flags(tokens, i, &mut ap)?;
                patterns.push(ap);
            }
            Token::Flag(f) => {
                match f.as_str() {
                    "g" => global.global_match_all = true,
                    "d" => global.delete_no_match = true,
                    "D" => global.delete_missing_match = true,
                    "l" if !seen_pattern => g_right_align = false,
                    "r" if !seen_pattern => g_right_align = true,
                    "W" if !seen_pattern => { /* handled per-pattern */ }
                    _ => {
                        if seen_pattern {
                            // Belongs to the last pattern - back up and re-process
                            // Actually, per-pattern flags come after the pattern token.
                            // If we reach here mid-stream it's an error or orphan flag.
                        }
                        // else: unknown global flag – ignore silently
                    }
                }
                i += 1;
            }
            Token::FlagVal(f, v) => {
                match f.as_str() {
                    "E" => global.engine = RegexEngine::from_str(v),
                    "f" if !seen_pattern => {
                        g_fill = v.chars().next().unwrap_or(' ');
                    }
                    "p" if !seen_pattern => {
                        let n = v
                            .parse::<usize>()
                            .map_err(|_| format!("invalid pad: {}", v))?;
                        g_pad_left = n;
                        g_pad_right = n;
                    }
                    "pl" if !seen_pattern => {
                        g_pad_left = v
                            .parse::<usize>()
                            .map_err(|_| format!("invalid pad: {}", v))?;
                    }
                    "pr" if !seen_pattern => {
                        g_pad_right = v
                            .parse::<usize>()
                            .map_err(|_| format!("invalid pad: {}", v))?;
                    }
                    _ => {}
                }
                i += 1;
            }
        }
    }

    if patterns.is_empty() {
        return Err("no patterns given".to_string());
    }

    Ok(Command { global, patterns })
}

/// Consume per-pattern flags after a pattern token, return new index
fn consume_pattern_flags(
    tokens: &[Token],
    mut i: usize,
    ap: &mut AlignPattern,
) -> Result<usize, String> {
    while i < tokens.len() {
        match &tokens[i] {
            Token::Flag(f) => match f.as_str() {
                "l" => {
                    ap.right_align = false;
                    i += 1;
                }
                "r" => {
                    ap.right_align = true;
                    i += 1;
                }
                "W" => {
                    ap.no_word_bound = true;
                    i += 1;
                }
                "C" => {
                    ap.context_whole = true;
                    i += 1;
                }
                _ => break,
            },
            Token::FlagVal(f, v) => match f.as_str() {
                "f" => {
                    ap.fill = v.chars().next().unwrap_or(' ');
                    i += 1;
                }
                "p" => {
                    let n = v
                        .parse::<usize>()
                        .map_err(|_| format!("invalid pad: {}", v))?;
                    ap.pad_left = n;
                    ap.pad_right = n;
                    i += 1;
                }
                "pl" => {
                    ap.pad_left = v
                        .parse::<usize>()
                        .map_err(|_| format!("invalid pad: {}", v))?;
                    i += 1;
                }
                "pr" => {
                    ap.pad_right = v
                        .parse::<usize>()
                        .map_err(|_| format!("invalid pad: {}", v))?;
                    i += 1;
                }
                "n" => {
                    if v == "*" {
                        ap.repeat = None; // infinite
                    } else {
                        ap.repeat = Some(
                            v.parse::<usize>()
                                .map_err(|_| format!("invalid repeat: {}", v))?,
                        );
                    }
                    i += 1;
                }
                "w" => {
                    ap.word_bound = Some(v.clone());
                    i += 1;
                }
                "c" => {
                    ap.context = Some(v.clone());
                    i += 1;
                }
                _ => break,
            },
            // Stop at next pattern or unknown flag
            _ => break,
        }
    }
    Ok(i)
}

fn apply_config_defaults_literal(ap: &mut AlignPattern, key: &str, config: &Config) {
    if let Some(defaults) = config.pattern_defaults(key) {
        apply_pattern_defaults(ap, defaults);
    }
}

fn apply_config_defaults_regex(ap: &mut AlignPattern, key: &str, config: &Config) {
    if let Some(defaults) = config.pattern_defaults(key) {
        apply_pattern_defaults(ap, defaults);
    }
}

fn apply_pattern_defaults(ap: &mut AlignPattern, d: &crate::config::PatternDefaults) {
    if let Some(f) = d.fill {
        ap.fill = f;
    }
    if let Some(ref p) = d.pad {
        ap.pad_left = p.left();
        ap.pad_right = p.right();
    }
    if let Some(ref a) = d.align {
        ap.right_align = a == "right";
    }
    if let Some(ref w) = d.word {
        ap.word_bound = Some(w.clone());
    }
    if let Some(ref c) = d.context {
        ap.context = Some(c.clone());
    }
}
