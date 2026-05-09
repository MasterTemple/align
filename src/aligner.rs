use std::collections::HashMap;
use unicode_width::UnicodeWidthStr;

use crate::engine::{compile_literal, CompiledRegex, RegexEngine, RegexMatch};
use crate::parser::{AlignPattern, Command};

/// A match found in a line for a particular pattern slot
#[derive(Debug, Clone)]
struct SlotMatch {
    /// Byte offset of match start in the line
    start: usize,
    /// Byte offset of match end in the line
    end: usize,
    /// Optional context match (byte range) preceding this match
    context_start: Option<usize>,
}

pub struct Aligner {
    cmd: Command,
}

impl Aligner {
    pub fn new(cmd: Command) -> Self {
        Aligner { cmd }
    }

    pub fn process(&self, lines: &mut Vec<String>) -> Vec<String> {
        // Compile patterns once
        let engine = &self.cmd.global.engine;
        let compiled: Vec<Option<CompiledRegex>> = self
            .cmd
            .patterns
            .iter()
            .map(|p| compile_pattern(p, engine).ok())
            .collect();

        // Compile word-bound patterns
        let word_bounds: Vec<Option<CompiledRegex>> = self
            .cmd
            .patterns
            .iter()
            .map(|p| compile_word_bound(p, engine))
            .collect();

        // For each pattern, find matches in each line
        // Structure: matches_by_pattern[pat_idx][line_idx] = Vec<SlotMatch>
        let mut matches_by_pattern: Vec<Vec<Vec<SlotMatch>>> = Vec::new();

        for (pi, pattern) in self.cmd.patterns.iter().enumerate() {
            let re = match &compiled[pi] {
                Some(r) => r,
                None => {
                    matches_by_pattern.push(vec![vec![]; lines.len()]);
                    continue;
                }
            };
            let wb = word_bounds[pi].as_ref();
            let ctx_re = compile_context(pattern, engine);

            let mut pat_matches: Vec<Vec<SlotMatch>> = Vec::new();
            for line in lines.iter() {
                let ms = find_matches_in_line(line, re, wb, pattern, ctx_re.as_ref(), engine);
                pat_matches.push(ms);
            }
            matches_by_pattern.push(pat_matches);
        }

        // Determine which lines match
        let line_has_any: Vec<bool> = (0..lines.len())
            .map(|li| matches_by_pattern.iter().any(|pm| !pm[li].is_empty()))
            .collect();

        let line_has_all: Vec<bool> = (0..lines.len())
            .map(|li| matches_by_pattern.iter().all(|pm| !pm[li].is_empty()))
            .collect();

        // Apply global filter: -g means only align lines where ALL patterns match
        // -d / -D deletes
        // Build output
        let mut output: Vec<String> = Vec::new();

        // For each pattern, compute per-column alignment offsets
        // We process patterns in order; after each pass, lines are modified.
        // We need to track the accumulated mutations per line.
        // Strategy: work with a mutable Vec of (line_chars, offset_map) and apply per pattern.

        // Simpler approach: build a "working" set of lines, apply each pattern in sequence.
        let mut working: Vec<Option<String>> = lines.iter().cloned().map(Some).collect();

        for (pi, pattern) in self.cmd.patterns.iter().enumerate() {
            // Collect matches for this pattern across lines
            let pat_line_matches = &matches_by_pattern[pi];

            // Determine which lines are active for this pattern
            // Lines are active if: they have a match, and global constraints are satisfied
            let active: Vec<bool> = (0..lines.len())
                .map(|li| {
                    if working[li].is_none() {
                        return false;
                    }
                    if self.cmd.global.global_match_all && !line_has_all[li] {
                        return false;
                    }
                    !pat_line_matches[li].is_empty()
                })
                .collect();

            // Collect the nth match for alignment (we align by occurrence index)
            // Max occurrences across active lines
            let max_occ = pat_line_matches
                .iter()
                .enumerate()
                .filter(|(li, _)| active[*li])
                .map(|(_, ms)| ms.len())
                .max()
                .unwrap_or(0);

            let repeat_limit = pattern.repeat.unwrap_or(usize::MAX);
            let occ_count = max_occ.min(repeat_limit);

            for occ in 0..occ_count {
                // For this occurrence, compute the column positions
                // Column = position of the match start (with context adjustment)
                let col_positions: Vec<Option<usize>> = (0..lines.len())
                    .map(|li| {
                        if !active[*&li] {
                            return None;
                        }
                        let ms = &pat_line_matches[li];
                        ms.get(occ).map(|m| {
                            let line = working[li].as_ref().unwrap();
                            let ctx_start = m.context_start.unwrap_or(m.start);
                            // Visual width of prefix up to context start or match start
                            let prefix = &line[..ctx_start];
                            UnicodeWidthStr::width(prefix)
                        })
                    })
                    .collect();

                // Maximum column among active lines (with left padding)
                let max_col = col_positions.iter().filter_map(|c| *c).max().unwrap_or(0);

                // Apply alignment to each active line for this occurrence
                for li in 0..lines.len() {
                    if let Some(target_col) = col_positions[li] {
                        let line = working[li].take().unwrap();
                        let ms = &pat_line_matches[li];
                        if let Some(slot) = ms.get(occ) {
                            let new_line = align_match(&line, slot, target_col, max_col, pattern);
                            working[li] = Some(new_line);
                        } else {
                            working[li] = Some(line);
                        }
                    }
                }
            }
        }

        // Apply deletion flags and collect output
        for (li, line_opt) in working.into_iter().enumerate() {
            let line = match line_opt {
                Some(l) => l,
                None => continue,
            };

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

/// Find all (optionally word-bounded) matches of `re` in `line`
fn find_matches_in_line(
    line: &str,
    re: &CompiledRegex,
    word_bound: Option<&CompiledRegex>,
    pattern: &AlignPattern,
    ctx_re: Option<&CompiledRegex>,
    engine: &RegexEngine,
) -> Vec<SlotMatch> {
    let raw_matches = re.find_all(line);
    let repeat_limit = pattern.repeat.unwrap_or(usize::MAX);

    let mut result = Vec::new();
    let mut last_end = 0usize;

    for m in raw_matches.into_iter().take(repeat_limit) {
        // Word boundary check
        if !pattern.no_word_bound {
            if let Some(wb) = word_bound {
                if !check_word_bound(line, m.start, m.end, wb) {
                    continue;
                }
            }
        }

        // Context
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

/// Check that the match at [start, end) in `line` is delimited by word boundaries
fn check_word_bound(line: &str, start: usize, end: usize, wb: &CompiledRegex) -> bool {
    // Before the match: either start of string or boundary char precedes it
    let before_ok = if start == 0 {
        true
    } else {
        let prev_char_end = start;
        // find last char boundary before start
        let mut prev_start = prev_char_end - 1;
        while prev_start > 0 && !line.is_char_boundary(prev_start) {
            prev_start -= 1;
        }
        let prev_char = &line[prev_start..prev_char_end];
        wb.find_at(prev_char, 0).is_some()
    };

    // After the match: either end of string or boundary char follows
    let after_ok = if end >= line.len() {
        true
    } else {
        let mut next_end = end + 1;
        while next_end <= line.len() && !line.is_char_boundary(next_end) {
            next_end += 1;
        }
        let next_char = &line[end..next_end];
        wb.find_at(next_char, 0).is_some()
    };

    before_ok && after_ok
}

/// Determine the context start (for -c and -C flags)
fn find_context_start(
    line: &str,
    last_end: usize,
    match_start: usize,
    pattern: &AlignPattern,
    ctx_re: Option<&CompiledRegex>,
) -> Option<usize> {
    if pattern.context_whole {
        // -C: whole slice between last match end and current match start
        return Some(last_end);
    }

    if let Some(ctx) = ctx_re {
        // -c pattern: find the pattern in the slice [last_end..match_start]
        let slice = &line[last_end..match_start];
        if let Some(cm) = ctx.find_at(slice, 0) {
            return Some(last_end + cm.start);
        }
    }

    None
}

/// Apply padding/alignment to make `slot` in `line` land at `target_col`
fn align_match(
    line: &str,
    slot: &SlotMatch,
    current_col: usize,
    target_col: usize,
    pattern: &AlignPattern,
) -> String {
    // The anchor position: context start if set, else match start
    let anchor = slot.context_start.unwrap_or(slot.start);

    // We need to insert (target_col - current_col) filler chars before `anchor`
    let delta = target_col as isize - current_col as isize;

    let fill = pattern.fill;
    let pad_left = pattern.pad_left;
    let pad_right = pattern.pad_right;

    let mut result = String::new();

    if delta >= 0 {
        // Insert `delta` filler chars at anchor
        result.push_str(&line[..anchor]);
        for _ in 0..delta {
            result.push(fill);
        }
        // Ensure left padding before the match (if anchor == match start)
        // and right padding after match end
        if anchor == slot.start {
            // Ensure pad_left spaces before match
            let existing_left = count_trailing_fill(&result, fill);
            if existing_left < pad_left {
                for _ in existing_left..pad_left {
                    result.push(fill);
                }
            }
            result.push_str(&line[slot.start..slot.end]);
            // Right padding
            ensure_right_padding(&mut result, &line[slot.end..], fill, pad_right);
        } else {
            // Context case: just append remainder
            result.push_str(&line[anchor..]);
        }
    } else {
        // Need to trim chars at anchor to move anchor left
        let trim_count = (-delta) as usize;
        // Trim from left of anchor position (remove spaces before anchor)
        let before = &line[..anchor];
        let trimmed_before = trim_end_by(before, fill, trim_count);
        result.push_str(trimmed_before);

        if anchor == slot.start {
            let existing_left = count_trailing_fill(&result, fill);
            if existing_left < pad_left {
                for _ in existing_left..pad_left {
                    result.push(fill);
                }
            }
            result.push_str(&line[slot.start..slot.end]);
            ensure_right_padding(&mut result, &line[slot.end..], fill, pad_right);
        } else {
            result.push_str(&line[anchor..]);
        }
    }

    result
}

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
    let existing_right = rest.chars().take_while(|&c| c == fill).count();
    if existing_right < pad_right {
        for _ in existing_right..pad_right {
            result.push(fill);
        }
        // Skip the existing fill chars in rest
        let skip_bytes: usize = rest
            .chars()
            .take(existing_right)
            .map(|c| c.len_utf8())
            .sum();
        result.push_str(&rest[skip_bytes..]);
    } else {
        result.push_str(rest);
    }
}

fn compile_pattern(pattern: &AlignPattern, engine: &RegexEngine) -> Result<CompiledRegex, String> {
    if pattern.is_regex {
        CompiledRegex::compile(&pattern.raw, &pattern.regex_flags, engine)
    } else {
        compile_literal(&pattern.raw, engine)
    }
}

fn compile_word_bound(pattern: &AlignPattern, engine: &RegexEngine) -> Option<CompiledRegex> {
    if pattern.no_word_bound {
        return None;
    }
    let wb_src = pattern.word_bound.as_deref().unwrap_or(r"[^A-Za-z0-9_]");
    // Strip outer / / if present
    let wb_src = if wb_src.starts_with('/') && wb_src.ends_with('/') {
        &wb_src[1..wb_src.len() - 1]
    } else {
        wb_src
    };
    CompiledRegex::compile(wb_src, "", engine).ok()
}

fn compile_context(pattern: &AlignPattern, engine: &RegexEngine) -> Option<CompiledRegex> {
    if pattern.context_whole {
        return None; // handled inline
    }
    let ctx_src = pattern.context.as_deref()?;
    // Strip outer / / delimiters if present
    let (src, flags) = if ctx_src.starts_with('/') {
        let inner = &ctx_src[1..];
        if let Some(end) = inner.rfind('/') {
            (&inner[..end], &inner[end + 1..])
        } else {
            (inner, "")
        }
    } else {
        (ctx_src, "")
    };
    CompiledRegex::compile(src, flags, engine).ok()
}
