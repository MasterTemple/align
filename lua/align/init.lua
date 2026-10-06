--- align.nvim: align text into columns with the `align` binary.
---
---   :[range]Align                       open the interactive UI
---   :[range]Align <pattern>             align right away (like :!align, without the shell)
---   :[range]Align <name>                apply a saved pattern
---   :[range]Align telescope [mode]      pick from saved/recent patterns
---
--- No range means the whole buffer.
local bridge = require("align.bridge")

local M = {}

--- Must match the version of the Rust crates (checked by :checkhealth align).
M.version = "0.2.0"

---@class align.SavedPattern
---@field name string
---@field pattern string
---@field filetypes? string[] omit or {} for all; "" matches buffers without a filetype

---@class align.Config
M.config = {
  --- Path to the binary. nil = the plugin's own build, then $PATH, then ~/.cargo/bin.
  ---@type string|nil
  bin = nil,
  --- Preview debounce in milliseconds.
  debounce_ms = 50,
  --- History entries kept for the session.
  history_max = 100,
  --- Border for the floating windows.
  border = "rounded",
  --- Saved named patterns.
  ---@type align.SavedPattern[]
  patterns = {},
}

---@param opts? align.Config
function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})
end

-- ─── history ────────────────────────────────────────────────────────────────

local history = {} ---@type string[] oldest → newest

---@param pattern string
function M.history_push(pattern)
  if pattern == "" then return end
  for i = #history, 1, -1 do
    if history[i] == pattern then table.remove(history, i) end
  end
  table.insert(history, pattern)
  while #history > M.config.history_max do table.remove(history, 1) end
end

---@return string[] oldest → newest
function M.get_history() return history end

-- ─── saved patterns ─────────────────────────────────────────────────────────

local function matches_ft(entry, ft)
  local fts = entry.filetypes
  if not fts or #fts == 0 then return true end
  return vim.tbl_contains(fts, ft)
end

--- Saved patterns available for a filetype.
---@param ft string
---@return align.SavedPattern[]
function M.get_saved(ft)
  return vim.tbl_filter(function(p) return matches_ft(p, ft) end, M.config.patterns)
end

---@param name string
---@param ft string
---@return string|nil
local function find_saved(name, ft)
  for _, p in ipairs(M.get_saved(ft)) do
    if p.name == name then return p.pattern end
  end
end

-- ─── core ───────────────────────────────────────────────────────────────────

---@class align.Range
---@field buf integer
---@field first integer 0-based, inclusive
---@field last integer 0-based, exclusive

--- Resolve the binary or notify and return nil.
---@return string|nil
function M.bin()
  local bin, err = bridge.resolve(M.config.bin)
  if not bin then vim.notify(err, vim.log.levels.ERROR) end
  return bin
end

--- Run align on lines without touching any buffer.
---@param pattern string
---@param lines string[]
---@param opts { tabstop?: integer }
---@param callback fun(output: string[]|nil, err: align.Error|nil)
function M.run(pattern, lines, opts, callback)
  local bin = M.bin()
  if not bin then return end
  bridge.call(bin, pattern, lines, opts, callback)
end

---@param range align.Range
---@return string[]
function M.get_lines(range)
  return vim.api.nvim_buf_get_lines(range.buf, range.first, range.last, false)
end

--- Align a buffer range with `pattern` and write the result back
--- (one undo step; skipped when nothing changes).
---@param pattern string
---@param range align.Range
---@param on_done? fun(err: align.Error|nil)
function M.apply(pattern, range, on_done)
  on_done = on_done or function(err)
    if err then vim.notify("align: " .. err.message, vim.log.levels.ERROR) end
  end
  local lines = M.get_lines(range)
  local tick = vim.api.nvim_buf_get_changedtick(range.buf)
  M.history_push(pattern)
  M.run(pattern, lines, { tabstop = vim.bo[range.buf].tabstop }, function(output, err)
    if err then return on_done(err) end
    if not vim.api.nvim_buf_is_valid(range.buf) then return on_done(nil) end
    if vim.api.nvim_buf_get_changedtick(range.buf) ~= tick then
      return on_done({ message = "buffer changed while aligning; nothing applied" })
    end
    if not vim.deep_equal(output, lines) then
      vim.api.nvim_buf_set_lines(range.buf, range.first, range.last, false, output)
    end
    on_done(nil)
  end)
end

--- Open the interactive UI.
---@param range align.Range
---@param initial? string
function M.open(range, initial)
  require("align.ui").open(range, initial)
end

-- ─── :Align ─────────────────────────────────────────────────────────────────

--- Handler for the :Align user command.
---@param info table nvim_create_user_command callback argument
function M.command(info)
  local buf = vim.api.nvim_get_current_buf()
  local range = info.range == 0
      and { buf = buf, first = 0, last = vim.api.nvim_buf_line_count(buf) }
    or { buf = buf, first = info.line1 - 1, last = info.line2 }
  local args = vim.trim(info.args)

  if args == "" then
    return M.open(range)
  end
  if info.fargs[1] == "telescope" and #info.fargs <= 2 then
    return require("align.telescope").pick(info.fargs[2] or "all", range)
  end
  local saved = #info.fargs == 1 and find_saved(args, vim.bo[buf].filetype)
  M.apply(saved or args, range)
end

--- Completion for :Align (saved names and `telescope` modes).
function M.complete(arg_lead, cmd_line)
  local words = vim.split(vim.trim(cmd_line:gsub("^%S*Align", "")), "%s+", { trimempty = true })
  local new_word = cmd_line:sub(-1) == " "
  local index = #words + (new_word and 1 or 0)
  local candidates = {}
  if index <= 1 then
    candidates = { "telescope" }
    for _, p in ipairs(M.get_saved(vim.bo.filetype)) do table.insert(candidates, p.name) end
  elseif index == 2 and words[1] == "telescope" then
    candidates = { "saved", "recent", "all" }
  end
  return vim.tbl_filter(function(c) return vim.startswith(c, arg_lead) end, candidates)
end

return M
