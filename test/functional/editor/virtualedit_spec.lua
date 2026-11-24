local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each
local eq = t.eq

describe("'virtualedit'", function()
  before_each(n.clear)

  -- Runs `keys` on `lines` with 'virtualedit' set to "all" and returns the
  -- lines and the unnamed register.
  local function edit(lines, keys)
    return n.exec_lua(function(l, k)
      vim.o.virtualedit = 'all'
      vim.api.nvim_buf_set_lines(0, 0, -1, true, l)
      vim.fn.setreg('"', '')
      vim.cmd('normal! ' .. vim.keycode(k))
      return { vim.api.nvim_buf_get_lines(0, 0, -1, true), vim.fn.getreg('"') }
    end, lines, keys)
  end

  it('ends a motion moved back to the end of a line like $', function()
    -- An exclusive motion that ends in column 1 ends at the end of the line
    -- before it. Upstream ends it at the start of a last tab, before a cursor
    -- inside that tab, and crashes.
    eq({ { 'a  ', '' }, '     ' }, edit({ 'a\t', '' }, '4|d}'))
    for _, case in ipairs({
      { { 'a\t', '' }, '4|y}', '4|y$' },
      { { 'a\t', '' }, '4|d}', '4|d$' },
      { { 'a\t', '' }, '3|d}', '3|d$' },
      { { 'a\t', '' }, '2|d}', '2|d$' },
      { { 'a\t', '' }, '4|c}x<Esc>', '4|c$x<Esc>' },
      { { 'a\t', 'b' }, '4|d/^b<CR>', '4|d$' },
      { { 'xa\t', '' }, '2|d}', '2|d$' },
      { { 'a語', '' }, '3|d}', '3|d$' },
    }) do
      local lines, keys, like = unpack(case)
      eq(edit(lines, like), edit(lines, keys), keys .. ' on ' .. vim.inspect(lines))
    end
  end)
end)
