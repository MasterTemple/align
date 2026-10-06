--- align.nvim — lua/align/init.lua
---
--- Command dispatch:
---   :[range]Align                          open interactive UI
---   :[range]Align <name>                   apply a saved named pattern directly
---   :[range]Align telescope [saved|recent|all]   open Telescope picker
---
--- No range → whole file is used (equivalent to %).
--- Filetype filtering applies to named patterns and telescope "saved" mode.

local M = {}

-- ─────────────────────────────────────────────────────────────────────────────
-- Default config
-- ─────────────────────────────────────────────────────────────────────────────

M.config = {
  --- Path to the align binary. Falls back to ~/.cargo/bin/align automatically.
  bin = "align",

  --- Preview debounce in milliseconds.
  debounce_ms = 80,

  --- Maximum history entries kept for the session.
  history_max = 100,

  --- Highlight group for error text in the preview window.
  hl_error = "DiagnosticError",

  --- Border style for floating windows (any nvim_open_win value).
  border = "rounded",

  --- Saved named patterns.
  ---
  --- Format:
  ---   { name = "my-pattern", pattern = "= -p 1" }
  ---   { name = "rust-arms",  pattern = "if '=>'", filetypes = { "rust" } }
  ---   { name = "scratch",    pattern = "=",       filetypes = { "" } }
  ---
  --- filetypes is optional; omit or leave empty to match all file types.
  --- Use "" in the list to match buffers with no filetype (scratch buffers).
  patterns = {},
}

-- ─────────────────────────────────────────────────────────────────────────────
-- Session history  (module-level; survives across command invocations)
-- ─────────────────────────────────────────────────────────────────────────────

local history     = {}   -- list of raw pattern strings, oldest → newest
local history_pos = nil  -- nil = not browsing; integer = current index

local function history_push(raw)
  if not raw or raw == "" then return end
  for i = #history, 1, -1 do
    if history[i] == raw then table.remove(history, i) end
  end
  table.insert(history, raw)
  if #history > M.config.history_max then table.remove(history, 1) end
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Binary resolution
-- ─────────────────────────────────────────────────────────────────────────────

--- Returns resolved binary path, or (nil, error_string).
local function resolve_bin()
  local b = M.config.bin
  if vim.fn.executable(b) == 1 then return b end
  local cargo = vim.fn.expand("~/.cargo/bin/align")
  if vim.fn.executable(cargo) == 1 then return cargo end
  return nil, string.format(
    "align.nvim: binary '%s' not found.\n"
    .. "Install: cargo install --path <workspace>/align\n"
    .. "Or set:  require('align').setup({ bin = '/full/path/to/align' })", b)
end

-- ─────────────────────────────────────────────────────────────────────────────
-- JSON bridge  (async, callback-based)
-- ─────────────────────────────────────────────────────────────────────────────

--- Run `align --json`, delivering aligned output (or error) to callback.
---@param bin      string
---@param args     string[]
---@param lines    string[]
---@param callback fun(output: string[]|nil, err: string|nil)
local function call_align(bin, args, lines, callback)
  local req       = vim.fn.json_encode({ args = args, lines = lines })
  local out_bufs  = {}
  local err_bufs  = {}

  local jid = vim.fn.jobstart({ bin, "--json" }, {
    stdin     = "pipe",
    on_stdout = function(_, data)
      for _, c in ipairs(data) do table.insert(out_bufs, c) end
    end,
    on_stderr = function(_, data)
      for _, c in ipairs(data) do
        if c ~= "" then table.insert(err_bufs, c) end
      end
    end,
    on_exit = function(_, code)
      local raw = table.concat(out_bufs, ""):gsub("^%s+", ""):gsub("%s+$", "")
      local eraw = table.concat(err_bufs, "")
      if raw == "" then
        local msg = eraw ~= "" and eraw or ("align exited with code " .. code)
        if msg:find("no patterns given") or msg:find("parse error") then
          msg = "Binary lacks --json support — rebuild: cargo install --path <workspace>/align"
        end
        callback(nil, msg); return
      end
      local ok, resp = pcall(vim.fn.json_decode, raw)
      if not ok then
        callback(nil, "JSON decode error: " .. tostring(resp)); return
      end
      if resp.error ~= nil and resp.error ~= vim.NIL then
        callback(nil, tostring(resp.error))
      else
        callback(resp.output, nil)
      end
    end,
  })

  if not jid or jid <= 0 then
    callback(nil, jid == 0 and ("binary not found: " .. bin) or "jobstart failed")
    return
  end
  vim.fn.chansend(jid, req)
  vim.fn.chanclose(jid, "stdin")
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Tokenizer
-- ─────────────────────────────────────────────────────────────────────────────

--- Split a raw pattern string into args the binary understands.
--- Handles "…", '…', `…` (quoted), /…/flags (regex), and bare words.
local function tokenize(raw)
  local tokens = {}
  local i, len = 1, #raw
  while i <= len do
    local c = raw:sub(i, i)
    if c:match("%s") then
      i = i + 1
    elseif c == '"' or c == "'" or c == "`" then
      local delim, j, tok = c, i + 1, ""
      while j <= len do
        local d = raw:sub(j, j)
        if d == "\\" and j + 1 <= len and raw:sub(j+1,j+1) == delim then
          tok = tok .. delim; j = j + 2
        elseif d == delim then
          j = j + 1; break
        else
          tok = tok .. d; j = j + 1
        end
      end
      table.insert(tokens, delim .. tok .. delim); i = j
    elseif c == "/" then
      local j, tok = i + 1, "/"
      while j <= len do
        local d = raw:sub(j, j)
        tok = tok .. d; j = j + 1
        if d == "/" then
          while j <= len and raw:sub(j,j):match("[%a]") do
            tok = tok .. raw:sub(j,j); j = j + 1
          end
          break
        end
        if d == "\\" and j <= len then tok = tok .. raw:sub(j,j); j = j + 1 end
      end
      table.insert(tokens, tok); i = j
    else
      local j = i
      while j <= len and not raw:sub(j,j):match("%s") do j = j + 1 end
      table.insert(tokens, raw:sub(i, j-1)); i = j
    end
  end
  return tokens
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Filetype / pattern helpers
-- ─────────────────────────────────────────────────────────────────────────────

local function buf_ft(bufnr)
  return vim.api.nvim_buf_get_option(bufnr or 0, "filetype") or ""
end

local function pattern_matches_ft(entry, ft)
  local fts = entry.filetypes
  if not fts or #fts == 0 then return true end
  for _, f in ipairs(fts) do if f == ft then return true end end
  return false
end

local function patterns_for_ft(ft)
  local out = {}
  for _, p in ipairs(M.config.patterns) do
    if pattern_matches_ft(p, ft) then table.insert(out, p) end
  end
  return out
end

--- Find a named pattern by name. Tries ft-filtered first, then any ft.
local function find_named(name, ft)
  for _, p in ipairs(M.config.patterns) do
    if p.name == name and pattern_matches_ft(p, ft) then return p.pattern end
  end
  for _, p in ipairs(M.config.patterns) do
    if p.name == name then return p.pattern end
  end
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Core apply  (shared by UI, direct-name, and telescope)
-- ─────────────────────────────────────────────────────────────────────────────

--- Apply raw pattern string to lines, write result back to bufnr[first..last).
--- Pushes to history. Calls done(err_or_nil) when finished.
local function apply_raw(bin, raw, lines, bufnr, first_line, last_line, done)
  history_push(raw)
  call_align(bin, tokenize(raw), lines, function(output, err)
    if err then done(err); return end
    if output and vim.api.nvim_buf_is_valid(bufnr) then
      vim.api.nvim_buf_set_lines(bufnr, first_line, last_line, false, output)
    end
    done(nil)
  end)
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Floating UI internals
-- ─────────────────────────────────────────────────────────────────────────────

local function render_preview(sess, output, err, original)
  if not vim.api.nvim_buf_is_valid(sess.preview_buf) then return end
  vim.api.nvim_buf_set_option(sess.preview_buf, "modifiable", true)
  vim.api.nvim_buf_clear_namespace(sess.preview_buf, -1, 0, -1)
  if err then
    vim.api.nvim_buf_set_lines(sess.preview_buf, 0, -1, false, { "  ✗ " .. err })
    vim.api.nvim_buf_add_highlight(sess.preview_buf, -1, M.config.hl_error, 0, 0, -1)
  elseif output then
    vim.api.nvim_buf_set_lines(sess.preview_buf, 0, -1, false, output)
    for i, line in ipairs(output) do
      if line ~= (original[i] or "") then
        vim.api.nvim_buf_add_highlight(sess.preview_buf, -1, "DiffChange", i-1, 0, -1)
      end
    end
  else
    vim.api.nvim_buf_set_lines(sess.preview_buf, 0, -1, false, original)
  end
  vim.api.nvim_buf_set_option(sess.preview_buf, "modifiable", false)
end

local function schedule_update(sess)
  if sess.debounce_timer then
    sess.debounce_timer:stop()
  else
    sess.debounce_timer = vim.loop.new_timer()
  end
  sess.debounce_timer:start(M.config.debounce_ms, 0, vim.schedule_wrap(function()
    if not vim.api.nvim_buf_is_valid(sess.input_buf) then return end
    local raw = table.concat(
      vim.api.nvim_buf_get_lines(sess.input_buf, 0, -1, false), " ")
      :gsub("^%s+", ""):gsub("%s+$", "")
    if raw == "" then
      render_preview(sess, sess.lines, nil, sess.lines); return
    end
    call_align(sess.bin, tokenize(raw), sess.lines, function(out, err)
      render_preview(sess, out, err, sess.lines)
    end)
  end))
end

local function session_close(sess)
  if sess.debounce_timer then
    sess.debounce_timer:stop(); sess.debounce_timer:close()
    sess.debounce_timer = nil
  end
  pcall(vim.api.nvim_win_close,  sess.input_win,   true)
  pcall(vim.api.nvim_win_close,  sess.preview_win, true)
  pcall(vim.api.nvim_buf_delete, sess.input_buf,   { force = true })
  pcall(vim.api.nvim_buf_delete, sess.preview_buf, { force = true })
  history_pos = nil
end

local function setup_keymaps(sess, original_buf, fl, ll)
  local buf = sess.input_buf
  local function map(lhs, fn)
    local o = { noremap = true, silent = true, callback = fn }
    vim.api.nvim_buf_set_keymap(buf, "i", lhs, "", o)
    vim.api.nvim_buf_set_keymap(buf, "n", lhs, "", o)
  end

  map("<CR>", function()
    local raw = table.concat(
      vim.api.nvim_buf_get_lines(buf, 0, -1, false), " ")
      :gsub("^%s+", ""):gsub("%s+$", "")
    if raw == "" then session_close(sess); return end
    apply_raw(sess.bin, raw, sess.lines, original_buf, fl, ll, function(err)
      session_close(sess)
      if err then vim.notify("align: " .. err, vim.log.levels.ERROR) end
    end)
  end)

  map("<Esc>", function() session_close(sess) end)
  map("<C-c>", function() session_close(sess) end)

  -- <C-p>: step backwards through history
  map("<C-p>", function()
    if #history == 0 then return end
    history_pos = history_pos and math.max(1, history_pos - 1) or #history
    local entry = history[history_pos] or ""
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, { entry })
    vim.api.nvim_win_set_cursor(sess.input_win, { 1, #entry })
    schedule_update(sess)
  end)

  -- <C-n>: step forwards through history (past end = blank)
  map("<C-n>", function()
    if not history_pos then return end
    if history_pos < #history then
      history_pos = history_pos + 1
      local entry = history[history_pos] or ""
      vim.api.nvim_buf_set_lines(buf, 0, -1, false, { entry })
      vim.api.nvim_win_set_cursor(sess.input_win, { 1, #entry })
    else
      history_pos = nil
      vim.api.nvim_buf_set_lines(buf, 0, -1, false, { "" })
      vim.api.nvim_win_set_cursor(sess.input_win, { 1, 0 })
    end
    schedule_update(sess)
  end)
end

local function open_ui(bin, lines, original_buf, fl, ll, initial_text)
  local win_w     = math.min(math.max(60, vim.o.columns - 10), vim.o.columns - 4)
  local prev_h    = math.min(math.max(#lines, 3), math.floor(vim.o.lines * 0.45))
  local inp_h     = 1
  local start_row = math.floor((vim.o.lines  - (inp_h + 2 + prev_h + 2)) / 2)
  local col       = math.floor((vim.o.columns - win_w) / 2)

  local input_buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_option(input_buf, "buftype",  "nofile")
  vim.api.nvim_buf_set_option(input_buf, "filetype", "")
  vim.api.nvim_buf_set_lines(input_buf, 0, -1, false, { initial_text or "" })

  local input_win = vim.api.nvim_open_win(input_buf, true, {
    relative  = "editor",
    row       = start_row, col = col,
    width     = win_w, height = inp_h,
    border    = M.config.border,
    title     = " Align pattern  <CR> apply · <Esc> cancel · <C-p>/<C-n> history ",
    title_pos = "center",
    style     = "minimal",
  })
  vim.api.nvim_win_set_option(input_win, "winhl", "Normal:Normal,FloatBorder:FloatBorder")

  local preview_buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_option(preview_buf, "buftype", "nofile")
  vim.api.nvim_buf_set_lines(preview_buf, 0, -1, false, lines)       -- content first
  vim.api.nvim_buf_set_option(preview_buf, "modifiable", false)       -- then lock

  local preview_win = vim.api.nvim_open_win(preview_buf, false, {
    relative  = "editor",
    row       = start_row + inp_h + 2, col = col,
    width     = win_w, height = prev_h,
    border    = M.config.border,
    title     = " Preview ", title_pos = "center",
    style     = "minimal",
  })
  vim.api.nvim_win_set_option(preview_win, "winhl", "Normal:Normal,FloatBorder:FloatBorder")
  vim.api.nvim_win_set_option(preview_win, "wrap", false)

  vim.cmd("startinsert!")

  local sess = {
    bin = bin, input_buf = input_buf, input_win = input_win,
    preview_buf = preview_buf, preview_win = preview_win,
    lines = lines, debounce_timer = nil,
  }
  if initial_text and initial_text ~= "" then schedule_update(sess) end
  return sess
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Public API
-- ─────────────────────────────────────────────────────────────────────────────

--- Open the interactive floating UI.
---@param opts { first_line: integer, last_line: integer, initial_text?: string }
function M.open(opts)
  local bin, err = resolve_bin()
  if not bin then vim.notify(err, vim.log.levels.ERROR); return end

  local bufnr = vim.api.nvim_get_current_buf()
  local fl    = opts.first_line - 1   -- convert to 0-based
  local ll    = opts.last_line        -- exclusive end

  local lines = vim.api.nvim_buf_get_lines(bufnr, fl, ll, false)
  if #lines == 0 then
    vim.notify("align.nvim: no lines in range", vim.log.levels.WARN); return
  end

  local sess = open_ui(bin, lines, bufnr, fl, ll, opts.initial_text)
  setup_keymaps(sess, bufnr, fl, ll)

  vim.api.nvim_create_autocmd({ "TextChangedI", "TextChanged" }, {
    buffer   = sess.input_buf,
    callback = function() schedule_update(sess) end,
  })
  vim.api.nvim_create_autocmd("WinClosed", {
    pattern  = tostring(sess.input_win), once = true,
    callback = function() session_close(sess) end,
  })
end

--- Apply a named pattern directly without opening the UI.
---@param name       string
---@param first_line integer   1-based
---@param last_line  integer   1-based inclusive
function M.apply_named(name, first_line, last_line)
  local bin, err = resolve_bin()
  if not bin then vim.notify(err, vim.log.levels.ERROR); return end

  local bufnr = vim.api.nvim_get_current_buf()
  local raw   = find_named(name, buf_ft(bufnr))
  if not raw then
    vim.notify(
      ("align.nvim: no pattern named '%s' for filetype '%s'"):format(name, buf_ft(bufnr)),
      vim.log.levels.ERROR); return
  end

  local fl    = first_line - 1
  local ll    = last_line
  local lines = vim.api.nvim_buf_get_lines(bufnr, fl, ll, false)
  apply_raw(bin, raw, lines, bufnr, fl, ll, function(e)
    if e then vim.notify("align: " .. e, vim.log.levels.ERROR) end
  end)
end

--- Command-line completion for :Align.
---   First arg → saved pattern names (ft-filtered) + "telescope"
---   After "telescope" → "saved" | "recent" | "all"
function M.complete(arg_lead, cmd_line, _)
  local ft    = buf_ft(0)
  local parts = {}
  for p in cmd_line:gmatch("%S+") do table.insert(parts, p) end
  -- parts[1] = "Align"; trailing space means a new arg has started
  local n = #parts - 1 + (cmd_line:sub(-1) == " " and 1 or 0)

  local function filter(list)
    local out, lead = {}, vim.pesc(arg_lead)
    for _, c in ipairs(list) do
      if c:find("^" .. lead) then table.insert(out, c) end
    end
    return out
  end

  if n <= 1 then
    local cands = { "telescope" }
    for _, p in ipairs(patterns_for_ft(ft)) do table.insert(cands, p.name) end
    return filter(cands)
  elseif n == 2 and parts[2] == "telescope" then
    return filter({ "saved", "recent", "all" })
  end
  return {}
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Exports used by telescope.lua  (prefixed _ to signal internal use)
-- ─────────────────────────────────────────────────────────────────────────────

M._resolve_bin  = resolve_bin
M._call_align   = call_align
M._tokenize     = tokenize
M._apply_raw    = apply_raw
M._patterns_for_ft = patterns_for_ft

--- History list (most-recent last).  telescope reads this directly.
function M.get_history() return history end

--- Saved patterns visible for the given filetype.
function M.get_saved(ft) return patterns_for_ft(ft or "") end

-- ─────────────────────────────────────────────────────────────────────────────
-- Setup
-- ─────────────────────────────────────────────────────────────────────────────

---@param opts table|nil
function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})
end

return M
