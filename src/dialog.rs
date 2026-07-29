//! Translated from `src/nvim/dialog.c` (tractable core only).
//!
//! `dialog.c` implements `confirm()`/`:confirm` prompt dialogs - almost
//! entirely dependent on real UI prompting and the `confirm_buttons`/
//! hotkey-parsing machinery (`copy_confirm_hotkeys`, tightly coupled to
//! a real dialog's own file-static output buffer), none of which is
//! translated.
//!
//! Translated: [`copy_char`] - a small, pure, multi-byte-aware
//! character-copying helper needing only already-translated `mbyte.rs`
//! pieces.
//!
//! Deferred: everything else - `copy_confirm_hotkeys` (tightly coupled
//! to the real `confirm_buttons` dialog-text output buffer),
//! `console_dialog_alloc`/`msg_show_console_dialog`/`do_dialog`/
//! `vim_dialog_yesno`/`vim_dialog_ynr`, all needing real UI prompting.

use crate::mbyte::{mb_tolower, utf_char2bytes, utf_ptr2char, utfc_ptr2len};

/// Copies one character from `from`, handling multi-byte characters,
/// optionally lowercasing it (`copy_char`).
///
/// The original writes into a caller-provided `to` buffer and returns
/// the copied length in bytes; this returns the copied character's own
/// bytes directly as an owned `Vec<u8>` instead, matching this crate's
/// established C-out-parameter-to-owned-return convention.
#[must_use]
pub fn copy_char(from: &[u8], lowercase: bool) -> Vec<u8> {
    if lowercase {
        // SAFETY: `mb_tolower` has no preconditions beyond a valid
        // codepoint-or-negative value, which `utf_ptr2char` always
        // returns.
        let c = unsafe { mb_tolower(utf_ptr2char(from)) };
        let mut buf = [0u8; 6]; // enough for any single UTF-8 character
        let len = utf_char2bytes(c, &mut buf);
        buf[..len as usize].to_vec()
    } else {
        // SAFETY: `from` is a plain, valid byte slice.
        let len = unsafe { utfc_ptr2len(from) } as usize;
        from[..len.min(from.len())].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sets `'casemap'` to include `keepascii`, so ASCII lowercasing
    /// goes through the fast, locale-independent `tolower_asc` path
    /// instead of the locale-sensitive `tolower_loc`, which calls the
    /// real libc `tolower()` via FFI - unsupported under Miri (`"can't
    /// call foreign function `tolower`"`). Matches the same workaround
    /// already established in `mbyte.rs`'s own
    /// `mb_toupper_tolower_use_ascii_style_when_keepascii_is_set` test;
    /// produces the identical result for plain ASCII input either way.
    /// Also holds the shared global-state test lock for the whole
    /// duration `OPTION_VARS` matters, matching this crate's own
    /// established convention for any test touching shared globals.
    fn ascii_lowercase_test_guard() -> (std::sync::MutexGuard<'static, ()>, u32) {
        let lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.cmp_flags;
        opts.cmp_flags |= crate::option_vars::opt_cmp_flag::KEEPASCII;
        (lock, prev)
    }

    fn restore_cmp_flags(prev: u32) {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cmp_flags = prev;
    }

    #[test]
    fn copy_char_ascii_uppercase_lowercased() {
        let (_lock, prev) = ascii_lowercase_test_guard();
        assert_eq!(copy_char(b"ABC", true), b"a");
        restore_cmp_flags(prev);
    }

    #[test]
    fn copy_char_ascii_preserves_case_when_not_lowercasing() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(copy_char(b"ABC", false), b"A");
    }

    #[test]
    fn copy_char_only_copies_the_first_character() {
        {
            let _lock = crate::globals::global_state_test_lock();
            assert_eq!(copy_char(b"hello", false), b"h");
        }
        let (_lock, prev) = ascii_lowercase_test_guard();
        assert_eq!(copy_char(b"hello", true), b"h");
        restore_cmp_flags(prev);
    }

    #[test]
    fn copy_char_multibyte_character() {
        let _lock = crate::globals::global_state_test_lock();
        // 'é' (U+00E9) is 2 bytes in UTF-8.
        let input = "éclair".as_bytes();
        assert_eq!(copy_char(input, false), "é".as_bytes());
    }

    #[test]
    // Non-ASCII codepoints always go through mb_tolower's own
    // utf8proc_sys::utf8proc_tolower FFI call, regardless of
    // 'casemap' - unlike the ASCII tests above (fixable via the
    // keepascii workaround), there is no way to route this through a
    // pure-Rust code path. Miri cannot execute compiled foreign
    // functions from a real, vendored C library at all (confirmed this
    // is a genuine PRE-EXISTING gap, not something new here: the exact
    // same "can't call foreign function" error already occurs running
    // Miri directly against mbyte.rs's own pre-existing
    // `mb_toupper_tolower_use_ascii_style_when_keepascii_is_set` test,
    // via its own non-ASCII assertions) - so this one test is skipped
    // under Miri specifically, while still verified by the normal
    // (non-Miri) test suite above.
    #[cfg_attr(miri, ignore)]
    fn copy_char_multibyte_lowercase() {
        let _lock = crate::globals::global_state_test_lock();
        // 'É' (U+00C9) lowercases to 'é' (U+00E9), both 2 bytes in
        // UTF-8.
        let input = "École".as_bytes();
        assert_eq!(copy_char(input, true), "é".as_bytes());
    }
}
