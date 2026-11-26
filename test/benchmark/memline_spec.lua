local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each

describe('memline perf', function()
  before_each(function()
    n.clear()
    n.get_session().request_timeout = 60000
    n.exec_lua([[
      -- Fills the current buffer with `count` lines, every seventh one empty.
      _G.fill = function(count)
        local lines = {}
        for i = 1, count do
          lines[i] = i % 7 == 0 and '' or ('line %d of the buffer with some words in it'):format(i)
        end
        vim.api.nvim_buf_set_lines(0, 0, -1, true, lines)
      end

      -- Times `op` on a buffer of `count` lines prepared by `setup`.
      _G.time_op = function(count, setup, op)
        vim.cmd('silent! %bwipeout!')
        _G.fill(count)
        if setup ~= '' then
          vim.cmd(setup)
        end
        collectgarbage('collect')
        local start = vim.uv.hrtime()
        vim.cmd(op)
        return (vim.uv.hrtime() - start) / 1e6
      end
    ]])
  end)

  local function bench(name, setup, op)
    for _, count in ipairs({ 250000, 1250000 }) do
      local ms = n.exec_lua(function(c, s, o)
        return _G.time_op(c, s, o)
      end, count, setup, op)
      print(('\nmemline %s (%d lines): %.1f ms'):format(name, count, ms))
    end
  end

  it('loads a file and inserts into separate loaded chunks', function()
    local count = 500000
    local path = t.tmpname()
    local file = assert(io.open(path, 'wb'))
    for i = 1, count do
      file:write(('%08d item\n'):format(i))
    end
    assert(file:close())

    local result = n.exec_lua(function(input, line_count)
      vim.o.verbose = 0
      vim.o.swapfile = false
      vim.bo.undofile = false
      collectgarbage('collect')
      local initial_rss = vim.uv.resident_set_memory()
      local start = vim.uv.hrtime()
      vim.cmd.edit(vim.fn.fnameescape(input))
      local load_ms = (vim.uv.hrtime() - start) / 1e6
      assert(vim.api.nvim_buf_line_count(0) == line_count)
      collectgarbage('collect')
      local rss_growth = vim.uv.resident_set_memory() - initial_rss

      start = vim.uv.hrtime()
      local edits = 0
      for row = line_count - 200, 200, -800 do
        vim.api.nvim_buf_set_lines(0, row, row, true, { 'inserted' })
        edits = edits + 1
      end
      local insert_ms = (vim.uv.hrtime() - start) / 1e6
      assert(vim.api.nvim_buf_line_count(0) == line_count + edits)
      assert(vim.fn.line2byte(line_count + edits + 1) == line_count * 14 + edits * 9 + 1)
      collectgarbage('collect')
      return {
        load_ms = load_ms,
        insert_ms = insert_ms,
        rss_growth = rss_growth,
        edited_rss_growth = vim.uv.resident_set_memory() - initial_rss,
        edits = edits,
      }
    end, path, count)
    print(
      ('\nmemline file load (%d lines): %.2f ms, %.2f MiB RSS growth'):format(
        count,
        result.load_ms,
        result.rss_growth / 1048576
      )
    )
    print(
      ('memline first inserts (%d): %.2f ms, %.2f MiB total RSS growth'):format(
        result.edits,
        result.insert_ms,
        result.edited_rss_growth / 1048576
      )
    )
    n.command('bwipeout!')
    os.remove(path)
  end)

  it('pastes the whole buffer at the top', function()
    bench('paste whole buffer', 'silent %yank', 'silent normal! ggP')
  end)

  it('deletes all lines', function()
    bench('delete all lines', '', 'silent %delete _')
  end)

  it('sorts all lines', function()
    bench('sort', '', 'silent sort')
  end)

  it('undoes a substitution on every line', function()
    bench('undo substitution', 'silent %s/e/E/ge', 'silent undo')
  end)

  it('deletes every empty line with :global', function()
    bench('delete empty lines', '', 'silent g/^$/d')
  end)
end)
