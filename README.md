# align

Line up text in columns by matching literals or regular expressions.

One repository, three ways to use it:

| | |
|---|---|
| **CLI** `align` | a filter for the shell or Vim (`:'<,'>!align =`) |
| **Neovim plugin** | `:Align` with a live preview, saved patterns, Telescope |
| **Rust library** `align-lib` | the engine, with no I/O |

```rust
match value {                            match value {
    "a" if value < 1 => do_a(),              "a" if value < 1     => do_a(),
    "b" if value > other => do_b(),   →      "b" if value > other => do_b(),
    "c" => do_something_else(),              "c"                  => do_something_else(),
}                                        }
```
`align if '=>'`

```sql
join some_table T on T.onefield=O.twofield, -- some comment
left join some_other_table O on O.redfield = T.bluefield, -- another comment!
```
`align join on = --` gives
```sql
     join some_table T       on T.onefield = O.twofield,  -- some comment
left join some_other_table O on O.redfield = T.bluefield, -- another comment!
```

---

## Install

### Neovim (lazy.nvim)

Requires Neovim 0.10+ and a Rust toolchain (`cargo`). The binary is built
automatically on install and update. The plugin uses its own build, so
nothing needs to be on your `$PATH`.

```lua
{ "MasterTemple/align", opts = {} }
```

If the build fails or you update Rust, run `:Lazy build align`. Then run
`:checkhealth align` to check everything is working.

### CLI only

```sh
cargo install --git https://github.com/MasterTemple/align align-cli   # installs `align`
```

---

## How it works

1. **Patterns are matched left to right.** In each line, the second pattern is searched
   for after the first pattern's match, the third after the second's, and so on. If a
   pattern isn't found, it is skipped and the next one is searched for from the same spot.
2. **Each pattern matches once** unless you ask for more with `-n N`, `-n *` or `/regex/g`.
3. Each (pattern, occurrence) is a **column**. Columns are laid out left to right, and every
   line that has a column gets its match moved to the same display column.
4. **Gaps are rebuilt, not just padded.** The whitespace before and after each match is
   replaced with the padding (default 1 space) plus whatever is needed to line up. Running
   align again is a no-op, and re-running after an edit tightens things back up.
5. Indentation is kept. Padding is never added at the start or end of a line.
6. Lines with no match (or filtered out with `-g` / `-v` / `-e`) pass through unchanged.

```text
$ align =                 $ align = -n 2            $ align : =
a   = 1 = x = 9           a   = 1   = x = 9         x      : a  = 1 = 2
bbb = 100 = y = 8         bbb = 100 = y = 8         longer : bb = 1 = 22
```

## Usage

```
align [global flags] <pattern> [flags] [<pattern> [flags] ...]  < input
```

All arguments are joined with spaces and parsed as one string. So
`align if '=>'` and `align "if '=>'"` mean the same thing. To put a space inside
a literal, quote it inside the argument: `align "'= '"`. In Vim and the shell,
characters like `#`, `;` and `|` need quotes: `:'<,'>!align '#'`.

### Patterns

| Pattern | Meaning |
|---|---|
| `=` `->` `join` | Literal (no spaces). A `-` word that isn't a flag is a literal: `->`, `--`, `-` |
| `'= '` `"'"` `` `"` `` | Quoted literal (any of the three quote characters). Use one quote type to wrap another. |
| `/=+/` `/\d+/i` | Regex. Flags: `i` `m` `s`, `x` (fancy_regex only), and `g` (same as `-n *`) |
| `/` `//` | A lone `/` or a word starting with `//` is a literal |

**Word boundaries:** a match whose edge is a word character must not run into
another word character. `foo` doesn't match inside `foobar`, but `=` matches in
`a=1`. Use `-W` to turn this off, or `-w PAT` to set what counts as a boundary.

### Global flags

| Flag | |
|---|---|
| `-g PAT` | Only align lines matching `PAT` (like Vim's `:g`) |
| `-v PAT` | Don't align lines matching `PAT` (like Vim's `:v`) |
| `-e` | Only align lines where **every** pattern matches |
| `-d` | Delete lines with **no** match |
| `-D` | Delete lines that are missing **any** pattern |
| `-E ENGINE` | `fancy_regex` (default; Rust syntax + lookaround) or `regress` (JavaScript syntax) |

### Pattern flags

These go after a pattern. Placed before the first pattern, they become the
default for every pattern.

| Flag | |
|---|---|
| `-n N` / `-n *` | Match up to `N` times / as many times as possible (default 1) |
| `-p N`, `-pl N`, `-pr N` | Spaces between the match and its neighbours: both sides / left / right (default 1) |
| `-l` / `-r` | Line up the **left** (default) / **right** edges of the matches |
| `-j` | Right-justify the text before the match (`-C` is an alias) |
| `-c PAT` | Insert fill before the last `PAT` in the text before the match (`^` and `$` refer to that text) |
| `-f C` | Fill character for the alignment gap (padding stays spaces) |
| `-w PAT` / `-W` | Word-boundary characters / no word boundary |

```text
$ align /=+/ -r          $ align = -j             $ align . -p 0 -c /\d+$/
a     = 1                apple = 1                 3.14
bb   == 2                  fig = 22               72.0
ccc === 3                                          1.618

$ align = -f .           $ align /\d+/ -r -f 0    $ align ( , -pl 0 -n *
intro ..... = 1          id 0007                  f   ( a  , bb, c)
conclusion  = 42         id 1234                  fff ( aaa, b , cc)

$ align -v '#' =         $ align -e = :           $ align -D = :
a    = 1                 a   = 1 : x              a   = 1 : x
b = 2 # skip             bb = 2                   ccc = 3 : z
cccc = 3                 ccc = 3 : z
```

How the fill character is placed: punctuation (`.`, `-`) acts as a dot leader,
with a space on each side. Alphanumeric characters (`0`) go right against the
match, which gives zero-padding. A space is just a space.

### Other options

`align --help`, `align --version`, `align --config-path`, and `align --json`
(see [JSON protocol](#json-protocol)). Errors exit with status 2 and point at
the offending column:

```text
$ align = -x
align: unknown flag -x (quote it to align on the literal text, e.g. '-x')
    = -x
      ^
```

---

## Config file

The config file lives at `~/.config/align/config.toml`, or wherever
`align --config-path` says. A fully commented template is created on first
run. Set `ALIGN_CONFIG=/path` to use another file, or `ALIGN_CONFIG=` (empty)
to ignore it.

```toml
fill = ' '                       # alignment fill character
pad = 1                          # or { left = 0, right = 1 }
word_bound_literal = '/[^A-Za-z0-9_]/'
word_bound_regex = '/[^A-Za-z0-9_]/'
engine = 'fancy_regex'           # or 'regress'
tabstop = 8                      # the Neovim plugin passes the buffer's 'tabstop'

# Defaults whenever a pattern is used, keyed by the pattern as typed.
[patterns]
"," = { pad = { left = 0, right = 1 } }
"." = { pad = 0, context = '/\d+$/' }
'/=+/' = { align = 'right' }
```

Per-pattern keys: `fill`, `pad`, `align` (`'left'`/`'right'`), `word` (`''` turns
word boundaries off), `context` (`''` means the whole slice, like `-j`), and
`repeat` (a number or `'*'`). Unknown keys are reported as errors.

Precedence, from lowest to highest: built-in defaults → top-level config →
`[patterns]` config → flags before the first pattern → a regex's `g` flag →
flags after the pattern.

---

## Neovim plugin

```
:[range]Align                      open the input window with a live preview
:[range]Align <pattern>            align right away, e.g. :Align if '=>'
:[range]Align <name>               apply a saved pattern
:[range]Align telescope [saved|recent|all]
```

With no range, `:Align` works on the whole buffer. From visual mode, use
`:'<,'>Align`. Each alignment is a single undo step.

In the input window: `<CR>` applies, `<Esc>` / `<C-c>` cancels, and `<C-p>` / `<C-n>`
(or `<Up>` / `<Down>`) walk through this session's history. Changed lines are
highlighted in the preview. Errors appear in the preview, and the offending
column is underlined in the input.

```lua
{
  "MasterTemple/align",
  -- optional: dependencies = { "nvim-telescope/telescope.nvim" },
  keys = {
    { "<leader>a", ":Align<CR>", mode = { "n", "x" }, desc = "Align" },
  },
  opts = {
    bin = nil,           -- path to the binary; nil = plugin build → $PATH → ~/.cargo/bin
    debounce_ms = 50,    -- preview delay
    history_max = 100,
    border = "rounded",
    patterns = {         -- saved patterns: :Align <name>, completion, Telescope
      { name = "arms", pattern = "if '=>'", filetypes = { "rust" } },
      { name = "sql",  pattern = "join on = --" },
      { name = "eq",   pattern = "=" },  -- filetypes omitted = everywhere; "" = no filetype
    },
  },
}
```

In the Telescope picker, `<CR>` applies the selected pattern and `<C-e>` opens it
in the input window to edit first. The preview shows the aligned result.

---

## Library

```toml
[dependencies]
align-lib = { git = "https://github.com/MasterTemple/align" }
```

```rust
use align_lib::{align, Command, Config};

let out = align("=", &["foo = 1", "foobar = 2"], &Config::default())?;
assert_eq!(out, ["foo    = 1", "foobar = 2"]);

// Parse once, apply many times. Errors carry a column into the pattern.
let cmd = Command::parse("if '=>'", &Config::default())?;
let out = cmd.apply(&lines);
```

`Config::from_toml` parses a config file's contents. The library never reads or
writes files itself.

### JSON protocol

`align --json` reads one request from stdin and writes one response. Editor
integrations use this mode, so no shell quoting is involved.

```json
{ "pattern": "if '=>'", "lines": ["…", "…"], "tabstop": 4 }
{ "output": ["…", "…"], "error": null }
{ "output": null, "error": { "message": "unknown flag -x …", "col": 2 } }
```

`col` is a 0-based character offset into `pattern`, or `null`. The old v1 form
(`{"args": [...]}` with a string `error`) is still accepted.

---

## Development

```sh
cargo test                                   # unit, golden (crates/align-lib/tests/cases.txt), fuzz, CLI
cargo build --release && nvim --headless --clean -l tests/plugin.lua   # plugin tests
```

To add a golden case, append to `crates/align-lib/tests/cases.txt`:

```text
## description
$ = -n 2 :
< input line
> expected line
```

## Changes from 0.1

- Patterns are matched in order along the line, and each matches **once** by default.
  Before, every pattern matched everywhere, so a later pattern could break an
  earlier column. Use `-n *` or `/re/g` for the old repeat behaviour.
- Gaps are normalized: existing whitespace around a match is rebuilt from the padding.
- `-r` now works. `-j` is new (`-C` is kept as an alias). `-f` uses leader style.
- Right padding is exact, tabs are measured with `tabstop`, and no padding is
  added at the start or end of a line.
- `^`, `\b` and lookbehind see the whole line.
- Invalid regexes, unknown flags or engines, and config typos are now errors
  instead of being ignored.
- Every pattern flag can be a global default. The `word_bound_*` settings and
  `align = 'right'` in the config now take effect. The old default
  `/[^A-z0-9_]/` is migrated to `/[^A-Za-z0-9_]/`.
- Neovim: install with lazy.nvim (the binary is built automatically).
  `:Align <pattern>` applies directly. Added `:checkhealth align`.
