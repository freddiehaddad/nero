-- Nero-only: how expression text is read, beyond what the upstream tests cover.

local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each, finally = t.describe, t.it, t.before_each, t.finally
local eq = t.eq
local matches = t.matches
local pcall_err = t.pcall_err

describe('expression text', function()
  before_each(n.clear)

  it('of a long List or Dict is read in linear time', function()
    -- Reading each value used to scan the rest of the text: minutes here.
    local session = n.get_session()
    local previous_timeout = session.request_timeout
    session.request_timeout = 10000
    finally(function()
      session.request_timeout = previous_timeout
    end)
    eq(1, n.eval('eval(string(range(300000))) == range(300000)'))
    n.command([[let g:dicts = map(range(50000), {i, v -> {('k' .. v): [v, 's' .. v]}})]])
    eq(1, n.eval('eval(string(g:dicts)) == g:dicts'))
  end)

  it('does not index or call a value after white space', function()
    n.exec([[
      let g:l = [10, 20]
      let g:d = {'k': [30, 40]}
      function! F()
        return [50, 60]
      endfunction
    ]])
    eq('\n[10, 20] [1]', n.eval([[execute('echo g:l [1]')]]))
    eq('\n20', n.eval([[execute('echo g:l[1]')]]))
    eq('\n[30, 40] [0]', n.eval([[execute('echo g:d.k [0]')]]))
    eq(0, n.fn.exists('g:l [1]'))
    eq(1, n.fn.exists('g:l[1]'))
    eq('Vim(call):E488: Trailing characters: [0]', pcall_err(n.command, 'call F() [0]'))
    n.command('call F()[0]')
    eq('\ng:l[1]                #20', n.eval([[execute('let g:l[1]')]]))
    matches('E488: Trailing characters: %[1%]$', pcall_err(n.command, 'let g:l [1]'))
  end)
end)
