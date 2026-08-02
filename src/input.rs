//! Translated from `src/nvim/input.c` (a small, tractable slice).
//!
//! `input.c` (~3600 lines) is the core input engine: reading
//! characters from the user/a script file/the "stuff" (readahead)
//! buffers, applying mappings, assembling `K_SPECIAL`/multibyte byte
//! sequences into keys (`vgetc()`), and recording macros/redo keys.
//! Almost the entire file is one large, mutually-recursive machine
//! (`vgetc`/`vgetorpeek`/the typeahead buffer/mapping application) not
//! yet tractable.
//!
//! Translated: 4 small, self-contained predicate functions, none
//! needing the input engine itself:
//!
//! - [`stuff_empty`]/[`readbuf1_empty`] - whether the "stuff"
//!   (readahead) buffers are empty. Nothing yet writes real content
//!   into `READBUF1`/`READBUF2` (`stuffReadbuff`/`stuffcharReadbuff`/
//!   the `add_buff` family are not yet translated), so both predicates
//!   are always `true` today - a real, faithful reflection of "no
//!   readahead has ever been stuffed," not a hardcoded shortcut: this
//!   crate's own [`crate::input_defs::BuffheaderT`] was already
//!   redesigned (see its own doc comment) to hold a plain
//!   `Vec<BuffblockT>` instead of the original's manually-linked
//!   block list with a dummy first node, so the original's own
//!   `bh_first.b_next == NULL` check translates directly to
//!   `blocks.is_empty()`.
//! - [`using_script`] - whether keys are being read from a script file
//!   (`nvim -s script.txt`). `CURSCRIPT` starts at `-1` and is only
//!   ever changed by `open_scriptin`/`close_all_scripts` (neither
//!   translated), so this is always `false` today - matching a real,
//!   common session too (most sessions never use `-s`).
//! - [`noremap_keys`] - whether the characters obtained by the next
//!   `vgetc()` call cannot be remapped. `KEY_NOREMAP` starts at `0`
//!   and is only ever changed inside `vgetc()` itself (not
//!   translated), so this is always `false` today.
//!
//! Deferred: everything else - `vgetc`/`vgetorpeek` and the whole
//! typeahead-buffer/mapping-application machinery, `stuffReadbuff`/
//! `stuffcharReadbuff`/the `add_buff` family (would give
//! `READBUF1`/`READBUF2` real, observable content, but nothing
//! yet CONSUMES them either, since `vgetc` isn't translated - a good
//! candidate for a dedicated future pass), `ins_typebuf`/the
//! `typebuf`-instance itself (needs a new `TYPEBUF: GlobalCell<
//! crate::input_defs::TypebufT>`, not yet added), `redobuff`/
//! `old_redobuff`/`recordbuff` (the "." and macro-recording buffers),
//! `openscript`/`open_scriptin`/`close_all_scripts` (real script-file
//! I/O).

use crate::globals::GlobalCell;
use crate::input_defs::BuffheaderT;

/// First read ahead buffer. Used for translated commands
/// (`readbuf1`). File-static in the original.
static READBUF1: GlobalCell<BuffheaderT> = GlobalCell::new(BuffheaderT { blocks: Vec::new(), bh_index: 0 });

/// Second read ahead buffer. Used for redo (`readbuf2`). File-static
/// in the original.
static READBUF2: GlobalCell<BuffheaderT> = GlobalCell::new(BuffheaderT { blocks: Vec::new(), bh_index: 0 });

/// Index in `scriptin` (`curscript`). File-static in the original;
/// `-1` means no script is being read.
static CURSCRIPT: GlobalCell<i32> = GlobalCell::new(-1);

/// Remapping flags for the next `vgetc()`-obtained character
/// (`KeyNoremap`). File-static in the original.
static KEY_NOREMAP: GlobalCell<i32> = GlobalCell::new(0);

/// `tb_noremap`: don't remap (`RM_NONE`).
pub const RM_NONE: i32 = 1;
/// `tb_noremap`: remap local script mappings (`RM_SCRIPT`).
pub const RM_SCRIPT: i32 = 2;

/// Whether the stuff buffer is empty (`stuff_empty`).
#[must_use]
pub fn stuff_empty() -> bool {
    // SAFETY: momentary reads, no aliasing (each `get_mut()` call ends
    // before the next begins).
    unsafe { READBUF1.get_mut() }.blocks.is_empty() && unsafe { READBUF2.get_mut() }.blocks.is_empty()
}

/// Whether `readbuf1` is empty. There may still be redo characters in
/// `readbuf2` (`readbuf1_empty`).
#[must_use]
pub fn readbuf1_empty() -> bool {
    // SAFETY: momentary read.
    unsafe { READBUF1.get_mut() }.blocks.is_empty()
}

/// Whether keys are being read from a script file (`using_script`).
#[must_use]
pub fn using_script() -> bool {
    // SAFETY: momentary read.
    *unsafe { CURSCRIPT.get_mut() } >= 0
}

/// Whether keys cannot be remapped (`noremap_keys`).
#[must_use]
pub fn noremap_keys() -> bool {
    // SAFETY: momentary read.
    (unsafe { *KEY_NOREMAP.get_mut() } & (RM_NONE | RM_SCRIPT)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;
    use crate::input_defs::BuffblockT;

    fn reset_buffers() {
        // SAFETY: test-only, serialized via `global_state_test_lock`.
        unsafe {
            *READBUF1.get_mut() = BuffheaderT::default();
            *READBUF2.get_mut() = BuffheaderT::default();
            *CURSCRIPT.get_mut() = -1;
            *KEY_NOREMAP.get_mut() = 0;
        }
    }

    #[test]
    fn stuff_empty_true_when_both_buffers_are_empty() {
        let _lock = global_state_test_lock();
        reset_buffers();
        assert!(stuff_empty());
    }

    #[test]
    fn stuff_empty_false_when_readbuf1_has_a_block() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { READBUF1.get_mut() }.blocks.push(BuffblockT { b_str: b"x".to_vec() });
        assert!(!stuff_empty());
        reset_buffers();
    }

    #[test]
    fn stuff_empty_false_when_only_readbuf2_has_a_block() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { READBUF2.get_mut() }.blocks.push(BuffblockT { b_str: b"y".to_vec() });
        assert!(!stuff_empty());
        reset_buffers();
    }

    #[test]
    fn readbuf1_empty_ignores_readbuf2_content() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { READBUF2.get_mut() }.blocks.push(BuffblockT { b_str: b"z".to_vec() });
        assert!(readbuf1_empty(), "readbuf1 alone is still empty, regardless of readbuf2's content");
        reset_buffers();
    }

    #[test]
    fn readbuf1_empty_false_once_readbuf1_has_a_block() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { READBUF1.get_mut() }.blocks.push(BuffblockT { b_str: b"a".to_vec() });
        assert!(!readbuf1_empty());
        reset_buffers();
    }

    #[test]
    fn using_script_false_by_default() {
        let _lock = global_state_test_lock();
        reset_buffers();
        assert!(!using_script());
    }

    #[test]
    fn using_script_true_when_curscript_is_non_negative() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { *CURSCRIPT.get_mut() = 0 };
        assert!(using_script());
        reset_buffers();
    }

    #[test]
    fn noremap_keys_false_by_default() {
        let _lock = global_state_test_lock();
        reset_buffers();
        assert!(!noremap_keys());
    }

    #[test]
    fn noremap_keys_true_when_rm_none_set() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { *KEY_NOREMAP.get_mut() = RM_NONE };
        assert!(noremap_keys());
        reset_buffers();
    }

    #[test]
    fn noremap_keys_true_when_rm_script_set() {
        let _lock = global_state_test_lock();
        reset_buffers();
        unsafe { *KEY_NOREMAP.get_mut() = RM_SCRIPT };
        assert!(noremap_keys());
        reset_buffers();
    }

    #[test]
    fn noremap_keys_false_for_unrelated_flag_bits() {
        let _lock = global_state_test_lock();
        reset_buffers();
        // RM_ABBR (4) alone, without RM_NONE|RM_SCRIPT, must not count.
        unsafe { *KEY_NOREMAP.get_mut() = 4 };
        assert!(!noremap_keys());
        reset_buffers();
    }
}
