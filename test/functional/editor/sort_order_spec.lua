local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each
local eq = t.eq

-- Rust's sort functions may panic when the comparator is not a total order;
-- upstream's sorts accept any comparator.
describe('sort order', function()
  before_each(n.clear)

  it('of sort() with a compare function that is not a total order is upstream order', function()
    -- glibc's merge sort, which upstream's sort() uses, takes the left item
    -- unless the function returns more than zero.
    eq({ 1, 2, 3, 4, 5, 6, 7, 8, 9 }, n.eval('sort([5, 3, 9, 1, 7, 2, 8, 6, 4], {a, b -> a > b})'))
    -- 3, 10, 17 and 24 compare smaller than everything, from either side, as
    -- NaN does.
    eq(
      { 0, 1, 2, 5, 6, 7, 8, 9, 11, 12, 24, 13, 14, 16, 18, 19, 20, 21, 22, 23, 10, 17, 4, 3, 15 },
      n.eval(
        'sort(map(range(25), {i, v -> v * 37 % 25}), {a, b -> a % 7 == 3 || b % 7 == 3 ? -1 : a - b})'
      )
    )
  end)

  it('of completion matches with "nearest" puts the nearest first', function()
    -- Matches from other buffers have no distance and compare equal to every
    -- match, so the comparator is not a total order.
    n.exec([[
      set hidden complete=.,b completeopt=menuone,noinsert,nearest
      call setline(1, map(range(60), '"foo_b" .. v:val'))
      enew
      call setline(1, map(range(1, 10), '"foo_h" .. v:val'))
      call cursor(5, 1)
    ]])
    n.feed('ofo<C-n>')
    n.poke_eventloop()
    local expected = { 'foo_h6', 'foo_h5', 'foo_h7', 'foo_h4', 'foo_h8' }
    vim.list_extend(expected, { 'foo_h3', 'foo_h9', 'foo_h2', 'foo_h10', 'foo_h1' })
    for i = 0, 59 do
      expected[#expected + 1] = 'foo_b' .. i
    end
    local items = n.fn.complete_info({ 'items' }).items
    eq(
      expected,
      vim.tbl_map(function(item)
        return item.word
      end, items)
    )
  end)
end)
