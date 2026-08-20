//! Translated from `src/nvim/dialog.c` (tractable core only).
//!
//! `dialog.c` implements `confirm()`/`:confirm` prompt dialogs - almost
//! entirely dependent on real UI prompting and the `confirm_buttons`/
//! hotkey-parsing machinery (`copy_confirm_hotkeys`, tightly coupled to
//! a real dialog's own file-static output buffer), none of which is
//! translated.
//!
//! Translated: [`copy_char`] and the console-confirm formatting core
//! (`console_dialog_alloc`/`copy_confirm_hotkeys`), including the
//! exported `confirm_msg`/`confirm_buttons` state.
//!
//! Deferred: `msg_show_console_dialog`/`do_dialog`/
//! `vim_dialog_yesno`/`vim_dialog_ynr`, all needing real UI prompting.

use crate::mbyte::{mb_tolower, utf_char2bytes, utf_ptr2char, utfc_ptr2len};

const DLG_BUTTON_SEP: u8 = b'\n';
const DLG_HOTKEY_CHAR: u8 = b'&';
const HAS_HOTKEY_LEN: usize = 30;

/// `:confirm` message (`confirm_msg`).
pub static CONFIRM_MSG: crate::globals::GlobalCell<Option<Vec<u8>>> =
    crate::globals::GlobalCell::new(None);
/// `:confirm` button prompt (`confirm_buttons`).
pub static CONFIRM_BUTTONS: crate::globals::GlobalCell<Option<Vec<u8>>> =
    crate::globals::GlobalCell::new(None);

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

fn next_char_len(bytes: &[u8], offset: usize) -> usize {
    if offset >= bytes.len() {
        return 0;
    }
    usize::try_from(unsafe { utfc_ptr2len(&bytes[offset..]) }.max(1))
        .expect("character length is nonnegative")
        .min(bytes.len() - offset)
}

/// Allocate and initialize console-dialog formatting state
/// (`console_dialog_alloc`).
#[allow(dead_code)] // Used by msg_show_console_dialog once prompting lands.
fn console_dialog_alloc(
    message: &[u8],
    buttons: &[u8],
) -> ([bool; HAS_HOTKEY_LEN], Vec<u8>) {
    let mut has_hotkey = [false; HAS_HOTKEY_LEN];
    let mut index = 0usize;
    let mut offset = 0usize;
    while offset < buttons.len() {
        match buttons[offset] {
            DLG_BUTTON_SEP => {
                if index < HAS_HOTKEY_LEN - 1 {
                    index += 1;
                    has_hotkey[index] = false;
                }
            }
            DLG_HOTKEY_CHAR => {
                offset += 1;
                if index < HAS_HOTKEY_LEN {
                    has_hotkey[index] = true;
                }
                if offset >= buttons.len() {
                    break;
                }
            }
            _ => {}
        }
        offset += next_char_len(buttons, offset);
    }

    let confirm_msg = if crate::ui::ui_has(
        crate::ui::UiExtension::Messages,
    ) {
        message.to_vec()
    } else {
        let mut output = Vec::with_capacity(message.len() + 2);
        output.push(b'\n');
        output.extend_from_slice(message);
        output.push(b'\n');
        output
    };
    unsafe {
        *CONFIRM_MSG.get_mut() = Some(confirm_msg);
        *CONFIRM_BUTTONS.get_mut() = Some(Vec::new());
    }
    (has_hotkey, Vec::new())
}

/// Format confirm buttons and collect one lowercase hotkey per button
/// (`copy_confirm_hotkeys`).
#[allow(dead_code)] // Used by msg_show_console_dialog once prompting lands.
fn copy_confirm_hotkeys(
    buttons: &[u8],
    mut default_button: i32,
    has_hotkey: &[bool; HAS_HOTKEY_LEN],
    mut hotkeys: Vec<u8>,
) -> Vec<u8> {
    hotkeys.clear();
    if !buttons.is_empty() {
        hotkeys.extend(copy_char(buttons, true));
    }
    let mut current_hotkey = 0usize;
    let mut first_hotkey = !has_hotkey[0];
    let mut prompt = Vec::new();
    let mut index = 0usize;
    let mut offset = 0usize;

    while offset < buttons.len() {
        if buttons[offset] == DLG_BUTTON_SEP {
            prompt.extend_from_slice(b", ");
            current_hotkey = hotkeys.len();
            let next = offset + 1;
            if next < buttons.len() {
                hotkeys.extend(copy_char(&buttons[next..], true));
            }
            if default_button != 0 {
                default_button -= 1;
            }
            if index < HAS_HOTKEY_LEN - 1 {
                index += 1;
                if !has_hotkey[index] {
                    first_hotkey = true;
                }
            }
        } else if buttons[offset] == DLG_HOTKEY_CHAR || first_hotkey {
            if buttons[offset] == DLG_HOTKEY_CHAR {
                offset += 1;
                if offset >= buttons.len() {
                    break;
                }
            }
            first_hotkey = false;
            if buttons[offset] == DLG_HOTKEY_CHAR {
                prompt.push(DLG_HOTKEY_CHAR);
            } else {
                prompt.push(if default_button == 1 { b'[' } else { b'(' });
                prompt.extend(copy_char(&buttons[offset..], false));
                prompt.push(if default_button == 1 { b']' } else { b')' });

                hotkeys.truncate(current_hotkey);
                hotkeys.extend(copy_char(&buttons[offset..], true));
            }
        } else {
            prompt.extend(copy_char(&buttons[offset..], false));
        }

        offset += next_char_len(buttons, offset);
    }
    prompt.extend_from_slice(b": ");
    unsafe { *CONFIRM_BUTTONS.get_mut() = Some(prompt) };
    hotkeys
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

    struct ConfirmStateGuard {
        message: Option<Vec<u8>>,
        buttons: Option<Vec<u8>>,
    }

    impl ConfirmStateGuard {
        fn save() -> Self {
            Self {
                message: unsafe { CONFIRM_MSG.get_mut() }.clone(),
                buttons: unsafe { CONFIRM_BUTTONS.get_mut() }.clone(),
            }
        }
    }

    impl Drop for ConfirmStateGuard {
        fn drop(&mut self) {
            unsafe {
                *CONFIRM_MSG.get_mut() = self.message.take();
                *CONFIRM_BUTTONS.get_mut() = self.buttons.take();
            }
        }
    }

    struct CmpFlagsRestore(u32);

    impl Drop for CmpFlagsRestore {
        fn drop(&mut self) {
            restore_cmp_flags(self.0);
        }
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

    #[test]
    fn console_dialog_alloc_records_message_and_explicit_hotkeys() {
        let (_lock, prev) = ascii_lowercase_test_guard();
        let _cmp = CmpFlagsRestore(prev);
        let _state = ConfirmStateGuard::save();
        let (has_hotkey, storage) =
            console_dialog_alloc(b"Save changes?", b"&Yes\n&No");

        assert!(has_hotkey[0]);
        assert!(has_hotkey[1]);
        assert!(!has_hotkey[2]);
        assert!(storage.is_empty());
        assert_eq!(
            unsafe { CONFIRM_MSG.get_mut() }.as_deref(),
            Some(b"\nSave changes?\n".as_slice())
        );
    }

    #[test]
    fn copy_confirm_hotkeys_formats_explicit_button_hotkeys() {
        let (_lock, prev) = ascii_lowercase_test_guard();
        let _cmp = CmpFlagsRestore(prev);
        let _state = ConfirmStateGuard::save();
        let (has_hotkey, storage) =
            console_dialog_alloc(b"Question", b"&Yes\n&No");

        let hotkeys =
            copy_confirm_hotkeys(b"&Yes\n&No", 1, &has_hotkey, storage);

        assert_eq!(hotkeys, b"yn");
        assert_eq!(
            unsafe { CONFIRM_BUTTONS.get_mut() }.as_deref(),
            Some(b"[Y]es, (N)o: ".as_slice())
        );
    }

    #[test]
    fn copy_confirm_hotkeys_uses_first_char_when_no_marker_exists() {
        let (_lock, prev) = ascii_lowercase_test_guard();
        let _cmp = CmpFlagsRestore(prev);
        let _state = ConfirmStateGuard::save();
        let (has_hotkey, storage) =
            console_dialog_alloc(b"Question", b"Yes\nNo");

        let hotkeys =
            copy_confirm_hotkeys(b"Yes\nNo", 2, &has_hotkey, storage);

        assert_eq!(hotkeys, b"yn");
        assert_eq!(
            unsafe { CONFIRM_BUTTONS.get_mut() }.as_deref(),
            Some(b"(Y)es, [N]o: ".as_slice())
        );
    }

    #[test]
    fn doubled_ampersand_is_copied_literally() {
        let (_lock, prev) = ascii_lowercase_test_guard();
        let _cmp = CmpFlagsRestore(prev);
        let _state = ConfirmStateGuard::save();
        let (has_hotkey, storage) = console_dialog_alloc(
            b"Question",
            b"Save && Exit",
        );

        let hotkeys = copy_confirm_hotkeys(
            b"Save && Exit",
            1,
            &has_hotkey,
            storage,
        );

        assert_eq!(hotkeys, b"s");
        assert_eq!(
            unsafe { CONFIRM_BUTTONS.get_mut() }.as_deref(),
            Some(b"Save & Exit: ".as_slice())
        );
    }
}
