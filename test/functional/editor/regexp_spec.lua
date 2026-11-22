local t = require('test.testutil')
local n = require('test.functional.testnvim')()
local Screen = require('test.functional.ui.screen')

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

  it('reads submatch() from the substituted buffer after a search elsewhere', function()
    eq(
      'x1',
      n.exec_lua(function()
        vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'x1' })
        vim.cmd('new')
        vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'y2' })
        vim.g.other = vim.fn.win_getid()
        vim.cmd('wincmd p')
        vim.cmd([[silent s/x/\=win_execute(g:other, 'call search("y")') .. submatch(0)/]])
        return vim.api.nvim_get_current_line()
      end)
    )
  end)

  it('resolves \\z( \\) captures in syntax regions in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          nested = { 'Z', 'Z', 'Z', 'Z', 'Z', '' },
          failed_atomic = { 'Z', '', 'Z', '' },
        },
        n.exec_lua(function(re)
          vim.o.regexpengine = re
          local function names(lines, region)
            vim.cmd('syntax on | syntax clear')
            vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
            vim.cmd('syntax region Z ' .. region)
            local r = {}
            for lnum = 1, #lines do
              r[lnum] = vim.fn.synIDattr(vim.fn.synID(lnum, 1, 0), 'name')
            end
            return r
          end
          return {
            nested = names(
              { '<a>', 'x', '</b>', 'y', '</a>', 'z' },
              [[start="<\z(\w\+\)>" end="</\z1>"]]
            ),
            -- The first branch captures "a" and fails, so the region has no \z1 and ends at once.
            failed_atomic = names({ 'a', 'b', 'a', 'c' }, [[start="\z(a\)\@>x\|a" end="\z1"]]),
          }
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

  it('finds matches after a \\@> that spans lines in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          blocks = { 6, 1, 9, 3 },
          two_lines = { 1, 2, 2, 2 },
          from_line_end = { 1, 1, 2, 2 },
          to_line_start = { 1, 2, 2, 1 },
          empty_line = { 1, 2, 3, 2 },
        },
        n.exec_lua(function(p)
          -- Where the first match starts and ends.
          local function pos(lines, pat)
            vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
            vim.api.nvim_win_set_cursor(0, { 1, 0 })
            local s = vim.fn.searchpos(p .. pat, 'cW')
            vim.api.nvim_win_set_cursor(0, { 1, 0 })
            local e = vim.fn.searchpos(p .. pat, 'ceW')
            return { s[1], s[2], e[1], e[2] }
          end
          return {
            -- vim/vim#3651: the block without MIDDLE.
            blocks = pos(
              { 'BEGIN', 'some', 'MIDDLE', 'stuff', 'END', 'BEGIN', 'other', 'stuff', 'END' },
              [[\%(BEGIN\_.\{-}\%(MIDDLE\|END\)\)\@>\%(END\)\@<=]]
            ),
            two_lines = pos({ 'xa', 'by' }, [[\(a\nb\)\@>y]]),
            from_line_end = pos({ 'x', 'by' }, [[x\(\nb\)\@>y]]),
            to_line_start = pos({ 'xa', 'b' }, [[\(a\n\)\@>b]]),
            empty_line = pos({ 'xa', '', 'by' }, [[\(a\n\nb\)\@>y]]),
          }
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

  it('applies \\zs and \\ze inside look-arounds and \\@> in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          atomic_ze = { 'a', 0, 1 },
          atomic_zs = { ' ', 2, 3 },
          lookahead_ze = { 'foo', 0, 3 },
          lookahead_zs = { 'bar', 3, 6 },
          lookbehind_ze = { '', 1, 1 },
          lookbehind_zs = { 'ab', 0, 2 },
          buffer_atomic_ze = { 'a', 0 },
        },
        n.exec_lua(function(p)
          local function pos(str, pat)
            return vim.fn.matchstrpos(str, p .. pat)
          end
          local r = {}
          r.atomic_ze = pos('abc', [[\(a\zeb\)\@>c]])
          r.atomic_zs = pos('x0 ', [[\(.\zs\)\@>\s]])
          r.lookahead_ze = pos('foobar', [[\(foo\zebar\)\@=foobar]])
          r.lookahead_zs = pos('foobar', [[\(foo\zsbar\)\@=foobar]])
          r.lookbehind_ze = pos('b ', [[\(b\ze\)\@<= ]])
          r.lookbehind_zs = pos('ab', [[\(\zsa\)\@<=b]])
          vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'abc' })
          local m = vim.fn.matchbufline('', p .. [[\(a\zeb\)\@>c]], 1, 1)[1]
          r.buffer_atomic_ze = { m.text, m.byteidx }
          return r
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('substitutes matches that \\zs or \\ze puts before the search column', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          string_ends_at_column = 'Xb',
          string_starts_before = 'XXX',
          buffer_starts_before = 'XXX',
          buffer_ends_before = 'Xbc',
          -- The second match starts inside the first replacement: it replaces only "c".
          buffer_changes = { '--dXYZ', 3, { { 0, 2, 1 }, { 1, 1, 1 } } },
        },
        n.exec_lua(function(p)
          local api = vim.api
          local function sub_line(line, pat)
            api.nvim_buf_set_lines(0, 0, -1, false, { line })
            vim.cmd('silent s/' .. p .. pat .. '/X/g')
            return api.nvim_get_current_line()
          end
          local r = {}
          r.string_ends_at_column = vim.fn.substitute('ab', p .. [[\(\zsa\)\@<=]], 'X', 'g')
          r.string_starts_before = vim.fn.substitute('abcd', p .. [[\(\zs.\)\@<=.]], 'X', 'g')
          r.buffer_starts_before = sub_line('abcd', [[\(\zs.\)\@<=.]])
          r.buffer_ends_before = sub_line('abc', [[\(\zs.\ze.\)\@<=]])
          api.nvim_buf_set_lines(0, 0, -1, false, { 'abcdXYZ' })
          local ns = api.nvim_create_namespace('')
          local mark = api.nvim_buf_set_extmark(0, ns, 0, 4, {})
          local changes = {}
          api.nvim_buf_attach(0, false, {
            on_bytes = function(_, _, _, _, col, _, _, old_col, _, _, new_col)
              table.insert(changes, { col, old_col, new_col })
            end,
          })
          vim.cmd('silent s/' .. p .. [[\(\zs.\)\@<=[bc]/-/g]])
          r.buffer_changes = {
            api.nvim_get_current_line(),
            api.nvim_buf_get_extmark_by_id(0, ns, mark, {})[2],
            changes,
          }
          return r
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('highlights matches that \\zs or \\ze in a look-behind puts before the search', function()
    local screen = Screen.new(20, 4)
    n.exec_lua(function()
      vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'abc', 'abc', 'abc' })
      vim.fn.setreg('/', [[\(\zs.\ze.\)\@<=]])
      vim.v.hlsearch = 1
    end)
    screen:expect([[
      {10:^ab}c                 |
      {10:ab}c                 |*2
                          |
    ]])
    n.exec_lua(function()
      vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'abcd', 'abcd', 'abcd' })
      vim.o.cpoptions = vim.o.cpoptions:gsub('c', '')
      vim.fn.setreg('/', [[\(\zs.\)\@<=.]])
      vim.v.hlsearch = 1
    end)
    screen:expect([[
      {10:^abcd}                |
      {10:abcd}                |*2
                          |
    ]])
  end)
  it('reports submatches only from the path that matched in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          atomic_alternative = { 'b', '' },
          atomic_then_alternative = { 'aaaa', '' },
          postponed_lookbehind = { 'abc', 'a', 'b' },
          postponed_lookahead = { 'abc', 'a', 'b', 'c' },
          negative_lookahead = { '', '', '' },
          failed_negative_lookahead = { 'ab', 'a', '' },
          failed_lookbehind = { 'b', '', '', 'b' },
          failed_branch_ze = { 'b' },
        },
        n.exec_lua(function(p)
          local function ml(str, pat, count)
            return vim.list_slice(vim.fn.matchlist(str, p .. pat), 1, count)
          end
          local r = {}
          r.atomic_alternative = ml('ab', [[\(a\)\@>x\|b]], 2)
          r.atomic_then_alternative = ml('aaaa', [[\(a*\)\@>a\|a\+]], 2)
          r.postponed_lookbehind = ml('abc', [[\(a\)b\(b\)\@<=c]], 3)
          r.postponed_lookahead = ml('abc', [[\(a\)\(b\)\(c\)\@=c]], 4)
          r.negative_lookahead = ml('0', [[\(\(\d\)\@>0\)\@!]], 3)
          r.failed_negative_lookahead = ml('ab', [[\(\(a\)\@!\|a\)b]], 3)
          r.failed_lookbehind = ml('ab', [[\(x\)\@<=\(b\)\|\(b\)]], 4)
          r.failed_branch_ze = ml('xb', [[\(x\@>c\|\ze\&a\)\|b]], 1)
          return r
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('matches a long grapheme cluster as one character in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        { group_lengths = { 82, 1 }, chars = 2 },
        n.exec_lua(function(p)
          -- "a" with 40 combining acute accents is one character of 81 bytes.
          local s = 'a' .. string.rep('\204\129', 40) .. 'b'
          local m = vim.fn.matchlist(s, p .. [[^.\(.\)]])
          return {
            group_lengths = { #m[1], #m[2] },
            chars = vim.fn.strchars(vim.fn.substitute(s, p .. '.', 'X', 'g')),
          }
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('limits a look-behind to its character count in both engines', function()
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          fits = { 'b', 3, 4 },
          too_long = { '', -1, -1 },
          cluster = { 'x', 3, 4 },
          negative = { '', -1, -1 },
          buf_fits = { 1, 4 },
          buf_too_long = { 0, 0 },
          prev_line = { 2, 2 },
          prev_line_too_long = { 0, 0 },
        },
        n.exec_lua(function(p)
          local function matchpos(s, pat)
            return vim.fn.matchstrpos(s, p .. pat)
          end
          local function searchpos(lines, pat)
            vim.api.nvim_buf_set_lines(0, 0, -1, true, lines)
            vim.api.nvim_win_set_cursor(0, { 1, 0 })
            return vim.fn.searchpos(p .. pat, 'cW')
          end
          -- "e" with a combining acute accent is one character.
          local e_acute = 'e\204\129'
          return {
            fits = matchpos('aéb', [[\(aé\)\@2<=b]]),
            too_long = matchpos('xaéb', [[\(xaé\)\@2<=b]]),
            cluster = matchpos(e_acute .. 'x', [[\(]] .. e_acute .. [[\)\@1<=x]]),
            negative = matchpos('ééb', [[\(éé\)\@2<!b]]),
            buf_fits = searchpos({ 'aéb' }, [[\(aé\)\@2<=b]]),
            buf_too_long = searchpos({ 'xaéb' }, [[\(xaé\)\@2<=b]]),
            -- After crossing a line boundary, the limit counts from the end of the line.
            prev_line = searchpos({ 'éé', 'xb' }, [[\(éé\nx\)\@2<=b]]),
            prev_line_too_long = searchpos({ 'xéé', 'b' }, [[\(xéé\n\)\@2<=b]]),
          }
        end, [[\%#=]] .. engine)
      )
    end
  end)

  it('matches a decomposed character as one character in both engines', function()
    -- "é" written as "e" and a combining acute accent; a combining tilde.
    local e, tilde = 'e\204\129', '\204\131'
    local acute = e:sub(2)
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          -- A collection matches the whole character.
          collection = '[' .. e .. ']x',
          negated_collection = '[' .. e .. ']x',
          collection_ic = '[' .. e .. ']x',
          class_collection = '[' .. e .. '][x]',
          collection_then_x = '[' .. e .. 'x]',
          collection_extra_accent = '[' .. e .. tilde .. ']x',
          -- A match does not end before an accent.
          repeat_before_accent = '[]' .. e .. '[]x[]',
          plus_before_accent = e .. 'x',
          repeat_after_x = '[x]' .. e,
          repeat_ic = '[x]' .. e,
          precomposed_repeat = '[x]é' .. tilde,
          precomposed_optional = '[x]é' .. tilde,
          repeat_then_skip_accents = '[e' .. e .. '][]x[]',
          -- A lone accent matches a character that has it, and matching goes on.
          lone_accent = e .. 'x',
          lone_accent_then_no_x = '',
          lone_accent_groups = { e .. 'x', e, 'x' },
          lone_accents_any_order = 'x' .. acute .. tilde,
          dot_accent = e .. 'x',
          lone_accent_repeat = '[' .. e .. e .. ']x',
          lone_accent_star = '[' .. e .. '][]x[]',
        },
        n.exec_lua(function(p, e_, tilde_)
          local acute_ = e_:sub(2)
          local function sub(s, pat)
            return vim.fn.substitute(s, p .. pat, '[&]', 'g')
          end
          local function str(s, pat)
            return vim.fn.matchstr(s, p .. pat)
          end
          local groups = vim.fn.matchlist(e_ .. 'x', p .. '\\(' .. acute_ .. '\\)\\(x\\)')
          return {
            collection = sub(e_ .. 'x', '[e]'),
            negated_collection = sub(e_ .. 'x', '[^x]'),
            collection_ic = sub(e_ .. 'x', '\\c[E]'),
            class_collection = sub(e_ .. 'x', '[[:alpha:]]'),
            collection_then_x = sub(e_ .. 'x', '[^y]x'),
            collection_extra_accent = sub(e_ .. tilde_ .. 'x', '[' .. e_ .. ']'),
            repeat_before_accent = sub(e_ .. 'x', 'e*'),
            plus_before_accent = sub(e_ .. 'x', 'e\\+'),
            repeat_after_x = sub('x' .. e_, 'xe*'),
            repeat_ic = sub('x' .. e_, '\\cxE*'),
            precomposed_repeat = sub('xé' .. tilde_, 'xé*'),
            precomposed_optional = sub('xé' .. tilde_, 'xé\\='),
            repeat_then_skip_accents = sub('e' .. e_ .. 'x', 'e*\\%C'),
            lone_accent = str(e_ .. 'xy', acute_ .. 'x'),
            lone_accent_then_no_x = str(e_ .. 'y', acute_ .. 'x'),
            lone_accent_groups = vim.list_slice(groups, 1, 3),
            lone_accents_any_order = str(
              'e' .. tilde_ .. ' x' .. acute_ .. tilde_,
              tilde_ .. acute_
            ),
            dot_accent = str('a' .. e_ .. 'x', '.' .. acute_ .. 'x'),
            lone_accent_repeat = sub(e_ .. e_ .. 'x', acute_ .. '\\+'),
            lone_accent_star = sub(e_ .. 'x', acute_ .. '*'),
          }
        end, [[\%#=]] .. engine, e, tilde)
      )
    end
  end)

  it('matches case-folded letters in and after repeats in both engines', function()
    local kelvin = '\226\132\170' -- KELVIN SIGN, which folds to "k"
    for _, engine in ipairs({ 1, 2 }) do
      eq(
        {
          repeat_long_s = 'x[ſſ]x',
          repeat_s = 'x[ss]x',
          repeat_kelvin = 'x[' .. kelvin .. kelvin .. ']x',
        },
        n.exec_lua(function(p, kelvin_)
          local function sub(s, pat)
            return vim.fn.substitute(s, p .. pat, '[&]', 'g')
          end
          return {
            repeat_long_s = sub('xſſx', '\\cs\\+'),
            repeat_s = sub('xssx', '\\cſ\\+'),
            repeat_kelvin = sub('x' .. kelvin_ .. kelvin_ .. 'x', '\\ck\\+'),
          }
        end, [[\%#=]] .. engine, kelvin)
      )
    end
  end)
end)
