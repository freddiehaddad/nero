local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each

describe('Vimscript perf', function()
  before_each(function()
    n.clear()
    n.get_session().request_timeout = 60000
    n.exec([[
      function! Inc(x)
        let y = a:x + 1
        return y
      endfunction
    ]])
  end)

  --- Runs `setup` once, then `cmd` `rounds` times, and prints the median time.
  local function report(name, setup, cmd, rounds)
    local ms = n.exec_lua(function(setup_, cmd_, rounds_)
      vim.cmd(setup_)
      local times = {}
      for i = 1, rounds_ do
        local start = vim.uv.hrtime()
        vim.cmd(cmd_)
        times[i] = (vim.uv.hrtime() - start) / 1e6
      end
      table.sort(times)
      return times[math.ceil(rounds_ / 2)]
    end, setup, cmd, rounds)
    print(('\nvimscript %s: %.1f ms'):format(name, ms))
  end

  it('eval() of a saved list of 100000 numbers', function()
    report('eval list', 'let g:s = string(range(100000))', 'let g:x = eval(g:s)', 5)
  end)

  it('eval() of 20000 saved dictionaries', function()
    report(
      'eval dicts',
      [[let g:s = string(map(range(20000), {i, v -> {('k' .. v): v}}))]],
      'let g:x = eval(g:s)',
      5
    )
  end)

  it(':source of autoload/xml/xhtml10t.vim', function()
    report('source xhtml10t', '', 'source $VIMRUNTIME/autoload/xml/xhtml10t.vim', 5)
  end)

  it(':for loop with :let', function()
    report('for/let loop', '', 'for i in range(500000) | let x = i | endfor', 3)
  end)

  it('calls of a two-line function', function()
    report('function calls', '', 'for i in range(100000) | call Inc(i) | endfor', 3)
  end)

  it('map() with a lambda', function()
    report('map lambda', 'let g:l = range(1000000)', 'call map(copy(g:l), {i, v -> v + 1})', 3)
  end)

  it('sort() with a lambda', function()
    report(
      'sort lambda',
      'let g:l = map(range(100000), {i, v -> v * 7919 % 100003})',
      'call sort(copy(g:l), {a, b -> a - b})',
      3
    )
  end)

  it('gg=G on Python', function()
    local block = {
      'class C:',
      '    def method(self, a, b=(1, 2)):',
      '        if a > b[0]:',
      '            for x in range(a):',
      '                while x:',
      '                    x -= 1',
      '        else:',
      '            return [',
      '                a,',
      '                b,',
      '            ]',
      "        return {'k': a}",
      '',
    }
    local lines = {}
    for _ = 1, 160 do
      vim.list_extend(lines, block)
    end
    n.api.nvim_buf_set_lines(0, 0, -1, true, lines)
    n.command('filetype plugin indent on | setfiletype python')
    t.eq('python#GetIndent(v:lnum)', n.eval('&indentexpr'))
    report('reindent python', '', 'normal! gg=G', 3)
  end)
end)
