# align — workspace

This repo contains three crates / packages:

| Package       | Type            | Purpose                                             |
|---------------|-----------------|-----------------------------------------------------|
| `align-lib`   | Rust library    | Pure alignment logic — no I/O, embeddable anywhere  |
| `align`       | Rust binary     | CLI wrapper (legacy + new `--json` mode)            |
| `align.nvim`  | Neovim plugin   | Floating UI with live preview                       |

---

## Quick start

```sh
# Build and install the binary (adds `align` to ~/.cargo/bin)
cargo install --path align

# Or build for the current directory only
cargo build --release
export PATH="$PWD/target/release:$PATH"

# Smoke-test the JSON protocol used by the plugin
echo '{"args":["="],"lines":["foo = 1","foobar = 2"]}' | align --json
# → {"output":["foo    = 1","foobar = 2"],"error":null}

# Legacy CLI still works
echo "foo = 1\nfoobar = 2\nx = 3" | align =
```

---

## Repository layout

```
align-workspace/
├── Cargo.toml            ← workspace manifest
├── align-lib/            ← library crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs        ← public API  (align(), align_str())
│       ├── aligner.rs
│       ├── config.rs
│       ├── engine.rs
│       ├── parser.rs
│       └── tests.rs
├── align/                ← binary crate
│   ├── Cargo.toml
│   └── src/
│       └── main.rs       ← CLI + --json mode
└── align.nvim/           ← Neovim plugin
    ├── README.md
    ├── plugin/
    │   └── align.lua     ← :Align command
    └── lua/
        └── align/
            └── init.lua  ← UI, history, JSON bridge
```

---

## The `--json` mode

The Neovim plugin never invokes `align` through a shell.  Instead it calls
`align --json`, writes a JSON object to stdin, and reads one JSON object back:

**Request** (stdin):
```json
{ "args": ["if", "'=>'"], "lines": ["\"a\" if x => y", "\"bb\" if xx => yy"] }
```

**Response** (stdout, success):
```json
{ "output": ["\"a\"  if x  => y", "\"bb\" if xx => yy"], "error": null }
```

**Response** (stdout, error):
```json
{ "output": null, "error": "no patterns given" }
```

This bypasses every shell quoting issue: `#`, `;`, `"`, `=>`, backticks, etc.
all travel as ordinary JSON string characters.

---

## Using `align-lib` as a library

Add to your `Cargo.toml`:

```toml
[dependencies]
align-lib = { path = "../align-lib" }   # or publish to crates.io
```

Then:

```rust
use align_lib::{align_str, Config};

fn main() {
    let config = Config::default();
    let output = align_str(
        &["="],
        &["foo = 1", "foobar = 2", "x = 3"],
        &config,
    ).unwrap();
    for line in &output {
        println!("{}", line);
    }
    // foo    = 1
    // foobar = 2
    // x      = 3
}
```

The library surface:

```rust
// High-level: parse + align in one call
pub fn align(args: &[String], lines: &[String], config: &Config)
    -> Result<Vec<String>, String>;

pub fn align_str(args: &[&str], lines: &[&str], config: &Config)
    -> Result<Vec<String>, String>;

// Lower-level building blocks (all pub)
pub use parser::{parse_args, Command, AlignPattern, GlobalFlags};
pub use aligner::Aligner;
pub use config::Config;
```

---

## Neovim plugin setup

See [`align.nvim/README.md`](align.nvim/README.md) for full instructions.
Short version:

```lua
-- lazy.nvim
{
  dir = "/path/to/align.nvim",
  config = function()
    require("align").setup({
      bin = "align",       -- must be on $PATH or absolute path
      debounce_ms = 80,
      border = "rounded",
    })
  end,
}

-- Optional keymap
vim.keymap.set("x", "<leader>a", ":Align<CR>", { silent = true })
```

Usage:
1. Visually select lines (`V` + move)
2. Press `<leader>a` (or `:'<,'>Align`)
3. Type your pattern in the top window — preview updates live
4. `<CR>` to apply, `<Esc>` to cancel
5. `<C-p>` / `<C-n>` to cycle history

---

## Running tests

```sh
cargo test
# 29 tests, all passing
```
