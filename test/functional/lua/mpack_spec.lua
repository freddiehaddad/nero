-- Test suite for testing interactions with API bindings
local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each = t.describe, t.it, t.before_each
local clear = n.clear
local eq = t.eq
local exec_lua = n.exec_lua

describe('lua vim.mpack', function()
  before_each(clear)
  it('encodes vim.NIL', function()
    eq(
      { true, true, true, true },
      exec_lua(function()
        local var = vim.mpack.decode(vim.mpack.encode({ 33, vim.NIL, 77 }))
        return { var[1] == 33, var[2] == vim.NIL, var[3] == 77, var[4] == nil }
      end)
    )
  end)

  it('encodes vim.empty_dict()', function()
    eq(
      { { {}, 'foo', {} }, true, false },
      exec_lua(function()
        local var = vim.mpack.decode(vim.mpack.encode({ {}, 'foo', vim.empty_dict() }))
        return { var, vim.islist(var[1]), vim.islist(var[3]) }
      end)
    )
  end)

  it('encodes negative integers at type boundaries correctly #37202', function()
    -- Test boundary values between int8/int16/int32
    -- int8 range: -128 to -33 (fixint handles -32 to -1)
    -- int16 range: -32768 to -129
    -- int32 range: -2147483648 to -32769
    local result = exec_lua(function()
      local tests = {
        { -128, -128 }, -- int8 boundary (minimum int8)
        { -129, -129 }, -- int16 boundary (one past int8)
        { -32768, -32768 }, -- int16 boundary (minimum int16)
        { -32769, -32769 }, -- int32 boundary (one past int16)
      }
      local results = {}
      for _, test in ipairs(tests) do
        local input, expected = test[1], test[2]
        local decoded = vim.mpack.decode(vim.mpack.encode(input))
        table.insert(results, { input = input, decoded = decoded, ok = decoded == expected })
      end
      return results
    end)
    for _, r in ipairs(result) do
      eq(true, r.ok, string.format('encode/decode %d returned %d', r.input, r.decoded))
    end
  end)

  it('encodes negative integers below -2^32 correctly', function()
    eq(
      {},
      exec_lua(function()
        local failures = {}
        for _, v in ipairs({ -(2 ^ 32 + 5), -(3 * 2 ^ 32 + 70000), -(2 ^ 53 - 1) }) do
          local decoded = vim.mpack.decode(vim.mpack.encode(v))
          if decoded ~= v then
            table.insert(failures, ('%.17g -> %.17g'):format(v, decoded))
          end
        end
        return failures
      end)
    )
  end)

  it('encodes numbers beyond 2^53 #32216', function()
    eq(
      {},
      exec_lua(function()
        local failures = {}
        local tests = {
          2 ^ 53,
          -2 ^ 53,
          2 ^ 64 - 2048, -- largest uint64 double
          -2 ^ 63, -- smallest int64
          -2 ^ 63 - 2048, -- below int64, encoded as float
          -1.5 * 2 ^ 63,
          2 ^ 64, -- above uint64, encoded as float
          1e300,
          math.huge,
          -math.huge,
        }
        for _, v in ipairs(tests) do
          local decoded = vim.mpack.decode(vim.mpack.encode(v))
          if decoded ~= v then
            table.insert(failures, ('%.17g -> %.17g'):format(v, decoded))
          end
        end
        local nan = vim.mpack.decode(vim.mpack.encode(0 / 0))
        if nan == nan then
          table.insert(failures, ('nan -> %.17g'):format(nan))
        end
        return failures
      end)
    )
  end)

  it('encodes dict keys of length 20-31 as fixstr #32784', function()
    -- MessagePack fixstr format: 0xa0 | length (for lengths 0-31)
    -- Before #36737, strings 20-31 bytes were incorrectly encoded as str8 (0xd9, len)
    for len = 20, 31 do
      local expected_header = string.char(0xa0 + len) -- fixstr header
      local result = exec_lua(function(keylen)
        local key = string.rep('x', keylen)
        return vim.mpack.encode({ [key] = 1 })
      end, len)
      -- Byte 1 is fixmap header (0x81), byte 2 should be fixstr header for the key
      eq(expected_header, result:sub(2, 2), 'dict key length ' .. len .. ' should use fixstr')
    end
  end)

  it('round-trips strings larger than the packer buffer', function()
    eq(
      { true, true, true },
      exec_lua(function()
        local s = ('0123456789abcdef'):rep(256 * 1024) .. 'tail'
        local packed = vim.mpack.encode({ s, 1, s })
        local decoded = vim.mpack.decode(packed)
        local packer_out = vim.mpack.Packer()({ s, 1, s })
        return { decoded[1] == s and decoded[3] == s, decoded[2] == 1, packer_out == packed }
      end)
    )
  end)

  it('decodes fragmented binary and extension values', function()
    eq(
      true,
      exec_lua(function()
        local payload = 'a\000\255b'
        local wire = string.char(0x92, 0xc4, #payload)
          .. payload
          .. string.char(0xc7, 3, 42)
          .. 'xyz'
        for split = 1, #wire - 1 do
          local unpack = vim.mpack.Unpacker({
            ext = {
              [42] = function(tag, bytes)
                return { tag, vim.mpack.decode(vim.mpack.encode(bytes)) }
              end,
            },
          })
          local partial, next_pos = unpack(wire:sub(1, split))
          assert(partial == nil and next_pos == split + 1)
          local value, final_pos = unpack(wire:sub(split + 1))
          assert(final_pos == #wire - split + 1)
          assert(value[1] == payload and value[2][1] == 42 and value[2][2] == 'xyz')
        end
        return true
      end)
    )
  end)

  it('preserves streamed tokens when the unpacker grows', function()
    eq(
      true,
      exec_lua(function()
        local payload = 'a\000\255b'
        local values = {
          string.char(0xdb, 0, 0, 0, #payload) .. payload,
          string.char(0xc7, #payload, 42) .. payload,
        }
        for _, depth in ipairs({ 31, 32, 63, 64 }) do
          for _, encoded in ipairs(values) do
            local wire = string.char(0x91):rep(depth) .. encoded
            for split = 1, #wire do
              local unpack = vim.mpack.Unpacker()
              local value, pos = unpack(wire:sub(1, split))
              if split < #wire then
                assert(value == nil and pos == split + 1)
                value, pos = unpack(wire:sub(split + 1))
                assert(pos == #wire - split + 1)
              else
                assert(pos == #wire + 1)
              end
              for _ = 1, depth do
                value = value[1]
              end
              assert(value == payload)
            end
          end
        end
        return true
      end)
    )
  end)
end)
