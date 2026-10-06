use std::ops::Range;

use crate::error::{Error, Result};

/// Which regex engine compiles `/regex/` patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Engine {
    /// [`fancy_regex`]: Rust regex syntax plus lookaround and backreferences.
    #[default]
    FancyRegex,
    /// [`regress`]: ECMAScript (JavaScript) regex syntax.
    Regress,
}

impl Engine {
    pub fn from_name(name: &str) -> Result<Self> {
        match name.to_ascii_lowercase().replace('-', "_").as_str() {
            "fancy_regex" | "fancy" => Ok(Engine::FancyRegex),
            "regress" => Ok(Engine::Regress),
            _ => Err(Error::new(format!(
                "unknown regex engine '{name}' (expected 'fancy_regex' or 'regress')"
            ))),
        }
    }
}

/// An uncompiled pattern: what the user wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Literal(String),
    Regex { src: String, flags: String },
}

impl Source {
    /// Parse a standalone value such as a config entry (`'/\d+$/'`) or the
    /// argument of `-w`, `-c`, `-g`, `-v`. `/…/flags` is a regex, anything
    /// else is a literal.
    pub fn from_value(s: &str) -> Source {
        if let Some(rest) = s.strip_prefix('/') {
            if let Some(end) = rest.rfind('/') {
                let flags = &rest[end + 1..];
                if flags.chars().all(|c| c.is_ascii_alphabetic()) {
                    return Source::Regex {
                        src: unescape_slashes(&rest[..end]),
                        flags: flags.to_string(),
                    };
                }
            }
        }
        Source::Literal(s.to_string())
    }

    pub fn is_regex(&self) -> bool {
        matches!(self, Source::Regex { .. })
    }

    /// True when this is a regex with the `g` flag (repeat without limit).
    pub fn is_global(&self) -> bool {
        matches!(self, Source::Regex { flags, .. } if flags.contains('g'))
    }

    pub fn compile(&self, engine: Engine) -> Result<Matcher> {
        match self {
            Source::Literal(s) if s.is_empty() => Err(Error::new("empty pattern")),
            Source::Literal(s) => Ok(Matcher::Literal(s.clone())),
            Source::Regex { src, flags } => compile_regex(src, flags, engine),
        }
    }
}

/// `\/` inside `/…/` only exists to avoid ending the regex; neither engine needs it.
pub(crate) fn unescape_slashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('/') => out.push('/'),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn compile_regex(src: &str, flags: &str, engine: Engine) -> Result<Matcher> {
    if src.is_empty() {
        return Err(Error::new("empty regex"));
    }
    for f in flags.chars() {
        let ok = match f {
            'i' | 'm' | 's' | 'g' => true,
            'x' => engine == Engine::FancyRegex,
            _ => false,
        };
        if !ok {
            return Err(Error::new(format!("unsupported regex flag '{f}' in /{src}/{flags}")));
        }
    }
    match engine {
        Engine::FancyRegex => {
            let inline: String = flags.chars().filter(|c| "imsx".contains(*c)).collect();
            let full = if inline.is_empty() {
                src.to_string()
            } else {
                format!("(?{inline}){src}")
            };
            fancy_regex::Regex::new(&full)
                .map(|re| Matcher::Fancy(Box::new(re)))
                .map_err(|e| Error::new(format!("invalid regex /{src}/: {e}")))
        }
        Engine::Regress => {
            let mut rf = regress::Flags::default();
            rf.icase = flags.contains('i');
            rf.multiline = flags.contains('m');
            rf.dot_all = flags.contains('s');
            regress::Regex::with_flags(src, rf)
                .map(|re| Matcher::Regress(Box::new(re)))
                .map_err(|e| Error::new(format!("invalid regex /{src}/: {e}")))
        }
    }
}

/// A compiled pattern. All offsets are byte offsets into the searched text.
#[derive(Debug)]
pub enum Matcher {
    Literal(String),
    Fancy(Box<fancy_regex::Regex>),
    Regress(Box<regress::Regex>),
}

impl Matcher {
    /// First match starting at or after byte `pos`. Unlike slicing the text,
    /// this keeps `^`, `\b` and lookbehind aware of what precedes `pos`.
    pub fn find_from(&self, text: &str, pos: usize) -> Option<Range<usize>> {
        if pos > text.len() {
            return None;
        }
        match self {
            Matcher::Literal(lit) => text[pos..]
                .find(lit.as_str())
                .map(|i| pos + i..pos + i + lit.len()),
            Matcher::Fancy(re) => re
                .find_from_pos(text, pos)
                .ok()
                .flatten()
                .map(|m| m.start()..m.end()),
            Matcher::Regress(re) => re.find_from(text, pos).next().map(|m| m.range),
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.find_from(text, 0).is_some()
    }

    /// The last (rightmost-starting) match in `text`.
    pub fn find_last(&self, text: &str) -> Option<Range<usize>> {
        let mut last = None;
        let mut pos = 0;
        while let Some(m) = self.find_from(text, pos) {
            pos = if m.is_empty() {
                next_boundary(text, m.end)
            } else {
                m.end
            };
            last = Some(m);
            if pos > text.len() {
                break;
            }
        }
        last
    }
}

/// The byte offset of the char boundary after `pos` (or `len + 1` at the end).
pub(crate) fn next_boundary(text: &str, pos: usize) -> usize {
    text[pos..]
        .chars()
        .next()
        .map_or(text.len() + 1, |c| pos + c.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_from_keeps_left_context() {
        for engine in [Engine::FancyRegex, Engine::Regress] {
            let re = Source::from_value(r"/^\w/").compile(engine).unwrap();
            assert_eq!(re.find_from("ab cd", 0), Some(0..1));
            assert_eq!(re.find_from("ab cd", 1), None, "{engine:?}");

            let re = Source::from_value(r"/\b\w/").compile(engine).unwrap();
            assert_eq!(re.find_from("ab cd", 1), Some(3..4), "{engine:?}");
        }
    }

    #[test]
    fn literal_is_not_a_regex() {
        let m = Source::Literal("a.b".into()).compile(Engine::Regress).unwrap();
        assert_eq!(m.find_from("axb a.b", 0), Some(4..7));
    }

    #[test]
    fn from_value() {
        assert_eq!(
            Source::from_value(r"/\d+$/"),
            Source::Regex { src: r"\d+$".into(), flags: "".into() }
        );
        assert_eq!(Source::from_value("/"), Source::Literal("/".into()));
        assert_eq!(Source::from_value("//"), Source::Regex { src: "".into(), flags: "".into() });
        assert_eq!(Source::from_value(r"/a\/b/i"), Source::Regex { src: "a/b".into(), flags: "i".into() });
    }

    #[test]
    fn bad_input_errors() {
        assert!(Source::from_value("/[/").compile(Engine::FancyRegex).is_err());
        assert!(Source::from_value("/a/q").compile(Engine::FancyRegex).is_err());
        assert!(Source::from_value("/a/x").compile(Engine::Regress).is_err());
        assert!(Engine::from_name("bogus").is_err());
    }

    #[test]
    fn find_last() {
        let m = Source::from_value(r"/\d+/").compile(Engine::FancyRegex).unwrap();
        assert_eq!(m.find_last("a 12 b 345"), Some(7..10));
    }
}
