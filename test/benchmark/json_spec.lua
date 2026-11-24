local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each

describe('vim.json.decode perf', function()
  before_each(function()
    n.clear()
    n.get_session().request_timeout = 30000
    -- Decodes the JSON text of a value the way the LSP client decodes a message.
    n.exec_lua([[
      _G.time_decode = function(value)
        local text = vim.json.encode(value)
        vim.json.decode(text)
        collectgarbage('collect')
        local times = {}
        for i = 1, 5 do
          local start = vim.uv.hrtime()
          vim.json.decode(text)
          times[i] = (vim.uv.hrtime() - start) / 1e6
        end
        table.sort(times)
        return { #text / 1024, times[3] }
      end
    ]])
  end)

  local function report(name, result)
    print(('\njson decode %s (%d KB): %.3f ms'):format(name, result[1], result[2]))
  end

  it('completion items with fractional scores', function()
    report(
      'completion with decimals',
      n.exec_lua(function()
        local items = {}
        for i = 1, 3000 do
          items[i] = {
            label = 'completion_item_' .. i,
            kind = i % 25 + 1,
            detail = 'fun(a: integer, b: string): table<string, any>',
            documentation = { kind = 'markdown', value = ('Some *markdown* text. '):rep(8) },
            sortText = ('%08d'):format(i),
            textEdit = {
              range = {
                start = { line = i, character = 4 },
                ['end'] = { line = i, character = 12 },
              },
              newText = 'x',
            },
            score = 0.5 + i / 100003,
          }
        end
        return _G.time_decode({
          jsonrpc = '2.0',
          id = 1,
          result = { isIncomplete = false, items = items },
        })
      end)
    )
  end)

  it('completion items without fractional numbers', function()
    report(
      'completion without decimals',
      n.exec_lua(function()
        local items = {}
        for i = 1, 3000 do
          items[i] = {
            label = 'completion_item_' .. i,
            kind = i % 25 + 1,
            detail = 'fun(a: integer, b: string): table<string, any>',
            documentation = { kind = 'markdown', value = ('Some *markdown* text. '):rep(8) },
            sortText = ('%08d'):format(i),
            textEdit = {
              range = {
                start = { line = i, character = 4 },
                ['end'] = { line = i, character = 12 },
              },
              newText = 'x',
            },
            data = { id = i },
          }
        end
        return _G.time_decode({
          jsonrpc = '2.0',
          id = 1,
          result = { isIncomplete = false, items = items },
        })
      end)
    )
  end)

  it('document colors', function()
    report(
      'document colors',
      n.exec_lua(function()
        local colors = {}
        for i = 1, 3000 do
          colors[i] = {
            range = { start = { line = i, character = 10 }, ['end'] = { line = i, character = 17 } },
            color = {
              red = (i % 255) / 255,
              green = ((i * 7) % 255) / 255,
              blue = ((i * 13) % 255) / 255,
              alpha = 1,
            },
          }
        end
        return _G.time_decode({ jsonrpc = '2.0', id = 2, result = colors })
      end)
    )
  end)

  it('coordinates', function()
    report(
      'coordinates',
      n.exec_lua(function()
        local coords = {}
        for i = 1, 50000 do
          coords[i] = { -122.4194 + i * 1e-5, 37.7749 + i * 2e-5 }
        end
        return _G.time_decode({ type = 'LineString', coordinates = coords })
      end)
    )
  end)

  it('semantic tokens', function()
    report(
      'semantic tokens',
      n.exec_lua(function()
        local data = {}
        for i = 1, 300000 do
          data[i] = (i * 7919) % 300
        end
        return _G.time_decode({ jsonrpc = '2.0', id = 3, result = { resultId = '1', data = data } })
      end)
    )
  end)
end)
