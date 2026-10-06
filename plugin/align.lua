if vim.g.loaded_align then return end
vim.g.loaded_align = true

vim.api.nvim_create_user_command("Align", function(info)
  require("align").command(info)
end, {
  range = true,
  nargs = "*",
  complete = function(arg_lead, cmd_line)
    return require("align").complete(arg_lead, cmd_line)
  end,
  desc = "Align lines into columns (no range = whole buffer)",
})
