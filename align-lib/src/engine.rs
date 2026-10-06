/// Represents a single regex match with its byte range
#[derive(Debug, Clone)]
pub struct RegexMatch {
    pub start: usize,
    pub end: usize,
}

/// Which regex engine to use
#[derive(Debug, Clone, PartialEq)]
pub enum RegexEngine {
    FancyRegex,
    Regress,
}

impl RegexEngine {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "regress" => RegexEngine::Regress,
            _ => RegexEngine::FancyRegex,
        }
    }
}

/// A compiled regex that abstracts over the underlying engine
pub enum CompiledRegex {
    Fancy(fancy_regex::Regex),
    Regress(regress::Regex),
}

impl std::fmt::Debug for CompiledRegex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompiledRegex::Fancy(_) => write!(f, "CompiledRegex::Fancy(...)"),
            CompiledRegex::Regress(_) => write!(f, "CompiledRegex::Regress(...)"),
        }
    }
}

impl CompiledRegex {
    /// Compile a regex pattern with the given engine
    pub fn compile(pattern: &str, flags: &str, engine: &RegexEngine) -> Result<Self, String> {
        match engine {
            RegexEngine::FancyRegex => {
                // Build pattern with flags embedded
                let full = build_fancy_pattern(pattern, flags);
                fancy_regex::Regex::new(&full)
                    .map(CompiledRegex::Fancy)
                    .map_err(|e| e.to_string())
            }
            RegexEngine::Regress => {
                let rf = build_regress_flags(flags);
                regress::Regex::with_flags(pattern, rf)
                    .map(CompiledRegex::Regress)
                    .map_err(|e| e.to_string())
            }
        }
    }

    pub fn find_one(&self, text: &str) -> Option<RegexMatch> {
        self.find_at(text, 0)
    }

    /// Find the first match in `text` starting at byte offset `start`
    pub fn find_at(&self, text: &str, start: usize) -> Option<RegexMatch> {
        match self {
            CompiledRegex::Fancy(re) => {
                let slice = &text[start..];
                re.find(slice).ok().flatten().map(|m| RegexMatch {
                    start: start + m.start(),
                    end: start + m.end(),
                })
            }
            CompiledRegex::Regress(re) => re.find_from(text, start).next().map(|m| RegexMatch {
                start: m.range.start,
                end: m.range.end,
            }),
        }
    }

    /// Find all non-overlapping matches in `text`
    pub fn find_all(&self, text: &str) -> Vec<RegexMatch> {
        let mut results = Vec::new();
        let mut pos = 0;
        while pos <= text.len() {
            match self.find_at(text, pos) {
                Some(m) => {
                    if m.end > m.start {
                        // Normal non-empty match
                        pos = m.end;
                        results.push(m);
                    } else {
                        // Zero-length match: advance one char boundary to avoid infinite loop
                        pos = next_char_boundary(text, m.start + 1);
                        // Only push if the pattern genuinely matches here (not an artifact)
                        // Zero-length matches are only useful for lookahead/behind patterns;
                        // for our alignment use-case we skip them to avoid matching everywhere.
                    }
                }
                None => break,
            }
        }
        results
    }
}

fn next_char_boundary(s: &str, mut pos: usize) -> usize {
    while pos <= s.len() && !s.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

fn build_fancy_pattern(pattern: &str, flags: &str) -> String {
    if flags.is_empty() {
        return pattern.to_string();
    }
    // fancy_regex supports (?flags) inline syntax
    let mut inline = String::from("(?");
    for ch in flags.chars() {
        match ch {
            'i' | 's' | 'm' | 'x' => inline.push(ch),
            _ => {} // 'g' is handled externally (find_all)
        }
    }
    if inline == "(?)" || inline == "(?" {
        return pattern.to_string();
    }
    inline.push(')');
    format!("{}{}", inline, pattern)
}

fn build_regress_flags(flags: &str) -> regress::Flags {
    let mut f = regress::Flags::default();
    for ch in flags.chars() {
        match ch {
            'i' => f.icase = true,
            'm' => f.multiline = true,
            's' => f.dot_all = true,
            _ => {}
        }
    }
    f
}

/// Compile a literal string as a regex (escaped)
pub fn compile_literal(literal: &str, engine: &RegexEngine) -> Result<CompiledRegex, String> {
    let escaped = fancy_regex::escape(literal);
    CompiledRegex::compile(&escaped, "", engine)
}
