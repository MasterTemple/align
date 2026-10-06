# align.nvim

A Neovim plugin for the [`align`](https://github.com/MasterTemple/align) CLI, featuring:

- Live floating-window UI with preview
- Named patterns with per-filetype filtering
- Telescope integration (saved / recent / all)
- Session history with `<C-p>` / `<C-n>`
- Shell-quoting-free: all binary communication goes through `--json`

---

## Commands

```
:[range]Align                           open interactive UI
:[range]Align <name>                    apply a saved named pattern directly
:[range]Align telescope [saved|recent|all]  open Telescope picker
```

**No range** → the whole file is used (equivalent to `:%Align`).

Tab-completion works on the first argument: named patterns (filtered by the
current buffer's filetype) and `telescope` are offered.  After `telescope`,
`saved`, `recent`, and `all` are offered.

---

## UI

```
╭──────────── Align pattern  <CR> apply · <Esc> cancel · <C-p>/<C-n> history ────────────╮
│ if '=>'                                                                                  │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
╭──────────────────────────────── Preview ─────────────────────────────────────────────────╮
│ "a" if value < 1     => do_a(),                                                          │
│ "b" if value > other => do_b(),                                                          │
│ "c"                  => do_something_else(),                                             │
│ "complex"            => todo!(),                                                         │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

- Type a pattern — preview updates live (debounced, default 80 ms)
- Changed lines are highlighted with `DiffChange`
- `<C-p>` / `<C-n>` — walk through session history
- `<CR>` — apply and close
- `<Esc>` / `<C-c>` — cancel

---

## Installation

### 1. Build the binary

```sh
cd align-workspace
cargo build --release
cargo install --path align   # → ~/.cargo/bin/align
```

### 2. Install the plugin

**lazy.nvim**
```lua
{
  dir = "/path/to/align.nvim",
  dependencies = { "nvim-telescope/telescope.nvim" },  -- optional
  config = function()
    require("align").setup({
      bin         = "align",      -- or absolute path
      debounce_ms = 80,
      border      = "rounded",
      patterns = {
        { name = "constants",  pattern = "= -p 1" },
        { name = "rust-arms",  pattern = "if '=>'",  filetypes = { "rust" } },
        { name = "sql-join",   pattern = "join on = --" },
        { name = "scratch-eq", pattern = "=",         filetypes = { "" } },
      },
    })
  end,
}
```

**vim-plug / packer** — add the directory and call `require("align").setup(...)`.

### 3. Optional keymap

```lua
-- Open UI for the visual selection (or whole file if no selection)
vim.keymap.set({ "n", "x" }, "<leader>a", ":Align<CR>", { silent = true })

-- Telescope picker (all modes)
vim.keymap.set({ "n", "x" }, "<leader>A", ":Align telescope all<CR>", { silent = true })
```

---

## Configuration

```lua
require("align").setup({
  -- Path to the align binary.
  -- Automatically falls back to ~/.cargo/bin/align if not found on $PATH.
  bin = "align",

  -- Preview debounce delay in milliseconds.
  debounce_ms = 80,

  -- Maximum history entries kept per session.
  history_max = 100,

  -- Highlight group for error lines in the preview window.
  hl_error = "DiagnosticError",

  -- Border style for floating windows ("none","single","double","rounded","solid","shadow").
  border = "rounded",

  -- Named saved patterns.
  patterns = {
    -- Available in all file types:
    { name = "eq",       pattern = "=" },
    { name = "constants",pattern = "= -p 1" },
    { name = "sql",      pattern = "join on = --" },

    -- Only available in Rust buffers:
    { name = "arms",     pattern = "if '=>'",     filetypes = { "rust" } },
    { name = "fields",   pattern = ":",            filetypes = { "rust", "toml" } },

    -- Only available in buffers with no filetype (scratch):
    { name = "scratch",  pattern = "=",            filetypes = { "" } },
  },
})
```

### Named pattern fields

| Field       | Type       | Description                                        |
|-------------|------------|----------------------------------------------------|
| `name`      | `string`   | Name used in `:Align <name>` and completions       |
| `pattern`   | `string`   | Full pattern string, same syntax as CLI            |
| `filetypes` | `string[]` | Optional. Omit/`{}` = all ft. `""` = no filetype  |

---

## Telescope picker

```
:Align telescope saved   — named patterns visible for the current filetype
:Align telescope recent  — session history, most-recent first
:Align telescope all     — saved then recent, deduplicated
```

Inside the picker:

| Key    | Action                                           |
|--------|--------------------------------------------------|
| `<CR>` | Apply the selected pattern immediately           |
| `<C-e>`| Open the floating UI pre-filled (to tweak first)|

The preview panel shows the aligned result of the current range, updated as
you move through entries.

---

## Pattern syntax

The input window (and `pattern` config values) use the full `align` CLI syntax.

| Input             | Effect                                    |
|-------------------|-------------------------------------------|
| `=`               | Align on `=`                              |
| `/=>/`            | Align on `=>` (regex)                     |
| `if '=>'`         | Align on `if`, then on `=>`              |
| `= -p 2`          | Align on `=` with 2-space padding         |
| `/foo/i -r`       | Case-insensitive, right-align             |
| `-g y =`          | Only align lines that contain `y`         |
| `join on = --`    | Align SQL join syntax (4 patterns)        |

See the [align README](../README.md) for the full flag reference.

---

## Architecture

```
:Align
  │
  ├─ no args / 1 unknown arg ──→  open_ui()  (floating windows)
  │                                    │
  ├─ <name> (saved pattern) ───→  apply_named()
  │                                    │
  └─ telescope [mode] ─────────→  telescope.pick()
                                       │
                              All paths call:
                                       │
                              call_align(bin, args, lines, cb)
                                       │
                              vim.fn.jobstart({ bin, "--json" })
                                  stdin:  { args, lines }  (JSON)
                                  stdout: { output, error } (JSON)
```
