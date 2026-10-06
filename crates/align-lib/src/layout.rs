//! Matching and layout.
//!
//! Each line is matched left to right: pattern *k+1* is searched where
//! pattern *k* stopped, and a pattern that isn't found is skipped. Every
//! (pattern, occurrence) pair is a column; columns are laid out left to right,
//! each one rebuilding the gap before its match from padding plus however much
//! fill is needed to reach the widest line.

use std::collections::BTreeSet;
use std::ops::Range;

use unicode_width::UnicodeWidthChar;

use crate::matcher::{next_boundary, Matcher};
use crate::parse::{Align, Context, PatternSpec};

/// A pattern with its regexes compiled.
#[derive(Debug)]
pub(crate) struct Compiled {
    pub spec: PatternSpec,
    pub matcher: Matcher,
    pub word: Option<Matcher>,
    pub context: CompiledContext,
}

#[derive(Debug)]
pub(crate) enum CompiledContext {
    Off,
    Whole,
    Pattern(Matcher),
}

impl Compiled {
    pub fn new(spec: PatternSpec, matcher: Matcher, word: Option<Matcher>, context: CompiledContext) -> Self {
        debug_assert!(matches!(
            (&spec.context, &context),
            (Context::Off, CompiledContext::Off)
                | (Context::Whole, CompiledContext::Whole)
                | (Context::Pattern(_), CompiledContext::Pattern(_))
        ));
        Compiled { spec, matcher, word, context }
    }
}

/// (pattern index, occurrence index). Ordering = column order.
pub(crate) type ColumnId = (usize, usize);

pub(crate) struct LineMatch {
    pub col: ColumnId,
    pub range: Range<usize>,
}

// ─── matching ────────────────────────────────────────────────────────────────

/// Find the matches of every pattern in `line`, in order.
pub(crate) fn match_line(line: &str, patterns: &[Compiled]) -> Vec<LineMatch> {
    let mut out = Vec::new();
    let mut cursor = 0;
    for (pi, pat) in patterns.iter().enumerate() {
        let mut search = cursor;
        let mut occ = 0;
        while occ < pat.spec.repeat.limit() {
            let Some(range) = find_bounded(line, search, pat) else { break };
            // An empty match can't be repeated in place.
            search = if range.is_empty() { next_boundary(line, range.end) } else { range.end };
            cursor = range.end;
            out.push(LineMatch { col: (pi, occ), range });
            occ += 1;
            if search > line.len() {
                break;
            }
        }
    }
    out
}

/// Next match at or after `pos` that passes the word-bound check.
fn find_bounded(line: &str, mut pos: usize, pat: &Compiled) -> Option<Range<usize>> {
    loop {
        let m = pat.matcher.find_from(line, pos)?;
        match &pat.word {
            Some(wb) if !word_bound_ok(line, &m, wb) => {
                pos = next_boundary(line, m.start);
                if pos > line.len() {
                    return None;
                }
            }
            _ => return Some(m),
        }
    }
}

/// A side of the match needs a boundary only if the match's own edge
/// character is a "word" character (one the boundary pattern rejects).
/// So `foo` must stand alone, while `=` matches anywhere.
fn word_bound_ok(line: &str, m: &Range<usize>, wb: &Matcher) -> bool {
    let is_bound = |c: char| wb.is_match(c.encode_utf8(&mut [0; 4]));
    let text = &line[m.clone()];
    let (Some(first), Some(last)) = (text.chars().next(), text.chars().next_back()) else {
        return true;
    };
    let left_ok = is_bound(first) || line[..m.start].chars().next_back().map_or(true, is_bound);
    let right_ok = is_bound(last) || line[m.end..].chars().next().map_or(true, is_bound);
    left_ok && right_ok
}

// ─── layout ──────────────────────────────────────────────────────────────────

/// Display width of `s` when it starts at column `col`.
fn advance(mut col: usize, s: &str, tabstop: usize) -> usize {
    for c in s.chars() {
        col += if c == '\t' { tabstop - col % tabstop } else { c.width().unwrap_or(0) };
    }
    col
}

/// Punctuation fills (`.`, `-`, `·`) are leaders: padded on both sides.
/// Whitespace fills are plain gaps, and alphanumeric fills (`0` for
/// zero-padding) sit directly against the match.
fn is_leader(fill: char) -> bool {
    !fill.is_whitespace() && !fill.is_alphanumeric()
}

/// `seg` without trailing whitespace or a trailing leader run (a run of
/// `fill` that follows whitespace), so re-aligning is idempotent.
fn trim_leader(seg: &str, fill: char) -> &str {
    let s = seg.trim_end();
    if is_leader(fill) {
        let t = s.trim_end_matches(fill);
        if t.len() < s.len() && t.ends_with(char::is_whitespace) {
            return t.trim_end();
        }
    }
    s
}

/// Per-line state while columns are being laid out.
struct Builder<'a> {
    line: &'a str,
    matches: Vec<LineMatch>,
    /// Index into `matches` of the next match to emit.
    next: usize,
    /// Output so far (ends right after the last emitted match).
    out: String,
    /// Display width of `out`.
    width: usize,
    /// Byte offset in `line` after the last emitted match.
    pos: usize,
    /// Right padding of the last emitted match, if any.
    prev_pad_right: Option<usize>,
}

/// How one line will lay out the current column, before the fill is known.
struct Plan<'a> {
    /// Display width of the line's output before this segment.
    head_width: usize,
    lead: String,
    body: &'a str,
    gap: usize,
    /// Spaces between a leader fill run and the match.
    inner: usize,
    /// Byte offset in `body` where fill goes; `None` = right before the match.
    insert_at: Option<usize>,
    /// Display width up to the match with zero fill.
    min_width: usize,
    match_width: usize,
}

impl Plan<'_> {
    /// How many fill chars inserted at `at` widen the line by exactly
    /// `extra` columns. Tabs after `at` can make that impossible.
    fn context_fill(&self, at: usize, extra: usize, fill: char, tabstop: usize) -> Option<usize> {
        let base = advance(advance(self.head_width, &self.lead, tabstop), &self.body[..at], tabstop);
        let width_with = |k: usize| {
            let w = (0..k).fold(base, |w, _| advance(w, fill.encode_utf8(&mut [0; 4]), tabstop));
            advance(w, &self.body[at..], tabstop) + self.gap
        };
        let goal = width_with(0) + extra;
        let guess = extra / fill.width().unwrap_or(1).max(1);
        (guess.saturating_sub(tabstop)..=guess + tabstop).find(|&k| width_with(k) == goal)
    }
}

impl<'a> Builder<'a> {
    fn plan(&self, pat: &Compiled, range: &Range<usize>, tabstop: usize) -> Plan<'a> {
        let seg = &self.line[self.pos..range.start];
        let body_end = trim_leader(seg, pat.spec.fill).len();
        let (lead, body_start) = match self.prev_pad_right {
            // After a match: the leading whitespace is that match's right padding.
            Some(_) => (String::new(), seg.len() - seg.trim_start().len()),
            // At line start: leading whitespace is indentation, kept verbatim.
            None => {
                let start = seg.len() - seg.trim_start().len();
                (seg[..start].to_string(), start)
            }
        };
        let body = &seg[body_start.min(body_end)..body_end];
        let (lead, gap) = match self.prev_pad_right {
            Some(pr) if body.is_empty() => (String::new(), pr.max(pat.spec.pad_left)),
            Some(pr) => (" ".repeat(pr), pat.spec.pad_left),
            None if body.is_empty() => (lead, 0),
            None => (lead, pat.spec.pad_left),
        };
        let insert_at = match &pat.context {
            CompiledContext::Off => None,
            CompiledContext::Whole => Some(0),
            CompiledContext::Pattern(m) => m.find_last(body).map(|r| r.start),
        };
        let inner = if insert_at.is_none() && is_leader(pat.spec.fill) { gap } else { 0 };
        let before = advance(advance(self.width, &lead, tabstop), body, tabstop);
        Plan {
            head_width: self.width,
            lead,
            body,
            gap,
            inner,
            insert_at,
            min_width: before + gap + inner,
            match_width: advance(0, &self.line[range.clone()], tabstop),
        }
    }

    /// Emit the segment and match, widening the gap by `extra` columns.
    fn emit(&mut self, plan: Plan, extra: usize, pat: &Compiled, range: Range<usize>, tabstop: usize) {
        let fill = pat.spec.fill;
        let fill_width = fill.width().unwrap_or(0);
        let mut s = plan.lead.clone();
        match plan.insert_at.and_then(|at| plan.context_fill(at, extra, fill, tabstop).map(|k| (at, k))) {
            Some((at, count)) => {
                s.push_str(&plan.body[..at]);
                s.extend(std::iter::repeat(fill).take(count));
                s.push_str(&plan.body[at..]);
                s.push_str(&" ".repeat(plan.gap));
            }
            // No context, or tabs make the context column unreachable:
            // fill goes right before the match, which is always exact.
            None => {
                let (count, spaces) = match fill_width {
                    0 => (0, extra),
                    w => (extra / w, extra % w),
                };
                s.push_str(plan.body);
                s.push_str(&" ".repeat(plan.gap + spaces));
                s.extend(std::iter::repeat(fill).take(count));
                s.push_str(&" ".repeat(plan.inner));
            }
        }
        s.push_str(&self.line[range.clone()]);
        self.width = advance(self.width, &s, tabstop);
        self.out.push_str(&s);
        self.pos = range.end;
        self.prev_pad_right = Some(pat.spec.pad_right);
        self.next += 1;
    }

    fn finish(mut self) -> String {
        let rest = &self.line[self.pos..];
        let trimmed = rest.trim_start();
        if let Some(pr) = self.prev_pad_right {
            // No padding (or stray whitespace) at the end of the line.
            if !trimmed.is_empty() {
                self.out.push_str(&" ".repeat(pr));
                self.out.push_str(trimmed);
            }
        } else {
            self.out.push_str(rest);
        }
        self.out
    }
}

/// Lay out `lines`. `matches[i]` is `None` for lines that must stay untouched.
pub(crate) fn layout(
    lines: &[&str],
    matches: Vec<Option<Vec<LineMatch>>>,
    patterns: &[Compiled],
    tabstop: usize,
) -> Vec<String> {
    let mut builders: Vec<Option<Builder>> = lines
        .iter()
        .zip(matches)
        .map(|(line, m)| {
            m.filter(|m| !m.is_empty()).map(|matches| Builder {
                line,
                matches,
                next: 0,
                out: String::new(),
                width: 0,
                pos: 0,
                prev_pad_right: None,
            })
        })
        .collect();

    let columns: BTreeSet<ColumnId> = builders
        .iter()
        .flatten()
        .flat_map(|b| b.matches.iter().map(|m| m.col))
        .collect();

    for col in columns {
        let pat = &patterns[col.0];
        let mut plans = Vec::new();
        for (li, b) in builders.iter().enumerate() {
            let Some(b) = b else { continue };
            if let Some(m) = b.matches.get(b.next).filter(|m| m.col == col) {
                plans.push((li, b.plan(pat, &m.range, tabstop), m.range.clone()));
            }
        }
        let edge = |p: &Plan| match pat.spec.align {
            Align::Left => p.min_width,
            Align::Right => p.min_width + p.match_width,
        };
        let target = plans.iter().map(|(_, p, _)| edge(p)).max().unwrap_or(0);
        for (li, plan, range) in plans {
            let extra = target - edge(&plan);
            builders[li].as_mut().unwrap().emit(plan, extra, pat, range, tabstop);
        }
    }

    builders
        .into_iter()
        .zip(lines)
        .map(|(b, line)| b.map_or_else(|| line.to_string(), Builder::finish))
        .collect()
}
