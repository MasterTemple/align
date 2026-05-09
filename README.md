# align

A CLI tool that aligns text by matched characters or patterns. Designed to be used standalone or piped through Vim (`:'<,'>!align`).

## Installation

```sh
cargo install --path .
```

## Quick Start

```sh
# Align `=` signs across lines from stdin
echo "foo = 1
foobar = 2
x = 3" | align =

# Output:
# foo    = 1
# foobar = 2
# x      = 3
```

## Usage

```
align [global-flags] <pattern> [flags] [<pattern> [flags] ...]
```

Lines are read from stdin. Patterns are matched in order; lines without any match are passed through unchanged.

---

## Patterns

### Literal Patterns

Literals may be:
- **Unquoted** (no spaces): `align =` (NOTE: Vim or the shell may treat certain characters like `;` or `#` specially, in that case, wrap with quotes)
- **Quoted** with backtick, single, or double quote: `align '='`, `align "="`, `` align `=` ``
- A `-` that doesn't match a flag is treated as a literal: `align ->`, `align --`

**Edge cases:** A lone `` ` ``, `'`, `"`, or `/` must be wrapped in another pair of quotes to be a literal.

### Regex Patterns

Delimited by `/`:

```sh
align /=>/          # match =>
align /\s*=\s*/     # match = with surrounding whitespace
align /foo/gi       # case-insensitive, global (find all)
```

Supported regex flags: `i` (case-insensitive), `s` (dot-all), `m` (multiline).

---

## Global Flags

These apply to the entire command and must come before any pattern:

| Flag | Description |
|------|-------------|
| `-g` | Only align lines where **all** patterns match (like Vim's `:g`) |
| `-d` | Delete lines with **no** match |
| `-D` | Delete lines that don't have **every** match |
| `-E {engine}` | Set regex engine: `fancy_regex` (default) or `regress` |

---

## Per-Pattern Flags

These follow immediately after their pattern:

| Flag | Description |
|------|-------------|
| `-f {char}` | Filler character for this alignment (default: space) |
| `-p {n}` | Padding on both sides of the match |
| `-pl {n}` | Left padding |
| `-pr {n}` | Right padding |
| `-l` | Left-align (default) |
| `-r` | Right-align |
| `-w {pat}` | Word-boundary delimiter (default: `/[^A-Za-z0-9_]/`) |
| `-W` | Disable word-boundary checking |
| `-n {n\|*}` | Repeat this pattern `n` times (`*` = unlimited) |
| `-c {pat}` | Context pattern — aligns the slice before the match |
| `-C` | Use the entire slice before the match as context |

**Flags before any pattern** override defaults for all patterns.

---

## Examples

### Basic alignment

```sh
printf 'a = 1\nfoobar = 2\nx = 3\n' | align =
# a      = 1
# foobar = 2
# x      = 3
```

### Multiple patterns

```sh
printf 'a = 1: foo\nlonger = 22: bar\n' | align = :
# a      = 1:  foo
# longer = 22: bar
```

### Regex alignment

```sh
printf 'key => value\nlonger_key => other\n' | align /=>/
# key        => value
# longer_key => other
```

### Dot alignment with context (`-c`)

```sh
printf '3.14\n72.0\n1.618\n' | align . -p 0 -c '/\d+$/'
#  3.14
# 72.0
#  1.618
```

### Custom filler

```sh
printf 'a = 1\nfoobar = 2\n' | align = -f .
# a...... = 1
# foobar  = 2
```

### Delete non-matching lines

```sh
printf 'match = yes\nno match here\nalso = yes\n' | align -d =
# match = yes
# also  = yes
```

### Only align when all patterns match (`-g`)

```sh
printf 'a = 1: x\nb = 2\nc = 3: z\n' | align -g = :
# a = 1: x  ← aligned (has both = and :)
# b = 2      ← unchanged (missing :)
# c = 3: z  ← aligned
```

### Vim usage

In visual mode, select lines and run:

```vim
:'<,'>!align =
:'<,'>!align /=>/ -p 2
:'<,'>!align = : -g
```

---

## Config File

Located at `~/.config/align/config.toml` (created with defaults on first run):

```toml
# Default filler character
fill = ' '

# Default padding
pad = 1

# Regex engine: 'fancy_regex' or 'regress'
engine = 'fancy_regex'

# Word boundary for literals
word_bound_literal = '/[^A-Za-z0-9_]/'

# Word boundary for regex patterns
word_bound_regex = '/[^A-Za-z0-9_]/'

# Per-pattern defaults
[patterns."="]
fill = ' '
pad = 1

[patterns.","]
pad = { left = 0, right = 1 }

[patterns."."]
pad = 0
context = '/\d+$/'
```

### Per-pattern config keys

| Key | Type | Description |
|-----|------|-------------|
| `fill` | char | Filler character |
| `pad` | int or `{ left, right }` | Padding |
| `align` | `"left"` or `"right"` | Alignment direction |
| `word` | string | Word-boundary pattern |
| `context` | string | Context pattern |

---

## Regex Engines

| Engine | Crate | Notes |
|--------|-------|-------|
| `fancy_regex` | [fancy_regex](https://crates.io/crates/fancy_regex) | Default. Supports lookaheads/lookbehinds. |
| `regress` | [regress](https://crates.io/crates/regress) | ES2021-compatible. |

Switch engines:

```sh
align -E regress /(?<=:)\s*\w+/
```

---

## How It Works

1. Parse all patterns and flags from the argument string (quotes preserved).
2. For each pattern, find all matches in each line (respecting word boundaries, repeat limits).
3. Compute the maximum column position across all matched lines.
4. Insert or trim filler characters so every match lands at that column.
5. Apply padding rules around each match.
6. Output the result (applying deletion filters if set).

Multiple patterns are applied sequentially; each pass sees the output of the previous.
