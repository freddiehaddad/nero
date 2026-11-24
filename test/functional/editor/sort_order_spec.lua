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
end)
