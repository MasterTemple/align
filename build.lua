-- Run by lazy.nvim after install/update (and by :Lazy build align).
-- Builds the `align` binary into target/release/, where the plugin looks first.
local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h")

if vim.fn.executable("cargo") == 0 then
  error("align.nvim: `cargo` not found. Install Rust from https://rustup.rs, then run :Lazy build align")
end

local done, result = false, nil
vim.system({ "cargo", "build", "--release", "-p", "align-cli" }, { cwd = root, text = true }, function(res)
  result, done = res, true
end)

-- Inside lazy's build task, yielding reports progress without blocking the UI.
while not done do
  if coroutine.isyieldable() then
    if coroutine.yield("cargo build --release (align)") == "abort" then
      error("align.nvim: build aborted")
    end
  else
    vim.wait(200, function() return done end)
  end
end

if result.code ~= 0 then
  error("align.nvim: cargo build failed:\n" .. (result.stderr or ""))
end
