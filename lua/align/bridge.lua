--- Finding and calling the `align` binary (`align --json`).
local M = {}

--- Plugin root (the directory containing lua/, plugin/, Cargo.toml).
M.root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h:h")

local exe = vim.fn.has("win32") == 1 and "align.exe" or "align"

--- Places the binary is looked for, in order.
---@param configured string|nil
---@return string[]
function M.candidates(configured)
  if configured then return { configured } end
  return {
    M.root .. "/target/release/" .. exe,
    exe,
    vim.fn.expand("~/.cargo/bin/") .. exe,
  }
end

--- Resolved binary path, or nil and an explanation.
---@param configured string|nil
---@return string|nil, string|nil
function M.resolve(configured)
  for _, c in ipairs(M.candidates(configured)) do
    if vim.fn.executable(c) == 1 then return c end
  end
  if configured then
    return nil, ("align.nvim: bin = %q is not executable"):format(configured)
  end
  return nil, "align.nvim: the align binary hasn't been built. Run :Lazy build align "
    .. "(or `cargo build --release` in " .. M.root .. ")"
end

---@class align.Error
---@field message string
---@field col integer|nil 0-based character offset into the pattern

--- Align `lines` with `pattern` asynchronously.
--- `callback(output, err)` runs on the main loop.
---@param bin string
---@param pattern string
---@param lines string[]
---@param opts { tabstop?: integer }
---@param callback fun(output: string[]|nil, err: align.Error|nil)
function M.call(bin, pattern, lines, opts, callback)
  local request = vim.json.encode({ pattern = pattern, lines = lines, tabstop = opts.tabstop })
  local ok, err = pcall(vim.system, { bin, "--json" }, { stdin = request, text = true }, function(res)
    vim.schedule(function()
      local raw = vim.trim(res.stdout or "")
      if res.code ~= 0 or raw == "" then
        local msg = vim.trim(res.stderr or "")
        callback(nil, { message = msg ~= "" and msg or ("align exited with code " .. res.code) })
        return
      end
      local decoded_ok, resp = pcall(vim.json.decode, raw, { luanil = { object = true } })
      if not decoded_ok or type(resp) ~= "table" then
        callback(nil, { message = "unexpected output from " .. bin .. ": " .. raw })
      elseif resp.error then
        local e = resp.error
        if type(e) == "string" then
          -- v1 binaries return plain strings and don't understand "pattern".
          e = { message = e .. " (the align binary is outdated; run :Lazy build align)" }
        end
        callback(nil, e)
      else
        callback(resp.output, nil)
      end
    end)
  end)
  if not ok then
    callback(nil, { message = "failed to run " .. bin .. ": " .. tostring(err) })
  end
end

--- `align --version` output, synchronously (used by :checkhealth).
---@param bin string
---@return string|nil
function M.version(bin)
  local res = vim.system({ bin, "--version" }, { text = true }):wait(2000)
  if res.code ~= 0 then return nil end
  return (res.stdout or ""):match("align ([%w%.%-]+)")
end

return M
