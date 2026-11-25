local n = require('test.functional.testnvim')()
local t = require('test.testutil')

local describe, it = t.describe, t.it

describe('string display width', function()
  local shapes = {
    { name = 'ASCII', text = ('editor text '):rep(64) },
    {
      name = 'mixed graphemes',
      text = ('ae\204\129\230\132\155\240\159\145\169\226\128\141\240\159\140\190'):rep(64),
    },
  }
  for _, shape in ipairs(shapes) do
    for _, api in ipairs({ 'vim.api.nvim_strwidth', 'vim.fn.strwidth' }) do
      it(shape.name .. ' ' .. api, function()
        n.clear()
        n.exec_lua(function(text)
          _G.width_text = text
        end, shape.text)
        n.bench(api .. '(_G.width_text)', {
          n = 20000,
          warmup = 1000,
          label = shape.name .. ' ' .. api,
        })
      end)
    end
  end
end)
