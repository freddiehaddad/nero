local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each
local clear = n.clear
local exec_lua = n.exec_lua
local eq = t.eq
local matches = t.matches
local pcall_err = t.pcall_err

describe('xdiff bindings', function()
  before_each(function()
    clear()
  end)

  describe('can diff text', function()
    local a1 = 'Hello\n'
    local b1 = 'Helli\n'

    local a2 = 'Hello\nbye\nfoo\n'
    local b2 = 'Helli\nbye\nbar\nbaz\n'

    it('with no callback', function()
      eq(
        table.concat({
          '@@ -1 +1 @@',
          '-Hello',
          '+Helli',
          '',
        }, '\n'),
        exec_lua(function()
          return vim.text.diff(a1, b1)
        end)
      )

      eq(
        table.concat({
          '@@ -1 +1 @@',
          '-Hello',
          '+Helli',
          '@@ -3 +3,2 @@',
          '-foo',
          '+bar',
          '+baz',
          '',
        }, '\n'),
        exec_lua(function()
          return vim.text.diff(a2, b2)
        end)
      )
    end)

    it('with callback', function()
      eq(
        { { 1, 1, 1, 1 } },
        exec_lua(function()
          local exp = {} --- @type table[]
          assert(vim.text.diff(a1, b1, {
            on_hunk = function(...)
              exp[#exp + 1] = { ... }
            end,
          }) == nil)
          return exp
        end)
      )

      eq(
        { { 1, 1, 1, 1 }, { 3, 1, 3, 2 } },
        exec_lua(function()
          local exp = {} --- @type table[]
          assert(vim.text.diff(a2, b2, {
            on_hunk = function(...)
              exp[#exp + 1] = { ... }
            end,
          }) == nil)
          return exp
        end)
      )

      -- gives higher precedence to on_hunk over result_type
      eq(
        { { 1, 1, 1, 1 }, { 3, 1, 3, 2 } },
        exec_lua(function()
          local exp = {} --- @type table[]
          assert(vim.text.diff(a2, b2, {
            on_hunk = function(...)
              exp[#exp + 1] = { ... }
            end,
            result_type = 'indices',
          }) == nil)
          return exp
        end)
      )
    end)

    it('with error callback', function()
      t.matches(
        [[on_hunk: %.%.%./xdiff_spec%.lua%:0%: ERROR1]],
        pcall_err(exec_lua, function()
          vim.text.diff(a1, b1, {
            on_hunk = function()
              error('ERROR1')
            end,
          })
        end)
      )
    end)

    it('with hunk_lines', function()
      eq(
        { { 1, 1, 1, 1 } },
        exec_lua(function()
          return vim.text.diff(a1, b1, { result_type = 'indices' })
        end)
      )

      eq(
        { { 1, 1, 1, 1 }, { 3, 1, 3, 2 } },
        exec_lua(function()
          return vim.text.diff(a2, b2, { result_type = 'indices' })
        end)
      )
    end)

    it('can run different algorithms', function()
      local a = table.concat({
        '.foo1 {',
        '    margin: 0;',
        '}',
        '',
        '.bar {',
        '    margin: 0;',
        '}',
        '',
      }, '\n')

      local b = table.concat({
        '.bar {',
        '    margin: 0;',
        '}',
        '',
        '.foo1 {',
        '    margin: 0;',
        '    color: green;',
        '}',
        '',
      }, '\n')

      eq(
        table.concat({
          '@@ -1,4 +0,0 @@',
          '-.foo1 {',
          '-    margin: 0;',
          '-}',
          '-',
          '@@ -7,0 +4,5 @@',
          '+',
          '+.foo1 {',
          '+    margin: 0;',
          '+    color: green;',
          '+}',
          '',
        }, '\n'),
        exec_lua(function()
          return vim.text.diff(a, b, {
            algorithm = 'patience',
          })
        end)
      )
    end)
  end)

  it('locates later hunks after a hunk exceeds the linematch limit', function()
    for _, algorithm in ipairs({ 'myers', 'minimal', 'patience', 'histogram' }) do
      eq(
        { { 2, 1, 2, 1 }, { 4, 3, 4, 3 }, { 8, 1, 8, 1 } },
        exec_lua(function(alg)
          return vim.text.diff(
            'keep0\nold1\nkeep2\nold3\nold4\nold5\nkeep6\nold7\nkeep8\n',
            'keep0\nnew1\nkeep2\nnew3\nnew4\nnew5\nkeep6\nnew7\nkeep8\n',
            { result_type = 'indices', linematch = 2, algorithm = alg }
          )
        end, algorithm)
      )
    end
  end)

  it('locates insertions, deletions and binary hunks at input boundaries', function()
    local cases = {
      { '', '', {} },
      { '', 'new\n', { { 0, 0, 1, 1 } } },
      { 'one\ntwo', '', { { 1, 2, 0, 0 } } },
      { 'anchor\n', 'anchor\nlast', { { 1, 0, 2, 1 } } },
      { 'anchor\nlast', 'anchor\n', { { 2, 1, 1, 0 } } },
      {
        'keep0\nremove\nkeep1\nkeep2\nkeep3\n',
        'insert\nkeep0\nkeep1\nkeep2\nnew\nkeep3\n',
        { { 0, 0, 1, 1 }, { 2, 1, 2, 0 }, { 4, 0, 5, 1 } },
      },
      {
        'keep\nold\0\195\169\nanchor\nold ending',
        'keep\nnew\0\195\169\nanchor\nnew ending',
        { { 2, 1, 2, 1 }, { 4, 1, 4, 1 } },
      },
    }
    for _, algorithm in ipairs({ 'myers', 'minimal', 'patience', 'histogram' }) do
      for _, case in ipairs(cases) do
        eq(
          case[3],
          exec_lua(function(a, b, alg)
            return vim.text.diff(a, b, {
              result_type = 'indices',
              linematch = 40,
              algorithm = alg,
            })
          end, case[1], case[2], algorithm)
        )
      end
    end
  end)

  it('keeps nested diff calls independent', function()
    eq(
      {
        { { 1, 1, 1, 1 }, { 3, 1, 3, 1 } },
        { { { 1, 0, 2, 1 } }, { { 1, 0, 2, 1 } } },
      },
      exec_lua(function()
        local outer, inner = {}, {}
        vim.text.diff('old one\nanchor\nold two\n', 'new one\nanchor\nnew two\n', {
          on_hunk = function(...)
            outer[#outer + 1] = { ... }
            inner[#inner + 1] = vim.text.diff('anchor\n', 'anchor\nlast', {
              result_type = 'indices',
              linematch = 40,
            })
          end,
        })
        return { outer, inner }
      end)
    )
  end)

  it('can handle bad args', function()
    matches([[Expected at least 2 arguments$]], pcall_err(exec_lua, [[vim.text.diff('a')]]))

    matches(
      [[bad argument %#1 to '_?diff' %(expected string%)]],
      pcall_err(exec_lua, [[vim.text.diff(1, 2)]])
    )

    matches(
      [[bad argument %#3 to '_?diff' %(expected table%)]],
      pcall_err(exec_lua, [[vim.text.diff('a', 'b', true)]])
    )

    matches(
      [[invalid key: bad_key$]],
      pcall_err(exec_lua, [[vim.text.diff('a', 'b', { bad_key = true })]])
    )

    matches(
      [[on_hunk is not a function$]],
      pcall_err(exec_lua, [[vim.text.diff('a', 'b', { on_hunk = true })]])
    )
  end)

  it('can handle strings with embedded NUL characters (GitHub #30305)', function()
    eq(
      { { 0, 0, 1, 1 }, { 1, 0, 3, 2 } },
      exec_lua(function()
        return vim.text.diff('\n', '\0\n\n\nb', { linematch = true, result_type = 'indices' })
      end)
    )
  end)
end)
