local n = require('test.functional.testnvim')()
local t = require('test.testutil')

local describe, it, before_each = t.describe, t.it, t.before_each

local netrw = 'runtime/pack/dist/opt/netrw/autoload/netrw.vim'

describe('regexp workloads perf', function()
  before_each(n.clear)

  --- Fills the buffer with 100k Vim script assignments.
  local function set_vim_lines()
    n.exec_lua(function()
      local lines = {} --- @type string[]
      for i = 1, 100000 do
        lines[i] = ('let s:var_%d = get(g:, "opt_%d", %d) " comment'):format(i, i, i)
      end
      vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
    end)
  end

  it(':%s///gn across 100k lines', function()
    set_vim_lines()
    local samples = n.exec_lua(function()
      local out = {} --- @type number[]
      for i = 1, 5 do
        local t0 = vim.uv.hrtime()
        vim.cmd([[silent %s/\<opt_\d\+\>//gn]])
        out[i] = vim.uv.hrtime() - t0
      end
      return out
    end)
    t.bench_report(samples, { label = 'regexp_subst_count_100k' })
  end)

  it('search() through 100k lines', function()
    set_vim_lines()
    local samples = n.exec_lua(function()
      local out = {} --- @type number[]
      for i = 1, 5 do
        vim.cmd('normal! gg')
        local t0 = vim.uv.hrtime()
        local count = 0
        while vim.fn.search([[\<var_\d\+\>]], 'W') > 0 do
          count = count + 1
        end
        out[i] = vim.uv.hrtime() - t0
        assert(count == 100000)
      end
      return out
    end)
    t.bench_report(samples, { label = 'regexp_search_loop_100k' })
  end)

  it('substitute() with 20k matches in one string', function()
    local samples = n.exec_lua(function()
      local s = ('ab '):rep(20000)
      local out = {} --- @type number[]
      for i = 1, 5 do
        local t0 = vim.uv.hrtime()
        vim.fn.substitute(s, ' ', '_', 'g')
        out[i] = vim.uv.hrtime() - t0
      end
      return out
    end)
    t.bench_report(samples, { label = 'regexp_substitute_20k' })
  end)

  it('vim.regex():match_str() on 100k strings', function()
    local samples = n.exec_lua(function()
      local lines = {} --- @type string[]
      for i = 1, 100000 do
        lines[i] = ('let s:var_%d = get(g:, "opt_%d", %d) " comment'):format(i, i, i)
      end
      local re = vim.regex([[\<opt_\d\+\>]])
      local out = {} --- @type number[]
      for i = 1, 5 do
        local t0 = vim.uv.hrtime()
        for _, line in ipairs(lines) do
          re:match_str(line)
        end
        out[i] = vim.uv.hrtime() - t0
      end
      return out
    end)
    t.bench_report(samples, { label = 'regexp_match_str_100k' })
  end)

  it('legacy syntax redraw scrolling 4000 lines of Vim script', function()
    local samples = n.exec_lua(function(path)
      vim.o.columns, vim.o.lines = 160, 50
      vim.cmd('silent edit ' .. path)
      vim.cmd('syntax on')
      vim.wo.foldenable = false
      local out = {} --- @type number[]
      for i = 1, 5 do
        -- Load the syntax again, so every sample parses the text from scratch.
        vim.bo.syntax = ''
        vim.bo.syntax = 'vim'
        vim.cmd('normal! gg')
        vim.cmd('redraw!')
        local t0 = vim.uv.hrtime()
        while vim.fn.line('w$') < 4000 do
          vim.cmd('normal! \6')
          vim.cmd('redraw')
        end
        out[i] = vim.uv.hrtime() - t0
      end
      return out
    end, netrw)
    t.bench_report(samples, { label = 'regexp_syntax_scroll' })
  end)

  it('gg=G on 1000 lines of Vim script', function()
    local samples = n.exec_lua(function(path)
      local lines = vim.fn.readfile(path, '', 1000)
      vim.cmd('filetype indent on')
      vim.bo.filetype = 'vim'
      local out = {} --- @type number[]
      for i = 1, 5 do
        vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
        local t0 = vim.uv.hrtime()
        vim.cmd('silent normal! gg=G')
        out[i] = vim.uv.hrtime() - t0
      end
      return out
    end, netrw)
    t.bench_report(samples, { label = 'regexp_indent' })
  end)
end)
