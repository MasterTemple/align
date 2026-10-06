--- :checkhealth align
local M = {}

function M.check()
  local health = vim.health
  local align = require("align")
  local bridge = require("align.bridge")

  health.start("align.nvim")
  if vim.fn.has("nvim-0.10") == 1 then
    health.ok("Neovim " .. tostring(vim.version()))
  else
    health.error("Neovim 0.10+ is required")
  end

  if vim.fn.executable("cargo") == 1 then
    health.ok("cargo found (needed to build the binary)")
  else
    health.warn("cargo not found", { "Install Rust from https://rustup.rs, then run :Lazy build align" })
  end

  local bin, err = bridge.resolve(align.config.bin)
  if not bin then
    health.error(err or "binary not found", { "Searched: " .. table.concat(bridge.candidates(align.config.bin), ", ") })
    return
  end
  health.ok("binary: " .. vim.fn.exepath(bin))

  local version = bridge.version(bin)
  if version == align.version then
    health.ok("binary version " .. version)
  else
    health.error(
      ("binary version %s, plugin version %s"):format(version or "unknown", align.version),
      { "Rebuild with :Lazy build align" }
    )
  end

  local res = vim.system({ bin, "--json" }, {
    stdin = vim.json.encode({ pattern = "=", lines = { "a = 1", "bb = 2" } }),
    text = true,
  }):wait(2000)
  local ok, decoded = pcall(vim.json.decode, res.stdout or "")
  if ok and type(decoded) == "table" and type(decoded.output) == "table" and decoded.output[1] == "a  = 1" then
    health.ok("JSON round-trip works")
  else
    health.error("JSON round-trip failed: " .. vim.trim((res.stdout or "") .. (res.stderr or "")))
  end
end

return M
