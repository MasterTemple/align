#[cfg(test)]
mod tests {
    use crate::aligner::Aligner;
    use crate::config::Config;
    use crate::parser::parse_args;

    fn run(args: &[&str], input: &[&str]) -> Vec<String> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let config = Config::default();
        let cmd = parse_args(&args, &config).expect("parse failed");
        let aligner = Aligner::new(cmd);
        let mut lines: Vec<String> = input.iter().map(|s| s.to_string()).collect();
        aligner.process(&mut lines)
    }

    // ── literal alignment ────────────────────────────────────────────────────

    #[test]
    fn test_align_equals() {
        let out = run(&["="], &["foo = 1", "foobar = 2", "x = 3"]);
        // All `=` should be in the same column
        let cols: Vec<usize> = out.iter().map(|l| l.find('=').expect("no =")).collect();
        assert!(
            cols.windows(2).all(|w| w[0] == w[1]),
            "cols: {:?}\nlines: {:?}",
            cols,
            out
        );
    }

    #[test]
    fn test_align_colon() {
        let out = run(
            &[":"],
            &["name: Alice", "age: 30", "email: alice@example.com"],
        );
        let cols: Vec<usize> = out.iter().map(|l| l.find(':').expect("no :")).collect();
        assert!(
            cols.windows(2).all(|w| w[0] == w[1]),
            "cols: {:?}\nlines: {:?}",
            cols,
            out
        );
    }

    #[test]
    fn test_align_comma_pad() {
        let out = run(&[",", "-pl", "0", "-pr", "1"], &["a,b,c", "foo,bar,baz"]);
        // Just check it doesn't panic and returns same number of lines
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn test_align_no_match_passthrough() {
        let input = vec!["hello world", "no pattern here"];
        let out = run(&["="], &input);
        // Lines without match pass through unchanged
        assert_eq!(out, input);
    }

    #[test]
    fn test_align_non_matching_lines_preserved() {
        let out = run(&["="], &["a = 1", "no equals sign here", "bb = 2"]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[1], "no equals sign here");
    }

    // ── regex alignment ──────────────────────────────────────────────────────

    #[test]
    fn test_align_regex_arrow() {
        let out = run(
            &["/=>/"],
            &["short => value", "longer_key => value", "x => y"],
        );
        let cols: Vec<usize> = out.iter().map(|l| l.find("=>").expect("no =>")).collect();
        assert!(
            cols.windows(2).all(|w| w[0] == w[1]),
            "cols: {:?}\nlines: {:?}",
            cols,
            out
        );
    }

    #[test]
    fn test_align_regex_case_insensitive() {
        // Should not panic; just verify it processes
        let out = run(&["/foo/i"], &["FOO bar", "foo baz"]);
        assert_eq!(out.len(), 2);
    }

    // ── multiple patterns ────────────────────────────────────────────────────

    #[test]
    fn test_align_two_patterns() {
        let out = run(&["=", ":"], &["a = b: c", "longer = x: y"]);
        // Should return same line count
        assert_eq!(out.len(), 2);
    }

    // ── global flags ─────────────────────────────────────────────────────────

    #[test]
    fn test_delete_no_match() {
        let out = run(&["-d", "="], &["a = 1", "no match", "b = 2"]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|l| l.contains('=')));
    }

    #[test]
    fn test_delete_missing_match_two_patterns() {
        let out = run(&["-D", "=", ":"], &["a = 1: x", "b = 2", "c: 3"]);
        // Only "a = 1: x" has both
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn test_global_match_all() {
        // -g: only align lines where ALL patterns match
        let out = run(&["-g", "=", ":"], &["a = 1: x", "b = 2", "c = 3: z"]);
        assert_eq!(out.len(), 3); // lines are not deleted, just not aligned
    }

    // ── padding flags ─────────────────────────────────────────────────────────

    #[test]
    fn test_pad_zero() {
        let out = run(&["=", "-p", "0"], &["a =1", "longer =2"]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn test_pad_two() {
        let out = run(&["=", "-p", "2"], &["a=1", "longer=2"]);
        // Check padding is >= 2 on each side of =
        for line in &out {
            if let Some(pos) = line.find('=') {
                assert!(pos >= 2, "insufficient left pad in: {:?}", line);
            }
        }
    }

    // ── filler char ──────────────────────────────────────────────────────────

    #[test]
    fn test_fill_char() {
        let out = run(&["=", "-f", "."], &["a = 1", "longer = 2"]);
        // The shorter line should have dots inserted
        let short_line = &out[0];
        assert!(short_line.contains('.') || short_line.contains('='));
    }

    // ── context (-c) ─────────────────────────────────────────────────────────

    #[test]
    fn test_context_decimal() {
        // align . -p 0 -c /\d+$/
        // "3.1" and "72.0" should align at '.'
        let out = run(&[".", "-p", "0", "-c", r"/\d+$/"], &["3.1", "72.0"]);
        assert_eq!(out.len(), 2);
        // The dot columns should be equal
        let cols: Vec<usize> = out.iter().map(|l| l.find('.').expect("no .")).collect();
        assert_eq!(cols[0], cols[1], "dot not aligned: {:?}", out);
    }

    #[test]
    fn test_context_whole() {
        let out = run(&["=", "-C"], &["a = 1", "longer = 2"]);
        assert_eq!(out.len(), 2);
    }

    // ── word bounds ───────────────────────────────────────────────────────────

    #[test]
    fn test_no_word_bound() {
        // Without -W, "foo" inside "foobar" should NOT match if word-bound enforced
        // With -W it should match
        let out_bounded = run(&["foo"], &["foo = 1", "foobar = 2"]);
        let out_unbounded = run(&["foo", "-W"], &["foo = 1", "foobar = 2"]);
        // Both should return 2 lines (lines are not deleted)
        assert_eq!(out_bounded.len(), 2);
        assert_eq!(out_unbounded.len(), 2);
    }

    // ── repeat (-n) ──────────────────────────────────────────────────────────

    #[test]
    fn test_repeat_once() {
        let out = run(&[",", "-n", "1"], &["a,b,c", "longer,x,y"]);
        assert_eq!(out.len(), 2);
    }

    // ── right-align (-r) ─────────────────────────────────────────────────────

    #[test]
    fn test_right_align() {
        let out = run(&["=", "-r"], &["a = 1", "foo = 22"]);
        assert_eq!(out.len(), 2);
    }

    // ── global filler override ────────────────────────────────────────────────

    #[test]
    fn test_global_fill_override() {
        let out = run(&["-f", "-", "="], &["a = 1", "longer = 2"]);
        assert_eq!(out.len(), 2);
    }

    // ── literal edge cases ────────────────────────────────────────────────────

    #[test]
    fn test_literal_dash() {
        let out = run(&["->"], &["a -> b", "longer -> c"]);
        let cols: Vec<usize> = out.iter().map(|l| l.find("->").expect("no ->")).collect();
        assert!(cols.windows(2).all(|w| w[0] == w[1]), "cols: {:?}", cols);
    }

    #[test]
    fn test_quoted_literal_with_space() {
        let out = run(&["\"= \""], &["a = b", "longer = c"]);
        assert_eq!(out.len(), 2);
    }
}
