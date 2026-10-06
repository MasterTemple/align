-- Headless plugin tests:  nvim --headless --clean -l tests/plugin.lua
-- Requires target/release/align (cargo build --release).
local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
vim.opt.rtp:prepend(root)
vim.env.ALIGN_CONFIG = "" -- built-in defaults, ignore the user's config
vim.cmd.runtime("plugin/align.lua")

local align = require("align")
local failures, count = {}, 0

local function test(name, fn)
  count = count + 1
  vim.cmd("silent! %bwipeout!")
  local ok, err = pcall(fn)
  if not ok then table.insert(failures, name .. ": " .. tostring(err)) end
end

local function eq(expected, actual, what)
  if not vim.deep_equal(expected, actual) then
    error(("%s\n  expected: %s\n  actual:   %s"):format(what or "mismatch", vim.inspect(expected), vim.inspect(actual)), 2)
  end
end

local function buffer(lines)
  vim.cmd.enew()
  vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
  return vim.api.nvim_get_current_buf()
end

local function lines() return vim.api.nvim_buf_get_lines(0, 0, -1, false) end

--- Wait until the buffer content changes from `before`.
local function wait_change(before)
  vim.wait(3000, function() return not vim.deep_equal(lines(), before) end, 10)
end

local notified = {}
vim.notify = function(msg, level) table.insert(notified, { msg = msg, level = level }) end

test("binary resolves to the plugin's own build", function()
  local bin = require("align.bridge").resolve(nil)
  eq(root .. "/target/release/align", bin)
end)

test(":Align <pattern> aligns the whole buffer", function()
  buffer({ "a = 1", "foobar = 2" })
  local before = lines()
  vim.cmd("Align =")
  wait_change(before)
  eq({ "a      = 1", "foobar = 2" }, lines())
end)

test(":Align with several args (P1) and quotes", function()
  buffer({ [["a" if x => y]], [["bb" => z]] })
  local before = lines()
  vim.cmd([[Align if '=>']])
  wait_change(before)
  eq({ [["a" if x => y]], [["bb"     => z]] }, lines())
end)

test(":'<,'>Align only touches the range", function()
  buffer({ "x = 1", "a = 1", "foobar = 2", "yyyyyyyyy = 3" })
  local before = lines()
  vim.cmd("2,3Align =")
  wait_change(before)
  eq({ "x = 1", "a      = 1", "foobar = 2", "yyyyyyyyy = 3" }, lines())
end)

test("saved patterns by name, filtered by filetype", function()
  align.setup({ patterns = {
    { name = "eq", pattern = "= -p 0" },
    { name = "luaonly", pattern = ":", filetypes = { "alignft" } },
  } })
  buffer({ "a = 1", "bb = 2" })
  local before = lines()
  vim.cmd("Align eq")
  wait_change(before)
  eq({ "a =1", "bb=2" }, lines())
  eq({ "telescope", "eq" }, align.complete("", "Align "), "completion hides alignft-only pattern")
  vim.bo.filetype = "alignft"
  eq({ "telescope", "eq", "luaonly" }, align.complete("", "Align "))
  eq({ "recent" }, align.complete("r", "Align telescope r"))
  align.setup({ patterns = {} })
end)

test("history records applied patterns", function()
  eq(true, vim.tbl_contains(align.get_history(), "="))
end)

test("errors are reported, buffer untouched", function()
  notified = {}
  buffer({ "a = 1", "bb = 2" })
  vim.cmd("Align = -x")
  vim.wait(3000, function() return #notified > 0 end, 10)
  eq({ "a = 1", "bb = 2" }, lines())
  assert(notified[1] and notified[1].msg:find("unknown flag %-x"), vim.inspect(notified))
end)

test("tabstop comes from the buffer", function()
  buffer({ "\tx = 1", "yyyyyy = 2" })
  vim.bo.tabstop = 4
  local before = lines()
  vim.cmd("Align =")
  wait_change(before)
  eq({ "\tx  = 1", "yyyyyy = 2" }, lines())
end)

test("UI: preview updates, <CR> applies", function()
  local buf = buffer({ "a = 1", "foobar = 2" })
  align.open({ buf = buf, first = 0, last = 2 }, "=")
  local preview
  vim.wait(3000, function()
    for _, win in ipairs(vim.api.nvim_list_wins()) do
      local b = vim.api.nvim_win_get_buf(win)
      if vim.api.nvim_win_get_config(win).title and b ~= vim.api.nvim_get_current_buf() then preview = b end
    end
    return preview and vim.api.nvim_buf_get_lines(preview, 0, 1, false)[1] == "a      = 1"
  end, 10)
  eq({ "a      = 1", "foobar = 2" }, vim.api.nvim_buf_get_lines(preview, 0, -1, false), "preview")
  eq({ "a = 1", "foobar = 2" }, vim.api.nvim_buf_get_lines(buf, 0, -1, false), "not applied yet")
  vim.api.nvim_feedkeys(vim.keycode("<CR>"), "x", false)
  vim.wait(3000, function() return vim.api.nvim_buf_get_lines(buf, 0, 1, false)[1] == "a      = 1" end, 10)
  eq({ "a      = 1", "foobar = 2" }, vim.api.nvim_buf_get_lines(buf, 0, -1, false))
  eq(1, #vim.api.nvim_list_wins(), "floats closed")
end)

test("UI: errors show in the preview", function()
  local buf = buffer({ "a = 1" })
  align.open({ buf = buf, first = 0, last = 1 }, "/[/")
  local found
  vim.wait(3000, function()
    for _, win in ipairs(vim.api.nvim_list_wins()) do
      local first = vim.api.nvim_buf_get_lines(vim.api.nvim_win_get_buf(win), 0, 1, false)[1] or ""
      if first:find("invalid regex") then found = true end
    end
    return found
  end, 10)
  assert(found, "no error in preview")
  vim.api.nvim_feedkeys(vim.keycode("<Esc>"), "x", false)
  eq(1, #vim.api.nvim_list_wins(), "floats closed")
end)

test(":checkhealth align passes", function()
  vim.cmd("checkhealth align")
  local text = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
  assert(not text:find("ERROR"), text)
  assert(text:find("JSON round%-trip works"), text)
end)

if #failures > 0 then
  io.stderr:write(("%d/%d plugin tests failed:\n\n%s\n"):format(#failures, count, table.concat(failures, "\n\n")))
  os.exit(1)
end
print(("%d plugin tests passed"):format(count))
