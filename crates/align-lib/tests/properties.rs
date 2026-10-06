//! Randomized invariants over generated input (deterministic seed, no deps).

use align_lib::{align, Config};
use unicode_width::UnicodeWidthStr;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.next() as usize % xs.len()]
    }
}

fn gen_line(rng: &mut Rng) -> String {
    let atoms = ["a", "bb", "ccc", "x_1", "日本", "=", "==", ",", ":", "->", "//", " ", "  ", "\t", "42", "3.14"];
    let n = rng.next() % 9;
    (0..n).map(|_| rng.pick(&atoms)).collect()
}

fn non_ws(s: &[String]) -> String {
    s.iter().flat_map(|l| l.chars()).filter(|c| !c.is_whitespace()).collect()
}

fn width_before(line: &str, needle_byte: usize, tabstop: usize) -> usize {
    let mut col = 0;
    for c in line[..needle_byte].chars() {
        col += if c == '\t' { tabstop - col % tabstop } else { c.to_string().width() };
    }
    col
}

#[test]
fn invariants() {
    let config = Config::default();
    let patterns = [
        "=", "= :", ": =", ", -n *", "/=+/ -r", "-> // -n 2", "= -p 0", "= -j", ". -c /\\d+$/ -p 0",
        "/\\d+/ -r", ", -pl 0 -n * =", "-W a -n *",
    ];
    let mut rng = Rng(0x5eed);
    for round in 0..400 {
        let lines: Vec<String> = (0..1 + rng.next() % 6).map(|_| gen_line(&mut rng)).collect();
        for pat in patterns {
            let out = align(pat, &lines, &config).unwrap();
            let ctx = format!("round {round} `align {pat}`\n in:  {lines:?}\n out: {out:?}");
            assert_eq!(out.len(), lines.len(), "{ctx}");
            assert_eq!(non_ws(&out), non_ws(&lines), "text changed: {ctx}");
            assert_eq!(align(pat, &out, &config).unwrap(), out, "not idempotent: {ctx}");
        }
        // Single literal: every line's first `=` ends up in one column.
        let out = align("= -W", &lines, &config).unwrap();
        let cols: Vec<usize> = out
            .iter()
            .filter_map(|l| l.find('=').map(|b| width_before(l, b, config.tabstop)))
            .collect();
        assert!(cols.windows(2).all(|w| w[0] == w[1]), "misaligned: {lines:?} -> {out:?}");
    }
}
