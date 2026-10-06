--- align.nvim — plugin/align.lua
---
--- Registers the :Align user command.  Sourced automatically by Neovim
--- when the plugin is on the runtimepath.
---
--- Usage:
---   :[range]Align                         open interactive UI
---   :[range]Align <name>                  apply a named saved pattern
---   :[range]Align telescope [saved|recent|all]   open Telescope picker
---
--- No range → whole file (1,$).

if vim.g.loaded_align_nvim then return end
vim.g.loaded_align_nvim = true

vim.api.nvim_create_user_command("Align", function(info)
  local align = require("align")

  -- Determine the effective range.
  -- info.range == 0 means the user gave no range → use whole file.
  local first = info.range == 0 and 1              or info.line1
  local last  = info.range == 0 and vim.fn.line("$") or info.line2

  local args = info.fargs   -- list of whitespace-split arguments

  -- ── :Align telescope [saved|recent|all] ─────────────────────────────────
  if args[1] == "telescope" then
    local mode = args[2] or "all"
    require("align.telescope").pick(mode, { first_line = first, last_line = last })
    return
  end

  -- ── :Align <name>  — apply a saved pattern directly ──────────────────────
  if #args == 1 then
    -- Could be a named pattern OR a bare single-token pattern like "=".
    -- We prefer named patterns; if the name isn't found, fall through to UI
    -- with the arg pre-filled so the user can see / fix it.
    local ft  = vim.api.nvim_buf_get_option(0, "filetype") or ""
    local raw = nil
    for _, p in ipairs(align.config.patterns) do
      local fts = p.filetypes
      local ft_ok = not fts or #fts == 0
      if not ft_ok then
        for _, f in ipairs(fts) do if f == ft then ft_ok = true; break end end
      end
      if ft_ok and p.name == args[1] then raw = p.pattern; break end
    end
    if raw then
      align.apply_named(args[1], first, last)
      return
    end
    -- Not a saved name → open UI pre-filled with the argument
    align.open({ first_line = first, last_line = last, initial_text = args[1] })
    return
  end

  -- ── :Align  (no args) — open interactive UI ──────────────────────────────
  align.open({ first_line = first, last_line = last })
end, {
  range    = true,
  nargs    = "*",
  complete = function(arg_lead, cmd_line, cursor_pos)
    return require("align").complete(arg_lead, cmd_line, cursor_pos)
  end,
  desc = "Align selected (or whole-file) lines using the align binary",
})
