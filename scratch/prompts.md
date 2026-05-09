Use Rust to write a CLI tool called "align".

The goal is to, for each given line, match certain characters and align them by column.
- It does this by inserting or trimming a filler character (default is space, overridden with `-f`) so that the matches are aligned
- There is a default padding of 1 filler character, but it can be overwritten with `-p`
- If the matched characters are different lengths, it can be specified if they are to be left aligned or right aligned with `-l` (default) or `-r`
    - NOTE: aligning left or right

- Read the entire pattern/flags input as 1 string and parse it, keeping single/double quotes intact
- Lines that don't match any patterns are just left as normal
- Multiple patterns can be given, and they are each followed by optional flags
- Flags given before any pattern override the default for all patterns in this command
- By default, when multiple patterns are given, each line matches as many of them as they can, not having early termination

- The given patterns may be literals or regular expressions
- Regular expressions are delimeted by `/` and may be followed by regex flags: `/../gi` (support look aheads and look behinds)
- Literal expressions are delimeted by any pair of quotes: backtick `\``, single quote `'`, or double quote `"` (this way the user can easily specify one or two quote types without having to escape it)
- Literal expressions may also, if not containing a space, not be surrounded by (omit) quotes
    - Edge cases:
    - A one backtick, single quote, double quote, or slash may not be a literal on its own (it must be wrapped in another pair of quotes)
    - A `-` may be interpreted as literal if it does not match to a flag: `-`, `--`, `->` and so on

Only Global Flags:
- `-g`: Similar to how Vim has a global command `:g` which filters the input to only operate on certain lines, this flag will only perform the alignment on lines when every pattern matches
2. `-d`: Delete all lines that don’t have any match
3. `-D`: Delete all lines that don’t have every match
4. `-E`: Specify the RegEx engine

Each Alignment Flags:
1. `-f {n}`: the filler character for that particular alignment
2. `-p {n}`: the amount of padding required to be around the match (NOTE: this doesn't add space if it already exists)
  - `-pl {n}`: the amount of left padding around a match
  - `-pr {n}`: the amount of right padding around a match
3. `-l`: left align the text (default: true)
4. `-r`: right align the text
5. `-w {lit|pat}`: specify the word-bounds delimeter to filter the current alignment pattern (whether it is a pattern or literal) (default: `/[^A-z0-9_]/`)
6. `-W`: inverse of `-w`, do not require match to be delimeted by word bounds
7. `-n {n}`: specify the number of times to repeat this pattern (`*` means infinite). note: use the flags for this pattern that come after this one (this doesn't need to be placed as the last flag of a pattern)
8. `-c {lit|pat}`: the context that should be aligned
    - this is performed on the slice between the last alignment (or the start of the line if this is the first match) and this match
    - NOTE: `^` and `$` should match the beginning/end of this section, not the entire line
    - for example: `align . -p 0 -c /\d+$/` matches `.` in `3.1` and `72.0` will align at the `.`, but it will shift `3.` over giving
```
 3.1
72.0
```
whereas `align . -p 0` by itself would only shift over the `.`
```
3 .1
72.0
```

9. `-C`: align the whole slice (between last and current match) as context (basicaly `-c /^.*$/`)

NOTE: if an alignment flag is provided before any patterns are given, it is applied globally

Config File: (`~/.config/align/config.toml`) (write one with the defaults if it does not exist)
- Default padding character (default: ' ')
- Default padding amount (default: 1)
- Default flags for certain characters/delimeters:
    - flags: `-f`, `-p`, `-pl`, `-pr`, `-l/r`, `-w`, `-W` (`word = ""`), `-c`, and `-C` (`context = ""`)
    - example: `"=" = { fill = ' ', pad = 1,  }`
    - example: `"," = { fill = ' ', pad = { left = 0, right = 1 },  }`
    - example: `"." = { pad = 0, context = "/\d+$/" }`
    - example: `"?" = { align = "left", word = "/\d+$/" }`
- Default word bounding for literals (default: `/[^A-z0-9_]/`)
- Default word bounding for regex (default: `/[^A-z0-9_]/`)
- Specify the regex engine (default: `fancy_regex`)

I want to be able to use it as a CLI tool or in Vim like `:'<,'>!align`

- You may use external crates
- Use `directories` for `ProjectDirs::from("", "", "Align")`
- Use crate `fancy_regex` as the default regex engine, and `regress` as another option
    - Create an enum `RegexEngines` that stores the different engines as variants, and use methods on there to create an abstraction for the different regex engines and different regex matches

---


Config:
- Specify the regex engine (default: `fancy_regex`)

- Use crate `fancy_regex` as the default regex engine, and `regress` as another option
- Create an enum `RegexEngines`
I will add this myself, it is already complex enough

Note

Examples:

1

```
let some_var = 3.1;
let another_var = 72.0;
```

`align =`

```
let some_var    = 3.1;
let another_var = 72.0;
```

2

```
let some_var = 3.1;
let another_var = 72.0;
```

`align = . -p 0 -w`

```
let some_var    =  3.1;
let another_var = 72.0;
```

---

Small fixes:
- Added `.next()` to engine.rs in `CompiledRegex::Regress(re) => re.find_from(text, start).next().map(|m|`
- Renamed `fancy_regex` to `fancy-regex` in `Cargo.toml`

Here are the failed tests:

```
failures:

---- tests::tests::test_align_colon stdout ----

thread 'tests::tests::test_align_colon' (77787) panicked at src/tests.rs:38:9:
cols: [4, 3, 5]
lines: ["name: Alice", "age: 30", "email: alice@example.com"]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- tests::tests::test_literal_dash stdout ----

thread 'tests::tests::test_literal_dash' (77802) panicked at src/tests.rs:10:46:
parse failed: "no patterns given"

---- tests::tests::test_context_decimal stdout ----

thread 'tests::tests::test_context_decimal' (77795) panicked at src/tests.rs:163:9:
assertion `left == right` failed: dot not aligned: ["3.1", "72.0"]
  left: 1
 right: 2

---- tests::tests::test_delete_missing_match_two_patterns stdout ----

thread 'tests::tests::test_delete_missing_match_two_patterns' (77797) panicked at src/tests.rs:114:9:
assertion `left == right` failed
  left: 0
 right: 1

---- tests::tests::test_pad_two stdout ----

thread 'tests::tests::test_pad_two' (77804) panicked at src/tests.rs:138:17:
insufficient left pad in: "a=1"


failures:
    tests::tests::test_align_colon
    tests::tests::test_context_decimal
    tests::tests::test_delete_missing_match_two_patterns
    tests::tests::test_literal_dash
    tests::tests::test_pad_two

test result: FAILED. 17 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Another test to add:

When I run `align join on = --`

on

```
join some_table T on T.onefield=O.twofield, -- some comment
left join some_other_table O on O.redfield = T.bluefield, -- another comment!
```

it should result in

```
     join some_table T       on T.onefield = O.twofield,  -- some comment
left join some_other_table O on O.redfield = T.bluefield, -- another comment!
```

but instead it is

```
     join some_tab           le T on T.onefi              el d=O.twofield, -- some comment
left join some_other_table O on O.redfield = T.bluefield, -- another comment!
```
