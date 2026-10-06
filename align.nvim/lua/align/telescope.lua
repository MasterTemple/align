--- align.nvim — lua/align/telescope.lua
---
--- Three Telescope pickers, opened via :Align telescope [saved|recent|all]
---
---   saved   — named patterns from config.patterns, filtered by current filetype
---   recent  — session history, most-recent first
---   all     — saved entries first, then recent, deduped by pattern string
---
--- Keymaps inside every picker:
---   <CR>    apply the selected pattern immediately (no floating UI)
---   <C-e>   open the floating UI pre-filled (so you can tweak before applying)
---
--- The preview window shows the aligned output of the current range, updated
--- as you move through the list.

local M = {}

-- ─────────────────────────────────────────────────────────────────────────────
-- Helpers
-- ─────────────────────────────────────────────────────────────────────────────

local function need_telescope()
  local ok, _ = pcall(require, "telescope")
  if not ok then
    vim.notify(
      "align.nvim: telescope.nvim is required.\n"
      .. "Install it with your plugin manager (e.g. 'nvim-telescope/telescope.nvim').",
      vim.log.levels.ERROR)
    return false
  end
  return true
end

--- Remove duplicate entries (by pattern string), preserving order.
local function dedup(entries)
  local seen, out = {}, {}
  for _, e in ipairs(entries) do
    if not seen[e.pattern] then
      seen[e.pattern] = true
      table.insert(out, e)
    end
  end
  return out
end

--- Render aligned output (or an error) into a Telescope preview buffer.
local function render_into(buf, lines, output, err)
  if not vim.api.nvim_buf_is_valid(buf) then return end
  vim.api.nvim_buf_set_option(buf, "modifiable", true)
  vim.api.nvim_buf_clear_namespace(buf, -1, 0, -1)
  if err then
    vim.api.nvim_buf_set_lines(buf, 0, -1, false,
      vim.list_extend({ "  ✗ " .. err }, vim.deepcopy(lines)))
    vim.api.nvim_buf_add_highlight(buf, -1, "DiagnosticError", 0, 0, -1)
  else
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, output)
    for i, line in ipairs(output) do
      if line ~= (lines[i] or "") then
        vim.api.nvim_buf_add_highlight(buf, -1, "DiffChange", i - 1, 0, -1)
      end
    end
  end
  vim.api.nvim_buf_set_option(buf, "modifiable", false)
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Previewer factory
-- ─────────────────────────────────────────────────────────────────────────────

--- Build a buffer previewer that runs align on `ctx.lines` for each entry.
---@param bin   string    resolved binary path
---@param ctx   { lines: string[] }
local function make_previewer(bin, ctx)
  local previewers = require("telescope.previewers")
  local align      = require("align")

  return previewers.new_buffer_previewer({
    title = "Aligned preview",

    -- Show original content immediately while the async job runs.
    get_buffer_by_name = function(_, entry)
      return entry.value.pattern   -- unique key per entry
    end,

    define_preview = function(self, entry)
      local pbuf    = self.state.bufnr
      local pattern = entry.value.pattern

      -- Show original while waiting
      vim.api.nvim_buf_set_option(pbuf, "modifiable", true)
      vim.api.nvim_buf_set_lines(pbuf, 0, -1, false, ctx.lines)
      vim.api.nvim_buf_set_option(pbuf, "modifiable", false)

      if not pattern or pattern == "" then return end

      local args = align._tokenize(pattern)
      align._call_align(bin, args, ctx.lines, function(output, err)
        vim.schedule(function()
          render_into(pbuf, ctx.lines, output, err)
        end)
      end)
    end,
  })
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Public: open a picker
-- ─────────────────────────────────────────────────────────────────────────────

--- Open a Telescope picker for align patterns.
---@param mode  "saved"|"recent"|"all"
---@param opts  { first_line: integer, last_line: integer }
function M.pick(mode, opts)
  if not need_telescope() then return end

  local pickers   = require("telescope.pickers")
  local finders   = require("telescope.finders")
  local conf      = require("telescope.config").values
  local actions   = require("telescope.actions")
  local act_state = require("telescope.actions.state")
  local align     = require("align")

  -- Resolve binary early so we can bail cleanly before opening the picker.
  local bin, err = align._resolve_bin()
  if not bin then vim.notify(err, vim.log.levels.ERROR); return end

  local original_buf = vim.api.nvim_get_current_buf()
  local ft           = vim.api.nvim_buf_get_option(original_buf, "filetype") or ""
  local fl           = opts.first_line - 1   -- 0-based
  local ll           = opts.last_line         -- exclusive
  local lines        = vim.api.nvim_buf_get_lines(original_buf, fl, ll, false)

  if #lines == 0 then
    vim.notify("align.nvim: no lines in range", vim.log.levels.WARN); return
  end

  -- ── Build entry list ────────────────────────────────────────────────────

  local entries = {}

  local function push_saved()
    for _, p in ipairs(align.get_saved(ft)) do
      local label = p.name
      if p.filetypes and #p.filetypes > 0 then
        label = label .. "  [" .. table.concat(p.filetypes, ", ") .. "]"
      end
      table.insert(entries, {
        label   = label,
        pattern = p.pattern,
        kind    = "saved",
        name    = p.name,
      })
    end
  end

  local function push_recent()
    local hist = align.get_history()
    for i = #hist, 1, -1 do   -- most-recent first
      table.insert(entries, {
        label   = hist[i],
        pattern = hist[i],
        kind    = "recent",
      })
    end
  end

  if     mode == "saved"  then push_saved()
  elseif mode == "recent" then push_recent()
  elseif mode == "all"    then push_saved(); push_recent()
  else
    vim.notify("align telescope: unknown mode '" .. tostring(mode) .. "'",
      vim.log.levels.ERROR); return
  end

  entries = dedup(entries)

  if #entries == 0 then
    local msg = ({
      saved  = ("no saved patterns for filetype '%s'"):format(ft),
      recent = "no recent history yet",
      all    = "no saved patterns or history yet",
    })[mode] or "no entries"
    vim.notify("align.nvim: " .. msg, vim.log.levels.INFO); return
  end

  -- ── Actions ─────────────────────────────────────────────────────────────

  --- Apply pattern from an entry table, closing the picker first.
  local function do_apply(entry_val, prompt_buf)
    actions.close(prompt_buf)
    vim.schedule(function()
      align._apply_raw(bin, entry_val.pattern, lines, original_buf, fl, ll,
        function(e)
          if e then vim.notify("align: " .. e, vim.log.levels.ERROR) end
        end)
    end)
  end

  --- Open the floating UI pre-filled with the selected pattern.
  local function do_edit(entry_val, prompt_buf)
    actions.close(prompt_buf)
    vim.schedule(function()
      align.open({
        first_line   = opts.first_line,
        last_line    = opts.last_line,
        initial_text = entry_val.pattern,
      })
    end)
  end

  -- ── Picker ──────────────────────────────────────────────────────────────

  pickers.new({}, {
    prompt_title = ("Align [%s]  <CR> apply · <C-e> edit"):format(mode),

    finder = finders.new_table({
      results = entries,
      entry_maker = function(e)
        -- Prefix kind indicator so saved/recent are visually distinct in "all"
        local kind_icon = e.kind == "saved" and "  " or "  "
        return {
          value   = e,
          display = kind_icon .. e.label,
          ordinal = e.label .. " " .. e.pattern,
        }
      end,
    }),

    sorter    = conf.generic_sorter({}),
    previewer = make_previewer(bin, { lines = lines }),

    attach_mappings = function(prompt_buf, map_fn)
      -- Replace default <CR>
      actions.select_default:replace(function()
        local sel = act_state.get_selected_entry()
        if sel then do_apply(sel.value, prompt_buf) end
      end)
      -- <C-e>: open floating UI for editing
      for _, mode_str in ipairs({ "i", "n" }) do
        map_fn(mode_str, "<C-e>", function()
          local sel = act_state.get_selected_entry()
          if sel then do_edit(sel.value, prompt_buf) end
        end)
      end
      return true  -- keep default mappings for everything else
    end,
  }):find()
end

return M
