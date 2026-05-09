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
        // Colons with a space after them — punctuation, so word-bound is off by default.
        // The colon must end up in the same column across all lines.
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
        assert_eq!(out.len(), 2);
        // Both = and : should be aligned
        let eq_cols: Vec<usize> = out.iter().map(|l| l.find('=').unwrap()).collect();
        let co_cols: Vec<usize> = out.iter().map(|l| l.find(':').unwrap()).collect();
        assert_eq!(eq_cols[0], eq_cols[1], "= not aligned: {:?}", out);
        assert_eq!(co_cols[0], co_cols[1], ": not aligned: {:?}", out);
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
        // Only "a = 1: x" has both = and :
        assert_eq!(out.len(), 1);
        assert!(out[0].contains('=') && out[0].contains(':'));
    }

    #[test]
    fn test_global_match_all() {
        // -g: only align lines where ALL patterns match; others pass through unchanged
        let out = run(&["-g", "=", ":"], &["a = 1: x", "b = 2", "c = 3: z"]);
        // All 3 lines returned (none deleted)
        assert_eq!(out.len(), 3);
        // The non-matching line is unchanged
        assert_eq!(out[1], "b = 2");
    }

    // ── padding flags ─────────────────────────────────────────────────────────

    #[test]
    fn test_pad_zero() {
        let out = run(&["=", "-p", "0"], &["a =1", "longer =2"]);
        assert_eq!(out.len(), 2);
        // = columns must be equal
        let cols: Vec<usize> = out.iter().map(|l| l.find('=').unwrap()).collect();
        assert_eq!(cols[0], cols[1], "= not aligned: {:?}", out);
    }

    #[test]
    fn test_pad_two() {
        // Use spaced input so = is word-boundary-free and clearly separated
        let out = run(&["=", "-p", "2"], &["a = 1", "longer = 2"]);
        // Check that there are at least 2 spaces before = on every line
        for line in &out {
            if let Some(pos) = line.find('=') {
                let before = &line[..pos];
                let trailing_spaces = before.chars().rev().take_while(|&c| c == ' ').count();
                assert!(
                    trailing_spaces >= 2,
                    "insufficient left pad ({}) in: {:?}",
                    trailing_spaces,
                    line
                );
            }
        }
    }

    // ── filler char ──────────────────────────────────────────────────────────

    #[test]
    fn test_fill_char() {
        let out = run(&["=", "-f", "."], &["a = 1", "longer = 2"]);
        assert_eq!(out.len(), 2);
        // The shorter line should have dots inserted before =
        let short = &out[0];
        assert!(short.contains('.'), "expected dots in: {:?}", short);
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
    fn test_word_bound_on_for_word_patterns() {
        // `foo` is a word pattern → word-bound ON by default
        // "foo" at word boundary matches; "foobar" does not
        let out = run(
            &["foo"],
            &[
                "foo = 1",
                "foobar = 2", // "foo" here is NOT at a word boundary on the right
            ],
        );
        // "foobar = 2" has no word-bound match → passes through unchanged
        assert_eq!(out[1], "foobar = 2");
    }

    #[test]
    fn test_word_bound_off_for_punctuation() {
        // `=` is punctuation → word-bound OFF by default, matches even inside words
        let out = run(&["="], &["a=1", "longer=2"]);
        // Both lines should have = aligned
        let cols: Vec<usize> = out.iter().map(|l| l.find('=').unwrap()).collect();
        assert_eq!(cols[0], cols[1], "= not aligned: {:?}", out);
    }

    #[test]
    fn test_explicit_no_word_bound() {
        // -W disables word-bound even for word patterns
        let out = run(&["foo", "-W"], &["foo = 1", "foobar = 2"]);
        // Both should match "foo"; foobar's foo is also matched
        assert_eq!(out.len(), 2);
        let cols: Vec<usize> = out.iter().map(|l| l.find("foo").unwrap()).collect();
        assert_eq!(cols[0], cols[1]);
    }

    // ── repeat (-n) ──────────────────────────────────────────────────────────

    #[test]
    fn test_repeat_once() {
        let out = run(&[",", "-n", "1"], &["a,b,c", "longer,x,y"]);
        assert_eq!(out.len(), 2);
        // Only first comma aligned; columns should be equal
        let cols: Vec<usize> = out.iter().map(|l| l.find(',').unwrap()).collect();
        assert_eq!(cols[0], cols[1], "first , not aligned: {:?}", out);
    }

    // ── right-align (-r) ─────────────────────────────────────────────────────

    #[test]
    fn test_right_align() {
        let out = run(&["=", "-r"], &["a = 1", "foo = 22"]);
        assert_eq!(out.len(), 2);
        let cols: Vec<usize> = out.iter().map(|l| l.find('=').unwrap()).collect();
        assert_eq!(cols[0], cols[1], "= not aligned: {:?}", out);
    }

    // ── global filler override ────────────────────────────────────────────────

    #[test]
    fn test_global_fill_override() {
        let out = run(&["-f", ".", "="], &["a = 1", "longer = 2"]);
        assert_eq!(out.len(), 2);
        // shorter line should have dots
        assert!(out[0].contains('.'), "expected dots: {:?}", out);
    }

    // ── literal edge cases ────────────────────────────────────────────────────

    #[test]
    fn test_literal_dash_arrow() {
        // `->` starts with `-` but is not a known flag → treated as literal
        let out = run(&["->"], &["a -> b", "longer -> c"]);
        let cols: Vec<usize> = out.iter().map(|l| l.find("->").expect("no ->")).collect();
        assert!(cols.windows(2).all(|w| w[0] == w[1]), "cols: {:?}", cols);
    }

    #[test]
    fn test_literal_double_dash() {
        // `--` is a literal
        let out = run(&["--"], &["a -- b", "longer -- c"]);
        let cols: Vec<usize> = out.iter().map(|l| l.find("--").expect("no --")).collect();
        assert!(cols.windows(2).all(|w| w[0] == w[1]), "cols: {:?}", cols);
    }

    #[test]
    fn test_quoted_literal_with_space() {
        let out = run(&["\"= \""], &["a = b", "longer = c"]);
        assert_eq!(out.len(), 2);
    }

    // ── join example ─────────────────────────────────────────────────────────

    #[test]
    fn test_join_example() {
        // `align join on = --`
        // Multiple patterns: word "join", word "on", punctuation "=", literal "--"
        let out = run(
            &["join", "on", "=", "--"],
            &[
                "join some_table T on T.onefield=O.twofield, -- some comment",
                "left join some_other_table O on O.redfield = T.bluefield, -- another comment!",
            ],
        );
        assert_eq!(out.len(), 2);

        // "join" must be in the same column on both lines
        let join_cols: Vec<usize> = out
            .iter()
            .map(|l| l.find("join").expect("no join"))
            .collect();
        assert_eq!(
            join_cols[0], join_cols[1],
            "join not aligned:\n{:?}\n{:?}",
            out[0], out[1]
        );

        // "on" (as a whole word) must be in the same column on both lines.
        // Use word-boundary aware search: find " on " or "on " at start etc.
        let on_col = |line: &str| -> usize {
            // find the standalone word "on"
            let mut i = 0;
            let bytes = line.as_bytes();
            while i + 2 <= bytes.len() {
                if &bytes[i..i + 2] == b"on" {
                    let before_ok =
                        i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
                    let after_ok = i + 2 >= bytes.len()
                        || !bytes[i + 2].is_ascii_alphanumeric() && bytes[i + 2] != b'_';
                    if before_ok && after_ok {
                        return i;
                    }
                }
                i += 1;
            }
            panic!("no standalone 'on' in: {:?}", line);
        };
        let on_cols: Vec<usize> = out.iter().map(|l| on_col(l)).collect();
        assert_eq!(
            on_cols[0], on_cols[1],
            "on not aligned:\n{:?}\n{:?}",
            out[0], out[1]
        );

        // "=" must be in the same column
        let eq_cols: Vec<usize> = out.iter().map(|l| l.find('=').expect("no =")).collect();
        assert_eq!(
            eq_cols[0], eq_cols[1],
            "= not aligned:\n{:?}\n{:?}",
            out[0], out[1]
        );

        // "--" must be in the same column
        let dc_cols: Vec<usize> = out.iter().map(|l| l.find("--").expect("no --")).collect();
        assert_eq!(
            dc_cols[0], dc_cols[1],
            "-- not aligned:\n{:?}\n{:?}",
            out[0], out[1]
        );
    }

    #[test]
    fn test_join_example_expected_output() {
        // Verify the exact expected output from the spec
        let out = run(
            &["join", "on", "=", "--"],
            &[
                "join some_table T on T.onefield=O.twofield, -- some comment",
                "left join some_other_table O on O.redfield = T.bluefield, -- another comment!",
            ],
        );
        let expected = [
            "     join some_table T       on T.onefield = O.twofield,  -- some comment",
            "left join some_other_table O on O.redfield = T.bluefield, -- another comment!",
        ];
        assert_eq!(
            out[0], expected[0],
            "\ngot:      {:?}\nexpected: {:?}",
            out[0], expected[0]
        );
        assert_eq!(
            out[1], expected[1],
            "\ngot:      {:?}\nexpected: {:?}",
            out[1], expected[1]
        );
    }
}
