use unicode_width::UnicodeWidthStr;

use crate::engine::{compile_literal, CompiledRegex, RegexEngine};
use crate::parser::{parse_regex_or_lit, AlignPattern, Command};

/// A match found in a line for a particular pattern slot
#[derive(Debug, Clone)]
struct SlotMatch {
    /// Byte offset of match start in the (current) line
    start: usize,
    /// Byte offset of match end in the (current) line
    end: usize,
    /// For `-c` / `-C`: the byte offset where we insert fill characters.
    /// Inserting here pushes the match (which comes after) to the right.
    context_start: Option<usize>,
}

impl SlotMatch {
    /// The byte position where we insert / trim fill characters.
    fn insert_point(&self) -> usize {
        self.context_start.unwrap_or(self.start)
    }

    /// Visual column of the match start — always used as the alignment anchor.
    fn match_col(&self, line: &str) -> usize {
        UnicodeWidthStr::width(&line[..self.start])
    }

    /// Visual column of the insert point (only needed for context-mode delta).
    fn insert_col(&self, line: &str) -> usize {
        UnicodeWidthStr::width(&line[..self.insert_point()])
    }
}

pub struct Aligner {
    cmd: Command,
}

impl Aligner {
    pub fn new(cmd: Command) -> Self {
        Aligner { cmd }
    }

    pub fn process(&self, lines: &mut Vec<String>) -> Vec<String> {
        let engine = &self.cmd.global.engine;

        // Compile each pattern's regex and auxiliary regexes once.
        let compiled: Vec<Option<CompiledRegex>> = self
            .cmd
            .patterns
            .iter()
            .map(|p| compile_pattern(p, engine).ok())
            .collect();
        let word_bounds: Vec<Option<CompiledRegex>> = self
            .cmd
            .patterns
            .iter()
            .map(|p| compile_word_bound(p, engine))
            .collect();
        let ctx_res: Vec<Option<CompiledRegex>> = self
            .cmd
            .patterns
            .iter()
            .map(|p| compile_context(p, engine))
            .collect();

        let ignore_pat = self
            .cmd
            .global
            .ignore
            .as_ref()
            .and_then(|p| parse_regex_or_lit(&p.chars().collect::<Vec<char>>(), engine).ok());

        // let keep_pat = self
        //     .cmd
        //     .global
        //     .ignore
        //     .as_ref()
        //     .and_then(|p| parse_regex_or_lit(&p.chars().collect::<Vec<char>>(), engine).ok());

        let n = lines.len();

        // Presence check on ORIGINAL lines (for global flags only).
        let line_has_pat: Vec<Vec<bool>> = (0..self.cmd.patterns.len())
            .map(|pi| {
                let re = match &compiled[pi] {
                    Some(r) => r,
                    None => return vec![false; n],
                };
                let wb = word_bounds[pi].as_ref();
                let pat = &self.cmd.patterns[pi];
                (0..n)
                    .map(|li| {
                        !find_matches_in_line(&lines[li], re, wb, pat, ctx_res[pi].as_ref())
                            .is_empty()
                    })
                    .collect()
            })
            .collect();

        let line_is_ignored: Vec<bool> = if let Some(pat) = ignore_pat {
            (0..n)
                .map(|li| !pat.find_one(&lines[li]).is_some())
                .collect()
        } else {
            vec![false; n]
        };

        let line_has_any: Vec<bool> = (0..n)
            .map(|li| line_has_pat.iter().any(|pm| pm[li]))
            .collect();
        let line_has_all: Vec<bool> = (0..n)
            .map(|li| line_has_pat.iter().all(|pm| pm[li]))
            .collect();

        // Working lines — patterns applied sequentially.
        let mut working: Vec<String> = lines.to_vec();

        for (pi, pattern) in self.cmd.patterns.iter().enumerate() {
            let re = match &compiled[pi] {
                Some(r) => r,
                None => continue,
            };
            let wb = word_bounds[pi].as_ref();
            let ctx_re = ctx_res[pi].as_ref();

            // Which lines participate in this pattern's alignment pass?
            let active: Vec<bool> = (0..n)
                .map(|li| {
                    if line_is_ignored[li] {
                        return false;
                    }
                    if self.cmd.global.match_every && !line_has_all[li] {
                        return false;
                    }
                    line_has_pat[pi][li]
                })
                .collect();

            // Determine max occurrence count across active lines.
            let repeat_limit = pattern.repeat.unwrap_or(usize::MAX);
            let max_occ = (0..n)
                .filter(|&li| active[li])
                .map(|li| find_matches_in_line(&working[li], re, wb, pattern, ctx_re).len())
                .max()
                .unwrap_or(0);
            let occ_count = max_occ.min(repeat_limit);

            // Process each occurrence slot.
            // Re-find matches on current working lines each slot (offsets change).
            for occ in 0..occ_count {
                let per_line: Vec<Option<SlotMatch>> = (0..n)
                    .map(|li| {
                        if !active[li] {
                            return None;
                        }
                        find_matches_in_line(&working[li], re, wb, pattern, ctx_re)
                            .into_iter()
                            .nth(occ)
                    })
                    .collect();

                // Target = the column the match should land in (same for all lines).
                let target_col = compute_target_col(&working, &per_line, pattern);

                // Apply alignment to each active line.
                for li in 0..n {
                    if let Some(ref slot) = per_line[li] {
                        let new_line = align_match(&working[li], slot, target_col, pattern);
                        working[li] = new_line;
                    }
                }
            }
        }

        // Collect output, applying deletion flags.
        let mut output = Vec::new();
        for (li, line) in working.into_iter().enumerate() {
            if self.cmd.global.delete_no_match && !line_has_any[li] {
                continue;
            }
            if self.cmd.global.delete_missing_match && !line_has_all[li] {
                continue;
            }
            output.push(line);
        }
        output
    }
}

// ── target column computation ────────────────────────────────────────────────

/// Compute the target match column for this occurrence slot.
///
/// The target is the column where the match should land on every line.
/// It must be large enough that:
///   - every line's match can actually reach it (given the insert point)
///   - every line has at least pad_left fill chars before the match
fn compute_target_col(
    working: &[String],
    per_line: &[Option<SlotMatch>],
    pattern: &AlignPattern,
) -> usize {
    let pad_left = pattern.pad_left;
    let fill = pattern.fill;

    per_line
        .iter()
        .enumerate()
        .filter_map(|(li, slot_opt)| {
            let slot = slot_opt.as_ref()?;
            let line = &working[li];
            let match_col = slot.match_col(line);

            if slot.context_start.is_some() {
                // Context mode: we insert at insert_point to push the match right.
                // The match col is what we align; inserting fill before context
                // shifts match_col upward. The minimum target is just match_col
                // (we can always add more fill if another line needs a higher col).
                Some(match_col)
            } else {
                // Plain mode: ensure pad_left fill chars before match.
                let prefix = &line[..slot.start];
                let existing_fill = count_trailing_fill(prefix, fill);
                let non_fill_col = match_col.saturating_sub(existing_fill);
                Some(non_fill_col + pad_left)
            }
        })
        .max()
        .unwrap_or(0)
}

// ── match finding ────────────────────────────────────────────────────────────

fn find_matches_in_line(
    line: &str,
    re: &CompiledRegex,
    word_bound: Option<&CompiledRegex>,
    pattern: &AlignPattern,
    ctx_re: Option<&CompiledRegex>,
) -> Vec<SlotMatch> {
    let raw_matches = re.find_all(line);
    let repeat_limit = pattern.repeat.unwrap_or(usize::MAX);

    let mut result = Vec::new();
    let mut last_end = 0usize;

    for m in raw_matches.into_iter() {
        if result.len() >= repeat_limit {
            break;
        }

        if let Some(wb) = word_bound {
            if !check_word_bound(line, m.start, m.end, wb) {
                continue;
            }
        }

        let context_start = find_context_start(line, last_end, m.start, pattern, ctx_re);

        result.push(SlotMatch {
            start: m.start,
            end: m.end,
            context_start,
        });
        last_end = m.end;
    }

    result
}

fn check_word_bound(line: &str, start: usize, end: usize, wb: &CompiledRegex) -> bool {
    let before_ok = if start == 0 {
        true
    } else {
        let mut prev_start = start - 1;
        while prev_start > 0 && !line.is_char_boundary(prev_start) {
            prev_start -= 1;
        }
        wb.find_at(&line[prev_start..start], 0).is_some()
    };

    let after_ok = if end >= line.len() {
        true
    } else {
        let mut next_end = end + 1;
        while next_end <= line.len() && !line.is_char_boundary(next_end) {
            next_end += 1;
        }
        wb.find_at(&line[end..next_end], 0).is_some()
    };

    before_ok && after_ok
}

/// Determine the context insert point for -c / -C.
///
/// For `-C`: insert at `last_end` (start of the whole slice between last match and this one).
/// For `-c /pat/`: find the last (rightmost) occurrence of pat in the slice
///   `line[last_end..match_start]`, use its start as insert point.
fn find_context_start(
    line: &str,
    last_end: usize,
    match_start: usize,
    pattern: &AlignPattern,
    ctx_re: Option<&CompiledRegex>,
) -> Option<usize> {
    if pattern.context_whole {
        return Some(last_end);
    }
    if let Some(ctx) = ctx_re {
        let slice = &line[last_end..match_start];
        let matches = ctx.find_all(slice);
        if let Some(cm) = matches.last() {
            return Some(last_end + cm.start);
        }
    }
    None
}

// ── alignment ────────────────────────────────────────────────────────────────

/// Rewrite `line` so that `slot`'s match lands at `target_col`.
fn align_match(line: &str, slot: &SlotMatch, target_col: usize, pattern: &AlignPattern) -> String {
    let fill = pattern.fill;
    let pad_left = pattern.pad_left;
    let pad_right = pattern.pad_right;

    if slot.context_start.is_some() {
        align_with_context(line, slot, target_col, fill)
    } else {
        align_plain(line, slot, target_col, fill, pad_left, pad_right)
    }
}

/// Plain alignment: adjust fill before match so match lands at target_col.
///
/// We strip all existing fill from the prefix, compute the non-fill column,
/// then write exactly enough fill chars so the match lands at target_col
/// (but never fewer than pad_left).
fn align_plain(
    line: &str,
    slot: &SlotMatch,
    target_col: usize,
    fill: char,
    pad_left: usize,
    pad_right: usize,
) -> String {
    let match_start = slot.start;
    let match_end = slot.end;

    let prefix = &line[..match_start];
    let existing_fill = count_trailing_fill(prefix, fill);
    // Visual width of the fill-stripped prefix
    let non_fill_col = UnicodeWidthStr::width(prefix).saturating_sub(existing_fill);

    // Fill count = target_col - non_fill_col, but at least pad_left
    let needed_fill = if target_col >= non_fill_col {
        (target_col - non_fill_col).max(pad_left)
    } else {
        pad_left
    };

    let prefix_trimmed = trim_end_by(prefix, fill, existing_fill);
    let mut result = prefix_trimmed.to_string();
    for _ in 0..needed_fill {
        result.push(fill);
    }

    result.push_str(&line[match_start..match_end]);

    let rest = &line[match_end..];
    ensure_right_padding(&mut result, rest, fill, pad_right);

    result
}

/// Context alignment (-c/-C):
///
/// We want the match to land at `target_col`. Currently the match is at
/// `slot.match_col(line)`. The difference (delta) must be added as fill
/// characters at `insert_point`.
fn align_with_context(line: &str, slot: &SlotMatch, target_col: usize, fill: char) -> String {
    let insert_point = slot.insert_point();
    let current_match_col = slot.match_col(line);
    let delta = target_col as isize - current_match_col as isize;

    if delta == 0 {
        return line.to_string();
    }

    let prefix = &line[..insert_point];

    let mut result = if delta > 0 {
        let mut s = prefix.to_string();
        for _ in 0..delta {
            s.push(fill);
        }
        s
    } else {
        // Trim |delta| fill chars from the end of the prefix
        let trim = (-delta) as usize;
        trim_end_by(prefix, fill, trim).to_string()
    };

    result.push_str(&line[insert_point..]);
    result
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn count_trailing_fill(s: &str, fill: char) -> usize {
    s.chars().rev().take_while(|&c| c == fill).count()
}

fn trim_end_by<'a>(s: &'a str, fill: char, n: usize) -> &'a str {
    let mut count = 0;
    let mut end = s.len();
    for c in s.chars().rev() {
        if count >= n {
            break;
        }
        if c == fill {
            end -= c.len_utf8();
            count += 1;
        } else {
            break;
        }
    }
    &s[..end]
}

fn ensure_right_padding(result: &mut String, rest: &str, fill: char, pad_right: usize) {
    let existing: usize = rest.chars().take_while(|&c| c == fill).count();
    if existing < pad_right {
        for _ in existing..pad_right {
            result.push(fill);
        }
        let skip: usize = rest.chars().take(existing).map(|c| c.len_utf8()).sum();
        result.push_str(&rest[skip..]);
    } else {
        result.push_str(rest);
    }
}

// ── compilation helpers ───────────────────────────────────────────────────────

fn compile_pattern(pattern: &AlignPattern, engine: &RegexEngine) -> Result<CompiledRegex, String> {
    if pattern.is_regex {
        CompiledRegex::compile(&pattern.raw, &pattern.regex_flags, engine)
    } else {
        compile_literal(&pattern.raw, engine)
    }
}

/// Compile the word-bound regex for this pattern.
///
/// Enabled by default only for word-char-only patterns (`[A-Za-z0-9_]+`).
/// Punctuation patterns default to no word-bound (they match anywhere).
fn compile_word_bound(pattern: &AlignPattern, engine: &RegexEngine) -> Option<CompiledRegex> {
    if pattern.no_word_bound {
        return None;
    }

    if let Some(ref wb_src) = pattern.word_bound {
        let src = strip_regex_delimiters(wb_src);
        return CompiledRegex::compile(src, "", engine).ok();
    }

    let pattern_text = &pattern.raw;
    let is_word_pattern = !pattern_text.is_empty()
        && pattern_text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_');

    if !is_word_pattern {
        return None;
    }

    CompiledRegex::compile(r"[^A-Za-z0-9_]", "", engine).ok()
}

fn compile_context(pattern: &AlignPattern, engine: &RegexEngine) -> Option<CompiledRegex> {
    if pattern.context_whole {
        return None;
    }
    let ctx_src = pattern.context.as_deref()?;
    let (src, flags) = split_regex_with_flags(ctx_src);
    CompiledRegex::compile(src, flags, engine).ok()
}

fn strip_regex_delimiters(s: &str) -> &str {
    if s.starts_with('/') && s.ends_with('/') && s.len() >= 2 {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn split_regex_with_flags(s: &str) -> (&str, &str) {
    if s.starts_with('/') {
        let inner = &s[1..];
        if let Some(end) = inner.rfind('/') {
            return (&inner[..end], &inner[end + 1..]);
        }
        return (inner, "");
    }
    (s, "")
}
