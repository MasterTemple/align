--- Telescope picker over saved and recent patterns, with an aligned preview.
---   :Align telescope saved|recent|all
--- <CR> applies the pattern; <C-e> opens the UI prefilled with it.
local align = require("align")

local M = {}

local ns = vim.api.nvim_create_namespace("align.telescope")

---@param mode "saved"|"recent"|"all"
---@param range align.Range
---@return table[]
local function entries(mode, range)
  local out, seen = {}, {}
  local function add(e)
    if not seen[e.pattern] then
      seen[e.pattern] = true
      table.insert(out, e)
    end
  end
  if mode == "saved" or mode == "all" then
    for _, p in ipairs(align.get_saved(vim.bo[range.buf].filetype)) do
      add({ label = p.name .. "  " .. p.pattern, pattern = p.pattern, kind = "saved" })
    end
  end
  if mode == "recent" or mode == "all" then
    local history = align.get_history()
    for i = #history, 1, -1 do
      add({ label = history[i], pattern = history[i], kind = "recent" })
    end
  end
  return out
end

---@param mode string
---@param range align.Range
function M.pick(mode, range)
  local ok = pcall(require, "telescope")
  if not ok then
    return vim.notify("align: telescope.nvim is not installed", vim.log.levels.ERROR)
  end
  if not vim.tbl_contains({ "saved", "recent", "all" }, mode) then
    return vim.notify("align: unknown telescope mode '" .. mode .. "' (saved, recent, all)", vim.log.levels.ERROR)
  end
  if not align.bin() then return end

  local pickers = require("telescope.pickers")
  local finders = require("telescope.finders")
  local previewers = require("telescope.previewers")
  local conf = require("telescope.config").values
  local actions = require("telescope.actions")
  local state = require("telescope.actions.state")

  local results = entries(mode, range)
  if #results == 0 then
    return vim.notify("align: no " .. (mode == "all" and "saved or recent" or mode) .. " patterns", vim.log.levels.INFO)
  end
  local lines = align.get_lines(range)
  local tabstop = vim.bo[range.buf].tabstop

  local previewer = previewers.new_buffer_previewer({
    title = "Aligned",
    define_preview = function(self, entry)
      local buf = self.state.bufnr
      vim.bo[buf].tabstop = tabstop
      vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
      align.run(entry.value.pattern, lines, { tabstop = tabstop }, function(output, err)
        if not vim.api.nvim_buf_is_valid(buf) then return end
        vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
        if err then
          vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.list_extend({ "✗ " .. err.message, "" }, lines))
          vim.api.nvim_buf_set_extmark(buf, ns, 0, 0, { line_hl_group = "DiagnosticError" })
          return
        end
        vim.api.nvim_buf_set_lines(buf, 0, -1, false, output)
        for i, line in ipairs(output) do
          if line ~= lines[i] then
            vim.api.nvim_buf_set_extmark(buf, ns, i - 1, 0, { line_hl_group = "DiffChange" })
          end
        end
      end)
    end,
  })

  pickers.new({}, {
    prompt_title = ("Align [%s]  <CR> apply · <C-e> edit"):format(mode),
    finder = finders.new_table({
      results = results,
      entry_maker = function(e)
        local icon = e.kind == "saved" and "★ " or "↺ "
        return { value = e, display = icon .. e.label, ordinal = e.label }
      end,
    }),
    sorter = conf.generic_sorter({}),
    previewer = previewer,
    attach_mappings = function(prompt_buf, map)
      actions.select_default:replace(function()
        local sel = state.get_selected_entry()
        actions.close(prompt_buf)
        if sel then align.apply(sel.value.pattern, range) end
      end)
      map({ "i", "n" }, "<C-e>", function()
        local sel = state.get_selected_entry()
        actions.close(prompt_buf)
        if sel then vim.schedule(function() align.open(range, sel.value.pattern) end) end
      end)
      return true
    end,
  }):find()
end

return M
