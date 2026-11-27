local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it = t.describe, t.it
local uv = vim.uv

--- Raw embedded instance: grid events are not decoded, so the measurement
--- covers the server-side redraw rather than the client.
local function spawn_embed()
  local stdin, stdout = assert(uv.new_pipe()), assert(uv.new_pipe())
  local proc = assert(uv.spawn(n.nvim_prog, {
    args = { '--embed', '--clean', '-n' },
    stdio = { stdin, stdout, nil },
  }, function() end))
  local ext = function(_, s)
    return s
  end
  local session = vim.mpack.Session({
    unpack = vim.mpack.Unpacker({ ext = { [0] = ext, [1] = ext, [2] = ext } }),
  })
  local received, sent = 0, 0
  stdout:read_start(function(err, data)
    assert(not err, err)
    local pos = 1
    while data and pos <= #data do
      local kind, _, _, _, next_pos = session:receive(data, pos)
      pos = next_pos
      if kind == 'response' then
        received = received + 1
      end
    end
  end)
  local function request(method, ...)
    stdin:write(session:request(true) .. vim.mpack.encode(method) .. vim.mpack.encode({ ... }))
    sent = sent + 1
    while received < sent do
      uv.run('once')
    end
  end
  local function close()
    proc:kill()
    stdin:close()
    stdout:close()
    proc:close()
  end
  return request, close
end

describe('grid resize perf', function()
  for _, size in ipairs({ { 80, 24 }, { 300, 100 }, { 999, 999 } }) do
    local width, height = size[1], size[2]
    it(('resize a UI to %dx%d'):format(width, height), function()
      local request, close = spawn_embed()
      request('nvim_ui_attach', width, height, { ext_linegrid = true, rgb = true })
      local times = {}
      for i = 1, 9 do
        local start = uv.hrtime()
        -- Alternate sizes so every round reallocates and redraws the grid.
        request('nvim_ui_try_resize', width - i % 2, height)
        request('nvim_eval', '1')
        times[i] = (uv.hrtime() - start) / 1e6
      end
      close()
      table.sort(times)
      print(('\nresize %dx%d: %.3f ms'):format(width, height, times[5]))
    end)
  end
end)
