//! Golden tests driven by `cases.txt`.
//!
//! Format (one case per `## name` header):
//!
//! ```text
//! ## name
//! $ <pattern string>
//! % <config toml line>      (optional, repeatable)
//! < <input line>            (`<` alone = empty line; `\t` = tab)
//! > <expected line>         (or `> ERROR: <substring of the error>`)
//! ```
//!
//! Every successful case is also re-run on its own output, which must not change.

use align_lib::{align, Config};

struct Case {
    name: String,
    line: usize,
    pattern: String,
    config: String,
    input: Vec<String>,
    expected: Vec<String>,
}

fn unescape(s: &str) -> String {
    s.replace("\\t", "\t")
}

fn content(line: &str, prefix: char) -> String {
    let rest = &line[prefix.len_utf8()..];
    unescape(rest.strip_prefix(' ').unwrap_or(rest))
}

fn parse_cases(text: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some(name) = line.strip_prefix("## ") {
            cases.push(Case {
                name: name.trim().to_string(),
                line: i + 1,
                pattern: String::new(),
                config: String::new(),
                input: Vec::new(),
                expected: Vec::new(),
            });
            continue;
        }
        let Some(case) = cases.last_mut() else { continue };
        match line.chars().next() {
            Some('$') => case.pattern = content(line, '$'),
            Some('%') => {
                case.config.push_str(&content(line, '%'));
                case.config.push('\n');
            }
            Some('<') => case.input.push(content(line, '<')),
            Some('>') => case.expected.push(content(line, '>')),
            _ => {}
        }
    }
    cases
}

#[test]
fn golden() {
    let text = include_str!("cases.txt");
    let cases = parse_cases(text);
    assert!(cases.len() > 10, "fixture didn't parse");
    let mut failures = Vec::new();

    for case in &cases {
        let config = Config::from_toml(&case.config).expect("bad config in fixture");
        let result = align(&case.pattern, &case.input, &config);
        let label = format!("{} (cases.txt:{}) `align {}`", case.name, case.line, case.pattern);

        if let Some(needle) = case.expected.first().and_then(|e| e.strip_prefix("ERROR: ")) {
            match result {
                Err(e) if e.to_string().contains(needle) => {}
                other => failures.push(format!("{label}\n  expected error containing {needle:?}\n  got {other:?}")),
            }
            continue;
        }
        match result {
            Err(e) => failures.push(format!("{label}\n  unexpected error: {e}")),
            Ok(out) if out != case.expected => failures.push(format!(
                "{label}\n  expected:\n{}\n  got:\n{}",
                show(&case.expected),
                show(&out)
            )),
            Ok(out) => {
                let again = align(&case.pattern, &out, &config).unwrap();
                if again != out {
                    failures.push(format!("{label}\n  not idempotent; second run:\n{}", show(&again)));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

fn show(lines: &[String]) -> String {
    lines.iter().map(|l| format!("    |{}|", l.replace('\t', "→"))).collect::<Vec<_>>().join("\n")
}
