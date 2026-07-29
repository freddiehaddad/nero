//! Translated from `src/nvim/indent_c.c` (tractable core only).
//!
//! `indent_c.c` (~3700 lines) implements neovim's C-style indent engine
//! (`cindent`/`get_c_indent`) - almost entirely dependent on the
//! brace/paren/comment-skipping backtracking machinery
//! (`cin_skipcomment`/`cin_nocode`/`find_start_comment`/etc.), none of
//! which is translated.
//!
//! Translated: [`cindent_on`] and [`cin_starts_with`] - both pure
//! functions needing only already-translated option fields and
//! [`crate::charset::vim_isidc`].
//!
//! Deferred: everything else - `check_linecomment` (needs
//! `is_pos_in_string`/`skip_string`), `cin_is_cinword` (needs
//! `vim_iswordc`, the `'iskeyword'`-aware word-character test, not yet
//! translated - a different, more involved function than
//! [`crate::charset::vim_isidc`]), `cin_ends_in`/`cin_is_cpp_extern_c`
//! (need `cin_skipcomment`/`cin_nocode`), and the rest of the real
//! indent-computation algorithm.

use crate::charset::vim_isidc;

/// Whether C-indenting is currently active for the current buffer
/// (`cindent_on`): `'paste'` is off, and either `'cindent'` is set or
/// `'indentexpr'` is non-empty.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (matching `crate::indent::get_indent`'s own safety
/// doc for the same field).
#[must_use]
pub unsafe fn cindent_on() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    let paste = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste;
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    paste == 0 && (curbuf.b_p_cin != 0 || !curbuf.b_p_inde.as_deref().unwrap_or(&[]).is_empty())
}

/// Whether `s` starts with `word` followed by a non-identifier
/// character (or nothing at all) (`cin_starts_with`).
#[must_use]
pub fn cin_starts_with(s: &[u8], word: &[u8]) -> bool {
    s.starts_with(word) && !s.get(word.len()).is_some_and(|&c| vim_isidc(i32::from(c)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    /// RAII guard installing `buf` as `curbuf`, restoring the previous
    /// pointer on drop, and holding `global_state_test_lock` for its
    /// whole lifetime - matching `indent.rs`'s own `CursorTestGuard`
    /// precedent.
    struct CurbufGuard {
        prev_curbuf: *mut BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurbufGuard {
        fn set(buf: *mut BufT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = CurbufGuard { prev_curbuf: globals.curbuf, _lock };
            globals.curbuf = buf;
            guard
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.prev_curbuf;
        }
    }

    fn cindent_on_with(paste: i32, cin: i32, inde: Option<Vec<u8>>) -> bool {
        let mut buf = BufT { b_p_cin: cin, b_p_inde: inde, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf);
        let old_paste = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste = paste;
        let result = unsafe { cindent_on() };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste = old_paste;
        result
    }

    #[test]
    fn cindent_on_true_when_cin_set_and_not_pasting() {
        assert!(cindent_on_with(0, 1, None));
    }

    #[test]
    fn cindent_on_true_when_indentexpr_set_and_not_pasting() {
        assert!(cindent_on_with(0, 0, Some(b"MyIndent()".to_vec())));
    }

    #[test]
    fn cindent_on_false_when_neither_cin_nor_indentexpr_set() {
        assert!(!cindent_on_with(0, 0, None));
    }

    #[test]
    fn cindent_on_false_while_pasting_even_if_cin_set() {
        assert!(!cindent_on_with(1, 1, None));
    }

    #[test]
    fn cin_starts_with_exact_match() {
        assert!(cin_starts_with(b"case", b"case"));
    }

    #[test]
    fn cin_starts_with_followed_by_non_identifier() {
        assert!(cin_starts_with(b"case 1:", b"case"));
        assert!(cin_starts_with(b"enum{", b"enum"));
    }

    #[test]
    fn cin_starts_with_rejects_longer_identifier() {
        // "casement" starts with "case" but is followed by an
        // identifier character, so it's a different word entirely.
        assert!(!cin_starts_with(b"casement", b"case"));
    }

    #[test]
    fn cin_starts_with_rejects_wrong_prefix() {
        assert!(!cin_starts_with(b"default:", b"case"));
    }
}
