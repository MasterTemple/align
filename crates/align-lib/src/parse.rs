//! Pattern-string parsing: `[global flags] <pattern> [flags] [<pattern> [flags] …]`.

use crate::config::{Config, PatternDefaults, RepeatConfig};
use crate::error::{Error, Result};
use crate::matcher::{unescape_slashes, Engine, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// Left edges of the matches line up (`-l`, default).
    Left,
    /// Right edges of the matches line up (`-r`).
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Count(usize),
    Unlimited,
}

impl Repeat {
    pub fn limit(self) -> usize {
        match self {
            Repeat::Count(n) => n,
            Repeat::Unlimited => usize::MAX,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordBound {
    Off,
    On(Source),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Context {
    /// Fill goes right before the match.
    Off,
    /// Fill goes at the start of the slice before the match (`-j` / `-C`).
    Whole,
    /// Fill goes before the last match of this pattern in the slice (`-c`).
    Pattern(Source),
}

/// One pattern with every option resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternSpec {
    pub source: Source,
    pub fill: char,
    pub pad_left: usize,
    pub pad_right: usize,
    pub align: Align,
    pub word: WordBound,
    pub repeat: Repeat,
    pub context: Context,
}

/// A fully parsed (not yet compiled) command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub patterns: Vec<PatternSpec>,
    /// `-g`: only align lines matching this.
    pub keep: Option<Source>,
    /// `-v`: only align lines not matching this.
    pub ignore: Option<Source>,
    /// `-e`: only align lines where every pattern matches.
    pub every: bool,
    /// `-d`: delete lines with no match.
    pub delete_unmatched: bool,
    /// `-D`: delete lines missing any pattern.
    pub delete_incomplete: bool,
    pub engine: Engine,
}

// ─── tokens ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Flag(String),
    Word(String),
    Quoted(String),
    Regex { src: String, flags: String },
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    col: usize,
}

fn is_quote(c: char) -> bool {
    matches!(c, '"' | '\'' | '`')
}

fn lex(input: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = input.chars().collect();
    let at_end = |i: usize| i >= chars.len() || chars[i].is_whitespace();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let col = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let tok = if c == '-' {
            let mut j = i + 1;
            while !at_end(j) {
                j += 1;
            }
            let rest: String = chars[i + 1..j].iter().collect();
            i = j;
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphabetic()) {
                Tok::Flag(rest)
            } else {
                Tok::Word(format!("-{rest}"))
            }
        } else if c == '/' && !at_end(i + 1) && chars[i + 1] != '/' {
            let mut j = i + 1;
            let mut src = String::new();
            loop {
                match chars.get(j) {
                    None => {
                        return Err(Error::at(
                            col,
                            "unterminated regex (quote a literal '/', e.g. '/')",
                        ))
                    }
                    Some('\\') if j + 1 < chars.len() => {
                        src.push('\\');
                        src.push(chars[j + 1]);
                        j += 2;
                    }
                    Some('/') => {
                        j += 1;
                        break;
                    }
                    Some(&ch) => {
                        src.push(ch);
                        j += 1;
                    }
                }
            }
            let mut flags = String::new();
            while j < chars.len() && chars[j].is_ascii_alphabetic() {
                flags.push(chars[j]);
                j += 1;
            }
            if !at_end(j) {
                return Err(Error::at(j, "unexpected character after regex"));
            }
            i = j;
            Tok::Regex {
                src: unescape_slashes(&src),
                flags,
            }
        } else if is_quote(c) {
            let mut j = i + 1;
            let mut lit = String::new();
            loop {
                match chars.get(j) {
                    None => {
                        return Err(Error::at(
                            col,
                            format!("unterminated {c}quote{c} (wrap a lone quote in a different quote)"),
                        ))
                    }
                    Some('\\') if chars.get(j + 1) == Some(&c) => {
                        lit.push(c);
                        j += 2;
                    }
                    Some(&ch) if ch == c => {
                        j += 1;
                        break;
                    }
                    Some(&ch) => {
                        lit.push(ch);
                        j += 1;
                    }
                }
            }
            i = j;
            Tok::Quoted(lit)
        } else {
            let mut j = i;
            while !at_end(j) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            i = j;
            Tok::Word(word)
        };
        tokens.push(Token { tok, col });
    }
    Ok(tokens)
}

// ─── parser ──────────────────────────────────────────────────────────────────

/// Options that may be set at some layer; `None` means "inherit".
#[derive(Debug, Clone, Default)]
struct Overrides {
    fill: Option<char>,
    pad_left: Option<usize>,
    pad_right: Option<usize>,
    align: Option<Align>,
    word: Option<WordBound>,
    repeat: Option<Repeat>,
    context: Option<Context>,
}

impl Overrides {
    fn apply_to(&self, p: &mut PatternSpec) {
        if let Some(v) = self.fill {
            p.fill = v;
        }
        if let Some(v) = self.pad_left {
            p.pad_left = v;
        }
        if let Some(v) = self.pad_right {
            p.pad_right = v;
        }
        if let Some(v) = self.align {
            p.align = v;
        }
        if let Some(v) = &self.word {
            p.word = v.clone();
        }
        if let Some(v) = self.repeat {
            p.repeat = v;
        }
        if let Some(v) = &self.context {
            p.context = v.clone();
        }
    }

    fn from_config(d: &PatternDefaults) -> Result<Overrides> {
        Ok(Overrides {
            fill: d.fill,
            pad_left: d.pad.map(|p| p.left()),
            pad_right: d.pad.map(|p| p.right()),
            align: match d.align.as_deref() {
                None => None,
                Some("left") => Some(Align::Left),
                Some("right") => Some(Align::Right),
                Some(other) => {
                    return Err(Error::new(format!(
                        "align = '{other}' (expected 'left' or 'right')"
                    )))
                }
            },
            word: d.word.as_deref().map(word_bound_from_value),
            repeat: match &d.repeat {
                None => None,
                Some(RepeatConfig::Count(n)) => Some(parse_repeat(&n.to_string())?),
                Some(RepeatConfig::Text(s)) => Some(parse_repeat(s)?),
            },
            context: d.context.as_deref().map(|c| {
                if c.is_empty() {
                    Context::Whole
                } else {
                    Context::Pattern(Source::from_value(c))
                }
            }),
        })
    }
}

fn word_bound_from_value(v: &str) -> WordBound {
    if v.is_empty() {
        WordBound::Off
    } else {
        WordBound::On(Source::from_value(v))
    }
}

fn parse_repeat(v: &str) -> Result<Repeat> {
    if v == "*" {
        return Ok(Repeat::Unlimited);
    }
    match v.parse::<usize>() {
        Ok(n) if n >= 1 => Ok(Repeat::Count(n)),
        _ => Err(Error::new(format!("invalid repeat count '{v}' (expected a number ≥ 1 or *)"))),
    }
}

fn parse_count(flag: &str, v: &str) -> Result<usize> {
    v.parse::<usize>()
        .map_err(|_| Error::new(format!("-{flag} expects a number, got '{v}'")))
}

const GLOBAL_SWITCHES: &[&str] = &["e", "d", "D"];
const GLOBAL_VALUED: &[&str] = &["g", "v", "E"];
const PATTERN_SWITCHES: &[&str] = &["l", "r", "j", "C", "W"];
const PATTERN_VALUED: &[&str] = &["f", "p", "pl", "pr", "n", "w", "c"];

pub fn parse(input: &str, config: &Config) -> Result<Spec> {
    let tokens = lex(input)?;
    let mut spec = Spec {
        patterns: Vec::new(),
        keep: None,
        ignore: None,
        every: false,
        delete_unmatched: false,
        delete_incomplete: false,
        engine: Engine::from_name(&config.engine).map_err(|e| Error::new(format!("config: {}", e.message)))?,
    };
    let mut global = Overrides::default();
    // (source, column, explicit per-pattern flags)
    let mut patterns: Vec<(Source, usize, Overrides)> = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        let Token { tok, col } = &tokens[i];
        let col = *col;
        i += 1;
        let name = match tok {
            Tok::Word(s) | Tok::Quoted(s) => {
                patterns.push((Source::Literal(s.clone()), col, Overrides::default()));
                continue;
            }
            Tok::Regex { src, flags } => {
                let source = Source::Regex { src: src.clone(), flags: flags.clone() };
                patterns.push((source, col, Overrides::default()));
                continue;
            }
            Tok::Flag(name) => name.as_str(),
        };

        let takes_value = GLOBAL_VALUED.contains(&name) || PATTERN_VALUED.contains(&name);
        if !takes_value && !GLOBAL_SWITCHES.contains(&name) && !PATTERN_SWITCHES.contains(&name) {
            return Err(Error::at(
                col,
                format!("unknown flag -{name} (quote it to align on the literal text, e.g. '-{name}')"),
            ));
        }
        let value = if takes_value {
            let Some(next) = tokens.get(i) else {
                return Err(Error::at(col, format!("-{name} expects a value")));
            };
            i += 1;
            match &next.tok {
                Tok::Flag(_) => return Err(Error::at(next.col, format!("-{name} expects a value"))),
                other => Some((other.clone(), next.col)),
            }
        } else {
            None
        };
        let text = |flag: &str| -> Result<String> {
            match &value {
                Some((Tok::Word(s) | Tok::Quoted(s), _)) => Ok(s.clone()),
                Some((_, c)) => Err(Error::at(*c, format!("-{flag} expects plain text, not a regex"))),
                None => unreachable!(),
            }
        };
        let source = || -> Source {
            match &value {
                Some((Tok::Word(s) | Tok::Quoted(s), _)) => Source::Literal(s.clone()),
                Some((Tok::Regex { src, flags }, _)) => Source::Regex { src: src.clone(), flags: flags.clone() },
                _ => unreachable!(),
            }
        };
        let value_col = value.as_ref().map_or(col, |(_, c)| *c);

        let target = match patterns.last_mut() {
            Some((_, _, o)) => o,
            None => &mut global,
        };
        let res: Result<()> = (|| {
            match name {
                "e" => spec.every = true,
                "d" => spec.delete_unmatched = true,
                "D" => spec.delete_incomplete = true,
                "g" => spec.keep = Some(source()),
                "v" => spec.ignore = Some(source()),
                "E" => spec.engine = Engine::from_name(&text("E")?)?,
                "l" => target.align = Some(Align::Left),
                "r" => target.align = Some(Align::Right),
                "j" | "C" => target.context = Some(Context::Whole),
                "W" => target.word = Some(WordBound::Off),
                "w" => target.word = Some(WordBound::On(source())),
                "c" => target.context = Some(Context::Pattern(source())),
                "n" => target.repeat = Some(parse_repeat(&text("n")?)?),
                "f" => {
                    let t = text("f")?;
                    let mut cs = t.chars();
                    match (cs.next(), cs.next()) {
                        (Some(c), None) => target.fill = Some(c),
                        _ => return Err(Error::new(format!("-f expects a single character, got '{t}'"))),
                    }
                }
                "p" => {
                    let n = parse_count("p", &text("p")?)?;
                    target.pad_left = Some(n);
                    target.pad_right = Some(n);
                }
                "pl" => target.pad_left = Some(parse_count("pl", &text("pl")?)?),
                "pr" => target.pad_right = Some(parse_count("pr", &text("pr")?)?),
                _ => unreachable!(),
            }
            Ok(())
        })();
        res.map_err(|e| e.with_col(value_col))?;
    }

    if patterns.is_empty() {
        return Err(Error::new("no patterns given"));
    }

    for (source, col, explicit) in patterns {
        let word_default = if source.is_regex() {
            &config.word_bound_regex
        } else {
            &config.word_bound_literal
        };
        let mut p = PatternSpec {
            fill: config.fill,
            pad_left: config.pad.left(),
            pad_right: config.pad.right(),
            align: Align::Left,
            word: word_bound_from_value(word_default),
            repeat: Repeat::Count(1),
            context: Context::Off,
            source,
        };
        if let Some(d) = config.pattern_defaults(&p.source) {
            Overrides::from_config(d)
                .map_err(|e| Error::at(col, format!("config for pattern: {}", e.message)))?
                .apply_to(&mut p);
        }
        global.apply_to(&mut p);
        if p.source.is_global() {
            p.repeat = Repeat::Unlimited;
        }
        explicit.apply_to(&mut p);
        spec.patterns.push(p);
    }
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Spec> {
        parse(s, &Config::default())
    }

    fn lit(s: &str) -> Source {
        Source::Literal(s.into())
    }

    #[test]
    fn literals_and_regexes() {
        let s = p(r#"= '= ' "'" `"` -> -- - / // /=>/i"#).unwrap();
        let srcs: Vec<_> = s.patterns.iter().map(|p| p.source.clone()).collect();
        assert_eq!(
            srcs,
            vec![
                lit("="),
                lit("= "),
                lit("'"),
                lit("\""),
                lit("->"),
                lit("--"),
                lit("-"),
                lit("/"),
                lit("//"),
                Source::Regex { src: "=>".into(), flags: "i".into() },
            ]
        );
    }

    #[test]
    fn defaults() {
        let s = p("=").unwrap();
        let pat = &s.patterns[0];
        assert_eq!(pat.repeat, Repeat::Count(1));
        assert_eq!((pat.pad_left, pat.pad_right, pat.fill), (1, 1, ' '));
        assert_eq!(pat.align, Align::Left);
    }

    #[test]
    fn per_pattern_flags_bind_to_previous_pattern() {
        let s = p("= -n 2 -r : -p 0").unwrap();
        assert_eq!(s.patterns[0].repeat, Repeat::Count(2));
        assert_eq!(s.patterns[0].align, Align::Right);
        assert_eq!(s.patterns[1].repeat, Repeat::Count(1));
        assert_eq!(s.patterns[1].pad_left, 0);
        assert_eq!(s.patterns[0].pad_left, 1);
    }

    #[test]
    fn global_flags_before_first_pattern() {
        let s = p("-n * -W -j -f . = : -n 1").unwrap();
        for pat in &s.patterns {
            assert_eq!(pat.word, WordBound::Off);
            assert_eq!(pat.context, Context::Whole);
            assert_eq!(pat.fill, '.');
        }
        assert_eq!(s.patterns[0].repeat, Repeat::Unlimited);
        assert_eq!(s.patterns[1].repeat, Repeat::Count(1));
    }

    #[test]
    fn g_regex_flag_means_unlimited() {
        assert_eq!(p("/,/g").unwrap().patterns[0].repeat, Repeat::Unlimited);
        assert_eq!(p("/,/g -n 2").unwrap().patterns[0].repeat, Repeat::Count(2));
    }

    #[test]
    fn global_only_flags() {
        let s = p("-e -d -D -g foo -v /bar/ -E regress =").unwrap();
        assert!(s.every && s.delete_unmatched && s.delete_incomplete);
        assert_eq!(s.keep, Some(lit("foo")));
        assert_eq!(s.engine, Engine::Regress);
    }

    #[test]
    fn errors() {
        for (input, needle) in [
            ("", "no patterns"),
            ("= -x", "unknown flag -x"),
            ("= -foo", "unknown flag -foo"),
            ("= -p", "expects a value"),
            ("= -p -r", "expects a value"),
            ("= -p two", "expects a number"),
            ("= -f ab", "single character"),
            ("= -n 0", "repeat count"),
            ("-E bogus =", "unknown regex engine"),
            ("/abc", "unterminated regex"),
            ("'abc", "unterminated"),
            ("/a/b/", "after regex"),
            ("= -p /2/", "not a regex"),
        ] {
            let err = p(input).unwrap_err();
            assert!(err.message.contains(needle), "{input:?}: {err}");
        }
        assert_eq!(p("= -x").unwrap_err().col, Some(2));
    }

    #[test]
    fn config_layering() {
        let config = Config::from_toml(
            "pad = 2\n[patterns]\n\".\" = { pad = 0, context = '/\\d+$/', repeat = '*' }\n",
        )
        .unwrap();
        let s = parse(". =", &config).unwrap();
        assert_eq!(s.patterns[0].pad_left, 0);
        assert_eq!(s.patterns[0].repeat, Repeat::Unlimited);
        assert!(matches!(s.patterns[0].context, Context::Pattern(_)));
        assert_eq!(s.patterns[1].pad_left, 2);
        // Explicit command-line flags beat config defaults, even global ones.
        let s = parse("-p 3 .", &config).unwrap();
        assert_eq!(s.patterns[0].pad_left, 3);
    }
}
