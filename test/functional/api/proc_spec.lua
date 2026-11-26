local t = require('test.testutil')
local n = require('test.functional.testnvim')()

local describe, it, before_each, after_each = t.describe, t.it, t.before_each, t.after_each
local clear = n.clear
local eq = t.eq
local fn = n.fn
local neq = t.neq
local nvim_argv = n.nvim_argv
local request = n.request
local retry = t.retry
local NIL = vim.NIL
local is_os = t.is_os

describe('API', function()
  before_each(clear)

  describe('nvim_get_proc_children', function()
    it('returns child process ids', function()
      local this_pid = fn.getpid()

      -- Might be non-zero already (left-over from some other test?),
      -- but this is not what is tested here.
      local initial_children = request('nvim_get_proc_children', this_pid)

      local job1 = fn.jobstart(nvim_argv)
      retry(nil, nil, function()
        eq(#initial_children + 1, #request('nvim_get_proc_children', this_pid))
      end)

      local job2 = fn.jobstart(nvim_argv)
      retry(nil, nil, function()
        eq(#initial_children + 2, #request('nvim_get_proc_children', this_pid))
      end)

      fn.jobstop(job1)
      retry(nil, nil, function()
        eq(#initial_children + 1, #request('nvim_get_proc_children', this_pid))
      end)

      fn.jobstop(job2)
      retry(nil, nil, function()
        eq(#initial_children, #request('nvim_get_proc_children', this_pid))
      end)
    end)

    it('validation', function()
      local status, rv = pcall(request, 'nvim_get_proc_children', -1)
      eq(false, status)
      eq("Invalid 'pid': -1", string.match(rv, 'Invalid.*'))

      status, rv = pcall(request, 'nvim_get_proc_children', 0)
      eq(false, status)
      eq("Invalid 'pid': 0", string.match(rv, 'Invalid.*'))

      -- Assume PID 99999 does not exist.
      status, rv = pcall(request, 'nvim_get_proc_children', 99999)
      eq(true, status)
      eq({}, rv)
    end)
  end)

  describe('nvim_get_proc', function()
    it('returns process info', function()
      local pid = fn.getpid()
      local pinfo = request('nvim_get_proc', pid)
      eq(vim.fs.basename(n.nvim_prog), pinfo.name)
      eq(pid, pinfo.pid)
      eq('number', type(pinfo.ppid))
      neq(pid, pinfo.ppid)
    end)

    it('validation', function()
      local status, rv = pcall(request, 'nvim_get_proc', -1)
      eq(false, status)
      eq("Invalid 'pid': -1", string.match(rv, 'Invalid.*'))

      status, rv = pcall(request, 'nvim_get_proc', 0)
      eq(false, status)
      eq("Invalid 'pid': 0", string.match(rv, 'Invalid.*'))

      -- Assume PID 99999 does not exist.
      status, rv = pcall(request, 'nvim_get_proc', 99999)
      eq(true, status)
      eq(NIL, rv)
    end)
  end)

  if is_os('win') then
    describe('Windows process identity', function()
      local ffi = require('ffi')
      local uv = vim.uv
      ffi.cdef([[
        void *__stdcall OpenProcess(unsigned long access, int inherit, unsigned long pid);
        int __stdcall CloseHandle(void *handle);
        unsigned long __stdcall WaitForSingleObject(void *handle, unsigned long millis);
      ]])
      local kernel = ffi.load('kernel32')
      local server, address, clients, ready, handles, orphan_job

      local function retain(pid)
        local handle = kernel.OpenProcess(0x100000, 0, pid)
        assert(handle ~= nil)
        handles[#handles + 1] = handle
        return handle
      end

      before_each(function()
        clients, ready, handles = {}, {}, {}
        orphan_job = nil
        server = assert(uv.new_tcp())
        assert(server:bind('127.0.0.1', 0))
        address = '127.0.0.1:' .. server:getsockname().port
        assert(server:listen(8, function(err)
          assert(not err, err)
          local client = assert(uv.new_tcp())
          assert(server:accept(client))
          clients[#clients + 1] = client
          local received = ''
          client:read_start(function(_, data)
            if data then
              received = received .. data
              local pid = tonumber(received:match('^(%d+)\n'))
              if pid then
                ready[pid] = true
              end
            end
          end)
        end))
      end)

      after_each(function()
        if orphan_job then
          pcall(fn.chansend, orphan_job, '\n')
        end
        for _, client in ipairs(clients) do
          if not client:is_closing() then
            client:close()
          end
        end
        server:close()
        local ok, err = pcall(retry, nil, 10000, function()
          uv.run('nowait')
          for _, handle in ipairs(handles) do
            eq(0, tonumber(kernel.WaitForSingleObject(handle, 0)))
          end
        end)
        for _, handle in ipairs(handles) do
          eq(1, tonumber(kernel.CloseHandle(handle)))
        end
        assert(ok, err)
      end)

      it('does not report children of an exited parent', function()
        local parent = n.exec_lua(function(argv)
          _G.orphan_output = ''
          _G.orphan_exit = nil
          local job = vim.fn.jobstart(argv, {
            on_stdout = function(_, data)
              _G.orphan_output = _G.orphan_output .. table.concat(data, '\n')
            end,
            on_exit = function(_, status)
              _G.orphan_exit = status
            end,
          })
          assert(job > 0)
          return { job, vim.fn.jobpid(job) }
        end, { n.testprg('shell-test'), 'ORPHAN', address })
        orphan_job = parent[1]
        -- Retaining handles prevents either PID from being reused during the assertions.
        retain(parent[2])
        local child = retry(nil, 10000, function()
          uv.run('nowait')
          local pid = tonumber(n.exec_lua('return _G.orphan_output'):match('^(%d+)\n'))
          assert(pid and ready[pid])
          return pid
        end)
        local child_handle = retain(child)
        eq(parent[2], request('nvim_get_proc', child).ppid)

        fn.chansend(orphan_job, '\n')
        retry(nil, 10000, function()
          eq(0, n.exec_lua('return _G.orphan_exit'))
        end)
        eq(NIL, request('nvim_get_proc', parent[2]))
        eq(258, tonumber(kernel.WaitForSingleObject(child_handle, 0)))
        eq({}, request('nvim_get_proc_children', parent[2]))
      end)

      it('stops an owned tree without stopping a sibling', function()
        local tree = fn.jobstart({ n.testprg('shell-test'), 'PROCESS_TREE', address, '2' })
        local sibling = fn.jobstart({ n.testprg('shell-test'), 'PROCESS_TREE', address, '0' })
        assert(tree > 0 and sibling > 0)
        local root_pid = fn.jobpid(tree)
        local sibling_pid = fn.jobpid(sibling)
        retry(nil, 10000, function()
          uv.run('nowait')
          eq(4, vim.tbl_count(ready))
        end)
        local tree_handles = {}
        local sibling_handle
        for pid in pairs(ready) do
          local handle = retain(pid)
          if pid == sibling_pid then
            sibling_handle = handle
          else
            tree_handles[#tree_handles + 1] = handle
          end
        end
        assert(ready[root_pid] and sibling_handle)
        eq(3, #tree_handles)
        eq(1, fn.jobstop(tree))
        retry(nil, 10000, function()
          for _, handle in ipairs(tree_handles) do
            eq(0, tonumber(kernel.WaitForSingleObject(handle, 0)))
          end
        end)
        eq(258, tonumber(kernel.WaitForSingleObject(sibling_handle, 0)))
      end)
    end)
  end
end)
