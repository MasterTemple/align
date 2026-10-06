--- Floating input + live preview for :Align.
local align = require("align")

local M = {}

local ns = vim.api.nvim_create_namespace("align.ui")

local function input_text(buf)
  return vim.trim(table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), " "))
end

---@param sess table
---@param output string[]|nil
---@param err align.Error|nil
local function render(sess, output, err)
  local pbuf, ibuf = sess.preview_buf, sess.input_buf
  if not vim.api.nvim_buf_is_valid(pbuf) then return end
  vim.api.nvim_buf_clear_namespace(pbuf, ns, 0, -1)
  vim.api.nvim_buf_clear_namespace(ibuf, ns, 0, -1)
  vim.bo[pbuf].modifiable = true

  if err then
    vim.api.nvim_buf_set_lines(pbuf, 0, -1, false, vim.list_extend({ "✗ " .. err.message, "" }, sess.lines))
    vim.api.nvim_buf_set_extmark(pbuf, ns, 0, 0, { line_hl_group = "DiagnosticError" })
    local text = vim.api.nvim_buf_get_lines(ibuf, 0, 1, false)[1] or ""
    if err.col and err.col < vim.fn.strchars(text) then
      local start = vim.fn.byteidx(text, err.col)
      local stop = vim.fn.byteidx(text, err.col + 1)
      vim.api.nvim_buf_set_extmark(ibuf, ns, 0, start, { end_col = stop, hl_group = "DiagnosticUnderlineError" })
    end
  else
    output = output or sess.lines
    vim.api.nvim_buf_set_lines(pbuf, 0, -1, false, output)
    for i, line in ipairs(output) do
      if line ~= sess.lines[i] then
        vim.api.nvim_buf_set_extmark(pbuf, ns, i - 1, 0, { line_hl_group = "DiffChange" })
      end
    end
  end
  vim.bo[pbuf].modifiable = false
end

local function refresh(sess)
  sess.generation = sess.generation + 1
  local generation = sess.generation
  local pattern = input_text(sess.input_buf)
  if pattern == "" then return render(sess, nil, nil) end
  align.run(pattern, sess.lines, { tabstop = sess.tabstop }, function(output, err)
    -- Ignore results that arrive after a newer request was made.
    if generation == sess.generation and not sess.closed then render(sess, output, err) end
  end)
end

local function schedule_refresh(sess)
  sess.timer:stop()
  sess.timer:start(align.config.debounce_ms, 0, vim.schedule_wrap(function()
    if not sess.closed then refresh(sess) end
  end))
end

local function close(sess)
  if sess.closed then return end
  sess.closed = true
  sess.timer:stop()
  sess.timer:close()
  vim.cmd.stopinsert()
  for _, win in ipairs({ sess.input_win, sess.preview_win }) do
    pcall(vim.api.nvim_win_close, win, true)
  end
  if vim.api.nvim_win_is_valid(sess.origin_win) then vim.api.nvim_set_current_win(sess.origin_win) end
end

local function set_input(sess, text)
  vim.api.nvim_buf_set_lines(sess.input_buf, 0, -1, false, { text })
  vim.api.nvim_win_set_cursor(sess.input_win, { 1, #text })
end

local function keymaps(sess)
  local function map(modes, lhs, fn)
    vim.keymap.set(modes, lhs, fn, { buffer = sess.input_buf, nowait = true, silent = true })
  end

  map({ "i", "n" }, "<CR>", function()
    local pattern = input_text(sess.input_buf)
    close(sess)
    if pattern ~= "" then align.apply(pattern, sess.range) end
  end)
  map({ "i", "n" }, "<C-c>", function() close(sess) end)
  map("n", "<Esc>", function() close(sess) end)
  map("n", "q", function() close(sess) end)
  map("i", "<Esc>", function() close(sess) end)

  local history = align.get_history()
  local function step(delta)
    if #history == 0 then return end
    local pos = (sess.history_pos or (#history + 1)) + delta
    if pos > #history then
      sess.history_pos = nil
      set_input(sess, sess.draft or "")
    else
      if not sess.history_pos then sess.draft = input_text(sess.input_buf) end
      sess.history_pos = math.max(1, pos)
      set_input(sess, history[sess.history_pos])
    end
  end
  for _, lhs in ipairs({ "<C-p>", "<Up>" }) do map({ "i", "n" }, lhs, function() step(-1) end) end
  for _, lhs in ipairs({ "<C-n>", "<Down>" }) do map({ "i", "n" }, lhs, function() step(1) end) end
end

---@param range align.Range
---@param initial? string
function M.open(range, initial)
  if not align.bin() then return end
  local lines = align.get_lines(range)
  if #lines == 0 then
    return vim.notify("align: no lines in range", vim.log.levels.WARN)
  end

  local width = math.max(40, math.min(vim.o.columns - 6, 120))
  local preview_height = math.max(3, math.min(#lines + 2, math.floor(vim.o.lines * 0.5)))
  local row = math.max(0, math.floor((vim.o.lines - preview_height - 5) / 2))
  local col = math.floor((vim.o.columns - width) / 2)

  local sess = {
    range = range,
    lines = lines,
    tabstop = vim.bo[range.buf].tabstop,
    origin_win = vim.api.nvim_get_current_win(),
    generation = 0,
    timer = assert(vim.uv.new_timer()),
    closed = false,
  }

  sess.input_buf = vim.api.nvim_create_buf(false, true)
  vim.bo[sess.input_buf].bufhidden = "wipe"
  sess.input_win = vim.api.nvim_open_win(sess.input_buf, true, {
    relative = "editor", row = row, col = col, width = width, height = 1,
    border = align.config.border, style = "minimal",
    title = " Align  <CR> apply · <Esc> cancel · <C-p>/<C-n> history ", title_pos = "center",
  })

  sess.preview_buf = vim.api.nvim_create_buf(false, true)
  vim.bo[sess.preview_buf].bufhidden = "wipe"
  vim.bo[sess.preview_buf].tabstop = sess.tabstop
  sess.preview_win = vim.api.nvim_open_win(sess.preview_buf, false, {
    relative = "editor", row = row + 3, col = col, width = width, height = preview_height,
    border = align.config.border, style = "minimal", title = " Preview ", title_pos = "center",
    focusable = false,
  })
  vim.wo[sess.preview_win].wrap = false
  render(sess, nil, nil)

  keymaps(sess)
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI" }, {
    buffer = sess.input_buf,
    callback = function() schedule_refresh(sess) end,
  })
  vim.api.nvim_create_autocmd("BufLeave", {
    buffer = sess.input_buf, once = true,
    callback = function() vim.schedule(function() close(sess) end) end,
  })

  if initial and initial ~= "" then
    set_input(sess, initial)
    refresh(sess)
  end
  vim.cmd.startinsert({ bang = true })
end

return M
