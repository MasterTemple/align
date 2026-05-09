use fancy_regex::Regex;
use unicode_width::UnicodeWidthStr;

use crate::parser::{GlobalFlags, PatternFlags, PatternKind, PatternSpec, Repeat};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn process(lines: &[String], global: &GlobalFlags, specs: &[PatternSpec]) -> Vec<String> {
    if specs.is_empty() {
        return lines.to_vec();
    }

    // Compile all patterns once.
    let compiled: Vec<CompiledSpec> = specs.iter().map(|s| compile_spec(s)).collect();

    // For each pattern, find all match positions across all lines.
    // Then for each "column group" compute the target column and rewrite lines.

    // We process patterns left-to-right, each time re-parsing the (already
    // partially-rewritten) lines so that earlier alignments affect the column
    // positions seen by later patterns.

    let mut result: Vec<String> = lines.to_vec();

    // First, decide which lines are eligible (global -g/-d/-D flags).
    let any_match: Vec<bool> = result
        .iter()
        .map(|l| compiled.iter().any(|c| has_match(l, c)))
        .collect();
    let all_match: Vec<bool> = result
        .iter()
        .map(|l| compiled.iter().all(|c| has_match(l, c)))
        .collect();

    // Apply -d / -D filtering
    if global.delete_no_match || global.delete_not_all {
        result = result
            .into_iter()
            .enumerate()
            .filter(|(idx, _)| {
                if global.delete_no_match && !any_match[*idx] {
                    return false;
                }
                if global.delete_not_all && !all_match[*idx] {
                    return false;
                }
                true
            })
            .map(|(_, l)| l)
            .collect();

        // Recompute match flags after filter
        let any_match2: Vec<bool> = result
            .iter()
            .map(|l| compiled.iter().any(|c| has_match(l, c)))
            .collect();
        let all_match2: Vec<bool> = result
            .iter()
            .map(|l| compiled.iter().all(|c| has_match(l, c)))
            .collect();

        // Re-apply alignment
        for cspec in &compiled {
            align_pass(&mut result, global, cspec, &any_match2, &all_match2);
        }
    } else {
        for cspec in &compiled {
            align_pass(&mut result, global, cspec, &any_match, &all_match);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Compiled representation
// ---------------------------------------------------------------------------

struct CompiledSpec {
    regex: Regex,
    flags: PatternFlags,
    context_regex: Option<Regex>,
    /// original text for word-bound wrapping
    word_bound: bool,
}

fn compile_spec(spec: &PatternSpec) -> CompiledSpec {
    let word_bound = spec.flags.word_bound.unwrap_or(false);
    let regex = build_regex(&spec.kind, word_bound);
    let context_regex = if spec.flags.context_whole {
        Some(Regex::new("^.*$").unwrap())
    } else {
        spec.flags.context.as_ref().map(|ck| build_regex(ck, false))
    };
    CompiledSpec {
        regex,
        flags: spec.flags.clone(),
        context_regex,
        word_bound,
    }
}

fn build_regex(kind: &PatternKind, word_bound: bool) -> Regex {
    match kind {
        PatternKind::Regex { source, flags } => {
            let mut pattern = source.clone();
            // handle inline flags
            let mut prefix = "(?".to_string();
            let mut has_flags = false;
            for ch in flags.chars() {
                match ch {
                    'i' => {
                        prefix.push('i');
                        has_flags = true;
                    }
                    's' => {
                        prefix.push('s');
                        has_flags = true;
                    }
                    'x' => {
                        prefix.push('x');
                        has_flags = true;
                    }
                    'm' => {
                        prefix.push('m');
                        has_flags = true;
                    }
                    _ => {}
                }
            }
            if has_flags {
                prefix.push(')');
                pattern = format!("{prefix}{pattern}");
            }
            Regex::new(&pattern).unwrap_or_else(|e| {
                eprintln!("align: invalid regex /{source}/{flags}: {e}");
                Regex::new("(?!x)x").unwrap() // never-matching sentinel
            })
        }
        PatternKind::Literal(s) => {
            let escaped = fancy_regex::escape(s);
            let pat = if word_bound {
                format!(r"(?<!\w){escaped}(?!\w)")
            } else {
                escaped.into_owned()
            };
            Regex::new(&pat).unwrap_or_else(|e| {
                eprintln!("align: failed to compile literal pattern {s:?}: {e}");
                Regex::new("(?!x)x").unwrap()
            })
        }
    }
}

fn has_match(line: &str, cspec: &CompiledSpec) -> bool {
    cspec.regex.is_match(line).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// One alignment pass for one compiled spec
// ---------------------------------------------------------------------------

fn align_pass(
    lines: &mut Vec<String>,
    global: &GlobalFlags,
    cspec: &CompiledSpec,
    any_match: &[bool],
    all_match: &[bool],
) {
    let flags = &cspec.flags;
    let fill = flags.fill.unwrap_or(' ');
    let pad_left = flags.pad_left.unwrap_or(1);
    let pad_right = flags.pad_right.unwrap_or(1);

    let max_repeats = match &flags.repeat {
        Repeat::Count(n) => *n,
        Repeat::Infinite => usize::MAX,
    };

    // We align one "occurrence index" at a time (first match across all lines,
    // then second match, etc.)
    for occurrence in 0..max_repeats {
        // For each line, find the nth occurrence and record its byte start and
        // the byte start of the *padded* prefix (after trimming existing padding).
        let mut match_infos: Vec<Option<MatchInfo>> = lines
            .iter()
            .enumerate()
            .map(|(idx, line)| {
                // Respect -g: skip lines that don't have all matches
                if global.only_all_match && !all_match[idx] {
                    return None;
                }
                find_nth_match(line, &cspec.regex, occurrence, cspec, pad_left, pad_right)
            })
            .collect();

        // Check if any line has a match at this occurrence
        if match_infos.iter().all(|m| m.is_none()) {
            break;
        }

        // Compute the target column (maximum left-side width among matching lines).
        // "left-side width" = the visual column where the match starts (after trimming
        // any existing padding we injected).
        let max_left = match_infos
            .iter()
            .filter_map(|m| m.as_ref())
            .map(|m| m.left_width)
            .max()
            .unwrap_or(0);

        // Also figure out the match text width for right-align.
        let max_match_width = match_infos
            .iter()
            .filter_map(|m| m.as_ref())
            .map(|m| m.match_width)
            .max()
            .unwrap_or(0);

        // Rewrite each line
        for (idx, info) in match_infos.iter().enumerate() {
            let info = match info {
                Some(i) => i,
                None => continue,
            };

            let line = &lines[idx];
            let new_line = rewrite_line(
                line,
                info,
                max_left,
                max_match_width,
                fill,
                pad_left,
                pad_right,
                flags.left_align,
            );
            lines[idx] = new_line;
        }
    }
}

// ---------------------------------------------------------------------------
// Match info
// ---------------------------------------------------------------------------

struct MatchInfo {
    /// byte offset of the start of the (trimmed) prefix region
    prefix_byte_start: usize,
    /// byte offset where the match text itself starts
    match_byte_start: usize,
    /// byte offset where the match text ends
    match_byte_end: usize,
    /// visual width of the left side (prefix_byte_start..match_byte_start)
    left_width: usize,
    /// visual width of the matched text
    match_width: usize,
    /// if context is used: byte offset within [prefix_byte_start..match_byte_start]
    /// where the context alignment target starts
    context_offset: Option<usize>,
}

/// Find the `n`th (0-indexed) match of `regex` in `line`.
/// Returns None if no such match.
fn find_nth_match(
    line: &str,
    regex: &Regex,
    n: usize,
    cspec: &CompiledSpec,
    pad_left: usize,
    pad_right: usize,
) -> Option<MatchInfo> {
    let fill = cspec.flags.fill.unwrap_or(' ');

    // Collect all matches
    let mut count = 0;
    let mut search_start = 0;

    loop {
        let m = regex.find_from_pos(line, search_start).ok()??;
        if count == n {
            // We found our match. Now strip existing padding around it so that
            // re-runs are idempotent.
            let (prefix_byte_start, match_start, match_end) =
                strip_padding(line, m.start(), m.end(), fill, pad_left, pad_right);

            let prefix = &line[prefix_byte_start..match_start];
            let left_width = visual_width(prefix);
            let match_width = visual_width(&line[match_start..match_end]);

            // context: find sub-pattern in prefix
            let context_offset = if let Some(ctx_re) = &cspec.context_regex {
                let prefix_slice = &line[prefix_byte_start..match_start];
                ctx_re
                    .find(prefix_slice)
                    .ok()
                    .flatten()
                    .map(|cm| prefix_byte_start + cm.start())
            } else {
                None
            };

            return Some(MatchInfo {
                prefix_byte_start,
                match_byte_start: match_start,
                match_byte_end: match_end,
                left_width,
                match_width,
                context_offset,
            });
        }
        count += 1;
        // Advance past this match (at least 1 to avoid infinite loops on zero-width matches)
        search_start = if m.end() > m.start() {
            m.end()
        } else {
            m.start() + 1
        };
        if search_start > line.len() {
            break;
        }
    }

    None
}

/// Strip the padding that `align` itself added on a previous run so that
/// repeated runs are idempotent.
///
/// Returns (prefix_start, match_start, match_end) after stripping.
fn strip_padding(
    line: &str,
    match_start: usize,
    match_end: usize,
    fill: char,
    pad_left: usize,
    pad_right: usize,
) -> (usize, usize, usize) {
    // Strip fill chars to the left of match_start (but not more than pad_left each run,
    // because we can't tell how many were original). Actually we strip ALL consecutive fill
    // chars immediately before the match – the aligner will re-add the correct amount.
    let before = &line[..match_start];
    let left_stripped = before.trim_end_matches(fill);
    let new_match_start = left_stripped.len();

    // Strip fill chars to the right of match_end
    let after = &line[match_end..];
    let right_stripped = after.trim_start_matches(fill);
    let stripped_right = after.len() - right_stripped.len();
    let new_match_end = match_end - stripped_right; // no: end stays but after shifts

    // We return the (prefix_start=0, new_match_start, match_end without trailing fill)
    // "prefix_start" is always 0 here since the prefix is everything before the match.
    (0, new_match_start, match_end)
}

// ---------------------------------------------------------------------------
// Rewrite a line to put the match at the target column
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn rewrite_line(
    line: &str,
    info: &MatchInfo,
    target_left: usize,
    max_match_width: usize,
    fill: char,
    pad_left: usize,
    pad_right: usize,
    left_align: bool,
) -> String {
    // Split line into: head | gap | match_text | tail
    let head = &line[info.prefix_byte_start..info.match_byte_start];
    let match_text = &line[info.match_byte_start..info.match_byte_end];
    let tail = &line[info.match_byte_end..];

    let head_vis = visual_width(head);
    let match_vis = visual_width(match_text);

    // How many fill chars to insert/remove before the match so that
    // the match starts at target_left + pad_left.
    let desired_match_col = target_left + pad_left;
    let new_gap = if desired_match_col >= head_vis {
        desired_match_col - head_vis
    } else {
        0
    };

    // Align the match text itself (for right-align, pad so all matches are the same width)
    let (match_prefix, match_suffix) = if !left_align && max_match_width > match_vis {
        let diff = max_match_width - match_vis;
        (fill.to_string().repeat(diff), String::new())
    } else {
        (String::new(), String::new())
    };

    let gap_str = fill.to_string().repeat(new_gap);
    let right_pad_str = fill.to_string().repeat(pad_right);

    // head already has everything up to (but not including) the old padding
    let prefix = &line[..info.prefix_byte_start];

    format!("{prefix}{head}{gap_str}{match_prefix}{match_text}{match_suffix}{right_pad_str}{tail}")
}

// ---------------------------------------------------------------------------
// Utility: visual column width (handles multi-byte / wide chars)
// ---------------------------------------------------------------------------

fn visual_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}
