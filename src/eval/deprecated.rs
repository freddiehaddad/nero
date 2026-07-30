//! Translated from `src/nvim/eval/deprecated.c` (tractable core only).
//!
//! `eval/deprecated.c` implements a handful of legacy/deprecated
//! Vimscript builtins that predate their modern replacements:
//! `rpcstart()`/`rpcstop()` (superseded by `jobstart()`/`jobstop()`),
//! `last_buffer_nr()` (superseded by `bufnr('$')`), and `termopen()`
//! (superseded by `jobstart()` with `{term: v:true}`).
//!
//! Only [`f_last_buffer_nr`] is tractable today: `rpcstart()`/
//! `rpcstop()` need `channel_job_start`/`find_job`/`channel_close`
//! (the whole `channel.c`/job-control subsystem, not translated), and
//! `termopen()` needs `f_jobstart` (the same subsystem) - neither is
//! translated.

use crate::eval::typval_defs::TypvalT;

/// `last_buffer_nr()` - the highest buffer number in use
/// (`f_last_buffer_nr`, `eval/deprecated.c`), via a real
/// `GLOBALS.firstbuf`/`BufT.b_next` walk (`FOR_ALL_BUFFERS` in the
/// original).
///
/// # Safety
/// `GLOBALS.firstbuf` must either be null or point to the head of a
/// valid, well-formed `BufT` linked list (every `b_next` pointer
/// either null or pointing at another valid `BufT`).
pub unsafe fn f_last_buffer_nr(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut n: i32 = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let mut buf = unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf;
    while !buf.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let b = unsafe { &*buf };
        if n < b.handle {
            n = b.handle;
        }
        buf = b.b_next;
    }
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(i64::from(n));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;
    use crate::eval::typval_defs::TypvalValue;

    /// RAII guard restoring `GLOBALS.firstbuf` on drop, mirroring this
    /// crate's established `LastbufGuard`/`CurbufGuard` pattern for
    /// tests that need to install a temporary buffer list.
    struct FirstbufGuard {
        previous: *mut BufT,
    }

    impl FirstbufGuard {
        fn set(new_head: *mut BufT) -> Self {
            // SAFETY: caller holds `global_state_test_lock()` for this
            // guard's whole lifetime (checked by every call site).
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let previous = globals.firstbuf;
            globals.firstbuf = new_head;
            Self { previous }
        }
    }

    impl Drop for FirstbufGuard {
        fn drop(&mut self) {
            // SAFETY: forwarded from `set`'s own safety reasoning.
            unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf = self.previous;
        }
    }

    #[test]
    fn last_buffer_nr_is_zero_when_the_buffer_list_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = FirstbufGuard::set(std::ptr::null_mut());

        let mut rettv = TypvalT::default();
        unsafe { f_last_buffer_nr(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn last_buffer_nr_returns_the_highest_handle_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf3 = BufT { handle: 3, b_next: std::ptr::null_mut(), ..Default::default() };
        let mut buf7 = BufT { handle: 7, b_next: &mut buf3 as *mut BufT, ..Default::default() };
        let mut buf1 = BufT { handle: 1, b_next: &mut buf7 as *mut BufT, ..Default::default() };
        let _guard = FirstbufGuard::set(&mut buf1 as *mut BufT);

        // 3 buffers, own numbers deliberately out of list order (1, 7,
        // 3) - the highest, 7, is in the MIDDLE of the list, verifying
        // the whole list is walked, not just the head/tail.
        let mut rettv = TypvalT::default();
        unsafe { f_last_buffer_nr(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(7));
    }

    #[test]
    fn last_buffer_nr_ignores_negative_handles() {
        let _lock = crate::globals::global_state_test_lock();

        // A negative handle (e.g. an unlisted/internal buffer) never
        // raises `n` above its own starting value of 0.
        let mut buf_neg = BufT { handle: -1, b_next: std::ptr::null_mut(), ..Default::default() };
        let _guard = FirstbufGuard::set(&mut buf_neg as *mut BufT);

        let mut rettv = TypvalT::default();
        unsafe { f_last_buffer_nr(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }
}
