# align: rewrite plan

Goal: replace https://github.com/MasterTemple/align with a single repo that ships
1. a Rust library (`align-lib`)
2. a CLI (`align`)
3. a Neovim plugin installable with lazy.nvim, which builds the binary itself (needs `cargo`)

Sources reviewed: this workspace (`align-lib`, `align`, `align.nvim`), plus upstream
`README.md`, `scratch/prompts.md` (the original spec) and `scratch/tests.md` (hand-written cases).
Every bug below was reproduced against `target/debug/align` built from this workspace.

---

## 1. Decisions (agreed)

| Topic | Decision |
|---|---|
| Multiple patterns | **Sequential.** On each line, pattern *k+1* is searched starting where pattern *k* matched. If a pattern isn't found, skip it and search for the next one from the same position. Don't stop early. |
| Repeats | **Default `-n 1`.** `-n N` repeats up to N times and `-n *` repeats without limit. `/re/g` is shorthand for `-n *`. |
| Columns | Each (pattern index, occurrence index) is one column. All lines share the same column order, and a line that is missing a column leaves it empty. |
| Gap whitespace | **Normalize.** Existing whitespace or fill in the gap before a match is stripped, then rebuilt from padding plus the alignment gap. Re-running after edits tightens the result again (idempotent). |
| `-r` | **Right-align the match.** The right edges of matches with different lengths line up (e.g. `/=+/`). |
| `-j` (new) | **Right-justify the segment before the match.** Same as the existing `-C`, which stays as an alias. |
| `-f` fill | **Leader style.** Padding is always spaces, and only the extra alignment gap uses the fill char: `a ..... = 1` / `foobar  = 2`. |

## 2. Decisions I made (say so if you disagree)

- **No padding at the edges of a line.** Left padding only applies when non-whitespace text comes before the match, and right padding only when text follows it. This fixes `align foo` turning `foo = 2` into ` foo = 2`, and stops trailing whitespace.
- **Errors are reported, never swallowed.** This covers invalid regexes (pattern, `-g`, `-v`, `-w`, `-c`), unknown engine names, unknown flags, a missing flag value and a multi-char `-f`. The CLI exits non-zero and JSON mode returns `error`.
- **Every per-pattern flag also works as a global default** when it comes before the first pattern (`-n`, `-w`, `-W`, `-c`, `-C`/`-j`, `-r`, `-l`, `-f`, `-p*`), as the README already promises.
- **Tabs** count as whitespace in a gap. Their width follows a `tabstop` setting (default 8; the plugin passes the buffer's `tabstop`).
- **Config:** `word_bound_literal` and `word_bound_regex` actually get used. The default becomes `/[^A-Za-z0-9_]/` (the current `A-z` range also matches `[\]^_` and backtick). `align = "right"` works. `context = ""` means whole-slice. The library never writes files; only the CLI creates the default config.
- **Plugin `:Align <args>`:** if the first arg is a saved name, apply that pattern. Otherwise treat all the args as a pattern and apply it directly (like `:!align`). `:Align` with no args opens the UI.
- **Plugin ↔ binary:** keep the subprocess approach (`align --json`), but send the **raw pattern string** and let Rust parse it. This deletes the duplicate Lua tokenizer. A native Lua module (mlua cdylib) is faster but fragile across LuaJIT and macOS, so it's not worth it for now.

---

## 3. Feature inventory and status

✅ works · ⚠️ partly works / has bugs · ❌ broken or missing

### Pattern syntax
| Feature | Status | Notes |
|---|---|---|
| Bare literal `=` | ✅ | |
| Quoted literal `'…'` `"…"` `` `…` `` | ✅ | `\<quote>` escape works |
| `-`-prefixed literal (`->`, `--`, `-`) | ⚠️ | Any all-alphabetic `-word` is treated as a flag and **silently dropped** (B9) |
| Lone `/` or `//` as a literal | ✅ | |
| Regex `/…/flags` (`i s m`) | ⚠️ | `g` is ignored (it will mean `-n *`); `^`, `\b` and lookbehind break after the first match (B5) |
| Engines `fancy_regex` / `regress` (`-E`) | ⚠️ | An unknown engine silently falls back to fancy (B9) |
| Zero-width matches (`/(?<=:)/`) | ❌ | Always skipped. They should be allowed as align points. |

### Global flags
| Flag | Status | Notes |
|---|---|---|
| `-g pat` keep lines | ✅ | |
| `-v pat` ignore lines | ✅ | |
| `-e` only align lines matching every pattern | ✅ | |
| `-d` delete lines with no match | ✅ | |
| `-D` delete lines missing a match | ✅ | |
| `-E engine` | ⚠️ | See above |
| Per-pattern flags as global defaults | ⚠️ | Only `-f -p -pl -pr -l -r` work. `-n -w -W -c -C` are silently ignored. |

### Per-pattern flags
| Flag | Status | Notes |
|---|---|---|
| `-p` / `-pl` | ✅ | |
| `-pr` / right half of `-p` | ❌ | Off by the existing space count: `= -p 2` gives 1 space after `=` (B4) |
| `-f char` | ❌ | Existing spaces aren't treated as gap, so you get `a ......=. 1` (B6) |
| `-l` / `-r` | ❌ | `-r` is parsed but never read by the aligner (B3) |
| `-w pat` / `-W` | ⚠️ | Work, but the config defaults are ignored (B8) |
| `-n N` / `-n *` | ❌ | The default is unlimited, so patterns repeat without being asked (**B1**) |
| `-c pat` context | ✅ | `. -p 0 -c /\d+$/` gives ` 3.1` / `72.0` |
| `-C` whole-slice context | ✅ | Becomes `-j` (alias kept) |

### Multiple patterns
| Feature | Status | Notes |
|---|---|---|
| Patterns in line order | ⚠️ | Only works when patterns are given in line order and each occurs once |
| Out-of-order or repeated patterns | ❌ | Independent passes break earlier columns (**B2**) |

### Config (`~/.config/align/config.toml`)
| Key | Status |
|---|---|
| `fill`, `pad`, `engine` | ✅ |
| `word_bound_literal`, `word_bound_regex` | ❌ never read |
| `[patterns."x"]` `fill`/`pad`/`word`/`context` | ✅ |
| `[patterns."x"] align = "right"` | ❌ (same cause as `-r`) |

### CLI
| Feature | Status |
|---|---|
| stdin → stdout | ✅ |
| `--json` mode | ✅ (protocol changes, see §5) |
| Exit code on errors | ❌ always 0 for bad regex, etc. |
| `--help` / `--version` | ❌ missing |
| Tabs in input | ❌ width counted as 0, so the result is misaligned (B7) |

### Neovim plugin
| Feature | Status | Notes |
|---|---|---|
| `:[range]Align` floating UI + live preview | ✅ | |
| History `<C-p>`/`<C-n>` | ✅ | Session only. Persisting it is optional. |
| Saved named patterns + filetype filter | ✅ | |
| `:Align <name>` | ✅ | |
| `:Align <pattern>` | ⚠️ | One arg opens the UI prefilled; **two or more args open an empty UI and ignore the args** (P1) |
| Telescope picker (saved/recent/all, `<C-e>` edit) | ✅ | |
| lazy.nvim install | ❌ | Plugin lives in a subdirectory, so you have to build manually and need `dir =` (P2) |
| Binary lookup | ⚠️ | Only `$PATH` and `~/.cargo/bin`, not the plugin's own build |
| Deprecated APIs | ⚠️ | `nvim_buf_get_option`, `nvim_buf_set_option`, `vim.loop`, `nvim_buf_add_highlight` |

---

## 4. Bug list (with repros)

| ID | Bug | Repro → actual |
|---|---|---|
| **B1** | Patterns repeat without `-n` (your bug) | `align = -n 2 :` on `x: a = 1 = 2` / `longer: bb = 1 = 22` gives `x      : a       = 1 = 2` / `longer : bb = 1 = 22`. Cause: `repeat` defaults to `None`, which means unlimited. |
| **B2** | Each pattern is an independent whole-line pass, so later passes undo earlier ones and pattern order is meaningless | `align // =` on `let a = 1; // c` / `let bbbb = 22; // d` leaves `//` misaligned |
| **B3** | `-r` (and config `align="right"`) does nothing | `align /=+/ -r` gives the same output as without `-r` |
| **B4** | Right padding is short by the number of existing spaces | `align = -pr 3` on `a = 1` gives `=  1` (2 spaces) |
| **B5** | `find_at` slices the string, so `^`, `\b` and lookbehind re-match at every slice start | `align /\b\w/ -n 3` on `ab cd ef` gives ` a b c d ef` |
| **B6** | With a custom fill, existing spaces aren't part of the gap | `align -f . =` gives `a ......=. 1` |
| **B7** | Tabs have width 0 and aren't treated as gap | `a\t= 1` / `bbbb = 2` gives `a\t   = 1` |
| **B8** | Config `word_bound_*` is ignored; the hard-coded default is used instead | — |
| **B9** | Errors are swallowed | `align '/[/'`, `align -E bogus =`, `align = -x` and `align -w foo =` all exit 0 and silently do nothing or ignore the flag |
| **B10** | Padding is added at the start of a line | `align foo` on `foo_x = 1` / `foo = 2` gives ` foo = 2` |
| **B11** | `align 3 -W -p 0` removes the space in `hi 3` | Resolved as intended behaviour by the "normalize" decision. Documented, not changed. |
| P1 | `:Align = :` ignores its args | see above |
| P2 | Not installable as a lazy.nvim plugin | see §5 |
| P3 | The Lua tokenizer duplicates the Rust parser, and they disagree on edge cases (e.g. `\/` inside a regex) | — |

The 29 existing tests pass, but they mostly assert "same column" with inputs where the
bugs above don't show up. They will be replaced with exact-output tests.

---

## 5. Refactor: yes, a large one

The aligner's core model (independent passes, re-searching lines that were already
modified, special-case context mode) is the root of B1, B2, B3, B4 and B10. Patching it
would mean fighting the architecture, so I'll rewrite the core around a **cell/table
model** and keep the parts that are fine (config structs, engine wrapper, most of
the tokenizer).

### New pipeline (library)
1. **Parse** the string into `Command { global: GlobalOpts, patterns: Vec<PatternSpec> }`. It returns a typed `Error` with a position for invalid input. Global defaults → config per-pattern defaults → explicit per-pattern flags (later wins).
2. **Compile** all regexes once (pattern, word bound, context, `-g`, `-v`), propagating errors.
3. **Match** each line sequentially. Keep a cursor; for each pattern, repeat up to `n` times: `find_from(line, cursor)` using the engines' native "search from position" APIs (fixes B5), then apply the word-bound check and advance. The result is the list of `(column_id, match_range)` for the line.
4. **Split** each line into cells: `seg0, m(c0), seg1, m(c1), …, tail`. Each segment has its edge whitespace trimmed (normalize), and context (`-c`/`-j`) decides where alignment fill is inserted inside a segment.
5. **Layout** column by column, left to right. Compute each line's width before the column (display width with tabstops), then target = the max over participating lines. Insert fill per the rules: pad spaces, leader-style fill, `-r` adding the length difference before the match, and no padding at line edges.
6. **Filter and emit**: `-g`/`-v`/`-e` decide participation, `-d`/`-D` drop lines.

Because the layout uses only the stored cells, never re-searched strings, earlier
columns can't be disturbed.

### Crate / repo layout (final, for upstream)
```
align/                      ← repo root = lazy.nvim plugin root
├── Cargo.toml              (workspace)
├── crates/align-lib/       library: parse, match, layout; no I/O
├── crates/align-cli/       binary `align`: argv, config file, --json, --help/--version
├── lua/align/{init,ui,telescope,bridge,health}.lua
├── plugin/align.lua
├── build.lua               lazy.nvim runs this automatically: cargo build --release
├── doc/align.txt           :help align
└── README.md
```

### lazy.nvim
```lua
{ "MasterTemple/align", build = "cargo build --release", opts = {} }
```
- `build.lua` at the root means the `build` line is optional. It runs `cargo build --release -p align-cli` and reports failures through lazy's UI.
- Binary lookup order: `opts.bin` → `<plugin_root>/target/release/align` → `$PATH` → `~/.cargo/bin/align`.
- `:checkhealth align` reports whether cargo and the binary were found, the binary's version, and whether that version matches the plugin.

### JSON protocol v2
Request: `{ "pattern": "= -n 2 :", "lines": [...], "tabstop": 4 }`. Response: `{ "output": [...] | null, "error": {message, col} | null }`.
The old `args` array stays accepted for one release.

---

## 6. Work phases

1. **Spec tests first.** Encode every decision above, plus the README and `scratch/tests.md` examples, as exact-output golden tests (`input / pattern / expected`). Many will fail at first.
2. **Library rewrite** (parse → compile → match → cells → layout) until the goldens pass. Add property tests: output equals input with whitespace removed, the result is idempotent, and every column lines up.
3. **CLI**: error exit codes, `--help`, `--version`, JSON v2, and config file creation moved here.
4. **Plugin**: move to the repo root, `build.lua`, binary lookup, raw-string protocol (drop the Lua tokenizer), the P1 fix, the deprecated API updates, `:checkhealth`, and `doc/align.txt`.
5. **Docs + migration**: rewrite the README, check every example in it against the binary, and push to the upstream repo.

## 7. Still open (minor, defaults chosen)
- Persist plugin history to disk? Default: no.
- Should the `-n N` count include the first match (`-n 2` = two occurrences)? Default: **yes**.
- `-n *` followed by later patterns: the later patterns are searched after the last repeat, and their column sits after the widest repeat count across lines.
