local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each
local eq = t.eq

describe('regexp engine', function()
  before_each(n.clear)

  it('matches unset groups, multi-line backreferences and multibyte repeats', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          unset_backref = { 'b', 0, 1 },
          backref_past_last_line = { 0, 0 },
          multiline_backref = { 1, 2 },
          unset_submatch_string = "['', [], 'b']",
          unset_submatch_buffer = { "['', [], 'b']" },
          multibyte_repeat_ic = 'ÉéÉ',
        },
        n.exec_lua(function(p)
          local fn, api = vim.fn, vim.api
          local function search(lines, pat)
            api.nvim_buf_set_lines(0, 0, -1, false, lines)
            return fn.searchpos(p .. pat, 'cnw')
          end
          local show = [[\=string([submatch(1), submatch(1, 1), submatch(2)])]]
          local r = {}
          r.unset_backref = fn.matchstrpos('b', p .. [[\(a\)\=b\1]])
          r.backref_past_last_line = search({ 'xb', 'cb' }, [[\(b\nc\)\1]])
          r.multiline_backref = search({ 'xb', 'cb', 'c' }, [[\(b\nc\)\1]])
          r.unset_submatch_string = fn.substitute('ab', p .. [[\(x\)\=a\(b\)]], show, '')
          api.nvim_buf_set_lines(0, 0, -1, false, { 'ab' })
          vim.cmd('silent s/' .. p .. [[\(x\)\=a\(b\)/]] .. show .. '/')
          r.unset_submatch_buffer = api.nvim_buf_get_lines(0, 0, -1, false)
          r.multibyte_repeat_ic = fn.matchstr('xÉéÉy', p .. [[\cé\+]])
          return r
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('substitutes multi-line submatches and escaped text in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          multiline_submatch_upper = { 'a[1', 'B2', 'C3', 'D]4' },
          escaped_backslash_and_cr = { [[<x\y>]], '[a\rb]' },
        },
        n.exec_lua(function(p)
          local function sub(lines, ...)
            vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
            for _, cmd in ipairs({ ... }) do
              vim.cmd('silent ' .. cmd)
            end
            return vim.api.nvim_buf_get_lines(0, 0, -1, false)
          end
          local r = {}
          r.multiline_submatch_upper =
            sub({ 'a1', 'b2', 'c3', 'd4' }, '1s/' .. p .. [[\(1\nb2\nc3\nd\)/[\U\1]/]])
          r.escaped_backslash_and_cr =
            sub({ [[x\y]], 'a\rb' }, '1s/' .. p .. [[x\\y/<&>/]], '2s/' .. p .. [[a\rb/[&]/]])
          return r
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('resolves \\z( \\) captures in syntax regions in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        { 'Z', 'Z', 'Z', 'Z', 'Z', '' },
        n.exec_lua(function(re)
          vim.o.regexpengine = re
          vim.cmd('syntax on | syntax clear')
          vim.api.nvim_buf_set_lines(0, 0, -1, false, { '<a>', 'x', '</b>', 'y', '</a>', 'z' })
          vim.cmd([[syntax region Z start="<\z(\w\+\)>" end="</\z1>"]])
          local names = {}
          for lnum = 1, 6 do
            names[lnum] = vim.fn.synIDattr(vim.fn.synID(lnum, 1, 0), 'name')
          end
          return names
        end, engine)
      )
    end
  end)

  it('applies buffer keywords and case flags on parallel :global and :vimgrep', function()
    eq(
      {
        keyword_dash_global = 45,
        keyword_dash_vimgrep = 45,
        keyword_default_global = 90,
        ignorecase = 90,
        ignorecase_with_C = 45,
        noignorecase_with_c = 90,
      },
      n.exec_lua(function()
        -- More than 8192 lines selects the parallel :global and :vimgrep scan.
        local lines = {}
        for i = 1, 9000 do
          lines[i] = i % 200 == 0 and 'x foo-bar' or i % 200 == 100 and 'x FOO bar' or 'filler'
        end
        vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
        vim.api.nvim_buf_set_name(0, 'Xregexp_parallel')
        local function count(pat)
          vim.g.n = 0
          vim.cmd('silent g/' .. pat .. '/let g:n += 1')
          return vim.g.n
        end
        local r = {}
        vim.cmd('setlocal iskeyword+=-')
        r.keyword_dash_global = count([[\<bar\>]])
        vim.cmd([[silent vimgrep /\<bar\>/j %]])
        r.keyword_dash_vimgrep = #vim.fn.getqflist()
        vim.cmd('setlocal iskeyword-=-')
        r.keyword_default_global = count([[\<bar\>]])
        vim.o.ignorecase = true
        r.ignorecase = count([[\<foo\>]])
        r.ignorecase_with_C = count([[\C\<foo\>]])
        vim.o.ignorecase = false
        r.noignorecase_with_c = count([[\c\<foo\>]])
        return r
      end)
    )
  end)

  it('ends empty loops after zero-width NFA states', function()
    eq(
      {
        eol_star_search = 1,
        bol_group_repeat = { '', '' },
      },
      n.exec_lua(function()
        vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'x' })
        local r = {}
        -- The timeout turns a hang into a failure.
        r.eol_star_search = vim.fn.search([[\%#=2x\_$*]], 'nw', 0, 10000)
        r.bol_group_repeat = vim.list_slice(vim.fn.matchlist('x', [[\%#=2^\(\s*\)\+]]), 1, 2)
        return r
      end)
    )
  end)

  it('finds matches after \\@> in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          one_char_atom = { 'x-0', '' },
          group_before_atom = { 'x-0', 'x' },
          submatch_before_atom = { '', 'b' },
        },
        n.exec_lua(function(p)
          local function ml(str, pat)
            return vim.list_slice(vim.fn.matchlist(str, p .. pat), 1, 2)
          end
          local r = {}
          r.one_char_atom = ml('ax-0', [[.b\=.\@>0]])
          r.group_before_atom = ml('abx-0', [[\(.b\=\).\@>0]])
          r.submatch_before_atom = ml('Ab-', [[\a\=\(\a\)\=.\@>\zs]])
          return r
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('prefers the first matching alternative in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          negative_lookbehind = { '', 0, 0 },
          negative_lookahead = { '', 0, 0 },
        },
        n.exec_lua(function(p)
          local r = {}
          r.negative_lookbehind = vim.fn.matchstrpos('A', p .. [[\%(x\)\@<!\|\i\?]])
          r.negative_lookahead = vim.fn.matchstrpos('A', p .. [[\%(x\)\@!\|\i\?]])
          return r
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('reports NFA submatches only from the path that matched', function()
    eq(
      {
        atomic_alternative = { 'b', '' },
        postponed_lookbehind = { 'abc', 'a', 'b' },
        postponed_lookahead = { 'abc', 'a', 'b', 'c' },
        negative_lookahead = { '', '', '' },
        failed_branch_ze = { 'b' },
      },
      n.exec_lua(function()
        -- Only the NFA engine: the backtracking engine keeps groups set by failed attempts.
        local function ml(str, pat, count)
          return vim.list_slice(vim.fn.matchlist(str, [[\%#=2]] .. pat), 1, count)
        end
        local r = {}
        r.atomic_alternative = ml('ab', [[\(a\)\@>x\|b]], 2)
        r.postponed_lookbehind = ml('abc', [[\(a\)b\(b\)\@<=c]], 3)
        r.postponed_lookahead = ml('abc', [[\(a\)\(b\)\(c\)\@=c]], 4)
        r.negative_lookahead = ml('0', [[\(\(\d\)\@>0\)\@!]], 3)
        r.failed_branch_ze = ml('xb', [[\(x\@>c\|\ze\&a\)\|b]], 1)
        return r
      end)
    )
  end)
end)
