-- Lines that run again and again (the lines of a repeating loop and of a
-- function) run from statements parsed once. Each case here runs with that
-- and with NERO_NO_PARSED_LINES set, which turns it off, and has to leave the
-- same exceptions, errors, messages and variables.

local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it = t.describe, t.it
local eq = t.eq
local clear = n.clear
local call = n.call
local exec = n.exec

local harness = [[
  function! W(d, k, z)
    call add(g:log, 'w:' . a:k)
  endfunction

  function! s:Reset() abort
    silent! unlockvar g:a g:b g:s g:l g:log
    let g:a = 1
    let g:b = 2
    let g:s = 'q'
    let g:l = [1, 2]
    let g:log = []
    unlet! g:x g:i g:j g:k g:w g:t g:final g:ret
    for n in range(4)
      execute 'unlet! g:c' . n
    endfor
  endfunction

  function! s:State() abort
    let counters = map(range(4), {_, n -> get(g:, 'c' . n, 'NOC')})
    return string([g:a, g:b, g:s, g:l, g:log, get(g:, 'x', 'NOX'), get(g:, 'i', 'NOI'),
          \ get(g:, 'final', 'NOFINAL'), get(g:, 'ret', 'NORET'), counters])
  endfunction

  function! RunWay(way, file)
    call s:Reset()
    let v:errmsg = ''
    let exc = ''
    let g:msgs = ''
    redir => g:msgs
    try
      if a:way ==# 'src_try'
        execute 'source' fnameescape(a:file)
      elseif a:way ==# 'src_silent'
        execute 'silent! source' fnameescape(a:file)
      elseif a:way ==# 'fn_try'
        let g:ret = CaseF()
      elseif a:way ==# 'fn_abort_try'
        let g:ret = CaseFA()
      else
        silent! let g:ret = CaseF()
      endif
    catch
      let exc = v:exception . ' @ ' . v:throwpoint
    endtry
    redir END
    let text = a:way . ' | ' . exc . ' | ' . v:errmsg . ' | ' . g:msgs . ' | ' . s:State()
    let text = substitute(text, '\V' . escape(a:file, '\'), 'FILE', 'g')
    return substitute(text, '<SNR>\d\+_', '<SNR>N_', 'g')
  endfunction

  function! SetCase(lines, file) abort
    call writefile(a:lines, a:file)
    call writefile(['function! CaseF()'] + a:lines + ['endfunction', 'function! CaseFA() abort']
          \ + a:lines + ['endfunction'], a:file . 'f')
    execute 'source' fnameescape(a:file . 'f')
  endfunction
]]

local ways = { 'src_try', 'src_silent', 'fn_try', 'fn_abort_try', 'fn_silent' }

local loop_cases = {
  { 'for i in range(3)', 'let g:a += i', 'endfor' },
  { 'for i in range(3) | let g:a += i | endfor' },
  { 'let g:a = 0 | while g:a < 3 | let g:a += 1 | endwhile' },
  {
    'let x = 0',
    'while x < 5',
    'let x += 1',
    'if x % 2',
    'continue',
    'endif',
    'let g:s .= x',
    'endwhile',
  },
  { 'for i in range(10)', 'if i == 4', 'break', 'endif', 'let g:a = i', 'endfor' },
  {
    'for i in range(3)',
    'for j in range(3)',
    'if j == i',
    'continue',
    'endif',
    'if j > 1',
    'break',
    'endif',
    'let g:a += i * j',
    'endfor',
    'endfor',
  },
  { 'for [k, v] in [[1, 2], [3, 4]]', 'let g:a += k * v', 'endfor' },
  { 'for c in "abc"', 'let g:s .= c', 'endfor' },
  { 'for b in 0z010203', 'let g:a += b', 'endfor' },
  { 'for i in range(0)', 'let g:a = 99', 'endfor' },
  { 'for i in g:l', 'call add(g:l, i)', 'if len(g:l) > 6', 'break', 'endif', 'endfor' },
  { 'for i in range(3)', 'unlet i', 'endfor' },
  { 'for i in range(3)', 'let g:a = i | let g:b = i * 2', 'endfor' },
  {
    'for i in range(5)',
    'if i == 1',
    'let g:a = 1',
    'elseif i == 2',
    'let g:a = 2',
    'elseif i == 3',
    'let g:a = 3',
    'else',
    'let g:a = 0',
    'endif',
    'endfor',
  },
  { 'let x = 3', 'while x', 'let x -= 1', 'endwhile', 'let g:a = x' },
  { 'let g:a = 0', 'while 1', 'let g:a += 1', 'if g:a >= 5', 'break', 'endif', 'endwhile' },
  { 'for i in range(4)', 'let g:a = i ? i * 2 : -1', "let g:s = i ?? 'none'", 'endfor' },
  {
    'for i in range(3)',
    'let g:a = 1.0 / i',
    'let g:b = 10 / i',
    'let g:c0 = -10 % (i - 1)',
    'endfor',
  },
  { 'for i in range(3)', "let g:s ..= i . 'x'", 'let g:s .= 0x10 + 0b11', 'endfor' },
  {
    "for w in ['ab', 'cd', 'abc']",
    "if w =~ '^ab' && w !~# 'C'",
    'let g:a += 1',
    'endif',
    'endfor',
  },
  {
    'set ignorecase',
    "for w in ['A', 'a']",
    "let g:a += w == 'a'",
    "let g:b += w ==# 'a'",
    'endfor',
    'set noignorecase',
  },
  { 'for i in range(2)', 'let g:a += 1', 'endfor " done' },
  { 'for i in range(2)', 'let g:c0 = 0', 'while g:c0 < 2', 'let g:c0 += 1', 'endwhile', 'endfor' },
  { 'for i in range(3)', 'let g:a = count', 'let g:b = v:true + v:false', 'endfor' },
  { 'for i in range(2)', 'let l:x = i', 'endfor' },
  {
    'for i in range(3)',
    'if i',
    'if i > 1',
    'if i > 2',
    'else',
    'let g:a = i',
    'endif',
    'endif',
    'endif',
    'endfor',
  },
  { 'for i in range(2)', 'let g:l2 =<< END', 'x', 'END', 'endfor' },
  { 'for i in range(3)', 'let g:a = g:nope', 'let g:b += 1', 'endfor' },
  { 'for i in range(3)', 'if g:nope', 'let g:a = 5', 'endif', 'let g:b += 1', 'endfor' },
  { 'while g:nope', 'let g:a = 5', 'endwhile' },
  { 'let x = 0', 'while x < 3', 'let x += 1', 'let g:a = g:l - 1', 'endwhile' },
  { 'for i in range(3)', "let g:s = g:l . 'a'", 'endfor' },
  { 'for i in range(3)', 'let g:a = g:l * g:nope', 'endfor' },
  { 'for i in range(3)', 'if i == 1', 'throw "oops"', 'endif', 'let g:a = i', 'endfor' },
  { 'for i in range(3)', 'try', 'let g:a = g:nope', 'catch', 'let g:b += i', 'endtry', 'endfor' },
  {
    'for i in range(3)',
    'try',
    'if i == 1',
    'continue',
    'endif',
    'let g:a += i',
    'finally',
    'let g:b += 1',
    'endtry',
    'endfor',
  },
  { 'for i in range(3)', 'try', 'break', 'finally', 'let g:b += 1', 'endtry', 'endfor' },
  { 'for i in range(3)', 'silent! let g:a = g:nope', 'let g:b += 1', 'endfor' },
  { 'for i in range(2)', 'execute "let g:a += 1"', 'endfor' },
  { 'lockvar g:a', 'for i in range(2)', 'let g:a = i', 'endfor', 'unlockvar g:a' },
  {
    "call dictwatcheradd(g:, 'a', 'W')",
    'for i in range(3)',
    'let g:a = i',
    'endfor',
    "call dictwatcherdel(g:, 'a', 'W')",
  },
  { 'for i in range(3)', 'let g:a = i', 'endfor', 'for i in range(3)', 'let g:a = i', 'endfor' },
  { 'let g:a = 1 +', 'for i in range(2)', 'let g:b = (1', 'endfor' },
  { 'for i in range(3)', 'endif', 'endfor' },
  { 'for i in range(3)', 'else', 'endfor' },
  { 'if 1', 'for i in range(3)', 'endif', 'endfor' },
  { 'for i in range(3)', 'let g:a = i', 'endwhile' },
  { 'continue' },
  { 'for i in range(3)', 'let g:a +=', 'endfor' },
  { 'let g:final = 0', 'for i in range(3)', 'let g:final += i', 'endfor' },
}

local function_cases = {
  { 'let x = 1', 'if x', 'return x + 1', 'endif', 'return 0' },
  { 'for i in range(5)', 'if i == 3', 'return i', 'endif', 'endfor', 'return -1' },
  { 'try', 'return 1', 'finally', 'let g:a = 9', 'endtry' },
  { 'try', 'return 1', 'finally', 'return 2', 'endtry' },
  { 'let g:a = g:nope', 'return 5' },
  { 'return g:nope' },
  { 'return g:l - 1' },
  { 'let x = 0', 'while x < 3', 'let x += 1', 'return 3', 'endwhile' },
  { 'let x = 1', 'let x .= 2', 'return x' },
  { 'return' },
  { 'if 0', 'return 1', 'endif', 'let g:a = 5' },
  {
    'for i in range(3)',
    'try',
    'return i',
    'finally',
    'continue',
    'endtry',
    'endfor',
    'return -1',
  },
  { 'let x = 1 | return x' },
  { 'return 1 | let g:a = 2' },
  { 'let g:a = 1', 'let g:b = g:a + 1', 'let g:s = g:a . g:b', 'return g:s' },
  { 'let x = 0', 'while x < 3', 'let x += 1', 'endwhile', 'return x' },
  { 'let g:a = a:0', 'let g:b = a:firstline', 'return a:lastline' },
  { 'throw "oops"', 'return 1' },
  { 'if 1', 'let g:a = 2', 'else', 'return 3', 'endif', 'return g:a' },
  { 'let x = 1', 'unlet x', 'return exists("x")' },
  { 'try', 'let g:a = g:nope', 'catch', 'return 7', 'endtry', 'return 8' },
  { 'let g:a = 1', 'return g:a ? "yes" : "no"' },
  { 'let x = 5', 'return -x' },
}

--- Runs each case in a new Nero with `env` and returns what each way left.
--- Each way is an API call of its own: after an error is caught, lines do
--- not run from statements until the outermost try ends, which for an API
--- call is when the call returns.
local function run_cases(cases, env)
  clear({ env = env })
  exec(harness)
  local file = t.tmpname()
  local results = {}
  for i, lines in ipairs(cases) do
    call('SetCase', lines, file)
    results[i] = {}
    for _, way in ipairs(ways) do
      table.insert(results[i], call('RunWay', way, file))
    end
  end
  os.remove(file)
  os.remove(file .. 'f')
  return results
end

local function check(cases)
  local parsed = run_cases(cases, {})
  local plain = run_cases(cases, { NERO_NO_PARSED_LINES = '1' })
  for i, lines in ipairs(cases) do
    eq(plain[i], parsed[i], table.concat(lines, '\n'))
  end
end

describe('lines run from parsed statements', function()
  it('do what do_one_cmd() does in repeating loops', function()
    check(loop_cases)
  end)

  it('do what do_one_cmd() does in functions', function()
    check(function_cases)
  end)
end)
