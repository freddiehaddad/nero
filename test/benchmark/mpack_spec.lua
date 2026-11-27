local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each

describe('vim.mpack perf', function()
  before_each(function()
    n.clear()
    n.get_session().request_timeout = 60000
  end)

  for _, mib in ipairs({ 1, 4, 16 }) do
    it(('encode and decode a %d MiB string'):format(mib), function()
      local result = n.exec_lua(function(size)
        local s = ('x'):rep(size * 1024 * 1024)
        local function median(f)
          local times = {}
          for i = 1, 5 do
            local start = vim.uv.hrtime()
            f()
            times[i] = (vim.uv.hrtime() - start) / 1e6
          end
          table.sort(times)
          return times[3]
        end
        local packed = vim.mpack.encode(s)
        return {
          median(function()
            vim.mpack.encode(s)
          end),
          median(function()
            vim.mpack.decode(packed)
          end),
        }
      end, mib)
      print(
        ('\nmpack %d MiB string: encode %.3f ms, decode %.3f ms'):format(mib, result[1], result[2])
      )
    end)
  end
end)
